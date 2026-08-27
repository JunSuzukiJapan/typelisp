# typelisp 関数・メソッド一覧

呼び出し形式は3種類ある。

- 自由関数: `(name args...)`
- インスタンスメソッド: `(name receiver args...)`（第一引数の静的型から解決）
- static / 関連関数: `(Type::name args...)`

構文（特殊形・定義方法）は [syntax.md](syntax.md) を参照。

## 1. 算術・比較（`i32` / `i64`）

第一引数の型で `i32` 用と `i64` 用のどちらに解決されるかが決まる（両者は独立で、暗黙変換はない）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | 四則演算。`/` はゼロ方向切り捨て・ゼロ除算で panic |
| `mod` | `(mod a b)` | `(T,T)→T` | 剰余（CL の `mod`、**床除算**＝符号は除数側。`(mod -7 3)`→`2`）。ゼロ除算で panic |
| `rem` | `(rem a b)` | `(T,T)→T` | 剰余（CL の `rem`、**切り捨て除算**＝符号は被除数側。`(rem -7 3)`→`-1`）。ゼロ除算で panic |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | CL の2引数 `floor`/`ceiling`/`round`/`truncate`（`(floor 7 2)`→商3・剰余1）に相当。多値の代わりに商・剰余を`cons-cell`（`car`=商、`cdr`=剰余）で返す（§5。多値そのものは非採用——[dev/cl-missing-classes-and-methods.md](dev/cl-missing-classes-and-methods.md) §3 の 4）。`round-div` は同点をCL準拠で偶数側に丸める |
| `abs` | `(abs x)` | `T→T` | 絶対値（`prelude.rs` のメソッド） |
| `signum` | `(signum x)` | `T→T` | 符号（`1`/`-1`/`0`） |
| `gcd` | `(gcd a b)` | `(T,T)→T` | 最大公約数 |
| `lcm` | `(lcm a b)` | `(T,T)→T` | 最小公倍数（どちらかが0なら0） |
| `max` `min` | `(op a b)` | `(T,T)→T` | 大きい方／小さい方（3引数以上は §4 の可変長糖衣で展開） |
| `1+` `1-` | `(op x)` | `T→T` | `x±1`（`prelude.rs` のメソッド） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | いずれも `=` と同じ（同型の数値に差はない） |
| `int->float` | `(int->float x)` | `T→f64` | `f64` への拡大変換 |
| `int->bignum` | `(int->bignum x)` | `T→bignum` | `bignum` への拡大変換（常に正確） |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | `ratio` への拡大変換（常に正確） |
| `int->char` | `(int->char x)` | `T→char` | Unicode スカラ値として解釈。不正な値は panic |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | `int->char` の失敗を `None` で返す版 |

これらの変換は `(as Type x)`/`(try-as Type x)` 特殊形（[syntax.md](syntax.md) 参照）の実体でもある。
ビット演算（`logand`/`ash`/`ldb` 等）と述語（`zerop`/`evenp` 等）は型をまたいで同じ形なので §4 に
まとめてある。

`i8` `i16` `isize` `u8` `u16` `u32` `u64` `usize` はこの節の表をそのまま持ち、`f32` は §2 の
`f64` の表をそのまま持つ。**幅は静的な区別だけで、実行時表現は共通**——整数はどの型でも
`i64`、浮動小数点はどちらも `f64` なので、`u8` の算術は 8 ビットで巻き戻らず、`f32` の算術は
f32 精度に丸めない。これは `i32` が最初からそうだった扱い（`i32` の加算も 32 ビットで
巻き戻らない）をそのまま広げたもの。

CL の派生カタログ（`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` と §4 の述語）は
`i32`/`i64`/`f64`/`bignum`/`ratio` のまま。CL 側に対応物が無い幅なので、同じ名前を 8 型ぶん
並べても CL 準拠には近づかない——狭い幅が要るのはデータを*名指す*ためで、計算する型としてでは
ない。必要なら `(as i32 x)` で移る（§1b の変換は全ペアにある）。

## 2. 算術・比較（`f64`）

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754。ゼロ除算は panic せず `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | 床除算の剰余（CL 準拠、符号は除数側。`a - b*floor(a/b)`） |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | 切り捨て除算の剰余（CL 準拠、符号は被除数側。`a - b*truncate(a/b)`） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | いずれも `=` と同じ |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | 冪乗 |
| `abs` | `(abs x)` | `f64→f64` | 絶対値 |
| `signum` | `(signum x)` | `f64→f64` | 符号（`1.0`/`-1.0`、`±0.0`/`NaN` はそのまま。CL 準拠で Rust の `signum` とは異なる） |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | 大きい方／小さい方（3引数以上は §4 の可変長糖衣で展開） |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | 単項演算 |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | 超越関数。`log` は自然対数 |
| `log`（2引数） | `(log x base)` | `(f64,f64)→f64` | 底を指定した対数。チェッカーが `(/ (log x) (log base))` へ展開する糖衣（§4） |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | CL の2引数版（`(floor 7.0 2.0)`→商2・剰余1）に相当。§1 の同名メソッドと同じ設計（`car`=商、`cdr`=剰余） |
| `float->int` | `(float->int x)` | `f64→i32` | ゼロ方向への切り捨てで `i32` へ変換 |
| `float->bignum` | `(float->bignum x)` | `f64→bignum` | ゼロ方向への切り捨てで `bignum` へ変換 |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | 正確な二進有理数として `ratio` へ変換（CL の `rational`） |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | CL の同名関数。上の `floor`/`ceiling`/`round`/`truncate` の別名——CL では無印の方が整数を返すので、`f` 付きの方がこの言語の挙動に一致する |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→i32` | それぞれ 2 / 53 / 53（`0.0` の precision だけ 0）。`f64` は常に IEEE-754 binary64 なので定数 |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` か `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,i32)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,i32>` | 仮数（`[1/2,1)`、符号なし）と指数。CL は3値返しだが多値は非採用なので、符号は `float-sign` が担う |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<bignum,i32>` | 同じ分解を厳密な 53 ビット整数の仮数で。`仮数 * 2^指数` がちょうど元の値 |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **その float に読み戻る最も簡単な**有理数（`(rationalize 0.1)` は `1/10`）。厳密な二進値が要るなら `float->ratio` |

> **CL との差: `round` の丸め方**。`round`（したがって `fround`/`round-div`）は
> **0 から遠い方へ**丸める（`(round 2.5)` = `3.0`）。CL は**偶数側へ**丸めるので `2` になる。
> Rust の `f64::round` をそのまま使っている既存の挙動で、Phase 1c は `fround` を
> `round` と一致させることを優先して**この差をそのまま引き継いだ**（別々に丸める 2 つの名前が
> 並ぶ方が悪い）。直すなら `round` 本体を CL 準拠にするのが筋で、`round` は島が lowering
> している組み込みなので島側も同時に変わる。

## 2.5 多倍長数値（`bignum` / `ratio`）

CL 準拠の任意精度数値型。`bignum` は多倍長整数、`ratio` は常に既約・正の分母で保たれる有理数。
どちらもヒープ確保され、`i32`/`i64`/`f64` との暗黙変換はない（明示的な変換メソッドまたは
`as`/`try-as` を使う）。整数/比リテラル構文は [syntax.md](syntax.md) を参照。

**`bignum`**（CL の整数と同じ演算集合。`/` はゼロ方向切り捨て、除算・剰余系はゼロ除算で panic）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(bignum,bignum)→bignum` | 四則（`/` は切り捨て） |
| `mod` | `(mod a b)` | `(bignum,bignum)→bignum` | 床除算の剰余（符号は除数側。§1 の `mod` と同じ） |
| `rem` | `(rem a b)` | `(bignum,bignum)→bignum` | 切り捨て除算の剰余（符号は被除数側。§1 の `rem` と同じ） |
| `abs` | `(abs x)` | `bignum→bignum` | 絶対値 |
| `signum` | `(signum x)` | `bignum→bignum` | 符号（`1`/`-1`/`0`） |
| `gcd` | `(gcd a b)` | `(bignum,bignum)→bignum` | 最大公約数 |
| `lcm` | `(lcm a b)` | `(bignum,bignum)→bignum` | 最小公倍数（どちらかが0なら0） |
| `expt` | `(expt a b)` | `(bignum,bignum)→bignum` | 冪乗。指数が負なら panic（結果が `ratio` になり `bignum` で表せないため） |
| `max` `min` | `(op a b)` | `(bignum,bignum)→bignum` | 大きい方／小さい方 |
| `1+` `1-` | `(op x)` | `bignum→bignum` | `x±1` |
| `logand` `logior` `logxor` `ash` | `(op a b)` | `(bignum,bignum)→bignum` | ビット演算（無限精度2の補数、§4） |
| `lognot` `logcount` `integer-length` | `(op x)` | `bignum→bignum` | 同上（単項） |
| `logbitp` `logtest` | `(op a b)` | `(bignum,bignum)→bool` | 同上（述語） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(bignum,bignum)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bignum,bignum)→bool` | いずれも `=` と同じ |
| `bignum->int` | `(bignum->int x)` | `bignum→i32` | 縮小変換。`i64` に収まらなければ panic |
| `try-bignum->int` | `(try-bignum->int x)` | `bignum→Option<i32>` | 収まらなければ `None` |
| `bignum->float` | `(bignum->float x)` | `bignum→f64` | `f64` へ変換 |
| `bignum->ratio` | `(bignum->ratio x)` | `bignum→ratio` | `ratio` への拡大変換（正確） |
| `print` `println` | `(op x)` | `bignum→Unit` | 標準出力へ書く（§15） |

