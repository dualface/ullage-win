//! The daemon's HTTP API as a `ControlClient`.
//!
//! `fetch_usage` of `ullage-cli` takes any `ControlClient`, so the window
//! gets its cards, rows and checks from that one place whichever way the
//! daemon is reached. This client only turns the one request it needs, `Show`
//! of every account, into `GET /v1/usage`, and the answer back into the
//! `ControlResponse` the control socket would have given.

use std::cell::Cell;
use std::time::Duration;

use serde::Deserialize;
use ullage_cli::{ClientError, ControlClient};
use ullage_protocol::{
    ControlCommand, ControlError, ControlRequest, ControlResponse, ControlResult,
};

use super::address::RemoteAddress;
use super::http::{self, HttpError, Request};
use super::store::Token;

/// The longest one fetch may take, from connecting to the last byte.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

pub struct HttpClient {
    address: RemoteAddress,
    token: Token,
    rejected: Cell<bool>,
}

impl HttpClient {
    pub fn new(address: RemoteAddress, token: Token) -> Self {
        Self {
            address,
            token,
            rejected: Cell::new(false),
        }
    }

    /// Whether the daemon answered `401`: the token is not, or no longer, one
    /// it knows. `ClientError` has no word for it, so the caller asks here
    /// after a failed fetch.
    pub fn was_rejected(&self) -> bool {
        self.rejected.get()
    }
}

