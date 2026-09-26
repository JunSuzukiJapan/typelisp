//! The `typl` CLI: with a file argument, loads and runs it (and its `use`
//! dependencies — see `typelisp::project` for the file-to-module mapping);
//! with no argument, a read-eval-print loop over the `Reader` -> `Checker` ->
//! `Interp` pipeline, using `rustyline` for Emacs-style line editing/history
//! (Ctrl+P/Ctrl+N to move through history, Ctrl+R to search it, etc. — all
//! `rustyline`'s default `EditMode::Emacs` bindings, matching bash/readline).

use std::cell::RefCell;
use std::path::{Path as FsPath, PathBuf};
use std::rc::Rc;

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use typelisp::project::{find_src_root, load_file_flat, Loader};
use typelisp::*;

const PROMPT_PRIMARY: &str = "typl> ";
const PROMPT_CONTINUE: &str = "...   ";

/// Default cons-arena capacity (cells), used when `--heap-cells` is absent.
/// The arena is fixed-size — sized up front, never grown (see
/// `typelisp_mem::Heap`'s module doc / `Error::HeapExhausted`); this is the
/// same `1 << 16` the runtime paths have always allocated.
const DEFAULT_HEAP_CELLS: usize = 1 << 16;

fn main() -> rustyline::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A global `--heap-cells N` (or `--heap-cells=N`) sizes the fixed cons
    // arena every run mode below allocates; strip it (and its value) first so
    // neither the subcommand dispatch nor the file-name search below trips
    // over its numeric argument.
    let (heap_cells, args) = parse_heap_cells(args);
    // A global, repeatable `--feature NAME` (or `--feature=NAME`) adds a
    // `#+`/`#-` reader-conditional feature on top of the host platform
    // defaults (see `Features::host`) — typelisp's equivalent of pushing onto
    // CL's `*features*` before loading.
    let (features, args) = parse_features(args);
    // A global `--image FILE` (or `--image=FILE`) starts from a dump written by
    // `(dump ...)` instead of the prelude and island compiled into this binary.
    let (image, args) = parse_image(args);
    // The first non-flag argument names a source file to run;
    // with none, start the REPL.
    //
    // Everything from the file name onward is the *script's* command line,
    // and `(command-line-args)` must answer with it rather than with `typl`'s
    // own `std::env::args()` — otherwise the same source would see a
    // different vector run by `typl` than run as an AOT executable, where
    // element 0 is the program and element 1 the first argument. Installing
    // it here, before any user code, is what makes the two agree; see
    // `typelisp_rt::sys_builtin::COMMAND_LINE_ARGS`.
    if let Some(pos) = args.iter().position(|a| !a.starts_with("--")) {
        typelisp_rt::sys_builtin::set_command_line_args(args[pos..].to_vec());
        std::process::exit(run_file(&args[pos], heap_cells, features, image));
    }
    // The REPL has no script, so its command line is just `typl` itself —
    // set explicitly rather than left to the process argv, which would leak
    // `--heap-cells` and friends into a program's view of its arguments.
    typelisp_rt::sys_builtin::set_command_line_args(vec!["typl".to_string()]);
    repl(heap_cells, features, image)
}

/// Parses a global `--image FILE` / `--image=FILE` flag out of `args`,
/// returning the path and the remaining arguments with the flag removed.
///
/// Exits with a diagnostic on a missing value, for the same reason
/// [`parse_heap_cells`] does: a typo here silently starts a different
/// environment than the one asked for. A later occurrence wins.
fn parse_image(args: Vec<String>) -> (Option<PathBuf>, Vec<String>) {
    let mut image = None;
    let mut rest = Vec::with_capacity(args.len());
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--image" {
            match it.next() {
                Some(v) => image = Some(PathBuf::from(v)),
                None => {
                    eprintln!("--image: needs a path to a dump");
                    std::process::exit(1);
                }
            }
        } else if let Some(v) = a.strip_prefix("--image=") {
            image = Some(PathBuf::from(v));
        } else {
            rest.push(a);
        }
    }
    (image, rest)
}

