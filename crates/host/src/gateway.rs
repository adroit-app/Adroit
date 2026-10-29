//! The gateway that lets paired clients reach this instance's hosts over the
//! network, and the private socket its operator manages it through.

use adroit_net::{
    Admin, AdminAnswer, Fingerprint, Gateway, GatewayFiles, Hosts, HostsNow, Identity,
    PAIRING_PERIOD, Pairing, PeerFile, Waiting, ask_gateway,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener, UnixStream};

/// The port a gateway listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 47130;
const MAX_ADMIN_LINE: u64 = 4096;

/// A gateway serving its listeners and its admin socket.
pub struct Running {
    pub server: Fingerprint,
    pub listening: Vec<std::net::SocketAddr>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    admin: PathBuf,
    announcing: Arc<AtomicBool>,
}

impl Running {
    /// Tells the operator, when asked, whether it announces itself now.
    pub fn announcing(&self, announcing: bool) {
        self.announcing.store(announcing, Ordering::Relaxed);
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.tasks.iter().for_each(|task| task.abort());
        let _ = std::fs::remove_file(&self.admin);
    }
}

/// Starts a gateway with the files in `files`, listening on `listen`.
pub async fn start(
    files: &GatewayFiles,
    listen: &[std::net::SocketAddr],
    hosts: Arc<dyn Hosts>,
) -> std::io::Result<Running> {
    let identity = Identity::load_or_create(&files.key).map_err(std::io::Error::other)?;
    let peers = Arc::new(PeerFile::open(&files.peers).map_err(std::io::Error::other)?);
    let pairing = Arc::new(Pairing::new(identity.fingerprint(), peers.clone()));
    let gateway = Gateway::new(&identity, peers.clone(), pairing.clone(), hosts)
        .map_err(std::io::Error::other)?;
    let mut tasks = Vec::new();
    let mut listening = Vec::new();
    for address in listen {
        let listener = TcpListener::bind(address).await?;
        listening.push(listener.local_addr()?);
        let gateway = gateway.clone();
        tasks.push(tokio::spawn(async move {
            let _ = gateway.listen(listener).await;
        }));
    }
    match std::fs::remove_file(&files.admin) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let admin = UnixListener::bind(&files.admin)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&files.admin, std::fs::Permissions::from_mode(0o600))?;
    }
    let announcing = Arc::new(AtomicBool::new(false));
    let served = Arc::new(Served {
        server: identity.fingerprint(),
        listening: listening.clone(),
        pairing,
        peers,
        gateway,
        announcing: announcing.clone(),
    });
    tasks.push(tokio::spawn(async move {
        while let Ok((stream, _)) = admin.accept().await {
            let served = served.clone();
            tokio::spawn(async move {
                let _ = served.answer(stream).await;
            });
        }
    }));
    Ok(Running {
        server: identity.fingerprint(),
        listening,
        tasks,
        admin: files.admin.clone(),
        announcing,
    })
}

/// The hosts of this instance, found through their discovery files.
struct Discovered;

impl Hosts for Discovered {
    fn current(&self) -> Result<HostsNow, String> {
        let info = crate::remote::info().map_err(|error| error.to_string())?;
        Ok(HostsNow {
            info: serde_json::to_vec(&info).map_err(|error| error.to_string())?,
            work: info.work.socket,
            terminal: info.terminal.socket,
        })
    }
}

/// Runs `adroit remote gateway`, `pair`, `devices` or `revoke`.
pub fn cli(operation: crate::cli::RemoteCommand) -> i32 {
    use crate::cli::RemoteCommand;
    let dir = crate::remote::root();
    let done = (|| -> std::io::Result<i32> {
        std::fs::create_dir_all(dir.parent().unwrap_or(Path::new("/")))?;
        crate::remote::private_dir(&dir, true)?;
        let files = GatewayFiles::in_dir(&dir);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        match operation {
            RemoteCommand::Gateway { listen, advertise } => {
                runtime.block_on(serve(&files, listen, advertise))
            }
            RemoteCommand::Pair => runtime.block_on(pair(&files)),
            RemoteCommand::Devices => devices(&files),
            RemoteCommand::Revoke(prefix) => runtime.block_on(revoke(&files, &prefix)),
            _ => unreachable!("not a gateway command"),
        }
    })();
    done.unwrap_or_else(|error| {
        eprintln!("adroit: {error}");
        1
    })
}

