//! What the conversation of the session shown says: its stats, read on every
//! sheet for the usage at the panel's foot, and, while the overview is shown,
//! its links, the small copies of its images, and what it told other
//! sessions through Adroit.

use std::collections::BTreeMap;
use std::time::Duration;

use adroit_agents::ConversationReader;
use adroit_model::links::Links;
use adroit_model::{ConversationStats, Session};
use adroit_protocol::{ConversationRead, DetailsChange};

use super::Watched;
use crate::pictures::{self, Thumbs};

type Reader = Box<dyn ConversationReader>;

pub(super) async fn watch(mut watched: Watched) {
    let mut held: Option<(Key, Reader)> = None;
    let mut thumbs = Thumbs::new();
    loop {
        let seen = watched.seen();
        match seen.session.filter(has_transcript) {
            Some(s) => {
                let key = Key::of(&s);
                let reader = match held.take() {
                    Some((was, reader)) if was == key => Some(reader),
                    _ => adroit_agents::adapter(s.agent).transcript(
                        &crate::programs::agents(),
                        &crate::paths::home(),
                        &s,
                    ),
                };
                if let Some(reader) = reader {
                    let overview = seen.sight.overview;
                    let mut kept = std::mem::take(&mut thumbs);
                    let read = tokio::task::spawn_blocking(move || {
                        let mut reader = reader;
                        let (links, stats) = sample_conversation(&mut *reader, overview);
                        let links = links.map(|mut links| {
                            pictures::refresh_thumbs(&mut links.images, &mut kept);
                            links
                        });
                        (reader, links, stats, kept)
                    })
                    .await;
                    if let Ok((reader, links, stats, kept)) = read {
                        thumbs = kept;
                        held = Some((key, reader));
                        let mut changes = vec![DetailsChange::Conversation {
                            session: s.id.clone(),
                            conversation: ConversationRead {
                                id: s.session_id.clone(),
                                stats,
                            },
                        }];
                        if let Some(links) = links {
                            changes.extend(overview_changes(&s, links, &thumbs).await);
                        }
                        watched.record(changes);
                    }
                }
            }
            None => held = None,
        }
        watched
            .wait(Duration::from_secs(2), |s| {
                (s.session.as_ref().map(Key::of), s.sight.overview)
            })
            .await;
    }
}

/// What the overview shows of `session` besides its stats: its `links`,
/// with the sessions it messaged through Adroit among them, the small copies
/// of its images in `thumbs`, and the messages themselves.
async fn overview_changes(
    session: &Session,
    mut links: Links,
    thumbs: &Thumbs,
) -> Vec<DetailsChange> {
    let mut changes = Vec::new();
    if let Some(record) = session.delegation {
        let root = crate::delegation::app::root();
        links.peers = crate::delegation::app::peers_at(&root, record).await;
        changes.push(DetailsChange::Notes {
            session: session.id.clone(),
            notes: crate::delegation::app::notes_at(&root, record).await,
        });
    }
    let shown: BTreeMap<_, _> = links
        .images
        .iter()
        .filter_map(|f| {
            let (_, thumb) = thumbs.get(&f.path)?;
            Some((f.path.clone(), thumb.clone()))
        })
        .collect();
    changes.push(DetailsChange::Thumbs {
        session: session.id.clone(),
        thumbs: shown,
    });
    changes.push(DetailsChange::Links {
        session: session.id.clone(),
        links,
    });
    changes
}

/// Which conversation a reader reads: it changes when the session goes on
/// in another conversation, or another directory.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    agent: adroit_model::Agent,
    cwd: std::path::PathBuf,
    session_id: String,
}

impl Key {
    fn of(session: &Session) -> Key {
        Key {
            agent: session.agent,
            cwd: session.cwd.clone(),
            session_id: session.session_id.clone(),
        }
    }
}

/// Whether `session`'s agent keeps a transcript the host can read, and the
/// conversation has told its id yet.
fn has_transcript(session: &Session) -> bool {
    adroit_agents::adapter(session.agent)
        .capabilities()
        .transcript
        && !session.session_id.is_empty()
}

/// What `reader` says of its conversation: its links only while the overview,
/// which alone lists them, is shown.
fn sample_conversation(
    reader: &mut dyn ConversationReader,
    overview: bool,
) -> (Option<Links>, ConversationStats) {
    let links = reader.read();
    let stats = reader.stats();
    (overview.then_some(links), stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_stats_refresh_on_changes_and_log_without_overview_links() {
        struct Reader(u64);

        impl ConversationReader for Reader {
            fn read(&mut self) -> Links {
                self.0 += 1;
                Links::default()
            }

            fn stats(&self) -> ConversationStats {
                ConversationStats {
                    context: Some((self.0, 100)),
                    ..Default::default()
                }
            }
        }

        let mut reader = Reader(0);
        for expected in [1, 2] {
            let (links, stats) = sample_conversation(&mut reader, false);
            assert!(links.is_none(), "only Overview needs conversation links");
            assert_eq!(stats.context, Some((expected, 100)));
        }
        assert!(sample_conversation(&mut reader, true).0.is_some());
    }

    #[test]
    fn conversation_sampling_follows_the_selected_session_and_its_transcript_id() {
        let mut session = Session {
            id: "shown".into(),
            session_id: "first".into(),
            cwd: "/project".into(),
            ..Default::default()
        };
        let shown = session.clone();
        assert!(has_transcript(&shown), "selected session has a transcript");
        assert!(!has_transcript(&Session {
            session_id: String::new(),
            ..shown.clone()
        }));
        assert_eq!(Key::of(&shown), Key::of(&session));

        session.session_id = "resumed".into();
        assert_ne!(Key::of(&shown), Key::of(&session));
        session.cwd = "/other".into();
        assert_ne!(Key::of(&shown), Key::of(&session));
    }
}
