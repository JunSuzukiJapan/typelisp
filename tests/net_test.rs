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
(defun port-of ((l tcp-listener)) int
  (let ((addr (local-address l)))
    (unwrap (parse-int (substring addr (+ 1 (unwrap (search addr ":"))) (length addr))))))
"#;

/// An echo server: one task per connection, each echoing lines until the
/// client closes.
const ECHO: &str = r#"
(defun echo-conn ((c tcp-stream)) ()
  (loop
    (match (read-line c)
      ((some line) (write-line c line))
      ((none) (break))))
  (close c))

(defun echo-serve ((l tcp-listener) (n int)) ()
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
(defun try-it () Result<tcp-stream, :dyn Error>
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
(defun count-lines ((c tcp-stream)) ()
  (let ((n 0))
    (loop (match (read-line c) ((some _) (setf n (+ n 1))) ((none) (break))))
    (write-line c (format false \"~a lines\" n))
    (close c)))
(defun serve-one ((l tcp-listener)) ()
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
(defun send-bytes ((l tcp-listener)) ()
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
(defun send-text ((l tcp-listener)) ()
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
(defstruct peeker (l tcp-listener))
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
