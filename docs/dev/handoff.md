# 引き継ぎ（2026-10-30）

別のセッション（クラウドを含む）がこの作業を続けるための文書。作業の設計と段階は
[explicit-symbols-plan.md](explicit-symbols-plan.md) にある。ここには、その計画のどこまで進んで
いて、次に何をするかだけを書く。作業が進んだら、この文書も書き換える。

## 1. 今の位置

- ブランチ: `claude/nice-feynman-dasp54`（`feature/keyword-markers` の続き）。main には入れていない。
- 段階 4（名前の解決をポインタにする）と段階 5（型をフォームと `TypeId` で運ぶ）は済み。
  各段階で変えたことは計画の 4d・5a〜5e の項にある。
- 段階 6 は 6a（再公開 `(pub use PATH)`）、6b（ルートの組み込みを `std` へ）、6c（`std::prelude` が
  `std` を全部再公開）、6d（`std` の子モジュールに分ける）、6d' （シンボルの大文字と小文字の区別）が
  済み。6d で移したものの一覧は計画の 6d の項にある。`std` 直下に残したもの（`not` `unreachable` `todo`、
  関数の組み合わせ、`compile-file` `dump`）は 5 節の表に置き場所が無いため。
- 段階 7 は済み。7a で局所の束縛・block・macrolet をシンボルで引き、7b で組み込みの関数・メソッド・
  実行時の定義の表をシンボルで引き、すべての定義の名前のシンボルの home を定義したモジュールにし
  （番人は `tests/definition_home_test.rs`）、定義を指すコア IR を定義のシンボル 1 つにした
  （`core::definition_form`/`definition_value`）。モジュールの名前と `use` はルートからのパスのまま
  （`core::root_path_form`/`root_path_value`）。予約語の名前で定義するとエラーになる
  （2026-10-29 のユーザーの決定）。
- 段階 8（ダンプ・fasl を計画 3.6 の形に）と段階 9（利用者向け文書）は済み。ダンプの形式は 30。
  計画の段階はすべて終わった。
- ABI は 0.3.46。全体テストは最後のコミットで通っている。
- 途中の作業は無い（作業ツリーはコミット済み）。

## 2. 次にすること（順に）

1. 計画の段階は残っていない。`feature/keyword-markers` を main に入れるかどうかはユーザーが決める。

## 3. 移すときの手順

1. registry の登録先の名前空間を変え、prelude のソース（`crates/typelisp-front/src/prelude.rs`）なら
   `(module NAME ...)` で包んで `pub` を付ける。新しい子モジュールは `registry::PRELUDE_MODULES` に足す。
2. コードの中でパスを組み立てている箇所（`Path::std("...")`、`Path::in_ns(...)`）を探して直す。
   組み込みの型は `Path::builtin(wk::X)` を使う。
3. コア IR を手で書いているテスト（`src/compile/core_bridge.rs`、`eval/interp/core_eval.rs`、
   `eval/interp/core_cps.rs`、`tests/compile_test.rs` のテスト）の `::std::...` のパスを新しい場所に直す
   （読み込み関数が `core::name_written_definitions` で定義のシンボルに置き換える）。新しい子モジュールは
   各テストの読み込み関数（`read_core` / `read1`）で宣言する。
4. 島 → prelude → ABI の順に作り直す（スキル commit-check の 2 節）。島の名前や IR が変わったときは
   島を 2 回作り、2 回目で成果物が変わらないことを確かめる。
5. `scripts/with-llvm-env.sh cargo build -p typelisp-front` で staticlib を作り直してから全体テスト。
   ABI を変えたのにこれを忘れると、AOT のテストが「not of ABI version」でリンクに失敗する。

## 4. 守ること

ユーザーが決めたこと。

- 応答と文書は日本語。文書に絵文字・飾り罫線を使わない。利用者向け文書から `docs/dev` へ
  リンクしない。
- パスを含んだ名前を登録しない。`foo::bar::zot` は「foo の子に bar、bar の子に zot」と読む
  設計だけを受け付ける。`::` でつないだ文字列で比べない。文字列で比べている箇所は、できれば
  シンボルで比べるようにする。
