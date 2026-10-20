use adroit_model::{
    Leaders, LogicalSessionId, RunId, Session, SessionBinding, TerminalBinding, TerminalProgram,
    TerminalResourceId, TerminalSession, build,
};
use std::collections::HashMap;

#[test]
fn a_neovim_terminal_is_listed_by_place_without_becoming_a_worker() {
    let terminal = TerminalSession {
        resource_id: TerminalResourceId::new(),
        id: 7,
        name: "editor".into(),
        cwd: "/plain".into(),
        program: TerminalProgram::Neovim,
        binding: Some(TerminalBinding {
            run_id: RunId::new(),
            worker_id: Some(LogicalSessionId::new()),
        }),
    };
    let tree = build(
        &[],
        std::slice::from_ref(&terminal),
        &HashMap::new(),
        &[],
        None,
    );
    let view = Leaders::from_tree(&tree).unwrap();

    assert_eq!(tree.workspaces[0].worktrees[0].terminals, [terminal]);
    assert!(view.leaders.is_empty());
    assert!(view.unassigned.is_empty());
}

#[test]
fn tools_belong_to_their_run_without_counting_as_workers() {
    let run = RunId::new();
    let leader = Session {
        binding: Some(SessionBinding::leader(LogicalSessionId::new(), Some(run))),
        cwd: "/repo".into(),
        ..Default::default()
    };
    let terminal = TerminalSession {
        cwd: "/elsewhere".into(),
        binding: Some(TerminalBinding {
            run_id: run,
            worker_id: None,
        }),
        ..Default::default()
    };
    let tree = build(&[leader], &[terminal], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();
    assert_eq!(view.leaders[0].tools.len(), 1);
    assert!(view.leaders[0].workers.is_empty());
}

#[test]
fn a_shared_terminal_does_not_inherit_a_run_from_its_directory() {
    let leader = Session {
        binding: Some(SessionBinding::leader(
            LogicalSessionId::new(),
            Some(RunId::new()),
        )),
        cwd: "/repo".into(),
        ..Default::default()
    };
    let terminal = TerminalSession {
        cwd: "/repo".into(),
        ..Default::default()
    };
    let tree = build(&[leader], &[terminal], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();
    assert!(view.leaders[0].tools.is_empty());
    assert_eq!(view.unassigned_terminals.len(), 1);
}