/// Parses zero or more `--feature NAME` / `--feature=NAME` flags out of
/// `args`, returning the accumulated feature names (in the order given) and
/// the remaining arguments with every occurrence removed. A later duplicate
/// of the same name is harmless — `Features::with` inserts into a set.
fn parse_features(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut features = Vec::new();
    let mut rest = Vec::with_capacity(args.len());
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--feature" {
            match it.next() {
                Some(v) => features.push(v),
                None => {
                    eprintln!("--feature: needs a feature name");
                    std::process::exit(1);
                }
            }
        } else if let Some(v) = a.strip_prefix("--feature=") {
            features.push(v.to_string());
        } else {
            rest.push(a);
        }
    }
    (features, rest)
}

/// Parses a global `--heap-cells N` / `--heap-cells=N` flag out of `args`,
/// returning the requested cons-arena capacity (defaulting to
/// [`DEFAULT_HEAP_CELLS`]) and the remaining arguments with the flag and its
/// value removed. Exits the process with a diagnostic on a missing or invalid
/// value — the flag sizes a one-shot allocation, so a typo is better caught
/// before any work than silently ignored. A later occurrence wins.
fn parse_heap_cells(args: Vec<String>) -> (usize, Vec<String>) {
    let mut capacity = DEFAULT_HEAP_CELLS;
    let mut rest = Vec::with_capacity(args.len());
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--heap-cells" {
            match it.next() {
                Some(v) => capacity = parse_heap_cells_value(&v),
                None => {
                    eprintln!("--heap-cells: needs a positive integer (number of cons cells)");
                    std::process::exit(1);
                }
            }
        } else if let Some(v) = a.strip_prefix("--heap-cells=") {
            capacity = parse_heap_cells_value(v);
        } else {
            rest.push(a);
        }
    }
    (capacity, rest)
}

/// Parses the value half of `--heap-cells`; exits with a diagnostic if it is
/// not a positive integer (zero is rejected — an empty arena exhausts on the
/// first `cons`, so it is never a useful request) or if the arena could not
/// fit in any address space ([`typelisp::Heap::max_capacity`]).
///
/// There is no smaller ceiling: `heap-info` reports every count as an `int`,
/// so any arena this process can hold is one `room` can describe. A request
/// the machine merely lacks the memory for fails at the allocation itself.
fn parse_heap_cells_value(v: &str) -> usize {
    let max = typelisp::Heap::max_capacity();
    match v.parse::<usize>() {
        Ok(n) if n > max => {
            eprintln!(
                "--heap-cells: `{}` cells would not fit in this machine's address space ({} is the maximum)",
                v, max
            );
            std::process::exit(1);
        }
        Ok(n) if n > 0 => n,
        _ => {
            eprintln!("--heap-cells: `{}` is not a positive integer", v);
            std::process::exit(1);
        }
    }
}

/// Brings up the environment a session runs in, and starts recording what that
/// session defines.
///
/// Either the prelude and compiler island compiled into this binary (the
/// island always: interp-closure removal Stage 5 means a closure defined at
/// runtime is JIT-compiled at definition time rather than falling back to an
/// interpreted one), or, with `--image`, a dump written by `(dump ...)`.
///
/// The recording baseline is taken here, once the environment is complete and
/// before a line of user code runs — everything after this point is the
/// session's, and everything before it belongs to a unit already written down.
/// It costs ~14ms over ~630 entries (`typl-bench-prelude`), which is what
/// `(dump ...)` costs a session that never calls it.
fn load_environment(
    heap: &mut Heap,
    checker: &mut Checker,
    interp: &mut Interp,
    image: Option<PathBuf>,
) -> Result<(), String> {
    match image {
        Some(path) => {
            let bytes = std::fs::read(&path).map_err(|e| format!("--image {}: {}", path.display(), e))?;
            typelisp::compile::dump::load_image(
                heap,
                checker,
                interp,
                std::borrow::Cow::Owned(bytes),
                &path.display().to_string(),
            )?;
        }
        None => {
            load_prelude(heap, checker, interp);
            typelisp::load_compiler_aot(heap, checker, interp);
        }
    }
    interp.start_recording(checker.signature(heap)?);
    Ok(())
}

