//! The agents adroit runs in its consoles, each behind one adapter: how it is
//! started, asked things outside a console, and read back.

pub mod claude;
mod codex;
pub mod commands;
mod delivery;
pub mod links;
pub mod login;
pub mod opencode;
pub mod own_login;
pub mod pi;
mod tail;
#[cfg(test)]
mod testing;
mod time;
pub mod usage;
pub use delivery::{DeliveryFailure, DeliveryFuture};
pub use own_login::conflict;

use adroit_model::Agent;
use adroit_model::config::{CodexOptions, Launch, OpenCodeOptions, PiOptions, SessionOptions};
use adroit_model::profile::{AgentOptions, AgentProfile, ProfileId};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A command that starts an agent: the program, its arguments and the
/// variables set for it. What asks an agent something takes a way to make
/// one from the arguments it needs, so it starts as the user has sessions
/// start.
pub type Started = (String, Vec<String>, Vec<(String, String)>);

/// What the command `started` runs in `dir`, its input closed, printed and
/// how it ended; `None` when it had not ended within `timeout` or could not
/// be waited for, and is killed.
pub(crate) async fn output(
    (program, args, env): Started,
    dir: &Path,
    timeout: Duration,
) -> std::io::Result<Option<std::process::Output>> {
    use std::process::Stdio;
    let child = tokio::process::Command::new(program)
        .args(args)
        .envs(env)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    Ok(tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .ok()
        .and_then(Result::ok))
}

/// Each image's path in double quotes, as a terminal pastes a file dragged
/// onto it, which an agent's input takes as that image attached.
pub(crate) fn quoted_pastes(images: &[PathBuf]) -> Vec<String> {
    images
        .iter()
        .map(|image| format!("\"{}\"", image.display()))
        .collect()
}

/// `pasts` as the conversations kept in `context`'s login, which a
/// resume starts from its profile.
pub(crate) fn kept_in(
    context: &AgentContext,
    pasts: Vec<adroit_model::Past>,
) -> Vec<adroit_model::Past> {
    pasts
        .into_iter()
        .map(|past| adroit_model::Past {
            home: context.home.clone(),
            profile: context.profile.clone(),
            profile_name: context.profile_name.clone(),
            ..past
        })
        .collect()
}

/// Read `agent`'s `listed` sessions against adroit's `own`: each of `own`
/// that `agent` runs learns the process that holds its conversation, and
/// follows that process to the conversation a `/clear`, `/new` or `/resume`
/// moves it to. What was learned, by each session's daemon id.
pub(crate) fn follow(
    agent: Agent,
    own: &mut [adroit_model::Session],
    listed: Vec<adroit_model::Running>,
) -> Vec<(u128, Learned)> {
    let mut learned = Vec::new();
    for s in own.iter_mut().filter(|s| s.agent == agent) {
        let Some(id) = adroit_model::daemon::id_of(&s.id) else {
            continue;
        };
        match s.pid {
            Some(pid) => {
                let moved = listed
                    .iter()
                    .find(|l| l.pid == pid)
                    .filter(|l| !l.session_id.is_empty() && l.session_id != s.session_id);
                if let Some(l) = moved {
                    s.session_id = l.session_id.clone();
                    learned.push((id, Learned::Conversation(s.session_id.clone())));
                }
            }
            None => {
                let holding = listed
                    .iter()
                    .filter(|_| !s.session_id.is_empty())
                    .find(|l| l.session_id == s.session_id)
                    .map(|l| l.pid);
                if let Some(pid) = holding {
                    s.pid = Some(pid);
                    learned.push((id, Learned::Pid(pid)));
                }
            }
        }
    }
    learned
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
/// than reading the context itself: one profile, with the login it runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContext {
    pub profile: ProfileId,
    /// What the profile is called.
    pub profile_name: String,
    /// The state directory of the profile's own login, exactly as the
    /// settings write it; none in the agent's own environment.
    pub home: Option<PathBuf>,
    /// The shell a program started through the shell runs in.
    pub shell: String,
    /// The adroit program a login of its own starts the agent's program
    /// through, as the last step before it: the one running, unless
    /// told otherwise.
    pub adroit: PathBuf,
    pub launch: Launch,
    pub options: AgentOptions,
}

impl AgentContext {
    /// `profile` run in its login, through `shell`.
    pub fn new(profile: &AgentProfile, shell: impl Into<String>) -> Self {
        Self {
            profile: profile.id.clone(),
            profile_name: profile.label().to_string(),
            home: profile.home.clone(),
            shell: shell.into(),
            adroit: std::env::current_exe().unwrap_or_else(|_| "adroit".into()),
            launch: profile.launch.clone(),
            options: profile.options.clone(),
        }
    }

    /// The profile adroit comes with for `agent`, in its own environment.
    pub fn builtin(agent: Agent) -> Self {
        Self::new(&AgentProfile::builtin(agent), "")
    }

