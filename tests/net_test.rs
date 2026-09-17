//! TCP sockets — `tcp-connect`/`tcp-listen`/`accept` and the two stream
//! views over a connection.
//!
//! Everything here runs on the loopback interface, on a port the OS picks
//! (`(tcp-listen "127.0.0.1" 0)` + `local-address`), so nothing depends on
//! the machine or the network. What the tests check beyond "bytes cross":
//! that a task waiting on a socket **parks** rather than stopping the
//! thread — the property every socket operation is built around
//! (`typelisp_rt::net`'s module docs) — and that an interpreted turn of the
//! prelude's wait loop and a compiled one see the same answers.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run_with(src: &str, with_compiler: bool, stress: bool) -> Result<(Heap, Value), EvalError> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    if with_compiler {
        typelisp::compile::install_llvm_backend();
        load_compiler(&mut h, &mut chk, &mut interp);
    }
    h.set_gc_stress(stress);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok((h, last))
}

fn text(src: &str) -> String {
    match run_with(src, false, false).expect("eval failed") {
        (h, Value::Str(id)) => h.string(id).to_string(),
        (_, other) => panic!("expected a string, got {:?}", other),
    }
}

fn text_stressed(src: &str) -> String {
    match run_with(src, false, true).expect("eval failed") {
        (h, Value::Str(id)) => h.string(id).to_string(),
        (_, other) => panic!("expected a string, got {:?}", other),
    }
}

fn text_compiled(src: &str) -> String {
    match run_with(src, true, false).expect("eval failed") {
        (h, Value::Str(id)) => h.string(id).to_string(),
        (_, other) => panic!("expected a string, got {:?}", other),
    }
}

fn err(src: &str) -> String {
    match run_with(src, false, false) {
        Err(e) => e.to_string(),
        Ok((_, v)) => panic!("expected an error, got {:?}", v),
    }
}

/// A listener on a free loopback port, and its port number.
const LISTEN: &str = r#"
(defun port-of ((l socket-listener)) int
  (let ((addr (local-address l)))
    (unwrap (parse-int (substring addr (+ 1 (unwrap (search addr ":"))) (length addr))))))
"#;

/// An echo server: one task per connection, each echoing lines until the
/// client closes.
const ECHO: &str = r#"
(defun echo-conn ((c socket-stream)) ()
  (loop
    (match (read-line c)
      ((some line) (write-line c line))
      ((none) (break))))
  (close c))

(defun echo-serve ((l socket-listener) (n int)) ()
  (dotimes (i n)
    (match (accept l)
      ((ok c) (go (echo-conn c)))
      ((err e) (panic (message e))))))
"#;

#[test]
fn a_line_is_echoed_over_the_loopback() {
    let src = format!(
        "{LISTEN}{ECHO}
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (echo-serve l 1))
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" (port-of l)))))
    (write-line c \"hello, socket\")
    (let ((reply (unwrap (read-line c))))
      (close c)
      (close l)
      reply)))"
    );
    assert_eq!(text(&src), "hello, socket");
    assert_eq!(text_stressed(&src), "hello, socket");
}

#[test]
fn the_compiled_wait_loop_sees_the_same_answers() {
    let src = format!(
        "{LISTEN}{ECHO}
(defun round-trip ((port int)) string
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" port))))
    (write-line c \"compiled hello\")
    (let ((reply (unwrap (read-line c))))
      (close c)
      reply)))
(compile round-trip)
(compile echo-conn)
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (echo-serve l 1))
  (let ((r (round-trip (port-of l))))
    (close l)
    r))"
    );
    assert_eq!(text_compiled(&src), "compiled hello");
}

#[test]
fn a_task_waiting_on_accept_does_not_stop_the_others() {
    // While `main` sits in `accept` with nobody connecting, a sleeping task
    // must keep waking. The counter it advances is the evidence; the
    // connection that finally arrives is made by a third task after it.
    let src = format!(
        "{LISTEN}
(defvar (ticks int) 0)
(defun tick ((n int)) ()
  (dotimes (i n) (sleep 0.005) (setf ticks (+ ticks 1))))
(defun connect-later ((port int)) ()
  (sleep 0.05)
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" port))))
    (close c)))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (tick 5))
  (go (connect-later (port-of l)))
  (let ((c (unwrap (accept l))))
    (close c)
    (close l)
    (format false \"~a\" (>= ticks 5))))"
    );
    assert_eq!(text(&src), "true");
}

