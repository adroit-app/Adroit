use super::*;
use std::collections::BTreeMap;

/// `settings` as adroit writes them, read back.
pub(crate) fn written_and_read(settings: &Settings) -> Settings {
    Settings::parse(&toml::to_string_pretty(settings).unwrap()).unwrap()
}

#[test]
fn a_file_missing_reads_as_designed_and_one_that_cannot_be_had_names_itself() {
    use adroit_protocol::{ConfigFile, FileText};
    let file = |text| ConfigFile {
        path: "/c/settings.toml".into(),
        text,
    };
    assert_eq!(
        Settings::read(&file(FileText::Missing)),
        Ok(Settings::default())
    );
    let read = Settings::read(&file(FileText::Read {
        text: "zoom = 1.25\n".into(),
    }));
    assert_eq!(read.map(|s| s.zoom), Ok(1.25));
    let broken = Settings::read(&file(FileText::Read {
        text: "zoom = ".into(),
    }))
    .unwrap_err();
    assert!(
        broken.starts_with("Cannot load /c/settings.toml: "),
        "{broken}"
    );
    assert_eq!(
        Settings::read(&file(FileText::Unreadable {
            error: "denied".into()
        })),
        Err("Cannot read /c/settings.toml: denied".into())
    );
}

#[test]
fn each_agent_s_profile_is_edited_as_its_default_profile() {
    let mut s = Settings::parse("[codex]\ncommand = [\"/opt/codex\"]\n").unwrap();
    assert_eq!(s.profile_of(Agent::Codex).launch.command, ["/opt/codex"]);
    assert_eq!(s.profile_of(Agent::Claude).launch, Launch::default());
    s.profile_of_mut(Agent::Claude).launch.via_shell = true;
    assert!(s.profiles[0].launch.via_shell);
}

#[test]
fn a_file_written_before_sessions_could_start_sessions_still_loads_and_loses_that_section() {
    let settings =
        Settings::parse("[orchestration]\nmax_workers = 2\nmax_workers_per_run = 1\n").unwrap();
    assert_eq!(settings.delegation, DelegationSettings::default());
    let written = toml::to_string_pretty(&settings).unwrap();
    assert!(!written.contains("orchestration"), "{written}");
}

#[test]
fn sessions_open_sessions_unless_the_file_switches_it_off() {
    assert!(Settings::default().delegation.enabled);
    let off = Settings::parse("[delegation]\nenabled = false\n").unwrap();
    assert!(!off.delegation.enabled);
}

#[test]
fn a_file_that_capped_sessions_opened_by_sessions_still_loads_and_loses_the_cap() {
    for cap in [0, 20] {
        let settings = Settings::parse(&format!("[delegation]\nmax_started = {cap}\n")).unwrap();
        assert_eq!(settings.delegation, DelegationSettings::default());
        let written = toml::to_string_pretty(&settings).unwrap();
        assert!(!written.contains("max_started"), "{written}");
    }
}

#[test]
fn machine_usage_is_enabled_by_default_and_each_resource_can_be_hidden() {
    let defaults = Settings::parse("").unwrap().usage;
    assert!(defaults.cpu && defaults.gpu && defaults.memory && defaults.storage);
    let settings =
        Settings::parse("[usage]\ncpu = false\ngpu = false\nmemory = true\nstorage = false\n")
            .unwrap();
    assert!(!settings.usage.cpu && !settings.usage.gpu && !settings.usage.storage);
    assert!(settings.usage.memory);
    assert_eq!(written_and_read(&settings), settings);
    assert!(Settings::parse("[usage]\nmemroy = false\n").is_err());
}

#[test]
fn confirming_adroit_s_channel_warning_is_on_unless_the_user_turns_it_off() {
    let claude = |s: &Settings| s.profile_of(Agent::Claude).claude().cloned().unwrap();
    assert!(claude(&Settings::parse("").unwrap()).confirm_channel_warning);
    let settings = Settings::parse("[sessions]\nconfirm_channel_warning = false\n").unwrap();
    assert!(!claude(&settings).confirm_channel_warning);
    assert_eq!(written_and_read(&settings), settings);
}