    pub fn agent(&self) -> Agent {
        self.options.agent()
    }

    /// The path the variable `name` holds as the agent is started: as the
    /// launch sets it, else as adroit's own environment does.
    pub(crate) fn variable(&self, name: &str) -> Option<PathBuf> {
        self.launch
            .env
            .get(name)
            .map(PathBuf::from)
            .or_else(|| std::env::var_os(name).map(PathBuf::from))
    }

    /// Claude Code's options; nothing chosen for a profile of another agent.
    pub fn claude(&self) -> SessionOptions {
        self.options.claude().cloned().unwrap_or_default()
    }

    pub fn claude_mut(&mut self) -> Option<&mut SessionOptions> {
        self.options.claude_mut()
    }

    /// Codex's options; nothing chosen for a profile of another agent.
    pub fn codex(&self) -> CodexOptions {
        self.options.codex().cloned().unwrap_or_default()
    }

    pub fn codex_mut(&mut self) -> Option<&mut CodexOptions> {
        self.options.codex_mut()
    }

    /// Pi's options; nothing chosen for a profile of another agent.
    pub fn pi(&self) -> PiOptions {
        self.options.pi().cloned().unwrap_or_default()
    }

    pub fn pi_mut(&mut self) -> Option<&mut PiOptions> {
        self.options.pi_mut()
    }

    /// OpenCode's options; nothing chosen for a profile of another agent.
    pub fn opencode(&self) -> OpenCodeOptions {
        self.options.opencode().cloned().unwrap_or_default()
    }

    pub fn opencode_mut(&mut self) -> Option<&mut OpenCodeOptions> {
        self.options.opencode_mut()
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
    fn read(&mut self) -> crate::links::Links;
    fn stats(&self) -> adroit_model::ConversationStats {
        adroit_model::ConversationStats::default()
    }
}

type DeveloperInstructions =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, String>> + Send>>;

type ConversationLookup =
    std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>>;

/// Why a session cannot start, as its agent's program tells of itself.
type Refusal = std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send>>;

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
    /// state in, which a login of its own sets.
    fn state_variable(&self) -> &'static str;
    /// Whether a login of its own removes `name`, a variable known to
    /// carry a login or to move the agent's state, or one of their families,
    /// even when a shell's startup files set it again.
    fn denies(&self, name: &str) -> bool;
    /// What a login of its own in `home` sets last, which the profile's
    /// launch cannot set: the state variable naming `home`, unless the agent
    /// keeps its state in more than one place.
    fn own_login_variables(&self, home: &Path) -> Vec<(String, String)> {
        vec![(self.state_variable().into(), home.display().to_string())]
    }
    /// Whether a login of its own removes `name` from what the agent
    /// inherits, unless the profile's launch sets it: those it
    /// [denies](AgentAdapter::denies), unless the agent reads credentials
    /// from more variables than a list of them could keep up with, and only
    /// those known to be harmless are left.
    fn removes(&self, name: &str) -> bool {
        self.denies(name)
    }
    /// The variables a login of its own sets whose values from outside
    /// it are kept as well, under [`login::OUTSIDE`], for what the agent
    /// runs to be given back.
    fn kept_outside(&self) -> &'static [&'static str] {
        &[]
    }
    /// What starts the CLI's own login, which the user goes through in a
    /// terminal.
    fn login_args(&self) -> Vec<String>;
    /// What asks the CLI whether `context`'s login is logged in, without
    /// logging in or out.
    fn status_args(&self, context: &AgentContext) -> Vec<String>;
    /// Whether the CLI is logged in, as it answered [`AgentAdapter::status_args`]
    /// with the exit status `code` and `out` on its output.
    fn standing(&self, code: Option<i32>, _out: &str) -> adroit_model::profile::LoginStanding {
        login::by_status(code)
    }
    /// Why a session of `context` cannot start in `dir`, as the agent's
    /// program, asked as the session would start it, tells of itself; none
    /// when it can, or tells nothing adroit can read.
    fn refusal(&self, _context: AgentContext, _dir: PathBuf) -> Refusal {
        Box::pin(async { None })
    }
    /// What a login of its own starts the program with, ahead of
    /// adroit's arguments.
    fn own_login_args(&self) -> Vec<String> {
        Vec::new()
    }
    /// What starts the agent with `args` as `context` says: the program,
    /// its arguments, and `env` with the agent's own variables and then the
    /// launch's, in the state directory of `context`'s login.
    fn launch(
        &self,
        context: &AgentContext,
        args: Vec<String>,
        env: Vec<(String, String)>,
    ) -> crate::Started {
        let env = env.into_iter().chain(self.environment()).collect();
        own_login::started(self, context, args, env)
    }
    fn capabilities(&self) -> Capabilities;
    fn usage_url(&self) -> Option<&'static str> {
        None
    }
    fn builtin_commands(&self) -> Vec<crate::commands::Command> {
        Vec::new()
    }
    fn commands(&self, _context: AgentContext, _dir: PathBuf) -> Probe<crate::commands::Command> {
        Box::pin(async { None })
    }
    /// The models the provider accepts, asked of it in `dir` as `context` starts it.
    fn models(&self, context: AgentContext, dir: PathBuf) -> Probe<adroit_model::ProviderModel> {
        self.models_within(context, dir, crate::commands::PROBE_TIMEOUT)
    }
    /// [`AgentAdapter::models`], given up after `timeout`.
    fn models_within(
        &self,
        _context: AgentContext,
        _dir: PathBuf,
        _timeout: std::time::Duration,
    ) -> Probe<adroit_model::ProviderModel> {
        Box::pin(async { None })
    }
    fn usage(&self, _context: AgentContext, _home: PathBuf) -> Probe<crate::usage::Limit> {
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
    /// `context`'s login keeps them, `home` being the user's home.
    fn history(&self, _context: &AgentContext, _home: &Path, _project: &Path) -> Vec<String> {
        Vec::new()
    }
    fn dialog_open(&self, _screen: &[String]) -> bool {
        false
    }
    /// Whether a session just started is ready for its first prompt, as its
    /// screen shows; by default as [`readiness`] tells from its state.
    fn startup_readiness(&self, screen: &[String]) -> Option<adroit_model::StartupReadiness> {
        self.screen_state(screen).and_then(readiness)
    }
    fn screen_state(&self, _screen: &[String]) -> Option<adroit_model::State> {
        None
    }
    fn rename_command(&self, _name: &str) -> Option<String> {
        None
    }
}

