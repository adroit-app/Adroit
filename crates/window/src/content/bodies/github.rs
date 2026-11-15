//! A window showing a pull request or an issue as GitHub shows its
//! conversation.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use adroit_core::viewers::{Effect, MarkdownState, ViewerState};
use adroit_model::EnvironmentId;
use adroit_model::github::{Listing, Thread};
use adroit_protocol::{Answer, Ask};
use arto_frontend::Libraries;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::content::WindowId;
use crate::themes::Document;
use crate::viewers::{
    self, frame_script, frame_scroll, page_frame, theme_message, thread_document, thread_fragment,
};

/// How long what is shown stands before it is read again.
const READ_EVERY: Duration = Duration::from_secs(60);

static NEXT: AtomicU64 = AtomicU64::new(1);

/// A window on the pull request or issue `number` of `listing` in the
/// repository at `repository` of `environment`, read whole from GitHub when
/// it opens, again every minute it stays and whenever the work is read
/// again; handed the viewer's actions sent to this window alone.
#[component]
pub(crate) fn GitHubWindow(
    window: WindowId,
    environment: EnvironmentId,
    repository: PathBuf,
    listing: Listing,
    number: u64,
    theme: ReadSignal<Document>,
    commands: ReadSignal<Option<(WindowId, viewers::Command)>>,
    on_close: EventHandler<()>,
) -> Element {
    let app = use_context::<AppCtx>();
    let command = use_memo(move || commands().filter(|(w, _)| *w == window).map(|(_, c)| c));
    let prefix = use_hook(|| format!("adroit-github-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let media_id = format!("{prefix}-media");
    let box_id = format!("{prefix}-box");
    let nonce = use_hook(|| format!("{:016x}{:016x}", rand_part(), rand_part()));
    let mut state = use_signal(|| ViewerState::Markdown(MarkdownState::default()));
    let mut read = use_signal(|| None::<Result<Thread, String>>);
    // The frame's document is set once: a frame given another reloads and
    // loses its scroll, so what is read later reaches it by message.
    let mut frame_doc = use_signal(|| None::<(String, Libraries)>);
    let mut reads = use_signal(|| 0_u64);
    let last =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(command.peek().map_or(0, |c| c.seq))));
    let slug = {
        let hosts = app.hosts.read();
        adroit_core::tree::github(hosts.work(&environment), &repository)
            .map(|r| r.slug.clone())
            .unwrap_or_default()
    };

    use_hook(move || {
        spawn(async move {
            loop {
                tokio::time::sleep(READ_EVERY).await;
                reads += 1;
            }
        })
    });

    use_effect({
        let (environment, repository) = (environment.clone(), repository.clone());
        move || {
            let _ = reads();
            let asked = app.ask_in(
                &environment,
                Ask::GitHubThread {
                    repository: repository.clone(),
                    listing,
                    number,
                },
            );
            spawn(async move {
                let answer = match asked.await {
                    Answer::Thread { thread } => Ok(*thread),
                    Answer::Failed { text } => Err(text),
                    other => Err(format!("the work host answered {other:?}")),
                };
                // A read that failed leaves what was read before shown.
                let keeps = answer.is_err() && matches!(*read.peek(), Some(Ok(_)));
                if !keeps {
                    read.set(Some(answer));
                }
            });
        }
    });

    // What was read is drawn into the frame: the first time as its
    // document, later by message.
    use_effect({
        let (media_id, nonce, slug) = (media_id.clone(), nonce.clone(), slug.clone());
        move || {
            let Some(Ok(thread)) = read() else {
                return;
            };
            let now = *app.clock.peek();
            let fragment = thread_fragment(&slug, &thread, now);
            let libraries = Libraries::for_fragment(&fragment);
            let had = frame_doc.peek().as_ref().map(|(_, l)| *l);
            match had {
                // A diagram, a formula or code needs bundles the frame may
                // not have loaded, which the message loads first.
                Some(_) => {
                    let scripts: Vec<String> = viewers::library_names(libraries)
                        .into_iter()
                        .map(|n| format!("{}{}/{n}", viewers::ORIGIN, viewers::ARTO_PREFIX))
                        .collect();
                    let message =
                        serde_json::json!({ "markdown": fragment, "scripts": scripts }).to_string();
                    document::eval(&frame_script(&media_id, &message));
                }
                None => {
                    let doc = thread_document(&fragment, libraries, &theme.peek(), &nonce);
                    frame_doc.set(Some((doc, libraries)));
                }
            }
        }
    });

    use_effect({
        let media_id = media_id.clone();
        move || {
            let theme = theme();
            document::eval(&frame_script(&media_id, &theme_message(&theme)));
        }
    });

    // A link pressed in the page is opened in the browser.
    use_hook({
        let media_id = media_id.clone();
        move || {
            spawn(async move {
                let mut heard = document::eval(&format!(
                    "window.addEventListener('message', (e) => {{
                       const f = document.getElementById({media_id:?});
                       if (!f || e.source !== f.contentWindow) return;
                       const m = e.data;
                       if (m && m.adroitViewerOpen === 1 && typeof m.url === 'string') dioxus.send(m.url);
                     }});
                     await new Promise(() => {{}});"
                ));
                while let Ok(url) = heard.recv::<String>().await {
                    if url.starts_with("https://") || url.starts_with("http://") {
                        crate::bridge::open([url]);
                    }
                }
            });
        }
    });

    use_effect({
        let media_id = media_id.clone();
        move || {
            let Some(cmd) = command() else {
                return;
            };
            if cmd.seq == last.get() {
                return;
            }
            last.set(cmd.seq);
            match state.write().apply(cmd.action) {
                Effect::Scroll(s) => {
                    document::eval(&frame_script(&media_id, &frame_scroll(s)));
                }
                Effect::Reload => reads += 1,
                Effect::Close => on_close.call(()),
                Effect::CopyPath => {
                    if let Some(Ok(thread)) = read.peek().as_ref() {
                        let _ = crate::clipboard::copy_to_clipboard(&thread.item.url);
                    }
                }
                Effect::Redraw | Effect::Media(_) | Effect::Nothing => {}
            }
        }
    });

    use_effect({
        let media_id = media_id.clone();
        move || {
            if let ViewerState::Markdown(s) = state() {
                document::eval(&frame_script(
                    &media_id,
                    &format!("{{\"zoom\": {}}}", s.zoom.factor()),
                ));
            }
        }
    });

    let zoom = match state() {
        ViewerState::Markdown(s) => s.zoom,
        _ => Default::default(),
    };
    let detail = match read().as_ref() {
        None => "reading…".to_string(),
        Some(Err(why)) => why.clone(),
        Some(Ok(_)) => format!("{}%", zoom.percent()),
    };
    let body = match (frame_doc.read().clone(), read().as_ref()) {
        (Some((doc, _)), _) => page_frame(
            media_id.clone(),
            box_id.clone(),
            zoom.factor(),
            0.0,
            Some(theme.read().scheme),
            doc,
        ),
        (None, Some(Err(why))) => rsx! {
            div { class: "viewer-empty", "{why}" }
        },
        (None, _) => rsx! {},
    };
    rsx! {
        div { class: "win-viewer",
            div { class: "viewer",
                style { {viewers::CSS} }
                div { id: "{box_id}", class: "viewer-box", {body} }
                div { class: "viewer-status",
                    span { class: "name", "{slug}#{number}" }
                    span { "{detail}" }
                }
            }
        }
    }
}

/// Part of a nonce: what the clock and the process tell apart from one page
/// to the next. The nonce keeps out a script a post writes, which cannot
/// read it, not one that guesses it.
fn rand_part() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(adroit_model::time::now_ms());
    hasher.finish()
}
