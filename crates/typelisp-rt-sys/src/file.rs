//! A file as a stream: the backend `open` puts in the runtime's stream table.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};

use typelisp_rt::stream::{read_utf8_char, ExtBackend, Handle, StreamObj, StreamResult, StreamTable};

/// An open file, read or written through a buffer.
enum FileBackend {
    In(BufReader<File>),
    Out(BufWriter<File>),
}

/// One byte of `r`, or `None` at its end; `who` names the operation in the
/// error.
fn read_one(r: &mut BufReader<File>, who: &str) -> StreamResult<Option<u8>> {
    let mut buf = [0u8; 1];
    let n = r.read(&mut buf).map_err(|e| format!("{}: {}", who, e))?;
    Ok(if n == 0 { None } else { Some(buf[0]) })
}

impl ExtBackend for FileBackend {
    fn read_char(&mut self) -> StreamResult<Option<char>> {
        match self {
            FileBackend::In(r) => read_utf8_char(|| read_one(r, "read")),
            FileBackend::Out(_) => Err("the stream is not an input stream".to_string()),
        }
    }

    fn read_byte(&mut self) -> StreamResult<Option<u8>> {
        match self {
            FileBackend::In(r) => read_one(r, "read-byte"),
            FileBackend::Out(_) => Err("read-byte: the stream is not a byte input stream".to_string()),
        }
    }

    fn write_byte(&mut self, b: u8) -> StreamResult<()> {
        match self {
            FileBackend::Out(w) => w.write_all(&[b]).map_err(|e| format!("write-byte: {}", e)),
            FileBackend::In(_) => Err("write-byte: the stream is not a byte output stream".to_string()),
        }
    }

    fn write_str(&mut self, text: &str) -> StreamResult<()> {
        match self {
            FileBackend::Out(w) => w.write_all(text.as_bytes()).map_err(|e| format!("write: {}", e)),
            FileBackend::In(_) => Err("the stream is not an output stream".to_string()),
        }
    }

    fn finish_output(&mut self) -> StreamResult<()> {
        match self {
            FileBackend::Out(w) => w.flush().map_err(|e| format!("finish-output: {}", e)),
            FileBackend::In(_) => Ok(()),
        }
    }

    /// An OS read may always block, and answering otherwise would be a
    /// guess.
    fn listen(&self) -> bool {
        false
    }

    fn close(&mut self) -> StreamResult<()> {
        match self {
            FileBackend::Out(w) => w.flush().map_err(|e| format!("close: {}", e)),
            FileBackend::In(_) => Ok(()),
        }
    }

    /// Never asked: [`open_file`] records the path, which only the opener
    /// knows.
    fn describe(&self) -> StreamResult<String> {
        Err("describe: a file stream was opened without recording its path".to_string())
    }

    /// A file is always ready, so there is nothing to wait on.
    fn wait_fd(&self) -> StreamResult<(i32, typelisp_rt::sched::PollerRef)> {
        Err("net-wait: only a socket can be waited on".to_string())
    }
}

/// Opens `path` and puts it in `table`. `mode`: 0 input, 1 output
/// (truncate), 2 output (append).
///
/// An integer rather than a keyword because the CL-shaped
/// `:direction`/`:if-exists` surface belongs in the prelude's `open`; this
/// layer only needs to know which `OpenOptions` to use.
pub fn open_file(table: &mut StreamTable, path: &str, mode: i64) -> StreamResult<Handle> {
    let describe = |e: std::io::Error| format!("{}: {}", path, e);
    let (backend, input, output) = match mode {
        0 => (FileBackend::In(BufReader::new(File::open(path).map_err(describe)?)), true, false),
        1 => (FileBackend::Out(BufWriter::new(File::create(path).map_err(describe)?)), false, true),
        2 => {
            let f = OpenOptions::new().create(true).append(true).open(path).map_err(describe)?;
            (FileBackend::Out(BufWriter::new(f)), false, true)
        }
        other => return Err(format!("open: unknown mode {}", other)),
    };
    let described = Some(format!("for \"file {}\"", path));
    Ok(table.insert(StreamObj::ext(Box::new(backend), input, output, described)))
}
