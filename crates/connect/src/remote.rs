//! A selected work machine, reached through its gateway, and the checks made
//! before the window uses its hosts.

use crate::Target;
use adroit_protocol::client::Connection;
use adroit_protocol::{RemoteInfo, SPOKEN, agree};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(15);

/// The hosts' discovery, accepted only when the window speaks their
/// protocols and, on a reconnection, they are the hosts it saw before.
pub(crate) fn admit(discovery: &[u8], expected: Option<&RemoteInfo>) -> Result<RemoteInfo, String> {
    let info: RemoteInfo =
        serde_json::from_slice(discovery).map_err(|e| format!("invalid remote discovery: {e}"))?;
    info.validate()?;
    if !info
        .work
        .protocols
        .iter()
        .any(|v| SPOKEN.iter().any(|ours| ours.0 == *v))
        || !info
            .terminal
            .protocols
            .iter()
            .any(|v| adroit_term::SPEAKS.contains(v))
    {
        return Err("remote host protocols are incompatible".into());
    }
    if let Some(expected) = expected
        && (expected.instance != info.instance || expected.terminal != info.terminal)
    {
        return Err(
            "remote terminal identity or generation changed; open a new connection explicitly"
                .into(),
        );
    }
    Ok(info)
}

/// Greets both hosts through the forwarded sockets and confirms they are
/// the ones the discovery named.
pub(crate) async fn greet(
    info: &RemoteInfo,
    work: &Path,
    terminal: &adroit_term::Daemon,
) -> Result<(), String> {
    let connection = Connection::open_unscoped(work)
        .await
        .map_err(|e| format!("cannot greet remote work host: {e}"))?;
    if connection.hello.endpoint.as_ref() != Some(&info.work)
        || connection.hello.terminal.as_ref() != Some(&info.terminal)
        || connection.hello.mutation_epoch.is_none()
        || agree(SPOKEN, &connection.hello.speaks).is_none()
    {
        return Err("remote work host identity or protocol changed during connection".into());
    }
    tokio::time::timeout(DEADLINE, terminal.sessions())
        .await
        .map_err(|_| "remote terminal handshake timed out".to_string())?
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Where this user keeps the key it pairs with: one for every client
/// instance, since any of them may open a profile that was paired.
pub fn client_key() -> PathBuf {
    adroit_model::paths::state_dir().join("adroit/net/client.key")
}

/// Asks the work machine at `address` to pair under `name`, handing `show`
/// the code to type there once the machine answered, and returns the target
/// that reaches it once the operator typed that code.
pub async fn pair_with(
    address: &str,
    name: Option<&str>,
    show: impl FnOnce(String),
) -> Result<Target, String> {
    let name = name.map_or_else(adroit_net::host_name, str::to_string);
    if !crate::valid_address(address) {
        return Err(
            "give the work machine's address with its port, such as work-box.local:47130".into(),
        );
    }
    let identity = adroit_net::Identity::load_or_create(client_key()).map_err(|e| e.to_string())?;
    let code = adroit_net::PairingCode::generate();
    let asking = async {
        let mut show = Some(show);
        loop {
            let stream = tokio::time::timeout(DEADLINE, tokio::net::TcpStream::connect(address))
                .await
                .map_err(|_| format!("cannot reach {address}: timed out"))?
                .map_err(|e| format!("cannot reach {address}: {e}"))?;
            if let Some(show) = show.take() {
                show(code.to_string());
            }
            match adroit_net::pair(&identity, stream, &name, &code).await {
                // No exchange ran, so the same code may be offered again.
                Err(adroit_net::PairError::Closed | adroit_net::PairError::Busy) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                // The machine proved it knew the code, so its key is known;
                // whether it let this client in, only connecting tells.
                Err(adroit_net::PairError::Unconfirmed(server)) => {
                    let asked = adroit_net::Remote::new(identity.clone(), server, address)
                        .info()
                        .await;
                    return took(&asked).map(|()| server);
                }
                done => return done.map_err(|e| e.to_string()),
            }
        }
    };
    let server = tokio::time::timeout(PAIRING_WAIT, asking)
        .await
        .map_err(|_| "the work machine did not pair in time".to_string())??;
    Ok(Target::Native {
        address: address.into(),
        server: server.to_string(),
    })
}

/// Whether a pairing whose last answer was lost took, by what the work
/// machine said when asked with the client's key.
fn took(asked: &Result<Vec<u8>, adroit_net::Error>) -> Result<(), String> {
    match asked {
        Ok(_) => Ok(()),
        Err(adroit_net::Error::NotPaired) => {
            Err("the work machine did not finish pairing; pair again with a new code".into())
        }
        Err(error) => Err(format!(
            "the work machine may have paired, but cannot be asked to find out: {error}"
        )),
    }
}

/// Pairs as `pair_with` does and adds a profile for the work machine.
pub fn pair_cli(address: &str, name: Option<&str>, show: impl Fn(&str)) -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let target = runtime.block_on(pair_with(address, name, |code| {
        show(&format!(
            "Run `adroit remote pair` on the work machine and type this code there: {code}"
        ))
    }))?;
    let Target::Native { server, .. } = &target else {
        unreachable!("pairing reaches a paired work machine");
    };
    let server = server.clone();
    let path = crate::profiles::Catalog::path();
    let mut catalog = crate::profiles::Catalog::load(&path)?;
    let expected = catalog.revision;
    let profile_name = format!("Work machine at {address}");
    catalog
        .profiles
        .push(crate::profiles::Profile::new(&profile_name, target));
    catalog.save(&path, expected)?;
    Ok(format!(
        "Paired with the work machine {} and added the profile \"{profile_name}\"",
        &server[..16]
    ))
}

