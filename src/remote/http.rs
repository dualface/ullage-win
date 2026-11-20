//! A small HTTP/1.1 client for the daemon's HTTP API.
//!
//! The API is plain HTTP on a trusted network, small JSON bodies, one
//! request per connection. That needs no more than this: the request is
//! written, the answer is read until its `Content-Length` or the end of the
//! connection, and a redirect is an answer like any other, never followed.
//! One deadline covers the connection, the request and the answer, so a
//! daemon that stops talking cannot hold the caller for longer than that.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use super::address::RemoteAddress;

/// The most a body may hold. The daemon's own answers are far smaller.
pub const MAX_BODY: usize = 4 * 1024 * 1024;
/// The most the status line and headers may hold.
const MAX_HEAD: usize = 16 * 1024;

pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
    pub body: Option<&'a [u8]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpError {
    /// The connection was refused, or broke.
    Unreachable,
    /// The deadline passed.
    TimedOut,
    /// The request could not be written, or the answer is not HTTP/1.x in
    /// the shape this client reads.
    Invalid,
    /// The body is larger than [`MAX_BODY`].
    TooLarge,
}

/// Sends `request` to `address`, and reads the answer, within `timeout`.
pub fn send(
    address: &RemoteAddress,
    request: &Request<'_>,
    timeout: Duration,
) -> Result<Response, HttpError> {
    let head = build_head(address, request)?;
    let deadline = Instant::now() + timeout;
    let mut stream = TcpStream::connect_timeout(&address.socket_addr(), remaining(deadline)?)
        .map_err(io_error)?;
    write_all(&mut stream, &head, deadline)?;
    if let Some(body) = request.body {
        write_all(&mut stream, body, deadline)?;
    }
    let response = read_response(&mut stream, deadline)?;
    // An answer that completed at the deadline is too late.
    remaining(deadline)?;
    Ok(response)
}

/// Writes all of `bytes`. Each write is given what is left of the deadline.
fn write_all(stream: &mut TcpStream, bytes: &[u8], deadline: Instant) -> Result<(), HttpError> {
    let mut rest = bytes;
    while !rest.is_empty() {
        stream
            .set_write_timeout(Some(remaining(deadline)?))
            .map_err(io_error)?;
        let count = stream.write(rest).map_err(io_error)?;
        if count == 0 {
            return Err(HttpError::Unreachable);
        }
        rest = &rest[count..];
    }
    Ok(())
}

fn remaining(deadline: Instant) -> Result<Duration, HttpError> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(HttpError::TimedOut)
    } else {
        Ok(left)
    }
}

fn io_error(error: io::Error) -> HttpError {
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => HttpError::TimedOut,
        _ => HttpError::Unreachable,
    }
}

/// Text that may go into a header or the request line: visible ASCII only,
/// so no value can start a new line.
fn is_plain(text: &str) -> bool {
    text.bytes().all(|byte| (0x20..0x7f).contains(&byte))
}

fn build_head(address: &RemoteAddress, request: &Request<'_>) -> Result<Vec<u8>, HttpError> {
    let path_ok = request.path.starts_with('/')
        && request
            .path
            .bytes()
            .all(|byte| (0x21..0x7f).contains(&byte));
    if !matches!(request.method, "GET" | "POST") || !path_ok {
        return Err(HttpError::Invalid);
    }
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAccept: application/json\r\n",
        request.method, request.path
    );
    for (name, value) in request.headers {
        let name_ok = !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        if !name_ok || !is_plain(value) {
            return Err(HttpError::Invalid);
        }
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = request.body {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    Ok(head.into_bytes())
}

fn read_some(
    stream: &mut TcpStream,
    chunk: &mut [u8],
    deadline: Instant,
) -> Result<usize, HttpError> {
    stream
        .set_read_timeout(Some(remaining(deadline)?))
        .map_err(io_error)?;
    stream.read(chunk).map_err(io_error)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn read_response(stream: &mut TcpStream, deadline: Instant) -> Result<Response, HttpError> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(end) = find(&buffer, b"\r\n\r\n") {
            break end;
        }
        if buffer.len() > MAX_HEAD {
            return Err(HttpError::Invalid);
        }
        let count = read_some(stream, &mut chunk, deadline)?;
        if count == 0 {
            return Err(HttpError::Invalid);
        }
        buffer.extend_from_slice(&chunk[..count]);
    };
    let (status, length) = parse_head(&buffer[..head_end])?;
    let mut body = buffer[head_end + 4..].to_vec();
    match length {
        Some(length) => {
            if length > MAX_BODY {
                return Err(HttpError::TooLarge);
            }
            while body.len() < length {
                let count = read_some(stream, &mut chunk, deadline)?;
                if count == 0 {
                    return Err(HttpError::Invalid);
                }
                body.extend_from_slice(&chunk[..count]);
            }
            body.truncate(length);
        }
        None => loop {
            if body.len() > MAX_BODY {
                return Err(HttpError::TooLarge);
            }
            let count = read_some(stream, &mut chunk, deadline)?;
            if count == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..count]);
        },
    }
    Ok(Response { status, body })
}

