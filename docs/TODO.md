# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-06-25 / ブランチ: `feature/compiler`

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
| （未コミット） | エラー処理（Result/Error/Never/`panic`）+ 名前空間（`::`→`Value::Path`、module/use、Rust 流の型/メソッド: defstruct/defmethod、インスタンス・static ディスパッチ） |

**テスト**: `cargo test` で mem 28 / read 21 / type 9 / check 50 / error 10 / namespace 25 / eval 58 / macro 25 /
hashtable 12 / vector 15 / string 24 / numeric 20 / prelude 62 = 359 件 green、警告0（clippy 含む）。
旧来あった3件の既存警告（`check_assoc_call`/`check_construct` の too_many_arguments、`alloc_quoted` の
only_used_in_recursion）は解消済み: 前者2件は `(type_fq, method)`/`(adt_name, variant)` をタプル1引数に
まとめてアリティを減らし、後者は `self` を使わず再帰のみに使っていたため `Interp` のメソッドから
モジュール直下の自由関数に変更（`env_get` 等と同じ並び）。
**Miri**: `cargo +nightly miri test --test mem_test`（26/28、重い2件除外）— `Value::Path` 追加後も UB/リーク無し。
`eval_test` はフル実行だと（再帰系テスト等で）数分単位になるため、`loop`/`break`/`return` と実行時 `Sexpr`/cons ヒープ
連携に関わる14件を `cargo +nightly miri test --test eval_test -- <該当テスト名...>` で個別検証— 全件 green、UB/リーク無し。

### 確定仕様ドキュメント
- [language-design.md](language-design.md) — 言語の基本方針、メモリモデル/GC、`::`/module/型の関係、特殊形・関数カタログ（Rust 組み込み vs typelisp）、defmethod、defstruct、エラー処理（Result/Never/panic）。

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
- `compile` feature（LLVM/inkwell ベースの JIT コンパイラ）は 2026-06-23 に全面リバート済み
  （`Vector<T>`/`defstruct` の再設計に伴う削除——本ファイル「ステップ5: compile」参照）。
  `Cargo.toml` に `inkwell` 依存・`compile` feature は存在しない。`cargo build`/`cargo test` に
  feature 指定は不要（単一構成）。

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
- **エラー処理**: `Result<T,E>`/`Error` 組み込み、`Never` 型(`!`)、`panic` 特殊形（任意の期待型に適合）。`?`/try は無し。
- **`::`/名前空間**: reader が `::` を `Value::Path` に分割。**型は名前空間でなく Rust 同様**（型は assoc 関数/メソッドを持つ）。
  `module`/`use`、`defstruct`（2026-06-23に全面削除後、**再設計・再実装完了**——単一variant
  `"new"`固定・`AdtKind::Struct`・`RtValue::Struct`(`Rc<RefCell<..>>`で可変)、フィールド読み書きは
  `変数::フィールド名`/`(setf 変数::フィールド名 v)`、ジェネリック`defstruct`も対応、
  [[typelisp-vector-defstruct-revert]]参照）、`defmethod`（インスタンス `(self T)` / static
  `(T ...)`）、インスタンス・ディスパッチ（第一引数型）と `Type::method` 静的呼び出し、
  裸名解決 現NS→root。`Option`/`Result`/`Error`のコンストラクタも同型スコープ化済み
  （`Option::some`等、`(use Option)`で裸名展開可能、`Sexpr`の`nil`/`cons`のみ例外的に裸名維持）。
- **プリミティブ型への `defmethod` 拡張**（[cl-equivalence-catalog.md](cl-equivalence-catalog.md) §0.1）:
  `i8`/`i16`/`i32`/`i64`/`isize`/`u8`/`u16`/`u32`/`u64`/`usize`/`f32`/`f64`/`bool`/`char`/`string`
  （`Type` の全プリミティブ variant、`Unit`/`Never` 除く）も `defmethod` の受け手になれる。
  `types::prim_type_path` がプリミティブ `Type` をレジストリ上の `Path`（例 `Type::I32 → i32`）に
  マップし、`Registry::with_builtins` がそれぞれの空 `AdtDef` を事前登録、`check_defmethod`/
  `check_instance_method` が `Type::Named` と同様にこの `Path` で解決する。trait/動的ディスパッチは
  導入せず、既存の「受け手の静的型で一意に解決する」仕組みを対象範囲だけ広げたもの。
- **assoc呼び出しのジェネリック受け手対応**（[cl-equivalence-catalog.md](cl-equivalence-catalog.md) ステップ3a）:
  `check_assoc_call`（[src/check/checker.rs](../src/check/checker.rs)）が、`check_construct` と同型の
  `subst`/`subst_apply` で、メソッド署名中の型変数（受け手の `def.params`、例 `HashTable<K,V>` の
  `k`/`v`）を具体型へ代入するようになった。インスタンス呼び出しは受け手の確定型から、引数からは
  何も推論できない static 呼び出し（`HashTable::new` 等）は呼び出し元の `expected`（戻り値の期待型）
  から代入元を得る（field-less構成子 `None` が `expected` から型引数を学ぶのと同じ要領）。
  受け手/expected/メソッド識別子は `AssocCall` 構造体にまとめてアリティを抑えている。
- **未実装（後続、影響範囲の大きさで優先順位付け——[[feedback-impl-priority]]）**:
  1. ~~**ユーザ定義のジェネリック `defstruct`**~~ — **完了**（`defstruct`再設計の一部、
     `(defstruct (Name T...) (field Type)...)`、`check_defun`の型パラメータ構文を踏襲）。
  2. **`use a::b`**（モジュール名を現NSに alias として導入）。名前空間機能で、モジュール分割が
     進むほど影響範囲が広がるため次点。
  3. **可視性**（`FnSig::public` 等のフィールドは既に存在するが常に `true` で実効性なし。
     ステップ5 Phase 5 の共有ライブラリ化が `pub` を export 基準に使う計画があるが、現状は
     何もブロックしていない）。
  4. **関数カタログの実装本体（eval 待ち）**: 本節執筆当時（名前空間完成時点）は eval 未着手
     だったための記述で、現在は4a–4sで大半が実装済み。残りは下記「次の候補（eval 拡充）」に
     引き継がれているため、独立項目としては実質解消済み。

