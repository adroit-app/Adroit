//! What the chrome draws over the window.

use adroit_core::msg::Msg;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::keymap::Keymap;
use crate::keys_help;
use crate::keys_help_view::KeysHelpView;

/// The keys help for the areas the chrome holds it open for, with the
/// bindings `keymap` holds.
#[component]
pub(crate) fn KeysHelp(keymap: Keymap) -> Element {
    let app = use_context::<AppCtx>();
    let here = app.chrome.read().keys_help.clone();
    rsx! {
        if let Some(here) = here {
            KeysHelpView {
                groups: keys_help::groups(&keymap, &here),
                onclose: move |_| app.dispatch(Msg::KeysHelpClose),
            }
        }
    }
}
