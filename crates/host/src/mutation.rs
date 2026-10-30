//! Generation-scoped admission, with bounded retention and execution waits.

use adroit_protocol::{Answer, MutationStamp, MutationState, RemoteEndpoint, Request};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Entry {
    canonical: Vec<u8>,
    state: MutationState,
    /// Whether its change can still take effect, which an unknown state
    /// alone does not tell once its caller stopped waiting.
    running: bool,
}

struct Ledger {
    epoch: u64,
    capacity: usize,
    entries: HashMap<String, Entry>,
}

#[derive(Debug, PartialEq)]
enum Admission {
    New,
    Known(MutationState),
}

impl Ledger {
    fn new(capacity: usize) -> Self {
        Self {
            epoch: 1,
            capacity,
            entries: HashMap::new(),
        }
    }
    fn epoch(&self) -> u64 {
        self.epoch
    }
    fn admit(&mut self, epoch: u64, id: &str, canonical: &[u8]) -> Admission {
        if epoch != self.epoch {
            return Admission::Known(MutationState::Retired);
        }
        if let Some(entry) = self.entries.get(id) {
            return Admission::Known(if entry.canonical == canonical {
                entry.state.clone()
            } else {
                MutationState::Conflict
            });
        }
        if self.entries.len() >= self.capacity {
            return Admission::Known(MutationState::Full);
        }
        self.entries.insert(
            id.into(),
            Entry {
                canonical: canonical.into(),
                state: MutationState::Accepted,
                running: true,
            },
        );
        Admission::New
    }
    fn query(&self, epoch: u64, id: &str) -> MutationState {
        if epoch != self.epoch {
            return MutationState::Unknown;
        }
        self.entries
            .get(id)
            .map_or(MutationState::Unknown, |entry| entry.state.clone())
    }
    /// Its caller stopped waiting while the change may still take effect.
    fn waited(&mut self, epoch: u64, id: &str) {
        if epoch == self.epoch
            && let Some(entry) = self.entries.get_mut(id)
        {
            entry.state = MutationState::Unknown;
        }
    }
    fn unknown(&mut self, epoch: u64, id: &str) {
        self.settle(epoch, id, MutationState::Unknown);
    }
    fn refuse(&mut self, epoch: u64, id: &str, text: &str) -> MutationState {
        let state = MutationState::Rejected { text: text.into() };
        self.settle(epoch, id, state.clone());
        state
    }
    fn finish(&mut self, epoch: u64, id: &str, answer: Answer) {
        let state = if serde_json::to_vec(&answer).is_ok_and(|bytes| bytes.len() <= 65536) {
            MutationState::Result {
                answer: Box::new(answer),
            }
        } else {
            MutationState::Unknown
        };
        self.settle(epoch, id, state);
    }
    fn settle(&mut self, epoch: u64, id: &str, state: MutationState) {
        if epoch != self.epoch {
            return;
        }
        if let Some(entry) = self.entries.get_mut(id) {
            entry.state = state;
            entry.running = false;
        }
        self.retire_completed();
    }

    fn retire_completed(&mut self) {
        if self.entries.len() >= self.capacity && self.entries.values().all(|entry| !entry.running)
        {
            self.entries.clear();
            self.epoch += 1;
        }
    }
}

#[derive(Clone)]
pub(crate) struct Service {
    ledger: Arc<Mutex<Ledger>>,
    dispatch: Arc<tokio::sync::Mutex<()>>,
}

