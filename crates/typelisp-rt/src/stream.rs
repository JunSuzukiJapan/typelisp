//! The native half of the stream subsystem: the leaf backends that actually
//! touch the OS, addressed by an opaque integer **handle**.
//!
//! Everything a program sees is defined in `src/prelude.rs` on top of this —
//! the `Stream`/`InputStream`/`OutputStream` trait hierarchy, the concrete
//! stream types (ordinary `defstruct`s holding one handle), and *all* of the
//! composite streams (broadcast, two-way, echo, concatenated), which are
//! plain typelisp structs over other streams and need nothing from this
//! module at all.
//!
//! # Why a handle and not a value type
//!
//! A concrete stream type has to be **heap-representable**: `:dyn CharOutput`
//! boxes it, and `Vector<:dyn CharOutput>` stores it. A native-representation
//! value (the shape `random-state` uses) can be neither. The GC heap's
//! `BoxedObj` is `pub(crate)` to `typelisp-mem`, which cannot name a type
//! defined up here, so a new boxed variant is not available either.
//!
//! An integer handle sidesteps both: the stream *value* is a `defstruct`
//! holding an `i64`, heap-representable like any other struct, while the OS
//! resource lives in [`StreamTable`]. The struct's field is not `pub`, so a
//! handle cannot be forged from typelisp code, and every entry point here
//! validates it regardless.
//!
//! The cost is that dropping the last reference to a stream does **not**
//! close it. That was already true of a value-typed design: the collector
//! only runs when the cons arena is exhausted, so a finalizer would fire at
//! an unpredictable time or never at all. Closing stays explicit (`close`, or
//! the `with-open-file` macro that wraps it).
//!
//! # Why the table lives here and not on the interpreter
//!
//! It used to be a field of `Interp` (`src/eval/stream.rs`), which made these
//! builtins reachable only from interpreted code: an AOT-linked executable
//! links `typelisp-rt` and no interpreter at all, so a shim standing on the
//! other side of that field could not exist, and every stream method in the
//! prelude — 65 definitions, by far the largest gap in what the precompiled
//! prelude could cover — had to stay tree-walked.
//!
//! Moving the table into the runtime crate, behind one thread-local handle
//! ([`with_streams`]), is what lets both sides address the same open streams:
//! a handle a compiled `open` returns is the same handle interpreted code
//! closes. Thread-local rather than global because a `Heap` is
//! ([`crate::active_heap`]) — the two are always used together, and the tests
//! that run interpreters in parallel threads would otherwise share stdin.
//!
//! Since `docs/dev/os-threads-design.md` §5/Phase 1d, the table itself lives
//! in [`crate::shared::RtShared::streams`], behind the same `Arc` handle
//! [`crate::global_new`]/[`crate::vtable_define`] now reach through
//! ([`crate::shared::rt_shared`]) — deliberately **not** cleared by
//! `Interp::new` the way `RtShared::globals`/`RtShared::vtables` are: the
//! table's one interpreter-visible behaviour change described two
//! paragraphs down (outliving any single `Interp`) would otherwise be lost
//! the moment a fresh `Interp` reset it away.
//!
//! What only some targets have — a file, a socket — lives in the same table
//! behind [`ExtBackend`], put there by the runtime crate that brings it
//! (`typelisp-rt-sys` for files, `typelisp-rt-posix` for sockets). That
//! crate reaches its own backend again with [`StreamObj::ext_parts`]. A
//! socket is never read or written through the operations here: it is
//! non-blocking, and the socket builtins are the half that knows how to say
//! "not yet"; its backend refuses the operations here rather than serving
//! them with a blocking call that would stop every task.
//!
//! Slots are never reused (see [`StreamTable`]), so the table's one
//! interpreter-visible behaviour change — it outlives any single `Interp`
//! rather than being dropped with it — cannot make a stale handle name a
//! different stream; it names a closed one forever.

use typelisp_mem::prelude::*;
use core::convert::TryFrom;

use typelisp_mem::Heap;