impl ControlClient for HttpClient {
    fn send(&self, request: &ControlRequest) -> Result<ControlResponse, ClientError> {
        // The only request the HTTP API answers the way the window needs it.
        if !matches!(request.command, ControlCommand::Show { account_id: None }) {
            return Err(ClientError::InvalidResponse);
        }
        let authorization = format!("Bearer {}", self.token.expose());
        let answer = http::send(
            &self.address,
            &Request {
                method: "GET",
                path: "/v1/usage",
                headers: &[("Authorization", &authorization)],
                body: None,
            },
            FETCH_TIMEOUT,
        )
        .map_err(|error| match error {
            HttpError::Unreachable | HttpError::TimedOut => ClientError::DaemonUnavailable,
            HttpError::Invalid | HttpError::TooLarge => ClientError::InvalidResponse,
        })?;
        match map_response(answer.status, &answer.body, &request.request_id) {
            Mapped::Response(response) => Ok(*response),
            Mapped::Rejected => {
                self.rejected.set(true);
                Err(ClientError::InvalidResponse)
            }
            Mapped::Invalid => Err(ClientError::InvalidResponse),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Mapped {
    Response(Box<ControlResponse>),
    /// `401`.
    Rejected,
    Invalid,
}

/// The body of a good answer: the `ControlResult`, spread into the document
/// next to `version` and `request_id`. It is not a `ControlResponse`, whose
/// `result` field holds the result as an object of its own.
#[derive(Deserialize)]
struct ResultDocument {
    version: u16,
    #[serde(flatten)]
    result: ControlResult,
}

/// The body of an error the daemon reports: its `ControlError`, spread into
/// the document next to `version` and `request_id`.
#[derive(Deserialize)]
struct ErrorDocument {
    version: u16,
    #[serde(flatten)]
    error: ControlError,
}

/// What a protocol mismatch looks like over HTTP.
#[derive(Deserialize)]
struct MismatchDocument {
    version: u16,
    error: String,
    supported_version: u16,
}

/// Turns an HTTP answer into the response of the control protocol.
///
/// The daemon makes up the `request_id` of an HTTP answer itself (`http-N`)
/// and does not repeat the client's, but `fetch_usage` checks that the
/// answer carries the id of its request. So the answer takes the request's
/// id here, which is safe: one connection carries one request, and what is
/// read is the answer to it.
pub fn map_response(status: u16, body: &[u8], request_id: &str) -> Mapped {
    let response = |version, result| {
        Mapped::Response(Box::new(ControlResponse {
            version,
            request_id: request_id.to_owned(),
            result,
            diagnostic: None,
            daemon_version: None,
        }))
    };
    match status {
        200 => match serde_json::from_slice::<ResultDocument>(body) {
            Ok(document) => response(document.version, document.result),
            Err(_) => Mapped::Invalid,
        },
        401 => Mapped::Rejected,
        400 => {
            if let Ok(document) = serde_json::from_slice::<MismatchDocument>(body)
                && document.error == "protocol_mismatch"
            {
                return response(
                    document.version,
                    ControlResult::ProtocolMismatch {
                        supported_version: document.supported_version,
                    },
                );
            }
            error_response(body, response)
        }
        _ => error_response(body, response),
    }
}

fn error_response(body: &[u8], response: impl Fn(u16, ControlResult) -> Mapped) -> Mapped {
    match serde_json::from_slice::<ErrorDocument>(body) {
        Ok(document) => response(document.version, ControlResult::Error(document.error)),
        Err(_) => Mapped::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use ullage_protocol::CONTROL_PROTOCOL_VERSION;

    use super::super::testserver::{Reply, ok, reply, serve};
    use super::*;

    const TOKEN: &str = "tok-123_ABC";

    fn show_all() -> ControlRequest {
        ControlRequest::new("req-1", ControlCommand::Show { account_id: None })
    }

    fn snapshots_body(request_id: &str) -> Vec<u8> {
        let mut document = serde_json::to_value(ControlResult::Snapshots(Vec::new())).unwrap();
        // What `ullage-http` adds to the serialized result.
        let map = document.as_object_mut().unwrap();
        map.insert("version".into(), json!(CONTROL_PROTOCOL_VERSION));
        map.insert("request_id".into(), json!(request_id));
        serde_json::to_vec(&document).unwrap()
    }

    fn error_body(error: ControlError) -> Vec<u8> {
        let mut document = serde_json::to_value(error).unwrap();
        let map = document.as_object_mut().unwrap();
        map.insert("version".into(), json!(CONTROL_PROTOCOL_VERSION));
        map.insert("request_id".into(), json!("http-9"));
        serde_json::to_vec(&document).unwrap()
    }

    #[test]
    fn the_answer_of_the_http_api_becomes_a_control_response() {
        // The shape was read from `ullage-http`: the serialized result, plus
        // `version` and `request_id`. It is not a `ControlResponse` itself.
        assert!(serde_json::from_slice::<ControlResponse>(&snapshots_body("http-7")).is_err());
        let body = snapshots_body("http-7");
        let text = String::from_utf8(body.clone()).unwrap();
        assert!(text.contains("\"result\":\"snapshots\""), "{text}");
        assert!(text.contains("\"payload\":[]"), "{text}");
        let Mapped::Response(response) = map_response(200, &body, "req-1") else {
            panic!("{text}");
        };
        assert_eq!(response.version, CONTROL_PROTOCOL_VERSION);
        assert_eq!(response.result, ControlResult::Snapshots(Vec::new()));
    }

    #[test]
    fn the_answer_carries_the_id_of_the_request_whatever_the_body_says() {
        for body_id in ["http-7", "req-1", "", "something else"] {
            let Mapped::Response(response) = map_response(200, &snapshots_body(body_id), "req-1")
            else {
                panic!("{body_id}");
            };
            assert_eq!(response.request_id, "req-1", "{body_id:?}");
        }
    }

    #[test]
    fn a_protocol_mismatch_keeps_the_version_the_daemon_supports() {
        let body = json!({
            "version": 9,
            "request_id": "http-1",
            "error": "protocol_mismatch",
            "supported_version": 9,
        })
        .to_string();
        let Mapped::Response(response) = map_response(400, body.as_bytes(), "req-1") else {
            panic!()
        };
        assert_eq!(response.version, 9);
        assert_eq!(response.request_id, "req-1");
        assert_eq!(
            response.result,
            ControlResult::ProtocolMismatch {
                supported_version: 9
            }
        );
    }

    #[test]
    fn an_error_document_becomes_the_error_of_the_control_protocol() {
        let error = ControlError::AccountNotFound {
            account_id: "gone".into(),
        };
        for status in [400, 404, 409, 429, 500, 504] {
            let Mapped::Response(response) =
                map_response(status, &error_body(error.clone()), "req-1")
            else {
                panic!("{status}");
            };
            assert_eq!(
                response.result,
                ControlResult::Error(error.clone()),
                "{status}"
            );
            assert_eq!(response.request_id, "req-1");
            assert_eq!(response.diagnostic, None);
        }
    }

    #[test]
    fn a_rejected_token_is_not_an_invalid_answer() {
        assert_eq!(
            map_response(401, b"{\"error\":\"unauthorized\"}", "r"),
            Mapped::Rejected
        );
        assert_eq!(map_response(401, b"", "r"), Mapped::Rejected);
    }

    #[test]
    fn anything_else_is_invalid() {
        for (status, body) in [
            (200, &b"not json"[..]),
            (200, b"{}"),
            (
                200,
                b"{\"result\":\"nonsense\",\"version\":1,\"request_id\":\"a\"}",
            ),
            (302, b""),
            (403, b"{\"error\":\"forbidden\"}"),
            (404, b"{\"error\":\"not_found\"}"),
            (500, b""),
            (400, b"{\"error\":\"bad_request\"}"),
            (400, b"{\"version\":1,\"error\":\"protocol_mismatch\"}"),
        ] {
            assert_eq!(map_response(status, body, "r"), Mapped::Invalid, "{status}");
        }
    }

    fn client(server: &super::super::testserver::Server) -> HttpClient {
        HttpClient::new(server.address, Token::parse(TOKEN).unwrap())
    }

    #[test]
    fn a_show_of_every_account_is_a_get_of_the_usage_with_the_token() {
        let server = serve(vec![Reply::Raw(ok(&snapshots_body("http-3")))]);
        let response = client(&server).send(&show_all()).unwrap();
        assert_eq!(response.request_id, "req-1");
        let seen = server.finish();
        let head = &seen[0].head;
        assert!(head.starts_with("GET /v1/usage HTTP/1.1\r\n"), "{head}");
        assert!(head.contains(&format!("Authorization: Bearer {TOKEN}\r\n")));
        assert!(head.contains(&format!("Host: {}\r\n", seen[0].address)));
    }

    #[test]
    fn no_other_request_is_sent() {
        let server = serve(vec![Reply::Raw(ok(&snapshots_body("http-1")))]);
        let client = client(&server);
        for command in [
            ControlCommand::Show {
                account_id: Some("a".into()),
            },
            ControlCommand::DaemonStatus,
            ControlCommand::ListAccounts,
        ] {
            let request = ControlRequest::new("req-2", command);
            assert!(matches!(
                client.send(&request),
                Err(ClientError::InvalidResponse)
            ));
        }
        // The server never saw a connection: finish it by one more send.
        assert!(client.send(&show_all()).is_ok());
        assert_eq!(server.finish().len(), 1);
    }

    #[test]
    fn a_401_is_remembered_for_the_caller() {
        let server = serve(vec![Reply::Raw(reply(
            "401 Unauthorized",
            &[("WWW-Authenticate", "Bearer")],
            b"{\"error\":\"unauthorized\"}",
        ))]);
        let client = client(&server);
        assert!(!client.was_rejected());
        assert!(matches!(
            client.send(&show_all()),
            Err(ClientError::InvalidResponse)
        ));
        assert!(client.was_rejected());
    }

    #[test]
    fn a_daemon_that_does_not_answer_is_unavailable() {
        let server = serve(vec![Reply::Raw(Vec::new())]);
        let client = client(&server);
        // An empty answer is not HTTP.
        assert!(matches!(
            client.send(&show_all()),
            Err(ClientError::InvalidResponse)
        ));
        let address = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = RemoteAddress::parse(&listener.local_addr().unwrap().to_string());
            drop(listener);
            address.unwrap()
        };
        let refused = HttpClient::new(address, Token::parse(TOKEN).unwrap());
        assert!(matches!(
            refused.send(&show_all()),
            Err(ClientError::DaemonUnavailable)
        ));
    }
}
