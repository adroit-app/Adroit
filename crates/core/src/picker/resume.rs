//! The list of the conversations the agents kept: which directory's it
//! lists, and what choosing a row does.

use std::path::{Path, PathBuf};

use adroit_model::Past;
use adroit_protocol::{PastSight, WorkSnapshot};

use super::fuzzy::{Query, best};
use super::{close, short};
use crate::content::{self, Tab};
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, nav};

/// The most conversations the list draws at once.
const ROWS: usize = 200;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resume {
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

impl Resume {
    /// Which conversations the host is asked to list.
    pub fn sight(&self) -> PastSight {
        PastSight {
            dir: self.dir.clone().filter(|_| !self.every),
        }
    }

    /// List every directory's conversations, or the directory's again; with
    /// no directory, every directory's stay listed.
    pub(crate) fn toggle_every(&mut self) {
        if self.dir.is_some() {
            self.every = !self.every;
            self.cursor = 0;
        }
    }
}

/// The conversations listed for what the list shows, those its query
/// answers to, best first; none while they are being read. Each is matched
/// by its title and the directory it ran in as drawn, `home` written as `~`.
pub fn rows(list: &Resume, home: &Path, work: Option<&WorkSnapshot>) -> Option<Vec<Row>> {
    let pasts = work?.past.of(&list.sight())?;
    let query = Query::new(&list.query);
    let matched = best(
        pasts.iter().filter_map(|p| {
            let text = format!("{} {}", p.title, short(&p.cwd, home));
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

/// The list opened on the directory the focused window works in: the one
/// its session runs in, or else its worktree; every directory's with
/// nothing open.
pub(super) fn open(state: &ViewState) -> Resume {
    let showing = state.content.showing();
    let work = state.host.work.as_ref();
    let session_dir = match &showing.tab {
        Some(Tab::Session(id)) => nav::find(tree::listed(work), id).map(|(_, s)| s.cwd.clone()),
        _ => None,
    };
    let dir = session_dir.or_else(|| showing.place.map(|p| p.worktree));
    Resume {
        every: dir.is_none(),
        dir,
        ..Resume::default()
    }
}

/// The row `at` chosen: the session that holds its conversation shown, or
/// else the conversation resumed in a new session. The list closes either
/// way.
pub(super) fn choose(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let view = &state.picker;
    let chosen = match &view.open {
        Some(super::Picking::Resume(list)) => rows(list, &view.home, state.host.work.as_ref())
            .and_then(|mut r| (at < r.len()).then(|| r.swap_remove(at))),
        _ => None,
    };
    let Some(row) = chosen else {
        return Vec::new();
    };
    close(&mut state.picker);
    match row.session {
        Some(id) => content::show_session(state, id),
        None => vec![Effect::Resume(row.past)],
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::{Agent, Key};
    use adroit_protocol::{PastChange, Request};

    use super::*;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::testing::{listing, session, session_tab};

    const HOME: &str = "/Users/me";

    fn window() -> Scenario {
        Scenario::new().send(Msg::Opened { home: HOME.into() })
    }

    /// The rows the list draws; none while they are read.
    fn drawn(run: &Scenario) -> Option<Vec<Row>> {
        let view = &run.state().picker;
        match &view.open {
            Some(super::super::Picking::Resume(list)) => {
                rows(list, &view.home, run.state().host.work.as_ref())
            }
            _ => None,
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

    /// The conversations read for `dir`, or for every directory.
    fn pasts(dir: Option<&str>, conversations: Vec<Past>) -> PastChange {
        PastChange::Listed {
            sight: PastSight {
                dir: dir.map(Into::into),
            },
            conversations,
        }
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
        Msg::PickerKey(Key {
            ctrl,
            ..Key::new(name)
        })
    }

    fn titles(run: &Scenario) -> Vec<String> {
        drawn(run)
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.past.title)
            .collect()
    }

    /// A window showing the session `a`, which runs in `/r/src`.
    fn showing_a() -> Scenario {
        window()
            .event(listing([session("a").cwd("/r/src").session_id("conv-a")]))
            .send(Msg::ShowSession("a".into()))
    }

    #[test]
    fn the_list_asks_for_the_conversations_of_where_the_focused_window_works() {
        let run = showing_a().send(Msg::ResumeOpen);
        assert_eq!(watched(&run), here("/r/src"));
    }

    #[test]
    fn with_nothing_open_every_directory_s_conversations_are_listed() {
        let run = window().send(Msg::ResumeOpen);
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
        for close in [key("Escape", false), key("c", true), Msg::PickerClose] {
            let run = showing_a().send(Msg::ResumeOpen).send(close.clone());
            assert_eq!(watched(&run), Some(None), "{close:?}");
            assert!(run.state().picker.open.is_none());
        }
    }

    #[test]
    fn nothing_is_drawn_until_the_list_asked_for_is_read() {
        let run = showing_a()
            .send(Msg::ResumeOpen)
            .tell(pasts(None, vec![past(Agent::Claude, "x", "/q", "other")]));
        assert_eq!(drawn(&run), None);
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
                .tell(pasts(Some("/r/src"), listed.clone()))
                .send(Msg::PickerQuery(typed.into()));
            assert_eq!(titles(&run), want, "{typed}");
        }
    }

    #[test]
    fn a_directory_is_matched_as_drawn_with_home_as_a_tilde() {
        let listed = vec![
            past(Agent::Claude, "1", "/Users/me/w", "one"),
            past(Agent::Claude, "2", "/srv/w", "two"),
        ];
        let cases = [("~/w", vec!["one"]), ("Users", vec![])];
        for (typed, want) in cases {
            let run = showing_a()
                .send(Msg::ResumeOpen)
                .tell(pasts(Some("/r/src"), listed.clone()))
                .send(Msg::PickerQuery(typed.into()));
            assert_eq!(titles(&run), want, "{typed}");
        }
    }

    #[test]
    fn choosing_a_conversation_no_session_holds_resumes_it_and_closes_the_list() {
        let chosen = past(Agent::Codex, "019f", "/r/src", "review the API");
        let run = showing_a()
            .send(Msg::ResumeOpen)
            .tell(pasts(
                Some("/r/src"),
                vec![past(Agent::Claude, "1", "/r/src", "fix"), chosen.clone()],
            ))
            .send(key("ArrowDown", false))
            .send(key("Enter", false));
        assert!(run.last_effects().contains(&Effect::Resume(chosen)));
        assert!(run.state().picker.open.is_none());
    }

    #[test]
    fn choosing_a_conversation_a_session_here_holds_shows_that_session() {
        let run = window()
            .event(listing([
                session("a").cwd("/r/src").session_id("conv-a"),
                session("b").cwd("/r/src").session_id("conv-b"),
            ]))
            .send(Msg::ShowSession("a".into()))
            .send(Msg::ResumeOpen)
            .tell(pasts(
                Some("/r/src"),
                vec![past(Agent::Claude, "conv-b", "/r/src", "the other")],
            ));
        let got = drawn(&run).unwrap();
        assert_eq!(got[0].session.as_deref(), Some("b"));
        let run = run.send(Msg::PickerChoose(0));
        assert!(
            !run.last_effects()
                .iter()
                .any(|e| matches!(e, Effect::Resume(_)))
        );
        assert_eq!(run.state().content.showing().tab, Some(session_tab("b")));
    }

    #[test]
    fn the_same_id_from_another_agent_is_not_the_session_s() {
        let run = showing_a().send(Msg::ResumeOpen).tell(pasts(
            Some("/r/src"),
            vec![past(Agent::Codex, "conv-a", "/r/src", "codex")],
        ));
        let got = drawn(&run).unwrap();
        assert_eq!(got[0].session, None);
    }

    #[test]
    fn the_cursor_goes_round_the_rows_and_a_row_past_them_chooses_nothing() {
        let run = showing_a().send(Msg::ResumeOpen).tell(pasts(
            Some("/r/src"),
            vec![
                past(Agent::Claude, "1", "/r/src", "one"),
                past(Agent::Claude, "2", "/r/src", "two"),
            ],
        ));
        let up = run.send(key("p", true));
        assert_eq!(up.state().picker.open.as_ref().unwrap().cursor(), 1);
        let round = up.send(key("n", true));
        assert_eq!(round.state().picker.open.as_ref().unwrap().cursor(), 0);
        let nothing = round.send(Msg::PickerChoose(5));
        assert!(nothing.last_effects().is_empty());
        assert!(nothing.state().picker.open.is_some());
    }
}
