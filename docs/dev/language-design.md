# typelisp 言語設計（確定仕様）

最終更新: 2026-08-05 / ブランチ: `main`

このドキュメントは、設計で**確定した言語仕様**を後から見返せるよう記録するもの。
現在の残作業は [TODO.md](TODO.md)、完了した実装の経緯は [implementation-log.md](implementation-log.md) を参照。

---

## 0. 言語の基本方針

- **静的型付け**: すべての式が静的型を持つ。動的タグ付き Lisp（旧 3ab5599 の `Object`）ではない。
- **文法は macro-lisp 準拠**: `/Users/suzukijun/Program/Rust/macro-lisp` の**構文**に従う（実装は参考にしない）。
  `defun`/`defvar`/`defconstant`/`let`/`if`/`when`/`unless`/`cond`/`match`/`loop`/`while`/`dotimes`/`do`/`doiter`/`while-let`/`if-let`/`lambda`/`progn`/`module` 等。関数の引数・戻り型とグローバル（`defvar`/`defconstant`）の型は必須、局所束縛（`let`）のみ型推論可。
  （`defstruct`（ユーザ定義product型）は初回実装がフィールドの読み書き手段を欠く不完全な設計と
  判明し2026-06-23に一度全面削除、2026-06-24にフィールドアクセサ（`変数::フィールド名`/
  `(setf 変数::フィールド名 v)`）付きで再設計・再実装済み。`defenum`（ユーザ定義直和型）も
  2026-07-09に追加済み——詳細は §6。）
- **真偽値は `true`/`false`**。`nil`/`t` は言語に存在しない。
- **`nil` の代替は `Option<T>`**（組み込みの直和型——`Some(value T)`/`None` の2構成子）。
- **`read` の戻り値は組み込み直和型 `Sexpr`**。
  `Sexpr = Nil|Int|Float|Char|Bool|Sym|Str|Cons(Sexpr, Sexpr)`。
  空リスト `()` は `Sexpr` の値としての `Nil`（cons と並ぶ第一級の構成子）。car/cdr はどちらも `Sexpr`
  （`Option` で包まない）。`Sexpr` 専用の cons 操作は `sexpr-cons`/`sexpr-car`/`sexpr-cdr`/
  `sexpr-consp`/`sexpr-null`/`sexpr-atom`（内部 island 層、§4.1）。裸の `cons`/`car`/`cdr` は
  Symbol/Sexpr 再設計 Phase 4b で汎用 `cons-cell<A,B>`（`defstruct`）のフィールドアクセサへ
  付け替えられており、`Sexpr` 専用ではない。`list` は `Sexpr` の `Cons`/`Nil` へ脱糖する。
  `dolist`（`(dolist (var list-form [result-form]) body...)`）は cons セル（`Sexpr`）
  リスト専用の反復——各要素は異種の `Sexpr` なので body 側で `match` により形状分岐する
  （`doiter` のような単一 `Item` 型ではない）。`match` を融合せず反復と分岐を直交させる方針
  （body は素の `progn`）。`prelude.rs` の `defmacro`（`let`+`while`+`sexpr-*` で `Cons` を辿る、
  §3 末尾 `doiter` と同型の薄いマクロ）。
  `()` は期待型が `Sexpr` のとき `Nil` に、`Option<T>` のとき `None` になる（`Unit` はその他の文脈）。
  実行時は `Nil` を `Value::Empty` で符号化（`Cons` と対等な variant、`Option` ラッパーではない）。
  これは言語レベルで禁止した「真偽値としての `nil`」とは別物（あくまで read データ内の空リスト表現）。
- **`Symbol` は `Sexpr` とは別の独立したプリミティブ型**（`Sexpr` の `Sym` 構成子とは別物）。
  `Sexpr` が要求される文脈へは暗黙変換されるが、逆方向（`Sexpr`→`Symbol`）の自動変換はない。
  `gensym`/`symbol->string`/`string->symbol`（§4.1）はこの `Symbol` 型を使う。
- **`Sexpr` はユーザ定義 ADT インスタンスも保持できる**（2026-07-24、CL の cons が任意のオブジェクトを
  保持できる仕様——`(list (make-point ..) 42)` は正当で `#S(POINT :X 1 :Y 2)` と印字される——へ
  合わせる決定。プリティプリンタ設計議論（TODO T5）中に確定）。対象はヒープ表現（`is_heap_repr`）の
  登録済み ADT——`defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>`/`cons-cell<K,V>`/`Option`/`Result`。
  実行時表現は既に統一済み（`RtValue::Sexpr(Value::Boxed(_))`）なので挿入は透明な retype（実行時
  コストゼロ）。スカラ（`i32`/`f64`/`bignum`/`ratio`/`char`/`bool`/`string`）も `Symbol` 同様に暗黙
  `Sexpr` 化されるが、こちらは実表現が異なるため対応する `Sexpr` コンストラクタで実際にラップされる
  （`&rest`/`format` 引数と同じ変換）。ネイティブ表現のジェネリック実体化（`Option<llvm-value>` 等）
  は対象外（`Sexpr` の表現を持たないため従来通り型エラー）。`match` 側は `Sexpr` スクルーティニーに
  対する **downcast パターン**（型名先頭のフィールド分解 `(point x y)`／裸または修飾の enum 変種名
  `(red)`/`(color::red)`／丸ごと束縛 `(the point p)`）で取り出す——構文の詳細は
  [syntax.md](../syntax.md) の `match` 節。ランタイムテストは boxed オブジェクトの `type_name` 文字列
  比較（+ enum は variant index）で、downcast パターンは `Sexpr` 本来の11変種の網羅性カバレッジには
  数えない。`equal` は CL 同様 struct/enum に対し同一性（`eq`）のまま、`equalp` はスロットごとの
  再帰比較に拡張。
- **大文字小文字は区別しない**（シンボルは小文字に正規化してインターン）。
- **Rust 相互運用はしない**（`&args[1]`, `env::args().collect()` 等は対象外）。
- **構成子パターンは S 式形** `(Some v)` / `(Cons a d)`。
- **実行モデル**: インタプリタ（eval）が基本。加えて明示的なネイティブコンパイル（`compile`/
  `compile-file`、LLVM ベース、`src/compile/`）も実装済み——`compile` は JIT（呼び出し前に
  呼び出し先も `compile` 済みである必要がある）、`compile-file` は独立した AOT 実行（専用の
  新規 `Heap`/`Checker`/`Interp` で 1 ファイルを読み、`cc` を介してネイティブ実行ファイルへ
  リンクする）。旧実装は 2026-06-23 に `Vector<T>`/`defstruct` の全面リバートに伴って一度
  削除されたが、2026-06-25 以降 Vector/defstruct 再設計後の前提の上で再実装されている
  （対応構文の範囲は都度拡張中——詳細は `src/compile/`、[implementation-log.md](implementation-log.md) 参照）。
- **ファイル拡張子**: ソースファイルは `.typl` のみ。コンパイル済みモジュール形式は無い
  （`.fastl` は 2026-08-14 に削除——§2.4）。LLVM の `compile-file` は `cc` でリンクした
  **ネイティブ実行ファイル**を直接出力するもので、中間ファイルは残さない。`.typlc` という
  拡張子は実装上使用されていない。
- **命名規則 `!`/`?`**: 関数名の末尾に `!`（破壊的操作）や `?`（述語）を接尾辞として使わない（詳細・理由は §7.3）。

---

## 1. メモリモデル / GC

- cons セルは**固定アリーナ**（起動時に確保、再確保しない＝生ポインタが安定）。既定は 65536（`1 << 16`）セルで、`typl --heap-cells N` で起動時に容量を指定できる（ファイル実行/REPL 共通のグローバルフラグ、`--heap-cells=N` 形も可。`main.rs` の `parse_heap_cells`）。
- 割当はフリーリストから。空なら GC、それでも空なら **`Error::HeapExhausted`（成長しない）**。
- **mark-sweep GC**（反復マーク＝深い構造でもスタック溢れなし、循環回収）。ルート集合 `push_root`/`pop_root`。
- **生ポインタは `ConsRef` に隠蔽、公開 API は安全**。
- シンボルはインターン（小文字正規化・永続）。文字列は GC 管理（到達可能のみ生存）。
- **実行時 `Sexpr` 値も同じヒープ**（`RtValue::Sexpr`）: read 時のデータと eval 中にプログラムが `cons`/`list`/構成子で
  作るデータは同一の cons アリーナ・GC を共有する。Symbol/Sexpr 再設計以降、`defstruct` インスタンス/
  `Vector<T>`/`cons-cell<K,V>`/クロージャも同じ `RtValue::Sexpr` 経由（`Value::Boxed` の各 `BoxedObj`
  variant）でこの GC ヒープに乗る（専用の `RtValue::Struct`/`RtValue::Closure` は無い）。`Option<T>`/
  `Result<T,E>` のような小さな組み込み直和型のみ `RtValue::Data` として Rust ヒープ上＝GC 対象外のまま。
  インタプリタ側のルート管理: 生成した可変スロット（`let`/引数/クロージャ捕捉/`match` 束縛）をすべて弱参照で
  `Interp.slots` に登録し、`heap.cons` 呼び出し直前に `sync_roots` で「今生きているスロットが持つ `Sexpr` 値」を
  再収集してヒープのルート集合を差し替える。スロットの生存は通常の `Rc` 所有権（env フレーム/`globals`/クロージャの
  捕捉環境）に委ねており、専用の push/pop 管理は不要。

