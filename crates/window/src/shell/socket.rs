//! This window's socket, which it answers: adroit opened again asks it to
//! come forward, and `adroit editor`, run by a session asked for its input,
//! hands that input over to the prompt when this window asked for it.

use std::path::Path;

use adroit_core::msg::Msg;
use dioxus::prelude::*;
use tokio::sync::oneshot;

use super::Ui;
use crate::bridge::AppCtx;
use crate::editor;
use crate::panes::adroit_program;
use crate::prompts::{Op, Prompts};
use crate::window_socket::{Request, Response, serve, take_listener, windows};

/// Answer this window's socket, which it claimed as it opened.
pub(crate) fn use_window_socket(ui: &Ui) {
    let (app, prompts) = (ui.app, ui.consoles.prompts);
    use_hook(move || {
        spawn(async move {
            let Some(listener) = take_listener() else {
                app.status(Some("cannot answer window requests: no socket".into()));
                return;
            };
            let mut asked = match serve(listener) {
                Ok(asked) => asked,
                Err(e) => {
                    app.status(Some(format!("cannot answer window requests: {e}")));
                    return;
                }
            };
            // Every window of the instance writes the same script, which
            // offers the input to each of them.
            match editor::install(&adroit_program(), &windows()) {
                Ok(_) => app.dispatch(Msg::EditorAnswers),
                Err(e) => app.status(Some(format!("cannot write the editor script: {e}"))),
            }
            while let Some((request, reply)) = asked.recv().await {
                if let Request::Editor { path } = &request {
                    let _ = reply.send(take_input(path, prompts, app).await);
                    continue;
                }
                if request == Request::Show {
                    let w = dioxus::desktop::window();
                    w.window.set_visible(true);
                    w.window.set_minimized(false);
                    w.window.set_focus();
                    let _ = reply.send(Response::Shown);
                    continue;
                }
            }
        });
    });
}

/// Move the text a session handed its external editor in `path` into the
/// prompt, leaving the file empty so the session's input is cleared. A file
/// adroit did not ask for is left alone: the user opened the editor.
async fn take_input(path: &Path, prompts: Prompts, app: AppCtx) -> Response {
    let asked = app.console.peek().pull.clone();
    let Some(tab) = asked else {
        return Response::Declined;
    };
    app.dispatch(Msg::InputTaken);
    let lines = editor::typed(&std::fs::read_to_string(path).unwrap_or_default());
    if lines.is_empty() {
        return Response::Taken(0);
    }
    // The input is emptied only once the prompt holds the text, or keeps it
    // for the tab it was asked for; with no prompt to take it, Claude Code
    // keeps it.
    let (took, taken) = oneshot::channel();
    if prompts
        .send(Op::Add(Some(tab), lines.clone(), Some(took)))
        .is_err()
        || !taken.await.unwrap_or(false)
    {
        return Response::Taken(0);
    }
    if let Err(e) = std::fs::write(path, "") {
        app.status(Some(format!("cannot clear the session's input: {e}")));
        return Response::Taken(0);
    }
    let n = lines.len();
    app.notice(moved_notice(n));
    Response::Taken(n)
}

fn moved_notice(n: usize) -> String {
    format!(
        "moved {n} line{} from the session's input to the prompt",
        if n == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_taken_from_a_session_s_input_is_counted_without_naming_its_agent() {
        assert_eq!(
            moved_notice(1),
            "moved 1 line from the session's input to the prompt"
        );
        assert_eq!(
            moved_notice(3),
            "moved 3 lines from the session's input to the prompt"
        );
    }
}
