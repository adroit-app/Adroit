//! Builders for the sessions, worktrees and trees tests start from, shared by
//! every crate's tests through the `testing` feature.
//!
//! Each builder starts from the plainest value, a session adroit runs with
//! nothing reported of it, and names only what a test sets, so a field added
//! to [`Session`] changes this file alone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::profile::{AgentConnection, AgentProfile};
use crate::{
    Agent, AgentPermissions, Session, SessionId, State, TerminalSession, Tree, Wait, Worktree,
    build,
};

/// A session adroit runs, with `id` as its id and its name.
pub fn session(id: impl Into<String>) -> SessionBuilder {
    let id = id.into();
    SessionBuilder(Session {
        name: id.clone(),
        id,
        ..Session::default()
    })
}

/// A [`Session`] being built; see [`session`].
#[derive(Debug, Clone)]
#[must_use]
pub struct SessionBuilder(Session);

impl SessionBuilder {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.0.name = name.into();
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.0.cwd = cwd.into();
        self
    }

    pub fn state(mut self, state: State) -> Self {
        self.0.state = state;
        self
    }

    pub fn working(self) -> Self {
        self.state(State::Working)
    }

    pub fn waiting(self, wait: Wait) -> Self {
        self.state(State::Waiting(wait))
    }

    pub fn agent(mut self, agent: Agent) -> Self {
        self.0.agent = agent;
        self
    }

    /// The conversation's id.
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.0.session_id = id.into();
        self
    }

    pub fn pid(mut self, pid: u32) -> Self {
        self.0.pid = Some(pid);
        self
    }

    pub fn started_ms(mut self, at: u64) -> Self {
        self.0.started_ms = Some(at);
        self
    }

    pub fn last_input_ms(mut self, at: u64) -> Self {
        self.0.last_input_ms = Some(at);
        self
    }

    pub fn permissions(mut self, permissions: AgentPermissions) -> Self {
        self.0.permissions = permissions;
        self
    }

    /// Adroit's record of the session.
    pub fn delegation(mut self, id: SessionId) -> Self {
        self.0.delegation = Some(id);
        self
    }

    /// The session that started this one.
    pub fn parent(mut self, id: SessionId) -> Self {
        self.0.parent = Some(id);
        self
    }

    pub fn adroit_editor(mut self, runs: bool) -> Self {
        self.0.adroit_editor = runs;
        self
    }

    pub fn build(self) -> Session {
        self.0
    }
}

impl From<SessionBuilder> for Session {
    fn from(b: SessionBuilder) -> Self {
        b.build()
    }
}

/// The worktree at `path` of the repository whose main checkout is `repo`,
/// on `branch`.
pub fn worktree(
    path: impl Into<PathBuf>,
    repo: impl Into<PathBuf>,
    branch: impl Into<String>,
) -> Worktree {
    Worktree {
        path: path.into(),
        repo: repo.into(),
        branch: Some(branch.into()),
    }
}

/// A shell adroit runs in `cwd`, named after its id.
pub fn shell(id: u128, cwd: impl Into<PathBuf>) -> TerminalSession {
    TerminalSession {
        id,
        name: format!("shell-{id}"),
        cwd: cwd.into(),
        ..TerminalSession::default()
    }
}

/// What git said of each directory, as [`build`] takes it: `None` for one
/// outside git.
pub fn found(entries: &[(&str, Option<Worktree>)]) -> HashMap<PathBuf, Option<Worktree>> {
    entries
        .iter()
        .map(|(dir, w)| (PathBuf::from(dir), w.clone()))
        .collect()
}

/// A tree with nothing in it yet.
pub fn tree() -> TreeBuilder {
    TreeBuilder::default()
}

/// What [`build`] is handed, gathered one piece at a time; see [`tree`].
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct TreeBuilder {
    sessions: Vec<Session>,
    terminals: Vec<TerminalSession>,
    found: HashMap<PathBuf, Option<Worktree>>,
    listed: Vec<Worktree>,
    scratch: Option<PathBuf>,
}

