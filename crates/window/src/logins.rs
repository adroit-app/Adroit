//! What the window knows of each profile's login, as the CLI it starts
//! said: asked as a draft shows the profile, kept a while so drafts shown
//! one after another ask once, and asked again once a login terminal opened
//! from a draft ends.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use adroit_model::profile::{ConnectionId, LoginStanding, ProfileId};
use adroit_protocol::{Answer, Ask};
use dioxus::prelude::*;

use crate::bridge::AppCtx;

/// How long an answer stands before a draft shown again asks again.
const FRESH: Duration = Duration::from_secs(60);

/// What is known of one profile's login.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Login {
    Asking,
    /// Its login terminal is open; it is asked again once that ends.
    LoggingIn,
    /// As its CLI answered at that moment.
    Is(LoginStanding, Instant),
}

pub(crate) type Logins = HashMap<ProfileId, Login>;

/// Whether a login known as `known` is to be asked of its CLI at `now`:
/// never asked, or answered too long ago; not while it is being asked.
pub(crate) fn stale(known: Option<&Login>, now: Instant) -> bool {
    match known {
        None => true,
        Some(Login::Is(_, at)) => now.saturating_duration_since(*at) >= FRESH,
        Some(Login::Asking | Login::LoggingIn) => false,
    }
}

/// Ask the work host whether `profile`, which runs in `connection`, is
/// logged in, once the terminal `after` has ended when one is named; the
/// answer goes to the window's logins whichever draft asked.
pub(crate) fn ask(app: AppCtx, profile: ProfileId, connection: ConnectionId, after: Option<u128>) {
    let mut logins = app.logins;
    logins.write().insert(
        profile.clone(),
        match after {
            Some(_) => Login::LoggingIn,
            None => Login::Asking,
        },
    );
    let asked = app.ask(Ask::LoginStanding {
        connection,
        profile: Some(profile.clone()),
        after,
    });
    app.keep(async move {
        match asked.await {
            Answer::LoginStanding { standing, .. } => {
                logins
                    .write()
                    .insert(profile, Login::Is(standing, Instant::now()));
            }
            // Asked again when a draft shows it next.
            _ => {
                logins.write().remove(&profile);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_login_is_asked_when_unknown_or_answered_long_enough_ago_and_not_while_asked() {
        let now = Instant::now();
        let answered = |ago| Login::Is(LoginStanding::LoggedIn, now - ago);
        assert!(stale(None, now));
        assert!(!stale(Some(&answered(Duration::from_secs(5))), now));
        assert!(stale(Some(&answered(FRESH)), now));
        assert!(!stale(Some(&Login::Asking), now));
        assert!(!stale(Some(&Login::LoggingIn), now));
    }
}
