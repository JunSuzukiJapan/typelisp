# Common Lisp との差分 — 未実装のクラス（型）とメソッド（関数）の全リスト

作成: 2026-07-29 / 対象ブランチ: `feature/vscode-extension`（`main` からの差分はエディタ関連のみ）

このドキュメントは **ANSI Common Lisp（CLHS）に存在して typelisp に無いもの** を、クラス（型）と
メソッド（関数・マクロ・特殊形）に分けて網羅列挙する。「CL 同等の表現力のために何を足すか」を
提案した [cl-equivalence-catalog.md](cl-equivalence-catalog.md)（2026-06-18、提案項目はほぼ実装済み）
とは目的が異なり、**こちらは残差の棚卸し**である。

実装状況は docs（古い可能性がある）ではなく、以下を直接読んで確認した:

- `src/check/registry.rs` — Rust 組み込み型・組み込みメソッドの登録表
- `src/check/checker.rs` — 特殊形の一覧（`check` の文字列 match）
- `src/prelude.rs` — typelisp で書かれた標準ライブラリ（`defun`/`defmethod`/`defmacro`/`impl`）
- `src/eval/interp.rs` — 組み込みメソッドの実体

## 0. 記号の意味

| 記号 | 意味 |
|---|---|
| ❌ | **未実装**。設計方針とは矛盾しないので、やろうと思えば追加できる |
| ⚠️ | **部分実装 / 差異あり**。相当物はあるが CL と意味・引数・戻り値が違う |
| ⛔ | **設計上の対象外**。言語設計の確定事項（[language-design.md](language-design.md)）と衝突する |
| ✅ | 実装済み（別名・別形のものだけ、対応関係の確認用に載せる） |

⛔ の根拠になっている設計上の確定事項は次の5つ。以降で何度も参照するので番号を振る。

- **(D1) 静的型付け**: すべての式の型がコンパイル時に決まる。動的型の値は `Sexpr` に限られ、
  取り出すには `match` が要る。→ CL の「実行時に型を問い合わせる」系 API が原理的に載らない。
- **(D2) `nil` が無い**: 偽は `false`、空リストは `Sexpr::Nil`、「値が無い」は `Option<T>`。
  CL の「nil を偽・空リスト・失敗の三役で使う」慣用が全滅する（[[typelisp-language-spec]]）。
- **(D3) コンディションシステムを採らない**: 回復可能な失敗は `Result<T,E>`、回復不能は `panic`
  （§7.1）。非局所脱出は `break`/`return`（直近ループのみ）だけ。
- **(D4) 破壊的操作を原則採らない**: `nreverse`/`nconc`/`rplaca` 等は撤去済み
  （[[typelisp-vector-defstruct-revert]]）。書き換えは `setf` で場所を明示する。
- **(D5) 動的束縛（special 変数）が無い**: `let` は常に字句束縛。CL の `*print-\*`/`*read-\*` 等の
  制御変数は「代入可能なグローバル」に読み替えている（functions.md §15.1）。

---

## 1. クラス（型）

CLHS Figure 4-8（standardized atomic type specifiers）と 4.3.7（クラス階層）に載る型を全部並べる。

### 1.1 数値

| CL のクラス | typelisp | 備考 |
|---|---|---|
| `number` | ⛔ | 数値型を束ねる抽象型が無い。`i32`/`i64`/`f64`/`bignum`/`ratio` は互いに独立で暗黙変換もない (D1) |
| `real` | ⛔ | 同上 |
| `rational` | ⛔ | 同上（`bignum` と `ratio` を束ねる型が無い） |
| `integer` | ⚠️ | `i32`/`i64`/`bignum` が別型として存在。CL のような「fixnum→bignum の自動昇格」は無い |
| `fixnum` | ✅ | `i32` / `i64`（`i8`/`i16`/`u8`/`u16`/`u32`/`u64`/`isize`/`usize` は**型登録だけで演算メソッドが1つも無い**） |
| `bignum` | ✅ | `bignum` |
| `ratio` | ✅ | `ratio` |
| `float` | ⚠️ | `f64` のみ。`f32` は型登録だけで演算が無い |
| `short-float` / `single-float` / `double-float` / `long-float` | ❌ | 精度別のサブタイプが無い |
| `complex` | ❌ | 複素数そのものが無い。`sqrt`/`log`/`expt` が CL では複素数を返す場面で panic か NaN になる |

### 1.2 文字・シンボル・真偽

| CL のクラス | typelisp | 備考 |
|---|---|---|
| `character` | ✅ | `char`（Unicode スカラ値） |
| `base-char` / `standard-char` / `extended-char` | ⛔ | 文字型のサブタイプ区分が無い |
| `symbol` | ✅ | `Symbol`（`string->symbol` で intern、`symbol->string` で名前） |
| `keyword` | ⚠️ | 独立した型は無い。`:name` は先頭がコロンのシンボルで、`keywordp` で判定するだけ |
| `boolean` | ✅ | `bool`（`true`/`false`） |
| `null` | ⛔ | (D2)。`Sexpr::Nil` は `Sexpr` の1構成子であって型ではない |
| `t`（型としての） | ⛔ | すべての値を含む上位型が無い (D1)。近いのは `Sexpr`（`Sexpr` 表現を持つ値に限る）と `:dyn Trait` |
| `nil`（型としての、空型） | ✅ | `!`（Never）が相当（§7.2） |
| `atom` | ⛔ | 「cons でないもの」という型が無い。`Sexpr` に対する述語 `sexpr-atom` はある |

### 1.3 リスト・シーケンス・配列

