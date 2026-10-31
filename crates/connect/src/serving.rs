//! This computer served as a work machine: its services, its gateway and the
//! clients paired with it, as the operator at its desktop manages them.

use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_net::{Admin, AdminAnswer, Fingerprint, GatewayFiles, PAIRING_PERIOD, PeerFile};

/// The instance whose hosts and gateway serve this computer to others.
const SERVED_INSTANCE: &str = adroit_model::paths::DEFAULT_INSTANCE;

/// The gateway serving this computer, as it answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Serving {
    /// Its key's fingerprint, which a client pins.
    pub server: String,
    pub listening: Vec<String>,
    /// Addresses another computer can type to reach it.
    pub addresses: Vec<String>,
    /// Whether it accepts pairing requests now.
    pub pairing: bool,
    /// Whether it announces itself on the local network.
    pub announcing: bool,
}

/// A client asking to pair, waiting for the code it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asking {
    pub id: u64,
    pub name: String,
    pub fingerprint: String,
    /// Paired already: pairing again renews it.
    pub paired: bool,
    /// This computer itself asks, with the key it pairs with as a client.
    pub this_computer: bool,
}

/// A client paired with this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Client {
    pub name: String,
    pub fingerprint: String,
    /// Seconds since the Unix epoch.
    pub paired_at: u64,
    /// This computer itself, paired as a client of its own gateway.
    pub this_computer: bool,
}

/// How long a gateway just started has to answer before turning it on is
/// told to have failed: launchd starts a service that exited again only
/// after ten seconds.
const STARTING: Duration = Duration::from_secs(12);

/// The gateway of one instance on this computer, and the files it keeps.
#[derive(Clone, Debug)]
pub struct ThisComputer {
    files: GatewayFiles,
    /// Where the gateway's service writes what it says on failing.
    log: PathBuf,
    /// The key this computer pairs with as a client, when it has one.
    own: Option<String>,
}

impl ThisComputer {
    /// The instance this computer serves, whichever instance's window asks:
    /// a machine has one gateway on its port, and a window of an environment that
    /// reaches this very computer is a client of it, not another server.
    pub fn current() -> Self {
        let key = crate::client_key();
        let own = key
            .exists()
            .then(|| adroit_net::Identity::load_or_create(&key).ok())
            .flatten()
            .map(|identity| identity.fingerprint().to_string());
        let served =
            adroit_model::paths::instance_dir(&adroit_model::paths::state_dir(), SERVED_INSTANCE);
        Self {
            own,
            ..Self::in_dir(adroit_model::paths::remote_dir(&served))
        }
    }

    pub fn in_dir(dir: impl AsRef<Path>) -> Self {
        Self {
            files: GatewayFiles::in_dir(&dir),
            log: dir.as_ref().join("gateway.log"),
            own: None,
        }
    }

    /// As [`ThisComputer::in_dir`], pairing as a client with `own`.
    pub fn in_dir_as(dir: impl AsRef<Path>, own: impl Into<String>) -> Self {
        Self {
            own: Some(own.into()),
            ..Self::in_dir(dir)
        }
    }

    fn is_own(&self, fingerprint: &str) -> bool {
        self.own.as_deref() == Some(fingerprint)
    }

    async fn ask(&self, admin: Admin) -> Result<AdminAnswer, String> {
        match adroit_net::ask_gateway(&self.files.admin, &admin).await {
            Ok(AdminAnswer::Failed(text)) => Err(text),
            Ok(answer) => Ok(answer),
            Err(_) => Err("This computer is not accepting connections".into()),
        }
    }

    /// The gateway running for this instance; none when it is not.
    pub async fn serving(&self) -> Option<Serving> {
        match self.ask(Admin::Status).await {
            Ok(AdminAnswer::Status {
                server,
                listening,
                open,
                announcing,
            }) => Some(Serving {
                server: server.to_string(),
                addresses: reachable_at(&listening, &adroit_net::host_name(), &local_addresses()),
                listening,
                pairing: open,
                announcing,
            }),
            _ => None,
        }
    }

