//! `room` / `dribble` / `ed` / `disassemble` — the half of CLHS 25.2's tool
//! layer that is not `trace` (which has its own suite, `trace_test.rs`).
//!
//! The four split into two categories, and the split is the design:
//!
//! - `room`, `dribble` and `ed` are **ordinary builtins**. The heap
//!   statistics and the dribble sink belong to the runtime, and launching an
//!   editor is a process call, so a `defun` that calls any of them compiles
//!   like any other.
//! - `disassemble` is **interpreter-only**, the category `compile`,
//!   `compile-file` and `dump` are in: it is not that it cannot be compiled,
//!   it is that it *is* the compiler.

extern crate typelisp;
use std::io::Write;
use typelisp::{load_compiler, load_prelude, Checker, Error, Heap, Interp, Reader, Value};

/// Runs `src` and returns its last value together with the heap it lives in.
fn eval(src: &str) -> (Heap, Value) {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    (h, last)
}

/// Runs `src` and returns its last value as text.
fn eval_string(src: &str) -> String {
    let (h, v) = eval(src);
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// The `TypeError` message the first failing form produces.
fn check_error(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                interp.exec(&mut h, tl).expect("eval failed");
            }
            Err(e) => {
                return match e.kind() {
                    Error::TypeError(m) => m.clone(),
                    other => panic!("expected a type error, got {:?}", other),
                }
            }
        }
    }
    panic!("expected `{}` to fail checking", src)
}

/// Runs `src` with `*standard-output*` pointed at a string stream, and
/// returns what was written there.
///
/// Same trick `trace_test.rs` uses for `*trace-output*`: the global holds a
/// `standard-stream`, which is a struct around a native handle, so it can be
/// pointed at a string output stream and read back.
fn stdout_of(src: &str) -> String {
    eval_string(&format!(
        "(defvar (*cap* i32) (stream-string-output))\n\
         (setf *standard-output* (standard-stream::new *cap*))\n\
         {}\n\
         (match (stream-take-output-string *cap*) ((ok s) s) ((err _) \"<capture failed>\"))",
        src
    ))
}

/// A path under this test's own temporary directory.
fn temp_path(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("typelisp-repl-tools-{}-{}", std::process::id(), name));
    p
}

// ---- room ----------------------------------------------------------------

/// `(heap-info)` reports the collector's real state, not a snapshot taken
/// somewhere else: the two halves of the arena add up to the whole.
#[test]
fn heap_info_reports_a_consistent_arena() {
    assert_eq!(
        eval_string(
            "(let ((i (heap-info)))\n\
               (format false \"~d\" (- i::capacity (+ i::live i::free))))"
        ),
        "0"
    );
    // The arena the prelude was loaded into is not empty, and the collector
    // has run: both are facts about any session that got this far.
    assert_eq!(eval_string("(let ((i (heap-info))) (format false \"~a\" (> i::live 0)))"), "true");
}

/// CL's `room` prints and returns nothing. The default report is the two
/// lines about cells and collections; `(room true)` adds the object counts
/// and the arena's growth policy.
#[test]
fn room_prints_a_report_and_returns_nothing() {
    let brief = stdout_of("(room)");
    assert!(brief.starts_with("Cons cells: "), "{:?}", brief);
    assert!(brief.contains("Collections: "), "{:?}", brief);
    assert!(!brief.contains("Symbols:"), "the brief report should stop there: {:?}", brief);

    let full = stdout_of("(room true)");
    for line in ["Cons cells: ", "Collections: ", "Symbols:", "Strings:", "Boxes:", "Arena:"] {
        assert!(full.contains(line), "`{}` is missing from the verbose report: {:?}", line, full);
    }

    let (h, v) = eval("(room)");
    assert!(matches!(v, Value::Empty), "room should answer `()`, got {:?}", v);
    drop(h);
}

/// `gc-count` is an `int` like the cell counts — a collection counter is
/// not bounded by the arena, and an `int` past the fixnum range is a bignum
/// rather than a wrapped word.
#[test]
fn the_collection_counter_is_an_int() {
    assert_eq!(
        eval_string(
            "(let ((i (heap-info)))\n\
               (format false \"~a\" (>= (+ i::gc-count (the int 0)) (the int 0))))"
        ),
        "true"
    );
}

/// `room` is an ordinary function, so a `defun` that calls it compiles.
#[test]
fn room_compiles() {
    let (h, v) = eval("(defun live-cells () int (let ((i (heap-info))) i::live))\n(compile live-cells)");
    assert!(matches!(v, Value::Bool(true)), "expected the compile to succeed, got {:?}", v);
    drop(h);
}

