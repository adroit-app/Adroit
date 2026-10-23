//! How the window shows the work: every slice of its state, each changed
//! only by `update`.

use crate::chrome::Chrome;
use crate::config::Config;
use crate::console::ConsoleView;
use crate::content::ContentView;
use crate::details::Details;
use crate::host::Host;
use crate::overlay::Overlays;
use crate::picker::PickerView;
use crate::prompt::PromptView;
use crate::tree::TreeView;

#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    pub chrome: Chrome,
    pub config: Config,
    pub host: Host,
    pub content: ContentView,
    pub tree: TreeView,
    pub details: Details,
    pub picker: PickerView,
    pub overlays: Overlays,
    pub console: ConsoleView,
    pub prompt: PromptView,
}

impl Default for ViewState {
    fn default() -> Self {
        let mut state = Self {
            chrome: Chrome::default(),
            config: Config::default(),
            host: Host::default(),
            content: ContentView::default(),
            tree: TreeView::default(),
            details: Details::default(),
            picker: PickerView::default(),
            overlays: Overlays::default(),
            console: ConsoleView::default(),
            prompt: PromptView::default(),
        };
        crate::details::follow_settings(
            &mut state.details,
            state.chrome.panels.layout.show_details,
            &state.config.settings.details,
        );
        // A window that shows nothing asks nothing of the host, which starts
        // out reading nothing.
        state.host.watched = crate::settle::sight(&state);
        state
    }
}
