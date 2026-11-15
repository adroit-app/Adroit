//! A pull request or an issue read whole, with the images it shows.

use std::path::Path;

use adroit_model::github::{Did, Image, Item, Label, Listing, Post, Pull, Thread};
use base64::Engine;
use serde::Deserialize;

use super::{reason, remote};
use crate::{Error, run};

/// The most bytes of images a thread carries, short enough of the largest
/// frame the work host sends once in base64 to leave room for the text.
const CARRIED_BYTES: usize = 8 * 1024 * 1024;

/// The pull request or the issue `number` of the repository at `root`, with
/// its comments and reviews, and the images they show as far as
/// [`CARRIED_BYTES`] holds them. An image of a private repository is served
/// only to its login, at an address GitHub signs for a few minutes, so the
/// host fetches it here rather than leave the window an address it cannot
/// read.
pub async fn thread(root: &Path, listing: Listing, number: u64) -> Result<Thread, String> {
    let remote = remote(root)
        .await
        .ok_or("the repository has no remote on GitHub")?;
    let slug = &remote.slug;
    let issue = api(root, &format!("repos/{slug}/issues/{number}"), false).await?;
    let comments = api(
        root,
        &format!("repos/{slug}/issues/{number}/comments?per_page=100"),
        true,
    )
    .await?;
    let (pull, reviews) = match listing {
        Listing::Pulls => (
            Some(api(root, &format!("repos/{slug}/pulls/{number}"), false).await?),
            api(
                root,
                &format!("repos/{slug}/pulls/{number}/reviews?per_page=100"),
                true,
            )
            .await?,
        ),
        Listing::Issues => (None, String::new()),
    };
    let (mut thread, texts) = read(&issue, pull.as_deref(), &comments, &reviews)?;
    let mut wanted: Vec<(String, String)> = Vec::new();
    for (written, fetched) in texts.iter().flat_map(images) {
        if !wanted.iter().any(|(w, _)| *w == written) {
            wanted.push((written, fetched));
        }
    }
    // Each read fetches into a folder of its own, gone once it is read.
    static READS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let read_no = READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("adroit-github-{}-{read_no}", std::process::id()));
    let fetched = fetch(&dir, wanted).await;
    thread.images = carried(fetched, CARRIED_BYTES);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(thread)
}

/// What GitHub's API answers at `path`, the body of each text rendered as
/// HTML beside its Markdown; with `many`, each item of every page, one JSON
/// value after another.
async fn api(root: &Path, path: &str, many: bool) -> Result<String, String> {
    let mut args = vec![
        "api",
        "-H",
        "Accept: application/vnd.github.full+json",
        path,
    ];
    if many {
        args.extend(["--paginate", "--jq", ".[]"]);
    }
    match run("gh", &args, Some(root)).await {
        Ok(out) => Ok(out),
        Err(Error::Failed { stderr, .. }) => Err(reason(&stderr)),
        Err(e) => Err(e.to_string()),
    }
}

/// A text written on GitHub: its Markdown and, where GitHub gave it, the
/// HTML it renders to, whose images are addresses that can be fetched.
#[derive(Debug, Clone, Default, PartialEq)]
struct Text {
    markdown: String,
    html: String,
}

#[derive(Deserialize)]
struct User {
    login: String,
}

fn login(user: &Option<User>) -> String {
    user.as_ref()
        .map_or_else(|| "ghost".to_string(), |u| u.login.clone())
}

