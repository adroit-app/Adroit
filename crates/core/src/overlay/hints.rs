//! The keys an overlay names along its foot or on its buttons, each with
//! what it does, as the bindings in force have them.

use crate::actions::Action;
use crate::keymap::{Context, Keymap};
use crate::picker::{Mode, Picking, Purpose};

/// Keys and what they do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub keys: String,
    pub does: &'static str,
}

impl Hint {
    fn new(keys: impl AsRef<str>, does: &'static str) -> Self {
        Hint {
            keys: short(keys.as_ref()),
            does,
        }
    }
}

/// `keys` with the escape key named as short as a hint has room for; the
/// keymap's own name stays `Escape`.
fn short(keys: &str) -> String {
    let mut out = String::with_capacity(keys.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        out.push_str(if word == "Escape" { "Esc" } else { word });
        word.clear();
    };
    for c in keys.chars() {
        if c == '+' || c == '/' {
            flush(&mut word, &mut out);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// The first keys bound to `action` over the window, with `does`; none
/// while nothing is.
fn bound(
    keymap: &Keymap,
    contexts: &[Context],
    action: &Action,
    does: &'static str,
) -> Option<Hint> {
    keymap.keys_in(contexts, action).map(|k| Hint::new(k, does))
}

/// The keys that move down and up over the window, as one hint.
fn moving(keymap: &Keymap, does: &'static str) -> Option<Hint> {
    let down = keymap.keys_for(Context::Overlay, &Action::OverlayDown)?;
    let up = keymap.keys_for(Context::Overlay, &Action::OverlayUp)?;
    Some(Hint::new(pair(&down, &up), does))
}

/// Two keys as one: `Ctrl+n/p` for `Ctrl+n` and `Ctrl+p`, which differ only
/// in the character after their modifiers; `a/b` otherwise.
pub(crate) fn pair(a: &str, b: &str) -> String {
    let head = |k: &str| k.rsplit_once('+').map_or("", |(h, _)| h).to_string();
    let last = |k: &str| k.rsplit_once('+').map_or(k, |(_, l)| l).to_string();
    let one_char = |k: &str| last(k).chars().count() == 1;
    if !head(a).is_empty() && head(a) == head(b) && one_char(a) && one_char(b) {
        return format!("{a}/{}", last(b));
    }
    format!("{a}/{b}")
}

/// What the picker open names along its foot: the keys over the window, the
/// filer's in its file tree, and the keys of its own.
pub fn picker(keymap: &Keymap, open: &Picking) -> Vec<Hint> {
    let over = [Context::Overlay];
    let close = || bound(keymap, &over, &Action::OverlayClose, "close");
    // The keys of the picker's own are those `picker::press` reads.
    let own = |keys: &[(&str, &'static str)]| -> Vec<Hint> {
        keys.iter().map(|(k, d)| Hint::new(*k, d)).collect()
    };
    let hints: Vec<Option<Hint>> = match open {
        Picking::Go(p) => {
            let adding = p.purpose == Purpose::Add;
            let choose = if adding { "add" } else { "choose" };
            let mut there = own(&[("Ctrl+s", "shell"), ("Ctrl+e", "Neovim")]);
            if !adding {
                there.push(Hint::new("Ctrl+t", "add to tree"));
            }
            let there = there.into_iter().map(Some);
            match p.mode {
                Mode::List => [
                    moving(keymap, "move"),
                    bound(keymap, &over, &Action::OverlayChoose, choose),
                ]
                .into_iter()
                .chain(there)
                .chain([Some(Hint::new("Ctrl+f", "file tree")), close()])
                .collect(),
                Mode::Tree => {
                    let filer = [Context::Filer];
                    [
                        (Action::FilerDown, "down"),
                        (Action::FilerUp, "up"),
                        (Action::FilerInto, "open"),
                        (Action::FilerOut, "close"),
                        (Action::FilerHidden, "hidden"),
                        (Action::FilerOpen, choose),
                    ]
                    .iter()
                    .map(|(a, d)| bound(keymap, &filer, a, d))
                    .chain(there)
                    .chain([Some(Hint::new("Ctrl+f", "list")), close()])
                    .collect()
                }
            }
        }
        Picking::Resume(_) => vec![
            moving(keymap, "move"),
            bound(keymap, &over, &Action::OverlayChoose, "resume"),
            Some(Hint::new("Ctrl+a", "this directory or every one")),
            close(),
        ],
        Picking::Interrupted(_) => vec![
            moving(keymap, "move"),
            bound(keymap, &over, &Action::OverlayChoose, "bring back"),
            close(),
        ],
    };
    hints.into_iter().flatten().collect()
}

/// What the keys help names along its foot.
pub fn keys_help(keymap: &Keymap) -> Vec<Hint> {
    [
        moving(keymap, "scroll"),
        bound(keymap, &[Context::Overlay], &Action::OverlayClose, "close"),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The keys a question's buttons show: those that answer no, and yes.
pub fn answers(keymap: &Keymap) -> (Option<String>, Option<String>) {
    let question = [Context::Confirm, Context::Overlay];
    (
        keymap.keys_in(&question, &Action::OverlayClose),
        keymap.keys_in(&question, &Action::OverlayChoose),
    )
}

/// The keys the buttons of a question about a name pasted that is taken
/// show besides its no: those that rename, and overwrite.
pub fn pasting_answers(keymap: &Keymap) -> (Option<String>, Option<String>) {
    let question = [Context::Confirm, Context::Overlay];
    (
        keymap.keys_in(&question, &Action::PasteRename),
        keymap.keys_in(&question, &Action::PasteOverwrite),
    )
}

#[cfg(test)]
mod tests;
