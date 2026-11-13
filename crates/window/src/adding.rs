//! The sheet adding an environment over the main window: where it works,
//! pairing with a work machine not paired yet, which of the environments its
//! work host serves it joins, and its name.

use adroit_connect::environments::{Catalog, Environment};
use adroit_connect::{Heard, PairingStep, Target};
use adroit_core::msg::Msg;
use adroit_model::EnvironmentId;
use adroit_protocol::ServedEnvironment;
use dioxus::core::{Runtime, ScopeId, Task, current_scope_id};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::Button;
use crate::environments::{Joining, fresh_name, machine_name, of_machine, of_served, retarget};
use crate::icons::{Icon, IconView};
use crate::keys;

/// How far adding has come.
#[derive(Debug, Clone, PartialEq)]
enum Step {
    Where,
    /// Pairing with the work machine at `address`, named `name` until
    /// another name is typed.
    Pair {
        address: String,
        name: String,
    },
    /// Choosing which environment the work host `target` reaches serves to
    /// join, or a new one, named `name` until another name is typed;
    /// `paired` names the work machine paired with on the way.
    Join {
        target: Target,
        name: String,
        paired: Option<String>,
    },
    /// Naming the environment for `target`, working where `joining` says.
    Name {
        target: Target,
        name: String,
        paired: Option<String>,
        joining: Joining,
    },
}

/// Where a choice leads: to a step, or to an environment kept already, as
/// it is or moved to where its work machine was just paired.
#[derive(Debug, Clone, PartialEq)]
enum Next {
    Step(Step),
    Select(Environment),
    Move(Environment),
}

/// Where the work machine `heard` on the network leads: to what it serves
/// when an environment is kept for it, or else to pairing.
fn heard(catalog: &Catalog, heard: &Heard) -> Next {
    machine(
        catalog,
        of_machine(catalog, |_, server| server.starts_with(&heard.key)),
        Step::Pair {
            address: heard.address.clone(),
            name: heard.name.clone(),
        },
    )
}

/// Where the work machine at the `address` typed leads, as [`heard`] does
/// for one known by that address.
fn typed(catalog: &Catalog, address: &str) -> Next {
    let address = address.trim();
    machine(
        catalog,
        of_machine(catalog, |kept, _| kept == address),
        Step::Pair {
            address: address.into(),
            name: machine_name(address),
        },
    )
}

/// Where pairing with the work machine `target` reaches, called `name`,
/// leads: its environment moved to `target` when it was reached another way,
/// or else to what it serves.
fn paired(catalog: &Catalog, target: Target, name: &str) -> Next {
    let joined = |name: &str| {
        Next::Step(Step::Join {
            target: target.clone(),
            name: fresh_name(catalog, name),
            paired: Some(name_of(&target)),
        })
    };
    match &target {
        Target::Native { server, .. } => match of_machine(catalog, |_, kept| kept == server) {
            Some(kept) if !kept.archived && kept.target != target => Next::Move(Environment {
                target: target.clone(),
                ..kept.clone()
            }),
            Some(kept) => joined(&kept.name),
            None => joined(name),
        },
        Target::Local => joined(name),
    }
}

/// What this computer's work host serves.
fn this_computer(catalog: &Catalog) -> Next {
    Next::Step(Step::Join {
        target: Target::Local,
        name: fresh_name(catalog, "This computer"),
        paired: None,
    })
}

/// What the work machine `kept` already serves, or else `pairing`.
fn machine(catalog: &Catalog, kept: Option<&Environment>, pairing: Step) -> Next {
    match kept {
        Some(kept) => Next::Step(Step::Join {
            target: kept.target.clone(),
            name: fresh_name(catalog, &kept.name),
            paired: None,
        }),
        None => Next::Step(pairing),
    }
}

/// Where choosing `served`, of the work host `target` reaches, leads: the
/// environment kept working in it, or else naming one after what it was
/// called, here or by a window elsewhere, or after its machine.
fn chosen(
    catalog: &Catalog,
    target: &Target,
    paired: Option<&str>,
    served: &ServedEnvironment,
) -> Next {
    let kept = of_served(catalog, target, &served.id);
    if let Some(kept) = kept.filter(|kept| !kept.archived) {
        return Next::Select(kept.clone());
    }
    let name = kept
        .map(|kept| kept.name.clone())
        .or_else(|| served.name.clone())
        .unwrap_or_else(|| name_of(target));
    Next::Step(Step::Name {
        target: target.clone(),
        name: fresh_name(catalog, &name),
        paired: paired.map(String::from),
        joining: Joining::Served(served.id.clone()),
    })
}

