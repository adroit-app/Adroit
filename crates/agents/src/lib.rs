//! The agents adroit runs in its consoles, each behind one adapter: how it is
//! started, asked things outside a console, and read back.

pub mod claude;
mod codex;
mod delivery;
pub use delivery::{DeliveryFailure, DeliveryFuture};

use adroit_model::Agent;
use adroit_model::config::{CodexOptions, Launch, SessionOptions};
use std::path::{Path, PathBuf};

/// A command that starts an agent: the program, its arguments and the
/// variables set for it. What asks an agent something takes a way to make
/// one from the arguments it needs, so it starts as the user has sessions
/// start.
pub type Started = (String, Vec<String>, Vec<(String, String)>);

/// What the settings say of how agents start: an adapter is handed these
/// rather than reading the settings itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentSettings {
    /// The shell a program started through the shell runs in.
    pub shell: String,
    pub claude: Launch,
    pub codex: Launch,
    /// How Claude Code sessions run.
    pub sessions: SessionOptions,
    pub codex_sessions: CodexOptions,
}

impl AgentSettings {
    pub fn launch(&self, agent: Agent) -> &Launch {
        match agent {
            Agent::Claude => &self.claude,
            Agent::Codex => &self.codex,
        }
    }
}

/// What a session learned from its agent's listing, kept on the session so a
/// window opened later knows it too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Learned {
    /// The conversation the session's process moved to.
    Conversation(String),
    /// The process that holds the session's conversation.
    Pid(u32),
}

/// Optional integrations beyond the shared terminal and prompt.
pub struct Capabilities {
    pub command_probe: bool,
    pub usage_limits: bool,
    pub transcript: bool,
    pub shell_bang: bool,
    pub screen_state: bool,
}

type Listing = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<adroit_model::Session>, String>> + Send>,
>;

type Probe<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Option<Vec<T>>> + Send>>;

type ContextProbe = std::pin::Pin<Box<dyn std::future::Future<Output = Option<(u64, u64)>> + Send>>;

/// A provider reader produces the details the overview can display.
pub trait ConversationReader: Send {
    fn read(&mut self) -> crate::claude::links::Links;
    fn stats(&self) -> adroit_model::ConversationStats {
        adroit_model::ConversationStats::default()
    }
}

type DeveloperInstructions =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, String>> + Send>>;

type ConversationLookup =
    std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>>;

/// A terminal agent owns its arguments, environment and optional integrations.
pub trait AgentAdapter: Sync {
    fn conversation_title_needs_lookup(&self, _title: &str) -> bool {
        false
    }

    fn conversation_matches_title(&self, title: &str, conversation: &str) -> bool {
        self.conversation_title(title).as_deref() == Some(conversation)
    }

    fn resolve_conversation(
        &self,
        _settings: AgentSettings,
        _cwd: PathBuf,
        _title: String,
    ) -> ConversationLookup {
        Box::pin(async { None })
    }

    fn developer_instructions(
        &self,
        _settings: AgentSettings,
        _cwd: PathBuf,
    ) -> DeveloperInstructions {
        Box::pin(async { Ok(None) })
    }

    /// Arguments that add Adroit's `instructions` to the agent's own.
    fn instruction_args(&self, instructions: &str) -> Vec<String>;

    fn deliver(
        &self,
        _settings: AgentSettings,
        _cwd: PathBuf,
        _conversation: String,
        _envelope: String,
    ) -> DeliveryFuture {
        Box::pin(async { Err(DeliveryFailure::Unsupported) })
    }

