//! What the view state asks of what lives in the window: the panes, the
//! prompt's Neovim and keeper, the console's reader, starting sessions and
//! shells, and the images and viewers it shows.

use adroit_core::effect::Effect;
use adroit_nvim::notation;
use adroit_protocol::{Answer, Ask};
use dioxus::prelude::*;

use super::Ui;
use crate::prompts::Op;
use crate::viewers;
use crate::{complete, keys};

/// Hand the view state's effects that live here to `ui`, once.
pub(crate) fn use_effects(ui: &Ui) {
    let ui = ui.clone();
    use_hook(move || {
        let app = ui.app;
        app.hand_over(move |effect| ui.run(effect));
    });
}

impl Ui {
    /// Carry out `effect`, which the window holds the means of.
    fn run(&self, effect: Effect) {
        match effect {
            Effect::Resume(past) => self.resume(past),
            Effect::RestartDaemon => self.restart_daemon(),
            Effect::StartTerminal(ask) => self.start_terminal(ask),
            Effect::OpenLogin { profile } => self.open_login(profile),
            Effect::StartDraft(start) => self.start_draft(*start),
            Effect::TypeInto { tab, typing } => self.type_into_tab(tab, typing),
            Effect::Palette(colours) => self.repaint(&colours),
            // A session no longer listed has nothing to show; its pane is
            // closed, and the images its console drew are not kept.
            Effect::KeepPanes {
                sessions,
                terminals,
            } => self.keep_panes(&sessions, &terminals),
            Effect::EndShell(id) => self.end_shell(id),
            Effect::AskInput(tab) => self.ask_input(tab),
            Effect::TakePrompt => self.take_prompt(),
            Effect::SendPrompt(taken) => self.send_prompt(*taken),
            Effect::TakeSuggestion {
                key,
                neovim_owns,
                session,
                hands_back,
                no_images,
                tab,
            } => self.take_suggestion(key, neovim_owns, session, hands_back, no_images, tab),
            Effect::ConsoleLive => self.console_live(),
            Effect::FilterFocus(on) => self.local.focus_filter(on),
            Effect::Viewer { window, action } => {
                let mut command = self.local.viewer_command;
                let seq = command.peek().as_ref().map_or(1, |(_, c)| c.seq + 1);
                command.set(Some((window, viewers::Command { seq, action })));
            }
            effect => self.run_console(effect),
        }
    }

    /// Carry out an effect on the focused window's pane, the console's
    /// reader, or the prompt.
    fn run_console(&self, effect: Effect) {
        let prompts = self.consoles.prompts;
        let nvim = self.consoles.nvim.peek().clone();
        let mut frame = self.local.frame;
        match effect {
            Effect::PaneKey(key) => {
                if let Some(p) = self.active_pane() {
                    p.key(&key);
                }
            }
            Effect::PaneScrollKey(key) => {
                if let Some(p) = self.active_pane() {
                    p.key(&key);
                    *frame.write() += 1;
                }
            }
            Effect::PaneText { text, pasted } => {
                if let Some(p) = self.active_pane() {
                    if pasted {
                        p.paste(&text);
                    } else {
                        p.type_text(&text);
                    }
                }
            }
            Effect::PaneWheel(lines) => {
                if let Some(p) = self.active_pane() {
                    p.wheel(lines, 0, 0);
                    *frame.write() += 1;
                }
            }
            Effect::ReaderKey(key) => {
                if let (Some(r), Some(n)) = (self.consoles.reader.peek().clone(), notation(&key)) {
                    spawn(async move {
                        let _ = r.input(&n).await;
                    });
                }
            }
            Effect::ReadConsole(keys) => {
                self.reader
                    .enter(keys.iter().filter_map(notation).collect());
            }
            Effect::StartWriting => self.consoles.start_writing(),
            Effect::PromptKey(key) => {
                if let Some(n) = notation(&key)
                    && let Some(nv) = nvim
                {
                    spawn(async move {
                        if let Err(e) = nv.input(&n).await {
                            keys::trace(|| format!("prompt refused {n}: {e}"));
                        }
                    });
                }
            }
            Effect::PromptText { text, pasted } => {
                if let Some(nv) = nvim {
                    spawn(async move {
                        if pasted {
                            let _ = nv.paste(&text).await;
                        } else {
                            let _ = nv.type_text(&text).await;
                        }
                    });
                }
            }
            Effect::PromptOffer(offers) => {
                if let Some(nv) = nvim {
                    let list = complete::offered(&offers);
                    spawn(async move {
                        let _ = nv.set_candidates(&list).await;
                    });
                }
            }
            Effect::PromptCommands(list) => {
                if let Some(nv) = nvim {
                    let list = complete::commands(&list);
                    spawn(async move {
                        let _ = nv.set_commands(&list).await;
                    });
                }
            }
            Effect::PromptRoot(root) => {
                if let Some(nv) = nvim {
                    spawn(async move {
                        let _ = nv.set_root(root.as_deref()).await;
                    });
                }
            }
            Effect::KeepImage { mime, data } => {
                let (app, kept) = (self.app, self.app.ask(Ask::KeepImage { mime, data }));
                spawn(async move {
                    match kept.await {
                        Answer::Kept { path } => {
                            let _ = prompts.send(Op::Attach(path));
                        }
                        Answer::Failed { text } => app.notice(text),
                        answer => app.notice(format!("the work host answered {answer:?}")),
                    }
                });
            }
            Effect::KeepImageFiles(paths) => {
                let app = self.app;
                let asked: Vec<_> = paths
                    .into_iter()
                    .map(|path| app.ask(Ask::KeepImageFile { path }))
                    .collect();
                // Attached in the order dropped, whichever copy is made first.
                spawn(async move {
                    for kept in asked {
                        match kept.await {
                            Answer::Kept { path } => {
                                let _ = prompts.send(Op::Attach(path));
                            }
                            Answer::Failed { text } => app.notice(text),
                            answer => app.notice(format!("the work host answered {answer:?}")),
                        }
                    }
                });
            }
            Effect::PromptClose(to) => {
                let _ = prompts.send(Op::Close(to));
            }
            Effect::PromptShow(tab) => {
                let _ = prompts.send(Op::Show(tab));
            }
            Effect::PromptSent => {
                let _ = prompts.send(Op::Sent);
            }
            Effect::PromptRestore { tab, text, images } => {
                let lines: Vec<String> = text.lines().map(String::from).collect();
                let _ = prompts.send(Op::RestoreImages(tab.clone(), images));
                let _ = prompts.send(Op::Add(tab, lines, None));
            }
            Effect::RemoveImage(path) => {
                let _ = prompts.send(Op::RemoveImage(path));
            }
            Effect::PromptRecall { tab, back, sent } => {
                let _ = prompts.send(Op::Recall(tab, back, sent));
            }
            _ => {}
        }
    }
}
