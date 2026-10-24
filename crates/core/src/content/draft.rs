//! Which profile a window's session runs, and with it its agent: the one a
//! running session was started from, or the one chosen for a draft before it
//! starts.

use adroit_model::profile::ProfileId;
use adroit_model::{Agent, AgentPermissions};

use crate::content::DraftOptions;

pub fn can_choose(tab: Option<&crate::content::Tab>) -> bool {
    matches!(tab, None | Some(crate::content::Tab::Draft { .. }))
}

/// The profile a draft starts from: `chosen` while a session can start from
/// it, else the default profile.
pub fn profile_for(settings: &crate::settings::Settings, chosen: Option<&ProfileId>) -> ProfileId {
    chosen
        .filter(|id| settings.find_profile(id).is_some())
        .cloned()
        .unwrap_or_else(|| settings.profile_of(settings.default_agent()).id.clone())
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
    fn a_draft_starts_from_the_profile_chosen_while_it_can_else_the_default_one() {
        let s = Settings::parse(
            "profile = \"work\"\n[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"work\"\nagent = \"claude\"\n[[profiles]]\nid = \"review\"\nagent = \"codex\"\n",
        )
        .unwrap();
        let id = |text: &str| text.parse::<ProfileId>().unwrap();
        let pick = |chosen: Option<&str>| profile_for(&s, chosen.map(id).as_ref()).to_string();
        assert_eq!(pick(Some("claude")), "claude");
        assert_eq!(pick(Some("review")), "review");
        assert_eq!(pick(None), "work");
        // Adroit's own for an agent with no profile is there to start from.
        assert_eq!(pick(Some("pi")), "pi");
        // Deleted since, it is not taken.
        assert_eq!(pick(Some("gone")), "work");
    }

    #[test]
    fn the_profile_is_chosen_only_before_a_session_starts() {
        use crate::content::Tab;
        assert!(can_choose(None));
        assert!(can_choose(Some(&Tab::draft("/project"))));
        for tab in [Tab::Session("running".into()), Tab::Terminal(1)] {
            assert!(!can_choose(Some(&tab)));
        }
    }
}
