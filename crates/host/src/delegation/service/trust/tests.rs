use std::sync::Mutex;

use super::*;
use crate::delegation::service::fixtures::watching;

fn recorded(name: &str) -> Vec<String> {
    let path = format!(
        "{}/../agents/tests/screens/{name}.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .lines()
        .map(String::from)
        .collect()
}

/// A Claude Code that asks whether to trust a folder.
struct Asks {
    screen: Mutex<Vec<String>>,
    pressed: Mutex<Vec<&'static str>>,
    /// Whether it asks no more.
    over: Mutex<bool>,
    /// Whether its keys move the selection, and whether Enter chooses.
    moves: bool,
    chooses: bool,
    /// What the first key brings up in its place.
    becomes: Option<Vec<String>>,
}

impl Asks {
    fn new() -> Self {
        Self {
            screen: Mutex::new(recorded("trust")),
            pressed: Mutex::default(),
            over: Mutex::default(),
            moves: true,
            chooses: true,
            becomes: None,
        }
    }

    fn pressed(&self) -> Vec<&'static str> {
        self.pressed.lock().unwrap().clone()
    }
}

fn moved(rows: &[String]) -> Vec<String> {
    rows.iter()
        .map(|row| match row.trim() {
            "❯ No, exit" => "   No, exit".into(),
            "No, exit" => " ❯ No, exit".into(),
            "Yes, I trust this folder" => " ❯ Yes, I trust this folder".into(),
            "❯ Yes, I trust this folder" => "   Yes, I trust this folder".into(),
            _ => row.clone(),
        })
        .collect()
}

impl Screen for Asks {
    async fn reports(&self) -> Result<bool, String> {
        Ok(!*self.over.lock().unwrap())
    }

    async fn shows(&self) -> Result<Vec<String>, String> {
        Ok(self.screen.lock().unwrap().clone())
    }

    async fn press(&self, key: &'static str) -> Result<(), String> {
        self.pressed.lock().unwrap().push(key);
        let mut screen = self.screen.lock().unwrap();
        if let Some(other) = &self.becomes {
            *screen = other.clone();
            *self.over.lock().unwrap() = true;
        } else if key == "Enter" && self.chooses {
            *screen = recorded("input");
            *self.over.lock().unwrap() = true;
        } else if key != "Enter" && self.moves {
            *screen = moved(&screen);
        }
        Ok(())
    }
}

fn question() -> TrustAsked {
    trust_asked(&recorded("trust")).unwrap()
}

#[tokio::test]
async fn trusting_selects_the_option_to_trust_before_it_is_confirmed() {
    let asks = Asks::new();
    assert_eq!(answer(&asks, &question(), true).await, Ok(()));
    assert_eq!(asks.pressed(), ["ArrowDown", "Enter"]);
}

#[tokio::test]
async fn not_trusting_confirms_the_option_to_leave_and_moves_to_it_when_it_is_not_selected() {
    let asks = Asks::new();
    assert_eq!(answer(&asks, &question(), false).await, Ok(()));
    assert_eq!(asks.pressed(), ["Enter"]);

    let asks = Asks::new();
    let other = moved(&recorded("trust"));
    *asks.screen.lock().unwrap() = other;
    assert_eq!(answer(&asks, &question(), false).await, Ok(()));
    assert_eq!(asks.pressed(), ["ArrowUp", "Enter"]);
}

#[tokio::test]
async fn nothing_is_typed_on_a_screen_that_asks_anything_else() {
    for name in [
        "trust-with-settings",
        "input",
        "permissions",
        "dev-channels",
    ] {
        let asks = Asks::new();
        *asks.screen.lock().unwrap() = recorded(name);
        let answered = answer(&asks, &question(), true).await;
        assert_eq!(answered, Err(Unanswered::Gone), "{name}");
        assert!(asks.pressed().is_empty(), "{name}");
    }
}

#[tokio::test]
async fn nothing_is_confirmed_once_another_dialog_took_the_question_s_place() {
    let asks = Asks {
        becomes: Some(recorded("permissions")),
        ..Asks::new()
    };
    let answered = answer(&asks, &question(), true).await;
    assert_eq!(answered, Err(Unanswered::Gone));
    assert_eq!(asks.pressed(), ["ArrowDown"]);
}

/// Whether `asks` is still not answered after a while.
async fn unanswered(asks: &Asks) -> bool {
    let given = Duration::from_millis(300);
    tokio::time::timeout(given, answer(asks, &question(), true))
        .await
        .is_err()
}

#[tokio::test]
async fn nothing_is_confirmed_while_the_selection_did_not_move() {
    let asks = Asks {
        moves: false,
        ..Asks::new()
    };
    assert!(unanswered(&asks).await);
    assert_eq!(asks.pressed(), ["ArrowDown"]);
}

#[tokio::test]
async fn an_answer_is_taken_only_once_the_agent_reports_to_ask_no_more() {
    let asks = Asks {
        chooses: false,
        ..Asks::new()
    };
    assert!(unanswered(&asks).await);
    // A screen too short to show the question tells nothing of it.
    let asks = Asks {
        chooses: false,
        ..Asks::new()
    };
    let (answered, ()) = tokio::join!(unanswered(&asks), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        asks.screen.lock().unwrap().truncate(6);
    });
    assert!(answered);
}