/// Load and execute `file` (and, transitively, whatever its `use`s pull in).
/// Returns the process exit code. Top-level expression results are not
/// printed — printing is the REPL's affordance; a script prints via `print`.
fn run_file(file: &str, heap_cells: usize, features: Vec<String>, image: Option<PathBuf>) -> i32 {
    let mut heap = Heap::with_capacity(heap_cells);
    let reader = Reader::with_features(features);
    let mut checker = Checker::new();
    checker.set_redef_policy(parse_redef_policy());
    let mut interp = Interp::new();
    if let Err(e) = load_environment(&mut heap, &mut checker, &mut interp, image) {
        eprintln!("error: {}", e);
        return 1;
    }

    let file = PathBuf::from(file);
    let dir = file.parent().filter(|p| !p.as_os_str().is_empty()).map(FsPath::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let src_root = find_src_root(&dir).unwrap_or(dir);
    let mut loader = Loader::new(src_root.clone());

    let result = loader.load_entry(&mut heap, &reader, &mut checker, &mut interp, &file);
    for w in checker.take_warnings() {
        eprintln!("{}", w);
    }
    if let Err(e) = result {
        eprintln!("error: {}", e);
        return 1;
    }
    // Place a runtime `(eval ...)` in the script's own file-derived module, so
    // the script's module-scoped globals/functions resolve from an eval'd form
    // (and eval-defined names register there) — the load phase checked the
    // file's forms in this same module, but left the checker at the root. A
    // path that can't derive a module (shouldn't happen — the load above
    // already used it) falls back to the root.
    let entry_ns = typelisp::project::module_segs_for(&file, &src_root).unwrap_or_default();
    checker.set_current_ns(entry_ns);
    // Hand the interpreter a shared handle to the now-fully-loaded checker so
    // any top-level `(eval ...)` in the queued forms can type-check the code
    // it's given against this program's own global environment
    // (`Interp::eval_form`). Done here, after all load/check-phase uses of
    // `checker` are finished and `checker` is moved into the `RefCell` — the
    // exec loop below holds no checker borrow, so `eval`'s own
    // `borrow_mut()` never conflicts. `run_file` never touches `checker`
    // again, so moving it in is free (unlike the REPL, which keeps checking
    // after — see `try_run_pending`).
    let checker = Rc::new(RefCell::new(checker));
    interp.set_checker(Rc::clone(&checker));
    // All read roots are popped by now (the loader's per-file discipline), so
    // executing the queued forms — including heap-touching `defvar`
    // initializers and top-level expressions — is safe.
    for tl in loader.take_pending() {
        if let Err(e) = interp.exec(&mut heap, tl) {
            eprintln!("error: {}", e);
            return 1;
        }
    }
    0
}

fn repl(heap_cells: usize, features: Vec<String>, image: Option<PathBuf>) -> rustyline::Result<()> {
    let mut heap = Heap::with_capacity(heap_cells);
    let reader = Reader::with_features(features);
    // The checker lives behind a shared `RefCell` so the interpreter can reach
    // it to type-check a runtime `(eval ...)` form (`Interp::eval_form`), while
    // the REPL keeps checking each new line through the same cell. Every
    // per-line use borrows it only during the check phase, releasing before
    // the exec phase — see `try_run_pending`'s borrow discipline.
    let checker = Rc::new(RefCell::new(Checker::new()));
    checker.borrow_mut().set_redef_policy(parse_redef_policy());
    let mut interp = Interp::new();
    if let Err(e) = load_environment(&mut heap, &mut checker.borrow_mut(), &mut interp, image) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
    // Wire the interpreter to the checker now that loading is done, so
    // `(eval ...)` at the REPL type-checks against the live environment.
    interp.set_checker(Rc::clone(&checker));
    // `use` in the REPL resolves files against the current directory (or the
    // project root if a manifest is found above it).
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let src_root = find_src_root(&cwd).unwrap_or(cwd);
    let mut loader = Loader::new(src_root);
    let mut rl = DefaultEditor::new()?;
    let hist_path = history_path();
    let _ = rl.load_history(&hist_path);

    let mut pending = String::new();
    loop {
        let prompt = if pending.is_empty() { PROMPT_PRIMARY } else { PROMPT_CONTINUE };
        // Native: while this thread waits for a keystroke it holds nothing
        // another thread's collection would have to wait for.
        match heap.native(|| rl.readline(prompt)) {
            Ok(line) => {
                let trimmed = line.trim();
                if pending.is_empty() && (trimmed == ":quit" || trimmed == ":exit") {
                    break;
                }
                let _ = rl.add_history_entry(line.as_str());
                // The REPL's own half of `dribble`: the line as typed. The
                // other two doors a session's output leaves by are inside the
                // runtime — see `typelisp_abi::dribble`'s module docs.
                typelisp_abi::dribble::note(&line);
                typelisp_abi::dribble::note("\n");
                pending.push_str(&line);
                pending.push('\n');
                try_run_pending(&mut heap, &reader, &checker, &mut interp, &mut loader, &mut pending);
            }
            Err(ReadlineError::Interrupted) => {
                pending.clear();
                println!();
            }
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("readline error: {}", e);
                break;
            }
        }
    }
    let _ = rl.save_history(&hist_path);
    Ok(())
}

