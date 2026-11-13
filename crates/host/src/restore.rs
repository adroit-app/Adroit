//! Bringing back the sessions and terminals a terminal host took with it:
//! telling them from those that ended, and what bringing a chosen few back
//! starts or refuses.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use adroit_model::{BroughtBack, Interrupted, InterruptedKind, SessionId};

use crate::delegation::app::with_store;
use crate::delegation::{Session, store::KeptTerminal};

/// How a session or terminal Adroit keeps a record of stands, by what the
/// terminal host lists now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    /// The terminal host holds it still.
    Runs,
    /// Its program exited, or it was let go, in the terminal host it ran in.
    Ended,
    /// The terminal host it ran in is gone, and took it along.
    Interrupted,
    /// Which terminal host runs now cannot be told, so neither can its fate.
    Unknown,
}

/// The fate of what ran in the terminal host `ran_in` names, listed as
/// `listed` says (whether its program exited, none when it is not listed),
/// while the terminal host `now` names runs. One recorded before terminal
/// hosts were told apart ran in none, and ended when it is gone.
pub fn fate(listed: Option<bool>, ran_in: Option<&str>, now: Option<&str>) -> Fate {
    match (listed, ran_in, now) {
        (Some(false), ..) => Fate::Runs,
        (Some(true), ..) | (None, None, _) => Fate::Ended,
        (None, Some(_), None) => Fate::Unknown,
        (None, Some(ran_in), Some(now)) if ran_in == now => Fate::Ended,
        (None, Some(_), Some(_)) => Fate::Interrupted,
    }
}

/// `told`, given whether the terminal host now running drains toward its
/// stop, none when that cannot be told. What ends while it drains, it takes
/// down itself, so that waits for the terminal host after it to be told
/// interrupted rather than ended.
pub fn settled(told: Fate, draining: Option<bool>) -> Fate {
    match (told, draining) {
        (Fate::Ended, Some(false)) => Fate::Ended,
        (Fate::Ended, _) => Fate::Unknown,
        (told, _) => told,
    }
}

/// What the window lists of what waits to be brought back, sessions first.
pub fn listed(sessions: &[Session], terminals: &[KeptTerminal]) -> Vec<Interrupted> {
    let sessions = sessions.iter().filter_map(|record| {
        Some(Interrupted {
            id: crate::sessions::hex(record.terminal?),
            name: record.name.clone(),
            cwd: record.cwd.clone(),
            started_ms: record.started_ms,
            what: InterruptedKind::Session {
                agent: record.agent,
                profile: record.profile.clone(),
                conversation: record.conversation.clone().filter(|c| !c.is_empty()),
            },
        })
    });
    let terminals = terminals.iter().map(|kept| Interrupted {
        id: crate::sessions::hex(kept.terminal.id),
        name: kept.terminal.name.clone(),
        cwd: kept.terminal.cwd.clone(),
        started_ms: kept.started_ms,
        what: InterruptedKind::Terminal {
            program: kept.terminal.program,
            file: kept.terminal.file.clone(),
        },
    });
    sessions.chain(terminals).collect()
}

/// What waits to be brought back as `daemon` lists its sessions now: what
/// was recorded as interrupted, and what is about to be, so a window never
/// takes what the terminal host took for what ended in the moment between.
pub async fn waiting(daemon: &adroit_term::Daemon) -> Result<Vec<Interrupted>, String> {
    let native = daemon.sessions().await.map_err(|e| e.to_string())?;
    let now = daemon.generation();
    let root = crate::delegation::app::root();
    let (records, terminals, running) = with_store(&root, |store| {
        Ok((
            store.sessions()?,
            store.interrupted_terminals()?,
            store.terminals()?,
        ))
    })
    .await
    .map_err(|e| e.to_string())?;
    let taken = |terminal: u128, ran_in: Option<&str>| {
        let listed = native.iter().find(|n| n.id == terminal).map(|n| n.exited);
        fate(listed, ran_in, now.as_deref()) == Fate::Interrupted
    };
    let sessions: Vec<Session> = records
        .into_iter()
        .filter(|r| {
            r.interrupted
                || (!r.ended
                    && r.terminal
                        .is_some_and(|t| taken(t, r.generation.as_deref())))
        })
        .collect();
    let terminals: Vec<KeptTerminal> = terminals
        .into_iter()
        .chain(
            running
                .into_iter()
                .filter(|k| taken(k.terminal.id, k.generation.as_deref())),
        )
        .collect();
    Ok(listed(&sessions, &terminals))
}

