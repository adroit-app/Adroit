//! The sessions and terminals there are, placed by what git says of where
//! each runs, listed every two seconds or as soon as a window asks.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use adroit_model::{Session, State, TerminalSession, Tree, Worktree, build};
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
    let mut ran = Ran::default();
    loop {
        match read_sessions(&daemon, &watched.inputs()).await {
            Ok((sessions, terminals)) => {
                let kept = kept_workspaces();
                let found = inspect(&sessions, &terminals, &kept, &mut cache).await;
                let scratch = crate::paths::scratch_dir();
                let tree = |listed: &[Worktree]| {
                    build(&sessions, &terminals, &found, listed, &kept, Some(&scratch))
                };
                let started = ran.started(&tree(&[]), &sessions, &terminals);
                if !started.is_empty()
                    && let Err(error) =
                        crate::workspaces::keep(&crate::paths::instance_state(), started)
                {
                    eprintln!("adroit: cannot keep the workspaces sessions started in: {error}");
                }
                // Listed all the same when the store cannot be read: what waits
                // is only offered.
                let interrupted = crate::restore::waiting(&daemon).await.unwrap_or_default();
                let listing = |listed: &[Worktree]| TreeChange::Listed {
                    listed: Box::new(Listed {
                        tree: tree(listed),
                        sessions: sessions.clone(),
                        terminals: terminals.clone(),
                        interrupted: interrupted.clone(),
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

/// The workspaces the tree keeps that are still on disk; none while they
/// cannot be read.
fn kept_workspaces() -> Vec<PathBuf> {
    match crate::workspaces::load(&crate::paths::instance_state()) {
        Ok(kept) => kept.into_iter().filter(|dir| dir.is_dir()).collect(),
        Err(error) => {
            eprintln!("adroit: {error}");
            Vec::new()
        }
    }
}

/// The sessions and terminals already seen running, by their ids.
#[derive(Debug, Default)]
struct Ran {
    sessions: HashSet<String>,
    terminals: HashSet<u128>,
}

impl Ran {
    /// The workspaces in `tree` that a session or terminal not seen running
    /// before runs in, to be kept once it ends. One seen before is passed
    /// over, so a workspace taken out of the tree while it ran stays out.
    fn started(
        &mut self,
        tree: &Tree,
        sessions: &[Session],
        terminals: &[TerminalSession],
    ) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = sessions
            .iter()
            .filter(|s| !self.sessions.contains(&s.id))
            .filter_map(|s| tree.find(&s.id).map(|(ws, _, _)| ws.root.clone()))
            .chain(
                terminals
                    .iter()
                    .filter(|t| !self.terminals.contains(&t.id))
                    .filter_map(|t| tree.terminal(t.id).map(|(ws, _, _)| ws.root.clone())),
            )
            .collect();
        roots.sort();
        roots.dedup();
        self.sessions = sessions.iter().map(|s| s.id.clone()).collect();
        self.terminals = terminals.iter().map(|t| t.id).collect();
        roots
    }
}

/// What git says of each directory a session or a terminal runs in, and of
/// each workspace kept, asked again of those it said nothing of lately.
async fn inspect(
    sessions: &[Session],
    terminals: &[TerminalSession],
    kept: &[PathBuf],
    cache: &mut HashMap<PathBuf, (Instant, Option<Worktree>)>,
) -> HashMap<PathBuf, Option<Worktree>> {
    let dirs = || {
        sessions
            .iter()
            .map(|s| &s.cwd)
            .chain(terminals.iter().map(|s| &s.cwd))
            .chain(kept)
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
                let screen = match (!reported || reads_past(&c.state))
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
                        let seen = screen
                            .as_ref()
                            .and_then(|screen| past_report(&c.state, adapter, screen));
                        match seen {
                            Some(seen) => Some(seen),
                            None if ended(&c.state) => {
                                behind(&c.state, super::behind::running(s.id, &c).await)
                            }
                            None => None,
                        }
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
    let ids: Vec<u128> = kept.iter().map(|s| s.id).collect();
    super::hold::keep_only(&ids);
    super::behind::keep_only(&ids);
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

/// Whether the screen of a session whose program reports `reported` is read
/// for what the report may not tell, as [`past_report`] takes it.
pub(crate) fn reads_past(reported: &State) -> bool {
    matches!(reported, State::Working | State::Done | State::Idle)
}

/// What a session whose program reports `reported` stands as, now that its
/// screen reads as `seen`; none when the report stands, as
/// [`past_report`] tells.
pub(crate) fn past(reported: &State, seen: State) -> Option<State> {
    match (reported, &seen) {
        (State::Working, State::Waiting(_)) => Some(seen),
        (reported, State::Background) if ended(reported) => Some(seen),
        _ => None,
    }
}

/// What a session whose program reports `reported` stands as while a
/// subagent it started is `running` or not; none when the report stands.
pub(crate) fn behind(reported: &State, running: bool) -> Option<State> {
    (running && ended(reported)).then_some(State::Background)
}

/// Whether `reported` says the session's turn ended, which work it started
/// may still run behind.
pub(crate) fn ended(reported: &State) -> bool {
    matches!(reported, State::Done | State::Idle)
}

/// How a session stands whose program reports `reported`, when `adapter`
/// reads on its `screen` what the report does not tell: a request that waits
/// on the user while the report says it works, as a report of working that
/// the program never followed up would hide the request; or work still
/// running behind a turn the report says ended, which brings the session
/// back once it ends. A dialog the user opened, such as a picker, asks
/// nothing of them and leaves the report standing.
fn past_report(
    reported: &State,
    adapter: &dyn adroit_agents::AgentAdapter,
    screen: &[String],
) -> Option<State> {
    past(reported, adapter.screen_state(screen)?)
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
    use adroit_model::testing::{session, shell, tree};

    use super::*;

    #[test]
    fn the_workspace_of_what_starts_running_is_kept_once_and_only_then() {
        let sessions = [session("a").cwd("/api").build()];
        let terminals = [shell(7, "/notes")];
        let t = tree()
            .sessions(sessions.clone())
            .terminal(terminals[0].clone())
            .checkout("/api", "/api", "main")
            .found("/notes", None)
            .build();
        let mut ran = Ran::default();

        assert_eq!(
            ran.started(&t, &sessions, &terminals),
            [PathBuf::from("/api"), "/notes".into()]
        );
        assert!(ran.started(&t, &sessions, &terminals).is_empty());
        assert!(ran.started(&t, &[], &[]).is_empty());
        assert_eq!(
            ran.started(&t, &sessions, &[]),
            [PathBuf::from("/api")],
            "one that ended and runs again starts anew"
        );
    }

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
            past_report(
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
            past_report(&State::Working, claude, &screen("model-while-working")),
            None
        );
    }

    #[test]
    fn a_request_on_screen_wins_over_no_report_but_working() {
        let opencode = adroit_agents::adapter(Agent::OpenCode);
        for reported in [State::Idle, State::Done, State::Failed] {
            assert_eq!(
                past_report(&reported, opencode, &screen("opencode-permission")),
                None
            );
        }
    }

    #[test]
    fn a_finished_turn_with_work_running_behind_it_stands_in_the_background() {
        let claude = adroit_agents::adapter(Agent::Claude);
        for reported in [State::Done, State::Idle] {
            assert_eq!(
                past_report(&reported, claude, &screen("background-work")),
                Some(State::Background),
                "{reported:?}"
            );
            assert_eq!(
                past_report(&reported, claude, &screen("after-a-reply")),
                None
            );
        }
    }

    #[test]
    fn work_running_behind_neither_a_working_turn_nor_a_failure_changes_it() {
        let claude = adroit_agents::adapter(Agent::Claude);
        for reported in [State::Working, State::Failed] {
            assert_eq!(
                past_report(&reported, claude, &screen("background-work")),
                None
            );
        }
    }

    #[test]
    fn a_subagent_running_behind_a_finished_turn_stands_it_in_the_background() {
        assert_eq!(behind(&State::Done, true), Some(State::Background));
        assert_eq!(behind(&State::Idle, true), Some(State::Background));
        assert_eq!(behind(&State::Done, false), None);
        assert_eq!(behind(&State::Working, true), None);
        assert_eq!(behind(&State::Failed, true), None);
        assert_eq!(
            behind(&State::Waiting(adroit_model::Wait::Input), true),
            None
        );
    }

    #[test]
    fn the_screen_is_read_past_a_report_of_working_done_or_idle_alone() {
        assert!(reads_past(&State::Working));
        assert!(reads_past(&State::Done));
        assert!(reads_past(&State::Idle));
        assert!(!reads_past(&State::Failed));
        assert!(!reads_past(&State::Waiting(adroit_model::Wait::Input)));
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
