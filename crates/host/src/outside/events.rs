//! Each change of how a session stands, kept in order so that a program
//! outside Adroit can follow them and take up where it left off.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Found, Listed, Standing, gone, opener, records};
use crate::delegation::{Session as Record, app, questions};
use adroit_model::State;

/// How long one ask for changes waits for the first.
pub(super) const PATIENCE: Duration = Duration::from_secs(25);
const LOOK_EVERY: Duration = Duration::from_millis(250);
/// How long a watch waits before it asks a work host that went away again.
const AGAIN: Duration = Duration::from_secs(1);
/// The most changes told at once, which fit what a caller reads of a command.
const AT_ONCE: usize = 50;

/// What tells one standing from another: a change of anything else of a
/// session is none here.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    state: String,
    waiting_for: Option<String>,
    question: Option<String>,
}

impl Key {
    /// What the change kept as `body` left a session at.
    fn kept(body: &Value) -> Self {
        let text = |value: &Value| value.as_str().map(String::from);
        Self {
            state: text(&body["state"]).unwrap_or_default(),
            waiting_for: text(&body["waiting_for"]),
            question: text(&body["question"]["id"]),
        }
    }

    /// Whether the session's turn has ended.
    fn over(&self) -> bool {
        matches!(self.state.as_str(), "idle" | "done" | "failed")
    }
}

/// How each session was last told to stand, by the folder of its records.
static LAST: LazyLock<Mutex<HashMap<PathBuf, HashMap<String, Key>>>> =
    LazyLock::new(Default::default);

/// How each session in `root` last stood, read once from what is kept.
async fn last(root: &PathBuf) -> Result<HashMap<String, Key>, String> {
    let held = {
        let known = LAST.lock().unwrap_or_else(|e| e.into_inner());
        known.get(root).cloned()
    };
    if let Some(held) = held {
        return Ok(held);
    }
    let kept = app::with_store(root, |store| store.last_standings())
        .await
        .map_err(|e| e.to_string())?;
    let read: HashMap<String, Key> = kept
        .into_iter()
        .map(|(session, body)| (session, Key::kept(&body)))
        .collect();
    let mut known = LAST.lock().unwrap_or_else(|e| e.into_inner());
    Ok(known.entry(root.clone()).or_insert(read).clone())
}

/// What is kept of a session that now stands as `listed`.
fn body(listed: &Listed) -> Value {
    let mut body = json!(listed);
    // Told of nothing that was said since, it would mislead.
    if let Some(body) = body.as_object_mut() {
        body.remove("last_active_ms");
    }
    let asked = listed.id.parse().ok().and_then(questions::of);
    body["question"] = match asked.filter(|_| listed.state == "waiting") {
        Some(q) => json!({"id": q.id, "kind": q.kind, "tool": q.tool}),
        None => Value::Null,
    };
    body
}

/// One change told at a time, so they are kept in the order they are told.
static TELLING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Keeps how `listed` stands, if it stands otherwise than when last told.
async fn tell(root: &PathBuf, listed: &Listed) -> Result<(), String> {
    let _one = TELLING.lock().await;
    let body = body(listed);
    let key = Key::kept(&body);
    if last(root).await?.get(&listed.id) == Some(&key) {
        return Ok(());
    }
    let session = listed.id.clone();
    app::with_store(root, move |store| store.record_standing(&session, &body))
        .await
        .map_err(|e| e.to_string())?;
    // Only once it is kept: one that was not is told again.
    let mut known = LAST.lock().unwrap_or_else(|e| e.into_inner());
    known
        .entry(root.clone())
        .or_default()
        .insert(listed.id.clone(), key);
    Ok(())
}

