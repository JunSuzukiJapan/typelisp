# Symbol型導入と Sexpr の裏方化（型システム再設計）— 完了

最終更新: 2026-07-22 / ブランチ: `main`（作業ブランチ `feature/symbol-type` は main へマージ済み）

> **2026-07-10 追記**: Phase 5 の `match`-on-`Sexpr` fence は、`(read)` ビルトイン導入準備のため
> 再解禁された（詳細は Phase 5 節末尾の「Phase 5 方針転換」を参照）。Sexpr は user-facing の
> `match` 対象として復帰したが、`sexpr-*` アクセサ層自体は撤去していない。

このドキュメントは複数フェーズにわたる型システム再設計の**進捗**を記録する。
Phase 0〜3・Phase 4a（コレクションコンビネータの generic `Iter` 化）・**Phase 4b（`cons`/`car`/`cdr` の
`cons<T,U>` 付け替え）**・**Phase 5（`match` の enum 専用 fence＋ユーザー面 Sexpr リスト操作の撤去）**まで完了。
Phase 4b と Phase 5 は元々一体だったが、リスク分離のため Phase 5 を先行実施した（Commit 1＝Phase 5、Commit 2＝Phase 4b）。
**Phase 6.5（ユーザー面リスト/ペア走査 API の `cons<T,U>`/`Vector<T>`/`Iter` 上での再設計）と
Phase 6.6（その残ギャップ解消＝impl メソッドの `where` 節＋compile の `equals`/`less` 対応）も完了**。
**Phase 7（ドキュメント整備）も完了を確認**（2026-07-22、`docs/dev/TODO.md`からの棚卸しで
`docs/functions.md`/`docs/syntax.md`/`docs/dev/language-design.md`を突き合わせ——`gensym`の型表記
`()→Symbol`、`Symbol`独立プリミティブ型としての記載、`Vector<T>`への一本化、`cons`/`car`/`cdr`の
`cons<T,U>`化、`match`のenum専用化、`Sexpr`が内部island層である旨、いずれも既に反映済みだった。
下記「Phase 7」節参照）。`defenum`（ユーザー多variant sum型）は本再設計とは別に
2026-07-09実装済み（[implementation-log.md](implementation-log.md)参照）。値レベル `&rest`/`apply`
（当初の Phase 6「`&rest` → `Vector<T>`」計画は破棄）は本再設計とは無関係な既知の制限として
2026-07-08に一度削除されたが、2026-07-15に別件（`docs/dev/TODO.md`の残作業棚卸し）で再導入した
（下記 Phase 6 節参照）。

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

### Phase 4a — コレクションコンビネータを generic `Iter` 化【✅ 完了】
ユーザー判断で当初計画（Sexpr 版撤去＋`vector-` 版へ一本化）を上書きし、`map`/`filter`/
`foldl`/`foldr`/`reverse`/`find`/`position`/`count`/`remove-if` を **`Iter` を実装する任意の
コレクションで使える generic 自由 `defun`** に統一した。

実装済み内容:
- **型システム拡張（`checker.rs::check_call`）**: `where` 節の関連型ピンが *推論* に参加するように
  した。(1) ピン検証で `declared_ty` を `subst_apply` してから比較（ピンが型変数でも可）、
  (2) 引数 unify 後・「cannot infer」判定前に、具体化済みイテレータの実 `Item` を
  `resolve_trait_assoc_type` で解決して型変数ピンへ `unify`。これにより `reverse` のように
  要素型 `A` が引数に現れず**イテレータの `Item` からしか決まらない**コンビネータも書ける。
- **prelude**: Sexpr 版 `map`/`filter`/`foldl`/`foldr`/`reverse`/`find-if`/`position*`/`count*`/
  `remove*` と Phase 3 の `vector-*` 版を撤去し、`(defun (map I A U) ((it I) (f (fn (A) U)))
  Vector<U> (where (Iter I (Item A))))` 形の generic 版に置換。イテレータを取り
  （`(map (iter coll) f)`）、結果コレクションは `Vector` に materialize、`foldr`/`reverse` は
  前方向 `Iter` を一旦 `Vector` にバッファ。要素比較が要る演算は述語版（`A` に `Eq` 境界なし）。
- **島内部 `sexpr-map` 新設**: `case`/`do` マクロ本体が展開時に Sexpr を `map` していた箇所を
  `sexpr-*` ベースの `sexpr-map` に移行（ユーザー面 `map` は generic `Iter` 版になったため）。
