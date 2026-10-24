//! The Agents pages of the Settings window: what its sidebar lists under
//! Agents, and which page is shown for what was chosen there.

use adroit_model::Agent;
use adroit_model::profile::{ConnectionId, ProfileId};

use crate::settings::Settings;

/// A page under Agents.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentPage {
    /// One profile, to edit.
    Profile(ProfileId),
    /// A profile to make.
    NewProfile,
    /// What accounts are, and one to make.
    Accounts,
    /// One account, which settings.toml calls a connection.
    Account(ConnectionId),
    /// The profiles and accounts archived, to bring back.
    Archived,
}

/// One entry of the sidebar under Agents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The page it opens; none for a caption over the entries after it.
    pub page: Option<AgentPage>,
    pub label: String,
    /// Whether it is the profile new sessions start from.
    pub default: bool,
    /// How far it is set in: 1 under Agents, 2 under Accounts.
    pub depth: u8,
}

impl Entry {
    fn plain(page: AgentPage, label: &str, depth: u8) -> Self {
        Self {
            page: Some(page),
            ..Self::caption(label, depth)
        }
    }

    fn caption(label: &str, depth: u8) -> Self {
        Self {
            page: None,
            label: label.to_string(),
            default: false,
            depth,
        }
    }
}

/// The sidebar under Agents, in order: every profile not archived, a new
/// one, the accounts under a caption naming their agent with its shared
/// one first, then what is archived when anything is.
pub fn entries(settings: &Settings) -> Vec<Entry> {
    let profiles = settings
        .profiles
        .iter()
        .filter(|p| !p.archived)
        .map(|p| Entry {
            default: p.id == settings.profile,
            ..Entry::plain(AgentPage::Profile(p.id.clone()), p.label(), 1)
        });
    let accounts = Agent::ALL.into_iter().flat_map(|agent| {
        let shared = (ConnectionId::implicit(agent), "Shared".to_string());
        let own = settings
            .connections
            .iter()
            .filter(move |c| c.agent == agent && !c.archived)
            .map(|c| {
                (
                    c.id.clone(),
                    crate::profiles::connection_name(settings, &c.id),
                )
            });
        let listed = std::iter::once(shared)
            .chain(own)
            .map(|(id, label)| Entry::plain(AgentPage::Account(id), &label, 2));
        std::iter::once(Entry::caption(agent.label(), 2)).chain(listed)
    });
    let archive = has_archived(settings).then(|| Entry::plain(AgentPage::Archived, "Archived", 1));
    profiles
        .chain([
            Entry::plain(AgentPage::NewProfile, "New profile", 1),
            Entry::plain(AgentPage::Accounts, "Accounts", 1),
        ])
        .chain(accounts)
        .chain(archive)
        .collect()
}

fn has_archived(settings: &Settings) -> bool {
    settings.profiles.iter().any(|p| p.archived) || settings.connections.iter().any(|c| c.archived)
}

