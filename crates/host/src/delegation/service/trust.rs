//! Claude Code's question whether to trust the folder a session starts in,
//! which it asks before it runs any hook: read from the session's screen, and
//! answered there by the keys a user would type.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adroit_agents::claude::dialog::{TrustAsked, blankless, trust_asked};
use adroit_model::{Agent, MessageId, SessionId};
use adroit_term::{Daemon, Input};
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::delegation::Session;
use crate::delegation::questions::{self, Asking, Decision, TRUST};

/// What Claude Code reports while it asks.
const REPORTED: &str = "trust this folder";
/// How long an answer may take, less than the one who gave it is kept waiting.
const ANSWERS: Duration = Duration::from_secs(4);
/// What is told of an answer Claude Code was not seen to take in that time.
const UNSEEN: &str = "the agent was not seen to take the answer; look at the session";
const LOOK_EVERY: Duration = Duration::from_millis(50);

/// The question `record`'s session asks in `native` on `screen`, when both
/// what Claude Code reports and what it shows say so, of the folder the
/// session was started in.
fn asked(record: &Session, native: &adroit_term::Session, screen: &[String]) -> Option<TrustAsked> {
    if record.agent != Agent::Claude || !reports(native) {
        return None;
    }
    let folder = folder(record);
    trust_asked(screen).filter(|asked| asked.folder == blankless(&folder.display().to_string()))
}

/// Whether the program in `native` reports that it asks.
fn reports(native: &adroit_term::Session) -> bool {
    native
        .status
        .as_ref()
        .is_some_and(|s| s.state == "blocked" && s.msg.as_deref() == Some(REPORTED))
}

/// The folder the session runs in, as Claude Code names it.
fn folder(record: &Session) -> PathBuf {
    adroit_sys::fs::canonicalize(&record.cwd).unwrap_or_else(|_| record.cwd.clone())
}

/// The id of what `asked` asks in `terminal`, the same whenever it is read.
fn id(terminal: u128, asked: &TrustAsked) -> MessageId {
    let told = format!("{terminal:032x}\n{}", blankless(&asked.text));
    let digest = ring::digest::digest(&ring::digest::SHA256, told.as_bytes());
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_ref()[..16]);
    format!("{:032x}", u128::from_be_bytes(bytes))
        .parse()
        .expect("thirty-two hexadecimal digits")
}

/// What trusting `folder`, of `repository` if any, lets Claude Code do, and
/// for how long.
fn scope(folder: &Path, home: &Path, repository: Option<&Path>) -> String {
    if folder == home {
        return "Trusts the home folder for this session only.".into();
    }
    let reach = match repository {
        Some(repository) => format!(
            "the whole repository of {}, its worktrees and every folder below them",
            repository.display()
        ),
        None => "this folder and every folder below it".into(),
    };
    format!(
        "Claude Code keeps this trust for later sessions too; it covers {reach}, and no folder above."
    )
}

/// The `.git` over `folder`, for a repository git itself tells nothing of, as
/// of one another user owns: Claude Code trusts it whole all the same.
fn marked(folder: &Path) -> Option<PathBuf> {
    folder
        .ancestors()
        .map(|above| above.join(".git"))
        .find(|git| git.exists())
}

async fn input(folder: &Path, asked: &TrustAsked) -> Value {
    let repository = match adroit_vcs::common_dir(folder).await {
        Some(repository) => Some(repository),
        None => marked(folder),
    };
    let home = adroit_model::paths::home();
    let home = adroit_sys::fs::canonicalize(&home).unwrap_or(home);
    json!({
        "folder": folder,
        "repository": repository,
        "scope": scope(folder, &home, repository.as_deref()),
        "dialog": asked.text,
    })
}

