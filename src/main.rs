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

use typelisp::project::{find_src_root, Loader};
use typelisp::repl::{Startup, StartupAction};
use typelisp::*;

/// Default cons-arena capacity (cells), used when `--heap-cells` is absent.
/// The arena is fixed-size — sized up front, never grown (see
/// `typelisp_mem::Heap`'s module doc / `Error::HeapExhausted`); this is the
/// same `1 << 16` the runtime paths have always allocated.
const DEFAULT_HEAP_CELLS: usize = 1 << 16;

/// What `typl --version` reports: the workspace's version, which every crate
/// shares (`Cargo.toml`'s `[workspace.package]`).
const TYPL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What `typl --help` prints.
const HELP: &str = "\
Usage:
  typl [OPTIONS]                                 start the REPL
  typl [OPTIONS] FILE [ARGS...]                  run FILE; ARGS are its (command-line-args)
  typl [--lib-dir DIR] [-O<N>] -c SOURCE [-o OUTPUT] [--heap-cells N]
       [--max-memory SIZE] [--no-cons] [--target TARGET] [--linker-script FILE]
       [--board-functions WHO] [--link FILE]
                                                 compile SOURCE to an executable
  typl [OPTIONS] test [FILE.typl] [FILTER]       run the #[test] functions of the
                                                 project here (or of FILE) whose
                                                 name contains FILTER

Options:
  -c, --compile SOURCE   compile SOURCE to a native executable
  -o OUTPUT              name of the executable (default: SOURCE without .typl)
  --release              a release build: every eprint/eprintln is left out
                         (of a run, a test, or the executable -c builds)
  -O0, -O1, -O2, -O3     how much compiled code is optimized (default -O0):
                         the executable -c builds, and what is compiled
                         while running or testing (the interpreter is not)
  --lib-dir DIR          link DIR/libtypelisp_front.a into compiled executables
                         (default: a release typl writes the one built into
                         it to $TYPELISP_HOME/lib, ~/.typelisp/lib if unset;
                         a debug typl uses the one in the tree it was built in)
  --image FILE           start from a dump written by (dump ...)
  --heap-cells N         initial capacity of the cons arena, in cells; with -c,
                         the capacity of the executable's arena (default 65536)
  --max-memory SIZE      after -c: the most the executable may allocate, in
                         bytes (K, M or G may follow); more is a runtime error
  --no-cons              after -c: the executable has no cons cells; a program
                         that needs them is a compile error
  --target TARGET        after -c: build for TARGET instead of this machine:
                         wasm32-wasip1 makes a WASI module, wasm32-unknown-unknown
                         a module and the JavaScript that runs it (default
                         OUTPUT: SOURCE with .wasm), thumbv7em-none-eabihf an
                         ELF executable for a Cortex-M4F/M7 board and
                         thumbv8m.main-none-eabihf one for a Cortex-M33 board
                         (default OUTPUT: SOURCE with .elf)
  --linker-script FILE   after --target for a board: the board's flash and
                         RAM, as a memory.x (default: those of QEMU's
                         mps2-an386 for a Cortex-M4, mps2-an505 for an M33)
  --board-functions WHO  after --target for a board: who gives the board's
                         output, input, exit and clock: semihosting (QEMU or
                         a debugger; the default) or c (the board's
                         typelisp_host_* C functions, given with --link)
  --link FILE            after --target for a board: an object or static
                         library to link with (repeatable)
  --feature NAME         add a feature for #+/#- (repeatable)
  --on-redefine=POLICY   on redefinition: warn (default), error or silent
  --eval FORM            REPL only: run FORM before the first prompt
                         (repeatable; run in order with --load)
  --load FILE            REPL only: run FILE's forms before the first prompt
                         (repeatable)
  --no-init              REPL only: do not read ~/.typlrc at startup
  --remove-lib           remove the library this typl wrote for compiled
                         executables to link, and exit
  --remove-lib --others  remove the ones other builds of typl wrote instead
  --remove-lib --all     remove every build's
  --help                 print this help and exit
  --version              print the version and exit