#[test]
fn a_file_written_before_profiles_that_chose_codex_starts_from_the_codex_profile() {
    assert_eq!(Settings::parse("").unwrap().default_agent(), Agent::Claude);
    let s = Settings::parse(
        r#"agent = "codex"
[codex]
command = ["/opt/bin/codex"]
via_shell = true
[codex.env]
FOO = "bar"
[codex_sessions]
model = "chosen-model"
"#,
    )
    .unwrap();
    assert_eq!(s.profile.as_str(), "codex");
    assert_eq!(s.default_agent(), Agent::Codex);
    let codex = s.profile_of(Agent::Codex);
    assert_eq!(codex.id.as_str(), "codex");
    assert_eq!(codex.home, None);
    assert_eq!(codex.launch.command, ["/opt/bin/codex"]);
    assert!(codex.launch.via_shell);
    assert_eq!(codex.launch.env.get("FOO").map(String::as_str), Some("bar"));
    assert_eq!(codex.codex().unwrap().model, "chosen-model");
    assert_eq!(
        s.profile_of(Agent::Claude).into_owned(),
        AgentProfile::builtin(Agent::Claude)
    );
    assert_eq!(written_and_read(&s), s);
    assert!(Settings::parse(r#"agent = "other""#).is_err());
}

#[test]
fn settings_not_named_are_as_designed() {
    let s = Settings::parse("zoom = 1.25\n").unwrap();
    assert_eq!(s.zoom, 1.25);
    assert_eq!(s.prompt_min_rows, 3);
    assert!(Settings::parse("colour = \"red\"\n").is_err());
}

#[test]
fn agents_name_their_sessions_unless_the_setting_says_not() {
    assert!(Settings::parse("").unwrap().rename_sessions);
    assert!(
        !Settings::parse("rename_sessions = false\n")
            .unwrap()
            .rename_sessions
    );
}

#[test]
fn the_mac_is_kept_awake_unless_the_setting_says_not() {
    assert!(Settings::parse("").unwrap().keep_awake);
    assert!(!Settings::parse("keep_awake = false\n").unwrap().keep_awake);
}

#[test]
fn settings_earlier_versions_kept_for_projects_are_read_and_dropped() {
    let s =
        Settings::parse("zoom = 1.25\nnaming_model = \"haiku\"\nprojects_dir = \"~/Projects\"\n")
            .unwrap();
    assert_eq!(s.zoom, 1.25);
}

#[test]
fn settings_out_of_reason_are_brought_in() {
    let s = Settings {
        zoom: 9.0,
        prompt_min_rows: 0,
        prompt_max_rows: 0,
        font_family: "  ".into(),
        font_size: 99.0,
        font_weight: 1234,
        light_theme: " ".into(),
        dark_theme: "".into(),
        theme: Some("  ".into()),
        keep_awake: true,
        ..Settings::default()
    }
    .sane();
    assert_eq!(s.zoom, 1.75);
    assert_eq!((s.prompt_min_rows, s.prompt_max_rows), (1, 1));
    assert_eq!(s.font_family, "");
    assert_eq!(s.font_size, 32.0);
    assert_eq!(s.font_weight, 900);
    assert_eq!(s.light_theme, LIGHT_THEME);
    assert_eq!(s.dark_theme, DARK_THEME);
    assert_eq!(s.theme, None);
}

#[test]
fn nightfox_and_dawnfox_are_drawn_when_the_settings_name_no_theme() {
    let s = Settings::default();
    assert_eq!(
        (s.dark_theme.as_str(), s.light_theme.as_str()),
        ("Nightfox", "Dawnfox")
    );
}

#[test]
fn the_theme_earlier_versions_called_adroit_s_own_is_nightfox() {
    let s = Settings {
        light_theme: "Adroit".into(),
        dark_theme: " Adroit ".into(),
        theme: Some("Adroit".into()),
        ..Settings::default()
    }
    .sane();
    assert_eq!(s.light_theme, "Nightfox");
    assert_eq!(s.dark_theme, "Nightfox");
    assert_eq!(s.theme.as_deref(), Some("Nightfox"));
}

#[test]
fn the_built_in_font_named_is_the_font_left_unnamed() {
    let s = Settings {
        font_family: format!(" {} ", crate::fonts::FAMILY),
        ..Settings::default()
    }
    .sane();
    assert_eq!(s.font_family, "");
}

#[test]
fn the_theme_follows_the_system_with_a_light_and_a_dark_one_unless_one_appearance_is_chosen() {
    let s = Settings::parse("").unwrap();
    assert_eq!(s.appearance, Appearance::System);
    assert_eq!(s.light_theme, LIGHT_THEME);
    assert_eq!(s.dark_theme, DARK_THEME);
    let s = Settings::parse("light_theme = \"Latte\"\ndark_theme = \"Nord\"\n").unwrap();
    for (appearance, system, theme) in [
        (Appearance::System, ThemeKind::Light, "Latte"),
        (Appearance::System, ThemeKind::Dark, "Nord"),
        (Appearance::Light, ThemeKind::Light, "Latte"),
        (Appearance::Light, ThemeKind::Dark, "Latte"),
        (Appearance::Dark, ThemeKind::Light, "Nord"),
        (Appearance::Dark, ThemeKind::Dark, "Nord"),
    ] {
        let s = Settings {
            appearance,
            ..s.clone()
        };
        assert_eq!(s.theme_in(system), theme, "{appearance:?} in {system:?}");
    }
    let s = Settings::parse("appearance = \"light\"\n").unwrap();
    assert_eq!(s.appearance, Appearance::Light);
    assert_eq!(written_and_read(&s), s);
    assert!(Settings::parse("appearance = \"auto\"\n").is_err());
}

#[test]
fn the_one_theme_earlier_versions_named_takes_the_slot_of_its_kind_and_that_appearance() {
    let s = Settings::parse("theme = \"Nord\"\n").unwrap();
    assert_eq!(s.theme.as_deref(), Some("Nord"));
    let dark = s.clone().migrated(ThemeKind::Dark);
    assert_eq!(
        (dark.appearance, dark.dark_theme.as_str(), dark.theme),
        (Appearance::Dark, "Nord", None)
    );
    assert_eq!(dark.light_theme, LIGHT_THEME);
    let light = Settings::parse("theme = \"Latte\"\n")
        .unwrap()
        .migrated(ThemeKind::Light);
    assert_eq!(
        (light.appearance, light.light_theme.as_str()),
        (Appearance::Light, "Latte")
    );
    assert_eq!(light.dark_theme, DARK_THEME);
    // Written again, the earlier key is gone.
    let written = toml::to_string_pretty(&light).unwrap();
    assert!(!written.contains("\ntheme"), "{written}");
    assert!(written.contains("light_theme = \"Latte\""), "{written}");
    // Settings without it are left as they are.
    let none = Settings::default();
    assert_eq!(none.clone().migrated(ThemeKind::Light), none);
}

#[test]
fn the_console_s_font_comes_first_with_the_built_in_ones_behind_it() {
    let s = Settings {
        font_family: "IntoneMono Nerd Font".into(),
        ..Settings::default()
    };
    let family = s.console_font();
    assert!(family.starts_with("\"IntoneMono Nerd Font\", "), "{family}");
    assert!(family.ends_with("monospace"), "{family}");
    assert_eq!(Settings::default().console_font(), MONO);
}

#[test]
fn the_console_is_drawn_in_the_built_in_nerd_font_with_a_japanese_one_behind_it() {
    let family = Settings::default().console_font();
    assert!(
        family.starts_with(&format!("\"{}\", ", crate::fonts::FAMILY)),
        "{family}"
    );
    assert!(family.contains("\"Hiragino Kaku Gothic ProN\""), "{family}");
}

#[test]
fn settings_written_read_back_the_same() {
    let s = Settings {
        profile: "codex-review".parse().unwrap(),
        profiles: vec![
            AgentProfile {
                launch: Launch {
                    command: vec!["claude".into(), "--model".into(), "opus".into()],
                    via_shell: true,
                    env: BTreeMap::from([("FOO".into(), "1".into())]),
                },
                options: AgentOptions::Claude(SessionOptions {
                    model: "opus".into(),
                    ..SessionOptions::default()
                }),
                home: Some("/Users/me/claude-work".into()),
                ..AgentProfile::builtin(Agent::Claude)
            },
            AgentProfile {
                id: "codex-review".parse().unwrap(),
                name: "Review".into(),
                ..AgentProfile::builtin(Agent::Codex)
            },
        ],
        unshared: Vec::new(),
        delegation: DelegationSettings { enabled: false },
        outside: OutsideSettings { steer: true },
        zoom: 1.1,
        prompt_min_rows: 2,
        prompt_max_rows: 20,
        font_family: "IntoneMono Nerd Font".into(),
        font_size: 14.0,
        font_weight: 300,
        appearance: Appearance::Dark,
        light_theme: "Catppuccin Latte".into(),
        dark_theme: "Nightfox".into(),
        theme: None,
        keep_awake: false,
        notify_waiting: true,
        rename_sessions: true,
        restore_sessions: RestoreSessions::Always,
        shell: ShellLaunch {
            command: vec!["/bin/bash".into(), "-l".into()],
            env: BTreeMap::from([("LANG".into(), "C".into())]),
        },
        neovim: Launch {
            command: vec!["nvim".into(), "--listen".into(), "/tmp/n".into()],
            via_shell: false,
            env: BTreeMap::new(),
        },
        scratch_dir: "~/notes".into(),
        picker: PickerSettings {
            ghq: false,
            roots: vec!["~/src".into()],
        },
        console: ConsoleSettings {
            option_as_alt: OptionAsAlt::Right,
            cursor_blink: true,
        },
        usage: UsageSettings::default(),
        details: DetailsSettings {
            log_commits: 10,
            diff_lines: 300,
            links_kept: 20,
            links_shown: 3,
        },
        panel: crate::details::DetailsLayout {
            environment: Some(crate::details::Surface {
                foot: Vec::new(),
                ..crate::details::Surface::environment()
            }),
            ..Default::default()
        },
    };
    assert_eq!(written_and_read(&s), s);
}

#[test]
fn how_shells_and_claude_code_start_is_read_from_tables_of_their_own() {
    let s = Settings::parse(
        "[shell]\ncommand = [\"/bin/bash\", \"-l\"]\n[claude]\nvia_shell = true\n[claude.env]\nFOO = \"1\"\n",
    )
    .unwrap();
    assert_eq!(s.shell.command, ["/bin/bash", "-l"]);
    let claude = s.profile_of(Agent::Claude);
    assert!(claude.launch.via_shell);
    assert_eq!(claude.launch.env.get("FOO").map(String::as_str), Some("1"));
    assert_eq!(
        Settings::parse("")
            .unwrap()
            .profile_of(Agent::Claude)
            .launch,
        Launch::default()
    );
    let s = Settings::parse("[neovim]\ncommand = [\"/opt/nvim/bin/nvim\"]\nvia_shell = true\n")
        .unwrap();
    assert_eq!(s.neovim.command, ["/opt/nvim/bin/nvim"]);
    assert!(s.neovim.via_shell);
    assert!(Settings::parse("[claude]\nvia_shel = true\n").is_err());
}

#[test]
fn a_command_keeps_no_empty_words() {
    let mut s = Settings {
        shell: ShellLaunch {
            command: vec!["  ".into(), " /bin/zsh ".into(), "".into()],
            ..ShellLaunch::default()
        },
        ..Settings::default()
    };
    s.profile_of_mut(Agent::Claude).launch.command = vec![" ".into()];
    let s = s.sane();
    assert_eq!(s.shell.command, ["/bin/zsh"]);
    assert!(s.profile_of(Agent::Claude).launch.command.is_empty());
}

#[test]
fn the_file_read_again_is_taken_in_only_when_it_reads_and_says_something_new() {
    let now = Settings::default();
    let next = reread(&now, "zoom = 1.25\n").unwrap();
    assert_eq!(next.zoom, 1.25);
    // What adroit wrote itself, or a file caught half written, changes nothing.
    assert_eq!(reread(&now, &toml::to_string_pretty(&now).unwrap()), None);
    assert_eq!(reread(&now, "zoom = "), None);
    assert_eq!(reread(&now, "colour = \"red\"\n"), None);
}

#[test]
fn the_picker_asks_ghq_and_lists_the_folders_the_settings_name() {
    let home = std::path::Path::new("/h");
    let s = Settings::parse("").unwrap();
    assert!(s.picker.ghq);
    assert!(s.picker.root_paths(home).is_empty());
    let s =
        Settings::parse("[picker]\nghq = false\nroots = [\"~/src\", \"/work\", \" \"]\n").unwrap();
    assert!(!s.picker.ghq);
    assert_eq!(
        s.picker.root_paths(home),
        [PathBuf::from("/h/src"), PathBuf::from("/work")]
    );
}

#[test]
fn option_counts_as_alt_and_the_cursor_holds_still_unless_the_settings_say_otherwise() {
    let s = Settings::parse("").unwrap();
    assert_eq!(s.console, ConsoleSettings::default());
    assert_eq!(s.console.option_as_alt, OptionAsAlt::Both);
    assert!(!s.console.cursor_blink);
    let s = Settings::parse("[console]\noption_as_alt = \"left\"\ncursor_blink = true\n").unwrap();
    assert_eq!(s.console.option_as_alt, OptionAsAlt::Left);
    assert!(s.console.cursor_blink);
    assert!(Settings::parse("[console]\noption_as_alt = \"up\"\n").is_err());
}

#[test]
fn what_a_terminal_host_took_with_it_is_offered_back_unless_the_settings_say_otherwise() {
    assert_eq!(
        Settings::parse("").unwrap().restore_sessions,
        RestoreSessions::Ask
    );
    for (written, read) in [
        ("always", RestoreSessions::Always),
        ("never", RestoreSessions::Never),
    ] {
        let settings = Settings::parse(&format!("restore_sessions = \"{written}\"\n")).unwrap();
        assert_eq!(settings.restore_sessions, read);
        assert_eq!(read.name(), written);
    }
    assert!(Settings::parse("restore_sessions = \"sometimes\"\n").is_err());
}

#[test]
fn a_session_starting_to_wait_is_told_of_only_when_the_settings_ask() {
    assert!(!Settings::parse("").unwrap().notify_waiting);
    assert!(
        Settings::parse("notify_waiting = true\n")
            .unwrap()
            .notify_waiting
    );
}

#[test]
fn the_details_panel_reads_as_much_as_the_settings_say_and_at_least_one() {
    let s = Settings::parse("").unwrap();
    assert_eq!(
        (
            s.details.log_commits,
            s.details.diff_lines,
            s.details.links_kept
        ),
        (50, 2000, 100)
    );
    assert_eq!(s.details.links_shown, 5);
    assert_eq!(
        Settings::parse("[details]\nlinks_shown = 0\n")
            .unwrap()
            .details
            .links_shown,
        1
    );
    let s =
        Settings::parse("[details]\nlog_commits = 0\ndiff_lines = 9\nlinks_kept = 5000\n").unwrap();
    assert_eq!(
        (
            s.details.log_commits,
            s.details.diff_lines,
            s.details.links_kept
        ),
        (1, 9, 1000)
    );
}

pub(crate) const PROFILES: &str = r#"profile = "claude-work"

[[profiles]]
id = "claude"
name = "Claude Personal"
agent = "claude"

[[profiles]]
id = "claude-work"
name = "Claude Work"
agent = "claude"
home = "/Users/me/.local/share/adroit/profiles/claude-work"

[profiles.launch]
via_shell = true

[profiles.claude]
model = "opus"

[[profiles]]
id = "codex-review"
name = "Codex Review"
agent = "codex"

[profiles.codex]
sandbox_mode = "read-only"
"#;

#[test]
fn profiles_are_read_as_settings_toml_names_them() {
    let s = Settings::parse(PROFILES).unwrap();
    let ids: Vec<&str> = s.profiles.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["claude", "claude-work", "codex-review"]);
    assert_eq!(s.profile.as_str(), "claude-work");
    let work = &s.profiles[1];
    assert_eq!(work.label(), "Claude Work");
    assert_eq!(
        work.home.as_deref(),
        Some(Path::new(
            "/Users/me/.local/share/adroit/profiles/claude-work"
        ))
    );
    assert!(work.launch.via_shell);
    assert_eq!(work.claude().unwrap().model, "opus");
    assert_eq!(s.profiles[0].home, None);
    assert_eq!(s.profiles[2].codex().unwrap().sandbox_mode, "read-only");
    assert_eq!(written_and_read(&s), s);
}

/// A file as versions that kept logins in connections wrote it: two
/// profiles share `work`, with one archived before them.
const CONNECTIONS: &str = r#"profile = "claude-work"

[[profiles]]
id = "claude"
agent = "claude"

[[profiles]]
id = "old"
agent = "claude"
connection = "work"
archived = true

[[profiles]]
id = "claude-work"
name = "Claude Work"
agent = "claude"
connection = "work"

[[profiles]]
id = "claude-review"
name = "Claude Review"
agent = "claude"
connection = "work"

[[profiles]]
id = "pi-local"
agent = "pi"
connection = "local"

[[profiles]]
id = "codex"
agent = "codex"
connection = "codex"

[[connections]]
id = "work"
name = "Work"
agent = "claude"
home = "/Users/me/.adroit/claude-work"

[[connections]]
id = "local"
agent = "pi"
home = "/Users/me/.adroit/pi-local"

[[connections]]
id = "unused"
agent = "codex"
home = "/Users/me/.adroit/unused"
archived = true
"#;

#[test]
fn a_file_written_with_connections_gives_each_profile_its_connection_s_home() {
    let s = Settings::parse(CONNECTIONS).unwrap();
    let homes: Vec<(&str, Option<&Path>)> = s
        .profiles
        .iter()
        .map(|p| (p.id.as_str(), p.home.as_deref()))
        .collect();
    assert_eq!(
        homes,
        [
            ("claude", None),
            (
                "claude-work",
                Some(Path::new("/Users/me/.adroit/claude-work"))
            ),
            ("claude-review", None),
            ("pi-local", Some(Path::new("/Users/me/.adroit/pi-local"))),
            ("codex", None),
        ]
    );
}

#[test]
fn of_profiles_sharing_a_connection_the_first_keeps_it_and_the_others_are_told_of() {
    let s = Settings::parse(CONNECTIONS).unwrap();
    assert_eq!(s.unshared, ["claude-review".parse::<ProfileId>().unwrap()]);
    assert_eq!(
        s.find_profile(&"claude-review".parse().unwrap())
            .unwrap()
            .home,
        None
    );
}

#[test]
fn a_file_written_with_connections_is_written_with_homes_and_no_connections() {
    let earlier = Settings::parse(CONNECTIONS).unwrap();
    let written = toml::to_string_pretty(&earlier).unwrap();
    let table: toml::Table = written.parse().unwrap();
    assert!(!table.contains_key("connections"), "{written}");
    assert!(!written.contains("connection ="), "{written}");
    let again = Settings::parse(&written).unwrap();
    assert_eq!(again.profiles, earlier.profiles);
    // Once written, nothing is left to tell of.
    assert!(again.unshared.is_empty());
}

#[test]
fn a_file_without_profiles_reads_as_the_profiles_adroit_comes_with() {
    let s = Settings::parse("").unwrap();
    assert_eq!(s, Settings::default());
    assert_eq!(s.profile, ProfileId::builtin(Agent::Claude));
    assert_eq!(s.profiles, Agent::ALL.map(AgentProfile::builtin));
    assert!(s.unshared.is_empty());
}

#[test]
fn a_file_written_before_profiles_reads_its_claude_tables_as_the_claude_profile() {
    let s = Settings::parse(
        r#"[claude]
command = ["/opt/claude", "--verbose"]
via_shell = true
[claude.env]
FOO = "1"
[sessions]
model = "opus"
permission_mode = "plan"
"#,
    )
    .unwrap();
    assert_eq!(s.profile.as_str(), "claude");
    let claude = s.profile_of(Agent::Claude);
    assert_eq!(claude.id.as_str(), "claude");
    assert_eq!(claude.home, None);
    assert_eq!(claude.launch.command, ["/opt/claude", "--verbose"]);
    assert!(claude.launch.via_shell);
    assert_eq!(claude.launch.env.get("FOO").map(String::as_str), Some("1"));
    let options = claude.claude().unwrap();
    assert_eq!(
        (options.model.as_str(), options.permission_mode.as_str()),
        ("opus", "plan")
    );
    assert_eq!(
        s.profile_of(Agent::Codex).into_owned(),
        AgentProfile::builtin(Agent::Codex)
    );
}

#[test]
fn a_file_written_before_profiles_is_written_with_profiles_and_reads_back_the_same() {
    let earlier = Settings::parse(
        "agent = \"codex\"\n[claude]\nvia_shell = true\n[codex_sessions]\nmodel = \"m\"\n",
    )
    .unwrap();
    let written = toml::to_string_pretty(&earlier).unwrap();
    let table: toml::Table = written.parse().unwrap();
    for key in ["agent", "claude", "sessions", "codex", "codex_sessions"] {
        assert!(!table.contains_key(key), "{key} written:\n{written}");
    }
    assert!(table.contains_key("profiles"), "{written}");
    let again = Settings::parse(&written).unwrap();
    assert_eq!(again, earlier);
    assert_eq!(again.profile, earlier.profile);
    // Written once more, the file stays as it is.
    assert_eq!(toml::to_string_pretty(&again).unwrap(), written);
}

#[test]
fn a_file_naming_profiles_beside_the_earlier_agent_tables_is_refused_naming_them() {
    let error = Settings::parse(&format!("{PROFILES}\n[sessions]\nmodel = \"opus\"\n"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("`profiles` cannot be named beside `sessions`"),
        "{error}"
    );
    let error =
        Settings::parse("profile = \"codex\"\nagent = \"codex\"\n[codex]\nvia_shell = true\n")
            .unwrap_err()
            .to_string();
    assert!(
        error.contains("`profile` cannot be named beside `agent`, `codex`"),
        "{error}"
    );
    let error = Settings::parse(
        "[claude]\nvia_shell = true\n[[connections]]\nid = \"work\"\nagent = \"claude\"\nhome = \"/w\"\n",
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("`connections` cannot be named beside `claude`"),
        "{error}"
    );
    // The profile alone picks among the profiles adroit comes with.
    let s = Settings::parse("profile = \"codex\"\n").unwrap();
    assert_eq!(s.default_agent(), Agent::Codex);
}

#[test]
fn settings_whose_profiles_or_connections_do_not_hold_together_are_refused() {
    let profile = |id: &str, agent: &str, rest: &str| {
        format!("[[profiles]]\nid = \"{id}\"\nagent = \"{agent}\"\n{rest}\n")
    };
    let connection = |id: &str, agent: &str, home: &str| {
        format!("[[connections]]\nid = \"{id}\"\nagent = \"{agent}\"\nhome = \"{home}\"\n")
    };
    let cases = [
        (
            profile("claude", "claude", "") + &profile("claude", "claude", ""),
            "profile `claude` is defined twice",
        ),
        (
            profile("Claude Work", "claude", ""),
            "`Claude Work` is not a valid id",
        ),
        (
            profile("claude", "claude", "connection = \"gone\""),
            "profile `claude` names connection `gone`, which is not defined",
        ),
        (
            profile("claude", "claude", "connection = \"work\"")
                + &connection("work", "codex", "/w"),
            "profile `claude` starts Claude Code but connection `work` belongs to Codex",
        ),
        (
            profile("claude", "claude", "home = \"/w\"")
                + &profile("codex", "codex", "home = \"/w\""),
            "profiles `claude` and `codex` both keep their login in /w",
        ),
        (
            profile("claude", "claude", "home = \"~/w\""),
            "profile `claude` needs an absolute path as its home",
        ),
        (
            profile("claude", "claude", "home = \"/h\"\nconnection = \"work\"")
                + &connection("work", "claude", "/w"),
            "profile `claude` names both `connection` and `home`",
        ),
        (
            profile("claude", "claude", "[profiles.codex]\nmodel = \"m\""),
            "profile `claude` starts Claude Code and cannot hold options for Codex",
        ),
        (
            profile("claude", "claude", "") + &connection("claude", "claude", "/w"),
            "connection id `claude` is reserved for Claude Code's own environment",
        ),
        (
            profile("claude", "claude", "") + &connection("work", "claude", "w"),
            "connection `work` needs an absolute path as its home",
        ),
        (
            profile("claude", "claude", "")
                + &connection("work", "claude", "/w")
                + &connection("work", "claude", "/v"),
            "connection `work` is defined twice",
        ),
        (
            format!("profile = \"other\"\n{}", profile("claude", "claude", "")),
            "profile `other` is not defined",
        ),
        (
            format!(
                "profile = \"old\"\n{}{}",
                profile("claude", "claude", ""),
                profile("old", "claude", "archived = true")
            ),
            "profile `old` is not defined",
        ),
        (
            profile("old", "claude", "archived = true"),
            "the settings hold no profile",
        ),
    ];
    for (text, why) in cases {
        let error = Settings::parse(&text).unwrap_err().to_string();
        assert!(error.contains(why), "{text}\n{error}\ndoes not say {why}");
    }
}

#[test]
fn with_no_profile_named_the_first_profile_is_the_default() {
    let s = Settings::parse(
        "[[profiles]]\nid = \"review\"\nagent = \"codex\"\n[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n",
    )
    .unwrap();
    assert_eq!(s.profile.as_str(), "review");
    assert_eq!(s.default_agent(), Agent::Codex);
}

#[test]
fn a_profile_written_archived_is_read_as_if_it_were_not_there_and_dropped_when_written() {
    let s = Settings::parse(
        "[[profiles]]\nid = \"old\"\nagent = \"claude\"\nhome = \"/o\"\narchived = true\n[[profiles]]\nid = \"review\"\nagent = \"codex\"\n[[profiles]]\nid = \"new\"\nagent = \"claude\"\nhome = \"/o\"\n",
    )
    .unwrap();
    let ids: Vec<&str> = s.profiles.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["review", "new"]);
    assert_eq!(s.profile.as_str(), "review");
    assert_eq!(s.find_profile(&"old".parse().unwrap()), None);
    let written = toml::to_string_pretty(&s).unwrap();
    assert!(!written.contains("old"), "{written}");
    assert!(!written.contains("archived"), "{written}");
}

#[test]
fn a_launch_kept_as_json_is_read_and_checked_as_the_file_is() {
    let s = Settings::parse(PROFILES).unwrap();
    let kept = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<Settings>(&kept).unwrap(), s);
    let mut broken: serde_json::Value = serde_json::from_str(&kept).unwrap();
    broken["profile"] = "gone".into();
    let error = serde_json::from_value::<Settings>(broken)
        .unwrap_err()
        .to_string();
    assert!(error.contains("profile `gone` is not defined"), "{error}");
}

