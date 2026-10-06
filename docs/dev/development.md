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
scripts/clean-stale-objects.sh              # 直近 2 日分を残して削除
scripts/clean-stale-objects.sh --dry-run    # 件数と大きさだけ数える
```

`.o` を消しても再ビルドは起きない（cargo の fingerprint はこれらのファイルを追跡していない）。
失うのは、消した世代の `.o` を指しているバイナリのバックトレース行番号だけ。

同じスクリプトが、古い実行ファイルも消す。テストバイナリと `[[bin]]` は `deps` に
`<名前>-<16 桁のハッシュ>` で置かれ、コードや機能フラグが変わるたびに新しいハッシュで
増えていく（テストバイナリは 1 本 100MB を超える）。同じ名前でより新しいものがあり、
かつ 2 日より古いものを `.d` ごと消す。cargo は無くなったものを次のビルドで作り直すので、
壊れることはない。2 日の条件は、機能フラグ違いで並んで使われている 2 つ
（`--features dev-tools` の有無など）を作り直させないためにある。

`deps` を入れ替えるので、`cargo` やテストが走っている間は実行しない。

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

## ランタイムライブラリと ABI バージョン

`typl -c` が作る実行ファイルは、`typelisp-front` の staticlib（`libtypelisp_front.a`。rt・print・
read・mem・abi と、`eval` 用のチェッカーとインタプリタを含む）をリンクする。どこのものを
リンクするかはビルドの仕方で決まる。

- debug ビルド: `target/debug/libtypelisp_front.a`。
- release ビルド: build.rs が内側の cargo でもう一度ビルドしたものを `typl` に内蔵し、
  最初のリンクのときに `$TYPELISP_HOME/lib/<ビルドID>/` へ書き出す。
- `TYPELISP_LINK_TREE_RUNTIME=1` を付けた release ビルド: 内側の cargo を走らせず、
  `target/release/libtypelisp_front.a` をリンクする。開発中に release ビルドを繰り返すとき、
  ランタイムを 2 回ビルドする待ちを省くためのもの。こうして作った `typl` はツリーの外では
  使えないので、インストールしない（ビルド時に cargo が警告を出す）。

```sh
TYPELISP_LINK_TREE_RUNTIME=1 scripts/with-llvm-env.sh cargo build --release
```

**ABI バージョン。** 生成コードがアーカイブについて仮定していること（`rt_*` シンボルと型、
`typelisp-abi`・`typelisp-mem` の定数、well-known シンボルの番号、組み込み型の変種の順序と
シグネチャ、型ごとの表現、ダンプ形式の版など）を `compile::abi_signature::describe` が
書き出す。版は `MAJOR.MINOR.PATCH` で、MAJOR.MINOR は typelisp 本体の版のものを使い、
PATCH はその下で記述が変わるたびに 1 つ上げる。本体の MAJOR.MINOR が上がったら、記述が
変わっていなくても、新しい MAJOR.MINOR の PATCH 0 に移る。たとえば本体 0.1.x に対して
ABI は 0.1.y で、x と y は一致しなくてよい。アーカイブは `typelisp_abi_v<MAJOR>_<MINOR>_<PATCH>`
を定義し、生成コードの `main` がそれを参照するので、版の違うアーカイブはリンクの時点で断られる。

記述は `docs/dev/api_version/` に置く。

- `latest_api_signature.md`: 最新の版。
- `history/api_<MAJOR.MINOR.PATCH>.md`: 版ごとの記述。最新の版のものも含む。**削除も編集もしない。**

`abi_version_test` が、記述が最新の版と一致しないとき、版の MAJOR.MINOR が本体のものと
違うとき、履歴が欠けたり書き換えられたりしているとき、共有される定数が記述から漏れているときに
落ちる。記述か本体の MAJOR.MINOR が変わったら次を走らせる。履歴に次の版を書き、
`latest_api_signature.md` をその写しにし、`typelisp-abi` の `with_abi_version!` を書き換える。

```sh
scripts/regen-abi-version.sh          # 記述か本体の MAJOR.MINOR が変わったときだけ新しい版を書く
scripts/regen-abi-version.sh --bump   # 記述に現れない変更のために版を上げる
```

記述に現れないもの（シムが引数をどう扱うか、生成コードがヒープを直接読む箇所など）を、
ランタイムと合わせて変えたときは `--bump` を使う。

## リリースの手順

GitHub のリリース（macOS の配布用実行ファイル）は `.github/workflows/release.yml` が作る。
crates.io への公開は手で行う。

1. 版を上げる。全クレートの版はルートの `Cargo.toml` の `[workspace.package]` の `version` 1 つ。
   同じファイルの `[workspace.dependencies]` にある内部クレートの `version` と、
   `tests/typl_help_version_test.rs` の期待値も合わせて直し、`Cargo.lock` と一緒にコミットする。
2. main に入れたら、Actions の Release を手で起動する（`gh workflow run release.yml --ref main`）。
   手で起動したときは、全環境のテストと配布物のビルドまでで止まり、何も公開しない。テストは
   環境ごとに 4 つに分けて走り、数時間かかる。
3. `cargo publish --dry-run` を、下の 5. の順に通す。
4. タグ `v<version>` を main に打って push する。release.yml が、タグと `[workspace.package]` の
   版が合うことを確かめ、2. と同じテストとビルドをして、リリースを下書きで作って両 CPU の
   tar.gz と SHA-256 を添付し、公開し、Intel と Apple Silicon の runner で `install.sh` から
   入れて版・JIT・AOT を確かめる。リリースノートは `--generate-notes` が作るコミットの一覧なので、
   書き直すなら公開後に `gh release edit` で差し替える。
5. crates.io に、依存される側から順に公開する: typelisp-mem → typelisp-abi → typelisp-print →
   typelisp-read → typelisp-rt → typelisp-front → typelisp。前のクレートが crates.io の索引に
   載る前に次を出すと、依存が見つからずに止まる。

## 配布用の実行ファイル（macOS）

`install.sh`（リポジトリのルート）が GitHub のリリースから取ってくる `typl` / `typl-lsp` は、
CPU ごとの tar.gz と、その SHA-256 のファイル。`scripts/dist/build.sh` が、その CPU の Mac で作る
（release.yml では `macos-26`（Apple Silicon）と `macos-15-intel` の runner）。LLVM は CPU によって
出どころが違う: Apple Silicon は LLVM 公式のビルド済み配布物（`build.sh` がダウンロードする）、
Intel は Homebrew の llvm@22（LLVM に Intel の Mac 向けの配布物が無い）。

- `build.sh` が最初に表示する `minimum macOS:` の行が、その CPU 向けの `typl` が動く最も古い
  macOS。Apple Silicon では LLVM 公式の配布物が対象とする macOS（22.1.8 では 14.0）になる。
  ただし Apple Silicon でサポートするのは macOS 26 以降で、それより前では `typl` が起動時に
  警告する（JIT のコードで landing pad に入ると、システムの unwinder が落ちる）。
  Intel では、Homebrew の LLVM や zstd がその Mac の macOS 向けにビルドされたものだと、ここが
  上がる。
- `build.sh` は依存ライブラリが OS のものだけであることと、JIT と AOT が動くことを確かめ、
  そうでなければ止まる。
- Apple Silicon の初回は、LLVM 公式の配布物（約 1.4GB）のダウンロードと変換、zstd のビルドで
  数分かかる。結果は `target/dist/cache`（約 420MB）に残り、2 回目からは使い回す（release.yml
  では `build.sh` のハッシュをキーにキャッシュする）。作り直すにはそのフォルダを消す。

### 手元の Mac で作る

公開済みのリリースのファイルだけを差し替えるときなど、release.yml を通さずに作る場合。

初めて使う Mac での準備（一度だけ）:

1. Xcode Command Line Tools を入れる（`xcode-select --install`）。`build.sh` が使う `otool`・`ar`
   と、確認に使う `cc` が入る。
2. Homebrew で GitHub CLI を入れる（`brew install gh`）。Intel の Mac では LLVM 22 と zstd も
   入れる（`brew install llvm@22 zstd gh`）。Apple Silicon では要らない。
3. Rust を入れる（rustup か `brew install rust`）。std が対象とする最低 macOS バージョンが、配る
   実行ファイルの最低バージョンの候補になる（上の `minimum macOS:`）。古い Rust は新しい macOS
   で動かないことがある（rustc 1.89 は macOS 27 で、自分の作った proc-macro の dylib を dyld に
   「mis-aligned LINKEDIT string pool」と言われて読み込めなかった）。`rustup update stable` で
   新しくしておく。
4. `gh auth login` で、このリポジトリのリリースに書き込めるアカウントにログインする。
5. リポジトリを clone する（`git clone https://github.com/JunSuzukiJapan/typelisp.git`）。

