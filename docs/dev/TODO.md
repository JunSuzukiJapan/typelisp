# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-23 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。完了した実装の詳細な経緯・設計判断は
[implementation-log.md](implementation-log.md) を参照（2026-06-27 にこちらから分離、
以後も完了項目は都度こちらへ移設する）。
**言語仕様の確定事項は [language-design.md](language-design.md) を参照。**

---

## 残っている作業

2026-07-21 時点で行われていたモジュール可視性の祖先チェーン方式への再設計・Interpのスコープ
ツリー全面移行・それに伴う重複コード整理（[implementation-log.md](implementation-log.md) 参照）は
いずれも全テストgreenで `main` にマージ済み。[symbol-sexpr-redesign.md](symbol-sexpr-redesign.md)
の Phase 7（ドキュメント整備）も `docs/functions.md`/`docs/syntax.md`/`docs/dev/language-design.md`
との突き合わせを完了し、完了扱いにした（2026-07-22）。

CL同等カタログ・可視性・trait機構・compile（普通に書けるコードから到達する範囲）は実装済みで、
**進行中の作業はない**。ただし [language-design.md](language-design.md) §8「当面の範囲外」／§7.4／
[functions.md](../functions.md) に「将来課題」として散在していた未実装項目を、以下に**着手候補の
TODO として正式に格上げ**する（2026-07-23、この一覧化で棚卸し）。優先度は目安であり、着手順は未確定。

完了した項目（T1「`format` の書式指定子」、T5「`--heap-cells N`」）は
[implementation-log.md](implementation-log.md) 末尾へ移設した（2026-07-23）。

### T2. `defmacro` の構造化ラムダリスト `&optional` / `&key`（優先度: 中）

現状 `defmacro` のラムダリストは `&rest` のみ対応（`&rest` は 2026-07-15 に再導入済み）。
`&optional`（省略可能引数＋デフォルト値）と `&key`（キーワード引数）は
[language-design.md](language-design.md) §8 で対象外扱いのまま。

- マクロ展開時のみの機能なので型システムへの波及は小さいが、デフォルト値式の評価タイミングと
  `&rest` との併用順序（CL のラムダリスト規約）を仕様として固める必要がある。

### T3. ユーザ定義エラー型（優先度: 中〜低）

現状エラー型は組み込み汎用 `Error` のみで、既定は `Result<T, Error>`
（[language-design.md](language-design.md) §7.4）。trait機構（`deftrait`/`impl`/`where`）は
実装済みだが、**ユーザ定義エラー型をこの機構で扱えるように拡張する作業自体は未着手**。

- `defstruct`/`defenum` で定義した型をエラーとして `Result<T, MyError>` に載せられるようにする。
- 動的ディスパッチ（T4）と関連: 複数のエラー型を一様に扱う場面では vtable 相当が絡む可能性がある。

### T4. 動的ディスパッチ（vtable / `dyn Trait` 相当）（優先度: 低）

静的 trait 機構（単型化ベース）は実装済み（§5.1）。実行時に型が決まる動的ディスパッチ
（[language-design.md](language-design.md) §8）は未実装。

- 静的型・単型化を前提とした現在の設計への影響が大きく、設計判断（表現・GC・compile対応）を
  要する重い項目。優先度は最も低い。

### T5. pretty printer（CL の Lisp Pretty Printer 相当）（優先度: 低）

CL は ANSI 標準の pretty printer を持つ（CLHS 22.2、元は R. Waters の XP）。`format` 実装
（[implementation-log.md](implementation-log.md)、2026-07-23）ではこの系統のディレクティブを未対応
（no-op / 近似）にしてある。**pretty printer 本体を別タスクとして切り出す**（2026-07-23、`~i`/`~_`
等の議論で棚卸し）。

未対応で、この項目で扱う範囲:
- 特殊変数 `*print-pretty*` / `*print-right-margin*` / `*print-miser-width*` / `*print-pprint-dispatch*`
- 関数 `pprint` / `pprint-fill` / `pprint-linear` / `pprint-tabular` / `pprint-logical-block` /
  `pprint-newline` / `pprint-indent` / `pprint-tab` / `set-pprint-dispatch`
- `format` ディレクティブの pretty 連動分: `~w`（現状は単なる `prin1` に寄せてある）、`~_`（条件改行）、
  `~i`（インデント。現状 no-op）、`~<...~:>`（閉じに `:` が付く**論理ブロック**用法。現状の桁揃え
  `~<...~>` とは別物）、`~:t`（論理ブロック内タブ）

- 重い理由: format 単体でなく**印字系全体**に、行幅追跡・インデントスタック・条件改行判断を持つ出力
  ストリーム層が要る。静的型・`*print-*` 変数の持ち方（動的変数機構の要否）とも絡む。優先度は低。
- 関連: [[typelisp-format-directives]]（`format` 実装。未対応分の一覧はここと docs/functions.md §15）。

### 意図的に「やらない」もの（TODO ではない）

以下は将来課題ではなく**設計判断で対象外**と確定済み。混同しないこと。

- **`Sexpr` への `Iter<Item>` trait 実装**: 要素型が固定されないリストにジェネリックな
  `Iter<Item>` を被せるのは型システム上不適切というユーザー判断（[language-design.md](language-design.md)
  §5末尾、[[typelisp-typechecking-is-not-design-soundness]]）。一度実装したが撤回済み。
  なお、cons セルのリストを走査する反復手段としては **`dolist` マクロが別途ある**
  （`src/prelude.rs` の `defmacro dolist`）。`Iter` トレイトを介さず、`consp`/`car`/`cdr` で
  直接歩いて各要素を束縛する（要素は動的に `Sexpr`。使う側が `match` で具体型に分解する）ので、
  上記の「ジェネリックな `Iter<Item>` を被せない」方針と両立している。
- **`?`/`try` 構文**、および `!`/`?` の命名接尾辞: CL に倣い非採用（§7.3）。

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

`compile`（LLVM JIT/AOT）機能は現状「今のフェーズが実際に使う AST ノードだけ本実装、それ以外は
`ast_bridge` が `(unsupported "<Variant>")` を返しコンパイラ本体が明示的に panic する」設計
（`ast_bridge.rs` 冒頭のdocコメント参照）。ユーザーが普通に書けるコードから実際に到達しうる
`unsupported` は 2026-07-16 時点で解消済み——残る `Expr::TraitCall` は単型化後に到達不能な
診断専用ノードと確認済みで対象外（`tests/trait_test.rs` 参照）。
