//! Work settings and task context belonging to a working environment.

use std::cell::RefCell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const ENV: &str = "ADROIT_ENVIRONMENT";
pub const TAG: &str = "adroit.environment";

pub struct Environment {
    pub id: String,
    pub root: PathBuf,
    pub settings: RwLock<crate::settings::Settings>,
}

tokio::task_local! { static ACTIVE: RefCell<Option<Arc<Environment>>>; }
thread_local! { static BLOCKING: RefCell<Option<Arc<Environment>>> = const { RefCell::new(None) }; }

impl Environment {
    pub fn at(id: &str, root: &Path) -> Result<Arc<Self>, String> {
        if !adroit_model::paths::is_instance(id) {
            return Err("Invalid work environment".into());
        }
        adroit_term::own_dir(&root.join("environments").join(id).join("owner.sock"))
            .map_err(|e| e.to_string())?;
        let context = Arc::new(Self {
            id: id.into(),
            root: root.join("environments").join(id),
            settings: RwLock::new(crate::settings::Settings::default()),
        });
        let config = context.root.join("config");
        let settings =
            crate::settings::Settings::read(&crate::config::file(&config.join("settings.toml")))
                .map(|settings| crate::themes::migrated(settings, &config.join("themes")))
                .unwrap_or_else(crate::settings::recover);
        *context.settings.write().map_err(|e| e.to_string())? = settings;
        Ok(context)
    }
}

pub fn current() -> Option<Arc<Environment>> {
    ACTIVE
        .try_with(|v| v.borrow().clone())
        .unwrap_or_else(|_| BLOCKING.with(|v| v.borrow().clone()))
}

pub fn select(context: Arc<Environment>) -> Result<(), String> {
    ACTIVE
        .try_with(|v| *v.borrow_mut() = Some(context))
        .map_err(|_| "No work connection context".into())
}

pub async fn run<T>(context: Option<Arc<Environment>>, future: impl Future<Output = T>) -> T {
    ACTIVE.scope(RefCell::new(context), future).await
}

fn blocking<T>(context: Option<Arc<Environment>>, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<Arc<Environment>>);
    impl Drop for Reset {
        fn drop(&mut self) {
            BLOCKING.with(|v| *v.borrow_mut() = self.0.take());
        }
    }
    let _reset = Reset(BLOCKING.with(|v| v.replace(context)));
    f()
}

pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(run(current(), future))
}

pub fn spawn_blocking<F, T>(f: F) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let context = current();
    tokio::task::spawn_blocking(move || blocking(context, f))
}

pub fn owns(tags: &[String]) -> bool {
    adroit_term::tags::value(tags, TAG) == current().map(|c| c.id.clone())
}

pub fn tags() -> Vec<String> {
    current().map_or_else(Vec::new, |c| adroit_term::tags::tagged(TAG, &c.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_saved_settings_do_not_prevent_recovering_an_existing_environment() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("environments/work/config");
        let _context = Environment::at("work", root.path()).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("settings.toml"), "scratch_dir = [").unwrap();
        let context = Environment::at("work", root.path()).unwrap();
        run(Some(context), async {
            assert!(crate::settings::load_error().is_some());
            assert_eq!(crate::settings::current().delegation.max_started, 0);
        })
        .await;
    }

    #[tokio::test]
    async fn environments_on_one_host_keep_settings_and_spawned_work_apart() {
        let dir = tempfile::tempdir().unwrap();
        let a = Environment::at("personal", dir.path()).unwrap();
        let b = Environment::at("work", dir.path()).unwrap();
        run(Some(a.clone()), async {
            let mut settings = crate::settings::current();
            settings.scratch_dir = "/personal".into();
            crate::settings::set(settings).unwrap();
            assert_eq!(
                spawn(async { crate::settings::current().scratch_dir })
                    .await
                    .unwrap(),
                "/personal"
            );
            assert_eq!(
                spawn_blocking(|| crate::settings::current().scratch_dir)
                    .await
                    .unwrap(),
                "/personal"
            );
        })
        .await;
        run(Some(b), async {
            assert_ne!(crate::settings::current().scratch_dir, "/personal");
        })
        .await;
        let reloaded = Environment::at("personal", dir.path()).unwrap();
        run(Some(reloaded), async {
            assert_eq!(crate::settings::current().scratch_dir, "/personal");
        })
        .await;
        assert!(Environment::at("../escape", dir.path()).is_err());
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;

    #[tokio::test]
    async fn only_the_selected_environment_sees_a_terminals_tags() {
        let dir = tempfile::tempdir().unwrap();
        let a = Environment::at("a", dir.path()).unwrap();
        let b = Environment::at("b", dir.path()).unwrap();
        let tagged = run(Some(a.clone()), async { tags() }).await;
        assert!(!owns(&tagged));
        run(Some(a), async {
            assert!(owns(&tagged));
            assert!(!owns(&[]));
        })
        .await;
        run(Some(b), async {
            assert!(!owns(&tagged));
        })
        .await;
        assert!(owns(&[]));
    }
}

#[cfg(test)]
mod reload_tests {
    use super::*;
    #[tokio::test]
    async fn creating_an_environment_inside_an_unscoped_task_reads_its_own_saved_settings() {
        let dir = tempfile::tempdir().unwrap();
        let context = Environment::at("work", dir.path()).unwrap();
        run(Some(context), async {
            let mut settings = crate::settings::current();
            settings.scratch_dir = "/work-only".into();
            crate::settings::set(settings).unwrap();
        })
        .await;
        run(None, async {
            let restored = Environment::at("work", dir.path()).unwrap();
            run(Some(restored), async {
                assert_eq!(crate::settings::current().scratch_dir, "/work-only");
            })
            .await;
        })
        .await;
    }
}
