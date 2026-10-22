# 引き継ぎ（2026-10-22）

別のセッション（クラウドを含む）がこの作業を続けるための文書。作業の設計と段階は
[explicit-symbols-plan.md](explicit-symbols-plan.md) にある。ここには、その計画のどこまで進んで
いて、何が途中で、次に何をするかだけを書く。作業が進んだら、この文書も書き換える。

## 1. 今の位置

- ブランチ: `feature/keyword-markers`。main には入れていない。
- 計画の段階 4c-3（パスを読んだシンボルで引き、祖先の連鎖と `std` の段を消す）は済み。
  全体テストが通り、警告は 0。4c-3 のコミットは `wip:` で始まるバックアップのコミットと、
  それを仕上げるコミットの 2 つに分かれている（push 済みの履歴は書き換えない）。
  4c-3 で変えたことは計画の 4c-3 の項にある。
- 次は段階 4d。

## 2. 4c-3 の終わりに見つけて直したこと

- `use_target` が `heap.names()` のロックを握ったまま `use_target_in` を呼び、同じロックを
  もう一度取って止まっていた（examples_check_test が 5 時間止まった）。`Heap::names` は
  `NamesGuard` を返し、同じスレッドが二度取ろうとしたらその場で panic する。
- ルートからのパス（`(use ::main::f)`）で、ローダーが先頭の空の段をファイルの綴りに入れ、
  読み込み中のファイルを別の名前のモジュールとしてもう一度読み込んでいた。
  `Loader::ensure_loaded` が空の段を飛ばし、`try_load_module` は空の段を受け取ると panic する。
  回帰テストは `tests/module_file_test.rs` の末尾の 2 本。
- ファイルの一番外側はモジュール `main`（ファイル名から）、REPL の一番外側は `user` で、
  どちらもルートではない。子のモジュールから親の定義を使うには `::main::f` のように
  ルートからのパスで書く。`docs/ja/reference/syntax.md` の例を `::geo::f` に直した。

## 3. 次にすること（順に）

1. 翻訳。`docs/ja/reference/syntax.md` の例を `::f` から `::geo::f` に、「ルートからの」を足して
   直した。そのコミットのハッシュで、全言語の `reference/syntax.md` の同じ文を直し、1 行目の
   translated-from を進める。`scripts/check-translations.sh` で確かめる（pre-push も検査する）。
2. 段階 4d。文字列で解決する処理の残りを消す。対象は `path_form`、`segments_form`、
   `path_from_id`、`intern_path_id`、`core.rs` の `path_field`、`Info::ns`、`fq`、`enter_module`、
   `mod_aliases`。
3. そのあと段階 5〜9（計画の 6 節）。各段階で全テストを通してからコミットし、push する。
   段階 6 で `Info::in_scope` の移行のための広い規則
   （`item_ns == cur || in_std(item_ns) || (in_std(cur) && item_ns == NsId::ROOT)`）を狭める。

## 4. 守ること

ユーザーが決めたこと。

- 応答と文書は日本語。文書に絵文字・飾り罫線を使わない。利用者向け文書から `docs/dev` へ
  リンクしない。
- パスを含んだ名前を登録しない。`foo::bar::zot` は「foo の子に bar、bar の子に zot」と読む
  設計だけを受け付ける。チェッカーは型検査だけをし、型情報を持つ層とは分ける。
- フォールバックするコードを書かない。どうしても要るなら `unimplemented!()` にするか、
  ユーザーに尋ねる。
- エラーは見つけたらすぐ直す。いつからあるかを調べない。「既存の制限」「今回と無関係」で
  片付けない。実例だけでなく、同じ間違いがまた書けないように穴を塞ぐ。
- `cargo fmt` を打たない（リポジトリは整形されておらず、130 ファイルが巻き添えになる）。
- テストが走っている間にソースを編集しない。テストが読む文書（`docs/ja/reference`）も同じ。
- `cargo check` が通っただけで完了にしない。警告 0 まで直す（dead code の警告は未実装の印）。
- コメントは実装より長生きさせない。消したコードの経緯をコメントに残さない。
- マシン固有の絶対パスをコードや文書に書かない。
- コミットメッセージの末尾は `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
  コミットのたびに push する（バックアップのため）。
- 実行時間を計測して数字を出さない。

## 5. コミット前の確認

- クローンしたら最初に `git config core.hooksPath scripts/git-hooks` を打つ。pre-commit フックが、
  翻訳の鮮度（CI の Translations と同じ検査）と、足した行の絶対パス・絵文字を毎回確かめる。
  日本語の原本を変えるコミットだけは翻訳の検査を飛ばし、pre-push が push の前に検査する。
- コミットや push の前の残りの確認（警告 0 の `cargo check`、生成物の再生成の判断、全体テストの
  判定、翻訳の直し方）は、スキル `.claude/skills/commit-check/SKILL.md` にまとめてある。

## 6. 落とし穴

- デバッグ用の行を消すのに `git checkout <file>` を使わない。未コミットの変更がすべて消える
  （このセッションで `crates/typelisp-mem/src/symbols.rs` を一度消し、会話の記録から復元した）。
  編集で消す。
- zsh は変数を単語に分けない。`for t in "a b"; do set -- $t` は効かず、テスト名の絞り込みが
  空振りする。引数は直接書く。
- LLVM を使うテストを別々のプロセスで同時に走らせると SIGSEGV する。全体は
  `scripts/test-serial.sh` で流し、個別のテストも 1 本ずつ流す。SIGSEGV したテストは
  `test result` の行を出さないので、行が無いことも失敗として数える。
- AOT のテスト（実行ファイルを作るもの）は `cargo test` が作らない staticlib にリンクする。
  ソースを変えたら `scripts/with-llvm-env.sh cargo build -p typelisp-front` を先に。
- LLVM 22 が要る（`LLVM_SYS_221_PREFIX`）。`scripts/with-llvm-env.sh` がそれを設定する。
  Linux での用意は CI の設定（`.github/workflows/`）を見る。
- `src/compiler_island.typld` と `crates/typelisp-front/src/prelude.typld` は生成物。
  手で直さず、再生成のスクリプトで作る。
