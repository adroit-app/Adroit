//! The commands an agent offers in a directory, kept for a while as asking
//! costs a process.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use adroit_model::Command;

/// How long an agent is given to answer for its commands.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a directory's commands are offered before they are asked for
/// again. Short, as skills, plugins and MCP servers come and go while
/// sessions run, and asking costs a process for a moment and no tokens.
pub const COMMANDS_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
struct Entry {
    commands: Vec<Command>,
    asked: Option<Instant>,
    asking: bool,
}

/// Each directory's commands, with when they were asked for: a stale list is
/// offered until a fresh one arrives, and one directory is asked once at a
/// time.
#[derive(Debug, Default)]
pub struct Cache {
    dirs: HashMap<PathBuf, Entry>,
}

impl Cache {
    /// The commands known for `dir`, however old; none before the first
    /// answer.
    pub fn get(&self, dir: &Path) -> &[Command] {
        self.dirs.get(dir).map_or(&[], |e| &e.commands)
    }

    /// Whether `dir` should be asked now, at `now`: never asked, or asked
    /// longer than [`COMMANDS_TTL`] ago, and not being asked. A yes counts as
    /// asking until [`Cache::settle`].
    pub fn want(&mut self, dir: &Path, now: Instant) -> bool {
        let e = self.dirs.entry(dir.to_path_buf()).or_default();
        let stale = e
            .asked
            .is_none_or(|at| now.saturating_duration_since(at) >= COMMANDS_TTL);
        if e.asking || !stale {
            return false;
        }
        e.asking = true;
        true
    }

    /// Take the answer for `dir`, at `now`. A failure keeps the list there
    /// was, and waits the same time as an answer before asking again.
    pub fn settle(&mut self, dir: &Path, now: Instant, got: Option<Vec<Command>>) {
        let e = self.dirs.entry(dir.to_path_buf()).or_default();
        e.asking = false;
        e.asked = Some(now);
        if let Some(list) = got {
            e.commands = list;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str, description: &str, argument_hint: &str) -> Command {
        Command {
            name: name.into(),
            description: description.into(),
            argument_hint: argument_hint.into(),
        }
    }

    fn dir() -> PathBuf {
        PathBuf::from("/src/api")
    }

    #[test]
    fn a_directory_never_asked_is_asked_once_until_it_answers() {
        let mut cache = Cache::default();
        let t = Instant::now();
        assert!(cache.want(&dir(), t));
        assert!(!cache.want(&dir(), t + COMMANDS_TTL * 2));
        assert!(cache.get(&dir()).is_empty());
    }

    #[test]
    fn a_fresh_answer_is_offered_without_asking_again() {
        let mut cache = Cache::default();
        let t = Instant::now();
        cache.want(&dir(), t);
        cache.settle(&dir(), t, Some(vec![command("review", "", "")]));
        assert!(!cache.want(&dir(), t + COMMANDS_TTL - Duration::from_secs(1)));
        assert_eq!(cache.get(&dir()), [command("review", "", "")]);
    }

    #[test]
    fn a_stale_answer_is_asked_for_again_once_and_offered_meanwhile() {
        let mut cache = Cache::default();
        let t = Instant::now();
        cache.want(&dir(), t);
        cache.settle(&dir(), t, Some(vec![command("review", "", "")]));
        let later = t + COMMANDS_TTL;
        assert!(cache.want(&dir(), later));
        assert!(!cache.want(&dir(), later));
        assert_eq!(cache.get(&dir()), [command("review", "", "")]);
        cache.settle(&dir(), later, Some(vec![command("plan", "", "")]));
        assert_eq!(cache.get(&dir()), [command("plan", "", "")]);
    }

    #[test]
    fn a_failure_keeps_the_list_there_was_and_waits_before_asking_again() {
        let mut cache = Cache::default();
        let t = Instant::now();
        cache.want(&dir(), t);
        cache.settle(&dir(), t, Some(vec![command("review", "", "")]));
        let later = t + COMMANDS_TTL;
        cache.want(&dir(), later);
        cache.settle(&dir(), later, None);
        assert_eq!(cache.get(&dir()), [command("review", "", "")]);
        assert!(!cache.want(&dir(), later + Duration::from_secs(1)));
        assert!(cache.want(&dir(), later + COMMANDS_TTL));
    }

    #[test]
    fn each_directory_is_asked_on_its_own() {
        let mut cache = Cache::default();
        let t = Instant::now();
        assert!(cache.want(&dir(), t));
        assert!(cache.want(Path::new("/src/web"), t));
    }
}
