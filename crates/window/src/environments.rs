//! The environments kept on this computer, which every window lists.

use std::path::Path;

use adroit_connect::environments::{Catalog, Environment};
use adroit_model::EnvironmentId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub(crate) fn active_name() -> String {
    let binding = adroit_connect::environments::active().ok().flatten();
    binding.map_or_else(
        || "Personal".into(),
        |binding| {
            Catalog::load(&Catalog::path())
                .ok()
                .and_then(|c| {
                    c.environments
                        .into_iter()
                        .find(|p| p.id == binding.environment)
                })
                .map_or(binding.name, |p| p.name)
        },
    )
}

thread_local! {
    /// Where an environment asked for is shown: the window this process
    /// holds, once it listens.
    static SHOWING: std::cell::RefCell<Option<UnboundedSender<Environment>>> =
        const { std::cell::RefCell::new(None) };
}

/// The environments asked to be shown in this process's window, from the
/// Environments window, which has no hold of the window.
pub(crate) fn listen() -> UnboundedReceiver<Environment> {
    let (asked, showing) = tokio::sync::mpsc::unbounded_channel();
    SHOWING.with(|s| *s.borrow_mut() = Some(asked));
    showing
}

/// Show `environment`'s heading in this process's window.
pub(crate) fn show(environment: &Environment) -> Result<(), String> {
    let sent = SHOWING.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|asked| asked.send(environment.clone()).is_ok())
    });
    sent.then_some(())
        .ok_or_else(|| "No window can show the environment".into())
}

/// Open `environment` in a window of its own, in a new independent
/// instance when `independent`.
pub(crate) fn open(environment: &Environment, independent: bool) -> Result<(), String> {
    let program = std::env::current_exe().map_err(|e| e.to_string())?;
    adroit_connect::environments::open_command(&program, environment, independent)?
        .spawn()
        .map_err(|e| format!("Cannot open environment: {e}"))?;
    Ok(())
}

/// Where the work of `environment`, one a window shows beside its own, keeps
/// its data on this computer.
pub(crate) fn data_dir_of(
    environment: &adroit_model::EnvironmentId,
) -> Result<std::path::PathBuf, String> {
    let catalog = Catalog::load(&Catalog::path())?;
    let shown = catalog
        .environments
        .iter()
        .find(|e| e.id == environment.to_string())
        .ok_or("The environment is no longer listed")?;
    shown
        .bind(&adroit_model::paths::instance())?
        .local_data_dir(&crate::paths::state_dir(), &crate::paths::data_dir())
}

pub(crate) fn work_data_dir() -> Result<std::path::PathBuf, String> {
    let data = crate::paths::data_dir();
    adroit_connect::environments::active()?.map_or(Ok(data.clone()), |binding| {
        binding.local_data_dir(&crate::paths::state_dir(), &data)
    })
}

/// What a window started in `own` changes to list what a catalog keeps.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Following {
    /// The environments kept that the window has no link to yet.
    pub(crate) link: Vec<Environment>,
    /// The environments linked that are no longer kept, or are archived.
    pub(crate) unlink: Vec<EnvironmentId>,
}

/// How the window started in `own`, linked to `linked`, comes to list every
/// environment `catalog` keeps unarchived. `own` is never linked again nor
/// unlinked: the window's process stays bound to it.
pub(crate) fn follow<'a>(
    catalog: &Catalog,
    linked: impl IntoIterator<Item = &'a EnvironmentId>,
    own: &EnvironmentId,
) -> Following {
    let kept: Vec<&Environment> = catalog
        .environments
        .iter()
        .filter(|environment| !environment.archived)
        .collect();
    let linked: Vec<&EnvironmentId> = linked.into_iter().collect();
    let is_kept = |id: &EnvironmentId| kept.iter().any(|e| EnvironmentId::new(&e.id) == *id);
    Following {
        link: kept
            .iter()
            .filter(|e| {
                let id = EnvironmentId::new(&e.id);
                id != *own && !linked.contains(&&id)
            })
            .map(|e| (*e).clone())
            .collect(),
        unlink: linked
            .into_iter()
            .filter(|id| *id != own && !is_kept(id))
            .cloned()
            .collect(),
    }
}

/// Archive `environment` in the catalog at `path`, refusing to when another
/// window saved it since it was read.
pub(crate) fn archive(path: &Path, environment: &EnvironmentId) -> Result<(), String> {
    let mut catalog = Catalog::load(path)?;
    let revision = catalog.revision;
    let kept = catalog
        .environments
        .iter_mut()
        .find(|e| EnvironmentId::new(&e.id) == *environment)
        .ok_or("The environment is no longer listed")?;
    kept.archived = true;
    catalog.save(path, revision)
}

#[cfg(test)]
mod tests {
    use adroit_connect::Target;

    use super::*;

    fn environment(id: &str, archived: bool) -> Environment {
        Environment {
            id: id.into(),
            name: id.to_uppercase(),
            target: Target::Local,
            scope: Some(id.into()),
            archived,
            legacy_client: None,
            work_instance: None,
        }
    }

    fn catalog(environments: impl IntoIterator<Item = Environment>) -> Catalog {
        Catalog {
            environments: environments.into_iter().collect(),
            ..Catalog::default()
        }
    }

    fn ids<const N: usize>(ids: [&str; N]) -> [EnvironmentId; N] {
        ids.map(EnvironmentId::new)
    }

    #[test]
    fn every_environment_kept_and_not_linked_is_linked_but_the_own_one() {
        let catalog = catalog([
            environment("own", false),
            environment("linked", false),
            environment("added", false),
            environment("archived", true),
        ]);
        let following = follow(
            &catalog,
            &ids(["own", "linked"]),
            &EnvironmentId::new("own"),
        );
        assert_eq!(following.link, [environment("added", false)]);
        assert!(following.unlink.is_empty());
    }

    #[test]
    fn an_environment_archived_or_no_longer_kept_is_unlinked_but_never_the_own_one() {
        let catalog = catalog([environment("own", true), environment("archived", true)]);
        let following = follow(
            &catalog,
            &ids(["own", "archived", "deleted"]),
            &EnvironmentId::new("own"),
        );
        assert!(following.link.is_empty());
        assert_eq!(following.unlink, ids(["archived", "deleted"]));
    }

    #[test]
    fn a_window_listing_what_the_catalog_keeps_changes_nothing() {
        let catalog = catalog([environment("own", false), environment("there", false)]);
        let following = follow(&catalog, &ids(["own", "there"]), &EnvironmentId::new("own"));
        assert_eq!(following, Following::default());
    }

    #[test]
    fn an_environment_archived_stays_in_the_catalog_to_be_restored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.toml");
        let mut saved = catalog([environment("own", false), environment("there", false)]);
        saved.save(&path, 0).unwrap();
        archive(&path, &EnvironmentId::new("there")).unwrap();
        assert_eq!(
            Catalog::load(&path).unwrap().environments,
            [environment("own", false), environment("there", true)]
        );
    }

    #[test]
    fn an_environment_the_catalog_no_longer_holds_is_not_archived() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.toml");
        catalog([environment("own", false)]).save(&path, 0).unwrap();
        assert!(archive(&path, &EnvironmentId::new("there")).is_err());
    }
}