async fn serve(
    files: &GatewayFiles,
    listen: Option<String>,
    advertise: bool,
) -> std::io::Result<i32> {
    let addresses: Vec<std::net::SocketAddr> = match listen {
        Some(address) => vec![address.parse().map_err(std::io::Error::other)?],
        // Every interface: a client is let in only with a paired key, so a
        // narrower bind would buy little and break on a changed address.
        None => vec![(std::net::Ipv6Addr::UNSPECIFIED, DEFAULT_PORT).into()],
    };
    let running = match start(files, &addresses, Arc::new(Discovered)).await {
        Err(error) if listen_was_default(&addresses) => {
            let fallback = [(std::net::Ipv4Addr::UNSPECIFIED, DEFAULT_PORT).into()];
            start(files, &fallback, Arc::new(Discovered))
                .await
                .map_err(|_| error)?
        }
        running => running?,
    };
    let listening: Vec<String> = running.listening.iter().map(ToString::to_string).collect();
    eprintln!(
        "adroit: gateway {} listening on {}",
        running.server,
        listening.join(", ")
    );
    // Kept for as long as the gateway serves; dropping it withdraws it.
    let _announced = if advertise {
        let port = running.listening[0].port();
        let name = format!("Adroit on {}", adroit_net::host_name());
        match adroit_net::advertise(&name, running.server, port) {
            Ok(announced) => {
                running.announcing(true);
                tokio::spawn(watch_announcing());
                Some(announced)
            }
            Err(error) => {
                eprintln!("adroit: cannot announce the gateway on the local network: {error}");
                None
            }
        }
    } else {
        None
    };
    std::future::pending::<()>().await;
    Ok(0)
}

/// How often a gateway that cannot reach the local network tries again.
const ANNOUNCING_RETRY: Duration = Duration::from_secs(60);

/// Tells the operator when the announcement cannot leave this machine, and
/// again once it can.
async fn watch_announcing() {
    let mut was = None;
    loop {
        let now = tokio::task::spawn_blocking(adroit_net::can_multicast)
            .await
            .unwrap_or_else(|error| Err(std::io::Error::other(error)));
        if let Some(told) = announcing(was, &now) {
            eprintln!("{told}");
        }
        if now.is_ok() {
            return;
        }
        was = Some(false);
        tokio::time::sleep(ANNOUNCING_RETRY).await;
    }
}

/// What to tell the operator when the announcement could last leave this
/// machine as `was` says and now does as `now` says.
fn announcing(was: Option<bool>, now: &std::io::Result<()>) -> Option<String> {
    let hint = if cfg!(target_os = "macos") {
        "; allow Adroit to find devices on local networks in System Settings > Privacy & Security > Local Network"
    } else {
        ""
    };
    match (was, now) {
        (Some(false), Err(_)) | (Some(true) | None, Ok(())) => None,
        (_, Err(error)) => Some(format!(
            "adroit: the gateway cannot announce itself on the local network: {error}{hint}"
        )),
        (Some(false), Ok(())) => {
            Some("adroit: the gateway announces itself on the local network now".into())
        }
    }
}

fn listen_was_default(addresses: &[std::net::SocketAddr]) -> bool {
    addresses == [(std::net::Ipv6Addr::UNSPECIFIED, DEFAULT_PORT).into()]
}

fn short(fingerprint: &Fingerprint) -> String {
    fingerprint.to_string()[..16].to_string()
}

