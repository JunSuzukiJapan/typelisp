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
//! Three more things follow the same rule. **TLS** (`tls-connect`,
//! `tls-listen`) is the same `Tcp` entry with a `rustls::Connection` between
//! the buffers and the socket: `rustls` is sans-IO, so [`StreamTable::
//! net_fill`] and [`StreamTable::net_flush`] are still the only places bytes
//! move, and they drive the handshake too, one non-blocking step at a time,
//! the way Go's `tls.Conn` shakes hands on its first `Read` or `Write`. A
//! client wants to know the handshake's outcome before it uses the
//! connection (an untrusted certificate is `tls-connect`'s `Err`), so
//! [`StreamTable::net_tls_handshake`] also steps it on its own; a server
//! does not step it at all — `accept` returns at once, and the task serving
//! the connection completes the handshake in its first read, so a client
//! that is slow to finish its handshake delays nobody but itself.
//! **UDP** is whole datagrams — `None` when none has arrived.
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
//!
//! Two kinds of failure, told apart by who caused them. The program's own —
//! a closed handle, a byte read behind a pushed-back character — are `Err`
//! and the prelude panics on them. The **peer's** — a reset connection, a
//! TLS alert, a socket closed under a write — are not the program's fault
//! and must not end a server that has other clients; they are recorded on
//! the entry ([`Backend::Tcp`]'s `failed`), after which a read says end of
//! input and a write is a no-op, and [`StreamTable::net_socket_error`] says
//! what happened. Go's `bufio.Writer` keeps its first error the same way.

use std::collections::{HashMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};

/// What a `tls-listen` listener holds: the configuration every accepted
/// connection shares, and the certificates it chooses among — kept apart
/// from the configuration so that `tls-add-certificate` can add one
/// without rebuilding it.
pub(crate) struct TlsServer {
    config: Arc<rustls::ServerConfig>,
    certs: Arc<SniCerts>,
}

/// The server's certificates, chosen by the name the client asked for
/// (SNI): the one `tls-listen` was given serves any name not listed, so a
/// client that sends no name — or one nobody added — still gets a
/// certificate and can decide for itself whether it is for the host it
/// meant. Names are added under a lock because the resolver is shared with
/// every connection's handshake.
#[derive(Debug)]
struct SniCerts {
    default: Arc<rustls::sign::CertifiedKey>,
    by_name: Mutex<HashMap<String, Arc<rustls::sign::CertifiedKey>>>,
}

impl rustls::server::ResolvesServerCert for SniCerts {
    fn resolve(&self, hello: rustls::server::ClientHello<'_>) -> Option<Arc<rustls::sign::CertifiedKey>> {
        let named = hello.server_name().and_then(|name| match self.by_name.lock() {
            Ok(table) => table.get(name).cloned(),
            Err(_) => None,
        });
        Some(named.unwrap_or_else(|| Arc::clone(&self.default)))
    }
}

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
    tls: &'a mut Option<Box<rustls::Connection>>,
    rbuf: &'a mut VecDeque<u8>,
    wbuf: &'a mut VecDeque<u8>,
    failed: &'a mut Option<String>,
    pushback: &'a mut Vec<char>,
    last_written: &'a mut Option<char>,
}

/// A fresh entry for a connected socket, before anything has crossed it.
fn tcp_entry(sock: Sock, tls: Option<Box<rustls::Connection>>) -> Backend {
    Backend::Tcp { sock, tls, rbuf: VecDeque::new(), wbuf: VecDeque::new(), failed: None }
}

