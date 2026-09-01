# typelisp 開発 TODO

最終更新: 2026-09-01 / ブランチ: `feature/remove-i64`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

**CL 残差を埋める実装計画**（[cl-parity-plan.md](cl-parity-plan.md)、2026-08-20 策定）。
Common Lisp にあって typelisp に無いものを Phase 0〜9 に落とした計画。
着手順の目安は同計画の「実施順序と依存」節。

進捗:

| Phase | 状態 |
|---|---|
| 0（実装前に確かめる 6 件） | **完了** 2026-08-20。結果と、それが計画本体に強いた訂正は同計画の該当節 |
| 1（数値層 1a〜1d） | **全完了**（1a/1b/1d は 2026-08-21）。1c の保留 2 項目（乱数のシード指定・`ldb` 系の幅拡張）はそのまま。1d の complex は prelude の `defstruct`（成分は `f64` 固定、`(sqrt -1.0)` は実数 NaN のまま——[implementation-log.md](implementation-log.md) 該当節）。Phase 2 が残した「char/int の native lowering 5 つ」は未着手——1a と同じ島の分岐を触る作業 |
| 2（文字・文字列 2a/2b） | **完了** 2026-08-20 |
| 3（リスト・シーケンス 3a〜3e） | **全完了**（3e は 2026-08-21）。3e は Phase 5b に依存しなかった——対象は全部ジェネリック `defun` で、`defun` の `&key` は既に通っていた。まだキーワードを取れないのは `defmethod` の側（破壊的操作と `search`/`mismatch`）——Phase 5b で機構は入ったので、あとは対象の書き換えだけ |
| 4（制御構造・マクロ） | **4a 部分完了** 2026-08-20（`prog1`/`prog2`/`do*`/`ecase`/`ccase`/`setq`/`psetq`/`psetf`/`pushnew`）。残るは `block`/`return-from`（島の引数引き回しに全面的に触る）・`prog`/`prog*`・`destructuring-bind`・`sleep`、および `loop` の `:named`（`block` 依存）。**4b 完了** 2026-08-21（`:named` を除く拡張 `loop`）。**4c 部分完了** 2026-08-21（`macroexpand`/`macroexpand-1`、`complement`、`gensym` のプレフィクスと `*gensym-counter*`）——残るは `macrolet`/`symbol-macrolet` のみで、やり方は同計画に書いた（他の項目は「入れない」理由つきで確定） |
| 5（定義形の拡張 5a〜5c） | **5b 完了** 2026-08-21（`defmethod` が `&optional`/`&key`/`&rest` を取る。`lambda`/`labels` とトレイトのメソッドは「入れない」理由つきで確定）。**5c 完了** 2026-08-21（`deftype`。型の綴りであって型ではない——展開は型パーサの中で起き、エラーメッセージも展開後を見せる）。**5a 完了** 2026-08-21（オプションリスト・スロットのデフォルト・`:constructor`・`:copier`・`:include`。`:conc-name` と `:predicate` は「入れない」理由つきで確定）。**Phase 5 全完了** |
| 6（コレクション 6a〜6c） | **6a 部分完了** 2026-08-22（`Hash` トレイト＋`sxhash`、鍵のハッシュ可能性が静的に、`maphash`/`size`）。残るはユーザ定義型を鍵にする分（mem 層のバケットが要る）。**6b 完了** 2026-08-22（`Array<T>`。`Vector` 2 本の上の prelude `defstruct` で、Rust 側の追加はゼロ。`(aref a i j)` だけ checker の糖衣）。**6c 完了** 2026-08-22（`BitVector`。1 語 **32bit** ——64bit は上の整数切り詰めに当たる） |
| 7（エラーと動的束縛 7a/7b） | **全完了** 2026-08-22。7a は `SimpleError`/`WrappedError`/`wrap-error`/`describe-error`/`assert`/`warn`（コンディションシステムは予定どおり非採用）。7b は `dlet`（保存→代入→`unwind-protect` で復元）と印字制御変数一式＋`with-standard-io-syntax`、副産物で radix リーダマクロ `#b`/`#o`/`#x`/`#NNr`。入れなかった変数は同計画の表に 1 つずつ理由つき |
| 8（印字とリーダ 8a〜8c） | **8a/8b 完了** 2026-08-23（プリンタとリーダ）。この Stage で分かった 3 件——ジェネリック型に `print-object` が発火しない・`~/name/` が AOT で使えない・それに依存して見送った `*print-array*`／`Array<T>` の `print-object`——は下に別項で書いた。**残るは 8c**（`readtable` とリーダマクロ）で、これだけは先行条件が別物: 「全フォームを読んでから検査／評価する」を**フォーム単位の「読む→チェック→評価」ループ**に転換するのが先で、影響範囲は `prelude::load_interpreted_with`・`project.rs`・`main.rs`・LSP の各ドライバ。転換自体を独立した Stage として切る |
| 9（シンボル・パッケージ・環境） | **9c 完了** 2026-08-20（コマンドライン引数・環境変数・ファイルシステム問い合わせ・日時の分解合成・`y-or-n-p`）。保留は `libc` が要る 4 群と REPL ツール層。9a/9b/9d 未着手 |
| 付録 C（小さな不整合 4 件） | **完了** 2026-08-20 |
| 付録 D（範囲外の既存問題 2 件） | **完了** 2026-08-21。D-2 は `Heap::cons` の成長条件（回収後の空きが 1/4 未満なら伸ばす）、D-1 は AOT 実行ファイルが prelude を持ち歩くように |

