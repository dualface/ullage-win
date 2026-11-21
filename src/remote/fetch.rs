//! One fetch from a remote daemon: the saved credential, the client, and
//! `fetch_usage` of `ullage-cli`.

use ullage_cli::usage_view::{FetchError, UsageFetch, fetch_usage};

use super::address::RemoteAddress;
use super::client::{FETCH_TIMEOUT, HttpClient};
use super::http::Deadline;
use super::limits::within_limits;
use super::resolve::{Resolver, SystemResolver};
use super::store::{TokenStore, credential_for};
use crate::state::FailureKind;

/// Fetches the usage from `address`, with the token saved for that address.
/// A token saved for another address, or none, or one that cannot be read,
/// is "not paired" when the check is done in time, and nothing is sent. When
/// the check is done only after the deadline, the result is the timeout
/// instead, again with nothing sent, so that a slow credential store does not
/// clear what the window shows. A reply that `fetch_usage` accepts but that
/// is over the limits of [`super::limits`] is an invalid one.
pub fn fetch_remote(
    store: &dyn TokenStore,
    address: &RemoteAddress,
) -> Result<UsageFetch, FailureKind> {
    fetch_remote_by(store, address, Deadline::after(FETCH_TIMEOUT))
}

/// [`fetch_remote`] with the deadline of the whole fetch. It is passed to
/// the client, so that the transport, the reading of the answer, and the
/// checks of `fetch_usage` all share it, and it is the last word: a result
/// that is ready only after it, a fetch or an error, is a timeout.
fn fetch_remote_by(
    store: &dyn TokenStore,
    address: &RemoteAddress,
    deadline: Deadline,
) -> Result<UsageFetch, FailureKind> {
    fetch_remote_in(store, address, deadline, &SystemResolver)
}

