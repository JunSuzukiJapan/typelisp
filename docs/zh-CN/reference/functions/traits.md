<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# 标准 trait

用于迭代、比较、算术和存取元素的 trait。其他标准 trait 在各自的章节中：`Hash`（[HashTable](collections.md#4-hashtablekv)）、
`Error`（[错误类型](option-result.md#3-错误类型与-error-trait)）、`print-object`
（[打印](printing.md#5-print-object按类型的打印表示)）、流的 trait 群与 `Pathish`（[流与文件](streams-files.md)）。
哪些类型实现了哪些 trait 见[类型一览](../types.md)。trait 的定义方法见[语法参考](../syntax.md#39-deftrait--impl--trait-机制)。

## 1. `Iter` trait 与迭代

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` 分别通过 `vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>` 实现
`Iter`（用 `(iter 集合)` 取得迭代器）。`Chan<T>` 自身就是 `Iter`（`recv` 相当于 `next`。[通道](concurrency.md#2-chant--通道)）。
`Sexpr` 列表没有实现 `Iter`（元素类型不统一）。为自己的类型实现 `Iter` 后，就可以直接用 `doiter` 遍历，也可以传给
[序列函数](sequences.md#4-iter-上的序列函数)。

## 2. `Equal` / `Ord`（比较）

`Equal` 是 `equal` 所用的 trait，相当于 Rust 的 `PartialEq`。`Ord` 相当于 Rust 的 `PartialOrd`。在泛型函数的
`:where` 约束中用它们来要求元素类型可比较。

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; 必须实现
(deftrait Ord (Equal)                                     ; 继承 Equal
  (defmethod less (self (other Self)) bool)                  ; 必须实现
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

实现 `Equal` 要写 `equal`，实现 `Ord` 只需写 `less`。`Ord` 的其余部分由默认实现补上。`Ord` 继承 `Equal`，
所以 `impl Ord X` 之前需要 `impl Equal X`。

`(equal a b)` 调用 `a` 的类型的 `Equal` 实现。对未实现 `Equal` 的类型写它会是类型错误。按同一性比较用
`eq`/`eql`，按结构比较用 `equalp`。这三个可用于任何类型的值。

各 trait 方法可以直接作为函数调用（在 `where (Equal A)`/`(Ord A)` 约束内，或对已实现它的具体类型）：

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | 是否相等（Rust 的 `==`） |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

已实现 `Equal` 的：所有数值类型（`i8`〜`u32` / `f32` / `f64` / `int` / `ratio`）与 `bool` `char` `string`
`symbol` `complex`、`Sexpr`、`cons-cell<A,B>` 和元组（元素为 `Equal` 时逐元素比较）、`Option<T>` 与
`Result<T,E>`（变体相同且内容 `equal` 时）。内置类型的各自内置方法 `equal` 原样就是 `Equal` 的实现。数值用
`=` 比较，字符串区分大小写按内容比较。已实现 `Ord` 的：所有数值类型以及 `char` `string`，以及
`cons-cell<A,B>`（元素为 `Ord` 时按字典序）。

列表（`Sexpr`）的 `equal` 是 CL 的 `equal`：cons 按结构、字符串按内容、其余原子用 `eql` 比较。这种比较在运行时
遍历值，不调用任何方法。因此列表中的结构体与 CL 一样按同一性比较，与其类型的 `impl Equal` 无关。

`Ord` 的方法名不与内置运算符（`< <= > >=`）或 `lt` 重名，是因为内置的无法重定义，而各实现都委托给它们。
标量比较运算符本身是按接收者类型的内置方法（[数值](numbers.md)・[字符串与字符](collections.md)）。在约束内
写运算符，会被读作 trait 的方法（第 3 章）。`=` 变为 `equal`，`/=` 变为 `(not (equal a b))`。

## 3. 算术 trait（`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`）

供泛型代码要求"可以相加的类型"的一层。**具体类型的运算使用内置运算符**（[数值](numbers.md)），不经过这一层。

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; 距离总是 int（与 ash 相同）
(deftrait Number (Add Sub Mul Rem Ord))                   ; 没有方法，五个的组合
```

**在约束内可以用运算符书写。** 当接收者是由 `:where` 约束的类型变量时，运算符被解读为 trait 方法（`+`→`add`、
`-`→`sub`、`*`→`mul`、`/`→`div`、`rem`→`remainder`、`logand`→`bit-and`、`=`→`equal`、`<`→`less` …）：

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

trait 一侧的方法名不是 `+`，是因为 `+` 是内置方法名，`impl` 会拒绝重定义（`cannot redefine built-in method`）。没有
`Neg`——`(- x)` 展开为 `(- (- x x) x)`，所以只需 `Sub`。

已实现：`Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` 用于所有数值类型（除 `complex` 外），`Bits` 用于所有整数类型和 `int`。


**`Div` 用关联类型 `Output` 表明结果类型。** 与 CL 一样，整数之间的 `/` 是 `rational`，因此 `int` 的
`Output` 是 `rational`，其他数值类型则是自身。由于 `Output` 因类型而异，`Number` 不包含 `Div`（继承
列表必须固定所有关联类型）。做除法的泛型函数用 `(:where (Div T (Output R)))` 接收结果类型，或者用
`(:where (Div T (Output T)))` 要求在 `T` 内封闭的除法（此时不能传 `int`）。

## 4. `Push` trait（添加元素）

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

可以一个一个添加元素的集合。泛型函数用 `(:where (Push C (Item T)))` 要求“可以添加 `T` 的类型”，并对任何这样的集合用同一个 `(push c x)` 添加。

| 类型 | `push` 的含义 |
|---|---|
| `Vector<T>` | 追加到末尾 |
| `Deque<T>` | 追加到末尾（`push-back`） |
| `Array<T>` | 在填充指针的位置添加（`push-extend`）。没有填充指针时 panic |
| `HashSet<T>` | 放入集合（`insert`） |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

为自己的类型实现 `Push`，就能传给同样的泛型函数。也可以像 `:dyn Push<int>` 那样作为 trait 对象。

## 5. `Pop` trait（取出元素）

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

可以取回 `push` 最后添加的元素的集合。为空时返回 `none`。泛型函数用 `(:where (Pop C (Item T)))` 要求它。

| 类型 | `pop` 的含义 |
|---|---|
| `Vector<T>` | 取出末尾元素 |
| `Deque<T>` | 取出末尾元素（`pop-back`） |
| `Array<T>` | 取出 fill pointer 前面的那个元素。没有 fill pointer 时 panic |

`HashSet<T>` 没有“最后的元素”，所以它是 `Push` 但不是 `Pop`。

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

和 `Push` 一样，可以为自己的类型实现它，也可以像 `:dyn Pop<int>` 那样作为 trait 对象。
