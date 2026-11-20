//! One fetch from a remote daemon: the saved credential, the client, and
//! `fetch_usage` of `ullage-cli`.

use ullage_cli::usage_view::{FetchError, UsageFetch, fetch_usage};

use super::address::RemoteAddress;
use super::client::{FETCH_TIMEOUT, HttpClient};
use super::http::Deadline;
use super::store::{TokenStore, credential_for};
use crate::state::FailureKind;

/// Fetches the usage from `address`, with the token saved for that address.
/// A token saved for another address, or none, or one that cannot be read,
/// is "not paired" when the check is done in time, and nothing is sent. When
/// the check is done only after the deadline, the result is the timeout
/// instead, again with nothing sent, so that a slow credential store does not
/// clear what the window shows.
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
    let result = match credential_for(store, address) {
        // The answer to "not paired" is judged too: the credential store may
        // have been slow, and a late answer is a timeout, which keeps what the
        // window shows.
        None => Err(FailureKind::NotPaired),
        Some(credential) => {
            let client = HttpClient::new(*address, credential.token, deadline);
            fetch_usage(&client).map_err(|error| {
                // `ClientError` has no word for a refused token: the client
                // noted it.
                if error == FetchError::InvalidResponse && client.was_rejected() {
                    FailureKind::TokenRejected
                } else {
                    FailureKind::Fetch(error)
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
    use ullage_protocol::{CONTROL_PROTOCOL_VERSION, ControlError, ControlResult};

    use super::super::store::{Credential, Token, memory::MemoryStore};
    use super::super::testserver::{Reply, ok, reply, serve};
    use super::*;

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

    #[test]
    fn a_working_daemon_gives_its_snapshots() {
        let server = serve(vec![Reply::Raw(ok(&body(ControlResult::Snapshots(
            Vec::new(),
        ))))]);
        let fetched = fetch_remote(&store_for(server.address), &server.address).unwrap();
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
            fetch_remote(&store_for(server.address), &server.address).unwrap_err(),
            FailureKind::TokenRejected
        );
        let server = serve(vec![Reply::Raw(ok(b"not json"))]);
        assert_eq!(
            fetch_remote(&store_for(server.address), &server.address).unwrap_err(),
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
        let failure = fetch_remote(&store_for(server.address), &server.address).unwrap_err();
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
            fetch_remote(&store_for(gone), &gone).unwrap_err(),
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
                &store_for(server.address),
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
        let unreadable = store_for(server.address);
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
        let failure = fetch_remote(&store_for(server.address), &server.address).unwrap_err();
        let FailureKind::Fetch(FetchError::ProtocolMismatch {
            supported_version, ..
        }) = failure
        else {
            panic!("{failure:?}");
        };
        assert_eq!(supported_version, CONTROL_PROTOCOL_VERSION - 1);
    }
}