2026-08-27〜30 の 4 件（[implementation-log.md](implementation-log.md) の該当節）は
**残作業を 1 つも足していない**。null 排除（`Sexpr` の `nil` を `Option` へ）は
[null-elimination-plan.md](null-elimination-plan.md) が 2026-08-28 完了・08-30 再点検で
「残作業は無い」と結論している。型 identity とシンボルの比較は文字列からインターン済み ID／
ポインタになり、残した文字列は理由つきで同ログにある。`sexpr-*` 抽出子の型を狭める案
（A: 引数を `Sexpr` に、C: フロー依存の絞り込み）は**どちらもやらないと結論した**——
`Sexpr` は 11 変種を束ねた 1 つの型なので、狭めても島が落ちる 2 経路（cons か・Int か）は
どちらも言えないまま。島のブートストラップが単型化の束を見ていなかった件は修正済みで、
成果物は md5 同一。

2026-08-22 に Phase 6b/6c の締めとして**直列の全実行**（`scripts/test-serial.sh`、97 本）を
初めて回したところ、6b/6c と無関係な赤が 3 件出た。3 件とも前フェーズの取りこぼしで、
`cargo check` は通っていた。内訳と、うち 1 件が本物の言語バグだった話は
[implementation-log.md](implementation-log.md) の該当節。**フェーズの締めは「関係しそうな
テストを選んで回す」ではなく直列の全実行にする。**

**64bit 幅の整数型を言語から消した**（2026-09-01、[remove-i64-plan.md](remove-i64-plan.md)）。

Phase 6a/6c で見つかった**コンパイル済みコードの整数切り詰め** 3 件のうち、リテラル以外の
2 件（±2^60 の外にある `i64` のグローバル、および `Vector`/`HashTable`/`defstruct` の
フィールドへの格納）は**原因そのものを消して**閉じた。実行時の値は下位 3bit がタグの 1 語で、
即値の整数には 61bit しか残らない——**タグに干渉する幅の型を作らない**のが直し方だった
（多くの Lisp が整数を 1 種類に絞ってタグ用の bit を確保しているのと同じ理屈）。
`i64`/`u64`/`isize`/`usize` を削除し、残るのは `i8`/`i16`/`i32`/`u8`/`u16`/`u32` の 6 つ。
`i32` より大きい数は `bignum`。

同じ作業で、**型名は幅と符号そのもの**にした。`i32` は「32bit を符号付きとして扱う」、
`u32` は「32bit を符号なしとして扱う」以上の意味を持たない——`(+ (the u8 200) (the u8 100))`
は 44、`(+ 2147483647 1)` は `-2147483648`。**`f32` も本物の binary32** で、
`(/ (the f32 1.0) (the f32 3.0))` は `f64` の答えとは違う。これで
[functions.md](../functions.md) §1b の「幅は静的な区別だけで、実行時表現は共通」という
明文の方針は全面的に撤回された。

