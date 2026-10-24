//! Pi, the `pi` coding agent: started in its own terminal interface, asked
//! through its RPC mode, tied to adroit by an extension adroit writes, and
//! read back from the sessions it keeps.

use super::*;

mod commands;
pub(crate) mod extension;
mod rpc;
mod screen;
mod sessions;
mod transcript;

pub struct Pi;

/// The tools a read-only session is given: Pi's own that read, none that
/// write or run a command. Pi has no sandbox, so a session that keeps `bash`
/// can write wherever its user can.
pub const READ_ONLY_TOOLS: [&str; 4] = ["read", "grep", "find", "ls"];

/// Whether a connection of its own removes `name`: a variable a provider
/// reads a credential or an endpoint from, ahead of or beside the logins in
/// the connection's `auth.json` and `models.json`, or one that moves Pi's
/// sessions or packages elsewhere. The cloud providers' own variables go
/// too: Pi reads them for Bedrock, Vertex and the like without a switch.
fn denied(name: &str) -> bool {
    const FAMILIES: &[&str] = &[
        "ANTHROPIC_",
        "OPENAI_",
        "AZURE_OPENAI_",
        "GEMINI_",
        "GOOGLE_CLOUD_",
        "AWS_",
        "CLOUDFLARE_",
        "COPILOT_",
        "AI_GATEWAY_",
        "QWEN_TOKEN_PLAN_",
        "XIAOMI_TOKEN_PLAN_",
        "PI_SESSION_",
    ];
    const NAMES: &[&str] = &[
        "GCLOUD_PROJECT",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "HF_TOKEN",
        "LLAMA_BASE_URL",
        "PI_CODING_AGENT_SESSION_DIR",
        "PI_PACKAGE_DIR",
    ];
    name.ends_with("_API_KEY")
        || NAMES.contains(&name)
        || FAMILIES.iter().any(|family| name.starts_with(family))
}

/// The provider of `model`, written as `provider/id`.
fn provider(model: &str) -> Option<&str> {
    model
        .trim()
        .split_once('/')
        .map(|(provider, _)| provider)
        .filter(|p| !p.is_empty())
}

impl AgentAdapter for Pi {
    fn instruction_args(&self, instructions: &str) -> Vec<String> {
        vec!["--append-system-prompt".into(), instructions.into()]
    }

    fn image_prompt(&self, text: &str, _images: &[PathBuf]) -> String {
        text.into()
    }

    // Pi attaches no pasted path; the model reads the image with its `read`
    // tool, so each path stands apart from the text after it.
    fn image_pastes(&self, images: &[PathBuf]) -> Vec<String> {
        crate::quoted_pastes(images)
            .into_iter()
            .map(|paste| paste + " ")
            .collect()
    }

    fn image_args(&self, images: &[PathBuf]) -> Vec<String> {
        images
            .iter()
            .map(|image| format!("@{}", image.display()))
            .collect()
    }

    fn program(&self) -> String {
        "pi".into()
    }

