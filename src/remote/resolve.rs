//! From the address of a daemon to the addresses to connect to.
//!
//! A literal stands for itself. A host name is resolved on every fetch and
//! every pairing, so a daemon whose address changed is found again without
//! pairing anew. Every address it resolves to must be one the device token
//! may be sent to (`address::allowed`): an answer that points anywhere else
//! is refused as a whole, and none of the others is tried instead.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4, ToSocketAddrs};
use std::sync::mpsc;
use std::thread;

use super::address::{RemoteAddress, allowed};
use super::http::Deadline;

/// How many addresses of one name are looked at, and connected to.
const MAX_ADDRESSES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveError {
    /// The resolver reported an error: the name is unknown, or there is no
    /// resolver to ask.
    LookupFailed,
    /// The answer holds no IPv4 address.
    NoAddress,
    /// The answer holds this address, which may not receive the token.
    NotAllowed(Ipv4Addr),
    /// The deadline passed before the answer came.
    Timeout,
}

/// Where the addresses of a host name come from. A test gives its own.
pub trait Resolver {
    /// Returns without waiting for a slow answer past `deadline`.
    fn lookup(
        &self,
        name: &str,
        port: u16,
        deadline: &Deadline,
    ) -> Result<Vec<IpAddr>, ResolveError>;
}

/// The resolver of the system.
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn lookup(
        &self,
        name: &str,
        port: u16,
        deadline: &Deadline,
    ) -> Result<Vec<IpAddr>, ResolveError> {
        let name = name.to_owned();
        lookup_in_thread(deadline, move || {
            (name.as_str(), port)
                .to_socket_addrs()
                .map(|found| found.map(|address| address.ip()).collect())
        })
    }
}

/// Runs a blocking lookup on a thread of its own and waits for it no longer
/// than the deadline allows. The system gives no way to stop a lookup, so a
/// slow one goes on after this returns, and its answer is dropped.
fn lookup_in_thread(
    deadline: &Deadline,
    lookup: impl FnOnce() -> io::Result<Vec<IpAddr>> + Send + 'static,
) -> Result<Vec<IpAddr>, ResolveError> {
    let wait = deadline.remaining().map_err(|_| ResolveError::Timeout)?;
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("resolve".into())
        .spawn(move || {
            // The receiver is gone when nobody waits any longer.
            let _ = sender.send(lookup());
        })
        .map_err(|_| ResolveError::LookupFailed)?;
    match receiver.recv_timeout(wait) {
        Ok(Ok(found)) => Ok(found),
        Ok(Err(_)) => Err(ResolveError::LookupFailed),
        Err(_) => Err(ResolveError::Timeout),
    }
}

/// The addresses to connect to, in the order to try them.
pub fn resolve(
    address: &RemoteAddress,
    resolver: &dyn Resolver,
    deadline: &Deadline,
) -> Result<Vec<SocketAddrV4>, ResolveError> {
    if let Some(literal) = address.literal() {
        return Ok(vec![literal]);
    }
    let name = address.name().expect("an address is a literal or a name");
    let port = address.port();
    // Only IPv4 is used. The first `MAX_ADDRESSES` are the ones that are
    // checked and tried, and no other.
    let found: Vec<Ipv4Addr> = resolver
        .lookup(name, port, deadline)?
        .into_iter()
        .filter_map(|ip| match ip {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        })
        .take(MAX_ADDRESSES)
        .collect();
    if found.is_empty() {
        return Err(ResolveError::NoAddress);
    }
    if let Some(refused) = found.iter().copied().find(|ip| !allowed(*ip)) {
        return Err(ResolveError::NotAllowed(refused));
    }
    Ok(found
        .into_iter()
        .map(|ip| SocketAddrV4::new(ip, port))
        .collect())
}

#[cfg(test)]
pub mod fake {
    use std::cell::Cell;
    use std::net::Ipv6Addr;

    use super::*;

    /// A resolver with a prepared answer. It counts its calls.
    pub struct Fake {
        pub answer: Result<Vec<IpAddr>, ResolveError>,
        pub calls: Cell<usize>,
    }

    impl Fake {
        pub fn answering(ips: &[&str]) -> Self {
            Self {
                answer: Ok(ips.iter().map(|ip| ip.parse().unwrap()).collect()),
                calls: Cell::new(0),
            }
        }

        pub fn failing(error: ResolveError) -> Self {
            Self {
                answer: Err(error),
                calls: Cell::new(0),
            }
        }
    }

    impl Resolver for Fake {
        fn lookup(&self, _: &str, _: u16, _: &Deadline) -> Result<Vec<IpAddr>, ResolveError> {
            self.calls.set(self.calls.get() + 1);
            self.answer.clone()
        }
    }

