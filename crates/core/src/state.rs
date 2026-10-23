//! How the window shows the work: every slice of its state, each changed
//! only by `update`.

use crate::chrome::Chrome;
use crate::details::Details;
use crate::host::Host;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewState {
    pub chrome: Chrome,
    pub host: Host,
    pub details: Details,
}
