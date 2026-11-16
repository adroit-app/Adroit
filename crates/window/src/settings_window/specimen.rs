//! A picture of each component of the details panel, drawn with the markup
//! and classes the real component draws but with made-up content: the
//! Settings window has none of the main window's state to draw it from.

use adroit_core::details::{Component, Gauge, GaugeKind, Placed};
use adroit_core::graph;
use adroit_model::usage::Severity;
use dioxus::prelude::*;

use crate::components::{Fact, Facts, ListRow, PlacedButton, Section, Spacer};
use crate::details::Gauges;
use crate::icons::{Icon, IconView};

/// What `placed` looks like in the details panel, with sample content,
/// folded as it is placed when it folds.
#[component]
pub(super) fn Specimen(placed: Placed) -> Element {
    let component = placed.component;
    let folded = placed.folded == Some(true);
    if component.is_button() {
        return rsx! { PlacedButton { placed: placed.clone(), onclick: |_| {} } };
    }
    if component == Component::Section {
        let title = placed.title.clone().unwrap_or_default();
        return rsx! {
            div { class: "details-section", span { class: "details-label", "{title}" } }
        };
    }
    if folded && component.folds() {
        return match component {
            Component::Running => rsx! {
                div { class: "details-block details-links",
                    {folded_heading("1 subagent · 1 process")}
                }
            },
            c => rsx! {
                div { class: "details-block details-links", {folded_heading(&folded_said(c))} }
            },
        };
    }
    match component {
        Component::Session => session(),
        Component::Section => rsx! {},
        Component::Change => change(),
        Component::Timeline => timeline(),
        Component::Running => running(),
        Component::Processes => processes(),
        Component::About => about(),
        Component::Sessions => links(
            "2 sessions · review-login-retry, docs-timeout",
            &[
                Link::new(
                    Icon::MessagePlus,
                    "review-login-retry",
                    "opened · heard from",
                ),
                Link::new(Icon::Message, "docs-timeout", "messaged · not running"),
            ],
        ),
        Component::Messages => links(
            "2 messages · reviewer",
            &[
                Link::new(
                    Icon::Message,
                    "Please review the retry on timeout",
                    "sent · 4 min ago",
                ),
                Link::new(
                    Icon::Message,
                    "Looks good; one nit in the test",
                    "received · 1 min ago",
                ),
            ],
        ),
        Component::Subagents => links(
            "1 running · 1 done",
            &[
                Link {
                    tone: "state-glyph run",
                    badge: Some("Explore"),
                    ..Link::new(
                        Icon::Loader,
                        "Find where the login times out",
                        "running 1m 12s",
                    )
                },
                Link {
                    tone: "state-glyph done",
                    badge: Some("code-writer"),
                    ..Link::new(Icon::Check, "Write the retry test", "done in 3m 40s")
                },
            ],
        ),
        Component::Artifacts => links(
            "1 artifact · Login retry plan",
            &[Link::new(Icon::AppWindow, "Login retry plan", "3f9c2a1e")],
        ),
        Component::Images => images(),
        Component::Links => links(
            "2 links · docs.rs, www.rfc-editor.org",
            &[
                Link::new(Icon::Link, "docs.rs/reqwest/latest/reqwest", "docs.rs"),
                Link::new(Icon::Link, "RFC 9110: HTTP Semantics", "www.rfc-editor.org"),
            ],
        ),
        Component::Files => links(
            "3 files · login.rs, retry.rs, README.md",
            &[
                Link::new(Icon::FilePencil, "src/auth/login.rs:88", "edited"),
                Link::new(Icon::File, "src/auth/retry.rs", "read"),
                Link::new(Icon::File, "README.md", "mentioned"),
            ],
        ),
        Component::Changes => changes(),
        Component::Log => log(),
        Component::Prompts => links(
            "2 prompts · latest: Add a test for the second timeout",
            &[
                Link::new(
                    Icon::Message,
                    "Retry the login once when it times out",
                    "12 min ago",
                ),
                Link::new(
                    Icon::Message,
                    "Add a test for the second timeout",
                    "5 min ago",
                ),
            ],
        ),
        Component::Context => usage(Gauge {
            label: "Context".into(),
            used: 42,
            left: "58% left".into(),
            note: "42% used · 84k / 200k tokens".into(),
            kind: GaugeKind::Context,
            severity: Severity::Normal,
        }),
        Component::SessionLimit => usage(Gauge {
            label: "Session".into(),
            used: 31,
            left: "69% left".into(),
            note: "31% used · resets in 2 h 40 min".into(),
            kind: GaugeKind::Session,
            severity: Severity::Normal,
        }),
        Component::WeekLimit => usage(Gauge {
            label: "Week".into(),
            used: 76,
            left: "24% left".into(),
            note: "76% used · resets in 3 days".into(),
            kind: GaugeKind::Week,
            severity: Severity::Warning,
        }),
        Component::ModelLimits => usage(Gauge {
            label: "Week · Opus".into(),
            used: 18,
            left: "82% left".into(),
            note: "18% used · resets in 3 days".into(),
            kind: GaugeKind::Other,
            severity: Severity::Normal,
        }),
        Component::Cpu => meter(Meter {
            label: "CPU",
            kind: "cpu",
            icon: Icon::Cpu,
            used: 23.0,
            note: "10 cores · load 2.31",
        }),
        Component::Gpu => meter(Meter {
            label: "GPU",
            kind: "gpu",
            icon: Icon::Gpu,
            used: 8.0,
            note: "Apple M2 Pro",
        }),
        Component::Memory => meter(Meter {
            label: "Memory",
            kind: "memory",
            icon: Icon::Memory,
            used: 61.0,
            note: "19.5 / 32.0 GiB",
        }),
        Component::Storage => meter(Meter {
            label: "Storage",
            kind: "storage",
            icon: Icon::Database,
            used: 47.0,
            note: "468 / 994 GB",
        }),
        Component::Environment => environment(),
        Component::EnvironmentRetry => button(Icon::Refresh, "Retry now", true),
        Component::EnvironmentPair => button(Icon::ArrowsExchange, "Pair again", false),
        Component::EnvironmentSettings => button(Icon::Settings, "Environment settings", false),
        Component::EnvironmentWaiting => links(
            "",
            &[Link::new(
                Icon::FilePencil,
                "Rename the session fix-login-timeout",
                "waiting",
            )],
        ),
        Component::Profiles => profiles(),
        Component::Github => github(),
        Component::GithubShow => button(Icon::ArrowRight, "Show", true),
        Component::GithubOpen => button(Icon::BrandGithub, "Open on GitHub", false),
        Component::GithubStart => button(Icon::MessagePlus, "Start a session", false),
        Component::GithubConversation => github_posts(),
        Component::GithubFiles => github_files(),
        Component::GithubChecks => github_checks(),
    }
}

