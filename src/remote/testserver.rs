//! A daemon for tests: a listener on the loopback that answers each
//! connection with a prepared reply and keeps what it was sent.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::address::RemoteAddress;

pub enum Reply {
    /// Sent as it is, then the connection closes.
    Raw(Vec<u8>),
    /// The connection stays open and silent for this long.
    Hang(Duration),
}

/// What one connection sent.
pub struct Seen {
    /// The request line and the headers, each ending in CR LF, without the
    /// blank line.
    pub head: String,
    pub body: Vec<u8>,
    /// The address the server listens on, for the `Host` check.
    pub address: RemoteAddress,
}

pub struct Server {
    pub address: RemoteAddress,
    handle: JoinHandle<Vec<Seen>>,
}

impl Server {
    /// Waits for the replies to be used up, and returns what was seen.
    pub fn finish(self) -> Vec<Seen> {
        self.handle.join().expect("the test server ended")
    }

    /// Like [`Server::finish`], for a server that was given more replies than
    /// the client asks for: gives it a moment, then stops waiting.
    pub fn finish_after(self, wait: Duration) -> Vec<Seen> {
        thread::sleep(wait);
        // Connect once per reply left, so that the listener's loop ends.
        for _ in 0..4 {
            let _ = std::net::TcpStream::connect(self.address.socket_addr());
        }
        self.finish()
    }
}

/// `200 OK` with `body`.
pub fn ok(body: &[u8]) -> Vec<u8> {
    reply("200 OK", &[("Content-Type", "application/json")], body)
}

pub fn reply(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut text = format!("HTTP/1.1 {status}\r\n");
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    let mut bytes = text.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

/// Listens on a free loopback port and answers one connection per reply.
/// With no replies the listener is closed again when the server is dropped,
/// which leaves a port nobody listens on.
pub fn serve(replies: Vec<Reply>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let address = RemoteAddress::parse(&listener.local_addr().unwrap().to_string()).unwrap();
    let handle = thread::spawn(move || {
        let mut seen = Vec::new();
        for reply in replies {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let (head, body) = read_request(&mut stream);
            if head.is_empty() {
                // A connection that sent nothing: it only woke the listener.
                break;
            }
            seen.push(Seen {
                head,
                body,
                address,
            });
            match reply {
                Reply::Raw(bytes) => {
                    let _ = stream.write_all(&bytes);
                }
                Reply::Hang(wait) => thread::sleep(wait),
            }
        }
        seen
    });
    Server { address, handle }
}

fn read_request(stream: &mut std::net::TcpStream) -> (String, Vec<u8>) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break end;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return (String::new(), Vec::new()),
            Ok(count) => buffer.extend_from_slice(&chunk[..count]),
        }
    };
    let head = String::from_utf8_lossy(&buffer[..end + 2]).into_owned();
    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);
    let mut body = buffer[end + 4..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => body.extend_from_slice(&chunk[..count]),
        }
    }
    (head, body)
}
