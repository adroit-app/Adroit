//! The system clipboard, which what the window copies goes to.

use std::path::PathBuf;

/// The programs that put their input on the system clipboard, the first of
/// them installed taken.
#[cfg(target_os = "macos")]
fn clipboard_commands() -> Vec<std::process::Command> {
    let mut cmd = std::process::Command::new("pbcopy");
    // pbcopy decodes its input by the locale. An app opened from Finder has
    // none, so pbcopy falls back to the user's legacy text encoding (Mac
    // Japanese, say), fails on UTF-8 and empties the clipboard without an
    // error. `LC_CTYPE` alone loses to an `LC_ALL` the app may inherit.
    cmd.env("LC_ALL", "en_US.UTF-8");
    vec![cmd]
}

/// The programs that put their input on the system clipboard, the first of
/// them installed taken.
#[cfg(windows)]
fn clipboard_commands() -> Vec<std::process::Command> {
    vec![std::process::Command::new("clip")]
}

/// The programs that put their input on the system clipboard, the first of
/// them installed taken: those of the display server the window is on.
#[cfg(all(unix, not(target_os = "macos")))]
fn clipboard_commands() -> Vec<std::process::Command> {
    free_desktop_clipboards(std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty()))
        .into_iter()
        .map(|(program, args)| {
            let mut cmd = std::process::Command::new(program);
            cmd.args(args);
            cmd
        })
        .collect()
}

/// The programs, with their arguments, that put their input on the
/// clipboard of a desktop that follows freedesktop.org, Wayland's first on
/// `wayland`; X's serve Wayland's own programs too, through Xwayland.
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
fn free_desktop_clipboards(wayland: bool) -> Vec<(&'static str, &'static [&'static str])> {
    let mut programs: Vec<(&str, &[&str])> = Vec::new();
    if wayland {
        programs.push(("wl-copy", &[]));
    }
    programs.push(("xclip", &["-selection", "clipboard"]));
    programs.push(("xsel", &["--clipboard", "--input"]));
    programs
}

/// `text` as the clipboard program reads it: UTF-8, and on Windows UTF-16
/// behind its byte-order mark, as `clip` reads anything else in the console's
/// code page.
fn encoded(text: &str) -> Vec<u8> {
    if cfg!(windows) {
        [0xfeff_u16]
            .into_iter()
            .chain(text.encode_utf16())
            .flat_map(u16::to_le_bytes)
            .collect()
    } else {
        text.as_bytes().to_vec()
    }
}

/// Put `text` on the system clipboard.
pub(crate) fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    copy_with(clipboard_commands(), &encoded(text))
}

/// Put `paths` on the system clipboard: as files the Finder pastes, with
/// their paths as text, when they are on this computer; as their paths
/// alone, a line each, when they are on a work machine elsewhere.
pub(crate) fn copy_paths(paths: &[PathBuf], here: bool) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    if here {
        return pasteboard::write_files(paths);
    }
    let _ = here;
    let text: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    copy_to_clipboard(&text.join("\n"))
}

/// The paths on the system clipboard: the files the Finder or the tree
/// put there when they are on this computer, or else the paths its text
/// names. None when it holds anything else.
pub(crate) fn read_paths(here: bool) -> std::io::Result<Vec<PathBuf>> {
    #[cfg(target_os = "macos")]
    if here && let Some(files) = pasteboard::read_files() {
        return Ok(files);
    }
    let _ = here;
    Ok(adroit_core::tree::clipboard::paths_in(&read_text()?))
}

/// The text on the system clipboard.
#[cfg(target_os = "macos")]
fn read_text() -> std::io::Result<String> {
    Ok(pasteboard::read_text().unwrap_or_default())
}

/// The text on the system clipboard, from the first of the programs that
/// print it that is installed and ends well.
#[cfg(not(target_os = "macos"))]
fn read_text() -> std::io::Result<String> {
    let commands: Vec<(&str, &[&str])> = if cfg!(windows) {
        vec![(
            "powershell",
            &["-NoProfile", "-Command", "Get-Clipboard -Raw"],
        )]
    } else {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty());
        let mut programs: Vec<(&str, &[&str])> = Vec::new();
        if wayland {
            programs.push(("wl-paste", &["--no-newline"]));
        }
        programs.push(("xclip", &["-selection", "clipboard", "-o"]));
        programs.push(("xsel", &["--clipboard", "--output"]));
        programs
    };
    let mut last = None;
    for (program, args) in commands {
        match std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        {
            Ok(out) if out.status.success() => {
                return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
            }
            Ok(out) => {
                last = Some(std::io::Error::other(format!(
                    "{program}: ended with {}",
                    out.status
                )));
            }
            Err(e) => last = Some(std::io::Error::new(e.kind(), format!("{program}: {e}"))),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("no clipboard program to paste with")))
}