**`ratio`**（`/` はゼロ除算で panic）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | 四則（結果は常に既約） |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | 床除算の剰余（CL 準拠、符号は除数側） |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | 切り捨て除算の剰余（CL 準拠、符号は被除数側） |
| `abs` | `(abs x)` | `ratio→ratio` | 絶対値 |
| `signum` | `(signum x)` | `ratio→ratio` | 符号（`1`/`-1`/`0` を `ratio` で返す） |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | 冪乗。指数は整数値の `ratio` のみ（非整数なら panic）。負指数は逆数 |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | 大きい方／小さい方 |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`。`ratio` にビット演算は無い（CL も整数専用） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | いずれも `=` と同じ |
| `numerator` | `(numerator x)` | `ratio→bignum` | 既約分子（CL と同名） |
| `denominator` | `(denominator x)` | `ratio→bignum` | 既約分母（常に正） |
| `ratio->bignum` | `(ratio->bignum x)` | `ratio→bignum` | 整数部（ゼロ方向切り捨て） |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | `f64` へ変換 |
| `print` `println` | `(op x)` | `ratio→Unit` | 標準出力へ書く（§15） |

`i32`/`i64`/`f64` からの入口は `int->bignum`/`int->ratio`（§1）と `float->bignum`/`float->ratio`
（§2）。`bignum`/`ratio` は `i32` 等とは独立した別型で、混在した算術には明示変換が必要。

> **実装と compile 対応**: `abs`/`signum`/`rem`/`gcd`/`lcm`/`expt`（および `f64`/`ratio` の
> `mod`）は Rust ビルトインではなく `prelude.rs` の**typelisp メソッド**として各型のプリミティブ演算
> （`/`・`mod`・`floor`/`truncate`・`ratio->bignum` 等）から組み立てられている。したがって JIT/AOT
> でも通常の関数として**コンパイル可能**で、インタプリタと結果が一致する（専用の `rt_*` シムや
> compiler.rs 分岐は不要）。整数 `mod` と各型の四則・比較・変換はネイティブ命令へ直接ローワリングされる。

## 2.6 複素数（`complex`）

**組み込み型ではなく prelude の `defstruct`。** `bignum`/`ratio` が Rust 実装なのは
`BigInt`/`BigRational` の演算がこの言語で書けないからで、`f64` 2 つの複素数はそれに当たらない
（新しい `Repr` も `rt_*` シムも島の lowering も要らず、書いた日から JIT/AOT で動く）。

**CL との違い 2 つ**（どちらも静的型付けの帰結）:

1. **成分は `f64` 固定。** CL の complex は有理数も持て、`(complex 1 2)` と
   `(complex 1.0 2.0)` は別の型。静的な型はどちらかを選ぶ必要があり、超越関数が返すのは
   浮動小数点のほう。
2. **`(sqrt -1.0)` は今までどおり実数の `sqrt`（NaN）。** CL は `sqrt` が和型を返せるので
   実数から複素数を返せるが、`f64` の `sqrt` は `f64` を返さねばならない。複素数の結果は
   複素数の引数から出る——`(sqrt (complex -1.0 0.0))` が `i`。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | 生成。`z::re`/`z::im` で成分を直接読める |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | 実部・虚部。**実数にも効く**（`(realpart 3.0)`→`3.0`、`(imagpart 3.0)`→`0.0`）。CL と同じ |
| `conjugate` | `(conjugate z)` | `complex→complex` | 共役（実数にも効く） |
| `phase` | `(phase z)` | `complex→f64` | 偏角 (-pi,pi]（実数にも効く） |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | 絶対値。**唯一、受け手の型を返さない `abs`**（CL 同様、複素数の絶対値は実数） |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | 複素数の四則 |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | 成分ごとの一致。`Eq` も実装済み（`Ord` は無い——複素数に順序は無く、CL の `<` も拒む） |
| `zerop` | `(zerop z)` | `complex→bool` | 両成分が 0 か |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` は主値 |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`。`(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | ベクトル `(x,y)` の角度。**CL の 2 引数 `(atan y x)` はこれへの糖衣**（`log` の 2 引数版と同じくアリティで分岐） |

`print-object` を実装しているので `~a`/`~s` は CL と同じ `#C(re im)` で印字する
（読み戻す `#C` 構文はこの言語のリーダに無い）。

## 3. 論理・真偽値

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | 否定 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | すべて値の等価比較 |

`and`/`or` は短絡評価が必要なため特殊形（[syntax.md](syntax.md) 参照）。

## 4. 数値ヘルパー

`abs`/`signum`（全数値型）・`gcd`/`lcm`（整数型のみ）・`rem`（`f64` 含む全実数型）・`expt`
（`bignum`/`f64`/`ratio`）は、各数値型のメソッドとして `prelude.rs` に typelisp で定義されている
（レシーバ型で解決。`(abs x)` は `x` の型に応じたメソッド）。詳細と型は各型の節（§1・§2・§2.5）を
参照。整数の `expt` は無い（`bignum` 昇格が無くオーバーフローするため、`(int->bignum x)` 経由で
`bignum` の `expt` を使う）。

（`ratio-expt-int` も `prelude.rs` に `defun` として存在するが、これは `ratio` の `expt` が整数乗を
計算するための内部ヘルパーであり、通常は `(expt r n)` を使う。）

以下は 2026-07-31 に CL 準拠で追加した残りの数値カタログ。可変長・0/1引数の呼び出し形（§4.1）、
述語（§4.2）、定数（§4.3）、ビット演算とバイト指定子（§4.4）、乱数（§4.5）、時間（§4.6）。
すべて JIT/AOT コンパイルできる
（コンパイルできないものの一覧は [syntax.md](syntax.md) §10）。

### 4.1 可変長・0/1引数（チェッカーの糖衣）

CL の算術・比較は可変長だが、`defmethod` はレシーバ型でしか解決せずアリティでは解決しない。
そこで**チェッカーが構文糖衣として展開**する（`Checker::check_variadic_arith` /
`check_variadic_cmp` / `check_nullary_or_unary_numeric_op` / `check_log_with_base`）。展開後は
常に2引数のメソッド呼び出しなので、インタプリタも JIT/AOT も無改修で動く。

| 書ける形 | 展開 | 対象 |
|---|---|---|
| `(op a b c ...)` | `(op (op a b) c)` の左畳み込み | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | 各項を一時変数に束縛した `(and (cmp a b) (cmp b c) ...)` | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | 上記のうち単位元を持つもの |
| `(op x)` | `+ * max min logand logior logxor` は `x` そのもの。`(- x)` は符号反転、`(/ x)` は逆数、`(gcd x)`/`(lcm x)` は `(abs x)`（CL 準拠） | 同上 |
| `(cmp x)` | `x` を評価して `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

各項は左から1回だけ評価される（比較の可変長版が一時変数を挟むのはこのため）。

`isqrt` / 整数の `expt`（`i32`/`i64`）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | 平方根を超えない最大の整数。負なら panic |
| `expt` | `(expt n e)` | `(T,T)→T` | 冪乗（二乗法）。CL は負の指数に有理数を返すが、整数型では表せないので panic——`ratio` に変換してから使う |

### 4.2 述語

| 名前 | 形式 | 型 | 対応する型 |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `i32` `i64` `f64` `bignum` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `i32` `i64` `bignum`（CL 同様、整数型のみ） |

CL の `numberp`/`integerp`/`floatp` 等の**型述語は無い**——静的型付けなので実行時に型を問う場面が
無い（[language-design.md](dev/language-design.md) §0）。

### 4.3 定数

| 名前 | 型 | 値 |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `i32` | `boole` に渡す演算コード（CL のキーワードの代わり） |

数値限界定数（CLHS 12.1.4.2 / 12.1.3）:

| 名前 | 型 | 説明 |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `i64` | ここでの fixnum は実行時が運ぶ即値整数＝`i64`（静的型が `i32` でも実行時表現は `i64`） |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | 有限で最大／最小 |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | 非正規化数を含む、0 でない最小の絶対値 |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | 正規化数に限った同じもの |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | CL の定義（`(/= (+ 1 e) 1)` を満たす最小の正の `e`）に従うので、2^-53 **より 1 ULP 大きい**——2^-53 自身は最近接偶数丸めで `1.0` に戻ってしまう |

### 4.4 ビット演算

無限精度の2の補数として定義される（CL §12.10）。`i32`/`i64`/`bignum` に実装があり、`ratio` には
無い（CL 自体もビット演算は整数専用）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | 論理積・論理和・排他的論理和（可変長・0引数版は §4.1） |
| `lognot` | `(lognot x)` | `T→T` | ビット反転 |
| `ash` | `(ash x count)` | `(T,T)→T` | 算術シフト。`count` が正なら左 |
| `logbitp` | `(logbitp index x)` | `(T,T)→bool` | `index` ビット目が立っているか |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | 立っているビット数（負数なら 0 ビットの数） |
| `integer-length` | `(integer-length x)` | `T→T` | 符号を除いて表現に要するビット数 |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | 上記から合成した残り7種（`prelude.rs`） |

**バイト指定子**（`i32` のみ）。CL の `byte` が返す不透明なオブジェクトの代わりに、既存の
`cons-cell<i32,i32>`（`car`=サイズ、`cdr`=位置）を流用する。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `byte` | `(byte size position)` | `(i32,i32)→cons-cell<i32,i32>` | バイト指定子を作る |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<i32,i32>→i32` | 成分を取り出す |
| `ldb` | `(ldb b x)` | `(cons-cell<i32,i32>,i32)→i32` | `x` から指定バイトを取り出して右詰め |
| `ldb-test` | `(ldb-test b x)` | `(cons-cell<i32,i32>,i32)→bool` | 指定バイトに立っているビットがあるか |
| `mask-field` | `(mask-field b x)` | `(cons-cell<i32,i32>,i32)→i32` | 指定バイト以外を 0 にする（位置は保つ） |
| `dpb` | `(dpb newbyte b x)` | `(i32,cons-cell<i32,i32>,i32)→i32` | 右詰めの `newbyte` を `x` の指定バイトへ埋める |
| `deposit-field` | `(deposit-field newbyte b x)` | `(i32,cons-cell<i32,i32>,i32)→i32` | `dpb` の「位置を保ったまま」版 |
| `boole` | `(boole op a b)` | `(i32,i32,i32)→i32` | `op`（§4.3 の `boole-*` 定数）で選んだ 16 種の2項論理演算 |

### 4.5 乱数

`random` と `random-state` 一式はレシーバを持たない自由関数（`prelude.rs`）。状態は
xorshift64 で、インタプリタと compiled コードは同じ列を返す。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `random` | `(random n [state])` | `i32 &optional random-state → i32` | `0` 以上 `n` 未満の乱数。状態を省略すると `*random-state*` から引いて進める |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | 引数なしなら新しい状態、渡せばその複製（複製は同じ列を再生する） |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | 常に `true`（静的型が既に他の型を排除しているため。CL との対応のためだけに在る） |
| `*random-state*` | — | `random-state` | `random` の既定の状態。動的束縛が無いので**代入可能なグローバル**（`setf` で差し替える） |

新しい状態のシードは壁時計から採る。**シード値を外から与える手段は無い**ので、実行を跨いで
同じ列を再現することはできない（同一プロセス内なら `make-random-state` の複製で再生できる）。

上の4つは `make-random-state-fresh` / `random-state-copy` / `random-state-next` という Rust
プリミティブ（ビットをいじる部分だけ）の上に載った prelude の `defun`。プリミティブ側も呼べるが、
`&optional` を持てるのは `defun` の側なので、通常は上の名前を使う。

### 4.6 時間

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `get-universal-time` | `(get-universal-time)` | `()→i64` | CL の紀元（1900-01-01 UTC）からの秒 |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→i64` | プロセス基準の経過時間。単位は次の定数 |
| `internal-time-units-per-second` | — | `i64` | `1000000`（マイクロ秒）。CL 同様、値は処理系の選択 |
| `time` | `(time form)` | マクロ | `form` を実行し、かかった実時間を1行印字して `form` の値をそのまま返す |

#### 日時への分解・合成

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` の7フィールド。CL の9個の返り値の代わり（多値が無いため） |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(i64,i32)→decoded-time` | 万国時を暦の成分へ。`zone` はグリニッジ以西の時間数（CL と同じ向き）、既定 `0`＝UTC |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(i32×6,i32)→i64` | 逆向き |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | いまを分解したもの |

`day-of-week` は CL と同じく **0 が月曜、6 が日曜**。万国時 0（1900-01-01）が月曜なので、
単なる剰余で出る。暦の計算は Howard Hinnant の `civil_from_days` / `days_from_civil` を
CL の紀元へずらしたもので、表も閏年の場合分けも持たない厳密な整数演算。

**CL との違い**: CL の `decode-universal-time` は zone 引数を省くと**地方時**へ分解するが、
ここでは **UTC** へ分解する。この処理系のランタイムはタイムゾーンのデータベースを持たないので、
CL の9個の返り値のうち `daylight-p` と「既定の分解が使った zone」の2つは、
偽の値を返すのではなく**用意していない**。明示的な zone を渡す形（CL にもある）が代わり。

CPU 時間（`get-internal-run-time`）は無い。`libc` の `getrusage` が要るが、
このワークスペースは `libc` に依存していない——実時間で代用すると嘘になるので置いていない。

### 4.7 実行環境

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | コマンドライン。**要素0はプログラム名** |
| `getenv` | `(getenv name)` | `string→Option<string>` | 環境変数。未設定でも非UTF-8でも `none` |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`。`user-homedir-pathname`（§19.2）の土台 |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | Cargo のパッケージ版数 |
| `machine-type` | `(machine-type)` | `()→string` | CPU アーキテクチャ（`x86_64` / `aarch64` …） |
| `software-type` | `(software-type)` | `()→string` | OS（`macos` / `linux` …） |

`command-line-args` の要素0は、`typl script.typl a b` ならスクリプトのパス、AOT 実行ファイル
`./prog a b` なら実行ファイル自身。**どちらの走らせ方でも同じ添字で同じ引数が読める**ようにこう
決めてある（`typl` は自分の名前と `--heap-cells` 等の大域フラグを取り除いてから渡す）。

`machine-instance`（ホスト名）・`software-version`・`short-site-name` / `long-site-name` は
無い。ホスト名の取得には `libc` が要り、残りは CL でも `NIL` を返してよいことになっている——
中身の無い定数を並べるより、無い方を選んだ。

### 4.8 ユーザへの問いかけ

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | `y` / `n` を1文字で受ける。受け付けるまで訊き直す |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | `yes` / `no` を綴らせる。間違えると高くつく問い用 |

どちらも `*standard-input*` から読む。入力の終端だけが問い直しを止め、そのときは `false`。

## 5. `cons`/`car`/`cdr`（ジェネリックなペア）と `Sexpr`

Symbol/Sexpr 再設計 Phase 4b 以降、`cons`/`car`/`cdr` は `Sexpr` 専用ではなく**ジェネリックな
ペア型 `cons-cell<A,B>`**（`defstruct`、`prelude.rs`）のコンストラクタ/フィールドアクセサに
付け替えられている。フィールドは `変数::car`/`変数::cdr`（[syntax.md](syntax.md) の `defstruct`
アクセサ構文）でも `(car 変数)`/`(cdr 変数)`（インスタンスメソッド呼び出し）でも読める。
**`set-car`/`set-cdr`（破壊的更新）は完全に撤去済み**——`cons-cell` のフィールドを書き換えるには
`(setf 変数::car v)`/`(setf 変数::cdr v)` を使う。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | ペアを作る（自由関数、`prelude.rs`） |
| `car` | `(car p)` | `cons-cell<A,B>→A` | 先頭（`defstruct` フィールドアクセサ、インスタンスメソッドとして呼べる） |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | 残り（同上） |

`read` が返すデータ型 `Sexpr`（`Int | Float | Char | Bool | Sym | Str | Cons | Bignum | Ratio | Path`）
自体のセル操作は、上記の汎用 `cons`/`car`/`cdr` とは別の内部 island 層 `sexpr-*` が担う
（`read`/`eval`/`print`/`defmacro`/自己ホストコンパイラ `compiler.rs` の内部でのみ使われ、
ユーザー向けライブラリ関数からは `sexpr-*` を直接呼ぶ場面はほぼ無い）。

**S 式データの型は `Option<Sexpr>` である。** 空リストは `Sexpr` の変種ではなく
`Option` の `none` であり、`Sexpr` そのものは「空でない S 式」を意味する。
したがって `sexpr-*` は引数も戻り値も `Option<Sexpr>` を取る。

- `()` は `Option<Sexpr>` が期待される位置で空リストになる（`(Option::none)` とも書ける）
- `Sexpr` は `Option<Sexpr>` が期待される位置へ暗黙に広がる（実行時の変換は無い）。
  逆向き——`Option<Sexpr>` を `Sexpr` として使う——は「空リストではない」の主張なので、
  `match` か `unwrap` で明示的に示す必要がある
- `match` では `Sexpr` の 10 変種と `none` を**同じ腕の並びに平らに**書ける
  （[syntax.md](syntax.md) の `match` 参照）

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | `Sexpr` セルを作る |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | 先頭。**空リストなら空リスト**（CL 準拠）。`Cons` でない原子は panic |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | 残り。**空リストなら空リスト**（CL 準拠）。`Cons` でない原子は panic |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | `Cons` かどうか |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | 空リストかどうか |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | `Cons` でないか |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | `Sym`（シンボル）かどうか |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→i64` | `Int` の中身を取り出す。`Int` でなければ panic |
| `sexpr-float` | `(sexpr-float s)` | `Option<Sexpr>→f64` | `Float` の中身。型違いは panic |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | `Char` の中身。型違いは panic |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | `Bool` の中身。型違いは panic |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | `Str` の中身。型違いは panic |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | `Sym` の名前。型違いは panic |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 同一性比較（`Cons`/`Str` はポインタ、それ以外は値） |

上の `sexpr-*` アクセサは Rust 組み込み。これらの上に、`Sexpr` リスト全体を扱う次の2つが
`prelude.rs` に typelisp の `defun` として定義されている（`defmacro` の本体で引数の `Sexpr`
フォーム列を組み立て・変換するマクロ作者向け。`,@`（unquote-splicing）は内部で `sexpr-append`
へ展開される）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `sexpr-append` | `(sexpr-append a b)` | `(Sexpr,Sexpr)→Sexpr` | 2つの `Sexpr` リストを連結（非破壊） |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Sexpr) Sexpr),Sexpr)→Sexpr` | `Sexpr` リストの各要素へ `f` を適用した新しい `Sexpr` リスト（`map`（§6）は `Iter` 用でマクロ引数リストは回せないため、その代替） |

## 6. シーケンス操作（`Iter` 上のライブラリ関数）

Phase 6.5 の再設計で、旧来の `Sexpr` リスト用ライブラリは **`Iter` トレイト上のジェネリック
関数**へ作り直された。呼び出しはコレクションから `(iter coll)` でカーソルを得て渡す
（`Vector<T>` / `HashTable<K,V>` が `Iter` を実装。`Sexpr` のリストは `Iter` を実装しない
ので、これらの関数の対象にはならない）。旧 API との主な違いは、**コレクション/イテレータを
第一引数に取る**点と、**結果のコレクションは新しい `Vector` として返る**点。表中の `Iter<A>` は
「`Item` が `A` の任意の `Iter` 実装型」を表す。

`symbol->string` / `string->symbol` は独立したプリミティブ型 `Symbol`（`Sexpr` の `Sym` 構成子
とは別物——`Sexpr` が要求される文脈へは暗黙変換されるが逆方向の自動変換はない）と `string` の
橋渡し:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `Symbol→string` | シンボル名を取り出す |
| `string->symbol` | `(string->symbol s)` | `string→Symbol` | 文字列からシンボルを作る（intern） |
| `keywordp` | `(keywordp s)` | `Symbol→bool` | キーワード（`:name`）か。コロンは名前の一部なので判定は先頭文字（[syntax.md](syntax.md) §1） |
| `equal` | `(equal a b)` | `(Sexpr,Sexpr)→bool` | 構造的等価（`Cons` は再帰、`Str` は内容比較） |
| `equalp` | `(equalp a b)` | `(Sexpr,Sexpr)→bool` | `equal` に加え大文字小文字無視・数値の型跨ぎ比較 |

述語を取る関数（CL の `-if` 系に対応。すべて `where (Iter I (Item A))`）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | 写像 |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 条件を満たす要素のみ |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 条件を満たす要素を除く |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | 条件を満たす最初の要素 |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<i32>` | 条件を満たす最初の位置 |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→i32` | 条件を満たす個数 |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 全要素が条件を満たすか |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | いずれかが条件を満たすか（CL の `some` 相当、`Some` 構成子との衝突回避名） |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | 左畳み込み |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | 右畳み込み |

添字・長さ・スライス（すべて `where (Iter I (Item A))`）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→i32` | 要素数 |
| `append` | `(append a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | 2つのイテレータを連結 |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | 反転（非破壊） |
| `nth` | `(nth n it)` | `(i32,Iter<A>)→Option<A>` | `n` 番目の要素（範囲外は `None`） |
| `elt` | `(elt it n)` | `(Iter<A>,i32)→Option<A>` | `nth` の引数順違い版 |
| `take` | `(take it n)` | `(Iter<A>,i32)→Vector<A>` | 先頭 `n` 個 |
| `subseq` | `(subseq it start end)` | `(Iter<A>,i32,i32)→Vector<A>` | `[start,end)`（`end` は長さでクランプ） |
| `last` | `(last it)` | `Iter<A>→Option<A>` | 最後の**要素**（CL の「最後のセル」ではない） |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | 最後の要素を除く |

`Eq` / `Ord` 境界を要求する関数（述語の代わりにトレイトで比較。§12.1 参照）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | `x` と等しい要素があるか（CL と違い残りリストではなく `bool`） |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | `x` と等しい最初の要素（CL 本来の `find`。デフォルト `:test` の `eql` に相当） |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<i32>` where `Eq A` | `x` と等しい最初の位置 |
| `count` | `(count x it)` | `(A,Iter<A>)→i32` where `Eq A` | `x` と等しい要素の個数 |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL 本来の `(sort sequence predicate)`。安定な非破壊挿入ソート。`cmp` は「第1引数が第2引数より真に前」で `true` |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | `car` が `k` と等しい最初のペア。値は `(cdr p)` で取り出す |

