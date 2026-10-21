//! Human UI previews; never used as controller authority.
use super::*;
use crate::control::protocol::{
    UiDetailPage, UiPage, UiRecordChunk, UiRecordRef, UiTable, UiTaskHistoryPage,
};

const PAGE_BYTES: usize = 256 * 1024;
const CHUNK_BYTES: usize = 32 * 1024;

// Decide attention against complete history before applying the transport cursor.
// Answer evidence is retained for questions whose delivery still needs attention.
const ATTENTION: &str = "WITH attention AS (
    SELECT m.id FROM messages m WHERE
    NOT EXISTS (SELECT 1 FROM deliveries d WHERE d.message_id=m.id AND d.recipient=m.recipient
        AND json_extract(d.data, '$.state')='acknowledged')
    OR (json_extract(m.data, '$.kind')='question' AND NOT EXISTS (
        SELECT 1 FROM messages a WHERE json_extract(a.data, '$.kind')='answer'
        AND json_extract(a.data, '$.reply_to')=m.id AND a.run_id=m.run_id
        AND a.sender=m.recipient AND a.recipient=m.sender))
), visible AS (
    SELECT m.id, m.rowid AS sequence, m.data FROM messages m WHERE m.id IN (SELECT id FROM attention)
    OR (json_extract(m.data, '$.kind')='answer' AND EXISTS (
        SELECT 1 FROM messages q JOIN attention ON attention.id=q.id
        WHERE json_extract(q.data, '$.kind')='question'
        AND json_extract(m.data, '$.reply_to')=q.id AND m.run_id=q.run_id
        AND m.sender=q.recipient AND m.recipient=q.sender))
) ";

impl UiTable {
    fn table(self) -> &'static str {
        match self {
            Self::Sessions => "logical_sessions",
            Self::Reports => "task_reports",
            Self::Completions => "run_completions",
            Self::Coordination => "coordination_notices",
            _ => self.field(),
        }
    }
}

fn epoch(connection: &Connection, expected: Option<u64>) -> Result<u64> {
    let current = cursor(connection)?;
    if expected.is_some_and(|expected| expected != current) {
        return Err(ControlError::new(
            ErrorCode::RevisionConflict,
            "UI state changed; refresh all pages",
        ));
    }
    Ok(current)
}

// Previews retain routing, identity, state, revisions and nonempty attention.
// Only prose and evidence payloads are abbreviated; detail reads retain originals.
fn preview(table: UiTable, text: &str) -> Result<Value> {
    let mut value: Value = decode(text)?;
    if matches!(table, UiTable::Candidates) {
        let candidate: CandidateRecord = decode(text)?;
        value = serde_json::to_value(candidate.summary())
            .map_err(|error| invalid(error.to_string()))?;
    }
    for path in match table {
        UiTable::Reports => &["/report/result/artifact/data"][..],
        UiTable::Candidates => &["/artifact/data"][..],
        _ => &[],
    } {
        if let Some(data) = value.pointer_mut(path) {
            *data = Value::String("Load full details to read evidence".into());
        }
    }
    fn abbreviate(value: &mut Value, key: &str) {
        match value {
            Value::String(text) if key == "instructions" && text.len() > 1024 => {
                let label = text
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                *text = format!("{label}\n… [preview; load full details]");
            }
            Value::String(text)
                if matches!(
                    key,
                    "instructions"
                        | "body"
                        | "summary"
                        | "goal"
                        | "constraints"
                        | "decisions"
                        | "unresolved"
                        | "last_error"
                        | "error"
                        | "purpose"
                ) =>
            {
                if text.len() > 1024 {
                    let mut end = 1024;
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    text.truncate(end);
                    text.push_str("\n… [preview; load full details]");
                }
            }
            Value::Array(items) => {
                for item in items {
                    abbreviate(item, key);
                }
            }
            Value::Object(fields) => {
                for (key, value) in fields {
                    abbreviate(value, key);
                }
            }
            _ => {}
        }
    }
    abbreviate(&mut value, "");
    Ok(value)
}

