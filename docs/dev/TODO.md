# typelisp 開発 TODO

最終更新: 2026-09-04 / ブランチ: `main`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

**CL 残差を埋める実装計画**（[cl-parity-plan.md](cl-parity-plan.md)、2026-08-20 策定）。
Common Lisp にあって typelisp に無いものを Phase 0〜9 に落とした計画。
着手順の目安は同計画の「実施順序と依存」節。

進捗:

| Phase | 状態 |
|---|---|
| 0（実装前に確かめる 6 件） | **完了** 2026-08-20。結果と、それが計画本体に強いた訂正は同計画の該当節 |
| 1（数値層 1a〜1d） | **全完了**（1a/1b/1d は 2026-08-21）。1c の保留 2 項目は **2026-09-03 に両方解消**——乱数は `seed-random-state`、`ldb`/`dpb`/`boole` は整数を第 1 引数に移して `Bits` 境界付きの総称に（`logbitp` も同じ並びへ）。途中で `ash` のバグも直した（第 2 引数が値と同じ型で、符号なし幅に右シフトが無かった。距離は `i32` に）。1d の complex は prelude の `defstruct`（成分は `f64` 固定、`(sqrt -1.0)` は実数 NaN のまま——[implementation-log.md](implementation-log.md) 該当節）。Phase 2 が残した「char/int の native lowering 5 つ」は **2026-09-03 完了**——直後に [syntax.md](../syntax.md) §10 の「コンパイルできない組み込み」の表も**空にした**——`string::upcase`/`downcase`、`try-*` 系（`Option` を返す変換）、`bignum` の残りのビット演算の 3 群。どれも prelude から到達しないので `PRELUDE_COMPILE_UNSUPPORTED` には現れず、ユーザーが自分で呼んだときだけ出る穴だった |
| 2（文字・文字列 2a/2b） | **完了** 2026-08-20 |
| 3（リスト・シーケンス 3a〜3e） | **全完了**（3e は 2026-08-21、`defmethod` 側は 2026-09-03）。3e は Phase 5b に依存しなかった——対象は全部ジェネリック `defun` で、`defun` の `&key` は既に通っていた。残っていた `defmethod` 側（破壊的操作と `search`/`mismatch`）も CL のキーワード集合を持つ。`search` のキーワードだけ番号でなく名前にした（受け手が先なので CL の番号は逆の意味になる） |
| 4（制御構造・マクロ） | **4a 部分完了** 2026-08-20（`prog1`/`prog2`/`do*`/`ecase`/`ccase`/`setq`/`psetq`/`psetf`/`pushnew`）。**`block`/`return-from` は 2026-09-04 完了**（`tests/block_test.rs` 26 本、うち 5 本はコンパイル経路・1 本は `gc_stress`。島の引数列は 3 本増え、引き回しは 162 箇所。いちばん重かったのは引き回しではなく `unwind-protect` との相互作用で、静的脱出が cleanup を走らせる仕組みが「脱出先は 1 つ」を前提にしていた。`forms.rs` の既知のルート漏れの窓に 5 件目を落として `gc_stress` で釣った）。残るは `prog`/`prog*`・`destructuring-bind`・`sleep`、および `loop` の `:named`。**4b 完了** 2026-08-21（`:named` を除く拡張 `loop`）。**4c 完了** 2026-09-03（`macroexpand`/`macroexpand-1`、`complement`、`gensym` のプレフィクスと `*gensym-counter*` が 2026-08-21、`macrolet`/`symbol-macrolet` が 09-03。他の項目は「入れない」理由つきで確定） |
| 5（定義形の拡張 5a〜5c） | **5b 完了** 2026-08-21（`defmethod` が `&optional`/`&key`/`&rest` を取る。`lambda`/`labels` とトレイトのメソッドは「入れない」理由つきで確定）。**5c 完了** 2026-08-21（`deftype`。型の綴りであって型ではない——展開は型パーサの中で起き、エラーメッセージも展開後を見せる）。**5a 完了** 2026-08-21（オプションリスト・スロットのデフォルト・`:constructor`・`:copier`・`:include`。`:conc-name` と `:predicate` は「入れない」理由つきで確定）。**Phase 5 全完了** |
| 6（コレクション 6a〜6c） | **6a 完了**（`Hash` トレイト＋`sxhash`、鍵のハッシュ可能性が静的に、`maphash`/`size` が 2026-08-22、**ユーザ定義型を鍵にする分が 2026-09-03**）。表は `hash -> バケット`になり、`get`/`set`/`remove` は prelude の `defmethod` へ移った。**6b 完了** 2026-08-22（`Array<T>`。`Vector` 2 本の上の prelude `defstruct` で、Rust 側の追加はゼロ。`(aref a i j)` だけ checker の糖衣）。**6c 完了** 2026-08-22（`BitVector`。1 語 **31bit** ——32 番目の bit は `i32` の符号） |
| 7（エラーと動的束縛 7a/7b） | **全完了** 2026-08-22。7a は `SimpleError`/`WrappedError`/`wrap-error`/`describe-error`/`assert`/`warn`（コンディションシステムは予定どおり非採用）。7b は `dlet`（保存→代入→`unwind-protect` で復元）と印字制御変数一式＋`with-standard-io-syntax`、副産物で radix リーダマクロ `#b`/`#o`/`#x`/`#NNr`。入れなかった変数は同計画の表に 1 つずつ理由つき |
| 8（印字とリーダ 8a〜8c） | **8a/8b 完了** 2026-08-23（プリンタとリーダ）+ 2026-09-03（`*print-array*` と `Array<T>` の `print-object`）。この Stage で分かった 3 件のうち、ジェネリック型に `print-object` が発火しない件は 2026-08-31 に、それに依存して見送っていた `*print-array*` は 09-03 に閉じた。残るのは `~/name/` が AOT で使えないことだけ。**先行条件のアーキテクチャ転換は 2026-09-04 完了**——全ドライバがフォーム単位で「読む→チェック→評価」するようになった（`tests/read_check_eval_test.rs` 16 本）。**残るは 8c 本体**（`readtable` とリーダマクロ） |
| 9（シンボル・パッケージ・環境） | **9c 完了** 2026-08-20（コマンドライン引数・環境変数・ファイルシステム問い合わせ・日時の分解合成・`y-or-n-p`）。保留は `libc` が要る 4 群と REPL ツール層。9a/9b/9d 未着手 |
| 付録 C（小さな不整合 4 件） | **完了** 2026-08-20 |
| 付録 D（範囲外の既存問題 2 件） | **完了** 2026-08-21。D-2 は `Heap::cons` の成長条件（回収後の空きが 1/4 未満なら伸ばす）、D-1 は AOT 実行ファイルが prelude を持ち歩くように |

