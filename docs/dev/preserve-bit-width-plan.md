# 幅を捨てている設計をすべて無くす

状態: 完了（2026-09-02、ブランチ `feature/preserve-bit-width`、S1〜S7 すべて）
前提: `docs/dev/remove-i64-plan.md`（64bit 幅の整数型の削除）が完了していること。

## 方針（決定事項）

**幅を持たない名前・表現・変換を残さない。** `sexpr-float` のように幅を言わない
名前は、それ自体がバグの温床であって、症状ではなく欠陥である。
手数と時間は理由にしない。

## いま幅を捨てている 3 箇所

### 1. `BoxedObj::Float` — 2 つの幅を 1 つの箱に畳んでいる

`f32` も `f64` も `BoxedObj::Float(f64)`。値は binary32 に丸められて入っている
（`remove-i64-plan.md` §3.8）が、箱は自分がどちらか言えない。帰結として
`(the f32 0.1)` が `0.10000000149011612` と印字される。

検出方式（「binary32 で表せるなら短く出す」）は**不健全**なので採らない:
`0.10000000149011612` は binary32 で表せる `f64` なので `0.1` と出てしまい、
読み戻すと別の `f64` になる。読み戻せる印字という性質が壊れる。

参照 64 箇所 / 19 ファイル、うち rt 側 26。

### 2. `Sexpr` の `int` / `float` — 8 つの型を 2 つの変種に畳んでいる

`checker.rs` の `sexpr_ctor_for` が 6 つの整数幅を `int` へ、2 つの浮動小数幅を
`float` へ落とす。変種のフィールド型は `i32` / `f64` 固定で、`construct_sexpr` は
**フィールド型の検査を意図的に迂回**して `(Int (the u8 200))` を通している。

`Sexpr` は包みではない。`(SEXPR_INT, Value::Int(_))` — `Value::Int` が*そのまま*
Sexpr の Int なので、**幅を書き留める場所が存在しない**。名前を直すだけでは
足りず、表現から直す。

`sexpr-int` は島に 44、テストに 27。`sexpr-float` はテストに 2。
`(int n)` パターンはテスト・例に 13、`(float f)` に 1。Sexpr を match する
テストは 12 本。

### 3. `format` の `~/name/` — 2 の帰結

値が幅を言えないので候補が 6 つになり、2 つ以上が定義していれば曖昧さエラー。
`remove-i64-plan.md` はこれが `i64` 削除で消えると予測したが、外れて候補が
2 から 6 に増えた。原因は候補の数ではなく、**値が幅を言えないこと**だった。

## 設計

### D1. `Sexpr` の変種を型名そのものにする

`int` / `float` を廃し、`i8 i16 i32 u8 u16 u32 f32 f64` にする。

変種番号は**島の IR とコンパイル済みコードに焼き込まれている**
（`registry.rs` の `sexpr_def` の doc）。付け替えは全成果物を無効にするので:

- 番号 1（`int`）を `i32` に、2（`float`）を `f64` に**改名**する。実行時表現が
  変わらないので成果物は壊れない。
- `i8 i16 u8 u16 u32 f32` は**末尾に追加**（11..16）。`bignum`/`ratio`/`path` が
  そうやって足されている前例に従う。

**訂正（着手して判明）**: 11 は空いていない。`Repr::field_kind` の番号は
`Sexpr` の変種番号**そのもの**で（`repr.rs` の doc が「この番号空間には
producer が 2 つある」と明記している）、`Unit` が「その番号付けの次」として
11 を借りている。`Sexpr` の次の変種 index は 11 なので衝突する。
`Unit` を番号空間の外（100）へ動かし、11 以降を `Sexpr` に明け渡す。
島の `compile-sexpr-field` / `compile-tag-struct-field` が 11 を unit として
見ているので、島の SOURCE も変わる。

`Sexpr` は 11 変種から 17 変種になる。`match` を書く側は整数を 6 通り書くことに
なるが、それは幅を持つ言語の当然の帰結であって、畳んで隠す理由にはならない。

### D2. 実行時表現

| 変種 | 表現 | 確保 |
|---|---|---|
| `i32` | 素の `Value::Int`（現状のまま） | なし |
| `i8` `i16` `u8` `u16` `u32` | `BoxedObj::Narrow { width, signed, value }` | あり |
| `f64` | `BoxedObj::Float64` | あり（現状のまま） |
| `f32` | `BoxedObj::Float32` | あり |

狭い整数を箱にするのは `bignum`/`ratio` と同じ道筋で、この codebase に既に
ある機構（確保・`is_*`・アクセサ・GC・等価・印字・`rt_*_new`/`rt_*_value`）を
そのまま使えるため。`TAG_IMMEDIATE` の空き 61 bit に副タグで詰める案は確保を
省けるが、ABI と島の codegen を変えるので後日の最適化として分ける。

**箱に入るのは Sexpr に入るときだけ**。静的に型が分かる位置の `u8` は今までどおり
素の正規化済みワードで、演算は何も変わらない。

### D3. アクセサ

`sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32`
`sexpr-f32` `sexpr-f64`。**`sexpr-int` と `sexpr-float` は削除**する。

### D4. 検査の迂回をやめる

`sexpr_ctor_for` は畳まない。`construct_sexpr` のフィールド型検査の迂回
（`checker.rs` の `&rest` 経路）を削除する。変種のフィールド型が実際の型と
一致するようになるので、迂回する理由がなくなる。

### D5. `format` の `~/name/`

値が幅を言えるようになるので候補は 1 つに定まり、曖昧さエラーが消える。
`Interp::format_call` と `Checker::format_call_owners` を単一候補に戻す。

## 進捗（2026-09-02）

