use std::collections::{HashMap, HashSet};

use crate::{LogicalSessionId, RunId, Session, SessionRole, Tree};

/// A projection borrowing the same sessions as the workspace tree.
#[derive(Debug, Default)]
pub struct Leaders<'a> {
    pub leaders: Vec<LeaderNode<'a>>,
    pub unassigned: Vec<UnassignedSession<'a>>,
}

#[derive(Debug)]
pub struct LeaderNode<'a> {
    pub session: &'a Session,
    pub workers: Vec<&'a Session>,
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
        let mut seen = HashSet::new();
        let mut runs = HashSet::new();
        let mut leaders = HashMap::new();
        for (_, session) in tree.sessions() {
            let Some(binding) = &session.binding else {
                continue;
            };
            if !seen.insert(binding.logical_id()) {
                return Err(ProjectionError::DuplicateLogicalId(binding.logical_id()));
            }
            if binding.role() == SessionRole::Leader {
                if let Some(run_id) = binding.run_id()
                    && !runs.insert(run_id)
                {
                    return Err(ProjectionError::DuplicateRunId(run_id));
                }
                leaders.insert(binding.logical_id(), out.leaders.len());
                out.leaders.push(LeaderNode {
                    session,
                    workers: Vec::new(),
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
        Ok(out)
    }
}
