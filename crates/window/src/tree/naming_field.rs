//! The field a new file's or directory's name is typed in, drawn first
//! among the entries of the directory it is made in.

use adroit_core::msg::Msg;
use adroit_core::overlay::naming::{Kind, Naming};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::{FILE_INDENT, FileGlyph, InlineField, ListRow};
use crate::file_icons;

#[component]
pub(crate) fn NamingField(naming: Naming, depth: usize) -> Element {
    let app = use_context::<AppCtx>();
    let kind = match naming.kind {
        Kind::Directory => file_icons::Kind::Directory { open: false },
        Kind::File => file_icons::Kind::File,
    };
    // The icon follows the name typed, as the row it becomes will draw it.
    let icon = file_icons::find(naming.dir.join(naming.text.trim()), kind);
    let colour = icon
        .colour
        .map(|c| format!("color:{}", c.css()))
        .unwrap_or_default();
    let label = match naming.kind {
        _ if naming.pasting.is_some() => "Name to paste by",
        Kind::Directory => "New directory name",
        Kind::File => "New file name",
    };
    rsx! {
        ListRow {
            kind: "file naming",
            style: "padding-left:{8 + FILE_INDENT * depth}px",
            FileGlyph { glyph: icon.glyph, style: colour }
            InlineField {
                class: "session-rename-input file-naming-input",
                value: naming.text,
                aria_label: label,
                placeholder: "a name, or a/path",
                focus: true,
                contained: true,
                oninput: move |text| app.dispatch(Msg::NamingText(text)),
                onenter: move |()| app.dispatch(Msg::NamingEnd { commit: true }),
                onescape: move |()| app.dispatch(Msg::NamingEnd { commit: false }),
                // A click elsewhere is no reason to make a file.
                onblur: move |()| app.dispatch(Msg::NamingEnd { commit: false }),
            }
        }
    }
}
