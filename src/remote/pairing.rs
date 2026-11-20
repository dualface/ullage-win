//! Pairing this device with a daemon.
//!
//! The user runs `ullage device pair` on the daemon's host, and copies
//! `<ip>:<port> <code>` to the clipboard. This module reads that text, asks
//! the daemon for a device token, and keeps the token and the address
//! together, so that they never get out of step.

use std::fmt;
use std::time::Duration;

use serde::Deserialize;

use super::address::{AddressError, RemoteAddress};
use super::http::{self, HttpError, Request};
use super::store::{Credential, StoreError, Token, TokenStore};

/// The longest a pairing request may take. A fetch has its own limit.
pub const PAIR_TIMEOUT: Duration = Duration::from_secs(15);

const CODE_MAX_LEN: usize = 16;
/// The daemon refuses a longer device name.
const DEVICE_NAME_MAX_LEN: usize = 64;
const FALLBACK_DEVICE_NAME: &str = "ullage-win";

/// The code `ullage device pair` printed: letters, digits and a hyphen.
#[derive(Clone, PartialEq, Eq)]
pub struct PairCode(String);

impl PairCode {
    fn parse(text: &str) -> Option<Self> {
        let valid = !text.is_empty()
            && text.len() <= CODE_MAX_LEN
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        valid.then(|| Self(text.to_owned()))
    }
}

/// A code is a secret for five minutes: it never prints.
impl fmt::Debug for PairCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairCode(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    /// Not two parts separated by white space.
    Format,
    Address(AddressError),
    Code,
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipboardError::Format => {
                f.write_str("The clipboard does not hold \"<ip>:<port> <pairing code>\".")
            }
            ClipboardError::Address(error) => write!(f, "The address is refused: {error}."),
            ClipboardError::Code => f.write_str(
                "The pairing code may hold only letters, digits and hyphens, up to 16 characters.",
            ),
        }
    }
}

/// Reads `<ip>:<port> <code>` from the clipboard's text.
pub fn parse_clipboard(text: &str) -> Result<(RemoteAddress, PairCode), ClipboardError> {
    let mut parts = text.split_whitespace();
    let (Some(address), Some(code), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(ClipboardError::Format);
    };
    let address = RemoteAddress::parse(address).map_err(ClipboardError::Address)?;
    let code = PairCode::parse(code).ok_or(ClipboardError::Code)?;
    Ok((address, code))
}

/// The name this device gives itself: the computer's name, reduced to what
/// the daemon takes, or a fixed name when nothing is left.
pub fn device_name(computer_name: Option<&str>) -> String {
    let name: String = computer_name
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .take(DEVICE_NAME_MAX_LEN)
        .collect();
    if name.is_empty() {
        FALLBACK_DEVICE_NAME.to_owned()
    } else {
        name
    }
}

/// Why a pairing did not finish. None of the words holds the code or a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairError {
    Unreachable,
    /// The daemon did not take the code: wrong, used, expired, or too many
    /// wrong ones before it.
    Refused,
    /// One attempt a second per source address.
    TooFast,
    /// Anything else: an answer this app cannot read.
    Failed,
}

impl fmt::Display for PairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PairError::Unreachable => "The daemon could not be reached.",
            PairError::Refused => {
                "The daemon did not accept the pairing code. Run `ullage device pair` again for a new one."
            }
            PairError::TooFast => "The daemon asks to wait a moment before the next try.",
            PairError::Failed => "The daemon gave an answer this app cannot read.",
        })
    }
}

#[derive(Deserialize)]
struct PairReply {
    device_token: String,
}

