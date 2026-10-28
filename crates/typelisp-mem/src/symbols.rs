//! The symbol table: process-global, permanent, and **one table per module**.
//!
//! A symbol's identity is the **address of its [`Symbol`] header** — the same
//! thing CL means by comparing symbols with `eq`, and the same thing
//! [`super::value::ConsRef`] already is for a cons cell. Asking "is this
//! `&rest`?" is a pointer comparison; nothing reads the name back out to
//! answer it.
//!
//! Four properties hold, and each one is load-bearing:
//!
//! 1. **Global.** The table is not part of any `Heap`. It is one process-wide
//!    structure, so a symbol is meaningful across heaps — which matters
//!    because compiled code reaches symbols through a raw tagged word, with no
//!    heap handle in sight.
//! 2. **Permanent.** Nothing here is ever freed. Headers are `Box::leak`ed, so
//!    the `'static` lifetime [`SymRef::name`] hands out is real rather than
//!    asserted, and the address can never dangle. That is what makes the
//!    `unsafe` below simple enough not to need Miri watching it: there is no
//!    deallocation to get wrong.
//! 3. **One table per module.** A module owns the symbols written inside it.
//!    Two modules that both write `foo` get **two different symbols** — the
//!    module a name is written in is part of what the name means.
//! 4. **Sharing is by pointer, never by name.** A module that must see another
//!    module's symbol holds *that symbol* in its own table ([`import`]): one
//!    header, two names pointing at it, so `==` on the addresses answers
//!    "the same symbol" with nothing to fall back on. There is no search up
//!    the module tree — a lookup reads exactly one table.
//!
//! ## The system module
//!
//! [`NsId::SYSTEM`] holds the fixed vocabulary ([`BUILTIN_SYMBOLS`]) and
//! nothing else. It is created once; every module created afterwards
//! *imports* those symbols by pointer, which is why `defun` written inside
//! `(module m ...)` is the very same symbol as `defun` written at the top
//! level — and why the checker can keep recognizing a definition by identity.
//! Without that import a module's `defun` would be `m`'s own new symbol, and
//! every definition inside a module would silently stop being a definition.
//!
//! [`NsId::ROOT`] is an ordinary module that happens to be first: the prelude,
//! the core macro layer and the island are read into it.
//!
//! ## Keywords
//!
//! [`NsId::KEYWORD`] is CL's `KEYWORD` package: the module `keyword` at the
//! root, which every keyword belongs to whichever module wrote it. The reader
//! turns `:foo` into the symbol *named* `foo` in that module ([`intern_keyword`]);
//! the colon is how a keyword is spelled, not part of its name. Being a
//! keyword is being owned by that module ([`SymRef::is_keyword`]), and nothing
//! about a name says so: `(intern ":foo")` is an ordinary symbol whose name
//! starts with a colon. The keyword module imports nothing, and no other
//! module imports a keyword, so `foo` and `:foo` never meet in one table.
//!
//! ## Why an address and not an index
//!
//! An index needs a reverse table (`id -> name`) to print with, and is only
//! meaningful relative to the table that issued it. An address *is* the
//! header, so the name is one dereference away and no reverse table exists at
//! all. The tagged compiled-code representation carries the address directly
//! (`typelisp-abi`'s `encode`), exactly as it already does for `Cons`.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, OnceLock};

// ---- the well-known vocabulary ------------------------------------------

/// Compile-time string equality, for [`sym_index`].
const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Where `name` sits in [`BUILTIN_SYMBOLS`]. A name that is not in the table
/// is a compile error.
const fn sym_index(name: &str) -> u32 {
    let mut i = 0;
    while i < BUILTIN_SYMBOLS.len() {
        if str_eq(BUILTIN_SYMBOLS[i], name) {
            return i as u32;
        }
        i += 1;
    }
    panic!("a `wk` constant names a symbol that is not in BUILTIN_SYMBOLS")
}

/// Declares the compiler's fixed vocabulary, and one [`wk`] constant per entry.
///
/// One list, two outputs: the constant's value is *computed from* the table by
/// [`sym_index`] at compile time, so a constant and its name cannot drift
/// apart — a name that moves takes its constant with it, and a constant whose
/// name is not in the table fails to compile.
macro_rules! well_known_symbols {
    ($($section:literal $($konst:ident => $name:literal)+)+) => {
        /// The names the compiler compares symbols against — the head of a
        /// definition form, a lambda-list marker, a core-IR operator tag, a
        /// `loop` clause word. Interned when the table is created: an entry
        /// spelled with a leading colon is the keyword of that name (the
        /// colon dropped) in the keyword module, every other one is the
        /// system module's, imported into each module.
        ///
        /// This list lives in the memory crate rather than with the front end
        /// that gives each name meaning, because the table itself does. The
        /// crate does not interpret the names; it interns them, the way it
        /// interns any other string.
        pub const BUILTIN_SYMBOLS: &[&str] = &[$($($name,)+)+];

        /// One constant per [`BUILTIN_SYMBOLS`] entry: the value
        /// [`SymRef::well_known`] returns for that symbol.
        ///
        /// **These are not identities.** Two symbols are the same when their
        /// addresses are equal; a `wk` constant only answers "is this
        /// particular member of the fixed vocabulary?", so that a dispatch
        /// over that vocabulary can stay a `match` (an address cannot be a
        /// pattern). Every symbol outside the vocabulary reports
        /// [`NOT_WELL_KNOWN`], so `well_known()` must never be used to compare
        /// two symbols to each other — use `==` on the [`SymRef`]s.
        pub mod wk {
            $($(
                #[doc = $section]
                pub const $konst: u32 = super::sym_index($name);
            )+)+
        }
    };
}

/// What [`SymRef::well_known`] reports for a symbol outside the fixed
/// vocabulary — every user-written name, and everything `gensym` makes.
pub const NOT_WELL_KNOWN: u32 = u32::MAX;

