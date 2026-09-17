//! TCP sockets in the stream table: everything a socket can do **right
//! now**, and nothing it would have to wait for.
//!
//! The rule every operation here follows: try once, non-blocking, and if the
//! socket is not ready say so *as an answer* — `None` from [`StreamTable::
//! net_fill`], `None` from [`StreamTable::net_accept`], `false` from
//! [`StreamTable::net_flush`]. The waiting is done in typelisp: the prelude's
//! `socket-stream` methods loop, and when the answer is "not yet" they call
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
//! Three more things follow the same rule. **TLS** (`tls-connect`) is the
//! same `Tcp` entry with a `rustls::ClientConnection` between the buffers
//! and the socket: `rustls` is sans-IO, so [`StreamTable::net_fill`] and
//! [`StreamTable::net_flush`] are still the only places bytes move, and the
//! handshake is driven by [`StreamTable::net_tls_handshake`] one non-blocking
//! step at a time. **UDP** is whole datagrams — `None` when none has arrived.
//! **Name resolution** ([`StreamTable::net_resolve_begin`]) is the one thing
//! that has no non-blocking form in the C library, so it runs on a helper
//! thread that writes a byte into a pipe when it is done; the pipe's read end
//! is what the task waits on, and the thread never touches the heap.
//!
//! One socket, two views: the prelude's `socket-stream` (characters) and
//! `socket-byte-stream` (bytes) are two structs over the **same handle**, so the
//! byte and character operations here share one `rbuf`. Mixing them is
//! legal — an HTTP client reads headers as text and the body as bytes — with
//! the one rule `read_byte` already has: a pushed-back character and a byte
//! read disagree about where the stream is, so the byte read is refused
//! while pushback is pending.

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};

use crate::stream::{Backend, Handle, Listen, ResolverAnswer, Sock, StreamObj, StreamResult, StreamTable};

/// How much one `net_fill` asks the OS for. Big enough that a line of text
/// arrives in one read; small enough that a slow reader does not pin
/// megabytes per connection.
const FILL_CHUNK: usize = 16 * 1024;

