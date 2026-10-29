//! The remote hosts and the gateway registered as the user's services, so
//! they run without a terminal and start again with the machine.

use std::path::{Path, PathBuf};

/// One of the services an instance runs on a work machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Terminal,
    Work,
    Gateway,
}

impl Role {
    const ALL: [Role; 3] = [Role::Terminal, Role::Work, Role::Gateway];

    /// Its `adroit remote` subcommand.
    fn subcommand(self) -> &'static str {
        match self {
            Role::Terminal => "daemon",
            Role::Work => "host",
            Role::Gateway => "gateway",
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Role::Terminal => "terminal host",
            Role::Work => "work host",
            Role::Gateway => "gateway",
        }
    }
}

/// What the services of one instance run, and with what.
#[derive(Clone, Debug)]
pub struct Plan {
    pub program: PathBuf,
    pub instance: String,
    /// The PATH the services start with.
    pub path: String,
    pub listen: Option<String>,
    /// Whether the gateway announces itself on the local network.
    pub advertise: bool,
    pub home: PathBuf,
    /// Where each service writes what it reports.
    pub logs: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Launchd,
    Systemd,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Launchd
        } else {
            Self::Systemd
        }
    }
}

/// A service's definition and where it is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub role: Role,
    pub name: String,
    pub file: PathBuf,
    pub text: String,
}

/// Runs the platform's service manager.
pub trait Run {
    fn run(&self, program: &str, args: &[String]) -> std::io::Result<String>;
}

fn arguments(plan: &Plan, role: Role) -> Vec<String> {
    let mut arguments = vec![
        plan.program.display().to_string(),
        "remote".into(),
        role.subcommand().into(),
        "--instance".into(),
        plan.instance.clone(),
    ];
    if let (Role::Gateway, Some(listen)) = (role, &plan.listen) {
        arguments.extend(["--listen".into(), listen.clone()]);
    }
    if role == Role::Gateway && !plan.advertise {
        arguments.push("--no-advertise".into());
    }
    arguments
}

pub fn units(plan: &Plan, platform: Platform) -> Vec<Unit> {
    Role::ALL
        .into_iter()
        .map(|role| match platform {
            Platform::Launchd => launchd(plan, role),
            Platform::Systemd => systemd(plan, role),
        })
        .collect()
}

fn launchd(plan: &Plan, role: Role) -> Unit {
    let name = format!("app.adroit.{}.{}", plan.instance, role.subcommand());
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let arguments: String = arguments(plan, role)
        .iter()
        .map(|argument| format!("\t\t<string>{}</string>\n", escape(argument)))
        .collect();
    let log = plan.logs.join(format!("{}.log", role.subcommand()));
    let text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
\t<key>Label</key>\n\
\t<string>{name}</string>\n\
\t<key>ProgramArguments</key>\n\
\t<array>\n\
{arguments}\
\t</array>\n\
\t<key>EnvironmentVariables</key>\n\
\t<dict>\n\
\t\t<key>PATH</key>\n\
\t\t<string>{path}</string>\n\
\t</dict>\n\
\t<key>RunAtLoad</key>\n\
\t<true/>\n\
\t<key>KeepAlive</key>\n\
\t<true/>\n\
\t<key>StandardErrorPath</key>\n\
\t<string>{log}</string>\n\
</dict>\n\
</plist>\n",
        path = escape(&plan.path),
        log = escape(&log.display().to_string()),
    );
    Unit {
        role,
        file: plan
            .home
            .join("Library/LaunchAgents")
            .join(format!("{name}.plist")),
        name,
        text,
    }
}

/// A word systemd reads as one argument, with nothing expanded.
fn systemd_word(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    format!("\"{escaped}\"")
}

fn systemd(plan: &Plan, role: Role) -> Unit {
    let name = format!("adroit-{}-{}.service", plan.instance, role.subcommand());
    let after = match role {
        Role::Terminal => String::new(),
        _ => format!(
            "After=adroit-{}-{}.service\n",
            plan.instance,
            Role::Terminal.subcommand()
        ),
    };
    let command = arguments(plan, role)
        .iter()
        .map(|argument| systemd_word(argument))
        .collect::<Vec<_>>()
        .join(" ");
    let text = format!(
        "[Unit]\n\
Description=Adroit {describe} ({instance})\n\
{after}\
\n\
[Service]\n\
ExecStart={command}\n\
Environment={path}\n\
Restart=always\n\
RestartSec=2\n\
\n\
[Install]\n\
WantedBy=default.target\n",
        describe = role.describe(),
        instance = plan.instance,
        path = systemd_word(&format!("PATH={}", plan.path)),
    );
    Unit {
        role,
        file: plan.home.join(".config/systemd/user").join(&name),
        name,
        text,
    }
}

