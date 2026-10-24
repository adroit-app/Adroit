//! OpenCode, the `opencode` coding agent of its second line: started in its
//! own terminal interface with a server of that interface's own, tied to
//! adroit by a plugin adroit writes, and asked through its own commands what
//! it keeps.

use serde_json::{Value, json};

use super::*;

mod cli;
mod plugin;
mod screen;
mod sessions;
mod transcript;

pub struct OpenCode;

/// The oldest OpenCode adroit runs, as its major and minor version: the line
/// whose interface takes a conversation's id before it is made, tells its
/// logins as JSON and runs a server of its own for one interface.
pub const OLDEST: (u64, u64) = (2, 1);

/// What every command adroit runs OpenCode with carries. Without it OpenCode
/// starts a background server on a port fixed for the machine, which
/// outlives the command and keeps the environment of the first that started
/// it: another account's login, or another session's credential.
const STANDALONE: &str = "--standalone";

/// What hands a fragment of OpenCode's configuration from where adroit's
/// arguments are made to [`AgentAdapter::launch`], which puts every fragment
/// into [`CONFIG`], the one variable OpenCode takes configuration for one run
/// from.
const FRAGMENT: &str = "--adroit-config";

/// The variable OpenCode reads configuration from over its files.
const CONFIG: &str = "OPENCODE_CONFIG_CONTENT";

/// The variable the plugin learns the conversation the interface opens from.
const SESSION: &str = "ADROIT_OPENCODE_SESSION";

/// The tools of adroit's MCP server, as OpenCode's permission rules name
/// them.
pub const TOOLS: &str = "adroit_*";

/// The arguments that add `fragment` to OpenCode's configuration for the run
/// they are handed to.
pub fn config_args(fragment: Value) -> Vec<String> {
    vec![FRAGMENT.into(), fragment.to_string()]
}

/// The rules a read-only session runs under, after any other: it may change
/// no file and run no command. OpenCode has no sandbox, so this is all that
/// keeps it to reading.
pub fn read_only_rules() -> Vec<adroit_model::config::OpenCodeRule> {
    ["edit", "shell"]
        .map(|action| adroit_model::config::OpenCodeRule {
            action: action.into(),
            resource: "*".into(),
            effect: "deny".into(),
        })
        .to_vec()
}

/// Whether an account of its own lets `name` reach OpenCode from the
/// environment it is started in: what locates the user, the terminal and
/// the network, and adroit's own. OpenCode connects every provider whose
/// credential variable models.dev lists, a list that changes with every
/// release of it, so the rest is removed rather than the providers' named.
fn allowed(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "SHELL",
        "TERM",
        "COLORTERM",
        "TERM_PROGRAM",
        "TERM_PROGRAM_VERSION",
        "TERMINFO",
        "TERMINFO_DIRS",
        "LANG",
        "LANGUAGE",
        "TZ",
        "TMPDIR",
        "EDITOR",
        "VISUAL",
        "PAGER",
        "MANPATH",
        "DISPLAY",
        "SSH_AUTH_SOCK",
        "NO_COLOR",
        "FORCE_COLOR",
        "CLICOLOR",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "no_proxy",
        "all_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "NODE_EXTRA_CA_CERTS",
        "__CF_USER_TEXT_ENCODING",
    ];
    const FAMILIES: &[&str] = &["LC_", "ADROIT_", "XDG_"];
    NAMES.contains(&name) || FAMILIES.iter().any(|family| name.starts_with(family))
}

/// Whether an account of its own decides on `name` alone: a variable that
/// carries a login, moves OpenCode's configuration, database or rules
/// elsewhere, or lets a client into its server.
fn denied(name: &str) -> bool {
    matches!(
        name,
        "OPENCODE_AUTH_CONTENT"
            | "OPENCODE_CONFIG"
            | "OPENCODE_CONFIG_DIR"
            | "OPENCODE_TUI_CONFIG"
            | "OPENCODE_CLI_CONFIG_CONTENT"
            | "OPENCODE_DB"
            | "OPENCODE_DISABLE_CHANNEL_DB"
            | "OPENCODE_PERMISSION"
            | "OPENCODE_PASSWORD"
            | "OPENCODE_SERVER_PASSWORD"
            | "OPENCODE_SERVER_USERNAME"
            | "OPENCODE_API_KEY"
            | "OPENCODE_TEST_HOME"
    )
}

/// The roots OpenCode keeps everything of its own under, each named by its
/// variable and the folder of an account's home it is moved to.
const ROOTS: [(&str, &str); 4] = [
    ("XDG_CONFIG_HOME", "config"),
    ("XDG_DATA_HOME", "data"),
    ("XDG_STATE_HOME", "state"),
    ("XDG_CACHE_HOME", "cache"),
];

/// The variables of [`ROOTS`].
const ROOT_NAMES: [&str; 4] = [ROOTS[0].0, ROOTS[1].0, ROOTS[2].0, ROOTS[3].0];

