//! Sessions run in a felis daemon served from the test itself.

use std::path::PathBuf;
use std::time::Duration;

use adroit_term::{Daemon, Key, Palette, Pane, Size, Spawn, tags};

/// A daemon on a socket of its own, served until the test ends.
async fn daemon() -> (Daemon, tempfile::TempDir) {
    daemon_telling(|_| {}).await
}

/// [`daemon`], telling `running` often how many sessions still run.
async fn daemon_telling(
    running: impl FnMut(usize) + Send + 'static,
) -> (Daemon, tempfile::TempDir) {
    // A socket path has to be short, so not under the usual temp folder.
    let dir = tempfile::Builder::new()
        .prefix("adroit-")
        .tempdir_in("/tmp")
        .unwrap();
    // The daemon listens only in a folder open to no one else.
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let socket = dir.path().join("d.sock");
    let serving = socket.clone();
    let served =
        tokio::spawn(
            async move { Daemon::serve(&serving, Duration::from_millis(20), running).await },
        );
    let d = Daemon::new(socket);
    let mut last = None;
    for _ in 0..100 {
        match d.sessions().await {
            Ok(_) => return (d, dir),
            Err(e) => last = Some(e),
        }
        if served.is_finished() {
            panic!("the daemon stopped: {:?}", served.await);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the daemon did not answer: {last:?}");
}

fn sh(script: &str) -> Spawn {
    Spawn {
        program: "/bin/sh".into(),
        args: vec!["-c".into(), script.into()],
        cwd: PathBuf::from("/tmp"),
        cols: 40,
        rows: 6,
        ..Spawn::default()
    }
}

/// Wait until `pane`'s screen satisfies `ok`, or fail after a while.
async fn until(pane: &Pane, ok: impl Fn(&[String]) -> bool) -> Vec<String> {
    for _ in 0..200 {
        let text = pane.screen().text();
        if ok(&text) {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("never shown; the screen has {:?}", pane.screen().text());
}

async fn attach(d: &Daemon, id: u128) -> Pane {
    Pane::attach(d, id, Size::new(40, 6), &Palette::default(), || {})
        .await
        .unwrap()
}

#[tokio::test]
async fn a_session_s_screen_shows_what_its_program_wrote() {
    let (d, _dir) = daemon().await;
    let id = d.spawn(sh("printf hello; sleep 5")).await.unwrap();
    let pane = attach(&d, id).await;
    until(&pane, |t| t[0] == "hello").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_headless_start_is_released_before_its_command_runtime_exits() {
    let (daemon, dir) = daemon().await;
    for round in 0..20 {
        let marker = dir.path().join(format!("released-{round}"));
        let mut spawn =
            sh(r#"printf headless > "$ADROIT_RELEASE_PROOF"; printf headless; sleep 5"#);
        spawn.env.push((
            "ADROIT_RELEASE_PROOF".into(),
            marker.to_string_lossy().into_owned(),
        ));
        let starting = daemon.clone();
        let id = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let pane = Pane::start(
                    &starting,
                    spawn,
                    Size::new(40, 6),
                    &Palette::default(),
                    |_| || {},
                )
                .await
                .unwrap();
                pane.id()
            })
        })
        .await
        .unwrap();
        let mut released = false;
        for _ in 0..100 {
            if marker.exists()
                && daemon
                    .visible(id)
                    .await
                    .unwrap()
                    .iter()
                    .any(|line| line.contains("headless"))
            {
                released = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let visible = daemon.visible(id).await.unwrap();
        daemon.end(id).await.unwrap();
        assert!(
            released,
            "startup round {round}: program ran = {}, visible = {visible:?}",
            marker.exists()
        );
    }
}

#[tokio::test]
async fn a_session_is_listed_with_its_tags_and_keeps_running_without_a_window() {
    let (d, _dir) = daemon().await;
    let mut spawn = sh("printf kept; sleep 5");
    spawn.tags = tags::tagged("adroit.kind", "shell");
    let id = d.spawn(spawn).await.unwrap();
    let listed = d.sessions().await.unwrap();
    let s = listed.iter().find(|s| s.id == id).unwrap();
    assert_eq!(
        tags::value(&s.tags, "adroit.kind").as_deref(),
        Some("shell")
    );
    drop(attach(&d, id).await);
    let pane = attach(&d, id).await;
    until(&pane, |t| t[0] == "kept").await;
}

#[tokio::test]
async fn keys_reach_the_program_as_its_modes_ask() {
    let (d, _dir) = daemon().await;
    let id = d
        .spawn(sh(
            "stty raw -echo; printf ready; head -c 3 | od -An -tx1 | tr -s ' '; sleep 5",
        ))
        .await
        .unwrap();
    let pane = attach(&d, id).await;
    until(&pane, |t| t[0].starts_with("ready")).await;
    pane.key(&Key::new("c").ctrl());
    pane.key(&Key::new(" ").ctrl());
    pane.key(&Key::new("a"));
    until(&pane, |t| t.iter().any(|l| l.contains("03 00 61"))).await;
}

#[tokio::test]
async fn a_paste_is_bracketed_when_the_program_asks() {
    let (d, _dir) = daemon().await;
    let id = d
        .spawn(sh(r"stty raw -echo; printf '\033[?2004hready'; head -c 13 | od -An -c | tr -d ' \n'; sleep 5"))
        .await
        .unwrap();
    let pane = attach(&d, id).await;
    until(&pane, |t| t[0].starts_with("ready")).await;
    assert!(pane.bracketed_paste());
    pane.paste("x");
    until(&pane, |t| t.iter().any(|l| l.contains("033[200~x033[201~"))).await;
}

#[tokio::test]
async fn a_status_the_program_reports_is_listed() {
    let (d, _dir) = daemon().await;
    let id = d
        .spawn(sh(
            r"printf '\033]7501;state=blocked:kind=permission\033\\'; sleep 5",
        ))
        .await
        .unwrap();
    for _ in 0..100 {
        let listed = d.sessions().await.unwrap();
        if let Some(status) = listed
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.status.clone())
        {
            assert_eq!(status.state, "blocked");
            assert_eq!(status.kind.as_deref(), Some("permission"));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no status listed");
}

#[tokio::test]
async fn everything_a_session_keeps_is_read_with_its_colours() {
    let (d, _dir) = daemon().await;
    let id = d
        .spawn(sh(
            r"for i in 1 2 3 4 5 6 7 8 9 10; do echo line$i; done; printf '\033[31mred'; sleep 5",
        ))
        .await
        .unwrap();
    let pane = attach(&d, id).await;
    until(&pane, |t| t.iter().any(|l| l == "red")).await;
    let all = d.everything(id, 40, &Palette::default()).await.unwrap();
    let text = all.text();
    assert_eq!(text[0], "line1");
    let red = text.iter().position(|l| l == "red").unwrap();
    assert_eq!(all.lines[red].runs[0].style.fg, Palette::default().ansi[1]);
}

#[tokio::test]
async fn everything_a_pane_reads_is_in_the_colours_its_program_set() {
    let (d, _dir) = daemon().await;
    let blue = adroit_term::Rgb::new(0, 0, 255);
    let id = d
        .spawn(sh(
            r"printf '\033]4;1;rgb:00/00/ff\033\\\033[31mred'; sleep 5",
        ))
        .await
        .unwrap();
    let pane = attach(&d, id).await;
    until(&pane, |t| t.iter().any(|l| l == "red")).await;
    for _ in 0..200 {
        if pane.screen().lines[0].runs[0].style.fg == blue {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let all = pane.everything(&d, 40).await.unwrap();
    let red = all.text().iter().position(|l| l == "red").unwrap();
    assert_eq!(all.lines[red].runs[0].style.fg, blue);
}

#[tokio::test]
async fn a_session_whose_program_exits_says_so() {
    let (d, _dir) = daemon().await;
    let id = d.spawn(sh("sleep 0.3")).await.unwrap();
    let pane = attach(&d, id).await;
    for _ in 0..200 {
        if pane.exited() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("never exited");
}

#[tokio::test]
async fn an_ended_session_is_listed_no_more() {
    let (d, _dir) = daemon().await;
    let id = d.spawn(sh("sleep 5")).await.unwrap();
    d.end(id).await.unwrap();
    for _ in 0..200 {
        if !d
            .sessions()
            .await
            .unwrap()
            .iter()
            .any(|s| s.id == id && !s.exited)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("still listed");
}

#[tokio::test]
async fn the_daemon_tells_how_many_sessions_run_as_they_start_and_end() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (d, _dir) = daemon_telling(move |n| {
        let _ = tx.send(n);
    })
    .await;
    let until_told = async |rx: &mut tokio::sync::mpsc::UnboundedReceiver<usize>, want| {
        tokio::time::timeout(Duration::from_secs(5), async {
            while rx.recv().await != Some(want) {}
        })
        .await
        .unwrap_or_else(|_| panic!("never told {want}"));
    };
    until_told(&mut rx, 0).await;
    let first = d.spawn(sh("sleep 5")).await.unwrap();
    let second = d.spawn(sh("sleep 5")).await.unwrap();
    until_told(&mut rx, 2).await;
    d.end(first).await.unwrap();
    until_told(&mut rx, 1).await;
    d.end(second).await.unwrap();
    until_told(&mut rx, 0).await;
}

#[tokio::test]
async fn a_session_whose_program_exited_runs_no_more() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (d, _dir) = daemon_telling(move |n| {
        let _ = tx.send(n);
    })
    .await;
    let id = d.spawn(sh("sleep 0.3")).await.unwrap();
    let exited_but_listed = async {
        loop {
            let listed = d.sessions().await.unwrap();
            if listed.iter().any(|s| s.id == id && s.exited) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), exited_but_listed)
        .await
        .expect("never listed as exited");
    while rx.try_recv().is_ok() {}
    let told = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap();
    assert_eq!(told, Some(0));
}

#[tokio::test]
async fn a_resize_reaches_the_program() {
    let (d, _dir) = daemon().await;
    let id = d.spawn(sh("sleep 0.5; stty size; sleep 5")).await.unwrap();
    let pane = attach(&d, id).await;
    pane.resize(Size::new(30, 5));
    until(&pane, |t| t.iter().any(|l| l == "5 30")).await;
}

#[tokio::test]
async fn a_click_goes_to_a_program_that_asked_for_the_mouse() {
    let (d, _dir) = daemon().await;
    let id = d
        .spawn(sh(r"stty raw -echo; printf '\033[?1000h\033[?1006hready'; head -c 18 | tr '\033' 'E'; sleep 5"))
        .await
        .unwrap();
    let pane = attach(&d, id).await;
    until(&pane, |t| t[0].starts_with("ready")).await;
    assert!(pane.takes_mouse());
    pane.click(2, 1);
    until(&pane, |t| {
        t.iter().any(|l| l.contains("E[<0;3;2ME[<0;3;2m"))
    })
    .await;
}

#[tokio::test]
async fn an_image_the_program_shows_is_drawn_over_the_cells_it_covers() {
    let (d, _dir) = daemon().await;
    // One red pixel, put two cells wide and one high.
    let id = d
        .spawn(sh(
            r"printf '\033_Ga=T,f=24,s=1,v=1,c=2,r=1;/wAA\033\\'; sleep 5",
        ))
        .await
        .unwrap();
    let pane = attach(&d, id).await;
    for _ in 0..200 {
        let pictures = pane.pictures();
        if let Some(p) = pictures.first() {
            assert_eq!(p.dst, [0.0, 0.0, 2.0, 1.0]);
            assert_eq!(p.size, [1, 1]);
            assert!(p.url.starts_with("data:image/png;base64,"));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no image drawn");
}

#[tokio::test]
async fn an_image_given_only_its_width_keeps_its_shape_in_cells_taller_than_wide() {
    let (d, _dir) = daemon().await;
    // Two by two pixels, put four cells wide and as tall as its shape asks.
    let id = d
        .spawn(sh(
            r"printf '\033_Ga=T,f=24,s=2,v=2,c=4;/wAA/wAA/wAA/wAA\033\\'; sleep 5",
        ))
        .await
        .unwrap();
    // Cells half as wide as they are tall: a square four cells wide is two
    // rows high.
    let size = Size {
        cell_width: 8.0,
        cell_height: 16.0,
        ..Size::new(40, 6)
    };
    let pane = Pane::attach(&d, id, size, &Palette::default(), || {})
        .await
        .unwrap();
    for _ in 0..200 {
        if let Some(p) = pane.pictures().first() {
            assert_eq!(p.dst, [0.0, 0.0, 4.0, 2.0]);
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no image drawn");
}

#[tokio::test]
async fn an_image_stays_within_the_rows_the_program_was_moved_past() {
    let (d, _dir) = daemon().await;
    let id = d
        .spawn(sh(
            r"sleep 0.5; printf '\033_Ga=T,f=24,s=2,v=2,c=4;/wAA/wAA/wAA/wAA\033\\'; printf '\nafter'; sleep 5",
        ))
        .await
        .unwrap();
    // Cells of a fraction of a pixel, as a measured font gives.
    let size = Size {
        cell_width: 8.6,
        cell_height: 17.0,
        ..Size::new(40, 6)
    };
    let pane = Pane::attach(&d, id, size, &Palette::default(), || {})
        .await
        .unwrap();
    let text = until(&pane, |t| t.iter().any(|l| l == "after")).await;
    let after = text.iter().position(|l| l == "after").unwrap() as f32;
    let p = pane.pictures().into_iter().next().expect("no image drawn");
    assert!(
        p.dst[1] + p.dst[3] <= after,
        "{:?} runs into row {after}",
        p.dst
    );
}

#[tokio::test]
async fn each_image_the_program_sends_is_handed_over_once_whole() {
    let (d, _dir) = daemon().await;
    // A red pixel, then a two by one image of a red and a green pixel.
    let id = d
        .spawn(sh(
            r"printf '_Ga=T,f=24,s=1,v=1;/wAA\'; printf '_Ga=T,f=24,s=2,v=1;/wAAAP8A\'; sleep 5",
        ))
        .await
        .unwrap();
    let pane = attach(&d, id).await;
    let mut got = Vec::new();
    for _ in 0..200 {
        got.extend(pane.take_drawn());
        if got.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let shapes: Vec<_> = got.iter().map(|i| (i.width, i.height)).collect();
    assert_eq!(shapes, [(1, 1), (2, 1)]);
    assert_eq!(got[1].rgba, [255, 0, 0, 255, 0, 255, 0, 255]);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(pane.take_drawn().is_empty(), "handed over again");
}

#[tokio::test]
async fn tags_are_changed_on_a_running_session() {
    let (d, _dir) = daemon().await;
    let id = d.spawn(sh("sleep 5")).await.unwrap();
    d.tag(id, tags::tagged("adroit.name", "renamed"), vec![])
        .await
        .unwrap();
    let listed = d.sessions().await.unwrap();
    let s = listed.iter().find(|s| s.id == id).unwrap();
    assert_eq!(
        tags::value(&s.tags, "adroit.name").as_deref(),
        Some("renamed")
    );
}

/// A palette no terminal draws with by default.
fn themed(background: adroit_term::Rgb) -> Palette {
    Palette {
        background,
        ..Palette::default()
    }
}

async fn start(d: &Daemon, spawn: Spawn, palette: &Palette) -> Pane {
    Pane::start(d, spawn, Size::new(40, 6), palette, |_| || {})
        .await
        .unwrap()
}

/// Ask for the background as the terminal's reply, its escape shown as `E`.
const ASK_BACKGROUND: &str = r"stty raw -echo; printf '\033]11;?\033\\'; head -c 23 | tr '\033' E";

#[tokio::test]
async fn a_program_asking_for_the_background_as_it_starts_is_told_the_theme_s() {
    let (d, _dir) = daemon().await;
    let palette = themed(adroit_term::Rgb::new(0x12, 0x34, 0x56));
    // The reply is shown as the terminal echoes it, the program asking before
    // it does anything else.
    let pane = start(&d, sh(r"printf '\033]11;?\033\\'; sleep 5"), &palette).await;
    let text = until(&pane, |t| t.iter().any(|l| l.contains("rgb:"))).await;
    assert!(
        text.iter().any(|l| l.contains("rgb:1212/3434/5656")),
        "{text:?}"
    );
}

#[tokio::test]
async fn a_program_started_in_a_pane_draws_from_the_top_of_its_screen() {
    let (d, _dir) = daemon().await;
    let pane = start(&d, sh("printf hello; sleep 5"), &Palette::default()).await;
    until(&pane, |t| t[0] == "hello").await;
}

#[tokio::test]
async fn a_program_started_in_a_pane_is_told_a_new_theme_once_it_changes() {
    let (d, _dir) = daemon().await;
    let first = themed(adroit_term::Rgb::new(0x12, 0x34, 0x56));
    let pane = start(
        &d,
        sh(&format!(
            "stty raw -echo; printf ready; head -c 1 >/dev/null; {ASK_BACKGROUND}; sleep 5"
        )),
        &first,
    )
    .await;
    until(&pane, |t| t[0].starts_with("ready")).await;
    pane.set_palette(&themed(adroit_term::Rgb::new(0xab, 0xcd, 0xef)));
    pane.type_text("x");
    let text = until(&pane, |t| t.iter().any(|l| l.contains("rgb:"))).await;
    assert!(
        text.iter().any(|l| l.contains("rgb:abab/cdcd/efef")),
        "{text:?}"
    );
}

#[tokio::test]
async fn a_program_that_cannot_be_found_is_not_started() {
    let (d, _dir) = daemon().await;
    let spawn = Spawn {
        program: "/no/such/program".into(),
        ..sh("")
    };
    let started = Pane::start(&d, spawn, Size::new(40, 6), &Palette::default(), |_| || {}).await;
    assert!(started.is_err());
    assert!(d.sessions().await.unwrap().is_empty());
}

#[tokio::test]
async fn only_input_sent_to_the_program_updates_the_last_input_time() {
    let (d, _dir) = daemon().await;
    let id = d.spawn(sh("printf ready; cat")).await.unwrap();
    let pane = attach(&d, id).await;
    until(&pane, |t| t[0].contains("ready")).await;
    assert_eq!(pane.last_input_ms(), None);
    assert!(!pane.key(&Key::new("Shift")));
    pane.click(1, 1);
    pane.wheel(1, 1, 1);
    pane.resize(Size::new(50, 6));
    assert_eq!(pane.last_input_ms(), None);
    assert!(pane.key(&Key::new("Enter")));
    let key_at = pane.last_input_ms().unwrap();
    tokio::time::sleep(Duration::from_millis(2)).await;
    pane.type_text("answer");
    let typed_at = pane.last_input_ms().unwrap();
    assert!(typed_at >= key_at);
    tokio::time::sleep(Duration::from_millis(2)).await;
    pane.paste("selection");
    assert!(pane.last_input_ms().unwrap() >= typed_at);
}

#[tokio::test]
async fn session_ownership_is_prepared_before_its_program_can_edit() {
    let (daemon, dir) = daemon().await;
    let marker = dir.path().join("released-after-ownership");
    let mut spawn = sh("printf ready > \"$ADROIT_RELEASE_PROOF\"; sleep 5");
    spawn.env.push((
        "ADROIT_RELEASE_PROOF".into(),
        marker.to_string_lossy().into_owned(),
    ));
    let marker_check = marker.clone();
    let pane = Pane::start_prepared(
        &daemon,
        spawn,
        Size::new(40, 6),
        &Palette::default(),
        |_| || {},
        move |id| async move {
            assert!(!marker_check.exists());
            assert_ne!(id, 0);
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert!(!marker_check.exists());
            Ok(())
        },
    )
    .await
    .unwrap();
    for _ in 0..100 {
        if marker.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(marker.exists());
    daemon.end(pane.id()).await.unwrap();
}

#[tokio::test]
async fn refused_startup_preparation_never_releases_the_program_or_keeps_its_session() {
    let (daemon, dir) = daemon().await;
    let marker = dir.path().join("must-not-run");
    let mut spawn = sh("printf bad > \"$ADROIT_RELEASE_PROOF\"; sleep 5");
    spawn.env.push((
        "ADROIT_RELEASE_PROOF".into(),
        marker.to_string_lossy().into_owned(),
    ));
    let result = Pane::start_prepared(
        &daemon,
        spawn,
        Size::new(40, 6),
        &Palette::default(),
        |_| || {},
        |_| async { Err(adroit_term::Error::Protocol("assignment changed".into())) },
    )
    .await;
    assert!(
        matches!(&result,Err(adroit_term::Error::PreparationAborted {session,reason}) if *session != 0 && reason.contains("assignment changed"))
    );
    assert!(!marker.exists());
    assert!(daemon.sessions().await.unwrap().is_empty());
}