fn name_of(target: &Target) -> String {
    match target {
        Target::Native { address, .. } => machine_name(address),
        Target::Local => "This computer".into(),
    }
}

/// The sheet, while the view state holds it open.
#[component]
pub(crate) fn AddingSheet() -> Element {
    let app = use_context::<AppCtx>();
    let open = app.overlays.read().adding;
    rsx! {
        if open {
            Sheet {}
        }
    }
}

/// What the sheet holds, which each step changes. Its tasks end with it.
#[derive(Clone, Copy)]
struct Adding {
    app: AppCtx,
    /// The sheet's, so pairing outlives the step that started it.
    scope: ScopeId,
    catalog: Signal<Result<Catalog, String>>,
    step: Signal<Step>,
    error: Signal<Option<String>>,
    pairing: Signal<Option<PairingStep>>,
    /// Kept past the next code, so the person knows why the code changed.
    mismatched: Signal<bool>,
    task: Signal<Option<Task>>,
}

impl Adding {
    fn close(self) {
        self.app.dispatch(Msg::AddingClose);
    }

    fn go(mut self, next: Next) {
        self.error.set(None);
        match next {
            Next::Select(environment) => {
                self.close();
                self.app.show_environment(&environment);
            }
            Next::Move(environment) => {
                let id = EnvironmentId::new(&environment.id);
                match retarget(&Catalog::path(), &id, environment.target.clone()) {
                    Ok(()) => {
                        // The window's own environment is not followed from
                        // the catalog, so its link is told here.
                        if let Target::Native { address, .. } = &environment.target {
                            self.app.move_remote(&id, address);
                        }
                        self.go(Next::Select(environment))
                    }
                    Err(why) => self.error.set(Some(why)),
                }
            }
            Next::Step(Step::Pair { address, name }) => {
                self.step.set(Step::Pair {
                    address: address.clone(),
                    name: name.clone(),
                });
                self.pair(address, name);
            }
            Next::Step(step) => self.step.set(step),
        }
    }

    fn pair(mut self, address: String, name: String) {
        self.stop();
        let task = Runtime::current().in_scope(self.scope, || {
            spawn(async move {
                let mut pairing = self.pairing;
                let mut mismatched = self.mismatched;
                let result = adroit_connect::pair_with(&address, None, |step| {
                    if step == PairingStep::Mismatched {
                        mismatched.set(true);
                    }
                    pairing.set(Some(step));
                })
                .await;
                self.task.set(None);
                match result {
                    Ok(target) => {
                        let catalog = Catalog::load(&Catalog::path());
                        self.catalog.set(catalog.clone());
                        match catalog {
                            Ok(catalog) => {
                                // Their links, refused while this computer
                                // was not paired, wait to be asked again.
                                if let Target::Native { server, .. } = &target {
                                    let on_it = |e: &&Environment| {
                                        !e.archived
                                            && matches!(&e.target, Target::Native { server: kept, .. } if kept == server)
                                    };
                                    for kept in catalog.environments.iter().filter(on_it) {
                                        self.app.retry_remote(&EnvironmentId::new(&kept.id));
                                    }
                                }
                                let next = paired(&catalog, target, &name);
                                self.go(next)
                            }
                            Err(why) => self.error.set(Some(why)),
                        }
                    }
                    Err(why) => self.error.set(Some(why)),
                }
            })
        });
        self.task.set(Some(task));
    }

    /// Stop pairing, forgetting how far it came.
    fn stop(mut self) {
        if let Some(task) = self.task.write().take() {
            task.cancel();
        }
        self.pairing.set(None);
        self.mismatched.set(false);
    }

    fn back(mut self) {
        self.stop();
        self.error.set(None);
        self.step.set(Step::Where);
    }

    fn add(mut self, name: &str, target: Target, joining: Joining) {
        if name.trim().is_empty() {
            self.error.set(Some("Give the environment a name".into()));
            return;
        }
        match crate::environments::join(&Catalog::path(), name, target, joining) {
            Ok(environment) => self.go(Next::Select(environment)),
            Err(why) => self.error.set(Some(why)),
        }
    }
}