/// A row of a component that lists rows under a heading, as the panel
/// draws it.
#[derive(Clone, Copy)]
struct Link {
    icon: Icon,
    /// The class that colours its icon; empty for the panel's own colour.
    tone: &'static str,
    label: &'static str,
    badge: Option<&'static str>,
    tag: &'static str,
}

impl Link {
    const fn new(icon: Icon, label: &'static str, tag: &'static str) -> Self {
        Self {
            icon,
            tone: "",
            label,
            badge: None,
            tag,
        }
    }
}

/// The line over a group's rows, saying what they come to, open.
fn heading(said: &str) -> Element {
    rsx! {
        ListRow { kind: "details-link-heading",
            IconView { icon: Icon::ChevronDown, size: 12 }
            span { class: "list-row-name details-summary", "{said}" }
        }
    }
}

/// The line of a folded group, saying what its rows come to.
fn folded_heading(said: &str) -> Element {
    rsx! {
        ListRow { kind: "details-link-heading",
            IconView { icon: Icon::ChevronRight, size: 12 }
            span { class: "list-row-name details-summary", "{said}" }
        }
    }
}

/// What a folded component's line says in the picture.
fn folded_said(c: Component) -> String {
    match c {
        Component::About => "work · Claude Code".into(),
        Component::Sessions => "2 sessions · review-login-retry, docs-timeout".into(),
        Component::Messages => "2 messages · reviewer".into(),
        Component::Subagents => "1 running · 1 done".into(),
        Component::Artifacts => "1 artifact · Login retry plan".into(),
        Component::Images => "2 images".into(),
        Component::Links => "2 links · docs.rs, www.rfc-editor.org".into(),
        Component::Files => "3 files · login.rs, retry.rs, README.md".into(),
        Component::Prompts => "2 prompts · latest: Add a test for the second timeout".into(),
        Component::Processes => "2 processes · cargo, node".into(),
        Component::Changes => "3 files changed".into(),
        Component::Log => "24 changes · latest: fix: retry the login on timeout".into(),
        c => c.title().into(),
    }
}

