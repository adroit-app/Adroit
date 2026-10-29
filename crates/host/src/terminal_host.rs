//! The terminal host as the work host keeps it: started as the work host
//! starts, when none runs, judged against this build, and replaced only when
//! the user asks.

use adroit_protocol::TerminalHostStanding;
use tokio::sync::watch;

use crate::daemon;
use crate::standing::{self, Standing};
use crate::work_host::adroit_program;

/// How the terminal host stands, or why it could not be started; none until
/// it has been started and judged.
pub type Judged = Option<Result<TerminalHostStanding, String>>;

impl From<Standing> for TerminalHostStanding {
    fn from(standing: Standing) -> Self {
        match standing {
            Standing::Absent => TerminalHostStanding::Absent,
            Standing::Compatible => TerminalHostStanding::Works,
            Standing::Incompatible(build) => TerminalHostStanding::Old { build },
        }
    }
}

/// Start the terminal host as adroit's own where none runs, with the
/// description of the terminal its programs run in, and tell `judged` how it
/// stands.
pub(crate) async fn start(judged: &watch::Sender<Judged>) {
    let compiled =
        crate::environment::spawn_blocking(|| daemon::install_terminfo(&daemon::terminfo_dir()))
            .await;
    let mut failed = match compiled {
        Ok(Err(e)) => Some(format!("cannot install the terminal's description: {e}")),
        _ => None,
    };
    let terminal_host = daemon::daemon();
    let started = if crate::remote::active() {
        terminal_host.sessions().await.map(|_| ())
    } else {
        terminal_host
            .start(&adroit_program(), &daemon::serve_args())
            .await
    };
    if let Err(e) = started {
        failed = Some(format!("cannot start the sessions' daemon: {e}"));
    }
    let stands = standing::settled(&terminal_host).await;
    judged.send_replace(Some(match failed {
        Some(text) => Err(text),
        None => Ok(stands.into()),
    }));
}

/// Replace the terminal host with one of this build, ending every session it
/// holds, and tell `judged` how the new one stands.
pub(crate) async fn restart(
    judged: &watch::Sender<Judged>,
) -> Result<TerminalHostStanding, String> {
    if crate::remote::active() || crate::environment::current().is_some() {
        return Err(
            "manage the remote terminal service on its host; it is not restarted by a window"
                .into(),
        );
    }
    let terminal_host = daemon::daemon();
    terminal_host
        .restart(&adroit_program(), &daemon::serve_args())
        .await
        .map_err(|e| format!("cannot restart the sessions' daemon: {e}"))?;
    let stands: TerminalHostStanding = standing::settled(&terminal_host).await.into();
    judged.send_replace(Some(Ok(stands.clone())));
    Ok(stands)
}

/// How the terminal host stands, once it has been started and judged.
pub(crate) async fn judged(
    mut judged: watch::Receiver<Judged>,
) -> Result<TerminalHostStanding, String> {
    match judged.wait_for(Option::is_some).await {
        Ok(stands) => stands.clone().expect("waited until judged"),
        Err(_) => Err("the work host stopped".into()),
    }
}
