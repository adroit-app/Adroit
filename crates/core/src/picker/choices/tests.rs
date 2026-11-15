use adroit_model::testing::{self, session};
use adroit_model::{State, Wait};

use super::*;
use crate::completion::Viewed;

fn paths(ps: &[&str]) -> Vec<PathBuf> {
    ps.iter().map(PathBuf::from).collect()
}

const HOME: &str = "/Users/me";

fn matching(cs: &[Candidate], query: &str, cap: usize) -> Vec<Candidate> {
    super::matching(cs, query, Path::new(HOME), cap)
}

fn listed(cs: &[Candidate]) -> Vec<(&str, Source)> {
    cs.iter()
        .map(|c| (c.path.to_str().unwrap(), c.source))
        .collect()
}

#[test]
fn the_sources_are_listed_running_then_recent_then_ghq_each_in_its_own_order() {
    let cs = candidates(
        &EnvironmentId::default(),
        &paths(&["/r2", "/r1"]),
        &paths(&["/c"]),
        &paths(&["/g2", "/g1"]),
        &[],
    );
    assert_eq!(
        listed(&cs),
        vec![
            ("/r2", Source::Tree),
            ("/r1", Source::Tree),
            ("/c", Source::Recent),
            ("/g2", Source::Ghq),
            ("/g1", Source::Ghq),
        ]
    );
}

#[test]
fn a_path_offered_twice_is_listed_once_under_the_first_source_that_offers_it() {
    let cs = candidates(
        &EnvironmentId::default(),
        &paths(&["/a"]),
        &paths(&["/b", "/a", "/b"]),
        &paths(&["/a", "/b", "/g"]),
        &[],
    );
    assert_eq!(
        listed(&cs),
        vec![
            ("/a", Source::Tree),
            ("/b", Source::Recent),
            ("/g", Source::Ghq),
        ]
    );
}

#[test]
fn nothing_typed_keeps_the_list_in_order_up_to_the_cap() {
    let cs = candidates(
        &EnvironmentId::default(),
        &[],
        &paths(&["/z/long/path", "/a", "/m"]),
        &[],
        &[],
    );
    assert_eq!(
        listed(&matching(&cs, "", 2)),
        vec![("/z/long/path", Source::Recent), ("/a", Source::Recent)]
    );
}

#[test]
fn a_query_keeps_what_matches_best_first() {
    let cs = candidates(
        &EnvironmentId::default(),
        &[],
        &paths(&["/w/adroit/docs", "/w/arto", "/w/src/adroit"]),
        &[],
        &[],
    );
    let found = matching(&cs, "adroit", 10);
    assert_eq!(
        listed(&found),
        vec![
            ("/w/src/adroit", Source::Recent),
            ("/w/adroit/docs", Source::Recent),
        ]
    );
}

#[test]
fn folders_under_the_listed_roots_come_last_and_once() {
    let cs = candidates(
        &EnvironmentId::default(),
        &paths(&["/a"]),
        &[],
        &paths(&["/g"]),
        &paths(&["/g", "/l"]),
    );
    assert_eq!(
        cs.iter()
            .map(|c| (c.path.to_str().unwrap(), c.source))
            .collect::<Vec<_>>(),
        vec![
            ("/a", Source::Tree),
            ("/g", Source::Ghq),
            ("/l", Source::Listed)
        ]
    );
}

#[test]
fn candidates_that_answer_equally_keep_the_order_they_were_listed_in() {
    let cs = candidates(
        &EnvironmentId::default(),
        &paths(&["/b/x"]),
        &paths(&["/a/x"]),
        &[],
        &[],
    );
    assert_eq!(
        listed(&matching(&cs, "x", 10)),
        vec![("/b/x", Source::Tree), ("/a/x", Source::Recent)]
    );
}

#[test]
fn a_query_is_matched_against_the_path_as_drawn_with_home_as_a_tilde() {
    let cs = candidates(
        &EnvironmentId::default(),
        &[],
        &paths(&["/Users/me/src/adroit", "/opt/adroit"]),
        &[],
        &[],
    );
    assert_eq!(
        listed(&matching(&cs, "^~/src", 10)),
        vec![("/Users/me/src/adroit", Source::Recent)]
    );
    // The home directory's own name is not in what is drawn.
    assert!(matching(&cs, "me/", 10).is_empty());
}

const OAUTH: &str = "/w/api/.claude/worktrees/oauth";