fn links(title: &str, rows: &[Link]) -> Element {
    rsx! {
        div { class: "details-block details-links",
            if !title.is_empty() {
                {heading(title)}
            }
            for (i , r) in rows.iter().copied().enumerate() {
                ListRow { key: "{i}", kind: "details-link", title: "{r.label}",
                    if r.tone.is_empty() {
                        IconView { icon: r.icon, size: 13 }
                    } else {
                        span { class: "{r.tone}", IconView { icon: r.icon, size: 13 } }
                    }
                    span { class: "list-row-name", "{r.label}" }
                    if let Some(b) = r.badge {
                        span { class: "commit-mark details-link-badge", "{b}" }
                    }
                    span { class: "details-link-tag", "{r.tag}" }
                }
            }
        }
    }
}

fn session() -> Element {
    rsx! {
        Section { label: "fix-login-timeout",
            Facts {
                Fact { label: "Model", span { class: "details-facts-value", "opus" } }
                Fact { label: "State",
                    span { class: "details-facts-value",
                        span { class: "state-badge run", "working" }
                    }
                }
                Fact { label: "Started", span { class: "details-facts-value", "12 min ago" } }
                Fact { label: "Last input", span { class: "details-facts-value", "3m 05s ago" } }
            }
        }
    }
}

fn change() -> Element {
    rsx! {
        div { class: "details-block details-links",
            ListRow { kind: "details-link details-change", title: "fix: retry the login on timeout",
                IconView { icon: Icon::GitCommit, size: 13 }
                span { class: "commit-mark", "kxqz" }
                span { class: "list-row-name", "fix: retry the login on timeout" }
                span { class: "details-link-tag", "3" }
            }
        }
    }
}

/// A bar split by the status of the work, as `(tone, width)` parts.
fn stack_bar(kind: &str, parts: &[(&str, f32)]) -> Element {
    rsx! {
        div { class: "timeline-bar {kind}",
            for (tone , width) in parts.iter().copied() {
                div { key: "{tone}", class: "timeline-bar-part {tone}", style: "width:{width}%" }
            }
        }
    }
}

fn timeline() -> Element {
    let tasks = [
        (
            "done",
            "●",
            "Completed",
            "Find where the login times out",
            0,
            "100.00%",
            100.0,
        ),
        (
            "run",
            "◐",
            "In progress",
            "Write the retry test",
            0,
            "50.00%",
            50.0,
        ),
        (
            "idle",
            "○",
            "Not started",
            "Document the retry",
            1,
            "0.00%",
            0.0,
        ),
    ];
    rsx! {
        div { class: "details-block details-links timeline",
            ListRow { kind: "timeline-tree", title: "Hide its tasks",
                div { class: "timeline-tree-head",
                    IconView { icon: Icon::ChevronDown, size: 12 }
                    span { class: "list-row-name timeline-tree-title", "Retry the login on timeout" }
                    span { class: "state-badge run", "In progress" }
                }
                div { class: "timeline-tree-bar",
                    {stack_bar("", &[("done", 62.5), ("run", 25.0), ("idle", 12.5)])}
                    span { class: "timeline-share", "62.50%" }
                }
                span { class: "details-link-tag timeline-tree-tag", "5/8 leaf tasks · 12m" }
            }
            for (tone , mark , status , title , depth , share , done) in tasks {
                ListRow { key: "{title}",
                    kind: "details-link timeline-task",
                    title: "{title}",
                    style: "padding-left:{18 + 14 * depth}px",
                    span { class: "timeline-mark {tone}", title: "{status}", "{mark}" }
                    span { class: "list-row-name", "{title}" }
                    {stack_bar("small", &[("done", done), ("idle", 100.0 - done)])}
                    span { class: "timeline-share", "{share}" }
                }
            }
        }
    }
}