/// Runs `f` against this thread's [`StreamTable`].
///
/// The only way to reach it: both the interpreter's `stream-*` builtins and
/// the compiled `rt_stream_*` shims go through here, which is what makes
/// "the same handle means the same stream in both" true by construction
/// rather than by two tables staying in step.
///
/// **The whole call is native** (`docs/dev/os-threads-design.md` §4): both
/// the wait for the lock and `f`. Either can take as long as someone else
/// decides — `f` may block reading stdin or a pipe, and the lock may be held
/// by a thread doing exactly that — and a thread that waits while counted as
/// running holds up every other thread's collection for as long as it
/// waits. Native is sound here because nothing inside touches the heap:
/// `StreamTable` holds only Rust data, and `heap` is borrowed for the whole
/// call, so `f` cannot capture it.
///
/// The lock is taken *and released* inside the native section, not just
/// used there. A guard still held on the way out would be held while
/// `leave_native` parks for a collection in progress — and a thread waiting
/// for that lock outside a native section is one the collection waits for:
/// a deadlock, not just a delay.
pub fn with_streams<T>(heap: &mut Heap, f: impl FnOnce(&mut StreamTable) -> T) -> T {
    let shared = crate::shared::rt_shared();
    heap.native(|| {
        let mut streams = shared.streams.lock();
        f(&mut streams)
    })
}

/// A leaf backend this crate does not know: a file, a socket — what only
/// some targets have, brought by the runtime crate that implements it.
///
/// The table calls these only on a stream that is open, and the reading ones
/// only on an input stream, the writing ones only on an output stream
/// ([`StreamTable::readable`]/[`StreamTable::writable`]); character pushback
/// is the table's, and served before [`Self::read_char`] is asked. Each
/// answers for itself whether it can do what it is asked — a socket refuses
/// a blocking character read, a file refuses to be waited on.
pub trait ExtBackend: core::any::Any + Send {
    /// One character, or `None` at end of input.
    fn read_char(&mut self) -> StreamResult<Option<char>>;
    /// One byte, or `None` at end of input.
    fn read_byte(&mut self) -> StreamResult<Option<u8>>;
    fn write_byte(&mut self, b: u8) -> StreamResult<()>;
    fn write_str(&mut self, text: &str) -> StreamResult<()>;
    fn finish_output(&mut self) -> StreamResult<()>;
    /// Whether a character is already here, so that reading one would not
    /// wait.
    fn listen(&self) -> bool;
    /// Flushes and releases what the backend holds, as far as can be done
    /// without waiting. The table marks the stream closed afterwards.
    fn close(&mut self) -> StreamResult<()>;
    /// What the stream is attached to, as the printer shows it inside
    /// `#<...>` (see [`StreamTable::describe`]).
    fn describe(&self) -> StreamResult<String>;
    /// The OS descriptor a task waiting on this stream parks on.
    fn wait_fd(&self) -> StreamResult<i32>;
}

/// What a stream is attached to. Only *leaf* backends live here; a composite
/// stream is a typelisp struct holding other streams (see the module docs).
pub(crate) enum Backend {
    Stdin,
    Stdout,
    Stderr,
    /// Indexed by *character*, not byte, so reading is O(1) per character and
    /// can never split a multi-byte one.
    StringIn { chars: Vec<char>, pos: usize },
    /// Drained by `get-output-stream-string`.
    StringOut(String),
    /// A backend of another crate's ([`ExtBackend`]).
    Ext(Box<dyn ExtBackend>),
    /// A closed stream. The direction flags survive so `input-stream-p` /
    /// `output-stream-p` keep answering what they answered before — CL does
    /// not say those become meaningless after `close`, and a closed output
    /// file reporting itself as an input stream would be a plain lie.
    Closed,
}

/// One open (or closed) stream: its backend and what every stream keeps
/// whatever it is attached to.
pub struct StreamObj {
    backend: Backend,
    /// Characters pushed back by `unread-char`, most recent last.
    pushback: Vec<char>,
    input: bool,
    output: bool,
    /// The last character written, for `fresh-line`'s "only if not already at
    /// the start of a line" rule.
    last_written: Option<char>,
    /// What the stream is attached to, as the printer shows it
    /// ([`StreamTable::describe`]) — recorded where only the opener knows it
    /// (a file's path), and on `close` for a socket, whose addresses go with
    /// its descriptor.
    described: Option<String>,
}

