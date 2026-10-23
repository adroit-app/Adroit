//! The details panel inside its frame: the sheet tabs, the sheet shown, and
//! the usage at its foot.

use std::path::PathBuf;
use std::time::Duration;

use adroit_core::actions::Action;
use adroit_core::msg::Msg;
use adroit_model::time::now_ms;
use adroit_model::{Agent, Process, Session};
use dioxus::prelude::*;

use super::changes::Repository;
pub(crate) use super::changes::Worktree;
pub(crate) use super::overview::LinkMenu;
use super::overview::Overview;
use super::usage::Usage;
use crate::bridge::AppCtx;
use crate::details::{Sheet, repository, system_gauges, usage};
use crate::icons::IconView;
use crate::settings::UsageSettings;

/// The action that shows `sheet`.
pub(crate) fn sheet_action(sheet: Sheet) -> Action {
    match sheet {
        Sheet::Overview => Action::DetailsOverview,
        Sheet::Changes => Action::DetailsChanges,
        Sheet::Log => Action::DetailsLog,
    }
}

#[component]
pub(crate) fn DetailsPanel(
    /// The worktree the details describe, while a window shows something.
    worktree: Option<PathBuf>,
    workspace_root: Option<PathBuf>,
    /// What the worktree is, for the changes and the log.
    kind: Worktree,
    /// When the shown session last took input through its console.
    live_input_ms: Option<u64>,
    /// The agent of what is shown, whose usage page the foot links.
    agent: Agent,
    usage_settings: UsageSettings,
    /// The keys of each sheet's tab, as its title.
    sheet_keys: Vec<String>,
    back_keys: String,
    on_kill: EventHandler<(Session, Process)>,
    on_menu: EventHandler<LinkMenu>,
) -> Element {
    let app = use_context::<AppCtx>();
    // The overview tells how long ago things were; it is told again now
    // and then while it is in sight.
    let mut clock = use_signal(now_ms);
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let d = app.details.peek();
            if d.shown.open && d.drill.sheet() == Sheet::Overview {
                clock.set(now_ms());
            }
        }
    });
    // Keep the row selected in view, after it is drawn.
    use_effect(move || {
        let _ = (app.details.read(), app.host.read());
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.details-scroll .selected')
               .forEach((r) => r.scrollIntoView({ block: 'nearest' })));",
        );
    });
    let details = app.details.read().clone();
    let host = app.host.read();
    let work = host.work.as_ref();
    let sheet_now = details.drill.sheet();
    let view = repository(&details, work).filter(|v| worktree.as_ref() == Some(&v.worktree));
    let gauges = usage(&details, work, now_ms());
    let system = work
        .and_then(|w| w.details.system.as_ref())
        .map(|s| system_gauges(s, &usage_settings))
        .unwrap_or_default();
    let shown_kind = if worktree.is_none() {
        Worktree::None
    } else {
        kind
    };
    rsx! {
        div { class: "panel-tabs",
            for (s , keys) in Sheet::ALL.into_iter().zip(sheet_keys) {
                button {
                    key: "{s.title()}",
                    class: if s == sheet_now { "panel-tab active" } else { "panel-tab" },
                    "aria-pressed": s == sheet_now,
                    title: keys,
                    onclick: move |_| app.dispatch(Msg::DetailsSheet(s)),
                    if let Some(icon) = sheet_action(s).icon() {
                        IconView { icon, size: 13 }
                    }
                    "{s.title()}"
                }
            }
        }
        div { class: "details-scroll",
            if sheet_now == Sheet::Overview {
                Overview {
                    worktree: worktree.clone(),
                    workspace_root,
                    live_input_ms,
                    now_ms: clock(),
                    back_keys: back_keys.clone(),
                    on_kill,
                    on_menu,
                }
            }
            if details.drill.spot().is_some() {
                Repository { drill: details.drill.clone(), view, worktree: shown_kind, back_keys }
            }
        }
        Usage { gauges, agent, system }
    }
}
