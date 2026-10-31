use adroit_connect::Target;
use adroit_connect::environments::{Binding, Catalog, Environment};

#[test]
fn environments_on_the_same_machine_keep_distinct_work_and_client_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.toml");
    let mut catalog = Catalog::load(&path).unwrap();
    let a = Environment::new("Personal", Target::Local);
    let b = Environment::new("Work", Target::Local);
    assert_ne!(a.scope, b.scope);
    catalog.environments.extend([a.clone(), b.clone()]);
    catalog.save(&path, 0).unwrap();
    let loaded = Catalog::load(&path).unwrap();
    assert_eq!(loaded.environments, vec![a.clone(), b.clone()]);
    let first = a.bind("first").unwrap();
    let second = b.bind("second").unwrap();
    assert_ne!(first.scope, second.scope);
    assert_ne!(first.client, second.client);
    let state = dir.path().join("state");
    let legacy = dir.path().join("legacy");
    let first_home = first.local_data_dir(&state, &legacy).unwrap();
    let second_home = second.local_data_dir(&state, &legacy).unwrap();
    assert_ne!(first_home, second_home);
    assert_eq!(
        first_home,
        a.bind("independent-window")
            .unwrap()
            .local_data_dir(&state, &legacy)
            .unwrap()
    );
}

#[test]
fn stale_catalog_writes_do_not_replace_another_windows_environment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.toml");
    let mut first = Catalog::load(&path).unwrap();
    let mut stale = first.clone();
    first
        .environments
        .push(Environment::new("Work", Target::Local));
    first.save(&path, 0).unwrap();
    stale
        .environments
        .push(Environment::new("Lost", Target::Local));
    assert!(stale.save(&path, 0).is_err());
    assert_eq!(Catalog::load(&path).unwrap().environments[0].name, "Work");
}

#[test]
fn migrating_a_legacy_instance_preserves_its_active_target_and_pending_choice() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("instance");
    std::fs::create_dir_all(&state).unwrap();
    let remote = Target::Native {
        address: "work.local:47130".into(),
        server: "ab".repeat(32),
    };
    remote.save_next(&state.join("execution.toml")).unwrap();
    let mut catalog = Catalog::default();
    let binding = catalog.import_legacy("default", &state).unwrap();
    assert_eq!(binding.target, Target::Local);
    assert_eq!(binding.scope, None);
    assert_eq!(catalog.environments.len(), 2);
    assert_eq!(catalog.environments[1].target, remote);
    assert!(state.join("execution.next.toml").exists());
    assert_eq!(catalog.import_legacy("default", &state).unwrap(), binding);
    assert_eq!(catalog.environments.len(), 2);
}

#[test]
fn a_bound_instance_keeps_its_target_when_the_catalog_is_edited() {
    let mut environment = Environment::new("Work", Target::Local);
    let binding = environment.bind("client").unwrap();
    environment.target = Target::Native {
        address: "other.local:47130".into(),
        server: "ab".repeat(32),
    };
    assert_eq!(binding.target, Target::Local);
    assert_ne!(binding, environment.bind("client").unwrap());
    assert!(environment.bind("../escape").is_err());
}

#[test]
fn independent_windows_of_an_imported_local_environment_keep_the_original_work_service() {
    let mut environment = Environment::new("Existing", Target::Local);
    environment.scope = None;
    environment.legacy_client = Some("old-work".into());
    let first = environment.bind("old-work").unwrap();
    let other = environment.bind("another-window").unwrap();
    assert_eq!(first.work_instance, Some("old-work".into()));
    assert_eq!(first.work_instance, other.work_instance);
}

#[test]
fn a_pending_local_choice_keeps_its_original_work_service_without_reusing_its_client() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("old-work");
    std::fs::create_dir_all(&state).unwrap();
    Target::Native {
        address: "work.local:47130".into(),
        server: "ab".repeat(32),
    }
    .save(&state.join("execution.toml"))
    .unwrap();
    Target::Local
        .save_next(&state.join("execution.toml"))
        .unwrap();
    let mut catalog = Catalog::default();
    catalog.import_legacy("old-work", &state).unwrap();
    let saved = &catalog.environments[1];
    assert_eq!(saved.legacy_client, None);
    assert_eq!(
        saved
            .bind("another-window")
            .unwrap()
            .work_instance
            .as_deref(),
        Some("old-work")
    );
}

fn saved_catalog(path: &std::path::Path) -> Catalog {
    let mut catalog = Catalog::default();
    catalog.environments.extend([
        Environment::new("Personal", Target::Local),
        Environment::new(
            "Work",
            Target::Native {
                address: "work.local:47130".into(),
                server: "ab".repeat(32),
            },
        ),
    ]);
    catalog.save(path, 0).unwrap();
    catalog
}

fn as_profiles(text: &str) -> String {
    text.replace("[[environments]]", "[[profiles]]")
        .replace("[environments.", "[profiles.")
}

#[test]
fn the_catalog_is_written_with_environments_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.toml");
    saved_catalog(&path);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[[environments]]"), "{text}");
    assert!(!text.contains("profiles"), "{text}");
}

#[test]
fn a_catalog_written_with_profiles_entries_is_read_the_same_way() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.toml");
    let catalog = saved_catalog(&path);
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, as_profiles(&text)).unwrap();
    assert_eq!(Catalog::load(&path).unwrap(), catalog);
}

#[test]
fn a_catalog_naming_both_environments_and_profiles_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.toml");
    saved_catalog(&path);
    let text = std::fs::read_to_string(&path).unwrap();
    let entries = text.find("[[environments]]").unwrap();
    let both = format!("{text}\n{}", as_profiles(&text[entries..]));
    std::fs::write(&path, both).unwrap();
    let error = Catalog::load(&path).unwrap_err();
    assert!(
        error.contains("[[environments]]") && error.contains("[[profiles]]"),
        "{error}"
    );
}

#[test]
fn an_instance_binding_is_written_as_environment_toml() {
    let dir = tempfile::tempdir().unwrap();
    let environment = Environment::new("Work", Target::Local);
    let binding = environment.bind("client").unwrap();
    binding.save(dir.path()).unwrap();
    let text = std::fs::read_to_string(dir.path().join("environment.toml")).unwrap();
    assert!(
        text.contains(&format!("environment = \"{}\"", environment.id)),
        "{text}"
    );
    assert!(!dir.path().join("profile.toml").exists());
    assert_eq!(Binding::load(dir.path()).unwrap(), Some(binding));
}

#[test]
fn an_instance_bound_in_profile_toml_keeps_its_binding() {
    let dir = tempfile::tempdir().unwrap();
    let environment = Environment::new("Work", Target::Local);
    let binding = environment.bind("client").unwrap();
    binding.save(dir.path()).unwrap();
    let written = dir.path().join("environment.toml");
    let text = std::fs::read_to_string(&written).unwrap();
    std::fs::remove_file(&written).unwrap();
    std::fs::write(
        dir.path().join("profile.toml"),
        text.replace("environment = ", "profile = "),
    )
    .unwrap();
    assert_eq!(Binding::load(dir.path()).unwrap(), Some(binding.clone()));
    binding.save(dir.path()).unwrap();
    assert!(!written.exists(), "an existing binding is never rewritten");
    let other = Environment::new("Other", Target::Local)
        .bind("client")
        .unwrap();
    assert!(other.save(dir.path()).is_err());
}
