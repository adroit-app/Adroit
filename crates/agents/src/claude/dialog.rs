//! Telling from Claude Code's screen whether one of its own dialogs, such as
//! the picker `/model` opens, has the keys in place of its input box, as the
//! rules in force for Claude Code read it, and what the dialog asks.

use adroit_model::{Agent, State};

use crate::Judgement;

/// Whether the screen whose rows are `lines` shows a dialog of Claude Code's
/// that takes keys: one drawn below a rule across the screen, with a hint of
/// its keys such as "Esc to cancel". It takes the place of the input box, a
/// prompt between two such rules, or sits right above it while Claude works.
pub fn dialog_open<S: AsRef<str>>(lines: &[S]) -> bool {
    crate::rules::dialog_open(Agent::Claude, lines)
}

/// How a session stands by its screen, for one whose status its program has
/// not reported: working while its spinner sits right above the input box,
/// idle with the input box alone. Nothing is told before the input box is
/// drawn, as while Claude Code asks whether to trust the folder, nor while a
/// dialog of its own has the keys.
pub fn judge_state<S: AsRef<str>>(lines: &[S]) -> Judgement<State> {
    crate::rules::judge_state(Agent::Claude, lines)
}

/// The channels named by Claude Code's warning before it loads development
/// channels, while that warning is the dialog open on the screen with its
/// option to go on selected, so that Enter would confirm it; `None` for any
/// other screen. The warning is recognised by its exact wording, so a warning
/// worded otherwise is not taken for it.
pub fn development_channels<S: AsRef<str>>(lines: &[S]) -> Option<Vec<String>> {
    if !dialog_open(lines) {
        return None;
    }
    let from = (0..lines.len()).rfind(|&i| rule(lines[i].as_ref()))?;
    let shown: Vec<&str> = lines[from + 1..]
        .iter()
        .map(|line| line.as_ref().trim())
        .filter(|line| !line.is_empty())
        .collect();
    let [title, .., go_on, exit, keys] = shown.as_slice() else {
        return None;
    };
    if *title != "WARNING: Loading development channels"
        || *go_on != "❯ 1. I am using this for local development"
        || *exit != "2. Exit"
        || *keys != "Enter to confirm · Esc to cancel"
    {
        return None;
    }
    let named = shown
        .iter()
        .find_map(|line| line.strip_prefix("Channels:"))?;
    let channels: Vec<String> = named
        .split([',', ' '])
        .filter(|channel| !channel.is_empty())
        .map(String::from)
        .collect();
    (!channels.is_empty()).then_some(channels)
}

/// The shortest rule taken for one across the screen; a dialog's own boxes
/// are indented and cornered, so they never pass.
const RULE_MIN: usize = 20;

