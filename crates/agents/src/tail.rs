//! A file of JSON lines read as it grows, as the agents write their
//! transcripts: each read takes in only what was written since the last.

use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use serde_json::Value;

/// How far a file has been read.
#[derive(Debug, Clone, Default)]
pub(crate) struct Tail {
    read_to: u64,
    /// The file read, by its inode, to tell one put in its place.
    inode: Option<u64>,
    /// The start of a line not written to its end yet.
    rest: Vec<u8>,
}

/// What one read found.
#[derive(Debug, PartialEq)]
pub(crate) enum Tailed {
    /// The file could not be opened; the tail starts over.
    Missing,
    /// The file opened but could not be looked at; the tail is as it was.
    Unreadable,
    /// What was written since the last read: whether the file shrank or was
    /// replaced, and so was read from its start; whether a line was ended;
    /// and the ended lines that are JSON.
    Lines {
        restarted: bool,
        ended: bool,
        values: Vec<Value>,
    },
}

impl Tail {
    /// Whether a file has been opened since the tail last started over.
    pub(crate) fn opened(&self) -> bool {
        self.inode.is_some()
    }

    /// Read on in the file at `path`.
    pub(crate) fn read(&mut self, path: &Path) -> Tailed {
        let Ok(mut file) = std::fs::File::open(path) else {
            *self = Tail::default();
            return Tailed::Missing;
        };
        let Ok(meta) = file.metadata() else {
            return Tailed::Unreadable;
        };
        let restarted =
            meta.len() < self.read_to || self.inode.is_some_and(|ino| ino != meta.ino());
        if restarted {
            *self = Tail::default();
        }
        self.inode = Some(meta.ino());
        let unended = Tailed::Lines {
            restarted,
            ended: false,
            values: Vec::new(),
        };
        let mut bytes = Vec::new();
        if file
            .seek(SeekFrom::Start(self.read_to))
            .and_then(|_| file.take(meta.len() - self.read_to).read_to_end(&mut bytes))
            .is_err()
        {
            return unended;
        }
        self.read_to += bytes.len() as u64;
        self.rest.extend(bytes);
        let Some(end) = self.rest.iter().rposition(|b| *b == b'\n') else {
            return unended;
        };
        let done: Vec<u8> = self.rest.drain(..=end).collect();
        Tailed::Lines {
            restarted,
            ended: true,
            values: String::from_utf8_lossy(&done)
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::append;
    use serde_json::json;

    fn lines(restarted: bool, ended: bool, values: &[Value]) -> Tailed {
        Tailed::Lines {
            restarted,
            ended,
            values: values.to_vec(),
        }
    }

    #[test]
    fn a_file_is_read_on_from_where_it_was_left_a_partial_line_awaiting_its_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        append(&path, "{\"a\":1}\n{\"b\"");
        let mut tail = Tail::default();
        assert_eq!(tail.read(&path), lines(false, true, &[json!({"a": 1})]));
        assert_eq!(tail.read(&path), lines(false, false, &[]));
        append(&path, ":2}\n");
        assert_eq!(tail.read(&path), lines(false, true, &[json!({"b": 2})]));
    }

    #[test]
    fn an_ended_line_that_is_not_json_is_passed_over_but_counts_as_ended() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        append(&path, "not json\n\n");
        assert_eq!(Tail::default().read(&path), lines(false, true, &[]));
    }

    #[test]
    fn a_file_that_shrank_or_was_replaced_is_read_again_from_its_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        append(&path, "{\"a\":1}\n{\"b\":2}\n");
        let mut tail = Tail::default();
        tail.read(&path);
        std::fs::write(&path, "{\"c\":3}\n").unwrap();
        assert_eq!(tail.read(&path), lines(true, true, &[json!({"c": 3})]));
        let other = dir.path().join("other.jsonl");
        append(&other, "{\"c\":3}\n{\"d\":4}\n");
        std::fs::rename(&other, &path).unwrap();
        assert_eq!(
            tail.read(&path),
            lines(true, true, &[json!({"c": 3}), json!({"d": 4})])
        );
    }

    #[test]
    fn a_missing_file_starts_the_tail_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut tail = Tail::default();
        assert_eq!(tail.read(&path), Tailed::Missing);
        assert!(!tail.opened());
        append(&path, "{\"a\":1}\n");
        tail.read(&path);
        assert!(tail.opened());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(tail.read(&path), Tailed::Missing);
        assert!(!tail.opened());
        append(&path, "{\"a\":1}\n");
        assert_eq!(tail.read(&path), lines(false, true, &[json!({"a": 1})]));
    }
}
