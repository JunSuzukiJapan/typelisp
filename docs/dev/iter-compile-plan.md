# Iter トレイトを持つ全型の compile 対応 — 段階計画

起案: 2026-07-13 / ブランチ: `feature/iter-compile`

> **状況（2026-07-13）**: Stage A〜E 全完了。Vector/HashTable 反復が compile 可能になり、
> ユーザー定義 Iter 型も推移的自動 compile で「タダで」通る。当初「反復とは別問題」として
> 意図的に対象外としていた `HashTable::get`/`remove`（`Option` 返し）も同日中に追加解消 —
> `rt_hashtable_contains`/`_get_raw`/`_remove_raw` + 実行時分岐（`compile-if`と同型のalloca+
> 分岐+merge、phiビルトインなし）で対応。
>
> **追記（2026-09-03）**: この 3 本のシムと、それを使う `compile-hashtable-op` の
> 制御フローは**もう無い**。`HashTable` の `get`/`set`/`remove` は prelude の
> `defmethod` になり（キーをハッシュすることも 2 つのキーを比べることもキーの型自身の
> typelisp メソッドなので、Rust のシムからは呼べない）、`Option` は prelude が作る。
> 下の層に残ったのは `bucket-*` 5 本。以下の記述は起案・完了当時のもの。

## 背景・目的（起案時点、2026-07-13）

`compile`（LLVM JIT/AOT）は起案時点で、ユーザーが普通に書けるコードから到達しうる `unsupported` は
解消済み（TODO.md）だが、**Iter トレイトを介したコレクション反復**（`doiter` / `map`/`filter`/
`member`/`foldl` 等のコンビネータ）は compile できなかった。これは `docs/dev/TODO.md` で「将来課題
（Phase 6.6 対象外）」として明示的に見送られていた項目。本計画はこれを解消する。

> 上記は起案時点の状態描写であり現状ではない。冒頭の状況ノート通り Stage A〜E 全完了済み。

### 誤解しやすい点: トレイトディスパッチは compile のブロッカーではない

`(next it)` は**単型化後に具体メソッド呼び出しに解決**され、通常のユーザー `defmethod` と同じ
`compile-assoc-user`（マングル名呼び出し）で compile される。compile 側の `TraitCall` 機構は
「単型化後に到達不能」として 2026-07-04 に削除済み（[[typelisp-compile-traitcall-arc-match-gc-fix]]）。

➡️ **ユーザー定義 Iter 型は型ごとの専用 compile 対応が不要**。その `next` は具体メソッドとして
compile される。したがって本課題の本質は「底のプリミティブ層を compile 可能にする」こと。

### Iter を持つ型は prelude に 2 つだけ

- `vector-iter<T>`（`Vector<T>` の反復子。`next` は `len`/`get` on Vector を使う）
- `hashtable-iter<K,V>`（`HashTable<K,V>` の反復子。**内部で Vector snapshot 上を歩く** ——
  `next` は `len`/`get` on Vector を使い、HashTable 自体には触らない。`iter` メソッドが
  `(entries self)` で snapshot Vector を作る所だけが HashTable プリミティブ）
- `Sexpr` は意図的に非 Iter。

## 既に compile 可能なもの（確認済み ✓）

- defstruct / `Vector<T>` / `cons-cell<K,V>` の**構築**（`translate_construct` の mutable 分岐 →
  `rt_struct_new`）
- boxed-struct フィールド get/set（`rt_struct_field_get`/`rt_struct_field_set`）と Match（struct-kind、
  2026-07-12）
- `Option::some`/`none` 構築・`Option` の match（sum-ADT box、MATCH_KIND_BOX）
- ヒープクロージャ（`map`/`filter` の `f`/`pred` 引数、Stage 6b）
- i32/string/char の `Eq`/`Ord`（`member`/`sort` 用、2026-07-09）
- ユーザー定義メソッドのマングル名分岐（`compile-assoc-user`）

## 不足（本計画の対象）— 2 層

### Vector 層（基盤。hashtable-iter も Vector を使うため最優先）
- `len` → `Heap::struct_field_count` を露出する `rt_struct_field_count`（**新設**、typelisp-rt）
- `push` → `Heap::struct_push_field` を露出する `rt_struct_push_field`（**新設**、typelisp-rt）
- `get`/`set` → 既存 `rt_struct_field_get`/`rt_struct_field_set` を再利用。ただし
  **要素型 T の kind によるタグ付け（push/set）/デコード（get）が必要**。compiled scalar は
  生の機械値、boxed-struct フィールドはタグ付き `Value` なので、`compile-tag-struct-field`
  （encode）/`compile-sexpr-field`（decode、Match struct-kind で使用中）を要素型に適用する。
- `compile-assoc` に Vector レシーバ分岐（`vector-native-method?` 述語 + 各メソッド実装）
- **要素型 kind の配線**: 現状 `ast_bridge` の `Expr::Assoc` 変換は戻り型/要素型を wire form に
  載せていない。Match struct-kind が `Pattern::Ctor` に `field_types`/`field_kinds` を足したのと
  同型で、Assoc の Vector メソッドに要素 kind を載せる。

