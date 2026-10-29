use adroit_connect::{Target, execution_path};

fn remote() -> Target {
    Target::Native {
        address: "work-box.local:47130".into(),
        server: "ab".repeat(32),
    }
}

#[test]
fn each_client_instance_keeps_its_own_connection_choice() {
    let dir = tempfile::tempdir().unwrap();
    let default = execution_path(dir.path(), "default").unwrap();
    let other = execution_path(dir.path(), "research").unwrap();
    remote().save(&default).unwrap();
    assert_eq!(Target::load(&default).unwrap(), remote());
    assert_eq!(Target::load(&other).unwrap(), Target::Local);
    Target::Local.save(&other).unwrap();
    assert_eq!(Target::load(&default).unwrap(), remote());
    assert_eq!(Target::load(&other).unwrap(), Target::Local);
    Target::Local.save(&default).unwrap();
    assert_eq!(Target::load(&default).unwrap(), Target::Local);
}

#[test]
fn an_invalid_choice_does_not_replace_the_existing_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = execution_path(dir.path(), "default").unwrap();
    remote().save(&path).unwrap();
    let mut invalid = remote();
    if let Target::Native { address, .. } = &mut invalid {
        *address = "-oProxyCommand=bad:1".into();
    }
    assert!(invalid.save(&path).is_err());
    assert_eq!(Target::load(&path).unwrap(), remote());
    assert!(execution_path(dir.path(), "../other").is_err());
}

#[test]
fn a_choice_too_large_to_read_cannot_replace_a_working_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let path = execution_path(dir.path(), "default").unwrap();
    remote().save(&path).unwrap();
    let mut large = remote();
    if let Target::Native { address, .. } = &mut large {
        *address = format!("{}:1", "a".repeat(65536));
    }
    assert!(large.save(&path).is_err());
    assert_eq!(Target::load(&path).unwrap(), remote());
}

#[test]
fn a_saved_next_choice_leaves_running_cli_routes_unchanged_until_activation() {
    let dir = tempfile::tempdir().unwrap();
    let active = execution_path(dir.path(), "default").unwrap();
    remote().save_next(&active).unwrap();
    assert_eq!(Target::load(&active).unwrap(), Target::Local);
    assert_eq!(adroit_connect::saved_choice(&active).unwrap(), remote());
    adroit_connect::activate_saved(&active).unwrap();
    assert_eq!(Target::load(&active).unwrap(), remote());
    Target::Local.save_next(&active).unwrap();
    assert_eq!(Target::load(&active).unwrap(), remote());
    adroit_connect::activate_saved(&active).unwrap();
    assert_eq!(Target::load(&active).unwrap(), Target::Local);
    adroit_connect::activate_saved(&active).unwrap();
    assert_eq!(Target::load(&active).unwrap(), Target::Local);
}

#[test]
fn a_paired_work_machine_is_kept_by_its_address_and_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = execution_path(dir.path(), "default").unwrap();
    let paired = Target::Native {
        address: "work-box.local:47130".into(),
        server: "ab".repeat(32),
    };
    paired.save(&path).unwrap();
    assert_eq!(Target::load(&path).unwrap(), paired);
    for address in ["192.168.1.5:47130", "[fe80::1]:47130", "work-box:1"] {
        let target = Target::Native {
            address: address.into(),
            server: "ab".repeat(32),
        };
        assert!(target.validate().is_ok(), "{address}");
    }
}

#[test]
fn a_paired_work_machine_needs_a_port_and_a_whole_key_fingerprint() {
    for (address, server) in [
        ("work-box", "ab".repeat(32)),
        ("work-box:0", "ab".repeat(32)),
        ("-oProxyCommand=x:1", "ab".repeat(32)),
        ("work-box:47130", "ab".repeat(31)),
        ("work-box:47130", "AB".repeat(32)),
    ] {
        let target = Target::Native {
            address: address.into(),
            server,
        };
        assert!(target.validate().is_err(), "{address}");
    }
}
