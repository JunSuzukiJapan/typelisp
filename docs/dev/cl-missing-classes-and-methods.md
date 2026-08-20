# Common Lisp との差分 — 未実装のクラス（型）とメソッド（関数）の全リスト

作成: 2026-07-29 / 最終更新: 2026-08-18（`catch`/`throw`/`unwind-protect`、`defun` の
`&optional`/`&key`、`sort` の比較関数と項目ベースの `find`/`position`/`count`、`time`/`random-state`
の実装を反映。§0 の (D3)、§1.5、§2.3、§2.7、§2.10、§2.15、§2.21、§3 を更新）

このドキュメントは **ANSI Common Lisp（CLHS）に存在して typelisp に無いもの** を、クラス（型）と
メソッド（関数・マクロ・特殊形）に分けて網羅列挙する。「CL 同等の表現力のために何を足すか」を
提案した [cl-equivalence-catalog.md](cl-equivalence-catalog.md)（2026-06-18、提案項目はほぼ実装済み）
とは目的が異なり、**こちらは残差の棚卸し**である。

**この残差を埋める実行計画は [cl-parity-plan.md](cl-parity-plan.md)（2026-08-20 策定）にある。**
下の表の ❌/⚠️/⛔ がそれぞれどの Phase に落ちたか（落ちていないなら理由）は同計画の付録 A。

実装状況は docs（古い可能性がある）ではなく、以下を直接読んで確認した:

- `crates/typelisp-front/src/check/registry.rs` — Rust 組み込み型・組み込みメソッドの登録表
- `crates/typelisp-front/src/check/checker.rs` — 特殊形の一覧（`check` の文字列 match）
- `crates/typelisp-front/src/prelude.rs` — typelisp で書かれた標準ライブラリ（`defun`/`defmethod`/`defmacro`/`impl`）
- `crates/typelisp-front/src/eval/interp.rs` — 組み込みメソッドの実体

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
  （language-design.md §7.1）。2026-08-16 に非採用が確定（同 §9）。ただし**非採用なのは
  コンディション**（`define-condition`/`handler-bind`/`handler-case`/`invoke-restart`）であって
  非局所脱出そのものではない——CL でもこの2つは別の機構で、静的な脱出 `break`/`return`
  （直近ループのみ）に加え、動的な脱出 `catch`/`throw`/`unwind-protect` を同日に実装した
  （language-design.md §7.5、[[typelisp-catch-throw-design]]）。落ちているのは
  「ハンドラを積んでスタックを巻き戻さずに走らせる」層と restart だけ。
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
| `pathname` | ✅ | 2026-08-05。`defstruct pathname`＋パス名指定子トレイト `Pathish`（§2.17） |
| `logical-pathname` | ⛔ | 論理パス名は採用しない |
| `stream` および全サブクラス（`file-stream`/`string-stream`/`broadcast-stream`/`concatenated-stream`/`echo-stream`/`two-way-stream`/`string-input-stream`/`string-output-stream`) | ✅ | 2026-08-02。ただしクラス階層ではなく**トレイト階層**（§2.18）。`synonym-stream` のみ無し |
| `readtable` | ❌ | リーダマクロを登録する表が無い（リーダの構文は固定） |
| `random-state` | ✅ | 2026-07-31。`random-state` 型（ネイティブの xorshift 状態）＋ `make-random-state` / `random-state-p` / `*random-state*`。`(random n &optional state)` で状態を明示できる |
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
| `compile` / `compile-file` | ⚠️ | 実体は LLVM JIT / AOT ネイティブ実行ファイル生成。CL の「fasl を作る」意味とは違い、中間ファイルは残さない。コンパイル済みモジュール形式は無い |
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
| `setf` | ⚠️ | 実装済み。place は「変数」「変数::フィールド」に加え、**`(accessor recv key...)` 形の呼び出し形 place** も 2026-07-30 対応（`Checker::check_setf_call_place`）。CL の `defsetf`/`define-setf-expander`（実行時のグローバルな名前→名前の登録テーブル）に相当する仕組みは無いが不要——`recv` の静的な型はチェック時にすでに分かっているので、`変数::field`＝`field`/`set-field` と同じ規約をそのまま流用し、`recv` の型が `set-{accessor}` という名のインスタンスメソッドを持っていればそれを setter として使う。ユーザ定義型は `defmethod set-foo ...` を書くだけで任意のアクセサ名 `foo` を setf 可能にでき、しかも型ごとに独立（CL のグローバル1本の名前テーブルと違い、別の型が同じアクセサ名を別の setter に割り当てても衝突しない）。`Vector<T>`/`HashTable<K,V>` の `get`→`set`（`set-get` ではない）は既存 API 互換のための特例。CL の `(setf (gethash k h) v)`/`(setf (aref a i) v)` に相当するものはこれで書ける（例: `(setf (get h k) v)`）。`(setf (car x) v)` 相当は無い（`Sexpr` の cons セルは `p::car` フィールド place で書く） |
| `psetf` / `psetq` / `setq` | ✅ | 3つとも `defmacro`（`setf` へ展開）（2026-08-20、Phase 4a）。`psetf`/`psetq` は全ての値を先に評価してから代入するので `(psetq a b b a)` が交換になる |
| `shiftf` / `rotatef` | ✅ | 2026-07-30 実装（`Checker::check_rotatef_shiftf`）。上記どの place 種でも使えるが、読み取り型と書き込み型が非対称な place（`HashTable<K,V>` の `get`→`Option<V>`／`set`→`V`）をまたぐ回転は型エラーになる（CL の untyped `gethash` と違い静的型があるため） |
| `incf` / `decf` | ✅ | 2026-07-30 実装（`Checker::check_incf_decf`）。`delta` 省略時は `1` |
| `push` / `pop` | ⚠️ | `Vector<T>` のメソッドとして存在（`(push vec item)`、受け手が先）。2026-07-30、`(push item vec)` という CL の引数順も同名のまま両立するようにした（`Checker::try_instance_method_swapped` — 通常の受け手優先解決が失敗した場合だけ引数を入れ替えて再試行する2引数汎用フォールバック）。`Vector<T>` は参照型（ヒープ上で直接変異）なので CL のような setf 展開は不要。`pushnew` は 2026-08-20（Phase 4a）に `defmethod` として追加——CL がマクロなのは place を書き換えるためで、ここは受け手がその場で変異するので不要。`remf` はプロパティリストごと対象外（§2.12） |
| `block` / `return-from` | ⚠️ | `return` はあるが **直近のループからしか脱出できない**。名前付きブロックも関数からの早期リターンも無い。字句的な入れ子を跨ぐ脱出が要る場合は `catch`/`throw`（動的）で代用する |
| `tagbody` / `go` | ⛔ | goto |
| `catch` / `throw` | ✅ | 2026-08-16 実装（syntax.md §8 / language-design.md §7.5）。**動的**な脱出で、関数を何段跨いでも同じタグの `catch` に届く。CL との差は**タグがリテラルシンボル限定**（評価されない）で、そのシンボルが飛ぶ値の型を運ぶこと（`Checker::throw_tags`。計算したタグでは突き合わせる型が無くなる）。`throw` の型は `!`、`(catch 'tag e)` の型は `e` の型とタグの型の合流 |
| `unwind-protect` | ✅ | 2026-08-16 実装。`cleanup` は `protected` をどう抜けても走る——正常終了・`throw`・`panic` に加えて `break`/`return` でも。interpreted / compiled 両経路（compiled 側は「上げうる呼び出しを保護経由にする」方式、[[typelisp-compiled-catch-throw]]） |
| `destructuring-bind` | ❌ | `defmacro` のラムダリストでは分配束縛ができる（`&optional`/`&key` 含む）が、式としての `destructuring-bind` は無い |
| `prog1` / `prog2` | ✅ | `defmacro`（2026-08-20、Phase 4a） |
| `prog` / `prog*` | ❌ | `block nil` + `tagbody` の糖衣なので、`block`（Phase 4a の残り）と `tagbody`（⛔ goto）に依存する |
| `typecase` / `etypecase` / `ctypecase` | ⛔ | (D1)。`match` が相当 |
| `ecase` / `ccase` | ✅ | `defmacro`（2026-08-20、Phase 4a）。どれにも当たらなければ panic。`ccase` は差し出せる restart が無いので `ecase` と同一の展開 |
| `sleep` | ❌ | |

