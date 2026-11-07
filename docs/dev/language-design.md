# typelisp 言語設計（確定仕様）

最終更新: 2026-11-07 / ブランチ: `main`

このドキュメントは、設計で**確定した言語仕様とその理由**を後から見返せるよう記録するもの。
構文と組み込み関数の網羅的な一覧は利用者向けリファレンス（[syntax.md](../ja/reference/syntax.md)、
[types.md](../ja/reference/types.md)、[functions/](../ja/reference/functions/README.md)）にあり、
ここでは繰り返さない。現在の残作業は [TODO.md](TODO.md)、完了した実装の経緯は
[implementation-log.md](implementation-log.md) を参照。

---

## 0. 言語の基本方針

- **静的型付け**: すべての式が静的型を持つ。動的タグ付き Lisp ではない。関数の引数・戻り型と
  グローバル（`defvar`/`defparameter`/`defconstant`）の型は必須で、型推論するのは局所束縛（`let`）だけ。
- **文法は S 式で、CL に寄せる**。特殊形・マクロ・関数の名前と意味は、静的型付けと両立する限り
  Common Lisp に合わせる。合わせられないものは [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md)
  に理由とともに記録する。
- **真偽値は `true`/`false`**。`nil`/`t` は言語に存在しない。
- **`nil` の代替は `Option<T>`**（組み込みの直和型。変種は `(some v)` と `none`）。
- **S 式データの型は `Option<Sexpr>`**。`Sexpr` は「空でない S 式」で、空リスト `()` は
  `Sexpr` の変種ではなく `Option` の `none` になる。`Sexpr` の変種は `int`・`i8`〜`u32`・`f32`・
  `f64`・`char`・`bool`・`sym`・`str`・`cons`・`ratio`・`path`・`vector`・`array`・`tuple` の 19 個
  （変種番号は `crates/typelisp-front/src/sexpr_variant.rs` に集約）。数値の変種が幅ごとに
  分かれているのは、`Sexpr` が「値の型がほかのどこにも書かれていない唯一の場所」だから
  （[preserve-bit-width-plan.md](preserve-bit-width-plan.md)）。`read` の戻り型は
  `Result<Option<Sexpr>, ReadError>`。
  - `()` は期待型が `Option<Sexpr>` なら `none`、それ以外の文脈では `Unit` の値。これは言語レベルで
    禁止した「真偽値としての `nil`」とは別物。
  - 裸の `cons`/`car`/`cdr` はジェネリックなペア `cons-cell<A,B>`（標準ライブラリの `defstruct`）の
    操作で、S 式専用ではない。S 式のセルは `sexpr-cons`/`sexpr-car`/`sexpr-cdr` などで扱う。
  - `dolist` は S 式リスト専用の反復で、各要素は異種の S 式なので本体で `match` して形を分ける。
    反復と分岐を融合しない（本体は素の `progn`）。
- **`Symbol` は `Sexpr` とは別の独立した型**（静的型 `symbol`）。`Sexpr` が要求される文脈へは
  `sym` 変種として暗黙変換されるが、逆方向の自動変換はない。文字列からシンボルを得るのは
  `intern` だけ（CL 準拠。`string->symbol` は無い）。
- **`Sexpr` はユーザ定義型の値も保持できる**（2026-07-24、CL の cons が任意のオブジェクトを
  保持できるのに合わせた決定）。ヒープ表現を持つ登録済みの型（`defstruct`/`defenum`/`Vector<T>`/
  `HashTable<K,V>`/`cons-cell<A,B>`/`Option`/`Result` など）は透明な retype で入り、スカラは対応する
  `Sexpr` 変種で包まれる。取り出しは `match` の downcast パターン（型名先頭のフィールド分解、
  列挙型の変種名、`(the T p)`）で、これは `Sexpr` 本来の変種の網羅性には数えない。`equal` は
  CL と同様に構造体・列挙型に対して同一性（`eq`）のまま、`equalp` はスロットごとの再帰比較。
- **大文字小文字を区別する**（2026-10-25 決定）。シンボルは書いたとおりの綴りでインターンし、
  正規化しない。型名は大文字で始まり（`Option`、`Vector`）、基本型（`int`、`string`）と変種名
  （`some`、`none`、`ok`、`err`）は小文字。`match` の腕の一番外に裸で書いた名前は常に変種で、
  何にでも当たるのは `_` だけ。
- **キーワードは CL 同等**（2026-10-17 決定）。`:name` はどのモジュールで書いても名前空間
  `keyword` に属する同じ 1 つのシンボルで、自己評価する。
- **Rust との相互運用はしない**。外部のコードとつなぐのは C の FFI（`defffi`、`def-c-struct`、
  コールバック）だけで、検査できない前提は `unsafe` で引き受ける。
- **実行モデル**: インタプリタ（eval）と、LLVM による明示的なネイティブコンパイル（`compile` は
  JIT、`compile-file` は AOT で実行ファイルを出す）。コンパイラ本体は typelisp 自身で書かれている
  （自己ホスト。コードベース内の呼称は「島」）。標準ライブラリ（prelude）は事前にコンパイルされて
  コミットされている（§2.4）。
- **ファイル拡張子**: ソースは `.typl`。セッションのダンプ（`(dump ...)`、`typl --image`）は
  `.typld`。モジュール単位のコンパイル済み形式は無く、`.typlc` という拡張子も使わない
  （`compile-file` は実行ファイルを直接出し、中間ファイルを残さない）。
