//! Which agent and profile a window's session runs: those a running session
//! was started with, or those chosen for a draft before it starts.

use std::collections::HashMap;

use adroit_model::profile::ProfileId;
use adroit_model::{Agent, AgentPermissions};

use crate::content::DraftOptions;

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

/// The profile a draft of `agent` starts from: `chosen` while it is one a
/// session can start from and starts `agent`, else `agent`'s default profile.
pub fn profile_for(
    settings: &crate::settings::Settings,
    agent: Agent,
    chosen: Option<&ProfileId>,
) -> ProfileId {
    chosen
        .filter(|id| {
            settings
                .find_profile(id)
                .is_some_and(|p| !p.archived && p.agent() == agent)
        })
        .cloned()
        .unwrap_or_else(|| settings.profile_of(agent).id.clone())
}

/// What a draft's `options` keep when it moves from `from` to `to`, the
/// profile called `to_name`: the model and permissions chosen within one
/// agent; across agents neither, which no longer apply, with what to tell
/// of that when there was something to drop.
pub fn carried(
    options: &DraftOptions,
    from: Agent,
    to: Agent,
    to_name: &str,
) -> (DraftOptions, Option<String>) {
    if from == to {
        return (options.clone(), None);
    }
    let dropped = options.model.is_some() || options.permissions != AgentPermissions::default();
    let kept = DraftOptions {
        model: None,
        permissions: AgentPermissions::default(),
        ..options.clone()
    };
    let told = dropped.then(|| {
        format!(
            "{} starts {}: the model and permissions chosen for {} are back to {to_name}'s defaults",
            to_name,
            to.label(),
            from.label()
        )
    });
    (kept, told)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn switching_within_an_agent_keeps_the_model_and_permissions_and_across_resets_them() {
        let chosen = DraftOptions {
            model: Some("opus".into()),
            permissions: AgentPermissions::ReadOnly,
            ..DraftOptions::default()
        };
        assert_eq!(
            carried(&chosen, Agent::Claude, Agent::Claude, "Claude Work"),
            (chosen.clone(), None)
        );
        let (kept, told) = carried(&chosen, Agent::Claude, Agent::Codex, "Codex Review");
        assert_eq!(kept.model, None);
        assert_eq!(kept.permissions, AgentPermissions::default());
        assert_eq!(
            told.as_deref(),
            Some(
                "Codex Review starts Codex: the model and permissions chosen for Claude Code are back to Codex Review's defaults"
            )
        );
        // With nothing chosen there is nothing to tell.
        assert_eq!(
            carried(
                &DraftOptions::default(),
                Agent::Claude,
                Agent::Codex,
                "Codex"
            )
            .1,
            None
        );
    }

    #[test]
    fn a_draft_starts_from_the_profile_chosen_while_it_can_else_its_agent_s_default() {
        let s = Settings::parse(
            "profile = \"work\"\n[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"work\"\nagent = \"claude\"\n[[profiles]]\nid = \"old\"\nagent = \"claude\"\narchived = true\n[[profiles]]\nid = \"review\"\nagent = \"codex\"\n",
        )
        .unwrap();
        let id = |text: &str| text.parse::<ProfileId>().unwrap();
        let pick = |agent, chosen: Option<&str>| {
            profile_for(&s, agent, chosen.map(id).as_ref()).to_string()
        };
        assert_eq!(pick(Agent::Claude, Some("claude")), "claude");
        assert_eq!(pick(Agent::Claude, None), "work");
        assert_eq!(pick(Agent::Codex, None), "review");
        // Archived, gone or of another agent since, it is not taken.
        assert_eq!(pick(Agent::Claude, Some("old")), "work");
        assert_eq!(pick(Agent::Claude, Some("gone")), "work");
        assert_eq!(pick(Agent::Codex, Some("claude")), "review");
    }

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
        assert_eq!(settings.default_agent(), Agent::Claude);
        assert_eq!(last, Some(Agent::Codex));
        assert_eq!(
            for_tab(
                &tree,
                Some(&api),
                &chosen,
                last.unwrap_or(settings.default_agent())
            ),
            Agent::Codex
        );
        assert_eq!(
            for_tab(
                &tree,
                Some(&web),
                &chosen,
                last.unwrap_or(settings.default_agent())
            ),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, Some(&web), &HashMap::new(), settings.default_agent()),
            Agent::Claude
        );

        choose(&mut chosen, Some(web.clone()), Agent::Claude, &mut last);
        assert_eq!(last, Some(Agent::Claude));
        assert_eq!(
            for_tab(
                &tree,
                Some(&api),
                &chosen,
                last.unwrap_or(settings.default_agent())
            ),
            Agent::Codex
        );
        assert_eq!(
            for_tab(
                &tree,
                Some(&draft("/other")),
                &chosen,
                last.unwrap_or(settings.default_agent())
            ),
            Agent::Claude
        );
    }
}