/// A pull request or an issue from what GitHub's API answered: the issue,
/// the pull request when it is one, and the comments and reviews, each a
/// JSON value after another; with every text it holds, the body first, for
/// the images they show.
fn read(
    issue: &str,
    pull: Option<&str>,
    comments: &str,
    reviews: &str,
) -> Result<(Thread, Vec<Text>), String> {
    #[derive(Deserialize)]
    struct Titled {
        title: String,
    }
    #[derive(Deserialize)]
    struct Issue {
        number: u64,
        title: String,
        html_url: String,
        user: Option<User>,
        body: Option<String>,
        body_html: Option<String>,
        #[serde(default)]
        state: String,
        #[serde(default)]
        created_at: String,
        #[serde(default)]
        updated_at: String,
        #[serde(default)]
        labels: Vec<Label>,
        #[serde(default)]
        assignees: Vec<User>,
        milestone: Option<Titled>,
    }
    #[derive(Deserialize)]
    struct Ref {
        #[serde(rename = "ref")]
        name: String,
    }
    #[derive(Deserialize)]
    struct PullRead {
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        merged: bool,
        head: Ref,
        base: Ref,
        #[serde(default)]
        additions: u64,
        #[serde(default)]
        deletions: u64,
        #[serde(default)]
        changed_files: u64,
        #[serde(default)]
        requested_reviewers: Vec<User>,
    }
    #[derive(Deserialize)]
    struct Written {
        user: Option<User>,
        body: Option<String>,
        body_html: Option<String>,
        #[serde(default)]
        state: String,
        created_at: Option<String>,
        submitted_at: Option<String>,
    }
    let unread = |e: serde_json::Error| format!("GitHub answered what cannot be read: {e}");
    let text = |markdown: Option<String>, html: Option<String>| Text {
        markdown: markdown.unwrap_or_default().trim().replace("\r\n", "\n"),
        html: html.unwrap_or_default(),
    };
    let each = |out: &str| -> Result<Vec<Written>, String> {
        serde_json::Deserializer::from_str(out)
            .into_iter::<Written>()
            .collect::<Result<_, _>>()
            .map_err(unread)
    };
    let issue: Issue = serde_json::from_str(issue).map_err(unread)?;
    let pull: Option<PullRead> = pull.map(serde_json::from_str).transpose().map_err(unread)?;
    let state = match &pull {
        Some(p) if p.merged => "merged".to_string(),
        Some(p) if p.draft && issue.state == "open" => "draft".to_string(),
        _ => issue.state.to_lowercase(),
    };
    let comments = each(comments)?.into_iter().map(|c| {
        let at = c.created_at.clone().unwrap_or_default();
        (
            login(&c.user),
            Did::Commented,
            at,
            text(c.body, c.body_html),
        )
    });
    let reviews = each(reviews)?.into_iter().filter_map(|r| {
        let did = match r.state.as_str() {
            "APPROVED" => Did::Approved,
            "CHANGES_REQUESTED" => Did::ChangesRequested,
            "DISMISSED" => Did::Dismissed,
            "PENDING" => return None,
            _ if r.body.as_deref().unwrap_or_default().trim().is_empty() => return None,
            _ => Did::Reviewed,
        };
        let at = r.submitted_at.clone().unwrap_or_default();
        Some((login(&r.user), did, at, text(r.body, r.body_html)))
    });
    let mut said: Vec<_> = comments.chain(reviews).collect();
    said.sort_by(|a, b| a.2.cmp(&b.2));
    let body = text(issue.body, issue.body_html);
    let author = login(&issue.user);
    let texts = std::iter::once(body.clone())
        .chain(said.iter().map(|s| s.3.clone()))
        .collect();
    let thread = Thread {
        item: Item {
            number: issue.number,
            title: issue.title,
            url: issue.html_url,
            author: author.clone(),
            draft: pull.as_ref().is_some_and(|p| p.draft),
            created_at: issue.created_at.clone(),
            updated_at: issue.updated_at,
            labels: issue.labels,
            assignees: issue.assignees.into_iter().map(|a| a.login).collect(),
            milestone: issue.milestone.map(|m| m.title),
            pull: pull.map(|p| Pull {
                head: p.head.name,
                base: p.base.name,
                additions: p.additions,
                deletions: p.deletions,
                changed_files: p.changed_files,
                requested: p.requested_reviewers.into_iter().map(|r| r.login).collect(),
                ..Default::default()
            }),
        },
        state,
        body: Post {
            author,
            did: Did::Opened,
            at: issue.created_at,
            body: body.markdown,
        },
        said: said
            .into_iter()
            .map(|(author, did, at, text)| Post {
                author,
                did,
                at,
                body: text.markdown,
            })
            .collect(),
        images: Vec::new(),
    };
    Ok((thread, texts))
}

