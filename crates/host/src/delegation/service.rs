//! The work host's side of delegation: it answers sessions on a socket,
//! starts the sessions they ask for, hands Codex its messages and watches
//! every session it keeps a record of.
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use adroit_model::{Agent, ModelCatalog, SessionId, StartupReadiness};
use adroit_term::{Daemon, tags};
use serde_json::{Value, json};
use tokio::io::BufReader;

use super::{
    Code, Error, Message, Readiness, Result, Session,
    launch::CLAUDE_CHANNEL,
    protocol::{self, Command, Request, Response, VERSION},
    store::Store,
};
use crate::settings::Settings;

type SettingsSource = Arc<dyn Fn() -> std::result::Result<Settings, String> + Send + Sync>;

/// How long a session may start without becoming ready or showing a dialog
/// before it shows as stalled.
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
        .map_err(Error::storage)?
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
            .map_err(Error::storage)?;
        let metadata = std::fs::symlink_metadata(root).map_err(Error::storage)?;
        // SAFETY: geteuid only reads the current process identity.
        if !metadata.is_dir()
            || metadata.mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(Error::storage(
                "the delegation directory must be private and owned by the current user",
            ));
        }
        let claim = File::open(root).map_err(Error::storage)?;
        claim.try_lock().map_err(Error::storage)?;
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
        let listener = crate::work_host::bind_private(&socket).map_err(Error::storage)?;
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
    protocol::write_frame(
        &mut write,
        &Response {
            version: VERSION,
            result,
        },
    )
    .await
}

async fn dispatch(context: &Context, request: Request) -> Result<Value> {
    let credential = request.credential;
    let caller = context
        .call(move |store| {
            let id = store.authenticate(&credential)?;
            store.known(id)
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
                "opened_by": parent.map(|p| json!({"id": p.id, "name": p.name, "ended": p.ended})),
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
        Command::Models => Ok(serde_json::to_value(
            context.models.lock().expect("the models' lock").view(),
        )
        .map_err(Error::storage)?),
        Command::Next => {
            let id = caller.id;
            let next = context.call(move |store| next_message(store, id)).await?;
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
    match sender {
        Some(sender) => format!(
            "[message] from {} ({}) working in {}:\n{}",
            sender.name,
            sender.id,
            sender.cwd.display(),
            message.body
        ),
        None => format!("[message] from {}:\n{}", message.sender, message.body),
    }
}

async fn start(context: &Context, caller: &Session, start: protocol::Start) -> Result<Value> {
    let prompt = start.prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err(Error::new(Code::Invalid, "the prompt is empty"));
    }
    let settings = context.settings()?;
    let running = context.call(|store| store.running_opened()).await?;
    if running >= settings.delegation.max_started {
        return Err(Error::new(
            Code::Refused,
            format!(
                "{running} sessions opened by sessions already run, and at most {} may; carry on yourself, or ask the user to end one",
                settings.delegation.max_started
            ),
        ));
    }
    // The caller's own profile is not passed on: a session starts from the
    // one it names, else from the default.
    let chosen = crate::programs::starting(&settings, start.agent, start.profile.as_ref())
        .map_err(|error| Error::new(Code::Invalid, error))?;
    let (agent, profile) = (chosen.agent(), chosen.profile);
    context
        .models
        .lock()
        .expect("the models' lock")
        .check(&profile, start.model.as_deref())
        .map_err(|error| Error::new(Code::Invalid, error))?;
    let dir = start.dir.clone().unwrap_or_else(|| caller.cwd.clone());
    let dir = std::fs::canonicalize(&dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| Error::new(Code::Invalid, format!("{} is no directory", dir.display())))?;
    let name = {
        let listed = context.daemon.sessions().await.map_err(Error::storage)?;
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
        profile: Some(profile),
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
    let palette = crate::themes::palette_in_force(&settings)
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
        let opened_by = session
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
            "opened_by": opened_by,
        }));
    }
    Ok(Value::Array(out))
}