#[component]
fn Sheet() -> Element {
    let app = use_context::<AppCtx>();
    let adding = Adding {
        app,
        scope: current_scope_id(),
        catalog: use_signal(|| Catalog::load(&Catalog::path())),
        step: use_signal(|| Step::Where),
        error: use_signal(|| None),
        pairing: use_signal(|| None),
        mismatched: use_signal(|| false),
        task: use_signal(|| None),
    };
    use_hook(move || {
        if let Some((address, name)) = app.take_pair_again() {
            adding.go(Next::Step(Step::Pair { address, name }));
        }
    });
    let step = adding.step.read().clone();
    let (title, at) = match &step {
        Step::Where => ("Add environment".to_string(), 0),
        Step::Pair { name, .. } => (format!("Pair with {name}"), 1),
        Step::Join { .. } => ("Join or start one".to_string(), 2),
        Step::Name { .. } => ("Name it".to_string(), 3),
    };
    let error = adding.error.read().clone();
    rsx! {
        div { class: "dialog-scrim", onclick: move |_| adding.close(),
            div {
                class: "dialog adding-sheet",
                role: "dialog",
                aria_label: "Add environment",
                onclick: move |e| e.stop_propagation(),
                // A key on a button of the sheet is the sheet's, not the
                // window's; Escape there closes it as it does elsewhere.
                onkeydown: move |e: Event<KeyboardData>| {
                    if keys::of_field(&e).is_some_and(|k| k.key == "Escape") {
                        e.prevent_default();
                        adding.close();
                    }
                },
                div { class: "adding-head",
                    h3 { "{title}" }
                    div { class: "adding-steps",
                        for (i , label) in ["Where", "Pair", "Join", "Name"].into_iter().enumerate() {
                            span { class: if i == at { "on" } else { "" }, "{label}" }
                        }
                    }
                }
                div { class: "adding-body",
                    match step {
                        Step::Where => rsx! { Where { adding } },
                        Step::Pair { name, .. } => rsx! { Pair { adding, name } },
                        Step::Join { target, name, paired } => rsx! { Join { adding, target, name, paired } },
                        Step::Name { target, name, paired, joining } => rsx! { Name { adding, target, name, paired, joining } },
                    }
                    if let Some(why) = error {
                        p { class: "adding-error", role: "alert", "{why}" }
                    }
                }
            }
        }
    }
}

impl PartialEq for Adding {
    fn eq(&self, other: &Self) -> bool {
        self.step == other.step
    }
}

#[component]
fn Where(adding: Adding) -> Element {
    let mut heard_now = use_signal(Vec::<Heard>::new);
    let mut looking = use_signal(|| false);
    let mut address = use_signal(String::new);
    let mut look = move || {
        if *looking.peek() {
            return;
        }
        looking.set(true);
        let mut error = adding.error;
        spawn(async move {
            match adroit_connect::discover().await {
                Ok(found) => heard_now.set(found),
                Err(why) => error.set(Some(why)),
            }
            looking.set(false);
        });
    };
    use_hook(look);
    let catalog = adding.catalog.read().clone().unwrap_or_default();
    let machines: Vec<(Heard, Next)> = heard_now
        .read()
        .iter()
        .map(|found| (found.clone(), heard(&catalog, found)))
        .collect();
    let pair_typed = move || {
        let typed = address.peek().clone();
        if !typed.trim().is_empty() {
            let catalog = adding.catalog.peek().clone().unwrap_or_default();
            adding.go(self::typed(&catalog, &typed));
        }
    };
    rsx! {
        div { class: "adding-label",
            span { "On this network" }
            button {
                r#type: "button",
                class: if looking() { "sidebar-tool looking" } else { "sidebar-tool" },
                title: "Look again",
                aria_label: "Look for work machines again",
                disabled: looking(),
                onclick: move |_| look(),
                IconView { icon: Icon::Refresh, size: 13 }
            }
        }
        div { class: "adding-options", role: "list", aria_busy: looking(),
            for (found , next) in machines {
                button {
                    key: "{found.key}",
                    r#type: "button",
                    role: "listitem",
                    class: "adding-option",
                    onclick: {
                        let next = next.clone();
                        move |_| adding.go(next.clone())
                    },
                    IconView { icon: Icon::Server, size: 16 }
                    span { class: "adding-option-text",
                        "{found.name}"
                        small { "{found.address} · key {found.key}" }
                    }
                    span { class: "adding-option-go",
                        match next {
                            Next::Select(_) | Next::Move(_) => "Already added",
                            Next::Step(Step::Pair { .. }) => "Pair →",
                            Next::Step(Step::Join { .. }) => "Paired · Next →",
                            Next::Step(_) => "Next →",
                        }
                    }
                }
            }
            if heard_now.read().is_empty() {
                p { class: "adding-note",
                    if looking() { "Looking for work machines…" } else { "No work machine announced itself on this network. Type its address below instead." }
                }
            }
        }
        div { class: "adding-label", "Or by address" }
        div { class: "adding-address",
            input {
                class: "adding-input",
                r#type: "text",
                aria_label: "Work machine address",
                placeholder: "work-box.local:47130",
                spellcheck: "false",
                autocomplete: "off",
                "autocorrect": "off",
                "autocapitalize": "off",
                value: "{address}",
                oninput: move |e: Event<FormData>| address.set(e.value()),
                onkeydown: move |e: Event<KeyboardData>| {
                    if keys::of_field(&e).is_some_and(|k| k.key == "Enter") {
                        e.prevent_default();
                        pair_typed();
                    }
                },
            }
            Button { kind: "primary", onclick: move |_| pair_typed(), "Pair" }
        }
        div { class: "adding-label", "On this computer" }
        div { class: "adding-options",
            button {
                r#type: "button",
                class: "adding-option",
                onclick: move |_| adding.go(this_computer(&adding.catalog.peek().clone().unwrap_or_default())),
                IconView { icon: Icon::DeviceDesktop, size: 16 }
                span { class: "adding-option-text",
                    "This computer"
                    small { "Join an environment it serves, or start another" }
                }
                span { class: "adding-option-go", "Next →" }
            }
        }
        div { class: "dialog-actions",
            Button { onclick: move |_| adding.close(), "Cancel" }
        }
    }
}

