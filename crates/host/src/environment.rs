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

/// The longest name an environment is called, in bytes.
const LONGEST_NAME: usize = 256;

/// Where the name the environment `id`, none for the host's own, was last
/// called is kept in the host's `state`.
fn name_file(state: &Path, id: Option<&str>) -> PathBuf {
    match id {
        Some(id) => state.join("environments").join(id).join("name"),
        None => state.join("environment-name"),
    }
}

/// Call the environment `id` of the host whose state is `state`, none for
/// the host's own, `name`.
pub fn call(state: &Path, id: Option<&str>, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.len() > LONGEST_NAME {
        return Err("An environment's name is nonempty and at most 256 bytes".into());
    }
    if id.is_some_and(|id| !adroit_model::paths::is_instance(id)) {
        return Err("Invalid work environment".into());
    }
    let file = name_file(state, id);
    if std::fs::read_to_string(&file).is_ok_and(|kept| kept == name) {
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let partial = file.with_extension("partial");
    std::fs::write(&partial, name)
        .and_then(|()| std::fs::rename(&partial, &file))
        .map_err(|e| e.to_string())
}

/// The environments the host whose state is `state` serves, its own first,
/// with the sessions `running` in each, by the environment they run in.
pub fn served(
    state: &Path,
    running: &std::collections::BTreeMap<Option<String>, usize>,
) -> Vec<adroit_protocol::ServedEnvironment> {
    let mut ids: std::collections::BTreeSet<String> = std::fs::read_dir(state.join("environments"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    ids.extend(running.keys().flatten().cloned());
    std::iter::once(None)
        .chain(
            ids.into_iter()
                .filter(|id| adroit_model::paths::is_instance(id))
                .map(Some),
        )
        .map(|id| {
            let name = std::fs::read_to_string(name_file(state, id.as_deref()))
                .ok()
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty() && name.len() <= LONGEST_NAME);
            adroit_protocol::ServedEnvironment {
                running: running.get(&id).copied().unwrap_or(0),
                id,
                name,
            }
        })
        .collect()
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
            assert!(!crate::settings::current().delegation.enabled);
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
mod served_tests {
    use super::*;
    use adroit_protocol::ServedEnvironment;
    use std::collections::BTreeMap;

    fn served_in(state: &Path, running: &[(Option<&str>, usize)]) -> Vec<ServedEnvironment> {
        let running = running
            .iter()
            .map(|(id, n)| (id.map(String::from), *n))
            .collect::<BTreeMap<_, _>>();
        served(state, &running)
    }

    #[test]
    fn the_hosts_own_environment_is_served_first_and_each_one_made_after_it() {
        let dir = tempfile::tempdir().unwrap();
        Environment::at("work", dir.path()).unwrap();
        Environment::at("play", dir.path()).unwrap();
        std::fs::create_dir_all(dir.path().join("environments/not an id")).unwrap();
        let ids: Vec<_> = served_in(dir.path(), &[])
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, [None, Some("play".into()), Some("work".into())]);
    }

    #[test]
    fn each_environment_is_served_with_the_sessions_running_in_it() {
        let dir = tempfile::tempdir().unwrap();
        Environment::at("work", dir.path()).unwrap();
        let served = served_in(dir.path(), &[(None, 2), (Some("work"), 1)]);
        assert_eq!(served.iter().map(|e| e.running).collect::<Vec<_>>(), [2, 1]);
    }

    #[test]
    fn an_environment_is_served_under_the_name_a_window_last_called_it() {
        let dir = tempfile::tempdir().unwrap();
        Environment::at("work", dir.path()).unwrap();
        call(dir.path(), None, "Personal").unwrap();
        call(dir.path(), Some("work"), " Work box ").unwrap();
        call(dir.path(), Some("work"), "仕事").unwrap();
        let names: Vec<_> = served_in(dir.path(), &[])
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, [Some("Personal".into()), Some("仕事".into())]);
    }

    #[test]
    fn an_environment_no_window_named_is_served_without_a_name() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(served_in(dir.path(), &[])[0].name, None);
    }

    #[test]
    fn a_name_that_is_empty_or_too_long_or_for_no_valid_environment_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(call(dir.path(), None, "  ").is_err());
        assert!(call(dir.path(), None, &"x".repeat(257)).is_err());
        assert!(call(dir.path(), Some("../escape"), "Escape").is_err());
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
