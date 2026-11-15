//! The recorded sessions and contexts the service's tests watch.
use std::sync::{Arc, Mutex};

use adroit_model::Agent;
use adroit_term::Daemon;

use super::Context;
use crate::delegation::{Session, store::Store};
use crate::settings::Settings;

pub(super) fn conversation_watch(
    title: &str,
) -> (tempfile::TempDir, Context, Session, adroit_term::Session) {
    watching(Agent::Codex, Some(title), Vec::new())
}

/// A recorded session of `agent` that knows a conversation, and the
/// daemon session it runs in, with `title` and `tags`.
pub(super) fn watching(
    agent: Agent,
    title: Option<&str>,
    tags: Vec<String>,
) -> (tempfile::TempDir, Context, Session, adroit_term::Session) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
    let (id, _) = store
        .register(crate::delegation::store::NewSession {
            parent: None,
            agent,
            profile: adroit_model::profile::ProfileId::builtin(agent),
            home: None,
            name: "review".into(),
            cwd: dir.path().into(),
            launch: Settings::default(),
            model: None,
            permissions: Default::default(),
            started_ms: 1,
            name_given: false,
            code: None,
        })
        .unwrap();
    store
        .learn_conversation(id, "123e4567-e89b-12d3-a456-426614174000")
        .unwrap();
    let record = store.known(id).unwrap();
    let context = Context {
        store: Arc::new(Mutex::new(store)),
        models: Default::default(),
        daemon: Daemon::new(dir.path().join("unused.sock")),
        program: "/adroit".into(),
        root: dir.path().into(),
        settings: Arc::new(|| Ok(Settings::default())),
    };
    let native = adroit_term::Session {
        id: 1,
        tags,
        title: title.map(Into::into),
        exited: false,
        status: None,
        sequence: 1,
    };
    (dir, context, record, native)
}
