//! The keys that answer what a session asks the user, read off the dialog its
//! screen shows by the rules in force for its agent, so that nothing is typed
//! unless the screen shows the very dialog that was answered.

use adroit_model::Agent;
use adroit_model::asking::{Asked, Reply};

use crate::rules::{Labels, Rules, Shown, in_force};

/// How much of what is asked has to show in the dialog: a long command may
/// be cut short where the dialog draws it.
const SHOWN: usize = 80;

/// What to type into a session of `agent` whose screen is `screen` to give
/// `reply` to `asked`, or why nothing may be typed.
pub fn keys<S: AsRef<str>>(
    agent: Agent,
    asked: &Asked,
    reply: &Reply,
    screen: &[S],
) -> Result<String, String> {
    keys_by(&in_force(agent).rules, asked, reply, screen)
}

fn keys_by<S: AsRef<str>>(
    rules: &Rules,
    asked: &Asked,
    reply: &Reply,
    screen: &[S],
) -> Result<String, String> {
    let answers = rules
        .answers()
        .ok_or("this agent's dialogs are answered only at its terminal")?;
    let shown = rules
        .shown(screen)
        .ok_or("the session's screen shows no dialog asking that")?;
    let label = match (asked, reply) {
        (Asked::Tool { about, .. }, Reply::Allow | Reply::Deny) => {
            let label = pick(&shown, reply, &answers.tool)?;
            if !shows(&shown, about) {
                return Err("the session's dialog asks about another call".into());
            }
            label
        }
        // The plan is not looked for on the screen: the dialog draws it as
        // markdown, scrolled away when it is long.
        (Asked::Plan { .. }, Reply::Allow | Reply::Deny) => pick(&shown, reply, &answers.plan)?,
        (Asked::Questions { questions }, Reply::Choose { label }) => {
            let [question] = questions.as_slice() else {
                return Err("several questions are answered only at the session's terminal".into());
            };
            if question.multi_select {
                return Err(
                    "a question taking several options is answered only at the session's terminal"
                        .into(),
                );
            }
            if !question.options.iter().any(|o| &o.label == label) {
                return Err(format!("the question offers no option {label:?}"));
            }
            let all_offered = question
                .options
                .iter()
                .all(|o| offered(&shown, &o.label).is_some());
            if !shows(&shown, &question.question) || !all_offered {
                return Err("the session's dialog asks another question".into());
            }
            label
        }
        _ => return Err("that answer does not fit what the session asks".into()),
    };
    offered(&shown, label)
        .map(str::to_owned)
        .ok_or_else(|| format!("the session's dialog offers no option {label:?}"))
}

/// The label `reply` chooses among `labels`, once the dialog offers every
/// one of them, as a dialog of that kind does.
fn pick<'a>(shown: &Shown, reply: &Reply, labels: &'a Labels) -> Result<&'a String, String> {
    let kind = std::iter::once(&labels.allow).chain(labels.deny.as_ref());
    if !kind
        .into_iter()
        .all(|label| offered(shown, label).is_some())
    {
        return Err("the session's dialog asks something else".into());
    }
    match reply {
        Reply::Allow => Ok(&labels.allow),
        _ => labels
            .deny
            .as_ref()
            .ok_or_else(|| String::from("that answer is given only at the session's terminal")),
    }
}

/// The key that chooses the option labelled exactly `label`, the lowest
/// drawn: the dialog asking now is drawn below anything left above it.
fn offered<'a>(shown: &'a Shown, label: &str) -> Option<&'a str> {
    shown
        .options
        .iter()
        .rfind(|o| o.label == label)
        .map(|o| o.key.as_str())
}

/// Whether the dialog's rows show `text`, however its lines are wrapped.
fn shows(shown: &Shown, text: &str) -> bool {
    let drawn: String = shown.rows.concat().split_whitespace().collect();
    let wanted: String = text
        .trim_end_matches('…')
        .split_whitespace()
        .collect::<String>()
        .chars()
        .take(SHOWN)
        .collect();
    !wanted.is_empty() && drawn.contains(&wanted)
}

#[cfg(test)]
mod tests {
    use adroit_model::asking::{Choice, Question};

    use super::*;
    use crate::testing::{recorded, screen};

    fn claude() -> Rules {
        Rules::bundled(Agent::Claude).unwrap()
    }

    fn tool(about: &str) -> Asked {
        Asked::Tool {
            name: "Bash".into(),
            about: about.into(),
        }
    }

    fn fruit(labels: &[&str]) -> Question {
        Question {
            header: "Fruit".into(),
            question: "Pick a fruit".into(),
            options: labels
                .iter()
                .map(|l| Choice {
                    label: (*l).into(),
                    description: String::new(),
                })
                .collect(),
            multi_select: false,
        }
    }

    fn questions(questions: Vec<Question>) -> Asked {
        Asked::Questions { questions }
    }

    fn choose(label: &str) -> Reply {
        Reply::Choose {
            label: label.into(),
        }
    }

    fn plan() -> Asked {
        Asked::Plan {
            plan: "Context: create a probe file hello.txt in the working directory.\n".into(),
        }
    }

    #[test]
    fn allowing_a_tool_types_the_number_of_its_dialog_s_yes() {
        let keys = keys_by(
            &claude(),
            &tool("date > stamp2.txt"),
            &Reply::Allow,
            &recorded("permission"),
        );
        assert_eq!(keys.as_deref(), Ok("1"));
    }

