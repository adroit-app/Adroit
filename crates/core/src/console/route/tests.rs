use super::*;
use crate::testing::here;

#[test]
fn an_empty_prompt_passes_ctrl_enter_to_its_session_alone() {
    let session = Tab::Session(here(), "s1".into());
    assert_eq!(passes_enter(Some(&session), "  \n"), Some("s1".to_string()));
    assert_eq!(passes_enter(Some(&session), "hi"), None);
    let draft = Tab::draft(here(), "/w");
    assert_eq!(passes_enter(Some(&draft), ""), None);
    assert_eq!(passes_enter(None, ""), None);
}

#[test]
fn an_unbound_key_in_a_session_s_console_starts_reading_unless_it_is_read_already() {
    // (area, a session shown, its keys passed on, reading) -> reading starts
    let table = [
        (Area::Console, true, false, false, true),
        (Area::Console, true, false, true, false),
        (Area::Console, true, true, false, false),
        (Area::Console, false, false, false, false),
        (Area::Prompt, true, false, false, false),
        (Area::Sidebar, true, false, false, false),
    ];
    for (area, session, passing, reading, want) in table {
        assert_eq!(
            starts_reading(area, session, passing, reading),
            want,
            "{area:?} {session} {passing} {reading}"
        );
    }
}

#[test]
fn a_shell_takes_the_console_s_keys_and_a_waiting_session_the_prompt_s_too() {
    // (area, a session shown, it waits) -> the keys go to the console's program
    let table = [
        (Area::Console, false, false, true),
        (Area::Console, true, false, false),
        (Area::Console, true, true, true),
        (Area::Prompt, false, false, false),
        (Area::Prompt, true, false, false),
        (Area::Prompt, true, true, true),
        (Area::Sidebar, true, true, false),
        (Area::Details, true, true, false),
    ];
    for (area, session, waits, want) in table {
        assert_eq!(
            passes(area, session, waits),
            want,
            "{area:?} {session} {waits}"
        );
    }
}

#[test]
fn text_for_a_session_is_written_in_the_prompt_and_the_rest_goes_to_the_console_s_program() {
    use Typed::*;
    // (area, a session shown, keys passed directly, it waits) -> where text goes
    let table = [
        (Area::Prompt, true, false, false, Prompt),
        (Area::Prompt, false, false, false, Prompt),
        (Area::Prompt, true, false, true, Program),
        (Area::Console, true, false, false, Prompt),
        (Area::Console, true, true, false, Program),
        (Area::Console, true, false, true, Program),
        (Area::Console, false, false, false, Program),
        (Area::Sidebar, true, false, false, Nowhere),
        (Area::Details, false, true, true, Nowhere),
    ];
    for (area, session, direct, waits, want) in table {
        assert_eq!(
            typed_into(area, session, direct, waits),
            want,
            "{area:?} {session} {direct} {waits}"
        );
    }
}

#[test]
fn a_key_is_looked_up_where_the_keys_are_and_only_globally_while_they_pass_on() {
    use Context::*;
    let session = Tab::Session(here(), "s".into());
    let viewer = Tab::Viewer(here(), "/a.png".into());
    let draft = Tab::draft(here(), "/w");
    // (area, shown, passing, windowing, inserting) -> contexts
    type Case<'a> = (Area, Option<&'a Tab>, bool, bool, bool, Vec<Context>);
    let table: [Case; 10] = [
        (
            Area::Sidebar,
            Some(&session),
            false,
            false,
            false,
            vec![Tree, Global],
        ),
        (
            Area::Details,
            Some(&session),
            false,
            false,
            false,
            vec![Details, Tree, Global],
        ),
        (
            Area::Console,
            Some(&session),
            false,
            false,
            false,
            vec![Console, Global],
        ),
        (
            Area::Console,
            Some(&viewer),
            false,
            false,
            false,
            vec![Viewer, Global],
        ),
        (
            Area::Console,
            Some(&session),
            true,
            false,
            false,
            vec![Global],
        ),
        (
            Area::Console,
            Some(&session),
            false,
            true,
            false,
            vec![Global],
        ),
        (
            Area::Prompt,
            Some(&session),
            false,
            false,
            false,
            vec![Prompt, Global],
        ),
        (
            Area::Prompt,
            Some(&session),
            false,
            false,
            true,
            vec![Prompt],
        ),
        (
            Area::Prompt,
            Some(&draft),
            false,
            false,
            true,
            vec![Draft, Prompt],
        ),
        (
            Area::Console,
            None,
            false,
            false,
            false,
            vec![Draft, Console, Global],
        ),
    ];
    for (area, shown, passing, windowing, inserting, want) in table {
        assert_eq!(
            contexts(area, shown, passing, windowing, inserting),
            want,
            "{area:?} {shown:?} {passing} {windowing} {inserting}"
        );
    }
    assert_eq!(help_contexts(Area::Console, None), vec![Console, Global]);
}

#[test]
fn the_footer_names_what_the_keys_do_where_they_are() {
    let session = Tab::Session(here(), "s".into());
    let shell = Tab::Terminal(here(), 1);
    let viewer = Tab::Viewer(here(), "/a.png".into());
    let at = |area, shown| Footing {
        picking: false,
        area,
        shown,
        direct: false,
        answering: false,
        reading: None,
        prompt: ("insert", "INSERT"),
        repeating: false,
    };
    let cases = [
        (
            Footing {
                picking: true,
                ..at(Area::Console, Some(&session))
            },
            ("PICK", "normal"),
        ),
        (
            Footing {
                repeating: true,
                ..at(Area::Prompt, Some(&session))
            },
            ("REPEAT", "pass"),
        ),
        (
            Footing {
                repeating: true,
                ..at(Area::Sidebar, None)
            },
            ("REPEAT", "pass"),
        ),
        (
            Footing {
                direct: true,
                ..at(Area::Console, Some(&session))
            },
            ("DIRECT", "pass"),
        ),
        (
            Footing {
                answering: true,
                ..at(Area::Console, Some(&session))
            },
            ("PASS", "pass"),
        ),
        (at(Area::Console, Some(&shell)), ("PASS", "pass")),
        (
            Footing {
                reading: Some("normal"),
                ..at(Area::Console, Some(&session))
            },
            ("READ", "normal"),
        ),
        (
            Footing {
                reading: Some("normal"),
                ..at(Area::Console, Some(&shell))
            },
            ("READ", "normal"),
        ),
        (
            Footing {
                reading: Some("visual"),
                answering: true,
                ..at(Area::Console, Some(&session))
            },
            ("VISUAL", "normal"),
        ),
        (
            Footing {
                reading: Some("visual"),
                ..at(Area::Console, Some(&session))
            },
            ("VISUAL", "normal"),
        ),
        (at(Area::Console, Some(&viewer)), ("VIEWER", "normal")),
        (at(Area::Console, Some(&session)), ("CONSOLE", "normal")),
        (
            Footing {
                answering: true,
                ..at(Area::Prompt, Some(&session))
            },
            ("PASS", "pass"),
        ),
        (at(Area::Prompt, Some(&session)), ("INSERT", "insert")),
        (at(Area::Sidebar, None), ("TREE", "normal")),
        (at(Area::Details, None), ("DETAILS", "normal")),
    ];
    for (footing, (word, class)) in cases {
        assert_eq!(footing.mode(), (word.to_string(), class), "{footing:?}");
    }
}
