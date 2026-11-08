//! Why an agent's screen was read as it was: the rule that decided, the rows
//! it matched, or the default taken when no rule's sign was on the screen.

/// What a reading of a screen told, with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement<T> {
    /// What the screen tells; none when it tells nothing.
    pub value: Option<T>,
    pub verdict: Verdict,
}

/// The rule a reading of a screen went by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// The rule's name, `<agent>.<rule>`, as in `claude.spinner_above_input`.
    pub rule: String,
    /// Whether no rule's sign was on the screen and `rule` names the default
    /// taken instead.
    pub default: bool,
    /// The rows the rule went by.
    pub evidence: Vec<Evidence>,
}

/// One row of a screen a rule went by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// Where the row is on the screen, from its top.
    pub row: usize,
    pub text: String,
}

impl Verdict {
    /// The rule `rule`, whose sign `rows` of `screen` show.
    pub fn matched<S: AsRef<str>>(
        rule: impl Into<String>,
        screen: &[S],
        rows: impl IntoIterator<Item = usize>,
    ) -> Self {
        Self {
            rule: rule.into(),
            default: false,
            evidence: evidence(screen, rows),
        }
    }

    /// The default `rule`, taken by `rows` of `screen` where no rule's sign
    /// is shown.
    pub fn defaulted<S: AsRef<str>>(
        rule: impl Into<String>,
        screen: &[S],
        rows: impl IntoIterator<Item = usize>,
    ) -> Self {
        Self {
            default: true,
            ..Self::matched(rule, screen, rows)
        }
    }
}

fn evidence<S: AsRef<str>>(screen: &[S], rows: impl IntoIterator<Item = usize>) -> Vec<Evidence> {
    rows.into_iter()
        .filter_map(|row| {
            screen.get(row).map(|text| Evidence {
                row,
                text: text.as_ref().trim_end().to_string(),
            })
        })
        .collect()
}

impl<T> Judgement<T> {
    pub fn new(value: Option<T>, verdict: Verdict) -> Self {
        Self { value, verdict }
    }

    /// What `then` makes of what was told, by the same rule.
    pub fn then<U>(self, then: impl FnOnce(T) -> Option<U>) -> Judgement<U> {
        Judgement {
            value: self.value.and_then(then),
            verdict: self.verdict,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verdict_keeps_the_rows_it_went_by_and_skips_those_off_the_screen() {
        let screen = ["a  ", "b"];
        assert_eq!(
            Verdict::matched("x.y", &screen, [0, 5]),
            Verdict {
                rule: "x.y".into(),
                default: false,
                evidence: vec![Evidence {
                    row: 0,
                    text: "a".into()
                }],
            }
        );
        assert!(Verdict::defaulted("x.z", &screen, []).default);
    }

    #[test]
    fn what_is_made_of_a_judgement_keeps_its_verdict() {
        let judged = Judgement::new(Some(2), Verdict::defaulted("x.y", &[""], []));
        let made = judged.clone().then(|n| (n > 1).then_some("big"));
        assert_eq!(made.value, Some("big"));
        assert_eq!(made.verdict, judged.verdict);
        assert_eq!(judged.then(|_| None::<()>).value, None);
    }
}