/// An [`ExtBackend`] of type `T`, with the two per-stream facts its
/// operations also need.
pub struct ExtParts<'a, T> {
    pub backend: &'a mut T,
    /// Characters pushed back by `unread-char`, most recent last.
    pub pushback: &'a mut Vec<char>,
    /// The last character written (`fresh-line`).
    pub last_written: &'a mut Option<char>,
}

impl StreamObj {
    fn new(backend: Backend, input: bool, output: bool) -> StreamObj {
        StreamObj { backend, pushback: Vec::new(), input, output, last_written: None, described: None }
    }

    /// A stream over another crate's backend. `described` is what the
    /// printer shows it attached to, when only the opener knows (a file's
    /// path); `None` asks the backend each time.
    pub fn ext(backend: Box<dyn ExtBackend>, input: bool, output: bool, described: Option<String>) -> StreamObj {
        StreamObj { described, ..StreamObj::new(Backend::Ext(backend), input, output) }
    }

    /// Whether the stream has been closed.
    pub fn is_closed(&self) -> bool {
        matches!(self.backend, Backend::Closed)
    }

    /// Closes the stream without asking its backend to flush anything: for
    /// a backend that is used up (a name resolution once it has answered).
    pub fn set_closed(&mut self) {
        self.backend = Backend::Closed;
        self.pushback.clear();
    }

    /// The backend, when it is another crate's of type `T`, with the
    /// stream's pushback and last character.
    pub fn ext_parts<T: ExtBackend>(&mut self) -> Option<ExtParts<'_, T>> {
        match &mut self.backend {
            Backend::Ext(b) => {
                let any: &mut dyn core::any::Any = b.as_mut();
                let backend = any.downcast_mut::<T>()?;
                Some(ExtParts { backend, pushback: &mut self.pushback, last_written: &mut self.last_written })
            }
            _ => None,
        }
    }
}

/// What a live backend is attached to, the way SBCL names it inside
/// `#<...>`: `for "standard output"` for a standard stream, nothing for a
/// string stream, and whatever another crate's backend says for its own.
fn describe_backend(backend: &Backend) -> StreamResult<String> {
    Ok(match backend {
        Backend::Stdin => "for \"standard input\"".to_string(),
        Backend::Stdout => "for \"standard output\"".to_string(),
        Backend::Stderr => "for \"standard error\"".to_string(),
        Backend::Ext(b) => b.describe()?,
        Backend::StringIn { .. } | Backend::StringOut(_) | Backend::Closed => String::new(),
    })
}

/// One byte of standard input, or `None` at its end.
fn read_stdin_byte(who: &str) -> StreamResult<Option<u8>> {
    let mut buf = [0u8; 1];
    let n = typelisp_mem::host::read_stdin(&mut buf).map_err(|e| format!("{}: {}", who, e))?;
    Ok(if n == 0 { None } else { Some(buf[0]) })
}

/// Decodes one UTF-8 character from `next_byte`, or `None` if the input
/// ended before it started.
///
/// Byte-at-a-time on purpose: the caller may stop after any character (that
/// is what reading one character means), so consuming a fixed-size block
/// would swallow input a later read on the same descriptor must still see.
pub fn read_utf8_char(mut next_byte: impl FnMut() -> StreamResult<Option<u8>>) -> StreamResult<Option<char>> {
    let mut buf = [0u8; 4];
    for i in 0..4 {
        match next_byte()? {
            None => {
                return if i == 0 {
                    Ok(None)
                } else {
                    Err("invalid UTF-8: input ended mid-character".to_string())
                }
            }
            Some(b) => buf[i] = b,
        }
        match core::str::from_utf8(&buf[..=i]) {
            Ok(s) => return Ok(s.chars().next()),
            // Incomplete but valid so far: take another byte.
            Err(e) if e.error_len().is_none() => continue,
            Err(_) => return Err("invalid UTF-8 in input".to_string()),
        }
    }
    Err("invalid UTF-8 in input".to_string())
}

