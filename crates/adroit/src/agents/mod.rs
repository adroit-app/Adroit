//! Provider-specific behavior behind the terminal session interface.

mod claude;
mod codex;
mod delivery;
pub use delivery::{DeliveryFailure, DeliveryFuture};

pub use codex::CodexOptions;

use crate::programs::Launch;
use crate::settings::Settings;
use adroit_model::Agent;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
type Learned = Vec<(u128, &'static str, String)>;

type Probe<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Option<Vec<T>>> + Send>>;

type ContextProbe = std::pin::Pin<Box<dyn std::future::Future<Output = Option<(u64, u64)>> + Send>>;

/// A provider reader produces the details the overview can display.
pub trait ConversationReader: Send {
    fn read(&mut self) -> adroit_claude::links::Links;
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
        _settings: Settings,
        _cwd: PathBuf,
        _title: String,
    ) -> ConversationLookup {
        Box::pin(async { None })
    }

    fn developer_instructions(&self, _settings: Settings, _cwd: PathBuf) -> DeveloperInstructions {
        Box::pin(async { Ok(None) })
    }

    fn role_args(
        &self,
        settings: &mut Settings,
        role: adroit_model::SessionRole,
        instructions: &str,
    ) -> Vec<String>;

    fn deliver(
        &self,
        _settings: Settings,
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
    fn launch<'a>(&self, settings: &'a Settings) -> &'a Launch;
    fn capabilities(&self) -> Capabilities;
    fn usage_url(&self) -> Option<&'static str> {
        None
    }
    fn builtin_commands(&self) -> Vec<adroit_claude::commands::Command> {
        Vec::new()
    }
    fn commands(&self, _dir: PathBuf) -> Probe<adroit_claude::commands::Command> {
        Box::pin(async { None })
    }
    /// The models the provider accepts, asked of it in `dir` as `settings` start it.
    fn models(&self, _settings: Settings, _dir: PathBuf) -> Probe<adroit_model::ProviderModel> {
        Box::pin(async { None })
    }
    fn usage(&self, _home: PathBuf) -> Probe<adroit_claude::usage::Limit> {
        Box::pin(async { None })
    }
    /// How full the context of `session`'s conversation, run on `model`, is,
    /// for an agent whose transcript does not say.
    fn context(&self, _session: &adroit_model::Session, _model: Option<String>) -> ContextProbe {
        Box::pin(async { None })
    }
    fn transcript(
        &self,
        _home: &Path,
        _session: &adroit_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        None
    }
    fn list_sessions(&self) -> Listing {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn follow(
        &self,
        _own: &mut [adroit_model::Session],
        listed: Vec<adroit_model::Session>,
    ) -> (Vec<adroit_model::Session>, Learned) {
        (listed, Vec::new())
    }
    fn resume_args(&self, _settings: &Settings, _conversation: &str) -> Option<Vec<String>> {
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
        settings: &Settings,
        name: &str,
        conversation: &str,
        prompt: &str,
    ) -> Vec<String>;
    fn integration_args(&self, _program: &Path) -> Vec<String> {
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
    fn startup_readiness(&self, _screen: &[String]) -> Option<adroit_model::WorkerReadiness> {
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

/// The open session keeps its own provider when the default changes; a
/// session not started yet runs the one chosen for it, if any.
pub fn for_tab(
    tree: &adroit_model::Tree,
    tab: Option<&crate::tabs::Tab>,
    chosen: &HashMap<crate::tabs::Tab, crate::tabs::DraftOptions>,
    fallback: Agent,
) -> Agent {
    match tab {
        Some(crate::tabs::Tab::Session(id)) => {
            crate::nav::find(tree, id).map_or(fallback, |(_, s)| s.agent)
        }
        Some(draft @ crate::tabs::Tab::Draft { .. }) => chosen
            .get(draft)
            .and_then(|options| options.agent)
            .unwrap_or(fallback),
        _ => fallback,
    }
}

pub fn can_choose(tab: Option<&crate::tabs::Tab>) -> bool {
    matches!(tab, None | Some(crate::tabs::Tab::Draft { .. }))
}

/// Remember the choice for subsequent drafts, and for the open draft, if any.
pub fn choose(
    chosen: &mut HashMap<crate::tabs::Tab, crate::tabs::DraftOptions>,
    tab: Option<crate::tabs::Tab>,
    agent: Agent,
    last: &mut Option<Agent>,
) {
    if !can_choose(tab.as_ref()) {
        return;
    }
    if let Some(draft) = tab {
        chosen.entry(draft).or_default().agent = Some(agent);
    }
    *last = Some(agent);
}

/// Apply the provider's launch command and environment to its arguments.
pub fn launch(
    agent: Agent,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> adroit_claude::Started {
    let settings = crate::settings::current();
    launch_with_settings(&settings, agent, args, env)
}

pub fn launch_with_settings(
    settings: &Settings,
    agent: Agent,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> adroit_claude::Started {
    let a = adapter(agent);
    let launch = a.launch(settings);
    let shell = settings.shell.program(&crate::programs::login_shell());
    let (program, args) = launch.command_line(&a.program(), &args, &shell);
    let env = env
        .into_iter()
        .chain(a.environment())
        .chain(launch.env.clone())
        .collect();
    (program, args, env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
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
        let mut settings = Settings::default();
        settings.sessions.model = "claude-model".into();
        settings.codex_sessions.model = "codex-model".into();
        let a = adapter(Agent::Codex);
        assert_eq!(
            a.start_args(&settings, "fix", "", "--help"),
            ["--no-alt-screen", "--model", "codex-model", "--", "--help"]
        );
        assert_eq!(
            a.start_args(&Settings::default(), "fix", "", "read"),
            ["--no-alt-screen", "--", "read"]
        );
        assert!(a.conversation().is_empty());
        assert!(a.capabilities().transcript);
        assert!(a.capabilities().command_probe);
        assert!(!a.capabilities().shell_bang);
    }
    #[test]
    fn a_running_session_keeps_its_adapter_when_the_default_changes() {
        use crate::tabs::Tab;
        use adroit_model::{Session, Tree, WorkspaceNode, Worktree, WorktreeNode};
        let tree = Tree {
            workspaces: vec![WorkspaceNode {
                root: "/api".into(),
                name: "api".into(),
                git: false,
                scratch: false,
                worktrees: vec![WorktreeNode {
                    worktree: Worktree::plain("/api"),
                    terminals: vec![],
                    sessions: vec![Session {
                        id: "codex-session".into(),
                        agent: Agent::Codex,
                        ..Default::default()
                    }],
                }],
            }],
        };
        assert_eq!(
            for_tab(
                &tree,
                Some(&Tab::Session("codex-session".into())),
                &HashMap::new(),
                Agent::Claude
            ),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, None, &HashMap::new(), Agent::Claude),
            Agent::Claude
        );
    }

    #[test]
    fn a_draft_runs_the_agent_chosen_for_it_and_otherwise_the_default() {
        use crate::tabs::Tab;
        let draft = |dir: &str| Tab::Draft {
            worktree: dir.into(),
            id: Default::default(),
        };
        let api = draft("/api");
        let chosen = HashMap::from([(api.clone(), Agent::Codex.into())]);
        let tree = adroit_model::Tree::default();
        assert_eq!(
            for_tab(&tree, Some(&api), &chosen, Agent::Claude),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, Some(&draft("/web")), &chosen, Agent::Claude),
            Agent::Claude
        );
    }

    #[test]
    fn choosing_an_agent_in_an_empty_window_sets_the_next_session_agent() {
        use crate::tabs::Tab;

        let tree = adroit_model::Tree::default();
        let draft = Tab::Draft {
            worktree: "/project".into(),
            id: Default::default(),
        };
        let mut chosen = HashMap::from([(draft.clone(), Agent::Claude.into())]);
        let mut last = None;

        choose(&mut chosen, None, Agent::Codex, &mut last);

        assert_eq!(last, Some(Agent::Codex));
        assert_eq!(chosen.len(), 1);
        let fallback = last.unwrap_or(Agent::Claude);
        assert_eq!(for_tab(&tree, None, &chosen, fallback), Agent::Codex);
        assert_eq!(
            for_tab(&tree, Some(&draft), &chosen, fallback),
            Agent::Claude
        );
        for _ in 0..2 {
            let next = Tab::Draft {
                worktree: "/next".into(),
                id: Default::default(),
            };
            assert_eq!(for_tab(&tree, Some(&next), &chosen, fallback), Agent::Codex);
        }
    }

    #[test]
    fn agent_selection_is_available_only_before_a_session_starts() {
        use crate::tabs::Tab;

        assert!(can_choose(None));
        for _ in 0..2 {
            assert!(can_choose(Some(&Tab::Draft {
                worktree: "/project".into(),
                id: Default::default(),
            })));
        }
        for tab in [Tab::Session("running".into()), Tab::Terminal(1)] {
            assert!(!can_choose(Some(&tab)));
            let mut chosen = HashMap::new();
            let mut last = Some(Agent::Claude);
            choose(&mut chosen, Some(tab), Agent::Codex, &mut last);
            assert_eq!(last, Some(Agent::Claude));
            assert!(chosen.is_empty());
        }
    }

    #[test]
    fn choosing_an_agent_sets_the_next_draft_default_only_until_the_window_closes() {
        use crate::tabs::Tab;
        let draft = |dir: &str| Tab::Draft {
            worktree: dir.into(),
            id: Default::default(),
        };
        let api = draft("/api");
        let web = draft("/web");
        let tree = adroit_model::Tree::default();
        let mut chosen = HashMap::new();
        let settings = Settings::default();
        let mut last = None;

        choose(&mut chosen, Some(api.clone()), Agent::Codex, &mut last);
        assert_eq!(settings.agent, Agent::Claude);
        assert_eq!(last, Some(Agent::Codex));
        assert_eq!(
            for_tab(&tree, Some(&api), &chosen, last.unwrap_or(settings.agent)),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, Some(&web), &chosen, last.unwrap_or(settings.agent)),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, Some(&web), &HashMap::new(), settings.agent),
            Agent::Claude
        );

        choose(&mut chosen, Some(web.clone()), Agent::Claude, &mut last);
        assert_eq!(last, Some(Agent::Claude));
        assert_eq!(
            for_tab(&tree, Some(&api), &chosen, last.unwrap_or(settings.agent)),
            Agent::Codex
        );
        assert_eq!(
            for_tab(
                &tree,
                Some(&draft("/other")),
                &chosen,
                last.unwrap_or(settings.agent)
            ),
            Agent::Claude
        );
    }

    #[tokio::test]
    async fn unsupported_integrations_never_start_claude_for_codex() {
        let a = adapter(Agent::Codex);
        assert!(!a.integration_args(Path::new("/unused/adroit")).is_empty());
        assert!(a.environment().is_empty());
        assert!(a.history(Path::new("/unused")).is_empty());
        assert!(a.list_sessions().await.unwrap().is_empty());
        assert!(
            a.transcript(Path::new("/unused"), &Default::default())
                .is_none()
        );
        assert!(a.resume_args(&Settings::default(), "uuid").is_none());
        assert!(a.rename_command("fix").is_none());
    }

    #[test]
    fn claude_keeps_its_conversation_options_and_integrations() {
        let a = adapter(Agent::Claude);
        assert_eq!(
            a.start_args(&Settings::default(), "fix", "uuid", "read"),
            ["--name", "fix", "--session-id", "uuid", "--", "read"]
        );
        assert!(!a.conversation().is_empty());
        assert!(a.capabilities().transcript);
        assert!(a.capabilities().command_probe);
        assert!(a.capabilities().shell_bang);
    }

    #[test]
    fn claude_startup_classification_requires_a_live_prompt_or_human_decision() {
        use adroit_model::WorkerReadiness::*;
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
        let path = format!(
            "{}/../claude/tests/screens/{name}.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn a_claude_dialog_open_at_startup_awaits_the_user_s_input() {
        use adroit_model::WorkerReadiness::*;
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
    fn claude_is_ready_once_the_development_channels_warning_is_confirmed() {
        let claude = adapter(Agent::Claude);
        assert_eq!(
            claude.startup_readiness(&claude_screen("dev-channels-confirmed")),
            Some(adroit_model::WorkerReadiness::Ready)
        );
    }
}
