# ABI バージョン 0.3.35

`scripts/regen-abi-version.sh` が生成した文書。手で編集しない。

コンパイラが生成するコードが、ABI バージョン 0.3.35 のランタイムライブラリについて仮定していることの一覧。
ここに書かれていない前提(シムが引数をどう扱うか、生成コードがヒープを直接読む箇所など)は含まない。

内容ハッシュ (FNV-1a 64): `44c9f4595e810bdc`

## 呼び出し規約 (typelisp_abi)

| 名前 | 値 |
|---|---|
| STATUS_RETURN | 0 |
| STATUS_CALL | 1 |
| STATUS_SUSPEND | 2 |
| STATUS_UNWIND | 3 |
| BODY_ABI_CLASSIC | 0 |
| BODY_ABI_COROUTINE | 1 |
| FRAME_VALUE_SLOT | 0 |
| FRAME_HANDLER_SLOT | 1 |
| FRAME_PC_SLOT | 2 |
| FRAME_RESERVED_SLOTS | 3 |

## 中断の種類 (typelisp_abi::call_state)

| 名前 | 値 |
|---|---|
| SUSPEND_YIELD | 0 |
| SUSPEND_SLEEP | 1 |
| SUSPEND_WAIT | 2 |
| SUSPEND_SAFEPOINT | 3 |
| SUSPEND_CHAN_NEW | 4 |
| SUSPEND_CHAN_LEN | 5 |
| SUSPEND_CHAN_CAP | 6 |
| SUSPEND_CHAN_CLOSE | 7 |
| SUSPEND_CHAN_SEND | 8 |
| SUSPEND_CHAN_RECV | 9 |
| SUSPEND_CHAN_SELECT | 10 |
| SUSPEND_IO | 11 |
| SUSPEND_IO_FOR | 12 |
| SUSPEND_TASK | 13 |
| SUSPEND_THREAD | 14 |
| SUSPEND_MAIN | 15 |

## 生成コードの本体の ABI (typelisp::compile)

| 名前 | 値 |
|---|---|
| EMITTED_BODY_ABI | 1 |

## 値の表現 (typelisp_mem::tagged)

| 名前 | 値 |
|---|---|
| LAYOUT | 2 |
| LAYOUT_THREE_BIT | 0 |
| LAYOUT_FIXNUM_ONE_BIT | 1 |
| LAYOUT_OPTION_NICHE | 2 |
| FIXNUM_SHIFT | 1 |
| FIXNUM_MIN | -4611686018427387904 |
| FIXNUM_MAX | 4611686018427387903 |
| FIXNUM_MASK | 1 |
| FIXNUM_TAG | 0 |
| LOW_MASK | 7 |
| TAG_CONS | 1 |
| TAG_SYMBOL | 3 |
| TAG_BOXED | 5 |
| TAG_SMALL | 7 |
| SMALL_MASK | 63 |
| SMALL_SHIFT | 6 |
| TAG_IMMEDIATE | 7 |
| TAG_CHAR | 15 |
| TAG_STR | 23 |
| TAG_PATH | 31 |
| BOXED_SHIFT | 3 |
| IMMEDIATE_NIL | 0 |
| IMMEDIATE_FALSE | 1 |
| IMMEDIATE_TRUE | 2 |
| NIL_WORD | 7 |

## C メモリ (typelisp_rt::c_mem)

| 名前 | 値 |
|---|---|
| KIND_I8 | 1 |
| KIND_I16 | 2 |
| KIND_I32 | 3 |
| KIND_U8 | 4 |
| KIND_U16 | 5 |
| KIND_U32 | 6 |
| KIND_C_LONG | 7 |
| KIND_C_ULONG | 8 |
| KIND_F32 | 9 |
| KIND_F64 | 10 |
| KIND_BOOL | 11 |
| KIND_PTR | 12 |

## 終了コード (typelisp_rt)

| 名前 | 値 |
|---|---|
| EXIT_CODE_PANIC | 1 |
| EXIT_CODE_SUCCESS | 0 |

## ダンプ形式 (typelisp_front::dump)

| 名前 | 値 |
|---|---|
| FORMAT_VERSION | 29 |
| CONTAINER_VERSION | 1 |

## well-known シンボル (typelisp_mem::BUILTIN_SYMBOLS)

| 番号 | 名前 |
|---|---|
| 0 | `quote` |
| 1 | `unquote` |
| 2 | `unquote-splicing` |
| 3 | `the` |
| 4 | `fn` |
| 5 | `pub` |
| 6 | `where` |
| 7 | `return` |
| 8 | `&rest` |
| 9 | `&optional` |
| 10 | `&key` |
| 11 | `:dyn` |
| 12 | `=` |
| 13 | `:=` |
| 14 | `int` |
| 15 | `float` |
| 16 | `bignum` |
| 17 | `ratio` |
| 18 | `char` |
| 19 | `bool` |
| 20 | `str` |
| 21 | `unit` |
| 22 | `var` |
| 23 | `set` |
| 24 | `global` |
| 25 | `set-global` |
| 26 | `let` |
| 27 | `lambda` |
| 28 | `labels` |
| 29 | `call` |
| 30 | `assoc` |
| 31 | `apply` |
| 32 | `if` |
| 33 | `loop` |
| 34 | `break` |
| 35 | `panic` |
| 36 | `match` |
| 37 | `construct` |
| 38 | `field-get` |
| 39 | `field-set` |
| 40 | `dyn-new` |
| 41 | `dyn-upcast` |
| 42 | `dyn-call` |
| 43 | `dyn-value` |
| 44 | `catch` |
| 45 | `throw` |
| 46 | `unwind-protect` |
| 47 | `sym` |
| 48 | `fnref` |
| 49 | `methodref` |
| 50 | `compile-fn` |
| 51 | `method` |
| 52 | `pat-wild` |
| 53 | `pat-empty` |
| 54 | `pat-nonempty` |
| 55 | `pat-bind` |
| 56 | `pat-lit` |
| 57 | `pat-guard` |
| 58 | `pat-ctor` |
| 59 | `pat-typetest` |
| 60 | `defun` |
| 61 | `defmethod` |
| 62 | `defmacro` |
| 63 | `defvar` |
| 64 | `defstruct` |
| 65 | `defenum` |
| 66 | `module` |
| 67 | `use` |
| 68 | `load` |
| 69 | `expr` |
| 70 | `defsignature` |
| 71 | `defconstant` |
| 72 | `deftrait` |
| 73 | `deftype` |
| 74 | `impl` |
| 75 | `nil` |
| 76 | `cons` |
| 77 | `path` |
| 78 | `progn` |
| 79 | `cond` |
| 80 | `and` |
| 81 | `or` |
| 82 | `list` |
| 83 | `block` |
| 84 | `when` |
| 85 | `unless` |
| 86 | `while` |
| 87 | `let*` |
| 88 | `case` |
| 89 | `setf` |
| 90 | `as` |
| 91 | `dolist` |
| 92 | `dotimes` |
| 93 | `doiter` |
| 94 | `until` |
| 95 | `do` |
| 96 | `:upcase` |
| 97 | `:capitalize` |
| 98 | `<monomorph specializations>` |
| 99 | `:with` |
| 100 | `:for` |
| 101 | `:as` |
| 102 | `:repeat` |
| 103 | `:initially` |
| 104 | `:finally` |
| 105 | `:in` |
| 106 | `:across` |
| 107 | `:on` |
| 108 | `:from` |
| 109 | `:downfrom` |
| 110 | `:upfrom` |
| 111 | `:to` |
| 112 | `:below` |
| 113 | `:downto` |
| 114 | `int-any-width` |
| 115 | `float-any-width` |
| 116 | `i32` |
| 117 | `f64` |
| 118 | `f32` |
| 119 | `:above` |
| 120 | `:by` |
| 121 | `:then` |
| 122 | `:do` |
| 123 | `:doing` |
| 124 | `:collect` |
| 125 | `:collecting` |
| 126 | `:append` |
| 127 | `:appending` |
| 128 | `:sum` |
| 129 | `:summing` |
| 130 | `:count` |
| 131 | `:counting` |
| 132 | `:maximize` |
| 133 | `:maximizing` |
| 134 | `:minimize` |
| 135 | `:minimizing` |
| 136 | `:always` |
| 137 | `:never` |
| 138 | `:thereis` |
| 139 | `:while` |
| 140 | `:until` |
| 141 | `:return` |
| 142 | `:when` |
| 143 | `:if` |
| 144 | `:unless` |
| 145 | `:else` |
| 146 | `:into` |
| 147 | `i8` |
| 148 | `i16` |
| 149 | `u8` |
| 150 | `u16` |
| 151 | `u32` |
| 152 | `return-from` |
| 153 | `in-module` |
| 154 | `import` |
| 155 | `shadowing-import` |
| 156 | `defparameter` |
| 157 | `:named` |
| 158 | `trace` |
| 159 | `untrace` |
| 160 | `step` |
| 161 | `disassemble-fn` |
| 162 | `defffi` |
| 163 | `go` |
| 164 | `select` |
| 165 | `untag-int` |
| 166 | `tag-int` |
| 167 | `some-of` |
| 168 | `pat-some` |
| 169 | `box-option` |
| 170 | `else` |
| 171 | `spawn` |
| 172 | `tag` |
| 173 | `thread` |
| 174 | `spawn-thread` |
| 175 | `task` |
| 176 | `&body` |
| 177 | `pat-when` |
| 178 | `not` |
| 179 | `unsafe` |
| 180 | `def-c-struct` |
| 181 | `self` |
| 182 | `test` |
| 183 | `ptr` |
| 184 | `recv` |
| 185 | `send` |
| 186 | `:type` |
| 187 | `:where` |
| 188 | `:or` |
| 189 | `:constructor` |
| 190 | `:copier` |
| 191 | `:include` |
| 192 | `:conc-name` |
| 193 | `:predicate` |
| 194 | `:initial-offset` |
| 195 | `:library` |
| 196 | `:prefix` |
| 197 | `:per-line-prefix` |
| 198 | `:suffix` |
| 199 | `:downcase` |
| 200 | `:linear` |
| 201 | `:fill` |
| 202 | `:miser` |
| 203 | `:mandatory` |
| 204 | `:block` |
| 205 | `:current` |
| 206 | `:line` |
| 207 | `:section` |
| 208 | `:line-relative` |
| 209 | `:section-relative` |
| 210 | `intern` |
| 211 | `_` |
| 212 | `incf` |
| 213 | `decf` |
| 214 | `rotatef` |
| 215 | `shiftf` |
| 216 | `macrolet` |
| 217 | `symbol-macrolet` |
| 218 | `try-as` |
| 219 | `compile` |
| 220 | `quasiquote` |
| 221 | `format` |
| 222 | `print` |
| 223 | `println` |
| 224 | `eprint` |
| 225 | `eprintln` |
| 226 | `source-file` |
| 227 | `pprint` |
| 228 | `pprint-fill` |
| 229 | `pprint-linear` |
| 230 | `pprint-tabular` |
| 231 | `pprint-logical-block` |
| 232 | `c-alloc` |
| 233 | `c-ref` |
| 234 | `c-deref` |
| 235 | `make-instance` |
| 236 | `documentation` |
| 237 | `disassemble` |
| 238 | `ed` |
| 239 | `%internal` |
| 240 | `attribute` |
| 241 | `apply-indirect` |
| 242 | `cellvar` |
| 243 | `cellset` |
| 244 | `global-init` |
| 245 | `hashtable-op` |
| 246 | `llvm-op` |
| 247 | `suspend` |
| 248 | `vector-op` |
| 249 | `handle` |
| 250 | `raw-word` |
| 251 | `rational` |
| 252 | `random-state` |
| 253 | `sexpr` |
| 254 | `struct` |
| 255 | `enum` |
| 256 | `dyn` |
| 257 | `scope` |
| 258 | `vector` |
| 259 | `hashtable` |
| 260 | `niche` |
| 261 | `none` |
| 262 | `%type-apply` |
| 263 | `%type-member` |
| 264 | `%value-member` |
| 265 | `shadow` |
| 266 | `type-named` |
| 267 | `type-fn` |
| 268 | `type-dyn` |
| 269 | `type-ptr` |
| 270 | `type-cstruct` |
| 271 | `type-param` |
| 272 | `type-var` |
| 273 | `type-unit` |
| 274 | `type-never` |
| 275 | `type-ref` |
| 276 | `type-value` |
| 277 | `Option` |
| 278 | `Result` |
| 279 | `Sexpr` |
| 280 | `Vector` |
| 281 | `HashTable` |
| 282 | `Scope` |
| 283 | `Chan` |
| 284 | `Task` |
| 285 | `Thread` |
| 286 | `Array` |
| 287 | `string` |
| 288 | `symbol` |
| 289 | `ParseIntError` |
| 290 | `ParseFloatError` |
| 291 | `ReadError` |
| 292 | `EvalError` |
| 293 | `FileError` |
| 294 | `NetError` |
| 295 | `llvm-module` |
| 296 | `llvm-function` |
| 297 | `llvm-builder` |
| 298 | `llvm-basic-block` |
| 299 | `llvm-value` |
| 300 | `c-long` |
| 301 | `c-ulong` |
| 302 | `!` |
| 303 | `Self` |

## 組み込み型 (typelisp_mem::BUILTIN_TYPES)

| 番号 | 型 |
|---|---|
| 0 | `std::Option` |
| 1 | `std::Result` |
| 2 | `std::Vector` |
| 3 | `std::cons-cell` |
| 4 | `std::HashTable` |
| 5 | `std::Scope` |
| 6 | `std::scope-frame` |
| 7 | `std::ReadError` |
| 8 | `std::FileError` |
| 9 | `std::ParseIntError` |
| 10 | `std::ParseFloatError` |
| 11 | `std::time::universal-time` |
| 12 | `std::time::internal-time` |
| 13 | `std::repl::heap-info` |
| 14 | `std::NetError` |
| 15 | `std::Sexpr` |
| 16 | `std::Option<std::Sexpr>` |
| 17 | `std::Vector<std::Option<std::Sexpr>>` |
| 18 | `std::array<std::Option<std::Sexpr>>` |

## 実行時が組み立てる値の型 (typelisp_rt)

