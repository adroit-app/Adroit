//! The work host's side of delegation: it answers sessions on a socket,
//! starts the sessions they ask for, hands Codex its messages and watches
//! every session it keeps a record of.
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use adroit_model::{Agent, ModelCatalog, SessionId, StartupReadiness};
use adroit_term::{Daemon, tags};
use serde_json::{Value, json};
use tokio::io::{AsyncWriteExt, BufReader};

use super::{
    Code, Error, Message, MessageKind, Readiness, Result, Session,
    launch::CLAUDE_CHANNEL,
    protocol::{self, Command, Request, Response, VERSION},
    store::Store,
};
use crate::settings::Settings;

type SettingsSource = Arc<dyn Fn() -> std::result::Result<Settings, String> + Send + Sync>;

/// How long a session may start without becoming ready or showing a dialog
/// before its starter is told it may have stalled.
const STALL: Duration = Duration::from_secs(60);

/// The tag a terminal carries once its warning about Adroit's channel was
/// confirmed, so that one launch is never confirmed twice.
const CHANNEL_CONFIRMED: &str = "adroit.channel-confirmed";

#[derive(Clone)]
pub struct Context {
    store: Arc<Mutex<Store>>,
    models: Arc<Mutex<ModelCatalog>>,
    daemon: Daemon,
    program: PathBuf,
    root: PathBuf,
    settings: SettingsSource,
}

impl Context {
    async fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let mut store = store.lock().expect("the store's lock");
            store.ensure_current_schema()?;
            operation(&mut store)
        })
        .await
        .map_err(|error| Error::new(Code::Storage, error.to_string()))?
    }

    fn settings(&self) -> Result<Settings> {
        (self.settings)()
            .map(Settings::sane)
            .map_err(|error| Error::new(Code::Invalid, error))
    }
}

pub struct Service {
    tasks: Vec<tokio::task::JoinHandle<()>>,
    socket: PathBuf,
    _claim: File,
}

impl Drop for Service {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl Service {
    /// Opens the state at `root`, answers sessions on its socket and watches them.
    pub async fn start(
        root: &Path,
        daemon: Daemon,
        program: PathBuf,
        settings: impl Fn() -> std::result::Result<Settings, String> + Send + Sync + 'static,
    ) -> Result<Self> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(storage)?;
        let metadata = std::fs::symlink_metadata(root).map_err(storage)?;
        // SAFETY: geteuid only reads the current process identity.
        if !metadata.is_dir()
            || metadata.mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(storage(
                "the delegation directory must be private and owned by the current user",
            ));
        }
        let claim = File::open(root).map_err(storage)?;
        claim.try_lock().map_err(storage)?;
        let store = Store::open(&super::app::database(root))?;
        let context = Context {
            store: Arc::new(Mutex::new(store)),
            models: Arc::new(Mutex::new(ModelCatalog::default())),
            daemon,
            program,
            root: root.to_path_buf(),
            settings: Arc::new(settings),
        };
        let socket = protocol::socket(root);
        match std::fs::remove_file(&socket) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage(error)),
        }
        let listener = tokio::net::UnixListener::bind(&socket).map_err(storage)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
            .map_err(storage)?;
        let serving = context.clone();
        let tasks = vec![
            tokio::spawn(async move { serve(listener, serving).await }),
            tokio::spawn(supervise(context.clone())),
            tokio::spawn(watch_models(context)),
        ];
        Ok(Self {
            tasks,
            socket,
            _claim: claim,
        })
    }
}

async fn serve(listener: tokio::net::UnixListener, context: Context) {
    let permits = Arc::new(tokio::sync::Semaphore::new(128));
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            continue;
        };
        let context = context.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let _ = reply(stream, context).await;
        });
    }
}

