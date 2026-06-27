# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-06-27 / ブランチ: `feature/compile-sexpr`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離した）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

---

## 残っている作業（影響範囲の大きさで優先順位付け——[[feedback-impl-priority]]）

1. **`doiter`**（特殊形・checker拡張）—「何に対する反復か」（イテレータ抽象 or リスト限定）の
   仕様が未確定（[cl-equivalence-catalog.md](cl-equivalence-catalog.md) §1.1）。実装方針より
   先に**ユーザとの仕様確認**が必要なため、このドキュメント単独では着手できない。
2. **可視性**（`FnSig::public` 等のフィールドは既に存在するが常に `true` で実効性なし）。
   将来 compile 機能の共有ライブラリ化が `pub` を export 基準に使う計画があるが、現状は何も
   ブロックしていないため優先度は低いまま。
3. **compile 機能の残課題**（LLVM JIT/AOT コンパイラという実験的サブシステム内の技術的負債。
   優先順位はユーザー未確認——コア言語機能ではないため上記2項目より下位）:
   - Stage 7: 文字列対応（優先度低、計画上も最後に残されたステージ）。
   - 一般ADT箱（`Construct`/`FieldGet`/`FieldSet`）の**フィールド自体**のGCルート保護
     （`Cons`構築/呼び出し引数は対応済み——フィールドは`Expr::Construct`のwire形式が型情報を
     運んでいないため、まず拡張が必要な一段大きい変更）。
   - `compile-if-branch`経由の値（`if`/`return`/`setf`/`match`アーム）のkind対応——現時点では
     具体的な破壊を実証するテストが書けていない（書けたら優先度を上げる）。
   - `compile-assoc`がユーザー定義メソッド呼び出しを認識しない——コンパイル済みコードの中から
     別のcompile済みメソッドを呼べない（`p::x`等をボディに含む`defun`自体をcompileできない）。
   - retain/release対の重複除去（Swift ARC Optimizer的な最適化パス）。正しさは確認済みで
     パフォーマンスチューニングのみ、後回し。
   - ネストした`labels`が外側labelsの兄弟を参照するケース（既存の "known limitation"、
     `compile-labels`は常に`inner-fn-env`を空から始める設計）。
   - `break`/`return`を文の位置以外（算術オペランド/呼び出し引数の中）でも許容する——型レベル
     では合法だが未対応。
   - mark-and-sweepによるサイクル収集本体——既存の`ClosureBox`設計では真の参照循環がそもそも
     構築不可能と判明済みのため、収集すべき対象が無く緊急性は低い。

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