**cl-parity-plan.md の残り 5 件を埋めた**（2026-09-03）。8a の `*print-array*`、3e の
`defmethod` 側、Phase 1 の char/int lowering、4c の `macrolet`/`symbol-macrolet`、
6a のユーザ定義型キー。経緯と設計判断は各 Stage の節（[cl-parity-plan.md](cl-parity-plan.md)）と
[implementation-log.md](implementation-log.md) に書いた。**この 5 件が「ついでに」暴いた
既存のバグが 3 件**あり、そちらのほうが記録の価値がある:

1. **`where` 境界の受け手が、引数を入れ替えて別の型に当てられていた。**
   `try_instance_method` の覗き見は境界を見ないので型変数の受け手を解決できず、
   `try_instance_method_swapped` が「では引数が逆順なのだろう」と第 2 引数の型に
   当てる。`bool` が `print-object` を実装した瞬間、`(print-object x true)` が
   「expected Bool, found t」——**誰も書いていない呼び出しについての診断**——で落ちた。
   潜在期間の長さではなく、*新しい impl が 1 つ増えるだけで発火する*ことが怖い形。
2. **特殊化の名前に埋まったモジュール修飾された型引数**。`hashtable::get <m::pt,i32>`
   のような名前を `Interp::method_key` が `rsplit_once("::")` で切ると、型引数の内側で
   切れる。**ルートで書いたプログラムでは絶対に再現しない**（型が 1 セグメントなので）。
   [[typelisp-type-identity-invariant]] と同じ族で、`(module m ...)` を張って試す
   という当時の教訓がそのまま効いた。