- **命名規則 `!`/`?`**: 関数名の末尾に `!`（破壊的操作）や `?`（述語）を付けない（§7.3）。

---

## 1. メモリモデル / GC

- cons セルは**チャンク単位のアリーナ**に置く。チャンクは再確保も移動もしないので、生ポインタは
  ヒープの寿命のあいだ安定する。足りなくなればチャンクを**追加して伸びる**。伸びる上限は
  初期容量（既定 65536 セル、`typl --heap-cells N` で指定）の 256 倍で、そこを超えた確保は
  `heap exhausted` になる——初期容量は「最初にこれだけ確保する」、上限は「ここを越えたらリークと
  みなす」という意味。
- 割当はフリーリストから。空なら GC して再試行する。
- **精密な mark-sweep GC**。マークは反復（深い構造でもネイティブのスタックを使わない）で、
  循環も回収する。文字列と箱詰めのオブジェクト（構造体・列挙型・ベクタ・クロージャ・trait
  オブジェクトなど）も同じヒープにあり、到達できなければ回収される。シンボルはインターンされ、
  回収されない（CL のパッケージ内のシンボルと同じ）。
- **ルートは明示的に登録する**。スコープの出入りで push/pop する LIFO のスタック（タスクごとに
  1 本）、持ち主が寿命を超える値の永続ルート、区間だけ生かすセッションルート、飛行中の `throw` の
  値、の 4 つ。`Vec<Value>` に置いただけの値はコレクタから見えない。
- **複数の OS スレッドが 1 つのヒープを共有する**。各スレッドはビューを持ち、GC は他の全スレッドが
  safepoint かネイティブ区間で止まるのを待ってから走る（stop-the-world）。設計は
  [os-threads-design.md](os-threads-design.md)。
- **生ポインタは `ConsRef` などに隠し、公開 API は安全**。`unsafe` は `typelisp-mem` に閉じ込める。

---

## 2. `::`・module・型の関係（Rust 流）

### 2.1 `::` は reader がパスに分割する
- ソース上の `Foo::Bar` は**読み取り時に**セグメント列へ分割され、パスの値になる。解決時に
  文字列を再分割しない。
- reader が行うのは**明示された `::` の分割だけ**。裸名のスコープ解決と、各セグメントが module か
  型かの判定は、すべて型検査器（checker）が行う。
- 先頭が `::` のものはルートからの絶対パス。空セグメント（`foo::`、`a::::b`）は読み取りエラー。
  セグメントの綴りは変えない（大文字小文字を区別する）。
- 総称は最終セグメントに付き（`a::Vec<T>`）、型引数の内側の `::` はパス区切りにならない。
- **名前を `::` でつないだ文字列として登録・比較しない**（2026-10-17 決定）。`foo::bar::zot` は
  「foo の子に bar、bar の子に zot」というモジュールの木と、そこに属するシンボルで表す。

### 2.2 module は名前空間、型は名前空間ではない
- **module = 名前空間**（`module`/`in-module`/`use` で扱う）。
- **型は Rust と同様**に、静的メソッドとメソッドを持つだけで、名前空間ではない。
- `Foo::Bar` の意味は文脈で決まる（Rust のパス解決と同じ）。`Foo` が module なら「module Foo の
  項目 Bar」、型なら「型 Foo の変種か静的メソッド Bar」。値の位置の `x::f`（`x` が値の名前）は
  フィールドの読み出し。
- **型とトレイトは同じ名前空間**を共有する（§7.4）。

### 2.3 ファイル↔モジュール対応（2026-07-11 実装）

- **ファイルパス＝モジュールパス**: ソースルートからの相対ファイルパスがそのままモジュールの
  パスになる（`<root>/geo/point.typl` → `geo::point`、ディレクトリも 1 セグメント）。ファイルの
  内容は暗黙にそのモジュールに包まれ、明示の `(module bar ...)` はその内側にネストする
  （`geo::point::bar`）ので、両者を調停する規則は要らない。
- **ソースルート**: マニフェスト `typelisp.toml`（空でよい。認識するキーは `src = "dir"` だけ）を
  上方に探す。無ければエントリファイルのディレクトリ（REPL はカレントディレクトリ）。
- **1 フォームずつ読む→依存を解決する→検査する→実行する**（`crates/typelisp-front/src/project.rs`）。
  次のフォームは、前のフォームが走り終わるまで読まない。これはリーダマクロと読み込み時評価
  （`#.`）が、手前で定義した関数を呼べるようにするため。その帰結として、`use` は**それより後の
  フォーム**の依存であって、ファイル全体の依存ではない（`(use m)` より上で `m::f` と書くと
  未解決になる）。
- **モジュールファイルは単位として検査し、実行は `use` した側に任せる**（CL の `compile-file`
  に相当）。`load` と REPL はフォームごとにその場で実行する（CL の `load`）。
- **オンデマンドロード**: `(use geo::point)` が未ロードのモジュールを指すと、対応するファイルを
  読み込み、同じ `Registry` に検査して登録する。`use a::b::c` は `a/b/c.typl` → `a/b.typl` →
  `a.typl` の最長プレフィックス順で探す。
