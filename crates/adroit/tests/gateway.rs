//! The gateway in front of a work machine's hosts: pairing with it, and
//! reaching and finding the hosts through it.
#![cfg(unix)]

use std::time::Duration;

mod support {
    pub mod isolated;
}

use support::isolated::{Hosts, Service, isolated, remote_info};

struct Gatewayed {
    _services: Vec<Service>,
    info: serde_json::Value,
    address: String,
    server: String,
    /// What the gateway wrote to its standard error.
    log: std::path::PathBuf,
}

/// Both remote hosts and a gateway in front of them, on a free local port.
fn gatewayed(hosts: &Hosts, advertise: bool) -> Gatewayed {
    let _terminal = Service::start(hosts, "daemon");
    let record = hosts
        .0
        .path()
        .join("state/adroit/instances/headless-test/remote/terminal.json");
    for _ in 0..1000 {
        if record.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _work = Service::start(hosts, "host");
    let info = remote_info(hosts);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let address = format!("127.0.0.1:{port}");
    // An announcement names the machine's addresses on the interfaces that
    // multicast, which loopback does not everywhere, so an announced
    // gateway listens on them all.
    let listen = if advertise {
        format!("0.0.0.0:{port}")
    } else {
        address.clone()
    };
    let log = hosts.0.path().join("gateway.log");
    let _gateway = Service(
        isolated(hosts.0.path())
            .args(["remote", "gateway", "--listen", &listen])
            .args((!advertise).then_some("--no-advertise"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .unwrap(),
    );
    let server = (0..250)
        .find_map(|_| {
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            let found = text
                .lines()
                .find(|line| line.contains("gateway listening"))
                .and_then(|line| {
                    line.split_whitespace()
                        .find_map(|word| word.strip_prefix("server="))
                })
                .map(str::to_string);
            if found.is_none() {
                std::thread::sleep(Duration::from_millis(20));
            }
            found
        })
        .unwrap_or_else(|| {
            panic!(
                "the gateway did not start: {}",
                std::fs::read_to_string(&log).unwrap_or_default()
            )
        });
    Gatewayed {
        _services: vec![_terminal, _work, _gateway],
        info,
        address,
        server,
        log,
    }
}

#[test]
fn a_paired_client_reaches_the_hosts_through_the_gateway_and_leaving_keeps_their_sessions() {
    let hosts = Hosts::new();
    let Gatewayed {
        _services,
        info,
        address,
        server,
        ..
    } = gatewayed(&hosts, false);
    let paired_key = hosts.0.path().join("client/paired.key");
    let client = adroit_net::Identity::load_or_create(&paired_key).unwrap();
    adroit_net::Enroll::enroll(
        &adroit_net::PeerFile::open(
            hosts
                .0
                .path()
                .join("state/adroit/instances/headless-test/remote/peers.json"),
        )
        .unwrap(),
        client.fingerprint(),
        "test client",
    )
    .unwrap();
    let target = adroit_connect::Target::Native {
        address: address.clone(),
        server: server.clone(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let remote = adroit_connect::Remote::with_key(&target, &paired_key).unwrap();
        let tunnel = remote.connect(None).await.unwrap();
        assert_eq!(tunnel.info().instance, info["instance"].as_str().unwrap());
        assert_eq!(
            tunnel.info().terminal.generation,
            info["terminal"]["generation"].as_str().unwrap()
        );
        let connection = adroit_protocol::client::Connection::open_unscoped(&remote.work_socket())
            .await
            .unwrap();
        assert_eq!(
            connection.hello.endpoint.as_ref(),
            Some(&tunnel.info().work)
        );
        let id = adroit_term::Daemon::new(info["terminal"]["socket"].as_str().unwrap())
            .spawn(adroit_term::Spawn {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), "printf alive; read line".into()],
                cwd: hosts.0.path().into(),
                cols: 40,
                rows: 6,
                ..Default::default()
            })
            .await
            .unwrap();
        let listed = |sessions: Vec<adroit_term::Session>| sessions.iter().any(|s| s.id == id);
        assert!(listed(remote.terminal().sessions().await.unwrap()));
        drop(connection);
        let seen = tunnel.info().clone();
        drop(tunnel);
        let again = remote.connect(Some(&seen)).await.unwrap();
        assert_eq!(again.info(), &seen);
        assert!(listed(remote.terminal().sessions().await.unwrap()));

        let stranger =
            adroit_connect::Remote::with_key(&target, &hosts.0.path().join("client/stranger.key"))
                .unwrap();
        let refused = stranger.connect(None).await.err().unwrap();
        assert!(refused.contains("refused this client's key"), "{refused}");
    });
}

/// Lines a child prints, read on a thread of their own.
fn lines_of(output: impl std::io::Read + Send + 'static) -> std::sync::mpsc::Receiver<String> {
    let (send, lines) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(output).lines() {
            let Ok(line) = line else { break };
            if send.send(line).is_err() {
                break;
            }
        }
    });
    lines
}

fn line_with(lines: &std::sync::mpsc::Receiver<String>, needle: &str) -> String {
    loop {
        let line = lines
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("no line with {needle:?}"));
        if line.contains(needle) {
            return line;
        }
    }
}

