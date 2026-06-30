# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-06-30 / ブランチ: `feature/compile-sexpr`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離した）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

---

## 残っている作業（影響範囲の大きさで優先順位付け——[[feedback-impl-priority]]）

1. **`Sexpr`/`HashTable<K,V>`への`Iter`実装** — `doiter`は意図的に`Vector<T>`のみで動作確認した
   （`Sexpr`は要素型が固定されないため`Iter<Item>`を実装すべきでない、というユーザー判断
   ——詳細は[implementation-log.md](implementation-log.md)の「trait機構+doiter」節参照）。
   `HashTable<K,V>`への走査API（keys/values/entries相当）自体が未実装。
2. **compile 機能の残課題**（LLVM JIT/AOT コンパイラという実験的サブシステム内の技術的負債。
   優先順位はユーザー未確認——コア言語機能ではないため上記項目より下位。影響範囲基準で並べ替え済み
   ——[[feedback-impl-priority]]、詳細は[implementation-log.md](implementation-log.md)の
   「compile機能の残課題の一部対応」節参照）。`Expr::TraitCall`はcompile機能では`unsupported`の
   プレースホルダのまま（`src/compile/ast_bridge.rs`）——`Iter`/`doiter`を使うコードは現状compile
   できない:
   - `compile-if-branch`経由の値のうち、`if`/`match`自身のmerge slot・`return`の
     loop-slotを素通りする**束縛されない一時値**のkind対応——store直後にloadする
     隣接命令なので単体では安全、かつ呼び出し引数・Cons構築・一般ADTフィールド
     経由の消費先は既に対応済みのため、具体的な破壊を実証するテストはまだ書けて
     いない（書けたら優先度を上げる）。`setf`によるkind=2束縛の**再代入**自体は
     2026-06-30に対応済み——`compile-set`が`rt_set_sexpr_root`で再代入後の値の
     GCルートを更新する（再代入前は値のスナップショットを積んだまま動かない
     既存rootが新しい値を一切保護しておらず、`typelisp-rt`の
     `a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root`
     で実証済み）。
   - Stage 7: 文字列対応（優先度低、計画上も最後に残されたステージ）。
   - retain/release対の重複除去（Swift ARC Optimizer的な最適化パス）。正しさは確認済みで
     パフォーマンスチューニングのみ、後回し。

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