- **マクロ展開が生成する `use`**（2026-07-15）: トップレベルのマクロ呼び出しはフォームごとに
  事前展開し（`Checker::try_expand_toplevel_macro`）、展開結果の依存をロードしてから展開結果を
  トップレベル形として検査する。依存のロード中は名前空間の文脈を退避する
  （`Checker::suspend_ns_context`。依存が現在のモジュールの下にネストして登録されるのを防ぐ）。
- **循環参照は明示エラー**（`circular module dependency: a -> b -> a`）。黙って飛ばすと、後段で
  紛らわしい「no such function」になる。モジュールをまたぐ相互参照は受け付けない。
- **実行**: `typl <file.typl>`（引数なしは REPL）。REPL の `use` も同じローダで解決し、REPL で
  書いた定義はモジュール `user` に入る。LSP（`typl-lsp`）も同じローダでファイルをまたいだ
  診断を行う（コードは実行しない）。

### 2.4 `(load)` と prelude

- **`(load "path")`**: CL 流のフラットロード（対象ファイルのフォームを今の名前空間に読み込む。
  `use` のようにモジュールで包まない）。トップレベル専用。`path` は読み込み元ファイルの
  ディレクトリからの相対で、拡張子が無ければ `.typl` を補う。
- **モジュール単位のコンパイル済み形式は無い**。2026-07-14 に fasl（検査済み状態の直列化、
  `.fastl`）を入れたが、2026-08-14 に削除した——read と型検査を飛ばすだけで、実行を速く
  しなかったため。経緯は [implementation-log.md](implementation-log.md)。
- **prelude はコミット済みのダンプから読む**（`crates/typelisp-front/src/prelude.typld`、
  `src/compile/prelude_bootstrap.rs`）。ダンプは検査済みの状態と、ジェネリックでない定義の
  ネイティブコードの両方を持つ。ダンプが今の `SOURCE` から作られたものでなければ起動時に
  断る。`SOURCE` を変えたら `scripts/regen-prelude-bitcode.sh` で作り直す
  （[development.md](development.md)）。LLVM を持たないビルドと生成器は、ソースから読んで
  検査し、インタプリタで実行する（`prelude::load_interpreted`）。

### 2.5 名前解決規則（2026-10-11 一本化）

設計と経緯は [name-resolution.md](name-resolution.md)、利用者向けの規則は
[syntax.md](../ja/reference/syntax.md) §3.10。要点:

- **解決は 1 か所で行う**（`Checker::resolve_name`）。呼び出し・値・パス・`compile`/`trace`
  などの全員がそこを通り、実行時には解決し直さない。
- **関数・グローバル変数・マクロは 1 つの名前空間**に入る。CL のように関数と変数で名前空間が
  分かれてはいない。同じモジュールで 2 つに同じ名前は付けられない。
- **メソッドは第 1 引数の型だけ**から探す。
- 裸名 `n` の呼び出し `(n a ...)`（特殊形の名前はこれより先に決まる）: ローカル → モジュールの
  名前 `n` がマクロならそれ → 構成子と `use` した静的メソッド → 第 1 引数の型のメソッド `n` →
  モジュールの名前 `n` が関数かグローバル変数ならそれ。
- モジュールの名前は、今のモジュール自身の定義 → `use` の別名 → 標準ライブラリが公開する名前の
  順に探す。**親のモジュールは探さない**。
- **標準ライブラリ（`std::prelude`）が公開する名前以外は `use` が要る**（2026-10-17 決定、
  [explicit-symbols-plan.md](explicit-symbols-plan.md)）。標準ライブラリが公開する名前と同じ名前を
  定義するには先に `(shadow name)` を書く。予約語（特殊形の名前）は定義の名前にできない。

---

## 3. 特殊形とトップレベル定義

特殊形とトップレベル定義の名前の一覧は 1 つの定数が持つ
（`crates/typelisp-front/src/check/analyze.rs` の `SPECIAL_FORM_HEADS` と `TOPLEVEL_FORM_HEADS`。
一覧がチェッカーの振り分けと一致することは `editor_keyword_sync_test` が確かめる）。構文と意味は
[syntax.md](../ja/reference/syntax.md)。`when`/`unless`/`cond`/`case`/`dotimes`/`dolist`/`doiter`/
`while-let` などは特殊形ではなくマクロで、標準ライブラリかコアマクロ層に書かれている。

**特殊形よりマクロ**: 新しい構文は、既存の型推論で書けるならマクロで書く。例えば `doiter` は
`while-let` を呼ぶだけのマクロで、ループ変数の型は展開後の `(some var)` パターンの型から
`Checker::check_ctor_pattern` が推論するので、checker の特殊形にする必要が無い。

確定している規則:

- **マクロは定義より後のフォームでしか呼べない**。`defmacro` の登録はそのフォームを実行した
  副作用として起きるため、原理的にそれより前のフォームからは呼べない。CL 流で非衛生的。
  モジュールをまたいで `mod::macro-name` と呼ぶには `pub defmacro` が要る。
- **トップレベルは 1 フォームずつ、ソース順に検査・実行する**。`defun` も定義より前では呼べず、
  **前方参照は `defsignature` で明示的に宣言する**（相互再帰はこれでしか書けない）。2026-08-01
  から 2026-08-23 までは全 `defun` のシグネチャを先行登録していたが、それは「最初のフォームを
  検査する前に全フォームを読む」ことを要求し、リーダマクロと両立しない。宣言できないもの
  （マクロ・型・ジェネリック関数）とその理由は [syntax.md](../ja/reference/syntax.md) の
  `defsignature` 節にある。