/// The page shown for `chosen`: itself while it still has something to
/// show, else the default profile's for a profile or the archive, and the
/// accounts' for an account. Nothing chosen shows the default profile.
pub fn shown(settings: &Settings, chosen: Option<&AgentPage>) -> AgentPage {
    let default = || AgentPage::Profile(settings.profile.clone());
    match chosen {
        None => default(),
        Some(AgentPage::Profile(id)) => {
            if settings.profiles.iter().any(|p| &p.id == id && !p.archived) {
                AgentPage::Profile(id.clone())
            } else {
                default()
            }
        }
        Some(AgentPage::Account(id)) => {
            let open = id.implicit_agent().is_some()
                || settings
                    .connections
                    .iter()
                    .any(|c| &c.id == id && !c.archived);
            if open {
                AgentPage::Account(id.clone())
            } else {
                AgentPage::Accounts
            }
        }
        Some(AgentPage::Archived) if !has_archived(settings) => default(),
        Some(page) => page.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::profile::{AgentConnection, AgentProfile};
    use adroit_model::testing::{connection, profile};

    fn settings() -> Settings {
        Settings {
            profile: "claude-work".parse().unwrap(),
            profiles: vec![
                profile("claude", Agent::Claude).build(),
                AgentProfile {
                    name: "Claude Work".into(),
                    ..profile("claude-work", Agent::Claude)
                        .connection("work")
                        .build()
                },
                profile("codex", Agent::Codex).build(),
                profile("old", Agent::Claude).archived().build(),
            ],
            connections: vec![
                AgentConnection {
                    name: "Work".into(),
                    ..connection("work", Agent::Claude, "/w").build()
                },
                connection("gone", Agent::Claude, "/g").archived().build(),
                connection("ci", Agent::Codex, "/c").build(),
            ],
            ..Settings::default()
        }
    }

    fn pid(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    fn cid(text: &str) -> ConnectionId {
        text.parse().unwrap()
    }

    #[test]
    fn the_sidebar_lists_profiles_a_new_one_accounts_under_their_agent_and_the_archive() {
        let listed: Vec<(String, bool, u8, bool)> = entries(&settings())
            .into_iter()
            .map(|e| (e.label, e.default, e.depth, e.page.is_none()))
            .collect();
        let row = |label: &str, default: bool, depth: u8, caption: bool| {
            (label.to_string(), default, depth, caption)
        };
        assert_eq!(
            listed,
            [
                row("claude", false, 1, false),
                row("Claude Work", true, 1, false),
                row("codex", false, 1, false),
                row("New profile", false, 1, false),
                row("Accounts", false, 1, false),
                row("Claude Code", false, 2, true),
                row("Shared", false, 2, false),
                row("Work", false, 2, false),
                row("Codex", false, 2, true),
                row("Shared", false, 2, false),
                row("ci", false, 2, false),
                row("Archived", false, 1, false),
            ]
        );
    }

    #[test]
    fn each_entry_opens_the_page_of_what_it_names() {
        let pages: Vec<Option<AgentPage>> =
            entries(&settings()).into_iter().map(|e| e.page).collect();
        assert_eq!(
            pages,
            [
                Some(AgentPage::Profile(pid("claude"))),
                Some(AgentPage::Profile(pid("claude-work"))),
                Some(AgentPage::Profile(pid("codex"))),
                Some(AgentPage::NewProfile),
                Some(AgentPage::Accounts),
                None,
                Some(AgentPage::Account(ConnectionId::implicit(Agent::Claude))),
                Some(AgentPage::Account(cid("work"))),
                None,
                Some(AgentPage::Account(ConnectionId::implicit(Agent::Codex))),
                Some(AgentPage::Account(cid("ci"))),
                Some(AgentPage::Archived),
            ]
        );
    }

    #[test]
    fn the_archive_is_listed_only_while_something_is_archived() {
        let mut s = settings();
        s.profiles.retain(|p| !p.archived);
        s.connections.retain(|c| !c.archived);
        assert!(
            !entries(&s)
                .iter()
                .any(|e| e.page == Some(AgentPage::Archived))
        );
        s.connections[0].archived = true;
        s.profiles[1].connection = ConnectionId::implicit(Agent::Claude);
        assert!(
            entries(&s)
                .iter()
                .any(|e| e.page == Some(AgentPage::Archived))
        );
    }

    #[test]
    fn nothing_chosen_shows_the_default_profile() {
        assert_eq!(
            shown(&settings(), None),
            AgentPage::Profile(pid("claude-work"))
        );
    }

    #[test]
    fn a_page_chosen_is_shown_while_it_has_something_to_show() {
        let s = settings();
        for page in [
            AgentPage::Profile(pid("codex")),
            AgentPage::NewProfile,
            AgentPage::Accounts,
            AgentPage::Account(cid("ci")),
            AgentPage::Account(ConnectionId::implicit(Agent::Codex)),
            AgentPage::Archived,
        ] {
            assert_eq!(shown(&s, Some(&page)), page);
        }
    }

    #[test]
    fn a_profile_archived_or_gone_falls_back_to_the_default_profile() {
        let s = settings();
        let default = AgentPage::Profile(pid("claude-work"));
        assert_eq!(shown(&s, Some(&AgentPage::Profile(pid("old")))), default);
        assert_eq!(shown(&s, Some(&AgentPage::Profile(pid("nope")))), default);
    }

    #[test]
    fn an_account_archived_or_gone_falls_back_to_the_accounts() {
        let s = settings();
        assert_eq!(
            shown(&s, Some(&AgentPage::Account(cid("gone")))),
            AgentPage::Accounts
        );
        assert_eq!(
            shown(&s, Some(&AgentPage::Account(cid("nope")))),
            AgentPage::Accounts
        );
    }

    #[test]
    fn the_archive_emptied_falls_back_to_the_default_profile() {
        let mut s = settings();
        s.profiles.retain(|p| !p.archived);
        s.connections.retain(|c| !c.archived);
        assert_eq!(
            shown(&s, Some(&AgentPage::Archived)),
            AgentPage::Profile(pid("claude-work"))
        );
    }
}
