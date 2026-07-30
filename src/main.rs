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

use typelisp::fasl::{registry_mark, source_hash, Fasl};
use typelisp::project::{find_src_root, load_file_flat, needs_immediate_exec, Loader};
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
    // `typl compile-module <file.typl> [-o out.fasl]` — precompile a source
    // file to a fasl (see `crate::fasl` / `compile_module`), for
    // fasl-preferred `(load)`.
    if args.first().map(String::as_str) == Some("compile-module") {
        std::process::exit(compile_module(&args[1..], heap_cells, features));
    }
    // Otherwise the first non-flag argument names a source file to run;
    // with none, start the REPL.
    if let Some(file) = args.iter().find(|a| !a.starts_with("--")) {
        std::process::exit(run_file(file, heap_cells, features));
    }
    repl(heap_cells, features)
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
/// first `cons`, so it is never a useful request).
fn parse_heap_cells_value(v: &str) -> usize {
    match v.parse::<usize>() {
        Ok(n) if n > 0 => n,
        _ => {
            eprintln!("--heap-cells: `{}` is not a positive integer", v);
            std::process::exit(1);
        }
    }
}

/// `compile-module <file.typl> [-o <out.fasl>]`: checks `file` against a
/// prelude-loaded environment and writes a fasl of the definitions it added.
/// Does *not* run the file's top-level expressions (only registrations are
/// captured — a `TopLevel::Expr` in the source is a compile error, since a
/// fasl is a module of definitions, not a script). Returns an exit code.
fn compile_module(args: &[String], heap_cells: usize, features: Vec<String>) -> i32 {
    let mut input: Option<&str> = None;
    let mut output: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" => match it.next() {
                Some(o) => output = Some(o.clone()),
                None => {
                    eprintln!("compile-module: -o needs an output path");
                    return 1;
                }
            },
            _ if input.is_none() => input = Some(a),
            _ => {
                eprintln!("compile-module: unexpected argument `{}`", a);
                return 1;
            }
        }
    }
    let input = match input {
        Some(f) => f,
        None => {
            eprintln!("compile-module: usage: typl compile-module <file.typl> [-o <out.fastl>]");
            return 1;
        }
    };
    let out_path = output
        .unwrap_or_else(|| PathBuf::from(input).with_extension(typelisp::project::FASL_EXTENSION).to_string_lossy().into_owned());

    let src = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("compile-module: cannot read `{}`: {}", input, e);
            return 1;
        }
    };

    let mut heap = Heap::with_capacity(heap_cells);
    let reader = Reader::with_features(features);
    let mut checker = Checker::new();
    checker.set_redef_policy(parse_redef_policy());
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    // Load the compiler island natively (interp-closure removal Stage 5) so a
    // module whose top-level initializer builds a closure JIT-compiles it at
    // definition time instead of falling back to an interpreted closure.
    typelisp::load_compiler_aot(&mut heap, &mut checker, &mut interp);

    let mark = registry_mark(&checker);
    let forms = match reader.read_all_in_spanned(&mut heap, input, &src) {
        Ok(fs) => fs,
        Err(e) => {
            eprintln!("compile-module: {}", e);
            return 1;
        }
    };
    let mut top_levels = Vec::new();
    for (v, loc) in forms {
        match checker.check_form_at(&mut heap, &interp, v, Some(loc)) {
            Ok(TopLevel::Expr(_)) => {
                eprintln!("compile-module: `{}` contains a top-level expression; a module is definitions only", input);
                return 1;
            }
            Ok(TopLevel::Load { path }) => {
                // A `(load)` inside a compiled module is applied at compile
                // time (its definitions become part of this module's own
                // environment), the same as a source load.
                let dir = PathBuf::from(input).parent().unwrap_or_else(|| FsPath::new(".")).to_path_buf();
                if let Err(e) = load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, &path) {
                    eprintln!("compile-module: {}", e);
                    return 1;
                }
            }
            Ok(tl) => {
                if let Err(e) = interp.exec(&mut heap, tl.clone()) {
                    eprintln!("compile-module: exec: {}", e);
                    return 1;
                }
                top_levels.push(tl);
            }
            Err(e) => {
                eprintln!("compile-module: {}", e);
                return 1;
            }
        }
    }
    for w in checker.take_warnings() {
        eprintln!("{}", w);
    }

    let fasl = match Fasl::capture(&heap, &checker, &mark, top_levels, source_hash(&src)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("compile-module: capture: {}", e);
            return 1;
        }
    };
    let bytes = match fasl.to_bytes() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("compile-module: serialize: {}", e);
            return 1;
        }
    };
    if let Err(e) = std::fs::write(&out_path, bytes) {
        eprintln!("compile-module: cannot write `{}`: {}", out_path, e);
        return 1;
    }
    0
}

