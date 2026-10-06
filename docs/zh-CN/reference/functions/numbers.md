<!-- translated-from: docs/ja/reference/functions/numbers.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 数值

整数、浮点数、有理数、复数、布尔值的运算以及与数值相关的函数。调用形式的读法见[内置函数](README.md)。

## 1. 固定宽度整数

整数类型有 7 种：**`int`**（CL 的 `integer`——任意精度，未注解整数字面量的默认类型；第 3 章），以及固定宽度的
`i8` `i16` `i32` `u8` `u16` `u32`。运算针对哪种类型解析由第一个参数的类型决定（它们彼此独立，没有隐式转换）。
**没有 64 位整数类型**——运行时的值是低位为标签的一个字，立即值整数只剩 63 位，自称 64 位的类型必然会在某处丢掉最高位。
`int` 超过这 63 位就变成多精度，所以不在意宽度时请使用 `int`。下表是 6 种固定宽度类型的（`int` 的表在第 3 章）。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | 四则运算。`/` 向零截断，除以零时 panic |
| `mod` | `(mod a b)` | `(T,T)→T` | 余数（CL 的 `mod`，**向下取整除法**：符号随除数。`(mod -7 3)`→`2`）。除以零时 panic |
| `rem` | `(rem a b)` | `(T,T)→T` | 余数（CL 的 `rem`，**截断除法**：符号随被除数。`(rem -7 3)`→`-1`）。除以零时 panic |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | 相当于 CL 的双参数 `floor`/`ceiling`/`round`/`truncate`（`(floor 7 2)`→商 3、余数 1）。用 `cons-cell`（`car`=商、`cdr`=余数）代替多值返回商和余数。`round-div` 遵循 CL 把平局舍入到偶数 |
| `abs` | `(abs x)` | `T→T` | 绝对值 |
| `signum` | `(signum x)` | `T→T` | 符号（`1`/`-1`/`0`） |
| `gcd` | `(gcd a b)` | `(T,T)→T` | 最大公约数 |
| `lcm` | `(lcm a b)` | `(T,T)→T` | 最小公倍数（任一为 0 时为 0） |
| `max` `min` | `(op a b)` | `(T,T)→T` | 较大者／较小者（三个以上参数由第 8 章的可变参数语法糖展开） |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | 比较 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | 都与 `=` 相同（同类型的数值之间没有区别） |
| `int->float` | `(int->float x)` | `T→f64` | 扩展转换为 `f64` |
| `int->int` | `(int->int x)` | `T→int` | 扩展转换为 `int`（总是精确的）。`(as int x)` 的实质 |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | 扩展转换为 `ratio`（总是精确的） |
| `int->char` | `(int->char x)` | `T→char` | 解释为 Unicode 标量值。非法值时 panic |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | 用 `None` 返回 `int->char` 失败的版本 |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | 宽度转换。放不下的值被截断（与 Rust 的 `as` 相同） |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | 把同样的转换作为询问。值放不进该宽度时为 `None` |

