//! Which agent a window's session runs: the one a running session was
//! started with, or the one chosen for a draft before it starts.

use std::collections::HashMap;

use adroit_model::Agent;

/// The open session keeps its own provider when the default changes; a
/// session not started yet runs the one chosen for it, if any.
pub fn for_tab(
    tree: &adroit_model::Tree,
    tab: Option<&crate::content::Tab>,
    chosen: &HashMap<crate::content::Tab, crate::content::DraftOptions>,
    fallback: Agent,
) -> Agent {
    match tab {
        Some(crate::content::Tab::Session(id)) => {
            crate::tree::nav::find(tree, id).map_or(fallback, |(_, s)| s.agent)
        }
        Some(draft @ crate::content::Tab::Draft { .. }) => chosen
            .get(draft)
            .and_then(|options| options.agent)
            .unwrap_or(fallback),
        _ => fallback,
    }
}

pub fn can_choose(tab: Option<&crate::content::Tab>) -> bool {
    matches!(tab, None | Some(crate::content::Tab::Draft { .. }))
}

/// Remember the choice for subsequent drafts, and for the open draft, if any.
pub fn choose(
    chosen: &mut HashMap<crate::content::Tab, crate::content::DraftOptions>,
    tab: Option<crate::content::Tab>,
    agent: Agent,
    last: &mut Option<Agent>,
) {
    if !can_choose(tab.as_ref()) {
        return;
    }
    if let Some(draft) = tab {
        chosen.entry(draft).or_default().agent = Some(agent);
    }
    *last = Some(agent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn a_running_session_keeps_its_adapter_when_the_default_changes() {
        use crate::content::Tab;
        use adroit_model::testing::{session, tree};
        let tree = tree()
            .found("/api", None)
            .session(session("codex-session").cwd("/api").agent(Agent::Codex))
            .build();
        assert_eq!(
            for_tab(
                &tree,
                Some(&Tab::Session("codex-session".into())),
                &HashMap::new(),
                Agent::Claude
            ),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, None, &HashMap::new(), Agent::Claude),
            Agent::Claude
        );
    }

    #[test]
    fn a_draft_runs_the_agent_chosen_for_it_and_otherwise_the_default() {
        use crate::content::Tab;
        let draft = |dir: &str| Tab::Draft {
            worktree: dir.into(),
            id: Default::default(),
        };
        let api = draft("/api");
        let chosen = HashMap::from([(api.clone(), Agent::Codex.into())]);
        let tree = adroit_model::Tree::default();
        assert_eq!(
            for_tab(&tree, Some(&api), &chosen, Agent::Claude),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, Some(&draft("/web")), &chosen, Agent::Claude),
            Agent::Claude
        );
    }

    #[test]
    fn choosing_an_agent_in_an_empty_window_sets_the_next_session_agent() {
        use crate::content::Tab;

        let tree = adroit_model::Tree::default();
        let draft = Tab::Draft {
            worktree: "/project".into(),
            id: Default::default(),
        };
        let mut chosen = HashMap::from([(draft.clone(), Agent::Claude.into())]);
        let mut last = None;

        choose(&mut chosen, None, Agent::Codex, &mut last);

        assert_eq!(last, Some(Agent::Codex));
        assert_eq!(chosen.len(), 1);
        let fallback = last.unwrap_or(Agent::Claude);
        assert_eq!(for_tab(&tree, None, &chosen, fallback), Agent::Codex);
        assert_eq!(
            for_tab(&tree, Some(&draft), &chosen, fallback),
            Agent::Claude
        );
        for _ in 0..2 {
            let next = Tab::Draft {
                worktree: "/next".into(),
                id: Default::default(),
            };
            assert_eq!(for_tab(&tree, Some(&next), &chosen, fallback), Agent::Codex);
        }
    }

    #[test]
    fn agent_selection_is_available_only_before_a_session_starts() {
        use crate::content::Tab;

        assert!(can_choose(None));
        for _ in 0..2 {
            assert!(can_choose(Some(&Tab::Draft {
                worktree: "/project".into(),
                id: Default::default(),
            })));
        }
        for tab in [Tab::Session("running".into()), Tab::Terminal(1)] {
            assert!(!can_choose(Some(&tab)));
            let mut chosen = HashMap::new();
            let mut last = Some(Agent::Claude);
            choose(&mut chosen, Some(tab), Agent::Codex, &mut last);
            assert_eq!(last, Some(Agent::Claude));
            assert!(chosen.is_empty());
        }
    }

    #[test]
    fn choosing_an_agent_sets_the_next_draft_default_only_until_the_window_closes() {
        use crate::content::Tab;
        let draft = |dir: &str| Tab::Draft {
            worktree: dir.into(),
            id: Default::default(),
        };
        let api = draft("/api");
        let web = draft("/web");
        let tree = adroit_model::Tree::default();
        let mut chosen = HashMap::new();
        let settings = Settings::default();
        let mut last = None;

        choose(&mut chosen, Some(api.clone()), Agent::Codex, &mut last);
        assert_eq!(settings.agent, Agent::Claude);
        assert_eq!(last, Some(Agent::Codex));
        assert_eq!(
            for_tab(&tree, Some(&api), &chosen, last.unwrap_or(settings.agent)),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, Some(&web), &chosen, last.unwrap_or(settings.agent)),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, Some(&web), &HashMap::new(), settings.agent),
            Agent::Claude
        );

        choose(&mut chosen, Some(web.clone()), Agent::Claude, &mut last);
        assert_eq!(last, Some(Agent::Claude));
        assert_eq!(
            for_tab(&tree, Some(&api), &chosen, last.unwrap_or(settings.agent)),
            Agent::Codex
        );
        assert_eq!(
            for_tab(
                &tree,
                Some(&draft("/other")),
                &chosen,
                last.unwrap_or(settings.agent)
            ),
            Agent::Claude
        );
    }
}