/// What writing a unit did to the file already there.
#[derive(Clone, Copy, PartialEq)]
enum Written {
    Created,
    Replaced,
    Unchanged,
}

fn write(unit: &Unit) -> std::io::Result<Written> {
    use std::io::Write;
    let written = match std::fs::read_to_string(&unit.file) {
        Ok(text) if text == unit.text => return Ok(Written::Unchanged),
        Ok(_) => Written::Replaced,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Written::Created,
        Err(error) => return Err(error),
    };
    let parent = unit.file.parent().unwrap_or(Path::new("/"));
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(unit.text.as_bytes())?;
    staged.as_file().sync_all()?;
    staged.persist(&unit.file).map_err(|error| error.error)?;
    Ok(written)
}

fn remove(unit: &Unit) -> std::io::Result<()> {
    match std::fs::remove_file(&unit.file) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn uid() -> u32 {
    // SAFETY: getuid has no preconditions.
    unsafe { libc::getuid() }
}

/// Writes the services and starts them; returns what the operator should
/// still know, such as a step only they may take.
pub fn enable(plan: &Plan, platform: Platform, run: &dyn Run) -> std::io::Result<Vec<String>> {
    std::fs::create_dir_all(&plan.logs)?;
    let units = units(plan, platform);
    let written = units
        .iter()
        .map(write)
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut notes = Vec::new();
    match platform {
        Platform::Launchd => {
            for (unit, written) in units.iter().zip(written) {
                let target = format!("gui/{}/{}", uid(), unit.name);
                // Leaving an unchanged, loaded agent alone keeps the terminal
                // host's sessions when only another service changed.
                if written == Written::Unchanged
                    && run
                        .run("launchctl", &["print".into(), target.clone()])
                        .is_ok()
                {
                    continue;
                }
                let _ = run.run("launchctl", &["bootout".into(), target]);
                run.run(
                    "launchctl",
                    &[
                        "bootstrap".into(),
                        format!("gui/{}", uid()),
                        unit.file.display().to_string(),
                    ],
                )?;
            }
        }
        Platform::Systemd => {
            run.run("systemctl", &["--user".into(), "daemon-reload".into()])?;
            let mut enabling = vec!["--user".into(), "enable".into(), "--now".into()];
            enabling.extend(units.iter().map(|unit| unit.name.clone()));
            run.run("systemctl", &enabling)?;
            // Enabling leaves a running unit as it was started.
            let replaced: Vec<String> = units
                .iter()
                .zip(written)
                .filter(|(_, written)| *written == Written::Replaced)
                .map(|(unit, _)| unit.name.clone())
                .collect();
            if !replaced.is_empty() {
                let mut restarting = vec!["--user".into(), "restart".into()];
                restarting.extend(replaced);
                run.run("systemctl", &restarting)?;
            }
            let linger = run.run(
                "loginctl",
                &[
                    "show-user".into(),
                    uid().to_string(),
                    "--property=Linger".into(),
                ],
            );
            if !linger.is_ok_and(|text| text.trim() == "Linger=yes") {
                notes.push(
                    "These services stop when you log out. To keep them running without a login, run: sudo loginctl enable-linger $USER".into(),
                );
            }
        }
    }
    Ok(notes)
}

/// Stops the services and removes them.
pub fn disable(plan: &Plan, platform: Platform, run: &dyn Run) -> std::io::Result<()> {
    let units = units(plan, platform);
    match platform {
        Platform::Launchd => {
            for unit in &units {
                let _ = run.run(
                    "launchctl",
                    &["bootout".into(), format!("gui/{}/{}", uid(), unit.name)],
                );
            }
            units.iter().try_for_each(remove)?;
        }
        Platform::Systemd => {
            // systemctl refuses to disable a unit it has no file for.
            let enabled: Vec<String> = units
                .iter()
                .filter(|unit| unit.file.exists())
                .map(|unit| unit.name.clone())
                .collect();
            if enabled.is_empty() {
                return Ok(());
            }
            let mut disabling = vec!["--user".into(), "disable".into(), "--now".into()];
            disabling.extend(enabled);
            let stopped = run.run("systemctl", &disabling);
            units.iter().try_for_each(remove)?;
            run.run("systemctl", &["--user".into(), "daemon-reload".into()])?;
            stopped?;
        }
    }
    Ok(())
}

/// Runs `adroit remote enable` or `disable` for the settled instance.
pub fn cli(operation: crate::cli::RemoteCommand) -> i32 {
    use crate::cli::RemoteCommand;
    let done = (|| -> std::io::Result<()> {
        let logs = crate::remote::root();
        std::fs::create_dir_all(logs.parent().unwrap_or(Path::new("/")))?;
        crate::remote::private_dir(&logs, true)?;
        let path = crate::env::login_shell()
            .and_then(|shell| crate::env::login_path(&shell))
            .or_else(|| std::env::var("PATH").ok())
            .unwrap_or_else(|| "/usr/bin:/bin".into());
        let (listen, advertise) = match &operation {
            RemoteCommand::Enable { listen, advertise } => (listen.clone(), *advertise),
            _ => (None, true),
        };
        let plan = Plan {
            program: std::env::current_exe()?,
            instance: crate::paths::instance(),
            path,
            listen,
            advertise,
            home: adroit_model::paths::home(),
            logs,
        };
        let platform = Platform::current();
        match operation {
            RemoteCommand::Enable { .. } => {
                for note in enable(&plan, platform, &System)? {
                    println!("{note}");
                }
                for unit in units(&plan, platform) {
                    println!("Started {} as {}", unit.role.describe(), unit.name);
                }
            }
            _ => {
                disable(&plan, platform, &System)?;
                println!(
                    "Stopped and removed the services of instance {}",
                    plan.instance
                );
            }
        }
        Ok(())
    })();
    match done {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("adroit: {error}");
            1
        }
    }
}

