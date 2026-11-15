use std::path::Path;
use std::sync::Arc;

use adroit_term::Daemon;
use serde_json::json;

use super::*;
use crate::delegation::protocol::Command;
use crate::delegation::service::fixtures::conversation_watch;
use crate::settings::Settings;
use crate::testing::record;

/// A terminal host serving in `dir`, and a terminal running there.
async fn serving(dir: &Path) -> (Daemon, u128) {
    let socket = dir.join("daemon.sock");
    let serving = socket.clone();
    crate::environment::spawn(async move {
        Daemon::serve(&serving, Duration::from_millis(20), |_| {})
            .await
            .unwrap()
    });
    let daemon = Daemon::new(socket);
    for _ in 0..100 {
        if daemon.sessions().await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let terminal = daemon
        .spawn(adroit_term::Spawn {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "read answer".into()],
            cwd: dir.into(),
            env: Vec::new(),
            tags: Vec::new(),
            cols: 80,
            rows: 24,
        })
        .await
        .unwrap();
    (daemon, terminal)
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs its terminal's program with /bin/sh")]
async fn a_session_closing_itself_is_answered_before_it_ends_and_is_recorded_ended() {
    let dir = tempfile::Builder::new()
        .prefix("adroit-")
        .tempdir_in(adroit_sys::local::temp_dir())
        .unwrap();
    adroit_sys::fs::set_mode(dir.path(), 0o700).unwrap();
    let (_state, mut context, caller, _) = conversation_watch("review");
    let (daemon, terminal) = serving(dir.path()).await;
    context.daemon = daemon.clone();
    context
        .store
        .lock()
        .unwrap()
        .attach(caller.id, terminal, None)
        .unwrap();
    let caller = context.store.lock().unwrap().known(caller.id).unwrap();

    close(&context, &caller).await.unwrap();

    let runs =
        |sessions: &[adroit_term::Session]| sessions.iter().any(|s| s.id == terminal && !s.exited);
    assert!(runs(&daemon.sessions().await.unwrap()));
    let mut ended = false;
    for _ in 0..200 {
        let gone = !runs(&daemon.sessions().await.unwrap());
        if gone
            && context
                .store
                .lock()
                .unwrap()
                .known(caller.id)
                .unwrap()
                .ended
        {
            ended = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ended, "the session still runs or is not recorded ended");
}

#[tokio::test]
async fn a_session_with_no_terminal_yet_cannot_close_itself() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let refused = close(&context, &caller).await.unwrap_err();

    assert_eq!(refused.code, Code::Refused);
}

#[test]
fn a_codex_subagent_cannot_close_its_session() {
    assert!(Command::Close.acts());
}

fn renaming(name: &str) -> protocol::Rename {
    protocol::Rename { name: name.into() }
}

#[tokio::test]
async fn a_session_is_not_renamed_by_its_agent_while_renaming_is_switched_off() {
    let (_dir, mut context, caller, _) = conversation_watch("review");
    context.settings = Arc::new(|| Ok(Settings::parse("rename_sessions = false\n").unwrap()));

    let refused = rename(&context, &caller, renaming("レビュー"))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(refused.message.contains("keep"), "{}", refused.message);
}

#[tokio::test]
async fn a_name_a_person_gave_is_not_changed_by_the_session_s_agent() {
    let (_dir, context, mut caller, _) = conversation_watch("review");
    caller.name_given = true;

    let refused = rename(&context, &caller, renaming("レビュー"))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(refused.message.contains("review"), "{}", refused.message);
}

#[tokio::test]
async fn a_name_without_letters_or_digits_is_refused() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let refused = rename(&context, &caller, renaming(" ！？ "))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Invalid);
}

fn tasks(ops: Value) -> protocol::TimelineTasks {
    serde_json::from_value(serde_json::json!({ "ops": ops })).unwrap()
}

#[tokio::test]
async fn a_timeline_call_is_journaled_and_answered_with_the_trees() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let answer = record_timeline(
        &context,
        &caller,
        tasks(json!([
            {"op": "add", "id": "goal", "title": "Review"},
            {"op": "add", "id": "a", "parent": "goal", "title": "Read", "status": "completed"},
            {"op": "add", "id": "b", "parent": "goal", "title": "Write"},
        ])),
    )
    .await
    .unwrap();

    let text = answer.as_str().unwrap();
    assert!(text.contains("50.00% completed"), "{text}");
    let kept = context
        .store
        .lock()
        .unwrap()
        .timeline_of(&caller.code)
        .unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].ops.len(), 3);
}

#[tokio::test]
async fn a_timeline_call_says_which_changes_it_could_not_make() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let answer = record_timeline(
        &context,
        &caller,
        tasks(json!([
            {"op": "add", "id": "goal", "title": "Review"},
            {"op": "update", "id": "ghost", "status": "completed"},
        ])),
    )
    .await
    .unwrap();

    let text = answer.as_str().unwrap();
    assert!(text.contains("Not made:\nghost: no such task"), "{text}");
}

