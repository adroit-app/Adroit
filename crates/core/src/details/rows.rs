//! The overview's rows as the panel draws them: grouped under their
//! headings, each with its icon, label and tag.

use std::path::Path;
use std::sync::Arc;

use adroit_model::links::{Kind as LinkKind, Touch};
use adroit_model::subagents::Standing;

use super::{Entry, Group, Source, Tie, shown_agent, shown_link, shown_shot};
use crate::icons::Icon;

/// How a subagent is marked, by how it stands: the class that colours its
/// icon as a session's in the same standing is, and the icon.
pub fn agent_glyph(standing: Standing) -> (&'static str, Icon) {
    match standing {
        Standing::Running => ("state-glyph run", Icon::Loader),
        Standing::Done => ("state-glyph done", Icon::Check),
        Standing::Failed => ("state-glyph fail", Icon::AlertTriangle),
        Standing::Stopped => ("state-glyph idle", Icon::PlayerStop),
    }
}

/// A link as the overview draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRow {
    /// Its place among the overview's rows.
    pub at: usize,
    pub icon: Icon,
    /// The class that colours its icon; empty for the panel's own colour.
    pub tone: &'static str,
    pub label: String,
    /// A chip after its label, as a subagent's kind.
    pub badge: Option<String>,
    pub tag: String,
    pub target: String,
    pub file: bool,
    /// A small copy of an image, drawn in place of the icon.
    pub thumb: Option<Arc<str>>,
}

/// A group of the overview's links as drawn: its heading, at its place among
/// the overview's rows, and the links shown under it.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkSection {
    pub group: Group,
    pub at: usize,
    pub count: usize,
    pub folded: bool,
    pub rows: Vec<LinkRow>,
    /// The row for the links not shown: its place and how many they are.
    pub more: Option<(usize, usize)>,
}