| 表 | 関数 | 型 |
|---|---|---|
| sys_builtin::RESULT_TYPES | `parse-float` | `std::Result<std::f64,std::ParseFloatError>` |
| sys_builtin::RESULT_TYPES | `command-line-args` | `std::Vector<std::string>` |
| sys_builtin::RESULT_TYPES | `getenv` | `std::Option<std::string>` |
| sys_builtin::RESULT_TYPES | `home-directory` | `std::Option<std::string>` |
| sys_builtin::RESULT_TYPES | `machine-instance` | `std::Option<std::string>` |
| sys_builtin::RESULT_TYPES | `machine-version` | `std::Option<std::string>` |
| sys_builtin::RESULT_TYPES | `software-version` | `std::Option<std::string>` |
| sys_builtin::RESULT_TYPES | `timezone-offset-seconds` | `std::Option<std::int>` |
| sys_builtin::RESULT_TYPES | `timezone-daylight-p` | `std::Option<std::bool>` |
| sys_builtin::RESULT_TYPES | `dribble-start` | `std::Result<(),std::FileError>` |
| sys_builtin::RESULT_TYPES | `dribble-stop` | `std::Result<(),std::FileError>` |
| sys_builtin::RESULT_TYPES | `ed-open` | `std::Result<(),std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-open-file` | `std::Result<std::i32,std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-close` | `std::Result<(),std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-input-p` | `std::Result<std::bool,std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-output-p` | `std::Result<std::bool,std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-listen` | `std::Result<std::bool,std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-position` | `std::Result<std::int,std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-at-line-start` | `std::Result<std::bool,std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-read-char` | `std::Result<std::Option<std::char>,std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-unread-char` | `std::Result<(),std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-read-byte` | `std::Result<std::Option<std::int>,std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-write-byte` | `std::Result<(),std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-write-string` | `std::Result<(),std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-finish-output` | `std::Result<(),std::FileError>` |
| stream_builtin::RESULT_TYPES | `stream-take-output-string` | `std::Result<std::string,std::FileError>` |
| stream_builtin::RESULT_TYPES | `file-delete` | `std::Result<(),std::FileError>` |
| stream_builtin::RESULT_TYPES | `file-truename` | `std::Result<std::string,std::FileError>` |
| stream_builtin::RESULT_TYPES | `file-modified-date` | `std::Result<std::time::universal-time,std::FileError>` |
| stream_builtin::RESULT_TYPES | `file-owner-name` | `std::Result<std::Option<std::string>,std::FileError>` |
| stream_builtin::RESULT_TYPES | `file-list-directory` | `std::Result<std::Vector<std::string>,std::FileError>` |
| stream_builtin::RESULT_TYPES | `file-create-directories` | `std::Result<(),std::FileError>` |
| stream_builtin::RESULT_TYPES | `file-rename` | `std::Result<(),std::FileError>` |
| stream_builtin::INNER_TYPES | `stream-read-char` | `std::Option<std::char>` |
| stream_builtin::INNER_TYPES | `stream-read-byte` | `std::Option<std::int>` |
| stream_builtin::INNER_TYPES | `file-list-directory` | `std::Vector<std::string>` |
| stream_builtin::INNER_TYPES | `file-owner-name` | `std::Option<std::string>` |
| net_builtin::RESULT_TYPES | `net-resolve-begin` | `std::Result<std::i32,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-resolve-finish` | `std::Result<std::Option<std::Vector<std::string>>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-connect-begin` | `std::Result<std::i32,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-connect-finish` | `std::Result<(),std::NetError>` |
| net_builtin::RESULT_TYPES | `net-listen` | `std::Result<std::i32,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-tls-listen` | `std::Result<std::i32,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-unix-connect` | `std::Result<std::i32,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-unix-listen` | `std::Result<std::i32,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-accept` | `std::Result<std::Option<std::i32>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-fill` | `std::Result<std::Option<std::int>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-pop-byte` | `std::Result<std::Option<std::int>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-pop-char` | `std::Result<std::Option<std::char>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-buffered-p` | `std::Result<std::bool,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-push-string` | `std::Result<(),std::NetError>` |
| net_builtin::RESULT_TYPES | `net-push-byte` | `std::Result<(),std::NetError>` |
| net_builtin::RESULT_TYPES | `net-flush` | `std::Result<std::Option<std::int>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-socket-error` | `std::Result<std::Option<std::string>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-peer-subject` | `std::Result<std::Option<std::string>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-server-name` | `std::Result<std::Option<std::string>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-set-nodelay` | `std::Result<(),std::NetError>` |
| net_builtin::RESULT_TYPES | `net-set-keepalive` | `std::Result<(),std::NetError>` |
| net_builtin::RESULT_TYPES | `net-set-keepalive-period` | `std::Result<(),std::NetError>` |
| net_builtin::RESULT_TYPES | `net-tls-add-certificate` | `std::Result<(),std::NetError>` |
| net_builtin::RESULT_TYPES | `net-shutdown-write` | `std::Result<(),std::NetError>` |
| net_builtin::RESULT_TYPES | `net-local-address` | `std::Result<std::string,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-peer-address` | `std::Result<std::string,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-tls-start` | `std::Result<(),std::NetError>` |
| net_builtin::RESULT_TYPES | `net-tls-handshake` | `std::Result<std::Option<std::int>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-udp-bind` | `std::Result<std::i32,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-udp-send-to` | `std::Result<std::bool,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-udp-recv` | `std::Result<std::Option<std::Vector<std::int>>,std::NetError>` |
| net_builtin::RESULT_TYPES | `net-udp-last-sender` | `std::Result<std::string,std::NetError>` |
| net_builtin::INNER_TYPES | `net-resolve-finish` | `std::Option<std::Vector<std::string>>` |
| net_builtin::INNER_TYPES | `net-tls-handshake` | `std::Option<std::int>` |
| net_builtin::INNER_TYPES | `net-udp-recv` | `std::Option<std::Vector<std::int>>` |
| net_builtin::INNER_TYPES | `net-accept` | `std::Option<std::i32>` |
| net_builtin::INNER_TYPES | `net-fill` | `std::Option<std::int>` |
| net_builtin::INNER_TYPES | `net-flush` | `std::Option<std::int>` |
| net_builtin::INNER_TYPES | `net-socket-error` | `std::Option<std::string>` |
| net_builtin::INNER_TYPES | `net-peer-subject` | `std::Option<std::string>` |
| net_builtin::INNER_TYPES | `net-server-name` | `std::Option<std::string>` |
| net_builtin::INNER_TYPES | `net-pop-byte` | `std::Option<std::int>` |
| net_builtin::INNER_TYPES | `net-pop-char` | `std::Option<std::char>` |
| readtable::READER_MACRO_OPTION_TYPE | | `std::option<(fn (std::io::string-input-stream,std::char) std::Option<std::Sexpr>)>` |

## 型ごとの表現 (Repr)

| 型 | 型キー | Repr | field_kind | binding_kind |
|---|---|---|---|---|
| `!` | `!` | none | 0 | 0 |
| `()` | `()` | unit | 100 | 0 |
| `(fn (std::io::string-input-stream char) std::Option<std::Sexpr>)` | `(fn (std::io::string-input-stream,std::char) std::Option<std::Sexpr>)` | fn | 6 | 2 |
| `E` | `E` | none | 0 | 0 |
| `K` | `K` | none | 0 | 0 |
| `T` | `T` | none | 0 | 0 |
| `V` | `V` | none | 0 | 0 |
| `bool` | `std::bool` | bool | 4 | 0 |
| `c-long` | `std::c-long` | raw-word | 0 | 0 |
| `c-ulong` | `std::c-ulong` | raw-word | 0 | 0 |
| `char` | `std::char` | char | 3 | 0 |
| `f32` | `std::f32` | f32 | 11 | 0 |
| `f64` | `std::f64` | f64 | 2 | 0 |
| `i16` | `std::i16` | int-any-width | 1 | 0 |
| `i32` | `std::i32` | int-any-width | 1 | 0 |
| `i8` | `std::i8` | int-any-width | 1 | 0 |
| `int` | `std::int` | int | 6 | 2 |
| `random-state` | `std::random-state` | random-state | 6 | 2 |
| `ratio` | `std::ratio` | ratio | 6 | 2 |
| `rational` | `std::rational` | rational | 6 | 2 |
| `std::HashTable<K,V>` | `std::HashTable<K,V>` | hashtable | 6 | 2 |
| `std::Option<(fn (std::io::string-input-stream char) std::Option<std::Sexpr>)>` | `std::Option<(fn (std::io::string-input-stream,std::char) std::Option<std::Sexpr>)>` | niche | 101 | 2 |
| `std::Option<T>` | `std::Option<T>` | enum | 6 | 2 |
| `std::Option<V>` | `std::Option<V>` | enum | 6 | 2 |
| `std::Option<bool>` | `std::Option<std::bool>` | niche | 101 | 0 |
| `std::Option<c-long>` | `std::Option<std::c-long>` | enum | 6 | 2 |
| `std::Option<c-ulong>` | `std::Option<std::c-ulong>` | enum | 6 | 2 |
| `std::Option<char>` | `std::Option<std::char>` | niche | 101 | 0 |
| `std::Option<f32>` | `std::Option<std::f32>` | niche | 101 | 2 |
| `std::Option<f64>` | `std::Option<std::f64>` | niche | 101 | 2 |
| `std::Option<i16>` | `std::Option<std::i16>` | niche | 101 | 0 |
| `std::Option<i32>` | `std::Option<std::i32>` | niche | 101 | 0 |
| `std::Option<i8>` | `std::Option<std::i8>` | niche | 101 | 0 |
| `std::Option<int>` | `std::Option<std::int>` | niche | 101 | 2 |
| `std::Option<std::Sexpr>` | `std::Option<std::Sexpr>` | niche | 101 | 2 |
| `std::Option<string>` | `std::Option<std::string>` | niche | 101 | 2 |
| `std::Option<u16>` | `std::Option<std::u16>` | niche | 101 | 0 |
| `std::Option<u32>` | `std::Option<std::u32>` | niche | 101 | 0 |
| `std::Option<u8>` | `std::Option<std::u8>` | niche | 101 | 0 |
| `std::Result<(),std::FileError>` | `std::Result<(),std::FileError>` | enum | 6 | 2 |
| `std::Result<(),std::NetError>` | `std::Result<(),std::NetError>` | enum | 6 | 2 |
| `std::Result<bool,std::FileError>` | `std::Result<std::bool,std::FileError>` | enum | 6 | 2 |
| `std::Result<bool,std::NetError>` | `std::Result<std::bool,std::NetError>` | enum | 6 | 2 |
| `std::Result<f64,std::ParseFloatError>` | `std::Result<std::f64,std::ParseFloatError>` | enum | 6 | 2 |
| `std::Result<i32,std::FileError>` | `std::Result<std::i32,std::FileError>` | enum | 6 | 2 |
| `std::Result<i32,std::NetError>` | `std::Result<std::i32,std::NetError>` | enum | 6 | 2 |
| `std::Result<int,std::FileError>` | `std::Result<std::int,std::FileError>` | enum | 6 | 2 |
| `std::Result<std::Option<char>,std::FileError>` | `std::Result<std::Option<std::char>,std::FileError>` | enum | 6 | 2 |
| `std::Result<std::Option<char>,std::NetError>` | `std::Result<std::Option<std::char>,std::NetError>` | enum | 6 | 2 |
| `std::Result<std::Option<i32>,std::NetError>` | `std::Result<std::Option<std::i32>,std::NetError>` | enum | 6 | 2 |
| `std::Result<std::Option<int>,std::FileError>` | `std::Result<std::Option<std::int>,std::FileError>` | enum | 6 | 2 |
| `std::Result<std::Option<int>,std::NetError>` | `std::Result<std::Option<std::int>,std::NetError>` | enum | 6 | 2 |
| `std::Result<std::Option<std::Sexpr>,std::EvalError>` | `std::Result<std::Option<std::Sexpr>,std::EvalError>` | enum | 6 | 2 |
| `std::Result<std::Option<std::Sexpr>,std::ReadError>` | `std::Result<std::Option<std::Sexpr>,std::ReadError>` | enum | 6 | 2 |
| `std::Result<std::Option<std::Vector<int>>,std::NetError>` | `std::Result<std::Option<std::Vector<std::int>>,std::NetError>` | enum | 6 | 2 |
| `std::Result<std::Option<std::Vector<string>>,std::NetError>` | `std::Result<std::Option<std::Vector<std::string>>,std::NetError>` | enum | 6 | 2 |
| `std::Result<std::Option<string>,std::FileError>` | `std::Result<std::Option<std::string>,std::FileError>` | enum | 6 | 2 |
| `std::Result<std::Option<string>,std::NetError>` | `std::Result<std::Option<std::string>,std::NetError>` | enum | 6 | 2 |
| `std::Result<std::Vector<string>,std::FileError>` | `std::Result<std::Vector<std::string>,std::FileError>` | enum | 6 | 2 |
| `std::Result<std::cons-cell<std::Option<std::Sexpr>,int>,std::ReadError>` | `std::Result<std::cons-cell<std::Option<std::Sexpr>,std::int>,std::ReadError>` | enum | 6 | 2 |
| `std::Result<std::time::universal-time,std::FileError>` | `std::Result<std::time::universal-time,std::FileError>` | enum | 6 | 2 |
| `std::Result<string,std::FileError>` | `std::Result<std::string,std::FileError>` | enum | 6 | 2 |
| `std::Result<string,std::NetError>` | `std::Result<std::string,std::NetError>` | enum | 6 | 2 |
| `std::Scope<V>` | `std::Scope<V>` | scope | 6 | 2 |
| `std::Vector<K>` | `std::Vector<K>` | vector | 6 | 2 |
| `std::Vector<T>` | `std::Vector<T>` | vector | 6 | 2 |
| `std::Vector<V>` | `std::Vector<V>` | vector | 6 | 2 |
| `std::Vector<int>` | `std::Vector<std::int>` | vector | 6 | 2 |
| `std::Vector<std::Option<std::Sexpr>>` | `std::Vector<std::Option<std::Sexpr>>` | vector | 6 | 2 |
| `std::Vector<std::cons-cell<K,V>>` | `std::Vector<std::cons-cell<K,V>>` | vector | 6 | 2 |
| `std::Vector<string>` | `std::Vector<std::string>` | vector | 6 | 2 |
| `std::array<std::Option<std::Sexpr>>` | `std::array<std::Option<std::Sexpr>>` | none | 0 | 0 |
| `std::llvm-basic-block` | `std::llvm-basic-block` | handle | 1 | 0 |
| `std::llvm-builder` | `std::llvm-builder` | handle | 1 | 0 |
| `std::llvm-function` | `std::llvm-function` | handle | 1 | 0 |
| `std::llvm-module` | `std::llvm-module` | handle | 1 | 0 |
| `std::llvm-value` | `std::llvm-value` | handle | 1 | 0 |
| `std::repl::heap-info` | `std::repl::heap-info` | none | 0 | 0 |
| `std::sync::Chan<T>` | `std::sync::Chan<T>` | struct | 6 | 2 |
| `std::thread::Task<T>` | `std::thread::Task<T>` | struct | 6 | 2 |
| `std::thread::Thread<T>` | `std::thread::Thread<T>` | struct | 6 | 2 |
| `std::time::internal-time` | `std::time::internal-time` | none | 0 | 0 |
| `std::time::universal-time` | `std::time::universal-time` | none | 0 | 0 |
| `string` | `std::string` | str | 6 | 2 |
| `symbol` | `std::symbol` | sym | 6 | 0 |
| `u16` | `std::u16` | int-any-width | 1 | 0 |
| `u32` | `std::u32` | int-any-width | 1 | 0 |
| `u8` | `std::u8` | int-any-width | 1 | 0 |

