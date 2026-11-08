//! The sessions and terminals there are, placed by what git says of where
//! each runs, listed every two seconds or as soon as a window asks.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use adroit_model::{Session, State, TerminalSession, Worktree, build};
use adroit_protocol::{Listed, TreeChange};
use adroit_term::Daemon;

use super::Watched;
use crate::sessions::{self, Kept};
use crate::work::Seen;

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
                let found = inspect(&sessions, &terminals, &mut cache).await;
                let scratch = crate::paths::scratch_dir();
                let listing = |listed: &[Worktree]| TreeChange::Listed {
                    listed: Box::new(Listed {
                        tree: build(&sessions, &terminals, &found, listed, Some(&scratch)),
                        sessions: sessions.clone(),
                        terminals: terminals.clone(),
                    }),
                };
                watched.record([TreeChange::Failing { error: None }]);
                // Each window lists every worktree git knows, or only those
                // where something runs, as it asks.
                watched.record_to([listing(&[])], |s| !s.tree.every_worktree);
                if every_worktree(&watched.seen()) {
                    let listed = worktrees(&found, &mut lists).await;
                    watched.record_to([listing(&listed)], |s| s.tree.every_worktree);
                }
            }
            Err(error) => watched.record([TreeChange::Failing { error: Some(error) }]),
        }
        let asked = watched
            .wait_listing(Duration::from_secs(2), every_worktree)
            .await;
        // Asked for by hand: look at every worktree again too.
        if asked {
            cache.clear();
            lists.clear();
        }
    }
}

