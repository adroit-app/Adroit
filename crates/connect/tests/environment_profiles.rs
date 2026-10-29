use adroit_connect::Target;
use adroit_connect::profiles::{Catalog, Profile};

#[test]
fn profiles_on_the_same_machine_keep_distinct_work_and_client_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.toml");
    let mut catalog = Catalog::load(&path).unwrap();
    let a = Profile::new("Personal", Target::Local);
    let b = Profile::new("Work", Target::Local);
    assert_ne!(a.scope, b.scope);
    catalog.profiles.extend([a.clone(), b.clone()]);
    catalog.save(&path, 0).unwrap();
    let loaded = Catalog::load(&path).unwrap();
    assert_eq!(loaded.profiles, vec![a.clone(), b.clone()]);
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
fn stale_catalog_writes_do_not_replace_another_windows_profile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.toml");
    let mut first = Catalog::load(&path).unwrap();
    let mut stale = first.clone();
    first.profiles.push(Profile::new("Work", Target::Local));
    first.save(&path, 0).unwrap();
    stale.profiles.push(Profile::new("Lost", Target::Local));
    assert!(stale.save(&path, 0).is_err());
    assert_eq!(Catalog::load(&path).unwrap().profiles[0].name, "Work");
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
    assert_eq!(catalog.profiles.len(), 2);
    assert_eq!(catalog.profiles[1].target, remote);
    assert!(state.join("execution.next.toml").exists());
    assert_eq!(catalog.import_legacy("default", &state).unwrap(), binding);
    assert_eq!(catalog.profiles.len(), 2);
}

#[test]
fn a_bound_instance_keeps_its_target_when_the_catalog_is_edited() {
    let mut profile = Profile::new("Work", Target::Local);
    let binding = profile.bind("client").unwrap();
    profile.target = Target::Native {
        address: "other.local:47130".into(),
        server: "ab".repeat(32),
    };
    assert_eq!(binding.target, Target::Local);
    assert_ne!(binding, profile.bind("client").unwrap());
    assert!(profile.bind("../escape").is_err());
}

#[test]
fn independent_windows_of_an_imported_local_profile_keep_the_original_work_service() {
    let mut profile = Profile::new("Existing", Target::Local);
    profile.scope = None;
    profile.legacy_client = Some("old-work".into());
    let first = profile.bind("old-work").unwrap();
    let other = profile.bind("another-window").unwrap();
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
    let saved = &catalog.profiles[1];
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
