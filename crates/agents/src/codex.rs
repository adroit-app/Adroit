use super::*;

mod commands;
mod identity;
mod metrics;
mod models;
mod past;
mod rpc;
mod transcript;
mod usage;

pub struct Codex;

impl AgentAdapter for Codex {
    fn startup_readiness(&self, screen: &[String]) -> Option<adroit_model::StartupReadiness> {
        match self.screen_state(screen) {
            Some(adroit_model::State::Idle | adroit_model::State::Working) => {
                Some(adroit_model::StartupReadiness::Ready)
            }
            Some(adroit_model::State::Waiting(adroit_model::Wait::Permission)) => {
                Some(adroit_model::StartupReadiness::AwaitingPermission)
            }
            Some(adroit_model::State::Waiting(adroit_model::Wait::Input)) => {
                Some(adroit_model::StartupReadiness::AwaitingInput)
            }
            _ => None,
        }
    }
    fn conversation_title_needs_lookup(&self, title: &str) -> bool {
        identity::prefix(title).is_some()
    }

    fn conversation_matches_title(&self, title: &str, conversation: &str) -> bool {
        self.conversation_title(title).as_deref() == Some(conversation)
            || identity::prefix(title).is_some_and(|prefix| {
                transcript::thread_id(conversation) && conversation.starts_with(&prefix)
            })
    }

    fn resolve_conversation(
        &self,
        context: AgentContext,
        cwd: PathBuf,
        title: String,
    ) -> ConversationLookup {
        Box::pin(identity::resolve(context, cwd, title))
    }

