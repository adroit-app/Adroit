//! The environments kept on this computer, which every window lists.

use std::path::Path;

use adroit_connect::Target;
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
/// Settings window, which has no hold of the window.
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
    /// The environments kept that the window has no link to yet, or a link
    /// to where they no longer are.
    pub(crate) link: Vec<Environment>,
    /// The environments linked that are no longer kept, are archived, or
    /// have moved; those that moved are linked again.
    pub(crate) unlink: Vec<EnvironmentId>,
}

/// How the window started in `own`, linked to `linked`, each with the target
/// its link reaches when known, comes to list every environment `catalog`
/// keeps unarchived. `own` is never linked again nor unlinked: the window's
/// process stays bound to it.
pub(crate) fn follow(
    catalog: &Catalog,
    linked: impl IntoIterator<Item = (EnvironmentId, Option<Target>)>,
    own: &EnvironmentId,
) -> Following {
    let kept: Vec<&Environment> = catalog
        .environments
        .iter()
        .filter(|environment| !environment.archived)
        .collect();
    let linked: Vec<(EnvironmentId, Option<Target>)> =
        linked.into_iter().filter(|(id, _)| id != own).collect();
    let stands = |id: &EnvironmentId, target: &Option<Target>| {
        kept.iter().any(|e| {
            EnvironmentId::new(&e.id) == *id && target.as_ref().is_none_or(|t| *t == e.target)
        })
    };
    Following {
        link: kept
            .iter()
            .filter(|e| {
                let id = EnvironmentId::new(&e.id);
                id != *own && !linked.iter().any(|(l, t)| *l == id && stands(l, t))
            })
            .map(|e| (*e).clone())
            .collect(),
        unlink: linked
            .into_iter()
            .filter(|(id, target)| !stands(id, target))
            .map(|(id, _)| id)
            .collect(),
    }
}

/// A name for the work machine at `address`: its host, without the port, or
/// what a loopback address means.
pub(crate) fn machine_name(address: &str) -> String {
    let host = address.rsplit_once(':').map_or(address, |(host, _)| host);
    let host = host.trim_matches(['[', ']']);
    let loopback = host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    match loopback {
        true => "This computer's gateway".into(),
        false => host.to_string(),
    }
}

/// `base`, or `base` numbered past the names of the environments `catalog`
/// lists.
pub(crate) fn fresh_name(catalog: &Catalog, base: &str) -> String {
    let taken = |name: &str| {
        catalog
            .environments
            .iter()
            .any(|e| !e.archived && e.name == name)
    };
    (1..)
        .map(|n| match n {
            1 => base.to_string(),
            n => format!("{base} {n}"),
        })
        .find(|name| !taken(name))
        .unwrap_or_else(|| base.to_string())
}

/// The environment `catalog` keeps for the work machine whose connection
/// `reaches`, one listed before one archived. Environments on this computer
/// are never one machine's: each is one of its own.
pub(crate) fn of_machine(
    catalog: &Catalog,
    reaches: impl Fn(&str, &str) -> bool,
) -> Option<&Environment> {
    let mine = |e: &&Environment| matches!(&e.target, Target::Native { address, server } if reaches(address, server));
    let mut kept = catalog.environments.iter().filter(mine);
    let first = kept.clone().find(|e| !e.archived);
    first.or_else(|| kept.next())
}

/// Keep an environment named `name` for `target` in the catalog at `path`,
/// refusing to when another window saved it since it was read: the one the
/// work machine `target` reaches has already, restored when archived, or
/// else a new one.
pub(crate) fn add(path: &Path, name: &str, target: Target) -> Result<Environment, String> {
    let mut catalog = Catalog::load(path)?;
    let revision = catalog.revision;
    let known = match &target {
        Target::Native { server, .. } => {
            of_machine(&catalog, |_, kept| kept == server).map(|e| e.id.clone())
        }
        Target::Local => None,
    };
    let added = match catalog
        .environments
        .iter_mut()
        .find(|e| Some(&e.id) == known.as_ref())
    {
        Some(kept) => {
            kept.archived = false;
            kept.name = name.trim().into();
            kept.target = target;
            kept.clone()
        }
        None => {
            let new = Environment::new(name, target);
            catalog.environments.push(new.clone());
            new
        }
    };
    catalog.save(path, revision)?;
    Ok(added)
}

