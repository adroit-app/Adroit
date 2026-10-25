use adroit_core::config::Config;
use dioxus::prelude::*;

/// Why the settings cannot be had, while they cannot, and what came of
/// reading them that the user is to know of, as `config` reads now.
#[component]
pub fn SettingsFileError(config: Signal<Config>) -> Element {
    rsx! {
        SettingsLoadError { error: config.read().settings_error.clone() }
        SettingsNotice { notice: adroit_core::profiles::unshared_notice(&config.read().settings) }
    }
}

/// `notice`, what came of reading the settings that the user is to know of,
/// told over the window.
#[component]
pub fn SettingsNotice(notice: Option<String>) -> Element {
    rsx! {
        if let Some(notice) = notice {
            Banner { role: "status", colour: "var(--wait)", text: notice }
        }
    }
}

/// `error`, why the settings cannot be had, told over the window.
#[component]
pub fn SettingsLoadError(error: Option<String>) -> Element {
    rsx! {
        if let Some(error) = error {
            Banner {
                role: "alert",
                colour: "var(--fail, #f08080)",
                text: format!("{error}. Fix the configuration file before saving settings."),
            }
        }
    }
}

/// `text` told over the window in `colour`, as `role` says it.
#[component]
fn Banner(role: &'static str, colour: &'static str, text: String) -> Element {
    rsx! {
        div {
            class: "settings-error",
            role,
            style: "padding: 12px; color: {colour}; white-space: pre-wrap; flex: none; max-height: 30vh; overflow: auto; user-select: text",
            "{text}"
        }
    }
}
