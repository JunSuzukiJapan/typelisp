# typelisp 開発手順（ビルド・テスト）

最終更新: 2026-09-04 / ブランチ: `main`

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

## テストが遅くなったら、まず `target/debug/deps` の `.o` を数える

macOS の dev プロファイルは `split-debuginfo = "unpacked"` が既定で、rustc は
コード生成単位ごとの `.o` を成果物の隣に残し、リンク済みバイナリはそれを指すデバッグマップを
持つ（バックトレースの行番号はこれ）。cargo は**前の世代の `.o` を消さない**ので、
`target/debug/deps` はクレートをコンパイルするたびに 1 世代ぶん増え続ける。

ディスクを食うだけの話ではない。2026-09-04 の測定で、このディレクトリには `.o` が
**1,774,287 個（80 GB）**あり、**そこから exec すること自体**が次のコストだった:

| 同じ 51MB のテストバイナリを `--list`（テスト本体は 1 つも走らない） | 実時間 |
|---|---|
| `target/debug/deps/` から | 21〜40 秒（CPU は 0.03 秒。プロセスは `ps` で `UN` = 割り込み不能待ち） |
| `target/debug/` から（同じファイルシステム、小さいディレクトリ） | 0.03 秒 |

テストバイナリは全部このディレクトリから exec されるので、`scripts/test-serial.sh` は
バイナリ 1 本ごとにこれを払う。`editor_keyword_sync_test` が「40 秒かかるテスト」に
見えていたのはこれで、6 つのテスト本体の合計は 0.85 秒だった。**`cargo test` が報告する
`finished in ...` はテスト本体の時間だけなので、この差はテストの出力からは見えない。**

掃除:

```sh
scripts/clean-stale-objects.sh              # 直近 2 日分の .o を残して削除
scripts/clean-stale-objects.sh --dry-run    # 件数だけ数える
```

再ビルドは起きない（cargo の fingerprint はこれらのファイルを追跡していない）。
失うのは、消した世代の `.o` を指しているバイナリのバックトレース行番号だけ。

## コミット済みダンプ成果物の再生成

`src/compiler_island.typld`（自己ホストコンパイラ島）と
`crates/typelisp-front/src/prelude.typld`（事前コンパイルされた prelude）はどちらもコミット
されたバイナリ成果物で、対応する `SOURCE` を編集したら再生成が要る。番人テストが一致するまで
落ち続ける。1 ファイルに**検査済み状態とビットコードの両方**が入っている（[syntax.md](../ja/reference/syntax.md)
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

`inkwell`（LLVM 22 バインディング）に依存するため `LLVM_SYS_221_PREFIX` が必要
（`brew install llvm@22` 済みが前提）。**このパスはマシンごとに異なるため、リポジトリ内の
どのファイルにも絶対パスをハードコードしない**——`scripts/with-llvm-env.sh` が
`brew --prefix llvm@22` で都度動的解決する:

```sh
scripts/with-llvm-env.sh cargo build
scripts/with-llvm-env.sh cargo test
```

`scripts/setup-cargo-env.sh` を 1 度走らせれば、同じ値を `.cargo/config.toml`（git 管理外）に
書くので素の `cargo build`/`cargo test` で動く。`LLVM_SYS_221_PREFIX` を自分のシェルで既に
export 済みでも同じ（macOS では下の `MACOSX_DEPLOYMENT_TARGET` も設定するとよい）。
`cargo install --path .` も、インストールするパッケージ側の `.cargo/config.toml` を読む
（作業ディレクトリに関係なく。cargo 1.98 で確認）。

macOS では両スクリプトが `MACOSX_DEPLOYMENT_TARGET` も設定する。値は
`scripts/macos-deployment-target.sh` がツールチェーンの libstd のオブジェクトから読む
（Rust 1.98 の x86_64 では 15.0。`rustc --print deployment-target` の既定値 10.12 より高い）。
未設定だと rustc・`cc` クレート（rustls が使う ring の C/アセンブリ。既定は SDK の版）・
`compile-file` のリンク（Apple clang の既定）が別々の最低 OS 版を選び、ld が「より新しい
macOS 向けのオブジェクト」と警告する。動作は壊れない（macOS 15 + SDK 26.2 で、`typl` も
`typl -c` の実行ファイルも TLS 込みで動いた）ので、build.rs は未設定でもビルドを止めない。
未設定なら build.rs が libstd から同じ値を読み、内蔵ライブラリを作る内側の cargo と
`compile-file` のリンクに渡す——これで `typl -c` の警告（ring のオブジェクトごとに 1 行、
27 行）は消える。外側のビルドはもう走っているので `typl` 自身のリンクだけは食い違うが、
cargo はリンク警告を表示しない。
**Rust のツールチェーンを上げたら `scripts/setup-cargo-env.sh` を走らせ直す。**
libstd が見つからない・オブジェクトの版が 1 つに揃わない・版が読めない場合はスクリプトが
理由と対処法を出して止まる（黙って別の値を選ばない）。値を自分で決めて進めるなら
`scripts/setup-cargo-env.sh --deployment-target <version>`、または
`MACOSX_DEPLOYMENT_TARGET=<version> scripts/with-llvm-env.sh cargo ...`（export 済みの値は尊重する）。シェルの環境変数が古い
LLVMバージョンを指す等でシャドウされていると、素の `cargo` は `LLVMConstShl` 等の未定義
シンボルでリンクエラーになることがある——その場合は上記スクリプト経由で実行すること。

## 配布用の実行ファイル（macOS）

`install.sh`（リポジトリのルート）が GitHub のリリースから取ってくる `typl` / `typl-lsp` は、
CPU ごとの tar.gz と、その SHA-256 のファイル。Homebrew の LLVM は自分の CPU 向けしか入って
いないので、各 CPU 向けのビルドはそれぞれの Mac で行う。

1. Intel の Mac と Apple Silicon の Mac で、それぞれ `scripts/dist/build.sh` を実行する。
   `target/dist/typelisp-<version>-<arch>.tar.gz` と `.tar.gz.sha256` ができる。
2. 4 つのファイルを `gh release upload v<version> ...` でリリースに添付する。

`install.sh` はアセットの名前を `typelisp-<version>-<arch>.tar.gz`（`arch` は `x86_64` か
`arm64`）と決め打ちしているので、名前を変えるなら両方を直す。

`build.sh` が普通のリリースビルドと違うのは 2 点。zstd を静的にリンクする（Homebrew の LLVM は
zstd 付きでビルドされていて、llvm-sys は `-lzstd` を渡す。`libzstd.a` だけを置いたフォルダを先に
探させると、リンカは Homebrew の dylib でなくそちらを取る）。最低 macOS バージョンを、std・
Homebrew の LLVM・Homebrew の zstd のうち最も高いものにする。リンク先が `/usr/lib` と
`/System/Library` 以外にあればそこで止まる。

Developer ID での署名はしない。curl でダウンロードしたファイルには quarantine 属性が付かず、
Gatekeeper は見ない。Apple Silicon の実行ファイルに要る署名は、リンカが付ける ad-hoc 署名で
足りる。ブラウザでダウンロードした tar.gz から取り出した typl は Gatekeeper に止められる。
署名と公証をするなら hardened runtime が要り、typl には JIT 用の
`com.apple.security.cs.allow-unsigned-executable-memory`（無いと最初の JIT でカーネルに SIGKILL
される）と FFI 用の `com.apple.security.cs.disable-library-validation`（無いと `dlopen` が失敗
する）が要る。どちらも外して確かめた。そのためのスクリプトは一度作って消した（コミット 9955789）。

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
