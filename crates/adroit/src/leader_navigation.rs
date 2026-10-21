//! Alternate navigation over the same native sessions, joined to current ownership.
use adroit_model::*;
use std::collections::HashMap;

use crate::nav::Item;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowRole {
    Leader,
    Worker,
    Terminal,
    Unassigned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub item: Item,
    pub role: RowRole,
    pub run_id: Option<RunId>,
    pub reason: Option<String>,
}

pub fn project(tree: &Tree, snapshot: &ControlSnapshot) -> Vec<Row> {
    let authority: HashMap<_, _> = snapshot
        .sessions
        .iter()
        .map(|session| (session.binding.logical_id(), session))
        .collect();
    let runs: HashMap<_, _> = snapshot.runs.iter().map(|run| (run.id, run)).collect();
    let mut checked = tree.clone();
    let mut reasons = HashMap::new();
    for session in checked
        .workspaces
        .iter_mut()
        .flat_map(|workspace| &mut workspace.worktrees)
        .flat_map(|worktree| &mut worktree.sessions)
    {
        let Some(binding) = &session.binding else {
            continue;
        };
        let current = authority.get(&binding.logical_id()).is_some_and(|record| {
            record.binding == *binding
                && record.terminal_id.as_deref() == Some(session.id.as_str())
                && record.agent == session.agent
                && record.state == ManagedSessionState::Running
                && binding
                    .run_id()
                    .and_then(|id| runs.get(&id))
                    .is_some_and(|run| run.leader_id == binding.leader_id())
        });
        if !current {
            reasons.insert(
                session.id.clone(),
                "The terminal has no current matching controller assignment".to_owned(),
            );
            session.binding = None;
        }
    }
    let Ok(groups) = Leaders::from_tree(&checked) else {
        return checked
            .sessions()
            .map(|(_, session)| Row {
                item: Item::Session(session.id.clone()),
                role: RowRole::Unassigned,
                run_id: None,
                reason: Some("Conflicting session identities need reconciliation".into()),
            })
            .chain(
                checked
                    .worktrees()
                    .flat_map(|worktree| &worktree.terminals)
                    .map(|terminal| Row {
                        item: Item::Terminal(terminal.id),
                        role: RowRole::Terminal,
                        run_id: None,
                        reason: Some("Unassigned terminal".into()),
                    }),
            )
            .collect();
    };
    let mut rows = Vec::new();
    for group in groups.leaders {
        let run_id = group
            .session
            .binding
            .as_ref()
            .and_then(SessionBinding::run_id);
        rows.push(Row {
            item: Item::Session(group.session.id.clone()),
            role: RowRole::Leader,
            run_id,
            reason: None,
        });
        for worker in group.workers {
            rows.push(Row {
                item: Item::Session(worker.id.clone()),
                role: RowRole::Worker,
                run_id,
                reason: None,
            });
        }
        for terminal in group.tools {
            rows.push(Row {
                item: Item::Terminal(terminal.id),
                role: RowRole::Terminal,
                run_id,
                reason: None,
            });
        }
    }
    for unassigned in groups.unassigned {
        let reason =
            reasons
                .remove(&unassigned.session.id)
                .unwrap_or_else(|| match unassigned.reason {
                    UnassignedReason::Unbound => "No assigned request".into(),
                    UnassignedReason::MissingLeader => "The assigned Leader is not visible".into(),
                    UnassignedReason::RunMismatch => "The Leader belongs to another request".into(),
                });
        rows.push(Row {
            item: Item::Session(unassigned.session.id.clone()),
            role: RowRole::Unassigned,
            run_id: unassigned
                .session
                .binding
                .as_ref()
                .and_then(SessionBinding::run_id),
            reason: Some(reason),
        });
    }
    for unassigned in groups.unassigned_terminals {
        rows.push(Row {
            item: Item::Terminal(unassigned.terminal.id),
            role: RowRole::Terminal,
            run_id: None,
            reason: Some(
                match unassigned.reason {
                    UnassignedTerminalReason::Unbound => "Unassigned terminal",
                    UnassignedTerminalReason::MissingRun => "The assigned Leader is not visible",
                    UnassignedTerminalReason::WorkerMismatch => {
                        "The terminal's Worker belongs to another request"
                    }
                }
                .into(),
            ),
        });
    }
    rows
}

