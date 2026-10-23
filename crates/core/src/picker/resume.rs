//! The list of the conversations the agents kept: which directory's it
//! lists, what is typed to narrow it, the row the cursor is on, and what
//! choosing a row does.

use std::path::PathBuf;

use adroit_model::{Key, Past};
use adroit_protocol::{PastSight, WorkSnapshot};

use super::fuzzy::{Query, best};
use crate::content::{self, Tab};
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, nav};

/// The most conversations the list draws at once.
const ROWS: usize = 200;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResumeView {
    pub open: bool,
    /// The directory the focused window worked in when the list opened.
    pub dir: Option<PathBuf>,
    /// Whether every directory's conversations are listed, rather than the
    /// directory's.
    pub every: bool,
    pub query: String,
    pub cursor: usize,
}

/// A conversation the list draws, with the session of this window that
/// holds it, if one does.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub past: Past,
    pub session: Option<String>,
}

impl ResumeView {
    /// Which conversations the host is asked to list while the list is open.
    pub fn sight(&self) -> Option<PastSight> {
        self.open.then(|| PastSight {
            dir: self.dir.clone().filter(|_| !self.every),
        })
    }
}

/// The conversations listed for what the list shows, those `query` answers
/// to, best first; none while they are being read.
pub fn rows(view: &ResumeView, work: Option<&WorkSnapshot>) -> Option<Vec<Row>> {
    let sight = view.sight()?;
    let pasts = work?.past.of(&sight)?;
    let query = Query::new(&view.query);
    let matched = best(
        pasts.iter().filter_map(|p| {
            let text = format!("{} {}", p.title, p.cwd.display());
            query.rank_named(&text, &p.title).map(|rank| (rank, p))
        }),
        ROWS,
    );
    let sessions = work.map(|w| w.tree.sessions()).unwrap_or_default();
    Some(
        matched
            .into_iter()
            .map(|p| Row {
                past: p.clone(),
                session: sessions
                    .iter()
                    .find(|s| s.agent == p.agent && s.session_id == p.id)
                    .map(|s| s.id.clone()),
            })
            .collect(),
    )
}

/// Open the list on the directory the focused window works in: the one its
/// session runs in, or else its worktree; every directory's with nothing
/// open.
pub(crate) fn open(state: &mut ViewState) -> Vec<Effect> {
    let showing = state.content.showing();
    let work = state.host.work.as_ref();
    let session_dir = match &showing.tab {
        Some(Tab::Session(id)) => nav::find(tree::listed(work), id).map(|(_, s)| s.cwd.clone()),
        _ => None,
    };
    let dir = session_dir.or_else(|| showing.place.map(|p| p.worktree));
    state.resume = ResumeView {
        open: true,
        every: dir.is_none(),
        dir,
        ..ResumeView::default()
    };
    Vec::new()
}

pub(crate) fn close(view: &mut ResumeView) -> Vec<Effect> {
    *view = ResumeView::default();
    Vec::new()
}

pub(crate) fn query(view: &mut ResumeView, text: String) -> Vec<Effect> {
    view.query = text;
    view.cursor = 0;
    Vec::new()
}

/// List every directory's conversations, or the directory's again; with no
/// directory, every directory's stay listed.
pub(crate) fn toggle_every(view: &mut ResumeView) -> Vec<Effect> {
    if view.dir.is_some() {
        view.every = !view.every;
        view.cursor = 0;
    }
    Vec::new()
}

/// A key the list's query field does not type with.
pub(crate) fn press(state: &mut ViewState, key: &Key) -> Vec<Effect> {
    let plain = !key.ctrl && !key.alt && !key.meta;
    let ctrl = key.ctrl && !key.alt && !key.meta;
    let delta = match key.key.as_str() {
        "Escape" if plain => return close(&mut state.resume),
        "c" if ctrl => return close(&mut state.resume),
        "a" if ctrl => return toggle_every(&mut state.resume),
        "Enter" if plain => {
            let at = state.resume.cursor;
            return choose(state, at);
        }
        "ArrowDown" | "Tab" if plain && !key.shift => 1,
        "Tab" if plain => -1,
        "n" | "j" if ctrl => 1,
        "ArrowUp" if plain => -1,
        "p" | "k" if ctrl => -1,
        _ => return Vec::new(),
    };
    let len = rows(&state.resume, state.host.work.as_ref()).map_or(0, |r| r.len());
    state.resume.cursor = crate::picker::step(state.resume.cursor, len, delta);
    Vec::new()
}

