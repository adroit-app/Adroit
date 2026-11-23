use std::path::PathBuf;

use super::*;
use crate::delegation::service::fixtures::{conversation_watch, watching};
use crate::settings::Settings;

#[tokio::test]
async fn the_conversation_another_agent_s_session_is_tagged_with_is_recorded() {
    let next = "223e4567-e89b-12d3-a456-426614174000";
    for agent in [Agent::Claude, Agent::Pi, Agent::OpenCode] {
        let tags = crate::sessions::conversation_tags(next);
        let (_dir, context, mut record, native) = watching(agent, None, tags);
        Watcher::learn_tagged_conversation(&context, &mut record, &native)
            .await
            .unwrap();
        assert_eq!(record.conversation.as_deref(), Some(next), "{agent:?}");
        let kept = context.store.lock().unwrap().known(record.id).unwrap();
        assert_eq!(kept.conversation.as_deref(), Some(next), "{agent:?}");
    }
}

#[tokio::test]
async fn a_session_tagged_with_no_conversation_keeps_the_recorded_one() {
    let (_dir, context, mut record, native) = watching(Agent::Claude, None, Vec::new());
    let known = record.conversation.clone();
    Watcher::learn_tagged_conversation(&context, &mut record, &native)
        .await
        .unwrap();
    assert!(known.is_some());
    assert_eq!(record.conversation, known);
}

#[tokio::test]
async fn a_changed_codex_title_updates_the_record_used_by_the_next_delivery() {
    let next = "123e4567-e89b-12d3-a456-426614174001";
    let (_dir, context, mut record, native) = conversation_watch(&format!("Codex · {next}"));
    assert!(
        Watcher::default()
            .follow_conversation(&context, &mut record, &native)
            .await
            .unwrap()
    );
    assert_eq!(record.conversation.as_deref(), Some(next));
}

#[tokio::test]
async fn codex_delivery_waits_for_the_current_title_and_discards_an_old_lookup() {
    let next = "123e4567-e89b-12d3-a457-426614174001";
    let title = format!("codex | {}...", &next[..29]);
    let (_dir, context, mut record, mut native) = conversation_watch(&title);
    let mut watcher = Watcher::default();
    // Keep the test independent of an installed provider.
    watcher.looking.insert(record.id);
    assert!(
        !watcher
            .follow_conversation(&context, &mut record, &native)
            .await
            .unwrap()
    );
    watcher
        .resolved
        .insert(record.id, ("old title".into(), next.into()));
    assert!(
        !watcher
            .follow_conversation(&context, &mut record, &native)
            .await
            .unwrap()
    );
    assert_ne!(record.conversation.as_deref(), Some(next));
    watcher.resolved.insert(record.id, (title, next.into()));
    assert!(
        watcher
            .follow_conversation(&context, &mut record, &native)
            .await
            .unwrap()
    );
    assert_eq!(record.conversation.as_deref(), Some(next));
    assert_eq!(
        context
            .store
            .lock()
            .unwrap()
            .known(record.id)
            .unwrap()
            .conversation,
        record.conversation
    );
    assert!(
        watcher
            .follow_conversation(&context, &mut record, &native)
            .await
            .unwrap()
    );
    native.title = None;
    assert!(
        !watcher
            .follow_conversation(&context, &mut record, &native)
            .await
            .unwrap()
    );
}