### 2.4 反復（CLHS 6）

| CL | 状態 | 備考 |
|---|---|---|
| 拡張 `loop`（`for`/`in`/`across`/`collect`/`sum`/`when`/`finally` …） | ❌ | typelisp の `loop` は**無限ループのみ**で、CL の LOOP DSL とは名前が同じだけの別物。`collect`/`sum` 等の集約は `map`/`foldl` を使う |
| `do` / `do*` | ✅ | 両方 `defmacro`。`do` は並行ステップ、`do*` は `let*` 束縛と順次代入（2026-08-20、Phase 4a） |
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
| `:include`（構造体の継承） | ❌ | 型の継承は無い（トレイトの継承・デフォルト実装で共通の振る舞いは括れる） |
| `:constructor`（BOA コンストラクタ・複数コンストラクタ） | ❌ | 自動生成の `new`（全フィールドを位置引数で受ける）のみ。キーワード引数コンストラクタが無い |
| `:conc-name` / `:predicate` / `:copier` | ❌ | アクセサ名の変更、`point-p` 述語、`copy-point` が生成されない |
| `:print-function` / `:print-object` | ✅ | `impl print-object` が相当 |
| `:type` / `:initial-offset` / `:named` | ⛔ | 表現を list/vector に変える指定は (D1) と衝突 |
| スロットの初期値 | ❌ | フィールドのデフォルト値が書けない |

### 2.7 コンディション（CLHS 9）

全体が ⛔ (D3)。`Result<T,E>` + `match` + `panic` で書き換える方針で、2026-08-16 に非採用が確定した
（language-design.md §9）。ただし**非局所脱出は別機構として実装済み**（`catch`/`throw`/
`unwind-protect`、§2.3）——CL でもこの2つは別の機構なので、ここで落ちているのは
「ハンドラを積んで、スタックを巻き戻さずにハンドラを走らせる」層と restart だけ。

| CL | 状態 |
|---|---|
| `define-condition` / `make-condition` | ⛔（`defstruct`/`defenum` + `impl Error` が代替） |
| `signal` / `error` / `cerror` / `warn` / `break` | ⛔（`panic` のみ。**警告を出して続行する仕組みが無い**） |
| `handler-case` / `handler-bind` / `ignore-errors` | ⛔（`match` で `Result` を分岐）。`panic` は 2026-08-16 以降 abort でなく unwind するので `unwind-protect` の cleanup は走るが、**捕まえて継続する手段は無い**（`catch` が受けるのは `throw` だけ） |
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

**述語** — 2026-07-31 実装:

| CL | 状態 | 備考 |
|---|---|---|
| `zerop` `plusp` `minusp` | ✅ | `i32`/`i64`/`f64`/`bignum`/`ratio` 全型の `defmethod`（`prelude.rs`） |
| `evenp` `oddp` | ✅ | `i32`/`i64`/`bignum`（整数型のみ、CL 仕様通り） |
| `numberp` `integerp` `rationalp` `floatp` `realp` `complexp` | ⛔ | (D1) 静的型付けのため実行時型問い合わせが原理的に載らない |

**基本演算**:

| CL | 状態 | 備考 |
|---|---|---|
| `+` `-` `*` `/` `=` `/=` `<` `<=` `>` `>=` | ✅ | **2026-07-31 可変長化+0/1引数対応**。`Checker::check_variadic_arith`/`check_variadic_cmp`（`checker.rs`）が `(+ a b c)` を `(+ (+ a b) c)` に、`(< a b c)` を一時変数束縛＋`(and (< a b) (< b c))` に構文糖衣展開。`Checker::check_nullary_or_unary_numeric_op` が CL の0/1引数版も実装: `(+)=0`、`(*)=1`、`(- x)`/`(/ x)`（単項否定・逆数、`(let ((%t x)) (- (- %t %t) %t))` 型のトリックでリテラル型変換問題を回避）、`(< x)=true` 等 |
| `max` / `min` | ✅ | 型ごとの2引数ビルトイン(`icmp`+`select`、`bignum`/`ratio`は`rt_*_cmp`+`select`) + 可変長糖衣展開で3引数以上にも対応 |
| `1+` / `1-` | ✅ | 型ごとの `defmethod`（`prelude.rs`） |
| `abs` `signum` `gcd` `lcm` `mod` `rem` `expt` | ✅ | 型ごとのメソッド。`gcd`/`lcm` は 0/1/n 引数すべて（チェッカー糖衣、`(gcd)`=0・`(lcm)`=1・1引数は `abs`）、整数の `expt` も `i32`/`i64` に（2026-08-20、Phase 1c） |
| `floor` `ceiling` `round` `truncate` | ⚠️ | 1引数版（`f64→f64`）はCL相当。~~除数を取る2引数版も商・剰余の多値も無い~~ → **2026-07-29 `floor-div`/`ceiling-div`/`round-div`/`truncate-div` として実装済み**（`i32`/`i64`/`f64`、商・剰余を`cons-cell`で返す。多値そのものは非採用、§3.4参照）。CL と同名の2引数オーバーロードにしなかったのは `defmethod` が受け手の型でのみ解決しアリティでは解決しないため |
| `ffloor` `fceiling` `fround` `ftruncate` | ✅ | 既存の `f64` `floor` 等の別名（CL では無印が整数を返すので `f` 付きの方が一致する）（2026-08-20、Phase 1c）。**丸め方だけ CL と違う**——0 から遠い方へ丸める |
| `sqrt` | ✅ | `f64` の `sqrt` と、整数の `isqrt`（`i32`/`i64`）（2026-08-20、Phase 1c） |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | ✅ | **2026-07-31実装**（`f64`、`registry.rs`/`interp.rs`）。`log` は自然対数のみ（1引数）に加え、`(log number base)` の2引数版は `Checker::check_log_with_base` が `(/ (log number) (log base))` へアリティ展開して対応 |
| `pi` | ✅ | **2026-07-31実装**。`f64` 定数（`prelude.rs` の `defconstant`） |
| `float` `rational` `rationalize` | ✅ | `int->float`/`float->ratio`（CL の `rational`）に加え `rationalize`（読み戻せる最も簡単な有理数。`(rationalize 0.1)`=`1/10`）（2026-08-20、Phase 1c） |
| `numerator` / `denominator` | ✅ | |
| `complex` `realpart` `imagpart` `conjugate` `phase` `cis` | ❌ | 複素数が無いため |
| `float-sign` `float-digits` `float-precision` `decode-float` `integer-decode-float` `scale-float` `float-radix` | ✅ | 7つとも（2026-08-20、Phase 1c）。`decode-float` は CL の3値返しのうち仮数と指数を `cons-cell` で返し、符号は `float-sign` が担う |
| `random` | ⚠️ | **2026-07-31 に `random-state` 一式を追加**。`(random n &optional state)`（`i32` のみ）／`make-random-state`（引数なし＝新しい状態、状態を渡す＝その複製）／`random-state-p`／`*random-state*`（(D5) のため動的束縛でなく代入可能なグローバル）。状態は xorshift64、interpreted と compiled で同じ列を返す。残る差は**シード値を外から与えられない**こと——`make-random-state-fresh` は壁時計から採るので、同一プロセス内で `make-random-state` による複製を使えば列を再生できるが、実行を跨いで再現はできない（CL の `(make-random-state nil)`/`t` の区別も無い） |

**ビット演算** — 2026-07-31実装:

| CL | 状態 | 備考 |
|---|---|---|
| `logand` `logior` `logxor` `lognot` `ash` `logbitp` `logcount` `logtest` `integer-length` | ✅ | `i32`/`i64`（`registry.rs`/`interp.rs`）+ `bignum`（`num-bigint`のネイティブビット演算+独自popcount/bit-length実装）。無限精度2の補数として実装。`ratio` には未対応（CL自体もビット演算は整数専用でratioには定義が無い） |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | ✅ | `i32`/`i64`/`bignum` の `defmethod`（`prelude.rs`、上記プリミティブから合成） |
| `byte` `byte-size` `byte-position` `ldb` `ldb-test` `dpb` `mask-field` `deposit-field` | ⚠️ | `i32` のみ（`prelude.rs`）。バイト指定子は新規struct型を作らず既存の`cons-cell<i32,i32>`を流用。**`i64`/`bignum` への拡張は保留**——`defmethod` は受け手でしか解決せず、CL の `(ldb bytespec integer)` は指定子が先なので、整数側の幅で実装を選べない（引数順を変えるか指定子に幅を持たせるかの設計判断が要る） |
| `boole` | ✅ | `i32` のみ。16個の `boole-*` 定数(`i32`コード、CLのキーワードの代わり)+ `defmethod`（`prelude.rs`） |

