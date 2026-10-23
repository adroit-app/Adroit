//! The instance's window socket, which this window answers: another window
//! of the instance opened asks it to come forward, and `adroit editor`, run
//! by a session asked for its input, hands that input over to the prompt.

use std::path::Path;

use adroit_core::msg::Msg;
use dioxus::prelude::*;
use tokio::sync::oneshot;

use super::Ui;
use crate::bridge::AppCtx;
use crate::editor;
use crate::panes::adroit_program;
use crate::paths::instance_state;
use crate::prompts::{Op, Prompts};
use crate::window_socket::{Request, Response, ask, bind, serve, socket_path};

/// Answer the instance's window socket; a window opened together with
/// another of the instance, which claimed the socket first, brings that
/// one forward and closes.
pub(crate) fn use_window_socket(ui: &Ui) {
    let (app, prompts) = (ui.app, ui.consoles.prompts);
    use_hook(move || {
        let socket = socket_path(&instance_state());
        spawn(async move {
            let mut asked = match bind(&socket) {
                Ok(listener) => {
                    // Only the window that answers on the socket can take
                    // Claude Code's input; another one would hand it to this
                    // one, which never asked.
                    match editor::install(&adroit_program(), &socket) {
                        Ok(_) => app.dispatch(Msg::EditorAnswers),
                        Err(e) => app.status(Some(format!("cannot write the editor script: {e}"))),
                    }
                    serve(listener)
                }
                Err(e) => {
                    if matches!(ask(&socket, &Request::Show), Ok(Response::Shown)) {
                        dioxus::desktop::window().close();
                        return;
                    }
                    app.status(Some(format!("cannot answer window requests: {e}")));
                    return;
                }
            };
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
