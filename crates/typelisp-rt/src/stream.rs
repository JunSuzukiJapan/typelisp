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
//! Moving the table into the runtime crate, behind one thread-local
//! ([`with_streams`]), is what lets both sides address the same open streams:
//! a handle a compiled `open` returns is the same handle interpreted code
//! closes. Thread-local rather than global because a `Heap` is
//! ([`crate::active_heap`]) — the two are always used together, and the tests
//! that run interpreters in parallel threads would otherwise share stdin.
//!
//! Slots are never reused (see [`StreamTable`]), so the table's one
//! interpreter-visible behaviour change — it outlives any single `Interp`
//! rather than being dropped with it — cannot make a stale handle name a
//! different stream; it names a closed one forever.

use std::cell::RefCell;
use std::convert::TryFrom;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};

thread_local! {
    /// This thread's open streams. See the module docs for why it is here.
    static STREAMS: RefCell<StreamTable> = RefCell::new(StreamTable::default());
}

/// Runs `f` against this thread's [`StreamTable`].
///
/// The only way to reach it: both the interpreter's `stream-*` builtins and
/// the compiled `rt_stream_*` shims go through here, which is what makes
/// "the same handle means the same stream in both" true by construction
/// rather than by two tables staying in step.
pub fn with_streams<T>(f: impl FnOnce(&mut StreamTable) -> T) -> T {
    STREAMS.with(|t| f(&mut t.borrow_mut()))
}

/// What a stream is attached to. Only *leaf* backends live here; a composite
/// stream is a typelisp struct holding other streams (see the module docs).
enum Backend {
    Stdin,
    Stdout,
    Stderr,
    FileIn(BufReader<File>),
    FileOut(BufWriter<File>),
    /// Indexed by *character*, not byte, so reading is O(1) per character and
    /// can never split a multi-byte one.
    StringIn { chars: Vec<char>, pos: usize },
    /// Drained by `get-output-stream-string`.
    StringOut(String),
    /// A closed stream. The direction flags survive so `input-stream-p` /
    /// `output-stream-p` keep answering what they answered before — CL does
    /// not say those become meaningless after `close`, and a closed output
    /// file reporting itself as an input stream would be a plain lie.
    Closed,
}

struct StreamObj {
    backend: Backend,
    /// Characters pushed back by `unread-char`, most recent last.
    pushback: Vec<char>,
    input: bool,
    output: bool,
    /// The last character written, for `fresh-line`'s "only if not already at
    /// the start of a line" rule.
    last_written: Option<char>,
}

impl StreamObj {
    fn new(backend: Backend, input: bool, output: bool) -> StreamObj {
        StreamObj { backend, pushback: Vec::new(), input, output, last_written: None }
    }
}

/// A stream handle as typelisp sees it: an opaque `i64`. Handles start at 1,
/// so a zero-initialised field can never be mistaken for a stream.
pub type Handle = i64;

/// Every failure this module reports; the caller turns it into the prelude's
/// `FileError`.
pub type StreamResult<T> = Result<T, String>;

