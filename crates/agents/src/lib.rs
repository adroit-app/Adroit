//! The agents adroit runs in its consoles, each behind one adapter: how it is
//! started, asked things outside a console, and read back.

pub mod claude;
mod codex;
pub mod connection;
mod delivery;
#[cfg(test)]
mod testing;
pub use connection::conflict;
pub use delivery::{DeliveryFailure, DeliveryFuture};

use adroit_model::Agent;
use adroit_model::config::{CodexOptions, Launch, SessionOptions};
use adroit_model::profile::{AgentOptions, AgentProfile, ConnectionId, ProfileId};
use std::path::{Path, PathBuf};

/// A command that starts an agent: the program, its arguments and the
/// variables set for it. What asks an agent something takes a way to make
/// one from the arguments it needs, so it starts as the user has sessions
/// start.
pub type Started = (String, Vec<String>, Vec<(String, String)>);

/// Each image's path in double quotes, as a terminal pastes a file dragged
/// onto it, which an agent's input takes as that image attached.
pub(crate) fn quoted_pastes(images: &[PathBuf]) -> Vec<String> {
    images
        .iter()
        .map(|image| format!("\"{}\"", image.display()))
        .collect()
}

/// `program` as a shell reads it back as one word, for what tells an agent
/// the command to run.
pub fn shell_word(program: impl AsRef<Path>) -> String {
    let word = program.as_ref().to_string_lossy();
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c))
    {
        word.into_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// What an adapter is handed to start its agent, or ask it something, rather
/// than reading the context itself: one profile, resolved against the
/// connection it runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContext {
    pub profile: ProfileId,
    pub connection: ConnectionId,
    /// The connection's state directory, exactly as the context write it;
    /// none in the agent's own environment.
    pub home: Option<PathBuf>,
    /// The shell a program started through the shell runs in.
    pub shell: String,
    /// The adroit program a connection of its own starts the agent's program
    /// through, as the last step before it: the one running, unless
    /// told otherwise.
    pub adroit: PathBuf,
    pub launch: Launch,
    pub options: AgentOptions,
}

impl AgentContext {
    /// `profile` run in the connection whose state directory is `home`, none
    /// for the agent's own environment, through `shell`.
    pub fn new(profile: &AgentProfile, home: Option<PathBuf>, shell: impl Into<String>) -> Self {
        Self {
            profile: profile.id.clone(),
            connection: profile.connection.clone(),
            home,
            shell: shell.into(),
            adroit: std::env::current_exe().unwrap_or_else(|_| "adroit".into()),
            launch: profile.launch.clone(),
            options: profile.options.clone(),
        }
    }

    /// The profile adroit comes with for `agent`, in its own environment.
    pub fn builtin(agent: Agent) -> Self {
        Self::new(&AgentProfile::builtin(agent), None, "")
    }

    pub fn agent(&self) -> Agent {
        self.options.agent()
    }

    /// Claude Code's options; nothing chosen for a profile of another agent.
    pub fn claude(&self) -> SessionOptions {
        match &self.options {
            AgentOptions::Claude(options) => options.clone(),
            AgentOptions::Codex(_) => SessionOptions::default(),
        }
    }

    pub fn claude_mut(&mut self) -> Option<&mut SessionOptions> {
        match &mut self.options {
            AgentOptions::Claude(options) => Some(options),
            AgentOptions::Codex(_) => None,
        }
    }

    /// Codex's options; nothing chosen for a profile of another agent.
    pub fn codex(&self) -> CodexOptions {
        match &self.options {
            AgentOptions::Codex(options) => options.clone(),
            AgentOptions::Claude(_) => CodexOptions::default(),
        }
    }