well_known_symbols! {
    "Syntax the checker reads."
    QUOTE => "quote"
    UNQUOTE => "unquote"
    UNQUOTE_SPLICING => "unquote-splicing"
    THE => "the"
    FN => "fn"
    PUB => "pub"
    // Retired 2026-10-17: the bound clause is `(:where ...)` now, a keyword.
    RETIRED_WHERE => "where"
    RETURN => "return"
    REST => "&rest"
    OPTIONAL => "&optional"
    KEY => "&key"
    DYN => ":dyn"
    // Retired 2026-10-17: `match`'s value pattern and `loop`'s binding are
    // spelled with the keyword `:=` now, not a bare `=`.
    RETIRED_EQUALS => "="
    COLON_EQUALS => ":="

    "Core-IR operator tags shared with the island (`compile-value`'s dispatch)."
    // **This table is positional.** `sym_index` is the offset into it, and
    // compiled island code carries those offsets, so an entry may never be
    // inserted or removed in the middle — and a rename in place silently
    // changes what an already-compiled index resolves to. Replacements go at
    // the *end* of the table (see the last section).
    //
    // `int`/`float` were the core-IR tags before the width work; they were
    // retired rather than deleted, because deleting them would renumber
    // everything after and invalidate every committed artifact. `float` still
    // is (the live spelling is `FLOAT_ANY_WIDTH`). `int` came back into
    // service, same slot and same name, when `int` became the
    // arbitrary-precision type: it is that type's name, its `Repr` tag, and
    // the core-IR tag of its literal `(int N)` — none of which an artifact
    // compiled while the slot was retired could have carried.
    INT => "int"
    RETIRED_FLOAT => "float"
    BIGNUM => "bignum"
    RATIO => "ratio"
    CHAR => "char"
    BOOL => "bool"
    STR => "str"
    UNIT => "unit"
    VAR => "var"
    SET => "set"
    GLOBAL => "global"
    SET_GLOBAL => "set-global"
    LET => "let"
    LAMBDA => "lambda"
    LABELS => "labels"
    CALL => "call"
    ASSOC => "assoc"
    APPLY => "apply"
    IF => "if"
    LOOP => "loop"
    BREAK => "break"
    PANIC => "panic"
    MATCH => "match"
    CONSTRUCT => "construct"
    FIELD_GET => "field-get"
    FIELD_SET => "field-set"
    DYN_NEW => "dyn-new"
    DYN_UPCAST => "dyn-upcast"
    DYN_CALL => "dyn-call"
    DYN_VALUE => "dyn-value"
    CATCH => "catch"
    THROW => "throw"
    UNWIND_PROTECT => "unwind-protect"

    "Core-IR tags with no island counterpart: the bridge turns each into something else."
    SYM => "sym"
    FNREF => "fnref"
    METHODREF => "methodref"
    COMPILE_FN => "compile-fn"
    METHOD => "method"
    PAT_WILD => "pat-wild"
    PAT_EMPTY => "pat-empty"
    RETIRED_PAT_NONEMPTY => "pat-nonempty"
    PAT_BIND => "pat-bind"
    PAT_LIT => "pat-lit"
    PAT_GUARD => "pat-guard"
    PAT_CTOR => "pat-ctor"
    PAT_TYPETEST => "pat-typetest"

    "Core-IR top-level tags, and the definition-form heads they are lowered from."
    DEFUN => "defun"
    DEFMETHOD => "defmethod"
    DEFMACRO => "defmacro"
    DEFVAR => "defvar"
    DEFSTRUCT => "defstruct"
    DEFENUM => "defenum"
    MODULE => "module"
    USE => "use"
    LOAD => "load"
    EXPR => "expr"
    DEFSIGNATURE => "defsignature"
    DEFCONSTANT => "defconstant"
    DEFTRAIT => "deftrait"
    DEFTYPE => "deftype"
    IMPL => "impl"

    "`Sexpr`'s own constructors, as a downcast pattern's head can spell them."
    NIL => "nil"
    CONS => "cons"
    PATH => "path"

    "Forms the pretty printer lays out code-shaped, beyond those already above."
    PROGN => "progn"
    COND => "cond"
    AND => "and"
    OR => "or"
    LIST => "list"
    BLOCK => "block"
    WHEN => "when"
    UNLESS => "unless"
    WHILE => "while"
    LET_STAR => "let*"
    CASE => "case"
    SETF => "setf"
    AS => "as"
    DOLIST => "dolist"
    DOTIMES => "dotimes"
    DOITER => "doiter"
    UNTIL => "until"
    DO => "do"

    "Values a well-known global may hold, compared against as symbols."
    // Retired 2026-10-25 with `*print-case*`: symbols print as spelled.
    RETIRED_UPCASE => ":upcase"
    RETIRED_CAPITALIZE => ":capitalize"

    "The synthetic module `check_program` collects monomorphized specializations into."
    MONO_BUNDLE => "<monomorph specializations>"

    "`loop`'s clause words. Keywords, so distinct from the bare symbols above."
    KW_WITH => ":with"
    KW_FOR => ":for"
    KW_AS => ":as"
    KW_REPEAT => ":repeat"
    KW_INITIALLY => ":initially"
    KW_FINALLY => ":finally"
    KW_IN => ":in"
    KW_ACROSS => ":across"
    KW_ON => ":on"
    KW_FROM => ":from"
    KW_DOWNFROM => ":downfrom"
    KW_UPFROM => ":upfrom"
    KW_TO => ":to"
    KW_BELOW => ":below"
    KW_DOWNTO => ":downto"

    "Names appended after the width work (2026-09-02). Appended, never inserted — see the positional note on `RETIRED_INT`."
    // The core-IR operator tags. `-any-width`, not `int`/`float`: one node
    // serves every width of its family, and the name has to say so. The
    // literal's payload is already the number the type names — a narrow
    // integer normalized, a binary32 float rounded and widened — so the
    // constant it compiles to is correct whichever width the type was, and no
    // width is discarded by leaving it out of the node. The width that *does*
    // matter downstream is the one the value is boxed at, and that reaches
    // the island by a different route: `Repr::field_kind`.
    INT_ANY_WIDTH => "int-any-width"
    FLOAT_ANY_WIDTH => "float-any-width"
    // `Sexpr`'s numeric constructors, spelled by width because a `Sexpr` node
    // keeps the width of the value put into it. These used to share `INT`/
    // `FLOAT` with the core-IR tags — one spelling standing for two different
    // things, which is exactly what folding the widths away allowed.
    I32 => "i32"
    F64 => "f64"
    F32 => "f32"
    KW_ABOVE => ":above"
    KW_BY => ":by"
    KW_THEN => ":then"
    KW_DO => ":do"
    KW_DOING => ":doing"
    KW_COLLECT => ":collect"
    KW_COLLECTING => ":collecting"
    KW_APPEND => ":append"
    KW_APPENDING => ":appending"
    KW_SUM => ":sum"
    KW_SUMMING => ":summing"
    KW_COUNT => ":count"
    KW_COUNTING => ":counting"
    KW_MAXIMIZE => ":maximize"
    KW_MAXIMIZING => ":maximizing"
    KW_MINIMIZE => ":minimize"
    KW_MINIMIZING => ":minimizing"
    KW_ALWAYS => ":always"
    KW_NEVER => ":never"
    KW_THEREIS => ":thereis"
    KW_WHILE => ":while"
    KW_UNTIL => ":until"
    KW_RETURN => ":return"
    KW_WHEN => ":when"
    KW_IF => ":if"
    KW_UNLESS => ":unless"
    KW_ELSE => ":else"
    KW_INTO => ":into"

    "Appended after the narrow-integer `Sexpr` variants (2026-09-02), at the very end of the table for the reason `RETIRED_INT` records."
    // `Sexpr`'s constructors for the five integer widths that are not `i32`
    // — peers of `I32`/`F64`/`F32` above, listed here rather than beside
    // them because this table is positional and appending is the only edit
    // that moves nothing.
    I8 => "i8"
    I16 => "i16"
    U8 => "u8"
    U16 => "u16"
    U32 => "u32"

    "Appended for `return-from` (2026-09-04), at the very end for the reason `RETIRED_INT` records."
    // `block`'s own entry is already above, among the forms the pretty printer
    // lays out code-shaped; it is now a core-IR tag as well, which changes
    // nothing about the table. Both are a source special form and a core-IR
    // tag, the way `loop`/`break`/`return` are. `block` is the *lexical* named
    // escape — `catch`/`throw` remain the dynamic one — so the name it carries
    // is resolved where it is written and never travels at run time the way a
    // `throw`'s tag does.
    RETURN_FROM => "return-from"

    "Appended for Stage 9a (2026-09-04), again at the very end. `in-module` is
     the flat form of `(module ...)`; `import` is the CL-compatible spelling of
     `use`, and `shadowing-import` the one that means to take a bare name
     another binding already holds. CL's `unuse-package` has no entry because
     it is not adopted — see cl-parity-plan.md Stage 9a. `shadow` came later
     (the last section)."
    IN_MODULE => "in-module"
    IMPORT => "import"
    SHADOWING_IMPORT => "shadowing-import"

    "Appended for Stage 9b (2026-09-04). A *source* form only — it lowers to the
     same `(defvar ...)` node with a trailing `true`, so the core vocabulary is
     unchanged (see `Checker::defvar_form`)."
    DEFPARAMETER => "defparameter"

    "Appended for Stage 4a (2026-09-05): the extended `loop`'s `:named`, held
     back until `block`/`return-from` existed to give it a meaning. A clause
     word like its neighbours above, listed here rather than beside them
     because this table is positional — see the module comment."
    KW_NAMED => ":named"

    "Appended for the REPL tool layer (2026-09-05). Four core-IR tags with no
     island counterpart: `trace`/`untrace`/`step`/`disassemble-fn` are
     interpreter-only, the same category `compile-fn` is in, so the bridge
     never sees one. Appended rather than filed beside `COMPILE_FN` because
     this table is positional — see the module comment."
    TRACE => "trace"
    UNTRACE => "untrace"
    STEP => "step"
    DISASSEMBLE_FN => "disassemble-fn"

    "Appended for the C FFI (2026-09-06). A top-level tag, so its natural
     home is beside `DEFSIGNATURE` — the declaration it is shaped after —
     but this table is positional and an entry may only be appended; see
     the module comment."
    DEFFFI => "defffi"

    "Appended for the concurrency work (2026-09-09) as the form that starts a
     task, then spelled `go`. Retired on 2026-09-25 when the form was renamed
     `task` (appended as `TASK` at the end): a rename in place would change
     what an already-compiled index resolves to. Nothing produces this tag."
    RETIRED_GO => "go"

    "Appended for `select` (2026-09-12). Waiting on several channels at once, \
     and the reason it is a form rather than a macro over a function is the \
     same as `task`'s and then some: the arms' bodies are *its* subforms, a \
     receive arm binds a name whose type comes from the channel, and the whole \
     form's type is the join of the arms' — none of which a macro can spell.\n\n\
     The channel expressions and the values to send are **not** in this node. \
     The checker binds them in a `let` it wraps around this one, so operand \
     evaluation — including a suspension in the middle of it — is the `let`'s \
     job, and nothing here evaluates anything that can stop."
    SELECT => "select"

    "Appended for the `int` boundary (2026-09-16). A Rust-implemented builtin's
     `int`-typed parameters and result cross into compiled code as raw machine
     words (`Checker::int_boundary_raw`): `(untag-int E)` is a fixnum's payload
     and a language error for a bignum, `(tag-int E)` the tagged word of a raw
     one. The interpreter runs both as the identity, keeping only the check."
    UNTAG_INT => "untag-int"
    TAG_INT => "tag-int"

    "Appended for the `Option<T>` niche (2026-09-17). `(some-of REPR E)` is
     `E`'s value as a tagged field word (`none` stays `sexpr`'s empty-list
     construct); `(pat-some REPR P)` rejects the empty-list word and applies
     `P` to the payload it untags by `REPR`; `(box-option KEY E)` puts a
     niched value into the `BoxedObj::Enum` box a `Sexpr` slot needs. The
     `Option<Sexpr>`-only `pat-nonempty` retired into `pat-some`."
    SOME_OF => "some-of"
    PAT_SOME => "pat-some"
    BOX_OPTION => "box-option"

    "Appended for `cond`/`case` in file modules (2026-09-17). The core macros
     recognise their fallthrough clause with `(eq (car clause) (quote else))`,
     and `eq` on symbols is address equality: the `else` a user writes has
     to *be* the one the macro quoted. It was, as long as everything was read
     at the root. A script run as `typl file.typl` reads in its own file
     module, whose `else` was a different symbol — and every `(cond ... (else
     ...))` in a script failed with `unbound variable: else` while the same
     form worked at the REPL. In the vocabulary, the name is imported into
     every module from the system one, and there is one `else` again.
     `select`'s arm compares by name in the checker and never noticed.
     Retired on 2026-10-17, when the fallthrough clause became `:else` — a
     keyword, which is one symbol in every module without a vocabulary entry.
     Kept, not removed, because this table is positional."
    RETIRED_ELSE => "else"

    "Appended for tasks in AOT executables (2026-09-17). Two nodes only the
     compile bridge builds, out of a `(task RET-R CALL)`: `(spawn RET-R
     LAMBDA)` hands the scheduler a closure to run as a task and wakes with
     its handle, and `(tag R E)` is `E`'s value in its tagged form — what the
     closure answers with, so a scheduler with no representations in hand
     (an AOT executable's) still receives a word it can keep. The checker
     never writes either; the free-variable walk has to know them because
     the synthesized `let`/`lambda` around them go through the ordinary
     translations."
    SPAWN => "spawn"
    TAG => "tag"

    "Appended for `thread` (2026-09-24). `(thread (f a b))` is `task`'s shape
     and `task`'s rule — the parts of the call are evaluated where the form
     stands — for a task the scheduler runs on an OS thread of its own. The
     bridge lowers it as it lowers `task`, except that the closure goes to
     `(spawn-thread RET-R LAMBDA)`, which wakes with a `Thread<T>` handle."
    THREAD => "thread"
    SPAWN_THREAD => "spawn-thread"

    "Appended for the rename of `go` (2026-09-25), which retired `RETIRED_GO`.
     `task` starts a task with a call: `(task (f a b))` evaluates `f` and
     every argument where it stands and only the call itself happens in the
     new task, which is why it is a form with a callee and arguments rather
     than a thunk. It cannot be a macro over a function — `lambda` needs its
     return type written out and a macro does not know what `(f a b)`
     returns; only the checker does. `(task ...)` gives a `Task<T>` as
     `(thread ...)` gives a `Thread<T>`."
    TASK => "task"

    "Appended for `destructuring-bind` (2026-09-30). `&body` is `&rest`'s
     synonym in a destructuring lambda list, and the library recognizes it by
     identity like the other lambda-list keywords. Outside the vocabulary each
     module read its own `&body`, so the library's did not match one written
     in a user's file."
    AMP_BODY => "&body"

    "Appended for `match` guards (2026-10-10). `(pat-when PAT TEST)` matches
     `PAT`, then evaluates `TEST` with `PAT`'s bindings in scope and fails
     over to the next arm when it is false. A pattern node rather than a
     field of the arm, so that every tier tests it where it already tests a
     pattern, and failing it is failing the pattern."
    PAT_WHEN => "pat-when"

    "Appended 2026-10-17, when keywords moved into a module of their own and
     every comparison of a symbol became a comparison of pointers. Each of
     these used to be recognized by reading its name back as text, which a
     symbol written in a module of the same name also passed; in the
     vocabulary, each one is one symbol everywhere, compared by `is`. The
     keywords among them live in the keyword module (see the module doc
     comment), the rest in the system one."
    NOT => "not"
    UNSAFE => "unsafe"
    DEF_C_STRUCT => "def-c-struct"
    SELF => "self"
    TEST => "test"
    PTR => "ptr"
    RECV => "recv"
    SEND => "send"
    KW_TYPE => ":type"
    KW_WHERE => ":where"
    KW_OR => ":or"
    KW_CONSTRUCTOR => ":constructor"
    KW_COPIER => ":copier"
    KW_INCLUDE => ":include"
    KW_CONC_NAME => ":conc-name"
    KW_PREDICATE => ":predicate"
    KW_INITIAL_OFFSET => ":initial-offset"
    KW_LIBRARY => ":library"
    KW_PREFIX => ":prefix"
    KW_PER_LINE_PREFIX => ":per-line-prefix"
    KW_SUFFIX => ":suffix"
    // Retired 2026-10-25 with `*print-case*`.
    RETIRED_KW_DOWNCASE => ":downcase"
    KW_LINEAR => ":linear"
    KW_FILL => ":fill"
    KW_MISER => ":miser"
    KW_MANDATORY => ":mandatory"
    KW_BLOCK => ":block"
    KW_CURRENT => ":current"
    KW_LINE => ":line"
    KW_SECTION => ":section"
    KW_LINE_RELATIVE => ":line-relative"
    KW_SECTION_RELATIVE => ":section-relative"
    INTERN => "intern"
    UNDERSCORE => "_"
    "Appended 2026-10-17, when the special-form dispatch moved from matching
     the head's name to matching the head symbol. Every head the dispatch
     recognizes is in the vocabulary, so it is one symbol in every module."
    INCF => "incf"
    DECF => "decf"
    ROTATEF => "rotatef"
    SHIFTF => "shiftf"
    MACROLET => "macrolet"
    SYMBOL_MACROLET => "symbol-macrolet"
    TRY_AS => "try-as"
    COMPILE => "compile"
    QUASIQUOTE => "quasiquote"
    FORMAT => "format"
    PRINT => "print"
    PRINTLN => "println"
    EPRINT => "eprint"
    EPRINTLN => "eprintln"
    SOURCE_FILE => "source-file"
    PPRINT => "pprint"
    PPRINT_FILL => "pprint-fill"
    PPRINT_LINEAR => "pprint-linear"
    PPRINT_TABULAR => "pprint-tabular"
    PPRINT_LOGICAL_BLOCK => "pprint-logical-block"
    C_ALLOC => "c-alloc"
    C_REF => "c-ref"
    C_DEREF => "c-deref"
    MAKE_INSTANCE => "make-instance"
    DOCUMENTATION => "documentation"
    DISASSEMBLE => "disassemble"
    ED => "ed"
    INTERNAL_MODULE_NAME => "%internal"
    ATTRIBUTE => "attribute"
    "Appended 2026-10-17 with the above: the island-only core-IR tags the
     bridge builds, so every tag is made from its constant and recognized by
     it, never by its name."
    APPLY_INDIRECT => "apply-indirect"
    CELLVAR => "cellvar"
    CELLSET => "cellset"
    GLOBAL_INIT => "global-init"
    HASHTABLE_OP => "hashtable-op"
    LLVM_OP => "llvm-op"
    SUSPEND => "suspend"
    VECTOR_OP => "vector-op"
    "Appended 2026-10-17 with the above: the value representations the core
     IR writes as symbols (`Repr::write`), the ones not already above."
    REPR_HANDLE => "handle"
    REPR_RAW_WORD => "raw-word"
    REPR_RATIONAL => "rational"
    REPR_RANDOM_STATE => "random-state"
    REPR_SEXPR => "sexpr"
    REPR_STRUCT => "struct"
    REPR_ENUM => "enum"
    REPR_DYN => "dyn"
    REPR_SCOPE => "scope"
    REPR_VECTOR => "vector"
    REPR_HASHTABLE => "hashtable"
    REPR_NICHE => "niche"
    REPR_NONE => "none"

    "How the reader writes a generic type and a member of one: `Name<A,B>`
     reads as `(%type-apply Name A B)`, and `Name<A>::m` as
     `(%type-member (%type-apply Name A) m)` — a structure, so no symbol's
     name ever holds a type or a path. Reserved: no program binds them."
    TYPE_APPLY => "%type-apply"
    TYPE_MEMBER => "%type-member"

    "How the reader writes a member of a value: `p::x`, where `p` is neither
     a module nor a type, reads as `(%value-member p x)`. Which `x` it is
     depends on `p`'s type, so the analysis resolves it. Reserved."
    VALUE_MEMBER => "%value-member"

    "Appended for the explicit-symbols plan's stage 4b (2026-10-21). CL's
     `shadow`: `(shadow name...)` makes each name, from there on in the module
     it is written in, read as that module's own symbol rather than the one
     the standard library offers every module — which a module must do
     before it defines one of those names."
    SHADOW => "shadow"

    "Appended for the explicit-symbols plan's stage 5a (2026-10-23): how the
     core IR writes a type — a value's runtime identity, a built-in's result
     type, a field's type template. A named type is `(type-named PATH ARG...)`
     with `PATH` the definition's path from the root; a field's template
     names the owning type's `N`th parameter as `(type-param N)`, and a
     generic definition's body, lowered but never run, names one of its own
     parameters as `(type-var \"NAME\")`."
    TYPE_NAMED => "type-named"
    TYPE_FN => "type-fn"
    TYPE_DYN => "type-dyn"
    TYPE_PTR => "type-ptr"
    TYPE_CSTRUCT => "type-cstruct"
    TYPE_PARAM => "type-param"
    TYPE_VAR => "type-var"
    TYPE_UNIT => "type-unit"
    TYPE_NEVER => "type-never"

    "Appended for the explicit-symbols plan's stage 5d (2026-10-23): the
     island's `(type-ref N)`, the type at place `N` of the compiling
     process's type table, which compiled code reads from its module's
     table."
    TYPE_REF => "type-ref"
    "Appended for the explicit-symbols plan's stage 5d (2026-10-23): a type
     handed to a builtin as a value, `(type-value TYPE)` — the id of the
     type `TYPE` writes."
    TYPE_VALUE => "type-value"

    "Appended for case-sensitive names (2026-10-25): the built-in types'
     names, so every module reads each as one symbol and a type is told
     from another by that symbol rather than by its spelling — the island
     included, which compares a receiver's type with `(eq ty 'Sexpr)`."
    OPTION => "Option"
    RESULT => "Result"
    SEXPR => "Sexpr"
    VECTOR => "Vector"
    HASHTABLE => "HashTable"
    SCOPE => "Scope"
    CHAN => "Chan"
    TASK_TYPE => "Task"
    THREAD_TYPE => "Thread"
    ARRAY => "Array"
    STRING => "string"
    SYMBOL => "symbol"
    PARSE_INT_ERROR => "ParseIntError"
    PARSE_FLOAT_ERROR => "ParseFloatError"
    READ_ERROR => "ReadError"
    EVAL_ERROR => "EvalError"
    FILE_ERROR => "FileError"
    NET_ERROR => "NetError"
    LLVM_MODULE => "llvm-module"
    LLVM_FUNCTION => "llvm-function"
    LLVM_BUILDER => "llvm-builder"
    LLVM_BASIC_BLOCK => "llvm-basic-block"
    LLVM_VALUE => "llvm-value"
    C_LONG => "c-long"
    C_ULONG => "c-ulong"
    NEVER => "!"
    SELF_TYPE => "Self"

    "Appended for the explicit-symbols plan's stage 7b (2026-10-28): the names
     of the built-in methods, which their dispatch compares a method's symbol
     against. Owned by `%member`, where the name of every type's member is
     made ([`member_symbol`]); the four spelled like an older member of this
     table (`=`, `set`, `send`, `recv`) are that member, taken into `%member`."
    M_OP_MUL => "*"
    M_OP_ADD => "+"
    M_OP_SUB => "-"
    M_OP_DIV => "/"
    M_OP_NE => "/="
    M_OP_LT => "<"
    M_OP_LE => "<="
    M_OP_GT => ">"
    M_OP_GE => ">="
    M_ACOS => "acos"
    M_ACOSH => "acosh"
    M_ADD_COROUTINE_FUNCTION => "add-coroutine-function"
    M_ADD_FUNCTION => "add-function"
    M_ADD_FUNCTION_WITH_ENV => "add-function-with-env"
    M_ALLOCA_ARGS => "alloca-args"
    M_ALPHAP => "alphap"
    M_APPEND => "append"
    M_APPEND_BLOCK => "append-block"
    M_ASH => "ash"
    M_ASIN => "asin"
    M_ASINH => "asinh"
    M_ATAN => "atan"
    M_ATANH => "atanh"
    M_AVAILABLE_PARALLELISM => "available-parallelism"
    M_BLOCK_TERMINATED_P => "block-terminated?"
    M_BUCKET_COUNT => "bucket-count"
    M_BUCKET_DELETE => "bucket-delete"
    M_BUCKET_KEY => "bucket-key"
    M_BUCKET_PUT => "bucket-put"
    M_BUCKET_VALUE => "bucket-value"
    M_BUILD_ADD => "build-add"
    M_BUILD_AND => "build-and"
    M_BUILD_ASHR => "build-ashr"
    M_BUILD_BR => "build-br"
    M_BUILD_CALL => "build-call"
    M_BUILD_CALL_WITH_ENV => "build-call-with-env"
    M_BUILD_COND_BR => "build-cond-br"
    M_BUILD_FADD => "build-fadd"
    M_BUILD_FCEIL => "build-fceil"
    M_BUILD_FCMP_EQ => "build-fcmp-eq"
    M_BUILD_FCMP_GE => "build-fcmp-ge"
    M_BUILD_FCMP_GT => "build-fcmp-gt"
    M_BUILD_FCMP_LE => "build-fcmp-le"
    M_BUILD_FCMP_LT => "build-fcmp-lt"
    M_BUILD_FCMP_NE => "build-fcmp-ne"
    M_BUILD_FCOS => "build-fcos"
    M_BUILD_FDIV => "build-fdiv"
    M_BUILD_FEXP => "build-fexp"
    M_BUILD_FFLOOR => "build-ffloor"
    M_BUILD_FLOG => "build-flog"
    M_BUILD_FMAXNUM => "build-fmaxnum"
    M_BUILD_FMINNUM => "build-fminnum"
    M_BUILD_FMUL => "build-fmul"
    M_BUILD_FN_ADDRESS => "build-fn-address"
    M_BUILD_FPOW => "build-fpow"
    M_BUILD_FPTOSI => "build-fptosi"
    M_BUILD_FREE => "build-free"
    M_BUILD_FREM => "build-frem"
    M_BUILD_FROUND => "build-fround"
    M_BUILD_FROUND32 => "build-fround32"
    M_BUILD_FSIN => "build-fsin"
    M_BUILD_FSQRT => "build-fsqrt"
    M_BUILD_FSUB => "build-fsub"
    M_BUILD_FTRUNC => "build-ftrunc"
    M_BUILD_ICMP_EQ => "build-icmp-eq"
    M_BUILD_ICMP_GE => "build-icmp-ge"
    M_BUILD_ICMP_GT => "build-icmp-gt"
    M_BUILD_ICMP_LE => "build-icmp-le"
    M_BUILD_ICMP_LT => "build-icmp-lt"
    M_BUILD_ICMP_NE => "build-icmp-ne"
    M_BUILD_INT_TO_PTR => "build-int-to-ptr"
    M_BUILD_LSHR => "build-lshr"
    M_BUILD_MAKE_CLOSURE => "build-make-closure"
    M_BUILD_MALLOC => "build-malloc"
    M_BUILD_MUL => "build-mul"
    M_BUILD_OR => "build-or"
    M_BUILD_PTR_TO_INT => "build-ptr-to-int"
    M_BUILD_RET => "build-ret"
    M_BUILD_SADD_OVERFLOW => "build-sadd-overflow"
    M_BUILD_SELECT => "build-select"
    M_BUILD_SHL => "build-shl"
    M_BUILD_SITOFP => "build-sitofp"
    M_BUILD_SLOT_PTR => "build-slot-ptr"
    M_BUILD_SMUL_OVERFLOW => "build-smul-overflow"
    M_BUILD_SSUB_OVERFLOW => "build-ssub-overflow"
    M_BUILD_SUB => "build-sub"
    M_BUILD_XOR => "build-xor"
    M_CAP => "cap"
    M_CEILING => "ceiling"
    M_CHAR_TO_INT => "char->int"
    M_CHAR_TO_STRING => "char->string"
    M_CLEAR => "clear"
    M_CLONE_FRAMES => "clone-frames"
    M_CLOSE => "close"
    M_CONST_WORD => "const-word"
    M_COROUTINE_APPLY => "coroutine-apply"
    M_COROUTINE_BEGIN => "coroutine-begin"
    M_COROUTINE_CALL => "coroutine-call"
    M_COROUTINE_CALL_ENV => "coroutine-call-env"
    M_COROUTINE_DYN_CALL => "coroutine-dyn-call"
    M_COROUTINE_END => "coroutine-end"
    M_COROUTINE_SUSPEND => "coroutine-suspend"
    M_COROUTINE_UNWIND => "coroutine-unwind"
    M_COS => "cos"
    M_COSH => "cosh"
    M_COUNT => "count"
    M_CREATE => "create"
    M_CURRENT_ID => "current-id"
    M_DENOMINATOR => "denominator"
    M_DIGITP => "digitp"
    M_DOWNCASE => "downcase"
    M_ENTRIES => "entries"
    M_EQ => "eq"
    M_EQL => "eql"
    M_EQUAL => "equal"
    M_EQUALP => "equalp"
    M_EXP => "exp"
    M_EXPT => "expt"
    M_FLOAT_TO_F32 => "float->f32"
    M_FLOAT_TO_F64 => "float->f64"
    M_FLOAT_TO_INT => "float->int"
    M_FLOAT_TO_RATIO => "float->ratio"
    M_FLOOR => "floor"
    M_FRAME_CLEAR_HANDLER => "frame-clear-handler"
    M_FRAME_SET_HANDLER => "frame-set-handler"
    M_FRAME_SLOT => "frame-slot"
    M_FRAME_SLOT_ROOTED => "frame-slot-rooted"
    M_FUNCTION_PARAM => "function-param"
    M_GET => "get"
    M_GET_FUNCTION => "get-function"
    M_INT_TO_C_LONG => "int->c-long"
    M_INT_TO_C_ULONG => "int->c-ulong"
    M_INT_TO_CHAR => "int->char"
    M_INT_TO_FLOAT => "int->float"
    M_INT_TO_I16 => "int->i16"
    M_INT_TO_I32 => "int->i32"
    M_INT_TO_I8 => "int->i8"
    M_INT_TO_INT => "int->int"
    M_INT_TO_RATIO => "int->ratio"
    M_INT_TO_U16 => "int->u16"
    M_INT_TO_U32 => "int->u32"
    M_INT_TO_U8 => "int->u8"
    M_INTEGER_LENGTH => "integer-length"
    M_JOIN => "join"
    M_KEYS => "keys"
    M_LEN => "len"
    M_LENGTH => "length"
    M_LOAD_ARG => "load-arg"
    M_LOAD_ENV => "load-env"
    M_LOAD_RAW => "load-raw"
    M_LOAD_TYPE => "load-type"
    M_LOG => "log"
    M_LOGAND => "logand"
    M_LOGBITP => "logbitp"
    M_LOGCOUNT => "logcount"
    M_LOGIOR => "logior"
    M_LOGNOT => "lognot"
    M_LOGTEST => "logtest"
    M_LOGXOR => "logxor"
    M_LT => "lt"
    M_MAX => "max"
    M_MIN => "min"
    M_MOD => "mod"
    M_NEW => "new"
    M_NUMERATOR => "numerator"
    M_POP => "pop"
    M_POP_FRAME => "pop-frame"
    M_POSITION_AT_END => "position-at-end"
    M_PUSH => "push"
    M_PUSH_FRAME => "push-frame"
    M_RATIO_TO_FLOAT => "ratio->float"
    M_RATIO_TO_INT => "ratio->int"
    M_RATIO_TO_RATIONAL => "ratio->rational"
    M_RATIONAL_TO_FLOAT => "rational->float"
    M_RATIONAL_TO_INT => "rational->int"
    M_RATIONAL_TO_RATIO => "rational->ratio"
    M_REF => "ref"
    M_ROUND => "round"
    M_SCRATCH_SLOT => "scratch-slot"
    M_SIN => "sin"
    M_SINH => "sinh"
    M_SQRT => "sqrt"
    M_STORE_ARG => "store-arg"
    M_SUBSTRING => "substring"
    M_TAN => "tan"
    M_TANH => "tanh"
    M_TO_STRING => "to-string"
    M_TRUNCATE => "truncate"
    M_TRY_FLOAT_TO_F32 => "try-float->f32"
    M_TRY_FLOAT_TO_F64 => "try-float->f64"
    M_TRY_INT_TO_C_LONG => "try-int->c-long"
    M_TRY_INT_TO_C_ULONG => "try-int->c-ulong"
    M_TRY_INT_TO_CHAR => "try-int->char"
    M_TRY_INT_TO_I16 => "try-int->i16"
    M_TRY_INT_TO_I32 => "try-int->i32"
    M_TRY_INT_TO_I8 => "try-int->i8"
    M_TRY_INT_TO_U16 => "try-int->u16"
    M_TRY_INT_TO_U32 => "try-int->u32"
    M_TRY_INT_TO_U8 => "try-int->u8"
    M_TRY_RATIONAL_TO_INT => "try-rational->int"
    M_UPCASE => "upcase"
    M_VALUES => "values"
    M_VERIFY => "verify"
    M_WAIT => "wait"
    "Appended for the explicit-symbols plan's stage 7b (2026-10-28): the names
     of the built-in free functions, each owned by the module that defines it
     ([`HOMED_VOCABULARY`]), so a call's path names the function by the
     vocabulary symbol and its dispatch compares that. `intern`, an older
     member, is the one spelled like one, and `equal`/`equalp` are the
     methods' names above, taken into `std::cmp`."
    F_COMPILE_FILE => "compile-file"
    F_DUMP => "dump"
    F_DRIBBLE_START => "dribble-start"
    F_DRIBBLE_STOP => "dribble-stop"
    F_ED_OPEN => "ed-open"
    F_FILE_CREATE_DIRECTORIES => "file-create-directories"
    F_FILE_DELETE => "file-delete"
    F_FILE_DIRECTORY_P => "file-directory-p"
    F_FILE_EXISTS_P => "file-exists-p"
    F_FILE_LIST_DIRECTORY => "file-list-directory"
    F_FILE_MODIFIED_DATE => "file-modified-date"
    F_FILE_OWNER_NAME => "file-owner-name"
    F_FILE_RENAME => "file-rename"
    F_FILE_TRUENAME => "file-truename"
    F_MAKE_RANDOM_STATE_FRESH => "make-random-state-fresh"
    F_NET_ACCEPT => "net-accept"
    F_NET_BUFFERED_P => "net-buffered-p"
    F_NET_CONNECT_BEGIN => "net-connect-begin"
    F_NET_CONNECT_FINISH => "net-connect-finish"
    F_NET_FILL => "net-fill"
    F_NET_FLUSH => "net-flush"
    F_NET_LISTEN => "net-listen"
    F_NET_LOCAL_ADDRESS => "net-local-address"
    F_NET_PEER_ADDRESS => "net-peer-address"
    F_NET_PEER_SUBJECT => "net-peer-subject"
    F_NET_POP_BYTE => "net-pop-byte"
    F_NET_POP_CHAR => "net-pop-char"
    F_NET_PUSH_BYTE => "net-push-byte"
    F_NET_PUSH_STRING => "net-push-string"
    F_NET_RESOLVE_BEGIN => "net-resolve-begin"
    F_NET_RESOLVE_FINISH => "net-resolve-finish"
    F_NET_SERVER_NAME => "net-server-name"
    F_NET_SET_KEEPALIVE => "net-set-keepalive"
    F_NET_SET_KEEPALIVE_PERIOD => "net-set-keepalive-period"
    F_NET_SET_NODELAY => "net-set-nodelay"
    F_NET_SHUTDOWN_WRITE => "net-shutdown-write"
    F_NET_SOCKET_ERROR => "net-socket-error"
    F_NET_TLS_ADD_CERTIFICATE => "net-tls-add-certificate"
    F_NET_TLS_HANDSHAKE => "net-tls-handshake"
    F_NET_TLS_LISTEN => "net-tls-listen"
    F_NET_TLS_START => "net-tls-start"
    F_NET_UDP_BIND => "net-udp-bind"
    F_NET_UDP_LAST_SENDER => "net-udp-last-sender"
    F_NET_UDP_RECV => "net-udp-recv"
    F_NET_UDP_SEND_TO => "net-udp-send-to"
    F_NET_UNIX_CONNECT => "net-unix-connect"
    F_NET_UNIX_LISTEN => "net-unix-listen"
    F_NET_WAIT => "net-wait"
    F_NET_WAIT_FOR => "net-wait-for"
    F_RANDOM_STATE_COPY => "random-state-copy"
    F_RANDOM_STATE_NEXT => "random-state-next"
    F_READ_DATUM_AT => "read-datum-at"
    F_STREAM_AT_LINE_START => "stream-at-line-start"
    F_STREAM_CLOSE => "stream-close"
    F_STREAM_DESCRIBE => "stream-describe"
    F_STREAM_FINISH_OUTPUT => "stream-finish-output"
    F_STREAM_INPUT_P => "stream-input-p"
    F_STREAM_LISTEN => "stream-listen"
    F_STREAM_OPEN_FILE => "stream-open-file"
    F_STREAM_OPEN_P => "stream-open-p"
    F_STREAM_OUTPUT_P => "stream-output-p"
    F_STREAM_POSITION => "stream-position"
    F_STREAM_READ_BYTE => "stream-read-byte"
    F_STREAM_READ_CHAR => "stream-read-char"
    F_STREAM_STDERR => "stream-stderr"
    F_STREAM_STDIN => "stream-stdin"
    F_STREAM_STDOUT => "stream-stdout"
    F_STREAM_STRING_INPUT => "stream-string-input"
    F_STREAM_STRING_OUTPUT => "stream-string-output"
    F_STREAM_TAKE_OUTPUT_STRING => "stream-take-output-string"
    F_STREAM_UNREAD_CHAR => "stream-unread-char"
    F_STREAM_WRITE_BYTE => "stream-write-byte"
    F_STREAM_WRITE_STRING => "stream-write-string"
    F_COMMAND_LINE_ARGS => "command-line-args"
    F_GETENV => "getenv"
    F_HOME_DIRECTORY => "home-directory"
    F_LISP_IMPLEMENTATION_VERSION => "lisp-implementation-version"
    F_MACHINE_INSTANCE => "machine-instance"
    F_MACHINE_TYPE => "machine-type"
    F_MACHINE_VERSION => "machine-version"
    F_SOFTWARE_TYPE => "software-type"
    F_SOFTWARE_VERSION => "software-version"
    F_EVAL => "eval"
    F_MACROEXPAND => "macroexpand"
    F_MACROEXPAND_1 => "macroexpand-1"
    F_PPRINT_INDENT => "pprint-indent"
    F_PPRINT_LIST_EXHAUSTED => "pprint-list-exhausted"
    F_PPRINT_NEWLINE => "pprint-newline"
    F_PPRINT_POP => "pprint-pop"
    F_PPRINT_TAB => "pprint-tab"
    F_PARSE_FLOAT => "parse-float"
    F_EXIT => "exit"
    F_SEED_RANDOM_STATE => "seed-random-state"
    F_GET_DISPATCH_MACRO_CHARACTER => "get-dispatch-macro-character"
    F_GET_MACRO_CHARACTER => "get-macro-character"
    F_READ => "read"
    F_SET_DISPATCH_MACRO_CHARACTER => "set-dispatch-macro-character"
    F_SET_MACRO_CHARACTER => "set-macro-character"
    F_HEAP_INFO => "heap-info"
    F_SEXPR_ATOM => "sexpr-atom"
    F_SEXPR_BOOL => "sexpr-bool"
    F_SEXPR_CAR => "sexpr-car"
    F_SEXPR_CDR => "sexpr-cdr"
    F_SEXPR_CHAR => "sexpr-char"
    F_SEXPR_CONS => "sexpr-cons"
    F_SEXPR_CONSP => "sexpr-consp"
    F_SEXPR_F32 => "sexpr-f32"
    F_SEXPR_F64 => "sexpr-f64"
    F_SEXPR_I16 => "sexpr-i16"
    F_SEXPR_I32 => "sexpr-i32"
    F_SEXPR_I8 => "sexpr-i8"
    F_SEXPR_INT => "sexpr-int"
    F_SEXPR_NULL => "sexpr-null"
    F_SEXPR_STR => "sexpr-str"
    F_SEXPR_SYM_NAME => "sexpr-sym-name"
    F_SEXPR_SYMP => "sexpr-symp"
    F_SEXPR_U16 => "sexpr-u16"
    F_SEXPR_U32 => "sexpr-u32"
    F_SEXPR_U8 => "sexpr-u8"
    F_KEYWORDP => "keywordp"
    F_SYMBOL_TO_STRING => "symbol->string"
    F_SLEEP => "sleep"
    F_YIELD => "yield"
    F_GET_INTERNAL_REAL_TIME => "get-internal-real-time"
    F_GET_INTERNAL_RUN_TIME => "get-internal-run-time"
    F_GET_UNIVERSAL_TIME => "get-universal-time"
    F_TIMEZONE_DAYLIGHT_P => "timezone-daylight-p"
    F_TIMEZONE_OFFSET_SECONDS => "timezone-offset-seconds"
    F_FORMAT_RT => "format-rt"
    F_PRINT_RT => "print-rt"
    F_PRINTLN_RT => "println-rt"
    F_PPRINT_RT => "pprint-rt"
    F_PPRINT_BLOCK_START_RT => "pprint-block-start-rt"
    F_PPRINT_BLOCK_END_RT => "pprint-block-end-rt"
    F_C_ARENA_OPEN => " c-arena-open"
    F_C_ARENA_CLOSE => " c-arena-close"
    F_C_ALLOC => " c-alloc"
    F_C_PTR_CHECK => " c-ptr-check"
    F_C_INDEX => " c-index"
    F_C_OFFSET => " c-offset"
    F_C_LOAD => " c-load"
    F_C_STORE => " c-store"
    F_FFI_CALLBACK_ADDRESS => " ffi-callback-address"
}


// ---- the symbol itself ---------------------------------------------------

/// An interned symbol's header — what a [`SymRef`] points at.
///
/// `align(8)` is not decoration: the tagged compiled-code representation packs
/// a 3-bit tag into the low bits of this address, so the header must be
/// 8-byte aligned or the tag would collide with the pointer. `Cell` relies on
/// the same property for `Cons`.
///
/// Every field is immutable once written, and the header is never freed.
#[repr(C, align(8))]
pub struct Symbol {
    /// The spelling: the name, with a colon in front
    /// for a keyword. One allocation serves [`SymRef::name`] and
    /// [`SymRef::spelling`], which differ only by that colon.
    spelling: Box<str>,
    /// The module that owns this symbol — the one it was created in. A module
    /// that merely [`import`]s it is not its home.
    home: NsId,
    /// Index into [`BUILTIN_SYMBOLS`], or [`NOT_WELL_KNOWN`].
    well_known: u32,
}

/// An interned symbol: the address of its [`Symbol`] header, compared by
/// identity. The pointer counterpart of [`super::value::ConsRef`].
#[derive(Clone, Copy)]
pub struct SymRef(*const Symbol);

// SAFETY: a `Symbol` is written once, never mutated, and never freed, so the
// address is valid for the life of the process and reading through it from any
// thread races with nothing. This is what lets one global table serve every
// thread (`cargo test` runs many at once) — unlike a `Heap`, which is mutable
// and therefore stays thread-local.
unsafe impl Send for SymRef {}
unsafe impl Sync for SymRef {}

impl PartialEq for SymRef {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for SymRef {}

impl std::hash::Hash for SymRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::fmt::Debug for SymRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SymRef({})", self.name())
    }
}

/// How the symbol is written ([`SymRef::spelling`]) — what a message naming
/// it shows.
impl std::fmt::Display for SymRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.spelling())
    }
}

impl SymRef {
    fn header(&self) -> &'static Symbol {
        // SAFETY: the only way to obtain a `SymRef` is from the table, which
        // leaks every header it builds — so this address is always a live,
        // immutable `Symbol` (see the module doc comment's property 2).
        unsafe { &*self.0 }
    }

    /// The symbol's name — CL's `symbol-name`, so a
    /// keyword's has no colon: `:foo`'s name is `foo`.
    ///
    /// `'static` for real, not asserted: headers are leaked. Reading a name is
    /// one dereference — there is no reverse table, which is the whole point
    /// of the identity being an address.
    pub fn name(&self) -> &'static str {
        let spelling: &'static str = &self.header().spelling;
        if self.is_keyword() {
            &spelling[1..]
        } else {
            spelling
        }
    }

    /// How the symbol is written: its name, with a keyword's colon in front.
    /// What the compiler keys a symbol by when it needs a string (an
    /// environment, a diagnostic), so a keyword never stands where the plain
    /// symbol of the same name does. Not an identity: two symbols are the
    /// same when their addresses are.
    pub fn spelling(&self) -> &'static str {
        &self.header().spelling
    }

    /// The module that owns this symbol. Not "a module it is visible from":
    /// an imported symbol keeps the home it was created with.
    pub fn home(&self) -> NsId {
        self.header().home
    }

    /// Whether this is a keyword — owned by [`NsId::KEYWORD`]. The only test
    /// there is: a keyword's name is an ordinary name, colon-less.
    pub fn is_keyword(&self) -> bool {
        self.home() == NsId::KEYWORD
    }

    /// Which member of the fixed vocabulary this is, or [`NOT_WELL_KNOWN`].
    ///
    /// For dispatching over that vocabulary with a `match` — see [`wk`] for
    /// why this exists and why it is never an identity test.
    pub fn well_known(&self) -> u32 {
        self.header().well_known
    }

    /// Whether this is a member of the vocabulary every module holds alike —
    /// a reserved word, a keyword, or a name of `std`'s or the core IR's —
    /// so that where it was made says nothing about which definition it
    /// names. A member [`HOMED_VOCABULARY`] places in a module is that
    /// module's, like any symbol made there, even when the module is `std`
    /// (`dump`).
    pub fn is_shared_vocabulary(&self) -> bool {
        self.well_known() != NOT_WELL_KNOWN && self.well_known() < FIRST_HOMED
    }

    /// Whether this is the vocabulary member `konst` names.
    pub fn is(&self, konst: u32) -> bool {
        debug_assert!(konst != NOT_WELL_KNOWN, "`is` takes a `wk` constant, not NOT_WELL_KNOWN");
        self.header().well_known == konst
    }

    /// The raw address, as an opaque integer — for embedding in
    /// `typelisp-abi`'s tagged compiled-code representation, exactly as
    /// [`super::value::ConsRef::addr`] is. Never meaningful to do arithmetic
    /// on; only round-tripped through [`SymRef::from_addr`].
    pub fn addr(&self) -> usize {
        self.0 as usize
    }

    /// # Safety
    ///
    /// `addr` must have come from [`SymRef::addr`]. Since symbols are never
    /// freed, any address that ever was one still is.
    pub unsafe fn from_addr(addr: usize) -> SymRef {
        SymRef(addr as *const Symbol)
    }
}

