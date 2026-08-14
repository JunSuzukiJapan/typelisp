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
//! immediately; a dependency that isn't open is read from disk. Every pass
//! re-reads and re-checks the prelude and each dependency from scratch —
//! nothing is cached between passes.
//!
//! Editing a dependency also refreshes whoever depends on it: `publish`
//! records each document's `Loader::loaded_files` in `deps` and, after
//! diagnosing the document that actually changed, transitively re-diagnoses
//! every other open document whose last-recorded dependencies include it —
//! no waiting for the dependent's own `didChange`.
//!
//! Hover and goto-definition are served from a per-document `Analysis`
//! (the last *successfully* checked top-level body plus `Registry::
//! def_locs`, see `check::locate`) cached alongside `docs`/`deps`. Both are
//! best-effort: `check::locate::locate_node` only finds nodes with a
//! recorded start location (list forms — see its doc comment for why bare
//! atoms have none), and goto-definition only resolves references that
//! already carry a fully-qualified path (`Global`/`Call`/`FnRef`/`Assoc`/
//! `MethodRef`/`Construct` — not a local variable). A document with a
//! current type error keeps serving its last-good `Analysis` rather than
//! going blank. Completion is served from the same `Analysis` plus
//! `check::locate::completion_candidates`/`completion_locals` (globals and
//! local bindings, including inside non-catchall `match` arms via the
//! checker's error-recovery mode — see `Checker::set_recover`).
//!
//! Semantic tokens come from the same `Analysis`. The checker records the
//! source span of every user-defined type/trait name it *resolves*
//! (`Checker::take_type_uses`, turned into tokens by `check::semantic`), so
//! the highlighting covers a type imported through `use` — which no
//! editor-side grammar can reach — and never fires on a function that merely
//! shares a type's name, since no type was resolved at that position.

// Documents are keyed by `lsp_types::Uri`, which clippy flags as a "mutable
// key type" because it contains an interior-mutability cell — a lazily
// populated hash cache (`Cell<NonZero<u32>>`), *not* state that participates
// in `Eq`/`Hash`. The key's observable identity is immutable, so using it as
// a `HashMap`/`HashSet` key is sound; the lint is a false positive for this
// upstream type and there is no more-natural document key to switch to.
#![allow(clippy::mutable_key_type)]

use std::collections::{HashMap, HashSet, VecDeque};

use std::path::{Path as FsPath, PathBuf};

use lsp_server::{Connection, Message, RequestId, Response, ResponseError};
use lsp_types::{
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _,
        PublishDiagnostics,
    },
    request::{Completion, GotoDefinition, HoverRequest, Request as _, SemanticTokensFullRequest},
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DiagnosticSeverity, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    HoverProviderCapability, InitializeParams, Location, MarkedString, OneOf, Position, PublishDiagnosticsParams,
    Range, SemanticToken, SemanticTokenType, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensServerCapabilities, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
};

use typelisp::check::core;
use typelisp::check::semantic::{encode, file_type_tokens, TypeKind, TypeToken};
use typelisp::project::{find_src_root, module_segs_for, Loader};
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

/// The legend the server advertises, in the order `semantic_token_index` maps
/// onto. All three are LSP *standard* token types, so every editor and theme
/// already knows how to colour them without any typelisp-specific setup.
const SEMANTIC_TOKEN_TYPES: [SemanticTokenType; 3] =
    [SemanticTokenType::STRUCT, SemanticTokenType::ENUM, SemanticTokenType::INTERFACE];

fn semantic_token_index(kind: TypeKind) -> u32 {
    match kind {
        TypeKind::Struct => 0,
        TypeKind::Enum => 1,
        // A trait is the nearest thing typelisp has to an interface.
        TypeKind::Trait => 2,
    }
}