#[test]
fn a_repository_git_tells_nothing_of_is_known_by_its_mark() {
    let dir = tempfile::tempdir().unwrap();
    let below = dir.path().join("a").join("b");
    std::fs::create_dir_all(&below).unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    assert_eq!(marked(&below), Some(dir.path().join(".git")));
}

/// A session of `agent`, and the screen on which it asks whether to trust the
/// folder it runs in.
fn starting(
    agent: Agent,
) -> (
    tempfile::TempDir,
    Daemon,
    Session,
    adroit_term::Session,
    Vec<String>,
) {
    let (dir, context, record, mut native) = watching(agent, None, Vec::new());
    native.status = Some(adroit_term::Status {
        state: "blocked".into(),
        kind: Some("permission".into()),
        msg: Some(REPORTED.into()),
    });
    let named = folder(&record).display().to_string();
    let screen = recorded("trust")
        .into_iter()
        .map(|row| row.replace("/Users/me/.local/share/famulen/notebook", &named))
        .collect();
    (dir, context.daemon, record, native, screen)
}

#[test]
fn a_trust_question_is_told_only_when_reported_and_shown_of_the_session_s_own_folder() {
    let (_dir, _, record, native, screen) = starting(Agent::Claude);
    assert!(asked(&record, &native, &screen).is_some());
    assert!(asked(&record, &native, &recorded("trust")).is_none());
    assert!(asked(&record, &native, &recorded("input")).is_none());
    let mut silent = native.clone();
    silent.status = None;
    assert!(asked(&record, &silent, &screen).is_none());
    let mut other = native;
    other.status.as_mut().unwrap().msg = Some("approve Bash".into());
    assert!(asked(&record, &other, &screen).is_none());
    let (_dir, _, record, native, screen) = starting(Agent::Codex);
    assert!(asked(&record, &native, &screen).is_none());
}

/// The question told of `session`, once it is.
async fn told(session: SessionId) -> questions::Question {
    loop {
        if let Some(question) = questions::of(session) {
            return question;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn a_trust_question_is_held_as_one_permission_until_it_is_not_asked() {
    let (_dir, daemon, record, native, screen) = starting(Agent::Claude);
    let held = follow(None, &daemon, &record, &native, &screen);
    let question = told(record.id).await;
    assert_eq!(question.kind, questions::Kind::Permission);
    assert_eq!(question.tool, TRUST);
    assert_eq!(question.input["folder"], json!(folder(&record)));
    let dialog = question.input["dialog"].as_str().unwrap();
    assert!(dialog.contains("Quick safety check"));

    let held = follow(held, &daemon, &record, &native, &moved(&screen));
    assert!(held.is_some());
    assert_eq!(questions::of(record.id).unwrap(), question, "held once");

    let none = follow(held, &daemon, &record, &native, &recorded("input"));
    assert!(none.is_none());
    for _ in 0..100 {
        if questions::of(record.id).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(questions::of(record.id).is_none());

    let _again = follow(None, &daemon, &record, &native, &screen);
    assert_eq!(told(record.id).await.id, question.id);
}

#[test]
fn a_question_s_id_is_its_terminal_s_and_its_words_own() {
    let warned = trust_asked(&recorded("trust-with-settings")).unwrap();
    assert_eq!(id(1, &question()), id(1, &question()));
    assert_ne!(id(1, &question()), id(2, &question()));
    assert_ne!(id(1, &question()), id(1, &warned));
}

#[tokio::test]
async fn a_failed_answer_lets_the_question_be_asked_again() {
    let (_dir, daemon, record, native, screen) = starting(Agent::Claude);
    let held = follow(None, &daemon, &record, &native, &screen);
    let id = told(record.id).await.id;
    // The terminal host is not there, so the screen cannot be read.
    let allow = Decision::Allow { input: None };
    let taken = questions::answer(record.id, id, allow).await;
    assert!(matches!(taken, questions::Taken::Failed(_)));
    let held = held.unwrap();
    while !held.task.is_finished() {
        tokio::task::yield_now().await;
    }
    let none = follow(Some(held), &daemon, &record, &native, &screen);
    assert!(
        none.is_none(),
        "what was read may be from before the answer"
    );
    let _held = follow(none, &daemon, &record, &native, &screen);
    assert_eq!(told(record.id).await.id, id);
}

#[test]
fn what_trusting_covers_is_told_as_claude_code_takes_it() {
    let home = Path::new("/Users/me");
    let plain = scope(Path::new("/Users/me/notes"), home, None);
    assert!(plain.contains("later sessions") && plain.contains("every folder below it"));
    let repository = scope(
        Path::new("/Users/me/src/api/web"),
        home,
        Some(Path::new("/Users/me/src/api/.git")),
    );
    assert!(repository.contains("whole repository of /Users/me/src/api/.git"));
    assert!(scope(home, home, None).contains("this session only"));
}
