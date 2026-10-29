//! The environments a user opens, independently of their running work.

use adroit_connect::profiles::{Catalog, Profile};
use dioxus::prelude::*;

pub(crate) fn active_name() -> String {
    let binding = adroit_connect::profiles::active().ok().flatten();
    binding.map_or_else(
        || "Personal".into(),
        |binding| {
            Catalog::load(&Catalog::path())
                .ok()
                .and_then(|c| c.profiles.into_iter().find(|p| p.id == binding.profile))
                .map_or(binding.name, |p| p.name)
        },
    )
}

pub(crate) fn open(profile: &Profile, independent: bool) -> Result<(), String> {
    let program = std::env::current_exe().map_err(|e| e.to_string())?;
    adroit_connect::profiles::open_command(&program, profile, independent)?
        .spawn()
        .map_err(|e| format!("Cannot open profile: {e}"))?;
    Ok(())
}

#[component]
pub(crate) fn ProfileMenu() -> Element {
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
    let active = adroit_connect::profiles::active().ok().flatten();
    let id = active
        .as_ref()
        .map_or("", |b| b.profile.as_str())
        .to_string();
    rsx! {
        div { class: "environment-switcher",
            select { key: "{opened}", aria_label: "Working profile", title: "Open a working profile", value: "{id}",
                onfocus: move |_| catalog.set(Catalog::load(&Catalog::path())),
                onchange: move |e| {
                    opened += 1;
                    let id = e.value();
                    if id == "manage" { crate::profile_manager::open(); return; }
                    if let Ok(catalog) = &*catalog.peek()
                        && let Some(profile) = catalog.profiles.iter().find(|p| p.id == id) {
                            error.set(open(profile, false).err());
                        }
                },
                if let Ok(catalog) = &*catalog.read() {
                    for profile in catalog.profiles.iter().filter(|p| !p.archived || p.id == id) {
                        option { value: "{profile.id}", selected: profile.id == id, "{profile.name}" }
                    }
                }
                option { value: "manage", "New or manage profiles…" }
            }
            if let Some(error) = error() { span { class: "environment-error", role: "alert", "{error}" } }
        }
    }
}

pub(crate) fn work_data_dir() -> Result<std::path::PathBuf, String> {
    let data = crate::paths::data_dir();
    adroit_connect::profiles::active()?.map_or(Ok(data.clone()), |binding| {
        binding.local_data_dir(&crate::paths::state_dir(), &data)
    })
}
