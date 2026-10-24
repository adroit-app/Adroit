//! Pieces of markup the windows draw alike, each drawn by one component so
//! the classes the stylesheets rule are written in one place.

mod button;
mod crumbs;
mod dialog;
mod empty;
mod facts;
mod form;
mod keys;
mod list_row;
mod marks;
mod section;
mod state_glyph;

pub use button::*;
pub use crumbs::*;
pub use dialog::*;
pub use empty::*;
pub use facts::*;
pub use form::*;
pub use keys::*;
pub use list_row::*;
pub(crate) use marks::*;
pub use section::*;
pub use state_glyph::*;

/// `base` followed by the classes in `extra`, when it names any.
fn classes(base: &str, extra: &str) -> String {
    let extra = extra.trim();
    if extra.is_empty() {
        base.to_string()
    } else {
        format!("{base} {extra}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_classes_follow_the_base_one() {
        assert_eq!(classes("dialog", "picker-dialog"), "dialog picker-dialog");
    }

    #[test]
    fn no_extra_class_leaves_the_base_one_alone() {
        assert_eq!(classes("dialog", ""), "dialog");
        assert_eq!(classes("dialog", " "), "dialog");
    }
}
