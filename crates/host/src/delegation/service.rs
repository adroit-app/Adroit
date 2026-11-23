//! The work host's side of delegation: it answers sessions on a socket,
//! starts the sessions they ask for, hands Codex its messages and watches
//! every session it keeps a record of.
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use adroit_model::ModelCatalog;
use adroit_term::Daemon;

use super::{Code, Error, Result, protocol, store::Store};
use crate::settings::Settings;

mod commands;
#[cfg(test)]
mod fixtures;
mod models;
mod serve;
mod watcher;

pub use serve::envelope;

type SettingsSource = Arc<dyn Fn() -> std::result::Result<Settings, String> + Send + Sync>;

/// How long a session may start without becoming ready or showing a dialog
/// before it shows as stalled.
pub(crate) const STALL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct Context {
    store: Arc<Mutex<Store>>,
    models: Arc<Mutex<ModelCatalog>>,
    daemon: Daemon,
    program: PathBuf,
    root: PathBuf,
    settings: SettingsSource,
}

impl Context {
    async fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.store.clone();
        crate::environment::spawn_blocking(move || {
            let mut store = store.lock().expect("the store's lock");
            store.ensure_current_schema()?;
            operation(&mut store)
        })
        .await
        .map_err(Error::storage)?
    }

    /// Whether sessions take up work other sessions ask of them at all; not
    /// while the settings cannot be read.
    fn takes_requests(&self) -> bool {
        self.settings()
            .is_ok_and(|settings| settings.delegation.requests)
    }

    fn settings(&self) -> Result<Settings> {
        (self.settings)()
            .map(Settings::sane)
            .map_err(|error| Error::new(Code::Invalid, error))
    }
}

pub struct Service {
    tasks: Vec<tokio::task::JoinHandle<()>>,
    socket: PathBuf,
    _claim: File,
}

impl Drop for Service {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl Service {
    /// Opens the state at `root`, answers sessions on its socket and watches them.
    pub async fn start(
        root: &Path,
        daemon: Daemon,
        program: PathBuf,
        settings: impl Fn() -> std::result::Result<Settings, String> + Send + Sync + 'static,
    ) -> Result<Self> {
        adroit_sys::fs::private_dir(root).map_err(Error::storage)?;
        let claim = adroit_sys::fs::lockable(root).map_err(Error::storage)?;
        claim.try_lock().map_err(Error::storage)?;
        let store = Store::open(&super::app::database(root))?;
        let context = Context {
            store: Arc::new(Mutex::new(store)),
            models: Arc::new(Mutex::new(ModelCatalog::default())),
            daemon,
            program,
            root: root.to_path_buf(),
            settings: Arc::new(settings),
        };
        let socket = protocol::socket(root);
        adroit_term::own_dir(&socket).map_err(Error::storage)?;
        let listener = crate::work_host::bind_private(&socket).map_err(Error::storage)?;
        let serving = context.clone();
        let tasks = vec![
            crate::environment::spawn(async move { serve::serve(listener, serving).await }),
            crate::environment::spawn(watcher::supervise(context.clone())),
            crate::environment::spawn(models::watch_models(context)),
        ];
        Ok(Self {
            tasks,
            socket,
            _claim: claim,
        })
    }
}
