use super::*;

use serde_json::json;

fn assistant(content: serde_json::Value) -> String {
    json!({"type": "assistant", "message": {"role": "assistant", "content": content}}).to_string()
}

fn user(content: serde_json::Value) -> String {
    json!({"type": "user", "message": {"role": "user", "content": content}}).to_string()
}

pub(crate) fn said(text: &str) -> String {
    assistant(json!([{"type": "text", "text": text}]))
}

pub(crate) fn tool(id: &str, name: &str, input: serde_json::Value) -> String {
    assistant(json!([{"type": "tool_use", "id": id, "name": name, "input": input}]))
}

fn result(id: &str, content: serde_json::Value) -> String {
    user(json!([{"type": "tool_result", "tool_use_id": id, "content": content}]))
}

fn read(lines: &[String], on_disk: &[&str]) -> Links {
    let mut c = Collector::new("/w", "/home/me");
    for line in lines {
        c.feed(line, &|p| on_disk.iter().any(|d| Path::new(d) == p));
    }
    c.links()
}

pub(crate) fn targets(links: &[Link]) -> Vec<&str> {
    links.iter().map(|l| l.target.as_str()).collect()
}

fn file(path: &str, touch: Touch, line: Option<u32>) -> Link {
    Link {
        kind: Kind::File(touch),
        target: path.into(),
        title: None,
        line,
    }
}

#[test]
fn what_was_said_to_the_session_is_kept_newest_first_without_what_adroit_or_its_agent_put_there() {
    let got = read(
        &[
            user(json!("fix the login")),
            said("Done."),
            user(
                json!([{"type": "text", "text": "now <system-reminder>x</system-reminder>add a test"}]),
            ),
            result("t1", json!("tool output")),
            json!({"type": "user", "isMeta": true, "message": {"content": "Caveat: meta"}})
                .to_string(),
            user(json!("<command-name>/clear</command-name>")),
        ],
        &[],
    );
    let texts: Vec<(u64, &str)> = got
        .prompts
        .iter()
        .map(|p| (p.turn, p.text.as_str()))
        .collect();
    assert_eq!(texts, [(2, "now add a test"), (1, "fix the login")]);
}

