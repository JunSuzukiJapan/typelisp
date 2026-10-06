# ABI バージョン 2

`scripts/regen-abi-version.sh` が生成した文書。手で編集しない。

コンパイラが生成するコードが、ABI バージョン 2 のランタイムライブラリについて仮定していることの一覧。
ここに書かれていない前提(シムが引数をどう扱うか、生成コードがヒープを直接読む箇所など)は含まない。

内容ハッシュ (FNV-1a 64): `312e2b147e06402d`

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
| FRAME_RESERVED_SLOTS | 2 |

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
| DESC_FIELD | '\u{1f}' |
| DESC_PAIR | '\u{1e}' |

## 終了コード (typelisp_rt)

| 名前 | 値 |
|---|---|
| EXIT_CODE_PANIC | 1 |
| EXIT_CODE_SUCCESS | 0 |

## ダンプ形式 (typelisp_front::dump)

| 名前 | 値 |
|---|---|
| FORMAT_VERSION | 15 |
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

## 組み込み型キー (typelisp_mem::BUILTIN_TYPE_KEYS)

| 番号 | 型キー |
|---|---|
| 0 | `option` |
| 1 | `result` |
| 2 | `vector` |
| 3 | `cons-cell` |
| 4 | `hashtable` |
| 5 | `scope` |
| 6 | `scope-frame` |
| 7 | `readerror` |
| 8 | `fileerror` |
| 9 | `parseinterror` |
| 10 | `parsefloaterror` |
| 11 | `universal-time` |
| 12 | `internal-time` |
| 13 | `heap-info` |
| 14 | `neterror` |

## 実行時が組み立てる値の型キー (typelisp_rt)

| 表 | 関数 | 型キー |
|---|---|---|
| sys_builtin::RESULT_KEYS | `parse-float` | `result<f64,parsefloaterror>` |
| sys_builtin::RESULT_KEYS | `command-line-args` | `vector<string>` |
| sys_builtin::RESULT_KEYS | `getenv` | `option<string>` |
| sys_builtin::RESULT_KEYS | `home-directory` | `option<string>` |
| sys_builtin::RESULT_KEYS | `machine-instance` | `option<string>` |
| sys_builtin::RESULT_KEYS | `machine-version` | `option<string>` |
| sys_builtin::RESULT_KEYS | `software-version` | `option<string>` |
| sys_builtin::RESULT_KEYS | `timezone-offset-seconds` | `option<int>` |
| sys_builtin::RESULT_KEYS | `timezone-daylight-p` | `option<bool>` |
| sys_builtin::RESULT_KEYS | `dribble-start` | `result<(),fileerror>` |
| sys_builtin::RESULT_KEYS | `dribble-stop` | `result<(),fileerror>` |
| sys_builtin::RESULT_KEYS | `ed-open` | `result<(),fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-open-file` | `result<i32,fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-close` | `result<(),fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-input-p` | `result<bool,fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-output-p` | `result<bool,fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-listen` | `result<bool,fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-position` | `result<int,fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-at-line-start` | `result<bool,fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-read-char` | `result<option<char>,fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-unread-char` | `result<(),fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-read-byte` | `result<option<int>,fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-write-byte` | `result<(),fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-write-string` | `result<(),fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-finish-output` | `result<(),fileerror>` |
| stream_builtin::RESULT_KEYS | `stream-take-output-string` | `result<string,fileerror>` |
| stream_builtin::RESULT_KEYS | `file-delete` | `result<(),fileerror>` |
| stream_builtin::RESULT_KEYS | `file-truename` | `result<string,fileerror>` |
| stream_builtin::RESULT_KEYS | `file-modified-date` | `result<universal-time,fileerror>` |
| stream_builtin::RESULT_KEYS | `file-owner-name` | `result<option<string>,fileerror>` |
| stream_builtin::RESULT_KEYS | `file-list-directory` | `result<vector<string>,fileerror>` |
| stream_builtin::RESULT_KEYS | `file-create-directories` | `result<(),fileerror>` |
| stream_builtin::RESULT_KEYS | `file-rename` | `result<(),fileerror>` |
| stream_builtin::INNER_KEYS | `stream-read-char` | `option<char>` |
| stream_builtin::INNER_KEYS | `stream-read-byte` | `option<int>` |
| stream_builtin::INNER_KEYS | `file-list-directory` | `vector<string>` |
| stream_builtin::INNER_KEYS | `file-owner-name` | `option<string>` |
| net_builtin::RESULT_KEYS | `net-resolve-begin` | `result<i32,neterror>` |
| net_builtin::RESULT_KEYS | `net-resolve-finish` | `result<option<vector<string>>,neterror>` |
| net_builtin::RESULT_KEYS | `net-connect-begin` | `result<i32,neterror>` |
| net_builtin::RESULT_KEYS | `net-connect-finish` | `result<(),neterror>` |
| net_builtin::RESULT_KEYS | `net-listen` | `result<i32,neterror>` |
| net_builtin::RESULT_KEYS | `net-tls-listen` | `result<i32,neterror>` |
| net_builtin::RESULT_KEYS | `net-unix-connect` | `result<i32,neterror>` |
| net_builtin::RESULT_KEYS | `net-unix-listen` | `result<i32,neterror>` |
| net_builtin::RESULT_KEYS | `net-accept` | `result<option<i32>,neterror>` |
| net_builtin::RESULT_KEYS | `net-fill` | `result<option<int>,neterror>` |
| net_builtin::RESULT_KEYS | `net-pop-byte` | `result<option<int>,neterror>` |
| net_builtin::RESULT_KEYS | `net-pop-char` | `result<option<char>,neterror>` |
| net_builtin::RESULT_KEYS | `net-buffered-p` | `result<bool,neterror>` |
| net_builtin::RESULT_KEYS | `net-push-string` | `result<(),neterror>` |
| net_builtin::RESULT_KEYS | `net-push-byte` | `result<(),neterror>` |
| net_builtin::RESULT_KEYS | `net-flush` | `result<option<int>,neterror>` |
| net_builtin::RESULT_KEYS | `net-socket-error` | `result<option<string>,neterror>` |
| net_builtin::RESULT_KEYS | `net-peer-subject` | `result<option<string>,neterror>` |
| net_builtin::RESULT_KEYS | `net-server-name` | `result<option<string>,neterror>` |
| net_builtin::RESULT_KEYS | `net-set-nodelay` | `result<(),neterror>` |
| net_builtin::RESULT_KEYS | `net-set-keepalive` | `result<(),neterror>` |
| net_builtin::RESULT_KEYS | `net-set-keepalive-period` | `result<(),neterror>` |
| net_builtin::RESULT_KEYS | `net-tls-add-certificate` | `result<(),neterror>` |
| net_builtin::RESULT_KEYS | `net-shutdown-write` | `result<(),neterror>` |
| net_builtin::RESULT_KEYS | `net-local-address` | `result<string,neterror>` |
| net_builtin::RESULT_KEYS | `net-peer-address` | `result<string,neterror>` |
| net_builtin::RESULT_KEYS | `net-tls-start` | `result<(),neterror>` |
| net_builtin::RESULT_KEYS | `net-tls-handshake` | `result<option<int>,neterror>` |
| net_builtin::RESULT_KEYS | `net-udp-bind` | `result<i32,neterror>` |
| net_builtin::RESULT_KEYS | `net-udp-send-to` | `result<bool,neterror>` |
| net_builtin::RESULT_KEYS | `net-udp-recv` | `result<option<vector<int>>,neterror>` |
| net_builtin::RESULT_KEYS | `net-udp-last-sender` | `result<string,neterror>` |
| net_builtin::INNER_KEYS | `net-resolve-finish` | `option<vector<string>>` |
| net_builtin::INNER_KEYS | `net-tls-handshake` | `option<int>` |
| net_builtin::INNER_KEYS | `net-udp-recv` | `option<vector<int>>` |
| net_builtin::INNER_KEYS | `net-accept` | `option<i32>` |
| net_builtin::INNER_KEYS | `net-fill` | `option<int>` |
| net_builtin::INNER_KEYS | `net-flush` | `option<int>` |
| net_builtin::INNER_KEYS | `net-socket-error` | `option<string>` |
| net_builtin::INNER_KEYS | `net-peer-subject` | `option<string>` |
| net_builtin::INNER_KEYS | `net-server-name` | `option<string>` |
| net_builtin::INNER_KEYS | `net-pop-byte` | `option<int>` |
| net_builtin::INNER_KEYS | `net-pop-char` | `option<char>` |
| readtable::READER_MACRO_OPTION_KEY | | `option<(fn (string-input-stream,char) option<sexpr>)>` |

