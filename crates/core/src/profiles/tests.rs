use super::*;
use crate::settings::Launch;
use crate::settings::tests::{PROFILES, written_and_read};
use adroit_model::testing::profile;

#[test]
fn numbered_ids_run_from_the_base_then_count_from_two() {
    let ids: Vec<String> = numbered("claude").take(3).collect();
    assert_eq!(ids, ["claude", "claude-2", "claude-3"]);
}

fn settings() -> Settings {
    Settings {
        profile: "claude".parse().unwrap(),
        profiles: vec![
            AgentProfile {
                name: "Claude Personal".into(),
                ..profile("claude", Agent::Claude).build()
            },
            AgentProfile {
                name: "Claude Work".into(),
                ..profile("claude-work", Agent::Claude)
                    .home("/Users/me/work")
                    .build()
            },
            profile("codex", Agent::Codex).build(),
        ],
        ..Settings::default()
    }
}

fn id(text: &str) -> ProfileId {
    text.parse().unwrap()
}

/// Whether `s` would be read back as they are written, with every agent
/// still started from a profile that is found as it is offered.
fn holds(s: &Settings) {
    adroit_model::profile::check(&s.profile, &s.profiles).unwrap();
    for agent in Agent::ALL {
        let offered = s.profile_of(agent).into_owned();
        let found = s.find_profile(&offered.id).map(|p| p.into_owned());
        assert_eq!(found.as_ref(), Some(&offered), "{agent:?}");
        assert_eq!(offered.agent(), agent, "{offered:?}");
        assert!(startable(s).contains(&offered), "{agent:?}");
    }
    let offered = startable(s);
    for p in &offered {
        assert_eq!(
            s.find_profile(&p.id).map(|f| f.into_owned()).as_ref(),
            Some(p)
        );
    }
    let ids: std::collections::HashSet<_> = offered.iter().map(|p| &p.id).collect();
    assert_eq!(ids.len(), offered.len(), "{offered:?}");
}

fn none(_: &Path) -> Option<PathBuf> {
    None
}

#[test]
fn an_id_is_made_from_the_name_in_the_characters_an_id_may_hold() {
    let free = |_: &str| false;
    assert_eq!(fresh_id("Claude Work", "x", free), "claude-work");
    assert_eq!(fresh_id("  Review / CI!! ", "x", free), "review-ci");
    assert_eq!(fresh_id("仕事", "claude", free), "claude");
    assert_eq!(fresh_id("2nd", "x", free), "2nd");
    assert_eq!(fresh_id(&"a".repeat(80), "x", free).len(), 60);
    let ids = [
        fresh_id("Work", "x", free),
        fresh_id("x".repeat(80).as_str(), "x", free) + "-99",
    ];
    for made in ids {
        assert!(made.parse::<ProfileId>().is_ok(), "{made}");
    }
}

#[test]
fn an_id_already_taken_is_numbered_from_two() {
    let taken = |id: &str| ["work", "work-2"].contains(&id);
    assert_eq!(fresh_id("Work", "x", taken), "work-3");
}