上の 3 つの表と §6.1 の関数の多くは、CL のキーワード引数 `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count` も取る。どれがどれを取るかは **§6.3**。

`(list e1 e2 ... en)` は特殊形（`(cons e1 (cons e2 (... ())))` へ展開、[syntax.md](syntax.md) 参照）。
`map` / `filter` 等が返す `Vector<T>` を再び回すには `(iter result)` を渡す。

> **旧 API から削除された関数**（`docs/dev/symbol-sexpr-redesign.md` Phase 5 / 6.5）:
> `consp` `null` `atom`（`Sexpr` 述語）、`nthcdr` `copy-list`（cons チェーン専用）、
> `nconc` `nreverse`（破壊的操作）、`remove-if-not`。`find-if`/`count-if`/`position-if`
> は一時 `find`/`count`/`position` に統合されていたが、CL 本来の項目ベース版
> `find`/`count`/`position`（上表）を別途追加したのに伴い述語版の名前として復活した。
> `remove`（要素削除）はその後 Phase 3b で `Iter` 上に復活した（下表）。
> `nconc`/`nreverse`/`remove-if-not`/`copy-list` も Phase 3b/3d で戻っている。

### 6.1 CL カタログの残り（cl-parity-plan.md Phase 3a/3b/3c）

すべて上と同じ `where (Iter I (Item A))` のジェネリック `defun`。`Vector<T>` からは
`(iter v)` で渡す。結果のコレクションはやはり新しい `Vector` として返る。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CL の名前付き添字。`(nth k it)` に委譲 |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | 先頭を除いた残り（共有される tail cons ではなく新しい `Vector`） |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | イテレータを `Vector` に実体化（CL `copy-seq`/`copy-list`） |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` を反転して `b` を続ける |
| `Vector::filled` | `(Vector::filled n x)` | `(i32,T)→Vector<T>` | `x` を `n` 個（CL `make-list`/`make-sequence`）。`Vector::new` と同じく型引数は期待型から来るので、裸の `let` には `the` が要る |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `member` と同じく **`bool`**（イテレータに返すべき tail cons が無い） |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `any`/`every` の否定 |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | 各正版と同型 | 述語を否定した版 |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | 値で削除 |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | 重複除去。CL どおり**最後の出現を残す**（最初の出現を残すには `:from-end true`。§6.3） |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | 値／述語で置換 |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | `Iter<cons-cell<K,V>>` 上 | `assoc` の述語版・値側版 |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | 先頭にペアを足す |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | 2列を組にする。短い方で止まる |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | CL の複数シーケンス `mapcar`。短い方で止まる |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | 副作用のための写像 |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | 写像して連結 |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | 連続する**末尾**への写像 |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | 併合。CL は整列済みを要求するが、これは連結を整列する |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | 無ければ**先頭に**足す |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | 集合演算。CL は順序を規定しないが、ここは**初出順**で安定 |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | 包含 |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | 接尾辞か／接尾辞を除いた前半。CL は**構造の共有**を問うが、共有すべき構造が無いので**値として**の接尾辞を問う |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | 要素ごとの等価。`Vector<T>` 自身に `Eq` の実装は無いので、`tailp` はこれを経由する |
| `caar`…`cddddr` | `(cadr p)` | ネストしたペア上 | CL の 28 個。**リストではなくペア**の走査で、`cadr` は `cons-cell<A,cons-cell<B,C>>` を取る |

`caar`〜`cddddr` が `defmethod` でなく `defun` なのは、`defmethod` の受け手が型変数を束縛するのは
型引数の**最上位**だけで（`check_defmethod` の `written_vars`）、`cons-cell<A,cons-cell<B,C>>` の
`B`/`C` が未束縛になるため。

CL にあってここに無いもの: `list*`（末尾を差し替えた不完全リストという概念が無い）、
`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if`（任意深さの異種の木を走査する型が書けない。
`Sexpr` の木としてなら `equal` が `tree-equal` に当たる）、
プロパティリスト一式 `getf`/`get-properties`/`symbol-plist`/`remprop`（キーと値が交互に並ぶ
無型のリストという表現が無い。同じ役割は `assoc`（連想リスト）か `HashTable` が担う）。

### 6.3 キーワード引数（cl-parity-plan.md Phase 3e）

CL のシーケンス関数が取るキーワード `:key` / `:test` / `:test-not` / `:start` / `:end` /
`:from-end` / `:count` を、上の §6／§6.1 のジェネリック `defun` に持たせた。すべて
**省略可能**で、省略時の意味は今までの挙動と同じ（`remove-duplicates` だけ例外——下記）。

| キーワード | 型 | 意味 |
|---|---|---|
| `:key` | `(fn (A) A)` | 比較・述語にかける前に要素へ適用する射影 |
| `:test` | `(fn (A A) bool)` | `Eq` 境界の `equals` の代わりに使う等価判定。第1引数が**探している項目**、第2引数が（`:key` 適用後の）要素——CL と同じ順 |
| `:test-not` | `(fn (A A) bool)` | `:test` の否定 |
| `:start` `:end` | `i32` | 走査する窓 `[start, end)`。添字は列全体に対するもの |
| `:from-end` | `bool` | 探索は**最後の**一致を答える。`:count` と併せると影響を受けるのは末尾側から |
| `:count` | `i32` | `remove`／`substitute` 系が影響を与える最大個数 |

どの関数がどれを取るかは CL に従う:

| 関数 | 取るキーワード |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | 上の全部（`:count` を含む） |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not`（`assoc` の `:key` は `car`、`rassoc` は `cdr` に掛かる） |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |

```lisp
(find 2 (iter v) :key (lambda ((x i32)) i32 (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; 末尾側の 1 個だけ消す
(position 3 (iter v) :start 1)                          ; 添字は列全体に対するもの
```

**CL と違うところ 3 点**:

1. **`:key` の射影は要素型の中に閉じる**（`(fn (A) A)`）。CL のように別の型へ射影する
   ことはできない——型変数を増やすと省略時に決まらなくなるため。異なる型への射影が要る
   場面は `-if` 系にラムダを渡すほうで書ける（`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`）。
2. **項目ベースの探索では `:key` は要素にだけ掛かる**（探している項目には掛からない）。
   CL の `find`/`position`/`count`/`member`/`remove`/`substitute` と同じ規則。集合演算では
   両辺とも要素なので両方に掛かる。
3. **`remove-duplicates` の既定が変わった**。Phase 3e 以前は無条件に最初の出現を残していたが、
   CL の既定は**最後**を残す。以前の挙動は `:from-end true`。

破壊的な版（§6.2、`Vector<T>` の `defmethod`）と `search`/`mismatch`（`string` の
`defmethod`）はまだキーワードを取らない。`defmethod` は `&optional`/`&key` を受け付けない
（cl-parity-plan.md Phase 5b）。

### 6.2 破壊的操作（cl-parity-plan.md Phase 3d）

`Vector<T>` の `defmethod`。**受け手を書き換えたうえで受け手自身を返す**ので、`(nreverse v)` は
`reverse` と同じ形で書けて `v` 自身も反転する。`!` 接尾辞は使わない規約
（[language-design.md](dev/language-design.md) §7.3）に従い CL の名前をそのまま使う。

