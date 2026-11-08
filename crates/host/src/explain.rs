//! `adroit session explain`: where the state a session shows came from, as
//! plain text to paste into a report or as JSON.

use std::time::Duration;

use adroit_agents::{Judgement, Verdict};
use adroit_model::{Agent, StartupReadiness, State};
use serde_json::{Value, json};

use crate::watch::hold::Shown;

pub(crate) const USAGE: &str = "usage: adroit session explain <session> [--json] | adroit session explain --file <path> --agent <claude|codex|pi|opencode> [--json]";

/// How many of the screen's last rows an explanation shows.
const TAIL: usize = 20;

/// What `adroit session explain` was asked for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Asked {
    /// A running session, by its id, its record's id or its name.
    Session { session: String, json: bool },
    /// A screen kept in a file, read as `agent`'s.
    File {
        path: std::path::PathBuf,
        agent: Agent,
        json: bool,
    },
}

pub(crate) fn parse(args: &[String]) -> Option<Asked> {
    let (mut session, mut path, mut agent, mut json) = (None, None, None, false);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--file" if path.is_none() => path = Some(rest.next()?.into()),
            "--agent" if agent.is_none() => agent = Some(Agent::parse(rest.next()?)?),
            word if !word.starts_with('-') && session.is_none() => session = Some(word.to_string()),
            _ => return None,
        }
    }
    match (session, path, agent) {
        (Some(session), None, None) => Some(Asked::Session { session, json }),
        (None, Some(path), Some(agent)) => Some(Asked::File { path, agent, json }),
        _ => None,
    }
}

/// What the program of a session reported of itself by OSC 7501.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reported {
    pub state: String,
    pub kind: Option<String>,
    pub msg: Option<String>,
}

/// How a session just started stands, as the work host records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Start {
    pub recorded: crate::delegation::Readiness,
    /// How long it may go on starting before it shows as stalled; none once
    /// it no longer starts.
    pub stalls_in: Option<Duration>,
}

/// Where the state a session shows came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    /// What its program reported by OSC 7501.
    Reported,
    /// What its screen shows.
    Screen,
    /// Neither yet: it shows as starting.
    Startup,
}

impl Source {
    fn key(self) -> &'static str {
        match self {
            Source::Reported => "osc7501",
            Source::Screen => "screen",
            Source::Startup => "startup",
        }
    }
}

/// Everything an explanation says of one session or one screen.
#[derive(Debug, Clone)]
pub(crate) struct Explained {
    /// The session's name and id; none for a screen kept in a file.
    pub session: Option<(String, String)>,
    pub agent: Agent,
    pub reported: Option<Reported>,
    /// How the screen reads; none when the screen was not read, as while the
    /// program reports its state.
    pub screen: Option<Judgement<State>>,
    /// What the screen was last shown as by the reads that list sessions.
    pub shown: Option<Shown>,
    pub startup: Judgement<StartupReadiness>,
    pub start: Option<Start>,
    pub rules: RulesUsed,
    pub tail: Vec<String>,
}

/// The rules that read the agent's screen: the user's file they came from,
/// none for the bundled ones, and why a file of the user's was set aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RulesUsed {
    pub from: Option<std::path::PathBuf>,
    pub warning: Option<String>,
}

impl RulesUsed {
    fn of(agent: Agent) -> Self {
        let in_force = adroit_agents::rules::in_force(agent);
        Self {
            from: in_force.from.clone(),
            warning: in_force.warning.clone(),
        }
    }
}

impl Explained {
    /// The state the session shows and where it came from.
    pub(crate) fn state(&self) -> (State, Source) {
        if let Some(reported) = &self.reported {
            return (
                State::reported(&reported.state, reported.kind.as_deref()),
                Source::Reported,
            );
        }
        let read = self
            .shown
            .as_ref()
            .map(|shown| shown.state.clone())
            .or_else(|| self.screen.as_ref().and_then(|s| s.value.clone()));
        match read {
            Some(state) => (state, Source::Screen),
            None => (State::Starting, Source::Startup),
        }
    }

