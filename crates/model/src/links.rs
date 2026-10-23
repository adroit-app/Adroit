//! What a conversation brought up: the links, files and images it touched,
//! the sessions it messaged and the subagents it ran.

use std::path::Path;

use crate::subagents::Agent;

/// The most links each group keeps: the newest. The window shows fewer, as
/// its settings say.
pub const KEPT: usize = 1000;

/// How a session came across a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Touch {
    /// Named in a reply.
    Mentioned,
    /// Read with Claude Code's `Read`.
    Read,
    /// Written or edited.
    Edited,
}

/// What a link points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A claude.ai artifact.
    Artifact,
    /// Any other web address.
    Url,
    File(Touch),
}

/// Something a session brought up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub kind: Kind,
    /// The address, or the file's absolute path.
    pub target: String,
    /// What the conversation called it.
    pub title: Option<String>,
    /// The line of a file it was named at.
    pub line: Option<u32>,
}

/// Another session a session exchanged messages with through Claude Code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// The name Claude Code addresses it by.
    pub name: String,
    /// Whether this session messaged it.
    pub sent: bool,
    /// Whether it messaged this session.
    pub received: bool,
}

/// An image file a session came across.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFile {
    /// The file's absolute path.
    pub path: String,
    pub touch: Touch,
    /// When the session last came across it, in milliseconds since the Unix
    /// epoch, as the transcript tells.
    pub at_ms: Option<u64>,
}

/// Whether `path` names an image, told by its extension.
pub fn is_image(path: &str) -> bool {
    const IMAGES: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "heic", "heif", "tif", "tiff", "ico",
        "avif",
    ];
    let p = Path::new(path);
    !path.ends_with('/')
        && p.file_stem().is_some()
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| IMAGES.iter().any(|i| e.eq_ignore_ascii_case(i)))
}

/// What a session brought up, each group newest first, and the subagents
/// it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Links {
    pub artifacts: Vec<Link>,
    pub urls: Vec<Link>,
    pub files: Vec<Link>,
    /// The image files, apart from the other files.
    pub images: Vec<ImageFile>,
    pub peers: Vec<Peer>,
    pub agents: Vec<Agent>,
}

impl Links {
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
            && self.urls.is_empty()
            && self.files.is_empty()
            && self.images.is_empty()
            && self.peers.is_empty()
            && self.agents.is_empty()
    }

    /// Every link, artifacts first, then the other addresses, then files.
    pub fn rows(&self) -> impl Iterator<Item = &Link> {
        self.artifacts.iter().chain(&self.urls).chain(&self.files)
    }
}

/// The id of a claude.ai artifact `url` names.
pub fn artifact_id(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://claude.ai/artifact/")
        .or_else(|| url.strip_prefix("https://claude.ai/code/artifact/"))?;
    let id = rest.split(['/', '?', '#']).next()?;
    (!id.is_empty()).then_some(id)
}