- `every`/`any`/`member`/`assoc`/`sort` は Sexpr 専用のまま存続（`eq` 比較や cons 直操作が要る）。
- **コレクション直接形は見送り**: `(map coll f)`（イテレータでなく collection を直接）には
  `IntoIter` 相当のトレイト＋関連型の関連型境界が要り、現行の型システムでは安価に表現不能。
  イテレータ取り（`(iter coll)` 橋渡し）が `doiter`/`count-iter` 既存慣習とも一致。
- テスト: `vector_ops_test.rs`（generic 版へ更新、19）、`doiter_test.rs`（ピン推論・HashTable 越し、+2）、
  `prelude_test.rs`（撤去済み Sexpr コンビネータのテストを削除）、`dispatch_test.rs`（`count` は
  generic `Iter` 版 vs HashTable メソッドの分岐に更新、`remove` はメソッド専用に）。

検証: `scripts/test-serial.sh` 全 green、clippy ゼロ。

### Phase 4b — 自由 `cons`/`car`/`cdr`/`set-car`/`set-cdr` を `cons<T,U>` に付け替え【✅ 完了 / Commit 2】
Phase 5 で存続 Sexpr 関数を全撤去済み（島は `sexpr-*`＋`string` メソッドのみ使用）だったため、
Phase 4b は当初懸念（存続関数の `car`/`cdr` 移行）が不要になり、`cons`/`car`/`cdr` の付け替えに集中できた。

実装済み内容:
- **`cons` はペア構築の自由 `defun`（`prelude.rs`）**: `(defun (cons A B) ((a A) (b B)) cons-cell<A,B>
  (cons-cell::new a b))`。`car`/`cdr` は `cons-cell` の**フィールドアクセサメソッド**（`defstruct` が
  `car`/`cdr` フィールドから自動生成、受け手型付きで静的型 `A`/`B` を射影）。`set-car`/`set-cdr` は撤去
  （唯一の呼び手 `nconc`/`nreverse` は Phase 5 で削除、ペアのフィールド変更は `(setf p::car v)`）。
- **Sexpr `cons` バリアントのベア名を閉じた（`registry.rs`）**: `register_ctors` 後に `root.ctors.remove("cons")`。
  これで `(cons a b)`/`(Cons a b)`（大文字は case-fold で同一）が自由ペア関数へ解決。`nil`/`int`/`str`/… の
  データコンストラクタは据え置き（`(Int 5)`/`(Nil)` は依然書ける）。
- **内部 Sexpr cons 構築の付け替え（`checker.rs`）**: `resolve_ctor("cons")` に依存していた `list`
  （`check_list_lit`）・quasiquote（`check_qq_template`）を新ヘルパー `sexpr_cons_ctor()`（`sexpr` 型定義から
  直接 variant を引く）経由に。`&rest` リスト構築 `cons_rest_list` の `Expr::Call(cons)` を `sexpr-cons` へ。
- **compile シム（`interp.rs`/`compiler.rs`）**: `is_rt_builtin_name` を `car|cdr|cons|set-car|set-cdr` から
  `sexpr-car|sexpr-cdr|sexpr-cons` に、`compile-call` の名前マップ（`car→rt_car` 等）を `sexpr-car→rt_car` 等へ。
  ＝ Phase 1 の申し送り（島 `sexpr-*` の compile 対応）をここで完了。自由 `cons`/`car`/`cdr` は通常の
  `defun`/メソッドとしてコンパイルされる。`eval_builtin` の旧 `cons`/`car`/`cdr`/`set-car`/`set-cdr` アームは削除。
- **テスト**: Sexpr を明示構築/走査するテスト（eval/check/compile/macro/redefine/scope の cons/car/cdr）を
  `sexpr-cons`/`sexpr-car`/`sexpr-cdr` へ書換。`set-car`/`set-cdr` テストは削除。マクロ本体の `(cons ..)` は
  `(sexpr-cons ..)` へ（マクロ作者向け仕様、§7）。新規に `cons<T,U>` ペアのテスト（`(car (cons 1 2))`=1、
  異種型ペア `(cons 7 "x")`、`(setf p::car ..)`、非ペア `car` の型エラー）を prelude_test に追加。

検証: `scripts/test-serial.sh` 全 green、clippy ゼロ。

### Phase 5 — `match` の enum 専用 fence + ユーザー面 Sexpr リスト操作の撤去【✅ 完了 / Commit 1】
ユーザー判断（本セッション）で当初の据え置き（`consp`/`equal`/`append` を compilable な `match` ベースで存続）を上書きし、
**ユーザー面 Sexpr リスト操作を一括撤去し、`match`-on-`Sexpr` を prelude 含め完全撤廃**した（「あとから `cons<T,U>`/`Vector<T>` 上に再設計」）。

