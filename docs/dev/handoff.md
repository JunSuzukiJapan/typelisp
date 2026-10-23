# 引き継ぎ（2026-10-23）

別のセッション（クラウドを含む）がこの作業を続けるための文書。作業の設計と段階は
[explicit-symbols-plan.md](explicit-symbols-plan.md) にある。ここには、その計画のどこまで進んで
いて、何が途中で、次に何をするかだけを書く。作業が進んだら、この文書も書き換える。

## 1. 今の位置

- ブランチ: `feature/keyword-markers`。main には入れていない。
- 計画の段階 4（名前の解決をポインタにする）は 4d まで済み。4d で変えたことは計画の 4d の項に
  ある。要点は、解析が今のモジュールを `NsId` で持つことと、IR の中の定義の名前をルートからの
  パス（`::f`、`::m::f`）で読み書きすること（`core::path_form` / `core::path_value`）。ダンプの形式は 25、
  ABI は 0.3.14。
- 段階 5 は 5a まで済み。IR とコンパイル済みコードが型をフォームで運ぶ（計画の 5a の項）。
  ダンプの形式は 26、ABI は 0.3.16。
- 5b も済み。ヒープの型の表は形（`TypeShape`）で引き、`TypeKeyId` は `TypeId` になった
  （計画の 5b の項）。ABI は 0.3.17。
- 5c も済み。型の表はプロセスに 1 つ、印字と vtable の表は定義か `TypeId` で引き、実行ファイルは
  起動時に型の表を作る（計画の 5c の項）。ABI は 0.3.18。
- 5d も済み。コンパイル済みコードは型をモジュールの型の表から読み（`compile::type_table`）、
  FFI の型・コールバック・C メモリ層は `TypeId` と定義のシンボルで引く（計画の 5d の項）。
  ダンプの形式は 27、ABI は 0.3.20。

## 2. 4d で知っておくこと

- IR の定義の名前は `core::path_form` で書き、`core::path_value`（`core::path_field`）で読む。
  読む側は木の子をシンボルでたどるので、`core::path_form` を通らずに組んだパス（綴りから
  intern したシンボルの並び）を渡すと panic する。
- 手書きの IR のテストは、定義を `::f`、`::m::f`、`(assoc ::int ...)` のように書き、`%core` で
  そのまま読む。読んだあとにシンボルを付け替える補助関数は作らない。印字した IR を比べる
  テストの期待値も同じ形になる。
- ダンプの中のパスのシンボルの意味が変わったので、島と prelude の成果物を作り直した。
  ダンプを読む処理を変えたら、島 → prelude → ABI の順に作り直す（スキル commit-check の 2 節）。
- `Checker::exit_file_module` は引数を取らない（戻り先は `enter_file_module` が積む）。
- 綴りの並びから `Path` を作り直す処理は、型キーの文字列（`type_key.rs`、`ConcreteNames`）と
  評価器の表（`eval/scope.rs`、`eval/interp.rs`）に残っている。段階 5 と 7 の対象。

## 3. 次にすること（順に）

1. 5e（`print-object` の表の文字列キー `value_key_for_method` / `method_for_value` を含む）。
   FFI の thunk とコールバックの本体の LLVM 名はまだパスの文字列から作る
   （`ConcreteNames::symbol(&path.to_string())`）。これも 5e の単型化の識別と一緒に直す。
2. そのあと段階 6〜9（計画の 6 節）。各段階で全テストを通してからコミットし、push する。
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
