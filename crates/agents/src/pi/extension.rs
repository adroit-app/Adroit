//! The extension and skill every Pi session adroit starts loads, written
//! into adroit's state folder, and what the extension records there of the
//! conversations Pi holds.

use std::io::Write;
use std::path::{Path, PathBuf};

use adroit_model::Running;

const EXTENSION: &str = include_str!("../../pi/adroit.ts");
const LOGIN: &str = include_str!("../../pi/login.ts");

/// How far apart the start a record names and the one the system tells of
/// its process may be for both to be the same process: the extension can
/// only reckon its own from how long it has run.
const START_SLACK_SECS: u64 = 5;

/// `text` as a string of the extension's script.
fn quoted(text: &Path) -> String {
    serde_json::to_string(&text.to_string_lossy()).expect("a path as JSON")
}

/// The extension, naming `program` as adroit's command and `running` as the
/// folder it keeps its records in.
fn extension(program: &Path, running: &Path) -> String {
    EXTENSION
        .replace("\"@ADROIT@\"", &quoted(program))
        .replace("\"@RUNNING@\"", &quoted(running))
}

/// The folder in `dir`, adroit's own for Pi, the extension keeps the record
/// of each Pi process in.
fn running_in(dir: &Path) -> PathBuf {
    dir.join("running")
}

/// adroit's own folder for this instance's Pi.
fn instance_dir() -> PathBuf {
    use adroit_model::paths::{instance, instance_dir, state_dir};
    instance_dir(&state_dir(), &instance()).join("pi")
}

/// The folder this instance's Pi sessions keep their records in.
pub(super) fn running_dir() -> PathBuf {
    running_in(&instance_dir())
}

/// Put `contents` at `path`, readable by its user alone, unless it is there
/// already. It is written beside and moved over, so a Pi loading it at that
/// moment reads the old file or the new one whole, and whatever stood at
/// `path`, a link included, is replaced rather than written through.
pub(crate) fn write_whole(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
        && std::fs::read(path).is_ok_and(|kept| kept == contents.as_bytes())
    {
        return Ok(());
    }
    let written = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&written)
        .and_then(|mut file| file.write_all(contents.as_bytes()))
        .and_then(|()| std::fs::rename(&written, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&written);
    }
    result
}

/// Write the extension and adroit's skill into `dir`, naming `program`, over
/// what an earlier adroit wrote there: the extension's path and the skill's.
fn install(program: &Path, dir: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let script = dir.join("adroit.ts");
    write_whole(&script, &extension(program, &running_in(dir)))?;
    let skill = dir.join("skills").join("start-session");
    std::fs::create_dir_all(&skill)?;
    write_whole(
        &skill.join("SKILL.md"),
        &crate::claude::plugin::start_session(program),
    )?;
    Ok((script, skill))
}

/// The arguments that load the extension and the skill, written afresh into
/// `dir` for the session about to start; none when they cannot be written,
/// as the session runs without them.
pub(super) fn args(program: &Path, dir: &Path) -> Vec<String> {
    match install(program, dir) {
        Ok((script, skill)) => vec![
            "-e".into(),
            script.display().to_string(),
            "--skill".into(),
            skill.display().to_string(),
        ],
        Err(_) => Vec::new(),
    }
}

/// What opens Pi to log in: its own interface with `/login` typed in, keeping
/// no session.
pub(super) fn login_args() -> Vec<String> {
    login_args_in(&instance_dir())
}

/// What opens Pi to log in, with the extension that types `/login` written
/// into `dir`, adroit's own folder for Pi: a shared temporary folder would
/// let another user put a script of theirs in its place.
fn login_args_in(dir: &Path) -> Vec<String> {
    let mut args: Vec<String> = ["--no-session", "--tui-mode", "regular"]
        .map(String::from)
        .to_vec();
    let script = dir.join("login.ts");
    let written = std::fs::create_dir_all(dir).and_then(|()| write_whole(&script, LOGIN));
    if written.is_ok() {
        args.extend(["-e".into(), script.display().to_string()]);
    }
    args
}

/// When the process `pid` started, in seconds since the epoch, while it runs.
fn started_secs(pid: u32) -> Option<u64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system
        .process(pid)
        .filter(|p| p.status() != sysinfo::ProcessStatus::Zombie)
        .map(|p| p.start_time())
}

