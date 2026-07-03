# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-03 / ブランチ: `feature/compile-sexpr`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離した）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

---

## 残っている作業（影響範囲の大きさで優先順位付け——[[feedback-impl-priority]]）

1. **Sexpr/RtValue内部表現統合**（2026-07-02起案、進行中）——`Sexpr`（`crates/typelisp-mem::Value`）
   と`RtValue`（`src/eval/value.rs`）という2つの並行した実行時値表現に分かれているのは
   内部表現として不自然、というユーザー指摘を受けて着手。`defstruct`インスタンス
   （`Vector<T>`/`cons-cell<K,V>`含む）・クロージャ・`HashTable<K,V>`・`Scope<V>`を
   `Sexpr`側の`Value::Boxed`表現に統合し、LLVM builder等コンパイラ内部専用のFFIハンドルだけを
   `RtValue`に残す。設計・ステージ分割の詳細は
   [implementation-log.md](implementation-log.md)の「Sexpr/RtValue内部表現統合 実装計画」節参照。
   - **Stage 0（完了）**: `Value::Boxed`/`BoxedObj::Float`の骨組み、`TAG_FLOAT`→`TAG_BOXED`再利用。
   - **Stage 1（完了）**: `defstruct`/`Vector<T>`/`cons-cell<K,V>`が最終的に載る
     `BoxedObj::Struct`/`StructPayload::Fields`のmem/rt層プラミング
     （`Heap::alloc_struct`等、`rt_struct_new`/`rt_struct_field_get`/`rt_struct_field_set`）。
     まだ`RtValue::Struct`と並存するだけで、インタプリタ/コンパイラのどちらにも未配線。
   - **Stage 2（完了）**: Struct: インタプリタ結線——`RtValue::Struct`/`StructData`を削除し
     `Expr::Construct`/`FieldGet`/`FieldSet`を新表現（`RtValue::Sexpr`が包む
     `Value::Boxed`/`BoxedObj::Struct`）に接続。`Vector<T>`/`cons-cell<K,V>`も自動的に
     新表現に乗った。
   - **Stage 3（完了）**: Struct: コンパイラ結線——`mutable`フラグ分岐、
     `compile-construct-boxed-struct`新設。
   - **Stage 4（完了）**: HashTable: `StructPayload::Map`のmem層プラミング
     （`MemHashKey`、`Heap::alloc_hashtable`/`hashtable_get`/`_set`/`_remove`/`_count`/`_clear`、
     文字列キーの内容ベース等価性のための`Heap::intern_string`）。コンパイル対応は最初から
     スコープ外（HashTable/Scope/ClosureはJIT/AOT非対応のまま）なので`typelisp-rt`側の変更なし。
   - **Stage 5（完了）**: HashTable: インタプリタ結線——`RtValue::HashTable`/`HashKey`削除、
     `Vector<T>`と同じ「type_name駆動の`eval_builtin_method`分岐」パターンで
     `get/set/remove/count/clear/keys/values/entries`を再実装。
   - **Stage 6a-6b（未着手）**: 変数束縛スロットの`BoxedObj::Cell`化 → `RtValue::Closure`を
     `BoxedObj::Closure`に統合。
   - **Stage 7-8（未着手）**: `Scope<V>`を`BoxedObj::Struct`（`StructPayload::Frames`）に統合
     （自己ホスティングコンパイラ自体がScopeに依存するため最後に単独で着地）。

直近完了: Sexpr/RtValue内部表現統合Stage 5（2026-07-03）——HashTable: インタプリタ結線。
`src/eval/value.rs`から`RtValue::HashTable`/`HashKey`を削除し、`eval_builtin_method`の
`"hashtable"`アームを`Vector<T>`と同じ「type_name駆動の分岐」パターンで再実装
（`get`/`set`/`remove`/`count`/`clear`/`keys`/`values`/`entries`）。走査用に
`Heap::hashtable_pairs`をmem層に新設し、`decode_struct_field`の非struct-Boxed判定を
`is_hashtable`も見る3値判定に修正（放置するとヒープ常駐化した`HashTable`をfloatと誤読して
panicする潜在バグだった）。実装中に、typelisp自身で書かれた自己ホスティングコンパイラ
（`src/compiler.rs`）が`compile-let`のスクラッチアキュムレータ`acc`として
`HashTable<string,llvm-value>`を使い`RtValue::LlvmValue`を格納していたことが発覚
（`crate::mem::Value`はinkwell型を表現できないため`tests/compile_file_test.rs`の3テストが
壊れた）——`acc`を（`RtValue`をそのまま保持できる）単一フレームの`Scope<llvm-value>`に
置き換えて解決。全体テスト（`scripts/with-llvm-env.sh cargo test`、31クレート）green。
詳細は[implementation-log.md](implementation-log.md)の「Sexpr/RtValue内部表現統合」節参照。

その前に完了（いずれも詳細は[implementation-log.md](implementation-log.md)参照）:
Sexpr/RtValue内部表現統合Stage 4（HashTableのmem層プラミング、2026-07-03）／
「型が分からない」を誤った理由とする未対応箇所の一掃（2026-07-02）／
Sexpr/RtValue内部表現統合Stage 3（コンパイラ結線、2026-07-02）／
`HashTable<K,V>`への`Iter`実装（2026-07-02）／
compile機能（LLVM JIT/AOTコンパイラ）の残課題解消（2026-07-01時点で全完了）。

---

## 開発コマンド

```sh
cargo test                                   # 全体
cargo +nightly miri test --test mem_test     # GC/ポインタの UB・リーク検査
cargo +nightly miri test --test read_test
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --test numeric_test  # random が SystemTime を使うため isolation 解除が必要
cargo run                                    # REPL（typl、prelude 読み込み済み）
```

### compile 機能のビルド

`inkwell`（LLVM 17 バインディング）に依存するため `LLVM_SYS_170_PREFIX` が必要
（`brew install llvm@17` 済みが前提）。**このパスはマシンごとに異なるため、リポジトリ内の
どのファイルにも絶対パスをハードコードしない**——`scripts/with-llvm-env.sh` が
`brew --prefix llvm@17` で都度動的解決する:

```sh
scripts/with-llvm-env.sh cargo build
scripts/with-llvm-env.sh cargo test
```

`LLVM_SYS_170_PREFIX` を自分のシェルで既に export 済みなら、素の `cargo build`/`cargo test`
でも動く（このスクリプトは便宜上のラッパーであり必須ではない）。シェルの環境変数が古い
LLVMバージョンを指す等でシャドウされていると、素の `cargo` は `LLVMConstShl` 等の未定義
シンボルでリンクエラーになることがある——その場合は上記スクリプト経由で実行すること。
