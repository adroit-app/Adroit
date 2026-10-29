//! Gateways announced on the local network by the fingerprint of their key.
//! What is heard is only where to try: the key is still proved over TLS.

use crate::{Error, Fingerprint};
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

/// The DNS-SD service type gateways announce.
pub const SERVICE: &str = "_adroit._tcp.local.";
/// The version of what an announcement carries.
const VERSION: &str = "1";
/// Announcements heard at once, beyond which more are ignored.
const MAX_FOUND: usize = 64;

/// This machine's host name, as people are shown it beside a pairing
/// request or an announcement.
pub fn host_name() -> String {
    let mut buffer = [0u8; 256];
    // SAFETY: the buffer and its length match, as gethostname requires.
    let read = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    let name = String::from_utf8_lossy(&buffer[..end]);
    let name = name.trim_end_matches(".local");
    if read == 0 && !name.is_empty() {
        name.to_string()
    } else {
        "this computer".into()
    }
}

/// A gateway's announcement, withdrawn when dropped.
pub struct Advertisement {
    daemon: mdns_sd::ServiceDaemon,
    fullname: String,
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// A gateway heard on the local network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    pub server: Fingerprint,
    pub addresses: Vec<SocketAddr>,
}

/// Announces the gateway whose key is `server` at `port` under `name`.
pub fn advertise(name: &str, server: Fingerprint, port: u16) -> Result<Advertisement, Error> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(discovery)?;
    let id = server.to_string();
    // A host name of its own, so the announcement neither takes nor
    // collides with the name the operating system answers for.
    let host = format!("adroit-{}.local.", &id[..12]);
    let info = mdns_sd::ServiceInfo::new(
        SERVICE,
        name,
        &host,
        "",
        port,
        &[("id", id.as_str()), ("v", VERSION)][..],
    )
    .map_err(discovery)?
    .enable_addr_auto();
    let fullname = info.get_fullname().to_string();
    daemon.register(info).map_err(discovery)?;
    Ok(Advertisement { daemon, fullname })
}

/// Whether this process can send to the mDNS group, by asking it once for
/// gateways. The mDNS daemon drops a refused send without a word, as it is
/// when macOS keeps an app without local network access to the loopback.
pub fn can_multicast() -> std::io::Result<()> {
    let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0))?;
    socket.send_to(&query(), (std::net::Ipv4Addr::new(224, 0, 0, 251), 5353))?;
    Ok(())
}

/// A DNS question for the gateways' service type.
fn query() -> Vec<u8> {
    let mut packet = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in SERVICE.trim_end_matches('.').split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    // The root, then type PTR in class IN.
    packet.extend_from_slice(&[0, 0, 12, 0, 1]);
    packet
}

/// Every gateway heard within `wait`.
pub async fn browse(wait: Duration) -> Result<Vec<Found>, Error> {
    listen(wait, |_| false).await
}

/// Where the gateway with `server`'s key announces itself, if it does
/// within `wait`.
pub async fn find(server: Fingerprint, wait: Duration) -> Result<Option<Found>, Error> {
    let heard = listen(wait, move |found| found.server == server).await?;
    Ok(heard.into_iter().find(|found| found.server == server))
}

/// Collects announcements for `wait`, or until `enough` says one suffices.
async fn listen(
    wait: Duration,
    enough: impl Fn(&Found) -> bool + Send + 'static,
) -> Result<Vec<Found>, Error> {
    tokio::task::spawn_blocking(move || {
        let daemon = mdns_sd::ServiceDaemon::new().map_err(discovery)?;
        let events = daemon.browse(SERVICE).map_err(discovery)?;
        let until = Instant::now() + wait;
        let mut heard: Vec<Found> = Vec::new();
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            let Ok(event) = events.recv_timeout(left) else {
                break;
            };
            let mdns_sd::ServiceEvent::ServiceResolved(service) = event else {
                continue;
            };
            let Some(found) = parse(&service) else {
                continue;
            };
            let done = enough(&found);
            match heard.iter().position(|known| known.server == found.server) {
                Some(index) => heard[index] = found,
                None if heard.len() < MAX_FOUND => heard.push(found),
                None => {}
            }
            if done {
                break;
            }
        }
        let _ = daemon.stop_browse(SERVICE);
        let _ = daemon.shutdown();
        Ok(heard)
    })
    .await
    .map_err(|error| Error::Io(std::io::Error::other(error)))?
}

/// What an announcement says, if it says it in the form gateways use.
fn parse(service: &mdns_sd::ResolvedService) -> Option<Found> {
    if service.get_property_val_str("v") != Some(VERSION) {
        return None;
    }
    let server = service.get_property_val_str("id")?.parse().ok()?;
    let name = service
        .get_fullname()
        .strip_suffix(&format!(".{SERVICE}"))?
        .replace("\\.", ".")
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let mut addresses: Vec<SocketAddr> = service
        .get_addresses()
        .iter()
        .map(|address| address.to_ip_addr())
        // A link-local IPv6 address means nothing without its interface.
        .filter(|address| !matches!(address, IpAddr::V6(v6) if v6.is_unicast_link_local()))
        .map(|address| SocketAddr::new(address, service.get_port()))
        .collect();
    addresses.sort_by_key(|address| (address.is_ipv6(), *address));
    Some(Found {
        name,
        server,
        addresses,
    })
}

fn discovery(error: mdns_sd::Error) -> Error {
    Error::Io(std::io::Error::other(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    #[tokio::test]
    async fn an_announced_gateway_is_found_by_its_key() {
        let server = Identity::generate().unwrap().fingerprint();
        let _announced = advertise("Adroit test", server, 47999).unwrap();
        let found = find(server, Duration::from_secs(10))
            .await
            .unwrap()
            .expect("the announcement is heard");
        assert_eq!(found.server, server);
        assert_eq!(found.name, "Adroit test");
        assert!(!found.addresses.is_empty());
        assert!(
            found
                .addresses
                .iter()
                .all(|address| address.port() == 47999)
        );
    }

    #[test]
    fn the_probe_asks_for_gateways_as_one_mdns_question() {
        let asked = query();
        assert_eq!(&asked[..12], &[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            &asked[12..],
            b"\x07_adroit\x04_tcp\x05local\x00\x00\x0c\x00\x01"
        );
    }

    #[tokio::test]
    async fn a_key_no_one_announces_is_not_found() {
        let nobody = Identity::generate().unwrap().fingerprint();
        assert_eq!(
            find(nobody, Duration::from_millis(500)).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn browsing_lists_an_announced_gateway() {
        let server = Identity::generate().unwrap().fingerprint();
        let _announced = advertise("Adroit listed", server, 47998).unwrap();
        let mut heard = Vec::new();
        for _ in 0..5 {
            heard = browse(Duration::from_secs(2)).await.unwrap();
            if heard.iter().any(|found| found.server == server) {
                break;
            }
        }
        assert!(
            heard
                .iter()
                .any(|found| found.server == server && found.name == "Adroit listed")
        );
    }
}