/// The status code and the `Content-Length`, if there is one.
fn parse_head(head: &[u8]) -> Result<(u16, Option<usize>), HttpError> {
    let head = std::str::from_utf8(head).map_err(|_| HttpError::Invalid)?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or(HttpError::Invalid)?;
    let mut parts = status_line.split(' ');
    if !parts
        .next()
        .is_some_and(|version| version.starts_with("HTTP/1."))
    {
        return Err(HttpError::Invalid);
    }
    let status: u16 = parts
        .next()
        .and_then(|code| code.parse().ok())
        .ok_or(HttpError::Invalid)?;
    // Answers before the final one are not read.
    if !(200..=599).contains(&status) {
        return Err(HttpError::Invalid);
    }
    let mut length = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(HttpError::Invalid)?;
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "content-length" => {
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(HttpError::Invalid);
                }
                let parsed: usize = value.parse().map_err(|_| HttpError::TooLarge)?;
                if length.is_some_and(|earlier| earlier != parsed) {
                    return Err(HttpError::Invalid);
                }
                length = Some(parsed);
            }
            // Chunked bodies are not read.
            "transfer-encoding" if !value.eq_ignore_ascii_case("identity") => {
                return Err(HttpError::Invalid);
            }
            _ => {}
        }
    }
    Ok((status, length))
}

#[cfg(test)]
mod tests {
    use super::super::testserver::{Reply, ok, reply, serve};
    use super::*;

    const WAIT: Duration = Duration::from_secs(5);