    pub fn v6() -> IpAddr {
        IpAddr::V6(Ipv6Addr::LOCALHOST)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::fake::{Fake, v6};
    use super::*;

    fn deadline() -> Deadline {
        Deadline::after(Duration::from_secs(5))
    }

    fn addr(text: &str) -> RemoteAddress {
        RemoteAddress::parse(text).unwrap()
    }

    fn sockets(texts: &[&str]) -> Vec<SocketAddrV4> {
        texts.iter().map(|text| text.parse().unwrap()).collect()
    }

    #[test]
    fn a_literal_is_its_own_answer_and_asks_no_resolver() {
        let resolver = Fake::failing(ResolveError::LookupFailed);
        assert_eq!(
            resolve(&addr("100.71.140.68:7878"), &resolver, &deadline()),
            Ok(sockets(&["100.71.140.68:7878"]))
        );
        assert_eq!(resolver.calls.get(), 0);
    }

    #[test]
    fn a_name_gives_its_addresses_with_the_port_in_order() {
        let resolver = Fake::answering(&["100.71.140.68", "192.168.1.5", "127.0.0.1"]);
        assert_eq!(
            resolve(&addr("center:7878"), &resolver, &deadline()),
            Ok(sockets(&[
                "100.71.140.68:7878",
                "192.168.1.5:7878",
                "127.0.0.1:7878"
            ]))
        );
        assert_eq!(resolver.calls.get(), 1);
    }

    #[test]
    fn ipv6_answers_are_dropped() {
        let mut resolver = Fake::answering(&["100.71.140.68"]);
        resolver.answer.as_mut().unwrap().insert(0, v6());
        assert_eq!(
            resolve(&addr("center:7878"), &resolver, &deadline()),
            Ok(sockets(&["100.71.140.68:7878"]))
        );
    }

    #[test]
    fn an_answer_with_only_ipv6_has_no_address() {
        let resolver = Fake {
            answer: Ok(vec![v6()]),
            calls: Default::default(),
        };
        assert_eq!(
            resolve(&addr("center:7878"), &resolver, &deadline()),
            Err(ResolveError::NoAddress)
        );
        let empty = Fake::answering(&[]);
        assert_eq!(
            resolve(&addr("center:7878"), &empty, &deadline()),
            Err(ResolveError::NoAddress)
        );
    }

    #[test]
    fn one_address_that_may_not_receive_the_token_refuses_the_whole_answer() {
        for bad in [
            "8.8.8.8",
            "0.0.0.0",
            "169.254.1.1",
            "100.128.0.1",
            "224.0.0.1",
        ] {
            let resolver = Fake::answering(&["100.71.140.68", bad, "192.168.1.5"]);
            assert_eq!(
                resolve(&addr("center:7878"), &resolver, &deadline()),
                Err(ResolveError::NotAllowed(bad.parse().unwrap())),
                "{bad}"
            );
        }
    }

    #[test]
    fn only_the_first_eight_addresses_are_checked_and_used() {
        let mut ips = vec!["10.0.0.1"; 8];
        ips.push("8.8.8.8");
        let resolver = Fake::answering(&ips);
        assert_eq!(
            resolve(&addr("center:80"), &resolver, &deadline())
                .unwrap()
                .len(),
            8
        );
        // An address in the first eight is checked.
        let mut ips = vec!["10.0.0.1"; 7];
        ips.push("8.8.8.8");
        let resolver = Fake::answering(&ips);
        assert_eq!(
            resolve(&addr("center:80"), &resolver, &deadline()),
            Err(ResolveError::NotAllowed("8.8.8.8".parse().unwrap()))
        );
        // IPv6 answers do not count toward the eight.
        let mut ips = vec![v6(); 10];
        ips.push("10.0.0.2".parse().unwrap());
        let resolver = Fake {
            answer: Ok(ips),
            calls: Default::default(),
        };
        assert_eq!(
            resolve(&addr("center:80"), &resolver, &deadline()),
            Ok(sockets(&["10.0.0.2:80"]))
        );
    }

    #[test]
    fn the_errors_of_the_resolver_are_passed_on() {
        for error in [ResolveError::LookupFailed, ResolveError::Timeout] {
            let resolver = Fake::failing(error);
            assert_eq!(
                resolve(&addr("center:7878"), &resolver, &deadline()),
                Err(error)
            );
        }
    }

    #[test]
    fn a_lookup_that_answers_in_time_gives_its_addresses() {
        let found = lookup_in_thread(&deadline(), || Ok(vec!["10.0.0.1".parse().unwrap()]));
        assert_eq!(found, Ok(vec!["10.0.0.1".parse::<IpAddr>().unwrap()]));
    }

    #[test]
    fn a_lookup_that_fails_is_a_failed_lookup() {
        let found = lookup_in_thread(&deadline(), || Err(io::Error::other("no such host")));
        assert_eq!(found, Err(ResolveError::LookupFailed));
    }

    #[test]
    fn a_slow_lookup_is_a_timeout_that_does_not_hold_the_caller() {
        let started = Instant::now();
        let found = lookup_in_thread(&Deadline::after(Duration::from_millis(100)), || {
            thread::sleep(Duration::from_secs(2));
            Ok(vec!["10.0.0.1".parse().unwrap()])
        });
        assert_eq!(found, Err(ResolveError::Timeout));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_deadline_that_has_passed_starts_no_lookup() {
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = ran.clone();
        let found = lookup_in_thread(&Deadline::after(Duration::ZERO), move || {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(Vec::new())
        });
        assert_eq!(found, Err(ResolveError::Timeout));
        thread::sleep(Duration::from_millis(100));
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn the_system_resolves_localhost() {
        // `localhost` is in every hosts file, so this needs no network.
        let found = SystemResolver.lookup("localhost", 80, &deadline()).unwrap();
        assert!(found.iter().any(|ip| ip.is_loopback()));
    }
}
