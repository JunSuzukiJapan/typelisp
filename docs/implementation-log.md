# typelisp 実装ログ（アーカイブ）

最終更新: 2026-07-02 / ブランチ: `feature/compile-sexpr`

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
影響範囲基準で残課題群の最優先として追加対応）。**完了していない残課題（mark-and-sweep
サイクル収集・ネストしたlabelsのknown limitation・retain/release重複除去・break/returnの
対応範囲拡大・compile-if-branchのkind対応・compile-assocのユーザー定義メソッド呼び出し
対応、いずれも優先順位はユーザー未確認）は [TODO.md](TODO.md) を参照**。

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
  gapが残った——詳細はTODO.md参照。今回はメソッド単体のcompile+`Expr::Assoc`からの
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

  `compile-if-branch`経由の値のkind対応はまだ未対応（詳細・理由はTODO.md参照）。

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

残課題はTODO.mdの「compile機能の残課題」節を参照（本対応で1項目解消）。

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
フィールド経由で既に対応済み」という従来の分析のまま、未着手（具体的な
破壊を実証するテストはまだ書けていない）。

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
  伝播、呼び出し側シグネチャには影響しない——呼び出し側での境界検証は未実装のまま、TODO.md参照）。
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

## Sexpr/RtValue内部表現統合 実装計画（2026-07-02起案、Stage 0完了・Stage 1-8未着手）

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
- **Stage 1（未着手）**: `StructPayload::Fields`のmem/rt層プラミング（`rt_struct_new/get/set`）。
- **Stage 2（未着手）**: Struct: インタプリタ結線——`RtValue::Struct`/`StructData`を削除し
  `Expr::Construct`/`FieldGet`/`FieldSet`を新表現に接続。`Vector<T>`/`cons-cell<K,V>`も
  この時点で自動的に新表現に乗る。
- **Stage 3（未着手）**: Struct: コンパイラ結線——`mutable`フラグ分岐、
  `compile-construct-boxed-struct`新設。
- **Stage 4（未着手）**: HashTable: `StructPayload::Map`のmem/rt層プラミング
  （`HashKey`を`MemHashKey`として`typelisp-mem`に移植）。
- **Stage 5（未着手）**: HashTable: インタプリタ結線——`RtValue::HashTable`削除、
  `Vector<T>`と同じ「type_name駆動の`eval_builtin_method`分岐」パターンで
  `get/set/remove/count/clear/keys/values/entries`を再実装。
- **Stage 6a（未着手）**: `BoxedObj::Cell`導入 + `Interp::slot`/`Env`/`sync_roots`を
  `Rc<RefCell<RtValue>>`から`Value::Boxed(Cell)`に置き換え（let/引数/matchバインディング/
  globals全部に影響）。
- **Stage 6b（未着手）**: `RtValue::Closure`自体を`BoxedObj::Closure{body_token, env}`+
  外側クレートの`ClosureBody`サイドテーブルに移行。
- **Stage 7（未着手）**: Scope: `StructPayload::Frames`のmem/rt層プラミング。
- **Stage 8（未着手）**: Scope: インタプリタ結線（最後に単独で着地——
  `src/compiler.rs`自体が`Scope<llvm-value>`で自身のenv/fn-envを構築しているため、
  ここのバグは自己ホスティングコンパイラ全体を静かに壊しうる。
  `tests/compile_test.rs`/`tests/compile_file_test.rs`のフルパスを退行チェックの
  必須ゲートとする）。

各段階で`RtValue`からバリアントが1つずつ消えていき、最終的に`RtValue`には
`Int/Float/Bool/Char/Str/Unit/Data/Sexpr(Value)/Builtin/BuiltinMethod`とLLVM系5種が残る
（名前はそのまま維持、リネームは今回のスコープ外）。詳細な実装計画は
`/Users/suzukijun/.claude/plans/sexpr-lisp-lisp-s-s-lisp-lisp-sexpr-rtv-eager-garden.md`
（Plan mode成果物）参照。