    pub(crate) fn text(&self) -> String {
        let mut out = Vec::new();
        if let Some((name, id)) = &self.session {
            out.push(format!("session: {name} ({id})"));
        }
        out.push(format!("agent: {}", self.agent.key()));
        if self.session.is_some() {
            let (state, source) = self.state();
            out.push(format!("state: {} (from {})", state.word(), source.key()));
            out.push(match &self.reported {
                Some(r) => format!(
                    "reported: state={} kind={} msg={}",
                    r.state,
                    r.kind.as_deref().unwrap_or("-"),
                    r.msg.as_deref().unwrap_or("-")
                ),
                None => "reported: nothing".into(),
            });
        }
        match &self.screen {
            Some(read) => {
                let told = read.value.as_ref().map_or("nothing", State::word);
                out.push(format!("screen: {told} {}", verdict_text(&read.verdict)));
                out.extend(rows(&read.verdict));
            }
            None => out.push("screen: not read while the program reports its state".into()),
        }
        if self.shown.as_ref().is_some_and(|shown| shown.held) {
            out.push(
                "held: working until the next read shows idle again, as this idle shows no sign of its own"
                    .into(),
            );
        }
        let told = self.startup.value.map_or("nothing", readiness_key);
        out.push(format!(
            "startup: {told} {}",
            verdict_text(&self.startup.verdict)
        ));
        out.extend(rows(&self.startup.verdict));
        if let Some(start) = &self.start {
            let stalls = match start.stalls_in {
                Some(left) => format!(", stalled in {}s", left.as_secs()),
                None => String::new(),
            };
            out.push(format!("recorded: {}{stalls}", start.recorded.key()));
        }
        out.push(match &self.rules.from {
            Some(path) => format!("rules: {}", path.display()),
            None => "rules: bundled".into(),
        });
        if let Some(warning) = &self.rules.warning {
            out.push(format!("rules warning: {warning}"));
        }
        out.push(format!("screen tail ({} rows):", self.tail.len()));
        out.extend(self.tail.iter().map(|row| format!("  | {row}")));
        out.join("\n")
    }

    pub(crate) fn json(&self) -> Value {
        let (state, source) = self.state();
        json!({
            "session": self.session.as_ref().map(|(name, id)| json!({"name": name, "id": id})),
            "agent": self.agent.key(),
            "state": self.session.as_ref().map(|_| state.word()),
            "source": self.session.as_ref().map(|_| source.key()),
            "reported": self.reported.as_ref().map(|r| json!({
                "state": r.state, "kind": r.kind, "msg": r.msg,
            })),
            "screen": self.screen.as_ref().map(|read| json!({
                "state": read.value.as_ref().map(State::word),
                "verdict": verdict_json(&read.verdict),
            })),
            "held": self.shown.as_ref().is_some_and(|shown| shown.held),
            "startup": {
                "readiness": self.startup.value.map(readiness_key),
                "verdict": verdict_json(&self.startup.verdict),
            },
            "recorded": self.start.as_ref().map(|start| json!({
                "readiness": start.recorded.key(),
                "stalls_in_secs": start.stalls_in.map(|left| left.as_secs()),
            })),
            "rules": {
                "from": self.rules.from,
                "warning": self.rules.warning,
            },
            "tail": self.tail,
        })
    }
}

fn verdict_text(verdict: &Verdict) -> String {
    match verdict.default {
        true => format!("by default {} (no rule's sign shown)", verdict.rule),
        false => format!("by rule {}", verdict.rule),
    }
}

fn rows(verdict: &Verdict) -> impl Iterator<Item = String> + '_ {
    verdict
        .evidence
        .iter()
        .map(|e| format!("  row {}: {}", e.row, e.text))
}

fn verdict_json(verdict: &Verdict) -> Value {
    json!({
        "rule": verdict.rule,
        "default": verdict.default,
        "evidence": verdict.evidence.iter().map(|e| json!({"row": e.row, "text": e.text})).collect::<Vec<_>>(),
    })
}