// ---- dribble -------------------------------------------------------------

/// A dribble file collects both doors a program's output leaves by: the
/// printer's own (`println`) and the stream table's (`write-line` on a
/// stdout-backed stream). Closing it stops the recording.
#[test]
fn dribble_records_both_kinds_of_output_until_it_is_closed() {
    let path = temp_path("session.log");
    let _ = std::fs::remove_file(&path);
    let (h, _) = eval(&format!(
        "(match (dribble \"{}\") ((ok _) ()) ((err e) (println \"start failed: ~a\" (message e))))\n\
         (println \"through the printer\")\n\
         (write-line *standard-output* \"through the stream\")\n\
         (match (dribble) ((ok _) ()) ((err e) (println \"stop failed: ~a\" (message e))))\n\
         (println \"after the dribble closed\")",
        path.display()
    ));
    drop(h);
    let text = std::fs::read_to_string(&path).expect("the dribble file should exist");
    let _ = std::fs::remove_file(&path);
    assert!(text.contains("through the printer"), "{:?}", text);
    assert!(text.contains("through the stream"), "{:?}", text);
    assert!(!text.contains("after the dribble closed"), "recording did not stop: {:?}", text);
}

/// A builtin `println` method taken as a function value prints through the
/// same door as the `println` form, so the dribble records it too.
#[test]
fn dribble_records_a_builtin_print_method_used_as_a_value() {
    let path = temp_path("method.log");
    let _ = std::fs::remove_file(&path);
    let (h, _) = eval(&format!(
        "(match (dribble \"{}\") ((ok _) ()) ((err e) (println \"start failed: ~a\" (message e))))\n\
         (defun call-it ((f (fn (bool) ())) (b bool)) () (f b))\n\
         (call-it println true)\n\
         (match (dribble) ((ok _) ()) ((err e) (println \"stop failed: ~a\" (message e))))",
        path.display()
    ));
    drop(h);
    let text = std::fs::read_to_string(&path).expect("the dribble file should exist");
    let _ = std::fs::remove_file(&path);
    assert_eq!(text, "true\n");
}

/// Starting a second dribble closes the first, as CL's does — what was
/// already written stays written, and nothing after goes to the old file.
#[test]
fn a_second_dribble_closes_the_first() {
    let (first, second) = (temp_path("first.log"), temp_path("second.log"));
    for p in [&first, &second] {
        let _ = std::fs::remove_file(p);
    }
    let (h, _) = eval(&format!(
        "(match (dribble \"{}\") ((ok _) ()) ((err _) ()))\n\
         (println \"one\")\n\
         (match (dribble \"{}\") ((ok _) ()) ((err _) ()))\n\
         (println \"two\")\n\
         (match (dribble) ((ok _) ()) ((err _) ()))",
        first.display(),
        second.display()
    ));
    drop(h);
    let a = std::fs::read_to_string(&first).expect("the first file should exist");
    let b = std::fs::read_to_string(&second).expect("the second file should exist");
    for p in [&first, &second] {
        let _ = std::fs::remove_file(p);
    }
    assert!(a.contains("one") && !a.contains("two"), "the first file kept recording: {:?}", a);
    assert!(b.contains("two") && !b.contains("one"), "the second file has the wrong contents: {:?}", b);
}

/// A path that cannot be opened is an ordinary `Err`, not a panic: `dribble`
/// is a convenience, and a program that asks for one in a directory that does
/// not exist should be able to carry on.
#[test]
fn an_unopenable_dribble_path_is_an_error_value() {
    assert!(
        eval_string(
            "(match (dribble \"/no/such/directory/session.log\")\n\
               ((ok _) \"unexpectedly opened\")\n\
               ((err e) (message e)))"
        )
        .starts_with("dribble: /no/such/directory/session.log:")
    );
}

// ---- ed ------------------------------------------------------------------

/// Serializes the tests that set `$EDITOR`/`$VISUAL`.
///
/// The environment is process-wide and the test runner is multi-threaded, so
/// two `ed` tests running at once would each see the other's variables. This
/// is the same reason `std::env::set_var` is the awkward part of testing
/// anything that reads the environment.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A script that records the arguments it was given, for `$EDITOR`.
fn fake_editor(record_to: &std::path::Path) -> std::path::PathBuf {
    let script = temp_path(&format!("editor-{}.sh", record_to.file_name().unwrap().to_string_lossy()));
    let mut f = std::fs::File::create(&script).expect("create the fake editor");
    write!(f, "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n", record_to.display()).expect("write it");
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    script
}