fn page(
    connection: &Connection,
    table: UiTable,
    after: u64,
    expected: Option<u64>,
) -> Result<UiPage> {
    if after > 0 && expected.is_none() {
        return Err(invalid("UI continuation requires an epoch"));
    }
    let after: i64 = after
        .try_into()
        .map_err(|_| invalid("UI cursor is too large"))?;
    let epoch = epoch(connection, expected)?;
    let query = match table {
        UiTable::Messages => format!("{ATTENTION} SELECT sequence, data FROM visible WHERE sequence > ?1 ORDER BY sequence LIMIT 65"),
        UiTable::Deliveries => format!("{ATTENTION} SELECT rowid, data FROM deliveries WHERE rowid > ?1 AND message_id IN (SELECT id FROM visible) ORDER BY rowid LIMIT 65"),
        UiTable::Reports => "SELECT r.rowid, r.data FROM task_reports r WHERE r.rowid > ?1 AND EXISTS (SELECT 1 FROM tasks t WHERE json_extract(t.data, '$.current_attempt')=r.attempt_id) ORDER BY r.rowid LIMIT 65".into(),
        _ => format!("SELECT rowid, data FROM {} WHERE rowid > ?1 ORDER BY rowid LIMIT 65", table.table()),
    };
    let mut statement = connection.prepare(&query)?;
    let records = statement.query_map([after], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut rows = vec![];
    let mut bytes = 0;
    let mut last = after as u64;
    let mut next_cursor = None;
    for record in records {
        let (rowid, text) = record?;
        let row = preview(table, &text)?;
        let size = encode(&row)?.len() + 1;
        if size > PAGE_BYTES {
            if rows.is_empty() {
                return Ok(UiPage {
                    epoch,
                    next_cursor: Some(rowid as u64),
                    rows,
                    record: Some(UiRecordRef {
                        rowid: rowid as u64,
                        bytes: (size - 1) as u64,
                    }),
                });
            }
            next_cursor = Some(last);
            break;
        }
        if bytes + size > PAGE_BYTES || rows.len() == 64 {
            next_cursor = Some(last);
            break;
        }
        bytes += size;
        rows.push(row);
        last = rowid as u64;
    }
    Ok(UiPage {
        epoch,
        next_cursor,
        rows,
        record: None,
    })
}

fn record_chunk(
    connection: &Connection,
    table: UiTable,
    rowid: u64,
    offset: u64,
    expected: u64,
) -> Result<UiRecordChunk> {
    let epoch = epoch(connection, Some(expected))?;
    let rowid_sql: i64 = rowid
        .try_into()
        .map_err(|_| invalid("invalid UI row identity"))?;
    let text: String = connection.query_row(
        &format!("SELECT data FROM {} WHERE rowid = ?1", table.table()),
        [rowid_sql],
        |row| row.get(0),
    )?;
    let text = encode(&preview(table, &text)?)?;
    let start: usize = offset
        .try_into()
        .map_err(|_| invalid("invalid UI record offset"))?;
    if start >= text.len() || !text.is_char_boundary(start) {
        return Err(invalid("invalid UI record offset"));
    }
    let mut end = (start + CHUNK_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    // JSON escaping expands each input byte by at most six, leaving ample
    // space for the response envelope and newline below the frame limit.
    Ok(UiRecordChunk {
        epoch,
        rowid,
        offset,
        text: text[start..end].into(),
    })
}

fn detail(
    connection: &Connection,
    table: UiTable,
    id: String,
    offset: u32,
    expected: Option<u64>,
) -> Result<UiDetailPage> {
    if offset > 0 && expected.is_none() {
        return Err(invalid("UI detail continuation requires an epoch"));
    }
    let epoch = epoch(connection, expected)?;
    if id.len() > 128 || matches!(table, UiTable::Deliveries | UiTable::RunWorkAreas) {
        return Err(invalid("invalid UI detail identity"));
    }
    let text: Option<String> = connection
        .query_row(
            &format!("SELECT data FROM {} WHERE id = ?1", table.table()),
            [id],
            |row| row.get(0),
        )
        .optional()?;
    let text =
        text.ok_or_else(|| ControlError::new(ErrorCode::NotFound, "UI detail no longer exists"))?;
    let start = offset as usize;
    if start > text.len() || !text.is_char_boundary(start) {
        return Err(invalid("invalid UI detail offset"));
    }
    let mut end = (start + CHUNK_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(UiDetailPage {
        epoch,
        next_offset: if end < text.len() {
            Some(
                end.try_into()
                    .map_err(|_| invalid("UI detail is too large"))?,
            )
        } else {
            None
        },
        text: text[start..end].into(),
    })
}

impl Store {
    pub fn ui_page(&mut self, table: UiTable, after: u64, expected: Option<u64>) -> Result<UiPage> {
        let transaction = self.connection.transaction()?;
        page(&transaction, table, after, expected)
    }
    pub fn ui_record(
        &mut self,
        table: UiTable,
        rowid: u64,
        offset: u64,
        expected: u64,
    ) -> Result<UiRecordChunk> {
        let transaction = self.connection.transaction()?;
        record_chunk(&transaction, table, rowid, offset, expected)
    }
    pub fn ui_detail(
        &mut self,
        table: UiTable,
        id: String,
        offset: u32,
        expected: Option<u64>,
    ) -> Result<UiDetailPage> {
        let transaction = self.connection.transaction()?;
        detail(&transaction, table, id, offset, expected)
    }
}

fn private(path: &Path, directory: bool) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| ControlError::new(ErrorCode::StorageFailure, error.to_string()))?;
    // SAFETY: geteuid only reads the process identity.
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err(ControlError::new(
            ErrorCode::OwnershipMismatch,
            "UI compatibility reads require private owned control files",
        ));
    }
    Ok(())
}

