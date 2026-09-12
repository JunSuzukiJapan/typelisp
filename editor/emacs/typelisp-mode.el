;;; typelisp-mode.el --- Major mode for the typelisp language -*- lexical-binding: t; -*-

;; Author: typelisp project
;; Keywords: languages, lisp
;; Version: 0.1.0
;; Package-Requires: ((emacs "26.1"))

;;; Commentary:

;; A major mode for editing typelisp source files (`.typl').
;;
;; typelisp is a statically-typed Lisp with S-expression syntax.  This mode
;; provides syntax highlighting (special forms, definitions, builtin
;; functions, primitive/generic types, numeric/character literals, `format'
;; control-string directives), comment handling (`;' line comments and
;; nestable `#| ... |#' block comments), Lisp-style s-expression navigation
;; and indentation, an `imenu' index of the file's definitions, and commands
;; to run the file or start a REPL through the `typl' CLI.
;;
;; Installation:
;;
;;   (add-to-list 'load-path "/path/to/typelisp/editor/emacs")
;;   (require 'typelisp-mode)
;;
;; `.typl' files will then open in `typelisp-mode' automatically.
;;
;; For diagnostics, hover, goto-definition and completion, point `eglot' or
;; `lsp-mode' at the `typl-lsp' language server -- see README.md.

;;; Code:

(require 'lisp-mode)
(require 'imenu)
(require 'comint)
(require 'compile)

;; `eglot' (and the `jsonrpc' library it is built on) is optional: the mode is
;; fully usable without a language server, so these are declared rather than
;; required.  Every call site is guarded by `fboundp' or by eglot having
;; already loaded -- see the semantic-tokens section.
(declare-function jsonrpc-async-request "jsonrpc")
(declare-function eglot-current-server "eglot")
(declare-function eglot--capabilities "eglot")
(declare-function eglot-server-capabilities "eglot")
(declare-function eglot-path-to-uri "eglot")
(declare-function eglot--path-to-uri "eglot")
(declare-function eglot--signal-textDocument/didChange "eglot")

(defgroup typelisp nil
  "Major mode for editing typelisp code."
  :group 'languages
  :prefix "typelisp-")

;;; Keyword tables ----------------------------------------------------------

(defconst typelisp-definition-forms
  '("defun" "defsignature" "defffi" "defmethod" "defmacro")
  "Definition forms whose defined name is a function name.
`defsignature' declares one ahead of its definition -- the only way to write
mutual recursion at top level, since forms are checked in source order.
`defffi' declares one whose body is a C function, reached through a thunk;
like `defsignature' it has no body here, so neither takes an indent rule.")

(defconst typelisp-type-definition-forms
  '("defstruct" "defenum" "deftrait" "deftype")
  "Definition forms whose defined name is a type, trait or type-alias name.
Types and traits share one namespace (as in Rust), which is why they share a
rule here: within a module a `defstruct'/`defenum' and a `deftrait' cannot
take the same name.  A `deftype' alias occupies that same namespace -- it is
a spelling for a type, expanded where it is written.")

(defconst typelisp-variable-definition-forms
  '("defvar" "defparameter" "defconstant")
  "Definition forms whose defined name is a variable.
`defvar' initializes only an unbound global, `defparameter' always assigns --
CL's distinction, which is what makes re-loading a file keep the values a
session has changed.")

(defconst typelisp-special-forms
  '(;; binding & conditionals (docs/syntax.md §4)
    "let" "let*" "if" "when" "unless" "cond" "case" "and" "or" "progn" "unsafe"
    "the" "match" "if-let" "while-let"
    ;; CL's chapter-5 control forms (cl-parity-plan.md Phase 4a, §4/§7)
    "ecase" "ccase" "prog1" "prog2" "setq" "psetq" "psetf"
    ;; iteration (§5)
    "loop" "while" "until" "dotimes" "dolist" "do" "do*" "doiter"
    "break" "return"
    ;; the lexical named escape (Phase 4a): `block' is the target,
    ;; `return-from' leaves it — still a *static* exit, like `break'
    "block" "return-from"
    ;; concurrency: `go' starts a task with a call
    "go"
    ;; non-local exit (§8) — `break'/`return'/`return-from' above are the
    ;; *static* exits, these are the dynamic ones
    "catch" "throw" "unwind-protect"
    ;; function values & application (§6)
    "lambda" "labels" "apply"
    ;; local macro bindings (functions.md §14.1) — like `labels', but the
    ;; names they bind are not values
    "macrolet" "symbol-macrolet"
    ;; other special forms (§7)
    "setf" "incf" "decf" "rotatef" "shiftf"
    "list" "quote" "quasiquote" "unquote" "unquote-splicing"
    "panic" "unreachable" "todo" "as" "try-as" "compile" "documentation"
    ;; the REPL tool layer (CLHS 25.2).  Special forms because each takes the
    ;; *name* of a definition rather than a value — `step' is the exception,
    ;; which takes a form.  `room'/`dribble' are ordinary functions and are
    ;; listed with the builtins below.
    "trace" "untrace" "step" "disassemble" "ed"
    ;; the file this form was read from, folded to a literal at check time
    ;; (Stage 9b) — CL's `*load-pathname*' in the place it can be right
    "source-file"
    ;; formatted output — special forms so that each variadic argument keeps
    ;; its own type on the way into `Sexpr' (functions.md §15)
    "print" "println" "format"
    ;; pretty printer (functions.md §15.1)
    "pprint" "pprint-fill" "pprint-linear" "pprint-tabular"
    "pprint-logical-block")
  "Control-flow and other special forms.")

(defconst typelisp-clause-keywords
  '("else")
  "Clause markers recognized inside `cond' / `case' forms.")

(defconst typelisp-declaration-keywords
  '("pub" "module" "in-module" "use" "import" "shadowing-import" "load" "impl" "where")
  "Declaration / namespace keywords.
`in-module' is the flat form of `module'; `import' is CL's spelling of `use',
and `shadowing-import' the one that means to take a bare name something else
already holds.")

(defconst typelisp-lambda-list-keywords
  '("&rest" "&optional" "&key")
  "Lambda-list markers.
`&rest' appears in `defun' / `defmacro' / `lambda' parameter lists and in
function types; `&optional' and `&key' are `defmacro'-only.")

(defconst typelisp-builtin-functions
  '(;; streams and files (functions.md §18)
    "at-line-start" "char->string" "close" "copy-stream" "delete-file"
    "direction-append" "direction-input" "direction-output" "finish-output"
    "fresh-line" "get-output-stream-string" "make-broadcast-stream" "make-concatenated-stream"
    "make-echo-stream" "make-peek-stream" "make-string-input-stream" "make-string-output-stream"
    "make-two-way-stream"
    "open-binary" "open-binary-input" "open-binary-output"
    "destructuring-bind" "sleep"
    ;; concurrency: the task handle's own method, the voluntary switch, and
    ;; the channel operations (`close'/`len' are shared with streams and
    ;; sequences and are listed with those)
    "wait" "yield"
    "send" "recv" "cap" "after"
    ;; the readtable (syntax.md §11)
    "get-dispatch-macro-character" "get-macro-character"
    "set-dispatch-macro-character" "set-macro-character"
    "read-datum-at" "read-delimited-list" "read-from-string"
    "read-from-string-preserving-whitespace" "read-sexpr-preserving-whitespace"
    "listen" "open-file" "open-input" "open-output" "open-stream-p" "peek-char"
    "read-byte" "write-byte"
    "probe-file" "read-all" "read-char" "read-char-no-hang" "read-file-lines"
    "read-file-string" "read-item" "read-line" "read-lines" "read-sequence" "read-sexpr"
    "rename-file" "write-sequence"
    "terpri" "unread-char" "with-input-from-string" "with-open-file" "with-output-to-string"
    "write-char" "write-file-string" "write-item" "write-line"
    "write-lines" "write-string"
    ;; pathnames (functions.md §19)
    "directory-namestring" "enough-namestring" "file-namestring" "make-pathname"
    "merge-pathnames" "namestring" "parse-namestring" "pathname-absolute-p"
    "pathname-directory" "pathname-name" "pathname-type" "to-pathname"
    ;; numeric helpers (§4)
    "abs" "gcd" "lcm" "signum" "random" "expt" "sqrt" "floor" "ceiling"
    "round" "truncate" "floor-div" "ceiling-div" "round-div" "truncate-div"
    "mod" "rem" "not"
    ;; CL numeric predicates and max/min (`1+`/`1-` are punctuation-only
    ;; names, like `+`/`-` themselves, so `is_excluded`/the operator pattern
    ;; already covers them — see editor_keyword_sync_test.rs)
    "zerop" "plusp" "minusp" "evenp" "oddp" "max" "min"
    ;; transcendental functions (f64 only)
    "sin" "cos" "tan" "asin" "acos" "atan" "sinh" "cosh" "tanh"
    "asinh" "acosh" "atanh" "exp" "log"
    ;; bitwise operators (fixed-width integers, plus bignum for the first nine)
    "logand" "logior" "logxor" "lognot" "ash" "logbitp" "logcount" "logtest"
    "integer-length"
    ;; the rest of the bitwise catalog (fixed-width integers and bignum, built from the above)
    "logeqv" "lognand" "lognor" "logandc1" "logandc2" "logorc1" "logorc2"
    ;; byte-specifier mini-API (i32 only)
    "byte" "byte-size" "byte-position" "ldb" "ldb-test" "dpb" "mask-field"
    "deposit-field" "boole"
    ;; numeric / char conversions (§4, §2.5).  The `as'/`try-as' special
    ;; forms are sugar over exactly these.
    "int->float" "int->char" "try-int->char" "float->int" "char->int"
    ;; the width casts, one pair per fixed-width type -- `as'/`try-as'
    ;; between two integer (or two float) types desugars into these
    "int->i8" "int->i16" "int->i32" "int->u8" "int->u16" "int->u32"
    "try-int->i8" "try-int->i16" "try-int->i32"
    "try-int->u8" "try-int->u16" "try-int->u32"
    ;; the same casts into and out of the two C-boundary words -- these are
    ;; all a `c-long' / `c-ulong' carries, since they have no arithmetic
    "int->c-long" "int->c-ulong" "try-int->c-long" "try-int->c-ulong"
    "float->f32" "float->f64" "try-float->f32" "try-float->f64"
    "int->bignum" "bignum->int" "try-bignum->int" "bignum->float"
    "float->bignum" "bignum->ratio" "ratio->bignum" "int->ratio"
    "float->ratio" "ratio->float" "numerator" "denominator"
    ;; the prelude helper `expt' recurses through for a `ratio' base -- an
    ;; ordinary root-namespace function, so it is callable and highlighted
    "ratio-expt-int"
    "symbol->string" "string->symbol"
    ;; generic pair (cons-cell<A,B>); set-car/set-cdr were removed, mutate
    ;; via (setf p::car v)/(setf p::cdr v) instead
    "cons" "car" "cdr"
    ;; Sexpr accessors/constructors used by macro bodies (§5)
    "sexpr-car" "sexpr-cdr" "sexpr-cons" "sexpr-consp" "sexpr-null"
    "sexpr-atom" "sexpr-symp" "sexpr-sym-name"
    "sexpr-i8" "sexpr-i16" "sexpr-i32" "sexpr-u8" "sexpr-u16" "sexpr-u32"
    "sexpr-f32" "sexpr-f64"
    "sexpr-char" "sexpr-bool" "sexpr-str" "sexpr-append" "sexpr-map"
    ;; equality
    "eq" "eql" "equal" "equalp"
    ;; Iter-based sequence library (§6).  `find'/`position'/`count' are CL's
    ;; own item-based (`Eq'-bounded) versions; `find-if'/`position-if'/
    ;; `count-if'/`remove-if' take a predicate instead.
    "length" "append" "nth" "elt" "take" "subseq" "last" "butlast"
    "member" "every" "any" "sort" "assoc" "map" "filter" "foldl" "foldr"
    "reverse" "find" "position" "count" "find-if" "position-if" "count-if"
    "remove-if"
    ;; Option / Result methods (§7)
    "unwrap" "unwrap-or" "is-some" "is-none" "is-ok" "is-err"
    ;; `Error' trait methods and the concrete->trait-object widener (§7.1)
    "message" "source" "as-dyn-error"
    ;; Vector / HashTable methods
    "new" "push" "get" "set" "len" "iter" "pop" "clear" "keys" "values"
    "entries" "remove"
    ;; the `Iter' trait's own method -- `iter' gets the cursor, `next' advances
    ;; it (§12)
    "next"
    ;; string / char methods
    "upcase" "downcase" "ref" "substring" "alphap" "digitp" "lt"
    ;; the rest of CL's numeric catalog (Phase 1c, §2/§4.3)
    "ffloor" "fceiling" "fround" "ftruncate" "isqrt" "rationalize"
    "float-radix" "float-digits" "float-precision" "float-sign"
    "scale-float" "decode-float" "integer-decode-float"
    ;; the CL character/string catalog (cl-parity-plan.md Phase 2a/2b, §8/§9):
    ;; case-insensitive order, case and class predicates, the CL-conformant
    ;; digit weight, character names, and the string utilities.
    "lessp" "greaterp" "not-lessp" "not-greaterp"
    "upper-casep" "lower-casep" "both-casep" "alphanumericp"
    "graphicp" "standardp" "digit-weight" "digit->char"
    "char->name" "name->char"
    "filled" "search" "mismatch" "trim" "left-trim" "right-trim"
    "capitalize" "split"
    ;; their code-point helpers -- ordinary public prelude functions, so the
    ;; registry offers them and this list has to know them
    "ascii-alpha-code" "ascii-digit-code" "ascii-downcase-code"
    "ascii-upcase-char" "ascii-downcase-char"
    "char-in-bag" "string-fold-compare"
    ;; higher-order combinators
    "identity" "const" "compose" "flip"
    ;; the CL list/sequence catalog (cl-parity-plan.md Phase 3a/3b/3c, §6.1) --
    ;; generic `defun's over the `Iter' trait
    "first" "second" "third" "fourth" "fifth" "sixth"
    "seventh" "eighth" "ninth" "tenth"
    "acons" "adjoin" "assoc-if" "copy-seq" "count-if-not" "find-if-not"
    "intersection" "ldiff" "map2" "mapc" "mapcan" "maplist"
    "member-if" "member-if-not" "merge" "notany" "notevery" "pairlis"
    "position-if-not" "rassoc" "rassoc-if" "remove-duplicates" "remove-if-not"
    "rest" "revappend"
    "seq-equals" "set-difference" "set-exclusive-or" "subsetp" "substitute" "substitute-if"
    "tailp" "union"
    ;; the Phase 3e keyword layer's shared loop cores (§6.3)
    "seq-any-core" "seq-count-core" "seq-dedup-core" "seq-edit-core"
    "seq-find-core" "seq-flag" "seq-in-bounds" "seq-limit"
    "seq-position-core" "seq-sort-core" "seq-window-start" "seq-window-end"
    "string-window-equal"
    ;; the 28 `c*r' pair accessors (§6.1)
    "caaaar" "caaadr" "caaar" "caadar" "caaddr" "caadr"
    "caar" "cadaar" "cadadr" "cadar" "caddar" "cadddr"
    "caddr" "cadr" "cdaaar" "cdaadr" "cdaar" "cdadar"
    "cdaddr" "cdadr" "cdar" "cddaar" "cddadr" "cddar"
    "cdddar" "cddddr" "cdddr" "cddr"
    ;; `pushnew' -- a `defmethod' on `Vector<T>', not one of the macros above
    "pushnew"
    ;; the destructive `Vector<T>' operations (Phase 3d, §6.2)
    "nreverse" "nconc" "nreconc" "nbutlast" "nsubstitute" "nsubstitute-if"
    "delete" "delete-if" "delete-if-not" "delete-duplicates" "fill" "replace"
    "map-into" "set-contents" "rplaca" "rplacd"
    ;; Eq / Ord trait methods
    "equals" "not-equals" "less" "less-equal" "greater" "greater-equal"
    ;; the `Hash' trait and the hash-table methods that came with it (Phase 6a)
    "sxhash" "sxhash-string" "maphash" "size"
    ;; complex numbers (Phase 1d).  `complex' is both the type and CL's
    ;; constructor function, so it appears in both lists.
    "complex" "realpart" "imagpart" "conjugate" "phase" "cis" "atan2"
    ;; the arithmetic traits' methods (Phase 1a).  The operators themselves
    ;; are builtins; these are what a `where'-bounded type variable spells
    ;; them as.
    "add" "sub" "mul" "div" "remainder"
    "bit-and" "bit-or" "bit-xor" "bit-not" "shift"
    ;; `Array<T>' (Phase 6b).  `aref' is checker sugar rather than a
    ;; registered name, so it is here for highlighting only.
    "make" "aref" "rank" "dimension" "dimensions" "total-size"
    "in-bounds" "row-major-index" "row-major-get" "row-major-set"
    "adjust" "push-extend" "fill-pointer"
    ;; `BitVector' (Phase 6c).  `bit-and'/`bit-xor'/`bit-not' are already
    ;; above as the `Bits' trait's methods.
    "bit" "sbit" "set-bit" "set-sbit"
    "bit-ior" "bit-eqv" "bit-nand" "bit-nor"
    "bit-andc1" "bit-andc2" "bit-orc1" "bit-orc2"
    ;; errors and dynamic rebinding (Phase 7).  `assert'/`warn'/`dlet'/
    ;; `with-standard-io-syntax' are macros, not functions, but they read as
    ;; ordinary calls.
    "simple-error" "wrap-error" "describe-error"
    "assert" "warn" "dlet" "with-standard-io-syntax"
    ;; the one-object printers (Phase 8a).  Macros over `format', so they
    ;; read as ordinary calls too.
    "prin1" "princ" "write"
    "prin1-to-string" "princ-to-string" "write-to-string"
    ;; pretty-printer helpers callable inside `pprint-logical-block' (§15.1)
    "pprint-newline" "pprint-indent" "pprint-tab" "pprint-pop"
    "pprint-exit-if-list-exhausted" "pprint-list-exhausted"
    ;; the `print-object' trait method (§15.2)
    "print-object"
    ;; parsing & evaluation (§16)
    "parse-int" "parse-float" "read" "eval"
    ;; macro / system
    "gensym" "macroexpand" "macroexpand-1" "complement"
    "keywordp" "exit" "compile-file" "dump"
    ;; random-state (CLHS 12.1.6).  `make-random-state-fresh'/
    ;; `random-state-copy'/`random-state-next' are the native primitives
    ;; `random'/`make-random-state'/`random-state-p' are built on.
    "random-state-p" "make-random-state" "make-random-state-fresh"
    "random-state-copy" "random-state-next" "seed-random-state"
    ;; time (CLHS 25.1).  `get-internal-run-time' is CPU time, its sibling
    ;; elapsed time; `time' reports both.
    "time" "get-universal-time" "get-internal-real-time"
    "get-internal-run-time"
    "internal-time-units-per-second" "internal-time-seconds"
    ;; universal time, decomposed (CLHS 25.1).  The no-zone default is CL's
    ;; local time; the two `timezone-' primitives are what answers it.
    "decode-universal-time" "encode-universal-time" "get-decoded-time"
    "timezone-offset-seconds" "timezone-daylight-p"
    ;; filesystem queries that need no open stream (CLHS 20.1).  The `file-'
    ;; primitives these call are deliberately absent, like `file-exists-p'
    ;; before them: a user writes the `Pathish' wrapper, never the primitive.
    "truename" "file-write-date" "file-author" "directory" "directory-p"
    "ensure-directories-exist"
    ;; the environment and the running implementation (CLHS 25.1), plus the
    ;; two things CL has no equivalent of at all
    "command-line-args" "getenv" "home-directory" "user-homedir-pathname"
    "lisp-implementation-type" "lisp-implementation-version"
    "machine-type" "software-type"
    "machine-instance" "machine-version" "software-version"
    "short-site-name" "long-site-name"
    ;; asking the user a question (CLHS 25.2)
    "y-or-n-p" "yes-or-no-p"
    ;; the REPL tool layer's ordinary functions (CLHS 25.2).  `heap-info' is
    ;; what `room' prints and what a program reads the same numbers from;
    ;; `dribble-start'/`dribble-stop'/`ed-open' are the primitives the
    ;; `dribble' function and the `ed' special form reduce to.
    "room" "heap-info" "dribble" "dribble-start" "dribble-stop" "ed-open")
  "Builtin functions and methods from the standard catalog (docs/functions.md).")

(defconst typelisp-primitive-types
  '("i8" "i16" "i32" "u8" "u16" "u32"
    "f32" "f64" "bignum" "ratio" "random-state" "bool" "char" "string" "symbol"
    "ptr" "c-long" "c-ulong")
  "Primitive/scalar type names.
Includes the heap-boxed arbitrary-precision `bignum' / `ratio', which are
their own static types with no implicit conversion to or from the fixed-width
numerics (docs/syntax.md §2), and the opaque mutable `random-state' PRNG
stream (CLHS 12.1.6).

`ptr' / `c-long' / `c-ulong' are the C-boundary words: only writable inside
`unsafe', and only as an argument, a return type or a local (docs/syntax.md
§3).")

(defconst typelisp-builtin-types
  '(;; stream traits and concrete stream types (§18)
    "broadcast-stream" "charinput" "charoutput" "concatenated-stream"
    "binary-file-stream" "byteinput" "byteoutput"
    "echo-stream" "fileerror" "inputstream" "outputstream" "peekinput"
    "peek-stream" "standard-stream" "stream" "string-input-stream"
    "string-output-stream" "two-way-stream"
    ;; pathnames (§19): the type and its designator trait
    "pathname" "pathish"
    ;; builtin generic/abstract types
    "Option" "Result" "Sexpr" "HashTable" "Vector" "Self"
    ;; the handle `go' hands back, and the channel tasks talk over
    "Task" "Chan"
    ;; builtin concrete error types, one per fallible builtin (§7.1).  `Error'
    ;; itself is *not* a type -- it is the prelude trait these implement, used
    ;; as `:dyn Error'.
    "ParseIntError" "ParseFloatError" "ReadError" "ReadOutcome" "EvalError"
    ;; builtin generic pair & iterator types (lowercase)
    "cons-cell" "vector-iter" "hashtable-iter" "array-iter"
    ;; complex numbers (Phase 1d) -- a prelude `defstruct', not a builtin
    "complex"
    ;; the multi-dimensional array and the bit vector (Phase 6b/6c) -- prelude
    ;; `defstruct's over `Vector', not builtins
    "Array" "BitVector"
    ;; the general-purpose error types (Phase 7a)
    "SimpleError" "WrappedError"
    ;; the struct `decode-universal-time' answers with, standing in for CL's
    ;; nine return values
    "decoded-time"
    ;; the structs the two clocks answer with.  They are structs rather than
    ;; a single integer because there is no 64-bit-wide integer type to hold
    ;; the count (2026-09-01): `universal-time' splits it into whole days and
    ;; the second within the day, `internal-time' into seconds and microseconds.
    "universal-time" "internal-time"
    ;; builtin traits
    "Iter" "Eq" "Ord" "Error" "Hash"
    ;; the arithmetic traits (Phase 1a)
    "Add" "Sub" "Mul" "Div" "Rem" "Bits" "Number")
  "Builtin generic/abstract type names and traits.")

(defconst typelisp-constants
  '("true" "false" "pi"
    ;; `boole`'s 16 op-code constants (`i32`, not keywords -- this language
    ;; has no keyword-symbol type for CL's `boole-and` etc to be)
    "boole-clr" "boole-set" "boole-1" "boole-2" "boole-c1" "boole-c2"
    "boole-and" "boole-ior" "boole-xor" "boole-eqv" "boole-nand" "boole-nor"
    "boole-andc1" "boole-andc2" "boole-orc1" "boole-orc2"
    ;; CL's numeric limit constants (cl-parity-plan.md Phase 1c, §4.3)
    "most-positive-fixnum" "most-negative-fixnum"
    "most-positive-double-float" "most-negative-double-float"
    "least-positive-double-float" "least-negative-double-float"
    "least-positive-normalized-double-float" "least-negative-normalized-double-float"
    "double-float-epsilon" "double-float-negative-epsilon")
  "Literal constants.")

;;; Font lock ---------------------------------------------------------------

(defconst typelisp--symbol-rx "\\(?:\\sw\\|\\s_\\)+"
  "Regexp matching a typelisp symbol token (word/symbol constituents).")

(defconst typelisp--pub-rx "(\\(?:pub[ \t\n]+\\)?"
  "Regexp matching an opening paren, optionally followed by `pub '.
Visibility is spelled flat -- `(pub defun f ...)', not `(pub (defun f ...))'
-- so every definition-form rule has to allow the marker between the paren
and the form's own head (docs/syntax.md §3).")

(defconst typelisp--number-rx
  (concat "\\_<[-+]?\\(?:"
          "0[xX][0-9a-fA-F]+"           ; hex integer: 0xff
          "\\|[0-9]+/[0-9]+"            ; ratio: 1/3 (always reduced by the reader)
          "\\|[0-9]+\\.[0-9]+\\(?:[eE][-+]?[0-9]+\\)?" ; float: 1.5, 3.0e10
          "\\|[0-9]+[eE][-+]?[0-9]+"    ; float in exponent-only form: 1e5
          "\\|[0-9]+"                   ; decimal integer (bignum if too wide)
          "\\)\\_>")
  "Regexp matching a numeric literal (docs/syntax.md §1).")

(defconst typelisp--format-directive-rx
  (concat "~"
          ;; prefix parameters and modifiers: 3, 'c, v, #, comma-separated,
          ;; plus the `:' / `@' modifiers
          "\\(?:'.\\|[-+0-9,vV#:@]\\)*"
          ;; The directive character itself (`~<newline>' ignores whitespace).
          ;; `/' is deliberately absent: `~/name/' function-call directives are
          ;; rejected by the formatter, so leaving one unhighlighted is a hint
          ;; rather than a gap.
          "\\(?:[][a-zA-Z%&|~$_^<>{}();*?]\\|\n\\)")
  "Regexp matching a `format' control-string directive (docs/functions.md §15).")

;;; Types this buffer defines -------------------------------------------------

;; A `defstruct'/`defenum'/`deftrait' name is usually lowercase (`rect',
;; `todo-item', `board'), so the Capitalized-name rule below cannot reach its
;; *uses* -- only the definition site, which its own rule already covers. That
;; left the odd result that in a statically-typed language the type annotations
;; were the one thing not coloured. These two functions close that: the buffer's
;; own type names are collected on demand and matched wherever they appear.
;;
;; This is the *fallback*, used when no language server is answering
;; `textDocument/semanticTokens' for the buffer (see the semantic-tokens
;; section below).  It is buffer-local and textual, so it has two limits the
;; server does not: a type imported through `use' is invisible to it, and a
;; function sharing a type's name is indistinguishable from the type.  When the
;; server is connected these rules stand down entirely rather than competing.

(defvar-local typelisp--local-types nil
  "Cache of `(TICK . REGEXP)' for the types this buffer defines.
TICK is the `buffer-chars-modified-tick' the regexp was built at; REGEXP is nil
when the buffer defines no types.")

(defconst typelisp--type-adjacent "-A-Za-z0-9_?!*>=/+.%&^~:"
  "Characters that must not precede a type name for it to be one.
`<' and `,' are absent on purpose: a type does appear directly after them, as
the argument of a generic (`Vector<lexpr>', `HashTable<i32,todo-item>'). `>' is
present, which is what stops the `bignum' in `int->bignum' reading as a type.")

(defun typelisp--scan-local-types ()
  "Regexp matching the types defined in this buffer, or nil if there are none."
  (let (names)
    (save-excursion
      (goto-char (point-min))
      (while (re-search-forward
              (concat typelisp--pub-rx
                      (regexp-opt typelisp-type-definition-forms t)
                      "\\_>[ \t\n]*\\(" typelisp--symbol-rx "\\)")
              nil t)
        ;; A generic header is a single token, so `point<T>' defines `point'.
        (push (car (split-string (match-string-no-properties 2) "<")) names)))
    (when names
      (concat
       ;; Either a symbol boundary, or a `<' consumed so that a type used as a
       ;; generic argument still matches -- `<' is a symbol constituent here, so
       ;; `\\_<' alone would not fire inside `Vector<lexpr>'.
       "\\(?:\\_<\\|<\\)\\(" (regexp-opt (delete-dups names)) "\\)"
       ;; And a boundary, or one of the characters a type name may butt against:
       ;; `<' opens its generic arguments, `:' starts `::method', `>' closes an
       ;; enclosing generic. Requiring one of these is what keeps `rect' from
       ;; matching inside `rectangle'.
       "\\(?:\\_>\\|[<:>]\\)"))))

(defun typelisp--local-type-regexp ()
  "The cached regexp from `typelisp--scan-local-types', rebuilt when stale."
  (let ((tick (buffer-chars-modified-tick)))
    (unless (eq (car typelisp--local-types) tick)
      (setq typelisp--local-types (cons tick (typelisp--scan-local-types))))
    (cdr typelisp--local-types)))

(defun typelisp--match-local-type (limit)
  "Font-lock matcher for a use of a type this buffer defines, before LIMIT.
Stands down when a language server is colouring the buffer instead: the
server's answer is resolution-driven and strictly better (see
`typelisp-semantic-tokens-mode')."
  (unless (typelisp--server-highlights-types-p)
    (let ((regexp (typelisp--local-type-regexp)))
      (and regexp (re-search-forward regexp limit t)))))

;;; Semantic tokens (types resolved by the language server) -------------------

;; `typl-lsp' answers `textDocument/semanticTokens/full' with every position
;; where the *checker* resolved a user-defined type or trait name.  That is
;; strictly more than the buffer scan above can produce -- it sees types
;; imported through `use', and it never mistakes a same-named function for a
;; type, because a token exists only where the type grammar actually ran.
;;
;; `lsp-mode' consumes that natively (`lsp-semantic-tokens-enable'), so this
;; code is for `eglot', which does not implement semantic tokens at all --
;; Emacs 29/30's `eglot.el' contains no code for the request.  Rather than
;; leave eglot users on the weaker fallback, the mode issues the request itself
;; over eglot's own JSON-RPC connection and draws the result with overlays.
;; Overlays (not text properties) because they survive font-lock's
;; refontification and move with the text on edit.

(defcustom typelisp-semantic-tokens t
  "Whether to colour type names using the language server's semantic tokens.
Only takes effect in a buffer managed by `eglot'; `lsp-mode' has its own
implementation and this one stands aside for it."
  :type 'boolean
  :group 'typelisp)

(defcustom typelisp-semantic-tokens-idle-delay 0.6
  "Seconds of idle time before re-requesting semantic tokens after an edit.
Should stay above `eglot-send-changes-idle-time' so the server has been told
about the edit before it is asked to describe the result."
  :type 'number
  :group 'typelisp)

(defconst typelisp-semantic-token-faces
  '(("struct" . font-lock-type-face)
    ("enum" . font-lock-type-face)
    ("interface" . font-lock-type-face))
  "Face for each token type in the server's legend.
The three names are LSP standard token types, chosen by the server precisely
so no typelisp-specific theme support is needed; a `deftrait' maps onto
`interface'.  Kept as an alist keyed by the legend *name* rather than by
index, so a change to the server's legend order cannot silently mis-colour.")

(defvar-local typelisp--semantic-overlays nil
  "Overlays currently drawing server-reported type names in this buffer.")

(defvar-local typelisp--semantic-timer nil
  "Idle timer that will refresh this buffer's semantic tokens, if any.")

(defvar-local typelisp--semantic-active nil
  "Non-nil once the server has answered semantic tokens for this buffer.
Read by `typelisp--server-highlights-types-p' to retire the buffer-local
fallback rules, so the two never paint the same buffer.")

(defun typelisp--server-highlights-types-p ()
  "Whether a language server is colouring type names in this buffer.
True for `lsp-mode' with semantic tokens enabled, and for this mode's own
eglot client once it has received an answer."
  (or typelisp--semantic-active
      (and (bound-and-true-p lsp-mode)
           (bound-and-true-p lsp-semantic-tokens-enable))))

(defun typelisp--eglot-server ()
  "The eglot server managing this buffer, or nil.
Nil also when `lsp-mode' is in charge, which owns semantic tokens itself."
  (and typelisp-semantic-tokens
       (not (bound-and-true-p lsp-mode))
       (fboundp 'eglot-current-server)
       (eglot-current-server)))

(defun typelisp--semantic-legend (server)
  "The token-type legend SERVER advertised, as a list of strings.
Read from the server's own `initialize' response so this client never has to
assume an order; nil when the capability is absent or unreadable, in which
case no tokens are drawn (guessing a legend would mis-colour silently)."
  (let ((caps (cond ((fboundp 'eglot--capabilities) (eglot--capabilities server))
                    ((fboundp 'eglot-server-capabilities) (eglot-server-capabilities server)))))
    (let* ((provider (plist-get caps :semanticTokensProvider))
           (legend (plist-get provider :legend))
           (types (plist-get legend :tokenTypes)))
      (and types (append types nil)))))

(defun typelisp--semantic-uri ()
  "This buffer's file as an LSP URI, or nil for a buffer with no file."
  (when buffer-file-name
    (cond ((fboundp 'eglot-path-to-uri) (eglot-path-to-uri buffer-file-name))
          ((fboundp 'eglot--path-to-uri) (eglot--path-to-uri buffer-file-name))
          (t (concat "file://" (expand-file-name buffer-file-name))))))

(defun typelisp--semantic-clear ()
  "Remove every overlay this buffer's semantic tokens created."
  (mapc #'delete-overlay typelisp--semantic-overlays)
  (setq typelisp--semantic-overlays nil))

(defun typelisp--semantic-position (line character)
  "Buffer position of 0-based LINE and CHARACTER.
CHARACTER is treated as a count of characters rather than UTF-16 code units,
matching how `typl-lsp' itself reads the field -- the two agree for everything
outside the astral planes."
  (save-excursion
    (goto-char (point-min))
    (forward-line line)
    ;; Clamped to the line's end: a token computed from text the server saw
    ;; can outrun a line the user has since shortened, and a position past the
    ;; newline would put the overlay on the following line.
    (min (+ (point) character) (line-end-position))))

(defun typelisp--semantic-apply (data legend)
  "Draw the tokens in DATA, decoded against LEGEND.
DATA is the LSP wire format: five integers per token, with the line and (within
a line) the column stored as deltas from the previous token."
  (typelisp--semantic-clear)
  (let ((was-active typelisp--semantic-active)
        (line 0) (col 0) (i 0) (n (length data)))
    (while (<= (+ i 5) n)
      (let ((dl (aref data i))
            (dc (aref data (+ i 1)))
            (len (aref data (+ i 2)))
            (type (aref data (+ i 3))))
        (setq line (+ line dl))
        (setq col (if (zerop dl) (+ col dc) dc))
        (let* ((name (nth type legend))
               (face (cdr (assoc name typelisp-semantic-token-faces))))
          (when face
            (let* ((start (typelisp--semantic-position line col))
                   (end (min (+ start len) (point-max)))
                   (ov (make-overlay start end nil t nil)))
              (overlay-put ov 'face face)
              (overlay-put ov 'typelisp-semantic t)
              (push ov typelisp--semantic-overlays))))
        (setq i (+ i 5))))
    (setq typelisp--semantic-active t)
    ;; Only on the first answer: that is when the fallback rules retire (see
    ;; `typelisp--server-highlights-types-p'), so the buffer has to be
    ;; repainted to drop whatever colour they had put on a non-type.  Doing it
    ;; on every refresh would refontify the whole buffer after each edit for
    ;; no change in what font-lock produces -- the overlays carry the tokens.
    (unless was-active
      (font-lock-flush))))

(defun typelisp--semantic-refresh ()
  "Ask the server for this buffer's semantic tokens and draw the answer."
  (let ((server (typelisp--eglot-server))
        (uri (typelisp--semantic-uri)))
    (when (and server uri)
      (let ((legend (typelisp--semantic-legend server))
            (buffer (current-buffer)))
        (when legend
          ;; Flush pending edits first: eglot batches `didChange' on its own
          ;; idle timer, and tokens computed from text the server has not seen
          ;; would be placed at the wrong offsets.
          (when (fboundp 'eglot--signal-textDocument/didChange)
            (eglot--signal-textDocument/didChange))
          (jsonrpc-async-request
           server :textDocument/semanticTokens/full
           (list :textDocument (list :uri uri))
           :success-fn
           (lambda (result)
             (when (buffer-live-p buffer)
               (with-current-buffer buffer
                 (let ((data (plist-get result :data)))
                   (when (vectorp data)
                     (typelisp--semantic-apply data legend))))))
           ;; A server that cannot answer (older build, request in flight
           ;; during shutdown) simply leaves the buffer on the fallback rules.
           :error-fn #'ignore
           :timeout-fn #'ignore))))))

(defun typelisp--semantic-schedule (&rest _)
  "Queue a semantic-tokens refresh after `typelisp-semantic-tokens-idle-delay'."
  (when (timerp typelisp--semantic-timer)
    (cancel-timer typelisp--semantic-timer))
  (setq typelisp--semantic-timer
        (run-with-idle-timer typelisp-semantic-tokens-idle-delay nil
                             #'typelisp--semantic-refresh-buffer
                             (current-buffer))))

(defun typelisp--semantic-refresh-buffer (buffer)
  "Refresh semantic tokens in BUFFER, if it is still alive."
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (setq typelisp--semantic-timer nil)
      (typelisp--semantic-refresh))))

(define-minor-mode typelisp-semantic-tokens-mode
  "Colour type names from the language server's semantic tokens.
Enabled automatically in a `typelisp-mode' buffer that eglot is managing; see
`typelisp-semantic-tokens' to turn it off."
  :lighter nil
  (if typelisp-semantic-tokens-mode
      (progn
        (add-hook 'after-change-functions #'typelisp--semantic-schedule nil t)
        (typelisp--semantic-refresh))
    (remove-hook 'after-change-functions #'typelisp--semantic-schedule t)
    (when (timerp typelisp--semantic-timer)
      (cancel-timer typelisp--semantic-timer)
      (setq typelisp--semantic-timer nil))
    (typelisp--semantic-clear)
    (setq typelisp--semantic-active nil)
    (font-lock-flush)))

(defun typelisp--maybe-enable-semantic-tokens ()
  "Turn `typelisp-semantic-tokens-mode' on or off to follow eglot.
Hung on `eglot-managed-mode-hook', which runs on both connect and disconnect."
  (when (derived-mode-p 'typelisp-mode)
    (typelisp-semantic-tokens-mode
     (if (and typelisp-semantic-tokens (bound-and-true-p eglot--managed-mode)) 1 -1))))

(add-hook 'eglot-managed-mode-hook #'typelisp--maybe-enable-semantic-tokens)

(defun typelisp--match-format-directive (limit)
  "Move point to the next `format' directive inside a string, before LIMIT.
A font-lock matcher function: the directive syntax is only meaningful inside
the control string of `format' / `print' / `println', and a string is the one
place a plain regexp rule cannot reach on its own, since the syntactic pass
has already claimed it for `font-lock-string-face'."
  (let (found)
    (while (and (not found)
                (re-search-forward typelisp--format-directive-rx limit t))
      ;; `nth 3' of the parse state is non-nil inside a string.
      (when (nth 3 (syntax-ppss (match-beginning 0)))
        (setq found t)))
    found))

(defvar typelisp-font-lock-keywords
  `(
    ;; Character literals: #\a  #\Space  #\Newline  (claimed first so that a
    ;; capitalized name like `Space' is not mistaken for a type).
    ("#\\\\\\(?:[A-Za-z][A-Za-z0-9]*\\|.\\)" . font-lock-constant-face)

    ;; `format' directives inside control strings.  `prepend' rather than the
    ;; default nil override, because the string face is already in place.
    (typelisp--match-format-directive 0 font-lock-constant-face prepend)

    ;; `:dyn Trait' -- the trait-object type.  Claimed before the general
    ;; keyword rule below (which would otherwise colour it as a constant):
    ;; `:dyn' is a reserved keyword valid only in a type position, and
    ;; highlighting it distinctly is the reason the language spells trait
    ;; objects this way at all.  The trait name after it gets the type face.
    ;; Both halves need an explicit character set rather than `\\_<' / the
    ;; general symbol regexp: `<' and `>' are symbol constituents here (a
    ;; generic type is read as one token), so inside `Vector<:dyn Drawable>'
    ;; there is no symbol boundary before `:dyn', and `Drawable>' would
    ;; otherwise be captured with the closing bracket attached.
    ("\\(?:^\\|[^:[:alnum:]_-]\\)\\(:dyn\\)\\_>[ \t\n]*\\([A-Za-z][A-Za-z0-9_-]*\\)?"
     (1 font-lock-keyword-face)
     (2 font-lock-type-face nil t))

    ;; Keywords: :name (CL-style self-evaluating symbols).  Not `::foo',
    ;; which is the absolute-path syntax.
    ("\\(?:^\\|[^:[:alnum:]_-]\\)\\(:[A-Za-z][A-Za-z0-9_?!*<>=/+-]*\\)"
     1 font-lock-constant-face)

    ;; Definition forms binding a function name:  (defun NAME ...)
    ;; and the `pub'-prefixed form  (pub defun NAME ...).
    (,(concat typelisp--pub-rx (regexp-opt typelisp-definition-forms t)
              "\\_>[ \t\n]*(?[ \t\n]*\\(" typelisp--symbol-rx "\\)?")
     (1 font-lock-keyword-face)
     (2 font-lock-function-name-face nil t))

    ;; Definition forms binding a type/trait name:  (defstruct NAME ...).
    ;; The generic header is one token (`point<T>'), so it is captured whole.
    (,(concat typelisp--pub-rx (regexp-opt typelisp-type-definition-forms t)
              "\\_>[ \t\n]*(?[ \t\n]*\\(" typelisp--symbol-rx "\\)?")
     (1 font-lock-keyword-face)
     (2 font-lock-type-face nil t))

    ;; Variable definitions:  (defvar (NAME Type) ...)
    (,(concat typelisp--pub-rx (regexp-opt typelisp-variable-definition-forms t)
              "\\_>[ \t\n]*(?[ \t\n]*\\(" typelisp--symbol-rx "\\)?")
     (1 font-lock-keyword-face)
     (2 font-lock-variable-name-face nil t))

    ;; `(impl Trait Type ...)': both names denote types (traits and types share
    ;; one namespace), and neither is necessarily capitalized -- the prelude's
    ;; own `print-object' trait is not -- so the general Capitalized-name rule
    ;; below does not reach them.  Placed ahead of the builtin-function rule so
    ;; that a trait sharing a name with its method (`print-object' again) reads
    ;; as the type it is in this position.
    (,(concat "(impl\\(?:<[^>]*>\\)?[ \t\n]+\\(" typelisp--symbol-rx "\\)"
              "\\(?:[ \t\n]+\\(" typelisp--symbol-rx "\\)\\)?")
     (1 font-lock-type-face)
     (2 font-lock-type-face nil t))

    ;; Namespace / declaration keywords
    (,(regexp-opt typelisp-declaration-keywords 'symbols)
     . font-lock-keyword-face)

    ;; Lambda-list markers.  `font-lock-type-face' to match how `lisp-mode'
    ;; fontifies `&rest'/`&optional' in Emacs Lisp buffers.
    (,(regexp-opt typelisp-lambda-list-keywords 'symbols)
     . font-lock-type-face)

    ;; Special forms
    (,(regexp-opt typelisp-special-forms 'symbols)
     . font-lock-keyword-face)

    ;; `cond'/`case' clause markers
    (,(regexp-opt typelisp-clause-keywords 'symbols)
     . font-lock-keyword-face)

    ;; Builtin functions
    (,(regexp-opt typelisp-builtin-functions 'symbols)
     . font-lock-builtin-face)

    ;; Primitive & builtin types
    (,(regexp-opt (append typelisp-primitive-types typelisp-builtin-types)
                  'symbols)
     . font-lock-type-face)

    ;; The Never type `!' used as a type annotation
    ("\\_<!\\_>" . font-lock-type-face)

    ;; Uses of a type this buffer defines, which is usually lowercase and so
    ;; invisible to the Capitalized rule below.  After the builtin rules, so a
    ;; user type sharing a builtin's name does not steal it.
    (typelisp--match-local-type 1 font-lock-type-face)

    ;; User-defined types: CapitalizedName, optionally generic <...>
    ("\\_<\\([A-Z][A-Za-z0-9_]*\\)" 1 font-lock-type-face)

    ;; Constants
    (,(regexp-opt typelisp-constants 'symbols)
     . font-lock-constant-face)

    ;; Numeric literals: decimal/hex integers, floats, ratios
    (,typelisp--number-rx . font-lock-constant-face)

    ;; Earmuffed globals: the CL convention for a special variable, which the
    ;; printer-control variables (`*print-pretty*', `*print-right-margin*',
    ;; `*print-miser-width*') follow -- they are ordinary assignable globals
    ;; here, so a user's own earmuffed `defvar' matches too.
    ("\\_<\\(\\*[A-Za-z0-9?!*<>=/+-]+\\*\\)\\_>" 1 font-lock-variable-name-face)
    )
  "Font-lock rules for `typelisp-mode'.")

;;; Syntax table ------------------------------------------------------------

(defvar typelisp-mode-syntax-table
  (let ((table (make-syntax-table lisp-mode-syntax-table)))
    ;; `;' starts a line comment through end of line.
    (modify-syntax-entry ?\; "<" table)
    (modify-syntax-entry ?\n ">" table)
    ;; Nestable block comments `#| ... |#'.
    ;;   `#' : expression prefix, and first char of `#|' / second char of `|#'.
    ;;   `|' : punctuation, and second char of `#|' / first char of `|#'.
    (modify-syntax-entry ?\# "' 14bn" table)
    (modify-syntax-entry ?\| ". 23bn" table)
    ;; Operator/identifier constituents used in typelisp symbols.
    (dolist (ch '(?- ?+ ?* ?/ ?< ?> ?= ?! ?? ?_ ?. ?% ?& ?^ ?~))
      (modify-syntax-entry ch "_" table))
    ;; `"' delimits strings.
    (modify-syntax-entry ?\" "\"" table)
    ;; `\' is the string escape character.
    (modify-syntax-entry ?\\ "\\" table)
    table)
  "Syntax table for `typelisp-mode'.")

;;; Indentation -------------------------------------------------------------

(defconst typelisp-indent-specs
  ;; A `defun' spec means "everything up to the end of the first line is
  ;; header; indent the body by `lisp-body-indent'".  An integer N means
  ;; "N distinguished arguments, then body".  See `lisp-indent-function'.
  '(;; Definition forms.  All of them take `defun' rather than an integer,
    ;; because a typelisp header is *longer* than the Emacs Lisp shape the
    ;; integers were tuned for -- `(defun NAME (PARAMS) RETTYPE ...)' has three
    ;; header elements, `(defmethod ...)' likewise, and `impl' has two -- while
    ;; `defun'-style indentation is defined by where the *line* ends and so
    ;; does not care how many elements precede the body.
    ("defun"       . defun)
    ("defmethod"   . defun)
    ("defmacro"    . defun)
    ("defstruct"   . defun)
    ("defenum"     . defun)
    ("deftrait"    . defun)
    ("deftype"     . defun)
    ("defvar"      . defun)
    ("defconstant" . defun)
    ("module"      . defun)
    ("impl"        . defun)
    ;; `pub' prefixes a whole definition flatly -- `(pub defun f ...)' -- so
    ;; it needs the same rule, or the body would align under `defun'.
    ("pub"         . defun)
    ;; `lambda' also carries a return type: `(lambda (PARAMS) RETTYPE ...)'.
    ("lambda"      . defun)
    ;; Binding and conditionals.
    ("let"         . 1)
    ("let*"        . 1)
    ("labels"      . 1)
    ("macrolet"    . 1)
    ("symbol-macrolet" . 1)
    ;; `if' takes exactly three elements -- the `else' branch is mandatory --
    ;; so all three are distinguished and the two branches line up with each
    ;; other, the Common Lisp style the examples are written in.  (Emacs Lisp's
    ;; spec of 2 instead drops `else' to the body indent, which reads as an
    ;; asymmetry between branches that cannot arise here.)
    ("if"          . 3)
    ("if-let"      . 3)
    ("when"        . 1)
    ("unless"      . 1)
    ("cond"        . 0)
    ("case"        . 1)
    ("and"         . 0)
    ("or"          . 0)
    ("progn"       . 0)
    ("unsafe"      . 0)
    ("the"         . 1)
    ("match"       . 1)
    ("while-let"   . 1)
    ;; Iteration.
    ("loop"        . 0)
    ("while"       . 1)
    ("until"       . 1)
    ("dotimes"     . 1)
    ("dolist"      . 1)
    ("doiter"      . 1)
    ("do"          . 2)
    ;; Non-local exit: the tag / the protected form is the distinguished head,
    ;; the rest is the body.
    ("catch"          . 1)
    ("unwind-protect" . 1)
    ;; Pretty printer.
    ("pprint-logical-block" . 1))
  "Indent specs for typelisp forms, keyed by the form's head as written.

Consulted by `typelisp-indent-function'.  Keeping the table here rather than
in `lisp-indent-function' symbol properties is deliberate: a property is
global, so putting typelisp's shapes on `defun' / `let' / `if' would change
how every other Lisp buffer in the session indents.")

;; Bound by `calculate-lisp-indent' around the call to `lisp-indent-function'.
(defvar calculate-lisp-indent-last-sexp)

(defun typelisp--head-at (pos)
  "Return the head symbol of the form whose open paren is at POS, downcased.
Nil if POS is nil or the form's first element is not a symbol (a binding list,
say, whose first element is itself a list)."
  (when pos
    (save-excursion
      (goto-char (1+ pos))
      (skip-chars-forward " \t\n")
      (when (looking-at "\\(?:\\sw\\|\\s_\\)+")
        ;; `impl<T>' lexes as one symbol (`<'/`>' are symbol constituents),
        ;; but the head for indentation purposes is `impl' -- the type
        ;; parameters no more change how the form indents than a `defun''s do.
        (replace-regexp-in-string
         "<[^>]*>\\'" "" (downcase (match-string-no-properties 0)))))))

(defun typelisp--local-defform-body-p (state)
  "Non-nil when STATE puts point in the body of a *local* definition.
Two forms nest a `(NAME (PARAMS) RETTYPE BODY...)' definition inside another
form instead of at top level: an `impl' / `deftrait' member, and a `labels'
local function -- which sits one level deeper still, inside the binding list.
Their heads are user-chosen names with no entry in `typelisp-indent-specs', so
without this check their bodies would align under the parameter list rather
than indent like any other definition body.

The `labels' case is why the binding list is matched by its *grandparent*: a
`let' binding has the very same shape one level down, and its value expression
should keep aligning under the bound name."
  (let* ((opens (reverse (nth 9 state)))  ; innermost containing form first
         (parent (nth 1 opens))
         (grandparent (nth 2 opens)))
    (or (member (typelisp--head-at parent) '("impl" "deftrait"))
        (and parent
             (null (typelisp--head-at parent))
             (equal (typelisp--head-at grandparent) "labels")))))

(defun typelisp-indent-function (indent-point state)
  "Compute indentation for the typelisp line beginning at INDENT-POINT.
STATE is the `parse-partial-sexp' state there.  A drop-in replacement for
`lisp-indent-function' that resolves the enclosing form's spec through
`typelisp-indent-specs' instead of symbol properties; returns nil (meaning
\"nothing special, align with the previous argument\") for any other head."
  (let ((normal-indent (current-column)))
    (goto-char (1+ (elt state 1)))
    (parse-partial-sexp (point) calculate-lisp-indent-last-sexp 0 t)
    (if (and (elt state 2) (not (looking-at "\\sw\\|\\s_")))
        ;; The head of the form is not a symbol (e.g. a list): indent under the
        ;; first sexp on the line the last complete sexp started on.  Verbatim
        ;; from `lisp-indent-function', which has no seam to reuse here.
        (progn
          (unless (> (save-excursion (forward-line 1) (point))
                     calculate-lisp-indent-last-sexp)
            (goto-char calculate-lisp-indent-last-sexp)
            (beginning-of-line)
            (parse-partial-sexp (point) calculate-lisp-indent-last-sexp 0 t))
          (backward-prefix-chars)
          (current-column))
      (let* ((head (downcase (buffer-substring-no-properties
                              (point) (progn (forward-sexp 1) (point)))))
             (spec (cdr (assoc head typelisp-indent-specs))))
        ;; An unlisted `def...' head is treated as a definition form, so that a
        ;; user's own `(defmacro defthing ...)' indents like the built-in ones.
        ;; Same heuristic `lisp-indent-function' applies.
        (when (and (null spec)
                   (or (and (> (length head) 3) (string-prefix-p "def" head))
                       (typelisp--local-defform-body-p state)))
          (setq spec 'defun))
        (cond ((eq spec 'defun) (lisp-indent-defform state indent-point))
              ((integerp spec)
               (lisp-indent-specform spec state indent-point normal-indent)))))))

;;; Imenu -------------------------------------------------------------------

(defconst typelisp-imenu-generic-expression
  `(("Functions"
     ,(concat typelisp--pub-rx "defun\\_>[ \t\n]*\\(" typelisp--symbol-rx "\\)") 1)
    ("Methods"
     ,(concat typelisp--pub-rx "defmethod\\_>[ \t\n]*\\(" typelisp--symbol-rx "\\)") 1)
    ("Macros"
     ,(concat typelisp--pub-rx "defmacro\\_>[ \t\n]*\\(" typelisp--symbol-rx "\\)") 1)
    ("Types"
     ,(concat typelisp--pub-rx "def\\(?:struct\\|enum\\)\\_>[ \t\n]*\\("
              typelisp--symbol-rx "\\)")
     1)
    ("Traits"
     ,(concat typelisp--pub-rx "deftrait\\_>[ \t\n]*\\(" typelisp--symbol-rx "\\)") 1)
    ("Impls"
     ,(concat "(impl\\(?:<[^>]*>\\)?[ \t\n]+\\(" typelisp--symbol-rx "[ \t\n]+"
              typelisp--symbol-rx "\\)")
     1)
    ("Variables"
     ,(concat typelisp--pub-rx "def\\(?:var\\|constant\\)\\_>[ \t\n]*(?[ \t\n]*\\("
              typelisp--symbol-rx "\\)")
     1)
    ("Modules"
     ,(concat "(module\\_>[ \t\n]*\\(" typelisp--symbol-rx "\\)") 1))
  "`imenu-generic-expression' for `typelisp-mode'.
The generic-expression form (rather than a custom `imenu-create-index-function')
is enough here because every definition form names its subject in a fixed
position right after the head, modulo the optional `pub' marker.  Generic
headers come along for free: `<' and `>' are symbol constituents in
`typelisp-mode-syntax-table', so `point<T>' is matched whole.")

;;; Running -----------------------------------------------------------------

(defcustom typelisp-program "typl"
  "Name of (or path to) the typelisp CLI executable.
Invoked as `typl FILE' to run a source file, and `typl compile-module FILE'
to precompile one to a fasl."
  :type 'string
  :group 'typelisp)

(defconst typelisp-compilation-error-regexp
  ;; Diagnostics are printed as `error: FILE:LINE:COL: message' -- one format
  ;; for type errors and run-time panics alike, since both carry a source
  ;; location (see `typelisp::errors').  Warnings without a location (e.g.
  ;; `warning: redefining function `f'') simply do not match, which is what we
  ;; want: there is nothing to jump to.
  (list 'typelisp
        (concat "^\\(?:error\\|\\(warning\\)\\): "
                "\\([^:\n]+\\):\\([0-9]+\\):\\([0-9]+\\): ")
        2 3 4 '(1))
  "A `compilation-error-regexp-alist-alist' entry for typelisp diagnostics.")

(defun typelisp--setup-compilation ()
  "Teach `compile' how to parse typelisp diagnostics."
  (add-to-list 'compilation-error-regexp-alist-alist
               typelisp-compilation-error-regexp)
  (add-to-list 'compilation-error-regexp-alist 'typelisp))

(defun typelisp--run (args)
  "Run `typelisp-program' with ARGS on the current buffer's file via `compile'.
Saves the buffer first: the CLI reads from disk, so running an unsaved buffer
would silently check the previous revision."
  (let ((file (buffer-file-name)))
    (unless file
      (user-error "Buffer is not visiting a file"))
    (save-buffer)
    (typelisp--setup-compilation)
    (compile (mapconcat #'shell-quote-argument
                        (append (list typelisp-program) args (list file))
                        " "))))

(defun typelisp-run-buffer ()
  "Run the current buffer's file with `typl FILE'.
Its `use' dependencies are resolved through the project's `typelisp.toml'
(docs/syntax.md, \"ファイル↔モジュール対応\")."
  (interactive)
  (typelisp--run nil))

(defun typelisp-compile-module ()
  "Precompile the current buffer's file to a fasl with `typl compile-module'.
Only definitions are captured -- a top-level expression in the file is an
error, since a fasl is a module rather than a script."
  (interactive)
  (typelisp--run '("compile-module")))

(defun typelisp-repl ()
  "Start the typelisp REPL (`typl' with no file argument) in a comint buffer."
  (interactive)
  (pop-to-buffer
   (make-comint-in-buffer "typelisp-repl" "*typelisp-repl*" typelisp-program)))

;;; Mode definition ---------------------------------------------------------

(defvar typelisp-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c C-c") #'typelisp-run-buffer)
    (define-key map (kbd "C-c C-k") #'typelisp-compile-module)
    (define-key map (kbd "C-c C-z") #'typelisp-repl)
    map)
  "Keymap for `typelisp-mode'.")

;;;###autoload
(define-derived-mode typelisp-mode prog-mode "typelisp"
  "Major mode for editing typelisp source code.

\\{typelisp-mode-map}"
  :syntax-table typelisp-mode-syntax-table
  (setq-local comment-start ";")
  (setq-local comment-start-skip ";+[ \t]*")
  (setq-local comment-add 1)
  (setq-local comment-end "")
  (setq-local block-comment-start "#|")
  (setq-local block-comment-end "|#")
  (setq-local parse-sexp-ignore-comments t)
  (setq-local indent-line-function 'lisp-indent-line)
  (setq-local lisp-indent-function 'typelisp-indent-function)
  ;; Lisp indents with spaces; the fixed two-column steps a `defun' spec
  ;; produces would otherwise be written out as tabs.
  (setq-local indent-tabs-mode nil)
  (setq-local imenu-generic-expression typelisp-imenu-generic-expression)
  (setq-local font-lock-defaults
              '(typelisp-font-lock-keywords
                nil                     ; keywords-only
                nil                     ; case-fold: keep case-sensitive so that
                                        ; Capitalized type names highlight distinctly
                nil                     ; syntax-alist
                nil))                   ; other vars
  (typelisp--setup-compilation))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.typl\\'" . typelisp-mode))

(provide 'typelisp-mode)

;;; typelisp-mode.el ends here