fn readiness_key(readiness: StartupReadiness) -> &'static str {
    match readiness {
        StartupReadiness::Spawned => "spawned",
        StartupReadiness::Ready => "ready",
        StartupReadiness::AwaitingTrust => "awaiting_trust",
        StartupReadiness::AwaitingPermission => "awaiting_permission",
        StartupReadiness::AwaitingInput => "awaiting_input",
        StartupReadiness::Stalled => "stalled",
    }
}

/// The last rows of `screen` that show something, as many as an
/// explanation shows.
pub(crate) fn tail(screen: &[String]) -> Vec<String> {
    let end = screen
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |at| at + 1);
    screen[end.saturating_sub(TAIL)..end]
        .iter()
        .map(|row| row.trim_end().to_string())
        .collect()
}

/// `screen` read as `agent`'s, as a screen kept in a file is.
pub(crate) fn of_screen(agent: Agent, screen: &[String]) -> Explained {
    let adapter = adroit_agents::adapter(agent);
    Explained {
        session: None,
        agent,
        reported: None,
        screen: Some(adapter.judge_state(screen)),
        shown: None,
        startup: adapter.judge_startup(screen),
        start: None,
        rules: RulesUsed::of(agent),
        tail: tail(screen),
    }
}

/// What `explained` prints, as `json` asks.
pub(crate) fn printed(explained: &Explained, json: bool) -> String {
    match json {
        true => explained.json().to_string(),
        false => explained.text(),
    }
}

/// The session of `sessions` that `asked` names: by its id in the terminal
/// host, the id of its record, or a name no other session has.
pub(crate) fn named<'a>(
    sessions: &'a [adroit_term::Session],
    asked: &str,
) -> Result<&'a adroit_term::Session, String> {
    let read = |s: &adroit_term::Session| match crate::sessions::read(s) {
        Some(crate::sessions::Kept::Agent(c)) => Some(c),
        _ => None,
    };
    let by_id = sessions.iter().find(|s| {
        read(s)
            .is_some_and(|c| c.id == asked || c.delegation.is_some_and(|d| d.to_string() == asked))
    });
    if let Some(found) = by_id {
        return Ok(found);
    }
    let by_name: Vec<_> = sessions
        .iter()
        .filter(|s| read(s).is_some_and(|c| c.name == asked))
        .collect();
    match by_name.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no session is named {asked}")),
        _ => Err(format!("several sessions are named {asked}; give its id")),
    }
}

/// How long a session that has been starting for `elapsed` may go on before
/// it shows as stalled.
pub(crate) fn stalls_in(
    recorded: crate::delegation::Readiness,
    elapsed: Duration,
) -> Option<Duration> {
    (recorded == crate::delegation::Readiness::Starting)
        .then(|| crate::delegation::service::STALL.saturating_sub(elapsed))
}

/// The session `asked` names as the work host sees it now.
async fn of_session(asked: &str) -> Result<Explained, String> {
    let daemon = crate::daemon::daemon();
    let sessions = daemon.sessions().await.map_err(|e| e.to_string())?;
    let native = named(&sessions, asked)?;
    let Some(crate::sessions::Kept::Agent(c)) = crate::sessions::read(native) else {
        return Err(format!("{asked} is no agent's session"));
    };
    let adapter = adroit_agents::adapter(c.agent);
    let screen = daemon.visible(native.id).await.map_err(|e| e.to_string())?;
    let read = native.status.is_none() && adapter.capabilities().screen_state;
    let start = match c.delegation {
        Some(id) => crate::delegation::app::sessions(&crate::delegation::app::root())
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|record| record.id == id)
            .map(|record| Start {
                recorded: record.readiness,
                stalls_in: stalls_in(
                    record.readiness,
                    Duration::from_millis(
                        adroit_model::time::now_ms().saturating_sub(record.started_ms),
                    ),
                ),
            }),
        None => None,
    };
    Ok(Explained {
        session: Some((c.name.clone(), c.id.clone())),
        agent: c.agent,
        reported: native.status.as_ref().map(|s| Reported {
            state: s.state.clone(),
            kind: s.kind.clone(),
            msg: s.msg.clone(),
        }),
        screen: read.then(|| adapter.judge_state(&screen)),
        shown: crate::watch::hold::shown(native.id).filter(|_| read),
        startup: adapter.judge_startup(&screen),
        start,
        rules: RulesUsed::of(c.agent),
        tail: tail(&screen),
    })
}