## ランタイム関数

| シンボル | LLVM の型 |
|---|---|
| `rt_atom` | `i64 (ptr, i32)` |
| `rt_bignum_new` | `i64 (ptr, i32)` |
| `rt_box_kind` | `i64 (ptr, i32)` |
| `rt_c_alloc` | `i64 (ptr, i32)` |
| `rt_c_arena_close` | `i64 (ptr, i32)` |
| `rt_c_arena_open` | `i64 (ptr, i32)` |
| `rt_c_index` | `i64 (ptr, i32)` |
| `rt_c_load` | `i64 (ptr, i32)` |
| `rt_c_offset` | `i64 (ptr, i32)` |
| `rt_c_ptr_check` | `i64 (ptr, i32)` |
| `rt_c_store` | `i64 (ptr, i32)` |
| `rt_car` | `i64 (ptr, i32)` |
| `rt_cdr` | `i64 (ptr, i32)` |
| `rt_cell_get` | `i64 (ptr, i32)` |
| `rt_cell_new` | `i64 (ptr, i32)` |
| `rt_cell_set` | `i64 (ptr, i32)` |
| `rt_char_alphap` | `i64 (ptr, i32)` |
| `rt_char_digitp` | `i64 (ptr, i32)` |
| `rt_char_downcase` | `i64 (ptr, i32)` |
| `rt_char_equalp` | `i64 (ptr, i32)` |
| `rt_char_upcase` | `i64 (ptr, i32)` |
| `rt_closure_env_get` | `i64 (ptr, i32)` |
| `rt_closure_env_len` | `i64 (ptr, i32)` |
| `rt_closure_fnptr` | `i64 (ptr, i32)` |
| `rt_closure_new` | `i64 (ptr, i32)` |
| `rt_command_line_args` | `i64 (ptr, i32)` |
| `rt_cons` | `i64 (ptr, i32)` |
| `rt_consp` | `i64 (ptr, i32)` |
| `rt_coroutine_closure_new` | `i64 (ptr, i32)` |
| `rt_data_field` | `i64 (ptr, i32)` |
| `rt_data_new` | `i64 (ptr, i32)` |
| `rt_data_variant` | `i64 (ptr, i32)` |
| `rt_dribble_start` | `i64 (ptr, i32)` |
| `rt_dribble_stop` | `i64 (ptr, i32)` |
| `rt_drive_body` | `i64 (ptr, i32)` |
| `rt_dyn_new` | `i64 (ptr, i32)` |
| `rt_dyn_upcast` | `i64 (ptr, i32)` |
| `rt_dyn_value` | `i64 (ptr, i32)` |
| `rt_dyn_vtable` | `i64 (ptr, i32)` |
| `rt_ed_open` | `i64 (ptr, i32)` |
| `rt_eval` | `i64 (ptr, i32)` |
| `rt_eval_init` | `i64 (ptr, i32)` |
| `rt_eval_state` | `i64 (ptr, i32)` |
| `rt_exit` | `i64 (ptr, i32)` |
| `rt_f32_new` | `i64 (ptr, i32)` |
| `rt_f32_value` | `i64 (ptr, i32)` |
| `rt_f64_acos` | `i64 (ptr, i32)` |
| `rt_f64_acosh` | `i64 (ptr, i32)` |
| `rt_f64_asin` | `i64 (ptr, i32)` |
| `rt_f64_asinh` | `i64 (ptr, i32)` |
| `rt_f64_atan` | `i64 (ptr, i32)` |
| `rt_f64_atanh` | `i64 (ptr, i32)` |
| `rt_f64_cosh` | `i64 (ptr, i32)` |
| `rt_f64_fits_f32` | `i64 (ptr, i32)` |
| `rt_f64_new` | `i64 (ptr, i32)` |
| `rt_f64_sinh` | `i64 (ptr, i32)` |
| `rt_f64_tan` | `i64 (ptr, i32)` |
| `rt_f64_tanh` | `i64 (ptr, i32)` |
| `rt_f64_value` | `i64 (ptr, i32)` |
| `rt_ffi_callback_address` | `i64 (ptr, i32)` |
| `rt_ffi_callback_enter` | `i64 (ptr, i32)` |
| `rt_ffi_callback_invoke` | `i64 (ptr, i32)` |
| `rt_ffi_callback_leave` | `i64 (ptr, i32)` |
| `rt_ffi_callback_register` | `i64 (ptr, i32)` |
| `rt_ffi_cstring_free` | `i64 (ptr, i32)` |
| `rt_ffi_cstring_new` | `i64 (ptr, i32)` |
| `rt_ffi_enter_native` | `i64 (ptr, i32)` |
| `rt_ffi_leave_native` | `i64 (ptr, i32)` |
| `rt_ffi_string_from_cstr` | `i64 (ptr, i32)` |
| `rt_file_create_directories` | `i64 (ptr, i32)` |
| `rt_file_delete` | `i64 (ptr, i32)` |
| `rt_file_directory_p` | `i64 (ptr, i32)` |
| `rt_file_exists_p` | `i64 (ptr, i32)` |
| `rt_file_list_directory` | `i64 (ptr, i32)` |
| `rt_file_modified_date` | `i64 (ptr, i32)` |
| `rt_file_owner_name` | `i64 (ptr, i32)` |
| `rt_file_rename` | `i64 (ptr, i32)` |
| `rt_file_truename` | `i64 (ptr, i32)` |
| `rt_float_to_int` | `i64 (ptr, i32)` |
| `rt_float_to_ratio` | `i64 (ptr, i32)` |
| `rt_format` | `i64 (ptr, i32)` |
| `rt_format_call_method` | `i64 (ptr, i32)` |
| `rt_frame_apply` | `i64 (ptr, i32)` |
| `rt_frame_call` | `i64 (ptr, i32)` |
| `rt_frame_call_env` | `i64 (ptr, i32)` |
| `rt_frame_dyn_call` | `i64 (ptr, i32)` |
| `rt_frame_open` | `i64 (ptr, i32)` |
| `rt_get_dispatch_macro_character` | `i64 (ptr, i32)` |
| `rt_get_internal_real_time` | `i64 (ptr, i32)` |
| `rt_get_internal_run_time` | `i64 (ptr, i32)` |
| `rt_get_macro_character` | `i64 (ptr, i32)` |
| `rt_get_universal_time` | `i64 (ptr, i32)` |
| `rt_getenv` | `i64 (ptr, i32)` |
| `rt_global_get` | `i64 (ptr, i32)` |
| `rt_global_new` | `i64 (ptr, i32)` |
| `rt_global_set` | `i64 (ptr, i32)` |
| `rt_hashtable_bucket_count` | `i64 (ptr, i32)` |
| `rt_hashtable_bucket_delete` | `i64 (ptr, i32)` |
| `rt_hashtable_bucket_key` | `i64 (ptr, i32)` |
| `rt_hashtable_bucket_put` | `i64 (ptr, i32)` |
| `rt_hashtable_bucket_value` | `i64 (ptr, i32)` |
| `rt_hashtable_clear` | `i64 (ptr, i32)` |
| `rt_hashtable_count` | `i64 (ptr, i32)` |
| `rt_hashtable_entries` | `i64 (ptr, i32)` |
| `rt_hashtable_keys` | `i64 (ptr, i32)` |
| `rt_hashtable_new` | `i64 (ptr, i32)` |
| `rt_hashtable_values` | `i64 (ptr, i32)` |
| `rt_heap_info` | `i64 (ptr, i32)` |
| `rt_heap_init` | `i64 (ptr, i32)` |
| `rt_home_directory` | `i64 (ptr, i32)` |
| `rt_int_ash` | `i64 (ptr, i32)` |
| `rt_int_div` | `i64 (ptr, i32)` |
| `rt_int_fits` | `i64 (ptr, i32)` |
| `rt_int_fits_char` | `i64 (ptr, i32)` |
| `rt_int_integer_length` | `i64 (ptr, i32)` |
| `rt_int_logbitp` | `i64 (ptr, i32)` |
| `rt_int_logcount` | `i64 (ptr, i32)` |
| `rt_int_mod` | `i64 (ptr, i32)` |
| `rt_int_not_fixnum` | `i64 (ptr, i32)` |
| `rt_int_to_char` | `i64 (ptr, i32)` |
| `rt_int_to_ratio` | `i64 (ptr, i32)` |
| `rt_integer_add` | `i64 (ptr, i32)` |
| `rt_integer_ash` | `i64 (ptr, i32)` |
| `rt_integer_cmp` | `i64 (ptr, i32)` |
| `rt_integer_div` | `i64 (ptr, i32)` |
| `rt_integer_fits` | `i64 (ptr, i32)` |
| `rt_integer_fits_char` | `i64 (ptr, i32)` |
| `rt_integer_from_word` | `i64 (ptr, i32)` |
| `rt_integer_integer_length` | `i64 (ptr, i32)` |
| `rt_integer_logand` | `i64 (ptr, i32)` |
| `rt_integer_logbitp` | `i64 (ptr, i32)` |
| `rt_integer_logcount` | `i64 (ptr, i32)` |
| `rt_integer_logior` | `i64 (ptr, i32)` |
| `rt_integer_lognot` | `i64 (ptr, i32)` |
| `rt_integer_logtest` | `i64 (ptr, i32)` |
| `rt_integer_logxor` | `i64 (ptr, i32)` |
| `rt_integer_mod` | `i64 (ptr, i32)` |
| `rt_integer_mul` | `i64 (ptr, i32)` |
| `rt_integer_narrow` | `i64 (ptr, i32)` |
| `rt_integer_sub` | `i64 (ptr, i32)` |
| `rt_integer_to_char` | `i64 (ptr, i32)` |
| `rt_integer_to_float` | `i64 (ptr, i32)` |
| `rt_integer_to_ratio` | `i64 (ptr, i32)` |
| `rt_intern` | `i64 (ptr, i32)` |
| `rt_intern_path` | `i64 (ptr, i32)` |
| `rt_intern_symbol` | `i64 (ptr, i32)` |
| `rt_keywordp` | `i64 (ptr, i32)` |
| `rt_lisp_implementation_version` | `i64 (ptr, i32)` |
| `rt_list_to_path` | `i64 (ptr, i32)` |
| `rt_list_to_sexpr_tuple` | `i64 (ptr, i32)` |
| `rt_list_to_sexpr_vector` | `i64 (ptr, i32)` |
| `rt_lists_to_sexpr_array` | `i64 (ptr, i32)` |
| `rt_llvm_call` | `i64 (ptr, i32)` |
| `rt_loop_safepoint` | `i64 (ptr, i32)` |
| `rt_machine_instance` | `i64 (ptr, i32)` |
| `rt_machine_type` | `i64 (ptr, i32)` |
| `rt_machine_version` | `i64 (ptr, i32)` |
| `rt_macroexpand` | `i64 (ptr, i32)` |
| `rt_macroexpand_1` | `i64 (ptr, i32)` |
| `rt_make_random_state_fresh` | `i64 (ptr, i32)` |
| `rt_match_fail` | `i64 (ptr, i32)` |
| `rt_narrow_new` | `i64 (ptr, i32)` |
| `rt_narrow_value` | `i64 (ptr, i32)` |
| `rt_net_accept` | `i64 (ptr, i32)` |
| `rt_net_buffered_p` | `i64 (ptr, i32)` |
| `rt_net_connect_begin` | `i64 (ptr, i32)` |
| `rt_net_connect_finish` | `i64 (ptr, i32)` |
| `rt_net_fill` | `i64 (ptr, i32)` |
| `rt_net_flush` | `i64 (ptr, i32)` |
| `rt_net_listen` | `i64 (ptr, i32)` |
| `rt_net_local_address` | `i64 (ptr, i32)` |
| `rt_net_peer_address` | `i64 (ptr, i32)` |
| `rt_net_peer_subject` | `i64 (ptr, i32)` |
| `rt_net_pop_byte` | `i64 (ptr, i32)` |
| `rt_net_pop_char` | `i64 (ptr, i32)` |
| `rt_net_push_byte` | `i64 (ptr, i32)` |
| `rt_net_push_string` | `i64 (ptr, i32)` |
| `rt_net_resolve_begin` | `i64 (ptr, i32)` |
| `rt_net_resolve_finish` | `i64 (ptr, i32)` |
| `rt_net_server_name` | `i64 (ptr, i32)` |
| `rt_net_set_keepalive` | `i64 (ptr, i32)` |
| `rt_net_set_keepalive_period` | `i64 (ptr, i32)` |
| `rt_net_set_nodelay` | `i64 (ptr, i32)` |
| `rt_net_shutdown_write` | `i64 (ptr, i32)` |
| `rt_net_socket_error` | `i64 (ptr, i32)` |
| `rt_net_tls_add_certificate` | `i64 (ptr, i32)` |
| `rt_net_tls_handshake` | `i64 (ptr, i32)` |
| `rt_net_tls_listen` | `i64 (ptr, i32)` |
| `rt_net_tls_start` | `i64 (ptr, i32)` |
| `rt_net_udp_bind` | `i64 (ptr, i32)` |
| `rt_net_udp_last_sender` | `i64 (ptr, i32)` |
| `rt_net_udp_recv` | `i64 (ptr, i32)` |
| `rt_net_udp_send_to` | `i64 (ptr, i32)` |
| `rt_net_unix_connect` | `i64 (ptr, i32)` |
| `rt_net_unix_listen` | `i64 (ptr, i32)` |
| `rt_null` | `i64 (ptr, i32)` |
| `rt_panic` | `i64 (ptr, i32)` |
| `rt_parse_float` | `i64 (ptr, i32)` |
| `rt_path_to_list` | `i64 (ptr, i32)` |
| `rt_pending_arg` | `i64 (ptr, i32)` |
| `rt_pending_argc` | `i64 (ptr, i32)` |
| `rt_pending_env` | `i64 (ptr, i32)` |
| `rt_pending_envc` | `i64 (ptr, i32)` |
| `rt_pop_sexpr_root` | `i64 (ptr, i32)` |
| `rt_pprint` | `i64 (ptr, i32)` |
| `rt_pprint_block_end` | `i64 (ptr, i32)` |
| `rt_pprint_block_start` | `i64 (ptr, i32)` |
| `rt_pprint_indent` | `i64 (ptr, i32)` |
| `rt_pprint_list_exhausted` | `i64 (ptr, i32)` |
| `rt_pprint_newline` | `i64 (ptr, i32)` |
| `rt_pprint_pop` | `i64 (ptr, i32)` |
| `rt_pprint_tab` | `i64 (ptr, i32)` |
| `rt_print` | `i64 (ptr, i32)` |
| `rt_print_enum_variant` | `i64 (ptr, i32)` |
| `rt_print_field_name` | `i64 (ptr, i32)` |
| `rt_print_field_template` | `i64 (ptr, i32)` |
| `rt_print_global` | `i64 (ptr, i32)` |
| `rt_print_name_import` | `i64 (ptr, i32)` |
| `rt_print_name_module` | `i64 (ptr, i32)` |
| `rt_print_object_method` | `i64 (ptr, i32)` |
| `rt_println` | `i64 (ptr, i32)` |
| `rt_push_permanent_sexpr_root` | `i64 (ptr, i32)` |
| `rt_push_sexpr_root` | `i64 (ptr, i32)` |
| `rt_random_state_copy` | `i64 (ptr, i32)` |
| `rt_random_state_next` | `i64 (ptr, i32)` |
| `rt_ratio_add` | `i64 (ptr, i32)` |
| `rt_ratio_cmp` | `i64 (ptr, i32)` |
| `rt_ratio_denominator` | `i64 (ptr, i32)` |
| `rt_ratio_div` | `i64 (ptr, i32)` |
| `rt_ratio_from_bignums` | `i64 (ptr, i32)` |
| `rt_ratio_mul` | `i64 (ptr, i32)` |
| `rt_ratio_numerator` | `i64 (ptr, i32)` |
| `rt_ratio_sub` | `i64 (ptr, i32)` |
| `rt_ratio_to_float` | `i64 (ptr, i32)` |
| `rt_ratio_to_int` | `i64 (ptr, i32)` |
| `rt_rational_add` | `i64 (ptr, i32)` |
| `rt_rational_cmp` | `i64 (ptr, i32)` |
| `rt_rational_denominator` | `i64 (ptr, i32)` |
| `rt_rational_div` | `i64 (ptr, i32)` |
| `rt_rational_from_ratio` | `i64 (ptr, i32)` |
| `rt_rational_is_int` | `i64 (ptr, i32)` |
| `rt_rational_mul` | `i64 (ptr, i32)` |
| `rt_rational_numerator` | `i64 (ptr, i32)` |
| `rt_rational_sub` | `i64 (ptr, i32)` |
| `rt_rational_to_float` | `i64 (ptr, i32)` |
| `rt_rational_to_int` | `i64 (ptr, i32)` |
| `rt_rational_to_ratio` | `i64 (ptr, i32)` |
| `rt_read` | `i64 (ptr, i32)` |
| `rt_read_datum_at` | `i64 (ptr, i32)` |
| `rt_root_count` | `i64 (ptr, i32)` |
| `rt_run_entry` | `i64 (i64)` |
| `rt_seed_random_state` | `i64 (ptr, i32)` |
| `rt_set_car` | `i64 (ptr, i32)` |
| `rt_set_cdr` | `i64 (ptr, i32)` |
| `rt_set_dispatch_macro_character` | `i64 (ptr, i32)` |
| `rt_set_macro_character` | `i64 (ptr, i32)` |
| `rt_set_sexpr_root` | `i64 (ptr, i32)` |
| `rt_sexpr_bool` | `i64 (ptr, i32)` |
| `rt_sexpr_char` | `i64 (ptr, i32)` |
| `rt_sexpr_eql` | `i64 (ptr, i32)` |
| `rt_sexpr_equal` | `i64 (ptr, i32)` |
| `rt_sexpr_equalp` | `i64 (ptr, i32)` |
| `rt_sexpr_f32` | `i64 (ptr, i32)` |
| `rt_sexpr_f64` | `i64 (ptr, i32)` |
| `rt_sexpr_i16` | `i64 (ptr, i32)` |
| `rt_sexpr_i32` | `i64 (ptr, i32)` |
| `rt_sexpr_i8` | `i64 (ptr, i32)` |
| `rt_sexpr_instance_test` | `i64 (ptr, i32)` |
| `rt_sexpr_int` | `i64 (ptr, i32)` |
| `rt_sexpr_str` | `i64 (ptr, i32)` |
| `rt_sexpr_tuple_items` | `i64 (ptr, i32)` |
| `rt_sexpr_u16` | `i64 (ptr, i32)` |
| `rt_sexpr_u32` | `i64 (ptr, i32)` |
| `rt_sexpr_u8` | `i64 (ptr, i32)` |
| `rt_software_type` | `i64 (ptr, i32)` |
| `rt_software_version` | `i64 (ptr, i32)` |
| `rt_str_append` | `i64 (ptr, i32)` |
| `rt_str_downcase` | `i64 (ptr, i32)` |
| `rt_str_eq` | `i64 (ptr, i32)` |
| `rt_str_equalp` | `i64 (ptr, i32)` |
| `rt_str_length` | `i64 (ptr, i32)` |
| `rt_str_lt` | `i64 (ptr, i32)` |
| `rt_str_new` | `i64 (ptr, i32)` |
| `rt_str_ref` | `i64 (ptr, i32)` |
| `rt_str_substring` | `i64 (ptr, i32)` |
| `rt_str_upcase` | `i64 (ptr, i32)` |
| `rt_stream_at_line_start` | `i64 (ptr, i32)` |
| `rt_stream_close` | `i64 (ptr, i32)` |
| `rt_stream_describe` | `i64 (ptr, i32)` |
| `rt_stream_finish_output` | `i64 (ptr, i32)` |
| `rt_stream_input_p` | `i64 (ptr, i32)` |
| `rt_stream_listen` | `i64 (ptr, i32)` |
| `rt_stream_open_file` | `i64 (ptr, i32)` |
| `rt_stream_open_p` | `i64 (ptr, i32)` |
| `rt_stream_output_p` | `i64 (ptr, i32)` |
| `rt_stream_position` | `i64 (ptr, i32)` |
| `rt_stream_read_byte` | `i64 (ptr, i32)` |
| `rt_stream_read_char` | `i64 (ptr, i32)` |
| `rt_stream_stderr` | `i64 (ptr, i32)` |
| `rt_stream_stdin` | `i64 (ptr, i32)` |
| `rt_stream_stdout` | `i64 (ptr, i32)` |
| `rt_stream_string_input` | `i64 (ptr, i32)` |
| `rt_stream_string_output` | `i64 (ptr, i32)` |
| `rt_stream_take_output_string` | `i64 (ptr, i32)` |
| `rt_stream_unread_char` | `i64 (ptr, i32)` |
| `rt_stream_write_byte` | `i64 (ptr, i32)` |
| `rt_stream_write_string` | `i64 (ptr, i32)` |
| `rt_struct_field_count` | `i64 (ptr, i32)` |
| `rt_struct_field_get` | `i64 (ptr, i32)` |
| `rt_struct_field_set` | `i64 (ptr, i32)` |
| `rt_struct_new` | `i64 (ptr, i32)` |
| `rt_struct_pop_field` | `i64 (ptr, i32)` |
| `rt_struct_push_field` | `i64 (ptr, i32)` |
| `rt_suspend_chan_cap` | `i64 (ptr, i32)` |
| `rt_suspend_chan_close` | `i64 (ptr, i32)` |
| `rt_suspend_chan_len` | `i64 (ptr, i32)` |
| `rt_suspend_chan_new` | `i64 (ptr, i32)` |
| `rt_suspend_chan_recv` | `i64 (ptr, i32)` |
| `rt_suspend_chan_select` | `i64 (ptr, i32)` |
| `rt_suspend_chan_send` | `i64 (ptr, i32)` |
| `rt_suspend_io` | `i64 (ptr, i32)` |
| `rt_suspend_io_for` | `i64 (ptr, i32)` |
| `rt_suspend_main` | `i64 (ptr, i32)` |
| `rt_suspend_sleep` | `i64 (ptr, i32)` |
| `rt_suspend_task` | `i64 (ptr, i32)` |
| `rt_suspend_thread` | `i64 (ptr, i32)` |
| `rt_suspend_wait` | `i64 (ptr, i32)` |
| `rt_suspend_yield` | `i64 (ptr, i32)` |
| `rt_sym_name` | `i64 (ptr, i32)` |
| `rt_symp` | `i64 (ptr, i32)` |
| `rt_thread_available_parallelism` | `i64 (ptr, i32)` |
| `rt_thread_current_id` | `i64 (ptr, i32)` |
| `rt_throw` | `i64 (ptr, i32)` |
| `rt_throw_matches` | `i64 (ptr, i32)` |
| `rt_throw_take_value` | `i64 (ptr, i32)` |
| `rt_timezone_daylight_p` | `i64 (ptr, i32)` |
| `rt_timezone_offset_seconds` | `i64 (ptr, i32)` |
| `rt_truncate_sexpr_roots` | `i64 (ptr, i32)` |
| `rt_types_define` | `i64 (ptr, i32)` |
| `rt_upcast_set` | `i64 (ptr, i32)` |
| `rt_vector_to_sexpr_tuple` | `i64 (ptr, i32)` |
| `rt_vtable_set` | `i64 (ptr, i32)` |
| `rt_wk_symbol` | `i64 (ptr, i32)` |

