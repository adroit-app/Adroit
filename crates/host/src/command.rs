//! A CLI result can carry effects even when its native reply was lost.

pub(crate) struct Output {
    pub wrote: Result<String, String>,
    pub code: i32,
    pub effects_possible: bool,
}

impl Output {
    pub fn answer(self, remote: bool) -> adroit_protocol::Answer {
        use adroit_protocol::Answer;
        if remote
            && self.effects_possible
            && let Err(text) = &self.wrote
        {
            return Answer::Unconfirmed { text: text.clone() };
        }
        let (stdout, stderr) = match self.wrote {
            Ok(text) => (text, String::new()),
            Err(text) => (String::new(), text),
        };
        Answer::Ran {
            stdout,
            stderr,
            code: self.code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_protocol::Answer;

    #[test]
    fn a_failed_effectful_command_does_not_become_a_confirmed_remote_exit() {
        let result = || Output {
            wrote: Err("lost terminal reply".into()),
            code: 1,
            effects_possible: true,
        };
        assert!(matches!(result().answer(true), Answer::Unconfirmed { .. }));
        assert!(matches!(
            result().answer(false),
            Answer::Ran { code: 1, .. }
        ));
        assert!(matches!(
            Output {
                effects_possible: false,
                ..result()
            }
            .answer(true),
            Answer::Ran { code: 1, .. }
        ));
        assert!(matches!(
            Output {
                wrote: Ok("started".into()),
                code: 0,
                effects_possible: true
            }
            .answer(true),
            Answer::Ran { code: 0, .. }
        ));
    }
}
