//! Builders for what the adapter tests start from: stand-in programs, the
//! lines they answer with, and the screens and files adroit reads back.

use std::path::{Path, PathBuf};

use crate::Started;

/// A stand-in program named `name` that runs `body`, a shell script, in a
/// directory of its own: the directory, gone once dropped, and the
/// program's path.
pub fn program(name: &str, body: &str) -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join(name);
    std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, program)
}

/// `program` started with the arguments a probe asks for and nothing else.
pub fn launching(program: &Path) -> impl FnOnce(Vec<String>) -> Started + use<> {
    let program = program.to_str().unwrap().to_string();
    move |args| (program, args, Vec::new())
}

/// `program` started with `args` and nothing else.
pub fn started(program: &Path, args: &[&str]) -> Started {
    (
        program.to_str().unwrap().to_string(),
        args.iter().map(|a| a.to_string()).collect(),
        Vec::new(),
    )
}

/// The script of a Codex app server that answers initializing, keeps the
/// request after it in `capture`, in the directory it runs in, and answers
/// it with `answer`.
pub fn app_server(capture: &str, answer: &str) -> String {
    format!(
        "read line\necho '{{\"id\":\"adroit-init\",\"result\":{{}}}}'\nread line\nIFS= read -r line\nprintf '%s\\n' \"$line\" > {capture}\necho '{answer}'\nexec sleep 30"
    )
}

/// Claude Code's answer to adroit's control request, carrying `response`.
pub fn control_success(response: &str) -> String {
    format!(
        r#"{{"type":"control_response","response":{{"subtype":"success","request_id":"adroit","response":{response}}}}}"#
    )
}

/// Claude Code refusing adroit's control request with `error`.
pub fn control_error(error: &str) -> String {
    format!(
        r#"{{"type":"control_response","response":{{"subtype":"error","request_id":"adroit","error":"{error}"}}}}"#
    )
}

/// A screen showing `text`, a row a line.
pub fn screen(text: &str) -> Vec<String> {
    text.lines().map(String::from).collect()
}

/// A screen kept under `tests/screens`; `claude::dialog`'s tests tell where
/// each was recorded.
pub fn recorded(name: &str) -> Vec<String> {
    let path = format!("{}/tests/screens/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    screen(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}")))
}

/// `text` written onto the end of the file at `path`, made if missing.
pub fn append(path: &Path, text: &str) {
    use std::io::Write;
    std::fs::File::options()
        .create(true)
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}