## 3. ステップ4：eval（**4a/4b 実装済み**）
- **型付き AST（`check::Typed`）上のツリーウォークインタプリタ**（`src/eval/`、既定の実行経路）。
  - 実装済み（4a）: `RtValue`（リテラル＋構成子インスタンス `Data{type,variant,fields}`）、リテラル/`var`/`if`/`let`/
    `call`（defun）/`construct`/`match`（パターン照合）/`assoc`（インスタンス・static メソッド）/`panic`（`EvalError::Panic`）。
    `TopLevel::{Defun,Defmethod,Defstruct,Module,Use,Expr}` を `Interp::exec` で処理。関数/メソッドは FQ 名で解決。
  - 実装済み（4b）: 組み込み i32 算術/比較（`+ - * / mod < <= > >= = /=`）。`/`/`mod` のゼロ除算は panic。
  - 実装済み（4c）: 派生制御特殊形 `when`/`unless`/`and`/`or`/`cond`/`let*`（`if`/`let` へ脱糖、AST/eval 追加なし）。
    **訂正（2026-06-22）**: `when`/`unless`/`and`/`or`/`cond`（`let*`を除く）はステップ5
    Phase4でchecker特殊形から`prelude.rs`の`defmacro`へ移行済み——「`if`/`let`へ脱糖」という
    挙動自体は変わらないが、脱糖を行う場所がRustの`check_*`関数からtypelispのマクロ展開に移った。
  - 実装済み（4d）: 可変ローカル変数 `setf` ＋ `while` ループ（AST に `Set`/`While`、eval 環境を `Rc<RefCell>` の可変スロット化）。
    **訂正（2026-06-22）**: `while`はステップ5 Phase4で`loop`+`if`+`break`への`defmacro`に
    移行し、AST の`Expr::While`バリアント自体を削除した（`setf`/`Expr::Set`は不変）。
  - 実装済み（4e）: グローバル定義 `defvar`（可変）/`defconstant`（不変）。名前空間に属し（FQ パス）、関数本体からも参照可。
    AST に `Global`/`SetGlobal`、`Interp.globals`。型注釈 `(name Type)` は任意。`setf` はグローバルにも対応（定数は拒否）。
  - 実装済み（4f）: `lambda`/クロージャ。`RtValue::Closure`（捕捉した可変スロットを共有＝真のクロージャ）。
    AST に `Lambda`/`Apply`。関数値の呼び出しは頭がローカル/グローバル変数・任意の式のとき `Apply` にディスパッチ。
  - 実装済み（4g）: 名前付き関数の値化（`Expr::FnRef`、組み込みは `RtValue::Builtin`）＋ `dotimes`（`let`+`while`+`setf` へ脱糖）。
    **訂正（2026-06-22）**: `dotimes`もPhase4で`defmacro`化（`let`+`while`マクロの組み合わせ、
    `gensym`で上限変数を作る）。
  - 実装済み（4h）: `cons`/`car`/`cdr`（`Sexpr` 上、`FnSig` 登録で値化も可）。`car`/`cdr` は非 `Cons`（`Nil` 含む）で panic。
    `list`（`(Cons e1 (Cons e2 (... (Nil))))` へ脱糖）／`dolist`（`let`+`while`+`match` で `Cons`/`Nil` を辿る脱糖、結果は `Unit`）。
    **訂正（2026-06-22）**: `dolist`もPhase4で`defmacro`化（`consp`/`car`/`cdr`を使う
    `let`+`while`マクロ、`match`は使わなくなった）。
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
  - 実装済み（4l）: `defmacro` の `&rest`（[cl-equivalence-catalog.md](cl-equivalence-catalog.md) で
    見つかった前提 — `case`/`until`/`while-let`/`do` 等の可変個引数マクロに必要。`apply` 用の
    **型付き**可変長引数とは別物で、型は常に `Sexpr` 固定のため実装コストは小さい）。
    - パラメータリスト末尾の `&rest name` を `Checker::check_defmacro` で検出し、`MacroDef`/
      `Checker::resolve_macro` を `arity`（固定引数数）+ `rest: bool` に拡張。呼び出し側の引数数検査は
      `rest` なら「`arity` 以上」、無しなら従来通り「`arity` と一致」。
    - `Interp::expand_macro` から呼ぶ新規 `Interp::bind_macro_args` が、固定引数を1:1で
      `RtValue::Sexpr` に変換した後、残りの生の呼び出しフォームを `heap.cons` で1本のリストに
      集約し（`Self::alloc_quoted` の `Cons` ケースと同じ push_root/pop_root 規律）、`name` パラメータに
      `Sexpr` リストとして束縛する。`apply` 自体は不変（params/argsを1:1で zip するのみ）。
    - TDD: `tests/macro_test.rs` に6件追加（固定引数なし/ありでの収集、空 `&rest`、引数不足のエラー、
      `&rest` が末尾以外だとエラー、GC強制発生下での収集の回帰テスト）。
  - 実装済み（4m）: `HashTable<K,V>`（[cl-equivalence-catalog.md](cl-equivalence-catalog.md) ステップ3b）。
    - 型登録: `registry.rs` の `hashtable_def()` が `Option`/`Result` と同型で `params: ["k","v"]`、
      `variants: []`、`assoc` に `new`（static）/`get`/`set`/`remove`/`count`/`clear`（instance）の
      メタデータを直接登録（`defmethod` の本体は無い、組み込み）。
    - ランタイム表現: `RtValue::HashTable(Rc<RefCell<HashMap<HashKey, RtValue>>>)`。GC管理の cons
      ヒープには一切触れず、`Option`/`Result`/ユーザ `defstruct`（`RtValue::Data`）と同じ「Rustの
      通常の所有権で管理」という既存方針をそのまま踏襲（`mem::Heap` 自体の変更は不要だった —
      カタログが想定していたより GC統合は大幅に小さく済んだ）。キーは `HashKey`
      （`Int`/`Bool`/`Char`/`Str` の4種のみ。`f64` は `Eq` が無いため、`Closure`/`HashTable`自身等は
      構造的等価性が無意味なため対象外）に限定し、非対応のキー型は `EvalError::Panic`（`car`/`cdr` が
      非Consでpanicするのと同じ「型システムが追い切れない所はランタイムpanic」という既存方針）。
    - 実行時ディスパッチ: `Expr::Assoc` の評価は従来 `Interp::methods`（ユーザの`defmethod`）しか
      見ていなかったので、`eval_builtin`（自由関数の組み込み実装）と同型の新規 `eval_builtin_method`
      をフォールバックとして追加（ユーザ定義 → 組み込みの優先順位は `Expr::Call` と同じ）。
    - GC統合: `collect_sexpr_roots`（`sync_roots` が使う再帰関数）に `RtValue::HashTable` のケースを
      1つ追加し、値に含まれる `RtValue::Sexpr` を再帰的に辿るだけで済んだ（キー側は `HashKey` が
      `Sexpr` を持ち得ないため不要）。GC強制発生下でのテストで検証済み（一旦このケースを外して
      テストが実際に失敗する/値が壊れることを確認した上で復元——`tests/hashtable_test.rs` の
      `sexpr_values_survive_gc_pressure`）。
    - 既知の制約（対応しない、ドキュメントに明記）: `Rc<RefCell<..>>` 方式のため、HashTableが自分
      自身を値として保持する等の循環参照はmark-sweepと違い回収されない —
      既存の`RtValue::Closure`の捕捉環境（同じ`Rc<RefCell<Slot>>`方式）が元から持つ制約と同じで、
      新規に持ち込むものではない。`to-list`/`keys`/`values` は対象外（`V`が任意の`RtValue`のため
      `Sexpr`への汎用変換が部分関数になり、カタログ自身も「最小限プリミティブ」と留保しているため、
      本筋のCRUDと独立に後続で検討）。
    - これに先立ち、ステップ3aで `check_assoc_call` にジェネリック受け手対応を追加済み（上記2.5節）。
    - TDD: 新規 `tests/hashtable_test.rs`（12件。CRUD一式、static呼び出しの型推論、K/Vが異なる複数
      インスタンスの相互非干渉、キー型不一致の型エラー、GC回帰テスト）。
  - 実装済み（4n、**2026-06-23に全面削除・後日再設計予定**——
    [[typelisp-vector-defstruct-revert]]参照）: `Vector<T>`（[cl-equivalence-catalog.md](cl-equivalence-catalog.md) ステップ4）。
    HashTableと完全に同型のパターンで実装（ステップ3で確立した「組み込みnominal型は`registry.rs`に
    `AdtDef`を直書き + `RtValue`に`Rc<RefCell<..>>`変種を追加 + `eval_builtin_method`にディスパッチを
    1分岐追加 + `collect_sexpr_roots`に1ケース追加」という型に3a/3bのインフラがそのまま乗った）。
    - 型登録: `registry.rs` の `vector_def()`、`params: ["t"]`、`assoc` に `new(i32,t)->Vector<t>`
      （static）/ `get(Vector<t>,i32)->t` / `set(Vector<t>,i32,t)->()` / `length(Vector<t>)->i32` /
      `push(Vector<t>,t)->()` / `pop(Vector<t>)->Option<t>`（instance）。`get`/`set`は`Option`で
      包まずTを直接返す/受ける — 範囲外（負数含む）は`car`/`cdr`の非Consと同じ「型システムが
      追えない所はランタイムpanic」方針でpanic。
    - ランタイム表現: `RtValue::Vector(Rc<RefCell<Vec<RtValue>>>)`。HashTableと同じくRustの通常の
      所有権で管理、`mem::Heap`は無変更。
    - 実行時ディスパッチ: `eval_builtin_method`を`hashtable`/`vector`の2分岐に拡張（共通の
      ユーザ定義→組み込みフォールバック順序は`Expr::Assoc`評価側で既存のまま）。
    - GC統合: `collect_sexpr_roots`に`RtValue::Vector`のケースを1つ追加（要素を再帰的に辿る）。
      HashTableと同じ手順でテストが実際に修正前は失敗する/値が壊れることを確認した上で復元
      （`tests/vector_test.rs`の`sexpr_values_survive_gc_pressure`）。
    - 既知の制約: HashTableと同じ循環参照リークの受容、`to-list`/`from-list`は対象外
      （要素型`T`が任意の`RtValue`になり得るため`Sexpr`への汎用変換が部分関数になる、という
      HashTableの`to-list`/`keys`/`values`除外と同じ理由）。
    - TDD: 新規 `tests/vector_test.rs`（15件。CRUD一式、static呼び出しの型推論、範囲外/負数
      インデックスのpanic、要素型が異なる複数インスタンスの相互非干渉、要素型不一致の型エラー、
      GC回帰テスト）。
  - **`Vector<T>`/`defstruct`全面削除 + `&rest`のSexpr化（2026-06-23）**:
    `RtValue::Vector`という専用enumバリアントの存在自体が「ユーザー定義型と同様に扱うべき」
    という設計原則に反すると判明し、議論の結果`Vector<T>`と`defstruct`（フィールド読み書き
    手段が無い不完全な設計と判明）を両方削除、再設計は後日に持ち越した
    （[[typelisp-vector-defstruct-revert]]参照）。副次的に`compile`機能全体も削除（同機能の
    typelisp本体`compiler_source.rs`が`Vector<T>`を内部データ構造として使っていたため）。
    - **`&rest`（`defun`/`lambda`の可変長引数）を`Sexpr`ベースに再設計**: `&rest`の収集先が
      `Vector<elem>`だったため巻き込まれて壊れた。Web検索で確認した事実：CLを含むあらゆる
      Lispの`&rest`パラメータは常にconsセルの素のリストに束縛され、ベクタではない——
      Lispの本質的特徴は cons セルで構築された異種混在可能なリストであり、ベクタ/配列は
      別の独立したデータ構造（CLにも存在するが`&rest`が使うものではない）。型付きLispでも
      ホモジニアスなリスト型はcons構造自体を型パラメータ化して作る（フラットな配列を別に
      用意するのではない）。これに従い、`&rest (xs T)`の本体内での型は常に`Sexpr`（`defmacro`の
      `&rest`と統一）、呼び出し側での各可変長引数の型チェック（`T`との一致）は維持。
      各要素は`Sexpr`の対応するコンストラクタ（`i32`/`i64`→`Int`等、`Checker::sexpr_ctor_for`）で
      ラップして`cons`で連結（`Checker::wrap_rest_elem`/`cons_rest_list`、`defmacro`の
      `&rest`収集ロジックと同型）。本体内で要素を取り出すには`match`/`car`/`cdr`が必要
      （`(Int n)`は常に`n: i64`——`i32`/`i64`どちらも同じ`Sexpr`variantにラップされるため）。
      `apply`の`rest-list`引数も同様に`Sexpr`一本になり、CLの`apply`同様要素の内容は
      事前チェックしない（リストの中身は呼び出し先の責任）。
  - 実装済み（4o）: 文字列・char の基本プリミティブ（[cl-equivalence-catalog.md](cl-equivalence-catalog.md)
    §2.2 d、ステップ5）。HashTable/Vectorと同じ「メタデータのみのassoc登録 + `eval_builtin_method`
    へのディスパッチ追加」パターンだが、`string`/`char`は**ジェネリックでない既存の組み込み型**
    （ステップ1で受け手として`defmethod`対応済み）なのでステップ3aのような`subst`代入は不要、
    GC統合も不要（`RtValue::Str`/`Char`は`Sexpr`を保持し得ないため`collect_sexpr_roots`の変更なし）
    — ステップ3/4よりさらに単純だった。
    - 型登録: `registry.rs`の`string_assoc()`/`char_assoc()`が、`with_builtins`のプリミティブ登録
      ループ内で`Type::Str`/`Type::Char`の場合だけ非空の`assoc`を渡す（他のプリミティブは従来通り
      空）。`string`: `upcase`/`downcase`/`length`/`ref`/`substring`/`append`/`eq`/`lt`。
      `char`: `upcase`/`downcase`/`eq`/`lt`/`alphap`/`digitp`（命名は2026-06-20に`alpha?`/`digit?`
      から`?`接尾辞を廃して訂正、§7.3参照）。
    - 意図的な設計判断: `length`/`ref`/`substring`の添字は**Unicodeスカラー値(char)単位**
      （バイト単位ではない）。`upcase`/`downcase`はASCII限定
      （`to_ascii_uppercase`/`lowercase`、独語`ß`→`SS`のような長さが変わるUnicode大小変換を回避）。
      `alphap`/`digitp`もASCII限定（CLの`alpha-char-p`/`digit-char-p`の進数無し版に相当）。
      `ref`/`substring`は範囲外（負数含む）・`start > end`をランタイムpanic
      （`car`/`cdr`の非Consと同じ「型システムが追えない所はpanic」方針）。
    - 実行時ディスパッチ: `eval_builtin_method`を`string`/`char`の2分岐をさらに追加（既存の
      `hashtable`/`vector`と並列）。
    - TDD: 新規`tests/string_test.rs`（24件。各メソッドの基本動作、範囲外/不正範囲のpanic、
      `string`/`char`それぞれにしか無いメソッド（`alphap`等）が他方の受け手では型エラーになる
      こと、`eq`等の同名メソッドが型ごとに別テーブルで衝突しないことの回帰）。miri green
      （24/24、898秒、UB/リーク無し——GC非依存のため時間のほとんどはmiriのインタプリタ
      オーバーヘッド自体）。
  - 実装済み（4p）: 数値拡張（[cl-equivalence-catalog.md](cl-equivalence-catalog.md) §2.2 f、
    ステップ6）。`gcd`/`lcm`/`signum`はTypeLisp実装に回すステップ7へ先送り（カタログの分類どおり）。
    - **設計変更（破壊的、ただし振る舞いは保存）**: 既存の`i32`算術/比較演算子（`+ - * / mod
      < <= > >= = /=`）を自由関数から**`i32`のインスタンスメソッド**（`registry::int_assoc`）へ
      移行し、同じシンボルを`i64`/`f64`でも使えるようにした（受け手の型で単一静的ディスパッチ
      ——`HashTable::get`等と全く同じ仕組み）。自由関数のままだと1シンボルにつき1シグネチャしか
      持てず、`i64`/`f64`版を追加できなかったため。`int_assoc(ty: Type)`は`i32`/`i64`で共通
      （`RtValue::Int(i64)`がどちらも同じランタイム表現のため、`eval_int_builtin`もそのまま共用）。
      `float_assoc()`が`f64`の`+ - * / mod`/比較に加え`expt`/`sqrt`/`floor`/`ceiling`/`round`/
      `truncate`を提供。整数の`/`/`mod`は0除算でpanic（既存どおり）、浮動小数は0除算でも
      panicせずIEEE-754の`inf`/`NaN`を返す（仕様の違いを明記）。
    - **移行で発覚した2件のレグレッションと修正**:
      1. `check_dotimes`の内部脱糖（ループ変数の`+1`・上限比較`<`）が`Expr::Call`を直接構築して
         いたため、自由関数テーブルから外れた`+`/`<`が`NoSuchFunction`になった。
         `Expr::Assoc{type_name: i32, instance: true, ...}`を直接構築するよう修正。
      2. `+`を高階関数へ値として渡すテスト（`builtin_as_value`、`RtValue::Builtin`経由）が
         `+`が自由関数で無くなったことで「unbound variable」エラーに。新規`Expr::MethodRef`
         （`FnRef`のインスタンスメソッド版、受け手型を`expected`の関数型から読む）と
         `RtValue::BuiltinMethod(Path, String)`を追加し、`Checker::method_value`が
         `Value::Symbol`解決のフォールバックとして担う形で復旧——`i32`だけでなく`f64`等にも
         汎用化されている（`tests/numeric_test.rs`の`f64_operator_as_a_value`で確認）。
         ジェネリックなメソッド（`HashTable::get`等）はこの経路では値化できない
         （呼び出し元の型引数が無いため代入できず、単に解決失敗するだけ）。
    - `random`: 唯一の自然な受け手を持たないので`gensym`と同様に自由関数のまま。
      `(random n)`は`[0, n)`の`i32`を返し、`n <= 0`はpanic。OS乱数源は使わず、
      `std::sync::atomic::AtomicU64`によるプロセス全体共有のxorshift64*（`SystemTime`で
      遅延シード）——暗号学的安全性は無く、`gensym`の「衝突耐性のみ」と同じ位置づけ。
    - TDD: 新規`tests/numeric_test.rs`（20件。i64/f64の四則・比較、整数0除算のpanicと浮動小数
      0除算のinf、expt/sqrt/floor/ceiling/round/truncate、random の範囲内/可変性/非正の境界
      panic、f64でのoperator-as-value）。既存`tests/eval_test.rs`の`arithmetic`/`comparison`/
      `builtin_as_value`/`dotimes`系がi32の回帰カバレッジを継続。miri green
      （20/20、`MIRIFLAGS=-Zmiri-disable-isolation`が必要——`random`の`SystemTime::now()`が
      デフォルトのmiri isolationでブロックされるため、デフォルト実行はエラーになる点に注意）。
  - 実装済み（4q）: TypeLispライブラリ関数 ステップ7a（基盤+基礎述語のみ。シーケンス操作/sort/
    gcd/lcm/signum/リスト一式/Option・Result補助/高階関数/数値補助は7b以降へ先送り——
    範囲が非常に広いため、まず土台と`case`(ステップ8)等が前提とする`eq`/`equal`を固めた）。
    - **新規基盤: prelude機構**（[src/prelude.rs](../src/prelude.rs)）。これまで「TypeLisp側
      ライブラリ関数」は定義しても自動では使えなかった（`Checker::new`/`Interp::new`は
      組み込みメタデータのみで、`defun`等を事前ロードする仕組みが無かった）。`prelude::SOURCE`
      （typelisp自身で書いた`defun`定義の文字列）+ `prelude::load(heap, chk, interp)`
      （read_all→check_form→exec を回すだけ）を追加し、`lib.rs`で`load_prelude`としてpub use。
      REPL（`main.rs`）はheap/checker/interp生成直後に呼ぶ。既存12個のテストファイルは
      ライブラリ関数を使わないため未変更（後続でprelude依存が増えたら個別に retrofit する）、
      新規`tests/prelude_test.rs`が自前のハーネスで`load_prelude`を呼ぶ。prelude自体は固定の
      既知ソースなので、読み込み失敗は`.expect`でpanicする設計（呼び出し側にResultを持たせない）。
    - **`eq`の全型対応**（カタログ§2.1の「型ごとに`defmethod`」を完成）: `bool`用に新規
      `bool_assoc()`、`i32`/`i64`/`f64`は既存の`int_assoc`/`float_assoc`に`=`と同じ`cmp()`を
      `eq`という別キーで追加登録（"型による値の同一性に違いが無いスカラーは`=`のエイリアス"）。
      `Sexpr`用に新規`sexpr_assoc()`（`sexpr_def`にこれまで無かった`assoc`を追加）— `Value`の
      `derive(PartialEq)`をそのまま使うことで**CLの`eq`と完全に一致するセマンティクス**が
      ノーコストで得られた: `Cons`は同一heapセルかどうか（真の識別子比較）、`Sym`はシンボルが
      常にinternされるため名前が同じなら必ず一致、`Int`/`Float`/`Char`/`Bool`はスカラー値比較
      （`Str`だけ要注意——別々に確保された同内容の`Sexpr::Str`は`StrId`が異なるため`eq`はfalse
      になる。CLでも文字列の`eq`は識別子比較なので、これは仕様どおりの挙動）。
      `char`/`string`の`eq`はステップ5で既に実装済み。
    - **`not`**（Rust組み込み自由関数、`(bool)->bool`。`and`/`or`と違って短絡評価が要らないので
      特殊形にする必要は無い）。
    - **`consp`/`null`/`atom`/`equal`**（typelispのprelude、`match`一発または再帰で書ける）:
      `consp`/`null`は`Sexpr`の`Cons`/`Nil`構成子を`match`するだけ、`atom`は`(not (consp s))`。
      `equal`は`eq`の再帰的構造版——`Cons`は両辺を`car`/`cdr`で再帰比較、`Str`は中身を比較する
      必要があるため`Sexpr::Str`の`match`で取り出した値（型は中身の`string`プリミティブ型その
      もの）に対して**string型自身の`eq`**（内容比較）を呼ぶことで対応、それ以外（Nil/Int/
      Float/Char/Bool/Sym）は`Sexpr`の`eq`がすでに正しい結果を返すのでそのまま使う。
    - TDD: 新規`tests/prelude_test.rs`（22件。not、eqの全対応型（bool/i32/i64/f64/char/string/
      sexpr atom/sexpr cons-is-identity-not-structural）、consp/null/atomの基本ケース、equalの
      再帰比較・長さ不一致・文字列内容比較・型不一致エラー）。既存319件への影響無し
      （リグレッション無し、新規追加のみ）。
  - 実装済み（4r）: TypeLispライブラリ関数 ステップ7b（`Sexpr`リスト操作一式。`prelude.rs`の
    `SOURCE`に追記、新規Rust側コードは無し——preludeローダーがそのまま使えた）。
    `length`/`append`/`reverse`(`reverse-onto`補助)/`nthcdr`/`nth`/`elt`/`last`/`butlast`/
    `take`/`subseq`/`copy-list`/`member`/`find-if`/`every`/`any`/`count-if`/`count`/
    `position-if`(`position-if-from`補助)/`position`/`remove-if`/`remove-if-not`/`remove`/
    `map`/`filter`/`foldl`/`foldr`（29関数）。
    - **設計判断**: `Sexpr`は単一の具象型なので`Vector<T>`/`HashTable<K,V>`と違いジェネリクスは
      一切不要（リストはすでに`Sexpr`そのもの）。アイテム指定版（`member`/`remove`/`count`/
      `position`）は`eq`で比較（CL既定の`eql`相当）、`-if`版は`(fn (Sexpr) bool)`の述語を取る。
      `defun`は`defmacro`と同様に前方参照不可・自己再帰のみ可能なため、依存される補助関数を
      `SOURCE`内で先に定義する順序に注意した。
    - **命名衝突の発見**: CL流に`some`という名前にしたところ、`Option`の`Some`構成子と
      （シンボルが大文字小文字無視のため）名前が一致し、コンストラクタ解決が自由関数解決より
      優先されるため`(some pred lst)`が常に`Option::Some`を構築しようとしてエラーになった。
      当初は`some?`にリネームして解決（既存の`alpha?`/`digit?`と同じ`?`接尾辞の述語命名規則）。
      **訂正（2026-06-20）**: `!`/`?`接尾辞を使わない命名規則を確定（§7.3）したため、
      `some?`/`alpha?`/`digit?`/`is-some?`/`is-none?`/`is-ok?`/`is-err?`をそれぞれ
      `any`/`alphap`/`digitp`/`is-some`/`is-none`/`is-ok`/`is-err`にリネーム
      （`any`は`some`との衝突を避けつつ`?`も使わない名前、Rustの`Iterator::any`相当）。
    - **`Sexpr`は任意の値を表すため「ちゃんとしたリストである」という制約を型で表現できない** ——
      `Cons`/`Nil`しか想定しない再帰関数はすべて`_`のワイルドカードアームを追加し、それ以外の
      バリアント（Int/Float/Char/Bool/Sym/Str）が来たら`panic`する設計にした（`car`/`cdr`の
      非Consでpanicする既存方針と統一）。最初の実装ではワイルドカードを忘れて
      `non-exhaustive match`エラーになった——TDDで即座に検出。
    - TDD: `tests/prelude_test.rs`に31件追加（53件中）。各関数の基本動作、空リスト/範囲外の
      境界、非リストへのpanic、`foldl`/`foldr`の畳み込み方向の違い（ドット対チェーンで検証）。
      既存350件への影響無し。
  - 実装済み（4s）: `set-car`/`set-cdr`（CL `rplaca`/`rplacd`）+ `nconc`/`nreverse`（破壊的版
    `append`/`reverse`）。**ユーザから優先順位の指摘を受けて7cから前倒しした**
    （[[feedback-impl-priority]]: 「Xが前提」と書いた時点でXの方が影響範囲が広いので先にやる
    べき、という補強）。
    - `mem::Heap::set_car`/`set_cdr`は**既に実装済みだった**（`rplaca`/`rplacd`相当、
      [src/mem/heap.rs:235](../src/mem/heap.rs)）が、言語レベル（registry/eval）に配線されて
      いなかった。`registry.rs`に自由関数として登録（`(Sexpr,Sexpr)->Unit`）、
      `eval_builtin`に`heap.set_car`/`set_cdr`を呼ぶ分岐を追加するだけで済んだ。
      非Consでpanic（`car`/`cdr`と同じ方針）。
    - `nconc`/`nreverse`は`prelude.rs`に追記（Rust側追加コード無し）。`nconc`は`last`+
      `set-cdr`で末尾を繋ぎ替え、`nreverse`は`nreverse-onto`補助関数で3ポインタ方式の
      in-place反転（パターンマッチで元のcdrを先に取得してから書き換える順序が必須）。
      どちらも既存のコンスセルを再利用するため、別の参照からも変更が見える
      （aliasing。`set_car_overwrites_in_place`等のテストで検証）。
    - **重要**: `set_car`/`set_cdr`の`unsafe`コードは実装済みだったが言語経由で一度も
      実行されておらず、miriでの検証も今回が初めて。
    - TDD: `tests/prelude_test.rs`に9件追加（62件中）。in-place変更の別参照からの可視性、
      非Consでのpanic、`nconc`の空リスト時の挙動、`nreverse`の空/単一要素ケース。
- **次の候補（eval 拡充、影響範囲の大きさで優先順位付け——[[feedback-impl-priority]]）**:
  1. **`defun` にジェネリック型パラメータを追加**（現状は組み込み型のみ `registry.rs` で直接
     `params` を設定できるが、ユーザ `defun` は単相のみ）。下記5の高階関数（`identity`/`const`/
     `compose`/`flip`）の前提であり、型システムの拡張で影響範囲が最も広いため最優先。
  2. **`defun`/`lambda` の型付き `&rest`／`apply`**: 関数呼び出し機構の拡張で、可変長引数を
     使う将来の関数・ライブラリ全般に影響するため次点。
  3. **`doiter`**: 特殊形・checker拡張。`case`/`do`/`while-let`は別途実装済み（下記**訂正**参照）。
  4. **`the`**: 型注釈として他コードからも汎用的に使われうるため、下記のライブラリ関数本体より先にやる。
  - **訂正（2026-06-22）**: 本リストの旧3「`case`/`do`/`while-let`」と旧4「マクロの`,@`」は
    この節を書いた時点では未着手だったが、**実際には既に実装済みだった**（`,@`は`append`を
    使う形で`prelude.rs`に`case`/`do`/`while-let`を追加する際に同時実装、いずれも本TODO本文中
    「実装済み（4r以前のどこか、`prelude.rs`参照）」として個別の記録が無いまま反映されており、
    本節だけが古い情報のまま取り残されていた）。`,@`/`gensym`は[[typelisp-llvm-compiler]]の
    Phase4（ステップ5節）で`while`/`dotimes`/`dolist`等のマクロ化にもそのまま使われている。
  5. **TypeLispライブラリ関数 ステップ7c以降**（`prelude.rs` に追記していく、最も影響範囲が
     狭いリーフ機能のため最後）: `sort`、`gcd`/`lcm`/`signum`、`assoc`、Option/Result補助
     （`unwrap`/`unwrap-or`/`is-some`/`map-option`等）、高階（`identity`/`const`/`compose`/
     `flip`——1のジェネリック型パラメータが前提）、数値補助（`min`/`max`/`sum`/`range`/
     `evenp`等）。
- **ステップ5: compile（実装後に全面リバート、2026-06-23）**: 明示`compile`/`compile-file`機能
  （コンパイラ本体をtypelispで書き、RustはLLVMバインディング(inkwell)・ASTブリッジ・ランタイム
  支援ライブラリのみ提供、「コンパイル済みバイトコード」はLLVM IRとする設計、2026-06-20方針確定・
  ブランチ`feature/compiler`）は、Phase 0からPhase 6a（`i64`/`bool`/`f64`/`char`/`Sexpr`全variant・
  `let`/`if`/`loop`/`break`/`return`・自己再帰/相互再帰・`Vector<i32|i64>`パラメータ対応まで実装、
  69→77テストgreen）まで実装が完了していたが、**`Vector<T>`の再設計議論を機に、機能全体を完全に
  リバート**した（`src/compile/`一式・Cargo.tomlの`compile` feature/`inkwell`依存・
  `tests/compile_test.rs`・`src/eval/interp.rs`/`src/check/registry.rs`内の関連コード全削除）。

  **リバートに至った経緯**: `Vector<T>`をコンパイル対応させる際、`RtValue::Vector`という専用
  enumバリアントの存在自体が「ユーザー定義型と同様に扱うべき」という設計原則に反すると判明
  （[[typelisp-vector-defstruct-revert]]参照）。この議論を掘り進めた結果、`defstruct`
  （ユーザー定義型機構）自体の設計も不完全（フィールド読み書き手段が無い）と判明し、
  `Vector<T>`・`defstruct`の両方を削除して再設計する方針になった。さらに、`compile`機能の
  typelisp本体（`compiler_source.rs`）自体が`Vector<T>`を内部データ構造（`params`/`vals`/`roots`
  テーブル）として全面的に使っていたため、Vector削除は必然的に`compile`機能全体の削除を伴った。

  **実装されていた内容の概要（参考、詳細はgit historyの`feature/compiler`ブランチ参照）**:
  統一呼び出し規約`TlValue`（`{tag:u8, payload:union}`、LLVM側`{i64,i64}`構造体）、
  `compile_lock`によるLLVM Context競合の解消、`i64`/`bool`/`f64`/`char`混在対応、`let`束縛、
  関数間直接呼び出し（自己/相互再帰含む`compile-group`）、`Sexpr`の全variant対応＋GC
  rooting（`push-root`/`pop-root`/`roots: Vector<i32>`による位置/内容分離）、`loop`/`break`/
  `return`、`setf`（`Sexpr`型変数への対応含む）、`Vector<i32|i64>`パラメータ対応。

  **次にcompileへ再着手する場合の前提**: `Vector<T>`/`defstruct`の再設計が完了していること
  （特に`compiler_source.rs`を何で書き直すか——`Vector<T>`が無くなった以上、`Sexpr`ベースか、
  再設計後の何らかの型を使う必要がある）。

### compile機能 再構築（2026-06-24開始、`feature/compiler`ブランチ、進行中）

`defstruct`再設計完了（[[typelisp-defstruct-redesign]]）を受けてゼロから再構築開始。
**過去の実装（git history）は一切参照しない**——設計は現在のコードベースのみを前提に行う方針
（ユーザー明示指示）。詳細な進捗・設計判断は[[typelisp-compile-rebuild-2026]]（memory）、
全体計画は`~/.claude/plans/distributed-baking-owl.md`参照。

**確定方針**:
- フルスコープ（JIT基盤・ABI・スカラ型対応・制御構造の再構築からdefstruct/defmethod対応まで）。
- self-hosting（コンパイラ本体はtypelisp自身のdefunで書く、Rustはinkwellバインディング・AST
  ブリッジ・ランタイムシムのみ）。
- JIT（`compile`）とAOT（`compile-file`）の両方を最初から見据える（共通コアを最大化、出口だけ分岐）。

**Phase 0完了（commit `551fc9c`）**: typelisp言語から直接LLVM IRを組み立てられる基盤。
`RtValue`に`Llvm*`系5バリアント追加、`registry.rs`/`eval_llvm_builtin_method`に
`llvm-module`/`llvm-function`/`llvm-builder`/`llvm-basic-block`/`llvm-value`の5型と
`create`/`add-function`/`append-block`/`build-*`等を実装、`compile::ast_bridge::ast_to_sexpr`
（typed AST→タグ付きSexpr、今回はリテラル系のみ実翻訳）、`src/compiler.rs`（self-hosting
コンパイラ本体の最小スライス）。ビルドに`inkwell 0.9`（`llvm17-0`）が必要、
`LLVM_SYS_170_PREFIX`は`scripts/with-llvm-env.sh`が`brew --prefix llvm@17`で動的解決
（絶対パスは一切ハードコードしない——[[feedback-no-hardcoded-absolute-paths]]）。

**Phase 1完了（commit `aeaacf3`、`590709d`）**: JIT実行をInterpに統合。`(compile "fn-name")`
がtypelisp言語内から呼べ、以降の`Expr::Call`はJIT済みネイティブコードへ優先ディスパッチする。
全コンパイル済み関数は固定ABI`i64 fn(i64* args, i32 argc)`に統一。`i64`の`+`/`-`/`*`のみ対応
（比較演算はABI拡張が要るため後続Phaseへ先送り）。コンパイラ本体は CL `labels`
（既存の`Expr::Labels`、トップレベル`defun`は相互再帰不可なため）で
`compile-value`/`compile-int`/`compile-var`/`compile-assoc`を相互再帰させる構造。

**発見した制約・注意点（今後も踏まえる）**:
- トップレベル`defun`同士は相互再帰不可能（前方参照不可、自己再帰のみ）。コンパイラ本体を
  拡張する際は、Rustから呼ばれる唯一のエントリポイント（`compile-function`）内の`labels`
  ブロックにサブ関数を追加していく。
- LLVM Contextに触るコードは`compile::COMPILE_LOCK`を必ず取得する——**テストコードも例外
  ではない**。Rust側で直接`create_jit_execution_engine`等を呼ぶテストでロックを取らずマルチ
  スレッド実行で間欠的SIGSEGVが発生した実例あり。
- LLVM 17はopaque pointerがデフォルト（inkwellの`llvm17-0` featureは`typed-pointers`を含まない）
  なので、ポインタ型は`Context::ptr_type`、`build_gep`/`build_load`は`pointee_ty`引数を渡す版を使う。

**Phase 2完了（commit `ca30fc5`）**: AOT出口。`(compile-file "src.typl" "out")`が独立した
typelispソースファイルを読み、ファイル中の全`defun`を1つのLLVM moduleへコンパイルし、
`TargetMachine`でオブジェクトファイル化→システムリンカ(`cc`)で実行ファイルにリンクする。
新規`src/compile/aot.rs`（`compile::aot::compile_file`）、`registry.rs`に
`compile-file: (string,string)->bool`を自由関数登録、`Interp::eval_builtin`に
`"compile-file"`ディスパッチを追加。

- **設計が計画から変わった点（実装中に判明した制約による）**: 当初案は「関数ごとに
  別moduleを作りLLVMの`Module::link_in_module`で1つに結合する」だったが、実装すると
  必ず`Rc::try_unwrap`に失敗した。原因は`compile-function`(typelisp側)内の`labels`が
  相互再帰のために**意図的に**参照循環を作る構造（[[typelisp-compile-rebuild-2026]]
  のPhase1の節参照）——`compile-value`等の4つのclosureが自分自身を含む全labels名を
  captureするため、外側の`m`(module)のslotがプログラム終了まで解放されない。
  この設計は変更せず（mutual recursionに必要な既存の仕組み）、代わりに
  **`compile-function`がmoduleを生成せず、Rust側から引数として受け取る**ように変更した
  （`(defun compile-function ((m llvm-module) (name string) (param-names Sexpr)
  (body Sexpr)) llvm-module ...)`）。これにより「1個の関数をコンパイルして既存のmodule
  に追加する」が本当に共有可能なJIT/AOT共通の最小単位になった——JIT(`Interp::compile_function`)
  は使い捨ての1関数用moduleを都度生成して渡し、AOT(`compile_file`)は1つのmoduleを
  ファイル全体で共有して`defun`の数だけ渡す。Rust側は`Interp::build_module`を
  `Interp::add_compiled_function(heap, module: Rc<RefCell<Module>>, name, internal_name)
  -> Result<(), EvalError>`に置き換え（戻り値でmoduleを返す必要がなくなった）。
  `link_in_module`は使われていない。
- **`libtlrt`雛形は今回作らなかった**（計画書には「最初は空でよい」とあったが見送った）:
  Phase 2時点では生成コードがランタイムシムを一切呼ばない（`ast_bridge`の翻訳対象が
  リテラル/var/算術のみのため）。空のstaticlibを追加するだけの新規cargoクレートは
  Phase 4でシムの最初のシンボルが要るようになった時点で作る方が、使われないコードを
  先取りしないという既存方針（YAGNI）に合う。リンクは`cc`にオブジェクトファイル
  単体を渡すだけで成立している。
- **エントリポイント規約**: ファイル中の`main`という名前・**0引数**の`defun`が必須
  （他の名前の関数は呼び出されないが、コンパイル自体は通る——複数`defun`が同じ
  moduleに共存できることの確認に使っている、`tests/compile_file_test.rs`の
  `compiles_a_file_with_a_non_main_helper_function_too`参照）。コンパイル後の
  LLVM関数名は`main`ではなく`tl_main`にし（固定ABI`i64 fn(i64*,i32)`とCの
  `int main(void)`がリンク非互換なため）、Rust側(`build_main_wrapper`)が別途
  本物の`main`（`tl_main`を呼んで`i64`結果を`i32`終了コードへtruncateするだけ）を
  直接inkwellで組み立てて追加する——これはJIT/AOT共通コアの外側（出口側）の処理。
- **`expected`が算術の最初の引数まで伝播しない既存挙動への対応**: `(+ 4 2)`のような
  裸の整数リテラルの算術は、`main`の宣言戻り値型が`i64`でも常に`i32`になる
  （`Checker::try_instance_method`が受け手の型を**最初の引数を`expected: None`で
  検査して**決めるため——メソッド解決にはまず型が要り、型を知るには`expected`を
  使えない、という設計上の理由がある。`main`はAOTの制約でパラメータも`let`も
  使えないため、外側の`expected`を伝える手段がない）。これはcompile-file固有の
  バグではなく既存の型検査の仕様通りの挙動なので、テスト側（`main`の宣言型を`i32`に
  する）で対応した。`i32`/`i64`はランタイム表現・生成されるLLVM IRの両方で完全に
  同一（`RtValue::Int(i64)`/`const-i64`固定）なため、動作に違いは無い。
- TDD: 新規`tests/compile_file_test.rs`（7件。定数`main`、算術、`main`以外の`defun`
  共存、終了コードの32bit truncate、`main`欠落エラー、`defun`以外のトップレベル形式
  エラー、JIT/AOTペアテスト）。並列実行を含め複数回連続green（`COMPILE_LOCK`の
  保持区間がRust→typelisp→Rustの再入を避けるよう注意——`add_compiled_function`の
  呼び出し中は外側でロックを取ってはいけない、デッドロックする）。

**次回やること（Phase 3: 制御構造）**:
1. `compiler.rs`のコンパイラ本体に`Expr::Let`/`Expr::If`(値位置・phi)/`Expr::Loop`/
   `Expr::Break`/`Expr::Return`のコード生成を追加（`ast_bridge`側の対応する翻訳も
   `(unsupported ...)`から実装に変える）。比較演算・`f64`/`bool`/`char`対応も
   このPhaseに含む。
2. 自己再帰はそのまま自分のLLVM関数を呼ぶだけ、特別扱い不要。
3. 相互再帰: AOTはファイル全体が1moduleなので「全関数を先にdeclare→本体生成」の
   2パスで自然に解決できる。JITは依存関数グループを1moduleに集約する仕組みが
   別途必要（Phase 2で作った「moduleを共有して複数回`add_compiled_function`を呼ぶ」
   仕組みがそのまま使えるはず——呼び出し先の関数も同じmoduleに先に登録しておけば
   `build-call`で参照できる）。
4. その後: Phase 4(Struct読み書き) → Phase 5(defmethod)。Struct構築・戻り値化は
   Phase 6として設計のみ記録し実装範囲には含めない方針（AOTの方が単純という
   非対称性があるため）。

---

## 開発コマンド
```sh
cargo test                                   # 全体
cargo +nightly miri test --test mem_test     # GC/ポインタの UB・リーク検査
cargo +nightly miri test --test read_test
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --test numeric_test  # random が SystemTime を使うため isolation 解除が必要
cargo run                                    # 最小 main（defun を read）
```

### compile機能のビルド（2026-06-24再着手、`feature/compiler`ブランチ）
`inkwell`（LLVM 17バインディング）に依存するため、`LLVM_SYS_170_PREFIX`が必要
（`brew install llvm@17`済みが前提）。**このパスはマシンごとに異なるため、
リポジトリ内のどのファイルにも絶対パスをハードコードしない** —
`scripts/with-llvm-env.sh`が`brew --prefix llvm@17`で都度動的解決する:
```sh
scripts/with-llvm-env.sh cargo build
scripts/with-llvm-env.sh cargo test
```
`LLVM_SYS_170_PREFIX`を自分のシェルで既にexport済みなら、素の`cargo build`/
`cargo test`でも動く（このスクリプトは便宜上のラッパーであり必須ではない）。
旧実装参照: `git log backup/typed-lisp-m14` / 構文参照: `/Users/suzukijun/Program/Rust/macro-lisp`
