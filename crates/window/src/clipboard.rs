//! The system clipboard, which what the window copies goes to.

/// The program that puts its input on the system clipboard.
#[cfg(not(windows))]
fn clipboard_command() -> std::process::Command {
    let mut cmd = std::process::Command::new("pbcopy");
    // pbcopy decodes its input by the locale. An app opened from Finder has
    // none, so pbcopy falls back to the user's legacy text encoding (Mac
    // Japanese, say), fails on UTF-8 and empties the clipboard without an
    // error. `LC_CTYPE` alone loses to an `LC_ALL` the app may inherit.
    cmd.env("LC_ALL", "en_US.UTF-8");
    cmd
}

/// The program that puts its input on the system clipboard.
#[cfg(windows)]
fn clipboard_command() -> std::process::Command {
    std::process::Command::new("clip")
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
    use std::io::Write;
    let mut child = clipboard_command()
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(&encoded(text))?;
    child.wait()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clipboard_reads_the_text_as_utf_8_whatever_the_app_s_locale() {
        let cmd = clipboard_command();
        let lc_all = cmd
            .get_envs()
            .find(|(k, _)| *k == "LC_ALL")
            .and_then(|(_, v)| v);
        assert_eq!(lc_all, Some(std::ffi::OsStr::new("en_US.UTF-8")));
    }
}