async fn watch_models(context: Context) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // By the generation each profile is asked as, so a profile configured
    // anew is asked at once and an answer is learned for the one asked.
    let mut next: HashMap<u64, tokio::time::Instant> = HashMap::new();
    let mut asking = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            Some(Ok((generation, got))) = asking.join_next(), if !asking.is_empty() => {
                let got: Option<Vec<adroit_model::ProviderModel>> = got;
                let wait = if got.is_some() { 600 } else { 60 };
                next.insert(generation, tokio::time::Instant::now() + Duration::from_secs(wait));
                if let Some(models) = got {
                    context.models.lock().expect("the models' lock").learn(generation, models);
                }
                continue;
            }
        }
        let Ok(settings) = context.settings() else {
            continue;
        };
        let profiles = crate::programs::profiles(&settings);
        let to_ask = {
            let mut models = context.models.lock().expect("the models' lock");
            models.configure(
                profiles.iter().map(asked_of).collect(),
                settings.profile_of(settings.default_agent()).id.clone(),
            );
            models.to_ask()
        };
        let now = tokio::time::Instant::now();
        next.retain(|generation, _| to_ask.iter().any(|(_, g)| g == generation));
        for (asked, generation) in to_ask {
            let Some(started) = profiles.iter().find(|p| p.profile == asked.profile) else {
                continue;
            };
            if next.get(&generation).is_none_or(|at| now >= *at) {
                // Far ahead while asking, so a slow answer is not asked twice.
                next.insert(generation, now + Duration::from_secs(600));
                let answer = adroit_agents::adapter(started.agent())
                    .models(started.clone(), crate::paths::home());
                asking.spawn(async move { (generation, answer.await) });
            }
        }
    }
}