--help, --version, --remove-lib, --others and --all after FILE are passed to
the program.
--image and --feature cannot be combined with -c.
";

fn main() -> rustyline::Result<()> {
    if let Some(warning) = typelisp::compile::orc::unsupported_host_warning() {
        eprintln!("{}", warning);
    }
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
    // `--eval`, `--load` and `--no-init` are the REPL's; taken from before a
    // script's name only, since from the name on they are the script's.
    let (startup, args) = parse_startup(args);
    // `--help`/`--version`/`--remove-lib` (and its `--others`/`--all`) are
    // `typl`'s only among the options before a script's name; from the name
    // on, they are the script's own arguments.
    let own_len = args
        .iter()
        .position(|a| !a.starts_with("--") && typelisp::opt_level::parse_flag(a).is_none())
        .unwrap_or(args.len());
    // `--release` only sets the process's release flag; what behaves
    // differently in a release build asks it (`typelisp::release`).
    let release = args[..own_len].iter().any(|a| a == "--release");
    // `-O<N>` likewise only records the level (`typelisp::opt_level`); the
    // last one given counts.
    for a in &args[..own_len] {
        match typelisp::opt_level::parse_flag(a) {
            Some(Ok(level)) => typelisp::opt_level::set_opt_level(level),
            Some(Err(e)) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
            None => {}
        }
    }
    let args: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|&(i, a)| !(i < own_len && (a == "--release" || typelisp::opt_level::parse_flag(a).is_some())))
        .map(|(_, a)| a.clone())
        .collect();
    if release {
        typelisp::release::set_release(true);
    }
    let own = &args[..args.iter().position(|a| !a.starts_with("--")).unwrap_or(args.len())];
    if own.iter().any(|a| a == "--help") {
        print!("{}", HELP);
        std::process::exit(0);
    }
    if own.iter().any(|a| a == "--version") {
        println!("typl {}", TYPL_VERSION);
        std::process::exit(0);
    }
    // `--others`/`--all` say which libraries `--remove-lib` removes, and mean
    // nothing without it — refused rather than ignored.
    let (others, all) = (own.iter().any(|a| a == "--others"), own.iter().any(|a| a == "--all"));
    if own.iter().any(|a| a == "--remove-lib") {
        let which = match (others, all) {
            (false, false) => typelisp::compile::aot::RemoveLib::This,
            (true, false) => typelisp::compile::aot::RemoveLib::Others,
            (false, true) => typelisp::compile::aot::RemoveLib::All,
            (true, true) => {
                eprintln!("--remove-lib: --others and --all cannot be combined");
                std::process::exit(1);
            }
        };
        match typelisp::compile::aot::remove_written_archives(which) {
            Ok(removed) if removed.is_empty() => println!("no library to remove"),
            Ok(removed) => {
                for dir in removed {
                    println!("removed {}", dir.display());
                }
            }
            Err(e) => {
                eprintln!("--remove-lib: {}", e);
                std::process::exit(1);
            }
        }
        std::process::exit(0);
    }
    for (given, flag) in [(others, "--others"), (all, "--all")] {
        if given {
            eprintln!("{}: only means something with --remove-lib", flag);
            std::process::exit(1);
        }
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
        //
        // `--heap-cells` is the exception: with `-c` it sizes the
        // executable's cons arena instead.
        for (given, given_flag) in [(!features.is_empty(), "--feature"), (image.is_some(), "--image")] {
            if given {
                eprintln!("{}: {} has no effect on compilation", flag, given_flag);
                std::process::exit(1);
            }
        }
        std::process::exit(compile_command(flag, &args[1..], heap_cells));
    }
    let heap_cells = heap_cells.unwrap_or(DEFAULT_HEAP_CELLS);
    // What starts the REPL is the absence of a subcommand and a script, so
    // an option of the REPL's beside either is refused rather than ignored.
    let starts_repl = !args.iter().any(|a| !a.starts_with("--"));
    if !starts_repl {
        if let Some(flag) = startup_flag_given(&startup) {
            eprintln!("{}: only for the REPL (typl with no FILE)", flag);
            std::process::exit(1);
        }
    }
    if args.first().is_some_and(|a| a == "test") {
        std::process::exit(test_command(&args[1..], heap_cells, features, image));
    }
    // The first non-flag argument names a source file to run;
    // with none, start the REPL.
    //
    // Everything from the file name onward is the *script's* command line,
    // and `(command-line-args)` must answer with it rather than with `typl`'s
    // own `std::env::args()` — otherwise the same source would see a
    // different vector run by `typl` than run as an AOT executable, where
    // element 0 is the program and element 1 the first argument. Installing
    // it here, before any user code, is what makes the two agree; see
    // `typelisp_rt_sys::env::COMMAND_LINE_ARGS`.
    if let Some(pos) = args.iter().position(|a| !a.starts_with("--")) {
        typelisp_rt_sys::env::set_command_line_args(args[pos..].to_vec());
        std::process::exit(run_file(&args[pos], heap_cells, features, image));
    }
    // The REPL has no script, so its command line is just `typl` itself —
    // set explicitly rather than left to the process argv, which would leak
    // `--heap-cells` and friends into a program's view of its arguments.
    typelisp_rt_sys::env::set_command_line_args(vec!["typl".to_string()]);
    start_repl(heap_cells, features, image, startup)
}