// ---- the module tree -----------------------------------------------------

/// A module's table, as an index into the one `Vec` behind the lock.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NsId(u32);

impl NsId {
    /// The system module: the fixed vocabulary ([`BUILTIN_SYMBOLS`]) and
    /// nothing else. Every other module imports these by pointer when it is
    /// created — see the module doc comment.
    pub const SYSTEM: NsId = NsId(0);

    /// The top-level module: the prelude, the core macro layer and the island
    /// are read into it, as is a source file that declares no module of its
    /// own. Ordinary in every respect but being created first.
    pub const ROOT: NsId = NsId(1);

    /// Where every keyword lives — see the module doc comment. It is the
    /// root's child `keyword`, so its path is `["keyword"]` and re-interning a
    /// keyword by path lands back here.
    pub const KEYWORD: NsId = NsId(2);

    /// The standard library's module, the root's child `std`. What it
    /// exports is visible in every module without being imported: reading a
    /// name a module does not hold yields `std`'s symbol of that name when
    /// `std` exports one ([`intern_in`]).
    pub const STD: NsId = NsId(3);

    /// The core IR's module, the root's child `%core`: the IR's tags and the
    /// names the compiler island is written with. No other module imports
    /// these, so a user's `var` or `call` is the user's own symbol.
    pub const CORE: NsId = NsId(4);
}