/// Each image of `fetched`, in the order the posts show them, as far as
/// `most` bytes hold.
fn carried(fetched: Vec<(String, std::path::PathBuf, &'static str)>, most: usize) -> Vec<Image> {
    let mut bytes = 0;
    let mut images = Vec::new();
    for (address, file, mime) in fetched {
        let Ok(read) = std::fs::read(&file) else {
            continue;
        };
        if bytes + read.len() > most {
            continue;
        }
        bytes += read.len();
        images.push(Image {
            address,
            mime: mime.to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(read),
        });
    }
    images
}

/// The images `text` shows, each by the address its Markdown names and the
/// one to fetch it at. GitHub renders an image of a private repository at
/// an address signed for a few minutes, which its HTML holds: the HTML's
/// images stand for the Markdown's in their order when they are as many,
/// and otherwise only by the address an image says it was written as.
fn images(text: &Text) -> Vec<(String, String)> {
    let written: Vec<String> = written_images(&text.markdown)
        .into_iter()
        .filter(|w| w.starts_with("https://") || w.starts_with("http://"))
        .collect();
    let rendered = rendered_images(&text.html);
    if written.len() == rendered.len() {
        return written
            .into_iter()
            .zip(rendered.into_iter().map(|(src, _)| src))
            .collect();
    }
    written
        .into_iter()
        .map(|w| {
            let fetched = rendered
                .iter()
                .find(|(_, canonical)| canonical.as_deref() == Some(w.as_str()))
                .map_or_else(|| w.clone(), |(src, _)| src.clone());
            (w, fetched)
        })
        .collect()
}

/// The addresses of the images Markdown shows, as `![…](…)` or as an
/// `<img>` tag, in the order written.
fn written_images(markdown: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    for (at, _) in markdown.match_indices("![") {
        let rest = &markdown[at..];
        let Some(open) = rest.find("](") else {
            continue;
        };
        let target = rest[open + 2..].trim_start();
        let address = match target.strip_prefix('<') {
            Some(inside) => inside.split('>').next(),
            None => target.split([')', ' ', '\n', '\t']).next(),
        };
        if let Some(address) = address.filter(|a| !a.is_empty()) {
            found.push((at, address.to_string()));
        }
    }
    for (at, tag) in tags(markdown, "img") {
        if let Some(src) = attribute(tag, "src") {
            found.push((at, src));
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, a)| a).collect()
}

/// The images HTML shows, but emoji: each by its `src` and the address it
/// says it was written as, when it says.
fn rendered_images(html: &str) -> Vec<(String, Option<String>)> {
    tags(html, "img")
        .into_iter()
        .filter(|(_, tag)| !attribute(tag, "class").is_some_and(|c| c.contains("emoji")))
        .filter_map(|(_, tag)| Some((attribute(tag, "src")?, attribute(tag, "data-canonical-src"))))
        .collect()
}

/// Every `<name …>` tag of `text`, where it starts and what it holds up to
/// its `>`.
fn tags<'a>(text: &'a str, name: &str) -> Vec<(usize, &'a str)> {
    let open = format!("<{name}");
    text.match_indices(&open)
        .filter_map(|(at, _)| {
            let rest = &text[at + open.len()..];
            rest.starts_with(|c: char| c.is_whitespace())
                .then(|| rest.find('>').map(|end| (at, &rest[..end])))
                .flatten()
        })
        .collect()
}

/// The value of the attribute `name` of a tag's insides, quoted or not,
/// its character references read.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before = rest[..at].chars().next_back();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let raw = match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next()?,
            _ => value
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()?,
        };
        return Some(
            raw.replace("&quot;", "\"")
                .replace("&#39;", "'")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
        );
    }
}

/// The most bytes an image fetched may hold.
const MOST_BYTES: u64 = 32 * 1024 * 1024;

/// Each image of `images` fetched into `dir`, in their order: the address it
/// was written with, the file it was fetched to and the type it was served
/// as. An image that cannot be fetched, or is served as no image, is left
/// out.
async fn fetch(
    dir: &Path,
    images: Vec<(String, String)>,
) -> Vec<(String, std::path::PathBuf, &'static str)> {
    if images.is_empty() || std::fs::create_dir_all(dir).is_err() {
        return Vec::new();
    }
    let mut fetching = tokio::task::JoinSet::new();
    for (n, (written, address)) in images.into_iter().enumerate() {
        let file = dir.join(n.to_string());
        fetching.spawn(async move {
            let most = MOST_BYTES.to_string();
            let file_arg = file.to_string_lossy().into_owned();
            let args = [
                "-sSfL",
                "--max-time",
                "30",
                "--max-filesize",
                &most,
                "-o",
                &file_arg,
                "-w",
                "%{content_type}",
                &address,
            ];
            let kind = run("curl", &args, None).await.ok()?;
            Some((n, written, file, mime(&kind)?))
        });
    }
    let mut fetched = Vec::new();
    while let Some(done) = fetching.join_next().await {
        if let Ok(Some(one)) = done {
            fetched.push(one);
        }
    }
    fetched.sort_by_key(|(n, ..)| *n);
    fetched
        .into_iter()
        .map(|(_, written, file, mime)| (written, file, mime))
        .collect()
}

