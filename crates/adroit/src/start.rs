//! `adroit session start`: start a session in the daemon from a command, as
//! a session hands work to another.

use std::io::Read;
use std::path::PathBuf;

use adroit_claude::Access;
use adroit_term::{Daemon, Spawn};

use crate::scratch::{self, Moment};
use crate::sessions::{self, Kept};
use crate::tabs::{fresh_name, label_from, label_of};

const USAGE: &str =
    "usage: adroit session start [--dir <path>] [--name <name>] [--read-only] [-- <prompt>]";

/// Where Claude Code tells a command it runs which conversation it is.
const CONVERSATION_ENV: &str = "CLAUDE_CODE_SESSION_ID";

/// The size a session starts at, until a window shows it at its own.
const COLS: u16 = 120;
const ROWS: u16 = 40;

/// What `adroit session start` was asked for.
#[derive(Debug, Default, PartialEq, Eq)]
struct Asked {
    dir: Option<PathBuf>,
    name: Option<String>,
    read_only: bool,
    /// `None` when the prompt is to be read from standard input.
    prompt: Option<String>,
}

fn parse(args: &[String]) -> Option<Asked> {
    let mut asked = Asked::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--dir" => asked.dir = Some(rest.next()?.into()),
            "--name" => asked.name = Some(rest.next()?.clone()),
            "--read-only" => asked.read_only = true,
            "--" => {
                let words: Vec<&str> = rest.by_ref().map(String::as_str).collect();
                asked.prompt = Some(words.join(" "));
            }
            _ => return None,
        }
    }
    Some(asked)
}

/// The name the session goes by: the one given, or one made from its prompt,
/// numbered when taken.
fn name_for(given: Option<&str>, prompt: &str, taken: &[&str]) -> String {
    let label = given
        .and_then(label_of)
        .unwrap_or_else(|| label_from(prompt));
    fresh_name(&label, taken)
}

/// The conversation the command was run from, as Claude Code tells it.
fn started_from(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

async fn start(
    daemon: &Daemon,
    asked: &Asked,
    dir: PathBuf,
    prompt: &str,
    from: Option<&str>,
) -> Result<String, String> {
    let listed = daemon
        .sessions()
        .await
        .map_err(|e| format!("cannot reach the daemon: {e}"))?;
    let names: Vec<String> = listed
        .iter()
        .filter_map(|s| match sessions::read(s)? {
            Kept::Claude(c) => Some(c.name),
            Kept::Shell(_) => None,
        })
        .collect();
    let taken: Vec<&str> = names.iter().map(String::as_str).collect();
    let name = name_for(asked.name.as_deref(), prompt, &taken);
    let conversation = adroit_claude::new_session_id();
    let access = if asked.read_only {
        Access::Read
    } else {
        Access::Edit
    };
    let program = std::env::current_exe().unwrap_or_else(|_| "adroit".into());
    let mut args = crate::plugin::args(&program);
    args.extend(adroit_claude::start_args(
        &name,
        &conversation,
        access,
        prompt,
    ));
    let spawn = Spawn {
        program: adroit_claude::claude_program(),
        args,
        cwd: dir.clone(),
        env: adroit_claude::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .chain(crate::daemon::terminal_env())
            .collect(),
        tags: sessions::claude_tags(&name, &dir, &conversation, asked.read_only, now_ms(), from),
        cols: COLS,
        rows: ROWS,
    };
    daemon
        .spawn(spawn)
        .await
        .map_err(|e| format!("cannot start {name}: {e}"))?;
    Ok(name)
}

/// `adroit session <args>`. Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    let Some(asked) = (match args {
        [op, rest @ ..] if op == "start" => parse(rest),
        _ => None,
    }) else {
        eprintln!("{USAGE}");
        return 2;
    };
    let prompt = match &asked.prompt {
        Some(p) => p.clone(),
        None => {
            let mut p = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut p) {
                eprintln!("adroit: cannot read the prompt: {e}");
                return 1;
            }
            p
        }
    };
    let prompt = prompt.trim();
    if prompt.is_empty() {
        eprintln!("adroit: the prompt is empty");
        return 2;
    }
    let from = started_from(std::env::var(CONVERSATION_ENV).ok());
    // The daemon hands the new session this process's environment, which
    // holds the marks of the session it was run from.
    let names = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned());
    for k in crate::env::inherited(names) {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::remove_var(k) };
    }
    let dir = match asked
        .dir
        .clone()
        .map_or_else(std::env::current_dir, Ok)
        .and_then(std::fs::canonicalize)
        .and_then(|d| scratch::start_dir(&d, &crate::paths::scratch_dir(), Moment::now()))
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("adroit: {e}");
            return 1;
        }
    };
    let started = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
        .and_then(|rt| {
            rt.block_on(start(
                &crate::daemon::daemon(),
                &asked,
                dir,
                prompt,
                from.as_deref(),
            ))
        });
    match started {
        Ok(name) => {
            println!("{name}");
            0
        }
        Err(e) => {
            eprintln!("adroit: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn the_prompt_follows_the_options_after_a_double_dash() {
        assert_eq!(
            parse(&args(&[
                "--dir",
                "/src/api",
                "--name",
                "flaky",
                "--read-only",
                "--",
                "find why",
                "--it fails"
            ])),
            Some(Asked {
                dir: Some("/src/api".into()),
                name: Some("flaky".into()),
                read_only: true,
                prompt: Some("find why --it fails".into()),
            })
        );
    }

    #[test]
    fn without_a_double_dash_the_prompt_is_left_to_standard_input() {
        assert_eq!(parse(&args(&[])), Some(Asked::default()));
    }

    #[test]
    fn an_unknown_option_or_one_missing_its_value_is_refused() {
        assert_eq!(parse(&args(&["--model", "x"])), None);
        assert_eq!(parse(&args(&["find why"])), None);
        assert_eq!(parse(&args(&["--dir"])), None);
    }

    #[test]
    fn a_session_is_named_from_its_prompt_unless_given_one() {
        assert_eq!(
            name_for(None, "Find why the login test is flaky", &[]),
            "find-login-test"
        );
        assert_eq!(name_for(Some("Review API"), "anything", &[]), "review-api");
    }

    #[test]
    fn a_name_already_taken_gets_a_number() {
        assert_eq!(
            name_for(Some("review"), "anything", &["review"]),
            "review-2"
        );
    }

    #[test]
    fn a_session_is_started_from_the_conversation_claude_code_names() {
        assert_eq!(started_from(Some("a08b".into())).as_deref(), Some("a08b"));
        assert_eq!(started_from(Some(String::new())), None);
        assert_eq!(started_from(None), None);
    }
}