---

## 2. `::`・module・型の関係（Rust 流）

### 2.1 `::` は reader が `Value::Path` に分割
- ソース上の `Foo::Bar` は **読み取り時に** セグメント列へ分割され、専用の `Value::Path([sym...])` になる。
  解決時に文字列を再分割しない（効率・見通しのため reader 段で構造化する）。
- reader が行うのは**明示 `::` の分割のみ**。CL の `*package*` のような「現在の名前空間」追跡はしない。
  裸名のスコープ解決・各セグメントが module か型かの判定は、すべて型検査器（checker）が行う。
- **Sexpr コアの拡張**: 読み取り結果のデータ型 `Sexpr` に `Path` を追加する。
  ```
  Sexpr = Nil | Int | Float | Char | Bool | Sym | Str
        | Cons(Sexpr, Sexpr)
        | Path([Sym, ...])          ; 例 'std::process::exit
  ```
  （`Nil`/`Cons(Sexpr, Sexpr)`/`Path` いずれも実装済み——`Value::Path`は`crates/typelisp-mem/src/heap.rs`
  の`PathId`でインターンされる。）
- セグメントは小文字化される（シンボルと同じ正規化）。空セグメント（`foo::`, `::bar`, `a::::b`）は読み取りエラー。
- 総称は最終セグメントに付く（`a::Vec<T>` → セグメント `[a, vec<t>]`、型は最終セグメントの `<>` を解釈）。

### 2.2 module は名前空間、型は名前空間ではない
- **module = 名前空間**（`module`/`use` で扱う）。
- **型は Rust 同様**に扱う ＝ 型は **関連関数（associated function）/メソッド** を持つ。型は名前空間ではない。
- したがって `Foo::Bar` の意味は文脈で決まる（Rust のパス解決と同じ）:
  - `Foo` が **module** なら「module Foo の項目 Bar」。
  - `Foo` が **型** なら「型 Foo の関連関数/メソッド Bar」。
  - checker が先頭セグメントを解決し、module か型かを判定して残りを下降解決する。

### 2.3 ファイル↔モジュール対応（2026-07-11 実装）

- **ファイルパス＝モジュールPath**: ソースルートからの相対ファイルパスがそのまま `Path` の
  セグメント列（`<root>/geo/point.typl` → `geo::point`、ディレクトリも1セグメント、小文字化）。
  ファイルの内容は暗黙にそのモジュールに包まれ、明示 `(module bar ...)` はその内側にネスト
  （`geo::point::bar`）——ファイル境界と `module` 宣言を調停する整合性ルールは不要（衝突しようがない）。
- **ソースルート**: マニフェスト `typelisp.toml`（空でよい、任意キーは `src = "dir"` のみ・
  手書きパース）を上方探索。無ければエントリファイルのディレクトリ（REPLはcwd）。
- **オンデマンドロード**（`src/project.rs` の `Loader`）: ファイルのチェック前にトップレベルの
  `(use ...)`（`(module ...)` 内も含む）を走査し、未ロードの対応ファイルを再帰的に
  読み込み→チェック→同一 `Registry` へ登録。**走査先行方式**（チェック中の
  `Error::ModuleNotLoaded` を捕捉してリトライする方式ではない）なので同じフォームが
  二度チェックされることがなく、ロードが偽の再定義警告を出さない。
- **マクロ展開が生成する `use`**（2026-07-15 実装）: トップレベルのマクロ呼び出しは
  チェックループがフォームごとに事前展開し（`Checker::try_expand_toplevel_macro`）、
  展開結果を走査して依存をロードした上で展開結果自体をトップレベル形としてチェックする
  （`Checker::check_form_dispatch` もマクロ展開を再ディスパッチするため、展開結果が
  `use`/`defun`/`module` 等の定義形でも正しく処理される。展開は一度きり）。マクロは
  定義（`defmacro` のチェック＋即時exec）より後でしか呼べないため、事前の一括走査では
  原理的に展開できない——これがフォーム単位の事前展開にした理由。チェック途中の依存
  ロードは名前空間コンテキストを一時退避して行う（`Checker::suspend_ns_context`——
  依存モジュールが現在ファイルのモジュール配下にネスト登録されるのを防ぐ）。
  式位置に `use`/`module` が現れた場合は専用エラー（トップレベル専用である旨）。
  マクロの呼び出し可能タイミング自体の制約は§3の`defmacro`仕様を参照。
  テスト: `tests/macro_use_test.rs`。
- **モジュール越しのマクロ呼び出し**（2026-07-15 実装）: `resolve_macro_path`（`resolve_fn_path`の
  マクロ版、同じ可視性規則）を新設し、`mod::macro-name`という`::`修飾呼び出しを式位置
  （`check_path_call`）・トップレベル（`try_expand_toplevel_macro`）の両方で解決する。
  上記のマクロ生成`use`と組み合わせ可能（例: `lib::get-answer`というマクロが`'(dep::answer)`を
  生成するなら、`dep`は自動でロードされる）。
