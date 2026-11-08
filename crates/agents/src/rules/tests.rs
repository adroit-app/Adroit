use super::*;
use crate::testing::screen;
use adroit_model::{Agent, StartupReadiness, State, Wait};

fn bundled(agent: Agent) -> Rules {
    Rules::bundled(agent).unwrap_or_else(|| panic!("no rules bundled for {agent:?}"))
}

#[test]
fn codex_s_rules_name_the_rule_that_decided() {
    let rules = bundled(Agent::Codex);
    for (text, rule, default) in [
        ("", "codex.no_composer", true),
        (
            "enter/esc continue · ⌃c quit",
            "codex.dialog_without_composer",
            false,
        ),
        (
            "Your answer:\n› \nenter to submit answer",
            "codex.question_below_composer",
            false,
        ),
        (
            "› 1. Yes\nPress enter to confirm or esc to cancel",
            "codex.permission_below_composer",
            false,
        ),
        ("› 1. One\n  2. Two", "codex.dialog_below_composer", false),
        (
            "• Working (2s • esc to interrupt)\n› ",
            "codex.interrupt_hint",
            false,
        ),
        ("• Worked for 2s\n› ", "codex.worked_for", false),
        (
            "• Worked for 2s\n› old\n• Thinking\n› ",
            "codex.composer_idle",
            true,
        ),
        ("› \n? for shortcuts", "codex.composer_idle", true),
    ] {
        let reading = rules.judge(&screen(text));
        assert_eq!(reading.matched.rule, rule, "{text:?}");
        assert_eq!(reading.matched.default, default, "{text:?}");
    }
}

#[test]
fn a_rule_tells_the_row_whose_sign_it_matched() {
    let reading = bundled(Agent::Codex).judge(&screen(
        "Some reply\n• Working (2s • esc to interrupt)\n\n› \n100% context left",
    ));
    assert_eq!(reading.state, Some(State::Working));
    assert_eq!(
        reading.matched.evidence,
        vec![(1, "• Working (2s • esc to interrupt)".to_string())]
    );
}

#[test]
fn a_default_taken_by_nothing_matching_tells_the_row_it_went_by() {
    let reading = bundled(Agent::Codex).judge(&screen("› \n? for shortcuts"));
    assert_eq!(reading.matched.evidence, vec![(0, "› ".to_string())]);
    assert!(
        bundled(Agent::Codex)
            .judge(&screen(""))
            .matched
            .evidence
            .is_empty()
    );
}

#[test]
fn startup_follows_the_state_unless_the_rule_says_otherwise() {
    let rules = Rules::parse(
        "pi",
        r#"
agent = "pi"
[lines]
trust = '(?i)^trust project folder\?$'
[[rules]]
id = "trust_asked"
state = "waiting-input"
startup = "awaiting-trust"
when = { has = "trust" }
[[rules]]
id = "idle"
state = "idle"
default = true
"#,
    )
    .unwrap();
    let trust = rules.judge_startup(&screen("Trust project folder?"));
    assert_eq!(trust.readiness, Some(StartupReadiness::AwaitingTrust));
    assert_eq!(trust.matched.rule, "pi.trust_asked");
    assert_eq!(
        rules.judge(&screen("Trust project folder?")).state,
        Some(State::Waiting(Wait::Input))
    );
    assert_eq!(
        rules.judge_startup(&screen("anything")).readiness,
        Some(StartupReadiness::Ready)
    );
}

#[test]
fn startup_rules_of_their_own_judge_startup_apart_from_the_state() {
    let rules = Rules::parse(
        "claude",
        r#"
agent = "claude"
[lines]
trust = 'trust'
[[rules]]
id = "idle"
state = "idle"
[[startup]]
id = "trust_asked"
startup = "awaiting-trust"
when = { has = "trust" }
[[startup]]
id = "nothing_yet"
default = true
"#,
    )
    .unwrap();
    assert_eq!(
        rules.judge_startup(&screen("trust")).readiness,
        Some(StartupReadiness::AwaitingTrust)
    );
    let nothing = rules.judge_startup(&screen("other"));
    assert_eq!(nothing.readiness, None);
    assert_eq!(nothing.matched.rule, "claude.nothing_yet");
}