## 型ごとの表現 (Repr)

| 型 | 型キー | Repr | field_kind | binding_kind |
|---|---|---|---|---|
| `!` | `!` | none | 0 | 0 |
| `()` | `()` | unit | 100 | 0 |
| `(fn (string-input-stream char) Option<Sexpr>)` | `(fn (string-input-stream,char) option<sexpr>)` | fn | 6 | 2 |
| `Chan<t>` | `chan<t>` | struct | 6 | 2 |
| `HashTable<k,v>` | `hashtable<k,v>` | hashtable | 6 | 2 |
| `Option<(fn (string-input-stream char) Option<Sexpr>)>` | `option<(fn (string-input-stream,char) option<sexpr>)>` | niche | 101 | 2 |
| `Option<Sexpr>` | `option<sexpr>` | niche | 101 | 2 |
| `Option<bool>` | `option<bool>` | niche | 101 | 0 |
| `Option<c-long>` | `option<c-long>` | enum | 6 | 2 |
| `Option<c-ulong>` | `option<c-ulong>` | enum | 6 | 2 |
| `Option<char>` | `option<char>` | niche | 101 | 0 |
| `Option<f32>` | `option<f32>` | niche | 101 | 2 |
| `Option<f64>` | `option<f64>` | niche | 101 | 2 |
| `Option<i16>` | `option<i16>` | niche | 101 | 0 |
| `Option<i32>` | `option<i32>` | niche | 101 | 0 |
| `Option<i8>` | `option<i8>` | niche | 101 | 0 |
| `Option<int>` | `option<int>` | niche | 101 | 2 |
| `Option<string>` | `option<string>` | niche | 101 | 2 |
| `Option<t>` | `option<t>` | enum | 6 | 2 |
| `Option<u16>` | `option<u16>` | niche | 101 | 0 |
| `Option<u32>` | `option<u32>` | niche | 101 | 0 |
| `Option<u8>` | `option<u8>` | niche | 101 | 0 |
| `Option<v>` | `option<v>` | enum | 6 | 2 |
| `Result<(),fileerror>` | `result<(),fileerror>` | enum | 6 | 2 |
| `Result<(),neterror>` | `result<(),neterror>` | enum | 6 | 2 |
| `Result<Option<Sexpr>,evalerror>` | `result<option<sexpr>,evalerror>` | enum | 6 | 2 |
| `Result<Option<Sexpr>,readerror>` | `result<option<sexpr>,readerror>` | enum | 6 | 2 |
| `Result<Option<Vector<int>>,neterror>` | `result<option<vector<int>>,neterror>` | enum | 6 | 2 |
| `Result<Option<Vector<string>>,neterror>` | `result<option<vector<string>>,neterror>` | enum | 6 | 2 |
| `Result<Option<char>,fileerror>` | `result<option<char>,fileerror>` | enum | 6 | 2 |
| `Result<Option<char>,neterror>` | `result<option<char>,neterror>` | enum | 6 | 2 |
| `Result<Option<i32>,neterror>` | `result<option<i32>,neterror>` | enum | 6 | 2 |
| `Result<Option<int>,fileerror>` | `result<option<int>,fileerror>` | enum | 6 | 2 |
| `Result<Option<int>,neterror>` | `result<option<int>,neterror>` | enum | 6 | 2 |
| `Result<Option<string>,fileerror>` | `result<option<string>,fileerror>` | enum | 6 | 2 |
| `Result<Option<string>,neterror>` | `result<option<string>,neterror>` | enum | 6 | 2 |
| `Result<Vector<string>,fileerror>` | `result<vector<string>,fileerror>` | enum | 6 | 2 |
| `Result<bool,fileerror>` | `result<bool,fileerror>` | enum | 6 | 2 |
| `Result<bool,neterror>` | `result<bool,neterror>` | enum | 6 | 2 |
| `Result<cons-cell<Option<Sexpr>,int>,readerror>` | `result<cons-cell<option<sexpr>,int>,readerror>` | enum | 6 | 2 |
| `Result<f64,parsefloaterror>` | `result<f64,parsefloaterror>` | enum | 6 | 2 |
| `Result<i32,fileerror>` | `result<i32,fileerror>` | enum | 6 | 2 |
| `Result<i32,neterror>` | `result<i32,neterror>` | enum | 6 | 2 |
| `Result<int,fileerror>` | `result<int,fileerror>` | enum | 6 | 2 |
| `Result<string,fileerror>` | `result<string,fileerror>` | enum | 6 | 2 |
| `Result<string,neterror>` | `result<string,neterror>` | enum | 6 | 2 |
| `Result<universal-time,fileerror>` | `result<universal-time,fileerror>` | enum | 6 | 2 |
| `Task<t>` | `task<t>` | struct | 6 | 2 |
| `Thread<t>` | `thread<t>` | struct | 6 | 2 |
| `Vector<cons-cell<k,v>>` | `vector<cons-cell<k,v>>` | vector | 6 | 2 |
| `Vector<int>` | `vector<int>` | vector | 6 | 2 |
| `Vector<k>` | `vector<k>` | vector | 6 | 2 |
| `Vector<string>` | `vector<string>` | vector | 6 | 2 |
| `Vector<t>` | `vector<t>` | vector | 6 | 2 |
| `Vector<v>` | `vector<v>` | vector | 6 | 2 |
| `bool` | `bool` | bool | 4 | 0 |
| `c-long` | `c-long` | raw-word | 0 | 0 |
| `c-ulong` | `c-ulong` | raw-word | 0 | 0 |
| `char` | `char` | char | 3 | 0 |
| `e` | `e` | none | 0 | 0 |
| `f32` | `f32` | f32 | 11 | 0 |
| `f64` | `f64` | f64 | 2 | 0 |
| `heap-info` | `heap-info` | none | 0 | 0 |
| `i16` | `i16` | int-any-width | 1 | 0 |
| `i32` | `i32` | int-any-width | 1 | 0 |
| `i8` | `i8` | int-any-width | 1 | 0 |
| `int` | `int` | int | 6 | 2 |
| `internal-time` | `internal-time` | none | 0 | 0 |
| `k` | `k` | none | 0 | 0 |
| `llvm-basic-block` | `llvm-basic-block` | handle | 1 | 0 |
| `llvm-builder` | `llvm-builder` | handle | 1 | 0 |
| `llvm-function` | `llvm-function` | handle | 1 | 0 |
| `llvm-module` | `llvm-module` | handle | 1 | 0 |
| `llvm-value` | `llvm-value` | handle | 1 | 0 |
| `random-state` | `random-state` | random-state | 6 | 2 |
| `ratio` | `ratio` | ratio | 6 | 2 |
| `scope<v>` | `scope<v>` | scope | 6 | 2 |
| `string` | `string` | str | 6 | 2 |
| `symbol` | `symbol` | sym | 6 | 0 |
| `t` | `t` | none | 0 | 0 |
| `u16` | `u16` | int-any-width | 1 | 0 |
| `u32` | `u32` | int-any-width | 1 | 0 |
| `u8` | `u8` | int-any-width | 1 | 0 |
| `universal-time` | `universal-time` | none | 0 | 0 |
| `v` | `v` | none | 0 | 0 |

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
| `rt_frame_data` | `i64 (ptr, i32)` |
| `rt_frame_dyn_call` | `i64 (ptr, i32)` |
| `rt_frame_entered` | `i64 (ptr, i32)` |
| `rt_frame_mask_bit` | `i64 (ptr, i32)` |
| `rt_frame_new` | `i64 (ptr, i32)` |
| `rt_frame_pc` | `i64 (ptr, i32)` |
| `rt_frame_set_pc` | `i64 (ptr, i32)` |
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
| `rt_intern_path` | `i64 (ptr, i32)` |
| `rt_intern_symbol` | `i64 (ptr, i32)` |
| `rt_lisp_implementation_version` | `i64 (ptr, i32)` |
| `rt_list_to_path` | `i64 (ptr, i32)` |
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
| `rt_upcast_set` | `i64 (ptr, i32)` |
| `rt_vtable_set` | `i64 (ptr, i32)` |
| `rt_wk_symbol` | `i64 (ptr, i32)` |