/// A work machine heard on the local network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    pub name: String,
    /// An address to pair with.
    pub address: String,
    /// The start of its key's fingerprint, to tell machines of one name apart.
    pub key: String,
}

/// The work machines announcing themselves on the local network.
pub async fn discover() -> Result<Vec<Heard>, String> {
    let heard = adroit_net::browse(Duration::from_secs(3))
        .await
        .map_err(|e| e.to_string())?;
    Ok(heard
        .into_iter()
        .filter_map(|found| {
            Some(Heard {
                address: found.addresses.first()?.to_string(),
                key: found.server.to_string()[..16].to_string(),
                name: found.name,
            })
        })
        .collect())
}

/// The work machines heard on the local network, one line each: its name,
/// an address to pair with, and the start of its key's fingerprint.
pub fn discover_cli() -> Result<Vec<String>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let heard = runtime.block_on(discover())?;
    Ok(heard
        .into_iter()
        .map(|heard| format!("{}  {}  {}", heard.name, heard.address, heard.key))
        .collect())
}

/// How long a client listens for its work machine on the local network
/// once the saved address failed.
const FIND_WAIT: Duration = Duration::from_secs(3);

/// How long the client waits for the operator to type its code.
const PAIRING_WAIT: Duration = Duration::from_secs(5 * 60);

