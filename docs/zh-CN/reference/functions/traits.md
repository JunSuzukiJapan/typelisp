<!-- translated-from: docs/ja/reference/functions/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 标准 trait

用于迭代、比较和算术的 trait。其他标准 trait 在各自的章节中：`Hash`（[HashTable](collections.md#4-hashtablekv)）、
`Error`（[错误类型](option-result.md#3-错误类型与-error-trait)）、`print-object`
（[打印](printing.md#5-print-object按类型的打印表示)）、流的 trait 群与 `Pathish`（[流与文件](streams-files.md)）。
哪些类型实现了哪些 trait 见[类型一览](../types.md)。trait 的定义方法见[语法参考](../syntax.md#39-deftrait--impl--trait-机制)。

## 1. `Iter` trait 与迭代

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` 分别通过 `vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>` 实现
`Iter`（用 `(iter 集合)` 取得迭代器）。`Chan<T>` 自身就是 `Iter`（`recv` 相当于 `next`。[通道](concurrency.md#2-chant--通道)）。
`Sexpr` 列表没有实现 `Iter`（元素类型不统一）。为自己的类型实现 `Iter` 后，就可以直接用 `doiter` 遍历，也可以传给
[序列函数](sequences.md#4-iter-上的序列函数)。

## 2. `Eq` / `Ord`（比较）

相当于 Rust 的 `PartialEq`/`PartialOrd`（名称为 `Eq`/`Ord`）。在泛型函数的 `where` 约束中用来要求元素类型可比较
（`sort`/`member`/`assoc` 等）。

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; 必须实现
  (not-equals ((self Self) (other Self)) bool             ; 默认实现
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; 继承 Eq
  (less ((self Self) (other Self)) bool)                  ; 必须实现
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

实现 `Eq` 只需写 `equals`，实现 `Ord` 只需写 `less`。其余由默认实现补全。`Ord` 继承 `Eq`，所以 `impl Ord X` 之前
需要 `impl Eq X`。

各 trait 方法可以直接作为函数调用（在 `where (Eq A)`/`(Ord A)` 约束内，或对已实现的具体类型）：

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | 是否相等（Rust 的 `==`） |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | 是否不相等（`!=`） |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

已实现 `Eq` 的：所有数值类型（`i8`〜`u32` / `f32` / `f64` / `int` / `ratio`）以及 `bool` `char` `string` `symbol`
`complex`、`Sexpr`（`eq`，即同一性。供 `match` 的值模式使用），以及 `cons-cell<A,B>`（元素为 `Eq` 时递归比较）。
已实现 `Ord` 的：所有数值类型以及 `char` `string`，以及 `cons-cell<A,B>`（字典序，元素为 `Ord` 时）。

方法名之所以不与内置运算符（`= /= < <= > >=`）或 `eq`/`lt` 重合，是因为内置的不能重定义，各实现会委托给它们。标量的
比较运算符本身是各接收者类型的内置方法（[数值](numbers.md)、[字符串与字符](collections.md)）。在约束内写运算符，会被
解读为 trait 方法（第 3 章）。

## 3. 算术 trait（`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`）

供泛型代码要求"可以相加的类型"的一层。**具体类型的运算使用内置运算符**（[数值](numbers.md)），不经过这一层。

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
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; 距离总是 int（与 ash 相同）
(deftrait Number (Add Sub Mul Div Rem Ord))               ; 没有方法，6 个的组合
```

**在约束内可以用运算符书写。** 当接收者是由 `where` 约束的类型变量时，运算符被解读为 trait 方法（`+`→`add`、
`-`→`sub`、`*`→`mul`、`/`→`div`、`rem`→`remainder`、`logand`→`bit-and`、`=`→`equals`、`<`→`less` …）：

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

trait 一侧的方法名不是 `+`，是因为 `+` 是内置方法名，`impl` 会拒绝重定义（`cannot redefine built-in method`）。没有
`Neg`——`(- x)` 展开为 `(- (- x x) x)`，所以只需 `Sub`。

已实现：`Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` 用于所有数值类型（除 `complex` 外），`Bits` 用于所有整数类型和 `int`。
