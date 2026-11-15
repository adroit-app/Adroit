//! A pull request or an issue drawn as GitHub draws its conversation: its
//! title and how it stands, then a card for its body and for each thing
//! said on it, each drawn by Arto from what was written.

use adroit_model::github::{Check, Did, FileChange, Outcome, Post, Thread};
use adroit_model::time::{ago, parse_utc};

use super::markdown::{escape, render_post};

/// The thread `thread` of the repository `slug` as the page's fragment, its
/// times told as they stand at `now_ms`.
pub fn thread_fragment(slug: &str, thread: &Thread, now_ms: u64) -> String {
    let item = &thread.item;
    let when = |at: &str| match parse_utc(at) {
        Some(ms) => format!(
            "<time title=\"{}\">{}</time>",
            escape(at),
            ago(now_ms.saturating_sub(ms))
        ),
        None => String::new(),
    };
    let author = format!("<b>@{}</b>", escape(&item.author));
    let did = match (&item.pull, thread.state.as_str()) {
        (Some(pull), "merged") => format!(
            "{author} merged {} into {}",
            reference(&pull.head),
            reference(&pull.base)
        ),
        (Some(pull), _) => format!(
            "{author} wants to merge {} into {}",
            reference(&pull.head),
            reference(&pull.base)
        ),
        (None, _) => format!("{author} opened this issue"),
    };
    let labels: String = item
        .labels
        .iter()
        .map(|l| {
            format!(
                "<span class=\"gh-label\" style=\"--label:#{}\">{}</span>",
                label_colour(&l.color),
                escape(&l.name)
            )
        })
        .collect();
    let mut out = format!(
        "<header class=\"gh-head\"><h1 class=\"gh-title\">{title} <span class=\"gh-number\">#{number}</span></h1>\
         <div class=\"gh-meta\"><span class=\"gh-state {state}\">{state_word}</span>{did} · {opened}\
         <span class=\"gh-number\">{slug}</span></div>",
        title = escape(&item.title),
        number = item.number,
        state = state_class(&thread.state),
        state_word = state_word(&thread.state),
        opened = when(&item.created_at),
        slug = escape(slug),
    );
    if !labels.is_empty() {
        out.push_str(&format!("<div class=\"gh-labels\">{labels}</div>"));
    }
    out.push_str("</header>");
    let pull = item.pull.is_some();
    out.push_str(&format!(
        "<nav class=\"gh-tabs\"><button class=\"gh-tab on\" data-pane=\"conversation\">Conversation<span class=\"gh-count\">{}</span></button>",
        thread.said.len()
    ));
    if pull {
        let failed = thread
            .checks
            .iter()
            .filter(|c| c.outcome == Outcome::Failed)
            .count();
        let mark = if failed > 0 {
            "<span class=\"gh-tab-fail\">✗</span>"
        } else {
            ""
        };
        out.push_str(&format!(
            "<button class=\"gh-tab\" data-pane=\"files\">Files changed<span class=\"gh-count\">{}</span></button>\
             <button class=\"gh-tab\" data-pane=\"checks\">Checks{mark}<span class=\"gh-count\">{}</span></button>",
            thread.files.len(),
            thread.checks.len(),
        ));
    }
    out.push_str("</nav><div class=\"gh-pane\" data-pane=\"conversation\">");
    for post in std::iter::once(&thread.body).chain(&thread.said) {
        out.push_str(&card(post, thread, &when));
    }
    out.push_str("</div>");
    if pull {
        out.push_str("<div class=\"gh-pane\" data-pane=\"files\" hidden>");
        out.push_str(&changed(&thread.files));
        out.push_str("</div><div class=\"gh-pane\" data-pane=\"checks\" hidden>");
        out.push_str(&checks(&thread.checks));
        out.push_str("</div>");
    }
    out
}

