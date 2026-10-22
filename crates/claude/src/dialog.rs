//! Telling from Claude Code's screen whether one of its own dialogs, such as
//! the picker `/model` opens, has the keys in place of its input box.

/// Whether the screen whose rows are `lines` shows a dialog of Claude Code's
/// that takes keys: one drawn below a rule across the screen, with a hint of
/// its keys such as "Esc to cancel". It takes the place of the input box, a
/// prompt between two such rules, or sits right above it while Claude works.
pub fn dialog_open<S: AsRef<str>>(lines: &[S]) -> bool {
    let line = |i: usize| lines[i].as_ref();
    let rules: Vec<usize> = (0..lines.len()).filter(|&i| rule(line(i))).collect();
    let input = rules.windows(2).rposition(|w| prompt(line(w[0] + 1)));
    let (from, to) = match input {
        Some(0) => return false,
        Some(at) => (rules[at - 1], rules[at]),
        None => match rules.last() {
            Some(&last) => (last, lines.len()),
            None => return false,
        },
    };
    (from + 1..to).any(|i| key_hint(line(i)))
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

fn rule(line: &str) -> bool {
    let line = line.trim_end();
    line.starts_with('─') && line.chars().all(|c| c == '─') && line.chars().count() >= RULE_MIN
}

/// The first line of the input box, after its prompt: `❯`, or `!` in bash mode.
fn prompt(line: &str) -> bool {
    line.starts_with(['❯', '>', '!'])
}

/// A footer naming the dialog's keys. Every dialog lets Escape leave it,
/// written "Esc to cancel", "Esc to close", "esc to clear" and the like.
fn key_hint(line: &str) -> bool {
    line.to_lowercase().contains("esc to ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A screen kept under `tests/screens`. Those named after a slash command
    /// and `input`, `slash-menu` and `after-a-reply` were recorded from Claude
    /// Code 2.1.302 at 120×40, and `model-while-working` from 2.1.303 at
    /// 120×40 with the lines only that session showed taken out; `working` and
    /// `quoting-a-dialog` are `after-a-reply` with a spinner, or a reply
    /// quoting a dialog, written in. `dev-channels` was recorded from 2.1.304
    /// at 120×40 launched with a development channel, and
    /// `dev-channels-confirmed` right after confirming it, with the lines only
    /// that recording's environment showed taken out.
    fn screen(name: &str) -> Vec<String> {
        let path = format!("{}/tests/screens/{name}.txt", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn the_pickers_and_settings_slash_commands_open_are_dialogs() {
        for name in ["model", "config", "permissions", "mcp", "theme"] {
            assert!(dialog_open(&screen(name)), "{name}");
        }
    }

    #[test]
    fn a_picker_opened_while_claude_works_is_a_dialog_above_the_input_box() {
        assert!(dialog_open(&screen("model-while-working")));
    }

    #[test]
    fn the_input_box_waiting_for_a_message_is_no_dialog() {
        assert!(!dialog_open(&screen("input")));
        assert!(!dialog_open(&screen("after-a-reply")));
    }

    #[test]
    fn the_slash_command_menu_below_the_input_box_is_no_dialog() {
        assert!(!dialog_open(&screen("slash-menu")));
    }

    #[test]
    fn a_session_working_with_its_spinner_shows_no_dialog() {
        assert!(!dialog_open(&screen("working")));
    }

    #[test]
    fn a_reply_quoting_a_dialog_above_the_input_box_is_no_dialog() {
        assert!(!dialog_open(&screen("quoting-a-dialog")));
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
        assert!(dialog_open(&screen("dev-channels")));
    }

    #[test]
    fn the_input_box_after_confirming_the_warning_is_no_dialog() {
        assert!(!dialog_open(&screen("dev-channels-confirmed")));
    }

    #[test]
    fn the_development_channels_warning_names_the_channels_it_would_load() {
        assert_eq!(
            development_channels(&screen("dev-channels")),
            Some(vec!["server:adroit_control".to_string()])
        );
    }

    #[test]
    fn a_warning_naming_several_channels_names_each_of_them() {
        let lines = screen("dev-channels")
            .into_iter()
            .map(|line| {
                line.replace(
                    "Channels: server:adroit_control",
                    "Channels: server:adroit_control, plugin:other@market",
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            development_channels(&lines),
            Some(vec![
                "server:adroit_control".to_string(),
                "plugin:other@market".to_string()
            ])
        );
    }

    #[test]
    fn the_warning_with_exit_selected_is_not_taken_for_the_warning_to_confirm() {
        let lines = screen("dev-channels")
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
            assert_eq!(development_channels(&screen(name)), None, "{name}");
        }
    }
}
