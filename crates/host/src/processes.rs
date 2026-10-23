//! The live processes of a terminal agent session, and stopping one.

use std::collections::HashSet;
use std::path::PathBuf;

use adroit_model::Session;
use adroit_model::config::{format_command, parse_command};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, UpdateKind};

pub use adroit_model::Process;

#[derive(Default)]
pub struct Sampler {
    system: sysinfo::System,
}

impl Sampler {
    fn refresh(&mut self) -> Vec<Process> {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cmd(UpdateKind::Always)
                .with_exe(UpdateKind::Always)
                .with_environ(UpdateKind::OnlyIfNotSet)
                .with_cwd(UpdateKind::Always)
                .with_memory(),
        );
        self.system
            .processes()
            .iter()
            .map(|(pid, p)| {
                let pid = pid.as_u32();
                let args: Vec<String> = p
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().into_owned())
                    .collect();
                Process {
                    key: format!("\0process:{pid}:{}", p.start_time()),
                    pid,
                    parent: p.parent().map(|p| p.as_u32()),
                    name: p.name().to_string_lossy().into_owned(),
                    command: format_command(&args),
                    executable: p
                        .exe()
                        .map(PathBuf::from)
                        .or_else(|| args.first().map(PathBuf::from)),
                    cwd: p.cwd().map(PathBuf::from),
                    memory: p.memory(),
                    started_secs: p.start_time(),
                    zombie: p.status() == sysinfo::ProcessStatus::Zombie,
                    owner: p
                        .environ()
                        .iter()
                        .filter_map(|s| s.to_str())
                        .find_map(|s| s.strip_prefix("FELIS_SESSION_ID=").map(str::to_owned)),
                }
            })
            .collect()
    }

    pub fn sample(&mut self, session: &Session) -> Vec<Process> {
        associated(&self.refresh(), session)
            .into_iter()
            .cloned()
            .collect()
    }
}

fn agent_runtime(process: &Process) -> bool {
    let reserved = |name: &str| {
        matches!(
            name,
            "codex" | "claude" | "node_repl" | "codex-code-mode-host"
        )
    };
    let file = process
        .executable
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str());
    reserved(&process.name)
        || file.is_some_and(reserved)
        || (file == Some("adroit") && serves_mcp(process))
}

/// Whether it runs `adroit mcp`, the server through which the agent reaches
/// adroit's tools.
fn serves_mcp(process: &Process) -> bool {
    parse_command(&process.command).is_some_and(|args| args.get(1).is_some_and(|a| a == "mcp"))
}

fn associated<'a>(all: &'a [Process], session: &Session) -> Vec<&'a Process> {
    let own = (!session.id.is_empty()).then_some(session.id.as_str());
    let root = session
        .pid
        .and_then(|pid| all.iter().find(|p| p.pid == pid))
        .or_else(|| {
            all.iter()
                .filter(|p| own.is_some() && p.owner.as_deref() == own)
                .min_by_key(|p| (p.started_secs, p.pid))
        });
    let Some(root) = root else { return Vec::new() };
    let owner = own.or(root.owner.as_deref());
    let mut included = HashSet::from([root.pid]);
    loop {
        let mut changed = false;
        for p in all {
            let owned = p.owner.is_none() || p.owner.as_deref() == owner;
            if owned && p.parent.is_some_and(|parent| included.contains(&parent)) {
                changed |= included.insert(p.pid);
            }
        }
        if !changed {
            break;
        }
    }
    let mut selected: Vec<_> = all
        .iter()
        .filter(|p| {
            p.pid != root.pid
                && !agent_runtime(p)
                && (included.contains(&p.pid) || (own.is_some() && p.owner.as_deref() == own))
        })
        .collect();
    selected.sort_by_key(|p| p.pid);
    selected
}

fn verified<'a>(
    all: &'a [Process],
    session: &Session,
    target: &Process,
) -> Result<&'a Process, String> {
    let found = associated(all, session)
        .into_iter()
        .find(|p| p.pid == target.pid && p.started_secs == target.started_secs)
        .ok_or_else(|| {
            "The process ended, changed, or no longer belongs to this session".to_string()
        })?;
    if found.zombie {
        return Err(format!(
            "PID {} has already exited; its parent PID {} must reap it",
            found.pid,
            found.parent.unwrap_or(0)
        ));
    }
    Ok(found)
}

