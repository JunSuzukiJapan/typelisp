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
2. **compile 機能の残課題**（LLVM JIT/AOT コンパイラはコア機能。影響範囲基準で並べ替え済み
   ——[[feedback-impl-priority]]、詳細は[implementation-log.md](implementation-log.md)の
   「compile機能の残課題の一部対応」節参照）。`Expr::TraitCall`の実runtime dispatch対応・
   `compile-match`のGCルート漏れ修正・retain/release対の重複除去パス・`compile-let`の
   `unroot-let-sexpr-values`不正IRリスク修正は2026-07-01に対応済み。同日、
   `compile-loop`/`compile-break`/`compile-return`に一般的なGCルート巻き戻し機構
   （`loop-root-base` + `rt_truncate_sexpr_roots`）を追加し、`compile-match`の
   scrutinee・`compile-let`の`Sexpr`束縛のいずれも`break`/`return`早期脱出時に
   GCルートが漏れる問題を解消済み（詳細はimplementation-log.mdの該当節参照）。
   残っているのは:
   - Stage 7: 文字列対応（優先度低、計画上も最後に残されたステージ）。

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