/// The general pasteboard, read and written as the Finder does for files.
#[cfg(target_os = "macos")]
mod pasteboard {
    use std::path::PathBuf;

    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{
        NSPasteboard, NSPasteboardItem, NSPasteboardTypeFileURL, NSPasteboardTypeString,
        NSPasteboardWriting,
    };
    use objc2_foundation::{NSArray, NSString, NSURL};

    /// `paths` as one item each, a file URL the Finder pastes and the path
    /// as text for anything that pastes text.
    pub(super) fn write_files(paths: &[PathBuf]) -> std::io::Result<()> {
        let items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = paths
            .iter()
            .map(|path| {
                let text = NSString::from_str(&path.to_string_lossy());
                let url = NSURL::fileURLWithPath(&text);
                let item = NSPasteboardItem::new();
                if let Some(url) = url.absoluteString() {
                    item.setString_forType(&url, unsafe { NSPasteboardTypeFileURL });
                }
                item.setString_forType(&text, unsafe { NSPasteboardTypeString });
                ProtocolObject::from_retained(item)
            })
            .collect();
        let board = NSPasteboard::generalPasteboard();
        board.clearContents();
        if board.writeObjects(&NSArray::from_retained_slice(&items)) {
            Ok(())
        } else {
            Err(std::io::Error::other("the pasteboard took nothing"))
        }
    }

    /// The files on the pasteboard, by their file URLs; `None` when it
    /// holds none.
    pub(super) fn read_files() -> Option<Vec<PathBuf>> {
        let items = NSPasteboard::generalPasteboard().pasteboardItems()?;
        let urls: Vec<String> = items
            .iter()
            .filter_map(|item| item.stringForType(unsafe { NSPasteboardTypeFileURL }))
            .map(|url| url.to_string())
            .collect();
        let files = adroit_core::tree::clipboard::paths_in(&urls.join("\n"));
        (!files.is_empty()).then_some(files)
    }

    pub(super) fn read_text() -> Option<String> {
        NSPasteboard::generalPasteboard()
            .stringForType(unsafe { NSPasteboardTypeString })
            .map(|text| text.to_string())
    }
}

/// Give `input` to the first of `commands` that takes it and ends well; why
/// none did, when none does.
fn copy_with(commands: Vec<std::process::Command>, input: &[u8]) -> std::io::Result<()> {
    let mut missing = None;
    let mut failed = None;
    for mut cmd in commands {
        let program = cmd.get_program().to_string_lossy().into_owned();
        match copied(&mut cmd, input) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.get_or_insert(program);
            }
            Err(e) => {
                failed.get_or_insert(std::io::Error::new(e.kind(), format!("{program}: {e}")));
            }
        }
    }
    Err(failed.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "no clipboard program to copy with: install {}",
                missing.as_deref().unwrap_or("one")
            ),
        )
    }))
}

/// Give `input` to `cmd` and wait for it to end well.
fn copied(cmd: &mut std::process::Command, input: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let written = child.stdin.take().expect("stdin is piped").write_all(input);
    let status = child.wait()?;
    written?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("ended with {status}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn the_clipboard_reads_the_text_as_utf_8_whatever_the_app_s_locale() {
        let cmd = clipboard_commands().remove(0);
        let lc_all = cmd
            .get_envs()
            .find(|(k, _)| *k == "LC_ALL")
            .and_then(|(_, v)| v);
        assert_eq!(lc_all, Some(std::ffi::OsStr::new("en_US.UTF-8")));
    }

    #[cfg(unix)]
    fn shell(script: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[cfg(unix)]
    #[test]
    fn a_clipboard_program_that_fails_gives_the_text_to_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let took = dir.path().join("took");
        let commands = vec![
            std::process::Command::new("adroit-no-such-clipboard"),
            shell("cat >/dev/null; exit 1"),
            shell(&format!("cat > '{}'", took.display())),
        ];
        copy_with(commands, "日本語".as_bytes()).unwrap();
        assert_eq!(std::fs::read_to_string(took).unwrap(), "日本語");
    }

    #[cfg(unix)]
    #[test]
    fn copying_fails_when_every_clipboard_program_fails_or_is_missing() {
        let failed = copy_with(
            vec![
                std::process::Command::new("adroit-no-such-clipboard"),
                shell("exit 3"),
            ],
            b"x",
        )
        .unwrap_err();
        assert_ne!(failed.kind(), std::io::ErrorKind::NotFound);
        let missing = copy_with(
            vec![std::process::Command::new("adroit-no-such-clipboard")],
            b"x",
        )
        .unwrap_err();
        assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);
        assert!(missing.to_string().contains("adroit-no-such-clipboard"));
    }

    #[test]
    fn a_free_desktop_copies_with_wayland_s_program_first_only_on_wayland() {
        let programs = |wayland| {
            free_desktop_clipboards(wayland)
                .into_iter()
                .map(|(program, _)| program)
                .collect::<Vec<_>>()
        };
        assert_eq!(programs(true), ["wl-copy", "xclip", "xsel"]);
        assert_eq!(programs(false), ["xclip", "xsel"]);
    }
}
