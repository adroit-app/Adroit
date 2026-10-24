//! What the focused window shows under its strip: its console, read live or
//! in the reader, and the prompt below it; a draft's form, or what to do
//! with nothing open.

use adroit_core::content::{Area, Tab, file_name, shows_prompt};
use adroit_core::msg::Msg;
use adroit_core::tree as tree_view;
use dioxus::html::HasFileData;
use dioxus::prelude::*;

use super::{DraftForm, NothingOpen, Starting};
use crate::actions::Action;
use crate::bridge::AppCtx;
use crate::console::{Composer, ConsoleScreen, Consoles, Hub, ReaderScreen};
use crate::keymap::Context;
use crate::nav;
use crate::prompts::Op;

#[component]
pub(crate) fn FocusedBody() -> Element {
    let app = use_context::<AppCtx>();
    let hub = use_context::<Hub>();
    let consoles = use_context::<Consoles>();
    let Consoles {
        frame,
        attached,
        nvim,
        prompt_frame,
        prompt_view,
        prompt_rows,
        reader,
        reader_frame,
        reader_view,
        read_rows,
        cell_width,
        mut console_box,
        composer_height,
        prompt_width,
        prompts,
        scroll_reader,
        click_reader,
    } = consoles;
    let _ = attached();
    let content = app.content.read();
    let area = content.focus;
    let active_tab = content.active().cloned();
    let host = app.host.read();
    let work = host.work.as_ref();
    let tree = tree_view::listed(work);
    let current_agent = content.drafts.agent_of(
        tree,
        active_tab.as_ref(),
        app.config.read().settings.default_agent(),
    );
    let pane = active_tab
        .as_ref()
        .and_then(|tab| hub.panes.borrow().get(tab).cloned());
    let console_background = hub.palette.read().background.hex();
    let (reading, passing, dialog) = {
        let console = app.console.read();
        (console.reading, console.direct, console.dialog)
    };
    // The session shown waits on an answer, its own or a dialog's.
    let answering = match &active_tab {
        Some(Tab::Session(id)) => {
            dialog || nav::find(tree, id).is_some_and(|(_, s)| s.state.takes_every_key())
        }
        _ => false,
    };
    // The images on the console's screen, which is the last of the lines the
    // reader holds, moved as the reader is scrolled over them.
    let reader_pictures: Vec<adroit_term::Picture> = if reading {
        let _ = reader_frame();
        let _ = frame();
        let (top, left) = reader_view();
        let (held, rows) = read_rows();
        let down = held.saturating_sub(rows) as f32 - top as f32;
        pane.as_ref()
            .map(|p| p.pictures())
            .unwrap_or_default()
            .into_iter()
            .map(|mut p| {
                p.dst[0] -= left as f32;
                p.dst[1] += down;
                p
            })
            .filter(|p| p.dst[0] + p.dst[2] > 0.0)
            .collect()
    } else {
        Vec::new()
    };
    let target = match &active_tab {
        Some(Tab::Session(id)) => nav::find(tree, id)
            .map(|(_, s)| s.name.clone())
            .unwrap_or_else(|| id.clone()),
        Some(Tab::Terminal(n)) => tree_view::terminals(work)
            .iter()
            .find(|s| s.id == *n)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        Some(Tab::Draft { .. }) => "new session".into(),
        Some(Tab::Viewer(path)) => file_name(path),
        None => "new scratch session".into(),
    };
    let draft_options = active_tab
        .as_ref()
        .and_then(|tab| content.drafts.options.get(tab).cloned())
        .unwrap_or_default();
    let next_options = content.drafts.next.clone().unwrap_or_default();
    drop((content, host));
    let prompt_shown = shows_prompt(active_tab.as_ref());
    let composer_room = if prompt_shown {
        composer_height() + 22.0
    } else {
        0.0
    };
    let (prompt_shell, shown_images, selected_image) = {
        let prompt = app.prompt.read();
        (prompt.shell, prompt.images.clone(), prompt.selected.clone())
    };
    let km = app.chrome.read().keymap.clone();
    let k_send = km.keys_for(Context::Prompt, &Action::PromptSend);
    let cw = cell_width();
    let choose_agent = move |tab: Option<Tab>, agent| app.dispatch(Msg::DraftAgent { tab, agent });
    rsx! {
        div {
            class: "console",
            style: "--composer:{composer_room}px;--console-bg:{console_background};background:{console_background}",
            onresize: move |e: Event<ResizeData>| {
                e.stop_propagation();
                if let Ok(sz) = e.data().get_content_box_size() {
                    console_box.set((sz.width, sz.height));
                }
            },
            // Files may be dropped on the console and the prompt card both;
            // a drop the page does not take would open the file in its place.
            ondragover: move |e: Event<DragData>| e.prevent_default(),
            ondrop: move |e: Event<DragData>| {
                e.prevent_default();
                let paths = e.files().iter().map(|f| f.path()).collect();
                app.dispatch(Msg::FilesDropped(paths));
            },
            match (&active_tab, pane.is_some()) {
                (Some(tab @ Tab::Draft { worktree, .. }), _) => rsx! {
                    DraftForm {
                        worktree: worktree.clone(),
                        agent: current_agent,
                        options: draft_options,
                        keymap: km.clone(),
                        onagent: move |a| {
                            choose_agent(app.active(), a);
                            app.dispatch(Msg::Focus(Area::Prompt));
                            if !consoles.writing() {
                                consoles.start_writing();
                            }
                        },
                        onoptions: {
                            let tab = tab.clone();
                            move |options| app.dispatch(Msg::DraftOptions { tab: Some(tab.clone()), options })
                        },
                    }
                },
                (Some(_), true) if reading => rsx! {
                    ReaderScreen { reader, reader_frame, cell_width: cw, focused: area == Area::Console, pictures: reader_pictures, onscroll: scroll_reader, onclick: click_reader }
                },
                (Some(tab), true) => rsx! {
                    ConsoleScreen { tab: tab.clone(), frame, cell_width: cw, focused: area == Area::Console }
                },
                (Some(_), false) => rsx! {
                    Starting {}
                },
                (None, _) => rsx! {
                    NothingOpen {
                        agent: current_agent,
                        options: next_options,
                        keymap: km.clone(),
                        onpick: move |_| app.dispatch(Msg::PickerOpen),
                        onagent: move |a| choose_agent(None, a),
                        onoptions: move |options| app.dispatch(Msg::DraftOptions { tab: None, options }),
                    }
                },
            }
            if passing && area == Area::Console {
                div { class: "direct-banner",
                    span { class: "direct-banner-title", "Passing every key" }
                    span { "to the session. " kbd { "Esc" } " stops, " kbd { "Ctrl-c" } " sends Esc." }
                }
            }
            if prompt_shown {
                Composer {
                    nvim,
                    prompt_frame,
                    cell_width: cw,
                    focused: area == Area::Prompt,
                    target,
                    composer_height,
                    prompt_width,
                    onpick: move |_| {
                        app.dispatch(Msg::ImageSelect(None));
                        // A click on the prompt is to write there; the
                        // first key typed would otherwise run as a command.
                        if !consoles.writing() {
                            consoles.start_writing();
                        }
                    },
                    send_keys: k_send,
                    view: prompt_view(),
                    rows: prompt_rows(),
                    shell: prompt_shell,
                    answering,
                    images: shown_images,
                    image_keys: km.keys_for(Context::Prompt, &Action::PromptImages),
                    selected_image,
                    onselectimage: move |path| app.dispatch(Msg::ImageSelect(Some(path))),
                    onremoveimage: move |path| { let _ = prompts.send(Op::RemoveImage(path)); },
                }
            }
        }
    }
}