/// A stream handle as typelisp sees it: an opaque `i64`. Handles start at 1,
/// so a zero-initialised field can never be mistaken for a stream.
pub type Handle = i64;

/// Every failure this module reports; the caller turns it into the prelude's
/// `FileError`.
pub type StreamResult<T> = Result<T, String>;

/// The table of open streams.
///
/// Slots are **never reused**: a closed handle stays closed forever rather
/// than silently becoming a different stream later. Programs open few enough
/// streams for that to cost nothing, and the alternative (generation
/// counters) buys nothing over a stale-handle error.
#[derive(Default)]
pub struct StreamTable {
    slots: Vec<Option<StreamObj>>,
}

impl StreamTable {
    /// Puts `s` in the table, under a handle no stream has had before.
    pub fn insert(&mut self, s: StreamObj) -> Handle {
        self.slots.push(Some(s));
        self.slots.len() as Handle
    }

    /// The stream under `h`, open or closed.
    pub fn get(&mut self, h: Handle) -> StreamResult<&mut StreamObj> {
        let i = usize::try_from(h - 1).map_err(|_| format!("invalid stream handle {}", h))?;
        match self.slots.get_mut(i) {
            Some(Some(s)) => Ok(s),
            _ => Err(format!("invalid stream handle {}", h)),
        }
    }

    fn readable(&mut self, h: Handle) -> StreamResult<&mut StreamObj> {
        let s = self.get(h)?;
        if s.is_closed() {
            return Err("the stream is closed".to_string());
        }
        if !s.input {
            return Err("the stream is not an input stream".to_string());
        }
        Ok(s)
    }

    fn writable(&mut self, h: Handle) -> StreamResult<&mut StreamObj> {
        let s = self.get(h)?;
        if s.is_closed() {
            return Err("the stream is closed".to_string());
        }
        if !s.output {
            return Err("the stream is not an output stream".to_string());
        }
        Ok(s)
    }

    // ---- construction -----------------------------------------------------

    pub fn stdin(&mut self) -> Handle {
        self.insert(StreamObj::new(Backend::Stdin, true, false))
    }

    /// What stream `h` is attached to, for the printer (`#<file-stream for
    /// "file /tmp/a.txt" {3}>`): see [`describe_backend`]. A closed stream
    /// keeps the description it had.
    pub fn describe(&mut self, h: Handle) -> StreamResult<String> {
        let s = self.get(h)?;
        match &s.described {
            Some(d) => Ok(d.clone()),
            None => describe_backend(&s.backend),
        }
    }

    pub fn stdout(&mut self) -> Handle {
        self.insert(StreamObj::new(Backend::Stdout, false, true))
    }

    pub fn stderr(&mut self) -> Handle {
        self.insert(StreamObj::new(Backend::Stderr, false, true))
    }

    pub fn string_input(&mut self, text: &str) -> Handle {
        let backend = Backend::StringIn { chars: text.chars().collect(), pos: 0 };
        self.insert(StreamObj::new(backend, true, false))
    }

    pub fn string_output(&mut self) -> Handle {
        self.insert(StreamObj::new(Backend::StringOut(String::new()), false, true))
    }

    // ---- lifetime ---------------------------------------------------------

    /// Flush and release the backing resource. Idempotent, as CL's `close` is.
    pub fn close(&mut self, h: Handle) -> StreamResult<()> {
        let s = self.get(h)?;
        if let Backend::Ext(b) = &mut s.backend {
            b.close()?;
        }
        // What it was attached to outlives the descriptor, as an SBCL
        // fd-stream's name does.
        if s.described.is_none() {
            s.described = Some(describe_backend(&s.backend)?);
        }
        // A string output stream keeps its text: CL allows
        // `get-output-stream-string` after `close`.
        if !matches!(s.backend, Backend::StringOut(_)) {
            s.backend = Backend::Closed;
        }
        s.pushback.clear();
        Ok(())
    }

