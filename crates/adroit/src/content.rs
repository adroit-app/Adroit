//! The content as the window draws it: the tabs bar, the windows of the
//! shown tab with the borders between them, and what each kind of window
//! shows, read from the content slice and what the work host listed.

mod bodies;
mod tabs_bar;
mod view;
mod window;

pub(crate) use adroit_core::content::*;
pub(crate) use view::*;

fn short(path: &std::path::Path) -> String {
    adroit_core::picker::short(path, &crate::paths::home())
}