/// The files a pull request changes, each as a card of its diff.
fn changed(files: &[FileChange]) -> String {
    if files.is_empty() {
        return "<p class=\"gh-empty\">No file changes to show.</p>".into();
    }
    let mut out = String::new();
    for file in files {
        let (class, letter) = match file.status.as_str() {
            "added" => ("gh-file-status gh-file-added", "A"),
            "removed" => ("gh-file-status gh-file-removed", "D"),
            "renamed" => ("gh-file-status gh-file-renamed", "R"),
            _ => ("gh-file-status gh-file-modified", "M"),
        };
        let path = match &file.previous {
            Some(previous) => format!("{} → {}", escape(previous), escape(&file.path)),
            None => escape(&file.path),
        };
        out.push_str(&format!(
            "<section class=\"gh-file\"><div class=\"gh-file-head\"><span class=\"{class}\">{letter}</span>\
             <span class=\"gh-file-path\">{path}</span><span class=\"gh-file-count\"><span class=\"gh-add\">+{}</span> \
             <span class=\"gh-del\">−{}</span></span></div>",
            file.additions, file.deletions,
        ));
        match &file.patch {
            Some(patch) => out.push_str(&diff(patch)),
            None => out.push_str("<p class=\"gh-empty gh-file-none\">No diff to show: a binary file, or one too large.</p>"),
        }
        out.push_str("</section>");
    }
    out
}

/// A unified diff's hunks as a table: each line by its number in the file
/// before and after, added and removed lines marked.
fn diff(patch: &str) -> String {
    let (mut old, mut new) = (0_u64, 0_u64);
    let mut rows = String::new();
    for line in patch.lines() {
        if let Some(head) = line.strip_prefix("@@") {
            let numbers = |sign: char| {
                head.split_whitespace()
                    .find_map(|w| w.strip_prefix(sign))
                    .and_then(|w| w.split(',').next())
                    .and_then(|n| n.parse::<u64>().ok())
                    .unwrap_or(1)
            };
            (old, new) = (numbers('-'), numbers('+'));
            rows.push_str(&format!(
                "<tr class=\"gh-hunk\"><td colspan=\"3\">{}</td></tr>",
                escape(line)
            ));
            continue;
        }
        let (class, before, after, text) = match line.chars().next() {
            Some('+') => {
                new += 1;
                (
                    "gh-line-add",
                    String::new(),
                    (new - 1).to_string(),
                    &line[1..],
                )
            }
            Some('-') => {
                old += 1;
                (
                    "gh-line-del",
                    (old - 1).to_string(),
                    String::new(),
                    &line[1..],
                )
            }
            Some('\\') => ("gh-line-note", String::new(), String::new(), line),
            _ => {
                old += 1;
                new += 1;
                (
                    "gh-line-ctx",
                    (old - 1).to_string(),
                    (new - 1).to_string(),
                    line.get(1..).unwrap_or_default(),
                )
            }
        };
        rows.push_str(&format!(
            "<tr class=\"{class}\"><td class=\"gh-num\">{before}</td><td class=\"gh-num\">{after}</td><td class=\"gh-code\">{}</td></tr>",
            escape(text)
        ));
    }
    format!("<table class=\"gh-diff\"><tbody>{rows}</tbody></table>")
}

/// The checks on a pull request's head: how many ended which way, then
/// each by how it ended, its name, what ran it and what it said.
fn checks(checks: &[Check]) -> String {
    if checks.is_empty() {
        return "<p class=\"gh-empty\">No checks have run.</p>".into();
    }
    let count = |outcome: Outcome| checks.iter().filter(|c| c.outcome == outcome).count();
    let summary: Vec<String> = [
        (Outcome::Failed, "failing"),
        (Outcome::Pending, "in progress"),
        (Outcome::Passed, "successful"),
        (Outcome::Skipped, "skipped"),
    ]
    .into_iter()
    .filter_map(|(outcome, word)| {
        let n = count(outcome);
        (n > 0).then(|| format!("{n} {word}"))
    })
    .collect();
    let mut out = format!(
        "<p class=\"gh-checks-summary\">{}</p><ul class=\"gh-checks\">",
        summary.join(", ")
    );
    for check in checks {
        let (class, mark) = match check.outcome {
            Outcome::Failed => ("gh-check gh-check-failed", "✗"),
            Outcome::Pending => ("gh-check gh-check-pending", "●"),
            Outcome::Passed => ("gh-check gh-check-passed", "✓"),
            Outcome::Skipped => ("gh-check gh-check-skipped", "⊘"),
        };
        let name = if check.link.starts_with("https://") || check.link.starts_with("http://") {
            format!(
                "<a href=\"{}\">{}</a>",
                escape(&check.link),
                escape(&check.name)
            )
        } else {
            escape(&check.name)
        };
        out.push_str(&format!(
            "<li class=\"{class}\"><span class=\"gh-check-mark\">{mark}</span><span class=\"gh-check-name\">{name}</span>\
             <span class=\"gh-check-by\">{}</span><span class=\"gh-check-said\">{}</span></li>",
            escape(&check.by),
            escape(&check.said),
        ));
    }
    out.push_str("</ul>");
    out
}

