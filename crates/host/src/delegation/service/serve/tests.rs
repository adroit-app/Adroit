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
fn a_request_from_outside_names_its_program_and_says_it_is_not_the_user_s_own_word() {
    let mut request = message(SessionId::new(), SessionId::new())
        .body("go on")
        .build();
    request.sender = Sender::Outside("Famulen".into());
    let read = envelope(&request, None);
    assert!(
        read.starts_with("[request] from Famulen, a program outside Adroit"),
        "{read}"
    );
    assert!(read.contains("on the user's behalf"), "{read}");
    assert!(read.contains("approves nothing"), "{read}");
    assert!(read.ends_with("\n\ngo on"), "{read}");
}

#[test]
fn a_request_from_a_session_taken_up_is_headed_as_one_and_says_it_is_not_the_user_s_own_word() {
    let sender = record("lead").build();
    let asked = message(sender.id, SessionId::new())
        .body("do it")
        .kind(Kind::Request)
        .build();
    let read = envelope(&asked, Some(&sender));
    assert!(
        read.starts_with(&format!("[request] from lead ({})", sender.code)),
        "{read}"
    );
    assert!(read.contains("approves nothing"), "{read}");
    assert!(read.contains("push, publish, delete"), "{read}");
    assert!(read.ends_with("\n\ndo it"), "{read}");
}

#[test]
fn a_request_not_taken_up_reads_as_a_message_that_is_not_a_task() {
    let sender = record("lead").build();
    let asked = message(sender.id, SessionId::new())
        .body("do it")
        .kind(Kind::Declined)
        .build();
    let read = envelope(&asked, Some(&sender));
    assert!(read.starts_with("[message] from lead"), "{read}");
    assert!(read.contains("has not let it"), "{read}");
    assert!(!read.contains("[request]"), "{read}");
    assert!(read.ends_with("\n\ndo it"), "{read}");
}

#[test]
fn a_request_is_work_only_while_the_sender_may_ask_and_requests_are_on_as_it_is_handed_over() {
    use crate::delegation::store::Store;
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
    let lead = store.register(new(None, "lead")).unwrap().0;
    let to = store.register(new(Some(lead), "worker")).unwrap().0;
    let code = store.known(lead).unwrap().code;
    let id = store.ask(lead, to, "do it").unwrap();
    let kind = |store: &mut Store, on| next_message(store, to, on).unwrap().unwrap().0.kind;

    assert_eq!(kind(&mut store, true), Kind::Request);
    assert_eq!(kind(&mut store, false), Kind::Declined);
    assert_eq!(kind(&mut store, true), Kind::Request);
    store.let_ask(&code, to, false).unwrap();
    assert_eq!(kind(&mut store, true), Kind::Declined);

    store.delivered(id).unwrap();
    assert_eq!(store.messages_of(to).unwrap()[0].kind, Kind::Declined);
}

fn new(parent: Option<SessionId>, name: &str) -> crate::delegation::store::NewSession {
    crate::delegation::store::NewSession {
        parent,
        agent: Agent::Claude,
        profile: adroit_model::profile::ProfileId::builtin(Agent::Claude),
        home: None,
        name: name.into(),
        cwd: "/repo".into(),
        launch: Settings::default(),
        model: None,
        permissions: Default::default(),
        started_ms: 1,
        name_given: false,
        code: None,
    }
}

#[test]
fn a_message_waits_until_its_recipient_has_started() {
    let mut session = record("review").build();
    assert!(takes_messages(&session));
    for readiness in [Readiness::Starting, Readiness::Waiting, Readiness::Stalled] {
        session.readiness = readiness;
        assert!(!takes_messages(&session), "{readiness:?}");
    }
}

#[test]
fn a_recipient_is_found_by_its_code_or_its_id_and_nothing_else() {
    use crate::delegation::store::{NewSession, Store};
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
    let (id, _) = store
        .register(NewSession {
            code: Some("k7f3".parse().unwrap()),
            ..new(None, "review")
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