async fn reply(stream: tokio::net::UnixStream, context: Context) -> Result<()> {
    let (read, mut write) = stream.into_split();
    let frame = protocol::read_frame(&mut BufReader::new(read)).await?;
    let result = match serde_json::from_slice::<Request>(&frame) {
        Ok(request) if request.version == VERSION => dispatch(&context, request).await,
        Ok(_) => Err(Error::new(Code::Invalid, "unsupported protocol version")),
        Err(error) => Err(Error::new(
            Code::Invalid,
            format!("invalid request: {error}"),
        )),
    };
    let mut bytes = serde_json::to_vec(&Response {
        version: VERSION,
        result,
    })
    .map_err(storage)?;
    bytes.push(b'\n');
    write.write_all(&bytes).await.map_err(storage)
}

async fn dispatch(context: &Context, request: Request) -> Result<Value> {
    let credential = request.credential;
    let caller = context
        .call(move |store| {
            let id = store.authenticate(&credential)?;
            store
                .session(id)?
                .ok_or_else(|| Error::new(Code::NotFound, "the session is unknown"))
        })
        .await?;
    if request.command.acts() {
        from_own_conversation(&caller, request.thread.as_deref())?;
    }
    match request.command {
        Command::Context => {
            let parent = match caller.parent {
                Some(parent) => context.call(move |store| store.session(parent)).await?,
                None => None,
            };
            Ok(json!({
                "id": caller.id,
                "name": caller.name,
                "dir": caller.cwd,
                "started_by": parent.map(|p| json!({"id": p.id, "name": p.name, "ended": p.ended})),
            }))
        }
        Command::StartSession(start) => self::start(context, &caller, start).await,
        Command::Send(send) => {
            let id = context
                .call(move |store| store.send(caller.id, send.to, &send.body))
                .await?;
            Ok(json!({"sent": id}))
        }
        Command::Sessions(filter) => {
            let all = context.call(|store| store.sessions()).await?;
            listed(all, &filter).await
        }
        Command::Report(report) => {
            let id = context
                .call(move |store| store.report(caller.id, &report.body))
                .await?;
            Ok(json!({"reported": id, "note": "this session ends once this turn is over"}))
        }
        Command::StopSession(stop) => {
            let target = context
                .call(move |store| store.session(stop.id))
                .await?
                .ok_or_else(|| Error::new(Code::NotFound, "no session has that id"))?;
            if target.parent != Some(caller.id) {
                return Err(Error::new(
                    Code::Refused,
                    "a session can end only the sessions it started",
                ));
            }
            if let Some(terminal) = target.terminal {
                context.daemon.end(terminal).await.map_err(storage)?;
            }
            context.call(move |store| store.end(target.id)).await?;
            Ok(json!({"ended": target.id}))
        }
        Command::Models => Ok(serde_json::to_value(
            context.models.lock().expect("the models' lock").view(),
        )
        .map_err(storage)?),
        Command::Next => {
            let id = caller.id;
            let next = context
                .call(move |store| {
                    let Some(message) = store.next_undelivered(id)? else {
                        return Ok(None);
                    };
                    let sender = match message.sender {
                        Some(sender) => store.session(sender)?,
                        None => None,
                    };
                    Ok(Some((message, sender)))
                })
                .await?;
            Ok(match next {
                Some((message, sender)) => json!({
                    "id": message.id,
                    "from": message.sender,
                    "content": envelope(&message, sender.as_ref()),
                }),
                None => Value::Null,
            })
        }
        Command::Delivered { id } => {
            let recipient = caller.id;
            context
                .call(move |store| {
                    let mine = store
                        .messages_of(recipient)?
                        .iter()
                        .any(|message| message.id == id && message.recipient == recipient);
                    if !mine {
                        return Err(Error::new(
                            Code::Refused,
                            "the message is not this session's",
                        ));
                    }
                    store.delivered(id)
                })
                .await?;
            Ok(Value::Null)
        }
    }
}