/// Whether a session whose screen shows `state` has started: ready at its
/// prompt or in a turn, or held at a question; none while it shows neither.
pub(crate) fn readiness(state: adroit_model::State) -> Option<adroit_model::StartupReadiness> {
    use adroit_model::{StartupReadiness, State, Wait};
    match state {
        State::Idle | State::Working => Some(StartupReadiness::Ready),
        State::Waiting(Wait::Permission) => Some(StartupReadiness::AwaitingPermission),
        State::Waiting(Wait::Input) => Some(StartupReadiness::AwaitingInput),
        _ => None,
    }
}

pub fn adapter(agent: Agent) -> &'static dyn AgentAdapter {
    match agent {
        Agent::Claude => &claude::Claude,
        Agent::Codex => &codex::Codex,
        Agent::Pi => &pi::Pi,
        Agent::OpenCode => &opencode::OpenCode,
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
    use crate::testing::{program, recorded, screen, started};
    use adroit_model::Agent;

    #[test]
    fn a_variable_is_the_launch_s_before_adroit_s_own() {
        let mut context = AgentContext::builtin(Agent::Codex);
        assert_eq!(
            context.variable("HOME"),
            std::env::var_os("HOME").map(PathBuf::from)
        );
        assert_eq!(context.variable("ADROIT_TEST_NEVER_SET"), None);
        context
            .launch
            .env
            .insert("HOME".into(), "/launch/home".into());
        assert_eq!(context.variable("HOME"), Some("/launch/home".into()));
    }

    #[test]
    fn a_screen_s_state_tells_whether_a_session_has_started() {
        use adroit_model::{StartupReadiness, State, Wait};
        for (state, readiness_is) in [
            (State::Idle, Some(StartupReadiness::Ready)),
            (State::Working, Some(StartupReadiness::Ready)),
            (
                State::Waiting(Wait::Permission),
                Some(StartupReadiness::AwaitingPermission),
            ),
            (
                State::Waiting(Wait::Input),
                Some(StartupReadiness::AwaitingInput),
            ),
            (State::Waiting(Wait::Other("x".into())), None),
            (State::Starting, None),
            (State::Done, None),
            (State::Failed, None),
        ] {
            assert_eq!(readiness(state.clone()), readiness_is, "{state:?}");
        }
    }

    #[tokio::test]
    async fn a_command_run_to_its_end_gives_its_status_and_what_it_printed() {
        let (dir, p) = program("cli", "pwd\necho oops >&2\nexit 3");
        let out = output(started(&p, &[]), dir.path(), Duration::from_secs(5))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out.status.code(), Some(3));
        let printed = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(
            Path::new(printed.trim()).canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[tokio::test]
    async fn a_command_that_does_not_end_in_time_gives_nothing() {
        let (dir, p) = program("cli", "exec sleep 30");
        let out = output(started(&p, &[]), dir.path(), Duration::from_millis(200)).await;
        assert!(matches!(out, Ok(None)));
    }

    #[tokio::test]
    async fn a_command_that_cannot_start_says_why() {
        let missing = started(Path::new("/nonexistent/adroit-test-cli"), &[]);
        let out = output(missing, Path::new("/"), Duration::from_secs(5)).await;
        assert!(matches!(out, Err(e) if e.kind() == std::io::ErrorKind::NotFound));
    }
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
