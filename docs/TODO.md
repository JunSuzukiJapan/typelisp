# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-02 / ブランチ: `feature/compile-sexpr`

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
   - **Stage 2-3（未着手）**: Struct: インタプリタ結線 → コンパイラ結線の2段階。
   - **Stage 4-5（未着手）**: `HashTable<K,V>`を`BoxedObj::Struct`（`StructPayload::Map`）に統合。
   - **Stage 6a-6b（未着手）**: 変数束縛スロットの`BoxedObj::Cell`化 → `RtValue::Closure`を
     `BoxedObj::Closure`に統合。
   - **Stage 7-8（未着手）**: `Scope<V>`を`BoxedObj::Struct`（`StructPayload::Frames`）に統合
     （自己ホスティングコンパイラ自体がScopeに依存するため最後に単独で着地）。

直近完了: Sexpr/RtValue内部表現統合Stage 1（2026-07-02）——`BoxedObj::Struct`/
`StructPayload::Fields`のmem/rt層プラミング（`Heap::alloc_struct`等、
`rt_struct_new`/`rt_struct_field_get`/`rt_struct_field_set`）。インタプリタ/コンパイラは
まだ無配線（Stage 2-3）、詳細は[implementation-log.md](implementation-log.md)参照。

その前に完了: `HashTable<K,V>`への`Iter`実装（2026-07-02）——`keys`/`values`/`entries`
（Rust builtin、`registry::hashtable_def`/`eval_builtin_method`）を新設し、`entries`が返す
`Vector<cons-cell<K,V>>`（`cons-cell<A,B>`は`prelude.rs`の汎用`car`/`cdr`構造体）を
`hashtable-iter<K,V>`が`vector-iter<T>`と同じカーソル走査で辿る形で`Iter`を実装。`HashMap`に
安定した再開可能カーソルがないため、スナップショット方式（呼び出し時点のコピー）。`Sexpr`は
意図的に`Iter`非対応のまま——各`cons`セルの`car`が独立に動的型付けされるため`Iter`が要求する
「1つの`Item`型」を正しく宣言できない。詳細は[implementation-log.md](implementation-log.md)参照。

compile 機能（LLVM JIT/AOT コンパイラ）の残課題は2026-07-01時点で全て解消済み——
Stage 7（文字列対応、計画上最後に残されていたステージ）を含め、詳細は
[implementation-log.md](implementation-log.md)の該当節を参照。

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
