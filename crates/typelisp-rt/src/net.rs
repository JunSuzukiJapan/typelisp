//! TCP sockets in the stream table: everything a socket can do **right
//! now**, and nothing it would have to wait for.
//!
//! The rule every operation here follows: try once, non-blocking, and if the
//! socket is not ready say so *as an answer* — `None` from [`StreamTable::
//! net_fill`], `None` from [`StreamTable::net_accept`], `false` from
//! [`StreamTable::net_flush`]. The waiting is done in typelisp: the prelude's
//! `tcp-stream` methods loop, and when the answer is "not yet" they call
//! `(net-wait h interest)`, which parks the **task** (the scheduler's
//! `Waiting::Io`) until `poll` says the descriptor is ready. Nothing in this
//! module can stop the thread, which is what keeps every other task running
//! while one is waiting on the network.
//!
//! Why the buffers live here and not in typelisp: a `read` from the OS hands
//! back as many bytes as it likes, and a UTF-8 character can end in the next
//! one. Decoding a character out of a byte queue is the same code
//! `stream.rs`'s `read_one_char` runs over a `BufRead`, written once more
//! over a `VecDeque` because the queue can come up *short* — which the
//! blocking version never has to say.
//!
//! One socket, two views: the prelude's `tcp-stream` (characters) and
//! `tcp-byte-stream` (bytes) are two structs over the **same handle**, so the
//! byte and character operations here share one `rbuf`. Mixing them is
//! legal — an HTTP client reads headers as text and the body as bytes — with
//! the one rule `read_byte` already has: a pushed-back character and a byte
//! read disagree about where the stream is, so the byte read is refused
//! while pushback is pending.

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};

use crate::stream::{Backend, Handle, StreamObj, StreamResult, StreamTable};

/// How much one `net_fill` asks the OS for. Big enough that a line of text
/// arrives in one read; small enough that a slow reader does not pin
/// megabytes per connection.
const FILL_CHUNK: usize = 16 * 1024;

/// The parts of a connected socket's entry an operation works on: the
/// socket, its read and write buffers, and the two per-stream facts every
/// backend keeps (`unread-char`'s pushback, `fresh-line`'s last character).
struct TcpParts<'a> {
    sock: &'a mut TcpStream,
    rbuf: &'a mut VecDeque<u8>,
    wbuf: &'a mut VecDeque<u8>,
    pushback: &'a mut Vec<char>,
    last_written: &'a mut Option<char>,
}

