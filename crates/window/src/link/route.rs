//! What a link connects to: the environment it asks its work host for, and
//! where that host and its terminal host are.

use std::path::{Path, PathBuf};

use adroit_connect::Target;
use adroit_connect::environments::{Binding, Environment};
use adroit_model::EnvironmentId;

/// The environment a link asks for, and the way to its work host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Route {
    /// The environment the work along the route belongs to.
    pub(crate) environment: EnvironmentId,
    pub(crate) scope: Option<String>,
    pub(crate) toward: Result<Toward, String>,
}

/// Where a link's work host is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Toward {
    /// On this machine, listening at `host`, its terminal host at `terminal`.
    Local { host: PathBuf, terminal: PathBuf },
    /// On a work machine reached through its gateway.
    Remote(Target),
}

impl Toward {
    /// This process's instance's own work host and terminal host.
    pub(crate) fn own() -> Toward {
        Toward::Local {
            host: adroit_model::paths::work_host_socket(),
            terminal: adroit_model::paths::terminal_host_socket(),
        }
    }
}

impl Route {
    /// The route this process's instance has selected: its environment's
    /// binding, or else the target `execution` names with no environment.
    pub(crate) fn selected(execution: &Path) -> Route {
        let own = Toward::own();
        match adroit_connect::environments::active() {
            Ok(Some(binding)) => Route::bound(
                &binding,
                &adroit_model::paths::instance(),
                &adroit_model::paths::state_dir(),
                own,
            ),
            Ok(None) => Route::unbound(Target::load(execution), own),
            Err(text) => Route {
                environment: EnvironmentId::unbound(),
                scope: None,
                toward: Err(text),
            },
        }
    }

    /// The route `binding` names, from the instance `instance` whose own
    /// work host is `own`, with instances' folders under `state`.
    pub(crate) fn bound(binding: &Binding, instance: &str, state: &Path, own: Toward) -> Route {
        let toward = match &binding.target {
            Target::Local => {
                let work = binding.work_instance.as_deref().unwrap_or("default");
                if binding.scope.is_some() || work != instance {
                    let service = adroit_model::paths::instance_dir(state, work);
                    Toward::Local {
                        host: service.join("host/host.sock"),
                        terminal: service.join("felis/daemon.sock"),
                    }
                } else {
                    own
                }
            }
            target => Toward::Remote(target.clone()),
        };
        Route {
            environment: EnvironmentId::new(&binding.environment),
            scope: binding.scope.clone(),
            toward: Ok(toward),
        }
    }

    /// The route to `environment`, shown in a window of the instance
    /// `instance` beside its own, as that instance's binding to it would
    /// name it.
    pub(crate) fn shown(
        environment: &Environment,
        instance: &str,
        state: &Path,
        own: Toward,
    ) -> Result<Route, String> {
        Ok(Route::bound(
            &environment.bind(instance)?,
            instance,
            state,
            own,
        ))
    }

