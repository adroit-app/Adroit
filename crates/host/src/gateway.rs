//! The gateway that lets paired clients reach this instance's hosts over the
//! network, and the private socket its operator manages it through.

use adroit_net::{
    Admin, AdminAnswer, DEFAULT_PORT, Fingerprint, Gateway, GatewayFiles, Hosts, HostsNow,
    Identity, PAIRING_PERIOD, Pairing, PeerFile, Waiting, ask_gateway,
};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

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
    let admin = crate::work_host::bind_private(&files.admin)?;
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
        while let Ok(stream) = admin.accept().await {
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

/// How often the gateway looks whether the instance's hosts run.
const LOOK_AGAIN: std::time::Duration = std::time::Duration::from_secs(2);

/// Start the instance's work host, which starts its terminal host, whenever
/// none runs, so that paired clients find them after the machine starts.
/// One already running is left as it is whatever its build: the windows on
/// this machine replace it as they need, and it outlives the gateway.
async fn keep_hosts_up() {
    use crate::work_host::{adroit_program, serve_args, socket, standing, start};
    loop {
        if standing(&socket()).await == adroit_protocol::WorkHost::Absent
            && let Err(e) = start(&socket(), &adroit_program(), &serve_args()).await
        {
            tracing::warn!(error = %e, "cannot start the work host");
        }
        tokio::time::sleep(LOOK_AGAIN).await;
    }
}

/// The hosts of this instance, found through their discovery files.
/// Says once, until they are found again, that they are not: a paired client
/// cut off right after its handshake learns no more than that.
#[derive(Default)]
struct Discovered {
    missing: AtomicBool,
    restarting: tokio::sync::Mutex<()>,
    updating: tokio::sync::Mutex<()>,
}

impl Hosts for Discovered {
    fn current(&self) -> Result<HostsNow, String> {
        let info = match crate::remote::info() {
            Ok(info) => {
                if self.missing.swap(false, Ordering::Relaxed) {
                    tracing::info!("paired clients reach the hosts again");
                }
                info
            }
            Err(error) => {
                if !self.missing.swap(true, Ordering::Relaxed) {
                    tracing::warn!(%error, "paired clients cannot reach the hosts");
                }
                return Err(error.to_string());
            }
        };
        Ok(HostsNow {
            info: serde_json::to_vec(&info).map_err(|error| error.to_string())?,
            work: info.work.socket,
            terminal: info.terminal.socket,
        })
    }

    fn restart_work(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        use crate::work_host::{adroit_program, ensure, serve_args, socket, stop};
        Box::pin(async {
            let _turn = self.restarting.lock().await;
            tracing::info!("restarting the work host for a paired client");
            let socket = socket();
            stop(&socket).await.map_err(|e| e.to_string())?;
            ensure(&socket, &adroit_program(), &serve_args())
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    fn update(&self) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>> {
        use crate::update::{Updated, brew, update};
        Box::pin(async {
            let _turn = self
                .updating
                .try_lock()
                .map_err(|_| "Adroit is being updated there already")?;
            let brew = brew().ok_or("there is no Homebrew there to update Adroit with")?;
            tracing::info!("updating Adroit for a paired client");
            match update(&brew, &crate::work_host::adroit_program()).await? {
                Updated::Already(version) => Ok(format!("Adroit {version} is the newest there")),
                Updated::To(version) => {
                    self.restart_work().await.map_err(|why| {
                        format!(
                            "Adroit {version} is installed, but its work host does not start: {why}"
                        )
                    })?;
                    if supervised() {
                        // Once the client has been told what came of it.
                        tokio::spawn(async {
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            std::process::exit(0);
                        });
                    }
                    Ok(format!("Updated Adroit to {version}"))
                }
            }
        })
    }
}

/// Whether the service manager runs this gateway, and starts it again when
/// it ends.
fn supervised() -> bool {
    std::env::var("XPC_SERVICE_NAME")
        .is_ok_and(|name| name.starts_with("app.adroit.") && name.ends_with(".gateway"))
}

/// Runs `adroit remote gateway`, `pair`, `devices` or `revoke`.
pub fn cli(operation: crate::cli::RemoteCommand) -> i32 {
    use crate::cli::RemoteCommand;
    let dir = crate::remote::root();
    let done = (|| -> std::io::Result<i32> {
        adroit_sys::fs::private_dir(&dir)?;
        let files = GatewayFiles::in_dir(&dir);
        use crate::cli::block_on;
        match operation {
            RemoteCommand::Gateway { listen, advertise } => {
                block_on(serve(&files, listen, advertise))?
            }
            RemoteCommand::Pair => block_on(pair(&files))?,
            RemoteCommand::Devices => devices(&files),
            RemoteCommand::Revoke(prefix) => block_on(revoke(&files, &prefix))?,
            _ => unreachable!("not a gateway command"),
        }
    })();
    done.unwrap_or_else(|error| crate::cli::failed(error, 1))
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
    let running = match start(files, &addresses, Arc::new(Discovered::default())).await {
        Err(error) if listen_was_default(&addresses) => {
            let fallback = [(std::net::Ipv4Addr::UNSPECIFIED, DEFAULT_PORT).into()];
            start(files, &fallback, Arc::new(Discovered::default()))
                .await
                .map_err(|_| error)?
        }
        running => running?,
    };
    let listening: Vec<String> = running.listening.iter().map(ToString::to_string).collect();
    tracing::info!(
        server = %running.server,
        listening = %listening.join(", "),
        "gateway listening"
    );
    // Kept for as long as the gateway serves; dropping it withdraws it.
    let _announced = if advertise {
        let name = format!("Adroit on {}", adroit_net::host_name());
        match adroit_net::advertise(&name, running.server, &running.listening) {
            Ok(announced) => {
                running.announcing(true);
                tokio::spawn(watch_announcing());
                Some(announced)
            }
            Err(error) => {
                tracing::warn!(%error, "cannot announce the gateway on the local network");
                None
            }
        }
    } else {
        None
    };
    keep_hosts_up().await;
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
        match announcing(was, &now) {
            Some(Announcing::Failing(error)) => tracing::warn!(
                %error,
                hint = LOCAL_NETWORK_HINT,
                "the gateway cannot announce itself on the local network"
            ),
            Some(Announcing::Again) => {
                tracing::info!("the gateway announces itself on the local network now")
            }
            None => {}
        }
        if now.is_ok() {
            return;
        }
        was = Some(false);
        tokio::time::sleep(ANNOUNCING_RETRY).await;
    }
}

/// What a user does to let the gateway reach the local network.
const LOCAL_NETWORK_HINT: &str = if cfg!(target_os = "macos") {
    "allow Adroit to find devices on local networks in System Settings > Privacy & Security > Local Network"
} else {
    ""
};

/// What the operator is told when announcing changed.
enum Announcing<'a> {
    Failing(&'a std::io::Error),
    Again,
}

/// What to tell the operator when the announcement could last leave this
/// machine as `was` says and now does as `now` says.
fn announcing(was: Option<bool>, now: &std::io::Result<()>) -> Option<Announcing<'_>> {
    match (was, now) {
        (Some(false), Err(_)) | (Some(true) | None, Ok(())) => None,
        (_, Err(error)) => Some(Announcing::Failing(error)),
        (Some(false), Ok(())) => Some(Announcing::Again),
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
    async fn answer(&self, stream: adroit_sys::local::Stream) -> std::io::Result<()> {
        let (read, mut write) = tokio::io::split(stream);
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
                            tracing::info!(%name, %client, "paired");
                            AdminAnswer::Paired(client)
                        }
                        Err(error) => {
                            tracing::info!(%name, %error, "did not pair");
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
                    tracing::info!(%fingerprint, "revoked and ended its connections");
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

        fn restart_work(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }

        fn update(&self) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>> {
            Box::pin(async { Ok(String::new()) })
        }
    }

    async fn started() -> (tempfile::TempDir, GatewayFiles, Running) {
        let dir = tempfile::Builder::new()
            .prefix("ag-")
            .tempdir_in(adroit_sys::local::temp_dir())
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
        assert!(announcing(None, &Ok(())).is_none());
        let failing = failed();
        assert!(matches!(
            announcing(None, &failing),
            Some(Announcing::Failing(error)) if error.to_string() == "No route to host"
        ));
        assert!(announcing(Some(false), &failed()).is_none());
        assert!(matches!(
            announcing(Some(false), &Ok(())),
            Some(Announcing::Again)
        ));
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
    #[cfg_attr(windows, ignore = "serves as a work machine, which runs Unix")]
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
        let mut stream = adroit_sys::local::connect(&files.admin).await.unwrap();
        stream.write_all(b"{\"op\":\"nonsense\"}\n").await.unwrap();
        let mut answer = String::new();
        BufReader::new(stream).read_line(&mut answer).await.unwrap();
        assert!(matches!(
            serde_json::from_str::<AdminAnswer>(&answer).unwrap(),
            AdminAnswer::Failed(_)
        ));
    }
}