这些转换也是特殊形式 `(as Type x)`/`(try-as Type x)`（[语法参考](../syntax.md#7-其他特殊形式)）的实质。位运算
（`logand`/`ash`/`ldb` 等）和谓词（`zerop`/`evenp` 等）在各类型间形式相同，所以归到第 11 章和第 9 章。

`i8` `i16` `u8` `u16` `u32` 原样拥有本章的表，`f32` 原样拥有第 4 章 `f64` 的表。

**类型名就是宽度和符号本身**——`i32` 的意思只是"把 32 位当作有符号处理"，`u32` 只是"把 32 位当作无符号处理"。
`(+ (the u8 200) (the u8 100))` 是 `44`，`(+ 2147483647 1)`（`i32`）是 `-2147483648`，`(lognot (the u32 0))`
是 `4294967295`。`f32` 也一样，是真正的 binary32——`(/ (the f32 1.0) (the f32 3.0))` 打印为 `0.33333334`，
与 `f64` 的 `0.3333333333333333` 是不同的值。

CL 的派生目录（`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` 以及第 9 章的谓词）在 `int`/`i32`/`f64`/`ratio` 上
提供。其他宽度需要时，用 `(as int x)` / `(as i32 x)` 转过去（所有组合都有宽度转换）。

## 2. C 边界上的原始字（`ptr` / `c-long` / `c-ulong`）

只用于与 [`defffi`](../syntax.md#33-defffi--声明-c-函数ffi) 声明的 C 函数交换值的 3 种类型。`ptr` 是不透明指针，
`c-long` / `c-ulong` 是 C 的 `long` / `unsigned long`。要成为值必须处于 `(unsafe ...)` 之中。

**没有算术。** 第 1 章的表一项也不适用——`(+ p 1)` 和 `(< n m)` 都不能写。它们是交给 C 的字，不是用来计算的类型，
要计算请转到有宽度的类型。`c-long` / `c-ulong` 上只有转换：

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | 与第 1 章相同的宽度转换。放不下的值被截断 |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | 把同样的转换作为询问 |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | 从原始字之间以及第 1 章的整数类型创建的入口 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | 同上 |
| `int->int` | `(int->int x)` | `T→int` | **总是精确的**。读取放不进 `i32` 的 `size_t` 的正当方式 |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` 就是它们，与第 1 章所有整数类型之间都有转换。
`ptr` 连这张表都没有——没有提供把指针当作数读取的途径。它只是被传递、接收、再交给另一个 C 函数的值。

**也不能打印。** `(println "~a" x)` 不接受原始字（它没有 `Sexpr` 表示），请像 `(println "~a" (as int n))` 这样
先转到有宽度的类型。

第 1 章开头的"没有 64 位整数类型"对这 3 种也成立。它之所以成立，是因为**它们无法保存**：不能放进 `defstruct` 的字段、
`defvar`、类型参数内部或 `Sexpr` 中，只是作为参数、返回值和局部变量穿过函数的字。详情见
[语法参考](../syntax.md#ptr--c-long--c-ulong--原始机器字)。

## 3. 任意精度整数 `int`

CL 的 `integer`，也就是这门语言的**整数**——未注解的整数字面量是这个类型，`length`、`char->int` 等返回数的内置函数
也返回这个类型。值在放得进 63 位立即值（fixnum）时是立即值，运算结果放不下时自动提升为多精度，放得下时又回到立即值。
`eq` 在 fixnum 范围内总是值的同一性，`eql`/`=` 在整个范围内是数值的同一性。它与固定宽度整数类型（第 1 章）是不同的
类型，没有隐式转换——`(as int x)` 是从固定宽度的精确扩展，`(as i32 n)` / `(try-as i32 n)` 是从 `int` 的截断／判定
（与第 1 章的 `int->W` / `try-int->W` 含义相同）。

`Sexpr` 的整数变体也只有 `int` 一种（`(int n)` 同时接受 fixnum 和多精度）。

接受下标或个数的内置函数（`substring`、`Vector` 的 `get`、`ash` 的位数等）接受 `int`，但传入放不进 fixnum 的值会
产生运行时错误（"an integer argument does not fit a fixnum"）。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | 不会溢出（会提升） |
| `/` | `(/ a b)` | `(int,int)→int` | 向零截断。除以零时 panic |
| `mod` | `(mod a b)` | `(int,int)→int` | 向下取整除法的余数（符号随除数） |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | 都是 `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | 与第 11 章相同（无限位的二进制补码） |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | 与第 1 章相同 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | 截断／判定。`W` 是 6 种宽度以及 `c-long`/`c-ulong` |
| `int->int` | | `int→int` | 恒等（固定宽度和 C 字一侧的 `int->int` 是扩展。第 1 章） |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | 与第 1 章形式相同。`expt` 只接受非负指数 |

## 4. 浮点数（`f64` / `f32`）

`f32` 也有同样的表。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754。除以零不会 panic，而是 `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | 向下取整除法的余数（遵循 CL，符号随除数。`a - b*floor(a/b)`） |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | 截断除法的余数（遵循 CL，符号随被除数。`a - b*truncate(a/b)`） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | 比较 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | 都与 `=` 相同 |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | 幂 |
| `abs` | `(abs x)` | `f64→f64` | 绝对值 |
| `signum` | `(signum x)` | `f64→f64` | 符号（`1.0`/`-1.0`，`±0.0`/`NaN` 原样返回。遵循 CL，与 Rust 的 `signum` 不同） |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | 较大者／较小者（三个以上参数由第 8 章的可变参数语法糖展开） |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | 一元运算 |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | 超越函数。`log` 是自然对数 |
| `log`（双参数） | `(log x base)` | `(f64,f64)→f64` | 指定底数的对数。展开为 `(/ (log x) (log base))`（第 8 章） |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | 相当于 CL 的双参数版本（`(floor 7.0 2.0)`→商 3、余数 1）。与第 1 章同名函数的设计相同（`car`=商、`cdr`=余数） |
| `float->int` | `(float->int x)` | `f64→int` | 向零截断转换为 `int`（CL 的 `truncate`；任何大小的有限值都是精确的）。无穷大和 NaN 时 panic。需要固定宽度时用 `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | 作为精确的二进制有理数转换为 `ratio`（CL 的 `rational`） |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | 浮点宽度的转换。`float->f32` 舍入到最近，`float->f64` 总是精确的。`(as f32 x)` 的实质 |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | 把同样的转换作为询问。舍入会改变值时为 `none`（扩展到 `f64` 总是 `some`）。`(try-as f32 x)` 的实质 |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | CL 的同名函数。上面 `floor`/`ceiling`/`round`/`truncate` 的别名——在 CL 中不带前缀的返回整数，所以带 `f` 的才符合本语言的行为 |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | 分别为 2 / 53 / 53（只有 `0.0` 的 precision 是 0）。`f64` 总是 IEEE-754 binary64，所以是常量 |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` 或 `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | 尾数（在 `[1/2,1)` 中，无符号）与指数。CL 返回 3 个值，但没有多值，所以符号交给 `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | 用精确的 53 位整数尾数做同样的分解。`尾数 * 2^指数` 恰好是原值 |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **读回时恰为该浮点数的最简单的**有理数（`(rationalize 0.1)` 是 `1/10`）。需要精确的二进制值时用 `float->ratio` |

**与 CL 的区别：`round` 的舍入方式。** `round`（因而还有 `fround`/`round-div`）**远离零**舍入（`(round 2.5)` =
`3.0`）。CL **舍入到偶数**，得到 `2`。

## 5. 有理数 `ratio`

遵循 CL 的任意精度有理数。始终保持既约且分母为正，在堆上分配。与整数类型和 `f64` 之间没有隐式转换（请使用显式的转换
方法或 `as`/`try-as`）。比例字面量的语法见[语法参考](../syntax.md#1-词法元素)。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | 四则运算（结果总是既约的）。`/` 除以零时 panic |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | 向下取整除法的余数（遵循 CL，符号随除数） |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | 截断除法的余数（遵循 CL，符号随被除数） |
| `abs` | `(abs x)` | `ratio→ratio` | 绝对值 |
| `signum` | `(signum x)` | `ratio→ratio` | 符号（以 `ratio` 返回 `1`/`-1`/`0`） |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | 幂。指数只能是整数值的 `ratio`（否则 panic）。负指数得到倒数 |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | 较大者／较小者 |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`。`ratio` 没有位运算（CL 中位运算也只用于整数） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | 比较 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | 都与 `=` 相同 |
| `numerator` | `(numerator x)` | `ratio→int` | 既约分子（与 CL 同名） |
| `denominator` | `(denominator x)` | `ratio→int` | 既约分母（总是正数） |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | 整数部分（向零截断） |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | 转换为 `f64` |

从固定宽度整数和 `f64` 进入的途径是 `int->int`/`int->ratio`（第 1 章）和 `float->int`/`float->ratio`（第 4 章）。
`int`/`ratio` 是与 `i32` 等相互独立的类型，混合运算需要显式转换。

## 6. 复数 `complex`

标准库中的结构体（`defstruct`）。

**与 CL 的两点区别**（都是静态类型的结果）：

1. **分量固定为 `f64`。** CL 的 complex 也可以持有有理数，`(complex 1 2)` 和 `(complex 1.0 2.0)` 是不同的类型。
   静态类型必须选择其一，而超越函数返回的是浮点的那一种。
2. **`(sqrt -1.0)` 是实数的 `sqrt`（NaN）。** CL 的 `sqrt` 可以从实数返回复数，但 `f64` 的 `sqrt` 必须返回 `f64`。
   复数结果来自复数参数——`(sqrt (complex -1.0 0.0))` 是 `i`。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | 创建。分量可以用 `z::re`/`z::im` 直接读取 |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | 实部、虚部。**也作用于实数**（`(realpart 3.0)`→`3.0`、`(imagpart 3.0)`→`0.0`）。与 CL 相同 |
| `conjugate` | `(conjugate z)` | `complex→complex` | 共轭（也作用于实数） |
| `phase` | `(phase z)` | `complex→f64` | 辐角 (-pi,pi]（也作用于实数） |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | 绝对值。**唯一不返回接收者类型的 `abs`**（与 CL 一样，复数的绝对值是实数） |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | 复数的四则运算 |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | 按分量判断相等。也实现了 `Eq`（没有 `Ord`——复数没有顺序，CL 的 `<` 也拒绝） |
| `zerop` | `(zerop z)` | `complex→bool` | 两个分量是否都为 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` 取主值 |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`。`(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | 向量 `(x,y)` 的角度。**CL 的双参数 `(atan y x)` 是它的语法糖**（与双参数 `log` 一样按参数个数分支） |

它实现了 `print-object`，所以 `~a`/`~s` 与 CL 一样打印为 `#C(re im)`（本语言的读取器没有读回 `#C` 的语法）。

## 7. 布尔值

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | 取反 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | 都是值的相等比较 |

`and`/`or` 需要短路求值，所以是特殊形式（[语法参考](../syntax.md#4-绑定与条件分支)）。

## 8. 数值辅助与调用语法糖

`abs`/`signum`（所有数值类型）、`gcd`/`lcm`（仅整数类型）、`rem`（包括 `f64` 在内的所有实数类型）、`expt`
（`int`/`f64`/`ratio`）都被定义为各数值类型的方法（按接收者的类型解析。`(abs x)` 是对应 `x` 的类型的方法）。各类型
的详情见第 1、3、4、5 章。固定宽度整数没有 `expt`（没有提升会溢出，请用 `(as int x)` 转到 `int` 再用它的 `expt`）。

### 8.1 可变参数与 0/1 参数形式

CL 的算术和比较是可变参数的，但方法只按接收者的类型解析，不按参数个数解析。因此下列形式**由检查器展开为双参数调用**。

| 可以写的形式 | 展开 | 对象 |
|---|---|---|
| `(op a b c ...)` | 左折叠 `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | 把各项绑定到临时变量的 `(and (cmp a b) (cmp b c) ...)` | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | 上面有单位元的那些 |
| `(op x)` | `+ * max min logand logior logxor` 为 `x` 本身。`(- x)` 取负，`(/ x)` 取倒数，`(gcd x)`/`(lcm x)` 为 `(abs x)`（遵循 CL） | 同上 |
| `(cmp x)` | 求值 `x` 后为 `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

各项从左到右只求值一次（比较的可变参数版本经过临时变量就是这个原因）。`/=` 的可变参数形式比较的是**相邻的对**，与询问
所有对是否互不相同的 CL 不同。

### 8.2 `isqrt` 与整数的 `expt`

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | 不超过平方根的最大整数。负数时 panic |
| `expt` | `(expt n e)` | `(T,T)→T` | 幂（平方乘法）。CL 对负指数返回有理数，但整数类型无法表示，所以 panic——请先转换为 `ratio` |

## 9. 谓词

| 名称 | 形式 | 类型 | 对应类型 |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32`（与 CL 一样只用于整数类型） |

**没有** CL 的 `numberp`/`integerp`/`floatp` 等**类型谓词**——由于是静态类型，值的类型不必在运行时询问就已确定。

## 10. 常量

| 名称 | 类型 | 值 |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | 传给 `boole` 的运算代码（代替 CL 的关键字） |

数值界限常量（CLHS 12.1.4.2 / 12.1.3）：

| 名称 | 类型 | 说明 |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | 63 位立即值的上限／下限（2^62-1 / -2^62）。超过它们的 `int` 是多精度 |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | 有限值中的最大／最小 |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | 包括非规格化数在内的非零最小绝对值 |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | 仅限规格化数的同一量 |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | 遵循 CL 的定义（满足 `(/= (+ 1 e) 1)` 的最小正数 `e`），所以**比 2^-53 大 1 ULP**——2^-53 本身在舍入到最近偶数下会回到 `1.0` |

## 11. 位运算

定义为无限精度的二进制补码（CL 12.10）。固定宽度整数类型和 `int` 有实现，`ratio` 没有（CL 本身的位运算也只用于整数）。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | 按位与、或、异或（可变参数和 0 参数版本见 8.1） |
| `lognot` | `(lognot x)` | `T→T` | 按位取反 |
| `ash` | `(ash x count)` | `(T,int)→T` | 算术移位。`count` 为正则左移，为负则右移 |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | 第 `index` 位是否为 1（**参数顺序与 CL 相反**，见下文） |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | 为 1 的位数（负数时为 0 的位数） |
| `integer-length` | `(integer-length x)` | `T→T` | 不计符号时表示所需的位数 |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | 由上面合成的其余 7 种 |

**只有 `ash` 的第 2 个参数是 `int` 而不是 `T`。** 它是以位计的**距离**，不是接收者类型的值，所以接收者的宽度和符号与
距离无关（与 CL 的 `(ash integer count)` 中 `count` 是任意整数的理由相同）。无符号数右移是逻辑移位
（`(ash (the u8 200) -3)` = `25`），有符号数是向负无穷方向舍入的算术移位（`(ash (the i32 -100) -4)` = `-7`）。
`logbitp` 的 `index` 出于同样理由也是 `int`。

**字节说明符。** 用 `cons-cell<int,int>`（`car`=大小、`cdr`=位置）代替 CL 的 `byte` 返回的不透明对象。大小和位置都是
位的个数，所以无论被取出的整数是什么宽度，都是 `int`。

**整数是第 1 个参数——顺序与 CL 不同。** CL 写成 `(ldb bytespec integer)`，但本语言按接收者（第 1 个参数）的类型选择
方法，如果说明符在前，就无法按整数的类型来选择。其他位运算都是 `(op integer ...)` 的形式（`(logand a b)`、
`(ash x count)`、`(lognot x)`），相反的只有 `ldb` 系列和 `logbitp`，所以把它们统一了。其余参数保持 CL 的相对顺序，
因此 `(dpb newbyte spec n)` 变成 `(dpb n newbyte spec)`。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | 创建字节说明符 |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | 取出分量 |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | 从 `x` 取出指定字节并右对齐 |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | 指定字节中是否有为 1 的位 |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | 把指定字节以外置 0（保持位置） |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | 把右对齐的 `newbyte` 嵌入 `x` 的指定字节 |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | `dpb` 的"保持位置"版本 |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | 由 `op`（第 10 章的 `boole-*` 常量）选择的 16 种二元逻辑运算之一 |

`T` 是实现了 `Bits` trait 的类型，即 `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`。只有 `boole` 保持 `op` 在最前——
没有理由改变 CL 的顺序。

## 12. 随机数

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | `0` 以上、小于 `n` 的随机数。省略状态时从 `*random-state*` 中取并推进它 |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | 没有参数时是新状态，传入时是其副本（副本重现相同的序列） |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | 总是 `true`（静态类型已经排除了其他类型。只是为了与 CL 对应而存在） |
| `*random-state*` | — | `random-state` | `random` 的默认状态。可以赋值的全局变量（用 `setf` 替换） |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | 该整数所指定的状态。相同的种子一定重现相同的序列 |

生成器是 xorshift64，解释执行和编译执行都返回相同的序列。

`make-random-state` 的新状态以挂钟时间作为种子，所以无法跨运行重现。需要重现时使用 `seed-random-state`：

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; 无论运行多少次都输出同样的 3 个数
```

**CL 没有可移植的指定种子的方法**（`make-random-state` 只接受 `nil`/`t`/状态），所以这个名字不是 CL 的，而是仿照了
SBCL 的 `sb-ext:seed-random-state`。

不同的种子生成不同的序列。`(seed-random-state 0)` 和 `(seed-random-state 1)` 不同，`-7` 和 `7` 也是不同的序列。