/// A Codex session's own conversation acts for it; its subagents share its
/// tools but must go through it.
fn from_own_conversation(caller: &Session, thread: Option<&str>) -> Result<()> {
    match (caller.agent, &caller.conversation, thread) {
        (Agent::Codex, Some(own), Some(thread)) if own != thread => Err(Error::new(
            Code::Refused,
            "only the session's own conversation can act for it; subagents return their results to it",
        )),
        _ => Ok(()),
    }
}

/// What a message reads as when it reaches its recipient's agent.
pub fn envelope(message: &Message, sender: Option<&Session>) -> String {
    match (message.kind, sender) {
        (MessageKind::Notice, _) | (_, None) => format!("[adroit] {}", message.body),
        (kind, Some(sender)) => format!(
            "[{}] from {} ({}) working in {}:\n{}",
            kind.key(),
            sender.name,
            sender.id,
            sender.cwd.display(),
            message.body
        ),
    }
}

async fn start(context: &Context, caller: &Session, start: protocol::Start) -> Result<Value> {
    if caller.parent.is_some() {
        return Err(Error::new(
            Code::Refused,
            "a session started by another session cannot start sessions; tell the session that started you how to divide the work",
        ));
    }
    let prompt = start.prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err(Error::new(Code::Invalid, "the prompt is empty"));
    }
    let settings = context.settings()?;
    let running = context.call(|store| store.running_started()).await?;
    if running >= settings.delegation.max_started {
        return Err(Error::new(
            Code::Refused,
            format!(
                "{running} started sessions already run, and at most {} may; wait for a report or end one",
                settings.delegation.max_started
            ),
        ));
    }
    let agent = {
        let models = context.models.lock().expect("the models' lock");
        let agent = start.agent.unwrap_or(settings.agent);
        models
            .check(Some(agent), start.model.as_deref())
            .map_err(|error| Error::new(Code::Invalid, error))?;
        agent
    };
    let dir = start.dir.clone().unwrap_or_else(|| caller.cwd.clone());
    let dir = std::fs::canonicalize(&dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| Error::new(Code::Invalid, format!("{} is no directory", dir.display())))?;
    let name = {
        let listed = context.daemon.sessions().await.map_err(storage)?;
        let names: Vec<String> = listed
            .iter()
            .filter_map(|s| match crate::sessions::read(s)? {
                crate::sessions::Kept::Agent(s) => Some(s.name),
                crate::sessions::Kept::Terminal(_) => None,
            })
            .collect();
        let taken: Vec<&str> = names.iter().map(String::as_str).collect();
        crate::start::name_for(start.name.as_deref(), &prompt, &taken)
    };
    let cwd = match (start.worktree, &start.base) {
        (true, base) => {
            let summary = prompt.lines().next().unwrap_or_default();
            adroit_vcs::worktree::create(&dir, &name, base.as_deref(), summary)
                .await
                .map_err(|error| Error::new(Code::Invalid, error))?
                .path
        }
        (false, Some(_)) => {
            return Err(Error::new(
                Code::Invalid,
                "a base revision needs a new worktree (worktree: true)",
            ));
        }
        (false, None) => dir,
    };
    let request = crate::execution::StartRequest {
        agent,
        model: start.model,
        name: name.clone(),
        cwd: cwd.clone(),
        prompt,
        permissions: start.permissions.unwrap_or_default(),
        env: crate::daemon::agent_env(),
        started_ms: super::now_ms(),
        ..Default::default()
    };
    let (resolved, recorded) = super::app::prepare(
        &context.root,
        &settings,
        request,
        Some(caller.id),
        &context.program,
    )
    .await
    .map_err(|error| Error::new(Code::Invalid, error))?;
    let mut spawn = resolved.spawn;
    spawn.cols = 120;
    spawn.rows = 40;
    let palette = crate::themes::palette(&settings.theme, &crate::settings::themes_dir())
        .map_err(|error| Error::new(Code::Invalid, error))?;
    let attach = recorded.clone();
    let started = crate::execution::spawn_headless_prepared(
        &context.daemon,
        spawn,
        &palette,
        move |terminal| async move { attach.attach(terminal).await },
    )
    .await;
    if let Err(error) = started {
        recorded.failed().await;
        return Err(Error::new(
            Code::Storage,
            format!("cannot start {name}: {error}"),
        ));
    }
    Ok(json!({"id": recorded.id, "name": name, "dir": cwd}))
}

