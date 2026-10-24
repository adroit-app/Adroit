//! The window's and the command line's side of delegation: recording the
//! sessions the user starts, which the work host then serves like any other.
use std::path::{Path, PathBuf};

use adroit_model::{Agent, SessionId};

use super::{
    Message, Result,
    launch::Delegation,
    store::{NewSession, Store},
};
use crate::{execution::StartRequest, settings::Settings};

/// Where the work host keeps delegation state and listens for sessions.
pub fn root() -> PathBuf {
    crate::paths::instance_state().join("delegation")
}

pub fn database(root: &Path) -> PathBuf {
    root.join("state.sqlite3")
}

async fn with_store<T: Send + 'static>(
    root: &Path,
    operation: impl FnOnce(&mut Store) -> Result<T> + Send + 'static,
) -> Result<T> {
    let path = database(root);
    tokio::task::spawn_blocking(move || {
        Store::open(&path).and_then(|mut store| operation(&mut store))
    })
    .await
    .map_err(|error| super::Error::new(super::Code::Storage, error.to_string()))?
}

/// A session recorded before its program starts, to be told of its terminal.
#[derive(Debug, Clone)]
pub struct Recorded {
    root: PathBuf,
    pub id: SessionId,
}

impl Recorded {
    pub async fn attach(&self, terminal: u128) -> adroit_term::Result<()> {
        let id = self.id;
        with_store(&self.root, move |store| store.attach(id, terminal))
            .await
            .map_err(|error| adroit_term::Error::Protocol(error.to_string()))
    }

    /// Records that the program never started, so the session is not served.
    pub async fn failed(&self) {
        let id = self.id;
        let _ = with_store(&self.root, move |store| store.end(id)).await;
    }
}

/// Records the session `request` starts, started by `parent` or by the user,
/// and gives the request what it needs to reach other sessions.
pub async fn record(
    root: &Path,
    settings: &Settings,
    request: &mut StartRequest,
    parent: Option<SessionId>,
) -> std::result::Result<Recorded, String> {
    let context =
        crate::programs::starting(settings, Some(request.agent), request.profile.as_ref())?;
    request.profile = Some(context.profile.clone());
    if request.agent == Agent::Codex {
        request.developer_instructions = adroit_agents::adapter(request.agent)
            .developer_instructions(context.clone(), request.cwd.clone())
            .await?;
    }
    let new = NewSession {
        parent,
        agent: request.agent,
        profile: context.profile,
        connection: context.connection,
        name: request.name.clone(),
        cwd: request.cwd.clone(),
        launch: settings.clone(),
        started_ms: request.started_ms,
    };
    let (id, token) = with_store(root, move |store| store.register(new))
        .await
        .map_err(|error| error.to_string())?;
    request.delegation = Some(Delegation::new(id, parent, root.to_path_buf(), token));
    Ok(Recorded {
        root: root.to_path_buf(),
        id,
    })
}

/// Records that the user ended `id`.
pub async fn ended(root: &Path, id: SessionId) -> Result<()> {
    with_store(root, move |store| store.end(id)).await
}

/// What `id` sent and received, the newest first, for its details.
pub async fn messages_of(root: &Path, id: SessionId) -> Result<Vec<Message>> {
    with_store(root, move |store| store.messages_of(id)).await
}

/// The settings `id` was started with, as Adroit recorded them.
pub async fn launch_of(root: &Path, id: SessionId) -> Result<Option<Settings>> {
    with_store(root, move |store| store.launch(id)).await
}

/// Every session Adroit keeps a record of, ended ones included.
pub async fn sessions(root: &Path) -> Result<Vec<super::Session>> {
    with_store(root, |store| store.sessions()).await
}

/// Records the session `request` starts and resolves how to launch it.
pub async fn prepare(
    root: &Path,
    settings: &Settings,
    mut request: StartRequest,
    parent: Option<SessionId>,
    program: &Path,
) -> std::result::Result<(crate::execution::ResolvedLaunch, Recorded), String> {
    // Refused before anything is recorded or asked of the agent, so a start
    // that cannot run leaves nothing behind.
    crate::execution::resolve(settings, request.clone(), program)?;
    let context =
        crate::programs::starting(settings, Some(request.agent), request.profile.as_ref())?;
    crate::programs::ready(&context, &request.cwd, &request.env)?;
    let asked = adroit_agents::AgentContext {
        adroit: program.to_path_buf(),
        ..context
    };
    if let Some(refused) = adroit_agents::adapter(request.agent)
        .refusal(asked, request.cwd.clone())
        .await
    {
        return Err(refused);
    }
    let recorded = record(root, settings, &mut request, parent).await?;
    match crate::execution::resolve(settings, request, program) {
        Ok(resolved) => Ok((resolved, recorded)),
        Err(error) => {
            recorded.failed().await;
            Err(error)
        }
    }
}

/// The sessions `record` messaged or heard from, the most recent first, by
/// the names they go by in `sessions`; Adroit's own notices name no session.
pub fn peers(
    record: SessionId,
    sessions: &[super::Session],
    messages: &[Message],
) -> Vec<adroit_agents::claude::links::Peer> {
    let mut out: Vec<adroit_agents::claude::links::Peer> = Vec::new();
    for message in messages {
        let (other, sent) = if message.sender == Some(record) {
            (Some(message.recipient), true)
        } else {
            (message.sender, false)
        };
        let Some(name) = other
            .and_then(|other| sessions.iter().find(|s| s.id == other))
            .map(|s| s.name.clone())
        else {
            continue;
        };
        let at = match out.iter().position(|p| p.name == name) {
            Some(at) => at,
            None => {
                out.push(adroit_agents::claude::links::Peer {
                    name,
                    sent: false,
                    received: false,
                });
                out.len() - 1
            }
        };
        if sent {
            out[at].sent = true;
        } else {
            out[at].received = true;
        }
    }
    out
}