#[test]
fn a_region_is_found_from_the_regions_it_is_built_on() {
    let rules = Rules::parse(
        "pi",
        r#"
agent = "pi"
[lines]
rule = '^──'
working = 'Working'
[regions]
lower = { last = "rule" }
above_lower = { before = "lower" }
upper = { within = "above_lower", last = "rule" }
editor = { within = "above_lower", after = "upper" }
[[rules]]
id = "no_editor"
default = true
when = { absent = "editor" }
[[rules]]
id = "working_rule"
state = "working"
when = { in = "upper", has = "working" }
[[rules]]
id = "editor_idle"
state = "idle"
default = true
"#,
    )
    .unwrap();
    assert_eq!(
        rules.judge(&screen("── Working ──\n> \n────")).state,
        Some(State::Working)
    );
    assert_eq!(
        rules.judge(&screen("Working\n────\n> \n────")).state,
        Some(State::Idle)
    );
    assert_eq!(
        rules.judge(&screen("── Working ──\n> ")).matched.rule,
        "pi.no_editor"
    );
}

#[test]
fn a_line_may_ask_what_the_row_after_it_shows() {
    let rules = Rules::parse(
        "claude",
        r#"
agent = "claude"
[lines]
prompt = '^❯'
top = { regex = '^──', next = "prompt" }
[regions]
top = { last = "top" }
[[rules]]
id = "input"
state = "idle"
when = { present = "top" }
[[rules]]
id = "none"
default = true
"#,
    )
    .unwrap();
    assert_eq!(
        rules.judge(&screen("────\n❯ \n────")).matched.rule,
        "claude.input"
    );
    assert_eq!(
        rules.judge(&screen("────\ntext\n────")).matched.rule,
        "claude.none"
    );
    assert_eq!(rules.judge(&screen("────")).matched.rule, "claude.none");
}

#[test]
fn the_last_rows_counted_are_the_last_rows_of_their_region() {
    let rules = Rules::parse(
        "codex",
        r#"
agent = "codex"
[lines]
x = 'x'
[regions]
last_two = { tail = 2 }
last_two_drawn = { tail_non_empty = 2 }
[[rules]]
id = "near"
state = "working"
when = { in = "last_two", has = "x" }
[[rules]]
id = "drawn"
state = "idle"
when = { in = "last_two_drawn", has = "x" }
[[rules]]
id = "none"
default = true
"#,
    )
    .unwrap();
    assert_eq!(rules.judge(&screen("a\nx\n")).matched.rule, "codex.near");
    assert_eq!(
        rules.judge(&screen("x\na\n\n\n")).matched.rule,
        "codex.drawn"
    );
    assert_eq!(
        rules.judge(&screen("x\na\nb\n\n")).matched.rule,
        "codex.none"
    );
}

#[test]
fn a_gate_holds_when_every_condition_it_names_holds() {
    let rules = Rules::parse(
        "codex",
        r#"
agent = "codex"
[lines]
a = 'a'
b = 'b'
c = 'c'
[[rules]]
id = "all"
state = "working"
when = { has = "a", all = [{ has = "b" }], not = { has = "c" } }
[[rules]]
id = "any"
state = "idle"
when = { any = [{ has = "b" }, { has = "c" }] }
[[rules]]
id = "none"
default = true
"#,
    )
    .unwrap();
    assert_eq!(rules.judge(&screen("a\nb")).matched.rule, "codex.all");
    assert_eq!(rules.judge(&screen("a\nb\nc")).matched.rule, "codex.any");
    assert_eq!(rules.judge(&screen("c")).matched.rule, "codex.any");
    assert_eq!(rules.judge(&screen("a")).matched.rule, "codex.none");
}

/// A valid set of rules with `extra` written after its lines, to break.
fn with(extra: &str, rules: &str) -> String {
    format!("agent = \"codex\"\n[lines]\nx = 'x'\n{extra}\n{rules}")
}

const CATCH_ALL: &str = "[[rules]]\nid = \"none\"\ndefault = true\n";

