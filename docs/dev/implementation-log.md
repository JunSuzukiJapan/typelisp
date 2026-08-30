# typelisp 実装ログ（アーカイブ）

最終更新: 2026-08-25 / ブランチ: `feature/cl-parity`

このドキュメントは、再実装（read 関数から作り直し）で**完了した**作業の経緯・設計判断を
記録するアーカイブ。**現在「残っている作業」は [TODO.md](TODO.md) を参照**——TODO.md が
長くなりすぎたため、2026-06-27 に完了済みの内容をこちらへ分離した。
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
- `src/check/resolved.rs` — `Ref` / `CompileTarget` / `Pattern`（当時は `src/check/ast.rs` で `Typed` / `Expr` / `Arm` を持っていた。cons セル化 Phase 2 で AST 自体が無くなり、消費者が再導出できない解決結果だけが残ったので Stage D で改名）
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
- 本節執筆当時（名前空間完成時点）は eval 未着手だったため「未実装」項目を別途リストして
  いたが、ユーザ定義ジェネリック`defstruct`・`use a::b`（モジュールalias）・関数カタログ本体は
  いずれもその後完了済み。可視性（`FnSig::public`）のみ現在も未実装のまま残っている——
  **現在の残作業は [TODO.md](TODO.md) を参照**。

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
- **「次の候補（eval 拡充）」リストの顛末**: 当初5項目（`defun`のジェネリック型パラメータ／
  型付き`&rest`・`apply`／`doiter`／`the`／ライブラリ関数ステップ7c以降）を影響範囲順に
  リストしていたが、本節の更新が実装に追いつかず、2026-06-22・2026-06-27の2回にわたって
  「実は既に完了していた」という訂正が入った——最終的に2026-06-27時点で`the`/`unreachable`/
  `todo`/`exit`も実装完了し、本リストの実質的な残りは`doiter`（仕様未確定、要設計）のみに
  なった。**現在の残作業は [TODO.md](TODO.md) を参照**。
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

**Phase 1完了（commit `aeaacf3`、`590709d`）**: JIT実行をInterpに統合。`(compile fn-name)`
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

### labels/クロージャのコンパイル対応（2026-06-25開始）

上の「Phase 3: 制御構造」メモ（相互再帰にAOTの2パス・JITの依存グループ集約が
必要、という記述）は**実装コードを直接確認した結果誤りだったと判明**し、
`labels`（CL流の局所相互再帰関数定義）と一般クロージャ（escapeするlambda値・
高階関数）のコンパイル対応という、計画当時より大きいスコープに置き換わった。
ユーザーの出発点の指摘——「`labels`は関数の循環参照を可能にする構文なので
必ず循環参照がある」「LLVMレベルでは関数の循環参照は可能」「よって`labels`内の
各関数はLLVMレベルでそれぞれ別関数としてコンパイルすべき」——から、
**クロージャの自由変数キャプチャに対応**・**一般のlambda/高階関数も対象に含める**
という2点をユーザーが追加スコープとして確定した。詳細な設計判断（クロージャの
実行時表現、自由変数解析、新規LLVM builtin一覧、段階分割Stage 1-4）は計画ファイル
`~/.claude/plans/labels-labels-resilient-shannon.md`参照。

**Stage 1完了**: `labels`の兄弟/自分自身への直接呼び出し（キャプチャ無し）。
新規`llvm-module::get-function`（同モジュール内の既存関数を名前で取得）、
`llvm-builder::alloca-args`/`store-arg`/`build-call`（既知の関数への直接call、
固定ABI`(args-ptr, argc)`を既存`load-arg`と対称な形で構築）。`ast_bridge.rs`に
`Expr::Labels`/`Expr::Apply`の実翻訳を追加（`Expr::Apply`は呼び出し先が現在の
`labels`兄弟/自分自身を指す`Var`のときだけ`(apply name arg...)`に、それ以外は
まだ`unsupported`——indirect/boxedディスパッチは後続Stageの仕事）。`compiler.rs`は
`builder`/`env`に加え`fn-env: HashTable<string,llvm-function>`を全ヘルパー間で
明示的に引数として渡す設計に変更（兄弟ごとに別のbuilder/blockでボディをコンパイル
する必要があり、Phase 1のようにクロージャで閉じ込める方式では対応できないため）。
LLVM関数名は`外側の関数名$内側の名前`でマングルし、同一moduleを共有する別の
`defun`の同名labels関数と衝突しないようにした。TDD: `tests/compile_test.rs`に
3件（直接呼び出しの生builtinテスト、compile-function直接呼び出しテスト、
`(compile name)`経由の実ソーステスト）、`tests/compile_file_test.rs`に
AOT版+JIT/AOTペアテスト2件追加。全件green、clippy警告0、複数回連続実行で安定。

**Stage 2完了**: `labels`の外側スコープキャプチャ対応。新規`src/compile/freevars.rs`
の`labels_free_vars(defs, outer_direct)`——`labels`ブロック**全体**の自由変数を
1つのリストとして集約（defごとに別々のリストを持たせるのではなく、全兄弟が
**同じ1つの**捕捉リストを共有する設計。理由: 兄弟Aが兄弟Bを呼ぶとき、Bが必要とする
捕捉変数をAが転送できる必要があるが、Aの「自分の自由変数」だけではBの分が
欠落する——全員に同じリストを持たせれば、各兄弟が自分のprologueで全捕捉変数を
受け取っておくだけで、誰が誰を呼んでも転送できる。詳細は
`labels_free_vars`/`compile-labels`のdocコメント参照）。`outer_direct`引数は
外側の`labels`がすでに直接呼び出し解決済みの名前（ネストした`labels`が外側の
兄弟を参照する場合に誤って捕捉と判定しないための除外——現状ネストした`labels`
自体は未対応・未テストだが、Stage 1から継承していた“外側兄弟への直接呼び出しは
可能”という前提を自由変数解析でも壊さないための安全策）。

ABI拡張: 捕捉ありの`labels`ブロックは**ブロック内の全兄弟**が`(args, argc, env,
env_len)`の4引数ABIになる（捕捉無しの既存関数・トップレベル`defun`は2引数の
まま不変——既存テスト・Stage 1のABIに一切影響なし）。新規
`llvm-module::add-function-with-env`（4引数ABIで宣言）、
`llvm-builder::load-env`（`load-arg`のenv版、`get_nth_param(2)`）、
`llvm-builder::build-call-with-env`（`build-call`のenv版、4引数を渡す呼び出し）。
`compiler.rs`は`compile-value`/`compile-apply`/`compile-assoc`/`compile-call-args`
全てに`captured: Sexpr`（現在の直接呼び出しスコープが共有する捕捉名リスト、
キャプチャ無しなら空リスト）を明示的に追加で引き渡す設計に拡張。新規
`bind-captures`（`bind-params`のenv版）、`compile-env-args`（呼び出し側で
捕捉変数の現在値を集めてenv配列を組み立てる、`compile-call-args`の対）、
`lookup-var`（`compile-var`の実体を切り出し`compile-env-args`と共有）。

**未決定だった保留事項3（if/比較演算の先行実装)は不要だった**: Stage 1と同じく
非再帰（自己呼び出し無し）の検証で十分だった——`labels`ブロック内の1兄弟が
別の兄弟を呼ぶだけで捕捉の転送ロジックを検証できるため、再帰の終了条件
（`if`+比較）は要らなかった。よって`if`/比較演算の実装は依然未着手のまま
（Stage 3着手時に改めて検討）。

**実装中に見つかった既存の根本問題（Stage 1から、今回修正）**: `ast_bridge.rs`の
`translate_labels`が、複数式bodyエラー（`fbody.len() != 1`）を返す際に、すでに
push済みの`def_values`のGC rootをpopせずreturnしていた——root stackの不変条件を
破る既存バグ（[[feedback-dont-excuse-by-age]]の通りStage 1からの既存問題かどうかは
重大度判断に使わない）。今回`captured_list`の追加と合わせて修正済み。

**既知の制約（今回判明、未解決）**: `compile-labels`は`inner-fn-env`を常に
`new-fn-env`で空から始めるため、ネストした`labels`の内側兄弟が外側の兄弟を
直接呼ぶケースは、`ast_bridge`側は`direct`集合で正しく直接呼び出しと判定する
ものの、`compiler.rs`側で`fn-env`の検索が失敗しコンパイル時panicになる
（Stage 1から存在、Stage 1/2どちらのテストでも露見しない——両方とも単一階層の
`labels`のみが対象のため）。ネストした`labels`のコンパイル対応に着手する際は
要修正（`compiler.rs`のモジュールdocコメントに記載済み）。

TDD: `tests/compile_test.rs`に3件追加（捕捉ありlabelsの生Sexprテスト、単一兄弟が
外側パラメータを捕捉する`(compile name)`経由のend-to-endテスト、兄弟Aが自分では
参照しない捕捉変数を兄弟Bのために転送するend-to-endテスト——共有リスト設計の
価値を直接立証）。`tests/compile_file_test.rs`に1件追加（捕捉ありlabelsを持つ
non-main helper defunがAOTで他の関数と同じmoduleに問題なくコンパイル・リンク
できることの確認——`main`自身はパラメータも`let`も使えないため捕捉元になる
変数が無く、捕捉ありlabelsを直接実行するJIT/AOTペアテストは書けない、既存の
`compiles_a_file_with_a_non_main_helper_function_too`と同じ制約）。全件green、
clippy警告0、5回連続実行で安定確認済み。

**Stage 3完了**: トップレベル`Expr::Call`（自己再帰含む）。

`ast_bridge.rs`に`Expr::Call`実翻訳（`translate_call`、`(call name arg...)`、
`Expr::Assoc`/`translate_apply`と同型、`Path`の`local()`のみ保持——名前空間越し
の解決は今回スコープ外）。同モジュールに`collect_call_targets`（JIT専用、
`Interp::compile_function`がコンパイラ本体を呼ぶ**前**に`Expr::Call`の呼び出し先
`Path`を収集する小さな専用ウォーカー——`ast_to_sexpr_scoped`が実際に再帰する
ノード形（`Assoc`/`Apply`/`Labels`/`Call`）だけを辿る設計、`unsupported`になる
ノードの中の呼び出しは収集する意味が無いため）。

`compiler.rs`に`compile-call`を新設——`compile-apply`の直接呼び出し経路と似て
いるが、`fn-env`ではなく`(get-function m name)`で`compile-function`が直接受け取った
`m`から検索（トップレベル`defun`は`fn-env`に登録されないため）。トップレベル
`defun`は外側スコープを捕捉できない（`check_defun`は常に空の`Env`から始まる）ため
`compile-apply`と違って常にプレーンな`build-call`（env配列が要らない）。

計画ファイル2.5節の訂正が的中: **AOTはコード変更不要**だった
（`compile::aot::compile_file`の既存の単純な前方ループのままで動く——`resolve_fn`の
制約上、呼び出し先は呼び出し元より必ずファイル内で先に定義済みなので、後の
`defun`をコンパイルする時点で呼び出し先は同じ共有moduleに本体まで含めて
すでに存在する）。

**JIT側のみ追加実装**（`Interp::compile_function`、使い捨てmodule方式が
Stage 1から変わらないため、AOTの「1つの共有module」とは事情が異なる）:
コンパイラ本体を呼ぶ**前**に`collect_call_targets`で呼び出し先`Path`を集め、
(1) 自分自身は除外（自己再帰は`compile-function`自身の`add-function`が
事前に解決するため何もしなくてよい）、(2) 残りが全て`Interp.compiled`に
既にあるか確認——無ければ「`g`を先に`compile`してください」という明確な
`Panic`、(3) あれば使い捨てmoduleに本体無しの外部宣言（新規
`declare_external_function`、`llvm-module::add-function`と同じABI構築を
共有する新規`compiled_fn_type()`ヘルパーに統合）をコンパイラ本体実行**前**に
追加、(4) コンパイラ本体実行後、JITエンジン生成時に`engine.add_global_mapping`
で各外部宣言の実アドレスを配線（`compile::CompiledFn::new`に
`externals: &[(String, usize)]`引数を追加、`CompiledFn`自身も
`engine.get_function_address`で取得した自分のアドレスを`address()`として
保持——後で自分を呼ぶ別の`compile`にこの仕組みで使われる）。

**自己再帰のテストは`if`/比較演算無しでも書けた**: 実行はしない
（ベースケースが無いので実行すれば無限ループになる——`if`/比較演算は依然未着手）
が、`(compile f)`がエラーにならないこと・IRに自己呼び出し命令が出ることは
検証可能——`(compile fn-name)`自体はボディを実行しないため。`if`/比較演算の
実装は依然先送り（Stage 4着手時に改めて検討）。

TDD: `tests/compile_test.rs`に6件追加（生Sexprの相互呼び出し・自己再帰IR検証、
`(compile name)`経由の相互呼び出しend-to-end、未コンパイル呼び出し先への
明確なエラー、自己再帰の`(compile name)`成功確認）。`tests/compile_file_test.rs`
に2件追加（`main`が別関数を呼ぶAOT、JIT/AOTペア）+既存2件を更新
（捕捉ありlabelsヘルパーを`main`が実際に呼ぶように強化、コメントの
古い「未対応」記述を修正）。全件green、clippy警告0、5回連続実行で安定確認済み。

**Stage 4完了（2026-06-25、未コミット）**: 一般クロージャ（`ClosureBox`、escapeする
`lambda`値・`Expr::FnRef`・indirect apply）。

**着手前の設計変更（計画ファイル1.2節の「AOT静的リンク方式」を確定）**:
計画では`tl_closure_*`をRust側`#[no_mangle] extern "C"`シム関数として実装し、
AOT用に別途staticlibをビルドして`cc`にリンクする方式を想定していたが、
実装時に**ClosureBoxの全操作を直接LLVM IRとして生成する方式**（`malloc`/`free`
はRust shimではなく`Builder::build_malloc`/`build_array_malloc`/`build_free`
というLLVMの「レガシーmalloc宣言ヘルパー」を使う）に変更した。これにより
新規ビルド成果物（staticlib）が一切不要になった——`malloc`/`free`はAOTの
`cc`リンクが標準で解決し、JITのMCJITも未解決外部シンボルをプロセス自身の
シンボルテーブル（libcを含む）にデフォルトでフォールバック検索するため、
`add_global_mapping`の追加配線も不要だった（`tests/compile_test.rs`の
`a_closure_made_from_a_capturing_function_can_be_called_indirectly`が
実際にJIT実行できたことでこの前提を実証——malloc呼び出しを含むコードが
追加配線無しで動いた）。typelisp向けbuiltin名・シグネチャ・意味論は計画通り
（`build-make-closure`/`build-closure-env-get`/`build-closure-apply`/
`build-closure-retain`/`build-closure-release`）だが内部実装が変わった、
という整理——`registry.rs`の各doc commentに記載済み。

- **ClosureBoxのレイアウト**: heap上のフラットな`i64`配列。slot 0=fn_ptr、
  slot 1=env_len、slot 2=refcount、slot 3以降=捕捉値。`build-make-closure`
  が`build_array_malloc`で確保し各slotを埋める（捕捉値は呼び出し元のスタック
  alloca-args配列からコピー——スタック配列は寿命が尽きるため）。
  `build-closure-apply`はslot 0/1/3..を読んでboxの**外から**間接呼び出しする
  （`Builder::build_indirect_call`、LLVM15+のAPI）。**全てのclosure化対象関数は
  捕捉が無くても4引数ABI(`add-function-with-env`)で統一**——これにより
  build-closure-applyがABI分岐を持つ必要が無くなった（計画の暗黙の前提を
  明文化）。`build-closure-retain`/`-release`はこのコードベースで唯一、
  実際の条件分岐（`build_conditional_branch`+新規basic block）を生成する
  builtin——`compile-closure-release`内部に閉じており、compiler.rs自体に
  `if`を教えるものではない。循環キャプチャは既存方針通りリークを受容
  （トレーシングGCは追加しない）。
- **ast_bridge.rs**: `Expr::Apply`を3分岐に拡張（直接呼び出し=Stage1不変／
  即時呼び出しlambda＝**`translate_labels`への委譲**（単一defの非再帰labels
  ブロックとして翻訳、ClosureBox不要）／その他=indirect、新規タグ
  `(apply-indirect callee-form arg...)`——計画原案の`(apply (direct|indirect
  ...) ...)`統一は採用せず、Stage1-3が既に出荷済みの`(apply name arg...)`
  形を変えずに済む独立タグにした、これも設計変更点）。`Expr::Lambda`単体
  （escapeする値）は新規`(lambda name (captured...) (params...) body)`、
  `name`はプロセス全体で一意な合成名（`fresh_lambda_name`、ユーザーソースに
  無名前のため）。`Expr::FnRef`はトップレベル関数を非捕捉のforwarding
  `lambda`（`(call name ...)`を転送するだけの本体）として同じタグに翻訳——
  「関数参照値」を別の実行時表現にせず同じClosureBox機構に載せる設計。
- **compiler.rs**: `compile-lambda`（新規関数を`add-function-with-env`で宣言
  →本体を**常に新規・空の`fn-env`**でコンパイル→outer envから捕捉値を集めて
  `build-make-closure`）、`compile-apply-indirect`（callee式をcompile-value
  で評価→`build-closure-apply`）。
- **`freevars.rs`**: `lambda_free_vars`新設（`labels_free_vars`の単一関数版、
  siblings概念が無いため`outer_direct`引数も無い）。
- **当初の既知の未対応は後日解決済み（下記参照）**: `labels`兄弟を**値として**
  （呼ぶのではなく）参照するケース（例: `(labels ((f ...)) f)`）は実装当初
  ClosureBox化されず「unbound variable」で失敗していたが、後続の修正で解決した。
- TDD: 生builtinテスト2件（make-closure+apply、retain/release後も
  呼び出し可能なことのクラッシュフリー回帰テスト——この2件目でmallocが
  JITで追加配線無しに解決することを実証）、ast_bridge単体テスト8件
  （escapeするlambda・捕捉あり・名前一意性・即時呼び出しのlabels化・
  indirect化・FnRefのforwarding化）、freevars単体テスト2件、
  `tests/compile_test.rs`にend-to-end4件（即時呼び出し無捕捉/捕捉、
  escapeする捕捉lambdaを別関数からindirect呼び出し、FnRefを値として
  渡す）、`tests/compile_file_test.rs`にAOT1件+JIT/AOTペア1件。全件green、
  clippy警告0、5回連続実行で安定確認済み。

### labels兄弟を値として参照するケースの解決（Stage 4の続き、2026-06-25）

Stage 4完了時に見送った「`labels`兄弟を値として参照する」ケース
（`(labels ((f ...)) f)`が「unbound variable」で失敗する問題）を、
plan modeでの設計検討（他言語のクロージャコンパイル手法の調査込み）の後に解決した。

**調査結果**: Scheme/SML/OCamlの`letrec`、V8のJS Context、C#のdisplay classは
いずれも「相互再帰するクロージャ群が1つの共有environmentフレームを持つ」
という同型のパターンを採用している。本プロジェクトの既存設計（labelsブロック
全体で1つの共有`captured`リストを使う、Stage 2で確定済み）はすでにこの
パターンを踏襲済みで、欠けていたのは「共有している情報（`fn-env`と
`captured`リスト）を値参照の場面でも使い切る」という最後の1段だけだった
——新しいビルトインや実行時表現は不要だった。Rust（`Rc<RefCell<>>`）・
Swift/Objective-C（ARC）の「循環参照はリークするが許容する」という前例は、
本プロジェクトが`build-closure-retain`/`-release`のみでweak参照を持たず
循環リークを受容する既存方針の妥当性を裏付けた。

**設計**: `ast_bridge.rs`/`freevars.rs`は無変更（`Expr::Var`は常に
`(var name)`のまま——`freevars.rs`の自由変数解析はStage 2の時点で
すでに正しくこのケースを想定済みだったと判明した）。`compiler.rs`内の
`lookup-var`/`compile-env-args`（従来`compile-function`の`labels`リング
**外側**の独立トップレベル`defun`）を、`resolve-value`（リネーム）として
**リング内に移動**——`env`に名前が無ければ`fn-env`を見て、見つかれば
今のスコープの共有`captured`リストから環境配列を構築し`build-make-closure`で
ClosureBoxにラップする。`compile-env-args`も`fn-env`引数を追加して同じ
リング内に移動（トップレベル`defun`同士は相互再帰できないため、両者が
相互再帰する今回の拡張にはリング内への移動が必須だった）。

**判明した重要な事実**: この方式では「2つのClosureBoxが互いを指し合う
真の参照循環」はそもそも構築不可能——`resolve-value`のフォールバックも
`compile-lambda`も毎回新規にヒープ確保するだけで、構築済みのBoxを後から
書き換えて相互参照させる手段が無いため（Scheme/OCamlの「プレースホルダーを
先に確保→後で相互参照を埋める」というletrecクロージャの定石とは異なる、
本プロジェクトの方がより簡素な設計）。当初Stage 4が想定していた「循環
リーク確認テスト」は意味を持たないと判明し、代わりに「2兄弟が互いを正しく
区別して値参照できること」を確認するテストに置き換えた。

**自動的に解決した範囲**: ネストした`lambda`が兄弟を間接キャプチャするケース
（`compile-lambda`自体は一切変更不要——`compile-lambda`がこのlambdaの
ClosureBox env配列を構築する呼び出しは**外側**スコープ（`fn-env`がまだ
有効）で行われるため、`resolve-value`の拡張だけで自動的に動作した。
plan mode時点ではこの点を慎重にトレースして検証が必要と判断していたが、
実装後のテスト（`compile_dispatches_an_escaping_lambda_that_indirectly_captures_a_labels_sibling`）
で実際に確認できた。

**変わらない既知の制約**: ネストした`labels`が外側labelsの兄弟を参照する
ケース（既存の「known limitation」）は今回も解決しない——ネストした
`compile-labels`は常に`inner-fn-env`を空から始める設計のまま。

TDD: `tests/compile_test.rs`に6件追加（裸参照+indirect呼び出し、捕捉あり版、
自己参照のIR検証のみ、end-to-endのlabels-as-escaping-value、2兄弟の取り違え
検出、ネストlambdaが兄弟を間接キャプチャ）。全件green、clippy警告0、
5回連続実行で安定確認済み。

### ClosureBoxの自動retain/release挿入（2026-06-25、commit 96fa72eの続き）

`build-closure-retain`/`build-closure-release`はStage 4からビルトインとして
存在していたが、`compiler.rs`は一度も呼んでいなかった——全ClosureBoxがリーク
しっぱなしの状態だった。これを解消し、非循環構造でのrefcountを正しく保つ
自動retain/release挿入を実装した（ユーザー提案の「参照カウント＋周期的な
mark-and-sweep」というサイクル収集方針の前提となる、より基礎的な土台）。

**所有権規約（R1-R4）**:
- **R1（関数入口）**: 関数（トップレベル`defun`/`compile-lambda`生成関数/
  `labels`兄弟いずれも）はparams・capturedをbind後、Fn型の各名前を
  `build-closure-retain`で1回retain（`retain-bindings`）。
- **R2（関数出口）**: Fn型のbind済み名前を無条件に一括release
  （`release-bindings`）——ただし本体が文字通り`(var name is-fn)`で
  `name`がこの関数自身の束縛名と一致する場合のみその名前を除外
  （`bare-returned-own-name`）。除外するだけで十分（R1のretainがそのまま
  戻り値の+1として機能する）で、別途の保護retainは不要と判明した
  （当初の設計案では保護retainが必要と考えていたが、callee-retains-on-entry
  規約のもとでは不要であることが手計算のトレースで確定した）。
- **R3（"var"の二分類）**: `resolve-value`が`env`でヒット（borrowed）か
  `fn-env`のfallback（fresh、新規box構築）かを`name-is-borrowed?`/
  `form-is-borrowed?`で判定。
- **R4（消費コンテキスト別）**: call-arg/直接呼び出しのenv-slotでは
  fresh値のみコール後にrelease（`release-pending-args`、`malloc`が
  nullを返さないことを利用した0センチネル方式、`build-closure-release`の
  共有関数自体がnull許容になるよう改修）。escapeするClosureBoxのenv-slot
  （`compile-lambda`/`resolve-value`のfallback両方が使う）ではborrowed値の
  みstore時にretain。

**型タグをSexprに追加**: captured/param名リストを`(name . is-fn)`ペアの
リストに、call-arg（apply/apply-indirect/call共通）を`(is-fn . form)`
ペアのリストに、`Expr::Var`を`(var name is-fn)`の3要素タグに変更
（`ast_bridge.rs`の`tagged_sym_list`/`tagged_ast_list_to_sexpr`、
`freevars.rs`の戻り値も`Vec<(String, Type)>`化）。

**escapeするBoxの再帰的解放**: R4でescapeするBoxの捕捉スロットを
retainして独立所有権を持たせたため、Box自体が最終的にfree される時、
捕捉スロット中のFn型のものも再帰的にreleaseする必要がある
（Rustの`Drop`/Swiftの`deinit`カスケードと同型）。新ヘッダースロット
`CLOSURE_FN_MASK_SLOT`（捕捉スロットごとに1bit、`compiler.rs`の
`compute-fn-mask`がコンパイル時に純粋なホスト側計算で求める）を追加し、
`build-closure-release`を**実行時に自己再帰するLLVM関数**
`__typelisp_closure_release`（モジュールごとに1つだけ、
`Module::get_function`でメモ化）として再実装した。当初「Rust側コード生成を
再帰呼び出しする」設計を考えたが、捕捉先Boxの構造（env_len/fn_mask）は
そのBox自身の中にあり実行時にしか分からないため、コンパイル時のIR静的展開
では原理的に不可能と判明し、実行時自己再帰のLLVM関数に設計変更した。

**ハマったポイント（実装中に判明）**:
- 算術演算子（`+`/`-`/`*`）は**最初の引数から**ジェネリックな数値型を推論し、
  裸の整数リテラルは`i32`にデフォルトする——`(* 2 (pow2 ...))`のように
  リテラルを先頭に置くと、2番目の引数が`i64`を返してもチェッカーが
  「`i32`を期待したのに`i64`が来た」と拒否する。`i64`を返す算術には
  i64型がすでに確定している式（関数呼び出しや型付き変数）を**先頭**に
  置く必要がある（`(* (pow2 ...) 2)`のように）。
- captured/param名ペアの名前部分は`Sym`（`sexpr-sym-name`で取り出す）、
  `(var name is-fn)`タグの名前部分や`labels`defの名前は`Str`
  （既存の`sexpr-str`のまま）——両方`sexpr-str`で取り出そうとすると
  `car: not a cons`にはならず黙って違う型を返す、もしくは
  パターンマッチで`panic`する（実際には全end-to-endテストが
  `car: not a cons`で落ち、原因はこの取り違えだった）。

**新規ビルトイン**: `load-raw`（`store-arg`の読み取り版、任意の配列
ポインタ+indexから読む、`load-arg`はfunction引数専用で使えないため）、
`debug-closure-refcount`（テスト専用、ClosureBoxのrefcountスロットを
直接読む——本番コードからは呼ばれない）。

**スコープ外**: `if`/`let`/`loop`実装後の規約再検討、mark-and-sweepによる
サイクル収集本体（既存設計ではサイクル構築自体が不可能なため不要）、
retain/release対の重複除去（Swift ARC Optimizer的な最適化パス）。

TDD: `tests/compile_test.rs`に新規4件追加（捕捉クロージャを介した
1000回繰り返し呼び出しでrefcountが1のまま安定することを確認、
escapeするlambdaをreleaseすると捕捉していたクロージャも再帰的にrelease
されることを確認、`__typelisp_closure_release`がモジュール内に複数
escapeする値があってもIR上1回だけ定義されることを確認、bareなsibling
参照をcall引数として渡した場合にコール後release呼び出しがIRに現れる
ことを確認）+ 既存の生Sexprテスト9件を新フォーマットに書き換え。
全件green（compile_test.rs 38件、compile_file_test.rs 14件含む全テスト
スイート）、clippy警告0、5回連続実行で安定確認済み（commit 4176868）。

### if/let/比較演算の実装（2026-06-26完了）

上の「次にやること候補」のうち、影響範囲（＝ブロッキング度）で最優先と判断し
実装した: ベースケース付きの本物の再帰関数（factorial等）はif/比較が無いと
compile機能では一切書けない、唯一「無いと本質的に何も書けない」候補だった
ため（他の3候補——mark-and-sweep/ネストlabels/retain-release重複除去——は
いずれも「無くても動く」）。

**新規LLVM builtin**（`registry.rs`/`interp.rs`）: `build-icmp-lt`/`-le`/`-gt`/
`-ge`/`-eq`/`-ne`（`i64`比較、`icmp`命令の`i1`結果を`build_int_z_extend`で
`i64`化——「全ての値はi64」という既存方針に合わせる）、`build-cond-br`
（`icmp ne cond, 0`+条件分岐）、`build-br`（無条件分岐）。

**`ast_bridge.rs`**: `Expr::If`→`(if is-fn cond-form then-form else-form)`、
`Expr::Let`→`(let ((name-sym . value-form)...) single-body-form)`を実翻訳
（`let`はlabels/lambdaと同じ単一式bodyの制約）。`collect_calls`（JIT用の
事前宣言コレクタ）にif/letの再帰アームを追加——if/letの中の`Expr::Call`が
JIT事前宣言から漏れる既存の抜け穴で、if/letが実翻訳されることで初めて
表面化した。`freevars.rs`は変更不要だった（Stage 2の時点でif/letアームが
先回りして実装済みと判明）。

**`compiler.rs`**: `cur-fn: llvm-function`を新規にringへスレッディング
（`compile-if`が`append-block`で新しいbasic blockを追加する対象——`labels`
兄弟の`sib-fn`/`lambda`の`nested-fn`/最外殻の`f`のいずれか——を、深い再帰
呼び出しの中でも知る必要があるため、builder/env/fn-env/capturedと同じ理由で
明示パラメータ化）。`compile-bool`（`(bool b)`→`const-i64` 0/1）、
`compile-assoc`に型ガード（`i64`/`i32`以外の受け手は明確にpanic——比較演算が
`f64`にも同名で存在するため、ガードを追加せずに済ませると`(< 1.0 2.0)`が
ビット列をi64として誤解釈する新規バグになっていた）+比較演算6種を追加。

**`compile-if`のFn型分岐対応（retain-before-merge）**: 当初「関数境界が無いため
R1-R4の前提が破れる、今回はunsupportedにして将来に先送り」という案を立てたが、
「影響範囲が大きいものから実装する」方針に反するという指摘を受け実際に解決した。
既存のR1-R4規約は「call/apply/labels/lambdaの戻り値は関数境界を通るので
必ずfresh化される」という前提に立つが、`if`は関数境界を作らないため、何の
対策もしないとborrowedな値（例: 関数の引用パラメータそのもの）が分岐の結果
としてそのまま外に出て、呼び出し側が「`(if ...)`という非var形だから常に
fresh」と誤分類し誤ってreleaseしてしまう（二重解放のリスク）。解決策は
`compile-escaping-env-args`の「borrowedな値をescapeするClosureBoxへ折り込む際に
retainする」と全く同じパターンをifのmerge地点に適用するだけ——`compile-if-branch`
が分岐の値を確定させた直後、`is-fn`かつ`form-is-borrowed?`なら
`build-closure-retain`を1回挿入する。これにより`if`はFn型分岐を含めて
完全にサポートし、`ast_bridge`での型ガードは不要になった。`tests/compile_test.rs`の
`compile_if_retains_a_borrowed_branch_value_before_it_escapes`がrefcountを
直接読んでこの修正を実証している。

**`compile-let`**: CL `let`のセマンティクス（バインディング値は全て外側の
envに対して計算、`Checker::check_let`のコメントで確認済み）を守るため
`compile-let-values`（全値を新規accumulatorへ計算、env未変更）→
`bind-let-values`（env既存値を`saved`へ退避してから上書き）→body実行→
`restore-let-values`（`saved`から復元、無ければ`remove`）の3段に分離。
`let`はFn型バインディングを禁止しない——`env`への出し入れだけで完結し
既存の`name-is-borrowed?`/`form-is-borrowed?`（envに存在するかだけを見る）が
そのまま正しく機能するため、`if`と違って関数境界の問題が生じない
（fresh値を束縛してreleaseしないまま終わる場合は既存の循環参照リークと
同種の「リークするが壊れない」挙動になるだけ、と手計算で確認済み）。
既知の制約: 同一`let`内に同名バインディングが複数あると復元がやや不正確に
なる（実用上ほぼ起こらないため対応見送り、`bind-let-values`/`restore-let-values`
のdocコメントに明記）。

**テスト**: `src/compile/ast_bridge.rs`に8件追加（if/letの実翻訳、
`collect_call_targets`のif/let内Call検出など）。`tests/compile_test.rs`に
8件追加（bool literal、i64比較、型ガードpanic、if実行、letシャドーイング
復元、ifのFn型分岐retain回帰、そして本タスクの核心——`(compile fact)`で
`(defun fact ((n i64)) i64 (if (<= n 1) 1 (* n (fact (- n 1)))))`をJIT
コンパイルし`fact(10) = 3628800`を実際に検証するend-to-endテスト）。
`tests/compile_file_test.rs`に2件追加（AOT版+JIT/AOTペア）。全件green、
clippy警告0（`-D warnings`含む）、既存テスト（`'(if true)`を「未対応タグ」の
プレースホルダーに使っていた1件のみ、`'(match)`に差し替え）を含め
リグレッション無し。

### loop/break/return/setfの実装（2026-06-26完了、未コミット）

「影響範囲の大きいものから実装する」方針——前段の3候補
（mark-and-sweep/ネストlabels/retain-release重複除去）は本節執筆時点で
いずれも「無くても動く」のに対し、`loop`は唯一未対応のまま残った制御構造で、
`prelude.rs`の`while`/`dotimes`/`dolist`（ひいてはそれらに依存する大半の
リスト操作関数）が`loop`+`if`+`break`/`return`へ脱糖する以上、`loop`が無い
ことは「compile機能でリスト操作系がまるごとコンパイル不能」を意味していた
ため最優先で着手した。

**スコープが`loop`/`break`/`return`だけでは閉じなかった**: `while`等の
脱糖先`(loop (if (not ,test) (break) ()) ,@body (setf ,var ...))`は
ループ変数の更新に必ず`setf`（`Expr::Set`、これも`ast_bridge`未対応のまま
だった）を使う。`setf`をLLVMレベルで動かすには、ループの2回目以降の
反復が1回目の`setf`の結果を読める必要があり、これは（real LLVM `phi`
ノードを組まない本コンパイラの設計では）**全ローカル変数（param/capture/
let束縛）の表現をSSA値からalloca済みスタックスロットに変更する**ことを
要求した——`setf`単体の対応ではなく、`bind-params`/`bind-captures`/
`bind-let-values`/`resolve-value`/`retain-bindings`/`release-bindings`
全てに及ぶ表現変更になった（詳細下記）。

**新規LLVM builtin**: `block-terminated?`（`llvm-builder`、host-levelな
真偽値クエリ、`llvm_module_def::verify`と同系統）——`break`/`return`は
直近のループ脱出ブロックへ`build-br`で直接ジャンプするため、その後に
`compile-if`の通常経路（store-then-branch-to-merge）や`compile-loop-body`
の次フォーム処理を続けると「1ブロックに2つ目のterminator」という不正IRに
なる。両者とも、この新builtinで「直前のフォームが既にブロックを終端した
か」を確認し、終端済みならその後の処理をスキップする。

**`ast_bridge.rs`**: `Expr::Loop`→`(loop body-form...)`（各文を裸のまま
翻訳、`apply`/`call`の引数のような`is-fn`タグ付けは不要——ループ本体の
各文の値は`break`/`return`以外では決して消費されないため）、`Expr::Break`
→`(break)`（フィールド無し）、`Expr::Return`→`(return is-fn value-form)`
（値無し`(return)`は`Expr::Unit`相当の`(unit)`を明示的な値として代入——
新規タグを増やさず既存の`compile-value`の"unit"分岐に載せるだけにした）、
`Expr::Set`→`(set name-str is-fn value-form)`（`is-fn`は`typed.ty`——
`Set`ノード自身の型は*代入先変数*の型、`Checker::check_setf`参照）。
`collect_call_targets`（JIT用）もLoop/Set/Returnの再帰を追加。

**`compiler.rs`のローカル変数表現変更**（`loop`/`break`/`return`/`setf`の
本質的な前提）: `env: HashTable<string,llvm-value>`が指す先を「値そのもの」
から「1要素`alloca-args`スロットへのポインタ」に変更。`bind-params`/
`bind-captures`は読み込んだ値をスロットへ`store-arg`してからそのスロットを
`env`に登録、`resolve-value`は`(get env name)`で得たスロットを
`load-raw`してから返す、`retain-bindings`/`release-bindings`も同様に
loadを挟む。`bind-let-values`は計算済みの値ごとに新規スロットを確保し直す
（同じ`let`の再実行——ループ内に`let`がある場合の各反復——ごとに新しい
スロットができる設計、`compile-if`の自分のmergeスロットが既に持っていた
「ループ内で毎回alloca、スタック消費は許容」という前例をそのまま踏襲）。
`name-is-borrowed?`/`form-is-borrowed?`（`env`に名前が存在するかだけを見る）
や`compile-env-args`/`compile-escaping-env-args`（`resolve-value`越しに
既にload済みの値を受け取るだけ）は無変更で正しく動作した。
**既知の制約（意図的な設計判断）**: キャプチャされた名前への`setf`は
*このアクティビエーションが受け取った自分のスロットだけ*を書き換える
——元の変数や同じ論理キャプチャを共有する他のクロージャへは伝播しない
（tree-walkインタプリタの`RtValue::Closure`が`Rc<RefCell<Slot>>`で実現する
真の共有可変クロージャとは異なる）。`while`/`dotimes`/`dolist`はいずれも
同一アクティビエーション内の`let`束縛だけを`setf`するため実害は無く、
ドキュメント化のみで対応は見送った。

**`loop`/`break`/`return`のスコープ境界**: `loop-exit: Option<llvm-basic-block>`
/`loop-slot: Option<llvm-value>`を`cur-fn`と同様にring全体へ明示的に
スレッディング。`compile-loop`が自分の本体をコンパイルする際だけ
`Option::some`の新規ペアを設定（ネスト時の「直近のループ」解決は通常の
コールスタックのスコープだけで自動的に達成される、`cur-fn`/`captured`の
labels/lambdaネスト対応と全く同じ理屈）。`compile-lambda`/
`compile-labels-bodies`は自分の新規関数本体をコンパイルする際`None`に
リセット（`Checker::check_lambda`/`check_labels`がloop_stackを空にリセット
するのと対応——`break`/`return`は決して関数境界を越えない、CLの
`return`/`return-from nil`がそうであるように常に*直近のループ*だけを
脱出するため、本コンパイラはunwind/continuation相当の機構を一切持たずに
済んでいる）。`compile-labels`の*末尾*bodyは新規関数境界ではない
（`check_labels`が`args[1..]`を loop_stack変更無しでcheckする）ため、
外側から受け取ったペアをそのまま転送する。

**`break`/`return`の対応範囲は文の位置のみ**: ループ本体の直接の1文として、
または`if`の枝の*全体*としてのみ対応（`while`/`dotimes`/`dolist`が実際に
必要とする形はこれで全てカバーされる）。算術オペランドや呼び出し引数の中に
埋め込まれた`break`/`return`（例: `(+ 1 (break))`、型レベルでは合法——
`Never`は何にでも単一化されるため）は対象外——対応するには`compile-assoc`
の`build-add`前や`compile-call-args`/`compile-let-values`の「次のフォームへ」
ステップ全てに同じ`block-terminated?`チェックを追加する必要があり、
今回実際に必要だった範囲を超えるため見送った（2個のterminatorを持つ不正IR
が生成されるが、`llvm-module::verify`以外には検出されない）。

**実装中に判明した別の壁（本ステージ内で解消）**: `while`の脱糖
`(if (not ,test) (break) ())`が呼ぶ`not`はRust組み込みの自由関数で、
typelispの関数本体（AST）を一切持たないため、`Interp::compile_function`の
呼び出し先事前チェックが「先にcompileしてください」と拒否し続け、
`while`/`dotimes`を使う関数がどれもcompile不能だった。

### Rust組み込み関数の方針確定 + `not`をtypelispへ移行（同日完了）

ユーザーから方針指示: 「Rustで書かれた組み込み関数は、ライブラリとして
１つにまとめて、typelispインタープリターでもコンパイラでも、その
ライブラリを使う方針とします」「コンパイラの負担を減らすため、Rustでしか
書けない物以外はtypelispで書くように」。すなわち (1) 本当にRust必須な
組み込み（GCヒープへの直接アクセス等）だけ共有ライブラリにまとめ
インタープリタ/コンパイラ両方から使う、(2) それ以外（typelispで表現可能な
もの）はtypelispへ移し、コンパイラが特別扱いする対象を減らす、という
二段方針。

**`not`を即座にtypelispへ移行**: `not`はGCヒープ等のRust専用機能に一切
依存しない単純な`bool->bool`（`if`一発で書ける）と判明したため、
`registry.rs`の自由関数登録と`interp.rs`の`eval_builtin`分岐を削除し、
`src/prelude.rs`に`(defun not ((b bool)) bool (if b false true))`を追加
（`atom`等より前に定義——`defun`は前方参照不可のため）。これにより`not`は
他のユーザー定義`defun`と全く同じ経路でコンパイル可能になり、
**`while`/`dotimes`が実際にend-to-endでcompile可能になった**
（`(compile not)`を呼び出し元より先に呼ぶ必要がある——他の関数間
呼び出しと同じ既存の「先にcompileしてください」要件のままで十分、
`not`専用の特別扱いは一切不要）。

**実装中に見つけた既存テストの回帰3件**: `not`がRust builtinから
typelisp `defun`になったことで、「check用の`Interp`にだけprelude を
ロードし、実行用には別の素の`Interp::new()`を使う」という既存テスト
ヘルパー（`tests/eval_test.rs`/`tests/hashtable_test.rs`/`tests/macro_test.rs`
の`run_with_capacity_and_prelude`系）が`NoSuchFunction("not")`で落ちた——
`while`の展開が呼ぶ`not`は以前はRust builtin（`Interp`の状態と無関係に
常に呼べた）だったが、今は`defun`本体が実行用`Interp.fns`に登録されている
必要がある。修正は3箇所とも同じ: 実行フェーズも（素の`Interp::new()`では
なく）prelude済みの`check_interp`自身を再利用する（`Interp`はheap状態を
持たず`exec`呼び出しごとに引数で受け取るだけなので、GC圧テスト用の小さい
heapを渡すこと自体には支障がない）。`tests/redefine_test.rs`の
「builtin関数の再定義は常にエラー」テストも例として`not`を使っていたため
（preludeをロードしないこのテストでは`not`はもはや何も登録されていない
名前になってしまう）、同種のRust専用自由関数`car`に差し替えた。

**`dolist`はまだcompile不能——ただし理由がより根本的なものに変わった**:
`not`解消後も`dolist`は`consp`/`car`/`cdr`を要し、`car`/`cdr`は
（コンスセルへの直接アクセスのため）原理的にRust専用のまま残る。だが
実際に手前で先にぶつかる壁は`consp`自身の本体`(match s ((Cons _ _) true)
(_ false))`——`Expr::Match`（`Construct`/`FieldGet`/`FieldSet`も同様）が
`ast_bridge`に翻訳が無く、そもそもコンパイル済みコードに`Sexpr`値の表現が
無い（`registry::llvm_module_def`の方針「全コンパイル値は素の`i64`」を
拡張しないまま）。つまり「組み込み自由関数を呼ぶ機構」を作っても
**それだけでは`dolist`は動かない**——`Sexpr`をコンパイル値として表現し
`Match`/`Construct`をコンパイル対応させる、本ステージとは別の・より大きい
作業が前提になる。`tests/compile_test.rs`の
`compile_of_a_dolist_based_function_fails_clearly_on_the_uncompiled_match_special_form`
で現状の挙動（`Match`未対応によるエラー）を記録した。

TDD: `src/compile/ast_bridge.rs`に8件追加（loop/break/return（値あり/値無し/
is-fn）/setfの翻訳、collect_call_targetsのLoop/Set/Return内Call検出）。
`tests/compile_test.rs`に10件追加（setf単体、ループ内return、値無しreturn、
setf+if+returnによるカウントループ、bare breakのみ（Unit型、ハング/クラッシュ
が唯一の失敗シグナルである旨をdoc commentに明記）、ネストループ（内側breakが
外側を脱出しないことの検証）、compile-returnのborrowed値retain回帰
（compile-ifの同種テストの構成を再利用）、while/dotimesが実際にcompile・
実行できることの実証、dolistがMatch未対応で明確に失敗することの記録）。
`tests/compile_file_test.rs`に3件追加（loopベースのhelper関数のAOT版、
ネストループのAOT版、JIT/AOTペア）。`tests/prelude_test.rs`にnotのテストは
既存のまま（移行後も green）。全件green（既存テスト含む全スイート）、
clippy警告0（`-D warnings`含む）、5回連続実行で安定確認済み。

### Sexpr表現 + Match/Construct/共有Rustライブラリ 実装計画（2026-06-26起案、Stage 0-8全完了・2026-07-01）

`not`移行後も`dolist`はcompile不能なまま（前節参照）。原因は`car`/`cdr`が
Rust専用なこと自体ではなく、**コンパイル済みコードに`Sexpr`値の表現も
GCヒープへの経路も無い**こと（`registry::llvm_module_def`の方針「全
コンパイル値は素の`i64`」のまま）。ユーザーと設計を詰めた結果、以下の
方針で固まった。

**設計の要点**:
- typelispは静的型付けなので動的タグが必要なのは`Sexpr`型だけ（他の型は
  コンパイル時に表現が確定する）。`mem::Value`（`src/mem/value.rs`）の
  9バリアントを下位3bitタグの8通りに収める（`Nil`/`Bool`を1タグに畳む）：
  000=Fixnum(Int、61bit化)/001=Cons(生ポインタ、`Cell`は8byte以上アライン
  済みで安全)/010=Symbol/011=Str/100=Char/101=Path（いずれも対応する
  `SymId`/`StrId`/`PathId`をそのまま即値化）/110=immediate定数{Nil,False,
  True}/111=Float(ボックス化、ClosureBoxと同型のmalloc+refcount。循環
  不可能なので安全)。SBCL/CCL/Chez Scheme等の下位タグ付きポインタ方式を
  参考にした（CPythonの全ボックス化は今回不採用——ClosureBoxの実績を
  活かしたタグ付き即値の方が小整数/文字等を確保なしで扱える）。
- `Heap`はCL/Scheme実装一般と同じく「プロセスにつき1つの暗黙の大域状態」
  として扱う。コンパイル済みコードからは`Heap`ポインタを明示的に取得・
  引渡しせず、共有Rustライブラリの各プリミティブ（`rt-cons`等）が内部で
  グローバルから解決する。ABI拡張（全呼び出し箇所への引数追加）は不要——
  JIT側は`Interp`がコンパイル済み関数を呼ぶ直前に登録するだけ、AOT側は
  生成された実行可能ファイルが起動時に自前でヒープを確保・登録する
  （AOTは独立プロセスなのでそもそも「呼び出し元から渡す」相手がいない）。
- 共有ライブラリの各関数は**既存の`compiled_fn_type`と全く同じABI**
  （`i64 fn(i64* args, i32 argc)`）で実装する。これにより
  labels/closures Stage 3で作った「外部関数アドレスを`add_global_mapping`
  で配線する」既存機構をそのまま再利用でき、新しい呼び出し機構を作らずに
  済む。
- `Match`/`Construct`/`FieldGet`/`FieldSet`自体のロジック（`compile-match`
  等）は既存の`compile-if`/`compile-loop`と同じく全部typelisp
  （`compiler.rs`）に書く。Rust側に追加するのはビット演算
  （`build-and`/`build-or`/`build-shl`/`build-lshr`）と汎用malloc/free+
  offset load/store（`ClosureBox`の汎用化）だけ——`Match`の多分岐自体は
  既存の`build-icmp-eq`/`build-cond-br`の連鎖で足り新規Rost不要。
  `Construct`/`FieldGet`/`FieldSet`（`Option`/`Result`/`defstruct`)は
  `Sexpr`とは別経路（`mem::Heap`と無関係、ClosureBoxと同型のmalloc+
  refcountに乗せられるので`Sexpr`より作業が軽い）。

**ステージ分割**（Stage 1-6=クロージャ/loopと同等以上の規模を見込む）:

- **Stage 0（完了、2026-06-26）**: AOTがRust側`extern "C"`関数（ダミーの
  `rt_ping`）をリンクできるかのスパイク。JITは`add_global_mapping`の
  `externals`配線（labels/closures Stage 3と同じ機構）で問題なく動作。
  AOTは`cc <obj>.o <staticlib> -o <out>`で実際にリンク・実行できることを
  確認——ただし**`typelisp`クレート自身を`staticlib`化する素朴な方法は
  実用に耺えなかった**：`inkwell`経由でLLVM全体を静的に内包しているため、
  `cc`（C++ランタイムやzlib/libffi/terminfo等を自動リンクしない）が
  `operator new`/`__cxa_*`/`compress2`/`ffi_call`/`setupterm`等、未定義
  シンボルの山で失敗する（`llvm-config --system-libs`相当の知識が必要に
  なり、しかも巨大な実行可能ファイルになる）。対策として**ワークスペース
  分割**を実施：`crates/typelisp-mem`（`mem::Heap`/`Value`+`Error`、LLVM
  依存ゼロ）と`crates/typelisp-rt`（`rt_*`関数群、`typelisp-mem`にのみ
  依存、`crate-type=["rlib","staticlib"]`）を新設。メインの`typelisp`
  クレートは両方にパス依存し、`src/mem.rs`/`src/errors.rs`/
  `src/compile/mod.rs`の`runtime`は全部`pub use`の薄い再エクスポートに
  変更——既存コードの`crate::Heap`/`crate::Error`/
  `crate::compile::runtime::*`という参照は一切変更不要だった。AOTは
  小さい（LLVM不在の）`libtypelisp_rt.a`だけをリンクすればよくなった。
  `[workspace] default-members`に全メンバーを明記しないと、素の
  `cargo build`/`cargo test`が`typelisp-rt`の`staticlib`を生成しない
  （依存先としては`rlib`しか要求されないため）ことも判明、対応済み。
- **Stage 1（完了、2026-06-26）**: 共有ライブラリの土台——
  `typelisp-rt`に`set_active_heap`/`active_heap()`（`thread_local!`、
  `cargo test`の並行実行で複数テストが別々の`Heap`を持つため**プレーンな
  `static`だと競合する**——各OSスレッドは常に1つの`Interp`/`Heap`しか
  同時に扱わないのでスレッドローカルで十分）と、診断用`rt_heap_live_count`
  /AOT起動時専用`rt_heap_init`（`Heap`を確保してそのまま登録、プロセス
  終了まで解放しない意図的なリーク）を実装。JIT側は
  [`Interp::eval`のコンパイル済み呼び出し箇所](src/eval/interp.rs#L273)で
  呼び出し直前に`set_active_heap`、AOT側は`build_main_wrapper`が生成する
  `main`が`tl_main`を呼ぶ前に必ず`rt_heap_init`を呼ぶよう変更。JIT/AOT
  双方の単体テストで実証、全テストgreen・clippy警告0・3回連続実行で安定。
- **Stage 2（完了、2026-06-26）**: タグ付きi64表現＋ビット演算プリミティブ
  ——`build-and`/`build-or`/`build-shl`/`build-lshr`に加え、実装中に
  **`build-ashr`（算術右シフト）の必要性が判明**して追加：`build-lshr`
  （論理右シフト）だけでは符号付き`Sexpr::Int`の負数payloadが復元できない
  （上位ビットが符号拡張されず0埋めになり値が壊れる）ため、fixnumの
  タグ外しには`build-ashr`が必須、Cons/Symbol/Str/Pathのような符号無し
  添字/ポインタには`build-lshr`を使う、という使い分けを確定。いずれも
  既存の`llvm_builder_build_int_op`ヘルパ（`build-add`/`build-sub`/`build-mul`
  と同型）を再利用するだけで実装でき、新規Rustコードは各1行。
  `tests/compile_test.rs`に生のLLVMビルダー呼び出し（`ast_bridge`/
  `compiler.rs`を経由しない、Phase 0/1と同じパターン）で2件追加：fixnum
  (タグ0)の`shl`→`ashr`往復（0/正/負/61bit境界値で検証）、tag=2の
  symbol的な値の`shl`+`or`→`and`+`lshr`往復（タグとpayload両方が正しく
  復元されることを1回の呼び出しで検証）。全テストgreen、clippy警告0
  （`--workspace`）、3回連続実行で安定。
- **Stage 3（完了、2026-06-26）**: `rt_cons`/`rt_car`/`rt_cdr`/
  `rt_set_car`/`rt_set_cdr`を`crates/typelisp-rt/src/lib.rs`に実装。
  `encode`/`decode`関数でStage 2のタグ表（8タグ）の残り全部（Cons/Symbol/
  Str/Char/Path/immediate Nil・Bool）を実装——`Value::Cons`は
  `ConsRef::addr`/`from_addr`（新設、`*mut Cell`自体は`typelisp-mem`内部
  限定なので生アドレス`usize`として越境させる）、`SymId`/`StrId`/`PathId`
  も`as_u32`/`from_u32`を新設して同様に即値化。`Value::Float`のみ未対応
  （malloc+refcountのボクシングが要るため、`Construct`/ADT側の作業と
  合わせてStage 6以降に先送りし、明示的にabortする旨をdoc commentに
  記録）。**エラー時の挙動はabort方針で確定**：`extern "C" fn`の中で
  Rustの`panic!`を素で使うと、呼び出し元がJIT/AOT生成のネイティブコード
  （Rustのunwindテーブルを持たない）の場合に未定義動作になるため、
  `eprintln!`+`std::process::abort()`の`fatal`ヘルパーに統一
  （`HeapExhausted`/`NotACons`/不明な即値タグ等、全てのエラー経路で使用）。
  `rt_set_car`/`rt_set_cdr`の返り値は`Sexpr`としての再エンコードではなく
  `compile-unit`と同じ`0`（Unit型はSexprのタグ空間と無関係）。GCルート
  安全性（Stage 4で対応予定の「呼び出し元フレームが握っている他のSexpr
  値がGCで誤って回収される」問題）は明示的に未対応のままdoc commentに
  記録。TDD: `typelisp-rt`に8件（encode/decode往復×2、cons/car/cdr往復、
  set-car/set-cdr）、メインクレートにJIT経由でのcons round-trip検証
  （実際のHeapが1セル増えたことまで確認）を1件追加。全テストgreen、
  clippy警告0（`--workspace`）、3回連続実行で安定。
- **Stage 4（完了、2026-06-26）**: GCルート安全性——
  `rt_push_sexpr_root`/`rt_pop_sexpr_root`を実装（`rt_push_sexpr_root`は
  `build-closure-retain`同様、引数を無変更で返しチェーン可能）。容量4の
  小さいHeapで「rootedな値を1つcons→push-root→無関係なconsを20回連続
  （free list 4セルを使い切り複数回gcを誘発）→pop-root→car/cdrで内容が
  保持されていることを確認」というテストと、**対照として「rootしないと
  同じシナリオで実際に値が壊れる」ことまで実証するテスト**を追加（固定
  サイズアリーナの決定的なsweep——`(0..cap).rev()`で未マーク順に新free
  listを再構築するため、再利用順序が常に同じ——を利用、フレーキーでない
  ことを確認済み）。

  **scope変更の経緯（重要）**: 当初「コンパイル済みのループでconsし続ける
  テスト」を想定していたが、実装を進める中で2つの未解決ギャップが先に
  判明したため、それらは本ステージの範囲外として明示的に先送りした：
  (1) `Expr::Call`がコンパイル済み関数を呼ぶ際の引数/戻り値は今も
  `RtValue::Int`決め打ち（[interp.rs](src/eval/interp.rs#L266)）——
  `Sexpr`型の引数/戻り値を実際にtypelisp呼び出し構文`(f sexpr式)`から
  渡すには、ここにエンコード/デコードの橋渡しを追加する必要がある。
  (2) `cons`の引数を作るには`(Int 5)`のような`Sexpr`コンストラクタ
  （`Expr::Construct`）が要るが、`ast_bridge`はまだ未対応（Stage 6で
  対応予定）——つまりtypelispソースから新しい`Sexpr::Int`値を作る手段が
  Stage 6まで存在しない。この2点が揃うまでは「実際のtypelispソースで
  コンパイル済み関数にSexprを渡す」エンドツーエンドテストは原理的に
  書けないため、Stage 4のテストはStage 0/3と同じ高さ（Rust直呼び/生IR）
  に留め、上記2点はStage 5/6で実際に必要になった時点で対応する。

  **Stage 3の「接続」を本ステージで完了**: Stage 3時点では`rt_cons`等の
  Rust関数を実装しただけで、`compiler.rs`の`compile-call`から実際に
  呼べるようにする「接続」は未完了だった（Stage 3の文言「実装と接続」の
  後半）。本ステージで`compile-call`に`car`/`cdr`/`cons`/`set-car`/
  `set-cdr`→`rt_car`/`rt_cdr`/`rt_cons`/`rt_set_car`/`rt_set_cdr`への
  名前書き換えを追加（`compiler.rs`本体への変更はこの1箇所のみ、既存の
  `get-function`+`build-call`の仕組みは無変更で再利用）。Rust側
  （`Interp::compile_function`/`compile::aot::compile_file`）は、この5つを
  「`(compile ...)`済みでなければ呼べない」という既存チェックから除外し、
  常に（呼ばれるかどうかに関わらず）モジュールへ前方宣言——JITは
  `add_global_mapping`で実アドレスを配線、AOTは`typelisp-rt`の
  staticlibに対する通常のリンカ記号解決に委ねる（追加の配線不要）。
- **Stage 5（完了、2026-06-26）**: `Match`対応（Sexprスクルーティニー）+
  `Expr::Call`のSexpr引き渡し対応。`consp`/`null`/`atom`が実際にコンパイル
  可能になることを確認した。

  **`Expr::Call`のSexpr引き渡し**（Stage 4で判明していた前提ギャップを解消）:
  [interp.rs](src/eval/interp.rs#L266)のコンパイル済み呼び出し分岐が、呼び出し先の
  `FnDef.sig`（パラメータ型・戻り値型）を見て、各引数/戻り値が`Sexpr`型なら
  `typelisp-rt`の`encode`/`decode`（Stage 3で実装、本ステージで`pub`化——別
  クレートをまたぐため）でタグ付きi64に変換、それ以外は従来通り素の`i64`
  として渡すようになった（`type_is_sexpr(ty: &Type)`が判定）。`bool`戻り値
  （`consp`等の宣言型）は対象外と明示的に決めた——`Sexpr`専用の橋渡しが
  本ステージの宣言された範囲であり、`bool`/`char`/`f64`の戻り値忠実性
  （`RtValue::Int`のまま返ってくる）は別の・まだ手を付けていないギャップ
  として残し、`compile_test.rs`のテストでもこの制約を明示してアサートして
  いる（`if`/`and`等の条件位置でコンパイル済み述語をそのまま使うには
  この別ギャップの解消が必要）。

  **`ast_bridge.rs`**: `Expr::Match(scrut, arms)`は、scrutineeの型が
  `Sexpr`のときだけ実翻訳し（`is_sexpr_scrutinee`）、それ以外（`Option`/
  `Result`/`defstruct`）は引き続き`unsupported`——`Construct`が無いと
  そもそも構築できない型を`match`できるようにしても検証不能なため、
  Stage 6に先送り。`(match is-fn scrutinee-form ((pattern-form .
  body-form)...))`という形に翻訳する`translate_match`/`translate_arms`を
  新設。パターン自体も`pattern_to_sexpr`で独立したタグ空間に翻訳:
  `(pat-wild)`/`(pat-bind name-str)`/`(pat-ctor variant-i64
  (sub-pattern...))`/`(pat-lit n)`——`Pattern::Int`/`Bool`/`Char`の3種の
  リテラルサブパターンを1つの共通タグに畳み込み、`n`はその場でRust側が
  「コンパイル済み表現としての生i64」（`Bool`は0/1、`Char`はUnicodeスカラー
  値）に変換済みにした。理由: コンパイル言語自体に`char`/`bool`→`i64`の
  変換プリミティブが無く、`compiler.rs`側で変換しようとすると新規ビルトインが
  要るが、Rust側で前計算すれば不要になる。`Ctor`パターンの`type_name`は
  チェック済み（scrutinee型が`sexpr`である以上、再帰するサブパターンも
  必ず`sexpr`自身——`cons`の2フィールドが共に`sexpr`型のため）なので、
  各パターンの`type_name`フィールドを個別に検証する必要は無いと判断した。
  `collect_calls`（JIT用）にも`Expr::Match`の再帰（scrutinee+各armの
  body、パターン木自体は式を持たないので不要）を追加。

  **`compiler.rs`**: 新規トップレベル`defun`群（`sexpr-int`、
  `compile-sexpr-tag-test`、`compile-sexpr-field`、
  `compile-pattern-guard`、`pattern-bound-names`、`save-env-names`/
  `restore-env-names`）+ 既存`labels`リング内への新規シブリング4つ
  （`compile-match`、`compile-match-arms`、`compile-pattern-test`、
  `compile-ctor-subpatterns`）。`compile-match`は`compile-if`と同型の
  「1スロットの共有merge + 各armをbranch-on-failureの連鎖で試す」構造——
  各armは自分専用の`next-block`（失敗時の合流先）を持ち、ネストした
  サブパターンの全guardがこの同じ`next-block`を共有することで、
  「このarmの失敗経路は何箇所あっても復元(restore)は1回で済む」という
  設計にした。`compile-sexpr-tag-test`（タグ判定、`nil`/`bool`は
  `TAG_IMMEDIATE`を共有するためpayloadも見る）と`compile-sexpr-field`
  （フィールド抽出、`int`は`build-ashr`、`char`は`build-lshr`、`bool`は
  payload-1、`cons`は`rt_car`/`rt_cdr`呼び出し）は`typelisp-rt`の
  `encode`/`decode`テーブルの逆操作をLLVM IRとして地で書いたもの——
  `sym`/`str`の`Str`フィールドと`float`フィールドは未対応のまま明確に
  panicする（文字列・float boxingは別ステージ）。`Bind`サブパターンが
  無い`Wildcard`位置では`compile-sexpr-field`呼び出し自体をスキップする
  最適化も入れた（`rt_car`/`rt_cdr`の不要呼び出しを避ける）。

  **新規Rust builtin1個**: `rt_match_fail`（`typelisp-rt`）——全armの
  パターンが失敗した場合のトラップ。型検査器の網羅性検査により
  型付きプログラムでは到達不能だが、コンパイラ自身のバグに備えて
  `fatal`（abort）する。既存の`rt_extern_functions()`（JIT配線・AOT前方
  宣言の単一の情報源）に追加するだけで両経路に自動的に伝播した。

  **実装中に見つけた3件のバグ（いずれも修正済み）**:
  1. `let`の束縛値は`expected: None`で検査されるため、`(let ((expected
     (if ... 0 ... 7 ...))) (const-i64 builder expected))`のような
     コードは`expected`がデフォルトで`i32`になり、`const-i64`の`i64`引数
     要求と衝突して型エラーになった——`compile-sexpr-tag-test`で発覚。
     修正: `let`で受けずに`if`連鎖を`const-i64`の引数位置に直接埋め込み、
     関数呼び出しの引数として`expected: i64`の文脈で検査させた。
  2. `compile-ctor-subpatterns`がサブパターンのタグ判定に`sexpr-str`を
     使っていたが、タグシンボル自体（`(car p)`）は`Sym`であり`Str`では
     ない——`sexpr-sym-name`に修正（既存の`compile-value`自身のディスパッチが
     `(match (car e) ((Sym s) ...))`と直接パターンマッチしているのに対し、
     ここだけ誤って`sexpr-str`を使っていた）。
  3. `pattern-bound-names`（1パターンを処理）と
     `pattern-bound-names-list`（パターン列を処理）が真に相互再帰する
     設計だったが、トップレベル`defun`同士は相互再帰不可——`compile-value`
     & co.と同じ理由で、1つの`defun`の中に`labels`で両者を閉じ込める形に
     直した（ユーザーから見える振る舞いは変わらない、内部構造のみの修正）。

  **テスト**: `ast_bridge.rs`に9件追加（wildcard-onlyの`Ctor`翻訳、
  非Sexprスクルーティニーの`unsupported`、`Bind`サブパターン、ネストした
  `Ctor`サブパターン（`(Cons _ (Nil))`）、`Int`/`Bool`/`Char`リテラルの
  `pat-lit`畳み込み、armの複数式bodyのエラー、`collect_call_targets`の
  scrutinee/arm-body再帰）。`compile_test.rs`に5件追加——`consp`/`null`/
  `atom`をCons/Nil両方の値で実行（`atom`は`not`+`consp`という既にcompile
  済みの別関数呼び出しも経由）、`Bind`抽出+Sexpr戻り値往復（`my-car`、
  `(cons (Int 42) (Int 99))`→`42`）、ネストした`Ctor`パターン
  （`second-is-nil`）、リテラル`Int`サブパターン（`is-zero`）。既存の
  「`match`は未対応」を前提にしていた2件のテストを更新——
  `the_compiler_body_panics_on_an_unsupported_tag`は未対応タグの代表を
  `match`から`construct`に差し替え、`dolist`がcompileできないことを示す
  テストは「`match`未対応」ではなく「`dolist`の脱糖が生む`let`の複数式body
  （`,@body`の後に隠れた`setf`が続く）が`translate_let`の単一式body制約に
  当たる」という**新しく判明した、より正確な**理由に更新した——`Match`が
  動くようになったことで`dolist`のコンパイルがどこまで進むかが変わった
  ため、テストの前提も合わせて修正する必要があった。全件green、clippy
  警告0（`--workspace`）、5回連続実行で安定確認済み。
- **Stage 6（完了、2026-06-26）**: `Construct`/`FieldGet`/`FieldSet`対応
  （一般ADT）。`check_construct`は`Sexpr`自身の8バリアントと
  `Option`/`Result`/`defstruct`を同じ`Expr::Construct`ノードで表現する
  ——つまりStage 4で持ち越されていたギャップ「`cons`に渡す値を
  typelispソースで作るには`(Int 5)`のような`Sexpr`コンストラクタが要る」
  は、一般ADTのConstruct対応と全く同じ実装で同時に解決した。

  **設計の要点（malloc+タグ箱の汎用化）**: `Sexpr`自身は既存のタグ付き
  `i64`/`rt_cons`表現（Stage 2-5）にそのまま乗せ、`compile-construct`は
  ノードの**checked型**（`ast_bridge::is_sexpr_type`、レジストリ非依存で
  判定可能）で`compile-construct-sexpr`（ビット演算/`rt_cons`の逆操作）
  と`compile-construct-box`（一般ADT用、新規）に振り分ける。後者は
  `ClosureBox`の汎用化——`[variant-tag, field0, field1, ...]`という
  固定レイアウトの`malloc`箱（`AdtKind::Sum`/`Struct`どちらも同じ箱、
  `mutable`フラグはレイアウトに影響しない）。Rust側新規ビルトインは
  ちょうど4つ（`build-malloc`/`build-free`/`build-int-to-ptr`/
  `build-ptr-to-int`、`registry::llvm_builder_def`+`interp.rs`）——
  オフセットload/store自体は既存の`load-raw`/`store-arg`がポインタ型
  `llvm-value`に対して既に汎用的だったため再利用するだけで済み、
  TODOの見立て「Rust側に追加するのは...だけ」通りの最小追加で済んだ。
  **リファインカウント/解放はこの段階では一切行わない**（`build-free`は
  公開するが`compiler.rs`からは未使用）——`labels`サブリングの未参照
  `ClosureBox`が許容リークになっているのと同じ判断。`compile-field-get`/
  `compile-field-set`は箱を`build-int-to-ptr`で戻し`1 + idx`オフセットで
  読み書きするだけ（`idx`は`Sexpr`の`Int`では運べない——`load-raw`/
  `store-arg`のインデックス引数は`i32`だが`Sexpr::Int`は常に`i64`に
  decodeされ、コンパイル済みコードに narrowing 変換プリミティブが無い
  ——ので`ast_bridge::idx_unary_list`が「中身は無意味、長さだけが`idx`」
  という単項リストにエンコードし、`compiler.rs`側は既存の
  `sexpr-list-length`（元々`i32`を返す）で長さを取り出す回避策にした）。

  **「is-fn」タグの「kind」への一般化**: Stage 6で新たに必要になった
  Sexprルート挿入パス（後述）は、`ClosureBox`retain/releaseと**同じ
  「束縛点ごとの種別判定」が必要**だが判定軸が違う（refcount増減 vs
  GCルートのpush/pop）。そこで`ast_bridge::tagged_sym_list`/
  `translate_let`が生成する各束縛ペアの第2要素を、`is-fn: Bool`から
  `kind: Int`（`0`=plain `1`=fn `2`=sexpr、`ast_bridge::binding_kind`）
  に一般化——`compiler.rs`の`retain-bindings`/`release-bindings`/
  `compute-fn-mask`/`compile-env-args`/`compile-escaping-env-args`/
  `bind-let-values`/`restore-let-values`/`compile-let-values`を機械的に
  追従させた（`Interp::add_compiled_function`が独自に持っていた
  `param_list`構築の重複コードも`ast_bridge::tagged_sym_list`呼び出しに
  統一——2箇所が同じ形を別々に作っていたことがバグの実際の原因になった
  ので、`pub(crate)`化して一本化）。call-argument側の`is-fn`タグ
  （`tagged_ast_list_to_sexpr`、`(is-fn . form)`）と`if`/`return`/`set`/
  `match`の`is-fn`タグ（`compile-if-branch`が読む方）は**意図的に未変更**
  ——後述のスコープ縮小（残課題）参照。

  **Sexprルート挿入パス**（Stage 4で実装済みの`rt_push_sexpr_root`/
  `rt_pop_sexpr_root`を実際に配線）: `kind = 2`（sexpr）の束縛に対して
  `retain-bindings`/`release-bindings`（関数引数・キャプチャ、関数の
  入口/出口）と`bind-let-values`/`restore-let-values`（`let`束縛、
  let開始/終了）がpush/pop呼び出しを追加。is-fnのretain/releaseとの
  決定的な違いは2点:
  (1) `let`束縛は（is-fnでは何もしない、リーク許容の箇所だが）sexprでは
  **必ず**push/popする——GCルート漏れは「リークするだけ」ではなく
  「後で読むと壊れている」という事故になるため。
  (2) `release-bindings`の`kind=2`分岐は`protected`（bare-returned-own-
  name）を**無視して常にpop**する——popは何も解放しない（ヒープ上の値
  自体は影響を受けない、GCの訪問対象から外れるだけ）ので、戻り値として
  出ていく名前であっても自分の役目（このスコープが持っていたルートの
  後始末）は完了させる必要があるという、is-fnのrefcount意味論とは
  別物の理屈になる。push/popの**順序や分割**は無関係（複数のpush/popが
  常にLIFOで正しくネストしてさえいれば、どの呼び出しがどの値を
  popしたかは関係ない——popした値自体は使わず捨てるだけなので）。
  **スコープを意図的に絞った範囲**: TODOで名指しされた
  `bind-params`/`bind-captures`/`bind-let-values`の3箇所のみ実装——
  call引数配列・env配列（`compile-call-args`/`compile-env-args`/
  `compile-escaping-env-args`）の一時的なSexpr値や、`if`/`match`の
  マージスロット・ループの`break`/`return`マージスロットを通過する
  **fresh**（束縛されていない）なSexpr値のGCルート保護は、依然未対応
  ——「束縛されていない一時値が次の割り当てで壊れうる」という同種の
  ギャップとして次段以降に持ち越し（Stage 5がbool戻り値の忠実性を
  明示的にスコープ外にした前例と同じ扱い）。

  **`Expr::FieldGet`/`FieldSet`のコンパイルロジックはJIT経由で
  end-to-endに実行できない（既知のギャップ）**: この2ノードは
  `Checker::check_defstruct`が自動生成するフィールドアクセサ/セッタ
  **メソッド自身の本体**としてしか出現しない（`p::x`/`(setf p::y v)`
  という呼び出し側の構文は常に`Expr::Assoc`に脱糖される）。一方
  `(compile name)`は`self.fns`（トップレベル`defun`）しか見ない
  （`Interp::compiled_fn_body`）——`self.methods`は対象外。つまり
  `compile-field-get`/`compile-field-set`自体は実装・`ast_bridge`単体
  テストで実証済みだが、実際のソースをコンパイルして実行する
  end-to-endテストは「`compile`がインスタンスメソッドも対象にする」
  という別の拡張（本ステージ未着手）が無いと書けない——Stage 5が
  bool戻り値の忠実性を未対応のまま残したのと同じ種類の、意図的に
  先送りした境界。

  **テスト**: `ast_bridge.rs`に8件追加（`Sexpr`自身のConstruct、一般ADT
  のConstruct、フィールド無しConstruct、`FieldGet`、`idx=0`の単項リスト
  境界値、`FieldSet`、`collect_call_targets`がConstruct引数/FieldGet/
  FieldSetへ再帰することの確認2件）+ 既存6件をkindタグ形状変更に追従
  （`untag_name`ヘルパをBoolからInt返却に変更、`translates_a_let_
  expression`を新しいネスト束縛ペア形状に追従）。`compile_test.rs`に
  5件追加：(1) `Sexpr`即値（`Int`/`Bool`/`Nil`、`Char`は別の既存ギャップ
  ——`compile-value`に`"char"`タグのハンドラが無い——のため対象外と
  明示）の構築→`Expr::Call`の既存decodeで往復、(2) `(Cons (Int a)
  (Int b))`構築→同じ関数内で`match`により分解→合計を返す
  end-to-end往復（`rt_cons`呼び出しを経由）、(3) `Option::some`の箱を
  構築し、戻り値の生アドレス（一般ADT箱は`Expr::Call`境界で未対応の
  ため`RtValue::Int`のまま——同種の既知ギャップ）を`unsafe`で直接読んで
  タグ/フィールドを検証（`debug-closure-refcount`と同じ流儀）、
  (4) `defstruct`の`point::new`で同じ箱表現を確認、(5)
  「`let`束縛のSexprローカルが、無関係な多数の割り当て後も内容を
  保持する」回帰テスト——`bind-let-values`のpush呼び出しを一時的に
  無効化すると実際に値が壊れて失敗することを確認した上で実装に戻した
  （Stage 4の「rootしないと壊れる」対照テストと同じ実証スタイル）。
  全体テスト3回連続実行で安定、clippy警告0（`--workspace`）。
- **Stage 7（完了、2026-07-01）**: 文字列対応——`Sexpr::Str`のConstruct/Match/
  FieldGetと、それを実用にする最小限の`String`組み込みメソッド
  （`length`/`ref`/`eq`/`append`）のコンパイルに対応。スコープは意図的に
  `str`（`Sexpr`の`str`バリアント兼`Type::Str`本体）のみに絞り、`sym`
  バリアント（タグ付き即値が`StrId`ではなく別テーブルの`SymId`であり、
  名前を文字列化するには別のインターン処理が要る）は既存のまま未対応で
  残した——両者とも表面上は`Type::Str`型の1フィールドという同じ形をして
  いるため、既存コードの`sym`/`str`をひとまとめに扱うコメントは誤解を招く
  として本ステージで書き分けた。

  **設計の要点（タグを剥がさない、という唯一の例外）**: `int`/`char`/`bool`の
  フィールド抽出（`compile-sexpr-field`）はタグを剥がして裸の`i64`/scalar/
  0-1にするが、`str`は逆に**タグを残したまま**にする——`typelisp-rt`の
  `rt_push_sexpr_root`/`rt_pop_sexpr_root`が`decode`（タグ判定）を経由する
  既存の汎用実装のままで保護できるようにするためで、剥がしてしまうと
  「ただの整数」と区別がつかなくなり、GC安全性の穴が再発する。この帰結として
  「裸の`Type::Str`値の表現は`Sexpr::Str`の表現と完全に同一」という単純な
  等式が成立し、`compile-sexpr-field`のstr抽出は`v`をそのまま返すだけ、
  `compile-construct-sexpr`のstr構築は`compile-value`の結果をそのまま
  返すだけで済んだ（追加のビット演算が一切不要）。

  **文字列リテラルの経路**: リテラルの内容はコンパイル時に確定しているが、
  それをターゲットプログラム（JITなら同一プロセスの生きた`Heap`、AOTなら
  起動時に`rt_heap_init`で新規に確保される別の`Heap`）に載せる手段が
  必要——`ast_bridge`の`Expr::Str`翻訳を、事前に`Heap::alloc_string`した
  `StrId`を埋め込む形から、**1文字ごとの`(int c)`ノードのリスト**
  `(str (int c0) (int c1) ...)`に変更した。これにより`compiler.rs`の新設
  `compile-str`は各文字を`compile-value`の既存`int`ハンドラでそのまま
  `const-i64`化し、`alloca-args`/`store-arg`で配列化して新設の
  `rt_str_new`（`typelisp-rt`、生の符号なし4バイトコードポイント列から
  `Heap::alloc_string`し、既にタグ付きの結果を返す）を呼ぶだけで済み、
  新しいリテラル埋め込み機構（LLVMグローバル文字列定数など）は一切不要
  だった。空文字列リテラルも`argc=0`の呼び出しとして自然に扱える。

  **`let`/パラメータ束縛の巻き込み**: `ast_bridge::binding_kind`を
  `Type::Str`も`KIND_SEXPR`（既存の`Sexpr`と同じタグ）に分類するよう拡張
  ——上記の「裸の`Type::Str`と`Sexpr::Str`は表現が同一」という設計の直接の
  帰結で、これをしないと`(let ((s (Str "hi"))) (Cons (Int 1) s))`のような
  コードで`s`のGCルートが一切保護されず、後続の`cons`が誘発する`gc()`で
  黙って壊れるという実在する穴になる（`Cons`フィールドの一時値は
  `compile-construct-sexpr`が個別に保護済みだが、`let`束縛はこの拡張なしでは
  無防備だった）。`compiler.rs`側の`retain-bindings`/`bind-let-values`等は
  `kind`の値だけを見る既存の汎用実装のままで、変更は`binding_kind`の
  1箇所のみで済んだ。

  **`String`組み込みメソッド4つ**: `compile-assoc`に`type-name`が
  `"string"`（`prim_type_path`が`Type::Str`に割り当てる名前——`Sexpr`の
  `"str"`バリアントタグとは別物なので注意）の分岐を新設し、`length`/`ref`/
  `eq`/`append`を対応する`rt_str_length`/`rt_str_ref`/`rt_str_eq`/
  `rt_str_append`（`typelisp-rt`新設4関数）への`alloca-args`/`store-arg`/
  `build-call`ラップに変換——`i64`/`i32`の算術・比較分岐と全く同じ形。
  `upcase`/`downcase`/`substring`/`lt`は未対応のまま「unsupported str
  method」にpanicする（スコープ外、YAGNI）。`Interp::compile_function`の
  事前ゲート（`self.methods`に無いbuiltinメソッド呼び出しを「未コンパイル」
  として弾く既存チェック）にも`"string"`を`"i64"`/`"i32"`と同じ除外リストに
  追加——ここを直さないと`compile-assoc`まで到達する前に「a builtin method
  with no compiled implementation」で弾かれてしまう。

  **テスト**: `typelisp-rt`に6件（`rt_str_new`往復×2、`rt_str_length`の
  Unicodeスカラー数カウント、`rt_str_ref`、`rt_str_eq`の内容比較、
  `rt_str_append`）、`ast_bridge.rs`は`translates_a_str_literal`を新しい
  `(str (int c)...)`形状に追従させ`translates_an_empty_str_literal`を追加、
  `compile_test.rs`に4件——`Construct`→`Match`→`length`のラウンドトリップ
  （空文字列含む）、別々に確保した同内容文字列の`eq`、`append`の`length`、
  そして`Cons`版の直接の前例
  （`compile_dispatches_a_function_that_keeps_a_let_bound_sexpr_local_rooted_across_many_allocations`）
  を模した「`let`束縛の`Type::Str`ローカルが多数の無関係な`cons`後も
  内容を保持する」回帰テスト。全体テスト（`cargo test`、workspace全体）
  green、clippy警告0。
- **Stage 8（完了、2026-06-27）**: `dolist`含む`prelude.rs`のリスト関数群が
  実際にコンパイル可能になることの実証＋全体回帰確認——8ステージ計画の
  本来の動機（[[typelisp-compile-labels-closures]]）そのものの達成点。

  **`let`の複数式body対応（本来の想定ブロッカー）**: `dolist`の展開
  `(let ((,lst ,lst-expr)) (while (consp ,lst) (let ((,var (car ,lst)))
  ,@body (setf ,lst (cdr ,lst)))))`の内側`let`は常に2文以上のbody
  （`,@body`に続く隠れた`setf`）——Stage5時点で「`match`ではなく`let`の
  単一式body制約が次の障害」と判明していた通りの想定内のギャップだった。
  `ast_bridge::translate_let`を`translate_loop`と同じ可変長body方式に
  変更（`(let bindings body-form...)`、0個も含む）、`compiler.rs`に新規
  `compile-let-body`（`compile-loop-body`と同じ`block-terminated?`連鎖
  だが、CLの`let`本来の「最後の式の値を返す」意味論を持つ点が違う）を
  追加して`compile-let`から呼ぶよう変更——想定通り`compiler.rs`本体への
  変更はこの1か所のみで済んだ。

  **未想定の発見: インタプリタ自身の`if`連鎖評価がワーカースレッドの
  デフォルトスタックを溢れさせるバグ**（`let`修正そのものとは無関係、
  本ステージで初めて十分な深さのASTがコンパイルされたために露見した
  既存の問題）: `dolist`展開全体をコンパイルしようとすると
  （`let`修正後も）スタックオーバーフローで落ちた。`Interp::eval`に
  一時的な深さ計測を入れて追跡した結果、`(list (Int 1) (Int 2) (Int 3))`
  のような3要素リストでは再現し1要素では再現しないことが分かり、
  `RUST_MIN_STACK=64MiB`で同じコードが成功することで「無限再帰ではなく
  スタック使用量の問題」と確定、`compile-value`/`compile-construct-sexpr`
  自身が`(if (eq s "int") ... (if (eq s "bool") ...))`という右に伸びる
  `if`連鎖（最大20分岐ほど）であることが原因と判明した——
  `Interp::eval`の`Expr::If`評価が`els`位置の入れ子`If`を
  `self.eval(heap, els, env)`で**再帰**していたため（Rustはdebugビルドで
  末尾呼び出し最適化をしない）、分岐1つ進むごとに新しいRust呼び出し
  フレームが積み上がっていた——`compile-value`の20分岐 ×
  `Construct`/`Match`の入れ子（3要素リストの構築で発生）が、テスト
  スレッドのデフォルトスタックを溢れさせるところまで積算されていた
  （1要素リストでは入れ子が浅く踏み切らなかった）。
  修正は`Interp::eval`の`Expr::If`評価をループ化——`els`が`Expr::If`である
  限り再帰せず`cur`を書き換えて回り続け、条件式自身の評価と最終的に
  選ばれた1つの葉だけが再帰する形にした（[interp.rs](src/eval/interp.rs)）。
  `compile-value`のような分岐数の多いタグディスパッチだけでなく、`cond`
  マクロの展開等、典型的なtypelisp全体のif連鎖に効くため
  影響範囲は本ステージの範囲を超える——根本原因のインタプリタ側を直した
  ので、`compiler.rs`側の分岐の書き方自体は変更不要だった。

  **テスト**: `ast_bridge.rs`の既存`let_rejects_a_multi_expression_body`
  を、複数式body/空bodyを翻訳できることを示す2件
  （`translates_a_let_expression_with_a_multi_statement_body`/
  `translates_a_let_expression_with_an_empty_body`）に置き換え。
  `compile_test.rs`の既存「`dolist`はまだコンパイルできない」テストを、
  `dolist`でSexprリストを合計する関数が実際にJIT実行されて正しい値
  （`6`）を返すことを示す
  `compile_dispatches_a_dolist_based_function_that_sums_a_sexpr_list_to_native_code`
  に置き換え。全体テスト3回連続実行で安定、clippy警告0（`--workspace`）。

### compile機能の残課題の一部対応（2026-06-27）

下記3件は完了（一般ADT箱のフィールドGCルート保護は同日後刻、[[feedback-impl-priority]]の
影響範囲基準で残課題群の最優先として追加対応）。**この時点で完了していなかった残課題
（mark-and-sweepサイクル収集・ネストしたlabelsのknown limitation・retain/release重複除去・
break/returnの対応範囲拡大・compile-if-branchのkind対応・compile-assocのユーザー定義
メソッド呼び出し対応）は、その後2026-07-01までにいずれも解消済み——本ファイル後続の各節を参照
（サイクル収集のみ実装ではなく「既存設計ではサイクル構築自体が不可能なため不要」という
判断で決着、前掲「ClosureBoxの自動retain/release挿入」節のスコープ外注記参照）**。

- **`compile`をインスタンスメソッド（`self.methods`）にも対応させる**
  （Stage 6で判明、2026-06-27対応済み）: `(compile type::method)`という
  名前を`Interp::method_key`/`resolve_fn_def`が`self.methods`に対して解決
  （`"::"`で分割し`Path::local()`で照合——`compile-call`の「修飾を捨てて
  ローカル名だけで見る」既存方針と同じ）するようにし、`compiled_fn_body`/
  `add_compiled_function`/`compile_function`は無変更で動いた（どちらも
  既に`name: &str`一本で抽象化されていたため）。

  これだけでは「コンパイルできるが呼べない」半端な機能になるため、
  `Expr::Assoc`の評価も`Expr::Call`と同じ「`compile`済みなら先にネイティブ
  実行」方式に揃えた——新規`compiled_methods: HashMap<(Path,String),
  CompiledFn>`フィールド+共通化した`call_compiled`ヘルパー（`Expr::Call`/
  `Expr::Assoc`両方が呼ぶ）。レシーバの表現問題（インタプリタ上の
  `RtValue::Struct`とコンパイル済みコードのmalloc箱は別表現）は、
  「コンパイル済みコードを一度でも経由した値は既に`RtValue::Int`（生
  アドレス）として流れている」という既存のStage6の挙動を利用して回避——
  レシーバが`RtValue::Int`であればそのまま渡す、純粋にインタプリタだけで
  作られた`RtValue::Struct`が来た場合は`Expr::Call`の一般ADT引数と同じ
  既存の（新規ではない）internal errorになる、という`Expr::Call`と完全に
  対称な挙動にした（新しい表現変換コードは一切書いていない——書けば
  mutableなdefstructの書き込みがコピー先にしか反映されない、という
  別の正しさ問題を生んでいたはず）。

  （`compile-assoc`がユーザー定義メソッド呼び出しを認識しない、という独立した
  gapが残った——翌日2026-06-28に解消、本ファイル後述の「`compile-assoc`のユーザー定義
  メソッド呼び出し対応」節参照。今回はメソッド単体のcompile+`Expr::Assoc`からの
  ネイティブ呼び出しのみが対象。）

  テスト: `tests/compile_test.rs`に4件追加（`point::x`アクセサの
  end-to-end実行、`point::set-x`セッタの破壊的書き込みが後続の読み出しに
  反映されることの確認、未知のメソッド名/未知の型名どちらも明確な
  `NoSuchFunction`になることの確認2件）。全体テスト3回連続green、
  clippy警告0。
- **一般ADT箱（`Construct`/`FieldGet`/`FieldSet`）のrefcount/解放と
  GCルート保護**（Stage 6で判明、2026-06-27に一部対応）: 現状`malloc`の
  みでリーク許容（変更なし、`ClosureBox`の未参照リークと同じ前例）。

  GCルート保護のうち**`Cons`構築のcar/cdrフィールドと通常の呼び出し引数
  配列の2箇所は対応済み**（本ファイルのこのすぐ上、Stage8の節とは別の
  2026-06-27の追加対応）: `compile-construct-sexpr`の`Cons`分岐
  （両フィールドが常にSexpr型と静的に分かるため型タグ不要、無条件に
  `push-sexpr-root`）と、`compile-call-args`（`ast_bridge::tagged_ast_list_to_sexpr`
  を`is-fn: Bool`から`kind: Int`へ一般化、Stage6の`tagged_sym_list`と
  同じ`binding_kind`を再利用——`(cons a b)`関数呼び出し経由のCons構築も
  `compile-call`経由でこの修正の対象に自動的に入った）。新規`push-sexpr-root`/
  `pop-sexpr-root`/`pop-sexpr-roots`ヘルパーを追加し、`compile-apply`/
  `compile-call`/`compile-apply-indirect`の3呼び出し元全てに配線。
  「rootしないと壊れる対照テスト」（小ヒープ+多数の無関係な`cons`で
  実際に破壊されることを先に実証、その後修正して直ることを確認、
  Stage4/6と同じ実証スタイル）2件追加。

  なお`compile-env-args`/`compile-escaping-env-args`は既存の（別の場所で既にrootされている）
  束縛値を**コピー**するだけで新規allocationを生まないため、追加対応は不要と判断済み
  （対応不要の理由をここに明記し、今後のセッションが同じ箇所を再検討しなくて済むように
  している）。

  **フィールド自体のGCルート保護も2026-06-27に対応完了**——[[feedback-impl-priority]]の
  影響範囲基準で残課題群の最優先と判断（一般ADTを使う全てのcompile済みコードに関わる
  正しさの欠落で、`compile-assoc`のメソッド呼び出し未対応より上位）。`Cons`構築/呼び出し
  引数の対応時には型タグが不要だったか既存の`is-fn`系タグを再利用できたが、今回は
  `Expr::Construct`のwire形式（`ast_bridge::translate_construct`）が一般ADTの各フィールドの
  型情報を運んでいなかったため、まず`tagged_ast_list_to_sexpr`（Stage8で`compile-call-args`用
  に作った`(kind . form)`タグ付けそのもの）を流用する形に拡張——`is-sexpr`が`true`
  （`Sexpr`自身のConstruct）の枝は無変更、`false`（一般ADT）の枝だけ`ast_list_to_sexpr`から
  `tagged_ast_list_to_sexpr`に切り替えた。

  **本質的な設計上の発見**: 単純に「フィールド保存直後に`push-sexpr-root`して該当する
  `pop-sexpr-root`を呼ばない」では直らない——`Heap.roots`は関数の出入りで必ずpush/popが
  対になる**単一のLIFOスタック**なので、ポップしないpushを混ぜると後続の無関係な
  `pop-sexpr-root`呼び出し全ての対応がズレて全体が壊れる。一般ADT箱の寿命はそれを構築した
  関数activationを超えうる（戻り値として返る、別の構造体に格納される等）ため、box自体が
  "deliberately leaked"（`build-free`されない）なのと対称的に、フィールドのルートも
  「**永久にリークする別系統のルート集合**」として扱うのが筋——`typelisp-mem::Heap`に
  LIFO規律と無関係な`permanent_roots: Vec<Value>`を新設し、`gc()`のmark phaseが`roots`と
  両方を辿るようにした（`push_permanent_root`、popは存在しない、by design）。`typelisp-rt`に
  対応する`rt_push_permanent_sexpr_root`を追加（`rt_pop_permanent_sexpr_root`は無し）、
  `compiler.rs`に`push-permanent-sexpr-root`ヘルパーを追加し、`compile-construct-box-fields`
  が`kind = 2`（Sexpr型）フィールドを格納した直後に呼ぶよう変更。

  「rootしないと壊れる対照テスト」（小ヒープ+`defstruct`のSexpr型フィールド+多数の無関係な
  `cons`で実際に破壊されることを先に実証——`a + b`が`333`ではなく`-1`になることを確認、
  その後修正して直ることを確認、既存ステージと同じ実証スタイル）を追加。
  `typelisp-mem`/`typelisp-rt`それぞれに、permanent rootがGCを跨いで値を保護することと
  通常の`roots`スタックのLIFO対応を乱さないこと（permanent pushを通常のpush/popの間に
  挟んでも、popされる値が変わらないこと）を確認する単体テストも追加。全体テスト3回連続
  green、clippy警告0（`--workspace`）。

  `compile-if-branch`経由の値のkind対応はこの時点ではまだ未対応（後日2026-07-01に解消
  ——調査の結果、実際に脆弱だったのは`compile-match`のscrutineeだったと判明。本ファイル
  後述の「compile-matchのGCルート漏れ修正」の段落を参照）。

## `compile-assoc`のユーザー定義メソッド呼び出し対応（2026-06-28）

[[feedback-impl-priority]]の影響範囲基準で残課題群の最優先だった「`compile-assoc`が
ユーザー定義メソッド呼び出しを認識しない」を解消——`p::x`等のメソッド呼び出しを
ボディに含む`defun`/`defmethod`自体が`compile`できない、構成可能性の根本的な欠落だった。

**核心の変更**: `compile-assoc`の`i64`/`i32`以外の受信型は従来即座に
`panic`していたが、その分岐を「`type-name::method`という名前へマングルし
`get-function`で検索して`build-call`する」という`compile-call`と同型の処理に
置き換えた。マングル名は`Interp::add_compiled_function`が単体の`(compile
"type::method")`実行時に`internal_name`として使う文字列と完全に一致させる
（新規`method_link_name`ヘルパーを両箇所で共有）ため、別のcompile済みメソッドを
呼ぶ側はそれが既にcompile済みかどうかを一切気にせず`get-function`に委ねられる。
自己再帰も無料で動く——`compile-function`自身の最初の一手（`add-function`）が
このマングル名を呼び出し前にすでにモジュールへ宣言しているため、
`compile-call`の自己再帰が成立する理屈とまったく同じ。

**前提として`Expr::Assoc`のwire形式を変更**: 従来`ast_bridge`は`Expr::Assoc`の
引数を`ast_list_to_sexpr`（タグ無し）で翻訳していた（「算術オペランドは常に`i64`、
`Fn`型になることはない」という前提）。ユーザー定義メソッドの受信者/引数は
`Fn`型/`Sexpr`型になり得るため、`Expr::Call`と同じ`tagged_ast_list_to_sexpr`
（`(kind . form)`タグ付け、Stage8で導入）に切り替え、`compile-call-args`の
既存のretain/GCルート処理をそのまま再利用できるようにした。型名自体も
`type_name.to_string()`（完全修飾）から`type_name.local()`に変更——
`Interp::method_key`の「修飾を捨ててローカル名だけで照合する」既存方針と
マングル名を一致させるための変更（現状ルートレベルの型しかテストされておらず
実害は無いが、将来ネストした型でも食い違わないようにする一貫性の修正）。

**Rust側の対応**: `ast_bridge::collect_call_targets`の内部実装を
`(Vec<Path>, Vec<(Path,String)>)`を一度の走査で集める`CallTargets`構造体に
一般化し、新規`collect_assoc_targets`で`Expr::Assoc`ターゲットも取得できる
ようにした。`Interp::compile_function`はこれを使い、各`(type_name, method)`を
(1) `i64`/`i32`ならスキップ（ネイティブ算術、外部呼び出し不要）、(2) `self.methods`に
無ければ即座に明確な`Panic`（`f64`/`str`/`char`等のビルトインメソッドは
そもそも`compile`対象のASTボディが無いため、対象外のまま）、(3) `self.compiled_methods`に
無ければ「先に`compile`してください」という明確な`Panic`（`Expr::Call`の既存方針と
対称）、(4) それ以外は`call_targets`と同じ要領でモジュールへ前方宣言し
`externals`に実アドレスを配線——という4分岐で処理する。

**テスト**: `tests/compile_test.rs`に5件追加——`defun`の本体から既にcompile済みの
`defstruct`フィールドアクセサ2つを呼んで合計を返す関数のend-to-end実行（本来の
依頼内容そのもの）、`defmethod`の自己再帰（`setf`によるフィールド更新+ゼロ引数の
インスタンスメソッド自己呼び出し）、未compileのユーザー定義メソッドを呼ぶ場合の
明確なエラー、未対応のビルトイン（`f64`）メソッドを呼ぶ場合の明確なエラー。
既存の生Sexprハンドフィードテスト（`compile-assoc`を直接呼ぶもの）はすべて
新しい`(kind . form)`タグ付き引数形式に書き換えた。`src/compile/ast_bridge.rs`にも
`collect_assoc_targets`の単体テスト3件追加。全体テスト3回連続green、
clippy警告0（`--workspace --all-targets`）。

本対応で当時の残課題リスト（前掲2026-06-27節）の1項目を解消。残りの項目も
2026-07-01までに全て解消済み——本ファイル後続の各節を参照。

## ネストした`labels`が外側`labels`の兄弟を参照できるよう修正（2026-06-29）

[[feedback-impl-priority]]の残課題群にあった既存の"known limitation"
（ネストした`labels`内のdef本体/trailing bodyが外側`labels`の兄弟をdirect call
しようとすると`compile-apply: no direct-callable function named ...`で
panicする）を解消した。

**根本原因の再診断**: 当初は「`compile-labels`が`inner-fn-env`を外側から
継承していない」という単純な話に見えたが、調査の結果、`compiler.rs`の
`env`/`fn-env`管理自体に設計原則の誤りがあると判明した。既存の3パターンを
比較すると——`loop`の`loop-exit`/`loop-slot`は再帰呼び出しの引数として
新しい値を渡す安全な値渡し、`let`の`env`は外側を直接mutateして`saved`に
退避し後で個別restoreする「mutate+restore」（同名重複bindingで復元が
狂うという既存の"Known limitation"付き）、`labels`の`fn-env`は常に空の
新規テーブル（今回の本題のバグ）——という3つの異なる（うち2つは問題のある）
パターンが混在していた。

ユーザー指摘によりCLHSの`labels`仕様（"the scope the created function
bindings encompasses the function definitions themselves as well as the
body"）を確認し、正しいモデルは「スコープのリストのリスト」だと整理した：
関数呼び出しの境界で新しいフレームのスタックを作り、`let`/`labels`はその
スタックに新しいフレームをpush/popする。`labels`が`let`と違うのは、各
def本体をコンパイルする新しいフレームスタックが、空からではなく
**このlabelsブロック自身が作ったフレーム（兄弟シグネチャ）を最初の要素として
共有してスタートする**点のみ——これがCLHSの記述の直接的な実装になる。

**設計**: 新しいRust組み込み型`Scope<V>`（`crate::check::registry::scope_def`/
`crate::eval::interp`の`scope_*`系builtin、`RtValue::Scope`）を追加した。
内部表現は「フレーム（`Rc<RefCell<HashMap<String,V>>>`、`HashTable`と
同じ表現）への参照のリスト」——`push-frame`/`pop-frame`はリストの
末尾操作、`clone-frames`は新しいリストを作って各フレームの`Rc`をclone
するだけ（フレームの内容は複製しない、ポインタコピーのみでO(深さ)）。
これは「新しいスコープに入るたびに名前空間を複製する」という効率の悪い
実装ではなく、Scheme/Lisp実装の「フレームのリスト」、JSのScope Chainと
同種の軽量なチェーン構造である。

`compiler.rs`の`env`/`fn-env`の型を`HashTable<string,V>`から`Scope<V>`に
変更（パラメータの個数は不変、型名のみ）。`compile-let`/`compile-match-arms`
は`push-frame`→bind→body→`pop-frame`に書き換え、`bind-let-values`/
`restore-let-values`/`pattern-bound-names`/`save-env-names`/
`restore-env-names`という個別キーのsave/restore関数群は全廃した
（`pop-frame`がフレーム単位で一括して戻すため、同名重複時の復元ミスという
"Known limitation"も同時に解消）。`compile-match-arms`は1点注意が必要
だった——LLVM基本ブロックの分岐とは無関係に、コンパイラ自身の実行は
1本のシーケンシャルな処理なので、成功パス・失敗パスそれぞれで`pop-frame`を
呼ぶと（旧`restore-env-names`の2回呼び出しは個別キーの復元なのでidempotent
だったのに対し）2回popしてフレームを1つ余分に消費してしまう。
`compile-if-branch`呼び出し直後の1箇所だけで`pop-frame`を呼ぶよう修正——
それ以降に生成されるどの基本ブロックのコードも、もう「popされた後の`env`」を
見るので問題ない。

`compile-labels`は外側から受け取った`fn-env`に対して`push-frame`し、
`declare-labels-siblings`/`compile-labels-bodies`にそのまま渡す（別の
`new-fn-env`は作らない）。各兄弟の本体をコンパイルする際の`fn-env`は
`(clone-frames inner-fn-env)`——外側の全フレーム（ネストしていれば祖先の
labelsのフレームも含めて）を共有しつつ、これにより何段ネストしていても、
どのレベルのlabels兄弟も`get`で見つかる（祖先を飛び越える参照も自動的に
動く）。

**captured-list（クロージャenv配列）の伝播**: `Scope`による名前解決の
修正だけでは、内側labelsの兄弟が外側labelsの（capture有りの）兄弟を呼ぶ
ケースで、その外側兄弟が必要とする捕獲値を呼び出し元が運べない問題が
残る。`freevars::labels_free_vars`に`outer_captured`パラメータ（直接の親
labelsブロックの共有captured-list）を追加し、結果の先頭に無条件コピー
してから既存の自由変数解析を行うよう変更——ネストのたびに外側を無条件
prefixとして継承するので、祖先を飛び越える参照でも必要な値は自動的に
伝播済みになる。`ast_bridge.rs`の`direct: &HashSet<String>`を引き回す
全関数（約20箇所）に同様に`outer_captured: &[(String, Type)]`を追加。
`translate_labels`はこのブロック自身が計算した`captured_names`を、各def
本体とtrailing bodyへの新しい`outer_captured`として渡す（直接の親は常に
このブロック自身）。`translate_lambda`は空の`outer_captured`を渡す——
`labels`と違い`lambda`は`compile-lambda`側で常に新規`fn-env`から始まる
ため、内側にネストした`labels`が継承すべき外側captured-listは存在しない。

**当初計画していた`fn-captured-len`は不要と判明**: 計画段階では
「呼び出し先ごとに期待するcaptured配列の長さを記録する並行Scope」が
別途必要と想定していたが、実装・検証の結果不要と判明した。`load-env`
（`llvm_builder_load_env`）はターゲット関数自身の`get_nth_param(2)`
（env配列）から、`bind-captures`が辿る「ターゲット自身のcaptured名リスト」
の長さ分だけGEP+loadするだけで、呼び出し元が渡した`env_len`という実引数
自体は本体側で一切参照されない。つまり呼び出し元が（無条件prefix継承の
おかげで）十分に長い配列を渡せば、ターゲットは自分が必要な先頭部分だけを
正しく読み取り、余分な要素は単に無視される——ABIミスマッチは実害なし。
これにより設計を1段シンプル化できた。

**テスト**: `tests/compile_test.rs`に6件追加——内側labelsの兄弟が外側
labels兄弟を（capture無し/capture有りそれぞれで）直接呼ぶケース、
trailing bodyが外側兄弟を呼ぶケース、内側兄弟が外側のcaptureに加えて
自分自身の追加captureも持つケース（`captured_inner`が`captured_outer`より
長い、`fn-captured-len`の必要性検証も兼ねる）、3段ネストで中間レベルを
スキップして祖父を直接呼ぶケース、内側兄弟が外側兄弟をbareで返して
ClosureBox化されるケース。新規`tests/scope_test.rs`に`Scope<V>`自体の
単体テスト9件（push/pop/get/set/clone-framesの基本動作、クローン後の
共有・独立性の確認）。既存の全テスト（labels/lambda/let/matchの兄弟参照系・
shadowing系を含む）は無修正でgreenのまま。`cargo +nightly miri test
--test mem_test`/`--test scope_test`もUB・リーク無し、clippy警告0
（`Scope`の内部表現が`clippy::type_complexity`を出したため`ScopeFrame`
型エイリアスを追加して解消）。

### `defstruct`フィールド単位の可視性 + assocメソッド呼び出し経路全体への可視性チェック導入（2026-06-30）

TODO.mdの「可視性（`FnSig::public`が常に`true`で実効性なし）」という記述は
古い情報と判明——2026-06-16のコミット`1a5d9f3`で関数/型/マクロ/変数の
クロスモジュール`pub`チェックは既に実装・テスト済みだった（`tests/
namespace_test.rs`の`private_fn_inaccessible_cross_module`等）。ただし
調査の過程で**真の欠落**を発見: `defstruct`が合成するフィールドgetter/
setterを含む`AssocFn`（型に紐づくstatic/instanceメソッド）は、`(use
Type)`によるstatic methodのbare化（`check_use`内）でしか`public`が
チェックされておらず、実際の呼び出し経路——`try_field_access`（`p::x`
糖衣構文）・`check_field_set`（`(setf p::x v)`）・`check_path_call`の
static member分岐（`Type::method`）・`try_instance_method`/
`check_instance_method`（`(method recv args...)`形式）——はいずれも
`af.sig.public`を一切見ていなかった。

**実装**: `Checker::assoc_visible(type_fq, af)`（`af.sig.public ||
self.same_module(type_fq.parent())`、`resolve_fn_path`の既存パターンと
同形）を新設し、上記5箇所すべてに適用。private扱いの場合は「forbidden」
ではなく「unresolved」として振る舞う（`resolve_fn_path`等の既存方針に
合わせ、存在自体を漏らさない）——`try_field_access`/`try_instance_method`
は`None`を返し、`check_field_set`/`check_instance_method`/
`check_path_call`は既存の「フィールド/パスが見つからない」エラーに
自然に落ちる。

**`defstruct`のフィールド構文拡張**: `(name type)`に加え`(pub name
type)`を許可（`Checker::parse_struct_fields`、`parse_param_pairs`とは
別関数——`defun`の引数リストには`pub`の概念がないため共有しない）。
**フィールドのデフォルト可視性はprivate**——構造体自身が`pub defstruct`
でも、フィールドごとに明示`pub`しない限りgetter/setterはモジュール外から
見えない（Rustの`pub struct { x: T }`と同じ規約）。これは他の`pub`（defun/
defmethod/defvar/defstruct自身）が一律「コンテナから継承せず明示
オプトイン」である設計と一貫させた判断。逆に非`pub`な構造体のフィールドを
`pub`にすることも許可——型自体は外から名指しできなくても、その型の値を
渡す`pub`な関数経由で個々のフィールドだけ覗ける、というopaqueハンドル的
パターンが成立する。

**テスト**: `tests/namespace_test.rs`に8件追加（`pub`構造体の非`pub`
フィールドが`p::x`/`(x p)`いずれの形式でもクロスモジュールから読めない
こと、setterも同様、`pub`フィールドはクロスモジュールで読み書き可能、
同一モジュール内では`pub`無しでも読める、非`pub`構造体の`pub`フィールド
がクロスモジュールでなお読めること）。既存`tests/struct_test.rs`の33件は
全てルート名前空間（`self.ns.is_empty()`によりvisibilityチェックが
自動的にバイパスされる）でのテストのため無修正でgreenのまま。全体
`cargo test`/`cargo clippy --all-targets`ともgreen・警告0。

## `setf`によるkind=2束縛再代入のGCルート未更新バグを修正（2026-06-30）

TODO.mdの「`compile-if-branch`経由の値のkind対応」のうち、`setf`部分だけを
具体的に実証・修正した。残り（`if`/`match`自身のmerge slot・`return`の
loop-slotを素通りする束縛されない一時値）は引き続き「store直後にload する
隣接命令なので単体では安全、消費先も呼び出し引数・Cons構築・一般ADT
フィールド経由で既に対応済み」という従来の分析のまま、この時点では未着手
（後日2026-07-01に決着——調査の結果`if`/`return`自身は常に安全と判明し、
実際に脆弱だった`compile-match`のscrutineeを実証テスト付きで修正。本ファイル
後述の「compile-matchのGCルート漏れ修正」の段落を参照）。

**根本原因**: `bind-params`/`bind-captures`/`bind-let-values`は`kind = 2`
（`Sexpr`型）の束縛に対して`rt_push_sexpr_root`でGCルートを1回だけ積むが、
これは値の**スナップショット**であり束縛の*スロット*を生きたまま参照する
ものではない（`typelisp-mem::Heap::push_root(&mut self, v: Value)`の
シグネチャ自体がそれを示している）。一方`compile-set`（`setf`）は束縛の
スロットを直接上書きするだけで、対応するGCルートには一切触れていなかった
——つまり`setf`で束縛を新しい値に再代入した瞬間、その新しい値は束縛の
スコープが終わるまでの間まったく無保護になり、無関係な別の割り当てが
起こす次のGCで破壊されうる（古いルートは再代入前の値を指したまま生き
続けるだけで、これ自体は実害のないリーク）。

`crates/typelisp-rt/src/lib.rs`に生のHeap API（JITを介さない）でこの
シナリオを直接再現するテスト
`a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root`
を追加し、実際に`(111 . 222)`が無関係なGCで別の値に化けることを確認した
——既存の`an_unrooted_value_is_corrupted_by_a_gc_triggered_by_other_allocations`
（一度もrootされない値）の「再代入によって既存rootが無効化される」版。

なお、この具体的な破壊は**JITコンパイル経由のend-to-endテストでは
再現できなかった**（`tests/compile_test.rs`の既存`let`束縛テストと同じ
手法で5通り以上のcapacity/反復回数を試した）——`load_compiler`だけで
6300セル前後がpermanent rootとして常時生存しており、その上で「`setf`で
再代入したばかりの未root値」と「ループの使い捨てallocation」が同じ
自由領域（昇順index優先のfree-list）のどのタイミングで衝突するかを
ピンポイントで作り込む必要があるため。生のHeap APIレベルでの実証は、
バグそのものが実在することの証明としては十分と判断した。

**修正**: `Heap::set_root(idx, v)`（既存だが未配線だった、ちょうどこの
用途のために用意されていたAPI）を呼べるよう、以下を配線した。

1. `typelisp-rt`に`rt_root_count`（現在のroot数を生のホストindexとして
   返す——他の`rt_*`と違いSexprタグを一切経由しない）と`rt_set_sexpr_root`
   （`(idx, new_value)`を受け取り`Heap::set_root`を呼ぶ、`rt_push_sexpr_root`
   と同じく新しい値をそのまま返す）を追加。`rt_extern_functions()`
   （JIT/AOT共通のフォワード宣言一覧、`src/eval/interp.rs`）に追加して
   両経路から`get-function`で見つかるようにした。
2. `compiler.rs`の`bind-params`/`bind-captures`/`bind-let-values`は
   束縛スロットを1語から2語の`alloca-args`に拡張——offset 0は従来通り
   値、offset 1は`kind = 2`の束縛だけが使う、その束縛のGCルートが
   積まれたスタック位置（`rt_root_count`を`rt_push_sexpr_root`の**直前**に
   呼んで取得——`push_root`は常に末尾に追加するため「現在のroot数」が
   そのまま「これから積まれるrootの位置」になる）。`kind`が0/1の束縛では
   offset 1は単に読まれない。
3. `ast_bridge::translate_set`の`(set name-str is-fn value-form)`タグを
   `(set name-str kind value-form)`へ一般化（Stage 6が呼び出し引数・
   `let`/パラメータ束縛に対して行った`is-fn: Bool`→`kind: Int`一般化の
   `setf`版——当時は意図的に未対応のまま残されていた）。`compile-set`は
   `kind`を読み、`(eq kind 1)`を`compile-if-branch`への既存の`is-fn`引数
   としてそのまま使い（Fn型のretainロジックは無変更）、`kind = 2`なら
   値スロットへのstore-argに加えてoffset 1のidxを読み出し
   `rt_set_sexpr_root`を呼ぶ。

**テスト**: `crates/typelisp-rt/src/lib.rs`に上記の破壊実証テストと、修正後に
同じシナリオが正しく保護されることを示す
`rt_set_sexpr_root_protects_a_setf_reassigned_value_across_a_gc_triggered_by_other_allocations`
の計2件追加。`src/compile/ast_bridge.rs`の`translates_a_setf`を新しい
`kind: Int`タグ形状に追従させ、`Sexpr`型ターゲットが`KIND_SEXPR`を運ぶこと
を示す`translates_a_setf_targeting_a_sexpr_local_with_kind_sexpr`を追加。
`tests/compile_test.rs`に、既存の「`let`束縛のSexprローカルが無関係な
大量割り当てを生き延びる」回帰テストの`setf`版
`compile_dispatches_a_function_that_keeps_a_setf_reassigned_sexpr_local_rooted_across_many_allocations`
を追加（JIT経由では上記の理由により破壊そのものは再現できないが、修正の
配線が実コンパイル済み関数を通しても壊れていないことを示す正の回帰
テストとして追加）。全体`cargo test --workspace`3回連続green、
`cargo clippy --workspace --all-targets -- -D warnings`警告0、
`cargo +nightly miri test --test mem_test`green（既存の
`typelisp-rt`単体クレートのMiriには本修正と無関係な既存の失敗2件が
あることを確認済み——`rt_heap_live_count_reflects_the_registered_heaps_real_state`
のstacked borrows違反と`rt_heap_init_registers_a_freshly_created_heap`の
意図的leak、いずれも変更前のコードでも再現する）。

## trait機構（deftrait/impl/where） + Vector\<T\> + doiter（2026-06-30）

TODO.mdの最優先課題`doiter`（「何に対する反復か」仕様未確定）の解決。ユーザーと協議し
「Iterトレイトを実装した型すべてで使える、汎用trait機構の上に作る」方針が確定、4フェーズで実装。

**フェーズ1: `Vector<T>`の再設計**。2026-06-23に専用`RtValue::Vector`バリアントもろとも
全面削除された経緯（[[typelisp-vector-defstruct-revert]]）を踏まえ、「専用RtValueバリアントを
作らず、ユーザー定義型（`defstruct`/`RtValue::Struct`）と対称に扱う」という当時の教訓を踏襲。
`StructData{ type_name, fields: Vec<RtValue> }`の`fields`を、`defstruct`の固定フィールド数とは
異なり**可変長コレクションとして扱う**ことで、`StructData`自体への変更なしに実現
（`src/check/registry.rs`の`vector_def`、`src/eval/interp.rs`の`eval_builtin_method`の
`"vector"`アーム——`new`/`push`/`get`/`set`/`len`、`HashTable`と同じ「Rust組み込みassocメソッド」
方式）。`tests/vector_test.rs`新設。

**フェーズ2: trait機構の基盤**。既存の単一静的ディスパッチ（`AdtDef.assoc`+`Expr::Assoc`+
`Interp::methods`）を可能な限り再利用する設計:
- `registry.rs`に`TraitDef{ name, assoc_types, methods: HashMap<String,FnSig>, ... }`新設、
  `Namespace.traits`、`AdtDef.impls: Vec<Path>`（実装済みtrait一覧）追加。
- `impl`登録時、各メソッド本体は**通常の`defmethod`と全く同じ実体**として対象型の`assoc`に
  挿入する（`Checker::check_impl`が`Self`/関連型名を対象型の具体型へ構文木レベルで置換してから
  `check_defmethod`を呼ぶ——置換は`Checker::subst_value`、受け手リストの**型位置のみ**を置換する
  必要があった点が落とし穴: `(self Self)`の変数名`self`と型キーワード`Self`は読み取り後どちらも
  同じ`"self"`という大文字小文字無視の文字列になるため、フォーム全体を素朴に一括置換すると
  レシーバの変数名まで型に化けてしまう——`((self Self))`のような受け手ペアの「型」要素だけを
  選んで置換し、変数名・本体は触らない実装に修正して解決）。
- 具体型に対する呼び出しはこれで既存コードパスのまま動く。ジェネリック関数本体の型変数レシーバ
  （`(where (Iter T))`宣言下の`(next it)`、`it: T`）は新規`Expr::TraitCall`ノードで表現——
  チェック時には実装型のPathを持たず、実行時にレシーバの値自身が持つ型タグ
  （`RtValue::Struct`の`type_name`等、`rtvalue_type_path`ヘルパー）を読んで、`Expr::Assoc`と
  **同じ`methods`テーブル**を引く。vtable等の専用間接構造ではなく、型消去インタプリタが要求する
  最小限の動的型タグ参照という整理。
- `Env`に`bounds: Rc<HashMap<String,Vec<Path>>>`追加（`where`節の宣言を関数本体チェック時のみ
  伝播、呼び出し側シグネチャには影響しない——呼び出し側での境界検証はこの時点では未実装、
  同日の後続作業で解消——下記「where節の関連型pin + 呼び出し側境界検証」節参照）。
- `tests/trait_test.rs`新設（具体型ディスパッチ、`where`境界経由のジェネリック呼び出し、型ごとの
  独立ディスパッチ、未実装trait呼び出しの型エラーを検証）。

**フェーズ3: `Iter`トレイト + `vector-iter<T>`**。`next: Self -> Option<Item>`という可変状態
モデル（呼び出しごとに`Self`の内部フィールドを`setf`で書き換える、Rust Iteratorに近い）。
`Vector<T>`自体ではなく別の`vector-iter<T>`（`vec`/`pos`の2フィールド`defstruct`）が`Iter`を
実装——`Vector<T>`自身に`next`を持たせると、複数の独立した反復状態を同時に持てなくなるため。
すべて`prelude.rs`に追加（`deftrait`/`impl`はtypelispソースとして書ける）。落とし穴2件:
(1) `(Option Item)`という型注釈は`(fn ...)`形式と誤認されパースエラーになる——このシステムでは
ジェネリック型は`Option<Item>`という単一トークン構文が必須（`(関数名 引数...)`形のリストは
`parse_type`が無条件に`parse_fn_type`へ回す）。(2) `impl Iter VectorIter<T>`のようにキャメル
ケースで書いた型名は、シンボルの大文字小文字無視正規化を経ても`defstruct`側の`vector-iter`
（ハイフン区切り）とは一致しない——`impl`の対象型名は実際の型のシンボル表記（ハイフン）と
揃える必要がある。`tests/iter_test.rs`新設。

**フェーズ4: `doiter`特殊形**。`while`/`dolist`/`case`/`do`はすべて`defmacro`（構文展開のみ）
だが、`doiter`は`var`の型が`coll`の`Iter::Item`（型チェッカーでしか分からない）に依存するため
唯一`defmacro`で書けず、`Checker::check_doiter`がcheckerレベルで`Expr`を直接構築する
（`while-let`が展開する`(loop (match val (pat body...) (_ (break))))`と同型のExpr木を、構文展開
ではなく直接組み立てる）。落とし穴: `def.assoc["next"].sig.ret`は`HashTable`の`get`等と同じく
`def.params`（例: `vector-iter<T>`の`t`）を型変数として含む**テンプレート**のまま登録されている
——`check_assoc_call`が行う`subst`/`subst_apply`による具体化を素通りしてしまい、`Item`型が型変数
`t`のまま漏れ出ていた。`coll`の具体型引数から`subst`を構築し`subst_apply`で具体化して解決。
`tests/doiter_test.rs`新設（合計計算、空Vector、`break`/`return`、`where`境界内、ネスト、
未実装trait呼び出しの型エラーを検証）。

**既知の制限（当時。2026-06-30の後続作業で関連型pin・呼び出し側境界検証は解消——下記
「trait機構: where節の関連型pin + 呼び出し側境界検証」参照）**: 関連型の具体指定（`where`節で
「Tの`Item`はi32」のような制約）は未実装——ジェネリック関数本体内で`Item`型の値に対する算術演算等
はできない（`doiter`/`Iter`自体の動作は問題ない）。呼び出し側での境界検証も未実装。`Sexpr`への
trait実装は意図的に対象外（要素型が固定されないリストにジェネリックな`Iter<Item>`を被せるのは
型システム上不適切、というユーザー判断——これは現在も変わらず対象外）。`Expr::TraitCall`は
compile機能（LLVM）では`unsupported`のプレースホルダのまま——`Iter`/`doiter`を使うコードは現状
compileできない（これも現在も変わらず）。

**テスト**: 全フェーズ完了後`cargo test`（`scripts/with-llvm-env.sh`経由）で既存含め全件green、
`cargo +nightly miri test --test mem_test`green。

### 追記: `doiter`をchecker特殊形から`defmacro`へ作り直し（同日）

上記フェーズ4で`doiter`をRust製checker特殊形（`Checker::check_doiter`、`var`の型を手動で導出し
`Expr`木を直接構築）として実装したが、ユーザーから「`var`の型がcheckerでしか分からないというのは
誤り。`dolist`同様ジェネリックなスペシャルフォームと考えれば`var`の型は型変数が指す型になるはずで、
`defmacro`で書けない理由にならない」という指摘を受け、検証の上で全面的に書き直した。

検証結果: `Checker::check_ctor_pattern`（`(some var)`のようなコンストラクタパターンのチェック）
は、scrutineeの型から`def.params`→`subst`→`subst_apply`で**`Pattern::Bind`の型を自動的に正しく
推論する**仕組みを既に持っており、`var`の型をマクロ展開時に知る必要は最初からなかった。
`while-let`マクロ（`prelude.rs`）のドキュメントコメント自体に「`val`は毎回再評価される
（`(next i)`のような状態変化を観察する呼び出しのため）」と明記されている——これは`doiter`が
`next`を繰り返し呼んで`Iter`の可変状態を観察する動作と完全に一致する。

`doiter`を`dotimes`/`dolist`と同じ「`gensym`で`coll`を一度だけ評価する隠しbinding」+
`while-let`呼び出しだけの`defmacro`に置き換え:
```lisp
(defmacro doiter (spec &rest body)
  (let ((var (car spec)) (coll-expr (car (cdr spec))) (tmp (gensym)))
    `(let ((,tmp ,coll-expr))
       (while-let ((some ,var) (next ,tmp)) ,@body))))
```
`(next ,tmp)`という呼び出しは、マクロ展開後に通常の`check_list`のhead解決
（`try_instance_method`→`check_instance_method`）を経由し、これはフェーズ2で実装した型変数分岐
（`Expr::TraitCall`生成）も含めてそのまま機能する——trait機構基盤（`TraitDef`/`AdtDef.impls`/
`Expr::TraitCall`/`check_instance_method`の拡張）は無変更。`Checker::check_doiter`関数と
`check_list`の`"doiter"`特殊形ディスパッチのみ削除。

**テスト**: `tests/doiter_test.rs`の既存7ケースを**一切変更せず**全green（実装方式が変わっても
挙動は完全に同一であることの実証）。`scripts/with-llvm-env.sh cargo test`で既存含め全件green、
`cargo +nightly miri test --test mem_test`green。`grep -n '"doiter"' src/check/checker.rs`が
ノーヒットであることを確認（checker側の特殊形ハードコードが完全に消えたことの確認）。

## trait機構: where節の関連型pin + 呼び出し側境界検証（2026-06-30）

TODO.mdの残作業冒頭2項目（[[typelisp-trait-mechanism-and-doiter]]の「既知の制限」だった2点）
をまとめて解消。両者は表裏一体——「呼び出し側で実際にItemが宣言通りか検証する」処理は関連型pin
機能なしには書けないため、1つの設計でまとめて実装した。

**設計の核**: `AdtDef.impls: Vec<Path>`（`check_impl`が書き込むだけで、それまでどこからも
読まれていなかった）を、実際に読む経路を新設するだけで両項目が解消できることが分かった
——`docs/language-design.md` §5.1の既存コメントも「この一覧を見るだけで判定できる」と
将来を見越して書かれていた。

**`where`節の文法拡張（関連型pin）**: 既存`(where (Trait T))`（厳密に2要素）を、
`(where (Iter T (Item i32)))`のように末尾に0個以上の`(AssocName ConcreteType)`を許す形へ拡張
（後方互換、pin無しは今まで通り）。内部表現として`TraitBound{ trait_path: Path, assoc:
HashMap<String,Type> }`（`src/check/registry.rs`）を新設し、`Env::bounds`/新設の`FnSig::bounds`
の値型を`Vec<Path>`から`Vec<TraitBound>`に変更。

**where節を関数シグネチャ自体に保存**: 旧実装は`check_defun`がwhere節をパースする**前**に
`FnSig`を登録していた（自己再帰のため）ため、パース結果が本体チェック用`Env`にしか残らず
レジストリ上の`FnSig`には一切記録されていなかった——これが呼び出し側検証が原理的に不可能
だった理由。where節のパースを`FnSig`構築より前に並べ替え、`FnSig.bounds`に保存することで
`Checker::check_call`がレジストリ越しに呼び出し対象関数自身のboundsを参照できるようにした。

**本体チェック側（pin代入）**: `check_instance_method`の型変数レシーバ分岐で`Expr::TraitCall`の
戻り型を`sig.ret.clone()`（trait定義上の`Option<Item>`、`Item`未解決のまま）としていたのを、
マッチした`TraitBound.assoc`で`subst_apply`してから使うよう変更。pin無し（`assoc`が空）なら
`subst_apply`は恒等関数で完全に後方互換、pin有りなら`Option<Item>`が`Option<i32>`に解決され、
ループ変数に対する算術演算が型チェックを通るようになる。

**呼び出し側検証**: `AdtDef`に`trait_assoc: HashMap<Path, HashMap<String,Type>>`（trait path →
{関連型名 → implが束縛した具体Type、ジェネリックimplなら自身のparamsを含んだまま}）を新設し、
`check_impl`の`(type AssocName Type)`処理で構造化して蓄積。具体型からこれを解決する
`resolve_trait_assoc_type`ヘルパー（`check_assoc_call`の`def.params`zipパターンを再利用）を新設。
`check_call`で型パラメータが全て解決済みになった後、`Ok(...)`を返す前に、各bound type paramの
具体型が`def.impls`にtrait pathを含むか・pinした関連型が`resolve_trait_assoc_type`の実際の
結果と一致するかを検証し、満たさなければハードエラーにする変更を追加。

**既知の限界（意図的にスコープ外）**: ネストしたジェネリック呼び出し——ある`where`境界付き
ジェネリック関数の中から、外側自身の型パラメータをそのまま渡して別の`where`境界付き関数を
呼ぶケース（Rustで言う境界の伝播）。呼び出し側検証は「裸の未解決型変数（`T`そのもの）」なら
スキップして既存のランタイムフォールバックに委ねるが、型変数を**含む**具体型（例:外側の`U`に
対する`vector-iter<U>`）の場合、トレイト実装の有無チェックは正しく動く一方、pinの一致チェック
は`U`という未解決のままの型と宣言値との構造的不一致で誤ってハードエラーになり得る。現状この
パターン（ジェネリック関数がジェネリック関数をpin付きで型変数のまま呼ぶ）を使う既存コードも
テストも無く、解決には同程度の追加コード（呼び出し元自身の`env.bounds`を`check_call`に伝播・
突き合わせ）が要るため、`doiter`のcount/sum制限と同種のYAGNI判断として今は広げないことにした
（[[feedback-impl-priority]]）。

**テスト**: `tests/trait_test.rs`に呼び出し側でtrait未実装の型を拒否するケースを追加。
`tests/doiter_test.rs`に`(where (Iter T (Item i32)))`で実際に`+`演算を行う`sum`版、および
pinと実際の関連型が食い違う場合に呼び出し側で拒否されるケースを追加（既存の`count`版は
pin無し経路の回帰確認としてそのまま維持、コメントのみ`Checker::check_doiter`という現存しない
関数名への古い参照を`check_instance_method`に修正）。全件green
（`scripts/with-llvm-env.sh cargo test`、compile機能含む）。

## compile機能: TraitCallランタイムdispatch・compile-matchのGCルート漏れ修正・retain/release重複除去パス（2026-07-01）

TODO.mdに残っていたcompile機能の3項目をまとめて対応。

**`Expr::TraitCall`の実装（型IDタグ付きボックス + 実行時ディスパッチチェーン）**:
ユーザーに「限定対応」（実装が1つしかなければ静的呼び出し扱い）か「汎用ランタイムディスパッチ」
（複数実装を実行時に型IDで分岐）かを確認し、後者を選択。設計:
- `Checker::check_instance_method`のTraitCall分岐で、その時点で登録済みの全impl実装型を
  `Registry`のnamespaceツリーを再帰的に走査して収集し、新設の`Expr::TraitCall::impls: Vec<Path>`
  に埋め込む（チェック時に確定——インタプリタの`Interp::eval`側TraitCall分岐は従来通り
  実行時型タグで動的解決するため`impls`を一切参照しない、この一覧は**compileのみ**が使う）。
- 一般ADTボックス（`compile-construct-box`）の先頭にtype-idスロットを新設
  （`ast_bridge::type_id_hash`——型のlocal名のFNV-1aハッシュ、プロセス横断でグローバルな
  型ID採番テーブルを持たずに済む）。スロット0=type-id、スロット1=variant tag、
  スロット2+i=フィールド、と1つずつシフト（`compile-field-get`/`-field-set`のoffsetも
  `+1`→`+2`に追随）。
- `ast_bridge::translate_trait_call`が`impls`（`i64`/`i32`/`sexpr`を除外——ボックス表現を
  持たないため）を候補リストとして`(trait-call method-str candidates-list arg-form...)`に
  翻訳、`compiler.rs`の`compile-trait-call`/`compile-trait-dispatch`が受信側の型IDを
  実行時に読み、候補を順に`build-icmp-eq`で比較する二分岐チェーン（`compile-if`と同じ
  `alloca-args`1スロットmergeパターンのn-way版）を生成。マッチしなければ`rt_trait_call_fail`
  （`rt_match_fail`と同型の`fatal`トラップ）。各候補の`type::method`は`Expr::Assoc`と
  全く同じ`method_link_name`前方宣言/wiring機構に相乗り
  （`ast_bridge::collect_trait_call_targets`が`collect_assoc_targets`と同じ
  `(Path, String)`ペア形式で追加するだけで`Interp::compile_function`側の変更は不要）。
- 制約: `impls`はチェック時点のスナップショットなので、ジェネリック関数を`check`した**後**に
  追加された`impl`はそのcompile済み呼び出しからは見えない（インタプリタ実行では問題なし、
  compileした場合のみの既知の制約——`Expr::TraitCall::impls`のdoc comment参照）。
- テスト: `tests/compile_test.rs`に単一impl版・複数impl版（`box-a`/`box-b`が同じtraitを
  実装し、実行時に正しい方へ分岐することを実証）を追加。既存の生ボックスレイアウトを
  直接読むテスト2件（`Option`/`defstruct`）をtype-idスロット追加後のoffsetに追随。

**retain/release対の重複除去（Swift ARC Optimizer的な最適化パス）**: `src/compile/arc_opt.rs`
新設。`build-closure-retain`が実際にはcallではなくインライン展開
（`inttoptr; getelementptr; load; add; getelementptr; store`の6命令、リファレンスカウント
スロットへの単純な+1）である点を利用し、`call void @__typelisp_closure_release`直前に
この6命令が同一クロージャ値に対して隣接している場合のみ両方をまとめて消去する、狭くだが
検証可能に安全なペフォールピープホール。全オペランドの依存関係をLLVM生値参照
（`AsValueRef`）で厳密照合するため、異なる2つのクロージャの隣接retain/releaseは誤って
消されない（否定テストで確認）。現行の`compiler.rs`のどのretain呼び出し元
（`retain-bindings`/`compile-escaping-env-args`/`compile-if-branch`）も実際には
隣接パターンを生成しないため、既存の全compileテストに対しては恒常的にno-op
（=0件除去）——`Interp::compile_function`（JIT）・`compile::aot::compile_file`（AOT）の
両方に無条件で組み込み済みで、将来のcodegen変更が隣接パターンを生成するようになった時点で
自動的に効き始める、前方互換の建て付け。テストは`tests/compile_test.rs`の既存
`closure_retain_then_release_leaves_it_still_callable`と全く同じ生IRを対象に、
除去件数が1件であること・除去後も正しく実行できることを確認する肯定テストと、
異なる2つの閉包値に対しては何も消さないことを確認する否定テストの2本。

**compile-matchのGCルート漏れ修正（実証テスト付き）**: 当初のTODOの表現は
「compile-if-branch経由の値のうち`if`/`match`のmerge slot・`return`のloop-slotを
素通りする一時値のkind対応」だったが、調査の結果`if`/`return`自身の分岐値は
（分岐ごとに毎回`compile-value`で作った直後に即座にstore→load、間に割り込みうる
アロケーションが存在しない）常に安全であることが判明——実際に脆弱だったのは
`compile-match`の**scrutinee**（マッチ対象）だった。scrutinee値自体がどこにも
ルート保護されておらず、パターンで取り出したサブフィールド（`pat-bind`）の安全性は
scrutinee自身がGCから到達可能であることに全面的に依存していたため、
`(match (fresh-heap-value) ((Cons a d) (アーム本体内で大量にアロケーションしてから a を返す)))`
という形——scrutinueがどの変数にも束縛されない**フレッシュな値**（関数呼び出し結果を
直接matchする等）かつアーム本体自身が他のアロケーションを行う——で、scrutineeのヒープ
セルがアーム本体中の無関係なアロケーションに巻き込まれて回収・再利用され、最終的に
壊れたデータが返ることを`tests/compile_test.rs`の
`compile_match_keeps_a_fresh_scrutinee_and_its_pattern_extracted_fields_rooted_across_an_arms_own_allocations`
で実証（小さいヒープ容量+大量アロケーションループで確実にGCを誘発、修正前は失敗することを
確認してから修正）。修正: `compile-match`がscrutinee計算直後に`push-sexpr-root`、
`merge-block`到達時（＝正常終了パスのみ）に`pop-sexpr-root`する形にブラケット化——
scrutinee自身さえ生きていればGCのmarkフェーズがそこから辿れるフィールドも全て
自動的にマークするため、`pat-bind`個々に別途ルートを積む必要はない。
`merge-block`は毎回新規に追加されるブロックで既存の`block-terminated?`ガードなしに
無条件でpopを積んでも不正なIR（terminator後の命令）にはならないため安全。
既知の残課題: アームが`break`/`return`で早期脱出し`merge-block`に到達しない経路では
このpopが実行されずルートが1つリークする——`break`/`return`をまたいで巻き戻す一般機構が
このコンパイラにまだ無いため（`labels`兄弟の未呼び出しboxリークと同種の、意図的に
スコープ外とした既知の限界。`compile-let`の`unroot-let-sexpr-values`も同型の
未対応ギャップを抱えていることを調査中に発見したが、今回のスコープ外として温存）。

## compile-letのunroot-let-sexpr-values不正IRリスクを修正（2026-07-01）

上の節で発見・温存した`compile-let`側のギャップを対応。`compile-match`のscrutineeと違い、
こちらは「リークするだけ」では済まない一段深刻な問題だった: `unroot-let-sexpr-values`は
`compile-let-body`が返った直後、**その時点のbuilder位置に無条件で**`call rt_pop_sexpr_root`
命令を積む。body内で`break`/`return`が実行された場合、`compile-break`/`compile-return`は
既にそのブロックへ`build-br`（terminator）を積んでいる——builderの挿入位置はterminator
命令を積んだ後もそのブロックのままなので（`build-br`のラッパー`llvm_builder_build_br`は
位置を移動しない）、直後の`unroot-let-sexpr-values`呼び出しはterminatorの**後**に
命令を追加してしまう。これはLLVMの基本ブロック不変条件違反（terminatorはブロック内で
唯一・かつ最後の命令でなければならない）で、単なるGCルートリークとは異なり不正なIRそのもの
——`compile-match`のscrutinee修正のように「新規ブロックに無条件で積むので安全」という
逃げ道が使えない箇所だった。

再現手順（`tests/compile_test.rs`の
`compile_let_does_not_emit_instructions_after_an_early_return_from_its_body`）:
`Sexpr`型（kind=2）の値を束縛する`let`の本体が`loop`の中で即座に`(return 42)`する、
という最小コードを`compile`させる。修正前にこのテストを実行すると、`(compile make-thing)`
自体が失敗し、`module.verify()`が`"Terminator found in the middle of a basic block!"`
を返すことを確認した——実際にJITエンジンへ渡してクラッシュさせるのはプロセスを巻き込む
危険な検証手段になる（LLVMの`report_fatal_error`は捕捉可能なRust panicではなく
プロセスabortになりうる）ため、安全に再現・確認するために合わせて
`Interp::compile_function`（JIT経路）に`module.verify()`呼び出しを追加した
（`compile::aot::compile_file`が既に同じ位置——`arc_opt`の後、実際のコード生成の前——で
行っているのと同じガード。これによりJIT経路でも不正なIRは「未定義動作としてクラッシュ」
ではなく「捕捉可能な`EvalError::Panic`」に変わる、という副次的な堅牢化でもある）。

修正本体: `compile-let`が`compile-let-body`の戻り値を受け取った直後、
`unroot-let-sexpr-values`呼び出しを`(if (block-terminated? builder) () (unroot-let-sexpr-values ...))`
でガード——`compile-if-branch`が自分の分岐値をstoreする前に同じ`block-terminated?`で
ガードしているのと全く同じパターン。ブロックが既にterminateされている（＝bodyが
`break`/`return`で早期脱出した）場合はpop自体をスキップする。これにより不正なIRの
生成は解消されるが、その経路では`kind = 2`束縛のGCルートは結局popされないまま残る——
これは`compile-match`のscrutinee用に既に文書化・許容している同種のリーク
（`break`/`return`をまたぐ巻き戻し機構がこのコンパイラにまだ無いための、`labels`兄弟の
未呼び出しboxリークと同種の意図的スコープ外）に完全に一致する形へ帰着した。
一般的な巻き戻し機構の設計自体は今回も引き続き未着手。

## `compile-loop`/`compile-break`/`compile-return`に一般的なGCルート巻き戻し機構を追加（2026-07-01）

上2節で「意図的スコープ外」として温存していた`compile-match`のscrutinee・`compile-let`の
`Sexpr`束縛、いずれの残課題（`break`/`return`早期脱出時にGCルートpopがスキップされる）も
同日中に一般的な機構で解消した。ユーザーからの「popをスキップしないようにはできないのか」
という質問がきっかけ——実装コストを提示した上でユーザーが「今すぐ実装する」を選択。

**設計**: `Heap.roots`（GCルートスタック）の実体は素の`Vec<Value>`（`typelisp-mem`の
`Heap`構造体）なので、1個ずつpop数を数え上げる代わりに「特定の深さまで一括truncateする」
操作を追加すれば、ネストした`let`/`match`スコープが何段開いていようと`break`/`return`が
一発で正しく巻き戻せる、という設計に着地した:

- `Heap::truncate_roots(len)`（`self.roots.truncate(len)`）を`typelisp-mem`に新設
- `typelisp-rt`に`rt_truncate_sexpr_roots`を新設（`args[0]`を`Heap::truncate_roots`に
  渡すだけ。`args[0] >= 現在のroot数`ならno-op——`Vec::truncate`と同じ挙動）
- `compiler.rs`の`loop-exit`/`loop-slot`が流れているのと全く同じ経路
  （`compile-value`始め、シグネチャが`(loop-exit ...) (loop-slot ...)`を持つ関数
  すべて25箇所）に、3つ目の道連れ引数`loop-root-base`（`Option<llvm-value>`）を追加。
  `compile-loop`が自分の本体に入る直前（pre-header）で`rt_root_count`を1回読み、
  それを`(Option::some root-base)`として`compile-loop-body`以下に渡す
  （`loop-exit`/`loop-slot`と全く同じ「ネストするたびに新しいtrioをインストールし、
  呼び出しスタックのスコープ復元に任せる」設計）。
- `compile-break`/`compile-return`は、ジャンプ（`build-br`）を積む直前に
  `rt_truncate_sexpr_roots(loop-root-base)`を無条件で呼ぶ。`break`/`return`と
  ループの間に`let`/`match`が何段ネストしていても、個々のpushを数える必要なく
  正しい深さまで一括で戻せる——`compile-match`のscrutinee・`compile-let`の
  `Sexpr`束縛、双方の既知の残課題がこの一箇所の追加で同時に解消した。

**副作用として発覚した既存呼び出し側の穴**: `compile-value`の全再帰呼び出しは
機械的な文字列置換（`(loop-exit ...) (loop-slot ...)` → 同+`(loop-root-base ...)`、
`loop-exit loop-slot` → 同+` loop-root-base`）で一括対応できたが、`compile-lambda`/
`compile-labels-bodies`の「新しい関数境界なので`(Option::none) (Option::none)`で
リセットする」3箇所（`compile-loop`自身の呼び出し元も含む）は`loop-exit`/`loop-slot`
という裸のシンボルではなく`(Option::none)`リテラルを直接渡していたため機械的置換の
対象外で、個別に3つ目の`(Option::none)`を追記する必要があった——見落とすと
チェッカーが`"function expects 9 argument(s), got 8"`で検出してくれた。

**既存テストで踏んだ地雷**: `tests/compile_test.rs`の
`compile_loop_retains_a_borrowed_return_value_before_it_escapes`は、
`Interp::compile_function`を経由せず`llvm-module::create`から手作りしたモジュールを
`create_jit_execution_engine`に直接渡すテストで、`rt_extern_functions()`による
自動前方宣言の恩恵を受けていなかった。`compile-loop`/`compile-return`が無条件で
`rt_root_count`/`rt_truncate_sexpr_roots`を呼ぶようになったことで、このテストの
モジュールにもこの2つの宣言（`(add-function m "rt_root_count")`のような、
本体なしの宣言のみ）が新たに必要になった——さらに、このテストは`Interp::eval`を
経由せず生成した関数を直接呼ぶため`set_active_heap`が一度も呼ばれておらず、
`rt_root_count`内部の`active_heap()`が`debug_assert!`で失敗し
「non-unwinding panic, aborting」（SIGABRT）でテストプロセスごと落ちた——
FFI境界を越えたRust panicはunwindできないため、これは"catchable"な失敗にはならない。
危険な実地検証を避けるため、まず`Interp::compile_function`（JIT経路）に
`aot.rs`と同じ`module.verify()`呼び出しを追加してから再現・確認する、という
安全な手順を踏んだ（このverify()呼び出し自体も今回のこの変更にとって
有用な恒久的な堅牢化——不正なIRを未定義動作クラッシュではなく捕捉可能な
`EvalError::Panic`に変える）。

**実証テスト**: `compile_return_truncates_a_sexpr_lets_gc_root_on_every_call_not_just_the_first`
（`tests/compile_test.rs`）。`Heap::root_count()`はコンパイル済みコードだけでなく
ツリーウォーク・インタプリタ自身の`Interp::sync_roots`（無関係なGC安全性の仕組み）でも
毎回のトップレベル呼び出しごとに一定量伸びるため、単純に「呼び出し前後で0のまま」とは
アサートできない——`Sexpr`束縛なし・ループなしの対照関数`trivial`と、`Sexpr`束縛+
即`return`の`leaky-inner`それぞれを500回ずつ呼び、両者の`root_count()`増分が
**一致する**（`leaky-inner`が`trivial`より余分にリークしていない）ことを検証する形にした。
`rt_truncate_sexpr_roots`の中身を一時的に無効化して確認したところ、
`leaky-inner`の増分だけが`trivial`のちょうど2倍（500回呼んで500余分にリーク）になり、
実際に1呼び出しにつき1ルートずつ着実に漏れていたことを確認してから元に戻した。

## `eq`/`eql`/`equal`/`equalp`をCommon Lisp準拠に再設計（2026-07-01）

Stage 7（文字列対応、compile機能）の作業中、テストコメントで`str::eq`を
「内容比較であってポインタ一致でないことを確認する」とCLの`eq`本来の仕様
であるかのように説明したところ、ユーザーから指摘を受けた。CLの
`eq`/`eql`/`equal`/`equalp`の正確な違いを調査した結果、`docs/cl-equivalence-catalog.md`
の当初設計（`case`が型を問わず`(eq a b)`で書けるよう、`eq`を型ごとに
「値の等価性」として再定義——`Str`は実質内容比較）がCL仕様と食い違って
いたことが判明——内容比較はCLでは`equal`/`equalp`の役割であり、
`eq`/`eql`は文字列に対して常に同一性判定のまま。ユーザーの指示で
4つとも正しく実装し直した。詳細な対応表・訂正内容は
`docs/cl-equivalence-catalog.md`の「5. 訂正（2026-07-01）」節を参照。

**最大の技術的分岐点**: `Type::Str`（プレーンな`string`型）の値は
インタプリタ内で`RtValue::Str(String)`という値型で、変数を読むたびに
`clone`（深いコピー）されており、「同じオブジェクト」という概念自体が
存在しなかった——`(let ((s "hi")) (eq s s))`ですら真の同一性判定が
成立しない。これを解決するため`RtValue::Str`を`Rc<str>`に変更
（`src/eval/value.rs`）——`Rc::clone`はポインタ複製（refcountインクリメント）
なので、同じ束縛を2回読んでも同一オブジェクトのままになり、`Rc::ptr_eq`
で正しい同一性判定ができるようになった。呼び出し箇所は10箇所程度で、
Rustの`Deref`/`AsRef`のおかげでほぼ機械的な追従で済んだ（`.into()`/
`.to_string()`/`.as_ref()`の使い分けのみ）。

**想定外の大きな副作用**: `compiler.rs`（自己ホスト型LLVMコンパイラ本体）
が、タグ・メソッド名・型名のディスパッチ全体で`(eq s "int")`のような
**文字列内容比較としての`eq`**に依存していた（48箇所）。`eq`の意味を
真の同一性に変えた瞬間、これらが全て「別々に確保された文字列は常に
不一致」になり、`compile-value: unsupported tag int`のような形で
自己ホスト型コンパイラ全体が機能しなくなった——`tests/compile_test.rs`
（interpreted経由でcompiler.rsを動かすテスト群）は偶然通っていたが、
`tests/compile_file_test.rs`（AOT経由、19件）で発覚。該当48箇所は
Pythonの正規表現一括置換（`\(eq (\S+) "` → `\(equal \1 "`）で`equal`に
置換して修正——このパターンに一致する箇所は全て文字列タグの比較のみで、
`(eq idx 0)`のような整数/Sexpr比較は無関係のため触れていない。

**設計の要点**:
- `eq`/`eql`: `Sexpr`は既存のまま（`Value`を直接比較、`Cons`/`Str`は
  同一性、他は値——immediateなので同一性と値比較が一致）。`Str`の`eq`は
  新規に`Rc::ptr_eq`で同一性判定に修正。`eql`は全型で`eq`のエイリアス
  として登録——このタグ付き即値表現では数値/文字がboxingされていないため
  `eq`と`eql`が理論上も一致する（CLでの両者の相違はboxed数値/文字での
  み生じる）。
- `equal`/`equalp`: `Str`/`char`に内容比較（`equalp`は大小無視）を新設。
  `i32`/`i64`/`f64`/`bool`は`eq`のエイリアス（静的型システム上、型跨ぎ
  比較は到達不能なため）。`Sexpr`は`prelude.rs`の既存`equal`（再帰構造
  比較）を修正（`Str`分岐が`eq`ではなく新設の`Str::equal`を呼ぶよう変更）
  し、`equalp`を新規追加（`Cons`再帰・`Str`/`Char`は大小無視・数値の
  型跨ぎ比較`Int`⇔`Float`は変換プリミティブが無いため意図的に未対応の
  まま`eql`にフォールバック——スコープを明示的に絞った）。
- `case`マクロ（`prelude.rs`）: `eq`ではなく`equal`を使うよう変更。
  ANSI CLの`case`は`eql`基準（文字列キーはほぼ一致しない）だが、本処理系
  はもともと文字列キーが内容一致してほしいという設計意図があったため、
  意図的に`equal`基準を採用——CL本来の`case`とは異なる、明示的な拡張と
  して文書化した。

**テスト**: `tests/prelude_test.rs`に`eql`セクション新設（bool/i32/char/
`Sexpr`atomの`eql`、`Str`の`eq`/`eql`が同一性のままであることの確認）、
`equal`セクション拡張（`i32`の`equal`、型不一致の`equal`は依然型エラー）、
`equalp`セクション新設（`Str`/`char`の大小無視、`Sexpr`再帰）、`case`が
文字列キーで実際に動くことを確認する新規テスト。`tests/string_test.rs`の
既存`eq`テストを同一性ベースに修正し`equal`/`equalp`テストを追加。
`tests/vector_test.rs`/`tests/compile_test.rs`の`RtValue::Str`関連ヘルパも
`Rc<str>`に追従。全体テスト（`cargo test`、workspace全体）green、
clippy警告0。

## `int->float`/`float->int`変換プリミティブを追加（2026-07-01）

前節の`equalp`再設計で「`Int`⇔`Float`型跨ぎ比較は変換プリミティブが無いため
未対応」として明示的にスコープ外にした残課題を解消。`docs/language-design.md`
§4.1の関数カタログに元々`int->float float->int`として名前だけ予約されていた
（実装は無し）ので、その名前をそのまま採用した。

**実装**: `registry::int_assoc`（`i32`/`i64`共通、幅ごとに別々に呼ばれるため
両方に`int->float: (fn (iN) f64)`が乗る）と`registry::float_assoc`に
`float->int: (fn (f64) i32)`をそれぞれ追加。どちらも他の算術/比較演算子と
同じ「レシーバ型のinstanceメソッド」として登録——`(int->float x)`という
ふつうの関数呼び出し構文のまま、`Checker::try_instance_method`が第一引数の
静的型で自動的にディスパッチする（`+`/`floor`等と同じ仕組み、新規の呼び出し
構文は不要）。実行時本体は`eval::interp`に`int_to_float`/`float_to_int`を
新設（`RtValue::Int(i64) as f64` / `RtValue::Float(f64) as i64`——`i32`/`i64`
は実行時には常に`RtValue::Int(i64)`に統一されているため片方の実装で両幅を
カバーする、`eval_int_builtin`と同じ前提）。`float->int`は0方向丸め
（Rustの`as i64`、CLの`truncate`と同じ向き）。

**`equalp`側の追従**（`prelude.rs`）: `Sexpr`の`Int`/`Float`分岐をそれぞれ
明示的に追加し、相手が逆の数値タグならその内側だけ`int->float`で揃えて`=`、
それ以外（同タグ含む）は既存の`eql`にフォールバックする形に変更——`Cons`/
`Str`/`Char`の既存分岐と同じ「自分の分岐で相手をmatchし直す」形にそろえた。

**テスト**: `tests/prelude_test.rs`に`equalp`のInt⇔Float相互変換テスト
（`(quote 1)`⇔`(quote 1.0)`双方向、値が違えばfalse）、他型とのequalpが
依然falseのままであることの確認、`int->float`/`float->int`単体テスト
（i32/i64双方からの変換、負数の0方向丸め）を追加。全体テスト
（`./scripts/with-llvm-env.sh cargo test`、workspace全体）green。

## `char->int`/`int->char`/`symbol->string`/`string->symbol`を追加（2026-07-01）

`docs/language-design.md` §4.1の変換カタログに名前だけ予約されていた残り4件を実装し、
`int->float`/`float->int`に続いて変換カタログを完成させた。

**`char->int`/`int->char`**（`registry::char_assoc`/`int_assoc`、`eval::interp`）:
`int->float`/`float->int`と同じ「レシーバ型のinstanceメソッド」パターン。`char->int`は
`char`のUnicodeスカラー値を`i32`として返す（常に成功——`char`は既に有効なスカラー値）。
`int->char`は逆方向で、サロゲート範囲や`U+10FFFF`超えなど無効な値では`car`/`cdr`の非`Cons`
panicと同じ前例で実行時panic（型システムでは「有効なスカラー値」を表現できないため）。
`u32::try_from`はedition 2018では暗黙にスコープに無く（`use std::convert::TryFrom`が要る）、
インポートを増やす代わりに範囲チェック+`as u32`キャストで済ませた。

**`symbol->string`/`string->symbol`**（`prelude.rs`、Rustビルトイン不要）: `registry::sexpr_def`
の`sym`ヴァリアントのフィールド型が既に`Type::Str`（プレーンな`string`）だったため、
`symbol->string`は`(Sym name) -> name`という`match`一発で書け、`string->symbol`も裸の`Sym`
コンストラクタ`(Sym s)`をそのまま返すだけで済んだ——コンストラクタ経由のインターン処理
（`Interp`の値構築ロジック、`SEXPR_SYM`分岐で`heap.intern_symbol`を呼ぶ）は`gensym`と同じ
既存の仕組みに乗っているため、新規のRust実装は一切不要だった。

**テスト**: `tests/prelude_test.rs`に`char->int`/`int->char`（往復、サロゲート・範囲外での
panicの両方）、`symbol->string`/`string->symbol`（往復、非symbolでのpanic）を追加。全体テスト
（`./scripts/with-llvm-env.sh cargo test`、workspace全体）green。

## Sexpr/RtValue内部表現統合 実装計画（2026-07-02起案、Stage 0-8全完了 2026-07-04 ※Stage 6の実装記録は「ジェネリック単型化とSexpr/RtValue統合Stage 6」節）

`HashTable<K,V>::entries`のペア型を巡る議論の中で、ユーザーから「`Sexpr`と`RtValue`という
2つの並行した実行時値表現に分かれていること自体がおかしい」という指摘を受けた。実際の
Lisp実装（SBCL等）は、言語仕様が動的型付けか静的型付けかに関わらず、ヒープ上の値を
すべて均一なタグ付きポインタ表現で扱い、単一のGCが一様にトレースする——typelispのように
「S式（quote/macro用データ）専用のSexprと、それ以外全部のRtValue」という2つの値の宇宙に
分かれているのは、その意味で内部表現として不自然だという指摘は妥当と判断した（詳細な
経緯・検討過程はセッション記録参照——「読み書き可能性」という筋の悪い反論を一度行い
訂正した上で、内部表現の均一性という論点で合意）。

**方針**: `defstruct`インスタンス（`Vector<T>`/`cons-cell<K,V>`含む）・クロージャ・
`HashTable<K,V>`・`Scope<V>`——ユーザーが typelisp で作れる値はすべて`Sexpr`
（`crates/typelisp-mem::Value`）の内部表現に統合し、LLVM builder等コンパイラ内部専用の
FFIハンドルだけを`RtValue`に残す。さらに調査で判明した事実として、`HashTable<K,V>`/
`Scope<V>`は現在`RtValue`の専用enumバリアントを持つが、これは2026-06-23の`Vector<T>`/
`defstruct`全面リバート後に確立された「専用バリアントを作らずユーザー定義型と対称に扱う」
という原則（[[typelisp-vector-defstruct-revert]]参照）を、HashTable/Scopeにだけ
適用し忘れていた歴史的な取りこぼしだった。今回の統合はこの非対称性も同時に解消する。

**設計の要点**:
- `Value`に1つだけ新しいヒープ常駐バリアント`Value::Boxed(BoxId)`を追加。`BoxId`は
  `Heap`内の伸長可能なスロットストア（`box_slots: Vec<Option<BoxedObj>>` + フリーリスト +
  markビット配列）への小さいインデックス——既存の`str_slots`（文字列専用の同種の仕組み、
  固定consアリーナとは別に伸長可能でmark-sweepされる）と全く同じパターンの一般化。
- `BoxedObj`はStage毎に増えていく想定: `Float(f64)`（Stage 0）、
  `Struct{type_name, payload: StructPayload}`（Stage 1、`Vector<T>`/`cons-cell<K,V>`/
  `HashTable<K,V>`/`Scope<V>`すべて同じ箱として扱う——特別扱いしない、という
  ユーザー指摘の反映。`StructPayload::Fields(Vec<Value>)`が固定長/可変長共通、
  `Map(HashMap<MemHashKey,Value>)`がHashTable、`Frames(Vec<HashMap<String,Value>>)`が
  Scope）、`Closure{body_token, env: Vec<Value>}`（bodyは`typelisp-mem`が依存できない
  `Typed`/`Expr`を含むため不透明トークン化、実体は呼び出し元クレート側のサイドテーブルで
  管理）、`Cell(Value)`（let/引数/クロージャ捕捉環境が共有する可変スロットのプリミティブ、
  `Rc<RefCell<RtValue>>`の置き換え）。
- タグ予算: `crates/typelisp-rt`の3bitタグは8種全て使用済みだが、`TAG_FLOAT`は実際には
  `encode`/`decode`が`fatal()`するだけの完全な未実装だったため`TAG_BOXED`として再利用——
  新しいタグビットの追加（4bit化、`Cons`のポインタ下位ビット依存や`compiler.rs`に
  ハードコードされたタグ定数全ての再設計が必要になる）を回避できた。
- GCのmark loopは`Vec<Value>`の反復ワークリストに一般化し、`Value::Boxed`を見たら
  markを立てた上でその`BoxedObj`が内部に保持する`Value`を同じスタックに積む
  （ネイティブ再帰なしという既存制約を維持）。副次的benefit: 今`HashTable`/`Struct`/
  `Closure`はRcベースで循環参照が意図的にリークする設計だが、mark-sweepの対象になることで
  正しく回収されるようになる。
- コンパイラ側（`src/compiler.rs`）は`Expr::Construct`の既存`mutable`フラグ
  （`AdtKind::Struct`かどうか、追加のASTプラミング不要）で3分岐: `is_sexpr` →
  既存の`compile-construct-sexpr`、`mutable`（構造体系） → 新設`compile-construct-boxed-struct`、
  それ以外（`Data`/Option/Result等の値semantics ADT） → 既存の`compile-construct-box`
  （無変更、スコープ外）。現状`HashTable`/`Scope`/`Closure`はJIT/AOTコンパイル対応が
  一切ないため、今回は内部表現の統合のみを行い、新規のコンパイル対応はスコープ外とする。

**ステージ分割**（本ドキュメント前掲「Sexpr表現+Match/Construct/共有Rustライブラリ
実装計画」（2026-06-26起案、Stage 0-8全完了）と同じ粒度）:

- **Stage 0（完了、2026-07-02）**: `Value::Boxed`/`BoxId`/`BoxedObj::Float`の骨組み。
  `crates/typelisp-mem`（`box_slots`、GC mark loopの`Vec<Value>`一般化）、
  `crates/typelisp-rt`（`TAG_FLOAT`→`TAG_BOXED`、`rt_float_new`/`rt_float_value`）、
  `src/compiler.rs`（`compile-float`新設、`compile-construct-sexpr`/`compile-sexpr-field`の
  variant-2対応、`rt_extern_functions`への登録）を実装。`Value::Float(f64)`という
  bareバリアントを削除し、`Sexpr::Float`は`Str`と同じ「ヒープ格納・identityベースの`eq`」
  になった——これに伴い、以前から用意されていた通り`eq`/`eql`が初めて分岐する必要が生じ
  （CLの`eql`は数値を値で比較する）、`sexpr_eql`をRust側に新設し`prelude.rs`の`equal`の
  catch-allを`eq`から`eql`に修正（CL仕様: `equal`は非cons/str atomに対し`eql`委譲）。
  実装中に発見・修正したバグ: `compile-float`が`compile-int`と同じ「素のビット値を返す、
  タグ付けは呼び出し元（`compile-construct-sexpr`）の仕事」という規約を誤解し、
  自身で`rt_float_new`を呼んでいたため二重boxingになりコンパイル済みコードでの
  `(Float ...)`構築が壊れたビット値を生成していた（実機テストで発見）。
  テスト: `tests/eval_test.rs`/`tests/prelude_test.rs`/`tests/compile_test.rs`に
  Float構築・eq/eql/equal/equalp分岐・JIT経由の往復テストを追加。全体テスト
  （31クレート）+ Miri（`mem_test`、`typelisp-rt`）green。
- **Stage 1（完了、2026-07-02）**: `StructPayload::Fields`のmem/rt層プラミング。
  `crates/typelisp-mem`（`BoxedObj::Struct{type_name, payload}`、`StructPayload::Fields(Vec<Value>)`、
  `Heap::alloc_struct/struct_type_name/struct_field_count/struct_field/struct_set_field`、
  `push_boxed_nested`のStruct対応でGC mark loopがフィールド内の`Value`まで辿るように拡張）、
  `crates/typelisp-rt`（`rt_struct_new`/`rt_struct_field_get`/`rt_struct_field_set`、
  型名は引数`args[0]`のタグ付き`Sexpr` `Str`として渡す設計——`rt_str_new`と同じ「呼び出し側が
  文字列を作ってから渡す」規約）を実装。`BoxedObj`は`Struct`が`Vec<Value>`を所有するため
  `Copy`を外し（`Clone`のみ）、`Heap::float_value`等の既存読み出し側を参照経由の借用に修正。
  **意図的にこのStageでは`src/compiler.rs`/`src/eval/interp.rs`のどちらにも配線しない**——
  `RtValue::Struct`/`StructData`は無変更のまま並存させ、新しいmem/rt層の表現だけを単独で
  テスト可能な状態にする（Stage 2でインタプリタを、Stage 3でコンパイラを繋ぐ）。
  テスト: `crates/typelisp-mem`の`tests/mem_test.rs`にフィールド読み書き・GCが構造体の
  フィールド経由でconsを辿る（トレース）こと・非構造体`BoxId`へのアクセスがpanicすることを
  検証するテストを追加、`crates/typelisp-rt`の`src/lib.rs`内テストモジュールに
  `rt_struct_new`/`rt_struct_field_get`/`rt_struct_field_set`のABI往復・GCルート保護の
  テストを追加。全体テスト（31クレート）+ Miri（`mem_test`、`typelisp-rt`）green。
- **Stage 2（完了、2026-07-02）**: Struct: インタプリタ結線。`RtValue::Struct`/`StructData`を
  完全に削除し、`defstruct`/`Vector<T>`/`cons-cell<K,V>`インスタンスすべてを
  `RtValue::Sexpr`が包む`Value::Boxed`/`BoxedObj::Struct`に統合した（`Vector<T>`/
  `cons-cell<K,V>`は当初の見立て通り、専用の書き換えなしで自動的に新表現に乗った——
  どちらも`prelude.rs`側は普通の`defstruct`として書かれており、Rust側の特別扱いは
  `eval_builtin_method`の`"vector"`アーム、および`hashtable_keys`/`values`/`entries`が
  `cons-cell`インスタンスを直接組み立てる箇所だけだった）。
  変更点: `Expr::Construct`の`mutable`分岐が`heap.alloc_struct`を呼ぶように変更、
  `Expr::FieldGet`/`FieldSet`が`heap.struct_field`/`struct_set_field`経由に変更、
  `eval_builtin_method`を`heap: &Heap`から`heap: &mut Heap`に変更（`vector`/`hashtable`の
  各アームがstruct構築・変異のため）、`match`のstructパターン（`Pattern::Ctor`）と
  REPLの`format_sexpr`（`main.rs`）を新表現に対応。`RtValue`とmem層の`Value`の境界を
  越える変換ヘルパーとして`rtvalue_to_struct_field`（encode、フィールドの実行時shapeから
  一意に決まるため型情報不要）/`decode_struct_field`（decode、非struct`Boxed`は常に
  `Float`とみなうヒューリスティック——`Sexpr`型フィールドがたまたま浮動小数点リテラルを
  保持するケースとの理論上の曖昧性は残るが、現在のテスト/組み込みのどこからも
  到達しない未使用経路であり、`Registry`アクセス不要な設計を優先して許容した）を新設。
  `crates/typelisp-mem::Heap`に`struct_push_field`（`Vector<T>::push`用、フィールド数を
  事後的に伸長する唯一の操作）と`is_struct`（`Boxed`が`Struct`か`Float`かの判別、
  `float_value`の"wrong kind"パニックを避けるため）を追加。`RtValue::Data`（`Option`/
  `Result`/ユーザーsum型）・`Closure`・`HashTable`・`Scope`型のフィールドは
  `crate::mem::Value`で表現できないため、`rtvalue_to_struct_field`は明示的に
  `EvalError::Internal`を返す（後続StageのClosure/HashTable統合が閉じるべきギャップで、
  Stage 2のスコープ外——現状のテスト/組み込みはスカラー・文字列・`Sexpr`・ネストした
  構造体フィールドしか使っていないため到達しない）。
  テスト: `tests/mem_test.rs`に`struct_push_field`/`is_struct`のテストを追加、
  `tests/struct_test.rs`の直接`RtValue::Struct`をパターンマッチしていた3テストを
  `Heap`の構造体アクセサ経由に書き換え。全体テスト（cargo test、31クレート＋LLVM経由の
  compile系）+ Miri（`mem_test`/`read_test`/`typelisp-rt`）green。
- **Stage 3（完了、2026-07-02）**: Struct: コンパイラ結線。`ast_bridge::translate_construct`が
  `Expr::Construct`の`mutable`フィールドを`construct`タグに追加フィールド（`mutable`/
  `type-id`/`type-name-str`の4フィールド化、`type-name-str`は型名を`str_literal_form`で
  `(str (int c)...)`literal化——AOT先の`Heap`とは`StrId`テーブルを共有しないため
  compile時にpre-alloc不可、Stage 7の`Expr::Str`と同じ制約）として伝搬し、
  `compiler.rs`の`compile-construct`を3分岐化（`is-sexpr`→既存`compile-construct-sexpr`、
  `mutable`→新設`compile-construct-boxed-struct`、それ以外→既存`compile-construct-box`
  無変更）。フィールド値のエンコードは`ast_bridge::struct_field_kind`（`Sexpr`のvariant番号
  1=int/2=float/3=char/4=bool/6=str・Sexpr素通し/0=未対応を再利用、`binding_kind`とは別軸の
  分類——`compile-construct-box-fields`のGCルート要否ではなく`rt_struct_new`に渡す前に
  必要なタグ変換の種類を表す）でタグ付けした`(kind . form)`リストを
  `struct_field_ast_list_to_sexpr`（`tagged_ast_list_to_sexpr`を`kind_fn`パラメータ化した
  共通実装`tagged_ast_list_to_sexpr_with`経由）で構築し、新設`compile-tag-struct-field`
  （`compile-sexpr-field`のデコードと対称なエンコード、`compile-construct-sexpr`の
  各variant分岐のビット操作を値ベースで再実装）が実際のタグ付けを行う。
  構築した`BoxedObj::Struct`は`push-permanent-sexpr-root`で即座に永続ルート化——
  `binding_kind`はregistry-freeなので構造体型をKIND_SEXPRに分類できず、スコープベースの
  GCルート追跡を素通りする代わりに、`compile-construct-box-fields`の`Sexpr`型フィールド
  既存の「意図的リーク」パターンを構造体自身に適用した設計判断。
  `compile-field-get`/`compile-field-set`は旧`malloc`box前提の`+2`オフセット読み書き
  （`build-int-to-ptr`+`load-raw`/`store-arg`）を全廃し、`rt_struct_field_get`/`_set`
  経由に書き換え（`defstruct`のFieldGet/FieldSetは常にmutable、つまり常にboxed-struct
  なので分岐不要）——タグ付き`Sexpr`⇔フィールド自身の表現の往復に`compile-sexpr-field`
  （デコード、既存関数を`kind`引数に流用）/`compile-tag-struct-field`（エンコード）を使う。
  `idx-unary-list`の長さを`i64`定数として使う箇所向けに`sexpr-list-length-i64`を新設
  （`sexpr-list-length`は`i32`返り、`i32`→`i64`への暗黙変換がこの言語に存在しないため）。
  `compile-trait-call`のレシーバ型ID読み出し（旧: レシーバを常に`malloc`'d boxのポインタと
  仮定して`load-raw`でslot 0を読む）は新設`compile-recv-type-id`で表現非依存化——
  レシーバの下位3bitタグで`TAG_BOXED`（boxed-struct）か生ポインタ（malloc'd box）かを
  実行時分岐し、前者は新設Rust側`rt_struct_type_id_hash`（構造体の`type_name`文字列に
  `ast_bridge::type_id_hash`と同じFNV-1aを実行時計算）、後者は従来通りslot 0読み出し。
  現状すべての`compile`済みtrait-callレシーバは`defstruct`（boxed-struct表現）のみだが、
  一般ADT側の経路も表現として残すことで将来の回帰を防ぐ設計。
  `src/eval/interp.rs`に`Interp::struct_types: HashSet<Path>`を追加（`TopLevel::Defstruct`
  実行時に型名を記録、`vector`型のみ`registry::vector_def`がRust側で直接`AdtKind::Struct`
  登録するため`TopLevel::Defstruct`を経由せず`Interp::new`で個別シード——`hashtable`/`scope`は
  `AdtKind::Sum`のままなので対象外）し、`call_compiled`の返り値デコード判定
  （`is_boxed_sexpr_type`、旧`type_is_sexpr`を置換）に使用。引数エンコード側は逆に
  静的型ではなく`RtValue`自身の実行時shape（`Sexpr`か`Int`か）で判定するよう設計変更——
  ジェネリック関数（`(defun (describe T) ((it T)) ...)`）の型変数`T`は`struct_types`に
  照会できる具体的な`Path`を持たないため、静的型ベースの判定では
  `compile_dispatches_a_trait_call_with_*_impl_to_native_code`のような「boxed-struct受け手を
  ジェネリック経由で呼ぶ」ケースが解決不能——`RtValue`の実行時variant自体は型付けが正しい
  プログラムなら常に一意に決まるため、静的型を経由せず直接判定する方が単純かつ正しい。
  既存の`compile_dispatches_a_function_that_constructs_a_defstruct_instance_to_native_code`
  テストは表現変更（生ポインタ→タグ付き`Value::Boxed`）に伴い、フィールド値を直接メモリ越しに
  検証する方式から`RtValue::Sexpr(Value::Boxed(_))`という形だけを検証する方式に書き換え
  （フィールド値自体の正しさはfield-accessorテストが別途検証）。GC-rootストレステスト6件
  （`1 << 13`容量、コンパイラ本体ロード直後のbaseline live_countが約6300→約8500に増加した
  ため`10500`に拡大）も容量調整。ネストした構造体フィールド・`Fn`型フィールドは
  `struct_field_kind`が`0`（未対応）を返しコンパイル時に明示的panicする既知のギャップ
  （当時の記録は「`ast_bridge`がregistry-freeなため`AdtKind::Struct`と一般ADTの
  `Type::Named`を区別できない」を理由としていたが、この理由付けは誤りで、ネスト構造体
  フィールドは同日中に対応済み——下記「『型が分からない』を誤った理由とする未対応箇所の
  一掃」節参照。`Fn`型・一般ADTフィールドの未対応は表現ギャップとして正当、同節参照）。
  テスト: 全体テスト（cargo test、31クレート＋LLVM経由のcompile系）+ Miri
  （`mem_test`/`read_test`/`typelisp-rt`）green。
- **Stage 4（完了、2026-07-03）**: HashTable: `StructPayload::Map`のmem層プラミング。
  `crates/typelisp-mem`に`MemHashKey`（`Int(i64)`/`Bool(bool)`/`Char(char)`/`Str(StrId)`、
  `src/eval/value.rs`の`HashKey`のmem層移植版——`Float`は除外、除外理由も含め既存の`HashKey`の
  doc commentをそのまま踏襲）と`StructPayload::Map(HashMap<MemHashKey, Value>)`を追加し、
  `Heap`に`alloc_hashtable`/`hashtable_get`/`hashtable_set`（`HashMap::insert`と同じ「前の値を
  返す」規約）/`hashtable_remove`/`hashtable_count`/`hashtable_clear`を新設。
  文字列キー特有の問題として、`Value::Str`同士の等価性は`StrId`（アロケーション識別子）基準
  ——`alloc_string`は意図的に内容で重複排除しない（`Sexpr::Str`の`eq`が識別子ベースであるべき
  という2026-07-01の`eq`/`eql`/`equal`/`equalp`再設計の帰結、`docs/cl-equivalence-catalog.md`
  参照）——だが、`HashTable`のキーとしては「内容が同じなら同じキー」（CLの`equal`ベース
  hash table相当）が必要で、素の`StrId`をそのままキーにすると同一内容の文字列リテラルが
  複数回評価されるたび別キー扱いになってしまう（`hashtable_test.rs`の`string_keys_work`が
  実際にこのパターンで書かれている）。解消策として`Heap::intern_string`（内容ベース重複排除、
  一致する内容が既にあれば既存`StrId`を返す、シンボルinternと同じパターン）を新設し、
  `HashTable`の文字列キーだけがこの経路を通る設計にした——`alloc_string`自体は無変更のまま
  （一般の`Sexpr::Str`のidentityベース`eq`セマンティクスに影響なし）。この非対称性
  （読み取り専用の`hashtable_get`/`hashtable_remove`は`lookup_hash_key`で「未internなら
  存在しないキー」と判定するだけで済むため`&self`のみ、書き込みの`hashtable_set`は
  `intern_hash_key`で新規内容なら`intern_string`を呼ぶため`&mut self`が必要）は`Heap`内部の
  非公開ヘルパー2つに閉じ込め、呼び出し側は普通の`Value::Str`を渡すだけでよい。内容重複排除
  された文字列は`push_permanent_root`で永続ルート化した（symbolのinternと同じ「二度と解放
  しない」トレードオフを採用——`str_intern`という別のcontent→StrId逆引きテーブル自体が
  GCの管理外にあるため、対応する文字列スロットがGCで回収されてしまうと逆引きテーブルが
  無効な/再利用されたStrIdを指す状態になり得るバグを防ぐため）。
  GCのmark loop（`Heap::push_boxed_nested`）も`Map`ペイロード対応——値に加えて
  `MemHashKey::Str`が持つキー文字列（`Value::Str`化して同じワークリストに積む）も辿るように
  拡張した（永続ルート化により実際には冗長だが、「生きているmapは自身のキー/値を生かす」
  という不変条件をmark phase単独の視点でも成立させる防御的実装、という位置づけ）。
  既存の`Heap::is_struct`は`BoxedObj::Struct`であること全般を見ていたため、`Map`ペイロードの
  箱も`true`を返すようになってしまう問題があり、`StructPayload::Fields`限定に絞り込んだ上で
  対称な`is_hashtable`（`StructPayload::Map`限定）を新設——両者を明確に分離することで、
  Stage 5でデコード判定に使う際に取り違えを防ぐ。
  コンパイル（LLVM JIT/AOT）対応は計画当初から`HashTable`/`Scope`/`Closure`ともスコープ外
  （コンパイル済みコードから直接呼べる機能を新規追加しない）と明記されているため、
  Stage 1（Struct）が`rt_struct_new`/`rt_struct_field_get`/`_set`を`typelisp-rt`に追加したのとは
  対照的に、今回は`crates/typelisp-rt`側の変更が一切ない——`mem`層のみで完結する唯一のStage。
  **意図的にこのStageでは`src/eval/interp.rs`を配線しない**——`RtValue::HashTable`は無変更の
  まま並存させ、新しい`Heap`APIだけを単独でテスト可能な状態にする（Stage 2がStructでやった
  のと同じ段階分け）。
  テスト: `tests/mem_test.rs`に19件追加——基本CRUD（get/set/remove/count/clear、上書き時に
  前の値を返す）、内容ベースの文字列キー等価性（別々の`alloc_string`呼び出しでも同内容なら
  同じキーとして扱われる/異なる内容は衝突しない）、キー型間の非衝突（`Int(0)`と`Bool(false)`
  が別キーであることを明示的に検証）、`HashTable`以外の箱（Float/Struct）へのアクセサ呼び出し
  がpanicすること、非対応キー型（Cons）がpanicすること、`is_hashtable`/`is_struct`の判別、
  GCが未到達`HashTable`を回収すること・rooted consを介して到達可能な`HashTable`が生き残ること・
  mapの値/文字列キー双方をmark phaseが辿ること（辿らない場合に無関係な文字列アロケーションの
  churnで破損する、という直接的な回帰テスト）。全体テスト（`scripts/with-llvm-env.sh cargo test`、
  31クレート＋LLVM経由のcompile系）+ Miri（`mem_test`）green。
- **Stage 5（完了、2026-07-03）**: HashTable: インタプリタ結線。`src/eval/value.rs`から
  `RtValue::HashTable`と`HashKey`を削除し、`eval_builtin_method`の`"hashtable"`アームを
  `Vector<T>`と同じ「type_name駆動の分岐」パターンで再実装——`new`は
  `RtValue::Sexpr(heap.alloc_hashtable())`、`get`/`set`/`remove`/`count`/`clear`は
  `expect_struct_box`でレシーバの`BoxId`を取り出し`Heap::hashtable_get`/`_set`/`_remove`/
  `_count`/`_clear`を呼ぶだけになった。
  `HashTable`のキー引数はStage 4で`Heap::lookup_hash_key`/`intern_hash_key`が非対応形状
  （例えば`HashTable<f64,_>`のような、traitがないこの言語では型検査で弾けない
  「hashable」違反）に対して素の`panic!`を投げる設計になっていた——このまま呼び出すと
  ユーザーの型選択ミス一つで`EvalError`を経由せずRustパニックが素通しになり、統合前の
  `HashKey::from_rtvalue`が持っていた「`EvalError::Panic`として捕捉可能」という契約を
  壊してしまう。`expect_hashable_key`を新設し、`Heap`に渡す前にインタプリタ側で
  同じ判定を先取りして`EvalError::Panic`に変換することでこの契約を維持した。
  `keys`/`values`/`entries`用に、Stage 4では用意していなかった走査API
  `Heap::hashtable_pairs(id) -> Vec<(Value, Value)>`を新設（`MemHashKey`→`Value`の
  逆変換を内包、`HashKey::from_rtvalue`の逆——旧`hashkey_to_rtvalue`のmem層移植版）。
  返ってくる`(Value, Value)`は`Vector<T>`/`cons-cell<K,V>`の生フィールドとしてそのまま
  使えるため、旧実装が行っていた「`RtValue`にデコード→`rtvalue_to_struct_field`で
  再エンコード」という往復が丸ごと不要になった（`vector_of`を`vector_of_raw`に置き換え、
  `Vec<Value>`を直接受けて`heap.alloc_struct`するだけに簡素化）。
  `decode_struct_field`の「非struct `Boxed`は常にfloat」という2値判定
  （Stage 2で導入、当時は`HashTable`がまだヒープ外にあったため無害だった）は、
  ヒープ常駐の`HashTable`が増えたことで誤判定になる——`Heap::is_hashtable`もチェックする
  3値判定に修正（放置していた場合`(print (HashTable::new))`や`Vector<HashTable<K,V>>`の
  要素読み出しが`heap.float_value`のpanicになっていたはずの潜在バグ）。同じ理由で
  `main.rs`の`format_sexpr`にも`is_hashtable`分岐を追加（REPLでの`HashTable`表示、
  旧`format_value`の`RtValue::HashTable`専用アームの後継）。`get`/`remove`はさらに
  `vector_get`と同じ理由で呼び出し元の検査済み戻り型（`Option<V>`、`hashtable_def`の
  シグネチャ通り）を受け取り、1段`Option`を剥がした上で`V`が`Sexpr`かどうかを
  `decode_struct_field`の形状ヒューリスティックより優先する`is_option_of_sexpr_ty`を新設
  （`HashTable<K,Sexpr>`が量子化リテラルをキー/値に持つケースの誤デコードを避ける、
  `Vector<Sexpr>::get`がとっくに対応していたのと同じ理由）。`rtvalue_type_path`
  （`Expr::TraitCall`のレシーバ型解決）にも`is_hashtable`専用アームを追加——それまでは
  ヒープ常駐化した`HashTable`ボックスがcatch-allの`RtValue::Sexpr(_) => "sexpr"`に
  落ちてしまい、実際には型`"hashtable"`であるべきところを`"sexpr"`と誤解決する経路に
  なっていた（今のところ`HashTable<K,V>`自身に`impl`するtraitはない——`Iter`は
  `hashtable-iter<K,V>`側に実装——ため到達しないが、`Sexpr`/`RtValue`統合が
  一貫して守ってきた「型は常に分かるはずなので誤魔化さない」という方針
  （[[feedback-static-types-are-always-known]]）に合わせて先に直した）。
  `collect_sexpr_roots`の`RtValue::HashTable`専用アームは削除——`HashTable`が
  `RtValue::Sexpr`に統一されたことで、他の`Sexpr`ボックス同様プレーンな
  `RtValue::Sexpr(val) => out.push(*val)`だけでルート化され、`Heap::push_boxed_nested`
  （`StructPayload::Map`のkey/valueトレース、Stage 4で実装済み）がマーク段階での
  再帰を担うため個別対応が不要になった。
  **実装中に発覚した想定外の依存**: `src/compiler.rs`（typelisp自身で書かれた
  自己ホスティングコンパイラ本体）が`compile-let`/`compile-let-values`の
  スクラッチアキュムレータ`acc`として`HashTable<string,llvm-value>`を使っており、
  `RtValue::LlvmValue`（inkwellのコンパイラ内部限定ハンドル）を値として格納していた。
  この値は`crate::mem::Value`で表現不可能（`typelisp-mem`クレートはinkwellは疎か
  `RtValue`にも依存できない）なため、`rtvalue_to_struct_field`が
  `EvalError::Internal`を返し、`tests/compile_file_test.rs`の`let`を含む3テスト
  （`compiles_and_runs_a_loop_based_sum_through_a_helper`等）が壊れて発覚した。
  `Scope<V>`（Stage 7-8まで未着手、今も`RtValue::Scope`のRust-native表現のまま）なら
  `RtValue`を無変換で保持できるため、`acc`の型を`HashTable<string,llvm-value>`から
  単一フレームの`Scope<llvm-value>`に切り替えて解決（`new-acc-table`の本体を
  `(HashTable::new)`→`(Scope::new)`に、`bind-let-values`/`compile-let-values`の`acc`引数
  型注釈を追随。`acc`は`get`/`set`しか使わず、push/pop-frameによるネストも一切ないため、
  「常に最新フレームに書く」`Scope::set`と「最新フレームから検索する」`Scope::get`は
  単一フレームの`HashTable`と完全に等価——意味論上のリグレッションなし）。
  テスト: 既存の`tests/hashtable_test.rs`（16件、GC圧テスト`sexpr_values_survive_gc_pressure`
  含む）・`tests/mem_test.rs`（58件）・`tests/dispatch_test.rs`・`tests/doiter_test.rs`が
  無改修のままgreen——Stage 4までに用意されていたテストが実装の正しさをそのまま検証できた
  （新規テスト追加は不要と判断）。全体テスト（`scripts/with-llvm-env.sh cargo test`、
  31クレート＋LLVM経由のcompile_file_test含む）green。Miriは実行不要
  （[[feedback-miri-timing]]、コミット前は通常の`cargo test`で十分という既存合意）。
- **Stage 6a（未着手）**: `BoxedObj::Cell`導入 + `Interp::slot`/`Env`/`sync_roots`を
  `Rc<RefCell<RtValue>>`から`Value::Boxed(Cell)`に置き換え（let/引数/matchバインディング/
  globals全部に影響）。
- **Stage 6b（未着手）**: `RtValue::Closure`自体を`BoxedObj::Closure{body_token, env}`+
  外側クレートの`ClosureBody`サイドテーブルに移行。
- **Stage 7（完了、2026-07-04）**: Scope: `StructPayload::Frame`/`Frames`のmem層プラミング。
  **表現は計画スケッチ（`Frames(Vec<HashMap<String,Value>>)`、フレームをインライン格納）から
  意図的に変更した**——現行`RtValue::Scope`の`clone-frames`はフレームを`Rc`で**参照共有**する
  （クローン後に共有フレームへ書いた束縛は両方のScopeから見える、いわゆるスコープチェーン——
  「親フレームをmutate/復元も複製もせず参照共有するチェーン構造にする。実在の言語処理系は
  みな同種設計」というユーザーの明示的な設計指示（2026-06-29、ネストlabelsのfn-env修正時）が
  根拠）ため、インライン格納ではdeep copyになりこの意味論が壊れる。代わりに**フレーム自体を
  独立した箱**（`StructPayload::Frame(HashMap<String, Value>)`、type_name `"scope-frame"`。
  キーは旧`ScopeFrame`の`HashMap<String, RtValue>`と同じ素の`String`所有——`Map`のバケット
  ストレージと同様ただのRustメモリなのでmarkはトレース不要、interningも不要）とし、Scope本体は
  `StructPayload::Frames(Vec<BoxId>)`（type_name `"scope"`）がフレーム箱を**インデックスで参照**
  する2層表現にした。`clone-frames`は`Vec<BoxId>`のコピー（フレームごとにポインタコピー1回、
  エントリは一切コピーしない——旧`Rc::clone`と同型）、フレーム*スタック*だけがScopeごとに
  独立なので`push-frame`/`pop-frame`は呼んだScopeにしか影響しない。
  `Heap` API: `alloc_scope`（`Scope::new`の「空フレーム1枚push済み」規約どおり、scope箱+
  初期フレーム箱の2箱を確保。box storeは伸長可能なのでGC誘発なし）/`scope_clone_frames`/
  `scope_push_frame`/`scope_pop_frame`（空スタックではno-op——旧実装の`Vec::pop`挙動を踏襲）/
  `scope_get`（最新フレームから外側へ検索、`Option<Value>`）/`scope_set`（常に最新フレームへ
  書く。フレームが1枚もなければ"no frame to write into"でpanic——インタプリタが自層で
  `EvalError`化すべき呼び出し側不変条件、hashtableの非対応キー型panicと同じ扱い）/
  `is_scope`（`Frames`ペイロード判定。`is_struct`/`is_hashtable`/`is_float`/`is_cell`/
  `is_closure`と並ぶ判別述語。フレーム箱には公開述語を設けない——Scopeの内部構成要素であり
  単独の言語値として渡らないため）。GC mark loop（`push_boxed_nested`）は`Frames`→各フレームを
  `Value::Boxed`として積む、`Frame`→束縛値を積む、の2アーム追加——共有フレームは
  「どれか1つのScopeが生きている限り生存」がmark-sweepで自然に成立する。
  Stage 4（HashTable）と同じくコンパイル対応はスコープ外のため`crates/typelisp-rt`は無変更、
  **インタプリタも意図的に未結線**（`RtValue::Scope`と並存のみ、結線はStage 8）。
  テスト: `tests/mem_test.rs`に14件追加——基本動作（初期フレーム、同一フレーム内上書き、
  push/popでのシャドウイングと復元、setは常に最新フレームのみ）、**参照共有の意味論**
  （clone後に共有フレームへ書いた束縛が他方から見える／clone後にpushしたフレームは共有されない
  ——この表現を選んだ理由そのものの固定化テスト）、全フレームpop後のset panic・空スタックpopの
  no-op・HashTable箱へのscopeアクセサpanic・Scope箱へのhashtableアクセサpanic・`is_scope`判別、
  GC（未到達Scopeのフレームごと回収／rooted Scope→フレーム→束縛値の2ホップトレース／
  非rootedクローンだけが死んでも共有フレームとそこへの書き込みは生存）。
  全体テスト（`scripts/with-llvm-env.sh cargo test`、31クレート）green。
- **Stage 8（完了、2026-07-04）**: Scope: インタプリタ結線。
  **設計判断——6a型の静的型駆動表現振り分けを採用**（候補だった6b型サイドテーブル方式との
  比較検討をユーザーに提示し「6a型でいこう」の決定を受けた）。採用理由: (1) LLVMハンドルの
  GCヒープ混入が「規約」ではなく型分類のレベルで構造的に不可能（6aの`Slot`と同じ保証）、
  (2) 失敗様態が決定的（分岐漏れは`EvalError::Internal`即死トラップで最初の実行で確定的に
  失敗する。サイドテーブルのトークン寿命バグはGCタイミング依存のヒーゼンバグになる——
  本リポジトリに苦闘の記録が複数ある種類）、(3) 自己ホスティングコンパイラの
  `Scope<llvm-value>`/`Scope<llvm-function>`（Scopeの主用途）が今日まで動いてきた
  Rust-native経路をそのまま通り続け、退行の暴露面が最小。6bがサイドテーブルを必要とした
  「1つの値の中にHeap/Nativeキャプチャが混在する」というクロージャ固有の事情は、
  要素型が単一の`Scope<V>`には存在しない——単型化により`V`はインスタンス単位で常に具体的で、
  きれいに二分できる。
  **統合の最終状態はここで確定**: `RtValue::Scope`は削除せず**native-repr `V`のScope専用**
  として残す。`Scope<llvm-*>`は中身がFFIハンドルの集合体なので、「LLVM builder等コンパイラ
  内部専用のFFIハンドルだけを`RtValue`に残す」という統合方針の精神にむしろ合致する
  （2026-07-03の「LLVM系5種とそれ以外を厳密に区分」指示とも整合）。
  実装: (1) `Interp::heap_repr_kind`と`Checker::is_heap_repr`の双子に
  `Scope<V>`→`V`で再帰する対のアームを追加——`Scope<Sexpr>`等の束縛スロットは
  `Slot::Heap`（cell_registry経由でGCが直接トレース）、`Scope<i32>`/`Scope<llvm-*>`は
  従来どおり`Slot::Native`。(2) `eval_builtin_method`に`interp: &Interp`（`struct_types`
  照会用）と`recv_ty: Option<&Type>`（レシーバ引数`args[0]`のchecked型——`Expr::Assoc`は
  型引数を運ばないため、`Scope<V>`の`V`が静的に手に入る唯一の経路。`Expr::Assoc`/
  `Expr::Apply`(BuiltinMethod)の両呼び出しサイトから配線）を追加。(3) `"scope"`アームを
  二経路化: `new`は自ノードの戻り型`Scope<V>`から、インスタンスメソッド5種はレシーバの
  静的型から、新設`Interp::scope_is_heap`で振り分け（**値形状は一切見ない**。`push-frame`/
  `pop-frame`/`set`は戻り型がUnitなのでret_tyからはVが復元できず、recv_ty配線が必須だった）。
  (4) ヒープ側ヘルパー`scope_*_heap` 5種を新設——`V`がheap-reprなら要素のランタイム表現は
  構造的に`RtValue::Sexpr`のみなので、`get`の出口デコードは無条件の`RtValue::Sexpr`包み
  （`hashtable_get`のような型付きデコード不要）、`set`の入口の非`Sexpr`値は不変条件即死。
  `set`はmem層panic（"no frame to write into"）をinterp側の`Heap::scope_frame_count`
  事前チェック（mem層に新設）で捕捉可能な`EvalError::Internal`に変換——`pop-frame`は
  typelispから叩けるためユーザー到達可能な条件であり、`expect_hashable_key`と同じ
  「RustパニックにしないでEvalErrorへ」の契約。(5) `main.rs`の`format_sexpr`に
  `is_scope`→`#<scope depth=N>`アーム追加（native側`format_value`と同一表示）。
  (6) `collect_sexpr_roots`は無変更（native Scopeの既存アームは残存必須、ヒープScopeは
  `RtValue::Sexpr`アームが既にカバー）。`decode_nonsexpr_field`もコード無変更
  （非float Boxedのcatch-allが`RtValue::Sexpr`を返す規約にヒープScopeも自然に乗る、
  docコメントのみ更新）。`rtvalue_to_struct_field`はheap-repr Scopeが`RtValue::Sexpr`として
  自動的にstructフィールド格納可能になり、native Scopeのみ従来どおり`Internal`エラー。
  テスト: `tests/scope_test.rs`に heap経路10件追加（`Scope<Sexpr>`でnative側9件をミラー、
  +全フレームpop後のsetが捕捉可能なエラーであること、`Scope<Vector<i32>>`の要素が参照
  semantics（getで返る箱=setで入れた箱、push が双方から見える）を保つこと、
  `hashtable_test.rs`と同型の極小ヒープGC圧テスト=束縛セル→Scope箱→Frames→Frame→値の
  マークだけで生存する実証）。`tests/mem_test.rs`に`scope_frame_count`。
  既存native側9件（`Scope<i32>`）は無改修で両表現の意味論等価を担保。
  全体テスト（`scripts/with-llvm-env.sh cargo test`、32バイナリ）green——必須ゲートの
  `tests/compile_test.rs`/`tests/compile_file_test.rs`（自己ホスティングコンパイラ、
  `Scope<llvm-*>`のnative経路）含む。clippyはHEAD比で新規警告ゼロ。
  **追補（同日、ユーザー提案）**: native側ペイロードを裸の
  `Rc<RefCell<Vec<ScopeFrame>>>`から`NativeScope`構造体（`src/eval/value.rs`）に
  カプセル化。`new`/`clone_frames`/`push_frame`/`pop_frame`/`get`/`set`/`depth`/
  `for_each_value`をメソッド化し、フレーム共有チェーンの不変条件を型の内側に閉じた——
  ヒープ側が`Heap::scope_*`メソッド群なのにnative側だけフリー関数が生の入れ子型を
  剥がして触るという非対称の解消。interp.rsのscope_*フリー関数は`args`スライスとの
  薄いアダプタに縮退、`collect_sexpr_roots`は`for_each_value`経由、`main.rs`の表示は
  `depth()`経由。振る舞い変更なし（`PartialEq`の内容比較・空スタックsetのエラー含め
  従来どおり）。なお同時に検討した「空スタックsetでフレーム自動再作成」案は不採用
  ——空スタックはpush/pop不均衡バグでしか到達せず、自動修復は関数エントリフレーム
  破壊を隠蔽して失敗を遠方に移動させるため（fail-fastの原則維持）。

各段階で`RtValue`からバリアントが1つずつ消えていき（`Struct`/`StructData`=Stage 2、
`HashTable`/`HashKey`=Stage 5、`Closure`=Stage 6b）、Stage 8完了で確定した最終状態では
`RtValue`に`Int/Float/Bool/Char/Str/Unit/Data/Sexpr(Value)/Builtin/BuiltinMethod`と
LLVM系5種、および**native-repr `V`専用となった`Scope`**が残る（heap-repr `V`のScopeは
`Sexpr`側へ移行済み。`Scope<llvm-*>`はFFIハンドルの集合体なので「コンパイラ内部専用の
FFIハンドルだけを`RtValue`に残す」という本計画の到達点に含まれる——Stage 8の項参照。
名前のリネームはスコープ外のまま）。詳細な実装計画は
`/Users/suzukijun/.claude/plans/sexpr-lisp-lisp-s-s-lisp-lisp-sexpr-rtv-eager-garden.md`
（Plan mode成果物）参照。

## 「型が分からない」を誤った理由とする未対応箇所の一掃（2026-07-02）

Stage 3完了報告の際に「ネストした構造体フィールドは`ast_bridge`がregistry-freeな設計のため
型が分からず未対応」と説明したところ、ユーザーから「typelispは静的型付けの言語なのに、
扱っているアイテムの型が分からない状況があるのはおかしい」という指摘を受けた。調査の結果、
指摘は正しい: checkerは検査完了時点で全式・全フィールドの型を完全に把握しており
（`Checker::registry()`、`AdtDef::kind`）、欠けていたのは型情報そのものではなく
**分類情報（`AdtKind`）を`ast_bridge`/interpの利用地点まで運ぶ経路**だけだった。
`Expr::Construct::mutable`が既にやっている「check時に解決してASTに焼き込む/解決済みデータを
手渡す」方式を各所に拡張し、「型が分からない」を理由にしていた未対応・不正確箇所を全て解消した。

1. **ネスト構造体フィールドのcompile対応**（`src/compile/ast_bridge.rs`）:
   `ast_to_sexpr`のシグネチャに`structs: &HashSet<Path>`（`AdtKind::Struct`と解決済みの
   型Path集合——唯一の実呼び出し元`Interp::add_compiled_function`が`Interp::struct_types`を
   渡す）を追加し、翻訳再帰全体（`ast_to_sexpr_scoped`/`translate_*`、`direct`/
   `outer_captured`と同じ経路）にスレッド。`struct_field_kind`は`Type::Named(p, _)`が
   `structs`に含まれればkind `6`（パススルー）に分類——ネスト構造体フィールドの値は
   それ自体がタグ付きboxed-struct `Sexpr`なので、`Str`/`Sexpr`フィールドと同じく
   エンコード（`compile-tag-struct-field`）・デコード（`compile-sexpr-field`）とも
   素通しで正しい。**`compiler.rs`側はkind 6が最初からパススルーなのでコード変更ゼロ**
   （コメントのみ更新）。construct引数・field-get・field-setの3経路全てで、
   ネストした`defstruct`/`Vector<T>`/`cons-cell<K,V>`型フィールドを含む関数が
   compile可能になった。`tagged_ast_list_to_sexpr_with`の`kind_fn`は
   `fn(&Type) -> i64`→`impl Fn(&Type) -> i64`（`structs`をキャプチャする
   クロージャを渡すため）。
   - kind `0`（明示的panic）に残るのは正当な表現ギャップのみ:
     `Type::Fn`（ClosureBoxとGC構造体フィールドの統合はStage 6a-6bの領分）、
     一般ADT（生`malloc`ポインタはGC走査対象の構造体フィールドに置けない——
     ADT自体のGC-box化までは本質的に不可能）、ジェネリック型変数
     （ジェネリック定義を1回だけcompileする現方式ではmonomorphizationなしに
     決まらない——これは型消去/パラメトリシティの問題であって
     「registryがないから」ではない）。`struct_field_kind`のdocを正確な理由に全面書き換え。
2. **interpのSexpr宣言フィールドdecode誤りを修正**（`src/eval/interp.rs`）:
   `decode_struct_field`のshapeヒューリスティックは、`Sexpr`宣言フィールドに格納された
   quoted `42`/`3.14`/`"s"`を`RtValue::Int`/`Float`/`Str`に誤変換していた（静的型と矛盾。
   旧docは「静的型駆動のdecodeには`Interp`が持たない`Registry`アクセスが要る」として
   Float曖昧性を既知の不正確さとして放置していたが、実際にはフィールドの静的型は
   利用地点に既にあった）。3経路をガード:
   - `Expr::FieldGet`のeval arm: FieldGetノード自身の`t.ty`（=フィールドの宣言型）が
     `Sexpr`なら`RtValue::Sexpr(raw)`を直接返す（新設ヘルパー`is_sexpr_ty`）。
   - `vector_get`: 呼び出しサイトのcheck済み返り値型で判定——`eval_builtin_method`に
     `ret_ty: &Type`を追加し、`Expr::Assoc`/`TraitCall`/`Apply`の3呼び出し元が
     `&t.ty`を渡す（`Vector<Sexpr>`の`get`が対象）。
   - `match`のboxed-structパターン: `Pattern::Ctor`に`sexpr_fields: Vec<bool>`を追加、
     `Checker::check_ctor_pattern`がフィールドのインスタンス化済み型（`subst_apply`後）
     から**check時に焼き込む**——`Expr::Construct::mutable`と同一の方式。
   - shapeヒューリスティックが残るのはジェネリックbody内（アクセサ本体の`ty`が型変数の
     まま型消去評価されるケース）のみで、これは真の型消去限界としてdocに明記
     （monomorphized評価かbox側への型引数記録が要る、という設計次元の課題）。
3. **`binding_kind`が構造体型を`KIND_SEXPR`にしない理由のコメント訂正**（`src/compiler.rs`
   `compile-construct-boxed-struct`のdoc）: 旧「registry-freeで構造体型と一般ADT型を
   区別できないから」→ 正「boxed-structは出生時に`push-permanent-sexpr-root`で
   永続保護済みなので、束縛単位のpush/popは冗長なブックキーピングにしかならないから」。

検証: `scripts/with-llvm-env.sh cargo test`全31バイナリ812件green（回帰テスト7件追加:
`struct_test.rs`にSexpr宣言フィールドのアクセサ/setf/match destructuring 4件、
`vector_test.rs`に`Vector<Sexpr>::get` 1件、`compile_test.rs`にネスト構造体の
compile済みconstruct+accessor/setter 2件）。`compiler.rs`本体はコメント変更のみのため
ヒープ容量調整は不要。

教訓（メモリ`feedback-static-types-are-always-known`に記録）: 静的型付け言語の処理系開発で
「この地点では型が分からない」という説明が出てきたら、それはほぼ常に「checkerが持っている
情報を運ぶ経路を作っていない」の言い換えであり、実装を諦める理由にならない。
`Expr::Construct::mutable`/`TraitCall::impls`/`Pattern::Ctor::sexpr_fields`のように
check時に解決してASTに焼き込むか、`ast_to_sexpr`の`structs`のように解決済みデータを
引数で手渡せばよい。真に静的に決まらないのは、ジェネリック定義を型消去のまま
1回だけ評価/compileする場合の型変数（monomorphizationの問題）と、マクロ展開時
（型検査前なので型が存在しない）だけ。

## ジェネリック単型化とSexpr/RtValue統合Stage 6（2026-07-03、6コミット）

Stage 6a（束縛スロットの`BoxedObj::Cell`化）の設計中に2つのユーザー指示を受け、
実装が「単型化 → 6a → 6b」の3段に再構成された:

1. **「LLVM系5種とそれ以外のデータを一緒に扱うのが間違い。必要な場所と不要な場所を
   厳密に区分して」**——LLVM値がスロット/クロージャに流れるのは`src/compiler.rs`の
   SOURCE解釈時のみ（.typl/prelude/defvarには一切ない）と調査で確定。LLVM値はSexprを
   内包せず、GC宇宙とLLVM宇宙は互いに参照しない分離した世界。
2. **「型消去実行（1共有ASTボディを全インスタンス化で使い回す）をやめ、特殊化された
   時点で型変数にあわせた関数を生成して使う。実行時の値形状フォールバックはすべて
   無くす」**——型消去下では束縛スロットの振り分けに実行時の値形状判定が9箇所
   （preludeのunwrap系match束縛・identity等のパラメータ・vector-iter::nextのlet束縛）で
   必要だったが、単型化後は静的型駆動に一本化できる。

### ジェネリック単型化（M1-M4）

**方式は再check**: 生Sexprテンプレート（`FnTemplate`/`MethodTemplate`、恒久ルート化）を
保持し、インスタンス化ごとに`type_var_bindings`（`canon`の単一フックで置換、型パーサ
無変更）の下で再checkする。check済みTyped ASTへの型置換は`Assoc`解決/`TraitCall`→`Assoc`
昇格/`Construct::mutable`/`sexpr_fields`の再導出が必要で実質check再実行になるため不採用。

- **M1（defun）**: `check_call`（`&self`）はメモ照会+`spec_pending`へのリクエスト積みのみ、
  特殊化生成は`check_form`（`&mut self`）が主check成功後にワークリストでドレインし
  `TopLevel::Module`（`MONO_BUNDLE_MODULE`、空白入りパスでリーダから構造的に到達不能）に
  [特殊化群..., 主フォーム]をバンドル——既存のcheck→execパイプラインは無変更で透過。
  マングル名は空白入り（`"identity <i32>"`）で衝突が構造的に不可能、レジストリ非登録で
  RedefPolicy/可視性と干渉しない。`spec_memo`はフォーム単位でクリア（バンドル自己完結、
  REPLがバッチを破棄しても宙に浮いた参照が残らない）。`Interp::exec`はtype_params非空の
  Defun/Defmethod登録をスキップ（消去済みボディの実行を構造的に排除）。多相再帰は
  budget 512の発散ガードでTypeError化。副産物: compileされたTraitCallディスパッチが
  ソース到達不能化（compile_testの該当2テストは新セマンティクスの検証に書き換え）。
- **M2（defmethod/impl/accessor）**: 所有型経由のジェネリックメソッドを特殊化。
  written_vars（レシーバに書かれた型変数名を位置zip）で束縛。defstructアクセサは
  生フォームが無いため`Getter`/`Setter`テンプレートからAdtDefのフィールド型に
  subst_applyして再合成。
- **M3（FnRef/MethodRef）**: 関数値化はexpected関数型からunifyで型引数を解決して
  特殊化参照へ。文脈が無い場合はcheck時TypeError（M1以降どのみち実行不能だったものが
  早く明確に）。`(compile generic)`もcheck時に理由付きで拒否。
- **M4（フォールバック全廃）**: `decode_struct_field`の形状ヒューリスティック→型駆動
  `decode_field_typed`（+非Sexpr側`decode_nonsexpr_field`）。Sexpr宣言スロットの
  クオート済みスカラ誤デコード（旧docコメントが「単型化評価が必要」と明記していた
  既知欠陥）が解消。TraitCall evalアームをInternal化、`rtvalue_type_path`
  （最後の値形状ディスパッチ）を削除。

### Stage 6a: `BoxedObj::Cell` + 静的型駆動二層Slot

`Slot::Heap(Rc<BoxId>)`＝ランタイム表現が常に`RtValue::Sexpr`の型（Sexpr・boxed struct・
hashtable）のみ／`Slot::Native(Rc<RefCell<RtValue>>)`＝それ以外。振り分けは束縛点の
静的型のみ（Let=束縛値のTyped.ty、Defvar=宣言ty、パラメータ=FnDef登録時kinds、
lambda/labels=ASTのType、matchは`Pattern::Bind`へcheck時焼き込み）。スカラ/Str/Floatは
decode曖昧性とeq同一性（`Rc::ptr_eq`）のためCell化しない。

**セル生存管理はHeap自身が持つ**（`cell_registry: Vec<Weak<BoxId>>`、`alloc_cell`が
`Rc<BoxId>`ハンドルを返し、gc()が生存セルを暗黙ルートとして自らmark）。当初は
interp側weakリスト+sync_roots拡張で実装したが、**compiledコード内で発生するGCは
interpのsync_roots規約の外**であり、`call_compiled`前の強制syncはルートスタック会計
テストと干渉した——Heap内レジストリなら「どこから起きたGCも生存セルを見る」が
構造的に成立する。副産物として「最終sync以降に作られたSexpr束縛がcompiledコードの
GCに晒される」pre-existingの穴もSexpr系束縛については閉じた。挙動改善: 束縛が
死んだ時点で値が回収可能になる（旧実装は評価終了後もsync_rootsの残留ルートが値を
生かし続けた——`runtime_cons_cells_survive_gc_when_rooted`の最終断言をlive_count 0に更新）。

### Stage 6b: `BoxedObj::Closure` + サイドテーブル、`RtValue::Closure`削除

クロージャは`Value::Boxed`のクロージャボックス（`BoxedObj::Closure{body_token,
env=ヒープセルキャプチャのみ}`）を包む`RtValue::Sexpr`になった。ボディ（Typed AST、
memクレートは依存不可）とNativeキャプチャ（スカラ・LLVMハンドル等）は`Interp`の
サイドテーブル（`closure_bodies: HashMap<u32, Rc<ClosureBody>>`）へ——**LLVM系が
GCヒープに入らない区分はクロージャ内部でも`Capture::Heap(idx)/Native(Rc)`として維持**。
sweepが未到達クロージャのtokenを蓄積（gc()はcons()内から暗黙に走るため戻り値方式は
不可）、sync_roots冒頭でドレイン→`ClosureBody`のdropでNativeキャプチャのRcも連鎖解放、
リークなし（`closure_body_count`で実証）。

- labelsのプレースホルダをヒープセル化: cell→closure box→sibling cell→…の相互再帰の
  結び目が純粋なヒープ循環になりmark-sweepが丸ごと回収——旧Rc表現が設計上リーク
  していた循環の実質修正。
- 新規GCハザード: calleeがGC管理値になったため`Expr::Apply`は引数評価前に
  `_f_anchor`で保護（無名callee`((f x) y)`形、極小ヒープテストで実証）。
  キャプチャセルは`Heap::adopt_cell`で再所有し、box自体が呼び出し中に死んでも
  フレーム束縛が独立に生存。
- 周辺: 「struct/hashtable以外のBoxed=Float」前提を`is_float`陽性判定に反転、
  `call_compiled`にインタプリタクロージャの越境ガード（compiledの`ClosureBox`と
  ABI非互換）、クロージャがstructフィールドに格納可能になった（`rtvalue_to_struct_field`は
  Sexprとして受ける）。

`RtValue`の残バリアント: `Int/Float/Bool/Char/Str/Unit/Data/Sexpr/Builtin/BuiltinMethod/
Scope` + LLVM系5種。テスト: monomorph_test 27本新設、mem/evalにGC実証テスト群
（セル・クロージャ・循環回収・サイドテーブル同期）。全32ターゲット+miri green。

## `defvar`/`defconstant`の型注釈を必須化（2026-07-03）

`(defvar name value)`（初期化子からの型推論）を削除し、`(defvar (name Type) value)`のみ許可
（ユーザー指示「defvarで型がない変数を定義できるのがおかしい」）。グローバルの型はプログラムの
公開サーフェスなので推論しない、という位置付け。`check_defvar`の`Value::Symbol`分岐を
理由付きTypeErrorに変更。副次的な整合: 単型化（M3）後、型注釈が初期化子のexpected型として
効くため、`(defvar (f (fn (i32) i32)) identity)`のようなジェネリック関数値のdefvar束縛が
（型なし時代は原理的に不可能だったのに対し）正しく特殊化されて動く——monomorph_testに
正のテストを追加し、「文脈なしのジェネリック値化はエラー」のテストはlet束縛経由に変更。
既存テスト17箇所を型付き形式へ更新、language-design.mdの仕様記述を更新。

## Language Server (`typl-lsp`) 追加 (2026-07-11)

`src/bin/lsp.rs`に診断（diagnostics）専用のLSPサーバーを追加。`lsp-server`/
`lsp-types`（stdio上のJSON-RPC、非同期ランタイム不要）を新規依存として追加し、
既存の`typl`バイナリと同じ`Reader`→`Checker`パイプラインを`didOpen`/`didChange`/
`didSave`ごとに文書全体で再実行、read/type errorとcheckerのwarningsを
`textDocument/publishDiagnostics`として送出する。

- 意図的にコード実行はしない: 診断はキー入力のたびに走るため、任意のユーザーコードを
  評価するのは危険（副作用・panic・無限ループ）。唯一の例外は`defmacro`
  （後続フォームのマクロ展開に必要、`Interp`への登録は純粋なHashMap insertで副作用なし）
  ——`main.rs`の`needs_immediate_exec`をそのまま複製。
- `Loc`（1-based line/col、単一文字位置）をLSPの0-based `Position`へ変換。checker
  warningsは位置情報を持たないため文書先頭に仮置き。
- `Connection`を`run()`に値渡しして関数末尾でdropしてから`io_threads.join()`する
  必要がある（`sender`が生きたままjoinすると書き込みスレッドが終了せずデッドロック）。
- 当初の未実装（次の一手）: 複数ファイル/`module`・`use`をまたぐ解決なし（開いている1
  ファイル単独でチェック）、hover/補完/goto-definitionなし、読み取りエラーは
  最初の1件で停止（S式リーダーの性質上、不整合な括弧を越えて再同期するのは困難、これは
  今も変わらない）。マルチファイル解決は直後の「ファイル↔モジュール対応」以降の節、
  hover/goto-definitionは「LSP: hover/goto-definition基盤実装」節（同日）で対応——
  補完は本節時点も含めて引き続き未着手。

## ファイル↔モジュール対応（マルチファイル対応）実装 (2026-07-11)

複数`.typl`ファイルでプログラムを構成する仕組みを新設（設計判断はユーザーとの対話で確定、
仕様の詳細は[language-design.md](language-design.md) §2.3）。核となる規約は
**「ファイルパス＝モジュールPath」**——ソースルート相対パスがそのままセグメント列になり
（`geo/point.typl`→`geo::point`）、ファイル内容は暗黙にそのモジュールへ包まれ、明示
`(module ...)`はその内側にネストする。ルートは`typelisp.toml`マニフェスト（上方探索、
任意キー`src = "dir"`のみ手書きパース）、無ければエントリのディレクトリ。

- **`src/project.rs`新設**: `find_src_root`/`module_segs_for`/`Loader`。`use`依存は
  チェック**前に**フォームを走査してオンデマンドで再帰ロード（走査先行なので同一フォームの
  再チェックが発生せず、偽の再定義警告が出ない）。循環は`loading`スタックで検出し
  `circular module dependency: a -> b -> a`の連鎖付きエラー。`needs_immediate_exec`は
  main.rs/lsp.rsの重複コピーをここへ集約。
- **`Error::ModuleNotLoaded(Vec<String>)`**（typelisp-mem）: `check_use`の3解決全滅時の
  構造化エラー。ローダ無しドライバでは従来と同じ「use: unresolved」表示。
- **`Checker::enter_module`/`exit_module`**: `check_module`のns push/pop部を公開APIに切り出し、
  ドライバが`(module ...)`のValueを合成せずにファイルを暗黙モジュール内でチェックできる。
- **GCルート規律**: 各ファイルのreadルートはそのファイルのチェック完了時にpop（ネストは
  LIFO）。チェック済み`TopLevel`はキューに積み、全ルートpop後に依存順で一括exec
  （`Defvar`初期化子等のヒープを触るexecを`sync_roots`不変条件と両立させる）。
  `defmacro`のみ即時exec（rootスタック非接触の純登録）。
- **`Reader::read_all_in_keep_locs`新設**: 依存ファイルの読み込みが外側ファイルの
  cons位置テーブルを`clear_cons_locs`で消してエラー位置が失われるバグをE2Eで発見、
  「クリアはセッション開始時に一度だけ」に修正。
- **`typl <file.typl>`実行モード**（main.rs、従来はREPL専用）とREPLの`use`結線、
  **LSPのクロスファイル診断**（lsp.rs、依存側エラーはfile:line:col付き全文で文書先頭に
  アンカー、開いている文書内のエラーは正確な位置）。
- テスト: `tests/module_file_test.rs`（9件——基本/ネストdir/module入れ子/アイテムuse/
  循環/未解決/defvar遅延exec/srcキー/無マニフェスト）。全36ターゲットgreen。

## LSPの依存ファイルにエディタバッファのオーバーレイを適用 (2026-07-11)

`typl-lsp`が依存ファイル([[typelisp-file-module-mapping]]の`use`ロード)を常にディスクから
読んでいたため、開いている依存ファイルの未保存編集が診断に反映されない問題を修正。
`project::Loader`に`overlay: HashMap<PathBuf, String>`フィールドと`set_overlay`を追加し、
`read_source`をメソッド化してoverlay優先→ディスクフォールバックの順にした。`lsp.rs`の
`publish`は自分以外の開いている全文書をoverlayとして`diagnostics_for`へ渡す（エントリ
文書自身は従来どおり直接テキストを渡すので対象外）。E2Eで「依存ファイルを保存前に編集
→即座に依存元の型エラーが解消する」ことを確認。

残課題（`docs/dev/TODO.md`）: 依存ファイルを編集しても依存元の診断は自動再発行されない
（依存元自身に変更イベントが来るまで反映されない、逆依存グラフ追跡が必要）。

## LSPの逆依存追跡: 依存ファイル編集で依存元の診断を自動更新 (2026-07-11)

overlay対応(直前のログ)だけでは、依存ファイル自身を開いて編集しないと依存元の診断が
更新されなかった（依存元に変更イベントが来るまで再チェックされない）ため、逆依存追跡を追加。

`project::Loader`に`loaded_files: HashSet<PathBuf>`を追加（`ensure_loaded`で依存ファイルの
パスを記録、entry自身は含まない）、公開アクセサ`loaded_files()`を新設。`lsp.rs`の`publish`は
`deps: HashMap<Uri, HashSet<PathBuf>>`（各文書が最後に依存したファイル集合）を維持し、
変更された文書を起点にBFSで「その文書に依存している他の開いている文書」を辿って
`publish_one`（実際の診断計算+deps更新）を再帰的に呼ぶ。visited集合で無限ループを防止。

副次的な修正: `ensure_loaded`のファイル存在チェックが`file.is_file()`（ディスクのみ）
だったため、まだ保存していない新規ファイルがoverlayにしか無い場合に発見できなかった
バグも同じ箇所で修正（overlayも確認するよう変更）。

E2Eで「依存ファイル(point.typl)のみを未保存で編集し、依存元(main.typl)には触れない」
操作を行い、point.typl向けとmain.typl向けの2件のpublishDiagnosticsが自動で届き、
main.typlの型エラーが消えることを確認。全36ターゲットgreen。

## LSP: hover/goto-definition基盤実装 (2026-07-11)

TODO.mdが調査時点で想定していた「`Typed`/`Expr`にspan（開始~終了）を追加する」大改修は
不要と判明。`Typed.loc: Option<Loc>`は既にリスト形式ノードの開き括弧位置
（`Checker::check`が`heap.cons_loc(v)`で埋める、点情報のみ）を持っており、「カーソル位置
以下で最大の開始位置を持つノードを深さ優先で探す」だけで正しい最小包含ノードが求まる
（S式の兄弟フォームはソース順に重ならないため）。この発見によりreader/heap側の変更なしで
実装できた。

- `src/check/registry.rs`: `DefLocs`（`Registry.def_locs`）をサイドテーブルとして新設。
  `FnSig`/`AssocFn`自体にはフィールドを足さない——`with_builtins()`だけで両者のリテラルが
  計130件超あり、ビルトインには参照すべき位置がそもそも無いため、既存構造体への必須
  フィールド追加は無意味に大きな差分になる。チェッカーの7つの登録箇所（`check_defun`/
  `check_defmethod`/`check_defstruct`/`check_defenum`/`check_defvar`/`check_deftrait`/
  `check_defmacro`）でだけ書き込む。
- `src/check/locate.rs`（新規）: `locate_node`（カーソル→最小包含ノード、全木探索）/
  `definition_target`（ノード→`DefLocs`引き、`Global`/`Call`/`FnRef`/`Assoc`/`MethodRef`/
  `Construct`のみ対象——いずれも解決済みの`Path`を既に持つので新たな名前解決は不要）/
  `hover_text`（`Typed.ty`を`{:?}`でフォーマット、`Type`に`Display`実装が無いため既存の
  エラーメッセージと同じ流儀に合わせた）。
- `src/bin/lsp.rs`: `hover_provider`/`definition_provider`を有効化。doc毎に直近成功した
  `Analysis { body: Vec<TopLevel>, def_locs: DefLocs }`をキャッシュし、型エラーがある間は
  直近成功時点のものを保持し続ける（現状の「毎回ゼロから読み直す」設計とは別軸）。

既知の制限（意図的なMVPスコープ）: ローカル変数（`Expr::Var`、`let`/`lambda`束縛）への
goto-definitionは非対応（束縛側にも位置情報を持たせていない）。アトム単体（裸のシンボル・
リテラル）は自身の位置を持てない（`Value::Symbol`はヒープ上で一意な参照ではなく使い回される
ため、`cons_locs`と同じ仕組みでは追跡不可能）——真のspan対応にはreader全体の作り替えが
必要だが、現状のMVPは「S式の兄弟フォームが重ならない」性質だけで実用上十分な精度が出ている
ため見送った。詳細な調査結果と補完（未着手）の設計メモはdocs/dev/TODO.mdの
「hover / goto-definition / 補完」節（折りたたみ内）に残している。

単体テストは`tests/lsp_locate_test.rs`（LSPのstdioトランスポートは介さず`Checker::check_form`
を直接駆動してコアロジックのみ検証）。全体テスト無回帰を確認。

## compile機能: グローバル変数（Global/SetGlobal）対応、JIT/AOT両方 (2026-07-11)

`defvar`/`defconstant`をcompile対象の関数から参照・代入できるようにした。コンパイル済み
（ネイティブ）コードはインタプリタの`Interp.globals: HashMap<Path, Slot>`に一切アクセス
できないため、JIT/AOTどちらもグローバル変数用のストレージを新設する必要があった。

**採用した設計**: コンパイル済みコードから参照されたグローバルだけを、GCの
**permanent root**（`crates/typelisp-mem`の`Heap.permanent_roots`——LIFOでpopされる通常の
`roots`とは別の「積んだら二度と外さない」Vec、既に`rt_push_permanent_sexpr_root`が構造体
フィールドの保護に使っている実績ある機構）に「昇格」させる。一度もコンパイルされない
グローバルは今まで通り`Slot`ベースのまま——RtValue→`mem::Value`変換に使う既存ヘルパー
`rtvalue_to_struct_field`が`RtValue::Data`（`Option`/`Result`/ユーザー`defenum`）を変換
できないため、全グローバルを一律移行すると同型の`defvar`が今日動いているのに壊れる。

- `crates/typelisp-mem/src/heap.rs`: `Heap::permanent_root`/`set_permanent_root`
  （インデックス指定の読み書き。GCのmark走査は`permanent_roots`を毎回インデックスで
  再読みするだけなので、値を上書きしても既存ロジックは無改造で安全）。
- `crates/typelisp-rt/src/lib.rs`: `rt_global_new`/`rt_global_get`/`rt_global_set`。
  「コンパイル時に割り振った小さい連番id」→「実際のpermanent_root位置」を仲介する
  `GLOBAL_INDEX`スレッドローカルを新設（構造体フィールド構築等、無関係なコードも
  `rt_push_permanent_sexpr_root`を呼ぶため、idと生のpermanent_root位置は同一視できない）。
  `reset_global_table()`（`Interp::new()`から呼ぶ——`cargo test`のスレッドプールで別
  `Heap`インスタンスが同じOSスレッドを再利用するケースの対策）。
- **JIT/AOTのid採番の非対称性**: JIT（`Interp::compile_function`経由の`promote_global`）は
  参照時に遅延昇格——コンパイルと実行が同一プロセス・同一`Heap`なので単一タイムラインで
  安全。AOT（`compile::aot::compile_file`）はコンパイル用のHeapと生成される実行ファイル
  自身が起動時に作る別のHeapという2つのタイムラインの数値を一致させる必要があるため、
  全`defvar`をファイル宣言順に**即座に**昇格し（`Interp::promote_global`を`pub(crate)`化）、
  `Interp::add_compiled_global_init`が生成する初期化関数群（`$global_init$N`）を
  `build_main_wrapper`が`rt_heap_init`と`tl_main`の間に同じ順序で挿入する。ファイル宣言順を
  使う理由: checkerの後方参照禁止制約により、あるdefvarの初期化式が参照できるのは常に
  「宣言順で前」のdefvarのみなので、宣言順での昇格・初期化は依存関係を自動的に満たす。
- 昇格済みグローバルは、インタプリタ自身の`Expr::Global`/`Expr::SetGlobal`評価も
  permanent-root経由に切り替えた——実装中に「コンパイル側の書き込みがインタプリタ側の
  読み取りに反映されない」乖離バグをテストで検出し、修正。
- `src/compile/ast_bridge.rs`/`src/compiler.rs`: 当初`binding_kind`（GCルート要否の3値
  分類）でタグ付け要否を判断しようとしたが誤り——コンパイル済み値が「タグ付きi64か生の
  i64か」を決めるのは`ast_bridge::struct_field_kind`（defstructフィールド用の7値分類、
  `compile-sexpr-field`/`compile-tag-struct-field`という既存のエンコード/デコード機構）
  だった。テストで「読んだ値が8倍（タグ付きのまま）」というバグとして発覚し、正しい方の
  機構に差し替えて解決。

新設: `collect_global_targets`/`translate_global`/`translate_set_global`/
`ast_to_sexpr_for_global_init`（`ast_bridge.rs`）、`compile-global`/`compile-set-global`/
`compile-global-init`（`compiler.rs`、3つ目はAOT専用）、`Interp.compiled_globals`/
`promote_global`/`add_compiled_global_init`（`interp.rs`）。

Option/Result/ユーザーenum型のグローバルはcompile対象から参照できないまま（既知の制限、
`promote_global`が明示的な`Panic`で報告）。

段階分け（Stage A: 新規基盤 → Stage B: JIT対応 → Stage C: AOT対応）で実装し、各段階後に
`cargo test`全体＋`typelisp-rt`への`miri`を実行——このコンパイラは過去に1行変更が無関係な
既存テストを非決定的に壊した前例（`compile機能のmatch分岐ヒーゼンバグ`節参照）があるため。
`compiler.rs`の`compile-value`ディスパッチは20段超ネストした`(if (equal s ...) ...)`鎖で、
新tag追加時の閉じ括弧の手動カウントは事故りやすく実際に複数回ミスした——文字列リテラル/
コメント対応の括弧深度トレーサーをPythonで書いて`SOURCE`定数全体の均衡を検証してから
テストを回す方法が有効だった（`cargo build`はこの文字列の中身を検査しないため、構文
エラーはコンパイラ自身をtypelispとして読み込む実行時にしか発覚しない）。

新規テスト: `tests/compile_test.rs`（JIT読み書き+Option型エラー、3件）、
`tests/compile_file_test.rs`（AOT読み書き+JIT/AOT一致性、3件）。既存500件超は無回帰。

## LSP: 補完（textDocument/completion）実装（2026-07-12）

hover/goto-definitionに続き、TODO.mdで「未着手」としていた補完を実装。設計メモ
（TODO.md「元の設計メモ」節）どおり、エラー耐性が無いtypelispの`Reader`を騙すための
前処理層＋可視スコープ内の名前を`Registry`から列挙する方式を採用。

**前処理層**（`src/bin/lsp.rs`、LSPトランスポート固有なのでlib側には置いていない）:
- カーソル位置から後方に「区切り文字（`read/reader.rs`の`is_delimiter`と同じ集合:
  空白/`(`/`)`/`"`/`'`/`` ` ``/`;`）でない文字」が続く範囲を「入力中の識別子」として
  切り出し、それより前のテキストだけを再チェック対象にする——入力中の識別子自体を
  含めてしまうと「未解決の参照」としてチェック全体が失敗するため、丸ごと除外するのが
  一番簡単で確実だった。
- 残ったテキストはほぼ必ず閉じ括弧が足りていないので、`heuristically_close`が
  `read/reader.rs`のトークナイザを必要な範囲だけ模倣した独自スキャナで括弧の深さを数え、
  不足分の`)`を機械的に補ってから`Reader`に渡す。行コメント（`;`〜EOL）・ブロックコメント
  （`#| ... |#`、ネスト対応）・文字列リテラル（バックスラッシュエスケープ対応）・文字
  リテラル（`#\(`のような「1文字なのに括弧に見える」トークン）は括弧カウントの対象外に
  スキップする必要があり、これを外すと`#\(`のようなコードで簡単に深さがずれた。
- `char_offset`/`prefix_start`は`Position`(LSPの0始まり行/文字)からのオフセット計算・
  識別子境界探索。`read/reader.rs::Cursor`が`chars: Vec<char>`（バイトではなく文字単位）で
  行/列を管理しているのに合わせ、こちらも`Vec<char>`で統一（hover/goto-defが既に
  `pos.character + 1`をそのまま`Loc`の列として使っている簡略化を踏襲——UTF-16コード
  ユニット厳密対応はしていない、MVPスコープ）。

**列挙ロジック**（`src/check/locate.rs::completion_candidates`、lib側——`Checker`と
`Registry`にしか依存しないのでコアパイプラインのテスト（`tests/lsp_completion_test.rs`）は
`Checker::check_form`を直接駆動でき、LSPのstdioトランスポートを介さない）:
- `Checker::resolve_fn`/`resolve_global`等が使っている「同一モジュール（`cur_ns()`）→
  ルート（`public`なら可視）」という既存の2段探索順序を、名前引きではなく列挙という形で
  そのままなぞった（`Namespace::fns`/`macros`/`types`/`vars`/`traits`/`modules`/`ctors`/
  `aliases`/`mod_aliases`/`static_uses`の9テーブル）。
- 補完はチェック完了後の状態を見るため、`Checker`自身の`self.ns`（チェック中の一時的な
  モジュールパス、`check_module`がpush/popして最終的に空へ戻る）は使えない——代わりに
  `project::module_segs_for(file, src_root)`でファイルパスから独立に導出したモジュール
  パスを引数で渡す設計にした（`completion_candidates(reg: &Registry, module_path: &[String])`
  という自由関数、`Checker`のメソッドにしなかった理由もこれ）。
- チェッカーは単一パスでフォームを処理しながらレジストリを直接書き換えていくため、
  パッチ済みテキストが（別の理由で）チェック失敗しても、失敗地点より前に登録済みの定義は
  そのまま`Registry`に残っている——`candidates_for`は`load_entry_src`の`Result`を捨てて
  常に`checker.registry()`を読む設計にし、これを利用してわざと緩くした。

**既知の制限**（TODO.mdのhover/goto-defと同じ理由）: ローカル変数（`let`/lambda束縛）は
補完候補に出ない——`Registry`/`Namespace`ツリーにしか無い名前しか列挙できないため。

新規テスト: `src/bin/lsp.rs`内`#[cfg(test)] mod completion_helper_tests`（`heuristically_close`/
`char_offset`/`prefix_start`の単体テスト9件、文字列・文字リテラル・行コメート内の括弧を
誤カウントしないことを含む）、`tests/lsp_completion_test.rs`（`completion_candidates`の
モジュール可視性込み5件）。stdio経由の手動スモークテスト（`initialize`→`didOpen`→
`textDocument/completion`）でも、閉じ括弧が足りない未確定入力に対して補完が返ることを確認。

## LSP: ローカル変数の hover / goto-definition / 補完 対応（2026-07-12、3コミット）

上記の補完実装（コミット`3e6272d`）で hover/goto-definition/補完の3機能が揃ったが、
すべてに共通する既知の制限として「ローカル変数（`let`/`let*`/`lambda`/`labels`束縛・関数
パラメータ）が対象外」が残っていた。ユーザーがこの制限の「完全対応」を選択したため、
Plan modeで設計を固めた上でPart A→B→Cの3段階（各段階でcargo test全体緑を確認してから
個別コミット）で実装。

**根本原因（2つ）**: (1) 位置テーブル`Heap.cons_locs`はリスト形式の頭セルにしか記録されず、
`Value::Symbol`はインターンされ出現ごとの識別子が無いため裸の変数参照に位置を紐付けられ
なかった。(2) 束縛サイト（`let`の束縛名・関数パラメータ名等）の位置もどこにも記録されて
いなかった。

**設計方針**: `Expr`/`TopLevel`の束縛構造（`Expr::Let`のタプル・`Lambda.params`・`Labels`
defs・`Defun.params`）は変更しない——拡張するとインタプリタ（`eval/interp.rs`）とcompile
パイプライン（`compile/ast_bridge.rs`等、過去にヒーゼンバグの前例あり——
[[typelisp-compile-match-arm-heisenbug]]）に波及するため。位置情報は**チェッカ内部の`Env`
と`DefLocs`側テーブルに閉じ込め、interp/compileのコードには一切触れない**方針とした。

### Part A（commit `78a3602`）: 裸アトムへのソース位置付与

- `crates/typelisp-mem/src/heap.rs`: `cons_locs`と同型の新テーブル`elem_locs`
  （スパインセルaddr→そのcarの位置）を新設。`set_elem_loc`/`list_to_vec_locs`
  （`Vec<(Value, Option<Loc>)>`を返す`list_to_vec`の対）を追加、`clear_cons_locs`で
  併せてクリア。
- `src/read/reader.rs`: `read_list`の読み取りループで各要素の直前に`cur.loc()`を捕捉し
  （`elem_locs: Vec<Loc>`、`elems`と並行）、build ループ（スパインセルを後ろから組み立てる
  既存ループ）で各セルに`set_elem_loc`する。
- `src/check/checker.rs`: `check`を`check_at(..., loc_hint: Option<Loc>)`の薄いラッパに
  分割——`check_at`は「cons なら`cons_loc`、atomなら`loc_hint`」で`Typed.loc`を埋める。
  `args: &[Value]`を引数式としてcheckする約15のヘルパ（`check_call`/`check_if`/`check_let`/
  `check_seq`/`check_lambda`/`check_labels`/`check_apply`/`check_assoc_call`/
  `check_construct`/`check_match`/`check_setf`/`check_return`/`check_panic`/`check_the`/
  `check_as`/`check_list_lit`/`check_path_call`/`try_instance_method`/`check_instance_method`
  等）に並行スライス`arg_locs: &[Option<Loc>]`を追加し、`check_list`が`list_to_vec_locs`で
  `args`/`arg_locs`を対で生成して fan-out する。`nth_loc`ヘルパ（`arg_locs.get(i).cloned()
  .flatten()`）で個々のインデックス参照を簡潔にした。

これ単独で**ローカル変数へのhover**が動くようになった（`locate_node`が`Var`ノードを正確に
特定でき、`hover_text`は既存の`node.ty`をそのまま使うため無改造）。副産物として全アトム
（`Global`/`FnRef`/`MethodRef`/リテラル）のhover/goto-def精度も向上した。

新規テスト: `tests/lsp_locate_test.rs`にローカル変数/let束縛へのhoverが正しい型を返す
（周囲のノードの型とは異なる型を選ぶことで誤ってenclosingノードを掴んでいないか検証）
新規テスト2件。既存500件超は無回帰（miri `read_test`/`mem_test`も実施）。

### Part B（commit `1147dc7`）: ローカルのgoto-definition

- `src/check/registry.rs`: `DefLocs`に`local_refs: HashMap<(u32,u32), Loc>`（参照位置
  (line,col)→束縛サイトLoc）を新設。グローバル定義と異なり`Path`で一意識別できないため
  参照位置自体をキーにする——**クエリ時にスコープ探索をしない**設計（チェック時に一度だけ
  解決して焼き込む、[[feedback-static-types-are-always-known]]の方針）。
- `src/check/checker.rs`: `Env.vars`を`Vec<(String, Type)>`→`Vec<(String, Type,
  Option<Loc>)>`に拡張（3-tuple化、checker内部構造でありASTではないため影響範囲は
  checker.rs内に閉じる）。`Env::get_loc`/`Env::extended_with_locs`を新設（既存の
  `Env::extended`は「位置を追跡しない」呼び出し元向けに維持——match束縛や
  monomorphization再チェック等）。`Checker`に`local_refs: RefCell<HashMap<(u32,u32),
  Loc>>`スクラッチフィールドを新設（`loop_stack`/`warnings`と同じ「`&self`の再帰中に書く」
  interior mutabilityパターン）、`check_at`で`Expr::Var`が`env.get_loc`にヒットしたら
  参照位置→束縛位置を記録し、`check_form`（`&mut self`の外側境界）の終端で
  `self.reg.def_locs.local_refs`へ`drain`して反映。
  `parse_params`/`parse_param_pairs`/`parse_defmethod_sig`（`MethodSig`に
  `self_name_loc`/`param_locs`追加）を各束縛名の位置も返すよう拡張し、
  `check_let`/`let_star_rec`/`check_lambda`/`check_labels`/`check_defun`/`check_defmethod`
  が`extended_with_locs`を使うよう更新。
- 副産物のバグ修正: `check_list`の「ローカル変数を関数値として呼ぶ」分岐
  （`labels`で定義した関数を`(g 1)`のように呼ぶケース）が`check_at`を介さず手組みの
  `Typed{ loc: None, ... }`を作っていたため、コールバック自身の位置が付かず`labels`関数名の
  goto-defが解決できなかった——`check_at`経由に修正（非symbol-headの callee 分岐と対称に
  なった）。
- `src/check/locate.rs`: `definition_target`に`Expr::Var`アームを追加、
  `def_locs.local_refs.get(&(node.loc.line, node.loc.col))`を引くだけ（スコープ探索なし）。

match束縛は意図的にスコープ外のまま（既存`Env::extended`を維持、`Pattern::Bind`に位置
追跡機構が無い）。`Expr`/`TopLevel`の束縛構造・interp/compileパイプラインは無変更。

新規テスト: `tests/lsp_locate_test.rs`にgoto-def成功系4件（param/let/lambda/labels、
labelsは関数名自体と内部パラメータの両方を検証）+ match束縛は従来通り`None`の確認1件。
既存無回帰。

### Part C（commit `8ac9dd9`）: 補完へのローカル名列挙

- `src/check/locate.rs`: `completion_locals(body, file, line, col) -> Vec<String>`を新設。
  `locate_node`でカーソル位置の最小ノード（target）を特定した後、`body`の先頭から
  スコープスタックを持って**同じ木を再走査**し、`Let`/`Lambda`/`Labels`ノードに入る際に
  束縛名をpushしてから子へ降り、`std::ptr::eq(node, target)`でtargetに到達したらそこで
  蓄積済みスコープを返す（`scope_top_level`/`scope_typed`の相互再帰）。`let`の束縛値は
  outer scopeでチェックされる（CL `let`セマンティクス、`check_let`と同じ順序）ため、
  束縛値の探索は該当名をpushする**前**に行う。
  - 実装中に見つけたバグ: 当初`ptr::eq`チェックを各関数の**先頭**（match分岐に入る前）に
    置いていたため、target が「スコープ導入ノード自身」（例：`let`の束縛リストを閉じた
    直後、本体にまだ何も無い状態でカーソルが位置するケース）に一致した場合、
    その束縛名を一度もpushしないまま早期returnしてしまっていた。`Let`/`Lambda`/`Labels`
    の各分岐内で「束縛をpushした**後**」に`ptr::eq`判定するよう再構成して解消
    （stdio経由の手動スモークテストで発覚——ユニットテストは束縛リスト閉じ直後ではなく
    実在の本体文の位置を使っていたため検出できなかった）。
- `src/bin/lsp.rs`: `candidates_for`がチェック成功時のみ`completion_locals`もマージ
  （Registry候補は従来通りチェック失敗時もbest-effort、`completion_locals`は完全な
  `Typed`木が要るため成功時限定）。副産物として`needs_completion_placeholder`を新設：
  補完リクエストは「入力中の識別子を除去→閉じ括弧を補う」設計のため、識別子がbody/
  引数位置の**最後の要素**だった場合に「bodyが空になる」「呼び出しの引数が足りない」
  という、識別子そのものとは無関係な理由でチェック全体が失敗し、ローカル補完が
  丸ごと無効化される問題が判明（デバッグ用の直接実行スクリプトで検証）。
  識別子の直前（末尾空白除去後）が`(`でなければ（＝そのリストの先頭要素として新規に
  タイプ中ではなく、既存の兄弟要素の後ろに続く位置だと分かれば）、閉じ括弧の前に
  `(panic "")`（`Type::Never`型、`(defun f () bool 0)`は型不一致エラーだが
  `(defun f () bool (panic ""))`は成功することを実測確認済み——「あらゆる期待型を満たす」
  性質を利用）を挿入する。`(`直後（新規呼び出しのcallee名がタイプ中）の場合は挿入しない
  ——空リスト`()`は`Unit`として素直に読めるが、`(panic "")`を挿入すると
  `((panic ""))`となり Never 値が「呼び出し可能な値として apply される」形になって
  `value is not callable`エラーを起こす回帰になるため。

既知の残存ギャップ: `completion_locals`のdocコメント参照——`Typed.loc`が範囲でなく開始点
のみのため、スコープ境界直後（束縛リストを閉じた直後で本体に何も無い状態）でのみ稀に
局所名を取りこぼす（本体に既存の文が1つでもあれば発生しない）。

新規テスト: `tests/lsp_completion_test.rs`に`completion_locals`単体3件（param+let束縛の
基本ケース、兄弟let間でスコープが漏れないこと、matchパターン束縛が対象外であること）、
`src/bin/lsp.rs`に`needs_completion_placeholder`単体3件。stdio経由の手動スモークテストで
ローカルパラメータ+let束縛の両方が実際に補完候補に出ることを確認、既存の完了パス
（"add"がregistry経由で見つかる元のスモークテスト）も無回帰。cargo test全体green。

## LSP: matchパターン束縛のgoto-def/補完 + スコープ境界直後の補完取りこぼし解消（2026-07-12）

前節の既知の残存ギャップ2件を解消。

**matchパターン束縛（`(match x ((Some y) ...))`の`y`）**:
- `src/check/checker.rs`: `check_pattern`/`check_ctor_pattern`の戻り値の束縛リストを
  `Vec<(String, Type, Option<Loc>)>`に拡張。位置の出どころは、アーム全体の変数パターンなら
  アームの`list_to_vec_locs`が返す先頭要素位置、コンストラクタパターンのフィールドなら
  `check_ctor_pattern`を`list_to_vec`→`list_to_vec_locs`に切り替えて得る各要素位置
  （ネストした`Ctor`にも再帰で伝播）。`check_match`は`env.extended(binds)`→
  `env.extended_with_locs(binds)`に変更するだけで、既存の`check_at`の
  「`Var`参照解決時に`env.get_loc`→`DefLocs::local_refs`へ記録」経路にそのまま乗り、
  goto-definitionが機能する。
- `src/check/locate.rs`: `scope_typed`に`Expr::Match`アームを新設——scrutineeは外側スコープで
  走査し、各アームは`pattern_bind_names`（`Bind`名+`Ctor`のサブパターン再帰）をpushしてから
  本体を走査、見つからなければtruncate。兄弟アームへの束縛リークなし。

**補完のスコープ境界直後の取りこぼし**:
- 真因はプレースホルダの挿入位置。`handle_completion`は`truncated.push_str(" (panic \"\")")`と
  **先頭スペース付き**で挿入していたため、`(panic "")`の記録位置がカーソル（＝入力中識別子の
  開始位置、`completion_locals`へ渡す`(line, col)`）の1桁**後ろ**になり、`locate_node`の
  「開始位置がカーソル**以前**で最大のノード」検索から漏れて、直前の束縛値ノード等が
  targetになりスコープ導入前で探索が終わっていた。`truncated`は識別子開始位置ちょうどで
  切られており直前は必ず区切り文字なので、スペースなしで直結すれば位置が完全一致し、
  プレースホルダ自身がカーソルのノードになる。1文字の修正+doc追記。
- プレースホルダを挿入しない「`(`直後のcallee入力中」ケースは、切り詰めで残る`()`（Unit）が
  reader の要素位置（`Heap::elem_locs`は`Value::Empty`要素にも記録する）を保持しているため
  元々スコープ内に解決される——テストで固定化した。

検証: `tests/lsp_locate_test.rs`にctorフィールド束縛/アーム全体束縛のgoto-def 2件、
`tests/lsp_completion_test.rs`にmatch束縛の提供/兄弟アーム非リーク/境界2形状（`(panic "")`型・
`()`型）の計4件（従来の「match束縛が対象外」テストは反転）。cargo test全38バイナリgreen。
stdio実測（initialize→didOpen→completion/definition）でgoto-def・アーム内補完・let本体
先頭フォーム入力中の補完がすべて機能することを確認。

副産物の発見（TODO.mdに既知の制限として記録）: catchallでないアーム本体内の補完は、
カーソル以降の切り捨てで後続アームが消え非網羅エラーになるため、ローカル候補が出ない
（truncate設計固有の制約、複数引数切り捨てと同族）。→次節「チェッカーのエラー回復モード」で解消。

## チェッカーのエラー回復モード（2026-07-12、前節の既知の制限を解消）

**背景**: 前節で「既知の制限」として記録した「非catchall `match`アーム本体内の補完で
ローカル候補が出ない」問題の根治。補完のtruncate設計（カーソル以降を切り捨て+`(panic "")`+
閉じ括弧補完）自体は変えず、**チェッカーを「最初のErrで中断」から「エラーを蓄積しつつ
部分的な`Typed`木を返す」設計**に拡張した。場当たり修正（網羅性チェックだけ緩和／末尾温存）
ではなく回復型への作り替えを選択したのは、(1)型エラーのあるファイルでもhover/goto/補完を
動かしたい、(2)ファイル内の全エラーを一度に診断したい、という汎用的な要求のため。

**アーキテクチャ**: `Result<_, Error>`のシグネチャは全て維持。`Checker`に`recover: bool`
フラグと`errors: RefCell<Vec<Error>>`蓄積器を追加（`warnings`/`take_warnings`と同型の前例に倣う）。
デフォルト`recover=false`は現行の厳格動作と完全一致するため、CLI/REPL/prelude/既存テスト
（600+件）は無変更。LSPの`diagnostics_for`/`candidates_for`だけが`set_recover(true)`する。

- **ホール表現**: 新`Expr`variantは追加せず`Expr::Panic`を再利用し`ty: Type::Never`。
  `Never`は期待型照合・`join_types`で万能に吸収されるためカスケードエラーが出ず、
  interp/ast_bridge/locate.rsは`Panic`を既に扱えるので下流は無変更。`Checker::hole(loc)`が生成。
- **回復ヘルパー**: `recovered(r, loc)`（式スロットに`hole`を差し替え）と
  `push_recovered(e, loc)`（`match`アーム/網羅性チェックのようにスロットがない箇所で記録のみ）。
  いずれも`Error::at`の最内優先で位置が付く。

**回復境界（6箇所）**:
- **B1** `check_match`の網羅性チェック: recover時は`Err`にせず記録して**完全な`Match`ノードを返す**
  （全アームは既にチェック済み）。動機バグはこれで直接解消。
- **B2** `check_match`のアーム単位: 形状/`check_pattern`失敗のアームを記録してスキップ、
  `arm_recovered`フラグで網羅性チェック自体を抑止（スキップでカバレッジ不明のため）。
  `join_types`失敗はアームを保持したまま結果型のみ据え置き。
- **B3** `check_seq`の要素単位: 本体の1フォームがホール化しても兄弟フォームは検査続行。
  `defun`/`defmethod`/`defmacro`/`lambda`/`let`/`labels`/`loop`本体+`match`アーム本体すべてに効く
  最重要境界。
- **B4** `check_let`/`let*`の束縛init: initがホール化しても束縛は`Never`型でスコープに残り、
  本体（と全束縛）が補完に見える。
- **B5** `check_form`最上位: フォーム全体の`Err`を記録し`TopLevel::Expr(hole)`を返す。
  `spec_pending`クリアを忘れると次フォームの`debug_assert`が落ちる。これにより
  `load_source_inner`は無変更で最後まで走り、ファイルの`TopLevel::Module`が`pending`へpushされ、
  部分木がLSPに届く。
- **B6** `drain_specializations`: 主フォームは検査成功したが単型化に失敗した場合、主フォームの木を
  保持しバンドルのみ破棄。単型化の中身は厳格のまま（テンプレート＝別ファイルの位置をホール化
  すると位置が狂うため記録のみ）。

**LSP配線**:
- `diagnostics_for`: prelude（厳格）ロード後に`set_recover(true)`。`take_errors()`で
  蓄積エラーを全て`error_diagnostic`化＝複数診断。`Analysis`は`result.is_ok()`
  （＝reader成功で木がある）なら型エラーがあっても構築。readerエラー時のみ`None`で
  last-good維持。
- `candidates_for`: `set_recover(true)`+`result.is_ok()`ゲートを撤廃し常に`completion_locals`を
  実行。readerエラー時は`pending`が空→`completion_locals`が`file`フィルタで空を返すので安全。

**検証**: `tests/lsp_completion_test.rs`に`program_recover`ヘルパー（recoverモードで検査し
`(body, errors)`を返す）と回復テスト7件——複数フォームエラー蓄積+間の`defun`は生存、
非catchall `match`アーム内でローカル提供（動機バグ）、不正アームのスキップ+良アーム生存、
不正本体フォームのホール化+兄弟生存、init失敗letの束縛保持、独立3エラーの個別位置記録。
`src/bin/lsp.rs`の`completion_helper_tests`に`candidates_for`のE2E 1件（実際の
patched text→recover経路でローカル候補が出ることを確認）。cargo test全green、
厳格経路（既存600+件）は無変更。

## クロージャ表現統一（labels/closures unification）実装計画（2026-07-17起案、Stage 1-10全完了 2026-07-18）

**背景**: interpクロージャとcompiledクロージャ(ClosureBox)の二重表現による境界ギャップ
（interpクロージャをcompiled関数に渡せない/compiled関数のFn戻り値がIntに化ける/structフィールド
へのFn格納不可）を解消する10段階計画。方針は「クロージャは定義時にJITコンパイルしてcompiled
表現に統一、捕獲変数へのsetfは共有セル（CL的）」。計画書全文は元セッションの
`~/.claude/plans/compile-traitcall-traitcall-cuddly-corbato.md`（リポジトリ外）。

Phase I（表現統一、Stage 1-6）は2026-07-17完了——Stage 1: mem/rt基盤
（`BoxedObj::CompiledClosure`+`rt_closure_*`/`rt_cell_*`）、Stage 2: compiled側flip+ARC全廃、
Stage 3: 境界開通（Fn引数/戻り値/structフィールド）、Stage 4: 共有セル捕獲
（`Ctx::cell_names`/`Ctx::visible_siblings`、`cellvar`/`cellset`タグ）、Stage 5: compileパイプライン
のTarjan SCCベース多パス化（`Interp::compute_sccs`/`compile_scc`、`CompiledFn::new_multi`）、
Stage 6: defun/lambda/labels/matchアームの複数式body対応（`ast_bridge::single_body_expr`、
`(let () e1 e2 ...)`ラップ）。全stage独立コミット・`scripts/test-serial.sh`全green維持。

**Stage 5実装時の発見（Stage 7設計に影響）**: 別々のトップレベル`defun`同士の構文上の相互再帰は
現行チェッカー（`Checker::check_form_at`がトップレベルformを出現順に1つずつ検査、前方参照不可）
では**そもそも記述不能**（`docs/syntax.md`が`labels`を「相互再帰可能なローカル関数定義」と
明記する通り、相互再帰は`labels`専用の言語機能）。Stage 5のSCC機構は当初ユーザー可視の
プログラムからは（生成した`FnDef`を直接`Interp::fns`へ注入する`src/eval/interp.rs`の
`scc_tests`モジュールでのみ）到達しなかったが、**Stage 7で各`labels`siblingが独立JIT単位になった
時点で実際に必要になる**基盤という位置づけだった（実際にStage 8でその役割を果たした——後述）。

**Stage 7: 定義時JITドライバ 完了**（2026-07-17）——`Expr::Lambda`/`Labels`/`FnRef`/`MethodRef`の
evalアームが、interpクロージャ（`make_closure`）を作る前にまず定義時JITを試みるようになった
（`Interp::jit_define_closure`、`interp.rs`）。設計は計画書からの想定どおり進んだ部分と、
実装時に判明して簡略化した部分がある——後者を優先して記す。

- **「コンストラクタ関数」の実体は自己ホストコンパイラの`compile-lambda`をそのまま再利用**:
  計画書は「捕獲セル参照を引数に取りClosureBoxを返すコンストラクタ関数」を新規にJITする想定
  だったが、実装時に`compile-lambda`（`compiler.rs`）自身が既にその形をしていると判明した——
  ラムダ/labels siblingを**捕獲名だけを引数に取る使い捨てトップレベル`defun`の唯一のbody式**
  として包む（`(defun __ctor (cap1 cap2 ...) (lambda name (captured...) (params...) body))`
  相当）だけで、既存の`compile-function`/`compile-lambda`パイプラインがClosureBox構築まで
  丸ごとやってくれる。唯一の仕掛けは、ctor自身の各引数の宣言型を**捕獲変数の実際の型ではなく
  常に`Sexpr`にする**こと——`struct_field_kind`のkind`6`（タグ付きポインタのパススルー、
  GCルート付き）を経由させることで、渡した既存セル参照（`Value::Boxed(cell_id)`）が
  再パッケージされず生の値のまま`env`に束縛され、内側の`(lambda ...)`が`compile-lambda`の
  outer half（`compile-escaping-env-args`/`resolve-value`）で読み出す時にそのまま拾える。
  結果、Stage 7のために新規の自己ホストLispコードは一切書いていない
  （`Interp::translate_and_compile`——旧`add_compiled_function`本体を汎用化したRust側の薄い層のみ）。
- **`Slot::TypedCell`**（`src/eval/value.rs`）: `Slot::Heap`と同じ`BoxedObj::Cell`だが
  `RtValue::Sexpr`限定ではなく任意の型を`rtvalue_to_struct_field`/`decode_field_typed`
  （`defstruct`フィールドと同じエンコード）で出し入れする。`Interp::apply`/`Expr::Let`が
  `freevars::names_captured_by_nested`で「ネストしたlambda/labelsに捕獲される束縛」を検出し、
  型がNative（スカラー等）かつ**JIT表現可能**（下記`is_jit_tier_ty`）な場合だけこのセルへ昇格する
  ——`Slot::Heap`が既にセルであるHeap-kind束縛（struct/enum/Fn等）は変更不要。
- **JIT tier判定**（`Interp::is_jit_tier_ty`）は`ast_bridge::struct_field_kind`のkind`0`
  （非対応の catch-all）を「compiled表現なし」の判定にそのまま流用——`llvm-builder`/
  `Scope<llvm-value>`等の自己ホストコンパイラ島専用ネイティブ型だけがここで弾かれ、
  特別な「コンパイラ島判定」を書かずに型システムだけでブートストラップ・パラドックス
  （compile-functionの実行に自身のJITが必要になる循環）を回避できた。
  - **落とし穴（初回実装で規模の大きい回帰を2件出した）**: (1) `Interp::apply`の新しい
    セル昇格判定を`is_jit_tier_ty`でガードし忘れ、`compile-function`自身の呼び出し
    （`m: llvm-module`引数、自身の`labels`内siblingに捕獲される）で`RtValue::LlvmModule`を
    セルへ詰めようとして`rtvalue_to_struct_field`が内部エラーになり、`(compile ...)`を使う
    テストが軒並み壊れた——`compile_test.rs`161/173件が一時失敗。(2) `Expr::Labels`/
    `jit_define_closure`が`collect_call_targets`/`collect_assoc_targets`の結果を無フィルタで
    「未コンパイルなら失敗」扱いしていたため、`i32::+`等のネイティブ組み込みメソッド
    （コンパイル時は`compile-assoc`が直接LLVM命令へlowerする、`self.methods`に登録がない）まで
    要求してしまっていた——`Interp::call_graph_edges`と同じフィルタ（vector-op/hashtable-op
    除外、`i64`/`i32`/`char`/`string`/`f64`/`bignum`/`ratio`はネイティブ、ユーザー定義
    methodは`self.methods`にあれば要求）を複製して解消。
  - **スタックオーバーフロー**: `Expr::Labels`/`Expr::Lambda`のtier判定を「捕獲名の型」より
    前に「宣言パラメータの型」だけで先にふるいにかける（`freevars::lambda_free_vars`という
    body全体を再帰的に歩く重い解析を、対象が自己ホストコンパイラ島だと分かった時点で省略する）
    ようにしたが、それでも`Interp::apply`自身の`names_captured_by_nested`呼び出し
    （`compile-function`の巨大な`labels`本体を毎回歩く）が`freevars::walk`の
    `Expr::If`素朴再帰と組み合わさり、`compile-value`ディスパッチ（~40タグの右下がりif連鎖）
    で`compile_dispatches_bignum_comparisons_and_agrees_with_the_interpreter`をスタック
    オーバーフローさせた。`Interp::eval`の`Expr::If`アームが既に採用している「反復的に
    else連鎖を辿る」手法（本ドキュメントの"interp if連鎖スタック
    オーバーフロー"参照）を`freevars::walk`にも移植して解消——既存の
    `scripts/test-serial.sh`の`RUST_MIN_STACK=32MB`余裕と合わせて全green。
  - **`set_active_heap`漏れ**: コンストラクタ関数呼び出し（`CompiledFn::call`）の前に
    `crate::compile::runtime::set_active_heap`を呼び忘れ、`rt_closure_new`等が
    "no active Heap registered" でpanicしていた——`Interp::call_compiled`と同じ手順を追加。
- **マクロ展開時のJIT抑止**: `Interp::jit_suppressed`（再入可能な`Cell<u32>`）を新設、
  `expand_macro`が`self.apply`を呼ぶ前後でinc/dec。
- **JITエンジンの「墓場」+定義サイトキャッシュ**: `Interp::jit_graveyard`
  （`Vec<Rc<CompiledFn>>`、永続保持でengine/codeを生かし続ける）と`Interp::jit_ctor_cache`
  （`(Loc, 型のDebug文字列) -> Rc<CompiledFn>`、`Type`に`Eq`/`Hash`が無いためDebug文字列で代用）。
  `Loc`が無い合成ノード（`labels` siblingをラップする際に新規合成する`Typed`等）はキャッシュ
  対象外——毎回JITし直す（正しさ優先、Stage 8以降の最適化候補）。
- **環境変数`TYPELISP_CLOSURE_JIT=off|prefer|require`**実装（デフォルト`prefer`、Stage 9で
  `prefer`は撤去され`off|(既定)`の2値へ整理——詳細は下記Stage 9）。当初`require`は全green化
  していなかった（想定どおり、Stage 8の仕事）——具体的には自己ホストコンパイラ自身のreentrant
  実行中に別のJIT試行が失敗すると`require`下ではそのままpanicとして伝播する等、被覆の狭さが
  そのまま見えた。`prefer`（当時のデフォルト）は`scripts/test-serial.sh`全体で green。
- テスト: 新規`tests/closure_jit_test.rs`（`(compile ...)`を一切呼ばない純粋interp実行での
  FnRef/MethodRef/lambda捕獲/make-counter型共有セル/labels相互再帰/labels間セル共有/
  マクロ展開との共存、計9件）。`TYPELISP_CLOSURE_JIT=require`で個別に再実行すると
  実際にJIT経路を通ったことを確認できる（ただし上記の理由で当初は全件通らなかった）。

**Stage 8: compiled被覆拡大 完了**（2026-07-18、6コミット）——受け入れ基準だった
「`TYPELISP_CLOSURE_JIT=require`での全体`scripts/test-serial.sh`完走」を達成
（41テストバイナリ全green、デフォルトpreferも当然green）。requireログの失敗約240件を
根因5種に分類し、次の順で潰した:

1. **`JitDecline` benign/gap二分類**——requireが「実際に埋めるべき被覆ギャップ(Gap)」だけを
   hard error化し、恒久的に正当なfallback（Benign＝Stage 9でも許容が確定しているもの:
   off指定／マクロ展開中／native tier型／コンパイラ島未ロード（`typl` CLI/REPLは
   `load_compiler`を呼ばない設計）／JIT中のheap枯渇（資源起因。needleは
   `mem::Error::HeapExhausted`のDisplayから構築し文字列照合））はrequire下でも静かに
   fallbackする。これが最大の雪崩3種を解消——(1)reentrantな`compile-function`実行中に
   コンパイラ島自身のlabelsのtier落ちがpanic化して外側のcompile全体を殺す
   （compile_testの129件・`--lib` SCCテストの正体）、(2)コンパイラ未ロードテストでの
   全lambda定義失敗、(3)極小ヒープGC圧テストのheap枯渇panic。
2. **推移的ターゲットcompile**——`jit_define_closure`が未compileのcall/assocターゲットで
   諦める代わりにStage 5のSCC機構（`compile_function`/`compile_scc`）を駆動して依存グラフ
   ごとcompile（Stage 5計画書の「Stage 7で実際に必要になる基盤」がここで本来の役割に就いた）。
   副産物でStage 7の潜在バグも修正: ctorのmethod external宣言/配線が`tl_`プレフィックスなしの
   旧`method_link_name`を使っており`compile-assoc`の`tl_type::method`ルックアップと不一致
   だった（「未compileなら即Err」ガードの陰で従来は到達不能）。
3. **`Unit`戻り値**——tier判定で戻り値位置に限り`Unit`を許可（`compile-unit`は元から存在。
   引数/捕獲位置はcrossing encodingが無いため除外のまま）+`decode_compiled_return`に
   `Type::Unit`アーム（compiled Unit戻りが生`Int(0)`でなく`RtValue::Unit`に。
   既存テスト3件が文書化していた旧挙動の期待値も更新）。
4. **`sexpr-*`アイランド層全体の`rt_*`シム**——car/cdr/cons以外（述語`consp`/`null`/`atom`/
   `symp`、抽出`int`/`bool`/`char`/`str`/`sym-name`）にcompiled loweringが無く、Sexprリストを
   歩くユーザー関数（`&rest`消費側の全て）がcompile不能だった。`rt_consp`等9関数を新設
   （`sexpr-float`は既存`rt_float_value`を再利用——bits-in-i64がcompiled f64規約そのもの）、
   `compile-call`のrename表・`is_rt_builtin_name`・`rt_extern_functions`に追加。あわせて
   compiled関数の**string戻り値**が`is_boxed_sexpr_type`に掛からず生Int化する既知の文書化済み
   ギャップも`Type::Str`アームで解消。
5. **`&rest`クロージャ解禁**——Stage 7の一律拒否を撤去するだけで動いた: checkerがrest引数を
   params末尾へ`(名前, Sexpr)`として折り込み済みで、余剰引数のパッキングも呼び出しサイトで
   check時に完了している（`wrap_rest_elem`/`cons_rest_list`）ため、apply時点でアリティは
   常に一致する。

Stage 8で**表面化しなかった**ためやらなかったこと（要求が現れたら再訪）:
- 計画時に候補として挙げていた`random`/`gensym`/`equal`/`equalp`等の`rt_*`シムと
  `compile-assoc`のlowering追加——現テストスイートのJIT経路からは一度も要求されなかった。
- **モジュール修飾defun（`m::inc`）のcall target**——SCC機構のノード同一性がroot名前提
  （`CallEdge::node_name`が`Path::local`へ潰す/`resolve_fn_def`が`Path::root`で引く）のため
  推移的compile不能で明示Gapにしてある（Stage 5からの継承制限）。現テストでは
  コンパイラ島未ロード（benign）の陰に隠れて顕在化しない。

**Stage 9: JIT必須化 完了**（2026-07-18）——受け入れ基準だった「（オプトインなしの）通常の
`scripts/test-serial.sh`完走」を達成（全ターゲットgreen、`TYPELISP_CLOSURE_JIT`を一切設定
しない素の実行で）。Stage 8時点では`Gap`失敗が`Require`モード（明示的opt-in）でのみ
`EvalError::Panic`に昇格し、デフォルトの`Prefer`モードは黙ってfallbackしていた——Stage 9は
「`Require`が正しい動作」という前提のもと、この昇格を無条件化しただけ（新規のfallback判定
ロジックは書いていない、Stage 8の`JitDecline` Benign/Gap分類がそのまま土台）。

- **`JitMode`を2値に整理**（`src/eval/interp.rs`）: `Off`/`Prefer`/`Require`の3値だった
  ところ、`Prefer`は「意味が消えた」ため削除——`Require`の「`Gap`は即`Panic`」という挙動を
  唯一の非`Off`モード`On`としてデフォルト化した。`jit_result_or_make_closure`の
  `if jit_mode() == JitMode::Require { ... }`ガードを撤去し、`Err(JitDecline::Gap(reason))`
  を無条件で`EvalError::Panic`にマッチさせる`match`アームへ差し替え——`Off`モードは
  `jit_define_closure`の入口で常に`Benign`declineとして即return する設計だった（Stage 7
  から不変）ため、`Off`が`Gap`分岐に到達することはそもそもなく、ガード撤去は安全と確認済み。
  環境変数`TYPELISP_CLOSURE_JIT`の文字列パースは`"off"`のみ意味を持ち、`"require"`/`"prefer"`
  含む他の値は（後方互換のため）すべて`On`として無害に解釈される。
- **Stage 8で残っていた2つの論点は「現状維持」で決定**——いずれもBenignとして恒久許容する側を
  選んだ（Stage 8の`JitDecline::Benign`docコメント自体が既にこの5項目を「Stage 9が残す
  fallback集合」と明記していたため、実装上の変更は不要だった）:
  - **コンパイラ島未ロード**: 「`typl` CLI/REPLでも`load_compiler`を常時呼ぶ」方向へは
    変更しなかった——`tests/*.rs`の大半（`struct_test`/`vector_test`/`iter_test`等）が
    軽量な`Interp::new()`+`load_prelude`のみで多数のクロージャ評価を行っており、
    全部にコンパイラ島ロードを強制すると起動コスト・ブランチ影響範囲が「JIT必須化」という
    今回のスコープを大きく超える。「未ロードなら諦めてinterpクロージャを使う」という
    既存のBenign fallbackを恒久仕様として確定させた。
  - **JIT中のheap枯渇**: 資源起因の一時的条件であり表現力のギャップではないため、Benignの
    ままとした（インタプリタ経路の通常のheap枯渇と同様、そもそも`HeapExhausted`は既に
    到る所で`Panic`化する一般的な失敗モードであり、JIT構築中に限って隠す理由がない一方、
    「構築中に限ってこれだけ黙ってinterpにfallbackする」挙動を変える積極的理由もない
    ため現状維持）。
  - 上記2点により、実質的なBenign集合は「native tier型」「マクロ展開中」
    「`TYPELISP_CLOSURE_JIT=off`」「コンパイラ島未ロード」「JIT中heap枯渇」の5項目のまま
    （Stage 8から不変）。
- ドキュメント更新: `JitDecline`/`JitMode`/`jit_or_make_closure`/`jit_result_or_make_closure`
  等のdocコメントから「`Prefer`/`Require`」という語彙を一掃し、「常時`On`、`Off`だけが例外」
  という新しい前提に書き換え。`tests/closure_jit_test.rs`冒頭のモジュールdocコメントも
  「デフォルトは`prefer`」という古い説明を削除し、Stage 9後の挙動（`Gap`は常にpanic、
  Benign集合のみ黙ってfallback）に合わせて更新。
- 検証: `scripts/with-llvm-env.sh scripts/test-serial.sh`（`TYPELISP_CLOSURE_JIT`未設定、
  つまり新デフォルト = 旧`require`相当）を通し実行、全ターゲットgreenを確認
  （Stage 8が`TYPELISP_CLOSURE_JIT=require`個別実行で先に証明済みだった内容が、
  無条件デフォルトとして再現されたことの確認）。

**Stage 10: 掃除 完了**（2026-07-18）——

- `call_compiled`（`Interp::encode_crossing_args`）のinterpクロージャ拒否コメントを更新:
  旧コメントは「pre-6b `RtValue::Closure`拒否と同じ姿勢」とだけ述べていたが、Stage 9で
  `Gap`起因のinterpクロージャ生成が消えたため、この分岐に現実に到達しうるのは恒久Benign集合
  経由の値だけになった、という正確な説明に書き換えた——native tier型のクロージャはそもそも
  compiled関数のパラメータ型になり得ない（native tier自体がcompiled表現を持たない型の
  集合であり、compiled関数の型シグネチャに現れようがない）ため実際にはここへ到達せず、
  現実的には`TYPELISP_CLOSURE_JIT=off`（JITを全面無効化した状態で別途`(compile name)`された
  関数へinterpクロージャを渡す）か、JIT中のheap枯渇でJIT可能だったはずのクロージャが
  たまたまinterp表現のまま残っていた場合にのみ発生する、と明記。
- `compiler.rs`/`freevars.rs`の「documented gap」記述を調査した結果、Stage 0起案時に
  計画書が参照していたcompiler.rs側の「捕獲変数setfのスナップショット挙動」はStage 4の
  共有セル化で既に解消・書き換え済み、freevars.rsには現在該当する記述が存在しない
  （`grep`で確認）、`ast_bridge.rs`の`struct_field_kind`ギャップコメントもStage 2/3のARC全廃
  完了時点で「Fn/enumいずれもgapは解消済み」へ既に書き換え済みと確認——Stage 10時点で
  追加の削除・書き換えは不要だった（計画時の想定より前倒しで各stageの実装中に片付いていた）。
- `docs/dev/language-design.md`§0（lambda節）を更新: クロージャが既定で
  `BoxedObj::CompiledClosure`として定義時JITされる旨、恒久fallback5項目、捕獲`setf`の
  共有セル意味論を明記。
- 本節（クロージャ表現統一 Stage 1-10 全体）を`docs/dev/TODO.md`から本ドキュメントへ移設し、
  TODO.md側は下記フォローアップ1件のみを残す形に整理。

**フォローアップ（TODO.mdへ記録・未着手）**: interpクロージャの完全削除
（`BoxedObj::Closure`/`ClosureBody`/`Capture`/`closure_bodies`/`make_closure`/`Expr::Apply`の
interpクロージャアーム）は本計画のスコープ外——自己ホストコンパイラ島自体（`compile-function`
等が読む埋め込みSOURCE）をAOT化し、コンパイラ島自身がJITを必要としなくなることが前提になる
（そうなって初めて、恒久Benign集合のうち「native tier型」の存在理由が消える）。

## interpクロージャ完全削除 実装計画（2026-07-18起案、Stage 1-8c全完了 2026-07-19）

**背景**: 上記クロージャ表現統一計画のフォローアップ。恒久Benign fallback 5項目のうち
「マクロ展開中」「コンパイラ島未ロード」「native tier型」を消すには、コンパイラ島自体
（`src/compiler.rs`の埋め込みSOURCE）をAOT化し島自身がJITを必要としない状態にする必要がある。
branch `feature/closure-unification`→（Stage 8以降）`feature/interp-closure-removal`。
ユーザー決定: フル実装・島は真のAOT化（λリフトではなくコミット済みbitcode方式）。

- **Stage 1（`73d7763`）**: llvm-*型のcompiled表現をi64ハンドルレジストリ化し、単一ディスパッチ
  `rt_llvm_call(op, a1..aN)`で既存`eval_llvm_builtin_method`へ橋渡し。個別シムを書かずに
  約45メソッドを吸収。
- **Stage 2（`ca85b84`）**: 島41 defun全数のSCC経由self-compileが成立することを検証、
  `char->int`をネイティブ化。
- **Stage 3（`99bba76`）**: `typl-bootstrap-island`バイナリでcheck済み島を`Fasl::capture`+
  全defunをcompiled moduleへ書き出し、`src/compiler_island.bc`+`.fasl`としてコミット。
  鮮度テスト（埋め込みfaslの`source_hash` vs 現在の`SOURCE`のhash）を追加。初回生成時は
  現行interp島で実行（この時点ではinterpクロージャ健在）。
- **Stage 4（`be47b95`）**: 実行時AOTローダ`compiler::load_aot`——`Fasl::from_bytes`+
  `Module::parse_bitcode_from_buffer`+`CompiledFn::new_multi`（externalsは`rt_extern_functions()`を
  module内宣言でフィルタ）+`Interp::install_compiled`。
- **Stage 5（`f444ae4`）**: `typl` CLI/REPL/LSP/全テストヘルパで`load_aot`を常時実行、
  「島未ロード」fallbackを実質廃止。
- **Stage 6（`2cfdf53`）**: マクロ展開中の定義時JIT解禁（`jit_suppressed`撤去）。島がAOT済みなら
  展開中JITの再入問題は消えるという想定通りだった。
- **Stage 7（`5c77d40`）**: `TYPELISP_CLOSURE_JIT`環境変数と`JitMode`を削除、JIT中heap枯渇の
  Benign分類を廃止し通常のheap枯渇と同様panic化。
- **Stage 8a（`50a235a`）**: 全経路`load_compiler`→`load_aot`移行。この移行で**native島の隠れ
  バグ2件**が露見（Stages 1-7ではllvm_shim/compile_testがinterpreted島に対して実行されていた
  ため潜在化していた）:
  1. **op-idの61bit切り詰め**——`llvm_op_id`のフルFNVハッシュがタグ付きsexpr Int（下位3bit
     タグ）で上位3bit消失。`llvm_op_id`を`((h<<3)>>3)`に修正。
  2. **f64リテラルの切り詰め**——f64ビットを`(float bits-as-int)`でタグ付きsexpr Intへ渡す
     箇所が同じ理由で下位3bitを失い、2.0が0.0化して除算がinfになる形で発覚。`(float hi lo)`の
     32bit×2分割に変更（`ast_bridge.rs`両呼び出し箇所+島`compile-float`、`build-shl`/`build-or`で
     再構成）。**教訓: op-id/f64ビット/大きなi64リテラルのようなフル64bit値はタグ付きsexpr
     Intで運べない**（大きなi64リテラルは同根の潜在バグとして未修正のまま残存、失敗テスト
     なし）。
  加えてnative島はコンパイルエラーがRust側のcatchable `EvalError`ではなくFFI境界を跨いで
  **abort**するため、`call_graph_edges`に`is_native_lowered_primitive_method`
  （島の`*-native-method?`述語のRust twin）を追加し、未対応メソッド呼び出しを島実行前に
  クリーンエラー化。
- **Stage 8b（`e9737a1`）**: bootstrapを自己ホスト・スナップショット連鎖化——`SOURCE`変更時は
  コミット済み**旧**`.bc`のnative `compile-function`で新`SOURCE`を再コンパイルする
  `scripts/regen-compiler-island.sh`。`install_island_bitcode`に`check_hash`引数追加。
  regenした`.bc`が従来とバイト完全一致することを確認し、連鎖が安定していることを実証。
- **Stage 8c（`2c604a5`、2026-07-19）**: interpクロージャ機構の物理削除——
  `BoxedObj::Closure`/`ClosureBody`/`Capture`/`Interp::closure_bodies`/`Interp::closure_tokens`/
  `make_closure`/`closure_body_count`/`Expr::Apply`のinterpアーム/`adopt_cell`/
  `dead_closure_tokens`/`take_dead_closure_tokens`/interpreted版`compiler::load`を全削除。
  `jit_or_make_closure`は`jit_closure`（フォールバック無し、JIT不能クロージャは即
  `EvalError::Panic`）へリネーム。`CompiledClosure`/`rt_closure_*`/`build-make-closure`は別物
  として存続。削除後、interpクロージャ抜きでもregenがバイト完全一致することを確認——regenに
  interpクロージャが不要であることの実証にもなった。

  **真のブロッカー=マクロ展開時lambda**（発見・解消 2026-07-19）: prelude
  `case`/`do`マクロは展開時に`sexpr-map`へlambdaを渡す。フォールバックが消えたためそのlambdaは
  JIT必須になったが、本体が`sexpr::eq`（case）/`gensym`（do）という非コンパイル可能な操作を
  使っていた。解消:
  - **gensym**→`rt_gensym`シム（`typelisp-rt`、`Heap::gensym()`でカウンタをHeapへ移動し
    interpの`gensym`ビルトインと共有、展開中にinterp/compiled両方のgensymが走っても同名衝突
    しない）。島`compile-call`に`gensym`→`rt_gensym`のリネームを追加。
  - **sexpr eq**→島`compile-assoc`に`sexpr`分岐（`build-icmp-eq`、charと同型、生ハンドル比較
    なのでルート不要）。`is_native_lowered_primitive_method`と、transitive-compileの2箇所の
    フィルタ（`call_graph_edges`と`jit_define_closure`内のassoc_targetsフィルタ、独立した
    2箇所あるので両方に追加が必要）に`sexpr`を追加。
  - **同時発見のSymbol→Sexpr coercionバグ**: checkerの型調整が`Symbol`を`Sexpr`期待位置で
    `Construct{SEXPR_SYM,[val]}`に包んでいた。interpは`rt_sexpr`パススルー（no-op）で無害だった
    が、島は変種5 constructを`compile-construct-sym`（名前文字列→`rt_intern_symbol`）に
    loweringするため、既にSymbol値のtmpを渡すとabortする。これがgensymの結果を`case`が使う
    まさにこの経路で踏まれた。修正はconstruct でラップせず透明にretypeする方式に変更
    （`Symbol`値の実行時表現は既に`RtValue::Sexpr(Value::Symbol)`——`construct_sexpr`の
    SEXPR_SYM armが実証）。`FASL_FORMAT_VERSION`を6→7へbump（キャッシュ済みASTに旧Construct
    ノードが残るため）。

  **case/do解消後、フルスイートで新たに露見した3カテゴリ**（フォールバック撤去で
  「JIT不能クロージャ」が即エラー化したことで初めて全経路のJIT健全性が問われた）:
  1. **整数`/`・`mod`が島未lowering**（`count_tallies`テスト）→`rt_i64_div`/`rt_i64_mod`シム
     （Rust側`checked_div`/`checked_rem`で0除算+`i64::MIN / -1`をどちらも`fatal`化、生i64演算）
     +島`compile-assoc`のint分岐+`int-native-method?`述語+`is_native_lowered_primitive_method`
     に`/`/`mod`を追加。
  2. **module修飾クロージャのSCC解決**（`module_function_as_value`テスト）→SCC機構をroot名から
     `qualified_fn_name`（full `::`結合、`fn_path_from_node_name`で逆変換）へ拡張:
     `CallEdge::node_name`/`resolve_fn_def`（method_key優先→module fn path）/`is_compiled_name`/
     `call_graph_edges`/`compile_scc`のuser_symbol_name・格納key/`ast_bridge.rs`の
     `translate_call`・`translate_fnref`のmangle名を全てqualified化。`jit_define_closure`の
     module修飾拒否ガードを削除。副産物として`tests/compile_test.rs`の
     `translates_a_call_keeping_only_the_paths_local_segment`
     （ローカルセグメントのみへ切り詰める旧仕様の回帰テスト）が新仕様と正しく矛盾するため
     `translates_a_call_mangling_the_paths_full_qualified_segments`へ更新——
     `m::inc`が`tl_inc`ではなく`tl_m::inc`へmangleされることを検証する内容に変更。
  3. **GC圧テスト3件のヒープ容量不足**（`*_survives_gc_pressure`）→定義時JIT自体がAST用cons
     cellを消費するため、旧2〜6セルのヒープでは「JIT中heap枯渇」で即失敗する。二分探索
     ハーネス（一時ファイル、削除済み）で実測した各シェイプの最小通過セル数はlambda単体18、
     `labels`2兄弟SCC=51（両siblingのASTを同時に保持するため単独lambdaより高い）。
     `a_lambda_captured_sexpr_binding_survives_gc_pressure`/
     `an_unnamed_callee_survives_argument_evaluation_under_gc_pressure`は48セル、
     `labels_siblings_mutually_recurse_under_gc_pressure`は64セルへ設定（churnは200のまま）。
     「32768セルでも失敗する」という調査中の一時的な観測は、後述のGCマスクバグが未修正
     だった時点のものであり、修正後は純粋な容量閾値の問題であってdotimes（ループ）の
     有無とも無関係と実証済み。

  **②の調査で連鎖的に発覚した本物のGCバグ**: `an_unnamed_callee_...`テストをJIT可能な
  ヒープサイズにしたところ`rt_cell_get: not a boxed cell`で別クラッシュ。原因は島
  `compute-sexpr-mask`が`kind==2`（tagged Sexpr）しかトレース対象にマスクしておらず、
  **cell化キャプチャ（`kind>=10`、ネスト捕獲で自動cell化された束縛）を非トレース扱い**に
  していたこと。cell slotは`BoxedObj::Cell`への生きたヒープ参照なのでGC markがこれを
  辿れないと、外側の関数がネストlambdaに捕獲された（自動cell化済みの）パラメータを持つ場合、
  GC圧下でcell自体が回収されてダングリング参照になる。修正は
  `(if (eq kind 2) true (>= kind 10))`。読み取り専用キャプチャ（kind 2）は元々マスク済み
  だったため他のテストは無事で、GC圧テストが従来極小ヒープでJIT自体に失敗していたために
  このバグが一度も実行に到達せず、8c以前は発見されなかった。**教訓: フォールバック撤去は
  「JITできるか」だけでなく「JITしたクロージャの実行時GC健全性」も初めて全経路で問う**。

- 検証: `scripts/with-llvm-env.sh scripts/test-serial.sh`全green（45バイナリ、`--lib`含む）。

**standalone `typl <file>`のprelude非可視は仕様、という以下の記述は誤りだった**（2026-07-21に
撤回・修正 — 末尾「モジュール可視性を祖先チェーン方式へ再設計（2026-07-21）」参照）。当時は
「entry fileは`module_segs_for`で必ずファイル名モジュールに包まれ、`resolve_fn`
（`checker.rs:1052-1055`）は非pub root fnをサブモジュールから見せない設計であり、preludeの
`abs`等は非pubなので裸の`.typl`ファイルから呼ぶと`no such function`になるのは意図された可視性
セマンティクスだ」と診断したが、これは「同一モジュールか literal root からの呼び出しかしか
見ない」という場当たり的な2段階チェックの副作用に過ぎず、そもそも多段ネストしたモジュール間の
祖先アクセスも一切考慮していない設計上の欠陥だった。ユーザー指摘を受けて撤廃し、Rust方式の
（祖先モジュールの非pubアイテムは子孫モジュールから常に見える）祖先チェーン解決へ作り直した。

Stage 8cで`jit_closure`/`jit_closure_from_result`（`interp.rs:685`/`700`）は「フォールバック
無し」に確定した——`JitDecline::Gap`もBenign（native tier型 / コンパイラ島未ロード、以前の
5項目のうち残る2つ）も全て`EvalError::Panic`へ変換される。Stage 5で島は全エントリポイントで
常時ロードされるため「島未ロード」は実質到達不能（自前で島ロードを省略した組み込み側でのみ
起こりうる真のエラー）、「native tier型」もcompiled関数のパラメータ/戻り値型になり得ない
ため到達しない。

**残る作業**: `docs/dev/TODO.md`のフォローアップ項目クローズ（本節完了により対応）、
`docs/dev/language-design.md`のfallback記述更新（恒久Benign集合という概念自体が実質消滅した
ことの反映）。

---

## compile機能: TraitCall機構の削除（2026-07-04）

ジェネリック単型化（2026-07-03、上記「ジェネリック単型化とSexpr/RtValue統合Stage 6」参照）で
`Expr::TraitCall`がコンパイル可能ソースから到達不能になったため、compile側の実行時ディスパッチ
機構を全撤去。削除対象: `ast_bridge`の`translate_trait_call`/`collect_trait_call_targets`/
`is_compilable_trait_impl`/`type_id_hash`、`compiler.rs`の`compile-trait-call`/
`compile-trait-dispatch`/`compile-recv-type-id`とディスパッチアーム、ランタイムの
`rt_trait_call_fail`/`rt_struct_type_id_hash`。あわせて一般ADT boxの先頭type-idスロット（trait
ディスパッチ専用の死んだヘッダ）を撤去し、`construct`ノードとboxレイアウトを
`[variant, field...]`に簡素化（`compile-field-get`/`compile-field-set`はもともと
`rt_struct_field_get`経由でこのレイアウトに非依存）。`Expr::TraitCall`ノード自体はチェッカが
ジェネリック定義時本体で生成しインタプリタが内部エラーとしてトラップする診断専用ノードとして
存続するが、`trait_name`/`impls`フィールドと`Checker::trait_impls`はcompile専用だったため撤去し
`{ method, args }`のみに縮小。

---

## compile機能: `Panic`/`MethodRef`/`Quote`対応（2026-07-12）

`ast_bridge.rs`の`Expr::Panic`/`Expr::MethodRef`/`Expr::Quote`アームを実装し、残っていた
`unsupported`3件を解消。

- **`Panic`**: `(panic msg)` -> `(panic msg-form)`。`msg`は常に`Str`型なので`kind`分岐は不要、
  `compiler.rs`の新設`compile-panic`が`msg-form`を`compile-str`同様にタグ付き`Sexpr::Str`へ
  コンパイルし新設`rt_panic`（`typelisp-rt`）へ渡す。`rt_panic`はインタプリタ経路の
  `EvalError::Panic`と同じ`"panic: {msg}"`書式を出力して`abort()`——コンパイル済みコードは
  JIT/AOTネイティブ境界を安全にunwindできないため、`rt_match_fail`と同じ「abortのみ許容」規約
  に従う意図的な仕様差（インタプリタ側は`Result`として回収可能だが、コンパイル済みコードでは
  プロセスごと落ちる）。
- **`MethodRef`**: `+`等のメソッドを値として使う式（`Checker::method_value`）->
  引数を`(assoc type-name method true ...)`呼び出しへそのまま転送する非キャプチャ`lambda`
  （`translate_fnref`の転送ラッパー手法をinstanceメソッドへ一般化、受け手は`compile-assoc`/
  `compile-assoc-user`——ネイティブ演算もユーザー定義メソッドも既存の`recv::method`呼び出し経路を
  再利用するため`compiler.rs`側の変更ゼロ）。ただし転送先の`(type_name, method)`を
  `ast_bridge::collect_calls`（前方宣言収集）が`Expr::Assoc`と同様に辿るよう追加が必要だった
  ——`Expr::FnRef`が転送`call`ラッパーに対して同じ扱いを受けているのと対称。
- **`Quote`**: `(quote datum)` -> `translate_construct`と全く同じ`(construct true false empty
  variant arg-form...)`ワイヤ形状を`QuotedSexpr`から直接合成（`Cons`は再帰）。
  `Nil`/`Int`/`Float`/`Bool`/`Char`/`Str`/`Cons`は既存の`compile-construct-sexpr`がそのまま
  対応、`Sym`/`Bignum`/`Ratio`（コンパイル表現なし）は入れ子内のどこにあっても`unsupported`へ
  伝播（`unsupported`は`Ok`を返す設計のため、再帰呼び出し結果のタグを見て伝播させる必要があった
  ——素朴に`?`へ任せると`(unsupported ...)`タグがデータとして埋め込まれてしまう落とし穴）。
- 副産物として発覚した既存の欠落も同時解消: `char`リテラルが単体で`compile-value`の
  ディスパッチタグに存在しなかった（`(defun f () char #\A)`が常に失敗していた）ため、
  新設`compile-char`/`sexpr-char`（`registry.rs`/`Interp::eval_builtin`）で対応
  （`char->int`のi32結果を`(as i64 ...)`の無償変換でi64化）。JIT呼び出し境界での`char`戻り値
  デコードも同日中に解消: `Interp::call_compiled`の戻り値デコードに`Type::Char`アームを追加
  （生i64コードポイント→`char::from_u32`→`RtValue::Char`、引数側の`*c as i64`の逆）。`char`を
  返すcompiled関数がインタプリタと相互運用可能に。
- テスト: `tests/compile_test.rs`に6件（panic branch/char literal/builtin・ユーザー定義method
  reified/quoted list構築/JIT-interp一致/quoted symbolのclean error）、`ast_bridge.rs`単体
  テストに5件。

---

## compile機能: `Match`のstruct-kind（`defstruct`/`Vector`のboxed-struct表現）scrutinee対応（2026-07-12）

これで`Match`の`unsupported`分岐は解消（sum-ADT box/struct-kind/`Sexpr`の3種すべてcompile時
`match`が可能に）。`translate_match`/`pattern_to_sexpr`の`is-box: bool`を`scrut-kind: i64`
（0=`Sexpr`、1=sum-ADT box、2=boxed struct、`ast_bridge::MATCH_KIND_*`）に一般化し、`pat-ctor`に
`field-kinds`（各フィールドの`struct_field_kind`のリスト、struct-kind時のみ意味を持つ）を追加。
boxed structは単一variant（`new`）なのでタグテストは一切発行せず（`compiler.rs::compile-pattern-test`
の`scrut-kind = 2`分岐）、フィールド抽出だけを行う新設`compile-struct-field`
（`rt_struct_field_get`→`compile-sexpr-field`、`compile-field-get`と同じ経路の再利用）で対応。
sum-ADT boxと異なりboxed structは通常のGC管理ヒープ値なので`compile-match`のGCルート要否判定
（`push-sexpr-root`/`pop-sexpr-root`）も`Sexpr`側と同様に必要（boxのみ不要）——ここは`scrut-kind`
を素朴に真偽反転しただけでは見落としがちな罠だった。フィールドごとの型情報が必要になったのは
今回が初めてで（sum-ADT boxは自身の`variant`だけで全フィールド共通のデコードができたが、
boxed structはフィールドごとにint/float/char/bool/passthroughが異なる）、`Pattern::Ctor`に
`field_types: Vec<Type>`を新設（`Checker::check_ctor_pattern`が`sexpr_fields`と並べて計算、
インタプリタ側は変更なし）。

---

## ファイル↔モジュール対応: `use`のsibling-relative解決（2026-07-13）

「`use`はソースルート相対のみ、兄弟ファイル相対の解決は未対応」を解消。`Loader::ensure_loaded`
が各prefix長でroot-relative候補を試した後にsibling-relative候補（`use`元ファイルのディレクトリを
前置）もフォールバックとして試すよう拡張、`Checker::find_module`に対応するsiblingティアを追加
（新設`file_ns`スタックで、ファイル自身のモジュールパスをネストした`(module ...)`越しにも
安定して保持——`self.ns`をそのまま使うとネストmodule内の`use`が誤ってそのmodule自身の親を
基準にしてしまう）。root-relativeが常に優先されるため既存の解決結果は非破壊。`project.rs`は
`enter_module`/`exit_module`ではなくファイル境界専用の`enter_file_module`/`exit_file_module`を
使用（nested `(module ...)`は従来通り`enter_module`のまま）。テスト: `module_file_test.rs`に
3件追加（bare名前解決・root優先・nested module内でもfile境界基準）。

---

## compile機能: char/stringの`equalp`（ASCII大文字小文字無視）対応（2026-07-13）

`eq`/`equal`（生コードポイントの`icmp`/`rt_str_eq`）と違い`equalp`はcase-foldingするため単一命令に
できない。新設`rt_char_equalp`（生i64コードポイント2つ）/`rt_str_equalp`を`compile-assoc`の
char/string分岐から呼ぶ（`char-native-method?`/`string-native-method?`にも追加）。テスト:
compile_test 2件 + typelisp-rt 2件。

---

## compile機能: `f64`レシーバのメソッド対応（2026-07-13）

算術（`+`/`-`/`*`/`/`/`mod`）と比較（`<`/`<=`/`>`/`>=`/`=`/`/=`/`eq`/`eql`/`equal`/`equalp`）を
対応。compiled `f64`は生bitパターンをi64に埋め込む表現なので、新設ビルトイン`build-fadd`/`fsub`/
`fmul`/`fdiv`/`frem`が各`bitcast` i64↔doubleで挟み、`build-fcmp-*`が比較（`<`等はordered、`/=`は
Rust`!=`に合わせunordered UNE）。`compile-assoc`にf64分岐+`float-native-method?`、
`call_compiled`にf64引数/戻り値マーシャリング（`to_bits`/`from_bits`）、method-target検証除外に
`f64`追加。当時の残課題（transcendental・変換）は後続節「`f64`の超越関数/丸め関数」
「`bignum`/`ratio`のcompile対応」で解消。落とし穴: LLVM `frem`はCの`fmod`呼び出しにlowerされる
ため`fmod`という名の関数compileはJITシンボル解決衝突で無限再帰したが、これは
「既知の制限・意図的に対象外7項目の解消」節（`tl_`シンボルプレフィックス導入）で解消。
テスト: compile_test 3件 + typelisp-rt（既存流用）。

---

## Iterトレイトを持つ全型（Vector/HashTable）のcompile対応（2026-07-13、branch `feature/iter-compile`）

「ジェネリック本体そのもののcompile」（旧・将来課題）を解消。`doiter`/`map`・`filter`・`member`等の
コンビネータ over `Vector<T>`・`HashTable<K,V>`がcompile可能に。ユーザー定義Iter型も、その`next`が
compile可能なプリミティブに落ちる限り専用対応ゼロで通る（トレイトディスパッチは単型化で消える
ため）。実体は2つ: (1) コレクション・プリミティブ層（`vector-op`/`hashtable-op`ノード +
`rt_struct_field_count`/`rt_struct_push_field`/`rt_hashtable_*`群、要素型kindによるタグ/デコード）、
(2) `Interp::compile_function_rec`——`(compile fn)`が呼ぶ単型化インスタンス（`vector::iter <i64>`等、
空白マングル名で名指し不可）を推移的に自動compile。当初「反復とは別問題」として対象外にしていた
`HashTable::get`/`remove`（`Option`返し）も同日中に追加解消——実行時のfound/not-found結果で
`Some`/`None`を組み立てるため`compile-construct-box`をそのまま使えず、新設
`rt_hashtable_contains`/`_get_raw`/`_remove_raw`を`compile-if`と同型のalloca+分岐+merge
（phiビルトインなし）で呼び分け。詳細は[iter-compile-plan.md](iter-compile-plan.md)。

---

## fasl（コンパイル済みモジュール）機構 + `(load)`（2026-07-14）

「LSPの依存キャッシュなし: 診断パスごとにpreludeソース(822行)をparse+型チェックし直す」を
解消——真のコスト要因はprelude再ロードだった。fasl機構を新設し、preludeを一度だけチェックして
`Fasl`（ヒープ非依存のシリアライズ）化、各診断パスは`Fasl::load_into`で新ヒープへ**確保API経由の
再構築**（parse/型チェックなし、値はheap.cons等で作り直すのでポインタ問題なし）。LSPは起動時に
`Rc<Fasl>`を1つ持ち各パスで再利用。ユーザー面には`(load "path")`フォームと`typl compile-module`
サブコマンド、CLI/REPL起動の`prelude::load_cached`（`$TYPL_CACHE_DIR`または`~/.typl/cache`に
キャッシュ）として一般公開。詳細は[[typelisp-fasl-compiled-modules]]、`src/fasl.rs`。

**残**（当時）: 依存*ファイル*（`use`先）自体のパス間キャッシュは未対応——2026-07-15に
「既知の制限・意図的に対象外7項目の解消」節の2番で解消（`ModuleCache`）。

---

## リーダーの真のspan対応（2026-07-14、branch `feature/reader-spans`）

「reader全体の作り替えが必要」として見送っていたが、位置テーブル機構（`cons_locs`/`elem_locs`）
はそのままに`Loc`へ`end_line`/`end_col`（排他的終端）を追加する増分で解消。
`Reader::read_datum_spanned`が全datumの開始〜終了を捕捉（末尾空白/コメントを消費するread関数が
無いため追加演算ゼロ）、`read_list`はcons_locが`(`〜`)`の完全spanに、quote/quasiquote/unquote/
unquote-splicing合成リストにも明示括弧と同等のcons_loc/elem_locを付与（従来は一切位置が
付かなかった）。トップレベル裸アトム（例: 単独の`42`）は`read_all_in_spanned`が`(Value, Loc)`で
返しそのspanを`Checker::check_form_at`の`loc_hint`へ渡すことで初めて位置を持つ。LSP側は
`locate_node`が「包含優先(`start<=cursor<end`)+非包含フォールバック(旧点近似)」の2段探索に、
`loc_to_range`が実spanでrange構築（診断のアンダーラインがフォーム全体に）、hoverにもrangeを
付与。`FASL_FORMAT_VERSION`を2へbump（`Loc`が`DefLocsRepr`/`Typed.loc`両方でserdeシリアライズ
されるため）。テスト: read_test 10件・lsp_locate_test 4件追加、miri(read_test/mem_test) green。

---

## マクロ生成`use` + モジュール越しマクロ呼出 + quoted `::`パス（2026-07-15）

「マクロ展開が生成した`use`はロードされない」を解消。根本原因は2つの複合だった: (1)checkerが
トップレベルのマクロ呼び出しを式として扱い、展開結果の`use`がトップレベル専用の`check_use`に
到達しない、(2)依存ロードの事前走査（`scan_form`）はマクロ展開前に走るため展開結果が映らない
（`defmacro`の登録自体がチェックループの副作用なので、一括の事前走査では原理的に展開不可能）。
対応: `Checker::try_expand_toplevel_macro`を新設し、`check_form_dispatch`が展開結果をトップレベル
形として再ディスパッチ（`use`だけでなく`defun`/`module`等の定義形の生成も可能に）、Loaderの
チェックループがフォームごとに事前展開→展開結果を`scan_form`で走査（依存ロード）→展開済み
フォームをチェック（二重展開なし）。チェック途中の依存ロードは`Checker::suspend_ns_context`で
名前空間を退避（現在ファイルのモジュール配下へのネスト登録を防ぐ——重要な罠だった）。副産物:
式位置の`use`/`module`は「トップレベル専用」の明確なエラーに（従来は紛らわしい
"unbound variable"）。マクロ生成`use`はマクロ定義より後のフォームでのみ機能するが、これは
バグではなく仕様（単一パスチェックの原理的帰結、`language-design.md`§3の`defmacro`仕様に明記）。

同日中に追加解消: モジュール越しのマクロ呼び出し（`resolve_macro_path`を新設、`mod::macro-name`
が式位置・トップレベル双方で解決可能に）、quoted data内の`::`パス（`QuotedSexpr::Path`を新設、
`'(dep::head)`が型チェックを通るように——`FASL_FORMAT_VERSION`を3へbump）。compile（LLVM）側は
Pathも`Sym`/`Bignum`/`Ratio`同様unsupportedのまま。テスト: `tests/macro_use_test.rs`。

---

## `bignum`/`ratio`型のcompile対応（2026-07-15）

「`f64`の超越関数/丸め関数」節の残課題（`float->bignum`/`float->ratio`のみ）が実は氷山の一角で、
`bignum`/`ratio`型そのものにcompiled表現が一切無い状態だったのを解消。設計: `bignum`/`ratio`は
`num_bigint::BigInt`/`num_rational::BigRational`という可変長Rust構造体で固定bit幅のFFI安全な
レイアウトを持たないため、`f64`のような「ネイティブ表現+box化」の二重化はできず、`Type::Str`と
同じ「常にタグ付き`TAG_BOXED`ポインタ」規約に統一。`crates/typelisp-rt`に`rt_bignum_new`/
`rt_ratio_from_bignums`（構築）・`rt_bignum_add/sub/mul/div/mod`/`rt_ratio_add/sub/mul/div`
（二項演算）・`rt_bignum_cmp`/`rt_ratio_cmp`（3-way比較、`str-lt-call`と同じ「1プリミティブから
6比較を派生」手法）・変換一式（`rt_int_to_bignum`/`rt_int_to_ratio`/`rt_float_to_bignum`/
`rt_float_to_ratio`/`rt_bignum_to_int`/`rt_bignum_fits_i32`+`rt_bignum_to_int_raw`（`try-bignum
->int`用Option二段呼び出し）/`rt_bignum_to_float`/`rt_bignum_to_ratio`/`rt_ratio_to_bignum`/
`rt_ratio_to_float`/`rt_ratio_numerator`/`rt_ratio_denominator`）を新設（`num-bigint`/
`num-rational`/`num-traits`を直接依存に追加）。ゼロ除算は`BigInt`/`BigRational`のDiv実装が
生Rust panicを起こすため、`extern "C"`境界越えUBを避けて演算前に明示チェック→`fatal`。
`ast_bridge.rs`側: `struct_field_kind`/`binding_kind`に`Type::Bignum`/`Type::Ratio`を`Str`と同じ
扱いで追加（後者はGC安全性の必須修正——ヒープ参照なのにルート登録されないバグになるところ
だった）、`Expr::Bignum`/`Expr::Ratio`・`QuotedSexpr::Bignum`/`Ratio`を`unsupported`から実リテラル
構築（`bignum_literal_form`/`ratio_literal_form`、`str_literal_form`と同じ「符号+桁を`(int _)`列
として埋め込み、`rt_bignum_new`呼び出し」方式）に変更。`compiler.rs`側: `compile-bignum-literal`/
`compile-ratio-literal`、`bignum-native-method?`/`ratio-native-method?`、`compile-assoc`への
分岐追加、`compile-sexpr-field`/`compile-construct-sexpr`のvariant 8/9を「panic」から`str`と同じ
passthroughに変更。`interp.rs`側: `call_compiled`の引数エンコード（`RtValue::Bignum`/`Ratio`を
都度ヒープへclone+root、`Str`と同型）と戻り値デコード（`Type::Bignum`/`Ratio`は`Type::Named`で
ないため`is_boxed_sexpr_type`に掛からず、専用分岐が必須だった）、`rt_extern_functions`への
26関数追加（JIT `add_global_mapping`用、AOTも同じ関数を再利用）、`compile_function`の
「ネイティブ受け皿型」除外リストに`bignum`/`ratio`追加。テスト: `compile_test`に13件追加
（リテラル・四則演算・比較・全変換・`try-bignum->int`のSome/None両方、bignum/ratio双方）。

落とし穴: 自己ホストコンパイラ本体（`compiler.rs`の`SOURCE`文字列）は巨大なS式で、括弧の数え
間違いが「離れた場所の`if`アリティ不整合」として現れデバッグが難航——コメント/文字列/`#\c`文字
リテラルを正しく読み飛ばす簡易パーサをその場で書いて特定した。既存テスト2件（`float->bignum`/
`sqrt`をnon-native `f64`メソッドの例に使っていたもの）は本対応で前提が崩れたため
`i64::int->char`（今も非ネイティブ）に差し替え。

---

## `f64`の超越関数/丸め関数 + `float->int`のcompile対応（2026-07-15）

「`f64`レシーバのメソッド対応」節で残としていたtranscendental（`sqrt`/`floor`/`ceiling`/`round`/
`truncate`/`expt`）と`float->int`を解消。算術と同じく「compiled `f64`は生bitパターンを`i64`に
埋め込む表現」を維持したまま、各操作をLLVM組み込み関数（`llvm.sqrt.f64`等、`expt`は
`llvm.pow.f64`）にlowering——新設ビルトイン`build-fsqrt`/`build-ffloor`/`build-fceil`/
`build-fround`/`build-ftrunc`/`build-fpow`が`Intrinsic::get_declaration`でモジュールへ宣言
（同一モジュール内の再呼び出しに対して冪等なため`get-function`のような事前存在チェック不要）
した上で`bitcast`+`build-call`+`bitcast`を行う（`build-fadd`等と同じ「i64-in/i64-out」規約）。
`float->int`は`build-fptosi`——単一の`fptosi`命令のみ、ヒープ確保・モジュール引数とも不要。
`compile-assoc`のf64分岐は先に単項（`a`のみ必要）を判定してから二項（`b2`も必要）へフォール
スルーする構造に組み替え——元の構造は無条件に`b2`も評価していたため、単項メソッドをそのまま
追加すると存在しない2引数目を読もうとして失敗する。残る未対応だった`float->bignum`/
`float->ratio`は同日中に前節「`bignum`/`ratio`型のcompile対応」で解消。

落とし穴: LLVMの`fptosi`はNaN/範囲外入力に対し値未定義（poison）——インタプリタ側`float_to_int`
（Rustの`as i64`、飽和的キャスト）とはその境界ケースのみ挙動が発散する。この差は同日中に
「既知の制限・意図的に対象外7項目の解消」節7番（`llvm.fptosi.sat`intrinsicへの切替え）で解消。

テスト: `compile_test`に4件追加（当時）（transcendental 5種一括+expt+float->int+既存2件の対象
差し替え`sqrt`→`float->bignum`——sqrt自体がcompile可能になったため）。

---

## 「既知の制限・意図的に対象外」7項目の解消（2026-07-15、branch `feature/known-limitations`）

これまで「既知の制限」「対象外」として個別に記載していた項目のうち、ユーザーの指示
（「対象外はほぼAIが勝手に決めたもの、客観的に実装可能性を判断せよ」）を受けて再調査した結果、
文書上ユーザー自身の判断と確認できたのは**`Sexpr`への`Iter`トレイト実装のみ**
（`language-design.md`に明記、[[typelisp-sexpr-hashtable-iter]]参照）で、残り7件は技術的に
実装可能と判明し全て解消した（規模の大きい順、コミット1件=1項目）。

1. **値レベル`&rest`/`apply`の再導入**——`Type::Fn`の第2フィールド（rest要素型）と`FnSig.rest`を
   復元、`(apply f a1..aN rest-list)`特殊形を再実装。固定引数は静的型検査、rest-listは`Sexpr`型で
   渡し`wrap_rest_elem`/`cons_rest_list`で単一リストへ畳む。`FASL_FORMAT_VERSION`を4へbump。
   唯一実在したギャップは、`&rest`関数を**第一級の値として参照する**（`Expr::FnRef`化される）
   場合のみ: `ast_bridge.rs`の`translate_fnref`が合成する転送用クロージャがrestパラメータを一切
   宣言・転送しないまま固定引数のみで組み立てられていた——compileはエラーにならず成功するが、
   実行時に空/未ルートの`xs`を読むため誤った値を返すか、GCが介入するとnullポインタ参照で
   クラッシュする「静かな不正確さ」だった。2026-07-16解消（`translate_fnref`の合成パラメータ
   リストに、`ty`の`rest`フィールドから`check_defun`/`check_lambda`と同じ`(rest名, Sexpr型)`を
   追加、回帰テスト`fnref_of_a_variadic_function_forwards_the_rest_list`で検証）。
   `translate_methodref`にも見た目上同じロジックがあるが、`defmethod`構文に`&rest`が存在しない
   （`MethodSig`に`rest`フィールドが無い）ため到達不可能と確認済み、コード変更は不要だった。
   （注: `&rest`付き`defun`自体の直接compile・`apply`経由の呼び出しは元から動作しており
   `unsupported`ではなかった——`Checker::check_defun`が`&rest`引数を`(rest名, Sexpr型)`として
   `params`末尾へ折り込む脱糖段階で、`Interp::compiled_fn_body`が読む名前・型のペア数は最初から
   一致しているため。`tests/compile_test.rs`の
   `a_variadic_function_compiles_and_dispatches_to_native_code`が元から証明済み。）
2. **依存ファイル（`use`先）のfaslインメモリキャッシュ**——「fasl機構+`(load)`」節（2026-07-14）で
   唯一残っていた「依存*ファイル*自体のパス間キャッシュ」を解消。`Loader`に`ModuleCache`
   （`Rc<RefCell<HashMap<PathBuf, Entry>>>`、`Entry`はdeps（各依存ファイルのパス+内容ハッシュ）と
   `Rc<Fasl>`）を追加、`try_load_module`がキャッシュ照合→ヒット時は`Fasl::load_into`で新ヒープへ
   再構築（deps全ファイルの内容ハッシュ再検証込み）。LSP起動時に`ModuleCache`を1つ持ち各診断/
   補完パスで共有。
3. **ネストしたジェネリック呼び出しの`where`境界検証**——`validate_where_bounds`の2つの穴を解消。
   (1) `Vector<U>`等に包んで転送する際の誤拒否（pin比較で開いた型変数を含む場合はスキップし
   単型化時の再検証に委譲）、(2) 裸の型変数がboundなしで外側関数へ転送されるケースの見逃し
   （`caller_bounds`を新設して外側のwhere節に一致するboundが宣言済みか照合）。
4. **`Option`/`Result`型グローバルのcompile参照**——`RtValue::Data` ⇔ compiled sum-ADTボックス
   （malloc配列`[variant, fields...]`）の相互変換（`data_to_box`/`decode_data_value`）を新設し、
   JIT/AOT双方から`Option`/`Result`型`defvar`の読み取り・書き込みが可能に。ユーザー定義`defenum`
   型グローバルは2026-07-16に追加解消（`TopLevel::Defenum`へのvariantフィールド型焼き込み+
   `Interp::enum_defs`、[[typelisp-defenum-global-compile]]）。`data_to_box`/`decode_data_value`
   という変換自体も同日中の「enum値のGCヒープ表現統一」（branch `feature/enum-heap-unification`、
   [[typelisp-enum-heap-unification]]）で全廃: enum値（`Option`/`Result`/ユーザー`defenum`）を
   defstructと同じGCヒープオブジェクト（`BoxedObj::Enum`、`rt_data_new`/`rt_data_variant`/
   `rt_data_field`）に統一し、interp/compiled両側が同じ表現を共有するようになった。旧
   `data_to_box`/`decode_data_value`/kind=10特殊経路は全廃、`global_field_kind`は他の値同様
   `kind=6`（struct_field_kind一本化）。ただしLLVMハンドル等ヒープ非対応型でインスタンス化された
   enum（`Option<llvm-value>`——自己ホストコンパイラ本体が多用）は`RtValue::Data`のnativeフォール
   バックとして存続（`Scope<V>`と同型の二重表現、`Checker::is_heap_repr`/
   `Interp::enum_fields_representable`が再帰的に判定）。これにより「enum値のネストしたOption等
   フィールドのencode不可」「enum引数/戻り値のcall_compiled非対応」の2つの残課題も解消。
5. **quoted data内の`Symbol`/`Path`のcompile対応**——`'foo`/`'(a b c)`/`'dep::head`がcompileを
   通るように。`rt_intern_symbol`/`rt_intern_path`を新設、`str`リテラルと同じ「タグ付き`Sexpr`を
   rt呼び出しで構築」方式。
6. **ユーザー定義関数のLLVMシンボルに`tl_`プレフィックス**——LLVMの`frem`命令がlibmの`fmod`
   シンボル呼び出しへlowerされるため、`fmod`という名のユーザー関数をcompileすると無限再帰して
   いた（旧「許容された既知のギャップ」）。`USER_SYMBOL_PREFIX = "tl_"`を導入し、通常呼び出し・
   関数値化・メソッド呼び出し・JIT/AOT双方のシンボル解決すべてに一貫適用して解消。
   `sexpr-car`/`sexpr-cdr`/`sexpr-cons`は`rt_car`/`rt_cdr`/`rt_cons`へ直接書き換わる
   コンパイラ組み込み経路のため意図的にプレフィックス対象外。
7. **`float->int`のLLVM `fptosi`飽和化**——素の`fptosi`命令はNaN/範囲外入力でpoison値になり
   インタプリタの`float_to_int`（Rustの`as i64`、飽和キャスト）と食い違っていた（旧「許容された
   既知のギャップ」）。`llvm.fptosi.sat`intrinsic（`i64`/`f64`でオーバーロード）へ切替え、
   NaN→0・+inf/オーバーフロー→`i64::MAX`・-inf/アンダーフロー→`i64::MIN`をインタプリタと
   一致させた。

---

## defenum型グローバルのcompile対応 + enum値のGCヒープ表現統一（2026-07-16）

上記「既知の制限・意図的に対象外7項目の解消」節4番の追記そのもの——branch
`feature/defenum-global-compile`と`feature/enum-heap-unification`（5 stage）で、
`TopLevel::Defenum`へのvariant型焼き込み+`Interp::enum_defs`+`subst_apply`（`FASL_FORMAT_VERSION`
を5へbump）、続けてenum値（`Option`/`Result`/ユーザー`defenum`）をdefstructと同じGCヒープ
オブジェクト（`BoxedObj::Enum`）に統一。詳細は上記4番および[[typelisp-defenum-global-compile]]/
[[typelisp-enum-heap-unification]]を参照。

---

## モジュール可視性を祖先チェーン方式へ再設計（2026-07-21）

`typl <file>`（`typelisp.toml`なしの単体ファイル実行）でprelude関数（`not`等、全て非pub）や
builtinのassoc関数（`Option::some`等）が「no such function」/「unresolved path」で呼べない、
という報告を再現・調査したところ、当初は「entry fileが`module_segs_for`でファイル名モジュールに
包まれ、preludeはrootに非pubで登録されるため、`resolve_fn`の`sig.public || self.ns.is_empty()`
判定に弾かれる、意図された可視性仕様」と診断した（上記の訂正済み旧記述）。ユーザー指摘により、
この2段階（「同一モジュールか」「literal rootからの呼び出しか」）チェック自体が場当たり的で、
多段ネストしたモジュール間の祖先アクセス（`(module a (module b ...))`で`b`から`a`の非pub
アイテムを見る、というRustと同じ意味論）を一切考慮していない設計上の欠陥だと判明し、全面的に
作り直した。

- **新設計**: `Checker::ns_ancestors`（`self.ns`から1段ずつ`pop`しrootまでの祖先チェーンを
  返すイテレータ）と`Checker::in_scope`（対象アイテムの所属モジュールが現在の名前空間自身か
  祖先かを判定、旧`same_module`の一般化）。素の名前解決系（`resolve_fn`/`resolve_macro`/
  `resolve_ctor`/`resolve_global`/型名解決の`resolve_bare_type`——`resolve_type_name`と
  `resolve_type_path`のモジュール修飾なしケースで共有）は`ns_ancestors()`を歩く形に、修飾
  パス解決系（`resolve_fn_path`/`resolve_macro_path`/`resolve_type_path`/`resolve_global_path`）
  と`assoc_visible`（メソッド/assoc関数、defstructフィールドアクセサ含む）は`same_module`を
  `in_scope`に置換。既存の`resolve_trait_name`（`ns.pop()`ループでrootまで歩く実装）が既に
  この方式の先例だった。
- **副産物のバグ修正**: `resolve_type_path`は`mods`が空（モジュール修飾なしの`Type::member`、
  例: `Option::some`）のとき`find_module(&[])`が常に「現在の名前空間自身」を返してしまい
  （空パスなら`Namespace::module`のループが回らずSome(self)）、`pub`判定に到達する前に
  rootの型テーブルへ到達する経路自体が無かった。`mods.is_empty()`のケースを`resolve_bare_type`
  へ委譲することで解消（型注釈としての`Option<i32>`と`Option::some`の`Option`部分が同じ
  ロジックで解決されるようになった）。
- **LSP補完** (`src/check/locate.rs`の`completion_candidates`/`push_namespace`) も同じ
  「現在の名前空間 + literal rootのみpublic_only」という2段パターンを実装しており同根の問題を
  抱えていたため、同じ祖先チェーン方式へ揃えた（`public_only`引数は不要になり削除）。
- **格納構造は変更していない**——`Registry`/`Namespace`の木構造や`Interp::fns: HashMap<Path,
  FnDef>`という実行時ディスパッチテーブル自体は元々「フラットな1テーブル」ではなく（前者は
  ツリー、後者はcheck時点で完全修飾済みPathへ解決された後のO(1)実行用インデックスで可視性判定
  とは無関係のレイヤー）、直したのは解決アルゴリズム（探索ロジック）の側だけ。
- 回帰確認: 兄弟モジュール間の非pubプライバシー（`tests/namespace_test.rs`の既存テスト群、
  `tests/macro_use_test.rs::cross_module_macro_call_respects_visibility`）は引き続き拒否される
  ことを確認済み。`tests/module_file_test.rs`に新規テスト5件
  （`bare_prelude_fn_is_reachable_from_a_wrapped_entry_file`ほか、祖先アクセスの新規許可・
  兄弟間拒否の回帰確認を含む）、`tests/lsp_completion_test.rs`の
  `a_private_root_function_is_not_visible_from_inside_a_submodule`は新仕様に合わせて
  `a_private_root_function_is_visible_from_inside_a_submodule`へ改名・反転。
- 検証: `scripts/test-serial.sh`（`--lib`+全integration testバイナリ、`--test-threads=1`）
  全green（`ALL TESTS PASSED (serial)`）。

---

## Interpのフラットテーブルをモジュールスコープツリーへ全面移行（2026-07-21）

上記の可視性再設計はチェッカー（静的解決）側の話だったが、実行時（`Interp`）側は依然として
チェッカーが解決した完全修飾`Path`をキーとする単一のフラット`HashMap`
（`fns`/`methods`/`globals`/`compiled`/`compiled_methods`/`struct_types`/`enum_defs`）に
フラット化されていた。branch `feature/scope-tree-interp`でこれを廃し、チェッカーの
`Namespace`木を模した実行時`ModuleScope`ツリー（`src/eval/scope.rs`新設、335行）へ置換。

- `Expr::Call`/`Global`/`FnRef`/`SetGlobal`は`Ref { written, home, resolved }`を持つ形へ変更
  （`FASL_FORMAT_VERSION`を7→9へbump、`Expr`/`TopLevel`の形状変更2回分）。`Interp`自身は
  `resolved`（チェック時点のキャッシュ）を無条件には信用せず、`written`+`home`から祖先チェーン
  探索/直接descentを実行時に独立して再実行する（祖先チェーン探索が失敗した場合のみ`resolved`へ
  フォールバック——`use`エイリアスなど実行時ツリー上では再解決不能なケース向け）。
- `(compile name)`/`(compile type::method)`もこの`Ref`機構に統一。旧実装は「builtinの`Call`に
  文字列を渡す」という偽装をしており、かつモジュールを無視して型のローカル名だけで木全体を
  線形探索していたため、呼び出し元自身のモジュールに定義された関数へのbare名参照が
  （root にしか無いという理由で）そもそも解決できないバグがあった。専用ノード
  `Expr::CompileFn(CompileTarget)`を新設し、checker側の解決結果
  （`resolve_fn`/`resolve_fn_path`/`resolve_bare_type`+assoc存在確認）をそのまま運ぶ形に修正。
- 検証: 全1255テストgreen（51バイナリ）。

続けて同日中に、この移行に伴う重複コードを整理（branch `feature/scope-tree-interp`内、
挙動変更なし・全1256テストgreen）:

- `qualified_fn_name(p)`ヘルパーを削除——`Path::Display`の`.to_string()`と完全に重複していたため
  6箇所すべてを`.to_string()`へ置換。
- `CallEdge::Method`のノード名生成/`compile_function`の`Method`分岐/エラーメッセージ2箇所が
  各々`format!("{}::{}", type_name, method)`を独自に組み立てていたのを、既存の共有ヘルパー
  `method_link_name`へ統一。
- `Checker::check_compile`: 単純名解決（`resolve_fn`）とtype::methodでなかった場合の修飾名
  フォールバック（`resolve_fn_path`）が「ジェネリックチェック+プレースホルダ`Path`+`mk_ref`」と
  いう同一パターンをほぼ複製していたのを共通クロージャ`fn_target`へ統合
  （`Path::root(name)`と`Path::from_segments(vec![name])`が等価であることを確認した上で統一）。

---

## `Vector<T>::pop`実装 + `functions.md`の記載整理（2026-07-22）

`docs/functions.md`のVector<T>節に残っていた「`pop`/`map`/`filter`/リスト変換などは未実装」という
注記を精査したところ、`map`/`filter`は既にPhase 6.5の`Iter`ジェネリック関数として実装済み（注記が
古いだけ）、`pop`とリスト変換だけが実際に未実装と判明。リスト変換（`Vector<T>`⇔`Sexpr`リスト）は
言語仕様上不可能——`cons`は`cons<T,U>`という異種ペア型で`(cons 1 "hello")`の型は
`cons<i32, cons<str, null>>`（1つめと2つめの要素の型が異なる）であり、`Sexpr`のリストはこの異種
入れ子`cons`連鎖である一方`Vector<T>`は単一要素型`T`のみのコレクションなので、汎用変換関数は
型パラメータでは表現できない——という理由を`functions.md`に明記した上で対象外を確定。

`pop`はHashTable::get/removeと同じ「空/未検出はNone、範囲外ではpanicしない」設計
（`Option<T>`返し）で新規実装。`Vec::pop`のRust標準シグネチャとも一致し、旧（2026-06-23に全面
削除済みの）`Vector<T>`初回実装が残していた`pop(Vector<t>)->Option<t>`という仕様（当時のcatalog
記載）とも合致する。

- **mem層**: `Heap::struct_pop_field`（`crates/typelisp-mem/src/heap.rs`）— 最後のフィールドを
  popし`Option<Value>`で返す（空は`None`、Structでない場合のみpanic）。
- **rt層**: `rt_struct_pop_field`（`crates/typelisp-rt/src/lib.rs`）— コンパイル済みコード用、
  空で呼ばれたら`fatal`（呼び出し側が`rt_struct_field_count`で事前チェック済みという前提、
  `rt_hashtable_get_raw`/`_remove_raw`と同じ規約）。
- **checker**: `registry.rs`の`vector_def`に`pop: Vector<t> -> Option<t>`を追加。
- **interp**: `vector_pop`が`hashtable_remove`と同型（`option_payload_ty`+`option_value`で
  `Option`値を構築）。
- **compile（JIT/AOT）**: `ast_bridge.rs`の`translate_vector_method`が`pop`のときだけ
  `option-type-name-form`（`(str "option")`）を`vector-op`ノードの末尾に追加
  （`HashTable::get`/`remove`の`option_type_name_form`と同じ役割）。`compiler.rs`の
  `compile-vector-op`に`pop`分岐を新設——`rt_struct_field_count`で空判定→空なら`None`を
  `rt_data_new`で構築、非空なら`rt_struct_pop_field`で取り出した値（既にタグ付き`Sexpr`、
  `push`が書き込んだ表現そのもの）をそのまま`Some`へ、`compile-hashtable-op`の`get`/`remove`と
  全く同じ「alloca+分岐+merge」パターンで実装。
- **自己ホストcompiler.rsの括弧デバッグ**: 深くネストした`(if ...)`の中間にケースを1つ挿入する
  際、閉じ括弧の数を手で数え違えて`if`が正しい引数数（cond/then/else）を持たなくなるヒーゼンバグを
  誘発した——単純な深さ集計（開き括弧-閉じ括弧の総数が0に戻るか）だけでは検出できず（合計は
  合っていても木の形が壊れうる）、実際にRust側で簡易S式パーサを書いて`(if ...)`ノードの子要素数を
  検証して特定。`scripts/regen-compiler-island.sh`（`compiler.rs`のSOURCEを変更したら必須、
  さもないと`island bitcode is stale`でテストがpanicする）も要実行。
- テスト: `tests/vector_test.rs`に3件（`Some`/`None`両方、`len`減少）、`tests/compile_test.rs`に
  3件（JIT経由、空/非空、`len`減少）追加。`typelisp-rt`単体テストにも`rt_struct_pop_field`の
  round-trip 1件追加。`scripts/test-serial.sh`全体green。

## `--heap-cells N`（cons アリーナ容量指定オプション）（2026-07-23、旧 TODO T5）

`typl --heap-cells N`（`--heap-cells=N` 形も可）で cons 固定アリーナの容量（既定 65536）を
起動時に指定できるようにした。`main.rs` の `parse_heap_cells` が全 run モード（`run`/REPL/
`compile-module`）共通のグローバルフラグとして先頭でパースし、各経路の `Heap::with_capacity`
へ渡す。不正値・0・値なしはロード前に `exit 1` で弾く。`tests/heap_cells_test.rs`（引数解析の
out-of-process テスト）、docs は [language-design.md](language-design.md) §2 / [syntax.md](../syntax.md) を更新。

## `format` の書式指定子 + `print`/`println` の書式指定統一（2026-07-23、旧 TODO T1）

CL 準拠 `format` と、書式ディレクティブを解釈する `print`/`println` を実装。詳細は
[functions.md](../functions.md) §15 を参照。

- **API**: `(format dest control args...)`（`dest`: `true`=CL の `t` で標準出力+文字列返し／`false`=CL の
  `nil` で文字列返しのみ）、`(print control args...)`／`(println control args...)`。旧来の単一値
  `princ` メソッド（`(println x)`）は廃止し、**第1引数を制御文字列とする書式指定に統一**した
  （ユーザ選択「常に書式(format委譲)」）。既存の examples/projects は全て新形式へ書き換え。
- **ディレクティブは CL をほぼ網羅**（同日、最小サブセット `~a ~s ~d ~% ~~` から全面拡張）:
  `~a ~s ~w`／`~d ~b ~o ~x ~r`（英語基数・序数・ローマ数字）／`~p ~c`／`~f ~e ~g ~$`／
  `~% ~& ~| ~~ ~t ~<改行>`／制御構造 `~(~) ~[~;~] ~{~}~^ ~<~;~> ~? ~*`。プレフィックス
  パラメータ（整数/`'c`/`v`/`#`）と `:`/`@` 修飾子も対応。`~d` 等の非整数引数は `~a` 相当で表示
  （CL準拠。最初の版はエラーにしていたのを変更）。未対応は `~/name/`（実行時関数解決機構が format の
  呼出規約に合わない）と pretty-printer 系 `~i`/`~_`（no-op）のみ→ pretty printer 本体は
  [TODO.md](TODO.md) の T6 として別タスク化。
- **実装3層**: (1) 可変長引数を各自の型のまま `Sexpr` へ包んでリスト化する特殊形
  `Checker::check_format`/`check_print_like`（`check_list_lit` と同系統。`&rest` は単一要素型
  なので使えず、`cons_hetero_sexpr` が各要素を `wrap_rest_elem`/`sexpr_ctor_for` で包む。対象は
  i32/i64/f64/bignum/ratio/char/bool/string/Sexpr、それ以外は型エラー）、(2) 書式エンジン専用
  モジュール [src/eval/format.rs](../../src/eval/format.rs)（制御文字列を `Node` 木にパース——block 系
  `~[ ~{ ~< ~(` の入れ子と clause 分割 `~;` を再帰下降で処理——→引数を `Vec<Value>` 化して `~*` 等の
  カーソル移動に対応→`State` が解釈。値描画 `render_value` は GCヒープ走査+enum 変種名解決を要する
  Rust 専用処理で `main.rs` の `format_sexpr`（REPL echo）の姉妹、standard/aesthetic フラグで
  文字列/文字のクォート有無を切替）、(3) 内部ビルトイン `format-rt`/`print-rt`/`println-rt`
  （`eval_builtin`。synthetic `Ref` 経由でディスパッチ）。`Interp::run_format` は enum 変種名表
  （`collect_struct_and_enum_types`）を渡す薄いラッパ。
- `compile` 対象外（旧 `print`/`println` も非対応だった）。`tests/format_test.rs` 37件。
  `is_builtin_form_head` にも format/print/println を追加。`scripts/test-serial.sh` 全体green。

## `defmacro` の `&optional`/`&key` 対応（2026-07-24、旧 TODO T2）

`defmacro` のラムダリストを CL 流の構造化ラムダリストへ拡張。従来は `&rest` のみ対応だったが、
`&optional`（デフォルト値付き省略可能引数）と `&key`（キーワード引数）を追加。順序は
`必須 &optional opt... &rest r &key key...`（各マーカー高々1回・この順序でのみ）。

- **構文**: `&optional`/`&key` の項は `name` または `(name デフォルト式)`。デフォルト式は
  **展開時に評価**され（`bind_macro_args`）、CL 同様に**先に束縛済みのパラメータを参照できる**
  （`(defmacro dup (x &optional (y x)) ...)` が動く）。デフォルトを書かなければ `()`=`Sexpr::Nil`。
  `&key` は呼び出し側 `:name 値`（順不同）。キーワードはシンボル名が `:` で始まる素のシンボル
  （typelisp に専用キーワード型は無く、リーダは `:b` を名前 `:b` のシンボルとして読む）。
- **表現**: 共有型 [`MacroLambda`](../../src/check/checker.rs)（`required`/`optionals: Vec<Vec<Typed>>`/
  `keys: Vec<(String, Vec<Typed>)>`）を新設し `TopLevel::Defmacro` と `FnDef` に持たせた。`params`
  は従来通り全束縛名を順に並べたフラット列（`apply` 用）で、`MacroLambda` は必須以降の各領域の
  埋め方（デフォルト式・キーワード名）だけを足す。`FnDef.rest: bool` は据え置き（`&rest` 有無）。
- **アリティ検査の分担**: 呼び出し側チェッカ（`Checker::check_macro_arity`、`MacroShape` 経由）は
  生の引数**個数**で判る範囲だけ検査（最小=`required`、`&rest`/`&key` 無しなら最大=`required+optional`。
  `optional==0` の素マクロは従来通り「厳密 N 個」メッセージ）。`:key` 個別検査（未知キーワード・
  奇数個の `:key` 列）は引数フォームのパースが要るので `Interp::expand_macro`/`bind_macro_args` に委譲。
- **GC**: `bind_macro_args` は各パラメータ値を生成した端から `Slot::Heap` セル（`alloc_cell` の
  暗黙ルート）に入れて保護しつつ、そのセル列をデフォルト式評価用の環境として再利用する
  （後続デフォルトの `cons` による GC から先行束縛を守る）。`apply` のセル確保は GC を誘発しない
  ので、`env` 破棄～`apply` 消費の隙間でも `argv` の生ポインタは有効。
- **MacroDef**（`check::registry`）は `arity`/`rest` → `required`/`optional`/`rest`/`keys` に変更、
  FASL は v10→**v11** へ bump。`tests/macro_test.rs` に §Phase F（11件）追加。LSP の `locate` は
  デフォルト式（`Typed`）も走査対象に含めた。

## 動的ディスパッチ（trait オブジェクト `:dyn Trait`、2026-07-25、旧 TODO T4）

C++ の vtbl 方式による動的ディスパッチを、インタプリタ／JIT／AOT の3経路すべてで実装した。
言語仕様としての説明は [language-design.md](language-design.md) §5.2、構文は
[syntax.md](../syntax.md) §1・§2。ここには**設計判断の経緯**だけを残す。

### なぜ vptr をオブジェクトでなく dyn 値の側に持つのか

C++ は vptr をオブジェクト先頭に埋め込むが、typelisp では **fat box**
`BoxedObj::Dyn { vtable_id, value }` を選び、vtable を **(具象型, trait) の組ごとに1本**とした。
箱詰め地点では具象型も trait も静的に分かっているので組を選べる——結果、呼び出し側は
「定数スロットを添字して間接呼び出し」という C++ とまったく同じ形になる。

オブジェクト側に持たせなかった理由:

1. **1つの型が複数 trait を実装できる**ので単一 vptr では足りない。`型ID → trait → vtable` の
   2段引き（実質 Go/Java の itable）になり、呼び出しが O(1) の添字でなくなる。
2. `BoxedObj::Struct`/`Enum`・`rt_struct_new`/`rt_data_new`・GC・`match` の実行時型テストを
   一切変更せずに済む。`:dyn` を書かないコードの表現もコストも変わらない。

代償は「箱詰めのたびに1確保」だが、これは明示的または期待型駆動の変換地点でしか起きない。

### なぜ `Expr::TraitCall` を復活させず新ノードにしたか

`Expr::TraitCall` はかつて実行時ディスパッチノードで、型IDハッシュの n分岐チェーンを引いていた
（2026-07-01実装 → 2026-07-04削除、本ログ参照）。単型化導入後は診断専用に降格している。
今回そこへ動的ディスパッチを戻すと、削除したばかりの「実装型を全部列挙して型名で分岐する」方式に
逆戻りしかねないため、**別ノード `Expr::DynCall`** を新設した。TraitCall は診断専用のまま。
実際、vtable 方式は旧方式の既知の制限（ジェネリック関数の check 後に追加された `impl` が
コンパイル済み呼び出しから見えない）を原理的に持たない。

### なぜ vtable_id を AST に焼かないか

`Expr::DynBox` が持つのは vtable の**中身**（`concrete_key` / `trait_path` / `slots`）で、数値 id は
`Interp::vtable_id_for` が exec／翻訳時に intern する。id は `Interp` ごとの通し番号なので、
別プロセスで採番された id が fasl 経由で持ち込まれると無意味になるため。

### スロット順の権威

`TraitDef::methods` は `HashMap` で反復順が不定なので、コンパイル済みコードが定数で添字する表の
レイアウトには使えない。`deftrait` の記述順を `TraitDef::method_order` として別に持ち、これを
唯一の権威とした（重複メソッド名は `deftrait` 時にエラー）。

### `:dyn Trait` を2語にしたことの波及

表記はユーザー判断（エディタで確実に強調表示できること）。空白区切りの2語なので、リーダを2箇所で
特別扱いする必要があった:

- **datum 位置**: `:dyn X` を `(:dyn X)` に畳む（quote と同じ `read_wrapped_body` を再利用）。
  これで引数ペア・戻り値型・`defstruct` フィールド等の「型は1 Value」という既存前提を崩さずに済み、
  型位置の個別パーサを1つも触らずに済んだ。
- **型引数の中**: `Vector<:dyn Drawable>` には datum 境界が無い（`Vector<...>` はシンボル1個の
  名前文字列を `NameLexer` にかけて解釈する構造）。`read_atom` に `<>` 深度追跡を入れ、閉じるまで
  空白込みで読むようにした。**投機的で、閉じなければ完全に巻き戻す**——この巻き戻しが、
  `(< a b)` / `(string< a b)` / `(a<b c)` といった既存の綴りを1つも列挙せずに後方互換を保つ鍵。
  改行・括弧・文字列・コメントに当たった時点で中断する。

副産物として CL 相当のキーワード機構（`:foo` は自己評価する interned シンボル、`keywordp`）を
導入した。表現は変えていない（従来どおり `Value::Symbol(":foo")`）ので、`defmacro` の `&key` が
先頭コロンを文字列として剥がす既存実装はそのまま動く。CL と違いコロンは名前の一部
（`(symbol->string :foo)` は `":foo"`）——パッケージ機構が無いため。

### 箱の透過性という一貫した規則

trait オブジェクトは**ディスパッチ以外の意味を持たない**、を全経路で貫いた: 印字は中身を出し、
`eq`/`eql`/`equal`/`equalp` は箱を透かし、trait のメソッド以外の呼び出しは `Sexpr` メソッド
カタログにフォールバックし、`Sexpr` に入れるときは箱を外す。特に**箱は暗黙に作られる**ので、
箱の有無で `eq` の答えが変わってはならない、というのが決め手。`match` も箱を外してから既存の
`Sexpr` downcast パターンに渡すので、言語の実行時型テストは依然として1種類だけ。

### compile 対応

- `ast_bridge` に `dyn-new`/`dyn-call`/`dyn-value` の3ノードを追加。`Expr::DynBox` は
  `collect_calls` で **slots を丸ごとメソッドターゲットとして登録**する——これで
  `compute_sccs` の finish-order 契約により、dyn 呼び出しに到達する時点で実装メソッドは
  必ずコンパイル済みになる（箱詰め地点で静的に列挙できるのが効いている）。
- 島側 `compile-dyn-call` は `rt_dyn_vtable` → `rt_vtable_slot` で生関数ポインタを取り、
  新組み込み `build-dyn-call` が `build_indirect_call` する。クロージャ呼び出しと違い env が
  無いので、`build-closure-apply` の 64スロット scratch とランタイムループは要らない。
- vtable の充填は `compile_scc` の**末尾**（メンバのアドレス確定後）。SCC 内のメソッドが
  スロットになり得るため。AOT では `ptrtoint` 定数で `rt_vtable_set` を起動時に呼ぶ。
- **「箱詰め地点で列挙できる」だけでは足りなかった**（実装中に発見）。`(compile render)` の
  `render` が `:dyn` を*受け取って呼ぶだけ*で、箱詰めは呼び出し側のインタプリタコードにある、
  という形では `render` の本体に `DynBox` が無く、実装メソッドが何も呼び出しグラフに載らない
  ——結果 `rt_vtable_slot` が空スロットで abort する。対策を3層にした:
  (a) `Expr::DynCall` に `impl_targets`（チェック時点の全実装、`Registry::trait_impls` の逆引き）
  を持たせて呼び出しグラフに載せる、(b) インタプリタが箱詰めするたびにその vtable を publish する
  （コンパイル直後だけでは、既にコンパイル済みのコードへ後から箱が渡る経路を取り逃す）、
  (c) 後から追加された `impl` のために、「この trait をディスパッチするコンパイル済みコードが
  既に存在する」ことを `Interp::dyn_dispatch_compiled` で記録しておき、その場合に限り箱詰め時に
  未コンパイルのスロットをコンパイルする。`:dyn` をコンパイルしないプログラムでは (c) の条件が
  常に偽なので、箱詰めが勝手にコンパイルを誘発することはない。
- AOT はそもそも `defstruct`/`defmethod`/`impl` をトップレベルに書けなかった（`compile-file` が
  `defun`/`defvar`/`defenum` 以外を拒否していた）ので、その前提拡張も併せて行った。

FASL は v11→**v12**。

---

## ユーザ定義エラー型 + `Error` トレイト（2026-07-25、旧 TODO T3）

`Result<T,E>` の `E` にユーザ定義型を載せること自体は**着手前から動いていた**（`E` は最初から
任意の型を取る総称パラメタで、`defstruct`/`defenum` の値も `match` もそのまま通る）。実際に
欠けていたのは「**エラーであることを表す共通の interface**」と、それを持たない組み込みエラー型の
特別扱いだった。作業前に現状を実測して確認した事実:

- `Result<T, ユーザ定義型>` は動く。`Result<T, :dyn Trait>` も動く（具象値は期待型位置で自動箱詰め）。
- 組み込みは汎用 `Error` 型（`error(string)` の単一変種）1つで、共通 interface は無い。

### 設計判断: `Error` は型ではなくトレイト（Rust std に合わせる）

ユーザー判断で**型とトレイトの同名共存を禁止**した（Rust では両者は同じ名前空間）。テーブルは
別（`Namespace::types` / `Namespace::traits`）なので機構上は共存できてしまい、`Foo` と
`:dyn Foo` が無関係な2つの定義を指す事故が起こり得る——`Checker::check_type_trait_clash` を
両方向（`deftrait` 側と `defstruct`/`defenum` 側）に入れて禁止した。

この禁止により「組み込み汎用型 `Error`」と「トレイト `Error`」は同居できない。Rust に寄せる
選択として**トレイトに `Error` の名を与え、具象型は発生源ごとに分割**した（`std` でも `Error` は
トレイト、具象型は `ParseIntError`/`io::Error`）:

| 型 | 生成元 | Rust の対応物 |
|---|---|---|
| `ParseIntError` | `parse-int` | `std::num::ParseIntError` |
| `ParseFloatError` | `parse-float` | `std::num::ParseFloatError` |
| `ReadError` | `read` | （typelisp 固有） |
| `EvalError` | `eval` | （typelisp 固有） |

4型とも「メッセージ文字列1つを持つ単一変種の直和型・型名＝変種名」で、旧 `error` 型と同じ形。
つまりヒープ表現・`match`・trait オブジェクト箱詰めの経路は既存のまま流用でき、**ユーザが
`(defenum my-err (my-err string))` と書いたときと1バイトも変わらない**（`registry.rs` の
`builtin_error_defs`、実行時生成は `interp.rs` の `result_err`）。

prelude に `Error` トレイト（`message` / `source`）と4型の `impl`、および
`as-dyn-error`（`Result<T,E>` → `Result<T,:dyn Error>`、`(where (Error E))`）を追加。
`source` は Rust の `Error::source` と同じく原因の連鎖を返す。

### 実装中に見つかった穴3つ（いずれも T3 の前提として修正）

1. **`deftrait` が自分自身を参照できない**。`source` の戻り型 `Option<:dyn Error>` は宣言中の
   トレイトを名指すが、メソッド署名は `TraitDef` 登録より前に解析されていたため
   「`dyn: unknown trait`」になった。`check_defstruct` が自己参照フィールドのために既にやっていた
   のと同じ**スタブ先行登録**で解決。
2. **`where` 節のトレイト名が未解決のまま保存されていた**（`Path::root(書かれた名前)`）。ファイル＝
   モジュールなので、モジュール内の `(where (Error E))` は `error` を root 直下と記録し、
   `TraitDef::name`/`Type::Dyn` の完全修飾パスと永久に一致しなかった（潜在バグ）。
   `resolve_trait_name` を通すよう修正。
3. **境界付き型変数を `:dyn` に箱詰めできない**。`as-dyn-error` の本体そのもの。ジェネリック関数の
   定義時本体検査では `E` はまだ型変数なので vtable を敷けない——`Expr::TraitCall`（消去済み
   ジェネリック本体の診断専用ノード。実行されない）を置き、各単型化で `E` が具象化した本体を
   再検査する際に本物の `dyn_vtable_slots` を通す形にした。境界があることが「後で必ず敷ける」
   保証になっている。

### 型位置にトレイト名を書いたときの診断

`Result<T, Error>`（旧来の綴り）が**黙って通ってしまう**のを避ける必要があった——未解決の型名は
ジェネリックの型変数と同じ扱いで通ってしまうため（`canon` は失敗できない）。全ての型注釈解析を
`Checker::parse_type_here` に一本化し、「型として解決できないがトレイトとしては解決できる名前」を
`` `error` is a trait, not a type — write `:dyn error` `` と報告するようにした。

### 影響範囲

`.typl` ソース中の `Error` 参照は 0 件、テストの typelisp ソース中12箇所と docs 15行のみ
（`Error` は主に Rust 側 API 名として登場していたため）。FASL は v12→**v13**
（消えた型を名指す署名・比較不能になる境界が古いキャッシュに残るため）。

## pretty printer（CLHS 22.2 相当、2026-07-26、旧 TODO T5 の Tier1/Tier2、branch `feature/pretty-printer`）

`format` の pretty 系ディレクティブが no-op のままだった穴を埋め、CL の Lisp Pretty Printer に
相当する整形機構を入れた。TODO T5 の Tier1（`*print-*` 変数 + `format` ディレクティブ + `pprint` 系）と
Tier2（`pprint-logical-block` 等のユーザ呼び出し可能 API）が対象。Tier3（`set-pprint-dispatch`）は
**未実装で TODO に残した**——理由は下記「Tier3 を見送った理由」。

利用者向けの仕様は [functions.md](../functions.md) §15.1。

### 中核: XP をストリームではなく2パスで実装した（[pprint.rs](../../src/eval/pprint.rs)）

CL の pretty printer（R. Waters の XP）は**本物の出力ストリームをラップし、行幅ぶんの有界先読みで
改行を確定する1パスのストリーム方式**。有界先読みが要るのは、ストリームが無限に続きうるから。
typelisp の印字経路はすべて `format` を通り、`format` は既に**インメモリで `String` を組み立てる**ので、
改行を決める時点で文書は全部手元にある——先読み機構を持つ理由がそもそも無い。

そこで「テキスト＋バイトオフセットに紐づく命令列」（`Out`）を組み立て、最後に1回レイアウトする2パスにした。
XP が近似している問いが「この区間は収まるか」なので、**直接それを計算するだけで結果は同じ**になる。
TODO T5 の調査メモがこの簡略化を明示的に容認していた。`Out` に命令が1つも記録されていなければ
（`*print-pretty*` が偽なら常にそう）レイアウトパス自体を通らないので、既存の出力経路のコストは変わらない。

改行判定は CLHS `pprint-newline` の規則をそのまま実装（`:linear` はセクション単位ではなく**ブロック単位**で
判定する——これが「1つ折れたらそのブロックの `:linear` は全部折れる」という CLHS の規則そのもので、
`pprint-linear` の「全部1行か1要素1行か」もこれで出る）。

実装中に落ちた罠2つ:

1. **ブロックの suffix 幅を測り忘れる**。`BlockStart` が prefix/suffix の両方を持つ構造にしたため、
   幅の累積計算で `BlockEnd` に何も加算しておらず、`(outer (a b))` が1桁短く見積もられていた。
   `Metrics` 側にも suffix 幅のスタックを持たせて解消。
2. **`:fill` の規則 (b) の解釈**。「直前のセクションが1行に収まらなかった」の「直前のセクション」に、
   *その条件改行自身で折れたこと*を含めてしまうと、1回折れた瞬間に以降の `:fill` が全部連鎖して折れ、
   語詰めが「1要素1行」に化ける。境界での改行はセクションの**内側**の出来事ではない、と分けて修正。
   これで `pprint` の出力が SBCL と一致するようになった。

タブは幅が桁位置に依存するため幅計算では 0 幅としているが、`pprint-tabular` が出す
「条件改行の直後のタブ」だけは実際の着地桁を計算してから測る（`skip_tab`）。これが無いと表形式が
右マージンを1桁はみ出す。

### `format` 側（[format.rs](../../src/eval/format.rs)）

出力先を `String` から `Out` に差し替え、`~_`（条件改行）/ `~i`（インデント）/ `~:t`（セクション相対タブ）/
`~<...~:>`（**論理ブロック**。閉じの `:` で桁揃えの `~<...~>` と分岐）を実装。`~a`/`~s`/`~w` は
`*print-pretty*` が真かつ桁揃えパラメータが無いときに整形経路へ入る（パラメータで幅を固定する指定と
レイアウトに幅を任せる指定は両立しないので、明示指定があるほうを優先）。

`~(...~)` の大文字小文字変換は変換後のバイト長が変わりうるので、命令アンカーを保つために
`Out::map_text`（命令の切れ目でテキストを区切り、語境界の状態を跨いで引き回しながら変換）を通す。

### 制御変数（prelude の `defvar`）

`*print-pretty*` / `*print-right-margin*` / `*print-miser-width*`。CL では特殊変数だが typelisp に
動的束縛は無いので通常のグローバルにし、`Interp::pretty_opts` が印字のたびに読み直す。
`*print-pretty*` の既定は `false`——既存プログラムの出力を1バイトも変えないため（CL でも初期値は
処理系定義）。`*print-miser-width*` は `nil` の代わりに 0 を「無効」とする。

### ユーザ API: 論理ブロックを「暗黙の大域状態」にした（Tier2）

T5 が Tier2 の前提としていた「可変 pretty ストリーム値型の新設」は、**作らずに済ませた**。
CL でストリームを引き回すのは第一級ストリームがあるからで、typelisp には無い——代わりに
**開いている論理ブロックをインタプリタの暗黙状態にした**（GC ヒープを暗黙の大域状態にしたのと同じ判断。
`Interp::PrettySession`）。最も外側の `pprint-logical-block` がセッションを開き、閉じたときに一括で
整形して書き出す。開いている間は `print`/`println`/`(format true ...)`/`pprint` の出力もすべて
セッションへ入るので、**内容は普通の `print` で書き、改行位置だけ `pprint-newline` で指定する**という
CL とほぼ同じコードになる。新しい値型は1つも増えていない。

- `pprint-logical-block` は特殊形（オプションが入れ子リスト上のキーワード引数で、本体列の前後に
  開始・終了を挟む必要があるため）。`pprint-newline`/`pprint-indent`/`pprint-tab`/`pprint-pop`/
  `pprint-list-exhausted` は素の組み込み関数（引数は CL のキーワードそのもの＝自己評価シンボル）。
- `pprint-pop` が辿るリストは **`Heap::alloc_cell` のセル**に置く。セルは `Rc` が生きている間ずっと
  GC ルートなので、ブロック本体が何を確保しても安全で、しかもブロックを抜ければ自動的に解放される
  （`push_permanent_root` は解放できないのでループ中の使用に耐えない）。
- `pprint-exit-if-list-exhausted` は CL ではブロックからの非局所脱出。typelisp に汎用の脱出は無いので
  **囲む `loop` からの `break`** に展開するマクロにした（CL 側の定型もつねに `loop` の中に書くので
  書き味は変わらない）。

### Tier3（`set-pprint-dispatch`）を見送った理由

T5 は「前提は T4 動的ディスパッチ、それが済めば残るは可変ストリーム値型だけ」と記録していたが、
実装してみると**残っていた前提はそこではなかった**。`set-pprint-dispatch` は「印字の途中で、
値の実行時型に応じてユーザ関数を呼び戻す」機構であり、必要なのは:

1. レンダラ全体への `&mut Heap` の引き回し（現在の `format`/`render_value` は `&Heap`）、
2. Rust 側から typelisp の関数値を呼ぶ橋（`Expr::Apply` の compiled-closure 経路の切り出し）、
3. **呼び戻しの最中に宙に浮く `Value` の GC ルート保護**——レンダラは走査中のリスト要素を
   Rust の `Vec<Value>` に持っており、そこからユーザコードを呼べば確保が起きて回収されうる。

3 が本質的な難所で、印字経路（全プログラムが通る）に微妙な GC バグを持ち込みうる。Tier1/Tier2 は
それ抜きで完結しており、単体で十分な価値があるため、Tier3 は分離して TODO に残した。

### テスト

[pprint_test.rs](../../tests/pprint_test.rs) 25件（`format` のディレクティブは戻り値の文字列で、
標準出力へ書く `pprint`/`pprint-logical-block` は `typl` バイナリを起動して stdout を読む）と、
[pprint.rs](../../src/eval/pprint.rs) 内のレイアウト単体テスト10件。

## `print-object` トレイト（型ごとの印字表現、2026-07-26、旧 TODO T5-b、branch `feature/print-object`）

pretty printer の Tier3 として残していた `set-pprint-dispatch` を実装しようとして、
**目標設定そのものがこの言語では間違っていた**と判明したので、`print-object` トレイトへ
差し替えた。利用者向け仕様は [functions.md](../functions.md) §15.2。

### なぜ `set-pprint-dispatch` を採らなかったか

TODO には「実行時の**型名文字列 → 関数**の登録表」と書いてあった。着手前の議論で、
ユーザーから「静的型付け言語なのだから呼び出し側の型は決まっているはず」という指摘があり、
そこから2点が出た:

1. **登録側が無検査**。`(set-pprint-dispatch "point" print-point)` の `"point"` も
   `(fn (sexpr) string)` というシグネチャも、コンパイル時には何も検証されない。型名の
   打ち間違いも、`point` 以外を受け取るプリンタの登録も通る。しかもプリンタ本体は `match` で
   引数を downcast し直す——**登録時点で分かっていた型を、捨ててから復元している**。
   この言語には「型ごとに実装を対応づける」機構が既にある（`deftrait`/`impl`、`AdtDef::impls`）。
2. **CL でも別物が2つある**。`print-object`（CLOS の総称関数＝クラスごとのメソッド）と
   `set-pprint-dispatch`（型指定子キーの表、pretty 時のみ）。trait/impl に対応するのは前者で、
   そちらを採るほうが CL 準拠でもある。

`set-pprint-dispatch` は「意図的にやらないもの」へ移した（当時は TODO.md、2026-07-29 に
[language-design.md](language-design.md) §9「採用しないと決めた機能」へ移設）。

### 静的に決まるのは「登録」であって「選択」ではない

差し替えの過程で一度、**checker が印字地点の静的型から impl を引いて呼び出しを事前に
埋め込む**（実行時機構ゼロ）案を検討したが、これは成立しない:

- どのディレクティブがどの引数を消費するかは**制御文字列の実行時の中身**で決まる
  （`(format false s x)` の `s` は変数でもよい）ので、checker には `~a`（princ）と
  `~s`（prin1）のどちらが問うているのか分からない。事前に文字列へ潰すと `~s` の
  意味が失われる。
- レンダラの中では分かる——`render_value(…, standard: bool, …)` の `standard` が
  まさにその区別。これを CL の `*print-escape*` として `print-object` の第2引数に渡す。

結果、CLOS と同じ構図に落ち着いた: **メソッドは型ごとに静的に定義・型検査され、選択は
印字の瞬間に行われる**。

### 実装

- prelude に `(deftrait print-object (print-object ((self Self) (escape bool)) string))`。
  トレイト名とメソッド名が同じでも通る（禁止されているのは*型*とトレイトの同名共存だけ）。
  組み込み型への `impl` は入れていない——既存の出力を1バイトも変えないため。
- **登録表は作らない**。`impl` はふつうの `defmethod` として `Interp` のモジュールツリーに
  載るので、`root.get_method(&type_path, "print-object")` がそのまま索引になる。二重管理ゼロ。
  拾った `FnDef::sig` が `([T, bool], string)` かを確認するのは、トレイト impl ではない
  同名メソッドを誤って拾わないための安全弁。
- 呼び出しは既存の `Interp::apply(heap, &FnDef, argv)`。当初 Tier3 の前提に挙げていた
  「Rust→typelisp の呼び出し橋の新設」は不要だった（compiled-closure 経路の切り出しも不要）。
- `format::build`/`render_value`/`pprint::render` を `&Heap` から `&mut Heap` へ。
  `enums` と `Option<&Interp>` は `RenderCtx`（Copy）にまとめて引き回す。`interp: None` なら
  ディスパッチしないので、単体テストや埋め込み用途は従来のまま。

### GC ルート保護——**追加のルートは要らなかった**（実装中に判明、要注意）

着手前はこれを最大の難所と見ていた。実際に書いたのも当初は「印字対象を入口で1回
`push_root` し、`expand_macro` と同じ順序で解く」ヘルパーだった。**これは不要であり、
しかも有害だった**:

- **不要**: ビルトインの引数は `Interp::eval_args` が1つずつ `native_slot` として
  登録しており、そのアンカーは `eval_builtin` の呼び出し全体にわたって生きている。
  `sync_roots` は確保の直前にその生きたスロット集合からルートを組み直す。GC は非移動の
  mark-sweep で `car`/`cdr` とボックス内の入れ子を辿るので、**最外の値1つのアンカーが
  その全部分値を覆う**——レンダラが持つ中間の `Vec<Value>` の中身も全部その子孫。
- **有害**: `format-rt` に入る時点で `self.rooted` は既に非ゼロ（引数リストは直前に
  `sexpr-cons` で組まれ、そのビルトインが `sync_roots` を呼ぶ）。その上に `push_root`
  すると、次の `sync_roots` が自分のバッチを**スタック先頭から** `rooted` 個 pop する際に
  こちらのルートまで持っていく。`expand_macro` の doc コメントが書いている strict-LIFO
  不変条件の違反そのもの。

GC ストレステストは削除前も通っていた——`eval_args` のアンカーが実際の保護をしていたから
であり、追加ルートは効いていなかった。**「念のため根を積む」がこのヒープでは安全側に倒れない**
という教訓（`sync_roots` は自分のバッチが先頭にある前提で pop する）。`Interp::print_object` の
doc コメントに理由を書き残した。

### 再入ガード

`(impl print-object point (... (format false "~a" self)))` は無限再帰する。印字中の値を
`Interp::printing` に積み、同じ値が再び現れたら組み込み表現へフォールバックする。深さ制限では
なく**値の同一性**で見るので、正当な自己参照構造の入れ子印字は妨げない。

### テスト

[pprint_test.rs](../../tests/pprint_test.rs) に8件追加（合計34件）。要は
`a_print_object_that_conses_heavily_survives_collections` ——20000セルのアリーナで、
プリンタ1回につき2000セル消費する impl を200回呼び、100行すべてが正しく出ることを確認する
（`eval_args` のアンカーが効いていなければ壊れる）。ほかに `escape` の `~a`/`~s` 差、入れ子リストでの
ディスパッチ、impl 無しの非回帰、同名だがトレイトでない `defmethod` の除外、再入ガード、
pretty printer との合成。

## 型名ハイライトを解決駆動へ全面再設計（2026-07-28、branch `feature/vscode-extension`）

`textDocument/semanticTokens` の初版（同ブランチ内、コミット `f3e0217` 時点）は
「レジストリから集めた型名の集合をドキュメント本文にテキスト照合する」実装だった。
これには自認していた欠陥が2つあり、ユーザーから**設計からやり直せ**という指摘を受けて
作り直した。

### 何が壊れていたか

1. **型名と関数名の衝突で誤着色**。型と関数は別テーブルなので `(defstruct pad ...)` と
   `(defun pad ...)` は両立するが、テキスト照合には区別がつかず `(pad 3)` の呼び出しまで
   型として着色していた。初版はこれを「型注釈は `Type` にパースされソース位置を持たないので
   AST では照合できない」としてコメントに残していた——が、それは**チェッカが位置を捨てている**
   という実装の都合であって、言語の性質ではない。
2. **eglot では越境した型が着色されない**。eglot は semanticTokens 非対応（Emacs 29.3/30 の
   `eglot.el` に該当コードが1行も無いことを確認済み）なので、README に「eglot ではバッファ内
   解決のみ」と書いて終わりにしていた。

### 型注釈にソース位置を持たせる（`types.rs`）

根本原因は `parse_type` が `Type` だけを返し、どのトークンのどの位置から来たかを捨てていた
こと。`parse_type_spanned` を追加し、**書かれた型名1つごとに `TypeNameSpan`（書かれたパス、
`:dyn` ヘッダか、最終セグメントの正確なスパン）を記録**するようにした。

ジェネリック型は**1つのシンボルトークン**として読まれる（`Vector<:dyn Shape>` も
`HashTable<i32,todo-item>` も1トークン）ので、位置はトークン内バイトオフセットから起こす。
`NameLexer` に1トークン先読みとトークン境界の報告を足した `Toks` を被せ、識別子の
バイト範囲を取れるようにしてある。

トークンは `Heap::intern_symbol` が小文字化して保持するので、オフセット計算が正しい
ソース列を指すのは**畳み込みが文字数を保つ場合だけ**（ASCII は保つが `İ` は2文字になる）。
そこで記録前にトークン長と一致するかを検証し、食い違ったら**間違った範囲を塗るのではなく
何も記録しない**。同じ検証を `::` パスの列計算にも入れてある。

### チェッカが解決時に記録する（`checker.rs`）

`parse_type_here` を `parse_type_here_at`（注釈自身の `Loc` を受け取る）に置き換え、
**全17か所の注釈サイト**に `Loc` を配線した。記録は `canon` と同じ解決を通るので、
束縛済み型変数はスキップ、組み込み型（`Option`/`Vector`/...）は各エディタの文法に任せて除外、
未解決名（ジェネリックテンプレートの型変数）は何も記録しない。

注釈以外にも、型名が現れてチェッカが解決している箇所を全て拾う:

| 位置 | 例 | 記録するセグメント |
|---|---|---|
| 定義ヘッダ | `(defstruct rect ...)` | 名前（`rect<T>` の `<T>` は含めない） |
| `impl` のトレイト/対象 | `(impl Shape rect ...)` | 両方 |
| `where` 境界のトレイト | `(where (Eq A))` | トレイト名 |
| 静的メンバ呼び出し | `(rect::new 5)` | 末尾から2番目 |
| `use` | `(use model::todo-item)` | **末尾**（インポートは型そのものを指す） |
| パターンヘッド | `(match s ((point x y) ...))` | ヘッド、`color::red` なら末尾から2番目 |

`impl` のメソッド署名は `Self`/関連型を置換してから再構築するため、置換で**変化しなかった**
スロットだけ元のスパンを引き継ぐ（`list_from_vec_locs`）。置換された `Self` には固有の位置が
無いので記録されない。

### 効果

`semantic.rs` はテキスト走査を全廃し、記録済み `TypeUse` をファイルで絞って LSP 座標へ
変換するだけになった。ジェネリックテンプレートはインスタンス化ごとに再チェックされ同じ
スパンを複数回記録するので、そこで dedup する。

**衝突の誤着色は原理的に消えた**——チェッカが型として解決した位置にしかトークンが無いので、
`(pad 3)` には記録が存在しない。[lsp_semantic_test.rs](../../tests/lsp_semantic_test.rs) に
その回帰テストを置いた。型エラーがある文書でも（recover モードで木が出るので）着色が
続くことも、テストと実プロセスの両方で確認済み。

### eglot 対応（`typelisp-mode.el`）

eglot が非対応なら**モード側が自分でリクエストを投げればよい**。eglot の JSON-RPC 接続に
`jsonrpc-async-request` で `textDocument/semanticTokens/full` を送り、オーバレイで描画する
`typelisp-semantic-tokens-mode` を新設した（eglot 接続時に自動で有効化）。

- **オーバレイ**（テキストプロパティでない）: font-lock の再描画で消えず、編集で一緒に動く
- legend は**サーバの `initialize` 応答から読む**。順序を仮定しない（読めなければ何も描かない）
- リクエスト前に `eglot--signal-textDocument/didChange` で保留中の編集を流す。eglot は
  didChange を自前のアイドルタイマでまとめるので、送る前に問い合わせるとサーバが知らない
  テキストに対する位置が返る
- サーバが答えている間はバッファ内解決のフォールバックが退く（`lsp-mode` 使用時も同様に退く）

[scripts/emacs-semantic-smoke.el](../../scripts/emacs-semantic-smoke.el) が**実際の eglot 接続**で
検証する（`store.typl` の越境 `todo-item` 3箇所が着色され、フォールバックが退いていること）。

### ドリフト検出

legend（`lsp.rs` の `SEMANTIC_TOKEN_TYPES` と両エディタの対応表）がずれても実行時エラーには
ならず全トークンの色が入れ替わるだけなので、`editor_keyword_sync_test` に名前・順序の一致
検査を追加した。

## `*print-circle*` / `*print-level*` / `*print-length*`（印字量の制御、2026-07-29）

CLHS 22.1.1 の「値のどこまでを印字するか」を決める3変数。
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) §3 の8番目として挙げていた
項目で、ユーザ向け仕様は [functions.md](../functions.md) §15.3。

### 着手前の状態は「落ちる」だった

「循環構造を印字すると停止しない」と書いていたが、実際に確かめると**停止しないのではなく
プロセスが死ぬ**:

```lisp
(defstruct node (val i64) (next Option<node>))
(let ((a (node::new 1 (option::none))))
  (setf a::next (option::some a))
  (println "~a" a))
;; thread 'main' has overflowed its stack
;; fatal runtime error: stack overflow, aborting
```

`render_value` が `car`/フィールドを無条件に再帰で辿るため。typelisp で循環を作れる経路は
**`defstruct` のフィールドを `setf` で自分自身に向けること**だけである（`Sexpr` の cons セルは
`set-car`/`set-cdr` を撤去済みで作成後に書き換えられないので、リテラルのリストは循環しない）。
逆に言えば、この経路がある以上は再現可能な実害だった。

### 設計

- **prelude のグローバル3つを追加**（`*print-circle*` `bool`、`*print-level*`/`*print-length*`
  `i64`）。既存の `*print-pretty*` 等と同じく `Interp::print_limits` が印字のたびに読む。
  CL の「無制限 = `nil`」は typelisp に `nil` が無いので **0 以下 = 無制限**（`*print-right-margin*`
  と同じ既存の慣習）。既定は3つとも「制限なし」で、CL の初期値とも既存の出力とも一致する。
- **`RenderCtx` に `Limits` を追加**。`enums`/`interp` と同じく印字経路全体を流れる。
- **`*print-circle*` は2パス**。パス1（`scan_shared`）が値を走査して2回以上到達するノードを
  記録し、パス2の描画時に最初の出現へ `#n=`、以降へ `#n#` を出す。パス1は**明示スタックの
  反復**で書いた——再帰にすると、循環そのものと長いリストの両方で落ちるため。2回目に見た
  ノードは印を付けて辿らないので循環でも必ず停止する。
- **ラベルを振るのは「入れ子を持てるノード」だけ**（cons セル・`defstruct`/`defenum` の箱・
  `:dyn` の箱）。float/bignum/ratio の箱も `Value::Boxed` だが中に値を持たないので、共有しても
  ラベルは付けない（`#1=` が出ても情報が無くノイズになる）。ハッシュテーブルは
  `#<hashtable count=N>` で中身を辿らないので同様に対象外。

### プリティプリンタとの合流が本題だった

素直に `render_value` だけへ実装すると穴が空く。`~a`/`~s`/`~w` は `*print-pretty*` が真だと
**リストを `pprint::render` 側の走査に渡す**（レイアウトのため）ので、そちらにも同じ打ち切りと
ラベル付けが要る。しかも `*print-circle*` のラベル番号は**1つの印字対象で通し**でなければ
ならないので、2つの走査が別々に状態を持つわけにいかない。

そこで `Renderer`（ラベル表＋採番カウンタ）を `format` 側に置き、`pprint::render` は入口で
1つ作って内部の `render_at` へ `&mut Renderer` と `depth` を引き回す形にした。両者の合流点は
`Renderer::pre`——ノードを見た瞬間に「`#n#` で終わり」「`#` で終わり」「`#n=` を前置して続行」の
3択を返す。`pprint` 側は非リスト値を `Renderer::render` へ委譲するので、そこで `pre` が
二重に走らないよう、リストの場合だけ自分で `pre` を呼ぶ。

`*print-length*` で切った `...` は、**要素と同じ条件改行の上に置く**（`push_ellipsis`）。
そうしないと折り返し時に `...` だけが行から溢れる。

### 未対応（意図的）

- **REPL のトップレベル値表示**（`src/main.rs` の `format_value`/`format_sexpr`）はこの3変数を
  見ない。REPL の表示器は `print-object` も `*print-pretty*` も見ない独立の実装で、そちらに
  合わせた。したがって循環値を REPL で**そのまま評価**するとやはり落ちる（`println` で印字する
  ぶんには `*print-circle*` が効く）。
- `Sexpr` のリスト自体が循環する経路は現状存在しないため、`pprint` 側の `list_items`（リストを
  `Vec` へ平坦化する）には循環対策を入れていない。`set-car`/`set-cdr` 相当を将来入れるなら
  ここも要対応。

### テスト

`tests/print_limits_test.rs`（23件）。深さ・長さの打ち切り、struct/enum フィールドへの適用、
0 以下が無制限であること、共有と循環のラベル付け、引数ごとにラベルが 1 から振り直されること、
`~s` 経路、`*print-pretty*` 経路、`println`/`pprint` 経路。循環を作るテストは
`(defenum link (no-link) (to node))` + `(defstruct node (val i64) (next link))` を使う
（`Option<node>` でも作れる。次の節で解消した「組み込み `option` の変種名が
`<unknown-variant>` と表示される」問題を発見したのはこのテストを書いていたときだが、
テスト自体をユーザ定義 `defenum` にしておけばその問題を踏まずに書けたので、
そのまま残してある）。

## 組み込み sum 型（`Option`/`Result`/エラー型）の変種名解決（2026-07-29）

上の節のテストを書いている最中に見つけたバグ。`(println "~a" (option::some 1))` が
`(some 1)` ではなく `(<unknown-variant> 1)` と表示される。

### 原因

印字器（`crate::eval::format::render_value` の boxed-enum アーム）が変種名を引く
`enums: &HashMap<Path, EnumDef>` は、`Interp::exec` が `TopLevel::Defenum` を実行した
ときにしか埋まらない（`Interp` のスコープツリー、`scope::TypeEntry::Enum`）。
`Option`/`Result`/`ParseIntError` 等4種の組み込みエラー型は**チェッカ側の
`Registry::with_builtins()`（src/check/registry.rs）で定義されるだけで exec されない**ため、
このテーブルに一度も載らない。

### 却下した最初の修正——参照時フォールバック

最初は「印字時に `enums` で引けなければ registry 側の定義を直接引き直す」
`registry::builtin_variant_name(path, index)` を実装した。しかしユーザーから
「フォールバックを使いたくなるのは大抵バグ。参照時にごまかすのではなく、定義を1箇所に
まとめ、登録も1箇所の関数からしかできないようにすべき」という指摘を受け、設計をやり直した。

### 採用した修正——定義源1箇所・登録関数1箇所

- **定義源**: `registry.rs` に `builtin_sum_defs() -> Vec<AdtDef>`（`option_def`/`result_def`/
  `builtin_error_defs` をまとめるだけ）を新設。`Sexpr` は含めない——`Sexpr` の値は
  `BoxedObj::Enum` にならず専用の `Value` バリアントで表現されるため、印字経路に乗ることが
  そもそもない（`Interp::construct_sexpr` が `alloc_enum` を経由しないことをコード読解で確認）。
  `Registry::with_builtins()` はこの1関数から `add_type` するだけになった
  （旧: `option_def()`/`result_def()`/`builtin_error_defs()` を個別に呼んでいた）。
- **登録関数**: `scope::ModuleScope::register_enum(&mut self, name, def)` を新設し、
  「`TypeEntry::Enum` を挿入するのはこの関数のみ」にした。`TopLevel::Defenum` の exec
  （旧: 3行のインライン `insert`）と、下記のシードの**両方**がここを通る。
- **シード**: `Interp::new()` で、既存の `vector` 手動シード（`AdtKind::Struct` だが
  `Defstruct` を exec しないので手で入れている、という既存の前例）の直後に、
  `registry::builtin_sum_defs()` を回して `register_enum` を呼ぶループを追加。
  組み込み4型+`Option`/`Result`が `Interp::new()` の時点でスコープツリーに載るので、
  印字器の `enums` lookup は `defenum` と組み込み型を区別せず同じ1回の lookup で済む。
- **名前ベースの特別扱いを削除**: シード前は `is_enum_path`/`enum_fields_representable`
  （どちらも interp.rs）が `option_path()`/`result_path()`/`is_builtin_error_type` という
  名前直書きの分岐を持っていた——これは「定義がスコープツリーに無いので名前で回避する」
  という同じ種類のごまかしだったので、シード後は冗長になった分をまとめて削除し、
  `find_type` によるツリー参照1本にした。`option_path`/`result_path` 関数自体も削除
  （呼び出し元が無くなったため）。

### 確認したこと

- ルート名前空間での `(defenum option ...)` のような組み込み名の再定義は、`RedefPolicy` に
  関係なくチェッカが常にエラーにする（`checker.rs` の `check_redef_outcome` が
  `is_builtin` なら無条件で拒否）ので、シードが後から上書きされる経路は無い。
- `compile`（LLVM JIT/AOT）側の `ast_bridge::is_enum_ty` は元々 `Option`/`Result`/エラー型を
  名前で先に判定してから `enums.contains` にフォールバックする作りだったので、シードで
  `enums` に6件増えても分類結果は変わらない（二重に守られていたのを一重にしただけ）。
- prelude 無しで `Interp::new()` を使う既存の単体テスト群への影響は
  「スコープツリーに6エントリ増える」だけで、挙動を観測しているテストは無い
  （`function_names()` は `fns` のみを走査するため無関係）。

`tests/format_test.rs` に `built_in_enum_values_print_their_variant_names` 等3件を追加。

## Common Lisp 準拠 docstring + `documentation`（2026-07-30）

`cl-missing-classes-and-methods.md` の既知ギャップ（§2.21「docstring の仕組みが無い」）を解消。
計画: `~/.claude/plans/idempotent-sparking-horizon.md`。

### 構文: フォームごとに CL の docstring 位置をそのまま踏襲

- `defun`/`defmethod`（`impl` 内含む）/`defmacro`: 本体の先頭（戻り値型・`where` 節の後）。
  CL 通り、後ろに本体フォームが最低1つ続く場合のみ docstring とみなす——単独の文字列は
  戻り値のまま区別する（`Checker::take_leading_docstring`、`parts.len() > at + 1` のガード）。
- `defvar`/`defconstant`: CL の `defvar`/`defparameter`/`defconstant` と同じで初期値の**後ろ**
  （`(defvar (name Type) value "doc")`）。他フォームと位置が逆なのは CL 自体がそうだから。
- `defstruct`/`defenum`/`deftrait`: 名前の直後、フィールド/バリアント/アイテム列の**前**。
  これらは「本体」に相当するものが無く、フィールド等は常に構造化された形（裸の文字列になり
  得ない）なので曖昧性ガードは不要——無条件で消費する。
- `deftrait` はトレイト全体に1つだけ（CL の `defgeneric` が個々のメソッドでなく generic function
  自体に docstring を持つのと同じモデル）。個々のメソッドの docstring は `impl` 側の
  `defmethod` 本体が持つ（`check_impl` は各メソッド項目をそのまま `check_defmethod` に委譲する
  ため、docstring 対応は追加コード無しで自動的に効く）。

### 保存先: `Docs`（`DefLocs` と同型・同方針）

`check::registry::DefLocs`（定義位置テーブル）が「`FnSig`/`AdtDef`/... に生やすと
`Registry::with_builtins` の約150箇所を全部触ることになるので別テーブルに分離する」という
設計だったので、docstring もそのまま同型の `Docs`（`fns`/`methods`/`types`/`vars`/`traits`/
`macros`、すべて `Path`（メソッドのみ `(Path,String)`）キー）として追加。挿入は `def_locs` を
挿入している箇所全部に1行ずつ足すだけで済んだ（`with_builtins` 側は無変更）。

### `documentation`: ランタイム機構を一切増やさない check 時特殊形

CL の `(documentation 'name 'function)` は型引数で名前空間の曖昧性を解くが、typelisp は
「checker は常にどこへ解決するか知っている」という既存方針（`Checker::check_compile` が
`(compile name)`/`(compile Type::method)` を未評価の名前として読み、`resolve_fn`/
`resolve_bare_type` 等の既存解決ヘルパーで `CompileTarget` を直接組み立てているのと同じ発想）
に沿い、`documentation` も **`quote`/`compile` と同じ「未評価の名前を受け取る特殊形」** として
実装した。`check_documentation` は `check_compile` をほぼそのままなぞる形——`name.rsplit_once("::")`
で裸名と `Type::method` を分岐し、裸名は変数→関数→型→トレイト→マクロの優先順位（`Checker::check`
の `Value::Symbol` 腕が式として評価するときの優先順位と同じ）で `resolve_global`/`resolve_fn`/
`resolve_bare_type`/`resolve_trait_name`/`resolve_macro` を順に試す。結果は `Docs` テーブルを
引いて `Option<Str>` の**定数**（`wrap_some`/`option_none` — 既存のオプション構築ヘルパーをその
まま再利用）として check 時に畳み込む。ランタイムの `Checker` ハンドルも新規 builtin 分派も
一切不要——`compile`/`format` 等と違い、実行時に何かを引き直す理由が構造的に存在しない。

解決に失敗した場合（そんな名前の定義が無い）は check 時のエラー。解決はできたが docstring が
無い場合のみ `Option::none`。モジュール修飾された自由名（`mod::name`。`Type::method` は対応）は
スコープ外とした。

### FASL（バージョン15）

`DefLocsRepr`/`RegistryDelta::def_locs`/`Fasl::capture`/`load_into` の docstring 版
（`DocsRepr`/`RegistryDelta::docs`）を完全に対称な形で追加——`def_locs` の diff/mark 機構には
乗らず丸ごとコピーする既存パターンをそのまま踏襲。`FASL_FORMAT_VERSION` を14→15に。この
バンプは他の大半のバンプと違い「デシリアライズが失敗する」種類ではなく（新フィールドが単に
無いだけ）、バンプ無しだと**古いキャッシュに依存したモジュール越しの `documentation` が
黙って `none` を返す**（実行時フォールバックが無いため後から気づく手段が無い）という理由での
バンプであることをコメントに明記した。

### LSP hover 統合

`check::locate::definition_target`（`Typed` ノード→参照先 `Path` を `DefLocs` で引く既存関数）
と同じ添字を `Docs` に対して行う `doc_for` を追加し、`hover_text` が型の下に docstring を
連結するよう拡張（`hover_text(node, docs)`）。`src/bin/lsp.rs` の `Analysis` に `docs: Docs`
（`def_locs` と同様のスナップショット）を追加しただけで済んだ。

### テスト

`tests/docstring_test.rs`（新規、16件）: 各フォームの docstring 位置・`documentation` の解決
優先順位・単独文字列本体が戻り値のまま残ること・`defvar` の3番目の引数が文字列でなければ
エラーになること・未定義名を渡した場合のエラーを検証。`tests/fasl_test.rs` に
`fasl_capture_preserves_docstrings_for_documentation`（prelude fasl の上に小さな documented
モジュールをもう一段 capture/load_into し、ソースを一切読まない環境で `documentation` が
正しい値を返すことを確認）を追加。`tests/lsp_locate_test.rs` は `hover_text`/`program` の
シグネチャ変更に合わせて更新。

editor/emacs・editor/vscode の特殊形一覧に `documentation` を追加（`tests/editor_keyword_sync_test.rs`
のセンチネル間スキャンで自動的に検出される対象）。

## 汎用 place 機構（`incf`/`decf`/`rotatef`/`shiftf`/呼び出し形 `setf`、2026-07-30）

`docs/dev/cl-missing-classes-and-methods.md` §3 で最高優先の欠落機構として挙げられていた
「place は変数と `変数::field` の2種のみ」を解消。CLHS 相当の `incf`/`decf`/`push`/`pop`/
`rotatef`/`shiftf`/`(setf (gethash ...))` が書けない状態から、`defsetf` 相当の完全な拡張性は
持たないまま実用範囲まで引き上げた。

### place の3種類目: `(accessor recv key...)` 呼び出し形

`Checker::check_setf`（`src/check/checker.rs`）の `args[0]` 分岐に、既存の `Value::Symbol`
（変数）・`Value::Path`（`var::field`、`check_field_set`）に加え `Value::Cons`（呼び出し形）を
追加。当初は `get`/`set` という命名規約1本をハードコードしていたが、レビューで「静的型を
うまく使えないか」という指摘を受けて設計を変えた——CL の `defsetf`/`define-setf-expander` は
Lisp-2 の**グローバルな関数名前空間**にアクセサ名→更新関数を登録する実行時テーブルだが、
このプロジェクトの呼び出しは元々 `recv` の型ごとに解決される（`try_instance_method`）。
`Checker::check_setf_call_place` は `recv`（呼び出し形の第1引数）をまず `check_at` で
チェックしてその**静的型**を求め、`var::field` が `field`/`set-field` という規約で
`check_field_set` から解決しているのと同じやり方で、その型が `set-{accessor}` という
インスタンスメソッドを持てばそれを setter として使う。ユーザ定義型は `defmethod set-foo`
を書くだけで任意のアクセサ名 `foo` を setf 可能にでき、しかも `(type, name)` で引くので
別々の型が同じアクセサ名を無関係な setter に割り当てても衝突しない——CL のグローバル1本の
テーブルにはできないことができる。`Vector<T>`/`HashTable<K,V>` が既に持つ `get`→`set`
（`set-get` ではない）は既存 API 互換のための特例フォールバックとして残した。この設計だと
`defsetf` のような**登録フォーム自体が要らない**——静的型が最初から分かっているので、CL の
「実行時テーブルを引く」というステップがまるごと不要になる。

### `incf`/`decf`/`rotatef`/`shiftf`: なぜ `defmacro` でなく checker 内蔵か

`setf` は `is_builtin_form_head` で保護された組み込み特殊形（ユーザの `defmacro` で上書き
不可）なので、`defmacro` 展開からは `setf` を呼べない。このため4つとも `prelude.rs` ではなく
`Checker::check_incf_decf`/`Checker::check_rotatef_shiftf`（`src/check/checker.rs`）に実装し、
`is_builtin_form_head`・`check_list` の SPECIAL-FORM DISPATCH に登録した。実装方式自体は
defmacro と同じ「読み取り済み `Value` を組み立てて `self.check` に投げ直す」パターン
（`heap.intern_symbol`/`heap.cons`/既存の `Checker::list_from_vec_locs`）で、新しい `Expr`
バリアントは一切追加していない——`let*`/`setf`/`progn`/`+`/`-` の再帰展開だけで完結する。

呼び出し形 place を2回（読み取り1回・書き込み1回）参照する必要がある（`incf`/`rotatef`/
`shiftf` はどれも「今の値を読んでから書く」形）ため、`recv`/`key...` を毎回評価し直すと
CL の `get-setf-expansion` が禁じている二重評価になる。`Checker::place_dedup` が
`(get recv key...)` を一度だけ評価する `let*` 束縛（`%place-tmp-N`、リーダーが絶対に生成
しない `%` プレフィックスなのでユーザコードと衝突しない）に分解し、`Checker::wrap_let_star`
がそれを結果の式に被せる。変数・`var::field` place は束縛不要（`var::field` の受け手は
`check_field_set` の制約で常に裸の変数名なので副作用の心配が無い）。

`rotatef`/`shiftf` は共有の `Checker::check_rotatef_shiftf` に統合——`shiftf` は「最後の
place が通常の値式になり、先頭 place の旧値を返す `rotatef`」として書ける。全 place の
部分式を先に評価してから全代入を行う、という CLHS の逐次評価順序をそのまま守っている。

### `push`/`pop`: 同名のまま両立させた引数順

`push`/`pop` という名前は既に `Vector<T>` のインスタンスメソッド（`(push vec item)`、
受け手が先）として prelude・examples 全体（15箇所以上）で使われており、CL の
`(push item place)`（要素が先）は名前も一致するため単純に defmacro 化すると全既存呼び出しを
壊す。ユーザの選択（「defmethod の仕組みを使えば両立できる」）に従い、`push` を型ベースで
多重ディスパッチする一般的なフォールバックとして実装——`Checker::try_instance_method_swapped`
が、通常の受け手優先解決（`try_instance_method`、`args[0]` の型でメソッド解決）が失敗した
場合に限り、2引数呼び出しの引数を入れ替えてもう一度試す。`Vector<T>` は参照型（ヒープ上の
struct を直接変異、`heap.struct_push_field`）なので、どちらの引数順で解決されても実行時の
意味は同じ——CL のような setf 展開は元から不要。この仕組みは `push` に限定していない汎用の
2引数フォールバックなので、他の同名衝突が将来起きても同じ経路で解決される。

### テスト

`tests/place_test.rs`（新規、19件）: `incf`/`decf`（delta 省略時1・戻り値・呼び出し形
place の型不整合が正しく型エラーになること）、`rotatef`/`shiftf`（2/3変数・単一 place・
0個の place・呼び出し形 place の二重評価回避）、呼び出し形 `setf`（`HashTable` 書き込み・
setter が無いアクセサの拒否・変数でも `var::field` でも呼び出し形でもない place の拒否・
`defstruct`＋手書き `defmethod at`/`set-at` によるユーザ定義アクセサ名での place 動作）を
検証。`tests/vector_test.rs` に3件追加: CL の引数順 `push`、`Vector` 要素への呼び出し形
`setf`、`incf` の呼び出し形 place がインデックス式を1回しか評価しないこと。

editor/emacs・editor/vscode の特殊形一覧に `incf`/`decf`/`rotatef`/`shiftf` を追加
（`tests/editor_keyword_sync_test.rs` のセンチネル間スキャンで自動検出される対象）。

## トレイト機構を Rust 同等にする（2026-08-01）

`deftrait` は 2026-06-30 の導入以来ほぼ手つかずで、Rust と比べてスーパトレイト・デフォルト
メソッド本体・`impl` の完全性検査・トレイトメソッドの `where` 節・ブランケット実装・`:dyn` の
アップキャストが全て欠けていた。実害は prelude に出ており、`Eq`/`Ord`/`Error` の16の `impl` が
約30個の自明なメソッド本体を書き写していた（`prelude.rs` のコメントが「typelisp `deftrait` has
no default method bodies, so each impl spells out every method」と明記していた）。

### 構文: 継承リストは必須の位置スロット

```lisp
(deftrait Eq () ...)                        ; 継承なし
(deftrait Ord (Eq) ...)                     ; Rust の trait Ord: Eq
(deftrait CharSource ((Iter (Item char))) ...)  ; 関連型のピン留め
```

`(where (Eq Self))` 形（Rust の脱糖そのもの）や `(deftrait (Ord Eq) ...)` 形も検討したが、
**トレイト名が `deftrait` 直後の symbol のまま**という性質を保つこの形を採った。Emacs/VSCode
両方の型名着色正規表現・imenu・symbols 抽出が**無改修**で通る（ヘッダをリスト化する案はここが
全部壊れる）。`Name<T>` ヘッダ統一の唯一の例外を作らずに済む点も同じ。

代償は既存 `deftrait` 約45箇所への `()` 挿入で、これは機械的。

内部表現は `where` 境界と同じ `TraitBound` を流用する——スーパトレイトは
`trait Ord where Self: Eq` なので、型変数スロット（常に `Self`）だけ構文から省いた形にあたる。
`parse_where_clause` のピン解析部を括り出して両方から呼ぶ。

### vtable: 継承分を先頭に置くことが接頭辞性を生む

`TraitDef::vtable_order` は「推移的に継承したメソッド（親の記述順）→ 自前のメソッド」。
この順序が**アップキャストの実装コストを決めた**。最左スーパトレイト鎖上の `X` について
`X.vtable_order` が `Y.vtable_order` の接頭辞になるので、`:dyn Y` の箱はそのまま `:dyn X` として
通用する——`coerce_to_dyn` が `Typed` の型を差し替えるだけで、式には触らない。結果として
インタプリタ・JIT・AOT・**島**のいずれも無改修（`compiler_island.bc` の再生成が不要）。

非最左のスーパトレイト（`D(B,C)` の `C`）は接頭辞にならないので、この時点では誤ディスパッチ
せずに型エラーで拒否した（対象外として文書化）。後日実装している——後述の
「非最左スーパトレイトへの `:dyn` アップキャスト（2026-08-04）」を参照。

ダイヤモンドは宣言元が一致するので合流して1スロット。別トレイト由来の同名メソッドの継承と、
サブによる親メソッドの再宣言は、どちらもエラー（呼び出し側に曖昧性解消の構文が無い）。

### デフォルト本体: 下流をゼロ変更で通す

`TraitDef::defaults` にメソッド項目を**まるごと**（`OwnedForm` 列）retain し、`check_impl` が
省略メソッドを**既存の rebuild-and-check ループ**に流す。`AdtDef::assoc` に普通の `AssocFn` が
入るので、`dyn_vtable_slots`・`Expr::DynCall`・`publish_vtable`・AOT・島はどれも変更不要。
「デフォルトとはユーザーが書かなかった `impl` 項目である」という一文がそのまま実装になる。

3点だけ補強が要った:

1. **名前空間**: 本体はトレイトを書いたモジュールで解決しなければならない（そのモジュールの
   非公開関数を呼ぶデフォルトが成立するため）。`check_defmethod_in` を分け、**本体の
   `check_seq` だけ** `enter_specialization` で ns を差し替える。ヘッダは `impl` 側の ns で
   `Self` 置換済みなので動かせない。
2. **impl レベル `where`**: `(impl Ord cons-cell<A,B> (where (Ord A) (Ord B)) ...)`。これが
   無いと、合成されたデフォルト本体が `less` を呼んだ際に `A`/`B` が未解決のまま
   `validate_where_bounds` の「呼び出し元の宣言済み境界を探す」経路に入り、境界を持たない
   合成デフォルトが硬いエラーになる。**デフォルト本体より先に実装する必要があった。**
   合流は構文的に行う（`merge_where_clauses`）ので、ジェネリック所有者が retain する
   `MethodTemplate::Form` にもそのまま入り、単型化側に引数を足さずに済む。
3. **メソッドの `where` 節の `Self` 置換**: `check_impl` は `elems[3..]` を素通ししていたので
   `where` 内の `Self` が置換されていなかった。

**docstring**: 本体を持つメソッドは持てる（`Docs::trait_methods`）。本体を持たないシグネチャは
持てない——末尾の文字列はそれ自体がデフォルト本体の戻り値になり、両者を区別できないため。
これは既存の CL 規則がそのまま効いた結果で、専用の禁止コードは要らなかった（一度書いた
禁止分岐は到達不能と分かって削除した）。

### impl の完全性・適合性検査

従来 `check_impl` は `TraitDef` を**一切参照していなかった**。メソッドの書き忘れは `:dyn` 化して
`dyn_vtable_slots` に到達するまで検出されず、シグネチャ違いに至っては検出されなかった
（`tests/seq_ops_test.rs` の `(impl Eq point (equals ...))` が実際にこの穴に乗っていた——
デフォルト本体が入った今は `not-equals` が埋まるので、そのまま通る）。

`check_impl_conformance` で、未宣言メソッド・重複・関連型の欠落・メソッドの欠落・
シグネチャ不一致を `impl` の時点で弾く。落とし穴: `Self` の比較には**パース済みの**対象型を
使うこと。プリミティブは `Type::I32` であって `Named("i32")` ではなく、両者は `mangle_type` の
表示が同じでも等しくない（最初に再構築したせいで prelude 全体が落ちた）。

### スーパトレイト義務は「記述順」の規則

`impl Ord X` は `impl Eq X` が**先に**書かれていることを要求する。Rust より制限が強いが、
REPL・逐次 `load`・fasl 復元のどれでも決定的に判定できる唯一の形——遅延して解消する
「プログラムの終わり」がどれにも無い。prelude は既にこの順だったので移行コストはゼロ。

その帰結として `AdtDef::impls` はスーパトレイトについて閉じるので、`validate_where_bounds` の
平坦な一覧走査は**変更不要**のまま済んだ。推移的にしたのは「型変数が未解決の枝」だけ
（呼び出し側が `(where (Ord T))` しか宣言していないのに呼び出し先が `(Eq T)` を要求する場合）。

### ブランケット実装

`(impl<T> Clamp T (where (Ord T)) ...)`。リーダは `impl<T>` を単一シンボルとして返す
（`defstruct vector-iter<T>` の名前と同じ字句化）ので、`check_form_dispatch` は `match` の前に
`parse_generic_name_header` を通す。対象が**裸の型変数**のときだけブランケットで、
型構築子（`Vector<T>`）なら所有 `AdtDef` が1つに定まるので従来の経路をそのまま通る。

宣言時は `Namespace::blanket_impls` に**保存するだけ**——`AdtDef` に何も登録しない。
対象が型変数である以上どの本体もジェネリックで、具体型が要求する前に生成するのは
単型化が避けているはずの先行展開そのもの。実体化は `SpecRequest::Blanket` として既存の
単型化キューに乗り、`materialize_blanket_impl` が保存した項目を live な
`(impl Trait 具体型 ...)` に組み立て直して **`check_impl` に通す**——「impl とは何か」の実装を
二重に持たないので、完全性検査・スーパトレイト義務・デフォルト本体・`Self` 置換がそのまま効く。

参照点は `type_implements` に集約（`validate_where_bounds` / `dyn_vtable_slots` /
`check_instance_method`）。`&self` のままで済むのは、**シグネチャが `TraitDef` 側から来る**ため
（`AdtDef::assoc` の事前投入が要らない）。`dyn_vtable_slots` だけは例外的に、まだ登録されていない
メソッド名をスロットに書く——スロットは名前でしかなく、解決は vtable 公開時だから。

コヒーレンス規則（決定的だが Rust より粗い）:
1. 1トレイトにつきブランケット実装は1つまで。構文的に判定、順序非依存。
2. 明示 `impl` が常に勝つ。Rust は明示 vs ブランケットの重なりを硬いエラーにするが、
   ブランケットがある型を覆うかは後から確立され得る境界に依存するので「重なるか」に
   単一時点の答えが無い。
3. 相互再帰的な境界は `BLANKET_BOUND_DEPTH` で打ち切る。

既知の制限だったもの: 一度も使われないブランケット実装の本体が型検査されなかった
（2026-08-04 に解消、本ファイル「ブランケット実装の本体を宣言時に型検査」参照）。

### その他

- `impl` のトレイト名に `::` パスを許可（従来は symbol のみで、他モジュールのトレイトを
  実装できなかった）。落とし穴: `a::b` という名前の *symbol* はセグメント1個であって
  `Value::Path` ではないので、実体化側は `OwnedForm::Path` で組み立てる必要がある。
- fasl v17。`TraitDef` の新フィールド4つ・`Namespace::blanket_impls`・`Docs::trait_methods`。
  併せて**トレイト差分が名前のみの比較**だったのを `TraitShape` 化した（`TypeShape` の前例に
  倣う。`TraitDef` に `PartialEq` を derive すると `FnSig::optionals` →
  `OptKeyParam::default: Option<Typed>` 経由で検査済み AST 全体に波及するため）。編集した
  トレイトが古いキャッシュから配られる、ユーザーに見えるバグだった。
- prelude: `Ord` が `Eq` を継承。`Eq` は `equals` のみ、`Ord` は `less` のみが実装必須になり、
  スカラ16 impl と `Error` 5 impl から約30個のメソッド本体が消えた。

## ストリーム/ファイル I/O をトレイトで再設計（2026-08-02）

第1版（CL のクラス階層を単一 `stream` 型に畳んだもの）を撤去し、同日入った Rust 同等の
トレイト機構の上で作り直した。撤去の判断と実測は `revert(streams)` コミットに、
第1版そのものの設計は git 履歴（`11707b5`）に残っている。

### 何が変わったか

| | 第1版 | 再設計 |
|---|---|---|
| Rust バックエンド | 1129行（合成ストリーム5種を含む） | 約300行（葉のみ） |
| `checker.rs` の特殊形 | `check_read`・format/print の型分岐 34行 | **0行** |
| 合成ストリーム | `StreamKind` 変種 + 全ディスパッチ地点の分岐 | typelisp の `defstruct` 8〜12行ずつ |
| ユーザ定義型がストリームになれるか | ✗ | ✓ |

### トレイト階層

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...全メソッドがデフォルト実装...)
(deftrait CharOutput ((OutputStream (Item char))) ...全メソッドがデフォルト実装...)
```

**関連型をピン留めしたスーパトレイト**が設計の要。`Item` を `char` に固定した上でしか
`read-line`／`write-string` のような文字前提のデフォルト本体は書けず、それができるので
具象型が書くのは `read-item`／`write-item` 各1つだけになる。`Item` を `InputStream` 側で
開いたままにしてあるのは、バイトストリームを足すときに並行の階層を作らずに済ませるため。

### 実装表現: なぜ不透明な i64 ハンドルなのか

具象ストリーム型は **heap 表現でなければならない**。`:dyn CharOutput` が箱詰めし、
`Vector<:dyn CharOutput>` が格納するからで、`random-state` と同じネイティブ表現ではどちらも
できない。GC ヒープの `BoxedObj` は typelisp-mem の `pub(crate)` なので、上位クレートで
定義した `StreamObj` を入れる変種を足すこともできない。

そこでストリームの**値**は `i64` を1つ持つ `defstruct` にし、OS リソースは `Interp` 上の
`StreamTable` に置いた。フィールドは `pub` でないのでハンドルを typelisp 側から捏造できず、
`BoxedObj`・GC・typelisp-mem を一切触らずに済む。スロットは**再利用しない**（閉じたハンドルは
別のストリームに化けるより、無効なままの方が良い）。

代償は、最後の参照が消えてもクローズされないこと。ただしこれは値型設計でも同じだった——
コレクタは cons アリーナ枯渇時にしか走らないので、ファイナライザは予測できない時点で動くか
一度も動かない。クローズは明示（`close`、または `with-open-file`）のままとした。

### 合成ストリームが typelisp に降りた

`broadcast-stream` の全実装が `Vector<:dyn CharOutput>` を持つ struct と `write-item` 1つ。
第1版では `StreamKind::Broadcast` 変種と、read/write/close/listen 等**全てのディスパッチ地点**に
分岐が要った。two-way / echo / concatenated も同様。入れ子（broadcast の中の broadcast）は
何もしなくても効く——合成ストリームもまた `:dyn CharOutput` だから。

`make-synonym-stream` は消滅した。第1版ではグローバル名を評価せず呼び出し元モジュールで
解決する checker 特殊形が要ったが、間接参照が欲しければ普通の struct を1つ書けばよい。

### 途中で判明した既存の穴（どちらも TODO.md に記録）

- **型引数の中に `()` を書けない**。`Result<(), FileError>` は型名レキサが `(`/`)` を扱えず
  `Path::from_segments` で **panic** する。第1版も避けていた（`Result<bool, E>` を使っていた）。
- **`match` の腕から `Result` の誤差型が推論されない**。`(result::ok v)` 単独では `E` が決まらず、
  期待型の無い `match` では兄弟の `err` 腕からも回復されない。戻り型を宣言した `io-ok`
  ヘルパで固定した。

### その他の落とし穴

- `pub deftrait` は非対応（仕様）。prelude の既存トレイトも `pub` なしで、可視性は
  `resolve_trait_name` が見ていないので実害は無い。
- 文字列に `iter` は無い。`(ref s i)` と `(length s)` のインデックスループを使う。
- `Vector` の要素数は `len`。ジェネリックな `length`（`Iter` 上の関数）は
  `Vector<:dyn T>` に対して要素型を推論できない。
- マクロ本体での分解は `sexpr-car`/`sexpr-cdr`（`car`/`cdr` は無い）。
- `impl Error FileError` は `unwrap-io` より**前**に置く必要がある（メソッドは定義順）。
- `char->string` を復活させた。第1版で入れて撤去時に外したが、`write-item` の実装で
  1文字を文字列にする必要があり戻ってきた。

---

## 型引数の `()` + `()` 型フィールドの compile 対応（2026-08-02）

TODO.md にあった「型引数の中に `()` を書けない」を解消し、続けてその過程で表面化した
「`()` 型のフィールドに compiled 表現が無い」も潰した。前者は読み取り／構文の話、
後者は値表現の話で、原因も直し方も無関係。

### 1. `Result<(), FileError>` が書けなかった理由は3層あった

`(`/`)` はリーダーのデリミタなので、トークンの中に入る経路がそもそも1つしか無い——
`extend_angle_token` の投機スキャン（山括弧が閉じるまで空白を越えて読み続ける、
`vector<:dyn drawable>` のための仕組み）だ。そこが `(` を見た瞬間に巻き戻していた。

- **リーダー**: 山括弧が開いている間に限り、隣接する `()` の2文字組だけを通す。
  単独の `(` は従来どおり投機を打ち切って巻き戻す——これが `(a<b c)` や `(string< a b)`
  の読み取りを一切変えないための条件。
- **`NameLexer`**: `NameTok::Unit` を追加。ちょうど2文字の `()` のときだけ出す。
  これが無いと `Ident("()")` になり、パス segment として `Path` に入ってしまう。
- **`types::parse_type_arg`**: そのトークンを `Type::Unit` として返す。

### 2. panic をエラーに

`Result<`（未終端ジェネリック。リーダーの投機が巻き戻して短いトークンだけが残る）は
`parse_qualified_generic` が segment 0 個で `Path::from_segments` に渡り、
`debug_assert` を踏んでいた。空 segment をその場でエラーにし、
`parse_type_name_rec` がトークン自身の `Loc` を貼るようにした（囲みのフォームではなく）。
`parse_qualified_generic`/`parse_type_arg` が `Result` を返すようになった以外、
呼び出し側の構造は変えていない。

### 3. `()` 型フィールドは interp でも壊れていた

パーサが直った直後に判明した。`(defstruct h (u ()))` は `rtvalue_to_struct_field` が
`Unit` を拒否して internal error。`Result<(), E>` の方はエラーにならないが、
`build_enum_value` が変換に失敗して **native-repr の `RtValue::Data` にフォールバック**
していた——だから interp では動いて見えて、compiled 境界を渡ろうとすると壊れる。

### 設計: 格納語と復号語が違う、唯一のフィールド種

unit 型は値を1つしか持たない。**スロットは情報を運ばない**ので、置く語は
「GC が `decode` して安全なもの」でさえあればよい。そこで:

| | 語 | 理由 |
|---|---|---|
| 格納 | `Value::Empty`（タグ付き `6`） | immediate nil は何も参照しない。interp 側 `rtvalue_to_struct_field` と同じ語にすることで、インタプリタが作った箱とネイティブが作った箱が一致する |
| 復号 | plain `0` | `compile-unit` が `Unit` 型の本体末尾に出すのと同じ表現 |

encode/decode の**どちらも渡された語を捨てて定数を出す**。これが他のどの kind とも違う点。

`ast_bridge::struct_field_kind` の kind は `Sexpr` の variant 番号を流用しているが、
`Unit` は `Sexpr` の variant ではないので借りる番号が無く、`nil` の `0` は
「表現不能」の catch-all が占有している。よって **11**（variant 番号の次）を新設した。

副作用として `Interp::is_jit_tier_ty` が `struct_field_kind(ty) != 0` で判定しているため、
`()` は**引数型・キャプチャ型としても**通るようになった。引数は
`encode_crossing_args` が plain `0` を送る（`compile-unit` の表現と同じ）。
`binding_kind` は `KIND_PLAIN` のままでよい——unit 値はヒープを指さないので GC ルート不要。

### 復号だけが型駆動になる

格納語 `Value::Empty` は、`Sexpr` 宣言のスロットが datum `()` を持っている場合と
**区別が付かない**。フィールド復号で形から型を復元できない唯一のケース。ただし
`()` 宣言のスロットは取り得る値が1つなので、宣言型だけで決まる——
`decode_field_typed` はまさにそのための関数なので、そこに arm を1つ足せば済んだ。
`match` の分解経路は `Pattern::Ctor::field_types`（check 時に焼き込み済み）を使う。
enum と struct の2つの腕が同じ復号をしていたので `decode_ctor_field` に括り出した。

### 変更点

- `src/read/reader.rs` — `extend_angle_token` に `()` 組
- `src/name_lexer.rs` — `NameTok::Unit`
- `src/types.rs` — `parse_type_arg` の `Unit`、空 segment のエラー化
- `src/compile/ast_bridge.rs` — `struct_field_kind` に `Type::Unit => 11`
- `src/compiler.rs` — `compile-tag-struct-field`（定数 `6`）/ `compile-sexpr-field`（定数 `0`）
- `src/eval/interp.rs` — `rtvalue_to_struct_field` / `decode_field_typed` /
  `decode_ctor_field`（新設）/ `encode_crossing_args`
- `src/compiler_island.bc` を再生成（`scripts/regen-compiler-island.sh`）

### prelude の回避策を撤去

`write-file-string`/`delete-file`/`rename-file` は `Result<bool, FileError>` を返し、
成功時に意味の無い `true` を運んでいた——`Result<(), E>` が書けなかった当時の回避策。
3つとも `Result<(), FileError>` に直した（`(io-ok ())`）。呼び出し側は
`((ok _) ...)` で受けるか戻り値を捨てるかのどちらかなので、既存コードへの影響は無い。

`with-open-file` マクロの `(io-ok ,result)` は元から body の型に対してジェネリックなので、
body が `()` を返す形（`(with-open-file (s ...) (write-string s "hi"))`）は以前から
インタプリタでは動いていた——ただし native-repr `RtValue::Data` として、である。
今回それが heap-repr になり、compiled 境界も渡れるようになった。

---

## 非最左スーパトレイトへの `:dyn` アップキャスト（2026-08-04）

`D(B,C)` の `:dyn D` を `:dyn C` へ。2026-08-01 に対象外として文書化した最後の
アップキャスト制限で、`D` の vtable が `[B の…, C の…, D 自身の…]` である以上
`C` のスロットは 0 始まりにならず、retype では `:dyn C` 呼び出し側が焼いた定数が
別のエントリを指してしまう、というものだった。

### 鍵は「どこなら具象型がまだ分かるか」

差し替え先の表は **(具象型, trait) の組**で決まるが、アップキャスト地点にあるのは trait だけで、
具象型はもう箱の中に消えている。逆に**箱詰め地点**には両方ある。そこで
`Expr::DynBox` に `supers`（推移的スーパトレイト閉包ぶんの slot 表）を持たせ、
`Interp::register_dyn_box` がそれら全部を intern して
「(vtable id, trait id) → vtable id」を登録する。アップキャストは実行時にこの表を引き、
**同じ具象値の周りに差し替え先の表で箱を作り直す**だけになる。

新しい表は2階建てにならない: 値が (vtable id, trait id) の2つ組で一意に決まるので、
`HashMap<(u32,u32), u32>` 1本で済む。trait 側を整数 id にしたのはコンパイル済みコードが
定数として焼けるようにするため（`Interp::trait_id_for`、vtable id とまったく同じ扱い）。

### `supers` は閉包すべてを載せる

「接頭辞にならない相手だけ」に絞る案は誤りだった。アップキャストは連鎖しうるので、
`D` から見れば接頭辞の相手でも、途中で作り直した `C` の箱から見ると接頭辞でないことがある
（`C(X,A)`、`D(B,C)`、`B(A)` で `D→C→A`。`A` は `D` の接頭辞だが `C` の接頭辞ではない）。
閉包全部なら考えることが無くなり、表は (型, メソッド) の名前列でしかないので安い。
同じ理由で登録も閉包内の**全ペア**に対して行う——表の値は
「この具象型がその trait に使う vtable」でしかなく、2つの trait の関係の向きに依存しない。
許可の判断はチェッカーの仕事なので、読まれない項目があっても無害。

`supers` の slot 表は `dyn_vtable_slots` を再実行せず、箱本体の slot 表から**メソッド名で
引き写す**（線形化はメソッド名ごとに1スロットなので、スーパトレイトの `vtable_order` は
必ず部分列）。箱がディスパッチする表と食い違いようがなく、箱本体が通した受理検査を
スーパトレイト側で落とす心配も無い。

### 4つの層すべてに同じ表が要る

| 層 | 表 | 変換 |
|---|---|---|
| インタプリタ | `Interp::dyn_upcasts` | `Expr::DynUpcast` の評価 |
| JIT | `typelisp_rt::UPCASTS`（`upcast_define` で publish） | `rt_dyn_upcast` |
| AOT | 同上（起動時に `rt_upcast_set` の定数列） | 同上 |
| 島 | — | `dyn-upcast` タグ → `compile-dyn-upcast` |

島の `compile-dyn-upcast` は `compile-dyn-new` と同形（スロット0が定数、値は `kind = 2` として
`compile-call-args` が root する）。`compiler_island.bc` を再生成した。

publish のタイミングだけ注意が要る: 箱詰めがインタプリタ側で起きてアップキャストが
コンパイル済みコードで起きる経路があるので、`Expr::DynBox` の評価では自分の表だけでなく
`supers` の表も `publish_vtable` する。

### 影響範囲

- `Expr` に `DynUpcast` を追加、`Expr::DynBox` に `supers` を追加 → FASL v18。
- `ast_bridge` の `vtables` 引数は `DynTables`（vtable id 表 + trait id 表）に置き換え。
  翻訳の入口も入れ子スコープの受け渡しも両方を必要とするので、引数を2本に増やさず束ねた。
- チェッカーの `upcast_dyn` は「接頭辞かどうか」を**可否ではなくコスト**の判定に変えた
  （接頭辞なら従来どおり式に触らない retype、そうでなければ `DynUpcast` を挿入）。
  受理条件は「推移的スーパトレイト閉包にあること」と「ピンが鎖に沿って一致すること」の2つ。
- テストは3層とも: `dyn_dispatch_test.rs`（interp、連鎖アップキャスト・`match` 透過性含む）、
  `compile_test.rs`（JIT、箱詰めがインタプリタ側の経路を含む）、`compile_file_test.rs`（AOT）。

---

## ブランケット実装の本体を宣言時に型検査（2026-08-04）

2026-08-01 の「既知の制限」——一度も使われないブランケット実装の本体は型検査されない——を解消。
Rust は `impl<T: Ord> Clamp for T { ... }` の本体を、使われるかどうかに関係なく宣言時に1回
検査する。こちらもそうした（`Checker::precheck_blanket_impl`）。

### 「受信型が未知だから検査できない」は誤り

当時の理由づけ（TODO.md にもそう書いてあった）は「受信型が未知の本体を検査する手段が無い」
だった。しかし**ジェネリック `defun` の本体は現に検査している**——型パラメータは未解決の
`Type::Named` として残り、それに対するメソッド呼び出しは `Env::bounds` 経由で
`Expr::TraitCall`（診断専用ノード）に解決される（`check_instance_method` の bounds 分岐）。
ブランケット実装の対象変数も同じ立場の型変数でしかない。足りなかったのは手段ではなく、
その手段を呼ぶ経路だった。

### スコープに入る境界は2つ

1. impl の `(where ...)`。`subst_method_item` がメソッド自身の `where` 節へ既に併合するので、
   `parse_defmethod_sig` がそのまま拾う。
2. **実装中のトレイト自身**。`impl<T> Clamp T` の中では `T` は `Clamp` を実装しているから、
   本体が `self` に対して兄弟メソッドを呼べる（Rust の暗黙の `Self: Trait`）。
   関連型のピン（この impl の `(type Item ...)`）も一緒に載せるので、
   兄弟メソッドの戻り型が関連型でも具体化される。

継承メソッドは 1 の側で自動的に効く（bounds 分岐は `trait_method` でスーパトレイト鎖を辿る）ので、
`(deftrait Doubled (Ranked) ...)` のブランケットは `(where (Ranked T))` を書かなくても
`(rank self)` を呼べる——Rust で `Self: Doubled` が `Self: Ranked` を含意するのと同じ。

### 検査結果は捨てる

登録先の型が無いので何も登録できないし、するべきでもない。実体化
（`materialize_blanket_impl`）は従来どおり対象ごとに本体を検査し直す。この先行検査が
捕まえるのは**どの対象でも誤り**であるもの（戻り型不一致、未定義関数、境界が正当化しない
メソッド呼び出し）だけで、それを *使った側の無関係なフォーム* ではなく impl の位置で報告する
のが値打ち。

### 副産物2つ

- `check_impl` のメソッド項目書き換え（`Self`/関連型の置換 + `where` 併合）を
  `subst_method_item` に抽出。先行検査は `Self` を**具体型でなく対象変数**に置換して同じ関数を通す。
  「impl のメソッド項目とは何か」の実装を二重に持たない、という `materialize_blanket_impl` と
  同じ方針。
- `type_implements` が**開いた型**（自由型変数を含む型）に対して実体化を要求しないようにした
  （`type_is_open` で判定）。先行検査中は受信型が型変数なので、境界の無いブランケット
  （`(impl<T> Show T)`）では「覆う」と答えた直後に型変数での実体化を要求してしまい、
  `check_impl` が「unknown type `t`」で落ちる。覆う判定自体は正しい——その型変数が表す具体型は
  どれも覆われ、各々が到着時に自分の実体化を要求する。

### テスト

`tests/blanket_impl_test.rs` に5件追加: 未使用ブランケットの戻り型誤り・未定義関数呼び出しが
宣言時に落ちること、境界が正当化する呼び出しと兄弟メソッド呼び出しは通ること、
どちらでもない呼び出しは落ちること。

### 残る隣接の穴

`deftrait` の**デフォルトメソッド本体**は、どの `impl` にも使われなければ依然として検査されない
（`TraitDefault` に保存するだけで、`impl` が replay して初めて検査される）。ブランケット実装の
本体とは別の機構なので今回は触らず、TODO.md に項目として残した。`Self` はトレイトのシグネチャ中
では既に型変数なので、同じ道具立てで検査できるはず（→ 下の「デフォルトメソッド本体を宣言時に
型検査」で、まさにその見立てのとおりに解消した）。

---

## デフォルトメソッド本体を宣言時に型検査（2026-08-04）

上の項の「残る隣接の穴」を解消。`deftrait` のデフォルト本体も、宣言の時点で1回
`Self` を型変数のまま検査する（`Checker::precheck_trait_defaults`）。

実装はブランケット実装の先行検査と同じ道具立てで、対象変数が `Self` に固定されただけ:
`parse_defmethod_sig_inner(abstract_receiver = true)` で受信型が型変数のシグネチャを読み、
`Self: そのトレイト` を `Env::bounds` に置いて `check_seq` を回す。見立てどおり、
足りなかったのは手段ではなく経路だった。

- 境界は1つでよい。`Self: そのトレイト` があれば継承メソッドも呼べる——bounds 分岐が
  `Registry::trait_method` でスーパトレイト鎖を辿るため（ブランケット側と同じ理由）。
- 関連型は**自分自身にピン留め**する（`Item` は型変数 `Item`）。兄弟メソッドが `Item` を返す、
  という照合が `Item` の正体を知らないまま通る。
- 検査結果は捨てる。`impl` が継承するたびに、`Self` が具体型の状態で検査し直される。
- `check_deftrait` は `TraitDef` を登録した**後**で検査する。本体が `self` に対して
  自分自身のメソッドを呼ぶので、登録前ではその呼び出しが解決できない。

### prelude のバグを1件その場で捕まえた

`InputStream::read-item` は**本体の無いシグネチャに docstring を書いていた**。
syntax.md が明記しているとおり（CL の `defun` と同じ規則）、後ろにフォームが続かない末尾の
文字列は docstring ではなく**本体**なので、これは「`Option<Item>` を宣言して `string` を返す
デフォルト実装」だった。`read-item` を省略する `impl` が1つも無かったので、それまで誰も
replay せず気づかれていない。コメントに落として解消した。この穴のためにこの検査を入れた、
という類の実例がいきなり出たかたち。

### テスト

`tests/trait_test.rs` に6件追加: 未使用デフォルト本体の戻り型誤り・未定義関数呼び出しが
宣言時に落ちること、兄弟メソッド／継承メソッドの呼び出しは通ること、関連型に対して
照合されること、検査に落ちた `deftrait` の後続フォームが壊れないこと。

---

## `match` の腕どうしが型引数を埋め合う（2026-08-04）

「`match` の腕から `Result` の誤差型が推論されない」を解消。`(result::ok v)` は `T` しか
決めず、`(result::err e)` は `E` しか決めない。期待型の無い位置（`let` の初期値など）に
書かれた `match` ではどちらの腕も単独で型付けできず、prelude は戻り型を宣言した `io-ok`
ヘルパで `E` を固定していた。その `io-ok` は今回削除した。

### 推論の穴を `!` で表す

`check_construct` が型引数を決められなかったとき、**`match` の腕を試し検査している間だけ**
（`Checker::infer_probe`）エラーにせず `Type::Never` を置く。`!` は底の型なので
`Result<string, !>` は `Result<string, E>` の部分型であり、嘘ではない。
`join_types` を型引数の中まで再帰する `merge_holes` に一般化すると、
`Result<string, !>` と `Result<!, FileError>` の合流がそのまま `Result<string, FileError>` になる
——制約ソルバがやる腕どうしの単一化を、穴の出どころが1箇所しか無いので構造的に書ける。

### 穴は AST に残さない

試し検査で作った `Typed` ノードは**必ず捨てる**。穴を1つでも作った腕（および単に失敗した腕）は
第2パスで、腕全体が持ち寄った型を期待型として**検査し直す**。持ち寄っても穴が残るなら期待型
無しで検査し直し、その腕自身の元のエラー（`cannot infer type argument ...`）がそのまま出る。
つまり `Never` を型引数に持つノードが retain されることはない。判定は穴の**個数**（`probe_holes`）で、
スナップショット＆巻き戻し方式にした。こうすると入れ子の `match` が自分の穴を自分で解決した場合に
外側の腕まで再検査せずに済む。

- 試し検査は診断も残さない（`diag_mark`/`diag_rollback` で `errors`/`warnings`/`type_uses` を
  巻き戻す）。巻き戻さないと recover モード（LSP）で同じ診断が二重に出るか、捨てた木の
  セマンティックトークンが残る。
- `local_refs` は位置キーの map なので巻き戻し不要、`spec_memo`/`spec_pending` も
  再検査が同じ実体化を要求し直すだけなので不要。
- 腕は `Vec<Option<Arm>>` で持つ。第2パスが埋めるまで穴が空くが、腕の順序は
  `match` の意味そのもの（先勝ち）なので位置は動かせない。

### 適用範囲

穴は `match` の腕限定。`(let ((r (result::ok 1))) ...)` のように腕でない位置で決まらない
型引数は、従来どおりその場でエラー。`Option` と `Result` のように型構成子が違う腕どうしは
合流しないので、これも従来どおり両方の腕が自分のエラーを出す。

`if` の分岐（したがって `cond`）は今回の対象外で、`(if c (result::ok 1) (result::err e))` を
期待型の無い位置に書くと従来どおり落ちる。`join_types` の一般化は共有しているので合流規則自体は
同じだが、試し検査と再検査の段取りは `check_match` の中にあり、`check_if` へ持って行くには
`&mut Heap` を跨ぐ共通化が要る。`if-let`/`while-let` は `match` へ展開されるので対象内。

### テスト

`tests/check_test.rs` に4件（腕どうしの合流、具体型の兄弟からの受け取り、腕でない位置は
従来どおり、型構成子が違う腕は従来どおり）、`tests/stream_test.rs` に1件
（`with-open-file` を `let` に束縛する——`io-ok` を消して初めて通る形）。

---

## 孤児になっていた引数無し `read-line` の始末（2026-08-04）

`(read-line)`（引数無し・標準入力）は 11707b5（ストリーム第1版）で `registry.rs` の
自由関数エントリが削除され、`src/eval/interp.rs` の `eval_read_line` だけが残っていた。
checker が名前を知らないので**到達不能な死にコード**。同じ名前は prelude の
`CharInput::read-line`（引数1個）が引き継いでいる。

放置されていたのは `examples/projects/` の3本（todo-cli / mini-lisp / expr-eval）が
旧綴りのままだったせいで、3本とも check で落ちていた（`no such function: read-line`）。
`docs/functions.md` §15 の表にも `(read-line)` が残っていた。

標準入力への道を1本にする方針で始末した:

- `eval_read_line` とディスパッチ分岐を削除。
- サンプル3本を `(read-line *standard-input*)` に。標準ストリームは prelude の
  `*standard-input*`（`standard-stream`、`CharInput` 実装済み）。
- `docs/functions.md` §15 の行を削除し、代わりに「標準入力を読むのは `*standard-input*` に対する
  `CharInput` のメソッド」と §18.1 への案内を書いた。

`print`/`println`/`format` は書式展開の近道として標準出力側に残る（これらは flush まで面倒を
見るので、プロンプトの可視性という別の役割がある）。読む側に同種の近道を残さないのは、
`read-line` が**トレイトメソッドと名前を共有してしまう**ため——0引数と1引数で解決経路は
分かれるので共存自体はできるが、同じ名前が2つの機構に属する状態は説明が増えるだけだった。

副産物として、`examples/projects/expr-eval` が check を通るようになった結果、**別の既存バグ**が
表に出た（`labels` の中で作った enum 値の変種タグが壊れる）。TODO.md に再現手順付きで記録した。

---

## compiled 側が ADT の型名を修飾せずに書いていたバグ（2026-08-04）

前項で `examples/projects/expr-eval` が実行時に落ちたのを追った結果。TODO.md には
「`labels` の中で作った enum 値の変種タグが壊れる」と書いたが、`labels` は症状であって
条件ではなかった。**非 root モジュールの ADT を compiled 経路で構築すると、値が持つ型名が
修飾されない**。`labels`/`lambda` の本体は常に compile されるので、そこが目に付いただけ。

### 値の実行時 identity は「型名の文字列」

`BoxedObj::Struct`/`Enum` は型名を文字列で持つ（`Heap::alloc_struct`/`alloc_enum`）。
これが値の実行時 identity で、次の全員がそれを比較・解決する:

| 読む側 | 用途 |
|---|---|
| `match_pattern`（enum/struct 腕） | 別 ADT との偽陽性防止（Sexpr ユーザ ADT 計画 §2 で追加） |
| `rt_sexpr_instance_test` | compiled 側の downcast（`(the T p)` / 型名先頭パターン） |
| `sexpr_equalp_val` | `equalp` の「同じ型か」 |
| `format.rs` / `main.rs` の印字 | 変種名の逆引き（`Path` にパースして registry を引く） |
| `Interp::print_object` | `print-object` 実装の探索 |

インタプリタは書く側も読む側も `Path::to_string()`（修飾済み）。ところが
`ast_bridge` は3箇所とも `Path::local()`（最終セグメント）を書いていた
（`translate_construct` / downcast `pat-ctor` / `pat-typetest`）。compiled 側どうしは
一貫していたので閉じた世界では動き、**境界を跨いだ瞬間に壊れる**:

- compiled で構築 → interp の `match`: `internal error: no matching match arm`
- interp で構築 → compiled の downcast: **黙って**マッチせず catch-all へ
- 印字: `(<unknown-variant> 5)` / `#<pt 1 2>`（`#<m::pt 1 2>` のはず）
- `equalp`: 同じ型の値どうしが false
- `print-object`: 実装が見つからず既定の描画に落ちる

修正は `.local()` → `.to_string()` の3箇所。関数呼び出し側は同じ問題を既に解いていた
（`translate_call`/`translate_fnref` は `m::inc` を `tl_m::inc` にマングルする。interp
クロージャ削除のときに直した）——型名側が取り残されていた。

island（`compiler.rs`）と prelude は `module` を1つも持たない＝全部 root なので、
`local()` と `to_string()` が同じ文字列。**既存の bitcode は再生成不要**で、
`compile-option-type-name` が焼き込む `"option"` もそのまま正しい。

### なぜテストが1件も落ちなかったか

`tests/` の全ソースは root 名前空間で checker に渡る。root では `local()` == `to_string()`
なので、compile 系のテストは1件も再現しない。再現には `(module ...)` か
（ファイル＝モジュールなので）プロジェクト構成が要る。回帰テストは4件とも
`(module m ...)` を張って書いた（`tests/compile_test.rs` 末尾）。修正を戻すと4件とも落ちる
ことを確認済み。

### 同族の2件目——組み込み型の*分類*も最終セグメントで見ていた

追っている途中で見つかり、同じコミットで直した。`ast_bridge` は組み込み型を
`type_name.local() == "vector"` / `"hashtable"` / `"llvm-*"` / `"scope"` で*分類*して
専用ノード（`vector-op`/`hashtable-op`/`llvm-op`）へ振り替えている。組み込み名の再定義は
root では拒否されるが、**モジュールの中では通る**ので、`(module m (defstruct vector ...))`
を書くと compiled 経路だけがユーザ型を組み込みと誤認する:

- `vector` を名乗るユーザ struct の `len` → interp は `300`、compiled は `1`
  （組み込みの「フィールド数」を読む）。**黙って違う値**。
- `hashtable` の方は `BoxId does not hold a HashTable` で**プロセスが abort**。

型名の identity（上）とは別の機構だが、間違いは同一——**型の identity は経路全体**であって
最終セグメントではない。`is_builtin_type(p, name)`（`p.is_simple() && p.local() == name`）を
1つ置いて、`ast_bridge` の6箇所と `interp.rs` の compile 駆動側4箇所を通した。駆動側も
一緒に直すのが必須で、片方だけだと「ast_bridge は実メソッド呼び出しに落とすのに駆動側は
組み込みだと思って compile しない」＝シンボル欠落になる。判定の形は既に同ファイルの
`is_llvm_handle_ty`/`is_enum_ty` が `p.is_simple()` 付きで書いていた——揃っていなかっただけ。

組み込みの受け側は常に root なので、モジュールの中から本物の `Vector<i32>` を使う経路は
従来どおり `vector-op` に落ちる（回帰テストで固定した）。

---

## 「型の identity」を書き間違えられない形にする（2026-08-04）

同じ日に同じ原因のバグを 2 件踏んだ（`2dd5171` / `7aebfd2`）ので、個別の修正ではなく
**書く時点で迷わない形**にした。原因はどちらも `Path::local()`（最終セグメント）を
型の identity として使ったこと。片方は値の実行時型名として、もう片方は組み込み型の分類として。
**同じ概念が 2 通りの綴りで書け、どちらも `String` なのでコンパイラも読み手も気付けない**のが
本当の問題だった。

### 打った手

**1. `Path::local()` → `Path::last_segment()`（57 箇所）**。`local()` は「ローカルな名前」と
読めてしまうが、返すのは「最後のセグメント」でしかない。改名すると
`last_segment() == "vector"` と書いた瞬間に怪しいと見える。あわせて `Display` の doc に
「**これが identity の綴り**」と書いた。合法な用法の大半は `parent()` + `last_segment()` の
名前表引き（モジュールの表は最終セグメントで引く）で、改名後の方が読みやすい。

**2. 組み込み判定を 1 対の関数と 3 本の const に集約**（`src/types.rs`）。
`path_is_builtin(p, name)` / `path_is_builtin_any(p, names)` はどちらも `is_simple()` を含む。
名前リストは意味ごとに別名で持つ——`LLVM_HANDLE_TYPES`（型としてのハンドル 5 種）と
`LLVM_METHOD_RECEIVER_TYPES`（メソッドを持つ 4 種）は今まで「たまたま違う」状態だったので、
名前を付けて違いを意図として固定した。`NATIVE_LOWERED_PRIMITIVES` も同様。

この過程で、**同じ間違いが `src/eval/interp.rs` の compile 駆動側に 5 箇所残っている**のが
見つかった（llvm ハンドル ×2、プリミティブ列 ×3）。`7aebfd2` は `ast_bridge` 側だけを締めて
いたので、両者が食い違ったまま——ast_bridge が実メソッド呼び出しに落とすものを駆動側が
「組み込みだから compile 不要」と判断する、シンボル欠落経路だった。同じ関数・同じリストを
見せて解消。

**3. 実行時 identity の綴りを 1 モジュールに**（`src/type_key.rs`）。書く
（`type_key_of` / `alloc_typed_struct` / `alloc_typed_enum`）・比べる（`heap_type_is`）・
読み戻す（`heap_type_path`）の 3 方向にそれぞれ 1 つだけ入口を置いた。`Heap::alloc_struct` /
`alloc_enum` / `struct_type_name` / `enum_type_name` を直接呼ぶのはこのモジュールの中だけ。
`crates/typelisp-rt` の shim（`rt_data_new` / `rt_sexpr_instance_test`）は compiled コードから
文字列を受け取る境界なので対象外——その端は「`ast_bridge` が焼き込むリテラルが
`type_key_of` の出力である」ことで担保する。

**4. 監視テスト `tests/type_identity_guard_test.rs`**。`editor_keyword_sync_test` と同じ
「ソースを読んで不変条件を縛る」方式で、`src/**.rs` を走査して 2 つを禁じる:

- 最終セグメントと文字列リテラルの比較（→ `path_is_builtin` を使え）
- `type_key.rs` の外での `alloc_struct` / `alloc_enum` / `*_type_name`（→ `type_key` を使え）

例外は `// type-identity-ok: <理由>` を当該行か直上のコメント塊に書く。**理由を書かせるのが要**で、
現在の例外は 6 箇所（root 組み込みをリテラルから組み立てる 4 箇所、`equalp` の「保存済みキー
どうしの比較」2 箇所）と `is_self_tvar`（`Self` は型変数であって組み込み型ではない）。
失敗メッセージには「どちらの問いを聞いているのか」の表を出して、反射的にマーカーを足す方向へ
逃げないようにした。

**5. 境界跨ぎ適合テスト**（`tests/compile_test.rs` 末尾）。`(module m ...)` の下で
構築側（interp / compiled / `labels` 内）× 消費側（interp の `match` / compiled の `match` /
compiled の downcast / 印字 / `equalp`）を回す 5 本。綴りの規約が将来変わっても、
**このクラスのバグは振る舞いで捕まる**。

### 実効性の確認

手当てそのものが効くことを、壊して確かめた:

| 壊した箇所 | 落ちるもの |
|---|---|
| `ast_bridge` の construct を `last_segment()` に戻す（＝`2dd5171` の再現） | 適合テスト **5 本すべて** |
| `path_is_builtin` を素の比較に戻す（＝`7aebfd2` の再現） | 監視テスト規則 1（file:line 付き） |
| `interp` の `alloc_enum` を直接呼びに戻す | 監視テスト規則 2（file:line 付き） |

なお `type_key_of` 自体を壊すと**両側が対称に変わる**ので適合テストは印字の 1 本しか落ちない。
「両側が同じ間違いをすれば気付けない」のは原理的な限界で、だから入口を 1 つにする（間違えようが
ない）方を主にし、テストは非対称の検出に使う、という役割分担にしている。

### 注意

`local()` は消えたので、これより前のログ中の `Path::local()` という記述は
`Path::last_segment()` と読み替えること。

---

## ストリームの残り 3 項目 + パス名層（2026-08-05）

TODO.md に 1 項目だけ残っていた「ストリームの未実装分」を全部片付けた。内訳は
`fresh-line` が `file-stream` 専用だったこと、`read` のストリーム版が無かったこと、
`format` のストリーム宛が無かったこと、パス名層が無かったこと（ファイルは文字列で指していた）。
途中で、ストリームとは関係なく**トレイトのデフォルト本体がモジュールを跨ぐと壊れる**既存バグが
出てきたので、それも直した（下記 5）。

### 1. `fresh-line` を「ストリームが答える問い」にした

行頭かどうかを覚えているのはストリーム自身なので、`CharOutput` に 2 つのメソッドを足した:

- `at-line-start` — デフォルト本体は `false`。
- `fresh-line` — `(if (at-line-start self) () (write-item self #\newline))`。

デフォルトを `false` にしたのは、**分からないなら改行を書く方が安全**だから（CL の
`fresh-line` の目的は「次の出力が行頭から始まること」を保証することで、余分な空行より
行の連結の方が害が大きい）。組み込みのリーフストリームはネイティブ層の `last_written` から
答える。合成ストリームでは `broadcast-stream` だけ `fresh-line` 自体を上書きしてある——
成分ごとに行頭かどうかは違うので、全体で 1 回判断すると既に行頭の成分に空行が入る。

旧 `(pub defun fresh-line ((s file-stream)) ())` は削除した（同名のトレイトメソッドが
`file-stream` にも同じ動作で生えるので、呼び出し側は変わらない）。

### 2. `read` のストリーム版 = `read-sexpr` + `PeekInput`

**パースは書き直していない。** 新しいのは「次のデータがどこで終わるか」を見つける
スキャナだけで、見つけた**テキストをそのまま `read` に渡す**。typelisp で 2 つ目の文法を
持つと `src/read/reader.rs` と歩調を合わせ続ける羽目になるが、スキャナが知るべきことは
「何が何を閉じるか」だけで、はるかに間違えにくい。構文エラーの文面も reader のものになる。

テキストは**逐語的に**貯める（空白もコメントも込み）。`Vector<:dyn CharOutput>` のような
総称型トークンは 1 行に収まっている間だけ 1 シンボルとして読めるので、空白を正規化すると
意味が変わりうる。

原子（atom）の終わりを知るには終端文字を**見て、戻す**必要がある。これがデフォルト本体を
書けない唯一の入力操作なので、`PeekInput (CharInput)` を別トレイトとして足した
（`unread-char` が必須、`peek-char` はデフォルト）。ネイティブのリーフ 3 種はネイティブ層の
pushback で実装し、それ以外は `make-peek-stream` で包めば得られる——`peek-stream` は
`:dyn CharInput` と `Option<char>` を 1 つ持つだけの、他の合成ストリームと同じ構造体。

戻り値は `Result<Option<Sexpr>, ReadError>`。入力末尾は `Ok(none)` にした（読み切りループが
値で終われる。エラーで終わると「構文エラー」と区別できない）。名前が `read` でないのは、
文字列を取る既存の `read`（CL の `read-from-string`）と単一・静的ディスパッチでは
同名にできないため。

コスト: テキストの蓄積は 1 文字ごとの `append` なので、1 つのデータの長さに対して O(n²)。
prelude の `read-line`/`read-all` と同じ形で、実用サイズのフォームでは問題にならないが、
巨大な 1 フォームを読ませると効いてくる。可変長文字列バッファが要るなら、この 3 つを
まとめて直すのが筋。

### 3. `format` のストリーム宛

`Checker::check_format` は destination を**期待型なしで**先に検査し、`bool` ならこれまで通り、
それ以外なら `(write-string dest (format false control args...))` へ**書き換えてから
check し直す**。自前でメソッド呼び出しを組み立てないのは、`write-string` の解決先が受け手
次第（具象型 / `:dyn CharOutput` / `where` 境界付き型変数）で、その 3 通りを特殊形が
知らずに済ませたいから。destination は 2 回検査されるが、検査に実行時作用は無い。

戻り値は `write-string` のもの、つまり `()`（CL の `(format stream ...)` も `nil`）。
`bool` 宛が文字列を返すのと非対称だが、両方返そうとすると一時変数を導入する必要がある。
`bool` でもストリームでもない destination は、`write-string` が無いという報告ではなく
destination の型エラーとして報告する（型変数と `:dyn` は書き換え後に判定されるので素通し）。

### 4. パス名層（CLHS 19）

`defstruct pathname`（directory / name / extension / absolutep）と、パス名指定子トレイト
`Pathish`（`string` と `pathname` が実装）。**ファイルを名指しする関数は全て
`(where (Pathish P))` に変えた**ので、文字列を渡す既存コードはそのまま、`pathname` も
そのまま渡せる。`string` 側の `namestring` は自分自身を返すだけなので、文字列経路に
パースのコストは乗らない。

分解・再構成は純粋な文字列処理なので**全て prelude（typelisp）で書いた**——ネイティブ側は
`namestring` が描いた文字列しか見ない。CL の `pathname` 関数だけは型名と衝突するため
`to-pathname`。ホスト/デバイス/バージョン成分、ワイルドカード、論理パス名は採用しない
（この処理系が走らないファイルシステムのための機能）。

### 5. 途中で見つかった既存バグ: トレイト impl のメソッドが private だった

`(impl CharInput my-type)` が**ファイル（＝モジュール）の中では通らない**ことに気付いた。
`impl` のメソッドは `public: false` で登録されていて、`assoc_visible` は「現在の名前空間が
その型のモジュールの中か」で判定する。ところがトレイトのデフォルト本体は**トレイトの
名前空間**で検査される（`check_defmethod_in` の `body_ns`。デフォルト本体の自由名を
トレイト側で解決するための正しい仕組み）ので、実装型が別モジュールにあると構造的に
「外」になり、兄弟メソッドが見えない。prelude の型は全部 root にあり、root の項目は
どこからでも見えるので、prelude 内では露見していなかった。

呼び出し側も同じ理由で壊れていた（`(module m ...)` の中で `impl Eq` した型に対する
`(equals ...)` が root から呼べない）。`pub` は `impl` には書けない（docs/syntax.md §pub）
ので、**private な trait impl は表現できないものを実装が勝手に作っていた**ことになる。
Rust も trait impl に可視性を持たない（トレイトのメソッドは値がある所ならどこでも呼べる）。
そこで `check_impl` は常に public でメソッドを登録するようにした。回帰テストは
`tests/trait_test.rs` の 2 本（デフォルト本体がモジュールを跨ぐ / 実装モジュールの外から呼ぶ）。

なお可視性フラグは fasl に載る。prelude の fasl は SOURCE のハッシュで無効化されるので
自動で作り直されるが、**この変更以前に作られたユーザモジュールの `.fastl` は古い private の
まま**なので、変な「no such function」が出たら `~/.typl/cache` を消すこと
（`registry.rs` 変更時と同じ罠——implementation-log の「古い fasl キャッシュ」の項）。

### 6. その他

`src/prelude.rs` の `SOURCE` は `r#"..."#` から `r##"..."##` にした。typelisp 側で
`"#"` や `"#\\"` のような文字列リテラルを書くと `"#` が Rust の raw string を閉じてしまう。

テスト: `tests/stream_test.rs` に 20 本追加（fresh-line / format 宛先 / `read-sexpr` /
peek・unread / 包み方）、`tests/pathname_test.rs` を新設（15 本）、`tests/trait_test.rs` に
可視性の回帰 2 本。

## edition 2018 → 2021（2026-08-05）

`cargo build --all-targets` が出していた `non_fmt_panics` 警告 2 件
（`tests/compile_test.rs` の `assert!(..., "built by {build}, printed as {got}")` と
`tests/format_test.rs` の `assert!(..., "got {s}")`）が発端。edition 2018 では
`assert!`/`panic!` に**引数無しのリテラル 1 個**を渡すと書式文字列として扱われず、
`{build}` が文字どおり出力される（`assert_eq!` のメッセージは常に `format_args!` を
通るので、同じファイルの `assert_eq!(..., "{build}")` は警告対象外だった）。まず明示引数
（`"built by {}, printed as {}", build, got`）に直して警告を解消し、その上でワークスペース
3 crate すべてを edition 2021 に上げた。

edition 2021 の破壊的変更のうち、このコードベースに当たるものは無かった:

- **配列の `into_iter()`**（`&T` → `T`）: 該当なし。`x[..].iter()` はすべてスライスで、
  スライスの `iter()` は edition に依らず `&T`。
- **クロージャの分割キャプチャ**（Drop の順序・タイミングが変わりうる）: `impl Drop` は
  `crates/typelisp-mem/src/heap.rs` の `Heap` だけで、値ごとクロージャに捕獲していない。
  GC ルートの LIFO 不変条件（`sync_roots`）に影響する経路は無い。
- **`TryFrom`/`TryInto`/`FromIterator` の prelude 入り**、**予約プレフィックス**
  （`ident"..."`）、**マクロ `:pat` の or パターン**: いずれも当たればコンパイルエラーに
  なるが、発生しなかった。
- **`panic!`/`assert!` の書式文字列化**: 上記 2 件が唯一の該当箇所で、先に潰してある。

副次的に、「`u32::try_from` は edition 2018 では暗黙にスコープに無いので範囲チェック +
`as u32` で済ませた」（`int->char` の項、2026-07-01）の制約は無くなった。既存コードは
そのままで正しく動くので書き換えていない。

非仮想 workspace のルートパッケージが edition 2021 になったことで、feature の
resolver も v2 が既定になる。依存グラフに変化は出ていない（`Cargo.lock` 差分無し）。

テスト: `scripts/test-serial.sh`（lib + tests/ 全バイナリを直列・単スレッド）で全緑、
`cargo build --workspace --all-targets` は警告ゼロ。

## 位置情報を cons セルに持たせる（2026-08-09）

Stage C（checker を core IR の生産者に切り替える、`docs/dev/` のプラン参照）の途中で
「fasl に焼くと位置が落ちる」問題に当たったのが発端。今日は `Typed.loc` がノードの中の
フィールドなので serde でそのまま JSON に入るが、core 形ではノードが cons セルになり、
位置は「セルのアドレスをキーにした side table」に移るので、セルを作り直すと必ず失われる。

`OwnedForm` に `Loc` を足すかという個別の判断ではなく、置き場所の設計を変えた。reader が
読むのは 1 つの S 式で、位置はその S 式に付帯する性質なのだから、セルの中にあるべきである。

### 外部表であるがゆえに要っていた 4 つの規律

1. reader の 2 表（`cons_locs`/`elem_locs`）は読み込みバッチごとに `clear_cons_locs` で
   全消し。でないと解放済みアドレスの再利用で古い位置が別のフォームを指す
2. 全消しされると lowered なノード（= 登録された関数本体）の位置が消えるので、全消しされ
   ない 3 つ目の表 `code_locs` が要る
3. 全消しされないなら古いエントリを許容できないので、GC の sweep が回収セルのエントリを
   消す仕事を負う
4. 依存ファイルの読み込みが外側ファイルの位置を消さないよう `read_all_in_keep_locs` と
   `read_all_in_keep_locs_spanned` という別入口が要る

`Cell` が 2 スロット持つと、この 4 つが 1 つの不変条件に置き換わる: **`Heap::cons` が
払い出すときに両スロットを 0 にする。** 位置はセルと運命を共にするので、全消しも sweep での
削除も opt-out も要らない。上の 4 つは全部消えた。

### なぜ 2 スロットか

リスト形と要素は別の span で、どちらも要る。`self_loc` は「このセルが先頭のフォームの範囲」
（reader の `cons_locs` と checker の `code_locs` は同じ 1 つの事実だったので統合。3 表に
分かれていた理由は寿命の違いだけで、それは表が外にあったことの帰結だった）。`car_loc` は
「car にある要素の範囲」で、**atom が位置を持つ唯一の道**である——シンボルは intern され、
小さなスカラは即値なので、占有ごとの identity を持たない。保持しているセルが持つ。

1 スロットなら `mark: bool` の後のパディングに収まって無料だったが、それでは片方の事実しか
持てない。`Cell` は 48 → 56 バイト（既定 `1<<16` セルで +512KB、`bootstrap` の `1<<18` で
+2MB）。`crates/typelisp-mem/src/value.rs` の `a_cell_is_seven_words` がこの数を固定する。

`Loc` は `Rc<str>` を含み 32 バイトで `Copy` でないので、セルは intern 表への `u32`
（`LocId`、0 = なし、id は 1 起点なので blank cell の全ゼロがそのまま NONE）を持つ。同じ行
から読まれた多数のセルは 1 エントリを共有する。表は縮まないが、上限は「読まれた相異なる
span の数」= ソース規模のオーダー。圧縮は sweep が全セルを歩くのでいつでも足せるが、計測が
要求するまで足さない。

### 島の bitcode は無効化されない

コンパイル済みコードは `rt_cons`/`rt_car`/`rt_cdr` の C 呼び出し経由でしかセルに触らない
（`crates/typelisp-rt/src/lib.rs`）ので、`Cell` のレイアウトは島から不可視。島トリオが
`compiler_island.bc` 再生成なしで緑であることで実証している。

### fasl

`OwnedForm::Cons` が 2 つの span を持ち、`owned_to_value` が cons したセルに書き戻す。
アドレスをキーにした旧表は、そのアドレスが属するヒープがもう無いので運ぶこと自体が不可能
だった——位置がセルと一緒に旅するようになったから運べる。`FASL_FORMAT_VERSION` 18 → 19。

### 順序（番人を先に立てた）

改修の前に `tests/fasl_test.rs::a_fasl_loaded_function_reports_its_own_source_position` を
足し、**今日の実装で通ること**を確認してから入れた。失いやすい性質（位置は値の一部ではない
ので誰かが意図的に運ばないとシリアライズを越えられない）なので、仕組みでなく性質に対して
テストを書いてある。`eval_in` は使えない——`into_kind` が位置を剥がすので検査対象が消える。

テスト: mem_test の位置系 3 件を書き換え（「sweep が表からエントリを消す」の検査 →
「**再利用されたセルが古い位置を持たない**」の検査）、span の intern を固定する 1 件、
`Cell` のサイズを固定する 1 件、`OwnedForm` の位置往復 1 件を追加。core_builder_test の
「全消しを生き延びる」は「収集を生き延びる」に。

## compiled → interpreted の穴を塞ぐ（`rt_apply_any`、2026-08-12）

cons セル化改修 Phase 2 の Stage D（後始末）。プランが Stage D に挙げていた項目のうち
`locate.rs`/`lsp.rs` のサイドチャネル化・`fasl` の `OwnedForm` 化・AST を直接叩く
テスト群の書き換えは Stage C の中で済んでおり、残っていたのは `rt_apply_any` だけだった。

### 何が空いていたか

Stage A4 で `BoxedObj::Closure`（インタプリタ実行されるクロージャ）が復活した。JIT は
「表現の要件」から「最適化」に降格したので、同じ `Type::Fn` の値が
`BoxedObj::CompiledClosure` のことも `BoxedObj::Closure` のこともある。ところが
コンパイル済み側の apply 地点（島の `compile-apply-indirect` → Rust ビルトイン
`build-closure-apply`）が発行していたのは `rt_closure_fnptr` + `rt_closure_env_get` の
コピーループ + 間接呼び出しで、これは前者しか扱えない。`rt_closure_fnptr` は
コンパイル済みクロージャでなければ `fatal` する——**捕捉できないプロセス abort** である。

```
(defun adder ((n i64)) (fn (i64) i64) (lambda ((x i64)) i64 (+ x n)))
(defun apply-fn ((f (fn (i64) i64)) (n i64)) i64 (f n))
(compile apply-fn)          ; adder は compile しない
(apply-fn (adder 5) 10)     ; → SIGABRT
```

先にこの落ちるテストを書いて SIGABRT を確認してから実装した。

### 分岐は値のある場所に置く

`build-closure-apply` の発行先を新シム `rt_apply_any(closure, args-ptr, argc)` 1 本に
変えた。箱の種類で分岐し、コンパイル済みなら旧来の系列（エントリポイント取得 → env の
再符号化 → `compiled_fn_type_with_env` 経由の呼び出し）を Rust で行い、インタプリタ
クロージャならフックを通してツリーウォーカへ再入する。**`src/compiler.rs` は不変**——島は
`build-closure-apply` を頼むだけで、何が出るかは Rust 側の裁量にある。これが島の bitcode を
凍結したまま入れられた理由。

### crate の向きとフック

`rt_apply_any` は `typelisp-rt` に置いた。AOT 実行ファイルは `typelisp-rt` の staticlib
だけをリンクするので、`rt_llvm_call` のように本体 crate に置くとクロージャを使う AOT
プログラムがリンクエラーになる。しかし `typelisp-rt` は `typelisp-mem` にしか依存しない
（インタプリタは逆向きの依存の先にある）ので、再入は関数ポインタでしか書けない:
`set_apply_interpreted` を `Interp::enter_compiled` が毎回登録する（`set_active_heap` と
同じ場所・同じ thread-local の理由）。AOT にはインタプリタが無いのでフックは未設定のままで、
その状態でインタプリタクロージャが来たら不変条件違反として `fatal` する。

### クロージャが自分の署名を持つ

境界を渡る語は宣言表現でしか復号できない（[[typelisp-crossing-must-be-type-driven]] と
同じ話——生の `f64` ビット列とタグ付き箱ポインタはどちらもただの `i64`)。apply 地点は
呼び先の `Fn` 型を静的には知っているが、実行時には何も運んでいない。そこで
`BoxedObj::Closure` に `ret` を足し、`params`（`((SYM REPR)...)`）と合わせて**箱が自分の
署名を持つ**ようにした。インタプリタ自身はどちらも読まない（`Value` を束縛して `Value` を
返すだけ）ので、唯一の読み手は `rt_apply_any` である。読み手が 1 つしか無いスロットは
mark フェーズが黙って辿らなくなる類なので、`tests/mem_test.rs` に `live_count` で気づく
テストを置き、`stack.push(*ret)` を外すと落ちることを確認した。

**組み込み関数だけは今も渡せない。** 箱が名前しか持たず署名が無いので、引数語を復号する
すべが無い。`encode_crossing_value` で捕捉可能なエラーとして弾いている（推測はしない）。

### 副作用: 島 bitcode の shim 表チェック

`rt_extern_functions` は 118 → 119 になった。`install_island_bitcode` は `check_hash` が
真のとき「新しい `.bc` は現行の集合をちょうど宣言している」と仮定して全件を
`add_global_mapping` に渡していたが、この仮定は誤りである——ハッシュが見ているのは
`compiler.rs` の `SOURCE` で、shim 表は Rust だからである。モジュールが宣言していない
shim を飛ばすフィルタを両方の読み込みに適用した（宣言が無ければ呼び出し地点も無い）。

### 印字の取りこぼし

`format_sexpr`（`main.rs`）と `format.rs` のクロージャの腕が `is_compiled_closure` しか
見ておらず、インタプリタクロージャが `#<unprintable>` になっていた。JIT が取ったかどうかは
値の性質ではないので、どちらも `#<closure>` と印字する。

### Miri の numeric_test は「走らないコマンド」だった（2026-08-13）

Stage D の検証で `MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --test
numeric_test` を回したところ **21 時間で終わらなかった**。テスト本体ではなく
`load_prelude` の中で、Miri の進捗バックトレース
（`-Zmiri-report-progress=50000000`）が指すのは
`Heap::intern_loc` ← `Heap::set_elem_loc` ← `reader::read_list` ← `Reader::read_all`。

履歴を調べると、このコマンド行は `numeric_test` 新設時（`8c8b812`、2026-06-18）に
「miri green 20/20」と一緒に書かれたもので、**当時の `numeric_test` は prelude を
読んでいなかった**（`src/prelude.rs` はその日に生まれたところで、テストは `Heap` を
直接組み立てていた）。数値ヘルパを prelude の typelisp メソッドへ移した `faee845`
（2026-07-22）で `load_prelude` を呼ぶようになったが、それ以降 Miri で走らせた記録は
無い。`dd17823`（2026-07-29）で TODO.md から development.md へ移ったのは文書の再編で、
実行ではない。**2 ヶ月間、テストの性質が変わったあとも同じコマンド行が運ばれていた。**

**cons セル化の退行ではない。** Phase 1c（`895aec5`）で同じ 1 テストを回しても 50 分で
同じく `load_prelude` の中にいる。prelude が 2,383 行に育って以降ずっとこうだった。

アルゴリズムの問題でもない（`intern_loc` は `HashMap` 引きで O(1)）。生ポインタの cons
アリーナへの書き込み 1 回ごとに Miri が Stacked Borrows の provenance 検査をするコストが、
prelude ぶんの回数だけ掛かる。ネイティブでは同じ 1 テストが 0.20 秒、`numeric_test`
バイナリ全体 33 件でも 0.05 秒。

`docs/dev/development.md` から該当行を落とし、「Miri で回せるのは prelude を読まない
テストだけ」を理由つきで書いた。**走らないコマンドを手順書に置いておくのは、番人の
ふりをした嘘である。** 同じファイルの「compile 機能の実装方針（未対応ノードの扱い）」も
`ast_bridge.rs` 前提のまま無効になっていたので、あわせて訂正した。

## 2 つの kind 分類を 1 つに統一する（2026-08-13）

cons セル化 Phase 2 で `src/check/repr.rs` に手書きの分類表が 2 つ新設された
（`Repr::binding_kind` / `Repr::field_kind`）。旧 `ast_bridge` の 2 つの分類器を
置き換えたもので、その際**旧分類器の不整合を 2 件そのまま引き継ぎ**、「島の bitcode が
凍結中なので今は直さない、島を作り直すときに両半分そろえて決着させる」と明記して
据え置いていた。島はその後一度も再生成されていない。

個別に直すのではなく、**表を 1 つにして食い違いが起こりえない形にした。** 2 つの
手書き表が同じ型について別々の答えを出せること自体が温床で、2 件はその温床が生んだ実例
である。

### 全 variant を突き合わせると、片方は他方の関数だった

| Repr | field | binding | |
|---|---|---|---|
| Sexpr / Str / Bignum / Ratio / Fn / Scope / Enum | 6 | 2 | 一致 |
| Struct / Vector / Dyn | 6 | **0** | 食い違い |
| HashTable | **0** | **0** | 両方誤り |
| Sym | 6 | 0 | **例外（正しい）** |
| Int / Handle / Float / Char / Bool / Unit / None | 1/2/3/4/11/0 | 0 | 一致 |

例外は `Sym` ただ 1 つ。GC の mark ループが辿るのは `Cons`/`Str`/`Boxed` だけで
`Value::Symbol` は素通しなので、**intern 済みシンボルは回収されない**——タグ付きだが
不死、という第 3 の状態である。それ以外はすべて「タグ付き ⇔ ルートが要る」で決まる。

そこで判断を 1 つの `Class` に集約し、2 つの番号をその射影にした。
`Tagged { collectable: bool }` の `collectable` が `Sym` の例外を明示的に持つ。
**新しい `Repr` variant を足す人は判断を 1 回しかしない。**

### 「島を作り直すまで直せない」は過大だった

doc とメモリは「kind 番号はインターフェースで、push と pop が同じ番号で対になって
いるから片側だけ動かせない」と言っていた。島のコードを読むと、これは**番号の意味を
変える（renumbering）**には当てはまるが、**どの型にどの番号を割り当てるか
（reclassification）**には当てはまらない。島は番号だけを見ており、型→番号の射影は
Rust 側（`core_bridge` / `Repr`）に閉じている。

実測で確認した: `Struct` を 2 に再分類した状態で、`compiler_island.bc` **無変更**のまま
島トリオ・`compile_test`・`compiled_binding_gc_test` すべて緑。

### 凍結 1 の危険性が「未確定」から確定へ

`tests/compiled_binding_gc_test.rs` は `Struct`/`Vector` の穴を突こうとして書かれ、
突けていなかった。**理由が判明した**: `compile-construct-boxed-struct` ほか 12 箇所が、
compiled コードの作る箱を**構築時に `push-permanent-sexpr-root` している**。permanent
root は pop されないので、compiled な struct ローカルは束縛 kind が何であろうと不死で、
どう割り当てを並べても回収されない。

**これは修正ではなくマスクである。** permanent root は意図的なリーク
（`compile-construct` に `build-free` の対がない）なので、いつか箱に本当の寿命を与える
人が、unrooted な束縛と収集の間に立っていた唯一のものを外すことになる。分類が正しく
なった今、その日は何事も起きない。

なお既存テストが落ちなかった直接の理由は別にもう 1 つあって、`p` を作ったあと
`sexpr-cons` しかしないと、回収されても `Heap::alloc_boxed` の `box_free.pop()` が
呼ばれず**箱のスロットが再利用されない**。間に箱の割り当てを挟むテストを足したが、
permanent root があるのでこれも落ちない。

### 凍結 2 は本物の穴だった

`HashTable<K,V>` の `field_kind` 0 は「構造体フィールドに置けない」を意味する番号で、
`Repr::None`（単型化前のジェネリック型変数）と共有していた。しかし
`Heap::alloc_hashtable` は `BoxedObj::Struct { payload: StructPayload::Map }` を作る——
**実行時表現はそのものずばり boxed struct** である。「struct の集合にも enum の集合にも
属さない」という 0 の理由は、旧分類器に渡していた集合の話で実体の話ではなかった。

帰結: `HashTable<K,V>` 型フィールドを持つ `defstruct` を compile すると

```
panic: compile-sexpr-field: field type is not representable in compiled code yet
```

で **SIGABRT していた**。表現できるのに「表現できない」と嘘をつく。テストは 1 件も
無かった。セル化された捕捉（`10 + field_kind`）の経路も同じ穴を踏む。番人を 2 本足し、
どちらも旧分類で SIGABRT することを確認してから直した。

### テスト

`src/check/repr.rs` の単体テスト 2 本は**旧来の誤りを固定していた**ので書き換えた
（「2 つの射影は文書化された場所でだけ食い違う」→「回収されうる表現はすべて素通し
かつルート付き」、「HashTable は variant を持つ前の番号を保つ」→「表現の穴は
ジェネリック型変数だけ」）。`core_bridge` のゴールデン 5 箇所も `Struct`/`Dyn`
レシーバが 0 から 2 になった。`the_boxed_values_kind_follows_its_representation` は
対比の軸が struct-vs-enum（＝バグそのもの）だったので scalar-vs-heap に変えた。

`src/compiler.rs` と `src/compiler_island.bc` は無変更。

### 島側のコメントは古いまま残る（次の島再生成時の宿題）

`src/compiler.rs` のうち **SOURCE 文字列の中（277〜4171 行）にある `;;` コメントは
触れない**。`island_artifacts_are_fresh`（`tests/island_artifacts_test.rs`）が
`source_hash(SOURCE)` と bitcode に埋め込まれたハッシュを比較しており、**コメント 1 文字の
変更でも島の再生成が要る**からである。SOURCE の外にある Rust の `//!` は自由に直せる。

そのため次の 2 つが宿題として残る:

1. **`ast_bridge` への参照 52 行**（Stage C で `src/compile/ast_bridge.rs` は削除済み。
   `grep -c ast_bridge src/compiler.rs` が 0 になったら片付いたということ）。
   SOURCE 外の 6 箇所は今回 `core_bridge`/`core_freevars` に直した。
2. **`compile-construct-boxed-struct` の doc（`:3557-3570`）が新しい分類と矛盾する。**
   「`binding_kind` は `mutable` struct を意図的に `KIND_SEXPR` にしない」と書いてあるが、
   今は `KIND_SEXPR` になる。この主張自体は当時正しく、根拠も正しかった
   （permanent root が誕生時から生かしている）。変わったのは、その依存を
   受け入れるかどうかの判断のほう。

**`compiler_island.bc` を再生成する人は、まずこの 2 つを消すこと。**

（**追記: 2026-08-13 中に両方とも解消した。** 下の「島のハッシュを読んだ形に対して取る」を参照。
宿題として残す前提だった「コメントは凍結されている」という制約そのものを外したので、
同種の宿題は今後発生しない。）

## 島のハッシュを読んだ形に対して取る（2026-08-13）

**問題**: `island_artifacts_are_fresh` が `source_hash(SOURCE)`——つまり**ソースのバイト列**——で
新鮮さを判定していた。コメントも空白も含まれるので、**コメント 1 文字の修正が 1MB のバイナリ
成果物を無効化する**。実際の帰結として SOURCE 内のコメントは事実上凍結され、削除済みモジュール
`ast_bridge` への参照が 52 行残り、分類と矛盾する doc が 1 つ残った。

**修正**: `bootstrap::island_source_hash` を新設し、**リーダーが読んだ形**をハッシュする。
`.bc` は SOURCE の**コンパイル結果**なので、陳腐化するのは形が変わったときだけ。
コメント・インデント・空行は形ではない。

### なぜコメント削除スクリプトではなくリーダーか

削除スクリプトは `;` の意味を自前で判定することになるが、`;` は常にコメント開始ではない
（`";"`、`#\;`）。**間違えたときの倒れ方が最悪**である——ハッシュが本物のコードを覆わなくなり、
番人が古い島を「新鮮」と呼ぶ。fail-open は新鮮さの番人が絶対に取ってはいけない方向。
リーダーはその区別をすでに正しく持っている。単体テスト
`a_semicolon_inside_a_string_is_not_a_comment` がこの罠を固定している。

### 位置情報は意図的に除外する

位置は cons セル自身に入っている（L1/L2 の変更）ので、ハッシュに含めると行番号が戻ってきて
意味がなくなる。除外して安全なのは、**リーダーより下流が位置をコンパイル済みコードに
持ち込まないから**（`core_bridge` は位置を出さず、`src/compiler.rs` も埋めない）。
`fasl::source_hash` はバイト基準のまま——fasl は位置込みの検査済み状態を保存する（v19 の
`OwnedForm` は両スロットを運ぶ）ので、あちらでは行の移動が本当に変更である。

### 副産物: 番人は成果物の 2 つの入力のうち 1 つしか見ていない

再生成して初めて分かったこと。**SOURCE 無変更のまま bitcode が 1,049,129 → 1,116,737 バイトに
変わった。** 古い島は `rt_closure_fnptr` + env ループ + 間接呼び出しを出しており、新しい島は
`rt_apply_any` 1 回を出す——Stage D で書き換えた **Rust 側ビルダ**の差である。

つまり `.bc` は「SOURCE」と「Rust 側の LLVM ビルダ」の 2 つの入力を持ち、ハッシュは前者しか
見ていない。実害は無かった（実行時にコンパイルされるユーザプログラムの IR は常に現在の Rust
ビルダから出る。遅れていたのは島自身の本体だけで、その呼び先はすべて compiled）。

## 番人を入力でなく出力に当てる（2026-08-13）

上の穴を塞いだ。`the_committed_island_matches_a_fresh_build`
（`tests/island_artifacts_test.rs`）が **bitcode を組み直してバイト列を比べる**。

### なぜコンパイラのバイナリをハッシュしないのか

「Rust 側のバイナリを 64 ビットずつ足す」は仕組みとしては動くが、**粒度が合わない**。
無関係な Rust の変更 1 行でも値が変わるので、番人が Rust をいじるたびに赤くなり、
そのたび 1MB のバイナリ差分が積まれる。毎コミット吠える番人は無視されるようになる。

必要な粒度は「出てくる IR が変わったか」であり、それにぴったり合う一番安いものは
**出力そのもの**である。IR を変えるものはこのバイトを変え、変えないものは変えない。
コンパイラのどの部分が効くかを当てる必要がない。

### 成立条件は再生成が不動点であること

ビルドはコミット済み `.bc` を入れて**その**ネイティブ `compile-function` に駆動させる
（スナップショット連鎖）ので、成果物は自分自身の構築の入力でもある。それでも収束するのは、
何が出るかを決めるのが SOURCE のロジックと Rust 側ビルダであって、駆動している世代では
ないから。**実測で確認**: 新世代から再生成しても md5 は同一。

### 番人が本当に落ちることの確認

- 旧 `.bc`（前コミット）に差し替え → 2 本とも落ちる
- **ビルダだけを変える**（`build_alloca` の命令名を 1 つ変更、SOURCE 無変更）
  → `island_artifacts_are_fresh` は **ok**、`the_committed_island_matches_a_fresh_build` が
  **落ちる**。これが「入力を 2 つとも覆う」ことの実証

### 既知の注意

bitcode の符号化は LLVM のバージョン依存なので、LLVM 17 のパッチリリースが違うと
バイト列も変わりうる。これは誤検出ではない——コミット済み成果物が手元と別のコンパイラで
作られていた、という事実そのもの。

## cons セル化のあとの片付け（2026-08-13）

Phase 2 完了後、不要になったもの・重複・単純に書けるようになったものを洗い出して片付けた。

### 到達不能なコードは警告に出ない

`pub` な項目には dead-code 警告が出ない。**警告 0 は「未実装の TODO が無い」を意味するが
「到達不能なコードが無い」は意味しない**（[[feedback-dead-code-warnings-are-the-todo-list]] の裏面）。
定義以外に参照が 1 つも無かったもの:

- `repr::is_scope_ty` / `repr::is_enum_ty` — `Repr::Scope` と `is_enum_ty_by` が引き継いだ後の残り
- `Registry::trait_def_mut` — 可変版だけ未使用
- `Error::read_error` — `Error::ReadError(s)` と同義の 1 行
- `core::unlowered` — Phase 2 の足場。呼ぶ側が 0（テスト 3 本と doc だけ）

`unlowered` を消すとき、それを使っていたテスト
`a_tag_with_no_evaluation_names_itself` は**性質を残して被写体だけ差し替えた**。
「未知のタグは自分の名前を言う」は足場と無関係に成り立つ性質で、宣言されていないタグ
`(no-such-tag 1)` で同じだけ試せる。

### 恒等関数が名前と 60 行の doc を連れて残っていた

`interp::rtvalue_to_struct_field` は全アームが引数をそのまま返す**恒等関数**で、
`rtvalue_to_sexpr` はそれを呼ぶだけだった。値世界の統合（[[typelisp-sexpr-rtvalue-unification]]）で
中身が消えたのに、名前・10 箇所の呼び出し・境界越えを説明する 60 行の doc が残っていた。
削除して呼び出し側に値を直接渡した。[[typelisp-decode-field-typed-was-identity]] と同じ形。

**調査中に一度、この関数を「存在しない」と誤って報告した。** `grep ... | head -5` で
定義行が出力から落ちていたのに、5 件の doc 参照だけを見て結論を出したため。
消費者を最後まで辿る前に判断しない、という同じ教訓。

### ガードの守備範囲が危険の範囲より狭かった

`core_builder_guard_test` は `src/check/` の生 `.cons(` だけを禁じていた。しかし
`Heap::cons` はどのモジュールから呼ばれたかを気にしない。同じ core IR を組む
`compile::core_bridge` と `eval::interp::core_eval` は対象外で、生 cons が 19 箇所あった。

- 走査対象を `src/check/` + `src/compile/` + `src/eval/` + `src/fasl.rs` に広げた
- `core::pair` を新設（`(name . kind)` `(kind . form)` `(pattern . body)` — IR はリストでない
  ペアだらけで、それぞれが手で `push_root` を書いていた）
- `#[cfg(test)]` 以降は走査しない。テストのフィクスチャは自分のアサーションの隣で読まれる
- 本番経路の例外 5 箇所に `core-build-ok:` と理由を書いた（`cons` 述語そのもの、
  マクロの `&rest` 用ユーザデータ、実行時環境フレーム、fasl の復元）

**ついでに実在した危険**: `arg_pairs_with` は `RootScope` の中でルートしたペアを `Vec` で
返していた。`return` でスコープが drop してルートが外れるので、呼び出し側の `f.extend` までの
間ペアは無防備だった。その窓の間に確保が無いので壊れていなかったが、そう書かれてもいなかった。
呼び出し側の `Items` へ直接 push する形にして窓ごと消した。

### 実装より生き残ったコメント

- `RtValue`（統合で削除された型）への言及 110 箇所のうち、**現在形で語っていた約 45 箇所**を
  現在の名前に直した。残りは過去形の記述か「Sexpr/RtValue 統合 Stage N」という**計画の名前**で、
  歴史として正しいので残した
- `ast_bridge`（Stage C で削除）への言及 8 箇所。うち 2 つは「削除したときに抜き出した」
  「あの漏れを見つけた場所」という歴史記述なので残し、6 つを現在の名前に直した
- `core.rs` の「`compile::core_bridge` が 1 関数で 16 個の pop を手で釣り合わせている」は
  旧 `ast_bridge` の話。現在の `core_bridge` の `pop_root` は 0
- `bootstrap_island.rs` と regen スクリプトが、実際には書かれない
  `compiler_island.fasl` を成果物として挙げていた

**`src/compiler.rs` の SOURCE 内のコメントも 1 つ直した**（存在しない関数名への参照）。
同日の「島のハッシュを読んだ形に対して取る」変更の最初の見返りで、島の再生成は不要だった
（ハッシュもバイト比較も無変更）。今朝なら 52 行の宿題に積まれていた種類の修正である。

## checker.rs の調査と、引けた境界 1 本（2026-08-14）

12,015 行・323 関数の `src/check/checker.rs` を、仮説を立てて測ってから触った。
**4 つの仮説のうち 3 つが反証され、実際に手を入れたのは 1 つ**。

### 反証された仮説

- **H1「`_opt_key` 族は本体の重複」** — `check_call` と `check_call_opt_key` の
  行一致は 33%、`check_defun` と `check_defun_opt_key` は 10%（`check_defun` は
  14 行のディスパッチャで実体は `_opt_key` 側）。コピーではなく別のロジックだった。
- **H4「`parse_*` 16 個は `types.rs` に属する」** — 13 個が `self.reg`/`self.ns` を使う。
  型注釈のパースは registry と名前空間に依存していて、型の側には出せない。
- **H3「型検査と lowering が絡んでいる」** — 絡んでいるのは 241 メソッド中 **39 個**
  （`check_match`/`check_call`/`check_inner`/`check_defstruct` …）。分離すれば同じ構文を
  2 度歩いて型情報を受け渡すことになる。**測って「触らない」と結論した。**

### 引けた境界: `impl Checker` の state-free な構築子

`impl Checker` の 271 メソッドのうち **24 個（393 行）が `&self` を取りながら本体で
一度も使っていなかった**。その多くが core IR のノード構築子（`if_form` `set_form`
`panic_form` `quote_form` `module_form` …）で、`src/check/forms.rs` へ移した。

問題は行数ではなく、**「checker の状態を使うもの」と「使わないもの」の境界が
どこにも引かれていなかった**こと。`&self` を取っているだけで、読む人には状態依存に見える。

**抽出は連鎖した。** `self` を「移した関数を呼ぶため」だけに使っていたメソッドが、
移動のたびに新たに state-free になり 3 巡した（24 → 7 → 3 → 2）。

| | 前 | 後 |
|---|---|---|
| `impl Checker` | 10,861 行 / 271 メソッド | 10,419 行 / 241 メソッド |
| `&self` を取りながら使わない | 24 | 2（意図的） |
| `checker.rs` | 12,015 行 | 11,646 行 |
| `src/check/forms.rs` | — | 426 行 / 27 関数 |

残した 2 つ（`check_quote`/`check_load`）は `check_special` のディスパッチ表の一員で、
20 以上の兄弟が全員 `&self` を取る。**一様な家族の形のほうが最小のシグネチャより読みやすい**
ので受け手を残し、両方にその理由を書いた。移した構築子との違いも明記した——
あちらは形を保つ理由が何も無いのに `&self` を取っていた。

### これ以上は割らない

境界を引いたあと、残りを測り直した:

| 群 | メソッド / 行 | 判定 |
|---|---|---|
| 型 + lowering の両方に触る | 39 / 2,890 | 不可分 |
| 兄弟メソッドを呼ぶ調整役 | 52 / 1,470 | 動かせない |
| `reg` だけ・自己完結 | 6 / ~164 | 動かせるが小さい |

`reg` だけを触る 29 メソッドのうち自己完結は 9 個（うち 3 個は残すべきアクセサ）で、
残り 20 個は兄弟を呼ぶ。**`checker.rs` が大きいのは checker が大きいからで、
引ける境界は 1 本しか無かった。** 同じ調査を繰り返さないために測定結果ごと記録する。

---

## fasl（コンパイル済みモジュール機構）を削除（2026-08-14、ブランチ `refactor/remove-fasl`）

TODO.md にあった「fasl の速度に本物の番人を置く」を検討するうちに、番人の手前の問いが出た:
**この fasl はロード時間を縮めるだけで、コンパイル済みコードではない。それに意味はあるのか。**

結論として機構ごと削除した。以下、判断の材料と、失ったものの実測値。

### fasl が何だったか

`Fasl` が持っていたのはレジストリ差分・ジェネリックのテンプレート・型チェック済みトップレベル
フォームの3つで、`load_into` はそれを積み直して `interp.exec` を回していた。つまり省いていたのは
**前半（read + 型チェック）だけ**で、後半の実行はインタプリタでまったく同じに走る——
**実行は1ミリ秒も速くならない**。CL の `.fasl` が指すのはコンパイル済みコードを収めたファイルで
あり、この機構はそれではなかった。ネイティブコードを持つ機構は別にある（`src/compiler_island.bc`
＋ `load_compiler_aot`、`compile` 機能）。

### 実測（削除前、release ビルド、N=5 の min）

| 測定 | source 経路 | fasl 経路 | 比 |
|---|---|---|---|
| A: prelude のみ（LSP per-pass の環境） | 55.8 ms | 7.3 ms | 7.6x |
| B: 診断パス全体（prelude + 小さな文書） | 57.6 ms | 7.6 ms | 7.6x |
| C: ディスクから起動（JSON パース込み） | 55.8 ms | 24.6 ms | 2.3x |

**機構は実際に効いていた**。削除の代償は LSP のキー入力ごとに +48 ms、CLI 起動に +31 ms。
それを承知の上で、「実行を速くしない機構をこの規模で抱える価値は無い」という判断で削除した。

削除**後**に実測した、複数ファイルのプロジェクトに対する診断パス1回
（`examples/projects/shape-canvas`、prelude + `use` 先3モジュール）は **74.3 ms**
（release、N=5 の min）。削除前の対応値は測っていない——`ModuleCache` の命中を含む数字は
機構を消した後では再現できないため。分かっているのは上の表の内訳（prelude ぶんが 7.3 ms →
55.8 ms）までで、依存モジュールぶんの増分は別途は測れていない。

C が A/B に比べて振るわないのは形式のせい: ディスク上の実物は 2,383 行のソースに対して
**2,499,193 バイトの JSON**（`serde_json`）で、その JSON パースが 24.6 ms のうち約 17 ms を
食っていた。ディスク経路は形式をバイナリ化すれば救えたが、それは「効いていない部分を直す」で
あって「実行が速くならない」という本体の問題には触らない。

TODO.md には「測る対象が違う（prelude ではなく LSP 診断パスを測れ）」と書いてあったが、これは
**誤りだった**。fasl が省くのは read+check だけで `exec` は両辺共通なのだから、正しい測定単位は
「同じモジュールを source から積む vs fasl から積む」であり、診断パスを測ると fasl と無関係な
文書側のチェックコストが両辺に乗って比を薄めるだけである（上の表の A と B が実際そうなっている）。
2026-08-07 に削除した旧 bench `bench_fasl_load_beats_source_load` は**測る対象自体は正しかった**
——欠陥は `#[ignore]` と `fasl_time < source`（1.01 倍でも通る）という assertion のほうにあった。

### 削除の範囲と、残ったもの

消したのは `src/fasl.rs` / `tests/fasl_test.rs` の全体、`typl compile-module` サブコマンド、
`.fastl` 拡張子、`prelude::load_cached`/`prelude_fasl` と `~/.typl/cache`、`(load)` の fasl 優先
分岐、LSP の `Rc<Fasl>`、`project::ModuleCache`（依存モジュールのパス間キャッシュ）、
`Checker::export_templates`/`install_templates`/`registry_mut`。付随して `registry.rs`/
`resolved.rs`/`types.rs` の serde 派生と `Path` の手書き `Serialize`/`Deserialize`、
`typelisp-mem` の `serde` feature、`num-bigint`/`num-rational` の `serde` feature も落ちた
（`serde`/`serde_json` 本体は `lsp-types` が要求するので残る）。

**`(load "path")` は言語機能として残る**——`load_file_flat` はもともとソースも読むので、
fasl 優先の分岐が消えただけ。

**予期していなかった発見**: `OwnedForm`（ヒープ非依存の読み形ミラー）は fasl のシリアライズ
担当として書かれたものだが、checker が独立にそれを必要としていた——`&optional`/`&key` の
デフォルト式（`OptKeyParam::default`）、`impl` より先に検査する `deftrait` のデフォルト本体、
per-instantiation の再検査のために retain するジェネリックテンプレート。どれも「チェック済みの
`Value` はあるヒープへの添字なので、そのヒープより長生きする状態は `Value` を持てない」という
同じ理由による。そこで `OwnedForm`/`OwnedCell`/`value_to_owned`/`owned_to_value` だけを
`src/owned_form.rs` へ移し（serde 派生は落として）、fasl 本体は消した。
**シリアライズ形式として書かれたものが、シリアライズをやめても生き残る**という形。

### 検証

`cargo check --all-targets` は警告 0（削除の途中で出た「消し残しの `pub` が dead code を隠す」
に当たらないよう、`load_source_flat` は `pub` を落として private にし、`registry_mut` は
呼び出し元が消えた時点で削除した）。`ModuleCache` を見ていた `module_file_test.rs` の4本は、
キャッシュ以外のことも見ていた3本をキャッシュ無しの形（別セッションで再ロードして編集が
見えること）に書き換え、純粋にキャッシュ命中数だけを見ていた1本を削除した。

## `(compile)` の未解決名をチェック時エラーに（2026-08-14、ブランチ `feature/compile-strict-names-and-prelude-bitcode`）

`Checker::check_compile` は解決できない名前をわざとエラーにせず、プレースホルダの `Path` を入れて
実行時の `EvalError::NoSuchFunction` に委ねていた。doc コメントに理由が「既存挙動の保存」と
明記されていたが、その保存はもう要らないという判断。

チェッカーは常にどこへ解決するか知っている（`documentation` 特殊形が同じ前提で完全にチェック時
解決している）ので、名前解決の失敗だけ実行時まで持ち越す理由がない。3 系統を区別して報告する:

- 裸の名前が解決しない → `no function \`foo\` is visible from here`
- `T::m` で型は解決したがそのメンバが無い → `type \`T\` has no associated function or method \`m\``
- `a::b` が型のメソッドでも自由関数でもない → `names neither a type's method nor a function`

2 番目は以前 `type_fq` の `.filter(...)` で潰れて 3 番目の枝に落ちていたので、型の解決結果を
別に保持して分岐させた。可視性は変わらない——`resolve_fn`/`resolve_fn_path` はもともと
「在るが見えない」を「解決しない」に畳むので、`(compile 他モジュールの非 pub)` も同じ経路で落ちる。

`Interp::compile_function` 側の再解決失敗は、チェッカーが解決を保証した後では内部不変条件違反な
ので `EvalError::NoSuchFunction` から `Internal` へ変えた（`compute_sccs` の finish-order を
`.expect()` で守っているのと同じ扱い）。

## prelude をビットコードへ事前コンパイル（2026-08-14、同ブランチ）

prelude は毎起動 read→check→exec され、本体はすべてツリーウォークで実行されていた。コンパイラ島
（`src/compiler_island.bc`）が既に「コミット済みビットコードを起動時に JIT インストールする」形で
同じ問題を解いているので、その機構を prelude にも広げた。**削除した fasl とは目的が違う**——fasl は
チェック済み状態をシリアライズしてロード時間だけを縮める機構で、`exec` は両辺共通だから実行は
1 バイトも速くならなかった。こちらがシリアライズするのは機械語である。

新しいもの: `src/compile/prelude_bootstrap.rs`（収集器＋生成器）、`src/prelude_compiled.bc`
（244 KB）、`src/bin/bootstrap_prelude.rs`、`scripts/regen-prelude-bitcode.sh`、
`tests/prelude_artifacts_test.rs`、`tests/prelude_compiled_test.rs`。
`Interp::install_island_bitcode` は `install_compiled_library` に一般化した（島は root の `defun`
決め打ちだったが、prelude は 105 個の `defmethod` を持つ）。

### 事前コンパイルできる範囲は判断ではなく構造で決まる

prelude の `defun` 86 個のうち **61 個がジェネリック**で、単型化は使用箇所ごとに走るから事前に
固められる本体が存在しない。ただしジェネリック判定を書く必要はない——チェッカーはジェネリック
テンプレートを**空の `(module PATH)`** として吐くので、収集器を素通りして自然に消える
（`deftrait` も同じ形）。「収集を生き延びたものをコンパイルする」がそのまま規則になる。

### 決定的採番

compiled コードはグローバル変数 id を定数として IR に焼き込み、id は `Interp` 生存期間ごとに 0 から
順に振られる。したがって生成時とロード時で採番順が一致しないと壊れる。`compile-file` が
`$global_init$N` をファイル宣言順に並べているのと同じ問題で、同じ解——**同じ 1 回の走査が生成側と
ロード側の両方で使われる**（`PreludePlan`）。手で並べた 2 つのリストを同期させる形にはしていない。

vtable/trait id（`dyn-new`/`dyn-upcast` が焼き込む）は今の prelude では発生しない
（`as-dyn-error` はジェネリック、ストリーム合成は既にボックス済みの `:dyn` を受け取るだけ、
`dyn-call` はスロット番号しか焼かない）。**仮定せず検査する**——生成器は最後に
`vtable_descriptors()`/`upcast_descriptors()` が空であることを確認し、空でなければ停止する。
焼き込んだ id がロード時に別の意味になる故障は静かなので、気付ける形にしてある。

### コンパイルできないものは「根本原因」で記録する

生成器は 111 個の prelude 定義をコンパイルできない。だがその原因は 9 種類しかないので、
`PRELUDE_COMPILE_UNSUPPORTED` は**定義でなくコンパイル経路の穴（呼び先の名前）**を記録する。
定義を並べると帰結の一覧になり、prelude を 1 行いじるたびに churn し、しかも「何を作れば短く
なるか」を何も言わない。

| 穴 | 巻き添えになる定義数 |
|---|---|
| `char::char->string` | 31 |
| `stream-read-char` ほか stream/file 系 10 種 | 計 65 |
| `bignum::lognot`/`logand`/`logior`、`bool::equal`、`symbol::eq`、`string::substring` | 計 13 |
| `symbol->string`、`random-state-next`、`random-state-copy` | 計 3 |

生成のたびに再計算して、このリストと完全一致しなければ停止する（見つけた集合を貼り付け可能な
形で印字する）。穴を塞げば「listed, but blocks nothing now」で落ちるので、リストが嘘になれない。
**次に最も効くのは `char->string` の lowering**（1 つで 31 定義）。

判定は emit 前に済ませる必要がある: 島の `compile-call` は宣言の無い呼び先に対して
`get-function` で**プロセスを abort する**ので「やってみて失敗したら除外」ができない。
`Interp::precheck_compilable` が `compute_sccs`（コンパイル経路自身のグラフ・フィルタ・
「本当に呼び先か」の定義をそのまま使う）に聞く。推移的なので、コンパイル不能なものを呼ぶ定義も
自動的にコンパイル不能と報告され、集合が呼び出し元について閉じる。

ロード側はこの走査を繰り返さない。**成果物自身に聞く**——モジュールが本体を持つシンボルが、
まさに emit されたものだから。起動時予算に対して安いだけでなく、同じ述語を 2 回走らせる形と違って
「食い違いようがない」。

### 副産物: Rust と島の native-method リストが食い違っていた

`is_native_lowered_primitive_method`（Rust）は `char->string` を「ネイティブに lowering される」と
主張していたが、島の `char-native-method?` には最初から無かった。このリストは**呼び出しが
コールグラフの本当の辺かどうかを決める**ので、食い違いは最適化の取りこぼしではない——辺が落ち、
宣言が emit されず、`get-function` がプロセスを abort する。`(char->string c)` を含む関数は
`(compile ...)` できなかったはずで、prelude 全体をコンパイルするまで誰も踏まなかった。

番人として `native_method_list_tests` を追加した。島の `SOURCE` から `*-native-method?` の
`(equal method "X")` を読み取って双方向に比較する。そのために Rust 側を `matches!` から
テーブル（`native_lowered_primitive_methods`）へ変えた——危険な向き（Rust だけが主張する）は
述語には問えない、列挙するものが無いから。`char->string` を戻すと実際に落ちることを確認済み。

### 実測（release、同一プロセス内 A/B、`scripts/bench-prelude.sh`）

N=3 の範囲で示す（同じ機械でも 1 割は動くので、1 回の数字を 3 桁で書いても意味がない）。

| ワークロード | compiled | interpreted | 倍率 |
|---|---|---|---|
| i64 `gcd` | 225〜261 ms | 1219〜1321 ms | **5.1〜5.4x** |
| i64 `abs`/`signum`/`rem` | 907〜969 ms | 1952〜1999 ms | **2.1〜2.2x** |
| bignum `abs`/`+` | 291〜305 ms | 416〜428 ms | **1.4〜1.5x** |
| f64 `abs`/`signum` | 1221〜1252 ms | 1774〜1863 ms | **1.4〜1.5x** |

倍率の差は「その本体でネイティブ化が何を消せるか」に対応している。`gcd` は再帰＋整数演算だけ
なのでツリーウォークの分がまるごと消えて 5 倍。bignum/f64 は本体の外——ボックス確保と
`rt_*` 呼び出し——が支配的なので 1.4 倍で頭打ちになる。

A/B を同一プロセス内で取るのは、両辺がビルド・マシン・ヒープサイズを共有するようにするため
（`typl` を 2 回叩く形だとそこが揃わない）。閾値 assert は置いていない——タイミングの assertion は
混んだマシンで落ちて、無視することを学習させるだけなので。

**起動時のコスト: +105〜109 ms**（prelude ロードが 58 ms → 165 ms）。空ファイルに対する `typl`
全体の起動は release で 0.84 s（N=5）なので、その約 13% にあたる。244 KB のビットコード全体を
MCJIT が起動時に解決する分。ユーザー選択により常時有効（切替フラグは設けていない）。

その一部は削れた: ステイルネス検査のハッシュを `island_source_hash(prelude::SOURCE)` で取ると
**ちょうど今読んだ prelude をもう一度 256K セルの `Heap` へ読み直す**ことになる。読んだ形に対して
ハッシュを取る `hash_read_forms` を切り出し、`PreludePlan` がキーを運ぶようにした（約 4 ms、
`load` あたりのパースが 2 回から 1 回へ）。残りの約 100 ms は MCJIT の解決そのもの。

---

## コンパイル経路の穴を塞ぐ（2026-08-14、同ブランチ）

前項の生成器が数え上げた穴——`PRELUDE_COMPILE_UNSUPPORTED` の 20 エントリ、巻き添え 111 定義——を
全部塞いだ。リストは空になり、**収集を生き延びた prelude 定義（＝ジェネリックでないもの）は
すべて事前コンパイルされる**。`src/prelude_compiled.bc` は 244 KB → 416 KB。

塞ぎ方は穴の性質ごとに 4 通りだった。

### 1. プリミティブ受け手の組み込みメソッド（`char->string` ほか）

`char->string`（単独で 31 定義）、`string::substring`、`bignum::logand`/`logior`/`logxor`/
`lognot`、`bool` の `eq`/`eql`/`equal`/`equalp`、`symbol::eq`/`eql`。

`char->string` は**新しい runtime shim すら要らなかった**。コンパイル後の `char` は生のコード
ポイントで、`rt_str_new` の引数はまさに生のコードポイント列だから、`rt_str_new(a, 1)` がそのまま
`char->string` である。31 定義を止めていたものの実体は 4 行の `icond` 節だった。

`bool`/`symbol` は `compile-assoc` にとって**新しい受け手型**なので、`NATIVE_LOWERED_PRIMITIVES`
を 8 → 10 に増やし、島に `bool-native-method?`/`symbol-native-method?` を足し、前項で入れた番人
（`native_method_list_tests`）の `PRIMITIVES` 表にも両方を登録した。番人の守備範囲を新しい型に
広げないまま片方だけ書くのは、まさに `char->string` が生き延びた形である。

`bignum::logxor` は最初は「prelude に呼び出し元が無い」と判断して外した。regen が
`blocks compilation, but not listed: bignum::logxor` で落ちた——`logeqv` は `lognot` の
`logxor` なので、**`lognot` を塞ぐまで `logxor` は見えなかった**。穴は互いに隠し合う。生成のたびに
再計算する仕掛けだから気付けた話で、手で書いた一覧なら見落としていた。

### 2. 自由な組み込み関数と、名前対応表の一本化

`symbol->string`（`rt_sym_name` の再利用——`sexpr-sym-name` と同じ内部表現に、チェッカー側の別
綴りが付いているだけ）、`random-state-next`/`random-state-copy`/`make-random-state-fresh`
（`random-state` はヒープ上の箱でしかないので shim は素直に書ける）。

ここで**対応表を 1 つに畳んだ**。従来は「組み込みかどうか」を `core_bridge` が
`is_rt_builtin_name` で判定し、「ではどの shim か」を島の `compile-call` が 15 段のネスト `if` で
再導出していた。一致しなければならない 2 つのリストで、しかも不一致の帰結はこのコンパイラで最悪の
もの——島の `get-function` は見つからない名前に対して**プロセスを abort する**。
`rt_builtin_symbol`（Rust）が shim 名まで答えるようにし、`symbols::callee_symbol_name` が
「マングルするか shim 名にするか」の唯一の判断点になり、島側のネスト `if` は削除した。島は渡された
名前をそのまま呼ぶ。

### 3. ストリーム（65 定義）: テーブルを `typelisp-rt` へ移す

最大の塊。そして「lowering を書いていない」のではなく、**書けない構造だった**——ストリーム表は
`Interp` のフィールドで、AOT でリンクした実行ファイルにはインタプリタが存在しない。`Interp` の中に
ある限り、compiled 側から触る shim は原理的に置けない。

`src/eval/stream.rs` → `crates/typelisp-rt/src/stream.rs`（std だけに依存していたのでそのまま
動く）へ移し、thread_local 1 つの後ろに置いた（`Heap` と同じ理由——2 つはいつも一緒に使われ、
テストはスレッド並列で走る）。compiled `open` が返したハンドルを interpreted `close` が閉じられる
のは、両者が同じ表を引くからで、2 つの表を同期させているからではない。

値づくり（`Result<_, FileError>`、`Ok(none)` での EOF、エラーメッセージの文面）も **1 実装**に
した: `stream_builtin::stream_builtin` を `Interp::eval_stream_builtin` と 20 個の `rt_stream_*`
shim の両方が呼ぶ。20 個それぞれに失敗様態と包み方があるので、境界の両側に書けば 20 回食い違える。
shim が足すのは表現の変換だけ——ハンドルと `bool` と `char` は compiled 側では生のマシン語、
文字列と `Result` はタグ付きヒープ値——で、それは呼び出し規約の話であって操作の話ではないから
shim 側にある。

型キー（`option`/`result`/`fileerror`）は rt 側の定数になった。`rt_data_new` が「compiled code から
文字列で受け取る」形で回避してきた一方、ここは**自分で作る**側なので受け取れない。
`type_key_of` と一致することを `tests/type_identity_guard_test.rs` に番人として追加した
（`src/` を走査する既存の 2 規則は別クレートに届かない）。破れ方が静かな不変条件は、仮定ではなく
検査にする。

### 4. 副産物: `Repr::RandomState`

穴を全部塞いだあと、生成器は `random` のコンパイルで島ごと abort した——
`compile-sexpr-field: field type is not representable in compiled code yet`。
`random` は `Option<random-state>` を `match` するが、`Type::RandomState` に `Repr` が無く
`Repr::None`（フィールド kind `0`）に落ちていた。`random-state` は `bignum`/`ratio` と同じ
「タグ付きの、回収可能なヒープ値」なので、`Repr::RandomState` を足して `Class::Tagged` 群へ入れる
だけで済む（島は番号で分岐するので再生成は不要）。

**precheck はこれを見られない**: `Interp::precheck_compilable` はコールグラフを辿って
「lowering の無い呼び先」を探す仕掛けで、*形*の穴は守備範囲の外にある。前項で
`build_prelude_bitcode` のエラーメッセージに書いておいた「precheck が通ったのにここで失敗したら、
それはコールグラフ走査に見えない穴」がそのまま起きた。

### 5. `:dyn` ディスパッチの穴（compiled → interpreted）

穴を塞いだ結果 prelude のストリームメソッドが compiled になり、`stream_test` の
「ユーザ定義の入力ストリーム」が
`rt_vtable_slot: vtable slot is empty — a dyn dispatch target was not compiled` で
プロセスごと落ちた。

compiled な `:dyn` 呼び出しは「スロットを読んで間接呼び出し」——**トレイトオブジェクトの中身が
必ず compiled である**という前提だった。そんな保証は無い: compiled な prelude の `read-line` に
渡される `:dyn CharInput` の具体型が**ユーザの defstruct** で、その `read-char` は普通の
interpreted メソッドでありうる。スロットは 0 で、引数を配列に積み終わった呼び出し側には行き先が
無い。

`rt_apply_any`（クロージャに対する同じ穴、2026-08-12）と同じ形で塞いだ——**分岐を値のある場所に
置く**。`rt_vtable_slot` + `build-dyn-call` の 2 段を `rt_dyn_call` 1 つにして、スロットが埋まって
いれば間接呼び出し、空ならインタプリタに「そのスロットのメソッドを reify したクロージャ」を訊いて、
`rt_apply_any` が使うのと同じ interpreted-apply フックへ渡す。引数配列の解釈が両者で一致するのは、
interpreted クロージャが自分のパラメータ表現を持ち歩いているからで、これも Stage D と同じ理屈。

あわせて `vtable_id_for` が**インターン時に publish** するようにした。従来は `compile_scc` の
あとだけで、事前コンパイル済み prelude しか無いセッション（＝何もコンパイルしない）ではコンパイル
済みターゲットの表が compiled 側に一度も届かなかった。

### 6. 副産物: promote された defvar を Rust 側の読み手が見ていなかった

`pprint_test`/`print_limits_test` の 14 件が「改行が一切入らない」形で落ちていた。原因は今回の
作業ではなく前項（prelude 事前コンパイル）にある: `promote_globals` が prelude の `defvar` を
**全部** promote するので `(setf *print-pretty* true)` は permanent root に書かれるのに、
`Interp::pretty_opts`/`print_limits` はインタプリタ側のセルを読んでいた。`eval_core` の
`global_core` は最初から両者を区別していたが、Rust から名前で読む 2 箇所だけが取り残されていた。

`Interp::global_value` を 1 つ作って両方をそこへ通した。破れ方が静かなバグ——設定が効かないだけで
エラーは出ない——なので、読み手を増やすときはこの入口を使うこと。

### 効果

`PRELUDE_COMPILE_UNSUPPORTED` は空になったが、**リストと突き合わせ検査は残す**。役割が
「今ある穴の記録」から「新しい穴が空いた瞬間に落ちる番人」に変わっただけで、prelude に定義を 1 つ
足して lowering の無い組み込みに触れば、その呼び先の名前を出して生成が止まる。

### 実測

`scripts/bench-prelude.sh`（release、同一プロセス内 A/B）。実行速度側は前項から動いていない
（同じ本体が同じように compiled になっているだけ）——動いたのは**起動時コスト**で、成果物が
244 KB → 417 KB になった分ほぼ比例して増えた:

| | 成果物 | インストール（N=3） | 空ファイルに対する `typl` 全体（N=5） |
|---|---|---|---|
| 穴を塞ぐ前 | 244 KB | +105〜109 ms | 0.84 s |
| 現在 | 417 KB | +198〜258 ms | 0.98 s |

穴を塞ぐとは「事前コンパイルされる本体が増える」ことなので、この増加は成果そのものの裏面である。
トレードオフの検討は [TODO.md](TODO.md) 側に残した。

### 代償: compiled な prelude 本体の `panic` はプロセスを abort する

`rt_panic` は「JIT/AOT のネイティブフレームを巻き戻す landing pad が無いので abort する」という
既存の意図的な設計だが、prelude が既定で compiled になったことで、**prelude 本体の `panic` が
catchable な `EvalError::Panic` ではなくプロセス終了になる**。`(random -5)`（今回 compiled に
なった）と `(expt 2n -1n)`/`(expt 2/3 1/2)`（前項で既になっていた）が該当し、テストは interpreted
prelude に対して検査する形へ変えた。REPL では 1 回のタイプミスがセッションを落とすので、残作業
として [TODO.md](TODO.md) に記録した。

---

## `catch`/`throw`/`unwind-protect`（2026-08-16、branch `feature/compiled-unwind`）

CL 流の非局所脱出。interpreted 層は先に入っていた（checker・core form・`EvalError::Throw`・
評価）。ここに記録するのは **compiled 側**と、そこで方式が変わった経緯。

言語としての確定事項（コンディション非採用、タグが型を運ぶ、静的な脱出と動的な脱出を混ぜない）は
[language-design.md](language-design.md) §7.5 / §9。

### 方式が変わった: LLVM EH ではなく「保護呼び出し」

[TODO.md](TODO.md) には「方式は LLVM EH 一択」と書いてあり、`tests/compiled_unwind_test.rs` が
`invoke` + cleanup 専用 `landingpad` + `resume` の実現可能性まで実証していた。着手して分かったのは、
**それで `unwind-protect` は書けるが `catch` は書けない**ということ:

* catch は unwind を*止める*。Rust の panic を止めるとは例外オブジェクトを解放することで、
  それができるのは `std::panic::catch_unwind` だけである——例外オブジェクト自身の
  `exception_cleanup` は `__rust_drop_panic()` で abort し（"Rust panics must be rethrown"）、
  `panic_count::decrease` も `catch_unwind` からしか呼ばれない（`library/std/src/panicking.rs`）。
  landing pad で飲むと確保が漏れ、`std::thread::panicking()` がプロセスの残りの寿命ずっと true になる。
* 自前の foreign exception を投げれば上は回避できるが、**interpreted フレームを跨ぐ throw が死ぬ**。
  compiled → `rt_apply_any` → interpreted → compiled → throw の間には Rust フレームがあり、
  Rust の `catch_unwind` は foreign exception を掴むと `__rust_foreign_exception()` で abort する。
  throw の輸送体は Rust panic でなければならない。

⇒ **unwind を止める Rust フレームが要る。** 一度は「catch の本体を 0 引数クロージャに切り出して
Rust から呼ぶ」案を検討したが、ユーザー判断で却下。理由は本質的で、記録しておく価値がある:

> 例外処理（`catch`/`throw`）と制御構文（`break`/`return`）をごっちゃにしている。
> break の処理が例外処理とかちあわない設計にするべき。

`break` は**静的**な脱出で、行き先は checker が決めている（最内 `loop`、関数境界を越えない）。
compiled 側ではそれが `br` 命令であり、`loop-exit`/`loop-slot` はその LLVM 関数のブロックと
alloca である。catch の本体を別関数に切り出すと、`(loop ... (catch 'a (break)) ...)` の `break` が
「別関数のブロックへ飛ぶ」になり、表現できない。動的な脱出の実装都合で静的な脱出を壊すことになる。

**採った形**: 本体は同じ LLVM 関数に残し、`catch_unwind` は**保護領域内の呼び出し**に置く。
領域の中では call が `rt_protected_*` トランポリン（Rust 関数）を経由し、戻ったら
`rt_unwind_pending` を見て領域の dispatch ブロックへ `build-cond-br` するだけ——compiled 側から
見れば unwind もただの制御フローになる。LLVM EH（`invoke`/`landingpad`/`resume`/personality）は
一切使っていないので、AOT で `__gxx_personality_v0` が undefined になる問題も起きない。

### 構成

| 層 | 変更 |
|---|---|
| `typelisp-mem` | `Heap` に 4 つ目のルート `in_flight_throw`（飛行中の throw の値。3 つのスタックはいずれも「unwind が切り捨てる／解放しない／throw で括られていない」ので使えない） |
| `typelisp-rt` | `CompiledThrow` payload、park/take、`rt_throw`・`rt_throw_matches`・`rt_throw_take_value`・`rt_unwind_pending`・`rt_resume_unwind`、`rt_protected_{call,call_env,apply_any,dyn_call,panic,throw}` |
| checker/forms | `(catch TAG BODY REPR)` / `(throw TAG VALUE REPR)` — 投げた値は境界を跨ぐので `apply` と同じ理由で repr が要る |
| bridge/freevars | 3 タグの翻訳と walk。タグは `(sym ...)` と同じノードになるので、島は `rt_intern_symbol` 済みの tagged `Sexpr` を渡す |
| 島 | `protect Option<llvm-basic-block>`（今いる領域の dispatch ブロック）を `loop-exit` の隣に引き回し、`compile-catch`/`compile-throw`/`compile-unwind-protect` と `emit-direct-call`/`emit-closure-apply`/`emit-rt-call` を新設 |
| 境界 | `catch_compiled_panic` に `CompiledThrow` の腕、`unwind_interpreted_failure` が `EvalError::Throw` だけ throw チャネルへ回す、`rt_run_entry`（AOT）で捕まらない throw を報告 |

**入れ子はブロックの連鎖で解く**（領域のリストを引き回さない）。各領域の dispatch ブロックは、
自分のタグでない unwind を外側の dispatch ブロックへ `br` し、最外は `rt_resume_unwind` へ落ちる。
`compile-break`/`compile-return` は `protect` を**受け取らない**——静的な脱出が領域を参照しないことが
シグネチャに出ている。

### 実装中に見つかったこと

* **飛行中の throw の値は GC ルートが要る。** `EvalError::Throw` が運ぶ `Box<Value>` はコレクタから
  見えず、その値を root していたスコープは unwind が全部捨てる。`break`/`return` は「巻き戻しは
  何も allocate しない」で済んでいたが、throw は違う——`unwind-protect` の cleanup が走り、
  それは allocate する。`Heap::in_flight_throw` がその 1 スロット。interpreted 側の
  `Op::Throw`/`Op::Catch` も同じスロットを使う（park する側と解放する側が両経路で対になる）。
* **飛行中でないタグを残してはいけない。** トランポリンが throw 以外（panic・interpreted の失敗）を
  捕まえたときにタグを消さないと、上位の `catch` が panic を自分の throw として claim してしまう。
* **`kind = 0` は「そのタグを投げるものが無い」を意味する。** `(catch 'unused ...)` や body が
  `break` だけの catch はタグの型が決まらず `Repr::NotRepresentable` になる。dispatch ブロックは
  到達不能なので decode しない（`compile-sexpr-field` はここで panic する）。
* **`gc-stress` を最初から有効にすると prelude のロードで落ちる。** checker の quasiquote 経路
  （`check_qq_template` → `construct_form`）に既存のルート漏れがあり、この作業とは無関係に
  再現する。テストは prelude をロードし終えてから stress を入れる形にした。

### 静的な脱出でも cleanup を走らせる（同日、後続作業）

上の時点では `break`/`return` で保護領域を抜けたときに cleanup が走らず、interpreted 側
（`Op::UnwindProtect` は `EvalError::Break`/`Return` も「protected が抜けた」経路として扱う）と
挙動が食い違っていた。これを塞いだ。

**動的な機構には触れていない。** 行き先が静的に分かるなら、走らせるべき cleanup の並びも静的に
分かる。`unwind-protect` は cleanup ブロックを 3 つ目（正常路・dispatch に続く）として持ち、
`compile-break`/`compile-return` はそこへ `br` する。各 cleanup ブロックは自分の外側の cleanup へ、
最後の 1 つがループの出口へ `br` する——**分岐の鎖**であって、タグを見る dispatch ではない。
歩く関数も分けてある（`emit-static-exit-onward` と `emit-unwind-onward`）: 次の行き先が
静的に分かるか実行時にしか分からないかが違うので、共有すると混ざる。

引き回すのは `exit-cleanup Option<llvm-basic-block>` 1 つ（`protect` の隣、50 関数）。
`compile-break` はこれを受け取る——`protect` は受け取らないままで、**追う鎖が違う**ことが
シグネチャに残っている。

**鎖は 2 箇所でリセットが要る:**

* **ループ境界**（`compile-loop` は本体に `Option::none` を渡す）。`protected` の内側のループを
  抜ける `break` は `protected` から出ていないので、その cleanup は走ってはいけない。
  リセットを忘れると、内側の `break` が cleanup を走らせたうえ**外側の**ループまで抜ける。
  番人は `an_inner_loops_break_does_not_run_an_enclosing_cleanup`。
* **関数境界**（lambda 本体・labels の兄弟・`compile-function` の 3 箇所）。ブロックは関数に
  属するので、跨いで `br` すれば不正 IR になる。ここは元から loop 三点セットを `Option::none` に
  していた 3 箇所と同一。

`catch` は鎖に何も足さずに素通しする（cleanup を持たないので当然）。`break` が catch を跨いで
その内側の `unwind-protect` の cleanup だけを走らせることの番人が
`a_break_passes_through_a_catch_but_runs_a_cleanup`。

**GC:** `return` の値は loop の結果スロット（ただの `alloca`、GC ルートではない）に置かれたまま
cleanup が走り、cleanup は allocate する。`a_returned_value_survives_a_compiled_cleanup_that_allocates`
がこれを `gc-stress` 下で見ている。正常路の値（`unwind-protect` 自身のスロット）についても同じ
問いがあるので番人を置いたが、そちらは元から通っていた。なお島を `gc-stress` 下でコンパイルする
ことはできない（前項の prelude quasiquote ルート漏れに先に当たる）ので、**コンパイルを済ませてから
stress を入れる**ヘルパを使っている。

**島の台帳を 2 つ更新すること。** 特殊形を足したときは `editor/emacs/typelisp-mode.el` と
`editor/vscode/syntaxes/typelisp.tmLanguage.json` の両方（VS Code の alternation は長い順に並べる）、
島に `defun` を足したときは `tests/island_self_compile_test.rs` の `ISLAND_DEFUNS`。後者は
**同居する `every_island_defun_compiles` の仕事リストでもある**ので、更新を忘れると新しい関数を
一度もコンパイルしないまま緑になる。

#### 島の再生成は 1 回では足りないことがある（自己ホストの不動点）

`the_committed_island_matches_a_fresh_build` が、再生成した直後に落ちた。同サイズで
4568 バイト違う。順序（prelude を先に作ったせいでは）を疑ったが**外れ**——
`build_island_bitcode` は `prelude::load_interpreted` を使い、「`prelude_compiled.bc` に
依存すると循環する」と明記して避けている。実験でも prelude 再生成で島は変わらない。

真因は自己ホストのブートストラップだった。`build_island_bitcode` は**コミット済みの `.bc` を
install して、その `compile-function` に新しい SOURCE をコンパイルさせる**。`bootstrap.rs` の
doc は「何が出力されるかは SOURCE と Rust 側ビルダで決まり、どの世代が駆動しているかには
依らないので、結果からビルドし直せば再現する」と書いていたが、**その前提は「島の codegen を
変えていない」ときだけ成り立つ**。

今回は `compile-break` の `store` と `rt_truncate_sexpr_roots` の順序を入れ替えた——
つまり島が `break` に対して吐く IR そのものを変えた。だから:

* 1 回目 = 新 SOURCE を**旧**島がコンパイル → `break` 箇所は旧順序
* そこからの fresh build = 新 SOURCE を**新**島がコンパイル → 新順序 ⇒ 不一致
* 2 回目で不動点、3 回目・4 回目も同一

差分が `break` の 2 箇所だけだったのは、島自身のコードに `break` が 2 つあるから。
番人は正しく働いた（バイト比較でしか見えない差を止めた）ので、直したのは案内のほう
（`scripts/regen-compiler-island.sh` と `bootstrap.rs` の doc）。

**調査の道具について:** 成果物は `llvm-dis`/`llvm-bcanalyzer` がそのままでは読めない
（`Bitcode stream should be a multiple of 4 bytes in length`——ファイルは 4n+1 バイト）。
末尾 1 バイトを落とすと読め、そこで初めて「差は 2 箇所の命令順序だけ」と分かった。
バイト差 4568 だけを見て IR の差の大きさを推し量ると誤る。

余分な 1 バイトは inkwell 0.9 の**意図的な仕様**で、バグではない。`MemoryBuffer::as_slice`
は LLVM が保証する終端 NUL を含めて返す（`get_size` が `LLVMGetBufferSize() + 1`）。
対になる読み込み側 `MemoryBuffer::create_from_memory_range_copy`（`install_compiled_library`）が
**末尾 NUL を assert で要求し、実サイズとして `len - 1` を渡す**ので、成果物はローダが
要求する形で保存されている。**削ってはいけない**——しかも削っても assert は落ちない
（ビットストリーム自身の末尾はパディングのゼロなので）。1 バイト短い buffer が LLVM に
渡るだけで、失敗は分かりにくい形で出る。書き出し 2 箇所にその旨を書いた。

---

## 実行時エラーを abort でなく catchable な panic に（2026-08-17、branch `feature/compiled-unwind`）

`(panic ...)` は unwind するようになっていたが、**言語が定義するエラーのほう**——ゼロ除算・
範囲外アクセス・非正の `random` 境界——は compiled 経路で `typelisp_rt::fatal()` に落ち、
プロセスを abort していた。同じ式が経路によって違う結末になる:

```
interp   (get v 9)  → panic: Vector: index 9 out of bounds   exit 1
compiled (get v 9)  → non-unwinding panic. aborting.         exit 134
interp   (rem 5 0)  → abort（prelude が compiled なので）     exit 134
```

`unwind-protect` の cleanup も走らず、REPL は 1 回のゼロ除算で落ちる。

### 線引きは SBCL に合わせた

SBCL 2.6.7 で実測して決めた。compiled でも interpreted でも `DIVISION-BY-ZERO` /
`INVALID-ARRAY-INDEX-ERROR` を signal し、`unwind-protect` は走り、プロセスは生き残る
（`safety 0` を明示したときだけ検査が消える）。つまり**経路による食い違いは方針ではなく不具合**。

基準は「ユーザが踏めるか」ではなく、**壊れたのが*プログラム*か*ランタイム*か**:

| SBCL | 用途 | typelisp |
|---|---|---|
| condition | 言語レベルのエラー全部 | catchable な panic（コンディションは非採用なので unwind） |
| `lose()` | ランタイムの破壊のみ | `fatal()` |

`fatal()` の doc を「ランタイム破壊専用」に書き直した。残る ~190 箇所（arity 違反・タグ違い・
ルートスタック破壊）はコンパイラ契約違反なので abort が正しい。**safety レベルは導入しない**——
検査は常時。

### `raise` と `extern "C-unwind"`

`rt_panic` の末尾（`install_quiet_panic_hook` + `panic_any(CompiledPanic)`）を
`typelisp_rt::raise(String) -> !` として括り出し、以下を `fatal` から差し替えた。unwind する
関数は**すべて `extern "C-unwind"`**（plain `extern "C"` を越える unwind は abort と定義されている）:

| 関数 | メッセージ（interpreted 側に一致させた） |
|---|---|
| `rt_i64_div` / `rt_i64_mod` | `divide by zero` / `mod by zero` |
| `rt_bignum_div` / `rt_bignum_mod` / `rt_ratio_div` | 同上 |
| `rt_str_ref` | `ref: index {i} out of range (length {n})` |
| `rt_str_substring` | `substring: invalid range {a}..{b} (length {n})` |
| `rt_random_state_next` | `random: bound must be positive, got {n}` |
| `rt_struct_field_get` / `_set` | `Vector: index {i} out of bounds` |

`rt_struct_field_*` には **rt 側で上限検査を足した**（`checked_field_index`）。mem 層の
`panic!` を捕まえるのではなく、落ちる前に検査する——捕まえる先が無いから。

**`fatal` のまま残したもの**（どちらも呼び出し側の契約を確認したうえで）:

* `rt_struct_pop_field` の空 vector — 唯一の呼び出し元（`compile-vector-op` の `pop`）が
  `rt_struct_field_count` を先に見て `None` を自分で作る。ここに来る＝契約違反
* `rt_ratio_from_bignums` の分母 0 — 作れるのはリテラルだけで、リーダーが `1/0` を
  読む時点で弾く（CL のリーダーと同じ）

`i64::MIN / -1` は除数の問題ではないので `arithmetic overflow: {a} / {b}` と名乗る
（interpreted 側は Rust のオーバーフロー panic になる。既存の食い違いで、この作業の対象外）。

interpreted 側も 1 箇所だけ直した: 負の添字が `Vector: invalid index Int(-1)` と Rust の
`Debug` を漏らしていた。compiled 側は生の `i64` しか持っていないので同じ形は作れないし、
そもそも範囲外アクセスの一種なので `Vector: index -1 out of bounds` に揃えた。

### 島: 「上げうる呼び出し」は保護経由でなければならない

保護呼び出し方式では、**unwind を捕まえるのは呼び出しを行ったフレーム**である。だから
`catch`/`unwind-protect` 領域の中で素の `build-call` を出すと、そこから上がった unwind は
その領域の cleanup を素通りする。上げうるようになった shim は `emit-direct-call`
（`protect` が `Some` なら `rt_protected_call` 経由）に移した:

* `compile-assoc` — `rt_i64_div`・`rt_i64_mod`・`rt_str_ref`・`rt_str_substring`、
  および **`rt_bignum_div`・`rt_bignum_mod`・`rt_ratio_div`**
* `compile-vector-op` — `rt_struct_field_get`・`rt_struct_field_set`

2 引数のものは新しい島の defun `raising-binop-call` にまとめた（`bignum-binop-call` の
保護版）。**プランは bignum/ratio を「`rt_builtin_symbol` 経由で `compile-call` に乗るので
対応済み」と書いていたが、これは誤りだった**——`bignum->ratio` のような*自由*ビルトインは
確かにそちらを通るが、`/`・`mod` は型のメソッドなので `compile-assoc` の
`bignum-binop-call`（素の `build-call`）に出ていた。`random-state-next` だけは本当に
自由ビルトインで、既に保護対応だった。

触っていないもの: `compile-field-get`/`_set`・`compile-struct-field`・`compile-box-field`
——添字が checker 由来の定数で、範囲外になりようがない。GC ルート操作などの残り ~97 の
素の `build-call` も unwind しない。

### テスト

**主眼は差分テスト** `tests/runtime_error_parity_test.rs`: 同じ式を interpreted と compiled の
両方で走らせ、**同じ `EvalError::Panic` メッセージ**になることを 15 ケースの表で確認する。
今回の穴はどれも「両パスが食い違う」形で現れたので、番人もその形にした。加えて cleanup が
走ること・`catch` は panic を claim しないこと・セッションが生き残ること。

AOT は `tests/compile_file_test.rs` に 2 本（exit 134 → exit 1 + `panic: divide by zero`、
および unwind-protect 経由）。`tests/numeric_test.rs` の `run_interpreted`（`random` 用の
逃げ道）は削除した。

### 途中で見つかった既存バグ

差分テストが並列実行で SIGSEGV した。調べると **`ExecutionEngine` の drop だけが
`COMPILE_LOCK` の外にある**——`llvm::Module` のデストラクタは共有 `LLVMContext` の値名
テーブルを書き換えるので、インタプリタを捨てるスレッドが compile 中のスレッドと競る。
**この作業とは無関係**で、変更を stash した状態の `tests/numeric_test.rs` が 3/3 で落ちた
（クラッシュレポートは片方が `MCJIT::~MCJIT`、もう片方が `install_compiled_library` の
ビットコード parse）。次の項で直した。

## LLVM オブジェクトの破棄を COMPILE_LOCK の内側へ（2026-08-17、同ブランチ）

上で見つけた既存バグの修正。`COMPILE_LOCK` の doc は「LLVM Context に触る操作を直列化する」と
書いていたが、**破棄がその規則から漏れていた**。何かを*呼ぶ*わけではないので、呼び出しに
ついて書かれた規則をすり抜ける。

### 直し方: ロックを取れる場所まで破棄を遅らせる（`retire_llvm`）

素直な案——`Drop` で `COMPILE_LOCK` を取る——は**デッドロックになる**。`COMPILE_LOCK` を
保持したまま LLVM オブジェクトがスコープを抜ける場所が正当に沢山ある（ガードの下でモジュールを
組み立てる `compile_test.rs` のケース群、`CompiledFn` を置き換える `install_compiled_library`/
`compile_scc`）。`Mutex` は再入不可なので、稀なクラッシュを確実なハングに取り替えるだけになる。
`std::sync::ReentrantLock` はまだ unstable（1.90 で確認）。

採ったのは**引退リスト**。`CompiledFn::drop` はエンジンの share を
`compile::retire_llvm` に渡すだけで、実際の破棄は次に誰かが `COMPILE_LOCK` を取って
コンパイルするとき（`CompiledFn::new`/`new_multi` の先頭）に行う。drop する側は文脈を
問わないので、どこで死んでも安全になる。

`unsafe impl Send` が 1 つ要る。根拠は**移動しかしないこと**: 引退も回収も move であって
中身に触らない。実際に走る操作は生成とデストラクタだけで、どちらもロックの下にある。
非アトミックな `Rc` の増減も同様——増えるのは `new_multi` の `clone`（ロック下）、減るのは
回収時の drop（ロック下）だけで、`ExecutionEngine` の share は `CompiledFn` の外に出ない。

代償は「次のコンパイルまで引退したオブジェクトが残る」こと。二度とコンパイルしない
プロセスはもう終了するところなので実害はない。

### 素の `Module` は別扱い（引退させられない）

エンジンに渡していない `Rc<RefCell<Module>>` を持つ 4 箇所（`compile_scc`・`aot::compile_file`・
島と prelude の bootstrap）は、ガードより**前**に宣言されているせいで drop 順が逆——ガードが
先に落ちてから Module が壊れる。ここは引退リストに入れずに、ガードを保持したまま明示的に
`drop(module)` する形にした（`?` の早期 return もその経路を通るよう、末尾を `result`
束縛に書き換えた）。

**引退リストに入れなかったのは意図的**: Rc の share を*クローンして*リストに預けると、
所有者スレッドと回収スレッドが同じ非アトミックカウンタを触りうる。最後の share を move
できると保証できない以上（島のハンドルレジストリが一時的に share を持つ）、`Rc` は
そのまま扱うほうが安全。

### 番人

`tests/llvm_teardown_race_test.rs`。**自前でスレッドを 3 本立てる**ので、
`--test-threads=1` の `scripts/test-serial.sh` でも素の `cargo test` でも同じ意味になる。
各スレッドが「prelude を install した Interp を作って走らせて捨てる」を 12 回繰り返す。
修正を戻すと 3/3 で SIGSEGV、入れると 3/3 green（8 秒）。レースなので証明ではなく探針だが、
壊れたときに必ず数秒で出る。

副産物として `tests/runtime_error_parity_test.rs` の直列化（`ONE_SESSION_AT_A_TIME`）を
外せた。並列でも落ちなくなり、実行時間も 53 秒 → 33 秒。`tests/numeric_test.rs` も
素の `cargo test` で 4/4 green（修正前は 3/3 で SIGSEGV）。

## `i64::MIN / -1` を両経路で同じ失敗に（2026-08-17、同ブランチ）

差分テストの表に載せられずに残っていた最後の食い違い。compiled は
`arithmetic overflow: ... / ...` を raise していたが、interpreted は素の `a / b` で
**Rust のオーバーフロー panic**——つまりプロセスが落ちる側だった。

`eval_int_builtin` を `checked_div` にして、compiled と同じ文言の `EvalError::Panic` に
した。CL ならここで bignum に広がるが、式の宣言型は `i64` であり、チェックの通った
プログラムに要求より広い型の値を返すことはできないので、失敗が正しい。

テストは表に 1 行足した。`i64::MIN` は**リテラルとして書けない**（リーダーは
`-9223372036854775808` を bignum として返す）ので `(- (- z 9223372036854775807) 1)` で
作る。第 1 引数を `i64` 型の変数にするのは、`-` の受け手型が「`expected: None` で
検査した第 1 引数」から決まるため——`0` と書くと `i32` に落ちる。

---

## checker の quasiquote に残っていた GC ルート漏れ（2026-08-17、branch `fix/checker-gc-root-leaks`）

`gc-stress` を最初から有効にすると prelude のロードで落ちる件。2026-08-16 の
`catch`/`throw` 作業のときに見つけて「この作業とは無関係」として置いてあったもの。

落ち方は `gc-root-audit: push_root given freed cell`——ルート漏れ用に常設してある O(1) の
検出装置（[[typelisp-gc-negative-reclaim-bug]] で入れたもの）が、`construct_form` の
`Items::extend` で既に解放されたセルを掴んだ。

**原因は 1 行**。`check_qq_template` の `,@`（unquote-splicing）の腕だけが、作った
`(call sexpr-append ...)` ノードを `forms::rooted` に通さずに返していた。`check_at` の
契約は「返すノードは必ず root 済み、解放は `check_form_at` の `truncate_roots` が一括で」
なので、ここだけが穴だった。

**なぜ prelude の一部の `,@` でしか出ないか**: 漏れが観測できるのは、ノードが親に届くまでの
間に誰かが allocate したときだけ。`,@` がテンプレートの**末尾**にあると、返った先の
`check_qq_template` はもう何もしないので助かる。途中にあると、親は自分の `cdr` を検査し
`construct` を組む——その最初の `cons` で回収される。

番人は `tests/checker_gc_stress_test.rs` に追加した。`,@` は prelude の `sexpr-append` を
要求するので、prelude を**素で**ロードしてから stress を入れる専用ヘルパを足してある
（prelude 自体を stress 下で検査すると 1 ロード 6 分半かかるうえ、見たいのはそこではない）。
修正を戻すと同じ audit で落ち、入れると通る。3 秒。

## LLVM ハンドルレジストリの解放もロックの内側へ（同日、同ブランチ）

前日に `ExecutionEngine` の破棄を引退リストでロック下に入れたが、TODO に「素の
`Module`/`Builder` が残る」と書いた分。島のハンドルレジストリ（スレッドローカルの
`NativeHandle` 表）が `Rc` share を持ち、その解放は 2 箇所ともロック外だった:

* `llvm_handles_release`（`run_compile_function` が 1 コンパイルごとに呼ぶ）
* **スレッド終了時**のレジストリ自身の破棄——`run_compile_function` は `argv` を組んだ
  *あと*に mark を取るので、コンパイルを駆動したモジュールの share は release の括りから
  外れ、スレッドの寿命まで残る

前者は「先に取り出してからロック下で drop」、後者は `LlvmHandles` newtype の `Drop` で
ロックを取る形にした。**引退リストには入れない**——`Rc` の share は全部同じスレッド
（レジストリはスレッドローカル）にあるので、その場でロックを取れば計数は単一スレッドのまま
デストラクタだけをロック下に置ける。別スレッドが drain するリストに移すと、そこが崩れる。

**正直に書いておく**: こちらは**クラッシュとして再現できていない**。
`tests/llvm_teardown_race_test.rs` に「3 スレッドがコンパイルして終了する」テストを足したが、
修正を戻しても 3/3 green（ラウンドを倍にしても同じ）。エンジンの破棄と違って観測された
不具合ではなく、「破棄は Context に触る操作だからロックの内側」という規則に揃えただけ。
テストの doc にもそう書いた——通ったことを「危うい経路を守れている」と読まないために。

## 「呼ぶとコンパイルできなくなるもの」を潰す（2026-08-18）

`docs/syntax.md` §10 に、呼ぶと `(compile f)` が通らなくなる組み込みの表があった。6項目
（システム組み込み・等価述語・印字一式・`read`・`eval`）。この作業でそれを1項目（`eval`）まで
減らした。手口は2つで、**ネイティブ shim を書く**か、**インタプリタ側にしか無い実装を
切り出してランタイム側へ下ろす**か。

### システム組み込みと等価述語（shim を書いた側）

`parse-int`/`parse-float`/`get-universal-time`/`get-internal-real-time`/`exit`/`string->symbol`
を `typelisp-rt` の `sys_builtin` に、`eql`/`equal`/`equalp` を `equality` に切り出した。
どちらもストリーム層と同じ「実装は1つ、両側に薄い edge」で、インタプリタも同じ関数を呼ぶ。
`eq`/`eql`/`equal`/`equalp` が `i32`/`i64`/`string`/`Sexpr` の全てで lowering できるように
なったので、**`case` が全型でコンパイルできる**ようになった。

その過程で既存バグを1件見つけた: compiled 側の文字列 `eq`/`eql` が*内容*比較、interpreted 側が
*同一性*比較で、同じプログラムが tier によって違う答えを出していた。コメントに「eq/eql/equal
再設計より前の名残」と書いてあったもの。identity に揃え、24 通りの比較を両 tier で走らせて
一致を確認した。

### 印字とリーダ（切り出した側）

`format`（1,977行）と pretty printer（979行）を `typelisp-print` クレートへ、リーダ（950行）と
`name_lexer` を `typelisp-read` クレートへ移した。どちらも `#[no_mangle]` の `rt_*` シムを
**自分のクレートの中に**持つ。

#### なぜモジュール分割では足りないか（実測）

`compile-file` がリンクするアーカイブは `typelisp-rt` の `staticlib` ただ1つで、リンカは
アーカイブを**メンバ単位**で引く。だから印字エンジンを `typelisp-rt` の 1 モジュールとして
置くと、rustc が小さい CGU をマージして `rt_cons` と同じオブジェクトに同居させ、そのオブジェクトは
必ず引かれるので、印字しないプログラムにも書式エンジンが入る。`(defun main () i32 42)` の
AOT 出力で 3,474,808 → 4,089,872 バイト。

クレートを分ければメンバは分かれる。ただし**依存の書き方に2つの罠**がある。

1. 参照が1つも無い依存クレートは、rustc が staticlib に同梱しない（実測: `ar t` で
   `typelisp_print` のメンバ 0 個）。`typelisp-rt` に `pub use typelisp_print;` を1行置くと
   117 個入る。この行はコード生成を伴わず呼び出し元も無いので、dead として消すと
   compiled な `println` が全部リンクエラーになる。消させないための注記をコードに書いた。
2. rustc は opt-level 0 と 1 で**ジェネリックの単型化をクレート間で共有**する。共有された
   単型化は参照なので、`typelisp-rt` のオブジェクトが `Vec::len` を求めて `typelisp_print` の
   メンバを引き、その巻き添えで印字エンジンが入る（debug で 4,083,736 バイト・印字シンボル
   237 個）。`[profile.dev.package.typelisp-print] opt-level = 2` で共有が止まり 3,510,608 バイト・
   0 個になる。release は元から共有しないので、この設定は release の挙動を debug に戻すだけ。

最終的な実測: 印字もリーダも使わない AOT 実行ファイル 3,530,224 バイト（印字 0 個・リーダ 0 個）、`println` を
1つ足すと 3,877,648 バイト（印字 124 個）、`read` を1つ足すと 3,647,960 バイト（リーダ 24 個、
印字は 0 個のまま——片方を使っても他方は入らない）。

#### `typelisp-abi`

印字クレートのシムは `encode`/`active_heap`/`fatal` を要るが、それらは `typelisp-rt` にあり、
`typelisp-rt` は `typelisp-print` に依存しているので循環する。両者の下に `typelisp-abi`
（タグ付きワード表現・暗黙のアクティブ `Heap`・シムの死に方）を切り出した。`ACTIVE_HEAP` が
**1つの thread-local である**ことが要点で、2つに割れるとシムがインタプリタの登録した
ヒープを見失う。

#### ヒープから読めない2つの事実

印字はヒープを見ても分からないものが2つある——enum の変種*名*（箱は index しか持たない）と、
型が `print-object` を持つかどうか。`PrintEnv` トレイト越しに渡す。インタプリタは自分の
スコープ木から答え（`ModuleScope::enum_variant_name` を新設。従来の
`collect_struct_and_enum_types` はプログラム中の全型のマップを毎回作っていた）、AOT 実行
ファイルは `build_main_wrapper` が生成した起動時登録のテーブルから答える。

起動時登録は、**モジュールが実際に印字シムを call しているときだけ**出す。出すこと自体が
印字エンジンへの参照だからで、無条件に出すと上の分離が無意味になる。判定は IR を歩いて
call 命令の callee 名を見る（`module_calls_any`）。

AOT の制御変数（`*print-pretty*` 等）は登録しない。`compile-file` はコンパイラ島だけを読み
prelude を読まないので、AOT プログラムにそれらのグローバルは存在せず、CL の初期値が唯一の
正解になる。`BARE_HOOKS` がそれをそのまま与える。

なお AOT で `print-object` を書けるのは `(defmethod print-object ...)` の形だけ。
`(impl print-object ...)` はトレイト本体が prelude にあるため——これは以前からの制限。

### 副産物: `src/project.rs` の GC ルート漏れ

ローダが1ファイル分の検査済みフォームを**ルートされていない `Vec<Value>`** に溜めていた。
コレクタから見えないので、次のフォームの検査が起こした GC で回収されうる。この作業で
アロケーションが増えて既定ヒープサイズで顕在化し、`not a top-level core form: (())` になった。
`check_impl` で過去に直ったのと同じ形のバグ。落ちるところで root し、`pop_roots_to` を
`push_permanent_root` の後ろへ動かした。回帰テストは `tests/loader_gc_test.rs`（修正を戻すと
実際に落ちることを確認済み）。

## `eval` をコンパイルできるようにする（2026-08-19）

上の表に残っていた最後の1つ。**表は空になった**——組み込みを理由にコンパイルできない関数は
もう無い。作業は2段で、(A) フロントエンドのクレート分離、(B) 実行ファイルの中の環境。

### (A) `typelisp-front` クレート

`check/` + `eval/` + `types.rs` + `type_key.rs` + `project.rs` + prelude の SOURCE、約 20,000 行が
`crates/typelisp-front` へ移った。`typelisp` は `pub use typelisp_front::{...}` で従来の
モジュールパスを再輸出するので、外から見た `typelisp::Heap` や `typelisp::check::core` は同じ。

依存の向きを逆にするのが前提だった。移す前は `eval/interp.rs` が `llvm-*` ビルダ（約1,550行）と
JIT ドライバを直に抱えていた。それぞれ `src/compile/llvm_builtins.rs` と `src/compile/driver.rs`
へ出し、インタプリタは関数ポインタの `Backend` 構造体越しにだけ backend を呼ぶ。ドライバは
`impl Interp` のメソッドから自由関数になった——分割後は他クレートの型に `impl` を書けないので。

事前に「一番深い結合」と見立てた `FnDef.compiled` は、実際には浅かった。`CompiledFn` は
`{engine, addr}` の2フィールドで、engine は機械語を生かしておくためだけ、フロント側の利用は
全部 `.address()`。読む側の半分だけを名指しする `CompiledBody` トレイトで済んだ。逆に重かったのは
ドライバのほうで、`Module` を作る/受け取るメソッドが `impl Interp` の中に散在していた。

`crossing.rs`（`catch_compiled_panic` 等）は backend からフロントへ**下ろした**。LLVM の型を
1つも名指しておらず `EvalError` だけで書かれている、評価器の側の境界だったので。

**なぜモジュールでなくクレートか**は印字・リーダと同じ理由（リンカはアーカイブをメンバ単位で
引く）。AOT がリンクするアーカイブは `libtypelisp_rt.a` → `libtypelisp_front.a` へ移した——
`cc` が取るアーカイブは1つで、front は rt の上にあるから両方入るのは外側だけ。`eval` を
呼ばないプログラムのサイズは **+784 バイト・front のシンボル 0 個**（`(defun main () i32 42)`）。

`opt-level = 2` は front には要らなかった。共有ジェネリクスは**下流へ流れる**ので、引かれうるのは
「リンク済みのものが上に乗っているクレート」だけ。print/read は rt の下だから要る、front は rt の
上だから要らない。0 シンボルで一致し、ビルドは 4分27秒 → 26秒。前回「新しくクレートを切ったら
同じ設定を足すこと」と書いたのは広すぎた。

番人3本がパスのずれを検出した。うち `type_identity_guard_test` は規則2本が「スキャンがファイルを
見失ったから」通っており、捕まえたのは `the_scan_covers_the_files_the_invariant_lives_in` のほう。
守備範囲と危険の範囲を一致させ直した。

### (B) 実行ファイルの中の環境

`eval` は「現在の大域環境」に対して型検査する。単体の実行ファイルにはそれが無いので、
`compile-file` は `eval` を呼ぶプログラムに限って起動列を生成する（判定は印字と同じ
`module_calls_any`）:

1. `rt_eval_source(ptr, len)` — プログラム自身のソースを埋め込んで渡す。
2. `rt_eval_global(name, id)` — 各グローバルのコンパイル済みスロット id。
3. `rt_heap_init(1 << 18)` — 既定の `1 << 16` では prelude が入らない。
4. `rt_eval_init()` — prelude を解釈実行で読み、プログラムのソースを読み直して定義を登録する。
5. コンパイル済みグローバル初期化列 → `tl_main`。

**4 が 5 より前にある位置は選択でなく強制**。`Interp::new` は `typelisp_rt::reset_global_table`
を呼ぶので、初期化列の後に `Interp` を作るとコンパイル済みコードが番号で触るスロットを消す。
その代わり `eval` は遅延ではなく起動時に組み立てられる。

グローバルの記憶域は共有する。`Interp::bind_compiled_global` で `compiled_globals` に
`path → id` を入れておけば、`global_core`/`set_global_core` がモジュール木より先にそれを見るので
読みも書きもコンパイル済みスロットへ行く。**`defvar` の初期化子を二度走らせない**のが設計上の
要点で、読み直しのほうは「すでにコンパイル済みスロットを持つ `defvar`」を `exec` しない
（typelisp の `defvar` は CL と違って毎回代入するので、素通しすると初期化子の副作用が2回走り、
プログラムがそれまでに書いた値も消える）。

eval したフォームは解釈実行される。プログラム自身の関数を呼んでも、走るのは読み直しが登録した
解釈実行用の本体のほう。結果は同じで速度だけが違うので、コンパイル済みアドレスの登録は入れて
いない。

#### 見つけたバグ: 宙に浮いた `ACTIVE_INTERP`

最初の実装は AOT で 100% CPU のまま止まらなかった。サンプルを取ると `rt_eval` →
`with_active_interp` → `HashMap::get` で回っていた。`rt_eval_init` の中で prelude を読む過程が
`install_print_hooks` を走らせ、**そのときスタックにあった `Interp` のアドレス**を
thread-local に書く。その直後に `Box::leak` で移動させたので、以降そのスロットは宙に浮く。
AOT には「次の crossing で書き直される」機会が無い（JIT の `enter_compiled` に当たるものが
無い）ので、そのまま印字とこのシムの両方が deref する。移動先で登録し直して解消。

`rt_eval` の分岐も「アクティブな `Interp` が居るか」ではなく `AOT_ENV` が非 null かで見るように
した。前者はスロットの鮮度が保証される文脈でしか意味を持たない問いで、どちらの deployment かを
判別する問いではない。

#### もう1件: `Value::Path` だけを見ていた

「この `defvar` はもう初期化済みか」の判定で `core::field(...)` の結果を `Value::Path` として
だけ受けていた。ルート直下の名前は裸の `Value::Symbol` で入っているので、**修飾されていない
グローバルを全部取りこぼす**。`core::path_field` が両方を吸収してくれるので、そちらを通す。
症状は「初期化子が2回走る」ではなく「読み直しの最中に、まだ存在しないコンパイル済みスロットを
触ってエラーになる」だった。

### 環境をコンパイル時に組み立てる（同日、第2版）

最初の版は環境を**起動時に**組み立てていた——プログラム自身のソースを実行ファイルに埋め込み、
prelude ともども読み直して型検査していた。実行ファイルが同じプログラムを機械語とソーステキストの
2つの形で持つことになり、起動に 0.18 秒（debug ビルド）かかっていた。

`compile-file` はその環境を作れる立場にいるので、コンパイル時に1回作って書き込むことにした
（`crates/typelisp-front/src/snapshot.rs`）。起動は復元だけ。**0.18 秒 → 0.09 秒**（同条件）、
front を最適化ビルドすると **0.05 秒**・実行ファイル 8.4MB。

保存するのは「型情報」ではなく**検査済み状態**である。署名だけでは `(eval '(f 1))` は型検査を
通ったあと実行するものが無い。中身は `Registry` の `root`/`docs`、`catch`/`throw` のタグ型、
ジェネリックの保持テンプレート、事前宣言名、そして**検査済みトップレベルフォーム全部**——復元は
それを `exec` してインタプリタ側の表を埋める。`DefLocs` だけは意図的に落とした（goto-definition
用で、読むのは `typl-lsp` だけ）。

これは 2026-08-14 に削除した fasl と同じ「検査済み状態のシリアライズ」だが、**削除の論拠は
ここには当たらない**。当時の理由は「実行を1ミリ秒も速くしない」で、それは LSP と REPL の用途では
正しかった。AOT 実行ファイルの起動では、その read+check こそが唯一のコストである。復活させたのは
機構そのものではなく考え方のほうで、汎用のモジュールキャッシュ（ハッシュ・`~/.typl/cache`・
`compile-module` サブコマンド）は無い。

形式は `serde` + `bincode`。JSON でないのは、旧 fasl が 2,383 行に対して 2,499,193 バイトの JSON で、
24.6ms のロードのうち約 17ms が JSON パースだったという実測があるため。`bignum`/`ratio` は
`num-bigint`/`num-rational` の `serde` feature を有効にせず**10進表記の文字列**で持つ——Cargo の
feature はグラフ全体に効くので、有効にすると全 AOT 実行ファイルがリンクするクレートにまで
シリアライザが入る。`Loc` も同じ理由で `typelisp-mem` に `serde` を足さず `serde(remote)` で書いた。
番人（`an_executable_that_never_evaluates_carries_no_interpreter`）が front シンボル 0 個を
確認しているので、これらが漏れていないことは測れている。

スナップショットのバイト列は**再現可能ではない**。`Namespace` が `HashMap` でできていて bincode は
反復順に書くため。今のところ誰も依存していないので、順序つきマップへ変える価値は無いと判断した。

### テスト

AOT 9本（`compile_file_test`）: 式の評価、自分の関数の呼び出し、プログラムが書いたグローバルの
読み、eval が書いたグローバルをプログラムが読む、初期化子が1回だけ走ること、eval した定義が
次の eval から見えること、そして **`eval` を呼ばない実行ファイルに front のシンボルが 0 個**。
スナップショット版で2本足した——**プログラムが一度も呼んでいない prelude のジェネリックを eval が
初めて単型化する**（復元したテンプレートしか答えられない。prelude の 86 defun 中 61 がジェネリック
で、生のテンプレートはヒープ `Value` を持つ）と、**prelude のマクロを eval が展開する**（`MacroDef`
と展開本体という別々の半分が両方復元されていないと通らない）。
JIT 3本（`eval_builtin_test`）: コンパイル済み本体からの `eval`、そこからグローバルが見えること、
そこで作った定義が残ること。

## ダンプ: ビットコードと型情報を対にする（2026-08-20、branch `feature/typelisp-dump`）

### 動機と測定

`typl` の起動 1.50s の内訳を先に測った（`typl-bench-prelude` に段別計測を追加、debug ビルド）:

| | read | predeclare | check | exec | 再ハッシュ | ローダ全体 |
|---|---|---|---|---|---|---|
| prelude（114KB / 2206 行） | 27.3ms | 2.1ms | 136.1ms | 3.4ms | — | 373.5ms |
| 島（282KB / 4355 行） | 56.6ms | 7.9ms | 307.3ms | 1.4ms | 70.3ms | 1019.9ms |

ネイティブ本体は `.bc` に焼いてあるのに、その本体が誰なのかを語る型情報だけは毎起動で
ゼロから組み直していた。島は SOURCE を 2 回読んでいた（本体の read と、鮮度ハッシュ用の
`island_source_hash` がもう 1 回で 70.3ms）。

**測って初めて見えた別件**: 内訳の合計は read+check が 597ms に対し、**ビットコードの
install（LLVM のパース + JIT）が 781ms**。この作業が消せるのは前者だけである。

### SBCL の dump を調べた結論

`save-lisp-and-die` は**ヒープをそのまま書き出しているだけ**だった。SBCL ではコンパイル済み
コードも環境も 1 つのヒープ上のオブジェクトなので、コードを保存する専用機構が要らない。
typelisp はそうではない——ネイティブ本体は LLVM の JIT メモリ上のアドレス、型情報と定義表は
cons アリーナの外にある Rust の `HashMap`。3 つのうちヒープなのは値だけなので、**メモリ
イメージ方式は原理的に取れない**。構造的には SBCL 系ではなく ECL 系（ネイティブコンパイル
方式の処理系は `save-lisp-and-die` 相当を持たず、コンパイル結果をリンクする）。

そこから来た設計判断:

- **linkage table の再構築はもう在った。** SBCL が `FOREIGN-REINIT` で `dlsym` のアドレスを
  起動時に書き直しているのと同じことを、`CompiledFn::new_multi` が `rt_*` について毎ロード
  やっている。ビットコードに焼いてあるのは名前だけ。ここは新規作業なし。
- **BUILD_ID → `FORMAT_VERSION`。** 「コアとランタイムの binary compatibility は一切無い」
  という SBCL の割り切りをそのまま採る。版違いはエラー。
- **`(dump)` はプロセスを殺さない。** SBCL が die するのは破壊的 GC でイメージを壊すため。
- **値は保存しない。** ダンプが保存するのは定義で、`defvar` の初期化式はロード時に走り直す。
  この 1 点だけが SBCL と意味的に違う。おかげで「保存できない値」（ストリームハンドル、
  クロージャの関数ポインタ、外部メモリ）という問題群がまるごと消えた。

### 形式

拡張子 `.typld`。**単位（unit）の並び**で、ロードは先頭から順に適用する。ヘッダは
マジック + 版 + 単位数 + 各単位の (types, bitcode) の offset/len。

**「`.bc` の後ろに型情報を足す」ではなくヘッダを置いた理由**: ローダは
`MemoryBuffer::create_from_memory_range_copy` で、inkwell の規約により末尾 1 バイトが NUL で
あることを要求し実サイズとして `len - 1` を渡す。ファイル全体を渡せば LLVM はビットストリームの
後ろに未知のバイト列を見ることになり、成功は保証されない（現に `llvm-dis` は末尾 NUL 1 バイト
だけで拒否する）。ヘッダがあれば `&image[off..off+len]` を渡すだけで、LLVM には従来と 1 バイトも
変わらないスライスが渡る。ファイルは 1 つのまま。

単位が持つのは**その単位が足したもの**だけ（`UnitState`: 検査器の差分、検査済みフォーム、
ビットコードが本体を持つ定義、グローバルとそのスロット id）。累積状態にすると島の単位が
prelude を丸ごと抱えて 1MB 太るうえ、ロード時に検査器を丸ごと差し替えることになり、prelude 側で
permanent root 済みのテンプレート 61 本が二重に root されて回収できないセルになる。差分なので
`load_prelude` / `load_compiler_aot` のシグネチャも呼び出し側 158 箇所も無改造で済んだ。

### 差分の取り方——ここが一番の設計上の勘所

「before に無いキー」では**足りない**。`impl` ブロックは既存の `AdtDef` の `assoc` を育てるし、
再定義は `FnSig` をその場で置き換える。つまりエントリは*追加*されるだけでなく*変更*される。
なので比較はキーではなく**内容ハッシュ**で行う: 各エントリを bincode して、そのバイト列を
ハッシュする（`RegistrySignature`）。ハッシュが無いか違えば差分に入り、適用時は上書きになる。

**そして最初の実装はこれで動かなかった。** 番人テストが「prelude の型と trait が全部 differ」と
言ってきた。原因は `AdtDef.assoc` などが `HashMap` で、**bincode はマップを反復順に書く**——
Rust の `HashMap` は `RandomState` なので、同じ内容でも走るたびに違うバイト列になる。
レジストリの入れ子マップ 7 つ（`TraitBound.assoc` / `FnSig.bounds` / `AdtDef.assoc` /
`AdtDef.trait_assoc` / `TraitDef.methods` / `TraitDef.defaults` / `BlanketImpl.bounds`）と、
checker 内の置換マップ（`HashMap<String, Type>`）を `BTreeMap` に変えて解決した。`Path` に
`Ord` を足したのはこのため。副産物としてダンプのバイト列が**再現可能**になり、
`the_committed_*_matches_a_fresh_build` がそのまま成り立つ。名前空間自身のマップは
`HashMap` のままだが、walk がキー順に並べ替えてから書く。

### グローバルのスロット id

順序ではなく**割り当てられた (path, id) の対**を書き出す。`Interp::promote_global` は宣言順に
呼ばれるだけでなく、コンパイル駆動側が本体の中で参照を見つけた時にも呼ばれるので、
「ソースを走査し直せば同じ順番になる」が成り立たない。ロード時は記録された順に
`promote_global` を呼び、**返った id が記録と一致しなければエラー**にする。ここを間違えると
「読み書きするアドレスが 1 つずれる」という静かな壊れ方をする。

### 結果

| | 起動 |
|---|---|
| 移行前 | 1.50s |
| prelude だけ移行 | 1.39s |
| 島も移行 | **1.07s** |

成果物は prelude 1.31MB（`crates/typelisp-front/src/prelude.typld`）、島 2.67MB
（`src/compiler_island.typld`）。prelude の成果物を front クレート側に置いたのは、AOT 実行
ファイルが `typelisp-front` しかリンクしないため——`compile-file` の eval 環境をここから
取れるようにする布石（次段）。

島の成果物は自己ホスト（前世代の島が次世代を吐く）なので再生成が 2 回必要になることがあるが、
今回はコンテナを変えただけで emit される IR は変わらないため 1 回で不動点に達した（md5 一致）。

### `compile-file` の eval 環境も同じ形式に

`snapshot.rs` を廃止し、AOT 実行ファイルが埋め込むのを「prelude 単位 + プログラム単位」の
2 単位にした。prelude の検査済み状態はコミット済み成果物からそのまま持ってくるので、
`compile-file` はもう毎回 prelude を読んで型検査し直さない（2 ファイルのコンパイルで
4.36s → 3.22s）。

**`capture_program_dump` を backend 側に置いた理由**が実測で決まった。front に置くと
`prelude::DUMP` を参照するオブジェクトが eval 入り AOT 実行ファイルへ引き込まれ、使えない
prelude のビットコード 418KB ごと 1.3MB 増える（14.4MB → 13.0MB）。リンカはアーカイブの
メンバ単位で引くので、「front に置くか backend に置くか」はそのまま実行ファイルのサイズになる。

旧 snapshot 比では実行ファイルが ~0.5MB 増える（コンテナのパーサと差分機構のぶん）。
引き換えに、eval の環境が `typl` 自身と同じ成果物から来るようになった。

### `(dump ...)` と `typl --image`

ユーザから見えるダンプ。書くのは「読み込んだ単位をそのまま並べたもの + セッション自身の単位」で、
出力は自己完結する。セッション中に `(compile f)` したものは、**ダンプ時にビットコードへ
再出力**する（JIT の結果は LLVM のメモリ上のアドレスであって、ファイルに載るものではない）。

**記録の粒度で 1 回間違えた。** 最初は「深さ 0 の `exec` で記録する」ようにしたが、`typl` は
スクリプト 1 本をファイル由来モジュール 1 つとして実行するので、深さ 0 のフォームは
`(module mk ...)` ただ 1 つ——しかもその `exec` が返るのは中の `(dump ...)` が走った**後**で、
記録は空のままだった。正解は「あらゆる深さで記録し、コンテナは自分自身を記録しない」。
各定義が自分の `exec` で自分を記録するので順序も保たれ、ファイルモジュールの
トップレベル式（`(dump ...)` 自身を含む）は入らない。

グローバルは初期化式を走らせ直した値で戻る（SBCL との唯一の意味的な差、既述）。
`tests/dump_image_test.rs` が `typl` バイナリを 2 回起動して、
定義・マクロ・コンパイル済み本体が渡ることと、トップレベル式が再実行されないこと、
グローバルが初期値で戻ること、他ビルドのイメージが拒否されることを確かめている。

---

## CL 残差を埋める（2026-08-20、Phase 0 / 1c / 2 / 3 / 4a 前半）

計画は [cl-parity-plan.md](cl-parity-plan.md)、進捗表は [TODO.md](TODO.md)。ここには
「なぜそうしたか」と、途中で見つけたものを書く。

### 何が入ったか

prelude に約 200 の名前。**全て `PRELUDE_COMPILE_UNSUPPORTED` に穴を開けずに通っている**ので、
JIT/AOT 対応込み。文字・文字列カタログ（Phase 2）、リスト/シーケンス/集合演算と破壊的操作
（Phase 3）、数値の残り（Phase 1c）、マクロで書ける制御形（Phase 4a 前半）。
テストは `char_string_catalog_test` 18 / `seq_catalog_test` 18 / `numeric_catalog_test` 8 /
`control_forms_test` 6 の計 50 本。

### 名前の付け方は既存の慣習に従った

計画 §1.1 は「受け手優先・Rust 風、CL 名は薄い別名」としていた。実際には**既にこのファイルが
選んでいた慣習に合わせる**方が一貫した——`alpha-char-p`→`alphap`、`zerop`/`evenp` の
`p` 接尾辞、`char->int`/`char->string` の変換名。なので `char-lessp`→`lessp`、
`upper-case-p`→`upper-casep`、`char-name`→`char->name`、`digit-char`→`digit->char`、
`make-string`→`string::filled`。別名は増やしていない（CL 対応は functions.md の表が持つ）。

Phase 3 では計画から**外れた**。§1.2 は「同じ CL 名を受け手型ごとに `defmethod` で定義する」と
していたが、既存の `Iter` ライブラリ（`map`/`filter`/`length`/`nth`/…）がジェネリック `defun` で
書かれているところへ `defmethod` を混ぜると、同じ名前が受け手の形で 2 通りに解決されうる——
計画自身のリスク表が `member` を名指しで警告していたのと同じ形。`Iter` に統一し、`member`/
`member-if`/`member-if-not` は 3 つとも `bool`（CL は残りのリスト）で揃えた。破壊的操作
（Phase 3d）だけは受け手を書き換えるので `Vector<T>` の `defmethod`。

### 島に lowering の無い組み込みを踏まない、という制約

Phase 2a の最初の草稿は `(upcase c)`/`(alphap c)` を素直に呼んだ。prelude の再生成が
`PRELUDE_COMPILE_UNSUPPORTED` の照合で止まり、`char::upcase`/`char::downcase`/`char::alphap`/
`char::digitp` と `i32::int->char` に lowering が無いこと、そしてそれを踏むと**その節の全定義が
インタプリタ専用に落ちる**ことを教えてきた。計画 §2-2 が「穴が開けば毎ビルドで落ちて教えて
くれる」と書いたとおりに機能した番人。

以後、Phase 2 も Phase 1c も全て `char->int` のコードポイント上と lowering 済みの演算だけで
書いてある。文字を*作る*ところは `(ref "0123456789ABC..." w)` のように `string::ref` で引く。
`scale-float` が `(* self (expt 2.0 (int->float n)))` でなく 2 倍/半分のループなのも、
`rationalize` が連分数の収束項を整数でなく `f64` で持つのも同じ理由。
**この 5 つの lowering を足す作業は残タスク**として計画に立ててある（Phase 1a と同じく
島の分岐を増やす作業なので、まとめると安い）。

### 見つけた checker のバグ 4 件——全部同じ形だった

**「型変数の名前がたまたま一致したときだけ動いていた」**。4 件が独立に見つかって、
4 件とも同じ診断になった。

1. **境界越しの `Self` 戻り型**（Phase 0.2 で発覚）。`check_instance_method` の bounds 分岐が
   トレイトメソッドの戻り型に**関連型しか**代入しておらず、`Self` を返すメソッドを
   `(where (Add T))` の下で呼ぶと `expected t, found self` になった。prelude のトレイト
   メソッドは 1 つも `Self` を返さない（`Eq`/`Ord` は `bool`、`Iter` は関連型）ので、
   これまで一度も踏まれていない。`Number` トレイト層（Phase 1a）の前提条件なので先に潰した。
2. **境界付きジェネリック同士の委譲**（Phase 3a で発覚）。`validate_where_bounds` が
   呼ばれ側の宣言ピン（呼ばれ側の型パラメータで書かれている）と呼び出し側のピンを
   **素のまま**比較していた。`elt`→`nth` が通っていたのは両方 `A` と綴っていたからで、
   `B` と綴れば落ち、構造化されたピン（`(Item cons-cell<K,V>)`）は一度も一致しなかった。

   ここは**自分の Phase 0 の結論も間違っていた**。当初「制約は既に解消済み」と書いたのは、
   検証に使った例がたまたま `A` を使っていたからで、*なぜ*動いたかを確かめていなかった。
   計画にも両方の訂正を残してある。
3. **ジェネリック `defmethod` の受け手の型パラメータ名**（Phase 3d で発覚）。
   `check_assoc_call` は `def.params` を受け手の具体引数と zip して特殊化するのに、
   登録される署名は `defmethod` が書かれたままの名前を保っていた。`Vector<A>` は
   `a` を置換しないまま具体引数を拒否し、`Vector<T>`（`Vector` 自身の宣言と同じ名前）は通る。
   登録時に所有型の名前へ書き換えるようにした——本体は書かれたままの名前で検査する
   （本文がそう書いてあるので）。
4. **compiled 経路の `format` が `f64` パラメータを生ワードで渡していた**（Phase 2b で発覚）。
   `wrap_rest_elem` が `is_heap_repr` の真を根拠に `Sexpr` 構成子を飛ばしていたが、
   `is_heap_repr` が答えているのは**インタプリタの**表現。compiled な `f64` は箱でもタグ付きでも
   ない生の `f64::to_bits` パターンで、下位 3 ビットがたまたま `Int` タグに読めるため
   `to_bits(x) >> 3` が印字されていた。`f64` だけ近道から外した
   （`string`/`bignum`/`ratio` は両世界で同じタグ付きの語なのでそのまま）。
   [[typelisp-crossing-must-be-type-driven]] と同じ形の誤りで 4 回目。

4 件とも回帰テスト付き（`trait_test` 2 本、`generic_defun_test` 4 本、`compile_test` 1 本）。

### 直さずに記録した CL との差

- **`round` の丸め方**。`round`（したがって Phase 1c で足した `fround`、既存の `round-div`）は
  Rust の `f64::round` をそのまま使うので **0 から遠い方へ**丸める。CL は偶数側なので
  `(round 2.5)` は CL で `2`、ここで `3.0`。`fround` を `round` と一致させる方を優先して
  差を引き継いだ（別々に丸める 2 つの名前が並ぶ方が悪い）。直すなら `round` 本体で、
  島が lowering しているので島側も同時に変わる。
- **n 引数の `/=`** は隣接ペア比較（`(/= a b a)` が真）。CL は全ペア相異を問う。
  可変長比較の糖衣が全ての比較演算子に対して選んでいる既存の意味で、`char`/`string` に
  `/=` を足したことで見えるようになっただけ。

### 対象外にしたもの（理由つき）

`list*`（「末尾を差し替えた不完全リスト」という概念が無い）、`copy-tree`/`copy-alist`/
`sublis`/`subst`/`subst-if`（任意深さの異種の木を走査する型が書けない）、
プロパティリスト一式 `getf`/`get-properties`/`symbol-plist`/`remprop`（キーと値が交互に並ぶ
無型のリストという表現が無く、同じ役割は `assoc` か `HashTable`）、可変文字列
（`Vector<char>` + `to-string` で足り、`string` の `eq` が `Rc::ptr_eq` である前提を崩す
代償に見合わない）、`Sexpr` 版の `rplaca`/`nconc`（cons セルが `car` のソース位置を
セル内に持つので、書き換えると以後の診断が静かにずれる）。

保留（設計判断が要る）: 乱数のシード指定（Rust プリミティブが要る）、`ldb`/`dpb` の
`i64`/`bignum` 拡張（`defmethod` は受け手でしか解決せず、CL の `(ldb bytespec integer)` は
指定子が先なので整数側の幅で実装を選べない）、`block`/`return-from`（島の
`compile-value` が `loop-exit`/`loop-slot` を全呼び出し地点に引き回しているので、
名前付き脱出先の*スタック*を足すと自己ホストコンパイラ全体に触る）、`destructuring-bind`。

---

## CL 同等カタログ Phase 9c — 実行環境（2026-08-20）

[cl-parity-plan.md](cl-parity-plan.md) の Stage 9c。コマンドライン引数・環境変数・
ファイルシステムへの問い合わせ・日時の分解合成・`y-or-n-p` が入った。
計画が「費用対効果で先に着手するなら」と名指ししていた 4 つのうち、最後の 1 つ。

### 組み込み*関数*は組み込み*メソッド*より安い。ただし島の再生成は要る

計画の §2-4 は「Rust builtin を足すときの触点は 8 箇所」とし、島の
`*-native-method?` と lowering を数えていた。**それはメソッドの話だった。**

`externs.rs` の `rt_builtin_symbol` の doc コメントが書いているとおり、島は
「bridge が名付けたシンボルを呼ぶ」だけで、自分では何も導出しない。だから自由関数を
足すのに `src/compiler.rs` の SOURCE を書き換える必要は無い——証拠として、
`file-exists-p` はそこに一度も現れないのに JIT でも AOT でも通る。

触点は registry（署名）→ interp（解釈側）→ rt（実装と `#[no_mangle]` シム）→
`externs.rs` の 3 表（シンボル表・`use` 一覧・アドレス表と**その配列長**）の 4 段。

**ここで一度間違えた。** この節には最初「島の再生成も要らない」と書いていたが、
全スイートで `the_committed_island_matches_a_fresh_build` が 452 バイト差で落ちた。
`rt_extern_functions()` に足した 11 個は島のビットコードに extern 宣言として現れる。
**「SOURCE を書き換えなくてよい」と「成果物のバイト列が変わらない」を混同していた**——
`src/compiler.rs` に名前が出てこないことが示すのは前者だけで、後者の証拠にはならない。
[[typelisp-島のハッシュは「読んだ形」に対して取る]] が名指ししている
「成果物の入力は 2 つだがハッシュは 1 つしか見ていない」の、もう一方の入口だった。

切り分けは Phase 1c/2/3/4a との対比で出た:

| 変えたもの | 島の成果物 |
|---|---|
| prelude を大幅に育てた（1c/2/3/4a、+43%） | **変わらない**（島テストは通った） |
| prelude に加えて registry へ組み込みを足した（9c） | **変わる**（452 バイト差） |

つまり島の成果物を動かすのは prelude の中身ではなく **extern の表**。
再生成は 1 回で不動点（島が*吐くもの*は変えていないため、2 回目も同じ 2,672,364 バイト）、
prelude 成果物のほうはバイト単位で不変だった（島 → prelude の順で回したが、
prelude 側は 1,867,175 バイトのまま）。

### `file-` は命名規約ではなく経路規則

最初の草稿は `file-truename` 以下 5 つを `sys_builtin.rs` に置いた。動かない。
`Interp::eval_builtin` に

```rust
name if name.starts_with("stream-") || name.starts_with("file-") =>
    self.eval_stream_builtin(heap, name, args),
```

というアームがあり、**この接頭辞を持つ名前は解釈経路では必ず `stream_builtin.rs` へ行く**。
そこに実装が無ければ、他のどこに書いても届かない。5 つとも `stream_builtin.rs` の
`dispatch` へ移し、シムは既存の `stream_shim!` マクロに 1 行ずつ足すだけになった
（手書きしていた 5 個ぶんの `unsafe extern "C"` が消えた）。

命名にも影響した: プリミティブは `file-modified-date`、prelude 側の CL 名は
`file-write-date`。同じ根名前空間に両方は置けないので、`file-exists-p`/`probe-file`、
`file-delete`/`delete-file` と同じ「プリミティブと CL 名を別綴りにする」既存の規約に従った。

### `command-line-args` の要素 0 を両世界で一致させる

`typl script.typl a b` と AOT の `./prog a b` では `std::env::args()` が食い違う
（前者は先頭が `typl`、`--heap-cells` 等の大域フラグも混ざる）。同じソースが
どちらの走らせ方でも同じ添字で同じ引数を読めないと、この関数を足す意味がほぼ無い。

`sys_builtin` に `OnceLock<Vec<String>>` を置き、`typl` の `main` が
「最初の非フラグ引数（＝ファイル名）以降」を渡す。AOT 側は何も設定せず、自分の argv を
そのまま読む。`build_main_wrapper` が生成する `main` は `argc`/`argv` を取らない
（`i32 ()` として作られる）が、Rust の `std` はプロセス開始時に argv を捕まえている
（macOS は `_NSGetArgv`、Linux は `.init_array`）ので問題にならない。

両方で実際に確かめた:

```
$ typl /tmp/argv_probe.typl alpha beta     $ /tmp/argv_aot alpha beta
count=3                                     count=3
prog=/tmp/argv_probe.typl                   prog=/tmp/argv_aot
rest=alpha                                  rest=alpha
```

### 暦は Hinnant の公式をそのまま

`decode-universal-time`/`encode-universal-time` は Howard Hinnant の
`civil_from_days`/`days_from_civil` を CL の紀元（1900-01-01）へずらしたもの。
表も閏年の場合分けも要らない厳密な整数演算で、prelude に typelisp で書いてある。

書く前に Python で同じ式を——`/` は 0 方向切り捨て、`mod` は床、という
この言語の意味論を再現して——20,005 件（`datetime` との突き合わせと往復）検証した。
実装後の初回テストが 12 本中 11 本通り、落ちた 1 本も暦とは無関係だったのはそのため。

1 箇所だけ意味論に気をつけた: `local` が負になりうる（小さい `ut` に正の `zone`）ので、
`mod`（床）と `/`（切り捨て）が食い違う。先に剰余を取り、差を割ることで除算を厳密にした。

### `y-or-n-p` はテストに書けないので、パイプで確かめた

この 2 つは `*standard-input*` を読む。Rust のテストからだと*プロセスの* stdin を
渡すことになり、手で `cargo test` を叩いた人の端末が繋がっていれば永久に止まる。
代わりにパイプで確認した:

```text
$ printf 'maybe\nY\nno\n' | typl yn.typl
delete everything? (y/n) delete everything? (y/n) true
really? (yes/no) false
```

効くべき 3 つ——受け付けない答えは訊き直す、大小文字を問わない、`no` は偽——が全部出ている。
経緯はテストファイルの冒頭コメントにも残した。

### 保留したもの（理由つき）

`get-internal-run-time`（CPU 時間）と `file-author` は `getrusage` / uid→名前の引き当てに
`libc` が要り、ワークスペースは `libc` に依存していない。実時間で CPU 時間を代用するのは嘘。
`machine-instance` 等のホスト名系も同じ理由、`software-version`/`short-site-name`/
`long-site-name` は CL でも `NIL` を返してよいので、中身の無い定数を並べるより置かない方を選んだ。
`trace`/`untrace`/`step`/`disassemble`/`room`/`ed`/`dribble` は REPL のツール層で別作業。

### 直さずに記録した CL との差

**`decode-universal-time` は zone 省略時に UTC へ分解する**（CL は地方時）。
ランタイムがタイムゾーンデータベースを持たないため。CL の 9 個の返り値のうち
`daylight-p` と「既定の分解が使った zone」は、偽の値を返すのではなく用意していない。
CL にもある明示 zone 引数（グリニッジ以西の時間数）が代わり。

### 副産物: この計画の範囲外の既存問題 2 件

どちらも main で再現し、どちらも設計判断が要るので直していない。詳細は
[cl-parity-plan.md](cl-parity-plan.md) の付録 D。

1. **AOT 実行ファイルから prelude の関数が一切呼べない**。`aot::compile_file` は
   `load_compiler`（島）しか呼ばず `load_prelude` を呼ばないので、`abs`/`gcd`/`identity`/
   `parse-namestring` すら `no such function` になる。組み込みと自分の定義だけが使える。
   この計画で足したものはほぼ全て prelude にあるので影響は小さくない。
   `compile_file_test` がこれを踏んでいないのは、どのテストもコンパイル対象のコードから
   prelude 関数を呼んでいないため。**JIT（`(compile name)`）は無関係**——prelude が載った
   プロセスの中で動くので通る（既存の `abs` と Phase 9c の `directory-p` で確認）。
2. **小さいヒープで GC がスラッシュする**。`Heap::cons` は「空なら `gc()`、それでも空なら
   `grow()`」の順なので、**`gc()` が 1 セルでも回収すると `grow()` は呼ばれない**。
   生存量が容量にわずかに届かない状態で毎回全体マークが走る。
   `editor_keyword_sync_test`（`Heap::with_capacity(1 << 16)`）は 6 テストで 649.74 秒、
   同じ `load_prelude` を `1 << 18` で呼ぶ `prelude_test` は 1 回 1.2 秒——約 90 倍。
   `1 << 16` は `src/main.rs` の `DEFAULT_HEAP_CELLS` でもある。

## 付録 D の 2 件 — GC のヘッドルームと、AOT が持ち歩く prelude（2026-08-21、branch `feature/cl-parity`）

[cl-parity-plan.md](cl-parity-plan.md) 付録 D。どちらも Phase 9c の作業中に踏んで
「計画の範囲外・設計判断が要る」として置いてあったもの。両方入れた。

### D-2: 回収したかどうかではなく、**空きが残ったか**で伸ばす

`Heap::cons` の規則は「free リストが空になったら `gc()`、それでも空なら `grow()`」だった。
**1 セルでも回収できれば `grow()` は呼ばれない。** 生存量が容量にわずかに届かない状態に
入ると、次の `cons` がまた free リストを空にするので、以後ほとんどの割り当てが
生存集合全体のマークを引く。

規則を「回収後の空きが容量の 1/4 に届かなければ伸ばす」に変えた（`HEADROOM_DIVISOR`）。

**固定アリーナの約束は 1 文字も変わっていない。** `set_growth_limit(0)` の heap では
`grow()` が最初の行で false を返すので、追加した呼び出しは何もしない——
`Error::HeapExhausted` は今も「回収が何も返さず、成長も許されていない」を意味する。
計画本文が「固定アリーナという設計上の約束と相談が要る」と書いていた相談は、
`grow()` 側が既に答えを持っていたので不要だった。

テストは壁時計でなく **GC 回数**を見る（`Heap::gc_count` を新設）。
生存 900 / 容量 1000 へ 20,000 回割り当てると、旧規則では約 200 回の全体マーク。
逆向きのテスト（生存 100 / 容量 1000 なら伸びない）も置いた——
ヘッドルームで測るということは、ゴミが十分あるワークロードは伸ばさないということ。

**ただし計画が挙げた 649.74 秒の内訳は GC だけではなかった。** 修正後 502.24 秒で、
残りを `sample` で見ると `install_compiled_library` → `LLVMGetFunctionAddress`、
つまり **prelude ビットコードの JIT** とその `COMPILE_LOCK` 待ちが支配的だった。
`editor_keyword_sync_test` は 6 テストがそれぞれ `load_prelude` を呼ぶので 6 回 JIT する。
「`1 << 16` だと 108 秒、`1 << 18` だと 1.2 秒、だから約 90 倍」という計画の推論は、
**容量差と「prelude を JIT するかどうか」の差を分離していなかった**。
GC スラッシュは実在した（上のテストが示す）が、この 1 本の主犯ではない。

### D-1: AOT 実行ファイルは prelude を**持ち歩く**

`compile::aot::compile_file` は `load_compiler`（島）しか呼んでおらず、
出来た実行ファイルは組み込みと自分の定義しか使えなかった（`abs` すら `no such function`）。
`prelude_bootstrap::load_for_aot` を足して 3 つのことをする:

1. **ビットコードを出力モジュールへ `link_in_module`。** 位置が効く——島の `compile-call` は
   呼び先を `get-function` でこのモジュールに探し、無ければ**プロセスを abort** するので、
   「書き出す時」ではなく「最初の本体を翻訳する前」に居なければならない。
2. **prelude の `defvar` を起動列の先頭に。** コンパイル済み本体はグローバルを
   焼き込んだスロット番号で読むので番号は付け直せない。`load_for_aot` が `compile_file` の
   どの行より先に走るぶん、採番も prelude → ファイルの順になる。
3. **ファイルが出す本体を全部、翻訳の前に宣言する。** ここが計画に無かった発見。

#### 発見: 単型化の束は呼び出し順に並んでいない

`(length (iter v))` を含むファイルが `tl_vector-iter::next <i32>` が無いと言って abort した。

最初はこう考えた——「ジェネリックの実体は prelude 成果物にも（本体が無いので）
ファイルのフォームにも（checker のキャッシュに当たるので）現れないのではないか」。
それを確かめるためにノード一覧を出したら、**実体は全部ファイルのノードに居た**:

    file nodes: ["length <vector-iter<i32>,i32>", "vector-iter::next <i32>",
                 "vector-iter::set-pos <i32>", ..., "main"]

問題は在不在ではなく**順序**だった。単型化の束（`MONO_BUNDLE_MODULE`）は
チェッカーが実体を*作った*順に並ぶので、`length <...>` がそれの呼ぶ
`vector-iter::next <i32>` より前に来る。`compile-file` は「先に定義された関数しか
呼べない」というファイルの規則に乗っていたので、束の中では成立しない。

直しはファイルが出す全シンボルの**前方宣言**を翻訳の前に置くこと——prelude 成果物の
生成器が自分の item 一覧に対してずっとやっていたのと同じこと。
prelude が入るまでこれが問題にならなかったのは、ジェネリックを*名指しできない*
ファイルはジェネリックを*実体化もしない*からで、穴は前からそこにあった。

**没にした最初の直し**: 「JIT と同じく呼び出しグラフで駆動する」と考えて
`compute_sccs` の推移閉包を足した。動くが、実測すると閉包は**常に空**だった
（上のとおり実体はファイル側に居るし、prelude が実体化したものは prelude 成果物に
本体がある——`build_prelude_artifact` は collection を通った item を全部コンパイルするので）。
一度も何も足さないコードは安全網ではなく死んだ枝なので消した。

#### 副次: `eval` 環境のグローバル束縛が全ユニットぶんになった

`dump::restore_dump` は「プログラム自身のユニットのグローバルだけが他人の所有物」
としていた。実行ファイルが prelude のグローバルも作るようになったので、
**全ユニットで `bind_globals` する**。そうしないと `eval` した `*print-pretty*` が
コンパイル済みコードとは別の記憶域を読む。

#### 踏んだ罠: 永続ルート

最初 `load_for_aot` は集めた `defvar` フォームを `heap.push_root` で押さえていた。
`load_unit` と島のロードがその上で押しては戻すので、**厳格な LIFO の途中に
戻されないルートを挟んだ**ことになり、フォームが回収されて
「global initializer が `defvar` でない」という遠くの症状になった。
`push_permanent_root`（LIFO 非依存の第 2 のルート集合）が正しい道具だった。
[[typelisp-permanent-gc-root]] が置かれた理由そのもの。

## CL 残差 Phase 1a/1b — 全ての幅を「数」にする（2026-08-21、branch `feature/cl-parity`）

[cl-parity-plan.md](cl-parity-plan.md) の Stage 1a（演算トレイト化と全型カタログ）と
Stage 1b（全数値型間の変換）。着手前の状態は計画の §1.3 が書いているとおりで、
`i8`/`i16`/`u8`/`u16`/`u32`/`u64`/`isize`/`usize`/`f32` は **`defmethod` の受け手として
型登録だけされていて、メソッドが 1 つも無かった**（`+` すら）。

### 幅は静的な区別であって、それ以上ではない

これを決めてから作業した。実行時の値は `Value::Int(i64)` と `Value::Float(f64)` の 2 つで、
どの型が貼られていても同じ——つまり `u8` の算術は 8 ビットで巻き戻らないし、`f32` の算術は
f32 精度に丸めない。

**これは新しい妥協ではなく、`i32` が最初からそうだった扱いの拡張**である
（`i32` の加算も 32 ビットでは巻き戻らない。`eval_int_builtin` の doc コメントが
「静的な型が `i32` か `i64` かに関わらず一様な `i64`」と明言している）。
本当に幅どおりに巻き戻す/丸めるなら、インタプリタと島の両方に幅を運んで両方でマスク・
`fptrunc` する必要があり、片方だけ直せば**解釈と compiled が食い違う**——いちばん悪い結末。
そこまでやる価値があるかは別の判断なので、`docs/functions.md` §1 に明記して据え置いた。

この決定のおかげで **Stage 1b は変換表ではなく張り替えになった**。整数どうし・浮動小数点
どうしの `as` は実行時に何もしない（`try-as` は常に `some`）。変換メソッドの名前に幅が
出てこないのも同じ理由で、`as_conversion` は型の*族*の間だけを表にし、
`check_as` が結果を要求された幅へ張り替える。

### 触点

計画の §2-4 が数えた「Rust builtin の触点 8 箇所」のうち、*メソッド*側の 4 段を全部通った:

| 段 | やったこと |
|---|---|
| registry | `int_assoc(ty)` を全整数型に、`float_assoc(ty)`（引数化した）を両浮動小数点型に |
| interp | `*type_name == Path::root("i32") \|\| ...` の分岐を `is_int_receiver`/`is_float_receiver` に |
| externs | `native_lowered_primitive_methods` の 2 アームに型名を追加、番人テストの `PRIMITIVES` 表も |
| 島 | `int-receiver-type?`/`float-receiver-type?` を新設し、`compile-assoc` の 2 箇所の型名比較を置換 |

型名のリストは `types.rs` の `INT_TYPE_NAMES`/`FLOAT_TYPE_NAMES` に 1 つ置いて Rust 側 3 箇所が
参照する（島は自分の SOURCE に持つ——番人は `the_rust_and_island_native_method_lists_agree`）。

### 演算子はトレイトメソッドになれない。だから綴りを対応させた

Phase 0.2 が見つけていたとおり、`(deftrait Add () (+ ((self Self) (other Self)) Self))` は
`impl` の時点で `cannot redefine built-in method `+`` に当たる。トレイト側は
`add`/`sub`/`mul`/`div`/`remainder`/`bit-and`… という綴りにして、**チェッカーが逆側から
埋めた**: 受け手の型が `where` で束縛された型変数のとき、演算子をその束縛のメソッド名へ
綴り直す（`Checker::trait_operator_method`）。だからジェネリックコードは今までどおり
演算子で書ける:

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T)) (+ a b c))
```

**綴り直しは「どの束縛も名乗っていない名前」に対してだけ**行う。束縛が自分でその名前を
宣言していれば必ずそちらが勝つので、ユーザのトレイトが `+` という*関数*を持っていても
奪われない。

`Neg` は入れなかった。`(- x)` は `check_unary_negate_or_invert` が `(- (- x x) x)` に
脱糖するので、単項マイナスに要るのは `Sub` だけ——トレイトを 1 つ足すより、脱糖が既に
そうなっている事実を記録するほうが正しい。

---

## CL 残差 Phase 1d — 複素数（2026-08-21、branch `feature/cl-parity`）

[cl-parity-plan.md](cl-parity-plan.md) の Stage 1d。

### 計画が指した先例は、ここまで届かない

計画は「`bignum`/`ratio` の先例に倣って」`TAG_BOXED` のヒープ箱 + Rust 側の算術、と
書いていた。**その先例が成り立つ理由がここには無い**——`bignum`/`ratio` が Rust に
あるのは `BigInt`/`BigRational` の算術がこの言語で書けないからで、`f64` 2 つの複素数は
`f64` の算術そのものである。

そこで prelude の `defstruct` にした。新しい `Repr` も `rt_*` シムも島の lowering も
成果物の手術も要らず、**書いたその日に通常経路で compile される**（`bignum` 対応が
どれだけの触点を要したかは同ログの該当節のとおり）。

```lisp
(pub defstruct complex (pub re f64) (pub im f64))
```

フィールドが `pub` なので `z::re` は他のどの構造体とも同じに読める。`realpart`/`imagpart`
は同じ読み出しの CL 綴り。

### CL から外れた 2 点（どちらも静的型が強いる）

1. **成分は `f64` 固定**。CL の complex は有理数も持てて `(complex 1 2)` と
   `(complex 1.0 2.0)` は*別の型*だが、静的型は 1 つ選ぶしかない。超越関数が揃って
   返すほうを取った。
2. **`(sqrt -1.0)` は今までどおり実数の NaN**。CL が実関数から complex を返せるのは
   `sqrt` の戻りが合併型だからで、ここでは `f64` の `sqrt` は `f64` を返すしかない。
   複素数は複素数の引数から出る:`(sqrt (complex::new -1.0 0.0))` が `i`。

どちらも `docs/functions.md` §2.6 に書いた。

### 演算子はここでは名乗れる

Stage 1a では `impl Add i32` のメソッドを `+` と綴れず `add` にした。**その制約は
primitive が組み込みメソッド表を持っていることから来る**ので、`complex` には掛からない
——`(pub defmethod + ((self complex) (other complex)) complex ...)` がそのまま通る。
`=`/`/=` も同様で、`Eq` は `(impl Eq complex ...)` がその `=` へ委譲する。

`Ord` は入れていない。複素数体は順序体ではなく、CL の `<` が複素数を撥ねるのと同じ理由。

### 実数側にも生やしたもの

`realpart`/`imagpart`/`conjugate`/`phase` は `f64` にも定義した。CL でも実数は虚部 0 の
複素数として扱えるので、呼ぶ側がどちらを持っているか知らずに実部を取れる。

### `(atan y x)`

CL の 2 引数 `atan` は prelude の `atan2` に落とす。`defmethod` はアリティで
オーバーロードできず 1 引数 `atan` は `f64` の組み込みメソッドなので、チェッカー側で
綴り替える——2 引数 `log` と同じ形（新設した `Checker::check_renamed_call`）。

### 検証

`tests/numeric_widths_test.rs` に 6 本追加（1a/1b の 9 本と合わせて 15/15 green）:
四則、`(sqrt -1)` が `i` になること、オイラーの等式 `e^(i*pi) + 1 = 0`（丸め許容）、
`phase`/`abs` の極形式の対、実数側アクセサ、`(atan y x)`。
prelude 成果物は 2,105,392 → 2,193,844 バイト。

---

## CL 残差 Phase 3e — シーケンス API のキーワード引数（2026-08-21、branch `feature/cl-parity`）

[cl-parity-plan.md](cl-parity-plan.md) の Stage 3e。`:key`/`:test`/`:test-not`/
`:start`/`:end`/`:from-end`/`:count` を、`Iter` 上のジェネリック `defun` 30 本に持たせた。

### 前提が要らなかった

計画は「前提: `lambda` と `defmethod` の `&optional`/`&key` 対応（Phase 5b）」と書いていたが、
**Phase 5b は要らなかった**。Stage 3a が受け手を `Iter` 実装型に統一した結果、対象は全部
`defmethod` ではなく**ジェネリック `defun`** で、`defun` の `&optional`/`&key` は既に通って
いたからである。まだ届かないのは逆側——`Vector<T>` の破壊的操作と `string` の
`search`/`mismatch` は `defmethod` なので、そちらが Phase 5b 待ち。

### `Option<(fn ...)>` は書けない。だから開封は宣言した関数の中に残る

キーワードはすべて**デフォルト無し**で宣言する。デフォルトを書いた `&optional`/`&key`
パラメータの宣言型は自分の型パラメータに触れられず（`check_defun_opt_key`)、
`:key`/`:test` は要素型の関数型なので、それに当たる。デフォルト無しなら制限は無く、
本体には `Option<...>` として届くので、既定値は本体で与える。

ここで詰まったのは共有ヘルパーのほう。`Option<(fn (A) A)>` は**型として綴れない**——
リーダはジェネリックトークンを最初の括弧で打ち切る（`extend_angle_token`。`(a<b c)` を
呼び出しとして読み続けるための意図的な設計で、「括弧を含まない、1 行に収まる、釣り合った
トークンだけが生き残る」とコメントが明言している）。`&key` パラメータはその型を*誰も
綴らずに*得るので成立するが、キーワードを受け渡すヘルパーは書けない。

そこで**キーワードの開封は宣言した関数の中に残し、コアへ渡すのは
`(fn (i32 A) bool)` のクロージャ**にした。コアは 7 本:

| コア | 役目 |
|---|---|
| `seq-in-bounds` / `seq-flag` / `seq-limit` | `Option<i32>`/`Option<bool>` の読み出し |
| `seq-find-core` / `seq-position-core` / `seq-count-core` | 前向き 1 パスの探索 |
| `seq-edit-core` | `remove`/`substitute` 系（2 パス） |
| `seq-any-core` / `seq-sort-core` | 集合演算の所属判定 / `:key` 付き挿入ソート |

`:from-end` を探索側は**逆走査でなく「break をやめる」**で実装した。一方向の使い捨て
カーソルにとって「終端から」とはそういうことで、off のときの費用がゼロで済む。
`:count` と `:from-end` が同時に来る `remove`/`substitute` だけは、どの一致に効かせるかを
決める前に総数が要るので 2 パスになる（結果はどのみち新しい `Vector` なので、実体化は
余分な費用ではない）。

### `:key` は要素型の中に閉じる

`(fn (A) A)`。`(fn (A) B)` と宣言すると `:key` 省略時に `B` を決めるものが無く、本体の
恒等フォールバック `((none) y)` が `A` と `B` の不一致で落ちる——実際に試して確かめた。
異なる型への射影（CL の `(find 3 alist :key #'car)`）は `-if` 系にラムダを渡すほうで書ける。
これは CL の `:key` が「`#'` とラムダが冗長だから」存在する機能で、こちらにはラムダがある。

項目ベースの探索では `:key` は**要素にだけ**掛かる（探している項目には掛からない）——
CL の規則。集合演算では両辺とも要素なので両方に掛かる。

### `:test` は境界と競合しなかった

計画は「`Eq` 境界版と `:test` 版のどちらを既定にするか設計判断が要る」としていたが、
CL 自身が `:test` の既定を `eql` としているのと同じ形——**既定は `Eq` 境界の `equals`、
`:test` を渡すとその場で差し替える**——で足り、名前を分ける必要は無かった。

### 見つけたもの 3 件

1. **`check_call_opt_key` に関連型ピンの推論が無かった（修正済み）。** `where` の
   `(Iter I (Item A))` だけで決まる型変数を `check_call` は推論するのに `&key` 版はして
   おらず、`(remove-duplicates (iter v))` が "cannot infer type parameter `a`" で落ちた。
   `check_call` の該当ブロックを `Checker::infer_pinned_assoc_types` に括り出して両方から
   呼ぶようにした。
2. **`lambda` が `match` のアーム束縛を捕獲すると compile できない（未修正）。**
   最小再現と原因は [TODO.md](TODO.md) に残した。この節の prelude はその形を避けて
   書いてある——`:test-not` の否定をラムダにして返さず、比較地点で `match` を展開する。
3. **`where` 付き `defun` は前方参照できない。** `predeclare_program` は SOURCE の実行前に
   走るので `deftrait Iter` がまだ登録されておらず、`where` 節が解決できずヘッダごと
   黙って捨てられる（この pass は「名前を足すことしかできない、決して拒否しない」設計なので
   エラーも出ない）。`prelude.rs` の冒頭コメントが「ヘルパーを先に置くのは今や必要でなく
   慣習」と書いているのは、**`where` の無い `defun` についてのみ**正しい。
   `seq-edit-core` が `copy-seq` を呼ばず実体化を書き下ろしているのはこれが理由。

### 挙動を変えたもの

**`remove-duplicates` の既定**。以前は無条件に最初の出現を残していたが、CL の既定は
最後を残す。以前の挙動は `:from-end true` で得られる。`docs/functions.md` §6.1/§6.3 と
`seq_catalog_test` の該当テストを直した。

**副産物**: `position-if-not` を足した。この節の見出しコメントが「CL が持つ `-if`/`-if-not`
の対を全部」と書いているのに 1 つだけ欠けていた。

### 検証

`tests/seq_keywords_test.rs`（14 本）。`seq_catalog_test`/`prelude_artifacts_test`/
`prelude_compiled_test`/`editor_keyword_sync_test` も green。

---

## CL 残差 Phase 4b — 拡張 `loop` DSL（2026-08-21、branch `feature/cl-parity`）

[cl-parity-plan.md](cl-parity-plan.md) の Stage 4b。CL の LOOP を、`:named` を除いて入れた。

### 分岐は第 1 要素、CL 自身の規則

Phase 0.6 の結論どおり `loop` の第 1 要素がキーワードなら DSL、そうでなければ今までの
無限ループ。CL 自身が simple loop 形を持つので規則としても CL 準拠で、**既に書かれている
`loop` は 1 つも意味が変わらない**（prelude の `while`/`dotimes`/`dolist` はどれも
第 1 要素がコンスなので無関係）。

CL は節の語を裸のシンボルで書くが、ここでは全部キーワードにした。裸の `for` はただの
変数参照になるし、キーワードであることが単純ループとの分かれ目でもある。例外は変数と値を
区切る `=` で、位置が一意なので裸でも `:=` でも読む。

### 計画の前提が誤っていた

計画は「静的型が問題にならない: `collect` は `(let ((acc (Vector::new))) … (push acc e) …
acc)` へ展開すれば要素型が推論で決まる」と書いていた。**決まらない**:

```text
(let ((acc (Vector::new))) (progn (push acc 5) (len acc)))
=> type error: cannot infer type argument `t` for `vector::new`
```

`Vector::new` の型引数は**期待型から前向きに**来る。後続の `push` から後ろ向きには届かない
——双方向検査であって単一化ベースの推論ではない。

そこで `check_loop_dsl` は 2 パスになった:

1. `:with`/`:for` の節ごとに**代表式**を 1 つ検査して、その変数の型を学び `Env` を伸ばす
   （`:for x :in s` の代表式は `(get (copy-seq s) 0)`。検査するだけで評価はしない）。
2. その環境で集約式を検査して要素型を求め、`(the Vector<T> (Vector::new))` の `T` を
   書き込む。

これは「チェッカーは常に型を知っている。知った型は自分が組む木へ焼き込む」の一例
（[[typelisp-static-types-are-always-known]]）。本体を 2 回検査するのはここが初めてではない
——ジェネリック `defun` の本体は診断で 1 回、特殊化ごとにもう 1 回検査される。

**書き出す型は往復で検証する。** `mangle_type` は表層型構文をそのまま出すが、**すべての型に
表層構文があるわけではない**——関数型の括弧はリーダのジェネリックトークンの中を通れない
（`extend_angle_token`）。`the_form` は書いた文字列を `parse_type_here_at` で読み直して
同じ型に戻ることを確かめ、戻らなければその場でエラーにする（黙って別の形を出さない）。

### 構造

`loop_dsl.rs` は 2 つの仕事しかしない: `parse` が節列を `Plan` に読む（名前も型も解決しない）、
`build` が `Plan` を普通のソース（`let*`/`loop`/`if`/`setf`/`push`）に戻す。型を要する
部分だけ `Resolved` として外から渡る。`checker.rs` に置かなかったのは
[[typelisp-checker-only-one-boundary]] の線引きどおり——状態を持たない構築は別ファイルへ。

展開の形:

```lisp
(let* (<状態> <蓄積> ("loop first" true))
  (progn <initially...>
    (loop
      (if "loop first" (progn (setf "loop first" false) ()) (progn <steppers> ()))
      (if <尽きた> (progn <finally...> (return <結果>)) ())
      (let* (<毎回の束縛>) (progn <本体節...> ())))))
```

**ステップを本体の頭に置き、初回だけ飛ばす**のが要。CL の順序は「初期化 → 判定 → 本体 →
ステップ → 判定 → …」で、これはその同じ列を、脱出を 1 箇所にまとめた形。`:for v = e :then f`
の `f` が 2 回目以降の**先頭**で評価されるという CL の規則も、これで自然に出る（末尾で
ステップすると最後の本体の後にもう 1 回 `f` を評価してしまう）。

生成する名前はすべて空白を含む（`"loop first"`、`"loop buf 0"`…）ので、ソースには書けず
ユーザ変数と衝突しない——`mangled_method_name` と同じ手。入れ子のループは同じ名前を使うが、
内側の `let*` が外側を隠し、外側のステップは内側の `let*` の外にあるので正しく働く
（テストあり）。

### 途中で踏んだ 3 つ

1. **`(Vector::new)` は `Value::Path` であって symbol ではない。** リーダが読み時に `::` で
   割るので、`intern_symbol("Vector::new")` で作った頭は「no such function」になる。
   `forms::path_form` を使う。
2. **`(progn … ())` は文の並びには要るが、値を返す位置では邪魔。** `:initially` を
   ループの前に置くラッパーで `statements`（末尾に `()` を足す）を使ってしまい、構築全体が
   Unit になった。値を返す `progn` と文の `progn` を別のヘルパーに分けた。
3. **`:finally (return e)` と合成の `(return <結果>)` が二重になる。** ループが 2 つの違う型で
   脱出することになる。`:finally` の最後が `(return …)` なら合成側を出さない——CL の
   `finally (return …)` の慣用そのもので、集約しないループが自分の答えを名乗る唯一の方法。

### CL から外したもの

- 節の語はキーワード（上記）。
- `:maximize`/`:minimize` は `Option<T>`（CL が空列に nil を返すのと同じで、任意の `Ord` 型に
  最小元は無い）。`:thereis` は `Option<T>` を取り `Option<T>` を返す——CL の「最初の非 nil 値」に
  当たるのがこれで、`bool` を試すのは `:always`/`:never`。
- **`:return` だけ書いて集約も `:finally` も無いのはエラー**。CL は尽きたとき nil を返すが、
  ここにはそれが無いのでループが「尽きたときの値」を言う必要がある。読める文言で先に弾く。
- `:named`（Phase 4a の `block`/`return-from` 依存）、`:and`、`:being`、`:it`、`:nconc` は無い。
- 変数節は本体節より前（CL の規則）。後ろに書くと「そこから先だけ回る」と読めるのでエラー。

### 検証

`tests/loop_dsl_test.rs`（20 本）。展開結果は普通のソースなので JIT/AOT もそのまま通る
（`(compile …)` で確認）。

---

## CL 残差 Phase 4c — 評価とマクロ（2026-08-21、branch `feature/cl-parity`）

[cl-parity-plan.md](cl-parity-plan.md) の Stage 4c。**部分完了**——入ったものと、
入れなかったものの理由を両方書く（後者のほうが多い節なので）。

### `macroexpand-1` / `macroexpand`

**展開の 1 段はチェッカーが使うのと同じ**（`Checker::try_expand_toplevel_macro`）。
第 2 の展開器を書くと必ずずれるので、そこは共有した。インタプリタ側は
`Interp::expand_step` がチェッカーハンドル越しにそれを呼ぶだけ。

**`macroexpand-1` は `Option<Sexpr>` を返す。** CL は「展開したか」を第 2 返り値で伝えるが
多値が無い。`none` を「マクロ呼び出しではない」にすると、**CL の真偽値より情報が多い**
——自分自身の呼び出しへ展開するマクロと非マクロを、CL の真偽値では区別できないが
これなら区別できる。`macroexpand` は `none` になるまで繰り返して最終形を返す。

コンパイル済みコードからは `eval` と同じ 2 経路で環境へ届く（JIT は
`with_active_interp`、AOT は `rt_eval_init` が建てた `AOT_ENV`）。`rt_macroexpand_1` /
`rt_macroexpand` は `rt_eval` と同じ形の shim で、環境の探し方を `expand_shim` に括った。

### `gensym` を prelude へ動かした

CL の `*gensym-counter*` を**プログラムが読み書きできる変数**にするには、カウンタが
typelisp 側の大域変数である必要がある。組み込みのカウンタは `Heap` にあって誰も名指し
できなかったので、`gensym` ごと prelude の `defun` にした:

```lisp
(pub defvar (*gensym-counter* i32) 0)
(pub defun gensym (&optional (prefix string "g")) Symbol
  (let ((n *gensym-counter*))
    (progn (setf *gensym-counter* (+ n 1))
           (string->symbol (format false " ~a~a" prefix n)))))
```

**「解釈と compiled が 1 つの列を共有する」という不変条件は保たれる**——理由が変わった
だけで、以前は「1 つの `Heap` カウンタを共有していたから」、いまは「**1 つの定義と
1 つの大域**を共有しているから」。組み込みを消したので `Heap::gensym`/`gensym_counter`/
`rt_gensym`/registry の `FnSig`/interp の分岐/externs の 3 箇所も消えた。
`Heap::gensym` は `pub` なので消し忘れても警告は出ない——[[typelisp-pub-hides-dead-code]]
のとおり、手で確かめて消した。

名前の先頭が空白なのは以前と同じで、それがこの機構の保証の全部である。**CL のような
uninterned シンボルではない**: ここのシンボルは常に intern されるので、2 つの `gensym` が
別物なのは*名前*が違うからで、別オブジェクトだからではない。

### 入れなかったもの

- **`constantly`** — CL のそれは*引数を無視する関数*を返す。無視される引数の型は
  **戻り型にしか現れない**。このチェッカーは型パラメータを引数から決めるので
  `(the (fn (i32) string) (constantly "hi"))` でも `cannot infer type parameter` になる
  （明示的な型適用も無い——`(mk<string,i32> …)` は `no such function`）。実際に両方試した。
  `(lambda ((x T)) A v)` が同じ字数で同じことを言う。
- **`macrolet` / `symbol-macrolet`** — 障害は 1 つではっきりしている。式の位置の検査は
  `&self` だが、マクロの定義には (1) レジストリへの登録（`check_defmacro` は `&mut self`）と
  (2) インタプリタ側でのラムダの登録（`exec` 相当）の両方が要る。**やり方は決まっている**
  ——`check_defmacro` を `&self` の本体検査部と `&mut self` の登録部に割り、`Checker` に
  スコープ付きローカルマクロ表（`RefCell`）を足して `resolve_macro` がレジストリより先に
  引き、`MacroExpander` に「この defmacro コア形を定義せよ」を 1 メソッド足す——が、
  片手間には入らないので別立てにした。
- **`eval-when`** — **選ぶべき区別が無い**。`typl` は各トップレベル形を検査→実行と 1 本で
  進む。`compile-file` は定義形を全部 `exec` するうえに裸のトップレベル式を受け付けない
  （`aot.rs` の `other =>` 分岐がその場で拒否する）。CL の 3 つの situation はここでは常に
  一致していて、`eval-when` は恒真のラッパーにしかならない。
- **`define-compiler-macro` / `compiler-macro-function` / `load-time-value`** — 前二者は
  コンパイラマクロ層が無い（`compile` は明示的な操作で、島は名前で呼び出しを書き換えない）。
  後者は実行と別のロード相が無い。
- **`(Symbol::new name)` / `copy-symbol` / `gentemp`** — uninterned シンボル自体は
  `Heap` に 1 メソッド足せば作れる（`sym_names` に押して `sym_ids` に入れない）が、
  **買えるものが無い**。シンボルが束縛子として働く場所は全部*名前*で引かれる
  （`Env::vars` の鍵は `String`）ので、同名の uninterned シンボル 2 つは肝心なところで
  衝突する。データとしてなら intern 済みと `eq` で区別が付くだけで、その区別の用途が無い。
  `gentemp` は intern された新しい名前を作るもので、それは `gensym` そのもの。

### 検証

`tests/macro_tools_test.rs`（7 本）。`macroexpand` はチェッカーハンドルを要するので、
テストハンドラも `main.rs` と同じに `interp.set_checker` する（借用を `exec` を跨いで
持たない——再入した `macroexpand` が自分の借用を取る場所がそこ）。

## CL 残差 Phase 5b — `defmethod` の `&optional` / `&key` / `&rest`（2026-08-21、branch `feature/cl-parity`）

`defun` は 2026-07-29 から 3 区画を取れたが、`defmethod` は `&rest` すら取れなかった
（`parse_defmethod_sig_inner` が `parse_param_pairs` を呼ぶだけだった）。ここで揃えた。

### 実行時パラメータをヘッダの出口で確定させる

`parse_defun_params_full` を `defun` から切り離して `parse_params_full(elems, what)` にし、
`defmethod` は受け手の次のスライスをそれに渡す。要点は返り値の形で、`MethodSig::params` は
最初から**実行時パラメータ**を並べる: 必須 → `&optional` の実効型 → `&rest` の名前
（`Sexpr`）→ `&key` の実効型。区画を書かなければ従来どおり必須だけが並ぶので、
`parse_param_pairs` を呼んでいた頃と同じ列になる。

この形にしたので、本体の `Env`、生成する `TopLevel::Defmethod`、単型化のテンプレート
再解析、コンパイル経路のどれも区画の存在を知らない。知っているのは 2 箇所だけ:

- 登録される `AssocFn` の `FnSig`（`params` は**必須引数のみ**、区画は `optionals`/`keys`/
  `rest` に入る。`defun` の `FnSig` と同じ約束）
- 呼び出し側 `check_assoc_call` の分岐 `push_assoc_opt_key_args`

自由関数版（`check_call_opt_key`）より単純な点が 1 つある。メソッドのシグネチャは自前の型
パラメータを持たない（`AssocFn` の `FnSig::type_params` は常に空）ので、推論パスが要らない。
効いている代入は受け手または期待型の型引数から呼び出し側が既に決めた `subst` だけ。

制限は `defun` と同じ 1 つ: **デフォルト式を書いたパラメータの型に所有者の型パラメータを
書けない**。省略時に埋め込むのは検査済みのノードで、その `.ty` が抽象変数のままだと下流の
表現判定（`core_bridge` の `binding_kind`）が読む型が嘘になる。デフォルトの無い
パラメータは `Option<T>` として呼び出し側で新しく作るので無制限。

ついでに `defun` 側と揃えて、この制限の検査を**デフォルト式を検査する前**に移した。
順序が逆だと、空 `Env` で検査されるデフォルト式自身のエラー（`self::v` は当然 unbound）が
先に出て、本当の理由が見えない。

### トレイトのメソッドでは使えない

vtable スロットのアリティは固定である。`:dyn` 受け手の呼び出しはトレイトの宣言から引数を
埋め、具象受け手の呼び出しは `impl` の宣言から埋めるので、両者が食い違うと同じ呼び出しが
2 通りになる。`deftrait` 側に区画の構文が無い以上、食い違いを作れるのは `impl` 側だけ。
そこで 2 箇所塞いだ:

- `subst_method_item` — `impl` ブロックの中に書いた場合。放っておくと `&key` という裸の
  シンボルを `(name type)` として読もうとして `ImproperList` になり、理由が伝わらない
- `check_impl_conformance` — `impl` の外に書いたメソッドを後から `impl` が拾う場合。
  `FnSig::params` は必須引数しか持たないので、区画を見ずに比較すると
  「トレイトは 1 引数、実体は実行時 2 引数」が素通りしてしまう

前者だけでは足りないことに、テストを書いていて気づいた。後者はテストで実際に到達する。

### `lambda` / `labels` は入れない

省略された引数を埋めるには、呼び出し側が**呼ばれる側の検査済みデフォルト式**を読む必要が
ある。それはシグネチャに載っていて、名前で解決したからこそ手に入る。`lambda` は値として
渡され、その値を説明するのは `Type::Fn` だけ——パラメータ型・`&rest` の要素型・戻り型しか
無い。式を置く場所が無いうえ、置けば「同じシグネチャでデフォルトだけ違う 2 つのラムダ」が
別の型になってしまう。`labels` の関数も `Type::Fn` 型のローカル変数で、値として渡せるので
同じ。`&rest` が両方で使えるのは、それが型の話に閉じていて `Type::Fn` に枠があるから。

理由は `checker::OPT_KEY_NEEDS_A_NAME` に書き、エラーメッセージがそれを言う。`labels` には
そもそも検査が無く「parameter must be (name type)」になっていたので、`lambda` と同じ
明示的な拒否を足した。

### テスト

`tests/optional_key_test.rs` に 16 本追加（合計 38 本）。`defmethod` の
`&optional`/デフォルト無し/`&key`/`&rest`/静的関数/ジェネリック所有者、interp と compile の
一致、キーワード名の誤り・引数過多・所有者型パラメータへの依存の 3 つのエラー、
`lambda`/`labels` の拒否とその理由、`lambda` の `&rest` が残っていること、トレイト
メソッドの拒否 2 経路。

## CL 残差 Phase 5c — `deftype`（2026-08-21、branch `feature/cl-parity`）

計画の読みどおり、checker の型解決にエイリアス表を足すだけで済んだ。実行時表現には
何も影響しない。

### 保存するのは正規化済み・展開済みの本体

`Namespace::type_aliases` に `TypeAlias { name, params, body, public }` を入れる。要点は
**`body` を正規化済み・エイリアス展開済みで保存する**こと。これで:

- 使用位置（`Checker::canon`）の展開が不動点探索ではなく**1 回の代入**になる
- **エイリアスの循環が「検出するもの」ではなく「作れないもの」になる**。`B` の本体を
  保存する時点で、その中の `A` は既に `A` の本体になっているので、相互参照が成立しない

代償はこの言語が随所で採っている「テキスト上の先行順」の規則（`check_supertrait_impls`
と同じ）で、後から `A` を再定義しても `B` には届かない。

自己参照だけは別で、登録前なので展開できず、未解決の裸の名前——型変数と区別がつかない
——になってしまう。放っておくと使用位置が「何も棲めない型」に展開されるので、
`deftype` の側で明示的に拒否する。

### 下流は誰も別名の存在を知らない

書き換えが**型パーサの中**で起きるので、`mangle_type`・単型化のキー・ダンプ・コンパイル
経路、そして**エラーメッセージ**まで、すべて展開後を見る。これは意図した取引で、CL の
`deftype` も型指定子の略記であって別の型ではない（`typep` は展開後について答える）。
テストで明示的に確かめてある（`a_mismatch_reports_the_expansion_not_the_alias`）。

したがって 2 つのことは**しない**: 新しい型を作らない（取り違えを捕まえたいなら
`defstruct`）、述語にならない（CL の `(deftype small () '(integer 0 9))` は値の集合を
表し `typep` が実行時に判定するが、ここでは型は実行時の witness を持たない）。

### `canon` は失敗できない

`canon` は `Result` を返せない場所から呼ばれるので、型引数の個数違いをそこで報告できない。
黙って展開を見送ると「未知の型」として遠くで出るだけなので、`check_type_alias_arity` が
**書かれた**型（`canon` 前なので綴りがそのまま残っている）を歩いて注釈の位置で報告する。

### テストが 2 つの穴を出した

- `(use m::meters)` — `check_use` は関数・型・モジュールしか試していなかった。別名の
  分岐を足した。取り込むのは名前だけで、構成子も静的メソッドも無い（本体が名指すものは
  本体自身の名前を保つ）
- 既存の型と同名の `deftype` — `check_redef` 経由にしていたが、`RedefPolicy` が再定義を
  許すと**両方が登録されたまま使用位置ではエイリアスが黙って勝つ**。別の表に入る以上
  これは再定義ではないので、トレイトと同名の場合と同じく無条件に拒否する

### 成果物

`Namespace` に serde のフィールドが増えたので `dump::FORMAT_VERSION` を 2 → 3 に上げ、
島と prelude を再生成した（島 → prelude の順、島は不動点まで 2 回）。エディタ定義 2 つと
pprint のインデント表にも `deftype` を足した。

`tests/deftype_test.rs` 20 本。

## CL 残差 Phase 5a — `defstruct` のオプションリスト（2026-08-21、branch `feature/cl-parity`）

`(defstruct (Name option...) fields...)` を新設し、フィールドは `(name Type default)` を
取れるようにした。

### 生成物は AST ではなく「ソース」として合成する

既存のアクセサは lowered AST を直接組み立てている。コンストラクタで同じことをすると、
`&key`/`&optional` の埋め込み・可視性・ジェネリック所有者に対する `MethodTemplate::Form`
の保持（＝単型化できること）を全部書き直すことになる。そこで `defmethod` の**ソース**を
合成して `check_defmethod_in` に流した。Phase 5b で `defmethod` が 3 区画を取れるように
なっていたので、キーワードコンストラクタはそのまま乗る。

本体は必ず `(Name::new ...)`。`new` は構造上の唯一のコンストラクタのままで、生成するのは
その*呼び方*でしかない。

副作用として `Type` を注釈の綴りに戻す必要が出た。`mangle_type` は*直列化*であって
（`(fn ...)` の中がカンマ区切り、`dyn` が裸）読み返す用ではないので、`Fn` と `:dyn` だけ
リストを組み立てる `type_source` を書いた。原子型は `mangle_type` のままでよい。

### 合成した値を根付ける

`Vec<Value>` はコレクタから見えない。合成したソースは 1 つのリストとして組み立て
（`Items` の入れ子で、内側は外側の heap を再借用する）、`check_defmethod_in` に渡す前に
その 1 つを `push_root` する。`:include` で引き継ぐデフォルトも、ヒープに実体化するのは
使う瞬間まで遅らせた（`SlotDefault::Inherited` は `OwnedForm` を保つ）。

### `:include` は型の関係を作らない

親のスロット列を先頭に連結する。それだけ。子は親の部分型ではなく、親のメソッドは子に
適用されず、両者を結ぶ実行時テストも無い。策定時に「この Stage で最も重い」と書いたのは
型の継承を入れる前提だったからで、部分型を導入しないと決めた時点で連結だけが残った。

デフォルトを別ファイルの親からも引き継げるよう、スロットのデフォルトは
`Registry::struct_defaults`（型のパスをキーにした疎な副表）に記録する。`AdtDef` の
フィールドにしなかったのは `Docs` と同じ理由で、組み込み型が 30 箇所で `AdtDef` を
リテラルに組み立てており、そのどれもスロットのデフォルトを持たないから。

### 入れないもの

- **`:conc-name`** — CL ではアクセサに接頭辞を付けて 1 つの平坦な関数名前空間での衝突を
  避ける。ここではアクセサは受け手の型でディスパッチするメソッドなので衝突が起きず、
  接頭辞は `instance::field`（スロット名しか知らない）を壊す。解く問題が無い
- **`:predicate`** — 型は実行時の witness を持たないコンパイル時の分類で、「point かも
  しれない未知の型の値」が存在する位置も無い（`Sexpr` の `match` は封じてあり `:dyn` は
  ダウンキャストできない）。生成される述語は常に `true` しか返せない
- `:type` / `:initial-offset` / `:named` — 表現はコンパイラのもので言語からは観測できない

スロットのデフォルトを読むのは生成されたコンストラクタだけなので、`:constructor` が
1 つも無いのにデフォルトを書いたら死んだ設定になる。その場でエラーにした。

### 副産物: 静的関数の型引数が引数から決まるようになった

`check_assoc_call` の `subst` は受け手か期待型からしか来ていなかった。受け手のない**静的**
関数（`cell::of`）は、結果の型が既に分かっている場所でしか書けないということで、
ジェネリック構造体に生成コンストラクタを付けたテストが最初にそれで落ちた。`check_call` と
同じ 2 段——型が閉じている時だけ期待型を渡し、チェック後に `unify` で精緻化——に揃えた。
`&optional`/`&key`/`&rest` の経路も同じにしてある。既に受け手が全部決めている場合、
`unify` は確認しかしない。

### ダンプの走査表に無い表は黙って消える

`Registry` に表を足すのは半分でしかない。`dump.rs` の `walk`/`apply_entries` は表を
**カテゴリごとに名前で**列挙するので、片方しか知らない表があると「定義を黙って落とす
デルタ」ができる（`walk` の doc コメント自身がそう書いている）。Stage 5c で足した
`Namespace::type_aliases` も、ここで足した `Registry::struct_defaults` も入っていなかった。
`cat::TYPE_ALIAS` / `cat::STRUCT_DEFAULT` を新設し、`FORMAT_VERSION` を 3 → 4 に上げ、
島と prelude を再生成した。

見つけ方が問題で、`deftype` のテストも `defstruct` のオプションのテストも 1 プロセス内で
完結していたので全部 green のまま通っていた。`tests/dump_image_test.rs` は別プロセスが
イメージから起動するので、そこに 2 本足して初めて往復が確かめられる。

`tests/defstruct_options_test.rs` 26 本、`tests/dump_image_test.rs` に 2 本。

## CL 残差 Phase 6a — ハッシュ表（2026-08-22、branch `feature/cl-parity`）

### `Hash` は `Eq` のサブトレイト

CL は `sxhash` の契約を含意で述べる——`(equal x y)` ならば `(= (sxhash x) (sxhash y))`。
`Eq` をスーパトレイトに持つトレイトにすると、同じことがこの言語の言葉で言える:
ハッシュできる型とは値を比較できる型で、`sxhash` はその比較と一致していなければならない。
逆は成り立たない（衝突はありうる）。

結果は非負で 30bit に収める。CL は fixnum と言うだけで幅を約束しないので、狭いほうを
選べる——そして狭いほうを選ぶ理由が実際にあった（後述のグローバル切り詰め）。文字列の
ハッシュは typelisp で書いた **32bit** FNV-1a。64bit 版だと 2 文字目で `h * 1099511628211`
が i64 を溢れる（この言語の整数演算は検査付き）ので、各段で 32bit に丸める版なら積が
2^56 を超えず絶対に溢れない。

### 鍵のハッシュ可能性が静的になった

`HashTable` の `get`/`set`/`remove` が `(where (Hash K))` を持つ。表が保持できないキー型は
**型エラー**になる——以前は `Heap::lookup_hash_key` の実行時 panic で、そのコメントは
「チェッカーはハッシュ可能性の境界を表現できない（この言語にトレイトが無いので）」と
言っていた。トレイトは 2026-06-30 に入っており、これがそのコメントへの回答。

代償として、組み込みの `HashTable` が prelude のトレイトに依存する。`hashtable_test` の
prelude を読まないランナーがそれで落ちたので prelude を読むようにした。島は
`HashTable` の値を自分では持たず `rt_hashtable_*` の呼び出しを*出す*だけなので影響しない。

### ユーザ定義型を鍵にするのは別作業

mem 層の表は `MemHashKey`（`Int`/`Bool`/`Char`/`Str` の閉じた集合）で引く。ユーザ型を
入れるには衝突を構造的等価で解決するバケット層が要る。それを prelude 側に置こうとすると
**型が合わない**: 表の宣言型 `V` と、格納したい「`(K,V)` のバケット」は別物で、組み込み
メソッドのシグネチャに後者を書く手段が無い（`cons-cell` は prelude の `defstruct` なので
`registry.rs` からは名指せない）。したがってバケットは Rust 側に置くことになる。
`sxhash` と `(where (Hash K))` はその入口として先に入れてある。

`make-hash-table` の `:test` も入れていない。表が持たない意味論の選択で（`equal` 一択）、
関数値を受け取っても比較できない。受け取って無視するのは受け取らないより悪い。

### 副産物: コンパイル済みコードの整数切り詰め 2 件

`sxhash` のマスクを 2^62-1 にしたら `(sxhash -1)` が `-1` を返した。追うと 2 件出た。

**1 件目（直した）: 整数リテラル。** 島へは `Sexpr` として渡るので 3bit タグを引いた
61bit しか残らず、`4611686018427387903` が `-1` にコンパイルされていた（インタプリタは
正しい値を返す）。`float` が最初から採っている 32bit 2 分割にして、LLVM 側で `shl`/`or`
で組み直す——島自身の算術で組むと同じように溢れる。`(int N)` を作る場所は 4 箇所あった
ので `core_bridge::int_node` 1 つに集約した（文字列リテラルの各文字、bignum の桁、
quote された Sexpr、そして値としての整数リテラル）。

自己ホストなので移行に順序が要った。島の読み手と Rust のエミッタを同時に変えると、
1 世代目が「新しい読み手を持つが古いエミッタで壊れて生成された島」になる。**二重読みの
コードは書かずに**、中間状態を挟んで抜けた:

1. 島 SOURCE を新形式の読み手にし、エミッタは古いまま → 旧世代が正しく生成できる
2. エミッタを新形式に切り替える → 1 で作った島が正しく読む
3. もう一度回して不動点

**2 件目（未修正）: 幅の広い `i64` グローバル。** `(defvar (big i64) 4611686018427387903)`
を compiled な関数から読むと `-1` になる。リテラルと違って渡し方の問題ではなく、
コンパイル済みコードから見たグローバルの表現そのものがタグ付きの語を経由している。
[TODO.md](TODO.md) に再現手順つきで記録した。prelude の `*sxhash-mask*` を 2^30-1 に
したのはこれを避けるため。

### 回帰が出した 3 件

- `compile_test` の 2 本と `hashtable_test` のランナーが prelude を読んでいなかった。
  `(where (Hash K))` は prelude のトレイトを要求するので、読ませるようにした
- `enum_test` の「古い `(defstruct (Pair A B) ...)` ヘッダは拒否される」が、Phase 5a で
  その位置がオプションリストになったため別のエラーで落ちるようになっていた。オプション
  位置の裸のシンボルを特別扱いして、「型パラメータは名前側に `Name<A,B>` と書く」と
  言うようにした——「オプションはリストでなければならない」より役に立つ
- `island_self_compile_test` の `ISLAND_DEFUNS` に `int-receiver-type?` /
  `float-receiver-type?` が無かった。**Phase 1a/1b で島に足したときから漏れていた**もので
  今回の変更とは無関係。番人のリストなので直した
- `compile_test` に**島の IR を手書きしている**箇所が 2 つあり（`'(int 42)` /
  `(int 99)`）、2 分割になったので `(int 0 42)` / `(int 0 99)` に直した。落ち方が
  SIGABRT だったので原因が見えにくい——1 フィールドのノードから 2 つ目を読もうとして
  `sexpr-int` が nil に当たる。チェッカー側の core IR（`core_vocabulary_test` などが
  検証しているもの）は 1 フィールドのままで、変わったのは島に渡る側だけ

`tests/hash_trait_test.rs` 14 本、`compile_test` に 2 本。

---

## CL 残差 Phase 6b/6c — 多次元配列とビットベクタ（2026-08-22、branch `feature/cl-parity`）

計画は [cl-parity-plan.md](cl-parity-plan.md) の Stage 6b / 6c。CLHS 15 の 2 つの型を入れた。

### どちらも prelude の `defstruct`（Rust 側の追加はゼロ）

`Array<T>` は `Vector<i32>`（次元列）と `Vector<T>`（row-major の平坦な格納）と
`Option<i32>`（fill pointer）を持つ `defstruct`。`BitVector` は `Vector<i64>`（詰めた語）と
`i32`（長さ）。

Phase 1d の `complex` と同じ判断で、これは節約ではなく**この言語で書ける物を Rust で書かない**
という方針そのもの（[typelisp-rust-builtin-policy]）。結果として:

- 新しい `Repr` も `rt_*` シムも島の lowering も要らない。
- 書いた日に JIT/AOT を通る。`PRELUDE_COMPILE_UNSUPPORTED` が空であることを守る番人が
  すでにあるので、「prelude に書いた物はコンパイルできる」は自動で検査される。
- `defstruct` にできることが全部できる（`setf` の place、`Iter`、ジェネリック）。

`Vector<T>` を `RtValue::Struct` の流用で入れたときの原則
（[typelisp-vector-defstruct-revert]）の素直な延長でもある。

### `Array::new` ではなく `Array::make`

`defstruct` は必ずフィールド順のコンストラクタ `Name::new` を生成する。`Array<T>` の
フィールドは**表現**（次元列・平坦な格納・fill pointer）であって、呼び手が渡したいもの
（形と初期値）ではない。`new` を「本来のコンストラクタ」の位置に置いたまま、作る側の
入口を `Array::make` にした。`BitVector::make` も同じ。

計画表には `Array::new` と書いてあったが、これは `defstruct` が `new` を予約していることを
見落としていた。`:constructor` オプション（Phase 5a）でも `new` は消せない——あれが作るのは
`new` の*呼び方*であって、`new` そのものは構造上の唯一のコンストラクタのまま。

### `(aref a i j)` — チェッカーの糖衣

`defmethod` は**アリティ**で解決するので、「末尾に同じ型の引数が何個か続く」形は宣言できない。
そこで `Array<T>` の `get`/`set` は添字を `Vector<i32>` 1 本で取り（`row-major-index` /
`in-bounds` が欲しい形でもある）、CL の綴りはチェッカーが書き換える:

```lisp
(aref a i j)
;; =>
(let* ((%a a) (%idx (the Vector<i32> (Vector::new))))
  (progn (push %idx i) (push %idx j) (get %a %idx)))
```

**展開先を `row-major-get` ではなく `get` にしたのが要点**。計画は
`(row-major-get a (row-major-index a <添字の Vector>))` と書いていたが、それだと
`(setf (aref a i j) v)` が既存の呼び出し形 place 機構に乗らない——あの機構は
`(accessor recv key...)` の `accessor` に対して `set-{accessor}`（と `get` の特例の `set`）を
探すので、読み書きが同じ `get`/`set` の対になっている必要がある。`get` に展開すると
書き込み側は同じ展開の末尾を `set` に差し替えるだけで済み、両方が `Array<T>` 自身の
`get`/`set`——すなわち添字の範囲検査——を通る。

配列を先に `let*` で束縛するのは**評価順**のため。展開後の形では配列は最後（`get` の位置）に
読まれるので、束縛しないと `(aref (f) (g))` が `g` → `f` の順に評価されてしまう。添字は
1 回ずつしか現れないので、そちらに一時変数は要らない。

糖衣は名前に対して無条件（2 引数 `atan` と同じ）。ただし失敗したときだけ受け手をもう一度
検査して、`(aref v 0)`（`Vector<T>`）に「`vector<i32>` は `Array<T>` ではない」と言わせている。
書き換えた形の型エラーはユーザが書いていない型を名指すので、**同じ失敗についてまともな文を
出すためだけの再検査**——`aref` に第 2 の意味を与えているわけではない。

### 添字の範囲検査は省けない

`row-major-index` は畳み込む前に `in-bounds` を通す。省くと 2x3 の配列で `(aref a 0 5)` が
オフセット 5、つまり**別の行の実在するセル**を静かに読む。「間違った答えを返す」のと
「エラーになる」のは同じコストではない。

### `BitVector` の 1 語が 32bit である理由（既存バグの 3 件目）

最初 1 語 64bit で書いたところ、ビット 62 以上が立たなかった。切り分けの結果:

| 試したこと | 結果 |
|---|---|
| `(ash 1 62)` をインタプリタで | 正しい |
| `(ash 1 62)` をコンパイル済み関数で | 正しい |
| 幅の広い `i64` をコンパイル済み関数の引数に / 戻り値に | 正しい |
| **コンパイル済み関数で `Vector<i64>` に入れて読み戻す** | **壊れる** |

コンパイル済みコードはコンテナの要素をタグ付きの語（`typelisp-abi` の `encode`、下位 3bit が
タグ）で往復させるので、payload に入らない `i64` は往復で上位ビットを失う。prelude の
メソッドは全部コンパイル済みで走るから、`BitVector` はこれを正面から踏んだ。

**Phase 6a で見つけた整数切り詰めの 3 件目**で、しかも一番範囲が広い——グローバルだけでなく
`Vector`/`HashTable`/`defstruct` のフィールドでも、±2^60 の外にある `i64` は黙って壊れる。
再現手順は [TODO.md](TODO.md) に書いた。本筋の直し方（payload に入らない整数を `TAG_BOXED` の
箱へ逃がす）は島の lowering・`rt_*` シム・GC ルート・等価述語と印字・ダンプの版まで動く
1 フェーズ分の作業なので、Phase 6c の片手間には入れない。

1 語 32bit は「端に近寄らない」選択。60bit なら payload をぎりぎりまで使えるが、
タグが 1bit 増えたら黙って壊れる。密度は半分になるが、この型の正しさが表現の内部事情に
依存しなくなる。

### 長さの先のビットは常に 0

`lognot` は語の 64bit 全部を立てるので、長さの先にビットが残ると同じ長さの 2 本が食い違う。
`bitvector-trim` を、そこを立てうる操作（`bit-not` と全ての `bit-*` の対）の末尾に置いた。
テストは「補集合の往復が恒等」「補集合どうしの `bit-and` が全部 1」「語境界を跨いでも
1 ビットずつ独立に立つ」の 3 本で見ている。

### この 2 つで見つけた小さいこと

- `(if cond (setf x v) ())` は型エラーになる。`setf` は代入した**値**を返すので、腕の型が
  `bool`/`i32` と `Unit` で食い違う。`(setf ok (and ok cond))` と書くか `progn` で包む。
- `defmethod` は前方参照できない（`predeclare_program` の対象外）。`Array::make` が
  `Vector::filled` を呼ぶので、この節は prelude のそれより後に置く必要がある。

テストは `tests/array_test.rs`（37 本）、`tests/bit_vector_test.rs`（18 本）、
`tests/compile_test.rs` に 5 本。

### 全テストを直列で回して見つかった、前フェーズの取りこぼし 3 件

Phase 6b/6c の回帰を `scripts/test-serial.sh`（97 個のテストバイナリ全部）で回したところ、
6b/6c とは無関係な赤が 3 件出た。3 件とも**前のフェーズで入れた変更が、そのとき回さなかった
テストを壊していた**もので、`cargo check` は全部通っていた。

1. **`--lib` の `core_bridge::tests` 28 本** — Phase 6a が `int` ノードを 32bit 2 分割
   （`(int HI LO)`）にしたのに、ゴールデン文字列が `(int N)` のままだった。6a では
   `compile_test` 内の手書き島 IR は直したが、`--lib` を回していなかった。
   `(pat-lit N)` も同じ 2 分割になっているのを見落としていた。

2. **`place_test::setf_get_writes_through_a_hashtable_entry`** — prelude を読まないランナで
   `HashTable<string,i32>` を使っていた。6a で鍵に `(where (Hash K))` が付き、
   `Hash` の実装は全部 prelude の `impl` なので、素の `Checker` では型が付かない。
   prelude を読むランナに移した（Phase 4c で `macro_test`/`check_test` を移したのと同じ形）。

3. **`builtin_fn_value_test` 2 本 — これだけは本物のバグ**。
   `gensym` は Phase 4c でプレフィクス引数を得て Rust の組み込みから prelude の `defun` に
   移った。そのため (a) このテストが「自由な*組み込み*を関数値にする」経路をもう通って
   いなかった、(b) `(call0 gensym)` が**型検査を通ってから実行時にアリティ不一致で落ちた**。

   (b) が言語のバグ。`FnSig::params` は必須引数だけを持ち、`&optional`/`&key` は別のフィールドに
   ある——名前で呼ぶ呼び出し側は宣言からそれを埋められるからで、`Checker::fn_ref_node` は
   その `params` だけで `Type::Fn` を作っていた。しかし実行時の関数は**宣言した名前の数だけ
   引数を取る**。関数値には名前が無く、値を説明するのは `Type::Fn` だけなので、
   そこに全アリティが出ていないと辻褄が合わない。

   `fn_value_params`（必須 → `&optional` → `&key`、デフォルトの無いものは本体と同じ
   `Option<T>`）を新設して `fn_ref_node` をそれに合わせた。デフォルトは間接呼び出しでは
   埋まらない——埋める場所が `Type::Fn` に無いのは `OPT_KEY_NEEDS_A_NAME` が
   `lambda` について書いているのと同じ理由——ので、呼び手が自分で全部渡す。
   テストは `tests/optional_key_test.rs` に 4 本。

   テスト側は `gensym` を `string->symbol`（本物の自由な組み込み）に替えて、
   テストが名乗っている経路を実際に通るようにした。

**教訓**: 「関係しそうなテストを選んで回す」では足りない。`--lib` は 6a のときに回して
いれば 28 本が即座に赤で出ていた。フェーズの締めは直列の全実行にする。

---

## CL 残差 Phase 7 — エラーと動的束縛（2026-08-22、branch `feature/cl-parity`）

計画は [cl-parity-plan.md](cl-parity-plan.md) の Stage 7a / 7b。

### 7a — コンディション型を `Error` トレイトへ写像して、残った穴を埋める

コンディション**システム**は非採用のまま（language-design.md §9）。入れたのは
`SimpleError` / `WrappedError` / `wrap-error` / `describe-error` / `assert` / `warn` の 6 つで、
全部 prelude。Rust 側の追加はゼロ。

**`assert` はマクロにした**。失敗時に `assertion failed: (= 1 2)` と*書かれたまま*テストを
名指せるのは、展開時にフォームを `(quote ...)` で埋め込めるマクロだけだから。関数だと
「何が偽だったか」を言えない。CL の restart は提供する物が無いので、偽なら `panic` ——
CL も restart が全部断られれば同じところへ行く。

**`warn` は `*error-output*` へ書いて続行する**。この言語で「`Result` を返しもせず
プログラムを終わらせもせずに報告する」唯一の手段で、地図 §2.7 が「警告を出して続行する
仕組みが無い」と書いていた穴がこれで埋まった。

**テストで観測できない半分を正直に書いた**。`warn` の文面は `*error-output*` へ出るが、
`*error-output*` は `standard-stream` 型で、文字列ストリームは別の型なのでプロセス内に
差し替え先が無い。テストは「引数を評価する」「続行する」だけを見ていて、そのことを
テストファイルに書いてある。

### 7b — `dlet`、そして「入れなかった変数」の理由

**`dlet`** は保存 → 代入 → `unwind-protect` で復元。CL はこれを `let` と書くが、この言語の
`let` は常に字句束縛なので `(let ((*print-base* 16)) ...)` は「何も読まないローカル」を
静かに作ってしまう。名前は Emacs Lisp の同名・同義のマクロから。

cleanup が正常終了・`throw`・`panic`・`break`/`return` のどれでも走るので、単スレッドでは
CL の動的束縛と区別が付かない。テスト 16 本のうち 4 本がその「どう抜けたか」だけを見ている。
**スレッドごとの束縛ではない**点は違うので、そう書いた。

**印字制御変数**は `Limits` を `PrintVars` に改名して（3 つの限界だけを運ぶ名前ではなくなった）
`*print-base*` / `*print-radix*` / `*print-case*` / `*print-readably*` を足し、`Opts` に
`*print-lines*` を足した。`*print-case*` は CL と同じ綴りの `:upcase`/`:downcase`/
`:capitalize` を取る——**この言語のキーワードは自己評価する `symbol`** で、型ではないが値としては
CL と同じ書き方ができる（`Checker::check_symbol` の `:` 分岐）。「キーワード型が無い」という
`boole-*` 定数の注記に引きずられて最初は文字列にしようとしていた。

**副産物として radix リーダマクロ `#b`/`#o`/`#x`/`#NNr` を入れた**（本来 Stage 8b）。
`*print-radix*` の存在理由は「`*read-base*` が何であれ同じ数に読み戻せる印を付ける」ことなので、
リーダが `#x` を知らないままではドキュメントの記述が嘘になる。自分で書いた説明が実装と
食い違ったので、実装のほうを合わせた。

### 途中で壁に当たった 1 件: 標準ストリームを `:dyn` にできない

`*standard-output*` を `:dyn CharOutput` にすれば、CL がいちばんよく動的束縛を使う用途——
1 つのフォームの間だけ出力を文字列ストリームへ向ける——が書けるようになる。やってみたら
prelude の成果物ビルドが落ちた:

> prelude: a compiled body boxes or upcasts a trait object, whose vtable/trait ids are baked
> in per site — the artifact needs an ordered replay of those tables at load time

グローバルの初期化式はコンパイル済み本体で、`dyn-new`/`dyn-upcast` は箱詰め/アップキャスト
**地点ごと**に vtable id・trait id を焼き込む。JIT で載せるライブラリにはその番号を再生して
vtable アドレスを公開する起動列が無い、という既存の検査に正面から当たった（`prelude_bootstrap`
の末尾。「今日どの prelude 本体もここに届かない」と書いてあったが、届かせようとしたわけだ）。

同じ理由で CL の `*terminal-io*` / `*query-io*` / `*debug-io*` も作れない——どれも two-way
ストリームで、`two-way-stream` は両半分を `:dyn` へアップキャストして作る。ユーザコードは
`(make-two-way-stream ...)` を自由に作れるので、**prelude だけが持てない**。
型は元に戻し、理由を prelude のコメントと対応表の両方に書いた。

### 入れなかった変数（全部、理由つきで）

`*print-gensym*`（未 intern シンボルが無い）、`*read-default-float-format*`（浮動小数点型が
1 つ）、`*read-suppress*`/`*read-eval*`（`#.` が無く `#+`/`#-` はリーダ内部で完結）、
`*macroexpand-hook*`（展開はチェッカーの中で起きる。目的は Phase 4c の `macroexpand` が満たす）、
`*read-base*`（`read` はコンパイル済みコードからも `rt_read` 経由で呼ばれ、そちらに typelisp の
グローバルへの経路が無い。`PrintHooks` に相当するリーダ側の表が要る）。
`*print-array*`/`*print-escape*` は Phase 8a へ送った。

テストは `tests/error_catalog_test.rs`（16 本）、`tests/dynamic_binding_test.rs`（33 本）、
`tests/compile_test.rs` に 2 本。

---

## CL 残差 Phase 8a/8b — プリンタとリーダ（2026-08-23、branch `feature/cl-parity`）

計画は [cl-parity-plan.md](cl-parity-plan.md) の Phase 8。8c（リードテーブル）は
先行条件が別作業なので分けた。

### 1 引数プリンタは全部マクロ

`prin1`（`~s`）/ `princ`（`~a`）/ `write`（`*print-escape*` で選ぶ）と `-to-string` 三種。
Rust 側の追加はゼロ。

関数にできない理由がはっきりしている: **`format` の `&rest` は型変数を受け付けない**。

```lisp
(defun show<T> ((x T)) string (format false "~a" x))
;; => type error: &rest: element type Named("t") has no Sexpr encoding
```

prelude の `to-string` が「1 つのジェネリック `defun` ではなくスカラ型ごとの `defmethod`」
なのと同じ壁で、コメントにもそう書いてあった。マクロなら呼び出し地点で型が具体化しているので
通る。[[feedback-prefer-macro-over-checker-special-form]] の 3 例目。

CL の 1 引数 `print`（改行 → `prin1` → 空白）には**この言語での綴りが無い**。`print`/`println` は
制御文字列を取る `format` の短縮形として既に埋まっており、計画どおり併存させ改名しないため。

### `*print-escape*` を読むのは `write` だけ

CLHS どおり `~s`/`prin1`/`pprint` はこれを真に、`~a`/`princ` は偽に、それぞれ自分の呼び出しの
間だけ束縛する。つまり**誰も束縛していない状態で読まれるのは `write`/`write-to-string` だけ**で、
そこを実行時の `if` にすればプリンタ側は 1 行も変えなくてよい。`print-object` の実装は大域変数
ではなく自分の `escape` 引数を読む——そちらが directive の選んだ値を運ぶ。

### `~/name/`: CL の読みは静的型付けと両立しない

format 最後の未対応ディレクティブ。CL はグローバル関数を名指すが、**そう実装すると健全でない**:

- 制御文字列は実行時の `string` なので、どのディレクティブがどの引数に当たるかは検査時に決まらない
  （`~[`/`~{`/`~*` があるので静的な対応付けは一般に決定不能）。
- 実行時に名前で引ける定義が持っているのは `Repr` だけ（`FnDef` のコメント:
  「型は登録の時点で IR から消えている」）。`Repr::Struct` は全 `defstruct` を 1 つに潰すので、
  `point` 用のヘルパを `pathname` に対して呼べてしまう。

健全な道は 1 本しかなく、**値の型でディスパッチする**こと——`print-object` が既に使っている
仕組みで、そのメソッドはまさにその型に対して型検査済みなので構成上正しい。よって
`~/name/` は「引数自身の型のメソッド」を引く。形は `((self Self) (colon bool) (at bool)) → string`。

即値（`string`/`bool`/`char`/`symbol`/リスト）は型パスを持たないので明示的に対応付けた。
対応はどれも厳密（あらゆる `Value::Str` は `string`）。唯一厳密でないのが整数で、`i32`/`i64` は
生の語を共有し値から区別できない——**推測せず両方を候補にし、両方が同名メソッドを定義している
ときだけエラー**にした。片方だけなら曖昧さは無い。

`print-object` と違い、メソッドが無いのは**エラー**。`~a` には組み込みの表示という戻り先が
あるが、`~/name/` は名指しで特定のものを要求している。

**AOT では使えない**。メソッドを実行時の名前で引く以上、どのメソッドに到達しうるかを
コンパイル時に言えず、対応するには全型の全メソッドを起動時に登録することになる
（`print-object` は 1 トレイトの impl だけ）。黙って別の動作をするのではなく、その旨を
述べるエラーにした。

### バイト I/O はストリーム層が最初から空けてあった穴

`InputStream`/`OutputStream` の `Item` が開いたままなのは「バイトストリームが並行するトレイト
階層なしに `Item` を後で固定できるように」——2026-08-02 の設計コメントにそう書いてある。
そのとおり `ByteInput`/`ByteOutput` が `i64` に固定し、`binary-file-stream` が実装する。

CL は `:element-type '(unsigned-byte 8)` で要素型を**呼び出し**の性質にするが、ここでは
ストリームの**型**の性質なので、違うのは開く関数の側になる（`open-binary` 系）。
文字ストリームへの `read-byte` は型エラーで、native 層でも拒否する——次の文字の UTF-8
エンコーディングを返すのは「そこに無いファイルを発明する」こと。`unread-char` が保留中の
ときも拒否する（文字の押し戻しとバイト位置は、ストリーム位置が何を意味するかについて食い違う）。

### 見送り: `print-object` はジェネリック型に効かない

`*print-array*` と `Array<T>` の `print-object` を入れようとして、**既存の穴**に当たった:

```lisp
(defstruct gen<T> (v T))
(impl print-object gen<T> (print-object ((self Self) (escape bool)) string "GEN"))
(println "~a" (gen::new 1))                     ; => #<gen 1>
(println "~a" (print-object (gen::new 1) true)) ; => GEN
```

型検査を通り、名前で呼べば動き、**プリンタからだけ見えない**。登録漏れではない: プリンタは
値が持つ型キーで引くが、単型化が型引数を消しているのでキーは `gen` であって `gen<i64>` では
なく、値の側に実体化の情報が無い。全 T で 1 本の本体を共有する手も、`gen<T>` を印字するとは
`T` を印字することなので成立しない。`Array` だけ組み込みプリンタに型名で特別扱いさせるのは
[[typelisp-type-identity-invariant]] が避けている形なので採らなかった。
`tests/printer_test.rs::print_object_does_not_reach_a_generic_type` が現状を固定しており、
直ったらそのテストが落ちる。

### 8b: 読み終わり位置と、終端文字がトークンを終わらせていなかったバグ

`read-from-string` は CL では 2 値（datum と位置）を返す。多値が無いので `cons-cell<Sexpr, i64>`
1 つで返す——`Cursor.pos` は元から**文字**単位で、CL が返す位置そのものだった。位置があると
文字列を 1 データずつ読むのが再スキャンでなくループになる。

`read-preserving-whitespace` との差は**空白 1 文字**だけ。既存の `read-sexpr` は元々
「datum とその前の空白だけ消費し、後ろに手を付けない」＝ CL の用語では
`read-preserving-whitespace` の契約だった。区別を実装せず名前だけ増やすと CL 準拠は名目に
なるので、`read-sexpr` を CL の `read` に合わせ（終端の空白 1 文字を消費）、従来の挙動を
`read-sexpr-preserving-whitespace` として出した。**既存の挙動変更**なので
`stream_test` の 1 本を 3 本に分けた（CL 挙動 / preserving / 空白以外の終端は触らない）。

**本物のバグを 1 件修正**: 終端文字がトークンを終わらせていなかった。
`(read-delimited-list #\] s)` を `1]x` に対して使うと `1]x` が 1 つのアトムになる。CL は
終端文字をリードテーブルの *terminating macro character* にすることでこれを解決するが、
リードテーブルが無いのでスキャナに直接教える必要がある。`reader-scan-atom` /
`reader-scan-hash` / `reader-scan-datum` に呼び出し側の追加区切りを通し、**深さ 0 でだけ**
効くようにした（`(1 2]` の `]` はリスト自身のテキストの一部で、壊れたリストとして `read` が
報告するのが正しい）。

### 締めで出た 2 件

- **extern を足すと島の成果物が変わる**。`stream-read-byte`/`stream-write-byte`/`read-datum-at`
  を足したので `island_artifacts_test` が落ちた。成果物の入力は SOURCE・Rust 側ビルダ・extern の
  表の 3 つあるのにハッシュは SOURCE しか見ていない、という既知の穴
  （[[typelisp-island-hash-reads-forms]]）。`regen-compiler-island.sh` で解決。
- `rt_extern_functions` の戻り値は配列長を型に書いているので、extern を足すたびにそこも直す。

### テスト書きで確かめた言語の挙動

- **`equal` は構造に降りない**（`equalp` が降りる）。`(equal (option::some 2) (option::some 2))`
  は偽。CL 準拠（CL の `equal` が降りるのは cons・文字列・ビットベクタ・パス名だけ）。
- 組み込みエラー型は 1 変種の sum 型なので構築は **`(ReadError::ReadError "msg")`**。
  prelude はこれまで `match` で分解するだけで、構築の綴りがどこにも書かれていなかった。
- `(option::none)` は文脈から型引数を推論できないことがある（`(the Option<X> (option::none))`）。

テストは `tests/printer_test.rs`（31 本）、`tests/byte_io_test.rs`（13 本）、
`tests/reader_extras_test.rs`（22 本）。

---

## トップレベル前方参照の廃止 → `defsignature`（2026-08-24、branch `feature/cl-parity`、`af27bae`）

Phase 8c（リードテーブルとリーダマクロ）に着手しようとして、先行条件が塞がっていると分かった。

`Checker::predeclare_program`（2026-08-01、[two-pass-toplevel-plan.md](two-pass-toplevel-plan.md)
Phase 1）は「最初のフォームを**検査する**前に**全フォームを読む**」ことを要求する。
リーダマクロは「フォーム *k* を**実行してから** *k+1* を読む」ことを要求する。両者は正面から
衝突する。方針は**トップレベル `defun` の前方参照を廃止**と決まり、置き換えとして明示の
前方宣言を入れた。

```lisp
(defsignature <名前> (<引数型>...) <戻り型>)
(pub defsignature <名前> (<引数型>...) <戻り型>)
```

引数名は書かない——本体が無いので名付ける対象が無い。CL の対応物は
`(declaim (ftype (function (i64) bool) even2?))` だが、あちらは宣言システム一式を伴い、かつ
**助言**でしかない。こちらは静的型付けなので宣言は検査される。別物として独自の名前を持たせた。

### 決めた規則

- **宣言と定義は一致しなければならない。** 食い違いは定義地点でエラー（引数の個数・各引数の型・
  戻り型・`&rest`・型パラメータ・`&optional`/`&key`・`pub` をすべて比べる）。
  `predeclare_program` は黙って上書きしていた。宣言を信用できるものにするのがこの検査の目的。
- **宣言したまま定義しないのはエラー。** ユニットのロード完了時に `Checker::finish_unit` が
  未消化の宣言を全部挙げて報告する。REPL は 1 入力ごとには報告しない（宣言と定義を別の行に
  打てるべき）。
- **定義より後ろに置いた宣言もエラー。** 当初は再定義として扱うつもりだったが、`RedefPolicy` の
  既定が警告なので通ってしまい、その宣言が未消化のまま残って `finish_unit` が
  「定義がありません」という**明らかに嘘の**メッセージを出した。「何もできない宣言だ」と
  直接言うほうが正確。
- **ジェネリック関数は宣言できない。** 実体化は**保持したソースフォーム**から行う
  （`request_fn_specialization` がテンプレート表を先に引く）。本体の無い宣言では実体化しようが
  ない。今まで動いていたのは `predeclare_defun` が `defun` フォーム丸ごとをテンプレートとして
  保持していたから。宣言地点で理由を述べて拒否する。`&optional`/`&key` も同じ理由で拒否
  （既定値は検査済みの式で、署名だけ登録すると呼び出し側が不完全なものを見る）。
- 対象は `defun` のみ。`defmethod`／型／`defmacro` は今も前方参照不可で、変わらない。

### 実測: 前方参照に実際に依存していたもの

呼び出しグラフを作って（コメント・文字列を除去）調べた。

| 層 | トップレベル `defun` | 前方参照される名前 |
|---|---|---|
| 島 `src/compiler.rs` の `SOURCE` | 123 | **65** |
| prelude | 218 | 1 |
| `examples/` `projects/` `tests/**/*.typl`（22 ファイル） | — | **0** |

島の 65 は、コンパイラ中核の相互再帰リング（`compile-value` / `compile-assoc` / `compile-if` …）と、
そのリングから後ろ向きに呼ばれる `compile-int` / `compile-var` / `emit-rt-call` などの合計。
**この 65 は全部単型で、`where` も `&optional`/`&key` も無い**——上の唯一の制限に 1 つも当たらない。
prelude の 1 件（`read-delimited-list` → `sexpr-list-from`）は定義順の入れ替えで消した。

**測り間違いを 1 件やった。** 最初の強連結成分の分析で「循環の中から外へ」出る辺を数え落とし、
54 と報告した。島の再生成が `no such function: compile-int` で落ちて発覚。方針・設計・作業量は
変わらないが、数字は 65 が正しい。

### 消したもの

`predeclare_program` / `predeclare_form` / `predeclare_defun` と、その呼び出し 8 箇所
（`project.rs` の 2 ローダ、`compile/aot.rs`、`compile/dump.rs`、`compile/bootstrap.rs`、
`bin/bench_prelude.rs` ×2、`prelude.rs`）。`predeclared: HashSet<String>` と
`claim_predeclared`、dump へのシリアライズはそのまま `defsignature` が引き継いだ。

実行時は `"defsignature" => Ok(None)`——`"use"` と同じ no-op トップレベル。全部の効果は
チェッカーで済んでいる。

### 塞がった穴が 1 つ

「`where` 付き `defun` は前方参照できない」（`predeclare_program` が `deftrait` 登録前に走るため
ヘッダごと黙って捨てられていた）は、宣言を `deftrait` の後ろに置けるので消えた。

### 締めで出た 1 件

**prelude のダンプは SOURCE 文字列のハッシュ**なので、コメント 1 文字の修正で無効になる。
島のハッシュは「読んだ形」に対して取る（[[typelisp-island-hash-reads-forms]]）ので
コメントでは無効化されない——この非対称を取り違えて、prelude を再生成した**あとで**
`SOURCE` 内のコメントを直し、全直列回帰の 80 ターゲット・1,371 件の panic が全部
`prelude: the dump is stale` になった。再生成のやり直しだけで解決。

### この後

これで検査はソース順の 1 パスになり、Phase 8c の先行条件が外れた。残る 8c 本体は
「1 フォームずつ**読む**」へのドライバ改修と、リーダ側フック表（`PrintHooks` 相当）の新設。
後者は Phase 7b で `*read-base*` を見送った理由そのもので、作れば `*read-base*` /
`readtable-case` / `#.` も併せて入る。

テストは `tests/defsignature_test.rs`（26 本、`tests/forward_reference_test.rs` を書き換え）。

---

## `match` で比較できるものを増やす — 値パターンと裸 variant 名（2026-08-25、branch `feature/cl-parity`）

### 出発点にあった誤り

「シンボルには compiled 表現が無い」——これは**間違い**だった。`typelisp-abi` の
タグ表に `TAG_SYMBOL = 0b010` があり、compiled なシンボルは `(SymId << 3) | 2`、
すなわちシンボルテーブルへの索引を持つ即値である。リテラルは
`compile-construct-sym`（`src/compiler.rs`）が名前文字列を `rt_intern_symbol` に渡して
**実行時に intern** して作る——SymId を焼き込まないので AOT でも正しい。intern 済みなので
同一性比較がそのまま内容比較になり、パターンの比較は `icmp eq` 1 命令で済む。

実際に欠けていたのは 2 つ:

1. `check_pattern` が `Value::Symbol` を無条件に束縛パターンにし、`Value::Str` /
   `Value::Boxed`（f64 / bignum / ratio）/ `(quote sym)` をどれも受けなかった。
2. `match` のスクルーティニーが `expect_adt` で ADT に限定されていた。`string` を
   `match` できないので、そもそも文字列リテラルを**書ける場所が無かった**。

### 入れたもの

| パターン | 下がる先 | 比較 |
|---|---|---|
| 整数 / `true`/`false` / 文字 | `pat-lit`（従来どおり） | 語の比較 |
| 文字列 / f64 / `'sym` / bignum / ratio | **`pat-guard`（新設）** | その型の `Eq::equals` |
| `(= expr)`（新設） | 同上 | 同上 |
| 裸の variant 名 | `pat-ctor`（引数 0 個） | 変種タグ |

`(pat-guard SYM TEST)` は「`SYM` に検査対象を束縛し、`TEST`（`bool` 式）が真ならマッチ」
という 1 つのノード。`TEST` はチェッカーが**ソースとして**組み立てた
`(equals $match-scrut EXPR)` を通常の `check_at` に通したもので、これが要点:

- インスタンスメソッド解決・トレイト境界・単型化が**全部ただで付いてくる**。比較規則は
  その型自身の `Eq` 実装になり、ユーザ定義型は作者が書いたとおりに比べられる。
- `Eq` を持たない型は「マッチしない腕が黙って残る」ではなく**型エラー**になる。
- compile 側から見ると中身はただの `assoc` 呼び出しなので、`collect_targets` が依存として
  拾い、島は `compile-value` でそのまま翻訳できる。島に足したのは
  「値をスロットに入れて名前を束縛し、テストをコンパイルして分岐する」20 行だけ。

`$match-scrut` は全ガードで**共有の 1 名**。各ガードは自分の `TEST` を走らせる直前に
束縛する（インタプリタは都度 env を伸ばし、compiled 側は新しい alloca に入れて名前を張り替える）
ので、1 つのパターンに 2 つのガードがあっても互いの値を読まない。テストで固定してある。

### 裸の variant 名は既存の穴だった

```lisp
(defenum color (red) (blue))
(defun f ((c color)) i32 (match c (red 1) (blue 2)))   ; 修正前は常に 1
```

裸名は束縛パターン＝catchall なので、第 1 腕が常に勝ち、**しかも網羅性検査を満たしてしまう**
ので「第 2 腕に到達しない」と言う者が誰もいなかった。スクルーティニー自身の型が同名の変種を
持つときだけ変種として解決するようにした（`check_bare_ctor_pattern`）。持たない名前は
従来どおり束縛なので、`(match v (x ...))` は全部そのままの意味。フィールドを持つ変種を裸名で
書いたらアリティエラー（"expected 1 field(s), got 0"）で、これはこの場所で言えるどの文言より
読みやすい。

### スカラのスクルーティニーが暴いた 1 件

`compile-match` はスクルーティニーを**無条件に** `push-sexpr-root` していた。
`rt_push_sexpr_root` は渡された語を `decode` するので、生の `i64` の `7` は
`TAG_BOXED` の箱 id として解釈される——ADT しかスクルーティニーになれない間は
「冗長」で済んでいたものが、`(match n (1 ...) (_ ...))` を許した瞬間にバグになる。
`match` ノードの末尾フィールド（島が読んでいなかった分類番号）を
`Repr::binding_kind` に置き換え、`2` のときだけ root するようにした。

### 触った場所

- `check/resolved.rs` — `Pattern::Guard { name, form }`
- `check/checker.rs` — `check_pattern` に `interp`/`env` を通す（`&Heap` → `&mut Heap`）、
  `value_pattern` / `check_bare_ctor_pattern` / `eq_method_of` を新設、
  `check_match` の `expect_adt` を `Option` 化＋変種を持たない型は catchall 必須
- `eval/interp/core_eval.rs` — `match_core_pattern` に `&Interp`/`env` を通し `pat-guard` を追加
- `compile/core_bridge.rs` — `pat-guard` の翻訳、`match_kind_of` を廃して `binding_kind` へ
- `compile/core_freevars.rs` — ガードの `TEST` を自由変数の走査対象に（`(= limit)` のような
  外側参照を取りこぼすとクロージャの捕獲スロットが 1 つ足りなくなる）
- `check/locate.rs` — `pat-guard` は LSP から見て opaque（合成ノードで span が全部同じ、
  受け手の名前は誰も書いていない）
- `src/compiler.rs`（島）— `compile-pattern-test` の `pat-guard` 分岐、`compile-match` の
  条件付き root。**島の再生成は 2 回**（emit が変わったので不動点に達するまで）
- `docs/syntax.md` §4 match、`tests/match_value_test.rs`（16 本、全部 interp と compiled の両方）、
  `tests/core_vocabulary_test.rs`（`pat-guard` を語彙に追加）、`tests/check_test.rs`
  （「スカラは match できない」テストを「catchall が要る」テストへ）

### 続き: `impl Eq sexpr`（同日、`eq` 相当で採用）

上の「残した限界」の 1 件目——`Sexpr` に `Eq` が無い——はその場で入れた。**`eq` 相当**、つまり
CL の同一性。`equal` の再帰比較にしなかったのは、`sexpr` が万物の直和なので「2 つを比べる」に
唯一の正解が無く、`eq`/`eql`/`equal`/`equalp` が既に 4 つとも `sexpr` に生えている以上、
`equals` を `equal` の 3 つ目の名前にしても意味が増えないから。比較のコストが定数で済むのも大きい。

帰結が非対称なので、**checker で片側だけ塞いだ**:

| `Sexpr` スクルーティニーに対するリテラル | `eq` だと | 扱い |
|---|---|---|
| `'foo` / 整数 / 文字 / `true`/`false` | 内容どおり一致（intern 済み・即値） | **そのまま書ける** |
| 文字列 / f64 / bignum / ratio | `Str`・箱の同一性 | **型エラー**にして `(str "hi")` を名指す |

塞いだ理由はこの機能自身の原則（「マッチしない腕が黙って残るより、比較できないと言う」）で、
実際に `(match s ("hi" 1) (_ 0))` は `impl` を入れた直後に「型は通るが 0 を返す」状態だった。
`(= expr)` には掛けていない——`equals` を明示的に求めて書いた式だから。

副産物として `check_pattern` の `char`/`bool` の腕を整数と同じ形にした（型が一致すれば即値
パターン、そうでなければ値テスト）。これが無いと `Sexpr` に対する `#\a` / `true` が
「pattern does not match」で落ちる。

prelude の `SOURCE` を触ったのでダンプの再生成が要る（`scripts/regen-prelude-bitcode.sh`）。

### 残した限界

- **`bool` スクルーティニーは `(true ...) (false ...)` で網羅にならない**。変種を持たない型は
  一律 catchall 必須という規則を優先した。
- `(= expr)` の中は LSP の hover/goto が効かない（上記 opaque の帰結）。

## 2026-08-26 — 島に `icase` を入れ、コンパイラのディスパッチを書き直す

`cond`/`case` は prelude に既にある（`case` は CL の `eql` ではなく `equal` を使う——
`docs/cl-equivalence-catalog.md` の eq/eql/equal/equalp 節に記録済みの意図的な逸脱）。
足りなかったのは**島の側**で、島は prelude に依存しない（[[typelisp-island-no-prelude-dependency]]
の原則: `src/compiler.rs` の `SOURCE` に prelude 名を書くと 40 件超のテストが落ちる）。
`icond` と同じ理由で `icase` を島自前のマクロとして書いた。

```lisp
(defun icase-key-test ((key Sexpr)) Sexpr ...)   ; 単一キー or キー列 → bool 式
(defun icase-build ((key-form Sexpr) (clauses Sexpr)) Sexpr ...)
(defmacro icase (key &rest clauses) (icase-build key clauses))
```

展開形は `(let ((icase-key KEY)) <if の木>)`。キーは 1 つでも `(k1 k2 k3)` のリストでもよく、
リストは `(if (equal icase-key k1) true (if (equal icase-key k2) ...))` の or 連鎖になる。
`else` 節が最後の枝。**キーを 1 度だけ評価する**のが `icond` との差で、
`compile-value` の 41 分岐や `compile-assoc` の 131 メソッド節のように
同じ `(sexpr-sym-name (sexpr-car e))` を毎回引き直していた箇所がそのまま短くなる。

島のマクロ展開器の 3 制約は `icond` と同じ（[[typelisp-island-icond]]）:
クロージャ禁止・`,@` 禁止（prelude の `sexpr-append` が要る）・**自己再帰禁止**。
`icond` は 1 節ずつ剥がす再帰形だと展開器がスタックを溢れさせるので、`icase-build` も
`icond-build` と同様に**節を逆順に畳んで `if` の木を一気に作る**反復ループで書いた。
`ISLAND_MACROS`（`tests/island_self_compile_test.rs`）にも `"icase"` を足すこと。

### 書き直した箇所

| 箇所 | 前 | 後 |
|---|---|---|
| `compile-value` | 41 段の `if (equal tag ...)` 連鎖 | `icase` 1 つ |
| `compile-assoc` | 131 メソッド節 | 受け手型ごとの `icase`（外側は `icond` のまま） |
| `*-native-method?` / `*-receiver-type?` 12 個 | `icond` の述語列 | キー列 1 本ずつ |
| `compile-sexpr-field` | タグごとの連鎖 | `((5 6 8 9) v)` のような複数キー節 |
| `compile-sexpr-tag-test` | 1 関数に混在 | `compile-tag-bits-test` / `compile-box-kind-test` に分割 |
| `compile-pattern-test` | 連鎖 | `compile-ctor-pattern` を切り出し + `icase scrut-kind` |
| `compile-construct` / `compile-ctor-subpatterns` | `(eq variant 100)` 等の連鎖 | `icase` |

`int-equality-method?` は唯一の呼び出し側がキー列に化けたので削除した。

**残した `icond` は 2 つだけ**: `icond` マクロ定義自身と、`compile-assoc` の外側の受け手型
ディスパッチ。後者は節が `(if (equal type-name "sexpr") (sexpr-native-method? method) false)`
という**混合述語**（型名の一致 *かつ* メソッド名の所属判定）なので、単一キーの `icase` には
落ちない。2 way / 3 way の `(if (equal method "new") ...)` のような早期分岐も、
後続の束縛を使わずに抜けるものはそのまま残した（`icase` にすると読みにくくなるだけ）。

### 落とし穴

- **島は定義順に検査する**。`compile-sexpr-tag-test` から呼ぶ新ヘルパを後ろに置いたら
  `no such function: compile-box-kind-test`。前に移すか `defsignature` を書く
  （`compile-ctor-pattern` は後者）。
- **`setf` は代入値を返す**。`(if first (setf acc one) (setf acc (list ...)))` は
  両腕の型が食い違って `progn` の型エラーになる。`(setf acc (if first one ...))` に直す。
- SOURCE を触ったので**島の再生成が要る**。emit が変わった回は 2 回
  （[[typelisp-island-regen-fixpoint]]）、コメントだけなら 1 回。

## 2026-08-26（続き） — コアマクロ層の新設と `case` のキーのリテラル化

上の `icase` の節に書いた「島は prelude に依存しないので自前のマクロが要る」は、
**確かめたら成り立っていなかった**。島の SOURCE に prelude の `cond`/`case` を使う
`defun` を 2 本足してブートストラップを回したら、そのまま通った。

理由は `src/compile/bootstrap.rs:81` で、**島の SOURCE を読む前に prelude を
interpreted でロードしている**。島は既に `Option<llvm-basic-block>` など prelude 由来の
型に依存していて、「prelude に依存しない」の実体は「**emit するコードに prelude 関数を
残さない**」だった。マクロは展開時に消えるので、この柵の内側ではない。`icond` のコメントが
挙げていた 3 制約のうち、`,@` 禁止（`sexpr-append` が prelude の `defun`）と
クロージャ禁止（「島が未インストールで JIT が無い」）は現状には当てはまらない。

### コアマクロ層（`crates/typelisp-front/src/core_macros.rs`）

とはいえ「島が prelude を引く」のは筋が悪いので、**制御マクロを prelude から切り出して
独立した層にした**。定義は 1 つ、利用者は prelude と島の 2 つ。

入れたもの: `sexpr-append`（`,@` の展開先）、`and`/`or`/`when`/`unless`/`cond`/
`case`/`ecase`/`ccase`。層の規律は「**builtin と特殊形しか名指さない**」の 1 つだけ
（この層は prelude より前にロードされるので）。

ロード順は `prelude::load_interpreted_with` の先頭。**ダンプにも入る**——
`core_macros::load_with` が prelude のダンプ生成コールバックを受け取る。入れないと
ダンプ起動で `cond`/`case` が消える（実際に一度そうなった）。

**入力が 2 つになったのでハッシュも 2 つ見る**: `dump::sources_digest` /
`verify_sources_digest` を新設し、prelude ダンプの新鮮さは
`prelude::DUMPED_SOURCES`（コア層 + prelude）で判定する。片方しか見ないと、
コア層を編集しても古いダンプが「新鮮」と呼ばれる——島の成果物で一度やった
「入力は複数、ハッシュは 1 つ」の失敗と同じ形。

島側は `icond`/`icase` とその補助 3 defun を削除して `cond`/`case` に統一
（`icond-build`/`icase-key-test`/`icase-build`)。**削除範囲の罠**: `icond` の
コメントブロックと `icond-build` の間に 66 本の `defsignature`（前方宣言）が挟まっていて、
コメント先頭から `defmacro icase` までを一括削除したらそれごと消え、
`no such function: emit-direct-call` になった。HEAD と現在で
`^\((defsignature|defun|defmacro|defvar) NAME` を突き合わせ、消えたのが 5 件ちょうどで
順序も保たれていることを確認してから再生成した。

### `case` のキーはリテラルになった

CL と同じく**キーを評価しない**。これで CL のキー列 `((1 2 3) "low")` が入る。

| キーの書き方 | 意味 |
|---|---|
| `1` / `"one"` / `#\a` / `true` / `1.5` | そのままのリテラル |
| 裸のシンボル `a` | シンボル `a`（生成する比較は `(equal tmp (quote a))` で**従来と同一**） |
| `(k1 k2 ...)` | キー列。どれかに当たればマッチ |
| `'a`（＝`(quote a)`） | **エラー**。裸の `a` を書くよう名指す |

`'a` は CL では黙って 2 要素のキー列 `{quote, a}` になる有名な罠で、この言語では
`'a` が**変更前の正規の書き方**だったぶん踏みやすい。`match` の値パターンと同じ理由
（型は通るが決してマッチしない腕を黙って残さない）でエラーにした。移行が要ったのは
`tests/prelude_test.rs` の 1 箇所だけ。

`gensym` は使えない（prelude の `defun` で、`*gensym-counter*` と `format` を要求する）ので、
スクルーティニーの一時変数は**先頭が空白の固定名 `" case-key"`**。空白始まりはソースに
書けないので衝突しない——`gensym` 自身が使っているのと同じ保証で、カウンタが要らない。
固定名で足りるのは、束縛が唯一の読み手である `cond` を直接囲むからで、
`case` の腕に入れ子になった `case` は正しく shadow する。

## 2026-08-26（続き 2） — マクロのコンパイル

方針: **マクロは A という S 式から B という S 式を生成する関数であり、その変換処理自体は
コンパイルできる。コンパイル中にマクロに出会ったら展開してから展開後の S 式をコンパイルする。**

照合したところ、後半は満たしていた（むしろ厳密で、展開はコンパイルより前の**チェック時**に
完了し、コンパイラが見る時点でマクロは 1 つも残っていない）。**前半は満たしていなかった**——
`defmacro` は codegen 対象外で、展開器は常に interpreted で走っていた。

### 型はすでに揃っていた

`Checker::check_defmacro` は「全パラメータ `Sexpr`・結果 `Sexpr`」としてマクロ本体を
**完全に型検査済み**だった。記録していなかっただけ。`exec` の `defmacro` 腕が `sig: None`
で登録し、`compiled_fn_body` は `sig` 無しを拒否するので `(compile <マクロ>)` は
`has no signature (is it a defmacro?)` になっていた。

変更は 4 点:

1. `exec` の `defmacro` 腕で `sig: Some((vec![Repr::Sexpr; params.len()], Repr::Sexpr))`。
   `&rest` も 1 エントリ——`&rest` な `defun` の署名と同じ形で、`bind_macro_args` が
   rest を 1 つの `Sexpr` に集約してから渡すので長さが合う。
2. `expand_macro` が `apply`（常に木を歩く）でなく `enter`（compiled があればそちら）を呼ぶ。
   引数の束縛（`&optional`/`&key` のデフォルト評価）は展開前に済むので本体だけが境界を渡る。
3. `collect_item` の `"defmacro" => {}` を `CompiledItem::Fn` の収集に。prelude の 40 マクロが
   全部コンパイルされた（生成器の「コンパイルできない定義が現れたら止まる」照合は無言）。
4. **`(compile <マクロ名>)` は別途修正が要った**。インタプリタはマクロ本体を `defun` と同じ
   `fns` テーブルに置くが、**チェッカーは関数とマクロを別のマップで持つ**ので `resolve_fn`
   だけでは見つからない。`check_compile` に「関数を先に、無ければマクロ」のフォールバックを追加。

### 実測: 速くならなかった（記録）

release ビルド、各 7 回の最小値:

| | 起動のみ | マクロ 8,000 回 | `loop` 2,000 回 |
|---|---|---|---|
| 展開器 interpreted | 1.06s | 1.21s | 1.67s |
| 展開器 compiled | 1.21s | 1.38s | 1.85s |

**展開の増分は変わらない**（0.15s→0.17s、0.61s→0.64s）。一方**起動が 0.15s 遅い**——
prelude ダンプが 2,829,011 → 3,139,096 バイト（+310KB）になり、40 個のマクロ本体の
ビットコードを起動時にインストールする分。

見立て（プロファイルは未取得）: prelude のマクロ本体はほとんど数行で、
**1 回の展開あたりの境界コスト（`encode_crossing_args`・GC ルートの push/pop・戻り値の decode）が
小さな本体の木歩きを上回っている**。`loop` のような大きな展開器でも改善が見えないのは、
展開の総コストのうち展開器の実行が占める割合が小さく、**生成された S 式の型検査が支配的**
だからだと思われる。

**それでも入れた**（ユーザ判断）。方針「マクロはコンパイルされているもの」に実装を揃えるほうを
取り、境界コストが下がれば自動的に利益に転じる。速度を根拠に入れたのではない、というのが
ここに数字を残す理由。

## 2026-08-29 — 型 identity とシンボルを文字列で比べるのをやめる

ユーザ指摘：「型をテストするのに、文字列で比較するのはしてはいけない。型（型構造体あるいは
列挙体）そのものを比較しないとダメ」。続けて「文字列を比較してるところをすべて洗い出して、
それが本当に文字列でしか比較できないものか確認して、可能な限り文字列での比較をなくして」。

洗い出した結果は 4 分類。A（型 identity）と B（シンボル）を本作業で潰した。

### A. 型 identity を `String` からインターン済み ID へ

`BoxedObj::Struct`/`Enum` は型名を `String` で**インスタンスごとに**持ち、「同じ型か」は
その文字列比較だった（`equalp`、`rt_sexpr_instance_test`、`match` の struct/enum 腕）。

- `typelisp-mem` に `TypeKeyId` とヒープ内インターン表（`type_keys` / `type_key_ids`、恒久）を
  新設。`alloc_struct`/`alloc_enum` は **`TypeKeyId` しか受け取らない** ので、identity が
  comparable な ID として存在しないまま値を作ることができない
- 表示・診断用に `struct_type_name`/`enum_type_name` は残す（表を引くだけ）。比較は
  `struct_type_key`/`enum_type_key`
- `BUILTIN_TYPE_KEYS` を `Heap::with_capacity` が生成時にインターンするので、`option`/`result`/
  `vector`/`cons-cell`/`hashtable`/`scope`/`scope-frame`/各エラー型の ID は**コンパイル時定数**
  （`TypeKeyId::OPTION` 等）。これで 3 クレートに散っていた `RESULT_TYPE_KEY: &str = "result"`
  等 11 個の重複定数が消えた
- `type_key.rs` の入口は据え置き（`alloc_typed_struct`/`alloc_typed_enum`/`heap_type_is`/
  `heap_type_path`）。`type_key_of` は `Cow<str>` を返すようにし、単一セグメント（＝ほとんど）で
  join の確保をしない

**ABI は変えていない。** 島は今も型名を文字列リテラルで渡す。受け取る側
（`rt_struct_new`/`rt_data_new`/`rt_sexpr_instance_test`）が `type_key_arg` でインターンし、
**先に lookup してから mint する**ので定常状態では確保が起きない。島の再生成は不要だった。

**残した文字列**（理由つき）:

- `typelisp-print` の AOT 側 `PRINT_OBJECT` / `ENUM_NAMES` は型名キーのまま。登録
  （`rt_print_object_method`）が **`rt_heap_init` より前**に走る（`build_main_wrapper` が
  「no heap involved, so this can run before `rt_heap_init`」と明記）ので、登録時にインターン
  する相手がいない。印字 1 回ごとの `String` 確保だけは borrow を短くして消した
- `type_key.rs::heap_type_is` は `Path` → 名前 → ID の lookup を 1 回する。呼び出し側が持って
  いるのは名前（`Path`）なので、名前→ID の変換はこの境界に本質的に要る。比較そのものは整数

### B. シンボルを名前文字列で比べていた 16 箇所

`heap.symbol_name(id) == "&rest"` の形。`intern_symbol` が小文字畳み込みをするので**今も正しい**
が、`"&REST"` と書いた新しい箇所は黙って一致しなくなる——綴りを間違えられる比較だった。

`BUILTIN_SYMBOLS`（`quote`/`unquote`/`unquote-splicing`/`the`/`fn`/`pub`/`where`/`return`/
`&rest`/`&optional`/`&key`/`:dyn`/`=`/`:=`）を型キーと同じく生成時にインターンし、`SymId::REST`
等の定数で比較する。`is_symbol(heap, v, name)` は `is_symbol(v, sym)` になった。

**残した 1 箇所**: `loop_dsl.rs` の `symbol_name(id).starts_with(':')`。「キーワードシンボルか」は
開いた集合の問い（前方一致）であって、ID 比較にはならない。

**インターン順を変えて安全な根拠**（確認済み）:

- 成果物のハッシュは名前を辿る（`bootstrap.rs` の `hash_form` が「Interned ids are *not*
  hashed」と明記、テスト `the_hash_follows_names_not_intern_ids` つき）
- ダンプの直列化は `OwnedForm::Sym(String)` 経由で名前
- コンパイル済みコードは `SymId` を焼き込まず、実行時に `rt_intern_symbol` を呼ぶ

### 番人

`tests/type_identity_guard_test.rs` に 2 本追加：`BUILTIN_TYPE_KEYS` / `BUILTIN_SYMBOLS` の
各エントリが、実ヒープでその表の**添字どおりに**インターンされること。既存の「rt が綴る
キーは `type_key_of` の綴りか」は、11 個の `&str` 定数を比べる形から、表の中身を比べる形に
書き換えた。**名前だけ比べても添字がずれていれば全部隣の型になる**ので、ID 側の検査を
別に立ててある。

### D（実測して見送り）

IR の op 名ディスパッチ（`core_eval` の `Op::from_name`、40 タグ）。`BUILTIN_SYMBOLS` を入れた
ことで「id はヒープ固有だから表にできない」という当初の反対理由は消えたので、**実際に作って
測った**。

- 壁時計、同一ビルドで交互に測定、インタプリタ実行の `fib 25` ＋ 20 万回 `dotimes`：
  文字列 match / `SymId` 表とも **6 回中最良 5.33 秒**。差は測定誤差の中
- 表は安定した id で引く必要があるので、40 個のタグを `typelisp-mem` の `BUILTIN_SYMBOLS` に
  並べることになる——**メモリ層がフロントエンドの IR 語彙を持つ**。しかもタグを片方だけに
  足すと「未知の演算子」で落ちる新しい沈黙の失敗経路ができる

同日 `SymId` 比較にした 16 箇所とは性質が違う。あちらは同じ綴りが十数箇所に散っていて
「間違えて綴れる比較」だったが、こちらは中央の表 1 つで、間違える第 2 の場所が無い。
**見送り**——としたが、同日の後続作業（下記「シンボルテーブルを大域・恒久・生ポインタに」）で
**撤回して入れた**。ユーザの方針「シンボルの比較はポインタ値の比較すればいいだけだし、それ以外
の方法でしてはいけない」が壁時計の測定より上位にあるため。関数は `Op::from_sym(tag: SymRef)`
になっている。反対理由に挙げた「メモリ層がフロントエンドの IR 語彙を持つ」も、`BUILTIN_SYMBOLS`
を入れた時点で既に越えていた線で、見送りの根拠としては弱かった。

他の名前ディスパッチ（builtin メソッド名、`LlvmOp.type_key`）は、そもそも整数 op-id で
引いた後の静的テーブルのタグ比較なので、ヒープ上の値の identity を文字列で比べてはいない。

### 未着手（C）

- 文字列そのものの比較（`heap.string(i) == heap.string(j)`、CLI 引数、REPL の `:quit`、
  型構文レキサの `NameTok::Ident`）。文字列でしか比較できない

## 2026-08-29（続き） — シンボルテーブルを大域・恒久・生ポインタにし、モジュールの木に沿わせる

ユーザ指摘：「シンボルの比較はポインタ値の比較すればいいだけだし、それ以外の方法でしてはいけない」
「シンボルテーブルの設計がおかしそうだ」。挙がった 3 つの期待を照合した結果：

| 期待 | 直前の実態 |
|---|---|
| 大域に確保される（`intern_symbol` の戻り値が常に同じ） | **違う。`Heap` のフィールド**。ヒープごとに空から始まる |
| GC で解放されない | 合っていた（`gc()` が sweep するのは cons／`str_slots`／`box_slots` のみ） |
| 名前空間ごとに作られる | **違う。**ヒープごとに平坦な表 1 つ |

動機はコンパイラ側のメモリ管理。`SymId(u32)` は**ヒープへの索引**なので、compiled code は
シンボルを触るたび `ACTIVE_HEAP` を経由していた（`rt_sym_name`／`rt_intern_symbol` とも
`active_heap()` を呼ぶ）。生ポインタなら経由しない。ユーザから **unsafe 可・シンボルに miri は
不要（単純な仕組みにする）・ただしテストで動作を保証すること**の許可。

### 1. `Symbol` ヘッダと `SymRef`

```rust
#[repr(C, align(8))]                  // 下位 3 ビットをタグに空ける
pub struct Symbol { name: Box<str>, home: NsId, well_known: u32 }

#[derive(Clone, Copy)]
pub struct SymRef(*const Symbol);     // 同一性 = アドレス。`==` が CL の `eq`
```

`Value::Cons` が既に生ポインタをタグ語に載せていた（`(c.addr() as i64) | TAG_CONS`）ので、
`TAG_SYMBOL` も同じ形にしただけ。`ConsRef` が手本。

ヘッダは `Box::leak` する。**恒久なので `'static` が主張ではなく本物になり、アドレスが
ダングリングしようがない**——これが「miri で見張らなくていいくらい単純」の中身。
`unsafe impl Send/Sync for SymRef` はここから正当化される（不変・不滅の共有参照）。

**逆引き表 `sym_names: Vec<String>` は廃止した。** ポインタなら名前は 1 回の deref で返るので、
`symbol_name` を O(1) にするための表という理由が消える（この指摘はユーザから。以前の
「逆引きに要る」という説明は循環していた）。

### 2. なぜ `well_known: u32` が要るか

**ポインタは `const` にできず、`match` のパターンに書けない。** 同日午前に `SymId::PUB` 等で
書き直した分岐（IR タグ 39 way、定義形 15 way、loop 43 way）が `if` の連鎖に退化してしまう。
`Symbol` に固定語彙 151 個の添字を持たせ、`match sym.well_known() { wk::LET => ... }` と書く。
密な小整数なのでジャンプテーブルのまま。

**同一性の判定は常にポインタ。** `well_known` は固定語彙を引くためだけの副次情報で、語彙外の
シンボルは全部 `NOT_WELL_KNOWN` を返す（＝identity テストには使えない）。`wk::X` は
`const fn sym_index` が `BUILTIN_SYMBOLS` を探して作るので、**表に無い名前の定数はコンパイルが
通らない**。

### 3. 表はモジュールの木

ユーザ指示「モジュールごとに、子要素のテーブルを持たせるのが直感的だと思う」。checker 側の
`Namespace` と同型：

```rust
struct NsNode { syms: HashMap<Box<str>, SymRef>, children: HashMap<Box<str>, NsId>,
                parent: Option<NsId>, segment: Box<str> }
static SYMBOLS: OnceLock<Mutex<SymTable>>;   // ノードは Vec<NsNode>、NsId は添字
```

- **読みはロック不要**（ポインタを deref するだけ）。書き＝intern だけがロックを取る
- `cargo test` は複数スレッドで走るが、シンボルは不変・恒久なのでプロセス共有で安全
  （`ACTIVE_HEAP` が thread-local なのは*ヒープが可変でテストごとに別だから*で、シンボルには
  当てはまらない）

**intern の規則は current → 親 → … → root、無ければ current に作る。** これが無いと
`(module m (defun ...))` が壊れる：`defun` を認識しているのは root の `defun` との同一性なので、
モジュール内の `defun` が無条件に `m::defun` になると**モジュール内の全定義が定義でなくなる**。
親を辿る形は、この言語が既に持つ名前解決（current namespace first then root）と同じ。

順序依存（先に使ったモジュールがその名前を取る）に実害は無い：prelude・コアマクロ層・島は
ユーザコードより先に root へ入る（`load_for_aot` → `load_compiler` → `read_all_in_spanned`）。

### 4. リーダに基底名前空間を通す

`(module path body...)` は**入れ子**なので、読みながら push すれば字句的に決まる——CL の
`*package*` のような動的状態は要らない。`read_datum`／`read_list`／`read_atom` に `ns: NsId` を
足し、`read_list` が 2 番目の要素（パス）を読んだ時点で本体用の `NsId` に切り替える
（`module_body_ns`：頭が `wk::MODULE` かの identity テスト、相対降下、先頭が空セグメントなら
ROOT からやり直し）。公開の入口は `read_all_in_spanned_within(heap, file, src, ns)`；
既存の `read_all_in_spanned` は ROOT を渡す薄いラッパ。`project.rs::load_source_inner` は
手元の `segs` から `ns_of(segs)` を作って渡す。

**書かれたパス `m::foo` のセグメントは root の素の名前のまま**にした。`m::foo` はカレント
名前空間からの相対でもありうる（`n` の中なら `n::m::foo` かもしれない）ので、**リーダには
決められない**——解決はチェッカの仕事。裸のシンボルの住所は字句的に決まるが、修飾パスのそれは
解決を要する、という非対称。

`string->symbol` と `gensym` は実行時に作られて住所が字句的に決まらないので **root に入れる**。
動的な「現在のパッケージ」がこの言語に無いため。

### テスト（miri の代わり）

`tests/symbol_table_test.rs`、13 本。大域（**別々の `Heap` をまたいで同じポインタ**／大文字小文字
の畳み込み／複数スレッドから同名を同時に intern しても 1 つに収束——`Value` は `!Send` なので
`.addr()` を送る）、恒久（ヒープを落としてもシンボルの名前が読める／GC が回収しない）、タグ語の
不変条件（`addr() & 0b111 == 0`、`encode`/`decode` の往復）、名前空間（`m` と `n` で同名が別
シンボル／`(module m (defun ...))` の `defun` が root のそれと同一＝§3 の親辿りの番人／入れ子の
モジュールが鎖を全部辿る／`ns_path`）、端から端まで（モジュール本体が自分の表に裸の名前を読む）。

`tests/mem_test.rs` の `symbol_count` を数える 2 本は、大域化＋並列テストで不安定になるので
**同一性の主張に書き換えた**（元々測りたかったのはそちら）。実際、書き換える前に
「left: 1, right: 2」で落ちた——同じバイナリ内の別テストが先に "foo" を intern していたため。

### 島の成果物は再生成が要った（見立てが外れた）

計画では「島は素通しなので再生成不要」と見ていた。**島が出す IR については当たっていた**
（`compile-sexpr-field` はシンボルのペイロードを素通しする）。外れたのは別の理由で、
`SymId` → `SymRef` の改名が島の `SOURCE` 文字列**内のコメント 2 行**に当たり、ダンプの
`verify_digest` が**ソース文字列**に対して取られているために無効化された（読んだ形に対して
取る `island_source_hash` とは別物——「島のハッシュは読んだ形に対して取る」の裏面で、
ダンプ側は今もソース文字列を見ている）。

表現が実際に変わった以上コメントは直すべきなので、アドレスを説明する文言に書き直してから
再生成した。md5 `83ac1046…` → `b09f1282…`、不動点は 1 パスで到達（2 回目を回して確認）。
prelude のダンプも追随させた。

### 事故

- **BSD の `sed` は `\b` を解さない。** `SymId` → `SymRef` の一括改名が静かに何もせず、301 箇所が
  残っていた。`grep -rl` ＋ Python の `re.sub(r'\bSymId\b', ...)` でやり直した
- **正規表現の巻き添え。** `match tag {` → `match tag.well_known() {` が `core_bridge.rs` と
  `core_freevars.rs` の `tag: &str` にも当たった。`core_freevars` のほうは*本物の*シンボル比較
  だったので正しく変換し（`core::op` → `core::op_sym`、腕を `wk::*` へ）、`core_bridge` の
  `record` 以外は 1 行ずつ文字列の腕に戻した

## 2026-08-29（続き 2） — シンボルテーブルをモジュールごとに作り直す

ユーザ指示：「モジュールごとにシンボルテーブルを持つようにして。今あるシンボルテーブル関係の
ソースをすべて削除して、改めて1から作りなおして。属するモジュールが違えば、同じ名前のシンボルも
別のシンボルとして扱う。別のモジュールのシンボルを同じ名前で使いたい場合は、元モジュールの
シンボルへの参照（ポインタ）を持つことにより等値であることを比較するように」。

同日午前に入れた「current → 親 → … → root と辿ってから作る」規則の撤回にあたる。

### 名前が紛らわしかった

先に一度、説明を誤解された。`SymTable::new()` が「どのプロセスでも語彙 151 個を作る」と書いたら、
「`SymTable` はいくつも作成されるので、そのすべてに intern するのは間違っている」と指摘された。
実際は `SymTable` は**木全体**で、`static SYMBOLS: OnceLock<Mutex<SymTable>>` としてプロセスに 1 つ、
名前空間ごとの単位は `NsNode` のほう。**型名が単位を誤らせた**ので、doc comment を書き直した。

### 作り直した形

| | |
|---|---|
| `NsId::SYSTEM`（0） | 固定語彙 `BUILTIN_SYMBOLS` を 1 度だけ作って所有する。それ以外は何も持たない |
| `NsId::ROOT`（1） | ただ最初に作られただけの普通のモジュール。prelude・コアマクロ層・島が読まれる |
| `lookup(ns, name)` | **`ns` の表だけを引く。木は辿らない** |
| ノード生成時 | SYSTEM の 151 個を**ポインタで写す**（`push_node`） |
| `import(into, sym)` | 他モジュールのシンボルを自分の表に置く。所有者（`home`）は移らない |

「参照だけ持つ」は複製を配ることではない。配ると同期の問題が 2 つ（配り漏れ／後から増えた語）
生まれるが、**実体は 1 箇所でポインタが複数の名前から指される**形にはどちらも無い。

`(module m (defun ...))` が壊れないのは、親辿りではなく **SYSTEM からの取り込み**が効いているから。
番人 `a_definition_inside_a_module_is_still_a_definition` の理由書きをそう直した。

`symbol_count()` は `syms.len()` の総和ではなくなった（取り込みを見えるモジュールの数だけ数えて
しまう）。`SymTable::created` を数える。

### そして出たバグ — コンパイル済みと解釈実行でシンボルが食い違う

```
both interp:                 true
tag compiled, same? interp:  false      ← 同じ 'a-module-local-tag のはず
```

`core_bridge` は引用シンボルを**名前だけ**書き下していた。名前は「実行時にどの表を引くか」を
答えないので、`rt_intern_symbol` が root に入れて別のシンボルになる。

**この穴は同日午前の `eb8800b`（リーダに名前空間を通した時点）から開いていた。** 作り直しが
原因ではなく、常時露出させたので見つかった。

直しは「シンボルの何が向こう側で名前になるか」を書き下すこと。2 通りあり、どちらもコンパイル時に
確定する（`core_bridge::sym_form`）:

| 所有者 | 書き下すもの | 実行時 |
|---|---|---|
| SYSTEM の語彙 | `BUILTIN_SYMBOLS` の**添字**（`construct ... 102 INDEX`） | `rt_wk_symbol(i)` — 配列を 1 回引く |
| それ以外 | 名前 ＋ 所有モジュールのセグメント（`construct ... 100 name seg…`） | `rt_intern_symbol(name, seg…)` |

添字が使えるのは、どのプロセスも同じ並びで語彙を作るから。**アドレスは別プロセスへ越えられないが
添字は越えられる**——`well_known` を「`match` のためだけ」に入れたつもりだったが、2 つ目の使い道が
あった。文字列を組み立てず、表も引かない。

`well_known_symbol(i)` は `WELL_KNOWN: OnceLock<Box<[SymRef]>>` を読む。**`get_or_init` は使えない**：
初期化子が本体の表を作り、その構築が同じセルを `set` するので、初期化中の `OnceLock` に再入して
デッドロックする。`get().is_none()` で見てから `table()` を触る。

### 島の再生成は 2 段階

1 回目が `compile-construct: field type is not representable in compiled code yet` で落ちた。
**まだ知らないノードを吐かれた**もので、自己ホストの順序問題。手順は

1. 島の SOURCE に 102 の腕と可変長 `compile-construct-sym` を入れ、`sym_form` は**旧い形を吐いたまま**
   再生成する（＝コミット済みの島が新しい島を作れる）
2. `sym_form` を本来の形に戻して再生成（＝新しい島が受け取れる）
3. 不動点まで（3 回目・4 回目は md5 同一）

`externs.rs` に `rt_wk_symbol` を足したので、いずれにせよ島の成果物は変わる（extra の表は成果物の
3 つ目の入力）。prelude も追随。

### 落ちたテスト 1 本

`core_vocabulary_test::every_shared_tag_is_one_the_island_dispatches_on`。島の `case` キーが
文字列リテラルから裸のシンボルになり、文字列を探す走査が空集合を返していた。
**`accepted.len() > 30` が捕まえた**——この番人は今回で 3 つ目の形を見ており、毎回この 1 行が
効いている。走査を「`case` フォームを括弧の深さで歩き、各節の先頭トークンを取る」に書き直した。

## 2026-08-30 — 島のブートストラップが単型化の束を見ていなかった

`sexpr-*` の抽出子に非空 `Sexpr` を要求させる案（下記「A は不要」）を調べる過程で、
島に `unwrap` を 1 箇所書いたら島全体がビルドできなくなった。

```
get-function: no function named "tl_compile-char" in this module
```

**呼び先が無かったのではなく、呼び先を出力し忘れていた。** ジェネリックを呼ぶ
トップレベルは、チェッカが特殊化と一緒に `<monomorph specializations>` という
`module` に包んで返す（`checker.rs:1880` の doc comment が「a form's specializations
can never be separated from the form that needs them」と理由を書いている）。

島のブートストラップの `defun_name` は裸の `defun` しか見ないので、束を見た瞬間に
`None` を返し、**中の `defun` が呼び出し側もろとも全部落ちていた**。落ちた関数は
前方宣言もされないので、失敗は**別の関数の呼び出し地点**に現れる。

### 直し

`aot::collect_aot_item` が同じ問題をとっくに解いていた——`module` に再帰し、
`defun`/`defmethod` をリンク名つきで拾う。その形に揃えた（`collect_island_items`）。
`fn_names: Vec<String>` は `Vec<CompiledItem>` になった。特殊化された**メソッド**は
`user_method_symbol_name` でしか正しい記号名にならないので、これが要る。

未知のトップレベル形は黙って捨てず `Err` にした。今回の不具合がまさに
「黙って捨てて別の場所で落ちる」形だったため。

**島の成果物は 1 バイトも変わらない**（md5 同一）。出す IR には影響しない変更で、
それを不変性で確かめられるのが良い性質。

### 数値

これ以前、島はジェネリック関数を 1 つも呼んでいなかった。裏付け:
`unwrap` を 1 箇所入れた状態で `defun 125 + defsignature 68 + module 1 = 194`、
SOURCE の実形は `defun 126 + defsignature 68 = 194`。`module` 1 個がちょうど
`compile-char` 1 個を飲み込んでいる。`Option<Sexpr>`/`Scope<llvm-value>` は
ジェネリック**型**の具体化であって、呼び先は常に単型だった。

番人は `bootstrap.rs` の unit test 2 本（束から両方拾えること、未知形が拒否されること）。

### A は不要（調査の結論）

`sexpr-int`/`sexpr-str` ほか 6 個の抽出子の引数を `Option<Sexpr>` から `Sexpr` に
する案は**やらない**。理由は 3 つ。

1. **`null-elimination-plan.md` §2.3 が既に決めている。** 機械的な `unwrap` 置換は
   「実行時挙動は今日と同一で、書き換えコストだけ払って防げるバグは 0 件」。
   `unwrap` に意味があるのは「明示的で grep 可能な主張」である場合に限る、と
2. **77 箇所すべてが同じ機械的な形。** 引数は `sexpr-car` 57 / `sexpr-cdr` 7 …と、
   直接渡すものしかない。判断の入る「主張」が 1 つも無い
3. **診断が悪くなる。** 今日は空リストでも `sexpr-int: expected an Int Sexpr node` と
   関数名と期待変種が出る（`Value::Empty` は `Some(_)` 腕）。実装ログ 8403 行に
   このメッセージで実際にデバッグした記録がある。`unwrap` の汎用パニックはそれを消す

### フロー依存の絞り込み（C）も不要（同日、続けて調査）

「`(if (sexpr-symp x) ...)` の then 節で `x` を `Sexpr` に狭める」案も**やらない**。
A と同じ構造の誤りだった。

**`Sexpr`（空でない）は 11 変種をまとめた 1 つの型**で、島が落ちる 2 経路はどちらも
変種の問題なので、そこへ狭めても何も言えない:

- `sexpr-car` は**空リストではもう落ちない**（`interp.rs:1750`、`Some(v) if v.is_empty()`
  → `Ok(Value::Empty)`。null 排除作業の成果）。残る失敗は「アトムの car」＝ *cons か*
- 抽出子の失敗は「Int か」

しかも変種で分ける道具は最初からある。**`match` の `Sexpr` フェンスは 2026-07-10 に
再解禁されていた**（`symbol-sexpr-redesign.md` 冒頭の追記）。実測で解釈実行・compiled
とも動く。島が使っていないのは Phase 2 で*逆方向*に動かしたから——`match` ベースの
typed field reader を Rust 組み込みへ移した。

そして `match` に書き換えても失敗は消えない。`i64` を返す関数が Int でないノードを
受け取りうる以上、失敗腕が要る:

```lisp
(match (sexpr-car e) ((Int n) n) (_ (panic "expected an Int")))
```

これは今日の `sexpr-int` が中でやっていることそのもので、1 行が 2 行になるだけ。

**失敗を型で消すには IR のノード自体に型を付ける**（`IntNode`/`SymNode` …）しかなく、
それはチェッカの機能ではなく `core_bridge` と島のあいだの IR の再設計になる。
ここで止めた。

今日の形が正しいと考える。抽出子のパニックはブリッジのバグ検出器として働いていて、
`sexpr-int: expected an Int Sexpr node` と関数名・期待変種の両方を出す。