- **quoted data内の`::`パス**（2026-07-15 実装）: `QuotedSexpr::Path(Vec<String>)`を新設
  （`Sexpr`組み込みADTに`Path`バリアントを追加するのではなく、`Sym`同様チェッカー内部の
  ミラー表現として）。`(quote (dep::head))`や`` `(dep::head) ``（マクロ本体で他モジュールの
  関数を指す典型パターン）が型チェックを通るようになった。`value_to_quoted`
  （`src/check/checker.rs`）↔`alloc_quoted`（`src/eval/interp.rs`、`heap.intern_path`で復元）
  が対。~~compile（LLVM JIT/AOT）は非対応のまま（`Sym`/`Bignum`/`Ratio`と同じグループで
  `unsupported`）~~ **→ 2026-07-15 `Sym`/`Path`ともcompile対応**（`rt_intern_symbol`/
  `rt_intern_path`を新設、`str`リテラルと同じ「タグ付き`Sexpr`をrt呼び出しで構築」方式、
  `ast_bridge.rs::translate_quote`）。`FASL_FORMAT_VERSION`を3へbump（新バリアント追加のため）。
  テスト: `tests/macro_use_test.rs`、`tests/compile_test.rs`。
- **循環参照は明示エラー**（`circular module dependency: a -> b -> a`）: checkerは単一パスで
  Cのヘッダのような宣言/定義分離が無いため、サイレントスキップは後段の紛らわしい
  「no such function」になる。真の相互参照サポート（2段階チェック化）はスコープ外。
- **GCルート規律**: 各ファイルの `read_all` ルートはそのファイルのチェック完了時にpop
  （ネストロードはLIFO）。チェック済みフォームは実行キューに積み、全ルートpop後に
  依存順で一括exec（`defmacro` 登録のみ即時——rootスタックに触れない純粋な登録のため）。
  cons位置テーブルはロードセッション開始時に一度だけクリア
  （`Reader::read_all_in_keep_locs`——依存ファイルの読み込みが外側ファイルの位置情報を
  消さないため）。
- **実行**: `typl <file.typl>`（引数なしはREPL）。REPL の `use` も同じLoaderで解決。
  LSP（`typl-lsp`）も同じLoaderでクロスファイル診断を行う。

### 2.4 `(load)`

- **`(load "path")`**: CL流のフラットロード（対象ファイルのフォームをカレント名前空間に読み込む、
  `use` のモジュール包みとは別）。トップレベル専用。`path` は読み込み元ファイルのディレクトリ
  からの相対で、拡張子が無ければ `.typl` を補う。読み込んだファイル自身の `(load)`/`(use)` も
  再帰的に処理される。
- **コンパイル済みモジュール形式は無い**。2026-07-14 に fasl（チェック済み状態のシリアライズ、
  `.fastl`、`typl compile-module` が生成）を入れたが、2026-08-14 に削除した——ネイティブコードでは
  なく read+型チェックを飛ばすだけの機構で、実行を1ミリ秒も速くしないため。経緯と実測値は
  [implementation-log.md](implementation-log.md)。
- **prelude** も毎回ソースから読んで型チェックする（`prelude::load`）。LSP の各診断パスも同じ。

### 2.4 名前解決規則
- **裸名（修飾なし）**: 現在の module → root（組み込み）の順。**中間の親 module は歩かない**。
  曖昧（複数候補）または未発見は `TypeError`。
- **修飾パス `a::b::...`**: 先頭を現NS→root で解決し、module なら下降、型なら次（最終）セグメントを関連項目として解決。
  MVP で対応する形は `module*::item` と `[module*::]Type::assoc`。
- **裸 head（リスト先頭がシンボル）の優先順位**:
  1. 特殊形（`if`/`let`/`match`/`defun` …）
  2. 構成子（`Some`/`Ok`/ユーザ定義 ctor …）
  3. 自由関数（現NS→root）
  4. **インスタンスメソッド・ディスパッチ**（第一引数の静的型 `T` の `T` 関連メソッドを探す）
- インスタンスメソッド呼び出し `(m recv args...)` は recv の静的型からメソッドを解決（単一・静的ディスパッチ）。

---

## 3. 特殊形カタログ

| 分類 | 特殊形 | 備考 |
|---|---|---|
| 定義 | `defun` `defvar` `defconstant` `defmethod` `defmacro` `defstruct` `defenum` `module` `use` `lambda` `deftrait` `impl` | 引数・戻り型・グローバルの型は明示（`defvar`/`defconstant`は`(defvar (name Type) value)`で型必須、2026-07-03に型なし形式を削除。局所束縛`let`のみ推論可。`defmacro` は全パラメータ・戻りが `Sexpr` 固定なので型注釈なし、末尾 `&rest name` で可変長対応）。ジェネリック定義形（`defun`/`defstruct`/`defenum`）の型パラメータは名前に山括弧で書く（`name<T,U>`）。`defstruct`（ユーザ定義product型）は再設計後に実装済み。`defenum`（ユーザ定義直和型、`match`/`if-let`対応）を追加。`deftrait`/`impl`（trait機構、§5.1）は2026-06-30実装、2026-08-01にスーパトレイト・デフォルトメソッド本体・ブランケット実装（`impl<T>`、型パラメータを持つ唯一の**ヘッダ**）を追加 |
| 束縛 | `let` `let*` | |
| 制御 | `if` `when` `unless` `cond` `case` `match` `if-let` `while-let` `and` `or` `progn` `the` | `and`/`or` は短絡のため特殊形。`the` は型注釈 |
| 反復 | `loop` `while` `until` `dotimes` `do` `doiter` `dolist` | `doiter` は `Iter` トレイト経由で任意コレクション。`dolist` は cons セル（`Sexpr`）リスト専用で body 側 `match` により要素形状で分岐 |
| マクロ/引用 | `quote` `quasiquote` (`` ` ``) `unquote` (`,`) `,@` | `quote`/`quasiquote` の戻り型は常に `Sexpr`。`,@`（unquote-splicing）は2026-06-19実装済み |
| その他 | `setf` `break` `return` `panic` `unreachable` `todo` | `break`/`return`/`panic`/`unreachable`/`todo` は戻り型 `!`（§7） |

脱糖の例: `when`→`if`+`progn`、`unless`→`if`、`if-let (pat val) then else`→2 腕 `match`（包括アームで網羅）、
`quasiquote`→`Expr::Quote`+`Expr::Construct{Cons,..}` の組合せ（`list` の脱糖と同様、ランタイムマクロ機構は使わない）。

実装状況: `if` `let` `let*` `progn` `when` `unless` `and` `or` `cond` `case` `setf` `while` `until` `loop` `break`
`return` `lambda` `match` `if-let` `while-let` `do` `doiter` `the` `panic` `defvar` `defconstant` `module` `use`
`defmethod` `deftrait` `impl` `quote` `quasiquote` `defmacro` `defstruct` `defenum`
は実装済（[src/check/checker.rs](../src/check/checker.rs)。`the`のみchecker特殊形、
`case`/`until`/`while-let`/`do`/`doiter`は`prelude.rs`の`defmacro`）。`unreachable`/`todo`/`exit`（§4.1/§7）も実装済
（前2つは`panic`を呼ぶ`defmacro`、`exit`は`std::process::exit`を呼ぶRust組み込み自由関数）。
`doiter`も実装済（2026-06-30、`prelude.rs`の`defmacro`——`var`の型はマクロ展開時には分からないが、
展開先の`(some var)`のような構成子パターンの型を`Checker::check_ctor_pattern`がscrutinee（`next`の
戻り値`Option<Item>`）から自動推論するため、checker特殊形にする必要はない。`while-let`を呼ぶだけの
薄い`defmacro`、§5.1のtrait機構参照）。`defmacro` は CL 流（非衛生的）— 詳細は
[implementation-log.md](implementation-log.md) のステップ 4k を参照。`defstruct`/`defenum`は§6参照。

**仕様: マクロは定義（`defmacro`のチェック）より後のフォームでのみ呼び出せる**（同一ファイル内
の前方参照は不可）。`defmacro`自体の登録（マクロ展開器`Interp::fns`への登録）はそのフォームを
**exec**した副作用として起きるため、原理的にそれより前のフォームからは呼び出せない——依存ロード・
`use`生成マクロ（下記）を含め全マクロ呼び出しに共通するルール。