impl StreamTable {
    /// The socket behind `h`, or the same errors `readable`/`writable` give
    /// for a closed or wrong handle.
    fn tcp(&mut self, h: Handle, who: &str) -> StreamResult<TcpParts<'_>> {
        let s = self.get(h)?;
        match &mut s.backend {
            Backend::Tcp { sock, rbuf, wbuf } => {
                Ok(TcpParts { sock, rbuf, wbuf, pushback: &mut s.pushback, last_written: &mut s.last_written })
            }
            Backend::Closed => Err(format!("{}: the stream is closed", who)),
            _ => Err(format!("{}: not a TCP stream", who)),
        }
    }

    fn listener(&mut self, h: Handle, who: &str) -> StreamResult<&mut TcpListener> {
        match &mut self.get(h)?.backend {
            Backend::Listener(l) => Ok(l),
            Backend::Closed => Err(format!("{}: the listener is closed", who)),
            _ => Err(format!("{}: not a TCP listener", who)),
        }
    }

    /// Starts connecting to `host:port` and returns the handle at once; the
    /// connection is not up yet. The caller waits for the socket to become
    /// **writable** and then asks [`Self::net_connect_finish`].
    ///
    /// Name resolution happens here and is the one thing on this path that
    /// can take time: `to_socket_addrs` is `getaddrinfo`, which blocks the
    /// thread. Accepted for now — Go's cgo resolver blocks a thread too, and
    /// hands its P off while it does; the hand-off is what an M:N scheduler
    /// would add. The first address the resolver gives is the one tried.
    pub fn net_connect_begin(&mut self, host: &str, port: i64) -> StreamResult<Handle> {
        let port = u16::try_from(port).map_err(|_| format!("tcp-connect: {} is not a port number (0..65535)", port))?;
        let addr = (host, port)
            .to_socket_addrs()
            .map_err(|e| format!("tcp-connect: {}:{}: {}", host, port, e))?
            .next()
            .ok_or_else(|| format!("tcp-connect: {}: no address", host))?;
        let sock = crate::os::tcp_connect_begin(addr).map_err(|e| format!("tcp-connect: {}: {}", addr, e))?;
        Ok(self.insert(StreamObj::new(
            Backend::Tcp { sock, rbuf: VecDeque::new(), wbuf: VecDeque::new() },
            true,
            true,
        )))
    }

    /// Whether the connect [`Self::net_connect_begin`] started has
    /// succeeded — asked once the socket is writable. The error is the one
    /// the kernel recorded on the socket (`SO_ERROR`): "connection refused",
    /// "host unreachable", and so on.
    pub fn net_connect_finish(&mut self, h: Handle) -> StreamResult<()> {
        let TcpParts { sock, .. } = self.tcp(h, "tcp-connect")?;
        match sock.take_error() {
            Ok(None) => Ok(()),
            Ok(Some(e)) => Err(format!("tcp-connect: {}", e)),
            Err(e) => Err(format!("tcp-connect: {}", e)),
        }
    }

    /// A listening socket on `host:port`. Port `0` asks the OS for a free
    /// one — [`Self::net_local_address`] says which.
    pub fn net_listen(&mut self, host: &str, port: i64) -> StreamResult<Handle> {
        let port = u16::try_from(port).map_err(|_| format!("tcp-listen: {} is not a port number (0..65535)", port))?;
        let l = TcpListener::bind((host, port)).map_err(|e| format!("tcp-listen: {}:{}: {}", host, port, e))?;
        l.set_nonblocking(true).map_err(|e| format!("tcp-listen: {}", e))?;
        Ok(self.insert(StreamObj::new(Backend::Listener(l), false, false)))
    }

    /// The next connection waiting on listener `h`, or `None` if nobody is
    /// — the caller then waits for the listener to become **readable**.
    pub fn net_accept(&mut self, h: Handle) -> StreamResult<Option<Handle>> {
        let l = self.listener(h, "accept")?;
        let sock = match l.accept() {
            Ok((sock, _)) => sock,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
            Err(e) => return Err(format!("accept: {}", e)),
        };
        sock.set_nonblocking(true).map_err(|e| format!("accept: {}", e))?;
        Ok(Some(self.insert(StreamObj::new(
            Backend::Tcp { sock, rbuf: VecDeque::new(), wbuf: VecDeque::new() },
            true,
            true,
        ))))
    }

    /// Receives what the OS has into the read buffer: `Some(n)` bytes were
    /// added, `Some(0)` is end of input (the peer closed its side), `None`
    /// is "nothing yet" — the caller then waits for **readable**.
    pub fn net_fill(&mut self, h: Handle) -> StreamResult<Option<usize>> {
        let TcpParts { sock, rbuf, .. } = self.tcp(h, "read")?;
        let mut chunk = [0u8; FILL_CHUNK];
        match sock.read(&mut chunk) {
            Ok(n) => {
                rbuf.extend(&chunk[..n]);
                Ok(Some(n))
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) if e.kind() == ErrorKind::Interrupted => Ok(None),
            Err(e) => Err(format!("read: {}", e)),
        }
    }

    /// The next buffered byte, or `None` if the buffer is empty (the caller
    /// then [`Self::net_fill`]s). Refused while a character is pushed back,
    /// for `read_byte`'s reason.
    pub fn net_pop_byte(&mut self, h: Handle) -> StreamResult<Option<u8>> {
        let TcpParts { rbuf, pushback, .. } = self.tcp(h, "read-byte")?;
        if !pushback.is_empty() {
            return Err("read-byte: the stream has an unread character pending".to_string());
        }
        Ok(rbuf.pop_front())
    }

    /// The next character: a pushed-back one first, else one decoded from
    /// the front of the read buffer. `None` means the buffer does not hold a
    /// whole character yet — nothing is consumed, and the caller fills.
    /// Bytes that cannot begin or continue a UTF-8 character are an error,
    /// as they are on a file.
    pub fn net_pop_char(&mut self, h: Handle) -> StreamResult<Option<char>> {
        let TcpParts { rbuf, pushback, .. } = self.tcp(h, "read-char")?;
        if let Some(c) = pushback.pop() {
            return Ok(Some(c));
        }
        let Some(&first) = rbuf.front() else {
            return Ok(None);
        };
        let need = utf8_width(first).ok_or_else(|| "invalid UTF-8 in input".to_string())?;
        if rbuf.len() < need {
            return Ok(None);
        }
        let bytes: Vec<u8> = rbuf.iter().take(need).copied().collect();
        let c = std::str::from_utf8(&bytes)
            .map_err(|_| "invalid UTF-8 in input".to_string())?
            .chars()
            .next()
            .ok_or_else(|| "invalid UTF-8 in input".to_string())?;
        rbuf.drain(..need);
        Ok(Some(c))
    }

    /// Whether a read can answer without going to the OS: something is
    /// pushed back or buffered. What `listen` reports for a socket.
    pub fn net_buffered(&mut self, h: Handle) -> StreamResult<bool> {
        let TcpParts { rbuf, pushback, .. } = self.tcp(h, "listen")?;
        Ok(!pushback.is_empty() || !rbuf.is_empty())
    }

    /// Queues `text` to be sent. Never waits; [`Self::net_flush`] sends.
    pub fn net_push_string(&mut self, h: Handle, text: &str) -> StreamResult<()> {
        let TcpParts { wbuf, last_written, .. } = self.tcp(h, "write")?;
        *last_written = text.chars().last().or(*last_written);
        wbuf.extend(text.as_bytes());
        Ok(())
    }

    /// Queues one byte.
    pub fn net_push_byte(&mut self, h: Handle, b: u8) -> StreamResult<()> {
        let TcpParts { wbuf, last_written, .. } = self.tcp(h, "write-byte")?;
        *last_written = Some(char::from(b));
        wbuf.push_back(b);
        Ok(())
    }

    /// Sends as much of the write buffer as the OS will take now: `true`
    /// when it is empty, `false` when some remains — the caller then waits
    /// for **writable** and flushes again.
    pub fn net_flush(&mut self, h: Handle) -> StreamResult<bool> {
        let TcpParts { sock, wbuf, .. } = self.tcp(h, "finish-output")?;
        while !wbuf.is_empty() {
            let (front, _) = wbuf.as_slices();
            match sock.write(front) {
                Ok(0) => return Err("write: the connection is closed".to_string()),
                Ok(n) => {
                    wbuf.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(false),
                Err(e) if e.kind() == ErrorKind::Interrupted => return Ok(false),
                Err(e) => return Err(format!("write: {}", e)),
            }
        }
        Ok(true)
    }

    /// Closes the sending side only: the peer reads end of input, and this
    /// side can still read. What a request/response protocol needs to say
    /// "that was the whole request". Anything still buffered is lost, so the
    /// prelude flushes first.
    pub fn net_shutdown_write(&mut self, h: Handle) -> StreamResult<()> {
        let TcpParts { sock, .. } = self.tcp(h, "shutdown-output")?;
        sock.shutdown(std::net::Shutdown::Write).map_err(|e| format!("shutdown-output: {}", e))
    }

    /// This side's address as `host:port` — a socket's or a listener's. For
    /// a listener bound to port 0 this is how the chosen port is learned.
    pub fn net_local_address(&mut self, h: Handle) -> StreamResult<String> {
        let addr = match &self.get(h)?.backend {
            Backend::Tcp { sock, .. } => sock.local_addr(),
            Backend::Listener(l) => l.local_addr(),
            Backend::Closed => return Err("local-address: the stream is closed".to_string()),
            _ => return Err("local-address: not a socket".to_string()),
        };
        addr.map(|a| a.to_string()).map_err(|e| format!("local-address: {}", e))
    }

    /// The peer's address as `host:port`.
    pub fn net_peer_address(&mut self, h: Handle) -> StreamResult<String> {
        let TcpParts { sock, .. } = self.tcp(h, "peer-address")?;
        sock.peer_addr().map(|a| a.to_string()).map_err(|e| format!("peer-address: {}", e))
    }
}