    /// The OS descriptor under a stream a task can wait on — what the
    /// scheduler hands to `poll`. Only a socket has one worth waiting on: a
    /// file is always ready, and a string stream has no descriptor at all.
    pub fn raw_fd(&mut self, h: Handle) -> StreamResult<i32> {
        match &self.get(h)?.backend {
            Backend::Ext(b) => b.wait_fd(),
            Backend::Closed => Err("the stream is closed".to_string()),
            Backend::Stdin | Backend::Stdout | Backend::Stderr | Backend::StringIn { .. } | Backend::StringOut(_) => {
                Err("net-wait: only a socket can be waited on".to_string())
            }
        }
    }

    pub fn is_open(&mut self, h: Handle) -> bool {
        matches!(self.get(h).map(|s| s.is_closed()), Ok(false))
    }

    pub fn is_input(&mut self, h: Handle) -> StreamResult<bool> {
        Ok(self.get(h)?.input)
    }

    pub fn is_output(&mut self, h: Handle) -> StreamResult<bool> {
        Ok(self.get(h)?.output)
    }

    // ---- input ------------------------------------------------------------

    /// One character, or `None` at end of input.
    pub fn read_char(&mut self, h: Handle) -> StreamResult<Option<char>> {
        let s = self.readable(h)?;
        if let Some(c) = s.pushback.pop() {
            return Ok(Some(c));
        }
        match &mut s.backend {
            Backend::StringIn { chars, pos } => {
                let c = chars.get(*pos).copied();
                if c.is_some() {
                    *pos += 1;
                }
                Ok(c)
            }
            Backend::Stdin => read_utf8_char(|| read_stdin_byte("read")),
            Backend::Ext(b) => b.read_char(),
            Backend::Stdout | Backend::Stderr | Backend::StringOut(_) | Backend::Closed => {
                Err("the stream is not an input stream".to_string())
            }
        }
    }

    /// The next byte, or `None` at end of file.
    ///
    /// Only a byte stream (a file, stdin) answers this. A string stream is a
    /// sequence of characters, not of bytes — CL says `read-byte` on a
    /// character stream is an error, and answering with the UTF-8 encoding of
    /// whatever character came next would be inventing a file that is not
    /// there.
    ///
    /// Character pushback is refused rather than silently skipped: an
    /// `unread-char` and a byte read disagree about what the stream position
    /// even means.
    pub fn read_byte(&mut self, h: Handle) -> StreamResult<Option<u8>> {
        let s = self.readable(h)?;
        if !s.pushback.is_empty() {
            return Err("read-byte: the stream has an unread character pending".to_string());
        }
        match &mut s.backend {
            Backend::Stdin => read_stdin_byte("read-byte"),
            Backend::Ext(b) => b.read_byte(),
            Backend::Stdout | Backend::Stderr | Backend::StringIn { .. } | Backend::StringOut(_) | Backend::Closed => {
                Err("read-byte: the stream is not a byte input stream".to_string())
            }
        }
    }

    /// Write one byte. The mirror of [`Self::read_byte`], and refused on a
    /// string stream for the same reason.
    pub fn write_byte(&mut self, h: Handle, b: u8) -> StreamResult<()> {
        let s = self.writable(h)?;
        // `fresh-line` asks what was written last. A byte is not a character,
        // but a newline is byte 10 whichever way it was written, and that is
        // the only distinction `at_line_start` draws.
        s.last_written = Some(char::from(b));
        match &mut s.backend {
            Backend::Stdout => typelisp_mem::host::write_stdout(&[b]).map_err(|e| format!("write-byte: {}", e)),
            Backend::Stderr => typelisp_mem::host::write_stderr(&[b]).map_err(|e| format!("write-byte: {}", e)),
            Backend::Ext(e) => e.write_byte(b),
            Backend::Stdin | Backend::StringIn { .. } | Backend::StringOut(_) | Backend::Closed => {
                Err("write-byte: the stream is not a byte output stream".to_string())
            }
        }
    }

    /// Push `c` back so the next read returns it. CL guarantees only one
    /// character of pushback; a `Vec` costs nothing and lifts the limit, so
    /// nested readers never have to coordinate.
    pub fn unread_char(&mut self, h: Handle, c: char) -> StreamResult<()> {
        self.readable(h)?.pushback.push(c);
        Ok(())
    }