実装済み内容:
- **match fence（`checker.rs::check_match`）**: `expect_adt` 直後に scrutinee が `sexpr` 型なら拒否。
  `AdtDef` への `user_matchable: bool` 追加は不要と判明（`sexpr` パス比較で足りる。option/result/error/defstruct は素通り）。
  島（`compiler.rs`）は既に `match` でなく `sexpr-*` を使うので影響なし（Phase 2 の成果）。
- **prelude 撤去**: `consp`/`null`/`atom`/`length`/`append`/`nthcdr`/`nth`/`elt`/`last`/`butlast`/`take`/`subseq`/
  `copy-list`/`member`/`every`/`any`/`nconc`/`nreverse`/`insert-sorted`/`sort`/`assoc` と マクロ `dolist` を削除。
- **`equal`/`equalp` は存続（Rust builtin 化）**: 等値比較は「リスト操作」でなく `case` 展開先でもある遍在プリミティブなので残すが、
  `match` ベース defun から `Interp::eval_builtin` の Rust builtin（`sexpr_equal`/`sexpr_equalp`、`registry.rs` に FnSig 登録）へ移設。
  スカラ型の `equal`/`equalp` *メソッド*（string/char/int/…）は従来どおり別（インスタンスメソッド優先解決）。
- **島インフラの再配置**: 自己ホストコンパイラ `compiler.rs` は削除関数を一つも使っていなかった（`(append ..)`/`(equal ..)`/
  `(length ..)` は全て **`string` インスタンスメソッド**、`every`/`assoc` はコメントのみ）。`sexpr-*` 層で `Sexpr` を走査する。
  `,@`（unquote-splicing）だけが `Sexpr` リスト連結を要するため、内部 `sexpr-append`（prelude defun、`sexpr-*` ベース）を新設し
  `check_qq_template` をそこへ向けた。
- **`sexpr-*` 層の拡充**: `match`-on-`Sexpr` 廃止に伴い `Sexpr` payload を読む必要が残る箇所のため `sexpr-float`（`sexpr-int` の
  f64 版、`registry.rs`+`eval_builtin`）を新設。既存の `sexpr-int`/`sexpr-bool`/`sexpr-str`/`sexpr-sym-name`/`sexpr-consp`/… と揃う。
- **`while-let`/`doiter`/`do` マクロ**: 展開時に macro 引数（`Sexpr`）を走査する `car`/`cdr` を `sexpr-car`/`sexpr-cdr` へ移行。
- **テスト**: 撤去関数のテスト（prelude_test 35・dispatch_test 2・check/eval の dolist 5）は削除。`match`-on-`Sexpr` を
  他機能検証に使っていたテスト（eval &rest/apply/gc、scope_test の `Scope<Sexpr>`、struct_test の Sexpr フィールド、
  vector_test）は `sexpr-*` アクセサへ書き換え。**コンパイラの Sexpr `match` テスト（Stage 5 の 14 本）は削除**
  ＝ユーザー面 Sexpr `match` コンパイルという撤去済み能力の検証だったため（GC ルート機構自体は `typelisp-rt` の
  raw builtin テストで別途担保）。
- ユーザー多variant sum型のための `defenum` は本Phase範囲外——2026-07-09に別途実装済み。

検証: `scripts/test-serial.sh` 全 green、clippy ゼロ。

**残: ユーザー面リスト/ペア走査 API の再設計は Phase 6.5（下記）へ。**

#### Phase 5 方針転換 — `match`-on-`Sexpr` を再解禁【✅ 完了 / 2026-07-10】

上記 match fence（`checker.rs::check_match` の `sexpr` パス拒否）を撤回し、`match` を再び `Sexpr` の
eliminator として使えるようにした。ユーザー向けに実行時にしか型の分からない値を持ち込む
`(read)` ビルトインを導入する準備であり、read データの構造は型精緻化・網羅性チェック付きで
分解できる必要があるため、Phase 5 時点の「Sexpr は内部型・`sexpr-*` アクセサのみ」判断を上書きした。

実装済み内容:
- **fence 除去**: `checker.rs::check_match` の `adt_name == Path::root("sexpr")` 早期リターンを削除。
  網羅性チェック（`total_variants`）・`check_ctor_pattern` は元々 ADT 種別に依存しない汎用機構なので、
  追加実装なしで `sexpr` の全 10 variant（`nil`/`int`/`float`/`char`/`bool`/`sym`/`str`/`cons`/`bignum`/`ratio`）
  に対して働く。`if-let`/`while-let`（prelude マクロ、両者とも二腕 `match` へ展開）も自動的に追従。
