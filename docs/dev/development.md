# typelisp 開発手順（ビルド・テスト）

最終更新: 2026-07-29 / ブランチ: `main`

ビルドとテストの実行方法。2026-07-29 に [TODO.md](TODO.md) から分離した（TODO.md は残作業のみを
記録するドキュメントで、残作業が無くなったため）。

## 開発コマンド

```sh
cargo test                                   # 全体
cargo +nightly miri test --test mem_test     # GC/ポインタの UB・リーク検査
cargo +nightly miri test --test read_test
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --test numeric_test  # random が SystemTime を使うため isolation 解除が必要
cargo run                                    # REPL（typl、prelude 読み込み済み）
```

## compile 機能のビルド

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

## compile 機能の実装方針（未対応ノードの扱い）

`compile`（LLVM JIT/AOT）機能は現状「今のフェーズが実際に使う AST ノードだけ本実装、それ以外は
`ast_bridge` が `(unsupported "<Variant>")` を返しコンパイラ本体が明示的に panic する」設計
（`ast_bridge.rs` 冒頭のdocコメント参照）。ユーザーが普通に書けるコードから実際に到達しうる
`unsupported` は 2026-07-16 時点で解消済み（2026-07-25 に追加した `Expr::DynBox`/`DynCall`/
`DynValue` も同日中に本実装した）——残る `Expr::TraitCall` は単型化後に到達不能な
診断専用ノードと確認済みで対象外（`tests/trait_test.rs` 参照）。