/// The sessions that have not ended, narrowed as `filter` asks.
async fn listed(all: Vec<Session>, filter: &protocol::Sessions) -> Result<Value> {
    let wanted_repo = match &filter.repo {
        Some(dir) => Some(adroit_vcs::common_dir(dir).await.ok_or_else(|| {
            Error::new(
                Code::Invalid,
                format!("{} is in no repository", dir.display()),
            )
        })?),
        None => None,
    };
    let wanted_dir = filter
        .dir
        .as_ref()
        .map(|dir| std::fs::canonicalize(dir).unwrap_or(dir.clone()));
    let mut out = Vec::new();
    for session in all.iter().filter(|session| !session.ended) {
        let repository = adroit_vcs::common_dir(&session.cwd).await;
        if wanted_repo.is_some() && repository != wanted_repo {
            continue;
        }
        if let Some(dir) = &wanted_dir
            && !session.cwd.starts_with(dir)
        {
            continue;
        }
        let started_by = session
            .parent
            .and_then(|parent| all.iter().find(|s| s.id == parent))
            .map(|parent| json!({"id": parent.id, "name": parent.name}));
        out.push(json!({
            "id": session.id,
            "name": session.name,
            "agent": session.agent,
            "dir": session.cwd,
            "repository": repository,
            "readiness": session.readiness,
            "started_by": started_by,
        }));
    }
    Ok(Value::Array(out))
}

async fn watch_models(context: Context) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut next: HashMap<Agent, tokio::time::Instant> = HashMap::new();
    let mut asking = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            Some(Ok((agent, got))) = asking.join_next(), if !asking.is_empty() => {
                let got: Option<Vec<adroit_model::ProviderModel>> = got;
                let wait = if got.is_some() { 600 } else { 60 };
                next.insert(agent, tokio::time::Instant::now() + Duration::from_secs(wait));
                if let Some(models) = got {
                    context.models.lock().expect("the models' lock").learn(agent, models);
                }
                continue;
            }
        }
        let Ok(settings) = context.settings() else {
            continue;
        };
        context
            .models
            .lock()
            .expect("the models' lock")
            .set_default_agent(settings.agent);
        let now = tokio::time::Instant::now();
        for agent in Agent::ALL {
            if next.get(&agent).is_none_or(|at| now >= *at) {
                // Far ahead while asking, so a slow answer is not asked twice.
                next.insert(agent, now + Duration::from_secs(600));
                let asked = adroit_agents::adapter(agent)
                    .models(crate::programs::agents_of(&settings), crate::paths::home());
                asking.spawn(async move { (agent, asked.await) });
            }
        }
    }
}

async fn supervise(context: Context) {
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut watcher = Watcher::default();
    let mut last_error = None;
    loop {
        interval.tick().await;
        let result = watcher.tick(&context).await;
        let error = result.err().map(|error| error.to_string());
        if error.is_some() && error != last_error {
            eprintln!(
                "adroit: watching sessions: {}",
                error.as_deref().unwrap_or_default()
            );
        }
        last_error = error;
    }
}

#[derive(Default)]
struct Watcher {
    /// Codex sessions a message is being handed to, one at a time each.
    delivering: HashSet<SessionId>,
    deliveries: tokio::task::JoinSet<SessionId>,
    /// Codex conversations being looked up from a shortened title.
    lookups: tokio::task::JoinSet<(SessionId, Option<String>)>,
    looking: HashSet<SessionId>,
}

