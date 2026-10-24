use dioxus::prelude::*;

use super::classes;

/// The classes of a list row whose look `kind` names, such as `mono` or
/// `dim`, marked when `selected`.
fn row_class(kind: &str, selected: bool) -> String {
    let class = classes("list-row", kind);
    if selected {
        format!("{class} selected")
    } else {
        class
    }
}

/// One row of a list: what it shows in `children`, in the look `kind`
/// names, told to `onclick` when clicked and to `oncontextmenu` when a menu
/// is asked for on it.
#[component]
pub fn ListRow(
    #[props(default)] kind: String,
    #[props(default)] selected: bool,
    #[props(default)] style: Option<String>,
    #[props(default)] title: Option<String>,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default)] oncontextmenu: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: row_class(&kind, selected),
            style,
            title,
            onclick: move |e| {
                if let Some(onclick) = onclick {
                    onclick.call(e);
                }
            },
            oncontextmenu: move |e| {
                if let Some(oncontextmenu) = oncontextmenu {
                    oncontextmenu.call(e);
                }
            },
            {children}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_classed_by_its_look_then_by_being_selected() {
        assert_eq!(row_class("", false), "list-row");
        assert_eq!(row_class("", true), "list-row selected");
        assert_eq!(row_class("mono", false), "list-row mono");
        assert_eq!(row_class("mono", true), "list-row mono selected");
    }
}