/// The interpreter-side table of open streams.
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
    fn insert(&mut self, s: StreamObj) -> Handle {
        self.slots.push(Some(s));
        self.slots.len() as Handle
    }

    fn get(&mut self, h: Handle) -> StreamResult<&mut StreamObj> {
        let i = usize::try_from(h - 1).map_err(|_| format!("invalid stream handle {}", h))?;
        match self.slots.get_mut(i) {
            Some(Some(s)) => Ok(s),
            _ => Err(format!("invalid stream handle {}", h)),
        }
    }

    fn readable(&mut self, h: Handle) -> StreamResult<&mut StreamObj> {
        let s = self.get(h)?;
        if matches!(s.backend, Backend::Closed) {
            return Err("the stream is closed".to_string());
        }
        if !s.input {
            return Err("the stream is not an input stream".to_string());
        }
        Ok(s)
    }

    fn writable(&mut self, h: Handle) -> StreamResult<&mut StreamObj> {
        let s = self.get(h)?;
        if matches!(s.backend, Backend::Closed) {
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

    /// `mode`: 0 input, 1 output (truncate), 2 output (append).
    ///
    /// An integer rather than a keyword because the CL-shaped
    /// `:direction`/`:if-exists` surface belongs in the prelude's `open`;
    /// this layer only needs to know which `OpenOptions` to use.
    pub fn open_file(&mut self, path: &str, mode: i64) -> StreamResult<Handle> {
        let describe = |e: std::io::Error| format!("{}: {}", path, e);
        let obj = match mode {
            0 => {
                let f = File::open(path).map_err(describe)?;
                StreamObj::new(Backend::FileIn(BufReader::new(f)), true, false)
            }
            1 => {
                let f = File::create(path).map_err(describe)?;
                StreamObj::new(Backend::FileOut(BufWriter::new(f)), false, true)
            }
            2 => {
                let f = OpenOptions::new().create(true).append(true).open(path).map_err(describe)?;
                StreamObj::new(Backend::FileOut(BufWriter::new(f)), false, true)
            }
            other => return Err(format!("open: unknown mode {}", other)),
        };
        Ok(self.insert(obj))
    }

    // ---- lifetime ---------------------------------------------------------

    /// Flush and release the backing resource. Idempotent, as CL's `close` is.
    pub fn close(&mut self, h: Handle) -> StreamResult<()> {
        let s = self.get(h)?;
        if let Backend::FileOut(w) = &mut s.backend {
            w.flush().map_err(|e| format!("close: {}", e))?;
        }
        // A string output stream keeps its text: CL allows
        // `get-output-stream-string` after `close`.
        if !matches!(s.backend, Backend::StringOut(_)) {
            s.backend = Backend::Closed;
        }
        s.pushback.clear();
        Ok(())
    }

    pub fn is_open(&mut self, h: Handle) -> bool {
        matches!(self.get(h).map(|s| matches!(s.backend, Backend::Closed)), Ok(false))
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
            Backend::FileIn(r) => read_one_char(r),
            Backend::Stdin => {
                let stdin = std::io::stdin();
                let mut lock = stdin.lock();
                read_one_char(&mut lock)
            }
            _ => Err("the stream is not an input stream".to_string()),
        }
    }

    /// The next byte, or `None` at end of file.
    ///
    /// Only a *file* stream (and stdin) answers this. A string stream is a
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
        let mut buf = [0u8; 1];
        let n = match &mut s.backend {
            Backend::FileIn(r) => r.read(&mut buf).map_err(|e| format!("read-byte: {}", e))?,
            Backend::Stdin => {
                let stdin = std::io::stdin();
                let mut lock = stdin.lock();
                lock.read(&mut buf).map_err(|e| format!("read-byte: {}", e))?
            }
            _ => return Err("read-byte: the stream is not a byte input stream".to_string()),
        };
        Ok(if n == 0 { None } else { Some(buf[0]) })
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
            Backend::FileOut(w) => w.write_all(&[b]).map_err(|e| format!("write-byte: {}", e)),
            Backend::Stdout => std::io::stdout().write_all(&[b]).map_err(|e| format!("write-byte: {}", e)),
            Backend::Stderr => std::io::stderr().write_all(&[b]).map_err(|e| format!("write-byte: {}", e)),
            _ => Err("write-byte: the stream is not a byte output stream".to_string()),
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
    /// consumed. The floor at zero is for the program that unreads more than
    /// it read — CL leaves that undefined, and a negative position would
    /// travel further than the mistake did.
    pub fn position(&mut self, h: Handle) -> StreamResult<i64> {
        let s = self.readable(h)?;
        match &s.backend {
            Backend::StringIn { pos, .. } => Ok((*pos as i64 - s.pushback.len() as i64).max(0)),
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
            _ => false,
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
            Backend::FileOut(w) => w.write_all(text.as_bytes()).map_err(|e| format!("write: {}", e)),
            Backend::Stdout => {
                std::io::stdout().write_all(text.as_bytes()).map_err(|e| format!("write: {}", e))
            }
            Backend::Stderr => {
                std::io::stderr().write_all(text.as_bytes()).map_err(|e| format!("write: {}", e))
            }
            _ => Err("the stream is not an output stream".to_string()),
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
            Backend::FileOut(w) => w.flush().map_err(|e| format!("finish-output: {}", e)),
            Backend::Stdout => std::io::stdout().flush().map_err(|e| format!("finish-output: {}", e)),
            Backend::Stderr => std::io::stderr().flush().map_err(|e| format!("finish-output: {}", e)),
            _ => Ok(()),
        }
    }

    /// The accumulated text of a string output stream, clearing it — CL's
    /// `get-output-stream-string`.
    pub fn take_output_string(&mut self, h: Handle) -> StreamResult<String> {
        match &mut self.get(h)?.backend {
            Backend::StringOut(buf) => Ok(std::mem::take(buf)),
            _ => Err("get-output-stream-string: not a string output stream".to_string()),
        }
    }
}

/// Decode one UTF-8 character from a byte reader.
///
/// Byte-at-a-time on purpose: the caller may stop after any character (that
/// is what reading one character means), so consuming a fixed-size block
/// would swallow input a later read on the same descriptor must still see.
fn read_one_char<R: BufRead>(r: &mut R) -> StreamResult<Option<char>> {
    let mut buf = [0u8; 4];
    for i in 0..4 {
        match r.read(&mut buf[i..=i]) {
            Ok(0) => {
                return if i == 0 {
                    Ok(None)
                } else {
                    Err("invalid UTF-8: input ended mid-character".to_string())
                }
            }
            Ok(_) => {}
            Err(e) => return Err(format!("read: {}", e)),
        }
        match std::str::from_utf8(&buf[..=i]) {
            Ok(s) => return Ok(s.chars().next()),
            // Incomplete but valid so far: take another byte.
            Err(e) if e.error_len().is_none() => continue,
            Err(_) => return Err("invalid UTF-8 in input".to_string()),
        }
    }
    Err("invalid UTF-8 in input".to_string())
}