fn read_only(root: &Path, credential: &str) -> Result<Connection> {
    private(root, true)?;
    private(&root.join("human.token"), false)?;
    private(&root.join("state.sqlite3"), false)?;
    let token = std::fs::read_to_string(root.join("human.token"))
        .map_err(|error| ControlError::new(ErrorCode::StorageFailure, error.to_string()))?;
    if credential.len() != 72 || token != credential {
        return Err(ControlError::new(
            ErrorCode::OwnershipMismatch,
            "UI compatibility reads require the current human credential",
        ));
    }
    let connection = Connection::open_with_flags(
        root.join("state.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(Duration::from_millis(250))?;
    connection.pragma_update(None, "query_only", true)?;
    Ok(connection)
}

fn schema(connection: &Connection) -> Result<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(ControlError::new(
            ErrorCode::UnsupportedSchema,
            format!("UI compatibility reads require schema {SCHEMA_VERSION}; found {version}"),
        ));
    }
    Ok(())
}

pub fn read_only_snapshot(root: &Path, credential: &str) -> Result<ControlSnapshot> {
    let mut connection = read_only(root, credential)?;
    let transaction = connection.transaction()?;
    schema(&transaction)?;
    let epoch = cursor(&transaction)?;
    let mut snapshot = serde_json::to_value(ControlSnapshot::default())
        .map_err(|error| invalid(error.to_string()))?;
    snapshot["event_cursor"] = epoch.into();
    for table in UiTable::ALL {
        let mut after = 0;
        loop {
            let mut page = page(&transaction, table, after, Some(epoch))?;
            if let Some(reference) = page.record.take() {
                reference.validate_page(after, &page)?;
                let mut record = crate::control::protocol::UiRecordReader::new(reference, epoch);
                while !record.complete() {
                    record.push(record_chunk(
                        &transaction,
                        table,
                        record.reference.rowid,
                        record.offset(),
                        epoch,
                    )?)?;
                }
                page.rows.push(record.finish()?);
            }
            snapshot[table.field()]
                .as_array_mut()
                .expect("snapshot array")
                .extend(page.rows);
            match page.next_cursor {
                Some(next) => after = next,
                None => break,
            }
        }
    }
    serde_json::from_value(snapshot)
        .map_err(|error| ControlError::new(ErrorCode::CorruptStore, error.to_string()))
}

pub fn read_only_detail(
    root: &Path,
    credential: &str,
    table: UiTable,
    id: String,
) -> Result<String> {
    let mut connection = read_only(root, credential)?;
    let transaction = connection.transaction()?;
    schema(&transaction)?;
    let mut text = String::new();
    let mut offset = 0;
    let mut expected = None;
    loop {
        let page = detail(&transaction, table, id.clone(), offset, expected)?;
        expected = Some(page.epoch);
        text.push_str(&page.text);
        match page.next_offset {
            Some(next) => offset = next,
            None => return Ok(text),
        }
    }
}

