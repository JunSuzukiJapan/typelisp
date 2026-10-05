# 数値

整数・浮動小数点数・有理数・複素数・真偽値の演算と、数値まわりの関数。
呼び出し形式の読み方は [組み込み関数](README.md) を参照。

## 1. 固定幅整数

整数型は 7 つ。**`int`**（CL の `integer`——任意精度、未注釈の整数リテラルの既定型。3 章）と、
固定幅の `i8` `i16` `i32` `u8` `u16` `u32`。第一引数の型でどれ用に解決されるかが決まる
（互いに独立で、暗黙変換はない）。**64bit 幅の整数型は無い**——実行時の値は下位ビットがタグの
1語なので即値の整数には63bitしか残らず、64bitを名乗る型はどこかで最上位ビットを落とすことに
なる。`int` はその 63bit を超えたら多倍長になるので、幅を気にしないなら `int` を使う。
以下の表は固定幅の 6 つのもの（`int` の表は 3 章）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | 四則演算。`/` はゼロ方向切り捨て・ゼロ除算で panic |
| `mod` | `(mod a b)` | `(T,T)→T` | 剰余（CL の `mod`、**床除算**＝符号は除数側。`(mod -7 3)`→`2`）。ゼロ除算で panic |
| `rem` | `(rem a b)` | `(T,T)→T` | 剰余（CL の `rem`、**切り捨て除算**＝符号は被除数側。`(rem -7 3)`→`-1`）。ゼロ除算で panic |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | CL の2引数 `floor`/`ceiling`/`round`/`truncate`（`(floor 7 2)`→商3・剰余1）に相当。多値の代わりに商・剰余を `cons-cell`（`car`=商、`cdr`=剰余）で返す。`round-div` は同点を CL 準拠で偶数側に丸める |
| `abs` | `(abs x)` | `T→T` | 絶対値 |
| `signum` | `(signum x)` | `T→T` | 符号（`1`/`-1`/`0`） |
| `gcd` | `(gcd a b)` | `(T,T)→T` | 最大公約数 |
| `lcm` | `(lcm a b)` | `(T,T)→T` | 最小公倍数（どちらかが0なら0） |
| `max` `min` | `(op a b)` | `(T,T)→T` | 大きい方／小さい方（3引数以上は 8 章の可変長の糖衣で展開） |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | いずれも `=` と同じ（同型の数値に差はない） |
| `int->float` | `(int->float x)` | `T→f64` | `f64` への拡大変換 |
| `int->int` | `(int->int x)` | `T→int` | `int` への拡大変換（常に正確）。`(as int x)` の実体 |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | `ratio` への拡大変換（常に正確） |
| `int->char` | `(int->char x)` | `T→char` | Unicode スカラ値として解釈。不正な値は panic |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | `int->char` の失敗を `None` で返す版 |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | 幅変換。入らない値は切り詰める（Rust の `as` と同じ） |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | 同じ変換を問いとして。値がその幅に入らなければ `None` |

