//! Exact intents saved before a remote change can be written to a socket.

use adroit_protocol::{MutationStamp, MutationState, Request, is_generation};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const INPUT_LIMIT: u64 = 2 * 1024 * 1024;
const TOTAL_LIMIT: usize = 16 * 1024 * 1024;
const OUTCOME_LIMIT: usize = 65536 + 128;

fn retained(record: &Record) -> Result<usize, String> {
    Ok(serde_json::to_vec(&record.intent)
        .map_err(|e| e.to_string())?
        .len()
        + OUTCOME_LIMIT
        + 128)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub stamp: MutationStamp,
    pub request: Request,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<PromptContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptTarget {
    Nothing,
    Session {
        id: String,
    },
    Draft {
        worktree: PathBuf,
        id: adroit_model::DraftId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptContext {
    pub target: PromptTarget,
    pub text: String,
    pub images: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub intent: Intent,
    pub outcome: Option<MutationState>,
}

impl Intent {
    pub fn new(
        info: &adroit_protocol::RemoteInfo,
        epoch: u64,
        request: Request,
    ) -> Result<Self, String> {
        info.validate()?;
        let mut random = [0; 16];
        File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut random))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            stamp: MutationStamp {
                instance: info.instance.clone(),
                work: info.work.generation.clone(),
                terminal: info.terminal.generation.clone(),
                epoch,
                id: random.iter().map(|b| format!("{b:02x}")).collect(),
            },
            request,
            prompt: None,
        })
    }

    pub fn apply(&self) -> adroit_protocol::Ask {
        adroit_protocol::Ask::Apply {
            stamp: self.stamp.clone(),
            request: Box::new(self.request.clone()),
        }
    }
    pub fn query(&self) -> adroit_protocol::Ask {
        adroit_protocol::Ask::MutationStatus {
            stamp: self.stamp.clone(),
        }
    }
}

pub struct Journal {
    root: PathBuf,
    instance: String,
    capacity: usize,
    _owner: File,
}

fn private(metadata: &std::fs::Metadata) -> Result<(), String> {
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
        return Err("mutation journal must be private and owned by this user".into());
    }
    Ok(())
}

