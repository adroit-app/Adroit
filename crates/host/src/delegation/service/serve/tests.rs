use super::*;
use crate::settings::Settings;
use crate::testing::{message, record};

#[test]
fn a_message_names_its_sender_by_its_code_and_where_it_works() {
    let sender = record("review").build();
    let message = message(sender.id, SessionId::new()).body("done").build();
    let read = envelope(&message, Some(&sender));
    assert!(
        read.starts_with(&format!("[message] from review ({})", sender.code)),
        "{read}"
    );
    assert!(read.ends_with("\ndone"), "{read}");
    let unknown = envelope(&message, None);
    assert!(unknown.contains(&sender.id.to_string()), "{unknown}");
}

#[test]
fn a_recipient_is_found_by_its_code_or_its_id_and_nothing_else() {
    use crate::delegation::store::{NewSession, Store};
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
    let (id, _) = store
        .register(NewSession {
            parent: None,
            agent: Agent::Claude,
            profile: adroit_model::profile::ProfileId::builtin(Agent::Claude),
            home: None,
            name: "review".into(),
            cwd: "/repo".into(),
            launch: Settings::default(),
            model: None,
            permissions: Default::default(),
            started_ms: 1,
            name_given: false,
            code: Some("k7f3".parse().unwrap()),
        })
        .unwrap();

    assert_eq!(addressed(&store, "k7f3").unwrap(), id);
    assert_eq!(addressed(&store, &id.to_string()).unwrap(), id);
    for unknown in ["zzzz", "review", ""] {
        assert_eq!(
            addressed(&store, unknown).unwrap_err().code,
            Code::NotFound,
            "{unknown}"
        );
    }
}

#[test]
fn a_codex_subagent_cannot_act_for_the_session_it_runs_in() {
    let codex = record("review")
        .agent(Agent::Codex)
        .conversation("thread-a")
        .build();
    assert!(from_own_conversation(&codex, Some("thread-a")).is_ok());
    assert_eq!(
        from_own_conversation(&codex, Some("thread-b"))
            .unwrap_err()
            .code,
        Code::Refused
    );
    let fresh = record("review").agent(Agent::Codex).build();
    assert!(from_own_conversation(&fresh, Some("thread-b")).is_ok());
    let claude = record("review").conversation("x").build();
    assert!(from_own_conversation(&claude, Some("y")).is_ok());
}
