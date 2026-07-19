# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-19 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離、
以後も完了項目は都度こちらへ移設する）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

---

## 残っている作業

### 1. `symbol-sexpr-redesign.md` Phase 7（ドキュメント整備）— 要確認

[symbol-sexpr-redesign.md](symbol-sexpr-redesign.md) 上は「残るは Phase 7（ドキュメント整備）のみ」
という記載のままだが、実際に `docs/functions.md`/`docs/dev/language-design.md` を確認すると
`gensym` の型表記（`()→Symbol`）等 Phase 7 で直すはずだった箇所はすでに反映済みに見える。
記載自体が古い可能性が高い。次にこのドキュメントに触るときは:

- Phase 7 で挙げられている項目を1つずつ現状の `docs/functions.md`/`docs/syntax.md`/
  `docs/dev/language-design.md` と突き合わせ、すでに反映済みなら Phase 7 を完了扱いにする
- 本当に抜けている箇所があれば列挙して反映する

### 2. 次の作業候補は未定

2026-07-19 時点で `docs/dev/TODO.md` に記載されていた実装作業（クロージャ表現統一、
interp クロージャ完全削除、既知の制限7項目の解消など）はすべて完了し `main` にマージ済み
（[implementation-log.md](implementation-log.md) 参照）。次に着手する機能・改善は未指定。

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

`compile`（LLVM JIT/AOT）機能は現状「今のフェーズが実際に使う AST ノードだけ本実装、それ以外は
`ast_bridge` が `(unsupported "<Variant>")` を返しコンパイラ本体が明示的に panic する」設計
（`ast_bridge.rs` 冒頭のdocコメント参照）。ユーザーが普通に書けるコードから実際に到達しうる
`unsupported` は 2026-07-16 時点で解消済み——残る `Expr::TraitCall` は単型化後に到達不能な
診断専用ノードと確認済みで対象外（`tests/trait_test.rs` 参照）。
