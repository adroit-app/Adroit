//! The sessions and terminals there are, placed by what git says of where
//! each runs: those the terminal host runs for adroit and those running in
//! other terminals, listed every two seconds or as soon as the window asks.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use adroit_model::{Session, TerminalSession, Worktree, build};
use adroit_protocol::{Listed, TreeChange};
use adroit_term::Daemon;

use super::Watched;
use crate::sessions::{self, Kept};

/// How long what git said about a worktree is trusted before asking again.
const WORKTREE_TTL: Duration = Duration::from_secs(15);

pub(super) async fn watch(mut watched: Watched) {
    let daemon = crate::daemon::daemon();
    // What git said of each directory a session runs in, and of each
    // repository's worktrees, with when it said it.
    let mut cache: HashMap<PathBuf, (Instant, Option<Worktree>)> = HashMap::new();
    let mut lists: HashMap<PathBuf, (Instant, Vec<Worktree>)> = HashMap::new();
    loop {
        match read_sessions(&daemon, &watched.inputs()).await {
            Ok((sessions, terminals)) => {
                let every = watched.sight().tree.every_worktree;
                let found = inspect(&sessions, &terminals, &mut cache).await;
                let listed = if every {
                    worktrees(&found, &mut lists).await
                } else {
                    Vec::new()
                };
                let scratch = crate::paths::scratch_dir();
                let tree = build(&sessions, &terminals, &found, &listed, Some(&scratch));
                watched.record([
                    TreeChange::Failing { error: None },
                    TreeChange::Listed {
                        listed: Box::new(Listed {
                            sessions,
                            terminals,
                            tree,
                        }),
                    },
                ]);
            }
            Err(error) => watched.record([TreeChange::Failing { error: Some(error) }]),
        }
        let asked = watched
            .wait_listing(Duration::from_secs(2), |s| s.sight.tree.every_worktree)
            .await;
        // Asked for by hand: look at every worktree again too.
        if asked {
            cache.clear();
            lists.clear();
        }
    }
}

/// What git says of each directory a session or a terminal runs in, asked
/// again of those it said nothing of lately.
async fn inspect(
    sessions: &[Session],
    terminals: &[TerminalSession],
    cache: &mut HashMap<PathBuf, (Instant, Option<Worktree>)>,
) -> HashMap<PathBuf, Option<Worktree>> {
    let dirs = || {
        sessions
            .iter()
            .map(|s| &s.cwd)
            .chain(terminals.iter().map(|s| &s.cwd))
    };
    // Asked of git all at once: one by one, every directory would hold the
    // list back by its calls.
    let mut unread: Vec<PathBuf> = dirs()
        .filter(|d| {
            cache
                .get(*d)
                .is_none_or(|(at, _)| at.elapsed() > WORKTREE_TTL)
        })
        .cloned()
        .collect();
    unread.sort();
    unread.dedup();
    let mut asking = tokio::task::JoinSet::new();
    for dir in unread {
        asking.spawn(async move {
            let w = adroit_vcs::inspect(&dir).await;
            (dir, w)
        });
    }
    while let Some(answered) = asking.join_next().await {
        if let Ok((dir, w)) = answered {
            cache.insert(dir, (Instant::now(), w));
        }
    }
    dirs()
        .filter_map(|d| cache.get(d).map(|(_, w)| (d.clone(), w.clone())))
        .collect()
}

/// Every worktree git knows of each repository `found` names, those where
/// nothing runs too.
async fn worktrees(
    found: &HashMap<PathBuf, Option<Worktree>>,
    lists: &mut HashMap<PathBuf, (Instant, Vec<Worktree>)>,
) -> Vec<Worktree> {
    let mut repos: Vec<PathBuf> = found.values().flatten().map(|w| w.repo.clone()).collect();
    repos.sort();
    repos.dedup();
    // Asked all at once, as the directories are.
    let mut asking = tokio::task::JoinSet::new();
    for repo in repos.iter().filter(|r| {
        lists
            .get(*r)
            .is_none_or(|(at, _)| at.elapsed() > WORKTREE_TTL)
    }) {
        let repo = repo.clone();
        asking.spawn(async move {
            let ws = adroit_vcs::git::worktrees(&repo).await.unwrap_or_default();
            (repo, ws)
        });
    }
    while let Some(answered) = asking.join_next().await {
        if let Ok((repo, ws)) = answered {
            lists.insert(repo, (Instant::now(), ws));
        }
    }
    repos
        .iter()
        .filter_map(|repo| lists.get(repo))
        .flat_map(|(_, ws)| ws.iter().cloned())
        .collect()
}

/// Every session to list: those the daemon runs for adroit, with the terminals,
/// and those Claude Code lists in other terminals, but for adroit's own,
/// which Claude Code lists too.
pub async fn read_sessions(
    daemon: &Daemon,
    inputs: &BTreeMap<u128, u64>,
) -> Result<(Vec<Session>, Vec<TerminalSession>), String> {
    let kept = daemon.sessions().await.map_err(|e| e.to_string())?;
    let (mut own, mut kept_shells) = (Vec::new(), Vec::new());
    let mut learned = Vec::new();
    for s in &kept {
        match sessions::read(s) {
            Some(Kept::Agent(mut c)) => {
                if let Some(at) = inputs.get(&s.id)
                    && let Some(tag) = sessions::note_input(&mut c, *at)
                {
                    learned.push(tag);
                }
                let adapter = adroit_agents::adapter(c.agent);
                if let Some(title) = &s.title
                    && let Some(tag) = sessions::follow_title(&mut c, title)
                {
                    learned.push(tag);
                }
                if s.status.is_none()
                    && adapter.capabilities().screen_state
                    && let Ok(Ok(screen)) =
                        tokio::time::timeout(Duration::from_millis(300), daemon.visible(s.id)).await
                    && let Some(state) = adapter.screen_state(&screen)
                {
                    c.state = state;
                }
                own.push(c);
            }
            Some(Kept::Terminal(sh)) => kept_shells.push(sh),
            None => {}
        }
    }
    // Without Claude Code, or with a listing it cannot give, the sessions in
    // other terminals are not listed; adroit's own still are.
    let mut elsewhere = Vec::new();
    for agent in adroit_model::Agent::ALL {
        let adapter = adroit_agents::adapter(agent);
        let listed = adapter
            .list_sessions(crate::programs::agents())
            .await
            .unwrap_or_default();
        let (external, tags) = adapter.follow(&mut own, listed);
        elsewhere.extend(external);
        learned.extend(tags.into_iter().map(sessions::learned_tag));
    }
    // Kept on the session, so a window opened later follows it too; one not
    // kept is learned again on the next read.
    for (id, key, value) in learned {
        let held = kept
            .iter()
            .find(|s| s.id == id)
            .map(|s| adroit_term::tags::holding(&s.tags, key))
            .unwrap_or_default();
        let _ = daemon
            .tag(id, adroit_term::tags::tagged(key, &value), held)
            .await;
    }
    own.extend(elsewhere);
    Ok((own, kept_shells))
}
