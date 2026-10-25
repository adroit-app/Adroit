use adroit_model::links::Links;
use adroit_model::repository::{Listing, Spot};
use adroit_model::testing::{session, shell};
use adroit_model::{Agent, Past, Session};
use adroit_protocol::testing::{listed, snapshot};
use adroit_protocol::{
    Event, Offers, PastChange, PastSight, PastWork, PickSight, RepositoryRead, RepositorySight,
    Request, SPOKEN, Seq, Version,
};

use super::*;

/// The window a host run without one speaks as, in [`scenario`].
const LONE: WindowId = WindowId(1);

/// A host run without a window: `requests` in turn, as one window asks
/// them, the work they leave and every event they draw for that window.
fn scenario(work: Work, requests: impl IntoIterator<Item = Request>) -> (Work, Vec<Event>) {
    let mut host = Host::new(work);
    let events = requests
        .into_iter()
        .flat_map(|request| host.handle(LONE, request))
        .filter(|(to, _)| *to == LONE)
        .map(|(_, event)| event)
        .collect();
    (host.work, events)
}

/// A host as the one window [`LONE`] is served by it.
trait Lone {
    /// What the host tells [`LONE`] for `request`, which it asked.
    fn on(&mut self, request: Request) -> Vec<Event>;
    /// What [`LONE`] is sent of `changes` read.
    fn read<C: Into<Change>>(&mut self, changes: impl IntoIterator<Item = C>) -> Option<Event>;
}

impl Lone for Host {
    fn on(&mut self, request: Request) -> Vec<Event> {
        to(LONE, self.handle(LONE, request))
    }

    fn read<C: Into<Change>>(&mut self, changes: impl IntoIterator<Item = C>) -> Option<Event> {
        to(LONE, self.record(changes)).into_iter().next()
    }
}

/// The events of `sent` for `window`.
fn to(window: WindowId, sent: Vec<Sent>) -> Vec<Event> {
    sent.into_iter()
        .filter(|(to, _)| *to == window)
        .map(|(_, event)| event)
        .collect()
}

const A: WindowId = WindowId(1);
const B: WindowId = WindowId(2);

/// A host two windows said hello to, each showing the session named.
fn two_windows(a: &str, b: &str) -> Host {
    let mut host = Host::default();
    for (window, session) in [(A, a), (B, b)] {
        host.handle(window, hello(&[1]));
        host.handle(window, watch(Some(session), None));
    }
    host
}

#[test]
fn what_is_read_reaches_each_window_that_shows_it_and_no_other() {
    let mut host = two_windows("a", "b");
    let sent = host.record([links("a"), links("b")]);
    assert_eq!(changes(to(A, sent.clone()).pop()), vec![links("a")]);
    assert_eq!(changes(to(B, sent).pop()), vec![links("b")]);
    host.handle(B, watch(Some("a"), None));
    let sent = host.record([links("a")]);
    assert!(to(A, sent.clone()).is_empty(), "A has it already");
    assert_eq!(changes(to(B, sent).pop()), vec![links("a")]);
}

#[test]
fn each_window_s_patches_follow_on_from_its_own_snapshot() {
    let mut host = two_windows("a", "b");
    let seqs = |events: Vec<Event>| -> Vec<Seq> {
        events
            .into_iter()
            .filter_map(|e| match e {
                Event::Patch(p) => Some(p.seq),
                Event::Snapshot(s) => Some(s.seq),
                _ => None,
            })
            .collect()
    };
    let a = seqs(to(A, host.record([links("a")])));
    let b = seqs(to(B, host.record([links("b")])));
    let late = WindowId(3);
    let snapshot = seqs(to(late, host.handle(late, hello(&[1]))));
    assert_eq!((a, b), (vec![Seq(1)], vec![Seq(1)]));
    let next = seqs(to(
        late,
        host.record([TreeChange::Failing {
            error: Some("gone".into()),
        }]),
    ));
    assert_eq!(next, vec![snapshot[0].next()]);
}