/// Loads the environment and hands it to the REPL (`typelisp::repl`).
fn start_repl(heap_cells: usize, features: Vec<String>, image: Option<PathBuf>, startup: Startup) -> rustyline::Result<()> {
    let mut heap = Heap::with_capacity(heap_cells);
    let reader = Reader::with_features(features);
    let mut checker = Checker::new();
    checker.set_redef_policy(parse_redef_policy());
    let mut interp = Interp::new();
    if let Err(e) = load_environment(&mut heap, &mut checker, &mut interp, image) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
    typelisp::repl::run(&mut heap, &reader, checker, &mut interp, startup)
}

/// Takes `--eval FORM`, `--load FILE` (each also as `FLAG=VALUE`) and
/// `--no-init` out of `args`, up to the first argument that is not an
/// option: a script's name or a subcommand, and everything after it, is left
/// as it is. The actions keep the order they were given in.
fn parse_startup(args: Vec<String>) -> (Startup, Vec<String>) {
    let mut startup = Startup { init_file: true, actions: Vec::new() };
    let mut rest = Vec::with_capacity(args.len());
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if !a.starts_with('-') {
            rest.push(a);
            rest.extend(it);
            break;
        }
        if a == "--no-init" {
            startup.init_file = false;
            continue;
        }
        let mut action = None;
        for (flag, missing, make) in [
            ("--eval", "needs a form", StartupAction::Eval as fn(String) -> StartupAction),
            ("--load", "needs a file", |v: String| StartupAction::Load(PathBuf::from(v))),
        ] {
            if a == flag {
                match it.next() {
                    Some(v) => action = Some(make(v)),
                    None => {
                        eprintln!("{}: {}", flag, missing);
                        std::process::exit(1);
                    }
                }
            } else if let Some(v) = a.strip_prefix(flag).and_then(|v| v.strip_prefix('=')) {
                action = Some(make(v.to_string()));
            }
        }
        match action {
            Some(action) => startup.actions.push(action),
            None => rest.push(a),
        }
    }
    (startup, rest)
}

/// The first of the REPL's own options that `startup` says was given.
fn startup_flag_given(startup: &Startup) -> Option<&'static str> {
    match startup.actions.first() {
        Some(StartupAction::Eval(_)) => Some("--eval"),
        Some(StartupAction::Load(_)) => Some("--load"),
        None if !startup.init_file => Some("--no-init"),
        None => None,
    }
}

