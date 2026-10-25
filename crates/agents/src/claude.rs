//! The calls adroit makes to Claude Code: the sessions it runs, and what it
//! keeps of their conversations.

use std::path::{Path, PathBuf};

use adroit_model::{ParseError, Running};
use tokio::process::Command;

mod adapter;
pub use adapter::Claude;
pub mod commands;
pub mod context;
mod control;
pub mod dialog;
pub mod history;
pub mod links;
pub mod metrics;
pub mod models;
mod past;
pub mod places;
pub(crate) mod plugin;
pub mod subagents;
mod transcript;
pub mod usage;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot run {program}: {source}")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
    #[error("{program} failed: {stderr}")]
    Failed { program: String, stderr: String },
    #[error(transparent)]
    Sessions(#[from] ParseError),
}

/// The program used to reach Claude Code: `$ADROIT_CLAUDE`, or `claude`.
pub fn claude_program() -> String {
    std::env::var("ADROIT_CLAUDE").unwrap_or_else(|_| "claude".into())
}

use crate::Started;

/// The environment Claude Code runs in when adroit starts it: told the
/// console draws Kitty graphics, which it would not believe of a terminal
/// whose XTVERSION reply names felis rather than kitty or Ghostty; claiming
/// another name would mislead every other program asking.
pub const ENV: &[(&str, &str)] = &[("CLAUDE_CODE_FORCE_TERMINAL_IMAGES", "1")];

/// What Claude Code reads a login from ahead of the one in its state
/// directory, or what moves where that directory's state is found. The
/// cloud providers' own credentials, such as `AWS_*` and
/// `GOOGLE_APPLICATION_CREDENTIALS`, are left for the tools a session runs:
/// without a `CLAUDE_CODE_USE_*` switch Claude Code does not read them.
const DENIED: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_CUSTOM_HEADERS",
    "ANTHROPIC_FEDERATION_RULE_ID",
    "ANTHROPIC_IDENTITY_TOKEN_FILE",
    "ANTHROPIC_ORGANIZATION_ID",
    "ANTHROPIC_PROFILE",
    "ANTHROPIC_WORKSPACE_ID",
    "ANTHROPIC_AWS_API_KEY",
    "ANTHROPIC_AWS_BASE_URL",
    "ANTHROPIC_AWS_WORKSPACE_ID",
    "ANTHROPIC_BEDROCK_BASE_URL",
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "ANTHROPIC_VERTEX_BASE_URL",
    "ANTHROPIC_FOUNDRY_API_KEY",
    "ANTHROPIC_FOUNDRY_BASE_URL",
    "ANTHROPIC_FOUNDRY_RESOURCE",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_REFRESH_TOKEN",
    "CLAUDE_CODE_OAUTH_SCOPES",
    "CLAUDE_CODE_CUSTOM_OAUTH_URL",
    "CLAUDE_CODE_CLIENT_CERT",
    "CLAUDE_CODE_CLIENT_KEY",
    "CLAUDE_CODE_CLIENT_KEY_PASSPHRASE",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    "CLAUDE_CODE_USE_MANTLE",
    "CLAUDE_CODE_USE_GATEWAY",
    "CLAUDE_CODE_SKIP_BEDROCK_AUTH",
    "CLAUDE_CODE_SKIP_VERTEX_AUTH",
    "CLAUDE_CODE_SKIP_FOUNDRY_AUTH",
    "CLAUDE_CODE_SKIP_MANTLE_AUTH",
    "CLAUDE_CODE_SKIP_ANTHROPIC_AWS_AUTH",
    "CLAUDE_CODE_PROJECT_DIR_NAME",
    "CLAUDE_SECURESTORAGE_CONFIG_DIR",
    "CLAUDE_CONFIG_DIR",
    "CLOUD_ML_REGION",
];