/// Keeps every change since they were last told, `sessions` being those that
/// run and how they stand.
pub(crate) async fn follow(sessions: &[adroit_model::Session]) -> Result<(), String> {
    let root = app::root();
    let records = records().await?;
    for session in sessions {
        let record = records.iter().find(|r| Some(r.id) == session.delegation);
        let found = Found {
            standing: Standing::of(session.state.clone(), record.map(|r| r.readiness)),
            opened_by: opener(&records, session.parent),
            session: session.clone(),
        };
        tell(&root, &found.listed(None)).await?;
    }
    let runs = |r: &Record| sessions.iter().any(|s| s.delegation == Some(r.id));
    let told = last(&root).await?;
    for record in records.iter().filter(|r| r.ended && !runs(r)) {
        let ended = told
            .get(&record.id.to_string())
            .is_some_and(|key| key.state != "ended");
        if ended {
            tell(&root, &gone(record, &records).listed(None)).await?;
        }
    }
    Ok(())
}

/// Keeps that the session of `record` has begun a turn, when it was last told
/// to have ended one: a turn may begin and end between two reads of how the
/// sessions stand, and would pass untold.
pub(crate) async fn began(
    record: &Record,
    records: &[Record],
    native: &adroit_term::Session,
) -> Result<(), String> {
    let reported = native.status.as_ref().map(|s| s.state.as_str());
    if reported != Some("working") {
        return Ok(());
    }
    let root = app::root();
    let told = last(&root).await?;
    if !told
        .get(&record.id.to_string())
        .is_some_and(|key| key.over())
    {
        return Ok(());
    }
    let Some(session) = crate::sessions::agents(std::slice::from_ref(native)).next() else {
        return Ok(());
    };
    let found = Found {
        standing: Standing::Runs(State::Working),
        opened_by: opener(records, session.parent),
        session,
    };
    tell(&root, &found.listed(None)).await
}

/// Changes told at once, and where to go on from.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Told {
    pub events: Vec<Value>,
    pub next: i64,
    /// Whether changes since were let go of before they were asked for.
    pub missed: bool,
}

/// The changes kept in `store` after `since`, or none and where the next one
/// follows when `since` is none.
fn told(
    store: &crate::delegation::store::Store,
    since: Option<i64>,
) -> crate::delegation::Result<Told> {
    let (oldest, newest) = store.standings_kept()?;
    let Some(since) = since else {
        return Ok(Told {
            events: Vec::new(),
            next: newest,
            missed: false,
        });
    };
    let missed = since + 1 < oldest || since > newest;
    let from = if since > newest { newest } else { since };
    let kept = store.standings_since(from, AT_ONCE)?;
    let next = kept.last().map_or(from, |(seq, _, _)| *seq);
    let events = kept
        .into_iter()
        .map(|(seq, at_ms, mut body)| {
            body["seq"] = json!(seq);
            body["at_ms"] = json!(at_ms);
            body
        })
        .collect();
    Ok(Told {
        events,
        next,
        missed,
    })
}