#[component]
fn Pair(adding: Adding, name: String) -> Element {
    let pairing = adding.pairing.read().clone();
    let mismatched = *adding.mismatched.read();
    let waiting = adding.task.read().is_some();
    rsx! {
        match pairing {
            Some(PairingStep::Shown { code }) => rsx! {
                p { class: "adding-note", aria_live: "polite",
                    if mismatched { "That was not the code, so this is a new one. " }
                    "On {name}, type this code beside this computer's request."
                }
                div { class: "adding-code", "{code}" }
            },
            Some(PairingStep::NotAccepting) => rsx! {
                p { class: "adding-note", aria_live: "polite",
                    "{name} does not allow pairing now. On it, run "
                    code { "adroit remote pair" }
                    ", or allow pairing under Accept connections in its Settings."
                }
            },
            Some(PairingStep::Mismatched) => rsx! {
                p { class: "adding-note", aria_live: "polite", "That was not the code. Asking again with a new one…" }
            },
            None => rsx! {},
        }
        if waiting {
            p { class: "adding-note adding-waiting", span { class: "adding-spinner" } "Waiting for {name}…" }
        }
        div { class: "dialog-actions",
            Button { onclick: move |_| adding.back(), "Back" }
            Button { onclick: move |_| adding.close(), "Cancel" }
        }
    }
}