| CL のクラス | typelisp | 備考 |
|---|---|---|
| `cons` | ⚠️ | 2つある。汎用ペア `cons-cell<A,B>`（`defstruct`）と `Sexpr` の `Cons` 構成子。前者は静的な要素型を持ち、CL の「異種の入れ子」は `Sexpr` 側だけが担う |
| `list` | ⛔ | 型としての `list` が無い。`Sexpr` のリストは「`Cons` 連鎖である `Sexpr` 値」であり、静的には長さも要素型も区別されない |
| `sequence` | ⚠️ | 抽象型としては無い。代替は `Iter` トレイト（`Vector<T>`/`HashTable<K,V>` が実装）。**`Sexpr` のリストは意図的に `Iter` を実装しない**（language-design.md §9） |
| `array` | ❌ | 多次元配列が無い。`Vector<T>` は1次元のみ |
| `vector` | ⚠️ | `Vector<T>`（可変長・要素型が一様）。CL の `fill-pointer`/`adjustable` の概念は無い |
| `simple-vector` / `simple-array` | ⛔ | simple 系のサブタイプ区分が無い |
| `bit-vector` / `simple-bit-vector` | ❌ | ビットベクタが無い（`bit`/`sbit`/`bit-and` 系も同様に無い） |
| `string` | ⚠️ | `string` は**不変**。CL の「文字の配列」ではないので `(setf (char s i) c)` に相当する操作が無い |
| `base-string` / `simple-string` | ⛔ | サブタイプ区分が無い |
| `hash-table` | ✅ | `HashTable<K,V>` |

### 1.4 関数・オブジェクトシステム

| CL のクラス | typelisp | 備考 |
|---|---|---|
| `function` | ✅ | `(fn (T...) R)` 型。名前付き関数もそのまま値になり、関数値は `(f args...)` で直接呼べる（Lisp-1 流。`funcall` 不要） |
| `compiled-function` | ⛔ | 型としての区別は無い（`compile` はあるが型は変わらない） |
| `generic-function` / `standard-generic-function` | ⛔ | 総称関数が無い。単一・静的ディスパッチの `defmethod` と、trait（`deftrait`/`impl`/`:dyn`）で代替 |
| `method` / `method-combination` | ⛔ | メソッドが第一級オブジェクトでない。`:before`/`:after`/`:around` と `call-next-method` も無い |
| `class` / `standard-class` / `built-in-class` / `structure-class` | ❌ | メタオブジェクトが無い（`class-of`/`find-class`/MOP なし） |
| `standard-object` | ⛔ | CLOS のインスタンスが無い |
| `structure-object` | ✅ | `defstruct` のインスタンス |
| `t`（クラスとしての） | ⛔ | 上と同じ (D1) |

### 1.5 実行時システムのオブジェクト

| CL のクラス | typelisp | 備考 |
|---|---|---|
| `package` | ⚠️ | `module`/`use`/`pub` があるが、**パッケージは実行時オブジェクトではない**（値として取り回せない、`find-package` 等が無い） |
| `pathname` / `logical-pathname` | ❌ | パス名の型が無い。ファイルパスは `string` |
| `stream` および全サブクラス（`file-stream`/`string-stream`/`broadcast-stream`/`concatenated-stream`/`echo-stream`/`synonym-stream`/`two-way-stream`/`string-input-stream`/`string-output-stream`) | ❌ | **第一級ストリームが無い**。これが単体では最大の欠落で、§2.19〜§2.21 の関数群がまとめて落ちる |
| `readtable` | ❌ | リーダマクロを登録する表が無い（リーダの構文は固定） |
| `random-state` | ❌ | 乱数状態が値として無い（`random` は暗黙のグローバル状態を使う） |
| `restart` | ⛔ | (D3) |
| `condition` および全サブクラス（`serious-condition`/`error`/`warning`/`simple-condition`/`arithmetic-error`/`division-by-zero`/`floating-point-*`/`cell-error`/`unbound-variable`/`unbound-slot`/`undefined-function`/`control-error`/`file-error`/`package-error`/`parse-error`/`print-not-readable`/`program-error`/`reader-error`/`storage-condition`/`stream-error`/`end-of-file`/`type-error`/`style-warning` …） | ⛔ | (D3)。代替は `Error` トレイト＋操作ごとの具象エラー型（`ParseIntError`/`ParseFloatError`/`ReadError`/`EvalError`）＋ユーザ定義エラー型（functions.md §7.1）。**CL の標準コンディション型に一対一で対応する型は無い** |

---

## 2. メソッド・関数・マクロ・特殊形（CLHS 章順）

各節、**足りないものだけ**を挙げる（対応関係の確認に要るものだけ ✅ を併記）。

### 2.1 評価とコンパイル（CLHS 3）

| CL | 状態 | 備考 |
|---|---|---|
| `eval` | ✅ | 戻り型は `Result<Sexpr,EvalError>` 固定（functions.md §16） |
| `macroexpand` / `macroexpand-1` / `*macroexpand-hook*` | ❌ | マクロ展開結果をプログラムから覗く手段が無い（デバッグ時に効く） |
| `eval-when` | ❌ | コンパイル時／ロード時／実行時の区別が無い |
| `macrolet` / `symbol-macrolet` | ❌ | ローカルマクロ |
| `define-compiler-macro` / `compiler-macro-function` | ❌ | |
| `load-time-value` | ❌ | |
| `declare` / `declaim` / `proclaim` / `locally` | ⛔ | 型宣言は不要 (D1)、`optimize`/`inline`/`special` も現状概念が無い |
| `the` | ✅ | 型注釈として実装済み（実行時効果なし） |
| `function` (`#'`) | ⚠️ | 関数名をそのまま値として書けるので `#'` 構文は無い |
| `funcall` | ⚠️ | 関数値は `(f args...)` で直接呼べる（Lisp-1）。関数名の名前空間が分かれていないため不要 |
| `apply` | ✅ | 特殊形。`&rest` を持つ可変長関数にのみ適用できる |
| `compile` / `compile-file` | ⚠️ | 実体は LLVM JIT / AOT ネイティブ実行ファイル生成。CL の「fasl を作る」意味とは違う（fasl 相当は `load` と `typl compile-module`） |
| `constantly` | ❌ | `const` はあるが 2引数版（`(const x y)`）で、クロージャを返す `constantly` とは別物 |
| `complement` | ❌ | 述語の否定を返す高階関数 |
| `identity` | ✅ | |