/// [`peers`] read from the record at `root`.
pub async fn peers_at(root: &Path, record: SessionId) -> Vec<adroit_agents::claude::links::Peer> {
    with_store(root, move |store| {
        Ok(peers(
            record,
            &store.sessions()?,
            &store.messages_of(record)?,
        ))
    })
    .await
    .unwrap_or_default()
}

/// What `record` sent and received, newest first, as the overview lists it.
pub fn notes(
    record: SessionId,
    sessions: &[super::Session],
    messages: &[Message],
) -> Vec<crate::details::Note> {
    let name = |id: Option<SessionId>| {
        id.and_then(|id| sessions.iter().find(|s| s.id == id))
            .map_or_else(|| "Adroit".to_owned(), |s| s.name.clone())
    };
    messages
        .iter()
        .map(|message| {
            let outgoing = message.sender == Some(record);
            crate::details::Note {
                id: message.id.to_string(),
                outgoing,
                other: if outgoing {
                    name(Some(message.recipient))
                } else {
                    name(message.sender)
                },
                kind: message.kind.key().into(),
                body: message.body.clone(),
                sent_ms: message.sent_ms,
            }
        })
        .collect()
}

/// [`notes`] read from the record at `root`.
pub async fn notes_at(root: &Path, record: SessionId) -> Vec<crate::details::Note> {
    with_store(root, move |store| {
        Ok(notes(
            record,
            &store.sessions()?,
            &store.messages_of(record)?,
        ))
    })
    .await
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::record as recorded;

    /// Adroit's record of a session called `name` that has ended.
    fn session(name: &str) -> super::super::Session {
        recorded(name).ended().build()
    }

    /// A message delivered from `from` to `to`.
    fn message(from: Option<SessionId>, to: SessionId) -> Message {
        crate::testing::message(from, to).delivered().build()
    }

    #[test]
    fn the_sessions_a_session_talked_with_are_named_once_each_newest_first_ended_ones_too() {
        let (shown, api, web) = (session("fix"), session("api"), session("web"));
        let all = [shown.clone(), api.clone(), web.clone()];
        let newest_first = [
            message(Some(web.id), shown.id),
            message(Some(shown.id), api.id),
            message(Some(api.id), shown.id),
            message(None, shown.id),
        ];
        let got = peers(shown.id, &all, &newest_first);
        let got: Vec<_> = got
            .iter()
            .map(|p| (p.name.as_str(), p.sent, p.received))
            .collect();
        assert_eq!(got, [("web", false, true), ("api", true, true)]);
    }

    #[test]
    fn a_message_is_listed_with_the_session_on_the_other_end_or_adroit_for_its_notices() {
        let (shown, api) = (session("fix"), session("api"));
        let all = [shown.clone(), api.clone()];
        let listed = notes(
            shown.id,
            &all,
            &[message(Some(shown.id), api.id), message(None, shown.id)],
        );
        let listed: Vec<_> = listed
            .iter()
            .map(|n| (n.outgoing, n.other.as_str()))
            .collect();
        assert_eq!(listed, [(true, "api"), (false, "Adroit")]);
    }

    #[tokio::test]
    async fn a_session_is_recorded_with_the_profile_it_starts_from_and_that_profile_s_connection() {
        let root = tempfile::tempdir().unwrap();
        let settings =
            Settings::parse("[[profiles]]\nid = \"mine\"\nagent = \"claude\"\n").unwrap();
        let mut request = crate::execution::StartRequest {
            agent: Agent::Claude,
            name: "fix".into(),
            cwd: "/repo".into(),
            ..Default::default()
        };

        record(root.path(), &settings, &mut request, None)
            .await
            .unwrap();

        let kept = sessions(root.path()).await.unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(
            (kept[0].profile.as_str(), kept[0].connection.as_str()),
            ("mine", "claude")
        );
    }
    #[tokio::test]
    async fn a_start_that_is_refused_records_nothing_and_runs_no_agent() {
        let root = tempfile::tempdir().unwrap();
        let ran = root.path().join("ran");
        let script = root.path().join("codex.sh");
        std::fs::write(&script, format!("touch '{}'\n", ran.display())).unwrap();
        let settings = Settings::parse(&format!(
            "[[profiles]]\nid = \"codex\"\nagent = \"codex\"\n[profiles.launch]\ncommand = [\"/bin/sh\", \"{}\"]\n\n[[profiles]]\nid = \"old\"\nagent = \"codex\"\narchived = true\n",
            script.display()
        ))
        .unwrap();
        for (profile, model, why) in [
            ("gone", None, "profile `gone` is not defined"),
            ("claude", None, "starts Claude Code, not Codex"),
            ("old", None, "profile `old` is archived"),
            ("codex", Some(" "), "the model is empty"),
        ] {
            let request = crate::execution::StartRequest {
                agent: Agent::Codex,
                profile: Some(profile.parse().unwrap()),
                model: model.map(String::from),
                name: "fix".into(),
                cwd: root.path().into(),
                ..Default::default()
            };
            let error = prepare(root.path(), &settings, request, None, Path::new("/adroit"))
                .await
                .unwrap_err();
            assert!(error.contains(why), "{error}");
        }
        assert!(sessions(root.path()).await.unwrap().is_empty());
        assert!(!ran.exists(), "Codex was asked for its instructions");
    }
}