- **interp 側**: `match_sexpr_ctor`（`eval/interp.rs`）は Phase 5 当時から生存していたため無改修。
- **compile 側の穴埋め**: `float`/`bignum`/`ratio` は `TAG_BOXED` を共有し 3bit タグだけでは区別できないため、
  `typelisp-rt` に `rt_box_kind`（0=非該当・1=float・2=bignum・3=ratio）を新設し、
  `compiler.rs::compile-sexpr-tag-test` の該当 3 variant をタグ判定から `rt_box_kind` 呼び出しへ変更。
  ~~`bignum`/`ratio` の**ペイロード**（`compile-sexpr-field`）は `rt_bignum_*`/`rt_ratio_*` 未実装のため
  引き続き明示 panic（`(bignum _)`/`(ratio _)` のワイルドカードでの tag-only 分岐は compile 可能）。
  同様に `sym` のペイロード抽出（`Symbol` 型は compile 表現なし）も明示 panic のまま。~~
  **→ 2026-07-15 解消**（`compiler.rs::compile-sexpr-field`の`sym`(5)/`bignum`(8)/`ratio`(9)/`path`(10)
  各分岐は現在いずれもpanicせず、既にタグ付き済みの値をそのままpassthroughする——bignum/ratioの
  compile対応・quoted data内のSym/Pathのcompile対応が同日中に解消したため）。
- **潜在バグの発見と修正**: `compile-ctor-subpatterns` の「サブパターンが `pat-wild` か」判定が
  `(eq (sexpr-sym-name ...) "pat-wild")` という**識別子比較**で書かれており、文字列内容比較のつもりが
  常に false になっていた（`eq` は CL 同様ポインタ同一性、内容比較は `equal`）。このため `(sym _)`/
  `(bignum _)`/`(ratio _)` のようなワイルドカード付き ctor パターンでも無条件に `compile-sexpr-field` を
  呼んでいた。`eq`→`equal` に修正（`compiler.rs` 2 箇所）。Sexpr match 解禁でこの経路が初めて実際に
  踏まれるようになるまで顕在化していなかった。
- ~~**注記（未対応のまま）**: 実行時 `Value::Path`（`a::b` トークン）に対応する `Sexpr` variant は存在しない。
  path な `Sexpr` 値は `match` では `_` にしか落ちない。`(read)` 導入時に扱いを決める。~~
  **→ 2026-07-19 解消**（`registry::sexpr_def`に11番目のvariant`path`を追加、`crate::eval::interp`の
  `SEXPR_PATH`）。フィールド型は当初`Str`（書かれた形へ再結合、例`"dep::head"`）で実装したが、
  ユーザー指摘（「一般的に、いったん分解した要素を文字列に直すのは筋が悪く使い勝手が悪くなる」）
  により`Sexpr`のsym列（真のリスト、quoted `'(dep head)`と同じ形）へ設計変更——セグメント数や
  `car`/`cdr`による個別アクセスが再パース無しで手に入る。真のリスト構築は`match`中に新規cons
  セル確保が必要になる（既存のどの`match_sexpr_ctor`腕も「既にヒープ上のデータを読むだけ」だった
  唯一の例外）ため、`match_pattern`/`match_sexpr_ctor`のシグネチャを`&Heap`→`&mut Heap`に変更し、
  `alloc_quoted`の`QuotedSexpr::Cons`と同じ`push_root`/`pop_root`規律でリストを構築
  （要素は全て永続internされた`Value::Symbol`なのでルート不要、新規consチェーン自体のみ保護が必要）。
  構築（`(Path segs)`、`segs`はsymのリスト）も対称に対応。
  **自己ホストコンパイラ側も同日中に対応**（`sym`のペイロードも含めユーザー指示で追加実装）:
  `struct_field_kind`が既に`Type::Symbol => 6`（`Str`と同じpassthrough kind）としていたことが手掛かりで、
  `compile-sexpr-field`/`compile-construct-sexpr`のvariant5（sym）はpanicからpassthroughへ、variant10
  （path）は新設`rt_path_to_list`/`rt_list_to_path`（`typelisp-rt`、pathのinternセグメントから
  cons チェーンを構築/逆にリストをinternする、同じpush_root規律）呼び出しへ変更、
  `compile-sexpr-tag-test`にvariant10→タグ5（`TAG_PATH`）を追加。`Interp::decode_compiled_return`にも
  欠落していた`Type::Symbol`腕を追加（`is_boxed_sexpr_type`が拾わずbare`RtValue::Int`に化けていた
  既存の欠落、`sym`を返すcompiled関数がinterp境界を越えると壊れていた）。
  **踏んだ罠**: `ast_bridge.rs`の`translate_quote`（quoted symbol/path literal、2026-07-15の
  「既知の制限7項目解消」で追加）が既にvariant番号`5`/`10`を「リテラル名文字列からintern」という
  **別の意味**でマジックナンバー使用済みだったため、`compile-construct`の`(eq variant 5)`/
  `(eq variant 10)`早期分岐が今回追加した「既存のタグ付き値からconstruct」ケースと衝突し、
  `(Path segs)`（`segs`が既にタグ付きSexprリスト）を書くと`rt_intern_symbol`が非Str引数を受け取り
  abort——`compile_construct_and_round_trips_the_path_variant`テストのSIGABRTで発覚。
  quoted literal側を範囲外の専用マーカー`QUOTE_SYM_MARKER=100`/`QUOTE_PATH_MARKER=101`へ振り直して
  解消（`ast_bridge.rs`の定数+`compiler.rs`の対応する分岐、単体テスト2件も更新）。
  テスト: `tests/match_sexpr_test.rs`に3件（構築/分解の往復、非pathスクルティニーが誤って`path`腕に
  落ちないことの確認、既存の網羅的11分岐テストも更新）、`tests/compile_test.rs`に3件
  （sym payloadのbind+JIT境界越え往復、pathのタグ判定+抽出、pathの構築+往復）。
