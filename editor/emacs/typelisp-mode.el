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
;; functions, primitive/generic types, literals), comment handling
;; (`;' line comments and nestable `#| ... |#' block comments), and
;; Lisp-style s-expression navigation and indentation.
;;
;; Installation:
;;
;;   (add-to-list 'load-path "/path/to/typelisp/editor/emacs")
;;   (require 'typelisp-mode)
;;
;; `.typl' files will then open in `typelisp-mode' automatically.

;;; Code:

(require 'lisp-mode)

(defgroup typelisp nil
  "Major mode for editing typelisp code."
  :group 'languages
  :prefix "typelisp-")

;;; Keyword tables ----------------------------------------------------------

(defconst typelisp-definition-forms
  '("defun" "defmethod" "defmacro" "defstruct" "defenum" "deftrait")
  "Definition forms whose defined name is a function/type name.")

(defconst typelisp-variable-definition-forms
  '("defvar" "defconstant")
  "Definition forms whose defined name is a variable.")

(defconst typelisp-special-forms
  '("let" "let*" "if" "when" "unless" "cond" "case" "and" "or" "progn"
    "the" "match" "if-let" "while-let" "loop" "while" "until" "dotimes"
    "do" "doiter" "break" "return" "lambda" "labels" "setf"
    "list" "quote" "quasiquote" "unquote" "unquote-splicing"
    "panic" "unreachable" "todo" "compile" "compile-file")
  "Control-flow and other special forms.")

(defconst typelisp-declaration-keywords
  '("pub" "module" "use" "impl" "where")
  "Declaration / namespace keywords.")

(defconst typelisp-builtin-functions
  '(;; numeric helpers & conversions
    "abs" "gcd" "lcm" "signum" "random" "expt" "sqrt" "floor" "ceiling"
    "round" "truncate" "mod" "not"
    "int->float" "int->char" "float->int" "char->int"
    "symbol->string" "string->symbol"
    ;; generic pair (cons-cell<A,B>); set-car/set-cdr were removed, mutate
    ;; via (setf p::car v)/(setf p::cdr v) instead
    "cons" "car" "cdr"
    ;; equality
    "eq" "eql" "equal" "equalp"
    ;; Iter-based sequence library (§6)
    "length" "append" "nth" "elt" "take" "subseq" "last" "butlast"
    "member" "every" "any" "sort" "assoc" "map" "filter" "foldl" "foldr"
    "reverse" "find" "position" "count" "remove-if"
    ;; Option / Result methods
    "unwrap" "unwrap-or" "is-some" "is-none" "is-ok" "is-err"
    ;; Vector / HashTable methods
    "new" "push" "get" "set" "len" "iter" "pop" "clear" "keys" "values"
    "entries" "remove"
    ;; string / char methods
    "upcase" "downcase" "ref" "substring" "alphap" "digitp" "lt"
    ;; higher-order combinators
    "identity" "const" "compose" "flip"
    ;; Eq / Ord trait methods
    "equals" "not-equals" "less" "less-equal" "greater" "greater-equal"
    ;; macro / system
    "gensym" "exit")
  "Builtin functions and methods from the standard catalog (docs/functions.md).")

(defconst typelisp-primitive-types
  '("i8" "i16" "i32" "i64" "isize" "u8" "u16" "u32" "u64" "usize"
    "f32" "f64" "bool" "char" "string" "symbol")
  "Primitive/scalar type names.")

(defconst typelisp-builtin-types
  '(;; builtin generic/abstract types
    "Option" "Result" "Error" "Sexpr" "HashTable" "Vector" "Self"
    ;; builtin generic pair & iterator types (lowercase)
    "cons-cell" "vector-iter" "hashtable-iter"
    ;; builtin traits
    "Iter" "Eq" "Ord")
  "Builtin generic/abstract type names and traits.")

(defconst typelisp-constants
  '("true" "false" "inf")
  "Literal constants.")

;;; Font lock ---------------------------------------------------------------