#[test]
fn a_refused_connection_is_an_error_value() {
    let src = format!(
        "{LISTEN}
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (let ((port (port-of l)))
    (close l)
    (match (tcp-connect \"127.0.0.1\" port)
      ((ok c) (progn (close c) \"connected?!\"))
      ((err e) (let ((m (message e))) (if (> (length m) 0) \"refused\" \"empty message\"))))))"
    );
    assert_eq!(text(&src), "refused");
}

#[test]
fn a_net_error_is_an_error_trait_object() {
    let src = r#"
(defun try-it () Result<socket-stream, :dyn Error>
  (as-dyn-error (tcp-connect "127.0.0.1" 1)))
(match (try-it)
  ((ok _) "no error")
  ((err e) (if (> (length (message e)) 0) "an Error" "")))"#;
    assert_eq!(text(src), "an Error");
}

#[test]
fn end_of_input_when_the_peer_closes_and_half_close() {
    // The client sends two lines and shuts its output; the server reads
    // until `none`, then answers with the count — which the client can still
    // read after `shutdown-output`.
    let src = format!(
        "{LISTEN}
(defun count-lines ((c socket-stream)) ()
  (let ((n 0))
    (loop (match (read-line c) ((some _) (setf n (+ n 1))) ((none) (break))))
    (write-line c (format false \"~a lines\" n))
    (close c)))
(defun serve-one ((l socket-listener)) ()
  (match (accept l) ((ok c) (count-lines c)) ((err e) (panic (message e)))))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (serve-one l))
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" (port-of l)))))
    (write-line c \"one\")
    (write-line c \"two\")
    (shutdown-output c)
    (let ((reply (unwrap (read-line c))))
      (let ((after (read-line c)))
        (close c)
        (close l)
        (format false \"~a / ~a\" reply (is-none after))))))"
    );
    assert_eq!(text(&src), "2 lines / true");
}

#[test]
fn the_byte_view_shares_the_connection() {
    let src = format!(
        "{LISTEN}
(defun send-bytes ((l socket-listener)) ()
  (match (accept l)
    ((ok c)
     (write-line c \"header\")
     (let ((b (byte-stream-of c)))
       (write-byte b 200)
       (write-byte b 10)
       (close b)))
    ((err e) (panic (message e)))))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (send-bytes l))
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" (port-of l)))))
    (let ((h (unwrap (read-line c)))
          (b (byte-stream-of c)))
      (let ((x (unwrap (read-byte b))) (y (unwrap (read-byte b))) (z (read-byte b)))
        (close c)
        (close l)
        (format false \"~a ~a ~a ~a\" h x y (is-none z))))))"
    );
    assert_eq!(text(&src), "header 200 10 true");
}

#[test]
fn a_pushed_back_character_refuses_a_byte_read() {
    let src = format!(
        "{LISTEN}
(defun send-text ((l socket-listener)) ()
  (match (accept l)
    ((ok c) (write-string c \"ab\") (close c))
    ((err e) (panic (message e)))))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (send-text l))
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" (port-of l)))))
    (let ((a (unwrap (read-char c))))
      (unread-char c a)
      (read-byte (byte-stream-of c)))))"
    );
    let e = err(&src);
    assert!(e.contains("unread character"), "{}", e);
}

#[test]
fn waiting_on_a_socket_from_a_print_method_is_refused() {
    // A `print-object` runs on a Rust frame with no continuation stack under
    // it, so a wait that cannot be answered at once is an error — the same
    // rule `sleep` and `recv` follow.
    let src = format!(
        "{LISTEN}
(defstruct peeker (l socket-listener))
(impl print-object peeker
  (print-object ((self Self) (escape bool)) string
    (match (accept self::l)
      ((ok c) (progn (close c) \"accepted\"))
      ((err e) (panic (message e))))))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (format false \"~a\" (peeker::new l)))"
    );
    let e = err(&src);
    assert!(e.contains("cannot block"), "{}", e);
}

#[test]
fn accepting_on_a_closed_listener_is_an_error_value() {
    let src = r#"
(let ((l (unwrap (tcp-listen "127.0.0.1" 0))))
  (close l)
  (match (accept l)
    ((ok c) (progn (close c) "accepted?!"))
    ((err e) (message e))))"#;
    let got = text(src);
    assert!(got.contains("closed"), "{}", got);
}

#[test]
fn waiting_on_a_closed_handle_is_a_panic() {
    // `net-wait` itself, below the prelude's loops: a handle that is closed
    // has nothing to wait on, and parking a task there would hang it.
    let src = r#"
(let ((h (unwrap (net-listen "127.0.0.1" 0))))
  (unwrap (stream-close h))
  (net-wait h 0))"#;
    let e = err(src);
    assert!(e.contains("closed"), "{}", e);
}

#[test]
fn several_clients_are_served_at_once() {
    // Three clients, three server tasks, all parked on their own sockets at
    // some point: `poll` has to wake the right one each time.
    let src = format!(
        "{LISTEN}{ECHO}
(defun client ((port int) (name string)) string
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" port))))
    (write-line c name)
    (let ((reply (unwrap (read-line c))))
      (close c)
      reply)))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (echo-serve l 3))
  (let ((port (port-of l)))
    (let ((a (go (client port \"a\")))
          (b (go (client port \"b\")))
          (c (go (client port \"c\"))))
      (let ((r (format false \"~a~a~a\" (wait a) (wait b) (wait c))))
        (close l)
        r))))"
    );
    assert_eq!(text(&src), "abc");
    assert_eq!(text_compiled(&format!("{src}").replace("(go (echo-serve l 3))", "(compile echo-conn) (go (echo-serve l 3))")), "abc");
}

// ---- timeouts, names, UDP, TLS ---------------------------------------------

#[test]
fn accept_with_a_timeout_gives_up() {
    let src = r#"
(let ((l (unwrap (tcp-listen "127.0.0.1" 0))))
  (match (accept l 0.05)
    ((ok c) (progn (close c) "accepted?!"))
    ((err e) (message e))))"#;
    assert_eq!(text(src), "accept: timed out");
}

#[test]
fn a_timed_wait_answers_true_when_data_arrives_first() {
    let src = format!(
        "{LISTEN}
(defun send-late ((l socket-listener)) ()
  (match (accept l)
    ((ok c) (sleep 0.02) (write-line c \"late\") (close c))
    ((err e) (panic (message e)))))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (send-late l))
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" (port-of l)))))
    (let ((first (wait-readable c 0.005))
          (second (wait-readable c 2.0)))
      (let ((line (unwrap (read-line c))))
        (close c)
        (close l)
        (format false \"~a ~a ~a\" first second line)))))"
    );
    assert_eq!(text(&src), "false true late");
}

#[test]
fn a_compiled_timed_wait_answers_the_same() {
    let src = format!(
        "{LISTEN}
(defun send-late ((l socket-listener)) ()
  (match (accept l)
    ((ok c) (sleep 0.02) (write-line c \"late\") (close c))
    ((err e) (panic (message e)))))
(defun client ((port int)) string
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" port))))
    (let ((first (wait-readable c 0.005))
          (second (wait-readable c 2.0)))
      (let ((line (unwrap (read-line c))))
        (close c)
        (format false \"~a ~a ~a\" first second line)))))
(compile client)
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (send-late l))
  (let ((r (client (port-of l))))
    (close l)
    r))"
    );
    assert_eq!(text_compiled(&src), "false true late");
}

#[test]
fn a_timed_wait_does_not_stop_other_tasks() {
    let src = r#"
(defvar (ticks int) 0)
(defun tick ((n int)) () (dotimes (i n) (sleep 0.005) (setf ticks (+ ticks 1))))
(let ((l (unwrap (tcp-listen "127.0.0.1" 0))))
  (go (tick 5))
  (match (accept l 0.1)
    ((ok c) (progn (close c) "accepted?!"))
    ((err e) (format false "~a ~a" (message e) (>= ticks 5)))))"#;
    assert_eq!(text(src), "accept: timed out true");
}

#[test]
fn connecting_by_name_resolves_on_a_thread() {
    let src = format!(
        "{LISTEN}{ECHO}
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (echo-serve l 1))
  (let ((c (unwrap (tcp-connect \"localhost\" (port-of l)))))
    (write-line c \"by name\")
    (let ((reply (unwrap (read-line c))))
      (close c)
      (close l)
      reply)))"
    );
    assert_eq!(text(&src), "by name");
}

#[test]
fn an_unresolvable_name_is_an_error_value_and_others_keep_running() {
    let src = r#"
(defvar (ticks int) 0)
(defun tick ((n int)) () (dotimes (i n) (sleep 0.005) (setf ticks (+ ticks 1))))
(go (tick 3))
(match (tcp-connect "no-such-host.invalid" 80)
  ((ok c) (progn (close c) "connected?!"))
  ((err e) (if (> (length (message e)) 0) "resolve failed" "")))"#;
    assert_eq!(text(src), "resolve failed");
}

#[test]
fn a_datagram_goes_from_one_socket_to_another() {
    let src = r#"
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))
      (b (unwrap (udp-bind "127.0.0.1" 0))))
  (let ((b-port (unwrap (parse-int (substring (local-address b) (+ 1 (unwrap (search (local-address b) ":"))) (length (local-address b)))))))
    (unwrap (send-to a "127.0.0.1" b-port (string->utf8 "ping é")))
    (let ((d (unwrap (recv-from b 2.0))))
      (let ((back (unwrap (utf8->string (bytes d))))
            (same (equal (from d) (local-address a))))
        (close a)
        (close b)
        (format false "~a ~a" back same)))))"#;
    assert_eq!(text(src), "ping é true");
    assert_eq!(text_stressed(src), "ping é true");
}

#[test]
fn recv_from_with_a_timeout_gives_up() {
    let src = r#"
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (match (recv-from s 0.05)
    ((ok d) "a datagram?!")
    ((err e) (message e))))"#;
    assert_eq!(text(src), "recv-from: timed out");
}

#[test]
fn utf8_round_trips_and_rejects_garbage() {
    let src = r#"
(let ((bytes (string->utf8 "aé€😀")))
  (format false "~a ~a ~a"
    (len bytes)
    (unwrap (utf8->string bytes))
    (is-none (utf8->string (string->utf8-broken)))))
"#;
    let src = src.replace("(string->utf8-broken)", "(let ((v (the Vector<int> (Vector::new)))) (push v 255) v)");
    assert_eq!(text(&src), "10 aé€😀 true");
}

#[test]
fn tls_against_a_plain_peer_fails_cleanly() {
    // A listener that never speaks TLS: the handshake must come back as an
    // `Err`, with every other task still running, not hang the program.
    let src = format!(
        "{LISTEN}
(defun sink ((l socket-listener)) ()
  (match (accept l)
    ((ok c) (write-line c \"not tls\") (close c))
    ((err e) (panic (message e)))))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (sink l))
  (match (tls-connect \"localhost\" (port-of l) 5.0)
    ((ok c) (progn (close c) \"handshake succeeded?!\"))
    ((err e) (if (> (length (message e)) 0) \"tls failed\" \"\"))))"
    );
    assert_eq!(text(&src), "tls failed");
}

#[test]
fn a_unix_domain_socket_is_the_same_stream() {
    // The same echo server, on a path instead of a port: `accept` gives a
    // `socket-stream` and everything after it is family-blind. Closing the
    // listener removes the file.
    let path = std::env::temp_dir().join(format!("typelisp-net-test-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let src = format!(
        "{ECHO}
(let ((l (unwrap (unix-listen \"{p}\"))))
  (go (echo-serve l 1))
  (let ((c (unwrap (unix-connect \"{p}\"))))
    (write-line c \"over unix\")
    (let ((reply (unwrap (read-line c)))
          (who (peer-address c)))
      (close c)
      (close l)
      (format false \"~a / ~a / ~a\" reply who (probe-file \"{p}\")))))",
        p = path.display()
    );
    assert_eq!(text(&src), format!("over unix / {} / false", path.display()));
    let _ = std::fs::remove_file(&path);
    assert_eq!(text_compiled(&src.replace("(go (echo-serve l 1))", "(compile echo-conn) (go (echo-serve l 1))")), format!("over unix / {} / false", path.display()));
}

#[test]
fn connecting_to_a_missing_unix_socket_is_an_error_value() {
    let src = r#"
(match (unix-connect "/nonexistent/typelisp.sock")
  ((ok c) (progn (close c) "connected?!"))
  ((err e) (if (> (length (message e)) 0) "refused" "")))"#;
    assert_eq!(text(src), "refused");
}

// ---- TLS server ------------------------------------------------------------

/// A certificate for `localhost` and its key, as PEM files: what a
/// `tls-listen` names, and what the client's `ca-file` names to trust it.
fn localhost_cert() -> (String, String) {
    let dir = std::env::temp_dir().join(format!("typelisp-net-tls-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");
    std::fs::write(&cert, ck.cert.pem()).unwrap();
    std::fs::write(&key, ck.signing_key.serialize_pem()).unwrap();
    (cert.to_string_lossy().into_owned(), key.to_string_lossy().into_owned())
}

#[test]
fn a_line_is_echoed_over_tls() {
    // The server task's first `read-line` completes the handshake; the
    // client trusts the certificate through `ca-file`. Interpreted and
    // compiled turns of the drain loop must see the same `some interest`.
    let (cert, key) = localhost_cert();
    let src = format!(
        "{LISTEN}{ECHO}
(let ((l (unwrap (tls-listen \"127.0.0.1\" 0 \"{cert}\" \"{key}\"))))
  (go (echo-serve l 1))
  (let ((c (unwrap (tls-connect \"localhost\" (port-of l) 5.0 \"{cert}\"))))
    (write-line c \"hello, tls\")
    (let ((reply (unwrap (read-line c))))
      (close c)
      (close l)
      reply)))"
    );
    assert_eq!(text(&src), "hello, tls");
    assert_eq!(text_compiled(&src.replace("(go (echo-serve l 1))", "(compile echo-conn) (go (echo-serve l 1))")), "hello, tls");
}

#[test]
fn a_server_that_writes_first_shakes_hands_in_its_write() {
    // A greeting before any read: the drain loop must wait for *readable*
    // while the handshake needs the client's next message, and the
    // greeting must arrive once it is done.
    let (cert, key) = localhost_cert();
    let src = format!(
        "{LISTEN}
(defun greet ((l socket-listener)) ()
  (match (accept l)
    ((ok c) (write-line c \"220 ready\") (close c))
    ((err e) (panic (message e)))))
(let ((l (unwrap (tls-listen \"127.0.0.1\" 0 \"{cert}\" \"{key}\"))))
  (go (greet l))
  (let ((c (unwrap (tls-connect \"localhost\" (port-of l) 5.0 \"{cert}\"))))
    (let ((reply (unwrap (read-line c))))
      (close c)
      (close l)
      reply)))"
    );
    assert_eq!(text(&src), "220 ready");
}

#[test]
fn an_untrusted_certificate_is_the_clients_error_and_the_servers_socket_error() {
    // The client, trusting only the public roots, rejects the certificate:
    // `tls-connect` is an `Err`. The server task sees end of input, not a
    // panic, and `socket-error` names the alert -- the server is still
    // there to serve the next client.
    let (cert, key) = localhost_cert();
    let src = format!(
        "{LISTEN}
(defvar (server-saw string) \"\")
(defun serve-one ((l socket-listener)) ()
  (match (accept l)
    ((ok c)
     (let ((line (read-line c)))
       (setf server-saw
             (format false \"~a/~a\" (is-none line)
                     (match (socket-error c) ((some e) (if (> (length (message e)) 0) \"failed\" \"\")) ((none) \"sound\"))))
       (close c)))
    ((err e) (panic (message e)))))
(let ((l (unwrap (tls-listen \"127.0.0.1\" 0 \"{cert}\" \"{key}\"))))
  (go (serve-one l))
  (let ((outcome (match (tls-connect \"localhost\" (port-of l) 5.0)
                   ((ok c) (progn (close c) \"trusted?!\"))
                   ((err e) \"rejected\"))))
    (sleep 0.2)
    (close l)
    (format false \"~a ~a\" outcome server-saw)))"
    );
    assert_eq!(text(&src), "rejected true/failed");
}

#[test]
fn a_peer_that_goes_away_does_not_panic_the_server() {
    // Plain TCP: the client closes without reading; the server's writes
    // after that go nowhere and `socket-error` says so, while a clean end
    // of input on a sound connection reports `none`.
    let src = format!(
        "{LISTEN}
(defvar (seen string) \"\")
(defun talk ((l socket-listener)) ()
  (match (accept l)
    ((ok c)
     (read-line c)
     (let ((i 0))
       (loop
         (write-line c \"chatter chatter chatter chatter chatter chatter chatter chatter\")
         (setf i (+ i 1))
         (when (or (is-some (socket-error c)) (> i 2000)) (return))))
     (setf seen (match (socket-error c) ((some e) \"broken\") ((none) \"sound\")))
     (close c))
    ((err e) (panic (message e)))))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (talk l))
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" (port-of l)))))
    (write-line c \"go\")
    (close c)
    (sleep 0.5)
    (close l)
    seen))"
    );
    assert_eq!(text(&src), "broken");
    assert_eq!(text(&format!(
        "{LISTEN}
(defun serve-one ((l socket-listener)) ()
  (match (accept l)
    ((ok c) (read-line c) (close c))
    ((err e) (panic (message e)))))
(let ((l (unwrap (tcp-listen \"127.0.0.1\" 0))))
  (go (serve-one l))
  (let ((c (unwrap (tcp-connect \"127.0.0.1\" (port-of l)))))
    (write-line c \"bye\")
    (let ((after (read-line c)))
      (let ((r (format false \"~a ~a\" (is-none after) (is-none (socket-error c)))))
        (close c)
        (close l)
        r))))"
    )), "true true");
}

#[test]
fn tls_listen_with_bad_files_is_an_error_value() {
    let (cert, key) = localhost_cert();
    let src = format!(
        "(match (tls-listen \"127.0.0.1\" 0 \"{key}\" \"{cert}\")
  ((ok l) (progn (close l) \"listening?!\"))
  ((err e) (if (> (length (message e)) 0) \"refused\" \"\")))"
    );
    assert_eq!(text(&src), "refused");
}