    /// The route an instance without an environment takes to `target`, its
    /// own work host being `own`.
    pub(crate) fn unbound(target: Result<Target, String>, own: Toward) -> Route {
        Route {
            environment: EnvironmentId::unbound(),
            scope: None,
            toward: target.map(|target| match target {
                Target::Local => own,
                target => Toward::Remote(target),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(target: Target, scope: Option<&str>, work: Option<&str>) -> Binding {
        Binding {
            environment: "env".into(),
            name: "Env".into(),
            target,
            scope: scope.map(Into::into),
            client: "client".into(),
            work_instance: work.map(Into::into),
        }
    }

    fn own() -> Toward {
        Toward::Local {
            host: "/own/host.sock".into(),
            terminal: "/own/daemon.sock".into(),
        }
    }

    fn native() -> Target {
        Target::Native {
            address: "work.example:7100".into(),
            server: "fingerprint".into(),
        }
    }

    #[test]
    fn a_scoped_local_environment_reaches_its_work_instances_hosts_and_asks_for_its_scope() {
        let route = Route::bound(
            &binding(Target::Local, Some("scope-a"), Some("work")),
            "client",
            Path::new("/state"),
            own(),
        );
        assert_eq!(
            route,
            Route {
                environment: EnvironmentId::new("env"),
                scope: Some("scope-a".into()),
                toward: Ok(Toward::Local {
                    host: "/state/adroit/instances/work/host/host.sock".into(),
                    terminal: "/state/adroit/instances/work/felis/daemon.sock".into(),
                }),
            }
        );
    }

    #[test]
    fn a_scoped_environment_on_the_default_work_instance_reaches_the_default_instances_hosts() {
        let route = Route::bound(
            &binding(Target::Local, Some("scope-a"), None),
            "default",
            Path::new("/state"),
            own(),
        );
        assert_eq!(
            route.toward,
            Ok(Toward::Local {
                host: "/state/adroit/host/host.sock".into(),
                terminal: "/state/adroit/felis/daemon.sock".into(),
            })
        );
    }

    #[test]
    fn an_unscoped_environment_on_the_instance_itself_keeps_the_instances_own_hosts() {
        let route = Route::bound(
            &binding(Target::Local, None, Some("client")),
            "client",
            Path::new("/state"),
            own(),
        );
        assert_eq!(
            route,
            Route {
                environment: EnvironmentId::new("env"),
                scope: None,
                toward: Ok(own()),
            }
        );
    }

    #[test]
    fn an_unscoped_environment_on_another_work_instance_reaches_that_instances_hosts() {
        let route = Route::bound(
            &binding(Target::Local, None, Some("work")),
            "client",
            Path::new("/state"),
            own(),
        );
        assert_eq!(
            route.toward,
            Ok(Toward::Local {
                host: "/state/adroit/instances/work/host/host.sock".into(),
                terminal: "/state/adroit/instances/work/felis/daemon.sock".into(),
            })
        );
    }

    #[test]
    fn an_environment_on_a_work_machine_goes_there_and_asks_for_its_scope() {
        let route = Route::bound(
            &binding(native(), Some("scope-a"), None),
            "client",
            Path::new("/state"),
            own(),
        );
        assert_eq!(
            route,
            Route {
                environment: EnvironmentId::new("env"),
                scope: Some("scope-a".into()),
                toward: Ok(Toward::Remote(native())),
            }
        );
    }

    #[test]
    fn an_environment_shown_beside_the_window_s_own_is_reached_as_its_binding_from_this_instance() {
        let mut environment = Environment::new("There", Target::Local);
        environment.work_instance = Some("work".into());
        let route = Route::shown(&environment, "client", Path::new("/state"), own()).unwrap();
        assert_eq!(
            route,
            Route {
                environment: EnvironmentId::new(&environment.id),
                scope: environment.scope.clone(),
                toward: Ok(Toward::Local {
                    host: "/state/adroit/instances/work/host/host.sock".into(),
                    terminal: "/state/adroit/instances/work/felis/daemon.sock".into(),
                }),
            }
        );
        let paired = Target::Native {
            address: "work.example:7100".into(),
            server: "ab".repeat(32),
        };
        let remote = Environment::new("Remote", paired.clone());
        assert_eq!(
            Route::shown(&remote, "client", Path::new("/state"), own())
                .unwrap()
                .toward,
            Ok(Toward::Remote(paired))
        );
    }

    #[test]
    fn a_bound_route_s_work_is_known_by_its_environment_and_an_unbound_one_s_by_none() {
        let bound = Route::bound(
            &binding(native(), Some("scope-a"), None),
            "client",
            Path::new("/state"),
            own(),
        );
        assert_eq!(bound.environment, EnvironmentId::new("env"));
        let unbound = Route::unbound(Ok(Target::Local), own());
        assert_eq!(unbound.environment, EnvironmentId::unbound());
    }

    #[test]
    fn without_an_environment_the_execution_target_is_followed_with_no_scope() {
        assert_eq!(
            Route::unbound(Ok(Target::Local), own()),
            Route {
                environment: EnvironmentId::unbound(),
                scope: None,
                toward: Ok(own()),
            }
        );
        assert_eq!(
            Route::unbound(Ok(native()), own()).toward,
            Ok(Toward::Remote(native()))
        );
        assert_eq!(
            Route::unbound(Err("unreadable".into()), own()).toward,
            Err("unreadable".into())
        );
    }
}
