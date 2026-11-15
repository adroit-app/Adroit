//! A pull request or an issue drawn as GitHub draws its conversation: its
//! title and how it stands, then a card for its body and for each thing
//! said on it, each drawn by Arto from what was written.

use adroit_model::github::{Did, Post, Thread};
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
    out.push_str(&format!(
        "<nav class=\"gh-tabs\"><span class=\"gh-tab on\">Conversation<span class=\"gh-count\">{}</span></span></nav>",
        thread.said.len()
    ));
    for post in std::iter::once(&thread.body).chain(&thread.said) {
        out.push_str(&card(post, thread, &when));
    }
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
        }
    }

    #[test]
    fn a_pull_request_heads_with_its_title_state_and_branches_then_a_card_for_each_post() {
        let page = thread_fragment("o/r", &thread(), NOW);
        assert!(page.contains("Fix &lt;the&gt; parser <span class=\"gh-number\">#7</span>"));
        assert!(page.contains("gh-state-open\">Open"));
        assert!(page.contains("<b>@alice</b> wants to merge <span class=\"gh-ref\">fix</span> into <span class=\"gh-ref\">main</span>"));
        assert!(page.contains("2 days ago"));
        assert!(page.contains("Conversation<span class=\"gh-count\">2</span>"));
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
    fn an_issue_says_who_opened_it_and_an_empty_body_says_so() {
        let mut issue = thread();
        issue.item.pull = None;
        issue.body.body = String::new();
        let page = thread_fragment("o/r", &issue, NOW);
        assert!(page.contains("<b>@alice</b> opened this issue"));
        assert!(page.contains("No description provided."));
    }
}