- **グローバルの型注釈は必須**——`(defvar (name Type) value)`。グローバルの型はプログラムの
  公開面であり、初期化子から推論しない（型なしの形は 2026-07-03 に削除）。注釈が初期化子の
  期待型になるので、ジェネリック関数を値として渡す単型化もここから効く。
- **`lambda` は真のクロージャ**。捕捉した変数への `setf` は共有セルを書き換え、同じセルを
  捕捉した全クロージャから見える（値のスナップショットではない）。名前付きの関数も値として
  渡せる。
- **`break`/`return` は直近のループだけを脱出する**（関数の早期リターンではなく、`lambda` の
  境界も越えない）。`loop` の型は内側の `break`/`return` の値型の join で、一度も脱出しない
  `loop` は `!`。関数から抜けるのは `return-from`（`defun`・メソッド・`labels` の各関数は自分の
  名前の `block` を暗黙に張る、CL と同じ）。
- **`let` は常に字句的**。CL の special 変数のような動的束縛は無く、これからも入れない。制御変数
  （`*print-base*` など）を一時的に変えるのは `dlet`——「保存 → 代入 → `unwind-protect` で復元」
  するスコープ付きの再束縛で、`let` の意味は変えない（[cl-parity-plan.md](cl-parity-plan.md) の (D5)）。
  タスクごとの束縛にはならない。
- **`setf` は代入した値を返す**（CL と同じ）。そのため `(if c (setf ...))` のように片側だけの
  `if` に置くと型が合わない。

---

## 4. 関数カタログ（Rust 組み込み vs typelisp ライブラリ）

**分離原則**: Rust でしか書けないもの（ヒープ・ランタイム・OS とのやりとり・プリミティブ演算・
ネイティブコード生成）だけを Rust の組み込みにし、それらの組み合わせで書けるものは typelisp
自身で書く（標準ライブラリ、`crates/typelisp-front/src/prelude.rs`）。すべて型付き。

一覧は [functions/](../ja/reference/functions/README.md) にあり、ここには持たない（同じ表を 2 か所に
持つと片方が古くなる）。

---

## 5. メソッド（impl）

メソッドはすべて `impl` に書く（2026-10-10 決定、[constructors-redesign.md](constructors-redesign.md)）。
`defmethod` は廃止し、書くとエラーになる。

- **固有 impl**: `(impl Type 項目...)`。Rust の `impl Point { ... }` に当たる。
  - 引数の先頭が `self` の項目は**インスタンスメソッド**で、`(name obj args...)` と呼ぶ。
    `self` には型を書かない（型は `Self`。`(self Type)` と書くと断る——2026-10-31 決定）。
  - `self` の無い項目は**静的メソッド**で、`(Type::name args...)` と呼ぶ。値を作る静的メソッドが
    その型のコンストラクタになる（名前は自由で `new` を勧める）。自動で作られる `Name::new` は無い。
  - 公開は項目ごと（`(pub name ...)`）。1 つの型に `impl` をいくつ書いてもよく、具体型ごとに
    別の `impl` も書ける（`(impl Box<i32> ...)`）。
- **構造体の値は `make-instance` で作る**（`(make-instance 'point :x 1 :y 2)`、CLOS と同じ綴り）。
  型名は検査時に決まる。
- **ディスパッチは既定で静的**（受け手の静的型で一意に解決する）。trait 機構（§5.1）もこの単一の
  静的ディスパッチの上に作られている。**動的ディスパッチは `:dyn Trait` 型を明示したときだけ**
  行われ、そのときは vtable を経由する（§5.2）。`:dyn` を書かないコードの表現とコストは変わらない。

### 5.1 trait 機構（deftrait / impl / where）

`doiter` が「`Iter` トレイトを実装した型すべてで使える」ことを要求したため 2026-06-30 に導入した。
静的メソッド解決をそのまま使う設計で、trait 専用のディスパッチ機構は作らない。構文は
[syntax.md](../ja/reference/syntax.md) §3.9。

- **trait 定義**: `(deftrait Name (Super...) (:type Assoc)... (method (self params...) Ret [body...])...)`。
  `Self` と関連型名はシグネチャの中で型変数として使える。
- **trait 実装**: `(impl Trait TargetType (:type Assoc Concrete)... 項目...)`。各メソッドは
  `TargetType` の普通のメソッドとして登録され（trait 用の別表は持たない）、`TargetType` が
  `Trait` を実装していることが記録される。trait の `impl` のメソッドは常に公開される。
- **スーパトレイト**（2026-08-01）: 名前の直後の**必須**リスト。Rust の `trait Ord: Eq` は
  `trait Ord where Self: Eq` の糖衣なので、内部表現は `where` 境界と同じものを使う。
  スーパトレイトの関連型は**全てピン留め必須**——そうしないと `:dyn Sub<...>` のピンを鎖に沿って
  合成できない。
  - **義務**: `impl Ord X` は `impl Eq X` が**先に**書かれていることを要求する
    （`check_supertrait_impls`）。Rust より制限が強いが、REPL でも逐次 `load` でも決定的に判定
    できる唯一の形。
  - **同名衝突**: サブが親のメソッドを再宣言すること、2 つの親から同名のメソッドを継承することは
    どちらもエラー。vtable のスロットは名前ごとに 1 つで、呼び出し側に曖昧さを解く構文が無いため。
    ダイヤモンドは宣言元が一致するので合流して 1 スロットになる。
