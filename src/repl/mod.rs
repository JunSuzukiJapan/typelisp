//! The `typl` read-eval-print loop over the `Reader` -> `Checker` -> `Interp`
//! pipeline, using `rustyline` for Emacs-style line editing and history
//! (Ctrl+P/Ctrl+N to move through history, Ctrl+R to search it, and the rest
//! of `rustyline`'s default `EditMode::Emacs` bindings).
//!
//! A line that starts with one of the REPL's own commands ([`commands`]) is
//! that command; anything else is forms to run. Before the first prompt the
//! session runs its startup ([`Startup`]): the init file, then each
//! `--eval`/`--load` in the order given.

pub mod commands;
mod interrupt;

use std::cell::RefCell;
use std::path::{Path as FsPath, PathBuf};
use std::rc::Rc;

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::project::{find_src_root, load_file_flat, Loader};
use crate::*;
use commands::CommandKind;

/// The prompt's start, before the module when that is not `user`.
const PROMPT_NAME: &str = "typl";

/// What runs before the first prompt.
pub struct Startup {
    /// Whether `~/.typlrc` is read (`--no-init` turns it off).
    pub init_file: bool,
    /// `--eval` and `--load`, in the order given.
    pub actions: Vec<StartupAction>,
}

/// One `--eval FORM` or `--load FILE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupAction {
    Eval(String),
    Load(PathBuf),
}

/// What a run of [`Session::run_text`] left behind.
enum Ran {
    /// Every form was read and run, or a failure was reported.
    Finished,
    /// The text ends inside a form; the forms before it ran, and the text
    /// from this character on is what is left of the unfinished one.
    Incomplete(usize),
}

/// The environment a REPL session runs in, and what it keeps between lines.
struct Session<'a> {
    heap: &'a mut Heap,
    reader: &'a Reader,
    checker: Rc<RefCell<Checker>>,
    interp: &'a mut Interp,
    loader: Loader,
    /// The calls that were open when the last line failed (`:backtrace`).
    backtrace: Vec<BacktraceFrame>,
}

/// Runs the REPL over an environment the caller has loaded (the prelude, or
/// an image), until `:quit` or end of input. The heap and the interpreter
/// stay where the caller has them: compiled code is handed their addresses.
pub fn run(heap: &mut Heap, reader: &Reader, checker: Checker, interp: &mut Interp, startup: Startup) -> rustyline::Result<()> {
    // The checker lives behind a shared `RefCell` so the interpreter can reach
    // it to type-check a runtime `(eval ...)` form (`Interp::eval_form`), while
    // the REPL keeps checking each new line through the same cell. Every
    // per-line use borrows it only during the check phase, releasing before
    // the exec phase — see `Session::run_text`'s borrow discipline.
    let checker = Rc::new(RefCell::new(checker));
    interp.set_checker(Rc::clone(&checker));
    // The session's own definitions go in the module `user`, not in the root
    // or the standard library's module.
    if let Err(e) = checker.borrow_mut().enter_file_module(&[crate::types::USER_MODULE.to_string()]) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
    // `use` in the REPL resolves files against the folder it started in (or
    // the project root if a manifest is found above it). `:cd` does not move
    // it: the modules already loaded were found from there.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let src_root = find_src_root(&cwd).unwrap_or(cwd);
    let mut session = Session { heap, reader, checker, interp, loader: Loader::new(src_root), backtrace: Vec::new() };
    interrupt::install();
    session.start(&startup);
    session.read_loop()
}