なお **`defun` は前方参照可**（相互再帰も可）。各ローダがファイル全体を読んだ直後に全トップ
レベル`defun`のシグネチャだけを先行登録する軽いパス（`Checker::predeclare_program`）を通すため。
マクロがこれに乗れないのは上記のとおり「登録では足りず実行済みである必要がある」から、型
（`defstruct`等）が乗れないのは「型の登録は、型を登録するコード自身が必要とするもの」でシグネチャ
のように自己完結しないから。詳細と例外は[syntax.md](../syntax.md)の`defun`節を参照。
`mod::macro-name`という`::`修飾呼び出しでモジュール越しにマクロを呼ぶこともできる（`use`のエイリ
アス経由ではなく常にフルパス、`pub defmacro`でなければ定義モジュール外から不可視——`defun`の
`mod::fn`呼び出しと同じ可視性規則）。
`when`/`unless`/`and`/`or`/`cond`/`let*` は `if`/`let` への脱糖。`setf`（可変ローカル/グローバル変数）/`while` は専用 AST
ノード（eval 環境は `Rc<RefCell>` の可変スロット）。`defvar`（可変）/`defconstant`（不変）はグローバル変数を現在の
名前空間に登録する。型注釈は**必須**——`(defvar (name Type) value)`（グローバルの型はプログラムの
公開サーフェスであり初期化子から推論しない。型なしの`(defvar name value)`形式は2026-07-03に削除。
副次的に、注釈が初期化子のexpected型になるため`(defvar (f (fn (i32) i32)) identity)`のような
ジェネリック関数値の単型化解決もここから効く）。
**`lambda`** は関数を第一級の値（GCヒープ上のクロージャボックス）にする: `(lambda (params) ret body...)`、型は `(fn ...)`。
定義時の環境（可変スロット）を捕捉する真のクロージャ。関数値の呼び出しは頭がローカル変数・グローバル変数・任意の式
（例 `((lambda ...) x)`）のとき `Expr::Apply` に。**名前付き関数も値化可能**（`id` 等を高階関数へ渡せる。`Expr::FnRef`、
組み込みは `RtValue::Builtin`）。**クロージャ表現統一**（`docs/dev/implementation-log.md`「クロージャ表現統一」節、2026-07-18 Stage 1-10 完了、
続く「interpクロージャ完全削除」節、2026-07-19 Stage 1-8c 完了）により、`lambda`/`labels` の各兄弟/`FnRef`/
`MethodRef` は評価（定義）時に必ず自己ホストコンパイラで JIT され、`BoxedObj::CompiledClosure`（コンパイル済み
コードと表現を共有する GC ヒープ値）として構築される——tree-walk インタプリタが実行するクロージャ表現
（`BoxedObj::Closure`/`make_closure` 等）は物理削除済みで、フォールバック先そのものが存在しない。自己ホスト
コンパイラ島自体もビルド時に AOT コンパイルされ実行時は常時ネイティブロードされるため、定義時 JIT が
失敗する経路（native tier 型の捕獲/引数/戻り値、コンパイラ島未ロード）は通常到達不能な異常系であり、
到達すれば `EvalError::Panic`（`definition-time JIT failed: ...`）になる。捕獲変数への `setf` は共有セル
（CL 的、書き換えは同一セルを指す全クロージャから見える）——値のスナップショットではない。**`dotimes`** `(dotimes (var count) body...)` は `let`+`while`+`setf` への脱糖。
**`list`** `(list e1 ... en)` は `(Cons e1 (Cons e2 (... (Nil))))` への脱糖（`(list)` は `(Nil)`）。**`dolist`**
`(dolist (var list-form [result-form]) body...)` は cons セル（`Sexpr`）リスト専用の反復——
`var` を各要素（`sexpr-car`、型は `Sexpr`）に束縛し `body` を実行、`cdr` が cons でなくなったら停止する
（不完全/ドットリストはエラーにせず途中で止まる）。各要素は異種の `Sexpr` なので `body` 側で `match`
により形状分岐する（`doiter` の単一 `Item` 型とは対照的）——`dolist` は `match` を融合せず反復と
分岐を直交させる（`body` は素の `progn`）。任意の `result-form` が全体の値（既定 `()`/`Unit`）で、CL と違い
`var` のスコープ外で評価される。`prelude.rs` の `defmacro`（`dotimes`/`doiter` と同型、`gensym` で
`list-form` を一度だけ評価）。
**`loop`** `(loop body...)` は無限ループ。**`break`/`return`** は CL 流：どちらも**直近のループのみ**を脱出する
（関数の早期 return ではない。`lambda` 境界は越えられない＝クロージャの中から外側のループへ break/return できない）。
`break` は値を取らず（常に `Unit` で脱出）、`return` は `(return)`／`(return value)` で値任意。`while`/`dotimes`/
`doiter`/`loop` いずれの内側でも使え、`loop` の型は内側で見つかった `break`/`return` の値型の join（`match`/`cond` の
腕と同様に一致が必要）。一度も脱出しない `loop` は型 `!`（Rust の `loop {}` と同じ）。`while` 系はもともと型が `Unit`
固定なので、その内側の `return` の値も `Unit` でなければ型エラー。
**`doiter`** `(doiter (var coll) body...)` は `Iter` トレイト（§5.1）を実装した値を反復する——
`coll` の `next` を `Item` が尽きるまで呼び、`var` に束縛して `body` を実行する。`dotimes`/`dolist`
と同じ「`gensym` で `coll` を一度だけ評価する隠しbinding」パターンで `while-let` を呼ぶだけの
`defmacro`（`prelude.rs`）: `` `(let ((,tmp ,coll)) (while-let ((some ,var) (next ,tmp)) ,@body)) ``。
`var` の型はマクロ展開時には分からないが、展開後の `(some var)` という構成子パターンの型を
`Checker::check_ctor_pattern` がscrutinee（`next` の戻り値 `Option<Item>`）から自動推論するので、
`case`/`do`/`while-let` 同様マクロのみで書け、checker特殊形は不要（`while-let` のドキュメント
コメントが明記する「`val` は毎回再評価される（`(next i)` のような状態変化観察のため）」という
性質をそのまま利用している）。

---

## 4. 関数カタログ（Rust 組み込み vs typelisp ライブラリ）

**分離原則**: ヒープ/ランタイム/IO/プリミティブ演算/ネイティブ codegen を要するものは **Rust 実装**。
それらの組合せで書けるものは **typelisp 自身で実装**（ライブラリ）。すべて型付き（引数/戻り型を明示）。

> 実装状況: 本節（§4）のカタログはほぼ全項目が実装済み——i8/i16/i32/i64/f32/f64/bignum/ratio の
> 算術・比較、`Sexpr`/`Symbol`/`char`/`string` 操作、`Option`/`Result` ヘルパー、`HashTable<K,V>`/
> `Vector<T>` の関連メソッド、`compile`/`compile-file` まで含む。個別の未実装項目は
> [functions.md](../functions.md) の該当箇所に明記。`Sexpr` の実行時値は §1 のとおり cons ヒープ
> （GC 管理）に統合済み。

### 4.1 Rust 組み込み（primitive）
| 種別 | 関数 | 備考 / 例 |
|---|---|---|
| 算術 | `+ - * / mod rem neg abs` | 型ごと。例 `+ : (fn (i32 i32) i32)`。`/` のゼロ除算は `Result` |
| 比較 | `= /= < <= > >=` | |
| 論理 | `not` | `and`/`or` は短絡で特殊形 |
| cons | `cons car cdr consp atom eq` | Symbol/Sexpr 再設計 Phase 4b で汎用 `cons-cell<A,B>`（`defstruct`）用に付け替え済み、`Sexpr` 専用ではない。`set-car`/`set-cdr` は完全撤去済み。`Sexpr` 専用操作は `sexpr-cons`/`sexpr-car`/`sexpr-cdr`/`sexpr-consp`/`sexpr-null`/`sexpr-atom`（内部 island 層） |
| 変換 | `int->float float->int char->int int->char symbol->string string->symbol` | `symbol->string`/`string->symbol` は §0 の `Symbol` 型を扱う（`Sexpr` ではない） |
| 文字列 | `string-length string-append string-ref substring string=` | |
| 解析 | `parse-int parse-float` | `Result<_, ParseIntError>` / `Result<_, ParseFloatError>`（§7.4） |
| IO | `print println princ format read read-line` | `read : (fn (String) Result<Sexpr, ReadError>)` |
| 発散 | `panic unreachable todo exit` | 戻り型 `!`（§7） |
| システム | `eval gc compile compile-file` | `compile`/`compile-file` は実装済み（§0、[src/compile/](../src/compile/)） |
| マクロ | `gensym` | 引数なし、フレッシュな `Symbol` を返す。symbol は常に intern される仕様のため衝突耐性のみ（CL の unforgeable な未intern symbol ではない） |

### 4.2 typelisp ライブラリ（derived）
| 種別 | 関数 |
|---|---|
| リスト | `list length append reverse nth last map filter foldl foldr member assoc find every any` |
| Option | `unwrap`(None で panic) `unwrap-or is-some is-none map-option and-then or-else` |
| Result | `is-ok is-err ok-or unwrap-or-else map-result` |
| 高階 | `identity const compose flip apply` |
| 数値補助 | `min max sum product range iota evenp oddp zerop` |

---

## 5. メソッド機構（defmethod）

CLOS の汎関数に相当する独自機構（CLOS とは別物）。**型は Rust 同様に関連項目を持つ**（§2.2）。

- **インスタンスメソッド**:
  ```
  (defmethod m ((self T) (a A) ...) Ret body...)
  ```
  型 `T` の関連項目として登録。呼び出しは通常形 `(m obj a ...)`。checker が `obj` の静的型 `T` から `m` を解決し、
  `self` を `T` で束縛して本体を検査。
- **static / 関連関数**（第一要素が型名シンボル）:
  ```
  (defmethod m (T (a A) ...) Ret body...)
  ```
  型 `T` の関連関数として登録。呼び出しは `(T::m a ...)`（head は `Path([t, m])`）。
- ディスパッチは **既定で静的**（self の静的型で一意に解決）。trait機構（§5.1、2026-06-30実装）も
  この単一静的ディスパッチを拡張する形で構築されている。**動的ディスパッチは `:dyn Trait` 型
  （trait オブジェクト）を明示的に書いたときだけ**行われ、そのときは本物の vtable を経由する
  （§5.2、2026-07-25実装）。「実装しない」という当初の方針からの転換であり、`:dyn` を書かない
  コードの表現・コストは一切変わっていない。

例（`i32` という組み込み型に対する static / instance メソッド。`defstruct` で定義したユーザ定義型
にも同じ構文で `defmethod` を書ける、§6 参照）:
```lisp
(defmethod zero (i32) i32 0)                  ; static → (i32::zero)
(defmethod double ((self i32)) i32 (+ self self))  ; instance → (double 3)
```

### 5.1 trait機構（deftrait / impl / where）

`doiter`（§3末尾）が「Iterトレイトを実装した型すべてで使える」ことを要求したため2026-06-30に導入。
`defmethod`の単一静的ディスパッチをそのまま再利用する設計（trait専用の新しいディスパッチ機構は作らない）。

- **trait定義**: `(deftrait Name (Super...) (type AssocName)... (method-name ((self Self) params...) Ret [body...])...)`。
  `Self`・宣言した関連型名はメソッドシグネチャの中で型変数として使える。
  ```lisp
  (deftrait Iter ()
    (type Item)
    (next ((self Self)) Option<Item>))
  ```
- **スーパトレイト**（2026-08-01追加）: 名前の直後の**必須**リスト。Rust の `trait Ord: Eq` は
  仕様上 `trait Ord where Self: Eq` の糖衣なので、内部表現は `where` 境界と同じ `TraitBound` を
  そのまま使い、型変数スロット（常に `Self`）だけ構文から省いている。要素は素の名前か
  `(Trait (Assoc Type))`。スーパトレイトの関連型は**全てピン留め必須** —— そうしないと
  `:dyn Sub<...>` のピンを鎖に沿って合成して継承メソッドのシグネチャを具体化できず、
  `TraitDef::assoc_types` に継承分を足すことになって既存トレイトの `:dyn` ピン個数が変わる。
  - **義務**: `impl Ord X` は `impl Eq X` が**先に**書かれていることを要求する（`check_supertrait_impls`）。
    直接のスーパトレイトだけ見れば十分（親の impl が祖父を強制済み）。厳密な記述順の規則で
    Rust より制限が強いが、REPL でも逐次 `load` でも決定的に判定できる唯一の形。
    その帰結として `AdtDef::impls` はスーパトレイトについて閉じるので、
    `validate_where_bounds` の平坦な一覧走査は変更不要のまま済む。
  - **同名衝突**: サブが親のメソッドを再宣言すること、2つの親から同名メソッドを継承することは
    どちらもエラー。vtable のスロットは名前ごとに1つで、呼び出し側に曖昧性解消の構文が無いため。
    ダイヤモンドは宣言元が一致するので合流して1スロットになる。
- **デフォルトメソッド本体**（2026-08-01追加）: シグネチャの後ろに本体を書くと `TraitDef::defaults`
  に**項目まるごと**（`OwnedForm` 列）で retain され、`check_impl` が省略メソッドを既存の
  rebuild-and-check ループに流し込む。`AdtDef::assoc` には普通の `AssocFn` が入るので、
  vtable・`Expr::DynCall`・AOT・島はどれも変更不要。本体はトレイトを書いたモジュールの
  名前空間で再チェックする（`check_defmethod_in`）——ヘッダは `impl` 側の名前空間で
  `Self` 置換済みなので、切り替えるのは本体だけ。
- **trait実装**: `(impl TraitName TargetType (type AssocName ConcreteType)... (method-name (recv params...) Ret body...)...)`。
  `Self`/関連型名は`TargetType`/`(type ...)`の具体型へ構文木レベルで置換されてから`defmethod`相当の
  処理に通る——実装後、各メソッドは`TargetType`の通常の`assoc`テーブルに**普通の`defmethod`として**
  挿入される（trait用の別テーブルは持たない）。`TargetType`の`AdtDef.impls`に`TraitName`が記録され、
  以後「型T が trait X を実装しているか」はこの一覧を見るだけで判定できる。
  ```lisp
  (impl Iter vector-iter<T>
    (type Item T)
    (next ((self Self)) Option<T> ...))
  ```
  具体型に対する呼び出し（`(next concrete-vec-iter)`）は、register済みの`assoc`テーブルを引く
  既存の`Expr::Assoc`機構がそのまま動く——trait導入前と挙動・コードパスとも変わらない。
- **ジェネリック関数のtrait境界**: `(defun name<T> (params...) Ret (where (Trait T (Assoc
  Concrete)...)...) body...)`。`(Assoc Concrete)...`は省略可能で、trait の関連型を具体型に
  pinする（2026-06-30追加）——例えば`Iter`の関連型`Item`を`i32`に固定したい場合
  `(where (Iter T (Item i32)))`と書く。本体チェック時にのみ「型変数Tはこのtraitのメソッドを
  呼べ、pinした関連型は具体型として扱える」という情報を与える。本体内で型変数Tの値に対する
  traitメソッド呼び出しは、新設の`Expr::TraitCall`ノードになる——`Expr::Assoc`と違い実装型の
  Pathをチェック時には持たず、実行時にレシーバの値自身が持つ型タグ（`RtValue::Struct`の
  `type_name`等）を読んで`Expr::Assoc`と同じ`methods`テーブルを引く。これが本機構で唯一「型消去後の
  実行時情報」を必要とする箇所だが、参照するテーブル自体はvtable等の専用間接構造ではなく、既存の
  固定`methods`テーブルそのもの。`Expr::TraitCall`の静的な戻り型は、マッチしたboundのpinで
  `subst_apply`してから使う——pin無しなら`Option<Item>`のまま（`Item`未解決）、pin有りなら
  `Option<Item>`が`Option<i32>`に解決され、ループ変数への算術演算等が型チェックを通る。
  ```lisp
  (defun count-iter<T> ((it T)) i32 (where (Iter T))
    (let ((n 0)) (doiter (x it) (setf n (+ n 1))) n))
  (defun sum-iter<T> ((it T)) i32 (where (Iter T (Item i32)))
    (let ((n 0)) (doiter (x it) (setf n (+ n x))) n))  ; x: i32（pin済み）
  ```
  `where`節は**呼び出し側シグネチャにも反映される**（2026-06-30追加、`FnSig.bounds`）——
  `Checker::check_call`が、各boundの型パラメータが実引数からどの具体型に解決されたかを確認し、
  その具体型が実際に`AdtDef.impls`へ要求traitを含むか、pinした関連型が
  （`AdtDef.trait_assoc`経由で解決した）実際の関連型と一致するかを検証する。満たさなければ
  型チェック時点でエラーになる——以前は境界を満たさない型を渡しても型チェックは通り、
  `Expr::TraitCall`評価時の実行時エラーに初めて落ちていた。
- ~~**既知の制限**: ネストしたジェネリック呼び出し——ある`where`境界付きジェネリック関数の中から、
  外側自身の型パラメータをそのまま渡して別の`where`境界付き関数を呼ぶケース——は呼び出し側検証の
  対象外（型変数が裸の場合はスキップして既存の実行時フォールバックに委ね、型変数を*含む*具体型
  に包まれている場合はpin一致チェックが誤って失敗し得る）。~~ **→ 2026-07-15解消**:
  `validate_where_bounds`に呼び出し元の`caller_bounds`を渡すようにし、裸の型変数は外側の`where`節に
  一致するboundが宣言済みか照合、具体型に包まれた型変数はpin比較を（開いたままなら）単型化時の
  再検証に委ねてスキップするよう修正。`Sexpr`へのtrait実装は意図的に対象外のまま
  （要素型が固定されないリストにジェネリックな`Iter<Item>`を被せるのは型システム上不適切、という
  ユーザー判断）。

- **impl レベル `where`**（2026-08-01追加）: `(impl Ord cons-cell<A,B> (where (Ord A) (Ord B)) ...)`。
  各メソッドの `where` へ**構文的に**合流させる（`merge_where_clauses`）ので、
  ジェネリック所有者が retain する `MethodTemplate::Form` にもそのまま入り、単型化側に
  引数を追加で通す必要がない。
- **impl の完全性・適合性検査**（2026-08-01追加、`check_impl_conformance`）: 従来 `check_impl` は
  `TraitDef` を一切参照しておらず、メソッドの書き忘れは `:dyn` 化して `dyn_vtable_slots` に
  到達するまで検出されなかった（シグネチャ違いに至っては検出されなかった）。宣言されていない
  メソッド・重複・関連型の欠落・メソッドの欠落・シグネチャ不一致を `impl` の時点で弾く。
  `Self` の比較には**パース済みの**対象型を使う——プリミティブは `Type::I32` であって
  `Named("i32")` ではなく、両者は表示が同じでも等しくない。
- **ブランケット実装**（2026-08-01追加）: `(impl<T> Clamp T (where (Ord T)) ...)`。
  リーダは `impl<T>` を単一シンボルとして返す（`defstruct vector-iter<T>` の名前と同じ字句化）ので、
  `check_form_dispatch` は `match` の前に `parse_generic_name_header` を通す。
  対象が**裸の型変数**のときだけブランケットで、型構築子（`Vector<T>`）なら所有 `AdtDef` が1つに
  定まるので従来の経路をそのまま通る。
  - 宣言時は `Namespace::blanket_impls` に**保存するだけ**——`AdtDef` には何も登録しない。
    対象が型変数である以上どの本体もジェネリックで、具体型が要求する前に**生成**するのは
    単型化が避けているはずの先行展開そのものだから。ただし**検査**は生成とは別で、
    宣言時に1回、対象を型変数のまま行う（`precheck_blanket_impl`、2026-08-04追加）。
  - 実体化は `SpecRequest::Blanket` として既存の単型化キューに乗り、`materialize_blanket_impl` が
    保存した項目を live な `(impl Trait 具体型 ...)` に組み立て直して **`check_impl` に通す**。
    「impl とは何か」の実装を二重に持たないので、完全性検査・スーパトレイト義務・
    デフォルト本体・`Self` 置換がそのまま効く。
  - 参照点は `type_implements` に集約（`validate_where_bounds` / `dyn_vtable_slots` /
    `check_instance_method`）。明示 `impl` を先に見るので、明示があればそちらが勝つ。
  - **コヒーレンス**: 1トレイトにつきブランケット実装は1つまで（構文的に判定、順序非依存）。
    Rust の重なり解析より粗く、重ならないプログラムの一部を拒否する。明示 vs ブランケットの
    重なりを Rust は硬いエラーにするが、ここでは明示優先の黙認とする——ブランケットが
    ある型を覆うかは後から確立され得る境界に依存し、「重なるか」に単一時点の答えが無いため。
  - 相互再帰的な境界（`impl<T> A T (where (B T))` + `impl<T> B T (where (A T))`）は
    `BLANKET_BOUND_DEPTH` で打ち切る。
  - **宣言時の本体検査**（2026-08-04追加、`precheck_blanket_impl`）: 対象を未解決の
    `Type::Named`（型変数）のまま、ジェネリック `defun` の本体とまったく同じ扱いで検査する。
    スコープに入る境界は impl の `(where ...)` と**実装中のトレイト自身**（`impl<T> Clamp T` の
    中では `T` は `Clamp` を実装しているので、`self` に対する兄弟メソッド呼び出しが通る）。
    検査結果は捨てる——登録先の型が無いので何も登録できない。**どの対象でも誤り**であるものだけを
    捕まえる位置づけで、対象ごとの検査は従来どおり実体化が行う。

### 5.2 動的ディスパッチ（trait オブジェクト `:dyn Trait`）

2026-07-25実装（TODO T4）。**C++ の vtbl と同じ「呼び出し側は定数スロットを添字して間接呼び出し
するだけ」の形**。§5.1 の静的 trait 機構はそのままで、`:dyn Trait` と書いた位置でだけ動的になる。

#### 表記

`:dyn` は CL 相当のキーワード（§3）の中でも**特別扱い**で、型位置以外に現れたらエラーになる
（エディタが確実に強調表示できるようにするため）。表記が空白区切りの2語なので、リーダが2箇所で
特別に扱う——datum 位置では `:dyn X` を `(:dyn X)` の2要素リストに畳み（quote と同じ仕組み）、
型引数の中（`Vector<:dyn Drawable>`）ではトークンの `<>` が閉じるまで空白込みで読む（投機的で、
閉じなければ完全に巻き戻すので既存の `(< a b)` 等は影響を受けない）。

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(impl Drawable circle (draw ((self Self)) string "circle"))

(defun render ((d :dyn Drawable)) string (draw d))   ; 引数位置
(render (circle::new 3))                              ; 暗黙に箱詰めされる
(render (as :dyn Drawable (circle::new 3)))           ; 明示形
(defun all () Vector<:dyn Drawable> (Vector::new))    ; 型引数の中
```