- **S1 完了**: `BoxedObj::Float32`/`Float64`、`FloatBox`、`Repr::F32`/`F64`、
  `Sexpr` の `f32`(11)、核 IR の `int-any-width`/`float-any-width`、島の再生成。
  `(the f32 0.1)` が `0.1` と印字されるようになった。
- **S2 完了**: `BoxedObj::Narrow(NarrowInt)`。`Heap::alloc_narrow`/`narrow_box`/
  `is_narrow`、`rt_narrow_new`/`rt_narrow_value`、`rt_box_kind` の 5..9、
  `eql`（幅ごと）/`equalp`（幅を跨ぐ）、印字。`normalize_int` の 3 つ目の写しを
  `typelisp-mem` の 1 つに畳んだ。
- **S3/S4 完了**: `Sexpr` の 12..16（`i8 i16 u8 u16 u32`）、アクセサ
  `sexpr-i8` 〜 `sexpr-u32`、`sexpr_ctor_for` の畳み込み廃止、島の
  `compile-sexpr-tag-test`/`compile-sexpr-field`/`compile-construct-sexpr`。
  **S1 の穴も塞いだ**: `compile-sexpr-tag-test` に `f32`(11) の分岐が無く、
  compiled な `match` が `(f32 x)` の腕で panic していた。
- **S5 完了**: `~/name/` の候補が単一に。`Interp::format_call`/
  `aot_format_call`/`Checker::format_call_owners`。ついでに `f64`/`bignum`/
  `ratio` 受け手の `~/name/` が「型名を持たない」で落ちていたのも直した
  （`Heap::primitive_box_type_name`）。

- **S6/S7 完了**: テストの追随（核 IR の `(int …)`→`(int-any-width …)`、`Sexpr` の
  `(Int …)`→`(i32 …)`、`sexpr-int`→`sexpr-i32`）と直列全実行。
  幅ごとの往復テストを 5 本（インタプリタ）+ 3 本（compiled）新設。

### S6/S7 で出た赤 52 件の内訳

| ターゲット | 件数 | 中身 |
|---|---|---|
| `--lib` | 46+43+1 | 手書きの核 IR が `(int …)` のまま（3 クレートに分散） |
| `compile_test` | 3 | 手書きの島 IR `(0 int 0 5)` |
| `numeric_widths_test` | 1 | **期待値のほうが古い**（`0.3333333432674408`）——今回消した欠陥そのものを固定していた |
| `printer_test` | 2 | **削除した挙動を固定していた**（幅の曖昧さエラー・別の幅のメソッドに届く） |

書き換えた `printer_test` が**本物のバグを 1 件釣った**: `format_call` が受け手を
宣言型で渡していなかった（`docs/dev/TODO.md` の該当節）。

直列全実行（`scripts/test-serial.sh`）は 118 個の `test result` すべて ok、
`ALL TESTS PASSED (serial)`。`cargo check --all-targets` は警告 0 件。

最後に `NarrowInt::wsig` を削除した。`pub` なだけで呼び出しが 1 つも無く、
`pub` が dead code 警告を消していた（[[typelisp-pub-hides-dead-code]] の形）。
必要になるのは「箱から `wsig` を作る」向きだが、島は宣言型から定数として
`wsig` を出すので、その向きは存在しない。

## 段階

- **S1** `BoxedObj::Float` を `Float32`/`Float64` に分ける。`is_f32`/`is_f64`、
  rt shim、印字（binary32 の最短往復桁）。
- **S2** `BoxedObj::Narrow` を新設。確保・アクセサ・GC・等価・印字・rt shim。
- **S3** `Sexpr` の変種を D1 のとおりに。`SEXPR_*`、`match_sexpr_core`、
  `construct_sexpr`、`sexpr_ctor_for`、網羅性検査。
- **S4** アクセサの分割と改名。島 44 箇所。組み込みの改名なのでコミット済み
  `.bc` が旧名を呼ぶ——`typelisp-island-regen-fixpoint` の手順（一時シム →
  不動点まで regen → シム除去 → もう 1 回 regen → md5 一致確認）。
- **S5** `format` の `~/name/` を単一候補に。
- **S6** prelude / `equalp` / 印字 / docs / エディタ定義 / テスト。
- **S7** 直列全実行。

## 予想される落とし穴

- **成果物の無効化**: `SOURCE` を触ると島のダンプの `source_digest`（生の文字列
  ハッシュ）が変わる。コメント 1 文字でも再生成が要る。`SOURCE` を触る作業を
  全部終えてから再生成する。
- **一括置換**: `remove-i64` では Rust の文字列リテラル内を機械置換して、
  LLVM IR の表明（`define i64 @f`）と関数名（`sexpr-list-length-i64`）を壊した。
  文字列の中身が 1 種類の言語だと思い込まない。
- **GC ルート**: 新しい箱を作る場所は確保点なので、`Vec<Value>` に貯める経路が
  あればルート漏れになる。`gc_stress` で確かめる。
- **境界**: 「箱の種別 = 静的型」は新しい不変条件。`encode_crossing_args` は
  同じ形のバグを 3 回出している。境界は宣言型駆動にする。
- **`Repr` も幅を落としている**。`Repr::Float` は「`f64`」と書いてあり、
  `Repr::Int` は「どの幅も 1 語」と書いてある。コンパイル済みの値が戻るとき
  箱を作るのは `Repr` なので、ここが幅を持たないと戻り値の箱の幅が決まらない。
  `Repr::F32` / `Repr::F64` に分ける。
- **FASL/ダンプ**: `OwnedForm::Float(f64)` も 1 変種だった。往復で `f32` が
  広がると、戻ってきた値の箱が型と食い違う。`F32`/`F64` に分け、
  `FORMAT_VERSION` を 6 -> 7 に上げた。
