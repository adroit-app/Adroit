//! The asks about the profiles' logins: how each stands, the terminal that
//! logs one in, and deleting the folder one keeps its login in.
use std::path::PathBuf;

use adroit_model::profile::ProfileId;
use adroit_model::{TerminalProgram, TerminalResourceId, TerminalSession};
use adroit_protocol::Answer;
use adroit_term::Spawn;

use super::{failed_spawn, size};
use crate::watch::Watched;
use crate::{daemon, held, programs, sessions, settings};

/// Whether the profile `profile` is logged in, as the CLI it starts says
/// where it runs it, asked in the user's home.
pub(super) async fn login_standing(profile: &ProfileId) -> Result<Answer, String> {
    let context = programs::for_profile(&settings::current(), profile)?;
    let standing = adroit_agents::login::standing(&context, &crate::paths::home()).await;
    Ok(Answer::LoginStanding { standing })
}

/// Delete the folder the profile `profile` keeps its login in, as the
/// settings say now: one adroit made, never the agent's own environment
/// nor a folder named by hand, and none a session still runs in.
pub(super) async fn delete_login(profile: &ProfileId) -> Result<Answer, String> {
    let settings = settings::current();
    let found = settings
        .profiles
        .iter()
        .find(|p| &p.id == profile)
        .ok_or_else(|| format!("profile `{profile}` is not defined"))?;
    let Some(home) = found.home.clone() else {
        return Err(format!(
            "{} runs in your terminal's login, of which adroit deletes nothing",
            found.label()
        ));
    };
    let data = crate::paths::data_dir();
    if !adroit_core::profiles::made_by_adroit(&home, &data, |p| {
        adroit_sys::fs::canonicalize(p).ok()
    }) {
        return Err(format!(
            "{} is not a folder adroit made, so it is left as it is; delete it yourself if it is to go",
            home.display()
        ));
    }
    unused_by_profiles(found, &settings)?;
    let kept = daemon::daemon()
        .sessions()
        .await
        .map_err(|e| e.to_string())?;
    let running: Vec<adroit_model::Session> = kept
        .iter()
        .filter_map(|s| match sessions::read_any_environment(s) {
            Some(sessions::Kept::Agent(session)) => Some(session),
            _ => None,
        })
        .collect();
    unused(found.label(), &home, &running)?;
    match tokio::fs::remove_dir_all(&home).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("{} could not be deleted: {e}", home.display()))
        }
        _ => Ok(Answer::Done),
    }
}

/// Whether no session of `running` runs in `home`, the folder of the
/// profile called `name`; why it cannot be deleted otherwise, naming them.
fn unused(
    name: &str,
    home: &std::path::Path,
    running: &[adroit_model::Session],
) -> Result<(), String> {
    let using: Vec<&str> = running
        .iter()
        .filter(|s| {
            s.home
                .as_deref()
                .is_some_and(|other| overlapping_logins(home, other))
        })
        .map(|s| match s.name.trim() {
            "" => s.id.as_str(),
            named => named,
        })
        .collect();
    if using.is_empty() {
        return Ok(());
    }
    let (runs, end) = match using.len() {
        1 => ("runs", "it"),
        _ => ("run", "them"),
    };
    Err(format!(
        "{} {runs} in {}; end {end}, or delete {name} without its folder",
        using.join(", "),
        home.display()
    ))
}

fn resolved_login(path: &std::path::Path) -> PathBuf {
    if let Ok(path) = adroit_sys::fs::canonicalize(path) {
        return path;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => resolved_login(parent).join(name),
        _ => path.to_path_buf(),
    }
}

fn overlapping_logins(a: &std::path::Path, b: &std::path::Path) -> bool {
    let (a, b) = (resolved_login(a), resolved_login(b));
    a.starts_with(&b) || b.starts_with(&a)
}

fn unused_by_profiles(
    deleting: &adroit_model::profile::AgentProfile,
    settings: &settings::Settings,
) -> Result<(), String> {
    let Some(home) = &deleting.home else {
        return Ok(());
    };
    if let Some(other) = settings.profiles.iter().find(|other| {
        other.id != deleting.id
            && other
                .home
                .as_deref()
                .is_some_and(|other| overlapping_logins(home, other))
    }) {
        return Err(format!(
            "{} keeps its login in an overlapping folder; delete {} without its folder",
            other.label(),
            deleting.label()
        ));
    }
    Ok(())
}