#[test]
fn a_launch_kept_as_json_before_profiles_reads_as_the_profiles_built_from_it() {
    let kept = r#"{"agent":"codex","codex":{"command":["/opt/codex"],"via_shell":false,"env":{}},
        "codex_sessions":{"model":"m","reasoning_effort":"","approval_policy":"","sandbox_mode":""},
        "delegation":{"max_started":4},"zoom":1.0,"prompt_min_rows":3,"prompt_max_rows":12,
        "font_family":"","font_size":13.0,"font_weight":400,"appearance":"system",
        "light_theme":"Dawnfox","dark_theme":"Nightfox","keep_awake":true,"notify_waiting":false,
        "shell":{"command":[],"env":{}},"claude":{"command":[],"via_shell":true,"env":{}},
        "neovim":{"command":[],"via_shell":false,"env":{}},
        "sessions":{"model":"opus","permission_mode":"","disallowed_tools":[],"confirm_channel_warning":false},
        "scratch_dir":"","picker":{"ghq":true,"roots":[]},
        "console":{"option_as_alt":"both","cursor_blink":false},
        "details":{"log_commits":50,"diff_lines":2000,"links_kept":100,"links_shown":5},
        "usage":{"cpu":true,"gpu":true,"memory":true,"storage":true}}"#;
    let s: Settings = serde_json::from_str(kept).unwrap();
    assert_eq!(s.default_agent(), Agent::Codex);
    assert_eq!(s.profile_of(Agent::Codex).launch.command, ["/opt/codex"]);
    assert_eq!(s.profile_of(Agent::Codex).codex().unwrap().model, "m");
    let claude = s.profile_of(Agent::Claude);
    assert!(claude.launch.via_shell);
    assert!(!claude.claude().unwrap().confirm_channel_warning);
}

