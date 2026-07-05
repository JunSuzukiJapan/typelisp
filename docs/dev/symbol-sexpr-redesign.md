# Symbol型導入と Sexpr の裏方化（型システム再設計）— 進行中

最終更新: 2026-07-05 / ブランチ: `feature/symbol-type`（`feature/compile-sexpr` から分岐）

このドキュメントは複数フェーズにわたる型システム再設計の**進捗と残作業**を記録する。
Phase 0・Phase 1・Phase 2 は完了。Phase 3（`Vector<T>` コレクション演算）以降が次回作業。

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

## 次回やること（Phase 3 以降）

### Phase 1 — 内部 Sexpr ナビゲーション層（島の生存キット）【✅ 完了】
島専用の Sexpr 型プリミティブを新設し、島が user-facing `car`/`cdr`/`match` に依存しなくて済む状態を作る（Phase 2 の前提）。

実装済み内容:
- `src/check/registry.rs`（`set-cdr` 登録直後、`351` 付近）: `sexpr-cons:(sexpr,sexpr)→sexpr`,
  `sexpr-car:sexpr→sexpr`, `sexpr-cdr:sexpr→sexpr`, `sexpr-consp/sexpr-null/sexpr-atom:sexpr→bool` を builtin 登録。
- `src/eval/interp.rs::eval_builtin`（`set-cdr` アーム直後）: 6 アームを追加。`sexpr-cons`/`sexpr-car`/`sexpr-cdr` は
  `cons`/`car`/`cdr` と同一の heap 操作（`heap.cons`/`car`/`cdr`、`sexpr-cons` は `sync_roots` も同様に呼ぶ）。
  述語は `Value::is_cons`/`is_empty` でランタイムタグを直接読む（`match` 非依存＝Phase 5 の match フェンスを跨いで生存）。
- Rust builtin 実装を選択（doc 推奨どおり）。既存 `sexpr-int`/`sexpr-bool`/`sexpr-str`/`sexpr-sym-name` は
  `compiler.rs` の typelisp defun（`match` 依存）だが、それらは Phase 2 で島が置換する側なので、本層は独立に Rust で置いた。
- `match`-on-Sexpr の置換手段は方針どおり **tag 判定述語＋`sexpr-*` アクセサ＋`if`**（新特殊形なし）。
- gating: 未実装（任意項目）。現状 `public: true` で登録＝ユーザーからも見える。Phase 2 の島移行時に必要なら遮断を再検討。
- テスト: `tests/eval_test.rs` に 3 test 追加（`sexpr_cons_car_cdr_mirror_the_user_facing_ops`／
  `sexpr_car_and_cdr_of_non_cons_panic`／`sexpr_tag_predicates_read_the_runtime_tag`）。user-facing `cons`/`car` との相互運用も検証。

検証: `scripts/test-serial.sh` 全 green（並列 LLVM SIGSEGV 回避）、clippy ゼロ。

**注（Phase 2 への申し送り）**: 本層は現状インタプリタ（`eval_builtin`）専用。島（`compiler.rs`）が
`sexpr-car` 等を実際に使い、かつその defun 自体を `compile` する段になったら、`car`/`cdr`/`cons` と同様の
runtime shim（`is_rt_builtin_name`＝`interp.rs:2132` 付近、`crate::compile::runtime` の `rt_car` 等）への
写像が必要になる。Phase 1 時点では未対応。

### Phase 2 — 島を user-facing car/cdr/match から全面移行【✅ 完了 / commit `ea8e879`→`718677b`】
実装済み内容（4 サブフェーズ、各復元点コミット）:

- **2a（`ea8e879`）— Sexpr payload 抽出を Rust builtin 化**: 島の型付きフィールド読取り
  `sexpr-str`/`sexpr-bool`/`sexpr-sym-name`/`sexpr-int` を `compiler.rs` の `match` ベース defun から
  `Interp::eval_builtin` の Rust builtin へ移設（`Value` payload を直接読む。タグ不一致で panic、旧 `(_ (panic ...))` 契約を保持）。
  加えて非 panic フォールバックのタグ判定用に `sexpr-symp` 述語を新設（`sexpr-consp`/`null`/`atom` の仲間）。
- **2b（`dacdc33`）— タグ抽出 match の書換**: `compile-value`/`compile-int`/`compile-bool`/`compile-float`/
  `compile-pattern-test`（panic フォールバック→抽出 builtin）と `form-is-borrowed?`/`bare-returned-own-name`
  （非 panic フォールバック→`sexpr-symp`＋`if`）。
- **2c（`dacdc33`）— car/cdr 機械置換**: `compiler.rs` の `(car `210 箇所を `(sexpr-car `/`(sexpr-cdr ` へ。
  runtime shim 名マップの文字列 `"car"`/`"cdr"`（`compile-call` の `raw-nm`→`rt_car` 変換）は不変。
- **2d（`dacdc33`）— Cons 走査 match の書換**: 24 箇所を
  `(if (sexpr-consp x) (let ((a (sexpr-car x)) (rest (sexpr-cdr x))) body) fallback)` へ。
  `Option`/`Scope` の `match`（`Some`/`None`）は列挙型なので維持（Phase 5 対象外）。