fn running() -> Element {
    rsx! {
        div { class: "details-block details-links",
            {heading("1 subagent · 1 process")}
            ListRow { kind: "details-link", title: "Find where the login times out",
                span { class: "state-glyph run", IconView { icon: Icon::Loader, size: 13 } }
                span { class: "list-row-name", "Find where the login times out" }
                span { class: "commit-mark details-link-badge", "Explore" }
                span { class: "details-link-tag", "running 1m 12s" }
            }
            ListRow { kind: "details-link", title: "PID 48213 · cargo test -p auth",
                IconView { icon: Icon::Terminal2, size: 13 }
                span { class: "list-row-name", "cargo" }
                span { class: "details-link-tag", "48213 · 212 MiB" }
                button {
                    class: "process-kill",
                    r#type: "button",
                    title: "Kill process…",
                    aria_label: "Kill cargo",
                    tabindex: "-1",
                    IconView { icon: Icon::X, size: 12 }
                }
            }
        }
    }
}

fn processes() -> Element {
    rsx! {
        div { class: "details-block details-links",
            {heading("2 processes · cargo, node")}
            for (name , command , tag) in [("cargo", "cargo test -p auth", "48213 · 212 MiB"), ("node", "node scripts/dev.js", "48190 · 96 MiB")] {
                ListRow { key: "{name}", kind: "details-link", title: "{command}",
                    IconView { icon: Icon::Terminal2, size: 13 }
                    span { class: "list-row-name", "{name}" }
                    span { class: "details-link-tag", "{tag}" }
                    button {
                        class: "process-kill",
                        r#type: "button",
                        title: "Kill process…",
                        aria_label: "Kill {name}",
                        tabindex: "-1",
                        IconView { icon: Icon::X, size: 12 }
                    }
                }
            }
        }
    }
}

fn about() -> Element {
    let facts = [
        ("Agent setup", "work · Claude Code"),
        (
            "Worktree",
            "~/src/webapp/.claude/worktrees/fix-login-timeout",
        ),
        ("Turns", "14"),
    ];
    rsx! {
        div { class: "details-block details-links",
            {heading("work · Claude Code")}
            for (label , value) in facts {
                ListRow { key: "{label}", kind: "details-link details-fact", title: "{value}",
                    span { class: "details-facts-key", "{label}" }
                    span { class: "list-row-name details-facts-value", "{value}" }
                }
            }
        }
    }
}

/// A small made-up picture: a dark frame with a few coloured lines.
const THUMB: &str = "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 56 40'><rect width='56' height='40' rx='3' fill='%2324283b'/><rect x='6' y='8' width='30' height='3' fill='%237aa2f7'/><rect x='6' y='16' width='40' height='3' fill='%239ece6a'/><rect x='6' y='24' width='22' height='3' fill='%23e0af68'/></svg>";

fn images() -> Element {
    rsx! {
        div { class: "details-block details-links",
            {heading("2 images")}
            ListRow { kind: "details-link details-image", title: "Image 1280 × 720",
                img { class: "details-thumb", src: THUMB, alt: "" }
                span { class: "list-row-name", "Image 1280 × 720" }
                span { class: "details-link-tag", "drawn" }
            }
            ListRow { kind: "details-link", title: "docs/login-flow.png",
                IconView { icon: Icon::Photo, size: 13 }
                span { class: "list-row-name", "docs/login-flow.png" }
                span { class: "details-link-tag", "read" }
            }
        }
    }
}

fn changes() -> Element {
    let files = [
        ("M", "src/auth/login.rs"),
        ("A", "src/auth/retry.rs"),
        ("M", "tests/login.rs"),
    ];
    rsx! {
        div { class: "list-rows details-git",
            div { class: "revision-description",
                div { class: "revision-heading",
                    span { class: "details-label", "Description" }
                    span { class: "commit-hash", "kxqz3f2a" }
                }
                pre { "fix: retry the login on timeout" }
            }
            for (status , path) in files {
                ListRow { key: "{path}", kind: "mono",
                    span { class: "details-file-status", "{status}" }
                    span { class: "list-row-name", "{path}" }
                }
            }
        }
    }
}