#[test]
fn an_address_inside_a_file_written_read_or_searched_is_not_brought_up() {
    let got = read(
        &[
            tool(
                "t1",
                "Edit",
                json!({"file_path": "/w/a.rs", "old_string": "x", "new_string": "see https://claude.ai/artifact/fake"}),
            ),
            tool(
                "t2",
                "Write",
                json!({"file_path": "/w/b.md", "content": "https://written.dev"}),
            ),
            tool("t3", "Read", json!({"file_path": "/w/c.md"})),
            result("t3", json!("1\thttps://read.dev")),
            tool("t4", "Grep", json!({"pattern": "http"})),
            result("t4", json!("c.md:1:https://grepped.dev")),
            // What a command printed, a page summarised for the conversation and
            // what the session said are brought up.
            tool(
                "t5",
                "WebFetch",
                json!({"url": "https://fetched.dev/page", "prompt": "links"}),
            ),
            result("t5", json!("The page links https://inside.dev")),
            tool("t6", "Bash", json!({"command": "gh pr create"})),
            result("t6", json!("https://github.com/o/r/pull/1")),
            said("Published https://claude.ai/artifact/real"),
        ],
        &[],
    );
    assert_eq!(targets(&got.artifacts), ["https://claude.ai/artifact/real"]);
    assert_eq!(
        targets(&got.urls),
        [
            "https://github.com/o/r/pull/1",
            "https://inside.dev",
            "https://fetched.dev/page"
        ]
    );
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn a_file_written_or_edited_is_listed_as_edited() {
    let got = read(
        &[
            tool(
                "t1",
                "Write",
                json!({"file_path": "/w/src/a.rs", "content": ""}),
            ),
            tool("t2", "Edit", json!({"file_path": "src/b.rs"})),
            tool("t3", "NotebookEdit", json!({"notebook_path": "/w/n.ipynb"})),
        ],
        &[],
    );
    assert_eq!(
        got.files,
        [
            file("/w/n.ipynb", Touch::Edited, None),
            file("/w/src/b.rs", Touch::Edited, None),
            file("/w/src/a.rs", Touch::Edited, None),
        ]
    );
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn a_file_read_and_later_edited_is_listed_once_as_edited() {
    let got = read(
        &[
            tool("t1", "Edit", json!({"file_path": "/w/a.rs"})),
            tool("t2", "Read", json!({"file_path": "/w/a.rs"})),
            tool("t3", "Read", json!({"file_path": "/w/b.rs"})),
        ],
        &[],
    );
    assert_eq!(
        got.files,
        [
            file("/w/b.rs", Touch::Read, None),
            file("/w/a.rs", Touch::Edited, None),
        ]
    );
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn a_path_named_in_a_reply_is_listed_only_when_it_is_there() {
    let got = read(
        &[said(
            "See `crates/x/y.rs:12:3`, then nope/z.rs and ~/notes/a.md. Also /etc/hosts.",
        )],
        &["/w/crates/x/y.rs", "/home/me/notes/a.md", "/etc/hosts"],
    );
    assert_eq!(
        got.files,
        [
            file("/etc/hosts", Touch::Mentioned, None),
            file("/home/me/notes/a.md", Touch::Mentioned, None),
            file("/w/crates/x/y.rs", Touch::Mentioned, Some(12)),
        ]
    );
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn a_relative_path_is_taken_from_the_session_s_directory_dots_resolved() {
    let got = read(&[said("Look at ./src/../lib.rs.")], &["/w/lib.rs"]);
    assert_eq!(targets(&got.files), ["/w/lib.rs"]);
}

#[test]
fn a_path_only_the_user_or_a_tool_names_is_not_taken_as_mentioned() {
    let got = read(
        &[user(json!("open /w/a.rs")), result("t1", json!("/w/a.rs"))],
        &["/w/a.rs"],
    );
    assert!(got.files.is_empty());
}

#[test]
fn an_address_sheds_the_punctuation_and_brackets_around_it() {
    let got = read(
        &[said(
            "(see https://a.dev/x). <https://b.dev/y> `https://c.dev/z`, \
                 https://d.dev/w/Rust_(language) and \"https://e.dev/q?a=1\"!",
        )],
        &[],
    );
    assert_eq!(
        targets(&got.urls),
        [
            "https://e.dev/q?a=1",
            "https://d.dev/w/Rust_(language)",
            "https://c.dev/z",
            "https://b.dev/y",
            "https://a.dev/x",
        ]
    );
}

#[test]
fn a_markdown_link_is_titled_by_its_text() {
    let got = read(
        &[said(
            "Read [the guide](https://a.dev/guide) or [https://b.dev](https://b.dev).",
        )],
        &[],
    );
    let titles: Vec<_> = got.urls.iter().map(|l| l.title.as_deref()).collect();
    assert_eq!(targets(&got.urls), ["https://b.dev", "https://a.dev/guide"]);
    assert_eq!(titles, [None, Some("the guide")]);
}

#[test]
fn addresses_come_from_the_user_tool_inputs_and_tool_results_too() {
    let got = read(
        &[
            user(json!("look at https://u.dev")),
            tool(
                "t1",
                "WebFetch",
                json!({"url": "https://i.dev/a", "prompt": "x"}),
            ),
            result(
                "t1",
                json!([{"type": "text", "text": "moved to https://r.dev/b"}]),
            ),
        ],
        &[],
    );
    assert_eq!(
        targets(&got.urls),
        ["https://r.dev/b", "https://i.dev/a", "https://u.dev"]
    );
}

#[test]
fn an_artifact_is_its_own_group_titled_by_the_publish_that_made_it() {
    let got = read(
        &[
            tool(
                "t1",
                "Artifact",
                json!({"file_path": "/s/report.html", "title": "Weekly report"}),
            ),
            result(
                "t1",
                json!("Published https://claude.ai/code/artifact/abc-123"),
            ),
            tool("t2", "Artifact", json!({"file_path": "/s/board.html"})),
            result("t2", json!("Published https://claude.ai/code/artifact/def")),
            said("Also https://claude.ai/artifact/xyz?v=2"),
        ],
        &[],
    );
    assert!(got.urls.is_empty());
    let titles: Vec<_> = got.artifacts.iter().map(|l| l.title.as_deref()).collect();
    assert_eq!(
        targets(&got.artifacts),
        [
            "https://claude.ai/artifact/xyz?v=2",
            "https://claude.ai/code/artifact/def",
            "https://claude.ai/code/artifact/abc-123",
        ]
    );
    assert_eq!(titles, [None, Some("board"), Some("Weekly report")]);
}

#[test]
fn something_brought_up_again_moves_to_the_front_once() {
    let got = read(
        &[
            said("https://a.dev"),
            said("https://b.dev"),
            said("https://a.dev again"),
        ],
        &[],
    );
    assert_eq!(targets(&got.urls), ["https://a.dev", "https://b.dev"]);
}

#[test]
fn malformed_lines_other_records_injected_text_and_thinking_are_passed_over() {
    let got = read(
        &[
            "not json".into(),
            json!({"type": "summary", "summary": "https://s.dev"}).to_string(),
            json!({"type": "user", "isMeta": true, "message": {"content": "https://m.dev"}})
                .to_string(),
            user(json!(
                "<system-reminder>see https://r.dev</system-reminder>and https://kept.dev"
            )),
            assistant(json!([{"type": "thinking", "thinking": "https://t.dev"}])),
            assistant(json!("no array")),
        ],
        &[],
    );
    assert_eq!(targets(&got.urls), ["https://kept.dev"]);
}

#[test]
fn only_the_head_of_a_large_tool_result_is_read() {
    let big = format!("https://head.dev {} https://tail.dev", "x".repeat(SCANNED));
    let got = read(&[result("t1", json!(big))], &[]);
    assert_eq!(targets(&got.urls), ["https://head.dev"]);
}

#[test]
fn each_group_keeps_its_newest() {
    let lines: Vec<String> = (0..KEPT + 5)
        .map(|i| said(&format!("https://a.dev/{i}")))
        .collect();
    let got = read(&lines, &[]);
    assert_eq!(got.urls.len(), KEPT);
    assert_eq!(got.urls[0].target, format!("https://a.dev/{}", KEPT + 4));
}

#[test]
fn the_rows_run_artifacts_then_addresses_then_files() {
    let got = read(
        &[
            tool("t1", "Read", json!({"file_path": "/w/a.rs"})),
            said("https://a.dev https://claude.ai/artifact/x"),
        ],
        &[],
    );
    let kinds: Vec<_> = got.rows().map(|l| l.kind).collect();
    assert_eq!(kinds, [Kind::Artifact, Kind::Url, Kind::File(Touch::Read)]);
}
fn sent(id: &str, to: &str) -> String {
    tool(
        id,
        "SendMessage",
        json!({"to": to, "summary": "s", "message": "m"}),
    )
}

fn delivered(id: &str, outcome: serde_json::Value) -> String {
    json!({
        "type": "user",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": id, "content": "delivered"}
        ]},
        "toolUseResult": outcome,
    })
    .to_string()
}

fn arrived(from: &str) -> String {
    json!({
        "type": "user",
        "isMeta": true,
        "message": {"role": "user", "content": "Another Claude session sent a message: …"},
        "origin": {"kind": "peer", "name": from, "body": "b"},
    })
    .to_string()
}

fn peer(name: &str, sent: bool, received: bool) -> Peer {
    Peer {
        name: name.into(),
        sent,
        received,
    }
}

#[test]
fn a_session_messaged_and_one_heard_from_are_its_peers_newest_first() {
    let links = read(
        &[
            sent("t1", "api-tests"),
            delivered("t1", json!({"success": true, "msg_id": "m1"})),
            arrived("review-api"),
        ],
        &[],
    );
    assert_eq!(
        links.peers,
        [
            peer("review-api", false, true),
            peer("api-tests", true, false)
        ]
    );
    assert!(!links.is_empty());
}

#[test]
fn a_peer_messaged_both_ways_is_listed_once() {
    let links = read(
        &[
            arrived("api-tests"),
            sent("t1", "api-tests"),
            delivered("t1", json!({"success": true, "msg_id": "m1"})),
        ],
        &[],
    );
    assert_eq!(links.peers, [peer("api-tests", true, true)]);
}

#[test]
fn a_peer_addressed_with_its_ref_goes_by_its_name() {
    let links = read(
        &[
            sent("t1", "api-tests [17b2f1]"),
            delivered("t1", json!({"success": true, "msg_id": "m1"})),
        ],
        &[],
    );
    assert_eq!(links.peers, [peer("api-tests", true, false)]);
}

#[test]
fn a_message_that_failed_or_went_to_an_agent_inside_the_session_makes_no_peer() {
    let links = read(
        &[
            sent("t1", "nobody"),
            delivered("t1", json!({"success": false})),
            sent("t2", "a1b2c3"),
            delivered(
                "t2",
                json!({"success": true, "pin": {"id": "a1b2c3", "name": "a1b2c3"}}),
            ),
            sent("t3", "never-answered"),
        ],
        &[],
    );
    assert!(links.peers.is_empty());
}

fn stamped(line: String, at: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(&line).unwrap();
    v["timestamp"] = json!(at);
    v.to_string()
}

fn image(path: &str, touch: Touch, at_ms: Option<u64>) -> ImageFile {
    ImageFile {
        path: path.into(),
        touch,
        at_ms,
    }
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn an_image_file_read_or_written_is_listed_apart_from_the_files_with_when() {
    let got = read(
        &[
            stamped(
                tool("t1", "Read", json!({"file_path": "/w/shot.png"})),
                "2026-10-20T00:00:01.000Z",
            ),
            tool("t2", "Read", json!({"file_path": "/w/a.rs"})),
            stamped(
                tool("t3", "Write", json!({"file_path": "plot.SVG"})),
                "2026-10-20T00:00:02.500Z",
            ),
        ],
        &[],
    );
    let at = |ms: u64| Some(1_792_454_400_000 + ms);
    assert_eq!(
        got.images,
        [
            image("/w/plot.SVG", Touch::Edited, at(2_500)),
            image("/w/shot.png", Touch::Read, at(1_000)),
        ]
    );
    assert_eq!(got.files, [file("/w/a.rs", Touch::Read, None)]);
    assert!(got.rows().all(|l| l.target != "/w/shot.png"));
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn an_image_named_in_a_reply_is_listed_when_it_is_there_and_last_brought_up_counts() {
    let got = read(
        &[
            stamped(
                tool("t1", "Read", json!({"file_path": "/w/a.jpg"})),
                "2026-10-20T00:00:01.000Z",
            ),
            stamped(said("See a.jpg and gone.png"), "2026-10-20T00:00:09.000Z"),
        ],
        &["/w/a.jpg"],
    );
    assert_eq!(
        got.images,
        [image("/w/a.jpg", Touch::Read, Some(1_792_454_409_000))]
    );
}

#[test]
fn an_image_is_told_by_its_extension_in_any_case() {
    for yes in [
        "a.png", "b.JPG", "c.jpeg", "d.gif", "e.webp", "f.bmp", "g.svg", "h.heic", "i.tiff",
    ] {
        assert!(is_image(yes), "{yes}");
    }
    for no in ["a.rs", "png", "/w/.png", "notes.md", "/w/dir.png/x"] {
        assert!(!is_image(no), "{no}");
    }
}

#[test]
fn a_record_said_is_taken_in_as_the_line_it_stands_for() {
    let mut c = Collector::new("/w", "/home/me");
    c.said(
        "assistant",
        &json!("2026-10-20T00:00:01.000Z"),
        json!([{"type": "text", "text": "see https://a.dev"}]),
        &|_| false,
    );
    c.said(
        "user",
        &Value::Null,
        json!([{"type": "text", "text": "and https://b.dev"}]),
        &|_| false,
    );
    assert_eq!(targets(&c.links().urls), ["https://b.dev", "https://a.dev"]);
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn an_agent_s_file_tools_are_called_as_claude_code_s_with_the_path_resolved() {
    let mut c = Collector::new("/w", "/home/me");
    let calls: Vec<Value> = [
        ("t1", "read", "src/a.rs"),
        ("t2", "edit", "~/notes/b.md"),
        ("t3", "write", "../c.txt"),
        ("t4", "read", "~"),
    ]
    .into_iter()
    .filter_map(|(id, name, path)| file_call(id, name, Some(path)))
    .collect();
    c.said("assistant", &Value::Null, json!(calls), &|_| false);
    assert_eq!(
        c.links().files,
        [
            file("/w/~", Touch::Read, None),
            file("/c.txt", Touch::Edited, None),
            file("/home/me/notes/b.md", Touch::Edited, None),
            file("/w/src/a.rs", Touch::Read, None),
        ]
    );
}

#[test]
fn a_tool_that_is_not_a_file_tool_or_names_no_path_is_no_file_call() {
    assert_eq!(file_call("t1", "bash", Some("/w/a")), None);
    assert_eq!(file_call("t1", "read", None), None);
}
