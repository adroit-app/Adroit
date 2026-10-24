use crate::{Session, State, Tree, Wait};

/// A session waiting on the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxItem {
    pub workspace: String,
    pub session: Session,
    pub wait: Wait,
}

/// Every session waiting on the user, across all workspaces, in the order the
/// tree holds them.
pub fn inbox(tree: &Tree) -> Vec<InboxItem> {
    tree.sessions()
        .filter_map(|(workspace, s)| match &s.state {
            State::Waiting(wait) => Some(InboxItem {
                workspace: workspace.to_string(),
                session: s.clone(),
                wait: wait.clone(),
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{session, tree};

    #[test]
    fn lists_only_waiting_sessions_with_their_workspace() {
        let tree = tree()
            .session(session("busy").cwd("/api").working())
            .session(session("asks").cwd("/api").waiting(Wait::Permission))
            .session(session("other").cwd("/notes").waiting(Wait::Input))
            .build();
        let items = inbox(&tree);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].session.name, "asks");
        assert_eq!(items[0].workspace, "api");
        assert_eq!(items[0].wait, Wait::Permission);
        assert_eq!(items[1].workspace, "notes");
    }
}