| 名前 | 形式 | 説明 |
|---|---|---|
| `nreverse` | `(nreverse v)` | その場で反転 |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | `remove`／`remove-if`／`filter`／`remove-duplicates` のその場版 |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | `substitute` 系のその場版 |
| `nbutlast` | `(nbutlast v)` | 末尾を1つ落とす |
| `fill` | `(fill v x)` | 全要素を `x` に。長さは変えない |
| `replace` | `(replace v src)` | `src` の要素を先頭から上書き。`(min (len v) (len src))` 個 |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`。同上 |
| `nconc` | `(nconc v w)` | `w` の要素を `v` に追加。CL と違い**共有構造の書き換えではない**（`w` は影響を受けない） |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | `v` の中身を `src` で置き換える（長さも変わる）。上の `delete`/`n...` 系の共通土台 |
| `rplaca` `rplacd` | `(rplaca p x)` | `cons-cell` の `set-car`/`set-cdr` に、セル自身を返す形を被せたもの |

`vector-push-extend`/`vector-pop` は既存の `push`/`pop` そのもの——`Vector<T>` は常に伸びるので、
CL の「fill pointer を持つベクタ」と「simple なベクタ」の区別に対応するものが無い。

**`Sexpr` 版の `rplaca`/`nconc` は無い**（意図的）。`Sexpr` の cons セルは自分の `car` の
ソース位置をセル内に持つ（`value::Cell::car_loc`）ので、`car` を書き換えると古い要素の位置が
新しい要素に付いたまま残り、以後の診断が静かにずれる。

## 7. `Option<T>` / `Result<T,E>`

構成子: `Option<T>` は `Some(T)` / `None`。`Result<T,E>` は `Ok(T)` / `Err(E)`。
`E` は任意の型でよい——組み込みの具象エラー型も、`defstruct`/`defenum` で書いた自前の型も
そのまま載る（§7.1）。

| 名前 | 形式 | Option | Result | 説明 |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | 値を取り出す。`None`/`Err` なら panic |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | 値、または既定値 |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | `Some` か |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | `None` か |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | `Ok` か |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | `Err` か |

構成子は `Option::some`/`Option::none`/`Result::ok`/`Result::err`（または `(use option)`/
`(use result)` で裸名 `some`/`none`/`ok`/`err` も使用可能）。

### 7.1 エラー型と `Error` トレイト

Rust の `std::error::Error` に倣い、**`Error` は型ではなくトレイト**。エラーを表す具象型は
用途ごとに分かれていて、いずれも `Error` を実装する。

| 型 | 生成元 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` |
| `EvalError` | `eval` |
| `FileError` | ファイル/ストリーム操作（§18） |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`。CL の `simple-error`——「何が起きたか言いたいだけ」のときの既定の選択肢 |
| `WrappedError` | `(wrap-error msg cause)`。自分のメッセージと原因の両方を運ぶ唯一の型で、`Error` トレイトに `source` がある理由 |

最初の 5 つはいずれも「メッセージ文字列を1つ持つ単一変種の直和型」で、型名と変種名が同じ
（`(match e ((ParseIntError m) m))`、構成は `(ParseIntError::ParseIntError "...")`）。
特別扱いは一切なく、自前のエラー型を `(defstruct my-err (...))` / `(defenum my-err ...)`
で書いたときとまったく同じ扱いになる。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | エラーメッセージ（`Error` トレイトのメソッド） |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | このエラーが包んでいる原因、無ければ `None`（Rust の `Error::source`） |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>`（`E` は `Error` 実装） | 具象エラー型を trait オブジェクトへ広げる |
| `describe-error` | `(describe-error e)` | `E→string`（`E` は `Error` 実装） | メッセージと、`source` を辿った原因の連鎖を 1 行 1 原因で。CL に対応物は無い（Rust の "caused by"） |

自前のエラー型に `Error` を実装すれば、組み込みエラーと**同じ形で**扱える:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; 具象型をそのまま E に載せる
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; 種類を問わず一様に扱う
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

複数のエラー型を1つの `Result` に集める場合は `Result<T, :dyn Error>`（Rust の
`Box<dyn Error>` 相当）を使い、具象エラーは `as-dyn-error` で広げる。typelisp には `?` が
無い（[language-design.md](dev/language-design.md) §7.3）ので、この変換は明示的に書く:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

なお**型とトレイトは1つの名前空間を共有する**（Rust と同じ）。同じモジュール内で
`defstruct`/`defenum` とトレイトに同じ名前は付けられず、型位置にトレイト名を書くと
「`error` is a trait, not a type — write `:dyn error`」と報告される。

## 8. 文字列 (`string`)

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | 大文字化（ASCII のみ） |
| `downcase` | `(downcase s)` | `string→string` | 小文字化（ASCII のみ） |
| `length` | `(length s)` | `string→i32` | 文字数 |
| `ref` | `(ref s i)` | `(string,i32)→char` | `i` 番目の文字。範囲外は panic |
| `substring` | `(substring s start end)` | `(string,i32,i32)→string` | 部分文字列 `[start,end)` |
| `append` | `(append s1 s2)` | `(string,string)→string` | 連結 |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | 辞書順比較（`i32` 等と同じくレシーバ型で多重定義） |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | 辞書順の狭義小なり（`<` の旧 CL カタログ名） |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | 同一性比較（内容ではなく参照）。2026-08-18 まで compiled 側だけ内容比較になっていたのを揃えた |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | 内容比較（大文字小文字を区別） |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | 内容比較（大文字小文字を無視、ASCII のみ） |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | 内容が異なるか（CL `string/=`。可変長形は隣接ペア比較——§4.1） |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | 大文字小文字を無視した順序比較（CL `string-lessp` 等）。共通接頭辞なら短い方が小 |
| `string::filled` | `(string::filled n c)` | `(i32,char)→string` | `c` を `n` 個並べた文字列（CL `make-string`） |
| `search` | `(search s sub)` | `(string,string)→Option<i32>` | `sub` が最初に現れる位置。**CL の `search` は引数順が逆**（`(search pattern sequence)`）。空文字列は 0 |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<i32>` | 最初に食い違う位置。`equal` なときだけ `none`。片方が接頭辞なら短い方の末尾 |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string,string?)→string` | 両端/左/右から `bag` に含まれる文字を除く（CL `string-trim` 等）。`bag` 省略時は空白類 `" \t\n\r"` |
| `capitalize` | `(capitalize s)` | `string→string` | 各語の先頭を大文字・残りを小文字（CL `string-capitalize`）。語＝英数字の極大連続 |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | `sep` で分割。CL に対応物は無い。連続する区切りは空要素を生む。`sep` が空なら panic |
| `to-string` | `(to-string x)` | `T→string` | `~a` 相当の文字列化。`i32`/`i64`/`f64`/`bool`/`char`/`string` に実装（CL `princ-to-string`） |

`trim` 系と `digit-weight`/`digit->char`（§9）だけ `defmethod` でなく `defun` なのは、
`defmethod` が `&optional`/`&key` を受け付けないため（`parse_defmethod_sig_inner`）。

## 9. 文字 (`char`)

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | 大文字化（ASCII のみ） |
| `downcase` | `(downcase c)` | `char→char` | 小文字化（ASCII のみ） |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | コードポイント順比較（レシーバ型で多重定義） |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | コードポイント順の狭義小なり（`<` の旧 CL カタログ名） |
| `alphap` | `(alphap c)` | `char→bool` | ASCII アルファベットか |
| `digitp` | `(digitp c)` | `char→bool` | ASCII 数字か |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | 値の比較 |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | 大文字小文字を無視した値の比較 |
| `char->int` | `(char->int c)` | `char→i32` | Unicode スカラ値（逆方向は §1 の `int->char`/`try-int->char`） |
| `char->string` | `(char->string c)` | `char→string` | 1文字だけの文字列。CL は `string` 関数が指定子を取って兼ねるが、この言語には指定子が無いので向きを名前に出している |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | 値が異なるか（CL `char/=`。**可変長形は隣接ペア比較**で、全ペア相異を問う CL とは異なる——§4.1） |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | 大文字小文字を無視した順序比較（CL `char-lessp` 等）。等値版は既存の `equalp`（CL `char-equal`） |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | 大文字か/小文字か/そもそも大小の別を持つか（CL `upper-case-p` 等） |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | 英字または数字か（CL 同名） |
| `graphicp` | `(graphicp c)` | `char→bool` | 印字可能か。空白は含み、改行・タブは含まない（CL `graphic-char-p`） |
| `standardp` | `(standardp c)` | `char→bool` | CL の標準文字 96 個か＝`graphicp` に改行を足したもの（CL `standard-char-p`） |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char,i32?)→Option<i32>` | その基数での数字の**重み**（CL `digit-char-p` 本来の意味）。既存の `digitp` は `bool` のまま据え置き |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(i32,i32?)→Option<char>` | 重み `w` を表す文字。10 以上は大文字（CL `digit-char`。基数は最大 36） |
| `char->name` | `(char->name c)` | `char→Option<string>` | 文字名。名前を持つのはリーダの表にある 7 つだけ（CL `char-name`） |
| `name->char` | `(name->char s)` | `string→Option<char>` | 文字名から文字。大文字小文字を無視し、リーダの別名（`linefeed`/`null`）も受ける（CL `name-char`） |

この節の実装は全て `char->int` のコードポイント上で書かれていて、`upcase`/`downcase`/`alphap`/
`digitp`/`int->char` を**呼ばない**。この 5 つは島に lowering が無い組み込み
（`externs::native_lowered_primitive_methods`）なので、触れると prelude 全体が
インタプリタ専用に落ちて `PRELUDE_COMPILE_UNSUPPORTED` に穴が開く。ASCII 限定なのも
既存の `upcase`/`alphap` と同じ理由（Unicode の表を実行時に持っていない）。

`char-code-limit` に当たる定数は無い（`char` は Unicode スカラ値で、上限は言語の性質ではなく
Unicode の性質）。`char-int` は `char->int` と同じ。

## 10. `Vector<T>`

可変長配列。ユーザ定義 `defstruct` と同じヒープ表現を流用した組み込み型。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | 空のベクタを作る（static） |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | 末尾に追加 |
| `get` | `(get v i)` | `(Vector<T>,i32)→T` | `i` 番目を読む。範囲外は panic |
| `set` | `(set v i x)` | `(Vector<T>,i32,T)→Unit` | `i` 番目を書き換える。範囲外は panic |
| `len` | `(len v)` | `Vector<T>→i32` | 要素数 |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | 末尾を取り除いて返す。空なら `None`（`get`/`set` と異なり範囲外でも panic しない） |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | `Iter` を実装するカーソルを作る（ライブラリ定義） |

`map`/`filter` は§6（`Iter` トレイト上のジェネリック関数）で実装済み——`(map (iter v) f)` のように
`Vector<T>` を `iter` でカーソル化して渡す。

**`Vector<T>` ↔ `Sexpr` リストの相互変換は言語仕様上できない（意図的に対象外）**: `cons-cell<A,B>`
は異種のペア型で、要素を並べた連鎖の型は要素ごとに変わる（`(cons 1 (cons "s" x))` の
`cdr` の型は `cons-cell<string, ...>`）。`Sexpr` のリストはこの異種の入れ子 `Cons` 連鎖であり、
`Vector<T>` のような単一の要素型 `T` だけからなるコレクションとは表現が根本的に異なるため、両者を
汎用的に変換する `to-list`/`from-list` のような関数は書けない（型パラメータ `T` だけでは `Sexpr` 側の
入れ子構造を静的に表現できない）。

## 11. `HashTable<K,V>`

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | 空のテーブルを作る（static） |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | 検索 |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | 挿入・上書き |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | 削除し、あれば旧値を返す |
| `count` | `(count h)` | `HashTable<K,V>→i32` | 要素数 |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | 全削除 |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | キーのスナップショット |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | 値のスナップショット |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | `(k . v)` ペアのスナップショット |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | `Iter` を実装するカーソル（ライブラリ定義） |

## 11.1 `Array<T>`（多次元配列）

`Vector<T>` 2 本（次元列と row-major の平坦な要素列）の上に書かれた prelude の
`defstruct`。組み込み型ではないので、`defstruct` にできることは全部できる。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<i32>,T)→Array<T>` | CL `make-array`。`dims` は複製される。`init` が全セルの初期値（CL の `:initial-element`。この言語に「未束縛のセル」は無いので必須）。`:fill-pointer` は 1 次元のときだけ |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<i32>)→T` | CL `aref` / `(setf (aref …))`。添字が範囲外なら panic |
| `aref` | `(aref a i j …)` | — | 裸の添字で書く CL の綴り。**チェッカーの糖衣**で上の `get`/`set` に展開される（§4.1 の可変長演算子と同じ手口）。`(setf (aref a i j) v)` も同じ |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,i32)→T` | CL `row-major-aref`。平坦な添字 |
| `rank` | `(rank a)` | `Array<T>→i32` | CL `array-rank` |
| `dimension` | `(dimension a n)` | `(Array<T>,i32)→i32` | CL `array-dimension` |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<i32>` | CL `array-dimensions`。CL が新しいリストを返すのと同じく**複製**を返す |
| `total-size` | `(total-size a)` | `Array<T>→i32` | CL `array-total-size`（fill pointer とは無関係の確保済みセル数） |
| `len` | `(len a)` | `Array<T>→i32` | CL の配列に対する `length`。fill pointer があればその値、無ければ `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<i32>)→bool` | CL `array-in-bounds-p`。添字の**個数**が違っても偽（エラーではない） |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<i32>)→i32` | CL `array-row-major-index` |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<i32>,T)→Unit` | CL `adjust-array`。ランクは変えられない。範囲に残る要素は添字ごと保存、増えたセルは `init`。CL と違い配列を返さない（この言語の配列は全部 adjustable なので、返す第 2 の配列が無い） |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CL `vector-push-extend`。fill pointer が無ければ panic |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CL `vector-pop`。空なら `none`（`Vector<T>` の `pop` と同じ） |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<i32>` | fill pointer（無ければ `none`）。`(setf a::fill-pointer …)` で書ける |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | row-major 順のカーソル。fill pointer があればそこで止まる |

- **添字は `Vector<i32>`**。`defmethod` はアリティで解決するので「末尾に同じ型の引数が
  何個か続く」形を宣言できない。`aref` の糖衣がその差を埋めている。
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` /
  `array-has-fill-pointer-p` は**無い**。受け手の静的型が既に答えている問い。