- **テスト**: `tests/match_sexpr_test.rs` 新設（interp 側、全 variant・ネストパターン・網羅性エラー・
  `if-let`/`while-let`）。`tests/compile_test.rs` 末尾に compile 側 4 本追加
  （payload 抽出、float/bignum/ratio 判別、nil/sym/str/bool のタグのみ分岐、interp/compile 一致）。

検証: `cargo test` 全 suite green、`scripts/test-serial.sh` green。

### Phase 6 — `&rest` → `Vector<T>`（破棄）
当初計画（`defun`/`lambda` の型付き `&rest` を `Vector<T>` に付け替える）は破棄。可変長パラメータを
均質配列型で表すのは不自然という判断。値レベル `&rest`/`apply` 自体は元の Sexpr ベース設計
（本フェーズより前から実装済みだったもの）のまま存続——2026-07-08 に本 Phase 6 の破棄を理由に
道連れで一度削除されたが、Sexpr ベース設計自体に欠陥があったわけではなかったため
2026-07-15 に `docs/dev/TODO.md` の残作業棚卸しで再導入した。`defmacro` の `&rest` は元から
Sexpr のまま（島）で変更なし。

### Phase 6.5 — ユーザー面リスト/ペア走査 API の再設計【✅ 完了】
Phase 5 で**ユーザー面の `Sexpr` リスト操作を一括撤去**した（`consp`/`null`/`atom`/`length`/`append`/
`nthcdr`/`nth`/`elt`/`last`/`butlast`/`take`/`subseq`/`copy-list`/`member`/`every`/`any`/`nconc`/
`nreverse`/`insert-sorted`/`sort`/`assoc`＋マクロ `dolist`）。本フェーズはこれらを
`cons<T,U>` ペア／`Vector<T>`／`Iter` トレイトの上で**型付き API として再構築**した。

**確定した設計（ユーザー判断）:**
- **API 軸**: Phase 4a の `map`/`filter` 等と同型の **generic `Iter` 自由 `defun`**
  （`(length (iter coll))` 形式、結果コレクションは `Vector<A>` に materialize）。`Vector` 専用にせず、
  `HashTable<K,V>` にも将来の `Iter` 実装型にも同じ定義が効く。
- **命名**: プレーン CL 名（`length`/`append`/`nth`/`elt`/`take`/`subseq`/`last`/`butlast`/`member`/
  `every`/`any`/`sort`/`assoc`）。Phase 5 で名前が空いていたため復活できた。既存 `vector-append`
  （Phase 3）は撤去し、generic 2-iterator 版 `append` に統合。
- **等値/順序は新設トレイトで表現**: `deftrait Eq (equals ((self Self)(other Self)) bool)` /
  `deftrait Ord (less ((self Self)(other Self)) bool)`。`member`/`sort`/`assoc` は
  `(where (Eq A))`/`(where (Ord A))` 境界で述語なしに書ける（既存 `find`/`position`/`count`/
  `remove-if` は述語版のまま存続＝両流儀併存）。
  - メソッド名は `equals`/`less`（`eq`/`eql`/`equal`/`equalp`/`lt` は全 scalar 型の builtin メソッドで
    再定義不可＝衝突。`?`/`!` サフィックスはプロジェクト規約で禁止）。
  - impl 対象は **i32/i64/f64/string/char/bool/symbol の7 primitive 型**（`f32`/`i8`/`i16`/`u*` 系は
    委譲先の `=`/`<` を持たないため対象外）。数値は `=`/`<` へ、`string`/`char`/`bool` は `equal` へ、
    `symbol` は `eq` へ委譲（`string` の `eq` は `Rc` 同一性なので `member`/`assoc` には不適）。
  - `cons-cell<A,B>` への再帰的 `Eq` impl は Phase 6.5 時点では**見送り**（impl メソッドが `where` 節を
    持てなかったため）→ **Phase 6.6 で解消済み**（下記）。
