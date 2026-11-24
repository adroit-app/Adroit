//! The words a session proposes to pass on as the user's: the banner that
//! says there are some, and the question the user answers once they have
//! read them.

use std::collections::BTreeSet;

use adroit_model::{EnvironmentId, MessageId, Relay, SessionCode};
use adroit_protocol::Request;

use super::confirm::Ask;
use crate::effect::Effect;
use crate::host::Hosts;
use crate::state::ViewState;

/// The proposals the banner was put away on, and those the user was told of
/// outside the window.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Relays {
    later: BTreeSet<MessageId>,
    told: BTreeSet<MessageId>,
}

/// Every proposal there is, with the environment its session runs in.
fn proposed(hosts: &Hosts) -> impl Iterator<Item = (&EnvironmentId, &Relay)> {
    hosts.each().flat_map(|(environment, _)| {
        crate::tree::relays(hosts.work(environment))
            .iter()
            .map(move |relay| (environment, relay))
    })
}

/// What the banner says is proposed, oldest first: what was not left for
/// later.
pub fn relay_banner<'a>(hosts: &'a Hosts, relays: &Relays) -> Vec<(&'a EnvironmentId, &'a Relay)> {
    proposed(hosts)
        .filter(|(_, relay)| !relays.later.contains(&relay.id))
        .collect()
}

/// Ask about the oldest proposal not left for later, beginning anew once all
/// were.
pub(crate) fn review_relay(state: &mut ViewState) -> Vec<Effect> {
    let later = &mut state.overlays.relays.later;
    if proposed(&state.hosts).all(|(_, relay)| later.contains(&relay.id)) {
        later.clear();
    }
    let chosen = proposed(&state.hosts)
        .find(|(_, relay)| !later.contains(&relay.id))
        .map(|(environment, relay)| (environment.clone(), relay.clone()));
    let Some((environment, relay)) = chosen else {
        return crate::chrome::status_line::notice(
            &mut state.chrome.status,
            "No session proposes to pass your words on".into(),
        );
    };
    state.overlays.confirm = Some(Ask::Relay {
        environment,
        relay,
        left_out: Vec::new(),
    });
    Vec::new()
}

pub(crate) fn relay_later(state: &mut ViewState) -> Vec<Effect> {
    state.overlays.relays.later = proposed(&state.hosts).map(|(_, r)| r.id).collect();
    Vec::new()
}

/// Leave what `ask` asks to decide of later, if it is words.
pub(crate) fn leave_relay(state: &mut ViewState, ask: &Ask) {
    if let Ask::Relay { relay, .. } = ask {
        state.overlays.relays.later.insert(relay.id);
    }
}

pub(crate) fn toggle_relay(state: &mut ViewState, code: &SessionCode) -> Vec<Effect> {
    if let Some(Ask::Relay { left_out, .. }) = state.overlays.confirm.as_mut() {
        match left_out.iter().position(|left| left == code) {
            Some(at) => drop(left_out.remove(at)),
            None => left_out.push(code.clone()),
        }
    }
    Vec::new()
}

pub(crate) fn approve_relay(ask: &Ask) -> Vec<Effect> {
    let Ask::Relay {
        environment, relay, ..
    } = ask
    else {
        return Vec::new();
    };
    vec![Effect::Host {
        environment: environment.clone(),
        request: Request::ApproveRelay {
            id: relay.id,
            to: ask.relay_to(),
        },
    }]
}

pub(crate) fn decline_relay(state: &mut ViewState) -> Vec<Effect> {
    match state.overlays.confirm.take() {
        Some(Ask::Relay {
            environment, relay, ..
        }) => vec![Effect::Host {
            environment,
            request: Request::DeclineRelay { id: relay.id },
        }],
        other => {
            state.overlays.confirm = other;
            Vec::new()
        }
    }
}

/// What is proposed in `environment` that was not yet told of, as a session
/// waiting on the user is told of: by name and by what it waits for.
pub(crate) fn proposed_anew(
    state: &mut ViewState,
    environment: &EnvironmentId,
) -> Vec<(String, String)> {
    let waiting: Vec<(MessageId, String)> = crate::tree::relays(state.hosts.work(environment))
        .iter()
        .map(|relay| (relay.id, relay.from_name.clone()))
        .collect();
    let told = &mut state.overlays.relays.told;
    waiting
        .into_iter()
        .filter(|(id, _)| told.insert(*id))
        .map(|(_, name)| (name, "your approval to pass your words on".into()))
        .collect()
}

#[cfg(test)]
mod tests {
    use adroit_model::Key;
    use adroit_model::RelayTo;

    use super::*;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::testing::{here, patch, snapshot};

    fn relay(from: &str) -> Relay {
        let to = |code: &str, name: &str| RelayTo {
            code: code.parse().unwrap(),
            name: name.into(),
        };
        Relay {
            id: MessageId::new(),
            from: "k7f3".parse().unwrap(),
            from_name: from.into(),
            to: vec![to("r4pj", "api"), to("95nd", "web")],
            body: "use tabs".into(),
            proposed_ms: 1,
        }
    }

    fn proposing(relays: &[&Relay]) -> Scenario {
        Scenario::new().event(snapshot()).event(patched(2, relays))
    }

    fn patched(seq: u64, relays: &[&Relay]) -> adroit_protocol::testing::PatchBuilder {
        let listed = relays
            .iter()
            .fold(adroit_protocol::testing::listed(), |listed, relay| {
                listed.relay((*relay).clone())
            });
        patch(seq).listed(listed)
    }