## 組み込み型

### `bool` (sum, 型引数 [])

- インスタンス `eq`: `(bool, bool) -> bool`
- インスタンス `eql`: `(bool, bool) -> bool`
- インスタンス `equal`: `(bool, bool) -> bool`
- インスタンス `equalp`: `(bool, bool) -> bool`

### `c-long` (sum, 型引数 [])

- インスタンス `int->c-long`: `(c-long) -> c-long`
- インスタンス `int->c-ulong`: `(c-long) -> c-ulong`
- インスタンス `int->i16`: `(c-long) -> i16`
- インスタンス `int->i32`: `(c-long) -> i32`
- インスタンス `int->i8`: `(c-long) -> i8`
- インスタンス `int->int`: `(c-long) -> int`
- インスタンス `int->u16`: `(c-long) -> u16`
- インスタンス `int->u32`: `(c-long) -> u32`
- インスタンス `int->u8`: `(c-long) -> u8`
- インスタンス `try-int->c-long`: `(c-long) -> Option<c-long>`
- インスタンス `try-int->c-ulong`: `(c-long) -> Option<c-ulong>`
- インスタンス `try-int->i16`: `(c-long) -> Option<i16>`
- インスタンス `try-int->i32`: `(c-long) -> Option<i32>`
- インスタンス `try-int->i8`: `(c-long) -> Option<i8>`
- インスタンス `try-int->u16`: `(c-long) -> Option<u16>`
- インスタンス `try-int->u32`: `(c-long) -> Option<u32>`
- インスタンス `try-int->u8`: `(c-long) -> Option<u8>`

### `c-ulong` (sum, 型引数 [])

