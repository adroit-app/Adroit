use std::collections::HashMap;

use crate::{LogicalSessionId, RunId, Session, SessionRole, TerminalSession, Tree};

/// A projection borrowing the same sessions as the workspace tree.
#[derive(Debug, Default)]
pub struct Leaders<'a> {
    pub leaders: Vec<LeaderNode<'a>>,
    pub unassigned: Vec<UnassignedSession<'a>>,
    pub unassigned_terminals: Vec<UnassignedTerminal<'a>>,
}

#[derive(Debug)]
pub struct LeaderNode<'a> {
    pub session: &'a Session,
    pub workers: Vec<&'a Session>,
    pub tools: Vec<&'a TerminalSession>,
}

#[derive(Debug)]
pub struct UnassignedTerminal<'a> {
    pub terminal: &'a TerminalSession,
    pub reason: UnassignedTerminalReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnassignedTerminalReason {
    Unbound,
    MissingRun,
    WorkerMismatch,
}

#[derive(Debug)]
pub struct UnassignedSession<'a> {
    pub session: &'a Session,
    pub reason: UnassignedReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnassignedReason {
    Unbound,
    MissingLeader,
    RunMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProjectionError {
    #[error("logical session {0} occurs more than once")]
    DuplicateLogicalId(LogicalSessionId),
    #[error("run {0} has more than one leader")]
    DuplicateRunId(RunId),
}

impl<'a> Leaders<'a> {
    pub fn from_tree(tree: &'a Tree) -> Result<Self, ProjectionError> {
        let mut out = Self::default();
        let mut sessions = HashMap::new();
        let mut runs = HashMap::new();
        let mut leaders = HashMap::new();
        for (_, session) in tree.sessions() {
            let Some(binding) = &session.binding else {
                continue;
            };
            if sessions.insert(binding.logical_id(), session).is_some() {
                return Err(ProjectionError::DuplicateLogicalId(binding.logical_id()));
            }
            if binding.role() == SessionRole::Leader {
                if let Some(run_id) = binding.run_id()
                    && runs.insert(run_id, out.leaders.len()).is_some()
                {
                    return Err(ProjectionError::DuplicateRunId(run_id));
                }
                leaders.insert(binding.logical_id(), out.leaders.len());
                out.leaders.push(LeaderNode {
                    session,
                    workers: Vec::new(),
                    tools: Vec::new(),
                });
            }
        }
        for (_, session) in tree.sessions() {
            let reason = match &session.binding {
                None => UnassignedReason::Unbound,
                Some(binding) if binding.role() == SessionRole::Leader => continue,
                Some(binding) => match leaders.get(&binding.leader_id()) {
                    None => UnassignedReason::MissingLeader,
                    Some(&index) => {
                        let group = &mut out.leaders[index];
                        let owner = group.session.binding.as_ref().expect("a listed leader");
                        if owner.run_id() == binding.run_id() {
                            group.workers.push(session);
                            continue;
                        }
                        UnassignedReason::RunMismatch
                    }
                },
            };
            out.unassigned.push(UnassignedSession { session, reason });
        }
        for terminal in tree.worktrees().flat_map(|worktree| &worktree.terminals) {
            let reason = match &terminal.binding {
                None => UnassignedTerminalReason::Unbound,
                Some(binding) => match runs.get(&binding.run_id) {
                    None => UnassignedTerminalReason::MissingRun,
                    Some(&index) => {
                        let conflicting_worker = binding
                            .worker_id
                            .and_then(|id| sessions.get(&id))
                            .and_then(|session| session.binding.as_ref())
                            .is_some_and(|worker| {
                                worker.role() != SessionRole::Worker
                                    || worker.run_id() != Some(binding.run_id)
                            });
                        if !conflicting_worker {
                            out.leaders[index].tools.push(terminal);
                            continue;
                        }
                        UnassignedTerminalReason::WorkerMismatch
                    }
                },
            };
            out.unassigned_terminals
                .push(UnassignedTerminal { terminal, reason });
        }
        Ok(out)
    }
}