/// `typl -c SOURCE [-o OUTPUT]`: `args` is what follows the `-c` (or
/// `--compile`, which `flag` names for the diagnostics). Without `-o`, the
/// executable is `SOURCE` with its `.typl` extension removed. A `--release`
/// or `-O<N>` among them is the global one, written after `-c`. `heap_cells`
/// is the global `--heap-cells`, which here sizes the executable's cons
/// arena. Returns the process exit code.
fn compile_command(flag: &str, args: &[String], heap_cells: Option<usize>) -> i32 {
    let mut source = None;
    let mut output = None;
    let mut options = typelisp::compile::aot::BuildOptions { heap_cells, ..Default::default() };
    // The last flag given that only a board takes.
    let mut board_only = None;
    let mut board_functions = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--no-cons" {
            options.no_cons = true;
        } else if let Some(v) = flag_value(a, "--target", &mut it) {
            let Some(v) = v else {
                eprintln!("--target: needs a target (wasm32-wasip1, wasm32-unknown-unknown, {})", typelisp::compile::aot::Target::board_names());
                return 1;
            };
            match typelisp::compile::aot::Target::parse(v) {
                Ok(target) => options.target = target,
                Err(e) => {
                    eprintln!("{}", e);
                    return 1;
                }
            }
        } else if let Some(v) = flag_value(a, "--board-functions", &mut it) {
            let Some(v) = v else {
                eprintln!("--board-functions: needs semihosting or c");
                return 1;
            };
            match typelisp::compile::aot::BoardFunctions::parse(v) {
                Ok(who) => {
                    options.board_functions = who;
                    board_only = Some("--board-functions");
                    board_functions = true;
                }
                Err(e) => {
                    eprintln!("{}", e);
                    return 1;
                }
            }
        } else if let Some(v) = flag_value(a, "--link", &mut it) {
            let Some(v) = v else {
                eprintln!("--link: needs an object or static library");
                return 1;
            };
            options.link.push(PathBuf::from(v));
            board_only = Some("--link");
        } else if let Some(v) = flag_value(a, "--linker-script", &mut it) {
            let Some(v) = v else {
                eprintln!("--linker-script: needs the board's memory.x");
                return 1;
            };
            options.linker_script = Some(PathBuf::from(v));
            board_only = Some("--linker-script");
        } else if let Some(v) = flag_value(a, "--max-memory", &mut it) {
            let Some(v) = v else {
                eprintln!("--max-memory: needs a size in bytes (K, M or G may follow)");
                return 1;
            };
            match parse_memory_size(v) {
                Ok(bytes) => options.max_memory = Some(bytes),
                Err(e) => {
                    eprintln!("--max-memory: {}", e);
                    return 1;
                }
            }
        } else if a == "--release" {
            typelisp::release::set_release(true);
        } else if let Some(level) = typelisp::opt_level::parse_flag(a) {
            match level {
                Ok(level) => typelisp::opt_level::set_opt_level(level),
                Err(e) => {
                    eprintln!("{}", e);
                    return 1;
                }
            }
        } else if a == "-o" {
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
            eprintln!("{}: unexpected argument `{}` (usage: typl {} SOURCE [-o OUTPUT] [--release] [-O<N>] [--max-memory SIZE] [--no-cons] [--target TARGET] [--linker-script FILE] [--board-functions WHO] [--link FILE])", flag, a, flag);
            return 1;
        }
    }
    if let Some(flag) = board_only.filter(|_| options.target.board().is_none()) {
        eprintln!("{}: only for a board (--target {})", flag, typelisp::compile::aot::Target::board_names());
        return 1;
    }
    if !options.target.has_runtime() {
        // What these are for is the runtime's.
        let runtime_only = [("--board-functions", board_functions), ("--heap-cells", heap_cells.is_some()), ("--max-memory", options.max_memory.is_some())];
        if let Some((flag, _)) = runtime_only.iter().find(|(_, given)| *given) {
            eprintln!("{}: the target {} has no runtime", flag, options.target.name());
            return 1;
        }
    }
    let Some(source) = source else {
        eprintln!("{}: needs a source file (usage: typl {} SOURCE [-o OUTPUT] [--release] [-O<N>] [--max-memory SIZE] [--no-cons] [--target TARGET] [--linker-script FILE] [--board-functions WHO] [--link FILE])", flag, flag);
        return 1;
    };
    let output = match output {
        Some(o) => o,
        None if source.extension().is_some_and(|e| e == "typl") => match options.target {
            typelisp::compile::aot::Target::Host => source.with_extension(""),
            typelisp::compile::aot::Target::Wasip1 | typelisp::compile::aot::Target::Wasm32Unknown => source.with_extension("wasm"),
            typelisp::compile::aot::Target::Thumbv7em | typelisp::compile::aot::Target::Thumbv8mMain | typelisp::compile::aot::Target::AvrAtmega328p => source.with_extension("elf"),
        },
        None => {
            eprintln!("{}: {} does not end in .typl, so give the executable's name with -o", flag, source.display());
            return 1;
        }
    };
    match typelisp::compile::aot::compile_file_with(&source.to_string_lossy(), &output.to_string_lossy(), &options) {
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

/// `FLAG VALUE` or `FLAG=VALUE` at `a`: `None` when `a` is not `flag`, and
/// `Some(None)` when it is but no value follows.
fn flag_value<'a>(a: &'a str, flag: &str, rest: &mut std::slice::Iter<'a, String>) -> Option<Option<&'a str>> {
    if a == flag {
        Some(rest.next().map(String::as_str))
    } else {
        a.strip_prefix(flag).and_then(|r| r.strip_prefix('=')).map(Some)
    }
}

/// `--max-memory`'s size: a positive number of bytes, optionally followed by
/// `K`, `M` or `G` (powers of 1024).
fn parse_memory_size(v: &str) -> Result<usize, String> {
    let (digits, unit) = match v.char_indices().last() {
        Some((i, 'K' | 'k')) => (&v[..i], 1usize << 10),
        Some((i, 'M' | 'm')) => (&v[..i], 1 << 20),
        Some((i, 'G' | 'g')) => (&v[..i], 1 << 30),
        _ => (v, 1),
    };
    match digits.parse::<usize>() {
        Ok(n) if n > 0 => n.checked_mul(unit).ok_or_else(|| format!("`{}` does not fit in this machine's address space", v)),
        _ => Err(format!("`{}` is not a positive size in bytes (K, M or G may follow)", v)),
    }
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
    interp.start_recording(checker.signature());
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
    // file's forms in this same module, but left the checker at the root.
    let entry_segs = typelisp::project::module_segs_for(&file, &src_root)
        .expect("the load above derived the entry file's module from this path");
    checker.set_current_ns(typelisp::symbols::ns_of(&entry_segs));
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

/// `typl test [FILE.typl] [FILTER]`: `args` is what follows `test`. Exits 0
/// when every test that ran passed.
fn test_command(args: &[String], heap_cells: usize, features: Vec<String>, image: Option<PathBuf>) -> i32 {
    let (file, rest) = match args.first() {
        Some(a) if a.ends_with(".typl") => (Some(PathBuf::from(a)), &args[1..]),
        _ => (None, args),
    };
    let filter = match rest {
        [] => None,
        [f] => Some(f.as_str()),
        _ => {
            eprintln!("typl test: [FILE.typl] [FILTER]: more than one filter given");
            return 1;
        }
    };
    let suite = match &file {
        Some(f) => typelisp::testing::Suite::of_file(f),
        None => std::env::current_dir()
            .map_err(|e| format!("the current folder: {}", e))
            .and_then(|dir| typelisp::testing::Suite::of_project(&dir)),
    };
    let suite = match suite {
        Ok(s) => s,
        Err(e) => {
            eprintln!("typl test: {}", e);
            return 1;
        }
    };
    let mut heap = Heap::with_capacity(heap_cells);
    let reader = Reader::with_features(features);
    let mut checker = Checker::new();
    checker.set_redef_policy(parse_redef_policy());
    let mut interp = Interp::new();
    if let Err(e) = load_environment(&mut heap, &mut checker, &mut interp, image) {
        eprintln!("error: {}", e);
        return 1;
    }
    let mut out = std::io::stdout();
    match typelisp::testing::run(&suite, filter, &mut heap, &reader, checker, &mut interp, &mut out) {
        Ok(outcome) if outcome.failed == 0 => 0,
        Ok(_) => 1,
        Err(e) => {
            eprintln!("error: {}", e);
            1
        }
    }
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

