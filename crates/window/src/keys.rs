//! Keys as the window's script reports them.

use std::sync::LazyLock;

use adroit_term::Key;
use dioxus::document::EvalError;
use serde::Deserialize;

/// What the window's script tells, one JSON object a message tagged by its
/// `kind`.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    /// A key pressed, by the name the browser gives it.
    Key {
        key: String,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        meta: bool,
    },
    /// Text typed, an input method's included.
    Text {
        text: String,
    },
    Paste {
        text: String,
    },
    /// An image pasted with no text beside it: its media type and its bytes
    /// in base64.
    Image {
        #[serde(rename = "type")]
        mime: String,
        data: String,
    },
    /// What the script saw, told while keys are traced.
    Trace {
        text: String,
    },
    /// The text area the script listens on is gone from the page.
    Lost,
    /// Whether the page has the keys of the window, told as it gains or
    /// loses them.
    Focused {
        on: bool,
    },
    /// A key dropped for the input method, and whether it answered within a
    /// moment.
    Dropped {
        answered: bool,
    },
    /// Whether an input method composes in the text area, told as a
    /// composition starts and ends.
    Composing {
        on: bool,
    },
}

/// What to do with what the script sent.
#[derive(Debug, PartialEq)]
pub enum Next {
    Handle(Message),
    /// A message that cannot be read, which costs that message and no other.
    Skip,
    /// Run the script again: its text area or its channel is gone, and no key
    /// would come any more.
    Restart,
}

/// What to do with what the script sent, or the error waiting for it.
pub fn after(received: Result<Message, EvalError>) -> Next {
    match received {
        Ok(Message::Lost) => Next::Restart,
        Ok(message) => Next::Handle(message),
        Err(EvalError::Serialization(_)) => Next::Skip,
        Err(_) => Next::Restart,
    }
}

impl Message {
    /// The key a key message tells of.
    pub fn key(&self) -> Option<Key> {
        match self {
            Message::Key {
                key,
                ctrl,
                alt,
                shift,
                meta,
            } => Some(Key {
                key: key.clone(),
                ctrl: *ctrl,
                alt: *alt,
                shift: *shift,
                meta: *meta,
            }),
            _ => None,
        }
    }
}

/// Looks at whether the page has the keys while its window has them, and
/// tells when the window should give them back to it.
#[derive(Debug, Default)]
pub struct Watch {
    astray: u32,
}

impl Watch {
    /// Whether to give the keys back to the page, looking once more: the
    /// window is key, the page has not had the keys since the last look either,
    /// and no developer tools are open to take them. A single look is not
    /// enough, as the page loses the keys for a moment to what macOS opens over
    /// it.
    pub fn look(&mut self, window_key: bool, page_focused: bool, devtools: bool) -> bool {
        if !window_key || page_focused || devtools {
            self.astray = 0;
            return false;
        }
        self.astray += 1;
        if self.astray < 2 {
            return false;
        }
        self.astray = 0;
        true
    }
}

/// How many keys in a row the input method may leave unanswered before it is
/// taken to have stopped answering them.
const RESET_AFTER: u32 = 3;

/// Counts the keys the page dropped as composing that the input method did
/// not answer, with a composition event or text, within a moment.
#[derive(Debug, Default)]
pub struct Unanswered {
    run: u32,
}

/// What a key dropped as composing tells.
#[derive(Debug, PartialEq, Eq)]
pub enum Lost {
    Nothing,
    /// The input method answered again after leaving so many keys
    /// unanswered.
    Ended(u32),
    /// The input method left so many keys in a row unanswered: reset it.
    Reset(u32),
}

impl Unanswered {
    /// A key dropped as composing, `answered` by the input method or not.
    pub fn dropped(&mut self, answered: bool) -> Lost {
        if answered {
            return match std::mem::take(&mut self.run) {
                0 => Lost::Nothing,
                n => Lost::Ended(n),
            };
        }
        self.run += 1;
        if self.run < RESET_AFTER {
            return Lost::Nothing;
        }
        Lost::Reset(std::mem::take(&mut self.run))
    }
}

/// The most the lost keys' log holds before it starts afresh.
pub const LOST_LOG_LIMIT: u64 = 256 * 1024;

/// Where the window writes of the keys the input method lost: the app's
/// standard error goes nowhere, and a trace of them must outlive the window.
pub fn lost_log() -> std::path::PathBuf {
    adroit_model::paths::instance_state().join("keys.log")
}