(defconst typelisp--symbol-rx "\\(?:\\sw\\|\\s_\\)+"
  "Regexp matching a typelisp symbol token (word/symbol constituents).")

(defvar typelisp-font-lock-keywords
  `(
    ;; Character literals: #\a  #\Space  #\Newline  (claimed first so that a
    ;; capitalized name like `Space' is not mistaken for a type).
    ("#\\\\\\(?:[A-Za-z][A-Za-z0-9]*\\|.\\)" . font-lock-constant-face)

    ;; `:dyn Trait' -- the trait-object type.  Claimed before the general
    ;; keyword rule below (which would otherwise colour it as a constant):
    ;; `:dyn' is a reserved keyword valid only in a type position, and
    ;; highlighting it distinctly is the reason the language spells trait
    ;; objects this way at all.  The trait name after it gets the type face.
    (,(concat "\\_<\\(:dyn\\)\\_>[ \t\n]*\\(" typelisp--symbol-rx "\\)?")
     (1 font-lock-keyword-face)
     (2 font-lock-type-face nil t))

    ;; Keywords: :name (CL-style self-evaluating symbols).  Not `::foo',
    ;; which is the absolute-path syntax.
    ("\\(?:^\\|[^:[:alnum:]_-]\\)\\(:[A-Za-z][A-Za-z0-9_?!*<>=/+-]*\\)"
     1 font-lock-constant-face)

    ;; Definition forms binding a function/type name:  (defun NAME ...)
    (,(concat "(" (regexp-opt typelisp-definition-forms t)
              "\\_>[ \t\n]*(?[ \t\n]*\\(" typelisp--symbol-rx "\\)?")
     (1 font-lock-keyword-face)
     (2 font-lock-function-name-face nil t))

    ;; Variable definitions:  (defvar (NAME Type) ...)
    (,(concat "(" (regexp-opt typelisp-variable-definition-forms t)
              "\\_>[ \t\n]*(?[ \t\n]*\\(" typelisp--symbol-rx "\\)?")
     (1 font-lock-keyword-face)
     (2 font-lock-variable-name-face nil t))

    ;; Namespace / declaration keywords
    (,(regexp-opt typelisp-declaration-keywords 'symbols)
     . font-lock-keyword-face)

    ;; Special forms
    (,(regexp-opt typelisp-special-forms 'symbols)
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

    ;; User-defined types: CapitalizedName, optionally generic <...>
    ("\\_<\\([A-Z][A-Za-z0-9_]*\\)" 1 font-lock-type-face)

    ;; Constants
    (,(regexp-opt typelisp-constants 'symbols)
     . font-lock-constant-face)
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

(defun typelisp--set-indentation ()
  "Install `lisp-indent-function' properties for typelisp-only forms.
Common forms shared with Emacs Lisp (`defun', `let', `if', ...) already
indent correctly under `lisp-mode' defaults and are left untouched so as
not to perturb other Lisp buffers."
  (dolist (spec '((defmethod   . defun)
                  (defmacro    . defun)
                  (defstruct   . defun)
                  (defenum     . defun)
                  (deftrait    . defun)
                  (module      . defun)
                  (impl        . 2)
                  (match       . 1)
                  (until       . 1)
                  (doiter      . 1)
                  (if-let      . 2)
                  (while-let   . 1)
                  (loop        . 0)))
    (put (car spec) 'lisp-indent-function (cdr spec))))

(typelisp--set-indentation)

;;; Mode definition ---------------------------------------------------------

(defvar typelisp-mode-map
  (let ((map (make-sparse-keymap)))
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
  (setq-local lisp-indent-function 'lisp-indent-function)
  (setq-local font-lock-defaults
              '(typelisp-font-lock-keywords
                nil                     ; keywords-only
                nil                     ; case-fold: keep case-sensitive so that
                                        ; Capitalized type names highlight distinctly
                nil                     ; syntax-alist
                nil)))                  ; other vars

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.typl\\'" . typelisp-mode))

(provide 'typelisp-mode)

;;; typelisp-mode.el ends here
