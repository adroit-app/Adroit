//! The plugin, skill and instructions every OpenCode session adroit starts
//! loads, written into adroit's state folder.

use std::path::{Path, PathBuf};

use crate::pi::extension::write_whole;

const PLUGIN: &str = include_str!("../../opencode/adroit.js");

/// adroit's own folder for this instance's OpenCode.
pub(super) fn instance_dir() -> PathBuf {
    use adroit_model::paths::{instance, instance_dir, state_dir};
    instance_dir(&state_dir(), &instance()).join("opencode")
}

/// The plugin, naming `program` as adroit's command.
fn plugin(program: &Path) -> String {
    let quoted = |text: &str| serde_json::to_string(text).expect("a string as JSON");
    PLUGIN
        .replace("\"@ADROIT@\"", &quoted(&program.to_string_lossy()))
        .replace("\"@OUTSIDE@\"", &quoted(crate::connection::OUTSIDE))
        .replace("\"@SESSION@\"", &quoted(super::SESSION))
}

/// Write the plugin and adroit's skill into `dir`, naming `program`, over
/// what an earlier adroit wrote there: the folder OpenCode loads the plugin
/// from, a folder of its own as OpenCode loads no other, and the folder it
/// finds the skill in.
pub(super) fn install(program: &Path, dir: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    let plugin_dir = dir.join("plugin");
    std::fs::create_dir_all(&plugin_dir)?;
    write_whole(&plugin_dir.join("index.js"), &plugin(program))?;
    let skills = dir.join("skills");
    let skill = skills.join("start-session");
    std::fs::create_dir_all(&skill)?;
    write_whole(
        &skill.join("SKILL.md"),
        &crate::claude::plugin::start_session(program),
    )?;
    Ok((plugin_dir, skills))
}