- **デフォルト本体**（2026-08-01）: シグネチャの後ろに本体を書くと、`impl` が省略したときに使う。
  本体はトレイトを書いたモジュールの名前空間で解決する。型検査は宣言の時点で 1 回、`Self` を
  型変数のまま行う（Rust と同じ）。
- **ジェネリック関数の trait 境界**: `(:where (Trait T (Assoc Concrete)...)...)`。関連型は
  ピン留めできる（`(:where (Iter T (Item i32)))`）。境界は呼び出し側でも検査し
  （`validate_where_bounds`）、満たさない型を渡すと型エラーになる。ジェネリック関数は呼び出しの
  具体型ごとに単型化して検査し直す（型消去して実行はしない）。
- **impl レベルの `where`**（2026-08-01）: `(impl Ord cons-cell<A,B> (:where (Ord A) (Ord B)) ...)`。
  各メソッドの `where` へ構文的に合流させる（`merge_where_clauses`）。
- **impl の完全性・適合性検査**（2026-08-01、`check_impl_conformance`）: 宣言されていない
  メソッド・重複・関連型の欠落・メソッドの欠落・シグネチャ不一致を `impl` の時点で断る。
- **ブランケット実装**（2026-08-01）: `(impl<T> Clamp T (:where (Ord T)) ...)`。
  - 宣言時は**保存するだけ**で、具体型が要求したときに単型化のキューから実体化する
    （`materialize_blanket_impl`）。実体化は保存した項目を普通の `(impl Trait 具体型 ...)` に
    組み立て直して `check_impl` に通すので、「impl とは何か」の実装を二重に持たない。
  - **宣言時の本体検査**（2026-08-04、`precheck_blanket_impl`）: 対象を型変数のまま 1 回検査し、
    どの対象でも誤りであるものだけを宣言時に捕まえる。
  - 「型が trait を実装しているか」の問いは `type_implements` に集約する。明示の `impl` を先に
    見るので、明示があればそちらが勝つ。
  - **コヒーレンス**: 1 トレイトにつきブランケット実装は 1 つまで。明示とブランケットの重なりは
    明示優先で黙認する——ブランケットがある型を覆うかは後から確立されうる境界に依存し、
    「重なるか」に単一時点の答えが無いため。相互再帰的な境界は `BLANKET_BOUND_DEPTH` で打ち切る。
- `Sexpr` への `Iter<Item>` 実装は採用しない（§9）。

### 5.2 動的ディスパッチ（trait オブジェクト `:dyn Trait`）

2026-07-25 実装。**C++ の vtbl と同じ「呼び出し側は定数スロットを添字して間接呼び出しするだけ」の
形**。§5.1 の静的な機構はそのままで、`:dyn Trait` と書いた位置でだけ動的になる。

#### 表記

`:dyn` は型位置専用の予約キーワードで、それ以外に現れたらエラーになる（エディタが確実に強調
できるようにするため）。表記が空白区切りの 2 語なので、リーダが 2 か所で特別に扱う——datum の
位置では `:dyn X` を `(:dyn X)` に畳み、型引数の中（`Vector<:dyn Drawable>`）では `<>` が閉じる
まで空白込みで読む（閉じなければ巻き戻すので `(< a b)` などに影響しない）。

```lisp
(deftrait Drawable () (draw (self) string))
(defstruct circle (r i32))
(impl Drawable circle (draw (self) string "circle"))

(defun render ((d :dyn Drawable)) string (draw d))         ; 引数位置
(render (make-instance 'circle :r 3))                       ; 暗黙に箱詰めされる
(render (as :dyn Drawable (make-instance 'circle :r 3)))    ; 明示形
(defun all () Vector<:dyn Drawable> (Vector::new))          ; 型引数の中
```

関連型は**位置指定でピン留め**する。`:dyn Iter<i32>` は `Item` を `i32` に固定した trait
オブジェクトで、Rust の `dyn Iterator<Item = i32>` に当たる。

#### 表現: vptr は「値の側」に持つ

trait オブジェクトの実行時表現は fat box（`BoxedObj::Dyn { vtable_id, value }`）。vtable は
**(具象型, trait) の組ごとに 1 本**で、箱詰め地点では両方が静的に分かっている。

C++ のように vptr をオブジェクト自身に持たせ**ない**理由:

- 1 つの型が複数の trait を実装できるので単一の vptr では足りず、「型→trait→vtable」の 2 段引きに
  なる（実質 Go/Java の itable で、O(1) の添字ではなくなる）。
- 既存の構造体・列挙型の箱と GC を変えずに済み、`:dyn` を書かないコードのコストが変わらない。

vtable のスロット順は `TraitDef::vtable_order`——**継承したメソッドが先頭**、その後に自前の
メソッドが `deftrait` の記述順で並ぶ。継承分を先頭に置くことで、最左スーパトレイトの鎖の上では
`X.vtable_order` が `Y.vtable_order` の接頭辞になり、`:dyn Y` の箱がそのまま `:dyn X` の箱として
通用する。中身はコンパイル済みコードの関数ポインタで、vtable はヒープ値を持たないので GC が
辿るのは箱の `value` だけ。

#### object safety（dyn 化できる条件）

