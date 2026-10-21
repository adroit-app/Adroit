//! Request progress supplements the existing session and workspace details.
use adroit_model::*;
use dioxus::prelude::*;

use crate::icons::{Icon, IconView};
use crate::leader_navigation::project;
use crate::nav::Item;

fn task_title(instructions: &str) -> String {
    let first = instructions
        .trim()
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    let end = first
        .char_indices()
        .find(|(i, c)| {
            *i > 0
                && matches!(c, '.' | '。' | '!' | '?')
                && (*c == '。'
                    || first[i + c.len_utf8()..]
                        .chars()
                        .next()
                        .is_none_or(char::is_whitespace))
        })
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(first.len());
    let title = first[..end].trim();
    if title.is_empty() {
        "Untitled task".into()
    } else {
        title.into()
    }
}

fn task_progress(states: &[TaskState]) -> String {
    let done = states
        .iter()
        .filter(|state| matches!(state, TaskState::Succeeded))
        .count();
    let active = states
        .iter()
        .filter(|state| matches!(state, TaskState::Running | TaskState::Verifying))
        .count();
    if active > 0 {
        format!("{done} of {} done · {active} active", states.len())
    } else {
        format!("{done} of {} done", states.len())
    }
}

fn state_icon(state: TaskState) -> Icon {
    match state {
        TaskState::Planned | TaskState::Queued => Icon::Circle,
        TaskState::Running | TaskState::Verifying => Icon::Loader,
        TaskState::Blocked | TaskState::Failed => Icon::AlertTriangle,
        TaskState::Succeeded => Icon::Check,
        TaskState::Cancelled => Icon::SquareX,
    }
}

fn task_state(state: TaskState) -> &'static str {
    match state {
        TaskState::Planned => "planned",
        TaskState::Queued => "queued",
        TaskState::Running => "running",
        TaskState::Verifying => "verifying",
        TaskState::Blocked => "blocked",
        TaskState::Succeeded => "succeeded",
        TaskState::Failed => "failed",
        TaskState::Cancelled => "cancelled",
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;

    #[test]
    fn task_title_uses_first_instruction_sentence_without_losing_utf8() {
        assert_eq!(
            task_title("  Investigate the issue. Then fix it.\nMore."),
            "Investigate the issue."
        );
        assert_eq!(
            task_title("\n\nInspect the UI\nThen repair it"),
            "Inspect the UI"
        );
        assert_eq!(task_title("日本語の指示。次の指示"), "日本語の指示。");
        assert_eq!(
            task_title("Read src/main.rs and then fix it. Verify behavior."),
            "Read src/main.rs and then fix it."
        );
    }

    #[test]
    fn progress_counts_completed_and_active_tasks() {
        assert_eq!(
            task_progress(&[TaskState::Succeeded, TaskState::Running, TaskState::Failed]),
            "1 of 3 done · 1 active"
        );
        assert_eq!(
            task_progress(&[TaskState::Succeeded, TaskState::Succeeded]),
            "2 of 2 done"
        );
    }
}

