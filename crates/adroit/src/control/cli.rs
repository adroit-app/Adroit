//! Machine-readable control requests for local agents and integrations.
use std::io::Read;

use adroit_model::{RequestId, SessionRef};

use super::{
    client::Client,
    protocol::{Command, MAX_FRAME},
    store::{ControlError, ErrorCode},
};

const USAGE: &str =
    "usage: adroit control [--request-id UUID] [--actor UUID GENERATION] < command.json";

fn options(args: &[String]) -> Result<(RequestId, Option<SessionRef>), String> {
    let mut request = None;
    let mut actor = None;
    let mut rest = args;
    while let Some((arg, tail)) = rest.split_first() {
        match (arg.as_str(), tail) {
            ("--request-id", [id, remaining @ ..]) if request.is_none() => {
                request = Some(id.parse().map_err(|_| USAGE)?);
                rest = remaining;
            }
            ("--actor", [id, generation, remaining @ ..]) if actor.is_none() => {
                actor = Some(SessionRef {
                    id: id.parse().map_err(|_| USAGE)?,
                    generation: generation.parse().map_err(|_| USAGE)?,
                });
                rest = remaining;
            }
            _ => return Err(USAGE.into()),
        }
    }
    Ok((request.unwrap_or_default(), actor))
}

pub fn cli(args: &[String]) -> i32 {
    if matches!(args, [mode] if mode == "mcp")
        || matches!(args, [mode, channel] if mode == "mcp" && channel == "--channel")
    {
        return super::bridge::cli(args.len() == 2);
    }
    let (request, actor) = match options(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let result = (|| {
        let mut input = Vec::new();
        std::io::stdin()
            .take((MAX_FRAME + 1) as u64)
            .read_to_end(&mut input)
            .map_err(invalid)?;
        if input.len() > MAX_FRAME {
            return Err(invalid("the control command is too large"));
        }
        let command: Command = serde_json::from_slice(&input).map_err(invalid)?;
        let root = super::root();
        let mut client = match std::env::var("ADROIT_CONTROL_TOKEN") {
            Ok(token) => Client::session(&super::session_root(), token),
            Err(std::env::VarError::NotPresent) => Client::human(&root)?,
            Err(_) => return Err(invalid("invalid control credential encoding")),
        };
        if let Some(actor) = actor {
            client = client.as_actor(actor);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(invalid)?;
        runtime.block_on(client.request(request, command))
    })();
    let exit = if result.is_ok() { 0 } else { 1 };
    // Include the ID even on transport failure so callers can safely retry.
    println!(
        "{}",
        serde_json::json!({ "request_id": request, "result": result })
    );
    exit
}

fn invalid(error: impl std::fmt::Display) -> ControlError {
    ControlError::new(ErrorCode::InvalidRequest, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_duplicate_options_are_rejected() {
        for words in [
            vec!["--wait"],
            vec!["--request-id"],
            vec!["--actor", "bad", "1"],
            vec!["--actor", "bad", "-1"],
        ] {
            assert!(options(&words.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
    }

    #[test]
    fn a_request_id_is_preserved_for_retries() {
        let id = RequestId::new();
        let args = vec!["--request-id".into(), id.to_string()];
        assert_eq!(options(&args).unwrap(), (id, None));
        assert!(options(&[args.clone(), args].concat()).is_err());
    }
}