**定数** — ✅（2026-08-20、Phase 1c）:

`most-positive-fixnum` `most-negative-fixnum` `most-positive-double-float`
`most-negative-double-float` `least-positive-double-float` `least-negative-double-float`
`least-positive-normalized-double-float` `least-negative-normalized-double-float`
`double-float-epsilon` `double-float-negative-epsilon`（`pi` は以前から）。
`single-float`/`long-float` 系の同名定数は `f32`/`long-float` を持たないので無い。

**コンパイル(JIT/AOT)対応**: 2026-07-31、上記の新規実装すべてに `compile`/`compile-file` 対応を追加。
`i32`/`i64` のビット演算・`max`/`min` はLLVM命令直結(`build-and`/`build-or`/`build-xor`/`build-select`)
または `rt_i64_*` シム(`ash`/`logbitp`/`logcount`/`integer-length`、可変シフト量のUB回避のため)。`f64` の
`sin`/`cos`/`exp`/`log`/`max`/`min` はLLVM intrinsic(`llvm.sin.f64`等、`build-fsin`等)、`tan`/`asin`/
`acos`/`atan`/`sinh`/`cosh`/`tanh`/`asinh`/`acosh`/`atanh` はこのプロジェクトが固定するLLVMバージョンに
intrinsicが無いため `rt_f64_*` シム。`bignum`/`ratio` の `max`/`min` は既存の `rt_*_cmp` 三値比較 +
`build-select`。可変長四則演算/比較・`log`の2引数版・0/1引数算術はチェッカー側の構文糖衣展開で常に2引数の
`Expr::Assoc` に潰されるため、`compiler.rs`/`interp.rs`のいずれも無改修で動作する。`logeqv`系・
`byte`/`ldb`/`dpb`/`boole`・述語(`zerop`等)は上記プリミティブから合成された通常の`defmethod`/`defun`な
ので自動的にコンパイル可能（追加のcompiler.rs対応は不要）。`tests/compile_test.rs`に合意テスト
(`compile_dispatches_f64_transcendental_functions_and_agrees_with_the_interpreter`等)を追加済み。

### 2.11 文字（CLHS 13）

| CL | 状態 | 備考 |
|---|---|---|
| `char=` `char<` `char<=` `char>` `char>=` | ⚠️ | 順序比較は `<`/`<=`/`>`/`>=` が `char` に多重定義されて ✅。等価は `equal`（`eq`/`eql` も同義）で、**数値と違い `=`/`/=` は `char` に定義されていない** |
| `char/=` | ✅ | `/=`（2026-08-20、Phase 2a）。**可変長形は隣接ペア比較**で、全ペア相異を問う CL とは異なる |
| `char-equal` / `char-lessp` 等（大文字小文字無視版） | ✅ | `equalp` に加え `lessp`/`greaterp`/`not-lessp`/`not-greaterp`（2026-08-20、Phase 2a） |
| `char-code` / `code-char` | ✅ | `char->int` / `int->char`（+ `try-int->char`） |
| `char-upcase` / `char-downcase` | ✅ | `upcase` / `downcase`（ASCII のみ） |
| `alpha-char-p` / `digit-char-p` | ✅ | `alphap` ✅ / CL 本来の重みは `digit-weight`（基数引数つき、`Option<i32>`）として別名で追加。`digitp` は bool のまま据え置き（prelude 自身のリーダが述語として呼ぶため）——2026-08-20、Phase 2a |
| `alphanumericp` `graphic-char-p` `standard-char-p` `upper-case-p` `lower-case-p` `both-case-p` | ✅ | `alphanumericp`/`graphicp`/`standardp`/`upper-casep`/`lower-casep`/`both-casep`（2026-08-20、Phase 2a） |
| `characterp` | ⛔ | 静的型付け（D1） |
| `char-name` / `name-char` / `char-int` / `digit-char` | ✅ | `char->name`/`name->char`（リーダの文字名表の 7 つ）／`char-int` は既存の `char->int` と同じ／`digit->char`（基数引数つき）——2026-08-20、Phase 2a |
| `char-code-limit` | ❌ | |

### 2.12 コンス（CLHS 14）

`cons`/`car`/`cdr` は `cons-cell<A,B>` として ✅（`Sexpr` 側は `sexpr-car`/`sexpr-cdr`）。以下は無い。

