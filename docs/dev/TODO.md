# typelisp 開発 TODO / 引き継ぎ

最終更新: 2026-07-26 / ブランチ: `main`

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
T3「ユーザ定義エラー型」、T4「動的ディスパッチ」、T5「`--heap-cells N`」、T5「pretty printer」、
T5-b「型ごとの印字表現」）は [implementation-log.md](implementation-log.md) 末尾へ移設した
（T1/T5 は 2026-07-23、T2 は 2026-07-24、T3/T4 は 2026-07-25、pretty printer と T5-b は 2026-07-26）。

**これで当初の T1〜T5 はすべて片付き、着手候補として格上げされた TODO は残っていない。**

なお、**Common Lisp と比べたときにまだ無いクラス（型）・メソッドの全リスト**は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md)（2026-07-29 作成）に
棚卸ししてある。あちらは TODO ではなく地図（残差の測定）で、各項目を「未実装だが追加可能」/
「部分実装・差異あり」/「設計上の対象外」に仕分けてある。次に何かを足すなら同ドキュメント §3
（横断的な欠落）が費用対効果の目安になる。

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
- **`set-pprint-dispatch` / `*print-pprint-dispatch*`**: CL の「型指定子をキーにした実行時の
  整形関数登録表」。文字列キーもプリンタのシグネチャも無検査で、「登録時点で分かっていた型を
  捨ててから `match` で復元する」形になり、静的型付け言語には合わない——CL のもう一方の機構
  である CLOS 総称関数 `print-object` に相当する **`print-object` トレイト**を 2026-07-26 に
  採用してこちらを置き換えた（[functions.md](../functions.md) §15.2）。判断の経緯は
  [implementation-log.md](implementation-log.md) 末尾。

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
`unsupported` は 2026-07-16 時点で解消済み（2026-07-25 に追加した `Expr::DynBox`/`DynCall`/
`DynValue` も同日中に本実装した）——残る `Expr::TraitCall` は単型化後に到達不能な
診断専用ノードと確認済みで対象外（`tests/trait_test.rs` 参照）。
