//! The environments a user opens, independently of their running work.

use adroit_connect::environments::{Catalog, Environment};
use dioxus::prelude::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub(crate) fn active_name() -> String {
    let binding = adroit_connect::environments::active().ok().flatten();
    binding.map_or_else(
        || "Personal".into(),
        |binding| {
            Catalog::load(&Catalog::path())
                .ok()
                .and_then(|c| {
                    c.environments
                        .into_iter()
                        .find(|p| p.id == binding.environment)
                })
                .map_or(binding.name, |p| p.name)
        },
    )
}

thread_local! {
    /// Where an environment asked for is shown: the window this process
    /// holds, once it listens.
    static SHOWING: std::cell::RefCell<Option<UnboundedSender<Environment>>> =
        const { std::cell::RefCell::new(None) };
}

/// The environments asked to be shown in this process's window, from its
/// menu or the Environments window, which has no hold of the window.
pub(crate) fn listen() -> UnboundedReceiver<Environment> {
    let (asked, showing) = tokio::sync::mpsc::unbounded_channel();
    SHOWING.with(|s| *s.borrow_mut() = Some(asked));
    showing
}

/// Show `environment` in this process's window beside the environments it
/// shows.
pub(crate) fn show(environment: &Environment) -> Result<(), String> {
    let sent = SHOWING.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|asked| asked.send(environment.clone()).is_ok())
    });
    sent.then_some(())
        .ok_or_else(|| "No window can show the environment".into())
}

/// Open `environment` in a window of its own, in a new independent
/// instance when `independent`.
pub(crate) fn open(environment: &Environment, independent: bool) -> Result<(), String> {
    let program = std::env::current_exe().map_err(|e| e.to_string())?;
    adroit_connect::environments::open_command(&program, environment, independent)?
        .spawn()
        .map_err(|e| format!("Cannot open environment: {e}"))?;
    Ok(())
}

#[component]
pub(crate) fn EnvironmentMenu() -> Element {
    let mut catalog = use_signal(|| Catalog::load(&Catalog::path()));
    let mut error = use_signal(|| None::<String>);
    let mut opened = use_signal(|| 0_u64);
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let next = Catalog::load(&Catalog::path());
            if *catalog.peek() != next {
                catalog.set(next);
            }
        }
    });
    let active = adroit_connect::environments::active().ok().flatten();
    let id = active
        .as_ref()
        .map_or("", |b| b.environment.as_str())
        .to_string();
    rsx! {
        div { class: "environment-switcher",
            select { key: "{opened}", aria_label: "Working environment", title: "Open a working environment", value: "{id}",
                onfocus: move |_| catalog.set(Catalog::load(&Catalog::path())),
                onchange: move |e| {
                    opened += 1;
                    let id = e.value();
                    if id == "manage" { crate::environment_manager::open(); return; }
                    if let Ok(catalog) = &*catalog.peek()
                        && let Some(environment) = catalog.environments.iter().find(|p| p.id == id) {
                            error.set(show(environment).err());
                        }
                },
                if let Ok(catalog) = &*catalog.read() {
                    for environment in catalog.environments.iter().filter(|p| !p.archived || p.id == id) {
                        option { value: "{environment.id}", selected: environment.id == id, "{environment.name}" }
                    }
                }
                option { value: "manage", "New or manage environments…" }
            }
            if let Some(error) = error() { span { class: "environment-error", role: "alert", "{error}" } }
        }
    }
}

/// Where the work of `environment`, one a window shows beside its own, keeps
/// its data on this computer.
pub(crate) fn data_dir_of(
    environment: &adroit_model::EnvironmentId,
) -> Result<std::path::PathBuf, String> {
    let catalog = Catalog::load(&Catalog::path())?;
    let shown = catalog
        .environments
        .iter()
        .find(|e| e.id == environment.to_string())
        .ok_or("The environment is no longer listed")?;
    shown
        .bind(&adroit_model::paths::instance())?
        .local_data_dir(&crate::paths::state_dir(), &crate::paths::data_dir())
}

pub(crate) fn work_data_dir() -> Result<std::path::PathBuf, String> {
    let data = crate::paths::data_dir();
    adroit_connect::environments::active()?.map_or(Ok(data.clone()), |binding| {
        binding.local_data_dir(&crate::paths::state_dir(), &data)
    })
}