## 組み込み型

### `std::EvalError` (sum, 型引数 [])

- 変種 0 `EvalError`: `string`

### `std::FileError` (sum, 型引数 [])

- 変種 0 `FileError`: `string`

### `std::HashTable` (sum, 型引数 [K, V])

- インスタンス `bucket-count`: `(std::HashTable<K,V>, int) -> int`
- インスタンス `bucket-delete`: `(std::HashTable<K,V>, int, int) -> ()`
- インスタンス `bucket-key`: `(std::HashTable<K,V>, int, int) -> K`
- インスタンス `bucket-put`: `(std::HashTable<K,V>, int, int, K, V) -> ()`
- インスタンス `bucket-value`: `(std::HashTable<K,V>, int, int) -> V`
- インスタンス `clear`: `(std::HashTable<K,V>) -> ()`
- インスタンス `count`: `(std::HashTable<K,V>) -> int`
- インスタンス `entries`: `(std::HashTable<K,V>) -> std::Vector<std::cons-cell<K,V>>`
- インスタンス `keys`: `(std::HashTable<K,V>) -> std::Vector<K>`
- 関連関数 `new`: `() -> std::HashTable<K,V>`
- インスタンス `values`: `(std::HashTable<K,V>) -> std::Vector<V>`

### `std::NetError` (sum, 型引数 [])

- 変種 0 `NetError`: `string`

### `std::Option` (sum, 型引数 [T])

- 変種 0 `some`: `T`
- 変種 1 `none`: 

### `std::ParseFloatError` (sum, 型引数 [])

- 変種 0 `ParseFloatError`: `string`

### `std::ParseIntError` (sum, 型引数 [])

- 変種 0 `ParseIntError`: `string`

### `std::ReadError` (sum, 型引数 [])

- 変種 0 `ReadError`: `string`

### `std::Result` (sum, 型引数 [T, E])

- 変種 0 `ok`: `T`
- 変種 1 `err`: `E`

### `std::Scope` (sum, 型引数 [V])

- インスタンス `clone-frames`: `(std::Scope<V>) -> std::Scope<V>`
- インスタンス `get`: `(std::Scope<V>, string) -> std::Option<V>`
- 関連関数 `new`: `() -> std::Scope<V>`
- インスタンス `pop-frame`: `(std::Scope<V>) -> ()`
- インスタンス `push-frame`: `(std::Scope<V>) -> ()`
- インスタンス `set`: `(std::Scope<V>, string, V) -> ()`

### `std::Sexpr` (sum, 型引数 [])

- 変種 0 `nil`: 
- 変種 1 `int`: `int`
- 変種 2 `f64`: `f64`
- 変種 3 `char`: `char`
- 変種 4 `bool`: `bool`
- 変種 5 `sym`: `symbol`
- 変種 6 `str`: `string`
- 変種 7 `cons`: `std::Option<std::Sexpr>`, `std::Option<std::Sexpr>`
- 変種 8 `bignum`: `int`
- 変種 9 `ratio`: `ratio`
- 変種 10 `path`: `std::Option<std::Sexpr>`
- 変種 11 `f32`: `f32`
- 変種 12 `i8`: `i8`
- 変種 13 `i16`: `i16`
- 変種 14 `u8`: `u8`
- 変種 15 `u16`: `u16`
- 変種 16 `u32`: `u32`
- 変種 17 `i32`: `i32`
- 変種 18 `vector`: `std::Vector<std::Option<std::Sexpr>>`
- 変種 19 `array`: `std::array<std::Option<std::Sexpr>>`
- 変種 20 `tuple`: `std::Vector<std::Option<std::Sexpr>>`
- インスタンス `eq`: `(std::Option<std::Sexpr>, std::Option<std::Sexpr>) -> bool`
- インスタンス `eql`: `(std::Option<std::Sexpr>, std::Option<std::Sexpr>) -> bool`

### `std::Vector` (struct, 型引数 [T])