/// Parses `--on-redefine=warn|error|silent` from the process arguments
/// (defaults to `RedefPolicy::Warn` if absent or unrecognized — no `clap`
/// dependency for one flag, hand-rolled like the rest of this small CLI).
fn parse_redef_policy() -> RedefPolicy {
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--on-redefine=") {
            return match v {
                "warn" => RedefPolicy::Warn,
                "error" => RedefPolicy::Error,
                "silent" => RedefPolicy::Silent,
                other => {
                    eprintln!("unknown --on-redefine value `{}` (expected warn/error/silent), using warn", other);
                    RedefPolicy::Warn
                }
            };
        }
    }
    RedefPolicy::Warn
}

/// `$HOME/.typl_history`, falling back to the current directory if `HOME` is
/// unset.
fn history_path() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home).join(".typl_history"),
        _ => PathBuf::from(".typl_history"),
    }
}

/// Read+check+execute whatever is currently in `pending`, **one form at a
/// time**: each form is read, checked and run before the next one is read
/// (`Reader::forms_in`), which is what a Lisp REPL has always meant by
/// reading from a stream and is what lets a form affect how the text after it
/// is read.
///
/// On a recoverable "need more input" error, `pending` is cut back to the
/// unread tail — the forms already read have already *run*, so leaving them
/// in `pending` would run them a second time when the next line arrives. On
/// success or a real error, clears `pending`.
///
/// GC-root discipline: the reader's roots protect the raw forms across
/// checking (macro expansion conses, and can collect), and are the only thing
/// doing so — `check_form`'s output never retains the raw `Value`. They
/// accumulate interleaved with the checked forms' own roots and are all
/// popped together at the end; nothing pops in between, so the stack stays
/// LIFO. `Interp::exec` is balanced on that stack (it roots a definition
/// permanently, on the other stack), so running a form mid-batch leaves the
/// discipline intact.
fn try_run_pending(
    heap: &mut Heap,
    reader: &Reader,
    checker: &RefCell<Checker>,
    interp: &mut Interp,
    loader: &mut Loader,
    pending: &mut String,
) {
    let mark = heap.root_count();
    let mut forms = reader.forms_in("<stdin>", pending);
    // Where the reader stopped after the last *completed* form, in characters.
    // Only used on the "need more input" path, to decide what is left over.
    let mut consumed = 0usize;
    let mut fatal: Option<Error> = None;
    let mut incomplete = false;

    loop {
        let (v, loc) = match forms.next_form_with(heap, Some(&*interp)) {
            Ok(Some(pair)) => pair,
            Ok(None) => break,
            Err(e) => {
                if is_incomplete(&e) {
                    incomplete = true;
                } else {
                    fatal = Some(e);
                }
                break;
            }
        };

        // Load this form's `use` dependencies before checking it, so
        // `check_use` finds them in the registry (see `typelisp::project`). A
        // nested load pushes and pops its own read roots strictly above ours,
        // so the root-stack discipline is undisturbed.
        if let Err(e) = loader.load_uses_in(heap, reader, &mut checker.borrow_mut(), interp, &[v]) {
            fatal = Some(e);
            break;
        }
        // A loaded module's own forms run before the form that `use`d it —
        // its definitions and `defvar` initializers must exist by the time
        // this form does anything.
        let mut load_failed = false;
        for tl in loader.take_pending() {
            if let Err(e) = interp.exec(heap, tl) {
                eprintln!("error: {}", e);
                load_failed = true;
                break;
            }
        }
        if load_failed {
            break;
        }

        // Borrow the checker only for the check itself, so the borrow is
        // released before any `interp.exec` below — a form's exec must be
        // free to let a runtime `(eval ...)` re-borrow the checker
        // (`Interp::eval_form`).
        let result = checker.borrow_mut().check_form_at(heap, &*interp, v, Some(loc));
        for w in checker.borrow_mut().take_warnings() {
            eprintln!("{}", w);
        }
        let tl = match result {
            Ok(tl) => tl,
            Err(e) => {
                fatal = Some(e);
                break;
            }
        };
        // `(load "path")` loads inline, fasl-preferred, into the current
        // (root) environment — resolved relative to the process cwd.
        // (A `(load)`ed file whose *own* top-level contains `(eval ...)`
        // is the one corner this borrow doesn't cover — `load_file_flat`
        // execs inline while this `borrow_mut` is held; an ordinary
        // module/fasl load never top-level-`eval`s, so it doesn't arise
        // in practice.)
        if let Some(path) = typelisp::project::load_path_of(heap, tl) {
            if let Err(e) = load_file_flat(heap, reader, &mut checker.borrow_mut(), interp, FsPath::new("."), &path) {
                fatal = Some(e);
                break;
            }
        } else {
            match interp.exec(heap, tl) {
                // The REPL's other half of `dribble`: the value echoed back.
                Ok(Some(v)) => {
                    let chk = checker.borrow();
                    let text = format_value(heap, chk.registry(), chk.expr_type(), &v);
                    typelisp_abi::dribble::note(&text);
                    typelisp_abi::dribble::note("\n");
                    println!("{}", text);
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("error: {}", e);
                    break;
                }
            }
        }
        consumed = forms.pos();
    }

    // Anything the dependency loads warned about: the per-form drain below
    // only covers this batch's own forms, and a `use` that failed never
    // reached it.
    for w in checker.borrow_mut().take_warnings() {
        eprintln!("{}", w);
    }
    while heap.root_count() > mark {
        heap.pop_root();
    }

    if incomplete {
        // Keep only what has not been read. Everything before `consumed` has
        // already run; re-reading it when the rest of the form arrives would
        // run it twice.
        *pending = pending.chars().skip(consumed).collect();
        return;
    }
    pending.clear();
    if let Some(e) = fatal {
        eprintln!("error: {}", e);
    }
}

