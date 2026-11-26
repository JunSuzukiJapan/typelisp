//! The process's environment: its command line, its environment variables,
//! the home directory.

use typelisp_mem::{spec, wk, Heap, StdModule, TypeSpec, Value};
use typelisp_rt::result_value::{option_in, vector_in, TypeTable};

/// The runtime type each builtin here answers with
/// (`typelisp_rt::result_value`).
pub const RESULT_TYPES: TypeTable = &[
    (wk::F_COMMAND_LINE_ARGS, TypeSpec::Named(StdModule::VEC, "Vector", &[spec::STRING])),
    (wk::F_GETENV, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    (wk::F_HOME_DIRECTORY, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
];

/// The process's own command line, or the override `typl` installs.
///
/// The two ways a typelisp program runs disagree about what `std::env::args`
/// means, which is the whole reason this slot exists:
///
/// | how it runs | `std::env::args()` | what the program should see |
/// |---|---|---|
/// | `./prog a b` (AOT) | `["./prog", "a", "b"]` | the same |
/// | `typl s.typl a b` | `["typl", "s.typl", "a", "b"]` | `["s.typl", "a", "b"]` |
///
/// So `typl` calls [`set_command_line_args`] with the script's own view
/// before running anything, and an AOT executable — whose `main` is the LLVM
/// wrapper in `compile::aot::build_main_wrapper`, which takes no `argc`/
/// `argv` — leaves the slot alone and reads its real argv here. Rust's `std`
/// captures argv at process start independently of `main`'s signature
/// (`_NSGetArgv` on macOS, an `.init_array` entry on Linux), so the wrapper's
/// empty parameter list costs nothing.
///
/// Element 0 names the program in both worlds: the script path interpreted,
/// the executable compiled. That is `sb-ext:*posix-argv*`'s convention and
/// Rust's, and it is the only choice that lets one source file be run either
/// way and index its arguments the same.
static COMMAND_LINE_ARGS: typelisp_mem::sync::OnceLock<Vec<String>> = typelisp_mem::sync::OnceLock::new();

/// Install the command line a typelisp program should see. Called once, by
/// `typl`'s `main`, before any user code runs; ignored if called twice (the
/// first caller wins, and there is only ever one).
pub fn set_command_line_args(args: Vec<String>) {
    let _ = COMMAND_LINE_ARGS.set(args);
}

/// `(command-line-args)`: the command line as a `Vector<string>`.
///
/// Allocating every element before the vector box is deliberate and safe:
/// `alloc_string` and `alloc_struct` never collect — only [`Heap::cons`]
/// does — so the `Vec<Value>` of elements cannot be invalidated between the
/// first string and the box that finally roots them all. A version of this
/// that consed would need each element rooted.
pub fn command_line_args(heap: &mut Heap) -> Value {
    let args: Vec<String> = match COMMAND_LINE_ARGS.get() {
        Some(installed) => installed.clone(),
        None => std::env::args().collect(),
    };
    let elems: Vec<Value> = args.into_iter().map(|a| heap.alloc_string(a)).collect();
    vector_in(heap, RESULT_TYPES, wk::F_COMMAND_LINE_ARGS, elems)
}

/// `(getenv name)`: the environment variable's value, or `None` when it is
/// unset *or* not valid Unicode. CL has no equivalent at all, so the name is
/// the one every CL implementation's extension uses (`sb-posix:getenv`,
/// `ccl:getenv`).
///
/// The two failure modes collapse into one `None` on purpose: a `string` in
/// this language is Rust's, so a variable holding bytes that are not UTF-8
/// has no value that could be handed back, and "unset" is the only honest
/// answer a `Option<string>` can give.
pub fn getenv(heap: &mut Heap, name: &str) -> Value {
    let v = std::env::var(name).ok().map(|s| heap.alloc_string(s));
    option_in(heap, RESULT_TYPES, wk::F_GETENV, v)
}

/// `(home-directory)`: `$HOME`, or `None` when it is unset — the primitive
/// the prelude's `user-homedir-pathname` (CLHS 25.1) is built on.
///
/// `None` rather than a guess: CL says `user-homedir-pathname` may return
/// `NIL` when the home directory cannot be determined, and inventing `"/"` or
/// the current directory would make an unset `$HOME` indistinguishable from a
/// real one.
pub fn home_directory(heap: &mut Heap) -> Value {
    let v = std::env::var("HOME").ok().map(|s| heap.alloc_string(s));
    option_in(heap, RESULT_TYPES, wk::F_HOME_DIRECTORY, v)
}