    /// How far into its text a string input stream has read, in characters.
    ///
    /// Only a string input stream answers. It is the one backend whose
    /// position is a character index a caller can act on: a file stream's is
    /// a *byte* offset inside a buffered reader, which is neither the same
    /// unit nor useful without a seek to go with it.
    ///
    /// Pushback counts backwards, because an unread character has not been
    /// consumed. A program that unread more than it read has no position —
    /// CL leaves that undefined — and is told so.
    pub fn position(&mut self, h: Handle) -> StreamResult<i64> {
        let s = self.readable(h)?;
        match &s.backend {
            Backend::StringIn { pos, .. } => match pos.checked_sub(s.pushback.len()) {
                Some(p) => Ok(p as i64),
                None => Err(format!(
                    "stream-position: {} character(s) were unread but only {} read, so there is no position",
                    s.pushback.len(),
                    pos
                )),
            },
            _ => Err("stream-position: only a string input stream has a character position".to_string()),
        }
    }

    /// Whether a character is available without blocking. Only ever `true`
    /// for a source already in memory — an OS read may always block, and
    /// answering otherwise would be a guess.
    pub fn listen(&mut self, h: Handle) -> StreamResult<bool> {
        let s = self.readable(h)?;
        if !s.pushback.is_empty() {
            return Ok(true);
        }
        Ok(match &s.backend {
            Backend::StringIn { chars, pos } => *pos < chars.len(),
            Backend::Ext(b) => b.listen(),
            Backend::Stdin | Backend::Stdout | Backend::Stderr | Backend::StringOut(_) | Backend::Closed => false,
        })
    }

    // ---- output -----------------------------------------------------------

    pub fn write_str(&mut self, h: Handle, text: &str) -> StreamResult<()> {
        let s = self.writable(h)?;
        s.last_written = text.chars().last().or(s.last_written);
        match &mut s.backend {
            Backend::StringOut(buf) => {
                buf.push_str(text);
                Ok(())
            }
            Backend::Stdout => {
                // One of the three doors a session's output leaves by; see
                // `typelisp_abi::dribble`'s module docs for the other two.
                // Only the character path is copied: `write-byte` to stdout is
                // binary output, and a dribble file is text a person reads
                // back.
                typelisp_abi::dribble::note(text);
                typelisp_mem::host::write_stdout(text.as_bytes()).map_err(|e| format!("write: {}", e))
            }
            Backend::Stderr => {
                typelisp_mem::host::write_stderr(text.as_bytes()).map_err(|e| format!("write: {}", e))
            }
            Backend::Ext(b) => b.write_str(text),
            Backend::Stdin | Backend::StringIn { .. } | Backend::Closed => {
                Err("the stream is not an output stream".to_string())
            }
        }
    }

    /// Whether the next character would start a fresh line — what
    /// `fresh-line` needs to decide whether to emit a newline at all.
    pub fn at_line_start(&mut self, h: Handle) -> StreamResult<bool> {
        Ok(matches!(self.writable(h)?.last_written, None | Some('\n')))
    }

    pub fn finish_output(&mut self, h: Handle) -> StreamResult<()> {
        let s = self.writable(h)?;
        match &mut s.backend {
            Backend::Stdout => typelisp_mem::host::flush_stdout().map_err(|e| format!("finish-output: {}", e)),
            Backend::Stderr => typelisp_mem::host::flush_stderr().map_err(|e| format!("finish-output: {}", e)),
            Backend::Ext(b) => b.finish_output(),
            Backend::Stdin | Backend::StringIn { .. } | Backend::StringOut(_) | Backend::Closed => Ok(()),
        }
    }

    /// The accumulated text of a string output stream, clearing it — CL's
    /// `get-output-stream-string`.
    pub fn take_output_string(&mut self, h: Handle) -> StreamResult<String> {
        match &mut self.get(h)?.backend {
            Backend::StringOut(buf) => Ok(core::mem::take(buf)),
            _ => Err("get-output-stream-string: not a string output stream".to_string()),
        }
    }
}