- **assoc**: `Item` が `cons-cell<K,V>` の任意イテレータ上に `(assoc k it)` → `Option<cons-cell<K,V>>`。
  複合の関連型ピン `(where (Iter I (Item cons-cell<K,V>)) (Eq K))` が成立するため、alist
  （`Vector<cons-cell<K,V>>`）にも `HashTable<K,V>`（`iter` の `Item` がまさに `cons-cell<K,V>`）にも
  同じ関数が効く。値は `(cdr (unwrap (assoc k it)))` で射影。
- **再提供しないもの**: 破壊的 `nconc`/`nreverse`（`reverse`＝Phase 4a の非破壊 generic 版で代替）、
  cons 鎖専用の `nthcdr`/`copy-list`（`Vector` 上では意味を持たない）。
- **CL からの意図的な乖離**（イテレータ形状に起因）:
  - `member` は **`bool`** を返す（イテレータに「残り」のコンスが無いため tail を返せない）。
  - `last` は最後の**要素**（`Option<A>`）を返す（CL の「最後の cons」ではない）。
  - `nth`/`elt` は範囲外で `nil` でなく **`Option<A>`**（`find`/`position` の慣習に合わせた）。
    CL の引数順をそのまま踏襲: `(nth n it)` だが `(elt it n)`。
  - `subseq` は `end` が入力長を超えてもクランプする（CL はエラー）。
- ~~**generic `defun` 本体は自己完結が必須**: `where` 境界の伝播は未実装（`Checker::check_call` の
  `cannot infer type parameter` 判定）のため、境界付き generic の本体から別の境界付き generic を
  呼べない。`elt` が `nth` へ委譲せず同じループを複製しているのはこのため。~~
  **→ 2026-07-19 解消**。原因は`check_call`の関連型ピン推論ループ（`sig.bounds`から`subst`へ
  実際の紐付けを解決する箇所）が、束縛先の型が**まだ裸の未解決型変数**（＝呼び出し元自身の型
  パラメータがそのまま転送されてきた場合）だと`reg.type_def`で引けず単に`continue`していたため
  ——`elt`の本体診断チェック中、`it : I`（`elt`自身の未解決`I`）を`nth`へそのまま渡すと、`nth`の
  戻り型`Option<A>`の`A`を紐付ける手掛かりが`(Item A)`ピン一本だけなのに、そのピン解決が
  スキップされ`A`が永久に未解決のまま「`cannot infer type parameter A for nth`」になっていた。
  `validate_where_bounds`（呼び出し先の検証側）は同種の裸型変数ケースを`caller_bounds`
  （`Env::bounds`、囲む関数自身の`where`節）と照合する分岐を2026-07-15の「既知の制限7項目解消」
  で既に持っていたが、*推論*側の同名ループには同じ分岐が無かった——検証は直っていたのに推論が
  直っていない、という非対称な見落とし。`validate_where_bounds`と対になる分岐を推論ループにも
  追加し、束縛先が裸の型変数のときは`env.bounds`から同じtrait名の呼び出し元自身の境界を探し、
  その関連型ピン（`elt`の`(Item A)`の`A`、まだ開いたまま）を`subst`へ直接unifyするよう変更
  （`check_call`、`src/check/checker.rs`）。`elt`を実際に`nth`へ委譲するよう`prelude.rs`を更新。
  テスト: `tests/tmp_where_bound_check.rs`で確認後削除（使い捨て）、既存の`tests/seq_ops_test.rs`の
  `elt`テストがそのまま回帰確認になる（実装が変わっても外部から見た挙動は同一）。
- **`compile`（自己ホストコンパイラ）は Phase 6.5 時点では未対応**（`compile-assoc` の固定リスト外
  panic）→ **Phase 6.6 で解消済み**（下記。primitive レシーバのユーザーメソッドが compile 可能に）。

**実装**: `src/prelude.rs`（`Eq`/`Ord` トレイト＋7型分の scalar impl＋13 個の `defun`）。
`checker.rs`/`registry.rs` は変更不要（primitive への `impl`・`Self` 第2引数・関連型ピンなし境界・
複合関連型ピンは全て既存機構で成立することを実装前に検証済み）。
テスト: `tests/trait_test.rs`（primitive 型への trait impl の単体テスト）、
`tests/vector_ops_test.rs`（`vector-append`→`append` 移行）、
`tests/seq_ops_test.rs`（新規、31 test：各関数の基本/境界ケース、`Eq`/`Ord` のユーザー型 impl・
未実装エラー、`HashTable` 越し `assoc`、string の `length`/`append` が builtin メソッド優先解決の
ままであることの回帰確認、同一プログラム内の複数型特殊化）。

