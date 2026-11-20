//! The address of a daemon on another host.
//!
//! Only an IPv4 literal is accepted, and only one on a network the user's
//! own devices share: loopback, Tailscale or a private range. A host name is
//! refused, so a DNS answer can never send the device token somewhere else.

use std::fmt;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteAddress {
    ip: Ipv4Addr,
    port: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressError {
    /// Not `a.b.c.d:port` with four plain decimal parts.
    NotIpv4Literal,
    /// Not a decimal number from 1 to 65535 without leading zeros.
    BadPort,
    /// A public, wildcard, link-local or multicast address.
    NotAllowed,
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AddressError::NotIpv4Literal => "the address is not of the form <ip>:<port>",
            AddressError::BadPort => "the port is not a number from 1 to 65535",
            AddressError::NotAllowed => {
                "the address is not a loopback, Tailscale or private network address"
            }
        })
    }
}

impl RemoteAddress {
    pub fn parse(text: &str) -> Result<Self, AddressError> {
        let (host, port) = text.split_once(':').ok_or(AddressError::NotIpv4Literal)?;
        // `Ipv4Addr` refuses spaces, signs and leading zeros.
        let ip: Ipv4Addr = host.parse().map_err(|_| AddressError::NotIpv4Literal)?;
        let port = parse_port(port)?;
        if !allowed(ip) {
            return Err(AddressError::NotAllowed);
        }
        Ok(Self { ip, port })
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(self.ip, self.port))
    }

    /// Whether traffic to this address is encrypted on its way: loopback
    /// stays on the host, and Tailscale encrypts with WireGuard. Anything
    /// else is plain HTTP on a local network.
    pub fn is_encrypted_path(&self) -> bool {
        self.ip.is_loopback() || tailscale(self.ip)
    }
}

impl fmt::Display for RemoteAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

impl Serialize for RemoteAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for RemoteAddress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
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

/// 100.64.0.0/10, the range Tailscale hands out.
fn tailscale(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && (64..=127).contains(&b)
}

fn allowed(ip: Ipv4Addr) -> bool {
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
    fn only_a_plain_ipv4_literal_is_a_host() {
        for text in [
            "",
            "center:7878",
            "center.tailnet.ts.net:7878",
            "100.71.140.68",
            "[::1]:7878",
            "::1:7878",
            " 100.71.140.68:7878",
            "100.71.140.68 :7878",
            "100.71.140:7878",
            "100.71.140.68.1:7878",
            "100.071.140.68:7878",
            "0x7f.0.0.1:7878",
            "http://100.71.140.68:7878",
        ] {
            assert_eq!(parse(text), Err(AddressError::NotIpv4Literal), "{text:?}");
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
        assert_eq!(address.socket_addr().to_string(), "100.71.140.68:7878");
    }

    #[test]
    fn only_loopback_and_tailscale_are_encrypted() {
        assert!(parse("127.0.0.1:1").unwrap().is_encrypted_path());
        assert!(parse("100.71.140.68:1").unwrap().is_encrypted_path());
        assert!(!parse("192.168.50.11:1").unwrap().is_encrypted_path());
        assert!(!parse("10.0.0.1:1").unwrap().is_encrypted_path());
    }

    #[test]
    fn an_address_is_stored_as_text_and_a_bad_one_is_refused() {
        let address = parse("100.71.140.68:7878").unwrap();
        assert_eq!(
            serde_json::to_string(&address).unwrap(),
            "\"100.71.140.68:7878\""
        );
        assert_eq!(
            serde_json::from_str::<RemoteAddress>("\"100.71.140.68:7878\"").unwrap(),
            address
        );
        assert!(serde_json::from_str::<RemoteAddress>("\"8.8.8.8:80\"").is_err());
        assert!(serde_json::from_str::<RemoteAddress>("5").is_err());
    }
}
