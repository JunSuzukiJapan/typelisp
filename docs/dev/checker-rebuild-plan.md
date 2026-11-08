# 型チェッカーの作り直し — 洗い出しと設計方針（案、2026-11-08）

## 1. 目的

継ぎはぎの修正が重なった型チェッカーを、小さい機能ごとに分かれた、見通しのいい設計に作り直す。
最初の作業量より、その後の追加・修正にかかる時間を短くすることを優先する（ユーザー決定）。

## 2. 今のチェッカーの境界

作り直しの間、外との境界は変えない。境界を固定すれば、評価器・コンパイラ・島・LSP には手を入れずに済む。

| 方向 | 中身 |
|---|---|
| 入力 | リーダーが読んだフォーム（`Value`）。最上位フォームを1つずつ受け取る |
| 出力 | core IR（`TopLevelForm = Value`、cons セルの木）。`Interp::exec` と島がこれを読む |
| 公開の入口 | `check_form` / `check_form_at`、`publish_names`、`finish_unit`、モジュールの出入り（`enter_module` など）、REPL とダンプ用の `signature` / `capture_delta` / `apply_delta`、LSP 用の `take_type_uses` / `take_errors` / `expr_type` / `registry` |
| 内部の流れ | フォーム → 解析（`Node` の木、`Checked`）→ 変換（`lower.rs`、`Node` → core IR） |

`Node` と `lower.rs` はチェッカーの内側にある。作り直しの対象に含めるかは 6 節で決める。

## 3. チェッカーが受け持つ機能

規模は `check/` 全体で約 3 万行。そのうち `analyze.rs` 1 ファイルが約 1.8 万行で、メソッドが約 430 個ある。

### 3.1 最上位フォーム

`defun` `defsignature` `defffi` `defvar` `defparameter` `defconstant` `defmacro` `module` `defmethod`
`defstruct` `defenum` `deftrait` `deftype` `impl`（`impl<T>` を含む）`use` `pub` `unsafe`（`def-c-struct`）。

### 3.2 式の特殊形

`if` `let` `let*` `progn` `unsafe` `c-alloc` `c-ref` `c-deref` `setf` `incf` `decf` `rotatef` `shiftf`
`loop` `block` `return-from` `catch` `throw` `unwind-protect` `break` `return` `list` `format`
`print` `println` `eprint` `eprintln` `pprint` 系 `pprint-logical-block` `lambda` `labels` `macrolet`
`symbol-macrolet` `apply` `match` `panic` `the` `make-instance` `task` `thread` `select` `as` `try-as`
`compile` `documentation` `ed` `trace` `untrace` `step` `disassemble` `source-file` `quote` `quasiquote` `:dyn`。

### 3.3 横断する仕組み

- 名前解決（モジュールの木、`use`、パス、メソッドの受け手）
- 型の構文解析と正規化（別名の展開、未定義の型名を断る）
- 登録表（`Registry`：関数・型・トレイト・impl・別名・ドキュメント・定義位置）
- ジェネリック（単型化の要求・テンプレート・特殊化名）
- トレイト（境界の検査、関連型、ブランケット impl、デフォルト本体、`:dyn` の表）
- マクロ展開（`defmacro`、`macrolet`、コアマクロ）
- 表現の分類（`Repr`、Option のニッチ、`Sexpr` への包み方）
- LSP 向けの記録（型の使用位置、ローカル参照、式の型、エラー回復）
- REPL・ダンプ向けの差分
- `#[test]` の収集、FFI コールバックの持ち上げ、`format` の書式エンジン、`loop` の DSL

## 4. 見つかった設計上の問題

### 4.1 型変数を名前で扱っている

`AdtDef.params` は名前の列で、フィールドやメソッドのシグネチャは `Type::Var("T")` の形で保存される。
ほかの定義は、受け手や対象に書かれた名前を宣言側の名前に書き直してから登録される
（`check_defmethod_in_form` の `owner_rename`、`check_impl_form` の `rename`）。

呼び出し側では、型の側のパラメータとメソッド自身のパラメータを、名前をキーにした 1 つの置換表に入れている。
名前がぶつからないようにするための検査が、誤って正しいコードを断っている。

```lisp
(defstruct Pair<T, U> (first T) (second U))
(impl Pair<A,B> (defmethod mk<T> (self (x T)) T x))
;; error: the method's own type parameter `T` has the name of a type parameter of `Pair`
```

実行時の型の識別（`type_key.rs`）は、すでに位置（`TypeShape::Param(i)`）で扱っている。
名前に頼っているのはチェッカーだけである。

### 4.2 書いただけで型変数が宣言されたことになる

`written_type_vars` が、`impl` の対象や `defmethod` の受け手に書かれた未知の名前を、すべて型変数として宣言扱いにする。
`(impl<T> ...)` の頭に書いた宣言は、ブランケット impl のとき以外は使われていない。
型変数は宣言した場所でだけ生まれる、に直す（ユーザー決定）。

```lisp
(impl<T,U> Pair<T,U> ...)
(defmethod pair-first<T,U> ((p Pair<T,U>)) T (first p))
```

### 4.3 ジェネリックな本体を 2 回、別の規則でチェックしている

ジェネリックな本体は、まず型変数を抽象のままチェックし、インスタンス化のたびに元のフォームを具体型でチェックし直す。
2 回のチェックは規則が食い違う。