async fn pair(files: &GatewayFiles) -> std::io::Result<i32> {
    let not_running = |error: std::io::Error| {
        std::io::Error::other(format!(
            "cannot reach the gateway ({error}); start it with `adroit remote gateway`"
        ))
    };
    ask_gateway(
        &files.admin,
        &Admin::Open {
            seconds: PAIRING_PERIOD.as_secs(),
        },
    )
    .await
    .map_err(not_running)?;
    if let AdminAnswer::Status {
        server, listening, ..
    } = ask_gateway(&files.admin, &Admin::Status).await?
    {
        println!("This work machine is {}", short(&server));
        println!("Listening on {}", listening.join(", "));
    }
    println!("Waiting for a pairing request; press Ctrl-C to stop.");
    let result = loop {
        let waiting = match ask_gateway(&files.admin, &Admin::Requests).await? {
            AdminAnswer::Requests(waiting) => waiting,
            AdminAnswer::Failed(text) => break Err(std::io::Error::other(text)),
            _ => break Err(std::io::Error::other("unexpected answer")),
        };
        let Some(request) = waiting.into_iter().next() else {
            if !matches!(
                ask_gateway(&files.admin, &Admin::Status).await?,
                AdminAnswer::Status { open: true, .. }
            ) {
                break Err(std::io::Error::other(
                    "pairing closed before a request came",
                ));
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            continue;
        };
        println!(
            "Request from \"{}\" ({})",
            request.name,
            short(&request.client)
        );
        print!("Type the code shown on that machine, or press Enter to refuse: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let typed = tokio::task::spawn_blocking(|| {
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map(|read| (read > 0).then_some(line))
        })
        .await
        .map_err(std::io::Error::other)??;
        let Some(typed) = typed else {
            ask_gateway(&files.admin, &Admin::Reject { id: request.id }).await?;
            break Err(std::io::Error::other(
                "the input closed before a code was typed",
            ));
        };
        let typed = typed.trim();
        if typed.is_empty() {
            ask_gateway(&files.admin, &Admin::Reject { id: request.id }).await?;
            println!("Refused.");
            continue;
        }
        let approve = Admin::Approve {
            id: request.id,
            code: typed.to_string(),
        };
        match ask_gateway(&files.admin, &approve).await? {
            AdminAnswer::Paired(client) => {
                println!("Paired \"{}\" ({})", request.name, short(&client));
                break Ok(0);
            }
            AdminAnswer::Failed(text) => println!("Not paired: {text}"),
            _ => println!("Not paired."),
        }
    };
    let _ = ask_gateway(&files.admin, &Admin::Close).await;
    result
}

fn devices(files: &GatewayFiles) -> std::io::Result<i32> {
    let peers = PeerFile::open(&files.peers)
        .map_err(std::io::Error::other)?
        .list();
    if peers.is_empty() {
        println!("No client is paired with this work machine");
    }
    for paired in peers {
        println!("{}  {}", paired.fingerprint, paired.name);
    }
    Ok(0)
}

async fn revoke(files: &GatewayFiles, prefix: &str) -> std::io::Result<i32> {
    if prefix.len() < 8 {
        return Err(std::io::Error::other(
            "name at least the first 8 characters of the fingerprint",
        ));
    }
    let peers = PeerFile::open(&files.peers).map_err(std::io::Error::other)?;
    let matching: Vec<_> = peers
        .list()
        .into_iter()
        .filter(|paired| paired.fingerprint.to_string().starts_with(prefix))
        .collect();
    let [paired] = matching.as_slice() else {
        return Err(std::io::Error::other(if matching.is_empty() {
            "no paired client has that fingerprint"
        } else {
            "more than one paired client starts with that; give more of it"
        }));
    };
    adroit_net::revoke_paired(files, paired.fingerprint)
        .await
        .map_err(std::io::Error::other)?;
    println!(
        "Revoked \"{}\" ({})",
        paired.name,
        short(&paired.fingerprint)
    );
    Ok(0)
}

struct Served {
    server: Fingerprint,
    listening: Vec<std::net::SocketAddr>,
    pairing: Arc<Pairing>,
    peers: Arc<PeerFile>,
    gateway: Arc<Gateway>,
    announcing: Arc<AtomicBool>,
}

impl Served {
    async fn answer(&self, stream: UnixStream) -> std::io::Result<()> {
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        BufReader::new(read)
            .take(MAX_ADMIN_LINE)
            .read_line(&mut line)
            .await?;
        let answer = match serde_json::from_str::<Admin>(&line) {
            Ok(admin) => self.handle(admin).await,
            Err(error) => AdminAnswer::Failed(error.to_string()),
        };
        let mut line = serde_json::to_vec(&answer).map_err(std::io::Error::other)?;
        line.push(b'\n');
        write.write_all(&line).await
    }