### 2.2 型とクラス（CLHS 4）

| CL | 状態 | 備考 |
|---|---|---|
| `typep` / `type-of` / `subtypep` | ⛔ | 実行時の型問い合わせ (D1)。`Sexpr` に限れば `match` と `sexpr-consp`/`sexpr-symp` 等が相当 |
| `coerce` | ⚠️ | 数値・文字間の変換は `as`/`try-as` 特殊形。シーケンス間の変換（`(coerce x 'list)` 等）は無い |
| `deftype` | ❌ | 型別名（type alias）が書けない。`defstruct`/`defenum` で新しい型を作るしかない |
| `check-type` | ⛔ | (D1)(D3) |
| `type-error` 系 | ⛔ | (D3) |

### 2.3 データと制御フロー（CLHS 5）

| CL | 状態 | 備考 |
|---|---|---|
| `values` / `values-list` / `multiple-value-bind` / `multiple-value-call` / `multiple-value-list` / `multiple-value-prog1` / `multiple-value-setq` / `nth-value` | ⛔ | **多値が無い**。`(values ...)` という名前は `HashTable` のメソッドとして別用途で使われている。複数の結果は `cons-cell` か `defstruct` で返す |
| `setf` | ⚠️ | 実装済みだが **place は「変数」と「変数::フィールド」の2種のみ**。ユーザ定義の setf 展開子（`defsetf`/`define-setf-expander`）も無く、`(setf (aref a i) v)`/`(setf (gethash k h) v)`/`(setf (car x) v)` のような「関数呼び出し形の place」は書けない |
| `psetf` / `psetq` / `setq` / `shiftf` / `rotatef` | ❌ | 上の place 機構が要る |
| `incf` / `decf` / `push` / `pop` / `pushnew` / `remf` | ❌ | 同上（`push`/`pop` は `Vector<T>` のメソッドとして同名で存在するが、CL の place マクロとは別物） |
| `block` / `return-from` | ⚠️ | `return` はあるが **直近のループからしか脱出できない**。名前付きブロックも関数からの早期リターンも無い |
| `tagbody` / `go` | ⛔ | goto |
| `catch` / `throw` | ⛔ | (D3) |
| `unwind-protect` | ⛔ | (D3)。後始末を保証する構文が無いのは、ファイル等のリソースを持つようになったときに効いてくる |
| `destructuring-bind` | ❌ | `defmacro` のラムダリストでは分配束縛ができる（`&optional`/`&key` 含む）が、式としての `destructuring-bind` は無い |
| `prog` / `prog*` / `prog1` / `prog2` | ❌ | `progn` ✅。`prog1`（最初の値を返す）は素直に書けるので優先度は低い |
| `typecase` / `etypecase` / `ctypecase` | ⛔ | (D1)。`match` が相当 |
| `ecase` / `ccase` | ❌ | `case` ✅（`equal` 比較・`else` 節）。網羅性を要求する `ecase` は無い |
| `sleep` | ❌ | |

### 2.4 反復（CLHS 6）

| CL | 状態 | 備考 |
|---|---|---|
| 拡張 `loop`（`for`/`in`/`across`/`collect`/`sum`/`when`/`finally` …） | ❌ | typelisp の `loop` は**無限ループのみ**で、CL の LOOP DSL とは名前が同じだけの別物。`collect`/`sum` 等の集約は `map`/`foldl` を使う |
| `do` / `do*` | ⚠️ | `do` ✅（`defmacro`）。逐次版 `do*` は無い |
| `dolist` / `dotimes` | ✅ | `dolist` は `Sexpr` の cons リストを歩く（要素は `Sexpr`）。`doiter` が `Iter` 版 |
| `mapc` / `mapcar` / `mapcan` / `mapl` / `maplist` / `mapcon` | ⚠️ | `map`（`Iter` 用）と `sexpr-map`（`Sexpr` リスト用）のみ。**複数シーケンスを同時に走査する版が無い**（CL の `(mapcar #'f a b)`）ので zip 相当が書けない |

### 2.5 オブジェクト（CLHS 7、CLOS）

CLOS 全体が ⛔（`deftrait`/`impl`/`:dyn` と `defstruct`/`defenum` で置き換える方針、§5.1/§5.2）。
対応の有無だけ記す。

