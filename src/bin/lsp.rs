//! `typl-lsp`: a minimal Language Server Protocol server for typelisp.
//!
//! Speaks LSP over stdio (`lsp-server`/`lsp-types`) and provides diagnostics
//! only: on every `didOpen`/`didChange`/`didSave` it re-runs the document
//! through the `Reader` -> `Checker` pipeline (the same pipeline `typl`'s REPL
//! uses, see `src/main.rs`) and publishes any read/type errors and checker
//! warnings.
//!
//! Deliberately does *not* evaluate the document: a diagnostics pass runs on
//! every keystroke, so executing arbitrary user code there would mean
//! running the user's program (side effects, `panic`, non-termination) just
//! to show a squiggly line. The one exception is `defmacro` forms, which
//! `Checker::check_form` needs registered in `Interp` *before* a later macro
//! use in the same document can be checked — `typelisp::project`'s loader
//! handles that exception itself (`Defmacro` registration is a pure
//! `HashMap` insert with no user-visible side effect); everything else the
//! loader queues for execution is simply discarded here.
//!
//! Each diagnostics pass builds a fresh `Heap`/`Checker`/`Interp` and runs
//! the document through `typelisp::project::Loader`, so `use` dependencies
//! are loaded from disk (rooted at the nearest `typelisp.toml`) and
//! cross-file references resolve. Dependencies are re-read from disk on
//! every pass — no cross-pass cache yet. Hover/completion/goto-definition
//! are still unimplemented; see the `TODO` list in `docs/dev/` if picking
//! this back up.

use std::collections::HashMap;

use std::path::{Path as FsPath, PathBuf};

use lsp_server::{Connection, Message, Response, ResponseError};
use lsp_types::{
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _,
        PublishDiagnostics,
    },
    Diagnostic, DiagnosticSeverity, InitializeParams, OneOf, Position, PublishDiagnosticsParams, Range,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

use typelisp::project::{find_src_root, Loader};
use typelisp::*;

fn main() {
    let (connection, io_threads) = Connection::stdio();
    // `run` takes `connection` by value so it (and the `Sender` it owns) is
    // dropped when the function returns, before `io_threads.join()` below —
    // the writer thread only exits once every `Sender` to its channel is
    // gone, so joining while `connection` is still alive in this scope would
    // deadlock.
    run(connection);
    io_threads.join().expect("LSP I/O threads panicked");
}

fn run(connection: Connection) {
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: None,
        definition_provider: Some(OneOf::Left(false)),
        ..Default::default()
    };
    let init_params = connection
        .initialize(serde_json::to_value(&capabilities).expect("ServerCapabilities always serializes"))
        .expect("LSP initialize handshake failed");
    let _params: InitializeParams =
        serde_json::from_value(init_params).unwrap_or_else(|_| InitializeParams::default());

    let mut docs: HashMap<Uri, String> = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req).unwrap_or(true) {
                    break;
                }
                // No other requests are handled in this diagnostics-only
                // server; respond so a spec-compliant client doesn't hang
                // waiting for a reply that will never come.
                let resp = Response {
                    id: req.id,
                    result: None,
                    error: Some(ResponseError {
                        code: lsp_server::ErrorCode::MethodNotFound as i32,
                        message: format!("unsupported request: {}", req.method),
                        data: None,
                    }),
                };
                if connection.sender.send(Message::Response(resp)).is_err() {
                    break;
                }
            }
            Message::Notification(not) => match not.method.as_str() {
                m if m == DidOpenTextDocument::METHOD => {
                    if let Ok(p) = serde_json::from_value::<lsp_types::DidOpenTextDocumentParams>(not.params) {
                        let uri = p.text_document.uri;
                        docs.insert(uri.clone(), p.text_document.text);
                        publish(&connection, &uri, docs.get(&uri).unwrap());
                    }
                }
                m if m == DidChangeTextDocument::METHOD => {
                    if let Ok(mut p) = serde_json::from_value::<lsp_types::DidChangeTextDocumentParams>(not.params) {
                        // `TextDocumentSyncKind::FULL` means the client always
                        // sends the whole new text as the last (and only)
                        // change event, with no `range`.
                        if let Some(change) = p.content_changes.pop() {
                            let uri = p.text_document.uri;
                            docs.insert(uri.clone(), change.text);
                            publish(&connection, &uri, docs.get(&uri).unwrap());
                        }
                    }
                }
                m if m == DidSaveTextDocument::METHOD => {
                    if let Ok(p) = serde_json::from_value::<lsp_types::DidSaveTextDocumentParams>(not.params) {
                        let uri = p.text_document.uri;
                        if let Some(text) = docs.get(&uri) {
                            publish(&connection, &uri, text);
                        }
                    }
                }
                m if m == DidCloseTextDocument::METHOD => {
                    if let Ok(p) = serde_json::from_value::<lsp_types::DidCloseTextDocumentParams>(not.params) {
                        let uri = p.text_document.uri;
                        docs.remove(&uri);
                        // Clear diagnostics for a closed document.
                        let params = PublishDiagnosticsParams { uri, diagnostics: Vec::new(), version: None };
                        let n = lsp_server::Notification::new(PublishDiagnostics::METHOD.into(), params);
                        let _ = connection.sender.send(Message::Notification(n));
                    }
                }
                "exit" => break,
                _ => {}
            },
            Message::Response(_) => {}
        }
    }
}

