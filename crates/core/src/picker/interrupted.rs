//! The list of what the terminal host took with it: the sessions and
//! terminals waiting to be brought back, and what choosing one does.

use std::path::Path;

use adroit_model::{EnvironmentId, Interrupted};

use super::fuzzy::{Query, best};
use super::{close, short};
use crate::effect::Effect;
use crate::host::Hosts;
use crate::state::ViewState;

/// The most the list draws at once.
const ROWS: usize = 200;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Waiting {
    /// The environment whose terminal host took them.
    pub environment: EnvironmentId,
    pub query: String,
    pub cursor: usize,
}

/// What waits in `environment` to be brought back, as its host lists it.
pub fn waiting<'a>(hosts: &'a Hosts, environment: &EnvironmentId) -> &'a [Interrupted] {
    crate::tree::interrupted(hosts.work(environment))
}

/// What waits that the list's query answers to, best first, each matched
/// by its name and its directory as drawn, `home` written as `~`.
pub fn rows<'a>(list: &Waiting, home: &Path, hosts: &'a Hosts) -> Vec<&'a Interrupted> {
    let query = Query::new(&list.query);
    best(
        waiting(hosts, &list.environment).iter().filter_map(|i| {
            let text = format!("{} {}", i.name, short(&i.cwd, home));
            query.rank_named(&text, &i.name).map(|rank| (rank, i))
        }),
        ROWS,
    )
}

/// The row `at` chosen: it is brought back, and the list closes.
pub(super) fn choose(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let view = &state.picker;
    let chosen = match &view.open {
        Some(super::Picking::Interrupted(list)) => rows(list, &view.home, &state.hosts)
            .get(at)
            .map(|i| (list.environment.clone(), i.id.clone())),
        _ => None,
    };
    let Some((environment, id)) = chosen else {
        return Vec::new();
    };
    close(&mut state.picker);
    vec![Effect::BringBack {
        environment,
        ids: vec![id],
    }]
}

#[cfg(test)]
mod tests {
    use adroit_model::{Agent, InterruptedKind, TerminalProgram};
    use adroit_protocol::testing::listed;

    use super::*;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::testing::{here, snapshot};

    fn interrupted(id: &str, name: &str) -> Interrupted {
        Interrupted {
            id: id.into(),
            name: name.into(),
            cwd: "/r".into(),
            started_ms: 1,
            what: InterruptedKind::Session {
                agent: Agent::Claude,
                profile: adroit_model::profile::ProfileId::builtin(Agent::Claude),
                conversation: Some("c".into()),
            },
        }
    }

    fn with_waiting() -> Scenario {
        let mut shell = interrupted("7", "shell");
        shell.what = InterruptedKind::Terminal {
            program: TerminalProgram::Shell,
            file: None,
        };
        Scenario::new().event(
            snapshot().listed(
                listed()
                    .interrupted(interrupted("a", "fix login"))
                    .interrupted(shell),
            ),
        )
    }

    fn names(run: &Scenario) -> Vec<String> {
        let state = run.state();
        match &state.picker.open {
            Some(super::super::Picking::Interrupted(list)) => {
                rows(list, &state.picker.home, &state.hosts)
                    .iter()
                    .map(|i| i.name.clone())
                    .collect()
            }
            other => panic!("not the list of what waits: {other:?}"),
        }
    }

    #[test]
    fn the_list_names_what_waits_and_narrows_as_typed() {
        let run = with_waiting().send(Msg::InterruptedOpen(here()));
        assert_eq!(names(&run), ["fix login", "shell"]);
        let run = run.send(Msg::PickerQuery("shell".into()));
        assert_eq!(names(&run), ["shell"]);
    }

    #[test]
    fn choosing_one_brings_it_back_and_closes_the_list() {
        let run = with_waiting()
            .send(Msg::InterruptedOpen(here()))
            .send(Msg::PickerChoose(1));
        assert_eq!(
            run.last_effects(),
            [Effect::BringBack {
                environment: here(),
                ids: vec!["7".into()],
            }]
        );
        assert_eq!(run.state().picker.open, None);
    }
}
