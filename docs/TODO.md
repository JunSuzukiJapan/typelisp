# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-06-14 / ブランチ: `feature/typed-lisp-impl`

このドキュメントは、再実装（read 関数から作り直し）の進捗と次回の作業を記録する。

---

## 0. 言語仕様（確定事項）

- **静的型付け**: すべての式が静的型を持つ。動的タグ付き Lisp（旧 3ab5599 の `Object`）ではない。
- **文法は macro-lisp 準拠**: `/Users/suzukijun/Program/Rust/macro-lisp` の**構文**に従う（実装は参考にしない）。
  `defun`/`defstruct`/`defvar`/`defconstant`/`let`/`if`/`when`/`unless`/`cond`/`match`/`loop`/`while`/`dotimes`/`do`/`doiter`/`while-let`/`if-let`/`lambda`/`progn`/`module` 等。関数の引数・戻り型は必須、局所束縛は型推論可。
- **真偽値は `true`/`false`**。`nil`/`t` は言語に存在しない。
- **`nil` の代替は `Option<T>`**（`(defstruct Option (Some (value T)) (None))` 相当の直和型）。
- **`read` の戻り値は組み込み直和型 `Sexpr`**。型レベルでは `Option<Sexpr>`（`None` = 空リスト `()`）。
  `Sexpr = Int|Float|Char|Bool|Sym|Str|Cons(Option<Sexpr>, Option<Sexpr>)`。
  car も cdr も `Option<Sexpr>`（`()` が要素にも来るため。例: `(defun f () ...)` の空引数列）。
  専用の `Nil` 構成子は型レベルには持たず、実行時は `None` を `Value::Empty` で符号化。
- **大文字小文字は区別しない**（シンボルは小文字に正規化してインターン）。
- **Rust 相互運用はしない**（`&args[1]`, `env::args().collect()` 等は対象外）。
- **構成子パターンは S 式形** `(Some v)` / `(Cons a d)`。
- **実行モデル**: 既定はインタプリタ（eval）。**ネイティブコンパイルは明示的 `compile`/`compile-file`（CL 準拠）を呼んだ時だけ**。LLVM コンパイラは feature gate。

## 1. メモリモデル / GC（確定・実装済み）

- cons セルは**固定アリーナ**（起動時に確保、再確保しない＝生ポインタが安定）。将来 `--heap-cells N` で容量指定。
- 割当はフリーリストから。空なら GC、それでも空なら **`Error::HeapExhausted`（成長しない）**。
- **mark-sweep GC**（反復マーク＝深い構造でもスタック溢れなし、循環回収）。ルート集合 `push_root`/`pop_root`。
- **生ポインタは `ConsRef` に隠蔽、公開 API は安全**。
- シンボルはインターン（小文字正規化・永続）。文字列は GC 管理（到達可能のみ生存）。

---

## 2. 進捗（コミット済み）

| コミット | 内容 |
|---|---|
| `2e4c749` | revert: M0–M14 を 3ab5599 相当へ巻き戻し（旧実装は `backup/typed-lisp-m14` に保全） |
| `5f8812c` | cons ヒープ + mark-sweep GC（test-first）、compile を feature gate、LLVM 無しで core ビルド可 |
| `24199e4` | `Value` を Sexpr 表現に（`Empty`/`Bool`/`Symbol`/`Str`）、文字列 GC、シンボル大小無視 |
| `92bd216` | CL 風 reader → Sexpr（read/read_all、rooting で GC 安全）、旧 Object/eval/旧テスト撤去 |
| `8143242` | `Type` 表現 + 型パース（3a） |
| `8a8e211` | 型付き AST + 型検査器（3b/3c）: defun/let/if/literal/var/call/構成子/match/if-let、組み込み直和型 Option/Sexpr |
| `aec77f3` | clippy 警告解消（Error の Display 実装、Reader の Default） |
| （未コミット） | エラー処理（Result/Error/Never/`panic!`）+ 名前空間（`::`→`Value::Path`、module/use、Rust 流の型/メソッド: defstruct/defmethod、インスタンス・static ディスパッチ） |

**テスト**: `cargo test` で mem 28 / read 21 / type 9 / check 22 / error 10 / namespace 11 / eval 15 = 116 件 green、警告0（clippy 含む）。
**Miri**: `cargo +nightly miri test --test mem_test`（26/28、重い2件除外）— `Value::Path` 追加後も UB/リーク無し。

### 確定仕様ドキュメント
- [language-design.md](language-design.md) — `::`/module/型の関係、特殊形・関数カタログ（Rust 組み込み vs typelisp）、defmethod、defstruct、エラー処理（Result/Never/panic!）。

### 主要ファイル
- `src/mem/value.rs` — `Value`（`Path` 追加）/ `ConsRef` / `SymId` / `StrId` / `PathId` / `Cell`
- `src/mem/heap.rs` — `Heap`（割当・car/cdr・set・GC・シンボル・文字列・パス intern・`list_to_vec`・ルート）
- `src/read/reader.rs` — `Reader::read` / `read_all`（`::` を top-level で分割し `Value::Path` 生成）
- `src/types.rs` — `Type`（`Never` 追加）/ `parse_type`（Path/`!` 対応）
- `src/check/ast.rs` — `Typed` / `Expr`（`Call`/`Assoc`/`Panic` 等）/ `Pattern` / `Arm`
- `src/check/registry.rs` — `AdtDef`(assoc 付)/`Variant`/`FnSig`/`AssocFn`/`Registry`（組み込み Option/Result/Error/Sexpr、modules/aliases）
- `src/check/checker.rs` — `Checker`（`check_form` 入口、`ns` 状態、名前解決 現NS→root、defun/defstruct/module/defmethod/use、双方向検査・単段具体化・網羅性・Never 適合）
- `src/eval/value.rs` — `RtValue`（実行時値、構成子インスタンス）/ `EvalError`（`Panic` 等）
- `src/eval/interp.rs` — `Interp`（`exec`/`eval`、関数・メソッドレジストリ、パターン照合、i32 組み込み演算）
- `src/errors.rs` — `Error`（`HeapExhausted`/`NotACons`/`ImproperList`/`TypeError` 等、全バリアント Display 実装）
- `tests/{mem,read,type,check,error,namespace,eval}_test.rs`

### ビルド注意
- `inkwell` は manifest から一旦除外（ロック可能な版に `llvm18-0` feature が無かったため）。
  `compile` 経路を実装するとき、インストール済み LLVM に合う `llvmNN-0` で再追加する。
- `compile` feature は現状 空（宣言のみ）。旧 `src/compile/*` は `#[cfg(feature="compile")]` 配下で
  既定ビルドから除外（中身は旧 Object 前提なので将来書き直し対象）。

---

## 3. ステップ3：型システム + match（**完了**）

承認済み方針「A: 型検査器の骨組み + 組み込み直和型（`Sexpr`/`Option`）+ `match`/`if-let` を最小構成」。
**最小ゴール達成**: `Option`/`Sexpr` に対する `match` を型検査できる（`unwrap-or` が通る／非網羅・型不一致はエラー）。
入口は `Checker::check_form(&Heap, Value) -> Result<TopLevel, Error>`。

実装メモ:
- **双方向検査**: `check(env, v, expected: Option<&Type>)`。整数/浮動小数リテラルと構成子（特に引数なし `None`）は
  `expected` を採用、それ以外は合成型を `expected` と突き合わせ。整数リテラル既定は `i32`、浮動小数は `f64`。
- **`()` の二面性**: `expected` が `Option<T>` のとき空リスト `()` は `None` 構成子として検査、その他は `Unit`。
- **単段具体化 + 単一化**: 構成子適用は `unify`/`subst_apply` でフィールド型テンプレートを実引数型と単一化し型引数を推論。
- **網羅性**: 全構成子被覆 or `_`/束縛の包括アームを要求。`if-let` は二腕 `match`（包括アーム付き）へ脱糖。
- 未実装: `while-let`（eval/loop 側）、`defstruct`（ユーザ定義直和型の登録 API は `Registry::add_adt` で準備済）、算術等の組み込み関数。

<details><summary>当初設計（参考）</summary>

### 3b: Sexpr → 型付き AST + 検査器の骨組み
- 型付き AST（`Sexpr` を head シンボルで振り分けて構築）:
  ```
  Typed { expr: Expr, ty: Type }
  Expr = Int|Float|Bool|Char|Str|Unit | Var(name)
       | If(c,t,e) | Let(binds, body) | Call(name, args)
       | Construct(type, variant_idx, args)   // (Some x) (None) (Cons a d)
       | Match(scrutinee, [Arm])              // if-let は Match へ脱糖
  Pattern = Wildcard | Bind(name) | Lit(..) | Ctor(name, idx, [Pattern])
  ```
- 直和型レジストリ（組み込み）:
  - `Option<T>`: `Some(T)->Option<T>`, `None->Option<T>`
  - `Sexpr`: `Int(i64)`/`Float(f64)`/`Char(char)`/`Bool(bool)`/`Sym(String)`/`Str(String)`/`Cons(Option<Sexpr>,Option<Sexpr>)`
- 検査: `defun`（型付き引数＋戻り型、本体を戻り型で検査）/ `let` / `if` / リテラル / 変数参照 /
  関数呼び出し（名前→fn 型）/ 構成子適用。環境（var→Type）、関数レジストリ（name→Fn型）。

### 3c: match/if-let/while-let の型付け
- scrutinee 型 `Named("option",[i32])` → レジストリの generics `[t]` を `t↦i32` で**単段具体化**。
- 構成子パターン `(Some v)`: 変種のフィールド型を確定し束縛（`v: i32`）。`_`/束縛/リテラルも。
- **網羅性**: 全構成子を覆う or `_`/束縛の包括アームが必要（Rust 同様）。
- 各腕本体は共通型 `R` に。`if-let (pat = e) then else` は単一パターン版（網羅性不要、`else` が残りを覆う）。
- TDD: 型検査の期待結果テストを先に（`tests/check_test.rs`）。

### 設計上の注意 / 未決事項
- ジェネリック具体化は単段で十分か（`Vec<Option<i32>>` のネストは後で）。
- リテラルの型（整数リテラルの既定型・型注釈による上書き）の規則は eval/checker で要設計。
- 型名は小文字正規化される点に注意（`String`→`string`, 型変数 `T`→`t`）。
</details>

## 3.5 エラー処理 + 名前空間（**完了**・型検査レベル）
確定仕様は [language-design.md](language-design.md)。実装済み（すべて型検査レベル、eval は未着手）:
- **エラー処理**: `Result<T,E>`/`Error` 組み込み、`Never` 型(`!`)、`panic!` 特殊形（任意の期待型に適合）。`?`/try は無し。
- **`::`/名前空間**: reader が `::` を `Value::Path` に分割。**型は名前空間でなく Rust 同様**（型は assoc 関数/メソッドを持つ）。
  `module`/`use`、`defstruct`（直和形・非ジェネリック）、`defmethod`（インスタンス `(self T)` / static `(T ...)`）、
  インスタンス・ディスパッチ（第一引数型）と `Type::method` 静的呼び出し、裸名解決 現NS→root。
- **未実装（後続）**: ジェネリック構造体/受け手、module 全体取り込み `use`、可視性、関数カタログの実装本体（eval 待ち）。

## 4. ステップ4：eval（**4a/4b 実装済み**）
- **型付き AST（`check::Typed`）上のツリーウォークインタプリタ**（`src/eval/`、既定の実行経路）。
  - 実装済み（4a）: `RtValue`（リテラル＋構成子インスタンス `Data{type,variant,fields}`）、リテラル/`var`/`if`/`let`/
    `call`（defun）/`construct`/`match`（パターン照合）/`assoc`（インスタンス・static メソッド）/`panic!`（`EvalError::Panic`）。
    `TopLevel::{Defun,Defmethod,Defstruct,Module,Use,Expr}` を `Interp::exec` で処理。関数/メソッドは FQ 名で解決。
  - 実装済み（4b）: 組み込み i32 算術/比較（`+ - * / mod < <= > >= = /=`）。`/`/`mod` のゼロ除算は panic。
  - TDD: `tests/eval_test.rs`（15件: unwrap-or・メソッド・factorial・fib・ゼロ除算 panic 等）。
- **次の候補（eval 拡充）**:
  - 組み込み関数の拡張（型ごとの算術／i64・f64、文字列・リスト・Option/Result ライブラリ関数）。カタログは [language-design.md](language-design.md) §3。
  - 残りの特殊形（`cond`/`when`/`unless`/`and`/`or`/`while`/`loop` 等）、`lambda`/クロージャ。
  - クロージャ・実行時値と GC の整合（現状 `RtValue` は Rust ヒープ上で完結、cons ヒープ非依存）。
- **ステップ5: compile**（明示 `compile`/`compile-file`。inkwell 再追加・LLVM コード生成。feature gate）。

---

## 開発コマンド
```sh
cargo test                                   # core（LLVM 不要）
cargo +nightly miri test --test mem_test     # GC/ポインタの UB・リーク検査
cargo +nightly miri test --test read_test
cargo run                                    # 最小 main（defun を read）
```
旧実装参照: `git log backup/typed-lisp-m14` / 構文参照: `/Users/suzukijun/Program/Rust/macro-lisp`
