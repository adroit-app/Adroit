//! How the values that do not go on the wire as they are written go: a
//! terminal's 128-bit id, which a JSON number does not hold and which the
//! terminal leaves out of its own JSON, since `adroit terminal list` shows
//! its resource id alone.

use std::collections::BTreeMap;

use adroit_model::TerminalSession;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A 128-bit id as a decimal string.
pub(crate) mod wide {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(id: &u128, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(&id.to_string())
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<u128, D::Error> {
        String::deserialize(input)?
            .parse()
            .map_err(D::Error::custom)
    }
}

/// A map keyed by 128-bit ids, each key a decimal string.
pub(crate) mod wide_keys {
    use super::*;

    pub(crate) fn serialize<S: Serializer, V: Serialize>(
        map: &BTreeMap<u128, V>,
        out: S,
    ) -> Result<S::Ok, S::Error> {
        out.collect_map(map.iter().map(|(k, v)| (k.to_string(), v)))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
        input: D,
    ) -> Result<BTreeMap<u128, V>, D::Error> {
        BTreeMap::<String, V>::deserialize(input)?
            .into_iter()
            .map(|(k, v)| Ok((k.parse().map_err(D::Error::custom)?, v)))
            .collect()
    }
}

/// A terminal with its id.
#[derive(Serialize, Deserialize)]
struct Terminal {
    #[serde(with = "wide")]
    id: u128,
    #[serde(flatten)]
    terminal: TerminalSession,
}

impl From<&TerminalSession> for Terminal {
    fn from(terminal: &TerminalSession) -> Self {
        Terminal {
            id: terminal.id,
            terminal: terminal.clone(),
        }
    }
}

impl From<Terminal> for TerminalSession {
    fn from(wire: Terminal) -> Self {
        TerminalSession {
            id: wire.id,
            ..wire.terminal
        }
    }
}

/// One terminal, with its id.
pub(crate) mod terminal {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        terminal: &TerminalSession,
        out: S,
    ) -> Result<S::Ok, S::Error> {
        Terminal::from(terminal).serialize(out)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        input: D,
    ) -> Result<TerminalSession, D::Error> {
        Terminal::deserialize(input).map(TerminalSession::from)
    }
}

/// Terminals, each with its id.
pub(crate) mod terminals {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        terminals: &[TerminalSession],
        out: S,
    ) -> Result<S::Ok, S::Error> {
        out.collect_seq(terminals.iter().map(Terminal::from))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        input: D,
    ) -> Result<Vec<TerminalSession>, D::Error> {
        Ok(Vec::<Terminal>::deserialize(input)?
            .into_iter()
            .map(TerminalSession::from)
            .collect())
    }
}