関連型は**位置指定でpin**する。`:dyn Iter<i32>` は `Iter` の `Item` を `i32` に固定した trait
オブジェクトで、Rust の `dyn Iterator<Item = i32>` に相当する（pin の個数は宣言と厳密に一致が必要）。

#### 表現: vptr は「値の側」に持つ

trait オブジェクトの実行時表現は **fat box** `BoxedObj::Dyn { vtable_id, value }`。vtable は
**(具象型, trait) の組ごとに1本**で、箱詰め地点では両方が静的に分かっているので組を選べる。

C++ のように vptr をオブジェクト自身に持たせ**ない**理由:

- 1つの型が複数の trait を実装できるので単一 vptr では足りず、「型ID→trait→vtable」の2段引きに
  なる（実質 Go/Java の itable で、O(1) の添字ではなくなる）。
- 既存の `defstruct`/`defenum` の箱・`rt_struct_new`/`rt_data_new`・GC を一切変更せずに済み、
  `:dyn` を書かないコードのコストが変わらない。

vtable のスロット順は `TraitDef::vtable_order` ——**継承したメソッドが先頭**、その後に自前の
メソッドが `deftrait` の記述順で並ぶ（`methods` は `HashMap` で反復順が不定なので使えない）。
継承分を先頭に置くのは装飾ではなく仕様: これにより「最左スーパトレイト鎖」上の任意の `X` について
`X.vtable_order` は `Y.vtable_order` の**接頭辞**になり、`:dyn Y` の箱がそのまま `:dyn X` の
箱として通用する（同じスロット番号が同じ実装を指す）。中身はコンパイル済みコードの生関数ポインタ（`compiled_fn_type` ABI）で、
インタプリタ側は同じ id 空間の並列表に `(型Path, メソッド名)` を持ち、`FnDef.compiled` の有無で
毎回ネイティブ／ツリーウォークを選ぶ——静的な `Expr::Assoc` とまったく同じ二段構え。
vtable はヒープ値を持たないので GC ルート登録は不要で、コレクタが辿るのは箱の `value` だけ。

