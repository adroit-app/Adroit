//! The terminal host as the window shows it: how it stands against this
//! build, as the work host that started it judges, and its restart when the
//! user asks.

use adroit_core::msg::Msg;
use adroit_protocol::{Answer, Ask};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::panes::tell_live_on;
use crate::shell::Ui;
use crate::shell::overlays::old_daemon_note;

/// Tell the window how the terminal host stands once the work host has
/// started it; then keep telling which sessions are live here.
pub(crate) fn use_hosts(ui: &Ui) {
    let (app, hub) = (ui.app, ui.hub.clone());
    use_hook(move || {
        spawn(async move {
            if let Some(note) = standing(app).await {
                app.dispatch(Msg::DaemonStanding(note));
            }
            tell_live_on(app, hub).await;
        });
    });
}

/// What the banner says of the terminal host, as the work host judges it;
/// none when the work host could not tell, which the status line says.
async fn standing(app: AppCtx) -> Option<Option<String>> {
    match app.ask(Ask::TerminalHost).await {
        Answer::TerminalHost { standing } => Some(old_daemon_note(&standing)),
        Answer::Failed { text } => {
            app.status(Some(text));
            None
        }
        _ => None,
    }
}

impl Ui {
    /// Replace the daemon with one of this build, ending every session it
    /// holds.
    pub(crate) fn restart_daemon(&self) {
        let app = self.app;
        spawn(async move {
            app.notice("Restarting the sessions' daemon".into());
            let note = match app.ask(Ask::RestartTerminalHost).await {
                Answer::TerminalHost { standing } => {
                    app.notice("Restarted the sessions' daemon".into());
                    Some(old_daemon_note(&standing))
                }
                Answer::Failed { text } => {
                    app.status(Some(text));
                    standing(app).await
                }
                _ => None,
            };
            if let Some(note) = note {
                app.dispatch(Msg::DaemonRestarted(note));
            }
            app.dispatch(Msg::Relist);
        });
    }
}