/// One post as a card: who wrote it, what it did and when, over what it
/// wrote. A review that wrote nothing is its head alone.
fn card(post: &Post, thread: &Thread, when: &dyn Fn(&str) -> String) -> String {
    let (class, mark) = match post.did {
        Did::Approved => (
            " gh-post-approved",
            "<span class=\"gh-post-mark\">Approved</span>",
        ),
        Did::ChangesRequested => (
            " gh-post-changes",
            "<span class=\"gh-post-mark\">Changes requested</span>",
        ),
        _ => ("", ""),
    };
    let body = post.body.trim();
    let shown = if body.is_empty() && post.did == Did::Opened {
        "<p class=\"gh-empty\">No description provided.</p>".to_string()
    } else if body.is_empty() {
        String::new()
    } else {
        render_post(&with_images(body, thread))
    };
    let plain = if shown.is_empty() {
        " gh-post-plain"
    } else {
        ""
    };
    let mut out = format!(
        "<section class=\"gh-post{class}{plain}\"><div class=\"gh-post-head\"><b>@{author}</b> {did} {mark} · {at}</div>",
        author = escape(&post.author),
        // A verdict is told by its mark.
        did = match post.did {
            Did::Approved | Did::ChangesRequested => "reviewed",
            did => did.words(),
        },
        at = when(&post.at),
    );
    if !shown.is_empty() {
        out.push_str(&format!(
            "<div class=\"gh-post-body markdown-body\">{shown}</div>"
        ));
    }
    out.push_str("</section>");
    out
}

/// `body` with each image `thread` carries drawn from its bytes rather than
/// fetched from where it was written: an image of a private repository is
/// served only to its login.
fn with_images(body: &str, thread: &Thread) -> String {
    thread.images.iter().fold(body.to_string(), |text, image| {
        text.replace(
            &image.address,
            &format!("data:{};base64,{}", image.mime, image.data),
        )
    })
}

fn reference(name: &str) -> String {
    format!("<span class=\"gh-ref\">{}</span>", escape(name))
}

fn state_class(state: &str) -> &'static str {
    match state {
        "draft" => "gh-state-draft",
        "closed" => "gh-state-closed",
        "merged" => "gh-state-merged",
        _ => "gh-state-open",
    }
}

fn state_word(state: &str) -> &'static str {
    match state {
        "draft" => "Draft",
        "closed" => "Closed",
        "merged" => "Merged",
        _ => "Open",
    }
}

