//! The `typl` CLI: with a file argument, loads and runs it (and its `use`
//! dependencies — see `typelisp::project` for the file-to-module mapping);
//! with no argument, a read-eval-print loop over the `Reader` -> `Checker` ->
//! `Interp` pipeline, using `rustyline` for Emacs-style line editing/history
//! (Ctrl+P/Ctrl+N to move through history, Ctrl+R to search it, etc. — all
//! `rustyline`'s default `EditMode::Emacs` bindings, matching bash/readline).
//! With `-c SOURCE [-o OUTPUT]` (or `--compile`), AOT-compiles `SOURCE` to
//! an executable instead, the same thing `(compile-file SOURCE OUTPUT)` does.
//! `--help` and `--version` print and exit.

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

/// What `typl --version` reports. Provisional until the first release fixes
/// the number, and independent of the crates' `Cargo.toml` versions until then.
const TYPL_VERSION: &str = "0.0.1";

/// What `typl --help` prints.
const HELP: &str = "\
Usage:
  typl [OPTIONS]                                 start the REPL
  typl [OPTIONS] FILE [ARGS...]                  run FILE; ARGS are its (command-line-args)
  typl [--lib-dir DIR] -c SOURCE [-o OUTPUT]     compile SOURCE to an executable

Options:
  -c, --compile SOURCE   compile SOURCE to a native executable
  -o OUTPUT              name of the executable (default: SOURCE without .typl)
  --lib-dir DIR          link DIR/libtypelisp_front.a into compiled executables
                         (default: the one in the tree typl was built in)
  --image FILE           start from a dump written by (dump ...)
  --heap-cells N         initial capacity of the cons arena, in cells
  --feature NAME         add a feature for #+/#- (repeatable)
  --on-redefine=POLICY   on redefinition: warn (default), error or silent
  --help                 print this help and exit
  --version              print the version and exit

--help and --version after FILE are passed to the program.
--image, --heap-cells and --feature cannot be combined with -c.
";

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
    // A global `--lib-dir DIR` (or `--lib-dir=DIR`) names the folder holding
    // the static library an AOT executable links, for `-c` and for every
    // `(compile-file ...)` this process runs.
    let (lib_dir, args) = parse_lib_dir(args);
    // `--help`/`--version` are `typl`'s only among the options before a
    // script's name; from the name on, they are the script's own arguments.
    let own = &args[..args.iter().position(|a| !a.starts_with("--")).unwrap_or(args.len())];
    if own.iter().any(|a| a == "--help") {
        print!("{}", HELP);
        std::process::exit(0);
    }
    if own.iter().any(|a| a == "--version") {
        println!("typl {}", TYPL_VERSION);
        std::process::exit(0);
    }
    if let Some(dir) = lib_dir {
        if let Err(e) = typelisp::compile::aot::set_lib_dir(dir) {
            eprintln!("--lib-dir: {}", e);
            std::process::exit(1);
        }
    }
    if let Some(flag) = args.first().filter(|a| *a == "-c" || *a == "--compile") {
        // The executable is compiled in an environment of its own
        // (`compile::aot::compile_file`), which none of these flags reach;
        // refused rather than accepted and ignored.
        for (given, given_flag) in [(heap_cells.is_some(), "--heap-cells"), (!features.is_empty(), "--feature"), (image.is_some(), "--image")] {
            if given {
                eprintln!("{}: {} has no effect on compilation", flag, given_flag);
                std::process::exit(1);
            }
        }
        std::process::exit(compile_command(flag, &args[1..]));
    }
    let heap_cells = heap_cells.unwrap_or(DEFAULT_HEAP_CELLS);
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

/// `typl -c SOURCE [-o OUTPUT]`: `args` is what follows the `-c` (or
/// `--compile`, which `flag` names for the diagnostics). Without `-o`, the
/// executable is `SOURCE` with its `.typl` extension removed. Returns the
/// process exit code.
fn compile_command(flag: &str, args: &[String]) -> i32 {
    let mut source = None;
    let mut output = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "-o" {
            match it.next() {
                Some(v) => output = Some(PathBuf::from(v)),
                None => {
                    eprintln!("-o: needs a path for the executable");
                    return 1;
                }
            }
        } else if source.is_none() && !a.starts_with('-') {
            source = Some(PathBuf::from(a));
        } else {
            eprintln!("{}: unexpected argument `{}` (usage: typl {} SOURCE [-o OUTPUT])", flag, a, flag);
            return 1;
        }
    }
    let Some(source) = source else {
        eprintln!("{}: needs a source file (usage: typl {} SOURCE [-o OUTPUT])", flag, flag);
        return 1;
    };
    let output = match output {
        Some(o) => o,
        None if source.extension().is_some_and(|e| e == "typl") => source.with_extension(""),
        None => {
            eprintln!("{}: {} does not end in .typl, so give the executable's name with -o", flag, source.display());
            return 1;
        }
    };
    match typelisp::compile::aot::compile_file(&source.to_string_lossy(), &output.to_string_lossy()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {}", e);
            1
        }
    }
}