/// The service manager on this machine.
pub struct System;

impl Run for System {
    fn run(&self, program: &str, args: &[String]) -> std::io::Result<String> {
        let output = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "{program} {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Recorded {
        calls: RefCell<Vec<String>>,
        linger: &'static str,
    }

    impl Recorded {
        fn new(linger: &'static str) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                linger,
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl Run for Recorded {
        fn run(&self, program: &str, args: &[String]) -> std::io::Result<String> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            Ok(if program == "loginctl" {
                format!("Linger={}", self.linger)
            } else {
                String::new()
            })
        }
    }

    fn plan(home: &Path) -> Plan {
        Plan {
            program: "/opt/Adroit & Co/bin/adroit".into(),
            instance: "ai".into(),
            path: "/opt/homebrew/bin:/usr/bin:/bin".into(),
            listen: Some("10.0.0.2:47130".into()),
            advertise: true,
            home: home.into(),
            logs: home.join("logs"),
        }
    }

    #[test]
    fn launchd_runs_each_service_as_an_agent_of_its_own() {
        let units = units(&plan(Path::new("/Users/u")), Platform::Launchd);
        let names: Vec<_> = units.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "app.adroit.ai.daemon",
                "app.adroit.ai.host",
                "app.adroit.ai.gateway"
            ]
        );
        for unit in &units {
            assert_eq!(
                unit.file,
                Path::new("/Users/u/Library/LaunchAgents").join(format!("{}.plist", unit.name))
            );
            assert!(
                unit.text
                    .contains("<string>/opt/Adroit &amp; Co/bin/adroit</string>")
            );
            assert!(
                unit.text
                    .contains("<string>--instance</string>\n\t\t<string>ai</string>")
            );
            assert!(unit.text.contains("<key>KeepAlive</key>\n\t<true/>"));
            assert!(
                unit.text
                    .contains("<string>/opt/homebrew/bin:/usr/bin:/bin</string>")
            );
        }
        assert!(
            units[2]
                .text
                .contains("<string>--listen</string>\n\t\t<string>10.0.0.2:47130</string>")
        );
        assert!(!units[1].text.contains("--listen"));
        assert!(!units[2].text.contains("--no-advertise"));
        let mut hidden = plan(Path::new("/Users/u"));
        hidden.advertise = false;
        assert!(
            super::units(&hidden, Platform::Launchd)[2]
                .text
                .contains("<string>--no-advertise</string>")
        );
    }

    #[test]
    fn systemd_starts_the_work_host_and_the_gateway_after_the_terminal_host() {
        let units = units(&plan(Path::new("/home/u")), Platform::Systemd);
        let names: Vec<_> = units.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "adroit-ai-daemon.service",
                "adroit-ai-host.service",
                "adroit-ai-gateway.service"
            ]
        );
        assert_eq!(
            units[0].file,
            Path::new("/home/u/.config/systemd/user/adroit-ai-daemon.service")
        );
        assert!(!units[0].text.contains("After="));
        for unit in &units[1..] {
            assert!(unit.text.contains("After=adroit-ai-daemon.service"));
        }
        assert!(units[2].text.contains(
            "ExecStart=\"/opt/Adroit & Co/bin/adroit\" \"remote\" \"gateway\" \"--instance\" \"ai\" \"--listen\" \"10.0.0.2:47130\""
        ));
        assert!(
            units[0]
                .text
                .contains("Environment=\"PATH=/opt/homebrew/bin:/usr/bin:/bin\"")
        );
        assert!(units[0].text.contains("WantedBy=default.target"));
    }

    #[test]
    fn systemd_quotes_what_it_would_otherwise_expand() {
        let mut odd = plan(Path::new("/home/u"));
        odd.program = "/opt/100%\"sure\"/adroit".into();
        let units = units(&odd, Platform::Systemd);
        assert!(
            units[0]
                .text
                .contains("ExecStart=\"/opt/100%%\\\"sure\\\"/adroit\"")
        );
    }

    #[test]
    fn enabling_on_linux_writes_the_units_starts_them_and_asks_for_lingering() {
        let home = tempfile::tempdir().unwrap();
        let plan = plan(home.path());
        let run = Recorded::new("no");
        let notes = enable(&plan, Platform::Systemd, &run).unwrap();
        for unit in units(&plan, Platform::Systemd) {
            assert_eq!(std::fs::read_to_string(&unit.file).unwrap(), unit.text);
        }
        let calls = run.calls();
        assert!(calls.contains(&"systemctl --user daemon-reload".to_string()));
        assert!(calls.contains(
            &"systemctl --user enable --now adroit-ai-daemon.service adroit-ai-host.service adroit-ai-gateway.service".to_string()
        ));
        assert!(
            notes
                .iter()
                .any(|note| note.contains("loginctl enable-linger"))
        );
        let lingering = Recorded::new("yes");
        assert!(
            enable(&plan, Platform::Systemd, &lingering)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn disabling_on_linux_stops_the_units_and_removes_them() {
        let home = tempfile::tempdir().unwrap();
        let plan = plan(home.path());
        enable(&plan, Platform::Systemd, &Recorded::new("yes")).unwrap();
        let run = Recorded::new("yes");
        disable(&plan, Platform::Systemd, &run).unwrap();
        assert!(run.calls().contains(
            &"systemctl --user disable --now adroit-ai-daemon.service adroit-ai-host.service adroit-ai-gateway.service".to_string()
        ));
        for unit in units(&plan, Platform::Systemd) {
            assert!(!unit.file.exists());
        }
    }

    #[test]
    fn enabling_again_on_linux_restarts_only_the_services_it_changed() {
        let home = tempfile::tempdir().unwrap();
        let mut plan = plan(home.path());
        enable(&plan, Platform::Systemd, &Recorded::new("yes")).unwrap();
        let same = Recorded::new("yes");
        enable(&plan, Platform::Systemd, &same).unwrap();
        assert!(!same.calls().iter().any(|call| call.contains("restart")));
        plan.advertise = false;
        let changed = Recorded::new("yes");
        enable(&plan, Platform::Systemd, &changed).unwrap();
        let restarted: Vec<_> = changed
            .calls()
            .into_iter()
            .filter(|call| call.contains("restart"))
            .collect();
        assert_eq!(
            restarted,
            ["systemctl --user restart adroit-ai-gateway.service"]
        );
    }

    #[test]
    fn disabling_on_linux_what_is_not_enabled_does_nothing() {
        let home = tempfile::tempdir().unwrap();
        let run = Recorded::new("yes");
        disable(&plan(home.path()), Platform::Systemd, &run).unwrap();
        assert!(!run.calls().iter().any(|call| call.contains("disable")));
    }

    #[test]
    fn enabling_again_on_macos_reloads_only_the_agents_it_changed() {
        let home = tempfile::tempdir().unwrap();
        let mut plan = plan(home.path());
        enable(&plan, Platform::Launchd, &Recorded::new("yes")).unwrap();
        let same = Recorded::new("yes");
        enable(&plan, Platform::Launchd, &same).unwrap();
        assert!(!same.calls().iter().any(|call| call.contains("bootout")));
        plan.listen = None;
        let changed = Recorded::new("yes");
        enable(&plan, Platform::Launchd, &changed).unwrap();
        let reloaded: Vec<_> = changed
            .calls()
            .into_iter()
            .filter(|call| call.contains("bootout"))
            .collect();
        assert_eq!(reloaded.len(), 1);
        assert!(reloaded[0].ends_with("/app.adroit.ai.gateway"));
    }

    #[test]
    fn enabling_on_macos_loads_each_agent_and_disabling_unloads_it() {
        let home = tempfile::tempdir().unwrap();
        let plan = plan(home.path());
        let run = Recorded::new("yes");
        assert!(enable(&plan, Platform::Launchd, &run).unwrap().is_empty());
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        let units = units(&plan, Platform::Launchd);
        for unit in &units {
            assert_eq!(std::fs::read_to_string(&unit.file).unwrap(), unit.text);
            assert!(run.calls().contains(&format!(
                "launchctl bootstrap gui/{uid} {}",
                unit.file.display()
            )));
        }
        let run = Recorded::new("yes");
        disable(&plan, Platform::Launchd, &run).unwrap();
        for unit in &units {
            assert!(
                run.calls()
                    .contains(&format!("launchctl bootout gui/{uid}/{}", unit.name))
            );
            assert!(!unit.file.exists());
        }
    }
}