pub fn kill(session: &Session, target: &Process) -> Result<(), String> {
    let mut sampler = Sampler::default();
    let all = sampler.refresh();
    let current = verified(&all, session, target)?;
    let process = sampler
        .system
        .process(Pid::from_u32(current.pid))
        .ok_or_else(|| "The process has ended".to_string())?;
    match process.kill_with(sysinfo::Signal::Kill) {
        Some(true) => Ok(()),
        Some(false) => Err(format!(
            "Cannot kill PID {}: permission denied or process ended",
            current.pid
        )),
        None => Err("Killing processes is not supported on this platform".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::Session;

    fn process(pid: u32, parent: u32, owner: Option<&str>) -> Process {
        Process {
            key: format!("\0process:{pid}:1"),
            pid,
            parent: Some(parent),
            owner: owner.map(str::to_owned),
            started_secs: u64::from(pid),
            name: format!("process-{pid}"),
            ..Process::default()
        }
    }

    #[test]
    fn an_agent_runtime_is_protected_by_its_executable_even_when_its_display_name_changes() {
        let root = process(10, 1, None);
        let mut runtime = process(11, 10, None);
        runtime.name = "worker".into();
        runtime.executable =
            Some("/Applications/ChatGPT.app/Contents/Resources/cua_node/bin/node_repl".into());
        let session = Session {
            pid: Some(10),
            ..Default::default()
        };
        assert!(verified(&[root, runtime.clone()], &session, &runtime).is_err());
        for name in ["codex", "claude", "node_repl", "codex-code-mode-host"] {
            runtime.executable = Some(PathBuf::from("/runtime/bin").join(name));
            assert!(agent_runtime(&runtime), "{name}");
        }
    }

    #[test]
    fn agent_runtimes_are_never_listed_or_killable_but_their_work_processes_are() {
        let mut all = vec![
            process(100, 1, Some("session")),
            process(110, 100, Some("session")),
            process(120, 100, Some("session")),
            process(130, 120, Some("session")),
            process(140, 130, None),
            process(150, 1, Some("session")),
            process(160, 1, None),
        ];
        all[0].name = "codex".into();
        all[1].name = "node_repl".into();
        all[2].name = "codex-code-mode-host".into();
        all[3].name = "zsh".into();
        all[4].name = "node".into();
        let session = Session {
            id: "session".into(),
            pid: Some(100),
            ..Default::default()
        };
        assert_eq!(
            associated(&all, &session)
                .iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>(),
            [130, 140, 150]
        );
        for runtime in &all[..3] {
            assert!(verified(&all, &session, runtime).is_err());
        }
        assert!(verified(&all, &session, &all[3]).is_ok());
    }

    #[test]
    fn adroit_s_own_helper_is_a_supporting_runtime_but_its_other_commands_are_work() {
        let mut helper = process(11, 10, None);
        helper.executable = Some("/Applications/Adroit.app/Contents/MacOS/adroit".into());
        helper.command = "/Applications/Adroit.app/Contents/MacOS/adroit mcp --channel".into();
        assert!(agent_runtime(&helper));
        helper.command = "'/Users/me/My Apps/adroit' mcp".into();
        assert!(agent_runtime(&helper));
        let mut work = helper.clone();
        work.command = "adroit terminal start --name probe".into();
        assert!(!agent_runtime(&work));
        let mut other = helper;
        other.executable = Some("/usr/local/bin/my-tool".into());
        other.command = "my-tool mcp".into();
        assert!(!agent_runtime(&other));
    }

    #[test]
    fn descendants_exclude_the_agent_unrelated_processes_and_other_sessions() {
        let all = vec![
            process(10, 1, Some("session")),
            process(11, 10, Some("session")),
            process(12, 11, None),
            process(13, 1, Some("session")),
            process(20, 1, Some("other")),
            process(21, 20, Some("other")),
        ];
        let session = Session {
            id: "session".into(),
            ..Session::default()
        };
        let shown = associated(&all, &session);
        assert_eq!(
            shown.iter().map(|p| p.pid).collect::<Vec<_>>(),
            [11, 12, 13]
        );
        let external = Session {
            pid: Some(20),
            ..Session::default()
        };
        assert_eq!(
            associated(&all, &external)
                .iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>(),
            [21]
        );
        assert!(associated(&all, &Session::default()).is_empty());
    }

    #[test]
    fn a_known_external_pid_does_not_claim_ancestors_with_inherited_session_environment() {
        let all = vec![
            process(10, 1, Some("same")),
            process(11, 10, Some("same")),
            process(12, 11, Some("same")),
        ];
        let session = Session {
            pid: Some(11),
            ..Session::default()
        };
        assert_eq!(
            associated(&all, &session)
                .iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>(),
            [12]
        );
        assert!(verified(&all, &session, &all[0]).is_err());
        let mut zombie = all[2].clone();
        zombie.zombie = true;
        assert!(
            verified(&[all[1].clone(), zombie.clone()], &session, &zombie)
                .unwrap_err()
                .contains("parent PID 11")
        );
    }

    #[test]
    fn process_identity_includes_start_time_and_refuses_unrelated_or_reused_pids() {
        let session = Session {
            pid: Some(10),
            ..Session::default()
        };
        let all = vec![
            process(10, 1, None),
            process(11, 10, None),
            process(20, 1, None),
        ];
        let target = all[1].clone();
        assert!(verified(&all, &session, &target).is_ok());
        let mut reused = target.clone();
        reused.started_secs += 1;
        assert!(verified(&all, &session, &reused).is_err());
        assert!(verified(&all, &session, &all[2]).is_err());
        assert!(verified(&all, &session, &all[0]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn an_identified_child_can_be_killed_and_an_exited_child_cannot_be_killed_again() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let session = Session {
            pid: Some(std::process::id()),
            ..Session::default()
        };
        let target = Sampler::default()
            .sample(&session)
            .into_iter()
            .find(|p| p.pid == child.id());
        let Some(target) = target else {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the child must be associated with its test parent");
        };
        let result = kill(&session, &target);
        if result.is_err() {
            let _ = child.kill();
        }
        let status = child.wait().unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert!(!status.success());
        assert!(kill(&session, &target).is_err());
    }
}