#### object safety（dyn 化できる条件）

`:dyn T` 型が**実際に作られる地点**でのみ検査する（`deftrait` 時ではない——静的にしか意味のない
メソッドを持つ trait でも、誰も trait オブジェクトを要求しなければ問題ない）。規則はすべて
「そのメソッドは1つの vtable スロットに収まる単一のエントリポイントを持たない」の言い換えで、
エラーメッセージにもその理由を書く:

1. trait が存在すること／メソッドが1つ以上あること
2. 各メソッドが `self` レシーバを持つこと（static 関連関数はディスパッチする値が無い）
3. `Self` がレシーバ以外に現れないこと（実装ごとに署名が変わる）
4. メソッド自身がジェネリックでないこと／可変長でないこと

さらに**具象型ごと**に、(a) その trait を実装していること、(b) ヒープ表現を持つこと
（fat box は `Value` を1つしか持てないので `i32` 等のプリミティブは入れられない——「trait 全体が
dyn 不可」ではなく「その型は入れられない」という粒度）、(c) 関連型の実際の束縛が pin と一致すること。

#### 暗黙 coercion の範囲

具象値が `:dyn Trait` を期待する位置に来たら自動で箱詰めされる。挿入箇所は `Sexpr` への暗黙変換と
**同じ場所**（`Checker::check_inner` の期待型突き合わせ）なので、及ぶ範囲も `Sexpr` と同一——
関数実引数・メソッド引数・コンストラクタのフィールド・`defvar` 初期値・戻り位置。
型注釈の無い `(let ((x obj)) ...)` には期待型が無いので coercion されない。

#### 箱の透過性

trait オブジェクトは**ディスパッチ以外の意味を持たない**:

- `print`/`format` は中身を印字する。
- `eq`/`eql`/`equal`/`equalp` は箱を透かして比較する（箱は暗黙に作られるので、箱の有無で
  同一性の答えが変わってはならない）。
- trait のメソッド以外を呼ぶと組み込み `Sexpr` メソッドカタログにフォールバックする。
- `Sexpr` に入れるときは箱を**外す**（`Sexpr` データの中に構造体の第二の表現を作らないため）。
- `match` はスクルーティニーの箱を外してから、既存の `Sexpr` downcast パターン（型名先頭／
  裸enum変種／`(the T p)`）にそのまま乗せる。実装型の集合は開いているので**非網羅**が正しく、
  catch-all が必須になる。

#### コンパイル済みコードとの関係

`:dyn` 呼び出しがネイティブになると、そこから届きうる**実装メソッドもネイティブでなければならない**
（vtable スロットから生関数ポインタを読んで間接呼び出しするため）。これは3つの経路で自動的に満たされる:

1. **コンパイル済みコードが箱詰めする場合**: 箱詰め地点には具象型と trait の両方があるので、
   その vtable のスロットがそのまま呼び出しグラフに載る。
2. **コンパイル済みコードがディスパッチする場合**: 呼び出し地点がチェックされた時点で存在した
   全実装（`Registry::trait_impls` の逆引き）が呼び出しグラフに載る。
3. **後から `impl` が追加された場合**: 上の2はどちらもスナップショットなので漏れる。この場合だけ、
   インタプリタが箱詰めする時点で「この trait をディスパッチするコンパイル済みコードが既にある」
   なら未コンパイルのスロットをその場でコンパイルする。

`:dyn` を一切コンパイルしないプログラムでは 3 の条件が常に偽なので、箱詰めが勝手にコンパイルを
誘発することはない。

#### スーパトレイトへの upcast（2026-08-01）

`:dyn Sub` の値を `:dyn Super` を要求する場所へ渡せる。上の接頭辞性から、これは
**型レベルの操作だけ**で済む——`coerce_to_dyn`/`upcast_dyn` が `Typed` の型を差し替え、式は
一切触らない。インタプリタ・JIT・AOT・島のどれも無変更で正しく動く理由:

- インタプリタの `Expr::DynCall` は `vtables[id][slot]` を引くだけで、`slot` は接頭辞同一。
- `translate_dyn_call` はスロットを定数として焼くが、焼く値が同じ。
- 島の `compile-dyn-call` に新しいタグが要らない（＝ `compiler_island.bc` の再生成が不要）。
- AOT の `rt_vtable_set` は `vtable_descriptors()` をそのまま出力する。