/// Write `instructions` into `dir` as a file OpenCode reads them from, named
/// by what it holds so that sessions told alike share one: its path.
pub(super) fn instructions(dir: &Path, instructions: &str) -> std::io::Result<PathBuf> {
    let dir = dir.join("instructions");
    std::fs::create_dir_all(&dir)?;
    // FNV-1a, which names the same text alike in every build.
    let hash = instructions
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    let path = dir.join(format!("{hash:016x}.md"));
    write_whole(&path, instructions)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugin_and_skill_are_written_naming_the_adroit_that_wrote_them() {
        let dir = tempfile::tempdir().unwrap();
        let (plugin, skills) = install(Path::new("/opt/my adroit"), dir.path()).unwrap();
        let script = std::fs::read_to_string(plugin.join("index.js")).unwrap();
        assert!(
            script.contains(r#"const ADROIT = "/opt/my adroit";"#),
            "{script}"
        );
        assert!(
            script.contains(&format!(
                r#"const OUTSIDE = "{}";"#,
                crate::connection::OUTSIDE
            )),
            "{script}"
        );
        assert!(!script.contains("@ADROIT@") && !script.contains("@OUTSIDE@"));
        let skill = std::fs::read_to_string(skills.join("start-session/SKILL.md")).unwrap();
        assert!(skill.contains("'/opt/my adroit'"), "{skill}");
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(plugin.join("index.js"))
            .unwrap()
            .permissions();
        assert_eq!(mode.mode() & 0o777, 0o600);
        // Written again by another adroit, it names that one.
        let (plugin, _) = install(Path::new("/opt/other"), dir.path()).unwrap();
        let script = std::fs::read_to_string(plugin.join("index.js")).unwrap();
        assert!(script.contains(r#"const ADROIT = "/opt/other";"#));
    }

    #[test]
    fn instructions_told_alike_are_kept_in_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let one = instructions(dir.path(), "be brief").unwrap();
        assert_eq!(std::fs::read_to_string(&one).unwrap(), "be brief");
        assert_eq!(instructions(dir.path(), "be brief").unwrap(), one);
        assert_ne!(instructions(dir.path(), "be kind").unwrap(), one);
    }

    /// A stand-in for OpenCode's server that loads the plugin, written into
    /// `dir` naming `adroit`, hands it the events of a conversation made once
    /// its first prompt is sent and of a new one opened after it, runs a
    /// shell command through its hook, and prints what it saw as JSON. As
    /// OpenCode, it takes a message once by its id; the first it takes, it
    /// fails to answer.
    const HARNESS: &str = r#"
const [, , plugin, delegated] = process.argv;
if (delegated !== "yes") delete process.env.ADROIT_DELEGATION_TOKEN;
const { default: definition } = await import(plugin);
const seen = { id: definition.id, handed: [], refused: 0, tried: [] };
const made = new Set();
const events = [];
let wake;
const event = (type, data) => {
    events.push({ type, data });
    wake?.();
};
const hooks = {};
const ctx = {
    options: {},
    shell: { hook: async (name, hook) => (hooks[name] = hook) },
    event: {
        subscribe: ({ signal }) => ({
            async *[Symbol.asyncIterator]() {
                while (!signal.aborted) {
                    while (events.length > 0) yield events.shift();
                    await new Promise((r) => (wake = r));
                }
            },
        }),
    },
    session: {
        prompt: async (input) => {
            seen.tried.push(input.id);
            if (!made.has(input.sessionID)) {
                seen.refused++;
                throw new Error("session not found");
            }
            if (seen.handed.some((handed) => handed.id === input.id)) return;
            seen.handed.push(input);
            if (seen.handed.length === 1) throw new Error("the answer was lost");
        },
    },
};
const until = async (check) => {
    for (let i = 0; i < 100 && !check(); i++) await new Promise((r) => setTimeout(r, 50));
};
const stop = await definition.setup(ctx);
const env = {
    XDG_STATE_HOME: "/account/state",
    XDG_DATA_HOME: "/account/data",
    PATH: "/bin",
    "@OUTSIDE@XDG_STATE_HOME": "/me/state",
    "@OUTSIDE@XDG_DATA_HOME": "",
};
await hooks["create.before"]({ command: "ls", env });
seen.shell = env;
if (delegated === "yes") {
    await until(() => seen.refused > 0);
    made.add("ses_first");
    event("session.inbox.enqueued", { sessionID: "ses_first" });
    await until(() => seen.handed.length === 1);
    event("session.updated", { sessionID: "ses_first" });
    const tries = () => seen.tried.filter((id) => id === seen.handed[0].id).length;
    await until(() => tries() >= seen.refused + 2);
    made.add("ses_new");
    made.add("ses_child");
    event("session.created", { sessionID: "ses_new" });
    event("session.created", { sessionID: "ses_child", parentID: "ses_new" });
    await until(() => seen.handed.length === 2);
} else {
    await new Promise((r) => setTimeout(r, 300));
}
await stop();
console.log(JSON.stringify(seen));
process.exit(0);
"#;

    /// What the harness saw, the plugin naming `adroit` as adroit's command,
    /// with a session's credential when `delegated`.
    fn run_in_node(dir: &Path, adroit: &Path, delegated: bool) -> serde_json::Value {
        let script = dir.join("index.mjs");
        std::fs::write(&script, plugin(adroit)).unwrap();
        let harness = dir.join("harness.mjs");
        std::fs::write(
            &harness,
            HARNESS.replace("@OUTSIDE@", crate::connection::OUTSIDE),
        )
        .unwrap();
        let out = std::process::Command::new("node")
            .arg(&harness)
            .arg(&script)
            .arg(if delegated { "yes" } else { "no" })
            .env("ADROIT_DELEGATION_TOKEN", "secret")
            .env("ADROIT_DELEGATION_ROOT", dir)
            .env(super::super::SESSION, "ses_first")
            .output()
            .expect("node");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }

    #[test]
    fn messages_heard_through_adroit_s_channel_are_queued_for_the_conversation_shown() {
        let dir = tempfile::tempdir().unwrap();
        let (_bin, adroit) = crate::testing::program(
            "adroit",
            &format!(
                r#"printf '%s ' "$@" > {args}
IFS= read -r initialize
echo '{{"jsonrpc":"2.0","id":1,"result":{{}}}}'
IFS= read -r initialized
echo '{{"jsonrpc":"2.0","method":"notifications/claude/channel","params":{{"content":"[message] one"}}}}'
sleep 1
echo '{{"jsonrpc":"2.0","method":"notifications/claude/channel","params":{{"content":"[message] two"}}}}'
exec sleep 30"#,
                args = dir.path().join("args").display(),
            ),
        );
        let seen = run_in_node(dir.path(), &adroit, true);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("args")).unwrap(),
            "mcp --channel "
        );
        assert_eq!(seen["id"], "adroit");
        // The first waits for the conversation, made once its prompt is sent.
        assert!(seen["refused"].as_u64().unwrap() >= 1);
        let handed = seen["handed"].as_array().unwrap();
        let ids: Vec<&str> = handed.iter().map(|h| h["id"].as_str().unwrap()).collect();
        assert!(ids.iter().all(|id| id.starts_with("msg_")), "{ids:?}");
        assert_ne!(ids[0], ids[1]);
        // Each try of a message names it alike, so one taken is not taken again.
        let tried = seen["tried"].as_array().unwrap();
        assert!(
            tried.iter().filter(|id| *id == ids[0]).count() >= 3,
            "{tried:?}"
        );
        assert_eq!(
            handed
                .iter()
                .map(|h| serde_json::json!([h["sessionID"], h["text"], h["delivery"]]))
                .collect::<Vec<_>>(),
            [
                serde_json::json!(["ses_first", "[message] one", "queue"]),
                serde_json::json!(["ses_new", "[message] two", "queue"]),
            ]
        );
    }

    #[test]
    fn commands_the_session_runs_get_back_the_folders_from_outside_its_account() {
        let dir = tempfile::tempdir().unwrap();
        let (_bin, adroit) = crate::testing::program(
            "adroit",
            &format!("touch {}", dir.path().join("ran").display()),
        );
        let seen = run_in_node(dir.path(), &adroit, false);
        assert_eq!(
            seen["shell"],
            serde_json::json!({"XDG_STATE_HOME": "/me/state", "PATH": "/bin"})
        );
        // Without a session's credential it hears nothing.
        assert_eq!(seen["handed"], serde_json::json!([]));
        assert!(!dir.path().join("ran").exists());
    }
}
