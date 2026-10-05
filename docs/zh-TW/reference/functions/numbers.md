<!-- translated-from: docs/ja/reference/functions/numbers.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 數值

整數、浮點數、有理數、複數、布林值的運算以及與數值相關的函式。呼叫形式的讀法見[內建函式](README.md)。

## 1. 固定寬度整數

整數型別有 7 種：**`int`**（CL 的 `integer`——任意精度，未註記整數字面值的預設型別；第 3 章），以及固定寬度的
`i8` `i16` `i32` `u8` `u16` `u32`。運算針對哪種型別解析由第一個引數的型別決定（它們彼此獨立，沒有隱式轉換）。
**沒有 64 位元整數型別**——執行期的值是低位元為標籤的一個字，立即值整數只剩 63 位元，自稱 64 位元的型別勢必會在某處丟掉最高位元。
`int` 超過這 63 位元就變成多倍精度，所以不在意寬度時請使用 `int`。下表是 6 種固定寬度型別的（`int` 的表在第 3 章）。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | 四則運算。`/` 朝零截斷，除以零時 panic |
| `mod` | `(mod a b)` | `(T,T)→T` | 餘數（CL 的 `mod`，**下取整除法**：正負號隨除數。`(mod -7 3)`→`2`）。除以零時 panic |
| `rem` | `(rem a b)` | `(T,T)→T` | 餘數（CL 的 `rem`，**截斷除法**：正負號隨被除數。`(rem -7 3)`→`-1`）。除以零時 panic |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | 相當於 CL 的雙引數 `floor`/`ceiling`/`round`/`truncate`（`(floor 7 2)`→商 3、餘數 1）。以 `cons-cell`（`car`=商、`cdr`=餘數）取代多值回傳商與餘數。`round-div` 依 CL 把平手捨入到偶數 |
| `abs` | `(abs x)` | `T→T` | 絕對值 |
| `signum` | `(signum x)` | `T→T` | 正負號（`1`/`-1`/`0`） |
| `gcd` | `(gcd a b)` | `(T,T)→T` | 最大公因數 |
| `lcm` | `(lcm a b)` | `(T,T)→T` | 最小公倍數（任一為 0 時為 0） |
| `max` `min` | `(op a b)` | `(T,T)→T` | 較大者／較小者（三個以上引數由第 8 章的可變引數語法糖展開） |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | 都與 `=` 相同（同型別的數值之間沒有差別） |
| `int->float` | `(int->float x)` | `T→f64` | 擴大轉換為 `f64` |
| `int->int` | `(int->int x)` | `T→int` | 擴大轉換為 `int`（一律精確）。`(as int x)` 的實質 |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | 擴大轉換為 `ratio`（一律精確） |
| `int->char` | `(int->char x)` | `T→char` | 解讀為 Unicode 純量值。不合法的值時 panic |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | 以 `None` 回傳 `int->char` 失敗的版本 |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | 寬度轉換。放不下的值會被截斷（與 Rust 的 `as` 相同） |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | 把同樣的轉換當作詢問。值放不進該寬度時為 `None` |