    fn shown(run: &Scenario) -> usize {
        let state = run.state();
        relay_banner(&state.hosts, &state.overlays.relays).len()
    }

    fn host(request: Request) -> Effect {
        Effect::Host {
            environment: here(),
            request,
        }
    }

    #[test]
    fn words_proposed_are_approved_only_once_the_user_asked_to_read_them() {
        let one = relay("lead");
        let run = proposing(&[&one]);
        assert_eq!(shown(&run), 1);
        assert_eq!(run.state().overlays.confirm, None);
        let keyed = run.send(Msg::HeldKey(Key::new("Enter")));
        assert!(!keyed.last_effects().iter().any(|e| matches!(
            e,
            Effect::Host {
                request: Request::ApproveRelay { .. },
                ..
            }
        )));

        let read = proposing(&[&one]).send(Msg::RelayReview);
        assert!(read.last_effects().is_empty());
        let approved = read.send(Msg::HeldKey(Key::new("y")));
        assert_eq!(
            approved.last_effects(),
            [host(Request::ApproveRelay {
                id: one.id,
                to: vec!["r4pj".parse().unwrap(), "95nd".parse().unwrap()],
            })]
        );
        assert_eq!(approved.state().overlays.confirm, None);
    }

    #[test]
    fn words_are_approved_for_the_sessions_left_in_and_not_for_none() {
        let one = relay("lead");
        let left_in = proposing(&[&one])
            .send(Msg::RelayReview)
            .send(Msg::RelayToggle("r4pj".parse().unwrap()));
        let none = left_in
            .send(Msg::RelayToggle("95nd".parse().unwrap()))
            .send(Msg::ConfirmAnswer(true));
        assert!(none.last_effects().is_empty());
        assert!(none.state().overlays.confirm.is_some());
        let again = none
            .send(Msg::RelayToggle("95nd".parse().unwrap()))
            .send(Msg::ConfirmAnswer(true));
        assert_eq!(
            again.last_effects(),
            [host(Request::ApproveRelay {
                id: one.id,
                to: vec!["95nd".parse().unwrap()],
            })]
        );
    }

    #[test]
    fn words_read_are_declined_or_left_to_decide_of_later() {
        let one = relay("lead");
        let read = || proposing(&[&one]).send(Msg::RelayReview);
        let declined = read().send(Msg::HeldKey(Key::new("d")));
        assert_eq!(
            declined.last_effects(),
            [host(Request::DeclineRelay { id: one.id })]
        );
        assert_eq!(declined.state().overlays.confirm, None);
        let left = read().send(Msg::HeldKey(Key::new("Escape")));
        assert!(left.last_effects().is_empty());
        assert_eq!(left.state().overlays.confirm, None);
    }

    #[test]
    fn the_key_that_declines_words_leaves_another_question_asked() {
        let run = Scenario::new()
            .send(Msg::Confirm(Box::new(Ask::RestartDaemon(here()))))
            .send(Msg::HeldKey(Key::new("d")));
        assert!(run.last_effects().is_empty());
        assert!(run.state().overlays.confirm.is_some());
    }

    #[test]
    fn the_banner_put_away_shows_again_only_once_more_is_proposed() {
        let (one, two) = (relay("lead"), relay("review"));
        let run = proposing(&[&one]).send(Msg::RelayLater);
        assert_eq!(shown(&run), 0);
        let run = run.event(patched(3, &[&one, &two]));
        assert_eq!(shown(&run), 1);
        assert_eq!(reading(&run.send(Msg::RelayReview)), Some(two.id));
    }

    fn reading(run: &Scenario) -> Option<MessageId> {
        match &run.state().overlays.confirm {
            Some(Ask::Relay { relay, .. }) => Some(relay.id),
            _ => None,
        }
    }

    #[test]
    fn words_left_for_later_give_way_to_the_next_and_come_round_again() {
        let (one, two) = (relay("lead"), relay("review"));
        let mut run = proposing(&[&one, &two]);
        for want in [one.id, two.id, one.id] {
            run = run.send(Msg::RelayReview);
            assert_eq!(reading(&run), Some(want));
            run = run.send(Msg::HeldKey(Key::new("Escape")));
        }
    }

    #[test]
    fn asking_to_read_words_while_none_are_proposed_says_so() {
        let run = Scenario::new().send(Msg::RelayReview);
        assert_eq!(run.state().overlays.confirm, None);
        assert_eq!(
            run.state().chrome.status.line.as_deref(),
            Some("No session proposes to pass your words on")
        );
    }

    #[test]
    fn words_proposed_are_told_of_once_as_a_session_waiting_is_while_the_settings_ask() {
        let one = relay("lead");
        let told = |run: &Scenario| {
            run.effects()
                .iter()
                .filter_map(|e| match e {
                    Effect::NotifyWaiting(waiting) => Some(waiting.len()),
                    _ => None,
                })
                .sum::<usize>()
        };
        let run = |settings: &str| {
            Scenario::new()
                .event(crate::config::testing::configured(settings))
                .event(snapshot().listed(adroit_protocol::testing::listed()))
        };
        let on = run("notify_waiting = true\n").event(patched(2, &[&one]));
        assert_eq!(told(&on), 1);
        assert_eq!(told(&on.event(patched(3, &[&one]))), 1);
        assert_eq!(told(&run("").event(patched(2, &[&one]))), 0);
    }
}
