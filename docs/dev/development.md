# typelisp 開発手順（ビルド・テスト）

最終更新: 2026-08-14 / ブランチ: `feature/compile-strict-names-and-prelude-bitcode`

ビルドとテストの実行方法。2026-07-29 に [TODO.md](TODO.md) から分離した（TODO.md は残作業のみを
記録するドキュメントで、残作業が無くなったため）。

## 開発コマンド

```sh
cargo test                                   # 全体
cargo +nightly miri test --test mem_test     # GC/ポインタの UB・リーク検査（約 50 秒）
cargo +nightly miri test --test read_test    # 約 4 分
cargo run                                    # REPL（typl、prelude 読み込み済み）
```

### Miri で回せるのは prelude を読まないテストだけ

`mem_test`/`read_test` はどちらも `load_prelude` を呼ばない。**prelude をロードする
テストは Miri では完走しない。**

- ここには `MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --test numeric_test`
  が並んでいたが、**走らないコマンドだった**。この行は `numeric_test` 新設時
  （`8c8b812`、2026-06-18）に「20/20 green」と一緒に書かれたもので、当時の
  `numeric_test` は prelude を読んでいなかった（`src/prelude.rs` はその日に生まれた
  ばかりで、テストは `Heap` を直接組み立てていた）。数値ヘルパを prelude の typelisp
  メソッドへ移した `faee845`（2026-07-22）で `load_prelude` を呼ぶようになり、以後
  一度も Miri で走らせていない。2026-08-13 に実際に回して確認: **1 テストだけに絞っても
  21 時間で prelude の「読み込み」すら終わらない**（ネイティブでは同じ 1 テストが 0.20 秒、
  バイナリ全体 33 件でも 0.05 秒）。
- 原因は cons セル化ではない。Phase 1c（`895aec5`）で同じ 1 テストを回しても 50 分で
  同じく `load_prelude` の中にいる。prelude が 2,383 行に育って以降ずっとこうだった
  というだけで、退行ではない。
- Miri の進捗バックトレース（`-Zmiri-report-progress=50000000`）が指すのは
  `Heap::intern_loc` ← `Heap::set_elem_loc` ← `reader::read_list`。アルゴリズムの
  問題ではなく（`intern_loc` は `HashMap` 引きで O(1)）、生ポインタの cons アリーナへの
  書き込み 1 回ごとに Miri が Stacked Borrows の provenance 検査をするコストが、
  prelude ぶんの回数だけ掛かる。
- したがって Miri は**メモリ層の検査装置**として使う。GC・生ポインタ・アリーナの UB は
  `mem_test` が見ており、そこが Miri を必要とする唯一の場所でもある。prelude を通す
  経路の検査が要るなら、`gc_stress`（`Heap::set_gc_stress` — ルート漏れの検出装置）
  の方が実用的。

なお Miri はコミット前の必須手順ではない（通常の `cargo test` で十分、という既存合意）。

## コミット済みダンプ成果物の再生成

`src/compiler_island.typld`（自己ホストコンパイラ島）と
`crates/typelisp-front/src/prelude.typld`（事前コンパイルされた prelude）はどちらもコミット
されたバイナリ成果物で、対応する `SOURCE` を編集したら再生成が要る。番人テストが一致するまで
落ち続ける。1 ファイルに**検査済み状態とビットコードの両方**が入っている（[syntax.md](../syntax.md)
§10「ダンプ」）。

```sh
scripts/regen-compiler-island.sh    # compiler.rs の SOURCE を変えたら
scripts/regen-prelude-bitcode.sh    # prelude.rs の SOURCE を変えたら
```

**両方要るときは島が先。** prelude は島によってコンパイルされるので、島を再生成すると
prelude の成果物も変わる（逆向きの依存は無い——島の生成器は prelude を interpreted で読む）。

`llvm-*` ビルダ（`eval_llvm_builtin_method`）やコンパイル・ブリッジを変えた場合も再生成が要る:
出力 IR が変わるのに `SOURCE` は変わらないので、ダイジェストを見る
`*_artifacts_are_fresh` は気付かない。バイト比較する
`the_committed_*_matches_a_fresh_build` の方が落ちる。

```sh
scripts/bench-prelude.sh            # 事前コンパイル済み prelude の効果を測る（release）
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

**この節は無効になった。** `compile`（LLVM JIT/AOT）は「今のフェーズが使う AST ノードだけ
本実装、残りは `ast_bridge` が `(unsupported "<Variant>")` を返して panic」という設計だったが、
cons セル化 Phase 2 Stage C（`9221319`）で `src/compile/ast_bridge.rs` ごと削除された。
checker が core IR を直接吐くようになり、bridge（`src/compile/core_bridge.rs`）は
core→島 IR の cons 変換だけを行う。`unsupported` という語彙自体が無い（`grep` で 0 件）。

未対応ノードという概念が消えたわけではなく、置き場所が変わった: 語彙に無いタグは
`core_bridge` がその場でエラーにする。`Expr::TraitCall`（単型化後は到達不能な診断専用
ノードだった）は core 形では `(panic (str ..))` に落ちる——消費者 2 つがどちらも
「実行されてはならない」としか言っていなかったので、それを語彙で表した。
