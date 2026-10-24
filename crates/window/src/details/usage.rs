//! The foot of the details panel: the account's usage limits and how full
//! the context is, then the whole machine's usage.

use adroit_core::details::{Gauge, SystemGauge};
use adroit_model::Agent;
use adroit_model::usage::Severity;
use dioxus::prelude::*;

use crate::components::LinkButton;
use crate::icons::{Icon, IconView};

/// The account's gauges, with a button to the agent's usage page while
/// they include the account's limits, and the machine's: `usage_url` is
/// the page where the agent's account shows its usage.
#[component]
pub(super) fn Usage(
    gauges: Vec<Gauge>,
    agent: Agent,
    usage_url: Option<String>,
    system: Vec<SystemGauge>,
) -> Element {
    let account = gauges.iter().any(|g| !g.context);
    rsx! {
        if !gauges.is_empty() {
            div { class: "details-usage",
                title: "Account limits and context utilization reported by the session's agent",
                div { class: "usage-header",
                    span { class: "details-label", "Usage" }
                    if let Some(url) = usage_url.filter(|_| account) {
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
                for g in gauges.into_iter() {
                    div { key: "{g.label}", class: "usage-gauge",
                        div { class: "usage-gauge-head",
                            span { class: "usage-gauge-label", "{g.label}" }
                            span { class: "usage-gauge-left", "{g.left}" }
                        }
                        div { class: "usage-gauge-track",
                            div {
                                class: match g.severity {
                                    _ if g.context => "usage-gauge-fill context",
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
        if !system.is_empty() {
            div { class: "details-system-usage",
                title: "Whole-machine resource usage",
                div { class: "system-usage-grid",
                    for g in system {
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