- `Array::new` は `defstruct` が生成するフィールド順のコンストラクタ（次元列・平坦な格納・
  fill pointer）で、作るときに使うものではない。`Array::make` を使う。

## 11.2 `BitVector`（ビットベクタ）

固定長のビット列。`Vector<i64>` に **1 語 32bit** で詰めた prelude の `defstruct`
（64bit にしない理由は [dev/TODO.md](dev/TODO.md) の整数切り詰めの節）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `i32→BitVector` | 長さ `n`、全ビット 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,i32)→bool` | 範囲外は panic |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,i32)→bool` | CL の綴り。`(setf (bit v i) b)` も書ける。CL の `sbit` は simple なビットベクタを要求する点だけが `bit` と違うが、この言語のビットベクタは 1 種類しかない |
| `len` | `(len v)` | `BitVector→i32` | ビット数 |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | 新しいビットベクタを返す。長さが違えば panic。CL の第 3 引数（結果の書き込み先）は無い |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | 補集合 |

`bit-vector-p` は無い（静的型が答えている）。長さの先にあるビットは常に 0 に保たれるので、
同じ長さの 2 本は必ず同じ表現を持つ。

## 12. `Iter` トレイトと反復

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>` はそれぞれ `vector-iter<T>`/`hashtable-iter<K,V>` を介して `Iter` を
実装している（`(iter コレクション)` でカーソルを取得）。`Sexpr` のリストには意図的に `Iter` を
実装していない（要素型が一様でないため）。ユーザ定義の `deftrait`/`impl` で独自のコレクションに
`Iter` を実装すれば、そのまま `doiter` で回せる（[syntax.md](syntax.md) 参照）。

`cons-cell<A,B>` は `(car cell)`/`(cdr cell)` を持つ汎用の2要素組（タプル構文の代わり）。

## 12.1 `Eq` / `Ord` トレイト（比較）

Rust の `PartialEq`/`PartialOrd` に相当（名前は `Eq`/`Ord`）。ジェネリック関数の `where` 境界で
要素型の比較を要求するのに使う（`sort`/`member`/`assoc` 等）。

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; 実装必須
  (not-equals ((self Self) (other Self)) bool             ; デフォルト実装
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; Eq を継承
  (less ((self Self) (other Self)) bool)                  ; 実装必須
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

`Eq` を実装するには `equals` だけ、`Ord` を実装するには `less` だけ書けばよい。残りはデフォルト
実装が埋める。`Ord` は `Eq` を継承するので、`impl Ord X` の前に `impl Eq X` が必要。

各トレイトメソッドはそのまま関数として呼べる（`where (Eq A)`/`(Ord A)` 境界内、または実装済みの
具体型に対して）:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | 等しいか（Rust の `==`） |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | 等しくないか（`!=`） |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` 実装済み: 全数値型（`i8`〜`usize` / `f32` / `f64` / `bignum` / `ratio`）と `bool` `char`
`string` `symbol`、および `cons-cell<A,B>`（要素が `Eq` なら再帰的に）。`Ord` 実装済み:
全数値型と `char` `string`、および `cons-cell<A,B>`（辞書順、要素が `Ord` なら）。メソッド名が組み込み演算子
（`= /= < <= > >=`）・`eq`/`lt` と重複
しないのは、組み込みは再定義できず各実装がそれらへ委譲するため。スカラの比較演算子そのものは
レシーバ型で多重定義された組み込みメソッド（§1・§2・§8・§9）。

## 12.2 算術トレイト（`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`）

ジェネリックコードが「足せる型」を要求するための層。**具体型の演算は今までどおり組み込み
演算子**（§1・§2）で、この層は通らない。

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self))
(deftrait Number (Add Sub Mul Div Rem Ord))               ; メソッド無し・6つの合成
```

**境界内では演算子で書ける。** `where` で束縛された型変数が受け手のとき、チェッカーが
演算子をトレイトメソッドへ綴り直す（`+`→`add`、`-`→`sub`、`*`→`mul`、`/`→`div`、
`rem`→`remainder`、`logand`→`bit-and`、`=`→`equals`、`<`→`less` …）:

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

トレイト側のメソッド名が `+` でないのは、`+` が組み込みメソッド名で `impl` が再定義を
拒むため（`cannot redefine built-in method`）。`Neg` は無い——`(- x)` は
`(- (- x x) x)` へ脱糖されるので `Sub` だけで足りる。

実装済み: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` は全数値型、`Bits` は全整数型と `bignum`。

## 13. 高階関数

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | そのまま返す |
| `const` | `(const x y)` | `(A,B)→A` | 第一引数を返す |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | 関数合成 `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | 2引数関数の引数順を入れ替える |

## 14. マクロ・システム

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→Symbol` | 新しいシンボル。名前は `" <prefix><n>"` で `n` は `*gensym-counter*`。先頭の空白はソースに書けないので、生成した束縛が書かれた名前と衝突しない |
| `*gensym-counter*` | 変数 | `i32` | `gensym` が次に使う番号。CL 同様、読んでも設定してもよい |
| `macroexpand-1` | `(macroexpand-1 form)` | `Sexpr→Result<Option<Sexpr>,EvalError>` | マクロ呼び出しを 1 段展開。`none` は「マクロ呼び出しではない」 |
| `macroexpand` | `(macroexpand form)` | `Sexpr→Result<Sexpr,EvalError>` | マクロでなくなるまで繰り返す |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | 述語の否定 |
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | 偽なら panic。メッセージ省略時は `assertion failed: <テストを書かれたまま>`（マクロなので式そのものを名指せる）。CL の restart はこの言語に無い |
| `warn` | `(warn control args...)` | `(string,...)→()` | `*error-output*` へ `WARNING: ` 付きで 1 行書いて**続行**する。`Result` を返しもせずプログラムを終わらせもせずに報告する唯一の手段 |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | グローバルを `body` の間だけ差し替え、抜けるときに戻す。CL はこれを `let` と書くが、この言語の `let` は常に字句束縛なので別名（Emacs Lisp の同名マクロと同じ役目）。復元は `unwind-protect` の cleanup なので、正常終了・`throw`・`panic`・`break`/`return` のどれで抜けても走る。**スレッドごとの束縛ではない** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | 印字制御変数を全部標準値に `dlet` する（§15.3） |
| `exit` | `(exit code)` | `i32→!` | プロセスを終了する |
| `dump` | `(dump path)` | `string→bool` | いまの環境（型情報 + コンパイル済み本体）を1ファイルへ書き出す。`typl --image <path>` で立ち上げ直せる。`compile`/`compile-file` と同じくインタプリタ専用（コンパイル済み関数からは呼べない） |

`macroexpand-1` が返すのは `Option`——CL は「展開したか」を第 2 返り値で伝えるが、多値が
無いので `none` がそれに当たる。CL の真偽値より情報が多く、**自分自身の呼び出しへ展開する
マクロと非マクロを取り違えようがない**。展開の 1 段はチェッカーが使うのと同じもの
（`Checker::try_expand_toplevel_macro`）なので、プログラムが見るものと検査が見たものは
ずれない。

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (some (+ 5 5)))
(macroexpand-1 '(+ 1 2))     ; => (ok none)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

CL にあってここに無いもの（cl-parity-plan.md Phase 4c に理由を記録）:
`constantly`（無視する引数の型が戻り型にしか現れず決まらない。`(lambda ((x T)) A v)` を書く）、
`macrolet`/`symbol-macrolet`、`eval-when`（`:compile-toplevel`/`:load-toplevel`/`:execute` が
常に一致するので選ぶ区別が無い）、`define-compiler-macro`、`load-time-value`、
`make-symbol`/`copy-symbol`/`gentemp`（uninterned シンボル。束縛子は名前で引かれるので買えるものが無い）。

`compile`/`compile-file`/`dump` は [syntax.md](syntax.md) の「コンパイル」節を参照。
`dump` が保存するのは**定義であって値ではない**——グローバルは初期化式を走らせ直した値で戻り、
セッションのトップレベル式は再実行されない。SBCL の `save-lisp-and-die` と違ってプロセスも死なない。

## 15. 標準入出力 (I/O)

`print`/`println`/`format` はいずれも**書式指定子（CL の `format` ディレクティブ)を解釈する特殊形**。
第1引数（`format` は第2引数）が**制御文字列**で、以降の可変長引数を各ディレクティブが順に消費する。
`(list ...)` と同じく、可変長引数は各自の型のまま `Sexpr` へ包まれてから渡る——`i32`/`i64`/`f64`/
`bignum`/`ratio`/`char`/`bool`/`string`/`Sexpr` はスカラ用の `Sexpr` コンストラクタでラップされ、
ユーザ定義 `defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>` 等ヒープ表現の ADT インスタンスは
無変換のまま `Sexpr` へ retype される（`(println "~a" my-struct)` はそのまま動く）。ネイティブ表現の
ジェネリック実体化（`Option<llvm-value>` 等）だけは `Sexpr` の表現を持たないため型エラーのまま。
`typl file.typl` によるスクリプト実行は [main.rs](../src/main.rs) の `run_file`
の通り**トップレベル式の戻り値を出力しない**ため、プログラム自身が標準出力へ書くにはこれらを呼ぶ必要がある。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | 制御文字列を書式展開し、改行なしで標準出力へ書く |
| `println` | `(println control args...)` | `(string, ...)→Unit` | 同上、末尾に改行を付ける |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CL の `format` 相当。展開した文字列を返す。`dest` が `true`（CL の `t`）なら加えて標準出力へも書く／`false`（CL の `nil`）なら書かず文字列を返すだけ |
| `format`（ストリーム宛） | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | `dest` が `bool` でなければ CL のストリーム宛。展開した文字列をそのストリームへ書く。戻り値は `()`（CL の `nil` 相当）で、文字列は返らない |

`dest` の型で2つの意味に分かれる（どちらになるかは静的に決まる）。ストリーム宛は
`(write-string dest (format false control args...))` へ展開されるだけなので、具象ストリーム型・
`:dyn CharOutput`・`(where (CharOutput S))` の型変数のいずれでも同じように書ける。
`bool` でもストリームでもない `dest` は「destination は `true`/`false` か `CharOutput` を実装した
ストリーム」という型エラーになる。

**`Option<Sexpr>` は透過的に印字される。** S 式データの型が `Option<Sexpr>` になったため、
`(some x)` の包みは印字に現れず、中身がそのまま出る。空リストは `()` と出る。
これは `Option<Sexpr>` の実行時表現が `Sexpr` そのもの（空リストが `none`）だからで、
他の `Option<T>` は従来どおり `(some ...)` / `(none)` と印字する。

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i64>   (Option::some 42)))   ; => (some 42)
```

**1 引数プリンタ**（CLHS 22.1.3）は書式展開ではなく、値ひとつをそのまま印字する。
すべて prelude のマクロで、ストリームは省略可能（既定 `*standard-output*`）。

| 名前 | 形式 | 説明 |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | 読み戻せる表現（`~s` と同じ）で書き、`x` を返す |
| `princ` | `(princ x [stream])` | 人向けの表現（`~a` と同じ）で書き、`x` を返す |
| `write` | `(write x [stream])` | `*print-escape*` が真なら `prin1`、偽なら `princ`。`x` を返す |
| `prin1-to-string` | `(prin1-to-string x)` | 書かずに文字列で返す（`~s`） |
| `princ-to-string` | `(princ-to-string x)` | 同上（`~a`）。受け手先頭の綴りは `to-string` |
| `write-to-string` | `(write-to-string x)` | 同上、`*print-escape*` に従う |

`print`/`println` は**これらではない**。制御文字列を取る `format` の短縮形であり、
CL の `print`（改行 → `prin1` → 空白）とは別の仕事なので、両方をそれぞれの名前で残してある。
その結果 **CL の 1 引数 `print` にはこの言語での綴りが無い**——`prin1` を書く。

関数でなくマクロなのは、`format` の可変長引数が型変数を受け付けないため（上記のとおり
`Sexpr` 表現を持つ具体型でなければならない）。マクロなら呼び出し地点で型が具体化している。

**標準入力を読む**のは専用関数ではなく、標準ストリーム `*standard-input*` に対する
`CharInput` のメソッド（§18.1）——`(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)`。標準出力・標準エラーも同様に `*standard-output*` /
`*error-output*` があり、`(write-line *standard-output* s)` のように書ける（上の
`print`/`println`/`format` は書式展開が要るときの近道で、常に標準出力へ書く）。

### 書式ディレクティブ

CL の `format` ディレクティブをほぼ網羅する。各ディレクティブは `~`、任意の**プレフィックス
パラメータ**（カンマ区切り。整数 / `'c`（文字）/ `v`（次の引数から取る）/ `#`（残り引数数））、任意の
**修飾子** `:`・`@`、ディレクティブ文字、の順。ディレクティブ文字は大文字小文字を区別しない。
引数が余れば無視（CL と同じ）、足りなければ実行時エラー。

