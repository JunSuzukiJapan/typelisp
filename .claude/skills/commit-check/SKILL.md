---
name: commit-check
description: typelisp でコミットや push をする前の確認。CI と同じ警告 0 の cargo check、翻訳の鮮度、生成物（島・prelude・ABI）の再生成が要るかの判断、全体テストの判定、文書の規則。「コミットする」「push する」「段階を終える」「CI が赤い」ときに使う。
---

# コミット前の確認

CI（`.github/workflows/ci.yml`）が落とすものと、ユーザーが決めた規則を、コミットの前に
確かめる。秒で済む検査は git の pre-commit フック（`scripts/git-hooks/pre-commit`）が毎回
強制する。ここに書くのは、時間がかかるか判断の要る残りの確認。

## 0. フックが有効か

`git config core.hooksPath` が `scripts/git-hooks` でなければ、
`git config core.hooksPath scripts/git-hooks` を打つ（クローンごとに 1 回。クラウドセッションも）。
フックが見るのは、翻訳の鮮度（CI の Translations と同じ `scripts/check-translations.sh`）、
足した行のマシン固有の絶対パス、足した行の絵文字・アイコン記号。フックに止められたら、
`--no-verify` で抜けずに直す。

## 1. 警告 0 の cargo check（CI の Check と同じ）

```sh
scripts/with-llvm-env.sh cargo check --workspace --all-targets --locked 2>&1 | grep -cE '^(warning|error)'
```

0 でなければ直す。CI はこれを macOS と Linux で走らせ、警告をエラーとして扱う。
`--locked` が落ちたら `Cargo.lock` を更新し忘れている。dead code の警告は、消すか、
使うはずだったのに繋がっていない箇所の印として扱う（`#[allow]` で黙らせない）。

## 2. 生成物の再生成が要るか

変えたもので決まる。足りないと、鮮度のテストが落ちる。

| 変えたもの | 打つもの | 落ちるテスト |
|---|---|---|
| 島の SOURCE（`compiler.rs`）、`externs.rs` の `rt_*` の表 | `scripts/regen-compiler-island.sh` | `island_artifacts_are_fresh` |
| `llvm-*` のビルダー、島が出す IR の形 | 上を 2 回（1 回目は古い島で作るため） | `the_committed_island_matches_a_fresh_build` |
| prelude の SOURCE（`prelude.rs`）、コンパイルの橋 | `scripts/regen-prelude-bitcode.sh` | `prelude_artifacts_are_fresh`、`the_committed_prelude_matches_a_fresh_build` |
| コンパイル済みコードが前提にするランタイムの形 | `scripts/regen-abi-version.sh` | `abi_version_test` |

島と prelude を両方作り直すときは、島が先（prelude は島がコンパイルする）。
生成物（`src/compiler_island.typld`、`crates/typelisp-front/src/prelude.typld`、
`docs/dev/api_version/`）は手で直さない。`docs/dev/api_version/history/` の既存ファイルは
消さず、書き換えない。

## 3. 全体テスト

```sh
scripts/with-llvm-env.sh scripts/test-serial.sh > <ログ> 2>&1
```

通ったと言えるのは、ログに `ALL TESTS PASSED (serial)` があるときだけ。

- LLVM を使うテストを別々のプロセスで同時に走らせると SIGSEGV する。個別のテストも 1 本ずつ
  流す。SIGSEGV したテストは `test result` の行を出さないので、行が無いのも失敗として数える。
- AOT のテスト（実行ファイルを作るもの）は `cargo test` が作らない staticlib にリンクする。
  ソースを変えたら先に `scripts/with-llvm-env.sh cargo build -p typelisp-front`。
- zsh は変数を単語に分けない。テスト名の絞り込みを変数で渡すと空振りするので、直接書く。
- テストが走っている間にソースを編集しない。
- 実行時間は計測しない。報告にも書かない。

## 4. 翻訳

日本語が原本。`docs/ja/<path>` を変えたら `docs/<言語>/<path>` の全言語、`README_JP.md` を
変えたら `README.md`（英）、`editor/<エディタ>/README_JP.md` を変えたらその各言語版。

1. 原本の変更をコミットする（訳の 1 行目に書くハッシュが要るので先に確定させる）。
2. 英語を先に、そのあと残りの言語を、原本で変えた箇所だけ直す。訳し直しで他の段落を
   崩さない。
3. 各訳の 1 行目 `<!-- translated-from: <原本> @ <完全なハッシュ> -->` を手順 1 のコミットにする。
   中身を直さずに 1 行目だけ進めない（古い訳が最新に見える）。
4. `scripts/check-translations.sh` が「all N translations are up to date」になること。

## 5. 文書の規則

- 絵文字・アイコン記号・飾り罫線を使わない（全文書、全訳）。見出し・表・箇条書き・
  コードブロックはよい。
- 利用者向け文書（`docs/<言語>/`、README）から `docs/dev` やソースへリンクしない。
  経緯、段階の番号、計測値も書かない。
- マシン固有の絶対パスを書かない。`brew --prefix` などで動的に解決する。
- コメントは今の実装を説明する。消したコードの経緯を残さない。

## 6. コミットと push

- `cargo fmt` を打たない（リポジトリは未整形で、130 ファイルが巻き添えになる）。
- `git diff --cached` で中身を見てからコミットする。デバッグ用の行は編集で消す
  （`git checkout <file>` は未コミットの変更を全部消す）。
- メッセージの末尾は `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- push した後は `gh run list --branch <ブランチ> --limit 3` で CI を見る。赤ければ
  `gh run view <id> --log-failed` で原因を見て直す。
- 引き継ぎ文書（`docs/dev/handoff.md`）がある作業なら、進んだところまで書き換える。