/// One thing bringing back the chosen does.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Resume `record`'s conversation in a new session, opened by what
    /// stands for `parent`, the session that opened it, which was or is
    /// being brought back.
    Resume {
        record: Box<Session>,
        parent: Option<SessionId>,
    },
    /// Open a terminal again as `kept` was.
    Reopen(KeptTerminal),
    /// Bring back nothing for the interrupted `id`, for `why`; `held_by`
    /// names the session that holds its conversation already.
    Refuse {
        id: String,
        name: String,
        why: String,
        held_by: Option<SessionId>,
    },
}

/// What bringing back `chosen`, by the daemon ids they ran as, does: in the
/// order they were recorded, so a session that opened another is brought
/// back first and opens it again. `records` are every session Adroit keeps,
/// `running` the sessions the terminal host holds now, and `exists` says
/// whether a directory is there still.
pub fn plan(
    chosen: &[String],
    records: &[Session],
    terminals: &[KeptTerminal],
    running: &[adroit_model::Session],
    exists: impl Fn(&Path) -> bool,
) -> Vec<Step> {
    let is_chosen = |id: &str| chosen.iter().any(|c| c == id);
    let mut steps = Vec::new();
    let mut held: HashMap<(adroit_model::Agent, String), Option<SessionId>> = running
        .iter()
        .filter(|s| !s.session_id.is_empty())
        .map(|s| ((s.agent, s.session_id.clone()), s.delegation))
        .collect();
    let mut resumed: HashSet<SessionId> = records
        .iter()
        .filter(|r| r.brought_back_by.is_some())
        .map(|r| r.id)
        .collect();
    for record in records.iter().filter(|r| r.interrupted) {
        let Some(id) = record.terminal.map(crate::sessions::hex) else {
            continue;
        };
        if !is_chosen(&id) {
            continue;
        }
        let refuse = |why: String, held_by| Step::Refuse {
            id: id.clone(),
            name: record.name.clone(),
            why,
            held_by,
        };
        let Some(conversation) = record.conversation.clone().filter(|c| !c.is_empty()) else {
            steps.push(refuse(
                format!(
                    "{} never told which conversation it held",
                    record.agent.label()
                ),
                None,
            ));
            continue;
        };
        if !exists(&record.cwd) {
            steps.push(refuse(
                format!("{} is not there any more", record.cwd.display()),
                None,
            ));
            continue;
        }
        if let Some(by) = held.get(&(record.agent, conversation.clone())) {
            steps.push(refuse("its conversation runs already".into(), *by));
            continue;
        }
        held.insert((record.agent, conversation), None);
        resumed.insert(record.id);
        let parent = record.parent.filter(|parent| resumed.contains(parent));
        steps.push(Step::Resume {
            record: Box::new(record.clone()),
            parent,
        });
    }
    for kept in terminals {
        let id = crate::sessions::hex(kept.terminal.id);
        if !is_chosen(&id) {
            continue;
        }
        if exists(&kept.terminal.cwd) {
            steps.push(Step::Reopen(kept.clone()));
        } else {
            steps.push(Step::Refuse {
                id,
                name: kept.terminal.name.clone(),
                why: format!("{} is not there any more", kept.terminal.cwd.display()),
                held_by: None,
            });
        }
    }
    steps
}

/// How long bringing back waits between one start and the next, so the
/// agents do not all start at once.
const STAGGER: std::time::Duration = std::time::Duration::from_millis(200);

/// Bring back `chosen`, by the daemon ids they ran as, as [`plan`] says,
/// recording what stands for each; what is refused waits no more.
pub async fn bring_back(chosen: Vec<String>) -> Result<Vec<BroughtBack>, String> {
    // One at a time: a session being brought back is still recorded as
    // interrupted, and its new session not yet listed, until it has started,
    // so a second run beside it would start it again.
    static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _turn = ONE_AT_A_TIME.lock().await;
    let root = crate::delegation::app::root();
    let settings = crate::settings::current();
    let daemon = crate::daemon::daemon();
    let (records, terminals) = with_store(&root, |store| {
        Ok((store.sessions()?, store.interrupted_terminals()?))
    })
    .await
    .map_err(|e| e.to_string())?;
    let listed = daemon.sessions().await.map_err(|e| e.to_string())?;
    let running: Vec<_> = crate::sessions::agents(&listed).collect();
    let steps = plan(&chosen, &records, &terminals, &running, Path::is_dir);
    let mut out = Vec::new();
    let mut started = false;
    for step in steps {
        if !matches!(step, Step::Refuse { .. }) && std::mem::replace(&mut started, true) {
            tokio::time::sleep(STAGGER).await;
        }
        out.push(match step {
            Step::Resume { record, parent } => {
                resume(&root, &settings, &daemon, *record, parent).await
            }
            Step::Reopen(kept) => reopen(&root, &settings, &daemon, kept).await,
            Step::Refuse {
                id,
                name,
                why,
                held_by,
            } => {
                refuse(&root, &records, &terminals, &id, held_by).await;
                BroughtBack {
                    id,
                    name,
                    by: None,
                    refused: Some(why),
                }
            }
        });
    }
    Ok(out)
}