#[test]
fn a_new_profile_takes_an_id_from_its_name_and_starts_in_its_agent_s_own_environment() {
    let mut s = settings();
    let added = add_profile(&mut s, "Claude Work", Agent::Claude).unwrap();
    assert_eq!(added, id("claude-work-2"));
    let p = s.profiles.last().unwrap();
    assert_eq!(p.name, "Claude Work");
    assert_eq!(p.home, None);
    assert_eq!(p.options, AgentOptions::of(Agent::Claude));
    holds(&s);
    // Another agent's key stands for the profile adroit comes with for it.
    let named_codex = add_profile(&mut s, "Codex", Agent::Claude).unwrap();
    assert_eq!(named_codex, id("codex-2"));
    assert_eq!(
        add_profile(&mut s, " ", Agent::Codex).unwrap_err(),
        "A profile needs a name"
    );
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn a_duplicate_keeps_its_profile_s_options_and_gets_a_login_of_its_own_when_the_profile_has_one() {
    let mut s = settings();
    profile_mut(&mut s, &id("claude-work"))
        .unwrap()
        .claude_mut()
        .unwrap()
        .model = "opus".into();
    let copy = duplicate_profile(&mut s, &id("claude-work"), Path::new("/d"), none).unwrap();
    assert_eq!(copy, id("claude-work-copy"));
    let made = &s.profiles[2];
    assert_eq!(made.id, copy);
    assert_eq!(made.name, "Claude Work copy");
    assert_eq!(
        made.home.as_deref(),
        Some(Path::new("/d/adroit/profiles/claude-work-copy"))
    );
    assert_eq!(made.claude().unwrap().model, "opus");
    holds(&s);
    // One in its agent's own environment is copied there.
    let copy = duplicate_profile(&mut s, &id("codex"), Path::new("/d"), none).unwrap();
    assert_eq!(s.find_profile(&copy).unwrap().home, None);
    holds(&s);
}

#[test]
fn renaming_changes_the_name_alone() {
    let mut s = settings();
    rename_profile(&mut s, &id("claude-work"), " Day job ").unwrap();
    let p = &s.profiles[1];
    assert_eq!(
        (p.id.as_str(), p.name.as_str(), p.home.as_deref()),
        ("claude-work", "Day job", Some(Path::new("/Users/me/work")))
    );
    assert!(rename_profile(&mut s, &id("claude"), "").is_err());
    assert_eq!(
        rename_profile(&mut s, &id("gone"), "x").unwrap_err(),
        "profile `gone` is not defined"
    );
}

#[test]
fn a_deleted_profile_is_given_back_with_its_login_and_the_default_one_cannot_be_deleted() {
    let mut s = settings();
    let deleted = delete_profile(&mut s, &id("claude-work")).unwrap();
    assert_eq!(deleted.home.as_deref(), Some(Path::new("/Users/me/work")));
    assert_eq!(s.find_profile(&id("claude-work")), None);
    holds(&s);
    let before = s.clone();
    assert_eq!(
        delete_profile(&mut s, &id("claude")).unwrap_err(),
        "Claude Personal is used for new sessions; use another profile for new sessions before deleting it"
    );
    assert_eq!(
        delete_profile(&mut s, &id("gone")).unwrap_err(),
        "profile `gone` is not defined"
    );
    assert_eq!(s, before);
    make_default(&mut s, &id("codex")).unwrap();
    delete_profile(&mut s, &id("claude")).unwrap();
    holds(&s);
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn deleting_offers_to_delete_only_a_folder_adroit_made() {
    let data = Path::new("/d");
    let with = |home: Option<&str>| {
        let mut p = profile("p", Agent::Claude).build();
        p.home = home.map(PathBuf::from);
        login_folder(&p, data, none)
    };
    assert_eq!(with(None), LoginFolder::Terminal);
    assert_eq!(
        with(Some("/d/adroit/profiles/p")),
        LoginFolder::Deletable("/d/adroit/profiles/p".into())
    );
    assert_eq!(
        with(Some("/Users/me/.claude")),
        LoginFolder::Kept("/Users/me/.claude".into())
    );
}

#[test]
fn once_deleted_a_profile_s_folder_kept_is_shown_by_its_path() {
    let shown = |p: &Path| p.display().to_string();
    let told =
        |folder: &LoginFolder, with_folder| deleted_notice("Work", folder, with_folder, shown);
    let made = LoginFolder::Deletable("/d/adroit/profiles/work".into());
    assert_eq!(told(&LoginFolder::Terminal, false), "Deleted Work.");
    assert_eq!(
        told(&made, true),
        "Deleted Work and its login folder /d/adroit/profiles/work."
    );
    assert_eq!(
        told(&made, false),
        "Deleted Work. Its login folder is kept at /d/adroit/profiles/work."
    );
    assert_eq!(
        told(&LoginFolder::Kept("/w".into()), true),
        "Deleted Work. Its login folder is kept at /w."
    );
}

#[test]
fn the_id_of_a_deleted_profile_can_be_taken_again() {
    let mut s = settings();
    delete_profile(&mut s, &id("claude-work")).unwrap();
    assert_eq!(
        add_profile(&mut s, "Claude Work", Agent::Claude).unwrap(),
        id("claude-work")
    );
    holds(&s);
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn only_a_folder_adroit_made_under_its_data_folder_is_deleted_with_its_profile() {
    let data = Path::new("/Users/me/.local/share");
    let made = |home: &str| made_by_adroit(Path::new(home), data, none);
    assert!(made("/Users/me/.local/share/adroit/profiles/work"));
    assert!(made("/Users/me/.local/share/adroit/connections/work"));
    assert!(!made("/Users/me/.local/share/adroit"));
    assert!(!made(
        "/Users/me/.local/share/adroit/profiles/../../../.claude"
    ));
    assert!(!made("/Users/me/.claude"));
    assert!(!made("/Users/me/.local/share/adroit-other/work"));
    // Through the links of the folders that exist, as homes are written.
    let canonical = |p: &Path| (p == Path::new("/tmp")).then(|| PathBuf::from("/private/tmp"));
    assert!(made_by_adroit(
        Path::new("/private/tmp/d/adroit/profiles/w"),
        Path::new("/tmp/d"),
        canonical
    ));
}

#[test]
fn changing_a_profile_s_agent_resets_what_it_held_for_the_agent_before_and_says_so() {
    let mut s = settings();
    {
        let work = profile_mut(&mut s, &id("claude-work")).unwrap();
        work.claude_mut().unwrap().model = "opus".into();
        work.launch.command = vec!["/opt/claude".into()];
        work.launch.via_shell = true;
    }
    let told = change_agent(&mut s, &id("claude-work"), Agent::Codex)
        .unwrap()
        .unwrap();
    assert_eq!(
        told,
        "Claude Work now starts Codex; its model, options, login and command were reset."
    );
    let p = &s.profiles[1];
    assert_eq!(p.options, AgentOptions::of(Agent::Codex));
    assert_eq!(p.home, None);
    assert!(p.launch.command.is_empty());
    assert!(p.launch.via_shell);
    assert_eq!(p.id, id("claude-work"));
    holds(&s);
    assert_eq!(
        change_agent(&mut s, &id("claude-work"), Agent::Codex),
        Ok(None)
    );
}

#[test]
fn an_agent_left_without_a_profile_of_its_own_still_starts_from_adroit_s() {
    // The only Codex profile, under Codex's key, deleted.
    let mut s = settings();
    delete_profile(&mut s, &id("codex")).unwrap();
    holds(&s);
    assert_eq!(s.profile_of(Agent::Codex).id, id("codex"));
    // The only Claude Code profile left, under its key, moved to Codex.
    let mut s = settings();
    delete_profile(&mut s, &id("claude-work")).unwrap();
    change_agent(&mut s, &id("claude"), Agent::Codex).unwrap();
    holds(&s);
    assert_eq!(s.profile_of(Agent::Claude).id, id("claude-2"));
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn a_login_of_its_own_keeps_its_state_under_adroit_s_data_folder_by_the_profile_s_id() {
    let mut s = settings();
    let data = Path::new("/Users/me/.local/share");
    set_login(&mut s, &id("codex"), Login::Own, data, none).unwrap();
    assert_eq!(
        s.profiles[2].home.as_deref(),
        Some(Path::new("/Users/me/.local/share/adroit/profiles/codex"))
    );
    holds(&s);
    // One it has already is kept as it is written.
    set_login(&mut s, &id("claude-work"), Login::Own, data, none).unwrap();
    assert_eq!(
        s.profiles[1].home.as_deref(),
        Some(Path::new("/Users/me/work"))
    );
    assert_eq!(
        set_login(&mut s, &id("gone"), Login::Own, data, none).unwrap_err(),
        "profile `gone` is not defined"
    );
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn a_login_of_its_own_is_written_once_through_the_links_of_the_folders_that_exist() {
    let mut s = settings();
    // `/tmp` is where `/private/tmp` is linked from.
    let canonical = |p: &Path| (p == Path::new("/tmp")).then(|| PathBuf::from("/private/tmp"));
    set_login(
        &mut s,
        &id("codex"),
        Login::Own,
        Path::new("/tmp/./d/"),
        canonical,
    )
    .unwrap();
    assert_eq!(
        s.profiles[2].home.as_deref(),
        Some(Path::new("/private/tmp/d/adroit/profiles/codex"))
    );
    assert_eq!(
        set_login(
            &mut s,
            &id("claude"),
            Login::Own,
            Path::new("relative"),
            none
        )
        .unwrap_err(),
        "relative/adroit/profiles/claude is not an absolute path"
    );
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn a_new_login_is_numbered_past_a_folder_that_exists_or_another_profile_has() {
    let mut s = settings();
    s.profiles[1].home = Some("/d/adroit/profiles/codex".into());
    // `codex-2` is there already, as a deleted profile left it.
    let left = |p: &Path| (p == Path::new("/d/adroit/profiles/codex-2")).then(|| p.into());
    set_login(&mut s, &id("codex"), Login::Own, Path::new("/d"), left).unwrap();
    assert_eq!(
        s.profiles[2].home.as_deref(),
        Some(Path::new("/d/adroit/profiles/codex-3"))
    );
    holds(&s);
    // So is a duplicate's.
    let copy = duplicate_profile(&mut s, &id("claude-work"), Path::new("/d"), |p: &Path| {
        (p == Path::new("/d/adroit/profiles/claude-work-copy")).then(|| p.into())
    })
    .unwrap();
    assert_eq!(
        s.find_profile(&copy).unwrap().home.as_deref(),
        Some(Path::new("/d/adroit/profiles/claude-work-copy-2"))
    );
}

#[test]
#[cfg_attr(windows, ignore = "written with Unix paths")]
fn back_in_its_terminal_s_login_a_profile_forgets_its_folder_and_takes_it_again_only_when_given_it()
{
    let mut s = settings();
    let data = Path::new("/d");
    set_login(&mut s, &id("codex"), Login::Own, data, none).unwrap();
    let own = s.profiles[2].home.clone().unwrap();
    set_login(&mut s, &id("codex"), Login::Terminal, data, none).unwrap();
    assert_eq!(s.profiles[2].home, None);
    holds(&s);
    // Its folder is there now, so a new login is not it.
    let there = |p: &Path| (p == own).then(|| p.to_path_buf());
    set_login(&mut s, &id("codex"), Login::Own, data, there).unwrap();
    assert_eq!(
        s.profiles[2].home.as_deref(),
        Some(Path::new("/d/adroit/profiles/codex-2"))
    );
    set_login(&mut s, &id("codex"), Login::Again(own.clone()), data, none).unwrap();
    assert_eq!(s.profiles[2].home.as_ref(), Some(&own));
    // Unless another profile has it since.
    set_login(&mut s, &id("codex"), Login::Terminal, data, none).unwrap();
    s.profiles[1].home = Some(own.clone());
    assert_eq!(
        set_login(&mut s, &id("codex"), Login::Again(own), data, none).unwrap_err(),
        "Claude Work keeps its login in /d/adroit/profiles/codex since; each profile needs a folder of its own"
    );
}

#[test]
fn a_draft_offers_a_profile_by_its_agent_and_name_and_says_when_it_has_a_login_of_its_own() {
    let mut s = settings();
    s.profiles.push(AgentProfile {
        name: "Codex".into(),
        ..profile("codex-2", Agent::Codex).build()
    });
    let offered: Vec<String> = startable(&s).iter().map(offered_as).collect();
    assert_eq!(
        offered,
        [
            "Claude Code · Claude Personal",
            "Claude Code · Claude Work · own login",
            "Codex · codex",
            "Codex",
            "Pi",
            "OpenCode"
        ]
    );
}

#[test]
fn a_kept_conversation_is_listed_with_its_profile_and_a_login_of_its_own() {
    let s = settings();
    let past = |profile: &str, home: Option<&str>| adroit_model::Past {
        agent: Agent::Claude,
        home: home.map(PathBuf::from),
        profile: id(profile),
        profile_name: "Listed".into(),
        id: "c".into(),
        cwd: "/r".into(),
        title: "t".into(),
        at_ms: 0,
    };
    assert_eq!(
        resumed_as(&s, &past("claude-work", Some("/Users/me/work"))),
        "Claude Code · Claude Work · own login"
    );
    // Its profile deleted since it was listed.
    assert_eq!(
        resumed_as(&s, &past("gone", Some("/Users/me/gone"))),
        "Claude Code · Listed · own login"
    );
    assert_eq!(
        resumed_as(&s, &past("claude", None)),
        "Claude Code · Claude Personal"
    );
}

#[test]
fn profiles_a_file_had_share_a_login_are_told_of_by_name_until_it_is_written_again() {
    let mut s = settings();
    assert_eq!(unshared_notice(&s), None);
    s.unshared = vec![id("codex")];
    assert_eq!(
        unshared_notice(&s).unwrap(),
        "codex shared a login with another profile, which keeps it; it now runs in your terminal's login. Give it a login of its own under Settings → Agents."
    );
    s.unshared = vec![id("codex"), id("claude")];
    assert_eq!(
        unshared_notice(&s).unwrap(),
        "codex, Claude Personal shared a login with another profile, which keeps it; they now run in your terminal's login. Give each a login of its own under Settings → Agents."
    );
}

#[test]
fn a_session_starts_from_a_profile_or_adroit_s_own_for_an_agent_with_none() {
    let mut s = settings();
    delete_profile(&mut s, &id("claude-work")).unwrap();
    let ids =
        |s: &Settings| -> Vec<String> { startable(s).iter().map(|p| p.id.to_string()).collect() };
    assert_eq!(ids(&s), ["claude", "codex", "pi", "opencode"]);
    s.profiles.retain(|p| p.agent() == Agent::Claude);
    assert_eq!(ids(&s), ["claude", "codex", "pi", "opencode"]);
    assert_eq!(startable(&s)[1], AgentProfile::builtin(Agent::Codex));
}

#[test]
fn an_agent_starts_from_the_default_profile_when_that_is_its_own_else_from_its_first() {
    let s = Settings::parse(PROFILES).unwrap();
    assert_eq!(s.profile_of(Agent::Claude).id.as_str(), "claude-work");
    assert_eq!(s.profile_of(Agent::Codex).id.as_str(), "codex-review");
    let s = Settings {
        profile: "codex-review".parse().unwrap(),
        ..s
    };
    assert_eq!(s.profile_of(Agent::Claude).id.as_str(), "claude");
    let mut s = s;
    s.profiles.remove(0);
    assert_eq!(s.profile_of(Agent::Claude).id.as_str(), "claude-work");
}

#[test]
fn a_profile_is_found_by_its_id_and_adroit_s_own_stands_for_an_agent_given_none() {
    let s = Settings::parse(
        "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"claude-work\"\nagent = \"claude\"\n",
    )
    .unwrap();
    let found = |s: &Settings, id: &str| s.find_profile(&id.parse().unwrap()).map(Cow::into_owned);
    assert_eq!(found(&s, "claude-work").unwrap().id.as_str(), "claude-work");
    assert_eq!(
        found(&s, "codex"),
        Some(AgentProfile::builtin(Agent::Codex))
    );
    assert_eq!(
        found(&s, "codex"),
        Some(s.profile_of(Agent::Codex).into_owned())
    );
    assert_eq!(found(&s, "gone"), None);
    // Once the agent has a profile of its own, adroit's stands for none.
    let s = Settings::parse(
        "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"codex-review\"\nagent = \"codex\"\n",
    )
    .unwrap();
    assert_eq!(found(&s, "codex"), None);
}

#[test]
fn adroit_s_own_profile_takes_an_id_no_profile_holds_so_it_is_found_as_offered() {
    let found = |s: &Settings, id: &ProfileId| s.find_profile(id).map(Cow::into_owned).unwrap();
    // The agent's key held by a profile of another agent.
    let s = Settings::parse(
        "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"codex\"\nagent = \"claude\"\n",
    )
    .unwrap();
    let offered = s.profile_of(Agent::Codex).into_owned();
    assert_eq!(offered.id.as_str(), "codex-2");
    assert_eq!(found(&s, &offered.id), offered);
    assert_eq!(
        found(&s, &"codex".parse().unwrap()).id.as_str(),
        "codex",
        "the profile that holds the key is still found by it"
    );
}

#[test]
fn a_session_is_shown_by_its_agent_and_a_profile_of_another_name_by_that_too() {
    let s = Settings::parse(PROFILES).unwrap();
    let shown = |agent, profile: &str, home: Option<&str>| {
        s.started_as(agent, &profile.parse().unwrap(), home.map(Path::new), "")
    };
    assert_eq!(
        shown(
            Agent::Claude,
            "claude-work",
            Some("/Users/me/.local/share/adroit/profiles/claude-work")
        ),
        "Claude Code · Claude Work"
    );
    assert_eq!(
        shown(Agent::Claude, "claude", None),
        "Claude Code · Claude Personal"
    );
    assert_eq!(shown(Agent::Codex, "gone", None), "Codex · gone");
    let own = Settings::default();
    assert_eq!(
        own.started_as(Agent::Codex, &"codex".parse().unwrap(), None, ""),
        "Codex"
    );
    // One started before profiles, from a profile since removed.
    let none = Settings::parse("[[profiles]]\nid = \"mine\"\nagent = \"claude\"\n").unwrap();
    assert_eq!(
        none.started_as(Agent::Codex, &"codex".parse().unwrap(), None, ""),
        "Codex"
    );
    // Adroit's own, numbered past a profile of another agent that holds
    // the key.
    let keyed = Settings::parse(
        "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"codex\"\nname = \"Old\"\nagent = \"claude\"\n",
    )
    .unwrap();
    assert_eq!(
        keyed.started_as(Agent::Codex, &"codex-2".parse().unwrap(), None, ""),
        "Codex"
    );
}

#[test]
fn a_session_of_a_profile_deleted_since_is_named_as_its_profile_was_called_when_it_started() {
    let s = Settings::parse(PROFILES).unwrap();
    let work = "claude-work".parse().unwrap();
    let shown = |s: &Settings, home: Option<&str>| {
        s.started_as(Agent::Claude, &work, home.map(Path::new), "Old Work")
    };
    let home = Some("/Users/me/.local/share/adroit/profiles/claude-work");
    // Still held, it goes by the name it has now.
    assert_eq!(shown(&s, home), "Claude Code · Claude Work");
    let mut deleted = s.clone();
    deleted.profiles.retain(|p| p.id != work);
    assert_eq!(shown(&deleted, home), "Claude Code · Old Work");
    // Its id taken again by a profile of another login is not it.
    assert_eq!(
        shown(&s, Some("/Users/me/old-work")),
        "Claude Code · Old Work"
    );
    // Not known, it goes by its id.
    assert_eq!(
        deleted.started_as(Agent::Claude, &work, home.map(Path::new), ""),
        "Claude Code · claude-work"
    );
}

#[test]
fn an_agent_without_a_profile_starts_as_adroit_s_own_and_is_given_one_when_edited() {
    let mut s = Settings::parse("[[profiles]]\nid = \"codex\"\nagent = \"claude\"\n").unwrap();
    assert_eq!(
        s.profile_of(Agent::Codex).into_owned().launch,
        Launch::default()
    );
    assert_eq!(s.profile_of(Agent::Codex).agent(), Agent::Codex);
    s.profile_of_mut(Agent::Codex).launch.via_shell = true;
    let added = s.profiles.last().unwrap();
    // The agent's key is taken, so the profile adroit adds is numbered.
    assert_eq!(
        (added.id.as_str(), added.agent()),
        ("codex-2", Agent::Codex)
    );
    assert!(
        written_and_read(&s)
            .profile_of(Agent::Codex)
            .launch
            .via_shell
    );
}