/// Whether `line` is a rule across the screen, as drawn above and below the
/// input box. A session started with a name carries it near the right end of
/// the input box's top rule, as in `──── fix ─`.
pub fn rule(line: &str) -> bool {
    let line = line.trim_end();
    let label = line.trim_matches('─');
    let drawn = line.chars().filter(|&c| c == '─').count();
    line.starts_with('─')
        && drawn >= RULE_MIN
        && (label.is_empty()
            || (line.ends_with('─')
                && label.starts_with(' ')
                && label.ends_with(' ')
                && !label.contains('─')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::recorded;

    // The screens kept under `tests/screens`. Those named after a slash command
    // and `input`, `slash-menu` and `after-a-reply` were recorded from Claude
    // Code 2.1.302 at 120×40, and `model-while-working` from 2.1.303 at
    // 120×40 with the lines only that session showed taken out; `working` and
    // `quoting-a-dialog` are `after-a-reply` with a spinner, or a reply
    // quoting a dialog, written in. Those ending in `-named` are the screen
    // without that suffix with the session's name, `fix`, written into the
    // input box's top rule as a session started with `--name` shows it. `dev-channels` was recorded from 2.1.304
    // at 120×40 launched with a development channel, and
    // `dev-channels-confirmed` right after confirming it, with the lines only
    // that recording's environment showed taken out.

    #[test]
    fn the_pickers_and_settings_slash_commands_open_are_dialogs() {
        for name in ["model", "config", "permissions", "mcp", "theme"] {
            assert!(dialog_open(&recorded(name)), "{name}");
        }
    }

    #[test]
    fn a_picker_opened_while_claude_works_is_a_dialog_above_the_input_box() {
        assert!(dialog_open(&recorded("model-while-working")));
    }

    #[test]
    fn the_input_box_waiting_for_a_message_is_no_dialog() {
        assert!(!dialog_open(&recorded("input")));
        assert!(!dialog_open(&recorded("after-a-reply")));
    }

    #[test]
    fn the_slash_command_menu_below_the_input_box_is_no_dialog() {
        assert!(!dialog_open(&recorded("slash-menu")));
    }

    #[test]
    fn a_session_working_with_its_spinner_shows_no_dialog() {
        assert!(!dialog_open(&recorded("working")));
    }

    #[test]
    fn a_picker_opened_above_an_input_box_carrying_the_session_s_name_is_a_dialog() {
        assert!(dialog_open(&recorded("model-while-working-named")));
        assert!(!dialog_open(&recorded("working-named")));
    }

    #[test]
    fn a_rule_across_the_screen_may_carry_a_name() {
        assert!(rule(&"─".repeat(80)));
        assert!(rule(&format!("{} keys-ctrl-q ─", "─".repeat(80))));
        assert!(!rule(&format!("{} keys-ctrl-q", "─".repeat(80))));
        assert!(!rule(&format!("{}keys-ctrl-q─", "─".repeat(80))));
        assert!(!rule(&format!("  {} keys-ctrl-q ─", "─".repeat(80))));
        assert!(!rule(&format!("{} a ─ b ─", "─".repeat(80))));
        assert!(!rule(&"─".repeat(RULE_MIN - 1)));
    }

    #[test]
    fn a_reply_quoting_a_dialog_above_the_input_box_is_no_dialog() {
        assert!(!dialog_open(&recorded("quoting-a-dialog")));
    }

    #[test]
    fn an_empty_screen_shows_no_dialog() {
        assert!(!dialog_open::<&str>(&[]));
        assert!(!dialog_open(&vec![String::new(); 40]));
    }

    #[test]
    fn a_rule_with_no_key_hint_below_it_is_no_dialog() {
        let lines = ["text", &"─".repeat(80), "  Some heading", "  more text"].map(String::from);
        assert!(!dialog_open(&lines));
    }

    #[test]
    fn the_warning_before_loading_development_channels_is_a_dialog() {
        assert!(dialog_open(&recorded("dev-channels")));
    }

    #[test]
    fn the_input_box_after_confirming_the_warning_is_no_dialog() {
        assert!(!dialog_open(&recorded("dev-channels-confirmed")));
    }

    #[test]
    fn the_development_channels_warning_names_the_channels_it_would_load() {
        assert_eq!(
            development_channels(&recorded("dev-channels")),
            Some(vec!["server:adroit".to_string()])
        );
    }

    #[test]
    fn a_warning_naming_several_channels_names_each_of_them() {
        let lines = recorded("dev-channels")
            .into_iter()
            .map(|line| {
                line.replace(
                    "Channels: server:adroit",
                    "Channels: server:adroit, plugin:other@market",
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            development_channels(&lines),
            Some(vec![
                "server:adroit".to_string(),
                "plugin:other@market".to_string()
            ])
        );
    }

    #[test]
    fn the_warning_with_exit_selected_is_not_taken_for_the_warning_to_confirm() {
        let lines = recorded("dev-channels")
            .into_iter()
            .map(|line| match line.trim() {
                "❯ 1. I am using this for local development" => {
                    "    1. I am using this for local development".to_string()
                }
                "2. Exit" => "  ❯ 2. Exit".to_string(),
                _ => line,
            })
            .collect::<Vec<_>>();
        assert!(dialog_open(&lines));
        assert_eq!(development_channels(&lines), None);
    }

    #[test]
    fn no_other_screen_is_taken_for_the_development_channels_warning() {
        for name in [
            "dev-channels-confirmed",
            "input",
            "permissions",
            "model",
            "quoting-a-dialog",
        ] {
            assert_eq!(development_channels(&recorded(name)), None, "{name}");
        }
    }

    fn state_of<S: AsRef<str>>(lines: &[S]) -> Option<State> {
        judge_state(lines).value
    }

    #[test]
    fn each_recorded_screen_is_read_by_the_rule_named() {
        for (name, state, rule) in [
            (
                "working",
                Some(State::Working),
                "claude.spinner_above_input",
            ),
            (
                "working-named",
                Some(State::Working),
                "claude.spinner_above_input",
            ),
            ("input", Some(State::Idle), "claude.input_box_idle"),
            ("after-a-reply", Some(State::Idle), "claude.input_box_idle"),
            (
                "dev-channels-confirmed",
                Some(State::Idle),
                "claude.input_box_idle",
            ),
            (
                "quoting-a-dialog",
                Some(State::Idle),
                "claude.input_box_idle",
            ),
            ("slash-menu", Some(State::Idle), "claude.input_box_idle"),
            ("model", None, "claude.dialog_open"),
            ("model-while-working", None, "claude.dialog_open"),
            ("model-while-working-named", None, "claude.dialog_open"),
            ("config", None, "claude.dialog_open"),
            ("permissions", None, "claude.dialog_open"),
            ("mcp", None, "claude.dialog_open"),
            ("theme", None, "claude.dialog_open"),
            ("dev-channels", None, "claude.dialog_open"),
        ] {
            let judged = judge_state(&recorded(name));
            assert_eq!(
                (judged.value, judged.verdict.rule.as_str()),
                (state, rule),
                "{name}"
            );
        }
    }

    #[test]
    fn a_rule_that_matched_names_the_rows_it_went_by() {
        let working = judge_state(&recorded("working")).verdict;
        assert!(!working.default);
        let rows: Vec<&str> = working.evidence.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(rows, ["✻ Running… (3s · esc to interrupt)"]);
        let model = judge_state(&recorded("model")).verdict;
        assert!(model.evidence[0].text.to_lowercase().contains("esc to "));
    }

    #[test]
    fn the_input_box_with_no_sign_of_work_is_idle_by_default() {
        let idle = judge_state(&recorded("input")).verdict;
        assert!(idle.default);
        assert!(idle.evidence[0].text.starts_with('❯'));
        let none = judge_state::<&str>(&[]).verdict;
        assert_eq!(
            (none.rule.as_str(), none.default),
            ("claude.no_input_box", true)
        );
    }

    #[test]
    fn a_session_with_its_spinner_above_the_input_box_works() {
        assert_eq!(state_of(&recorded("working")), Some(State::Working));
        assert_eq!(state_of(&recorded("working-named")), Some(State::Working));
    }

    #[test]
    fn a_tip_under_the_spinner_leaves_the_session_working() {
        let lines = recorded("working")
            .into_iter()
            .flat_map(|line| {
                let tip = line
                    .starts_with('✻')
                    .then(|| "  ⎿  Tip: Name your conversations with /rename".to_string());
                std::iter::once(line).chain(tip)
            })
            .collect::<Vec<_>>();
        assert_eq!(state_of(&lines), Some(State::Working));
    }

    #[test]
    fn a_session_with_its_input_box_and_no_spinner_is_idle() {
        for name in ["input", "after-a-reply", "dev-channels-confirmed"] {
            assert_eq!(state_of(&recorded(name)), Some(State::Idle), "{name}");
        }
    }

    #[test]
    fn a_screen_without_the_input_box_tells_no_state() {
        assert_eq!(state_of::<&str>(&[]), None);
        let trust = [
            "╭──────────────────────────────────────────╮",
            "│ Do you trust the files in this folder?   │",
            "│ ❯ 1. Yes, proceed                        │",
            "│   2. No, exit                            │",
            "╰──────────────────────────────────────────╯",
        ];
        assert_eq!(state_of(&trust), None);
    }

    #[test]
    fn a_screen_with_a_dialog_of_claude_s_own_tells_no_state() {
        for name in ["model", "model-while-working", "dev-channels"] {
            assert_eq!(state_of(&recorded(name)), None, "{name}");
        }
    }

    #[test]
    fn a_reply_mentioning_a_spinner_far_above_the_input_box_is_not_one() {
        let mut lines = recorded("after-a-reply");
        let at = lines.iter().position(|l| rule(l)).unwrap();
        lines.insert(
            at.saturating_sub(10),
            "✻ Running… (3s · esc to interrupt)".into(),
        );
        assert_eq!(state_of(&lines), Some(State::Idle));
    }
}