/// The parts of a connected socket's entry an operation works on: the
/// socket, its read and write buffers, and the two per-stream facts every
/// backend keeps (`unread-char`'s pushback, `fresh-line`'s last character).
struct TcpParts<'a> {
    sock: &'a mut Sock,
    tls: &'a mut Option<Box<rustls::ClientConnection>>,
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
            Backend::Tcp { sock, tls, rbuf, wbuf } => {
                Ok(TcpParts { sock, tls, rbuf, wbuf, pushback: &mut s.pushback, last_written: &mut s.last_written })
            }
            Backend::Closed => Err(format!("{}: the stream is closed", who)),
            _ => Err(format!("{}: not a TCP stream", who)),
        }
    }

    fn listener(&mut self, h: Handle, who: &str) -> StreamResult<&mut Listen> {
        match &mut self.get(h)?.backend {
            Backend::Listener(l) => Ok(l),
            Backend::Closed => Err(format!("{}: the listener is closed", who)),
            _ => Err(format!("{}: not a TCP listener", who)),
        }
    }

    /// Starts connecting to `addr` — an `ip:port` string, already resolved
    /// ([`Self::net_resolve_finish`]'s answer) — and returns the handle at
    /// once; the connection is not up yet. The caller waits for the socket
    /// to become **writable** and then asks [`Self::net_connect_finish`].
    ///
    /// An address and not a name, so that nothing on this path can take
    /// time: the one call that can, `getaddrinfo`, is the resolver's, on its
    /// own thread.
    pub fn net_connect_begin(&mut self, addr: &str) -> StreamResult<Handle> {
        let addr: SocketAddr = addr.parse().map_err(|_| format!("tcp-connect: {} is not an ip:port address", addr))?;
        let sock = crate::os::tcp_connect_begin(addr).map_err(|e| format!("tcp-connect: {}: {}", addr, e))?;
        Ok(self.insert(StreamObj::new(
            Backend::Tcp { sock: Sock::Tcp(sock), tls: None, rbuf: VecDeque::new(), wbuf: VecDeque::new() },
            true,
            true,
        )))
    }

    /// Connects to the Unix-domain socket at `path`. Complete on return:
    /// there is no handshake to wait for on a local socket — `connect`
    /// either succeeds, is refused (nobody listening, or no such file), or
    /// fails with `EAGAIN` when the listener's backlog is full, which is
    /// reported rather than retried since it names a listener that is not
    /// keeping up. The socket is then put into non-blocking mode like every
    /// other, and the same buffers and wait loops serve it.
    pub fn net_unix_connect(&mut self, path: &str) -> StreamResult<Handle> {
        let sock = std::os::unix::net::UnixStream::connect(path).map_err(|e| format!("unix-connect: {}: {}", path, e))?;
        sock.set_nonblocking(true).map_err(|e| format!("unix-connect: {}", e))?;
        Ok(self.insert(StreamObj::new(
            Backend::Tcp { sock: Sock::Unix(sock), tls: None, rbuf: VecDeque::new(), wbuf: VecDeque::new() },
            true,
            true,
        )))
    }

    /// A listening Unix-domain socket at `path`. The file must not exist:
    /// a leftover from a listener that was not closed is refused rather
    /// than silently replaced, since it may belong to a process that is
    /// still running. Closing the listener removes the file.
    pub fn net_unix_listen(&mut self, path: &str) -> StreamResult<Handle> {
        let l = std::os::unix::net::UnixListener::bind(path).map_err(|e| format!("unix-listen: {}: {}", path, e))?;
        l.set_nonblocking(true).map_err(|e| format!("unix-listen: {}", e))?;
        let path = std::path::PathBuf::from(path);
        Ok(self.insert(StreamObj::new(Backend::Listener(Listen::Unix { listener: l, path }), false, false)))
    }

    /// Starts resolving `host:port` on a helper thread and returns a handle
    /// the task can wait on (**readable** when the answer is in).
    ///
    /// `getaddrinfo` has no non-blocking form, and a name that does not
    /// resolve can take the resolver's whole timeout to say so — seconds
    /// during which every task would otherwise stop. So the call is made on
    /// a thread of its own, which does two things when it returns: leaves
    /// the answer in the shared slot, and writes one byte to a pipe. The
    /// read end of that pipe is the descriptor the task parks on, through
    /// the same `poll` every socket wait goes through. The thread touches
    /// nothing else — no heap, no table — which is what makes it safe next
    /// to a single-threaded runtime.
    ///
    /// An `ip:port` that needs no lookup still goes through here, so that
    /// there is one path; the thread just answers at once.
    pub fn net_resolve_begin(&mut self, host: &str, port: i64) -> StreamResult<Handle> {
        let port = u16::try_from(port).map_err(|_| format!("resolve: {} is not a port number (0..65535)", port))?;
        let (reader, mut writer) = std::io::pipe().map_err(|e| format!("resolve: {}", e))?;
        let answer: ResolverAnswer = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&answer);
        let host = host.to_string();
        std::thread::Builder::new()
            .name(format!("resolve {}", host))
            .spawn(move || {
                let result = (host.as_str(), port)
                    .to_socket_addrs()
                    .map_err(|e| format!("resolve: {}:{}: {}", host, port, e))
                    .and_then(|it| {
                        let all: Vec<SocketAddr> = it.collect();
                        if all.is_empty() {
                            Err(format!("resolve: {}: no address", host))
                        } else {
                            Ok(all)
                        }
                    });
                if let Ok(mut a) = slot.lock() {
                    *a = Some(result);
                }
                // The waiter may already have closed its end (a closed
                // handle): then there is nobody to tell, and that is fine.
                let _ = writer.write_all(&[1]);
            })
            .map_err(|e| format!("resolve: cannot start a resolver thread: {}", e))?;
        Ok(self.insert(StreamObj::new(Backend::Resolver { pipe: reader, answer }, false, false)))
    }

    /// The resolver's answer — **every** address the name has, each as
    /// `ip:port` in the resolver's order — once its handle is readable. A
    /// name usually has more than one (`localhost` is `::1` and `127.0.0.1`),
    /// and which one is reachable is not the resolver's to know, so the
    /// caller tries them in turn; that is what Go's dialer does too.
    /// `Ok(None)` if the thread has not finished yet (a spurious wake).
    /// Closes the handle once there is an answer — a resolution is used
    /// exactly once.
    pub fn net_resolve_finish(&mut self, h: Handle) -> StreamResult<Option<Vec<String>>> {
        let s = self.get(h)?;
        let taken = match &s.backend {
            Backend::Resolver { answer, .. } => match answer.lock() {
                Ok(mut a) => a.take(),
                Err(_) => return Err("resolve: the resolver thread panicked".to_string()),
            },
            Backend::Closed => return Err("resolve: the handle is closed".to_string()),
            _ => return Err("resolve: not a resolver handle".to_string()),
        };
        match taken {
            None => Ok(None),
            Some(result) => {
                s.backend = Backend::Closed;
                result.map(|all| Some(all.iter().map(|a| a.to_string()).collect()))
            }
        }
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
        Ok(self.insert(StreamObj::new(Backend::Listener(Listen::Tcp(l)), false, false)))
    }

    /// The next connection waiting on listener `h`, or `None` if nobody is
    /// — the caller then waits for the listener to become **readable**.
    pub fn net_accept(&mut self, h: Handle) -> StreamResult<Option<Handle>> {
        let l = self.listener(h, "accept")?;
        let accepted = match l {
            Listen::Tcp(l) => l.accept().map(|(s, _)| Sock::Tcp(s)),
            Listen::Unix { listener, .. } => listener.accept().map(|(s, _)| Sock::Unix(s)),
        };
        let sock = match accepted {
            Ok(sock) => sock,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
            Err(e) => return Err(format!("accept: {}", e)),
        };
        sock.set_nonblocking(true).map_err(|e| format!("accept: {}", e))?;
        Ok(Some(self.insert(StreamObj::new(
            Backend::Tcp { sock, tls: None, rbuf: VecDeque::new(), wbuf: VecDeque::new() },
            true,
            true,
        ))))
    }

    /// Receives what the OS has into the read buffer: `Some(n)` bytes were
    /// added, `Some(0)` is end of input (the peer closed its side), `None`
    /// is "nothing yet" — the caller then waits for **readable**.
    pub fn net_fill(&mut self, h: Handle) -> StreamResult<Option<usize>> {
        let TcpParts { sock, tls, rbuf, .. } = self.tcp(h, "read")?;
        let Some(conn) = tls else {
            let mut chunk = [0u8; FILL_CHUNK];
            return match sock.read(&mut chunk) {
                Ok(n) => {
                    rbuf.extend(&chunk[..n]);
                    Ok(Some(n))
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
                Err(e) if e.kind() == ErrorKind::Interrupted => Ok(None),
                Err(e) => Err(format!("read: {}", e)),
            };
        };
        // TLS: anything the connection wants to send first (a handshake
        // message, an alert) goes out, best effort — a handshake's writes
        // are small and fit the socket buffer, so this does not wait for
        // writable. Then one read of ciphertext, decrypted into `rbuf`.
        tls_write_out(conn, sock, "read")?;
        match conn.read_tls(sock) {
            Ok(0) => {
                // The peer closed the socket. Whether cleanly (`close_notify`
                // seen) or not, there is nothing more to read.
                return Ok(Some(0));
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
            Err(e) if e.kind() == ErrorKind::Interrupted => return Ok(None),
            Err(e) => return Err(format!("read: {}", e)),
        }
        let state = conn.process_new_packets().map_err(|e| format!("tls: {}", e))?;
        let plain = state.plaintext_bytes_to_read();
        if plain > 0 {
            let mut chunk = vec![0u8; plain];
            conn.reader().read_exact(&mut chunk).map_err(|e| format!("tls: {}", e))?;
            rbuf.extend(&chunk);
            return Ok(Some(plain));
        }
        if state.peer_has_closed() {
            return Ok(Some(0));
        }
        // Ciphertext arrived but decrypted to nothing yet (a handshake
        // record): not end of input, not nothing — the caller loops. `Some`
        // of zero would read as end of input, so the honest answer is "read
        // again", which `None` means without a wait since the socket may
        // already hold more.
        tls_write_out(conn, sock, "read")?;
        Ok(None)
    }

    /// One non-blocking step of a TLS handshake, after
    /// [`Self::net_tls_start`]: `Ok(None)` when it is complete, `Ok(Some(i))`
    /// when the socket has to become ready for `i` (`0` readable, `1`
    /// writable) before the next step. A certificate the root store does not
    /// trust, or a peer that speaks no TLS, is the `Err`.
    pub fn net_tls_handshake(&mut self, h: Handle) -> StreamResult<Option<i64>> {
        let TcpParts { sock, tls, .. } = self.tcp(h, "tls-connect")?;
        let Some(conn) = tls else {
            return Err("tls-connect: not a TLS connection".to_string());
        };
        if !conn.is_handshaking() {
            return Ok(None);
        }
        if conn.wants_write() {
            match conn.write_tls(sock) {
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(Some(1)),
                Err(e) => return Err(format!("tls-connect: {}", e)),
            }
        }
        if conn.wants_read() {
            match conn.read_tls(sock) {
                Ok(0) => return Err("tls-connect: the connection closed during the handshake".to_string()),
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(Some(0)),
                Err(e) => return Err(format!("tls-connect: {}", e)),
            }
            conn.process_new_packets().map_err(|e| format!("tls-connect: {}", e))?;
        }
        if conn.is_handshaking() {
            // More to do, and the socket may already be ready for it: say
            // which side, and let the caller's wait answer at once if so.
            Ok(Some(if conn.wants_write() { 1 } else { 0 }))
        } else {
            Ok(None)
        }
    }

    /// Puts a TLS client connection on a connected socket, for `server_name`
    /// (the name the certificate must be for). Nothing is sent yet: the
    /// handshake is [`Self::net_tls_handshake`]'s, one step per call.
    pub fn net_tls_start(&mut self, h: Handle, server_name: &str) -> StreamResult<()> {
        let TcpParts { tls, rbuf, wbuf, .. } = self.tcp(h, "tls-connect")?;
        if tls.is_some() {
            return Err("tls-connect: the connection already speaks TLS".to_string());
        }
        if !rbuf.is_empty() || !wbuf.is_empty() {
            return Err("tls-connect: the connection has already been used in the clear".to_string());
        }
        let name = rustls::pki_types::ServerName::try_from(server_name.to_string())
            .map_err(|_| format!("tls-connect: {} is not a valid server name", server_name))?;
        let conn = rustls::ClientConnection::new(tls_client_config(), name).map_err(|e| format!("tls-connect: {}", e))?;
        *tls = Some(Box::new(conn));
        Ok(())
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
        let TcpParts { sock, tls, wbuf, .. } = self.tcp(h, "finish-output")?;
        if let Some(conn) = tls {
            // Plaintext into the connection (it buffers without limit), then
            // ciphertext out to the socket for as long as the socket takes it.
            while !wbuf.is_empty() {
                let (front, _) = wbuf.as_slices();
                let n = conn.writer().write(front).map_err(|e| format!("tls: {}", e))?;
                wbuf.drain(..n);
            }
            while conn.wants_write() {
                match conn.write_tls(sock) {
                    Ok(0) => return Err("write: the connection is closed".to_string()),
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(false),
                    Err(e) if e.kind() == ErrorKind::Interrupted => return Ok(false),
                    Err(e) => return Err(format!("write: {}", e)),
                }
            }
            return Ok(true);
        }
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
        let TcpParts { sock, tls, .. } = self.tcp(h, "shutdown-output")?;
        if let Some(conn) = tls {
            // TLS has its own end-of-stream message, and the peer needs it to
            // tell a finished request from a truncated one.
            conn.send_close_notify();
            tls_write_out(conn, sock, "shutdown-output")?;
        }
        sock.shutdown(std::net::Shutdown::Write).map_err(|e| format!("shutdown-output: {}", e))
    }

    // ---- UDP ----------------------------------------------------------------

    /// A UDP socket bound to `host:port` (`0` for any free port).
    pub fn net_udp_bind(&mut self, host: &str, port: i64) -> StreamResult<Handle> {
        let port = u16::try_from(port).map_err(|_| format!("udp-bind: {} is not a port number (0..65535)", port))?;
        let sock = UdpSocket::bind((host, port)).map_err(|e| format!("udp-bind: {}:{}: {}", host, port, e))?;
        sock.set_nonblocking(true).map_err(|e| format!("udp-bind: {}", e))?;
        Ok(self.insert(StreamObj::new(Backend::Udp { sock, last_from: None }, false, false)))
    }

    fn udp(&mut self, h: Handle, who: &str) -> StreamResult<(&mut UdpSocket, &mut Option<SocketAddr>)> {
        match &mut self.get(h)?.backend {
            Backend::Udp { sock, last_from } => Ok((sock, last_from)),
            Backend::Closed => Err(format!("{}: the socket is closed", who)),
            _ => Err(format!("{}: not a UDP socket", who)),
        }
    }

    /// Sends one datagram to `addr` (`ip:port`, already resolved). `false`
    /// if the socket's send buffer is full — wait for writable and retry;
    /// a datagram is never sent in part.
    pub fn net_udp_send_to(&mut self, h: Handle, addr: &str, data: &[u8]) -> StreamResult<bool> {
        let addr: SocketAddr = addr.parse().map_err(|_| format!("send-to: {} is not an ip:port address", addr))?;
        let (sock, _) = self.udp(h, "send-to")?;
        match sock.send_to(data, addr) {
            Ok(n) if n == data.len() => Ok(true),
            Ok(n) => Err(format!("send-to: only {} of {} bytes were sent", n, data.len())),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(false),
            Err(e) if e.kind() == ErrorKind::Interrupted => Ok(false),
            Err(e) => Err(format!("send-to: {}", e)),
        }
    }

    /// The next datagram, or `None` if none has arrived — wait for readable.
    /// The sender is remembered for [`Self::net_udp_last_sender`]. A datagram
    /// longer than 64KiB cannot exist on IPv4, so nothing is truncated.
    pub fn net_udp_recv(&mut self, h: Handle) -> StreamResult<Option<Vec<u8>>> {
        let (sock, last_from) = self.udp(h, "recv-from")?;
        let mut buf = vec![0u8; 65536];
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                buf.truncate(n);
                *last_from = Some(from);
                Ok(Some(buf))
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) if e.kind() == ErrorKind::Interrupted => Ok(None),
            Err(e) => Err(format!("recv-from: {}", e)),
        }
    }

    /// Who sent the datagram [`Self::net_udp_recv`] last returned, as
    /// `ip:port`. An error before any has been received.
    pub fn net_udp_last_sender(&mut self, h: Handle) -> StreamResult<String> {
        let (_, last_from) = self.udp(h, "recv-from")?;
        last_from.map(|a| a.to_string()).ok_or_else(|| "recv-from: no datagram has been received yet".to_string())
    }

    /// This side's address as `host:port` — a socket's or a listener's. For
    /// a listener bound to port 0 this is how the chosen port is learned.
    pub fn net_local_address(&mut self, h: Handle) -> StreamResult<String> {
        let addr = match &self.get(h)?.backend {
            Backend::Tcp { sock, .. } => sock.local_address(),
            Backend::Listener(l) => l.local_address(),
            Backend::Udp { sock, .. } => sock.local_addr().map(|a| a.to_string()),
            Backend::Closed => return Err("local-address: the stream is closed".to_string()),
            _ => return Err("local-address: not a socket".to_string()),
        };
        addr.map_err(|e| format!("local-address: {}", e))
    }

    /// The peer's address as `host:port`.
    pub fn net_peer_address(&mut self, h: Handle) -> StreamResult<String> {
        let TcpParts { sock, .. } = self.tcp(h, "peer-address")?;
        sock.peer_address().map_err(|e| format!("peer-address: {}", e))
    }
}

