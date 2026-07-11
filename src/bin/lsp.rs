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
//! are loaded (rooted at the nearest `typelisp.toml`) and cross-file
//! references resolve. A dependency that is itself open in the editor is
//! read from its in-memory buffer (`Loader::set_overlay`, fed from this
//! file's `docs` map) rather than disk, so an unsaved edit to it is visible
//! immediately; a dependency that isn't open is read from disk, still
//! without a cross-pass cache.
//!
//! Editing a dependency also refreshes whoever depends on it: `publish`
//! records each document's `Loader::loaded_files` in `deps` and, after
//! diagnosing the document that actually changed, transitively re-diagnoses
//! every other open document whose last-recorded dependencies include it —
//! no waiting for the dependent's own `didChange`.
//!
//! Hover and goto-definition are served from a per-document `Analysis`
//! (the last *successfully* checked `TopLevel` body plus `Registry::
//! def_locs`, see `check::locate`) cached alongside `docs`/`deps`. Both are
//! best-effort: `check::locate::locate_node` only finds nodes with a
//! recorded start location (list forms — see its doc comment for why bare
//! atoms have none), and goto-definition only resolves references that
//! already carry a fully-qualified path (`Global`/`Call`/`FnRef`/`Assoc`/
//! `MethodRef`/`Construct` — not a local variable). A document with a
//! current type error keeps serving its last-good `Analysis` rather than
//! going blank. Completion is still unimplemented; see the `TODO` list in
//! `docs/dev/` if picking this back up.

use std::collections::{HashMap, HashSet, VecDeque};

use std::path::{Path as FsPath, PathBuf};