| CL | 状態 | 備考 |
|---|---|---|
| `defclass` / `make-instance` / `slot-value` / `with-slots` / `with-accessors` / `slot-boundp` / `slot-makunbound` | ⛔ | `defstruct` ＋ `Type::new` ＋ `変数::field` で代替。**スロットの未束縛状態は無い**（全フィールド必須） |
| `defgeneric` / `defmethod`（CLOS の） / `call-next-method` / `next-method-p` / `:before` `:after` `:around` / `define-method-combination` | ⛔ | typelisp の `defmethod` は「受け手の静的型で一意に解決する単一ディスパッチ」で別物。多重ディスパッチもメソッド結合も無い |
| `initialize-instance` / `shared-initialize` / `reinitialize-instance` / `change-class` / `update-instance-for-*` | ⛔ | コンストラクタは自動生成の `new` のみ。初期化フックが無い |
| `class-of` / `find-class` / `class-name` / MOP 全般 | ⛔ | (D1) |
| `print-object` | ✅ | トレイトとして実装済み（functions.md §15.2） |
| `describe` / `describe-object` / `inspect` | ❌ | 値の構造を人間向けに吐く汎用関数（デバッグ用）。`~s` である程度代替できる |
| `make-load-form` | ❌ | |

### 2.6 構造体（CLHS 8）

| CL | 状態 | 備考 |
|---|---|---|
| `defstruct` 本体 | ✅ | |
| `:include`（構造体の継承） | ❌ | 継承が無い（トレイトで共通の振る舞いだけは括れる） |
| `:constructor`（BOA コンストラクタ・複数コンストラクタ） | ❌ | 自動生成の `new`（全フィールドを位置引数で受ける）のみ。キーワード引数コンストラクタが無い |
| `:conc-name` / `:predicate` / `:copier` | ❌ | アクセサ名の変更、`point-p` 述語、`copy-point` が生成されない |
| `:print-function` / `:print-object` | ✅ | `impl print-object` が相当 |
| `:type` / `:initial-offset` / `:named` | ⛔ | 表現を list/vector に変える指定は (D1) と衝突 |
| スロットの初期値 | ❌ | フィールドのデフォルト値が書けない |

### 2.7 コンディション（CLHS 9）

全体が ⛔ (D3)。`Result<T,E>` + `match` + `panic` で書き換える方針。

| CL | 状態 |
|---|---|
| `define-condition` / `make-condition` | ⛔（`defstruct`/`defenum` + `impl Error` が代替） |
| `signal` / `error` / `cerror` / `warn` / `break` | ⛔（`panic` のみ。**警告を出して続行する仕組みが無い**） |
| `handler-case` / `handler-bind` / `ignore-errors` | ⛔（`match` で `Result` を分岐） |
| `restart-case` / `restart-bind` / `with-simple-restart` / `invoke-restart` / `find-restart` / `compute-restarts` / `abort` / `continue` / `muffle-warning` / `store-value` / `use-value` | ⛔ |
| `assert` | ❌（コンディション抜きの「条件が偽なら panic」なら追加可能） |
| `invoke-debugger` / `*debugger-hook*` | ⛔ |

### 2.8 シンボル（CLHS 10）

| CL | 状態 | 備考 |
|---|---|---|
| `symbol-name` / `intern` | ✅ | `symbol->string` / `string->symbol`。ただし**パッケージ引数が無い** |
| `keywordp` | ✅ | |
| `make-symbol` / `copy-symbol` / `gentemp` | ❌ | uninterned シンボルを作る手段が `gensym` だけ |
| `gensym` / `*gensym-counter*` | ⚠️ | `gensym` ✅（引数なし版のみ、プレフィクス指定不可）。カウンタ変数は無い |
| `symbol-value` / `set` / `boundp` / `makunbound` | ⛔ | (D1)(D5) |
| `symbol-function` / `fboundp` / `fmakunbound` / `fdefinition` | ⛔ | 関数を名前で実行時に引く操作 (D1)。`eval` で部分的に代替 |
| `symbol-plist` / `get` / `remprop` / `getf` / `get-properties` | ❌ | プロパティリストが無い（`get` は `Vector`/`HashTable` のメソッド名として別用途） |
| `symbol-package` | ⛔ | パッケージが実行時オブジェクトでない |
| `defconstant` / `defparameter` / `defvar` | ⚠️ | `defconstant` ✅ / `defvar` ✅。`defparameter` との区別（再ロード時に再初期化するか）は無い |

### 2.9 パッケージ（CLHS 11）

| CL | 状態 | 備考 |
|---|---|---|
| `defpackage` / `in-package` | ⚠️ | `module`（入れ子で書く）とファイル↔モジュール対応が相当。ファイル冒頭で名前空間を宣言する `in-package` 形式は無い |
| `export` / `unexport` | ✅ | `pub`（定義ごとに1つずつ、flat 形式のみ） |
| `use-package` / `unuse-package` | ⚠️ | `use` ✅。取り消しは無い |
| `import` / `shadowing-import` / `shadow` | ❌ | 個別シンボルの取り込み・遮蔽 |
| `find-package` / `package-name` / `package-nicknames` / `list-all-packages` / `delete-package` / `rename-package` / `package-use-list` | ⛔ | パッケージが実行時オブジェクトでない |
| `find-symbol` / `unintern` / `do-symbols` / `do-external-symbols` / `do-all-symbols` / `with-package-iterator` | ⛔ | 同上 |
| `*package*` | ⛔ | (D5) |

### 2.10 数値（CLHS 12）

typelisp で**最も数が多い欠落**がここ。以下はすべて ❌（実装可能）。

**述語** — CL では基本中の基本だが、1つも無い:

`zerop` `plusp` `minusp` `evenp` `oddp` `numberp` `integerp` `rationalp` `floatp` `realp`
`complexp`（後半5つは (D1) で ⛔ 寄り）

**基本演算**:

| CL | 状態 | 備考 |
|---|---|---|
| `+` `-` `*` `/` `=` `/=` `<` `<=` `>` `>=` | ⚠️ | すべて **2引数固定**。CL の可変長（`(+ 1 2 3)`、`(< a b c)`）が書けない |
| `max` / `min` | ❌ | 無い（`if` で書くしかない）。使用頻度からして最優先級 |
| `1+` / `1-` | ❌ | |
| `abs` `signum` `gcd` `lcm` `mod` `rem` `expt` | ✅ | 型ごとのメソッド。ただし `gcd`/`lcm` は 2引数固定、整数の `expt` は無い（`bignum` 経由） |
| `floor` `ceiling` `round` `truncate` | ⚠️ | 1引数版（`f64→f64`）はCL相当。~~除数を取る2引数版も商・剰余の多値も無い~~ → **2026-07-29 `floor-div`/`ceiling-div`/`round-div`/`truncate-div` として実装済み**（`i32`/`i64`/`f64`、商・剰余を`cons-cell`で返す。多値そのものは非採用、§3.4参照）。CL と同名の2引数オーバーロードにしなかったのは `defmethod` が受け手の型でのみ解決しアリティでは解決しないため |
| `ffloor` `fceiling` `fround` `ftruncate` | ❌ | |
| `sqrt` | ⚠️ | `f64` のみ。`isqrt` は無い |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | ❌ | **超越関数が1つも無い**。`pi` 定数も無い |
| `float` `rational` `rationalize` | ⚠️ | `int->float`/`float->ratio` 等の個別変換はある。`rationalize`（近似有理数化）は無い |
| `numerator` / `denominator` | ✅ | |
| `complex` `realpart` `imagpart` `conjugate` `phase` `cis` | ❌ | 複素数が無いため |
| `float-sign` `float-digits` `float-precision` `decode-float` `integer-decode-float` `scale-float` `float-radix` | ❌ | 浮動小数点の内部表現へのアクセス |
| `random` | ⚠️ | `(random n)` の `i32` 版のみ。`random-state` も `make-random-state` も `*random-state*` も無く、**シードを固定した再現可能な乱数が作れない** |

**ビット演算** — 全滅（❌）:

`logand` `logior` `logxor` `lognot` `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1`
`logorc2` `logbitp` `logcount` `logtest` `ash` `integer-length` `byte` `byte-size` `byte-position`
`ldb` `ldb-test` `dpb` `mask-field` `deposit-field` `boole`

**定数** — 全滅（❌）:

`most-positive-fixnum` `most-negative-fixnum` `most-positive-double-float` `least-positive-*`
`double-float-epsilon` `pi` など

### 2.11 文字（CLHS 13）

| CL | 状態 | 備考 |
|---|---|---|
| `char=` `char<` `char<=` `char>` `char>=` | ⚠️ | 順序比較は `<`/`<=`/`>`/`>=` が `char` に多重定義されて ✅。等価は `equal`（`eq`/`eql` も同義）で、**数値と違い `=`/`/=` は `char` に定義されていない** |
| `char/=` | ❌ | `(not (equal a b))` と書く |
| `char-equal` / `char-lessp` 等（大文字小文字無視版） | ⚠️ | `equalp` のみ ✅。順序比較の大文字小文字無視版は無い |
| `char-code` / `code-char` | ✅ | `char->int` / `int->char`（+ `try-int->char`） |
| `char-upcase` / `char-downcase` | ✅ | `upcase` / `downcase`（ASCII のみ） |
| `alpha-char-p` / `digit-char-p` | ⚠️ | `alphap` ✅ / `digitp` は **bool を返す**（CL は数字の重みか nil）。基数引数も無い |
| `alphanumericp` `graphic-char-p` `standard-char-p` `upper-case-p` `lower-case-p` `both-case-p` `characterp` | ❌ | |
| `char-name` / `name-char` / `char-int` / `digit-char` | ❌ | |
| `char-code-limit` | ❌ | |

### 2.12 コンス（CLHS 14）

`cons`/`car`/`cdr` は `cons-cell<A,B>` として ✅（`Sexpr` 側は `sexpr-car`/`sexpr-cdr`）。以下は無い。

| CL | 状態 | 備考 |
|---|---|---|
| `caar` … `cddddr`（28個） | ❌ | 合成アクセサ。`(car (cdr x))` と書けば済むが CL コードの移植では頻出 |
| `first` … `tenth` / `rest` | ❌ | |
| `list` / `list*` | ⚠️ | `list` ✅（特殊形、`Sexpr` を作る）。`list*` は無い |
| `make-list` / `copy-list` / `copy-tree` / `copy-alist` | ❌ | `copy-list` は Phase 5 で削除済み |
| `nth` / `nthcdr` | ⚠️ | `nth` は `Iter` 用で ✅、**`Sexpr` リストには使えない**。`nthcdr` は削除済み |
| `last` / `butlast` | ⚠️ | 両方 `Iter` 用で ✅。`last` は CL と違い**最後のセルでなく最後の要素**を返す。`nbutlast` は (D4) |
| `list-length` / `endp` / `null` / `consp` / `atom` / `listp` | ⚠️ | `Sexpr` 版の `sexpr-null`/`sexpr-consp`/`sexpr-atom` は ✅。汎用の `null`/`consp`/`atom` は削除済み |
| `rplaca` / `rplacd` | ⛔ | (D4)。`(setf x::car v)` を使う |
| `nconc` / `nreverse` / `nbutlast` / `nsubst` 等の n 系 | ⛔ | (D4) |
| `revappend` / `nreconc` | ❌ | |
| `append` | ⚠️ | `Iter` 版（2引数）と `string` 版と `sexpr-append` がある。CL の可変長・任意個は無い |
| `member` / `member-if` / `member-if-not` | ⚠️ | `member` は **`bool` を返す**（CL は残りのリスト）。`-if` 版は無い |
| `assoc` / `assoc-if` / `rassoc` / `rassoc-if` / `acons` / `pairlis` | ⚠️ | `assoc` ✅（`Eq K` 境界、`:test`/`:key` 無し）。ほかは無い |
| `sublis` / `subst` / `subst-if` / `tree-equal` | ❌ | 木の書き換え。マクロ処理で効く |
| `union` / `intersection` / `set-difference` / `set-exclusive-or` / `subsetp` / `adjoin` | ❌ | **集合演算が全滅** |
| `ldiff` / `tailp` | ❌ | |
| `getf` / `get-properties` | ❌ | プロパティリスト |