/// A commit of the log, as the panel lists it.
struct Commit {
    node: &'static str,
    parents: Vec<String>,
    short: &'static str,
    subject: &'static str,
    when: &'static str,
    current: bool,
    bookmark: Option<&'static str>,
}

fn log() -> Element {
    let commit = |node, parent: &str, short, subject, when| Commit {
        node,
        parents: vec![parent.to_string()],
        short,
        subject,
        when,
        current: false,
        bookmark: None,
    };
    let commits = [
        Commit {
            current: true,
            ..commit(
                "kxqz",
                "wnto",
                "kxqz",
                "fix: retry the login on timeout",
                "3 min ago",
            )
        },
        commit(
            "wnto",
            "ysml",
            "wnto",
            "test: cover the login timeout",
            "1 h ago",
        ),
        Commit {
            bookmark: Some("main"),
            ..commit(
                "ysml",
                "root",
                "ysml",
                "feat: sign in with a passkey",
                "2 days ago",
            )
        },
    ];
    let rows = graph::layout(commits.iter().map(|c| (c.node, c.parents.as_slice())));
    let lanes = rows.iter().map(graph::Row::width).max().unwrap_or(1);
    rsx! {
        div { class: "list-rows details-git",
            for (c , row) in commits.iter().zip(&rows) {
                div { key: "{c.node}", class: "commit",
                    div {
                        class: "commit-graph",
                        dangerous_inner_html: graph::svg(row, lanes, graph::Marks { current: c.current, empty: false, conflict: false }),
                    }
                    div { class: "commit-text",
                        span { class: "commit-hash", "{c.short}" }
                        span { class: "commit-subject",
                            if c.current {
                                span { class: "commit-mark current", "@" }
                            }
                            if let Some(b) = c.bookmark {
                                span { class: "commit-mark bookmark", "{b}" }
                            }
                            "{c.subject}"
                        }
                        span { class: "commit-when", "{c.when}" }
                    }
                }
            }
        }
    }
}

/// One of the agent's gauges, as the panel draws it in the block of the
/// gauges next to it, which the picture draws around it.
fn usage(gauge: Gauge) -> Element {
    rsx! { Gauges { gauges: vec![gauge] } }
}

/// One of the machine's meters.
struct Meter {
    label: &'static str,
    kind: &'static str,
    icon: Icon,
    used: f32,
    note: &'static str,
}

/// One of the machine's meters, as the panel draws it in the grid of the
/// meters next to it, which the picture draws around it.
fn meter(m: Meter) -> Element {
    rsx! {
                div { class: "usage-resource {m.kind}",
                    div { class: "usage-resource-head",
                        span { class: "usage-resource-label",
                            IconView { icon: m.icon, size: 14 }
                            "{m.label}"
                        }
                        span { class: "usage-resource-value", "{m.used:.0}%" }
                    }
                    div { class: "usage-resource-track",
                        div { class: "usage-resource-fill", style: "width:{m.used}%" }
                    }
                    span { class: "usage-resource-note", "{m.note}" }
                }
    }
}

fn environment() -> Element {
    rsx! {
        div { class: "environment-card",
            div { class: "environment-card-head",
                div { class: "environment-card-title",
                    span { class: "standing-dot ok", "aria-hidden": "true" }
                    IconView { icon: Icon::Server, size: 14 }
                    span { class: "environment-card-name", "build-box" }
                }
                div { class: "environment-card-address", "build-box.local" }
            }
            div { class: "details-block",
                div { class: "details-facts",
                    span { class: "details-facts-key", "Status" }
                    span { class: "details-facts-value", "Connected" }
                    span { class: "details-facts-key", "Address" }
                    span { class: "details-facts-value environment-card-mono", "192.168.1.20:7140" }
                }
            }
        }
    }
}

fn profiles() -> Element {
    let gauge = Gauge {
        label: "Session".into(),
        used: 31,
        left: "69% left".into(),
        note: "31% used · resets in 2 h 40 min".into(),
        kind: GaugeKind::Session,
        severity: Severity::Normal,
    };
    rsx! {
        div { class: "details-block",
            div { class: "environment-agent",
                div { class: "environment-agent-head",
                    span { class: "environment-agent-name", "work" }
                    span { class: "agent-badge", "claude" }
                    span { class: "environment-agent-default", "default" }
                    Spacer {}
                    span { class: "environment-login in", "Logged in" }
                    button { class: "usage-web", tabindex: "-1",
                        IconView { icon: Icon::Link, size: 12 }
                        "Web"
                    }
                }
                Gauges { gauges: vec![gauge] }
            }
            div { class: "environment-agent",
                div { class: "environment-agent-head",
                    span { class: "environment-agent-name", "review" }
                    span { class: "agent-badge", "codex" }
                    Spacer {}
                    span { class: "environment-login out", "Logged out" }
                }
            }
        }
    }
}

