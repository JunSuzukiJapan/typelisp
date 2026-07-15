# typelisp 言語設計（確定仕様）

最終更新: 2026-07-11 / ブランチ: `main`

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
  `dolist` は `doiter`（§3、Iter トレイト経由の反復）へ統合され削除済み。
  `()` は期待型が `Sexpr` のとき `Nil` に、`Option<T>` のとき `None` になる（`Unit` はその他の文脈）。
  実行時は `Nil` を `Value::Empty` で符号化（`Cons` と対等な variant、`Option` ラッパーではない）。
  これは言語レベルで禁止した「真偽値としての `nil`」とは別物（あくまで read データ内の空リスト表現）。
- **`Symbol` は `Sexpr` とは別の独立したプリミティブ型**（`Sexpr` の `Sym` 構成子とは別物）。
  `Sexpr` が要求される文脈へは暗黙変換されるが、逆方向（`Sexpr`→`Symbol`）の自動変換はない。
  `gensym`/`symbol->string`/`string->symbol`（§4.1）はこの `Symbol` 型を使う。
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
- **ファイル拡張子**: ソースファイルは `.typl`。コンパイル済みモジュール（CL の `.fasl` 相当の
  「チェック済み定義のシリアライズ」——§2.4、`typl compile-module` が生成）は **`.fastl`**。
  これは LLVM の `compile-file`（`cc` でリンクした**ネイティブ実行ファイル**を直接出力、中間
  ファイルではない）とは別物。`.typlc` という拡張子は実装上使用されていない。
- **命名規則 `!`/`?`**: 関数名の末尾に `!`（破壊的操作）や `?`（述語）を接尾辞として使わない（詳細・理由は §7.3）。

---

## 1. メモリモデル / GC

- cons セルは**固定アリーナ**（起動時に確保、再確保しない＝生ポインタが安定）。将来 `--heap-cells N` で容量指定。
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

### 2.4 `(load)` とコンパイル済みモジュール（fasl、2026-07-14 実装）

- **`(load "path")`**: CL流のフラットロード（対象ファイルのフォームをカレント名前空間に読み込む、
  `use` のモジュール包みとは別）。トップレベル専用。`path.fastl` があり `source_hash` が `.typl` と
  一致すれば fasl を直接ロード（read・マクロ展開・型チェックを全スキップ）、無ければソース。
  **自動コンパイルはしない**。
- **fasl の実体**: ネイティブコードではなく「**チェック済み状態のシリアライズ**」（`src/fasl.rs`、
  LLVM `(compile ...)` とは無関係）。中身は Registry の名前差分 + generic テンプレート（唯一 heap を
  参照する Checker 状態を `OwnedForm` 化）+ チェック済み `TopLevel` 列（ロード時 `interp.exec` で
  再登録）。**生ポインタ処理系なのでヒープの clone/コピーは不可**——ロード時に確保 API
  （`heap.cons`/`alloc_string`/`intern_symbol`）で値を作り直す（`owned_to_value`）。
- **生成**: `typl compile-module <file.typl> [-o out.fastl]`。モジュールは定義のみ（トップレベル式は
  エラー）。
- **prelude 起動最適化**: prelude 自身もこの機構で起動時ロード（`prelude::load_cached`、
  専用ディレクトリ `$TYPL_CACHE_DIR`|`~/.typl/cache/` にキャッシュ）。LSP は prelude fasl を起動時に1つ構築し
  各診断パスで `Fasl::load_into` 再利用——キー入力毎の prelude 再チェックが消える（実測 5.2倍速）。
  `prelude::load`（純ソース）はテストの hermeticity のため据え置き。

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
| 定義 | `defun` `defvar` `defconstant` `defmethod` `defmacro` `defstruct` `defenum` `module` `use` `lambda` `deftrait` `impl` | 引数・戻り型・グローバルの型は明示（`defvar`/`defconstant`は`(defvar (name Type) value)`で型必須、2026-07-03に型なし形式を削除。局所束縛`let`のみ推論可。`defmacro` は全パラメータ・戻りが `Sexpr` 固定なので型注釈なし、末尾 `&rest name` で可変長対応）。ジェネリック定義形（`defun`/`defstruct`/`defenum`）の型パラメータは名前に山括弧で書く（`name<T,U>`）。`defstruct`（ユーザ定義product型）は再設計後に実装済み。`defenum`（ユーザ定義直和型、`match`/`if-let`対応）を追加。`deftrait`/`impl`（trait機構、§5.1）は2026-06-30実装 |
| 束縛 | `let` `let*` | |
| 制御 | `if` `when` `unless` `cond` `case` `match` `if-let` `while-let` `and` `or` `progn` `the` | `and`/`or` は短絡のため特殊形。`the` は型注釈 |
| 反復 | `loop` `while` `until` `dotimes` `do` `doiter` | `dolist` は `doiter` へ統合され削除済み |
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
の前方参照は不可）。checkerは単一パスで、`defmacro`自体の登録（マクロ展開器`Interp::fns`への
登録）はそのフォームをチェックした副作用として起きるため、原理的にそれより前のフォームからは
呼び出せない——C言語のヘッダのような宣言/定義分離が無い設計（§2.3の循環参照エラーと同じ理由）
の帰結であり、依存ロード・`use`生成マクロ（下記）を含め全マクロ呼び出しに共通するルール。
`mod::macro-name`という`::`修飾呼び出しでモジュール越しにマクロを呼ぶこともできる（`use`のエイリ
アス経由ではなく常にフルパス、`pub defmacro`でなければ定義モジュール外から不可視——`defun`の
`mod::fn`呼び出しと同じ可視性規則）。
`when`/`unless`/`and`/`or`/`cond`/`let*` は `if`/`let` への脱糖。`setf`（可変ローカル/グローバル変数）/`while` は専用 AST
ノード（eval 環境は `Rc<RefCell>` の可変スロット）。`defvar`（可変）/`defconstant`（不変）はグローバル変数を現在の
名前空間に登録する。型注釈は**必須**——`(defvar (name Type) value)`（グローバルの型はプログラムの
公開サーフェスであり初期化子から推論しない。型なしの`(defvar name value)`形式は2026-07-03に削除。
副次的に、注釈が初期化子のexpected型になるため`(defvar (f (fn (i32) i32)) identity)`のような
ジェネリック関数値の単型化解決もここから効く）。
**`lambda`** は関数を第一級の値（GCヒープ上のクロージャボックス、`BoxedObj::Closure`）にする: `(lambda (params) ret body...)`、型は `(fn ...)`。
定義時の環境（可変スロット）を捕捉する真のクロージャ。関数値の呼び出しは頭がローカル変数・グローバル変数・任意の式
（例 `((lambda ...) x)`）のとき `Expr::Apply` に。**名前付き関数も値化可能**（`id` 等を高階関数へ渡せる。`Expr::FnRef`、
組み込みは `RtValue::Builtin`）。**`dotimes`** `(dotimes (var count) body...)` は `let`+`while`+`setf` への脱糖。
**`list`** `(list e1 ... en)` は `(Cons e1 (Cons e2 (... (Nil))))` への脱糖（`(list)` は `(Nil)`）。**`dolist`**
（`let`+`while`+`match` で `Sexpr` の `Cons`/`Nil` を辿る特殊形だった）は削除済み——`Iter` トレイト
経由で任意のコレクションを辿れる `doiter`（本節末尾）に統合された。
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
| 解析 | `parse-int parse-float` | `Result<_, Error>` |
| IO | `print println princ format read read-line` | `read : (fn (String) Result<Sexpr, Error>)` |
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
- ディスパッチは **まず静的**（self の静的型で一意に解決）。**動的ディスパッチ（vtable/`dyn Trait`相当）は
  実装しない**——trait機構（§5.1、2026-06-30実装）はこの単一静的ディスパッチを拡張する形で構築されており、
  ジェネリック関数本体の型変数レシーバ呼び出しのみ実行時に値自身の型タグを読む（§5.1参照、vtable的な
  間接呼び出しテーブルではない）。