/// Append `line` to the log at `path`, stamped with the time in milliseconds,
/// starting the log afresh once it holds `limit` bytes or more.
pub fn record(path: &std::path::Path, line: &str, limit: u64) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let full = std::fs::metadata(path).is_ok_and(|m| m.len() >= limit);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(!full)
        .write(true)
        .truncate(full)
        .open(path)?;
    writeln!(file, "{} {line}", adroit_model::time::now_ms())
}

/// Whether the way keys travel is written to the standard error, as
/// `ADROIT_DEBUG_KEYS` asks.
pub static TRACED: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("ADROIT_DEBUG_KEYS").is_some_and(|v| !v.is_empty()));

/// Write where a key went, while keys are traced; `what` is not even made
/// otherwise.
pub fn trace<D: std::fmt::Display>(what: impl FnOnce() -> D) {
    if *TRACED {
        eprintln!("[keys] {}", what());
    }
}

/// Whether the key is a modifier pressed on its own, which only changes the
/// keys that come with it.
pub fn held_alone(key: &Key) -> bool {
    matches!(
        key.key.as_str(),
        "Shift" | "Control" | "Alt" | "Meta" | "CapsLock" | "Fn"
    )
}

/// Whether a text field types or edits with `key` itself, rather than the
/// key being a command: Enter, Escape, Tab, the up and down arrows and any
/// held Control or Option are commands.
pub fn edits_text(key: &Key) -> bool {
    !key.ctrl
        && !key.alt
        && !key.meta
        && !matches!(
            key.key.as_str(),
            "Enter" | "Escape" | "Tab" | "ArrowUp" | "ArrowDown"
        )
}