許すのは (1) 対象が推移的スーパトレイト閉包にある (2) ピンが鎖に沿って一致する、の2条件。
(3) 双方の `vtable_order` が接頭辞関係にあるかどうかは**可否ではなくコストを決める**——
接頭辞なら上記のとおり式に触らない retype、そうでなければ次の変換が入る。

#### 非最左スーパトレイトへの upcast（2026-08-04）

`D(B,C)` の `:dyn D` を `:dyn C` にする場合、`D` の vtable は `[B の…, C の…, D 自身の…]` なので
`C` のスロットは 0 始まりではなく、retype では呼び出し側が焼いた定数が別のエントリを指してしまう。
そこで `Expr::DynUpcast` を挿入し、**同じ具象値の周りに `C` 自身の vtable で箱を作り直す**。

鍵は「どの表に差し替えるかは値の具象型で決まるが、upcast 地点にはもう具象型が無い」こと。
そこで**箱詰め地点**——具象型と trait の両方が揃う唯一の場所——で
`Expr::DynBox::supers`（推移的スーパトレイト閉包ぶんの slot 表）を並べて用意し、
`Interp::register_dyn_box` が全部を intern して
「(vtable id, trait id) → vtable id」を登録する。upcast は実行時にこの表を引くだけ:

- インタプリタ: `Interp::dyn_upcasts`。
- コンパイル済み: `typelisp-rt` の `UPCASTS`（`upcast_define` / AOT は `rt_upcast_set`）を
  `rt_dyn_upcast` が引く。島には `dyn-upcast` タグ（`compile-dyn-upcast`）を足した。

supers は「接頭辞にならない相手だけ」ではなく**閉包すべて**を載せる。upcast は連鎖しうるので、
`D` から見れば接頭辞でも途中の `C` から見れば接頭辞でない相手が出てくる。
`supers` の slot 表は `dyn_vtable_slots` を再実行するのではなく、箱本体の slot 表から
**メソッド名で引き写す**（線形化はメソッド名ごとに1スロットなので部分列になる）——
箱がディスパッチする表と食い違いようがなく、失敗しようもない。

登録は閉包内の全ペアに対して行う。表の値は「この具象型がその trait に使う vtable」でしかなく、
2つの trait がどちら向きに関係していようと正しいから、余分な項目は無害
（許可の判断はチェッカーの仕事で、読まれない項目は存在しないのと同じ）。

#### 対象外（v1）

- **無関係な trait への upcast**（`:dyn A` → `:dyn B`、継承関係なし）: 2本目の vtable が要るが、
  そのとき具象型はもう分からない。具象値から改めて箱詰めすること。
- **`Sexpr` から `:dyn T` への取り出し**（`(the :dyn T p)`）: 「その値がその trait を実装して
  いるか」の実行時判定には全実装型の型名照合が要り、2026-07-04 に削除した n分岐チェーン方式に
  逆戻りする。具象型に `match` すること。

---

## 6. ユーザ定義型（defstruct / defenum）

`defstruct`（ユーザ定義 product 型）と `defenum`（ユーザ定義 sum 型）は実装済み。

- **`defstruct`**: 初回実装（フィールドの読み書き手段が無く `match` によるパターン分解以外に
  フィールドへアクセスする方法が無い不完全な設計）を2026-06-23に一度全面削除し、2026-06-24に
  フィールドアクセサ（`変数::フィールド名`／`(setf 変数::フィールド名 v)`）付きで再設計・
  再実装した（`RtValue::Struct` ベース）。ジェネリック定義（`(defstruct name<T> ...)`）にも対応。
- **`defenum`**: ユーザ定義の直和型で、既存の組み込み sum-ADT 機構（`Option<T>`/`Result<T,E>` と
  同じ内部表現）の薄いラッパとして2026-07-09に追加。`match`/`if-let` でパターン分解できる。
  ジェネリックヘッダの書き方は `defstruct` と統一されており、どちらも `name<T,U>` の形。
- `defstruct`/`defenum` いずれも `defmethod`（§5）でインスタンス/静的メソッドを持てる。

組み込み直和型 `Option<T>` / `Sexpr` / `Result<T,E>` / `HashTable<K,V>` / `Vector<T>` は `defstruct`/
`defenum` とは独立に、Rust側の`registry.rs`へ直接登録されている（ユーザ定義の構文経路を通らない）。

---

## 7. エラー処理（Rust 流）

### 7.1 Result と panic の住み分け
- **回復可能な失敗** → `Result<T, E>`（`Ok(T) | Err(E)`、組み込み直和型）＋ `match`。
- **回復不能な失敗（バグ・不変条件違反）** → `panic`。
- **`?`/try は導入しない**（Lisp 文法に馴染まないため）。失敗の分岐は `match` で明示する。

### 7.2 `Never` 型（`!`）
- `panic` は**特殊形**で、戻り型は `!`（Never / ボトム型）。
- `!` は**任意の期待型に適合**する（Rust の coercion 相当）。よって分岐の一方で panic しても型検査が通る:
  ```lisp
  (defun f ((x i32)) i32
    (if (< x 0) (panic "neg") x))   ; else 枝は ! → i32 に適合
  ```
- 型検査器での扱い: `if`/`match` の枝結合では Never 側は結果型を拘束しない（両方 Never なら Never）。
  型の突き合わせ（reconcile/unify）でも Never をボトムとして任意型に適合させる。
- `unreachable` / `todo` / `exit` も `!`（発散）。

### 7.3 命名規則 `!`/`?`
- typelisp の関数・特殊形の名前には `!` を接尾辞として使わない（CL に倣う）。発散する操作
  （`panic`/`unreachable`/`todo`）も破壊的（mutating）操作（`set-car`/`set-cdr`/`vector-set`/
  `vector-push` 等）も同様に `!` なしの名前にする。`!` による操作名のマーキングは Scheme の作法
  （`set!`/`vector-set!` 等）であり、CL 同等の表現力を目指す typelisp では採用しない。
- 同様に `?` も接尾辞として使わない（Scheme の述語命名 `even?`/`null?` 等の作法）。述語は CL 流の
  `-p`／`p` 接尾辞（`zerop`/`evenp`/`oddp`/`consp`/`atom`/`alphap`/`digitp`）または `is-` 前置
  （`is-some`/`is-none`/`is-ok`/`is-err`）で命名する（§4.1/§4.2）。
  CL の `some`（リストの述語）は typelisp では使えない（`Option` の `Some` 構成子とシンボルが
  大文字小文字無視で一致し、構成子解決が自由関数解決より優先されるため）。代わりに `?` 接尾辞
  （`some?`）に逃げず、衝突しない別名 **`any`**（Rust の `Iterator::any` 相当）を使う。
- `!` という記号自体は **`Never` 型の表記**（§7.2、例 `(fn (i32) !)`）として構文上の意味を持つ。
  `?` は現時点で構文上の意味を持たない。いずれも命名規則上の接尾辞としては使わない。

### 7.4 エラー型 E（2026-07-25 実装、TODO T3）
**Rust の `std::error::Error` に倣う**——`Error` は**型ではなくトレイト**であり、汎用の
「なんでも入るエラー値」は存在しない。

- **具象エラー型は発生源ごと**: 組み込みの失敗する操作はそれぞれ自分の型を返す
  （`parse-int`→`ParseIntError`、`parse-float`→`ParseFloatError`、`read`→`ReadError`、
  `eval`→`EvalError`。[registry.rs](../../src/check/registry.rs) の `builtin_error_defs`）。
  いずれもメッセージ文字列1つを持つ単一変種の直和型で、型名＝変種名。
- **ユーザ定義型がそのまま E に載る**: `Result<T, MyError>` の `MyError` は `defstruct` でも
  `defenum` でも良い。`Result<T,E>` の `E` は最初から任意の型を取れる総称パラメタなので、
  ここに言語側の追加機構は要らない（`Error` の実装すら必須ではない）。
- **`Error` トレイト**（prelude）: `message`（メッセージ）と `source`
  （包んでいる原因、無ければ `None`。Rust の `Error::source`）の2メソッド。組み込みの4型も
  ユーザ定義型とまったく同じ `impl` を prelude に書いてあるだけで、特別扱いは無い。
- **複数のエラー型を一様に扱う**: trait オブジェクト `Result<T, :dyn Error>`（§5.2）を使う
  ——Rust の `Box<dyn Error>` に相当。具象型からの広げ方は prelude の
  `as-dyn-error`（`Result<T,E> → Result<T,:dyn Error>`、`E` に `(Error E)` 境界）。
  `?` は導入しない方針（§7.3）なので、この変換は明示的に書く。