/// Whether `name` is one of [`DENIED`], or of the families they come from.
pub(crate) fn denies(name: &str) -> bool {
    DENIED.contains(&name)
        || name.starts_with("CLAUDE_CODE_USE_")
        || [
            "ANTHROPIC_AWS_",
            "ANTHROPIC_BEDROCK_",
            "ANTHROPIC_VERTEX_",
            "ANTHROPIC_FOUNDRY_",
        ]
        .iter()
        .any(|family| name.starts_with(family))
        || (name.starts_with("CLAUDE_CODE_SKIP_") && name.ends_with("_AUTH"))
}

/// What `cmd`, running `program`, prints, or why it failed.
async fn output(program: &str, mut cmd: Command) -> Result<String, Error> {
    let out = cmd.output().await.map_err(|source| Error::Spawn {
        program: program.into(),
        source,
    })?;
    if !out.status.success() {
        return Err(Error::Failed {
            program: program.into(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The conversations running in a terminal of their own, adroit's sessions
/// among them, which Claude Code, started by `start`, lists.
pub async fn list_sessions(
    start: impl FnOnce(Vec<String>) -> Started,
) -> Result<Vec<Running>, Error> {
    let (program, args, env) = start(vec!["agents".into(), "--json".into()]);
    let mut cmd = Command::new(&program);
    cmd.args(args).envs(env);
    let out = output(&program, cmd).await?;
    Ok(parse_listing(&out)?)
}

/// The sessions in `out`, read from the first line a listing can be read
/// from: a shell that starts Claude Code may print from its startup files
/// first, lines starting with `[` among them.
fn parse_listing(out: &str) -> Result<Vec<Running>, ParseError> {
    let starts = std::iter::once(0).chain(out.match_indices("\n[").map(|(at, _)| at + 1));
    let mut failed = None;
    for at in starts {
        match adroit_model::parse_agents(&out[at..]) {
            Ok(sessions) => return Ok(sessions),
            Err(e) => {
                failed.get_or_insert(e);
            }
        }
    }
    Err(failed.expect("the whole output is tried"))
}

/// How a session runs: the model, the permission mode, the tools denied it
/// and whether it may write; what is left empty is Claude Code's own choice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub model: String,
    pub permission_mode: String,
    pub disallowed_tools: Vec<String>,
    pub read_only: Option<ReadOnly>,
}

impl Options {
    /// The arguments that say so to Claude Code.
    pub fn args(&self) -> Vec<String> {
        let mut disallowed = self.disallowed_tools.clone();
        if self.read_only.is_some() {
            for tool in EDITING_TOOLS {
                if !disallowed.iter().any(|held| held == tool) {
                    disallowed.push(tool.to_string());
                }
            }
        }
        let mut out = Vec::new();
        for (flag, value) in [
            ("--model", self.model.clone()),
            ("--permission-mode", self.permission_mode.clone()),
            ("--disallowedTools", disallowed.join(",")),
            (
                "--settings",
                self.read_only
                    .as_ref()
                    .map(ReadOnly::settings)
                    .unwrap_or_default(),
            ),
        ] {
            if !value.is_empty() {
                out.extend([flag.to_string(), value]);
            }
        }
        out
    }
}

const EDITING_TOOLS: [&str; 3] = ["Edit", "Write", "NotebookEdit"];

/// A session kept from writing: its editing tools are taken away and its
/// shell runs in Claude Code's sandbox, denied writes to the protected
/// directories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadOnly {
    protected: Vec<PathBuf>,
}

impl ReadOnly {
    /// A session working in `dir`, kept from writing to it and to the git
    /// history behind it.
    pub fn for_dir(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let mut protected = vec![dir.to_path_buf()];
        // Claude Code's sandbox opens a linked worktree's shared git
        // directory to writes so that commits work; a read-only session must
        // not change history either.
        protected.extend(adroit_vcs::linked_common_dir(dir));
        Self { protected }
    }

    /// A session kept from writing to exactly `paths`.
    pub fn protecting(paths: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        Self {
            protected: paths.into_iter().map(Into::into).collect(),
        }
    }

    pub fn protected(&self) -> &[PathBuf] {
        &self.protected
    }

    /// The settings that enforce it, as JSON for `--settings`. They rank
    /// above the user's and the project's settings files, and permission
    /// denials and sandbox write denials only add up across them, so neither
    /// can loosen it; the sandbox refuses to start rather than run unconfined.
    fn settings(&self) -> String {
        let denied: Vec<String> = self
            .protected
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        serde_json::json!({
            "permissions": {"deny": EDITING_TOOLS},
            "sandbox": {
                "enabled": true,
                "failIfUnavailable": true,
                "allowUnsandboxedCommands": false,
                "filesystem": {"denyWrite": denied}
            }
        })
        .to_string()
    }
}

/// A fresh id for a conversation adroit starts, chosen before Claude Code
/// runs so the session can carry it from the start.
pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The arguments that start Claude Code named `name`, holding the
/// conversation `session_id`, with a first prompt.
pub fn start_args(name: &str, session_id: &str, options: &Options, prompt: &str) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--name".into(),
        name.into(),
        "--session-id".into(),
        session_id.into(),
    ];
    args.extend(options.args());
    // The prompt follows `--`, so one starting with `-` is not read as a flag.
    args.extend(["--".into(), prompt.into()]);
    args
}

/// The arguments that load the plugin in `dir` for the session alone, put
/// before the others.
pub fn plugin_args(dir: impl AsRef<Path>) -> Vec<String> {
    vec![
        "--plugin-dir".into(),
        dir.as_ref().to_string_lossy().into_owned(),
    ]
}

/// The arguments that continue the conversation `session_id`, run as
/// `options` say.
pub fn resume_args(session_id: &str, options: &Options) -> Vec<String> {
    let mut args: Vec<String> = ["--resume", session_id].map(String::from).to_vec();
    args.extend(options.args());
    args
}

/// The command of a prompt that is a shell line for Claude Code: one whose
/// first character is `!`, without it. The window types the `!` as a key and
/// pastes the rest, as Claude Code switches its input to bash mode on a `!`
/// typed into it while empty, and a paste is not certain to.
pub fn shell_line(prompt: &str) -> Option<&str> {
    prompt.strip_prefix('!')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_starts_named_with_its_conversation_and_first_prompt() {
        assert_eq!(
            start_args("fix", "a08bbcf2-2cbf", &Options::default(), "-h is a flag"),
            [
                "--name",
                "fix",
                "--session-id",
                "a08bbcf2-2cbf",
                "--",
                "-h is a flag"
            ]
        );
    }

    #[test]
    fn a_session_runs_with_the_model_mode_and_tools_its_options_name() {
        let reader = Options {
            model: "opus".into(),
            permission_mode: "plan".into(),
            disallowed_tools: vec!["Edit".into(), "Write".into(), "NotebookEdit".into()],
            ..Options::default()
        };
        assert_eq!(
            start_args("look", "a08bbcf2-2cbf", &reader, "b"),
            [
                "--name",
                "look",
                "--session-id",
                "a08bbcf2-2cbf",
                "--model",
                "opus",
                "--permission-mode",
                "plan",
                "--disallowedTools",
                "Edit,Write,NotebookEdit",
                "--",
                "b"
            ]
        );
    }

    fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
        args.windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].as_str())
    }

    #[test]
    fn a_read_only_session_loses_the_editing_tools_besides_those_already_denied() {
        let options = Options {
            disallowed_tools: vec!["Agent".into(), "Edit".into()],
            read_only: Some(ReadOnly::protecting(["/work"])),
            ..Options::default()
        };
        assert_eq!(
            flag(&options.args(), "--disallowedTools"),
            Some("Agent,Edit,Write,NotebookEdit")
        );
    }

    #[test]
    fn a_read_only_session_runs_its_shell_in_a_sandbox_that_fails_closed_and_denies_its_directories()
     {
        let options = Options {
            read_only: Some(ReadOnly::protecting(["/work", "/repo/.git"])),
            ..Options::default()
        };
        let args = options.args();
        let settings: serde_json::Value =
            serde_json::from_str(flag(&args, "--settings").expect("settings are passed")).unwrap();
        assert_eq!(
            settings,
            serde_json::json!({
                "permissions": {"deny": ["Edit", "Write", "NotebookEdit"]},
                "sandbox": {
                    "enabled": true,
                    "failIfUnavailable": true,
                    "allowUnsandboxedCommands": false,
                    "filesystem": {"denyWrite": ["/work", "/repo/.git"]}
                }
            })
        );
    }

    #[test]
    fn a_session_that_is_not_read_only_gets_no_extra_settings() {
        assert_eq!(flag(&Options::default().args(), "--settings"), None);
    }

    #[test]
    fn a_read_only_directory_outside_git_protects_only_itself() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            ReadOnly::for_dir(root.path()).protected(),
            [root.path().to_path_buf()]
        );
    }

    #[test]
    fn a_read_only_directory_in_a_main_checkout_protects_only_itself() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".git")).unwrap();
        let dir = root.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(ReadOnly::for_dir(&dir).protected(), [dir]);
    }

    #[test]
    fn a_read_only_directory_in_a_linked_worktree_also_protects_the_shared_git_directory() {
        let root = tempfile::tempdir().unwrap();
        let gitdir = root.path().join("main/.git/worktrees/wt");
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
        let worktree = root.path().join("wt");
        std::fs::create_dir_all(worktree.join("sub")).unwrap();
        std::fs::write(worktree.join(".git"), "gitdir: ../main/.git/worktrees/wt\n").unwrap();
        let dir = worktree.join("sub");
        assert_eq!(
            ReadOnly::for_dir(&dir).protected(),
            [
                dir.clone(),
                root.path().join("main/.git").canonicalize().unwrap()
            ]
        );
    }

    #[test]
    fn a_plugin_is_handed_over_by_its_directory() {
        assert_eq!(
            plugin_args(Path::new("/state/plugin")),
            ["--plugin-dir", "/state/plugin"]
        );
    }

    #[test]
    fn a_resumed_session_continues_its_conversation() {
        assert_eq!(
            resume_args("a08bbcf2-2cbf", &Options::default()),
            ["--resume", "a08bbcf2-2cbf"]
        );
        let mode = Options {
            permission_mode: "acceptEdits".into(),
            ..Options::default()
        };
        assert_eq!(
            resume_args("a08bbcf2-2cbf", &mode),
            [
                "--resume",
                "a08bbcf2-2cbf",
                "--permission-mode",
                "acceptEdits"
            ]
        );
    }

    #[test]
    fn a_new_conversation_id_is_a_random_uuid_v4() {
        let id = new_session_id();
        let groups: Vec<usize> = id.split('-').map(str::len).collect();
        assert_eq!(groups, [8, 4, 4, 4, 12], "{id}");
        assert!(
            id.chars()
                .all(|c| c == '-' || matches!(c, '0'..='9' | 'a'..='f')),
            "{id}"
        );
        assert_eq!(id.as_bytes()[14], b'4', "version nibble of {id}");
        assert!(
            matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'),
            "variant nibble of {id}"
        );
    }

    #[test]
    fn each_new_conversation_id_differs() {
        assert_ne!(new_session_id(), new_session_id());
    }

    #[test]
    fn a_prompt_starting_with_a_bang_is_a_shell_line_of_what_follows() {
        assert_eq!(shell_line("!echo hi"), Some("echo hi"));
        assert_eq!(shell_line("!"), Some(""));
    }

    #[test]
    fn a_bang_anywhere_but_first_leaves_the_prompt_a_message() {
        assert_eq!(shell_line("say !this"), None);
        assert_eq!(shell_line(" !ls"), None);
    }

    #[tokio::test]
    async fn the_listing_is_read_past_what_a_shell_starting_claude_code_prints() {
        let listing = r#"[{"pid":1,"cwd":"/r","kind":"interactive","sessionId":"abc"}]"#;
        let start = |_| {
            let script = format!("echo welcome; echo '[INFO] ready'; echo '{listing}'");
            ("sh".to_string(), vec!["-c".to_string(), script], Vec::new())
        };

        let got = list_sessions(start).await.unwrap();

        assert_eq!(got.len(), 1);
        assert_eq!(got[0].session_id, "abc");
    }
}