#[test]
fn rules_that_cannot_be_read_are_refused_saying_why() {
    for (text, why) in [
        ("not toml [".to_string(), "TOML"),
        (
            "agent = \"pi\"\n[[rules]]\nid = \"a\"\n".to_string(),
            "written for pi, not codex",
        ),
        (with("", ""), "no rules"),
        (
            with("", "[[rules]]\nid = \"a\"\nwhen = { has = \"x\" }\n"),
            "last rule",
        ),
        (
            with(
                "",
                &format!("[[rules]]\nid = \"a\"\nwhen = {{ has = \"y\" }}\n{CATCH_ALL}"),
            ),
            "no line y",
        ),
        (
            with(
                "",
                &format!("[[rules]]\nid = \"a\"\nwhen = {{ present = \"r\" }}\n{CATCH_ALL}"),
            ),
            "no region r",
        ),
        (with("bad = '('", CATCH_ALL), "line bad"),
        (
            with(
                "[regions]\na = { after = \"b\" }\nb = { before = \"a\" }",
                CATCH_ALL,
            ),
            "refers to itself",
        ),
        (
            with("[regions]\na = { last = \"x\", tail = 2 }", CATCH_ALL),
            "region a",
        ),
        (
            with(
                "",
                &format!("[[rules]]\nid = \"a\"\nwhen = {{ in = \"r\" }}\n{CATCH_ALL}"),
            ),
            "in without has",
        ),
        (
            with(
                "",
                &format!("[[rules]]\nid = \"a\"\nwhen = {{}}\n{CATCH_ALL}"),
            ),
            "names no condition",
        ),
        (
            with(
                "",
                &format!("[[rules]]\nid = \"a\"\nstate = \"busy\"\n{CATCH_ALL}"),
            ),
            "TOML",
        ),
        (
            with(
                "",
                &format!("{CATCH_ALL}[[startup]]\nid = \"a\"\nstate = \"idle\"\n"),
            ),
            "startup rule a",
        ),
        (with("", &format!("{CATCH_ALL}{CATCH_ALL}")), "twice"),
    ] {
        let error = Rules::parse("codex", &text).expect_err(&text).to_string();
        assert!(error.contains(why), "{text:?}: {error}");
    }
}

#[test]
fn rules_too_large_to_judge_every_read_are_refused() {
    let many = (0..=MAX_RULES)
        .map(|i| format!("[[rules]]\nid = \"r{i}\"\nwhen = {{ has = \"x\" }}\n"))
        .collect::<String>();
    let error = Rules::parse("codex", &with("", &format!("{many}{CATCH_ALL}"))).unwrap_err();
    assert!(error.to_string().contains("rules"), "{error}");

    let long = format!("long = '{}'", "a".repeat(MAX_PATTERN + 1));
    let error = Rules::parse("codex", &with(&long, CATCH_ALL)).unwrap_err();
    assert!(error.to_string().contains("long"), "{error}");

    let mut deep = "{ has = \"x\" }".to_string();
    for _ in 0..=MAX_DEPTH {
        deep = format!("{{ not = {deep} }}");
    }
    let error = Rules::parse(
        "codex",
        &with(
            "",
            &format!("[[rules]]\nid = \"a\"\nwhen = {deep}\n{CATCH_ALL}"),
        ),
    )
    .unwrap_err();
    assert!(error.to_string().contains("deep"), "{error}");
}

#[test]
fn every_agent_s_bundled_rules_can_be_read() {
    for agent in Agent::ALL {
        if let Some(text) = BUNDLED.iter().find(|(a, _)| *a == agent).map(|(_, t)| t) {
            Rules::parse(agent.key(), text).unwrap_or_else(|e| panic!("{agent:?}: {e}"));
        }
    }
}

#[test]
fn a_named_condition_holds_wherever_a_rule_names_it() {
    let rules = Rules::parse(
        "codex",
        r#"
agent = "codex"
[lines]
x = 'x'
[conditions]
seen = { has = "x" }
[[rules]]
id = "seen"
state = "working"
when = { holds = "seen" }
[[rules]]
id = "unseen"
default = true
"#,
    )
    .unwrap();
    assert_eq!(rules.judge(&screen("x")).matched.rule, "codex.seen");
    assert_eq!(rules.judge(&screen("y")).matched.rule, "codex.unseen");
    let error = Rules::parse(
        "codex",
        &with(
            "[conditions]\na = { has = \"x\" }\nb = { holds = \"a\" }",
            CATCH_ALL,
        ),
    )
    .unwrap_err();
    assert!(error.to_string().contains("condition b"), "{error}");
}

const USER_CODEX: &str = r#"
agent = "codex"
[lines]
busy = 'busy'
[[rules]]
id = "busy"
state = "working"
when = { has = "busy" }
[[rules]]
id = "calm"
state = "idle"
default = true
"#;