/// A module of the standard library named from Rust, as its segments under
/// `std`: `StdModule(&[])` is `std` itself and `StdModule(&["time"])` is
/// `std::time`. What the runtime's constant type tables write a module as,
/// since only `std`'s own `NsId` is fixed.
#[derive(Clone, Copy, Debug)]
pub struct StdModule(pub &'static [&'static str]);

impl StdModule {
    pub const STD: StdModule = StdModule(&[]);
    pub const TIME: StdModule = StdModule(&["time"]);
    pub const REPL: StdModule = StdModule(&["repl"]);
    pub const THREAD: StdModule = StdModule(&["thread"]);
    pub const IO: StdModule = StdModule(&["io"]);
    pub const ARRAY: StdModule = StdModule(&["array"]);
    pub const OPTION: StdModule = StdModule(&["option"]);
    pub const RESULT: StdModule = StdModule(&["result"]);
    pub const VEC: StdModule = StdModule(&["vec"]);
    pub const SEXPR: StdModule = StdModule(&["sexpr"]);
    pub const COLLECTIONS: StdModule = StdModule(&["collections"]);
    pub const SYNC: StdModule = StdModule(&["sync"]);
    pub const READER: StdModule = StdModule(&["reader"]);
    pub const EVAL: StdModule = StdModule(&["eval"]);
    pub const NUM: StdModule = StdModule(&["num"]);
    pub const FS: StdModule = StdModule(&["fs"]);
    pub const NET: StdModule = StdModule(&["net"]);
    pub const PRIMITIVE: StdModule = StdModule(&["primitive"]);

    /// The module, found one segment at a time from `std`.
    pub fn ns(self) -> NsId {
        self.0.iter().fold(NsId::STD, |ns, segment| child_ns(ns, segment))
    }