- インスタンス `get`: `(std::Vector<T>, int) -> T`
- インスタンス `len`: `(std::Vector<T>) -> int`
- 関連関数 `new`: `() -> std::Vector<T>`
- インスタンス `pop`: `(std::Vector<T>) -> std::Option<T>`
- インスタンス `push`: `(std::Vector<T>, T) -> ()`
- インスタンス `set`: `(std::Vector<T>, int, T) -> ()`

### `std::bool` (sum, 型引数 [])

- インスタンス `eq`: `(bool, bool) -> bool`
- インスタンス `eql`: `(bool, bool) -> bool`
- インスタンス `equal`: `(bool, bool) -> bool`
- インスタンス `equalp`: `(bool, bool) -> bool`

### `std::c-long` (sum, 型引数 [])

- インスタンス `int->c-long`: `(c-long) -> c-long`
- インスタンス `int->c-ulong`: `(c-long) -> c-ulong`
- インスタンス `int->i16`: `(c-long) -> i16`
- インスタンス `int->i32`: `(c-long) -> i32`
- インスタンス `int->i8`: `(c-long) -> i8`
- インスタンス `int->int`: `(c-long) -> int`
- インスタンス `int->u16`: `(c-long) -> u16`
- インスタンス `int->u32`: `(c-long) -> u32`
- インスタンス `int->u8`: `(c-long) -> u8`
- インスタンス `try-int->c-long`: `(c-long) -> std::Option<c-long>`
- インスタンス `try-int->c-ulong`: `(c-long) -> std::Option<c-ulong>`
- インスタンス `try-int->i16`: `(c-long) -> std::Option<i16>`
- インスタンス `try-int->i32`: `(c-long) -> std::Option<i32>`
- インスタンス `try-int->i8`: `(c-long) -> std::Option<i8>`
- インスタンス `try-int->u16`: `(c-long) -> std::Option<u16>`
- インスタンス `try-int->u32`: `(c-long) -> std::Option<u32>`
- インスタンス `try-int->u8`: `(c-long) -> std::Option<u8>`

### `std::c-ulong` (sum, 型引数 [])

- インスタンス `int->c-long`: `(c-ulong) -> c-long`
- インスタンス `int->c-ulong`: `(c-ulong) -> c-ulong`
- インスタンス `int->i16`: `(c-ulong) -> i16`
- インスタンス `int->i32`: `(c-ulong) -> i32`
- インスタンス `int->i8`: `(c-ulong) -> i8`
- インスタンス `int->int`: `(c-ulong) -> int`
- インスタンス `int->u16`: `(c-ulong) -> u16`
- インスタンス `int->u32`: `(c-ulong) -> u32`
- インスタンス `int->u8`: `(c-ulong) -> u8`
- インスタンス `try-int->c-long`: `(c-ulong) -> std::Option<c-long>`
- インスタンス `try-int->c-ulong`: `(c-ulong) -> std::Option<c-ulong>`
- インスタンス `try-int->i16`: `(c-ulong) -> std::Option<i16>`
- インスタンス `try-int->i32`: `(c-ulong) -> std::Option<i32>`
- インスタンス `try-int->i8`: `(c-ulong) -> std::Option<i8>`
- インスタンス `try-int->u16`: `(c-ulong) -> std::Option<u16>`
- インスタンス `try-int->u32`: `(c-ulong) -> std::Option<u32>`
- インスタンス `try-int->u8`: `(c-ulong) -> std::Option<u8>`

### `std::char` (sum, 型引数 [])

- インスタンス `<`: `(char, char) -> bool`
- インスタンス `<=`: `(char, char) -> bool`
- インスタンス `>`: `(char, char) -> bool`
- インスタンス `>=`: `(char, char) -> bool`
- インスタンス `alphap`: `(char) -> bool`
- インスタンス `char->int`: `(char) -> int`
- インスタンス `char->string`: `(char) -> string`
- インスタンス `digitp`: `(char) -> bool`
- インスタンス `downcase`: `(char) -> char`
- インスタンス `eq`: `(char, char) -> bool`
- インスタンス `eql`: `(char, char) -> bool`
- インスタンス `equal`: `(char, char) -> bool`
- インスタンス `equalp`: `(char, char) -> bool`
- インスタンス `lt`: `(char, char) -> bool`
- インスタンス `upcase`: `(char) -> char`

### `std::f32` (sum, 型引数 [])

- インスタンス `*`: `(f32, f32) -> f32`
- インスタンス `+`: `(f32, f32) -> f32`
- インスタンス `-`: `(f32, f32) -> f32`
- インスタンス `/`: `(f32, f32) -> f32`
- インスタンス `/=`: `(f32, f32) -> bool`
- インスタンス `<`: `(f32, f32) -> bool`
- インスタンス `<=`: `(f32, f32) -> bool`
- インスタンス `=`: `(f32, f32) -> bool`
- インスタンス `>`: `(f32, f32) -> bool`
- インスタンス `>=`: `(f32, f32) -> bool`
- インスタンス `acos`: `(f32) -> f32`
- インスタンス `acosh`: `(f32) -> f32`
- インスタンス `asin`: `(f32) -> f32`
- インスタンス `asinh`: `(f32) -> f32`
- インスタンス `atan`: `(f32) -> f32`
- インスタンス `atanh`: `(f32) -> f32`
- インスタンス `ceiling`: `(f32) -> f32`
- インスタンス `cos`: `(f32) -> f32`
- インスタンス `cosh`: `(f32) -> f32`
- インスタンス `eq`: `(f32, f32) -> bool`
- インスタンス `eql`: `(f32, f32) -> bool`
- インスタンス `equal`: `(f32, f32) -> bool`
- インスタンス `equalp`: `(f32, f32) -> bool`
- インスタンス `exp`: `(f32) -> f32`
- インスタンス `expt`: `(f32, f32) -> f32`
- インスタンス `float->f32`: `(f32) -> f32`
- インスタンス `float->f64`: `(f32) -> f64`
- インスタンス `float->int`: `(f32) -> int`
- インスタンス `float->ratio`: `(f32) -> ratio`
- インスタンス `floor`: `(f32) -> f32`
- インスタンス `log`: `(f32) -> f32`
- インスタンス `max`: `(f32, f32) -> f32`
- インスタンス `min`: `(f32, f32) -> f32`
- インスタンス `round`: `(f32) -> f32`
- インスタンス `sin`: `(f32) -> f32`
- インスタンス `sinh`: `(f32) -> f32`
- インスタンス `sqrt`: `(f32) -> f32`
- インスタンス `tan`: `(f32) -> f32`
- インスタンス `tanh`: `(f32) -> f32`
- インスタンス `truncate`: `(f32) -> f32`
- インスタンス `try-float->f32`: `(f32) -> std::Option<f32>`
- インスタンス `try-float->f64`: `(f32) -> std::Option<f64>`

### `std::f64` (sum, 型引数 [])

- インスタンス `*`: `(f64, f64) -> f64`
- インスタンス `+`: `(f64, f64) -> f64`
- インスタンス `-`: `(f64, f64) -> f64`
- インスタンス `/`: `(f64, f64) -> f64`
- インスタンス `/=`: `(f64, f64) -> bool`
- インスタンス `<`: `(f64, f64) -> bool`
- インスタンス `<=`: `(f64, f64) -> bool`
- インスタンス `=`: `(f64, f64) -> bool`
- インスタンス `>`: `(f64, f64) -> bool`
- インスタンス `>=`: `(f64, f64) -> bool`
- インスタンス `acos`: `(f64) -> f64`
- インスタンス `acosh`: `(f64) -> f64`
- インスタンス `asin`: `(f64) -> f64`
- インスタンス `asinh`: `(f64) -> f64`
- インスタンス `atan`: `(f64) -> f64`
- インスタンス `atanh`: `(f64) -> f64`
- インスタンス `ceiling`: `(f64) -> f64`
- インスタンス `cos`: `(f64) -> f64`
- インスタンス `cosh`: `(f64) -> f64`
- インスタンス `eq`: `(f64, f64) -> bool`
- インスタンス `eql`: `(f64, f64) -> bool`
- インスタンス `equal`: `(f64, f64) -> bool`
- インスタンス `equalp`: `(f64, f64) -> bool`
- インスタンス `exp`: `(f64) -> f64`
- インスタンス `expt`: `(f64, f64) -> f64`
- インスタンス `float->f32`: `(f64) -> f32`
- インスタンス `float->f64`: `(f64) -> f64`
- インスタンス `float->int`: `(f64) -> int`
- インスタンス `float->ratio`: `(f64) -> ratio`
- インスタンス `floor`: `(f64) -> f64`
- インスタンス `log`: `(f64) -> f64`
- インスタンス `max`: `(f64, f64) -> f64`
- インスタンス `min`: `(f64, f64) -> f64`
- インスタンス `round`: `(f64) -> f64`
- インスタンス `sin`: `(f64) -> f64`
- インスタンス `sinh`: `(f64) -> f64`
- インスタンス `sqrt`: `(f64) -> f64`
- インスタンス `tan`: `(f64) -> f64`
- インスタンス `tanh`: `(f64) -> f64`
- インスタンス `truncate`: `(f64) -> f64`
- インスタンス `try-float->f32`: `(f64) -> std::Option<f32>`
- インスタンス `try-float->f64`: `(f64) -> std::Option<f64>`

### `std::i16` (sum, 型引数 [])

- インスタンス `*`: `(i16, i16) -> i16`
- インスタンス `+`: `(i16, i16) -> i16`
- インスタンス `-`: `(i16, i16) -> i16`
- インスタンス `/`: `(i16, i16) -> i16`
- インスタンス `/=`: `(i16, i16) -> bool`
- インスタンス `<`: `(i16, i16) -> bool`
- インスタンス `<=`: `(i16, i16) -> bool`
- インスタンス `=`: `(i16, i16) -> bool`
- インスタンス `>`: `(i16, i16) -> bool`
- インスタンス `>=`: `(i16, i16) -> bool`
- インスタンス `ash`: `(i16, int) -> i16`
- インスタンス `eq`: `(i16, i16) -> bool`
- インスタンス `eql`: `(i16, i16) -> bool`
- インスタンス `equal`: `(i16, i16) -> bool`
- インスタンス `equalp`: `(i16, i16) -> bool`
- インスタンス `int->c-long`: `(i16) -> c-long`
- インスタンス `int->c-ulong`: `(i16) -> c-ulong`
- インスタンス `int->char`: `(i16) -> char`
- インスタンス `int->float`: `(i16) -> f64`
- インスタンス `int->i16`: `(i16) -> i16`
- インスタンス `int->i32`: `(i16) -> i32`
- インスタンス `int->i8`: `(i16) -> i8`
- インスタンス `int->int`: `(i16) -> int`
- インスタンス `int->ratio`: `(i16) -> ratio`
- インスタンス `int->u16`: `(i16) -> u16`
- インスタンス `int->u32`: `(i16) -> u32`
- インスタンス `int->u8`: `(i16) -> u8`
- インスタンス `integer-length`: `(i16) -> i16`
- インスタンス `logand`: `(i16, i16) -> i16`
- インスタンス `logbitp`: `(i16, int) -> bool`
- インスタンス `logcount`: `(i16) -> i16`
- インスタンス `logior`: `(i16, i16) -> i16`
- インスタンス `lognot`: `(i16) -> i16`
- インスタンス `logtest`: `(i16, i16) -> bool`
- インスタンス `logxor`: `(i16, i16) -> i16`
- インスタンス `max`: `(i16, i16) -> i16`
- インスタンス `min`: `(i16, i16) -> i16`
- インスタンス `mod`: `(i16, i16) -> i16`
- インスタンス `try-int->c-long`: `(i16) -> std::Option<c-long>`
- インスタンス `try-int->c-ulong`: `(i16) -> std::Option<c-ulong>`
- インスタンス `try-int->char`: `(i16) -> std::Option<char>`
- インスタンス `try-int->i16`: `(i16) -> std::Option<i16>`
- インスタンス `try-int->i32`: `(i16) -> std::Option<i32>`
- インスタンス `try-int->i8`: `(i16) -> std::Option<i8>`
- インスタンス `try-int->u16`: `(i16) -> std::Option<u16>`
- インスタンス `try-int->u32`: `(i16) -> std::Option<u32>`
- インスタンス `try-int->u8`: `(i16) -> std::Option<u8>`

### `std::i32` (sum, 型引数 [])

- インスタンス `*`: `(i32, i32) -> i32`
- インスタンス `+`: `(i32, i32) -> i32`
- インスタンス `-`: `(i32, i32) -> i32`
- インスタンス `/`: `(i32, i32) -> i32`
- インスタンス `/=`: `(i32, i32) -> bool`
- インスタンス `<`: `(i32, i32) -> bool`
- インスタンス `<=`: `(i32, i32) -> bool`
- インスタンス `=`: `(i32, i32) -> bool`
- インスタンス `>`: `(i32, i32) -> bool`
- インスタンス `>=`: `(i32, i32) -> bool`
- インスタンス `ash`: `(i32, int) -> i32`
- インスタンス `eq`: `(i32, i32) -> bool`
- インスタンス `eql`: `(i32, i32) -> bool`
- インスタンス `equal`: `(i32, i32) -> bool`
- インスタンス `equalp`: `(i32, i32) -> bool`
- インスタンス `int->c-long`: `(i32) -> c-long`
- インスタンス `int->c-ulong`: `(i32) -> c-ulong`
- インスタンス `int->char`: `(i32) -> char`
- インスタンス `int->float`: `(i32) -> f64`
- インスタンス `int->i16`: `(i32) -> i16`
- インスタンス `int->i32`: `(i32) -> i32`
- インスタンス `int->i8`: `(i32) -> i8`
- インスタンス `int->int`: `(i32) -> int`
- インスタンス `int->ratio`: `(i32) -> ratio`
- インスタンス `int->u16`: `(i32) -> u16`
- インスタンス `int->u32`: `(i32) -> u32`
- インスタンス `int->u8`: `(i32) -> u8`
- インスタンス `integer-length`: `(i32) -> i32`
- インスタンス `logand`: `(i32, i32) -> i32`
- インスタンス `logbitp`: `(i32, int) -> bool`
- インスタンス `logcount`: `(i32) -> i32`
- インスタンス `logior`: `(i32, i32) -> i32`
- インスタンス `lognot`: `(i32) -> i32`
- インスタンス `logtest`: `(i32, i32) -> bool`
- インスタンス `logxor`: `(i32, i32) -> i32`
- インスタンス `max`: `(i32, i32) -> i32`
- インスタンス `min`: `(i32, i32) -> i32`
- インスタンス `mod`: `(i32, i32) -> i32`
- インスタンス `try-int->c-long`: `(i32) -> std::Option<c-long>`
- インスタンス `try-int->c-ulong`: `(i32) -> std::Option<c-ulong>`
- インスタンス `try-int->char`: `(i32) -> std::Option<char>`
- インスタンス `try-int->i16`: `(i32) -> std::Option<i16>`
- インスタンス `try-int->i32`: `(i32) -> std::Option<i32>`
- インスタンス `try-int->i8`: `(i32) -> std::Option<i8>`
- インスタンス `try-int->u16`: `(i32) -> std::Option<u16>`
- インスタンス `try-int->u32`: `(i32) -> std::Option<u32>`
- インスタンス `try-int->u8`: `(i32) -> std::Option<u8>`

