//! The Claude Code plugin every session adroit starts loads, which tells
//! Claude of the `adroit` command it can run from there.

use std::path::Path;

const MANIFEST: &str = include_str!("../../plugin/.claude-plugin/plugin.json");
const START_SESSION: &str = include_str!("../../plugin/skills/start-session/SKILL.md");

/// The skill, naming `program` as the command to run.
pub(crate) fn start_session(program: &Path) -> String {
    START_SESSION.replace("@ADROIT@", &crate::shell_word(program))
}

/// Write the plugin into `dir`, naming `program`, over what an earlier adroit
/// wrote there.
pub fn install(program: &Path, dir: &Path) -> std::io::Result<()> {
    let manifest = dir.join(".claude-plugin");
    std::fs::create_dir_all(&manifest)?;
    std::fs::write(manifest.join("plugin.json"), MANIFEST)?;
    let skill = dir.join("skills").join("start-session");
    std::fs::create_dir_all(&skill)?;
    std::fs::write(skill.join("SKILL.md"), start_session(program))
}

/// The arguments that load the plugin, written afresh into `dir` for the
/// session about to start; none when it cannot be written, as the session
/// runs without it.
pub fn args(program: &Path, dir: &Path) -> Vec<String> {
    match install(program, dir) {
        Ok(()) => super::plugin_args(dir),
        Err(_) => Vec::new(),
    }
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
    fn the_plugin_is_written_as_claude_code_loads_one() {
        let dir = tempfile::tempdir().unwrap();
        install(Path::new("/bin/adroit"), dir.path()).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".claude-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], "adroit");
        let skill =
            std::fs::read_to_string(dir.path().join("skills/start-session/SKILL.md")).unwrap();
        assert!(skill.starts_with("---\nname: start-session\n"));
    }

    #[test]
    fn writing_it_again_replaces_what_an_earlier_adroit_wrote() {
        let dir = tempfile::tempdir().unwrap();
        install(Path::new("/old/adroit"), dir.path()).unwrap();
        install(Path::new("/new/adroit"), dir.path()).unwrap();
        let skill =
            std::fs::read_to_string(dir.path().join("skills/start-session/SKILL.md")).unwrap();
        assert!(skill.contains("/new/adroit session start"));
        assert!(!skill.contains("/old/adroit"));
    }
}