    fn state_variable(&self) -> &'static str {
        "PI_CODING_AGENT_DIR"
    }

    fn denies(&self, name: &str) -> bool {
        denied(name)
    }

    fn login_args(&self) -> Vec<String> {
        extension::login_args()
    }

    // Pi tells whether one provider is ready, never whether any is; without
    // a provider it is only asked whether it runs at all.
    fn status_args(&self, context: &AgentContext) -> Vec<String> {
        match provider(&context.pi().model) {
            Some(provider) => ["auth", "check", "--provider", provider, "--json"]
                .map(String::from)
                .to_vec(),
            None => vec!["--version".into()],
        }
    }

    fn standing(&self, code: Option<i32>, out: &str) -> adroit_model::profile::LoginStanding {
        use adroit_model::profile::LoginStanding;
        let said = out
            .match_indices('{')
            .find_map(|(at, _)| serde_json::from_str::<serde_json::Value>(out[at..].trim()).ok())
            .and_then(|v| v["status"].as_str().map(String::from));
        match (code, said.as_deref()) {
            (Some(0), Some("ready")) => LoginStanding::LoggedIn,
            (Some(1), Some("not_ready")) => LoginStanding::LoggedOut,
            (Some(126 | 127), None) => LoginStanding::Missing,
            _ => LoginStanding::Unknown,
        }
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            command_probe: true,
            usage_limits: false,
            transcript: true,
            shell_bang: true,
            screen_state: true,
        }
    }

    fn builtin_commands(&self) -> Vec<crate::claude::commands::Command> {
        commands::builtins()
    }

    fn commands(
        &self,
        context: AgentContext,
        dir: PathBuf,
    ) -> Probe<crate::claude::commands::Command> {
        Box::pin(async move {
            let start = launch_in(&context, rpc::args(&context), Vec::new());
            commands::probe(start, &dir, crate::claude::commands::PROBE_TIMEOUT).await
        })
    }

    fn models_within(
        &self,
        context: AgentContext,
        dir: PathBuf,
        timeout: std::time::Duration,
    ) -> Probe<adroit_model::ProviderModel> {
        Box::pin(async move {
            let start = launch_in(&context, rpc::args(&context), Vec::new());
            rpc::models(start, &dir, timeout).await
        })
    }

    fn transcript(
        &self,
        context: &AgentContext,
        home: &Path,
        session: &adroit_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        if session.session_id.is_empty() {
            return None;
        }
        Some(Box::new(transcript::Transcript::open(
            sessions::root(context, home),
            session.session_id.clone(),
            &session.cwd,
            home,
        )))
    }

    fn list_sessions(&self, _context: AgentContext) -> Listing {
        Box::pin(async { Ok(extension::running(&extension::running_dir())) })
    }

    fn follow(
        &self,
        own: &mut [adroit_model::Session],
        listed: Vec<adroit_model::Running>,
    ) -> Vec<(u128, Learned)> {
        crate::claude::follow(Agent::Pi, own, listed)
    }

    fn resume_args(&self, context: &AgentContext, conversation: &str) -> Option<Vec<String>> {
        let mut args = options(context);
        args.extend(["--session-id".into(), conversation.into()]);
        Some(args)
    }

    fn past(
        &self,
        context: &AgentContext,
        home: &Path,
        dir: Option<&Path>,
    ) -> Vec<adroit_model::Past> {
        crate::kept_in(context, sessions::past(&sessions::root(context, home), dir))
    }

    fn conversation(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn start_args(
        &self,
        context: &AgentContext,
        name: &str,
        conversation: &str,
        prompt: &str,
    ) -> Vec<String> {
        let mut args = options(context);
        args.extend(["--session-id".into(), conversation.into()]);
        if !name.trim().is_empty() {
            args.extend(["--name".into(), name.into()]);
        }
        if !prompt.is_empty() {
            // After `--` a word starting with `@` is still a file to attach.
            let prompt = match prompt.starts_with('@') {
                true => format!(" {prompt}"),
                false => prompt.into(),
            };
            args.extend(["--".into(), prompt]);
        }
        args
    }

    fn integration_args(&self, program: &Path, state: &Path) -> Vec<String> {
        extension::args(program, &state.join("pi"))
    }

    fn history(&self, context: &AgentContext, home: &Path, project: &Path) -> Vec<String> {
        sessions::history(&sessions::root(context, home), project)
    }

    fn dialog_open(&self, screen: &[String]) -> bool {
        screen::dialog_open(screen)
    }

    fn startup_readiness(&self, screen: &[String]) -> Option<adroit_model::StartupReadiness> {
        screen::startup_readiness(screen)
    }

    fn screen_state(&self, screen: &[String]) -> Option<adroit_model::State> {
        screen::state(screen)
    }

    fn rename_command(&self, name: &str) -> Option<String> {
        Some(format!("/name {name}"))
    }
}