検証: `scripts/test-serial.sh` 全 green、clippy ゼロ。

> 併記: ユーザー多 variant sum 型のための `defenum`（本再設計の対象外）は2026-07-09に別途実装済み。

### Phase 6.6 — Phase 6.5 の残ギャップ解消【✅ 完了】
Phase 6.5 で将来課題とした2件（cons-cell への再帰的 Eq/Ord impl、compile の equals/less 対応）を解消した。
5 コミット構成（checker where節 → 呼び出しサイト検証 → prelude impl → compile i32/i64 → compile string/char）。

**1. implメソッド/defmethod の `where` 節対応（checker）:**
- `parse_defmethod_sig`（checker.rs）が ret 直後の `(where ...)` を `parse_defun_sig` と同一の peek で受理
  （`MethodSig` に `bounds`/`body_start` を追加）。`check_defmethod` は本体 env を `with_bounds` で構築
  ＝ impl メソッド本体で所有型の型変数へのトレイトメソッド呼び出しが既存の
  `check_instance_method` bounds ブランチ（`Expr::TraitCall`、診断専用）で型付けされる。
- 特殊化（`specialize_method_form`）は `specialize_defun_body` と同じ根拠で bounds を **drop**
  （具体化後は実レシーバの assoc で解決、呼び出しサイトで検証済み）。
- **呼び出しサイト検証**: `check_call` の境界検証ループを `validate_where_bounds` として抽出し
  `check_assoc_call`（cannot infer 判定直後）からも呼ぶ。境界不成立のメソッド呼び出しが特殊化 drain 内の
  不透明な `NoSuchFunction` でなく「does not implement trait」の明確な型エラーになる。
- `check_impl` は**無変更**で通った（メソッド form の elems[3..] パススルーで where 節が届く）。
  制約: where 節の型変数名はレシーバに書いた型変数名＝ defstruct 宣言のパラメータ名と同一必須。

**2. prelude: `cons-cell<A,B>` の再帰的 Eq/Ord impl:**
- `equals`: `(where (Eq A) (Eq B))` でフィールドごとの構造比較。ネスト
  （`cons-cell<cons-cell<i32,i32>,i32>`）は単型化が一段ずつ再帰して有限収束。
- `less`: `(where (Ord A) (Ord B))`、car 主キー・cdr 副キーの辞書式。double-less 形
  （car厳密小→true / car厳密大→false / 同値→cdr比較）で `(Eq A)` を追加要求しない。
- `member`/`sort` が `Vector<cons-cell<...>>` にそのまま効くようになった。

**3. compile: primitive レシーバのユーザーメソッド（`i32::equals` 等）対応:**
- **関門A（`Interp::compile_function`）**: assoc ターゲットのフィルタを
  「`self.methods` にユーザー登録がある組は通常の前方宣言/extern 配線対象、
  ネイティブに下ろす `i64`/`i32`/`char`/`string` builtin だけ除外」に変更。
- **関門B（`compile-assoc`、compiler.rs SOURCE）**: ディスパッチを (type-name, method) ペア基準に再構成。
  `int-native-method?`/`string-native-method?`/`char-native-method?` 述語で**引数コンパイル前に**判定し
  （IR 二重 emit 回避）、非ネイティブは新設 `compile-assoc-user`（旧 else 分岐の抽出、`m` を閉じ込む
  labels 兄弟）のマングル名呼び出しへ合流。
- **string**: `equal`→既存 `rt_str_eq`（内容比較、interp の `equal` と意味一致）、`lt`→新設
  `rt_str_lt`（typelisp-rt、interp の `string_lt` と同一意味論）。**char**: compiled では raw i64
  コードポイントなので `eq`/`eql`/`equal`→`icmp-eq`、`lt`→`icmp-lt`（`equalp` は case-insensitive
  なので非ネイティブのまま）。
- **`call_compiled` の境界変換を拡張**（既存ギャップが Eq/Ord で顕在化したため）: 戻り値の
  `Type::Bool` デコード（0/1→`RtValue::Bool`）と、引数の `Bool`/`Char`/`Str`
  （heap 確保＋call 中 root 保護）変換を追加。

