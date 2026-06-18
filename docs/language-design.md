# typelisp 言語設計（確定仕様）

最終更新: 2026-06-17 / ブランチ: `feature/typed-lisp-impl`

このドキュメントは、設計で**確定した言語仕様**を後から見返せるよう記録するもの。
実装の進捗・段取りは [TODO.md](TODO.md) を参照。

---

## 0. 言語の基本方針

- **静的型付け**: すべての式が静的型を持つ。動的タグ付き Lisp（旧 3ab5599 の `Object`）ではない。
- **文法は macro-lisp 準拠**: `/Users/suzukijun/Program/Rust/macro-lisp` の**構文**に従う（実装は参考にしない）。
  `defun`/`defstruct`/`defvar`/`defconstant`/`let`/`if`/`when`/`unless`/`cond`/`match`/`loop`/`while`/`dotimes`/`do`/`doiter`/`while-let`/`if-let`/`lambda`/`progn`/`module` 等。関数の引数・戻り型は必須、局所束縛は型推論可。
- **真偽値は `true`/`false`**。`nil`/`t` は言語に存在しない。
- **`nil` の代替は `Option<T>`**（`(defstruct Option (Some (value T)) (None))` 相当の直和型）。
- **`read` の戻り値は組み込み直和型 `Sexpr`**。
  `Sexpr = Nil|Int|Float|Char|Bool|Sym|Str|Cons(Sexpr, Sexpr)`。
  空リスト `()` は `Sexpr` の値としての `Nil`（cons と並ぶ第一級の構成子）。car/cdr はどちらも `Sexpr`
  （`Option` で包まない）。`cons`/`car`/`cdr`/`list`/`dolist` は cons と nil の双対のまま自然に書ける。
  `()` は期待型が `Sexpr` のとき `Nil` に、`Option<T>` のとき `None` になる（`Unit` はその他の文脈）。
  実行時は `Nil` を `Value::Empty` で符号化（`Cons` と対等な variant、`Option` ラッパーではない）。
  これは言語レベルで禁止した「真偽値としての `nil`」とは別物（あくまで read データ内の空リスト表現）。
- **大文字小文字は区別しない**（シンボルは小文字に正規化してインターン）。
- **Rust 相互運用はしない**（`&args[1]`, `env::args().collect()` 等は対象外）。
- **構成子パターンは S 式形** `(Some v)` / `(Cons a d)`。
- **実行モデル**: 既定はインタプリタ（eval）。**ネイティブコンパイルは明示的 `compile`/`compile-file`（CL 準拠）を呼んだ時だけ**。LLVM コンパイラは feature gate。

---

## 1. メモリモデル / GC

- cons セルは**固定アリーナ**（起動時に確保、再確保しない＝生ポインタが安定）。将来 `--heap-cells N` で容量指定。
- 割当はフリーリストから。空なら GC、それでも空なら **`Error::HeapExhausted`（成長しない）**。
- **mark-sweep GC**（反復マーク＝深い構造でもスタック溢れなし、循環回収）。ルート集合 `push_root`/`pop_root`。
- **生ポインタは `ConsRef` に隠蔽、公開 API は安全**。
- シンボルはインターン（小文字正規化・永続）。文字列は GC 管理（到達可能のみ生存）。
- **実行時 `Sexpr` 値も同じヒープ**（`RtValue::Sexpr`）: read 時のデータと eval 中にプログラムが `cons`/`list`/構成子で
  作るデータは同一の cons アリーナ・GC を共有する（`Sexpr`/`Option`/`defstruct` 等それ以外のADTは `RtValue::Data` の
  まま Rust ヒープ上＝GC 対象外。GC の対象は cons セル/シンボル/文字列のみという方針通り）。
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
  （`Nil`/`Cons(Sexpr, Sexpr)` は確定済み。`Path` 追加は未実装。）
- セグメントは小文字化される（シンボルと同じ正規化）。空セグメント（`foo::`, `::bar`, `a::::b`）は読み取りエラー。
- 総称は最終セグメントに付く（`a::Vec<T>` → セグメント `[a, vec<t>]`、型は最終セグメントの `<>` を解釈）。

### 2.2 module は名前空間、型は名前空間ではない
- **module = 名前空間**（`module`/`use` で扱う）。
- **型は Rust 同様**に扱う ＝ 型は **関連関数（associated function）/メソッド** を持つ。型は名前空間ではない。
- したがって `Foo::Bar` の意味は文脈で決まる（Rust のパス解決と同じ）:
  - `Foo` が **module** なら「module Foo の項目 Bar」。
  - `Foo` が **型** なら「型 Foo の関連関数/メソッド Bar」。
  - checker が先頭セグメントを解決し、module か型かを判定して残りを下降解決する。

