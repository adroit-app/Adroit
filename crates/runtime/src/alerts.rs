//! Telling the user, outside the window, that a session waits on them.

/// The AppleScript that shows a notification, its words quoted so none can
/// end the string they are in.
fn script(title: &str, subtitle: &str, body: &str) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    format!(
        "display notification {} with title {} subtitle {}",
        q(body),
        q(title),
        q(subtitle)
    )
}

/// Show a notification that the session `name` waits, for `why`, once the
/// future it gives is run.
pub fn notify_waiting(name: &str, why: &str) -> impl std::future::Future<Output = ()> + use<> {
    let mut cmd = tokio::process::Command::new("osascript");
    cmd.args(["-e", &script("Adroit", name, why)])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    async move {
        let _ = cmd.status().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notification_s_words_cannot_break_out_of_the_script() {
        assert_eq!(
            script("Adroit", "say \"hi\"", "a\\b"),
            r#"display notification "a\\b" with title "Adroit" subtitle "say \"hi\"""#
        );
    }
}