### 2.13 配列（CLHS 15）

多次元配列そのものが無いため全滅（❌）:

`make-array` `aref` `row-major-aref` `array-dimension` `array-dimensions` `array-rank`
`array-total-size` `array-element-type` `array-in-bounds-p` `array-row-major-index`
`array-displacement` `adjustable-array-p` `adjust-array` `fill-pointer` `array-has-fill-pointer-p`
`vector-push` `vector-push-extend` `vector-pop` `svref` `arrayp` `vectorp` `simple-vector-p`
`bit` `sbit` `bit-and` `bit-ior` `bit-xor` `bit-not` `bit-vector-p` `upgraded-array-element-type`

`Vector<T>` の `push`/`pop`/`get`/`set`/`len` が1次元の範囲を最小限カバーしているだけ。

### 2.14 文字列（CLHS 16）

| CL | 状態 | 備考 |
|---|---|---|
| `string=` `string<` `string<=` `string>` `string>=` | ✅ | `equal`（内容比較）と `<`/`<=`/`>`/`>=`（`string` に多重定義）。`char` と同じく `=`/`/=` は無い |
| `string/=` | ❌ | `(not (equal a b))` と書く。CL のように**不一致位置を返す**用途は代替が無い |
| `string-equal` / `string-lessp` 等（大文字小文字無視版） | ⚠️ | `equalp` のみ。順序比較版は無い |
| `char` / `schar` | ✅ | `ref`（範囲外は panic） |
| `subseq`（文字列に対して） | ✅ | `substring` |
| `string-upcase` / `string-downcase` | ✅ | `upcase` / `downcase`（ASCII のみ、`:start`/`:end` 無し） |
| `string-capitalize` / `nstring-*` | ❌ | 各語頭を大文字化 |
| `string-trim` / `string-left-trim` / `string-right-trim` | ❌ | **トリムが無い**。行入力を扱うと即欲しくなる |
| `concatenate` | ⚠️ | `append`（2引数）のみ |
| `make-string` / `string` / `stringp` / `simple-string-p` | ❌ | 「文字を n 個並べた文字列」も「値を文字列化する汎用 `string`」も無い（後者は `(format false "~a" x)` で代替） |
| `search` / `mismatch`（文字列検索） | ❌ | **部分文字列検索が無い** |
| `split-sequence` 相当 | ❌ | CL 標準にも無いが、実用上ほぼ必ず要る |
| `parse-integer` | ✅ | `parse-int`（`Result` を返す）。`:radix`/`:junk-allowed` は無い |

### 2.15 シーケンス（CLHS 17）

`Iter` トレイト上のジェネリック関数として一通り揃っている（functions.md §6）。差分だけ:

| CL | 状態 | 備考 |
|---|---|---|
| `length` `elt` `subseq` `reverse` `sort` `find` `position` `count` `remove-if` `every` `some` `reduce` `map` | ✅ | ただし `find`/`position`/`count` は**述語版だけ**（CL の `-if` 系に相当）で、値で探す `(find item seq)` は無い。`some` は `any`、`reduce` は `foldl`/`foldr` |
| `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` | ❌ | **キーワード引数が関数に無い**（`&optional`/`&key` は `defmacro` のみ実装、`defun`/`lambda` は `&rest` のみ）。この1点で CL のシーケンス API の柔軟性がまるごと落ちる |
| `sort` / `stable-sort` の述語引数 | ❌ | typelisp の `sort` は `Ord A` 境界で**昇順固定**。比較関数を渡せないので降順にもキー指定にもできない（実装は挿入ソートで安定） |
| `merge` | ❌ | |
| `copy-seq` / `fill` / `replace` / `map-into` | ❌ | |
| `concatenate` | ❌ | `append` は 2引数のみ |
| `substitute` / `substitute-if` / `nsubstitute` | ❌ | |
| `remove` / `remove-duplicates` / `delete` / `delete-if` / `delete-duplicates` | ⚠️ | `remove-if` ✅ のみ。値で消す `remove` と重複除去が無い（`delete` 系は (D4)） |
| `notany` / `notevery` / `count-if-not` / `find-if-not` / `remove-if-not` | ❌ | 否定版が一律に無い（`not` を挟めば書けるが CL コードの移植では頻出） |
| `search` / `mismatch` | ❌ | 部分列検索 |
| `make-sequence` / `coerce`（シーケンス変換） | ❌ | `Vector<T>` ↔ `Sexpr` リストの相互変換は**言語仕様上不可**と結論済み（functions.md §10） |
| `nreverse` | ⛔ | (D4) |

### 2.16 ハッシュテーブル（CLHS 18）

