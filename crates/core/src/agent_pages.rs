//! The Agents pages of the Settings window: what its sidebar lists under
//! Agents, and which page is shown for what was chosen there.

use adroit_model::profile::ProfileId;

use crate::settings::Settings;

/// A page under Agents.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentPage {
    /// One profile, to edit.
    Profile(ProfileId),
    /// A profile to make.
    NewProfile,
}

/// One entry of the sidebar under Agents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The page it opens.
    pub page: AgentPage,
    pub label: String,
    /// Whether it is the profile new sessions start from.
    pub default: bool,
}

impl Entry {
    fn plain(page: AgentPage, label: &str) -> Self {
        Self {
            page,
            label: label.to_string(),
            default: false,
        }
    }
}

/// The sidebar under Agents, in order: every profile, then a new one.
pub fn entries(settings: &Settings) -> Vec<Entry> {
    let profiles = settings.profiles.iter().map(|p| Entry {
        default: p.id == settings.profile,
        ..Entry::plain(AgentPage::Profile(p.id.clone()), p.label())
    });
    profiles
        .chain([Entry::plain(AgentPage::NewProfile, "New profile")])
        .collect()
}

/// The page shown for `chosen`: itself while it still has something to
/// show, else the default profile's. Nothing chosen shows the default
/// profile.
pub fn shown(settings: &Settings, chosen: Option<&AgentPage>) -> AgentPage {
    let default = || AgentPage::Profile(settings.profile.clone());
    match chosen {
        None => default(),
        Some(AgentPage::Profile(id)) => {
            if settings.profiles.iter().any(|p| &p.id == id) {
                AgentPage::Profile(id.clone())
            } else {
                default()
            }
        }
        Some(page) => page.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::Agent;
    use adroit_model::profile::AgentProfile;
    use adroit_model::testing::profile;

    fn settings() -> Settings {
        Settings {
            profile: "claude-work".parse().unwrap(),
            profiles: vec![
                profile("claude", Agent::Claude).build(),
                AgentProfile {
                    name: "Claude Work".into(),
                    ..profile("claude-work", Agent::Claude).home("/w").build()
                },
                profile("codex", Agent::Codex).home("/c").build(),
            ],
            ..Settings::default()
        }
    }

    fn pid(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    #[test]
    fn the_sidebar_lists_profiles_then_a_new_one() {
        let listed: Vec<(String, bool)> = entries(&settings())
            .into_iter()
            .map(|e| (e.label, e.default))
            .collect();
        let row = |label: &str, default: bool| (label.to_string(), default);
        assert_eq!(
            listed,
            [
                row("claude", false),
                row("Claude Work", true),
                row("codex", false),
                row("New profile", false),
            ]
        );
    }

    #[test]
    fn each_entry_opens_the_page_of_what_it_names() {
        let pages: Vec<AgentPage> = entries(&settings()).into_iter().map(|e| e.page).collect();
        assert_eq!(
            pages,
            [
                AgentPage::Profile(pid("claude")),
                AgentPage::Profile(pid("claude-work")),
                AgentPage::Profile(pid("codex")),
                AgentPage::NewProfile,
            ]
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
        for page in [AgentPage::Profile(pid("codex")), AgentPage::NewProfile] {
            assert_eq!(shown(&s, Some(&page)), page);
        }
    }

    #[test]
    fn a_profile_deleted_falls_back_to_the_default_profile() {
        let mut s = settings();
        let default = AgentPage::Profile(pid("claude-work"));
        s.profiles.retain(|p| p.id.as_str() != "codex");
        assert_eq!(shown(&s, Some(&AgentPage::Profile(pid("codex")))), default);
        assert_eq!(shown(&s, Some(&AgentPage::Profile(pid("nope")))), default);
    }
}