### `std::i8` (sum, 型引数 [])

- インスタンス `*`: `(i8, i8) -> i8`
- インスタンス `+`: `(i8, i8) -> i8`
- インスタンス `-`: `(i8, i8) -> i8`
- インスタンス `/`: `(i8, i8) -> i8`
- インスタンス `/=`: `(i8, i8) -> bool`
- インスタンス `<`: `(i8, i8) -> bool`
- インスタンス `<=`: `(i8, i8) -> bool`
- インスタンス `=`: `(i8, i8) -> bool`
- インスタンス `>`: `(i8, i8) -> bool`
- インスタンス `>=`: `(i8, i8) -> bool`
- インスタンス `ash`: `(i8, int) -> i8`
- インスタンス `eq`: `(i8, i8) -> bool`
- インスタンス `eql`: `(i8, i8) -> bool`
- インスタンス `equal`: `(i8, i8) -> bool`
- インスタンス `equalp`: `(i8, i8) -> bool`
- インスタンス `int->c-long`: `(i8) -> c-long`
- インスタンス `int->c-ulong`: `(i8) -> c-ulong`
- インスタンス `int->char`: `(i8) -> char`
- インスタンス `int->float`: `(i8) -> f64`
- インスタンス `int->i16`: `(i8) -> i16`
- インスタンス `int->i32`: `(i8) -> i32`
- インスタンス `int->i8`: `(i8) -> i8`
- インスタンス `int->int`: `(i8) -> int`
- インスタンス `int->ratio`: `(i8) -> ratio`
- インスタンス `int->u16`: `(i8) -> u16`
- インスタンス `int->u32`: `(i8) -> u32`
- インスタンス `int->u8`: `(i8) -> u8`
- インスタンス `integer-length`: `(i8) -> i8`
- インスタンス `logand`: `(i8, i8) -> i8`
- インスタンス `logbitp`: `(i8, int) -> bool`
- インスタンス `logcount`: `(i8) -> i8`
- インスタンス `logior`: `(i8, i8) -> i8`
- インスタンス `lognot`: `(i8) -> i8`
- インスタンス `logtest`: `(i8, i8) -> bool`
- インスタンス `logxor`: `(i8, i8) -> i8`
- インスタンス `max`: `(i8, i8) -> i8`
- インスタンス `min`: `(i8, i8) -> i8`
- インスタンス `mod`: `(i8, i8) -> i8`
- インスタンス `try-int->c-long`: `(i8) -> std::Option<c-long>`
- インスタンス `try-int->c-ulong`: `(i8) -> std::Option<c-ulong>`
- インスタンス `try-int->char`: `(i8) -> std::Option<char>`
- インスタンス `try-int->i16`: `(i8) -> std::Option<i16>`
- インスタンス `try-int->i32`: `(i8) -> std::Option<i32>`
- インスタンス `try-int->i8`: `(i8) -> std::Option<i8>`
- インスタンス `try-int->u16`: `(i8) -> std::Option<u16>`
- インスタンス `try-int->u32`: `(i8) -> std::Option<u32>`
- インスタンス `try-int->u8`: `(i8) -> std::Option<u8>`

### `std::int` (sum, 型引数 [])

- インスタンス `*`: `(int, int) -> int`
- インスタンス `+`: `(int, int) -> int`
- インスタンス `-`: `(int, int) -> int`
- インスタンス `/`: `(int, int) -> rational`
- インスタンス `/=`: `(int, int) -> bool`
- インスタンス `<`: `(int, int) -> bool`
- インスタンス `<=`: `(int, int) -> bool`
- インスタンス `=`: `(int, int) -> bool`
- インスタンス `>`: `(int, int) -> bool`
- インスタンス `>=`: `(int, int) -> bool`
- インスタンス `ash`: `(int, int) -> int`
- インスタンス `eq`: `(int, int) -> bool`
- インスタンス `eql`: `(int, int) -> bool`
- インスタンス `equal`: `(int, int) -> bool`
- インスタンス `equalp`: `(int, int) -> bool`
- インスタンス `int->c-long`: `(int) -> c-long`
- インスタンス `int->c-ulong`: `(int) -> c-ulong`
- インスタンス `int->char`: `(int) -> char`
- インスタンス `int->float`: `(int) -> f64`
- インスタンス `int->i16`: `(int) -> i16`
- インスタンス `int->i32`: `(int) -> i32`
- インスタンス `int->i8`: `(int) -> i8`
- インスタンス `int->int`: `(int) -> int`
- インスタンス `int->ratio`: `(int) -> ratio`
- インスタンス `int->u16`: `(int) -> u16`
- インスタンス `int->u32`: `(int) -> u32`
- インスタンス `int->u8`: `(int) -> u8`
- インスタンス `integer-length`: `(int) -> int`
- インスタンス `logand`: `(int, int) -> int`
- インスタンス `logbitp`: `(int, int) -> bool`
- インスタンス `logcount`: `(int) -> int`
- インスタンス `logior`: `(int, int) -> int`
- インスタンス `lognot`: `(int) -> int`
- インスタンス `logtest`: `(int, int) -> bool`
- インスタンス `logxor`: `(int, int) -> int`
- インスタンス `max`: `(int, int) -> int`
- インスタンス `min`: `(int, int) -> int`
- インスタンス `mod`: `(int, int) -> int`
- インスタンス `truncate`: `(int, int) -> int`
- インスタンス `try-int->c-long`: `(int) -> std::Option<c-long>`
- インスタンス `try-int->c-ulong`: `(int) -> std::Option<c-ulong>`
- インスタンス `try-int->char`: `(int) -> std::Option<char>`
- インスタンス `try-int->i16`: `(int) -> std::Option<i16>`
- インスタンス `try-int->i32`: `(int) -> std::Option<i32>`
- インスタンス `try-int->i8`: `(int) -> std::Option<i8>`
- インスタンス `try-int->u16`: `(int) -> std::Option<u16>`
- インスタンス `try-int->u32`: `(int) -> std::Option<u32>`
- インスタンス `try-int->u8`: `(int) -> std::Option<u8>`

### `std::llvm-basic-block` (sum, 型引数 [])


### `std::llvm-builder` (sum, 型引数 [])

