use super::*;

// Records as Claude Code 2.1.303 writes them, trimmed of the fields
// nothing here reads, their text shortened.

fn launch(call: &str, description: &str, at: &str) -> String {
    format!(
        r#"{{"parentUuid":"4370a257","isSidechain":false,"message":{{"model":"claude-opus-5-5","id":"msg_1","type":"message","role":"assistant","content":[{{"type":"tool_use","id":"{call}","name":"Agent","input":{{"description":"{description}","subagent_type":"Explore","model":"opus","prompt":"Repository: /w. Read-only investigation.\n\nGoal: list it.","run_in_background":"true"}},"caller":{{"type":"direct"}}}}],"stop_reason":"tool_use"}},"type":"assistant","uuid":"b898","timestamp":"{at}","sessionId":"e16c"}}"#
    )
}

fn launched(call: &str, id: &str, at: &str) -> String {
    format!(
        r#"{{"parentUuid":"b898","isSidechain":false,"type":"user","message":{{"role":"user","content":[{{"tool_use_id":"{call}","type":"tool_result","content":[{{"type":"text","text":"Async agent launched successfully.\nagentId: {id}"}}]}}]}},"uuid":"8473","timestamp":"{at}","toolUseResult":{{"isAsync":true,"status":"async_launched","agentId":"{id}","description":"d","resolvedModel":"claude-opus-5-5","prompt":"p","outputFile":"/tmp/tasks/{id}.output","canReadOutputFile":true,"canContinueAgent":true}},"sessionId":"e16c"}}"#
    )
}

fn notice(call: Option<&str>, id: &str, status: &str) -> String {
    let call = call
        .map(|c| format!("<tool-use-id>{c}</tool-use-id>\n"))
        .unwrap_or_default();
    format!(
        "<task-notification>\n<task-id>{id}</task-id>\n{call}<output-file>/tmp/tasks/{id}.output</output-file>\n<status>{status}</status>\n<summary>Agent \"d\" finished</summary>\n<usage><subagent_tokens>120123</subagent_tokens><tool_uses>18</tool_uses><duration_ms>120733</duration_ms></usage>\n</task-notification>"
    )
}

fn enqueued(text: &str, at: &str) -> String {
    serde_json::json!({
        "type": "queue-operation", "operation": "enqueue",
        "timestamp": at, "sessionId": "e16c", "content": text,
    })
    .to_string()
}

fn removed(text: &str, at: &str) -> String {
    serde_json::json!({
        "type": "queue-operation", "operation": "remove",
        "timestamp": at, "sessionId": "e16c", "content": text,
        "reason": "absorbed_mid_turn",
    })
    .to_string()
}

fn attached(text: &str, at: &str) -> String {
    serde_json::json!({
        "parentUuid": "afd3", "isSidechain": false,
        "attachment": {"type": "queued_command", "prompt": text, "commandMode": "task-notification",
            "origin": {"kind": "task-notification", "producer": "session-task"}},
        "type": "attachment", "uuid": "0887", "timestamp": at,
    })
    .to_string()
}

fn told(text: &str, at: &str) -> String {
    serde_json::json!({
        "parentUuid": "3a39", "isSidechain": false, "type": "user",
        "message": {"role": "user", "content": text},
        "uuid": "f353", "timestamp": at,
        "origin": {"kind": "task-notification"}, "promptSource": "system",
    })
    .to_string()
}

fn resumed(id: &str, at: &str) -> String {
    serde_json::json!({
            "type": "user", "timestamp": at,
            "message": {"role": "user", "content": [
                {"tool_use_id": "toolu_send", "type": "tool_result", "content": [{"type": "text", "text": "{\"success\":true}"}]}
            ]},
            "toolUseResult": {"success": true, "message": "Resuming agent", "resumedAgentId": id,
                "pin": {"id": id, "name": id, "ref": "0b6dfa"}},
        })
        .to_string()
}

fn read(lines: &[String]) -> Vec<Agent> {
    let mut c = Collector::default();
    for line in lines {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            c.feed(&v);
        }
    }
    c.agents()
}