/// Asks `address` for a device token with `code`.
pub fn pair(
    address: &RemoteAddress,
    code: &PairCode,
    device_name: &str,
) -> Result<Token, PairError> {
    let body = serde_json::json!({ "pair_code": code.0, "device_name": device_name }).to_string();
    let answer = http::send(
        address,
        &Request {
            method: "POST",
            path: "/v1/pair",
            headers: &[("Content-Type", "application/json")],
            body: Some(body.as_bytes()),
        },
        PAIR_TIMEOUT,
    )
    .map_err(|error| match error {
        HttpError::Unreachable | HttpError::TimedOut => PairError::Unreachable,
        HttpError::Invalid | HttpError::TooLarge => PairError::Failed,
    })?;
    match answer.status {
        200 => serde_json::from_slice::<PairReply>(&answer.body)
            .ok()
            .and_then(|reply| Token::parse(&reply.device_token))
            .ok_or(PairError::Failed),
        401 => Err(PairError::Refused),
        429 => Err(PairError::TooFast),
        _ => Err(PairError::Failed),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// The credential could not be stored. Nothing changed.
    Store,
    /// The settings could not be saved. The credential is as it was before.
    Settings,
    /// The settings could not be saved, and the credential could not be put
    /// back either: the earlier pairing, if there was one, is lost.
    Rollback,
}

/// Keeps a new credential and the settings that name its address as one
/// step. The credential goes first. When the settings then fail to save, the
/// credential is put back as it was, so the token is never left with an
/// address the settings do not name. When it cannot be put back, that is
/// told apart: the caller must not say that nothing changed.
pub fn apply(
    store: &dyn TokenStore,
    credential: &Credential,
    save_settings: impl FnOnce() -> std::io::Result<()>,
) -> Result<(), ApplyError> {
    let before = store.load().map_err(|StoreError| ApplyError::Store)?;
    store.save(credential).map_err(|_| ApplyError::Store)?;
    if save_settings().is_ok() {
        return Ok(());
    }
    let restored = match &before {
        Some(earlier) => store.save(earlier),
        None => store.delete(),
    };
    Err(if restored.is_ok() {
        ApplyError::Settings
    } else {
        ApplyError::Rollback
    })
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::super::store::memory::MemoryStore;
    use super::super::testserver::{Reply, reply, serve};
    use super::*;

    fn address() -> RemoteAddress {
        RemoteAddress::parse("100.71.140.68:7878").unwrap()
    }

    fn credential(token: &str) -> Credential {
        Credential {
            address: address(),
            token: Token::parse(token).unwrap(),
        }
    }

    #[test]
    fn the_clipboard_holds_an_address_and_a_code() {
        let (parsed, code) = parse_clipboard("100.71.140.68:7878 ABC-DEF").unwrap();
        assert_eq!(parsed, address());
        assert_eq!(code.0, "ABC-DEF");
        assert!(parse_clipboard("  100.71.140.68:7878 \t abcdef\r\n").is_ok());
        assert!(parse_clipboard("100.71.140.68:7878 ABCDEF").is_ok());
    }

    #[test]
    fn anything_else_on_the_clipboard_is_refused() {
        for text in [
            "",
            "100.71.140.68:7878",
            "ABC-DEF",
            "a b c",
            "1.2.3.4:5 X Y",
        ] {
            assert!(parse_clipboard(text).is_err(), "{text:?}");
        }
        assert_eq!(
            parse_clipboard("100.71.140.68:7878 AB CD"),
            Err(ClipboardError::Format)
        );
        assert_eq!(
            parse_clipboard("8.8.8.8:7878 ABC-DEF"),
            Err(ClipboardError::Address(AddressError::NotAllowed))
        );
        assert_eq!(
            parse_clipboard("center:7878 ABC-DEF"),
            Err(ClipboardError::Address(AddressError::NotIpv4Literal))
        );
        for code in ["AB!CD", "AB\u{0}CD", "ÄBC", &"A".repeat(17)] {
            assert_eq!(
                parse_clipboard(&format!("100.71.140.68:7878 {code}")),
                Err(ClipboardError::Code),
                "{code:?}"
            );
        }
        assert!(parse_clipboard(&format!("100.71.140.68:7878 {}", "A".repeat(16))).is_ok());
    }

    #[test]
    fn a_code_never_prints() {
        let (_, code) = parse_clipboard("100.71.140.68:7878 ABC-DEF").unwrap();
        assert!(!format!("{code:?}").contains("ABC"));
        for error in [
            PairError::Unreachable,
            PairError::Refused,
            PairError::TooFast,
            PairError::Failed,
        ] {
            assert!(!error.to_string().contains("ABC-DEF"));
        }
    }

    #[test]
    fn the_device_name_is_what_the_daemon_takes() {
        assert_eq!(device_name(Some("FLY")), "FLY");
        assert_eq!(device_name(Some("my pc!")), "mypc");
        assert_eq!(device_name(Some("名前")), "ullage-win");
        assert_eq!(device_name(Some("")), "ullage-win");
        assert_eq!(device_name(None), "ullage-win");
        assert_eq!(device_name(Some(&"a".repeat(100))).len(), 64);
    }

    #[test]
    fn a_pairing_posts_the_code_and_takes_the_token() {
        let token = "A".repeat(43);
        let body =
            format!("{{\"device_id\":\"1\",\"device_token\":\"{token}\",\"device_name\":\"FLY\"}}");
        let server = serve(vec![Reply::Raw(super::super::testserver::ok(
            body.as_bytes(),
        ))]);
        let (_, code) = parse_clipboard(&format!("{} ABC-DEF", server.address)).unwrap();
        let taken = pair(&server.address, &code, "FLY").unwrap();
        assert_eq!(taken.expose(), token);
        let seen = server.finish();
        let head = &seen[0].head;
        assert!(head.starts_with("POST /v1/pair HTTP/1.1\r\n"), "{head}");
        assert!(head.contains("Content-Type: application/json\r\n"));
        assert!(head.contains(&format!("Host: {}\r\n", seen[0].address)));
        assert!(!head.contains("Authorization"));
        let sent: serde_json::Value = serde_json::from_slice(&seen[0].body).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({"pair_code": "ABC-DEF", "device_name": "FLY"})
        );
    }

    fn pair_with(raw: Vec<u8>) -> Result<Token, PairError> {
        let server = serve(vec![Reply::Raw(raw)]);
        let (_, code) = parse_clipboard(&format!("{} ABC-DEF", server.address)).unwrap();
        pair(&server.address, &code, "FLY")
    }

    #[test]
    fn a_pairing_that_fails_says_why() {
        let json = &[("Content-Type", "application/json")];
        assert_eq!(
            pair_with(reply(
                "401 Unauthorized",
                json,
                b"{\"error\":\"pair_code_invalid\"}"
            )),
            Err(PairError::Refused)
        );
        assert_eq!(
            pair_with(reply("429 Too Many Requests", json, b"{}")),
            Err(PairError::TooFast)
        );
        for raw in [
            reply("500 Internal Server Error", json, b"{}"),
            reply("400 Bad Request", json, b"{\"error\":\"bad_request\"}"),
            reply("200 OK", json, b"not json"),
            reply("200 OK", json, b"{\"device_token\":\"\"}"),
            reply("200 OK", json, b"{\"device_token\":\"has space\"}"),
            reply("200 OK", json, b"{\"device_token\":\"a\\r\\nb\"}"),
            reply("200 OK", json, b"{}"),
        ] {
            assert_eq!(pair_with(raw), Err(PairError::Failed));
        }
        let gone = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let gone = RemoteAddress::parse(&listener.local_addr().unwrap().to_string());
            drop(listener);
            gone.unwrap()
        };
        let (_, code) = parse_clipboard(&format!("{gone} ABC-DEF")).unwrap();
        assert_eq!(pair(&gone, &code, "FLY"), Err(PairError::Unreachable));
    }

    #[test]
    fn the_new_credential_replaces_the_old_one_once_the_settings_are_saved() {
        let store = MemoryStore::with(credential("old-token"));
        let new = credential("new-token");
        apply(&store, &new, || Ok(())).unwrap();
        assert_eq!(store.load().unwrap(), Some(new));
    }

    #[test]
    fn a_settings_failure_puts_the_old_credential_back() {
        let old = credential("old-token");
        let store = MemoryStore::with(old.clone());
        let result = apply(&store, &credential("new-token"), || {
            Err(io::Error::other("disk full"))
        });
        assert_eq!(result, Err(ApplyError::Settings));
        assert_eq!(store.load().unwrap(), Some(old));
    }

    #[test]
    fn a_settings_failure_without_an_old_credential_leaves_none() {
        let store = MemoryStore::default();
        let result = apply(&store, &credential("new-token"), || {
            Err(io::Error::other("disk full"))
        });
        assert_eq!(result, Err(ApplyError::Settings));
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn a_credential_that_cannot_be_put_back_is_told_apart() {
        let store = MemoryStore::with(credential("old-token"));
        // The first save works; the one that puts the old credential back
        // does not.
        store.fail_saves_after.set(Some(1));
        let result = apply(&store, &credential("new-token"), || {
            Err(io::Error::other("disk full"))
        });
        assert_eq!(result, Err(ApplyError::Rollback));
    }

    #[test]
    fn a_store_failure_changes_nothing_and_does_not_save_settings() {
        let old = credential("old-token");
        let store = MemoryStore::with(old.clone());
        store.fail_saves_after.set(Some(0));
        let mut saved = false;
        let result = apply(&store, &credential("new-token"), || {
            saved = true;
            Ok(())
        });
        assert_eq!(result, Err(ApplyError::Store));
        assert!(!saved);
        assert_eq!(store.load().unwrap(), Some(old));
    }
}