#[test]
fn a_window_said_hello_late_is_sent_what_every_window_is_and_nothing_another_shows() {
    let mut host = two_windows("a", "b");
    host.record([links("a")]);
    host.record([TreeChange::Failing {
        error: Some("no daemon".into()),
    }]);
    let late = WindowId(3);
    let Some(Event::Snapshot(snapshot)) = to(late, host.handle(late, hello(&[1]))).pop() else {
        panic!("a snapshot");
    };
    assert_eq!(snapshot.tree.failing.as_deref(), Some("no daemon"));
    assert!(snapshot.details.sessions.is_empty());
}

#[test]
fn the_sessions_listed_and_the_terminals_opened_reach_every_window() {
    let mut host = two_windows("a", "b");
    let sent = host.handle(
        B,
        Request::TerminalOpened {
            terminal: shell(7, "/w"),
        },
    );
    assert_eq!(tree_changes(to(A, sent.clone())).len(), 1);
    assert_eq!(tree_changes(to(B, sent)).len(), 1);
    let sent = host.record([
        TreeChange::Failing { error: None },
        TreeChange::Failing {
            error: Some("x".into()),
        },
    ]);
    assert_eq!(to(A, sent.clone()).len(), 1);
    assert_eq!(to(B, sent).len(), 1);
}

#[test]
fn what_was_composed_or_read_for_one_window_reaches_it_alone() {
    let mut host = two_windows("a", "a");
    let sent = host.handle(
        A,
        Request::Compose {
            asked: 1,
            agent: Agent::Claude,
            text: "fix".into(),
            images: Vec::new(),
        },
    );
    assert_eq!(to(A, sent.clone()).len(), 1);
    assert!(to(B, sent).is_empty());
    let dialog = adroit_protocol::ConsoleChange::Dialog {
        dialog: adroit_protocol::Dialog {
            session: "a".into(),
            open: true,
        },
    };
    let sent = host.record_for(B, [dialog]);
    assert_eq!(sent.map(|(to, _)| to), Some(B));
    assert!(host.record_for(WindowId(9), [links("a")]).is_none(), "gone");
}

#[test]
fn what_a_window_asks_carried_out_or_read_is_known_by_the_window_that_asked() {
    let mut host = two_windows("a", "b");
    let mut jobs = host.take_jobs().expect("taken once");
    let mut redraws = host.take_redraws().expect("taken once");
    let rename = Request::Rename {
        id: "b".into(),
        name: "fixer".into(),
        was: "fix".into(),
        agent: Agent::Claude,
    };
    host.handle(B, rename.clone());
    assert_eq!(jobs.try_recv().ok(), Some((B, rename)));
    host.handle(
        A,
        Request::Redrawn {
            session: "a".into(),
            agent: Agent::Claude,
        },
    );
    assert_eq!(redraws.try_recv().ok().map(|r| r.window), Some(A));
}

#[test]
fn a_neovim_on_a_file_no_window_shows_any_longer_is_asked_to_quit_and_nothing_else() {
    let mut host = two_windows("a", "b");
    let mut quits = host.take_quits().expect("taken once");
    let on_file = TerminalSession {
        program: TerminalProgram::Neovim,
        file: Some("/w/a.rs".into()),
        ..shell(7, "/w")
    };
    let on_nothing = TerminalSession {
        program: TerminalProgram::Neovim,
        ..shell(8, "/w")
    };
    for terminal in [on_file, on_nothing, shell(9, "/w")] {
        host.handle(A, Request::TerminalOpened { terminal });
    }
    let showing = |ids: &[u128]| {
        Request::Watch(Box::new(Sight {
            terminals: ids.iter().copied().collect(),
            ..Sight::default()
        }))
    };
    host.handle(A, showing(&[7, 8, 9]));
    host.handle(B, showing(&[7]));
    host.handle(A, showing(&[]));
    assert!(quits.try_recv().is_err(), "another window shows it");
    host.handle(B, showing(&[]));
    assert_eq!(quits.try_recv().ok(), Some(7));
    assert!(quits.try_recv().is_err());
    host.handle(A, showing(&[7]));
    host.leave(A);
    assert!(
        quits.try_recv().is_err(),
        "a window gone shows nothing it hid"
    );
}

