//! CL's `dribble`: the file a copy of the session is written to. The copying
//! itself is `typelisp_abi::dribble`'s, a crate below every one that writes
//! the session (see that module's docs); this one opens and closes the file.

use std::fs::File;
use std::io::{BufWriter, Write};

use typelisp_mem::{spec, wk, Heap, StdModule, TypeId, TypeSpec, Value};
use typelisp_rt::result_value::{err_in, ok_in, TypeTable};
use typelisp_rt::stream_builtin::FILE_ERROR;

/// The runtime type each builtin here answers with
/// (`typelisp_rt::result_value`).
pub const RESULT_TYPES: TypeTable = &[
    (wk::F_DRIBBLE_START, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
    (wk::F_DRIBBLE_STOP, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
];

/// A dribble file.
struct FileSink(BufWriter<File>);

impl typelisp_abi::dribble::Sink for FileSink {
    /// Flushed on every note, for the same reason standard output is: a
    /// dribble file is read while the session is still running.
    fn write(&mut self, text: &str) -> bool {
        self.0.write_all(text.as_bytes()).and_then(|()| self.0.flush()).is_ok()
    }

    fn finish(mut self: Box<Self>) -> Result<(), String> {
        self.0.flush().map_err(|e| e.to_string())
    }
}

/// `(dribble-start path)`: begin copying the session into `path`, truncating
/// it. Starting a second one closes the first, which is what CL says happens
/// when `dribble` is called while a dribble is already open.
pub fn dribble_start(heap: &mut Heap, path: &str) -> Value {
    match File::create(path) {
        Ok(f) => {
            // The one it replaces is closed as it is dropped.
            drop(typelisp_abi::dribble::replace(Some(Box::new(FileSink(BufWriter::new(f))))));
            ok_in(heap, RESULT_TYPES, wk::F_DRIBBLE_START, Value::Empty)
        }
        Err(e) => err_in(heap, RESULT_TYPES, wk::F_DRIBBLE_START, TypeId::FILE_ERROR, format!("dribble: {}: {}", path, e)),
    }
}

/// `(dribble-stop)`: close the dribble file. A no-op when none is open, as
/// CL's argument-less `dribble` is.
///
/// `Result`, not `()`, because the close *flushes*: a full disk is discovered
/// here or nowhere, and discovering it nowhere would mean a truncated
/// transcript reported as a complete one.
pub fn dribble_stop(heap: &mut Heap) -> Value {
    let closed = match typelisp_abi::dribble::replace(None) {
        Some(sink) => sink.finish(),
        None => Ok(()),
    };
    match closed {
        Ok(()) => ok_in(heap, RESULT_TYPES, wk::F_DRIBBLE_STOP, Value::Empty),
        Err(e) => err_in(heap, RESULT_TYPES, wk::F_DRIBBLE_STOP, TypeId::FILE_ERROR, format!("dribble: {}", e)),
    }
}
