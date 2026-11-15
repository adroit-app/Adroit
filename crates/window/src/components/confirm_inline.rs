use dioxus::prelude::*;

use super::Button;

/// A question asked in place before something that cannot be undone:
/// `question`, what follows from it in `children`, then `confirm` saying
/// what is done and `keep` taking the question back. `busy` holds both
/// buttons while the answer is being carried out.
#[component]
pub fn ConfirmInline(
    question: String,
    confirm: String,
    #[props(default = "Keep".to_string())] keep: String,
    #[props(default)] busy: bool,
    onconfirm: EventHandler<()>,
    onkeep: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "settings-confirm", role: "alertdialog", aria_label: question.trim_end_matches('?'),
            p { class: "settings-confirm-head", "{question}" }
            {children}
            div { class: "settings-page-actions",
                Button { kind: "danger", disabled: busy, onclick: move |_| onconfirm.call(()), "{confirm}" }
                Button { disabled: busy, onclick: move |_| onkeep.call(()), "{keep}" }
            }
        }
    }
}
