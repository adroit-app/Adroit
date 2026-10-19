use super::*;

pub struct Claude;

impl AgentAdapter for Claude {
    fn label(&self) -> &'static str {
        "Claude Code"
    }
    fn program(&self) -> String {
        adroit_claude::claude_program()
    }
    fn launch<'a>(&self, settings: &'a Settings) -> &'a Launch {
        &settings.claude
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            command_probe: true,
            usage_limits: true,
            transcript: true,
            shell_bang: true,
        }
    }
    fn commands(&self, dir: PathBuf) -> Probe<adroit_claude::commands::Command> {
        Box::pin(async move {
            adroit_claude::commands::probe(
                crate::programs::claude_asked,
                &dir,
                adroit_claude::commands::PROBE_TIMEOUT,
            )
            .await
        })
    }
    fn usage(&self, home: PathBuf) -> Probe<adroit_claude::usage::Limit> {
        Box::pin(async move {
            adroit_claude::usage::probe(
                crate::programs::claude_asked,
                &home,
                adroit_claude::usage::PROBE_TIMEOUT,
            )
            .await
        })
    }
    fn transcript(
        &self,
        home: &Path,
        session: &adroit_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        let path = adroit_claude::places::transcript(home, &session.cwd, &session.session_id);
        Some(Box::new(ClaudeConversation(
            adroit_claude::links::Transcript::new(path, &session.cwd, home.to_path_buf()),
        )))
    }
    fn list_sessions(&self) -> Listing {
        Box::pin(async {
            adroit_claude::list_sessions(crate::programs::claude_asked)
                .await
                .map_err(|e| e.to_string())
        })
    }
    fn follow(
        &self,
        own: &mut [adroit_model::Session],
        listed: Vec<adroit_model::Session>,
    ) -> (Vec<adroit_model::Session>, Learned) {
        crate::sessions::follow(own, listed)
    }
    fn resume_args(&self, settings: &Settings, conversation: &str) -> Option<Vec<String>> {
        Some(adroit_claude::resume_args(
            conversation,
            &settings.sessions.options(),
        ))
    }
    fn conversation(&self) -> String {
        adroit_claude::new_session_id()
    }
    fn start_args(
        &self,
        settings: &Settings,
        name: &str,
        conversation: &str,
        reader: bool,
        prompt: &str,
    ) -> Vec<String> {
        let options = if reader {
            settings.readers.options()
        } else {
            settings.sessions.options()
        };
        adroit_claude::start_args(name, conversation, &options, prompt)
    }
    fn integration_args(&self, program: &Path) -> Vec<String> {
        crate::plugin::args(program)
    }
    fn environment(&self) -> Vec<(String, String)> {
        adroit_claude::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    fn history(&self, project: &Path) -> Vec<String> {
        adroit_claude::history::read(project)
    }
    fn dialog_open(&self, screen: &[String]) -> bool {
        adroit_claude::dialog::dialog_open(screen)
    }
    fn rename_command(&self, name: &str) -> Option<String> {
        Some(format!("/rename {name}"))
    }
}

struct ClaudeConversation(adroit_claude::links::Transcript);

impl ConversationReader for ClaudeConversation {
    fn read(&mut self) -> adroit_claude::links::Links {
        self.0.read(&|p: &Path| p.is_file());
        self.0.links()
    }
}
