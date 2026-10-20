//! Versioned, instance-local requests. Credentials never appear in events.
use std::path::PathBuf;

use adroit_model::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::store::ControlError;

pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 1_048_576;
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub id: RequestId,
    pub credential: String,
    /// Human clients may act as a selected session. Agent credentials cannot.
    pub actor: Option<SessionRef>,
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    RegisterLeader {
        id: LogicalSessionId,
        agent: Agent,
        cwd: PathBuf,
    },
    AttachSession {
        identity: SessionRef,
        cwd: PathBuf,
        terminal_id: String,
        conversation_id: String,
    },
    FailStart {
        identity: SessionRef,
        reason: String,
    },
    Grant {
        identity: SessionRef,
    },
    Snapshot,
    Mailbox {
        after: u64,
        limit: u32,
    },
    ClaimDelivery {
        message_id: MessageId,
    },
    RetryDelivery {
        message_id: MessageId,
    },
    AcceptDelivery {
        message_id: MessageId,
    },
    Events {
        after: u64,
        limit: u32,
    },
    OpenRun {
        id: RunId,
        brief: RunBrief,
    },
    PutTask {
        plan: TaskPlan,
    },
    QueueTask {
        version: TaskVersion,
    },
    SendMessage {
        message: AgentMessage,
    },
    Acknowledge {
        message_id: MessageId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u32,
    pub id: Option<RequestId>,
    pub result: Result<Value, ControlError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPage {
    /// Advances over filtered events as well as visible ones.
    pub next_cursor: u64,
    pub events: Vec<ControlEvent>,
}