#[test]
fn the_cli_pairs_a_client_by_the_code_it_shows_and_saves_an_environment() {
    use std::io::Write;
    use std::process::Stdio;
    let hosts = Hosts::new();
    let gateway = gatewayed(&hosts, false);
    let mut client = isolated(hosts.0.path())
        .args(["connect", "pair", &gateway.address, "--name", "laptop"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let client_lines = lines_of(client.stdout.take().unwrap());
    // Asked before the operator allows pairing, the client waits for it.
    line_with(&client_lines, "not accepting pairing requests");

    let mut operator = isolated(hosts.0.path())
        .args(["remote", "pair"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let operator_lines = lines_of(operator.stdout.take().unwrap());
    let shown = line_with(&client_lines, "Type this code");
    let code = shown.split_whitespace().last().unwrap().to_string();
    let request = line_with(&operator_lines, "Request from");
    assert!(request.contains("\"laptop\""), "{request}");
    let mut typing = operator.stdin.take().unwrap();
    writeln!(typing, "{code}").unwrap();
    line_with(&operator_lines, "Paired");
    assert!(operator.wait().unwrap().success());
    line_with(&client_lines, "added the environment");
    assert!(client.wait().unwrap().success());

    let catalog =
        std::fs::read_to_string(hosts.0.path().join("config/adroit/environments.toml")).unwrap();
    assert!(catalog.contains("backend = \"native\""), "{catalog}");
    assert!(catalog.contains(&gateway.server), "{catalog}");
    let devices = isolated(hosts.0.path())
        .args(["remote", "devices"])
        .output()
        .unwrap();
    let devices = String::from_utf8_lossy(&devices.stdout);
    assert!(devices.contains("laptop"), "{devices}");
}

/// Whether the gateway listening at the loopback `address` answers there but
/// at none of the other addresses it is announced at.
async fn answers_only_on_loopback(
    client: &adroit_net::Identity,
    server: adroit_net::Fingerprint,
    address: &str,
) -> bool {
    let answers = |address: String| async move {
        let remote = adroit_net::Remote::new(client.clone(), server, address);
        matches!(
            tokio::time::timeout(Duration::from_secs(3), remote.info()).await,
            Ok(Ok(_))
        )
    };
    if !answers(address.to_string()).await {
        return false;
    }
    let heard = adroit_net::browse(Duration::from_secs(3))
        .await
        .unwrap_or_default();
    let elsewhere: Vec<_> = heard
        .into_iter()
        .filter(|found| found.server == server)
        .flat_map(|found| found.addresses)
        .filter(|announced| !announced.ip().is_loopback())
        .collect();
    if elsewhere.is_empty() {
        return false;
    }
    for announced in elsewhere {
        if answers(announced.to_string()).await {
            return false;
        }
    }
    true
}

#[test]
fn a_paired_work_machine_is_found_on_the_local_network_when_its_address_changed() {
    // Browsing listens on every interface from this test executable, and a
    // Mac's application firewall asks about each build of it anew.
    if std::env::var_os("ADROIT_TEST_LAN").is_none() {
        eprintln!("not tested on the local network unless ADROIT_TEST_LAN is set");
        return;
    }
    let hosts = Hosts::new();
    let gateway = gatewayed(&hosts, true);
    let key = hosts.0.path().join("client/paired.key");
    let client = adroit_net::Identity::load_or_create(&key).unwrap();
    adroit_net::Enroll::enroll(
        &adroit_net::PeerFile::open(
            hosts
                .0
                .path()
                .join("state/adroit/instances/headless-test/remote/peers.json"),
        )
        .unwrap(),
        client.fingerprint(),
        "test client",
    )
    .unwrap();
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let target = adroit_connect::Target::Native {
        address: closed.to_string(),
        server: gateway.server.clone(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let discover = || {
        let heard = isolated(hosts.0.path())
            .args(["connect", "discover"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&heard.stdout).into_owned()
    };
    let reached = runtime.block_on(async {
        let remote = adroit_connect::Remote::with_key(&target, &key).unwrap();
        let tunnel = match remote.connect(None).await {
            Ok(tunnel) => tunnel,
            Err(error) => {
                let said = std::fs::read_to_string(&gateway.log).unwrap_or_default();
                // As on a macOS CI runner, which keeps a program from the
                // local network with no one to allow it.
                if said.contains("cannot announce itself on the local network") {
                    eprintln!("not tested where the gateway may not announce: {said}");
                    return false;
                }
                // As on a Mac whose local network privacy keeps the gateway
                // a freshly built executable runs from answering anywhere
                // but the loopback, while its announcements still go out.
                let server = gateway.server.parse().unwrap();
                if answers_only_on_loopback(&client, server, &gateway.address).await {
                    eprintln!("not tested where the gateway may answer only on the loopback");
                    return false;
                }
                panic!(
                    "{error}\nthe local network heard: {}\nthe gateway said: {said}",
                    discover()
                )
            }
        };
        assert_eq!(
            tunnel.info().instance,
            gateway.info["instance"].as_str().unwrap()
        );
        // Reached at whichever of the machine's addresses was announced.
        let reached: std::net::SocketAddr = tunnel.reached().parse().unwrap();
        let listening: std::net::SocketAddr = gateway.address.parse().unwrap();
        assert_eq!(reached.port(), listening.port(), "{reached}");
        true
    });
    if !reached {
        return;
    }
    let heard = discover();
    assert!(heard.contains(&gateway.server[..16]), "{heard}");
}

#[test]
fn a_gateway_starts_the_instance_s_hosts_where_none_run_and_leaves_them_running_when_it_stops() {
    let hosts = Hosts::new();
    let gateway = Service(
        isolated(hosts.0.path())
            .args([
                "remote",
                "gateway",
                "--listen",
                "127.0.0.1:0",
                "--no-advertise",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let info = remote_info(&hosts);
    drop(gateway);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(remote_info(&hosts)["work"]["pid"], info["work"]["pid"]);
    assert!(hosts.0.path().join("work.sock").exists());
}
