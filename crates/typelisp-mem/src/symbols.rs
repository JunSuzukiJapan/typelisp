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

use std::collections::HashMap;
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
        ///
        /// Names must already be lowercase: interning folds case, so a name
        /// spelled otherwise would intern under a different one.
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
    UPCASE => ":upcase"
    CAPITALIZE => ":capitalize"

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
     another binding already holds. CL's `shadow` and `unuse-package` have no
     entries because they are not adopted — see cl-parity-plan.md Stage 9a."
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
    KW_DOWNCASE => ":downcase"
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
    /// The canonical (lowercase) spelling: the name, with a colon in front
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

impl SymRef {
    fn header(&self) -> &'static Symbol {
        // SAFETY: the only way to obtain a `SymRef` is from the table, which
        // leaks every header it builds — so this address is always a live,
        // immutable `Symbol` (see the module doc comment's property 2).
        unsafe { &*self.0 }
    }

    /// The symbol's canonical (lowercase) name — CL's `symbol-name`, so a
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

/// The vocabulary members owned by [`NsId::STD`] rather than the system
/// module: names the standard library defines, which a module may define for
/// itself only after declaring so (`shadow`). Exported by `std`.
pub const STD_VOCABULARY: &[u32] = &[
    wk::COND, wk::AND, wk::OR, wk::WHEN, wk::UNLESS, wk::WHILE, wk::UNTIL, wk::CASE,
    wk::DOLIST, wk::DOTIMES, wk::DOITER, wk::DO, wk::NOT, wk::INTERN,
];

/// The vocabulary members owned by [`NsId::CORE`]: core-IR tags with no
/// meaning in source, the value representations the IR names, the names
/// retired from the vocabulary, and the reader's attribute head.
pub const CORE_VOCABULARY: &[u32] = &[
    wk::RETIRED_WHERE, wk::RETIRED_EQUALS, wk::INT, wk::RETIRED_FLOAT, wk::BIGNUM, wk::RATIO, wk::CHAR,
    wk::BOOL, wk::STR, wk::UNIT, wk::VAR, wk::SET, wk::GLOBAL, wk::SET_GLOBAL, wk::CALL, wk::ASSOC,
    wk::CONSTRUCT, wk::FIELD_GET, wk::FIELD_SET, wk::DYN_NEW, wk::DYN_UPCAST, wk::DYN_CALL, wk::DYN_VALUE,
    wk::SYM, wk::FNREF, wk::METHODREF, wk::COMPILE_FN, wk::METHOD, wk::PAT_WILD, wk::PAT_EMPTY,
    wk::RETIRED_PAT_NONEMPTY, wk::PAT_BIND, wk::PAT_LIT, wk::PAT_GUARD, wk::PAT_CTOR, wk::PAT_TYPETEST,
    wk::EXPR, wk::NIL, wk::CONS, wk::PATH, wk::INT_ANY_WIDTH, wk::FLOAT_ANY_WIDTH,
    wk::I32, wk::F64, wk::F32, wk::I8, wk::I16, wk::U8, wk::U16, wk::U32, wk::DISASSEMBLE_FN,
    wk::RETIRED_GO, wk::UNTAG_INT, wk::TAG_INT, wk::SOME_OF, wk::PAT_SOME, wk::BOX_OPTION,
    wk::RETIRED_ELSE, wk::SPAWN, wk::TAG, wk::SPAWN_THREAD, wk::PAT_WHEN, wk::INTERNAL_MODULE_NAME,
    wk::ATTRIBUTE, wk::APPLY_INDIRECT, wk::CELLVAR, wk::CELLSET, wk::GLOBAL_INIT, wk::HASHTABLE_OP,
    wk::LLVM_OP, wk::SUSPEND, wk::VECTOR_OP, wk::REPR_HANDLE, wk::REPR_RAW_WORD, wk::REPR_RATIONAL,
    wk::REPR_RANDOM_STATE, wk::REPR_SEXPR, wk::REPR_STRUCT, wk::REPR_ENUM, wk::REPR_DYN, wk::REPR_SCOPE,
    wk::REPR_VECTOR, wk::REPR_HASHTABLE, wk::REPR_NICHE, wk::REPR_NONE,
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
        let vocabulary: Vec<SymRef> = BUILTIN_SYMBOLS
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let i = i as u32;
                if let Some(kw) = name.strip_prefix(':') {
                    t.create(NsId::KEYWORD, kw, i)
                } else if STD_VOCABULARY.contains(&i) {
                    let sym = t.create(NsId::STD, name, i);
                    t.std_exports.insert((*name).into(), sym);
                    sym
                } else if CORE_VOCABULARY.contains(&i) {
                    t.create(NsId::CORE, name, i)
                } else {
                    t.create(NsId::SYSTEM, name, i)
                }
            })
            .collect();
        WELL_KNOWN
            .set(vocabulary.into_boxed_slice())
            .unwrap_or_else(|_| unreachable!("the table is built exactly once"));
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
    /// already held. `name` is already lowercase.
    fn read(&mut self, ns: NsId, name: &str) -> SymRef {
        if let Some(s) = self.lookup(ns, name) {
            return s;
        }
        if ns != NsId::KEYWORD {
            if let Some(s) = self.std_exports.get(name) {
                return *s;
            }
        }
        self.create(ns, name, NOT_WELL_KNOWN)
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
/// Names are **case-insensitive**: they fold to a canonical lowercase form, so
/// `Foo`, `FOO` and `foo` all intern to one symbol.
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
    table().read(ns, &name.to_lowercase())
}

/// The symbol named `name` whose home is `ns`, creating it there if `ns`
/// holds none — without looking at what `std` exports.
///
/// For rebuilding a symbol whose home was recorded (a dump, compiled code
/// re-interning a quoted symbol): the home is the answer, and consulting
/// `std`'s exports could hand back a different symbol of the same name.
pub fn intern_at_home(ns: NsId, name: &str) -> SymRef {
    let key = name.to_lowercase();
    let mut t = table();
    if let Some(s) = t.lookup(ns, &key) {
        return s;
    }
    t.create(ns, &key, NOT_WELL_KNOWN)
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

/// The module a namespace name names — the second argument of `intern`.
/// A name is a module path from the root with `::` between its segments
/// (`"geometry::shapes"`), `"keyword"` is the keyword module, and `""` is the
/// root. Segments fold to lowercase as module names do.
pub fn ns_named(name: &str) -> NsId {
    let name = name.strip_prefix("::").unwrap_or(name);
    if name.is_empty() {
        return NsId::ROOT;
    }
    let segments: Vec<String> = name.split("::").map(str::to_string).collect();
    ns_of(&segments)
}

/// The keyword module's segment under the root — CL's `KEYWORD` package,
/// and what `(intern NAME "keyword")` names.
pub const KEYWORD_MODULE: &str = "keyword";

/// [`intern_in`] at the root module — for every caller with no lexical module
/// to work from: the prelude and the island (which *are* root), the checker
/// building core-IR tags, and the runtime's `string->symbol`/`gensym`, whose
/// symbols are made while the program runs rather than while it is read.
pub fn intern(name: &str) -> SymRef {
    intern_in(NsId::ROOT, name)
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
    table().child(parent, &segment.to_lowercase())
}

/// The module `segments` names, relative to the root.
pub fn ns_of(segments: &[String]) -> NsId {
    let mut ns = NsId::ROOT;
    for s in segments {
        ns = child_ns(ns, s);
    }
    ns
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

