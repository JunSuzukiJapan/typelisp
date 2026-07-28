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

(defgroup typelisp nil
  "Major mode for editing typelisp code."
  :group 'languages
  :prefix "typelisp-")

;;; Keyword tables ----------------------------------------------------------

(defconst typelisp-definition-forms
  '("defun" "defmethod" "defmacro")
  "Definition forms whose defined name is a function name.")

(defconst typelisp-type-definition-forms
  '("defstruct" "defenum" "deftrait")
  "Definition forms whose defined name is a type or trait name.
Types and traits share one namespace (as in Rust), which is why they share a
rule here: within a module a `defstruct'/`defenum' and a `deftrait' cannot
take the same name.")

(defconst typelisp-variable-definition-forms
  '("defvar" "defconstant")
  "Definition forms whose defined name is a variable.")

(defconst typelisp-special-forms
  '(;; binding & conditionals (docs/syntax.md §4)
    "let" "let*" "if" "when" "unless" "cond" "case" "and" "or" "progn"
    "the" "match" "if-let" "while-let"
    ;; iteration (§5)
    "loop" "while" "until" "dotimes" "dolist" "do" "doiter"
    "break" "return"
    ;; function values & application (§6)
    "lambda" "labels" "apply"
    ;; other special forms (§7)
    "setf" "list" "quote" "quasiquote" "unquote" "unquote-splicing"
    "panic" "unreachable" "todo" "as" "try-as" "compile"
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
  '("pub" "module" "use" "load" "impl" "where")
  "Declaration / namespace keywords.")

(defconst typelisp-lambda-list-keywords
  '("&rest" "&optional" "&key")
  "Lambda-list markers.
`&rest' appears in `defun' / `defmacro' / `lambda' parameter lists and in
function types; `&optional' and `&key' are `defmacro'-only.")

(defconst typelisp-builtin-functions
  '(;; numeric helpers (§4)
    "abs" "gcd" "lcm" "signum" "random" "expt" "sqrt" "floor" "ceiling"
    "round" "truncate" "mod" "rem" "not"
    ;; numeric / char conversions (§4, §2.5).  The `as'/`try-as' special
    ;; forms are sugar over exactly these.
    "int->float" "int->char" "try-int->char" "float->int" "char->int"
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
    "sexpr-atom" "sexpr-symp" "sexpr-sym-name" "sexpr-int" "sexpr-float"
    "sexpr-char" "sexpr-bool" "sexpr-str" "sexpr-append" "sexpr-map"
    ;; equality
    "eq" "eql" "equal" "equalp"
    ;; Iter-based sequence library (§6)
    "length" "append" "nth" "elt" "take" "subseq" "last" "butlast"
    "member" "every" "any" "sort" "assoc" "map" "filter" "foldl" "foldr"
    "reverse" "find" "position" "count" "remove-if"
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
    ;; higher-order combinators
    "identity" "const" "compose" "flip"
    ;; Eq / Ord trait methods
    "equals" "not-equals" "less" "less-equal" "greater" "greater-equal"
    ;; pretty-printer helpers callable inside `pprint-logical-block' (§15.1)
    "pprint-newline" "pprint-indent" "pprint-tab" "pprint-pop"
    "pprint-exit-if-list-exhausted" "pprint-list-exhausted"
    ;; the `print-object' trait method (§15.2)
    "print-object"
    ;; I/O (§15)
    "read-line"
    ;; parsing & evaluation (§16)
    "parse-int" "parse-float" "read" "eval"
    ;; macro / system
    "gensym" "keywordp" "exit" "compile-file")
  "Builtin functions and methods from the standard catalog (docs/functions.md).")

(defconst typelisp-primitive-types
  '("i8" "i16" "i32" "i64" "isize" "u8" "u16" "u32" "u64" "usize"
    "f32" "f64" "bignum" "ratio" "bool" "char" "string" "symbol")
  "Primitive/scalar type names.
Includes the heap-boxed arbitrary-precision `bignum' / `ratio', which are
their own static types with no implicit conversion to or from the fixed-width
numerics (docs/syntax.md §2).")

(defconst typelisp-builtin-types
  '(;; builtin generic/abstract types
    "Option" "Result" "Sexpr" "HashTable" "Vector" "Self"
    ;; builtin concrete error types, one per fallible builtin (§7.1).  `Error'
    ;; itself is *not* a type -- it is the prelude trait these implement, used
    ;; as `:dyn Error'.
    "ParseIntError" "ParseFloatError" "ReadError" "EvalError"
    ;; builtin generic pair & iterator types (lowercase)
    "cons-cell" "vector-iter" "hashtable-iter"
    ;; builtin traits
    "Iter" "Eq" "Ord" "Error")
  "Builtin generic/abstract type names and traits.")

(defconst typelisp-constants
  '("true" "false")
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
;; Buffer-local by design. Resolving a type imported from another file would
;; mean reimplementing `use`/`typelisp.toml` resolution here, which is the
;; language server's job -- so a cross-file type stays uncoloured rather than
;; being guessed at.

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
  "Font-lock matcher for a use of a type this buffer defines, before LIMIT."
  (let ((regexp (typelisp--local-type-regexp)))
    (and regexp (re-search-forward regexp limit t))))

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
    (,(concat "(impl\\_>[ \t\n]*\\(" typelisp--symbol-rx "\\)"
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
        (downcase (match-string-no-properties 0))))))

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
     ,(concat "(impl\\_>[ \t\n]*\\(" typelisp--symbol-rx "[ \t\n]+"
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