`:dyn T` 型が**実際に作られる地点**でのみ検査する（`deftrait` の時点ではない）。規則はすべて
「そのメソッドは 1 つの vtable スロットに収まる単一の入口を持たない」の言い換え:

1. trait が存在し、メソッドが 1 つ以上あること
2. 各メソッドが `self` を受け手に持つこと（静的メソッドにはディスパッチする値が無い）
3. `Self` が受け手以外に現れないこと（実装ごとに署名が変わる）
4. メソッド自身がジェネリックでも可変長でもないこと

さらに**具象型ごと**に、(a) その trait を実装していること、(b) ヒープ表現を持つこと
（fat box は値を 1 つしか持てない。「trait 全体が dyn 不可」ではなく「その型は入れられない」と
いう粒度）、(c) 関連型の実際の束縛がピンと一致すること。

#### 暗黙の coercion の範囲

具象値が `:dyn Trait` を期待する位置に来たら自動で箱詰めする。挿入箇所は `Sexpr` への暗黙変換と
同じ（期待型の突き合わせ）なので、及ぶ範囲も同じ——関数の実引数・メソッドの引数・コンストラクタの
フィールド・`defvar` の初期値・戻り位置。型注釈の無い `(let ((x obj)) ...)` には期待型が無いので
箱詰めしない。

#### 箱の透過性

trait オブジェクトは**ディスパッチ以外の意味を持たない**:

- `print`/`format` は中身を印字する。
- `eq`/`eql`/`equal`/`equalp` は箱を透かして比べる（箱は暗黙に作られるので、箱の有無で同一性の
  答えが変わってはならない）。
- `Sexpr` に入れるときは箱を外す（S 式データの中に構造体の第二の表現を作らないため）。
- `match` は箱を外してから `Sexpr` の downcast パターンに乗せる。実装型の集合は開いているので
  **非網羅**が正しく、catch-all が要る。

#### コンパイル済みコードとの関係

`:dyn` の呼び出しがネイティブになると、そこから届きうる実装メソッドもネイティブでなければ
ならない（vtable のスロットから関数ポインタを読んで呼ぶため）。これは 3 つの経路で満たす:

1. コンパイル済みコードが箱詰めする場合、その vtable のスロットが呼び出しグラフに載る。
2. コンパイル済みコードがディスパッチする場合、その時点で存在した全実装
   （`Registry::trait_impls` の逆引き）が呼び出しグラフに載る。
3. 後から `impl` が追加された場合は 1・2 のどちらにも漏れるので、インタプリタが箱詰めする時点で
   「この trait をディスパッチするコンパイル済みコードが既にある」なら未コンパイルのスロットを
   その場でコンパイルする。`:dyn` を一切コンパイルしないプログラムでは、箱詰めがコンパイルを
   誘発することはない。

#### スーパトレイトへの upcast（2026-08-01）

`:dyn Sub` の値を `:dyn Super` を要求する位置へ渡せる。条件は (1) 対象が推移的スーパトレイトの
閉包にあること、(2) ピンが鎖に沿って一致すること。双方の `vtable_order` が接頭辞の関係にあるか
どうかは可否ではなく**コスト**を決める——接頭辞なら式に触らない型の差し替えだけで済み、
そうでなければ次の変換が入る。

#### 非最左スーパトレイトへの upcast（2026-08-04）

`D(B,C)` の `:dyn D` を `:dyn C` にする場合、`C` のスロットは 0 始まりではないので、型の
差し替えだけでは呼び出し側が焼いた定数が別のエントリを指してしまう。そこで `DynUpcast` を挿入し、
**同じ具象値の周りに `C` 自身の vtable で箱を作り直す**。

どの表に差し替えるかは値の具象型で決まるが、upcast 地点にはもう具象型が無い。そこで
**箱詰め地点**——具象型と trait の両方が揃う唯一の場所——で推移的スーパトレイト閉包ぶんの
スロット表を用意し、「(vtable id, trait id) → vtable id」を登録しておく。upcast は実行時に
この表を引くだけ（インタプリタは `dyn_upcasts`、コンパイル済みコードは `rt_dyn_upcast`）。
閉包内のスロット表は箱本体の表から**メソッド名で引き写す**ので、箱がディスパッチする表と
食い違いようがない。

#### 対象外（v1）

- **無関係な trait への変換**（`:dyn A` → `:dyn B`、継承関係なし）: 2 本目の vtable が要るが、
  そのとき具象型はもう分からない。具象値から改めて箱詰めすること。
- **`Sexpr` から `:dyn T` への取り出し**（`(the :dyn T p)`）: 「その値がその trait を実装して
  いるか」の実行時判定には全実装型との照合が要る。具象型に `match` すること。

---

## 6. ユーザ定義型（defstruct / defenum / deftype）

- **`defstruct`**: product 型。フィールドは `変数::フィールド名` で読み、
  `(setf 変数::フィールド名 v)` で書き換える。フィールドごとに `pub` を付けられる。ジェネリックな
  定義は `(defstruct name<T> ...)`。値は `make-instance` で作る（§5）。構造体は参照型で、ヒープ上を
  直接変異する。
- **`defenum`**: sum 型。組み込みの `Option<T>`/`Result<T,E>` と同じ内部表現で、`match`/`if-let`
  で分解する。ジェネリックなヘッダの書き方は `defstruct` と同じ `name<T,U>`。