**出力（引数を1つ消費）**

| ディレクティブ | パラメータ / 修飾子 | 意味 |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=右詰め | aesthetic（CL `princ`。文字列クォートなし） |
| `~s` | 同上 | standard（CL `prin1`。reader 構文） |
| `~w` | — | CL の `write`。`*print-pretty*` が真なら整形出力、偽なら `~s` と同義 |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=桁区切り, `@`=符号必須 | 10/2/8/16進整数（非整数は `~a` 相当で表示） |
| `~r` | `~radix,...`（基数指定）または無指定 | 基数指定時はその進数。無指定で `~r`=英語基数、`~:r`=英語序数、`~@r`=ローマ数字、`~:@r`=旧ローマ |
| `~p` | `:`=1つ戻る, `@`=y/ies | 複数形（`~p`→"s"、`~@p`→"y"/"ies"） |
| `~c` | `:`=名前, `@`=`#\`構文 | 文字 |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=符号 | 固定小数点 |
| `~e` `~g` | float | 指数 / 汎用浮動小数点 |
| `~$` | `~d,n,w,padchar` / `:`,`@` | 金額表記 |

**出力（引数を消費しない）**

| ディレクティブ | 意味 |
|---|---|
| `~%` | 改行（`~n%` で n 個） |
| `~&` | fresh-line（行頭でなければ改行。`~n&`） |
| `~\|` | 改ページ（form feed） |
| `~~` | リテラルの `~`（`~n~` で n 個） |
| `~t` | タブ（`~col,incT`。`:`/`@`=相対。pretty 時は `:`=論理ブロック起点の相対タブ） |
| `~_` | 条件改行（pretty。素=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`） |
| `~i` | インデント（pretty。`~ni`=ブロック起点+n / `~n:i`=現在桁+n） |
| `~<改行>` | 改行を無視（`:`=空白保持, `@`=改行保持） |

**制御構造**

| ディレクティブ | 意味 |
|---|---|
| `~(...~)` | 大文字小文字変換（`~(`小文字, `~:(`各語頭大文字, `~@(`先頭のみ大文字, `~:@(`全大文字） |
| `~[...~;...~]` | 条件選択（整数で分岐。`~:;`=デフォルト節, `~:[偽~;真~]`=真偽, `~@[...~]`=非false時のみ） |
| `~{...~}` | 反復（リスト引数を走査。`~:{`=部分リストごと, `~@{`=残り引数, `~^`=脱出, `~:}`=空でも1回） |
| `~<...~;...~>` | 桁揃え（セグメントを `~mincol` 幅に分散。`:`/`@`=端の詰め） |
| `~<...~;...~:>` | **論理ブロック**（閉じが `~:>`。上の桁揃えとは別物）。先頭セグメント=prefix、末尾=suffix（いずれもリテラル文字列のみ）。`~@;` 区切りなら prefix は**行頭 prefix**。`~:<` は prefix/suffix を `(`/`)` に既定。引数はリスト1つ（`~@<` は残り引数をその場で使う） |
| `~?` | 間接（次の引数=制御文字列、その次=引数リスト。`~@?`=以降の引数を流用） |
| `~*` | 引数スキップ（`~n*`=n個進む, `~:*`=戻る, `~@*`=絶対位置へ） |
| `~/name/` | メソッド呼び出し（下記。`:`/`@` フラグがメソッドへ渡る） |

**`~/name/` は CL と 1 点違う: 名前をグローバル関数ではなく引数自身の型のメソッドとして引く。**
メソッドの形は `((self Self) (colon bool) (at bool)) → string` で、ディレクティブの `:`/`@` が
そのまま渡る。CL の読み（グローバル関数）はこの言語では健全に実装できない——制御文字列は
実行時の `string` なので「どのディレクティブがどの引数に当たるか」は検査時に決まらず、
その時点で登録済みの定義が持っているのは表現（`Repr`）だけで、`Repr::Struct` は全 `defstruct` を
1 つに潰す。名前だけで引くと `point` 用のヘルパを `pathname` に対して呼べてしまう。値の型で
ディスパッチすれば、そのメソッドはまさにその型に対して型検査済みなので健全（`print-object` と
同じ仕組み）。`string`/`bool`/`char`/`symbol`/リストのような即値も引ける。整数だけは
`i32`/`i64` を値から区別できないため、**両方が同名メソッドを定義しているときだけ**エラーになる。
メソッドが無いのはエラー（`~a` と違い戻り先が無い）。**AOT 実行ファイルでは使えない**——
到達しうるメソッドをコンパイル時に決められないため。

```lisp
(defstruct point (x i64) (y i64))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```

CL の `~:a`/`~@[` の nil 特有挙動は typelisp の `false` に読み替える
（nil は無い）。pretty-printer 系ディレクティブ（`~_` `~i` `~:t` `~<...~:>`、および `~a`/`~s`/`~w` の
整形経路）は `*print-pretty*` が偽のとき CL 同様すべて no-op——既定は偽なので、既存の出力は一切変わらない。
詳細は下の §15.1。

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; 出力せず文字列だけ得る
  (println "~a" s))                   ; => id=42
```

`print`/`println`/`format` は呼び出しのたびに即座に `flush` する（パイプ経由でも標準入力を読む前に
プロンプトが確実に見えるようにするため）。書式エンジンは
[typelisp-print](../crates/typelisp-print/src/format.rs)（制御文字列を [`Node`] 木にパース→引数リストに
対して解釈。`~a`/`~s` の値描画は GC ヒープ走査が要る Rust 専用処理）。可変長引数を `Sexpr` リストへ
まとめる特殊形は [checker.rs](../src/check/checker.rs) の `check_format`/`check_print_like`。

**JIT/AOT コンパイルできる**（2026-08-18）。ヒープから読めない2つの事実——enum の変種*名*と型の
`print-object` メソッド——は `PrintEnv` 越しに渡す。インタプリタは自分のスコープ木から答え、AOT 実行
ファイルは起動時に登録したテーブルから答える。整形セッション（`pprint-logical-block`）も一箇所
なので、インタプリタのブロックの中で compiled な `println` を呼んでも同じバッファに入る。

### 15.1 pretty printer（CLHS 22.2）

CL の Lisp Pretty Printer 相当。**行幅に収まらない出力を、論理ブロックと条件改行の指定に従って
折り返す**。実体は [pprint.rs](../src/eval/pprint.rs)。

#### 制御変数

CL では特殊変数（`let` で動的に束縛する）だが、typelisp に動的束縛は無いので**通常の代入可能な
グローバル**（prelude の `defvar`）。`setf` した時点から以降のすべての印字に効く。

| 変数 | 型 | 既定 | 意味 |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | 真なら `~a`/`~s`/`~w` と pretty ディレクティブが整形経路に入る |
| `*print-right-margin*` | `i64` | `80` | 右マージン（桁）。0 以下は「マージン無し＝折らない」 |
| `*print-miser-width*` | `i64` | `0` | miser スタイルに入る幅。0 以下は CL の `nil`（miser 無効）に相当 |

既定が `false` なのは、既存プログラムの出力を一切変えないため（CL でも初期値は処理系定義）。
`pprint` 系と `pprint-logical-block` は `*print-pretty*` に関わらず常に整形する（CL の `pprint` の定義通り）。

上の3つが「どう並べるか」を決めるのに対し、**「どこまで印字するか」**を決める CLHS 22.1.1 の
制御変数が別に3つある。こちらは整形の有無に関係なく、`print`/`println`/`format`/`pprint` の
すべてに効く（§15.3）。

#### 既製レイアウト（特殊形）

`print` と同じく特殊形なので、引数はどんな型でもよい（各自の型のまま `Sexpr` へ包まれる）。

| 名前 | 形式 | 説明 |
|---|---|---|
| `pprint` | `(pprint x)` | 既定レイアウトで整形出力。CL 準拠で**先頭に改行**を出し、末尾には出さない |
| `pprint-fill` | `(pprint-fill x)` | 1行に入るだけ詰める（語詰め）。改行は出さない |
| `pprint-linear` | `(pprint-linear x)` | 全要素が1行に収まらなければ**1要素1行**。改行は出さない |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | `colinc` 桁の表形式（既定 16）。改行は出さない |

既定レイアウト（`pprint` / `*print-pretty*` 下の `~a`）は、CL の既定 `*print-pprint-dispatch*` に倣って
`(quote x)` を `'x` と略記し、`defun`/`let`/`if`/`lambda` 等のコード形は「頭部＋規定個数の引数を1行目、
残りの本体を2桁字下げして1行ずつ」に整形する。それ以外のリストは fill（語詰め）。

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i64 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i64
;;   (+ x 1)
;;   (* x 2))
```

#### 論理ブロックを自分で組む

| 名前 | 形式 | 説明 |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | 論理ブロックを開く特殊形。`obj` は `pprint-pop` が辿るリスト（辿らないなら `()`）。`:prefix` と `:per-line-prefix` は排他（CL と同じ） |
| `pprint-newline` | `(pprint-newline kind)` | 条件改行。`kind` は `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | 字下げ。`kind` は `:block`（ブロック起点から）/ `:current`（現在桁から） |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | タブ。`kind` は `:line` / `:section` / `:line-relative` / `:section-relative` |
| `pprint-pop` | `(pprint-pop)` | ブロックのリストから次の要素を取る（尽きていれば `()`） |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | リストが尽きたか |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | 尽きていれば囲む `loop` を `break`（マクロ） |

typelisp には第一級ストリームが無いので、**開いている論理ブロックは暗黙のインタプリタ状態**（GC ヒープと
同じ扱い）。最も外側の `pprint-logical-block` が開始し、それが閉じたときに一括で整形して標準出力へ書く。
開いている間は `print`/`println`/`(format true ...)`/`pprint` の出力もすべてそのブロックへ入るので、
**内容は普通の `print` で書き、改行位置だけ `pprint-newline` 等で指定する**——CL のコードとほぼ同じ形になる。

`pprint-exit-if-list-exhausted` は CL ではブロックからの非局所脱出だが、typelisp に汎用の脱出機構は
無いので**囲む `loop` からの `break`** として実装している。CL 側の定型もつねに `loop` の中に書くので、
実用上の書き味は変わらない。

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

条件改行の判定規則（CLHS `pprint-newline`）:

- `:mandatory` — つねに折る。
- `:linear` — 囲む論理ブロックが1行に収まらなければ折る。ブロック単位の判定なので、**同一ブロックの
  `:linear` はすべて一緒に折れる**（`pprint-linear` の「全部1行か1要素1行か」はこれ）。
- `:fill` — (a) 次の区間が行の残りに収まらない、(b) 直前の区間が1行に収まらなかった、
  (c) miser スタイルでブロックが1行に収まらない、のいずれかで折る。
- `:miser` — miser スタイル（ブロックの開始桁が右マージンから `*print-miser-width*` 以内）のときだけ
  `:linear` として働く。

### 15.2 `print-object`（型ごとの印字表現）

`impl print-object <型>` を書くと、`print`/`println`/`format`/`pprint` がその型の値を——
**リストの中に入れ子で埋まっていても**——その実装で印字する。CL の CLOS 総称関数
`print-object`（CLHS 22.1.4）に対応する。

```lisp
(deftrait print-object
  (print-object ((self Self) (escape bool)) string))
```

| 引数 | 意味 |
|---|---|
| `self` | 印字する値 |
| `escape` | CL の `*print-escape*`。`~s`/`prin1`/`pprint` で `true`（reader 構文）、`~a`/`princ` で `false`（人間向け）。気にしない実装は無視してよい |

戻り値の `string` がそのまま出力に流れる。組み込み型には**一切実装を入れていない**ので、
`impl` を書かない限り既存の出力（`#<point 1 2>` 形式）は1バイトも変わらない。

```lisp
(defstruct point (x i64) (y i64))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   ← 入れ子でも効く
```

pretty printer とも合成される（§15.1）。`*print-pretty*` が真なら、実装が返した文字列を
含むリストが右マージンで折り返される。

#### 設計上の約束ごと

- **登録は静的**。`impl` はふつうのメソッド定義として型検査されるので、型名の打ち間違いも
  シグネチャ違いもコンパイルエラーになる。別建ての登録表は無い。
- **ジェネリック型には効かない（既知の穴）**。`(impl print-object box<T> ...)` は型検査を通り、
  `(print-object x true)` と名前で呼べば動くのに、プリンタからは見えず組み込み表現のまま出る。
  プリンタは値が持つ型キーでメソッドを引くが、単型化が型引数を消しているのでキーは `box` で
  あって `box<i64>` ではなく、値の側に「どの実体化なのか」が書かれていない。`Array<T>` の
  `print-object` と `*print-array*` がまだ無いのはこれが理由（docs/dev/TODO.md）。