/// What `started`'s models are asked through, as the catalog keeps it.
fn asked_of(started: &adroit_agents::AgentContext) -> adroit_model::Asked {
    adroit_model::Asked {
        profile: started.profile.clone(),
        agent: started.agent(),
        home: started.home.clone(),
        through: format!("{:?}", (&started.home, &started.launch, &started.shell)),
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
            .map_err(|_| Error::storage("the terminal daemon did not answer"))?
            .map_err(Error::storage)?;
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
            if record.readiness != Readiness::Ready {
                let screen = context
                    .daemon
                    .visible(terminal)
                    .await
                    .map_err(Error::storage)?;
                self.starting(context, &record, native, &screen).await?;
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
        let (id, profile, home) = (record.id, record.profile.clone(), record.home.clone());
        let allowed = context
            .call(move |store| store.launch(id))
            .await?
            .and_then(|launch| crate::programs::recorded(&launch, &profile, home.as_deref()).ok())
            .is_some_and(|started| started.claude().confirm_channel_warning);
        if let Some(keys) = channel_warning_keys(record.agent, allowed, answered, screen) {
            // Tagged before pressing: a lost reply must not lead to a second press.
            context
                .daemon
                .tag(native.id, tags::tagged(CHANNEL_CONFIRMED, "1"), Vec::new())
                .await
                .map_err(Error::storage)?;
            context
                .daemon
                .press(native.id, keys)
                .await
                .map_err(Error::storage)?;
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
            let (profile, home) = (record.profile.clone(), record.home.clone());
            self.lookups.spawn(async move {
                let Ok(Some(launch)) = context.call(move |store| store.launch(id)).await else {
                    return (id, None);
                };
                let Ok(started) = crate::programs::recorded(&launch, &profile, home.as_deref())
                else {
                    return (id, None);
                };
                (id, adapter.resolve_conversation(started, cwd, title).await)
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
        let next = context.call(move |store| next_delivery(store, id)).await?;
        let Some((message, sender, started)) = next else {
            return Ok(());
        };
        self.delivering.insert(id);
        let text = envelope(&message, sender.as_ref());
        let (context, cwd) = (context.clone(), record.cwd.clone());
        self.deliveries.spawn(async move {
            let handed = crate::execution::deliver(started, cwd, conversation, text).await;
            if handed.is_ok() {
                let message = message.id;
                let _ = context.call(move |store| store.delivered(message)).await;
            }
            id
        });
        Ok(())
    }
}

/// The oldest message waiting for `id`, and its sender.
fn next_message(store: &Store, id: SessionId) -> Result<Option<(Message, Option<Session>)>> {
    let Some(message) = store.next_undelivered(id)? else {
        return Ok(None);
    };
    let sender = store.session(message.sender)?;
    Ok(Some((message, sender)))
}

/// The oldest message waiting for `id`, its sender, and the context it is
/// handed over in: that of the profile `id` was recorded as started from,
/// in the login it was recorded as running in, as the settings it was
/// started with say, whatever they say now, as another login's queue does
/// not know the session.
fn next_delivery(
    store: &mut Store,
    id: SessionId,
) -> Result<Option<(Message, Option<Session>, adroit_agents::AgentContext)>> {
    let Some((message, sender)) = next_message(store, id)? else {
        return Ok(None);
    };
    let (Some(launch), Some(recipient)) = (store.launch(id)?, store.session(id)?) else {
        return Ok(None);
    };
    let started = crate::programs::recorded(&launch, &recipient.profile, recipient.home.as_deref())
        .map_err(Error::storage)?;
    Ok(Some((message, sender, started)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{message, record};

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
    fn a_message_names_its_sender_and_where_it_works() {
        let sender = record("review").build();
        let message = message(sender.id, SessionId::new()).body("done").build();
        let read = envelope(&message, Some(&sender));
        assert!(read.starts_with("[message] from review"), "{read}");
        assert!(read.contains(&sender.id.to_string()), "{read}");
        assert!(read.ends_with("\ndone"), "{read}");
        let unknown = envelope(&message, None);
        assert!(unknown.contains(&sender.id.to_string()), "{unknown}");
    }

    #[test]
    fn a_codex_subagent_cannot_act_for_the_session_it_runs_in() {
        let codex = record("review")
            .agent(Agent::Codex)
            .conversation("thread-a")
            .build();
        assert!(from_own_conversation(&codex, Some("thread-a")).is_ok());
        assert_eq!(
            from_own_conversation(&codex, Some("thread-b"))
                .unwrap_err()
                .code,
            Code::Refused
        );
        let fresh = record("review").agent(Agent::Codex).build();
        assert!(from_own_conversation(&fresh, Some("thread-b")).is_ok());
        let claude = record("review").conversation("x").build();
        assert!(from_own_conversation(&claude, Some("y")).is_ok());
    }
    #[test]
    fn a_message_is_handed_over_in_the_login_its_recipient_started_in_after_a_restart() {
        use super::super::store::NewSession;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let launch = Settings::parse(
            "[[profiles]]\nid = \"codex\"\nagent = \"codex\"\n\n[[profiles]]\nid = \"codex-work\"\nagent = \"codex\"\nhome = \"/w/codex\"\n",
        )
        .unwrap();
        let new = |profile: &str, home: Option<&str>| NewSession {
            parent: None,
            agent: Agent::Codex,
            profile: profile.parse().unwrap(),
            home: home.map(PathBuf::from),
            name: profile.into(),
            cwd: "/repo".into(),
            launch: launch.clone(),
            started_ms: 1,
        };
        let to = {
            let mut store = Store::open(&path).unwrap();
            let (from, _) = store.register(new("codex", None)).unwrap();
            let (to, _) = store.register(new("codex-work", Some("/w/codex"))).unwrap();
            store.send(from, to, "review").unwrap();
            to
        };

        // Read again as a work host started anew does, asking only what was
        // recorded, never the settings in force.
        let mut store = Store::open(&path).unwrap();
        let (message, _, started) = next_delivery(&mut store, to).unwrap().unwrap();

        assert_eq!(message.body, "review");
        assert_eq!(started.home, Some("/w/codex".into()));
    }

    #[test]
    fn a_session_of_an_agent_its_settings_gave_no_profile_is_handed_its_messages() {
        use super::super::store::NewSession;
        let dir = tempfile::tempdir().unwrap();
        let launch = Settings::parse(
            "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n\n[[profiles]]\nid = \"claude-work\"\nagent = \"claude\"\n",
        )
        .unwrap();
        let new = |agent: Agent| {
            let started = crate::programs::starting(&launch, Some(agent), None).unwrap();
            NewSession {
                parent: None,
                agent,
                profile: started.profile,
                home: started.home,
                name: agent.key().into(),
                cwd: "/repo".into(),
                launch: launch.clone(),
                started_ms: 1,
            }
        };
        let mut store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
        let (from, _) = store.register(new(Agent::Claude)).unwrap();
        let (to, _) = store.register(new(Agent::Codex)).unwrap();
        store.send(from, to, "review").unwrap();

        let (message, _, handed) = next_delivery(&mut store, to).unwrap().unwrap();

        assert_eq!(message.body, "review");
        assert_eq!((handed.agent(), handed.home), (Agent::Codex, None));
    }
}