impl Default for Service {
    fn default() -> Self {
        Self {
            ledger: Arc::new(Mutex::new(Ledger::new(256))),
            dispatch: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}

impl Service {
    pub fn epoch(&self) -> u64 {
        self.ledger.lock().unwrap().epoch()
    }
    fn matches(stamp: &MutationStamp, work: &RemoteEndpoint, terminal: &RemoteEndpoint) -> bool {
        stamp.instance == work.instance
            && stamp.work == work.generation
            && stamp.terminal == terminal.generation
    }
    pub fn query(&self, stamp: &MutationStamp) -> MutationState {
        let (Some(work), Some(terminal)) = (crate::remote::ours(), crate::remote::terminal())
        else {
            return MutationState::Unknown;
        };
        if !Self::matches(stamp, work, terminal) {
            return MutationState::Unknown;
        }
        self.ledger.lock().unwrap().query(stamp.epoch, &stamp.id)
    }
    pub async fn apply<F, Fut>(
        &self,
        stamp: MutationStamp,
        request: Request,
        run: F,
    ) -> MutationState
    where
        F: FnOnce(Request) -> Fut + Send + 'static,
        Fut: Future<Output = Answer> + Send + 'static,
    {
        let (Some(work), Some(terminal)) = (crate::remote::ours(), crate::remote::terminal())
        else {
            return MutationState::Rejected {
                text: "remote mutation identity is unavailable".into(),
            };
        };
        if !Self::matches(&stamp, work, terminal) {
            return MutationState::Unknown;
        }
        if !adroit_protocol::is_generation(&stamp.id) || !request.mutates() {
            return MutationState::Rejected {
                text: "invalid mutation ID or nested/read-only payload".into(),
            };
        }
        let canonical = match serde_json::to_vec(&request) {
            Ok(bytes) if bytes.len() <= 1048576 => bytes,
            _ => {
                return MutationState::Rejected {
                    text: "mutation exceeds its input limit".into(),
                };
            }
        };
        self.execute(
            stamp.epoch,
            stamp.id,
            canonical,
            request,
            run,
            Duration::from_secs(15),
        )
        .await
    }
    async fn execute<F, Fut>(
        &self,
        epoch: u64,
        id: String,
        canonical: Vec<u8>,
        request: Request,
        run: F,
        deadline: Duration,
    ) -> MutationState
    where
        F: FnOnce(Request) -> Fut + Send + 'static,
        Fut: Future<Output = Answer> + Send + 'static,
    {
        match self.ledger.lock().unwrap().admit(epoch, &id, &canonical) {
            Admission::New => {}
            Admission::Known(state) => return state,
        }
        let guard = match tokio::time::timeout(
            Duration::from_secs(5).min(deadline),
            self.dispatch.clone().lock_owned(),
        )
        .await
        {
            Ok(guard) => guard,
            Err(_) => {
                return self.ledger.lock().unwrap().refuse(
                    epoch,
                    &id,
                    "another remote change is still running; this one was not applied",
                );
            }
        };
        let service = self.clone();
        let key = id.clone();
        let mut task = crate::environment::spawn(async move {
            let _guard = guard;
            let answer = match tokio::time::timeout(deadline.saturating_mul(2), run(request)).await
            {
                Ok(answer) => answer,
                Err(_) => {
                    service.ledger.lock().unwrap().unknown(epoch, &key);
                    return None;
                }
            };
            if matches!(answer, Answer::Unconfirmed { .. }) {
                service.ledger.lock().unwrap().unknown(epoch, &key);
                return None;
            }
            service
                .ledger
                .lock()
                .unwrap()
                .finish(epoch, &key, answer.clone());
            Some(answer)
        });
        match tokio::time::timeout(deadline, &mut task).await {
            Ok(Ok(Some(answer))) => MutationState::Result {
                answer: Box::new(answer),
            },
            Ok(Ok(None)) => MutationState::Unknown,
            Ok(Err(_)) => {
                self.ledger.lock().unwrap().unknown(epoch, &id);
                MutationState::Unknown
            }
            Err(_) => {
                self.ledger.lock().unwrap().waited(epoch, &id);
                let service = self.clone();
                crate::environment::spawn(async move {
                    // A task that ended without settling its entry panicked.
                    if task.await.is_err() {
                        service.ledger.lock().unwrap().unknown(epoch, &id);
                    }
                });
                MutationState::Unknown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_protocol::{Answer, MutationState};

    #[test]
    fn an_id_is_admitted_once_and_cannot_change_its_payload() {
        let mut ledger = Ledger::new(4);
        let epoch = ledger.epoch();
        assert_eq!(ledger.admit(epoch, "one", b"start"), Admission::New);
        assert_eq!(
            ledger.admit(epoch, "one", b"start"),
            Admission::Known(MutationState::Accepted)
        );
        assert_eq!(
            ledger.admit(epoch, "one", b"stop"),
            Admission::Known(MutationState::Conflict)
        );
        ledger.finish(epoch, "one", Answer::Done);
        assert_eq!(
            ledger.admit(epoch, "one", b"start"),
            Admission::Known(MutationState::Result {
                answer: Box::new(Answer::Done)
            })
        );
    }

    #[test]
    fn more_than_4096_completed_operations_do_not_exhaust_or_replay_retired_ids() {
        let mut ledger = Ledger::new(4096);
        for i in 0..10000 {
            let epoch = ledger.epoch();
            let id = format!("{i:032x}");
            assert_eq!(ledger.admit(epoch, &id, b"effect"), Admission::New);
            ledger.finish(epoch, &id, Answer::Done);
        }
        assert!(ledger.epoch() > 1);
        assert_eq!(
            ledger.admit(1, &format!("{:032x}", 0), b"effect"),
            Admission::Known(MutationState::Retired)
        );
        assert_eq!(
            ledger.query(1, &format!("{:032x}", 0)),
            MutationState::Unknown
        );
    }

    #[test]
    fn a_timed_out_operation_is_not_run_again_and_can_report_a_late_result() {
        let mut ledger = Ledger::new(2);
        let epoch = ledger.epoch();
        assert_eq!(ledger.admit(epoch, "one", b"effect"), Admission::New);
        ledger.unknown(epoch, "one");
        assert_eq!(
            ledger.admit(epoch, "one", b"effect"),
            Admission::Known(MutationState::Unknown)
        );
        ledger.finish(epoch, "one", Answer::Done);
        assert_eq!(
            ledger.query(epoch, "one"),
            MutationState::Result {
                answer: Box::new(Answer::Done)
            }
        );
    }

    #[test]
    fn a_full_epoch_of_unknown_outcomes_can_retire_without_readmitting_old_ids() {
        let mut ledger = Ledger::new(2);
        assert_eq!(ledger.admit(1, "one", b"effect"), Admission::New);
        assert_eq!(ledger.admit(1, "two", b"effect"), Admission::New);
        ledger.unknown(1, "one");
        assert_eq!(ledger.epoch(), 1);
        ledger.unknown(1, "two");
        assert_eq!(ledger.epoch(), 2);
        assert_eq!(
            ledger.admit(1, "one", b"effect"),
            Admission::Known(MutationState::Retired)
        );
        assert_eq!(ledger.admit(2, "three", b"effect"), Admission::New);
    }

    #[test]
    fn a_full_ledger_is_not_retired_under_a_change_still_running() {
        let mut ledger = Ledger::new(2);
        assert_eq!(ledger.admit(1, "one", b"effect"), Admission::New);
        assert_eq!(ledger.admit(1, "two", b"effect"), Admission::New);
        ledger.waited(1, "two");
        assert_eq!(ledger.query(1, "two"), MutationState::Unknown);
        ledger.finish(1, "one", Answer::Done);
        assert_eq!(ledger.epoch(), 1);
        ledger.finish(1, "two", Answer::Done);
        assert_eq!(ledger.epoch(), 2);
    }

    #[tokio::test]
    async fn a_lost_answer_keeps_one_effect_and_a_late_query_can_find_it() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let service = Service::default();
        let count = Arc::new(AtomicUsize::new(0));
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let effect = count.clone();
        let result = service
            .execute(
                1,
                "one".into(),
                b"effect".to_vec(),
                Request::Release { id: 1 },
                move |_| async move {
                    effect.fetch_add(1, Ordering::SeqCst);
                    let _ = released.await;
                    Answer::Done
                },
                Duration::from_millis(20),
            )
            .await;
        assert_eq!(result, MutationState::Unknown);
        let duplicate = service
            .execute(
                1,
                "one".into(),
                b"effect".to_vec(),
                Request::Release { id: 1 },
                |_| async { panic!("duplicate effect") },
                Duration::from_millis(20),
            )
            .await;
        assert_eq!(duplicate, MutationState::Unknown);
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if matches!(
                    service.ledger.lock().unwrap().query(1, "one"),
                    MutationState::Result { .. }
                ) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_blocked_dispatch_expires_without_starting_its_effect() {
        let service = Service::default();
        let guard = service.dispatch.lock().await;
        let result = service
            .execute(
                1,
                "one".into(),
                b"effect".to_vec(),
                Request::Release { id: 1 },
                |_| async { panic!("expired effect") },
                Duration::from_millis(20),
            )
            .await;
        assert!(matches!(result, MutationState::Rejected { .. }));
        drop(guard);
        assert_eq!(service.ledger.lock().unwrap().query(1, "one"), result);
        let again = service
            .execute(
                1,
                "one".into(),
                b"effect".to_vec(),
                Request::Release { id: 1 },
                |_| async { panic!("rejected effect started") },
                Duration::from_millis(20),
            )
            .await;
        assert_eq!(again, result);
    }

    #[tokio::test]
    async fn a_blackholed_operation_eventually_releases_dispatch_without_becoming_replayable() {
        let service = Service::default();
        let result = service
            .execute(
                1,
                "one".into(),
                b"effect".to_vec(),
                Request::Release { id: 1 },
                |_| std::future::pending::<Answer>(),
                Duration::from_millis(20),
            )
            .await;
        assert_eq!(result, MutationState::Unknown);
        drop(
            tokio::time::timeout(Duration::from_millis(200), service.dispatch.lock())
                .await
                .unwrap(),
        );
        let duplicate = service
            .execute(
                1,
                "one".into(),
                b"effect".to_vec(),
                Request::Release { id: 1 },
                |_| async { panic!("replayed blackholed operation") },
                Duration::from_millis(20),
            )
            .await;
        assert_eq!(duplicate, MutationState::Unknown);
        let next = service
            .execute(
                1,
                "two".into(),
                b"next".to_vec(),
                Request::Release { id: 2 },
                |_| async { Answer::Done },
                Duration::from_millis(20),
            )
            .await;
        assert!(matches!(next, MutationState::Result { .. }));
    }

    #[tokio::test]
    async fn an_uncertain_terminal_reply_never_becomes_a_confirmed_failure_or_a_replayed_effect() {
        let service = Service::default();
        let result = service
            .execute(
                1,
                "one".into(),
                b"effect".to_vec(),
                Request::Release { id: 1 },
                |_| async {
                    Answer::Unconfirmed {
                        text: "terminal connection closed after input".into(),
                    }
                },
                Duration::from_millis(100),
            )
            .await;
        assert_eq!(result, MutationState::Unknown);
        assert_eq!(
            service.ledger.lock().unwrap().query(1, "one"),
            MutationState::Unknown
        );
        let again = service
            .execute(
                1,
                "one".into(),
                b"effect".to_vec(),
                Request::Release { id: 1 },
                |_| async { panic!("replayed uncertain effect") },
                Duration::from_millis(100),
            )
            .await;
        assert_eq!(again, MutationState::Unknown);
    }
}
