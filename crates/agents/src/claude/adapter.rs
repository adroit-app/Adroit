use crate::*;
use adroit_model::config::SessionOptions;

pub struct Claude;

impl AgentAdapter for Claude {
    fn startup_readiness(&self, screen: &[String]) -> Option<adroit_model::StartupReadiness> {
        use adroit_model::StartupReadiness;
        let rules = screen
            .iter()
            .enumerate()
            .filter(|(_, line)| super::dialog::rule(line))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let prompt = rules
            .windows(2)
            .filter(|pair| screen[pair[0] + 1].starts_with(['❯', '>', '!']))
            .map(|pair| pair[0] + 1)
            .next_back();
        let decision_question = screen.iter().rposition(|line| {
            let line = line.to_lowercase();
            line.contains("trust this project")
                || line.contains("trust this folder")
                || line.contains("trust this directory")
                || line.contains("trust the files in this folder")
                || line.contains("do you want to allow")
                || line.contains("allow this tool")
                || line.contains("permission to")
        });
        if !super::dialog::dialog_open(screen)
            && prompt
                .is_some_and(|prompt| decision_question.is_none_or(|question| prompt > question))
        {
            return Some(StartupReadiness::Ready);
        }
        let tail = screen
            .iter()
            .rev()
            .take(24)
            .rev()
            .map(|line| line.to_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        let decision = tail.contains("yes") && tail.contains("no");
        if decision
            && (tail.contains("trust this project")
                || tail.contains("trust this folder")
                || tail.contains("trust this directory")
                || tail.contains("trust the files in this folder"))
        {
            return Some(StartupReadiness::AwaitingTrust);
        }
        if decision
            && (tail.contains("do you want to allow")
                || tail.contains("allow this tool")
                || tail.contains("permission to"))
        {
            return Some(StartupReadiness::AwaitingPermission);
        }
        super::dialog::dialog_open(screen).then_some(StartupReadiness::AwaitingInput)
    }

    fn instruction_args(&self, instructions: &str) -> Vec<String> {
        vec!["--append-system-prompt".into(), instructions.into()]
    }

    fn image_prompt(&self, text: &str, images: &[PathBuf]) -> String {
        let mut prompt = text.to_string();
        for image in images {
            prompt.push_str(&format!(" @\"{}\"", image.display()));
        }
        prompt
    }

    // Named with `@` in a prompt typed in, an image is a file for the model
    // to read; pasted, Claude Code attaches it.
    fn image_pastes(&self, images: &[PathBuf]) -> Vec<String> {
        crate::quoted_pastes(images)
    }

    fn program(&self) -> String {
        super::claude_program()
    }
    fn state_variable(&self) -> &'static str {
        "CLAUDE_CONFIG_DIR"
    }
    fn denies(&self, name: &str) -> bool {
        super::denies(name)
    }
    fn login_args(&self) -> Vec<String> {
        vec!["auth".into(), "login".into()]
    }
    // Not `claude doctor`, which makes keychain items of its own as it looks.
    fn status_args(&self, _context: &AgentContext) -> Vec<String> {
        vec!["auth".into(), "status".into(), "--json".into()]
    }
    fn standing(&self, code: Option<i32>, out: &str) -> adroit_model::profile::LoginStanding {
        use adroit_model::profile::LoginStanding;
        let said = out
            .match_indices('{')
            .find_map(|(at, _)| serde_json::from_str::<serde_json::Value>(out[at..].trim()).ok())
            .and_then(|v| v.get("loggedIn").and_then(serde_json::Value::as_bool));
        match (code, said) {
            (Some(0), Some(true)) => LoginStanding::LoggedIn,
            (Some(0 | 1), Some(false)) => LoginStanding::LoggedOut,
            (Some(126 | 127), _) => LoginStanding::Missing,
            _ => LoginStanding::Unknown,
        }
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            command_probe: true,
            usage_limits: true,
            transcript: true,
            shell_bang: true,
            screen_state: true,
        }
    }
    fn usage_url(&self) -> Option<&'static str> {
        Some("https://claude.ai/settings/usage")
    }
    fn commands(&self, context: AgentContext, dir: PathBuf) -> Probe<super::commands::Command> {
        Box::pin(async move {
            super::commands::probe(
                |args| asked(&context, args),
                &dir,
                super::commands::PROBE_TIMEOUT,
            )
            .await
        })
    }
    fn models_within(
        &self,
        context: AgentContext,
        dir: PathBuf,
        timeout: std::time::Duration,
    ) -> Probe<adroit_model::ProviderModel> {
        Box::pin(async move {
            super::models::probe(|args| launch_in(&context, args, Vec::new()), &dir, timeout).await
        })
    }
    fn usage(&self, context: AgentContext, home: PathBuf) -> Probe<super::usage::Limit> {
        Box::pin(async move {
            super::usage::probe(
                |args| asked(&context, args),
                &home,
                super::usage::PROBE_TIMEOUT,
            )
            .await
        })
    }
    fn context(
        &self,
        context: AgentContext,
        session: &adroit_model::Session,
        model: Option<String>,
    ) -> ContextProbe {
        let (dir, id) = (session.cwd.clone(), session.session_id.clone());
        Box::pin(async move {
            if id.is_empty() {
                return None;
            }
            super::context::probe(
                |args| asked(&context, args),
                &dir,
                &id,
                model.as_deref(),
                super::context::PROBE_TIMEOUT,
            )
            .await
        })
    }
    fn transcript(
        &self,
        context: &AgentContext,
        home: &Path,
        session: &adroit_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        let path = super::places::transcript(context, home, &session.cwd, &session.session_id);
        Some(Box::new(ClaudeConversation(super::links::Transcript::new(
            path,
            &session.cwd,
            home.to_path_buf(),
        ))))
    }
    fn list_sessions(&self, context: AgentContext) -> Listing {
        Box::pin(async move {
            super::list_sessions(|args| asked(&context, args))
                .await
                .map_err(|e| e.to_string())
        })
    }
    fn follow(
        &self,
        own: &mut [adroit_model::Session],
        listed: Vec<adroit_model::Running>,
    ) -> Vec<(u128, Learned)> {
        crate::claude::follow(Agent::Claude, own, listed)
    }
    fn resume_args(&self, context: &AgentContext, conversation: &str) -> Option<Vec<String>> {
        Some(super::resume_args(
            conversation,
            &options(&context.claude()),
        ))
    }
    fn past(
        &self,
        context: &AgentContext,
        home: &Path,
        dir: Option<&Path>,
    ) -> Vec<adroit_model::Past> {
        crate::kept_in(
            context,
            super::past::read(&super::places::projects(context, home), dir),
        )
    }
    fn conversation(&self) -> String {
        super::new_session_id()
    }
    fn start_args(
        &self,
        context: &AgentContext,
        name: &str,
        conversation: &str,
        prompt: &str,
    ) -> Vec<String> {
        let options = options(&context.claude());
        super::start_args(name, conversation, &options, prompt)
    }
    fn integration_args(&self, program: &Path, state: &Path) -> Vec<String> {
        super::plugin::args(program, &state.join("plugin"))
    }
    fn environment(&self) -> Vec<(String, String)> {
        super::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    fn history(&self, context: &AgentContext, home: &Path, project: &Path) -> Vec<String> {
        super::history::read(context, home, project)
    }
    fn dialog_open(&self, screen: &[String]) -> bool {
        super::dialog::dialog_open(screen)
    }
    fn screen_state(&self, screen: &[String]) -> Option<adroit_model::State> {
        super::dialog::screen_state(screen)
    }
    fn rename_command(&self, name: &str) -> Option<String> {
        Some(format!("/rename {name}"))
    }
}

struct ClaudeConversation(super::links::Transcript);

impl ConversationReader for ClaudeConversation {
    fn stats(&self) -> adroit_model::ConversationStats {
        self.0.stats()
    }
    fn read(&mut self) -> super::links::Links {
        self.0.read(&|p: &Path| p.is_file());
        self.0.links()
    }
}

/// Claude Code's options for a session as `sessions` says, each word trimmed
/// and empty tools dropped.
fn options(sessions: &SessionOptions) -> super::Options {
    super::Options {
        model: sessions.model.trim().to_string(),
        permission_mode: sessions.permission_mode.trim().to_string(),
        disallowed_tools: sessions
            .disallowed_tools
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        read_only: sessions.read_only.as_ref().map(super::ReadOnly::protecting),
    }
}

/// What starts Claude Code with `args` to ask it something outside a console,
/// as `context` starts a session: the program, its arguments, and the
/// variables its launch adds.
fn asked(context: &AgentContext, args: Vec<String>) -> Started {
    crate::own_login::started(&Claude, context, args, Vec::new())
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