use lsp_server::{Connection, Message, RequestId, Response, ResponseError};
use lsp_types::{
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _,
        PublishDiagnostics,
    },
    request::{GotoDefinition, HoverRequest, Request as _},
    Diagnostic, DiagnosticSeverity, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    HoverProviderCapability, InitializeParams, Location, MarkedString, OneOf, Position, PublishDiagnosticsParams,
    Range, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
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
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        ..Default::default()
    };
    let init_params = connection
        .initialize(serde_json::to_value(&capabilities).expect("ServerCapabilities always serializes"))
        .expect("LSP initialize handshake failed");
    let _params: InitializeParams =
        serde_json::from_value(init_params).unwrap_or_else(|_| InitializeParams::default());

    let mut docs: HashMap<Uri, String> = HashMap::new();
    // The filesystem paths each open document's last diagnostics pass
    // actually depended on (`Loader::loaded_files`) — lets `publish` find
    // and re-diagnose open documents that `use` whatever file just changed,
    // without waiting for their own change event.
    let mut deps: HashMap<Uri, HashSet<PathBuf>> = HashMap::new();
    // Each open document's most recent *successful* check — hover/goto-
    // definition read from this, not from `docs` directly, so a document
    // that currently has a type error keeps serving its last-good analysis
    // (see `publish_one`, which only overwrites an entry on success).
    let mut analyses: HashMap<Uri, Analysis> = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req).unwrap_or(true) {
                    break;
                }
                let resp = if req.method == HoverRequest::METHOD {
                    handle_hover(req.id, req.params, &analyses)
                } else if req.method == GotoDefinition::METHOD {
                    handle_goto_definition(req.id, req.params, &analyses)
                } else {
                    // No other requests are handled; respond so a
                    // spec-compliant client doesn't hang waiting for a
                    // reply that will never come.
                    Response {
                        id: req.id,
                        result: None,
                        error: Some(ResponseError {
                            code: lsp_server::ErrorCode::MethodNotFound as i32,
                            message: format!("unsupported request: {}", req.method),
                            data: None,
                        }),
                    }
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
                        publish(&connection, &uri, &docs, &mut deps, &mut analyses);
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
                            publish(&connection, &uri, &docs, &mut deps, &mut analyses);
                        }
                    }
                }
                m if m == DidSaveTextDocument::METHOD => {
                    if let Ok(p) = serde_json::from_value::<lsp_types::DidSaveTextDocumentParams>(not.params) {
                        let uri = p.text_document.uri;
                        if docs.contains_key(&uri) {
                            publish(&connection, &uri, &docs, &mut deps, &mut analyses);
                        }
                    }
                }
                m if m == DidCloseTextDocument::METHOD => {
                    if let Ok(p) = serde_json::from_value::<lsp_types::DidCloseTextDocumentParams>(not.params) {
                        let uri = p.text_document.uri;
                        docs.remove(&uri);
                        deps.remove(&uri);
                        analyses.remove(&uri);
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

/// Publish `uri`'s diagnostics, then transitively re-publish any other open
/// document whose last pass depended on `uri` (or on one of those, and so
/// on) — editing a dependency updates whoever `use`s it immediately, rather
/// than waiting for their own `didChange`. `deps` is both read (to find
/// dependents) and written (each republish refreshes its own entry), so a
/// stale dependency edge from a document that has since dropped the `use`
/// is corrected within the same pass that discovers it's stale.
fn publish(
    connection: &Connection,
    uri: &Uri,
    docs: &HashMap<Uri, String>,
    deps: &mut HashMap<Uri, HashSet<PathBuf>>,
    analyses: &mut HashMap<Uri, Analysis>,
) {
    let mut visited: HashSet<Uri> = HashSet::new();
    let mut queue: VecDeque<Uri> = VecDeque::new();
    visited.insert(uri.clone());
    queue.push_back(uri.clone());
    while let Some(cur) = queue.pop_front() {
        publish_one(connection, &cur, docs, deps, analyses);
        let cur_path = PathBuf::from(cur.path().as_str());
        for other in docs.keys() {
            if visited.contains(other) {
                continue;
            }
            if deps.get(other).is_some_and(|d| d.contains(&cur_path)) {
                visited.insert(other.clone());
                queue.push_back(other.clone());
            }
        }
    }
}

/// Diagnose `uri` alone and send its `publishDiagnostics`, recording what it
/// depended on this time in `deps` (see [`publish`]). `analyses[uri]` is
/// only overwritten when this pass succeeds — see [`Analysis`]'s doc
/// comment for why a document with a current error keeps serving its
/// last-good hover/goto-definition data instead of losing it.
fn publish_one(
    connection: &Connection,
    uri: &Uri,
    docs: &HashMap<Uri, String>,
    deps: &mut HashMap<Uri, HashSet<PathBuf>>,
    analyses: &mut HashMap<Uri, Analysis>,
) {
    // Every *other* open document becomes an overlay entry so a dependency
    // that's open in the editor is read from its buffer, not disk — see
    // `Loader::set_overlay`. `uri` itself is excluded: its text is passed to
    // `diagnostics_for` directly as the entry source, and the loader never
    // re-reads the entry file by path.
    let overlay: HashMap<PathBuf, String> = docs
        .iter()
        .filter(|(u, _)| *u != uri)
        .map(|(u, text)| (PathBuf::from(u.path().as_str()), text.clone()))
        .collect();
    let text = &docs[uri];
    // The URI's path component as a filesystem path (`file:///tmp/a.typl` ->
    // `/tmp/a.typl`). Percent-encoded characters are not decoded — good
    // enough for the ordinary-ASCII paths this MVP targets.
    let (diagnostics, loaded, analysis) = diagnostics_for(uri.path().as_str(), text, overlay);
    deps.insert(uri.clone(), loaded);
    if let Some(a) = analysis {
        analyses.insert(uri.clone(), a);
    }
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
///
/// Also returns the filesystem paths of every dependency file the loader
/// actually pulled in, so [`publish`] can tell which other open documents
/// this one's diagnostics depend on — and, when the check succeeds, an
/// [`Analysis`] for hover/goto-definition (`None` on a checker/read error:
/// there is no complete `Typed` tree to search, and [`publish_one`] leaves
/// whatever `Analysis` was cached from the last successful pass in place
/// rather than clearing it).
fn diagnostics_for(file: &str, text: &str, overlay: HashMap<PathBuf, String>) -> (Vec<Diagnostic>, HashSet<PathBuf>, Option<Analysis>) {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);

    let fs_file = FsPath::new(file);
    let dir = fs_file.parent().filter(|p| !p.as_os_str().is_empty()).map(FsPath::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let src_root = find_src_root(&dir).unwrap_or(dir);
    let mut loader = Loader::new(src_root);
    loader.set_overlay(overlay);

    let mut diagnostics = Vec::new();
    let result = loader.load_entry_src(&mut heap, &reader, &mut checker, &mut interp, fs_file, text);
    for w in checker.take_warnings() {
        diagnostics.push(warning_diagnostic(w));
    }
    let analysis = if result.is_ok() {
        // The entry file's own `TopLevel::Module` is always the last one
        // pushed: `Loader::load_source_inner` recurses into every `use`
        // dependency (pushing each dependency's module first) before
        // pushing its own — see that function's doc comment.
        let body = match loader.take_pending().pop() {
            Some(TopLevel::Module { body, .. }) => body,
            Some(other) => vec![other],
            None => Vec::new(),
        };
        Some(Analysis { file: file.to_string(), body, def_locs: checker.registry().def_locs.clone() })
    } else {
        None
    };
    if let Err(e) = result {
        diagnostics.push(error_diagnostic(&e, file));
    }
    (diagnostics, loader.loaded_files().clone(), analysis)
}

/// One open document's last *successful* check: the entry file's own
/// checked top-level forms, and a snapshot of `Registry::def_locs` (the
/// definition-site locations `check::locate::definition_target` resolves a
/// reference against) as of that check. Self-contained — `Typed`/`TopLevel`
/// own their `Loc`s directly (no live `Heap`/`Checker` reference needed) —
/// so it outlives the `Heap`/`Checker` `diagnostics_for` built it from.
struct Analysis {
    file: String,
    body: Vec<TopLevel>,
    def_locs: DefLocs,
}

/// `textDocument/hover`: the checked type of the smallest node
/// (`check::locate::locate_node`) at the request's cursor position, or a
/// `null` result if the document has no analysis yet, the cursor isn't
/// over a located node, or the request's params don't parse.
fn handle_hover(id: RequestId, params: serde_json::Value, analyses: &HashMap<Uri, Analysis>) -> Response {
    let result = (|| {
        let p: HoverParams = serde_json::from_value(params).ok()?;
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let analysis = analyses.get(&uri)?;
        let node = locate_node(&analysis.body, &analysis.file, pos.line + 1, pos.character + 1)?;
        let hover = Hover { contents: HoverContents::Scalar(MarkedString::String(hover_text(node))), range: None };
        Some(serde_json::to_value(hover).expect("Hover always serializes"))
    })();
    Response { id, result, error: None }
}

/// `textDocument/definition`: the same node lookup as [`handle_hover`], then
/// [`check::locate::definition_target`] to resolve it to a `Loc` — a `null`
/// result under the same conditions as hover, plus when the located node
/// isn't a resolvable reference (e.g. a local variable) or its target's
/// file can't be turned into a URI.
fn handle_goto_definition(id: RequestId, params: serde_json::Value, analyses: &HashMap<Uri, Analysis>) -> Response {
    let result = (|| {
        let p: GotoDefinitionParams = serde_json::from_value(params).ok()?;
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let analysis = analyses.get(&uri)?;
        let node = locate_node(&analysis.body, &analysis.file, pos.line + 1, pos.character + 1)?;
        let target = definition_target(node, &analysis.def_locs)?;
        // Same file as the request: reuse its `Uri` rather than reparsing
        // (also sidesteps the percent-encoding gap `publish_one`'s doc
        // comment mentions). A dependency file's definition needs a fresh
        // `Uri` built from its plain filesystem path — good enough for the
        // ordinary-ASCII paths this MVP targets, same caveat as elsewhere.
        let target_uri = if target.file.as_ref() == analysis.file.as_str() {
            uri
        } else {
            format!("file://{}", target.file).parse::<Uri>().ok()?
        };
        let resp = GotoDefinitionResponse::Scalar(Location::new(target_uri, loc_to_range(&target)));
        Some(serde_json::to_value(resp).expect("GotoDefinitionResponse always serializes"))
    })();
    Response { id, result, error: None }
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