/// A label's colour as GitHub gives it, kept to six hex digits so it can
/// stand in a style; grey for anything else.
pub fn label_colour(colour: &str) -> &str {
    if colour.len() == 6 && colour.chars().all(|c| c.is_ascii_hexdigit()) {
        colour
    } else {
        "8b949e"
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::github::{Image, Item, Label, Pull};

    use super::{Check, FileChange, Outcome};

    use super::*;

    const NOW: u64 = 1_794_585_621_000 + 2 * 86_400_000;

    fn thread() -> Thread {
        Thread {
            item: Item {
                number: 7,
                title: "Fix <the> parser".into(),
                author: "alice".into(),
                created_at: "2026-11-13T16:00:21Z".into(),
                labels: vec![Label {
                    name: "bug".into(),
                    color: "d73a4a\"><script>".into(),
                }],
                pull: Some(Pull {
                    head: "fix".into(),
                    base: "main".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            state: "open".into(),
            body: Post {
                author: "alice".into(),
                did: Did::Opened,
                at: "2026-11-13T16:00:21Z".into(),
                body: "It broke.\n\n![shot](https://github.com/a.png)".into(),
            },
            said: vec![
                Post {
                    author: "bob".into(),
                    did: Did::Approved,
                    at: "2026-11-14T16:00:21Z".into(),
                    body: String::new(),
                },
                Post {
                    author: "carol".into(),
                    did: Did::Commented,
                    at: "2026-11-15T16:00:21Z".into(),
                    body: "Thanks".into(),
                },
            ],
            images: vec![Image {
                address: "https://github.com/a.png".into(),
                mime: "image/png".into(),
                data: "iVBO".into(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_pull_request_heads_with_its_title_state_and_branches_then_a_card_for_each_post() {
        let page = thread_fragment("o/r", &thread(), NOW);
        assert!(page.contains("Fix &lt;the&gt; parser <span class=\"gh-number\">#7</span>"));
        assert!(page.contains("gh-state-open\">Open"));
        assert!(page.contains("<b>@alice</b> wants to merge <span class=\"gh-ref\">fix</span> into <span class=\"gh-ref\">main</span>"));
        assert!(page.contains("2 days ago"));
        assert!(
            page.contains(
                "data-pane=\"conversation\">Conversation<span class=\"gh-count\">2</span>"
            )
        );
        let cards: Vec<_> = page.match_indices("<section class=\"gh-post").collect();
        assert_eq!(cards.len(), 3);
        assert!(page.contains("gh-post gh-post-approved gh-post-plain"));
        assert!(page.contains("<b>@bob</b> reviewed <span class=\"gh-post-mark\">Approved</span>"));
        assert!(page.contains("<b>@carol</b> commented"));
    }

    #[test]
    fn what_a_post_writes_is_drawn_and_its_images_from_the_bytes_carried() {
        let page = thread_fragment("o/r", &thread(), NOW);
        assert!(page.contains("It broke.</p>"), "{page}");
        assert!(page.contains("data:image/png;base64,iVBO"), "{page}");
        assert!(!page.contains("https://github.com/a.png"), "{page}");
    }

    #[test]
    fn nothing_github_tells_reaches_the_page_as_markup() {
        let page = thread_fragment("o/r", &thread(), NOW);
        assert!(!page.contains("<script>"));
        assert!(page.contains("--label:#8b949e"));
    }

    #[test]
    fn a_pull_request_has_panes_for_its_changed_files_and_its_checks() {
        let mut pull = thread();
        pull.files = vec![FileChange {
            path: "src/a.rs".into(),
            status: "modified".into(),
            additions: 2,
            deletions: 1,
            patch: Some("@@ -10,2 +10,3 @@ fn a()\n ctx\n-old <b>\n+new\n+more\n\\ No newline at end of file".into()),
            ..Default::default()
        }];
        pull.checks = vec![
            Check {
                name: "lint".into(),
                outcome: Outcome::Failed,
                link: "https://x/1".into(),
                said: "2 errors".into(),
                ..Default::default()
            },
            Check {
                name: "build".into(),
                outcome: Outcome::Passed,
                link: "javascript:alert(1)".into(),
                ..Default::default()
            },
        ];
        let page = thread_fragment("o/r", &pull, NOW);
        assert!(
            page.contains("data-pane=\"files\">Files changed<span class=\"gh-count\">1</span>")
        );
        assert!(page.contains(
            "Checks<span class=\"gh-tab-fail\">✗</span><span class=\"gh-count\">2</span>"
        ));
        assert!(page.contains("<div class=\"gh-pane\" data-pane=\"files\" hidden>"));
        assert!(page.contains("<tr class=\"gh-line-ctx\"><td class=\"gh-num\">10</td><td class=\"gh-num\">10</td><td class=\"gh-code\">ctx</td></tr>"));
        assert!(page.contains("<tr class=\"gh-line-del\"><td class=\"gh-num\">11</td><td class=\"gh-num\"></td><td class=\"gh-code\">old &lt;b&gt;</td></tr>"));
        assert!(page.contains("<tr class=\"gh-line-add\"><td class=\"gh-num\"></td><td class=\"gh-num\">12</td><td class=\"gh-code\">more</td></tr>"));
        assert!(page.contains("1 failing, 1 successful"));
        assert!(page.contains("<a href=\"https://x/1\">lint</a>"));
        assert!(!page.contains("javascript:"));
    }

    #[test]
    fn an_issue_says_who_opened_it_and_an_empty_body_says_so() {
        let mut issue = thread();
        issue.item.pull = None;
        issue.body.body = String::new();
        let page = thread_fragment("o/r", &issue, NOW);
        assert!(page.contains("<b>@alice</b> opened this issue"));
        assert!(!page.contains("data-pane=\"files\""));
        assert!(page.contains("No description provided."));
    }
}