fn run(connection: Connection) {
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions { trigger_characters: Some(vec![":".to_string()]), ..Default::default() }),
        // Type names are the one thing an editor's own grammar cannot
        // resolve: a `defstruct`/`defenum`/`deftrait` name is usually
        // lowercase, a use of one imported through `use` lives in another
        // file entirely, and a function may share a type's name. Only the
        // checker knows which occurrence is which, and it records exactly
        // that while checking (`Checker::take_type_uses`).
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                work_done_progress_options: WorkDoneProgressOptions::default(),
                legend: SemanticTokensLegend {
                    token_types: SEMANTIC_TOKEN_TYPES.to_vec(),
                    token_modifiers: Vec::new(),
                },
                // Whole-document only: the tokens are already computed and
                // cached by the check the document's diagnostics needed
                // anyway, so answering a range would just mean filtering the
                // same list.
                range: Some(false),
                full: Some(SemanticTokensFullOptions::Bool(true)),
            },
        )),
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
                } else if req.method == Completion::METHOD {
                    handle_completion(req.id, req.params, &docs)
                } else if req.method == SemanticTokensFullRequest::METHOD {
                    handle_semantic_tokens(req.id, req.params, &analyses)
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

/// Every *other* open document as an overlay entry, so a dependency that's
/// open in the editor is read from its in-memory buffer, not disk — see
/// `Loader::set_overlay`. `exclude` (the document being processed) is left
/// out: its own text is passed to the pipeline directly as the entry source,
/// and the loader never re-reads the entry file by path.
fn build_overlay(docs: &HashMap<Uri, String>, exclude: &Uri) -> HashMap<PathBuf, String> {
    docs.iter()
        .filter(|(u, _)| *u != exclude)
        .map(|(u, text)| (PathBuf::from(u.path().as_str()), text.clone()))
        .collect()
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
    let overlay = build_overlay(docs, uri);
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
/// this one's diagnostics depend on — and, whenever the check produced a tree,
/// an [`Analysis`] for hover/goto-definition (`None` only on a *read* error:
/// there are no checked forms to search, and [`publish_one`] leaves whatever
/// `Analysis` was cached from the last pass that did produce one in place
/// rather than clearing it).
///
/// The checker runs in error-recovery mode ([`Checker::set_recover`], enabled
/// after the strict prelude load): a document with type errors still yields a
/// partial form list (so hover/goto keep working) and *all* of its type
/// errors are reported at once (via [`Checker::take_errors`]) rather than only
/// the first. `result` is `Err` only on a reader error, which stops the whole
/// document (an s-expression reader can't resync past an unmatched paren).
fn diagnostics_for(
    file: &str,
    text: &str,
    overlay: HashMap<PathBuf, String>,
) -> (Vec<Diagnostic>, HashSet<PathBuf>, Option<Analysis>) {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    checker.set_recover(true);

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
    // Every recoverable type error accumulated across the whole document, each
    // carrying its own source location — reported as its own diagnostic.
    for e in checker.take_errors() {
        diagnostics.push(error_diagnostic(&e, file));
    }
    let analysis = if result.is_ok() {
        // In recover mode this tree is present even when the document has
        // type errors — see [`module_body`].
        let body = module_body(&heap, loader.take_pending().pop());
        let def_locs = checker.registry().def_locs.clone();
        let docs = checker.registry().docs.clone();
        let tokens = file_type_tokens(&checker.take_type_uses(), file);
        // The tree is heap cells, so the `Analysis` takes the `Heap` with it —
        // see its doc comment.
        Some(Analysis { file: file.to_string(), heap, body, def_locs, docs, tokens })
    } else {
        None
    };
    // A reader error (the only thing `result` reports in recover mode) stops
    // the document, so report it too.
    if let Err(e) = result {
        diagnostics.push(error_diagnostic(&e, file));
    }
    (diagnostics, loader.loaded_files().clone(), analysis)
}

/// One open document's last check that produced a tree (in recover mode, any
/// check that got past the reader — type errors don't prevent it): the entry
/// file's own checked top-level forms, and a snapshot of `Registry::def_locs`
/// (the
/// definition-site locations `check::locate::definition_target` resolves a
/// reference against) as of that check.
///
/// The checked tree is cons cells, so this **owns the `Heap` those cells live
/// in** — a snapshot of the IR is a snapshot of its heap. That is the whole
/// reason `heap` is a field: `body`'s `Value`s are indices into it and mean
/// nothing without it, so the heap cannot be the one `diagnostics_for` drops
/// on the way out. Nothing conses on it again, so no collection can run and
/// the tree stays put; the forms are permanently rooted besides
/// (`Loader::load_source` roots each module wrapper).
///
/// The cost is one heap per open document. Acceptable for the handful of files
/// an editor session holds, and the alternative — copying the tree into a
/// heap-independent mirror — would need a second walker for every query.
struct Analysis {
    file: String,
    /// The heap `body` indexes into. Must outlive every query that reads it.
    heap: Heap,
    body: Vec<TopLevelForm>,
    def_locs: DefLocs,
    /// A snapshot of `Registry::docs` as of the same check — `hover_text`'s
    /// docstring lookup, alongside `def_locs`.
    docs: Docs,
    /// The semantic tokens of this document's last successful check: every
    /// position where the checker *resolved* a user-defined type or trait
    /// name, with its exact span (`Checker::take_type_uses` -> `semantic::
    /// file_type_tokens`). Resolution-driven — a function sharing a type's
    /// name can never appear here, and a type reached through `use` (which
    /// no editor-side scan can see) always does.
    tokens: Vec<TypeToken>,
}

/// `textDocument/semanticTokens/full`: every position where the last check
/// resolved a user-defined type or trait name — so a `defstruct`/`defenum`/
/// `deftrait` name reads as a type wherever it is *actually* used as one,
/// including a type imported through `use` (which no editor-side grammar can
/// resolve) and *never* at a same-named function's call sites (the checker
/// resolved those as calls, so no token was recorded there).
///
/// Served from the cached [`Analysis`]: in recover mode a document with type
/// errors still re-checks and refreshes its tokens on every change, so the
/// spans track the buffer; only a document that currently fails the *reader*
/// serves the spans of its last readable text.
fn handle_semantic_tokens(
    id: RequestId,
    params: serde_json::Value,
    analyses: &HashMap<Uri, Analysis>,
) -> Response {
    let result = (|| {
        let p: SemanticTokensParams = serde_json::from_value(params).ok()?;
        let uri = p.text_document.uri;
        let analysis = analyses.get(&uri)?;
        let data = encode(&analysis.tokens, semantic_token_index)
            .chunks_exact(5)
            .map(|c| SemanticToken {
                delta_line: c[0],
                delta_start: c[1],
                length: c[2],
                token_type: c[3],
                token_modifiers_bitset: c[4],
            })
            .collect();
        let result = SemanticTokens { result_id: None, data };
        Some(serde_json::to_value(result).expect("SemanticTokens always serializes"))
    })();
    // A document with no analysis yet answers `null`, which the spec allows and
    // clients read as "nothing to highlight" rather than an error.
    Response { id, result, error: None }
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
        let heap = &analysis.heap;
        let node = locate_node(heap, &analysis.body, &analysis.file, pos.line + 1, pos.character + 1)?;
        // The hovered node's own span, so the editor highlights exactly what
        // the type applies to. A degenerate span (macro-synthesized node)
        // would highlight a stray single character — omit the range instead.
        let range = heap.cons_loc(node).filter(|l| !l.is_degenerate()).map(|l| loc_to_range(&l));
        let text = hover_text(heap, node, &analysis.def_locs, &analysis.docs);
        let hover = Hover { contents: HoverContents::Scalar(MarkedString::String(text)), range };
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
        let heap = &analysis.heap;
        let node = locate_node(heap, &analysis.body, &analysis.file, pos.line + 1, pos.character + 1)?;
        let target = definition_target(heap, node, &analysis.def_locs)?;
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

/// `textDocument/completion`: unlike hover/goto-definition, this can't serve
/// from the cached [`Analysis`] — a completion request fires mid-edit, most
/// often while the document doesn't type-check (the very identifier being
/// typed is usually still unresolved). Instead it re-derives a best-effort
/// `Registry` from only the text *before* the identifier under the cursor:
/// - the in-progress identifier itself (`chars[prefix_start..offset]`) is cut
///   out entirely, so it can never appear as an unresolved reference and fail
///   the whole check;
/// - the resulting prefix is almost always missing some closing `)`s (the
///   user hasn't finished the form yet), so [`heuristically_close`] appends
///   whatever the paren-depth count says is missing before handing it to the
///   normal `Reader`/`Checker`/`Loader` pipeline (see [`candidates_for`]).
///
/// The checker is single-pass and mutates the registry as it goes, so even
/// if the patched text still fails to check for some other reason, whatever
/// got registered before that failure point is still usable — this is
/// deliberately lenient rather than requiring a clean check.
fn handle_completion(
    id: RequestId,
    params: serde_json::Value,
    docs: &HashMap<Uri, String>,
) -> Response {
    let result = (|| {
        let p: CompletionParams = serde_json::from_value(params).ok()?;
        let uri = p.text_document_position.text_document.uri.clone();
        let pos = p.text_document_position.position;
        let text = docs.get(&uri)?;
        let chars: Vec<char> = text.chars().collect();
        let offset = char_offset(&chars, pos);
        let prefix_start = prefix_start(&chars, offset);
        let prefix: String = chars[prefix_start..offset].iter().collect::<String>().to_lowercase();
        let mut truncated: String = chars[..prefix_start].iter().collect();
        // The cursor's own (1-based) position within `text` — computed
        // *before* `truncated` gains the placeholder below, so it names the
        // exact point the in-progress identifier starts at (unaffected by
        // anything appended after it) — the scope `completion_locals` needs
        // to search.
        let (line, col) = line_col_at(&chars, prefix_start);
        if needs_completion_placeholder(&truncated) {
            // No separator before the placeholder: `truncated` ends exactly
            // where the in-progress identifier began (its last char is
            // already a delimiter — that's how `prefix_start` stopped), so
            // appending directly puts the `(panic "")`'s own recorded
            // position at precisely `(line, col)`. That alignment is what
            // lets `locate_node` (greatest position `<= cursor`) pick the
            // placeholder itself as the cursor's node, so
            // `completion_locals` resolves scope *inside* the body/argument
            // slot being completed — a leading space used to shift it one
            // column past the cursor, silently excluding it and losing the
            // enclosing `let`/`lambda`/`labels` names whenever the slot was
            // the new scope's first form.
            truncated.push_str("(panic \"\")");
        }
        let patched = heuristically_close(&truncated);
        let overlay = build_overlay(docs, &uri);
        let candidates = candidates_for(uri.path().as_str(), &patched, overlay, line, col);
        let items: Vec<CompletionItem> = candidates
            .into_iter()
            .filter(|c| prefix.is_empty() || c.name.starts_with(&prefix))
            .map(|c| CompletionItem {
                label: c.name,
                kind: Some(completion_item_kind(c.kind)),
                detail: Some(c.detail),
                ..Default::default()
            })
            .collect();
        Some(serde_json::to_value(CompletionResponse::Array(items)).expect("CompletionResponse always serializes"))
    })();
    Response { id, result, error: None }
}

/// Re-checks `patched_text` as the content of `file` (see [`handle_completion`])
/// and returns every name reachable at `(line, col)`: [`completion_candidates`]
/// (module/`Registry`-level names) plus [`completion_locals`]
/// (`let`/`lambda`/`labels`/parameter and `match`-pattern names in lexical
/// scope there) — offered as [`CompletionKind::Variable`], matching how a
/// `Registry` variable is rendered.
///
/// The checker runs in error-recovery mode ([`Checker::set_recover`]): a
/// completion request's text is a document mid-edit and rarely type-checks, so
/// instead of skipping locals on any error (which lost them inside, e.g., a
/// non-catchall `match` arm, where truncating the source deletes the arms that
/// made the match exhaustive) the checker records errors and still returns a
/// best-effort partial form list for `completion_locals` to search. `result`
/// is `Err` only on a *reader* error, where the entry file's `(module ...)`
/// was never pushed; `take_pending().pop()` then yields either nothing or a
/// dependency's module, and `completion_locals` filters by `file` and returns
/// empty — so no gate is needed.
fn candidates_for(
    file: &str,
    patched_text: &str,
    overlay: HashMap<PathBuf, String>,
    line: u32,
    col: u32,
) -> Vec<CompletionCandidate> {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    checker.set_recover(true);

    let fs_file = FsPath::new(file);
    let dir = fs_file.parent().filter(|p| !p.as_os_str().is_empty()).map(FsPath::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let src_root = find_src_root(&dir).unwrap_or_else(|| dir.clone());
    let mut loader = Loader::new(src_root.clone());
    loader.set_overlay(overlay);
    let _ = loader.load_entry_src(&mut heap, &reader, &mut checker, &mut interp, fs_file, patched_text);

    let module_path = module_segs_for(fs_file, &src_root).unwrap_or_default();
    let mut candidates = completion_candidates(checker.registry(), &module_path);
    // Same extraction `diagnostics_for` uses for `Analysis.body`. Unlike there,
    // this heap is dropped at the end of the call — `completion_locals` returns
    // owned names, so nothing outlives it.
    let body = module_body(&heap, loader.take_pending().pop());
    for name in completion_locals(&heap, &body, file, line, col) {
        candidates.push(CompletionCandidate { name, kind: CompletionKind::Variable, detail: "local".to_string() });
    }
    candidates
}

/// The entry file's own top-level forms, unwrapped from the `(module PATH
/// BODY...)` the loader pushes last. `Loader::load_source_inner` recurses into
/// every `use` dependency (pushing each dependency's module first) before
/// pushing its own — see that function's doc comment — so the *last* pending
/// form is always this file's. In recover mode it is present even when the
/// document has type errors; on a reader error there is none at all, and both
/// `locate_node` and `completion_locals` read an empty body as "no answer".
fn module_body(heap: &Heap, last: Option<TopLevelForm>) -> Vec<TopLevelForm> {
    match last {
        // `(module PATH BODY...)` — drop the tag (`fields`) and the path.
        Some(tl) if core::op(heap, tl) == Some("module") => {
            core::fields(heap, tl).map(|f| f[1..].to_vec()).unwrap_or_default()
        }
        Some(other) => vec![other],
        None => Vec::new(),
    }
}

/// The 1-based `(line, col)` — matching [`Loc`]'s convention — of the char at
/// index `offset` in `chars`. Mirrors `read::reader::Cursor`'s own line/col
/// tracking (line starts at 1, column resets to 1 after each `\n`) so a
/// position computed here lines up with one recorded by the reader.
fn line_col_at(chars: &[char], offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut col = 1u32;
    for &c in &chars[..offset.min(chars.len())] {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn completion_item_kind(kind: CompletionKind) -> CompletionItemKind {
    match kind {
        CompletionKind::Function => CompletionItemKind::FUNCTION,
        CompletionKind::Method => CompletionItemKind::METHOD,
        CompletionKind::Type => CompletionItemKind::CLASS,
        CompletionKind::Variable => CompletionItemKind::VARIABLE,
        CompletionKind::Macro => CompletionItemKind::KEYWORD,
        CompletionKind::Trait => CompletionItemKind::INTERFACE,
        CompletionKind::Module => CompletionItemKind::MODULE,
    }
}

/// The char index (not byte index — matches `read::reader::Cursor`, which
/// indexes a `Vec<char>`) into `chars` that LSP's 0-based `(line, character)`
/// `pos` names. `character` is nominally a UTF-16 code unit count; treated
/// here as a plain char count like the rest of this file (`handle_hover`
/// etc. already make this same simplification by passing `pos.character + 1`
/// straight through as a `Loc` column) — good enough for the ordinary text
/// this MVP targets, wrong only for astral-plane characters before the
/// cursor on the same line.
fn char_offset(chars: &[char], pos: Position) -> usize {
    let mut offset = 0usize;
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, &c) in chars.iter().enumerate() {
        if line == pos.line {
            offset = i + (pos.character.saturating_sub(col)) as usize;
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        offset = i + 1;
    }
    offset.min(chars.len())
}

/// Same character class `read::reader::is_delimiter` uses (whitespace, list
/// parens, string/quote/quasiquote markers, `;`), just inverted and scanning
/// backward: the start of the identifier ending at `offset`, i.e. the first
/// index such that every char in `chars[start..offset]` is a non-delimiter.
fn prefix_start(chars: &[char], offset: usize) -> usize {
    let mut i = offset;
    while i > 0 {
        let c = chars[i - 1];
        if c.is_whitespace() || matches!(c, '(' | ')' | '"' | '\'' | '`' | ';') {
            break;
        }
        i -= 1;
    }
    i
}

/// Whether [`handle_completion`] should splice a placeholder expression onto
/// `truncated` before closing its parens — a further refinement on top of
/// [`heuristically_close`]'s pure paren-balancing, needed because truncating
/// right before the in-progress identifier can leave more than an unclosed
/// paren behind: it can leave a `let`/`lambda`/`labels`/`defun`/`defmethod`
/// body with *zero* forms, or a fixed-arity call with too *few* arguments —
/// either of which fails the check for a reason that has nothing to do with
/// the identifier being typed, and (before this existed) silently suppressed
/// `completion_locals`'s local-scope names for one of the single most common
/// completion moments: finishing the last statement of a function/`let`
/// body, or the last argument of a call.
///
/// The fix: splice in `(panic "")` — a [`crate::Type::Never`]-typed
/// expression, which (like a `break`/`return`) satisfies *any* expected type
/// or argument slot — right where the identifier was about to go. This is
/// only correct when that position is *already* known to be "some
/// expression is expected here": if the in-progress identifier is instead
/// the very *first* token inside a freshly-opened list (`(my⏐` — most often a
/// function/macro name being typed as a new call's head), the placeholder
/// would itself become that list's *callee* (`((panic ""))`), which fails
/// with "value is not callable" — a regression `heuristically_close` alone
/// never had, since an empty list (`()`) simply reads as `Unit`. So: only
/// when `truncated`'s last non-whitespace character is *not* `(` (meaning at
/// least one sibling token already precedes this position, so it's an
/// argument/body-statement slot, not a list head) does splicing the
/// placeholder in make things strictly more often correct rather than
/// introducing a new failure mode.
///
/// Even with this, multi-argument truncation (typing argument *K* of *N*
/// when *N* - *K* further arguments existed after the cursor and got
/// discarded along with everything past it) is still an accepted gap — this
/// only accounts for the one in-progress slot, not every argument that would
/// have followed it.
fn needs_completion_placeholder(truncated: &str) -> bool {
    !matches!(truncated.trim_end().chars().last(), None | Some('('))
}

/// Best-effort: appends whatever `)` are needed to close every `(` left open
/// in `text`, so a document truncated mid-form (see [`handle_completion`])
/// still reads. Mirrors just enough of `read::reader`'s tokenizer to keep the
/// paren count honest — skips line comments (`;` to EOL), block comments
/// (`#| ... |#`, nesting allowed), string literals (with backslash escapes),
/// and character literals (`#\x`, `#\Name`) — none of those should have their
/// `(`/`)` counted, e.g. a stray `#\(` is one character, not an open paren.
/// A text with more `)` than `(` (a genuine syntax error, not just
/// "unfinished") is left alone: the depth count is clamped at 0 rather than
/// trying to fix that.
fn heuristically_close(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut depth: i32 = 0;
    while i < chars.len() {
        match chars[i] {
            ';' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            '#' if chars.get(i + 1) == Some(&'|') => {
                i += 2;
                let mut nesting = 1;
                while i < chars.len() && nesting > 0 {
                    if chars[i] == '#' && chars.get(i + 1) == Some(&'|') {
                        nesting += 1;
                        i += 2;
                    } else if chars[i] == '|' && chars.get(i + 1) == Some(&'#') {
                        nesting -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            '#' if chars.get(i + 1) == Some(&'\\') => {
                i += 2;
                if i < chars.len() {
                    let first = chars[i];
                    i += 1;
                    if first.is_alphabetic() {
                        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-') {
                            i += 1;
                        }
                    }
                }
            }
            '(' => {
                depth += 1;
                i += 1;
            }
            ')' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    let mut out = text.to_string();
    for _ in 0..depth.max(0) {
        out.push(')');
    }
    out
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

/// `Loc`'s line/col are 1-based; LSP's `Position` is 0-based. Both ends are
/// exclusive-exclusive-compatible: `Loc::end_*` is already exclusive, same as
/// LSP's `Range.end`, so the span converts directly. A *degenerate* `Loc`
/// (end unknown — e.g. a node synthesized by macro expansion, or a read
/// error's point) falls back to highlighting a single character, the
/// pre-span behavior.
fn loc_to_range(loc: &Loc) -> Range {
    let line = loc.line.saturating_sub(1);
    let col = loc.col.saturating_sub(1);
    if loc.is_degenerate() {
        return Range::new(Position::new(line, col), Position::new(line, col + 1));
    }
    Range::new(
        Position::new(line, col),
        Position::new(loc.end_line.saturating_sub(1), loc.end_col.saturating_sub(1)),
    )
}

fn doc_start_range() -> Range {
    Range::new(Position::new(0, 0), Position::new(0, 1))
}

#[cfg(test)]
mod completion_helper_tests {
    use super::*;

    #[test]
    fn heuristically_close_appends_missing_parens() {
        assert_eq!(heuristically_close("(defun f () i32 (+ 1 "), "(defun f () i32 (+ 1 ))");
        assert_eq!(heuristically_close("(defun f () i32 1)"), "(defun f () i32 1)");
    }

    #[test]
    fn heuristically_close_ignores_parens_inside_a_string() {
        // The `(` after the opening `"` is string content, not a real open
        // paren — so only the two *actual* unclosed parens (`(defun` and
        // `(str-new`) get appended closers, and the never-closed string is
        // left exactly as-is (this function only balances parens).
        assert_eq!(heuristically_close(r#"(defun f () str (str-new "("#), "(defun f () str (str-new \"(".to_string() + "))");
    }

    #[test]
    fn heuristically_close_ignores_parens_inside_a_char_literal() {
        assert_eq!(heuristically_close(r"(defun f () char #\("), r"(defun f () char #\()");
    }

    #[test]
    fn heuristically_close_ignores_parens_inside_a_line_comment() {
        assert_eq!(heuristically_close("(defun f () i32 ; unclosed (\n  1"), "(defun f () i32 ; unclosed (\n  1)");
    }

    #[test]
    fn heuristically_close_does_not_go_negative_on_an_extra_close_paren() {
        assert_eq!(heuristically_close("(defun f () i32 1))"), "(defun f () i32 1))");
    }

    #[test]
    fn prefix_start_finds_the_identifier_ending_at_offset() {
        let chars: Vec<char> = "(add my-va".chars().collect();
        assert_eq!(prefix_start(&chars, chars.len()), 5);
    }

    #[test]
    fn prefix_start_is_the_offset_itself_right_after_a_delimiter() {
        let chars: Vec<char> = "(add ".chars().collect();
        assert_eq!(prefix_start(&chars, chars.len()), chars.len());
    }

    #[test]
    fn char_offset_finds_a_position_on_a_later_line() {
        let chars: Vec<char> = "(a)\n(b c".chars().collect();
        // Line 1 (0-based), character 4: right after "(b c".
        assert_eq!(char_offset(&chars, Position::new(1, 4)), chars.len());
    }

    #[test]
    fn char_offset_finds_a_position_on_the_first_line() {
        let chars: Vec<char> = "(add 1 2)".chars().collect();
        assert_eq!(char_offset(&chars, Position::new(0, 4)), 4);
    }

    #[test]
    fn line_col_at_finds_a_position_on_the_first_line() {
        let chars: Vec<char> = "(add 1 2)".chars().collect();
        // Offset 4 is the space right after "add" — 1-based column 5.
        assert_eq!(line_col_at(&chars, 4), (1, 5));
    }

    #[test]
    fn line_col_at_finds_a_position_on_a_later_line() {
        let chars: Vec<char> = "(a)\n(b c)".chars().collect();
        // Offset 7 is `c` on the second (1-based line 2) line.
        assert_eq!(line_col_at(&chars, 7), (2, 4));
    }

    #[test]
    fn needs_completion_placeholder_is_false_right_after_a_fresh_open_paren() {
        // `my` about to be typed as a new call's head/first token — must not
        // get a placeholder (it would become that list's unresolvable callee).
        assert!(!needs_completion_placeholder("(defun f () i32 ("));
    }

    #[test]
    fn needs_completion_placeholder_is_true_after_a_prior_sibling() {
        // A `let` body with one binding already closed — the identifier
        // about to be typed is a body statement, not a list head.
        assert!(needs_completion_placeholder("(defun f () i32 (let ((n 1)) "));
        // A call with one argument already present.
        assert!(needs_completion_placeholder("(+ mylocal "));
    }

    #[test]
    fn needs_completion_placeholder_is_false_at_the_very_start_of_the_document() {
        assert!(!needs_completion_placeholder(""));
        assert!(!needs_completion_placeholder("   "));
    }

    /// End-to-end regression for the motivating bug, through the real
    /// `candidates_for` entry point (recover mode + no `result.is_ok()` gate):
    /// the patched text `handle_completion` produces for a completion request
    /// inside a *non-catchall* `match` arm — the trailing arms truncated away,
    /// a `(panic "")` placeholder where the identifier was being typed. Before
    /// recovery this failed the whole check (non-exhaustive match) and offered
    /// no locals; now the arm's binding `x` and the parameter `o` appear.
    #[test]
    fn candidates_for_offers_locals_inside_a_truncated_non_catchall_match_arm() {
        // A path under a directory with no `typelisp.toml` above it, so the
        // loader treats it as a standalone entry (its text is passed directly,
        // so the file need not exist on disk).
        let file = "/tmp/typelisp-lsp-recover-test/f.typl";
        let patched = "(defun f ((o Option<i32>)) i32 (match o ((Some x) (panic \"\"))))";
        // The placeholder sits at 1-based column 51 (the `(` of `(panic "")`).
        let candidates = candidates_for(file, patched, HashMap::new(), 1, 51);
        let locals: Vec<&str> = candidates
            .iter()
            .filter(|c| c.kind == CompletionKind::Variable)
            .map(|c| c.name.as_str())
            .collect();
        assert!(locals.contains(&"o"), "parameter offered, got {:?}", locals);
        assert!(locals.contains(&"x"), "match-arm binding offered, got {:?}", locals);
    }
}
