//! Profiles as earlier files hold them: while logins were connections, kept
//! apart from the profiles that named them, read as profiles that own their
//! logins, so no login and no folder is lost; while profiles were archived
//! rather than deleted, those archived left out.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::Deserialize;

use super::{AgentProfile, ProfileEntry, ProfileId, plain_absolute, valid_id};
use crate::Agent;

/// A profile as a file of any version writes it, with the connection it
/// names when it was written with connections, and whether it was archived.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "ProfileEntry")]
pub struct Written {
    pub profile: AgentProfile,
    pub connection: Option<String>,
    pub archived: bool,
}

impl TryFrom<ProfileEntry> for Written {
    type Error = String;

    fn try_from(mut entry: ProfileEntry) -> Result<Self, Self::Error> {
        let connection = entry.connection.take();
        let archived = entry.archived;
        Ok(Self {
            profile: entry.profile()?,
            connection,
            archived,
        })
    }
}

/// A `[[connections]]` table: a state directory of one agent's CLI that
/// profiles named to run in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub agent: Agent,
    pub home: PathBuf,
    #[serde(default)]
    pub archived: bool,
}

/// `written` without those archived, each given the home of the connection
/// it names. Of the profiles that name one connection, the first keeps its
/// home; the others run in their agent's own environment, and are listed
/// after the profiles. Why not, when the connections or what names them do
/// not hold together.
pub fn adopt(
    written: Vec<Written>,
    connections: &[Connection],
) -> Result<(Vec<AgentProfile>, Vec<ProfileId>), String> {
    let mut seen = HashSet::new();
    for connection in connections {
        if !valid_id(&connection.id) {
            return Err(format!("`{}` is not a valid connection id", connection.id));
        }
        if !seen.insert(&connection.id) {
            return Err(format!("connection `{}` is defined twice", connection.id));
        }
        if let Some(agent) = Agent::parse(&connection.id) {
            return Err(format!(
                "connection id `{}` is reserved for {}'s own environment",
                connection.id,
                agent.label()
            ));
        }
        if !plain_absolute(&connection.home) {
            return Err(format!(
                "connection `{}` needs an absolute path as its home, written without `~`, `.` or `..` and with no `/` doubled or at its end",
                connection.id
            ));
        }
    }
    let mut named: Vec<(AgentProfile, Option<&Connection>)> = Vec::with_capacity(written.len());
    for Written {
        profile,
        connection,
        ..
    } in written.into_iter().filter(|w| !w.archived)
    {
        let Some(id) = connection else {
            named.push((profile, None));
            continue;
        };
        if profile.home.is_some() {
            return Err(format!(
                "profile `{}` names both `connection` and `home`; keep `home` alone",
                profile.id
            ));
        }
        let agent = profile.agent();
        let found = connections.iter().find(|c| c.id == id);
        let owner = match (Agent::parse(&id), found) {
            (Some(own), _) => own,
            (None, Some(c)) => c.agent,
            (None, None) => {
                return Err(format!(
                    "profile `{}` names connection `{id}`, which is not defined",
                    profile.id
                ));
            }
        };
        if owner != agent {
            return Err(format!(
                "profile `{}` starts {} but connection `{id}` belongs to {}",
                profile.id,
                agent.label(),
                owner.label()
            ));
        }
        named.push((profile, found));
    }
    let keeper = |connection: &Connection| {
        named
            .iter()
            .position(|(_, c)| c.is_some_and(|c| c.id == connection.id))
    };
    let keepers: Vec<Option<usize>> = named.iter().map(|(_, c)| c.and_then(keeper)).collect();
    let mut alone = Vec::new();
    let profiles = named
        .iter()
        .zip(keepers)
        .enumerate()
        .map(|(at, ((profile, connection), keeper))| {
            let mut profile = profile.clone();
            if let Some(connection) = connection {
                if keeper == Some(at) {
                    profile.home = Some(connection.home.clone());
                } else {
                    alone.push(profile.id.clone());
                }
            }
            profile
        })
        .collect();
    Ok((profiles, alone))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::testing::profile;

    fn written(profile: AgentProfile, connection: Option<&str>) -> Written {
        Written {
            profile,
            connection: connection.map(String::from),
            archived: false,
        }
    }

    fn archived(profile: AgentProfile, connection: Option<&str>) -> Written {
        Written {
            archived: true,
            ..written(profile, connection)
        }
    }

    fn connection(id: &str, agent: Agent, home: &str) -> Connection {
        Connection {
            id: id.into(),
            name: String::new(),
            agent,
            home: home.into(),
            archived: false,
        }
    }

    fn homes(profiles: &[AgentProfile]) -> Vec<(&str, Option<&Path>)> {
        profiles
            .iter()
            .map(|p| (p.id.as_str(), p.home.as_deref()))
            .collect()
    }

    #[test]
    fn a_profile_takes_the_home_of_the_connection_it_names() {
        let (profiles, alone) = adopt(
            vec![
                written(profile("own", Agent::Claude).build(), None),
                written(profile("named", Agent::Claude).build(), Some("claude")),
                written(profile("work", Agent::Claude).build(), Some("work")),
            ],
            &[connection("work", Agent::Claude, "/w")],
        )
        .unwrap();
        assert_eq!(
            homes(&profiles),
            [
                ("own", None),
                ("named", None),
                ("work", Some(Path::new("/w")))
            ]
        );
        assert!(alone.is_empty());
    }

    #[test]
    fn a_shared_connection_goes_to_its_first_profile_and_the_rest_run_in_their_agent_s_own_environment()
     {
        let (profiles, alone) = adopt(
            vec![
                written(profile("first", Agent::Claude).build(), Some("work")),
                written(profile("second", Agent::Claude).build(), Some("work")),
            ],
            &[connection("work", Agent::Claude, "/w")],
        )
        .unwrap();
        assert_eq!(
            homes(&profiles),
            [("first", Some(Path::new("/w"))), ("second", None)]
        );
        assert_eq!(alone, ["second".parse::<ProfileId>().unwrap()]);
    }

    #[test]
    fn a_profile_written_archived_is_left_out_and_passes_its_connection_on() {
        let (profiles, alone) = adopt(
            vec![
                archived(profile("old", Agent::Claude).build(), Some("work")),
                written(profile("first", Agent::Claude).build(), Some("work")),
                archived(profile("a", Agent::Codex).build(), Some("ci")),
                archived(profile("b", Agent::Codex).home("/b").build(), None),
            ],
            &[
                connection("work", Agent::Claude, "/w"),
                connection("ci", Agent::Codex, "/c"),
            ],
        )
        .unwrap();
        // A connection only archived profiles named goes to none: its
        // folder is left where it is.
        assert_eq!(homes(&profiles), [("first", Some(Path::new("/w")))]);
        assert!(alone.is_empty());
        let read: Written =
            serde_json::from_str(r#"{"id":"p","agent":"claude","archived":true}"#).unwrap();
        assert!(read.archived);
    }

    #[test]
    fn connections_and_what_names_them_that_do_not_hold_together_say_why() {
        let claude = || profile("claude", Agent::Claude).build();
        let work = || connection("work", Agent::Claude, "/w");
        let refused = |profiles: Vec<Written>, connections: Vec<Connection>, why: &str| {
            let error = adopt(profiles, &connections).unwrap_err();
            assert!(error.contains(why), "{error} does not say {why}");
        };
        refused(
            vec![],
            vec![work(), work()],
            "connection `work` is defined twice",
        );
        refused(
            vec![],
            vec![connection("codex", Agent::Codex, "/c")],
            "connection id `codex` is reserved for Codex's own environment",
        );
        refused(
            vec![],
            vec![connection("Work", Agent::Claude, "/w")],
            "`Work` is not a valid connection id",
        );
        for home in ["", "relative", "~/w", "/w/", "/a//b", "/a/./b", "/a/../b"] {
            refused(
                vec![],
                vec![connection("work", Agent::Claude, home)],
                "connection `work` needs an absolute path as its home",
            );
        }
        refused(
            vec![written(claude(), Some("gone"))],
            vec![],
            "profile `claude` names connection `gone`, which is not defined",
        );
        refused(
            vec![written(claude(), Some("work"))],
            vec![connection("work", Agent::Codex, "/w")],
            "profile `claude` starts Claude Code but connection `work` belongs to Codex",
        );
        refused(
            vec![written(claude(), Some("codex"))],
            vec![],
            "profile `claude` starts Claude Code but connection `codex` belongs to Codex",
        );
        refused(
            vec![written(
                AgentProfile {
                    home: Some("/h".into()),
                    ..claude()
                },
                Some("work"),
            )],
            vec![work()],
            "profile `claude` names both `connection` and `home`",
        );
    }

    #[test]
    fn a_profile_naming_a_connection_is_read_only_as_written_by_an_earlier_version() {
        let earlier: Written =
            serde_json::from_str(r#"{"id":"p","agent":"claude","connection":"work"}"#).unwrap();
        assert_eq!(earlier.connection.as_deref(), Some("work"));
        let error = serde_json::from_str::<AgentProfile>(
            r#"{"id":"p","agent":"claude","connection":"work"}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("profile `p` names connection `work`, which settings name no more"),
            "{error}"
        );
    }
}