    fn image_prompt(&self, text: &str, images: &[PathBuf]) -> String;
    fn image_pastes(&self, _images: &[PathBuf]) -> Vec<String> {
        Vec::new()
    }
    fn image_args(&self, _images: &[PathBuf]) -> Vec<String> {
        Vec::new()
    }
    fn label(&self) -> &'static str;
    fn program(&self) -> String;
    /// What starts the agent with `args` as `launch` says, `shell` being the
    /// shell it may be started through: the program, its arguments, and `env`
    /// with the agent's own variables and then the launch's.
    fn launch(
        &self,
        launch: &Launch,
        shell: &str,
        args: Vec<String>,
        env: Vec<(String, String)>,
    ) -> crate::Started {
        let (program, args) = launch.command_line(&self.program(), &args, shell);
        let env = env
            .into_iter()
            .chain(self.environment())
            .chain(launch.env.clone())
            .collect();
        (program, args, env)
    }
    fn capabilities(&self) -> Capabilities;
    fn usage_url(&self) -> Option<&'static str> {
        None
    }
    fn builtin_commands(&self) -> Vec<crate::claude::commands::Command> {
        Vec::new()
    }
    fn commands(
        &self,
        _settings: AgentSettings,
        _dir: PathBuf,
    ) -> Probe<crate::claude::commands::Command> {
        Box::pin(async { None })
    }
    /// The models the provider accepts, asked of it in `dir` as `settings` start it.
    fn models(
        &self,
        _settings: AgentSettings,
        _dir: PathBuf,
    ) -> Probe<adroit_model::ProviderModel> {
        Box::pin(async { None })
    }
    fn usage(
        &self,
        _settings: AgentSettings,
        _home: PathBuf,
    ) -> Probe<crate::claude::usage::Limit> {
        Box::pin(async { None })
    }
    /// How full the context of `session`'s conversation, run on `model`, is,
    /// for an agent whose transcript does not say.
    fn context(
        &self,
        _settings: AgentSettings,
        _session: &adroit_model::Session,
        _model: Option<String>,
    ) -> ContextProbe {
        Box::pin(async { None })
    }
    fn transcript(
        &self,
        _settings: &AgentSettings,
        _home: &Path,
        _session: &adroit_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        None
    }
    fn list_sessions(&self, _settings: AgentSettings) -> Listing {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn follow(
        &self,
        _own: &mut [adroit_model::Session],
        listed: Vec<adroit_model::Session>,
    ) -> (Vec<adroit_model::Session>, Vec<(u128, Learned)>) {
        (listed, Vec::new())
    }
    fn resume_args(&self, _settings: &AgentSettings, _conversation: &str) -> Option<Vec<String>> {
        None
    }
    fn conversation(&self) -> String {
        String::new()
    }
    fn conversation_title(&self, _title: &str) -> Option<String> {
        None
    }
    fn start_args(
        &self,
        settings: &AgentSettings,
        name: &str,
        conversation: &str,
        prompt: &str,
    ) -> Vec<String>;
    /// Arguments that tie the agent to adroit, `program` being adroit's own
    /// command and `state` the folder this instance keeps its state in, where
    /// what the agent loads may be written.
    fn integration_args(&self, _program: &Path, _state: &Path) -> Vec<String> {
        Vec::new()
    }
    fn environment(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    fn history(&self, _project: &Path) -> Vec<String> {
        Vec::new()
    }
    fn dialog_open(&self, _screen: &[String]) -> bool {
        false
    }
    fn startup_readiness(&self, _screen: &[String]) -> Option<adroit_model::StartupReadiness> {
        None
    }
    fn screen_state(&self, _screen: &[String]) -> Option<adroit_model::State> {
        None
    }
    fn rename_command(&self, _name: &str) -> Option<String> {
        None
    }
}

pub fn adapter(agent: Agent) -> &'static dyn AgentAdapter {
    match agent {
        Agent::Claude => &claude::Claude,
        Agent::Codex => &codex::Codex,
    }
}