/// [`fetch_remote_by`] with the resolver of host names a test may replace.
fn fetch_remote_in(
    store: &dyn TokenStore,
    address: &RemoteAddress,
    deadline: Deadline,
    resolver: &dyn Resolver,
) -> Result<UsageFetch, FailureKind> {
    let result = match credential_for(store, address) {
        // The answer to "not paired" is judged too: the credential store may
        // have been slow, and a late answer is a timeout, which keeps what the
        // window shows.
        None => Err(FailureKind::NotPaired),
        Some(credential) => {
            let client =
                HttpClient::with_resolver(address.clone(), credential.token, deadline, resolver);
            fetch_usage(&client)
                .map_err(|error| {
                    // `ClientError` has no word for a refused token, or for
                    // an address that may not receive it: the client noted
                    // them.
                    if error == FetchError::InvalidResponse && client.was_rejected() {
                        FailureKind::TokenRejected
                    } else if let (FetchError::InvalidResponse, Some(ip)) =
                        (&error, client.refused_address())
                    {
                        FailureKind::AddressRefused(ip)
                    } else {
                        FailureKind::Fetch(error)
                    }
                })
                // A reply over the limits is an invalid one, and is judged
                // below with the deadline like any other.
                .and_then(|fetch| {
                    if within_limits(&fetch.snapshots) {
                        Ok(fetch)
                    } else {
                        Err(FailureKind::Fetch(FetchError::InvalidResponse))
                    }
                })
        }
    };
    deadline.judge(result, FailureKind::Fetch(FetchError::DaemonUnavailable))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use ullage_protocol::{CONTROL_PROTOCOL_VERSION, ControlError, ControlResult, SnapshotPayload};

    use super::super::limits::{
        MAX_STRING_BYTES,
        tests::{other, reading, snapshot, usage_mut, window},
    };
    use super::super::resolve::{ResolveError, fake::Fake};
    use super::super::store::{Credential, Token, memory::MemoryStore};
    use super::super::testserver::{Reply, ok, reply, serve};
    use super::*;
    use crate::state::failure_for_kind;

    fn store_for(address: RemoteAddress) -> MemoryStore {
        MemoryStore::with(Credential {
            address,
            token: Token::parse("tok_1").unwrap(),
        })
    }

    /// What `ullage-http` sends: the result, plus `version` and `request_id`.
    fn body(result: ControlResult) -> Vec<u8> {
        let mut document = serde_json::to_value(result).unwrap();
        let map = document.as_object_mut().unwrap();
        map.insert("version".into(), json!(CONTROL_PROTOCOL_VERSION));
        map.insert("request_id".into(), json!("http-1"));
        serde_json::to_vec(&document).unwrap()
    }

    /// What a daemon gives, and what is over the limits: a label too long.
    fn normal_snapshots() -> Vec<SnapshotPayload> {
        let mut one = snapshot("a1", vec![window(other("w", "W"), vec![reading("usage")])]);
        usage_mut(&mut one).windows[0].measurements[0].used = 42.5;
        vec![one, snapshot("a2", Vec::new())]
    }

    /// Every text of the daemon's reply that the window must never show is
    /// `marker`, and the number is one that nothing else in a failure has.
    const MARKER: &str = "LEAK-MARKER-7f3a";
    const NUMBER: f64 = 987_654.25;

    fn oversized_snapshots() -> Vec<SnapshotPayload> {
        let label = MARKER.repeat(MAX_STRING_BYTES / MARKER.len() + 1);
        let mut measurement = reading(MARKER);
        measurement.used = NUMBER;
        let mut one = snapshot("a1", vec![window(other(MARKER, &label), vec![measurement])]);
        usage_mut(&mut one).account_label = Some(MARKER.into());
        vec![one]
    }

    fn named(server: &super::super::testserver::Server) -> RemoteAddress {
        RemoteAddress::parse(&format!("center:{}", server.address.port())).unwrap()
    }

    #[test]
    fn a_host_name_is_fetched_through_its_resolved_address() {
        let server = serve(vec![Reply::Raw(ok(&body(ControlResult::Snapshots(
            Vec::new(),
        ))))]);
        let address = named(&server);
        let resolver = Fake::answering(&["127.0.0.1"]);
        let fetched = fetch_remote_in(
            &store_for(address.clone()),
            &address,
            Deadline::after(FETCH_TIMEOUT),
            &resolver,
        )
        .unwrap();
        assert!(fetched.snapshots.is_empty());
    }

    #[test]
    fn a_token_saved_for_a_name_is_not_used_for_another_name_of_the_same_host() {
        let server = serve(vec![Reply::Raw(ok(&body(ControlResult::Snapshots(
            Vec::new(),
        ))))]);
        let saved = RemoteAddress::parse(&format!("center:{}", server.address.port())).unwrap();
        let other = RemoteAddress::parse(&format!("nas:{}", server.address.port())).unwrap();
        let resolver = Fake::answering(&["127.0.0.1"]);
        for asked in [other, server.address.clone()] {
            assert_eq!(
                fetch_remote_in(
                    &store_for(saved.clone()),
                    &asked,
                    Deadline::after(FETCH_TIMEOUT),
                    &resolver
                )
                .unwrap_err(),
                FailureKind::NotPaired
            );
        }
        assert_eq!(resolver.calls.get(), 0);
        let (seen, accepted) = server.finish_after(Duration::from_millis(200));
        assert!(seen.is_empty());
        assert_eq!(accepted, 0);
    }

    #[test]
    fn a_name_that_resolves_to_a_public_address_is_refused_with_that_address() {
        let address = RemoteAddress::parse("center:7878").unwrap();
        let resolver = Fake::answering(&["100.71.140.68", "8.8.8.8"]);
        assert_eq!(
            fetch_remote_in(
                &store_for(address.clone()),
                &address,
                Deadline::after(FETCH_TIMEOUT),
                &resolver
            )
            .unwrap_err(),
            FailureKind::AddressRefused("8.8.8.8".parse().unwrap())
        );
    }

    #[test]
    fn a_name_that_does_not_resolve_is_an_unavailable_daemon() {
        let address = RemoteAddress::parse("center:7878").unwrap();
        for error in [ResolveError::LookupFailed, ResolveError::NoAddress] {
            let resolver = Fake::failing(error);
            assert_eq!(
                fetch_remote_in(
                    &store_for(address.clone()),
                    &address,
                    Deadline::after(FETCH_TIMEOUT),
                    &resolver
                )
                .unwrap_err(),
                FailureKind::Fetch(FetchError::DaemonUnavailable)
            );
        }
    }

    #[test]
    fn a_working_daemon_gives_its_snapshots() {
        let server = serve(vec![Reply::Raw(ok(&body(ControlResult::Snapshots(
            Vec::new(),
        ))))]);
        let fetched = fetch_remote(&store_for(server.address.clone()), &server.address).unwrap();
        assert!(fetched.snapshots.is_empty());
    }

    #[test]
    fn without_a_credential_for_the_address_nothing_is_sent() {
        // A daemon that is listening, with an answer ready: nothing may ask.
        let server = serve(vec![Reply::Raw(ok(&body(ControlResult::Snapshots(
            Vec::new(),
        ))))]);
        let other = RemoteAddress::parse("10.0.0.1:7878").unwrap();
        for store in [MemoryStore::default(), store_for(other)] {
            assert_eq!(
                fetch_remote(&store, &server.address).unwrap_err(),
                FailureKind::NotPaired
            );
        }
        // Not even a connection was made.
        let (seen, accepted) = server.finish_after(Duration::from_millis(200));
        assert!(seen.is_empty());
        assert_eq!(accepted, 0);
    }

    #[test]
    fn a_refused_token_is_told_from_an_unreadable_answer() {
        let server = serve(vec![Reply::Raw(reply("401 Unauthorized", &[], b"{}"))]);
        assert_eq!(
            fetch_remote(&store_for(server.address.clone()), &server.address).unwrap_err(),
            FailureKind::TokenRejected
        );
        let server = serve(vec![Reply::Raw(ok(b"not json"))]);
        assert_eq!(
            fetch_remote(&store_for(server.address.clone()), &server.address).unwrap_err(),
            FailureKind::Fetch(FetchError::InvalidResponse)
        );
    }

    #[test]
    fn a_daemon_that_reports_an_error_is_told_from_one_that_cannot_be_reached() {
        let error = ControlError::AccountNotFound {
            account_id: "gone".into(),
        };
        let mut document = serde_json::to_value(&error).unwrap();
        let map = document.as_object_mut().unwrap();
        map.insert("version".into(), json!(CONTROL_PROTOCOL_VERSION));
        map.insert("request_id".into(), json!("http-2"));
        let server = serve(vec![Reply::Raw(reply(
            "404 Not Found",
            &[],
            document.to_string().as_bytes(),
        ))]);
        let failure =
            fetch_remote(&store_for(server.address.clone()), &server.address).unwrap_err();
        let FailureKind::Fetch(FetchError::Daemon(result)) = failure else {
            panic!("{failure:?}");
        };
        assert_eq!(*result, ControlResult::Error(error));

        let gone = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let gone = RemoteAddress::parse(&listener.local_addr().unwrap().to_string());
            drop(listener);
            gone.unwrap()
        };
        assert_eq!(
            fetch_remote(&store_for(gone.clone()), &gone).unwrap_err(),
            FailureKind::Fetch(FetchError::DaemonUnavailable)
        );
    }

    #[test]
    fn a_fetch_that_is_ready_only_after_the_deadline_is_a_timeout_whatever_it_is() {
        // The client's own judgement (the first) finds the deadline in time;
        // only the last one, that of `fetch_remote_by`, is late. Without that
        // one the fetch would be returned as it is, so this fails without it.
        let late_at_the_end = || Deadline::late_at(Duration::from_secs(5), 2);
        let gone = FailureKind::Fetch(FetchError::DaemonUnavailable);
        for (reply, would_be) in [
            (ok(&body(ControlResult::Snapshots(Vec::new()))), "a fetch"),
            (reply("401 Unauthorized", &[], b"{}"), "a rejected token"),
            (ok(b"not json"), "an invalid answer"),
        ] {
            let server = serve(vec![Reply::Raw(reply)]);
            let result = fetch_remote_by(
                &store_for(server.address.clone()),
                &server.address,
                late_at_the_end(),
            );
            assert_eq!(result.unwrap_err(), gone, "{would_be}");
        }
    }

    #[test]
    fn a_credential_that_is_missing_or_unreadable_is_a_timeout_when_late() {
        // In time, these are "not paired": the test above has that. Here the
        // check is done only after the deadline.
        let server = serve(vec![Reply::Raw(ok(&body(ControlResult::Snapshots(
            Vec::new(),
        ))))]);
        let other = RemoteAddress::parse("10.0.0.1:7878").unwrap();
        let late = || Deadline::late_after(Duration::from_secs(5));
        for store in [MemoryStore::default(), store_for(other)] {
            assert_eq!(
                fetch_remote_by(&store, &server.address, late()).unwrap_err(),
                FailureKind::Fetch(FetchError::DaemonUnavailable)
            );
        }
        let unreadable = store_for(server.address.clone());
        unreadable.fail_load.set(true);
        assert_eq!(
            fetch_remote_by(&unreadable, &server.address, late()).unwrap_err(),
            FailureKind::Fetch(FetchError::DaemonUnavailable)
        );
        // Neither branch connected to the daemon.
        let (seen, accepted) = server.finish_after(Duration::from_millis(100));
        assert!(seen.is_empty());
        assert_eq!(accepted, 0);
    }

    #[test]
    fn a_daemon_of_another_protocol_version_is_a_mismatch() {
        let mismatch = json!({
            "version": CONTROL_PROTOCOL_VERSION,
            "request_id": "http-3",
            "error": "protocol_mismatch",
            "supported_version": CONTROL_PROTOCOL_VERSION - 1,
        });
        let server = serve(vec![Reply::Raw(reply(
            "400 Bad Request",
            &[],
            mismatch.to_string().as_bytes(),
        ))]);
        let failure =
            fetch_remote(&store_for(server.address.clone()), &server.address).unwrap_err();
        let FailureKind::Fetch(FetchError::ProtocolMismatch {
            supported_version, ..
        }) = failure
        else {
            panic!("{failure:?}");
        };
        assert_eq!(supported_version, CONTROL_PROTOCOL_VERSION - 1);
    }

    #[test]
    fn a_reply_over_the_limits_is_an_invalid_one_and_one_within_them_is_unchanged() {
        // One daemon, one address, one credential: the reply over the limits
        // does not change what the next fetch gets.
        let normal = normal_snapshots();
        let server = serve(vec![
            Reply::Raw(ok(&body(ControlResult::Snapshots(oversized_snapshots())))),
            Reply::Raw(ok(&body(ControlResult::Snapshots(normal.clone())))),
        ]);
        let store = store_for(server.address.clone());
        assert_eq!(
            fetch_remote(&store, &server.address).unwrap_err(),
            FailureKind::Fetch(FetchError::InvalidResponse)
        );
        let fetched = fetch_remote(&store, &server.address).unwrap();
        assert_eq!(fetched.snapshots, normal);
    }

    #[test]
    fn what_the_window_says_of_a_reply_over_the_limits_has_nothing_of_the_reply() {
        let server = serve(vec![Reply::Raw(ok(&body(ControlResult::Snapshots(
            oversized_snapshots(),
        ))))]);
        // Make sure that the reply carries what must not be shown.
        let sent =
            String::from_utf8(body(ControlResult::Snapshots(oversized_snapshots()))).unwrap();
        assert!(sent.contains(MARKER) && sent.contains("987654.25"));

        let kind = fetch_remote(&store_for(server.address.clone()), &server.address).unwrap_err();
        let failure = failure_for_kind(&kind, Some(&server.address));
        assert_eq!(
            failure,
            failure_for_kind(
                &FailureKind::Fetch(FetchError::InvalidResponse),
                Some(&server.address)
            )
        );
        for text in [&failure.title, &failure.body, failure.command] {
            assert!(!text.contains(MARKER) && !text.contains("987654"), "{text}");
        }
        let error = FetchError::InvalidResponse;
        for text in [error.to_string(), format!("{error:?}")] {
            assert!(!text.contains(MARKER), "{text}");
        }
    }
}