#[test]
fn a_file_that_cannot_be_read_says_which_setting_and_why() {
    let cases = [
        (
            "zoom = \"big\"\n",
            "invalid type: string \"big\", expected f64\nin `zoom`\n",
        ),
        (
            "[delegation]\nenabled = 3\n",
            "invalid type: integer `3`, expected a boolean\nin `delegation.enabled`\n",
        ),
        (
            "[picker]\nnonsense = 1\n",
            "unknown field `nonsense`, expected `ghq` or `roots`\nin `picker`\n",
        ),
        (
            "profile = 3\n",
            "invalid type: integer `3`, expected a string\nin `profile`\n",
        ),
        (
            "profiles = 3\n",
            "invalid type: integer `3`, expected a sequence\nin `profiles`\n",
        ),
        (
            "[[profiles]]\nid = \"a\"\nagent = \"claude\"\nnonsense = 1\n",
            "unknown field `nonsense`, expected one of `id`, `name`, `agent`, `home`, `connection`, `launch`, `claude`, `codex`, `pi`, `opencode`, `archived`\nin `profiles`\n",
        ),
        (
            "profile = \"gone\"\n[[profiles]]\nid = \"a\"\nagent = \"claude\"\n",
            "profile `gone` is not defined\n",
        ),
        (
            "profiles = []\n",
            "the settings hold no profile; add one, or leave `profiles` out for the ones adroit comes with\n",
        ),
        // What only files written before profiles hold is read as strictly.
        (
            "claude = 3\n",
            "invalid type: integer `3`, expected struct Launch\nin `claude`\n",
        ),
        (
            "agent = \"nope\"\n",
            "unknown variant `nope`, expected one of `claude`, `codex`, `pi`, `opencode`\nin `agent`\n",
        ),
        (
            "[sessions]\nmodel = 3\n",
            "invalid type: integer `3`, expected a string\nin `sessions.model`\n",
        ),
        (
            "[codex_sessions]\nnonsense = 1\n",
            "unknown field `nonsense`, expected one of `model`, `reasoning_effort`, `approval_policy`, `sandbox_mode`\nin `codex_sessions`\n",
        ),
        (
            "connections = 3\n",
            "invalid type: integer `3`, expected a sequence\nin `connections`\n",
        ),
        (
            "agent = \"codex\"\nprofile = \"codex\"\n",
            "`profile` cannot be named beside `agent`, which only settings written before profiles hold; move what they say into a profile\n",
        ),
    ];
    for (text, error) in cases {
        assert_eq!(
            Settings::parse(text).unwrap_err().to_string(),
            error,
            "{text}"
        );
    }
}