fn standings(agents: &[Agent]) -> Vec<(&str, Standing)> {
    agents
        .iter()
        .map(|a| (a.description.as_str(), a.standing))
        .collect()
}

const T0: &str = "2026-10-19T14:13:00.648Z";
const T1: &str = "2026-10-19T14:13:02.184Z";
const T2: &str = "2026-10-19T14:15:00.648Z";
const T3: &str = "2026-10-19T14:20:00.648Z";

#[test]
fn a_transcript_tells_as_it_grows_whether_a_subagent_still_runs_behind_it() {
    let dir = std::env::temp_dir().join(format!("adroit-behind-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("talk.jsonl");
    let mut behind = Behind::new(&path);
    assert!(!behind.running(), "no transcript yet");
    let line = |text: String| format!("{text}\n");
    crate::testing::append(&path, &line(launch("toolu_a", "Survey", T0)));
    crate::testing::append(&path, &line(launched("toolu_a", "a7cd", T1)));
    assert!(behind.running());
    assert!(behind.running(), "read again with nothing new");
    crate::testing::append(&path, &line(told(&notice(None, "a7cd", "completed"), T2)));
    assert!(!behind.running());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_subagent_launched_runs_named_by_its_description_kind_and_prompt() {
    let got = read(&[
        launch("toolu_a", "Survey configurables", T0),
        launched("toolu_a", "a7cd", T1),
    ]);
    assert_eq!(
        got,
        [Agent {
            call: "toolu_a".into(),
            id: Some("a7cd".into()),
            description: "Survey configurables".into(),
            title: None,
            kind: Some("Explore".into()),
            prompt: "Repository: /w. Read-only investigation.\n\nGoal: list it.".into(),
            standing: Standing::Running,
            started_ms: Some(1_792_419_180_648),
            ended_ms: None,
            steps: Vec::new(),
        }]
    );
}

#[test]
fn a_notice_that_it_ended_says_how_wherever_claude_code_writes_it() {
    let got = read(&[
        launch("toolu_a", "enqueued", T0),
        launched("toolu_a", "aaa", T1),
        launch("toolu_b", "attached", T0),
        launched("toolu_b", "bbb", T1),
        launch("toolu_c", "told", T0),
        launched("toolu_c", "ccc", T1),
        launch("toolu_d", "removed only", T0),
        launched("toolu_d", "ddd", T1),
        enqueued(&notice(Some("toolu_a"), "aaa", "completed"), T2),
        attached(&notice(Some("toolu_b"), "bbb", "failed"), T2),
        told(&notice(Some("toolu_c"), "ccc", "killed"), T2),
        removed(&notice(Some("toolu_d"), "ddd", "completed"), T2),
    ]);
    assert_eq!(
        standings(&got),
        [
            ("removed only", Standing::Running),
            ("told", Standing::Stopped),
            ("attached", Standing::Failed),
            ("enqueued", Standing::Done),
        ]
    );
    assert_eq!(got[1].ended_ms, Some(1_792_419_300_648));
}

#[test]
fn a_notice_without_the_call_finds_the_subagent_by_its_id() {
    let got = read(&[
        launch("toolu_a", "a", T0),
        launched("toolu_a", "a7cd", T1),
        told(&notice(None, "a7cd", "stopped"), T2),
    ]);
    assert_eq!(standings(&got), [("a", Standing::Stopped)]);
}

#[test]
fn the_same_notice_written_again_keeps_when_it_first_ended() {
    let n = notice(Some("toolu_a"), "aaa", "completed");
    let got = read(&[
        launch("toolu_a", "a", T0),
        launched("toolu_a", "aaa", T1),
        enqueued(&n, T2),
        attached(&n, T3),
        told(&n, T3),
    ]);
    assert_eq!(got[0].ended_ms, epoch_ms(T2));
}

#[test]
fn a_subagent_resumed_runs_again_until_its_next_notice() {
    let lines = [
        launch("toolu_a", "a", T0),
        launched("toolu_a", "aaa", T1),
        enqueued(&notice(Some("toolu_a"), "aaa", "completed"), T1),
        resumed("aaa", T2),
    ];
    let got = read(&lines);
    assert_eq!(standings(&got), [("a", Standing::Running)]);
    assert_eq!(got[0].ended_ms, None);
    let mut more = lines.to_vec();
    more.push(enqueued(&notice(Some("toolu_a"), "aaa", "completed"), T3));
    assert_eq!(read(&more)[0].ended_ms, epoch_ms(T3));
}

#[test]
fn a_subagent_run_in_the_foreground_ends_with_its_tool_result() {
    let result = |call: &str, error: bool| {
        serde_json::json!({
            "type": "user", "timestamp": T2,
            "message": {"role": "user", "content": [
                {"tool_use_id": call, "type": "tool_result", "is_error": error, "content": "report"}
            ]},
            "toolUseResult": {"status": "completed"},
        })
        .to_string()
    };
    let got = read(&[
        launch("toolu_a", "ok", T0),
        launch("toolu_b", "broke", T0),
        result("toolu_a", false),
        result("toolu_b", true),
    ]);
    assert_eq!(
        standings(&got),
        [("broke", Standing::Failed), ("ok", Standing::Done)]
    );
}

#[test]
fn running_ones_come_first_the_latest_started_first_then_the_latest_to_end() {
    let mut lines = Vec::new();
    for (i, name) in ["first", "second", "third", "fourth"].iter().enumerate() {
        lines.push(launch(&format!("toolu_{i}"), name, T0));
        lines.push(launched(&format!("toolu_{i}"), &format!("a{i}"), T0));
    }
    lines.push(enqueued(&notice(Some("toolu_2"), "a2", "completed"), T1));
    lines.push(enqueued(&notice(Some("toolu_0"), "a0", "completed"), T2));
    assert_eq!(
        standings(&read(&lines)),
        [
            ("fourth", Standing::Running),
            ("second", Standing::Running),
            ("first", Standing::Done),
            ("third", Standing::Done),
        ]
    );
}

#[test]
fn only_the_latest_to_end_are_kept() {
    let mut lines = Vec::new();
    for i in 0..KEPT_ENDED + 2 {
        lines.push(launch(&format!("toolu_{i}"), &format!("{i}"), T0));
        lines.push(launched(&format!("toolu_{i}"), &format!("a{i}"), T0));
        lines.push(enqueued(
            &notice(Some(&format!("toolu_{i}")), &format!("a{i}"), "completed"),
            T1,
        ));
    }
    let got = read(&lines);
    assert_eq!(got.len(), KEPT_ENDED);
    assert_eq!(got[0].description, format!("{}", KEPT_ENDED + 1));
}

fn send(call: &str, to: &str, summary: &str, at: &str) -> String {
    serde_json::json!({
        "isSidechain": false, "type": "assistant", "timestamp": at,
        "message": {"role": "assistant", "stop_reason": "tool_use", "content": [
            {"type": "tool_use", "id": call, "name": "SendMessage", "input": {
                "to": to, "summary": summary,
                "message": "Change from the user: keep it per window.",
                "type": "message", "recipient": to, "recipient_kind": "agent",
                "content": "Change from the user: keep it…",
            }, "caller": {"type": "direct"}}
        ]},
    })
    .to_string()
}

fn sent(call: &str, outcome: serde_json::Value, at: &str) -> String {
    serde_json::json!({
        "isSidechain": false, "type": "user", "timestamp": at,
        "message": {"role": "user", "content": [
            {"tool_use_id": call, "type": "tool_result",
             "content": [{"type": "text", "text": outcome.to_string()}]}
        ]},
        "toolUseResult": outcome,
    })
    .to_string()
}

fn queued(id: &str) -> serde_json::Value {
    serde_json::json!({
        "success": true,
        "message": format!("Message queued for delivery to {id} at its next tool round."),
        "pin": {"id": id, "name": id, "ref": "d6a1b2"},
    })
}

fn titles(agents: &[Agent]) -> Vec<(&str, Option<&str>)> {
    agents
        .iter()
        .map(|a| (a.description.as_str(), a.title.as_deref()))
        .collect()
}

#[test]
fn a_subagent_is_titled_by_the_summary_of_the_latest_message_delivered_to_it() {
    let got = read(&[
        launch("toolu_a", "Persistent hover panel", T0),
        launched("toolu_a", "a1e4", T0),
        send("toolu_s1", "a1e4", "Hidden state per window", T1),
        sent("toolu_s1", queued("a1e4"), T1),
        send("toolu_s2", "a1e4", "Rename hover to detail", T2),
        sent("toolu_s2", queued("a1e4"), T2),
    ]);
    assert_eq!(
        titles(&got),
        [("Persistent hover panel", Some("Rename hover to detail"))]
    );
}

#[test]
fn a_message_that_resumed_a_finished_subagent_titles_it_too() {
    let got = read(&[
        launch("toolu_a", "Port stage 3", T0),
        launched("toolu_a", "ada0", T0),
        enqueued(&notice(Some("toolu_a"), "ada0", "completed"), T1),
        send("toolu_s", "ada0", "Restructure scripts into modules", T2),
        sent(
            "toolu_s",
            serde_json::json!({
                "success": true, "message": "Resuming agent ada0",
                "resumedAgentId": "ada0", "pin": {"id": "ada0", "name": "ada0", "ref": "0b6dfa"},
            }),
            T2,
        ),
    ]);
    assert_eq!(standings(&got), [("Port stage 3", Standing::Running)]);
    assert_eq!(
        titles(&got),
        [("Port stage 3", Some("Restructure scripts into modules"))]
    );
}

#[test]
fn a_message_not_delivered_or_to_someone_else_leaves_the_title_alone() {
    let got = read(&[
        launch("toolu_a", "Survey", T0),
        launched("toolu_a", "aaa", T0),
        send("toolu_s1", "aaa", "Never answered", T1),
        send("toolu_s2", "aaa", "Refused", T1),
        sent("toolu_s2", serde_json::json!({"success": false}), T1),
        send("toolu_s3", "api-tests", "To a peer", T1),
        sent("toolu_s3", queued("api-tests"), T1),
        send("toolu_s4", "aaa", "", T1),
        sent("toolu_s4", queued("aaa"), T1),
    ]);
    assert_eq!(titles(&got), [("Survey", None)]);
}

#[test]
fn other_tools_other_tasks_and_unknown_statuses_make_no_subagent() {
    let got = read(&[
            serde_json::json!({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": "toolu_b", "name": "Bash", "input": {"command": "ls", "run_in_background": true}}
            ]}})
            .to_string(),
            enqueued(&notice(Some("toolu_b"), "bats811mk", "completed"), T1),
            launch("toolu_a", "a", T0),
            enqueued(&notice(Some("toolu_a"), "aaa", "pending"), T1),
            "not json".into(),
        ]);
    assert_eq!(standings(&got), [("a", Standing::Running)]);
}

// A subagent's own transcript, as Claude Code 2.1.303 writes it.

fn sub_user(text: &str) -> String {
    serde_json::json!({
        "type": "user", "isSidechain": true, "agentId": "a635",
        "message": {"role": "user", "content": text},
    })
    .to_string()
}

fn sub_assistant(content: serde_json::Value) -> String {
    serde_json::json!({
        "type": "assistant", "isSidechain": true, "agentId": "a635",
        "message": {"role": "assistant", "content": content, "stop_reason": "tool_use"},
    })
    .to_string()
}

fn sub_tool(name: &str, input: serde_json::Value) -> String {
    sub_assistant(serde_json::json!([
        {"type": "tool_use", "id": "toolu_x", "name": name, "input": input}
    ]))
}

fn tool(name: &str, about: &str) -> Step {
    Step::Tool {
        name: name.into(),
        about: about.into(),
        outcome: None,
    }
}

#[test]
fn a_subagent_s_steps_are_the_tools_it_called_and_what_it_said() {
    let got = steps(
            [
                sub_user("Repository: /w. Read-only investigation."),
                sub_assistant(serde_json::json!([{"type": "thinking", "thinking": ""}])),
                sub_tool(
                    "Bash",
                    serde_json::json!({"command": "ls -la", "description": "List the files"}),
                ),
                sub_tool(
                    "Read",
                    serde_json::json!({"file_path": "/w/src/lib.rs", "limit": 40}),
                ),
                sub_tool("Grep", serde_json::json!({"pattern": "fn main", "path": "/w"})),
                sub_assistant(
                    serde_json::json!([{"type": "text", "text": "  Found it.\n"}, {"type": "text", "text": " "}]),
                ),
                sub_tool(
                    "Edit",
                    serde_json::json!({"replace_all": false, "old_string": "a", "new_string": "b", "file_path": "/w/a.rs"}),
                ),
                sub_tool("TaskStop", serde_json::json!({"task_id": 3})),
                sub_tool(
                    "SubagentHandback",
                    serde_json::json!({"message": "Findings:\n- one\n"}),
                ),
            ]
            .iter()
            .map(String::as_str),
        );
    assert_eq!(
        got,
        [
            tool("Bash", "List the files"),
            tool("Read", "/w/src/lib.rs"),
            tool("Grep", "fn main"),
            Step::Said("Found it.".into()),
            tool("Edit", "/w/a.rs"),
            tool("TaskStop", ""),
            Step::Said("Findings:\n- one".into()),
        ]
    );
}

#[test]
fn only_the_latest_steps_are_kept_each_cut_short() {
    let mut lines: Vec<String> = (0..STEPS + 3)
        .map(|i| sub_tool("Read", serde_json::json!({"file_path": format!("/w/{i}")})))
        .collect();
    lines.push(sub_assistant(
        serde_json::json!([{"type": "text", "text": "あ".repeat(STEP_TEXT)}]),
    ));
    let got = steps(lines.iter().map(String::as_str));
    assert_eq!(got.len(), STEPS);
    assert_eq!(got[0], tool("Read", "/w/4"));
    let Step::Said(last) = &got[STEPS - 1] else {
        panic!("the last step is what it said");
    };
    assert!(last.len() <= STEP_TEXT + '…'.len_utf8());
    assert!(last.ends_with('…'));
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("adroit-subagents-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("talk/subagents")).unwrap();
    dir.join("talk.jsonl")
}

fn feed(s: &mut Subagents, lines: &[String]) {
    for line in lines {
        s.feed(&serde_json::from_str(line).unwrap());
    }
}

#[test]
fn each_subagent_s_steps_are_read_from_its_own_transcript_as_it_grows() {
    let path = scratch("steps");
    let own = path.with_extension("").join("subagents/agent-aaa.jsonl");
    std::fs::write(
        &own,
        format!(
            "{}\n",
            sub_tool("Read", serde_json::json!({"file_path": "/w/a"}))
        ),
    )
    .unwrap();
    let mut s = Subagents::new(&path);
    feed(
        &mut s,
        &[
            launch("toolu_a", "a", T0),
            launched("toolu_a", "aaa", T1),
            launch("toolu_b", "not launched yet", T0),
        ],
    );
    assert!(s.look());
    assert!(!s.look(), "nothing new");
    assert_eq!(s.agents()[1].steps, [tool("Read", "/w/a")]);
    std::fs::write(
        &own,
        format!(
            "{}\n{}\n",
            sub_tool("Read", serde_json::json!({"file_path": "/w/a"})),
            sub_tool("Read", serde_json::json!({"file_path": "/w/b"}))
        ),
    )
    .unwrap();
    assert!(s.look());
    assert_eq!(
        s.agents()[1].steps,
        [tool("Read", "/w/a"), tool("Read", "/w/b")]
    );
    assert!(s.agents()[0].steps.is_empty());
}

#[test]
fn only_the_end_of_a_long_subagent_transcript_is_read() {
    let path = scratch("tail");
    let own = path.with_extension("").join("subagents/agent-aaa.jsonl");
    let early = sub_tool("Read", serde_json::json!({"file_path": "/w/early"}));
    let filler = sub_user(&"x".repeat(TAIL as usize));
    let late = sub_tool("Read", serde_json::json!({"file_path": "/w/late"}));
    std::fs::write(&own, format!("{early}\n{filler}\n{late}\n")).unwrap();
    let mut s = Subagents::new(&path);
    feed(
        &mut s,
        &[launch("toolu_a", "a", T0), launched("toolu_a", "aaa", T1)],
    );
    s.look();
    assert_eq!(s.agents()[0].steps, [tool("Read", "/w/late")]);
}