#[component]
fn Join(adding: Adding, target: Target, name: String, paired: Option<String>) -> Element {
    let mut listed = use_signal(|| None::<Result<Vec<ServedEnvironment>, String>>);
    use_hook({
        let target = target.clone();
        move || {
            spawn(async move {
                listed.set(Some(
                    crate::link::served(&target, crate::bridge::work_host).await,
                ));
            });
        }
    });
    let catalog = adding.catalog.read().clone().unwrap_or_default();
    let choices: Vec<(ServedEnvironment, Next)> = match &*listed.read() {
        Some(Ok(served)) => served
            .iter()
            .map(|s| (s.clone(), chosen(&catalog, &target, paired.as_deref(), s)))
            .collect(),
        _ => Vec::new(),
    };
    let new = Next::Step(Step::Name {
        target: target.clone(),
        name,
        paired: paired.clone(),
        joining: Joining::New,
    });
    let machine = name_of(&target);
    rsx! {
        if let Some(machine) = &paired {
            p { class: "adding-note adding-paired", IconView { icon: Icon::Check, size: 14 } "Paired with {machine}" }
        }
        div { class: "adding-label", "Running on {machine}" }
        div { class: "adding-options", role: "list", aria_busy: listed.read().is_none(),
            for (served , next) in choices {
                button {
                    key: "{served.id:?}",
                    r#type: "button",
                    role: "listitem",
                    class: "adding-option",
                    onclick: {
                        let next = next.clone();
                        move |_| adding.go(next.clone())
                    },
                    IconView { icon: Icon::Server, size: 16 }
                    span { class: "adding-option-text",
                        match (&served.name, &served.id) {
                            (Some(name), _) => rsx! { "{name}" },
                            (None, None) => rsx! { "{machine}'s own environment" },
                            (None, Some(_)) => rsx! { "An environment not named yet" },
                        }
                        small {
                            match served.running {
                                0 => "No sessions running".to_string(),
                                1 => "1 session running".to_string(),
                                n => format!("{n} sessions running"),
                            }
                        }
                    }
                    span { class: "adding-option-go",
                        match next {
                            Next::Select(_) => "Already added",
                            _ => "Join →",
                        }
                    }
                }
            }
            match &*listed.read() {
                None => rsx! {
                    p { class: "adding-note adding-waiting", span { class: "adding-spinner" } "Asking {machine} what it serves…" }
                },
                Some(Err(why)) => rsx! {
                    p { class: "adding-note", "Cannot list what {machine} serves: {why}" }
                },
                Some(Ok(_)) => rsx! {},
            }
        }
        div { class: "adding-label", "Or" }
        div { class: "adding-options",
            button {
                r#type: "button",
                class: "adding-option",
                onclick: move |_| adding.go(new.clone()),
                IconView { icon: Icon::Plus, size: 16 }
                span { class: "adding-option-text",
                    "New environment"
                    small { "Its own agents, settings and sessions, on the same machine" }
                }
                span { class: "adding-option-go", "Next →" }
            }
        }
        div { class: "dialog-actions",
            Button { onclick: move |_| adding.back(), "Back" }
            Button { onclick: move |_| adding.close(), "Cancel" }
        }
    }
}