/// The overview's `entries` as groups under their headings, paths shown
/// from `worktree` or `home`, times told at `now_ms`.
pub fn link_sections(
    entries: &[Entry],
    worktree: &Path,
    home: &Path,
    now_ms: u64,
) -> Vec<LinkSection> {
    let mut out: Vec<LinkSection> = Vec::new();
    for (at, entry) in entries.iter().enumerate() {
        match entry {
            Entry::More { hidden, .. } => {
                if let Some(section) = out.last_mut() {
                    section.more = Some((at, *hidden));
                }
            }
            Entry::Heading {
                group,
                count,
                folded,
            } => out.push(LinkSection {
                group: *group,
                at,
                count: *count,
                folded: *folded,
                rows: Vec::new(),
                more: None,
            }),
            Entry::Session(r) => {
                let icon = if r
                    .ties
                    .iter()
                    .any(|t| matches!(t, Tie::Opened | Tie::OpenedBy))
                {
                    Icon::MessagePlus
                } else {
                    Icon::Message
                };
                let mut tag: Vec<&str> = r.ties.iter().map(|t| t.word()).collect();
                if r.id.is_none() {
                    tag.push("not running");
                }
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon,
                        tone: "",
                        label: r.name.clone(),
                        badge: None,
                        tag: tag.join(" · "),
                        target: r.name.clone(),
                        file: false,
                        thumb: None,
                    });
                }
            }
            Entry::Image(shot) => {
                let (label, tag) = shown_shot(shot, worktree, home);
                let (target, file) = match &shot.source {
                    Source::File { path, .. } => (path.clone(), true),
                    Source::Console { .. } => (label.clone(), false),
                };
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon: Icon::Photo,
                        tone: "",
                        label,
                        badge: None,
                        tag,
                        target,
                        file,
                        thumb: shot.thumb.clone(),
                    });
                }
            }
            Entry::Process(p) => {
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon: Icon::Terminal2,
                        tone: if p.zombie { "state-glyph fail" } else { "" },
                        label: p.name.clone(),
                        badge: None,
                        tag: format!(
                            "{} · {:.0} MiB{}",
                            p.pid,
                            p.memory as f64 / (1024.0 * 1024.0),
                            if p.zombie { " · zombie" } else { "" }
                        ),
                        target: format!("PID {} · {}", p.pid, p.command),
                        file: false,
                        thumb: None,
                    });
                }
            }
            Entry::Fact(f) => {
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon: Icon::InfoCircle,
                        tone: "",
                        label: f.label.to_string(),
                        badge: None,
                        tag: f.value.clone(),
                        target: f.value.clone(),
                        file: false,
                        thumb: None,
                    });
                }
            }
            Entry::Message(n) => {
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon: Icon::Message,
                        tone: "",
                        label: n.summary().to_owned(),
                        badge: None,
                        tag: n.tag(now_ms),
                        target: n.body.clone(),
                        file: false,
                        thumb: None,
                    });
                }
            }
            Entry::Agent(a) => {
                let (label, tag) = shown_agent(a, now_ms);
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon: agent_glyph(a.standing).1,
                        tone: agent_glyph(a.standing).0,
                        label,
                        badge: a.kind.clone(),
                        tag,
                        target: a.call.clone(),
                        file: false,
                        thumb: None,
                    });
                }
            }
            Entry::Link(link) => {
                let icon = match link.kind {
                    LinkKind::Artifact => Icon::AppWindow,
                    LinkKind::Url => Icon::Link,
                    LinkKind::File(Touch::Edited) => Icon::FilePencil,
                    LinkKind::File(_) => Icon::File,
                };
                let (label, tag) = shown_link(link, worktree, home);
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon,
                        tone: "",
                        label,
                        badge: None,
                        tag,
                        target: link.target.clone(),
                        file: matches!(link.kind, LinkKind::File(_)),
                        thumb: None,
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use adroit_model::links::Link;

    use super::*;
    use crate::details::{Fact, Related, Shot, about, overview, with_agents, with_images};

    fn related_to(name: &str, id: Option<&str>, ties: &[Tie]) -> Related {
        Related {
            name: name.into(),
            id: id.map(str::to_string),
            ties: ties.to_vec(),
        }
    }

    fn link(kind: LinkKind, target: &str) -> Link {
        Link {
            kind,
            target: target.into(),
            title: None,
            line: None,
        }
    }

    #[test]
    fn the_overview_heads_each_group_of_links_and_marks_a_file_edited() {
        let rows = [
            link(LinkKind::Artifact, "https://claude.ai/artifact/a"),
            link(LinkKind::Url, "https://a.dev"),
            link(LinkKind::Url, "https://b.dev"),
            link(LinkKind::File(Touch::Edited), "/w/a.rs"),
            link(LinkKind::File(Touch::Read), "/w/b.rs"),
        ];
        let drawn = |folded: &HashSet<Group>| -> Vec<_> {
            link_sections(
                &overview(&[], &rows, folded),
                Path::new("/w"),
                Path::new("/h"),
                0,
            )
            .into_iter()
            .map(|s| {
                let rows: Vec<_> = s.rows.iter().map(|r| (r.at, r.icon)).collect();
                (s.group, s.at, s.count, s.folded, rows)
            })
            .collect()
        };
        assert_eq!(
            drawn(&HashSet::new()),
            [
                (Group::Artifacts, 0, 1, false, vec![(1, Icon::AppWindow)]),
                (
                    Group::Links,
                    2,
                    2,
                    false,
                    vec![(3, Icon::Link), (4, Icon::Link)]
                ),
                (
                    Group::Files,
                    5,
                    2,
                    false,
                    vec![(6, Icon::FilePencil), (7, Icon::File)]
                ),
            ]
        );
        assert_eq!(
            drawn(&HashSet::from([Group::Links])),
            [
                (Group::Artifacts, 0, 1, false, vec![(1, Icon::AppWindow)]),
                (Group::Links, 2, 2, true, vec![]),
                (
                    Group::Files,
                    3,
                    2,
                    false,
                    vec![(4, Icon::FilePencil), (5, Icon::File)]
                ),
            ]
        );
    }

    #[test]
    fn an_image_row_shows_its_small_copy_in_place_of_the_icon() {
        let thumb: Arc<str> = "data:image/png;base64,AA==".into();
        let shots = [
            Shot::console(7, 64, 32, 2, Some(thumb.clone())),
            Shot::file("/w/a.heic", Touch::Read, Some(1), None),
        ];
        let none = HashSet::new();
        let drawn = link_sections(
            &with_images(Vec::new(), &shots, &none, &none, 5),
            Path::new("/w"),
            Path::new("/h"),
            0,
        );
        assert_eq!(drawn[0].group, Group::Images);
        let rows: Vec<_> = drawn[0]
            .rows
            .iter()
            .map(|r| (r.label.as_str(), r.tag.as_str(), r.file, r.thumb.clone()))
            .collect();
        assert_eq!(
            rows,
            [
                ("Image 64 × 32", "drawn", false, Some(thumb)),
                ("a.heic", "read", true, None),
            ]
        );
        assert_eq!(drawn[0].rows[1].icon, Icon::Photo);
    }

    #[test]
    fn a_subagent_row_goes_by_its_title_with_its_kind_as_a_badge() {
        use adroit_model::subagents::{Agent, Standing};
        let agent = |title: Option<&str>, kind: Option<&str>| Agent {
            call: format!("toolu_{}", kind.unwrap_or("none")),
            id: None,
            description: "Persistent hover panel".into(),
            title: title.map(str::to_string),
            kind: kind.map(str::to_string),
            prompt: String::new(),
            standing: Standing::Stopped,
            started_ms: None,
            ended_ms: None,
            steps: Vec::new(),
        };
        let agents = [
            agent(Some("Rename hover to detail"), Some("code-writer")),
            agent(None, None),
        ];
        let none = HashSet::new();
        let drawn = link_sections(
            &with_agents(Vec::new(), &agents, &none),
            Path::new("/w"),
            Path::new("/h"),
            0,
        );
        let shown: Vec<_> = drawn[0]
            .rows
            .iter()
            .map(|r| (r.label.as_str(), r.badge.as_deref(), r.tag.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Rename hover to detail", Some("code-writer"), "stopped"),
                ("Persistent hover panel", None, "stopped"),
            ]
        );
    }

    #[test]
    fn a_session_row_says_how_it_is_tied_and_whether_it_still_runs() {
        let rows = [
            related_to("tests", Some("3"), &[Tie::Opened, Tie::Received]),
            related_to("gone", None, &[Tie::Sent]),
        ];
        let drawn = link_sections(
            &overview(&rows, &[], &HashSet::new()),
            Path::new("/w"),
            Path::new("/h"),
            0,
        );
        let shown: Vec<_> = drawn[0]
            .rows
            .iter()
            .map(|r| (r.icon, r.label.as_str(), r.tag.as_str()))
            .collect();
        assert_eq!(drawn[0].group, Group::Sessions);
        assert_eq!(
            shown,
            [
                (Icon::MessagePlus, "tests", "opened · heard from"),
                (Icon::Message, "gone", "messaged · not running"),
            ]
        );
    }

    #[test]
    fn a_fact_row_is_named_by_its_label_and_shows_its_value() {
        let facts = [Fact {
            label: "Worktree",
            value: "/w/x".into(),
            tone: None,
        }];
        let drawn = link_sections(
            &about(&facts, &HashSet::new()),
            Path::new("/w"),
            Path::new("/h"),
            0,
        );
        assert_eq!(drawn[0].group, Group::About);
        let row = &drawn[0].rows[0];
        assert_eq!(
            (row.at, row.label.as_str(), row.tag.as_str(), row.file),
            (1, "Worktree", "/w/x", false)
        );
    }

    #[test]
    fn a_subagent_is_marked_in_the_colour_a_session_in_the_same_standing_is() {
        assert_eq!(
            agent_glyph(Standing::Running),
            ("state-glyph run", Icon::Loader)
        );
        assert_eq!(
            agent_glyph(Standing::Done),
            ("state-glyph done", Icon::Check)
        );
        assert_eq!(
            agent_glyph(Standing::Failed),
            ("state-glyph fail", Icon::AlertTriangle)
        );
        assert_eq!(
            agent_glyph(Standing::Stopped),
            ("state-glyph idle", Icon::PlayerStop)
        );
    }
}