/// Let go of `chosen`, by the daemon ids they ran as.
pub async fn let_go(chosen: Vec<String>) -> Result<(), String> {
    let root = crate::delegation::app::root();
    with_store(&root, move |store| {
        for record in store.interrupted()? {
            if record
                .terminal
                .is_some_and(|t| chosen.contains(&crate::sessions::hex(t)))
            {
                store.let_go(record.id)?;
            }
        }
        for kept in store.interrupted_terminals()? {
            if chosen.contains(&crate::sessions::hex(kept.terminal.id)) {
                store.reopened_terminal(kept.terminal.id)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
}

async fn resume(
    root: &Path,
    settings: &crate::settings::Settings,
    daemon: &adroit_term::Daemon,
    record: Session,
    parent: Option<SessionId>,
) -> BroughtBack {
    let old = record
        .terminal
        .map(crate::sessions::hex)
        .unwrap_or_default();
    let name = record.name.clone();
    let failed = |why: String| BroughtBack {
        id: old.clone(),
        name: name.clone(),
        by: None,
        refused: Some(why),
    };
    let opener = match parent {
        Some(parent) => with_store(root, move |store| Ok(store.known(parent)?.brought_back_by))
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let request = crate::execution::StartRequest {
        agent: record.agent,
        profile: Some(record.profile.clone()),
        model: record.model.clone(),
        name: record.name.clone(),
        name_given: record.name_given,
        cwd: record.cwd.clone(),
        permissions: record.permissions.clone(),
        resume: record.conversation.clone(),
        home: record.home.clone(),
        env: crate::daemon::agent_env(),
        started_ms: crate::delegation::now_ms(),
        ..Default::default()
    };
    let palette = match crate::themes::palette_in_force(settings) {
        Ok(palette) => palette,
        Err(e) => return failed(e),
    };
    let program = crate::work_host::adroit_program();
    let (resolved, recorded) =
        match crate::delegation::app::prepare(root, settings, request, opener, &program).await {
            Ok(prepared) => prepared,
            Err(e) => return failed(e),
        };
    let mut spawn = resolved.spawn;
    spawn.tags.extend(crate::sessions::restored_tags(&old));
    let attach = recorded.clone();
    let spawned = crate::execution::spawn_headless_prepared(daemon, spawn, &palette, {
        move |terminal| async move { attach.attach(terminal).await }
    })
    .await;
    let terminal = match spawned {
        Ok(terminal) => terminal,
        Err(e) => {
            recorded.failed().await;
            return failed(crate::execution::spawn_failure(&e));
        }
    };
    let (id, by) = (record.id, recorded.id);
    if let Err(e) = with_store(root, move |store| store.brought_back(id, by)).await {
        return failed(e.to_string());
    }
    BroughtBack {
        id: old,
        name,
        by: Some(crate::sessions::hex(terminal)),
        refused: None,
    }
}

async fn reopen(
    root: &Path,
    settings: &crate::settings::Settings,
    daemon: &adroit_term::Daemon,
    kept: KeptTerminal,
) -> BroughtBack {
    let old = kept.terminal.id;
    let request = crate::execution::TerminalStartRequest {
        name: kept.terminal.name.clone(),
        cwd: kept.terminal.cwd.clone(),
        program: kept.terminal.program,
        env: crate::daemon::terminal_env(),
        file: kept.terminal.file.clone(),
        command: kept.terminal.command.clone(),
        restored_from: Some(crate::sessions::hex(old)),
        opener: None,
        show: false,
    };
    let outcome = match crate::terminals::start(daemon, settings, request).await {
        Ok(terminal) => {
            let generation = daemon.generation();
            let now = crate::delegation::now_ms();
            let id = crate::sessions::hex(terminal.id);
            with_store(root, move |store| {
                store.open_terminal(&terminal, generation, now)?;
                store.reopened_terminal(old)
            })
            .await
            .map(|()| id)
            .map_err(|e| e.to_string())
        }
        Err(e) => Err(e),
    };
    BroughtBack {
        id: crate::sessions::hex(old),
        name: kept.terminal.name,
        by: outcome.as_ref().ok().cloned(),
        refused: outcome.err(),
    }
}

async fn refuse(
    root: &Path,
    records: &[Session],
    terminals: &[KeptTerminal],
    id: &str,
    held_by: Option<SessionId>,
) {
    let record = records
        .iter()
        .find(|r| r.terminal.map(crate::sessions::hex).as_deref() == Some(id))
        .map(|r| r.id);
    let terminal = terminals
        .iter()
        .find(|k| crate::sessions::hex(k.terminal.id) == id)
        .map(|k| k.terminal.id);
    let _ = with_store(root, move |store| {
        match (record, held_by, terminal) {
            (Some(record), Some(by), _) => store.brought_back(record, by)?,
            (Some(record), None, _) => store.let_go(record)?,
            (None, _, Some(terminal)) => store.reopened_terminal(terminal)?,
            (None, _, None) => {}
        }
        Ok(())
    })
    .await;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use adroit_model::{Agent, TerminalProgram, TerminalSession};

    use super::*;
    use crate::testing::record;

    #[test]
    fn what_ran_is_told_by_the_terminal_host_it_ran_in() {
        let cases = [
            (Some(false), Some("a"), Some("a"), Fate::Runs),
            (Some(false), Some("a"), Some("b"), Fate::Runs),
            (Some(true), Some("a"), Some("a"), Fate::Ended),
            (None, Some("a"), Some("a"), Fate::Ended),
            (None, Some("a"), Some("b"), Fate::Interrupted),
            (None, Some("a"), None, Fate::Unknown),
            (None, None, Some("b"), Fate::Ended),
            (None, None, None, Fate::Ended),
        ];
        for (listed, ran_in, now, expected) in cases {
            assert_eq!(
                fate(listed, ran_in, now),
                expected,
                "listed {listed:?}, ran in {ran_in:?}, now {now:?}"
            );
        }
    }

    #[test]
    fn what_ends_while_its_terminal_host_drains_toward_its_stop_waits_for_the_next_one() {
        let cases = [
            (Fate::Ended, Some(false), Fate::Ended),
            (Fate::Ended, Some(true), Fate::Unknown),
            (Fate::Ended, None, Fate::Unknown),
            (Fate::Runs, Some(true), Fate::Runs),
            (Fate::Interrupted, Some(true), Fate::Interrupted),
            (Fate::Unknown, Some(false), Fate::Unknown),
        ];
        for (told, draining, expected) in cases {
            assert_eq!(
                settled(told, draining),
                expected,
                "told {told:?}, draining {draining:?}"
            );
        }
    }

    fn interrupted(name: &str, terminal: u128, conversation: Option<&str>) -> Session {
        let mut built = record(name).build();
        built.terminal = Some(terminal);
        built.conversation = conversation.map(Into::into);
        built.interrupted = true;
        built.ended = true;
        built
    }

    fn shell(id: u128, cwd: &str) -> KeptTerminal {
        KeptTerminal {
            terminal: TerminalSession {
                id,
                name: "shell".into(),
                cwd: cwd.into(),
                program: TerminalProgram::Shell,
                ..Default::default()
            },
            generation: Some("a".into()),
            started_ms: 1,
        }
    }

    fn chosen(ids: &[u128]) -> Vec<String> {
        ids.iter().map(|id| crate::sessions::hex(*id)).collect()
    }

    fn outcome(step: &Step) -> String {
        match step {
            Step::Resume { record, .. } => format!("resume {}", record.name),
            Step::Reopen(kept) => format!("reopen {}", kept.terminal.id),
            Step::Refuse { name, why, .. } => format!("refuse {name}: {why}"),
        }
    }

    fn outcomes(steps: &[Step]) -> Vec<String> {
        steps.iter().map(outcome).collect()
    }

    #[test]
    fn only_what_was_chosen_comes_back() {
        let records = [
            interrupted("a", 1, Some("c1")),
            interrupted("b", 2, Some("c2")),
        ];
        let terminals = [shell(3, "/repo"), shell(4, "/repo")];

        let steps = plan(&chosen(&[2, 4]), &records, &terminals, &[], |_| true);

        assert_eq!(outcomes(&steps), ["resume b", "reopen 4"]);
    }

    #[test]
    fn a_session_whose_conversation_is_not_known_is_refused() {
        let records = [interrupted("a", 1, None), interrupted("b", 2, Some(""))];

        let steps = plan(&chosen(&[1, 2]), &records, &[], &[], |_| true);

        assert_eq!(
            outcomes(&steps),
            [
                "refuse a: Claude Code never told which conversation it held",
                "refuse b: Claude Code never told which conversation it held",
            ]
        );
    }

    #[test]
    fn what_ran_in_a_directory_gone_is_refused_saying_so() {
        let records = [interrupted("a", 1, Some("c1"))];
        let terminals = [shell(3, "/gone")];
        let mut gone = records[0].clone();
        gone.cwd = "/gone".into();

        let steps = plan(&chosen(&[1, 3]), &[gone], &terminals, &[], |p| {
            p != Path::new("/gone")
        });

        assert_eq!(
            outcomes(&steps),
            [
                "refuse a: /gone is not there any more",
                "refuse shell: /gone is not there any more"
            ]
        );
    }

    #[test]
    fn a_conversation_a_session_runs_already_is_not_resumed_again() {
        let records = [interrupted("a", 1, Some("c1"))];
        let holder = SessionId::new();
        let running = adroit_model::Session {
            agent: Agent::Claude,
            session_id: "c1".into(),
            delegation: Some(holder),
            ..Default::default()
        };

        let steps = plan(&chosen(&[1]), &records, &[], &[running], |_| true);

        assert_eq!(
            steps,
            [Step::Refuse {
                id: crate::sessions::hex(1),
                name: "a".into(),
                why: "its conversation runs already".into(),
                held_by: Some(holder),
            }]
        );
    }

    #[test]
    fn the_same_conversation_interrupted_twice_comes_back_once() {
        let records = [
            interrupted("a", 1, Some("c1")),
            interrupted("again", 2, Some("c1")),
        ];

        let steps = plan(&chosen(&[1, 2]), &records, &[], &[], |_| true);

        assert_eq!(
            outcomes(&steps),
            ["resume a", "refuse again: its conversation runs already"]
        );
    }

    #[test]
    fn the_same_conversation_of_another_agent_is_another_conversation() {
        let mut codex = interrupted("b", 2, Some("c1"));
        codex.agent = Agent::Codex;
        let records = [interrupted("a", 1, Some("c1")), codex];

        let steps = plan(&chosen(&[1, 2]), &records, &[], &[], |_| true);

        assert_eq!(outcomes(&steps), ["resume a", "resume b"]);
    }

    fn parents(steps: &[Step]) -> Vec<Option<SessionId>> {
        steps
            .iter()
            .filter_map(|step| match step {
                Step::Resume { parent, .. } => Some(*parent),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_session_opened_by_one_brought_back_is_opened_by_what_stands_for_it() {
        let opener = interrupted("opener", 1, Some("c1"));
        let mut before = opener.clone();
        before.interrupted = false;
        before.brought_back_by = Some(SessionId::new());
        let mut opened = interrupted("opened", 2, Some("c2"));
        opened.parent = Some(opener.id);
        let records = [opener.clone(), opened.clone()];

        let together = plan(&chosen(&[1, 2]), &records, &[], &[], |_| true);
        let after = plan(&chosen(&[2]), &[before, opened], &[], &[], |_| true);

        assert_eq!(parents(&together), [None, Some(opener.id)]);
        assert_eq!(parents(&after), [Some(opener.id)]);
    }

    #[test]
    fn a_session_opened_by_one_not_brought_back_answers_to_the_user() {
        let opener = interrupted("opener", 1, Some("c1"));
        let mut opened = interrupted("opened", 2, Some("c2"));
        opened.parent = Some(opener.id);

        let steps = plan(&chosen(&[2]), &[opener, opened], &[], &[], |_| true);

        assert_eq!(parents(&steps), [None]);
    }

    #[test]
    fn what_waits_is_listed_by_the_daemon_id_it_ran_as() {
        let records = [interrupted("a", 1, Some("c1"))];
        let terminals = [shell(3, "/repo")];

        let listed = listed(&records, &terminals);

        let ids: Vec<_> = listed
            .iter()
            .map(|i| (i.id.clone(), i.is_session()))
            .collect();
        assert_eq!(
            ids,
            [
                (crate::sessions::hex(1), true),
                (crate::sessions::hex(3), false)
            ]
        );
        assert_eq!(listed[1].cwd, PathBuf::from("/repo"));
    }
}