- **`deftype`**: 型の別名。
- メソッドは `impl` で持たせる（§5）。
- 組み込みの型（`Option`/`Result`/`Sexpr`/`Vector`/`HashTable` など）は Rust 側の
  `crates/typelisp-front/src/check/registry.rs` に直接登録されるか、標準ライブラリに typelisp で
  書かれている（`cons-cell<A,B>` など）。

---

## 7. エラー処理（Rust 流）

### 7.1 Result と panic の住み分け
- **回復可能な失敗** → `Result<T, E>`（変種 `(ok v)` と `(err e)`）＋ `match`。
- **回復不能な失敗（バグ・不変条件違反）** → `panic`。
- **`?`/try は導入しない**（Lisp の文法に馴染まないため）。失敗の分岐は `match` で明示する。

### 7.2 `Never` 型（`!`）
- `panic` は特殊形で、戻り型は `!`（ボトム型）。
- `!` は**任意の期待型に適合**する（Rust の coercion に当たる）。分岐の一方で panic しても
  型検査が通る:
  ```lisp
  (defun f ((x i32)) i32
    (if (< x 0) (panic "neg") x))   ; then 枝は ! → i32 に適合
  ```
- `if`/`match` の枝の結合では、`!` の側は結果型を拘束しない（両方 `!` なら `!`）。
- `unreachable`/`todo`/`exit`/`throw`、脱出しない `loop` も `!`。

### 7.3 命名規則 `!`/`?`
- 関数・特殊形の名前に `!` を接尾辞として付けない（CL に倣う）。発散する操作（`panic`/
  `unreachable`/`todo`）も破壊的な操作も `!` なしの名前にする。`set!` のような `!` の付け方は
  Scheme の作法。
- `?` も接尾辞として使わない（Scheme の述語命名）。述語は CL 流の `-p`/`p` 接尾辞
  （`zerop`/`evenp`/`consp`/`alphap`）か、`is-` 前置（`is-some`/`is-ok`）で命名する。
  CL の `some`（列の述語）は `Option` の変種 `some` と同じ名前になり、構成子の解決が関数より先に
  来る（§2.5）ので使えない。`some?` に逃げず、衝突しない別名 **`any`**（Rust の `Iterator::any`）
  を使う。
- `!` という記号自体は `Never` 型の表記（§7.2、例 `(fn (i32) !)`）として構文上の意味を持つ。

### 7.4 エラー型 E（2026-07-25 実装）
**Rust の `std::error::Error` に倣う**——`Error` は**型ではなくトレイト**であり、汎用の
「なんでも入るエラー値」は存在しない。

- **具象エラー型は発生源ごと**: 組み込みの失敗する操作はそれぞれ自分の型を返す
  （`ParseIntError`/`ParseFloatError`/`ReadError`/`EvalError`/`FileError`/`NetError`。
  [registry.rs](../../crates/typelisp-front/src/check/registry.rs) の `builtin_error_defs`）。
- **ユーザ定義型がそのまま E に載る**: `Result<T, MyError>` の `MyError` は `defstruct` でも
  `defenum` でもよい。`E` は最初から任意の型を取れる型引数なので、言語側の追加の機構は要らない
  （`Error` の実装すら必須ではない）。
- **`Error` トレイト**（標準ライブラリ）: `message` と `source`（包んでいる原因。既定は `none`）の
  2 メソッド。組み込みのエラー型もユーザ定義型と同じ `impl` を標準ライブラリに書いてあるだけで、
  特別扱いは無い。
- **複数のエラー型を一様に扱う**: `Result<T, :dyn Error>`（§5.2）。Rust の `Box<dyn Error>` に
  当たる。具象型からの広げ方は `as-dyn-error`。`?` を導入しない方針（§7.1）なので明示的に書く。
- **型とトレイトは 1 つの名前空間を共有する**（Rust と同じ）。同一モジュール内で型とトレイトに
  同じ名前は付けられず（`Checker::check_type_trait_clash`）、型の位置にトレイト名を書けば
  「`Error` はトレイトである、`:dyn Error` と書け」と報告される。

### 7.5 非局所脱出（`catch`/`throw`/`unwind-protect`、2026-08-16 実装）

**失敗の表現と制御の移動を分ける。** 前者は値（`Result`/`Option`/`Error` トレイト、§7.1〜7.4）、
後者は CL 流の非局所脱出。`catch`/`throw`/`unwind-protect` は CL でも条件システムの一部ではなく、
非局所脱出のプリミティブである。

**コンディション（`define-condition`/`handler-bind`/`invoke-restart`）は採用しない**（§9）。

- **型はタグ（シンボル）が運ぶ**。`throw` と `catch` は動的に対応するので（別関数の奥から飛ぶ）、
  両者が共有するのはシンボルしかない。checker がシンボル→型の表を持ち（`throw_tags`）、最初の
  使用で型が決まり、以降の同じシンボルは全部それに突き合わせる。
- **タグはリテラルのシンボルに限る**。計算されたタグでは型を検査する対象が無くなる。
- **`throw` の型は `!`**。`(catch 'sym expr)` の型は expr の型とタグの型の join。
- **`unwind-protect` の cleanup は `protected` をどう抜けても走る**——正常終了・throw・panic に
  加えて、静的な脱出（`break`/`return`/`return-from`）でも走る。cleanup 自身の非局所脱出が、
  飛行中の脱出に勝つ（CLHS どおり）。

#### 静的な脱出と動的な脱出は混ぜない

