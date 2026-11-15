//! The short code a session keeps for as long as Adroit keeps its record.
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct SessionCode(String);

impl SessionCode {
    pub const LEN: usize = 4;
    /// The digits and lower-case letters, less those read as one another:
    /// `0` and `o`, `1`, `i` and `l`.
    pub const ALPHABET: &'static [u8] = b"23456789abcdefghjkmnpqrstuvwxyz";

    /// A code drawn at random; whether another session has it is the
    /// caller's to check.
    pub fn random() -> Self {
        let bytes = uuid::Uuid::new_v4().into_bytes();
        let alphabet = Self::ALPHABET;
        Self(
            bytes[..Self::LEN]
                .iter()
                .map(|b| alphabet[usize::from(*b) % alphabet.len()] as char)
                .collect(),
        )
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a session code")]
pub struct NotACode(String);

impl FromStr for SessionCode {
    type Err = NotACode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid = value.len() == Self::LEN && value.bytes().all(|b| Self::ALPHABET.contains(&b));
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(NotACode(value.to_owned()))
        }
    }
}

impl TryFrom<String> for SessionCode {
    type Error = NotACode;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<SessionCode> for String {
    fn from(code: SessionCode) -> Self {
        code.0
    }
}

impl fmt::Display for SessionCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_random_code_reads_back_as_a_code() {
        for _ in 0..1000 {
            let code = SessionCode::random();
            assert_eq!(code.as_str().parse::<SessionCode>(), Ok(code));
        }
    }

    #[test]
    fn a_code_holds_no_character_read_as_another() {
        for c in ["0", "o", "1", "i", "l"] {
            assert!(!SessionCode::ALPHABET.contains(&c.as_bytes()[0]), "{c}");
        }
    }

    #[test]
    fn neither_a_uuid_nor_a_name_nor_upper_case_reads_as_a_code() {
        for value in [
            "09dbc688-52b9-4672-93e0-e1e13948293e",
            "fix",
            "fixes",
            "K7F3",
            "k0f3",
            "",
        ] {
            assert!(value.parse::<SessionCode>().is_err(), "{value}");
        }
        assert!("k7f3".parse::<SessionCode>().is_ok());
    }

    #[test]
    fn a_code_is_a_plain_string_in_json() {
        let code: SessionCode = "k7f3".parse().unwrap();
        assert_eq!(serde_json::to_value(&code).unwrap(), "k7f3");
        assert!(serde_json::from_value::<SessionCode>("K7F3".into()).is_err());
    }
}