    #[test]
    fn denying_a_tool_types_the_number_its_dialog_draws_beside_no() {
        let keys = keys_by(
            &claude(),
            &tool("date > stamp2.txt"),
            &Reply::Deny,
            &recorded("permission"),
        );
        assert_eq!(keys.as_deref(), Ok("4"));
    }

    #[test]
    fn a_tool_s_dialog_asking_about_another_call_is_not_answered() {
        let keys = keys_by(
            &claude(),
            &tool("rm -rf build"),
            &Reply::Allow,
            &recorded("permission"),
        );
        assert!(keys.is_err(), "{keys:?}");
    }

    #[test]
    fn a_long_call_is_recognised_by_its_beginning_however_the_dialog_wraps_it() {
        let rule = "─".repeat(120);
        let wrapped = screen(&format!(
            "{rule}\n Bash command\n\n   cargo test --workspace --all-targets --features\n   one,two\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n\n Esc to cancel"
        ));
        let keys = keys_by(
            &claude(),
            &tool("cargo test --workspace --all-targets --features one,two"),
            &Reply::Deny,
            &wrapped,
        );
        assert_eq!(keys.as_deref(), Ok("2"));
    }

    #[test]
    fn a_question_s_dialog_is_not_taken_for_a_tool_s() {
        let keys = keys_by(
            &claude(),
            &tool("Pick a fruit"),
            &Reply::Allow,
            &recorded("question"),
        );
        assert!(keys.is_err(), "{keys:?}");
    }

    #[test]
    fn a_screen_with_no_dialog_is_not_answered() {
        for name in ["input", "after-a-reply", "working"] {
            let keys = keys_by(
                &claude(),
                &tool("date > stamp2.txt"),
                &Reply::Allow,
                &recorded(name),
            );
            assert!(keys.is_err(), "{name}: {keys:?}");
        }
    }

    #[test]
    fn a_dialog_of_another_kind_is_not_answered() {
        let keys = keys_by(&claude(), &plan(), &Reply::Allow, &recorded("model"));
        assert!(keys.is_err(), "{keys:?}");
    }

    #[test]
    fn choosing_an_option_types_the_number_drawn_beside_its_label() {
        let keys = keys_by(
            &claude(),
            &questions(vec![fruit(&["Apple", "Banana"])]),
            &choose("Banana"),
            &recorded("question"),
        );
        assert_eq!(keys.as_deref(), Ok("2"));
    }

    #[test]
    fn a_question_whose_options_the_dialog_does_not_draw_is_another_question() {
        let keys = keys_by(
            &claude(),
            &questions(vec![fruit(&["Apple", "Cherry"])]),
            &choose("Apple"),
            &recorded("question"),
        );
        assert!(keys.is_err(), "{keys:?}");
    }

    #[test]
    fn several_questions_or_several_options_at_once_are_left_to_the_terminal() {
        let several = keys_by(
            &claude(),
            &questions(vec![
                fruit(&["Apple", "Banana"]),
                fruit(&["Apple", "Banana"]),
            ]),
            &choose("Apple"),
            &recorded("question"),
        );
        assert!(several.is_err(), "{several:?}");
        let multi = keys_by(
            &claude(),
            &questions(vec![Question {
                multi_select: true,
                ..fruit(&["Apple", "Banana"])
            }]),
            &choose("Apple"),
            &recorded("question"),
        );
        assert!(multi.is_err(), "{multi:?}");
    }

    #[test]
    fn approving_a_plan_keeps_edits_approved_by_hand() {
        let approve = keys_by(&claude(), &plan(), &Reply::Allow, &recorded("plan"));
        assert_eq!(approve.as_deref(), Ok("2"));
    }

    #[test]
    fn a_plan_is_denied_only_at_the_terminal_as_its_dialog_offers_no_option_that_denies() {
        let deny = keys_by(&claude(), &plan(), &Reply::Deny, &recorded("plan"));
        assert!(deny.is_err(), "{deny:?}");
    }

    #[test]
    fn the_options_claude_code_adds_to_a_question_are_never_chosen() {
        for label in ["Type something.", "Chat about this"] {
            let keys = keys_by(
                &claude(),
                &questions(vec![fruit(&["Apple", "Banana"])]),
                &choose(label),
                &recorded("question"),
            );
            assert!(keys.is_err(), "{label}: {keys:?}");
        }
    }

    #[test]
    fn a_plan_s_dialog_is_not_taken_for_a_tool_s_nor_the_other_way() {
        let as_tool = keys_by(
            &claude(),
            &tool("Context: create a probe file hello.txt"),
            &Reply::Allow,
            &recorded("plan"),
        );
        assert!(as_tool.is_err(), "{as_tool:?}");
        let as_plan = keys_by(&claude(), &plan(), &Reply::Allow, &recorded("permission"));
        assert!(as_plan.is_err(), "{as_plan:?}");
    }

    #[test]
    fn an_answer_that_does_not_fit_what_is_asked_is_refused() {
        let keys = keys_by(
            &claude(),
            &tool("date > stamp2.txt"),
            &choose("Yes"),
            &recorded("permission"),
        );
        assert!(keys.is_err(), "{keys:?}");
    }

    #[test]
    fn an_agent_whose_rules_read_no_dialog_asking_is_answered_only_at_its_terminal() {
        let keys = keys_by(
            &Rules::bundled(Agent::Codex).unwrap(),
            &tool("date > stamp2.txt"),
            &Reply::Allow,
            &recorded("permission"),
        );
        assert!(keys.is_err(), "{keys:?}");
    }
}
