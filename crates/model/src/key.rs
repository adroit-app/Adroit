//! A key press as the window reports it.

/// A key press as the webview reports it: `key` is the DOM `KeyboardEvent.key`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Key {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Key {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            ..Self::default()
        }
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn meta(mut self) -> Self {
        self.meta = true;
        self
    }
}
