//! A live process of a session, opened from the overview.

use adroit_model::Process;
use dioxus::prelude::*;

use crate::components::{Button, Fact, Facts, LinkButton};
use crate::icons::Icon;

#[component]
pub fn ProcessView(
    process: Process,
    on_back: EventHandler<()>,
    on_kill: EventHandler<Process>,
) -> Element {
    let pid = process.pid;
    let parent = process
        .parent
        .map(|p| p.to_string())
        .unwrap_or_else(|| "Unknown".into());
    let memory = format!("{:.1} MiB", process.memory as f64 / (1024.0 * 1024.0));
    rsx! {
        div { class: "details-block",
            LinkButton { icon: Icon::ChevronLeft, onclick: move |_| on_back.call(()), "Back" }
            span { class: "details-label", "{process.name}" }
            Facts {
                Fact { label: "PID", span { "{pid}" } }
                Fact { label: "Parent", span { "{parent}" } }
                Fact { label: "Memory", span { "{memory}" } }
                if let Some(cwd) = &process.cwd {
                    Fact { label: "Directory", span { class: "details-facts-value", "{cwd.display()}" } }
                }
            }
            pre { class: "process-command", "{process.command}" }
            if process.zombie {
                span { class: "dim", "Already exited (zombie). Parent PID {parent} must reap it." }
            } else {
                Button { kind: "danger", onclick: move |_| on_kill.call(process.clone()), "Kill…" }
            }
        }
    }
}
