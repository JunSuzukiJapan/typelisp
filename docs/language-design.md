# typelisp 言語設計（確定仕様）

最終更新: 2026-06-30 / ブランチ: `feature/compile-sexpr`

このドキュメントは、設計で**確定した言語仕様**を後から見返せるよう記録するもの。
現在の残作業は [TODO.md](TODO.md)、完了した実装の経緯は [implementation-log.md](implementation-log.md) を参照。

---

## 0. 言語の基本方針

- **静的型付け**: すべての式が静的型を持つ。動的タグ付き Lisp（旧 3ab5599 の `Object`）ではない。
- **文法は macro-lisp 準拠**: `/Users/suzukijun/Program/Rust/macro-lisp` の**構文**に従う（実装は参考にしない）。
  `defun`/`defvar`/`defconstant`/`let`/`if`/`when`/`unless`/`cond`/`match`/`loop`/`while`/`dotimes`/`do`/`doiter`/`while-let`/`if-let`/`lambda`/`progn`/`module` 等。関数の引数・戻り型は必須、局所束縛は型推論可。
  （`defstruct` はユーザ定義型機構として実装されたが、フィールドの読み書き手段が無い不完全な
  設計と判明し2026-06-23に削除・再設計待ち——[[typelisp-vector-defstruct-revert]]参照。
  ユーザ定義型は当面言語に存在しない。）
- **真偽値は `true`/`false`**。`nil`/`t` は言語に存在しない。
- **`nil` の代替は `Option<T>`**（組み込みの直和型——`Some(value T)`/`None` の2構成子）。
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
- **実行モデル**: インタプリタ（eval）のみ。**ネイティブコンパイル（明示的 `compile`/`compile-file`、CL 準拠、
  LLVM ベース）は Phase 6a まで実装されたが、2026-06-23 に `Vector<T>`/`defstruct` の全面リバートに
  伴って削除済み**——再設計完了後に再着手する将来課題（§8、[[typelisp-vector-defstruct-revert]]参照）。
- **ファイル拡張子**: ソースファイルは `.typl`。`.typlc`（CL の `.fasl` 相当、コンパイル済みファイル用）は
  `compile`/`compile-file` 削除に伴い当面未使用。