impl Watcher {
    async fn tick(&mut self, context: &Context) -> Result<()> {
        while let Some(done) = self.deliveries.try_join_next() {
            if let Ok(id) = done {
                self.delivering.remove(&id);
            }
        }
        while let Some(found) = self.lookups.try_join_next() {
            if let Ok((id, conversation)) = found {
                self.looking.remove(&id);
                if let Some(conversation) = conversation {
                    context
                        .call(move |store| store.learn_conversation(id, &conversation))
                        .await?;
                }
            }
        }
        let native = tokio::time::timeout(Duration::from_secs(3), context.daemon.sessions())
            .await
            .map_err(|_| storage("the terminal daemon did not answer"))?
            .map_err(storage)?;
        let records = context.call(|store| store.sessions()).await?;
        for record in records.into_iter().filter(|record| !record.ended) {
            let Some(terminal) = record.terminal else {
                continue;
            };
            let Some(native) = native.iter().find(|native| native.id == terminal) else {
                let id = record.id;
                context.call(move |store| store.end(id)).await?;
                continue;
            };
            if native.exited {
                let id = record.id;
                context.call(move |store| store.end(id)).await?;
                continue;
            }
            if record.readiness != Readiness::Ready || record.reported {
                let screen = context.daemon.visible(terminal).await.map_err(storage)?;
                if record.readiness != Readiness::Ready {
                    self.starting(context, &record, native, &screen).await?;
                }
                if record.reported && turn_over(record.agent, native.status.as_ref(), &screen) {
                    context.daemon.end(terminal).await.map_err(storage)?;
                    let id = record.id;
                    context.call(move |store| store.end(id)).await?;
                    continue;
                }
            }
            if record.agent == Agent::Codex {
                self.follow_conversation(context, &record, native);
                if record.readiness == Readiness::Ready {
                    self.deliver(context, &record).await?;
                }
            }
        }
        Ok(())
    }

    /// Confirms the warning about Adroit's channel when the session's launch
    /// allows it, and records how the session stands while it starts.
    async fn starting(
        &mut self,
        context: &Context,
        record: &Session,
        native: &adroit_term::Session,
        screen: &[String],
    ) -> Result<()> {
        let answered = tags::value(&native.tags, CHANNEL_CONFIRMED).is_some();
        let id = record.id;
        let allowed = context
            .call(move |store| store.launch(id))
            .await?
            .is_some_and(|launch| launch.sessions.confirm_channel_warning);
        if let Some(keys) = channel_warning_keys(record.agent, allowed, answered, screen) {
            // Tagged before pressing: a lost reply must not lead to a second press.
            context
                .daemon
                .tag(native.id, tags::tagged(CHANNEL_CONFIRMED, "1"), Vec::new())
                .await
                .map_err(storage)?;
            context
                .daemon
                .press(native.id, keys)
                .await
                .map_err(storage)?;
            return Ok(());
        }
        let seen = adroit_agents::adapter(record.agent).startup_readiness(screen);
        let elapsed = Duration::from_millis(super::now_ms().saturating_sub(record.started_ms));
        if let Some(readiness) = readiness(seen, elapsed) {
            context
                .call(move |store| store.set_readiness(id, readiness))
                .await?;
        }
        Ok(())
    }

    fn follow_conversation(
        &mut self,
        context: &Context,
        record: &Session,
        native: &adroit_term::Session,
    ) {
        let adapter = adroit_agents::adapter(record.agent);
        let Some(title) = native.title.as_deref() else {
            return;
        };
        if record
            .conversation
            .as_deref()
            .is_some_and(|known| adapter.conversation_matches_title(title, known))
        {
            return;
        }
        let id = record.id;
        if let Some(conversation) = adapter.conversation_title(title) {
            let store = context.store.clone();
            // Learned at once: the next delivery must go to this conversation.
            if let Ok(mut store) = store.lock() {
                let _ = store.learn_conversation(id, &conversation);
            }
            return;
        }
        if adapter.conversation_title_needs_lookup(title) && self.looking.insert(id) {
            let (context, title, cwd) = (context.clone(), title.to_owned(), record.cwd.clone());
            self.lookups.spawn(async move {
                let Ok(Some(launch)) = context.call(move |store| store.launch(id)).await else {
                    return (id, None);
                };
                (
                    id,
                    adapter
                        .resolve_conversation(crate::programs::agents_of(&launch), cwd, title)
                        .await,
                )
            });
        }
    }

