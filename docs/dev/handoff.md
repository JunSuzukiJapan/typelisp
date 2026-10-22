# 引き継ぎ（2026-10-22）

別のセッション（クラウドを含む）がこの作業を続けるための文書。作業の設計と段階は
[explicit-symbols-plan.md](explicit-symbols-plan.md) にある。ここには、その計画のどこまで進んで
いて、何が途中で、次に何をするかだけを書く。作業が進んだら、この文書も書き換える。

## 1. 今の位置

- ブランチ: `feature/keyword-markers`。main には入れていない。
- 計画の段階 4c-3（パスを読んだシンボルで引き、祖先の連鎖と `std` の段を消す）の途中。
  コードは書き終わっていて、テストを通している最中。
- 4c-3 の作業は、最後の正式なコミット `c58cf29`（4c-2）の上に、`wip:` で始まるコミットとして
  バックアップのために置いてある。全テストが通るまでは `wip:` のままにする。全テストが
  通ったら、メッセージを 4c-3 の内容に書き直したコミットにする（`git commit --amend` でよい）。

## 2. 4c-3 で変えたこと

計画の 4c-3 の項に書いたことに加えて、テストを通すために次を変えた。計画の 4c-3 の項に
まだ書いていないものがあるので、コミットの前に書き足す。

- `Info::in_scope`（`crates/typelisp-front/src/check/info.rs`）は、移行のための広い規則のまま。
  `item_ns == cur || in_std(item_ns) || (in_std(cur) && item_ns == NsId::ROOT)`。
  標準ライブラリの定義は、その非公開の層（`%internal`、ストリームのハンドル）も含めて
  どこからでも見え、標準ライブラリのモジュールからはルートの非公開の組み込みが見える。
  段階 6 で狭める。
- `forms::member_form`（`check/forms.rs`）は `Result` を返し、`(%type-member 型 メンバー)` を作る。
  パスにしないのは、型と同じ名前のモジュールがあるとパスがモジュールの方に取られるため。
  `check/analyze.rs` の `check_list` が `named_type_member` でこの形を受け、
  `PathTarget::Member` として `check_target_call` に渡す（`check_path_call` の本体をこの関数へ
  切り出した）。`Name<A>::member` の形は `named_type_member` が `None` を返すので、従来どおり。
- 配列リテラルの先頭は `forms::name_form(&Path::internal("array-from-row-major"))`。
  `path_form` で書くと `std::%internal` が見えなかった。
- `use_target`（`check/analyze.rs`）の書き直し。1 段なら今のモジュールの定義かモジュール、
  2 段以上なら `use_target_in` でたどる。どちらでも見つからなければ、同じディレクトリの
  ファイルを名指す規則（ファイルのモジュールの親から最初の段を探す）を 2 段以上にも効かせる
  （`examples/projects/shape-canvas` の `(use canvas::canvas)`）。
- `Registry::publish_names`（`check/registry.rs`）は、各名前空間について
  `publish_imports` も呼ぶ。`use` の別名、モジュールの別名、構成子、静的メソッドの取り込みを
  セッションへ載せる。ダンプから戻したセッション（AOT 実行ファイルの中の `eval`）が
  `use` の取り込みを持っていなかったため。
- 祖先の非公開の定義を子から使っていたテストに `pub` を付けた（`tests/keyword_test.rs`、
  `tests/test_attribute_test.rs` ほか）。原則は「`pub` でない定義はほかのモジュールから
  見えない。祖先も同じ」。
- 日本語の文書: `docs/ja/reference/syntax.md` の 3.13 節（`pub deftrait`、`pub` を付けられないのは
  `module`/`use`/`impl` だけ）と、素の名前の探し方の段落（親のモジュールは探さない）。
  `docs/ja/guide/modules.md` の `pub` の対象にトレイトを足し、トレイトも `pub` で公開できると
  書き換えた。

## 3. 次にすること（順に）

1. テストの確認。前回の全体テストで落ちた 14 本を直した後、関係するテストを個別に流している。
   通ったもの: context、keyword、test_attribute、trace、tuple、vector_literal、option_niche、
   repl_tools。まだ確かめていないもの:
   - `examples_check_test`、`lsp_semantic_test`（`use` した canvas の型がハイライトされない、
     という失敗だった。同じディレクトリの `use` の修正で直っているはず）、`module_file_test`、
     `module_decl_test`、`macro_use_test`。
   - 名前で絞って流すもの。AOT のテストは先に staticlib を作る。
     ```sh
     scripts/with-llvm-env.sh cargo build -p typelisp-front
     scripts/with-llvm-env.sh cargo test -q --test compile_file_test an_evaluated_form
     scripts/with-llvm-env.sh cargo test -q --test compile_test compile_a_modules_function_and_a_same_named
     scripts/with-llvm-env.sh cargo test -q --test stream_test a_string_stream_that_unread
     ```
2. `scripts/regen-abi-version.sh`（`abi_version_test` が落ちていた）。
3. 島や prelude の成果物が古いと言われたら、`scripts/regen-compiler-island.sh`（2 回要ることが
   ある）、`scripts/regen-prelude-bitcode.sh`。
4. 全体テスト。`scripts/with-llvm-env.sh scripts/test-serial.sh > 任意のログ`。通ったと言えるのは、
   ログに `ALL TESTS PASSED (serial)` があり、`cargo check` の出力に `^warning` と `^error` の行が
   1 つも無いとき。
5. 子のモジュールから `(use ::f)` で親の `pub` な定義を取り込めるかを `typl` で確かめる。
   できるなら、`docs/ja/guide/modules.md` に「親のモジュールもほかのモジュールと同じ」と
   1 項目足す。
6. 翻訳。日本語の原本を変えたので、`docs/en/` を先に、そのあと残りの 8 言語を、同じ箇所だけ
   直す。各訳の冒頭の translated-from の行のコミットハッシュを、原本を変えたコミットに
   合わせる。`scripts/check-translations.sh` で確かめる（CI の Translations と同じ検査）。
7. 計画の 4c-3 の項に、2 節のうちまだ書いていないこと（`%type-member` の頭、`publish_imports`、
   2 段以上の同じディレクトリの `use`、`in_scope` が移行のあいだ広いまま）を書き足す。
8. コミット（`wip:` を 4c-3 のメッセージに直す）して push。CI（Translations を含む）を確かめる。
9. 段階 4d。文字列で解決する処理の残りを消す。対象は `path_form`、`segments_form`、
   `path_from_id`、`intern_path_id`、`core.rs` の `path_field`、`Info::ns`、`fq`、`enter_module`、
   `mod_aliases`。そのあと段階 5〜9（計画の 6 節）。各段階で全テストを通してからコミットし、
   push する。

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
- テストが走っている間にソースを編集しない。
- `cargo check` が通っただけで完了にしない。警告 0 まで直す（dead code の警告は未実装の印）。
- コメントは実装より長生きさせない。消したコードの経緯をコメントに残さない。
- マシン固有の絶対パスをコードや文書に書かない。
- コミットメッセージの末尾は `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
  コミットのたびに push する（バックアップのため）。
- 実行時間を計測して数字を出さない。

## 5. 落とし穴

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
