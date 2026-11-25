//! `adroit session answer`, `interrupt` and `end`: what a program outside
//! Adroit does to a session in the user's stead, with a key the user gave it.

use std::time::Duration;

use adroit_model::{MessageId, SessionId, State};
use serde_json::{Map, Value, json};

use super::{Found, Standing, cut, find, keys};
use crate::delegation::app;
use crate::delegation::questions::{self, Decision, Kind, Question, TRUST, Taken};

/// The exit status when the question named is not asked any more.
const NOT_ASKED: i32 = 4;
/// The exit status when the one who asks may not.
const NOT_ALLOWED: i32 = 5;

/// How long an agent is given to go on once it was answered.
const TAKES: Duration = Duration::from_secs(5);
/// How long a session is given to stop its turn.
const STOPS: Duration = Duration::from_secs(5);
const LOOK_EVERY: Duration = Duration::from_millis(250);
/// How much of what was answered is kept beside the session, in bytes.
const KEPT: usize = 2 * 1024;

/// What a question is answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Given {
    Allow,
    Deny {
        reason: Option<String>,
    },
    /// One for each question asked at once, in their order.
    Replies(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Act {
    Answer { question: MessageId, given: Given },
    Interrupt,
    End,
}

/// Why an act was not done, with its exit status.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Refused {
    pub text: String,
    pub code: i32,
}

impl Refused {
    fn new(text: impl Into<String>, code: i32) -> Self {
        Self {
            text: text.into(),
            code,
        }
    }
}

impl From<String> for Refused {
    fn from(text: String) -> Self {
        Self::new(text, 1)
    }
}