pub fn launch_with_settings(
    settings: &AgentSettings,
    agent: Agent,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> crate::Started {
    adapter(agent).launch(settings.launch(agent), &settings.shell, args, env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::Agent;
    #[test]
    fn images_are_prepared_for_the_receiving_agent_only_when_sent() {
        let images = vec![
            PathBuf::from("/tmp/my shot.png"),
            PathBuf::from("/tmp/two.png"),
        ];
        let claude = adapter(Agent::Claude);
        assert_eq!(
            claude.image_prompt("look", &images),
            "look @\"/tmp/my shot.png\" @\"/tmp/two.png\""
        );
        assert!(claude.image_pastes(&images).is_empty());
        let codex = adapter(Agent::Codex);
        assert_eq!(codex.image_prompt("look", &images), "look");
        assert_eq!(
            codex.image_pastes(&images),
            ["\"/tmp/my shot.png\"", "\"/tmp/two.png\""]
        );
        assert_eq!(
            codex.image_args(&images),
            ["--image", "/tmp/my shot.png", "--image", "/tmp/two.png"]
        );
        assert_eq!(claude.image_prompt("", &[]), "");
    }

    #[test]
    fn each_agent_starts_as_its_own_launch_says_its_variables_under_the_launch_s() {
        let settings = AgentSettings {
            codex: Launch {
                command: vec!["/opt/codex".into()],
                env: [("CODEX_HOME".to_string(), "/c".to_string())].into(),
                ..Launch::default()
            },
            ..AgentSettings::default()
        };
        let (program, args, env) =
            launch_with_settings(&settings, Agent::Codex, vec!["exec".into()], Vec::new());
        assert_eq!(
            (program.as_str(), args),
            ("/opt/codex", vec!["exec".to_string()])
        );
        assert_eq!(env, [("CODEX_HOME".to_string(), "/c".to_string())]);
        let (program, _, env) = adapter(Agent::Claude).launch(
            settings.launch(Agent::Claude),
            "/bin/zsh",
            Vec::new(),
            vec![("A".into(), "1".into())],
        );
        assert_eq!(program, crate::claude::claude_program());
        assert_eq!(env.first(), Some(&("A".to_string(), "1".to_string())));
        assert_eq!(env.len(), 1 + crate::claude::ENV.len());
    }

    #[test]
    fn usage_settings_open_the_account_page_for_each_agent() {
        assert_eq!(
            adapter(Agent::Claude).usage_url(),
            Some("https://claude.ai/settings/usage")
        );
        assert_eq!(
            adapter(Agent::Codex).usage_url(),
            Some("https://chatgpt.com/settings/usage")
        );
    }
    #[test]
    fn codex_receives_only_its_own_options_and_preserves_flag_like_prompts() {
        let mut settings = AgentSettings::default();
        settings.sessions.model = "claude-model".into();
        settings.codex_sessions.model = "codex-model".into();
        let a = adapter(Agent::Codex);
        assert_eq!(
            a.start_args(&settings, "fix", "", "--help"),
            ["--no-alt-screen", "--model", "codex-model", "--", "--help"]
        );
        assert_eq!(
            a.start_args(&AgentSettings::default(), "fix", "", "read"),
            ["--no-alt-screen", "--", "read"]
        );
        assert!(a.conversation().is_empty());
        assert!(a.capabilities().transcript);
        assert!(a.capabilities().command_probe);
        assert!(!a.capabilities().shell_bang);
    }
    #[tokio::test]
    async fn unsupported_integrations_never_start_claude_for_codex() {
        let a = adapter(Agent::Codex);
        assert!(
            !a.integration_args(Path::new("/unused/adroit"), Path::new("/unused"))
                .is_empty()
        );
        assert!(a.environment().is_empty());
        assert!(a.history(Path::new("/unused")).is_empty());
        assert!(
            a.list_sessions(AgentSettings::default())
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            a.transcript(
                &AgentSettings::default(),
                Path::new("/unused"),
                &Default::default()
            )
            .is_none()
        );
        assert!(a.resume_args(&AgentSettings::default(), "uuid").is_none());
        assert!(a.rename_command("fix").is_none());
    }

    #[test]
    fn claude_keeps_its_conversation_options_and_integrations() {
        let a = adapter(Agent::Claude);
        assert_eq!(
            a.start_args(&AgentSettings::default(), "fix", "uuid", "read"),
            ["--name", "fix", "--session-id", "uuid", "--", "read"]
        );
        assert!(!a.conversation().is_empty());
        assert!(a.capabilities().transcript);
        assert!(a.capabilities().command_probe);
        assert!(a.capabilities().shell_bang);
    }

    #[test]
    fn claude_startup_classification_requires_a_live_prompt_or_human_decision() {
        use adroit_model::StartupReadiness::*;
        let claude = adapter(Agent::Claude);
        let screen = |text: &str| text.lines().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            claude.startup_readiness(&screen("Trust this project?\n1. Yes\n2. No")),
            Some(AwaitingTrust)
        );
        assert_eq!(
            claude.startup_readiness(&screen("Do you want to allow this tool?\n1. Yes\n2. No")),
            Some(AwaitingPermission)
        );
        assert_eq!(
            claude.startup_readiness(&screen("Starting Claude Code...")),
            None
        );
        assert_eq!(claude.startup_readiness(&screen("────────────────────────────────────\n❯ Try something\n────────────────────────────────────")), Some(Ready));
        assert_eq!(claude.startup_readiness(&screen("Trust this project?\n1. Yes\n2. No\n────────────────────────────────────\n❯ Ask Claude\n────────────────────────────────────")), Some(Ready));
        assert_eq!(
            claude.startup_readiness(&screen(
                "Do you trust the files in this folder?\n1. Yes\n2. No"
            )),
            Some(AwaitingTrust)
        );
    }

    fn claude_screen(name: &str) -> Vec<String> {
        let path = format!("{}/tests/screens/{name}.txt", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn a_claude_dialog_open_at_startup_awaits_the_user_s_input() {
        use adroit_model::StartupReadiness::*;
        let claude = adapter(Agent::Claude);
        assert_eq!(
            claude.startup_readiness(&claude_screen("dev-channels")),
            Some(AwaitingInput)
        );
        assert_eq!(
            claude.startup_readiness(&claude_screen("model")),
            Some(AwaitingInput)
        );
    }

    #[test]
    fn a_named_claude_session_is_ready_while_it_works_and_awaits_a_picker_opened_above() {
        use adroit_model::StartupReadiness::*;
        let claude = adapter(Agent::Claude);
        assert_eq!(
            claude.startup_readiness(&claude_screen("working-named")),
            Some(Ready)
        );
        assert_eq!(
            claude.startup_readiness(&claude_screen("model-while-working-named")),
            Some(AwaitingInput)
        );
    }

    #[test]
    fn claude_is_ready_once_the_development_channels_warning_is_confirmed() {
        let claude = adapter(Agent::Claude);
        assert_eq!(
            claude.startup_readiness(&claude_screen("dev-channels-confirmed")),
            Some(adroit_model::StartupReadiness::Ready)
        );
    }
}
