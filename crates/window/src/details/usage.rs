//! The agent's usage: the account's limits and how full the context is;
//! and the whole machine's.

use adroit_core::details::{Gauge, SystemGauge};
use adroit_model::Agent;
use adroit_model::usage::Severity;
use dioxus::prelude::*;

use crate::components::LinkButton;
use crate::icons::{Icon, IconView};

/// The account's gauges, in lines of those beside each other, with a button
/// to the agent's usage page while they include the account's limits:
/// `usage_url` is the page where the agent's account shows its usage.
#[component]
pub(super) fn AgentUsage(
    lines: Vec<Vec<Gauge>>,
    agent: Agent,
    usage_url: Option<String>,
) -> Element {
    let lines: Vec<Vec<Gauge>> = lines.into_iter().filter(|l| !l.is_empty()).collect();
    let account = lines.iter().flatten().any(|g| !g.context());
    rsx! {
        if !lines.is_empty() {
            div { class: "details-usage",
                title: "Account limits and context utilization reported by the session's agent",
                if let Some(url) = usage_url.filter(|_| account) {
                    div { class: "usage-header web-only",
                        LinkButton {
                            icon: Icon::Link,
                            title: "Open {agent.label()} usage settings in your browser",
                            onclick: move |ev: MouseEvent| {
                                ev.stop_propagation();
                                crate::bridge::open([url.clone()]);
                            },
                            "Web"
                        }
                    }
                }
                for (at , gauges) in lines.into_iter().enumerate() {
                    div { key: "{at}", class: "usage-gauge-line", style: "--beside:{gauges.len()}",
                        Gauges { gauges: gauges.clone() }
                    }
                }
            }
        }
    }
}

/// A bar for each of `gauges`, with what is left and when it resets.
#[component]
pub(crate) fn Gauges(gauges: Vec<Gauge>) -> Element {
    rsx! {
        for g in gauges.into_iter() {
            div { key: "{g.label}", class: "usage-gauge",
                div { class: "usage-gauge-head",
                    span { class: "usage-gauge-label", "{g.label}" }
                    span { class: "usage-gauge-left", "{g.left}" }
                }
                div { class: "usage-gauge-track",
                    div {
                        class: match g.severity {
                            _ if g.context() => "usage-gauge-fill context",
                            Severity::Normal => "usage-gauge-fill",
                            Severity::Warning => "usage-gauge-fill warning",
                            Severity::Critical => "usage-gauge-fill critical",
                        },
                        style: "width:{g.used}%",
                    }
                }
                span { class: "usage-gauge-note", title: "{g.note}", "{g.note}" }
            }
        }
    }
}

/// The whole machine's usage, as its host last sampled it, in lines of the
/// meters beside each other.
#[component]
pub(crate) fn MachineUsage(lines: Vec<Vec<SystemGauge>>) -> Element {
    let lines: Vec<Vec<SystemGauge>> = lines.into_iter().filter(|l| !l.is_empty()).collect();
    rsx! {
        if !lines.is_empty() {
            div { class: "details-system-usage",
                title: "Whole-machine resource usage",
                for (at , system) in lines.into_iter().enumerate() {
                div { key: "{at}", class: "system-usage-grid", style: "--beside:{system.len()}",
                    for g in system.iter().cloned() {
                        div { key: "{g.label}", class: "{g.class}",
                            div { class: "usage-resource-head",
                                span { class: "usage-resource-label",
                                    IconView { icon: g.icon, size: 14 }
                                    "{g.label}"
                                }
                                span { class: "usage-resource-value", "{g.value}" }
                            }
                            div { class: "usage-resource-track",
                                div { class: "usage-resource-fill", style: "width:{g.used}%" }
                            }
                            if !g.note.is_empty() {
                                span { class: "usage-resource-note", title: "{g.note}", "{g.note}" }
                            }
                        }
                    }
                }
                }
            }
        }
    }
}