    /// Registers and starts the services as [`enable`] does, and waits for
    /// the gateway to answer: a service that started and exited at once
    /// leaves nothing serving.
    pub async fn serve(&self, advertise: bool) -> Result<Vec<String>, String> {
        let since = self.said();
        let notes = enable(advertise).await?;
        self.answering(since, STARTING).await?;
        Ok(notes)
    }

    /// How much the gateway's service has said so far.
    fn said(&self) -> u64 {
        std::fs::metadata(&self.log).map_or(0, |log| log.len())
    }

    /// The gateway once it answers within `within`; otherwise what its
    /// service said after `since`, the last thing first.
    async fn answering(&self, since: u64, within: Duration) -> Result<Serving, String> {
        let deadline = tokio::time::Instant::now() + within;
        loop {
            if let Ok(Some(serving)) = tokio::time::timeout_at(deadline, self.serving()).await {
                return Ok(serving);
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let said = std::fs::read(&self.log).unwrap_or_default();
        let said = String::from_utf8_lossy(said.get(since as usize..).unwrap_or_default());
        Err(match said.lines().rev().map(str::trim).find(|l| !l.is_empty()) {
            Some(last) if last.contains("Address already in use") => format!(
                "This computer did not start accepting connections: {}. Another program, or adroit serving this computer from another instance, holds the gateway's port.",
                last.trim_start_matches("adroit: ")
            ),
            Some(last) => format!(
                "This computer did not start accepting connections: {}",
                last.trim_start_matches("adroit: ")
            ),
            None => "This computer did not start accepting connections, and its gateway did not say why".into(),
        })
    }

    /// Lets clients ask to pair for a while.
    pub async fn open_pairing(&self) -> Result<(), String> {
        self.ask(Admin::Open {
            seconds: PAIRING_PERIOD.as_secs(),
        })
        .await
        .map(drop)
    }

    pub async fn close_pairing(&self) -> Result<(), String> {
        self.ask(Admin::Close).await.map(drop)
    }

    /// The clients waiting for the operator now.
    pub async fn asking(&self) -> Result<Vec<Asking>, String> {
        match self.ask(Admin::Requests).await? {
            AdminAnswer::Requests(waiting) => Ok(waiting
                .into_iter()
                .map(|waiting| Asking {
                    this_computer: self.is_own(&waiting.client.to_string()),
                    id: waiting.id,
                    name: waiting.name,
                    fingerprint: waiting.client.to_string(),
                    paired: waiting.paired,
                })
                .collect()),
            _ => Err("The gateway answered something else".into()),
        }
    }

    /// Pairs the client asking as `id` when `code` is the one it shows.
    pub async fn approve(&self, id: u64, code: &str) -> Result<(), String> {
        let code = code.trim().to_string();
        match self.ask(Admin::Approve { id, code }).await? {
            AdminAnswer::Paired(_) => Ok(()),
            _ => Err("The gateway answered something else".into()),
        }
    }

    pub async fn refuse(&self, id: u64) -> Result<(), String> {
        self.ask(Admin::Reject { id }).await.map(drop)
    }

    /// The clients paired with this computer.
    pub fn clients(&self) -> Result<Vec<Client>, String> {
        let peers = PeerFile::open(&self.files.peers).map_err(|e| e.to_string())?;
        Ok(peers
            .list()
            .into_iter()
            .map(|paired| Client {
                this_computer: self.is_own(&paired.fingerprint.to_string()),
                name: paired.name,
                fingerprint: paired.fingerprint.to_string(),
                paired_at: paired.paired_at,
            })
            .collect())
    }

    /// Forgets a paired client and ends its connections.
    pub async fn revoke(&self, fingerprint: &str) -> Result<(), String> {
        let fingerprint: Fingerprint = fingerprint.parse().map_err(|_| "Not a fingerprint")?;
        adroit_net::revoke_paired(&self.files, fingerprint)
            .await
            .map_err(|e| e.to_string())
    }
}

/// What another computer types to reach a gateway `listening` there, on
/// `host` with the interface addresses `locals`: one listening on every
/// interface is reached by the host's name and each address.
fn reachable_at(listening: &[String], host: &str, locals: &[std::net::IpAddr]) -> Vec<String> {
    let mut reachable: Vec<String> = Vec::new();
    for address in listening {
        let Ok(address) = address.parse::<std::net::SocketAddr>() else {
            continue;
        };
        let port = address.port();
        let here = match address.ip().is_unspecified() {
            true => std::iter::once(format!("{host}.local:{port}"))
                .chain(
                    locals
                        .iter()
                        .map(|ip| std::net::SocketAddr::new(*ip, port).to_string()),
                )
                .collect(),
            false => vec![address.to_string()],
        };
        for address in here {
            if !reachable.contains(&address) {
                reachable.push(address);
            }
        }
    }
    reachable
}

/// This computer's addresses another computer can reach: neither loopback
/// nor link-local, IPv4 first.
fn local_addresses() -> Vec<std::net::IpAddr> {
    let mut addresses = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `list`, freed below; each entry is read only
    // while the list lives.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return addresses;
        }
        let mut entry = list;
        while let Some(interface) = entry.as_ref() {
            entry = interface.ifa_next;
            let Some(address) = interface.ifa_addr.as_ref() else {
                continue;
            };
            let ip = match i32::from(address.sa_family) {
                libc::AF_INET => {
                    let v4 = &*(interface.ifa_addr as *const libc::sockaddr_in);
                    std::net::IpAddr::from(u32::from_be(v4.sin_addr.s_addr).to_be_bytes())
                }
                libc::AF_INET6 => {
                    let v6 = &*(interface.ifa_addr as *const libc::sockaddr_in6);
                    std::net::IpAddr::from(v6.sin6_addr.s6_addr)
                }
                _ => continue,
            };
            let link_local = match ip {
                std::net::IpAddr::V4(v4) => v4.is_link_local(),
                std::net::IpAddr::V6(v6) => v6.is_unicast_link_local(),
            };
            if !ip.is_loopback() && !link_local && !addresses.contains(&ip) {
                addresses.push(ip);
            }
        }
        libc::freeifaddrs(list);
    }
    addresses.sort_by_key(|ip| ip.is_ipv6());
    addresses
}

