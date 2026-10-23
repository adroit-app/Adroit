//! A live process of a session, opened from the overview.

use dioxus::prelude::*;

use adroit_model::Process;

#[dioxus::prelude::component]
pub fn ProcessView(
    process: Process,
    on_back: dioxus::prelude::EventHandler<()>,
    on_kill: dioxus::prelude::EventHandler<Process>,
) -> dioxus::prelude::Element {
    let pid = process.pid;
    let parent = process
        .parent
        .map(|p| p.to_string())
        .unwrap_or_else(|| "Unknown".into());
    let memory = format!("{:.1} MiB", process.memory as f64 / (1024.0 * 1024.0));
    rsx! {
        div { class: "details-block",
            button { class: "usage-web", onclick: move |_| on_back.call(()),
                crate::icons::IconView { icon: crate::icons::Icon::ChevronLeft, size: 12 }
                "Back"
            }
            span { class: "details-label", "{process.name}" }
            div { class: "details-facts",
                span { class: "details-facts-key", "PID" } span { "{pid}" }
                span { class: "details-facts-key", "Parent" } span { "{parent}" }
                span { class: "details-facts-key", "Memory" } span { "{memory}" }
                if let Some(cwd) = &process.cwd {
                    span { class: "details-facts-key", "Directory" }
                    span { class: "details-facts-value", "{cwd.display()}" }
                }
            }
            pre { class: "process-command", "{process.command}" }
            if process.zombie {
                span { class: "dim", "Already exited (zombie). Parent PID {parent} must reap it." }
            } else {
                button { class: "button danger", onclick: move |_| on_kill.call(process.clone()), "Kill…" }
            }
        }
    }
}