/// Archive `environment` in the catalog at `path`, refusing to when another
/// window saved it since it was read.
pub(crate) fn archive(path: &Path, environment: &EnvironmentId) -> Result<(), String> {
    change(path, environment, |kept| kept.archived = true)
}

/// List `environment`, archived in the catalog at `path`, again.
pub(crate) fn restore(path: &Path, environment: &EnvironmentId) -> Result<(), String> {
    change(path, environment, |kept| kept.archived = false)
}

/// Point `environment` in the catalog at `path` at `target`, the same work
/// machine reached another way, keeping what it is.
pub(crate) fn retarget(
    path: &Path,
    environment: &EnvironmentId,
    target: Target,
) -> Result<(), String> {
    change(path, environment, |kept| kept.target = target)
}

/// Call `environment` in the catalog at `path` by `name`, keeping what it is.
pub(crate) fn rename(path: &Path, environment: &EnvironmentId, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the environment a name".into());
    }
    change(path, environment, |kept| kept.name = name.into())
}

/// Change `environment` in the catalog at `path` as `edit` does, refusing to
/// when another window saved it since it was read.
fn change(
    path: &Path,
    environment: &EnvironmentId,
    edit: impl FnOnce(&mut Environment),
) -> Result<(), String> {
    let mut catalog = Catalog::load(path)?;
    let revision = catalog.revision;
    let kept = catalog
        .environments
        .iter_mut()
        .find(|e| EnvironmentId::new(&e.id) == *environment)
        .ok_or("The environment is no longer listed")?;
    edit(kept);
    catalog.save(path, revision)
}

#[cfg(test)]
mod tests {
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

    fn machine(id: &str, server: &str, archived: bool) -> Environment {
        Environment {
            target: Target::Native {
                address: format!("{id}.local:47130"),
                server: server.repeat(32),
            },
            ..environment(id, archived)
        }
    }

    #[test]
    fn a_work_machine_is_named_by_its_host() {
        assert_eq!(machine_name("work-box.local:47130"), "work-box.local");
        assert_eq!(machine_name("[fe80::1]:47130"), "fe80::1");
        assert_eq!(machine_name("work-box"), "work-box");
        assert_eq!(machine_name("127.0.0.1:47130"), "This computer's gateway");
        assert_eq!(machine_name("[::1]:47130"), "This computer's gateway");
        assert_eq!(machine_name("localhost:47130"), "This computer's gateway");
    }

    #[test]
    fn a_name_an_environment_listed_has_is_numbered_but_one_archived_is_free() {
        let mut kept = catalog([environment("a", false), environment("b", true)]);
        kept.environments[0].name = "Box".into();
        kept.environments[1].name = "Old".into();
        assert_eq!(fresh_name(&kept, "Box"), "Box 2");
        assert_eq!(fresh_name(&kept, "Old"), "Old");
        kept.environments.push(Environment {
            name: "Box 2".into(),
            ..environment("c", false)
        });
        assert_eq!(fresh_name(&kept, "Box"), "Box 3");
    }

    #[test]
    fn a_work_machine_s_environment_is_found_by_its_key_the_listed_one_first() {
        let kept = catalog([
            environment("local", false),
            machine("old", "ab", true),
            machine("box", "ab", false),
            machine("other", "cd", false),
        ]);
        let by_key = |key: &str| {
            of_machine(&kept, |_, server| server.starts_with(key)).map(|e| e.id.as_str())
        };
        assert_eq!(by_key("abab"), Some("box"));
        assert_eq!(by_key("cdcd"), Some("other"));
        assert_eq!(by_key("efef"), None);
        let archived = catalog([machine("old", "ab", true)]);
        assert_eq!(
            of_machine(&archived, |_, server| server.starts_with("ab")).map(|e| e.id.as_str()),
            Some("old")
        );
    }

