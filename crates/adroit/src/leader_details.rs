//! Request progress supplements the existing session and workspace details.
use adroit_model::*;
use dioxus::prelude::*;

use crate::leader_navigation::project;
use crate::nav::Item;

#[component]
pub fn LeaderRecovery(snapshot: ControlSnapshot) -> Element {
    let mut pending = use_signal(|| None::<SessionRef>);
    let mut error = use_signal(|| None::<String>);
    rsx! {
        for session in snapshot.sessions.iter().filter(|session| {
            session.binding.role() == SessionRole::Leader
                && matches!(session.state, ManagedSessionState::Exited | ManagedSessionState::Failed)
                && session.binding.run_id().is_some()
        }) {
            {
                let identity = session.reference();
                let title = session.binding.run_id().and_then(|id| snapshot.runs.iter().find(|run| run.id == id)).map(|run| run.brief.title.as_str()).unwrap_or("Saved request");
                rsx! {
                    div { class: "details-block request-recovery", key: "{identity.id}-{identity.generation}",
                        span { class: "details-label", "{title}" }
                        button {
                            class: "request-open",
                            disabled: pending() == Some(identity),
                            title: "Open a fresh Leader conversation with this saved request and its existing Workers",
                            onclick: move |_| {
                                pending.set(Some(identity));
                                error.set(None);
                                spawn(async move {
                                    let result = async {
                                        crate::control::client::Client::human(&crate::control::root())?
                                            .request(RequestId::new(), crate::control::protocol::Command::RestartLeader { identity }).await
                                    }.await;
                                    if let Err(failure) = result {
                                        error.set(Some(failure.to_string()));
                                        pending.set(None);
                                    }
                                });
                            },
                            if pending() == Some(identity) { "Restart queued" } else { "Restart Leader" }
                        }
                    }
                }
            }
        }
        if let Some(failure) = error() { p { class: "request-attention", "{failure}" } }
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
            if !run.brief.constraints.is_empty() {
                details { summary { "Constraints" }
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
            span { class: "details-label", "Tasks" }
            if tasks.is_empty() { p { class: "request-muted", "No tasks dispatched yet" } }
            for task in tasks {
                {
                    let attempt=task.current_attempt.and_then(|id|snapshot.attempts.iter().find(|attempt|attempt.id==id));
                    let native=attempt.and_then(|attempt|snapshot.sessions.iter().find(|session|session.binding.logical_id()==attempt.session_id)).and_then(|session|session.terminal_id.clone()).filter(|id|tree.find(id).is_some());
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
                    rsx! {
                        details { key: "{task.id}", class: "request-task",
                            summary {
                                span { class: "request-task-title", "{task.brief.instructions}" }
                                span { class: "request-state", "{task_state(task.state)}" }
                            }
                            if let Some(id)=native {
                                button { class: "request-open", onclick: move |_|on_open.call(id.clone()), "Open Worker" }
                            }
                            div { class: "details-facts",
                                span { class: "details-facts-key", "Agent" } span { class: "details-facts-value", "{agent}" }
                                span { class: "details-facts-key", "Requested" } span { class: "details-facts-value", "{requested}" }
                                span { class: "details-facts-key", "Selected" } span { class: "details-facts-value", "{selected}" }
                                span { class: "details-facts-key", "Actual" } span { class: "details-facts-value", "{actual}" }
                                span { class: "details-facts-key", "Selection" } span { class: "details-facts-value", "{selection}" }
                                span { class: "details-facts-key", "Revision" } span { class: "details-facts-value", "{task.revision}" }
                            }
                            if !task.brief.scope.is_empty() { p { class: "request-prose request-muted", {task.brief.scope.join(", ")} } }
                            if let Some(report)=report {
                                p { class: "request-prose", "{report.report.result.summary}" }
                                if let Some(TaskArtifact::Review {decision,data,..})=&report.report.result.artifact {
                                    p { class: "request-prose", "Review decision: {decision:?}" }
                                    pre { class: "request-prose request-check", {serde_json::to_string_pretty(data).unwrap_or_default()} }
                                }
                                if !report.applied { p { class: "request-attention", "Historical result; not applied to the current task" } }
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
                            details { key: "{notice.intent.id}", class: "request-notice",
                                summary { "{notice.intent.purpose}" }
                                p { class: "request-muted", if notice.outcome.is_some() { "Finished" } else { "Announced" } }
                                p { class: "request-prose", {notice.intent.scope.join(", ")} }
                                if let Some(id)=source { button { class: "request-open", onclick: move |_|on_open.call(id.clone()), "Open Leader" } }
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
