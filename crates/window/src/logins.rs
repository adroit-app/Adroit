//! What the window knows of each profile's login, as the CLI it starts
//! said: asked as a draft shows the profile, kept a while so drafts shown
//! one after another ask once, and asked again once a login terminal opened
//! from a draft ends.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use adroit_model::Agent;
use adroit_model::profile::{LoginStanding, ProfileId};
use adroit_protocol::{Answer, Ask};
use dioxus::prelude::*;

use crate::bridge::AppCtx;

/// How long an answer stands before a draft shown again asks again.
const FRESH: Duration = Duration::from_secs(60);

/// What is known of one profile's login.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Login {
    Asking,
    /// Its login terminal, by its id, is open; it is asked again once that
    /// ends.
    LoggingIn(u128),
    /// As its CLI answered at that moment.
    Is(LoginStanding, Instant),
}

pub(crate) type Logins = HashMap<ProfileId, Login>;

/// What a profile of `agent` can do without a login, said where it shows as
/// not logged in: OpenCode runs its free models and local ones without one.
pub(crate) fn without_login(agent: Agent) -> Option<&'static str> {
    match agent {
        Agent::OpenCode => Some("Free and local models need no login."),
        Agent::Claude | Agent::Codex | Agent::Pi => None,
    }
}

/// Whether a login known as `known` is to be asked of its CLI at `now`:
/// never asked, or answered too long ago; not while it is being asked.
pub(crate) fn stale(known: Option<&Login>, now: Instant) -> bool {
    match known {
        None => true,
        Some(Login::Is(_, at)) => now.saturating_duration_since(*at) >= FRESH,
        Some(Login::Asking | Login::LoggingIn(_)) => false,
    }
}

/// What is known of a login known as `known` while it is asked, once the
/// terminal `after` has ended when one is named: a login terminal still
/// open is still waited for.
pub(crate) fn asking(known: Option<&Login>, after: Option<u128>) -> Login {
    match (after, known) {
        (Some(terminal), _) | (None, Some(&Login::LoggingIn(terminal))) => {
            Login::LoggingIn(terminal)
        }
        _ => Login::Asking,
    }
}

/// What is known of a login known as `known` once its CLI answered
/// `standing` at `at`, to an ask that waited for the terminal `after`: a
/// login terminal waited for is waited for still, unless the answer came
/// once it ended or says the login is done.
pub(crate) fn answered(
    known: Option<&Login>,
    after: Option<u128>,
    standing: LoginStanding,
    at: Instant,
) -> Login {
    match known {
        Some(Login::LoggingIn(terminal)) if !ends_wait(*terminal, after, Some(standing)) => {
            Login::LoggingIn(*terminal)
        }
        _ => Login::Is(standing, at),
    }
}

/// Whether an answer, `standing` when the CLI gave one, ends the wait for
/// the login terminal `waited`: given once that terminal ended, or saying
/// the login is done.
pub(crate) fn ends_wait(
    waited: u128,
    after: Option<u128>,
    standing: Option<LoginStanding>,
) -> bool {
    after == Some(waited) || standing == Some(LoginStanding::LoggedIn)
}

/// Ask the work host whether `profile` is logged in, once the terminal
/// `after` has ended when one is named; the answer goes to the window's
/// logins whichever draft asked.
pub(crate) fn ask(app: AppCtx, profile: ProfileId, after: Option<u128>) {
    let mut logins = app.logins;
    let now = asking(logins.peek().get(&profile), after);
    logins.write().insert(profile.clone(), now);
    let asked = app.ask(Ask::LoginStanding {
        profile: profile.clone(),
        after,
    });
    app.keep(async move {
        let answer = asked.await;
        let mut logins = logins.write();
        let known = logins.get(&profile).copied();
        match answer {
            Answer::LoginStanding { standing } => {
                let next = answered(known.as_ref(), after, standing, Instant::now());
                logins.insert(profile, next);
            }
            // Asked again when a draft shows it next.
            _ => match known {
                Some(Login::LoggingIn(terminal)) if !ends_wait(terminal, after, None) => {}
                _ => {
                    logins.remove(&profile);
                }
            },
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
        assert!(!stale(Some(&Login::LoggingIn(7)), now));
    }

    #[test]
    fn only_opencode_says_what_runs_without_a_login() {
        for agent in Agent::ALL {
            assert_eq!(
                without_login(agent).is_some(),
                agent == Agent::OpenCode,
                "{agent:?}"
            );
        }
    }

    #[test]
    fn asking_while_a_login_terminal_is_waited_for_keeps_waiting_for_it() {
        assert_eq!(asking(None, None), Login::Asking);
        assert_eq!(asking(None, Some(7)), Login::LoggingIn(7));
        assert_eq!(
            asking(Some(&Login::LoggingIn(7)), None),
            Login::LoggingIn(7)
        );
        assert_eq!(
            asking(Some(&Login::LoggingIn(7)), Some(8)),
            Login::LoggingIn(8)
        );
    }

    #[test]
    fn a_wait_ends_when_its_terminal_does_or_an_answer_says_logged_in() {
        use LoginStanding::*;
        let now = Instant::now();
        let waiting = Login::LoggingIn(7);
        let is = |standing| Login::Is(standing, now);
        assert_eq!(
            answered(Some(&waiting), Some(7), LoggedOut, now),
            is(LoggedOut)
        );
        assert_eq!(answered(Some(&waiting), None, LoggedIn, now), is(LoggedIn));
        assert_eq!(answered(Some(&waiting), None, LoggedOut, now), waiting);
        assert_eq!(answered(Some(&waiting), Some(6), Unknown, now), waiting);
        assert_eq!(
            answered(Some(&Login::Asking), None, LoggedOut, now),
            is(LoggedOut)
        );
        assert_eq!(answered(None, Some(6), LoggedOut, now), is(LoggedOut));
    }
}