/// Start a terminal, listed in the tree as any other, that runs the CLI's
/// own login where the profile `profile` runs it, in the user's home.
pub(super) async fn open_login(
    watched: &Watched,
    profile: &ProfileId,
    cols: u16,
    rows: u16,
) -> Result<Answer, String> {
    let settings = settings::current();
    let context = programs::for_profile(&settings, profile)?;
    let (program, args, env) = adroit_agents::login::login(&context);
    let cwd = crate::paths::home();
    let terminal = TerminalSession {
        resource_id: TerminalResourceId::new(),
        id: 0,
        name: format!(
            "log in · {}",
            settings
                .find_profile(profile)
                .map_or_else(|| profile.to_string(), |p| p.label().to_string())
        ),
        cwd: cwd.clone(),
        program: TerminalProgram::Command,
        file: None,
        command: std::iter::once(program.clone())
            .chain(args.clone())
            .collect(),
        ..TerminalSession::default()
    };
    let (cols, rows) = size(cols, rows);
    let spawn = Spawn {
        tags: sessions::terminal_tags(&terminal)
            .into_iter()
            .chain(crate::environment::tags())
            .collect(),
        program,
        args,
        cwd,
        env: daemon::terminal_env().into_iter().chain(env).collect(),
        cols,
        rows,
    };
    let id = match daemon::daemon().spawn_held(spawn).await {
        Ok(id) => id,
        Err(error) => return failed_spawn(error, "cannot start the login terminal"),
    };
    held::hold(id).await;
    // Settings, which opens it too, shows no terminal to wait for; the
    // host's own presence has told the program the theme already.
    held::release(id).await;
    let terminal = TerminalSession { id, ..terminal };
    watched.terminal_opened(terminal.clone());
    Ok(Answer::Terminal { terminal })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn deleting_a_login_preserves_running_logins_nested_under_it_or_reached_by_an_alias() {
        use adroit_model::testing::session;
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("work");
        let nested = home.join("other");
        std::fs::create_dir_all(&nested).unwrap();
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&nested, &alias).unwrap();
        for path in [&nested, &alias] {
            let running = [session("a")
                .profile("other", Some(path.to_str().unwrap()))
                .build()];
            assert!(unused("Work", &home, &running).is_err(), "{path:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn deleting_a_login_preserves_other_configured_logins_even_when_none_run() {
        use adroit_model::testing::profile;
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("work");
        std::fs::create_dir(&home).unwrap();
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&home, &alias).unwrap();
        for other in [
            home.join("child"),
            alias.join("not-created"),
            home.clone(),
            dir.path().to_path_buf(),
        ] {
            let deleting = profile("work", adroit_model::Agent::Claude)
                .home(&home)
                .build();
            let settings = settings::Settings {
                profiles: vec![
                    deleting.clone(),
                    profile("other", adroit_model::Agent::Codex)
                        .home(other)
                        .build(),
                ],
                ..Default::default()
            };
            assert!(unused_by_profiles(&deleting, &settings).is_err());
        }
        assert!(!overlapping_logins(&home, &dir.path().join("work-other")));
    }

    #[test]
    fn a_login_s_folder_is_not_deleted_while_sessions_run_in_it_and_says_which() {
        use adroit_model::testing::session;
        let home = std::path::Path::new("/d/adroit/profiles/work");
        let elsewhere = [
            session("a")
                .name("fix")
                .profile("work", Some("/d/x"))
                .build(),
            session("b").name("fix").profile("claude", None).build(),
        ];
        assert_eq!(unused("Work", home, &elsewhere), Ok(()));
        let one = [session("a")
            .name("fix")
            .profile("work", Some("/d/adroit/profiles/work"))
            .build()];
        assert_eq!(
            unused("Work", home, &one),
            Err(
                "fix runs in /d/adroit/profiles/work; end it, or delete Work without its folder"
                    .into()
            )
        );
        let two = [
            one[0].clone(),
            session("0c")
                .name(" ")
                .profile("old", Some("/d/adroit/profiles/work"))
                .build(),
        ];
        assert_eq!(
            unused("Work", home, &two),
            Err("fix, 0c run in /d/adroit/profiles/work; end them, or delete Work without its folder".into())
        );
    }
}