`scripts/setup-cargo-env.sh` は要らない。`build.sh` は LLVM の場所と最低 macOS バージョンを
自分で決めて環境変数に設定する。

作って添付する。`<arch>` は Intel なら `x86_64`、Apple Silicon なら `arm64`。同じ名前のファイルが
既に添付されているときは `gh release upload` に `--clobber` を付ける。

```sh
git checkout v<version>
scripts/dist/build.sh
gh release upload v<version> target/dist/typelisp-<version>-<arch>.tar.gz target/dist/typelisp-<version>-<arch>.tar.gz.sha256
git checkout main
```

添付した後、その Mac で `install.sh` から入ることを確かめる。既に入れている typl を上書き
しないよう、インストール先を一時的なフォルダにする:

```sh
curl -fsSL https://raw.githubusercontent.com/JunSuzukiJapan/typelisp/main/install.sh | TYPELISP_HOME=/tmp/typelisp-check sh
/tmp/typelisp-check/bin/typl --version
rm -rf /tmp/typelisp-check
```

### 仕組み

`install.sh` はアセットの名前を `typelisp-<version>-<arch>.tar.gz`（`arch` は `x86_64` か
`arm64`）と決め打ちしているので、名前を変えるなら両方を直す。

`build.sh` が普通のリリースビルドと違うのは 2 点。zstd を静的にリンクする（LLVM は zstd 付きで
ビルドされていて、llvm-sys は `-lzstd` を渡す。`libzstd.a` だけを置いたフォルダを先に探させると、
リンカは Homebrew の dylib でなくそちらを取る）。最低 macOS バージョンを、std・LLVM・zstd のうち
最も高いものにする。リンク先が `/usr/lib` と `/System/Library` 以外にあればそこで止まる。