/// Registers the served instance's hosts and gateway as the user's services
/// and starts them, the gateway announcing itself when `advertise` says so;
/// returns what the operator should still know. Enabling again restarts only
/// what changed, so the hosts' sessions outlive a changed announcement.
pub async fn enable(advertise: bool) -> Result<Vec<String>, String> {
    let mut args = vec!["remote", "enable", "--instance", SERVED_INSTANCE];
    if !advertise {
        args.push("--no-advertise");
    }
    let said = adroit(&args).await?;
    // What it started is shown by the gateway answering; the rest is advice.
    Ok(said
        .lines()
        .filter(|line| !line.starts_with("Started "))
        .map(str::to_string)
        .collect())
}

/// Stops the served instance's services and removes them.
pub async fn disable() -> Result<(), String> {
    adroit(&["remote", "disable", "--instance", SERVED_INSTANCE])
        .await
        .map(drop)
}

/// Runs this very program, so the services start the executable inside the
/// app, which macOS lets onto the local network.
async fn adroit(args: &[&str]) -> Result<String, String> {
    let program = std::env::current_exe().map_err(|e| e.to_string())?;
    let output = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim().trim_start_matches("adroit: ");
        return Err(if said.is_empty() {
            format!("adroit {} failed", args.join(" "))
        } else {
            said.to_string()
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_net::{Enroll, Identity};

    #[test]
    fn a_gateway_on_every_interface_is_reached_by_its_host_name_and_addresses() {
        let locals = ["192.168.0.5".parse().unwrap(), "fd00::5".parse().unwrap()];
        assert_eq!(
            reachable_at(&["[::]:47130".into()], "work-box", &locals),
            [
                "work-box.local:47130",
                "192.168.0.5:47130",
                "[fd00::5]:47130"
            ]
        );
        assert_eq!(
            reachable_at(&["10.0.0.2:4000".into()], "work-box", &locals),
            ["10.0.0.2:4000"],
            "a gateway told where to listen is reached there"
        );
        assert_eq!(
            reachable_at(&["0.0.0.0:47130".into(), "[::]:47130".into()], "box", &[]),
            ["box.local:47130"]
        );
    }

    #[tokio::test]
    async fn a_computer_without_its_gateway_running_serves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let here = ThisComputer::in_dir(dir.path());
        assert_eq!(here.serving().await, None);
        assert!(here.open_pairing().await.is_err());
        assert!(here.asking().await.is_err());
    }

    #[tokio::test]
    async fn a_gateway_that_never_answers_is_told_by_what_it_said_since() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("gateway.log");
        std::fs::write(&log, "adroit: an older failure\n").unwrap();
        let here = ThisComputer::in_dir(dir.path());
        let since = here.said();
        std::fs::write(
            &log,
            "adroit: an older failure\nadroit: Address already in use (os error 48)\n",
        )
        .unwrap();
        let why = here
            .answering(since, Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(why.contains("Address already in use"), "{why}");
        assert!(!why.contains("older"), "{why}");
    }

    #[tokio::test]
    async fn a_gateway_that_never_answers_nor_says_why_is_still_told() {
        let dir = tempfile::tempdir().unwrap();
        let here = ThisComputer::in_dir(dir.path());
        let why = here
            .answering(here.said(), Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(why.contains("did not start"), "{why}");
    }

    #[tokio::test]
    async fn a_gateway_that_takes_the_question_but_never_answers_is_still_told() {
        let dir = tempfile::tempdir().unwrap();
        let here = ThisComputer::in_dir(dir.path());
        let listener = tokio::net::UnixListener::bind(&here.files.admin).unwrap();
        let _held = tokio::spawn(async move {
            let mut taken = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                taken.push(stream);
            }
        });
        let why = tokio::time::timeout(
            Duration::from_secs(2),
            here.answering(here.said(), Duration::from_millis(50)),
        )
        .await
        .expect("the wait ends at its deadline")
        .unwrap_err();
        assert!(why.contains("did not start"), "{why}");
    }

    #[tokio::test]
    async fn paired_clients_are_listed_and_revoked_by_their_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let own = Identity::generate().unwrap().fingerprint();
        let here = ThisComputer::in_dir_as(dir.path(), own.to_string());
        assert_eq!(here.clients().unwrap(), vec![]);
        let client = Identity::generate().unwrap().fingerprint();
        let peers = PeerFile::open(GatewayFiles::in_dir(dir.path()).peers).unwrap();
        peers.enroll(client, "laptop").unwrap();
        peers.enroll(own, "here").unwrap();
        let listed = here.clients().unwrap();
        let shown: Vec<_> = listed
            .iter()
            .map(|c| (c.name.as_str(), c.fingerprint.clone(), c.this_computer))
            .collect();
        assert_eq!(
            shown,
            [
                ("laptop", client.to_string(), false),
                ("here", own.to_string(), true)
            ]
        );
        assert!(listed.iter().all(|c| c.paired_at > 0));
        here.revoke(&client.to_string()).await.unwrap();
        here.revoke(&own.to_string()).await.unwrap();
        assert_eq!(here.clients().unwrap(), vec![]);
        assert!(here.revoke("not a fingerprint").await.is_err());
    }
}