    pub fn codex_mut(&mut self) -> Option<&mut CodexOptions> {
        match &mut self.options {
            AgentOptions::Codex(options) => Some(options),
            AgentOptions::Claude(_) => None,
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
    Box<dyn std::future::Future<Output = Result<Vec<adroit_model::Running>, String>> + Send>,
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
        _context: AgentContext,
        _cwd: PathBuf,
        _title: String,
    ) -> ConversationLookup {
        Box::pin(async { None })
    }

    fn developer_instructions(
        &self,
        _context: AgentContext,
        _cwd: PathBuf,
    ) -> DeveloperInstructions {
        Box::pin(async { Ok(None) })
    }

    /// Arguments that add Adroit's `instructions` to the agent's own.
    fn instruction_args(&self, instructions: &str) -> Vec<String>;

    fn deliver(
        &self,
        _context: AgentContext,
        _cwd: PathBuf,
        _conversation: String,
        _envelope: String,
    ) -> DeliveryFuture {
        Box::pin(async { Err(DeliveryFailure::Unsupported) })
    }

    /// The prompt `text` with `images` named in it as the agent reads them:
    /// the first prompt it is started with, and a prompt typed into it when
    /// it takes no [`AgentAdapter::image_pastes`].
    fn image_prompt(&self, text: &str, images: &[PathBuf]) -> String;
    /// What is pasted into a running session, one paste for each image,
    /// for the agent to attach it; none when it is named in the prompt
    /// instead.
    fn image_pastes(&self, _images: &[PathBuf]) -> Vec<String> {
        Vec::new()
    }
    fn image_args(&self, _images: &[PathBuf]) -> Vec<String> {
        Vec::new()
    }
    fn program(&self) -> String;
    /// The variable naming the directory the agent's program keeps its
    /// state in, which a connection of its own sets.
    fn state_variable(&self) -> &'static str;
    /// Whether a connection of its own removes `name`, a variable known to
    /// carry a login or to move the agent's state, or one of their families,
    /// even when a shell's startup files set it again.
    fn denies(&self, name: &str) -> bool;
    /// What a connection of its own starts the program with, ahead of
    /// adroit's arguments.
    fn connection_args(&self) -> Vec<String> {
        Vec::new()
    }
    /// What starts the agent with `args` as `context` says: the program,
    /// its arguments, and `env` with the agent's own variables and then the
    /// launch's, in the state directory of `context`'s connection.
    fn launch(
        &self,
        context: &AgentContext,
        args: Vec<String>,
        env: Vec<(String, String)>,
    ) -> crate::Started {
        let env = env.into_iter().chain(self.environment()).collect();
        connection::started(self, context, args, env)
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
        _context: AgentContext,
        _dir: PathBuf,
    ) -> Probe<crate::claude::commands::Command> {
        Box::pin(async { None })
    }
    /// The models the provider accepts, asked of it in `dir` as `context` starts it.
    fn models(&self, _context: AgentContext, _dir: PathBuf) -> Probe<adroit_model::ProviderModel> {
        Box::pin(async { None })
    }
    fn usage(&self, _context: AgentContext, _home: PathBuf) -> Probe<crate::claude::usage::Limit> {
        Box::pin(async { None })
    }
    /// How full the context of `session`'s conversation, run on `model`, is,
    /// for an agent whose transcript does not say.
    fn context(
        &self,
        _context: AgentContext,
        _session: &adroit_model::Session,
        _model: Option<String>,
    ) -> ContextProbe {
        Box::pin(async { None })
    }
    fn transcript(
        &self,
        _context: &AgentContext,
        _home: &Path,
        _session: &adroit_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        None
    }
    fn list_sessions(&self, _context: AgentContext) -> Listing {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn follow(
        &self,
        _own: &mut [adroit_model::Session],
        _listed: Vec<adroit_model::Running>,
    ) -> Vec<(u128, Learned)> {
        Vec::new()
    }
    /// The arguments that continue the conversation `conversation` in a new
    /// session, run as `context` says; none for an agent that cannot.
    fn resume_args(&self, _context: &AgentContext, _conversation: &str) -> Option<Vec<String>> {
        None
    }
    /// The conversations the agent kept that ran in `dir`, or in any
    /// directory when none is given, `home` being the user's home.
    fn past(
        &self,
        _context: &AgentContext,
        _home: &Path,
        _dir: Option<&Path>,
    ) -> Vec<adroit_model::Past> {
        Vec::new()
    }
    fn conversation(&self) -> String {
        String::new()
    }
    fn conversation_title(&self, _title: &str) -> Option<String> {
        None
    }
    fn start_args(
        &self,
        context: &AgentContext,
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
    /// The prompts sent in `project` that the agent keeps to recall, as
    /// `context`'s connection keeps them, `home` being the user's home.
    fn history(&self, _context: &AgentContext, _home: &Path, _project: &Path) -> Vec<String> {
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

/// What starts the agent of `context` with `args`, as `context` says.
pub fn launch_in(
    context: &AgentContext,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> crate::Started {
    adapter(context.agent()).launch(context, args, env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{recorded, screen};
    use adroit_model::Agent;
    #[test]
    fn images_are_prepared_for_the_receiving_agent_only_when_sent() {
        let images = vec![
            PathBuf::from("/tmp/my shot.png"),
            PathBuf::from("/tmp/two.png"),
        ];
        let claude = adapter(Agent::Claude);
        // Started with a first prompt, Claude Code reads the images it names.
        assert_eq!(
            claude.image_prompt("look", &images),
            "look @\"/tmp/my shot.png\" @\"/tmp/two.png\""
        );
        assert_eq!(
            claude.image_pastes(&images),
            ["\"/tmp/my shot.png\"", "\"/tmp/two.png\""]
        );
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
        let context = AgentContext {
            launch: Launch {
                command: vec!["/opt/codex".into()],
                env: [("CODEX_HOME".to_string(), "/c".to_string())].into(),
                ..Launch::default()
            },
            ..AgentContext::builtin(Agent::Codex)
        };
        let (program, args, env) = launch_in(&context, vec!["exec".into()], Vec::new());
        assert_eq!(
            (program.as_str(), args),
            ("/opt/codex", vec!["exec".to_string()])
        );
        assert_eq!(env, [("CODEX_HOME".to_string(), "/c".to_string())]);
        let (program, _, env) = adapter(Agent::Claude).launch(
            &AgentContext {
                shell: "/bin/zsh".into(),
                ..AgentContext::builtin(Agent::Claude)
            },
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
        let mut context = AgentContext::builtin(Agent::Codex);
        context.codex_mut().unwrap().model = "codex-model".into();
        let a = adapter(Agent::Codex);
        assert_eq!(
            a.start_args(&context, "fix", "", "--help"),
            ["--no-alt-screen", "--model", "codex-model", "--", "--help"]
        );
        assert_eq!(
            a.start_args(&AgentContext::builtin(Agent::Codex), "fix", "", "read"),
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
        assert!(
            a.history(
                &AgentContext::builtin(Agent::Codex),
                Path::new("/unused"),
                Path::new("/unused")
            )
            .is_empty()
        );
        assert!(
            a.list_sessions(AgentContext::builtin(Agent::Codex))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            a.transcript(
                &AgentContext::builtin(Agent::Codex),
                Path::new("/unused"),
                &Default::default()
            )
            .is_none()
        );
        assert!(a.rename_command("fix").is_none());
    }

    #[test]
    fn claude_keeps_its_conversation_options_and_integrations() {
        let a = adapter(Agent::Claude);
        assert_eq!(
            a.start_args(&AgentContext::builtin(Agent::Claude), "fix", "uuid", "read"),
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

    #[test]
    fn a_claude_dialog_open_at_startup_awaits_the_user_s_input() {
        use adroit_model::StartupReadiness::*;
        let claude = adapter(Agent::Claude);
        assert_eq!(
            claude.startup_readiness(&recorded("dev-channels")),
            Some(AwaitingInput)
        );
        assert_eq!(
            claude.startup_readiness(&recorded("model")),
            Some(AwaitingInput)
        );
    }

    #[test]
    fn a_named_claude_session_is_ready_while_it_works_and_awaits_a_picker_opened_above() {
        use adroit_model::StartupReadiness::*;
        let claude = adapter(Agent::Claude);
        assert_eq!(
            claude.startup_readiness(&recorded("working-named")),
            Some(Ready)
        );
        assert_eq!(
            claude.startup_readiness(&recorded("model-while-working-named")),
            Some(AwaitingInput)
        );
    }

    #[test]
    fn claude_is_ready_once_the_development_channels_warning_is_confirmed() {
        let claude = adapter(Agent::Claude);
        assert_eq!(
            claude.startup_readiness(&recorded("dev-channels-confirmed")),
            Some(adroit_model::StartupReadiness::Ready)
        );
    }
}
