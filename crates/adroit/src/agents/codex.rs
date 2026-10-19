use super::*;

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
            command_probe: false,
            usage_limits: false,
            transcript: false,
            shell_bang: false,
        }
    }
    fn start_args(
        &self,
        settings: &Settings,
        _name: &str,
        _conversation: &str,
        reader: bool,
        prompt: &str,
    ) -> Vec<String> {
        let mut args = Vec::new();
        let model = settings.codex_sessions.model.trim();
        if !model.is_empty() {
            args.extend(["--model".into(), model.into()]);
        }
        if reader {
            args.extend(["--sandbox".into(), "read-only".into()]);
        }
        args.extend(["--".into(), prompt.into()]);
        args
    }
}
