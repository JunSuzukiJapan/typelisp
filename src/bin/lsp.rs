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
//! use in the same document can be checked — mirroring
//! `main.rs::needs_immediate_exec` exactly, `Defmacro` registration is a pure
//! `HashMap` insert with no user-visible side effect.
//!
//! Each diagnostics pass builds a fresh `Heap`/`Checker`/`Interp` and only
//! ever looks at the one document being edited — there is no cross-file
//! `module`/`use` resolution yet. That is the main gap between this and a
//! "real" language server; see the `TODO` list in `docs/dev/` if picking
//! this back up.

use std::collections::HashMap;

use lsp_server::{Connection, Message, Response, ResponseError};
use lsp_types::{
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _,
        PublishDiagnostics,
    },
    Diagnostic, DiagnosticSeverity, InitializeParams, OneOf, Position, PublishDiagnosticsParams, Range,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

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
    let diagnostics = diagnostics_for(uri.as_str(), text);
    let params = PublishDiagnosticsParams { uri: uri.clone(), diagnostics, version: None };
    let n = lsp_server::Notification::new(PublishDiagnostics::METHOD.into(), params);
    let _ = connection.sender.send(Message::Notification(n));
}

/// Re-reads and re-checks `text` from scratch (as though it named `file` on
/// disk) and turns whatever the pipeline reports into LSP diagnostics.
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

    let mut diagnostics = Vec::new();

    let forms = match reader.read_all_in(&mut heap, file, text) {
        Ok(forms) => forms,
        Err(e) => {
            diagnostics.push(error_diagnostic(&e));
            return diagnostics;
        }
    };

    for v in forms {
        let result = checker.check_form(&mut heap, &interp, v);
        for w in checker.take_warnings() {
            diagnostics.push(warning_diagnostic(w));
        }
        match result {
            Ok(tl) => {
                if needs_immediate_exec(&tl) {
                    if let Err(e) = interp.exec(&mut heap, tl) {
                        diagnostics.push(eval_error_diagnostic(&e));
                    }
                }
            }
            Err(e) => {
                diagnostics.push(error_diagnostic(&e));
                break;
            }
        }
    }

    diagnostics
}

/// True for a `Defmacro` (or a checker-synthesized monomorphization bundle
/// containing one) — see `main.rs::needs_immediate_exec`, which this mirrors
/// exactly: it's copied rather than shared because it's ten lines and pulling
/// it out into the library for one non-REPL caller isn't worth the added
/// public surface.
fn needs_immediate_exec(tl: &TopLevel) -> bool {
    match tl {
        TopLevel::Defmacro { .. } => true,
        TopLevel::Module { path, body } if *path == Path::root(MONO_BUNDLE_MODULE) => {
            body.iter().any(needs_immediate_exec)
        }
        _ => false,
    }
}

fn error_diagnostic(e: &Error) -> Diagnostic {
    let range = e.loc().map(loc_to_range).unwrap_or_else(doc_start_range);
    diagnostic(format!("{}", e.kind()), range, DiagnosticSeverity::ERROR)
}

fn eval_error_diagnostic(e: &EvalError) -> Diagnostic {
    let range = e.loc().map(loc_to_range).unwrap_or_else(doc_start_range);
    diagnostic(format!("{}", e.kind()), range, DiagnosticSeverity::ERROR)
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