- インスタンス `int->c-long`: `(c-ulong) -> c-long`
- インスタンス `int->c-ulong`: `(c-ulong) -> c-ulong`
- インスタンス `int->i16`: `(c-ulong) -> i16`
- インスタンス `int->i32`: `(c-ulong) -> i32`
- インスタンス `int->i8`: `(c-ulong) -> i8`
- インスタンス `int->int`: `(c-ulong) -> int`
- インスタンス `int->u16`: `(c-ulong) -> u16`
- インスタンス `int->u32`: `(c-ulong) -> u32`
- インスタンス `int->u8`: `(c-ulong) -> u8`
- インスタンス `try-int->c-long`: `(c-ulong) -> Option<c-long>`
- インスタンス `try-int->c-ulong`: `(c-ulong) -> Option<c-ulong>`
- インスタンス `try-int->i16`: `(c-ulong) -> Option<i16>`
- インスタンス `try-int->i32`: `(c-ulong) -> Option<i32>`
- インスタンス `try-int->i8`: `(c-ulong) -> Option<i8>`
- インスタンス `try-int->u16`: `(c-ulong) -> Option<u16>`
- インスタンス `try-int->u32`: `(c-ulong) -> Option<u32>`
- インスタンス `try-int->u8`: `(c-ulong) -> Option<u8>`

### `chan` (struct, 型引数 [t])

- インスタンス `cap`: `(Chan<t>) -> int`
- インスタンス `close`: `(Chan<t>) -> ()`
- インスタンス `len`: `(Chan<t>) -> int`
- 関連関数 `new`: `(int) -> Chan<t>`
- インスタンス `recv`: `(Chan<t>) -> Option<t>`
- インスタンス `send`: `(Chan<t>, t) -> ()`

### `char` (sum, 型引数 [])

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

### `evalerror` (sum, 型引数 [])

- 変種 0 `evalerror`: `string`

### `f32` (sum, 型引数 [])

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
- インスタンス `try-float->f32`: `(f32) -> Option<f32>`
- インスタンス `try-float->f64`: `(f32) -> Option<f64>`

### `f64` (sum, 型引数 [])

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
- インスタンス `try-float->f32`: `(f64) -> Option<f32>`
- インスタンス `try-float->f64`: `(f64) -> Option<f64>`

### `fileerror` (sum, 型引数 [])

- 変種 0 `fileerror`: `string`

### `hashtable` (sum, 型引数 [k, v])

- インスタンス `bucket-count`: `(HashTable<k,v>, int) -> int`
- インスタンス `bucket-delete`: `(HashTable<k,v>, int, int) -> ()`
- インスタンス `bucket-key`: `(HashTable<k,v>, int, int) -> k`
- インスタンス `bucket-put`: `(HashTable<k,v>, int, int, k, v) -> ()`
- インスタンス `bucket-value`: `(HashTable<k,v>, int, int) -> v`
- インスタンス `clear`: `(HashTable<k,v>) -> ()`
- インスタンス `count`: `(HashTable<k,v>) -> int`
- インスタンス `entries`: `(HashTable<k,v>) -> Vector<cons-cell<k,v>>`
- インスタンス `keys`: `(HashTable<k,v>) -> Vector<k>`
- 関連関数 `new`: `() -> HashTable<k,v>`
- インスタンス `values`: `(HashTable<k,v>) -> Vector<v>`

### `i16` (sum, 型引数 [])

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
- インスタンス `try-int->c-long`: `(i16) -> Option<c-long>`
- インスタンス `try-int->c-ulong`: `(i16) -> Option<c-ulong>`
- インスタンス `try-int->char`: `(i16) -> Option<char>`
- インスタンス `try-int->i16`: `(i16) -> Option<i16>`
- インスタンス `try-int->i32`: `(i16) -> Option<i32>`
- インスタンス `try-int->i8`: `(i16) -> Option<i8>`
- インスタンス `try-int->u16`: `(i16) -> Option<u16>`
- インスタンス `try-int->u32`: `(i16) -> Option<u32>`
- インスタンス `try-int->u8`: `(i16) -> Option<u8>`

### `i32` (sum, 型引数 [])

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
- インスタンス `try-int->c-long`: `(i32) -> Option<c-long>`
- インスタンス `try-int->c-ulong`: `(i32) -> Option<c-ulong>`
- インスタンス `try-int->char`: `(i32) -> Option<char>`
- インスタンス `try-int->i16`: `(i32) -> Option<i16>`
- インスタンス `try-int->i32`: `(i32) -> Option<i32>`
- インスタンス `try-int->i8`: `(i32) -> Option<i8>`
- インスタンス `try-int->u16`: `(i32) -> Option<u16>`
- インスタンス `try-int->u32`: `(i32) -> Option<u32>`
- インスタンス `try-int->u8`: `(i32) -> Option<u8>`

### `i8` (sum, 型引数 [])

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
- インスタンス `try-int->c-long`: `(i8) -> Option<c-long>`
- インスタンス `try-int->c-ulong`: `(i8) -> Option<c-ulong>`
- インスタンス `try-int->char`: `(i8) -> Option<char>`
- インスタンス `try-int->i16`: `(i8) -> Option<i16>`
- インスタンス `try-int->i32`: `(i8) -> Option<i32>`
- インスタンス `try-int->i8`: `(i8) -> Option<i8>`
- インスタンス `try-int->u16`: `(i8) -> Option<u16>`
- インスタンス `try-int->u32`: `(i8) -> Option<u32>`
- インスタンス `try-int->u8`: `(i8) -> Option<u8>`

### `int` (sum, 型引数 [])

- インスタンス `*`: `(int, int) -> int`
- インスタンス `+`: `(int, int) -> int`
- インスタンス `-`: `(int, int) -> int`
- インスタンス `/`: `(int, int) -> int`
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
- インスタンス `try-int->c-long`: `(int) -> Option<c-long>`
- インスタンス `try-int->c-ulong`: `(int) -> Option<c-ulong>`
- インスタンス `try-int->char`: `(int) -> Option<char>`
- インスタンス `try-int->i16`: `(int) -> Option<i16>`
- インスタンス `try-int->i32`: `(int) -> Option<i32>`
- インスタンス `try-int->i8`: `(int) -> Option<i8>`
- インスタンス `try-int->u16`: `(int) -> Option<u16>`
- インスタンス `try-int->u32`: `(int) -> Option<u32>`
- インスタンス `try-int->u8`: `(int) -> Option<u8>`

### `llvm-basic-block` (sum, 型引数 [])


### `llvm-builder` (sum, 型引数 [])

