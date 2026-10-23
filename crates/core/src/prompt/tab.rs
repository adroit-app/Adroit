//! Where Tab pressed in the prompt goes: to Neovim, or to the session for
//! the prompt it suggests.

/// What the prompt stands in when Tab is pressed there.
pub struct PromptTab {
    /// Neovim shows its completions or its command line.
    pub neovim_owns: bool,
    /// The prompt is a running session's, which the key can reach.
    pub session: bool,
    /// The prompt holds no text and no image.
    pub empty: bool,
    /// The session hands its input back through adroit's editor.
    pub hands_back: bool,
}

/// Where Tab pressed in the prompt goes.
#[derive(Debug, PartialEq, Eq)]
pub enum TabGoes {
    Neovim,
    Session,
    /// To the session, which accepts the prompt it suggests, and then the
    /// session's input to the prompt.
    SessionThenBack,
}

impl PromptTab {
    pub fn goes(&self) -> TabGoes {
        // Text being written is Neovim's to complete; only an empty prompt
        // leaves Tab nothing to do there.
        match self {
            Self {
                neovim_owns: true, ..
            }
            | Self { session: false, .. }
            | Self { empty: false, .. } => TabGoes::Neovim,
            // An agent whose input cannot be taken back still gets its Tab,
            // as it did before Tab took suggestions.
            Self {
                hands_back: false, ..
            } => TabGoes::Session,
            Self {
                hands_back: true, ..
            } => TabGoes::SessionThenBack,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_in_an_empty_prompt_takes_the_suggestion_from_a_session_that_hands_its_input_back() {
        let tab = |neovim_owns, session, empty, hands_back| {
            PromptTab {
                neovim_owns,
                session,
                empty,
                hands_back,
            }
            .goes()
        };
        assert_eq!(tab(false, true, true, true), TabGoes::SessionThenBack);
        assert_eq!(tab(false, true, true, false), TabGoes::Session);
        for hands_back in [true, false] {
            assert_eq!(tab(false, true, false, hands_back), TabGoes::Neovim);
            for empty in [true, false] {
                assert_eq!(tab(true, true, empty, hands_back), TabGoes::Neovim);
                assert_eq!(tab(false, false, empty, hands_back), TabGoes::Neovim);
            }
        }
    }
}