    /// The module that defines the built-in type named by the vocabulary
    /// symbol `ty` (`wk::OPTION` ...). Panics for a symbol that names no
    /// built-in type.
    pub fn of_builtin_type(ty: u32) -> StdModule {
        match ty {
            wk::OPTION => StdModule::OPTION,
            wk::RESULT => StdModule::RESULT,
            wk::VECTOR => StdModule::VEC,
            wk::SEXPR => StdModule::SEXPR,
            wk::HASHTABLE | wk::SCOPE => StdModule::COLLECTIONS,
            wk::ARRAY => StdModule::ARRAY,
            wk::TASK_TYPE | wk::THREAD_TYPE => StdModule::THREAD,
            wk::CHAN => StdModule::SYNC,
            wk::READ_ERROR => StdModule::READER,
            wk::EVAL_ERROR => StdModule::EVAL,
            wk::PARSE_INT_ERROR | wk::PARSE_FLOAT_ERROR => StdModule::NUM,
            wk::FILE_ERROR => StdModule::FS,
            wk::NET_ERROR => StdModule::NET,
            wk::I8
            | wk::I16
            | wk::I32
            | wk::U8
            | wk::U16
            | wk::U32
            | wk::F32
            | wk::F64
            | wk::INT
            | wk::RATIO
            | wk::REPR_RATIONAL
            | wk::REPR_RANDOM_STATE
            | wk::BOOL
            | wk::CHAR
            | wk::NEVER
            | wk::PTR
            | wk::C_LONG
            | wk::C_ULONG
            | wk::STRING
            | wk::SYMBOL => StdModule::PRIMITIVE,
            _ => panic!("`{}` names no built-in type", well_known_symbol(ty).name()),
        }
    }
}

/// The vocabulary members owned by [`NsId::STD`] rather than the system
/// module: names the standard library defines, which a module may define for
/// itself only after declaring so (`shadow`). Exported by `std`.
///
/// The second group are IR tags spelled as a name the standard library
/// offers — a type, a function — so the IR's tag and the name the island
/// writes for that definition are one symbol.
pub const STD_VOCABULARY: &[u32] = &[
    wk::COND, wk::AND, wk::OR, wk::WHEN, wk::UNLESS, wk::WHILE, wk::UNTIL, wk::CASE,
    wk::DOLIST, wk::DOTIMES, wk::DOITER, wk::DO, wk::NOT, wk::INTERN,
    wk::INT, wk::BIGNUM, wk::RATIO, wk::CHAR, wk::BOOL, wk::STR, wk::ASSOC, wk::SYM, wk::NIL,
    wk::CONS, wk::PATH, wk::I32, wk::F64, wk::F32, wk::I8, wk::I16, wk::U8, wk::U16, wk::U32,
    wk::REPR_RATIONAL, wk::REPR_RANDOM_STATE, wk::REPR_SEXPR, wk::REPR_SCOPE, wk::REPR_VECTOR,
    wk::REPR_HASHTABLE, wk::REPR_NONE,
    wk::OPTION, wk::RESULT, wk::SEXPR, wk::VECTOR, wk::HASHTABLE, wk::SCOPE, wk::CHAN, wk::TASK_TYPE,
    wk::THREAD_TYPE, wk::ARRAY, wk::STRING, wk::SYMBOL, wk::PARSE_INT_ERROR, wk::PARSE_FLOAT_ERROR,
    wk::READ_ERROR, wk::EVAL_ERROR, wk::FILE_ERROR, wk::NET_ERROR, wk::LLVM_MODULE, wk::LLVM_FUNCTION,
    wk::LLVM_BUILDER, wk::LLVM_BASIC_BLOCK, wk::LLVM_VALUE, wk::C_LONG, wk::C_ULONG,
];

/// The vocabulary members owned by [`NsId::CORE`]: core-IR tags with no
/// meaning in source, the value representations the IR names, the names
/// retired from the vocabulary, and the reader's attribute head. None is
/// spelled as a name the standard library offers (those are in
/// [`STD_VOCABULARY`]): in `%core` such a spelling would read as the tag, not
/// as the definition.
pub const CORE_VOCABULARY: &[u32] = &[
    wk::RETIRED_WHERE, wk::RETIRED_EQUALS, wk::RETIRED_FLOAT, wk::UNIT, wk::VAR, wk::SET, wk::GLOBAL,
    wk::SET_GLOBAL, wk::CALL, wk::CONSTRUCT, wk::FIELD_GET, wk::FIELD_SET, wk::DYN_NEW, wk::DYN_UPCAST,
    wk::DYN_CALL, wk::DYN_VALUE, wk::FNREF, wk::METHODREF, wk::COMPILE_FN, wk::METHOD, wk::PAT_WILD,
    wk::PAT_EMPTY, wk::RETIRED_PAT_NONEMPTY, wk::PAT_BIND, wk::PAT_LIT, wk::PAT_GUARD, wk::PAT_CTOR,
    wk::PAT_TYPETEST, wk::EXPR, wk::INT_ANY_WIDTH, wk::FLOAT_ANY_WIDTH, wk::DISASSEMBLE_FN,
    wk::RETIRED_GO, wk::UNTAG_INT, wk::TAG_INT, wk::SOME_OF, wk::PAT_SOME, wk::BOX_OPTION,
    wk::RETIRED_ELSE, wk::SPAWN, wk::TAG, wk::SPAWN_THREAD, wk::PAT_WHEN, wk::INTERNAL_MODULE_NAME,
    wk::ATTRIBUTE, wk::APPLY_INDIRECT, wk::CELLVAR, wk::CELLSET, wk::GLOBAL_INIT, wk::HASHTABLE_OP,
    wk::LLVM_OP, wk::SUSPEND, wk::VECTOR_OP, wk::REPR_HANDLE, wk::REPR_RAW_WORD, wk::REPR_STRUCT,
    wk::REPR_ENUM, wk::REPR_DYN, wk::REPR_NICHE, wk::TYPE_NAMED, wk::TYPE_FN,
    wk::TYPE_DYN, wk::TYPE_PTR, wk::TYPE_CSTRUCT, wk::TYPE_PARAM, wk::TYPE_VAR, wk::TYPE_UNIT, wk::TYPE_NEVER,
    wk::TYPE_REF, wk::TYPE_VALUE,
];

/// The module the names of every type's members are made in, the root's
/// child `%member` ([`member_symbol`]): a member is no module's definition,
/// so its name is read in no module a program is written in.
pub const MEMBER_MODULE: &str = "%member";

/// The first member of the vocabulary that [`HOMED_VOCABULARY`] places:
/// every member from here on is made in its module.
const FIRST_HOMED: u32 = wk::M_OP_MUL;

/// The names of the built-in methods, owned by [`MEMBER_MODULE`].
pub const MEMBER_VOCABULARY: &[u32] = &[
    wk::M_OP_MUL,
    wk::M_OP_ADD,
    wk::M_OP_SUB,
    wk::M_OP_DIV,
    wk::M_OP_NE,
    wk::M_OP_LT,
    wk::M_OP_LE,
    wk::RETIRED_EQUALS,
    wk::M_OP_GT,
    wk::M_OP_GE,
    wk::M_ACOS,
    wk::M_ACOSH,
    wk::M_ADD_COROUTINE_FUNCTION,
    wk::M_ADD_FUNCTION,
    wk::M_ADD_FUNCTION_WITH_ENV,
    wk::M_ALLOCA_ARGS,
    wk::M_ALPHAP,
    wk::M_APPEND,
    wk::M_APPEND_BLOCK,
    wk::M_ASH,
    wk::M_ASIN,
    wk::M_ASINH,
    wk::M_ATAN,
    wk::M_ATANH,
    wk::M_AVAILABLE_PARALLELISM,
    wk::M_BLOCK_TERMINATED_P,
    wk::M_BUCKET_COUNT,
    wk::M_BUCKET_DELETE,
    wk::M_BUCKET_KEY,
    wk::M_BUCKET_PUT,
    wk::M_BUCKET_VALUE,
    wk::M_BUILD_ADD,
    wk::M_BUILD_AND,
    wk::M_BUILD_ASHR,
    wk::M_BUILD_BR,
    wk::M_BUILD_CALL,
    wk::M_BUILD_CALL_WITH_ENV,
    wk::M_BUILD_COND_BR,
    wk::M_BUILD_FADD,
    wk::M_BUILD_FCEIL,
    wk::M_BUILD_FCMP_EQ,
    wk::M_BUILD_FCMP_GE,
    wk::M_BUILD_FCMP_GT,
    wk::M_BUILD_FCMP_LE,
    wk::M_BUILD_FCMP_LT,
    wk::M_BUILD_FCMP_NE,
    wk::M_BUILD_FCOS,
    wk::M_BUILD_FDIV,
    wk::M_BUILD_FEXP,
    wk::M_BUILD_FFLOOR,
    wk::M_BUILD_FLOG,
    wk::M_BUILD_FMAXNUM,
    wk::M_BUILD_FMINNUM,
    wk::M_BUILD_FMUL,
    wk::M_BUILD_FN_ADDRESS,
    wk::M_BUILD_FPOW,
    wk::M_BUILD_FPTOSI,
    wk::M_BUILD_FREE,
    wk::M_BUILD_FREM,
    wk::M_BUILD_FROUND,
    wk::M_BUILD_FROUND32,
    wk::M_BUILD_FSIN,
    wk::M_BUILD_FSQRT,
    wk::M_BUILD_FSUB,
    wk::M_BUILD_FTRUNC,
    wk::M_BUILD_ICMP_EQ,
    wk::M_BUILD_ICMP_GE,
    wk::M_BUILD_ICMP_GT,
    wk::M_BUILD_ICMP_LE,
    wk::M_BUILD_ICMP_LT,
    wk::M_BUILD_ICMP_NE,
    wk::M_BUILD_INT_TO_PTR,
    wk::M_BUILD_LSHR,
    wk::M_BUILD_MAKE_CLOSURE,
    wk::M_BUILD_MALLOC,
    wk::M_BUILD_MUL,
    wk::M_BUILD_OR,
    wk::M_BUILD_PTR_TO_INT,
    wk::M_BUILD_RET,
    wk::M_BUILD_SADD_OVERFLOW,
    wk::M_BUILD_SELECT,
    wk::M_BUILD_SHL,
    wk::M_BUILD_SITOFP,
    wk::M_BUILD_SLOT_PTR,
    wk::M_BUILD_SMUL_OVERFLOW,
    wk::M_BUILD_SSUB_OVERFLOW,
    wk::M_BUILD_SUB,
    wk::M_BUILD_XOR,
    wk::M_CAP,
    wk::M_CEILING,
    wk::M_CHAR_TO_INT,
    wk::M_CHAR_TO_STRING,
    wk::M_CLEAR,
    wk::M_CLONE_FRAMES,
    wk::M_CLOSE,
    wk::M_CONST_WORD,
    wk::M_COROUTINE_APPLY,
    wk::M_COROUTINE_BEGIN,
    wk::M_COROUTINE_CALL,
    wk::M_COROUTINE_CALL_ENV,
    wk::M_COROUTINE_DYN_CALL,
    wk::M_COROUTINE_END,
    wk::M_COROUTINE_SUSPEND,
    wk::M_COROUTINE_UNWIND,
    wk::M_COS,
    wk::M_COSH,
    wk::M_COUNT,
    wk::M_CREATE,
    wk::M_CURRENT_ID,
    wk::M_DENOMINATOR,
    wk::M_DIGITP,
    wk::M_DOWNCASE,
    wk::M_ENTRIES,
    wk::M_EQ,
    wk::M_EQL,
    wk::M_EQUAL,
    wk::M_EQUALP,
    wk::M_EXP,
    wk::M_EXPT,
    wk::M_FLOAT_TO_F32,
    wk::M_FLOAT_TO_F64,
    wk::M_FLOAT_TO_INT,
    wk::M_FLOAT_TO_RATIO,
    wk::M_FLOOR,
    wk::M_FRAME_CLEAR_HANDLER,
    wk::M_FRAME_SET_HANDLER,
    wk::M_FRAME_SLOT,
    wk::M_FRAME_SLOT_ROOTED,
    wk::M_FUNCTION_PARAM,
    wk::M_GET,
    wk::M_GET_FUNCTION,
    wk::M_INT_TO_C_LONG,
    wk::M_INT_TO_C_ULONG,
    wk::M_INT_TO_CHAR,
    wk::M_INT_TO_FLOAT,
    wk::M_INT_TO_I16,
    wk::M_INT_TO_I32,
    wk::M_INT_TO_I8,
    wk::M_INT_TO_INT,
    wk::M_INT_TO_RATIO,
    wk::M_INT_TO_U16,
    wk::M_INT_TO_U32,
    wk::M_INT_TO_U8,
    wk::M_INTEGER_LENGTH,
    wk::M_JOIN,
    wk::M_KEYS,
    wk::M_LEN,
    wk::M_LENGTH,
    wk::M_LOAD_ARG,
    wk::M_LOAD_ENV,
    wk::M_LOAD_RAW,
    wk::M_LOAD_TYPE,
    wk::M_LOG,
    wk::M_LOGAND,
    wk::M_LOGBITP,
    wk::M_LOGCOUNT,
    wk::M_LOGIOR,
    wk::M_LOGNOT,
    wk::M_LOGTEST,
    wk::M_LOGXOR,
    wk::M_LT,
    wk::M_MAX,
    wk::M_MIN,
    wk::M_MOD,
    wk::M_NEW,
    wk::M_NUMERATOR,
    wk::M_POP,
    wk::M_POP_FRAME,
    wk::M_POSITION_AT_END,
    wk::M_PUSH,
    wk::M_PUSH_FRAME,
    wk::M_RATIO_TO_FLOAT,
    wk::M_RATIO_TO_INT,
    wk::M_RATIO_TO_RATIONAL,
    wk::M_RATIONAL_TO_FLOAT,
    wk::M_RATIONAL_TO_INT,
    wk::M_RATIONAL_TO_RATIO,
    wk::RECV,
    wk::M_REF,
    wk::M_ROUND,
    wk::M_SCRATCH_SLOT,
    wk::SEND,
    wk::SET,
    wk::M_SIN,
    wk::M_SINH,
    wk::M_SQRT,
    wk::M_STORE_ARG,
    wk::M_SUBSTRING,
    wk::M_TAN,
    wk::M_TANH,
    wk::M_TO_STRING,
    wk::M_TRUNCATE,
    wk::M_TRY_FLOAT_TO_F32,
    wk::M_TRY_FLOAT_TO_F64,
    wk::M_TRY_INT_TO_C_LONG,
    wk::M_TRY_INT_TO_C_ULONG,
    wk::M_TRY_INT_TO_CHAR,
    wk::M_TRY_INT_TO_I16,
    wk::M_TRY_INT_TO_I32,
    wk::M_TRY_INT_TO_I8,
    wk::M_TRY_INT_TO_U16,
    wk::M_TRY_INT_TO_U32,
    wk::M_TRY_INT_TO_U8,
    wk::M_TRY_RATIONAL_TO_INT,
    wk::M_UPCASE,
    wk::M_VALUES,
    wk::M_VERIFY,
    wk::M_WAIT,
];

