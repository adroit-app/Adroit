//! The sheet adding an environment over the main window: where it works,
//! pairing with a work machine not paired yet, and its name.

use adroit_connect::environments::{Catalog, Environment};
use adroit_connect::{Heard, PairingStep, Target};
use adroit_core::msg::Msg;
use dioxus::core::{Runtime, ScopeId, Task, current_scope_id};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::Button;
use crate::environments::{fresh_name, machine_name, of_machine};
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
    /// Naming the environment for `target`; `paired` names the work machine
    /// paired with on the way.
    Name {
        target: Target,
        name: String,
        paired: Option<String>,
    },
}

/// Where a choice leads: to a step, or to an environment kept already.
#[derive(Debug, Clone, PartialEq)]
enum Next {
    Step(Step),
    Select(Environment),
}

/// Where the work machine `heard` on the network leads: its environment
/// when one is listed, its name when it was paired before, or else pairing.
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
/// leads: its environment when one is listed, or else naming one.
fn paired(catalog: &Catalog, target: Target, name: &str) -> Next {
    let named = |name: String| {
        Next::Step(Step::Name {
            target: target.clone(),
            name,
            paired: Some(name_of(&target)),
        })
    };
    match &target {
        Target::Native { server, .. } => match of_machine(catalog, |_, kept| kept == server) {
            Some(kept) if !kept.archived => Next::Select(kept.clone()),
            Some(kept) => named(fresh_name(catalog, &kept.name)),
            None => named(fresh_name(catalog, name)),
        },
        Target::Local => named(fresh_name(catalog, name)),
    }
}

/// Naming another environment on this computer.
fn this_computer(catalog: &Catalog) -> Next {
    Next::Step(Step::Name {
        target: Target::Local,
        name: fresh_name(catalog, "This computer"),
        paired: None,
    })
}

/// A work machine `kept` already, or else `pairing`.
fn machine(catalog: &Catalog, kept: Option<&Environment>, pairing: Step) -> Next {
    match kept {
        Some(kept) if !kept.archived => Next::Select(kept.clone()),
        Some(kept) => Next::Step(Step::Name {
            target: kept.target.clone(),
            name: fresh_name(catalog, &kept.name),
            paired: None,
        }),
        None => Next::Step(pairing),
    }
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
                            Ok(catalog) => self.go(paired(&catalog, target, &name)),
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

    fn add(mut self, name: &str, target: Target) {
        if name.trim().is_empty() {
            self.error.set(Some("Give the environment a name".into()));
            return;
        }
        match crate::environments::add(&Catalog::path(), name, target) {
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
    let step = adding.step.read().clone();
    let (title, at) = match &step {
        Step::Where => ("Add environment".to_string(), 0),
        Step::Pair { name, .. } => (format!("Pair with {name}"), 1),
        Step::Name { .. } => ("Name it".to_string(), 2),
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
                        for (i , label) in ["Where", "Pair", "Name"].into_iter().enumerate() {
                            span { class: if i == at { "on" } else { "" }, "{label}" }
                        }
                    }
                }
                div { class: "adding-body",
                    match step {
                        Step::Where => rsx! { Where { adding } },
                        Step::Pair { name, .. } => rsx! { Pair { adding, name } },
                        Step::Name { target, name, paired } => rsx! { Name { adding, target, name, paired } },
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
                            Next::Select(_) => "Already added",
                            Next::Step(Step::Pair { .. }) => "Pair →",
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
                    "Another environment on this computer"
                    small { "Separate agents and settings, same machine" }
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
                    ", or turn on Accept connections and allow pairing."
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
fn Name(adding: Adding, target: Target, name: String, paired: Option<String>) -> Element {
    let mut typed = use_signal(|| name.clone());
    let target = use_signal(|| target.clone());
    let add = move || adding.add(&typed.peek(), target.peek().clone());
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

    #[test]
    fn a_machine_heard_with_an_environment_listed_selects_it_and_one_unknown_is_paired() {
        let listed = kept("aa", "Box", "box.local:47130", false);
        let catalog = catalog([listed.clone()]);
        assert_eq!(
            heard(&catalog, &found("box", "10.0.0.2:47130", "aa")),
            Next::Select(listed)
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
    fn a_machine_whose_environment_was_removed_is_named_again_without_pairing() {
        let removed = kept("aa", "Box", "box.local:47130", true);
        assert_eq!(
            heard(
                &catalog([removed.clone()]),
                &found("box", "box.local:47130", "aa")
            ),
            Next::Step(Step::Name {
                target: removed.target,
                name: "Box".into(),
                paired: None
            })
        );
    }

    #[test]
    fn an_address_typed_is_paired_named_after_its_host_unless_its_environment_is_listed() {
        let listed = kept("aa", "Box", "box.local:47130", false);
        let catalog = catalog([listed.clone()]);
        assert_eq!(typed(&catalog, " box.local:47130 "), Next::Select(listed));
        assert_eq!(
            typed(&catalog, "gpu.example.com:47130"),
            Next::Step(Step::Pair {
                address: "gpu.example.com:47130".into(),
                name: "gpu.example.com".into()
            })
        );
    }

    #[test]
    fn a_machine_paired_is_named_unless_an_environment_for_it_is_listed() {
        let listed = kept("aa", "Box", "box.local:47130", false);
        let catalog = catalog([listed.clone()]);
        let again = Target::Native {
            address: "10.0.0.2:47130".into(),
            server: format!("{:0<64}", "aa"),
        };
        assert_eq!(paired(&catalog, again, "box"), Next::Select(listed));
        let new = Target::Native {
            address: "gpu.local:47130".into(),
            server: format!("{:0<64}", "bb"),
        };
        assert_eq!(
            paired(&catalog, new.clone(), "Box"),
            Next::Step(Step::Name {
                target: new,
                name: "Box 2".into(),
                paired: Some("gpu.local".into())
            })
        );
    }

    #[test]
    fn another_environment_on_this_computer_is_named_after_it() {
        let catalog = catalog([Environment::new("This computer", Target::Local)]);
        assert_eq!(
            this_computer(&catalog),
            Next::Step(Step::Name {
                target: Target::Local,
                name: "This computer 2".into(),
                paired: None
            })
        );
    }
}
