//! How the window shows the work: every slice of its state, each changed
//! only by `update`.

use crate::chrome::Chrome;
use crate::details::Details;
use crate::host::Host;
use crate::tree::TreeView;

#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    pub chrome: Chrome,
    pub host: Host,
    pub tree: TreeView,
    pub details: Details,
}

impl Default for ViewState {
    fn default() -> Self {
        let mut state = Self {
            chrome: Chrome::default(),
            host: Host::default(),
            tree: TreeView::default(),
            details: Details::default(),
        };
        // A window that shows nothing asks nothing of the host, which starts
        // out reading nothing.
        state.host.watched = crate::settle::sight(&state);
        state
    }
}