/// The vocabulary members owned by a module the table makes along with them,
/// by the module's path from the root: each built-in free function's name in
/// the module that defines it, and the built-in methods' names in
/// [`MEMBER_MODULE`]. A member is made in the first module that lists it —
/// a function's ahead of the members', since a name is looked for in the
/// module it was made in — and taken by pointer into any later one, as is a
/// member an older group already owns. None of these is exported: a name the standard library offers
/// is offered by `std::prelude`.
pub const HOMED_VOCABULARY: &[(&[&str], &[u32])] = &[
    (&["std"], &[wk::F_COMPILE_FILE, wk::F_DUMP]),
    (&["std", "%internal"], &[wk::F_DRIBBLE_START, wk::F_DRIBBLE_STOP, wk::F_ED_OPEN, wk::F_FILE_CREATE_DIRECTORIES, wk::F_FILE_DELETE, wk::F_FILE_DIRECTORY_P, wk::F_FILE_EXISTS_P, wk::F_FILE_LIST_DIRECTORY, wk::F_FILE_MODIFIED_DATE, wk::F_FILE_OWNER_NAME, wk::F_FILE_RENAME, wk::F_FILE_TRUENAME, wk::F_MAKE_RANDOM_STATE_FRESH, wk::F_NET_ACCEPT, wk::F_NET_BUFFERED_P, wk::F_NET_CONNECT_BEGIN, wk::F_NET_CONNECT_FINISH, wk::F_NET_FILL, wk::F_NET_FLUSH, wk::F_NET_LISTEN, wk::F_NET_LOCAL_ADDRESS, wk::F_NET_PEER_ADDRESS, wk::F_NET_PEER_SUBJECT, wk::F_NET_POP_BYTE, wk::F_NET_POP_CHAR, wk::F_NET_PUSH_BYTE, wk::F_NET_PUSH_STRING, wk::F_NET_RESOLVE_BEGIN, wk::F_NET_RESOLVE_FINISH, wk::F_NET_SERVER_NAME, wk::F_NET_SET_KEEPALIVE, wk::F_NET_SET_KEEPALIVE_PERIOD, wk::F_NET_SET_NODELAY, wk::F_NET_SHUTDOWN_WRITE, wk::F_NET_SOCKET_ERROR, wk::F_NET_TLS_ADD_CERTIFICATE, wk::F_NET_TLS_HANDSHAKE, wk::F_NET_TLS_LISTEN, wk::F_NET_TLS_START, wk::F_NET_UDP_BIND, wk::F_NET_UDP_LAST_SENDER, wk::F_NET_UDP_RECV, wk::F_NET_UDP_SEND_TO, wk::F_NET_UNIX_CONNECT, wk::F_NET_UNIX_LISTEN, wk::F_NET_WAIT, wk::F_NET_WAIT_FOR, wk::F_RANDOM_STATE_COPY, wk::F_RANDOM_STATE_NEXT, wk::F_READ_DATUM_AT, wk::F_STREAM_AT_LINE_START, wk::F_STREAM_CLOSE, wk::F_STREAM_DESCRIBE, wk::F_STREAM_FINISH_OUTPUT, wk::F_STREAM_INPUT_P, wk::F_STREAM_LISTEN, wk::F_STREAM_OPEN_FILE, wk::F_STREAM_OPEN_P, wk::F_STREAM_OUTPUT_P, wk::F_STREAM_POSITION, wk::F_STREAM_READ_BYTE, wk::F_STREAM_READ_CHAR, wk::F_STREAM_STDERR, wk::F_STREAM_STDIN, wk::F_STREAM_STDOUT, wk::F_STREAM_STRING_INPUT, wk::F_STREAM_STRING_OUTPUT, wk::F_STREAM_TAKE_OUTPUT_STRING, wk::F_STREAM_UNREAD_CHAR, wk::F_STREAM_WRITE_BYTE, wk::F_STREAM_WRITE_STRING, wk::F_FORMAT_RT, wk::F_PRINT_RT, wk::F_PRINTLN_RT, wk::F_PPRINT_RT, wk::F_PPRINT_BLOCK_START_RT, wk::F_PPRINT_BLOCK_END_RT, wk::F_C_ARENA_OPEN, wk::F_C_ARENA_CLOSE, wk::F_C_ALLOC, wk::F_C_PTR_CHECK, wk::F_C_INDEX, wk::F_C_OFFSET, wk::F_C_LOAD, wk::F_C_STORE, wk::F_FFI_CALLBACK_ADDRESS]),
    (&["std", "cmp"], &[wk::M_EQUAL, wk::M_EQUALP]),
    (&["std", "env"], &[wk::F_COMMAND_LINE_ARGS, wk::F_GETENV, wk::F_HOME_DIRECTORY, wk::F_LISP_IMPLEMENTATION_VERSION, wk::F_MACHINE_INSTANCE, wk::F_MACHINE_TYPE, wk::F_MACHINE_VERSION, wk::F_SOFTWARE_TYPE, wk::F_SOFTWARE_VERSION]),
    (&["std", "eval"], &[wk::F_EVAL, wk::F_MACROEXPAND, wk::F_MACROEXPAND_1]),
    (&["std", "fmt"], &[wk::F_PPRINT_INDENT, wk::F_PPRINT_LIST_EXHAUSTED, wk::F_PPRINT_NEWLINE, wk::F_PPRINT_POP, wk::F_PPRINT_TAB]),
    (&["std", "num"], &[wk::F_PARSE_FLOAT]),
    (&["std", "process"], &[wk::F_EXIT]),
    (&["std", "random"], &[wk::F_SEED_RANDOM_STATE]),
    (&["std", "reader"], &[wk::F_GET_DISPATCH_MACRO_CHARACTER, wk::F_GET_MACRO_CHARACTER, wk::F_READ, wk::F_SET_DISPATCH_MACRO_CHARACTER, wk::F_SET_MACRO_CHARACTER]),
    (&["std", "repl"], &[wk::F_HEAP_INFO]),
    (&["std", "sexpr"], &[wk::F_SEXPR_ATOM, wk::F_SEXPR_BOOL, wk::F_SEXPR_CAR, wk::F_SEXPR_CDR, wk::F_SEXPR_CHAR, wk::F_SEXPR_CONS, wk::F_SEXPR_CONSP, wk::F_SEXPR_F32, wk::F_SEXPR_F64, wk::F_SEXPR_I16, wk::F_SEXPR_I32, wk::F_SEXPR_I8, wk::F_SEXPR_INT, wk::F_SEXPR_NULL, wk::F_SEXPR_STR, wk::F_SEXPR_SYM_NAME, wk::F_SEXPR_SYMP, wk::F_SEXPR_U16, wk::F_SEXPR_U32, wk::F_SEXPR_U8]),
    (&["std", "symbol"], &[wk::INTERN, wk::F_KEYWORDP, wk::F_SYMBOL_TO_STRING]),
    (&["std", "thread"], &[wk::F_SLEEP, wk::F_YIELD]),
    (&["std", "time"], &[wk::F_GET_INTERNAL_REAL_TIME, wk::F_GET_INTERNAL_RUN_TIME, wk::F_GET_UNIVERSAL_TIME, wk::F_TIMEZONE_DAYLIGHT_P, wk::F_TIMEZONE_OFFSET_SECONDS]),
    (&[MEMBER_MODULE], MEMBER_VOCABULARY),
];

/// The standard library module's segment under the root.
pub const STD_MODULE: &str = "std";

/// The core IR module's segment under the root.
pub const CORE_MODULE: &str = "%core";

/// One module's table.
///
/// `syms` holds both the symbols this module created and the ones it imported
/// from elsewhere; the two are not distinguished here because a lookup does
/// not care. `SymRef::home` is what tells them apart.
struct NsNode {
    syms: HashMap<Box<str>, SymRef>,
    /// Child modules, by the symbol that names each: the one its name reads
    /// as in this module, so `foo::bar` is `foo`'s child under `foo`'s `bar`.
    children: HashMap<SymRef, NsId>,
    /// The module this one is written inside, for [`SymTable::path`]. Never
    /// consulted when resolving a name.
    parent: Option<NsId>,
    /// The symbol this module is named by in its parent; `None` for the
    /// system module and the root, which have no parent.
    name: Option<SymRef>,
}

/// What a module's symbol stands for after a `use` brought something in
/// under it — see [`SessionNames::import_name`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Import {
    Module(NsId),
    /// A type, by its own name's symbol.
    Type(SymRef),
    /// A function, global or macro, by its own name's symbol.
    Item(SymRef),
    /// A constructor or a static method of a type, which `(use Type)` brings
    /// in bare, by the type's name's symbol.
    Member(SymRef),
}

/// Where a path's segment led: into a module, or to a type, whose members
/// the next segment names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathStep {
    Module(NsId),
    Type(SymRef),
}

struct SymTable {
    nodes: Vec<NsNode>,
    /// What [`NsId::STD`] exports, by name: the symbols every module sees
    /// without importing them. Consulted by [`intern_in`] after the module's
    /// own table.
    std_exports: HashMap<Box<str>, SymRef>,
    /// How many headers have been created. Summing `syms.len()` would count
    /// an imported symbol once per module that can see it.
    created: usize,
}