例（`i32` という組み込み型に対する static / instance メソッド。`defstruct` で定義したユーザ定義型
にも同じ構文で `defmethod` を書ける、§6 参照）:
```lisp
(defmethod zero (i32) i32 0)                  ; static → (i32::zero)
(defmethod double ((self i32)) i32 (+ self self))  ; instance → (double 3)
```

### 5.1 trait機構（deftrait / impl / where）

`doiter`（§3末尾）が「Iterトレイトを実装した型すべてで使える」ことを要求したため2026-06-30に導入。
`defmethod`の単一静的ディスパッチをそのまま再利用する設計（trait専用の新しいディスパッチ機構は作らない）。

- **trait定義**: `(deftrait Name (type AssocName)... (method-name ((self Self) params...) Ret)...)`。
  `Self`・宣言した関連型名は本体を持たないメソッドシグネチャの中で型変数として使える。
  ```lisp
  (deftrait Iter
    (type Item)
    (next ((self Self)) Option<Item>))
  ```
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

### 7.4 エラー型 E
- 当面は**組み込み汎用 `Error`**（メッセージ等を保持）。既定は `Result<T, Error>`。
- trait機構（`deftrait`/`impl`、§5.1）は実装済みだが、ユーザ定義エラー型をこの機構で扱えるように
  拡張する作業自体はまだ行っていない（§8）。

### 7.5 部分関数の失敗方針（Rust 流の混在）
| 操作 | 方針 |
|---|---|
| `vector-ref`（範囲外） | panic |
| `vector-get` | `Option<T>` |
| `/` `mod`（ゼロ除算） | panic（Rust の整数除算に忠実） |
| `parse-int` / `parse-float` | `Result<_, Error>` |
| `read` | `Result<Sexpr, Error>` |
| `unwrap`（None/Err） | panic |

原則: プログラマエラー＝panic、予期される失敗＝Result、安全版＝Option。

---

## 8. 当面の範囲外（将来課題）

- **`use a::b`（モジュール名を現NSに alias として導入）、可視性（pub/private）、絶対パス `::foo`は
  2026-06-16実装済み**（commit `1a5d9f3`、checker.rsの`check_use`/`check_pub`/`split_abs`）。
  ジェネリック構造体/受け手は`defstruct<T>`/`defenum<T>`として実装済み（§6）。ネスト総称の
  修飾型（型変数を含む具体型を跨いだ多段の`::`解決）の網羅的な検証は未確認、残課題として扱う。
- 動的ディスパッチ（vtable/`dyn Trait`相当）、ユーザ定義エラー型。**静的trait機構
  （`deftrait`/`impl`/`where`境界）は2026-06-30実装済み**——§5.1参照。
- 関数カタログ（§4）の実装本体は eval（step4）以降。**§4の実装状況は現時点でほぼ完了**——残る
  未実装項目は[functions.md](../functions.md)参照。
- `defmacro` の構造化ラムダリスト（`&rest` のみ実装済み、`&optional`/`&key` は対象外）、マクロの
  `use`-alias 解決。**`,@`（unquote-splicing）は2026-06-19実装済み**（commit `853bbdb`）。
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
