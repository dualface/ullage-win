//! Pairing this device with a daemon.
//!
//! The user runs `ullage device pair` on the daemon's host, and types the
//! address of the daemon and the pairing code into the pairing dialog. This
//! module checks what was typed, asks the daemon for a device token, and
//! keeps the token and the address together, so that they never get out of
//! step.

use std::fmt;
use std::net::Ipv4Addr;
use std::time::Duration;

use serde::Deserialize;

use super::address::RemoteAddress;
use super::http::{self, Deadline, HttpError, Request};
use super::resolve::{ResolveError, Resolver, SystemResolver, resolve};
use super::store::{Credential, StoreError, Token, TokenStore};

/// The longest a pairing request may take. A fetch has its own limit.
pub const PAIR_TIMEOUT: Duration = Duration::from_secs(15);

/// The daemon builds its pairing codes from these characters: no `0`, `1`,
/// `I`, `L`, `O` or `U`, which are easy to mix up.
pub const CODE_ALPHABET: &str = "23456789ABCDEFGHJKMNPQRSTVWXYZ";
/// The daemon refuses a longer device name.
const DEVICE_NAME_MAX_LEN: usize = 64;
const FALLBACK_DEVICE_NAME: &str = "ullage-win";

/// The code `ullage device pair` printed, in the one form this app takes:
/// `XXX-XXX`, six characters of [`CODE_ALPHABET`] in capitals.
#[derive(Clone, PartialEq, Eq)]
pub struct PairCode(String);

impl PairCode {
    pub fn parse(text: &str) -> Option<Self> {
        let valid = text.len() == 7
            && text.bytes().enumerate().all(|(index, byte)| match index {
                3 => byte == b'-',
                _ => CODE_ALPHABET.as_bytes().contains(&byte),
            });
        valid.then(|| Self(text.to_owned()))
    }
}

/// What the code box of the pairing dialog holds after an edit: capitals
/// only, nothing that is not in [`CODE_ALPHABET`], at most six characters,
/// and a hyphen after the third. Applying it to its own result changes
/// nothing, so the hyphen the dialog adds can be taken away again with the
/// backspace key.
pub fn format_pair_code_input(raw: &str) -> String {
    let mut formatted = String::with_capacity(7);
    let kept = raw
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| CODE_ALPHABET.contains(*c))
        .take(6);
    for (index, c) in kept.enumerate() {
        if index == 3 {
            formatted.push('-');
        }
        formatted.push(c);
    }
    formatted
}

/// Whether the dialog may take what was typed: an address that
/// `RemoteAddress::parse` takes, and a code of exactly the form `XXX-XXX`.
pub fn can_submit(address_text: &str, code_text: &str) -> bool {
    RemoteAddress::parse(address_text).is_ok() && PairCode::parse(code_text).is_some()
}

/// A code is a secret for five minutes: it never prints.
impl fmt::Debug for PairCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairCode(<redacted>)")
    }
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
    /// The name of the daemon resolved to this address, which may not
    /// receive a pairing code. Nothing was sent.
    AddressRefused(Ipv4Addr),
    /// One attempt a second per source address.
    TooFast,
    /// Anything else: an answer this app cannot read.
    Failed,
}

