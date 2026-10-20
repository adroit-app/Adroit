//! Provider-specific behavior behind the terminal session interface.

mod claude;
mod codex;

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
}

type Listing = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<adroit_model::Session>, String>> + Send>,
>;
type Learned = Vec<(u128, &'static str, String)>;

type Probe<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Option<Vec<T>>> + Send>>;

/// A provider reader produces the details the overview can display.
pub trait ConversationReader: Send {
    fn read(&mut self) -> adroit_claude::links::Links;
}

/// A terminal agent owns its arguments, environment and optional integrations.
pub trait AgentAdapter: Sync {
    fn label(&self) -> &'static str;
    fn program(&self) -> String;
    fn launch<'a>(&self, settings: &'a Settings) -> &'a Launch;
    fn capabilities(&self) -> Capabilities;
    fn commands(&self, _dir: PathBuf) -> Probe<adroit_claude::commands::Command> {
        Box::pin(async { None })
    }
    fn usage(&self, _home: PathBuf) -> Probe<adroit_claude::usage::Limit> {
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
    fn start_args(
        &self,
        settings: &Settings,
        name: &str,
        conversation: &str,
        reader: bool,
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
    chosen: &HashMap<crate::tabs::Tab, Agent>,
    fallback: Agent,
) -> Agent {
    match tab {
        Some(crate::tabs::Tab::Session(id)) => {
            crate::nav::find(tree, id).map_or(fallback, |(_, s)| s.agent)
        }
        Some(draft @ crate::tabs::Tab::Draft { .. }) => {
            chosen.get(draft).copied().unwrap_or(fallback)
        }
        _ => fallback,
    }
}

/// The same launch path is used by the window and the session command.
pub fn start(
    agent: Agent,
    name: &str,
    reader: bool,
    prompt: &str,
    env: Vec<(String, String)>,
) -> (String, adroit_claude::Started) {
    let settings = crate::settings::current();
    let a = adapter(agent);
    let conversation = a.conversation();
    let own = std::env::current_exe().unwrap_or_else(|_| "adroit".into());
    let mut args = a.integration_args(&own);
    args.extend(a.start_args(&settings, name, &conversation, reader, prompt));
    (conversation, launch(agent, args, env))
}

/// Apply the provider's launch command and environment to its arguments.
pub fn launch(
    agent: Agent,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> adroit_claude::Started {
    let settings = crate::settings::current();
    let a = adapter(agent);
    let launch = a.launch(&settings);
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
    fn codex_receives_only_its_own_options_and_preserves_flag_like_prompts() {
        let mut settings = Settings::default();
        settings.sessions.model = "claude-model".into();
        settings.codex_sessions.model = "codex-model".into();
        let a = adapter(Agent::Codex);
        assert_eq!(
            a.start_args(&settings, "fix", "", false, "--help"),
            ["--model", "codex-model", "--", "--help"]
        );
        assert_eq!(
            a.start_args(&Settings::default(), "fix", "", true, "read"),
            ["--sandbox", "read-only", "--", "read"]
        );
        assert!(a.conversation().is_empty());
        assert!(!a.capabilities().transcript);
        assert!(!a.capabilities().command_probe);
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
                    shells: vec![],
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
        use crate::tabs::{Role, Tab};
        let draft = |dir: &str| Tab::Draft {
            worktree: dir.into(),
            role: Role::Worker,
        };
        let chosen = HashMap::from([(draft("/api"), Agent::Codex)]);
        let tree = adroit_model::Tree::default();
        assert_eq!(
            for_tab(&tree, Some(&draft("/api")), &chosen, Agent::Claude),
            Agent::Codex
        );
        assert_eq!(
            for_tab(&tree, Some(&draft("/web")), &chosen, Agent::Claude),
            Agent::Claude
        );
    }

    #[tokio::test]
    async fn unsupported_integrations_never_start_claude_for_codex() {
        let a = adapter(Agent::Codex);
        assert!(a.integration_args(Path::new("/unused/adroit")).is_empty());
        assert!(a.environment().is_empty());
        assert!(a.history(Path::new("/unused")).is_empty());
        assert!(a.list_sessions().await.unwrap().is_empty());
        assert!(a.commands("/unused".into()).await.is_none());
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
            a.start_args(&Settings::default(), "fix", "uuid", false, "read"),
            ["--name", "fix", "--session-id", "uuid", "--", "read"]
        );
        assert!(!a.conversation().is_empty());
        assert!(a.capabilities().transcript);
        assert!(a.capabilities().command_probe);
        assert!(a.capabilities().shell_bang);
    }
}