#[component]
fn Name(
    adding: Adding,
    target: Target,
    name: String,
    paired: Option<String>,
    joining: Joining,
) -> Element {
    let mut typed = use_signal(|| name.clone());
    let target = use_signal(|| target.clone());
    let joining = use_signal(|| joining.clone());
    let add = move || adding.add(&typed.peek(), target.peek().clone(), joining.peek().clone());
    rsx! {
        if let Some(machine) = paired {
            p { class: "adding-note adding-paired", IconView { icon: Icon::Check, size: 14 } "Paired with {machine}" }
        }
        div { class: "adding-label", "Name" }
        input {
            class: "adding-input",
            r#type: "text",
            aria_label: "Environment name",
            spellcheck: "false",
            autocomplete: "off",
            "autocorrect": "off",
            "autocapitalize": "off",
            value: "{typed}",
            onmounted: move |e: MountedEvent| async move {
                let _ = e.data().set_focus(true).await;
            },
            oninput: move |e: Event<FormData>| typed.set(e.value()),
            onkeydown: move |e: Event<KeyboardData>| {
                if keys::of_field(&e).is_some_and(|k| k.key == "Enter") {
                    e.prevent_default();
                    add();
                }
            },
        }
        div { class: "dialog-actions",
            Button { onclick: move |_| adding.back(), "Back" }
            Button { kind: "primary", onclick: move |_| add(), "Add environment" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_protocol::ServedEnvironment;

    fn kept(id: &str, name: &str, address: &str, archived: bool) -> Environment {
        Environment {
            archived,
            ..Environment {
                id: id.into(),
                ..Environment::new(
                    name,
                    Target::Native {
                        address: address.into(),
                        server: format!("{id:0<64}"),
                    },
                )
            }
        }
    }

    fn catalog(environments: impl IntoIterator<Item = Environment>) -> Catalog {
        Catalog {
            environments: environments.into_iter().collect(),
            ..Catalog::default()
        }
    }

    fn found(name: &str, address: &str, key: &str) -> Heard {
        Heard {
            name: name.into(),
            address: address.into(),
            key: format!("{key:0<16}"),
        }
    }

    fn served(id: Option<&str>, name: Option<&str>) -> ServedEnvironment {
        ServedEnvironment {
            id: id.map(String::from),
            name: name.map(String::from),
            running: 0,
        }
    }

    #[test]
    fn a_machine_heard_with_an_environment_kept_lists_what_it_serves_and_one_unknown_is_paired() {
        let listed = kept("aa", "Box", "box.local:47130", false);
        let catalog = catalog([listed.clone()]);
        assert_eq!(
            heard(&catalog, &found("box", "10.0.0.2:47130", "aa")),
            Next::Step(Step::Join {
                target: listed.target,
                name: "Box 2".into(),
                paired: None
            })
        );
        assert_eq!(
            heard(&catalog, &found("gpu", "gpu.local:47130", "bb")),
            Next::Step(Step::Pair {
                address: "gpu.local:47130".into(),
                name: "gpu".into()
            })
        );
    }

    #[test]
    fn a_machine_whose_environment_was_removed_lists_what_it_serves_without_pairing() {
        let removed = kept("aa", "Box", "box.local:47130", true);
        assert_eq!(
            heard(
                &catalog([removed.clone()]),
                &found("box", "box.local:47130", "aa")
            ),
            Next::Step(Step::Join {
                target: removed.target,
                name: "Box".into(),
                paired: None
            })
        );
    }

    #[test]
    fn an_address_typed_is_paired_named_after_its_host_unless_an_environment_is_kept_for_it() {
        let listed = kept("aa", "Box", "box.local:47130", false);
        let catalog = catalog([listed.clone()]);
        assert!(matches!(
            typed(&catalog, " box.local:47130 "),
            Next::Step(Step::Join { .. })
        ));
        assert_eq!(
            typed(&catalog, "gpu.example.com:47130"),
            Next::Step(Step::Pair {
                address: "gpu.example.com:47130".into(),
                name: "gpu.example.com".into()
            })
        );
    }

    #[test]
    fn a_machine_paired_lists_what_it_serves() {
        let listed = kept("aa", "Box", "box.local:47130", false);
        let catalog = catalog([listed.clone()]);
        let new = Target::Native {
            address: "gpu.local:47130".into(),
            server: format!("{:0<64}", "bb"),
        };
        assert_eq!(
            paired(&catalog, new.clone(), "Box"),
            Next::Step(Step::Join {
                target: new,
                name: "Box 2".into(),
                paired: Some("gpu.local".into())
            })
        );
        assert!(matches!(
            paired(&catalog, listed.target.clone(), "box"),
            Next::Step(Step::Join { .. })
        ));
    }

    #[test]
    fn a_machine_paired_at_another_address_moves_its_listed_environment_there() {
        let listed = kept("aa", "Box", "box.local:47130", false);
        let moved = Target::Native {
            address: "10.0.0.2:47130".into(),
            server: format!("{:0<64}", "aa"),
        };
        assert_eq!(
            paired(&catalog([listed.clone()]), moved.clone(), "box"),
            Next::Move(Environment {
                target: moved,
                ..listed
            })
        );
    }

    #[test]
    fn this_computer_lists_what_its_work_host_serves() {
        let catalog = catalog([Environment::new("This computer", Target::Local)]);
        assert_eq!(
            this_computer(&catalog),
            Next::Step(Step::Join {
                target: Target::Local,
                name: "This computer 2".into(),
                paired: None
            })
        );
    }

    #[test]
    fn an_environment_served_and_kept_is_selected() {
        let listed = Environment {
            scope: Some("w".into()),
            ..kept("aa", "Work", "box.local:47130", false)
        };
        let catalog = catalog([listed.clone()]);
        assert_eq!(
            chosen(
                &catalog,
                &listed.target,
                None,
                &served(Some("w"), Some("Work"))
            ),
            Next::Select(listed)
        );
    }

    #[test]
    fn an_environment_served_and_not_kept_is_named_after_what_a_window_called_it() {
        let target = Target::Local;
        assert_eq!(
            chosen(&catalog([]), &target, None, &served(None, Some("Personal"))),
            Next::Step(Step::Name {
                target: target.clone(),
                name: "Personal".into(),
                paired: None,
                joining: Joining::Served(None),
            })
        );
        let removed = Environment {
            scope: Some("w".into()),
            ..kept("aa", "Old", "box.local:47130", true)
        };
        assert_eq!(
            chosen(
                &catalog([removed.clone()]),
                &removed.target,
                Some("box.local"),
                &served(Some("w"), None)
            ),
            Next::Step(Step::Name {
                target: removed.target.clone(),
                name: "Old".into(),
                paired: Some("box.local".into()),
                joining: Joining::Served(Some("w".into())),
            })
        );
    }

    #[test]
    fn an_environment_served_that_no_window_named_is_named_after_its_machine() {
        let target = Target::Native {
            address: "box.local:47130".into(),
            server: "ab".repeat(32),
        };
        let Next::Step(Step::Name { name, .. }) =
            chosen(&catalog([]), &target, None, &served(Some("w"), None))
        else {
            panic!("not named");
        };
        assert_eq!(name, "box.local");
    }
}