/// Sends whatever ciphertext the connection has queued, without waiting: a
/// socket buffer that is full stops the attempt and leaves the rest queued
/// for the next one. Only a real failure is an error.
fn tls_write_out(conn: &mut rustls::ClientConnection, sock: &mut Sock, who: &str) -> StreamResult<()> {
    while conn.wants_write() {
        match conn.write_tls(sock) {
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == ErrorKind::Interrupted => break,
            Err(e) => return Err(format!("{}: {}", who, e)),
        }
    }
    Ok(())
}

/// The one client configuration every `tls-connect` shares: Mozilla's root
/// store (bundled by `webpki-roots`), no client certificate. Built once.
fn tls_client_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: std::sync::OnceLock<Arc<rustls::ClientConfig>> = std::sync::OnceLock::new();
    Arc::clone(CONFIG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        Arc::new(rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
    }))
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
        let c = t.net_connect_begin(&addr).unwrap();
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
        t.close(l).unwrap();
        let outcome = match t.net_connect_begin(&addr) {
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
        assert!(t.net_resolve_begin("127.0.0.1", -1).is_err());
        assert!(t.net_connect_begin("not an address").is_err());
    }

    fn resolve(t: &mut StreamTable, host: &str, port: i64) -> Result<Vec<String>, String> {
        let r = t.net_resolve_begin(host, port)?;
        let fd = t.raw_fd(r).unwrap();
        loop {
            crate::os::poll_ready(&[(fd, crate::os::Interest::Readable)], Some(std::time::Duration::from_secs(10))).unwrap();
            if let Some(a) = t.net_resolve_finish(r)? {
                return Ok(a);
            }
        }
    }

    #[test]
    fn the_resolver_answers_through_its_pipe() {
        let mut t = StreamTable::default();
        assert_eq!(resolve(&mut t, "127.0.0.1", 80).unwrap(), vec!["127.0.0.1:80".to_string()]);
        let local = resolve(&mut t, "localhost", 8080).unwrap();
        assert!(!local.is_empty() && local.iter().all(|a| a.ends_with(":8080")), "{:?}", local);
        let err = resolve(&mut t, "no-such-host.invalid", 1).unwrap_err();
        assert!(err.starts_with("resolve:"), "{}", err);
    }

    #[test]
    fn a_datagram_crosses_the_loopback() {
        let mut t = StreamTable::default();
        let a = t.net_udp_bind("127.0.0.1", 0).unwrap();
        let b = t.net_udp_bind("127.0.0.1", 0).unwrap();
        let to = t.net_local_address(b).unwrap();
        assert!(t.net_udp_send_to(a, &to, b"ping").unwrap());
        let fd = t.raw_fd(b).unwrap();
        crate::os::poll_ready(&[(fd, crate::os::Interest::Readable)], Some(std::time::Duration::from_secs(5))).unwrap();
        assert_eq!(t.net_udp_recv(b).unwrap(), Some(b"ping".to_vec()));
        assert_eq!(t.net_udp_last_sender(b).unwrap(), t.net_local_address(a).unwrap());
        assert_eq!(t.net_udp_recv(b).unwrap(), None);
    }

    #[test]
    fn a_line_crosses_a_unix_socket_and_closing_removes_the_file() {
        let mut t = StreamTable::default();
        let dir = std::env::temp_dir().join(format!("typelisp-unix-{}", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let path = dir.to_string_lossy().into_owned();
        let l = t.net_unix_listen(&path).unwrap();
        assert_eq!(t.net_local_address(l).unwrap(), path);
        assert!(t.net_unix_listen(&path).is_err(), "a second bind on a live socket file succeeded");
        let c = t.net_unix_connect(&path).unwrap();
        let lfd = t.raw_fd(l).unwrap();
        crate::os::poll_ready(&[(lfd, crate::os::Interest::Readable)], Some(std::time::Duration::from_secs(5))).unwrap();
        let s = t.net_accept(l).unwrap().expect("a connection was waiting");
        assert_eq!(t.net_peer_address(s).unwrap(), "(unnamed)");
        t.net_push_string(c, "over unix\n").unwrap();
        assert!(t.net_flush(c).unwrap());
        assert!(fill_until_some(&mut t, s) > 0);
        let mut got = String::new();
        while let Some(ch) = t.net_pop_char(s).unwrap() {
            got.push(ch);
        }
        assert_eq!(got, "over unix\n");
        t.close(l).unwrap();
        assert!(!dir.exists(), "closing the listener left the socket file behind");
        assert!(t.net_unix_connect(&path).is_err());
    }

    #[test]
    fn tls_refuses_a_peer_that_speaks_no_tls() {
        // A plain echo peer answers the ClientHello with garbage: the
        // handshake must fail with an error, not hang or succeed.
        let mut t = StreamTable::default();
        let (c, s) = connected_pair(&mut t);
        t.net_tls_start(c, "localhost").unwrap();
        let cfd = t.raw_fd(c).unwrap();
        let sfd = t.raw_fd(s).unwrap();
        let outcome = loop {
            match t.net_tls_handshake(c) {
                Ok(None) => break Ok(()),
                Err(e) => break Err(e),
                Ok(Some(i)) => {
                    let interest = if i == 1 { crate::os::Interest::Writable } else { crate::os::Interest::Readable };
                    crate::os::poll_ready(&[(cfd, interest)], Some(std::time::Duration::from_millis(200))).unwrap();
                }
            }
            // The peer echoes whatever it got.
            crate::os::poll_ready(&[(sfd, crate::os::Interest::Readable)], Some(std::time::Duration::from_millis(200))).unwrap();
            if let Some(n) = t.net_fill(s).unwrap() {
                if n == 0 {
                    break Err("peer closed".to_string());
                }
                let mut bytes = Vec::new();
                while let Some(b) = t.net_pop_byte(s).unwrap() {
                    bytes.push(b);
                }
                for b in bytes {
                    t.net_push_byte(s, b).unwrap();
                }
                t.net_flush(s).unwrap();
            }
        };
        assert!(outcome.is_err(), "a TLS handshake against an echo peer succeeded");
    }
}