fn claude_screen(name: &str) -> Vec<String> {
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

#[test]
fn a_dialog_waits_on_the_user_at_once_and_a_silent_screen_stalls_only_after_a_while() {
    assert_eq!(
        readiness(Some(StartupReadiness::AwaitingTrust), Duration::ZERO),
        Some(Readiness::Waiting)
    );
    assert_eq!(readiness(None, STALL - Duration::from_millis(1)), None);
    assert_eq!(readiness(None, STALL), Some(Readiness::Stalled));
    assert_eq!(
        readiness(Some(StartupReadiness::Ready), STALL * 10),
        Some(Readiness::Ready)
    );
}

#[test]
fn adroit_s_channel_warning_is_left_to_the_user_unless_the_settings_allow_confirming_it() {
    let warning = claude_screen("dev-channels");
    assert_eq!(
        channel_warning_keys(Agent::Claude, false, false, &warning),
        None
    );
    assert_eq!(
        channel_warning_keys(Agent::Claude, true, false, &warning),
        Some(&b"\r"[..])
    );
    assert_eq!(
        channel_warning_keys(Agent::Claude, true, true, &warning),
        None
    );
}

#[test]
fn no_screen_but_the_warning_naming_adroit_s_channel_alone_is_confirmed() {
    let other = claude_screen("dev-channels")
        .into_iter()
        .map(|line| {
            line.replace(
                "Channels: server:adroit",
                "Channels: server:adroit, plugin:other@market",
            )
        })
        .collect::<Vec<_>>();
    for screen in [
        other,
        claude_screen("dev-channels-confirmed"),
        claude_screen("permissions"),
    ] {
        assert_eq!(
            channel_warning_keys(Agent::Claude, true, false, &screen),
            None
        );
    }
    assert_eq!(
        channel_warning_keys(Agent::Codex, true, false, &claude_screen("dev-channels")),
        None
    );
}

#[test]
fn a_message_is_handed_over_in_the_login_its_recipient_started_in_after_a_restart() {
    use crate::delegation::store::NewSession;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite3");
    let launch = Settings::parse(
        "[[profiles]]\nid = \"codex\"\nagent = \"codex\"\n\n[[profiles]]\nid = \"codex-work\"\nagent = \"codex\"\nhome = \"/w/codex\"\n",
    )
    .unwrap();
    let new = |profile: &str, home: Option<&str>| NewSession {
        parent: None,
        agent: Agent::Codex,
        profile: profile.parse().unwrap(),
        home: home.map(PathBuf::from),
        name: profile.into(),
        cwd: "/repo".into(),
        launch: launch.clone(),
        model: None,
        permissions: Default::default(),
        started_ms: 1,
        name_given: false,
        code: None,
    };
    let to = {
        let mut store = Store::open(&path).unwrap();
        let (from, _) = store.register(new("codex", None)).unwrap();
        let (to, _) = store.register(new("codex-work", Some("/w/codex"))).unwrap();
        store.send(from, to, "review").unwrap();
        to
    };

    // Read again as a work host started anew does, asking only what was
    // recorded, never the settings in force.
    let mut store = Store::open(&path).unwrap();
    let (message, _, started) = next_delivery(&mut store, to, true).unwrap().unwrap();

    assert_eq!(message.body, "review");
    assert_eq!(started.home, Some("/w/codex".into()));
}

#[test]
fn a_session_of_an_agent_its_settings_gave_no_profile_is_handed_its_messages() {
    use crate::delegation::store::NewSession;
    let dir = tempfile::tempdir().unwrap();
    let launch = Settings::parse(
        "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n\n[[profiles]]\nid = \"claude-work\"\nagent = \"claude\"\n",
    )
    .unwrap();
    let new = |agent: Agent| {
        let started = crate::programs::starting(&launch, Some(agent), None).unwrap();
        NewSession {
            parent: None,
            agent,
            profile: started.profile,
            home: started.home,
            name: agent.key().into(),
            cwd: "/repo".into(),
            launch: launch.clone(),
            model: None,
            permissions: Default::default(),
            started_ms: 1,
            name_given: false,
            code: None,
        }
    };
    let mut store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
    let (from, _) = store.register(new(Agent::Claude)).unwrap();
    let (to, _) = store.register(new(Agent::Codex)).unwrap();
    store.send(from, to, "review").unwrap();

    let (message, _, handed) = next_delivery(&mut store, to, true).unwrap().unwrap();

    assert_eq!(message.body, "review");
    assert_eq!((handed.agent(), handed.home), (Agent::Codex, None));
}
