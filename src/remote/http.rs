//! A small HTTP/1.1 client for the daemon's HTTP API.
//!
//! The API is plain HTTP on a trusted network, small JSON bodies, one
//! request per connection. That needs no more than this: the request is
//! written, the answer is read until its `Content-Length` or the end of the
//! connection, and a redirect is an answer like any other, never followed.
//! One deadline covers the connection, the request and the answer, so a
//! daemon that stops talking cannot hold the caller for longer than that.

use std::io::{self, Read, Write};
use std::net::{SocketAddrV4, TcpStream};
use std::time::{Duration, Instant};

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

/// The point in time by which an operation must be over, from its start to
/// its final result. One is made at the entry of a fetch or a pairing, and
/// passed down, so that the connection, the request, the answer, and the
/// checks after it all share the same limit.
#[derive(Clone, Copy, Debug)]
pub struct Deadline {
    end: Instant,
    /// For tests: the deadline passes at the n-th judgement, and not before.
    /// The network time is the real one, `limit`; what is made late is what
    /// comes after it. The count is shared by the copies of the deadline.
    #[cfg(test)]
    late_at_judgement: Option<(&'static std::sync::atomic::AtomicUsize, usize)>,
}

impl Deadline {
    pub fn after(limit: Duration) -> Self {
        Self {
            end: Instant::now() + limit,
            #[cfg(test)]
            late_at_judgement: None,
        }
    }

    /// A deadline that is over from the first judgement on: it stands for an
    /// answer that came in time and was processed too late.
    #[cfg(test)]
    pub fn late_after(limit: Duration) -> Self {
        Self::late_at(limit, 1)
    }

    /// A deadline that is over from the `n`-th judgement on. Earlier ones
    /// find it in time, so that a test can make only the last one late.
    #[cfg(test)]
    pub fn late_at(limit: Duration, n: usize) -> Self {
        let counter = Box::leak(Box::new(std::sync::atomic::AtomicUsize::new(0)));
        Self {
            end: Instant::now() + limit,
            late_at_judgement: Some((counter, n)),
        }
    }

    /// What is left, or a timeout when nothing is.
    pub fn remaining(&self) -> Result<Duration, HttpError> {
        let left = self.end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            Err(HttpError::TimedOut)
        } else {
            Ok(left)
        }
    }

    fn passed(&self) -> bool {
        #[cfg(test)]
        if let Some((count, n)) = self.late_at_judgement {
            let this = count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            return this >= n;
        }
        Instant::now() >= self.end
    }

    /// The final word on an operation: a result that comes after the
    /// deadline, a success or a failure, is `late` instead.
    pub fn judge<T, E>(&self, result: Result<T, E>, late: E) -> Result<T, E> {
        if self.passed() { Err(late) } else { result }
    }
}

/// What one attempt to connect may take: an even share of what is left, so
/// that an address that never answers cannot use up the time of the ones
/// after it. The last address may use all that is left.
pub fn attempt_budget(remaining: Duration, addresses_left: usize) -> Duration {
    match u32::try_from(addresses_left) {
        Ok(count) if count > 1 => remaining / count,
        _ => remaining,
    }
}

/// Connects to the first of `addresses` that answers.
fn connect(addresses: &[SocketAddrV4], deadline: &Deadline) -> Result<TcpStream, HttpError> {
    let mut last = HttpError::Unreachable;
    for (index, address) in addresses.iter().enumerate() {
        let budget = attempt_budget(deadline.remaining()?, addresses.len() - index);
        match TcpStream::connect_timeout(&(*address).into(), budget) {
            Ok(stream) => return Ok(stream),
            Err(error) => last = io_error(error),
        }
    }
    Err(last)
}