### 2.3 名前解決規則
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
| 定義 | `defun` `defstruct` `defvar` `defconstant` `defmethod` `defmacro` `module` `use` `lambda` | 引数・戻り型は明示（局所束縛は推論可。`defmacro` は全パラメータ・戻りが `Sexpr` 固定なので型注釈なし） |
| 束縛 | `let` `let*` | |
| 制御 | `if` `when` `unless` `cond` `case` `match` `if-let` `while-let` `and` `or` `progn` `the` | `and`/`or` は短絡のため特殊形。`the` は型注釈 |
| 反復 | `loop` `while` `until` `dotimes` `dolist` `do` `doiter` | |
| マクロ/引用 | `quote` `quasiquote` (`` ` ``) `unquote` (`,`) | `quote`/`quasiquote` の戻り型は常に `Sexpr`。`,@`（unquote-splicing）は未実装 |
| その他 | `setf` `break` `return` `panic` `unreachable` `todo` | `break`/`return`/`panic`/`unreachable`/`todo` は戻り型 `!`（§7） |

脱糖の例: `when`→`if`+`progn`、`unless`→`if`、`if-let (pat val) then else`→2 腕 `match`（包括アームで網羅）、
`quasiquote`→`Expr::Quote`+`Expr::Construct{Cons,..}` の組合せ（`list` の脱糖と同様、ランタイムマクロ機構は使わない）。

実装状況: `if` `let` `let*` `progn` `when` `unless` `and` `or` `cond` `setf` `while` `loop` `break` `return` `lambda`
`match` `if-let` `panic` `defstruct` `defvar` `defconstant` `module` `use` `defmethod` `quote` `quasiquote` `defmacro`
は実装済（[src/check/checker.rs](../src/check/checker.rs)）。`defmacro` は CL 流（非衛生的）— 詳細は
[TODO.md](TODO.md) のステップ 4k を参照。
`when`/`unless`/`and`/`or`/`cond`/`let*` は `if`/`let` への脱糖。`setf`（可変ローカル/グローバル変数）/`while` は専用 AST
ノード（eval 環境は `Rc<RefCell>` の可変スロット）。`defvar`（可変）/`defconstant`（不変）はグローバル変数を現在の
名前空間に登録し、型注釈 `(name Type)` は任意（省略時は値から推論）。
**`lambda`** は関数を第一級の値（`RtValue::Closure`）にする: `(lambda (params) ret body...)`、型は `(fn ...)`。
定義時の環境（可変スロット）を捕捉する真のクロージャ。関数値の呼び出しは頭がローカル変数・グローバル変数・任意の式
（例 `((lambda ...) x)`）のとき `Expr::Apply` に。**名前付き関数も値化可能**（`id` 等を高階関数へ渡せる。`Expr::FnRef`、
組み込みは `RtValue::Builtin`）。**`dotimes`** `(dotimes (var count) body...)` は `let`+`while`+`setf` への脱糖。
**`list`** `(list e1 ... en)` は `(Cons e1 (Cons e2 (... (Nil))))` への脱糖（`(list)` は `(Nil)`）。**`dolist`**
`(dolist (var list-expr) body...)` は `let`+`while`+`match` への脱糖（`Sexpr` の `Cons`/`Nil` を辿る、結果は `Unit`）。
**`loop`** `(loop body...)` は無限ループ。**`break`/`return`** は CL 流：どちらも**直近のループのみ**を脱出する
（関数の早期 return ではない。`lambda` 境界は越えられない＝クロージャの中から外側のループへ break/return できない）。
`break` は値を取らず（常に `Unit` で脱出）、`return` は `(return)`／`(return value)` で値任意。`while`/`dotimes`/
`dolist`/`loop` いずれの内側でも使え、`loop` の型は内側で見つかった `break`/`return` の値型の join（`match`/`cond` の
腕と同様に一致が必要）。一度も脱出しない `loop` は型 `!`（Rust の `loop {}` と同じ）。`while` 系はもともと型が `Unit`
固定なので、その内側の `return` の値も `Unit` でなければ型エラー。
残り（`case` `do` `doiter` `while-let` `the`）は今後。

---

## 4. 関数カタログ（Rust 組み込み vs typelisp ライブラリ）

**分離原則**: ヒープ/ランタイム/IO/プリミティブ演算/ネイティブ codegen を要するものは **Rust 実装**。
それらの組合せで書けるものは **typelisp 自身で実装**（ライブラリ）。すべて型付き（引数/戻り型を明示）。

> 実装状況: eval（step4）でツリーウォーク評価を実装済み。組み込み関数は**i32 の算術/比較**
> （`+ - * / mod < <= > >= = /=`、`/`/`mod` のゼロ除算は panic）と **`Sexpr` 上の `cons`/`car`/`cdr`**
> （`car`/`cdr` は非 `Cons`＝`Nil` 含むで panic）を実装済み。他のカタログ項目は今後 eval 拡充で追加。
> `Sexpr` の実行時値は §1 のとおり cons ヒープ（GC 管理）に統合済み。

### 4.1 Rust 組み込み（primitive）
| 種別 | 関数 | 備考 / 例 |
|---|---|---|
| 算術 | `+ - * / mod rem neg abs` | 型ごと。例 `+ : (fn (i32 i32) i32)`。`/` のゼロ除算は `Result` |
| 比較 | `= /= < <= > >=` | |
| 論理 | `not` | `and`/`or` は短絡で特殊形 |
| cons | `cons car cdr set-car set-cdr consp atom eq` | |
| 変換 | `int->float float->int char->int int->char symbol->string string->symbol` | |
| 文字列 | `string-length string-append string-ref substring string=?` | |
| ベクタ | `make-vector vector-ref vector-set vector-length vector-push vector-get` | `vector-ref` 範囲外は panic、`vector-get : Option<T>` |
| 解析 | `parse-int parse-float` | `Result<_, Error>` |
| IO | `print println princ format read read-line` | `read : (fn (String) Result<Sexpr, Error>)` |
| 発散 | `panic unreachable todo exit` | 戻り型 `!`（§7） |
| システム | `eval compile compile-file gc` | `compile`/`compile-file` は明示呼び出し時のみネイティブ化（feature gate） |
| マクロ | `gensym` | 引数なし、フレッシュな `Sexpr::Sym` を返す。symbol は常に intern される仕様のため衝突耐性のみ（CL の unforgeable な未intern symbol ではない） |

### 4.2 typelisp ライブラリ（derived）
| 種別 | 関数 |
|---|---|
| リスト | `list length append reverse nth last map filter foldl foldr member assoc find every some` |
| Option | `unwrap`(None で panic) `unwrap-or is-some is-none map-option and-then or-else` |
| Result | `is-ok is-err ok-or unwrap-or-else map-result` |
| 高階 | `identity const compose flip apply` |
| 数値補助 | `min max sum product range iota even? odd? zero?` |

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
- ディスパッチは **まず静的**（self の静的型で一意に解決）。将来 trait / 動的ディスパッチを追加する余地を残す。

例:
```lisp
(defstruct Point (mk (x i32) (y i32)))
(defmethod new (Point (x i32) (y i32)) Point (mk x y))   ; static → (Point::new 1 2)
(defmethod norm ((self Point)) i32                        ; instance → (norm p)
  (match self ((mk a b) a)))
```

---

## 6. defstruct 文法

**直和形**（メモリの `(defstruct Option (Some (value T)) (None))` 例に準拠）:
```
(defstruct Name (Ctor (field Type) ...) (Ctor2 ...) ...)
```
- 各構成子は `(構成子名 (フィールド名 型) ...)`。単一構成子＝積型（例 `(defstruct Point (Point (x i32) (y i32)))`）。
- 第1版は**非ジェネリック**のみ。ジェネリック構造体（`Pair<K,V>` 等）は後続。
- macro-lisp の積型専用 `defstruct` とは divergence する（typelisp は直和を第一級に扱う）。

組み込み直和型 `Option<T>` / `Sexpr` / `Result<T,E>` も同じモデル（[src/check/registry.rs](../src/check/registry.rs)）。

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

### 7.3 命名規則 `!`
- typelisp の関数・特殊形の名前には `!` を接尾辞として使わない（CL に倣う）。発散する操作
  （`panic`/`unreachable`/`todo`）も破壊的（mutating）操作（`set-car`/`set-cdr`/`vector-set`/
  `vector-push` 等）も同様に `!` なしの名前にする。`!` による操作名のマーキングは Scheme の作法
  （`set!`/`vector-set!` 等）であり、CL 同等の表現力を目指す typelisp では採用しない。
- `!` という記号自体は **`Never` 型の表記**（§7.2、例 `(fn (i32) !)`）としてのみ使われ、命名規則上の
  接尾辞ではない。

### 7.4 エラー型 E
- 当面は**組み込み汎用 `Error`**（メッセージ等を保持）。既定は `Result<T, Error>`。
- 将来 trait を導入した際に、ユーザ定義エラー型も扱えるよう拡張する。

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

- `use a::b`（モジュール名を現NSに alias として導入。個別 `use a::b::name` は対応）、ジェネリック構造体/受け手、ネスト総称の修飾型。
- 可視性（pub/private）、絶対パス `::foo`。
- trait / 動的ディスパッチ、ユーザ定義エラー型。
- 関数カタログ（§4）の実装本体は eval（step4）以降。
- `,@`（unquote-splicing、`append` 実装後）、`defmacro` の `&rest`／構造化ラムダリスト、マクロの `use`-alias 解決。