| CL | 状態 | 備考 |
|---|---|---|
| `make-hash-table` | ⚠️ | `HashTable::new` ✅。**`:test` を選べない**（キー等価性は組み込み固定）、`:size`/`:rehash-size`/`:rehash-threshold` も無し |
| `gethash` / `(setf gethash)` / `remhash` / `clrhash` / `hash-table-count` | ✅ | `get`（`Option<V>` を返す。CL の第2値の代わり）/ `set` / `remove` / `clear` / `count` |
| `maphash` | ⚠️ | `doiter` と `entries`/`keys`/`values` で代替できるが `maphash` そのものは無い |
| `with-hash-table-iterator` | ❌ | |
| `hash-table-p` / `hash-table-test` / `hash-table-size` / `hash-table-rehash-*` | ❌ | |
| `sxhash` | ❌ | ハッシュ値を取り出せない（ユーザ定義型をキーにする自前の `Hash` トレイトも無い） |

### 2.17 パス名（CLHS 19）・ファイル（CLHS 20）

**全滅（❌）**。パス名型もファイル操作も1つも無い。

`pathname` `make-pathname` `merge-pathnames` `pathname-directory` `pathname-name`
`pathname-type` `namestring` `parse-namestring` `truename` `probe-file` `directory`
`ensure-directories-exist` `delete-file` `rename-file` `file-write-date` `file-author`
`file-namestring` `enough-namestring` `wild-pathname-p` `translate-logical-pathname`

typelisp からファイルを読み書きする手段は**現時点で存在しない**（`load`/`compile-file` が
処理系側でパスを受け取るのみ）。§2.19 のストリームと合わせて、実用プログラムを書くうえでの
最大の穴。

### 2.18 ストリーム（CLHS 21）

**ほぼ全滅（❌）**。第一級ストリームが無いため。

| CL | 状態 | 備考 |
|---|---|---|
| `read-line` | ⚠️ | **標準入力からのみ**。`Option<string>` を返す（EOF は `None`）。ストリーム引数は取れない |
| `read-char` / `peek-char` / `unread-char` / `read-char-no-hang` / `terpri` / `fresh-line` / `write-char` / `write-string` / `write-line` / `read-sequence` / `write-sequence` | ❌ | |
| `open` / `close` / `with-open-file` / `with-open-stream` | ❌ | |
| `make-string-input-stream` / `make-string-output-stream` / `get-output-stream-string` / `with-input-from-string` / `with-output-to-string` | ❌ | 文字列ストリーム。`(format false ...)` が文字列出力の代わりを部分的に果たす |
| `make-broadcast-stream` / `make-concatenated-stream` / `make-echo-stream` / `make-synonym-stream` / `make-two-way-stream` | ❌ | |
| `finish-output` / `force-output` / `clear-output` / `clear-input` / `listen` | ⚠️ | `print`/`println`/`format` は**毎回自動 flush** するので `force-output` 相当は不要 |
| `streamp` / `input-stream-p` / `output-stream-p` / `open-stream-p` / `stream-element-type` | ❌ | |
| `*standard-output*` / `*standard-input*` / `*error-output*` / `*trace-output*` / `*query-io*` / `*terminal-io*` / `*debug-io*` | ⛔ | (D5)。**標準エラー出力へ書く手段が無い** |
| `y-or-n-p` / `yes-or-no-p` | ❌ | |

### 2.19 プリンタ（CLHS 22）

format と pretty printer は実装済み（functions.md §15/§15.1/§15.2）。差分:

| CL | 状態 | 備考 |
|---|---|---|
| `format` | ✅ | ディレクティブはほぼ全対応（`~/name/` のみ未対応）。出力先は `true`/`false` のみで**ストリームを渡せない** |
| `print` / `prin1` / `princ` / `write` / `write-to-string` / `prin1-to-string` / `princ-to-string` / `pprint` | ⚠️ | `print`/`println` は**制御文字列を取る format 系**であり CL の `print`（1引数、`~s` 相当）とは別物。`prin1`/`princ` 単体は無いが `~s`/`~a` で書ける。文字列化は `(format false ...)` |
| pretty printer 一式 | ✅ | `pprint`/`pprint-fill`/`pprint-linear`/`pprint-tabular`/`pprint-logical-block`/`pprint-newline`/`pprint-indent`/`pprint-tab`/`pprint-pop`/`pprint-exit-if-list-exhausted` |
| `print-object` | ✅ | トレイト |
| `*print-pretty*` / `*print-right-margin*` / `*print-miser-width*` | ✅ | 通常のグローバル変数（(D5) のため動的束縛でなく `setf`） |
| `*print-escape*` | ⚠️ | `print-object` の `escape` 引数としてのみ存在。変数としては無い |
| `*print-circle*` / `*print-level*` / `*print-length*` | ✅ | 2026-07-29 実装（functions.md §15.3）。`*print-circle*` は共有・循環構造を `#n=`/`#n#` でラベル付けし、後の2つは `#`/`...` で打ち切る。CL の `nil`（無制限）は 0 以下で表す |
| `*print-base*` / `*print-radix*` / `*print-case*` / `*print-lines*` / `*print-gensym*` / `*print-array*` / `*print-readably*` | ❌ | 基数・大文字小文字・行数などの制御 |
| `set-pprint-dispatch` / `*print-pprint-dispatch*` / `copy-pprint-dispatch` | ⛔ | 採用しないと確定済み（language-design.md §9、`print-object` トレイトで置き換え） |
| `write-byte` / `read-byte` | ❌ | バイナリ I/O |

### 2.20 リーダ（CLHS 23）

