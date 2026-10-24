//! What the work host's tests start from on top of the shared builders:
//! adroit's records of the sessions it runs and of the messages between
//! them.
//!
//! Each builder starts from the plainest record and names only what a test
//! sets, so a field added to a record changes this file alone.

use adroit_model::Agent;
use adroit_model::profile::ProfileId;

use crate::delegation::{Message, MessageId, MessageKind, Readiness, Session, SessionId};

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
        reported: false,
        ended: false,
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

    pub fn ended(mut self) -> Self {
        self.0.ended = true;
        self
    }

    pub fn build(self) -> Session {
        self.0
    }
}

/// A message without words `sender` sent `recipient`, not delivered yet;
/// `None` sends it as adroit.
pub fn message(sender: Option<SessionId>, recipient: SessionId) -> MessageBuilder {
    MessageBuilder(Message {
        id: MessageId::new(),
        sender,
        recipient,
        kind: MessageKind::Message,
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
    pub fn kind(mut self, kind: MessageKind) -> Self {
        self.0.kind = kind;
        self
    }

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