/// The type of an image as it was served, or none when what was served is
/// no image a page shows.
fn mime(content_type: &str) -> Option<&'static str> {
    let kind = content_type.split(';').next()?.trim().to_ascii_lowercase();
    Some(match kind.as_str() {
        "image/png" => "image/png",
        "image/jpeg" | "image/jpg" => "image/jpeg",
        "image/gif" => "image/gif",
        "image/webp" => "image/webp",
        "image/svg+xml" => "image/svg+xml",
        "image/avif" => "image/avif",
        "image/bmp" => "image/bmp",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PULL_ISSUE: &str = r#"{
        "number": 7, "title": "Fix the parser", "html_url": "https://github.com/o/r/pull/7",
        "user": {"login": "alice"}, "body": "It broke on tabs.\r\n", "body_html": "<p>It broke on tabs.</p>",
        "state": "open", "created_at": "2026-11-01T09:00:00Z", "updated_at": "2026-11-05T00:00:00Z",
        "labels": [{"name": "bug", "color": "d73a4a"}], "assignees": [{"login": "you"}],
        "milestone": {"title": "v1"}
    }"#;
    const PULL: &str = r#"{"draft": false, "merged": false,
        "head": {"ref": "fix-parser", "repo": {"full_name": "o/r"}}, "base": {"ref": "main"},
        "additions": 10, "deletions": 2, "changed_files": 3,
        "requested_reviewers": [{"login": "rev"}]}"#;

    fn text(markdown: &str, html: &str) -> Text {
        Text {
            markdown: markdown.into(),
            html: html.into(),
        }
    }

    #[test]
    fn a_pull_request_reads_with_its_body_and_what_was_said_on_it_oldest_first() {
        let comments =
            r#"{"user": {"login": "bob"}, "body": "Later.", "created_at": "2026-11-03T00:00:00Z"}"#;
        let reviews = concat!(
            r#"{"user": {"login": "carol"}, "body": "", "state": "APPROVED", "submitted_at": "2026-11-04T00:00:00Z"}"#,
            "\n",
            r#"{"user": {"login": "dave"}, "body": "", "state": "COMMENTED", "submitted_at": "2026-11-02T00:00:00Z"}"#,
            "\n",
            r#"{"user": {"login": "erin"}, "body": "Looks fine.", "state": "COMMENTED", "submitted_at": "2026-11-02T00:00:00Z"}"#,
            "\n",
            r#"{"user": {"login": "fay"}, "body": "draft", "state": "PENDING", "submitted_at": null}"#,
        );
        let (thread, texts) = read(PULL_ISSUE, Some(PULL), comments, reviews).unwrap();
        assert_eq!(
            thread.body,
            Post {
                author: "alice".into(),
                did: Did::Opened,
                at: "2026-11-01T09:00:00Z".into(),
                body: "It broke on tabs.".into(),
            }
        );
        let said: Vec<_> = thread
            .said
            .iter()
            .map(|p| (p.author.as_str(), p.did, p.body.as_str()))
            .collect();
        assert_eq!(
            said,
            [
                ("erin", Did::Reviewed, "Looks fine."),
                ("bob", Did::Commented, "Later."),
                ("carol", Did::Approved, ""),
            ]
        );
        assert_eq!(texts.len(), 4);
        assert_eq!(thread.state, "open");
        let item = &thread.item;
        assert_eq!(item.labels[0].name, "bug");
        assert_eq!(item.assignees, ["you"]);
        assert_eq!(item.milestone.as_deref(), Some("v1"));
        let pull = item.pull.as_ref().unwrap();
        assert_eq!(
            (pull.head.as_str(), pull.base.as_str()),
            ("fix-parser", "main")
        );
        assert_eq!(
            (pull.additions, pull.deletions, pull.changed_files),
            (10, 2, 3)
        );
        assert_eq!(pull.requested, ["rev"]);
    }

    #[test]
    fn an_issue_with_no_body_and_no_author_reads_as_one_by_a_ghost_with_nothing_said() {
        let issue = r#"{
            "number": 3, "title": "Crash", "html_url": "https://github.com/o/r/issues/3",
            "user": null, "body": null, "state": "open", "created_at": "2026-11-01T09:00:00Z"
        }"#;
        let (thread, _) = read(issue, None, "", "").unwrap();
        assert_eq!(thread.body.author, "ghost");
        assert_eq!(thread.body.body, "");
        assert_eq!(thread.item.pull, None);
    }

    #[test]
    fn a_pull_request_stands_as_a_draft_while_open_and_as_merged_once_merged() {
        let stands = |pull: &str| read(PULL_ISSUE, Some(pull), "", "").unwrap().0.state;
        assert_eq!(
            stands(&PULL.replace(r#""draft": false"#, r#""draft": true"#)),
            "draft"
        );
        assert_eq!(
            stands(&PULL.replace(r#""merged": false"#, r#""merged": true"#)),
            "merged"
        );
    }

    #[test]
    fn images_are_carried_in_order_as_far_as_the_bytes_allowed_hold_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str, n: usize| {
            let path = dir.path().join(name);
            std::fs::write(&path, vec![1u8; n]).unwrap();
            path
        };
        let fetched = vec![
            ("https://x/a".to_string(), file("a", 6), "image/png"),
            ("https://x/b".to_string(), file("b", 6), "image/png"),
            ("https://x/c".to_string(), file("c", 3), "image/gif"),
        ];
        let carried = carried(fetched, 10);
        let addresses: Vec<_> = carried.iter().map(|i| i.address.as_str()).collect();
        assert_eq!(addresses, ["https://x/a", "https://x/c"]);
        assert_eq!(carried[1].mime, "image/gif");
        assert_eq!(carried[1].data, "AQEB");
    }

    #[test]
    fn an_image_is_known_by_the_type_it_was_served_as() {
        assert_eq!(mime("image/png"), Some("image/png"));
        assert_eq!(mime("image/jpg; charset=binary"), Some("image/jpeg"));
        assert_eq!(mime("text/html; charset=utf-8"), None);
    }

    #[test]
    fn the_images_markdown_shows_are_found_in_the_order_written_however_written() {
        let markdown = concat!(
            "![a](https://x/a.png \"title\") then <img width=\"10\" src=\"https://x/b.png\">\n",
            "![c](<https://x/c d.png>) and ![d](d.png) <imgx src=\"no\">"
        );
        assert_eq!(
            written_images(markdown),
            [
                "https://x/a.png",
                "https://x/b.png",
                "https://x/c d.png",
                "d.png"
            ]
        );
    }

    #[test]
    fn a_private_image_is_fetched_at_the_signed_address_its_html_holds_in_its_place() {
        let shown = text(
            "![a](https://github.com/user-attachments/assets/a)\n![b](https://example.com/b.png)",
            concat!(
                r#"<p><a href="x"><img src="https://private-user-images.githubusercontent.com/1.png?jwt=t&amp;s=1" alt="a"></a>"#,
                r#"<img src="https://camo.githubusercontent.com/2" data-canonical-src="https://example.com/b.png"></p>"#,
            ),
        );
        assert_eq!(
            images(&shown),
            [
                (
                    "https://github.com/user-attachments/assets/a".to_string(),
                    "https://private-user-images.githubusercontent.com/1.png?jwt=t&s=1".to_string()
                ),
                (
                    "https://example.com/b.png".to_string(),
                    "https://camo.githubusercontent.com/2".to_string()
                ),
            ]
        );
    }

    #[test]
    fn images_the_html_does_not_match_one_for_one_are_fetched_by_what_they_say_or_as_written() {
        let shown = text(
            "`![no](https://x/code.png)` ![b](https://example.com/b.png) ![c](https://x/c.png)",
            concat!(
                r#"<img class="emoji" src="https://x/smile.png">"#,
                r#"<img src="https://camo/2" data-canonical-src="https://example.com/b.png">"#,
            ),
        );
        assert_eq!(
            images(&shown),
            [
                (
                    "https://x/code.png".to_string(),
                    "https://x/code.png".to_string()
                ),
                (
                    "https://example.com/b.png".to_string(),
                    "https://camo/2".to_string()
                ),
                ("https://x/c.png".to_string(), "https://x/c.png".to_string()),
            ]
        );
    }
}
