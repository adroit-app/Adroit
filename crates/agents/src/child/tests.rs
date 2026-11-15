use super::*;
use crate::testing::{program, started};

#[tokio::test]
#[cfg_attr(windows, ignore = "fakes the agent with a shell script")]
async fn a_conversation_reads_the_lines_its_child_answers_until_one_settles_it() {
    let (dir, child) = program(
        "agent",
        r#"IFS= read -r line
printf '%s' "$line" > asked
echo 'not json'
printf '{"id":"other"}\r\n'
echo '{"id":"adroit","answer":42}'
exec sleep 30"#,
    );

    let got = converse(
        started(&child, &[]),
        dir.path(),
        Duration::from_secs(5),
        async |talk: &mut Conversation| {
            talk.write("{\"id\":\"adroit\"}\n").await?;
            talk.until(|line| {
                let reply: serde_json::Value = serde_json::from_str(line).ok()?;
                (reply["id"] == "adroit").then(|| reply["answer"].clone())
            })
            .await
        },
    )
    .await;

    assert_eq!(got, Some(serde_json::json!(42)));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("asked")).unwrap(),
        "{\"id\":\"adroit\"}"
    );
}

#[tokio::test]
#[cfg_attr(windows, ignore = "fakes the agent with a shell script")]
async fn a_child_that_does_not_answer_in_time_is_given_up_on_and_killed() {
    let (dir, child) = program("agent", "echo $$ > pid\necho ready\nexec sleep 30");

    let started_at = std::time::Instant::now();
    let got = converse(
        started(&child, &[]),
        dir.path(),
        Duration::from_secs(2),
        async |talk: &mut Conversation| {
            talk.until(|line| (line == "ready").then_some(())).await?;
            talk.until(|_| None::<()>).await
        },
    )
    .await;

    assert_eq!(got, None);
    assert!(started_at.elapsed() < Duration::from_secs(5));
    let pid = std::fs::read_to_string(dir.path().join("pid")).unwrap();
    let alive = std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap()
        .success();
    assert!(!alive);
}

#[tokio::test]
#[cfg_attr(windows, ignore = "fakes the agent with a shell script")]
async fn a_child_that_ends_before_settling_it_answers_nothing() {
    let (dir, child) = program("agent", "echo '{}'");

    let got = converse(
        started(&child, &[]),
        dir.path(),
        Duration::from_secs(5),
        async |talk: &mut Conversation| talk.until(|_| None::<()>).await,
    )
    .await;

    assert_eq!(got, None);
}

#[tokio::test]
async fn a_program_that_cannot_start_answers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let got = converse(
        started(&dir.path().join("missing"), &[]),
        dir.path(),
        Duration::from_secs(5),
        async |_: &mut Conversation| Some(()),
    )
    .await;

    assert_eq!(got, None);
}
