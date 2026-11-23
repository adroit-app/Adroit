//! What a window or a command asks of the work host and waits for: each ask
//! is answered once, by its number, with what came of it or why it failed.

use std::path::PathBuf;

use adroit_model::{Agent, AgentPermissions, TerminalProgram, TerminalSession};
use serde::{Deserialize, Serialize};

use crate::{DraftChanges, Drafts, HostPath};

/// A file or a directory to paste at `target`, putting what is there in
/// the trash first when `replace`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasteEntry {
    pub source: PathBuf,
    pub target: PathBuf,
    pub replace: bool,
}

/// What is asked of the work host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Ask {
    PathEntries {
        root: Option<HostPath>,
        typed: String,
    },
    Apply {
        stamp: crate::MutationStamp,
        request: Box<crate::Request>,
    },
    MutationStatus {
        stamp: crate::MutationStamp,
    },
    MutationEpoch,
    SubmitPrompt {
        session: String,
        agent: Agent,
        text: String,
        images: Vec<PathBuf>,
    },
    /// Start a session with its program held back until the session is
    /// released.
    StartSession(Box<SessionStart>),
    /// Start a shell, a Neovim or a command with its program held back until
    /// the terminal is released.
    StartTerminal(Box<TerminalStart>),
    /// How the terminal host stands against this build.
    TerminalHost,
    /// Replace the terminal host with one of this build, ending every
    /// session it holds.
    RestartTerminalHost,
    /// Keep an image pasted into the prompt, of `mime`, its bytes in base64.
    KeepImage {
        mime: String,
        data: String,
    },
    /// Keep a copy of the image file at `path`, dropped on the prompt.
    KeepImageFile {
        path: PathBuf,
    },
    /// Make `path`, a directory when `dir` or else an empty file, with the
    /// directories it passes through; one already there is left as it is.
    MakeEntry {
        path: PathBuf,
        dir: bool,
    },
    /// Copy each entry's source to its target, or move it there when
    /// `moving`. A target already there is left as it is and answered as
    /// taken, unless the entry says to replace it, which puts it in the
    /// trash first.
    PasteEntries {
        entries: Vec<PasteEntry>,
        moving: bool,
    },
    /// Put each of `paths` in the work machine's trash.
    TrashEntries {
        paths: Vec<PathBuf>,
    },
    /// Make the settings those `text` writes in `settings.toml`'s format.
    SaveSettings {
        text: String,
    },
    SaveSettingsChecked {
        text: String,
        expected: crate::FileText,
    },
    /// Make `text` the key bindings over the defaults, as `keys.toml` holds
    /// them.
    SaveKeys {
        text: String,
    },
    /// What is written in the prompts, as kept.
    Drafts,
    /// Keep what a window changed of what is written in the prompts.
    KeepDrafts(Box<DraftChanges>),
    ConsumeDraft {
        key: String,
        text: String,
        images: Vec<PathBuf>,
    },
    /// The image file at `path` as a `data:` URL: whole, or as the small copy
    /// a list draws.
    Picture {
        path: PathBuf,
        whole: bool,
    },
    /// Whether the profile `profile` is logged in, as the CLI it starts says
    /// where it runs it, asked once the terminal `after`, when one is named,
    /// has ended.
    LoginStanding {
        profile: adroit_model::profile::ProfileId,
        #[serde(default, with = "crate::wire::wide_opt")]
        after: Option<u128>,
    },
    /// Start a terminal that runs the CLI's own login where the profile
    /// `profile` runs it.
    OpenLogin {
        profile: adroit_model::profile::ProfileId,
        cols: u16,
        rows: u16,
    },
    /// Delete the folder of its own the profile `profile` keeps its login
    /// in, as the settings say now: one adroit made, in which no session
    /// runs.
    DeleteLogin {
        profile: adroit_model::profile::ProfileId,
    },
    /// Bring back what the terminal host took with it, chosen by the
    /// daemon ids it ran as.
    BringBack {
        ids: Vec<String>,
    },
    /// Let go of what the terminal host took with it, chosen by the daemon
    /// ids it ran as, so it is offered no more.
    LetGo {
        ids: Vec<String>,
    },
    /// Clone the repository `typed` names with `ghq get` in a terminal held
    /// back until it is released, which goes on showing what the clone
    /// wrote once it ends.
    Clone {
        typed: String,
        cols: u16,
        rows: u16,
    },
    /// Where ghq keeps the repository at `path` under its roots, asked once
    /// the terminal `after` that cloned it has ended.
    Cloned {
        path: String,
        #[serde(with = "crate::wire::wide")]
        after: u128,
    },
    /// Make a worktree of the repository at `repository` that starts at the
    /// head of its pull request `number` on GitHub, titled `title`.
    PullWorktree {
        repository: PathBuf,
        number: u64,
        title: String,
    },
    /// Read the pull request or the issue `number` of the repository at
    /// `repository` whole, with what was said on it and the images shown.
    GitHubThread {
        repository: PathBuf,
        listing: adroit_model::github::Listing,
        number: u64,
    },
    /// Run the command `adroit <args>` as run in `cwd`, with `input` on its
    /// standard input.
    Run {
        args: Vec<String>,
        cwd: PathBuf,
        input: Option<String>,
    },
}