    /// Hands a Codex session its oldest waiting message through Codex's own queue.
    async fn deliver(&mut self, context: &Context, record: &Session) -> Result<()> {
        let Some(conversation) = record.conversation.clone() else {
            return Ok(());
        };
        if self.delivering.contains(&record.id) {
            return Ok(());
        }
        let id = record.id;
        let next = context
            .call(move |store| {
                let Some(message) = store.next_undelivered(id)? else {
                    return Ok(None);
                };
                let sender = match message.sender {
                    Some(sender) => store.session(sender)?,
                    None => None,
                };
                let launch = store.launch(id)?;
                Ok(launch.map(|launch| (message, sender, launch)))
            })
            .await?;
        let Some((message, sender, launch)) = next else {
            return Ok(());
        };
        self.delivering.insert(id);
        let text = envelope(&message, sender.as_ref());
        let (context, cwd) = (context.clone(), record.cwd.clone());
        self.deliveries.spawn(async move {
            let handed =
                crate::execution::deliver(Agent::Codex, launch, cwd, conversation, text).await;
            if handed.is_ok() {
                let message = message.id;
                let _ = context.call(move |store| store.delivered(message)).await;
            }
            id
        });
        Ok(())
    }
}

/// How a starting session stands, from what its adapter read on its screen
/// and how long it has been starting; `None` while that tells nothing yet.
fn readiness(seen: Option<StartupReadiness>, elapsed: Duration) -> Option<Readiness> {
    match seen {
        Some(StartupReadiness::Ready) => Some(Readiness::Ready),
        Some(
            StartupReadiness::AwaitingTrust
            | StartupReadiness::AwaitingPermission
            | StartupReadiness::AwaitingInput,
        ) => Some(Readiness::Waiting),
        Some(StartupReadiness::Stalled) => Some(Readiness::Stalled),
        Some(StartupReadiness::Spawned) | None => (elapsed >= STALL).then_some(Readiness::Stalled),
    }
}

/// Whether the turn a session reported in is over, by what its program says
/// of itself, or else what its screen shows.
fn turn_over(agent: Agent, status: Option<&adroit_term::Status>, screen: &[String]) -> bool {
    let state = match status {
        Some(status) => adroit_model::State::reported(&status.state, status.kind.as_deref()),
        None => match adroit_agents::adapter(agent).screen_state(screen) {
            Some(state) => state,
            None => return false,
        },
    };
    !matches!(
        state,
        adroit_model::State::Working | adroit_model::State::Starting
    )
}

/// The keys that confirm Claude Code's warning about loading development
/// channels on `screen`, when `allowed`, not yet `answered` for this launch,
/// and the warning names Adroit's own channel and nothing else.
fn channel_warning_keys(
    agent: Agent,
    allowed: bool,
    answered: bool,
    screen: &[String],
) -> Option<&'static [u8]> {
    let ours = || {
        adroit_agents::claude::dialog::development_channels(screen)
            .is_some_and(|channels| channels == [CLAUDE_CHANNEL])
    };
    (agent == Agent::Claude && allowed && !answered && ours()).then_some(b"\r")
}