**残る将来課題（Phase 6.6 対象外）:**
- ~~ジェネリック本体そのもの（`member` 等の `Iter` コンビネータ）の compile~~ **→ 2026-07-13 解消**
  （branch `feature/iter-compile`、[iter-compile-plan.md](iter-compile-plan.md)）。`Vector<T>::get/len/
  push` 等は `vector-op`/`hashtable-op` ノード + `rt_struct_*`/`rt_hashtable_*` で compile 対応。
  空白入りマングル名で名指しできない問題は `Interp::compile_function_rec` の**推移的自動 compile**で
  解決（`(compile fn)` が呼ぶ単型化インスタンスを自動的に辿って compile）。
- ~~`equalp` 系（case-insensitive）の compile~~ **→ 2026-07-13 解消**（char/string の `equalp` を
  `rt_char_equalp`/`rt_str_equalp` で対応、`compile-assoc` の char/string 分岐 + `char-native-method?`/
  `string-native-method?` に追加）。~~`HashTable::get`/`remove`（`Option` 返し）~~ も解消（上記参照）。
- ~~`f64` レシーバのメソッド compile~~ **→ 2026-07-13 解消**（算術 `+`/`-`/`*`/`/`/`mod` と比較
  `<`/`<=`/`>`/`>=`/`=`/`/=`/`eq`/`eql`/`equal`/`equalp`）。新設ビルトイン `build-fadd`/`fsub`/`fmul`/
  `fdiv`/`frem`（各 `bitcast` i64↔double で挟む）+ `build-fcmp-*`、`compile-assoc` に f64 分岐 +
  `float-native-method?`、`call_compiled` の f64 引数/戻り値マーシャリング。~~残: transcendental
  （`sqrt`/`floor`/`expt`/...、libm 必要）と変換（`float->int`/`float->bignum`/`float->ratio`）。~~
  →2026-07-15 追加解消: transcendental 全種（`sqrt`/`floor`/`ceiling`/`round`/`truncate`/`expt`）と
  `float->int`。各 LLVM 組み込み関数（`llvm.sqrt.f64`等、`expt`のみ`llvm.pow.f64`）へ lowering する
  新設ビルトイン `build-fsqrt`/.../`build-fpow`（`Intrinsic::get_declaration` でモジュールへ宣言、
  冪等なので再呼び出し安全）、`float->int` は単一 `fptosi` 命令の `build-fptosi`（ヒープ確保なし）。
  ~~残るは `float->bignum`/`float->ratio` のみ——`bignum`/`ratio` はcompiled表現が無いため対象外~~
  →2026-07-15 解消（`bignum`/`ratio` に `Type::Str` と同じ「常にタグ付き `TAG_BOXED` ポインタ」の
  compiled表現を新設、`rt_bignum_*`/`rt_ratio_*` 一式・`compile-bignum-literal`/
  `compile-ratio-literal`・`bignum-native-method?`/`ratio-native-method?` を追加）。
  落とし穴: LLVM `frem` は C の `fmod` 呼び出しに lower されるため、`fmod` という名の関数を compile
  すると JIT のシンボル解決が衝突して無限再帰する（`fptosi` にも類似の既知ギャップ: NaN/範囲外
  入力で poison 値になり、インタプリタの飽和的 `as i64` キャストとは境界ケースのみ発散する）。

テスト: trait_test（defmethod 直書き where・再帰 impl・境界エラー3本）、seq_ops_test（ペアの
equals/less/member/sort/ネスト/Eq 未実装エラー6本）、compile_test（i32/string/char の equals/less
compile＋未 compile エラー5本）、typelisp-rt（rt_str_lt 1本）。
検証: `scripts/test-serial.sh` 全 green、clippy ゼロ。

### Phase 7 — ドキュメント＆メモリ更新【✅ 完了 / 確認: 2026-07-22】
- `docs/functions.md`（§5/§6/§12/§14）・`docs/syntax.md`・`docs/dev/language-design.md`: Symbol、Vector ベースのコレクション、
  cons<T,U>、match enum 専用、Sexpr=内部島 を反映。2026-07-22時点で全項目確認済み——`functions.md:279`の
  `gensym`は`()→Symbol`、`functions.md:97-105`で`Symbol`が`Sexpr`と別の独立プリミティブ型と明記、
  `functions.md:201-`で`Vector<T>`への一本化、`functions.md:58-`で`cons<T,U>`化、`language-design.md`
  各所で`Sexpr`が内部island層である旨、いずれも反映済みだった（本節作成時点＝Phase 0では未更新だったが、
  以降のPhase進行中に随時追記されていたため、着手時点では既に完了扱いにできる状態だった）。
- メモリ: 本再設計の要点を記録（[[typelisp-sexpr-rtvalue-unification]] の続きとして）。

---

## 承認済み計画の原本

`~/.claude/plans/symbol-sexpr-sexpr-temporal-hammock.md`（セッション外のためリポジトリには無い。本ドキュメントが同内容の in-repo 版）。
