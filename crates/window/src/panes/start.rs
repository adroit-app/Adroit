//! Starting sessions and shells with a pane of their own, so their program
//! starts in the theme's colours: a shell or a Neovim asked for, a session
//! from a draft's first prompt, a conversation an agent kept continued. The
//! work host starts each with its program held; the window attaches, tells
//! it the theme's colours, and has the host let the program go.

use std::path::PathBuf;
use std::sync::Arc;

use adroit_core::content::{Area, Tab, TerminalAsk, file_name, fresh_name, label_from};
use adroit_core::msg::Msg;
use adroit_core::prompt::send::Start;
use adroit_protocol::{Answer, Ask, HostPath, Request, SessionStart, TerminalStart};
use adroit_term::{Pane, Size};
use dioxus::prelude::*;

use super::tell_live;
use crate::bridge::AppCtx;
use crate::console::Hub;
use crate::nav::Item;
use crate::shell::Ui;
use adroit_model::daemon as sessions;

/// This program, as a session should call it to reach the window.
pub(crate) fn adroit_program() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| "adroit".into())
}

/// Attach to the session `id`, which the host started with its program
/// held, as `tab` shows it at `size`, so it is told the theme's colours;
/// then have the host let the program go.
async fn attach_held(
    app: AppCtx,
    hub: &Hub,
    id: u128,
    tab: Tab,
    size: Size,
) -> adroit_term::Result<()> {
    let palette = hub.palette.peek().clone();
    let wake = hub.wake.clone();
    let woken = tab.clone();
    let pane = Pane::attach(&hub.daemon, id, size, &palette, move || {
        let _ = wake.send(woken.clone());
    })
    .await;
    // The colours are told before the program goes; whatever came of the
    // attach, it is not left waiting for a window that cannot show it.
    let fenced = match &pane {
        Ok(pane) => pane.fenced().await,
        Err(_) => Ok(()),
    };
    app.tell_host(Request::Release { id });
    let pane = pane?;
    fenced?;
    // A theme chosen while the session was being started passed it by.
    let now = hub.palette.peek().clone();
    if now != palette {
        pane.set_palette(&now);
    }
    hub.panes.borrow_mut().insert(tab, Arc::new(pane));
    tell_live(app, hub);
    Ok(())
}

