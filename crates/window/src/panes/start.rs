//! Starting sessions and shells with a pane of their own, so their program
//! starts in the theme's colours: a shell or a Neovim asked for, a session
//! from a draft's first prompt, a conversation an agent kept continued.

use std::path::PathBuf;
use std::sync::Arc;

use adroit_core::content::{Area, Tab, TerminalAsk, file_name, fresh_name, label_from};
use adroit_core::msg::Msg;
use adroit_core::prompt::send::Start;
use adroit_model::time::now_ms;
use adroit_term::{Pane, Size, Spawn};
use dioxus::prelude::*;

use super::tell_live;
use crate::bridge::AppCtx;
use crate::console::Hub;
use crate::delegation::app::{self as delegation, Recorded};
use crate::execution::{StartRequest, TerminalStartRequest};
use crate::nav::Item;
use crate::scratch::{self, Moment};
use crate::shell::Ui;
use crate::{daemon, programs, sessions};

/// This program, as a session should call it to reach the window.
pub(crate) fn adroit_program() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| "adroit".into())
}

/// Start a session as `spawn` says, with the pane of the tab `tab_of` its id
/// names, so that its program starts in the theme's colours.
async fn start(
    app: AppCtx,
    hub: &Hub,
    spawn: Spawn,
    size: Size,
    tab_of: impl Fn(u128) -> Tab,
    recorded: Option<Recorded>,
) -> adroit_term::Result<u128> {
    let palette = hub.palette.peek().clone();
    let wake = hub.wake.clone();
    let failure_owner = recorded.clone();
    let pane = Pane::start_prepared(
        &hub.daemon,
        spawn,
        size,
        &palette,
        |id| {
            let tab = tab_of(id);
            move || {
                let _ = wake.send(tab.clone());
            }
        },
        move |id| async move {
            match recorded {
                Some(recorded) => recorded.attach(id).await,
                None => Ok(()),
            }
        },
    )
    .await;
    let pane = match pane {
        Ok(pane) => pane,
        Err(error) => {
            if let Some(owner) = failure_owner {
                owner.failed().await;
            }
            return Err(error);
        }
    };
    // A theme chosen while the session was being started passed it by.
    let now = hub.palette.peek().clone();
    if now != palette {
        pane.set_palette(&now);
    }
    let id = pane.id();
    hub.panes.borrow_mut().insert(tab_of(id), Arc::new(pane));
    tell_live(app, hub);
    Ok(id)
}

impl Ui {
    /// Start the session `request` asks for at `size`, as the delegation store
    /// records it: why it could not be prepared, or why its pane could not
    /// start as `failed` words it.
    async fn start_session(
        &self,
        request: StartRequest,
        size: Size,
        failed: impl FnOnce(adroit_term::Error) -> String,
    ) -> Result<u128, String> {
        let settings = self.app.config.peek().settings.clone();
        let (resolved, recorded) = delegation::prepare(
            &delegation::root(),
            &settings,
            request,
            None,
            &adroit_program(),
        )
        .await?;
        let mut spawn = resolved.spawn;
        spawn.cols = size.cols;
        spawn.rows = size.rows;
        let tab_of = |id| Tab::Session(sessions::hex(id));
        start(self.app, &self.hub, spawn, size, tab_of, Some(recorded))
            .await
            .map_err(failed)
    }