fn task_history(
    connection: &Connection,
    task_id: TaskId,
    after: u64,
    expected: Option<u64>,
) -> Result<UiTaskHistoryPage> {
    if after > 0 && expected.is_none() {
        return Err(invalid("UI history continuation requires an epoch"));
    }
    let epoch = epoch(connection, expected)?;
    let after: i64 = after
        .try_into()
        .map_err(|_| invalid("invalid UI history cursor"))?;
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
        [task_id.to_string()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(ControlError::new(
            ErrorCode::NotFound,
            "UI history task no longer exists",
        ));
    }
    let mut statement = connection.prepare(
        "SELECT a.rowid, json_object(
        'attempt_id', a.id, 'task_revision', json_extract(a.data, '$.task_revision'),
        'session_id', a.session_id, 'state', json_extract(a.data, '$.state'),
        'report', CASE WHEN r.id IS NULL THEN NULL ELSE json_object(
            'id', r.id, 'task_revision', json_extract(r.data, '$.report.task.revision'),
            'applied', json_extract(r.data, '$.applied'),
            'status', json_extract(r.data, '$.report.result.status')) END)
        FROM attempts a LEFT JOIN task_reports r ON r.attempt_id=a.id
        WHERE a.task_id=?1 AND a.rowid>?2 ORDER BY a.rowid LIMIT 65",
    )?;
    let mut entries = vec![];
    let mut last = after as u64;
    let mut next_cursor = None;
    let records = statement.query_map(rusqlite::params![task_id.to_string(), after], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for record in records {
        let (rowid, text) = record?;
        if entries.len() == 64 {
            next_cursor = Some(last);
            break;
        }
        // SQLite JSON booleans are integers when extracted into json_object.
        let mut value: Value = decode(&text)?;
        if let Some(applied) = value.pointer_mut("/report/applied") {
            *applied = Value::Bool(applied.as_i64() == Some(1));
        }
        entries.push(serde_json::from_value(value).map_err(|error| invalid(error.to_string()))?);
        last = rowid as u64;
    }
    Ok(UiTaskHistoryPage {
        epoch,
        task_id,
        next_cursor,
        entries,
    })
}

impl Store {
    pub fn ui_task_history(
        &mut self,
        task_id: TaskId,
        after: u64,
        expected: Option<u64>,
    ) -> Result<UiTaskHistoryPage> {
        let transaction = self.connection.transaction()?;
        task_history(&transaction, task_id, after, expected)
    }
}

pub fn read_only_task_history(
    root: &Path,
    credential: &str,
    task_id: TaskId,
    expected: Option<u64>,
) -> Result<Vec<crate::control::protocol::UiAttemptHistory>> {
    let mut connection = read_only(root, credential)?;
    let transaction = connection.transaction()?;
    schema(&transaction)?;
    let epoch = epoch(&transaction, expected)?;
    let mut entries = vec![];
    let mut after = 0;
    loop {
        let page = task_history(&transaction, task_id, after, Some(epoch))?;
        entries.extend(page.entries);
        match page.next_cursor {
            Some(next) => after = next,
            None => return Ok(entries),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn compatibility_transaction_keeps_one_epoch_while_the_daemon_writes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("control");
        let mut writer = Store::open(&root.join("state.sqlite3")).unwrap();
        let token = "a".repeat(72);
        std::fs::write(root.join("human.token"), &token).unwrap();
        std::fs::set_permissions(
            root.join("human.token"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        writer
            .register_leader(
                RequestId::new(),
                LogicalSessionId::new(),
                Agent::Codex,
                "/tmp".into(),
            )
            .unwrap();
        let mut reader = read_only(&root, &token).unwrap();
        let transaction = reader.transaction().unwrap();
        schema(&transaction).unwrap();
        let initial = page(&transaction, UiTable::Sessions, 0, None).unwrap();
        assert_eq!(initial.rows.len(), 1);
        writer
            .register_leader(
                RequestId::new(),
                LogicalSessionId::new(),
                Agent::Codex,
                "/tmp".into(),
            )
            .unwrap();
        let same = page(&transaction, UiTable::Sessions, 0, Some(initial.epoch)).unwrap();
        assert_eq!(same.rows.len(), 1);
        assert!(
            transaction
                .execute("DELETE FROM logical_sessions", [])
                .is_err()
        );
        drop(transaction);
        assert_eq!(read_only_snapshot(&root, &token).unwrap().sessions.len(), 2);
    }
}