impl AgentAdapter for OpenCode {
    fn instruction_args(&self, instructions: &str) -> Vec<String> {
        match plugin::instructions(&plugin::instance_dir(), instructions) {
            Ok(path) => config_args(json!({ "instructions": [path] })),
            Err(_) => Vec::new(),
        }
    }

    // A path pasted into OpenCode's input attaches the file; one in the
    // first prompt is text, which the model can read by.
    fn image_prompt(&self, text: &str, images: &[PathBuf]) -> String {
        std::iter::once(text.to_string())
            .chain(crate::quoted_pastes(images))
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn image_pastes(&self, images: &[PathBuf]) -> Vec<String> {
        crate::quoted_pastes(images)
    }

    fn program(&self) -> String {
        "opencode".into()
    }

    // Where OpenCode keeps its logins; [`AgentAdapter::connection_variables`]
    // moves its other roots too.
    fn state_variable(&self) -> &'static str {
        "XDG_DATA_HOME"
    }

    fn connection_variables(&self, home: &Path) -> Vec<(String, String)> {
        let mut set: Vec<(String, String)> = ROOTS
            .iter()
            .map(|(name, dir)| (name.to_string(), home.join(dir).display().to_string()))
            .collect();
        // Claude Code's skills in the user's home would be every account's.
        set.push(("OPENCODE_DISABLE_CLAUDE_CODE".into(), "1".into()));
        set
    }

    fn denies(&self, name: &str) -> bool {
        denied(name)
    }

    fn removes(&self, name: &str) -> bool {
        denied(name) || !allowed(name)
    }

    fn kept_outside(&self) -> &'static [&'static str] {
        &ROOT_NAMES
    }

    fn login_args(&self) -> Vec<String> {
        ["auth", "login", STANDALONE].map(String::from).to_vec()
    }

    fn status_args(&self, _context: &AgentContext) -> Vec<String> {
        ["auth", "list", STANDALONE, "--format", "json"]
            .map(String::from)
            .to_vec()
    }

    // Any connection counts, a variable OpenCode found a key in as much as a
    // login it keeps.
    fn standing(&self, code: Option<i32>, out: &str) -> adroit_model::profile::LoginStanding {
        use adroit_model::profile::LoginStanding;
        match (code, serde_json::from_str::<Value>(out.trim()).ok()) {
            (Some(0), Some(Value::Array(listed))) => match listed.is_empty() {
                true => LoginStanding::LoggedOut,
                false => LoginStanding::LoggedIn,
            },
            (Some(126 | 127), None) => LoginStanding::Missing,
            _ => LoginStanding::Unknown,
        }
    }

    fn launch(
        &self,
        context: &AgentContext,
        args: Vec<String>,
        env: Vec<(String, String)>,
    ) -> crate::Started {
        let (args, mut config) = fragments(args);
        let mut context = context.clone();
        // The launch's own configuration is kept, under adroit's.
        if let Some(own) = context.launch.env.remove(CONFIG)
            && let Ok(own) = serde_json::from_str::<Value>(&own)
        {
            let ours = std::mem::replace(&mut config, own);
            merge(&mut config, ours);
        }
        let mut env: Vec<(String, String)> = env.into_iter().chain(self.environment()).collect();
        if config.as_object().is_some_and(|c| !c.is_empty()) {
            env.push((CONFIG.into(), config.to_string()));
        }
        if let Some(at) = args.iter().position(|arg| arg == "--session")
            && let Some(id) = args.get(at + 1)
        {
            env.push((SESSION.into(), id.clone()));
        }
        connection::started(self, &context, args, env)
    }

    fn refusal(&self, context: AgentContext, dir: PathBuf) -> Refusal {
        Box::pin(async move {
            let start = launch_in(&context, vec!["--version".into()], Vec::new());
            let (code, out) = cli::ask(start, &dir, cli::ASKED_FOR).await?;
            refused(code, &out, &context)
        })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            command_probe: false,
            usage_limits: false,
            transcript: true,
            shell_bang: true,
            screen_state: true,
        }
    }

    fn models_within(
        &self,
        context: AgentContext,
        dir: PathBuf,
        timeout: std::time::Duration,
    ) -> Probe<adroit_model::ProviderModel> {
        Box::pin(async move {
            let start = launch_in(
                &context,
                vec!["models".into(), STANDALONE.into()],
                Vec::new(),
            );
            let (code, out) = cli::ask(start, &dir, timeout).await?;
            (code == Some(0)).then(|| cli::models(&out))
        })
    }

    fn transcript(
        &self,
        context: &AgentContext,
        _home: &Path,
        session: &adroit_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        if session.session_id.is_empty() {
            return None;
        }
        Some(Box::new(transcript::Transcript::open(
            context.clone(),
            session.session_id.clone(),
            &session.cwd,
            &adroit_model::paths::home(),
        )))
    }

    fn resume_args(&self, context: &AgentContext, conversation: &str) -> Option<Vec<String>> {
        let mut args = options(context);
        args.extend(["--session".into(), conversation.into()]);
        Some(args)
    }

    fn past(
        &self,
        context: &AgentContext,
        home: &Path,
        dir: Option<&Path>,
    ) -> Vec<adroit_model::Past> {
        crate::kept_in(context, sessions::past(context, home, dir))
    }

    fn conversation(&self) -> String {
        sessions::new_id()
    }

    fn start_args(
        &self,
        context: &AgentContext,
        _name: &str,
        conversation: &str,
        prompt: &str,
    ) -> Vec<String> {
        let mut args = options(context);
        args.extend(["--session".into(), conversation.into()]);
        if !prompt.is_empty() {
            // Joined to its flag, a prompt that starts like a flag is still
            // the prompt.
            args.push(format!("--prompt={prompt}"));
        }
        args
    }

    fn integration_args(&self, program: &Path, state: &Path) -> Vec<String> {
        match plugin::install(program, &state.join("opencode")) {
            Ok((plugin, skills)) => config_args(json!({
                "plugins": [plugin],
                "skills": [skills],
            })),
            Err(_) => Vec::new(),
        }
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
}