/// The row `at` chosen: the session that holds its conversation shown, or
/// else the conversation resumed in a new session. The list closes either
/// way.
pub(crate) fn choose(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let chosen = rows(&state.resume, state.host.work.as_ref())
        .and_then(|mut r| (at < r.len()).then(|| r.swap_remove(at)));
    let Some(row) = chosen else {
        return Vec::new();
    };
    close(&mut state.resume);
    match row.session {
        Some(id) => content::show_session(state, id),
        None => vec![Effect::Resume(row.past)],
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::testing::worktree as wt;
    use adroit_model::{Agent, Session, build};
    use adroit_protocol::{Change, Event, Listed, PastChange, Request, Seq, TreeWork, WorkPatch};

    use super::*;
    use crate::msg::Msg;
    use crate::scenario::Scenario;

    fn session(id: &str, cwd: &str, conversation: &str) -> Session {
        Session {
            id: id.into(),
            name: format!("{id}-name"),
            cwd: cwd.into(),
            session_id: conversation.into(),
            ..Default::default()
        }
    }

    fn past(agent: Agent, id: &str, cwd: &str, title: &str) -> Past {
        Past {
            agent,
            id: id.into(),
            cwd: cwd.into(),
            title: title.into(),
            at_ms: 0,
        }
    }

    fn listing(sessions: Vec<Session>) -> Event {
        let found = sessions
            .iter()
            .map(|s| (s.cwd.clone(), Some(wt(s.cwd.clone(), "/r", "main"))))
            .collect();
        let tree = build(&sessions, &[], &found, &[], None);
        Event::Snapshot(Box::new(WorkSnapshot {
            seq: Seq(1),
            tree: TreeWork {
                listed: Some(Box::new(Listed {
                    sessions,
                    terminals: Vec::new(),
                    tree,
                })),
                ..Default::default()
            },
            ..Default::default()
        }))
    }

    fn pasts(dir: Option<&str>, conversations: Vec<Past>) -> Event {
        Event::Patch(WorkPatch {
            seq: Seq(2),
            changes: vec![Change::Past(PastChange::Listed {
                sight: PastSight {
                    dir: dir.map(Into::into),
                },
                conversations,
            })],
        })
    }

    fn watched(run: &Scenario) -> Option<Option<PastSight>> {
        run.effects()
            .iter()
            .filter_map(|e| match e {
                Effect::Host(Request::Watch(sight)) => Some(sight.past.clone()),
                _ => None,
            })
            .next_back()
    }

    fn here(dir: &str) -> Option<Option<PastSight>> {
        Some(Some(PastSight {
            dir: Some(dir.into()),
        }))
    }

    fn every() -> Option<Option<PastSight>> {
        Some(Some(PastSight { dir: None }))
    }

    fn key(name: &str, ctrl: bool) -> Msg {
        Msg::ResumeKey(Key {
            ctrl,
            ..Key::new(name)
        })
    }

    fn titles(run: &Scenario) -> Vec<String> {
        rows(&run.state().resume, run.state().host.work.as_ref())
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.past.title)
            .collect()
    }

    /// A window showing the session `a`, which runs in `/r/src`.
    fn showing_a() -> Scenario {
        Scenario::new()
            .event(listing(vec![session("a", "/r/src", "conv-a")]))
            .send(Msg::ShowSession("a".into()))
    }

    #[test]
    fn the_list_asks_for_the_conversations_of_where_the_focused_window_works() {
        let run = showing_a().send(Msg::ResumeOpen);
        assert_eq!(watched(&run), here("/r/src"));
    }

    #[test]
    fn with_nothing_open_every_directory_s_conversations_are_listed() {
        let run = Scenario::new().send(Msg::ResumeOpen);
        assert_eq!(watched(&run), every());
        // There is no directory's to go back to.
        let run = run.send(key("a", true));
        assert_eq!(watched(&run), every());
    }

    #[test]
    fn ctrl_a_lists_every_directory_s_and_again_the_directory_s() {
        let run = showing_a().send(Msg::ResumeOpen).send(key("a", true));
        assert_eq!(watched(&run), every());
        let run = run.send(key("a", true));
        assert_eq!(watched(&run), here("/r/src"));
    }

    #[test]
    fn closing_the_list_stops_the_host_listing() {
        for close in [key("Escape", false), key("c", true), Msg::ResumeClose] {
            let run = showing_a().send(Msg::ResumeOpen).send(close.clone());
            assert_eq!(watched(&run), Some(None), "{close:?}");
            assert!(!run.state().resume.open);
        }
    }

    #[test]
    fn nothing_is_drawn_until_the_list_asked_for_is_read() {
        let run = showing_a()
            .send(Msg::ResumeOpen)
            .event(pasts(None, vec![past(Agent::Claude, "x", "/q", "other")]));
        assert_eq!(
            rows(&run.state().resume, run.state().host.work.as_ref()),
            None
        );
    }

    #[test]
    fn typing_narrows_the_list_by_title_and_directory() {
        let listed = vec![
            past(Agent::Claude, "1", "/r/src", "fix the login"),
            past(Agent::Codex, "2", "/r/src", "review the API"),
            past(Agent::Claude, "3", "/r/src/docs", "write the guide"),
        ];
        let cases = [
            (
                "",
                vec!["fix the login", "review the API", "write the guide"],
            ),
            ("login", vec!["fix the login"]),
            ("docs", vec!["write the guide"]),
        ];
        for (typed, want) in cases {
            let run = showing_a()
                .send(Msg::ResumeOpen)
                .event(pasts(Some("/r/src"), listed.clone()))
                .send(Msg::ResumeQuery(typed.into()));
            assert_eq!(titles(&run), want, "{typed}");
        }
    }

    #[test]
    fn choosing_a_conversation_no_session_holds_resumes_it_and_closes_the_list() {
        let chosen = past(Agent::Codex, "019f", "/r/src", "review the API");
        let run = showing_a()
            .send(Msg::ResumeOpen)
            .event(pasts(
                Some("/r/src"),
                vec![past(Agent::Claude, "1", "/r/src", "fix"), chosen.clone()],
            ))
            .send(key("ArrowDown", false))
            .send(key("Enter", false));
        assert!(run.last_effects().contains(&Effect::Resume(chosen)));
        assert!(!run.state().resume.open);
    }

    #[test]
    fn choosing_a_conversation_a_session_here_holds_shows_that_session() {
        let run = Scenario::new()
            .event(listing(vec![
                session("a", "/r/src", "conv-a"),
                session("b", "/r/src", "conv-b"),
            ]))
            .send(Msg::ShowSession("a".into()))
            .send(Msg::ResumeOpen)
            .event(pasts(
                Some("/r/src"),
                vec![past(Agent::Claude, "conv-b", "/r/src", "the other")],
            ));
        let got = rows(&run.state().resume, run.state().host.work.as_ref()).unwrap();
        assert_eq!(got[0].session.as_deref(), Some("b"));
        let run = run.send(Msg::ResumeChoose(0));
        assert!(
            !run.last_effects()
                .iter()
                .any(|e| matches!(e, Effect::Resume(_)))
        );
        assert_eq!(
            run.state().content.showing().tab,
            Some(Tab::Session("b".into()))
        );
    }

    #[test]
    fn the_same_id_from_another_agent_is_not_the_session_s() {
        let run = showing_a().send(Msg::ResumeOpen).event(pasts(
            Some("/r/src"),
            vec![past(Agent::Codex, "conv-a", "/r/src", "codex")],
        ));
        let got = rows(&run.state().resume, run.state().host.work.as_ref()).unwrap();
        assert_eq!(got[0].session, None);
    }

    #[test]
    fn the_cursor_goes_round_the_rows_and_a_row_past_them_chooses_nothing() {
        let run = showing_a().send(Msg::ResumeOpen).event(pasts(
            Some("/r/src"),
            vec![
                past(Agent::Claude, "1", "/r/src", "one"),
                past(Agent::Claude, "2", "/r/src", "two"),
            ],
        ));
        let up = run.send(key("p", true));
        assert_eq!(up.state().resume.cursor, 1);
        let round = up.send(key("n", true));
        assert_eq!(round.state().resume.cursor, 0);
        let nothing = round.send(Msg::ResumeChoose(5));
        assert!(nothing.last_effects().is_empty());
        assert!(nothing.state().resume.open);
    }
}