/// True if `e` signals "ran out of input mid-form" (need another line)
/// rather than a real syntax error.
fn is_incomplete(e: &Error) -> bool {
    // `e` may be wrapped in `Error::At(loc, ..)`; match on the underlying kind.
    matches!(
        e.kind(),
        Error::IllegalEndWhileReadingList | Error::IllegalEndOfString | Error::IllegalEndOfEscapeSequence
    )
}

/// Format a value for REPL output, in the reader's own syntax where
/// possible (so the printed form can be pasted back in).
///
/// `ty` is the form's checked type, when the checker has one: a
/// niche-represented `Option<T>` (`check::repr::Repr::Niche`) is a bare
/// word the value alone cannot be recognized by, and prints as `none` /
/// `(some ...)` only because the type says so. `Option<Sexpr>` is the
/// documented exception (functions.md §15): S-expression data prints as
/// the S-expression, the empty list as `()`.
fn format_value(heap: &Heap, reg: &Registry, ty: Option<&Type>, v: &Value) -> String {
    if let Some(ty) = ty {
        let repr = check::repr::Repr::of_by(ty, &|p| reg.type_def(p).map(|d| d.kind));
        if repr.niche_payload().is_some_and(|p| *p != check::repr::Repr::Sexpr) {
            return match v {
                Value::Empty => "none".to_string(),
                _ => format!("(some {})", format_value(heap, reg, None, v)),
            };
        }
    }
    match v {
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => format!("#\\{}", c),
        Value::Empty => "()".to_string(),
        sv => format_sexpr(heap, reg, *sv),
    }
}