3. **`(if cond (setf ...) ())` は型エラー**。`setf` は代入した値を返すので分岐の型が
   揃わない。新しく書いた prelude コードで 4 回踏んだ。既存コードは
   `(progn (setf ...) ())` か `when`/`unless` を使っている。

**設計として引き受けたことも 3 つ**:

- `print-object` は「印字できる」という**境界**になった。スカラ型 14 個に impl を
  足したのは、`format` の `&rest` が型変数を受け取れない以上、ジェネリックなコードが
  「この値は描画してよい」と言う手段が境界しかないから（Rust の `T: Display`）。
  プリンタがそれを引くことは無い——引くのは型キーを持つ値だけ。
- `HashTable` の `get`/`set`/`remove` は**組み込みをやめて prelude のメソッドになった**。
  キーをハッシュすることも 2 つのキーを比べることも、キーの型自身の typelisp メソッド
  なので、Rust のシムからは呼べない。計画は逆に「バケットは Rust 側に置くことになる」と
  書いていたが、その根拠（組み込みのシグネチャに `(K,V)` を書けない）は**事実として
  誤り**だった。副産物で `symbol` キーの実行時 panic も消えた。
- 破壊的シーケンス操作のキーワードは**転送できない**（キーワード引数は `Option` で
  届き、呼び先は裸の値を要求し、`Option<(fn ...)>` は型として書けない）。共有するのは
  引数リストではなくループの中身で、繰り返すのは 2 行。

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

**幅を捨てている設計を無くした**（2026-09-02、[preserve-bit-width-plan.md](preserve-bit-width-plan.md)）。

`i64` を消した直後の状態には、幅を**名前にも表現にも持たない**箇所が 3 つ残っていた。
`BoxedObj::Float` が 2 つの幅を 1 つの箱に畳み、`Sexpr` の `int`/`float` が 8 つの型を
2 つの変種に畳み、その帰結として `format` の `~/name/` は受け手の型を決められなかった。
`(the f32 0.1)` が `0.10000000149011612` と印字されるのはこの畳み込みの症状で、
「binary32 で表せるなら短く出す」という値からの検出は**不健全**（`0.10000000149011612`
自体が binary32 で表せる `f64`）。直し方は表現を分けることしかなかった。

- **箱を幅ごとに**: `BoxedObj::Float32`/`Float64`/`Narrow{width,signed,value}`。
  読み出しは `FloatBox`/`NarrowInt` で、**幅を言わずに float を取り出す API は無い**。
- **`Sexpr` の変種を型名そのものに**: `i8 i16 i32 u8 u16 u32 f32 f64`（12..16 は末尾に追加。
  変種番号は島の IR に焼き込まれている）。アクセサも `sexpr-i8` 〜 `sexpr-f64` の 8 本。
  `sexpr-int`/`sexpr-float` は削除。**`Sexpr` は値の型がほかのどこにも書かれていない
  唯一の場所**なので、ここだけは幅ごとの変種と箱が要る——静的に型が分かる位置
  （局所変数・引数・`defstruct` フィールド）の `u8` は今までどおり素の正規化済みワード。
- **`~/name/` は単一候補に**。「候補 6 つのうち 2 つ以上が定義していたら曖昧さエラー」は
  診断の顔をした推測だった。ついでに `f64`/`bignum`/`ratio` を受け手にした `~/name/` が
  「型名を持たない」で落ちていたのも直した（`Heap::primitive_box_type_name`）。

分かったこと 3 つ:

1. **`BUILTIN_SYMBOLS` は位置表**。`sym_index` は配列の添字で、コンパイル済みの島が
   それを焼き込んでいる。`INT`/`FLOAT` を改名して `I32`/`F64`/`F32` を表の途中に
   入れたら以降が全部ずれ、旧島が `compile-value: unsupported tag int` で落ちた——
   **自分が扱えるはずのタグを拒否する**という分かりにくい形で出る。新しい名前は末尾に
   追加し、旧エントリは `RETIRED_*` として位置ごと残す。削除も改名も添字を動かす。
2. **`Sexpr` の畳み込みは名前だけの問題ではなかった。** `int` 変種のフィールド型は `i32`
   固定で、`construct_sexpr` はフィールド型の検査を**意図的に迂回**して `(Int (the u8 200))`
   を通していた。`(the u32 4000000000)` を入れると `i32` が持てない数を持った値が出てくる
   ——正規化の不変条件が黙って破れる。迂回する理由は変種を型名ごとにした時点で消えた。
3. **S1 の穴が S3 の作業で見つかった。** `f32` 変種を足したとき
   `compile-sexpr-tag-test` に 11 の分岐を入れ忘れていて、compiled な `match` の
   `(f32 x)` 腕が「unknown Sexpr variant」で panic していた。テストを回していれば
   その場で出たもので、**「幅ごとに分ける」作業は分岐を足す場所が 1 つでは済まない**。
4. **`format` の `~/name/` が受け手を宣言型で渡していなかった。** 新しく足した
   テストが釣った本物のバグ。`Interp::format_call` は印字器が持っている `Sexpr` の
   値をそのままメソッドの受け手として `apply` していた。`u8` の受け手の実行時表現は
   素の正規化済みワードだが、`Sexpr` の中では `BoxedObj::Narrow` ——`f64` は両側とも
   箱なので今まで表面化しなかった。`params[0] == Repr::Int` なら箱を開ける、と
   **宣言型駆動**に直した。[typelisp-crossing-must-be-type-driven] と同じ形が 4 回目。

**直列全実行の教訓がもう 1 つ**: `cargo test --lib` は**最初に落ちた lib ターゲットで
止まる**。`typelisp` の lib が 46 件落ちていた裏に、`typelisp-front` の 43 件と
`check::repr` の 1 件が隠れていた。「個別のスイートが全部 green でも完了ではない」の
一段下で、**同じ `--lib` の中でも先に落ちたものが後ろを隠す**。

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

`*print-array*` と `Array<T>` の `print-object` はこの穴に依存して見送っていたもので、
2026-09-03 に入れた（下記）。

Phase 2/3 の副産物として checker のバグを 4 件見つけて直した。4 件とも
**「型変数の名前がたまたま一致したときだけ動いていた」同じ形**（詳細は同計画の Phase 0 / Phase 3 の節）:
境界越しの `Self` 戻り型、境界付きジェネリック同士の委譲、ジェネリック `defmethod` の受け手の
型パラメータ名、そして compiled 経路で `format` が `f64` パラメータを生ワードのまま渡していた件。

Phase 9c の作業中に見つけた**計画の範囲外の既存問題 2 件**（同計画の付録 D）は
2026-08-21 に両方入れた。経緯は [implementation-log.md](implementation-log.md) の
「付録 D の 2 件」節。そこに書いた
「`editor_keyword_sync_test` は修正後も 502 秒、残りは 6 テストがそれぞれ prelude を
JIT しているぶん」は**2 段階で解消した**。JIT 6 回ぶんは 2026-09-03 の `OnceLock`
キャッシュ（`924066a`）で 1 回になり、そのあとに残っていた数十秒は**テストの中に
無かった**——`target/debug/deps` に溜まった 177 万個の `.o` のせいで、そのディレクトリ
からの `exec` 自体が 21〜40 秒かかっていた。経緯は
[implementation-log.md](implementation-log.md) の 2026-09-04 の節、掃除の手順は
[development.md](development.md) の該当節。

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
