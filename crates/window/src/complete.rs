//! What `@` and `/` in the prompt offer.

use adroit_model::Command;
use adroit_nvim::Candidate;

pub use adroit_core::complete::*;

/// What `@` offers, as the prompt's Neovim completes it.
pub fn offered(offers: &[Offer]) -> Vec<Candidate> {
    offers
        .iter()
        .map(|o| Candidate::new(o.word.clone(), o.kind))
        .collect()
}

/// The most characters of a command's description shown beside it.
const DESCRIPTION_SHOWN: usize = 60;

/// What `/` offers of an agent's commands: each by name, what it does
/// shortened beside it, and while picked, its arguments and what it does whole.
pub fn commands(list: &[Command]) -> Vec<Candidate> {
    list.iter()
        .map(|c| {
            let word = if c.name.starts_with('$') {
                c.name.clone()
            } else {
                format!("/{}", c.name)
            };
            let long = c.description.chars().count() > DESCRIPTION_SHOWN;
            let menu = if long {
                let kept: String = c.description.chars().take(DESCRIPTION_SHOWN - 1).collect();
                format!("{}…", kept.trim_end())
            } else {
                c.description.clone()
            };
            let usage =
                (!c.argument_hint.is_empty()).then(|| format!("{word} {}", c.argument_hint));
            let info = match (usage, long) {
                (Some(usage), true) => format!("{usage} — {}", c.description),
                (Some(usage), false) => usage,
                (None, true) => c.description.clone(),
                (None, false) => String::new(),
            };
            Candidate::new(word, menu).with_info(info)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_skills_keep_the_dollar_invocation_in_the_completion() {
        let got = commands(&[command("$deploy", "Deploy the service", "")]);
        assert_eq!(got[0].word, "$deploy");
    }

    fn command(name: &str, description: &str, argument_hint: &str) -> Command {
        Command {
            name: name.into(),
            description: description.into(),
            argument_hint: argument_hint.into(),
        }
    }

    #[test]
    fn a_command_is_offered_by_its_name_with_what_it_does_beside_it() {
        let got = commands(&[command("compact", "Clear history but keep a summary", "")]);
        assert_eq!(
            got,
            [Candidate::new(
                "/compact",
                "Clear history but keep a summary"
            )]
        );
    }

    #[test]
    fn a_long_description_is_shortened_beside_and_whole_while_picked() {
        let long = "あ".repeat(80);
        let got = commands(&[command("ja", &long, "")]);
        assert_eq!(got[0].menu.chars().count(), DESCRIPTION_SHOWN);
        assert!(got[0].menu.ends_with('…'));
        assert_eq!(got[0].info, long);
    }

    #[test]
    fn the_arguments_a_command_takes_are_shown_while_it_is_picked() {
        let got = commands(&[command("review", "Review a pull request", "[pr]")]);
        assert_eq!(got[0].info, "/review [pr]");
        let got = commands(&[command("ja", &"x".repeat(80), "[pr]")]);
        assert_eq!(got[0].info, format!("/ja [pr] — {}", "x".repeat(80)));
    }

    #[tokio::test]
    async fn codex_slash_commands_reach_the_embedded_prompt() {
        let list = [command(
            "model",
            "choose what model and reasoning effort to use",
            "",
        )];
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let editor = adroit_nvim::Embedded::spawn(
            &adroit_nvim::program(),
            &["-u", "NONE", "-i", "NONE", "--noplugin"],
            80,
            10,
            move |event| {
                let _ = tx.send(event);
            },
        )
        .await
        .unwrap();
        editor.set_commands(&commands(&list)).await.unwrap();
        editor.input("/mo").await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if editor
                    .state()
                    .popup()
                    .is_some_and(|p| p.items.iter().any(|c| c.word == "/model"))
                {
                    break;
                }
                rx.recv().await.expect("editor event");
            }
        })
        .await
        .expect("/model completion");
        editor.input("<C-y>").await.unwrap();
        assert_eq!(editor.take_text().await.unwrap(), "/model ");
    }
}
