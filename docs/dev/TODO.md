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
T3「ユーザ定義エラー型」、T4「動的ディスパッチ」、T5「`--heap-cells N`」、
T5「pretty printer」の Tier1/Tier2）は [implementation-log.md](implementation-log.md) 末尾へ移設した
（T1/T5 は 2026-07-23、T2 は 2026-07-24、T3/T4 は 2026-07-25、pretty printer は 2026-07-26）。

### T5-b. `set-pprint-dispatch` / `*print-pprint-dispatch*`（優先度: 低）

pretty printer 本体（T5 の Tier1/Tier2）は 2026-07-26 に実装済み
（[implementation-log.md](implementation-log.md) 末尾、利用者向け仕様は
[functions.md](../functions.md) §15.1）。`*print-pretty*`/`*print-right-margin*`/
`*print-miser-width*`、`format` の `~_`/`~i`/`~:t`/`~<...~:>`、`pprint`/`pprint-fill`/
`pprint-linear`/`pprint-tabular`、`pprint-logical-block`/`pprint-newline`/`pprint-indent`/
`pprint-tab`/`pprint-pop`/`pprint-exit-if-list-exhausted` はすべて使える。

**残っているのは Tier3 だけ**——「実行時の型ごとに整形関数を登録し、`print`/`write`/`~a` が
呼び出し側の関与なしにそれを自動選択する」`set-pprint-dispatch` と、その登録表
`*print-pprint-dispatch*`。

#### 前提の再評価（2026-07-26、実装してみて分かったこと）

以前このタスクは「前提は T4 動的ディスパッチ ＋ 可変 pretty ストリーム値型の新設」と記録していた。
実際に作ってみると**どちらでもなかった**:

- T4（`:dyn Trait`）は 2026-07-25 に入ったが、登録表の索引付けに `:dyn` は要らない。
  型名（`string`）→ 関数値の `HashTable` で足りる。
- 「可変 pretty ストリーム値型」も要らなかった。開いている論理ブロックを**インタプリタの暗黙状態**に
  する（GC ヒープと同じ扱い）ことで、新しい値型ゼロで Tier2 を実装できた。

本当に残っている前提は次の3点:

1. レンダラ経路全体に `&mut Heap` を通すこと。現在 `format::build`/`render_value`/`pprint::render` は
   `&Heap` で、ユーザ関数の呼び戻しには `&mut Heap` が要る（呼び出し元の
   `Interp::eval_builtin` には既に `&mut Heap` があるので、機械的だが広い変更）。
2. Rust から typelisp の関数値を呼ぶ橋。`Expr::Apply` の compiled-closure 経路
   （`encode_crossing_args` → `call_closure_box` → `decode_compiled_return`）を、AST ノードから
   切り離して再利用できる形にする必要がある。
3. **呼び戻し中に宙に浮く `Value` の GC ルート保護**。これが本当の難所——レンダラは走査中のリスト要素を
   Rust の `Vec<Value>` に保持しており（`pprint::list_items`）、そこからユーザコードを呼べば確保が
   起きて回収されうる。印字経路は全プログラムが通るので、ここに微妙な GC バグを入れると影響が広い。

3 を安全に片付ける設計（走査中の値をセル or ルートスタックへ退避する等）が決まってから着手すること。
なお「ユーザ定義型ごとの整形を自分で書いて明示的に呼ぶ」だけなら、Sexpr への暗黙変換と `match` の
downcast パターンで今でも書ける（自動選択でない、という点だけが違う）。

- 関連: [[typelisp-format-directives]]（`format` 実装）、[[typelisp-pretty-printer]]。

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
`unsupported` は 2026-07-16 時点で解消済み（2026-07-25 に追加した `Expr::DynBox`/`DynCall`/
`DynValue` も同日中に本実装した）——残る `Expr::TraitCall` は単型化後に到達不能な
診断専用ノードと確認済みで対象外（`tests/trait_test.rs` 参照）。