/// Format a `mem::Value` recursively, in the reader's own syntax. `reg` recovers an enum box's
/// variant *name* (the runtime stores only the index) so `(some 1)` prints
/// the way it always has.
fn format_sexpr(heap: &Heap, reg: &Registry, v: Value) -> String {
    match v {
        Value::Empty => "()".to_string(),
        Value::Int(i) => i.to_string(),
        // `Sexpr::f64`, a `defstruct`/`Vector<T>`/`cons-cell<K,V>` instance,
        // a `HashTable<K,V>`, and an enum value are all heap-boxed
        // (`Value::Boxed`, see `BoxedObj`) — `heap.is_struct`/
        // `is_hashtable`/`is_enum` tell them apart. A boxed struct prints
        // positionally (no field names at runtime), recursing through this
        // same function for each field.
        Value::Boxed(id) if heap.is_struct(id) => {
            let key = type_key::heap_type_key(heap, id).expect("a struct box has a type name").to_string();
            let parts: Vec<String> = (0..heap.struct_field_count(id))
                .map(|i| format_field(heap, reg, &key, None, i, heap.struct_field(id, i)))
                .collect();
            format!("#<{} {}>", type_key::heap_type_path(heap, id).expect("a struct box has a type name"), parts.join(" "))
        }
        // An enum value prints as its variant name applied to its fields —
        // `(some 1)` / a bare `none` — the exact shape the old
        // `RtValue::Data` arm produced. The box stores the *type* name and
        // variant *index*; the variant's name lives only in the checker's
        // registry, looked up by re-parsing the stored `Path` string.
        Value::Boxed(id) if heap.is_enum(id) => {
            let type_path = type_key::heap_type_path(heap, id).expect("an enum box has a type name");
            let variant = heap.enum_variant(id);
            let name = reg
                .type_def(&type_path)
                .and_then(|d| d.variants.get(variant))
                .map(|v| v.name.clone())
                .unwrap_or_else(|| "<unknown-variant>".to_string());
            if heap.enum_field_count(id) == 0 {
                name
            } else {
                let key = type_key::heap_type_key(heap, id).expect("an enum box has a type name").to_string();
                let parts: Vec<String> = (0..heap.enum_field_count(id))
                    .map(|i| format_field(heap, reg, &key, Some(variant), i, heap.enum_field(id, i)))
                    .collect();
                format!("({} {})", name, parts.join(" "))
            }
        }
        Value::Boxed(id) if heap.is_hashtable(id) => format!("#<hashtable count={}>", heap.hashtable_count(id)),
        // A `Scope<V>` with heap-repr `V` is boxed too since Stage 8, and
        // prints the same way a native-`V` scope always did.
        Value::Boxed(id) if heap.is_scope(id) => format!("#<scope depth={}>", heap.scope_frame_count(id)),
        // A closure is a boxed value, printed opaquely — and identically
        // whether it was compiled or is being tree-walked
        // (`BoxedObj::CompiledClosure` / `BoxedObj::Closure`). Which one a
        // given `lambda` became is a matter of whether the JIT took it, not
        // anything about the value, so printing must not tell them apart.
        Value::Boxed(id) if heap.is_compiled_closure(id) || heap.is_closure(id) => "#<closure>".to_string(),
        // The other kind of function value: a built-in reified as a value,
        // which prints as its name rather than opaquely — there is nothing
        // else to show, and the name is exactly what identifies it.
        Value::Boxed(id) if heap.is_builtin_fn(id) => match heap.builtin_fn_recv(id) {
            None => format!("#<builtin {}>", heap.builtin_fn_name(id)),
            Some(pid) => format!(
                "#<builtin {}::{}>",
                crate::types::path_from_id(heap, pid),
                heap.builtin_fn_name(id)
            ),
        },
        // CL prints a `random-state` unreadably (implementation-defined) too —
        // its seed is an implementation detail, not part of the value.
        Value::Boxed(id) if heap.is_random_state(id) => "#<random-state>".to_string(),
        Value::Boxed(id) if heap.is_bignum(id) => heap.bignum_value(id).to_string(),
        Value::Boxed(id) if heap.is_ratio(id) => {
            let r = heap.ratio_value(id);
            format!("{}/{}", r.numer(), r.denom())
        }
        // A narrow integer inside a `Sexpr` prints as the number it is — the
        // same text an `i32` of that value gives. The box carries the width so
        // the *type* survives, not to make the number look different.
        Value::Boxed(id) if heap.narrow_box(id).is_some() => {
            heap.narrow_box(id).expect("just tested").value.to_string()
        }
        // Positively `is_f64`/`is_f32`: the bare fall-through these replace
        // read every other box kind as an `f64`, which the accessor answers
        // with a panic.
        Value::Boxed(id) if heap.is_f64(id) => format_f64(heap.f64_value(id)),
        Value::Boxed(id) if heap.is_f32(id) => format_f32(heap.f32_value(id)),
        Value::Boxed(_) => "#<unprintable>".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => format!("#\\{}", c),
        Value::Symbol(id) => heap.symbol_name(id).to_string(),
        Value::Str(id) => format!("{:?}", heap.string(id)),
        Value::Path(id) => heap
            .path_segments(id)
            .iter()
            .map(|s| heap.symbol_name(*s))
            .collect::<Vec<_>>()
            .join("::"),
        Value::Cons(_) => format_list(heap, reg, v),
    }
}

