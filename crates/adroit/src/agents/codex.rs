use super::*;
use serde::{Deserialize, Serialize};

mod commands;
mod rpc;
mod transcript;
mod usage;

/// How Codex sessions start, apart from Claude Code's options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CodexOptions {
    pub model: String,
    pub reasoning_effort: String,
    pub approval_policy: String,
    pub sandbox_mode: String,
}

impl CodexOptions {
    pub fn reader() -> Self {
        Self {
            sandbox_mode: "read-only".into(),
            ..Self::default()
        }
    }
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
            transcript: true,
            shell_bang: false,
            screen_state: true,
        }
    }
    fn usage_url(&self) -> Option<&'static str> {
        Some("https://chatgpt.com/settings/usage")
    }
    fn integration_args(&self, _program: &Path) -> Vec<String> {
        vec![
            "-c".into(),
            "tui.terminal_title=[\"app-name\",\"thread-id\"]".into(),
        ]
    }
    fn conversation_title(&self, title: &str) -> Option<String> {
        title
            .split_whitespace()
            .find(|s| transcript::thread_id(s))
            .map(str::to_string)
    }
    fn transcript(
        &self,
        home: &Path,
        session: &adroit_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        if !transcript::thread_id(&session.session_id) {
            return None;
        }
        let root = crate::settings::current()
            .codex
            .env
            .get("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("CODEX_HOME").map(PathBuf::from))
            .unwrap_or_else(|| home.join(".codex"));
        Some(Box::new(transcript::Transcript::open(
            root,
            session.session_id.clone(),
            &session.cwd,
            home,
        )))
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
    fn screen_state(&self, screen: &[String]) -> Option<adroit_model::State> {
        use adroit_model::{State, Wait};

        let composer = screen
            .iter()
            .rposition(|s| s.trim_start().starts_with('›'))?;
        let below = &screen[composer + 1..];
        if self.dialog_open(screen) {
            return Some(
                if below
                    .iter()
                    .any(|s| s.contains("to submit answer") || s.contains("to submit all"))
                {
                    State::Waiting(Wait::Input)
                } else if below
                    .iter()
                    .any(|s| s.contains("Press enter to confirm or esc to cancel"))
                {
                    State::Waiting(Wait::Permission)
                } else {
                    State::Idle
                },
            );
        }
        // A previous composer belongs to the transcript, not the live footer.
        let working = screen[..composer]
            .iter()
            .rev()
            .take_while(|s| !s.trim_start().starts_with('›'))
            .any(|s| s.trim_start().starts_with("• ") && s.contains("esc to interrupt"));
        Some(if working { State::Working } else { State::Idle })
    }
    fn start_args(
        &self,
        settings: &Settings,
        _name: &str,
        _conversation: &str,
        reader: bool,
        prompt: &str,
    ) -> Vec<String> {
        let mut args = vec!["--no-alt-screen".into()];
        let options = if reader {
            &settings.codex_readers
        } else {
            &settings.codex_sessions
        };
        let model = options.model.trim();
        if !model.is_empty() {
            args.extend(["--model".into(), model.into()]);
        }
        let effort = options.reasoning_effort.trim();
        if !effort.is_empty() {
            args.extend([
                "-c".into(),
                format!(
                    "model_reasoning_effort={}",
                    serde_json::to_string(effort).unwrap()
                ),
            ]);
        }
        for (flag, value) in [
            ("--ask-for-approval", &options.approval_policy),
            ("--sandbox", &options.sandbox_mode),
        ] {
            if !value.trim().is_empty() {
                args.extend([flag.into(), value.trim().into()]);
            }
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
    fn each_role_passes_its_own_codex_options_before_the_prompt() {
        let mut settings = Settings {
            codex_sessions: CodexOptions {
                model: "worker".into(),
                reasoning_effort: "high".into(),
                approval_policy: "on-request".into(),
                sandbox_mode: "workspace-write".into(),
            },
            codex_readers: CodexOptions {
                model: "reader".into(),
                reasoning_effort: "low".into(),
                approval_policy: "never".into(),
                sandbox_mode: "read-only".into(),
            },
            ..Settings::default()
        };
        for (reader, model, effort, approval, sandbox) in [
            (false, "worker", "high", "on-request", "workspace-write"),
            (true, "reader", "low", "never", "read-only"),
        ] {
            assert_eq!(
                Codex.start_args(&settings, "", "", reader, "--help"),
                [
                    "--no-alt-screen",
                    "--model",
                    model,
                    "-c",
                    &format!("model_reasoning_effort=\"{effort}\""),
                    "--ask-for-approval",
                    approval,
                    "--sandbox",
                    sandbox,
                    "--",
                    "--help",
                ]
            );
        }
        settings.codex_readers = CodexOptions::default();
        assert_eq!(
            Codex.start_args(&settings, "", "", true, "read"),
            ["--no-alt-screen", "--", "read"]
        );
    }

    #[test]
    fn sessions_and_readers_keep_their_output_in_terminal_scrollback() {
        for reader in [false, true] {
            let args = Codex.start_args(&Settings::default(), "fix", "", reader, "hello");
            let options = args.split(|arg| arg == "--").next().unwrap();
            assert!(options.iter().any(|arg| arg == "--no-alt-screen"));
        }
    }

    #[test]
    fn codex_activity_is_read_from_the_live_composer_when_no_status_is_reported() {
        use adroit_model::{State, Wait};

        for (text, expected) in [
            (
                "• Working (2s • esc to interrupt)\n\n› \n\n100% context left",
                State::Working,
            ),
            (
                "• Reading files (2s • esc to interrupt)\n\n› Ask Codex to do anything\n\n? for shortcuts",
                State::Working,
            ),
            (
                "Would you like to run the following command?\n\n› 1. Yes, proceed (y)\n  2. No (esc)\n\nPress enter to confirm or esc to cancel",
                State::Waiting(Wait::Permission),
            ),
            (
                "Question 1/1 (1 unanswered)\nYour answer:\n› \n\nenter to submit answer | esc to interrupt",
                State::Waiting(Wait::Input),
            ),
            ("› \n\n? for shortcuts", State::Idle),
            ("• Worked for 2s\n\n› \n\n100% context left", State::Idle),
        ] {
            assert_eq!(Codex.screen_state(&screen(text)), Some(expected), "{text}");
        }
    }

    #[test]
    fn codex_activity_ignores_quoted_status_and_does_not_call_pickers_approval_requests() {
        use adroit_model::State;

        for text in [
            "• Working (2s • esc to interrupt)\n\n› old question\nAnswer\n\n› \n\n100% context left",
            "Would you like to run the following command?\n› 1. Yes\nPress enter to confirm or esc to cancel\n\n› \n\n100% context left",
            "Select Model and Effort\n\n› 1. gpt-5.1-codex\n  2. gpt-5.1-codex-mini\n\n↑/↓ to move, enter to select, esc to dismiss",
        ] {
            assert_eq!(
                Codex.screen_state(&screen(text)),
                Some(State::Idle),
                "{text}"
            );
        }
        assert_eq!(Codex.screen_state(&[]), None);
        assert_eq!(Codex.screen_state(&screen("Starting Codex…")), None);
    }

    #[test]
    fn codex_account_limits_are_available_to_the_overview() {
        assert!(Codex.capabilities().usage_limits);
    }

    #[test]
    fn codex_slash_completion_is_enabled() {
        assert!(Codex.capabilities().command_probe);
    }

    #[test]
    fn codex_details_follow_the_thread_reported_in_its_terminal_title() {
        let id = "01234567-89ab-cdef-0123-456789abcdef";
        assert!(Codex.capabilities().transcript);
        assert_eq!(
            Codex.conversation_title(&format!("Codex · {id}")),
            Some(id.into())
        );
        assert_eq!(Codex.conversation_title("Codex · project"), None);
        assert_eq!(
            Codex.conversation_title("01234567-89ab-cdef-0123-456789abcdeg"),
            None
        );
        let args = Codex.integration_args(Path::new("/bin/adroit"));
        assert_eq!(
            args,
            ["-c", "tui.terminal_title=[\"app-name\",\"thread-id\"]"]
        );
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
