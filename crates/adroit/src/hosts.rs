//! The hosts the window works with, started as it opens: the daemon its
//! sessions run in, as adroit's own when none runs yet, and the work host,
//! replaced silently when it is of another build; and the daemon replaced
//! when the user asks.

use adroit_core::msg::Msg;
use dioxus::prelude::*;

use crate::panes::{adroit_program, tell_live_on};
use crate::shell::Ui;
use crate::shell::overlays::old_daemon_note;
use crate::{daemon, standing, work_host};

/// Start the daemon and the work host, which lists the sessions from then
/// on; then keep telling which sessions are live here.
pub(crate) fn use_hosts(ui: &Ui) {
    let (app, hub) = (ui.app, ui.hub.clone());
    use_hook(move || {
        spawn(async move {
            let compiled =
                tokio::task::spawn_blocking(|| daemon::install_terminfo(&daemon::terminfo_dir()))
                    .await;
            if let Ok(Err(e)) = compiled {
                app.status(Some(format!(
                    "cannot install the terminal's description: {e}"
                )));
            }
            // The daemon may not run yet: started as adroit's own.
            if let Err(e) = hub
                .daemon
                .start(&adroit_program(), &daemon::serve_args())
                .await
            {
                app.status(Some(format!("cannot start the sessions' daemon: {e}")));
            }
            let standing = standing::settled(&hub.daemon).await;
            app.dispatch(Msg::DaemonStanding(old_daemon_note(&standing)));
            // Replaced silently when of another build: it ends no session.
            if let Err(e) = work_host::ensure(
                &work_host::socket(),
                &adroit_program(),
                &work_host::serve_args(),
            )
            .await
            {
                app.status(Some(format!("cannot start the work host: {e}")));
            }
            app.watch();
            tell_live_on(app, hub).await;
        });
    });
}

impl Ui {
    /// Replace the daemon with one of this build, ending every session it
    /// holds.
    pub(crate) fn restart_daemon(&self) {
        let (app, hub) = (self.app, self.hub.clone());
        spawn(async move {
            app.notice("Restarting the sessions' daemon".into());
            match hub
                .daemon
                .restart(&adroit_program(), &daemon::serve_args())
                .await
            {
                Ok(()) => app.notice("Restarted the sessions' daemon".into()),
                Err(e) => app.status(Some(format!("cannot restart the sessions' daemon: {e}"))),
            }
            let standing = standing::settled(&hub.daemon).await;
            app.dispatch(Msg::DaemonRestarted(old_daemon_note(&standing)));
            app.dispatch(Msg::Relist);
        });
    }
}