/// `api` runs a session in its main checkout and one in a worktree; `tmp`
/// is outside git.
fn tree() -> Tree {
    testing::tree()
        .session(
            session("a")
                .name("fix login")
                .cwd("/w/api")
                .state(State::Working),
        )
        .session(
            session("o")
                .name("add tokens")
                .cwd(OAUTH)
                .state(State::Waiting(Wait::Input)),
        )
        .session(session("z").name("scratch").cwd("/tmp"))
        .checkout("/w/api", "/w/api", "main")
        .checkout(OAUTH, "/w/api", "oauth")
        .found("/tmp", None)
        .build()
}

fn offered(found: &[SessionOffer]) -> Vec<(&str, &str, &str)> {
    found
        .iter()
        .map(|s| (s.id.as_str(), s.place.as_str(), s.state))
        .collect()
}

#[test]
fn every_session_is_offered_in_the_sidebar_s_order_with_where_it_runs_and_how_it_stands() {
    assert_eq!(
        offered(&sessions(
            &EnvironmentId::default(),
            &tree(),
            &Completions::default(),
            &Changes::default()
        )),
        vec![
            ("a", "api › main", "working"),
            ("o", "api › oauth", "waiting"),
            ("z", "tmp", "idle"),
        ]
    );
}

#[test]
fn a_session_is_found_by_its_name_or_by_where_it_runs() {
    let all = sessions(
        &EnvironmentId::default(),
        &tree(),
        &Completions::default(),
        &Changes::default(),
    );
    let ids = |q: &str| -> Vec<String> {
        matching_sessions(&all, q, 10)
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    assert_eq!(ids("login"), ["a"]);
    assert_eq!(ids("oauth"), ["o"]);
    assert_eq!(ids("api"), ["a", "o"]);
    assert_eq!(ids(""), ["o", "a", "z"], "the one waiting first");
    assert!(ids("nothing").is_empty());
}

#[test]
fn a_session_named_by_the_query_comes_before_one_that_only_runs_where_it_says() {
    let t = testing::tree()
        .session(session("x").name("docs").cwd("/w/scratch"))
        .session(session("s").name("scratch").cwd("/w/docs"))
        .build();
    let ids: Vec<String> = matching_sessions(
        &sessions(
            &EnvironmentId::default(),
            &t,
            &Completions::default(),
            &Changes::default(),
        ),
        "scratch",
        10,
    )
    .into_iter()
    .map(|s| s.id)
    .collect();
    assert_eq!(ids, ["s", "x"]);
}

fn dirs(ps: &[&str]) -> Vec<Candidate> {
    candidates(&EnvironmentId::default(), &[], &paths(ps), &[], &[])
}

fn chosen(found: &[Choice]) -> Vec<String> {
    found
        .iter()
        .map(|c| match c {
            Choice::Scratch => "scratch".into(),
            Choice::Session(s) => s.id.clone(),
            Choice::Directory(d) => d.path.display().to_string(),
            Choice::Clone(c) => format!("clone {}", c.target.path),
            Choice::Owner(o) => format!("owner {o}"),
            Choice::GitHubHelp => "help".into(),
        })
        .collect()
}

#[test]
fn the_sessions_come_before_the_directories_each_ranked_among_its_own() {
    let all = sessions(
        &EnvironmentId::default(),
        &tree(),
        &Completions::default(),
        &Changes::default(),
    );
    let ds = dirs(&["/w/x/api", "/w/api"]);
    let pick = |q: &str| chosen(&choices(&all, &ds, q, Path::new(HOME), 10));
    assert_eq!(pick(""), ["scratch", "o", "a", "z", "/w/x/api", "/w/api"]);
    // A directory answering better than a session still comes after it.
    assert_eq!(pick("api"), ["a", "o", "/w/api", "/w/x/api"]);
    assert_eq!(pick("login"), ["a"]);
    assert_eq!(pick("x/api"), ["/w/x/api"]);
}

#[test]
fn each_group_keeps_no_more_than_the_cap() {
    let all = sessions(
        &EnvironmentId::default(),
        &tree(),
        &Completions::default(),
        &Changes::default(),
    );
    let ds = dirs(&["/a", "/b", "/c"]);
    let found = choices(&all, &ds, "", Path::new(HOME), 2);
    assert_eq!(chosen(&found), ["scratch", "o", "a", "/a", "/b"]);
}

#[test]
fn a_scratch_session_is_offered_above_everything_while_the_query_answers_to_it() {
    let all = sessions(
        &EnvironmentId::default(),
        &tree(),
        &Completions::default(),
        &Changes::default(),
    );
    let pick = |q: &str| chosen(&choices(&all, &[], q, Path::new(HOME), 10));
    assert_eq!(pick("")[0], "scratch");
    // The session named scratch answers too, after it.
    assert_eq!(pick("scratch"), ["scratch", "z"]);
    assert_eq!(pick("new ses"), ["scratch"]);
    assert!(!pick("login").contains(&"scratch".to_string()));
}

#[test]
fn each_group_is_headed_once_above_its_first_row() {
    let all = sessions(
        &EnvironmentId::default(),
        &tree(),
        &Completions::default(),
        &Changes::default(),
    );
    let ds = dirs(&["/a", "/b"]);
    let headings = |found: &[Choice]| -> Vec<Option<&str>> {
        lines(found, Path::new(HOME))
            .into_iter()
            .map(|l| l.heading)
            .collect()
    };
    let both = choices(&all, &ds, "", Path::new(HOME), 10);
    // The scratch row stands above the groups, under no heading.
    assert_eq!(
        headings(&both),
        [None, Some("Sessions"), None, None, Some("Start in"), None]
    );
    // A group nothing in answers has no heading.
    let only_dirs = choices(&all, &ds, "/a", Path::new(HOME), 10);
    assert_eq!(headings(&only_dirs), [Some("Start in")]);
}

#[test]
fn a_row_draws_a_directory_by_its_short_path_a_session_by_its_name_and_place_and_the_scratch_row_by_what_it_starts()
 {
    let all = sessions(
        &EnvironmentId::default(),
        &tree(),
        &Completions::default(),
        &Changes::default(),
    );
    let found = [
        Choice::Scratch,
        Choice::Session(all[1].clone()),
        Choice::Directory(Candidate {
            environment: EnvironmentId::default(),
            path: "/Users/me/src/x".into(),
            source: Source::Ghq,
        }),
    ];
    assert_eq!(
        lines(&found, Path::new(HOME)),
        [
            Line {
                heading: None,
                name: SCRATCH.into(),
                place: None,
                badge: String::new(),
                mark: Mark::Scratch,
            },
            Line {
                heading: Some("Sessions"),
                name: "add tokens".into(),
                place: Some("api › oauth".into()),
                badge: "waiting".into(),
                mark: Mark::Session,
            },
            Line {
                heading: Some("Start in"),
                name: "~/src/x".into(),
                place: None,
                badge: "ghq".into(),
                mark: Mark::Directory,
            },
        ]
    );
}

fn cloning(ghq: &[&str]) -> Offers {
    Offers {
        clones: true,
        ghq: paths(ghq),
        ..Offers::default()
    }
}

#[test]
fn a_repository_typed_is_offered_to_clone_where_the_host_clones() {
    let here = EnvironmentId::default();
    let offered = clone_offer(&here, &cloning(&[]), "git@github.com:o/r").unwrap();
    assert_eq!(offered.target.path, "github.com/o/r");
    assert_eq!(offered.target.typed, "git@github.com:o/r");
    assert_eq!(clone_offer(&here, &cloning(&[]), "api"), None);
    assert_eq!(clone_offer(&here, &cloning(&[]), "o/r"), None);
    assert_eq!(
        clone_offer(&here, &Offers::default(), "github.com/o/r"),
        None
    );
}

#[test]
fn a_repository_ghq_keeps_already_is_not_offered_to_clone() {
    let kept = cloning(&["/g/github.com/O/R"]);
    assert_eq!(
        clone_offer(
            &EnvironmentId::default(),
            &kept,
            "https://github.com/o/r.git"
        ),
        None
    );
}

#[test]
fn a_clone_row_draws_the_repository_s_place_under_a_heading_of_its_own() {
    let target = adroit_model::GitHubTyped::target("o", "r");
    let found = [Choice::Clone(CloneOffer {
        environment: EnvironmentId::default(),
        target,
    })];
    assert_eq!(
        lines(&found, Path::new(HOME)),
        [Line {
            heading: Some("Clone with ghq"),
            name: "github.com/o/r".into(),
            place: None,
            badge: "ghq".into(),
            mark: Mark::Clone,
        }]
    );
}

#[test]
fn an_owner_row_draws_the_owner_and_the_help_row_what_to_type() {
    let found = [Choice::Owner("o".into()), Choice::GitHubHelp];
    assert_eq!(
        lines(&found, Path::new(HOME)),
        [
            Line {
                heading: Some("GitHub owners"),
                name: "o".into(),
                place: None,
                badge: "gh".into(),
                mark: Mark::Owner,
            },
            Line {
                heading: None,
                name: GITHUB_HELP.into(),
                place: None,
                badge: String::new(),
                mark: Mark::Help,
            },
        ]
    );
}

fn offer(id: &str, state: &'static str, changed: u64) -> SessionOffer {
    SessionOffer {
        environment: EnvironmentId::default(),
        id: id.into(),
        name: format!("{id} name"),
        place: "api".into(),
        state,
        changed,
    }
}

fn ids_for(offers: &[SessionOffer], query: &str) -> Vec<String> {
    matching_sessions(offers, query, 10)
        .into_iter()
        .map(|s| s.id)
        .collect()
}

#[test]
fn with_nothing_typed_the_sessions_stand_by_what_needs_the_user_the_latest_change_first() {
    let offers = [
        offer("idle", "idle", 9),
        offer("working-old", "working", 1),
        offer("done", "done", 0),
        offer("working-new", "working", 5),
        offer("starting", "starting", 8),
        offer("waiting-unseen", "waiting", 0),
        offer("failed", "failed", 2),
        offer("waiting-new", "waiting", 7),
        offer("waiting-unseen-too", "waiting", 0),
    ];
    assert_eq!(
        ids_for(&offers, ""),
        [
            "waiting-new",
            "waiting-unseen",
            "waiting-unseen-too",
            "done",
            "failed",
            "working-new",
            "working-old",
            "starting",
            "idle",
        ],
        "sessions never seen to change keep the sidebar's order among their own"
    );
}

#[test]
fn a_query_ranks_by_how_well_each_answers_however_it_stands() {
    let mut offers = [offer("a", "idle", 0), offer("b", "waiting", 3)];
    offers[0].name = "login".into();
    offers[1].name = "the slow logging in".into();
    assert_eq!(ids_for(&offers, "login"), ["a", "b"]);
}

#[test]
fn a_term_at_a_state_keeps_the_sessions_in_that_state() {
    let offers = [
        offer("i", "idle", 0),
        offer("w", "waiting", 0),
        offer("r", "working", 0),
        offer("d", "done", 0),
    ];
    for (query, kept) in [
        ("@w", &["w"][..]),
        ("@waiting", &["w"]),
        ("@wo", &["r"]),
        ("@d", &["d"]),
        ("@i", &["i"]),
        ("@W", &["w"]),
        // Two state terms keep either.
        ("@i @w", &["w", "i"]),
        // A term no state's word starts with keeps none.
        ("@x", &[]),
        // `@` alone names no state in particular.
        ("@", &["w", "d", "r", "i"]),
        // The rest is matched as before.
        ("@w w", &["w"]),
        ("@w nothing", &[]),
    ] {
        assert_eq!(ids_for(&offers, query), kept, "{query:?}");
    }
}

#[test]
fn a_query_naming_a_state_offers_only_sessions() {
    let all = sessions(
        &EnvironmentId::default(),
        &tree(),
        &Completions::default(),
        &Changes::default(),
    );
    let ds = dirs(&["/w/api"]);
    let pick = |q: &str| chosen(&choices(&all, &ds, q, Path::new(HOME), 10));
    assert_eq!(pick("@"), ["o", "a", "z"]);
    assert_eq!(pick("@wo api"), ["a"]);
    assert_eq!(pick("api"), ["a", "o", "/w/api"]);
}

#[test]
fn the_terms_naming_states_are_taken_out_of_what_is_matched() {
    for (query, text, states) in [
        ("api", "api", None),
        ("@w api", "api", Some(vec!["waiting"])),
        ("fix  @d  login", "fix login", Some(vec!["done"])),
        ("@w @w", "", Some(vec!["waiting"])),
        ("@i @w", "", Some(vec!["waiting", "idle"])),
        // `@` inside a term is the term's own.
        ("me@host", "me@host", None),
    ] {
        assert_eq!(
            Narrowing::new(query),
            Narrowing {
                text: text.into(),
                states
            },
            "{query:?}"
        );
    }
}

#[test]
fn a_finished_session_the_user_has_seen_is_offered_as_idle() {
    let t = testing::tree()
        .session(session("d").cwd("/w/api").state(State::Done))
        .session(session("u").cwd("/w/api").state(State::Done))
        .build();
    let seen = Completions::default().updated(&t, Some(Viewed::Session("d")));
    let found = sessions(&EnvironmentId::default(), &t, &seen, &Changes::default());
    assert_eq!(
        found
            .iter()
            .map(|s| (s.id.as_str(), s.state))
            .collect::<Vec<_>>(),
        [("d", "idle"), ("u", "done")]
    );
}

#[test]
fn a_row_is_the_same_row_however_it_now_stands() {
    let was = Choice::Session(offer("a", "working", 1));
    assert!(was.same_row(&Choice::Session(offer("a", "waiting", 4))));
    assert!(!was.same_row(&Choice::Session(offer("b", "working", 1))));
    assert!(Choice::Scratch.same_row(&Choice::Scratch));
    assert!(!was.same_row(&Choice::Scratch));
}

#[test]
fn every_source_is_named_by_its_badge() {
    assert_eq!(Source::Tree.label(), "tree");
    assert_eq!(Source::Recent.label(), "recent");
    assert_eq!(Source::Ghq.label(), "ghq");
}
