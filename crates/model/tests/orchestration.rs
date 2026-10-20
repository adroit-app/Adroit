use adroit_model::{
    AttemptId, BindingError, LogicalSessionId, RunId, SessionBinding, SessionRole, TaskId,
};

#[test]
fn a_leader_can_discuss_an_idea_before_starting_a_run() {
    let id = LogicalSessionId::new();
    let binding = SessionBinding::leader(id, None);

    assert_eq!(binding.logical_id(), id);
    assert_eq!(binding.role(), SessionRole::Leader);
    assert_eq!(binding.run_id(), None);
    assert_eq!(binding.leader_id(), id);
    assert_eq!(binding.work(), None);
}

#[test]
fn a_worker_records_its_run_task_attempt_and_leader_explicitly() {
    let id = LogicalSessionId::new();
    let leader = LogicalSessionId::new();
    let run = RunId::new();
    let task = TaskId::new();
    let attempt = AttemptId::new();
    let binding = SessionBinding::worker(id, leader, run, task, attempt).unwrap();

    assert_eq!(binding.logical_id(), id);
    assert_eq!(binding.role(), SessionRole::Worker);
    assert_eq!(binding.leader_id(), leader);
    assert_eq!(binding.run_id(), Some(run));
    assert_eq!(binding.work(), Some((task, attempt)));
}

#[test]
fn a_worker_cannot_be_its_own_leader() {
    let id = LogicalSessionId::new();
    assert_eq!(
        SessionBinding::worker(id, id, RunId::new(), TaskId::new(), AttemptId::new()),
        Err(BindingError::OwnLeader)
    );
}

#[test]
fn logical_ids_round_trip_as_uuid_strings_and_reject_invalid_input() {
    let id = LogicalSessionId::new();
    assert_eq!(id.to_string().parse::<LogicalSessionId>().unwrap(), id);
    let encoded = serde_json::to_string(&id).unwrap();
    assert_eq!(encoded, format!("\"{id}\""));
    assert_eq!(
        serde_json::from_str::<LogicalSessionId>(&encoded).unwrap(),
        id
    );
    assert!("terminal-1".parse::<LogicalSessionId>().is_err());
    assert!(serde_json::from_str::<LogicalSessionId>("\"terminal-1\"").is_err());
}