/// One field of a struct or enum box — as the value it is, unless the
/// registry says the field's type (the definition's, instantiated by the
/// box's own key) is a niche-represented `Option`, which the word cannot
/// say for itself. The same question `typelisp_print` asks its
/// `PrintEnv`, answered here from the checker's registry directly.
fn format_field(heap: &Heap, reg: &Registry, key: &str, variant: Option<usize>, index: usize, f: Value) -> String {
    let (base, _) = type_key::split_key(key);
    let path = Path::from_segments(base.split("::").map(str::to_string).collect());
    let args = typelisp_mem::type_key_args(key);
    // A `Vector<T>`'s fields are its elements, every one a `T` — the
    // registry records no fields for it, so the template is spelled here.
    let niched = if types::path_is_builtin(&path, "vector") {
        typelisp_mem::option_prints_wrapped(&typelisp_mem::instantiate_key_template("$0", &args))
    } else {
        reg.type_def(&path).is_some_and(|def| {
            let fields = match variant {
                Some(v) => def.variants.get(v).map(|v| &v.fields),
                None => def.variants.first().map(|v| &v.fields),
            };
            fields.and_then(|fs| fs.get(index)).is_some_and(|field_ty| {
                let template = type_key::field_key_template(&def.params, field_ty);
                typelisp_mem::option_prints_wrapped(&typelisp_mem::instantiate_key_template(&template, &args))
            })
        })
    };
    if !niched {
        return format_sexpr(heap, reg, f);
    }
    match f {
        Value::Empty => "none".to_string(),
        _ => format!("(some {})", format_sexpr(heap, reg, f)),
    }
}

fn format_list(heap: &Heap, reg: &Registry, mut v: Value) -> String {
    let mut parts = Vec::new();
    loop {
        match v {
            Value::Cons(_) => {
                // Safe: `v` was just matched as `Cons`, so `car`/`cdr` cannot
                // return `Error::NotACons` here.
                let car = heap.car(v).expect("cons car");
                parts.push(format_sexpr(heap, reg, car));
                v = heap.cdr(v).expect("cons cdr");
            }
            Value::Empty => return format!("({})", parts.join(" ")),
            other => return format!("({} . {})", parts.join(" "), format_sexpr(heap, reg, other)),
        }
    }
}

/// Render an `f64` guaranteeing a decimal point, so e.g. `2.0` doesn't print
/// as `2` (which would be confusable with an `Int`).
fn format_f64(f: f64) -> String {
    if f.is_finite() && f == f.trunc() {
        format!("{:.1}", f)
    } else {
        f.to_string()
    }
}

/// [`format_f64`] for an `f32` — `f32::to_string` is the shortest text that
/// reads back as the same binary32 value, which is a different (shorter)
/// answer than the widened `f64`'s.
fn format_f32(f: f32) -> String {
    if f.is_finite() && f == f.trunc() {
        format!("{:.1}", f)
    } else {
        f.to_string()
    }
}