Apple Silicon で Homebrew の LLVM を使わないのは、Homebrew のボトルが最新の macOS 向けにしか
無いため。macOS 27 の Mac で llvm@22 を使うと最低 macOS が 27.0 になった。LLVM 公式の
`LLVM-<version>-macOS-ARM64.tar.xz` は古い macOS 向けにビルドされているが、静的ライブラリの中身が
機械語でなく ThinLTO 用の LLVM bitcode なので、そのままではリンクできない。同梱の `ld64.lld` は
新しい SDK の `.tbd` を読めず、Apple の ld に同梱の `libLTO.dylib` を使わせると C++ ランタイムの
シンボル（`operator delete` など）が未定義になる。そこで `build.sh` は、`llvm-config --libnames`
が挙げるアーカイブ（`libLLVM*.a` と Polly）の各オブジェクトを、同梱の clang で
`llvm-config` 自身の最低 macOS 向けの機械語にしてから、アーカイブを元の順で作り直す。zstd も
Homebrew のものは最新の macOS 向けなので、同じ macOS 向けにソースからビルドする。どちらも
ダウンロードしたファイルの SHA-256 を `build.sh` に書いた値と照らす。LLVM や zstd の版を上げる
ときは、版と SHA-256 の両方を書き換える。変換したライブラリはライブラリごとのコード生成なので、
ThinLTO を通したものより JIT が遅い可能性はあるが、測っていない。

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