/// How a session runs, as `context` says, whether it starts a conversation
/// or resumes one. In a connection of its own, Pi keeps its sessions in the
/// connection whatever a project's settings say, and loads only the skills
/// the connection keeps, not those every Pi of the user shares.
fn options(context: &AgentContext) -> Vec<String> {
    let mut args = vec!["--tui-mode".into(), "regular".into()];
    let options = context.pi();
    for (flag, value) in [
        ("--model", &options.model),
        ("--thinking", &options.thinking),
    ] {
        if !value.trim().is_empty() {
            args.extend([flag.into(), value.trim().into()]);
        }
    }
    let tools: Vec<&str> = options
        .tools
        .iter()
        .map(|tool| tool.trim())
        .filter(|tool| !tool.is_empty())
        .collect();
    if !tools.is_empty() {
        args.extend(["--tools".into(), tools.join(",")]);
    }
    if let Some(home) = &context.home {
        args.extend([
            "--session-dir".into(),
            home.join("sessions").display().to_string(),
            "--no-skills".into(),
        ]);
        let skills = home.join("skills");
        if skills.is_dir() {
            args.extend(["--skill".into(), skills.display().to_string()]);
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::config::PiOptions;
    use adroit_model::profile::AgentOptions;

    fn with(options: PiOptions) -> AgentContext {
        AgentContext {
            options: AgentOptions::Pi(options),
            ..AgentContext::builtin(Agent::Pi)
        }
    }

    #[test]
    fn a_session_starts_pi_s_own_interface_in_the_terminal_s_scrollback_with_the_id_adroit_chose() {
        assert_eq!(
            Pi.start_args(&AgentContext::builtin(Agent::Pi), "fix", "c0ffee", "read"),
            [
                "--tui-mode",
                "regular",
                "--session-id",
                "c0ffee",
                "--name",
                "fix",
                "--",
                "read"
            ]
        );
        assert!(!Pi.conversation().is_empty());
        assert_ne!(Pi.conversation(), Pi.conversation());
    }

    #[test]
    fn a_profile_s_model_thinking_and_tools_are_pi_s_own_flags() {
        let context = with(PiOptions {
            model: " ollama/qwen3 ".into(),
            thinking: "high".into(),
            tools: vec!["read".into(), " ".into(), "bash".into()],
        });
        let args = Pi.start_args(&context, "", "id", "--help");
        assert_eq!(
            args,
            [
                "--tui-mode",
                "regular",
                "--model",
                "ollama/qwen3",
                "--thinking",
                "high",
                "--tools",
                "read,bash",
                "--session-id",
                "id",
                "--",
                "--help"
            ]
        );
    }

    #[test]
    fn a_first_prompt_that_starts_like_a_file_is_still_the_prompt() {
        let args = Pi.start_args(&AgentContext::builtin(Agent::Pi), "n", "id", "@here look");
        assert_eq!(args.last().map(String::as_str), Some(" @here look"));
        let args = Pi.start_args(&AgentContext::builtin(Agent::Pi), "n", "id", "");
        assert!(!args.contains(&"--".to_string()), "{args:?}");
    }

    #[test]
    fn images_are_attached_on_start_and_named_by_path_when_pasted() {
        let images = [
            PathBuf::from("/tmp/my shot.png"),
            PathBuf::from("/tmp/b.png"),
        ];
        assert_eq!(Pi.image_args(&images), ["@/tmp/my shot.png", "@/tmp/b.png"]);
        assert_eq!(Pi.image_prompt("look", &images), "look");
        assert_eq!(
            Pi.image_pastes(&images),
            ["\"/tmp/my shot.png\" ", "\"/tmp/b.png\" "]
        );
    }

    #[test]
    fn a_conversation_resumes_by_its_id_run_as_a_new_session_would_be() {
        let context = with(PiOptions {
            model: "fake/slow".into(),
            ..PiOptions::default()
        });
        assert_eq!(
            Pi.resume_args(&context, "c0ffee").unwrap(),
            [
                "--tui-mode",
                "regular",
                "--model",
                "fake/slow",
                "--session-id",
                "c0ffee"
            ]
        );
    }

    #[test]
    fn in_a_connection_pi_keeps_its_sessions_there_and_loads_only_the_connection_s_skills() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("work");
        let context = AgentContext {
            home: Some(home.clone()),
            ..AgentContext::builtin(Agent::Pi)
        };
        let args = Pi.resume_args(&context, "id").unwrap();
        let sessions = home.join("sessions").display().to_string();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--session-dir", sessions.as_str()]),
            "{args:?}"
        );
        assert!(args.contains(&"--no-skills".to_string()), "{args:?}");
        assert!(!args.contains(&"--skill".to_string()), "{args:?}");
        std::fs::create_dir_all(home.join("skills")).unwrap();
        let args = Pi.resume_args(&context, "id").unwrap();
        let skills = home.join("skills").display().to_string();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--skill", skills.as_str()]),
            "{args:?}"
        );
        // In its own environment Pi is left as it runs outside adroit.
        let own = Pi
            .resume_args(&AgentContext::builtin(Agent::Pi), "id")
            .unwrap();
        assert!(
            !own.iter()
                .any(|a| a == "--no-skills" || a == "--session-dir")
        );
    }

    #[test]
    fn a_connection_removes_every_provider_s_credentials_and_what_moves_pi_s_state() {
        for name in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_OAUTH_TOKEN",
            "OPENAI_API_KEY",
            "OPENAI_BASE_URL",
            "AZURE_OPENAI_RESOURCE_NAME",
            "GEMINI_API_KEY",
            "GOOGLE_CLOUD_PROJECT",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "GCLOUD_PROJECT",
            "AWS_PROFILE",
            "AWS_BEARER_TOKEN_BEDROCK",
            "CLOUDFLARE_ACCOUNT_ID",
            "COPILOT_GITHUB_TOKEN",
            "DEEPSEEK_API_KEY",
            "MINIMAX_CN_API_KEY",
            "SOMEONE_NEW_API_KEY",
            "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
            "QWEN_TOKEN_PLAN_API_KEY",
            "HF_TOKEN",
            "LLAMA_BASE_URL",
            "PI_CODING_AGENT_SESSION_DIR",
            "PI_PACKAGE_DIR",
            "PI_SESSION_ID",
            "PI_SESSION_FILE",
        ] {
            assert!(Pi.denies(name), "{name}");
        }
        for name in ["PATH", "HOME", "PI_SKIP_VERSION_CHECK", "TERM", "API_KEYS"] {
            assert!(!Pi.denies(name), "{name}");
        }
        assert_eq!(Pi.state_variable(), "PI_CODING_AGENT_DIR");
    }

    #[test]
    fn pi_offers_no_account_usage_and_runs_shell_lines_typed_after_a_bang() {
        let capabilities = Pi.capabilities();
        assert!(!capabilities.usage_limits);
        assert!(capabilities.shell_bang);
        assert!(capabilities.transcript);
        assert!(capabilities.command_probe);
        assert_eq!(Pi.usage_url(), None);
        assert_eq!(Pi.rename_command("fix").as_deref(), Some("/name fix"));
    }

    #[test]
    fn adroit_s_instructions_are_added_to_pi_s_own_system_prompt() {
        assert_eq!(
            Pi.instruction_args("be brief"),
            ["--append-system-prompt", "be brief"]
        );
    }
}
