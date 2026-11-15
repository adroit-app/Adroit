//! The details panel inside its frame: the tabs of what it describes, the
//! components of the tab shown or the row opened from them, and the foot.

use std::path::PathBuf;
use std::time::Duration;

use adroit_core::actions::Action;
use adroit_core::msg::Msg;
use adroit_model::config::UsageSettings;
use adroit_model::time::now_ms;
use adroit_model::{Agent, Process, Session};
use dioxus::prelude::*;

use super::changes::Repository;
pub(crate) use super::changes::Worktree;
pub(crate) use super::overview::LinkMenu;
use super::overview::OpenedRow;
use super::parts::RunView;
use crate::bridge::AppCtx;
use crate::details::{parts, repository, runs, system_gauges};
use crate::icons::IconView;

/// The usage of the machine of the environment the tree is at, `shown`
/// when no row is selected, as its host last sampled it, of the meters
/// `meters` names.
pub(crate) fn machine_gauges(
    app: &AppCtx,
    shown: &adroit_model::EnvironmentId,
    meters: &UsageSettings,
) -> Vec<adroit_core::details::SystemGauge> {
    let at = app.tree.read().environment_at(shown);
    app.hosts
        .read()
        .work(&at)
        .and_then(|w| w.details.system.as_ref())
        .map(|s| system_gauges(s, meters))
        .unwrap_or_default()
}

/// The action that shows the tab at `at`, counting from 0.
pub(crate) fn tab_action(at: usize) -> Action {
    Action::DetailsTab(u8::try_from(at + 1).unwrap_or(u8::MAX))
}

#[component]
pub(crate) fn DetailsPanel(
    /// The worktree the details describe, while a window shows something.
    worktree: Option<PathBuf>,
    /// What the worktree is, for the changes and the log.
    kind: Worktree,
    /// When the shown session last took input through its console.
    live_input_ms: Option<u64>,
    /// The agent of what is shown, whose usage page its usage links.
    agent: Agent,
    /// The keys of each tab, by its place, as its title.
    tab_keys: Vec<String>,
    back_keys: String,
    on_kill: EventHandler<(Session, Process)>,
    on_menu: EventHandler<LinkMenu>,
) -> Element {
    let app = use_context::<AppCtx>();
    // What tells how long ago things were is told again now and then while
    // the panel is in sight.
    let mut clock = use_signal(now_ms);
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if app.details.peek().shown.open {
                clock.set(now_ms());
            }
        }
    });
    // Keep the row selected in view, after it is drawn.
    use_effect(move || {
        let _ = (app.details.read(), app.hosts.read());
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.details-scroll .selected')
               .forEach((r) => r.scrollIntoView({ block: 'nearest' })));",
        );
    });
    let details = app.details.read().clone();
    let environment = app.shown_environment();
    let hosts = app.hosts.read();
    let work = hosts.work(&environment);
    let tabs = details.shown.surface().tabs;
    let now = details.drill.tab().min(tabs.len().saturating_sub(1));
    let opened_repository = details.drill.spot().is_some();
    let view = repository(&details, work).filter(|v| worktree.as_ref() == Some(&v.worktree));
    let shown_kind = if worktree.is_none() {
        Worktree::None
    } else {
        kind
    };
    let (body, foot): (Vec<_>, Vec<_>) = runs(parts(&details, work))
        .into_iter()
        .partition(|r| !r.first().is_some_and(|p| p.foot));
    let opened_row = details.drill.shows_row();
    rsx! {
        if tabs.len() > 1 {
            div { class: "panel-tabs",
                for (at , tab) in tabs.into_iter().enumerate() {
                    button {
                        key: "{at}-{tab.title}",
                        class: match (at == now, tab.icon().is_some()) {
                            (true, true) => "panel-tab active",
                            (true, false) => "panel-tab active no-icon",
                            (false, true) => "panel-tab",
                            (false, false) => "panel-tab no-icon",
                        },
                        "aria-pressed": at == now,
                        title: tab_keys.get(at).cloned().unwrap_or_default(),
                        onclick: move |_| app.dispatch(Msg::DetailsTab(at)),
                        if let Some(icon) = tab.icon() {
                            IconView { icon, size: 13 }
                        }
                        span { class: "panel-tab-label", "{tab.title}" }
                    }
                }
            }
        }
        div { class: "details-scroll",
            if opened_repository {
                Repository { drill: details.drill.clone(), view, worktree: shown_kind, back_keys: back_keys.clone() }
            } else if opened_row {
                OpenedRow { back_keys: back_keys.clone(), on_kill }
            } else {
                for run in body {
                    RunView {
                        key: "{run[0].placed.component.name()}-{run[0].at}",
                        run,
                        worktree: worktree.clone(),
                        kind: shown_kind,
                        live_input_ms,
                        agent,
                        now_ms: clock(),
                        on_kill,
                        on_menu,
                    }
                }
            }
        }
        if !foot.is_empty() {
            div { class: "details-foot",
                for run in foot {
                    RunView {
                        key: "foot-{run[0].placed.component.name()}-{run[0].at}",
                        run,
                        worktree: worktree.clone(),
                        kind: shown_kind,
                        live_input_ms,
                        agent,
                        now_ms: clock(),
                        on_kill,
                        on_menu,
                    }
                }
            }
        }
    }
}