impl fmt::Display for PairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let PairError::AddressRefused(ip) = self {
            return write!(
                f,
                "The name resolves to {ip}, which is not a loopback, Tailscale or private \
                 address, so nothing was sent."
            );
        }
        f.write_str(match self {
            PairError::AddressRefused(_) => unreachable!("handled above"),
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
    pair_by(address, code, device_name, Deadline::after(PAIR_TIMEOUT))
}

fn pair_by(
    address: &RemoteAddress,
    code: &PairCode,
    device_name: &str,
    deadline: Deadline,
) -> Result<Token, PairError> {
    pair_in(address, code, device_name, deadline, &SystemResolver)
}

/// [`pair`] with the deadline of the whole pairing: it covers the lookup of a
/// host name, the request, the answer and the reading of the token, and it is
/// the last word. A token that is ready only after it is not given.
fn pair_in(
    address: &RemoteAddress,
    code: &PairCode,
    device_name: &str,
    deadline: Deadline,
    resolver: &dyn Resolver,
) -> Result<Token, PairError> {
    let result = ask(address, code, device_name, &deadline, resolver);
    deadline.judge(result, PairError::Unreachable)
}

fn ask(
    address: &RemoteAddress,
    code: &PairCode,
    device_name: &str,
    deadline: &Deadline,
    resolver: &dyn Resolver,
) -> Result<Token, PairError> {
    let addresses = resolve(address, resolver, deadline).map_err(|error| match error {
        ResolveError::NotAllowed(ip) => PairError::AddressRefused(ip),
        _ => PairError::Unreachable,
    })?;
    let body = serde_json::json!({ "pair_code": code.0, "device_name": device_name }).to_string();
    let answer = http::send(
        &addresses,
        &Request {
            method: "POST",
            path: "/v1/pair",
            headers: &[("Content-Type", "application/json")],
            body: Some(body.as_bytes()),
        },
        deadline,
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

/// Which pairing may still be kept. A pairing takes time, and what the user
/// does meanwhile can make it stale: forgetting the remote daemon, choosing a
/// connection, or starting another pairing. A result that comes in after that
/// belongs to an epoch that is over and is dropped, so that a late success
/// never writes back a credential the user has just removed.
#[derive(Debug, Default)]
pub struct PairingEpoch(u32);

impl PairingEpoch {
    /// A pairing starts. It is the only one that may be kept.
    pub fn begin(&mut self) -> u32 {
        self.invalidate();
        self.0
    }

    /// The user did something that makes every pairing on its way stale.
    pub fn invalidate(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }

    pub fn is_current(&self, epoch: u32) -> bool {
        self.0 == epoch
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgetError {
    /// The saved credential could not be read. Nothing was changed.
    Read,
    /// The credential could not be deleted. Nothing was changed.
    Delete,
    /// The settings could not be saved. The credential is as it was before.
    Settings,
    /// The settings could not be saved, and the credential could not be put
    /// back: the pairing is lost.
    Rollback,
}

/// Deletes the credential and saves the settings that no longer name the
/// remote daemon, as one step. The credential is read first: what cannot be
/// read cannot be put back, so nothing is deleted. When the settings then
/// fail to save, the credential goes back.
pub fn forget(
    store: &dyn TokenStore,
    save_settings: impl FnOnce() -> std::io::Result<()>,
) -> Result<(), ForgetError> {
    let before = store.load().map_err(|StoreError| ForgetError::Read)?;
    store.delete().map_err(|StoreError| ForgetError::Delete)?;
    if save_settings().is_ok() {
        return Ok(());
    }
    let restored = match &before {
        Some(earlier) => store.save(earlier),
        None => Ok(()),
    };
    Err(if restored.is_ok() {
        ForgetError::Settings
    } else {
        ForgetError::Rollback
    })
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::super::resolve::fake::Fake;
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

    fn code(text: &str) -> PairCode {
        PairCode::parse(text).unwrap()
    }

    #[test]
    fn a_code_is_three_characters_a_hyphen_and_three_characters() {
        assert_eq!(code("ABC-DEF").0, "ABC-DEF");
        assert!(PairCode::parse("234-XYZ").is_some());
        assert!(PairCode::parse("HJK-MNP").is_some());
    }

    #[test]
    fn any_other_form_of_a_code_is_refused() {
        for text in [
            "",
            "ABC",
            "ABCDEF",
            "abc-def",
            "Abc-DEF",
            "ABC-DE",
            "ABC-DEFG",
            "ABCD-EF",
            "AB-CDEF",
            "ABC DEF",
            "ABC_DEF",
            "ABC--DE",
            "-ABCDEF",
            "ABCDEF-",
            " ABC-DEF",
            "ABC-DEF ",
            "ABC-DEF\n",
            "ABC-DE\u{0}",
            // Characters the daemon never uses.
            "0BC-DEF",
            "1BC-DEF",
            "IBC-DEF",
            "LBC-DEF",
            "OBC-DEF",
            "UBC-DEF",
            "ABC-DE!",
            "ÄBC-DEF",
            // Seven bytes, but not seven characters.
            "ABC-D\u{c4}",
        ] {
            assert!(PairCode::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn the_input_of_the_code_box_is_made_into_the_form_of_a_code() {
        for (raw, formatted) in [
            ("", ""),
            ("a", "A"),
            ("abc", "ABC"),
            ("abcd", "ABC-D"),
            ("abcdef", "ABC-DEF"),
            ("abc-def", "ABC-DEF"),
            ("a0b1c2", "ABC-2"),
            ("ABCDEFGH", "ABC-DEF"),
            (" abc-def\r\n", "ABC-DEF"),
            ("a b c d e f", "ABC-DEF"),
            ("ßabc", "ABC"),
            ("ä!?-", ""),
            ("0101OILU", ""),
            ("ABC-", "ABC"),
            ("-", ""),
        ] {
            assert_eq!(format_pair_code_input(raw), formatted, "{raw:?}");
        }
    }

    #[test]
    fn the_form_of_the_input_does_not_change_when_it_is_formatted_again() {
        for raw in [
            "abc",
            "abcdef",
            "a-b-c-d-e-f",
            "ABC-",
            "x",
            "ZZZZZZZZ",
            "2-3-4",
        ] {
            let once = format_pair_code_input(raw);
            assert_eq!(format_pair_code_input(&once), once, "{raw:?}");
            assert!(once.len() <= 7, "{raw:?}");
        }
        // A whole code, however it is typed, is a code.
        assert!(PairCode::parse(&format_pair_code_input("abcdef")).is_some());
        assert!(PairCode::parse(&format_pair_code_input("abc-def")).is_some());
        assert!(PairCode::parse(&format_pair_code_input("abcde")).is_none());
    }

    #[test]
    fn the_dialog_takes_a_good_address_with_a_whole_code_and_nothing_else() {
        assert!(can_submit("100.71.140.68:7878", "ABC-DEF"));
        assert!(can_submit("center:7878", "234-XYZ"));
        for (address, code) in [
            ("", "ABC-DEF"),
            ("center", "ABC-DEF"),
            ("8.8.8.8:7878", "ABC-DEF"),
            ("center:0", "ABC-DEF"),
            ("under_score:1", "ABC-DEF"),
            ("center:7878", ""),
            ("center:7878", "ABC-DE"),
            ("center:7878", "abc-def"),
            ("center:7878", "ABCDEF"),
            ("", ""),
        ] {
            assert!(!can_submit(address, code), "{address:?} {code:?}");
        }
    }

    #[test]
    fn a_token_that_is_ready_after_the_deadline_is_not_given() {
        let body = format!("{{\"device_token\":\"{}\"}}", "A".repeat(43));
        for reply in [
            super::super::testserver::ok(body.as_bytes()),
            reply("401 Unauthorized", &[], b"{}"),
            reply("500 Internal Server Error", &[], b"{}"),
        ] {
            let server = serve(vec![Reply::Raw(reply)]);
            let code = code("ABC-DEF");
            let late = Deadline::late_after(Duration::from_secs(5));
            assert_eq!(
                pair_by(&server.address, &code, "FLY", late),
                Err(PairError::Unreachable)
            );
        }
    }

    #[test]
    fn a_pairing_may_be_kept_only_until_the_user_does_something_that_makes_it_stale() {
        let mut epoch = PairingEpoch::default();
        let first = epoch.begin();
        assert!(epoch.is_current(first));
        // Forgetting, or choosing a connection, makes it stale.
        epoch.invalidate();
        assert!(!epoch.is_current(first));
        // So does starting another pairing: only the newest may be kept.
        let second = epoch.begin();
        let third = epoch.begin();
        assert!(!epoch.is_current(second));
        assert!(epoch.is_current(third));
        // The count wraps without making an old pairing current again.
        let mut wrapped = PairingEpoch(u32::MAX);
        let old = wrapped.0;
        wrapped.invalidate();
        assert!(!wrapped.is_current(old));
    }

    #[test]
    fn a_code_never_prints() {
        let code = code("ABC-DEF");
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

    fn pair_named(
        resolver: &Fake,
        server: &super::super::testserver::Server,
    ) -> Result<Token, PairError> {
        let address = RemoteAddress::parse(&format!("center:{}", server.address.port())).unwrap();
        let code = PairCode::parse("ABC-DEF").unwrap();
        pair_in(
            &address,
            &code,
            "FLY",
            Deadline::after(Duration::from_secs(5)),
            resolver,
        )
    }

    #[test]
    fn a_pairing_with_a_host_name_goes_to_the_address_it_resolves_to() {
        let body = format!("{{\"device_token\":\"{}\"}}", "A".repeat(43));
        let server = serve(vec![Reply::Raw(super::super::testserver::ok(
            body.as_bytes(),
        ))]);
        let resolver = Fake::answering(&["127.0.0.1"]);
        assert!(pair_named(&resolver, &server).is_ok());
        let seen = server.finish();
        // `Host` is the address connected to, never the name.
        let host = format!("Host: 127.0.0.1:{}\r\n", seen[0].address.port());
        assert!(seen[0].head.contains(&host), "{}", seen[0].head);
    }

    #[test]
    fn a_name_that_resolves_to_a_public_address_gets_no_pairing_code() {
        let server = serve(vec![Reply::Raw(super::super::testserver::ok(b"{}"))]);
        let resolver = Fake::answering(&["127.0.0.1", "8.8.8.8"]);
        let ip: Ipv4Addr = "8.8.8.8".parse().unwrap();
        assert_eq!(
            pair_named(&resolver, &server).unwrap_err(),
            PairError::AddressRefused(ip)
        );
        let (seen, accepted) = server.finish_after(Duration::from_millis(200));
        assert!(seen.is_empty());
        assert_eq!(accepted, 0);
        // The words name the address and hold no code.
        let words = PairError::AddressRefused(ip).to_string();
        assert!(words.contains("8.8.8.8") && !words.contains("ABC"));
    }

    #[test]
    fn a_name_that_does_not_resolve_is_unreachable() {
        let server = serve(vec![Reply::Raw(super::super::testserver::ok(b"{}"))]);
        for error in [
            ResolveError::LookupFailed,
            ResolveError::NoAddress,
            ResolveError::Timeout,
        ] {
            let resolver = Fake::failing(error);
            assert_eq!(
                pair_named(&resolver, &server).unwrap_err(),
                PairError::Unreachable
            );
        }
        let (seen, _) = server.finish_after(Duration::from_millis(100));
        assert!(seen.is_empty());
    }

    #[test]
    fn a_pairing_posts_the_code_and_takes_the_token() {
        let token = "A".repeat(43);
        let body =
            format!("{{\"device_id\":\"1\",\"device_token\":\"{token}\",\"device_name\":\"FLY\"}}");
        let server = serve(vec![Reply::Raw(super::super::testserver::ok(
            body.as_bytes(),
        ))]);
        let code = code("ABC-DEF");
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
        let code = code("ABC-DEF");
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
        let code = code("ABC-DEF");
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
    fn forgetting_deletes_the_credential_once_the_settings_are_saved() {
        let store = MemoryStore::with(credential("old-token"));
        forget(&store, || Ok(())).unwrap();
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn a_credential_that_cannot_be_read_is_not_deleted() {
        let store = MemoryStore::with(credential("old-token"));
        store.fail_load.set(true);
        let mut saved = false;
        let result = forget(&store, || {
            saved = true;
            Ok(())
        });
        assert_eq!(result, Err(ForgetError::Read));
        assert_eq!(store.deletes.get(), 0);
        assert!(!saved);
    }

    #[test]
    fn forgetting_with_settings_that_do_not_save_puts_the_credential_back() {
        let old = credential("old-token");
        let store = MemoryStore::with(old.clone());
        let result = forget(&store, || Err(io::Error::other("disk full")));
        assert_eq!(result, Err(ForgetError::Settings));
        assert_eq!(store.load().unwrap(), Some(old));
        // A credential that cannot be put back is told apart.
        let store = MemoryStore::with(credential("old-token"));
        store.fail_saves_after.set(Some(0));
        let result = forget(&store, || Err(io::Error::other("disk full")));
        assert_eq!(result, Err(ForgetError::Rollback));
    }

    #[test]
    fn forgetting_when_there_is_no_credential_still_saves_the_settings() {
        let store = MemoryStore::default();
        let mut saved = false;
        forget(&store, || {
            saved = true;
            Ok(())
        })
        .unwrap();
        assert!(saved);
        // And with settings that fail there is nothing to put back.
        let result = forget(&store, || Err(io::Error::other("disk full")));
        assert_eq!(result, Err(ForgetError::Settings));
        assert_eq!(store.load().unwrap(), None);
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