pub fn terminal_binding(
    tree: &Tree,
    snapshot: &ControlSnapshot,
    item: &Item,
) -> Option<TerminalBinding> {
    let run_id = project(tree, snapshot)
        .into_iter()
        .find(|row| &row.item == item)?
        .run_id?;
    let worker_id = match item {
        Item::Session(id) => tree
            .find(id)?
            .2
            .binding
            .as_ref()
            .filter(|binding| binding.role() == SessionRole::Worker)
            .map(SessionBinding::logical_id),
        Item::Terminal(id) => tree
            .terminal(*id)?
            .2
            .binding
            .as_ref()
            .and_then(|binding| binding.worker_id),
        _ => return None,
    };
    Some(TerminalBinding { run_id, worker_id })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerPresentation {
    pub label: String,
    pub metadata: String,
    pub instructions: Option<String>,
}

/// The shared sidebar filter for both navigation modes.
pub fn narrowed(tree: &Tree, snapshot: &ControlSnapshot, text: &str) -> Tree {
    crate::nav::narrowed_with_session_label(tree, text, |session| {
        worker_presentation(session, snapshot).map(|display| display.label)
    })
}

/// Presentation only: native identity and persisted names remain authoritative.
pub fn worker_presentation(
    session: &Session,
    snapshot: &ControlSnapshot,
) -> Option<WorkerPresentation> {
    let binding = session.binding.as_ref()?;
    let (task_id, attempt_id) = binding.work()?;
    let run_id = binding.run_id()?;
    let owned = snapshot
        .runs
        .iter()
        .any(|run| run.id == run_id && run.leader_id == binding.leader_id())
        && snapshot.sessions.iter().any(|record| {
            record.binding == *binding
                && record.terminal_id.as_deref() == Some(session.id.as_str())
                && record.agent == session.agent
        });
    let task = owned
        .then(|| {
            let task = snapshot
                .tasks
                .iter()
                .find(|task| task.id == task_id && task.run_id == run_id)?;
            snapshot.attempts.iter().find(|attempt| {
                attempt.id == attempt_id
                    && attempt.task_id == task.id
                    && attempt.session_id == binding.logical_id()
                    && attempt.task_revision == task.revision
            })?;
            Some(task)
        })
        .flatten();
    let instructions = task
        .map(|task| &task.brief.instructions)
        .filter(|text| !text.trim().is_empty())
        .cloned();
    let label = instructions
        .as_deref()
        .and_then(|text| text.lines().map(normalize).find(|line| !line.is_empty()))
        .or_else(|| {
            task.and_then(|task| {
                Some(
                    match task.brief.purpose {
                        TaskPurpose::Investigate => "Investigation",
                        TaskPurpose::Implement => "Implementation",
                        TaskPurpose::Review => "Review",
                        TaskPurpose::Classify => "Classification",
                        TaskPurpose::Integrate => "Integration",
                        TaskPurpose::Verify => "Verification",
                        TaskPurpose::Other => return None,
                    }
                    .to_owned(),
                )
            })
        })
        .or_else(|| {
            let name = normalize(&session.name);
            let identity = name.strip_prefix("worker-").unwrap_or(&name);
            (!name.is_empty() && identity.parse::<LogicalSessionId>().is_err()).then_some(name)
        })
        .unwrap_or_else(|| "Worker".into());
    // Snapshot records are in store insertion order. Keeping completed and hidden
    // sessions in this sequence avoids renumbering when the visible tree changes.
    let number = owned
        .then(|| {
            snapshot
                .sessions
                .iter()
                .filter(|record| {
                    record.binding.role() == SessionRole::Worker
                        && record.binding.run_id() == Some(run_id)
                        && record.binding.leader_id() == binding.leader_id()
                })
                .position(|record| record.binding.logical_id() == binding.logical_id())
        })
        .flatten();
    let mut metadata = number
        .map(|index| format!("Worker {}", index + 1))
        .unwrap_or_else(|| "Worker".into());
    if owned {
        let attempts: Vec<_> = snapshot
            .attempts
            .iter()
            .filter(|attempt| attempt.task_id == task_id)
            .collect();
        if attempts.len() > 1
            && let Some(index) = attempts.iter().position(|attempt| {
                attempt.id == attempt_id && attempt.session_id == binding.logical_id()
            })
        {
            metadata.push_str(&format!(" · Attempt {}", index + 1));
        }
    }
    Some(WorkerPresentation {
        label,
        metadata,
        instructions,
    })
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
