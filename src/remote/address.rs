//! The address of a daemon on another host.
//!
//! The host part is an IPv4 literal or a host name. A literal must be on a
//! network the user's own devices share: loopback, Tailscale or a private
//! range. A host name is resolved on every connection, and each of the first
//! eight IPv4 addresses it resolves to must be in the same ranges (see
//! `resolve`), so a DNS answer can never send the device token to the public
//! internet: an address that is not checked is not connected to either.

use std::fmt;
use std::net::{Ipv4Addr, SocketAddrV4};

use serde::{Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAddress {
    host: Host,
    port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Host {
    Ip(Ipv4Addr),
    /// Lower case, already checked by `valid_name`.
    Name(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressError {
    /// No `:` between the host and the port.
    NotAnAddress,
    /// Neither an IPv4 literal nor a host name.
    BadHost,
    /// Not a decimal number from 1 to 65535 without leading zeros.
    BadPort,
    /// An IPv4 literal that is public, wildcard, link-local or multicast.
    NotAllowed,
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AddressError::NotAnAddress => "the address is not of the form <host>:<port>",
            AddressError::BadHost => "the host is not a host name or an IPv4 address",
            AddressError::BadPort => "the port is not a number from 1 to 65535",
            AddressError::NotAllowed => {
                "the address is not a loopback, Tailscale or private network address"
            }
        })
    }
}

impl RemoteAddress {
    pub fn parse(text: &str) -> Result<Self, AddressError> {
        let (host, port) = text.split_once(':').ok_or(AddressError::NotAnAddress)?;
        // `Ipv4Addr` refuses spaces, signs and leading zeros.
        let host = match host.parse::<Ipv4Addr>() {
            Ok(ip) => {
                let port = parse_port(port)?;
                if !allowed(ip) {
                    return Err(AddressError::NotAllowed);
                }
                return Ok(Self {
                    host: Host::Ip(ip),
                    port,
                });
            }
            Err(_) if valid_name(host) => host.to_ascii_lowercase(),
            Err(_) => return Err(AddressError::BadHost),
        };
        Ok(Self {
            host: Host::Name(host),
            port: parse_port(port)?,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// The host name to resolve, or `None` for an IPv4 literal.
    pub fn name(&self) -> Option<&str> {
        match &self.host {
            Host::Ip(_) => None,
            Host::Name(name) => Some(name),
        }
    }

    /// The address to connect to when the host is an IPv4 literal.
    pub fn literal(&self) -> Option<SocketAddrV4> {
        match &self.host {
            Host::Ip(ip) => Some(SocketAddrV4::new(*ip, self.port)),
            Host::Name(_) => None,
        }
    }

    /// Whether traffic to this address is encrypted on its way: loopback
    /// stays on the host, and Tailscale encrypts with WireGuard. Anything
    /// else is plain HTTP on a local network. A host name is judged by its
    /// spelling, not by what it resolves to: `localhost` and the `.ts.net`
    /// names of a tailnet count as encrypted, every other name does not.
    pub fn is_encrypted_path(&self) -> bool {
        match &self.host {
            Host::Ip(ip) => ip.is_loopback() || tailscale(*ip),
            Host::Name(name) => name == "localhost" || name.ends_with(".ts.net"),
        }
    }
}

impl fmt::Display for RemoteAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.host {
            Host::Ip(ip) => write!(f, "{ip}:{}", self.port),
            Host::Name(name) => write!(f, "{name}:{}", self.port),
        }
    }
}

impl Serialize for RemoteAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

fn parse_port(text: &str) -> Result<u16, AddressError> {
    let plain = !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && !text.starts_with('0');
    if !plain {
        return Err(AddressError::BadPort);
    }
    text.parse().map_err(|_| AddressError::BadPort)
}

/// ASCII letters, digits, hyphens and dots only, so nothing that could end a
/// header line gets into a request. Labels are 1 to 63 characters without a
/// leading or trailing hyphen, the whole name is at most 253, and the last
/// label is not all digits: `0x7f.0.0.1` and `1.2.3` are not names, because
/// a resolver may read them as numbers.
fn valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 {
        return false;
    }
    let mut last = "";
    for label in name.split('.') {
        let plain = !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-');
        if !plain {
            return false;
        }
        last = label;
    }
    !last.bytes().all(|byte| byte.is_ascii_digit())
}

/// 100.64.0.0/10, the range Tailscale hands out.
fn tailscale(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && (64..=127).contains(&b)
}

/// Whether an address, written or resolved, may receive the device token.
pub fn allowed(ip: Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_private() || tailscale(ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<RemoteAddress, AddressError> {
        RemoteAddress::parse(text)
    }

    #[test]
    fn loopback_tailscale_and_private_addresses_are_allowed() {
        for text in [
            "127.0.0.1:7878",
            "100.71.140.68:7878",
            "100.64.0.1:1",
            "100.127.255.254:65535",
            "10.1.2.3:80",
            "172.16.0.1:80",
            "172.31.255.255:80",
            "192.168.50.11:7878",
        ] {
            assert!(parse(text).is_ok(), "{text}");
        }
    }

    #[test]
    fn other_addresses_are_not_allowed() {
        for text in [
            "8.8.8.8:80",
            "0.0.0.0:7878",
            "169.254.1.1:7878",
            "224.0.0.1:7878",
            "255.255.255.255:7878",
            "100.63.255.255:80",
            "100.128.0.0:80",
            "172.15.0.1:80",
            "172.32.0.1:80",
            "192.169.0.1:80",
        ] {
            assert_eq!(parse(text), Err(AddressError::NotAllowed), "{text}");
        }
    }

    #[test]
    fn a_host_is_an_ipv4_literal_or_a_name() {
        for text in [
            "center:7878",
            "Center:7878",
            "center.tailnet.ts.net:7878",
            "a.1b:80",
            "123abc:80",
            "a-b.c:80",
            "localhost:7878",
        ] {
            assert!(parse(text).is_ok(), "{text:?}");
        }
    }

    #[test]
    fn text_without_a_colon_is_not_an_address() {
        for text in ["", "100.71.140.68", "center"] {
            assert_eq!(parse(text), Err(AddressError::NotAnAddress), "{text:?}");
        }
    }

    #[test]
    fn a_host_that_is_neither_a_literal_nor_a_name_is_refused() {
        let too_long_label = format!("{}:80", "a".repeat(64));
        let too_long_name = format!("{}:80", ["abcdefghi"; 26].join("."));
        for text in [
            "[::1]:7878",
            "::1:7878",
            ":7878",
            " 100.71.140.68:7878",
            "100.71.140.68 :7878",
            "100.71.140:7878",
            "1.2.3:80",
            "100.71.140.68.1:7878",
            "100.071.140.68:7878",
            "0x7f.0.0.1:7878",
            "under_score:80",
            "-a:80",
            "a-:80",
            "a..b:80",
            ".a:80",
            "a.:80",
            "caf\u{e9}:80",
            "a b:80",
            "a\r\nb:80",
            too_long_label.as_str(),
            too_long_name.as_str(),
        ] {
            assert_eq!(parse(text), Err(AddressError::BadHost), "{text:?}");
        }
        // The longest name that is still allowed.
        let longest = format!("{}:80", ["abcdefghi"; 25].join(".") + ".abc");
        assert!(parse(&longest).is_ok(), "{longest}");
    }

    #[test]
    fn the_port_is_checked_after_the_host() {
        assert_eq!(
            parse("http://100.71.140.68:7878"),
            Err(AddressError::BadPort)
        );
        assert_eq!(parse("center:"), Err(AddressError::BadPort));
        assert_eq!(parse("center:0"), Err(AddressError::BadPort));
        assert_eq!(parse("center:80:90"), Err(AddressError::BadPort));
        assert_eq!(parse("a b:badport"), Err(AddressError::BadHost));
    }

    #[test]
    fn a_name_is_kept_in_lower_case() {
        let address = parse("Center.TS.Net:7878").unwrap();
        assert_eq!(address.to_string(), "center.ts.net:7878");
        assert_eq!(address.name(), Some("center.ts.net"));
        assert_eq!(address.literal(), None);
        assert_eq!(address.port(), 7878);
    }

    #[test]
    fn an_address_prints_in_a_form_that_parses_back() {
        for text in ["100.71.140.68:7878", "center:7878", "a.b-c.ts.net:1"] {
            let address = parse(text).unwrap();
            assert_eq!(address.to_string(), text);
            assert_eq!(parse(&address.to_string()), Ok(address));
        }
    }

    #[test]
    fn the_port_is_a_plain_number_from_1_to_65535() {
        for text in [
            "127.0.0.1:",
            "127.0.0.1:0",
            "127.0.0.1:07878",
            "127.0.0.1:65536",
            "127.0.0.1:-1",
            "127.0.0.1:+80",
            "127.0.0.1: 80",
            "127.0.0.1:80 ",
            "127.0.0.1:8 0",
            "127.0.0.1:80:90",
            "127.0.0.1:99999999999",
        ] {
            assert_eq!(parse(text), Err(AddressError::BadPort), "{text:?}");
        }
        assert!(parse("127.0.0.1:1").is_ok());
        assert!(parse("127.0.0.1:65535").is_ok());
    }

    #[test]
    fn an_address_prints_as_it_was_written() {
        let address = parse("100.71.140.68:7878").unwrap();
        assert_eq!(address.to_string(), "100.71.140.68:7878");
        assert_eq!(address.literal().unwrap().to_string(), "100.71.140.68:7878");
        assert_eq!(address.name(), None);
    }

    #[test]
    fn only_loopback_and_tailscale_are_encrypted() {
        assert!(parse("127.0.0.1:1").unwrap().is_encrypted_path());
        assert!(parse("100.71.140.68:1").unwrap().is_encrypted_path());
        assert!(!parse("192.168.50.11:1").unwrap().is_encrypted_path());
        assert!(!parse("10.0.0.1:1").unwrap().is_encrypted_path());
    }

    #[test]
    fn a_name_is_judged_by_its_spelling() {
        assert!(parse("localhost:1").unwrap().is_encrypted_path());
        assert!(parse("LOCALHOST:1").unwrap().is_encrypted_path());
        assert!(
            parse("center.tailnet.ts.net:1")
                .unwrap()
                .is_encrypted_path()
        );
        assert!(!parse("center:1").unwrap().is_encrypted_path());
        assert!(!parse("ts.net:1").unwrap().is_encrypted_path());
        assert!(
            !parse("center.ts.net.example.com:1")
                .unwrap()
                .is_encrypted_path()
        );
        assert!(!parse("fakets.net:1").unwrap().is_encrypted_path());
    }

    #[test]
    fn an_address_is_stored_as_text() {
        let address = parse("100.71.140.68:7878").unwrap();
        assert_eq!(
            serde_json::to_string(&address).unwrap(),
            "\"100.71.140.68:7878\""
        );
    }
}
