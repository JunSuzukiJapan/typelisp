//! TLS: the certificates a server chooses among, building the client and server
//! configurations from PEM files, and moving a connection's handshake and
//! records over its socket.

use typelisp_mem::prelude::*;
use core::any::Any;
use typelisp_mem::collections::HashMap;
use std::io::ErrorKind;
use std::sync::{Arc, Mutex};
use crate::stream::Sock;

/// A connection's TLS state as its stream entry holds it: a
/// `rustls::Connection` in a box that names no type. Every stream entry is
/// dropped and closed through code every program with a stream table
/// reaches (`Backend`'s drop, `StreamTable::close`); rustls's own type or
/// methods there would make each executable carry rustls. Through the box
/// they are reached only from where a connection is made ([`TlsConn::new`]),
/// which is what builds the box's vtable.
pub(crate) struct TlsConn(Box<dyn Session>);

/// What the code every stream reaches does with a TLS connection.
trait Session: Any + Send {
    /// Tells the peer the connection is closing (`close_notify`), best
    /// effort and without waiting.
    fn close(&mut self, sock: &mut Sock);
}

impl Session for rustls::Connection {
    fn close(&mut self, sock: &mut Sock) {
        self.send_close_notify();
        let _ = self.write_tls(sock);
    }
}

impl TlsConn {
    pub(super) fn new(conn: rustls::Connection) -> Self {
        TlsConn(Box::new(conn))
    }

    pub(super) fn get(&mut self) -> &mut rustls::Connection {
        (&mut *self.0 as &mut dyn Any).downcast_mut().expect("a `TlsConn` holds a `rustls::Connection`")
    }

    pub(super) fn peek(&self) -> &rustls::Connection {
        (&*self.0 as &dyn Any).downcast_ref().expect("a `TlsConn` holds a `rustls::Connection`")
    }

    /// [`Session::close`].
    pub(crate) fn close(&mut self, sock: &mut Sock) {
        self.0.close(sock);
    }
}

/// What a `tls-listen` listener holds, in a box that names no type for the
/// reason [`TlsConn`] gives: its [`ServerState`].
pub(crate) struct TlsServer(Box<dyn Any + Send>);

impl TlsServer {
    pub(super) fn state(&self) -> &ServerState {
        self.0.downcast_ref().expect("a `TlsServer` holds a `ServerState`")
    }
}

/// The configuration every connection a `tls-listen` listener accepts
/// shares, and the certificates it chooses among — kept apart from the
/// configuration so that `tls-add-certificate` can add one without
/// rebuilding it.
pub(super) struct ServerState {
    pub(super) config: Arc<rustls::ServerConfig>,
    certs: Arc<SniCerts>,
}

impl ServerState {
    /// Serves `key` to a client that asks for `name` (lower-cased), from the
    /// next handshake on.
    pub(super) fn add_certificate(&self, name: String, key: rustls::sign::CertifiedKey) -> Result<(), String> {
        let mut table = self.certs.by_name.lock().map_err(|_| "the certificate table is poisoned".to_string())?;
        table.insert(name, Arc::new(key));
        Ok(())
    }
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

/// Sends whatever ciphertext the connection has queued, without waiting: a
/// socket buffer that is full stops the attempt and leaves the rest queued
/// for the next one. Only a real failure is an error.
pub(super) fn tls_write_out(conn: &mut rustls::Connection, sock: &mut Sock) -> Result<(), std::io::Error> {
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
pub(super) fn tls_handshake_step(conn: &mut rustls::Connection, sock: &mut Sock) -> Result<Option<i64>, String> {
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
pub(super) fn tls_client_config(ca_file: Option<&str>, identity: Option<(&str, &str)>) -> Result<Arc<rustls::ClientConfig>, String> {
    static DEFAULT: typelisp_mem::sync::OnceLock<Arc<rustls::ClientConfig>> = typelisp_mem::sync::OnceLock::new();
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
pub(super) fn certified_key(cert_file: &str, key_file: &str) -> Result<rustls::sign::CertifiedKey, String> {
    use rustls::pki_types::pem::PemObject;
    static PROVIDER: typelisp_mem::sync::OnceLock<rustls::crypto::CryptoProvider> = typelisp_mem::sync::OnceLock::new();
    let provider = PROVIDER.get_or_init(rustls::crypto::ring::default_provider);
    let certs = pem_certificates(cert_file)?;
    let key = rustls::pki_types::PrivateKeyDer::from_pem_file(key_file).map_err(|e| format!("{}: {}", key_file, e))?;
    rustls::sign::CertifiedKey::from_der(certs, key, provider).map_err(|e| format!("{}: {}", cert_file, e))
}

/// A server configuration: the chain in `cert_file`, the key in `key_file`
/// (the default certificate of a resolver more can be added to), and with
/// `client_ca` a verifier that requires every client to present a
/// certificate issued by one in that file.
pub(super) fn tls_server_config(cert_file: &str, key_file: &str, client_ca: Option<&str>) -> Result<TlsServer, String> {
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
    Ok(TlsServer(Box::new(ServerState { config, certs })))
}