/// The conversation each Pi process recorded in `dir` holds. A record whose
/// process is gone, as one killed or crashed leaves it, or whose pid now
/// names a process started at another time, is removed.
pub(super) fn running(dir: &Path) -> Vec<Running> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .filter_map(|path| {
            Some((
                serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).ok()?).ok()?,
                path,
            ))
        })
        .filter_map(|(record, path)| {
            let pid = u32::try_from(record["pid"].as_u64()?).ok()?;
            let alive = record["started"].as_u64().is_some_and(|started| {
                started_secs(pid).is_some_and(|now| now.abs_diff(started) <= START_SLACK_SECS)
            });
            if !alive {
                let _ = std::fs::remove_file(&path);
                return None;
            }
            Some(Running {
                pid,
                session_id: record["sessionId"].as_str()?.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_names_the_adroit_that_wrote_it_and_where_it_keeps_its_records() {
        let script = extension(
            Path::new("/My Apps/\"adroit\""),
            Path::new("/state/pi/running"),
        );
        assert!(
            script.contains(r#"const ADROIT = "/My Apps/\"adroit\"";"#),
            "{script}"
        );
        assert!(script.contains(r#"const RUNNING = "/state/pi/running";"#));
        assert!(!script.contains("@ADROIT@") && !script.contains("@RUNNING@"));
    }

    /// A stand-in for Pi that loads the extension, written into `dir` naming
    /// `adroit`, opens a conversation, takes what the extension does, moves
    /// to another conversation as `/new` does, and prints what it saw as JSON.
    const HARNESS: &str = r#"
const [, , extension, delegated] = process.argv;
if (delegated !== "yes") {
    delete process.env.ADROIT_DELEGATION_TOKEN;
}
const { default: load } = await import(extension);
const seen = { servers: [], handed: [] };
const session = (id) => {
    const handlers = {};
    let active = true;
    const pi = {
        registerMcpServer: (name, config) => seen.servers.push({ name, ...config }),
        on: (event, handler) => (handlers[event] = handler),
        // Pi's own returns nothing, and refuses every call once replaced.
        sendUserMessage: (content, options) => {
            if (!active) throw new Error("replaced");
            seen.handed.push({ session: id, content, deliverAs: options?.deliverAs });
        },
    };
    load(pi);
    return {
        start: () => handlers.session_start({}, { sessionManager: { getSessionId: () => id } }),
        stop: async () => {
            await handlers.session_shutdown({});
            active = false;
        },
    };
};
const until = async (check) => {
    for (let i = 0; i < 100 && !check(); i++) await new Promise((r) => setTimeout(r, 50));
};
const first = session("conv-1");
seen.exitListeners = [process.listenerCount("exit")];
await first.start();
if (delegated === "yes") {
    await until(() => seen.handed.length === 1);
    const { readFileSync } = await import("node:fs");
    seen.record = JSON.parse(readFileSync(`${process.env.RECORDS}/${process.pid}.json`, "utf8"));
    await first.stop();
    const second = session("conv-2");
    seen.exitListeners.push(process.listenerCount("exit"));
    await second.start();
    await until(() => seen.handed.length === 2);
    globalThis[Symbol.for("adroit.pi")].listener?.kill();
} else {
    await new Promise((r) => setTimeout(r, 300));
}
console.log(JSON.stringify(seen));
process.exit(0);
"#;

    /// What the harness saw, the extension naming `adroit` as adroit's
    /// command, with a session's credential when `delegated`.
    fn run_in_node(dir: &Path, adroit: &Path, delegated: bool) -> serde_json::Value {
        let script = dir.join("adroit.mjs");
        std::fs::write(&script, extension(adroit, &running_in(dir))).unwrap();
        let harness = dir.join("harness.mjs");
        std::fs::write(&harness, HARNESS).unwrap();
        let out = std::process::Command::new("node")
            .arg(&harness)
            .arg(&script)
            .arg(if delegated { "yes" } else { "no" })
            .env("ADROIT_DELEGATION_TOKEN", "secret")
            .env("ADROIT_DELEGATION_ROOT", dir)
            .env("RECORDS", running_in(dir))
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
    fn messages_heard_through_adroit_s_channel_are_handed_to_the_conversation_open() {
        let dir = tempfile::tempdir().unwrap();
        // adroit's server, answering the extension as it answers Claude Code:
        // it is told who asks, then writes each message as a notification.
        let (_bin, adroit) = crate::testing::program(
            "adroit",
            &format!(
                r#"printf '%s ' "$@" > {args}
IFS= read -r initialize
echo '{{"jsonrpc":"2.0","id":1,"result":{{}}}}'
IFS= read -r initialized
printf '%s' "$initialized" > {initialized}
echo '{{"jsonrpc":"2.0","method":"notifications/claude/channel","params":{{"content":"[message] one"}}}}'
sleep 1
echo '{{"jsonrpc":"2.0","method":"notifications/claude/channel","params":{{"content":"[message] two"}}}}'
exec sleep 30"#,
                args = dir.path().join("args").display(),
                initialized = dir.path().join("initialized").display(),
            ),
        );
        let seen = {
            let record = running_in(dir.path());
            std::fs::create_dir_all(&record).unwrap();
            let seen = run_in_node(dir.path(), &adroit, true);
            assert!(
                running(&record).is_empty(),
                "a record outlived its conversation"
            );
            seen
        };
        assert_eq!(
            std::fs::read_to_string(dir.path().join("args")).unwrap(),
            "mcp --channel "
        );
        assert!(
            std::fs::read_to_string(dir.path().join("initialized"))
                .unwrap()
                .contains("notifications/initialized")
        );
        assert_eq!(
            seen["handed"],
            serde_json::json!([
                {"session": "conv-1", "content": "[message] one", "deliverAs": "followUp"},
                {"session": "conv-2", "content": "[message] two", "deliverAs": "followUp"},
            ])
        );
        let servers = seen["servers"].as_array().unwrap();
        assert!(servers.iter().all(|s| s["name"] == "adroit"
            && s["command"] == adroit.display().to_string()
            && s["args"] == serde_json::json!(["mcp"])
            && s["exposure"] == "direct"));
        assert_eq!(seen["record"]["sessionId"], "conv-1");
        assert!(seen["record"]["pid"].as_u64().is_some());
        assert!(seen["record"]["started"].as_u64().is_some());
        assert_eq!(
            seen["exitListeners"][0], seen["exitListeners"][1],
            "each load added a listener of its own"
        );
    }

    #[test]
    fn a_pi_without_a_session_s_credential_gets_no_server_and_hears_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (_bin, adroit) = crate::testing::program(
            "adroit",
            &format!("touch {}", dir.path().join("ran").display()),
        );
        let seen = run_in_node(dir.path(), &adroit, false);
        assert_eq!(seen["servers"], serde_json::json!([]));
        assert_eq!(seen["handed"], serde_json::json!([]));
        assert!(!dir.path().join("ran").exists());
    }

    #[test]
    fn a_session_loads_the_extension_and_adroit_s_skill_written_afresh() {
        let dir = tempfile::tempdir().unwrap();
        let args = args(Path::new("/old/adroit"), dir.path());
        let args = super::args(Path::new("/new/adroit"), dir.path())
            .into_iter()
            .chain(args)
            .collect::<Vec<_>>();
        let script = dir.path().join("adroit.ts");
        let skill = dir.path().join("skills/start-session");
        assert_eq!(
            args[..4],
            [
                "-e".to_string(),
                script.display().to_string(),
                "--skill".into(),
                skill.display().to_string()
            ]
        );
        let written = std::fs::read_to_string(&script).unwrap();
        assert!(written.contains("/new/adroit") && !written.contains("/old/adroit"));
        let running = dir.path().join("running").display().to_string();
        assert!(written.contains(&running));
        let skill = std::fs::read_to_string(skill.join("SKILL.md")).unwrap();
        assert!(skill.starts_with("---\nname: start-session\n"));
        assert!(skill.contains("/new/adroit session start"));
    }

    #[test]
    fn a_folder_that_cannot_be_written_starts_the_session_without_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, "").unwrap();
        assert!(args(Path::new("/adroit"), &file.join("pi")).is_empty());
    }

    #[test]
    fn a_pi_reading_the_extension_as_another_session_starts_reads_it_whole() {
        let dir = tempfile::tempdir().unwrap();
        let (script, _) = install(Path::new("/old/adroit"), dir.path()).unwrap();
        let mut reading = std::fs::File::open(&script).unwrap();
        install(Path::new("/new/adroit"), dir.path()).unwrap();
        let mut read = String::new();
        std::io::Read::read_to_string(&mut reading, &mut read).unwrap();
        assert_eq!(
            read,
            extension(Path::new("/old/adroit"), &running_in(dir.path()))
        );
    }

    #[test]
    fn the_extension_and_the_skill_are_left_alone_when_they_would_be_written_the_same() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().unwrap();
        let (script, skill) = install(Path::new("/adroit"), dir.path()).unwrap();
        let skill = skill.join("SKILL.md");
        let inode = |path: &Path| std::fs::metadata(path).unwrap().ino();
        let before = (inode(&script), inode(&skill));
        install(Path::new("/adroit"), dir.path()).unwrap();
        assert_eq!((inode(&script), inode(&skill)), before);
    }

    #[test]
    fn the_login_opens_pi_s_own_interface_with_login_typed_in_and_keeps_no_session() {
        let dir = tempfile::tempdir().unwrap();
        let args = login_args_in(dir.path());
        assert_eq!(args[..3], ["--no-session", "--tui-mode", "regular"]);
        assert_eq!(args[3], "-e");
        assert_eq!(Path::new(&args[4]).parent(), Some(dir.path()));
        let script = std::fs::read_to_string(&args[4]).unwrap();
        assert!(script.contains("setEditorText(\"/login\")"));
    }

    #[test]
    fn the_login_s_extension_is_the_user_s_alone_and_replaces_whatever_stood_at_its_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let planted = dir.path().join("planted.ts");
        std::fs::write(&planted, "planted").unwrap();
        std::os::unix::fs::symlink(&planted, dir.path().join("login.ts")).unwrap();
        let args = login_args_in(dir.path());
        let script = Path::new(&args[4]);
        assert_eq!(std::fs::read_to_string(&planted).unwrap(), "planted");
        assert!(!script.is_symlink());
        assert_eq!(std::fs::read_to_string(script).unwrap(), LOGIN);
        let mode = std::fs::metadata(script).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// A record of the process `pid`, started at `started`, holding `session`.
    fn record(dir: &Path, file: &str, pid: u32, started: u64, session: &str) {
        let record = serde_json::json!({"pid": pid, "started": started, "sessionId": session});
        std::fs::write(dir.join(file), record.to_string()).unwrap();
    }

    #[test]
    fn each_record_names_the_process_and_the_conversation_it_holds() {
        let dir = tempfile::tempdir().unwrap();
        let me = std::process::id();
        let started = started_secs(me).unwrap();
        record(dir.path(), "11.json", me, started, "a");
        std::fs::write(dir.path().join("12.json"), r#"{"pid":12}"#).unwrap();
        std::fs::write(
            dir.path().join("13.json.13.tmp"),
            r#"{"pid":13,"sessionId":"c"}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("14.json"), "half").unwrap();
        assert_eq!(
            running(dir.path()),
            [Running {
                pid: me,
                session_id: "a".into()
            }]
        );
        assert!(running(&dir.path().join("none")).is_empty());
    }

    #[test]
    fn a_record_left_by_a_process_gone_or_by_one_whose_pid_is_now_another_s_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let me = std::process::id();
        let started = started_secs(me).unwrap();
        let gone = {
            let mut child = std::process::Command::new("true").spawn().unwrap();
            child.wait().unwrap();
            child.id()
        };
        record(dir.path(), "live.json", me, started, "live");
        record(dir.path(), "gone.json", gone, started, "gone");
        record(dir.path(), "reused.json", me, started - 3600, "reused");
        assert_eq!(
            running(dir.path()),
            [Running {
                pid: me,
                session_id: "live".into()
            }]
        );
        assert!(dir.path().join("live.json").exists());
        assert!(!dir.path().join("gone.json").exists());
        assert!(!dir.path().join("reused.json").exists());
    }
}