    #[test]
    fn adding_keeps_a_new_environment_for_this_computer_each_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.toml");
        let first = add(&path, "Here", Target::Local).unwrap();
        let second = add(&path, " Also here ", Target::Local).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(second.name, "Also here");
        assert_eq!(Catalog::load(&path).unwrap().environments, [first, second]);
    }

    #[test]
    fn adding_a_work_machine_kept_before_restores_its_environment_under_the_name_typed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.toml");
        catalog([machine("box", "ab", true)])
            .save(&path, 0)
            .unwrap();
        let moved = Target::Native {
            address: "box.example.com:47130".into(),
            server: "ab".repeat(32),
        };
        let added = add(&path, "Box", moved.clone()).unwrap();
        assert_eq!(added.id, "box");
        assert!(!added.archived);
        assert_eq!(added.name, "Box");
        assert_eq!(added.target, moved);
        assert_eq!(Catalog::load(&path).unwrap().environments, [added]);
    }

    /// `ids` linked where each environment of the same id was.
    fn linked<const N: usize>(
        ids: [&str; N],
        where_: &Catalog,
    ) -> Vec<(EnvironmentId, Option<Target>)> {
        ids.map(|id| {
            let target = where_
                .environments
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.target.clone());
            (EnvironmentId::new(id), target)
        })
        .into()
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
            linked(["own", "linked"], &catalog),
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
            linked(["own", "archived", "deleted"], &catalog),
            &EnvironmentId::new("own"),
        );
        assert!(following.link.is_empty());
        assert_eq!(following.unlink, ids(["archived", "deleted"]));
    }

    #[test]
    fn a_window_listing_what_the_catalog_keeps_changes_nothing() {
        let catalog = catalog([environment("own", false), environment("there", false)]);
        let following = follow(
            &catalog,
            linked(["own", "there"], &catalog),
            &EnvironmentId::new("own"),
        );
        assert_eq!(following, Following::default());
    }

    #[test]
    fn an_environment_moved_to_another_address_is_linked_again_there_but_never_the_own_one() {
        let before = catalog([machine("own", "ab", false), machine("box", "cd", false)]);
        let mut after = before.clone();
        for moved in &mut after.environments {
            if let Target::Native { address, .. } = &mut moved.target {
                *address = format!("{}.example.com:47130", moved.id);
            }
        }
        let following = follow(
            &after,
            linked(["own", "box"], &before),
            &EnvironmentId::new("own"),
        );
        assert_eq!(following.unlink, ids(["box"]));
        assert_eq!(following.link, [after.environments[1].clone()]);
    }

    #[test]
    fn moving_an_environment_keeps_what_it_is_and_points_it_at_the_address() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.toml");
        catalog([machine("box", "ab", false)])
            .save(&path, 0)
            .unwrap();
        let moved = Target::Native {
            address: "box.example.com:47130".into(),
            server: "ab".repeat(32),
        };
        retarget(&path, &EnvironmentId::new("box"), moved.clone()).unwrap();
        let kept = Catalog::load(&path).unwrap().environments;
        assert_eq!(
            kept,
            [Environment {
                target: moved,
                ..machine("box", "ab", false)
            }]
        );
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
    fn an_environment_restored_is_listed_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.toml");
        catalog([environment("there", true)])
            .save(&path, 0)
            .unwrap();
        restore(&path, &EnvironmentId::new("there")).unwrap();
        assert_eq!(
            Catalog::load(&path).unwrap().environments,
            [environment("there", false)]
        );
    }

    #[test]
    fn renaming_an_environment_keeps_its_identity_and_refuses_an_empty_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.toml");
        catalog([environment("there", false)])
            .save(&path, 0)
            .unwrap();
        rename(&path, &EnvironmentId::new("there"), "  Work box ").unwrap();
        assert!(rename(&path, &EnvironmentId::new("there"), "  ").is_err());
        assert_eq!(
            Catalog::load(&path).unwrap().environments,
            [Environment {
                name: "Work box".into(),
                ..environment("there", false)
            }]
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
