//! What the work host's tests start from on top of the shared builders:
//! adroit's records of the sessions it runs and of the messages between
//! them.
//!
//! Each builder starts from the plainest record and names only what a test
//! sets, so a field added to a record changes this file alone.

use adroit_model::Agent;
use adroit_model::profile::ProfileId;

use crate::delegation::{Message, MessageId, Readiness, Session, SessionId};

/// Adroit's record of a running Claude Code session called `name` in
/// `/repo`, which the user started and which is ready.
pub fn record(name: &str) -> RecordBuilder {
    RecordBuilder(Session {
        id: SessionId::new(),
        parent: None,
        agent: Agent::Claude,
        profile: ProfileId::builtin(Agent::Claude),
        home: None,
        name: name.into(),
        cwd: "/repo".into(),
        terminal: None,
        conversation: None,
        readiness: Readiness::Ready,
        started_ms: 0,
        ended: false,
        model: None,
        permissions: Default::default(),
        generation: None,
        interrupted: false,
        brought_back_by: None,
        name_given: false,
        code: adroit_model::SessionCode::random(),
    })
}

/// A [`Session`] record being built; see [`record`].
#[derive(Debug, Clone)]
#[must_use]
pub struct RecordBuilder(Session);

impl RecordBuilder {
    pub fn agent(mut self, agent: Agent) -> Self {
        self.0.agent = agent;
        self
    }

    /// The conversation the session holds.
    pub fn conversation(mut self, id: impl Into<String>) -> Self {
        self.0.conversation = Some(id.into());
        self
    }

    /// Opened by the session recorded as `parent`.
    pub fn parent(mut self, parent: SessionId) -> Self {
        self.0.parent = Some(parent);
        self
    }

    pub fn ended(mut self) -> Self {
        self.0.ended = true;
        self
    }

    pub fn build(self) -> Session {
        self.0
    }
}

/// A message without words `sender` sent `recipient`, not delivered yet.
pub fn message(sender: SessionId, recipient: SessionId) -> MessageBuilder {
    MessageBuilder(Message {
        id: MessageId::new(),
        sender,
        recipient,
        body: String::new(),
        sent_ms: 0,
        delivered: false,
    })
}

/// A [`Message`] being built; see [`message`].
#[derive(Debug, Clone)]
#[must_use]
pub struct MessageBuilder(Message);

impl MessageBuilder {
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.0.body = body.into();
        self
    }

    pub fn delivered(mut self) -> Self {
        self.0.delivered = true;
        self
    }

    pub fn build(self) -> Message {
        self.0
    }
}

/// A git repository at `dir/api` with one commit, made without the user's
/// own git configuration signing it.
pub(crate) fn repository(dir: &std::path::Path) -> std::path::PathBuf {
    let source = dir.join("api");
    std::fs::create_dir(&source).unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["commit", "-q", "--allow-empty", "-m", "first"],
    ] {
        let status = std::process::Command::new("git")
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&source)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .status()
            .unwrap();
        assert!(status.success());
    }
    source
}
