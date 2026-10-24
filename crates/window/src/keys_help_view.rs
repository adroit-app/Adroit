//! The keys help drawn over the window. It only draws: the window feeds the
//! keys into [`crate::keys_help::press`].

use dioxus::document;
use dioxus::prelude::*;

use crate::components::{Dialog, KeyHint};
use crate::icons::{Icon, IconView};
use crate::keys_help::Group;

/// How far a line of [`crate::keys_help::Press::Scroll`] scrolls, in pixels.
const LINE: i32 = 40;

/// Scroll the keys help shown by `lines`, down when positive.
pub fn scroll(lines: i32) {
    let by = lines * LINE;
    document::eval(&format!(
        "document.querySelector('.keys-help-groups')?.scrollBy({{ top: {by}, behavior: 'smooth' }});"
    ));
}

#[component]
pub fn KeysHelpView(groups: Vec<Group>, onclose: EventHandler<()>) -> Element {
    rsx! {
        div { onclick: move |_| onclose.call(()),
            Dialog { kind: "keys-help-dialog", icon: Icon::Keyboard, title: "Keyboard Shortcuts",
                div { class: "keys-help-groups",
                    div { class: "keys-help-columns",
                        for group in groups {
                            section {
                                key: "{group.context.name()}",
                                class: if group.here { "keys-help-group here" } else { "keys-help-group" },
                                div { class: "keys-help-head",
                                    span { class: "keys-help-title", "{group.context.title()}" }
                                    if group.here {
                                        span { class: "keys-help-here", "Here" }
                                    }
                                }
                                div { class: "keys-help-hint", "{group.context.hint()}" }
                                for (i, row) in group.rows.into_iter().enumerate() {
                                    div { key: "{i}", class: "keys-help-row",
                                        span { class: "keys-help-icon",
                                            if let Some(icon) = row.icon {
                                                IconView { icon, size: 14 }
                                            }
                                        }
                                        span { class: "keys-help-label",
                                            "{row.label}"
                                            if row.repeats {
                                                span {
                                                    class: "keys-help-repeats",
                                                    title: "Repeats: press the last key again to do it again",
                                                    "↻"
                                                }
                                            }
                                        }
                                        span { class: "keys-help-keys",
                                            for (j, keys) in row.keys.into_iter().enumerate() {
                                                span { key: "{j}", class: "keys-help-seq",
                                                    for (k, chord) in keys.lead.into_iter().enumerate() {
                                                        kbd { key: "lead-{k}", "{chord}" }
                                                    }
                                                    for (k, end) in keys.ends.into_iter().enumerate() {
                                                        span { key: "end-{k}", class: "keys-help-end",
                                                            if k > 0 {
                                                                span { class: "keys-help-or", "/" }
                                                            }
                                                            kbd { "{end}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "keys-help-footer",
                    KeyHint { keys: "j/k", does: "scroll" }
                    KeyHint { keys: "Esc", does: "close" }
                    span { class: "keys-help-legend",
                        span { class: "keys-help-repeats", "↻" }
                        "the last key repeats it"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    const CSS: &str = include_str!("../assets/css/keys-help.css");

    fn rule(selector: &str) -> &'static str {
        CSS.lines()
            .find(|l| l.starts_with(&format!("{selector} {{")))
            .unwrap_or_else(|| panic!("no rule for {selector}"))
    }

    #[test]
    fn the_box_the_keys_scroll_reaches_every_group_downwards() {
        // Columns in a box of fixed height spill sideways, where a scroll
        // by `top` never reaches them.
        let scrolled = rule(".keys-help-groups");
        assert!(scrolled.contains("overflow-y: auto"), "{scrolled}");
        assert!(!scrolled.contains("columns"), "{scrolled}");
    }
}