這些轉換也是特殊形式 `(as Type x)`/`(try-as Type x)`（[語法參考](../syntax.md#7-其他特殊形式)）的實質。位元運算
（`logand`/`ash`/`ldb` 等）與述詞（`zerop`/`evenp` 等）在各型別間形式相同，所以歸到第 11 章與第 9 章。

`i8` `i16` `u8` `u16` `u32` 原樣擁有本章的表，`f32` 原樣擁有第 4 章 `f64` 的表。

**型別名稱就是寬度與正負號本身**——`i32` 的意思只是「把 32 位元當作有號處理」，`u32` 只是「把 32 位元當作無號處理」。
`(+ (the u8 200) (the u8 100))` 是 `44`，`(+ 2147483647 1)`（`i32`）是 `-2147483648`，`(lognot (the u32 0))` 是 `4294967295`。
`f32` 也一樣，是真正的 binary32——`(/ (the f32 1.0) (the f32 3.0))` 列印為 `0.33333334`，與 `f64` 的 `0.3333333333333333` 是不同的值。

CL 的衍生目錄（`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` 以及第 9 章的述詞）提供於 `int`/`i32`/`f64`/`ratio`。其他寬度需要時，以
`(as int x)` / `(as i32 x)` 轉過去（所有組合都有寬度轉換）。

## 2. C 邊界上的原始字（`ptr` / `c-long` / `c-ulong`）

只用於與以 [`defffi`](../syntax.md#33-defffi--宣告-c-函式ffi) 宣告的 C 函式交換值的 3 種型別。`ptr` 是不透明指標，`c-long` /
`c-ulong` 是 C 的 `long` / `unsigned long`。要成為值必須身處 `(unsafe ...)` 之中。

**沒有算術。** 第 1 章的表一項也不適用——`(+ p 1)` 與 `(< n m)` 都不能寫。它們是交給 C 的字，不是用來計算的型別，要計算請轉到有寬度
的型別。`c-long` / `c-ulong` 上只有轉換：

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | 與第 1 章相同的寬度轉換。放不下的值會被截斷 |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | 把同樣的轉換當作詢問 |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | 從原始字之間以及第 1 章的整數型別建立的入口 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | 同上 |
| `int->int` | `(int->int x)` | `T→int` | **一律精確**。讀取放不進 `i32` 的 `size_t` 的正當方式 |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` 就是它們，與第 1 章的所有整數型別之間都有轉換。`ptr` 連這張表都
沒有——沒有提供把指標當作數讀取的途徑。它只是被傳遞、接收、再交給另一個 C 函式的值。

**也不能列印。** `(println "~a" x)` 不接受原始字（它沒有 `Sexpr` 表示），請像 `(println "~a" (as int n))` 這樣先轉到有寬度的型別。

第 1 章開頭的「沒有 64 位元整數型別」對這 3 種也成立。它之所以成立，是因為**它們無法保存**：不能放進 `defstruct` 的欄位、`defvar`、
型別引數內部或 `Sexpr` 中，只是作為引數、回傳值與區域變數穿過函式的字。詳情見[語法參考](../syntax.md#ptr--c-long--c-ulong--原始機器字)。

## 3. 任意精度整數 `int`

CL 的 `integer`，也就是這個語言的**整數**——未註記的整數字面值是這個型別，`length`、`char->int` 等回傳數的內建函式也回傳這個型別。
值在放得進 63 位元立即值（fixnum）時是立即值，運算結果放不下時自動提升為多倍精度，放得下時又回到立即值。`eq` 在 fixnum 範圍內一律是值
的同一性，`eql`/`=` 在整個範圍內是數值的同一性。它與固定寬度整數型別（第 1 章）是不同的型別，沒有隱式轉換——`(as int x)` 是從固定
寬度的精確擴大，`(as i32 n)` / `(try-as i32 n)` 是從 `int` 的截斷／判定（與第 1 章的 `int->W` / `try-int->W` 意義相同）。

`Sexpr` 的整數變體也只有 `int` 一種（`(int n)` 同時接受 fixnum 與多倍精度）。

接受索引或個數的內建函式（`substring`、`Vector` 的 `get`、`ash` 的位數等）接受 `int`，但傳入放不進 fixnum 的值會產生執行期錯誤
（「an integer argument does not fit a fixnum」）。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | 不會溢位（會提升） |
| `/` | `(/ a b)` | `(int,int)→int` | 朝零截斷。除以零時 panic |
| `mod` | `(mod a b)` | `(int,int)→int` | 下取整除法的餘數（正負號隨除數） |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | 都是 `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | 與第 11 章相同（無限位元的二補數） |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | 與第 1 章相同 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | 截斷／判定。`W` 是 6 種寬度以及 `c-long`/`c-ulong` |
| `int->int` | | `int→int` | 恆等（固定寬度與 C 字端的 `int->int` 是擴大。第 1 章） |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | 與第 1 章形式相同。`expt` 只接受非負指數 |

## 4. 浮點數（`f64` / `f32`）

`f32` 也有同樣的表。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754。除以零不會 panic，而是 `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | 下取整除法的餘數（依 CL，正負號隨除數。`a - b*floor(a/b)`） |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | 截斷除法的餘數（依 CL，正負號隨被除數。`a - b*truncate(a/b)`） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | 都與 `=` 相同 |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | 冪 |
| `abs` | `(abs x)` | `f64→f64` | 絕對值 |
| `signum` | `(signum x)` | `f64→f64` | 正負號（`1.0`/`-1.0`，`±0.0`/`NaN` 原樣回傳。依 CL，與 Rust 的 `signum` 不同） |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | 較大者／較小者（三個以上引數由第 8 章的可變引數語法糖展開） |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | 一元運算 |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | 超越函式。`log` 是自然對數 |
| `log`（雙引數） | `(log x base)` | `(f64,f64)→f64` | 指定底數的對數。展開為 `(/ (log x) (log base))`（第 8 章） |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | 相當於 CL 的雙引數版本（`(floor 7.0 2.0)`→商 3、餘數 1）。與第 1 章同名函式的設計相同（`car`=商、`cdr`=餘數） |
| `float->int` | `(float->int x)` | `f64→int` | 朝零截斷轉換為 `int`（CL 的 `truncate`；任何大小的有限值都是精確的）。無限大與 NaN 時 panic。需要固定寬度時用 `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | 以精確的二進位有理數轉換為 `ratio`（CL 的 `rational`） |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | 浮點寬度的轉換。`float->f32` 捨入到最近，`float->f64` 一律精確。`(as f32 x)` 的實質 |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | 把同樣的轉換當作詢問。捨入會改變值時為 `none`（擴大到 `f64` 一律為 `some`）。`(try-as f32 x)` 的實質 |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | CL 的同名函式。上面 `floor`/`ceiling`/`round`/`truncate` 的別名——在 CL 中不帶前綴的回傳整數，所以帶 `f` 的才符合這個語言的行為 |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | 分別為 2 / 53 / 53（只有 `0.0` 的 precision 是 0）。`f64` 一律是 IEEE-754 binary64，所以是常數 |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` 或 `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | 尾數（在 `[1/2,1)` 中，無正負號）與指數。CL 回傳 3 個值，但沒有多值，所以正負號交給 `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | 以精確的 53 位元整數尾數做同樣的分解。`尾數 * 2^指數` 恰好是原值 |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **讀回時恰為該浮點數的最簡單的**有理數（`(rationalize 0.1)` 是 `1/10`）。需要精確的二進位值時用 `float->ratio` |

**與 CL 的差異：`round` 的捨入方式。** `round`（因而還有 `fround`/`round-div`）**遠離零**捨入（`(round 2.5)` = `3.0`）。CL **捨入到
偶數**，得到 `2`。

## 5. 有理數 `ratio`

依 CL 的任意精度有理數。一律保持既約且分母為正，在堆積上配置。與整數型別及 `f64` 之間沒有隱式轉換（請使用明確的轉換方法或
`as`/`try-as`）。比例字面值的語法見[語法參考](../syntax.md#1-詞法元素)。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | 四則運算（結果一律既約）。`/` 除以零時 panic |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | 下取整除法的餘數（依 CL，正負號隨除數） |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | 截斷除法的餘數（依 CL，正負號隨被除數） |
| `abs` | `(abs x)` | `ratio→ratio` | 絕對值 |
| `signum` | `(signum x)` | `ratio→ratio` | 正負號（以 `ratio` 回傳 `1`/`-1`/`0`） |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | 冪。指數只能是整數值的 `ratio`（否則 panic）。負指數得到倒數 |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | 較大者／較小者 |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`。`ratio` 沒有位元運算（CL 中位元運算也只用於整數） |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | 比較 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | 都與 `=` 相同 |
| `numerator` | `(numerator x)` | `ratio→int` | 既約分子（與 CL 同名） |
| `denominator` | `(denominator x)` | `ratio→int` | 既約分母（一律為正） |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | 整數部分（朝零截斷） |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | 轉換為 `f64` |

從固定寬度整數與 `f64` 進入的途徑是 `int->int`/`int->ratio`（第 1 章）與 `float->int`/`float->ratio`（第 4 章）。`int`/`ratio` 是與
`i32` 等彼此獨立的型別，混合運算需要明確轉換。

## 6. 複數 `complex`

標準函式庫中的結構（`defstruct`）。

**與 CL 的兩點差異**（都是靜態型別的結果）：

1. **分量固定為 `f64`。** CL 的 complex 也可以持有有理數，`(complex 1 2)` 與 `(complex 1.0 2.0)` 是不同的型別。靜態型別必須選擇其一，
   而超越函式回傳的是浮點的那一種。
2. **`(sqrt -1.0)` 是實數的 `sqrt`（NaN）。** CL 的 `sqrt` 可以從實數回傳複數，但 `f64` 的 `sqrt` 必須回傳 `f64`。複數結果來自複數
   引數——`(sqrt (complex -1.0 0.0))` 是 `i`。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | 建立。分量可以用 `z::re`/`z::im` 直接讀取 |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | 實部、虛部。**也作用於實數**（`(realpart 3.0)`→`3.0`、`(imagpart 3.0)`→`0.0`）。與 CL 相同 |
| `conjugate` | `(conjugate z)` | `complex→complex` | 共軛（也作用於實數） |
| `phase` | `(phase z)` | `complex→f64` | 輻角 (-pi,pi]（也作用於實數） |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | 絕對值。**唯一不回傳接收者型別的 `abs`**（與 CL 一樣，複數的絕對值是實數） |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | 複數的四則運算 |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | 依分量判斷相等。也實作了 `Eq`（沒有 `Ord`——複數沒有順序，CL 的 `<` 也拒絕） |
| `zerop` | `(zerop z)` | `complex→bool` | 兩個分量是否都為 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` 取主值 |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`。`(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | 向量 `(x,y)` 的角度。**CL 的雙引數 `(atan y x)` 是它的語法糖**（與雙引數 `log` 一樣依引數個數分支） |

它實作了 `print-object`，所以 `~a`/`~s` 與 CL 一樣列印為 `#C(re im)`（這個語言的讀取器沒有讀回 `#C` 的語法）。

## 7. 布林值

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | 否定 |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | 都是值的相等比較 |

`and`/`or` 需要短路求值，所以是特殊形式（[語法參考](../syntax.md#4-繫結與條件分支)）。

## 8. 數值輔助與呼叫語法糖

`abs`/`signum`（所有數值型別）、`gcd`/`lcm`（僅整數型別）、`rem`（包括 `f64` 在內的所有實數型別）、`expt`（`int`/`f64`/`ratio`）都被
定義為各數值型別的方法（依接收者的型別解析。`(abs x)` 是對應 `x` 的型別的方法）。各型別的詳情見第 1、3、4、5 章。固定寬度整數沒有
`expt`（沒有提升會溢位，請以 `(as int x)` 轉到 `int` 再使用它的 `expt`）。

### 8.1 可變引數與 0/1 引數形式

CL 的算術與比較是可變引數的，但方法只依接收者的型別解析，不依引數個數解析。因此下列形式**由檢查器展開為雙引數呼叫**。

| 可以寫的形式 | 展開 | 對象 |
|---|---|---|
| `(op a b c ...)` | 左摺疊 `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | 把各項繫結到暫存變數的 `(and (cmp a b) (cmp b c) ...)` | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | 上面有單位元素的那些 |
| `(op x)` | `+ * max min logand logior logxor` 為 `x` 本身。`(- x)` 取負，`(/ x)` 取倒數，`(gcd x)`/`(lcm x)` 為 `(abs x)`（依 CL） | 同上 |
| `(cmp x)` | 求值 `x` 後為 `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

各項由左至右只求值一次（比較的可變引數版本經過暫存變數就是這個原因）。`/=` 的可變引數形式比較的是**相鄰的組**，與詢問所有組是否互不
相同的 CL 不同。

### 8.2 `isqrt` 與整數的 `expt`

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | 不超過平方根的最大整數。負數時 panic |
| `expt` | `(expt n e)` | `(T,T)→T` | 冪（平方乘法）。CL 對負指數回傳有理數，但整數型別無法表示，所以 panic——請先轉換為 `ratio` |

## 9. 述詞

| 名稱 | 形式 | 型別 | 對應型別 |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32`（與 CL 一樣只用於整數型別） |

**沒有** CL 的 `numberp`/`integerp`/`floatp` 等**型別述詞**——因為是靜態型別，值的型別不必在執行期詢問就已決定。

## 10. 常數

| 名稱 | 型別 | 值 |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | 傳給 `boole` 的運算代碼（取代 CL 的關鍵字） |

數值界限常數（CLHS 12.1.4.2 / 12.1.3）：

| 名稱 | 型別 | 說明 |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | 63 位元立即值的上限／下限（2^62-1 / -2^62）。超過它們的 `int` 是多倍精度 |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | 有限值中的最大／最小 |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | 包括非正規化數在內的非零最小絕對值 |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | 僅限正規化數的同一量 |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | 依 CL 的定義（滿足 `(/= (+ 1 e) 1)` 的最小正數 `e`），所以**比 2^-53 大 1 ULP**——2^-53 本身在捨入到最近偶數下會回到 `1.0` |

## 11. 位元運算

定義為無限精度的二補數（CL 12.10）。固定寬度整數型別與 `int` 有實作，`ratio` 沒有（CL 本身的位元運算也只用於整數）。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | 位元 AND、OR、XOR（可變引數與 0 引數版本見 8.1） |
| `lognot` | `(lognot x)` | `T→T` | 位元反轉 |
| `ash` | `(ash x count)` | `(T,int)→T` | 算術位移。`count` 為正則左移，為負則右移 |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | 第 `index` 位元是否為 1（**引數順序與 CL 相反**，見下文） |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | 為 1 的位元數（負數時為 0 的位元數） |
| `integer-length` | `(integer-length x)` | `T→T` | 不計正負號時表示所需的位元數 |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | 由上面合成的其餘 7 種 |

**只有 `ash` 的第 2 個引數是 `int` 而不是 `T`。** 它是以位元計的**距離**，不是接收者型別的值，所以接收者的寬度與正負號與距離無關
（與 CL 的 `(ash integer count)` 中 `count` 是任意整數的理由相同）。無號數右移是邏輯位移（`(ash (the u8 200) -3)` = `25`），有號數是
朝負無限大方向捨入的算術位移（`(ash (the i32 -100) -4)` = `-7`）。`logbitp` 的 `index` 出於同樣理由也是 `int`。

**位元組指定子。** 以 `cons-cell<int,int>`（`car`=大小、`cdr`=位置）取代 CL 的 `byte` 回傳的不透明物件。大小與位置都是位元的個數，所以
無論被取出的整數是什麼寬度，都是 `int`。

**整數是第 1 個引數——順序與 CL 不同。** CL 寫成 `(ldb bytespec integer)`，但這個語言依接收者（第 1 個引數）的型別選擇方法，如果指定子在前，
就無法依整數的型別選擇。其他位元運算都是 `(op integer ...)` 的形式（`(logand a b)`、`(ash x count)`、`(lognot x)`），相反的只有 `ldb`
系列與 `logbitp`，所以把它們統一了。其餘引數保持 CL 的相對順序，因此 `(dpb newbyte spec n)` 變成 `(dpb n newbyte spec)`。

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | 建立位元組指定子 |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | 取出分量 |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | 從 `x` 取出指定的位元組並靠右對齊 |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | 指定的位元組中是否有為 1 的位元 |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | 把指定位元組以外設為 0（保持位置） |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | 把靠右對齊的 `newbyte` 嵌入 `x` 的指定位元組 |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | `dpb` 的「保持位置」版本 |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | 由 `op`（第 10 章的 `boole-*` 常數）選擇的 16 種二元邏輯運算之一 |

`T` 是實作了 `Bits` trait 的型別，即 `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`。只有 `boole` 保持 `op` 在最前——沒有理由改變 CL 的順序。

## 12. 亂數

| 名稱 | 形式 | 型別 | 說明 |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | `0` 以上、小於 `n` 的亂數。省略狀態時從 `*random-state*` 取出並推進它 |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | 沒有引數時是新狀態，傳入時是其副本（副本重現相同的序列） |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | 一律為 `true`（靜態型別已經排除了其他型別。只是為了與 CL 對應而存在） |
| `*random-state*` | — | `random-state` | `random` 的預設狀態。可以指派的全域變數（以 `setf` 替換） |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | 該整數所指定的狀態。相同的種子一定重現相同的序列 |

產生器是 xorshift64，直譯執行與編譯執行都回傳相同的序列。

`make-random-state` 的新狀態以時鐘時間作為種子，所以無法跨執行重現。需要重現時使用 `seed-random-state`：

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; 無論執行幾次都輸出同樣的 3 個數
```

**CL 沒有可移植的指定種子的方法**（`make-random-state` 只接受 `nil`/`t`/狀態），所以這個名稱不是 CL 的，而是仿照 SBCL 的
`sb-ext:seed-random-state`。

不同的種子產生不同的序列。`(seed-random-state 0)` 與 `(seed-random-state 1)` 不同，`-7` 與 `7` 也是不同的序列。