fn directory(path: &Path) -> Result<(), String> {
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {
            // The mode given to mkdir is narrowed by the umask, which another
            // thread binding a socket may have narrowed for a moment.
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            if let Some(parent) = path.parent() {
                File::open(parent)
                    .and_then(|parent| parent.sync_all())
                    .map_err(|e| e.to_string())?;
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_dir() {
        return Err("mutation journal directory must not be a symlink".into());
    }
    private(&metadata)
}

impl Journal {
    pub fn instance(&self) -> &str {
        &self.instance
    }
    pub fn open(root: &Path, instance: &str) -> Result<Self, String> {
        if !is_generation(instance) {
            return Err("invalid journal instance".into());
        }
        directory(root)?;
        let root = root.join(instance);
        directory(&root)?;
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(root.join("owner.lock"))
            .map_err(|e| e.to_string())?;
        let metadata = owner.metadata().map_err(|e| e.to_string())?;
        private(&metadata)?;
        if !metadata.is_file() || metadata.nlink() != 1 {
            return Err("invalid journal ownership file".into());
        }
        let until = std::time::Instant::now() + std::time::Duration::from_millis(250);
        loop {
            match owner.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < until => {
                    // A concurrent fork may still hold the closing owner's
                    // descriptor until exec applies close-on-exec.
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => {
                    return Err("another client already owns this remote mutation journal".into());
                }
            }
        }
        let journal = Self {
            root,
            instance: instance.into(),
            capacity: 256,
            _owner: owner,
        };
        journal.pending()?;
        Ok(journal)
    }

    fn record(&self, stamp: &MutationStamp) -> Result<PathBuf, String> {
        if stamp.instance != self.instance
            || !is_generation(&stamp.work)
            || !is_generation(&stamp.terminal)
            || !is_generation(&stamp.id)
            || stamp.epoch == 0
        {
            return Err("invalid mutation journal identity".into());
        }
        Ok(self.root.join(format!("{}.json", stamp.id)))
    }

    fn read(&self, path: &Path) -> Result<Record, String> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        private(&metadata)?;
        if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > INPUT_LIMIT {
            return Err("invalid mutation journal record".into());
        }
        let mut bytes = Vec::new();
        file.take(INPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > INPUT_LIMIT {
            return Err("mutation journal record exceeds its limit".into());
        }
        let record: Record = serde_json::from_slice(&bytes)
            .map_err(|e| format!("cannot recover mutation journal: {e}"))?;
        let intent = &record.intent;
        let pending = self.record(&intent.stamp)?;
        if (pending != path && pending.with_extension("draft") != path) || !intent.request.mutates()
        {
            return Err("mutation journal payload does not match its identity".into());
        }
        Ok(record)
    }

    pub fn pending(&self) -> Result<Vec<Intent>, String> {
        Ok(self.records()?.into_iter().map(|r| r.intent).collect())
    }

    pub fn records(&self) -> Result<Vec<Record>, String> {
        self.records_in(Some("json"))
    }

    pub fn drafts(&self) -> Result<Vec<Record>, String> {
        self.records_in(Some("draft"))
    }

    fn records_in(&self, extension: Option<&str>) -> Result<Vec<Record>, String> {
        let mut pending = Vec::new();
        let mut total = 0;
        let mut count = 0;
        for entry in std::fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path
                .extension()
                .is_some_and(|e| e == "json" || e == "draft")
            {
                let record = self.read(&path)?;
                total += retained(&record)?;
                count += 1;
                if extension.is_none_or(|e| path.extension().is_some_and(|got| got == e)) {
                    pending.push(record);
                }
                if count > self.capacity || total > TOTAL_LIMIT {
                    return Err("mutation journal is full; resolve pending outcomes first".into());
                }
            }
        }
        pending.sort_by(|a, b| a.intent.stamp.id.cmp(&b.intent.stamp.id));
        Ok(pending)
    }

    pub fn prepare(&self, intent: &Intent) -> Result<(), String> {
        let path = self.record(&intent.stamp)?;
        if !intent.request.mutates() {
            return Err("only work mutations belong in the journal".into());
        }
        let record = Record {
            intent: intent.clone(),
            outcome: None,
        };
        let bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        if bytes.len() + OUTCOME_LIMIT > INPUT_LIMIT as usize
            || intent.prompt.as_ref().is_some_and(|p| {
                serde_json::to_vec(p).map_or(true, |bytes| bytes.len() > 512 * 1024)
            })
        {
            return Err("pending prompt exceeds the journal limit".into());
        }
        if serde_json::to_vec(&intent.request)
            .map_err(|e| e.to_string())?
            .len()
            > 1024 * 1024
        {
            return Err("mutation journal record exceeds its limit".into());
        }
        let pending = self.records_in(None)?;
        if let Some(previous) = pending
            .iter()
            .find(|p| p.intent.stamp.id == intent.stamp.id)
        {
            return if previous.intent == *intent && path.exists() {
                Ok(())
            } else {
                Err("mutation ID already has another identity or payload".into())
            };
        }
        let total = pending
            .iter()
            .try_fold(0, |total, record| retained(record).map(|n| total + n))?;
        if pending.len() >= self.capacity || total + retained(&record)? > TOTAL_LIMIT {
            return Err("mutation journal is full; resolve pending outcomes first".into());
        }
        let mut temporary = tempfile::Builder::new()
            .prefix(".intent-")
            .tempfile_in(&self.root)
            .map_err(|e| e.to_string())?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::hard_link(temporary.path(), &path).map_err(|e| e.to_string())?;
        drop(temporary);
        self.sync()
    }

    pub fn finish(&self, stamp: &MutationStamp, state: &MutationState) -> Result<bool, String> {
        let path = self.record(stamp)?;
        let mut record = self.read(&path)?;
        if record.intent.stamp != *stamp {
            return Err("mutation journal identity changed".into());
        }
        if !matches!(
            state,
            MutationState::Result { .. } | MutationState::Rejected { .. }
        ) {
            return Ok(false);
        }
        if let Some(previous) = &record.outcome {
            return if previous == state {
                Ok(true)
            } else {
                Err("mutation outcome changed after it was recorded".into())
            };
        }
        if serde_json::to_vec(state).map_err(|e| e.to_string())?.len() > OUTCOME_LIMIT {
            return Err("mutation outcome exceeds the journal limit".into());
        }
        record.outcome = Some(state.clone());
        let bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > INPUT_LIMIT {
            return Err("mutation outcome exceeds the journal limit".into());
        }
        let mut temporary = tempfile::Builder::new()
            .prefix(".outcome-")
            .tempfile_in(&self.root)
            .map_err(|e| e.to_string())?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        temporary.persist(path).map_err(|e| e.to_string())?;
        self.sync()?;
        Ok(true)
    }

    pub fn acknowledge(&self, stamp: &MutationStamp) -> Result<(), String> {
        let path = self.record(stamp)?;
        let record = self.read(&path)?;
        if record.intent.stamp != *stamp || record.outcome.is_none() {
            return Err("an uncertain mutation cannot be acknowledged as completed".into());
        }
        if matches!(&record.outcome, Some(MutationState::Result { answer }) if matches!(answer.as_ref(), adroit_protocol::Answer::Done | adroit_protocol::Answer::Session { .. }))
        {
            self.forget_matching_drafts(record.intent.prompt.as_ref())?;
        }
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        self.sync()
    }

    pub fn retain_draft(&self, stamp: &MutationStamp, checked: bool) -> Result<(), String> {
        let path = self.record(stamp)?;
        let saved = path.with_extension("draft");
        if !path.exists() && saved.exists() {
            if self.read(&saved)?.intent.stamp == *stamp {
                return Ok(());
            }
            return Err("retained draft identity changed".into());
        }
        let record = self.read(&path)?;
        if record.intent.stamp != *stamp || record.intent.prompt.is_none() {
            return Err("no matching prompt can be retained".into());
        }
        let failed = matches!(&record.outcome, Some(MutationState::Rejected { .. }))
            || matches!(&record.outcome, Some(MutationState::Result { answer }) if matches!(answer.as_ref(), adroit_protocol::Answer::Failed { .. }));
        if !checked && !failed {
            return Err("check the uncertain outcome before retaining its draft".into());
        }
        std::fs::rename(path, saved).map_err(|e| e.to_string())?;
        self.sync()
    }

    pub fn forget_draft(&self, stamp: &MutationStamp) -> Result<(), String> {
        let path = self.record(stamp)?.with_extension("draft");
        if self.read(&path)?.intent.stamp != *stamp {
            return Err("retained draft identity changed".into());
        }
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        self.sync()
    }

    /// Only an explicit user decision after checking the work can forget an
    /// uncertain outcome. This neither changes the work nor resends an ID.
    pub fn discard_after_check(&self, stamp: &MutationStamp) -> Result<(), String> {
        let path = self.record(stamp)?;
        let record = self.read(&path)?;
        if record.intent.stamp != *stamp {
            return Err("mutation journal identity changed".into());
        }
        self.forget_matching_drafts(record.intent.prompt.as_ref())?;
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        self.sync()
    }

    fn forget_matching_drafts(&self, prompt: Option<&PromptContext>) -> Result<(), String> {
        let Some(prompt) = prompt else {
            return Ok(());
        };
        for record in self.drafts()? {
            if record.intent.prompt.as_ref() == Some(prompt) {
                self.forget_draft(&record.intent.stamp)?;
            }
        }
        Ok(())
    }

    fn sync(&self) -> Result<(), String> {
        File::open(&self.root)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    /// Run in a process of its own by the test below, since the umask it
    /// narrows is the whole process's.
    #[test]
    #[ignore]
    fn narrowed_umask_fixture() {
        let Some(root) = std::env::var_os("ADROIT_TEST_JOURNAL_ROOT") else {
            return;
        };
        // SAFETY: umask has no preconditions; this process runs this test alone.
        unsafe { libc::umask(0o177) };
        super::Journal::open(std::path::Path::new(&root), &"a".repeat(32)).unwrap();
    }

    #[test]
    fn a_journal_opens_while_a_socket_bind_has_narrowed_the_umask() {
        let dir = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "journal::tests::narrowed_umask_fixture",
                "--ignored",
                "--quiet",
            ])
            .env("ADROIT_TEST_JOURNAL_ROOT", dir.path().join("journal"))
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn a_closing_owner_can_handoff_the_journal_without_admitting_two_writers() {
        let dir = tempfile::tempdir().unwrap();
        let first = super::Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap();
        let releasing = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            drop(first);
        });
        let second = super::Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap();
        releasing.join().unwrap();
        assert!(super::Journal::open(&dir.path().join("journal"), &"a".repeat(32)).is_err());
        drop(second);
    }
    use super::*;
    use adroit_protocol::{Answer, Request};
    fn intent(n: u64) -> Intent {
        Intent {
            stamp: MutationStamp {
                instance: "a".repeat(32),
                work: "b".repeat(32),
                terminal: "c".repeat(32),
                epoch: 1,
                id: format!("{n:032x}"),
            },
            request: Request::Release { id: n.into() },
            prompt: None,
        }
    }

    #[test]
    fn a_restored_unsent_draft_keeps_its_durable_copy_after_pending_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("journal");
        let journal = Journal::open(&root, &"a".repeat(32)).unwrap();
        let mut sent = intent(1);
        sent.prompt = Some(PromptContext {
            target: PromptTarget::Nothing,
            text: "日本語\n\n".into(),
            images: vec![],
        });
        journal.prepare(&sent).unwrap();
        assert!(journal.retain_draft(&sent.stamp, false).is_err());
        journal.retain_draft(&sent.stamp, true).unwrap();
        assert!(journal.pending().unwrap().is_empty());
        drop(journal);
        let journal = Journal::open(&root, &"a".repeat(32)).unwrap();
        assert_eq!(journal.drafts().unwrap()[0].intent.prompt, sent.prompt);
        assert!(journal.prepare(&sent).is_err());
        journal.forget_draft(&sent.stamp).unwrap();
        assert!(journal.drafts().unwrap().is_empty());
    }

    #[test]
    fn an_uncertain_intent_survives_restart_with_its_exact_id_and_payload() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap();
        journal.prepare(&intent(1)).unwrap();
        assert!(
            !journal
                .finish(&intent(1).stamp, &MutationState::Unknown)
                .unwrap()
        );
        drop(journal);
        let journal = Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap();
        assert_eq!(journal.pending().unwrap(), vec![intent(1)]);
        journal.prepare(&intent(1)).unwrap();
        let mut changed = intent(1);
        changed.request = Request::Release { id: 2 };
        assert!(journal.prepare(&changed).is_err());
        assert!(
            journal
                .finish(
                    &intent(1).stamp,
                    &MutationState::Result {
                        answer: Box::new(Answer::Done)
                    }
                )
                .unwrap()
        );
        drop(journal);
        let journal = Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap();
        assert!(matches!(
            journal.records().unwrap()[0].outcome,
            Some(MutationState::Result { .. })
        ));
        journal.acknowledge(&intent(1).stamp).unwrap();
        assert!(journal.pending().unwrap().is_empty());
    }

    #[test]
    fn one_owner_and_bounded_retention_prevent_unbounded_or_competing_writes() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap();
        assert!(Journal::open(&dir.path().join("journal"), &"a".repeat(32)).is_err());
        journal.capacity = 2;
        journal.prepare(&intent(1)).unwrap();
        journal.prepare(&intent(2)).unwrap();
        assert!(journal.prepare(&intent(3)).is_err());
        assert_eq!(journal.pending().unwrap().len(), 2);
    }

    #[test]
    fn corrupt_or_redirected_records_fail_closed_without_overwriting_an_intent() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap();
        journal.prepare(&intent(1)).unwrap();
        let path = journal.record(&intent(1).stamp).unwrap();
        std::fs::write(&path, b"broken").unwrap();
        assert!(journal.pending().is_err());
        assert!(journal.prepare(&intent(2)).is_err());
        std::fs::remove_file(&path).unwrap();
        let outside = dir.path().join("outside");
        std::fs::write(&outside, b"untouched").unwrap();
        symlink(&outside, &path).unwrap();
        assert!(journal.pending().is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"untouched");
    }

    #[test]
    fn the_original_prompt_remains_recoverable_until_the_user_resolves_its_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap();
        let mut pending = intent(7);
        pending.prompt = Some(PromptContext {
            target: PromptTarget::Draft {
                worktree: "/work/project".into(),
                id: adroit_model::DraftId::new(),
            },
            text: "# session name\n日本語の依頼".into(),
            images: vec!["/work/image.png".into()],
        });
        journal.prepare(&pending).unwrap();
        drop(journal);
        let journal = Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap();
        assert_eq!(journal.pending().unwrap(), vec![pending.clone()]);
        assert!(journal.acknowledge(&pending.stamp).is_err());
        assert!(
            !journal
                .finish(&pending.stamp, &MutationState::Unknown)
                .unwrap()
        );
        assert_eq!(journal.pending().unwrap()[0].prompt, pending.prompt);
        journal.discard_after_check(&pending.stamp).unwrap();
        assert!(journal.pending().unwrap().is_empty());
    }
}