/// Whether a window lists every worktree git knows.
fn every_worktree(seen: &[Seen]) -> bool {
    seen.iter().any(|s| s.sight.tree.every_worktree)
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

/// Every session to list, those the daemon runs for adroit, with the
/// terminals.
pub async fn read_sessions(
    daemon: &Daemon,
    inputs: &BTreeMap<u128, u64>,
) -> Result<(Vec<Session>, Vec<TerminalSession>), String> {
    let kept = daemon.sessions().await.map_err(|e| e.to_string())?;
    let (mut own, mut kept_shells) = (Vec::new(), Vec::new());
    let mut learned = Vec::new();
    for s in kept.iter().filter(|s| crate::environment::owns(&s.tags)) {
        match sessions::read(s) {
            Some(Kept::Agent(mut c)) => {
                if sessions::earlier_connection(s).is_some()
                    && let Some(record) = c.delegation
                    && let Ok(Some(Some(home))) =
                        crate::delegation::app::home_of(&crate::delegation::app::root(), record)
                            .await
                {
                    // Named on the session from now on, as its record has it.
                    learned.push((s.id, sessions::HOME, home.to_string_lossy().into_owned()));
                    c.home = Some(home);
                }
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
                let reported = s.status.is_some();
                let screen = match (!reported || c.state == State::Working)
                    && adapter.capabilities().screen_state
                {
                    true => tokio::time::timeout(Duration::from_millis(300), daemon.visible(s.id))
                        .await
                        .ok()
                        .and_then(Result::ok),
                    false => None,
                };
                let state = match reported {
                    true => {
                        super::hold::settled(s.id, None);
                        screen
                            .as_ref()
                            .and_then(|screen| past_working(&c.state, adapter, screen))
                    }
                    false => super::hold::settled(
                        s.id,
                        screen.map(|screen| adapter.judge_state(&screen)).as_ref(),
                    ),
                };
                if let Some(state) = state {
                    c.state = state;
                    // What the program said went with the report set aside.
                    c.status_message = None;
                }
                // Named as the settings it started with call its profile,
                // which the settings now may hold no more.
                if let Ok(context) = crate::programs::of_session(&c).await {
                    c.profile_name = context.profile_name;
                }
                own.push(c);
            }
            Some(Kept::Terminal(sh)) => kept_shells.push(sh),
            None => {}
        }
    }
    super::hold::keep_only(&kept.iter().map(|s| s.id).collect::<Vec<_>>());
    // Without Claude Code, or with a listing it cannot give, nothing is
    // learned; the sessions are listed all the same.
    // Each login lists the sessions running in it alone, so each is asked as
    // a session of it was started and read against those.
    for (agent, home, context) in
        logins_of(
            &own,
            |s| async move { crate::programs::of_session(&s).await },
        )
        .await
    {
        let adapter = adroit_agents::adapter(agent);
        let listed = adapter.list_sessions(context).await.unwrap_or_default();
        let ours = |s: &Session| s.agent == agent && s.home == home;
        let mut these: Vec<Session> = own.iter().filter(|s| ours(s)).cloned().collect();
        let tags = adapter.follow(&mut these, listed);
        learned.extend(tags.into_iter().map(sessions::learned_tag));
        let mut followed = these.into_iter();
        for s in own.iter_mut().filter(|s| ours(s)) {
            if let Some(next) = followed.next() {
                *s = next;
            }
        }
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
    Ok((own, kept_shells))
}

/// How a session stands whose program reports `reported`, when `adapter`
/// reads on its `screen` a request that waits on the user while the report
/// says it works: a report of working that the program never followed up
/// would hide the request. A dialog the user opened, such as a picker, asks
/// nothing of them and leaves the report standing.
fn past_working(
    reported: &State,
    adapter: &dyn adroit_agents::AgentAdapter,
    screen: &[String],
) -> Option<State> {
    if *reported != State::Working {
        return None;
    }
    adapter
        .screen_state(screen)
        .filter(|seen| matches!(seen, State::Waiting(_)))
}

/// Each agent and login `own` has sessions in, with the context the
/// first of those sessions `of` tells one for runs in, tried in turn; one whose sessions
/// tell none is left out.
async fn logins_of<F, Fut>(
    own: &[Session],
    of: F,
) -> Vec<(
    adroit_model::Agent,
    Option<PathBuf>,
    adroit_agents::AgentContext,
)>
where
    F: Fn(Session) -> Fut,
    Fut: std::future::Future<Output = Result<adroit_agents::AgentContext, String>>,
{
    let mut out: Vec<(
        adroit_model::Agent,
        Option<PathBuf>,
        adroit_agents::AgentContext,
    )> = Vec::new();
    for s in own {
        if out
            .iter()
            .any(|(agent, home, _)| *agent == s.agent && *home == s.home)
        {
            continue;
        }
        if let Ok(context) = of(s.clone()).await {
            out.push((s.agent, s.home.clone(), context));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use adroit_model::Agent;
    use adroit_model::testing::session;

    use super::*;

    fn screen(name: &str) -> Vec<String> {
        let path = format!(
            "{}/../agents/tests/screens/{name}.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn a_request_on_screen_wins_over_a_report_of_working() {
        let on = |agent: Agent, name: &str| {
            past_working(
                &State::Working,
                adroit_agents::adapter(agent),
                &screen(name),
            )
        };
        assert!(
            matches!(
                on(Agent::OpenCode, "opencode-permission"),
                Some(State::Waiting(_))
            ),
            "{:?}",
            on(Agent::OpenCode, "opencode-permission")
        );
        assert_eq!(on(Agent::OpenCode, "opencode-working"), None);
        assert_eq!(on(Agent::Claude, "working"), None);
    }

    #[test]
    fn a_picker_the_user_opened_while_the_agent_works_leaves_it_working() {
        let claude = adroit_agents::adapter(Agent::Claude);
        assert_eq!(
            past_working(&State::Working, claude, &screen("model-while-working")),
            None
        );
    }

    #[test]
    fn a_report_of_anything_but_working_is_not_read_against_the_screen() {
        let opencode = adroit_agents::adapter(Agent::OpenCode);
        for reported in [State::Idle, State::Done, State::Failed] {
            assert_eq!(
                past_working(&reported, opencode, &screen("opencode-permission")),
                None
            );
        }
    }

    #[tokio::test]
    async fn a_login_is_asked_through_the_first_of_its_sessions_whose_context_is_known() {
        let own = [
            session("lost").profile("claude-work", Some("/w")).build(),
            session("kept").profile("claude-work", Some("/w")).build(),
            session("other").profile("claude-work", Some("/w")).build(),
            session("mine").build(),
        ];
        let of = |s: Session| async move {
            match s.name != "lost" {
                true => Ok(adroit_agents::AgentContext {
                    shell: s.name,
                    ..adroit_agents::AgentContext::builtin(Agent::Claude)
                }),
                false => Err("no record".to_string()),
            }
        };

        let asked: Vec<(Option<PathBuf>, String)> = logins_of(&own, of)
            .await
            .into_iter()
            .map(|(_, home, context)| (home, context.shell))
            .collect();

        assert_eq!(
            asked,
            [(Some("/w".into()), "kept".into()), (None, "mine".into())]
        );
    }
}