impl TreeBuilder {
    pub fn session(mut self, session: impl Into<Session>) -> Self {
        self.sessions.push(session.into());
        self
    }

    pub fn sessions<S: Into<Session>>(mut self, sessions: impl IntoIterator<Item = S>) -> Self {
        self.sessions.extend(sessions.into_iter().map(Into::into));
        self
    }

    pub fn terminal(mut self, terminal: TerminalSession) -> Self {
        self.terminals.push(terminal);
        self
    }

    /// What git said of `dir`: the worktree holding it, or `None` outside git.
    pub fn found(mut self, dir: impl Into<PathBuf>, worktree: Option<Worktree>) -> Self {
        self.found.insert(dir.into(), worktree);
        self
    }

    /// `path` found as the worktree of `repo` on `branch`, from its top.
    pub fn checkout(self, path: &str, repo: &str, branch: &str) -> Self {
        self.found(path, Some(worktree(path, repo, branch)))
    }

    /// A worktree git lists, whether anything runs in it or not.
    pub fn listed(mut self, worktree: Worktree) -> Self {
        self.listed.push(worktree);
        self
    }

    pub fn scratch(mut self, root: impl AsRef<Path>) -> Self {
        self.scratch = Some(root.as_ref().to_path_buf());
        self
    }

    pub fn build(self) -> Tree {
        build(
            &self.sessions,
            &self.terminals,
            &self.found,
            &self.listed,
            self.scratch.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_one_adroit_runs_named_by_its_id_until_told_otherwise() {
        let s = session("fix").build();
        assert_eq!((s.id.as_str(), s.name.as_str()), ("fix", "fix"));
        assert_eq!((s.state, s.agent), (State::Idle, Agent::Claude));
        let named = session("fix").name("Fix login").cwd("/api").build();
        assert_eq!(
            (named.id.as_str(), named.name.as_str(), named.cwd.as_path()),
            ("fix", "Fix login", Path::new("/api"))
        );
    }

    #[test]
    fn a_tree_is_built_from_what_it_was_handed() {
        let sh = shell(1, "/api");
        let t = tree()
            .session(session("a").cwd("/api"))
            .terminal(sh.clone())
            .checkout("/api", "/api", "main")
            .listed(worktree("/api/w", "/api", "w"))
            .build();
        assert_eq!(
            t,
            build(
                &[session("a").cwd("/api").build()],
                &[sh],
                &found(&[("/api", Some(worktree("/api", "/api", "main")))]),
                &[worktree("/api/w", "/api", "w")],
                None,
            )
        );
    }
}

/// A profile with `id` as its id and its name that starts `agent` in the
/// agent's own environment, as the CLI's own settings say.
pub fn profile(id: &str, agent: Agent) -> ProfileBuilder {
    ProfileBuilder(AgentProfile {
        id: id.parse().expect("a valid profile id"),
        name: id.to_string(),
        ..AgentProfile::builtin(agent)
    })
}

/// An [`AgentProfile`] being built; see [`profile`].
#[derive(Debug, Clone)]
#[must_use]
pub struct ProfileBuilder(AgentProfile);

impl ProfileBuilder {
    /// The connection it runs in, by id.
    pub fn connection(mut self, id: &str) -> Self {
        self.0.connection = id.parse().expect("a valid connection id");
        self
    }

    pub fn archived(mut self) -> Self {
        self.0.archived = true;
        self
    }

    pub fn build(self) -> AgentProfile {
        self.0
    }
}

/// A connection with `id` as its id and its name, of `agent`, kept in `home`.
pub fn connection(id: &str, agent: Agent, home: impl Into<PathBuf>) -> ConnectionBuilder {
    ConnectionBuilder(AgentConnection {
        id: id.parse().expect("a valid connection id"),
        name: id.to_string(),
        agent,
        home: home.into(),
        archived: false,
    })
}

/// An [`AgentConnection`] being built; see [`connection`].
#[derive(Debug, Clone)]
#[must_use]
pub struct ConnectionBuilder(AgentConnection);

impl ConnectionBuilder {
    pub fn build(self) -> AgentConnection {
        self.0
    }
}
