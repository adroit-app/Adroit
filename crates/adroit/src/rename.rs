//! Editing a session name where it is shown.

use adroit_model::Session;
use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Sidebar,
    Tab,
}

/// A session being renamed, and the name typed for it so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    pub id: String,
    /// What the session is called now.
    pub was: String,
    pub text: String,
    pub place: Place,
}

impl Rename {
    /// Start renaming `session`, with its name to change.
    pub fn of(session: &Session, place: Place) -> Self {
        Self {
            id: session.id.clone(),
            was: session.name.clone(),
            text: session.name.clone(),
            place,
        }
    }

    pub fn is_for(&self, id: &str, place: Place) -> bool {
        self.id == id && self.place == place
    }

    pub fn take(editing: &mut Option<Self>, id: &str, place: Place) -> Option<Self> {
        if editing.as_ref().is_some_and(|r| r.is_for(id, place)) {
            editing.take()
        } else {
            None
        }
    }

    /// The new name typed. `None` when there is none.
    pub fn name(&self) -> Option<String> {
        let name = self.text.trim();
        (!name.is_empty() && name != self.was).then(|| name.to_string())
    }
}

#[component]
pub fn SessionName(
    id: String,
    name: String,
    place: Place,
    mut editing: Signal<Option<Rename>>,
    onstart: EventHandler<(String, Place)>,
    onfinish: EventHandler<Rename>,
) -> Element {
    let rename = editing().filter(|r| r.is_for(&id, place));
    let finish = {
        let id = id.clone();
        EventHandler::new(move |commit: bool| {
            if let Some(rename) = Rename::take(&mut editing.write(), &id, place)
                && commit
            {
                onfinish.call(rename);
            }
        })
    };
    rsx! {
        if let Some(rename) = rename {
            input {
                class: "session-rename-input",
                value: "{rename.text}",
                aria_label: "Session name",
                autocomplete: "off",
                spellcheck: "false",
                "autocorrect": "off",
                "autocapitalize": "off",
                onmounted: move |e: MountedEvent| async move {
                    let _ = e.data().set_focus(true).await;
                    document::eval("document.querySelector('.session-rename-input')?.select()");
                },
                onmousedown: move |e: Event<MouseData>| e.stop_propagation(),
                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                ondoubleclick: move |e: Event<MouseData>| e.stop_propagation(),
                oninput: move |e: Event<FormData>| {
                    if let Some(r) = editing.write().as_mut().filter(|r| r.is_for(&id, place)) { r.text = e.value(); }
                },
                onblur: move |_| finish.call(true),
                onkeydown: move |e: Event<KeyboardData>| {
                    e.stop_propagation();
                    match crate::keys::of_field(&e).map(|k| k.key).as_deref() {
                        Some("Enter") => { e.prevent_default(); finish.call(true); }
                        Some("Escape") => { e.prevent_default(); finish.call(false); }
                        _ => {}
                    }
                },
            }
        } else {
            span {
                class: "session-name",
                ondoubleclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    onstart.call((id.clone(), place));
                },
                "{name}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(name: &str) -> Session {
        Session {
            id: "a".into(),
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn only_the_name_and_surface_that_started_editing_hold_the_input() {
        let rename = Rename::of(&session("api"), Place::Tab);
        assert!(rename.is_for("a", Place::Tab));
        assert!(!rename.is_for("a", Place::Sidebar));
        assert!(!rename.is_for("other", Place::Tab));
    }

    #[test]
    fn finishing_an_old_field_cannot_submit_or_cancel_a_new_one() {
        let mut editing = Some(Rename::of(&session("api"), Place::Tab));
        assert!(Rename::take(&mut editing, "a", Place::Sidebar).is_none());
        assert!(editing.is_some());
        assert!(Rename::take(&mut editing, "other", Place::Tab).is_none());
        assert!(Rename::take(&mut editing, "a", Place::Tab).is_some());
        assert!(Rename::take(&mut editing, "a", Place::Tab).is_none());
    }

    #[test]
    fn it_starts_from_the_name_shown_and_takes_the_one_typed() {
        let mut r = Rename::of(&session("api-fix"), Place::Sidebar);
        assert_eq!(r.text, "api-fix");
        r.text = " ログイン修正 ".into();
        assert_eq!(r.name().as_deref(), Some("ログイン修正"));
    }

    #[test]
    fn an_empty_or_unchanged_name_is_no_new_name() {
        let mut r = Rename::of(&session("api"), Place::Sidebar);
        assert_eq!(r.name(), None);
        r.text = " api ".into();
        assert_eq!(r.name(), None);
        r.text = "  ".into();
        assert_eq!(r.name(), None);
    }
}