/// `(ed name)` opens the file the definition is written in, at its line.
///
/// The line comes from `Registry::def_locs`, the same table the LSP's
/// goto-definition reads — resolved at check time, so what reaches the
/// runtime is a call with two constants in it.
#[test]
fn ed_opens_a_definition_at_its_own_line() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let record = temp_path("ed-args.txt");
    let _ = std::fs::remove_file(&record);
    let editor = fake_editor(&record);
    std::env::set_var("EDITOR", &editor);
    std::env::remove_var("VISUAL");
    let (h, _) = eval(
        "(defun first-one () int 1)\n\
         (defun second-one () int 2)\n\
         (match (ed second-one) ((ok _) ()) ((err e) (println \"ed failed: ~a\" (message e))))",
    );
    drop(h);
    let args = std::fs::read_to_string(&record).expect("the fake editor should have run");
    let _ = std::fs::remove_file(&record);
    let _ = std::fs::remove_file(&editor);
    let lines: Vec<&str> = args.lines().collect();
    // `second-one` is on line 2 of the source `eval` read.
    assert_eq!(lines.first().copied(), Some("+2"), "wrong line: {:?}", args);
    assert_eq!(lines.get(1).copied(), Some("<input>"), "wrong file: {:?}", args);
}

/// With neither variable set, `ed` says so rather than guessing an editor —
/// guessing is how `ed` launches something the user cannot exit.
#[test]
fn ed_without_an_editor_variable_is_an_error() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("EDITOR");
    std::env::remove_var("VISUAL");
    assert_eq!(
        eval_string(
            "(defun anything () int 1)\n\
             (match (ed anything) ((ok _) \"unexpectedly ran\") ((err e) (message e)))"
        ),
        "ed: neither $VISUAL nor $EDITOR is set"
    );
}

/// A builtin has no source location, and that is said rather than turned into
/// an editor opened on line 0 of nothing.
#[test]
fn ed_on_a_builtin_says_there_is_no_file() {
    let msg = check_error("(ed parse-float)");
    assert!(msg.starts_with("ed: `parse-float` has no source location"), "{}", msg);
}

/// A string argument is a *file*, not a name — CL's `ed` takes either, told
/// apart by shape, and this is the one form where a string is not an error.
#[test]
fn ed_takes_a_path_as_well_as_a_name() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let record = temp_path("ed-path-args.txt");
    let _ = std::fs::remove_file(&record);
    let editor = fake_editor(&record);
    std::env::set_var("EDITOR", &editor);
    std::env::remove_var("VISUAL");
    let (h, _) = eval("(match (ed \"notes.typl\") ((ok _) ()) ((err _) ()))");
    drop(h);
    let args = std::fs::read_to_string(&record).expect("the fake editor should have run");
    let _ = std::fs::remove_file(&record);
    let _ = std::fs::remove_file(&editor);
    // No `+N`: a path names no line.
    assert_eq!(args.lines().collect::<Vec<_>>(), vec!["notes.typl"], "{:?}", args);
}

/// `ed` is a special form whose whole job is resolution, so what survives
/// checking is a call to the ordinary builtin `ed-open` — which means a
/// `defun` that calls `ed` compiles.
#[test]
fn ed_compiles() {
    let (h, v) = eval(
        "(defun edit-me () int 1)\n\
         (defun open-it () Result<(), FileError> (ed edit-me))\n\
         (compile open-it)",
    );
    assert!(matches!(v, Value::Bool(true)), "expected the compile to succeed, got {:?}", v);
    drop(h);
}

// ---- disassemble ---------------------------------------------------------

/// Host assembly by default — what CL's `disassemble` promises, the
/// instructions this machine will actually run.
#[test]
fn disassemble_prints_host_assembly() {
    let out = stdout_of("(defun fact ((n int)) int (if (< n 2) 1 (* n (fact (- n 1)))))\n(disassemble fact)");
    assert!(out.contains("fact"), "the function's own symbol is missing: {:?}", out);
    // Assembler directives, not IR: every target's assembly has sections.
    assert!(out.contains(".section") || out.contains(".text"), "this does not look like assembly: {:?}", out);
    assert!(!out.contains("define i64"), "this is LLVM IR, not assembly: {:?}", out);
}