impl StreamTable {
    /// The socket behind `h`, or the same errors `readable`/`writable` give
    /// for a closed or wrong handle.
    fn tcp(&mut self, h: Handle, who: &str) -> StreamResult<TcpParts<'_>> {
        let s = self.get(h)?;
        match &mut s.backend {
            Backend::Tcp { sock, tls, rbuf, wbuf, failed } => Ok(TcpParts {
                sock,
                tls,
                rbuf,
                wbuf,
                failed,
                pushback: &mut s.pushback,
                last_written: &mut s.last_written,
            }),
            Backend::Closed => Err(format!("{}: the stream is closed", who)),
            _ => Err(format!("{}: not a TCP stream", who)),
        }
    }

    fn listener(&mut self, h: Handle, who: &str) -> StreamResult<(&mut Listen, &Option<TlsServer>)> {
        match &mut self.get(h)?.backend {
            Backend::Listener { listen, tls } => Ok((listen, tls)),
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
        Ok(self.insert(StreamObj::new(tcp_entry(Sock::Tcp(sock), None), true, true)))
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
        Ok(self.insert(StreamObj::new(tcp_entry(Sock::Unix(sock), None), true, true)))
    }

    /// A listening Unix-domain socket at `path`. The file must not exist:
    /// a leftover from a listener that was not closed is refused rather
    /// than silently replaced, since it may belong to a process that is
    /// still running. Closing the listener removes the file.
    pub fn net_unix_listen(&mut self, path: &str) -> StreamResult<Handle> {
        let l = std::os::unix::net::UnixListener::bind(path).map_err(|e| format!("unix-listen: {}: {}", path, e))?;
        l.set_nonblocking(true).map_err(|e| format!("unix-listen: {}", e))?;
        let path = std::path::PathBuf::from(path);
        let listen = Listen::Unix { listener: l, path };
        Ok(self.insert(StreamObj::new(Backend::Listener { listen, tls: None }, false, false)))
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
        self.listen_with(host, port, None, "tcp-listen")
    }

    /// A listening socket whose every accepted connection speaks TLS with
    /// the certificate chain in `cert_file` and the private key in
    /// `key_file` (both PEM; the chain leaf first). The files are read and
    /// checked here, once — a key that does not match, or a file that is
    /// not PEM, is this call's `Err`, not the first client's. With
    /// `client_ca` (PEM), every client must present a certificate issued by
    /// one in that file (mutual TLS); without it, clients are not asked for
    /// one.
    pub fn net_tls_listen(
        &mut self,
        host: &str,
        port: i64,
        cert_file: &str,
        key_file: &str,
        client_ca: Option<&str>,
    ) -> StreamResult<Handle> {
        let server = tls_server_config(cert_file, key_file, client_ca).map_err(|e| format!("tls-listen: {}", e))?;
        self.listen_with(host, port, Some(server), "tls-listen")
    }

    /// One more certificate for the `tls-listen` listener `h`, presented to
    /// clients that ask for `name` (SNI). The chain is checked to be for
    /// that name here, so a mismatch is this call's `Err` and not a
    /// client's failed handshake. Takes effect for the next handshake.
    pub fn net_tls_add_certificate(&mut self, h: Handle, name: &str, cert_file: &str, key_file: &str) -> StreamResult<()> {
        let (_, tls) = self.listener(h, "tls-add-certificate")?;
        let Some(server) = tls else {
            return Err("tls-add-certificate: the listener does not speak TLS (made by tcp-listen)".to_string());
        };
        let ck = certified_key(cert_file, key_file).map_err(|e| format!("tls-add-certificate: {}", e))?;
        let server_name = rustls::pki_types::ServerName::try_from(name.to_string())
            .map_err(|_| format!("tls-add-certificate: {} is not a valid server name", name))?;
        let rustls::pki_types::ServerName::DnsName(dns) = &server_name else {
            return Err(format!("tls-add-certificate: {} is an address, and SNI carries names only", name));
        };
        let end_entity = ck.end_entity_cert().map_err(|e| format!("tls-add-certificate: {}: {}", cert_file, e))?;
        rustls::server::ParsedCertificate::try_from(end_entity)
            .and_then(|cert| rustls::client::verify_server_name(&cert, &server_name))
            .map_err(|e| format!("tls-add-certificate: {} is not for {}: {}", cert_file, name, e))?;
        let mut table = server.certs.by_name.lock().map_err(|_| "tls-add-certificate: the certificate table is poisoned".to_string())?;
        table.insert(dns.as_ref().to_ascii_lowercase(), Arc::new(ck));
        Ok(())
    }

    fn listen_with(&mut self, host: &str, port: i64, tls: Option<TlsServer>, who: &str) -> StreamResult<Handle> {
        let port = u16::try_from(port).map_err(|_| format!("{}: {} is not a port number (0..65535)", who, port))?;
        let l = TcpListener::bind((host, port)).map_err(|e| format!("{}: {}:{}: {}", who, host, port, e))?;
        l.set_nonblocking(true).map_err(|e| format!("{}: {}", who, e))?;
        Ok(self.insert(StreamObj::new(Backend::Listener { listen: Listen::Tcp(l), tls }, false, false)))
    }

    /// The next connection waiting on listener `h`, or `None` if nobody is
    /// — the caller then waits for the listener to become **readable**. On
    /// a `tls-listen` listener the connection gets its server-side TLS
    /// state here, but nothing is exchanged yet: the handshake happens in
    /// the connection's first read or write, so this never waits on the
    /// client and the next `accept` is not behind a slow handshake.
    pub fn net_accept(&mut self, h: Handle) -> StreamResult<Option<Handle>> {
        let (l, tls) = self.listener(h, "accept")?;
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
        let conn = match tls {
            None => None,
            Some(TlsServer { config, .. }) => {
                let server = rustls::ServerConnection::new(Arc::clone(config)).map_err(|e| format!("accept: tls: {}", e))?;
                Some(Box::new(rustls::Connection::Server(server)))
            }
        };
        Ok(Some(self.insert(StreamObj::new(tcp_entry(sock, conn), true, true))))
    }

    /// Receives what the OS has into the read buffer: `Some(n)` bytes were
    /// added, `Some(0)` is end of input (the peer closed its side, or broke
    /// the connection — `net_socket_error` tells which), `None` is "nothing
    /// yet" — the caller then waits for **readable**.
    pub fn net_fill(&mut self, h: Handle) -> StreamResult<Option<usize>> {
        let TcpParts { sock, tls, rbuf, failed, .. } = self.tcp(h, "read")?;
        if failed.is_some() {
            return Ok(Some(0));
        }
        let Some(conn) = tls else {
            let mut chunk = [0u8; FILL_CHUNK];
            return match sock.read(&mut chunk) {
                Ok(n) => {
                    rbuf.extend(&chunk[..n]);
                    Ok(Some(n))
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
                Err(e) if e.kind() == ErrorKind::Interrupted => Ok(None),
                Err(e) => Ok(Some(peer_failed(failed, format!("read: {}", e)))),
            };
        };
        // TLS: anything the connection wants to send first (a handshake
        // message, an alert) goes out, best effort — a handshake's writes
        // are small and fit the socket buffer, so this does not wait for
        // writable. Then one read of ciphertext, decrypted into `rbuf`.
        if let Err(e) = tls_write_out(conn, sock) {
            return Ok(Some(peer_failed(failed, format!("read: {}", e))));
        }
        match conn.read_tls(sock) {
            Ok(0) => {
                // The peer closed the socket. Whether cleanly (`close_notify`
                // seen) or not, there is nothing more to read.
                return Ok(Some(0));
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
            Err(e) if e.kind() == ErrorKind::Interrupted => return Ok(None),
            Err(e) => return Ok(Some(peer_failed(failed, format!("read: {}", e)))),
        }
        // A bad record or an alert from the peer (a client that rejected
        // our certificate says so with one) ends the connection: the alert
        // rustls queues in answer goes out with the same best effort.
        let state = match conn.process_new_packets() {
            Ok(state) => state,
            Err(e) => {
                let _ = tls_write_out(conn, sock);
                return Ok(Some(peer_failed(failed, format!("tls: {}", e))));
            }
        };
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
        if let Err(e) = tls_write_out(conn, sock) {
            return Ok(Some(peer_failed(failed, format!("read: {}", e))));
        }
        Ok(None)
    }

    /// One non-blocking step of a TLS handshake, after
    /// [`Self::net_tls_start`]: `Ok(None)` when it is complete, `Ok(Some(i))`
    /// when the socket has to become ready for `i` (`0` readable, `1`
    /// writable) before the next step. A certificate the root store does not
    /// trust, or a peer that speaks no TLS, is the `Err` — an error *value*
    /// here, unlike in a read, because this is `tls-connect`'s to report.
    pub fn net_tls_handshake(&mut self, h: Handle) -> StreamResult<Option<i64>> {
        let TcpParts { sock, tls, .. } = self.tcp(h, "tls-connect")?;
        let Some(conn) = tls else {
            return Err("tls-connect: not a TLS connection".to_string());
        };
        tls_handshake_step(conn, sock).map_err(|e| format!("tls-connect: {}", e))
    }

    /// Puts a TLS client connection on a connected socket, for `server_name`
    /// (the name the certificate must be for). Trusts Mozilla's root store,
    /// or only the certificates in `ca_file` (PEM) when one is given — a
    /// private CA, or the test's self-signed one. With `cert_file` and
    /// `key_file` (PEM, both or neither) the client presents that
    /// certificate when the server asks for one (mutual TLS). Nothing is
    /// sent yet: the handshake is [`Self::net_tls_handshake`]'s, one step
    /// per call.
    pub fn net_tls_start(
        &mut self,
        h: Handle,
        server_name: &str,
        ca_file: Option<&str>,
        cert_file: Option<&str>,
        key_file: Option<&str>,
    ) -> StreamResult<()> {
        let identity = match (cert_file, key_file) {
            (None, None) => None,
            (Some(c), Some(k)) => Some((c, k)),
            _ => return Err("tls-connect: a client certificate needs both cert-file and key-file".to_string()),
        };
        let config = tls_client_config(ca_file, identity).map_err(|e| format!("tls-connect: {}", e))?;
        let TcpParts { tls, rbuf, wbuf, .. } = self.tcp(h, "tls-connect")?;
        if tls.is_some() {
            return Err("tls-connect: the connection already speaks TLS".to_string());
        }
        if !rbuf.is_empty() || !wbuf.is_empty() {
            return Err("tls-connect: the connection has already been used in the clear".to_string());
        }
        let name = rustls::pki_types::ServerName::try_from(server_name.to_string())
            .map_err(|_| format!("tls-connect: {} is not a valid server name", server_name))?;
        let conn = rustls::ClientConnection::new(config, name).map_err(|e| format!("tls-connect: {}", e))?;
        *tls = Some(Box::new(rustls::Connection::Client(conn)));
        Ok(())
    }

    /// The subject of the certificate the peer presented, in RFC 4514 form
    /// (`CN=client,O=Example,C=JP`) — who a mutual-TLS client is, or what
    /// name a server's certificate was issued to. `None` on a plain
    /// connection, before the handshake, or when the peer sent none (a
    /// server that did not ask for one). The DER walk is `crate::x509`'s;
    /// a certificate the verifier accepted is one it can read.
    pub fn net_peer_subject(&mut self, h: Handle) -> StreamResult<Option<String>> {
        let TcpParts { tls, .. } = self.tcp(h, "peer-subject")?;
        let Some(conn) = tls else {
            return Ok(None);
        };
        match conn.peer_certificates().and_then(|chain| chain.first()) {
            None => Ok(None),
            Some(cert) => crate::x509::subject(cert.as_ref()).map(Some).map_err(|e| format!("peer-subject: {}", e)),
        }
    }

    /// The name the client asked for (SNI) on a server-side TLS connection,
    /// once its handshake has read the ClientHello: what a server holding
    /// several certificates uses to know which site it is serving. `None`
    /// on a plain or client-side connection, before the handshake, or when
    /// the client sent no name (it connected by address).
    pub fn net_server_name(&mut self, h: Handle) -> StreamResult<Option<String>> {
        let TcpParts { tls, .. } = self.tcp(h, "requested-server-name")?;
        Ok(match tls.as_deref() {
            Some(rustls::Connection::Server(server)) => server.server_name().map(str::to_string),
            _ => None,
        })
    }

    /// Whether small writes go out at once (`TCP_NODELAY`, Nagle's
    /// algorithm off). Every `write-string` here already reaches the socket
    /// before it returns, so with Nagle on, a reply sent as two writes — a
    /// header, then a body — can sit until the peer's delayed ACK releases
    /// the second; a request/response protocol wants this on. A Unix-domain
    /// socket has no Nagle, so the property already holds and there is
    /// nothing to set.
    pub fn net_set_nodelay(&mut self, h: Handle, on: bool) -> StreamResult<()> {
        let TcpParts { sock, .. } = self.tcp(h, "set-nodelay")?;
        match sock {
            Sock::Tcp(s) => s.set_nodelay(on).map_err(|e| format!("set-nodelay: {}", e)),
            Sock::Unix(_) => Ok(()),
        }
    }

    /// Whether the OS probes an idle connection to find out that its peer
    /// is gone (`SO_KEEPALIVE`); see `crate::os::set_keepalive`. Refused on
    /// a Unix-domain socket, which has no peer that can vanish behind a
    /// network — asking is a misunderstanding, not a no-op.
    pub fn net_set_keepalive(&mut self, h: Handle, on: bool) -> StreamResult<()> {
        let TcpParts { sock, .. } = self.tcp(h, "set-keepalive")?;
        match sock {
            Sock::Tcp(_) => crate::os::set_keepalive(sock.raw_fd(), on).map_err(|e| format!("set-keepalive: {}", e)),
            Sock::Unix(_) => Err("set-keepalive: a Unix-domain socket has no keepalive".to_string()),
        }
    }

    /// The idle time before the first keepalive probe and the interval
    /// between probes, both `secs` (whole seconds — the OS's unit — at
    /// least one); see `crate::os::set_keepalive_period`.
    pub fn net_set_keepalive_period(&mut self, h: Handle, secs: i64) -> StreamResult<()> {
        let TcpParts { sock, .. } = self.tcp(h, "set-keepalive-period")?;
        let secs = match u32::try_from(secs) {
            Ok(s) if s >= 1 => s,
            _ => return Err(format!("set-keepalive-period: {} is not a period in whole seconds of at least 1", secs)),
        };
        match sock {
            Sock::Tcp(_) => crate::os::set_keepalive_period(sock.raw_fd(), secs)
                .map_err(|e| format!("set-keepalive-period: {}", e)),
            Sock::Unix(_) => Err("set-keepalive-period: a Unix-domain socket has no keepalive".to_string()),
        }
    }

    /// Why the peer's side of the connection is gone, if it is: the first
    /// failure the peer caused (a reset, a TLS alert, a close under a
    /// write). `None` while the connection is sound — including after a
    /// clean end of input, which is not a failure.
    pub fn net_socket_error(&mut self, h: Handle) -> StreamResult<Option<String>> {
        let TcpParts { failed, .. } = self.tcp(h, "socket-error")?;
        Ok(failed.clone())
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

    /// Sends as much of the write buffer as the OS will take now: `None`
    /// when it is empty, `Some(i)` when some remains and the socket has to
    /// become ready for `i` (`0` readable, `1` writable) first — the caller
    /// waits for that and flushes again. Writable is the usual answer;
    /// readable is a TLS connection whose handshake is not done and is
    /// waiting on the peer's next message, since nothing can be encrypted
    /// before it. A connection the peer has broken drops what was buffered
    /// and answers `None`: there is nothing more that can be sent.
    pub fn net_flush(&mut self, h: Handle) -> StreamResult<Option<i64>> {
        let TcpParts { sock, tls, wbuf, failed, .. } = self.tcp(h, "finish-output")?;
        if failed.is_some() {
            wbuf.clear();
            return Ok(None);
        }
        if let Some(conn) = tls {
            if conn.is_handshaking() {
                match tls_handshake_step(conn, sock) {
                    Ok(None) => {}
                    Ok(Some(interest)) => return Ok(Some(interest)),
                    Err(e) => {
                        wbuf.clear();
                        peer_failed(failed, format!("write: {}", e));
                        return Ok(None);
                    }
                }
            }
            // Plaintext into the connection (it buffers without limit), then
            // ciphertext out to the socket for as long as the socket takes it.
            while !wbuf.is_empty() {
                let (front, _) = wbuf.as_slices();
                let n = conn.writer().write(front).map_err(|e| format!("tls: {}", e))?;
                wbuf.drain(..n);
            }
            while conn.wants_write() {
                match conn.write_tls(sock) {
                    Ok(0) => return Ok(sent_nothing_more(failed, "write: the connection is closed".to_string())),
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(Some(1)),
                    Err(e) if e.kind() == ErrorKind::Interrupted => return Ok(Some(1)),
                    Err(e) => return Ok(sent_nothing_more(failed, format!("write: {}", e))),
                }
            }
            return Ok(None);
        }
        while !wbuf.is_empty() {
            let (front, _) = wbuf.as_slices();
            match sock.write(front) {
                Ok(0) => {
                    wbuf.clear();
                    return Ok(sent_nothing_more(failed, "write: the connection is closed".to_string()));
                }
                Ok(n) => {
                    wbuf.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(Some(1)),
                Err(e) if e.kind() == ErrorKind::Interrupted => return Ok(Some(1)),
                Err(e) => {
                    wbuf.clear();
                    return Ok(sent_nothing_more(failed, format!("write: {}", e)));
                }
            }
        }
        Ok(None)
    }

    /// Closes the sending side only: the peer reads end of input, and this
    /// side can still read. What a request/response protocol needs to say
    /// "that was the whole request". Anything still buffered is lost, so the
    /// prelude flushes first.
    pub fn net_shutdown_write(&mut self, h: Handle) -> StreamResult<()> {
        let TcpParts { sock, tls, failed, .. } = self.tcp(h, "shutdown-output")?;
        if failed.is_some() {
            return Ok(());
        }
        if let Some(conn) = tls {
            // TLS has its own end-of-stream message, and the peer needs it to
            // tell a finished request from a truncated one.
            conn.send_close_notify();
            if let Err(e) = tls_write_out(conn, sock) {
                peer_failed(failed, format!("shutdown-output: {}", e));
                return Ok(());
            }
        }
        if let Err(e) = sock.shutdown(std::net::Shutdown::Write) {
            peer_failed(failed, format!("shutdown-output: {}", e));
        }
        Ok(())
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
            Backend::Listener { listen, .. } => listen.local_address(),
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

/// Records the peer's failure — the first one only, since it is the cause
/// and what follows is consequence — and answers as a read does from then
/// on: end of input.
fn peer_failed(failed: &mut Option<String>, what: String) -> usize {
    if failed.is_none() {
        *failed = Some(what);
    }
    0
}

/// [`peer_failed`] for a write: nothing more can be sent, so the flush is
/// done.
fn sent_nothing_more(failed: &mut Option<String>, what: String) -> Option<i64> {
    peer_failed(failed, what);
    None
}

/// Sends whatever ciphertext the connection has queued, without waiting: a
/// socket buffer that is full stops the attempt and leaves the rest queued
/// for the next one. Only a real failure is an error.
fn tls_write_out(conn: &mut rustls::Connection, sock: &mut Sock) -> Result<(), std::io::Error> {
    while conn.wants_write() {
        match conn.write_tls(sock) {
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == ErrorKind::Interrupted => break,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// One non-blocking step of the handshake, for either side: `Ok(None)` when
/// it is complete, `Ok(Some(i))` when the socket has to become ready for
/// `i` (`0` readable, `1` writable) first. What the two callers do with an
/// `Err` differs — `tls-connect` reports it, a flush records it.
fn tls_handshake_step(conn: &mut rustls::Connection, sock: &mut Sock) -> Result<Option<i64>, String> {
    if conn.is_handshaking() {
        if conn.wants_write() {
            match conn.write_tls(sock) {
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(Some(1)),
                Err(e) => return Err(e.to_string()),
            }
        }
        if conn.wants_read() {
            match conn.read_tls(sock) {
                Ok(0) => return Err("the connection closed during the handshake".to_string()),
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(Some(0)),
                Err(e) => return Err(e.to_string()),
            }
            if let Err(e) = conn.process_new_packets() {
                // The alert that names the problem goes to the peer, best effort.
                let _ = tls_write_out(conn, sock);
                return Err(e.to_string());
            }
        }
    }
    if conn.is_handshaking() {
        // More to do, and the socket may already be ready for it: say
        // which side, and let the caller's wait answer at once if so.
        Ok(Some(if conn.wants_write() { 1 } else { 0 }))
    } else {
        // The message that completes a side's handshake (the client's
        // `Finished`) is queued by the step that processed the peer's last
        // one, so it is still to send: out it goes, best effort — it is
        // small, and a socket that cannot take it now takes it with the
        // first flush or fill.
        tls_write_out(conn, sock).map_err(|e| e.to_string())?;
        Ok(None)
    }
}

/// The certificates in a PEM file — at least one, or an error naming the
/// file.
fn pem_certificates(file: &str) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, String> {
    use rustls::pki_types::pem::PemObject;
    let certs = rustls::pki_types::CertificateDer::pem_file_iter(file)
        .map_err(|e| format!("{}: {}", file, e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{}: {}", file, e))?;
    if certs.is_empty() {
        return Err(format!("{}: no certificate in the file", file));
    }
    Ok(certs)
}

/// A root store of the certificates in a PEM file.
fn pem_roots(file: &str) -> Result<rustls::RootCertStore, String> {
    let mut roots = rustls::RootCertStore::empty();
    let (added, _) = roots.add_parsable_certificates(pem_certificates(file)?);
    if added == 0 {
        return Err(format!("{}: no usable certificate in the file", file));
    }
    Ok(roots)
}

/// A client configuration. The one every plain `tls-connect` shares —
/// Mozilla's root store (bundled by `webpki-roots`), no client
/// certificate — is built once; anything with a private CA or an identity
/// is built per connect, since those are rare and their files may change.
fn tls_client_config(ca_file: Option<&str>, identity: Option<(&str, &str)>) -> Result<Arc<rustls::ClientConfig>, String> {
    static DEFAULT: std::sync::OnceLock<Arc<rustls::ClientConfig>> = std::sync::OnceLock::new();
    if ca_file.is_none() && identity.is_none() {
        return Ok(Arc::clone(DEFAULT.get_or_init(|| {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            Arc::new(rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
        })));
    }
    let roots = match ca_file {
        Some(file) => pem_roots(file)?,
        None => {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            roots
        }
    };
    let builder = rustls::ClientConfig::builder().with_root_certificates(roots);
    let config = match identity {
        None => builder.with_no_client_auth(),
        Some((cert_file, key_file)) => {
            use rustls::pki_types::pem::PemObject;
            let certs = pem_certificates(cert_file)?;
            let key = rustls::pki_types::PrivateKeyDer::from_pem_file(key_file).map_err(|e| format!("{}: {}", key_file, e))?;
            builder.with_client_auth_cert(certs, key).map_err(|e| format!("{}: {}", cert_file, e))?
        }
    };
    Ok(Arc::new(config))
}

/// The chain in `cert_file` with the key in `key_file`, checked to match.
fn certified_key(cert_file: &str, key_file: &str) -> Result<rustls::sign::CertifiedKey, String> {
    use rustls::pki_types::pem::PemObject;
    static PROVIDER: std::sync::OnceLock<rustls::crypto::CryptoProvider> = std::sync::OnceLock::new();
    let provider = PROVIDER.get_or_init(rustls::crypto::ring::default_provider);
    let certs = pem_certificates(cert_file)?;
    let key = rustls::pki_types::PrivateKeyDer::from_pem_file(key_file).map_err(|e| format!("{}: {}", key_file, e))?;
    rustls::sign::CertifiedKey::from_der(certs, key, provider).map_err(|e| format!("{}: {}", cert_file, e))
}

/// A server configuration: the chain in `cert_file`, the key in `key_file`
/// (the default certificate of a resolver more can be added to), and with
/// `client_ca` a verifier that requires every client to present a
/// certificate issued by one in that file.
fn tls_server_config(cert_file: &str, key_file: &str, client_ca: Option<&str>) -> Result<TlsServer, String> {
    let certs = Arc::new(SniCerts { default: Arc::new(certified_key(cert_file, key_file)?), by_name: Mutex::new(HashMap::new()) });
    let builder = match client_ca {
        None => rustls::ServerConfig::builder().with_no_client_auth(),
        Some(file) => {
            let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(pem_roots(file)?))
                .build()
                .map_err(|e| format!("{}: {}", file, e))?;
            rustls::ServerConfig::builder().with_client_cert_verifier(verifier)
        }
    };
    let config = Arc::new(builder.with_cert_resolver(Arc::clone(&certs) as Arc<dyn rustls::server::ResolvesServerCert>));
    Ok(TlsServer { config, certs })
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
        assert_eq!(t.net_flush(c).unwrap(), None);
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
        assert_eq!(t.net_flush(c).unwrap(), None);
        fill_until_some(&mut t, s);
        assert_eq!(t.net_pop_char(s).unwrap(), None);
        t.net_push_byte(c, 0xA9).unwrap();
        assert_eq!(t.net_flush(c).unwrap(), None);
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
        assert_eq!(t.net_flush(c).unwrap(), None);
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

    /// A small PKI in PEM files: a CA, a server certificate for `localhost`
    /// it issued, and a client certificate it issued. The paths, as strings
    /// the builtins take.
    struct Pki {
        ca: String,
        cert: String,
        key: String,
        client_cert: String,
        client_key: String,
        alt_cert: String,
        alt_key: String,
    }

    fn test_pki() -> Pki {
        use rcgen::{BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair};
        let dir = std::env::temp_dir().join(format!("typelisp-tls-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_key = KeyPair::generate().unwrap();
        let ca_cert = ca_params.self_signed(&ca_key).unwrap();
        let issuer = rcgen::Issuer::from_params(&ca_params, &ca_key);
        let mut server = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        server.distinguished_name = rcgen::DistinguishedName::new();
        server.distinguished_name.push(rcgen::DnType::CommonName, "localhost");
        let server_key = KeyPair::generate().unwrap();
        let server_cert = server.signed_by(&server_key, &issuer).unwrap();
        let mut client = CertificateParams::new(vec!["client".to_string()]).unwrap();
        client.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        client.distinguished_name = rcgen::DistinguishedName::new();
        client.distinguished_name.push(rcgen::DnType::OrganizationName, "Example");
        client.distinguished_name.push(rcgen::DnType::CommonName, "client");
        let client_key = KeyPair::generate().unwrap();
        let client_cert = client.signed_by(&client_key, &issuer).unwrap();
        // A second server certificate, for another name (SNI).
        let mut alt = CertificateParams::new(vec!["alt.test".to_string()]).unwrap();
        alt.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        alt.distinguished_name = rcgen::DistinguishedName::new();
        alt.distinguished_name.push(rcgen::DnType::CommonName, "alt.test");
        let alt_key = KeyPair::generate().unwrap();
        let alt_cert = alt.signed_by(&alt_key, &issuer).unwrap();
        let write = |name: &str, text: String| {
            let path = dir.join(name);
            std::fs::write(&path, text).unwrap();
            path.to_string_lossy().into_owned()
        };
        Pki {
            ca: write("ca.pem", ca_cert.pem()),
            cert: write("cert.pem", server_cert.pem()),
            key: write("key.pem", server_key.serialize_pem()),
            client_cert: write("client.pem", client_cert.pem()),
            client_key: write("client-key.pem", client_key.serialize_pem()),
            alt_cert: write("alt.pem", alt_cert.pem()),
            alt_key: write("alt-key.pem", alt_key.serialize_pem()),
        }
    }

    /// Runs both sides of a TLS handshake over the loopback: the client
    /// steps `net_tls_handshake`, the server's side is driven by nothing
    /// but `net_fill` — as a real server task's first read would. `trust`
    /// is the client's CA file (none: the public roots), `identity` its
    /// certificate and key, `client_ca` what the server requires.
    fn tls_pair(
        t: &mut StreamTable,
        pki: &Pki,
        trust: Option<&str>,
        identity: Option<(&str, &str)>,
        client_ca: Option<&str>,
    ) -> (Result<Handle, String>, Handle) {
        let l = t.net_tls_listen("127.0.0.1", 0, &pki.cert, &pki.key, client_ca).unwrap();
        tls_pair_on(t, l, pki, "localhost", trust, identity)
    }

    fn tls_pair_on(
        t: &mut StreamTable,
        l: Handle,
        _pki: &Pki,
        server_name: &str,
        trust: Option<&str>,
        identity: Option<(&str, &str)>,
    ) -> (Result<Handle, String>, Handle) {
        let addr = t.net_local_address(l).unwrap();
        let c = t.net_connect_begin(&addr).unwrap();
        let cfd = t.raw_fd(c).unwrap();
        crate::os::poll_ready(&[(cfd, crate::os::Interest::Writable)], Some(std::time::Duration::from_secs(5))).unwrap();
        t.net_connect_finish(c).unwrap();
        t.net_tls_start(c, server_name, trust, identity.map(|i| i.0), identity.map(|i| i.1)).unwrap();
        let lfd = t.raw_fd(l).unwrap();
        crate::os::poll_ready(&[(lfd, crate::os::Interest::Readable)], Some(std::time::Duration::from_secs(5))).unwrap();
        let s = t.net_accept(l).unwrap().expect("a connection was waiting");
        let sfd = t.raw_fd(s).unwrap();
        let client = loop {
            match t.net_tls_handshake(c) {
                Ok(None) => break Ok(c),
                Err(e) => break Err(e),
                Ok(Some(i)) => {
                    let interest = if i == 1 { crate::os::Interest::Writable } else { crate::os::Interest::Readable };
                    crate::os::poll_ready(&[(cfd, interest)], Some(std::time::Duration::from_millis(200))).unwrap();
                }
            }
            // The server's turn, the way its task would take it: a read
            // that finds a handshake record answers "again", not data.
            crate::os::poll_ready(&[(sfd, crate::os::Interest::Readable)], Some(std::time::Duration::from_millis(200))).unwrap();
            let _ = t.net_fill(s).unwrap();
        };
        (client, s)
    }

    #[test]
    fn a_line_crosses_a_tls_connection_both_ways() {
        let mut t = StreamTable::default();
        let pki = test_pki();
        let (c, s) = tls_pair(&mut t, &pki, Some(&pki.ca), None, None);
        let c = c.expect("the client trusts the CA it was given");
        t.net_push_string(c, "over tls\n").unwrap();
        assert_eq!(t.net_flush(c).unwrap(), None);
        assert!(fill_until_some(&mut t, s) > 0);
        let mut got = String::new();
        while let Some(ch) = t.net_pop_char(s).unwrap() {
            got.push(ch);
        }
        assert_eq!(got, "over tls\n");
        // And back: the server writes after its handshake completed in
        // the reads above.
        t.net_push_string(s, "and back\n").unwrap();
        assert_eq!(t.net_flush(s).unwrap(), None);
        assert!(fill_until_some(&mut t, c) > 0);
        let mut got = String::new();
        while let Some(ch) = t.net_pop_char(c).unwrap() {
            got.push(ch);
        }
        assert_eq!(got, "and back\n");
        assert_eq!(t.net_socket_error(s).unwrap(), None);
        assert_eq!(t.net_socket_error(c).unwrap(), None);
    }

    #[test]
    fn a_server_writing_first_completes_the_handshake_in_its_flush() {
        // The server's first operation is a write (a greeting): the flush
        // must step the handshake, asking for *readable* while it waits on
        // the client, and send the greeting once it is done.
        let mut t = StreamTable::default();
        let pki = test_pki();
        let l = t.net_tls_listen("127.0.0.1", 0, &pki.cert, &pki.key, None).unwrap();
        let addr = t.net_local_address(l).unwrap();
        let c = t.net_connect_begin(&addr).unwrap();
        let cfd = t.raw_fd(c).unwrap();
        crate::os::poll_ready(&[(cfd, crate::os::Interest::Writable)], Some(std::time::Duration::from_secs(5))).unwrap();
        t.net_connect_finish(c).unwrap();
        t.net_tls_start(c, "localhost", Some(&pki.ca), None, None).unwrap();
        let lfd = t.raw_fd(l).unwrap();
        crate::os::poll_ready(&[(lfd, crate::os::Interest::Readable)], Some(std::time::Duration::from_secs(5))).unwrap();
        let s = t.net_accept(l).unwrap().unwrap();
        let sfd = t.raw_fd(s).unwrap();
        t.net_push_string(s, "220 hello\n").unwrap();
        let mut asked_readable = false;
        let mut client_done = false;
        for _ in 0..50 {
            match t.net_flush(s).unwrap() {
                None => break,
                Some(0) => {
                    asked_readable = true;
                    crate::os::poll_ready(&[(sfd, crate::os::Interest::Readable)], Some(std::time::Duration::from_millis(200))).unwrap();
                }
                Some(_) => {
                    crate::os::poll_ready(&[(sfd, crate::os::Interest::Writable)], Some(std::time::Duration::from_millis(200))).unwrap();
                }
            }
            if !client_done {
                match t.net_tls_handshake(c).unwrap() {
                    None => client_done = true,
                    Some(i) => {
                        let interest = if i == 1 { crate::os::Interest::Writable } else { crate::os::Interest::Readable };
                        crate::os::poll_ready(&[(cfd, interest)], Some(std::time::Duration::from_millis(200))).unwrap();
                    }
                }
            }
        }
        assert!(asked_readable, "the flush never had to wait for the client's handshake message");
        assert_eq!(t.net_flush(s).unwrap(), None);
        assert!(fill_until_some(&mut t, c) > 0);
        let mut got = String::new();
        while let Some(ch) = t.net_pop_char(c).unwrap() {
            got.push(ch);
        }
        assert_eq!(got, "220 hello\n");
    }

    #[test]
    fn an_untrusted_certificate_fails_the_client_and_only_marks_the_server() {
        let mut t = StreamTable::default();
        let pki = test_pki();
        // The client trusts the public roots, which did not sign this one.
        let (c, s) = tls_pair(&mut t, &pki, None, None, None);
        let err = c.expect_err("a self-signed certificate was accepted against the public roots");
        assert!(err.starts_with("tls-connect:"), "{}", err);
        // The server side: its reads say end of input, its writes go
        // nowhere, and neither is an error — a task serving this connection
        // ends normally, and `socket-error` says why.
        assert_eq!(fill_until_some(&mut t, s), 0);
        t.net_push_string(s, "unheard\n").unwrap();
        assert_eq!(t.net_flush(s).unwrap(), None);
        let why = t.net_socket_error(s).unwrap().expect("the server did not record the client's alert");
        assert!(why.contains("tls") || why.contains("read"), "{}", why);
    }

    #[test]
    fn a_peer_that_resets_marks_the_socket_instead_of_failing() {
        // The client goes away; the server's write cannot be delivered.
        // What the server sees is recorded, not raised.
        let mut t = StreamTable::default();
        let (c, s) = connected_pair(&mut t);
        t.close(c).unwrap();
        assert_eq!(fill_until_some(&mut t, s), 0);
        let sfd = t.raw_fd(s).unwrap();
        // The first write may still succeed (the kernel accepts it and
        // learns of the reset afterwards); one of a few will not.
        let mut failed = None;
        for _ in 0..10 {
            t.net_push_string(s, "into the void\n").unwrap();
            let _ = t.net_flush(s).unwrap();
            if let Some(why) = t.net_socket_error(s).unwrap() {
                failed = Some(why);
                break;
            }
            crate::os::poll_ready(&[(sfd, crate::os::Interest::Writable)], Some(std::time::Duration::from_millis(50))).unwrap();
        }
        let why = failed.expect("writing to a closed peer was never noticed");
        assert!(why.starts_with("write:"), "{}", why);
        assert_eq!(t.net_flush(s).unwrap(), None);
        assert_eq!(t.net_fill(s).unwrap(), Some(0));
        t.net_shutdown_write(s).unwrap();
    }

    #[test]
    fn a_client_certificate_is_required_when_the_listener_names_a_ca() {
        let mut t = StreamTable::default();
        let pki = test_pki();
        // With one: the handshake completes and a line crosses.
        let (c, s) = tls_pair(&mut t, &pki, Some(&pki.ca), Some((&pki.client_cert, &pki.client_key)), Some(&pki.ca));
        let c = c.expect("a client certificate the CA issued was refused");
        t.net_push_string(c, "authenticated\n").unwrap();
        assert_eq!(t.net_flush(c).unwrap(), None);
        assert!(fill_until_some(&mut t, s) > 0);
        assert_eq!(t.net_socket_error(s).unwrap(), None);
        // Without one: the server ends the handshake; the client sees it
        // as an error of `tls-connect`, the server as a marked socket.
        let (c, s) = tls_pair(&mut t, &pki, Some(&pki.ca), None, Some(&pki.ca));
        // In TLS 1.3 the client's side of the handshake completes before
        // the server has seen its (empty) certificate, so the refusal is
        // not `tls-connect`'s to see: the server's first read is what
        // processes the client's last flight and answers with the alert.
        assert_eq!(fill_until_some(&mut t, s), 0);
        assert!(t.net_socket_error(s).unwrap().is_some(), "the server accepted a client without a certificate");
        let refused = match c {
            Err(_) => true,
            Ok(c) => fill_until_some(&mut t, c) == 0 && t.net_socket_error(c).unwrap().is_some(),
        };
        assert!(refused, "a client without a certificate got through");
    }

    #[test]
    fn the_peer_subject_is_the_certificates_name() {
        let mut t = StreamTable::default();
        let pki = test_pki();
        let (c, s) = tls_pair(&mut t, &pki, Some(&pki.ca), Some((&pki.client_cert, &pki.client_key)), Some(&pki.ca));
        let c = c.unwrap();
        // The client sees the server's certificate after its handshake; the
        // server sees the client's once its first read has processed it.
        assert_eq!(t.net_peer_subject(c).unwrap().as_deref(), Some("CN=localhost"));
        t.net_push_string(c, "hi\n").unwrap();
        assert_eq!(t.net_flush(c).unwrap(), None);
        assert!(fill_until_some(&mut t, s) > 0);
        assert_eq!(t.net_peer_subject(s).unwrap().as_deref(), Some("CN=client,O=Example"));
        assert_eq!(t.net_server_name(s).unwrap().as_deref(), Some("localhost"));
        assert_eq!(t.net_server_name(c).unwrap(), None);
        // A plain connection has neither.
        let (p, _) = connected_pair(&mut t);
        assert_eq!(t.net_peer_subject(p).unwrap(), None);
        assert_eq!(t.net_server_name(p).unwrap(), None);
    }

    #[test]
    fn a_listener_serves_a_second_name_with_its_own_certificate() {
        let mut t = StreamTable::default();
        let pki = test_pki();
        let l = t.net_tls_listen("127.0.0.1", 0, &pki.cert, &pki.key, None).unwrap();
        // The wrong file for the name is refused up front.
        let err = t.net_tls_add_certificate(l, "alt.test", &pki.cert, &pki.key).unwrap_err();
        assert!(err.contains("not for alt.test"), "{}", err);
        t.net_tls_add_certificate(l, "alt.test", &pki.alt_cert, &pki.alt_key).unwrap();
        // Asking for alt.test gets alt's certificate...
        let (c, s) = tls_pair_on(&mut t, l, &pki, "alt.test", Some(&pki.ca), None);
        let c = c.expect("the certificate added for alt.test was not presented");
        assert_eq!(t.net_peer_subject(c).unwrap().as_deref(), Some("CN=alt.test"));
        let _ = s;
        // ...and localhost still gets the listener's own.
        let (c, _) = tls_pair_on(&mut t, l, &pki, "localhost", Some(&pki.ca), None);
        assert_eq!(t.net_peer_subject(c.unwrap()).unwrap().as_deref(), Some("CN=localhost"));
        // A plain listener has nothing to add to.
        let plain = t.net_listen("127.0.0.1", 0).unwrap();
        assert!(t.net_tls_add_certificate(plain, "alt.test", &pki.alt_cert, &pki.alt_key).is_err());
    }

    #[test]
    fn socket_options_are_set_on_tcp_and_refused_where_meaningless() {
        let mut t = StreamTable::default();
        let (c, _) = connected_pair(&mut t);
        t.net_set_nodelay(c, true).unwrap();
        t.net_set_keepalive(c, true).unwrap();
        t.net_set_keepalive_period(c, 15).unwrap();
        assert!(t.net_set_keepalive_period(c, 0).is_err());
        let dir = std::env::temp_dir().join(format!("typelisp-unix-opts-{}", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let path = dir.to_string_lossy().into_owned();
        let l = t.net_unix_listen(&path).unwrap();
        let u = t.net_unix_connect(&path).unwrap();
        t.net_set_nodelay(u, true).unwrap();
        assert!(t.net_set_keepalive(u, true).is_err());
        t.close(l).unwrap();
    }

    #[test]
    fn tls_listen_checks_its_files_up_front() {
        let mut t = StreamTable::default();
        let pki = test_pki();
        let err = t.net_tls_listen("127.0.0.1", 0, &pki.key, &pki.cert, None).unwrap_err();
        assert!(err.starts_with("tls-listen:"), "{}", err);
        let err = t.net_tls_listen("127.0.0.1", 0, "/no/such/cert.pem", &pki.key, None).unwrap_err();
        assert!(err.contains("/no/such/cert.pem"), "{}", err);
        let err = t.net_tls_listen("127.0.0.1", 0, &pki.cert, &pki.key, Some("/no/such/ca.pem")).unwrap_err();
        assert!(err.contains("/no/such/ca.pem"), "{}", err);
        let (c, s) = connected_pair(&mut t);
        let _ = s;
        let err = t.net_tls_start(c, "localhost", None, Some(&pki.client_cert), None).unwrap_err();
        assert!(err.contains("both"), "{}", err);
    }

    #[test]
    fn tls_refuses_a_peer_that_speaks_no_tls() {
        // A plain echo peer answers the ClientHello with garbage: the
        // handshake must fail with an error, not hang or succeed.
        let mut t = StreamTable::default();
        let (c, s) = connected_pair(&mut t);
        t.net_tls_start(c, "localhost", None, None, None).unwrap();
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