これらの変換は `(as Type x)`/`(try-as Type x)` 特殊形（[構文リファレンス](../syntax.md#7-その他の特殊形)）の
実体でもある。ビット演算（`logand`/`ash`/`ldb` 等）と述語（`zerop`/`evenp` 等）は型をまたいで同じ形
なので 9 章と 11 章にまとめてある。

`i8` `i16` `u8` `u16` `u32` はこの章の表をそのまま持ち、`f32` は 4 章の `f64` の表をそのまま持つ。

**型名は幅と符号そのもの**——`i32` は「32bit を符号付きとして扱う」、`u32` は「32bit を
符号なしとして扱う」以上の意味を持たない。`(+ (the u8 200) (the u8 100))` は `44`、
`(+ 2147483647 1)`（`i32`）は `-2147483648`、`(lognot (the u32 0))` は `4294967295`。
`f32` も同じで、本物の binary32——`(/ (the f32 1.0) (the f32 3.0))` は `0.33333334` と
印字され、`f64` の `0.3333333333333333` とは別の値になる。

CL の派生カタログ（`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` と 9 章の述語）は
`int`/`i32`/`f64`/`ratio` にある。他の幅で必要なら `(as int x)` / `(as i32 x)` で移る
（幅変換は全ペアにある）。

## 2. C 境界の生の語（`ptr` / `c-long` / `c-ulong`）

[`defffi`](../syntax.md#33-defffi--c-関数の宣言ffi) で宣言した C 関数との受け渡しにだけ使う3つの型。
`ptr` は不透明なポインタ、`c-long` / `c-ulong` は C の `long` / `unsigned long`。値にするには
`(unsafe ...)` の中に居ることが要る。

**算術は無い。** 1 章の表は1つも付かない——`(+ p 1)` も `(< n m)` も書けない。これらは C へ渡す
語であって計算する型ではないので、計算したければ幅のある型へ移る。`c-long` / `c-ulong` に
付いているのは変換だけ:

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | 1 章と同じ幅変換。入らない値は切り詰める |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | 同じ変換を問いとして |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | 生の語どうし、および 1 章の整数型から作る入口 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | 同上 |
| `int->int` | `(int->int x)` | `T→int` | **常に正確**。`i32` に入らない `size_t` を読む正直な方法 |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` がこれらの実体で、変換は
1 章の整数型との全ペアにある。`ptr` にはこの表も付かない——ポインタを数として読む道は用意して
いない。渡す・受け取る・別の C 関数へ渡し直すだけの値。

**印字もできない。** `(println "~a" x)` は生の語を受けない（`Sexpr` 表現を持たないため）ので、
`(println "~a" (as int n))` のように幅のある型へ移してから渡す。

1 章冒頭の「64bit 幅の整数型は無い」はこの3つでも破られていない。**保存できないから**成り立つ:
`defstruct` のフィールドにも `defvar` にも型引数の内側にも `Sexpr` の中にも置けず、引数・戻り値・
局所変数として関数の中を通り抜けるだけの語になっている。詳しくは
[構文リファレンス](../syntax.md#ptr--c-long--c-ulong--生の機械語)。

## 3. 任意精度整数 `int`

CL の `integer`、そしてこの言語の**整数**——未注釈の整数リテラルはこの型で、`length` や
`char->int` のように数を返す組み込みもこの型を返す。値は 63bit の即値（fixnum）に入るあいだは
即値で、演算の結果が入らなくなれば自動的に多倍長へ昇格し、収まればまた即値へ戻る。
`eq` は fixnum の範囲で常に値の同一性、`eql`/`=` は範囲を問わず数値の同一性になる。固定幅の
整数型（1 章）とは別の型で、暗黙変換はない——`(as int x)` が固定幅からの正確な拡大、
`(as i32 n)` / `(try-as i32 n)` が `int` からの切り詰め / 判定（1 章の `int->W` / `try-int->W` と
同じ意味）。

`Sexpr` の整数の変種も `int` 1 つ（`(int n)` は fixnum も多倍長も受ける）。

添字や個数を取る組み込み（`substring`、`Vector` の `get`、`ash` の桁数など）は `int` を受けるが、
fixnum に入らない値を渡すと実行時エラー（「an integer argument does not fit a fixnum」）になる。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | 溢れない（昇格する） |
| `/` | `(/ a b)` | `(int,int)→int` | ゼロ方向切り捨て。ゼロ除算で panic |
| `mod` | `(mod a b)` | `(int,int)→int` | 床除算の剰余（符号は除数側） |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | すべて `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | 11 章と同じ（無限桁の 2 の補数） |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | 1 章と同じ |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | 切り詰め / 判定。`W` は 6 幅と `c-long`/`c-ulong` |
| `int->int` | | `int→int` | 恒等（固定幅・C 語の側の `int->int` が拡大。1 章） |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | 1 章と同じ形。`expt` は非負の指数のみ |

## 4. 浮動小数点数（`f64` / `f32`）

`f32` も同じ表を持つ。

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
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | 大きい方／小さい方（3引数以上は 8 章の可変長の糖衣で展開） |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | 単項演算 |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | 超越関数。`log` は自然対数 |
| `log`（2引数） | `(log x base)` | `(f64,f64)→f64` | 底を指定した対数。`(/ (log x) (log base))` へ展開される（8 章） |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | CL の2引数版（`(floor 7.0 2.0)`→商3・剰余1）に相当。1 章の同名関数と同じ設計（`car`=商、`cdr`=剰余） |
| `float->int` | `(float->int x)` | `f64→int` | ゼロ方向への切り捨てで `int` へ変換（CL の `truncate`。どんな大きさの有限値でも正確）。無限大・NaN は panic。固定幅が要るなら `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | 正確な二進有理数として `ratio` へ変換（CL の `rational`） |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | 浮動小数点の幅の変換。`float->f32` は最近接へ丸め、`float->f64` は常に正確。`(as f32 x)` の実体 |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | 同じ変換を問いとして。丸めで値が変わるなら `none`（`f64` への拡大は常に `some`）。`(try-as f32 x)` の実体 |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | CL の同名関数。上の `floor`/`ceiling`/`round`/`truncate` の別名——CL では無印の方が整数を返すので、`f` 付きの方がこの言語の挙動に一致する |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | それぞれ 2 / 53 / 53（`0.0` の precision だけ 0）。`f64` は常に IEEE-754 binary64 なので定数 |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` か `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | 仮数（`[1/2,1)`、符号なし）と指数。CL は3値返しだが多値は無いので、符号は `float-sign` が担う |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | 同じ分解を厳密な 53 ビット整数の仮数で。`仮数 * 2^指数` がちょうど元の値 |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **その float に読み戻る最も簡単な**有理数（`(rationalize 0.1)` は `1/10`）。厳密な二進値が要るなら `float->ratio` |

**CL との違い: `round` の丸め方**。`round`（したがって `fround`/`round-div`）は
**0 から遠い方へ**丸める（`(round 2.5)` = `3.0`）。CL は**偶数側へ**丸めるので `2` になる。

## 5. 有理数 `ratio`

CL 準拠の任意精度有理数。常に既約・正の分母で保たれ、ヒープ確保される。整数型や `f64` との
暗黙変換はない（明示的な変換メソッドまたは `as`/`try-as` を使う）。比リテラル構文は
[構文リファレンス](../syntax.md#1-字句要素)を参照。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | 四則（結果は常に既約）。`/` はゼロ除算で panic |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | 床除算の剰余（CL 準拠、符号は除数側） |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | 切り捨て除算の剰余（CL 準拠、符号は被除数側） |
| `abs` | `(abs x)` | `ratio→ratio` | 絶対値 |
| `signum` | `(signum x)` | `ratio→ratio` | 符号（`1`/`-1`/`0` を `ratio` で返す） |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | 冪乗。指数は整数値の `ratio` のみ（非整数なら panic）。負指数は逆数 |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | 大きい方／小さい方 |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`。`ratio` にビット演算は無い（CL も整数専用） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | いずれも `=` と同じ |
| `numerator` | `(numerator x)` | `ratio→int` | 既約分子（CL と同名） |
| `denominator` | `(denominator x)` | `ratio→int` | 既約分母（常に正） |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | 整数部（ゼロ方向切り捨て） |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | `f64` へ変換 |

固定幅整数・`f64` からの入口は `int->int`/`int->ratio`（1 章）と `float->int`/`float->ratio`
（4 章）。`int`/`ratio` は `i32` 等とは独立した別型で、混在した算術には明示変換が必要。

## 6. 複素数 `complex`

標準ライブラリの構造体（`defstruct`）。

**CL との違い 2 つ**（どちらも静的型付けの帰結）:

1. **成分は `f64` 固定。** CL の complex は有理数も持て、`(complex 1 2)` と
   `(complex 1.0 2.0)` は別の型。静的な型はどちらかを選ぶ必要があり、超越関数が返すのは
   浮動小数点のほう。
2. **`(sqrt -1.0)` は実数の `sqrt`（NaN）。** CL は `sqrt` が実数から複素数を返せるが、
   `f64` の `sqrt` は `f64` を返さねばならない。複素数の結果は複素数の引数から出る——
   `(sqrt (complex -1.0 0.0))` が `i`。

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
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | ベクトル `(x,y)` の角度。**CL の 2 引数 `(atan y x)` はこれへの糖衣**（`log` の 2 引数版と同じく引数の個数で分岐） |

`print-object` を実装しているので `~a`/`~s` は CL と同じ `#C(re im)` で印字する
（読み戻す `#C` 構文はこの言語のリーダに無い）。

## 7. 真偽値

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | 否定 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | すべて値の等価比較 |

`and`/`or` は短絡評価が必要なため特殊形（[構文リファレンス](../syntax.md#4-束縛条件分岐)）。

## 8. 数値ヘルパーと呼び出しの糖衣

`abs`/`signum`（全数値型）・`gcd`/`lcm`（整数型のみ）・`rem`（`f64` 含む全実数型）・`expt`
（`int`/`f64`/`ratio`）は、各数値型のメソッドとして定義されている（受け手の型で解決される。
`(abs x)` は `x` の型に応じたメソッド）。型ごとの詳細は 1・3・4・5 章。固定幅整数の `expt` は
無い（昇格が無くオーバーフローするため、`(as int x)` で `int` に移してその `expt` を使う）。

### 8.1 可変長・0/1 引数

CL の算術・比較は可変長だが、メソッドは受け手の型でしか解決されず引数の個数では解決されない。
そこで次の形は**チェッカーが 2 引数の呼び出しへ展開する**。

| 書ける形 | 展開 | 対象 |
|---|---|---|
| `(op a b c ...)` | `(op (op a b) c)` の左畳み込み | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | 各項を一時変数に束縛した `(and (cmp a b) (cmp b c) ...)` | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | 上記のうち単位元を持つもの |
| `(op x)` | `+ * max min logand logior logxor` は `x` そのもの。`(- x)` は符号反転、`(/ x)` は逆数、`(gcd x)`/`(lcm x)` は `(abs x)`（CL 準拠） | 同上 |
| `(cmp x)` | `x` を評価して `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

各項は左から1回だけ評価される（比較の可変長版が一時変数を挟むのはこのため）。
`/=` の可変長形は**隣接ペア**の比較で、全ペアの相異を問う CL とは異なる。

### 8.2 `isqrt` と整数の `expt`

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | 平方根を超えない最大の整数。負なら panic |
| `expt` | `(expt n e)` | `(T,T)→T` | 冪乗（二乗法）。CL は負の指数に有理数を返すが、整数型では表せないので panic——`ratio` に変換してから使う |

## 9. 述語

| 名前 | 形式 | 型 | 対応する型 |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32`（CL 同様、整数型のみ） |

CL の `numberp`/`integerp`/`floatp` 等の**型述語は無い**——静的型付けなので、値の型は実行時に
尋ねるまでもなく決まっている。

## 10. 定数

| 名前 | 型 | 値 |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | `boole` に渡す演算コード（CL のキーワードの代わり） |

数値限界定数（CLHS 12.1.4.2 / 12.1.3）:

| 名前 | 型 | 説明 |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | 63bit 即値の上限／下限（2^62-1 / -2^62）。これを超えた `int` は多倍長になる |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | 有限で最大／最小 |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | 非正規化数を含む、0 でない最小の絶対値 |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | 正規化数に限った同じもの |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | CL の定義（`(/= (+ 1 e) 1)` を満たす最小の正の `e`）に従うので、2^-53 **より 1 ULP 大きい**——2^-53 自身は最近接偶数丸めで `1.0` に戻ってしまう |

## 11. ビット演算

無限精度の2の補数として定義される（CL 12.10）。固定幅整数型と `int` に実装があり、`ratio` には
無い（CL 自体もビット演算は整数専用）。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | 論理積・論理和・排他的論理和（可変長・0引数版は 8.1） |
| `lognot` | `(lognot x)` | `T→T` | ビット反転 |
| `ash` | `(ash x count)` | `(T,int)→T` | 算術シフト。`count` が正なら左、負なら右 |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | `index` ビット目が立っているか（**CL と引数順が逆**、下記） |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | 立っているビット数（負数なら 0 ビットの数） |
| `integer-length` | `(integer-length x)` | `T→T` | 符号を除いて表現に要するビット数 |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | 上記から合成した残り7種 |

**`ash` の第 2 引数だけが `T` でなく `int`。** これはビット単位の**距離**であって受け手の型の
値ではないので、受け手の幅と符号は距離について何も言わない（CL の `(ash integer count)` の
`count` が任意の整数なのと同じ理由）。符号なしの右シフトは論理シフト
（`(ash (the u8 200) -3)` = `25`）、符号付きは算術シフトで負の無限大方向へ丸まる
（`(ash (the i32 -100) -4)` = `-7`）。`logbitp` の `index` も同じ理由で `int`。

**バイト指定子**。CL の `byte` が返す不透明なオブジェクトの代わりに、
`cons-cell<int,int>`（`car`=サイズ、`cdr`=位置）を使う。サイズも位置もビットの個数なので、
取り出される整数がどの幅でも `int`。

**整数が第 1 引数——CL と順番が違う。** CL は `(ldb bytespec integer)` と書くが、この言語は
受け手（第 1 引数）の型でメソッドを選ぶので、指定子を先に書くと整数の型ごとに選び分けられない。
他のビット演算はすべて `(op integer ...)` の形（`(logand a b)`・`(ash x count)`・`(lognot x)`）で、
逆だったのは `ldb` 系と `logbitp` だけなので、そちらを揃えた。残りの引数は CL の相対順を保つので、
`(dpb newbyte spec n)` は `(dpb n newbyte spec)` になる。

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | バイト指定子を作る |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | 成分を取り出す |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | `x` から指定バイトを取り出して右詰め |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | 指定バイトに立っているビットがあるか |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | 指定バイト以外を 0 にする（位置は保つ） |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | 右詰めの `newbyte` を `x` の指定バイトへ埋める |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | `dpb` の「位置を保ったまま」版 |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | `op`（10 章の `boole-*` 定数）で選んだ 16 種の2項論理演算 |

`T` は `Bits` トレイトを実装する型、つまり `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`。`boole` だけ
`op` が先頭のまま——CL の並びを変える理由が無いため。

## 12. 乱数

| 名前 | 形式 | 型 | 説明 |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | `0` 以上 `n` 未満の乱数。状態を省略すると `*random-state*` から引いて進める |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | 引数なしなら新しい状態、渡せばその複製（複製は同じ列を再生する） |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | 常に `true`（静的型が既に他の型を排除しているため。CL との対応のためだけに在る） |
| `*random-state*` | — | `random-state` | `random` の既定の状態。代入可能なグローバル（`setf` で差し替える） |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | その整数が名指す状態。同じ種は必ず同じ列を再生する |

生成器は xorshift64 で、解釈実行でもコンパイル済みでも同じ列を返す。

`make-random-state` の新しい状態は壁時計からシードを採るので、実行を跨いで再現はできない。
再現したいときは `seed-random-state` を使う:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; 何回実行しても同じ 3 つの数が出る
```

**CL には移植可能なシード指定が無い**（`make-random-state` が取るのは `nil`/`t`/状態だけ）ので、
これは CL の名前ではなく SBCL の `sb-ext:seed-random-state` に倣った名前。

異なる種は異なる列を生む。`(seed-random-state 0)` と `(seed-random-state 1)` も、`-7` と `7` も
別の列になる。
