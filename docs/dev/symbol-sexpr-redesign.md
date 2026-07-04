# Symbol型導入と Sexpr の裏方化（型システム再設計）— 進行中

最終更新: 2026-07-04 / ブランチ: `feature/symbol-type`（`feature/compile-sexpr` から分岐）

このドキュメントは複数フェーズにわたる型システム再設計の**進捗と残作業**を記録する。
Phase 0 は完了・コミット済み。Phase 1 以降が次回作業。

---

## 背景・目的

発端は「`(gensym)` はシンボルを返すはずなのに型が `()→Sexpr` なのはおかしい」という指摘。
従来は `Symbol` 独立型が無く、シンボル・リスト・コードデータが単一合併型
`Sexpr = Nil|Int|Float|Char|Bool|Sym|Str|Cons(Sexpr,Sexpr)`（`src/check/registry.rs::sexpr_def`）に
まとまっていた。

**確定した設計（ユーザー判断）:**
- `Symbol` を第一級の型として新設（Phase 0 完了）。
- 等質コレクションは `Vector<T>` に一本化（`List<T>` は作らない）。`map`/`filter`/`fold` 等は Vector 上へ。
- `cons`/`car`/`cdr` は**ジェネリックなペア** `cons<T,U>` に付け替え（既存 `cons-cell<A,B>` の昇格）。
- `match` は **enum（sum 型）振り分け専用**。Sexpr のマッチはユーザー向け仕様から外す。
- `Sexpr` は **read/eval/print/defmacro/自己ホスト `compiler.rs` の内部島**に閉じ込め、
  ユーザー向け API シグネチャから消す。島は新設の `sexpr-car`/`sexpr-cdr` 等の内部アクセサに全面移行。
  ＝ `car`/`cdr`/`cons` を `cons<T,U>` に付け替え、島の約260箇所を `sexpr-*` に書き換える（最大主義パス）。

**再利用できる既存機構（調査済み）:**
- ジェネリック ADT・型引数付き `defun`・単型化（再check方式）は完動
  （`checker.rs` の `FnTemplate`/`MethodTemplate`/`drain_specializations`、`tests/generic_defun_test.rs`・`monomorph_test.rs`）。
- `cons-cell<A,B>`（`prelude.rs:649`）が既にジェネリックなフィールド射影で `car`/`cdr` を持つ。
- `Vector<T>`（`registry.rs::vector_def`、`new/push/get/set/len`）＋`Iter`トレイト＋`doiter`（prelude）。
- 期待型駆動の型引数推論（`None` 方式、`check_construct`）。空 Vector はこれに倣う。

**最重要リスク:** 自己ホストコンパイラ `compiler.rs`（約2500行）は Sexpr を
自由関数 `car` 82回・`cdr` 126回・`match` 53回で走査。prelude マクロ（`dolist`/`cond`/`if-let`）も同様。
Phase 2 の島書き換えが支配的リスク（週単位）。段階的・テスト逐次で。

---

## Phase 0 — Symbol 第一級型【✅ 完了 / commit `ae9dd00`】

実装済み内容:
- `src/types.rs`: `Type::Symbol` 追加（`primitive_types`/`prim_type_path`/`parse_type_name`）。
- `src/check/registry.rs`: `symbol` primitive 登録＋`symbol_assoc`（eq/eql）、`sexpr_def` の `sym` field を `Type::Str→Symbol`、
  `gensym: ()→symbol`、`symbol->string: symbol→string` / `string->symbol: string→symbol` を builtin 登録。
- `src/eval/interp.rs`: `symbol->string`/`string->symbol` の eval_builtin アーム、`construct_sexpr`/`match_sexpr_ctor` の
  `SEXPR_SYM` を Symbol 化（ランタイム表現は `RtValue::Sexpr(Value::Symbol(id))` で不変）、symbol の eq/eql を `sexpr_eq`/`sexpr_eql` へ委譲。
- `src/check/checker.rs`: **Symbol→Sexpr coercion** を中央 reconcile 点（`check` の期待型照合、`2347` 付近）に追加＝
  Symbol が有効な Sexpr datum なので `list`/`cons`/quasiquote へ流れる gensym 結果を自動で `Sexpr::Sym` にラップ。
  `sexpr_ctor_for` に `Symbol=>"sym"`、`mangle_type` に `Symbol`。
- `src/compiler.rs`: 島の `(Sym v)` 5箇所を `symbol->string` 経由に調整（`sexpr-sym-name` ほか）。
- `src/compile/ast_bridge.rs`: `struct_field_kind` に `Type::Symbol=>6`（passthrough、既に tagged な immediate）。
- テスト: `check_test`（gensym の型＝symbol 等）・`type_test`（`Symbol` パース）・`prelude_test`（新契約に更新）。

検証: 全テスト green（`compile_test`/`compile_file_test` 含む＝自己ホスト無傷）、clippy ゼロ。

**元の要望（gensym がシンボルを返す）はここで達成済み。**

---

## 次回やること（Phase 1 以降）