| CL | 状態 | 備考 |
|---|---|---|
| `caar` … `cddddr`（28個） | ✅ | 28 個すべて。ジェネリック `defun`（`defmethod` の受け手はネスト位置の型変数を束縛できない）。**リストでなくネストしたペア**の走査（2026-08-20、Phase 3） |
| `first` … `tenth` / `rest` | ✅ | `Iter` 上（2026-08-20、Phase 3）。`first`〜`tenth` は `Option<A>`、`rest` は新しい `Vector<A>` |
| `list` / `list*` | ⚠️ | `list` ✅（特殊形、`Sexpr` を作る）。`list*` は**対象外**——「末尾を差し替えた不完全リスト」という概念が無い |
| `make-list` / `copy-list` | ✅ | `Vector::filled` / `copy-seq`（2026-08-20、Phase 3） |
| `copy-tree` / `copy-alist` | ⛔ | 任意深さの異種の木を走査する型が書けない（`Sexpr` の木としてなら `equal` が `tree-equal` に当たる） |
| `nth` / `nthcdr` | ⚠️ | `nth` は `Iter` 用で ✅、**`Sexpr` リストには使えない**。`nthcdr` は削除済み |
| `last` / `butlast` | ⚠️ | 両方 `Iter` 用で ✅。`last` は CL と違い**最後のセルでなく最後の要素**を返す。`nbutlast` は ✅（2026-08-20、Phase 3） |
| `list-length` / `endp` / `null` / `consp` / `atom` / `listp` | ⚠️ | `Sexpr` 版の `sexpr-null`/`sexpr-consp`/`sexpr-atom` は ✅。汎用の `null`/`consp`/`atom` は削除済み |
| `rplaca` / `rplacd` | ✅ | `cons-cell` 上（2026-08-20、Phase 3）。(D4) は撤回（cl-parity-plan.md 付録 B）。**`Sexpr` 版は無い**——cons セルが `car` のソース位置を持つので書き換えると診断がずれる |
| `nconc` / `nreverse` / `nbutlast` / `nsubst` 等の n 系 | ✅ | `Vector<T>` 上（2026-08-20、Phase 3）。(D4) は撤回。`nconc` は CL と違い**共有構造の書き換えではない** |
| `revappend` / `nreconc` | ✅ | （2026-08-20、Phase 3） |
| `append` | ⚠️ | `Iter` 版（2引数）と `string` 版と `sexpr-append` がある。CL の可変長・任意個は無い |
| `member` / `member-if` / `member-if-not` | ⚠️ | 3つとも ✅ だが**すべて `bool` を返す**（CL は残りのリスト）。イテレータに返すべき tail cons が無いため——残りが要るなら `position` + `subseq`（2026-08-20、Phase 3） |
| `assoc` / `assoc-if` / `rassoc` / `rassoc-if` / `acons` / `pairlis` | ✅ | 6つとも（2026-08-20、Phase 3）（`:test`/`:key` は Phase 3e） |
| `sublis` / `subst` / `subst-if` / `tree-equal` | ⛔ | `copy-tree` と同じ理由で対象外 |
| `union` / `intersection` / `set-difference` / `set-exclusive-or` / `subsetp` / `adjoin` | ✅ | 6つとも `Iter` 上・`Eq` 境界（2026-08-20、Phase 3）。CL が規定しない結果の順序は**初出順**で安定させた |
| `ldiff` / `tailp` | ✅ | （2026-08-20、Phase 3）。CL は**構造の共有**を問うが、共有すべき構造が無いので**値として**の接尾辞を問う |
| `getf` / `get-properties` | ⛔ | キーと値が交互に並ぶ無型のリストという表現が無い。同じ役割は `assoc`（連想リスト）か `HashTable` |

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
| `string/=` | ✅ | `/=`（bool）と、不一致位置を返す `mismatch` の両方（2026-08-20、Phase 2b） |
| `string-equal` / `string-lessp` 等（大文字小文字無視版） | ✅ | `equalp` に加え `lessp`/`greaterp`/`not-lessp`/`not-greaterp`（2026-08-20、Phase 2b） |
| `char` / `schar` | ✅ | `ref`（範囲外は panic） |
| `subseq`（文字列に対して） | ✅ | `substring` |
| `string-upcase` / `string-downcase` | ✅ | `upcase` / `downcase`（ASCII のみ、`:start`/`:end` 無し） |
| `string-capitalize` | ✅ | `capitalize`（2026-08-20、Phase 2b） |
| `nstring-*` | ❌ | 破壊版。`string` が不変なので Phase 3d の可変文字列判断待ち |
| `string-trim` / `string-left-trim` / `string-right-trim` | ✅ | `trim`/`left-trim`/`right-trim`。`bag` 省略時は空白類（2026-08-20、Phase 2b） |
| `concatenate` | ⚠️ | `append`（2引数）のみ |
| `make-string` / `string`（文字列化） | ✅ | `string::filled` と `to-string`（2026-08-20、Phase 2b）。`to-string` はスカラ 6 型に実装 |
| `stringp` / `simple-string-p` | ⛔ | 静的型付け（D1） |
| `search` / `mismatch`（文字列検索） | ✅ | 受け手優先の `(search s sub)`（CL は引数順が逆）と `(mismatch a b)`（2026-08-20、Phase 2b） |
| `split-sequence` 相当 | ✅ | `(split s sep)`、`sep` は文字列（2026-08-20、Phase 2b） |
| `parse-integer` | ✅ | `parse-int`（`Result` を返す）。`:radix`/`:junk-allowed` は無い |

### 2.15 シーケンス（CLHS 17）

`Iter` トレイト上のジェネリック関数として一通り揃っている（functions.md §6）。差分だけ:

| CL | 状態 | 備考 |
|---|---|---|
| `length` `elt` `subseq` `reverse` `sort` `find` `position` `count` `remove-if` `every` `some` `reduce` `map` | ✅ | 2026-07-31 に**項目ベース版 `find`/`position`/`count`**（`(find x it)`、`Eq A` 境界。CL のデフォルト `:test` = `eql` に相当）を追加し、述語版は `find-if`/`position-if`/`count-if` の名で並立。`some` は `any`、`reduce` は `foldl`/`foldr` |
| `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` | ❌ | キーワード引数**機構**は 2026-07-29 に入った（`defun` が `&optional`/`&key` を取れる。`defmacro` は 2026-07-24 から。`lambda` は `&rest` のみ、`defmethod` は `&rest` すら取れない）が、**シーケンス API 側がまだ受けていない**。等価性は `Eq` トレイト固定なので `:test`/`:key` はトレイト境界とも噛み合わせが要る |
| `sort` / `stable-sort` の述語引数 | ✅ | 2026-07-31 に CL 本来の `(sort sequence predicate)` へ変更。`(sort it cmp)`、`cmp` は「第1引数が第2引数より真に前」で `true`。非破壊（新しい `Vector<A>` を返す）かつ安定な挿入ソートなので `stable-sort` は同じものになる |
| `merge` | ✅ | （2026-08-20、Phase 3）。CL は整列済みを要求するが、これは連結を整列する |
| `copy-seq` / `fill` / `replace` / `map-into` | ✅ | `copy-seq` は `Iter` 上、残り3つは `Vector<T>` のその場書き込み（2026-08-20、Phase 3） |
| `concatenate` | ⚠️ | `append`（2引数）が相当。可変長版は無い |
| `substitute` / `substitute-if` / `nsubstitute` | ✅ | `nsubstitute-if` も（2026-08-20、Phase 3） |
| `remove` / `remove-duplicates` / `delete` / `delete-if` / `delete-duplicates` | ✅ | 全部（2026-08-20、Phase 3）。`delete-if-not` も。(D4) は撤回 |
| `notany` / `notevery` / `count-if-not` / `find-if-not` / `remove-if-not` | ✅ | 5つとも（2026-08-20、Phase 3） |
| `search` / `mismatch` | ⚠️ | `string` 上は ✅（Phase 2b）。任意のシーケンス上の部分列検索は無い |
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

2026-08-02（ストリーム/ファイル）と 2026-08-05（パス名）で実装済み。以下は残差
（functions.md §18/§19）。

| CL | 状態 | 備考 |
|---|---|---|
| `pathname` / `make-pathname` / `merge-pathnames` / `namestring` / `parse-namestring` / `pathname-directory` / `pathname-name` / `pathname-type` / `file-namestring` / `enough-namestring` | ✅ | prelude で実装。CL の `pathname` 関数は型名と衝突するため `to-pathname` |
| パス名指定子（文字列 or パス名） | ✅ | `Pathish` トレイト。ファイルを名指しする関数は全てこれをジェネリックに取る |
| `directory-namestring` | ✅ | |
| `probe-file` / `delete-file` / `rename-file` | ✅ | |
| `truename` / `file-write-date` / `directory` / `ensure-directories-exist` | ✅ | 2026-08-20 実装（Phase 9c）。`file-*` プリミティブの上に `Pathish` の薄い層。`directory` はワイルドカード照合ではなく「そのディレクトリを並べる」（この言語のパス名にワイルドカードが無いため）。おまけで `directory-p` も |
| `file-author` | ⛔ | uid→ユーザ名の引き当てに `libc` が要る。ワークスペースは `libc` に依存していない |
| `wild-pathname-p` / `translate-logical-pathname` / `logical-pathname` | ⛔ | ワイルドカードも論理パス名も採用しない（この処理系が走らないファイルシステム向けの機能） |
| ホスト・デバイス・バージョン成分 | ⛔ | 同上。区切りは `/` 固定 |

### 2.18 ストリーム（CLHS 21）

2026-08-02 にトレイト階層として実装済み（functions.md §18）。クラス階層ではなくトレイト階層
なので、CL のクラス判定関数群は「型が答える問い」に置き換わっている。

| CL | 状態 | 備考 |
|---|---|---|
| `read-line` / `read-char` / `peek-char` / `unread-char` / `terpri` / `fresh-line` / `write-char` / `write-string` / `write-line` | ✅ | `CharInput`/`PeekInput`/`CharOutput` のメソッド。標準入出力も `*standard-input*` 等のストリーム値として同じメソッドで扱う |
| `open` / `close` / `with-open-file` / `with-open-stream` | ✅ | `open-file`（`Result` を返す）/ `close` / `with-open-file` |
| `make-string-input-stream` / `make-string-output-stream` / `get-output-stream-string` / `with-input-from-string` / `with-output-to-string` | ✅ | |
| `make-broadcast-stream` / `make-concatenated-stream` / `make-echo-stream` / `make-two-way-stream` | ✅ | いずれも合成ストリーム＝ただの `defstruct`（ネイティブ層の支援なし） |
| `make-synonym-stream` | ❌ | シンボルを介した間接参照が要る（動的束縛が無いので意味が薄い） |
| `finish-output` / `force-output` | ✅ | `finish-output`（`print`/`println`/`format` は毎回自動 flush する） |
| `clear-output` / `clear-input` / `listen` / `read-char-no-hang` | ❌ | ネイティブ層に `listen` はあるが typelisp へは未公開 |
| `read-sequence` / `write-sequence` | ❌ | 一括転送は `copy-stream`/`read-all`/`write-lines` で代替 |
| `streamp` / `input-stream-p` / `output-stream-p` / `stream-element-type` | ⛔ | 方向も要素型も型が持つ（実行時に尋ねる問いではない） |
| `open-stream-p` | ✅ | `Stream` トレイトのメソッド |
| `*standard-output*` / `*standard-input*` / `*error-output*` | ✅ | ただし代入可能なグローバル（(D5) のため動的束縛ではない） |
| `*trace-output*` / `*query-io*` / `*terminal-io*` / `*debug-io*` | ⛔ | (D5) |
| `y-or-n-p` / `yes-or-no-p` | ✅ | 2026-08-20 実装（Phase 9c）。`*standard-input*` から読み、受け付けるまで訊き直す。入力の終端だけが止め、そのとき `false` |