- **2e（`718677b`）— prelude マクロ本体**: expansion-time に Sexpr マクロ引数を操作するマクロ本体
  `and`/`or`/`dotimes`/`dolist`/`cond`/`if-let`/`case` を `sexpr-*` へ。マクロは常にインタプリタ実行
  （コンパイルされない）ため builtin で安全。展開**出力**側の `car`/`cdr`/`consp`（ユーザーコード）は不変。

**据え置いた判断（重要な設計制約）**: `consp`/`null`/`atom`/`equal`/`append` の defun は `match` ベースのまま。
これらは**ユーザーが `compile` 可能な関数**で、body がコンパイル可能である必要がある（`case` は `(equal ..)` に展開される）。
`sexpr-*` builtin への委譲は runtime shim 未整備（Phase 1 の申し送り＝`is_rt_builtin_name`/`rt_extern_functions`/
`compile-call` の名前マップ拡張）のため `compile` で失敗する（`compile_test` の
`compile_dispatches_consp_null_and_atom_...` が実証）。島はこれらを**インタプリタ経由でのみ**呼ぶため `match` で問題なし。
これらユーザー面 Sexpr リスト操作の移行は Phase 4/5 の課題（`sexpr-*` の compile 対応 shim を先に整備するか、
prelude/島コードを Phase 5 の match フェンスから除外するかの判断を含む）。

**マクロ作者への仕様変更（未ドキュメント化・要 §7）**: `defmacro` 本体で Sexpr マクロ引数を操作する場合、
移行済み標準マクロに倣い `sexpr-car`/`sexpr-cdr`/`sexpr-cons`/`sexpr-consp`/`sexpr-null`/`sexpr-symp` を使う。
`car`/`cdr` は Phase 4 で `cons<T,U>` に付け替わるため、マクロ本体（＝Sexpr 操作）で使うと型エラーになる。
現状ユーザー定義マクロは `car`/`cdr` でもまだ動く（Phase 4 未実施）が、Phase 4 完了時に破綻する。

検証: `scripts/test-serial.sh` 全 green（JIT/AOT compile テスト・end-to-end `compile_file_test` 含む）、clippy ゼロ。

### Phase 3 — `Vector<T>` コレクション演算【✅ 完了】
実装済み内容（`src/prelude.rs`、`iter`/`vector-iter` 定義直後）:
- ジェネリック prelude `defun` を 9 個追加、いずれも純 typelisp・Rust 不要:
  `vector-map`/`vector-filter`/`vector-foldl`/`vector-foldr`/`vector-reverse`/
  `vector-find`/`vector-position`/`vector-count`/`vector-append`。
  builtin の `Vector::new`/`push`/`get`/`len` と `Iter` トレイトの `doiter` 上に記述
  （左→右で足りる走査は `doiter`、逆順が要る `foldr`/`reverse` は明示 index ループ）。
- **`defmethod` でなく `defun` にした理由**: `vector-map`（`Vector<T>→Vector<U>`）・
  `vector-foldl`/`vector-foldr`（アキュムレータ型 `A`）は**受け手の `T` 以外の型変数**が要る。
  `defmethod` は所有型のパラメータ経由でしかジェネリックになれない（`check_assoc_call` は
  メソッドの型引数を `def.params` からのみ解決＝受け手に無い型変数は推論不能）。
  `(defun (name T U) ...)` なら両方を明示宣言でき `check_call` が引数から `unify` 推論する。
- **命名（`vector-` 接頭辞）の理由**: 同型の Sexpr リスト版 `map`/`filter`/`reverse`/… が
  まだ prelude に残る（Phase 4 で撤去）。メソッドと自由関数は受け手型で分岐でき同名共存できるが、
  **自由関数どうしは同名共存できない**ため接頭辞で回避。Phase 4 完了後に平の名前が空く。
- **要素比較が要る演算は述語版**（`vector-find`/`vector-position`/`vector-count`）:
  `T` に `Eq` 相当の境界が無く任意要素を比較する汎用等値が無いため、要素そのものでなく
  述語を取る（CL の `-if` 系に相当）。
- **可変長 `vector-of` ビルダは見送り**: 型付き `&rest (xs T)` は `defun` 本体では `xs` が
  `Sexpr` に潰れる（要素が `T` でなく `Sexpr`）ため純 typelisp では型付け不能。実装するなら
  `interp.rs` に N 引数 Rust builtin が要る。doc の「必要なら」に従い今回は不要と判断、
  テストは既存 vector と同様 `push` で構築。
- **`setf` は代入値の型を返す**（Unit でない）ため、`(if cond (setf n ...) ())` は
  `i32` vs `Unit` で不一致になる。`vector-count` はこれを踏んで `when`（`progn`＋末尾 `()`）で
  ループ本体を Unit 化。
- テスト: `tests/vector_ops_test.rs`（19 test、`load_prelude` 使用）。`map` の `U≠T`・
  `foldl` の `A≠T`・空 vector・string 要素・引数型不一致（型エラー）・単型化経路を網羅。

検証: `scripts/test-serial.sh` 全 green、clippy ゼロ。

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
