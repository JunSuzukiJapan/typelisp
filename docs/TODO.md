# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-01 / ブランチ: `feature/compile-sexpr`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離した）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

---

## 残っている作業（影響範囲の大きさで優先順位付け——[[feedback-impl-priority]]）

1. **`Sexpr`/`HashTable<K,V>`への`Iter`実装** — `doiter`は意図的に`Vector<T>`のみで動作確認した
   （`Sexpr`は要素型が固定されないため`Iter<Item>`を実装すべきでない、というユーザー判断
   ——詳細は[implementation-log.md](implementation-log.md)の「trait機構+doiter」節参照）。
   `HashTable<K,V>`への走査API（keys/values/entries相当）自体が未実装。

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