/// What `question` is decided by `given`; why not when it is no answer to it.
fn decide(question: &Question, given: &Given) -> Result<Decision, String> {
    match (question.kind, given) {
        (_, Given::Deny { reason }) => Ok(Decision::Deny {
            message: reason.clone(),
        }),
        (Kind::Permission, Given::Allow) => Ok(Decision::Allow { input: None }),
        (Kind::Plan, Given::Allow) => Ok(Decision::Allow {
            input: Some(question.input.clone()),
        }),
        (Kind::Question, Given::Replies(replies)) => {
            let asked: Vec<&str> = question.input["questions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|q| q["question"].as_str())
                .collect();
            if asked.is_empty() || asked.len() != replies.len() {
                return Err(format!(
                    "it asks {} and {} were answered; give a --reply for each",
                    asked.len(),
                    replies.len()
                ));
            }
            if replies.iter().any(|reply| reply.trim().is_empty()) {
                return Err("a reply is empty".into());
            }
            let answers: Map<String, Value> = asked
                .into_iter()
                .map(String::from)
                .zip(replies.iter().map(|reply| json!(reply)))
                .collect();
            let mut input = question.input.clone();
            input["answers"] = Value::Object(answers);
            Ok(Decision::Allow { input: Some(input) })
        }
        (Kind::Question, Given::Allow) => {
            Err("it is a question, answered with --reply or --deny".into())
        }
        (Kind::Permission | Kind::Plan, Given::Replies(_)) => {
            Err("it is answered with --allow or --deny".into())
        }
    }
}

/// What is told of an answer beside the session.
fn told(question: &Question, given: &Given) -> String {
    let what = match given {
        Given::Allow => "Allowed".to_string(),
        Given::Deny { reason: None } => "Denied".into(),
        // Claude Code leaves, and is told no reason.
        Given::Deny { .. } if question.tool == TRUST => "Denied".into(),
        Given::Deny {
            reason: Some(reason),
        } => format!("Denied, saying \"{reason}\","),
        Given::Replies(replies) => format!("Answered \"{}\" to", replies.join("\", \"")),
    };
    let input = question.input.to_string();
    let (input, _) = cut(&input, KEPT);
    format!("{what} {}: {input}", question.tool)
}

/// The name of the program that holds `key`, when the user lets those that
/// hold one act.
fn allowed(
    settings: Result<crate::settings::Settings, String>,
    path: &std::path::Path,
    key: &str,
    now_ms: u64,
) -> Result<String, Refused> {
    let on = settings
        .map_err(|e| Refused::new(e, NOT_ALLOWED))?
        .outside
        .steer;
    if !on {
        return Err(Refused::new(
            "the user has not let programs outside Adroit answer and stop sessions (Settings, General)",
            NOT_ALLOWED,
        ));
    }
    keys::holder(path, key, now_ms)
        .map_err(|e| Refused::new(e, NOT_ALLOWED))?
        .ok_or_else(|| {
            Refused::new(
                "the key on standard input is none the user gave",
                NOT_ALLOWED,
            )
        })
}

async fn record(session: SessionId, by: String, what: String) -> Result<(), String> {
    app::with_store(&app::root(), move |store| {
        store.record_act(&by, session, &what)
    })
    .await
    .map_err(|e| e.to_string())
}

async fn answer(
    asked: &str,
    found: &Found,
    session: SessionId,
    by: String,
    id: MessageId,
    given: &Given,
) -> Result<String, Refused> {
    let gone = || Refused::new("that is not asked any more", NOT_ASKED);
    if !waits(&found.standing) {
        return Err(gone());
    }
    let question = questions::named(session, id).ok_or_else(gone)?;
    let decision = decide(&question, given)?;
    match questions::answer(session, id, decision).await {
        Taken::Yes => {}
        Taken::NotAsked => return Err(gone()),
        Taken::Failed(why) => {
            let tried = outcome("", &format!("answer {}", question.tool), &Err(why.clone()));
            record(session, by, tried).await?;
            return Err(Refused::new(why, 1));
        }
    }
    let what = told(&question, given);
    record(session, by, what.clone()).await?;
    // It was seen to go, and what a session that starts asks next is no sign
    // that it was not.
    if question.tool == TRUST {
        return Ok(what);
    }
    let deadline = tokio::time::Instant::now() + TAKES;
    loop {
        let standing = find(asked).await?.standing;
        // One that asks anew has taken the answer to what it asked before.
        if !waits(&standing) || questions::of(session).is_some() {
            return Ok(what);
        }
        if tokio::time::Instant::now() > deadline {
            return Err(Refused::new(
                "the agent was told the answer, and still waits on the user",
                1,
            ));
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

fn waits(standing: &Standing) -> bool {
    matches!(standing, Standing::Runs(State::Waiting(_)))
}

/// The terminal the session `found` runs in.
fn terminal(found: &Found) -> Result<u128, Refused> {
    match found.standing {
        Standing::Ended => Err(Refused::new("the session has ended", 1)),
        Standing::Runs(_) => crate::sessions::id_of(&found.session.id)
            .ok_or_else(|| Refused::new("the session has no terminal", 1)),
    }
}

/// Whether a turn runs that can be interrupted: not while anything waits on
/// the user, whom the key that interrupts would answer.
fn interruptible(standing: &Standing) -> Result<(), &'static str> {
    match standing {
        Standing::Runs(State::Working) => Ok(()),
        Standing::Runs(State::Waiting(_)) => {
            Err("the session waits on the user, and has no turn running to interrupt")
        }
        _ => Err("the session has no turn running to interrupt"),
    }
}

async fn interrupt(
    asked: &str,
    found: &Found,
    session: SessionId,
    by: String,
) -> Result<String, Refused> {
    let refused = |why: &str| Refused::new(why, 1);
    interruptible(&found.standing).map_err(refused)?;
    let terminal = terminal(found)?;
    let adapter = adroit_agents::adapter(found.session.agent);
    let screen = crate::daemon::daemon()
        .visible(terminal)
        .await
        .map_err(|e| refused(&e.to_string()))?;
    if adapter.dialog_open(&screen) {
        return Err(refused("a dialog of the agent's has the keys"));
    }
    let keys: Vec<_> = adapter
        .interrupt_keys()
        .iter()
        .map(|name| adroit_protocol::Typing::Key {
            name: (*name).into(),
        })
        .collect();
    let stopped = match crate::asks::type_prompt(terminal, &keys).await {
        Ok(()) => stopped(asked).await,
        Err(error) => Err(error),
    };
    record(
        session,
        by,
        outcome("Interrupted the turn", "interrupt the turn", &stopped),
    )
    .await?;
    stopped.map(|()| "interrupted".into()).map_err(Into::into)
}

/// What is told of an act beside the session: that it was `done`, or that it
/// was tried `to` no avail.
fn outcome(done: &str, to: &str, acted: &Result<(), String>) -> String {
    match acted {
        Ok(()) => done.into(),
        Err(why) => format!("Tried to {to}: {why}"),
    }
}

/// Waits until the turn of the session `asked` names has stopped.
async fn stopped(asked: &str) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + STOPS;
    loop {
        tokio::time::sleep(LOOK_EVERY).await;
        if find(asked).await?.standing != Standing::Runs(State::Working) {
            return Ok(());
        }
        if tokio::time::Instant::now() > deadline {
            return Err("the turn was told to stop, and still runs".into());
        }
    }
}

async fn end(found: &Found, session: SessionId, by: String) -> Result<String, Refused> {
    let terminal = terminal(found)?;
    let ended = crate::daemon::daemon()
        .end(terminal)
        .await
        .map_err(|e| e.to_string());
    record(
        session,
        by,
        outcome("Ended the session", "end the session", &ended),
    )
    .await?;
    ended?;
    app::ended(&app::root(), session)
        .await
        .map_err(|e| Refused::new(e.to_string(), 1))?;
    Ok("ended".into())
}

/// Does `act` to the session `asked` names for the program that holds `key`:
/// what was done.
pub(super) async fn run(asked: &str, act: Act, key: &str) -> Result<String, Refused> {
    let by = allowed(
        crate::settings::try_load(),
        &keys::path(),
        key,
        adroit_model::time::now_ms(),
    )?;
    let found = find(asked).await?;
    let session = found
        .session
        .delegation
        .ok_or_else(|| Refused::new(format!("{asked} has no record of Adroit's"), 1))?;
    match act {
        Act::Answer { question, given } => {
            answer(asked, &found, session, by, question, &given).await
        }
        Act::Interrupt => interrupt(asked, &found, session, by).await,
        Act::End => end(&found, session, by).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn question(kind: Kind, input: Value) -> Question {
        Question {
            id: MessageId::new(),
            kind,
            tool: "T".into(),
            input,
            dir: None,
            asked_ms: 1,
        }
    }

    fn two() -> Question {
        question(
            Kind::Question,
            json!({"questions": [{"question": "Colour?"}, {"question": "Size?"}]}),
        )
    }

    fn replies(given: &[&str]) -> Given {
        Given::Replies(given.iter().map(|r| r.to_string()).collect())
    }

    #[test]
    fn a_permission_or_a_plan_is_allowed_or_denied_for_this_once() {
        for kind in [Kind::Permission, Kind::Plan] {
            let asked = question(kind, json!({"command": "ls"}));
            // Claude Code goes on with a plan only when given it back.
            let input = (kind == Kind::Plan).then(|| asked.input.clone());
            assert_eq!(decide(&asked, &Given::Allow), Ok(Decision::Allow { input }));
            let given = Given::Deny {
                reason: Some("not now".into()),
            };
            assert_eq!(
                decide(&asked, &given),
                Ok(Decision::Deny {
                    message: Some("not now".into())
                })
            );
            assert!(decide(&asked, &replies(&["yes"])).is_err());
        }
    }

    #[test]
    fn a_question_is_answered_with_one_reply_for_each_it_asks() {
        let Ok(Decision::Allow { input: Some(input) }) = decide(&two(), &replies(&["Red", "any"]))
        else {
            panic!("no answer");
        };
        assert_eq!(input["answers"], json!({"Colour?": "Red", "Size?": "any"}));
        assert_eq!(input["questions"], two().input["questions"]);
        for wrong in [&["Red"][..], &[], &["Red", " "], &["a", "b", "c"]] {
            assert!(decide(&two(), &replies(wrong)).is_err(), "{wrong:?}");
        }
        assert!(decide(&two(), &Given::Allow).is_err());
        assert!(decide(&two(), &Given::Deny { reason: None }).is_ok());
    }

    fn settings(steer: bool) -> Result<Settings, String> {
        let mut settings = Settings::default();
        settings.outside.steer = steer;
        Ok(settings)
    }

    #[test]
    fn only_the_holder_of_a_key_acts_and_only_while_the_user_lets_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k").join("keys.json");
        let (key, _) = keys::issue(&path, "Famulen", 1).unwrap();
        assert_eq!(
            allowed(settings(true), &path, &key, 2).as_deref(),
            Ok("Famulen")
        );
        let code = |settings, key: &str| allowed(settings, &path, key, 3).unwrap_err().code;
        assert_eq!(code(settings(false), &key), NOT_ALLOWED);
        assert_eq!(code(settings(true), "adroit-key-x"), NOT_ALLOWED);
        assert_eq!(code(settings(true), ""), NOT_ALLOWED);
        assert_eq!(code(Err("broken".into()), &key), NOT_ALLOWED);
        assert!(!Settings::default().outside.steer);
    }

    #[test]
    fn a_session_is_interrupted_only_while_its_turn_runs_and_nothing_waits_on_the_user() {
        use adroit_model::Wait;
        assert!(interruptible(&Standing::Runs(State::Working)).is_ok());
        for standing in [
            Standing::Runs(State::Waiting(Wait::Permission)),
            Standing::Runs(State::Idle),
            Standing::Runs(State::Done),
            Standing::Runs(State::Background),
            Standing::Runs(State::Starting),
            Standing::Ended,
        ] {
            assert!(interruptible(&standing).is_err(), "{standing:?}");
        }
    }

    #[test]
    fn an_act_that_failed_is_told_as_tried_with_why() {
        let told = |acted| outcome("Ended the session", "end the session", &acted);
        assert_eq!(told(Ok(())), "Ended the session");
        assert_eq!(told(Err("gone".into())), "Tried to end the session: gone");
    }

    #[test]
    fn a_folder_not_trusted_is_told_without_a_reason_no_one_was_given() {
        let mut asked = question(Kind::Permission, json!({"folder": "/f"}));
        asked.tool = TRUST.into();
        let given = Given::Deny {
            reason: Some("no".into()),
        };
        assert_eq!(
            told(&asked, &given),
            r#"Denied TrustFolder: {"folder":"/f"}"#
        );
    }

    #[test]
    fn what_was_answered_is_told_with_what_was_asked() {
        let asked = question(Kind::Permission, json!({"command": "ls"}));
        assert_eq!(
            told(&asked, &Given::Allow),
            r#"Allowed T: {"command":"ls"}"#
        );
        assert!(
            told(&two(), &replies(&["Red", "any"])).starts_with("Answered \"Red\", \"any\" to T:")
        );
    }
}