/// The changes after `since`, waited for up to `patience` while there are
/// none.
pub(super) async fn changes(since: Option<i64>, patience: Duration) -> Result<Told, String> {
    let deadline = tokio::time::Instant::now() + patience;
    let root = app::root();
    loop {
        let told = app::with_store(&root, move |store| told(store, since))
            .await
            .map_err(|e| e.to_string())?;
        let some = !told.events.is_empty() || told.missed || since.is_none();
        if some || tokio::time::Instant::now() + LOOK_EVERY > deadline {
            return Ok(told);
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

/// The lines a watch prints of `told`.
fn lines(told: &Told) -> Vec<String> {
    let missed = told
        .missed
        .then(|| json!({"missed": true, "next": told.next}).to_string());
    missed
        .into_iter()
        .chain(told.events.iter().map(Value::to_string))
        .collect()
}

/// Prints each change as it comes, from after `since`, until what reads them
/// goes. Returns the process exit code.
pub(super) fn watch(mut since: Option<i64>) -> i32 {
    let mut away = false;
    loop {
        let from = since.map(|seq| seq.to_string());
        let mut ask = vec!["session", "watch", "--once"];
        if let Some(from) = &from {
            ask.extend(["--since", from]);
        }
        let ask = ask.into_iter().map(String::from).collect();
        let told = crate::work_host::ran(ask, None).and_then(|ran| match ran.code {
            0 => serde_json::from_str::<Told>(&ran.stdout).map_err(|e| e.to_string()),
            _ => Err(ran.stderr),
        });
        match told {
            Ok(told) => {
                away = false;
                since = Some(told.next);
                let mut out = std::io::stdout().lock();
                for line in lines(&told) {
                    if writeln!(out, "{line}").is_err() {
                        return 0;
                    }
                }
                if out.flush().is_err() {
                    return 0;
                }
            }
            Err(error) => {
                if !std::mem::replace(&mut away, true) {
                    eprintln!("adroit: {error}; trying again");
                }
                std::thread::sleep(AGAIN);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delegation::store::Store;

    fn store(states: &[&str]) -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
        for state in states {
            store
                .record_standing("s", &json!({"id": "s", "state": state}))
                .unwrap();
        }
        (dir, store)
    }

    fn states(told: &Told) -> Vec<(i64, &str)> {
        told.events
            .iter()
            .map(|e| (e["seq"].as_i64().unwrap(), e["state"].as_str().unwrap()))
            .collect()
    }

    #[test]
    fn a_follower_takes_up_where_it_left_off_and_starts_at_the_present() {
        let (_dir, store) = store(&["working", "done"]);
        let present = told(&store, None).unwrap();
        assert_eq!((present.events.len(), present.next), (0, 2));
        let all = told(&store, Some(0)).unwrap();
        assert_eq!(states(&all), [(1, "working"), (2, "done")]);
        assert!(!all.missed);
        assert!(all.events[0]["at_ms"].as_u64().unwrap() > 0);
        let rest = told(&store, Some(1)).unwrap();
        assert_eq!((states(&rest), rest.next), (vec![(2, "done")], 2));
        assert!(told(&store, Some(2)).unwrap().events.is_empty());
    }

    #[tokio::test]
    async fn a_change_that_could_not_be_kept_is_kept_once_it_can_be() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let path = app::database(&root);
        drop(Store::open(&path).unwrap());
        let listed = |state: State| {
            Found {
                standing: Standing::Runs(state),
                opened_by: None,
                session: adroit_model::testing::session("worker").build(),
            }
            .listed(None)
        };
        tell(&root, &listed(State::Working)).await.unwrap();
        let sqlite = rusqlite::Connection::open(&path).unwrap();
        sqlite
            .execute_batch("ALTER TABLE standings RENAME TO away;")
            .unwrap();
        tell(&root, &listed(State::Done)).await.unwrap_err();
        sqlite
            .execute_batch("ALTER TABLE away RENAME TO standings;")
            .unwrap();

        tell(&root, &listed(State::Done)).await.unwrap();

        let kept = told(&Store::open(&path).unwrap(), Some(0)).unwrap();
        let states: Vec<_> = states(&kept).into_iter().map(|(_, state)| state).collect();
        assert_eq!(states, ["working", "done"]);
    }

    #[test]
    fn many_changes_are_told_a_part_at_a_time() {
        let (_dir, store) = store(&["working"; AT_ONCE + 1]);
        let first = told(&store, Some(0)).unwrap();
        assert_eq!((first.events.len(), first.next), (AT_ONCE, AT_ONCE as i64));
        assert_eq!(told(&store, Some(first.next)).unwrap().events.len(), 1);
    }

    #[test]
    fn a_follower_is_told_what_it_asks_for_is_no_longer_or_never_was_kept() {
        let (_dir, store) = store(&["working"]);
        let ahead = told(&store, Some(9)).unwrap();
        assert!(ahead.missed);
        assert_eq!((ahead.events.len(), ahead.next), (0, 1));
        assert_eq!(lines(&ahead), [r#"{"missed":true,"next":1}"#]);
    }

    #[test]
    fn only_the_state_what_is_waited_for_and_the_question_tell_standings_apart() {
        let key = |value: Value| Key::kept(&value);
        let waits = json!({"state": "waiting", "waiting_for": "permission",
            "question": {"id": "q1"}, "name": "a"});
        let mut renamed = waits.clone();
        renamed["name"] = json!("b");
        assert_eq!(key(waits.clone()), key(renamed));
        let mut next = waits.clone();
        next["question"]["id"] = json!("q2");
        assert_ne!(key(waits), key(next));
        assert!(key(json!({"state": "done"})).over());
        assert!(!key(json!({"state": "working"})).over());
    }
}
