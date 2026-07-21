# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-22 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離、
以後も完了項目は都度こちらへ移設する）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

---

## 残っている作業

2026-07-21 時点で行われていたモジュール可視性の祖先チェーン方式への再設計・Interpのスコープ
ツリー全面移行・それに伴う重複コード整理（[implementation-log.md](implementation-log.md) 参照）は
いずれも全テストgreenで `main` にマージ済み。[symbol-sexpr-redesign.md](symbol-sexpr-redesign.md)
の Phase 7（ドキュメント整備）も `docs/functions.md`/`docs/syntax.md`/`docs/dev/language-design.md`
との突き合わせを完了し、完了扱いにした（2026-07-22）。

現時点で着手中・積み残しの実装作業はなし。次に着手する機能・改善は未指定。

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