/// Load and execute `file` (and, transitively, whatever its `use`s pull in).
/// Returns the process exit code. Top-level expression results are not
/// printed — printing is the REPL's affordance; a script prints via `print`.
fn run_file(file: &str, heap_cells: usize, features: Vec<String>) -> i32 {
    let mut heap = Heap::with_capacity(heap_cells);
    let reader = Reader::with_features(features);
    let mut checker = Checker::new();
    checker.set_redef_policy(parse_redef_policy());
    let mut interp = Interp::new();
    typelisp::prelude::load_cached(&mut heap, &mut checker, &mut interp);
    // The compiler island is always loaded (interp-closure removal Stage 5),
    // as native AOT code, so any closure this file defines is JIT-compiled at
    // definition time rather than falling back to an interpreted closure.
    typelisp::load_compiler_aot(&mut heap, &mut checker, &mut interp);

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

fn repl(heap_cells: usize, features: Vec<String>) -> rustyline::Result<()> {
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
    typelisp::prelude::load_cached(&mut heap, &mut checker.borrow_mut(), &mut interp);
    // The compiler island is always loaded (interp-closure removal Stage 5),
    // as native AOT code, so a closure typed at the REPL is JIT-compiled at
    // definition time rather than falling back to an interpreted closure.
    typelisp::load_compiler_aot(&mut heap, &mut checker.borrow_mut(), &mut interp);
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
        match rl.readline(prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if pending.is_empty() && (trimmed == ":quit" || trimmed == ":exit") {
                    break;
                }
                let _ = rl.add_history_entry(line.as_str());
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

/// Read+check+execute whatever is currently in `pending`. On a recoverable
/// "need more input" error, leaves `pending` untouched so the caller keeps
/// accumulating lines. On success or a real error, clears `pending`.
///
/// GC-root discipline: `read_all`'s temporary roots must never be left on
/// `heap`'s root stack across an `Interp::exec` call, since `Interp`'s
/// (private) `sync_roots` pops a self-tracked count assuming nothing else
/// pushed on top of it. So: pop back to `mark` immediately on any read
/// failure (nothing left to check yet), or only after every form in the
/// batch has been through `check_form` (whose output never retains the raw
/// `Value`) — never pop in between, and never exec before popping.
///
/// One exception: a `(defmacro ...)` form is `exec`'d *immediately* once
/// checked, right here in the check loop, instead of waiting for the batch
/// exec pass below — a macro use later in the *same* pasted/typed batch needs
/// the macro's body already present in `Interp` to expand during checking
/// (see `MacroExpander`/`check::checker::check_list`). This doesn't violate
/// the invariant above: `Interp::exec` on a `Defmacro` is just a `HashMap`
/// insert, so it never touches the heap's root stack (unlike a real
/// expression `exec`, which runs `sync_roots`).
fn try_run_pending(
    heap: &mut Heap,
    reader: &Reader,
    checker: &RefCell<Checker>,
    interp: &mut Interp,
    loader: &mut Loader,
    pending: &mut String,
) {
    let mark = heap.root_count();
    let forms = match reader.read_all_in_spanned(heap, "<stdin>", pending) {
        Ok(forms) => forms,
        Err(e) => {
            while heap.root_count() > mark {
                heap.pop_root();
            }
            if is_incomplete(&e) {
                return; // keep accumulating
            }
            eprintln!("error: {}", e);
            pending.clear();
            return;
        }
    };

    // Load any `use` dependencies before checking, so `check_use` finds them
    // in the registry (see `typelisp::project`). A nested load pushes and
    // pops its own read roots strictly above this batch's, so the root-stack
    // discipline below is undisturbed.
    let form_values: Vec<Value> = forms.iter().map(|(v, _)| *v).collect();
    if let Err(e) = loader.load_uses_in(heap, reader, &mut checker.borrow_mut(), interp, &form_values) {
        while heap.root_count() > mark {
            heap.pop_root();
        }
        for w in checker.borrow_mut().take_warnings() {
            eprintln!("{}", w);
        }
        eprintln!("error: {}", e);
        pending.clear();
        return;
    }

    let mut checked = Vec::with_capacity(forms.len());
    let mut check_err = None;
    for (v, loc) in forms {
        // Borrow the checker only for the check itself (`result` owns the
        // `TopLevel`), so the borrow is released before any `interp.exec`
        // below — a `defmacro`'s immediate exec, or the batch exec further
        // down, must be free to let a runtime `(eval ...)` re-borrow the
        // checker (`Interp::eval_form`).
        let result = checker.borrow_mut().check_form_at(heap, &*interp, v, Some(loc));
        for w in checker.borrow_mut().take_warnings() {
            eprintln!("{}", w);
        }
        match result {
            // `(load "path")` loads inline, fasl-preferred, into the current
            // (root) environment — resolved relative to the process cwd.
            // (A `(load)`ed file whose *own* top-level contains `(eval ...)`
            // is the one corner this borrow doesn't cover — `load_file_flat`
            // execs inline while this `borrow_mut` is held; an ordinary
            // module/fasl load never top-level-`eval`s, so it doesn't arise
            // in practice.)
            Ok(TopLevel::Load { path }) => {
                if let Err(e) = load_file_flat(heap, reader, &mut checker.borrow_mut(), interp, FsPath::new("."), &path) {
                    check_err = Some(e);
                    break;
                }
            }
            Ok(tl) if needs_immediate_exec(&tl) => {
                let _ = interp.exec(heap, tl);
            }
            Ok(tl) => checked.push(tl),
            Err(e) => {
                check_err = Some(e);
                break;
            }
        }
    }
    while heap.root_count() > mark {
        heap.pop_root();
    }
    pending.clear();

    if let Some(e) = check_err {
        eprintln!("error: {}", e);
        return;
    }

    // Loaded modules' forms run before the batch that `use`d them (their
    // definitions and `defvar` initializers must exist by the time the
    // batch's own forms execute).
    for tl in loader.take_pending() {
        if let Err(e) = interp.exec(heap, tl) {
            eprintln!("error: {}", e);
            return;
        }
    }

    for tl in checked {
        match interp.exec(heap, tl) {
            Ok(Some(v)) => println!("{}", format_value(heap, checker.borrow().registry(), &v)),
            Ok(None) => {}
            Err(e) => {
                eprintln!("error: {}", e);
                break;
            }
        }
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

/// Format an `RtValue` for REPL output, in the reader's own syntax where
/// possible (so the printed form can be pasted back in).
fn format_value(heap: &Heap, reg: &Registry, v: &RtValue) -> String {
    match v {
        RtValue::Int(i) => i.to_string(),
        RtValue::Float(f) => format_float(*f),
        RtValue::Bignum(n) => n.to_string(),
        RtValue::Ratio(r) => format!("{}/{}", r.numer(), r.denom()),
        RtValue::Bool(b) => b.to_string(),
        RtValue::Char(c) => format!("#\\{}", c),
        RtValue::Str(s) => format!("{:?}", s),
        RtValue::Unit => "()".to_string(),
        // The native-repr fallback for an enum instantiated over a type
        // the heap cannot represent (`build_enum_value`'s doc comment) —
        // e.g. an `Option<llvm-value>` surfacing at a REPL somehow. Prints
        // the same shape `format_sexpr`'s boxed-enum arm does.
        RtValue::Data { type_name, variant, fields } => {
            let name = reg
                .type_def(type_name)
                .and_then(|d| d.variants.get(*variant))
                .map(|v| v.name.as_str())
                .unwrap_or("<unknown-variant>");
            if fields.is_empty() {
                name.to_string()
            } else {
                let parts: Vec<String> = fields.iter().map(|f| format_value(heap, reg, f)).collect();
                format!("({} {})", name, parts.join(" "))
            }
        }
        RtValue::Sexpr(sv) => format_sexpr(heap, reg, *sv),
        RtValue::Builtin(name) => format!("#<builtin {}>", name),
        RtValue::BuiltinMethod(type_name, method) => format!("#<builtin {}::{}>", type_name, method),
        RtValue::Scope(scope) => format!("#<scope depth={}>", scope.depth()),
        // Compiler-internal handles; not meant to be printed by user code,
        // so a terse opaque tag is enough.
        RtValue::LlvmModule(_) => "#<llvm-module>".to_string(),
        RtValue::LlvmBuilder(_) => "#<llvm-builder>".to_string(),
        RtValue::LlvmFunction(_) => "#<llvm-function>".to_string(),
        RtValue::LlvmBasicBlock(_) => "#<llvm-basic-block>".to_string(),
        RtValue::LlvmValue(_) => "#<llvm-value>".to_string(),
    }
}

/// Format a Sexpr-side `mem::Value` (the `RtValue::Sexpr` payload),
/// recursively, in the reader's own syntax. `reg` recovers an enum box's
/// variant *name* (the runtime stores only the index) so `(some 1)` prints
/// the way it always has.
fn format_sexpr(heap: &Heap, reg: &Registry, v: Value) -> String {
    match v {
        Value::Empty => "()".to_string(),
        Value::Int(i) => i.to_string(),
        // `Sexpr::Float`, a `defstruct`/`Vector<T>`/`cons-cell<K,V>` instance,
        // a `HashTable<K,V>`, and an enum value are all heap-boxed
        // (`Value::Boxed`, see `BoxedObj`) — `heap.is_struct`/
        // `is_hashtable`/`is_enum` tell them apart. A boxed struct prints
        // positionally (no field names at runtime), recursing through this
        // same function for each field.
        Value::Boxed(id) if heap.is_struct(id) => {
            let parts: Vec<String> =
                (0..heap.struct_field_count(id)).map(|i| format_sexpr(heap, reg, heap.struct_field(id, i))).collect();
            format!("#<{} {}>", heap.struct_type_name(id), parts.join(" "))
        }
        // An enum value prints as its variant name applied to its fields —
        // `(some 1)` / a bare `none` — the exact shape the old
        // `RtValue::Data` arm produced. The box stores the *type* name and
        // variant *index*; the variant's name lives only in the checker's
        // registry, looked up by re-parsing the stored `Path` string.
        Value::Boxed(id) if heap.is_enum(id) => {
            let type_path = typelisp::Path::from_segments(
                heap.enum_type_name(id).split("::").map(|s| s.to_string()).collect(),
            );
            let variant = heap.enum_variant(id);
            let name = reg
                .type_def(&type_path)
                .and_then(|d| d.variants.get(variant))
                .map(|v| v.name.clone())
                .unwrap_or_else(|| "<unknown-variant>".to_string());
            if heap.enum_field_count(id) == 0 {
                name
            } else {
                let parts: Vec<String> =
                    (0..heap.enum_field_count(id)).map(|i| format_sexpr(heap, reg, heap.enum_field(id, i))).collect();
                format!("({} {})", name, parts.join(" "))
            }
        }
        Value::Boxed(id) if heap.is_hashtable(id) => format!("#<hashtable count={}>", heap.hashtable_count(id)),
        // A `Scope<V>` with heap-repr `V` is boxed too since Stage 8 —
        // printed the same way `format_value`'s `RtValue::Scope` arm prints
        // a native-`V` scope.
        Value::Boxed(id) if heap.is_scope(id) => format!("#<scope depth={}>", heap.scope_frame_count(id)),
        // A closure is a boxed value, printed opaquely. Always a compiled
        // closure since interp-closure removal Stage 8c (`BoxedObj::Closure`
        // is gone); `BoxedObj::CompiledClosure` is the only closure box left.
        Value::Boxed(id) if heap.is_compiled_closure(id) => "#<closure>".to_string(),
        Value::Boxed(id) if heap.is_bignum(id) => heap.bignum_value(id).to_string(),
        Value::Boxed(id) if heap.is_ratio(id) => {
            let r = heap.ratio_value(id);
            format!("{}/{}", r.numer(), r.denom())
        }
        Value::Boxed(id) => format_float(heap.float_value(id)),
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
fn format_float(f: f64) -> String {
    if f.is_finite() && f == f.trunc() {
        format!("{:.1}", f)
    } else {
        f.to_string()
    }
}