| CL | 状態 | 備考 |
|---|---|---|
| `read` | ⚠️ | **文字列から1つ読む**（`(read s)` → `Result<Sexpr,ReadError>`）。ストリームからは読めない |
| `read-from-string` | ⚠️ | 実質これが `read`。ただし読んだ位置（第2値）が返らない |
| `read-preserving-whitespace` / `read-delimited-list` | ❌ | |
| `readtable` 関連（`copy-readtable` / `set-macro-character` / `get-macro-character` / `set-dispatch-macro-character` / `make-dispatch-macro-character` / `readtable-case` / `*readtable*`） | ❌ | **リーダマクロが定義できない**。`#.`/`#+`/`#-` 等の読み込み時制御も無い |
| `*read-base*` / `*read-default-float-format*` / `*read-suppress*` / `*read-eval*` | ⛔ | (D5) |
| `with-standard-io-syntax` | ⛔ | (D5) |
| `parse-integer` | ✅ | `parse-int` |

### 2.21 システム構築（CLHS 24）・環境（CLHS 25）

| CL | 状態 | 備考 |
|---|---|---|
| `load` | ⚠️ | fasl（チェック済みモジュール）優先ロード。CL の「ソースを読んで順に評価」とは意味が違う |
| `require` / `provide` / `*modules*` | ⚠️ | `module`/`use`＋ファイル↔モジュール対応が相当 |
| `*features*` / `#+` / `#-` | ❌ | **条件付きコンパイルが無い** |
| `compile-file-pathname` / `*compile-file-pathname*` / `*load-pathname*` 等 | ❌ | |
| `time` / `get-internal-real-time` / `get-internal-run-time` / `internal-time-units-per-second` | ❌ | **時間の計測手段が無い**（ベンチマークが書けない） |
| `get-universal-time` / `get-decoded-time` / `encode-universal-time` / `decode-universal-time` | ❌ | **日時が扱えない** |
| `sleep` | ❌ | |
| `room` / `ed` / `dribble` / `apropos` / `apropos-list` / `inspect` / `describe` | ❌ | 対話環境向け。REPL があるので `apropos`/`describe` は相性が良い |
| `documentation` / `(setf documentation)` / docstring | ❌ | **docstring の仕組みが無い**（LSP の hover と相性が良いので効果は大きい） |
| `lisp-implementation-type` / `lisp-implementation-version` / `machine-type` / `machine-version` / `machine-instance` / `software-type` / `software-version` / `short-site-name` / `long-site-name` | ❌ | |
| `user-homedir-pathname` | ❌ | パス名型が無い |
| `trace` / `untrace` / `step` / `disassemble` | ❌ | |
| コマンドライン引数の取得 | ❌ | CL 標準にも無いが、`typl file.typl` でスクリプトを書く以上ほぼ必須 |

---

## 3. 横断的な欠落（個別の関数より効いてくるもの）

上の表は関数単位だが、実際には**1つの機構が無いために関数が束で落ちている**箇所がある。
足すなら効果が大きい順に:

1. **ストリームとファイル I/O**（§2.17/§2.18）— CLHS 3章ぶんが丸ごと落ちている。現状 typelisp が
   触れる外界は「標準入力から1行」と「標準出力へ書く」だけで、ファイルを読むプログラムが書けない。
2. **関数の `&optional` / `&key`** — `defmacro` には実装済み（2026-07-24）だが `defun`/`lambda` は
   `&rest` のみ。このため CL のシーケンス API の `:key`/`:test`/`:start`/`:end`、
   `make-hash-table :test`、BOA コンストラクタなどが**構造的に書けない**。
3. **汎用 place（`setf` 展開子）** — 現在の place は変数と `変数::field` の2つだけ。
   `incf`/`decf`/`push`/`pop`/`rotatef`/`(setf (gethash ...))` 等はこれが要る。
4. **多値** — `floor` の商と剰余、`gethash` の存在フラグ、`read-from-string` の読み終わり位置など、
   CL の API 設計は多値を前提にしている箇所が多い。typelisp は `Option`/`cons-cell` で個別に
   回避しているが、CL コードの移植では毎回書き換えが要る（`gethash` 相当は `get`→`Option<V>` で
   解決済み、`floor` 相当は2026-07-29に `floor-div` 等→`cons-cell` で解決済み。§2.10 参照）。
5. **数値ライブラリの基礎**（§2.10）— `max`/`min`/`zerop`/`evenp`/超越関数/ビット演算が無い。
   一つ一つは小さいが、数を数える程度のコードでも欠落に当たる。単純に prelude へ足せるものが多い。
6. **述語や比較関数を引数に取れないコレクション API** — `sort` に比較関数を渡せず、
   `find`/`position`/`count` は述語版しかなく値版が無い。2 と合わせて解消すべき。
7. **時間・乱数の再現性**（§2.21/§2.10）— `time` も `get-universal-time` も `random-state` も無い。
8. ~~**`*print-circle*` / `*print-level*` / `*print-length*`**~~ — **2026-07-29 実装済み**
   （functions.md §15.3、[implementation-log.md](implementation-log.md) の該当節）。着手前は
   「循環構造を印字するとプロセスが落ちる」状態だった——`defstruct` のフィールドを `setf` で
   自分自身へ向けた値を `println` するとスタックオーバーフローで abort することを確認しており、
   `*print-circle*` を真にすればラベル付き（`#1=…#1#`）で印字できるようになった。

## 4. このドキュメントの位置づけ

ここに並べた ❌ は**すべてが TODO ではない**。[TODO.md](TODO.md) の「残っている作業」は現時点で
空であり、この一覧は「CL と比べたときの残差はどこか」を測るための地図として作った。着手する
場合は §3 の順序が費用対効果の目安になる（優先度は筆者の見立てで、確定した方針ではない）。

⛔ の項目については、[language-design.md](language-design.md) §7・§8・§9（採用しないと決めた
機能）が一次情報。CL に同名の機能があることを理由にこれらを再検討する場合は、
まず (D1)〜(D5) のどれと衝突するかを確認すること。
