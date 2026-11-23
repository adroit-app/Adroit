//! `adroit session resume` and `open`: a session that ended run on, and one
//! that runs put in front of the user.

use adroit_model::SessionId;
use adroit_term::tags;
use serde_json::{Value, json};

use super::{Found, Standing, ended, find, natives, records};
use crate::delegation::{Session as Record, app};

/// Why the session of `record` cannot be run on; `held` being the sessions
/// that run and `there` whether its directory is.
fn refusal(record: &Record, held: &[adroit_model::Session], there: bool) -> Option<String> {
    let conversation = record.conversation.as_deref().filter(|c| !c.is_empty());
    let Some(conversation) = conversation else {
        return Some(format!(
            "{} never told which conversation it held",
            record.agent.label()
        ));
    };
    if !there {
        return Some(format!("{} is not there any more", record.cwd.display()));
    }
    let runs = held
        .iter()
        .find(|s| s.agent == record.agent && s.session_id == conversation)?;
    Some(format!("its conversation runs already, as {}", runs.name))
}

/// Runs the ended session `asked` names on, and asks it for `request`, from
/// the program of that name, when one is given.
pub(super) async fn resume(
    asked: &str,
    request: Option<(String, String)>,
) -> Result<Value, String> {
    let _turn = crate::restore::ONE_AT_A_TIME.lock().await;
    let (natives, records) = (natives().await?, records().await?);
    if matches!(find(asked).await?.standing, Standing::Runs(_)) {
        return Err(format!("{asked} runs; ask it with send"));
    }
    let record = ended(&records, asked).ok_or_else(|| format!("no session is named {asked}"))?;
    let held: Vec<_> = crate::sessions::agents(&natives).collect();
    if let Some(why) = refusal(record, &held, record.cwd.is_dir()) {
        return Err(why);
    }
    let resumed = crate::restore::again(record, true).await?;
    if let Some((from, prompt)) = request {
        let id = resumed;
        app::with_store(&app::root(), move |store| {
            store.ask_from_outside(&from, id, &prompt)
        })
        .await
        .map_err(|e| format!("the session was resumed, and not asked: {e}"))?;
    }
    told(resumed).await
}

async fn told(id: SessionId) -> Result<Value, String> {
    let record = app::with_store(&app::root(), move |store| store.known(id))
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({"id": record.id, "code": record.code, "name": record.name, "dir": record.cwd}))
}

/// Opens the running session `asked` names in a tab of each window, behind the
/// one shown.
pub(super) async fn open(asked: &str) -> Result<Value, String> {
    let found = find(asked).await?;
    let terminal = running(&found)?;
    let native = natives()
        .await?
        .into_iter()
        .find(|n| n.id == terminal)
        .ok_or("the session has no terminal")?;
    let at = adroit_model::time::now_ms().to_string();
    crate::daemon::daemon()
        .tag(
            terminal,
            tags::tagged(crate::sessions::OPENED, &at),
            tags::holding(&native.tags, crate::sessions::OPENED),
        )
        .await
        .map_err(|e| e.to_string())?;
    let listed = found.listed(None);
    Ok(json!({"id": listed.id, "code": listed.code, "name": listed.name}))
}

fn running(found: &Found) -> Result<u128, String> {
    match found.standing {
        Standing::Ended => Err("the session has ended; resume it first".into()),
        Standing::Runs(_) => crate::sessions::id_of(&found.session.id)
            .ok_or_else(|| "the session has no terminal".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::record;

    #[test]
    fn a_session_is_run_on_from_a_conversation_it_told_in_a_directory_still_there() {
        let told = record("a").conversation("c1").ended().build();
        assert_eq!(refusal(&told, &[], true), None);
        assert!(refusal(&told, &[], false).unwrap().contains("not there"));
        let silent = record("a").ended().build();
        assert!(refusal(&silent, &[], true).unwrap().contains("never told"));
    }

    #[test]
    fn a_conversation_that_runs_is_not_resumed_a_second_time() {
        let told = record("a").conversation("c1").ended().build();
        let runs = |conversation: &str| adroit_model::Session {
            agent: told.agent,
            session_id: conversation.into(),
            name: "b".into(),
            ..Default::default()
        };
        assert!(
            refusal(&told, &[runs("c1")], true)
                .unwrap()
                .contains("as b")
        );
        assert_eq!(refusal(&told, &[runs("c2")], true), None);
    }
}