    /// Start a shell or a Neovim as asked, named apart from those in its
    /// worktree, and show it where it was asked for.
    pub(crate) fn start_terminal(&self, ask: TerminalAsk) {
        let app = self.app;
        let TerminalAsk {
            place: p,
            runs,
            file,
            opening,
            focus: take_keys,
        } = ask;
        let names: Vec<String> = {
            let host = app.host.peek();
            adroit_core::tree::terminals(host.work.as_ref())
                .iter()
                .filter(|s| s.cwd == p.worktree)
                .map(|s| s.name.clone())
                .collect()
        };
        let taken: Vec<&str> = names.iter().map(String::as_str).collect();
        let base = match (&file, runs) {
            (Some(f), _) => file_name(f),
            (None, runs) => crate::terminals::base_name(runs, &[]),
        };
        let name = fresh_name(&base, &taken);
        let size = *self.local.console_size.peek();
        let mut resolved = crate::execution::resolve_terminal(
            &app.config.peek().settings,
            TerminalStartRequest {
                name,
                cwd: p.worktree.clone(),
                program: runs,
                // Not `agent_env`: a shell or Neovim keeps the user's own editor.
                env: daemon::terminal_env(),
                file,
                command: Vec::new(),
            },
            &programs::login_shell(),
        );
        resolved.spawn.cols = size.cols;
        resolved.spawn.rows = size.rows;
        let hub = self.hub.clone();
        spawn(async move {
            match start(app, &hub, resolved.spawn, size, Tab::Terminal, None).await {
                Ok(id) => {
                    resolved.terminal.id = id;
                    app.dispatch(Msg::TerminalOpened(resolved.terminal));
                    app.dispatch(Msg::Show {
                        thing: Tab::Terminal(id),
                        place: Some(p),
                        how: Some(opening),
                    });
                    if take_keys {
                        app.dispatch(Msg::Focus(Area::Console));
                    }
                }
                Err(e) => app.status(Some(format!("cannot start {base}: {e}"))),
            }
        });
    }

    /// Start a session from a draft with its first prompt, in the draft's
    /// place, which a scratch draft makes only now: a draft left unsent
    /// leaves no empty directory behind.
    pub(crate) fn start_draft(&self, asked: Start) {
        let (app, ui) = (self.app, self.clone());
        spawn(async move {
            let size = *ui.local.console_size.peek();
            let Start {
                draft,
                name,
                text,
                options,
                taken,
            } = asked;
            let Tab::Draft { worktree, .. } = &draft else {
                app.dispatch(Msg::PromptWent(false));
                return;
            };
            let scratch = app.config.peek().scratch.clone();
            let started = match scratch::start_dir(worktree, &scratch, Moment::now()) {
                Ok(worktree) => {
                    let request = StartRequest {
                        agent: taken.agent,
                        name: name.clone(),
                        cwd: worktree,
                        model: options.model,
                        permissions: options.permissions,
                        prompt: text,
                        images: taken.images.clone(),
                        env: daemon::agent_env(),
                        started_ms: now_ms(),
                        ..Default::default()
                    };
                    let failed = |e| format!("cannot start {name}: {e}");
                    ui.start_session(request, size, failed).await
                }
                Err(e) => Err(format!("cannot make a directory for {name}: {e}")),
            };
            match started {
                Ok(id) => {
                    let id = sessions::hex(id);
                    app.dispatch(Msg::Relist);
                    app.dispatch(Msg::TreeSelect(Some(Item::Session(id.clone()))));
                    app.dispatch(Msg::Replace {
                        from: draft,
                        to: Tab::Session(id),
                    });
                    app.dispatch(Msg::PromptWent(true));
                }
                Err(e) => {
                    app.status(Some(e));
                    app.dispatch(Msg::PromptWent(false));
                }
            }
        });
    }

    /// Continue a conversation an agent kept in a new session, in the
    /// directory it ran in, shown where the user is.
    pub(crate) fn resume(&self, past: adroit_model::Past) {
        let (app, ui) = (self.app, self.clone());
        spawn(async move {
            let size = *ui.local.console_size.peek();
            let names: Vec<String> = {
                let host = app.host.peek();
                adroit_core::tree::listed(host.work.as_ref())
                    .sessions()
                    .map(|(_, s)| s.name.clone())
                    .collect()
            };
            let taken: Vec<&str> = names.iter().map(String::as_str).collect();
            let name = fresh_name(&label_from(&past.title), &taken);
            app.status(Some(format!("resuming {name}…")));
            let request = StartRequest {
                agent: past.agent,
                name: name.clone(),
                cwd: past.cwd.clone(),
                resume: Some(past.id.clone()),
                env: daemon::agent_env(),
                started_ms: now_ms(),
                ..Default::default()
            };
            match ui.start_session(request, size, |e| e.to_string()).await {
                Ok(id) => {
                    app.status(None);
                    let id = sessions::hex(id);
                    app.dispatch(Msg::Relist);
                    app.dispatch(Msg::TreeSelect(Some(Item::Session(id.clone()))));
                    app.dispatch(Msg::Show {
                        thing: Tab::Session(id),
                        place: None,
                        how: None,
                    });
                }
                Err(e) => app.status(Some(format!("cannot resume {name}: {e}"))),
            }
        });
    }
}