#[test]
fn a_setting_the_file_should_not_hold_is_named_with_those_it_may() {
    let error = |text: &str| Settings::parse(text).unwrap_err().to_string();
    assert_eq!(
        error("nonsense = 1\n"),
        "unknown field `nonsense`, expected one of `profile`, `profiles`, `delegation`, `outside`, `zoom`, `prompt_min_rows`, `prompt_max_rows`, `font_family`, `font_size`, `font_weight`, `appearance`, `light_theme`, `dark_theme`, `theme`, `keep_awake`, `notify_waiting`, `rename_sessions`, `restore_sessions`, `shell`, `neovim`, `scratch_dir`, `picker`, `console`, `details`, `usage`, `panel`\n"
    );
    assert_eq!(
        error("[delegation]\nnonsense = 1\n"),
        "unknown field `nonsense`, expected `enabled`\nin `delegation`\n"
    );
}

#[test]
fn every_option_as_alt_counts_both_alt_keys_off_macos() {
    for chosen in [
        OptionAsAlt::Both,
        OptionAsAlt::Left,
        OptionAsAlt::Right,
        OptionAsAlt::None,
    ] {
        let in_force = chosen.in_force();
        if cfg!(target_os = "macos") {
            assert_eq!(in_force, chosen);
        } else {
            assert_eq!(in_force, OptionAsAlt::Both, "{chosen:?}");
        }
    }
}