- インスタンス `alloca-args`: `(llvm-builder, int) -> llvm-value`
- インスタンス `block-terminated?`: `(llvm-builder) -> bool`
- インスタンス `build-add`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-and`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-ashr`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-br`: `(llvm-builder, llvm-basic-block) -> ()`
- インスタンス `build-call`: `(llvm-builder, llvm-function, llvm-value, int) -> llvm-value`
- インスタンス `build-call-with-env`: `(llvm-builder, llvm-function, llvm-value, int, llvm-value, int) -> llvm-value`
- インスタンス `build-cond-br`: `(llvm-builder, llvm-value, llvm-basic-block, llvm-basic-block) -> ()`
- インスタンス `build-fadd`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fceil`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-fcmp-eq`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fcmp-ge`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fcmp-gt`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fcmp-le`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fcmp-lt`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fcmp-ne`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fcos`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-fdiv`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fexp`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-ffloor`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-flog`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-fmaxnum`: `(llvm-builder, llvm-module, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fminnum`: `(llvm-builder, llvm-module, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fmul`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fn-address`: `(llvm-builder, llvm-function) -> llvm-value`
- インスタンス `build-fpow`: `(llvm-builder, llvm-module, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fptosi`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-free`: `(llvm-builder, llvm-value) -> ()`
- インスタンス `build-frem`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-fround`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-fround32`: `(llvm-builder, llvm-value) -> llvm-value`
- インスタンス `build-fsin`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-fsqrt`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-fsub`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-ftrunc`: `(llvm-builder, llvm-module, llvm-value) -> llvm-value`
- インスタンス `build-icmp-eq`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-icmp-ge`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-icmp-gt`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-icmp-le`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-icmp-lt`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-icmp-ne`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-int-to-ptr`: `(llvm-builder, llvm-value) -> llvm-value`
- インスタンス `build-lshr`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-make-closure`: `(llvm-builder, llvm-module, llvm-function, llvm-value, int, int, int) -> llvm-value`
- インスタンス `build-malloc`: `(llvm-builder, int) -> llvm-value`
- インスタンス `build-mul`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-or`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-ptr-to-int`: `(llvm-builder, llvm-value) -> llvm-value`
- インスタンス `build-ret`: `(llvm-builder, llvm-value) -> ()`
- インスタンス `build-sadd-overflow`: `(llvm-builder, llvm-module, llvm-value, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-select`: `(llvm-builder, llvm-value, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-shl`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-sitofp`: `(llvm-builder, llvm-value) -> llvm-value`
- インスタンス `build-slot-ptr`: `(llvm-builder, llvm-value, int) -> llvm-value`
- インスタンス `build-smul-overflow`: `(llvm-builder, llvm-module, llvm-value, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-ssub-overflow`: `(llvm-builder, llvm-module, llvm-value, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-sub`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `build-xor`: `(llvm-builder, llvm-value, llvm-value) -> llvm-value`
- インスタンス `const-word`: `(llvm-builder, int) -> llvm-value`
- インスタンス `coroutine-apply`: `(llvm-builder, llvm-module, llvm-value, llvm-value, int) -> llvm-value`
- インスタンス `coroutine-begin`: `(llvm-builder, llvm-module, llvm-function) -> ()`
- インスタンス `coroutine-call`: `(llvm-builder, llvm-module, llvm-value, llvm-value, int) -> llvm-value`
- インスタンス `coroutine-call-env`: `(llvm-builder, llvm-module, llvm-value, llvm-value, int, llvm-value, int) -> llvm-value`
- インスタンス `coroutine-dyn-call`: `(llvm-builder, llvm-module, llvm-value, llvm-value, llvm-value, int) -> llvm-value`
- インスタンス `coroutine-end`: `(llvm-builder, llvm-module, llvm-value) -> ()`
- インスタンス `coroutine-suspend`: `(llvm-builder, llvm-module) -> llvm-value`
- インスタンス `coroutine-unwind`: `(llvm-builder, llvm-module) -> ()`
- 関連関数 `create`: `() -> llvm-builder`
- インスタンス `frame-begin`: `(llvm-builder, llvm-module) -> ()`
- インスタンス `frame-clear-handler`: `(llvm-builder, llvm-module) -> ()`
- インスタンス `frame-end`: `(llvm-builder, llvm-module) -> ()`
- インスタンス `frame-set-handler`: `(llvm-builder, llvm-module, llvm-basic-block) -> ()`
- インスタンス `frame-slot`: `(llvm-builder) -> llvm-value`
- インスタンス `frame-slot-rooted`: `(llvm-builder, llvm-module) -> llvm-value`
- インスタンス `frame-value`: `(llvm-builder) -> llvm-value`
- インスタンス `load-arg`: `(llvm-builder, llvm-function, int) -> llvm-value`
- インスタンス `load-env`: `(llvm-builder, llvm-function, int) -> llvm-value`
- インスタンス `load-raw`: `(llvm-builder, llvm-value, int) -> llvm-value`
- インスタンス `position-at-end`: `(llvm-builder, llvm-basic-block) -> ()`
- インスタンス `store-arg`: `(llvm-builder, llvm-value, int, llvm-value) -> ()`

### `llvm-function` (sum, 型引数 [])

- インスタンス `append-block`: `(llvm-function, string) -> llvm-basic-block`
- インスタンス `function-param`: `(llvm-function, int) -> llvm-value`

### `llvm-module` (sum, 型引数 [])

- インスタンス `add-coroutine-function`: `(llvm-module, string) -> llvm-function`
- インスタンス `add-function`: `(llvm-module, string) -> llvm-function`
- インスタンス `add-function-with-env`: `(llvm-module, string) -> llvm-function`
- 関連関数 `create`: `(string) -> llvm-module`
- インスタンス `get-function`: `(llvm-module, string) -> llvm-function`
- インスタンス `to-string`: `(llvm-module) -> string`
- インスタンス `verify`: `(llvm-module) -> bool`

### `llvm-value` (sum, 型引数 [])


### `neterror` (sum, 型引数 [])

- 変種 0 `neterror`: `string`

### `option` (sum, 型引数 [t])

- 変種 0 `some`: `t`
- 変種 1 `none`: 

### `parsefloaterror` (sum, 型引数 [])

- 変種 0 `parsefloaterror`: `string`

### `parseinterror` (sum, 型引数 [])

- 変種 0 `parseinterror`: `string`

### `random-state` (sum, 型引数 [])


### `ratio` (sum, 型引数 [])

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

### `readerror` (sum, 型引数 [])

- 変種 0 `readerror`: `string`

### `result` (sum, 型引数 [t, e])

- 変種 0 `ok`: `t`
- 変種 1 `err`: `e`

### `scope` (sum, 型引数 [v])

- インスタンス `clone-frames`: `(scope<v>) -> scope<v>`
- インスタンス `get`: `(scope<v>, string) -> Option<v>`
- 関連関数 `new`: `() -> scope<v>`
- インスタンス `pop-frame`: `(scope<v>) -> ()`
- インスタンス `push-frame`: `(scope<v>) -> ()`
- インスタンス `set`: `(scope<v>, string, v) -> ()`

### `sexpr` (sum, 型引数 [])

- 変種 0 `nil`: 
- 変種 1 `int`: `int`
- 変種 2 `f64`: `f64`
- 変種 3 `char`: `char`
- 変種 4 `bool`: `bool`
- 変種 5 `sym`: `symbol`
- 変種 6 `str`: `string`
- 変種 7 `cons`: `Option<Sexpr>`, `Option<Sexpr>`
- 変種 8 `bignum`: `int`
- 変種 9 `ratio`: `ratio`
- 変種 10 `path`: `Option<Sexpr>`
- 変種 11 `f32`: `f32`
- 変種 12 `i8`: `i8`
- 変種 13 `i16`: `i16`
- 変種 14 `u8`: `u8`
- 変種 15 `u16`: `u16`
- 変種 16 `u32`: `u32`
- 変種 17 `i32`: `i32`
- インスタンス `eq`: `(Option<Sexpr>, Option<Sexpr>) -> bool`
- インスタンス `eql`: `(Option<Sexpr>, Option<Sexpr>) -> bool`

### `string` (sum, 型引数 [])

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

### `symbol` (sum, 型引数 [])

- インスタンス `eq`: `(symbol, symbol) -> bool`
- インスタンス `eql`: `(symbol, symbol) -> bool`

### `task` (struct, 型引数 [t])

- インスタンス `wait`: `(Task<t>) -> t`

### `thread` (struct, 型引数 [t])

- 関連関数 `available-parallelism`: `() -> int`
- 関連関数 `current-id`: `() -> int`
- インスタンス `join`: `(Thread<t>) -> t`

### `u16` (sum, 型引数 [])

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
- インスタンス `try-int->c-long`: `(u16) -> Option<c-long>`
- インスタンス `try-int->c-ulong`: `(u16) -> Option<c-ulong>`
- インスタンス `try-int->char`: `(u16) -> Option<char>`
- インスタンス `try-int->i16`: `(u16) -> Option<i16>`
- インスタンス `try-int->i32`: `(u16) -> Option<i32>`
- インスタンス `try-int->i8`: `(u16) -> Option<i8>`
- インスタンス `try-int->u16`: `(u16) -> Option<u16>`
- インスタンス `try-int->u32`: `(u16) -> Option<u32>`
- インスタンス `try-int->u8`: `(u16) -> Option<u8>`

### `u32` (sum, 型引数 [])

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
- インスタンス `try-int->c-long`: `(u32) -> Option<c-long>`
- インスタンス `try-int->c-ulong`: `(u32) -> Option<c-ulong>`
- インスタンス `try-int->char`: `(u32) -> Option<char>`
- インスタンス `try-int->i16`: `(u32) -> Option<i16>`
- インスタンス `try-int->i32`: `(u32) -> Option<i32>`
- インスタンス `try-int->i8`: `(u32) -> Option<i8>`
- インスタンス `try-int->u16`: `(u32) -> Option<u16>`
- インスタンス `try-int->u32`: `(u32) -> Option<u32>`
- インスタンス `try-int->u8`: `(u32) -> Option<u8>`

### `u8` (sum, 型引数 [])

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
- インスタンス `try-int->c-long`: `(u8) -> Option<c-long>`
- インスタンス `try-int->c-ulong`: `(u8) -> Option<c-ulong>`
- インスタンス `try-int->char`: `(u8) -> Option<char>`
- インスタンス `try-int->i16`: `(u8) -> Option<i16>`
- インスタンス `try-int->i32`: `(u8) -> Option<i32>`
- インスタンス `try-int->i8`: `(u8) -> Option<i8>`
- インスタンス `try-int->u16`: `(u8) -> Option<u16>`
- インスタンス `try-int->u32`: `(u8) -> Option<u32>`
- インスタンス `try-int->u8`: `(u8) -> Option<u8>`

### `vector` (struct, 型引数 [t])

- インスタンス `get`: `(Vector<t>, int) -> t`
- インスタンス `len`: `(Vector<t>) -> int`
- 関連関数 `new`: `() -> Vector<t>`
- インスタンス `pop`: `(Vector<t>) -> Option<t>`
- インスタンス `push`: `(Vector<t>, t) -> ()`
- インスタンス `set`: `(Vector<t>, int, t) -> ()`

## 組み込み関数

| 関数 | シム | シグネチャ |
|---|---|---|
| `%internal::dribble-start` | `rt_dribble_start` | `(string) -> Result<(),fileerror>` |
| `%internal::dribble-stop` | `rt_dribble_stop` | `() -> Result<(),fileerror>` |
| `%internal::ed-open` | `rt_ed_open` | `(string, int) -> Result<(),fileerror>` |
| `%internal::file-create-directories` | `rt_file_create_directories` | `(string) -> Result<(),fileerror>` |
| `%internal::file-delete` | `rt_file_delete` | `(string) -> Result<(),fileerror>` |
| `%internal::file-directory-p` | `rt_file_directory_p` | `(string) -> bool` |
| `%internal::file-exists-p` | `rt_file_exists_p` | `(string) -> bool` |
| `%internal::file-list-directory` | `rt_file_list_directory` | `(string) -> Result<Vector<string>,fileerror>` |
| `%internal::file-modified-date` | `rt_file_modified_date` | `(string) -> Result<universal-time,fileerror>` |
| `%internal::file-owner-name` | `rt_file_owner_name` | `(string) -> Result<Option<string>,fileerror>` |
| `%internal::file-rename` | `rt_file_rename` | `(string, string) -> Result<(),fileerror>` |
| `%internal::file-truename` | `rt_file_truename` | `(string) -> Result<string,fileerror>` |
| `%internal::make-random-state-fresh` | `rt_make_random_state_fresh` | `() -> random-state` |
| `%internal::net-accept` | `rt_net_accept` | `(i32) -> Result<Option<i32>,neterror>` |
| `%internal::net-buffered-p` | `rt_net_buffered_p` | `(i32) -> Result<bool,neterror>` |
| `%internal::net-connect-begin` | `rt_net_connect_begin` | `(string) -> Result<i32,neterror>` |
| `%internal::net-connect-finish` | `rt_net_connect_finish` | `(i32) -> Result<(),neterror>` |
| `%internal::net-fill` | `rt_net_fill` | `(i32) -> Result<Option<int>,neterror>` |
| `%internal::net-flush` | `rt_net_flush` | `(i32) -> Result<Option<int>,neterror>` |
| `%internal::net-listen` | `rt_net_listen` | `(string, int) -> Result<i32,neterror>` |
| `%internal::net-local-address` | `rt_net_local_address` | `(i32) -> Result<string,neterror>` |
| `%internal::net-peer-address` | `rt_net_peer_address` | `(i32) -> Result<string,neterror>` |
| `%internal::net-peer-subject` | `rt_net_peer_subject` | `(i32) -> Result<Option<string>,neterror>` |
| `%internal::net-pop-byte` | `rt_net_pop_byte` | `(i32) -> Result<Option<int>,neterror>` |
| `%internal::net-pop-char` | `rt_net_pop_char` | `(i32) -> Result<Option<char>,neterror>` |
| `%internal::net-push-byte` | `rt_net_push_byte` | `(i32, int) -> Result<(),neterror>` |
| `%internal::net-push-string` | `rt_net_push_string` | `(i32, string) -> Result<(),neterror>` |
| `%internal::net-resolve-begin` | `rt_net_resolve_begin` | `(string, int) -> Result<i32,neterror>` |
| `%internal::net-resolve-finish` | `rt_net_resolve_finish` | `(i32) -> Result<Option<Vector<string>>,neterror>` |
| `%internal::net-server-name` | `rt_net_server_name` | `(i32) -> Result<Option<string>,neterror>` |
| `%internal::net-set-keepalive` | `rt_net_set_keepalive` | `(i32, bool) -> Result<(),neterror>` |
| `%internal::net-set-keepalive-period` | `rt_net_set_keepalive_period` | `(i32, int) -> Result<(),neterror>` |
| `%internal::net-set-nodelay` | `rt_net_set_nodelay` | `(i32, bool) -> Result<(),neterror>` |
| `%internal::net-shutdown-write` | `rt_net_shutdown_write` | `(i32) -> Result<(),neterror>` |
| `%internal::net-socket-error` | `rt_net_socket_error` | `(i32) -> Result<Option<string>,neterror>` |
| `%internal::net-tls-add-certificate` | `rt_net_tls_add_certificate` | `(i32, string, string, string) -> Result<(),neterror>` |
| `%internal::net-tls-handshake` | `rt_net_tls_handshake` | `(i32) -> Result<Option<int>,neterror>` |
| `%internal::net-tls-listen` | `rt_net_tls_listen` | `(string, int, string, string, Option<string>) -> Result<i32,neterror>` |
| `%internal::net-tls-start` | `rt_net_tls_start` | `(i32, string, Option<string>, Option<string>, Option<string>) -> Result<(),neterror>` |
| `%internal::net-udp-bind` | `rt_net_udp_bind` | `(string, int) -> Result<i32,neterror>` |
| `%internal::net-udp-last-sender` | `rt_net_udp_last_sender` | `(i32) -> Result<string,neterror>` |
| `%internal::net-udp-recv` | `rt_net_udp_recv` | `(i32) -> Result<Option<Vector<int>>,neterror>` |
| `%internal::net-udp-send-to` | `rt_net_udp_send_to` | `(i32, string, Vector<int>) -> Result<bool,neterror>` |
| `%internal::net-unix-connect` | `rt_net_unix_connect` | `(string) -> Result<i32,neterror>` |
| `%internal::net-unix-listen` | `rt_net_unix_listen` | `(string) -> Result<i32,neterror>` |
| `%internal::net-wait` | `rt_suspend_io` | `(i32, int) -> ()` |
| `%internal::net-wait-for` | `rt_suspend_io_for` | `(i32, int, f64) -> bool` |
| `%internal::random-state-copy` | `rt_random_state_copy` | `(random-state) -> random-state` |
| `%internal::random-state-next` | `rt_random_state_next` | `(random-state, int) -> int` |
| `%internal::read-datum-at` | `rt_read_datum_at` | `(string, int, bool) -> Result<cons-cell<Option<Sexpr>,int>,readerror>` |
| `%internal::stream-at-line-start` | `rt_stream_at_line_start` | `(i32) -> Result<bool,fileerror>` |
| `%internal::stream-close` | `rt_stream_close` | `(i32) -> Result<(),fileerror>` |
| `%internal::stream-describe` | `rt_stream_describe` | `(i32) -> string` |
| `%internal::stream-finish-output` | `rt_stream_finish_output` | `(i32) -> Result<(),fileerror>` |
| `%internal::stream-input-p` | `rt_stream_input_p` | `(i32) -> Result<bool,fileerror>` |
| `%internal::stream-listen` | `rt_stream_listen` | `(i32) -> Result<bool,fileerror>` |
| `%internal::stream-open-file` | `rt_stream_open_file` | `(string, int) -> Result<i32,fileerror>` |
| `%internal::stream-open-p` | `rt_stream_open_p` | `(i32) -> bool` |
| `%internal::stream-output-p` | `rt_stream_output_p` | `(i32) -> Result<bool,fileerror>` |
| `%internal::stream-position` | `rt_stream_position` | `(i32) -> Result<int,fileerror>` |
| `%internal::stream-read-byte` | `rt_stream_read_byte` | `(i32) -> Result<Option<int>,fileerror>` |
| `%internal::stream-read-char` | `rt_stream_read_char` | `(i32) -> Result<Option<char>,fileerror>` |
| `%internal::stream-stderr` | `rt_stream_stderr` | `() -> i32` |
| `%internal::stream-stdin` | `rt_stream_stdin` | `() -> i32` |
| `%internal::stream-stdout` | `rt_stream_stdout` | `() -> i32` |
| `%internal::stream-string-input` | `rt_stream_string_input` | `(string) -> i32` |
| `%internal::stream-string-output` | `rt_stream_string_output` | `() -> i32` |
| `%internal::stream-take-output-string` | `rt_stream_take_output_string` | `(i32) -> Result<string,fileerror>` |
| `%internal::stream-unread-char` | `rt_stream_unread_char` | `(i32, char) -> Result<(),fileerror>` |
| `%internal::stream-write-byte` | `rt_stream_write_byte` | `(i32, int) -> Result<(),fileerror>` |
| `%internal::stream-write-string` | `rt_stream_write_string` | `(i32, string) -> Result<(),fileerror>` |
| `command-line-args` | `rt_command_line_args` | `() -> Vector<string>` |
| `compile-file` |  | `(string, string) -> bool` |
| `dump` |  | `(string) -> bool` |
| `equal` | `rt_sexpr_equal` | `<t> (t, t) -> bool` |
| `equalp` | `rt_sexpr_equalp` | `<t> (t, t) -> bool` |
| `eval` | `rt_eval` | `(Option<Sexpr>) -> Result<Option<Sexpr>,evalerror>` |
| `exit` | `rt_exit` | `(int) -> !` |
| `get-dispatch-macro-character` | `rt_get_dispatch_macro_character` | `(char, char) -> Option<(fn (string-input-stream char) Option<Sexpr>)>` |
| `get-internal-real-time` | `rt_get_internal_real_time` | `() -> internal-time` |
| `get-internal-run-time` | `rt_get_internal_run_time` | `() -> internal-time` |
| `get-macro-character` | `rt_get_macro_character` | `(char) -> Option<(fn (string-input-stream char) Option<Sexpr>)>` |
| `get-universal-time` | `rt_get_universal_time` | `() -> universal-time` |
| `getenv` | `rt_getenv` | `(string) -> Option<string>` |
| `heap-info` | `rt_heap_info` | `() -> heap-info` |
| `home-directory` | `rt_home_directory` | `() -> Option<string>` |
| `lisp-implementation-version` | `rt_lisp_implementation_version` | `() -> string` |
| `machine-instance` | `rt_machine_instance` | `() -> Option<string>` |
| `machine-type` | `rt_machine_type` | `() -> string` |
| `machine-version` | `rt_machine_version` | `() -> Option<string>` |
| `macroexpand` | `rt_macroexpand` | `(Option<Sexpr>) -> Result<Option<Sexpr>,evalerror>` |
| `macroexpand-1` | `rt_macroexpand_1` | `(Option<Sexpr>) -> Result<Option<Sexpr>,evalerror>` |
| `parse-float` | `rt_parse_float` | `(string) -> Result<f64,parsefloaterror>` |
| `pprint-indent` | `rt_pprint_indent` | `(symbol, int) -> ()` |
| `pprint-list-exhausted` | `rt_pprint_list_exhausted` | `() -> bool` |
| `pprint-newline` | `rt_pprint_newline` | `(symbol) -> ()` |
| `pprint-pop` | `rt_pprint_pop` | `() -> Option<Sexpr>` |
| `pprint-tab` | `rt_pprint_tab` | `(symbol, int, int) -> ()` |
| `read` | `rt_read` | `(string) -> Result<Option<Sexpr>,readerror>` |
| `seed-random-state` | `rt_seed_random_state` | `(int) -> random-state` |
| `set-dispatch-macro-character` | `rt_set_dispatch_macro_character` | `(char, char, (fn (string-input-stream char) Option<Sexpr>)) -> ()` |
| `set-macro-character` | `rt_set_macro_character` | `(char, (fn (string-input-stream char) Option<Sexpr>)) -> ()` |
| `sexpr-atom` | `rt_atom` | `(Option<Sexpr>) -> bool` |
| `sexpr-bool` | `rt_sexpr_bool` | `(Option<Sexpr>) -> bool` |
| `sexpr-car` | `rt_car` | `(Option<Sexpr>) -> Option<Sexpr>` |
| `sexpr-cdr` | `rt_cdr` | `(Option<Sexpr>) -> Option<Sexpr>` |
| `sexpr-char` | `rt_sexpr_char` | `(Option<Sexpr>) -> char` |
| `sexpr-cons` | `rt_cons` | `(Option<Sexpr>, Option<Sexpr>) -> Option<Sexpr>` |
| `sexpr-consp` | `rt_consp` | `(Option<Sexpr>) -> bool` |
| `sexpr-f32` | `rt_sexpr_f32` | `(Option<Sexpr>) -> f32` |
| `sexpr-f64` | `rt_sexpr_f64` | `(Option<Sexpr>) -> f64` |
| `sexpr-i16` | `rt_sexpr_i16` | `(Option<Sexpr>) -> i16` |
| `sexpr-i32` | `rt_sexpr_i32` | `(Option<Sexpr>) -> i32` |
| `sexpr-i8` | `rt_sexpr_i8` | `(Option<Sexpr>) -> i8` |
| `sexpr-int` | `rt_sexpr_int` | `(Option<Sexpr>) -> int` |
| `sexpr-null` | `rt_null` | `(Option<Sexpr>) -> bool` |
| `sexpr-str` | `rt_sexpr_str` | `(Option<Sexpr>) -> string` |
| `sexpr-sym-name` | `rt_sym_name` | `(Option<Sexpr>) -> string` |
| `sexpr-symp` | `rt_symp` | `(Option<Sexpr>) -> bool` |
| `sexpr-u16` | `rt_sexpr_u16` | `(Option<Sexpr>) -> u16` |
| `sexpr-u32` | `rt_sexpr_u32` | `(Option<Sexpr>) -> u32` |
| `sexpr-u8` | `rt_sexpr_u8` | `(Option<Sexpr>) -> u8` |
| `sleep` | `rt_suspend_sleep` | `(f64) -> ()` |
| `software-type` | `rt_software_type` | `() -> string` |
| `software-version` | `rt_software_version` | `() -> Option<string>` |
| `string->symbol` | `rt_intern_symbol` | `(string) -> symbol` |
| `symbol->string` | `rt_sym_name` | `(symbol) -> string` |
| `timezone-daylight-p` | `rt_timezone_daylight_p` | `(int, int) -> Option<bool>` |
| `timezone-offset-seconds` | `rt_timezone_offset_seconds` | `(int, int) -> Option<int>` |
| `yield` | `rt_suspend_yield` | `() -> ()` |