/// A private folder for forwarded sockets, in /tmp so their paths stay
/// within the platform's small limit.
fn private_sockets() -> Result<tempfile::TempDir, String> {
    let sockets = tempfile::Builder::new()
        .prefix("adroit-c-")
        .tempdir_in("/tmp")
        .map_err(|e| e.to_string())?;
    std::fs::set_permissions(sockets.path(), std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    Ok(sockets)
}

/// The selected work machine, paired through its gateway.
#[derive(Debug, Clone)]
pub struct Remote {
    address: String,
    identity: adroit_net::Identity,
    server: adroit_net::Fingerprint,
    sockets: Arc<tempfile::TempDir>,
    /// Where the local network last heard the machine, when that was not
    /// the saved address; shared by every clone, so a window reconnecting
    /// after the machine moved does not wait at the old address again.
    found: Arc<std::sync::Mutex<Option<String>>>,
}

impl Remote {
    pub fn new(target: &Target) -> Result<Self, String> {
        Self::with_key(target, &client_key())
    }

    pub fn with_key(target: &Target, key: &Path) -> Result<Self, String> {
        target.validate()?;
        let Target::Native { address, server } = target else {
            return Err("no paired work machine selected".into());
        };
        let server = server.parse().map_err(|_| "invalid work machine key")?;
        let identity = adroit_net::Identity::load_or_create(key).map_err(|e| e.to_string())?;
        let sockets = private_sockets()?;
        Ok(Self {
            address: address.clone(),
            identity,
            server,
            sockets: Arc::new(sockets),
            found: Arc::default(),
        })
    }

    /// The addresses to try, in order, before listening on the local network.
    fn candidates(&self) -> Vec<String> {
        let found = self.found.lock().unwrap().clone();
        found.into_iter().chain([self.address.clone()]).collect()
    }

    fn found_at(&self, address: &str) {
        *self.found.lock().unwrap() = (address != self.address).then(|| address.to_string());
    }

    /// How the work machine is named to the user.
    pub fn name(&self) -> String {
        self.address.clone()
    }

    pub fn work_socket(&self) -> PathBuf {
        self.sockets.path().join("work.sock")
    }

    pub fn terminal(&self) -> adroit_term::Daemon {
        adroit_term::Daemon::forwarded(self.sockets.path().join("terminal.sock"))
    }

    pub async fn connect(&self, expected: Option<&RemoteInfo>) -> Result<Tunnel, String> {
        let (net, discovery) = self.reach().await?;
        let info = admit(&discovery, expected)?;
        let forwarding = net
            .forward(&self.work_socket(), self.terminal().socket())
            .map_err(|e| e.to_string())?;
        greet(&info, &self.work_socket(), &self.terminal()).await?;
        Ok(Tunnel {
            remote: self.clone(),
            info,
            reached: net.address().to_string(),
            _forwarding: forwarding,
        })
    }

    /// The paired key at the saved address, or else wherever the local
    /// network hears it announced: an address is only where to try.
    async fn reach(&self) -> Result<(adroit_net::Remote, Vec<u8>), String> {
        let at =
            |address: String| adroit_net::Remote::new(self.identity.clone(), self.server, address);
        let mut failure = None;
        for address in self.candidates() {
            let remote = at(address);
            match remote.info().await {
                Ok(discovery) => {
                    self.found_at(remote.address());
                    return Ok((remote, discovery));
                }
                Err(adroit_net::Error::NotPaired) => {
                    return Err(self.explain(adroit_net::Error::NotPaired));
                }
                // What the saved address said is what is reported.
                Err(error) => failure = Some(error),
            }
        }
        let failure = failure.expect("the saved address is always tried");
        if let Ok(Some(found)) = adroit_net::find(self.server, FIND_WAIT).await {
            for address in found.addresses {
                let remote = at(address.to_string());
                match remote.info().await {
                    Ok(discovery) => {
                        self.found_at(remote.address());
                        return Ok((remote, discovery));
                    }
                    Err(adroit_net::Error::NotPaired) => {
                        return Err(self.explain(adroit_net::Error::NotPaired));
                    }
                    Err(_) => continue,
                }
            }
        }
        Err(self.explain(failure))
    }

    fn explain(&self, error: adroit_net::Error) -> String {
        match error {
            adroit_net::Error::NotPaired => format!(
                "the work machine at {} refused this client's key; pair it again with adroit connect pair",
                self.address
            ),
            adroit_net::Error::OtherServer => format!(
                "the machine at {} is not the paired work machine: its key differs",
                self.address
            ),
            error => format!("cannot reach the work machine at {}: {error}", self.address),
        }
    }
}

/// The way to a connected work machine's hosts, open while it is kept.
pub struct Tunnel {
    remote: Remote,
    info: RemoteInfo,
    /// The address the work machine answered at, which may not be the saved one.
    reached: String,
    _forwarding: adroit_net::Forwarding,
}

impl Tunnel {
    pub fn work(&self, journal: Arc<crate::journal::Journal>) -> Result<crate::work::Work, String> {
        crate::work::Work::new(self.remote.work_socket(), self.info.clone(), journal)
    }

    pub fn info(&self) -> &RemoteInfo {
        &self.info
    }

    /// The address the work machine answered at.
    pub fn reached(&self) -> &str {
        &self.reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pairing_whose_last_answer_was_lost_took_only_when_the_key_is_let_in() {
        assert!(took(&Ok(b"{}".to_vec())).is_ok());
        let refused = took(&Err(adroit_net::Error::NotPaired)).unwrap_err();
        assert!(refused.contains("did not finish pairing"), "{refused}");
        let unknown = took(&Err(adroit_net::Error::Unreachable("w:1".into()))).unwrap_err();
        assert!(unknown.contains("may have paired"), "{unknown}");
    }

    #[test]
    fn a_reconnection_tries_where_the_machine_was_last_found_before_the_saved_address() {
        let dir = tempfile::tempdir().unwrap();
        let target = Target::Native {
            address: "192.0.2.1:47130".into(),
            server: "ab".repeat(32),
        };
        let remote = Remote::with_key(&target, &dir.path().join("client.key")).unwrap();
        assert_eq!(remote.candidates(), ["192.0.2.1:47130"]);
        remote.found_at("10.0.0.9:47130");
        assert_eq!(remote.candidates(), ["10.0.0.9:47130", "192.0.2.1:47130"]);
        remote.clone().found_at("192.0.2.1:47130");
        assert_eq!(remote.candidates(), ["192.0.2.1:47130"]);
    }

    fn discovered() -> RemoteInfo {
        let instance = "a".repeat(32);
        let endpoint = |generation: &str, protocols: Vec<u32>| adroit_protocol::RemoteEndpoint {
            instance: instance.clone(),
            generation: generation.into(),
            pid: 1,
            socket: format!("/tmp/adroit-r-501/{instance}/{generation}.sock").into(),
            build: "test".into(),
            protocols,
        };
        RemoteInfo {
            instance: instance.clone(),
            work: endpoint(&"b".repeat(32), vec![SPOKEN[0].0]),
            terminal: endpoint(&"c".repeat(32), adroit_term::SPEAKS.to_vec()),
        }
    }

    fn moved(endpoint: &mut adroit_protocol::RemoteEndpoint, generation: String) {
        endpoint.socket =
            format!("/tmp/adroit-r-501/{}/{generation}.sock", endpoint.instance).into();
        endpoint.generation = generation;
    }

    fn admitted(info: &RemoteInfo, expected: Option<&RemoteInfo>) -> Result<RemoteInfo, String> {
        admit(&serde_json::to_vec(info).unwrap(), expected)
    }

    #[test]
    fn hosts_the_window_speaks_with_are_admitted_on_a_first_connection() {
        let info = discovered();
        assert_eq!(admitted(&info, None), Ok(info));
    }

    #[test]
    fn hosts_that_speak_none_of_the_window_s_protocols_are_refused() {
        let mut work = discovered();
        work.work.protocols = vec![u32::MAX];
        let refused = admitted(&work, None).unwrap_err();
        assert!(refused.contains("protocols are incompatible"), "{refused}");
        let mut terminal = discovered();
        terminal.terminal.protocols = vec![u32::MAX];
        let refused = admitted(&terminal, None).unwrap_err();
        assert!(refused.contains("protocols are incompatible"), "{refused}");
    }

    #[test]
    fn a_reconnection_is_refused_once_the_terminal_host_was_replaced() {
        let seen = discovered();
        let mut replaced = seen.clone();
        moved(&mut replaced.terminal, "d".repeat(32));
        let refused = admitted(&replaced, Some(&seen)).unwrap_err();
        assert!(refused.contains("generation changed"), "{refused}");
    }

    #[test]
    fn a_reconnection_to_a_replaced_work_host_beside_the_same_terminal_host_is_admitted() {
        let seen = discovered();
        let mut replaced = seen.clone();
        moved(&mut replaced.work, "e".repeat(32));
        assert_eq!(admitted(&replaced, Some(&seen)), Ok(replaced));
    }

    #[test]
    fn the_client_key_is_kept_in_adroit_s_own_state_folder() {
        assert!(client_key().starts_with(adroit_model::paths::state_dir().join("adroit")));
    }
}
