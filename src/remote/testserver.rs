//! A daemon for tests: a listener on the loopback that answers each
//! connection with a prepared reply and keeps what it was sent.

use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
    stop: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
}

impl Server {
    /// Asks the listener to stop, without waiting for it.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Waits for the replies to be used up, and returns what was seen.
    pub fn finish(self) -> Vec<Seen> {
        self.handle.join().expect("the test server ended")
    }

    /// For a server that was given more replies than the client asks for, or
    /// that must see nothing: gives the client a moment, then stops the
    /// server. Returns what was seen, and how many connections were
    /// accepted, whatever they sent. A connection that sent nothing, or only
    /// a part of a request, is counted here and not seen.
    pub fn finish_after(self, wait: Duration) -> (Vec<Seen>, usize) {
        thread::sleep(wait);
        self.stop.store(true, Ordering::SeqCst);
        let seen = self.handle.join().expect("the test server ended");
        (seen, self.accepted.load(Ordering::SeqCst))
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
pub fn serve(replies: Vec<Reply>) -> Server {
    serve_with(replies, None)
}

/// What a test uses to fix the order of events in the listener thread: it
/// says when the thread has found no connection waiting, and then parks until
/// it is told to go on.
pub struct Park {
    pub reached: std::sync::mpsc::Sender<()>,
    pub go_on: std::sync::mpsc::Receiver<()>,
}

/// [`serve`], with a listener thread that parks after the first time it finds
/// no connection waiting.
pub fn serve_with(replies: Vec<Reply>, park: Option<Park>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    listener
        .set_nonblocking(true)
        .expect("a listener that polls");
    let address = RemoteAddress::parse(&listener.local_addr().unwrap().to_string()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let accepted = Arc::new(AtomicUsize::new(0));
    let (stopped, counted) = (stop.clone(), accepted.clone());
    let handle = thread::spawn(move || {
        let mut park = park;
        let mut seen = Vec::new();
        let mut replies = replies.into_iter();
        let mut next = replies.next();
        while next.is_some() {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if let Some(park) = park.take() {
                        // The state a test wants: no connection was waiting a
                        // moment ago. What happens now is up to the test.
                        let _ = park.reached.send(());
                        let _ = park.go_on.recv();
                    }
                    if !stopped.load(Ordering::SeqCst) {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    // The stop may have come after the accept above found
                    // nothing, with a client connecting in between: what is
                    // queued is counted before the server ends.
                    match listener.accept() {
                        Ok((stream, _)) => stream,
                        Err(_) => break,
                    }
                }
                Err(_) => break,
            };
            counted.fetch_add(1, Ordering::SeqCst);
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let (head, body) = read_request(&mut stream);
            if head.is_empty() {
                // A connection that sent no request: the reply stays.
                continue;
            }
            seen.push(Seen {
                head,
                body,
                address,
            });
            match next.take() {
                Some(Reply::Raw(bytes)) => {
                    let _ = stream.write_all(&bytes);
                }
                Some(Reply::Hang(wait)) => thread::sleep(wait),
                None => {}
            }
            next = replies.next();
        }
        seen
    });
    Server {
        address,
        handle,
        stop,
        accepted,
    }
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

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::TcpStream;

    use super::*;

    #[test]
    fn a_connection_is_counted_whatever_it_sends() {
        // The count must include a connection that sends nothing and one that
        // sends only part of a request headers: neither makes a `Seen`.
        let server = serve(vec![Reply::Raw(ok(b"{}"))]);
        drop(TcpStream::connect(server.address.socket_addr()).unwrap());
        let mut partial = TcpStream::connect(server.address.socket_addr()).unwrap();
        partial.write_all(b"GET /x HTT").unwrap();
        drop(partial);
        let (seen, accepted) = server.finish_after(Duration::from_millis(200));
        assert!(seen.is_empty());
        assert_eq!(accepted, 2);
    }

    #[test]
    fn a_connection_that_is_queued_when_the_server_is_stopped_is_still_counted() {
        // The order is fixed, not left to the scheduler: the listener finds
        // no connection, and parks. A client connects (the system queues it)
        // and the stop is asked for. Only then does the listener go on, with
        // a stop that came after its last look and a connection it never
        // saw. A listener that ends on the stop alone counts nothing here.
        let (reached, reached_rx) = std::sync::mpsc::channel();
        let (go_on_tx, go_on) = std::sync::mpsc::channel();
        let server = serve_with(vec![Reply::Raw(ok(b"{}"))], Some(Park { reached, go_on }));
        reached_rx.recv().unwrap();
        // Connected and closed, so that the server does not wait for a
        // request that never comes.
        drop(TcpStream::connect(server.address.socket_addr()).unwrap());
        server.request_stop();
        go_on_tx.send(()).unwrap();
        let (seen, accepted) = server.finish_after(Duration::ZERO);
        assert!(seen.is_empty());
        assert_eq!(accepted, 1);
    }

    #[test]
    fn a_server_nobody_connects_to_has_seen_and_accepted_nothing() {
        let server = serve(vec![Reply::Raw(ok(b"{}"))]);
        let (seen, accepted) = server.finish_after(Duration::from_millis(50));
        assert!(seen.is_empty());
        assert_eq!(accepted, 0);
    }
}