### 2.19 プリンタ（CLHS 22）

format と pretty printer は実装済み（functions.md §15/§15.1/§15.2）。差分:

| CL | 状態 | 備考 |
|---|---|---|
| `format` | ✅ | ディレクティブはほぼ全対応（`~/name/` のみ未対応）。出力先は `true`/`false`／`CharOutput` を実装したストリーム（2026-08-05） |
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
| `read` | ✅ | ストリームからは `read-sexpr`（`PeekInput` を取り `Result<Option<Sexpr>,ReadError>` を返す。入力末尾は `Ok(none)`）。2026-08-05 |
| `read-from-string` | ⚠️ | これが `(read s)`。ただし読んだ位置（第2値）が返らない |
| `read-preserving-whitespace` / `read-delimited-list` | ❌ | |
| `readtable` 関連（`copy-readtable` / `set-macro-character` / `get-macro-character` / `set-dispatch-macro-character` / `make-dispatch-macro-character` / `readtable-case` / `*readtable*`） | ❌ | **リーダマクロが定義できない**。`#.`/`#+`/`#-` 等の読み込み時制御も無い |
| `*read-base*` / `*read-default-float-format*` / `*read-suppress*` / `*read-eval*` | ⛔ | (D5) |
| `with-standard-io-syntax` | ⛔ | (D5) |
| `parse-integer` | ✅ | `parse-int` |

### 2.21 システム構築（CLHS 24）・環境（CLHS 25）

| CL | 状態 | 備考 |
|---|---|---|
| `load` | ✅ | ソースを読んで順に評価するフラットロード。トップレベル専用 |
| `require` / `provide` / `*modules*` | ⚠️ | `module`/`use`＋ファイル↔モジュール対応が相当 |
| `*features*` / `#+` / `#-` | ⚠️ | 2026-07-30実装。`#+`/`#-`（`and`/`or`/`not`合成式込み）をリーダに追加。`*features*`はCLと違い**読み込み中に書き換え不可の固定集合**（全フォームを読んでからチェック/評価する既存アーキテクチャのため）。デフォルトはホストOS/アーキテクチャ＋`:typelisp`、`typl`の`--feature NAME`で追加可能 |
| `compile-file-pathname` / `*compile-file-pathname*` / `*load-pathname*` 等 | ❌ | |
| `time` / `get-internal-real-time` / `get-internal-run-time` / `internal-time-units-per-second` | ⚠️ | 2026-07-31 実装。`get-internal-real-time`（`i64`、マイクロ秒＝`internal-time-units-per-second` は 1_000_000）と、それを使う `time` マクロ（経過実時間を1行印字して `form` の値をそのまま返す）。**`get-internal-run-time`（CPU 時間）は無い**——`getrusage` に `libc` が要り、ワークスペースは `libc` に依存していない。実時間で代用すると嘘になる |
| `get-universal-time` / `get-decoded-time` / `encode-universal-time` / `decode-universal-time` | ✅ | 分解・合成を 2026-08-20 実装（Phase 9c）。多値が無いので `decoded-time` という `defstruct` 7 フィールドで返す。**CL は zone 省略時に地方時へ分解するが、ここは UTC**（タイムゾーンデータベースが無い）。CL にもある明示 zone 引数が代わり |
| `sleep` | ❌ | |
| `room` / `ed` / `dribble` / `apropos` / `apropos-list` / `inspect` / `describe` | ❌ | 対話環境向け。REPL があるので `apropos`/`describe` は相性が良い |
| `documentation` / docstring | ✅ | 2026-07-30実装。`defun`/`defmethod`/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/`deftrait` が docstring を持てる（位置は各フォームの CL 規則通り）。`documentation` は名前を評価せず解決する特殊形（`quote`/`compile` と同様）で check 時に定数へ畳み込まれる。LSP hover にも統合済み。`(setf documentation)` は対象外（functions.md §17） |
| `lisp-implementation-type` / `lisp-implementation-version` / `machine-type` / `software-type` | ✅ | 2026-08-20 実装（Phase 9c）。版数は Cargo から、機種と OS は `std::env::consts` から、いずれもコンパイル時に決まる |
| `machine-version` / `machine-instance` / `software-version` / `short-site-name` / `long-site-name` | ⛔ | ホスト名の取得に `libc` が要る。残りは CL でも `NIL` を返してよい——中身の無い定数を並べるより置かない方を選んだ |
| `user-homedir-pathname` | ✅ | 2026-08-20 実装（Phase 9c）。`$HOME` が無ければ `none`（CL も `NIL` を許す）。環境変数を読む `getenv` と、CL に無い `command-line-args` も同時に入った |
| `trace` / `untrace` / `step` / `disassemble` | ❌ | |
| コマンドライン引数の取得 | ❌ | CL 標準にも無いが、`typl file.typl` でスクリプトを書く以上ほぼ必須 |

---

## 3. 横断的な欠落（個別の関数より効いてくるもの）

上の表は関数単位だが、実際には**1つの機構が無いために関数が束で落ちている**箇所がある。
作成時（2026-07-29）に「足すなら効果が大きい順」で並べた 1〜7 と 9 はその順序のまま残し、
解消したものに取り消し線を引いて、いつ何で解消したかを書き足してある（8 は 2026-08-18 の
見直しで追加した項目）。**いま生きているのは 2（の後半）・4・7（の残差）・8 の4つ**で、
丸ごと残っているのは 4 と 8 だけ。