### HashTable 層（compile に未配線の 1 サブシステム。ast_bridge「planned to follow」）
- 構築・`get`/`set`/`remove`/`count`/`clear`（`rt_hashtable_*` は**一つも存在しない**）
- `keys`/`values`/`entries`（`Heap::hashtable_pairs` を読んで boxed Vector を組む rt 関数）
- `cons-cell<K,V>`（entries が生成、car/cdr = 構造体フィールドアクセスで既に compile 可）
- HashTable の match / construct は現状 `unsupported`

要素型 kind のタグ付け/デコードは Vector 同様、HashTable の get/keys/values/entries にも必要。

## 段階（依存順に land + test）

- **Stage A ✅ — Vector 層**: `rt_struct_field_count`/`rt_struct_push_field` 新設（typelisp-rt 単体
  テスト2件）→ `rt_extern_functions()` 登録 → `ast_bridge` の `vector-op` ノード
  （`translate_vector_method`、`{new,get,set,len,push}` を receiver 型で検出、`iter` は除外）+
  要素 kind 配線 + `compiler.rs` の `compile-vector-op`（`new` は既存 `compile-construct-boxed-struct`
  再利用、get/set は**実行時**インデックス + kind タグ/デコード）+ `compile_function` の
  method-target 検証から Vector builtin を除外。テスト: `compile_test` 5件（push/get/set/len/
  passthrough文字列要素/JIT-interp 一致）。
- **Stage B ✅ — 推移的自動 compile ドライバ**: 当初「Vector 上のコンビネータ検証」の予定だったが、
  `(compile fn)` が呼ぶ**単型化インスタンス**（`vector::iter <i64>` 等、空白マングル名で `(compile)`
  名指し不可、body は `self.methods`/`self.fns` に既存）を「先に compile しろ」と要求して失敗すると
  判明。`Interp::compile_function_rec` を新設し、未 compile の具体インスタンスを**推移的に自動
  compile**（`in_progress` 循環ガード付き、前方参照禁止言語なので相互再帰は実質到達不能）。
  これで `map`/`member`/`doiter` over `Vector<i32>` が end-to-end で通る。旧「callee は先に compile
  必須」を検証していた既存テスト3件は新挙動（自動 compile 成功）に書き換え。テスト: `compile_test` 3件。
- **Stage C ✅ — HashTable 層**: `rt_hashtable_new`/`set`/`count`/`clear`/`keys`/`values`/`entries`
  新設（typelisp-rt、キーのハッシュ化は mem 層が担当、`entries` は cons-cell を GC ルート保護しつつ
  Vector 化）+ `rt_extern_functions()` 登録 + `ast_bridge` の `hashtable-op` ノード
  （`translate_hashtable_method`、key/val 2 kind）+ `compiler.rs` の `compile-hashtable-op` +
  method-target 検証除外。テスト: typelisp-rt 単体1件。
- **Stage D ✅ — HashTable 反復 end-to-end**: `doiter`/`count`/`keys` over `HashTable<i64,i64>` が
  compile 通ることを検証（`hashtable-iter::next` は `entries` の Vector snapshot を Stage A の
  `vector-op` で歩く）。テスト: `compile_test` 2件。
- **Stage C 追補 ✅ — `HashTable::get`/`remove`（`Option` 返し）**: 当初「反復とは別問題」として
  対象外にしていたが同日中に解消。`get`/`remove` は**実行時**の found/not-found 結果で
  `Some`/`None` どちらの variant を構築するか決まるため、`Option::some`/`none` の通常コンパイル
  （`compile-construct-box`、コンパイル時定数variant）を単純に再利用できない。新設した
  `rt_hashtable_contains`（0/1）+ `rt_hashtable_get_raw`/`_remove_raw`（存在確認済み前提、
  タグ付き値を返す）を、`compile-if`と同型の「allocaでmergeスロット確保→分岐→各腕で結果を
  store→merge後にload」（phiビルトインが無いための代替パターン）で呼び分け、見つかった値は
  `compile-sexpr-field`（既存のstruct-fieldデコードをそのまま再利用——box内の値表現規約は
  struct-fieldのそれと一致）でデコードし、`Some`/`None`ボックスを直接組み立てる。テスト:
  `compile_test` 4件（found/absent/passthrough文字列/remove）。
- **Stage E — docs & memory**: `functions.md`/`language-design.md` 更新、TODO.md の将来課題項を解消、
  メモリ記録。

### 達成した設計上の要点
- **トレイトディスパッチは compile 不要**（単型化で消える）。ユーザー定義 Iter 型は、その `next` が
  compile 可能なプリミティブ（Vector/HashTable/scalar）に落ちる限り**専用対応ゼロで compile される**。
- 本課題の実体は (1) コレクション・プリミティブ層（vector-op/hashtable-op）と
  (2) 単型化インスタンスの推移的 compile ドライバ、の 2 つだった。
- `Option` 返しメソッドの compile は、`compile-if`の「alloca+分岐+merge」パターンを実行時分岐に
  再利用すれば、`compile-construct-box`（コンパイル時定数variant専用）を作り直さずに済む。

## ビルド/テスト

```sh
scripts/with-llvm-env.sh cargo test          # compile 関連は LLVM 環境必須
scripts/test-serial.sh                        # LLVM テストのプロセス間並列は SIGSEGV 回避に直列化
```

## 承認

段階ごとに land + test。Stage A から着手。