- **選択は印字時**。どのディレクティブがどの引数を消費するかは制御文字列の実行時の中身で
  決まるため、`~a` と `~s` の区別（＝`escape`）は印字の瞬間にしか分からない。CLOS が
  `print-object` メソッドを「クラスごとに定義し、印字時に選択する」のと同じ。
- **再入は組み込み表現へフォールバック**。実装が `(format false "~a" self)` と自分自身を
  印字すると無限再帰になるので、印字中の値が再び現れたら組み込み表現に落とす。深さ制限では
  なく値の同一性で見るので、正当な自己参照構造の入れ子印字は妨げない。
- **ジェネリック型の実行時選択は不可**。`Vector<point>` の箱は実行時には要素型を持たない
  （`vector` としか名乗らない）ので、`impl print-object Vector<T>` を実行時に選ぶことは
  できない。ただし組み込みの `Vector` 描画が要素へ再帰し、各要素がそこでディスパッチされる
  ので、見え方は揃う。
- CL のもう一方の機構 `set-pprint-dispatch` / `*print-pprint-dispatch*`（型指定子をキーに
  した実行時の登録表）は**採用しない**。文字列キーもプリンタのシグネチャも無検査で、
  「登録時点で分かっていた型を捨ててから `match` で復元する」形になり、静的型付け言語には
  合わない。非採用の確定事項としては
  [dev/language-design.md](dev/language-design.md) §9、経緯は
  [dev/implementation-log.md](dev/implementation-log.md)（旧 TODO T5-b の節）。

### 15.3 印字量の制御（`*print-level*` / `*print-length*` / `*print-circle*`）

CLHS 22.1.1 の「値のどこまでを印字するか」を決める制御変数。§15.1 の3つと同じく prelude の
代入可能なグローバルで、`print`/`println`/`format`/`pprint` のすべてに——`*print-pretty*` の
真偽にかかわらず——効く。

| 変数 | 型 | 既定 | 意味 |
|---|---|---|---|
| `*print-level*` | `i64` | `0` | この深さ以上に入れ子になったオブジェクトを `#` で置き換える。印字対象そのものが深さ 0。0 以下は無制限 |
| `*print-length*` | `i64` | `0` | リストの要素（`defstruct`/`defenum` 値のフィールドも）をこの個数まで印字し、残りを `...` にする。0 以下は無制限 |
| `*print-circle*` | `bool` | `false` | 真なら、印字前に値を走査して**2回以上現れるオブジェクトにラベルを振る**。最初の出現が `#n=…`、以降が `#n#` |

CL は「無制限」を `nil` で表すが typelisp に `nil` は無いので、`*print-right-margin*` 等と同じく
**0 以下を無制限**とする。既定はすべて「制限なし／ラベルなし」で、CL の初期値とも既存の出力とも
一致する。

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

#### `*print-circle*` と循環構造

**循環した構造を印字できるのはこの変数を真にしたときだけ**である。偽（既定）のまま自分自身を
指す値を印字すると、プリンタは循環を辿り続けてプロセスが落ちる——これは CL でも同じ挙動
（CLHS は `*print-circle*` が偽のときの循環構造の印字を未定義としている）。

循環は「`defstruct` のフィールドを `setf` で自分自身に向ける」経路でのみ作れる（`Sexpr` の
cons セルは作成後に書き換えられないので、`'(1 2 3)` のようなリストが循環することはない）:

```lisp
(defenum link (no-link) (to node))
(defstruct node (val i64) (next link))

(let ((a (node::new 1 (link::no-link))))
  (setf a::next (link::to a))       ; a が a 自身を指す
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node 1 (to #1#)>
```

ラベルは**1回の印字対象ごとに 1 から振り直す**（CL と同じ）。循環していなくても、同じ
オブジェクトが2回現れれば `#1=`/`#1#` が付く——「この2つは同一のオブジェクトだ」という情報を
出力に残すための CL の仕様どおりの挙動:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

共有が1つも無い値では**ラベルは一切現れない**ので、この変数を真にしたまま普段のコードを動かしても
出力は変わらない。

#### 基数・大小・読み戻し（cl-parity-plan.md Phase 7b）

| 変数 | 型 | 既定 | 意味 |
|---|---|---|---|
| `*print-base*` | `i64` | `10` | 整数（`i64` と `bignum`）を印字する基数。2〜36 の外は**印字エラー**（CL も範囲を規定している） |
| `*print-radix*` | `bool` | `false` | 真なら基数の印を付ける。`#b`/`#o`/`#x`、それ以外は `#NNr`、基数 10 は末尾の `.`。印は符号の**前**（`#x-ff`） |
| `*print-case*` | `symbol` | `:downcase` | シンボル名の大小。`:upcase` / `:downcase` / `:capitalize`（CL と同じ綴り。この言語のキーワードは自己評価する `symbol`） |
| `*print-readably*` | `bool` | `false` | 真なら読み戻せる形で印字する。エスケープを強制し、`*print-level*`/`*print-length*` の打ち切りを無効化する |
| `*print-lines*` | `i64` | `0` | pretty printer が使ってよい行数。超えた分は切り、末尾に CL と同じ `..` を付ける。0 以下は無制限 |
| `*print-escape*` | `bool` | `true` | `write`/`write-to-string` が `prin1` と `princ` のどちらをするか。**これを読むのはその 2 つだけ** |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

`*print-radix*` が付ける印はリーダが読み戻せる（§16 の radix マクロ）。

**`*print-case*` の既定が CL と違う理由**: CL の既定は `:upcase` だが、それは CL のリーダが
シンボル名を大文字で格納するから——つまり「格納されているまま」の意味。このリーダは小文字で
格納するので、同じ意味になる既定は `:downcase`。

**`*print-readably*` に無い半分**: CL は読み戻せない値に `print-not-readable` を上げるが、
この言語には上げるコンディションが無く、`print-object` が何でも印字しうるユーザ型について
可否を決める手段も無い。エスケープと打ち切りの上書きだけが入っている。

**`*print-escape*` を読むのが `write` だけな理由**: CLHS どおり `~s`/`prin1`/`pprint` は
これを真に、`~a`/`princ` は偽に、それぞれ自分の呼び出しの間だけ束縛する。つまり誰も
束縛していない状態で読まれるのは `write`/`write-to-string` だけ。`print-object` の実装は
この大域変数ではなく自分の `escape` 引数を読むこと——そちらが directive の選んだ値を運ぶ。

**CL にあって無いもの**: `*print-gensym*`（未 intern シンボルが無い）。
`*print-array*` は入っていない——`Array<T>` 自身の印字が要り、それには
`print-object` がジェネリック型に効く必要がある（§15.2 の「既知の穴」）。

#### 一時的な差し替え