#[test]
fn the_lister_counts_the_latest_typing_any_window_saw_and_forgets_a_window_gone() {
    let mut host = two_windows("a", "b");
    let inputs = host.inputs();
    host.handle(
        A,
        Request::Typed {
            inputs: BTreeMap::from([(7, 1_000), (8, 5_000)]),
        },
    );
    host.handle(
        B,
        Request::Typed {
            inputs: BTreeMap::from([(7, 3_000)]),
        },
    );
    assert_eq!(*inputs.borrow(), BTreeMap::from([(7, 3_000), (8, 5_000)]));
    host.leave(B);
    assert_eq!(*inputs.borrow(), BTreeMap::from([(7, 1_000), (8, 5_000)]));
}

#[test]
fn the_watchers_see_what_every_window_shows_until_it_goes() {
    let mut host = two_windows("a", "b");
    let seen = host.seen();
    let shown = || -> Vec<Option<String>> {
        seen.borrow()
            .iter()
            .map(|s| s.sight.session.clone())
            .collect()
    };
    assert_eq!(shown(), [Some("a".into()), Some("b".into())]);
    host.record([links("a"), links("b")]);
    host.leave(A);
    assert_eq!(shown(), [Some("b".into())]);
    assert_eq!(
        host.work().details().sessions.keys().collect::<Vec<_>>(),
        ["b"],
        "what A alone showed is no longer kept"
    );
}

#[test]
fn a_repository_read_reaches_the_windows_that_list_it_as_it_was_read() {
    let mut host = Host::default();
    for (window, spot) in [
        (A, Spot::Log),
        (B, Spot::Files(adroit_model::repository::Rev::WorkingCopy)),
    ] {
        host.handle(window, hello(&[1]));
        host.handle(
            window,
            Request::Watch(Box::new(Sight {
                repository: Some(RepositorySight {
                    worktree: "/w".into(),
                    spot,
                }),
                ..Default::default()
            })),
        );
    }
    let sent = host.record([log("/w")]);
    assert_eq!(changes(to(A, sent.clone()).pop()), vec![log("/w")]);
    assert!(to(B, sent).is_empty(), "B lists the changes, not the log");
}

#[test]
fn a_listing_meant_for_some_windows_reaches_those_alone() {
    let mut host = two_windows("a", "b");
    host.handle(
        B,
        Request::Watch(Box::new(Sight {
            tree: adroit_protocol::TreeSight {
                every_worktree: true,
                ..Default::default()
            },
            ..Default::default()
        })),
    );
    let listing = TreeChange::Listed {
        listed: Box::default(),
    };
    let sent = host.record_to([listing], |s| s.tree.every_worktree);
    assert!(to(A, sent.clone()).is_empty());
    assert_eq!(tree_changes(to(B, sent)).len(), 1);
}

#[test]
fn what_takes_a_while_is_queued_for_the_background_and_answers_nothing_at_once() {
    let mut host = Host::default();
    let mut jobs = host.take_jobs().expect("taken once");
    assert!(host.take_jobs().is_none());
    let stop = Request::Stop {
        session: Box::new(Session::default()),
    };
    let rename = Request::Rename {
        id: "0a".into(),
        name: "fixer".into(),
        was: "fix".into(),
        agent: Agent::Claude,
    };
    assert!(host.on(stop.clone()).is_empty());
    assert!(host.on(rename.clone()).is_empty());
    assert_eq!(jobs.try_recv().ok(), Some((LONE, stop)));
    assert_eq!(jobs.try_recv().ok(), Some((LONE, rename)));
}

#[test]
fn the_commands_read_reach_the_window_only_for_what_it_has_in_sight() {
    use adroit_protocol::{CommandsRead, CommandsSight};
    let mut host = Host::default();
    let sight = Sight {
        commands: Some(CommandsSight {
            agent: Agent::Claude,
            home: None,
            dir: "/w".into(),
        }),
        ..Default::default()
    };
    host.on(Request::Watch(Box::new(sight)));
    let read = |dir: &str| PromptChange::Commands {
        read: CommandsRead {
            agent: Agent::Claude,
            home: None,
            dir: dir.into(),
            commands: Vec::new(),
        },
    };
    assert!(host.read([read("/elsewhere")]).is_none());
    assert!(host.read([read("/w")]).is_some());
    let wanted = host.commands_wanted();
    assert!(host.on(Request::CommandsWanted).is_empty());
    assert!(wanted.has_changed().unwrap());
}