/// Removes every `FLAG VALUE` / `FLAG=VALUE` occurrence of `flag` from
/// `args`, returning the values in the order given and the remaining
/// arguments. Exits with `FLAG: missing` when `flag` is the last argument and
/// so has no value.
fn take_flag_values(args: Vec<String>, flag: &str, missing: &str) -> (Vec<String>, Vec<String>) {
    let mut values = Vec::new();
    let mut rest = Vec::with_capacity(args.len());
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == flag {
            match it.next() {
                Some(v) => values.push(v),
                None => {
                    eprintln!("{}: {}", flag, missing);
                    std::process::exit(1);
                }
            }
        } else if let Some(v) = a.strip_prefix(flag).and_then(|v| v.strip_prefix('=')) {
            values.push(v.to_string());
        } else {
            rest.push(a);
        }
    }
    (values, rest)
}

/// Parses a global `--lib-dir DIR` / `--lib-dir=DIR` flag out of `args`,
/// returning the folder and the remaining arguments with the flag removed.
/// Exits with a diagnostic on a missing value. A later occurrence wins.
fn parse_lib_dir(args: Vec<String>) -> (Option<PathBuf>, Vec<String>) {
    let (dirs, rest) = take_flag_values(args, "--lib-dir", "needs a folder");
    (dirs.last().map(PathBuf::from), rest)
}

/// Parses a global `--image FILE` / `--image=FILE` flag out of `args`,
/// returning the path and the remaining arguments with the flag removed.
///
/// Exits with a diagnostic on a missing value, for the same reason
/// [`parse_heap_cells`] does: a typo here silently starts a different
/// environment than the one asked for. A later occurrence wins.
fn parse_image(args: Vec<String>) -> (Option<PathBuf>, Vec<String>) {
    let (images, rest) = take_flag_values(args, "--image", "needs a path to a dump");
    (images.last().map(PathBuf::from), rest)
}

/// Parses zero or more `--feature NAME` / `--feature=NAME` flags out of
/// `args`, returning the accumulated feature names (in the order given) and
/// the remaining arguments with every occurrence removed. A later duplicate
/// of the same name is harmless — `Features::with` inserts into a set.
fn parse_features(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    take_flag_values(args, "--feature", "needs a feature name")
}

/// Parses a global `--heap-cells N` / `--heap-cells=N` flag out of `args`,
/// returning the requested cons-arena capacity (`None` when the flag is
/// absent; the caller applies [`DEFAULT_HEAP_CELLS`]) and the remaining
/// arguments with the flag and its value removed. Exits the process with a diagnostic on a missing or invalid
/// value — the flag sizes a one-shot allocation, so a typo is better caught
/// before any work than silently ignored. A later occurrence wins.
fn parse_heap_cells(args: Vec<String>) -> (Option<usize>, Vec<String>) {
    let (values, rest) = take_flag_values(args, "--heap-cells", "needs a positive integer (number of cons cells)");
    // Every occurrence is validated, not only the one that wins.
    let mut cells = None;
    for v in &values {
        cells = Some(parse_heap_cells_value(v));
    }
    (cells, rest)
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
                    // The form's checked type decides how a niched `Option`
                    // prints — the word alone cannot say it is one.
                    let repr = {
                        let chk = checker.borrow();
                        let reg = chk.registry();
                        chk.expr_type().map(|ty| check::repr::Repr::of_by(ty, &|p| reg.type_def(p).map(|d| d.kind)))
                    };
                    heap.push_root(v);
                    let rendered = interp.render_readably(heap, v, repr.as_ref());
                    heap.pop_root();
                    match rendered {
                        Ok(text) => {
                            typelisp_abi::dribble::note(&text);
                            typelisp_abi::dribble::note("\n");
                            println!("{}", text);
                        }
                        Err(e) => {
                            eprintln!("error: {}", e);
                            break;
                        }
                    }
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