/// How many bytes the UTF-8 character starting with `first` occupies, or
/// `None` if `first` cannot start one.
fn utf8_width(first: u8) -> Option<usize> {
    match first {
        0x00..=0x7F => Some(1),
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A loopback pair, connected. The connect is driven to completion here
    /// the way the prelude does it: wait for writable, then ask.
    fn connected_pair(t: &mut StreamTable) -> (Handle, Handle) {
        let l = t.net_listen("127.0.0.1", 0).unwrap();
        let addr = t.net_local_address(l).unwrap();
        let port: i64 = addr.rsplit(':').next().unwrap().parse().unwrap();
        let c = t.net_connect_begin("127.0.0.1", port).unwrap();
        let cfd = t.raw_fd(c).unwrap();
        crate::os::poll_ready(&[(cfd, crate::os::Interest::Writable)], Some(std::time::Duration::from_secs(5))).unwrap();
        t.net_connect_finish(c).unwrap();
        let lfd = t.raw_fd(l).unwrap();
        crate::os::poll_ready(&[(lfd, crate::os::Interest::Readable)], Some(std::time::Duration::from_secs(5))).unwrap();
        let s = t.net_accept(l).unwrap().expect("a connection was waiting");
        (c, s)
    }

    fn fill_until_some(t: &mut StreamTable, h: Handle) -> usize {
        let fd = t.raw_fd(h).unwrap();
        loop {
            if let Some(n) = t.net_fill(h).unwrap() {
                return n;
            }
            crate::os::poll_ready(&[(fd, crate::os::Interest::Readable)], Some(std::time::Duration::from_secs(5))).unwrap();
        }
    }

    #[test]
    fn a_line_crosses_the_loopback() {
        let mut t = StreamTable::default();
        let (c, s) = connected_pair(&mut t);
        t.net_push_string(c, "héllo\n").unwrap();
        assert!(t.net_flush(c).unwrap());
        assert!(fill_until_some(&mut t, s) > 0);
        let mut got = String::new();
        while let Some(ch) = t.net_pop_char(s).unwrap() {
            got.push(ch);
        }
        assert_eq!(got, "héllo\n");
    }

    #[test]
    fn a_character_split_across_reads_is_not_consumed_early() {
        let mut t = StreamTable::default();
        let (c, s) = connected_pair(&mut t);
        // "é" is C3 A9. Send only the first byte.
        t.net_push_byte(c, 0xC3).unwrap();
        assert!(t.net_flush(c).unwrap());
        fill_until_some(&mut t, s);
        assert_eq!(t.net_pop_char(s).unwrap(), None);
        t.net_push_byte(c, 0xA9).unwrap();
        assert!(t.net_flush(c).unwrap());
        fill_until_some(&mut t, s);
        assert_eq!(t.net_pop_char(s).unwrap(), Some('é'));
    }

    #[test]
    fn end_of_input_is_a_zero_fill() {
        let mut t = StreamTable::default();
        let (c, s) = connected_pair(&mut t);
        t.close(c).unwrap();
        assert_eq!(fill_until_some(&mut t, s), 0);
    }

    #[test]
    fn a_pushed_back_character_refuses_a_byte_read() {
        let mut t = StreamTable::default();
        let (_c, s) = connected_pair(&mut t);
        t.unread_char(s, 'x').unwrap();
        assert!(t.net_pop_byte(s).unwrap_err().contains("unread character"));
        assert_eq!(t.net_pop_char(s).unwrap(), Some('x'));
    }

    #[test]
    fn a_refused_connect_reports_it_after_the_wait() {
        let mut t = StreamTable::default();
        // A port nobody listens on: bind one, learn it, close it.
        let l = t.net_listen("127.0.0.1", 0).unwrap();
        let addr = t.net_local_address(l).unwrap();
        let port: i64 = addr.rsplit(':').next().unwrap().parse().unwrap();
        t.close(l).unwrap();
        let outcome = match t.net_connect_begin("127.0.0.1", port) {
            Err(e) => Err(e),
            Ok(c) => {
                let fd = t.raw_fd(c).unwrap();
                crate::os::poll_ready(&[(fd, crate::os::Interest::Writable)], Some(std::time::Duration::from_secs(5))).unwrap();
                t.net_connect_finish(c)
            }
        };
        assert!(outcome.is_err(), "connecting to a closed port succeeded");
    }

    #[test]
    fn a_bad_port_is_an_error_not_a_panic() {
        let mut t = StreamTable::default();
        assert!(t.net_listen("127.0.0.1", 70000).is_err());
        assert!(t.net_connect_begin("127.0.0.1", -1).is_err());
    }
}
