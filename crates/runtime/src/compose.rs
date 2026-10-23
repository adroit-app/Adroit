//! How a prompt is typed into a session, as its agent takes it: a line it
//! runs as a shell command after `!`, and the images attached as the agent
//! takes them, pasted or named in the prompt.

use std::path::PathBuf;

use adroit_model::Agent;
use adroit_protocol::{Composed, Typing};

/// How long a program is left after `!` before the command is pasted, and
/// after the prompt before Enter.
const SETTLE_MS: u64 = 40;

/// How long a program is left after each image pasted.
const IMAGE_MS: u64 = 80;

/// How `text`, with `images` attached, is typed into a session of `agent`,
/// for the request numbered `asked`.
pub fn compose(asked: u64, agent: Agent, text: &str, images: &[PathBuf]) -> Composed {
    let adapter = adroit_agents::adapter(agent);
    let command =
        adroit_agents::claude::shell_line(text).filter(|_| adapter.capabilities().shell_bang);
    let mut typing = match command {
        Some(command) => vec![
            Typing::Text { text: "!".into() },
            Typing::Pause { ms: SETTLE_MS },
            Typing::Paste {
                text: command.into(),
            },
        ],
        None => {
            let mut typing: Vec<Typing> = adapter
                .image_pastes(images)
                .into_iter()
                .flat_map(|image| {
                    [
                        Typing::Paste { text: image },
                        Typing::Pause { ms: IMAGE_MS },
                    ]
                })
                .collect();
            let message = adapter.image_prompt(text, images);
            if !message.is_empty() {
                typing.push(Typing::Paste { text: message });
            }
            typing
        }
    };
    typing.extend([
        Typing::Pause { ms: SETTLE_MS },
        Typing::Key {
            name: "Enter".into(),
        },
    ]);
    Composed {
        asked,
        shell: command.is_some(),
        image_missing: images.iter().any(|path| !path.is_file()),
        typing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paste(text: &str) -> Typing {
        Typing::Paste { text: text.into() }
    }

    fn enter() -> [Typing; 2] {
        [
            Typing::Pause { ms: SETTLE_MS },
            Typing::Key {
                name: "Enter".into(),
            },
        ]
    }

    #[test]
    fn claude_runs_a_line_after_a_bang_as_a_shell_command_and_codex_takes_it_as_a_message() {
        let claude = compose(1, Agent::Claude, "!ls -a", &[]);
        assert!(claude.shell);
        assert_eq!(
            claude.typing[..3],
            [
                Typing::Text { text: "!".into() },
                Typing::Pause { ms: SETTLE_MS },
                paste("ls -a"),
            ]
        );
        assert_eq!(claude.typing[3..], enter());
        let codex = compose(2, Agent::Codex, "!ls -a", &[]);
        assert!(!codex.shell);
        assert_eq!(codex.typing[0], paste("!ls -a"));
        assert_eq!(codex.asked, 2);
    }

    #[test]
    fn claude_names_the_images_in_its_prompt_and_codex_has_them_pasted_first() {
        let images = vec![PathBuf::from("/tmp/my shot.png")];
        let claude = compose(1, Agent::Claude, "look", &images);
        assert_eq!(claude.typing[0], paste("look @\"/tmp/my shot.png\""));
        assert_eq!(claude.typing[1..], enter());
        let codex = compose(1, Agent::Codex, "look", &images);
        assert_eq!(
            codex.typing[..3],
            [
                paste("\"/tmp/my shot.png\""),
                Typing::Pause { ms: IMAGE_MS },
                paste("look"),
            ]
        );
        let only_images = compose(1, Agent::Codex, "", &images);
        assert_eq!(only_images.typing.len(), 4, "no empty message is pasted");
    }

    #[test]
    fn an_image_no_longer_on_disk_is_told_of() {
        let dir = tempfile::tempdir().unwrap();
        let kept = dir.path().join("kept.png");
        std::fs::write(&kept, b"png").unwrap();
        let gone = dir.path().join("gone.png");
        assert!(!compose(1, Agent::Claude, "see", std::slice::from_ref(&kept)).image_missing);
        assert!(compose(1, Agent::Claude, "see", &[kept, gone]).image_missing);
    }
}
