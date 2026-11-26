//! One loopback HTTP listener for every pilot fake, so a process holds a single port and a
//! single start clock. Requests are routed by path prefix to the fake that owns it.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::demos_fake;

/// Bounds the request head, so a client that never ends it cannot grow the buffer.
const HEAD_LIMIT: usize = 8 * 1024;

/// Unix seconds, fixed on first call; every fake anchors its row times here.
pub fn start_time() -> i64 {
    static START: OnceLock<i64> = OnceLock::new();
    *START.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    })
}

/// `http://127.0.0.1:<port>` of the one shared listener, bound and served on first call.
pub fn root() -> std::io::Result<String> {
    static ROOT: OnceLock<std::io::Result<String>> = OnceLock::new();
    match ROOT.get_or_init(listen) {
        Ok(root) => Ok(root.clone()),
        Err(e) => Err(std::io::Error::new(e.kind(), e.to_string())),
    }
}

// The clock is forced before binding, so no fake's rows can postdate the listener.
fn listen() -> std::io::Result<String> {
    start_time();
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let root = format!("http://{}", listener.local_addr()?);
    let served = root.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // One connection at a time; a failed one is dropped and never stops the loop.
            let _ = serve(stream, &served);
        }
    });
    Ok(root)
}

// Reads the head up to its blank line (it may span several reads) under a read timeout, so an
// idle client cannot hold the only thread, then answers and closes.
fn serve(mut stream: TcpStream, root: &str) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut chunk)?;
        if n == 0 || head.len() + n > HEAD_LIMIT {
            return Ok(());
        }
        head.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&head);
    let target = head
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("GET "))
        .and_then(|rest| rest.split(' ').next())
        .unwrap_or("");
    let (status, kind, body) = route(target, root);
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.flush()
}

// Each fake owns one path prefix and sees only the path after it.
fn route(target: &str, root: &str) -> (&'static str, &'static str, Vec<u8>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if let Some(rest) = path.strip_prefix("/demostf") {
        return demos_fake::answer(rest, query, &format!("{root}/demostf"));
    }
    ("404 Not Found", "text/plain", b"not found".to_vec())
}