    fn get(path: &str) -> Request<'_> {
        Request {
            method: "GET",
            path,
            headers: &[("Authorization", "Bearer abc")],
            body: None,
        }
    }

    #[test]
    fn a_get_sends_the_host_and_the_headers_it_was_given() {
        let server = serve(vec![Reply::Raw(ok(b"{}"))]);
        let response = send(&server.address, &get("/v1/usage"), WAIT).unwrap();
        assert_eq!(
            response,
            Response {
                status: 200,
                body: b"{}".to_vec()
            }
        );
        let seen = server.finish();
        assert_eq!(seen.len(), 1);
        let head = &seen[0].head;
        assert!(head.starts_with("GET /v1/usage HTTP/1.1\r\n"), "{head}");
        assert!(head.contains(&format!("Host: {}\r\n", seen[0].address)));
        assert!(head.contains("Authorization: Bearer abc\r\n"));
        assert!(head.contains("Connection: close\r\n"));
    }

    #[test]
    fn a_post_sends_its_body_with_the_length() {
        let server = serve(vec![Reply::Raw(ok(b"{}"))]);
        let request = Request {
            method: "POST",
            path: "/v1/pair",
            headers: &[("Content-Type", "application/json")],
            body: Some(b"{\"a\":1}"),
        };
        send(&server.address, &request, WAIT).unwrap();
        let seen = server.finish();
        assert!(seen[0].head.contains("Content-Length: 7\r\n"));
        assert!(seen[0].head.contains("Content-Type: application/json\r\n"));
        assert_eq!(seen[0].body, b"{\"a\":1}");
    }

    #[test]
    fn a_body_without_a_length_is_read_to_the_end_of_the_connection() {
        let server = serve(vec![Reply::Raw(
            b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nhello".to_vec(),
        )]);
        let response = send(&server.address, &get("/x"), WAIT).unwrap();
        assert_eq!(response.body, b"hello");
    }

    #[test]
    fn a_redirect_is_an_answer_and_is_not_followed() {
        // The redirect points at a server of its own, which is watched.
        let target = serve(vec![Reply::Raw(ok(b"must not be asked for"))]);
        let location = format!("http://{}/", target.address);
        let server = serve(vec![
            Reply::Raw(reply("302 Found", &[("Location", &location)], b"")),
            Reply::Raw(ok(b"must not be asked for either")),
        ]);
        let response = send(&server.address, &get("/x"), WAIT).unwrap();
        assert_eq!(response.status, 302);
        assert_eq!(server.finish_after(Duration::from_millis(200)).len(), 1);
        assert!(target.finish_after(Duration::from_millis(200)).is_empty());
    }

    #[test]
    fn a_chunked_answer_is_invalid() {
        let server = serve(vec![Reply::Raw(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n".to_vec(),
        )]);
        assert_eq!(
            send(&server.address, &get("/x"), WAIT),
            Err(HttpError::Invalid)
        );
    }

    #[test]
    fn answers_that_are_not_http_are_invalid() {
        for raw in [
            &b"garbage\r\n\r\n"[..],
            b"HTTP/2 200 OK\r\n\r\n",
            b"HTTP/1.1 abc OK\r\n\r\n",
            b"HTTP/1.1 100 Continue\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Length: x\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\nabc",
            b"HTTP/1.1 200 OK\r\nbroken header\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nshort",
            b"",
        ] {
            let server = serve(vec![Reply::Raw(raw.to_vec())]);
            assert_eq!(
                send(&server.address, &get("/x"), WAIT),
                Err(HttpError::Invalid),
                "{:?}",
                String::from_utf8_lossy(raw)
            );
        }
    }

    #[test]
    fn a_head_that_never_ends_is_invalid() {
        let mut raw = b"HTTP/1.1 200 OK\r\nX-Long: ".to_vec();
        raw.extend(std::iter::repeat_n(b'a', MAX_HEAD + 100));
        let server = serve(vec![Reply::Raw(raw)]);
        assert_eq!(
            send(&server.address, &get("/x"), WAIT),
            Err(HttpError::Invalid)
        );
    }

    #[test]
    fn a_body_over_the_limit_is_too_large() {
        let declared = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        let server = serve(vec![Reply::Raw(declared.into_bytes())]);
        assert_eq!(
            send(&server.address, &get("/x"), WAIT),
            Err(HttpError::TooLarge)
        );
        let mut streamed = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
        streamed.extend(std::iter::repeat_n(b'a', MAX_BODY + 1));
        let server = serve(vec![Reply::Raw(streamed)]);
        assert_eq!(
            send(&server.address, &get("/x"), WAIT),
            Err(HttpError::TooLarge)
        );
    }

    #[test]
    fn a_refused_connection_is_unreachable() {
        let address = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = RemoteAddress::parse(&listener.local_addr().unwrap().to_string());
            drop(listener);
            address.unwrap()
        };
        assert_eq!(
            send(&address, &get("/x"), WAIT),
            Err(HttpError::Unreachable)
        );
    }

    #[test]
    fn a_silent_daemon_times_out() {
        let server = serve(vec![Reply::Hang(Duration::from_secs(2))]);
        let started = Instant::now();
        assert_eq!(
            send(&server.address, &get("/x"), Duration::from_millis(300)),
            Err(HttpError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn nothing_that_could_start_a_new_line_goes_into_a_request() {
        let address = RemoteAddress::parse("127.0.0.1:1").unwrap();
        let bad = |method, path, headers: &[(&str, &str)]| {
            let request = Request {
                method,
                path,
                headers,
                body: None,
            };
            build_head(&address, &request)
        };
        assert!(bad("GET", "/ok", &[]).is_ok());
        assert_eq!(bad("PUT", "/ok", &[]), Err(HttpError::Invalid));
        assert_eq!(bad("GET", "no-slash", &[]), Err(HttpError::Invalid));
        assert_eq!(bad("GET", "/a b", &[]), Err(HttpError::Invalid));
        assert_eq!(bad("GET", "/a\r\nX: y", &[]), Err(HttpError::Invalid));
        assert_eq!(
            bad("GET", "/ok", &[("X", "a\r\nInjected: 1")]),
            Err(HttpError::Invalid)
        );
        assert_eq!(bad("GET", "/ok", &[("X Y", "a")]), Err(HttpError::Invalid));
        assert_eq!(bad("GET", "/ok", &[("", "a")]), Err(HttpError::Invalid));
    }
}