/// A key pressed in a text field, or [`None`] while an input method composes
/// with it.
pub fn of_field(e: &dioxus::prelude::KeyboardData) -> Option<Key> {
    use dioxus::prelude::{Modifiers, ModifiersInteraction};
    if e.is_composing() {
        return None;
    }
    let held = e.modifiers();
    Some(Key {
        key: e.key().to_string(),
        ctrl: held.contains(Modifiers::CONTROL),
        alt: held.contains(Modifiers::ALT),
        shift: held.contains(Modifiers::SHIFT),
        meta: held.contains(Modifiers::META),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modifier_pressed_alone_is_no_key_of_its_own() {
        for name in ["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn"] {
            assert!(held_alone(&Key::new(name)), "{name}");
        }
        assert!(!held_alone(&Key::new("g")));
        assert!(!held_alone(&Key::new("c").ctrl()));
    }

    #[test]
    fn a_text_field_edits_with_plain_keys_but_for_the_ones_that_leave_or_move() {
        for edits in ["a", "あ", "Backspace", "Delete", "ArrowLeft", "Home", " "] {
            assert!(edits_text(&Key::new(edits)), "{edits}");
        }
        assert!(edits_text(&Key::new("A").shift()));
        for not in ["Enter", "Escape", "Tab", "ArrowUp", "ArrowDown"] {
            assert!(!edits_text(&Key::new(not)), "{not}");
        }
        assert!(!edits_text(&Key::new("n").ctrl()));
        assert!(!edits_text(&Key::new("f").alt()));
    }

    fn read(json: &str) -> Message {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_key_message_names_the_key_and_the_modifiers_held() {
        let m = read(r#"{"kind":"key","key":"Tab","ctrl":true,"shift":true}"#);
        assert_eq!(m.key(), Some(Key::new("Tab").ctrl().shift()));
        let m = read(r#"{"kind":"key","key":"|"}"#);
        assert_eq!(m.key(), Some(Key::new("|")));
    }

    #[test]
    fn text_and_pastes_carry_their_text() {
        assert_eq!(
            read(r#"{"kind":"text","text":"日本"}"#),
            Message::Text {
                text: "日本".into()
            }
        );
        assert_eq!(
            read(r#"{"kind":"paste","text":"a\nb"}"#),
            Message::Paste {
                text: "a\nb".into()
            }
        );
        assert_eq!(read(r#"{"kind":"text","text":"x"}"#).key(), None);
    }

    #[test]
    fn a_pasted_image_carries_its_media_type_and_its_bytes_in_base64() {
        assert_eq!(
            read(r#"{"kind":"image","type":"image/png","data":"iVBORw=="}"#),
            Message::Image {
                mime: "image/png".into(),
                data: "iVBORw==".into()
            }
        );
    }

    fn unreadable(json: &str) -> EvalError {
        EvalError::Serialization(serde_json::from_str::<Message>(json).unwrap_err())
    }

    #[test]
    fn a_message_read_is_handled() {
        let m = Message::Text { text: "a".into() };
        assert_eq!(
            after(Ok(m)),
            Next::Handle(Message::Text { text: "a".into() })
        );
    }

    #[test]
    fn a_message_that_cannot_be_read_is_passed_over_and_the_keys_after_it_still_come() {
        assert_eq!(
            after(Err(unreadable(r#"{"kind":"image","png":"…"}"#))),
            Next::Skip
        );
        assert_eq!(after(Err(unreadable(r#"{"kind":"text"}"#))), Next::Skip);
    }

    #[test]
    fn the_script_runs_again_when_its_keys_area_is_gone() {
        assert_eq!(after(Ok(read(r#"{"kind":"lost"}"#))), Next::Restart);
    }

    #[test]
    fn the_page_telling_whether_it_has_the_keys_is_read() {
        assert_eq!(
            read(r#"{"kind":"focused","on":false}"#),
            Message::Focused { on: false }
        );
    }

    #[test]
    fn the_page_telling_whether_an_input_method_composes_is_read() {
        assert_eq!(
            read(r#"{"kind":"composing","on":true}"#),
            Message::Composing { on: true }
        );
    }

    #[test]
    fn a_key_window_whose_page_stays_without_the_keys_gives_them_back_on_the_second_look() {
        let mut w = Watch::default();
        assert!(!w.look(true, false, false));
        assert!(w.look(true, false, false));
    }

    #[test]
    fn the_keys_are_given_back_once_and_then_looked_for_afresh() {
        let mut w = Watch::default();
        w.look(true, false, false);
        assert!(w.look(true, false, false));
        assert!(!w.look(true, false, false));
    }

    #[test]
    fn a_page_with_the_keys_again_between_looks_is_left_alone() {
        let mut w = Watch::default();
        w.look(true, false, false);
        assert!(!w.look(true, true, false));
        assert!(!w.look(true, false, false));
    }

    #[test]
    fn the_keys_are_not_taken_while_another_window_has_them_or_developer_tools_are_open() {
        let mut w = Watch::default();
        for _ in 0..3 {
            assert!(!w.look(false, false, false));
        }
        for _ in 0..3 {
            assert!(!w.look(true, false, true));
        }
    }

    #[test]
    fn the_script_runs_again_when_its_channel_ends() {
        assert_eq!(after(Err(EvalError::Finished)), Next::Restart);
        assert_eq!(
            after(Err(EvalError::Communication(
                "Receive channel closed".into()
            ))),
            Next::Restart
        );
    }

    #[test]
    fn the_page_telling_of_a_key_it_dropped_as_composing_is_read() {
        assert_eq!(
            read(r#"{"kind":"dropped","answered":false}"#),
            Message::Dropped { answered: false }
        );
    }

    #[test]
    fn keys_the_input_method_answers_are_typing_and_tell_nothing() {
        let mut u = Unanswered::default();
        for _ in 0..20 {
            assert_eq!(u.dropped(true), Lost::Nothing);
        }
    }

    #[test]
    fn a_few_keys_in_a_row_the_input_method_leaves_unanswered_reset_it() {
        let mut u = Unanswered::default();
        assert_eq!(u.dropped(false), Lost::Nothing);
        assert_eq!(u.dropped(false), Lost::Nothing);
        assert_eq!(u.dropped(false), Lost::Reset(3));
        // Counted afresh from the reset.
        assert_eq!(u.dropped(false), Lost::Nothing);
    }

    #[test]
    fn an_answer_ends_a_run_of_unanswered_keys_and_tells_how_many_were_lost() {
        let mut u = Unanswered::default();
        u.dropped(false);
        u.dropped(false);
        assert_eq!(u.dropped(true), Lost::Ended(2));
        assert_eq!(u.dropped(false), Lost::Nothing);
        assert_eq!(u.dropped(false), Lost::Nothing);
        assert_eq!(u.dropped(false), Lost::Reset(3));
    }

    #[test]
    fn the_lost_keys_log_keeps_each_line_stamped_and_starts_afresh_once_too_long() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep").join("keys.log");
        record(&path, "first", 64).unwrap();
        record(&path, "second", 64).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(
            lines[0].ends_with(" first") && lines[1].ends_with(" second"),
            "{text}"
        );
        record(&path, &"x".repeat(80), 64).unwrap();
        record(&path, "third", 64).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(text.trim_end().ends_with(" third"), "{text}");
    }
}