fn storage(error: impl std::fmt::Display) -> Error {
    Error::new(Code::Storage, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude_screen(name: &str) -> Vec<String> {
        let path = format!(
            "{}/../agents/tests/screens/{name}.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .map(String::from)
            .collect()
    }

    fn session(agent: Agent, conversation: Option<&str>) -> Session {
        Session {
            id: SessionId::new(),
            parent: None,
            agent,
            name: "review".into(),
            cwd: "/repo".into(),
            terminal: None,
            conversation: conversation.map(String::from),
            readiness: Readiness::Ready,
            started_ms: 0,
            reported: false,
            ended: false,
        }
    }

    #[test]
    fn a_dialog_waits_on_the_user_at_once_and_a_silent_screen_stalls_only_after_a_while() {
        assert_eq!(
            readiness(Some(StartupReadiness::AwaitingTrust), Duration::ZERO),
            Some(Readiness::Waiting)
        );
        assert_eq!(readiness(None, STALL - Duration::from_millis(1)), None);
        assert_eq!(readiness(None, STALL), Some(Readiness::Stalled));
        assert_eq!(
            readiness(Some(StartupReadiness::Ready), STALL * 10),
            Some(Readiness::Ready)
        );
    }

    #[test]
    fn a_reporting_session_ends_only_once_its_turn_is_over() {
        let status = |state: &str| adroit_term::Status {
            state: state.into(),
            kind: None,
            msg: None,
        };
        assert!(!turn_over(Agent::Claude, Some(&status("working")), &[]));
        assert!(turn_over(Agent::Claude, Some(&status("idle")), &[]));
        assert!(turn_over(Agent::Claude, Some(&status("done")), &[]));
        assert!(!turn_over(Agent::Claude, None, &[]));
    }

    #[test]
    fn adroit_s_channel_warning_is_left_to_the_user_unless_the_settings_allow_confirming_it() {
        let warning = claude_screen("dev-channels");
        assert_eq!(
            channel_warning_keys(Agent::Claude, false, false, &warning),
            None
        );
        assert_eq!(
            channel_warning_keys(Agent::Claude, true, false, &warning),
            Some(&b"\r"[..])
        );
        assert_eq!(
            channel_warning_keys(Agent::Claude, true, true, &warning),
            None
        );
    }

    #[test]
    fn no_screen_but_the_warning_naming_adroit_s_channel_alone_is_confirmed() {
        let other = claude_screen("dev-channels")
            .into_iter()
            .map(|line| {
                line.replace(
                    "Channels: server:adroit",
                    "Channels: server:adroit, plugin:other@market",
                )
            })
            .collect::<Vec<_>>();
        for screen in [
            other,
            claude_screen("dev-channels-confirmed"),
            claude_screen("permissions"),
        ] {
            assert_eq!(
                channel_warning_keys(Agent::Claude, true, false, &screen),
                None
            );
        }
        assert_eq!(
            channel_warning_keys(Agent::Codex, true, false, &claude_screen("dev-channels")),
            None
        );
    }

    #[test]
    fn a_message_names_its_sender_and_adroit_s_own_word_names_adroit() {
        let sender = session(Agent::Claude, None);
        let message = Message {
            id: adroit_model::MessageId::new(),
            sender: Some(sender.id),
            recipient: SessionId::new(),
            kind: MessageKind::Report,
            body: "done".into(),
            sent_ms: 0,
            delivered: false,
        };
        let read = envelope(&message, Some(&sender));
        assert!(read.starts_with("[report] from review"), "{read}");
        assert!(read.contains(&sender.id.to_string()), "{read}");
        assert!(read.ends_with("\ndone"), "{read}");
        let notice = Message {
            sender: None,
            kind: MessageKind::Notice,
            ..message
        };
        assert_eq!(envelope(&notice, None), "[adroit] done");
    }

    #[test]
    fn a_codex_subagent_cannot_act_for_the_session_it_runs_in() {
        let codex = session(Agent::Codex, Some("thread-a"));
        assert!(from_own_conversation(&codex, Some("thread-a")).is_ok());
        assert_eq!(
            from_own_conversation(&codex, Some("thread-b"))
                .unwrap_err()
                .code,
            Code::Refused
        );
        assert!(from_own_conversation(&session(Agent::Codex, None), Some("thread-b")).is_ok());
        assert!(from_own_conversation(&session(Agent::Claude, Some("x")), Some("y")).is_ok());
    }
}