impl Session<'_> {
    /// The init file, then `--eval`/`--load` in order. A failure is reported
    /// and the rest still runs: the session is there to be used either way.
    fn start(&mut self, startup: &Startup) {
        if startup.init_file {
            if let Some(path) = home_file(".typlrc").filter(|p| p.is_file()) {
                self.load_file(&path);
            }
        }
        for action in &startup.actions {
            match action {
                StartupAction::Eval(src) => {
                    if let Ran::Incomplete(_) = self.run_text("--eval", src, false) {
                        eprintln!("error: --eval: `{}` ends inside a form", src);
                    }
                }
                StartupAction::Load(path) => self.load_file(path),
            }
        }
    }

    /// A startup file's forms, run like typed lines without their values
    /// echoed.
    fn load_file(&mut self, path: &FsPath) {
        let name = path.display().to_string();
        match std::fs::read_to_string(path) {
            Ok(src) => {
                if let Ran::Incomplete(_) = self.run_text(&name, &src, false) {
                    eprintln!("error: {}: the file ends inside a form", name);
                }
            }
            Err(e) => eprintln!("error: cannot read `{}`: {}", name, e),
        }
    }

    fn read_loop(&mut self) -> rustyline::Result<()> {
        let mut rl = DefaultEditor::new()?;
        let hist_path = home_file(".typl_history").unwrap_or_else(|| PathBuf::from(".typl_history"));
        let _ = rl.load_history(&hist_path);

        let mut pending = String::new();
        // Whether `pending` is the form of a `:type` command rather than forms
        // to run.
        let mut showing_type = false;
        loop {
            let prompt = self.prompt();
            let prompt = if pending.is_empty() { prompt } else { format!("{:<width$}", "...", width = prompt.len()) };
            // Native: while this thread waits for a keystroke it holds nothing
            // another thread's collection would have to wait for.
            match self.heap.native(|| rl.readline(&prompt)) {
                Ok(line) => {
                    let _ = rl.add_history_entry(line.as_str());
                    // The REPL's own half of `dribble`: the line as typed. The
                    // other two doors a session's output leaves by are inside the
                    // runtime — see `typelisp_abi::dribble`'s module docs.
                    typelisp_abi::dribble::note(&line);
                    typelisp_abi::dribble::note("\n");
                    let mut text = line.as_str();
                    if pending.is_empty() {
                        match commands::parse(&line) {
                            Some((CommandKind::Quit, _)) => break,
                            Some((CommandKind::Type, form)) => {
                                showing_type = true;
                                text = form;
                            }
                            Some((kind, arg)) => {
                                self.command(kind, arg);
                                continue;
                            }
                            None => {}
                        }
                    }
                    pending.push_str(text);
                    pending.push('\n');
                    if showing_type {
                        self.show_types(&mut pending);
                        showing_type = !pending.is_empty();
                    } else {
                        match self.run_text("<stdin>", &pending, true) {
                            // Keep only what has not been read. Everything
                            // before it has already run; reading it again when
                            // the rest of the form arrives would run it twice.
                            Ran::Incomplete(consumed) => pending = pending.chars().skip(consumed).collect(),
                            Ran::Finished => pending.clear(),
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    pending.clear();
                    showing_type = false;
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

    /// `typl> ` in `user`, and the module's path after `typl` elsewhere.
    fn prompt(&self) -> String {
        let path = typelisp_mem::symbols::ns_path(self.checker.borrow().current_module());
        if path.len() == 1 && path[0] == crate::types::USER_MODULE {
            format!("{}> ", PROMPT_NAME)
        } else {
            format!("{} {}> ", PROMPT_NAME, path.join("::"))
        }
    }

    /// The commands that take a line of their own. `:quit` and `:type` are
    /// the read loop's: one ends it, the other reads forms the way a line
    /// of forms is read.
    fn command(&mut self, kind: CommandKind, arg: &str) {
        let out = match kind {
            CommandKind::Help => Ok(commands::help_text()),
            CommandKind::Pwd => std::env::current_dir()
                .map(|d| format!("{}\n", d.display()))
                .map_err(|e| format!(":pwd: {}", e)),
            CommandKind::Cd => {
                let dir = match arg {
                    "" => home_file("").ok_or_else(|| ":cd: no home folder (HOME is not set)".to_string()),
                    dir => Ok(PathBuf::from(dir)),
                };
                dir.and_then(|dir| {
                    std::env::set_current_dir(&dir).map_err(|e| format!(":cd: {}: {}", dir.display(), e))?;
                    std::env::current_dir().map(|d| format!("{}\n", d.display())).map_err(|e| format!(":cd: {}", e))
                })
            }
            CommandKind::Backtrace => Ok(self.backtrace_text()),
            CommandKind::Locals => self.locals_text(arg),
            CommandKind::Quit | CommandKind::Type => unreachable!("the read loop runs {:?} itself", kind),
        };
        match out {
            Ok(text) => {
                typelisp_abi::dribble::note(&text);
                print!("{}", text);
            }
            Err(e) => eprintln!("error: {}", e),
        }
    }

    /// What `:backtrace` prints: one line per call, innermost first, each
    /// with where it was made.
    fn backtrace_text(&self) -> String {
        if self.backtrace.is_empty() {
            return "no backtrace: the last line did not fail inside a function\n".to_string();
        }
        let calls: Vec<String> = self.backtrace.iter().map(BacktraceFrame::call_text).collect();
        let width = calls.iter().map(|c| c.chars().count()).max().unwrap_or(0);
        let mut out = String::new();
        for (i, (frame, call)) in self.backtrace.iter().zip(&calls).enumerate() {
            match &frame.called_at {
                Some(loc) => out.push_str(&format!("  {}: {:<width$}  called at {}\n", i, call, loc, width = width)),
                None => out.push_str(&format!("  {}: {}\n", i, call)),
            }
        }
        out
    }

    /// What `:locals N` prints: each argument of call `N` of the backtrace.
    fn locals_text(&self, arg: &str) -> Result<String, String> {
        let n = match arg {
            "" => 0,
            n => n.parse::<usize>().map_err(|_| format!(":locals: `{}` is not a call number of :backtrace", n))?,
        };
        let Some(frame) = self.backtrace.get(n) else {
            return Err(match self.backtrace.len() {
                0 => ":locals: no backtrace: the last line did not fail inside a function".to_string(),
                len => format!(":locals: :backtrace has calls 0 to {}", len - 1),
            });
        };
        if frame.args.is_empty() {
            return Ok(format!("{} takes no arguments\n", frame.name));
        }
        Ok(frame.args.iter().map(|(name, value)| format!("  {} = {}\n", name, value)).collect())
    }

    /// Read+check+execute `text`, **one form at a time**: each form is read,
    /// checked and run before the next one is read (`Reader::forms_within`),
    /// which is what a Lisp REPL has always meant by reading from a stream
    /// and is what lets a form affect how the text after it is read. With
    /// `echo`, each value is printed.
    ///
    /// GC-root discipline: the reader's roots protect the raw forms across
    /// checking (macro expansion conses, and can collect), and are the only
    /// thing doing so — `check_form`'s output never retains the raw `Value`.
    /// They accumulate interleaved with the checked forms' own roots and are
    /// all popped together at the end; nothing pops in between, so the stack
    /// stays LIFO. `Interp::exec` is balanced on that stack (it roots a
    /// definition permanently, on the other stack), so running a form
    /// mid-batch leaves the discipline intact.
    fn run_text(&mut self, file: &str, text: &str, echo: bool) -> Ran {
        // A Ctrl-C from before this text was given is not about it, and the
        // last failure's calls are not this text's.
        typelisp_abi::interrupt::clear();
        self.backtrace.clear();
        let Session { heap, reader, checker, interp, loader, backtrace } = self;
        let mark = heap.root_count();
        let mut forms = reader.forms_within(file, text, checker.borrow().current_module());
        // Where the reader stopped after the last *completed* form, in
        // characters.
        let mut consumed = 0usize;
        let mut fatal: Option<Error> = None;
        let mut incomplete = false;

        loop {
            let (v, loc) = match forms.next_form_with(heap, Some(&**interp)) {
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
            // `check_use` finds them in the registry (see `typelisp::project`).
            // A nested load pushes and pops its own read roots strictly above
            // ours, so the root-stack discipline is undisturbed.
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
                    *backtrace = interp.take_backtrace();
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
            let result = checker.borrow_mut().check_form_at(heap, &**interp, v, Some(loc));
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
            // An `(in-module ...)` moves the session, and the next form is read
            // where it moved to.
            forms.set_module(checker.borrow().current_module());
            // `(load "path")` loads inline into the current environment,
            // resolved relative to the process cwd. (A `(load)`ed file whose
            // *own* top-level contains `(eval ...)` is the one corner this
            // borrow doesn't cover — `load_file_flat` execs inline while this
            // `borrow_mut` is held.)
            if let Some(path) = crate::project::load_path_of(heap, tl) {
                if let Err(e) = load_file_flat(heap, reader, &mut checker.borrow_mut(), interp, FsPath::new("."), &path) {
                    fatal = Some(e);
                    break;
                }
            } else {
                match interp.exec(heap, tl) {
                    Ok(Some(v)) if echo => {
                        // The form's checked type decides how a niched `Option`
                        // prints — the word alone cannot say it is one.
                        let repr = {
                            let chk = checker.borrow();
                            chk.expr_type().map(|ty| check::repr::Repr::of_by(ty, &|p| chk.type_kind(p)))
                        };
                        heap.push_root(v);
                        let rendered = interp.render_readably(heap, v, repr.as_ref());
                        heap.pop_root();
                        match rendered {
                            // The REPL's other half of `dribble`: the value
                            // echoed back.
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
                    Ok(_) => {}
                    Err(e) => {
                        *backtrace = interp.take_backtrace();
                        eprintln!("error: {}", e);
                        break;
                    }
                }
            }
            consumed = forms.pos();
        }

        // Anything the dependency loads warned about: the per-form drain above
        // only covers this batch's own forms, and a `use` that failed never
        // reached it.
        for w in checker.borrow_mut().take_warnings() {
            eprintln!("{}", w);
        }
        while heap.root_count() > mark {
            heap.pop_root();
        }
        if incomplete {
            return Ran::Incomplete(consumed);
        }
        if let Some(e) = fatal {
            eprintln!("error: {}", e);
        }
        Ran::Finished
    }

    /// The REPL's `:type`: reads the forms in `pending` and prints the type
    /// checking proves for each, running none of them. Like a line of forms,
    /// it leaves `pending` holding what is unread when a form is not finished
    /// yet, and empties it otherwise.
    fn show_types(&mut self, pending: &mut String) {
        let Session { heap, reader, checker, interp, .. } = self;
        let mark = heap.root_count();
        let mut forms = reader.forms_within("<stdin>", pending, checker.borrow().current_module());
        let mut consumed = 0usize;
        let mut read_any = false;
        let mut incomplete = false;
        let mut fatal: Option<Error> = None;
        loop {
            let (v, loc) = match forms.next_form_with(heap, Some(&**interp)) {
                Ok(Some(pair)) => pair,
                Ok(None) => break,
                Err(e) => {
                    match is_incomplete(&e) {
                        true => incomplete = true,
                        false => fatal = Some(e),
                    }
                    break;
                }
            };
            read_any = true;
            let result = checker.borrow_mut().type_of_form(heap, &**interp, v, Some(loc));
            for w in checker.borrow_mut().take_warnings() {
                eprintln!("{}", w);
            }
            match result {
                Ok(ty) => {
                    let text = ty.to_string();
                    typelisp_abi::dribble::note(&text);
                    typelisp_abi::dribble::note("\n");
                    println!("{}", text);
                }
                Err(e) => {
                    fatal = Some(e);
                    break;
                }
            }
            consumed = forms.pos();
        }
        while heap.root_count() > mark {
            heap.pop_root();
        }
        if incomplete {
            *pending = pending.chars().skip(consumed).collect();
            return;
        }
        pending.clear();
        match fatal {
            Some(e) => eprintln!("error: {}", e),
            None if !read_any => eprintln!("error: :type needs a form, as in `:type (+ 1 2)`"),
            None => {}
        }
    }
}

/// `name` in the home folder; `None` when `HOME` is unset.
fn home_file(name: &str) -> Option<PathBuf> {
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => Some(PathBuf::from(home).join(name)),
        _ => None,
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