#[component]
pub fn RequestDetails(
    snapshot: ControlSnapshot,
    tree: Tree,
    item: Item,
    on_open: EventHandler<String>,
) -> Element {
    let run_id = project(&tree, &snapshot)
        .into_iter()
        .find(|row| row.item == item)
        .and_then(|row| row.run_id);
    let Some(run) = run_id.and_then(|id| snapshot.runs.iter().find(|run| run.id == id)) else {
        return rsx! {};
    };
    let tasks: Vec<_> = snapshot
        .tasks
        .iter()
        .filter(|task| task.run_id == run.id)
        .collect();
    let questions: Vec<_> = snapshot
        .messages
        .iter()
        .filter(|message| {
            message.run_id == run.id
                && message.kind == MessageKind::Question
                && !snapshot.messages.iter().any(|answer| {
                    answer.kind == MessageKind::Answer
                        && answer.reply_to == Some(message.id)
                        && answer.from == message.to
                        && answer.to == message.from
                })
        })
        .collect();
    let notices: Vec<_> = snapshot
        .coordination
        .iter()
        .filter(|notice| {
            notice
                .participants
                .iter()
                .any(|participant| participant.run_id == run.id)
        })
        .collect();
    rsx! {
        section { class: "details-block request-overview", "aria-label": "Request progress",
            div { class: "request-heading",
                span { class: "details-label", "{run.brief.title}" }
                span { class: "request-state", "{run.state.word()}" }
            }
            p { class: "request-prose", "{run.brief.goal}" }
            FullDetails { key: "run-{run.id}", table: crate::control::protocol::UiTable::Runs, id: run.id.to_string() }
            if !run.brief.constraints.is_empty() {
                details { class: "request-disclosure",
                    summary { IconView { icon: Icon::ListTree, size: 13 } span { "Constraints" } span { class: "request-count", "{run.brief.constraints.len()}" } IconView { icon: Icon::ChevronRight, size: 13 } }
                    for constraint in &run.brief.constraints { p { class: "request-prose", "{constraint}" } }
                }
            }
            if !run.brief.unresolved.is_empty() {
                div { class: "request-attention",
                    for question in &run.brief.unresolved { p { class: "request-prose", "{question}" } }
                }
            }
            if let Some(candidate)=run.candidate_id.and_then(|id|snapshot.candidates.iter().find(|candidate|candidate.plan.id==id)) {
                div { class: "request-evidence",
                    span { class: "details-label", "Completion candidate" }
                    span { "{candidate.plan.id}" }
                    span { "{candidate.state:?}" }
                    if let Some(proof)=&candidate.proof {
                        span { class: "details-path", "{proof.fingerprint}" }
                    }
                    if let Some(error)=&candidate.error { p { class: "request-attention request-prose", "{error}" } }
                }
            }
            if let Some(completion)=snapshot.completions.iter().find(|completion|completion.run_id==run.id) {
                p { class: "request-prose", "{completion.summary}" }
            }
        }
        section { class: "details-block request-tasks", "aria-label": "Request tasks",
            div { class: "request-section-heading",
                span { class: "details-label", "Tasks" }
                span { class: "request-muted", "{task_progress(&tasks.iter().map(|task| task.state).collect::<Vec<_>>())}" }
            }
            if tasks.is_empty() { p { class: "request-muted", "No tasks dispatched yet" } }
            for task in tasks {
                {
                    let attempt=task.current_attempt.and_then(|id|snapshot.attempts.iter().find(|attempt|attempt.id==id));
                    let native=attempt.filter(|attempt|attempt.task_revision==task.revision).and_then(|attempt|snapshot.sessions.iter().find(|session|session.binding.logical_id()==attempt.session_id && session.binding.work()==Some((task.id,attempt.id)))).and_then(|session|session.terminal_id.clone()).filter(|id|tree.find(id).is_some());
                    let state=task_state(task.state);
                    let icon=state_icon(task.state);
                    let tone=match task.state { TaskState::Running | TaskState::Verifying => "run", TaskState::Succeeded => "done", TaskState::Blocked | TaskState::Failed => "fail", _ => "quiet" };
                    let title=task_title(&task.brief.instructions);
                    let report=attempt.and_then(|attempt|snapshot.reports.iter().find(|record|record.report.attempt_id==attempt.id));
                    let requested=attempt.and_then(|attempt|attempt.target.requested_model.as_deref()).or(task.brief.target.model.as_deref()).unwrap_or("Unspecified");
                    let selected=attempt.and_then(|attempt|attempt.target.model.as_deref()).unwrap_or("Provider default");
                    let actual=attempt.and_then(|attempt|attempt.actual_model.as_deref()).unwrap_or("Not reported");
                    let selection=attempt.map(|attempt| match attempt.target.model_source {
                        ModelSource::Explicit => "Explicit",
                        ModelSource::Settings => "Settings",
                        ModelSource::AgentDefault => "Provider default",
                    }).unwrap_or("Not selected");
                    let agent=attempt.map(|attempt|attempt.target.agent).or(task.brief.target.agent).map(Agent::key).unwrap_or("Not selected");
                    let readiness=attempt.filter(|attempt|attempt.state == AttemptState::Running).map(|attempt| match attempt.readiness {
                        WorkerReadiness::Spawned => "Starting; terminal opened, waiting for agent readiness",
                        WorkerReadiness::Ready => "Ready",
                        WorkerReadiness::AwaitingTrust => "Trust decision needed; open Worker terminal",
                        WorkerReadiness::AwaitingPermission => "Permission decision needed; open Worker terminal",
                        WorkerReadiness::Stalled => "Startup stalled; open Worker terminal",
                    });
                    rsx! {
                        details { key: "{task.id}", class: "request-task",
                            summary {
                                span { class: "request-task-icon request-task-icon-{tone}", IconView { icon, size: 14 } }
                                span { class: "request-task-main",
                                    span { class: "request-task-title", "{title}" }
                                    span { class: "request-state", "{state}" }
                                }
                                IconView { icon: Icon::ChevronRight, size: 13 }
                            }
                            p { class: "request-prose request-instructions", "{task.brief.instructions}" }
                            if let Some(id)=native.clone() {
                                button { class: "request-action", onclick: move |_|on_open.call(id.clone()), IconView { icon: Icon::Terminal, size: 13 } "Open Worker" IconView { icon: Icon::ChevronRight, size: 13 } }
                            }
                            div { class: "details-facts",
                                span { class: "details-facts-key", "Agent" } span { class: "details-facts-value", "{agent}" }
                                span { class: "details-facts-key", "Requested" } span { class: "details-facts-value", "{requested}" }
                                span { class: "details-facts-key", "Selected" } span { class: "details-facts-value", "{selected}" }
                                span { class: "details-facts-key", "Actual" } span { class: "details-facts-value", "{actual}" }
                                span { class: "details-facts-key", "Selection" } span { class: "details-facts-value", "{selection}" }
                                if let Some(readiness)=readiness { span { class: "details-facts-key", "Readiness" } span { class: "details-facts-value", "{readiness}" } }
                                span { class: "details-facts-key", "Revision" } span { class: "details-facts-value", "{task.revision}" }
                            }
                            if !task.brief.scope.is_empty() { p { class: "request-prose request-muted", {task.brief.scope.join(", ")} } }
                            FullDetails { key: "task-{task.id}", table: crate::control::protocol::UiTable::Tasks, id: task.id.to_string() }
                            TaskHistory { key: "history-{task.id}", task: task.clone(), epoch: snapshot.event_cursor }
                            if let Some(report)=report {
                                FullDetails { key: "report-{report.report.message_id}", table: crate::control::protocol::UiTable::Reports, id: report.report.message_id.to_string() }
                                p { class: "request-prose", "{report.report.result.summary}" }
                                if let Some(TaskArtifact::Review {decision,data,..})=&report.report.result.artifact {
                                    p { class: "request-prose", "Review decision: {decision:?}" }
                                    pre { class: "request-prose request-check", {serde_json::to_string_pretty(data).unwrap_or_default()} }
                                }
                                if !report.applied || report.report.task.revision != task.revision { p { class: "request-attention", "Historical result; not applied to the current task" } }
                                for check in &report.report.result.checks {
                                    p { class: "request-prose request-check", {check.command.join(" ")} " (exit {check.exit_code})" }
                                }
                                for issue in &report.report.result.unresolved { p { class: "request-prose request-attention", "{issue}" } }
                            }
                        }
                    }
                }
            }
        }
        if !questions.is_empty() {
            section { class: "details-block request-questions", "aria-label": "Unanswered questions",
                span { class: "details-label", "Unanswered questions" }
                for question in questions {
                    div { key: "{question.id}", class: "request-question",
                        if question.blocks_work { span { class: "request-attention", "Blocks work" } }
                        p { class: "request-prose", "{question.body}" }
                        FullDetails { key: "message-{question.id}", table: crate::control::protocol::UiTable::Messages, id: question.id.to_string() }
                    }
                }
            }
        }
        if !notices.is_empty() {
            section { class: "details-block request-coordination", "aria-label": "Shared workspace changes",
                span { class: "details-label", "Shared workspace changes" }
                for notice in notices {
                    {
                        let source=snapshot.sessions.iter().find(|session|session.binding.logical_id()==notice.leader.id).and_then(|session|session.terminal_id.clone()).filter(|id|tree.find(id).is_some());
                        rsx! {
                            details { key: "{notice.intent.id}", class: "request-notice request-disclosure",
                                summary { IconView { icon: Icon::ArrowsExchange, size: 13 } span { "{notice.intent.purpose}" } IconView { icon: Icon::ChevronRight, size: 13 } }
                                p { class: "request-muted", if notice.outcome.is_some() { "Finished" } else { "Announced" } }
                                p { class: "request-prose", {notice.intent.scope.join(", ")} }
                                if let Some(id)=source { button { class: "request-action", onclick: move |_|on_open.call(id.clone()), IconView { icon: Icon::Terminal, size: 13 } "Open Leader" IconView { icon: Icon::ChevronRight, size: 13 } } }
                                for reply in snapshot.coordination_replies.iter().filter(|reply|reply.notice_id==notice.intent.id) {
                                    p { class: "request-prose", "{reply.body}" }
                                }
                                if let Some(outcome)=&notice.outcome {
                                    p { class: "request-prose", "{outcome.summary}" }
                                    if let Some(revision)=&outcome.revision { span { class: "details-path", "{revision}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Fetch evidence only on an explicit user action, keyed by its durable identity.
#[component]
fn FullDetails(table: crate::control::protocol::UiTable, id: String) -> Element {
    let mut text = use_signal(|| None::<String>);
    let mut pending = use_signal(|| false);
    rsx! {
        button { class: "request-action", disabled: pending(), "aria-label": if pending() { "Loading full details" } else { "Load full details" },
            onclick: move |_| {
                let id = id.clone();
                pending.set(true);
                spawn(async move {
                    let result = match crate::control::client::Client::human(&crate::control::root()) {
                        Ok(client) => client.ui_detail(table, id).await,
                        Err(error) => Err(error),
                    };
                    text.set(Some(match result {
                        Ok(raw) => serde_json::from_str::<serde_json::Value>(&raw)
                            .and_then(|value| serde_json::to_string_pretty(&value)).unwrap_or(raw),
                        Err(error) => format!("Cannot load details: {error}"),
                    }));
                    pending.set(false);
                });
            },
            IconView { icon: Icon::File, size: 13 }
            if pending() { "Loading details…" } else { "Full details" }
            IconView { icon: Icon::ChevronRight, size: 13 }
        }
        if let Some(text) = text() { pre { class: "request-prose request-check", "{text}" } }
    }
}

/// History discovery stays within this task; evidence is fetched on demand.
#[component]
fn TaskHistory(task: TaskRecord, epoch: u64) -> Element {
    let mut history = use_signal(|| None::<Vec<crate::control::protocol::UiAttemptHistory>>);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let task_id = task.id;
    rsx! {
        details { class: "request-disclosure request-history",
            summary { IconView { icon: Icon::ListTree, size: 13 } span { "Attempt and report history" } IconView { icon: Icon::ChevronRight, size: 13 } }
            button { class: "request-action", disabled: pending(),
                onclick: move |_| {
                    pending.set(true);
                    error.set(None);
                    spawn(async move {
                        let result = match crate::control::client::Client::human(&crate::control::root()) {
                            Ok(client) => client.ui_task_history(task_id, Some(epoch)).await,
                            Err(error) => Err(error),
                        };
                        match result {
                            Ok(entries) => history.set(Some(entries)),
                            Err(failure) => error.set(Some(format!("Cannot load history: {failure}"))),
                        }
                        pending.set(false);
                    });
                },
                IconView { icon: Icon::File, size: 13 }
                if pending() { "Loading history…" } else { "Load attempt history" }
                IconView { icon: Icon::ChevronRight, size: 13 }
            }
            if let Some(failure) = error() { p { class: "request-attention", "{failure}" } }
            if let Some(entries) = history() {
                if entries.is_empty() { p { class: "request-muted", "No attempts recorded" } }
                for entry in entries {
                    div { key: "{entry.attempt_id}",
                        p { class: "request-prose",
                            if entry.is_historical(&task) { "Historical attempt" } else { "Current attempt" }
                            " · revision {entry.task_revision} · {entry.state:?}"
                        }
                        p { class: "details-path", "Attempt {entry.attempt_id} · Session {entry.session_id}" }
                        if let Some(report) = entry.report {
                            p { class: "request-muted", "Report revision {report.task_revision} · {report.status:?}" }
                            FullDetails { table: crate::control::protocol::UiTable::Reports, id: report.id.to_string() }
                        }
                    }
                }
            }
        }
    }
}
