# typelisp 実装ログ（アーカイブ）

最終更新: 2026-08-04 / ブランチ: `main`

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
- **表現**: 共有型 [`MacroLambda`](../../src/check/ast.rs)（`required`/`optionals: Vec<Vec<Typed>>`/
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
では既に型変数なので、同じ道具立てで検査できるはず。
