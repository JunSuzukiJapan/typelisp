//! The compiler's fixed vocabulary: `BUILTIN_SYMBOLS` and one `wk` constant per
//! entry.

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
     `E`'s value as a tagged field word (`None` stays `sexpr`'s empty-list
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
     with `PATH` the definition's path from the root; a field's template, and
     a generic definition's body, lowered but never run, name the `N`th
     parameter as `(type-param N)`. `type-var` is retired: it named a
     parameter by the name its declaration wrote."
    TYPE_NAMED => "type-named"
    TYPE_FN => "type-fn"
    TYPE_DYN => "type-dyn"
    TYPE_PTR => "type-ptr"
    TYPE_CSTRUCT => "type-cstruct"
    TYPE_PARAM => "type-param"
    RETIRED_TYPE_VAR => "type-var"
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
    M_ARG_LIST => "arg-list"
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
    M_NO_VALUE => "no-value"
    M_CONST_VALUE => "const-value"
    M_CONST_LIKE => "const-like"
    M_CONVERT => "convert"
    M_COERCE => "coerce"
    M_TAG_FIELD => "tag-field"
    M_UNTAG_FIELD => "untag-field"
    M_GLOBAL_GET => "global-get"
    M_GLOBAL_PUT => "global-put"
    M_GLOBAL_NEW => "global-new"
    M_BUILD_COND_BR => "build-cond-br"
    M_BUILD_FIXED_BINOP => "build-fixed-binop"
    M_BUILD_FIXED_DIV_CHECK => "build-fixed-div-check"
    M_BUILD_FIXED_UNARY => "build-fixed-unary"
    M_BUILD_FLOAT_BINOP => "build-float-binop"
    M_BUILD_FLOAT_UNARY => "build-float-unary"
    M_BUILD_FN_ADDRESS => "build-fn-address"
    M_ADD_FRAME_SLOT => "add-frame-slot"
    M_BUILD_ICMP_EQ => "build-icmp-eq"
    M_BUILD_ICMP_GE => "build-icmp-ge"
    M_BUILD_ICMP_GT => "build-icmp-gt"
    M_BUILD_ICMP_LE => "build-icmp-le"
    M_BUILD_ICMP_LT => "build-icmp-lt"
    M_BUILD_ICMP_NE => "build-icmp-ne"
    M_ADD_FRAME_SLOT_ROOTED => "add-frame-slot-rooted"
    M_BUILD_LSHR => "build-lshr"
    M_BUILD_MAKE_CLOSURE => "build-make-closure"
    M_FRAME_SLOTS => "frame-slots"
    M_BUILD_MUL => "build-mul"
    M_BUILD_OR => "build-or"
    M_BUILD_NOT => "build-not"
    M_BUILD_RET => "build-ret"
    M_BUILD_SADD_OVERFLOW => "build-sadd-overflow"
    M_BUILD_SELECT => "build-select"
    M_BUILD_SHL => "build-shl"
    M_TEMP_SLOT => "temp-slot"
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
    M_C_LONG_BITS => "c-long-bits"
    M_DENOMINATOR => "denominator"
    M_DIGITP => "digitp"
    M_DOWNCASE => "downcase"
    M_ENTRIES => "entries"
    M_ENTRY_VALUE => "entry-value"
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
    M_SLOT_GET => "slot-get"
    M_LOAD_TYPE => "load-type"
    M_LOOP_SAFEPOINT => "loop-safepoint"
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
    M_REM => "rem"
    M_ROUND => "round"
    M_SCRATCH_SLOT => "scratch-slot"
    M_SIN => "sin"
    M_SINH => "sinh"
    M_SQRT => "sqrt"
    M_SLOT_SET => "slot-set"
    M_SUBSTRING => "substring"
    M_TAN => "tan"
    M_TANH => "tanh"
    M_TO_STRING => "to-string"
    M_TRUNCATE => "truncate"
    M_TRY_FLOAT_TO_F32 => "try-float->f32"
    M_TRY_FLOAT_TO_F64 => "try-float->f64"
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
     methods' names above, taken into `core::cmp`."
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
    "Retired on 2026-11-22, when `format`/`print`/`println` came to be
     expanded into the `F_FMT_*` pieces instead of lowering to one builtin
     each: a rename in place would change what an already-compiled index
     resolves to. Nothing produces these."
    RETIRED_FORMAT_RT => "format-rt"
    RETIRED_PRINT_RT => "print-rt"
    RETIRED_PRINTLN_RT => "println-rt"
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
    PAT_DYN => "pat-dyn"
    M_HASH_FUNCTION => "hash-function"
    M_TEST_FUNCTION => "test-function"
    F_SXHASH_EQ => "sxhash-eq"
    F_SXHASH_EQL => "sxhash-eql"
    F_SXHASH_EQUALP => "sxhash-equalp"
    "Appended 2026-11-18. `&environment` is a macro lambda-list keyword, recognized
     by identity like the others; `define-symbol-macro` is the head of a
     top-level definition form."
    AMP_ENVIRONMENT => "&environment"
    DEFINE_SYMBOL_MACRO => "define-symbol-macro"
    "Appended 2026-11-22. `format`/`print`/`println` are expanded when they are
     lowered into calls of these (`typeck::format_expand`), and the buffer they
     write into is a `format-out`. Spelled with a leading space, like the C
     memory builtins, so no source can name one."
    FORMAT_OUT => "format-out"
    F_FMT_OPEN => " fmt-open"
    F_FMT_TEXT => " fmt-text"
    F_FMT_FRESH_LINE => " fmt-fresh-line"
    F_FMT_A => " fmt-a"
    F_FMT_STRING => " fmt-string"
    F_FMT_NUMBER => " fmt-number"
    F_FMT_VALUE => " fmt-value"
    F_FMT_D => " fmt-d"
    F_FMT_INTEGER => " fmt-integer"
    F_FMT_INTEGER_ANY => " fmt-integer-any"
    F_FMT_WORDS => " fmt-words"
    F_FMT_PLURAL => " fmt-plural"
    F_FMT_CHAR => " fmt-char"
    F_FMT_FIXED => " fmt-fixed"
    F_FMT_EXPONENT => " fmt-exponent"
    F_FMT_GENERAL => " fmt-general"
    F_FMT_DOLLARS => " fmt-dollars"
    F_FMT_TAB => " fmt-tab"
    F_FMT_NEWLINE => " fmt-newline"
    F_FMT_INDENT => " fmt-indent"
    F_FMT_CASE_OPEN => " fmt-case-open"
    F_FMT_CASE_CLOSE => " fmt-case-close"
    F_FMT_JUST_OPEN => " fmt-just-open"
    F_FMT_JUST_SEGMENT => " fmt-just-segment"
    F_FMT_JUST_CLOSE => " fmt-just-close"
    F_FMT_BLOCK_OPEN => " fmt-block-open"
    F_FMT_BLOCK_CLOSE => " fmt-block-close"
    F_FMT_RESULT => " fmt-result"
    F_FMT_EMIT => " fmt-emit"
    F_FMT_FIRST => " fmt-first"
    F_FMT_REST => " fmt-rest"
    F_FMT_EMPTY => " fmt-empty"
    F_FMT_LENGTH => " fmt-length"
    F_FMT_GOTO => " fmt-goto"
    F_FMT_AS_LIST => " fmt-as-list"
    F_FMT_PARAM => " fmt-param"
    F_FMT_SELECT => " fmt-select"
    F_FMT_BOOL => " fmt-bool"
    "Appended 2026-11-23. The volatile reads and writes of a memory-mapped
     register, one per width, and the internal builtins they lower to."
    VOLATILE_READ_U8 => "volatile-read-u8"
    VOLATILE_READ_U16 => "volatile-read-u16"
    VOLATILE_READ_U32 => "volatile-read-u32"
    VOLATILE_WRITE_U8 => "volatile-write-u8"
    VOLATILE_WRITE_U16 => "volatile-write-u16"
    VOLATILE_WRITE_U32 => "volatile-write-u32"
    F_C_VOLATILE_LOAD => " c-volatile-load"
    F_C_VOLATILE_STORE => " c-volatile-store"
    F_ENABLE_INTERRUPT => "enable-interrupt"
    F_DISABLE_INTERRUPT => "disable-interrupt"
    "Appended 2026-11-25. The largest fixnum of the machine the code runs on,
     which `most-positive-fixnum` is."
    F_LARGEST_FIXNUM => "largest-fixnum"
    F_C_ADDRESS => " c-address"
    "Appended 2026-11-25. How many bytes a value takes."
    SIZE_OF => "size-of"
    SIZE_OF_VAL => "size-of-val"
    "Appended 2026-11-25. Where compiled code may collect on its way into a
     function."
    M_ENTRY_SAFEPOINT => "entry-safepoint"
    "Appended 2026-11-26. A float converted to an integer of a fixed width,
     with no `int` on the way."
    M_FLOAT_TO_I8 => "float->i8"
    M_FLOAT_TO_I16 => "float->i16"
    M_FLOAT_TO_I32 => "float->i32"
    M_FLOAT_TO_U8 => "float->u8"
    M_FLOAT_TO_U16 => "float->u16"
    M_FLOAT_TO_U32 => "float->u32"
}