単なる置換で済まなかった 7 箇所の結末:

- **core IR の 32bit 半分**: 両半分を符号付き `i32` のビットパターンで運ぶ。島側は
  `shl 32` -> `lshr 32` で符号拡張を落とす——マスク定数 `0xFFFF_FFFF` 自体が書けない。
- **島の `const-i64`** -> `const-word`。生成コードの語幅はソース言語の型と無関係。
- **`most-positive-fixnum`** は `i32` の上下限。
- **`sxhash`** は `i32` を返す。FNV-32 は「`i32` の演算が mod 2^32」であることを使って
  マスク無しの本物になった。
- **`BitVector`** は 1 語 31bit（`i32` の 32 番目の bit は符号）。
- **時刻**は構造体: `universal-time`(day/second)、`internal-time`(second/microsecond)。
- **整数演算の幅**: 値は常に「型が名乗る数」を 64bit の語に符号／ゼロ拡張して持つ。
  この不変条件があるので比較・除算・剰余は符号なしでも符号付き 64bit 命令のままでよい。

**この作業で新しく分かったこと**が 3 つある。

1. **自己ホストの改名は 1 世代では通らない。** 島の成果物は「前の世代の .bc が新しい
   SOURCE をコンパイルする」形で作るので、op-id が変わる改名（`const-i64` ->
   `const-word`）や引数の増減（`build-make-closure` のマスク 2 分割、`rt_int_*` の
   `wsig` オペランド）は、旧 .bc を走らせられなくする。再生成の間だけ旧名を通す
   一時シムを入れ、不動点まで回してからシムを外してもう 1 回回した。
   **島の再生成は 2 回で足りるとは限らない**（[typelisp-island-regen-fixpoint]）の一段上。
2. **`int->float` がコンパイルできていなかった。** prelude が使っていなかったので
   露見していなかっただけで、`internal-time-seconds` が初めて使った時点で
   PRELUDE_COMPILE_UNSUPPORTED が空でなくなった。`build-sitofp` を足して閉じた。
3. **~~`u32` のリテラルが 2147483647 までしか書けない。~~** → 2026-09-01 解消。
   リーダではなくチェッカーで解いた。リーダは今までどおり `i32` 超を `bignum` に読み
   （`*print-radix*` の往復が保てる）、**チェッカーが期待型を知っている場所で受け直す**——
   `bignum` リテラルの腕で、期待型が固定幅整数でその数が収まるならその型の整数literalにする。
   `(the u32 4294967295)` も `(the u32 #xFFFFFFFF)` も書ける。同時に**リテラルの範囲検査**を
   入れた（`int_lit_in_range`）：`(the u8 300)` はそれまで `u8` が 300 を持つ値を作っており、
   幅を意識する演算すべてと食い違っていた。切り詰めは `(as u8 300)` と明示する。

トップレベル `defun` の**前方参照は廃止**した（2026-08-23）。相互再帰は
`(defsignature name (型...) 戻り型)` で明示的に宣言する。理由は Phase 8c
（リードテーブルとリーダマクロ）との衝突で、旧機構 `Checker::predeclare_program` は
「最初のフォームを検査する前に全フォームを**読む**」ことを要求し、リーダマクロは
「フォーム *k* を実行してから *k+1* を読む」ことを要求する。詳細は
[implementation-log.md](implementation-log.md) の該当節。

`print-object` がジェネリック型に発火しなかった件は**直した**（2026-08-31、
[type-identity-instantiation-plan.md](type-identity-instantiation-plan.md)）。
値の実行時型キーが実体化を含むようになり（`gen<i32>`）、単型化が登録する
`print-object <i32>` を引けるようになった。副産物で downcast の不健全
（`(the gen<i32> x)` が `gen<string>` を通していた）も閉じた。

締めの直列全実行（`scripts/test-serial.sh`）で**この作業の取りこぼしが 3 種類**出た。
`cargo test` を個別スイートでしか回していなかったために見えなかったもので、
「`cargo check` が通っただけでは完了ではない」の一段上の形——**個別のスイートが
全部 green でも完了ではない**:

1. **`cargo test --lib` がコンパイルすら通っていなかった。** `core_eval.rs` の
   `#[cfg(test)]` の中に `alloc_builtin_fn` の呼び出しが 1 つ残っていた。
2. **手書きの core IR を持つ lib テスト 27 本**（`core_bridge` と `core_eval`）が
   新しい鍵フィールドを知らなかった。統合テスト側（`compile_test.rs`）は直したのに、
   同じ形のものが lib の中にもあった。
3. **`try-as i64` が実際に壊れていた**（`as_conversion_test` 2 本）。`Checker::check_as` は
   `i32` 幅で登録された変換を呼んで戻り型だけ `i64` に**付け替える**が、
   付け替えていたのは静的型だけで、`assoc` ノードが運ぶ**実行時の鍵**は `option<i32>` の
   ままだった。`option<i32>` の箱は `option<i64>` のパターンに 1 つも当たらない。
   `retype_option` が鍵も動かすようにして解消。**この付け替え自体、2026-09-01 に
   `i64` を消したときに消えた**——幅どうしの `as` が本物の変換になったので、
   `retype_option` ごと不要になった。

ついでに**組み込みのジェネリック型にも広げた**: `impl print-object Vector<T>` は
型検査を通り名前で呼べるのに選ばれない、という同じ穴が残っていた。`Vector::new` は
`construct` ノードを作らず Rust 側で箱を建てるので、特殊化の要求を `assoc`/`methodref`
ノード側にも置いた（`Checker::request_print_object`）。

**残り**: `*print-array*` と `Array<T>` の `print-object`（cl-parity-plan.md Phase 8a）は
この穴に依存して見送っていたので、**もう入れられる**。

Phase 2/3 の副産物として checker のバグを 4 件見つけて直した。4 件とも
**「型変数の名前がたまたま一致したときだけ動いていた」同じ形**（詳細は同計画の Phase 0 / Phase 3 の節）:
境界越しの `Self` 戻り型、境界付きジェネリック同士の委譲、ジェネリック `defmethod` の受け手の
型パラメータ名、そして compiled 経路で `format` が `f64` パラメータを生ワードのまま渡していた件。

Phase 9c の作業中に見つけた**計画の範囲外の既存問題 2 件**（同計画の付録 D）は
2026-08-21 に両方入れた。経緯は [implementation-log.md](implementation-log.md) の
「付録 D の 2 件」節。D-2 のほうで 1 つ分かったことを残しておく:
`editor_keyword_sync_test` の 649 秒は GC スラッシュ**だけ**が原因ではなく、
修正後も 502 秒かかる。残りは 6 テストがそれぞれ prelude を JIT していることで、
この 1 本を速くしたければ次はそちらを見る。

2026-08-20 に「ダンプ」（ビットコードと型情報を 1 ファイルに対で持つ）を入れ、起動は
1.50s → 1.07s になった。経緯は [implementation-log.md](implementation-log.md) の該当節。

2026-08-19 に「呼ぶとコンパイルできなくなるもの」（[syntax.md](../syntax.md) §10）の最後の1つ
`eval` を閉じ、あの表は空になった。経緯・設計判断は
[implementation-log.md](implementation-log.md) の該当節。

作業を始めるときはここに項目を足し、終わったら（経緯・設計判断を
[implementation-log.md](implementation-log.md) へ書いたうえで）ここから消す。

残差そのものの地図は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md)（TODO ではなく測定）。
そこから作った実行計画が上記の [cl-parity-plan.md](cl-parity-plan.md) で、地図の全行が
どの Phase に落ちたか（落ちていないなら理由）は同計画の付録 A にある。

## 関連ドキュメント

| 知りたいこと | 参照先 |
|---|---|
| 完了した実装の経緯・設計判断 | [implementation-log.md](implementation-log.md) |
| 言語仕様の確定事項・非採用と決めた機能 | [language-design.md](language-design.md)（非採用リストは §9） |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| それを埋める実行計画（Phase / 対象外の理由 / 完了判定） | [cl-parity-plan.md](cl-parity-plan.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |
