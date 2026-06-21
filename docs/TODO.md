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
- `compile` feature（既定では無効）は `inkwell = { version = "0.9", features = ["llvm17-0"] }` に依存。
  crates.io の inkwell 0.9.0（最新安定版）は `llvm11-0`〜`llvm17-0` のみ対応（18 以降は inkwell 本家
  master でも未対応、2026-06-20 確認）。`brew install llvm@17` 済みなら `llvm-config` は自動検出され
  追加設定なしで `cargo build --features compile` が通る（`/usr/local/opt/llvm@17/bin` が見えない構成の
  場合のみ `LLVM_SYS_170_PREFIX=$(brew --prefix llvm@17)` を指定）。
- 旧 `src/compile/*`（動的タグ付き旧 `Object` 値モデル前提・未完成プロトタイプ、再利用不可と確認済み）は
  削除済み。新コンパイラ基盤の設計は本ファイル「ステップ5: compile」を参照。
- `cargo build`/`cargo test`（feature 無し）は `compile` feature 追加後も無関係・無依存のまま
  （LLVM/inkwell は `compile` feature を付けない限りビルドに一切関与しない）。

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
  `module`/`use`、`defstruct`（直和形・非ジェネリック）、`defmethod`（インスタンス `(self T)` / static `(T ...)`）、
  インスタンス・ディスパッチ（第一引数型）と `Type::method` 静的呼び出し、裸名解決 現NS→root。
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
- **未実装（後続）**: **ユーザ定義のジェネリック `defstruct`**（`check_defstruct` は常に `params:
  Vec::new()` で構造体に型パラメータを宣言する構文がまだ無い — 上記の受け手側 generic 代入とは別物。
  `HashTable<K,V>` 等の組み込み型は `registry.rs` で直接 `AdtDef.params` を設定するため、この制約の
  影響を受けていない）、`use a::b`（モジュール名を現NSに alias として導入）、可視性、関数カタログの
  実装本体（eval 待ち）。

## 3. ステップ4：eval（**4a/4b 実装済み**）
- **型付き AST（`check::Typed`）上のツリーウォークインタプリタ**（`src/eval/`、既定の実行経路）。
  - 実装済み（4a）: `RtValue`（リテラル＋構成子インスタンス `Data{type,variant,fields}`）、リテラル/`var`/`if`/`let`/
    `call`（defun）/`construct`/`match`（パターン照合）/`assoc`（インスタンス・static メソッド）/`panic`（`EvalError::Panic`）。
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
  - 実装済み（4n）: `Vector<T>`（[cl-equivalence-catalog.md](cl-equivalence-catalog.md) ステップ4）。
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
- **次の候補（eval 拡充）**:
  - TypeLispライブラリ関数 ステップ7c以降（prelude.rsに追記していく）: `sort`、
    `gcd`/`lcm`/`signum`、`assoc`、
    Option/Result補助（`unwrap`/`unwrap-or`/`is-some`/`map-option`等）、
    高階（`identity`/`const`/`compose`/`flip`——`defun`にジェネリック型パラメータが無いため
    要設計）、数値補助（`min`/`max`/`sum`/`range`/`evenp`等）。
    `append`実装済みなので、マクロの`,@`（unquote-splicing）を追加できる。
  - `case`/`do`/`doiter`/`while-let`/`the` は未実装（`defmacro` の `&rest` は実装済みなので前提は満たした。
    `case` はさらに型ごとの `eq` が前提）。`defun`/`lambda` の型付き `&rest`／`apply` も未実装。
- **ステップ5: compile**（明示 `compile`/`compile-file`。コンパイラ本体は **typelisp で書く**、Rust は
  LLVM バインディング（inkwell）・ASTブリッジ・ランタイム支援ライブラリのみ提供。「コンパイル済みバイトコード」
  は **LLVM IR** とする。feature gate。段階的ロードマップ・ABI（`TlValue`）設計は別途記録予定
  （2026-06-20 方針確定、ブランチ `feature/compiler`）。
  - 実装済み（5a/Phase 0）: `brew install llvm@17` + `inkwell`(`llvm17-0`) を `compile` feature 下に追加、
    旧 `src/compile/*`（再利用不可と確認済み）を削除して smoke test（`Context`/`Module`生成・`verify`）に
    置き換え。`tests/compile_test.rs`（feature gate）で検証、既定ビルド（feature無し）への影響なしを確認。
  - 実装済み（Phase 1）: 関数単位コンパイラ`compile`の最初の縦スライス——全引数・戻り値が`i64`の
    関数のみ対象（スカラーのみ、`if`は末尾位置限定）。
    - **`compile`本体はtypelispで書いた**（`src/compile/compiler_source.rs::SOURCE`、`prelude.rs`と
      同じ「typelisp定義をread→check→execで読み込む」ロード機構）。`compile-value`/`compile-tail`が
      `AstExpr`を辿ってLLVM builderビルトインを呼びIRを組み立てる。`if`は末尾位置のみ対応
      （then/else各ブロックに個別の`ret`を置くだけで済み、phi/alloca-load-storeのマージ機構が
      不要——`compile-value`（値位置）は`if`に当たると明示的にpanicしてVer1の範囲外と分かるようにした）。
    - **ASTブリッジ**: `Interp::fn_body`/`fn_signature`（新規public、`FnDef`に`sig: Option<(Vec<Type>,
      Type)>`を追加——`defmethod`/`defmacro`は`None`）と、`src/compile/ast_bridge.rs::typed_to_ast`
      （`check::ast::Expr`→新規組み込み直和型`AstExpr`の変換、対応外のExpr variantは`None`を返し
      「コンパイル不可」を明示）。`registry.rs`に`AstExpr`（`AInt`/`ABool`/`AVar`/`AIf`/`ABinOp`の5
      variant）・ビルトイン自由関数`ast-params`/`ast-body`（いずれも対象関数が全`i64`でなければ`None`）
      を追加。
    - **LLVMバインディング**: `RtValue`に`LlvmModule`/`LlvmBuilder`/`LlvmFunction`/`LlvmBasicBlock`/
      `LlvmValue`を追加（すべて`#[cfg(feature="compile")]`）。`Module`/`Builder`はinkwellの`'ctx`
      ライフタイムを持つため、プロセス全体で1つの`Context`を`Box::leak`して`'static`化する手法を
      採用（`eval::interp::llvm_context`）——複数の`compile`呼び出し間で同じ型世界を共有できる。
      `registry.rs`に`LlvmModule`/`LlvmFunction`/`LlvmBuilder`/`LlvmBasicBlock`/`LlvmValue`型と
      そのメソッド（`add-function`/`verify`/`dump`、`get-param`/`append-block`、`position-at-end`/
      `build-op`/`build-cond-br`/`build-br`/`build-ret`）を`hashtable_def`/`vector_def`と同型の
      メタデータのみ登録で追加、`eval::interp::eval_llvm_builtin_method`が実行時実装を担当
      （`Expr::Assoc`評価が`eval_builtin_method`に続けてこちらにもフォールバックする）。
      `build-op`は`+ - * / mod < <= > >= = /=`を1つのビルトインで受ける（`AstExpr`の`ABinOp`が
      運ぶ演算子文字列をそのまま渡せるよう、typelisp側に演算子ディスパッチ表を持たせない設計）。
    - **インタプリタ統合**: `Interp.compiled: HashMap<Path, CompiledFn>`（`CompiledFn`は0〜3引数
      それぞれの`extern "C" fn(...) -> i64`+ JIT実行エンジンの`Rc`を保持するenum——Phase 1は
      統一バイトコード仕様（呼び出し規約）を導入せず、対象を全`i64`スカラーに絞ることでネイティブの
      固定シグネチャのまま済ませた、という簡略化）。`Expr::Call`の既存ディスパッチの先頭に
      `self.compiled`参照を1行追加するだけで、コンパイル済み関数は呼び出し元から見て完全に透過的
      （`tests/compile_test.rs`の`compiles_and_calls_an_all_i64_function`で実証）。
    - `llvm-finish-compile`ビルトイン（JIT実行エンジン生成→関数アドレス取得→`Interp.compiled`へ登録）
      の戻り値は`Result<bool,Error>`（`Result<(),Error>`ではない——`()`はリスト区切り文字の対なので
      ジェネリクスの`<...>`内に書けず、ソーステキスト上で書けないため、`compile`自身の宣言型として
      書ける形を採った）。
    - 既知の制約（次フェーズへ）: 統一呼び出し規約（`TlValue` ABI）は未導入、`Sexpr`/`HashTable`/
      `Vector`/`Closure`/`f64`/`bool`/`char`は未対応、コンパイル済み関数から未コンパイルの関数を
      呼ぶことは不可（逆方向のみ対応）、複数スレッドからの`compile`同時呼び出しは未対応
      （プロセス全体で1つの`Context`をunsafe `Sync`/`Send`実装で共有しているのみ——現状テストが
      並列に`compile`を呼ばないため実害なし、と明記）。
    - TDD: 新規`tests/compile_test.rs`（4件: smoke test、`max2`コンパイル後呼び出しの往復2件、
      全`i64`でない関数のコンパイル拒否）。既存テスト・featureなしビルドへの影響なし
      （`cargo build`/`cargo test`は無変更、`cargo clippy --all-targets`は両構成（あり/なし）で
      警告0）。
  - 実装済み（Phase 2a/呼び出し規約の一般化）: Phase 1の`CompiledFn`はネイティブのi64固定
    シグネチャ（`extern "C" fn(i64,...) -> i64`、0〜3引数のみ）に頼っていたが、これをPhase1限定の
    簡略化として置き換え、統一呼び出し規約`#[repr(C)] struct TlValue { tag: u8, payload: union{i64,
    f64,bool,char,ptr} }`（16バイト、LLVM側は`{i64,i64}`の無名struct型として
    `eval::interp::tlvalue_llvm_type`が定義しレイアウトを一致させる）を新設し、すべての関数を
    `extern "C" fn(*const TlValue, u32, *mut TlValue) -> i32`という単一シグネチャへ寄せた
    （アリティ違いごとに別の関数ポインタ型を作る必要がなくなった——`CompiledFn`は4バリアントの
    enumから`{f, arity, _engine}`の1構造体に縮退）。
    - **`add-function`**はもうアリティを取らない（LLVM型自体がアリティに依存しないため）。
      代わりに引数は`LlvmFunction`の第1引数（`args: TlValue*`配列）に入るようになったので、
      論理引数`i`を読むのは純粋なメタデータ取得ではなくIR構築操作になった——新規
      `LlvmBuilder::load-arg`（GEP→struct全体load→`build_extract_value`でpayloadフィールド取り出し）
      が旧`LlvmFunction::get-param`を置き換え。`build-ret`も`LlvmFunction`を追加引数に取るようになり、
      結果を`TlValue{tag=I64,payload=v}`に詰めて第3引数（`out: TlValue*`）へstoreした上で
      `ret i32 0`を発行する（旧`ret <value>`の直接returnを置き換え）。
    - typelisp側（`compiler_source.rs`）は`fill-param-values`/`make-param-values`が`get-param`から
      `load-arg`呼び出しに変わった（IR構築のため`b`をビルダー位置決め後に呼ぶ必要があり、`compile`内の
      `let*`の束縛順序を「`b`を`position-at-end`した後で`vals`を計算する」よう入れ替えた）以外は
      Phase1のロジックを保持。
    - **副産物としてPhase1の「0〜3引数まで」制約も自然に解消**（引数が配列経由になったため、
      関数ポインタ型をアリティ数だけ用意する必要がなくなった）——`tests/compile_test.rs`に
      5引数関数のコンパイル・呼び出しを確認する回帰テストを追加。
    - **並行テスト実行で発覚したLLVM Contextの競合バグを修正**: 複数の`compile`呼び出しが別スレッドで
      同時にLLVM C APIを呼ぶと（`struct_type`の型ユニーク化テーブル等の内部状態が競合し）
      非決定的にSIGSEGVするようになった（Phase1時点でも理論上のリスクとして`LlvmContextHandle`の
      docコメントに記録されていたが、Phase1の操作が単純すぎて実際には踏んでいなかった——Phase2aの
      GEP/struct操作の増加で実際に発生するようになった）。プロセス全体で1つの`compile_lock`
      （`std::sync::Mutex<()>`）を新設し、LLVMコンテキストに触れる各Rust関数（`llvm_module_new`等、
      呼び出しの起点ごと）の先頭で取得するようにして解消（`compile`自体はtypelispの通常の関数呼び出し
      の積み重ねなので、1回の`compile`呼び出し全体ではなく個々のLLVM C API呼び出し単位でロックする
      ことで、2つの`compile`呼び出しがどんな順序でインターリーブしても個々のLLVM操作自体は競合しない
      ようにしている）。修正前は`cargo test --features compile`を10回に1〜2回の頻度でSIGSEGVしていたが、
      修正後は25回連続green（通常実行15回＋新規テスト追加後10回）。
    - TDD: `tests/compile_test.rs`に1件追加（5引数関数）、既存4件は無変更で green
      （`compiled_function_still_works_the_other_way_round`等、外部から見た挙動はPhase1と同一を確認）。
      既定ビルド（feature無し）への影響なし、`cargo clippy --all-targets`は両構成で警告0
      （`compile`featureあり構成の既存警告1件は本セッション開始時から未コミットで残っていた
      `tests/prelude_test.rs`のmiriリーク対策実験によるもので、Phase2の変更とは無関係）。
  - 実装済み（Phase 2b/`i64`・`bool`混在シグネチャ）: Phase 1/2aは「全引数・戻り値が`i64`」のみ
    対象だったが、これを「各引数・戻り値が`i64`または`bool`（任意の混在可）」に拡張した——
    `(defun gt ((a i64) (b i64)) bool (> a b))`のような述語関数（`i64`引数+`bool`戻り値、
    現実のコードで最も典型的な形）が初めてコンパイル対象になった。
    - `is_i64_scalar_signature`を`is_supported_scalar_signature`（+ `is_supported_scalar_type`）に
      一般化（`eval::interp`）、`ast_bridge::typed_to_ast`の`Expr::Var`ガードを`Type::I64`単独から
      `Type::I64 | Type::Bool`に拡大。`AstExpr::ABool`自体はPhase1から登録済みだったが
      `compile-value`が未対応で`_`アームに落ちてpanicしていたのを今回初めて配線。
    - **`bool`は終始i1幅のLLVM値として扱い、`TlValue`境界（`load-arg`/`build-ret`）でのみ
      `i64`スロットとの幅変換を行う**設計（`build-op`の引数として`bool`値が混ざることはない——
      `i64`の二項演算は`ast_bridge`がi64の`Assoc`にしか反応しないため、幅の食い違いは原理的に
      発生しない）。新規Rust関数: `llvm_builder_load_arg_bool`（payload読み出し共通化した
      `load_arg_payload`へ`build_int_truncate`を追加）、`llvm_builder_build_ret_bool`
      （`build_int_z_extend`でi64へ拡張後、共通化した`build_ret_tlvalue`で`tag=Bool`を書き込む）、
      `llvm_const_bool`（`bool_type().const_int`）。既存`load-arg`/`build-ret`は共通ヘルパ
      （`load_arg_payload`/`build_ret_tlvalue`/`out_ptr_of`)抽出のリファクタのみで動作は不変。
    - **`AstExpr`自体は引数の型を持たない**（各ノードに型タグが無い）ため、`compile`が
      「このパラメータ位置・戻り値はbool型か」を問い合わせる新規Rust関数`param-is-bool`/
      `ret-is-bool`（`fn_signature`の型情報を参照）を追加し、typelisp側の`fill-param-values`/
      `compile-tail`がそれぞれ`load-arg`系/`build-ret`系のどちらを呼ぶか分岐するようにした
      （`Vector<i32>`等の並行した型タグ列を新設する案より既存`ast-params`/`ast-body`を不変に
      保てるため、この副問い合わせ方式を採用）。
    - `call_compiled`（Rust側の呼び出し）も、引数を`RtValue::Int`/`RtValue::Bool`どちらからでも
      `TlValue`へ詰められるよう`to_tlvalue_arg`を新設（引数側のtagは呼び出し先IRから読まれない
      ので実質どちらでもよいが一貫性のため正しく設定）、戻り値は`out.tag`を見て`RtValue::Int`/
      `RtValue::Bool`を分けて構築するよう変更（Phase2aは`tag`を無視して常に`Int`化していた）。
    - TDD: `tests/compile_test.rs`に5件追加（10件中）——`i64`引数+`bool`戻り値の述語関数、
      `bool`引数を`if`条件に直接使う関数、`bool`リテラルを条件に使う0引数関数、`bool`リテラルを
      そのまま返す関数。既存5件は無変更でgreen。並行実行でのSIGSEGV再発無し（10回連続green）。
      既定ビルド・`cargo clippy --all-targets`（両構成）への影響なし（警告0、既存の
      `prelude_test.rs`由来の1件のみで無関係）。
    - 未対応として残った範囲: `f64`/`char`（リテラル・演算・パラメータ・戻り値）、`let`束縛
      （`ast-body`は依然「単一式の本体のみ」)、`bool`同士の`eq`等のメソッド呼び出し
      （`and`/`or`は`if`へ脱糖されるため不要だが、明示的な`bool::eq`呼び出しは未橋渡し）。
  - 実装済み（Phase 2c/`f64`対応）: `i64`/`bool`に加え`f64`も任意混在できるようにした——
    `(defun quad ((a f64) (b f64)) f64 (* (+ a b) 2.0))`のような浮動小数演算関数、
    `i64`の比較で分岐しつつ`f64`を返す関数（`tests/compile_test.rs`の
    `compiles_a_function_mixing_i64_and_f64`）がコンパイル対象になった。
    - **`bool`と違い真に新しいLLVM値種別が必要**: `inkwell::values::FloatValue`は整数の
      `IntValue`とは別のRust型のため、`RtValue::LlvmValue`の表現を`IntValue`単独から
      `inkwell::values::BasicValueEnum`（IntValue/FloatValueを両方包む列挙体、inkwell組み込み）に
      広げた——`eval::value.rs`のdocコメントが当初から「widens to BasicValueEnum once f64/bool
      join」と予告していた設計に合わせた形（個別の`RtValue::LlvmFloatValue`variantを増設する
      案より、既存の型変換系inkwell APIをそのまま使えてRust側の分岐も1箇所に閉じるため）。
      旧`expect_llvm_value`は`expect_llvm_basic_value`+`expect_llvm_int_value`/
      `expect_llvm_float_value`（kind不一致はinkwellの`into_int_value`等の生パニックではなく
      `EvalError::Internal`を返す）に分割。
    - **`build-op`は実引数の実際のkind（IntValue/FloatValue）で内部分岐**
      （`llvm_builder_build_int_op`/`llvm_builder_build_float_op`に分離、`AstExpr`自体は
      演算子文字列以外の型情報を持たないため、operandの実際の値から判断するほうが自然）。
      浮動小数比較（`fcmp`）は整数比較と同じく常に`i1`の`IntValue`を返す（`FloatValue`にはならない）
      点に注意——`bool`は終始`IntValue`表現という既存方針と整合。比較predicateは
      Rustの`f64: PartialEq/PartialOrd`と一致させ「`<`/`<=`/`>`/`>=`/`=`はNaN関与で常にfalse
      （ordered: OLT/OLE/OGT/OGE/OEQ）、`/=`はNaN関与で常にtrue（unordered: UNE）」を採用。
    - **`load-arg-f64`/`build-ret-f64`は`build_bit_cast`で生ビット列を再解釈**（truncate/zext
      ではない——`TlValue::from_f64`がunionの`f64_`フィールドに直接書き込むことでIEEE-754の
      ビットパターンをそのまま`i64_`スロットへ重ね書きしており、読み出し側もビット再解釈で
      対応する必要があるため）。
    - **`param-is-bool`/`ret-is-bool`と同型の`param-is-f64`/`ret-is-f64`を追加**し、
      typelisp側`fill-param-values`/`compile-tail`を2値分岐から`cond`による3値分岐
      （bool/f64/その他=i64）に拡張。Rust側は`param_has_type`/`ret_has_type`という
      共通ヘルパに集約し、4関数間の重複を排除。
    - `AstExpr`に`afloat`variantを追加（既存`aint`/`abool`/`avar`/`aif`/`abinop`の並びに
      割り込ませず**末尾に追記**——これらのindexは`ast_bridge.rs`の定数と1:1対応する
      ワイヤフォーマットであり、既存variantの並び順を変えると動いていたものが壊れるため）。
      `ast_bridge::typed_to_ast`の`I64_BINOPS`は`f64`の同名演算子も同じ文字列集合なので
      `BINOPS`に改名し受け手の型チェック（`i64`または`f64`）を追加するだけで済んだ。
    - TDD: `tests/compile_test.rs`に4件追加（14件中）——`f64`算術、`f64`述語（比較→bool）、
      `f64`リテラル返却、`i64`/`f64`混在（`i64`の比較結果で`f64`の2計算を分岐——言語に
      暗黙の数値変換が無いため、`i64`と`f64`を同一`Assoc`呼び出し内で混在させることはできない
      ことを確認する意味も持つ）。既存10件は無変更でgreen。並行実行でのSIGSEGV再発無し
      （10回連続green）。既定ビルド・`cargo clippy --all-targets`（両構成）への影響なし。
  - 実装済み（Phase 2e/`let`束縛）: `(defun double ((x i64)) i64 (let ((y (* x 2))) y))`のような
    ローカル変数を持つ関数がコンパイル対象になった。当初想定（`ast-body`の「単一式」制約を
    「複数フォーム+let」へ拡張する必要があるかもしれない、という`alloca`+`load`/`store`方式の
    検討）は**実際には不要だった**——`defun`の本体がまるごと1個の`(let (...) ...)`式である場合、
    `ast-body`から見ればこれは元々「単一式」のまま（その式の中身が`let`なだけ）なので、既存の
    「本体は1フォームのみ」制約に一切触れずに済んだ。必要だったのは`Expr::Let`を`AstExpr`へ
    橋渡しすることだけ。
    - **`alloca`/`load`/`store`は不要、値レベルのSSA変数表で十分**: `let`束縛は不変
      （`defun`の`let`に再代入は無い）なので、既存の`params: Vector<string>`/
      `vals: Vector<LlvmValue>`という「名前→LLVM値」表をそのまま流用し、束縛が増えるたびに
      名前と値を**末尾に追加**した新しいVectorを作って子スコープに渡すだけで済む
      （メモリ上のスタック変数を介さず、SSA値を直接持ち運ぶ——LLVM最適化前提のIRとして自然な形）。
      新規`copy-strs`/`extend-strs`（`Vector<string>`用）・`copy-values`/`extend-values`
      （`Vector<LlvmValue>`用）が「base要素の後にextra要素を追加した**新しい**Vectorを作る」役。
      既存の`params`/`vals`への`push`による直接破壊的拡張は**しない**——`Vector`の変更は
      共有（`Rc<RefCell<..>>`）なので、`if`の片方の枝や`let`脱出後のコードがまだ参照する
      古い`params`/`vals`を書き換えてしまうため。
    - **シャドーイングのため`param-index`の探索方向を末尾から先頭へ反転**: `extend-strs`は
      新しい束縛をVectorの**末尾**に追加するため、外側の名前を内側の同名束縛が覆い隠す
      （shadow）には「末尾から探して最初に見つかった方を使う」必要がある——変更前の
      先頭からの探索（パラメータ一覧のみだった頃は重複名が無かったため問題にならなかった）
      のままだと外側の束縛が常に勝ってしまうバグになる。`tests/compile_test.rs`の
      `compiles_a_function_with_a_shadowing_let_binding`（`(let ((a (+ a 1))) a)`が
      6を返すこと、5を返してしまわないこと）で実地検証。
    - **CL `let`の並行束縛セマンティクス（各束縛の値式は外側スコープのみを見る、兄弟束縛は
      見えない）を保持**: 値側（`eval-bindings`）は常に**外側の`params`/`vals`**に対して
      `compile-value`するのに対し、本体側（`eval-body`/`eval-body-tail`）だけが拡張後の
      `new-params`/`new-vals`を使う、という非対称な設計で実現（`check_let`/`eval`側の既存
      実装と同じ非対称性をコンパイラ側にも持たせた形）。`compiles_a_function_whose_let_binding_
      value_sees_the_outer_scope`（`(let ((a (+ a 1)) (b a)) b)`が`a+1`ではなく元の`a`を
      返すこと）で実地検証——これが`let*`ではなく`let`であることの直接的な証拠。
    - **`labels`で前方参照制約を回避**: `ALet`の処理（束縛値の評価ループ・本体フォームの
      評価ループ）は`compile-value`/`compile-tail`自身を呼び出す必要があるが、別の
      トップレベル`defun`として書くと「`defun`は自己再帰のみ可・他のまだ定義されていない
      `defun`への前方参照は不可」という既存制約に抵触する（`compile-value`→新規ヘルパ→
      `compile-value`という相互参照になってしまう）。`labels`（CLの局所関数定義、ステップ9で
      実装済み）でヘルパを`compile-value`/`compile-tail`それぞれの本体内**局所**に定義する
      ことで回避——局所関数は定義時点で外側の`compile-value`/`compile-tail`が（自己再帰と
      同じ理由で）既にスコープに存在するため、前方参照にならない。
    - `AstExpr`に`alet`variantを追加（`afloat`と同じく既存variantの並びに割り込ませず**末尾に
      追記**）。3フィールド構成`(names: Vector<string>, values: Vector<AstExpr>, body:
      Vector<AstExpr>)`——`ast_bridge::typed_to_ast`が`Expr::Let`をこの形に変換。
    - TDD: `tests/compile_test.rs`に7件追加（21件中）——単純な束縛、複数束縛+複数本体フォーム
      （最後だけが値になる）、`let`の入れ子、`let`の値位置での使用（算術式の中）、`let`の
      tail-if枝での使用、シャドーイング、並行束縛セマンティクス。既存14件は無変更でgreen。
      並行実行でのSIGSEGV再発無し（10回連続green）。既定ビルド・`cargo clippy --all-targets`
      （両構成）への影響なし。
  - 実装済み（Phase 2f/コンパイル済み関数同士の直接呼び出し）: `(defun square ((x i64)) i64 (*
    x x)) (compile "square") (defun sum-of-squares ((a i64) (b i64)) i64 (+ (square a) (square
    b))) (compile "sum-of-squares")`のように、既に`compile`済みの別関数を呼ぶ関数がコンパイル
    対象になった——逆方向（コンパイル済みから未コンパイルを呼ぶ）はトランポリンが必要で
    対象外のまま、自己再帰・相互再帰も対象外（理由は下記）。
    - **未コンパイルの呼び出し先はASTブリッジ自体を拒否（`None`）、`compile-value`での
      panicにはしない**: `ast_bridge::typed_to_ast`に`interp: &Interp`を渡すよう変更し、
      `Expr::Call`に遭遇した際`interp.is_compiled(path)`（新規`Interp`メソッド、
      `self.compiled.borrow().contains_key(name)`）を見て、コンパイル済みでなければ`None`を
      返す。これにより`compile`の既存`(ast-body name)`の`(None) -> Err(...)`アームが
      自然に「呼び出し先未コンパイル」もハンドルする——typelisp側`compile-value`に
      `compiled?`チェック＋`panic`を書く案より、既存の「未対応構文は`ast-body`がNoneを返す」
      という設計哲学と一貫する。副作用として**自己再帰・相互再帰の呼び出しは対象外**
      （`compile`が`name`自身をビルド完了するまで`self.compiled`に登録しないため、
      自分自身を呼ぶ`Expr::Call`は常に「未コンパイル」と判定されブリッジが失敗する——
      これは意図的な制約として明記、将来別Phaseで対応）。
    - **呼び出し先関数は新モジュールに宣言のみ追加し、JITエンジン作成後に実アドレスへ
      マッピング**: 各`compile`呼び出しは独立した`Module`/`ExecutionEngine`を持つ
      （ヘッダの`LlvmContextHandle`は1つだが、JITエンジンは呼び出しごとに別）ため、
      呼び出し先の関数本体を新モジュールに直接持ち込むことはできない。新規
      `LlvmModule::get-or-declare-function`（モジュール内に同名関数が既にあれば取得、
      なければ`add-function`と同じ統一ABI型`i32 (ptr args, i32 argc, ptr out)`で
      **本体なし（`append-block`を呼ばない）**の宣言だけ追加）でモジュール内に呼び出し先の
      シンボルを用意し、`Interp::builtin_llvm_finish_compile`がJITエンジンを作った直後に
      モジュール内の「本体なし（`count_basic_blocks() == 0`）」関数を走査して
      `self.compiled`から対応する関数ポインタを検索し`ExecutionEngine::add_global_mapping`
      で実アドレスを教える、という2段階の設計。`get-or-declare-function`が「既にあれば
      取得」を行う理由は、同じ呼び出し先を1つの関数から複数回呼ぶケース（`add-function`を
      同名で2回呼ぶとLLVMが2番目を黙ってリネームしてしまい、2番目の呼び出し箇所が
      宣言を見つけられなくなる）に対応するため。
    - **`build-call`/`build-call-bool`/`build-call-f64`（`LlvmBuilder`新規メソッド）**:
      引数の`Vector<LlvmValue>`をスタック上の`[argc x TlValue]`（`alloca`）に詰め、
      各要素は`build-ret`系と共通化した`pack_tlvalue`ヘルパでtag/payloadをpack。
      各引数のtag判定は`build-op`と同じ「`AstExpr`自体は型タグを持たないので実際の
      `BasicValueEnum`のkindを見る」方式（`i1`→bool・他幅`IntValue`→i64・`FloatValue`→f64、
      新規共通ヘルパ`tlvalue_tag_and_payload`）。`out`用`TlValue`もスタックに`alloca`し、
      統一ABIの`call`命令を発行→`out`をloadしてpayload（生i64ビット列）を取り出すところまでを
      共通化（`llvm_builder_build_call_payload`）、3つの公開関数がそこからの解釈
      （i64のまま／truncateしてbool／bit-castしてf64）だけを分担——既存
      `load-arg`/`load-arg-bool`/`load-arg-f64`、`build-ret`/`-bool`/`-f64`と同型の3分岐。
    - typelisp側（`compiler_source.rs`）: `compile-value`/`compile-tail`の両方に
      `module: LlvmModule`引数を追加（`ACall`が`get-or-declare-function`を呼ぶのに必要）。
      `compile-value`の`labels`を`ALet`専用から「`ALet`と`ACall`が共有する`eval-args`」
      （「`Vector<AstExpr>`を現在のスコープで評価して`Vector<LlvmValue>`にする」処理は
      束縛値の評価も呼び出し引数の評価も全く同じ形）に統合。**`compile-tail`は`ACall`用の
      専用アームを持たない**——既存の`_`アーム（`compile-value`を呼んで`build-ret`系に渡す）
      が複数の基本ブロックに分岐しない「ただの値を作るAstExpr」全てを正しく処理できる
      ことに気づいたため（`AIf`/`ALet`だけが基本ブロック分岐を伴うので専用アームが必要）。
    - `AstExpr`に`acall`variant追加（`alet`と同じく既存の並びに割り込ませず**末尾に追記**）。
      2フィールド構成`(name: string, args: Vector<AstExpr>)`。
    - TDD: `tests/compile_test.rs`に5件追加（26件中）——別の既コンパイル関数を呼ぶ関数、
      同じ呼び出し先を1関数から2回呼ぶ関数（`get-or-declare-function`の再利用パス検証）、
      `bool`を返す呼び出し先を`if`条件に使う関数、`let`束縛の値の中で呼び出す関数、
      未コンパイルの関数を呼ぶ関数のコンパイル拒否。既存21件は無変更でgreen。
      並行実行でのSIGSEGV再発無し（10回連続green）。既定ビルド・`cargo clippy --all-targets`
      （両構成）への影響なし。
  - 実装済み（Phase 2d/`char`対応）: `i64`/`bool`/`f64`に加え`char`も任意混在できるように
    した——`char`リテラルを返す関数、`char`パラメータを`char::eq`/`char::lt`で比較する関数
    （`(defun min-char ((a char)(b char)) char (if (lt a b) a b))`）、`char`を引数・戻り値に
    取るコンパイル済み関数の直接呼び出しがコンパイル対象になった。
    - **`char`は算術が無いため`bool`と同じ「終始i32幅のIntValueとして扱い、`TlValue`境界
      （`load-arg-char`/`build-ret-char`/`build-call-char`）でのみi64スロットとの幅変換
      （`truncate`/`zext`）を行う」設計**（`bool`がi1なのに対し`char`はi32——Unicodeの
      最大コードポイントU+10FFFFは21bitで32bitに余裕で収まるため、符号付き/符号なしの
      違いを気にせず安全に格納できる）。新規Rust関数は既存`-bool`系と完全に同型
      （`llvm_builder_load_arg_char`/`llvm_builder_build_ret_char`/
      `llvm_builder_build_call_char`、`llvm_const_char`、`param-is-char`/`ret-is-char`）。
    - **`char::eq`/`char::lt`は新しい演算子を`build-op`に追加するのではなく、既存の
      `=`/`<`演算子文字列にast_bridge側で変換**: `char`値は終始i32幅の`IntValue`なので、
      i64用に実装済みの`icmp`ロジック（符号付き比較だが、Unicodeコードポイントは常に
      非負なので実害なし）がそのまま使える。メソッド名(`eq`/`lt`)と演算子文字列(`=`/`<`)が
      異なる唯一のケースで、`build-op`側に新しい名前を教えるのではなく
      `ast_bridge::typed_to_ast`側で変換する方を選んだ——`build-op`は「`AstExpr::ABinOp`が
      運ぶ演算子文字列をそのまま使う」という既存の単純さを保てる。
    - `RtValue::Char`の`call_compiled`戻り値デコードは`char::from_u32`を経由
      （無効なコードポイントは`EvalError::Internal`——コンパイル後コードが書き込む値は
      常にリテラル/`load-arg-char`由来の有効な`char`なので実際には到達しない）。
    - `compile-tail`のシグネチャに`char-ret`フラグを追加（`bool-ret`/`f64-ret`と並ぶ3つ目、
      4分岐の`cond`に拡張）、`compile-value`の`ACall`分岐・`fill-param-values`の`cond`も
      同様に4分岐へ拡張。
    - `AstExpr`に`achar`variant追加（既存の並びに割り込ませず**末尾に追記**）。
      1フィールド構成`(c: char)`。
    - TDD: `tests/compile_test.rs`に4件追加（30件中）——`char`リテラルを返す関数、
      `char::eq`で比較する関数、`char::lt`+`if`で小さい方を返す関数、`char`を介して
      別のコンパイル済み関数を呼ぶ関数（および橋渡し対象外の`upcase`呼び出しが
      正しく拒否されることの確認）。既存26件は無変更でgreen。並行実行でのSIGSEGV
      再発無し（10回連続green）。既定ビルド・`cargo clippy --all-targets`
      （両構成）への影響なし。
  - **次の作業（Phase 2残り、ブランチ`feature/compiler`で継続）**:
    - 自己再帰・相互再帰のコンパイル対応（Phase 2fでは明示的に対象外とした）: `name`自身を
      `compile`完了前に`self.compiled`へ仮登録する、または呼び出し先解決を`get-or-declare-
      function`＋`add_global_mapping`ではなく単純な`call`に倒せる「同一モジュール内の
      自己呼び出し」を特別扱いする、といった方向性が考えられるが優先度未定。
    - **Phase 3（Sexpr/HashTable/Vector対応）**: 既存Rust実装（`mem::Heap`の cons/GC、
      `eval::interp`のhashtable/vector操作）を`extern "C"`シムで薄くラップした「ランタイム支援
      ライブラリ」層を新設し、コンパイル後コードはこれらの操作をすべて呼び出し経由で行う
      （直接cons cellの形を読まない——当面`Sexpr`はオパーク）。**未解決のリスクとして明記**:
      コンパイル後コードのレジスタ/スタック上の値をmark-sweep GCのルート走査が辿れない問題。
      Phase3では「ランタイム呼び出しの瞬間だけGCが起こり、その結果は即座にrootingする」という
      限定的回避で進め、本格対応（stack map等）は対象外として記録する。
    - **Phase 4（クロージャ/高階関数、優先度低）**: GCルーティングの難度がさらに増すため、
      Phase 5（ファイルコンパイラ）より後でもよい。
    - **Phase 5（`compile-file`）**: ファイル全体の`defun`を1つのLLVM `Module`にまとめ
      （同一ファイル内呼び出しは直接`call`命令になる）、`TargetMachine::write_to_file`でオブジェクト
      ファイル出力→システムの`cc`をサブプロセス起動してリンク。既存Rust実装（cons heap/GC/
      HashTable/Vector等）を`extern "C"`シムでラップした静的ランタイムライブラリ（`libtlrt.a`、
      `Cargo.toml`に`[lib] crate-type = ["lib","staticlib"]`を追加し`typl`自身のビルド時に
      一緒に作る）を`cc`でリンクし、**typelisp/LLVMインストール無しで動く実行ファイル/共有
      ライブラリ**を生成する（ユーザー要求の核心）。実行ファイル化はエントリポイント規約として
      `main`という名前の関数（`(fn () i32)`等）を探す（CL/C慣習）。共有ライブラリ化は既存の
      `FnSig::public`（`pub` defun）をエクスポートシンボルの基準に流用。
    - **Phase 6（並行）**: 各Phase完了の都度、`TlValue` ABI仕様や`compile-file`のセマンティクスを
      `docs/language-design.md`に「確定仕様」として追記していく（このTODO.mdは進捗の記録、
      language-design.mdは確定した言語仕様という既存の役割分担を継続）。

---

## 開発コマンド
```sh
cargo test                                   # core（LLVM 不要）
cargo test --features compile                # compile（LLVM要、brew install llvm@17 済みなら追加設定不要）
cargo +nightly miri test --test mem_test     # GC/ポインタの UB・リーク検査
cargo +nightly miri test --test read_test
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --test numeric_test  # random が SystemTime を使うため isolation 解除が必要
cargo run                                    # 最小 main（defun を read）
```
旧実装参照: `git log backup/typed-lisp-m14` / 構文参照: `/Users/suzukijun/Program/Rust/macro-lisp`