    async fn handle(&self, admin: Admin) -> AdminAnswer {
        match admin {
            Admin::Open { seconds } => {
                self.pairing.open(Duration::from_secs(seconds));
                AdminAnswer::Done
            }
            Admin::Close => {
                self.pairing.close();
                AdminAnswer::Done
            }
            Admin::Requests => AdminAnswer::Requests(
                self.pairing
                    .requests()
                    .into_iter()
                    .map(|request| Waiting {
                        paired: adroit_net::Peers::admits(&*self.peers, &request.client),
                        id: request.id,
                        name: request.name,
                        client: request.client,
                    })
                    .collect(),
            ),
            Admin::Approve { id, code } => match code.parse() {
                Ok(code) => {
                    let name = self
                        .pairing
                        .requests()
                        .into_iter()
                        .find(|request| request.id == id)
                        .map(|request| request.name)
                        .unwrap_or_default();
                    match self.pairing.approve(id, code).await {
                        Ok(client) => {
                            eprintln!("adroit: paired \"{name}\" ({client})");
                            AdminAnswer::Paired(client)
                        }
                        Err(error) => {
                            eprintln!("adroit: did not pair \"{name}\": {error}");
                            AdminAnswer::Failed(error.to_string())
                        }
                    }
                }
                Err(error) => AdminAnswer::Failed(error.to_string()),
            },
            Admin::Reject { id } => match self.pairing.reject(id) {
                Ok(()) => AdminAnswer::Done,
                Err(error) => AdminAnswer::Failed(error.to_string()),
            },
            Admin::Revoke { fingerprint } => match self.peers.revoke(&fingerprint) {
                Ok(_) => {
                    eprintln!("adroit: revoked {fingerprint} and ended its connections");
                    self.gateway.disconnect(&fingerprint);
                    AdminAnswer::Done
                }
                Err(error) => AdminAnswer::Failed(error.to_string()),
            },
            Admin::Status => AdminAnswer::Status {
                server: self.server,
                listening: self.listening.iter().map(ToString::to_string).collect(),
                open: self.pairing.is_open(),
                announcing: self.announcing.load(Ordering::Relaxed),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_net::Enroll;

    struct NoHosts;

    impl Hosts for NoHosts {
        fn current(&self) -> Result<HostsNow, String> {
            Ok(HostsNow {
                info: b"{}".to_vec(),
                work: "/nonexistent/w".into(),
                terminal: "/nonexistent/t".into(),
            })
        }
    }

    async fn started() -> (tempfile::TempDir, GatewayFiles, Running) {
        let dir = tempfile::Builder::new()
            .prefix("ag-")
            .tempdir_in("/tmp")
            .unwrap();
        let files = GatewayFiles::in_dir(dir.path());
        let running = start(&files, &["127.0.0.1:0".parse().unwrap()], Arc::new(NoHosts))
            .await
            .unwrap();
        (dir, files, running)
    }

    #[test]
    fn the_operator_is_told_once_when_announcing_fails_and_once_when_it_works_again() {
        let failed = || Err(std::io::Error::other("No route to host"));
        assert_eq!(announcing(None, &Ok(())), None);
        let told = announcing(None, &failed()).unwrap();
        assert!(told.contains("cannot announce") && told.contains("No route to host"));
        assert_eq!(announcing(Some(false), &failed()), None);
        assert!(
            announcing(Some(false), &Ok(()))
                .unwrap()
                .contains("announces")
        );
    }

    #[tokio::test]
    async fn the_operator_is_told_whether_the_gateway_announces_itself() {
        let (_dir, files, running) = started().await;
        let announcing = || async {
            match ask_gateway(&files.admin, &Admin::Status).await.unwrap() {
                AdminAnswer::Status { announcing, .. } => announcing,
                answer => panic!("{answer:?}"),
            }
        };
        assert!(!announcing().await);
        running.announcing(true);
        assert!(announcing().await);
    }

    #[tokio::test]
    async fn the_gateway_keeps_its_key_across_starts() {
        let (_dir, files, running) = started().await;
        let first = running.server;
        drop(running);
        let again = start(&files, &["127.0.0.1:0".parse().unwrap()], Arc::new(NoHosts))
            .await
            .unwrap();
        assert_eq!(again.server, first);
    }

    #[tokio::test]
    async fn the_operator_pairs_a_client_through_the_admin_socket() {
        let (_dir, files, running) = started().await;
        assert!(matches!(
            ask_gateway(&files.admin, &Admin::Status).await.unwrap(),
            AdminAnswer::Status { open: false, .. }
        ));
        assert_eq!(
            ask_gateway(&files.admin, &Admin::Open { seconds: 60 })
                .await
                .unwrap(),
            AdminAnswer::Done
        );
        let client = Identity::generate().unwrap();
        let code = adroit_net::PairingCode::generate();
        let address = running.listening[0];
        let asking = {
            let (client, code) = (client.clone(), code.clone());
            tokio::spawn(async move {
                let stream = tokio::net::TcpStream::connect(address).await.unwrap();
                adroit_net::pair(&client, stream, "laptop", &code, || {}).await
            })
        };
        let waiting = loop {
            if let AdminAnswer::Requests(mut waiting) =
                ask_gateway(&files.admin, &Admin::Requests).await.unwrap()
                && let Some(waiting) = waiting.pop()
            {
                break waiting;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        assert_eq!(waiting.name, "laptop");
        assert_eq!(waiting.client, client.fingerprint());
        assert!(!waiting.paired, "a client asking first is not paired");
        let approved = ask_gateway(
            &files.admin,
            &Admin::Approve {
                id: waiting.id,
                code: code.to_string(),
            },
        )
        .await
        .unwrap();
        assert_eq!(approved, AdminAnswer::Paired(client.fingerprint()));
        assert_eq!(asking.await.unwrap().unwrap(), running.server);
        let peers = PeerFile::open(&files.peers).unwrap().list();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].fingerprint, client.fingerprint());
    }

    #[tokio::test]
    async fn the_operator_sees_a_paired_client_asking_again_as_paired() {
        let (_dir, files, running) = started().await;
        let client = Identity::generate().unwrap();
        PeerFile::open(&files.peers)
            .unwrap()
            .enroll(client.fingerprint(), "laptop")
            .unwrap();
        ask_gateway(&files.admin, &Admin::Open { seconds: 60 })
            .await
            .unwrap();
        let address = running.listening[0];
        let asking = tokio::spawn(async move {
            let stream = tokio::net::TcpStream::connect(address).await.unwrap();
            let code = adroit_net::PairingCode::generate();
            adroit_net::pair(&client, stream, "laptop", &code, || {}).await
        });
        let waiting = loop {
            if let AdminAnswer::Requests(mut waiting) =
                ask_gateway(&files.admin, &Admin::Requests).await.unwrap()
                && let Some(waiting) = waiting.pop()
            {
                break waiting;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        assert!(waiting.paired);
        asking.abort();
    }

    #[tokio::test]
    async fn revoking_through_the_admin_socket_forgets_the_client() {
        let (_dir, files, running) = started().await;
        let client = Identity::generate().unwrap();
        PeerFile::open(&files.peers)
            .unwrap()
            .enroll(client.fingerprint(), "laptop")
            .unwrap();
        let remote = adroit_net::Remote::new(
            client.clone(),
            running.server,
            running.listening[0].to_string(),
        );
        assert!(remote.info().await.is_ok());
        assert_eq!(
            ask_gateway(
                &files.admin,
                &Admin::Revoke {
                    fingerprint: client.fingerprint()
                }
            )
            .await
            .unwrap(),
            AdminAnswer::Done
        );
        assert!(PeerFile::open(&files.peers).unwrap().list().is_empty());
        assert!(remote.info().await.is_err());
    }

    #[tokio::test]
    async fn a_malformed_admin_request_is_answered_with_a_failure() {
        let (_dir, files, _running) = started().await;
        let mut stream = UnixStream::connect(&files.admin).await.unwrap();
        stream.write_all(b"{\"op\":\"nonsense\"}\n").await.unwrap();
        let mut answer = String::new();
        BufReader::new(stream).read_line(&mut answer).await.unwrap();
        assert!(matches!(
            serde_json::from_str::<AdminAnswer>(&answer).unwrap(),
            AdminAnswer::Failed(_)
        ));
    }
}
