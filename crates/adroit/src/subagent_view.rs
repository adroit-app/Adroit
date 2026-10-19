//! A subagent of the overview, opened: what it was asked, its latest steps,
//! and what it reported once it ended.

use adroit_claude::subagents::{Agent, Standing, Step};
use dioxus::prelude::*;

use crate::details::shown_agent;
use crate::icons::{Icon, IconView};

/// The icon a subagent's row is shown with, by how it stands.
pub fn agent_icon(standing: Standing) -> Icon {
    match standing {
        Standing::Running => Icon::Loader,
        Standing::Done => Icon::Check,
        Standing::Failed => Icon::AlertTriangle,
        Standing::Stopped => Icon::PlayerStop,
    }
}

/// What a subagent that ended reported: the last thing it said.
fn report(agent: &Agent) -> Option<&str> {
    if agent.standing == Standing::Running {
        return None;
    }
    agent.steps.iter().rev().find_map(|s| match s {
        Step::Said(text) => Some(text.as_str()),
        Step::Tool { .. } => None,
    })
}

/// The steps shown under the report, the latest first: those before the
/// report, which is shown on its own.
fn shown_steps(agent: &Agent) -> Vec<&Step> {
    let mut steps: Vec<&Step> = agent.steps.iter().collect();
    if report(agent).is_some()
        && let Some(at) = steps.iter().rposition(|s| matches!(s, Step::Said(_)))
    {
        steps.remove(at);
    }
    steps.reverse();
    steps
}

const CSS: &str = r#"
.subagent-text { white-space: pre-wrap; word-break: break-word; user-select: text; font-size: 12.5px; color: var(--text); max-height: 18em; overflow-y: auto; }
.subagent-steps { display: flex; flex-direction: column; gap: 4px; }
.subagent-step { display: flex; gap: 8px; font-size: 12px; line-height: 1.5; min-width: 0; }
.subagent-step-tool { flex: none; font-family: var(--mono); font-size: 11.5px; color: var(--text-2); }
.subagent-step-about { color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: text; }
.subagent-step-said { color: var(--text-2); white-space: pre-wrap; word-break: break-word; display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; user-select: text; }
"#;

/// A subagent opened from the overview, told at `now_ms`; `on_back` goes
/// back to the overview, as `back_keys` do.
#[component]
pub fn SubagentView(
    agent: Agent,
    now_ms: u64,
    back_keys: String,
    on_back: EventHandler<()>,
) -> Element {
    let (label, tag) = shown_agent(&agent, now_ms);
    let report = report(&agent).map(str::to_string);
    let steps: Vec<Step> = shown_steps(&agent).into_iter().cloned().collect();
    let doing = if agent.standing == Standing::Running {
        "Doing"
    } else {
        "Steps"
    };
    rsx! {
        style { {CSS} }
        div { class: "details-crumbs",
            title: "Back to the overview ({back_keys})",
            onclick: move |_| on_back.call(()),
            span { class: "details-crumb-back", IconView { icon: Icon::ChevronLeft } }
            span { class: "details-crumb", "{label}" }
        }
        div { class: "details-block",
            span { class: "details-label",
                IconView { icon: agent_icon(agent.standing), size: 12 }
                " {tag}"
            }
        }
        div { class: "details-block",
            span { class: "details-label", "Prompt" }
            div { class: "subagent-text", "{agent.prompt}" }
        }
        if let Some(text) = report {
            div { class: "details-block",
                span { class: "details-label", "Report" }
                div { class: "subagent-text", "{text}" }
            }
        }
        if !steps.is_empty() {
            div { class: "details-block",
                span { class: "details-label", "{doing}" }
                div { class: "subagent-steps",
                    for (i , step) in steps.iter().enumerate() {
                        match step {
                            Step::Tool { name, about } => rsx! {
                                div { key: "{i}", class: "subagent-step", title: "{about}",
                                    span { class: "subagent-step-tool", "{name}" }
                                    span { class: "subagent-step-about", "{about}" }
                                }
                            },
                            Step::Said(text) => rsx! {
                                div { key: "{i}", class: "subagent-step",
                                    span { class: "subagent-step-said", "{text}" }
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(standing: Standing, steps: Vec<Step>) -> Agent {
        Agent {
            call: "toolu_1".into(),
            id: Some("a1".into()),
            description: "Survey".into(),
            kind: None,
            prompt: "Look.".into(),
            standing,
            started_ms: None,
            ended_ms: None,
            steps,
        }
    }

    fn tool(name: &str) -> Step {
        Step::Tool {
            name: name.into(),
            about: String::new(),
        }
    }

    fn said(text: &str) -> Step {
        Step::Said(text.into())
    }

    #[test]
    fn a_subagent_that_ended_reports_the_last_thing_it_said_apart_from_its_steps() {
        let a = agent(
            Standing::Done,
            vec![
                said("Looking."),
                tool("Read"),
                said("Found it."),
                tool("Bash"),
            ],
        );
        assert_eq!(report(&a), Some("Found it."));
        assert_eq!(
            shown_steps(&a),
            [&tool("Bash"), &tool("Read"), &said("Looking.")]
        );
    }

    #[test]
    fn a_running_subagent_reports_nothing_yet_and_shows_every_step_latest_first() {
        let a = agent(Standing::Running, vec![tool("Read"), said("Reading.")]);
        assert_eq!(report(&a), None);
        assert_eq!(shown_steps(&a), [&said("Reading."), &tool("Read")]);
    }
}