impl SymTable {
    fn new() -> SymTable {
        let mut t = SymTable { nodes: Vec::new(), std_exports: HashMap::new(), created: 0 };
        // The fixed nodes, named once the root can read their names (below).
        let empty = |parent: Option<NsId>| NsNode { syms: HashMap::new(), children: HashMap::new(), parent, name: None };
        t.nodes.push(empty(None));
        t.nodes.push(empty(None));
        t.nodes.push(empty(Some(NsId::ROOT)));
        t.nodes.push(empty(Some(NsId::ROOT)));
        t.nodes.push(empty(Some(NsId::ROOT)));
        // The system module owns the reserved words. A keyword belongs to the
        // keyword module under its name without the colon, a standard
        // library name to `std` (which exports it), an IR name to `%core`.
        // A member of [`HOMED_VOCABULARY`] appended since is made below, in
        // its module, once the modules can be named.
        let mut vocabulary: Vec<Option<SymRef>> = BUILTIN_SYMBOLS
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let i = i as u32;
                if i >= FIRST_HOMED {
                    None
                } else if let Some(kw) = name.strip_prefix(':') {
                    Some(t.create(NsId::KEYWORD, kw, i))
                } else if STD_VOCABULARY.contains(&i) {
                    let sym = t.create(NsId::STD, name, i);
                    t.std_exports.insert((*name).into(), sym);
                    Some(sym)
                } else if CORE_VOCABULARY.contains(&i) {
                    Some(t.create(NsId::CORE, name, i))
                } else {
                    Some(t.create(NsId::SYSTEM, name, i))
                }
            })
            .collect();
        // Every module but the keyword one holds the reserved words.
        let reserved = t.nodes[NsId::SYSTEM.0 as usize].syms.clone();
        for ns in [NsId::ROOT, NsId::STD, NsId::CORE] {
            for (name, sym) in &reserved {
                t.nodes[ns.0 as usize].syms.insert(name.clone(), *sym);
            }
        }
        for (segment, id) in [(KEYWORD_MODULE, NsId::KEYWORD), (STD_MODULE, NsId::STD), (CORE_MODULE, NsId::CORE)] {
            let name = t.read(NsId::ROOT, segment);
            t.nodes[id.0 as usize].name = Some(name);
            t.nodes[NsId::ROOT.0 as usize].children.insert(name, id);
        }
        for (path, members) in HOMED_VOCABULARY {
            let ns = path.iter().fold(NsId::ROOT, |ns, segment| t.child(ns, segment));
            for &i in *members {
                let name = BUILTIN_SYMBOLS[i as usize];
                match vocabulary[i as usize] {
                    Some(sym) => {
                        t.nodes[ns.0 as usize].syms.insert(name.into(), sym);
                    }
                    None => vocabulary[i as usize] = Some(t.create(ns, name, i)),
                }
            }
        }
        let vocabulary: Vec<SymRef> = vocabulary
            .into_iter()
            .enumerate()
            .map(|(i, sym)| sym.unwrap_or_else(|| panic!("`{}` is in no group of the vocabulary", BUILTIN_SYMBOLS[i])))
            .collect();
        WELL_KNOWN
            .set(vocabulary.into_boxed_slice())
            .unwrap_or_else(|_| unreachable!("the table is built exactly once"));
        t
    }

    /// Appends a module node, giving it the reserved words. The fixed nodes
    /// (system, root, keyword, std, core) are built by [`SymTable::new`].
    fn push_node(&mut self, parent: NsId, name: SymRef) -> NsId {
        let id = NsId(self.nodes.len() as u32);
        // By pointer, not by name: this is property 4 in the module doc
        // comment, and the reason `defun` means the same thing in every
        // module.
        let syms = self.nodes[NsId::SYSTEM.0 as usize].syms.clone();
        self.nodes.push(NsNode { syms, children: HashMap::new(), parent: Some(parent), name: Some(name) });
        id
    }

    /// The symbol `name` names in `ns` — in that module's own table, and
    /// nowhere else. There is no walk up the tree.
    fn lookup(&self, ns: NsId, name: &str) -> Option<SymRef> {
        self.nodes[ns.0 as usize].syms.get(name).copied()
    }

    /// Build a symbol owned by `ns`. The caller has already looked.
    fn create(&mut self, ns: NsId, name: &str, well_known: u32) -> SymRef {
        // Leaked on purpose: symbols are permanent, so this is the allocation
        // that gives `SymRef::name` a genuine `'static` and makes the address
        // impossible to dangle.
        let spelling: Box<str> = if ns == NsId::KEYWORD { format!(":{}", name).into() } else { name.into() };
        let header: &'static Symbol = Box::leak(Box::new(Symbol { spelling, home: ns, well_known }));
        let sym = SymRef(header as *const Symbol);
        self.nodes[ns.0 as usize].syms.insert(name.into(), sym);
        self.created += 1;
        sym
    }

    /// What `name` reads as in `ns` — [`intern_in`]'s rule, with the lock
    /// already held.
    fn read(&mut self, ns: NsId, name: &str) -> SymRef {
        match self.find(ns, name) {
            Some(s) => s,
            None => self.create(ns, name, NOT_WELL_KNOWN),
        }
    }

    /// The symbol `name` would read as in `ns`, if it exists yet.
    fn find(&self, ns: NsId, name: &str) -> Option<SymRef> {
        self.lookup(ns, name).or_else(|| if ns == NsId::KEYWORD { None } else { self.std_exports.get(name).copied() })
    }

    /// `parent`'s child module named `segment`, if it exists.
    fn find_child(&self, parent: NsId, segment: &str) -> Option<NsId> {
        let name = self.find(parent, segment)?;
        self.nodes[parent.0 as usize].children.get(&name).copied()
    }

    /// `parent`'s child module named `segment`, created if new. The child is
    /// keyed by the symbol `segment` reads as in `parent`.
    fn child(&mut self, parent: NsId, segment: &str) -> NsId {
        let name = self.read(parent, segment);
        if let Some(id) = self.nodes[parent.0 as usize].children.get(&name) {
            return *id;
        }
        let id = self.push_node(parent, name);
        self.nodes[parent.0 as usize].children.insert(name, id);
        id
    }

    /// Where the segment `name`, read in the module `ns`, leads in the
    /// session `names`: a child module of `ns` the session has, or a type.
    fn step_down(&mut self, names: &SessionNames, ns: NsId, name: &str) -> Option<(SymRef, PathStep)> {
        let sym = self.read(ns, name);
        self.step_down_symbol(names, ns, sym).map(|step| (sym, step))
    }

    /// [`Self::step_down`] for the symbol the segment read as.
    fn step_down_symbol(&self, names: &SessionNames, ns: NsId, sym: SymRef) -> Option<PathStep> {
        if let Some(id) = self.nodes[ns.0 as usize].children.get(&sym) {
            if names.modules.contains(id) {
                return Some(PathStep::Module(*id));
            }
        }
        // A type is a step from the module that defines it. Its name alone
        // does not say which: a vocabulary symbol (`Vector`, `llvm-module`)
        // is the same symbol in every module.
        (names.type_home(sym) == Some(ns)).then_some(PathStep::Type(sym))
    }

    /// Where the name `name` written in `ns` leads, as a path's first step:
    /// a child module of `ns` or a type named by it, or else what a `use`
    /// brought in under it — a module's own definition wins over an import
    /// of the same name, as it does for every name.
    fn step_in(&mut self, names: &SessionNames, ns: NsId, name: &str) -> Option<(SymRef, PathStep)> {
        let sym = self.read(ns, name);
        self.step_in_symbol(names, ns, sym).map(|step| (sym, step))
    }

    /// [`Self::step_in`] for the symbol the segment read as.
    fn step_in_symbol(&self, names: &SessionNames, ns: NsId, sym: SymRef) -> Option<PathStep> {
        if let Some(own) = self.step_down_symbol(names, ns, sym) {
            return Some(own);
        }
        match names.imports.get(&(ns, sym)) {
            Some(Import::Module(id)) => Some(PathStep::Module(*id)),
            Some(Import::Type(t)) => Some(PathStep::Type(*t)),
            _ => None,
        }
    }

    /// Where the first segment `name` of a path read in `ns` leads: from
    /// `ns`, else a type the standard library offers every module under the
    /// name ([`SessionNames::offered_type`]), else from the root, else — while
    /// the standard library's names are still resolved by name — from `std`.
    fn first_step(&mut self, names: &SessionNames, ns: NsId, name: &str) -> Option<(SymRef, PathStep)> {
        let mut found = self.step_in(names, ns, name);
        if found.is_none() {
            let sym = self.read(ns, name);
            found = names.offered_type(sym).map(|step| (sym, step));
        }
        for outer in [NsId::ROOT, NsId::STD] {
            if found.is_none() && ns != outer {
                found = self.step_in(names, outer, name);
            }
        }
        found
    }

    /// Whether `ns` is `std` or a module inside it.
    fn within_std(&self, ns: NsId) -> bool {
        std::iter::successors(Some(ns), |m| self.nodes[m.0 as usize].parent).any(|m| m == NsId::STD)
    }

    fn path(&self, ns: NsId) -> Vec<String> {
        let mut segs = Vec::new();
        let mut cur = Some(ns);
        while let Some(id) = cur {
            let node = &self.nodes[id.0 as usize];
            if let Some(name) = node.name {
                segs.push(name.name().to_string());
            }
            cur = node.parent;
        }
        segs.reverse();
        segs
    }
}

static SYMBOLS: OnceLock<Mutex<SymTable>> = OnceLock::new();

/// The system module's symbols in [`BUILTIN_SYMBOLS`] order, for
/// [`well_known_symbol`]. Written once, while the table is being built, and
/// read without the lock afterwards.
static WELL_KNOWN: OnceLock<Box<[SymRef]>> = OnceLock::new();

