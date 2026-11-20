//! What a window says a session waiting on the user asks of them.

use adroit_model::asking::{Asked, Asking};
use adroit_model::{InboxItem, Session, State};

/// What the session `s` asks of the user in one short line, while it waits
/// on them and its agent's hooks told what for; none otherwise, as what the
/// hooks last told is not what a session not waiting asks.
pub fn waiting_on(s: &Session) -> Option<String> {
    match (&s.state, &s.asking) {
        (State::Waiting(_), Some(asking)) => Some(line(asking)),
        _ => None,
    }
}

/// What the row of `waiting`, listed again below the tree, says under its
/// name: its workspace, and what it asks, or else what it waits for.
pub fn waiting_sub(waiting: &InboxItem) -> String {
    let what = waiting_on(&waiting.session).unwrap_or_else(|| waiting.wait.to_string());
    format!("{} · {what}", waiting.workspace)
}

/// `asking` in one short line: the tool and what it runs, the first
/// question, or the plan's title.
pub fn line(asking: &Asking) -> String {
    let said = |kind: &str, text: &str| match first_line(text) {
        Some(text) => format!("{kind}: {text}"),
        None => kind.to_owned(),
    };
    match &asking.asked {
        Asked::Tool { name, about } => said(name, about),
        Asked::Questions { questions } => {
            let first = questions.first().map_or("", |q| q.question.as_str());
            match questions.len() {
                0 | 1 => said("Question", first),
                more => format!("{} (+{})", said("Question", first), more - 1),
            }
        }
        Asked::Plan { plan } => said("Plan", plan.trim_start_matches(['#', ' ', '\n'])),
    }
}

fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|l| !l.is_empty())
}

#[cfg(test)]
mod tests {
    use adroit_model::Wait;
    use adroit_model::asking::{Choice, Question};
    use adroit_model::testing::session;

    use super::*;

    fn question(text: &str) -> Question {
        Question {
            header: "Colour".into(),
            question: text.into(),
            options: vec![Choice {
                label: "Red".into(),
                description: String::new(),
            }],
            multi_select: false,
        }
    }

    fn tool(name: &str, about: &str) -> Asked {
        Asked::Tool {
            name: name.into(),
            about: about.into(),
        }
    }

    fn line_of(asked: Asked) -> Option<String> {
        waiting_on(&session("a").waiting(Wait::Permission).asking(asked).build())
    }

    #[test]
    fn a_tool_waiting_for_approval_reads_as_the_tool_and_what_it_runs() {
        assert_eq!(
            line_of(tool("Bash", "date > stamp.txt")).as_deref(),
            Some("Bash: date > stamp.txt")
        );
    }

    #[test]
    fn a_tool_whose_call_says_nothing_of_itself_reads_as_the_tool_alone() {
        assert_eq!(line_of(tool("Bash", "")).as_deref(), Some("Bash"));
    }

    #[test]
    fn a_question_reads_as_the_first_question_with_how_many_more_follow() {
        assert_eq!(
            line_of(Asked::Questions {
                questions: vec![question("Pick a colour")]
            })
            .as_deref(),
            Some("Question: Pick a colour")
        );
        assert_eq!(
            line_of(Asked::Questions {
                questions: vec![question("Pick a colour"), question("Pick a size")]
            })
            .as_deref(),
            Some("Question: Pick a colour (+1)")
        );
    }

    #[test]
    fn a_plan_reads_as_its_title() {
        assert_eq!(
            line_of(Asked::Plan {
                plan: "# Stamp the date\n\nWrite it.".into()
            })
            .as_deref(),
            Some("Plan: Stamp the date")
        );
        assert_eq!(
            line_of(Asked::Plan {
                plan: String::new()
            })
            .as_deref(),
            Some("Plan")
        );
    }

    #[test]
    fn a_session_not_waiting_says_nothing_of_what_its_hooks_last_told() {
        let s = session("a").working().asking(tool("Bash", "ls")).build();
        assert_eq!(waiting_on(&s), None);
    }

    #[test]
    fn a_waiting_row_says_what_its_session_asks_or_else_what_it_waits_for() {
        let item = |s: Session| InboxItem {
            workspace: "api".into(),
            session: s,
            wait: Wait::Permission,
        };
        let asks = session("a")
            .waiting(Wait::Permission)
            .asking(tool("Bash", "date > stamp.txt"))
            .build();
        assert_eq!(waiting_sub(&item(asks)), "api · Bash: date > stamp.txt");
        let told_nothing = session("a").waiting(Wait::Permission).build();
        assert_eq!(waiting_sub(&item(told_nothing)), "api · permission prompt");
    }

    #[test]
    fn a_waiting_session_whose_hooks_told_nothing_says_nothing_more() {
        assert_eq!(waiting_on(&session("a").waiting(Wait::Input).build()), None);
    }
}