- 抽象のチェックは Rust と同じく境界を要求する（`(+ x 1)` は「`T` に `+` が無い」でエラー）。
- 具体型でのチェックは、境界が無くても具体型で通れば通る。
- 抽象のチェックがエラーを出しても、テンプレートは登録に残る。そのため REPL では、エラーになった定義がそのまま呼べる（4.4）。
- 抽象のチェックだけが断る書き方があった。ジェネリックな本体の中の `(format false "~a" x)` がそれで、`x : T` を `Sexpr` に包めないとして断っていた（2026-11-08 に、型変数のときは包み方を具体型のチェックへ回すよう修正済み）。

### 4.4 チェックに失敗した定義が登録に残る

`check_defun` などは、自己再帰のためにシグネチャとテンプレートを本体のチェックより先に登録する。
本体のチェックが失敗しても登録を取り消さない。ファイル実行は最初のエラーで止まるので表に出ないが、REPL と LSP では、エラーになった定義が残って呼べてしまう。

```lisp
(defun inc<T> ((x T)) T (+ x 1))   ; error: no method `+` for type variable `T`
(inc 41)                           ; => 42
```

今のチェッカーでは直さず、新しいチェッカーで 5.3 の仕組みにより直す（ユーザー決定）。

### 4.5 ファイルと状態が大きすぎる

- `analyze.rs` 1 ファイルに、最上位フォーム・式・パターン・ジェネリック・トレイトがすべて入っている。
- `Checker` は 35 個、`Info` は 7 個の状態を持つ。その多くが `Cell` / `RefCell` で、チェックの途中で書き換わる
  （ループの段、ブロックの段、`unsafe` の深さ、推論の試し打ち `infer_probe` と穴の数 `probe_holes`、特殊化の待ち行列、型変数の束縛 `type_var_bindings` など）。
- 最上位フォームの振り分け表が、`pub` 付きと無しで 2 つある（`check_one_form` 内と `check_pub`）。

## 5. 設計方針（案）

### 5.1 小さい機能ごとにモジュールを分ける

1 つのモジュールは 1 つの機能だけを受け持ち、ほかのモジュールとは決まった型（`Type`、`Checked`、登録表の読み書き API）だけでやりとりする。
分け方の案を示す。

| モジュール | 受け持ち |
|---|---|
| `types` | `Type` の定義、構文解析、正規化、置換、単一化 |
| `scope` | 型変数の宣言とローカル変数の環境（親フレームを共有する連鎖） |
| `resolve` | 名前解決（モジュールの木、`use`、パス） |
| `registry` | 登録表。書き込みは 1 つの最上位フォームの単位でまとめて確定する |
| `toplevel/*` | 最上位フォーム 1 種類につき 1 ファイル（`defun.rs`、`defstruct.rs`、`impl.rs` …）。振り分け表は 1 つ |
| `expr/*` | 特殊形をまとまりごとに 1 ファイル（制御、束縛、代入、脱出、出力、関数、`match` …） |
| `generic` | インスタンス化と特殊化 |
| `traits` | 境界の検査、関連型、ブランケット impl、`:dyn` の表 |
| `macros` | マクロ展開 |
| `repr` | 表現の分類、`Sexpr` への包み方 |
| `ide` | LSP 向けの記録 |
| `lower` | `Node` → core IR |

### 5.2 型の表し方

- 型の側のパラメータは位置で持つ。登録表のシグネチャには、利用者が書けない位置の印で保存する。
- 型変数は宣言（`defun f<T>`、`defstruct Pair<T,U>`、`impl<T,U>`、`defmethod m<T,U>`、`deftrait` の `(:type ...)`）からだけ生まれる。
- メソッドの型パラメータは、宣言の頭に書いたものだけになる。受け手の型 `Pair<T,U>` は、ほかの引数と同じ「パラメータの型」として扱う。
  型の側のパラメータとメソッド自身のパラメータという 2 種類の区別が無くなり、名前がぶつかる検査も要らなくなる。

### 5.3 登録は成功してから確定する

1 つの最上位フォームの登録は、まとめて仮に書き込み、チェックが成功したときだけ確定する。失敗したら全部捨てる（4.4 の解消）。
自己再帰は、仮の書き込みが同じフォームの中から見えることで支える。

### 5.4 ジェネリックのチェック（ユーザー決定）

本体を 1 回だけ、境界にもとづいてチェックする（Rust と同じ）。インスタンス化は、チェック済みの木の型を置き換えて作る。
表現によって変わる選択（`Sexpr` への包み方など）は、置き換えた後に決まる節点として残す。

これを採ると、今「具体型では通る」だけで境界を書いていないジェネリックな定義は、エラーになる（4.3）。

## 6. 進め方（ユーザー決定）

1. この文書で設計を決める。
2. 新しいチェッカーを今のものの隣に置く。prelude を読み込まないテストから順に、新しいチェッカーでも通るようにしていく。
   prelude はほぼすべての機能を使うため、prelude を読み込むテストを新しいチェッカーで回せるのは、移植の終盤になる。
3. `Node` と `lower.rs` は、最初は今のものを使い回す。直すべき点が見つかったら、切り替えのあとの次の段階で扱う。
4. 全テストが新しいチェッカーで通ったら切り替え、古いチェッカーを丸ごと消す。