CL はこれらを `let` で束縛するが、この言語の `let` は字句束縛なので `dlet`（§14）を使う:

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; この 1 回だけ制限が効く
(with-standard-io-syntax (println "~a" x))   ; 全部を標準値に戻して印字
```

## 16. 解析・評価 (`parse-int` / `parse-float` / `read` / `eval`)

いずれも実行時の（プログラム自身は制御できない）テキスト・データを扱うため、失敗時は panic では
なく `Result` の `Err` を返す。エラー型は Rust の std に倣い**操作ごとの具象型**（§7.1）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `parse-int` | `(parse-int s)` | `string→Result<i32,ParseIntError>` | 10進整数（`+`/`-`前置可）。Rust の `str::parse::<i32>` と同じ受理範囲 |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | 浮動小数点数。Rust の `str::parse::<f64>` と同じ受理範囲（`inf`/`nan`含む） |
| `read` | `(read s)` | `string→Result<Sexpr,ReadError>` | `s` から `Sexpr` を1つ読む（`typl`/REPL がソーステキストを読むのと同じ reader を使う）。不完全な括弧・文字列などは `Err`。CL の `read-from-string` に当たる——ストリームから読むのは `read-sexpr`（§18.5） |
| `read-from-string` | `(read-from-string s [start])` | `(string,i64)→Result<cons-cell<Sexpr,i64>,ReadError>` | `read` に**読み終わり位置**を添えたもの。`(car r)` が値、`(cdr r)` が次に読む文字位置。`start` 省略時は 0 |
| `read-from-string-preserving-whitespace` | 同上 | 同上 | 同上だが datum を終わらせた空白を消費しない。違いは返る位置に出る |
| `eval` | `(eval form)` | `Sexpr→Result<Sexpr,EvalError>` | `form` を実行時に型チェックして評価する。CL の `eval` に準拠 |

CL は `read-from-string` から**2 値**（値と位置）を返すが、この言語に多値は無いので
`cons-cell` 1 つで返す。位置があると、文字列を 1 データずつ読むのが再スキャンではなく
ループになる:

```lisp
(let ((s "1 2 3") (i (the i64 0)) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (as i64 (length s))) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

`preserving-whitespace` の違いは**空白 1 文字**だけ——CL の `read` は datum を終わらせた空白を
消費し、`read-preserving-whitespace` は残す。`(read-from-string "12 34")` は位置 3 を返し、
preserving 版は 2 を返す。

リーダが読む数値表記は10進のほか、`0x`（16進）と CL の **radix マクロ** `#b`（2進）・`#o`（8進）・
`#x`（16進）・`#NNr`（基数 NN、2〜36）。符号は印の**後ろ**（`#x-ff`）で、`i64` に収まらなければ
`bignum` になる。`*print-radix*`（§15.3）が印字するのはこの表記なので、印字したものはそのまま
読み戻せる。CL の `*read-base*` は無い——理由は cl-parity-plan.md Stage 7b の表に記録した。

### `eval` の意味論（Common Lisp 準拠）

CLHS の `eval` に準拠する: **現在の大域環境**（グローバルの関数・変数・型・マクロ。実行時に
追加された定義も含む）で、かつ **null 字句環境**（呼び出し元の `let`/`lambda` のローカル束縛は
見えない）で評価する。式でも定義（`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`）でも評価
でき、定義は即座かつ永続的にグローバル環境へ登録される。

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; グローバル x が見える
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; 定義名を返す
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; 直前の定義が見える
```

- **戻り値**: 式なら評価結果を `Sexpr` として、定義なら定義名シンボルを返す（CL と同じ）。
  結果を `println` 等で表示するには `Sexpr` を `match`（`(int n)`/`(str s)`/…）で分解する。
- **静的型ゆえの差異（重要）**: CL は結果の実値（動的型）を返すが typelisp は戻り型を一律
  `Result<Sexpr,EvalError>` にするしかない。また **静的に書いたコードは、実行時に `eval` が定義する
  名前を前方参照できない**——チェッカーは全トップレベルフォームを実行前に検査するので、
  ファイル中に直接書いた `(sq 9)` は `sq` を定義する `eval` より前に検査され「未定義」になる。
  ただし **後続の `eval` からは見える**（その `eval` の型チェックは実行時、定義後に走るため）。
  REPL は1行ずつ検査・実行するので、`eval` で定義した名前を次の行から直接呼べる。
- **エラーの扱い**: チェッカーが静的に弾ける型エラー・構文エラーは `Err` を返す（パニックしない）。
  評価したコード内の**実行時パニック**（ゼロ除算等）は、直接書いたコードと同様にそのまま伝播する
  （CL の condition system は採らないので、panic を捕まえて継続する手段は無い。ただし panic は
  unwind するので、途中の `unwind-protect` の cleanup は走る——[syntax.md](syntax.md) §8）。
- **名前空間**: `typl file.typl` 実行時、`eval` はそのスクリプトのファイル由来モジュール名前空間で
  評価される（スクリプト自身のグローバルが見える）。REPL はルート名前空間で評価する。
- **コンパイル**: `read` も `eval` も JIT/AOT コンパイルできる（`read` は 2026-08-18、`eval` は
  2026-08-19）。JIT では走っているインタプリタがそのまま環境になる。AOT では `compile-file` が
  環境（検査済みの大域状態）をコンパイル時に組み立てて実行ファイルに書き込み、起動時は復元
  するだけ——`eval` を呼ぶプログラムだけが、そのぶんの起動時間とサイズを払う。埋め込まれるのは
  `(dump ...)` と同じ形式（prelude の単位 + プログラムの単位）。詳細と帰結（eval したフォームは
  解釈実行される）は [syntax.md](syntax.md) §10。

## 17. docstring / `documentation`（Common Lisp 準拠）

`defun`/`defmethod`（`impl` 内も含む）/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftrait` は docstring を持てる。位置は CL のそれぞれの規則にそのまま従う:

| フォーム | docstring の位置 |
|---|---|
| `defun` / `defmethod` / `defmacro` | 本体の先頭（戻り値型・`where` 節の後）。ただし後ろに本体フォームが最低1つ続く場合のみ——単独の文字列は戻り値のまま |
| `defvar` / `defconstant` | 初期値の**後ろ**: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | 名前の**直後**、フィールド/バリアント列の前 |
| `deftrait` | 継承リストの直後、アイテム列の前。トレイト全体に1つ。**デフォルト実装を持つメソッド**は、その本体の直前に自分の docstring を置ける |

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `documentation` | `(documentation name)` | （特殊形。`name` は裸シンボルまたは `Type::method`）→`Option<string>` | `name` の docstring を返す |

`documentation` は `quote`/`compile` と同様の特殊形（`name` を評価せず、未評価の名前として読む）。
CL の `(documentation 'name 'function)` と異なり型引数を取らない代わりに、裸名を**変数→関数→型→
トレイト→マクロ**の順（式として評価するときの裸識別子の優先順位と同じ）で解決する。`Type::method`
の形なら関連メソッド/静的メソッドの docstring を引く。

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**check 時に定数へ畳み込まれる**: `documentation` はランタイムのルックアップを一切行わない
（`Checker` は常にどの定義を指しているか静的に分かるため）。名前が何の定義にも解決できない場合は
check 時のエラー（未定義変数参照などと同様）。解決はできたが docstring が無い場合のみ `Option::none`。

**対象外**:
- `(setf documentation)`（docstring の実行時書き換え）は無い。
- モジュール修飾された自由名（`mod::name`。`Type::method` は対応）は非対応。
- `deftrait` 内の**本体を持たない**メソッド宣言は docstring を持てない。末尾の文字列リテラルは
  それ自体がデフォルト実装の本体（＝戻り値）になるので、両者を区別する手段が無い。
  デフォルト実装を持つメソッドは、`where` 節の後・本体の前に docstring を置ける
  （通常どおり「後ろに本体が続くときだけ docstring」の規則が効く）。

LSP のホバーにも統合されている: 定義済みの名前にカーソルを合わせると、型の下に docstring が
表示される（`src/check/locate.rs` の `doc_for`/`hover_text`）。

## 18. ストリームとファイル I/O

CL がクラス階層で表すものを、ここでは**トレイト階層**で表す。方向（入力／出力）も要素型も
**静的**に決まるので、「このストリームは読めるか」を実行時に尋ねる必要がない。

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; 文字入力
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; 文字出力
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; 1文字押し戻せる入力
```

文字を読む関数は `(where (CharInput S))` か `:dyn CharInput` を取れば、組み込み・ユーザ定義を
問わずあらゆるストリーム型を受け付ける。

### 18.1 メソッド（すべてトレイト経由）

`CharInput` の全メソッドはデフォルト実装を持つ。実装側が書くのは `read-item` だけ。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | 次の1要素。末尾なら `none`。**唯一の実装必須メソッド** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | 次の1文字 |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | 次の改行まで（改行は消費して除去）。改行で終わらない最終行も返る |
| `read-all` | `(read-all s)` | `(S)→string` | 残り全部 |

`PeekInput`（`CharInput` を継承）は**1文字の押し戻し**を足す。デフォルト本体を持てない唯一の
入力操作なので別トレイトにしてある——押し戻した文字を置く場所はストリーム自身しか持たない。
組み込みのリーフストリーム（`file-stream`/`string-input-stream`/`standard-stream`）は実装済み、
それ以外は `make-peek-stream` で包めば得られる（§18.3）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | 次の読みが `c` を返すようにする。**唯一の実装必須メソッド**。CL 同様、保証は1文字だけ |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | 消費せずに次の1文字を見る |

`CharOutput` も同様に、実装側が書くのは `write-item` だけ。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | 1要素を書く。**唯一の実装必須メソッド** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | 1文字書く |
| `write-string` | `(write-string s str)` | `(S,string)→()` | 文字列を書く |
| `write-line` | `(write-line s str)` | `(S,string)→()` | 文字列＋改行 |
| `terpri` | `(terpri s)` | `(S)→()` | 改行を1つ（CL の名前） |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | 行頭でなければ改行を1つ |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | 次に書く文字が行頭になるか。デフォルトは `false`（＝`fresh-line` は改行を書く。分からないなら書くほうが安全）。組み込みストリームは全て上書き済み |
| `finish-output` | `(finish-output s)` | `(S)→()` | バッファを送り出す |

`at-line-start` が覚えているのは**そのストリーム経由で書かれた分だけ**。`print`/`println`/
`(format true ...)` は標準出力へ直接書く（`*standard-output*` のハンドルを通らない）ので、
両者を混ぜると `(fresh-line *standard-output*)` の判断は `println` が書いた改行を知らない。
片方に寄せること。

`Stream` は全ストリーム共通:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | まだ開いているか |
| `close` | `(close s)` | `(S)→()` | 閉じる。**GC では閉じられない**ので明示的に（または `with-open-file` で） |

### 18.2 具象ストリーム型

| 型 | 作り方 | 実装するトレイト |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` は `direction-input` / `direction-output` / `direction-append` の3定数。
`open-file` は開けなければ `Err(FileError)` を返す（存在しないファイルは普通の結果であって
panic ではない）。ファイル名は文字列でも `pathname` でもよい（§19 の `Pathish`）。

`(get-output-stream-string s)` は `string-output-stream` に書かれた内容を返して空にする。
CL 同様、`close` 後でも取り出せる。

**バイト I/O** は `ByteInput`/`ByteOutput`。`InputStream`/`OutputStream` の `Item` を
`i64` に固定したもので、`CharInput`/`CharOutput` が `char` に固定しているのと同じ形。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<i64>` where `ByteInput S` | 次の1バイト。ファイル終端で `none` |
| `write-byte` | `(write-byte s b)` | `(S,i64)→()` where `ByteOutput S` | 1バイト書く。0..255 の外はエラー |

CL は `(open name :element-type '(unsigned-byte 8))` と要素型を**呼び出し**で決めるが、
ここでは要素型はストリームの**型**なので、違うのは開く関数の側になる。文字ストリームから
バイトを読むことは型エラーであり（`string-input-stream` は `ByteInput` を実装しない）、
native 層でも拒否する——次の文字の UTF-8 エンコーディングを返すのは、そこに無いファイルを
発明することだから。`unread-char` が保留中のストリームからのバイト読みも同じ理由で拒否する。

### 18.3 合成ストリーム

いずれも**ただの `defstruct`** で、ネイティブ層の支援を必要としない。入れ子にもできる。

| 名前 | 形式 | 説明 |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | `Vector<:dyn CharOutput>` の全てへ書く |
| `make-two-way-stream` | `(make-two-way-stream in out)` | `in` から読み `out` へ書く |
| `make-echo-stream` | `(make-echo-stream in out)` | `in` から読み、読んだ文字を `out` にも書く |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | `Vector<:dyn CharInput>` を順に読み継ぐ |
| `make-peek-stream` | `(make-peek-stream in)` | 任意の `:dyn CharInput` に1文字の押し戻しを足して `PeekInput` にする（`read-sexpr` 用） |

### 18.4 マクロ

| 名前 | 形式 | 説明 |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | 開く→本体→閉じる。`Result<本体の値, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | 文字列から読む |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | 書かれた内容を返す |

### 18.5 ジェネリック関数とファイル操作

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | 全部転送 |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | 残り全行 |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | `Sexpr` を1つ読む（CL の `read`）。入力末尾は `Ok(eof)`、読めたときは `Ok(datum d)`、データでなければ `Err`。datum を終わらせた**空白1文字を消費する**（CL と同じ）。`ReadOutcome` が `Option<Sexpr>` でないのは、空リスト `()` を読んだことと入力末尾とを同じ値で表さないため |
| `read-sexpr-preserving-whitespace` | 同上 | 同上 | 同上だが空白を残す（CL の `read-preserving-whitespace`） |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Sexpr,ReadError>` where `PeekInput S` | `ch` まで読んでリストにする。`ch` は消費。入力が尽きたら `Err` |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | 1行ずつ書く |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | 全内容 |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 全行 |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | 書き出す |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | 存在するか |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | 削除・改名（引数は `Pathish`） |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | シンボリックリンクと `.`/`..` を解いた絶対パス。存在しなければ `Err` |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<i64,FileError>` where `Pathish P` | 最終更新時刻。**万国時**なので `decode-universal-time`（§4.6）が読める |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | ディレクトリか。**無い場合も `false`** ——両者を分けるのは `probe-file` |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 中身を絶対パスで並べる。`.`/`..` は入らない。順序は OS のまま |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | 親ごと作る。既にあれば成功（「ensure」の意味） |

ファイルを名指しする引数は全て**文字列でも `pathname` でもよい**——CL のパス名指定子と同じ扱いで、
実行時の型テストではなく `Pathish` トレイトで解決している（§19）。

### 18.6 自分の型をストリームにする

`write-item` を1つ書けば、残りはデフォルト実装が付いてくる。合成ストリームにも入れられる。

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; 残りのメソッドは全部デフォルト

(write-line (counter::new 0) "四文字")  ; write-line も terpri も fresh-line も動く
```

入力側も同じで、書くのは `read-item` だけ。押し戻しを自前で持たない型でも、
`(read-sexpr (make-peek-stream my-stream))` と包めば `read` できる。

`read-delimited-list` の終端文字は**トークンも終わらせる**。CL は終端文字をリードテーブルの
terminating macro character にすることでこれを実現するが、リードテーブルが無い（Phase 8c）ので
スキャナに直接渡している。効くのは深さ 0 だけで、`(1 2]` の `]` はリスト自身のテキストの一部
として `read` に渡り、壊れたリストとして報告される。CL の第3引数 `recursive-p` に対応物は無い
（リーダマクロが無いので伝える相手がいない）。

### 18.7 CL との違い

- **クラス階層ではなくトレイト階層**。`input-stream-p` / `output-stream-p` は無い——方向は型が
  持つので、実行時に尋ねる問いではない。
- **`read` は文字列版とストリーム版で名前が違う**。`(read "...")`（CL の `read-from-string`）と
  `(read-sexpr s)`（CL の `read`）。単一・静的ディスパッチなので同名の多重定義ができない。
- **押し戻しは別トレイト**（`PeekInput`）。`read-char` しか要らない型に `unread-char` の実装を
  強いないため。
- **閉じるのは明示的**。GC はクローズしない（コレクタは cons アリーナ枯渇時にしか走らないので、
  ファイナライザは予測できない時点で動くか一度も動かない）。`with-open-file` を使うのが安全。
- **ストリーム操作は JIT/AOT コンパイルできる**（2026-08-14 の「コンパイル経路の穴」で解消。
  ストリーム表を `typelisp-rt` へ移したため、インタプリタの無い AOT 実行ファイルからも同じ表を
  引ける）。prelude のストリーム定義は事前コンパイル済みで出荷される。コンパイルできないものは
  [syntax.md](syntax.md) §10 に一覧がある（2026-08-18 時点で `eval` のみ）。

## 19. パス名 (`pathname`)

ファイル名を分解した値。`/` 区切りのディレクトリ成分・名前・型（拡張子）と、ルート始まりかどうか
を持つ。分解も再構成も純粋な文字列処理なので**この層は全て typelisp で書かれている**——ネイティブ
側が見るのは `namestring` が描いた文字列だけ。

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => ["var" "log"]（Vector<string>）
  (pathname-name p)        ; => (some "app.tar")   最後のドットで切る
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 19.1 パス名指定子トレイト `Pathish`

CL がパス名指定子（文字列 or パス名）を受ける場所で、こちらは `Pathish` を受ける。`string` と
`pathname` の両方が実装しており、**ファイル操作は全てこれをジェネリックに取る**ので、
`(open-input "a.txt")` と `(open-input p)` はどちらも普通の呼び出し（実行時の型テストは無い）。
文字列側の `namestring` は自分自身を返すだけなので、文字列を渡す限りパースは走らない。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | 文字列表現。**唯一の実装必須メソッド**（`to-pathname` と2つ） |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | `pathname` に直す（CL の `pathname` 関数。型名と衝突するので改名） |

### 19.2 関数

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | 文字列を分解する。末尾 `/`（や空名）は「名前無し」＝ディレクトリ |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | 持っている成分だけで組み立てる（全て `&key`）。省略した名前・型は「無い」ままで、`merge-pathnames` が埋める対象になる |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | 外側から順のディレクトリ成分 |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | 型を除いた名前。ディレクトリなら `none` |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | 最後のドット以降。先頭のドットは対象外（`.gitignore` は全部が名前） |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | ルート始まりか |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | ホームディレクトリ。`$HOME` が無ければ `none`（CL も `NIL` を許す） |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | 最後の `/` までの部分 |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | `name.type` の部分だけ |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | `p` に無い成分を `default` から補う。相対の `p` は `default` のディレクトリの下に置かれ、絶対の `p` は自分のディレクトリを保つ |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | `default` を基準にした相対表記。基準の下に無ければ `p` の全体 |

型引数はいずれも `(where (Pathish P))`。

### 19.3 CL との違い

- **ホスト・デバイス・バージョン成分は無い**。ワイルドカードパス名も `directory` による照合も、
  論理パス名（`logical-pathname`）も無い。CLHS 19 のそれらの部分は、この処理系が走らない
  ファイルシステムのためにある。区切りは `/` 固定。
- **`pathname` 関数は `to-pathname`**。型とトレイト・関数が同じ名前空間を共有するため。
- **ワイルドカードによる照合は無い**ので、`directory` は「そのディレクトリの中身を並べる」だけの
  関数になっている（§18.4）。CL の `directory` はパス名のパターンと照合する。
