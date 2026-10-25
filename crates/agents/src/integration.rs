//! What adroit writes for the agents it starts to load: the skill that
//! tells of its command, and the files it keeps for each agent in this
//! instance's state folder.

use std::io::Write;
use std::path::{Path, PathBuf};

use adroit_model::Agent;

const START_SESSION: &str = include_str!("../plugin/skills/start-session/SKILL.md");

/// The skill, naming `program` as the command to run.
pub(crate) fn start_session(program: &Path) -> String {
    START_SESSION.replace("@ADROIT@", &crate::shell_word(program))
}

/// Write the skill, naming `program`, into a folder of its own in `skills`,
/// over what an earlier adroit wrote there: that folder.
pub(crate) fn write_skill(program: &Path, skills: &Path) -> std::io::Result<PathBuf> {
    let skill = skills.join("start-session");
    std::fs::create_dir_all(&skill)?;
    write_whole(&skill.join("SKILL.md"), &start_session(program))?;
    Ok(skill)
}

/// adroit's own folder for this instance's `agent`.
pub(crate) fn instance_dir(agent: Agent) -> PathBuf {
    use adroit_model::paths::{instance, instance_dir, state_dir};
    instance_dir(&state_dir(), &instance()).join(agent.key())
}

/// Put `contents` at `path`, readable by its user alone, unless it is there
/// already. It is written beside and moved over, so an agent loading it at that
/// moment reads the old file or the new one whole, and whatever stood at
/// `path`, a link included, is replaced rather than written through.
pub(crate) fn write_whole(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
        && std::fs::read(path).is_ok_and(|kept| kept == contents.as_bytes())
    {
        return Ok(());
    }
    let written = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&written)
        .and_then(|mut file| file.write_all(contents.as_bytes()))
        .and_then(|()| std::fs::rename(&written, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&written);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skill_names_adroit_s_tools_rather_than_claude_code_s_messaging() {
        let skill = start_session(Path::new("/adroit"));
        for tool in ["start_session", "send", "sessions", "models"] {
            assert!(skill.contains(&format!("`{tool}`")), "{tool}");
        }
        assert!(!skill.contains("`report`"));
        assert!(!skill.contains("ListAgents"));
        assert!(!skill.contains("SendMessage"));
    }

    #[test]
    fn the_skill_names_the_adroit_it_was_written_by() {
        let skill = start_session(Path::new("/Applications/Adroit.app/Contents/MacOS/adroit"));
        assert!(skill.contains("/Applications/Adroit.app/Contents/MacOS/adroit session start"));
        assert!(skill.contains("/Applications/Adroit.app/Contents/MacOS/adroit terminal start"));
        assert!(!skill.contains("@ADROIT@"));
    }

    #[test]
    fn a_program_path_with_spaces_is_quoted_for_the_shell() {
        let skill = start_session(Path::new("/My Apps/it's/adroit"));
        assert!(skill.contains(r"'/My Apps/it'\''s/adroit' session start"));
    }

    #[test]
    fn the_skill_is_written_in_a_folder_of_its_own_among_the_skills() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write_skill(Path::new("/bin/adroit"), dir.path()).unwrap();
        assert_eq!(skill, dir.path().join("start-session"));
        let written = std::fs::read_to_string(skill.join("SKILL.md")).unwrap();
        assert_eq!(written, start_session(Path::new("/bin/adroit")));
    }
}