/// A session to start.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionStart {
    pub agent: Agent,
    /// The profile it starts from, which has to start `agent`; `agent`'s
    /// default profile when none is named.
    #[serde(default)]
    pub profile: Option<adroit_model::profile::ProfileId>,
    pub name: String,
    /// Whether the person starting it gave the name, which its agent then
    /// never changes; a name made from the prompt is the agent's to change.
    #[serde(default)]
    pub name_given: bool,
    /// Where it runs, or the draft's place a new folder is made under when
    /// it is the scratch folder.
    pub cwd: HostPath,
    /// Made from a draft, whose place may be the scratch folder.
    pub draft: bool,
    pub model: Option<String>,
    pub permissions: AgentPermissions,
    pub prompt: String,
    pub images: Vec<PathBuf>,
    /// The conversation it continues, by its id.
    pub resume: Option<String>,
    /// The folder of a profile's own that keeps the conversation it
    /// continues, which it has to run in; none for its agent's own
    /// environment.
    #[serde(default)]
    pub home: Option<PathBuf>,
    pub cols: u16,
    pub rows: u16,
}

/// A shell, a Neovim or a command to start.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalStart {
    pub name: String,
    pub cwd: HostPath,
    pub program: TerminalProgram,
    /// The file Neovim starts on.
    pub file: Option<PathBuf>,
    pub cols: u16,
    pub rows: u16,
}

/// How the terminal host stands against the work host's build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalHostStanding {
    /// None listens.
    Absent,
    /// It speaks a version of the terminal protocol this build does.
    Works,
    /// It speaks none: of the build it says, or none when it is too old to
    /// say which versions it speaks.
    Old { build: Option<String> },
}

impl Ask {
    /// Whether answering it waits on something that may take as long as the
    /// user likes, so the answer is worth nothing once the one who asked has
    /// hung up; every other ask is answered whether or not it is waited for.
    pub fn waits(&self) -> bool {
        match self {
            Self::LoginStanding { after, .. } => after.is_some(),
            Self::Cloned { .. } => true,
            Self::Run { args, .. } => {
                matches!(args.as_slice(), [on, wait, ..] if matches!(on.as_str(), "terminal" | "session") && wait == "wait")
            }
            _ => false,
        }
    }
}

/// What came of an ask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Answer {
    Unconfirmed {
        text: String,
    },
    DraftConsumed {
        cleared: bool,
    },
    PathEntries {
        entries: Vec<crate::PathEntry>,
    },
    Mutation {
        status: crate::MutationState,
    },
    MutationEpoch {
        epoch: u64,
    },
    /// The session started, by its id, held until released.
    Session {
        id: String,
    },
    /// The terminal started, held until released.
    Terminal {
        #[serde(with = "crate::wire::terminal")]
        terminal: TerminalSession,
    },
    TerminalHost {
        standing: TerminalHostStanding,
    },
    /// The image was kept here.
    Kept {
        path: PathBuf,
    },
    /// What a paste did: the entries pasted, the entries whose target was
    /// taken and left alone, and why the rest failed.
    Pasted {
        pasted: Vec<PasteEntry>,
        taken: Vec<PasteEntry>,
        failed: Vec<String>,
    },
    /// The worktree was made here.
    Worktree {
        path: PathBuf,
    },
    /// The pull request or the issue asked for, read whole.
    Thread {
        thread: Box<adroit_model::github::Thread>,
    },
    /// The image asked for, or none when it cannot be read or shown.
    Picture {
        url: Option<String>,
    },
    /// What the command wrote, and its exit status.
    Ran {
        stdout: String,
        stderr: String,
        code: i32,
    },
    /// Whether a profile is logged in, as the CLI it starts said.
    LoginStanding {
        standing: adroit_model::profile::LoginStanding,
    },
    /// What came of bringing back each one chosen.
    BroughtBack {
        outcomes: Vec<adroit_model::BroughtBack>,
    },
    /// Where ghq keeps the repository cloned, or none when it keeps none
    /// there: the clone failed.
    Cloned {
        dir: Option<PathBuf>,
    },
    /// What is written in the prompts, as kept.
    Drafts(Box<Drafts>),
    Done,
    /// It failed, for this reason, said to the user.
    Failed {
        text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(words: &[&str]) -> Ask {
        Ask::Run {
            args: words.iter().map(|w| w.to_string()).collect(),
            cwd: "/src".into(),
            input: None,
        }
    }

    #[test]
    fn waiting_for_a_clone_ends_with_the_one_who_asked() {
        assert!(
            Ask::Cloned {
                path: "github.com/o/r".into(),
                after: 1
            }
            .waits()
        );
    }

    #[test]
    fn waiting_for_a_session_ends_with_the_one_who_asked() {
        assert!(run(&["session", "wait", "wwdc"]).waits());
        assert!(!run(&["session", "show", "wwdc"]).waits());
    }

    #[test]
    fn waiting_for_a_terminal_s_output_ends_with_the_one_who_asked() {
        assert!(run(&["terminal", "wait", "--terminal", "x", "--text", "ready"]).waits());
        assert!(!run(&["terminal", "screen", "--terminal", "x"]).waits());
        assert!(!run(&["wait"]).waits());
        assert!(!run(&[]).waits());
    }
}