fn publish(connection: &Connection, uri: &Uri, text: &str) {
    // The URI's path component as a filesystem path (`file:///tmp/a.typl` ->
    // `/tmp/a.typl`). Percent-encoded characters are not decoded — good
    // enough for the ordinary-ASCII paths this MVP targets.
    let diagnostics = diagnostics_for(uri.path().as_str(), text);
    let params = PublishDiagnosticsParams { uri: uri.clone(), diagnostics, version: None };
    let n = lsp_server::Notification::new(PublishDiagnostics::METHOD.into(), params);
    let _ = connection.sender.send(Message::Notification(n));
}

/// Re-checks `text` from scratch as the content of `file` and turns whatever
/// the pipeline reports into LSP diagnostics. Runs through
/// `typelisp::project::Loader`, so the document's `use` dependencies are
/// loaded from disk (rooted at the nearest `typelisp.toml`, falling back to
/// the file's own directory) and cross-file references resolve. Only checks
/// — nothing is executed beyond `defmacro` registration (the loader's
/// built-in exception); the queued forms are discarded.
///
/// Unlike the REPL's `try_run_pending`, a read error here is never "need
/// more input to keep going" — the whole document is already in hand — so
/// every `Reader` error becomes a diagnostic directly, and reading stops at
/// the first one (an s-expression reader can't meaningfully resync past an
/// unmatched paren).
fn diagnostics_for(file: &str, text: &str) -> Vec<Diagnostic> {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);

    let fs_file = FsPath::new(file);
    let dir = fs_file.parent().filter(|p| !p.as_os_str().is_empty()).map(FsPath::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let src_root = find_src_root(&dir).unwrap_or(dir);
    let mut loader = Loader::new(src_root);

    let mut diagnostics = Vec::new();
    let result = loader.load_entry_src(&mut heap, &reader, &mut checker, &mut interp, fs_file, text);
    for w in checker.take_warnings() {
        diagnostics.push(warning_diagnostic(w));
    }
    if let Err(e) = result {
        diagnostics.push(error_diagnostic(&e, file));
    }
    diagnostics
}

/// An error's diagnostic, anchored at its source location when that location
/// is in the document being checked. An error from a *dependency* file (its
/// `Loc` names another path) can't be underlined in this document, so it's
/// anchored at the top with the full `file:line:col:`-prefixed message.
fn error_diagnostic(e: &Error, current_file: &str) -> Diagnostic {
    match e.loc() {
        Some(l) if &*l.file == current_file => {
            diagnostic(format!("{}", e.kind()), loc_to_range(l), DiagnosticSeverity::ERROR)
        }
        Some(_) => diagnostic(format!("{}", e), doc_start_range(), DiagnosticSeverity::ERROR),
        None => diagnostic(format!("{}", e.kind()), doc_start_range(), DiagnosticSeverity::ERROR),
    }
}

/// Checker warnings (e.g. "redefining function `foo`") carry no source
/// location today, so they're anchored at the top of the document — still
/// surfaces the warning text, just without a precise squiggly.
fn warning_diagnostic(message: String) -> Diagnostic {
    diagnostic(message, doc_start_range(), DiagnosticSeverity::WARNING)
}

fn diagnostic(message: String, range: Range, severity: DiagnosticSeverity) -> Diagnostic {
    Diagnostic { range, severity: Some(severity), source: Some("typelisp".into()), message, ..Default::default() }
}

/// `Loc`'s line/col are 1-based; LSP's `Position` is 0-based. Highlights a
/// single character at the location, since `Loc` is a point, not a span.
fn loc_to_range(loc: &Loc) -> Range {
    let line = loc.line.saturating_sub(1);
    let col = loc.col.saturating_sub(1);
    Range::new(Position::new(line, col), Position::new(line, col + 1))
}

fn doc_start_range() -> Range {
    Range::new(Position::new(0, 0), Position::new(0, 1))
}