#[test]
fn the_host_tells_every_agent_s_program_and_usage_as_its_adapter_does() {
    let work = Work::configured(ConfigWork::default()).snapshot();
    for agent in Agent::ALL {
        let adapter = adroit_agents::adapter(agent);
        let facts = work.agents.of(agent).expect("every agent is told of");
        assert_eq!(facts.program, adapter.program());
        assert_eq!(facts.usage_url.as_deref(), adapter.usage_url());
        assert_eq!(facts.usage_limits, adapter.capabilities().usage_limits);
    }
}

fn hello(versions: &[u32]) -> Request {
    Request::Hello {
        versions: versions.iter().copied().map(Version).collect(),
    }
}

#[test]
fn the_host_answers_hello_by_the_versions_both_speak() {
    let work = Work {
        seq: Seq(4),
        ..Default::default()
    };
    let cases = [
        (
            hello(&[1]),
            vec![
                Event::Welcome {
                    version: Version(1),
                },
                snapshot().seq(4).into(),
            ],
        ),
        (
            hello(&[99]),
            vec![Event::Refused {
                versions: SPOKEN.to_vec(),
            }],
        ),
        (
            hello(&[]),
            vec![Event::Refused {
                versions: SPOKEN.to_vec(),
            }],
        ),
    ];
    for (request, want) in cases {
        let (_, events) = scenario(work.clone(), [request.clone()]);
        assert_eq!(events, want, "{request:?}");
    }
}

#[test]
fn a_window_that_lost_track_is_sent_the_whole_work() {
    let (work, events) = scenario(
        Work {
            seq: Seq(9),
            ..Default::default()
        },
        [Request::Snapshot],
    );
    assert_eq!(events, vec![snapshot().seq(9).into()]);
    assert_eq!(
        work,
        Work {
            seq: Seq(9),
            ..Default::default()
        }
    );
}

fn watch(session: Option<&str>, worktree: Option<&str>) -> Request {
    Request::Watch(Box::new(Sight {
        session: session.map(Into::into),
        repository: worktree.map(|w| RepositorySight {
            worktree: w.into(),
            spot: Spot::Log,
        }),
        ..Default::default()
    }))
}

fn links(session: &str) -> DetailsChange {
    DetailsChange::Links {
        session: session.into(),
        links: Links::default(),
    }
}

fn log(worktree: &str) -> DetailsChange {
    DetailsChange::Repository {
        worktree: worktree.into(),
        read: RepositoryRead {
            spot: Spot::Log,
            listing: Ok(Listing::Log(Vec::new())),
            revision: None,
        },
    }
}

