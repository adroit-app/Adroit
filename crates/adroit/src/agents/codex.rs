use super::*;
use serde::{Deserialize, Serialize};

mod commands;
mod rpc;
mod usage;

/// How Codex sessions start, apart from Claude Code's options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CodexOptions {
    pub model: String,
}

pub struct Codex;

impl AgentAdapter for Codex {
    fn label(&self) -> &'static str {
        "Codex"
    }
    fn program(&self) -> String {
        "codex".into()
    }
    fn launch<'a>(&self, settings: &'a Settings) -> &'a Launch {
        &settings.codex
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            command_probe: true,
            usage_limits: true,
            transcript: false,
            shell_bang: false,
        }
    }
    fn usage_url(&self) -> Option<&'static str> {
        Some("https://chatgpt.com/settings/usage")
    }
    fn commands(&self, dir: PathBuf) -> Probe<adroit_claude::commands::Command> {
        Box::pin(async move {
            commands::probe(
                super::launch(Agent::Codex, vec!["app-server".into()], Vec::new()),
                &dir,
                adroit_claude::commands::PROBE_TIMEOUT,
            )
            .await
        })
    }
    fn builtin_commands(&self) -> Vec<adroit_claude::commands::Command> {
        commands::builtins()
    }
    fn usage(&self, home: PathBuf) -> Probe<adroit_claude::usage::Limit> {
        Box::pin(async move {
            usage::probe(
                super::launch(Agent::Codex, vec!["app-server".into()], Vec::new()),
                &home,
                adroit_claude::usage::PROBE_TIMEOUT,
            )
            .await
        })
    }
    fn dialog_open(&self, screen: &[String]) -> bool {
        // The composer uses the same chevron as a selected numbered row.
        // Look from the bottom so a reply quoting a menu cannot capture keys.
        let question = |line: &str| {
            let line = line.to_lowercase();
            line.contains("to submit answer") || line.contains("to submit all")
        };
        let hint = |line: &str| {
            let line = line.to_lowercase();
            question(&line)
                || line.contains("esc to cancel")
                || line.contains("esc to dismiss")
                || line.contains("esc to close")
                || line.contains("esc to go back")
                || line.contains("esc back")
        };
        let numbered = |row: &str| {
            row.trim_start()
                .split_once(". ")
                .is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        };
        for (index, line) in screen.iter().enumerate().rev() {
            if let Some(row) = line.trim_start().strip_prefix('›') {
                let below = &screen[index + 1..];
                return below.iter().any(|s| question(s))
                    || (numbered(row) && below.iter().any(|s| numbered(s) || hint(s)));
            }
        }
        screen
            .iter()
            .rev()
            .filter(|s| !s.trim().is_empty())
            .take(4)
            .any(|s| hint(s))
    }
    fn start_args(
        &self,
        settings: &Settings,
        _name: &str,
        _conversation: &str,
        reader: bool,
        prompt: &str,
    ) -> Vec<String> {
        let mut args = Vec::new();
        let model = settings.codex_sessions.model.trim();
        if !model.is_empty() {
            args.extend(["--model".into(), model.into()]);
        }
        if reader {
            args.extend(["--sandbox".into(), "read-only".into()]);
        }
        args.extend(["--".into(), prompt.into()]);
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(text: &str) -> Vec<String> {
        text.lines().map(String::from).collect()
    }

    #[test]
    fn codex_account_limits_are_available_to_the_overview() {
        assert!(Codex.capabilities().usage_limits);
    }

    #[test]
    fn codex_slash_completion_is_enabled() {
        assert!(Codex.capabilities().command_probe);
    }

    #[tokio::test]
    async fn codex_slash_commands_reach_the_embedded_prompt() {
        let list = Codex.builtin_commands();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let editor = adroit_nvim::Embedded::spawn(
            "nvim",
            &["-u", "NONE", "-i", "NONE", "--noplugin"],
            80,
            10,
            move |event| {
                let _ = tx.send(event);
            },
        )
        .await
        .unwrap();
        editor
            .set_commands(&crate::complete::commands(&list))
            .await
            .unwrap();
        editor.input("/mo").await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if editor
                    .state()
                    .popup()
                    .is_some_and(|p| p.items.iter().any(|c| c.word == "/model"))
                {
                    break;
                }
                rx.recv().await.expect("editor event");
            }
        })
        .await
        .expect("/model completion");
        editor.input("<C-y>").await.unwrap();
        assert_eq!(editor.take_text().await.unwrap(), "/model ");
    }

    #[test]
    fn codex_pickers_approvals_and_questions_take_every_key() {
        for text in [
            "Select Model and Effort\n\n› 1. gpt-5.1-codex (current)\n  2. gpt-5.1-codex-mini\n\n↑/↓ to move, enter to select, esc to dismiss",
            "Do you want to run this command?\n\n› 1. Yes, proceed (y)\n  2. No (esc)\n\nPress enter to confirm or esc to cancel",
            "Question 1/1 (1 unanswered)\nChoose an option.\n\n  › 1. Option 1\n    2. Option 2\n\n  tab to add notes | enter to submit answer | esc to interrupt",
            "Question 1/1 (1 unanswered)\nYour answer:\n› \n\nenter to submit answer | esc to interrupt",
            "Select Model and Effort\n\n› 1. gpt-5.1-codex (current)\n  2. gpt-5.1-codex-mini",
            "Question 1/1 (1 unanswered)\nYour answer:\n› \n\nenter to submit answer | esc to interrupt\n\n\n\n\n\n",
        ] {
            assert!(Codex.dialog_open(&screen(text)), "{text}");
        }
    }

    #[test]
    fn codex_composer_and_working_status_do_not_take_every_key() {
        for text in [
            "",
            "› \n\n? for shortcuts",
            "• Working (0s • esc to interrupt)\n\n› \n\n100% context left",
            "  › 1. A quoted choice\nPress enter to confirm or esc to cancel\n\n› Summarize this\n\n100% context left",
            "› /mo\n  /model  Choose what model and reasoning effort to use",
            "Question example:\n› \nenter to submit answer | esc to interrupt\n\n› Summarize this\n\n100% context left",
            "› 1. Fix the tests\n\n100% context left",
        ] {
            assert!(!Codex.dialog_open(&screen(text)), "{text}");
        }
    }
}