### Phase 1 — 内部 Sexpr ナビゲーション層（島の生存キット）
島専用の Sexpr 型プリミティブを新設し、島が user-facing `car`/`cdr`/`match` に依存しなくて済む状態を作る（Phase 2 の前提）。
- 追加: `sexpr-cons`/`sexpr-car`/`sexpr-cdr`/`sexpr-consp`/`sexpr-null`/`sexpr-atom`。
  既存 `sexpr-int`/`sexpr-bool`/`sexpr-str`/`sexpr-sym-name`（`compiler.rs:270-301` 付近）と同じ島 API 群に揃える。
  実装は Rust builtin（`registry.rs`＋`interp.rs::eval_builtin`）が素直。現行の自由 `cons`/`car`/`cdr`（`registry.rs:351-353`、
  `interp.rs::eval_builtin` の `"cons"` アーム）と同じ heap 操作を流用。
- `match`-on-Sexpr の置換手段: 新特殊形は作らず、**tag 判定述語＋`sexpr-*` アクセサ＋`if`** で分解（`compiler.rs` が既に一部この形）。
- gating（任意）: 内部専用としてユーザーから遮断（モジュール可視性 [[typelisp-visibility-pub]] またはビルトイン内部フラグ）。
- 検証: 構築した Sexpr 値に対する新アクセサのユニットテスト。

### Phase 2 — 島を user-facing car/cdr/match から全面移行【最大の山場】
- `compiler.rs` の `car`(82)/`cdr`(126)/`match`-on-Sexpr(53) を `sexpr-car`/`sexpr-cdr`＋tag述語`if`に書き換え。
- prelude マクロ `dolist`/`cond`/`if-let`（`prelude.rs:200-260` 付近）と、島に残す Sexpr list defun を `sexpr-*` に移行。
- **ユーザー定義マクロへの影響（要ドキュメント化）**: `defmacro` 本体は Sexpr マクロ引数を `car`/`cdr`/`match` で操作。
  付け替え後はマクロ作者が `sexpr-*` アクセサを使う（島 API をマクロ作者に公開）＝仕様変更。
- 進め方: クラスタごとに書き換え→逐次テスト。復元点コミット必須。LLVM 並列 SIGSEGV 回避に `scripts/test-serial.sh`
  （[[typelisp-llvm-link-error-env-shadow]]）。
- 検証: 全 `cargo test` green、JIT/AOT compile テスト通過、コンパイル済みプログラム1本を end-to-end 実行。

### Phase 3 — `Vector<T>` コレクション演算
- ジェネリック prelude `defun` を追加: `map`/`filter`/`foldl`/`foldr`/`reverse`/`member`/`find`/`position`/`count`/`length`/`append` 等。
  既存 `new`/`push`/`get`/`len`＋`doiter` の上に純 typelisp で記述（Rust 不要）。
- 必要なら可変長 `vector` ビルダ（`interp.rs::construct_vector` を N 引数対応に拡張）や vector リテラル。
- 検証: `(map inc (vector-of 1 2 3))` 等の型付きテスト＋単型化テスト。

### Phase 4 — 自由 `cons`/`car`/`cdr`/`set-car`/`set-cdr` を `cons<T,U>` に付け替え
- `registry.rs:351-361`: `cons-cell<A,B>` の演算を自由名に昇格（`cons:(T,U)→cons<T,U>`, `car:cons<T,U>→T`, `cdr:cons<T,U>→U`）。
- 旧 Sexpr list defun（`prelude.rs:170-410` 付近）をユーザー面から撤去（Vector 版に置換 or 島内部化）。
- テスト側 ~50 箇所（`tests/` の car/cdr/cons on Sexpr）を Vector か cons<T,U> か島内部 `sexpr-*` に更新。
- 検証: `cargo test`。

### Phase 5 — `match` の enum 専用 fence
- `AdtDef` に `user_matchable: bool` を追加（`sexpr` は false）。`check_match`/`expect_adt`（`checker.rs:3727`/`3903`）で
  ユーザーコードの `sexpr` scrutinee を拒否。島は match でなく `sexpr-*` を使うので影響なし。
- option/result/error/defstruct の match は維持。
- 任意: ユーザー多variant sum 型のための `defenum` 追加（現状 defstruct=単一variant のみ。「match は enum 振り分け」を実効化するなら要検討）。
- 検証: ユーザーの `(match sexpr値 ...)` がエラーに。option/defstruct の match は green。

### Phase 6 — `&rest` → `Vector<T>`（defun/lambda）
- `checker.rs`: `&rest` 束縛型 `sexpr_ty()`→`Vector<T>`（`1042`/`1057`/`1356` 付近）。`wrap_rest_elem`/`cons_rest_list`
  （`699-725` 付近）を Vector 構築に置換（`sexpr_ctor_for` の要素型制限が消える＝簡素化）。`apply` の末尾リストが Vector を受ける。
- `defmacro` の `&rest` は Sexpr のまま（島）。
- 検証: 可変長 defun／apply のテスト。

### Phase 7 — ドキュメント＆メモリ更新
- `docs/functions.md`（§5/§6/§12/§14）・`docs/syntax.md`・`docs/dev/language-design.md`: Symbol、Vector ベースのコレクション、
  cons<T,U>、match enum 専用、Sexpr=内部島 を反映。**Phase 0 時点では未更新**（`gensym` の型表記等は要修正）。
- メモリ: 本再設計の要点を記録（[[typelisp-sexpr-rtvalue-unification]] の続きとして）。

---

## 承認済み計画の原本

`~/.claude/plans/symbol-sexpr-sexpr-temporal-hammock.md`（セッション外のためリポジトリには無い。本ドキュメントが同内容の in-repo 版）。