/// How a session runs, as `context` says, whether it starts a conversation
/// or resumes one: with a server of its own, and the profile's model, agent
/// and rules as configuration, which OpenCode's interface takes no flags for.
fn options(context: &AgentContext) -> Vec<String> {
    let mut args = vec![STANDALONE.to_string()];
    let options = context.opencode();
    if options.auto {
        args.push("--auto".into());
    }
    let mut config = serde_json::Map::new();
    if !options.model.trim().is_empty() {
        config.insert("model".into(), json!(options.model.trim()));
    }
    if !options.agent.trim().is_empty() {
        config.insert("default_agent".into(), json!(options.agent.trim()));
    }
    if !options.permissions.is_empty() {
        config.insert("permissions".into(), json!(options.permissions));
    }
    if !config.is_empty() {
        args.extend(config_args(Value::Object(config)));
    }
    args
}

/// `args` without the configuration fragments among them, and the fragments
/// merged in the order they came.
fn fragments(args: Vec<String>) -> (Vec<String>, Value) {
    let mut config = json!({});
    let mut kept = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg != FRAGMENT {
            kept.push(arg);
            continue;
        }
        if let Some(fragment) = args.next().and_then(|f| serde_json::from_str(&f).ok()) {
            merge(&mut config, fragment);
        }
    }
    (kept, config)
}

/// `more` merged into `into` as OpenCode merges its configuration documents:
/// objects key by key, lists one after the other, anything else replaced.
fn merge(into: &mut Value, more: Value) {
    match (into, more) {
        (Value::Object(into), Value::Object(more)) => {
            for (key, value) in more {
                match into.get_mut(&key) {
                    Some(held) => merge(held, value),
                    None => {
                        into.insert(key, value);
                    }
                }
            }
        }
        (Value::Array(into), Value::Array(more)) => into.extend(more),
        (into, more) => *into = more,
    }
}

/// The version of OpenCode that answered `--version` with `out`, as its
/// major, minor and patch numbers.
fn version(out: &str) -> Option<(u64, u64, u64)> {
    out.split(|c: char| c.is_whitespace() || c == '\x1b')
        .map(|word| word.trim_start_matches(|c: char| !c.is_ascii_digit()))
        .find_map(|word| {
            let mut numbers = word
                .split(|c: char| !c.is_ascii_digit())
                .map(|n| n.parse::<u64>());
            match (numbers.next(), numbers.next(), numbers.next()) {
                (Some(Ok(major)), Some(Ok(minor)), Some(Ok(patch))) => Some((major, minor, patch)),
                _ => None,
            }
        })
}

/// Why OpenCode, which answered `--version` with the exit status `code` and
/// `out`, cannot run `context`'s sessions: a release older than [`OLDEST`],
/// whose interface adroit cannot drive. None for one it can, or one that
/// told nothing adroit can read.
fn refused(code: Option<i32>, out: &str, context: &AgentContext) -> Option<String> {
    let found = (code == Some(0)).then(|| version(out)).flatten()?;
    if (found.0, found.1) >= OLDEST {
        return None;
    }
    let (major, minor) = OLDEST;
    Some(format!(
        "profile `{}` runs OpenCode {}.{}.{}, and adroit runs OpenCode {major}.{minor} or later, the `@opencode/cli` package: install it with `npm i -g @opencode/cli` (it takes over the conversations of the release you have), or set the profile's launch command to it under Settings → Agents",
        context.profile, found.0, found.1, found.2
    ))
}

#[cfg(test)]
mod tests;