| | 静的な脱出 | 動的な脱出 |
|---|---|---|
| 構文 | `break` / `return` / `return-from` | `throw` / `panic` |
| 行き先 | checker が決定済み（`break`/`return` は最内のループ、`return-from` は名前で解決した `block`。どちらも関数の境界を越えない） | 実行時にタグで決まり、関数をまたぐ |
| コンパイル済みコード | 分岐命令 | unwind |

`block`/`return-from`（2026-09-04）は**静的な側**に入れた。意味論だけを見れば `block`→`catch`、
`return-from`→`throw` に落としても等価だが、行き先が実行時に決まらないものを実行時に決める機構に
載せると、この表の区別が意味を失う。

同じ理由で、コンパイル済みコードで `catch` の本体を別関数に切り出す案は却下した——本体が別関数に
なると `(loop ... (catch 'a (break)) ...)` の `break` が同じ関数内の分岐でなくなり、静的な脱出が
動的な機構に巻き込まれる。`unwind-protect` の cleanup を静的な脱出でも走らせるのも、行き先が
静的に分かる以上は走らせる cleanup の並びも静的に分かるので、「最内の cleanup への分岐」の鎖で
済む（タグを見る dispatch ではない）。

### 7.6 部分関数の失敗方針（Rust 流の混在）
| 操作 | 方針 |
|---|---|
| `Vector` の `get`/`set`（範囲外） | panic |
| `HashTable` の `get`、`Vector` の `pop`（空） | `Option<T>` |
| `/` `mod` `rem`（ゼロ除算） | panic |
| `parse-int` / `parse-float` | `Result<_, ParseIntError>` / `Result<_, ParseFloatError>` |
| `read` | `Result<Option<Sexpr>, ReadError>` |
| `unwrap`（`none`/`err`） | panic |

原則: プログラマの誤り＝panic、予期される失敗＝`Result`、無いことが普通にある＝`Option`。
個々の操作の方針は [functions/](../ja/reference/functions/README.md) の各表にある。

---

## 8. 当面の範囲外（将来課題）

意図的に受け入れている制限と残っている作業は [TODO.md](TODO.md) にまとめる（ここには持たない）。
`:dyn` の v1 の対象外は §5.2 末尾。

## 9. 採用しないと決めた機能（非採用）

§8 が「まだ無いもの（将来やるかもしれないもの）」であるのに対し、ここは**やらないと確定した
もの**。CL に同名の機能があることを理由に再検討する場合は、まず下記の判断根拠と衝突しないかを
確認すること。2026-07-29 に [TODO.md](TODO.md) から移設した（TODO ではないため）。

- **`Sexpr` への `Iter<Item>` trait 実装**: 要素型が固定されないリストにジェネリックな
  `Iter<Item>` を被せるのは型システム上不適切というユーザー判断。一度実装したが撤回済み。
  S 式のリストを走査する手段としては **`dolist` マクロが別途ある**。`Iter` トレイトを介さず
  `sexpr-car`/`sexpr-cdr` で直接歩いて各要素を束縛する（要素は S 式のまま。使う側が `match` で
  分解する）ので、この方針と両立している。
- **`?`/`try` 構文**、および `!`/`?` の命名接尾辞: 非採用（§7.1、§7.3）。
- **コンディション（`define-condition` / `handler-bind` / `handler-case` / `invoke-restart`）**:
  2026-08-16 に非採用と確定（§7.5）。「この関数が何を signal しうるか」が型に出ない機構であり、
  静的型付けと対立する。CL が持てるのは動的型付けだからで、静的に例外をやった唯一の主流例
  である Java の checked exception は広く失敗と評価されている。失敗の**表現**は値
  （`Result`/`Option`/`Error` トレイト）に、制御の**移動**は `catch`/`throw`/`unwind-protect` に
  一本化する。失うのは restart（`handler-bind` + `invoke-restart`）だけ。
- **`set-pprint-dispatch` / `*print-pprint-dispatch*`**: CL の「型指定子をキーにした実行時の
  整形関数登録表」。キーもプリンタのシグネチャも無検査で、「登録時点で分かっていた型を
  捨ててから `match` で復元する」形になり、静的型付け言語には合わない——CL のもう一方の機構
  である CLOS 総称関数 `print-object` に相当する **`print-object` トレイト**を 2026-07-26 に
  採用してこちらを置き換えた（[printing.md](../ja/reference/functions/printing.md) §5）。
- **ワイルドカードパス名・論理パス名（`logical-pathname`）、およびパス名のホスト/デバイス/
  バージョン成分**: 2026-08-05 に採用しないと確定（[streams-files.md](../ja/reference/functions/streams-files.md) §9）。
  いずれも CL が対応した「複数のファイルシステム世代」のための機能で、この処理系が走る環境には
  対応物が無い。
- **`input-stream-p` / `output-stream-p` / `stream-element-type`**: ストリームの方向も要素型も
  **型が持つ**（`CharInput`/`CharOutput` はトレイト）ので、実行時に尋ねる問いにならない
  （[streams-files.md](../ja/reference/functions/streams-files.md) §10）。
- **`push` の引数を CL の順にするマクロ**: 2026-10-31 に不採用。`push` は受け手（列）が先で
  `(push v x)`。順番だけ合わせても CL の `push`（場所を書き換えるマクロ）にはならない。

CL 全体と突き合わせた「無いもの」の網羅リストは
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) にある。
