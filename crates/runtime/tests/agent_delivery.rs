use std::path::Path;

use adroit_model::Agent;
use adroit_runtime::{execution::deliver, settings::Settings};

#[tokio::test]
async fn codex_delivery_targets_a_uuid_and_passes_the_envelope_as_one_argument() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("queue.sh");
    let captured = dir.path().join("args");
    std::fs::write(&script, "printf '%s\\n' \"$CODEX_HOME\" > \"$ADROIT_QUEUE_ARGS\"\nprintf '%s\\n' \"$@\" >> \"$ADROIT_QUEUE_ARGS\"\n").unwrap();
    let mut settings = Settings::default();
    settings.codex.command = vec!["/bin/sh".into(), script.to_string_lossy().into_owned()];
    settings.codex.env.insert(
        "ADROIT_QUEUE_ARGS".into(),
        captured.to_string_lossy().into_owned(),
    );
    settings
        .codex
        .env
        .insert("CODEX_HOME".into(), "/custom/account".into());
    let thread = "123e4567-e89b-12d3-a456-426614174000";
    deliver(
        Agent::Codex,
        settings.clone(),
        Path::new("/tmp").into(),
        thread.into(),
        "Review $(touch never); exact message".into(),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&captured).unwrap(),
        format!(
            "/custom/account\nqueue\n--thread\n{thread}\n--message\nReview $(touch never); exact message\n"
        )
    );
    std::fs::remove_file(&captured).unwrap();
    assert!(
        deliver(
            Agent::Codex,
            settings,
            dir.path().into(),
            "ambiguous-session-name".into(),
            "Review".into()
        )
        .await
        .is_err()
    );
    assert!(!captured.exists());
}

#[tokio::test]
async fn a_failed_queue_command_never_reports_transport_acceptance() {
    let mut settings = Settings::default();
    settings.codex.command = vec!["/usr/bin/false".into()];
    assert!(
        deliver(
            Agent::Codex,
            settings,
            "/tmp".into(),
            "123e4567-e89b-12d3-a456-426614174000".into(),
            "Report".into()
        )
        .await
        .is_err()
    );
}
