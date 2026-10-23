use adroit_core::content::{DraftOptions, Tab};
use adroit_core::prompt::drafts::Drafts;
use adroit_model::{Agent, AgentPermissions};
use std::collections::HashMap;

#[test]
fn changing_launch_options_keeps_the_prompt_on_the_same_draft() {
    let first = Tab::draft("/repo");
    let second = Tab::draft("/repo");
    let mut prompts = Drafts::default();
    prompts.show(Some(first.clone()), String::new());
    prompts.show(Some(second.clone()), "Keep this first prompt".into());
    let mut choices = HashMap::from([(first.clone(), DraftOptions::default())]);
    let options = choices.get_mut(&first).unwrap();
    options.agent = Some(Agent::Codex);
    options.model = Some("review-model".into());
    options.permissions = AgentPermissions::ReadOnly;

    assert_eq!(
        prompts.show(Some(first.clone()), "Keep this other prompt".into()),
        Some("Keep this first prompt".into())
    );
    assert_eq!(
        prompts.show(Some(second), "Keep this first prompt".into()),
        Some("Keep this other prompt".into())
    );
    assert!(choices.contains_key(&first));
}
