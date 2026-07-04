//! The `typl` REPL: a read-eval-print loop over the `Reader` -> `Checker` ->
//! `Interp` pipeline, using `rustyline` for Emacs-style line editing/history
//! (Ctrl+P/Ctrl+N to move through history, Ctrl+R to search it, etc. — all
//! `rustyline`'s default `EditMode::Emacs` bindings, matching bash/readline).

use std::path::PathBuf;

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use typelisp::*;

const PROMPT_PRIMARY: &str = "typl> ";
const PROMPT_CONTINUE: &str = "...   ";

fn main() -> rustyline::Result<()> {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    checker.set_redef_policy(parse_redef_policy());
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
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
                try_run_pending(&mut heap, &reader, &mut checker, &mut interp, &mut pending);
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
    checker: &mut Checker,
    interp: &mut Interp,
    pending: &mut String,
) {
    let mark = heap.root_count();
    let forms = match reader.read_all(heap, pending) {
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

    let mut checked = Vec::with_capacity(forms.len());
    let mut check_err = None;
    for v in forms {
        let result = checker.check_form(heap, &*interp, v);
        for w in checker.take_warnings() {
            eprintln!("{}", w);
        }
        match result {
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

    for tl in checked {
        match interp.exec(heap, tl) {
            Ok(Some(v)) => println!("{}", format_value(heap, checker.registry(), &v)),
            Ok(None) => {}
            Err(e) => {
                eprintln!("error: {}", e);
                break;
            }
        }
    }
}

/// True for a `Defmacro` — or a checker-synthesized monomorphization bundle
/// (see [`MONO_BUNDLE_MODULE`]) containing one: a macro whose body calls a
/// generic function comes back wrapped in a `Module` alongside the
/// specializations that call needs. Either way, everything inside is a pure
/// registration (`Defun`/`Defmacro` — the bundle's primary form *is* the
/// macro), so executing it early keeps the "exec never touches the root
/// stack here" invariant `try_run_pending` relies on. A user-written
/// `module` is deliberately *not* matched — executing one early would run
/// arbitrary body expressions out of order.
fn needs_immediate_exec(tl: &TopLevel) -> bool {
    match tl {
        TopLevel::Defmacro { .. } => true,
        TopLevel::Module { path, body } if *path == Path::root(MONO_BUNDLE_MODULE) => {
            body.iter().any(needs_immediate_exec)
        }
        _ => false,
    }
}

/// True if `e` signals "ran out of input mid-form" (need another line)
/// rather than a real syntax error.
fn is_incomplete(e: &Error) -> bool {
    matches!(
        e,
        Error::IllegalEndWhileReadingList | Error::IllegalEndOfString | Error::IllegalEndOfEscapeSequence
    )
}

/// Format an `RtValue` for REPL output, in the reader's own syntax where
/// possible (so the printed form can be pasted back in).
fn format_value(heap: &Heap, reg: &Registry, v: &RtValue) -> String {
    match v {
        RtValue::Int(i) => i.to_string(),
        RtValue::Float(f) => format_float(*f),
        RtValue::Bool(b) => b.to_string(),
        RtValue::Char(c) => format!("#\\{}", c),
        RtValue::Str(s) => format!("{:?}", s),
        RtValue::Unit => "()".to_string(),
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
        RtValue::Sexpr(sv) => format_sexpr(heap, *sv),
        RtValue::Builtin(name) => format!("#<builtin {}>", name),
        RtValue::BuiltinMethod(type_name, method) => format!("#<builtin {}::{}>", type_name, method),
        RtValue::Scope(frames) => format!("#<scope depth={}>", frames.borrow().len()),
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
/// recursively, in the reader's own syntax.
fn format_sexpr(heap: &Heap, v: Value) -> String {
    match v {
        Value::Empty => "()".to_string(),
        Value::Int(i) => i.to_string(),
        // `Sexpr::Float`, a `defstruct`/`Vector<T>`/`cons-cell<K,V>` instance,
        // and a `HashTable<K,V>` are all heap-boxed (`Value::Boxed`, see
        // `BoxedObj`) — `heap.is_struct`/`is_hashtable` tell them apart. A
        // boxed struct prints positionally (no field names at runtime, same
        // as `RtValue::Data`'s field list), recursing through this same
        // function for each field.
        Value::Boxed(id) if heap.is_struct(id) => {
            let parts: Vec<String> =
                (0..heap.struct_field_count(id)).map(|i| format_sexpr(heap, heap.struct_field(id, i))).collect();
            format!("#<{} {}>", heap.struct_type_name(id), parts.join(" "))
        }
        Value::Boxed(id) if heap.is_hashtable(id) => format!("#<hashtable count={}>", heap.hashtable_count(id)),
        // A `Scope<V>` with heap-repr `V` is boxed too since Stage 8 —
        // printed the same way `format_value`'s `RtValue::Scope` arm prints
        // a native-`V` scope.
        Value::Boxed(id) if heap.is_scope(id) => format!("#<scope depth={}>", heap.scope_frame_count(id)),
        // A closure is a boxed value too since Stage 6b — printed opaquely,
        // as the old dedicated `RtValue::Closure` arm did.
        Value::Boxed(id) if heap.is_closure(id) => "#<closure>".to_string(),
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
        Value::Cons(_) => format_list(heap, v),
    }
}

fn format_list(heap: &Heap, mut v: Value) -> String {
    let mut parts = Vec::new();
    loop {
        match v {
            Value::Cons(_) => {
                // Safe: `v` was just matched as `Cons`, so `car`/`cdr` cannot
                // return `Error::NotACons` here.
                let car = heap.car(v).expect("cons car");
                parts.push(format_sexpr(heap, car));
                v = heap.cdr(v).expect("cons cdr");
            }
            Value::Empty => return format!("({})", parts.join(" ")),
            other => return format!("({} . {})", parts.join(" "), format_sexpr(heap, other)),
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