- **命名規則 `!`/`?`**: 関数名の末尾に `!`（破壊的操作）や `?`（述語）を接尾辞として使わない（詳細・理由は §7.3）。

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
| 定義 | `defun` `defvar` `defconstant` `defmethod` `defmacro` `module` `use` `lambda` `deftrait` `impl` | 引数・戻り型は明示（局所束縛は推論可。`defmacro` は全パラメータ・戻りが `Sexpr` 固定なので型注釈なし、末尾 `&rest name` で可変長対応）。`defstruct`（ユーザ定義型）は2026-06-23に削除・再設計待ち。`deftrait`/`impl`（trait機構、§5.1）は2026-06-30実装 |
| 束縛 | `let` `let*` | |
| 制御 | `if` `when` `unless` `cond` `case` `match` `if-let` `while-let` `and` `or` `progn` `the` | `and`/`or` は短絡のため特殊形。`the` は型注釈 |
| 反復 | `loop` `while` `until` `dotimes` `dolist` `do` `doiter` | |
| マクロ/引用 | `quote` `quasiquote` (`` ` ``) `unquote` (`,`) | `quote`/`quasiquote` の戻り型は常に `Sexpr`。`,@`（unquote-splicing）は未実装 |
| その他 | `setf` `break` `return` `panic` `unreachable` `todo` | `break`/`return`/`panic`/`unreachable`/`todo` は戻り型 `!`（§7） |

脱糖の例: `when`→`if`+`progn`、`unless`→`if`、`if-let (pat val) then else`→2 腕 `match`（包括アームで網羅）、
`quasiquote`→`Expr::Quote`+`Expr::Construct{Cons,..}` の組合せ（`list` の脱糖と同様、ランタイムマクロ機構は使わない）。

実装状況: `if` `let` `let*` `progn` `when` `unless` `and` `or` `cond` `case` `setf` `while` `until` `loop` `break`
`return` `lambda` `match` `if-let` `while-let` `do` `doiter` `the` `panic` `defvar` `defconstant` `module` `use`
`defmethod` `deftrait` `impl` `quote` `quasiquote` `defmacro`
は実装済（[src/check/checker.rs](../src/check/checker.rs)。`the`のみchecker特殊形、
`case`/`until`/`while-let`/`do`/`doiter`は`prelude.rs`の`defmacro`）。`unreachable`/`todo`/`exit`（§4.1/§7）も実装済
（前2つは`panic`を呼ぶ`defmacro`、`exit`は`std::process::exit`を呼ぶRust組み込み自由関数）。
`doiter`も実装済（2026-06-30、`prelude.rs`の`defmacro`——`var`の型はマクロ展開時には分からないが、
展開先の`(some var)`のような構成子パターンの型を`Checker::check_ctor_pattern`がscrutinee（`next`の
戻り値`Option<Item>`）から自動推論するため、checker特殊形にする必要はない。`while-let`を呼ぶだけの
薄い`defmacro`、§5.1のtrait機構参照）。`defmacro` は CL 流（非衛生的）— 詳細は
[implementation-log.md](implementation-log.md) のステップ 4k を参照。`defstruct` は実装後2026-06-23に削除・再設計待ち。
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
| 文字列 | `string-length string-append string-ref substring string=` | |
| 解析 | `parse-int parse-float` | `Result<_, Error>` |
| IO | `print println princ format read read-line` | `read : (fn (String) Result<Sexpr, Error>)` |
| 発散 | `panic unreachable todo exit` | 戻り型 `!`（§7） |
| システム | `eval gc` | `compile`/`compile-file` は実装後2026-06-23に削除・再設計待ち（§0、§8） |
| マクロ | `gensym` | 引数なし、フレッシュな `Sexpr::Sym` を返す。symbol は常に intern される仕様のため衝突耐性のみ（CL の unforgeable な未intern symbol ではない） |

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

例（`i32` という既存の組み込み型に対する static / instance メソッド。ユーザ定義型
`defstruct` は2026-06-23に削除・再設計待ちのため、組み込み型を例に挙げる）:
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
- **ジェネリック関数のtrait境界**: `(defun (name T) (params...) Ret (where (Trait T)...) body...)`。
  `where`節は関数の**呼び出し側シグネチャには影響せず**（呼び出し側での境界検証は未実装、TODO.md参照）、
  本体チェック時にのみ「型変数Tはこのtraitのメソッドを呼べる」という情報を与える。本体内で型変数Tの
  値に対するtraitメソッド呼び出しは、新設の`Expr::TraitCall`ノードになる——`Expr::Assoc`と違い
  実装型のPathをチェック時には持たず、実行時にレシーバの値自身が持つ型タグ（`RtValue::Struct`の
  `type_name`等）を読んで`Expr::Assoc`と同じ`methods`テーブルを引く。これが本機構で唯一「型消去後の
  実行時情報」を必要とする箇所だが、参照するテーブル自体はvtable等の専用間接構造ではなく、既存の
  固定`methods`テーブルそのもの。
  ```lisp
  (defun (count-iter T) ((it T)) i32 (where (Iter T))
    (let ((n 0)) (doiter (x it) (setf n (+ n 1))) n))
  ```
- **既知の制限**（TODO.md参照）: 関連型の具体指定（「Tの`Item`はi32」のような制約）は`where`節で
  表現できない。呼び出し側での境界検証も未実装。`Sexpr`へのtrait実装は意図的に対象外（要素型が
  固定されないリストにジェネリックな`Iter<Item>`を被せるのは型システム上不適切、というユーザー判断）。

---

## 6. ユーザ定義型（当面範囲外）

`defstruct`（ユーザ定義の直和型）は実装されたが、フィールドの読み書き手段が無い不完全な設計
（`match` によるパターン分解以外にフィールドへアクセスする方法が無く、書き込み手段は一切無い）
と判明し、2026-06-23に全面削除した。再設計は[[typelisp-vector-defstruct-revert]]参照、§8。

組み込み直和型 `Option<T>` / `Sexpr` / `Result<T,E>` / `HashTable<K,V>` は `defstruct` の削除に
関わらず引き続き存在する（Rust側の`registry.rs`に直接登録、ユーザ定義の構文経路とは独立）。

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
- 動的ディスパッチ（vtable/`dyn Trait`相当）、ユーザ定義エラー型。**静的trait機構
  （`deftrait`/`impl`/`where`境界）は2026-06-30実装済み**——§5.1参照。
- 関数カタログ（§4）の実装本体は eval（step4）以降。
- `,@`（unquote-splicing、`append` 実装後）、`defmacro` の構造化ラムダリスト（`&rest` のみ実装済み、`&optional`/`&key` は対象外）、マクロの `use`-alias 解決。
- `defun`/`lambda` の**型付き** `&rest`／`apply` は実装済み（roadmap step10）。
  本体内では`&rest`は常に`Sexpr`（CLを含む全Lispの`&rest`同様、cons セルの素のリスト——
  ホモジニアスな配列型は使わない）。呼び出し側で各可変長引数を宣言した要素型と個別に
  チェックし、`Sexpr`の対応するコンストラクタでラップして`cons`連結する（2026-06-23、
  [[typelisp-vector-defstruct-revert]]参照）。
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
- **`compile`/`compile-file`の再実装**（同上の理由で2026-06-23に全面削除）: コンパイラ本体
  （typelisp自身で書く、`compiler_source.rs`）が内部データ構造に何を使うかは再検討の余地が
  あるが、Vector/defstructとも再設計完了済みなので前提は満たされている。
