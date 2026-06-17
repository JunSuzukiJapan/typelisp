# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-06-17 / ブランチ: `feature/typed-lisp-impl`

このドキュメントは、再実装（read 関数から作り直し）の進捗と次回の作業を記録する。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

---

## 1. 進捗（コミット済み）

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

**テスト**: `cargo test` で mem 28 / read 21 / type 9 / check 50 / error 10 / namespace 19 / eval 58 / macro 19 = 214 件 green、警告0（clippy 含む）。
**Miri**: `cargo +nightly miri test --test mem_test`（26/28、重い2件除外）— `Value::Path` 追加後も UB/リーク無し。
`eval_test` はフル実行だと（再帰系テスト等で）数分単位になるため、`loop`/`break`/`return` と実行時 `Sexpr`/cons ヒープ
連携に関わる14件を `cargo +nightly miri test --test eval_test -- <該当テスト名...>` で個別検証— 全件 green、UB/リーク無し。

### 確定仕様ドキュメント
- [language-design.md](language-design.md) — 言語の基本方針、メモリモデル/GC、`::`/module/型の関係、特殊形・関数カタログ（Rust 組み込み vs typelisp）、defmethod、defstruct、エラー処理（Result/Never/panic!）。

### 主要ファイル
- `src/mem/value.rs` — `Value`（`Path` 追加）/ `ConsRef` / `SymId` / `StrId` / `PathId` / `Cell`
- `src/mem/heap.rs` — `Heap`（割当・car/cdr・set・GC・シンボル・文字列・パス intern・`list_to_vec`・ルート）
- `src/read/reader.rs` — `Reader::read` / `read_all`（`::` を top-level で分割し `Value::Path` 生成）
- `src/types.rs` — `Type`（`Never` 追加）/ `parse_type`（Path/`!` 対応）
- `src/check/ast.rs` — `Typed` / `Expr`（`Call`/`Assoc`/`Panic` 等）/ `Pattern` / `Arm`
- `src/check/registry.rs` — `AdtDef`(assoc 付)/`Variant`/`FnSig`/`AssocFn`/`Registry`（組み込み Option/Result/Error/Sexpr、modules/aliases）
- `src/check/checker.rs` — `Checker`（`check_form` 入口、`ns` 状態、名前解決 現NS→root、defun/defstruct/module/defmethod/use、双方向検査・単段具体化・網羅性・Never 適合）
- `src/eval/value.rs` — `RtValue`（実行時値、構成子インスタンス、`Sexpr` は cons ヒープ参照）/ `EvalError`（`Panic`・`Break`/`Return` 内部シグナル等）
- `src/eval/interp.rs` — `Interp`（`exec`/`eval` は `&mut Heap` を受け取る、関数・メソッドレジストリ、パターン照合、i32 組み込み演算、スロット弱参照レジストリ + `sync_roots` で GC ルート管理）
- `src/errors.rs` — `Error`（`HeapExhausted`/`NotACons`/`ImproperList`/`TypeError` 等、全バリアント Display 実装）
- `tests/{mem,read,type,check,error,namespace,eval}_test.rs`

### ビルド注意
- `inkwell` は manifest から一旦除外（ロック可能な版に `llvm18-0` feature が無かったため）。
  `compile` 経路を実装するとき、インストール済み LLVM に合う `llvmNN-0` で再追加する。
- `compile` feature は現状 空（宣言のみ）。旧 `src/compile/*` は `#[cfg(feature="compile")]` 配下で
  既定ビルドから除外（中身は旧 Object 前提なので将来書き直し対象）。

---

## 2. ステップ3：型システム + match（**完了**）

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

## 2.5 エラー処理 + 名前空間（**完了**・型検査レベル）
確定仕様は [language-design.md](language-design.md)。実装済み（すべて型検査レベル、eval は未着手）:
- **エラー処理**: `Result<T,E>`/`Error` 組み込み、`Never` 型(`!`)、`panic!` 特殊形（任意の期待型に適合）。`?`/try は無し。
- **`::`/名前空間**: reader が `::` を `Value::Path` に分割。**型は名前空間でなく Rust 同様**（型は assoc 関数/メソッドを持つ）。
  `module`/`use`、`defstruct`（直和形・非ジェネリック）、`defmethod`（インスタンス `(self T)` / static `(T ...)`）、
  インスタンス・ディスパッチ（第一引数型）と `Type::method` 静的呼び出し、裸名解決 現NS→root。
- **未実装（後続）**: ジェネリック構造体/受け手、`use a::b`（モジュール名を現NSに alias として導入）、可視性、関数カタログの実装本体（eval 待ち）。

## 3. ステップ4：eval（**4a/4b 実装済み**）
- **型付き AST（`check::Typed`）上のツリーウォークインタプリタ**（`src/eval/`、既定の実行経路）。
  - 実装済み（4a）: `RtValue`（リテラル＋構成子インスタンス `Data{type,variant,fields}`）、リテラル/`var`/`if`/`let`/
    `call`（defun）/`construct`/`match`（パターン照合）/`assoc`（インスタンス・static メソッド）/`panic!`（`EvalError::Panic`）。
    `TopLevel::{Defun,Defmethod,Defstruct,Module,Use,Expr}` を `Interp::exec` で処理。関数/メソッドは FQ 名で解決。
  - 実装済み（4b）: 組み込み i32 算術/比較（`+ - * / mod < <= > >= = /=`）。`/`/`mod` のゼロ除算は panic。
  - 実装済み（4c）: 派生制御特殊形 `when`/`unless`/`and`/`or`/`cond`/`let*`（`if`/`let` へ脱糖、AST/eval 追加なし）。
  - 実装済み（4d）: 可変ローカル変数 `setf` ＋ `while` ループ（AST に `Set`/`While`、eval 環境を `Rc<RefCell>` の可変スロット化）。
  - 実装済み（4e）: グローバル定義 `defvar`（可変）/`defconstant`（不変）。名前空間に属し（FQ パス）、関数本体からも参照可。
    AST に `Global`/`SetGlobal`、`Interp.globals`。型注釈 `(name Type)` は任意。`setf` はグローバルにも対応（定数は拒否）。
  - 実装済み（4f）: `lambda`/クロージャ。`RtValue::Closure`（捕捉した可変スロットを共有＝真のクロージャ）。
    AST に `Lambda`/`Apply`。関数値の呼び出しは頭がローカル/グローバル変数・任意の式のとき `Apply` にディスパッチ。
  - 実装済み（4g）: 名前付き関数の値化（`Expr::FnRef`、組み込みは `RtValue::Builtin`）＋ `dotimes`（`let`+`while`+`setf` へ脱糖）。
  - 実装済み（4h）: `cons`/`car`/`cdr`（`Sexpr` 上、`FnSig` 登録で値化も可）。`car`/`cdr` は非 `Cons`（`Nil` 含む）で panic。
    `list`（`(Cons e1 (Cons e2 (... (Nil))))` へ脱糖）／`dolist`（`let`+`while`+`match` で `Cons`/`Nil` を辿る脱糖、結果は `Unit`）。
  - 実装済み（4i）: `loop`/`break`/`return` の非局所脱出（CL 流: 両方とも**直近のループのみ**を脱出。`break` は値なし、
    `return [value]` は値任意で `loop`/`while`/`dotimes`/`dolist` いずれも脱出可。`lambda` 境界は越えない）。
    `Checker.loop_stack`（`break`/`return` の値型を `join_types` で蓄積、`while`系は `Unit` で seed、`loop` は `Never` で
    seed）、eval は `EvalError::Break`/`Return` を内部シグナルとして `?` 伝播、`while`/`loop` だけがそれを捕捉して
    `Step::{Value,Exit}` に解決（`eval_loop_step`/`eval_loop_body`）。
  - 実装済み（4j）: 実行時 `Sexpr` 値を cons ヒープへ統合。`RtValue::Sexpr(mem::Value)` を追加し、`Sexpr` の
    `Construct`/`Match`/`cons`/`car`/`cdr` はすべて `Heap` 経由（`mem::Value` が `Sexpr` と同型なのでスカラー変種は
    そのまま、`Cons`/`Str`/`Sym` は `heap.cons`/`alloc_string`/`intern_symbol`）。GC ルート管理は「全スロットを弱参照で
    登録 → `heap.cons` 直前に `sync_roots` で生存スロットから `Sexpr` 値を再収集してルート差し替え」方式
    （`Interp.slots: Vec<Weak<..>>` + `rooted: Cell<usize>`）。`eval_args` は評価した各引数をスロットに包んで返し
    （`(Vec<RtValue>, Vec<Slot>)`）、呼び出し元がそのスロット列を保持する間だけ GC 安全になる。
    Sexpr 以外のADT（`Option`/`Result`/`defstruct`）は従来通り `RtValue::Data`（Rust ヒープ）のまま
    （設計上 GC 対象は cons セル/シンボル/文字列のみ）。
  - TDD: `tests/eval_test.rs`（58件、`runtime_cons_cells_survive_gc_when_rooted` で 2 セルの極小ヒープに対して
    GC を強制発生させルート保護を実地検証）／`tests/check_test.rs`（50件）。
  - 実装済み（4k）: **CL 流マクロ**（`defmacro`、非衛生的、`Sexpr` をコードとして扱う。Scheme 流の衛生的
    `syntax-rules` ではなく CL 流を採用 — 既存の `Sexpr`/`read`/`eval`/cons heap をそのまま再利用でき、
    プロジェクト全体の「Rust 流の実用混在」方針と一貫するため）。
    - `quote`（`'x` → `(quote x)`、`Expr::Quote(QuotedSexpr)`。`QuotedSexpr` は heap 非依存の owned 列挙体
      — `Expr`/`Typed` は `Interp.fns` に**プログラム全体の生存期間**保持されるが `sync_roots` は `slots` しか
      見ないため、生 `mem::Value` ポインタを埋め込むと GC から見えなくなる。評価ごとに heap へ**新規**割当）。
    - `quasiquote`/`unquote`（`` `x ``/`,x`。ランタイムマクロではなく純粋な checker 内 desugar — `unquote` の
      引数は使用箇所の `env` で `Sexpr` 型として検査されるので、ローカル変数を参照できる。`,@`（unquote-splicing）
      は `append` 未実装のため対象外）。
    - `gensym`（フレッシュな `Sexpr::Sym` を返す組み込み。symbol は常に intern される言語仕様のため、CL の
      unforgeable な未 intern シンボルではなく衝突耐性のある名前のみ）。
    - `defmacro`（`(defmacro name (p1 p2 ...) body...)`、パラメータは型注釈なしの裸名＝常に `Sexpr`。`Namespace.macros`
      に登録。マクロ呼び出しは checker の `check_list` で（特殊形・ローカル変数の次、構成子解決の前に）検出し、
      **未評価の**引数フォームをそのまま `MacroExpander::expand_macro` に渡して展開、展開結果を再帰的に検査
      — これが非衛生的（CL 流）たる所以。`MacroExpander` トレイトは `check` 側で定義し `Interp` が実装（`eval`→`check`
      の既存依存を一方向に保つため）。マクロの本体は `defun` と全く同じ `Interp.fns` テーブルに登録され、展開時の
      呼び出しは通常の関数呼び出しと同一機構（新規ランタイム機構は不要））。
    - **GC root 規律**: `Interp::expand_macro` は `apply` 呼び出し前後で heap の root stack を完全に元に戻す
      （`apply` が `sync_roots` を何度呼んでも、その分を先に pop → 自分の `raw_args` root を pop、の順序厳守）。
      checker 側も展開結果を `push_root`/`pop_root` で挟んで保護。結果として `main.rs` の既存「pop back to mark」
      一括処理は無変更で安全（マクロ展開は root stack に対して中立的）。ただし `defmacro` 自体は
      checker でチェック完了後**即座に** `interp.exec` する特例が必要（同一ペースト内の後続フォームがそのマクロを
      使うには、checker が macro 展開する時点で本体が `Interp.fns` に登録済みでなければならないため）。
    - 制約: マクロは前方参照不可（`defun` と同じ既存制約と一貫）、unquote の引数は `Sexpr` 型必須（暗黙の型変換なし）。
    - TDD: `tests/macro_test.rs`（19件。非衛生性を明示的に実証する `swap!` 例＋`gensym` での修正例、
      マクロ展開中に GC を強制発生させる回帰テストを含む）。
- **次の候補（eval 拡充）**:
  - 組み込み関数の拡張（型ごとの算術／i64・f64、文字列・ベクタ・Option/Result ライブラリ関数）。カタログは [language-design.md](language-design.md) §4。
  - typelisp ライブラリ関数（`length`/`append`/`reverse`/`map`/`filter`/`foldl`/`foldr` 等、`Sexpr` 上）。`append` 実装後、
    マクロの `,@`（unquote-splicing）を追加できる。
  - `case`/`do`/`doiter`/`while-let`/`the` は未実装。`defmacro` の `&rest`／構造化ラムダリストも未実装。
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
