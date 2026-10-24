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

/// How much longer a program is left after an image pasted for each
/// megabyte of it. Claude Code reads a pasted image while it goes on taking
/// keys, shrinking a large one first: an Enter that comes before it is done
/// sends the message without the image, which stays behind in the input.
const IMAGE_MS_PER_MB: u64 = 250;

/// How long a program is left after `image` is pasted, by its size.
fn image_pause(image: &std::path::Path) -> u64 {
    let bytes = std::fs::metadata(image).map_or(0, |m| m.len());
    IMAGE_MS + bytes * IMAGE_MS_PER_MB / 1_000_000
}

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
            let pastes = adapter.image_pastes(images);
            let message = if pastes.is_empty() {
                adapter.image_prompt(text, images)
            } else {
                text.to_string()
            };
            let mut typing: Vec<Typing> = pastes
                .into_iter()
                .zip(images)
                .flat_map(|(paste, image)| {
                    [
                        Typing::Paste { text: paste },
                        Typing::Pause {
                            ms: image_pause(image),
                        },
                    ]
                })
                .collect();
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
    fn each_image_is_pasted_on_its_own_before_the_words() {
        let images = vec![
            PathBuf::from("/tmp/my shot.png"),
            PathBuf::from("/tmp/two.png"),
        ];
        for agent in [Agent::Claude, Agent::Codex] {
            let composed = compose(1, agent, "look", &images);
            assert_eq!(
                composed.typing[..5],
                [
                    paste("\"/tmp/my shot.png\""),
                    Typing::Pause { ms: IMAGE_MS },
                    paste("\"/tmp/two.png\""),
                    Typing::Pause { ms: IMAGE_MS },
                    paste("look"),
                ],
                "{agent:?}"
            );
            assert_eq!(composed.typing[5..], enter(), "{agent:?}");
            let only_images = compose(1, agent, "", &images);
            assert_eq!(
                only_images.typing.len(),
                6,
                "{agent:?}: no empty message is pasted"
            );
        }
    }

    #[test]
    fn a_larger_image_is_given_longer_to_be_read_before_what_follows() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("small.png");
        let large = dir.path().join("large.png");
        std::fs::write(&small, vec![0; 1_000]).unwrap();
        std::fs::write(&large, vec![0; 3_000_000]).unwrap();
        let pause = |image: &PathBuf| {
            let typing = compose(1, Agent::Claude, "look", std::slice::from_ref(image)).typing;
            let Typing::Pause { ms } = typing[1] else {
                panic!("no pause after the image: {typing:?}")
            };
            ms
        };
        assert_eq!(pause(&small), IMAGE_MS);
        // Claude Code took half a second to read a 3 MB screenshot.
        assert!(pause(&large) >= 750, "{}", pause(&large));
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