1. ~~**ストリームとファイル I/O**（§2.17/§2.18）~~ — 2026-08-02（ストリーム）と 2026-08-05
   （パス名・`read-sexpr`・`format` の出力先）で解消。CLHS 21章はトレイト階層として、
   19/20章はパス名層として入っている。残差は §2.18 のバイナリ I/O・`listen` 系だけ。
2. ⚠️ **関数の `&optional` / `&key`** — 機構としては 2026-07-29 に解消（`defun` が両方取れる。
   `defmacro` は 2026-07-24 から）。ただし **`lambda` は `&rest` のみ、`defmethod` は
   `&rest` すら受け付けない**（`parse_defmethod_sig_inner` は `parse_param_pairs` を呼ぶだけ）。
   既存のシーケンス API も `:key`/`:test`/`:start`/`:end` を受けていない（§2.15）。
   `make-hash-table :test` と BOA コンストラクタも同様に未着手（§2.16/§2.6）——
   「書けない」から「書いていない」に変わった段階。
3. ~~**汎用 place（`setf` 展開子）**~~ — 2026-07-30 解消。place は変数・`変数::field` に加え
   `(accessor recv key...)` 形の呼び出し形（`recv` の静的型が `set-{accessor}` を持てば任意の
   アクセサ名で成立、ユーザ定義型も対象）に対応、`incf`/`decf`/`rotatef`/`shiftf`/
   `(setf (get ...))` を実装（詳細は §2.3 の該当行）。`defsetf`/`define-setf-expander` の
   ような実行時登録テーブルは意図的に作っていない——静的型を使えばそれ自体が要らない。
4. **多値** — `floor` の商と剰余、`gethash` の存在フラグ、`read-from-string` の読み終わり位置など、
   CL の API 設計は多値を前提にしている箇所が多い。typelisp は `Option`/`cons-cell` で個別に
   回避しているが、CL コードの移植では毎回書き換えが要る（`gethash` 相当は `get`→`Option<V>` で
   解決済み、`floor` 相当は2026-07-29に `floor-div` 等→`cons-cell` で解決済み。§2.10 参照）。
5. ~~**数値ライブラリの基礎**（§2.10）~~ — 2026-07-31 に解消。可変長 `+ - * / < <= > >=`（0/1引数版
   込み）・`max`/`min`・`zerop`/`plusp`/`minusp`/`evenp`/`oddp`・超越関数一式・`pi`・ビット演算
   （`logand` 系／`byte`/`ldb`/`dpb`/`boole`）を、JIT/AOT 対応込みで実装。残差は複素数と
   浮動小数点の内部表現アクセス、および `most-positive-fixnum` 等の定数。
6. ~~**述語や比較関数を引数に取れないコレクション API**~~ — 2026-07-31 に解消。`sort` は CL 本来の
   `(sort sequence predicate)` になり、項目ベースの `find`/`position`/`count` が述語版
   （`-if` 系）と並立した。残るのは 2 の後半、つまり `:key`/`:test` 等のキーワード引数。
7. ⚠️ **乱数の再現性**（§2.10）— 2026-07-31 に `time`/`get-internal-real-time`/
   `get-universal-time` と `random-state` 一式、2026-08-20 に日時の分解・合成
   （`decode-universal-time` 等、Phase 9c）が入った。残差は CPU 時間
   （`get-internal-run-time`、`libc` 依存）と、**乱数のシードを外から与える手段**——
   同一プロセス内なら `make-random-state` の複製で列を再生できるが、実行を跨いだ再現はできない。
8. **多次元配列・集合演算・文字列ユーティリティ**（§2.13/§2.12/§2.14）— いずれも単独の機構では
   なく「同じ層の関数が束で無い」箇所。`make-array`/`aref` 一式、`union`/`intersection`/`adjoin`、
   `string-trim`/`search`/`concatenate`/`split` 相当。CL コードの移植で真っ先に当たるのはここ。
9. ~~**`*print-circle*` / `*print-level*` / `*print-length*`**~~ — **2026-07-29 実装済み**
   （functions.md §15.3、[implementation-log.md](implementation-log.md) の該当節）。着手前は
   「循環構造を印字するとプロセスが落ちる」状態だった——`defstruct` のフィールドを `setf` で
   自分自身へ向けた値を `println` するとスタックオーバーフローで abort することを確認しており、
   `*print-circle*` を真にすればラベル付き（`#1=…#1#`）で印字できるようになった。

## 4. このドキュメントの位置づけ

ここに並べた ❌ は**すべてが TODO ではない**。[TODO.md](TODO.md) の「残っている作業」は現時点で
空であり、この一覧は「CL と比べたときの残差はどこか」を測るための地図として作った。着手する
場合は §3 の（取り消し線の付いていない）項目が費用対効果の目安になる（優先度は筆者の見立てで、
確定した方針ではない）。

この地図は放っておくと実装より古くなる。実際、2026-08-18 の見直しでは、作成時に挙げた §3 の
8項目のうち5項目がすでに解消済みで、そのうち3項目（1・5・6）は解消から今回まで表に反映されて
いなかった——`defun` の `&optional`/`&key` に至っては、この表を書いた**その日の夜**に入っている。
**表を根拠に「無い」と判断する前に、必ず `crates/typelisp-front/src/check/registry.rs` /
`crates/typelisp-front/src/prelude.rs` / `crates/typelisp-front/src/check/checker.rs` を
grep して確かめること。**

⛔ の項目については、[language-design.md](language-design.md) §7・§8・§9（採用しないと決めた
機能）が一次情報。CL に同名の機能があることを理由にこれらを再検討する場合は、
まず (D1)〜(D5) のどれと衝突するかを確認すること。