fn changes(event: Option<Event>) -> Vec<DetailsChange> {
    match event {
        Some(Event::Patch(patch)) => patch
            .changes
            .into_iter()
            .filter_map(|c| match c {
                Change::Details(c) => Some(c),
                Change::Tree(_)
                | Change::Past(_)
                | Change::Pick(_)
                | Change::Place(_)
                | Change::Prompt(_)
                | Change::Config(_)
                | Change::Console(_) => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn what_the_host_reads_reaches_the_window_once_and_only_while_shown() {
    let mut host = Host::default();
    host.on(watch(Some("a"), Some("/w")));
    // (read, what the window is sent)
    let cases = [
        (vec![links("a")], vec![links("a")]),
        (vec![links("a")], vec![]),
        (vec![links("b"), log("/v")], vec![]),
        (vec![log("/w"), log("/w")], vec![log("/w")]),
    ];
    for (read, want) in cases {
        assert_eq!(changes(host.read(read.clone())), want, "{read:?}");
    }
    assert_eq!(host.work().snapshot().seq, Seq(2));
}

fn listing(dir: Option<&str>) -> Request {
    Request::Watch(Box::new(Sight {
        past: Some(PastSight {
            dir: dir.map(Into::into),
        }),
        ..Default::default()
    }))
}

fn pasts(dir: Option<&str>, ids: &[&str]) -> PastChange {
    PastChange::Listed {
        sight: PastSight {
            dir: dir.map(Into::into),
        },
        conversations: ids
            .iter()
            .map(|id| Past {
                agent: Agent::Claude,
                id: id.to_string(),
                cwd: "/r".into(),
                title: id.to_string(),
                at_ms: 0,
                ..Default::default()
            })
            .collect(),
    }
}

fn past_changes(events: impl IntoIterator<Item = Event>) -> Vec<PastChange> {
    events
        .into_iter()
        .flat_map(|e| match e {
            Event::Patch(patch) => patch.changes,
            _ => Vec::new(),
        })
        .filter_map(|c| match c {
            Change::Past(c) => Some(c),
            _ => None,
        })
        .collect()
}

#[test]
fn the_conversations_listed_reach_the_window_only_for_the_list_it_shows() {
    let mut host = Host::default();
    host.on(listing(Some("/r")));
    // (listed, what the window is sent)
    let cases = [
        (pasts(Some("/r"), &["a"]), vec![pasts(Some("/r"), &["a"])]),
        (pasts(Some("/r"), &["a"]), vec![]),
        (pasts(None, &["a", "b"]), vec![]),
        (pasts(Some("/q"), &["c"]), vec![]),
    ];
    for (read, want) in cases {
        assert_eq!(past_changes(host.read([read.clone()])), want, "{read:?}");
    }
}

#[test]
fn closing_or_changing_the_list_drops_the_conversations_listed() {
    let mut host = Host::default();
    host.on(listing(Some("/r")));
    host.read([pasts(Some("/r"), &["a"])]);
    assert_eq!(past_changes(host.on(listing(None))), vec![PastChange::Left]);
    host.read([pasts(None, &["a", "b"])]);
    assert_eq!(
        past_changes(host.on(watch(None, None))),
        vec![PastChange::Left]
    );
    assert_eq!(host.work().snapshot().past, PastWork::default());
    assert!(host.on(watch(None, None)).is_empty());
}

#[test]
fn moving_away_drops_what_was_read_for_what_is_no_longer_shown() {
    let mut host = Host::default();
    host.on(watch(Some("a"), Some("/w")));
    host.read([links("a"), log("/w")]);
    let events = host.on(watch(Some("a"), Some("/v")));
    assert_eq!(
        changes(events.into_iter().next()),
        vec![DetailsChange::RepositoryLeft {
            worktree: "/w".into()
        }]
    );
    let events = host.on(watch(None, None));
    assert_eq!(
        changes(events.into_iter().next()),
        vec![DetailsChange::SessionLeft {
            session: "a".into()
        }]
    );
    assert_eq!(host.work().details(), &DetailsWork::default());
    assert!(host.on(watch(None, None)).is_empty());
}

#[test]
fn the_watchers_are_told_what_the_window_shows_and_when_to_read_again() {
    let mut host = Host::default();
    let seen = host.seen();
    let again = host.again();
    host.on(watch(Some("a"), None));
    assert_eq!(seen.borrow()[0].sight.session.as_deref(), Some("a"));
    assert!(!again.has_changed().unwrap());
    assert!(host.on(Request::ReadAgain).is_empty());
    assert!(again.has_changed().unwrap());
}

fn tree_changes(events: Vec<Event>) -> Vec<TreeChange> {
    events
        .into_iter()
        .flat_map(|e| match e {
            Event::Patch(patch) => patch.changes,
            _ => Vec::new(),
        })
        .filter_map(|c| match c {
            Change::Tree(c) => Some(c),
            Change::Details(_)
            | Change::Past(_)
            | Change::Pick(_)
            | Change::Place(_)
            | Change::Prompt(_)
            | Change::Config(_)
            | Change::Console(_) => None,
        })
        .collect()
}

fn sees(marked: &[&str], listed: &[&str]) -> Request {
    Request::Watch(Box::new(Sight {
        tree: adroit_protocol::TreeSight {
            marked: marked.iter().map(Into::into).collect(),
            listed: listed.iter().map(Into::into).collect(),
            ..Default::default()
        },
        ..Default::default()
    }))
}

fn entries(dir: &str) -> TreeChange {
    TreeChange::Entries {
        dir: dir.into(),
        entries: Vec::new(),
    }
}

fn marks(worktree: &str) -> TreeChange {
    TreeChange::Marks {
        worktree: worktree.into(),
        marks: Default::default(),
    }
}

#[test]
fn the_sessions_reach_the_window_whatever_it_shows_and_the_files_only_while_listed() {
    let mut host = Host::default();
    host.on(sees(&["/w"], &["/w"]));
    let failing = TreeChange::Failing {
        error: Some("no daemon".into()),
    };
    // (read, what the window is sent)
    let cases = [
        (vec![failing.clone()], vec![failing]),
        (
            vec![entries("/w"), marks("/w")],
            vec![entries("/w"), marks("/w")],
        ),
        (vec![entries("/v"), marks("/v")], vec![]),
    ];
    for (read, want) in cases {
        let got = tree_changes(host.read(read.clone()).into_iter().collect());
        assert_eq!(got, want, "{read:?}");
    }
    let left = tree_changes(host.on(sees(&[], &[])));
    assert_eq!(
        left,
        [
            TreeChange::MarksLeft {
                worktree: "/w".into()
            },
            TreeChange::EntriesLeft { dir: "/w".into() },
        ]
    );
    assert!(host.work().tree().entries.is_empty());
}

#[test]
fn a_terminal_the_window_opens_or_closes_is_placed_at_once() {
    let mut host = Host::default();
    host.read([TreeChange::Listed {
        listed: Box::new(
            listed()
                .session(session("a").cwd("/w"))
                .outside_git()
                .build(),
        ),
    }]);
    let opened = host.on(Request::TerminalOpened {
        terminal: shell(7, "/w"),
    });
    assert_eq!(tree_changes(opened).len(), 1);
    let ids = |host: &Host| {
        host.work()
            .tree()
            .tree()
            .worktrees()
            .flat_map(|w| w.terminals.iter().map(|t| t.id))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&host), [7]);
    assert_eq!(host.work().tree().terminals().len(), 1);
    let again = host.on(Request::TerminalOpened {
        terminal: shell(7, "/w"),
    });
    assert!(
        again.is_empty(),
        "a terminal listed already changes nothing"
    );
    host.on(Request::TerminalClosed { id: 7 });
    assert!(ids(&host).is_empty());
    assert!(host.work().tree().terminals().is_empty());
}

#[test]
fn the_lister_is_told_when_to_list_again_and_what_was_typed() {
    let mut host = Host::default();
    let relist = host.relist();
    let inputs = host.inputs();
    assert!(host.on(Request::Relist).is_empty());
    assert!(relist.has_changed().unwrap());
    let typed = BTreeMap::from([(7, 1_000)]);
    assert!(
        host.on(Request::Typed {
            inputs: typed.clone()
        })
        .is_empty()
    );
    assert!(inputs.has_changed().unwrap());
    assert_eq!(*inputs.borrow(), typed);
}

#[test]
fn the_watchers_read_the_session_shown_as_the_host_lists_it() {
    let mut host = Host::default();
    let seen = host.seen();
    let listing = |session_id: &str| TreeChange::Listed {
        listed: Box::new(
            listed()
                .session(session("a").session_id(session_id))
                .build(),
        ),
    };
    host.on(watch(Some("a"), None));
    assert_eq!(seen.borrow()[0].session, None, "not listed yet");
    host.read([listing("first")]);
    let conversation = || {
        seen.borrow()[0]
            .session
            .as_ref()
            .map(|s| s.session_id.clone())
    };
    assert_eq!(conversation().as_deref(), Some("first"));
    host.read([listing("resumed")]);
    assert_eq!(conversation().as_deref(), Some("resumed"));
    host.on(watch(None, None));
    assert_eq!(seen.borrow()[0].session, None);
}

fn picking(listed: &[&str]) -> Request {
    Request::Watch(Box::new(Sight {
        pick: Some(PickSight {
            listed: listed.iter().map(Into::into).collect(),
        }),
        ..Default::default()
    }))
}

fn offers(recent: &str) -> PickChange {
    PickChange::Offers {
        offers: Offers {
            recent: vec![recent.into()],
            ..Default::default()
        },
    }
}

fn patched(events: impl IntoIterator<Item = Event>) -> Vec<Change> {
    events
        .into_iter()
        .flat_map(|e| match e {
            Event::Patch(patch) => patch.changes,
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn the_picker_s_offers_reach_the_window_while_it_is_open_and_leave_as_it_closes() {
    let mut host = Host::default();
    assert!(patched(host.read([offers("/a")])).is_empty());
    host.on(picking(&[]));
    assert_eq!(
        patched(host.read([offers("/a")])),
        vec![Change::Pick(offers("/a"))]
    );
    assert_eq!(
        patched(host.on(watch(None, None))),
        vec![Change::Pick(PickChange::Left)]
    );
    assert_eq!(host.work().snapshot().pick.offers, None);
}

#[test]
fn a_history_asked_for_is_read_and_reaches_the_window_whatever_it_shows() {
    let mut host = Host::default();
    let mut asked = host.take_histories().expect("taken once");
    let ask = Request::History {
        asked: 1,
        agent: Agent::Claude,
        home: None,
        project: "/w".into(),
    };
    assert!(host.on(ask).is_empty());
    assert_eq!(
        asked.try_recv().ok().map(|a| (a.window, a.asked)),
        Some((LONE, 1))
    );
    let read = PromptChange::History {
        read: adroit_protocol::HistoryRead {
            asked: 1,
            project: "/w".into(),
            sent: vec!["fix".into()],
        },
    };
    assert_eq!(
        patched(host.record_for(LONE, [read.clone()]).map(|(_, e)| e)),
        vec![Change::Prompt(read)]
    );
}

#[test]
fn the_worktree_in_the_middle_is_read_while_it_is_there_and_left_as_it_goes() {
    let mut host = Host::default();
    let read = |worktree: &str| PlaceChange::Read {
        read: adroit_protocol::PlaceRead {
            worktree: adroit_protocol::HostPath::local(worktree),
            files: vec!["a.rs".into()],
            jj: false,
        },
    };
    let middle = |worktree: Option<&str>| {
        Request::Watch(Box::new(Sight {
            place: worktree.map(adroit_protocol::HostPath::local),
            ..Default::default()
        }))
    };
    assert!(patched(host.read([read("/w")])).is_empty());
    host.on(middle(Some("/w")));
    assert!(patched(host.read([read("/v")])).is_empty(), "not shown");
    assert_eq!(
        patched(host.read([read("/w")])),
        vec![Change::Place(read("/w"))]
    );
    assert!(patched(host.on(middle(Some("/w")))).is_empty());
    assert_eq!(
        patched(host.on(middle(Some("/v")))),
        vec![Change::Place(PlaceChange::Left)]
    );
    assert_eq!(host.work().snapshot().place.read, None);
}

#[test]
fn the_directories_the_picker_lists_are_listed_beside_the_tree_s() {
    let mut host = Host::default();
    host.on(picking(&["/home"]));
    // (read, what the window is sent)
    let cases = [
        (entries("/home"), vec![entries("/home")]),
        (entries("/w"), vec![]),
    ];
    for (read, want) in cases {
        let sent = host.read([read.clone()]).into_iter().collect();
        assert_eq!(tree_changes(sent), want, "{read:?}");
    }
    assert_eq!(
        tree_changes(host.on(watch(None, None))),
        vec![TreeChange::EntriesLeft {
            dir: "/home".into()
        }]
    );
}

#[test]
fn the_configuration_read_reaches_every_window_once_whatever_it_shows() {
    use adroit_protocol::{ConfigChange, ConfigFile, FileText};
    let read = |text: &str| ConfigChange::Settings {
        file: ConfigFile {
            path: "/c/settings.toml".into(),
            text: FileText::Read { text: text.into() },
        },
    };
    let configured = Work::configured(ConfigWork {
        settings: Some(match read("zoom = 1.25\n") {
            ConfigChange::Settings { file } => file,
            ConfigChange::Keys { .. } | ConfigChange::Configured { .. } => unreachable!(),
        }),
        keys: None,
        configured: None,
    });
    let mut host = Host::new(configured.clone());
    assert_eq!(
        host.on(hello(&[1])).last(),
        Some(&Event::Snapshot(Box::new(configured.snapshot())))
    );
    assert!(patched(host.read([read("zoom = 1.25\n")])).is_empty());
    assert_eq!(
        patched(host.read([read("zoom = 1.5\n")])),
        vec![Change::Config(read("zoom = 1.5\n"))]
    );
}