/// The second argument asks for the LLVM IR instead: the same module one step
/// earlier, for when the question is about this compiler rather than the chip.
#[test]
fn disassemble_can_print_llvm_ir_instead() {
    let out = stdout_of("(defun fact ((n int)) int (if (< n 2) 1 (* n (fact (- n 1)))))\n(disassemble fact true)");
    assert!(out.contains("define i64"), "this does not look like LLVM IR: {:?}", out);
    assert!(out.contains("fact"), "the function's own symbol is missing: {:?}", out);
}

/// Disassembling is a question, not a request to compile: the definition's
/// compiled slot is untouched, so a function that was interpreted before
/// still is afterwards.
///
/// Read through `trace`, which is the one thing that says out loud whether a
/// definition has a compiled body.
#[test]
fn disassemble_leaves_the_definition_interpreted() {
    let after_disassemble = eval_string(
        "(defvar (*cap* i32) (stream-string-output))\n\
         (setf *trace-output* (standard-stream::new *cap*))\n\
         (defun f ((n int)) int (+ n 1))\n\
         (defun quiet () () ())\n\
         (setf *standard-output* (standard-stream::new (stream-string-output)))\n\
         (disassemble f)\n\
         (trace f)\n\
         (match (stream-take-output-string *cap*) ((ok s) s) ((err _) \"<capture failed>\"))",
    );
    assert!(
        !after_disassemble.contains("has a compiled body"),
        "`disassemble` installed a compiled body: {:?}",
        after_disassemble
    );
}

/// A method is disassembled under its `type::method` link name, the same one
/// the compile driver and `Interp::method_key` use.
#[test]
fn a_method_can_be_disassembled() {
    let out = stdout_of(
        "(defstruct pt (pub x int) (pub y int))\n\
         (defmethod total ((self pt)) int (+ self::x self::y))\n\
         (disassemble pt::total true)",
    );
    assert!(out.contains("pt::total"), "the method's link name is missing: {:?}", out);
}

/// A generic definition has no single body to show — the same refusal
/// `compile` and `trace` make, from the same shared resolution.
#[test]
fn a_generic_function_cannot_be_disassembled() {
    let msg = check_error("(defun idg<T> ((x T)) T x)\n(disassemble idg)");
    assert!(msg.starts_with("disassemble: `idg` is generic"), "{}", msg);
}

/// The flag decides what to emit, so it is a literal rather than an
/// expression.
#[test]
fn the_llvm_flag_must_be_a_literal() {
    assert!(check_error("(defun f () int 1)\n(disassemble f (= 1 1))")
        .starts_with("disassemble: the second argument selects LLVM IR"));
    assert_eq!(
        check_error("(disassemble)"),
        "disassemble: (disassemble name [llvm]) — expected 1 or 2 arguments"
    );
}

/// The four interpreter-only forms are refused a lowering rather than given
/// one — the category `compile`/`compile-file`/`dump` are in.
///
/// The message has to come from the *bridge*, which means the free-variable
/// walk that runs before it has to know these tags too; getting that wrong
/// produces "the free-variable walk does not know the tag `trace`" instead,
/// which says nothing useful.
#[test]
fn the_interpreter_only_forms_refuse_to_be_compiled() {
    for (form, expected) in [
        ("(trace target)", "trace: `(trace ...)` is an interpreter-only action"),
        ("(untrace target)", "untrace: `(untrace ...)` is an interpreter-only action"),
        ("(disassemble target)", "disassemble: `(disassemble ...)` is an interpreter-only action"),
        ("(step (target))", "step: `(step ...)` is an interpreter-only action"),
    ] {
        let src = format!(
            "(defun target () int 1)\n\
             (defun uses-it () int (progn {} 0))\n\
             (compile uses-it)",
            form
        );
        let mut h = Heap::with_capacity(1 << 16);
        let mut chk = Checker::new();
        let mut interp = Interp::new();
        load_prelude(&mut h, &mut chk, &mut interp);
        load_compiler(&mut h, &mut chk, &mut interp);
        let r = Reader::new();
        let vs = r.read_all(&mut h, &src).expect("read failed");
        let mut err = None;
        for v in vs {
            let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
            if let Err(e) = interp.exec(&mut h, tl) {
                err = Some(e.to_string());
                break;
            }
        }
        let err = err.unwrap_or_else(|| panic!("`{}` compiled, and should not have", form));
        assert!(err.contains(expected), "wrong refusal for `{}`: {}", form, err);
    }
}