#[test]
fn the_bundled_rules_are_in_force_while_the_user_writes_none() {
    let dir = tempfile::tempdir().unwrap();
    let in_force = InForce::load(Agent::Codex, &dir.path().join("codex.toml"));
    assert_eq!(in_force.from, None);
    assert_eq!(in_force.warning, None);
    assert_eq!(
        in_force
            .rules
            .judge(&screen("› \n? for shortcuts"))
            .matched
            .rule,
        "codex.composer_idle"
    );
}

#[test]
fn the_user_s_rules_take_the_place_of_the_whole_bundled_set() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("codex.toml");
    std::fs::write(&path, USER_CODEX).unwrap();
    let in_force = InForce::load(Agent::Codex, &path);
    assert_eq!(in_force.from.as_deref(), Some(path.as_path()));
    assert_eq!(in_force.warning, None);
    assert_eq!(
        in_force.rules.judge(&screen("busy")).state,
        Some(State::Working)
    );
    let composer = in_force.rules.judge(&screen("› \n? for shortcuts"));
    assert_eq!(composer.matched.rule, "codex.calm");
}

#[test]
fn rules_of_the_user_s_that_cannot_be_used_give_way_to_the_bundled_ones_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("codex.toml");
    for (text, why) in [
        ("not [ toml", "TOML"),
        (
            USER_CODEX.replace("\"codex\"", "\"pi\"").as_str(),
            "written for pi",
        ),
    ] {
        std::fs::write(&path, text).unwrap();
        let in_force = InForce::load(Agent::Codex, &path);
        assert_eq!(in_force.from, None);
        let warning = in_force.warning.expect("a warning");
        assert!(warning.contains(&path.display().to_string()), "{warning}");
        assert!(warning.contains(why), "{warning}");
        assert_eq!(
            in_force
                .rules
                .judge(&screen("› \n? for shortcuts"))
                .matched
                .rule,
            "codex.composer_idle"
        );
    }
}

#[test]
fn the_rules_in_force_are_read_again_once_the_user_s_file_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("codex.toml");
    let kept = Kept::default();
    assert_eq!(kept.get(Agent::Codex, &path).from, None);

    std::fs::write(&path, USER_CODEX).unwrap();
    assert_eq!(
        kept.get(Agent::Codex, &path).from.as_deref(),
        Some(path.as_path())
    );
    let first = kept.get(Agent::Codex, &path);
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &kept.get(Agent::Codex, &path)
    ));

    std::fs::write(&path, "broken [").unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert!(kept.get(Agent::Codex, &path).warning.is_some());

    std::fs::remove_file(&path).unwrap();
    let gone = kept.get(Agent::Codex, &path);
    assert_eq!((gone.from.clone(), gone.warning.clone()), (None, None));
}

#[test]
fn the_user_keeps_an_agent_s_rules_in_a_file_named_for_it_in_adroit_s_configuration() {
    assert_eq!(
        user_file(Agent::OpenCode),
        adroit_model::paths::config_dir()
            .join("screen-rules")
            .join("opencode.toml")
    );
}

#[test]
fn a_rule_shows_the_rows_it_goes_by_though_it_matched_no_sign() {
    let rules = Rules::parse(
        "codex",
        r#"
agent = "codex"
[lines]
chevron = '^›'
[regions]
composer = { last = "chevron" }
below = { after = "composer" }
first_below = { within = "below", head = 1 }
[[rules]]
id = "none"
default = true
when = { absent = "composer" }
[[rules]]
id = "idle"
state = "idle"
default = true
shows = "first_below"
"#,
    )
    .unwrap();
    let idle = rules.judge(&screen("› \nhint\nmore"));
    assert_eq!(idle.matched.evidence, vec![(1, "hint".to_string())]);
    assert!(rules.judge(&screen("text")).matched.evidence.is_empty());
}

#[test]
fn the_set_the_specification_shows_can_be_read() {
    let spec = include_str!("../../../../docs/spec/screen-rules.md");
    let (_, after) = spec.split_once("```toml\n").unwrap();
    let (example, _) = after.split_once("```").unwrap();
    let rules = Rules::parse("codex", example).unwrap();
    let working = rules.judge(&screen("• Working (2s • esc to interrupt)\n› "));
    assert_eq!(working.matched.rule, "codex.interrupt_hint");
}