/// `adroit session explain <args>` as the work host runs it.
pub(crate) async fn run(args: &[String]) -> crate::command::Output {
    use crate::command::Output;
    let explained = match parse(args) {
        Some(Asked::Session { session, json }) => {
            of_session(&session).await.map(|e| printed(&e, json))
        }
        Some(Asked::File { .. }) => Err("a screen kept in a file is read where adroit runs".into()),
        None => {
            return Output {
                wrote: Err(USAGE.into()),
                code: 2,
                effects_possible: false,
            };
        }
    };
    let code = i32::from(explained.is_err());
    Output {
        wrote: explained,
        code,
        effects_possible: false,
    }
}

/// `adroit session explain --file`, read where adroit runs: what it prints
/// or why it cannot, and its exit code; none when asked for a session.
pub(crate) fn of_file(args: &[String]) -> Option<(Result<String, String>, i32)> {
    let Some(Asked::File { path, agent, json }) = parse(args) else {
        return None;
    };
    Some(match std::fs::read_to_string(&path) {
        Ok(text) => {
            let screen: Vec<String> = text.lines().map(String::from).collect();
            (Ok(printed(&of_screen(agent, &screen), json)), 0)
        }
        Err(e) => (Err(format!("cannot read {}: {e}", path.display())), 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delegation::Readiness;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    fn recorded(name: &str) -> Vec<String> {
        let path = format!(
            "{}/../agents/tests/screens/{name}.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn a_session_is_asked_by_one_word_and_a_kept_screen_by_its_file_and_agent() {
        assert_eq!(
            parse(&args(&["fix", "--json"])),
            Some(Asked::Session {
                session: "fix".into(),
                json: true
            })
        );
        assert_eq!(
            parse(&args(&["--file", "s.txt", "--agent", "codex"])),
            Some(Asked::File {
                path: "s.txt".into(),
                agent: Agent::Codex,
                json: false
            })
        );
        for wrong in [
            &[][..],
            &["a", "b"],
            &["--file", "s.txt"],
            &["--agent", "claude"],
            &["fix", "--file", "s.txt", "--agent", "claude"],
            &["--file", "s.txt", "--agent", "vim"],
            &["--loud", "fix"],
        ] {
            assert_eq!(parse(&args(wrong)), None, "{wrong:?}");
        }
    }

    fn explained(reported: Option<Reported>, shown: Option<Shown>) -> Explained {
        let screen = recorded("working");
        Explained {
            session: Some(("fix".into(), "00ab".into())),
            reported,
            shown,
            start: Some(Start {
                recorded: Readiness::Ready,
                stalls_in: None,
            }),
            ..of_screen(Agent::Claude, &screen)
        }
    }

    #[test]
    fn a_reported_state_wins_over_the_screen_and_a_read_one_over_starting() {
        let reported = Reported {
            state: "blocked".into(),
            kind: Some("permission".into()),
            msg: Some("Bash(rm)".into()),
        };
        assert_eq!(
            explained(Some(reported), None).state(),
            (
                State::Waiting(adroit_model::Wait::Permission),
                Source::Reported
            )
        );
        assert_eq!(
            explained(None, None).state(),
            (State::Working, Source::Screen)
        );
        let mut starting = of_screen(Agent::Claude, &[]);
        starting.session = Some(("fix".into(), "00ab".into()));
        assert_eq!(starting.state(), (State::Starting, Source::Startup));
    }

    #[test]
    fn what_the_reads_last_showed_is_the_state_the_screen_gave() {
        let held = Shown {
            state: State::Working,
            held: true,
        };
        let mut idle = explained(None, Some(held));
        idle.screen = Some(of_screen(Agent::Claude, &recorded("input")).screen.unwrap());
        assert_eq!(idle.state(), (State::Working, Source::Screen));
        assert!(idle.text().contains("\nheld: working until the next read"));
        assert_eq!(idle.json()["held"], true);
    }

    #[test]
    fn the_text_names_the_rule_its_rows_and_the_screen_s_last_rows() {
        let text = explained(None, None).text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[..6],
            [
                "session: fix (00ab)",
                "agent: claude",
                "state: working (from screen)",
                "reported: nothing",
                "screen: working by rule claude.spinner_above_input",
                "  row 28: ✻ Running… (3s · esc to interrupt)",
            ]
        );
        assert!(text.contains("\nstartup: ready by rule claude.input_box_ready\n"));
        assert!(text.contains("\nrecorded: ready\n"));
        assert!(text.contains("\n  | ❯"));
    }

    #[test]
    fn an_explanation_names_the_rules_in_force_and_why_the_user_s_were_set_aside() {
        let mut e = of_screen(Agent::Codex, &[]);
        e.rules = RulesUsed {
            from: None,
            warning: Some(
                "/c/codex.toml cannot be used, so the bundled rules judge instead".into(),
            ),
        };
        let text = e.text();
        assert!(
            text.contains("\nrules: bundled\nrules warning: /c/codex.toml cannot be used"),
            "{text}"
        );
        assert_eq!(e.json()["rules"]["from"], Value::Null);
        assert!(
            e.json()["rules"]["warning"]
                .as_str()
                .unwrap()
                .starts_with("/c/codex.toml")
        );

        e.rules = RulesUsed {
            from: Some("/c/codex.toml".into()),
            warning: None,
        };
        let text = e.text();
        assert!(
            text.contains("\nrules: /c/codex.toml\nscreen tail"),
            "{text}"
        );
        assert!(!text.contains("rules warning"));
        assert_eq!(e.json()["rules"]["from"], "/c/codex.toml");
    }

    #[test]
    fn a_kept_screen_says_how_it_reads_and_nothing_of_a_session() {
        let text = of_screen(Agent::Claude, &recorded("input")).text();
        assert!(text.starts_with(
            "agent: claude\nscreen: idle by default claude.input_box_idle (no rule's sign shown)\n"
        ));
        assert!(!text.contains("state:"));
        let json = of_screen(Agent::Claude, &recorded("input")).json();
        assert_eq!(json["screen"]["verdict"]["rule"], "claude.input_box_idle");
        assert_eq!(json["screen"]["verdict"]["default"], true);
        assert_eq!(json["session"], Value::Null);
    }

    #[test]
    fn a_reported_state_is_shown_raw_and_the_screen_unread() {
        let mut e = explained(
            Some(Reported {
                state: "working".into(),
                kind: None,
                msg: None,
            }),
            None,
        );
        e.screen = None;
        let text = e.text();
        assert!(text.contains("\nstate: working (from osc7501)\n"));
        assert!(text.contains("\nreported: state=working kind=- msg=-\n"));
        assert!(text.contains("\nscreen: not read while the program reports its state\n"));
        assert_eq!(e.json()["reported"]["state"], "working");
    }

    #[test]
    fn a_session_still_starting_says_how_long_until_it_stalls() {
        let stall = crate::delegation::service::STALL;
        assert_eq!(
            stalls_in(Readiness::Starting, Duration::from_secs(15)),
            Some(stall - Duration::from_secs(15))
        );
        assert_eq!(
            stalls_in(Readiness::Starting, stall * 2),
            Some(Duration::ZERO)
        );
        assert_eq!(stalls_in(Readiness::Ready, Duration::ZERO), None);
        let mut e = explained(None, None);
        e.start = Some(Start {
            recorded: Readiness::Starting,
            stalls_in: Some(Duration::from_secs(45)),
        });
        assert!(e.text().contains("\nrecorded: starting, stalled in 45s\n"));
    }

    #[test]
    fn the_tail_keeps_the_last_rows_that_show_something() {
        let screen: Vec<String> = (0..30)
            .map(|i| format!("row {i}  "))
            .chain(["".into(), "  ".into()])
            .collect();
        let tail = tail(&screen);
        assert_eq!(tail.len(), TAIL);
        assert_eq!(tail.last().map(String::as_str), Some("row 29"));
        assert!(super::tail(&[]).is_empty());
    }
}
