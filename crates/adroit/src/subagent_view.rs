//! A subagent of the overview, opened: what it was asked, its latest steps,
//! and what it reported once it ended.

use adroit_agents::claude::subagents::{Agent, Standing, Step};
use dioxus::prelude::*;

use crate::components::{Breadcrumbs, Section, StateGlyph};
use crate::details::shown_agent;
use crate::icons::Icon;

/// How a subagent is marked, by how it stands: the class that colours its
/// icon as a session's in the same standing is, and the icon.
pub fn agent_glyph(standing: Standing) -> (&'static str, Icon) {
    match standing {
        Standing::Running => ("state-glyph run", Icon::Loader),
        Standing::Done => ("state-glyph done", Icon::Check),
        Standing::Failed => ("state-glyph fail", Icon::AlertTriangle),
        Standing::Stopped => ("state-glyph idle", Icon::PlayerStop),
    }
}

/// What a subagent was started as, when a later title replaced it.
fn started_as(agent: &Agent) -> Option<&str> {
    agent
        .title
        .as_ref()
        .filter(|t| **t != agent.description && !agent.description.is_empty())
        .map(|_| agent.description.as_str())
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

const CSS: &str = include_str!("../assets/css/subagent.css");

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
    let started = started_as(&agent).map(str::to_string);
    let steps: Vec<Step> = shown_steps(&agent).into_iter().cloned().collect();
    let doing = if agent.standing == Standing::Running {
        "Doing"
    } else {
        "Steps"
    };
    rsx! {
        style { {CSS} }
        Breadcrumbs {
            title: "Back to the overview ({back_keys})",
            crumbs: vec![label],
            onback: move |_| on_back.call(()),
        }
        div { class: "details-block",
            span { class: "details-label",
                {
                    let (class, icon) = agent_glyph(agent.standing);
                    rsx! { StateGlyph { class, icon, size: 12 } }
                }
                " {tag}"
                if let Some(kind) = &agent.kind {
                    span { class: "commit-mark details-link-badge", style: "margin-left: 6px", "{kind}" }
                }
            }
        }
        if let Some(description) = started {
            Section { label: "Started as",
                div { class: "subagent-text", "{description}" }
            }
        }
        Section { label: "Prompt",
            div { class: "subagent-text", "{agent.prompt}" }
        }
        if let Some(text) = report {
            Section { label: "Report",
                div { class: "subagent-text", "{text}" }
            }
        }
        if !steps.is_empty() {
            Section { label: "{doing}",
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
            title: None,
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
    fn a_subagent_is_marked_in_the_colour_a_session_in_the_same_standing_is() {
        assert_eq!(
            agent_glyph(Standing::Running),
            ("state-glyph run", Icon::Loader)
        );
        assert_eq!(
            agent_glyph(Standing::Done),
            ("state-glyph done", Icon::Check)
        );
        assert_eq!(
            agent_glyph(Standing::Failed),
            ("state-glyph fail", Icon::AlertTriangle)
        );
        assert_eq!(
            agent_glyph(Standing::Stopped),
            ("state-glyph idle", Icon::PlayerStop)
        );
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
    fn a_subagent_titled_anew_tells_what_it_was_started_as() {
        let mut a = agent(Standing::Running, Vec::new());
        assert_eq!(started_as(&a), None);
        a.title = Some("Rename hover to detail".into());
        assert_eq!(started_as(&a), Some("Survey"));
        a.title = Some("Survey".into());
        assert_eq!(started_as(&a), None);
    }

    #[test]
    fn a_running_subagent_reports_nothing_yet_and_shows_every_step_latest_first() {
        let a = agent(Standing::Running, vec![tool("Read"), said("Reading.")]);
        assert_eq!(report(&a), None);
        assert_eq!(shown_steps(&a), [&said("Reading."), &tool("Read")]);
    }
}
