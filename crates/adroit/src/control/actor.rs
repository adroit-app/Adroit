//! A single database owner keeps synchronous SQLite work off async executors.
use std::path::PathBuf;

use tokio::sync::{mpsc, oneshot};

use super::store::{ControlError, ErrorCode, Store};

type Job = Box<dyn FnOnce(&mut Store) + Send>;

#[derive(Clone)]
pub struct Actor {
    jobs: mpsc::Sender<Job>,
}

impl Actor {
    pub async fn open(path: PathBuf) -> Result<Self, ControlError> {
        let (jobs, mut receiver) = mpsc::channel::<Job>(128);
        let (ready, result) = oneshot::channel();
        std::thread::Builder::new()
            .name("adroit-control".into())
            .spawn(move || {
                match Store::open(&path).and_then(|mut store| {
                    store.recover_deliveries()?;
                    Ok(store)
                }) {
                    Ok(mut store) => {
                        if ready.send(Ok(())).is_ok() {
                            while let Some(job) = receiver.blocking_recv() {
                                job(&mut store);
                            }
                        }
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
            })
            .map_err(|error| ControlError::new(ErrorCode::StorageFailure, error.to_string()))?;
        result.await.map_err(|_| stopped())??;
        Ok(Self { jobs })
    }

    pub async fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Store) -> Result<T, ControlError> + Send + 'static,
    ) -> Result<T, ControlError> {
        let (reply, result) = oneshot::channel();
        self.jobs
            .send(Box::new(move |store| {
                let _ = reply.send(
                    store
                        .ensure_current_schema()
                        .and_then(|()| operation(store)),
                );
            }))
            .await
            .map_err(|_| stopped())?;
        result.await.map_err(|_| stopped())?
    }
}

fn stopped() -> ControlError {
    ControlError::new(
        ErrorCode::StorageFailure,
        "the control database owner stopped",
    )
}