- シンボルは大文字と小文字を区別する。型名だけが大文字始まりで、基本型（`int` `string` など）と
  変種名（`some` `none` `ok` `err`）は小文字。名前を `to_lowercase` で正規化しない。
- `match` の腕の一番外に裸で書いた名前は常に変種。何にでも当たるのは `_` だけ。
- 文字列からシンボルを得るのは `intern`（CL 準拠）だけ。`string->symbol` は無い。
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
- 翻訳は commit-check の 4 節の手順で行う（日本語の原本を先にコミットし、訳の 1 行目にその
  ハッシュを書く）。

## 5. クラウドセッションでの用意

- クローンしたら最初に `git config core.hooksPath scripts/git-hooks` を打つ。pre-commit フックが、
  翻訳の鮮度（CI の Translations と同じ検査）と、足した行の絶対パス・絵文字を毎回確かめる。
  日本語の原本を変えるコミットだけは翻訳の検査を飛ばし、pre-push が push の前に検査する。
- LLVM 22 が要る。`scripts/with-llvm-env.sh` が `LLVM_SYS_221_PREFIX` を設定する（Linux では
  `scripts/linux-llvm-prefix.sh` が探す）。Linux で LLVM 22 と依存を入れる手順は
  `.github/actions/setup-build/action.yml` にある。
- クラウドの環境で apt が apt.llvm.org に届かないときは、apt にプロキシと CA を渡す。apt は `_apt`
  ユーザーで動き `/root` の下を読めないので、CA の束を読める場所へ写して
  `Acquire::https::apt.llvm.org::Proxy` と `::CAInfo` を `/etc/apt/apt.conf.d/` に書く。ネットワークの許可は
  始めたセッションには効かないことがある。
- クラウドのディスクは割り当てが小さい。テストの実行ファイル（`target/debug/deps`）が 20G を超え、
  `target/aot-test-tmp` にも数 G たまる。`CARGO_INCREMENTAL` を変えると実行ファイルの名前が変わって前の
  ものが残るので、全体テストの間は変えない。ディスクが一杯になると、走り終えたテストの実行ファイルと
  `target/aot-test-tmp` の古いものを消して空ける。
- cargo のコマンドは `scripts/with-llvm-env.sh` を通して打つ（例:
  `scripts/with-llvm-env.sh cargo check --workspace --all-targets --locked`）。
- 全体テストは `scripts/with-llvm-env.sh scripts/test-serial.sh > <ログ> 2>&1`。通ったと言えるのは
  ログに `ALL TESTS PASSED (serial)` があるときだけ。始める前に `~/.typelisp/cache` を消す
  （registry を変えると古い fasl のキャッシュが残る）。
- コミットや push の前の残りの確認（警告 0 の `cargo check`、生成物の再生成の判断、全体テストの
  判定、翻訳の直し方）は、スキル `.claude/skills/commit-check/SKILL.md` にまとめてある。

## 6. 落とし穴

- デバッグ用の行を消すのに `git checkout <file>` を使わない。未コミットの変更がすべて消える。
  編集で消す。
- zsh は変数を単語に分けない。`for t in "a b"; do set -- $t` は効かず、テスト名の絞り込みが
  空振りする。引数は直接書く。
- LLVM を使うテストを別々のプロセスで同時に走らせると SIGSEGV する。全体は
  `scripts/test-serial.sh` で流し、個別のテストも 1 本ずつ流す。SIGSEGV したテストは
  `test result` の行を出さないので、行が無いことも失敗として数える。
- lib のテストに出る PoisonError は、先に panic した別のテストの巻き添え。最初の panic を見る。
- `src/compiler_island.typld` と `crates/typelisp-front/src/prelude.typld` は生成物。
  手で直さず、再生成のスクリプトで作る。`docs/dev/api_version/history/` の既存のファイルは消さず、
  書き換えない。
- 型を子モジュールへ移すと、標準ライブラリの 2 つのモジュールが同じ名前を定義していないかを
  `Registry::reexport_std_in_prelude` が assert で確かめる。ライブラリ自身が二重に定義すると
  ここで panic する（利用者の定義はその前にチェッカーが断る）。