impl Ui {
    /// Start the session `start` asks for, and attach to it: its id, or why
    /// it could not start, or why its pane could not attach as `failed`
    /// words it.
    async fn start_session(
        &self,
        start: SessionStart,
        failed: impl FnOnce(adroit_term::Error) -> String,
    ) -> Result<String, String> {
        let size = Size::new(start.cols, start.rows);
        let id = match self.app.ask(Ask::StartSession(Box::new(start))).await {
            Answer::Session { id } => id,
            Answer::Failed { text } => return Err(text),
            answer => return Err(format!("the work host answered {answer:?}")),
        };
        let Some(terminal) = sessions::id_of(&id) else {
            return Err(format!("the work host started no session it names: {id}"));
        };
        let tab = Tab::Session(self.app.environment(), id.clone());
        attach_held(self.app, &self.hub, terminal, tab, size)
            .await
            .map_err(failed)?;
        Ok(id)
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
            (None, runs) => adroit_model::base_name(runs, &[]),
        };
        let name = fresh_name(&base, &taken);
        let size = *self.local.console_size.peek();
        let start = TerminalStart {
            name,
            cwd: HostPath::local(&p.worktree),
            program: runs,
            file,
            cols: size.cols,
            rows: size.rows,
        };
        let hub = self.hub.clone();
        spawn(async move {
            let terminal = match app.ask(Ask::StartTerminal(Box::new(start))).await {
                Answer::Terminal { terminal } => terminal,
                Answer::Failed { text } => {
                    app.status(Some(format!("cannot start {base}: {text}")));
                    return;
                }
                answer => {
                    app.status(Some(format!("cannot start {base}: {answer:?}")));
                    return;
                }
            };
            let id = terminal.id;
            let tab = Tab::Terminal(app.environment(), id);
            if let Err(e) = attach_held(app, &hub, id, tab.clone(), size).await {
                app.status(Some(format!("cannot start {base}: {e}")));
                return;
            }
            app.dispatch(Msg::TerminalOpened(terminal));
            app.dispatch(Msg::Show {
                thing: tab,
                place: Some(p),
                how: Some(opening),
            });
            if take_keys {
                app.dispatch(Msg::Focus(Area::Console));
            }
        });
    }

    /// Open a terminal that runs the CLI's own login where `profile` runs
    /// it, shown with the keys, for the user to log in
    /// there; whether it is logged in is asked again once it ends.
    pub(crate) fn open_login(&self, profile: adroit_model::profile::ProfileId) {
        let (app, hub) = (self.app, self.hub.clone());
        let size = *self.local.console_size.peek();
        spawn(async move {
            let asked = Ask::OpenLogin {
                profile: profile.clone(),
                cols: size.cols,
                rows: size.rows,
            };
            let terminal = match app.ask(asked).await {
                Answer::Terminal { terminal } => terminal,
                Answer::Failed { text } => {
                    app.status(Some(format!("cannot open the login terminal: {text}")));
                    return;
                }
                answer => {
                    app.status(Some(format!("cannot open the login terminal: {answer:?}")));
                    return;
                }
            };
            let id = terminal.id;
            crate::logins::ask(app, profile, Some(id));
            let tab = Tab::Terminal(app.environment(), id);
            if let Err(e) = attach_held(app, &hub, id, tab.clone(), size).await {
                app.status(Some(format!("cannot open the login terminal: {e}")));
                return;
            }
            app.dispatch(Msg::TerminalOpened(terminal));
            app.dispatch(Msg::Show {
                thing: tab,
                place: None,
                how: None,
            });
            app.dispatch(Msg::Focus(Area::Console));
        });
    }

    /// Start a session from a draft with its first prompt, in the draft's
    /// place, where the host makes a folder of its own for a scratch draft
    /// only now: a draft left unsent leaves no empty directory behind.
    pub(crate) fn start_draft(&self, asked: Start) {
        let (app, ui) = (self.app, self.clone());
        spawn(async move {
            let size = *ui.local.console_size.peek();
            let Start {
                draft,
                profile,
                name,
                text,
                options,
                taken,
            } = asked;
            let Tab::Draft {
                environment,
                worktree,
                ..
            } = &draft
            else {
                app.dispatch(Msg::PromptWent(false));
                return;
            };
            // The session started from a draft belongs to the draft's
            // environment.
            let environment = environment.clone();
            let start = SessionStart {
                agent: taken.agent,
                profile: Some(profile),
                name: name.clone(),
                cwd: HostPath::local(worktree),
                draft: true,
                model: options.model,
                permissions: options.permissions,
                prompt: text,
                images: taken.images.clone(),
                resume: None,
                home: None,
                cols: size.cols,
                rows: size.rows,
            };
            let failed = |e| format!("cannot start {name}: {e}");
            if app.remote_status().is_some() {
                let Some(context) = crate::prompts::remote::context(&taken) else {
                    app.dispatch(Msg::PromptRetained);
                    return;
                };
                match app
                    .ask_prompt(Ask::StartSession(Box::new(start)), context.clone())
                    .await
                {
                    Answer::Session { id } => {
                        let cleanup = ui.finish_remote_prompt(&context, true).await;
                        app.dispatch(Msg::Relist);
                        match cleanup {
                            Ok(()) => {
                                app.dispatch(Msg::Replace {
                                    from: draft,
                                    to: Tab::Session(environment.clone(), id.clone()),
                                });
                                app.dispatch(Msg::PromptWent(true));
                            }
                            Err(text) => {
                                app.status(Some(format!("The session started; {text}")));
                                app.dispatch(Msg::PromptRetained);
                            }
                        }
                        if let Some(terminal) = sessions::id_of(&id)
                            && let Err(error) = attach_held(
                                app,
                                &ui.hub,
                                terminal,
                                Tab::Session(environment, id),
                                size,
                            )
                            .await
                        {
                            app.status(Some(format!(
                                "The session started, but its console could not attach: {error}"
                            )));
                        }
                    }
                    answer => {
                        if matches!(answer, Answer::Failed { .. })
                            && let Err(text) = ui.finish_remote_prompt(&context, false).await
                        {
                            app.status(Some(text));
                        }
                        app.status(Some(format!("The draft is retained: {answer:?}")));
                        app.dispatch(Msg::PromptRetained);
                    }
                }
                return;
            }
            match ui.start_session(start, failed).await {
                Ok(id) => {
                    app.dispatch(Msg::Relist);
                    app.dispatch(Msg::TreeSelect(Some(Item::Session(
                        environment.clone(),
                        id.clone(),
                    ))));
                    app.dispatch(Msg::Replace {
                        from: draft,
                        to: Tab::Session(environment, id),
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
            let start = SessionStart {
                agent: past.agent,
                profile: Some(past.profile.clone()),
                name: name.clone(),
                cwd: HostPath::local(&past.cwd),
                resume: Some(past.id.clone()),
                home: past.home.clone(),
                cols: size.cols,
                rows: size.rows,
                ..Default::default()
            };
            let failed = |e| format!("cannot resume {name}: {e}");
            match ui.start_session(start, failed).await {
                Ok(id) => {
                    app.status(None);
                    app.dispatch(Msg::Relist);
                    let environment = app.environment();
                    app.dispatch(Msg::TreeSelect(Some(Item::Session(
                        environment.clone(),
                        id.clone(),
                    ))));
                    app.dispatch(Msg::Show {
                        thing: Tab::Session(environment, id),
                        place: None,
                        how: None,
                    });
                }
                Err(e) => app.status(Some(e)),
            }
        });
    }
}
