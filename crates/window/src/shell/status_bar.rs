//! The status bar: the mode the keys are in, what the window last said, and
//! the keys that matter where the keys are.

use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::Spacer;

/// The bar with the mode chip, worded `mode` and styled by `class`, over the
/// status line the chrome holds; `children` are the keys it lists, at its
/// end. `dialog` while the session in the console shows a dialog of its own.
#[component]
pub(crate) fn StatusBar(
    mode: String,
    class: &'static str,
    dialog: bool,
    children: Element,
) -> Element {
    let app = use_context::<AppCtx>();
    let line = app.chrome.read().status.line.clone();
    rsx! {
        div { class: "footer",
            span { class: "mode-chip {class}", title: "{mode_tip(&mode, dialog)}", "{mode}" }
            if let Some(msg) = line {
                span { class: "footer-status", title: "{msg}", "{msg}" }
            }
            Spacer {}
            {children}
        }
    }
}

/// What the mode chip's word means, `dialog` while the session in the console
/// shows a dialog of Claude Code's own.
pub(crate) fn mode_tip(mode: &str, dialog: bool) -> &'static str {
    match mode {
        "PASS" if dialog => {
            "The session shows a dialog: every key goes to it, but Ctrl+q sequences, until it closes"
        }
        "TREE" => "The tree has the keys: j and k move, Enter opens. Ctrl+q l goes right",
        "DETAILS" => {
            "The details panel has the keys: j and k move or scroll, Enter opens, h goes back, Cmd+] and Cmd+[ change the sheet"
        }
        "CONSOLE" => {
            "The console has the keys: Vim's moves read it, i writes a prompt below, Ctrl+q h goes back to the tree"
        }
        "VIEWER" => {
            "The viewer has the keys: j and k scroll, + and - zoom, q closes it. Ctrl+q h goes back to the tree"
        }
        "READ" | "VISUAL" => {
            "Vim's keys move, search, select and yank in the console; q, Esc or G goes back to it live, i writes a prompt"
        }
        "PASS" => "Every key goes to the console's program, but Ctrl+q sequences",
        "DIRECT" => "Every key goes to the session until Esc",
        "PICK" => "Choosing where a session works: type to narrow, Enter chooses, Esc closes",
        "INSERT" => "Writing a prompt: Ctrl+Enter sends it, Esc stops writing",
        "NORMAL" => "The prompt takes Vim commands: i writes again, Ctrl+q k goes up",
        _ => "The prompt is Neovim; this is its mode",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passing_keys_to_a_dialog_of_claude_code_s_says_so_in_the_mode_tip() {
        assert!(mode_tip("PASS", true).contains("dialog"));
        assert!(!mode_tip("PASS", false).contains("dialog"));
    }
}