/// A question held for as long as its session asks it.
pub(super) struct Held {
    asked: TrustAsked,
    /// Dropped to let go of the question.
    _asks: oneshot::Sender<()>,
    /// Whether it was decided, and is being answered.
    answering: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Held {
    /// Whether it is being answered, which nothing then keeps from its end.
    pub(super) fn answering(&self) -> bool {
        self.answering.load(Ordering::SeqCst) && !self.task.is_finished()
    }
}

/// Holds what `record`'s session asks in `native` on `screen`: `held` while
/// it is still that or is being answered, one anew when it asks otherwise,
/// none when it asks nothing.
pub(super) fn follow(
    held: Option<Held>,
    daemon: &Daemon,
    record: &Session,
    native: &adroit_term::Session,
    screen: &[String],
) -> Option<Held> {
    if held.as_ref().is_some_and(Held::answering) {
        return held;
    }
    let asked = asked(record, native, screen)?;
    if let Some(held) = held
        && held.asked.same(&asked)
    {
        // What was read may be from before it was answered: one that is still
        // asked is held anew by the next look.
        return (!held.task.is_finished()).then_some(held);
    }
    let (asks, gone) = oneshot::channel();
    let terminal = Terminal {
        daemon: daemon.clone(),
        id: native.id,
    };
    let answering = Arc::new(AtomicBool::new(false));
    let task = crate::environment::spawn(hold(
        terminal,
        record.id,
        folder(record),
        asked.clone(),
        answering.clone(),
        gone,
    ));
    Some(Held {
        asked,
        _asks: asks,
        answering,
        task,
    })
}

/// Holds what `asked` asks of `folder` until it is decided, and answers it so,
/// unless it is `gone` first.
async fn hold(
    terminal: Terminal,
    session: SessionId,
    folder: PathBuf,
    asked: TrustAsked,
    answering: Arc<AtomicBool>,
    gone: oneshot::Receiver<()>,
) {
    let decided = async {
        let asking = Asking {
            id: id(terminal.id, &asked),
            tool: TRUST.into(),
            input: input(&folder, &asked).await,
            dir: Some(folder.display().to_string()),
        };
        let (waits, decided) = questions::hold(session, asking, crate::delegation::now_ms())?;
        Some((waits, decided.await.ok()?))
    };
    let decided = tokio::select! {
        _ = gone => return,
        decided = decided => decided,
    };
    let Some((waits, (decision, taken))) = decided else {
        return;
    };
    answering.store(true, Ordering::SeqCst);
    let trust = matches!(decision, Decision::Allow { .. });
    let done = match tokio::time::timeout(ANSWERS, answer(&terminal, &asked, trust)).await {
        Ok(done) => done,
        Err(_) => Err(Unanswered::Failed(UNSEEN.into())),
    };
    drop(waits);
    match done {
        // Nothing was chosen, so it is told as asked no more.
        Err(Unanswered::Gone) => {}
        Err(Unanswered::Failed(why)) => drop(taken.send(Err(why))),
        Ok(()) => drop(taken.send(Ok(()))),
    }
}

/// The screen a question is answered on.
trait Screen: Send + Sync {
    /// Whether Claude Code still reports that it asks; not once it has ended.
    fn reports(&self) -> impl Future<Output = Result<bool, String>> + Send;
    fn shows(&self) -> impl Future<Output = Result<Vec<String>, String>> + Send;
    fn press(&self, key: &'static str) -> impl Future<Output = Result<(), String>> + Send;
}

struct Terminal {
    daemon: Daemon,
    id: u128,
}

impl Screen for Terminal {
    async fn reports(&self) -> Result<bool, String> {
        let sessions = self.daemon.sessions().await.map_err(|e| e.to_string())?;
        Ok(sessions
            .iter()
            .any(|s| s.id == self.id && !s.exited && reports(s)))
    }

    async fn shows(&self) -> Result<Vec<String>, String> {
        self.daemon
            .visible(self.id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn press(&self, key: &'static str) -> Result<(), String> {
        self.daemon
            .input(self.id, &[Input::Key(adroit_model::Key::new(key))])
            .await
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Unanswered {
    /// The question is not asked any more; nothing was chosen.
    Gone,
    Failed(String),
}

/// Chooses to `trust` the folder or to leave, each key typed only while the
/// screen still shows what `asked` asked.
async fn answer(screen: &impl Screen, asked: &TrustAsked, trust: bool) -> Result<(), Unanswered> {
    let shown = shown(screen, asked)
        .await
        .map_err(Unanswered::Failed)?
        .ok_or(Unanswered::Gone)?;
    if shown.trusting != trust {
        let key = match trust == shown.trust_below {
            true => "ArrowDown",
            false => "ArrowUp",
        };
        screen.press(key).await.map_err(Unanswered::Failed)?;
        selected(screen, asked, trust).await?;
    }
    screen.press("Enter").await.map_err(Unanswered::Failed)?;
    // Only what Claude Code reports tells: a screen can fail to show the
    // question, as a short one does, while it is still asked.
    loop {
        if screen.reports().await == Ok(false) {
            return Ok(());
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

/// Waits until the option to `trust`, or the other, is the one selected.
async fn selected(screen: &impl Screen, asked: &TrustAsked, trust: bool) -> Result<(), Unanswered> {
    loop {
        if let Ok(Some(shown)) = shown(screen, asked).await
            && shown.trusting == trust
        {
            return Ok(());
        }
        if screen.reports().await == Ok(false) {
            return Err(Unanswered::Gone);
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

/// What `asked` asks as the screen shows it now; none once it does not.
async fn shown(screen: &impl Screen, asked: &TrustAsked) -> Result<Option<TrustAsked>, String> {
    let rows = screen.shows().await?;
    Ok(trust_asked(&rows).filter(|shown| shown.same(asked)))
}

#[cfg(test)]
mod tests;