/// Sends `request` to the first of `addresses` that accepts a connection,
/// and reads the answer, within `deadline`. The `Host` header names the
/// address that was connected to, because the daemon accepts only the
/// addresses it listens on, and never the name that was resolved to them.
pub fn send(
    addresses: &[SocketAddrV4],
    request: &Request<'_>,
    deadline: &Deadline,
) -> Result<Response, HttpError> {
    // A request that cannot be sent is refused before any connection.
    build_head(*addresses.first().ok_or(HttpError::Unreachable)?, request)?;
    let mut stream = connect(addresses, deadline)?;
    let peer = match stream.peer_addr() {
        Ok(std::net::SocketAddr::V4(peer)) => peer,
        _ => return Err(HttpError::Unreachable),
    };
    let head = build_head(peer, request)?;
    write_all(&mut stream, &head, deadline)?;
    if let Some(body) = request.body {
        write_all(&mut stream, body, deadline)?;
    }
    let response = read_response(&mut stream, deadline)?;
    // An answer that completed at the deadline is too late.
    deadline.remaining()?;
    Ok(response)
}

/// Writes all of `bytes`. Each write is given what is left of the deadline.
fn write_all(stream: &mut TcpStream, bytes: &[u8], deadline: &Deadline) -> Result<(), HttpError> {
    let mut rest = bytes;
    while !rest.is_empty() {
        stream
            .set_write_timeout(Some(deadline.remaining()?))
            .map_err(io_error)?;
        let count = stream.write(rest).map_err(io_error)?;
        if count == 0 {
            return Err(HttpError::Unreachable);
        }
        rest = &rest[count..];
    }
    Ok(())
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

fn build_head(host: SocketAddrV4, request: &Request<'_>) -> Result<Vec<u8>, HttpError> {
    let path_ok = request.path.starts_with('/')
        && request
            .path
            .bytes()
            .all(|byte| (0x21..0x7f).contains(&byte));
    if !matches!(request.method, "GET" | "POST") || !path_ok {
        return Err(HttpError::Invalid);
    }
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nAccept: application/json\r\n",
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
    deadline: &Deadline,
) -> Result<usize, HttpError> {
    stream
        .set_read_timeout(Some(deadline.remaining()?))
        .map_err(io_error)?;
    stream.read(chunk).map_err(io_error)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn read_response(stream: &mut TcpStream, deadline: &Deadline) -> Result<Response, HttpError> {
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
        let response = send(
            &[server.address.literal().unwrap()],
            &get("/v1/usage"),
            &Deadline::after(WAIT),
        )
        .unwrap();
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
        send(
            &[server.address.literal().unwrap()],
            &request,
            &Deadline::after(WAIT),
        )
        .unwrap();
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
        let response = send(
            &[server.address.literal().unwrap()],
            &get("/x"),
            &Deadline::after(WAIT),
        )
        .unwrap();
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
        let response = send(
            &[server.address.literal().unwrap()],
            &get("/x"),
            &Deadline::after(WAIT),
        )
        .unwrap();
        assert_eq!(response.status, 302);
        assert_eq!(server.finish_after(Duration::from_millis(200)).0.len(), 1);
        // Nothing connected to where the redirect points, not even to
        // open a connection and close it.
        let (seen, accepted) = target.finish_after(Duration::from_millis(200));
        assert!(seen.is_empty());
        assert_eq!(accepted, 0);
    }

    #[test]
    fn a_chunked_answer_is_invalid() {
        let server = serve(vec![Reply::Raw(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n".to_vec(),
        )]);
        assert_eq!(
            send(
                &[server.address.literal().unwrap()],
                &get("/x"),
                &Deadline::after(WAIT)
            ),
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
                send(
                    &[server.address.literal().unwrap()],
                    &get("/x"),
                    &Deadline::after(WAIT)
                ),
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
            send(
                &[server.address.literal().unwrap()],
                &get("/x"),
                &Deadline::after(WAIT)
            ),
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
            send(
                &[server.address.literal().unwrap()],
                &get("/x"),
                &Deadline::after(WAIT)
            ),
            Err(HttpError::TooLarge)
        );
        let mut streamed = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
        streamed.extend(std::iter::repeat_n(b'a', MAX_BODY + 1));
        let server = serve(vec![Reply::Raw(streamed)]);
        assert_eq!(
            send(
                &[server.address.literal().unwrap()],
                &get("/x"),
                &Deadline::after(WAIT)
            ),
            Err(HttpError::TooLarge)
        );
    }

    #[test]
    fn a_refused_connection_is_unreachable() {
        let address = refused_address();
        assert_eq!(
            send(&[address], &get("/x"), &Deadline::after(WAIT)),
            Err(HttpError::Unreachable)
        );
    }

    /// A loopback address that nothing listens on.
    fn refused_address() -> SocketAddrV4 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let std::net::SocketAddr::V4(address) = listener.local_addr().unwrap() else {
            unreachable!("bound to an IPv4 address");
        };
        address
    }

    #[test]
    fn the_first_address_that_answers_is_used() {
        let server = serve(vec![Reply::Raw(ok(b"{}"))]);
        let live = server.address.literal().unwrap();
        let response = send(
            &[refused_address(), live, refused_address()],
            &get("/x"),
            &Deadline::after(WAIT),
        )
        .unwrap();
        assert_eq!(response.status, 200);
        // The `Host` header is the address that was connected to.
        let seen = server.finish();
        assert!(seen[0].head.contains(&format!("Host: {live}\r\n")));
    }

    #[test]
    fn all_addresses_refused_is_unreachable() {
        assert_eq!(
            send(
                &[refused_address(), refused_address()],
                &get("/x"),
                &Deadline::after(WAIT)
            ),
            Err(HttpError::Unreachable)
        );
    }

    #[test]
    fn no_address_is_unreachable() {
        assert_eq!(
            send(&[], &get("/x"), &Deadline::after(WAIT)),
            Err(HttpError::Unreachable)
        );
    }

    #[test]
    fn a_bad_request_is_refused_before_any_connection() {
        let server = serve(vec![Reply::Raw(ok(b"{}"))]);
        let request = Request {
            method: "GET",
            path: "no-slash",
            headers: &[],
            body: None,
        };
        assert_eq!(
            send(
                &[server.address.literal().unwrap()],
                &request,
                &Deadline::after(WAIT)
            ),
            Err(HttpError::Invalid)
        );
        let (seen, accepted) = server.finish_after(Duration::from_millis(100));
        assert!(seen.is_empty());
        assert_eq!(accepted, 0);
    }

    #[test]
    fn an_attempt_gets_an_even_share_of_what_is_left() {
        let second = Duration::from_secs(1);
        assert_eq!(attempt_budget(second * 30, 3), second * 10);
        assert_eq!(attempt_budget(second * 30, 2), second * 15);
        // The last address, or none, may use all that is left.
        assert_eq!(attempt_budget(second * 30, 1), second * 30);
        assert_eq!(attempt_budget(second * 30, 0), second * 30);
    }

    #[test]
    fn a_silent_daemon_times_out() {
        let server = serve(vec![Reply::Hang(Duration::from_secs(2))]);
        let started = Instant::now();
        assert_eq!(
            send(
                &[server.address.literal().unwrap()],
                &get("/x"),
                &Deadline::after(Duration::from_millis(300))
            ),
            Err(HttpError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_result_judged_after_the_deadline_is_the_late_one() {
        let on_time = Deadline::after(Duration::from_secs(60));
        assert_eq!(
            on_time.judge(Ok::<_, HttpError>(1), HttpError::TimedOut),
            Ok(1)
        );
        assert_eq!(
            on_time.judge(Err::<i32, _>(HttpError::Invalid), HttpError::TimedOut),
            Err(HttpError::Invalid)
        );
        // Over by the time the result is judged: a success and a failure
        // are both the late one.
        let late = Deadline::late_after(Duration::from_secs(60));
        assert_eq!(
            late.judge(Ok::<_, HttpError>(1), HttpError::TimedOut),
            Err(HttpError::TimedOut)
        );
        assert_eq!(
            late.judge(Err::<i32, _>(HttpError::Invalid), HttpError::TimedOut),
            Err(HttpError::TimedOut)
        );
        // A deadline that has passed leaves nothing to wait with.
        assert_eq!(
            Deadline::after(Duration::ZERO).remaining(),
            Err(HttpError::TimedOut)
        );
    }

    #[test]
    fn nothing_that_could_start_a_new_line_goes_into_a_request() {
        let address: SocketAddrV4 = "127.0.0.1:1".parse().unwrap();
        let bad = |method, path, headers: &[(&str, &str)]| {
            let request = Request {
                method,
                path,
                headers,
                body: None,
            };
            build_head(address, &request)
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