    fn developer_instructions(&self, context: AgentContext, cwd: PathBuf) -> DeveloperInstructions {
        Box::pin(async move {
            let start = launch_in(&context, vec!["app-server".into()], Vec::new());
            let result = rpc::request(
                start,
                &cwd,
                std::time::Duration::from_secs(3),
                "config/read",
                serde_json::json!({"cwd":cwd,"includeLayers":false}),
            )
            .await
            .ok_or_else(|| {
                "cannot read the effective Codex configuration before startup".to_owned()
            })?;
            let config = result
                .get("config")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| {
                    "Codex returned an unsupported effective configuration".to_owned()
                })?;
            match config.get("developer_instructions") {
                None | Some(serde_json::Value::Null) => Ok(None),
                Some(serde_json::Value::String(instructions)) => Ok(Some(instructions.clone())),
                _ => Err("Codex returned invalid developer instructions".into()),
            }
        })
    }

    fn instruction_args(&self, instructions: &str) -> Vec<String> {
        vec![
            "-c".into(),
            format!(
                "developer_instructions={}",
                serde_json::to_string(instructions).expect("serializable instructions")
            ),
        ]
    }

    fn deliver(
        &self,
        context: AgentContext,
        cwd: PathBuf,
        conversation: String,
        envelope: String,
    ) -> DeliveryFuture {
        Box::pin(crate::delivery::codex_queue(
            context,
            cwd,
            conversation,
            envelope,
        ))
    }

    fn image_prompt(&self, text: &str, _images: &[PathBuf]) -> String {
        text.into()
    }

    fn image_pastes(&self, images: &[PathBuf]) -> Vec<String> {
        crate::quoted_pastes(images)
    }

    fn image_args(&self, images: &[PathBuf]) -> Vec<String> {
        images
            .iter()
            .flat_map(|image| ["--image".into(), image.to_string_lossy().into_owned()])
            .collect()
    }

    fn program(&self) -> String {
        "codex".into()
    }
    fn state_variable(&self) -> &'static str {
        "CODEX_HOME"
    }
    // Codex reads every `OPENAI_*` and `CODEX_*` it knows ahead of, or
    // beside, the login in its home; the home itself is set apart.
    fn denies(&self, name: &str) -> bool {
        name.starts_with("OPENAI_") || (name.starts_with("CODEX_") && name != "CODEX_HOME")
    }
    // Codex keys its MCP servers' OAuth tokens in the keychain by server
    // alone, so every home would share them; in a file they stay in the home.
    fn connection_args(&self) -> Vec<String> {
        vec!["-c".into(), "mcp_oauth_credentials_store=\"file\"".into()]
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
    fn integration_args(&self, _program: &Path, _state: &Path) -> Vec<String> {
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
        context: &AgentContext,
        home: &Path,
        session: &adroit_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        if !transcript::thread_id(&session.session_id) {
            return None;
        }
        Some(Box::new(transcript::Transcript::open(
            root(context, home),
            session.session_id.clone(),
            &session.cwd,
            home,
        )))
    }
    fn commands(
        &self,
        context: AgentContext,
        dir: PathBuf,
    ) -> Probe<crate::claude::commands::Command> {
        Box::pin(async move {
            commands::probe(
                launch_in(&context, vec!["app-server".into()], Vec::new()),
                &dir,
                crate::claude::commands::PROBE_TIMEOUT,
            )
            .await
        })
    }
    fn builtin_commands(&self) -> Vec<crate::claude::commands::Command> {
        commands::builtins()
    }
    fn models(&self, context: AgentContext, dir: PathBuf) -> Probe<adroit_model::ProviderModel> {
        Box::pin(async move {
            models::probe(
                launch_in(&context, vec!["app-server".into()], Vec::new()),
                &dir,
                crate::claude::commands::PROBE_TIMEOUT,
            )
            .await
        })
    }
    fn usage(&self, context: AgentContext, home: PathBuf) -> Probe<crate::claude::usage::Limit> {
        Box::pin(async move {
            usage::probe(
                launch_in(&context, vec!["app-server".into()], Vec::new()),
                &home,
                crate::claude::usage::PROBE_TIMEOUT,
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
                || line.trim_start().starts_with("enter/esc continue")
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

        let composer = screen.iter().rposition(|s| s.trim_start().starts_with('›'));
        if composer.is_none() && self.dialog_open(screen) {
            return Some(State::Waiting(Wait::Input));
        }
        let composer = composer?;
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
                    State::Waiting(Wait::Input)
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
    fn past(
        &self,
        context: &AgentContext,
        home: &Path,
        dir: Option<&Path>,
    ) -> Vec<adroit_model::Past> {
        past::read(&root(context, home), dir)
    }
    fn resume_args(&self, context: &AgentContext, conversation: &str) -> Option<Vec<String>> {
        let mut args = vec!["resume".into(), conversation.into()];
        args.extend(options(context));
        Some(args)
    }
    fn start_args(
        &self,
        context: &AgentContext,
        _name: &str,
        _conversation: &str,
        prompt: &str,
    ) -> Vec<String> {
        let mut args = options(context);
        args.extend(["--".into(), prompt.into()]);
        args
    }
}

/// Where Codex, started as `context` says, keeps its state: the connection's
/// directory, else `CODEX_HOME` as the launch or adroit's own environment
/// sets it, else `.codex` in `home`.
fn root(context: &AgentContext, home: &Path) -> PathBuf {
    context
        .home
        .clone()
        .or_else(|| context.launch.env.get("CODEX_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("CODEX_HOME").map(PathBuf::from))
        .unwrap_or_else(|| home.join(".codex"))
}

/// How a session runs, as `context` says: what Codex is given ahead of a
/// prompt, whether it starts a conversation or resumes one.
fn options(context: &AgentContext) -> Vec<String> {
    let mut args = vec!["--no-alt-screen".into()];
    let options = context.codex();
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
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::screen;

    #[test]
    fn a_model_migration_dialog_before_the_composer_accepts_keys_and_needs_attention() {
        let dialog = screen(
            "GPT-5.4 is no longer available\n\nCodex now uses GPT-6 Sol in place of GPT-5.4. Switch to GPT-6 Sol to continue.\nenter/esc continue · ⌃c quit\n\n",
        );
        assert!(Codex.dialog_open(&dialog));
        assert_eq!(
            Codex.screen_state(&dialog),
            Some(adroit_model::State::Waiting(adroit_model::Wait::Input))
        );
        let transcript = screen(
            "GPT-5.4 is no longer available\nenter/esc continue · ⌃c quit\n\n› Ask Codex to do anything\n? for shortcuts",
        );
        assert!(!Codex.dialog_open(&transcript));
        assert_eq!(
            Codex.screen_state(&transcript),
            Some(adroit_model::State::Idle)
        );
    }

    #[test]
    fn startup_readiness_uses_the_codex_composer_and_permission_dialog() {
        use adroit_model::StartupReadiness;
        assert_eq!(Codex.startup_readiness(&screen("Starting Codex…")), None);
        assert_eq!(
            Codex.startup_readiness(&screen("› Ask Codex\n? for shortcuts")),
            Some(StartupReadiness::Ready)
        );
        assert_eq!(Codex.startup_readiness(&screen("Would you like to run the following command?\n› 1. Yes, proceed (y)\n  2. No (esc)\nPress enter to confirm or esc to cancel")), Some(StartupReadiness::AwaitingPermission));
    }

    #[test]
    fn startup_readiness_waits_for_live_codex_pickers() {
        use adroit_model::StartupReadiness;
        for hint in ["esc to dismiss", "esc to close", "esc to go back"] {
            let live = format!(
                "Select Model and Effort\n\n› 1. Current\n  2. Other\n\nenter to select, {hint}"
            );
            assert_eq!(
                Codex.startup_readiness(&screen(&live)),
                Some(StartupReadiness::AwaitingInput),
                "{hint}"
            );
            let quoted = format!("{live}\n\n› Ask Codex\n\n? for shortcuts");
            assert_eq!(
                Codex.startup_readiness(&screen(&quoted)),
                Some(StartupReadiness::Ready),
                "{hint}"
            );
        }
    }

    #[test]
    fn a_conversation_resumes_by_its_id_run_as_a_new_session_would_be() {
        let mut context = AgentContext::builtin(Agent::Codex);
        context.codex_mut().unwrap().model = "codex-model".into();
        context.codex_mut().unwrap().sandbox_mode = "read-only".into();
        assert_eq!(
            Codex.resume_args(&context, ID),
            Some(
                [
                    "resume",
                    ID,
                    "--no-alt-screen",
                    "--model",
                    "codex-model",
                    "--sandbox",
                    "read-only"
                ]
                .map(String::from)
                .to_vec()
            )
        );
    }

    const ID: &str = "019f7a4b-fee9-74d3-9977-5bc9bb134d11";

    #[test]
    fn sessions_keep_their_output_in_terminal_scrollback() {
        let args = Codex.start_args(&AgentContext::builtin(Agent::Codex), "fix", "", "hello");
        let options = args.split(|arg| arg == "--").next().unwrap();
        assert!(options.iter().any(|arg| arg == "--no-alt-screen"));
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
        let args = Codex.integration_args(Path::new("/bin/adroit"), Path::new("/unused"));
        assert_eq!(
            args,
            ["-c", "tui.terminal_title=[\"app-name\",\"thread-id\"]"]
        );
    }

    #[test]
    fn shortened_codex_titles_are_hints_and_never_full_conversation_ids() {
        let id = "01a15b64-d462-7570-8a72-28a490123456";
        let title = format!("codex | {}...", &id[..29]);
        assert_eq!(Codex.conversation_title(&title), None);
        assert!(Codex.conversation_title_needs_lookup(&title));
        assert!(Codex.conversation_matches_title(&title, id));
        assert!(!Codex.conversation_matches_title(&title, "01a15b65-d462-7570-8a72-28a490123456"));
        assert!(!Codex.conversation_title_needs_lookup("codex | 01a..."));
        assert!(!Codex.conversation_title_needs_lookup("codex | not-a-thread..."));
    }

    #[tokio::test]
    async fn shortened_titles_resolve_only_one_native_thread_through_the_saved_route() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("provider.sh");
        let capture = dir.path().join("query.json");
        std::fs::write(&program, r#"IFS= read -r init
printf '%s\n' '{"id":"adroit-init","result":{}}'
IFS= read -r initialized
IFS= read -r query
printf '%s' "$query" > "$ADROIT_TEST_CAPTURE"
printf '%s\n' '{"id":"adroit-query","result":{"data":[{"id":"01a15b64-d462-7570-8a72-28a490123456"}],"nextCursor":null}}'
"#).unwrap();
        let mut context = AgentContext::builtin(Agent::Codex);
        context.launch.command = vec!["/bin/sh".into(), program.to_string_lossy().into_owned()];
        context.launch.env.insert(
            "ADROIT_TEST_CAPTURE".into(),
            capture.to_string_lossy().into_owned(),
        );
        let got = Codex
            .resolve_conversation(
                context,
                dir.path().into(),
                "codex | 01a15b64-d462-7570-8a72-28a49...".into(),
            )
            .await;
        assert_eq!(got.as_deref(), Some("01a15b64-d462-7570-8a72-28a490123456"));
        let query: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(capture).unwrap()).unwrap();
        assert_eq!(query["method"], "thread/list");
        assert_eq!(query["params"]["useStateDbOnly"], true);
        assert_eq!(
            query["params"]["cwd"],
            dir.path().to_string_lossy().as_ref()
        );
    }

    #[tokio::test]
    async fn ambiguous_or_incomplete_native_thread_pages_never_supply_a_recipient() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("provider.sh");
        std::fs::write(
            &program,
            r#"IFS= read -r init
printf '%s\n' '{"id":"adroit-init","result":{}}'
IFS= read -r initialized
IFS= read -r query
printf '%s\n' "$ADROIT_TEST_PAGE"
"#,
        )
        .unwrap();
        let mut context = AgentContext::builtin(Agent::Codex);
        context.launch.command = vec!["/bin/sh".into(), program.to_string_lossy().into_owned()];
        for page in [
            serde_json::json!({"data":[],"nextCursor":null}),
            serde_json::json!({"data":[{"id":"01a15b64-d462-7570-8a72-28a490123456"},{"id":"01a15b64-d462-7570-8a72-28a49abcdef0"}],"nextCursor":null}),
            serde_json::json!({"data":[{"id":"01a15b64-d462-7570-8a72-28a490123456"}],"nextCursor":"repeated"}),
        ] {
            context.launch.env.insert(
                "ADROIT_TEST_PAGE".into(),
                serde_json::json!({"id":"adroit-query","result":page}).to_string(),
            );
            assert_eq!(
                Codex
                    .resolve_conversation(
                        context.clone(),
                        dir.path().into(),
                        "codex | 01a15b64-d462-7570-8a72-28a49...".into()
                    )
                    .await,
                None
            );
        }
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