fn table() -> MutexGuard<'static, SymTable> {
    SYMBOLS
        .get_or_init(|| Mutex::new(SymTable::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Intern `name` in module `ns`.
///
/// Names are case-sensitive: `Foo`, `FOO` and `foo` are three symbols.
///
/// The name means, in order: the symbol `ns`'s own table holds (one it
/// created, a reserved word every module holds, or one it declared its own
/// with `shadow`); else the symbol [`NsId::STD`] exports under that name;
/// else a **new symbol owned by `ns`**. Nothing else is looked at — not the
/// modules `ns` is written inside, not any other module — so `m::foo` and
/// `n::foo` are different symbols unless `std` exports `foo`.
///
/// `ns` may be [`NsId::KEYWORD`]; that is [`intern_keyword`], and the keyword
/// module sees nothing `std` exports. Nothing about `name` routes it anywhere
/// else — `":foo"` interned in a module is a symbol of that module whose name
/// starts with a colon, as CL's `intern` makes.
pub fn intern_in(ns: NsId, name: &str) -> SymRef {
    table().read(ns, name)
}

/// The symbol named `name` whose home is `ns`, creating it there if `ns`
/// holds none — without looking at what `std` exports.
///
/// For rebuilding a symbol whose home was recorded (a dump, compiled code
/// re-interning a quoted symbol): the home is the answer, and consulting
/// `std`'s exports could hand back a different symbol of the same name.
pub fn intern_at_home(ns: NsId, name: &str) -> SymRef {
    let mut t = table();
    if let Some(s) = t.lookup(ns, name) {
        return s;
    }
    t.create(ns, name, NOT_WELL_KNOWN)
}

/// A new symbol of `home` that no table holds: nothing reads as it, so a
/// definition it names is reached only through whoever made it — a
/// definition the source cannot write ([`crate::specialization`]). `name`
/// is what it is shown as.
pub fn unlisted(home: NsId, name: &str) -> SymRef {
    let mut t = table();
    // Leaked like every header, for the reasons `SymTable::create` gives.
    let header: &'static Symbol = Box::leak(Box::new(Symbol { spelling: name.into(), home, well_known: NOT_WELL_KNOWN }));
    t.created += 1;
    SymRef(header as *const Symbol)
}

/// [`intern_at_home`] for a home given as segments from the root.
pub fn intern_at_home_path(segments: &[String], name: &str) -> SymRef {
    intern_at_home(ns_of(segments), name)
}

/// The keyword named `name` — what the reader makes of `:name`. `name` is
/// the keyword's name, without the colon that spells it.
pub fn intern_keyword(name: &str) -> SymRef {
    intern_in(NsId::KEYWORD, name)
}

/// The module a namespace name names — the second argument of `intern` —
/// or `None` when there is no such module: CL's `intern` signals an error
/// for a package that does not exist rather than making one. A name is a
/// module path from the root with `::` between its segments
/// (`"geometry::shapes"`), `"keyword"` is the keyword module, and `""` is the
/// root.
pub fn ns_named(name: &str) -> Option<NsId> {
    let name = name.strip_prefix("::").unwrap_or(name);
    if name.is_empty() {
        return Some(NsId::ROOT);
    }
    let segments: Vec<String> = name.split("::").map(str::to_string).collect();
    find_ns(&segments)
}

/// The keyword module's segment under the root — CL's `KEYWORD` package,
/// and what `(intern NAME "keyword")` names.
pub const KEYWORD_MODULE: &str = "keyword";

/// [`intern_in`] at the root module — for every caller with no lexical module
/// to work from: the prelude and the island (which *are* root), the checker
/// building core-IR tags.
pub fn intern(name: &str) -> SymRef {
    intern_in(NsId::ROOT, name)
}

/// The symbol a member of a type — a method — is named by: made in
/// [`MEMBER_MODULE`], where a built-in method's name is the vocabulary
/// symbol its dispatch compares against.
pub fn member_symbol(name: &str) -> SymRef {
    static MEMBER: OnceLock<NsId> = OnceLock::new();
    let ns = *MEMBER.get_or_init(|| child_ns(NsId::ROOT, MEMBER_MODULE));
    intern_in(ns, name)
}

/// Makes `sym` visible in `into` under its own name, by pointer.
///
/// The imported name is *the same symbol*, so `==` compares equal across the
/// two modules — the only way one module is ever meant to see another's. Any
/// symbol `into` already had under that name is replaced.
pub fn import(into: NsId, sym: SymRef) {
    table().nodes[into.0 as usize].syms.insert(sym.name().into(), sym);
}

/// The module reached from `parent` by `segment`, creating it if new.
pub fn child_ns(parent: NsId, segment: &str) -> NsId {
    table().child(parent, segment)
}

/// What [`intern_in`] would return, without creating a symbol when there
/// is none: for looking a written name up in a table keyed by symbols, where
/// a name nothing defined must not leave a symbol behind.
pub fn find_exact(ns: NsId, name: &str) -> Option<SymRef> {
    table().find(ns, name)
}

/// The child module of `parent` whose name in `parent` is the symbol `name`
/// — the tree's own key, as [`ns_names`] gives it — if there is one.
pub fn child_ns_named(parent: NsId, name: SymRef) -> Option<NsId> {
    table().nodes[parent.0 as usize].children.get(&name).copied()
}

/// The definition a path from the root names — the module it is in and the
/// symbol it is defined under — reading `segments` as the tree's own keys:
/// the root's empty name, each module's name in its parent, then the
/// definition's name. `None` when the path does not start at the root.
///
/// # Panics
///
/// When a module segment is not a child of the module before it: the path
/// was not written from the tree.
pub fn definition_of_path(segments: &[SymRef]) -> Option<(NsId, SymRef)> {
    let (root, segs) = segments.split_first()?;
    if *root != intern_in(NsId::ROOT, "") {
        return None;
    }
    let (name, modules) = segs.split_last()?;
    let ns = modules.iter().fold(NsId::ROOT, |ns, m| {
        child_ns_named(ns, *m)
            .unwrap_or_else(|| panic!("`{}` is not a module of `{}`: the path was not written from the tree", m.name(), ns_path(ns).join("::")))
    });
    Some((ns, *name))
}

/// The module `ns` is a child of; `None` for the root and the system module.
pub fn ns_parent(ns: NsId) -> Option<NsId> {
    table().nodes[ns.0 as usize].parent
}

/// The module `segments` names from the root, if every segment exists — the
/// lookup [`ns_of`] makes, without creating modules.
pub fn find_ns(segments: &[String]) -> Option<NsId> {
    let t = table();
    let mut ns = NsId::ROOT;
    for s in segments {
        ns = t.find_child(ns, s)?;
    }
    Some(ns)
}

/// `ns`'s child modules, by the name of each, in no particular order.
pub fn ns_children(ns: NsId) -> Vec<(&'static str, NsId)> {
    table().nodes[ns.0 as usize].children.iter().map(|(name, id)| (name.name(), *id)).collect()
}

/// What names mean in one session: which modules the session has, which
/// names are types and what their members are, and what each `use` brought
/// in. The symbols themselves, and the tree of module names they belong to,
/// are the process's (see the module documentation); a name means something
/// only in the session that defined it. One per heap (`Heap::names`).
#[derive(Debug)]
pub struct SessionNames {
    /// The modules this session has: the fixed ones, every module a form was
    /// read in, every `(module ...)` read or checked, and their ancestors.
    modules: HashSet<NsId>,
    /// The types this session has, by their names' symbols, each with the
    /// names of its members (constructors, field accessors, static methods,
    /// methods): member name -> the member's symbol. A type is not a module;
    /// this is what `Type::member` is read against.
    types: HashMap<SymRef, HashMap<Box<str>, SymRef>>,
    /// The module each type in [`Self::types`] is defined in. A type's name
    /// need not be its module's own symbol — a built-in type at the root is
    /// named by the symbol `std` offers — so the symbol alone does not say.
    type_homes: HashMap<SymRef, NsId>,
    /// What `use` brought into a module, by the module and the symbol it was
    /// brought in under (that module's own).
    imports: HashMap<(NsId, SymRef), Import>,
    /// What every module reads a name as without bringing it in: the
    /// standard library's definitions, by name. Consulted by [`Self::read`]
    /// after what the module brought in itself.
    exports: HashMap<&'static str, SymRef>,
    /// The symbols a `(shadow ...)` made their modules' own ([`Self::shadow`]).
    shadowed: HashSet<SymRef>,
    /// The module of the top-level form being checked and run — CL's
    /// `*package*`: what a `read` at run time reads its names in.
    current: NsId,
}

impl SessionNames {
    /// A session with the fixed modules and nothing defined.
    pub fn new() -> SessionNames {
        let mut names = SessionNames {
            modules: HashSet::new(),
            types: HashMap::new(),
            type_homes: HashMap::new(),
            imports: HashMap::new(),
            exports: HashMap::new(),
            shadowed: HashSet::new(),
            current: NsId::ROOT,
        };
        for ns in [NsId::ROOT, NsId::KEYWORD, NsId::STD, NsId::CORE] {
            names.modules.insert(ns);
        }
        names
    }

    /// The module a `read` at run time reads its names in: the one the last
    /// [`Self::enter_module`] named.
    pub fn current_module(&self) -> NsId {
        self.current
    }

    /// Makes `ns` the module of the top-level form about to be checked and
    /// run.
    pub fn enter_module(&mut self, ns: NsId) {
        self.current = ns;
    }

    /// Records `ns`, and every module it is inside, as modules of this
    /// session.
    pub fn declare_module(&mut self, ns: NsId) {
        let t = table();
        let mut cur = Some(ns);
        while let Some(id) = cur {
            if !self.modules.insert(id) {
                break;
            }
            cur = t.nodes[id.0 as usize].parent;
        }
    }

    /// Records `ty` — the name of a type `home` defines — as a type, so a
    /// path can step into it (`ty::member`).
    pub fn declare_type(&mut self, home: NsId, ty: SymRef) {
        self.types.entry(ty).or_default();
        let prev = *self.type_homes.entry(ty).or_insert(home);
        assert!(prev == home, "type `{}` declared in two modules: {:?} and {:?}", ty.name(), prev, home);
    }

    /// The module that defines the type named `ty` ([`Self::declare_type`]).
    pub fn type_home(&self, ty: SymRef) -> Option<NsId> {
        self.type_homes.get(&ty).copied()
    }

    /// Records `member` as one of type `ty`'s members, under its name.
    pub fn declare_member(&mut self, ty: SymRef, member: SymRef) {
        self.types.entry(ty).or_default().insert(member.name().into(), member);
    }

    /// Records that `local` — `into`'s own symbol — stands for `what` in
    /// `into`, as a `use` there says.
    pub fn import_name(&mut self, into: NsId, local: SymRef, what: Import) {
        self.imports.insert((into, local), what);
    }

    /// Registers what a `use` written in `ns` brings in, when its path —
    /// `segments`, as written — leads to a module or a type this session
    /// has: under the path's last segment, read in `ns`. `false` when it
    /// leads to neither (a definition, or a module not loaded yet), which
    /// the `use` registers once it has resolved the path itself.
    pub fn import_written(&mut self, ns: NsId, segments: &[&str]) -> bool {
        let Some(last) = segments.last() else { return false };
        let (local, what) = {
            let mut t = table();
            let mut at: Option<PathStep> = None;
            for (i, seg) in segments.iter().enumerate() {
                let here = match at {
                    None if i == 0 && seg.is_empty() => {
                        at = Some(PathStep::Module(NsId::ROOT));
                        continue;
                    }
                    None => t.first_step(self, ns, seg),
                    Some(PathStep::Module(m)) => t.step_down(self, m, seg),
                    Some(PathStep::Type(_)) => None,
                };
                match here {
                    Some((_, step)) => at = Some(step),
                    None => return false,
                }
            }
            let what = match at {
                Some(PathStep::Module(id)) => Import::Module(id),
                Some(PathStep::Type(ty)) => Import::Type(ty),
                None => return false,
            };
            (t.read(ns, last), what)
        };
        self.import_name(ns, local, what);
        true
    }

    /// Makes every module read `sym`'s name as `sym` ([`Self::read`]).
    ///
    /// Panics if another symbol of the same name is already exported: one
    /// name read in a module is one symbol, so the library cannot offer two.
    pub fn export(&mut self, sym: SymRef) {
        let prev = *self.exports.entry(sym.name()).or_insert(sym);
        assert!(prev == sym, "two definitions offered to every module as `{}`: {:?} and {:?}", sym.name(), prev, sym);
    }

    /// Whether every module reads `sym`'s name as `sym` ([`Self::export`]).
    pub fn offers(&self, sym: SymRef) -> bool {
        self.exports.get(sym.name()) == Some(&sym)
    }

    /// The type `sym` names, when the standard library offers it to every
    /// module ([`Self::export`]): a path's first segment reaches it from any
    /// module, as the bare name does, though the type is defined elsewhere.
    fn offered_type(&self, sym: SymRef) -> Option<PathStep> {
        (self.offers(sym) && self.type_home(sym).is_some()).then_some(PathStep::Type(sym))
    }

    /// Makes `ns` read `name` as its own symbol from now on, in front of what
    /// the standard library offers every module under it — what
    /// `(shadow name)` written in `ns` says. The symbol is the one `ns`
    /// already owns under the name, else a new one.
    ///
    /// A reserved word cannot be shadowed: every module holds the same one,
    /// which is what lets a definition form mean the same thing everywhere.
    pub fn shadow(&mut self, ns: NsId, name: &str) -> Result<SymRef, String> {
        let mut t = table();
        let own = match t.lookup(ns, name) {
            Some(s) if s.home() == NsId::SYSTEM => {
                return Err(format!("shadow: `{}` is a reserved word, which every module shares", name));
            }
            Some(s) if s.home() == ns => s,
            _ => t.create(ns, name, NOT_WELL_KNOWN),
        };
        self.shadowed.insert(own);
        Ok(own)
    }

    /// What the name `name`, written in `ns`, reads as: a member of the fixed
    /// vocabulary `ns` holds under it, what a `(shadow ...)` in `ns` made its
    /// own, or what a `use` in `ns` brought in under it; else what the standard library offers every module under it
    /// ([`Self::export`]); else, in a module of the standard library, what
    /// `std` imports under it; else [`intern_in`]'s symbol.
    ///
    /// Any other symbol `ns`'s table holds under the name was made by reading
    /// it there earlier, which is no reason for it to stand in front of the
    /// library's.
    pub fn read(&self, ns: NsId, name: &str) -> SymRef {
        let mut t = table();
        if ns != NsId::KEYWORD {
            if let Some(own) = t.lookup(ns, name) {
                if own.well_known() != NOT_WELL_KNOWN || self.shadowed.contains(&own) || self.imports.contains_key(&(ns, own)) {
                    return own;
                }
            }
            if let Some(sym) = self.exports.get(name) {
                return *sym;
            }
            // A module of the standard library reads the library's names as
            // `std` imports them, offered to every module or not: the library
            // is written against all of itself.
            if ns != NsId::STD && t.within_std(ns) {
                if let Some(local) = t.lookup(NsId::STD, name) {
                    if let Some(Import::Item(sym) | Import::Type(sym)) = self.imports.get(&(NsId::STD, local)) {
                        return *sym;
                    }
                }
            }
        }
        t.read(ns, name)
    }

    /// What `local` stands for in `ns` by a `use`, if anything.
    pub fn imported(&self, ns: NsId, local: SymRef) -> Option<Import> {
        self.imports.get(&(ns, local)).copied()
    }

    /// The symbols a path written in `ns` reads as: `segments` taken one
    /// step at a time, each read in what the step before led to.
    ///
    /// The first segment leads where its name does in `ns` (an import, a
    /// child module, a type), else where it does in the root; an empty first
    /// segment is the root itself (`::name`). Every segment after it but the
    /// last must lead into a module, or to a type, after which only the last
    /// — a member of that type — may follow. The last is read in the module
    /// the path reached, or looked up among the type's members. A path that
    /// cannot be followed is an error that says where it stopped.
    pub fn read_path(&self, ns: NsId, segments: &[&str]) -> Result<Vec<SymRef>, String> {
        self.follow_path(ns, segments).map(|(syms, _)| syms)
    }

    /// [`Self::read_path`], with where the last segment was read: the module
    /// the path reached, or the type whose member it names.
    pub fn follow_path(&self, ns: NsId, segments: &[&str]) -> Result<(Vec<SymRef>, PathStep), String> {
        let mut t = table();
        let (last, steps) = segments.split_last().ok_or_else(|| "an empty path".to_string())?;
        let mut out = Vec::with_capacity(segments.len());
        let mut at: Option<PathStep> = None;
        for (i, seg) in steps.iter().enumerate() {
            let here = match at {
                None if seg.is_empty() => {
                    // `::a::b`: from the root. The empty segment stays a
                    // segment of the path, for the checker's "from root"
                    // reading.
                    out.push(t.read(NsId::ROOT, ""));
                    at = Some(PathStep::Module(NsId::ROOT));
                    continue;
                }
                None => t.first_step(self, ns, seg),
                Some(PathStep::Module(m)) => t.step_down(self, m, seg),
                Some(PathStep::Type(_)) => {
                    return Err(format!("`{}`: a type's member has no members of its own", segments[..=i].join("::")))
                }
            };
            let (sym, step) = here.ok_or_else(|| {
                format!("`{}`: no module or type `{}` is visible here", segments.join("::"), segments[..=i].join("::"))
            })?;
            out.push(sym);
            at = Some(step);
        }
        let reached = at.expect("a path has at least two segments");
        let last_sym = match reached {
            PathStep::Module(m) => t.read(m, last),
            PathStep::Type(ty) => *self.types[&ty].get(*last).ok_or_else(|| {
                format!("`{}`: type `{}` has no member `{}`", segments.join("::"), ty.name(), last)
            })?,
        };
        out.push(last_sym);
        Ok((out, reached))
    }

    /// Where the segments of a path — the symbols [`Self::read_path`] read
    /// them as — lead, for a path written in `ns`: the module the last of
    /// them reaches, or the type. Taken step by step as `read_path` took
    /// them, so a form read in `ns` reaches what its reading did. The first
    /// segment is also looked up in the module its symbol belongs to, after
    /// `ns`: a path a macro's expansion carried in from the macro's module
    /// leads where it did there.
    pub fn reached(&self, ns: NsId, segments: &[SymRef]) -> Result<PathStep, String> {
        let t = table();
        let spelled = |n: usize| segments[..n].iter().map(|s| s.name()).collect::<Vec<_>>().join("::");
        let (first, rest) = segments.split_first().ok_or_else(|| "an empty path".to_string())?;
        let mut at = if first.name().is_empty() {
            PathStep::Module(NsId::ROOT)
        } else {
            let home = (!first.is_shared_vocabulary()).then(|| first.home());
            [Some(ns), home]
                .into_iter()
                .flatten()
                .find_map(|outer| t.step_in_symbol(self, outer, *first))
                .or_else(|| self.offered_type(*first))
                .or_else(|| [NsId::ROOT, NsId::STD].into_iter().find_map(|outer| t.step_in_symbol(self, outer, *first)))
                .ok_or_else(|| format!("`{}`: no module or type `{}` is visible here", spelled(segments.len()), first.name()))?
        };
        for (i, seg) in rest.iter().enumerate() {
            at = match at {
                PathStep::Module(m) => t.step_down_symbol(self, m, *seg).ok_or_else(|| {
                    format!("`{}`: no module or type `{}` is visible here", spelled(segments.len()), spelled(i + 2))
                })?,
                PathStep::Type(_) => {
                    return Err(format!("`{}`: a type's member has no members of its own", spelled(i + 2)))
                }
            };
        }
        Ok(at)
    }

    /// Whether `name`, read in `ns` as the first segment of a path, names a
    /// module or a type — what [`Self::read_path`] can step into. Anything
    /// else before a `::` names a value (`p::x`), whose type only the
    /// analysis knows.
    pub fn names_scope(&self, ns: NsId, name: &str) -> bool {
        name.is_empty() || self.first_segment(ns, name).is_some()
    }

    /// The type `name`, read in `ns`, refers to — found the way the first
    /// segment of a path is ([`Self::read_path`]) — if it refers to one.
    pub fn type_named(&self, ns: NsId, name: &str) -> Option<SymRef> {
        match self.first_segment(ns, name)? {
            PathStep::Type(t) => Some(t),
            PathStep::Module(_) => None,
        }
    }

    /// Where `name`, read in `ns` as the first segment of a path, leads
    /// ([`Self::read_path`]): a module or a type, if either.
    pub fn first_segment(&self, ns: NsId, name: &str) -> Option<PathStep> {
        table().first_step(self, ns, name).map(|(_, step)| step)
    }
}

/// The module `segments` names, relative to the root.
pub fn ns_of(segments: &[String]) -> NsId {
    let mut ns = NsId::ROOT;
    for s in segments {
        ns = child_ns(ns, s);
    }
    ns
}

/// The symbol `ns` is named by in the module it is a child of; `None` for
/// the root and the system module.
pub fn ns_name(ns: NsId) -> Option<SymRef> {
    table().nodes[ns.0 as usize].name
}

/// The names of `ns` and of the modules above it, from the root down: the
/// symbol each is a child of the one before under.
pub fn ns_names(ns: NsId) -> Vec<SymRef> {
    let t = table();
    let mut names = Vec::new();
    let mut cur = Some(ns);
    while let Some(id) = cur {
        let node = &t.nodes[id.0 as usize];
        if let Some(name) = node.name {
            names.push(name);
        }
        cur = node.parent;
    }
    names.reverse();
    names
}

/// `ns`'s segments from the root, for diagnostics.
pub fn ns_path(ns: NsId) -> Vec<String> {
    table().path(ns)
}

/// How many symbols exist — headers created, not names visible. Diagnostics
/// only: the count is process-global and other threads may be interning.
pub fn symbol_count() -> usize {
    table().created
}

/// The system module's symbol at index `i` of [`BUILTIN_SYMBOLS`] — the
/// inverse of [`SymRef::well_known`].
///
/// This is how a member of the fixed vocabulary crosses into a compiled
/// program: its *address* cannot (an ahead-of-time compiled program is another
/// process), but its index can, because every process builds the vocabulary
/// from the same list in the same order. No name is spelled out and no lookup
/// happens — the alternative, rebuilding the name and interning it, is the
/// thing this exists to avoid.
///
/// Panics on an index outside the vocabulary: the caller is a compiler that
/// read the index off a symbol, so an out-of-range one is a bug here, not bad
/// input.
pub fn well_known_symbol(i: u32) -> SymRef {
    if WELL_KNOWN.get().is_none() {
        // Building the main table is what fills this in. Deliberately not
        // `get_or_init`: the initializer would have to build that table, whose
        // own construction `set`s this very cell — re-entering a `OnceLock`
        // that is mid-initialization deadlocks.
        drop(table());
    }
    WELL_KNOWN.get().expect("building the table fills WELL_KNOWN")[i as usize]
}