#[tokio::test]
async fn a_timeline_call_that_makes_no_change_is_refused_and_not_journaled() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let refused = record_timeline(
        &context,
        &caller,
        tasks(json!([{"op": "remove", "id": "ghost"}])),
    )
    .await
    .unwrap_err();

    assert_eq!(refused.code, Code::Invalid);
    assert!(refused.message.contains("ghost"), "{}", refused.message);
    let kept = context
        .store
        .lock()
        .unwrap()
        .timeline_of(&caller.code)
        .unwrap();
    assert!(kept.is_empty());
}

fn note_of(answer: Value) -> Option<String> {
    answer["note"].as_str().map(String::from)
}

#[tokio::test]
async fn after_a_clear_the_agent_is_asked_to_name_its_session_for_the_work_that_follows() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let note = note_of(prompted(&context, &caller, true).await.unwrap()).unwrap();

    assert!(note.contains("`review`"), "{note}");
    assert!(note.contains("cleared"), "{note}");
    assert!(note.contains("call `rename_session`"), "{note}");
}

#[tokio::test]
async fn at_each_prompt_the_agent_keeps_the_name_unless_the_work_turned() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();

    assert!(note.contains("`review`"), "{note}");
    assert!(note.contains("`rename_session`"), "{note}");
    assert!(note.contains("keep the name"), "{note}");
}

#[tokio::test]
async fn a_clear_hands_a_name_the_user_gave_back_to_the_agent() {
    let (_dir, context, mut caller, _) = conversation_watch("review");
    context
        .store
        .lock()
        .unwrap()
        .rename(caller.id, "review", true)
        .unwrap();
    caller.name_given = true;

    let note = note_of(prompted(&context, &caller, true).await.unwrap()).unwrap();

    assert!(note.contains("call `rename_session`"), "{note}");
    let kept = context.store.lock().unwrap().known(caller.id).unwrap();
    assert!(!kept.name_given);
}

#[tokio::test]
async fn a_session_whose_name_its_agent_may_not_change_is_not_asked_to_name_it() {
    let (_dir, mut context, mut caller, _) = conversation_watch("review");
    caller.name_given = true;
    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();
    assert!(!note.contains("`rename_session`"), "{note}");

    caller.name_given = false;
    context.settings = Arc::new(|| Ok(Settings::parse("rename_sessions = false\n").unwrap()));
    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();
    assert!(!note.contains("`rename_session`"), "{note}");
}

#[tokio::test]
async fn at_each_prompt_a_session_with_nothing_on_its_timeline_is_reminded_to_record_its_plan() {
    let (_dir, context, mut caller, _) = conversation_watch("review");
    caller.name_given = true;

    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();

    assert!(note.contains("`timeline_tasks`"), "{note}");
}

#[tokio::test]
async fn once_its_agent_has_recorded_a_timeline_a_prompt_no_longer_reminds_it() {
    let (_dir, context, mut caller, _) = conversation_watch("review");
    record_timeline(
        &context,
        &caller,
        tasks(json!([{"op": "add", "id": "goal", "title": "Review"}])),
    )
    .await
    .unwrap();

    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();
    assert!(!note.contains("`timeline_tasks`"), "{note}");

    caller.name_given = true;
    assert_eq!(
        note_of(prompted(&context, &caller, false).await.unwrap()),
        None
    );
}

#[tokio::test]
async fn a_start_while_sessions_opening_sessions_is_switched_off_is_refused_and_leaves_nothing() {
    let (dir, mut context, caller, _) = conversation_watch("review");
    let worktrees = dir.path().join("repo");
    std::fs::create_dir(&worktrees).unwrap();
    context.settings = Arc::new(|| Ok(Settings::parse("[delegation]\nenabled = false\n").unwrap()));
    let request = serde_json::from_value(serde_json::json!({
        "prompt": "review the change",
        "dir": worktrees,
        "worktree": true,
    }))
    .unwrap();

    let refused = start(&context, &caller, request).await.unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(
        refused.message.contains("carry on yourself"),
        "{}",
        refused.message
    );
    assert!(
        refused.message.contains("ask the user"),
        "{}",
        refused.message
    );
    let sessions = context.call(|store| store.sessions()).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(std::fs::read_dir(&worktrees).unwrap().count(), 0);
}

#[tokio::test]
async fn sessions_are_listed_with_their_codes_and_the_code_of_the_one_that_opened_them() {
    let opener = record("opener").build();
    let opened = record("opened").parent(opener.id).build();
    let listed = listed(
        vec![opener.clone(), opened.clone()],
        &protocol::Sessions::default(),
    )
    .await
    .unwrap();

    assert_eq!(listed[0]["code"], opener.code.as_str());
    assert_eq!(listed[1]["code"], opened.code.as_str());
    assert_eq!(listed[1]["opened_by"]["code"], opener.code.as_str());
}