/// A button of a pull request or issue, as the panel draws one.
fn button(icon: Icon, label: &str, primary: bool) -> Element {
    rsx! {
        div { class: "details-block details-button",
            button { class: if primary { "button primary" } else { "button" }, r#type: "button", tabindex: "-1",
                IconView { icon, size: 13 }
                "{label}"
            }
        }
    }
}

fn github_posts() -> Element {
    let posts = [
        (
            "octocat",
            "opened",
            "2 h ago",
            "Retry the login once when it times out, so a slow network does not sign people out.",
        ),
        (
            "hubot",
            "approved",
            "20 min ago",
            "Looks good; one nit in the test.",
        ),
    ];
    rsx! {
        div { class: "details-block github-posts",
            for (author , did , when , body) in posts {
                div { key: "{author}", class: "github-post",
                    div { class: "github-post-head",
                        span { class: "github-post-author", "" }
                        span { class: "github-post-did", "{did}" }
                        span { class: "details-link-tag", "{when}" }
                    }
                    div { class: "github-post-body", "{body}" }
                }
            }
        }
    }
}

fn github_files() -> Element {
    let files = [
        ("M", "src/auth/login.rs", 30, 5),
        ("A", "src/auth/retry.rs", 12, 0),
        ("M", "README.md", 0, 2),
    ];
    rsx! {
        div { class: "list-rows details-git",
            for (status , path , added , removed) in files {
                ListRow { key: "{path}", kind: "mono",
                    span { class: "details-file-status", "{status}" }
                    span { class: "list-row-name", "{path}" }
                    span { class: "details-link-tag",
                        span { class: "github-card-added", "+{added}" }
                        " "
                        span { class: "github-card-removed", "−{removed}" }
                    }
                }
            }
        }
    }
}

fn github_checks() -> Element {
    let checks = [
        ("fail", Icon::X, "test (windows)", true),
        ("run", Icon::Loader, "test (linux)", false),
        ("done", Icon::Check, "lint", false),
    ];
    rsx! {
        div { class: "details-block details-links",
            for (tone , icon , name , failed) in checks {
                ListRow { key: "{name}", kind: "details-link",
                    span { class: "state-glyph {tone}", IconView { icon, size: 13 } }
                    span { class: if failed { "list-row-name github-check-failed" } else { "list-row-name" }, "{name}" }
                    span { class: "details-link-tag", "GitHub Actions" }
                }
            }
        }
    }
}

fn github() -> Element {
    rsx! {
        div { class: "environment-card github-card",
            div { class: "environment-card-head",
                div { class: "environment-card-title",
                    span { class: "environment-card-name", "Retry the login on timeout" }
                }
                div { class: "environment-card-address", "acme/webapp#482" }
            }
            div { class: "details-block",
                div { class: "details-facts",
                    span { class: "details-facts-key", "State" }
                    span { class: "details-facts-value", span { class: "state-badge run", "Open" } }
                    span { class: "details-facts-key", "Branch" }
                    span { class: "details-facts-value github-card-mono", "fix-login-timeout → main" }
                    span { class: "details-facts-key", "Changes" }
                    span { class: "details-facts-value",
                        span { class: "github-card-added", "+42" }
                        " "
                        span { class: "github-card-removed", "−7" }
                        " · 3 files"
                    }
                    span { class: "details-facts-key", "Checks" }
                    span { class: "details-facts-value",
                        span { class: "github-card-check state-glyph done",
                            IconView { icon: Icon::Check, size: 12 }
                            "5"
                        }
                        span { class: "github-card-check state-glyph run",
                            IconView { icon: Icon::Loader, size: 12 }
                            "1"
                        }
                    }
                }
            }
        }
    }
}