- **型とトレイトは1つの名前空間を共有する**（Rust と同じ）。同一モジュール内で `defstruct`/
  `defenum` とトレイトに同じ名前は付けられず（`Checker::check_type_trait_clash`）、型位置に
  トレイト名を書けば「`Error` はトレイトである、`:dyn Error` と書け」と報告される。
  組み込みの具象エラー型を `Error` と呼べないのはこの規則ゆえで、Rust と同じ結論
  （`std` でも `Error` はトレイト、具象型は `ParseIntError`/`io::Error`）になっている。
- **境界付き型変数の箱詰め**: ジェネリック関数の中で `(where (Error E))` の `E` を
  `:dyn Error` へ箱詰めできる。vtable は `E` が具象化する単型化時に確定するので、
  定義時の本体検査には消去済みプレースホルダ（`Expr::TraitCall`）が残るだけになる。

### 7.5 部分関数の失敗方針（Rust 流の混在）
| 操作 | 方針 |
|---|---|
| `vector-ref`（範囲外） | panic |
| `vector-get` | `Option<T>` |
| `/` `mod`（ゼロ除算） | panic（Rust の整数除算に忠実） |
| `parse-int` / `parse-float` | `Result<_, ParseIntError>` / `Result<_, ParseFloatError>` |
| `read` | `Result<Sexpr, ReadError>` |
| `unwrap`（None/Err） | panic |

原則: プログラマエラー＝panic、予期される失敗＝Result、安全版＝Option。

---

## 8. 当面の範囲外（将来課題）

- **`use a::b`（モジュール名を現NSに alias として導入）、可視性（pub/private）、絶対パス `::foo`は
  2026-06-16実装済み**（commit `1a5d9f3`、checker.rsの`check_use`/`check_pub`/`split_abs`）。
  ジェネリック構造体/受け手は`defstruct<T>`/`defenum<T>`として実装済み（§6）。**ネスト総称の
  修飾型（型変数を含む具体型を跨いだ多段の`::`解決）は2026-07-22に動作確認済み**——
  `(module m (pub defstruct box<T> (pub val T)))`に対し外側の`defun wrap<T> ((x T))
  Vector<m::box<T>>`（`Vector<T>`の中に開いた型変数`T`を持つモジュール修飾ジェネリック型
  `m::box<T>`を2段ネスト）という組み合わせで、`m::box::new`呼び出し・`wrap`の戻り値・
  ネストした構造体への`b::val`フィールドアクセスまで一貫して正しく型検査・評価されることを
  使い捨てテストで確認した。単なる「元々のTODO項目が古くから未検証のまま放置されていた」もの
  で、この過程で実際のバグは見つからなかった。
- ~~ユーザ定義エラー型~~ **2026-07-25 実装済み（TODO T3）——§7.4参照**。前提だった静的trait機構
  （`deftrait`/`impl`/`where`境界）は2026-06-30（§5.1）、動的ディスパッチ（trait オブジェクト
  `:dyn Trait`、vtable方式）は2026-07-25（§5.2、当初「実装しない」としていた方針からの転換）。
- 関数カタログ（§4）の実装本体は eval（step4）以降。**§4の実装状況は現時点でほぼ完了**——残る
  未実装項目は[functions.md](../functions.md)参照。
- `defmacro` の構造化ラムダリスト（`&rest`/`&optional`/`&key` すべて実装済み——2026-07-24。
  デフォルト式は展開時評価・先行パラメータ参照可、`&key` は `:name 値`）、マクロの
  `use`-alias 解決（こちらは未実装）。**`,@`（unquote-splicing）は2026-06-19実装済み**（commit `853bbdb`）。
- `defun`/`lambda` の**型付き** `&rest`／`apply` 特殊形は実装済み（2026-06-23に一度実装、
  2026-07-08に「Phase 6『&rest→Vector<T>』計画の破棄」に巻き込まれて削除されたのち、
  2026-07-15に**再導入**——削除自体はSexprベースの`&rest`設計そのものの欠陥ではなく、破棄済みの
  別計画（可変長パラメータをVector<T>で表す案）を道連れにした過剰撤去だったと判断）。
  本体内では`&rest`は常に`Sexpr`（CLを含む全Lispの`&rest`同様、cons セルの素のリスト——
  ホモジニアスな配列型は使わない）。呼び出し側で各可変長引数を宣言した要素型と個別に
  チェックし、`Sexpr`の対応するコンストラクタでラップして`cons`連結する（[[typelisp-vector-defstruct-revert]]参照）。
  `fn` 型も`(fn (T1... &rest Te) Ret)`で可変長関数の型を書ける。
- **`Vector<T>`/`defstruct`の再設計**（2026-06-23に全面削除、[[typelisp-vector-defstruct-revert]]
  参照）: `Vector<T>`は`RtValue::Vector`という専用enumバリアントを持っていたが「ユーザー定義型と
  同様に扱うべき」という原則に反すると判明し、`defstruct`自体もフィールド読み書き手段の欠如という
  不完全な設計と判明したため、両方を削除して1から設計し直すことになった。**`defstruct`は
  2026-06-24に再設計・再実装完了**（フィールドアクセサ`変数::フィールド名`/`(setf 変数::フィールド名 v)`、
  ジェネリック対応、`RtValue::Struct`ベース）。**`Vector<T>`も2026-06-30に再設計完了**——専用
  `RtValue`バリアントを作らず`RtValue::Struct`をそのまま使い（`StructData.fields`を可変長
  コレクションとして扱う）、push/get/set/lenをRust組み込みの`assoc`メソッドとして実装
  （`src/check/registry.rs`の`vector_def`、`src/eval/interp.rs`の`eval_builtin_method`の
  `"vector"`アーム）。
- **`compile`/`compile-file`の再実装**（2026-06-23に全面削除、2026-06-24以降`feature/compiler`
  ブランチで再構築・main へマージ済み）: self-hosting方針（コンパイラ本体は`src/compiler.rs`に
  typelisp自身で書き、Rustは inkwell バインディング・AST ブリッジ（`src/compile/ast_bridge.rs`）・
  ランタイムシムのみを担う）で JIT（`compile`）/AOT（`compile-file`）とも実装済み。対応構文の
  範囲は段階的に拡張中——詳細な進捗は[implementation-log.md](implementation-log.md)参照。

## 9. 採用しないと決めた機能（非採用）

§8 が「まだ無いもの（将来やるかもしれないもの）」であるのに対し、ここは**やらないと確定した
もの**。CL に同名の機能があることを理由に再検討する場合は、まず下記の判断根拠と衝突しないかを
確認すること。2026-07-29 に [TODO.md](TODO.md) から移設した（TODO ではないため）。

- **`Sexpr` への `Iter<Item>` trait 実装**: 要素型が固定されないリストにジェネリックな
  `Iter<Item>` を被せるのは型システム上不適切というユーザー判断（§5 末尾、
  [[typelisp-typechecking-is-not-design-soundness]]）。一度実装したが撤回済み。
  なお、cons セルのリストを走査する反復手段としては **`dolist` マクロが別途ある**
  （`src/prelude.rs` の `defmacro dolist`）。`Iter` トレイトを介さず `sexpr-consp`/`sexpr-car`/
  `sexpr-cdr` で直接歩いて各要素を束縛する（要素は動的に `Sexpr`。使う側が `match` で具体型に
  分解する）ので、上記の「ジェネリックな `Iter<Item>` を被せない」方針と両立している。
- **`?`/`try` 構文**、および `!`/`?` の命名接尾辞: CL に倣い非採用（§7.3）。
- **`set-pprint-dispatch` / `*print-pprint-dispatch*`**: CL の「型指定子をキーにした実行時の
  整形関数登録表」。文字列キーもプリンタのシグネチャも無検査で、「登録時点で分かっていた型を
  捨ててから `match` で復元する」形になり、静的型付け言語には合わない——CL のもう一方の機構
  である CLOS 総称関数 `print-object` に相当する **`print-object` トレイト**を 2026-07-26 に
  採用してこちらを置き換えた（[functions.md](../functions.md) §15.2）。判断の経緯は
  [implementation-log.md](implementation-log.md) の `print-object` トレイトの節。

- **ワイルドカードパス名・論理パス名（`logical-pathname`）、およびパス名のホスト/デバイス/
  バージョン成分**: 2026-08-05 のパス名層で採用しないと確定（[functions.md](../functions.md) §19）。
  いずれも CL が対応した「複数のファイルシステム世代」のための機能で、この処理系が走る環境には
  対応物が無い。パス名は `/` 区切りのディレクトリ成分・名前・型だけを持つ。
- **`input-stream-p` / `output-stream-p` / `stream-element-type`**: ストリームの方向も要素型も
  **型が持つ**（`CharInput`/`CharOutput` はトレイト）ので、実行時に尋ねる問いにならない
  （[functions.md](../functions.md) §18.7）。

CL 全体と突き合わせた「無いもの」の網羅リストは
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) にある。