- インスタンス `alloca-args`: `(std::llvm-builder, int) -> std::llvm-value`
- インスタンス `block-terminated?`: `(std::llvm-builder) -> bool`
- インスタンス `build-add`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-and`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-ashr`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-br`: `(std::llvm-builder, std::llvm-basic-block) -> ()`
- インスタンス `build-call`: `(std::llvm-builder, std::llvm-function, std::llvm-value, int) -> std::llvm-value`
- インスタンス `build-call-with-env`: `(std::llvm-builder, std::llvm-function, std::llvm-value, int, std::llvm-value, int) -> std::llvm-value`
- インスタンス `build-cond-br`: `(std::llvm-builder, std::llvm-value, std::llvm-basic-block, std::llvm-basic-block) -> ()`
- インスタンス `build-fadd`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fceil`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fcmp-eq`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fcmp-ge`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fcmp-gt`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fcmp-le`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fcmp-lt`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fcmp-ne`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fcos`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fdiv`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fexp`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-ffloor`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-flog`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fmaxnum`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fminnum`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fmul`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fn-address`: `(std::llvm-builder, std::llvm-function) -> std::llvm-value`
- インスタンス `build-fpow`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fptosi`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-free`: `(std::llvm-builder, std::llvm-value) -> ()`
- インスタンス `build-frem`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fround`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fround32`: `(std::llvm-builder, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fsin`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fsqrt`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-fsub`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-ftrunc`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> std::llvm-value`
- インスタンス `build-icmp-eq`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-icmp-ge`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-icmp-gt`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-icmp-le`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-icmp-lt`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-icmp-ne`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-int-to-ptr`: `(std::llvm-builder, std::llvm-value) -> std::llvm-value`
- インスタンス `build-lshr`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-make-closure`: `(std::llvm-builder, std::llvm-module, std::llvm-function, std::llvm-value, int, int, int) -> std::llvm-value`
- インスタンス `build-malloc`: `(std::llvm-builder, int) -> std::llvm-value`
- インスタンス `build-mul`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-or`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-ptr-to-int`: `(std::llvm-builder, std::llvm-value) -> std::llvm-value`
- インスタンス `build-ret`: `(std::llvm-builder, std::llvm-value) -> ()`
- インスタンス `build-sadd-overflow`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-select`: `(std::llvm-builder, std::llvm-value, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-shl`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-sitofp`: `(std::llvm-builder, std::llvm-value) -> std::llvm-value`
- インスタンス `build-slot-ptr`: `(std::llvm-builder, std::llvm-value, int) -> std::llvm-value`
- インスタンス `build-smul-overflow`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-ssub-overflow`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-sub`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `build-xor`: `(std::llvm-builder, std::llvm-value, std::llvm-value) -> std::llvm-value`
- インスタンス `const-word`: `(std::llvm-builder, int) -> std::llvm-value`
- インスタンス `coroutine-apply`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value, int) -> std::llvm-value`
- インスタンス `coroutine-begin`: `(std::llvm-builder, std::llvm-module, std::llvm-function) -> ()`
- インスタンス `coroutine-call`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value, int) -> std::llvm-value`
- インスタンス `coroutine-call-env`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value, int, std::llvm-value, int) -> std::llvm-value`
- インスタンス `coroutine-dyn-call`: `(std::llvm-builder, std::llvm-module, std::llvm-value, std::llvm-value, std::llvm-value, int) -> std::llvm-value`
- インスタンス `coroutine-end`: `(std::llvm-builder, std::llvm-module, std::llvm-value) -> ()`
- インスタンス `coroutine-suspend`: `(std::llvm-builder, std::llvm-module) -> std::llvm-value`
- インスタンス `coroutine-unwind`: `(std::llvm-builder, std::llvm-module) -> ()`
- 関連関数 `create`: `() -> std::llvm-builder`
- インスタンス `frame-clear-handler`: `(std::llvm-builder, std::llvm-module) -> ()`
- インスタンス `frame-set-handler`: `(std::llvm-builder, std::llvm-module, std::llvm-basic-block) -> ()`
- インスタンス `frame-slot`: `(std::llvm-builder) -> std::llvm-value`
- インスタンス `frame-slot-rooted`: `(std::llvm-builder, std::llvm-module) -> std::llvm-value`
- インスタンス `load-arg`: `(std::llvm-builder, std::llvm-function, int) -> std::llvm-value`
- インスタンス `load-env`: `(std::llvm-builder, std::llvm-function, int) -> std::llvm-value`
- インスタンス `load-raw`: `(std::llvm-builder, std::llvm-value, int) -> std::llvm-value`
- インスタンス `load-type`: `(std::llvm-builder, std::llvm-module, int) -> std::llvm-value`
- インスタンス `position-at-end`: `(std::llvm-builder, std::llvm-basic-block) -> ()`
- インスタンス `scratch-slot`: `(std::llvm-builder, int) -> std::llvm-value`
- インスタンス `store-arg`: `(std::llvm-builder, std::llvm-value, int, std::llvm-value) -> ()`

### `std::llvm-function` (sum, 型引数 [])

- インスタンス `append-block`: `(std::llvm-function, string) -> std::llvm-basic-block`
- インスタンス `function-param`: `(std::llvm-function, int) -> std::llvm-value`

### `std::llvm-module` (sum, 型引数 [])

- インスタンス `add-coroutine-function`: `(std::llvm-module, string) -> std::llvm-function`
- インスタンス `add-function`: `(std::llvm-module, string) -> std::llvm-function`
- インスタンス `add-function-with-env`: `(std::llvm-module, string) -> std::llvm-function`
- 関連関数 `create`: `(string) -> std::llvm-module`
- インスタンス `get-function`: `(std::llvm-module, string) -> std::llvm-function`
- インスタンス `to-string`: `(std::llvm-module) -> string`
- インスタンス `verify`: `(std::llvm-module) -> bool`

### `std::llvm-value` (sum, 型引数 [])


### `std::random-state` (sum, 型引数 [])


### `std::ratio` (sum, 型引数 [])

- インスタンス `*`: `(ratio, ratio) -> ratio`
- インスタンス `+`: `(ratio, ratio) -> ratio`
- インスタンス `-`: `(ratio, ratio) -> ratio`
- インスタンス `/`: `(ratio, ratio) -> ratio`
- インスタンス `/=`: `(ratio, ratio) -> bool`
- インスタンス `<`: `(ratio, ratio) -> bool`
- インスタンス `<=`: `(ratio, ratio) -> bool`
- インスタンス `=`: `(ratio, ratio) -> bool`
- インスタンス `>`: `(ratio, ratio) -> bool`
- インスタンス `>=`: `(ratio, ratio) -> bool`
- インスタンス `denominator`: `(ratio) -> int`
- インスタンス `eq`: `(ratio, ratio) -> bool`
- インスタンス `eql`: `(ratio, ratio) -> bool`
- インスタンス `equal`: `(ratio, ratio) -> bool`
- インスタンス `equalp`: `(ratio, ratio) -> bool`
- インスタンス `max`: `(ratio, ratio) -> ratio`
- インスタンス `min`: `(ratio, ratio) -> ratio`
- インスタンス `numerator`: `(ratio) -> int`
- インスタンス `ratio->float`: `(ratio) -> f64`
- インスタンス `ratio->int`: `(ratio) -> int`
- インスタンス `ratio->rational`: `(ratio) -> rational`

### `std::rational` (sum, 型引数 [])

- インスタンス `*`: `(rational, rational) -> rational`
- インスタンス `+`: `(rational, rational) -> rational`
- インスタンス `-`: `(rational, rational) -> rational`
- インスタンス `/`: `(rational, rational) -> rational`
- インスタンス `/=`: `(rational, rational) -> bool`
- インスタンス `<`: `(rational, rational) -> bool`
- インスタンス `<=`: `(rational, rational) -> bool`
- インスタンス `=`: `(rational, rational) -> bool`
- インスタンス `>`: `(rational, rational) -> bool`
- インスタンス `>=`: `(rational, rational) -> bool`
- インスタンス `denominator`: `(rational) -> int`
- インスタンス `eq`: `(rational, rational) -> bool`
- インスタンス `eql`: `(rational, rational) -> bool`
- インスタンス `equal`: `(rational, rational) -> bool`
- インスタンス `equalp`: `(rational, rational) -> bool`
- インスタンス `max`: `(rational, rational) -> rational`
- インスタンス `min`: `(rational, rational) -> rational`
- インスタンス `numerator`: `(rational) -> int`
- インスタンス `rational->float`: `(rational) -> f64`
- インスタンス `rational->int`: `(rational) -> int`
- インスタンス `rational->ratio`: `(rational) -> ratio`
- インスタンス `try-rational->int`: `(rational) -> std::Option<int>`

### `std::string` (sum, 型引数 [])

- インスタンス `<`: `(string, string) -> bool`
- インスタンス `<=`: `(string, string) -> bool`
- インスタンス `>`: `(string, string) -> bool`
- インスタンス `>=`: `(string, string) -> bool`
- インスタンス `append`: `(string, string) -> string`
- インスタンス `downcase`: `(string) -> string`
- インスタンス `eq`: `(string, string) -> bool`
- インスタンス `eql`: `(string, string) -> bool`
- インスタンス `equal`: `(string, string) -> bool`
- インスタンス `equalp`: `(string, string) -> bool`
- インスタンス `length`: `(string) -> int`
- インスタンス `lt`: `(string, string) -> bool`
- インスタンス `ref`: `(string, int) -> char`
- インスタンス `substring`: `(string, int, int) -> string`
- インスタンス `upcase`: `(string) -> string`

### `std::symbol` (sum, 型引数 [])

- インスタンス `eq`: `(symbol, symbol) -> bool`
- インスタンス `eql`: `(symbol, symbol) -> bool`

### `std::sync::Chan` (struct, 型引数 [T])

- インスタンス `cap`: `(std::sync::Chan<T>) -> int`
- インスタンス `close`: `(std::sync::Chan<T>) -> ()`
- インスタンス `len`: `(std::sync::Chan<T>) -> int`
- 関連関数 `new`: `(int) -> std::sync::Chan<T>`
- インスタンス `recv`: `(std::sync::Chan<T>) -> std::Option<T>`
- インスタンス `send`: `(std::sync::Chan<T>, T) -> ()`

### `std::thread::Task` (struct, 型引数 [T])

- インスタンス `wait`: `(std::thread::Task<T>) -> T`

### `std::thread::Thread` (struct, 型引数 [T])

- 関連関数 `available-parallelism`: `() -> int`
- 関連関数 `current-id`: `() -> int`
- インスタンス `join`: `(std::thread::Thread<T>) -> T`

### `std::u16` (sum, 型引数 [])

- インスタンス `*`: `(u16, u16) -> u16`
- インスタンス `+`: `(u16, u16) -> u16`
- インスタンス `-`: `(u16, u16) -> u16`
- インスタンス `/`: `(u16, u16) -> u16`
- インスタンス `/=`: `(u16, u16) -> bool`
- インスタンス `<`: `(u16, u16) -> bool`
- インスタンス `<=`: `(u16, u16) -> bool`
- インスタンス `=`: `(u16, u16) -> bool`
- インスタンス `>`: `(u16, u16) -> bool`
- インスタンス `>=`: `(u16, u16) -> bool`
- インスタンス `ash`: `(u16, int) -> u16`
- インスタンス `eq`: `(u16, u16) -> bool`
- インスタンス `eql`: `(u16, u16) -> bool`
- インスタンス `equal`: `(u16, u16) -> bool`
- インスタンス `equalp`: `(u16, u16) -> bool`
- インスタンス `int->c-long`: `(u16) -> c-long`
- インスタンス `int->c-ulong`: `(u16) -> c-ulong`
- インスタンス `int->char`: `(u16) -> char`
- インスタンス `int->float`: `(u16) -> f64`
- インスタンス `int->i16`: `(u16) -> i16`
- インスタンス `int->i32`: `(u16) -> i32`
- インスタンス `int->i8`: `(u16) -> i8`
- インスタンス `int->int`: `(u16) -> int`
- インスタンス `int->ratio`: `(u16) -> ratio`
- インスタンス `int->u16`: `(u16) -> u16`
- インスタンス `int->u32`: `(u16) -> u32`
- インスタンス `int->u8`: `(u16) -> u8`
- インスタンス `integer-length`: `(u16) -> u16`
- インスタンス `logand`: `(u16, u16) -> u16`
- インスタンス `logbitp`: `(u16, int) -> bool`
- インスタンス `logcount`: `(u16) -> u16`
- インスタンス `logior`: `(u16, u16) -> u16`
- インスタンス `lognot`: `(u16) -> u16`
- インスタンス `logtest`: `(u16, u16) -> bool`
- インスタンス `logxor`: `(u16, u16) -> u16`
- インスタンス `max`: `(u16, u16) -> u16`
- インスタンス `min`: `(u16, u16) -> u16`
- インスタンス `mod`: `(u16, u16) -> u16`
- インスタンス `try-int->c-long`: `(u16) -> std::Option<c-long>`
- インスタンス `try-int->c-ulong`: `(u16) -> std::Option<c-ulong>`
- インスタンス `try-int->char`: `(u16) -> std::Option<char>`
- インスタンス `try-int->i16`: `(u16) -> std::Option<i16>`
- インスタンス `try-int->i32`: `(u16) -> std::Option<i32>`
- インスタンス `try-int->i8`: `(u16) -> std::Option<i8>`
- インスタンス `try-int->u16`: `(u16) -> std::Option<u16>`
- インスタンス `try-int->u32`: `(u16) -> std::Option<u32>`
- インスタンス `try-int->u8`: `(u16) -> std::Option<u8>`

### `std::u32` (sum, 型引数 [])

- インスタンス `*`: `(u32, u32) -> u32`
- インスタンス `+`: `(u32, u32) -> u32`
- インスタンス `-`: `(u32, u32) -> u32`
- インスタンス `/`: `(u32, u32) -> u32`
- インスタンス `/=`: `(u32, u32) -> bool`
- インスタンス `<`: `(u32, u32) -> bool`
- インスタンス `<=`: `(u32, u32) -> bool`
- インスタンス `=`: `(u32, u32) -> bool`
- インスタンス `>`: `(u32, u32) -> bool`
- インスタンス `>=`: `(u32, u32) -> bool`
- インスタンス `ash`: `(u32, int) -> u32`
- インスタンス `eq`: `(u32, u32) -> bool`
- インスタンス `eql`: `(u32, u32) -> bool`
- インスタンス `equal`: `(u32, u32) -> bool`
- インスタンス `equalp`: `(u32, u32) -> bool`
- インスタンス `int->c-long`: `(u32) -> c-long`
- インスタンス `int->c-ulong`: `(u32) -> c-ulong`
- インスタンス `int->char`: `(u32) -> char`
- インスタンス `int->float`: `(u32) -> f64`
- インスタンス `int->i16`: `(u32) -> i16`
- インスタンス `int->i32`: `(u32) -> i32`
- インスタンス `int->i8`: `(u32) -> i8`
- インスタンス `int->int`: `(u32) -> int`
- インスタンス `int->ratio`: `(u32) -> ratio`
- インスタンス `int->u16`: `(u32) -> u16`
- インスタンス `int->u32`: `(u32) -> u32`
- インスタンス `int->u8`: `(u32) -> u8`
- インスタンス `integer-length`: `(u32) -> u32`
- インスタンス `logand`: `(u32, u32) -> u32`
- インスタンス `logbitp`: `(u32, int) -> bool`
- インスタンス `logcount`: `(u32) -> u32`
- インスタンス `logior`: `(u32, u32) -> u32`
- インスタンス `lognot`: `(u32) -> u32`
- インスタンス `logtest`: `(u32, u32) -> bool`
- インスタンス `logxor`: `(u32, u32) -> u32`
- インスタンス `max`: `(u32, u32) -> u32`
- インスタンス `min`: `(u32, u32) -> u32`
- インスタンス `mod`: `(u32, u32) -> u32`
- インスタンス `try-int->c-long`: `(u32) -> std::Option<c-long>`
- インスタンス `try-int->c-ulong`: `(u32) -> std::Option<c-ulong>`
- インスタンス `try-int->char`: `(u32) -> std::Option<char>`
- インスタンス `try-int->i16`: `(u32) -> std::Option<i16>`
- インスタンス `try-int->i32`: `(u32) -> std::Option<i32>`
- インスタンス `try-int->i8`: `(u32) -> std::Option<i8>`
- インスタンス `try-int->u16`: `(u32) -> std::Option<u16>`
- インスタンス `try-int->u32`: `(u32) -> std::Option<u32>`
- インスタンス `try-int->u8`: `(u32) -> std::Option<u8>`

### `std::u8` (sum, 型引数 [])

- インスタンス `*`: `(u8, u8) -> u8`
- インスタンス `+`: `(u8, u8) -> u8`
- インスタンス `-`: `(u8, u8) -> u8`
- インスタンス `/`: `(u8, u8) -> u8`
- インスタンス `/=`: `(u8, u8) -> bool`
- インスタンス `<`: `(u8, u8) -> bool`
- インスタンス `<=`: `(u8, u8) -> bool`
- インスタンス `=`: `(u8, u8) -> bool`
- インスタンス `>`: `(u8, u8) -> bool`
- インスタンス `>=`: `(u8, u8) -> bool`
- インスタンス `ash`: `(u8, int) -> u8`
- インスタンス `eq`: `(u8, u8) -> bool`
- インスタンス `eql`: `(u8, u8) -> bool`
- インスタンス `equal`: `(u8, u8) -> bool`
- インスタンス `equalp`: `(u8, u8) -> bool`
- インスタンス `int->c-long`: `(u8) -> c-long`
- インスタンス `int->c-ulong`: `(u8) -> c-ulong`
- インスタンス `int->char`: `(u8) -> char`
- インスタンス `int->float`: `(u8) -> f64`
- インスタンス `int->i16`: `(u8) -> i16`
- インスタンス `int->i32`: `(u8) -> i32`
- インスタンス `int->i8`: `(u8) -> i8`
- インスタンス `int->int`: `(u8) -> int`
- インスタンス `int->ratio`: `(u8) -> ratio`
- インスタンス `int->u16`: `(u8) -> u16`
- インスタンス `int->u32`: `(u8) -> u32`
- インスタンス `int->u8`: `(u8) -> u8`
- インスタンス `integer-length`: `(u8) -> u8`
- インスタンス `logand`: `(u8, u8) -> u8`
- インスタンス `logbitp`: `(u8, int) -> bool`
- インスタンス `logcount`: `(u8) -> u8`
- インスタンス `logior`: `(u8, u8) -> u8`
- インスタンス `lognot`: `(u8) -> u8`
- インスタンス `logtest`: `(u8, u8) -> bool`
- インスタンス `logxor`: `(u8, u8) -> u8`
- インスタンス `max`: `(u8, u8) -> u8`
- インスタンス `min`: `(u8, u8) -> u8`
- インスタンス `mod`: `(u8, u8) -> u8`
- インスタンス `try-int->c-long`: `(u8) -> std::Option<c-long>`
- インスタンス `try-int->c-ulong`: `(u8) -> std::Option<c-ulong>`
- インスタンス `try-int->char`: `(u8) -> std::Option<char>`
- インスタンス `try-int->i16`: `(u8) -> std::Option<i16>`
- インスタンス `try-int->i32`: `(u8) -> std::Option<i32>`
- インスタンス `try-int->i8`: `(u8) -> std::Option<i8>`
- インスタンス `try-int->u16`: `(u8) -> std::Option<u16>`
- インスタンス `try-int->u32`: `(u8) -> std::Option<u32>`
- インスタンス `try-int->u8`: `(u8) -> std::Option<u8>`

## 組み込み関数

| 関数 | シム | シグネチャ |
|---|---|---|
| `std::%internal::dribble-start` | `rt_dribble_start` | `(string) -> std::Result<(),std::FileError>` |
| `std::%internal::dribble-stop` | `rt_dribble_stop` | `() -> std::Result<(),std::FileError>` |
| `std::%internal::ed-open` | `rt_ed_open` | `(string, int) -> std::Result<(),std::FileError>` |
| `std::%internal::file-create-directories` | `rt_file_create_directories` | `(string) -> std::Result<(),std::FileError>` |
| `std::%internal::file-delete` | `rt_file_delete` | `(string) -> std::Result<(),std::FileError>` |
| `std::%internal::file-directory-p` | `rt_file_directory_p` | `(string) -> bool` |
| `std::%internal::file-exists-p` | `rt_file_exists_p` | `(string) -> bool` |
| `std::%internal::file-list-directory` | `rt_file_list_directory` | `(string) -> std::Result<std::Vector<string>,std::FileError>` |
| `std::%internal::file-modified-date` | `rt_file_modified_date` | `(string) -> std::Result<std::time::universal-time,std::FileError>` |
| `std::%internal::file-owner-name` | `rt_file_owner_name` | `(string) -> std::Result<std::Option<string>,std::FileError>` |
| `std::%internal::file-rename` | `rt_file_rename` | `(string, string) -> std::Result<(),std::FileError>` |
| `std::%internal::file-truename` | `rt_file_truename` | `(string) -> std::Result<string,std::FileError>` |
| `std::%internal::make-random-state-fresh` | `rt_make_random_state_fresh` | `() -> random-state` |
| `std::%internal::net-accept` | `rt_net_accept` | `(i32) -> std::Result<std::Option<i32>,std::NetError>` |
| `std::%internal::net-buffered-p` | `rt_net_buffered_p` | `(i32) -> std::Result<bool,std::NetError>` |
| `std::%internal::net-connect-begin` | `rt_net_connect_begin` | `(string) -> std::Result<i32,std::NetError>` |
| `std::%internal::net-connect-finish` | `rt_net_connect_finish` | `(i32) -> std::Result<(),std::NetError>` |
| `std::%internal::net-fill` | `rt_net_fill` | `(i32) -> std::Result<std::Option<int>,std::NetError>` |
| `std::%internal::net-flush` | `rt_net_flush` | `(i32) -> std::Result<std::Option<int>,std::NetError>` |
| `std::%internal::net-listen` | `rt_net_listen` | `(string, int) -> std::Result<i32,std::NetError>` |
| `std::%internal::net-local-address` | `rt_net_local_address` | `(i32) -> std::Result<string,std::NetError>` |
| `std::%internal::net-peer-address` | `rt_net_peer_address` | `(i32) -> std::Result<string,std::NetError>` |
| `std::%internal::net-peer-subject` | `rt_net_peer_subject` | `(i32) -> std::Result<std::Option<string>,std::NetError>` |
| `std::%internal::net-pop-byte` | `rt_net_pop_byte` | `(i32) -> std::Result<std::Option<int>,std::NetError>` |
| `std::%internal::net-pop-char` | `rt_net_pop_char` | `(i32) -> std::Result<std::Option<char>,std::NetError>` |
| `std::%internal::net-push-byte` | `rt_net_push_byte` | `(i32, int) -> std::Result<(),std::NetError>` |
| `std::%internal::net-push-string` | `rt_net_push_string` | `(i32, string) -> std::Result<(),std::NetError>` |
| `std::%internal::net-resolve-begin` | `rt_net_resolve_begin` | `(string, int) -> std::Result<i32,std::NetError>` |
| `std::%internal::net-resolve-finish` | `rt_net_resolve_finish` | `(i32) -> std::Result<std::Option<std::Vector<string>>,std::NetError>` |
| `std::%internal::net-server-name` | `rt_net_server_name` | `(i32) -> std::Result<std::Option<string>,std::NetError>` |
| `std::%internal::net-set-keepalive` | `rt_net_set_keepalive` | `(i32, bool) -> std::Result<(),std::NetError>` |
| `std::%internal::net-set-keepalive-period` | `rt_net_set_keepalive_period` | `(i32, int) -> std::Result<(),std::NetError>` |
| `std::%internal::net-set-nodelay` | `rt_net_set_nodelay` | `(i32, bool) -> std::Result<(),std::NetError>` |
| `std::%internal::net-shutdown-write` | `rt_net_shutdown_write` | `(i32) -> std::Result<(),std::NetError>` |
| `std::%internal::net-socket-error` | `rt_net_socket_error` | `(i32) -> std::Result<std::Option<string>,std::NetError>` |
| `std::%internal::net-tls-add-certificate` | `rt_net_tls_add_certificate` | `(i32, string, string, string) -> std::Result<(),std::NetError>` |
| `std::%internal::net-tls-handshake` | `rt_net_tls_handshake` | `(i32) -> std::Result<std::Option<int>,std::NetError>` |
| `std::%internal::net-tls-listen` | `rt_net_tls_listen` | `(string, int, string, string, std::Option<string>) -> std::Result<i32,std::NetError>` |
| `std::%internal::net-tls-start` | `rt_net_tls_start` | `(i32, string, std::Option<string>, std::Option<string>, std::Option<string>) -> std::Result<(),std::NetError>` |
| `std::%internal::net-udp-bind` | `rt_net_udp_bind` | `(string, int) -> std::Result<i32,std::NetError>` |
| `std::%internal::net-udp-last-sender` | `rt_net_udp_last_sender` | `(i32) -> std::Result<string,std::NetError>` |
| `std::%internal::net-udp-recv` | `rt_net_udp_recv` | `(i32) -> std::Result<std::Option<std::Vector<int>>,std::NetError>` |
| `std::%internal::net-udp-send-to` | `rt_net_udp_send_to` | `(i32, string, std::Vector<int>) -> std::Result<bool,std::NetError>` |
| `std::%internal::net-unix-connect` | `rt_net_unix_connect` | `(string) -> std::Result<i32,std::NetError>` |
| `std::%internal::net-unix-listen` | `rt_net_unix_listen` | `(string) -> std::Result<i32,std::NetError>` |
| `std::%internal::net-wait` | `rt_suspend_io` | `(i32, int) -> ()` |
| `std::%internal::net-wait-for` | `rt_suspend_io_for` | `(i32, int, f64) -> bool` |
| `std::%internal::random-state-copy` | `rt_random_state_copy` | `(random-state) -> random-state` |
| `std::%internal::random-state-next` | `rt_random_state_next` | `(random-state, int) -> int` |
| `std::%internal::read-datum-at` | `rt_read_datum_at` | `(string, int, bool) -> std::Result<std::cons-cell<std::Option<std::Sexpr>,int>,std::ReadError>` |
| `std::%internal::stream-at-line-start` | `rt_stream_at_line_start` | `(i32) -> std::Result<bool,std::FileError>` |
| `std::%internal::stream-close` | `rt_stream_close` | `(i32) -> std::Result<(),std::FileError>` |
| `std::%internal::stream-describe` | `rt_stream_describe` | `(i32) -> string` |
| `std::%internal::stream-finish-output` | `rt_stream_finish_output` | `(i32) -> std::Result<(),std::FileError>` |
| `std::%internal::stream-input-p` | `rt_stream_input_p` | `(i32) -> std::Result<bool,std::FileError>` |
| `std::%internal::stream-listen` | `rt_stream_listen` | `(i32) -> std::Result<bool,std::FileError>` |
| `std::%internal::stream-open-file` | `rt_stream_open_file` | `(string, int) -> std::Result<i32,std::FileError>` |
| `std::%internal::stream-open-p` | `rt_stream_open_p` | `(i32) -> bool` |
| `std::%internal::stream-output-p` | `rt_stream_output_p` | `(i32) -> std::Result<bool,std::FileError>` |
| `std::%internal::stream-position` | `rt_stream_position` | `(i32) -> std::Result<int,std::FileError>` |
| `std::%internal::stream-read-byte` | `rt_stream_read_byte` | `(i32) -> std::Result<std::Option<int>,std::FileError>` |
| `std::%internal::stream-read-char` | `rt_stream_read_char` | `(i32) -> std::Result<std::Option<char>,std::FileError>` |
| `std::%internal::stream-stderr` | `rt_stream_stderr` | `() -> i32` |
| `std::%internal::stream-stdin` | `rt_stream_stdin` | `() -> i32` |
| `std::%internal::stream-stdout` | `rt_stream_stdout` | `() -> i32` |
| `std::%internal::stream-string-input` | `rt_stream_string_input` | `(string) -> i32` |
| `std::%internal::stream-string-output` | `rt_stream_string_output` | `() -> i32` |
| `std::%internal::stream-take-output-string` | `rt_stream_take_output_string` | `(i32) -> std::Result<string,std::FileError>` |
| `std::%internal::stream-unread-char` | `rt_stream_unread_char` | `(i32, char) -> std::Result<(),std::FileError>` |
| `std::%internal::stream-write-byte` | `rt_stream_write_byte` | `(i32, int) -> std::Result<(),std::FileError>` |
| `std::%internal::stream-write-string` | `rt_stream_write_string` | `(i32, string) -> std::Result<(),std::FileError>` |
| `std::cmp::equal` | `rt_sexpr_equal` | `<T> (T, T) -> bool` |
| `std::cmp::equalp` | `rt_sexpr_equalp` | `<T> (T, T) -> bool` |
| `std::compile-file` |  | `(string, string) -> bool` |
| `std::dump` |  | `(string) -> bool` |
| `std::env::command-line-args` | `rt_command_line_args` | `() -> std::Vector<string>` |
| `std::env::getenv` | `rt_getenv` | `(string) -> std::Option<string>` |
| `std::env::home-directory` | `rt_home_directory` | `() -> std::Option<string>` |
| `std::env::lisp-implementation-version` | `rt_lisp_implementation_version` | `() -> string` |
| `std::env::machine-instance` | `rt_machine_instance` | `() -> std::Option<string>` |
| `std::env::machine-type` | `rt_machine_type` | `() -> string` |
| `std::env::machine-version` | `rt_machine_version` | `() -> std::Option<string>` |
| `std::env::software-type` | `rt_software_type` | `() -> string` |
| `std::env::software-version` | `rt_software_version` | `() -> std::Option<string>` |
| `std::eval::eval` | `rt_eval` | `(std::Option<std::Sexpr>) -> std::Result<std::Option<std::Sexpr>,std::EvalError>` |
| `std::eval::macroexpand` | `rt_macroexpand` | `(std::Option<std::Sexpr>) -> std::Result<std::Option<std::Sexpr>,std::EvalError>` |
| `std::eval::macroexpand-1` | `rt_macroexpand_1` | `(std::Option<std::Sexpr>) -> std::Result<std::Option<std::Sexpr>,std::EvalError>` |
| `std::fmt::pprint-indent` | `rt_pprint_indent` | `(symbol, int) -> ()` |
| `std::fmt::pprint-list-exhausted` | `rt_pprint_list_exhausted` | `() -> bool` |
| `std::fmt::pprint-newline` | `rt_pprint_newline` | `(symbol) -> ()` |
| `std::fmt::pprint-pop` | `rt_pprint_pop` | `() -> std::Option<std::Sexpr>` |
| `std::fmt::pprint-tab` | `rt_pprint_tab` | `(symbol, int, int) -> ()` |
| `std::num::parse-float` | `rt_parse_float` | `(string) -> std::Result<f64,std::ParseFloatError>` |
| `std::process::exit` | `rt_exit` | `(int) -> !` |
| `std::random::seed-random-state` | `rt_seed_random_state` | `(int) -> random-state` |
| `std::reader::get-dispatch-macro-character` | `rt_get_dispatch_macro_character` | `(char, char) -> std::Option<(fn (std::io::string-input-stream char) std::Option<std::Sexpr>)>` |
| `std::reader::get-macro-character` | `rt_get_macro_character` | `(char) -> std::Option<(fn (std::io::string-input-stream char) std::Option<std::Sexpr>)>` |
| `std::reader::read` | `rt_read` | `(string) -> std::Result<std::Option<std::Sexpr>,std::ReadError>` |
| `std::reader::set-dispatch-macro-character` | `rt_set_dispatch_macro_character` | `(char, char, (fn (std::io::string-input-stream char) std::Option<std::Sexpr>)) -> ()` |
| `std::reader::set-macro-character` | `rt_set_macro_character` | `(char, (fn (std::io::string-input-stream char) std::Option<std::Sexpr>)) -> ()` |
| `std::repl::heap-info` | `rt_heap_info` | `() -> std::repl::heap-info` |
| `std::sexpr-atom` | `rt_atom` | `(std::Option<std::Sexpr>) -> bool` |
| `std::sexpr-bool` | `rt_sexpr_bool` | `(std::Option<std::Sexpr>) -> bool` |
| `std::sexpr-car` | `rt_car` | `(std::Option<std::Sexpr>) -> std::Option<std::Sexpr>` |
| `std::sexpr-cdr` | `rt_cdr` | `(std::Option<std::Sexpr>) -> std::Option<std::Sexpr>` |
| `std::sexpr-char` | `rt_sexpr_char` | `(std::Option<std::Sexpr>) -> char` |
| `std::sexpr-cons` | `rt_cons` | `(std::Option<std::Sexpr>, std::Option<std::Sexpr>) -> std::Option<std::Sexpr>` |
| `std::sexpr-consp` | `rt_consp` | `(std::Option<std::Sexpr>) -> bool` |
| `std::sexpr-f32` | `rt_sexpr_f32` | `(std::Option<std::Sexpr>) -> f32` |
| `std::sexpr-f64` | `rt_sexpr_f64` | `(std::Option<std::Sexpr>) -> f64` |
| `std::sexpr-i16` | `rt_sexpr_i16` | `(std::Option<std::Sexpr>) -> i16` |
| `std::sexpr-i32` | `rt_sexpr_i32` | `(std::Option<std::Sexpr>) -> i32` |
| `std::sexpr-i8` | `rt_sexpr_i8` | `(std::Option<std::Sexpr>) -> i8` |
| `std::sexpr-int` | `rt_sexpr_int` | `(std::Option<std::Sexpr>) -> int` |
| `std::sexpr-null` | `rt_null` | `(std::Option<std::Sexpr>) -> bool` |
| `std::sexpr-str` | `rt_sexpr_str` | `(std::Option<std::Sexpr>) -> string` |
| `std::sexpr-sym-name` | `rt_sym_name` | `(std::Option<std::Sexpr>) -> string` |
| `std::sexpr-symp` | `rt_symp` | `(std::Option<std::Sexpr>) -> bool` |
| `std::sexpr-u16` | `rt_sexpr_u16` | `(std::Option<std::Sexpr>) -> u16` |
| `std::sexpr-u32` | `rt_sexpr_u32` | `(std::Option<std::Sexpr>) -> u32` |
| `std::sexpr-u8` | `rt_sexpr_u8` | `(std::Option<std::Sexpr>) -> u8` |
| `std::symbol::intern` | `rt_intern` | `(string, string) -> symbol` |
| `std::symbol::keywordp` | `rt_keywordp` | `(symbol) -> bool` |
| `std::symbol::string->symbol` | `rt_intern_symbol` | `(string) -> symbol` |
| `std::symbol::symbol->string` | `rt_sym_name` | `(symbol) -> string` |
| `std::thread::sleep` | `rt_suspend_sleep` | `(f64) -> ()` |
| `std::thread::yield` | `rt_suspend_yield` | `() -> ()` |
| `std::time::get-internal-real-time` | `rt_get_internal_real_time` | `() -> std::time::internal-time` |
| `std::time::get-internal-run-time` | `rt_get_internal_run_time` | `() -> std::time::internal-time` |
| `std::time::get-universal-time` | `rt_get_universal_time` | `() -> std::time::universal-time` |
| `std::time::timezone-daylight-p` | `rt_timezone_daylight_p` | `(int, int) -> std::Option<bool>` |
| `std::time::timezone-offset-seconds` | `rt_timezone_offset_seconds` | `(int, int) -> std::Option<int>` |

