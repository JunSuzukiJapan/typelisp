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

完了した項目（T1「`format` の書式指定子」、T2「`defmacro` の `&optional`/`&key`」、
T5「`--heap-cells N`」）は [implementation-log.md](implementation-log.md) 末尾へ移設した
（T1/T5 は 2026-07-23、T2 は 2026-07-24）。

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
- T5 の pretty printer の Tier2/3（`pprint-logical-block` 等の公開・`set-pprint-dispatch`）は
  この T4 を前提にする（2026-07-23 結論。理由は下記 T5 参照）。

### T5. pretty printer（CL の Lisp Pretty Printer 相当）（優先度: 低）

CL は ANSI 標準の pretty printer を持つ（CLHS 22.2、実体は R. Waters の XP アルゴリズム
= "XP: A Common Lisp Pretty Printing System", MIT AI Memo 1102a, 1989。SBCL の
`src/code/pprint.lisp` 等が同系統）。`format` 実装（[implementation-log.md](implementation-log.md)、
2026-07-23）ではこの系統のディレクティブを未対応（no-op / 近似）にしてある。**pretty printer 本体を
別タスクとして切り出す**（2026-07-23、`~i`/`~_` 等の議論で棚卸し）。

#### CL の実装の要点（調査メモ、2026-07-23）

「木を組んでレイアウトを後計算」ではなく、**本物の出力ストリームをラップした pretty-stream に書き込みを
バッファしながら、有界の先読みで改行を確定する1パスのストリーム方式**（線形時間・行幅程度の有界メモリ）。

- pretty-stream の状態: 未確定文字バッファ、その先頭桁、開いている論理ブロックのスタック
  （各ブロックが prefix / per-line-prefix / suffix / インデント量を保持）、バッファ位置に紐づく
  **命令キュー**（`block-start` / `block-end` / `newline`〈`:linear`/`:fill`/`:miser`/`:mandatory`〉/
  `indentation`〈`:block`/`:current`〉/ `tab`）。`newline` と `block-start` は共通の section-start
  として `depth` と後埋めの前方ポインタ `section-end` を持つ。
- 中心のトリック: 条件改行の場で改行可否は決められない（そのセクションが行に収まるか未確定）。
  セクション末尾が来る前にバッファ長が右マージンを超えたら**折る**、セクション末尾が先に来たら**折らない**。
  先読みは現セクション末尾までで足りるのでバッファは行幅で頭打ち。
- 改行種別: `:linear`=囲みセクション全体が収まらなければ折る（同一セクションで揃う）／`:fill`=次の
  部分区間が収まらない時だけ折る（語詰め）／`:miser`=miser モード（右マージンから
  `*print-miser-width*` 以内で行が始まった時）だけ折る／`:mandatory`=常に折る。
- `write`/`print` は `*print-pretty*` が真のとき整形経路に入り、**`*print-pprint-dispatch*`** を引いて
  オブジェクト型に対応する整形関数を呼ぶ（既定にリスト用・`quote`/`let`/`defun` 等の特殊形専用の
  整形関数が登録済み）。`set-pprint-dispatch` でユーザが登録＝**実行時型→任意関数の動的ディスパッチ**。
- CL では `format` の pretty 系ディレクティブは `pprint-*` API のシンタックスシュガー
  （`~<...~:>`→`pprint-logical-block`、`~_`→`pprint-newline`、`~I`→`pprint-indent`、
  `~:T`→`pprint-tab`、`~W`→`write`）。実体はすべて XP の pretty-stream に落ちる。

#### 段階分け（2026-07-23 の議論で確定した着手方針）

未対応項目を、typelisp への収まりの良さで3段に分ける:

- **Tier1（ストリーム値型を新設せず format 経由で提供。単独で着手可、これを土台にする）**
  - 特殊変数 `*print-pretty*` / `*print-right-margin*` / `*print-miser-width*` を prelude の `defvar`
    グローバルとして持ち、`setf` で変更・`run_format` で読む（typelisp に CL の `let` 動的束縛は無いので、
    動的束縛ではなくグローバル代入で代替する）。
  - `format` の pretty 連動ディレクティブ: `~_`（条件改行。`~:_`=fill/`~@_`=miser/`~:@_`=mandatory/
    素=linear）、`~i`（インデント。`~n:i`=current）、`~<...~:>`（**論理ブロック**用法。閉じに `:` が付く。
    現状の桁揃え `~<...~>` とは別物として分岐）、`~:t`（論理ブロック内タブ）、`~w`（`*print-pretty*` 準拠。
    現状は単なる `prin1`）。
  - 関数 `pprint` / `pprint-fill` / `pprint-linear` / `pprint-tabular`（S式を既定レイアウトで整形出力）。
  - format は既にインメモリ `String` を構築する方式なので、XP のストリーム層を厳密再現せず、構築中の
    バッファ上で同じ先読み判定を回す簡略版で同等結果を出せる（ストリーム値をユーザに露出しない範囲）。

- **Tier2/3（T4 動的ディスパッチ導入後に着手する。← 2026-07-23 結論）**
  - `pprint-logical-block` / `pprint-newline` / `pprint-indent` / `pprint-tab`
    （+ `pprint-pop` / `pprint-exit-if-list-exhausted`）をユーザ呼び出し可能な関数として公開。
  - `set-pprint-dispatch` / `*print-pprint-dispatch*`。
  - **T4 を前提にする理由**: CL でこれらが自然に効くのは (a) CL がもともと第一級ストリームを至る所で
    持ち、(b) 動的ディスパッチがあるから。typelisp は現状どちらも無い——公開するには言語に無い新概念
    「可変 pretty ストリーム値型」を新設せねばならず（過去に `Vector`/`RtValue` 専用バリアントを
    「ユーザ定義型と同様に扱うべき」で作り直した方針とも衝突しうる）、最大の見返り（ユーザ定義型の独自
    プリンタが `print`/`write` で**自動選択**される）は `set-pprint-dispatch`＝実行時型→任意関数の
    動的ディスパッチ、すなわち上記 T4 そのものを要する。
    T4 抜きで公開しても「ユーザが自分の型を手動整形するとき明示的に呼ぶ」に留まり中途半端になるため、
    T4 と一緒に扱う。
  - **2026-07-24 追記**: 「Sexpr にユーザ定義型を入れる」プラン
    （`~/.claude/plans/async-conjuring-hanrahan.md`）の実装により、`defstruct`/`defenum` インスタンスは
    暗黙に `Sexpr` へ変換でき、`match` の downcast パターン（型名先頭/裸enum変種/`(the T p)`）で
    実行時型ごとの分岐がユーザコードから**手動で**書けるようになった。これは
    `set-pprint-dispatch`＝**自動選択**（`print`/`write` が呼び出し側の関与なしに登録済み整形関数へ
    振り分ける）とは別物——ユーザが自分の `pprint-my-type` 相当の関数内で `(match v ((point x y) ...)
    ((circle r) ...) ...)` と手書きすれば型ごとの整形は今でも書けるが、`print`/`write` 自身が
    その関数を"知って"呼び出す仕組み（＝実行時型→関数の索引付きディスパッチテーブル）は依然として
    存在しない。したがって上記の T4 依存という結論そのものは変わらない——ただし「ユーザ定義型を
    Sexpr に入れて match で分岐する」という土台コードは今回のプラン実装で先に揃ったので、T4 着手後の
    `set-pprint-dispatch` 実装コストは軽減される見込み。

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
