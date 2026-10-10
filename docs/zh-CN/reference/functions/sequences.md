<!-- translated-from: docs/ja/reference/functions/sequences.md @ eae672c1a271f0b6947f024e81dee8338b2f5ff4 -->
# 序对、S 表达式与序列

泛型序对 `cons-cell`、S 表达式数据 `Sexpr`、符号、构建在 `Iter` 之上的序列函数，以及高阶函数。

## 1. 序对 `cons-cell<A,B>`

`cons`/`car`/`cdr` 是**泛型序对类型 `cons-cell<A,B>`**（标准库中的 `defstruct`）的构造函数和字段访问器。字段既可以
用 `变量::car`/`变量::cdr`（[语法参考](../syntax.md#36-defstruct--结构体用户定义类型)中 `defstruct` 的访问器语法）
读取，也可以用 `(car 变量)`/`(cdr 变量)` 读取。修改时使用 `(setf 变量::car v)`/`(setf 变量::cdr v)`。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | 创建序对 |
| `car` | `(car p)` | `cons-cell<A,B>→A` | 第一个元素 |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | 其余部分 |

`cons-cell` 也用来代替元组语法。返回多值的 CL 函数（`floor` 的商和余数、`read-from-string` 的值和位置等）在本语言中
返回 `cons-cell`。

## 2. S 表达式数据 `Sexpr`

`read` 返回的数据类型 `Sexpr` 有 19 种变体：
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`。
`vector` 和 `array` 是用 `#(..)` 和 `#nA(..)` 写的数据（[语法参考](../syntax.md#1-词法元素)），内容分别是 `Vector<Option<Sexpr>>` 和 `Array<Option<Sexpr>>`——对 `(vector v)` 绑定的 `v` 可以直接使用 `len`、`get` 等。
`tuple` 是用 `#{..}` 写的数据，`(tuple v)` 绑定的 `v` 是按顺序排列元素的新 `Vector<Option<Sexpr>>`（为了用同一个类型接收任意长度的元组）。
处理 S 表达式单元的不是第 1 章通用的 `cons`/`car`/`cdr`，而是 `sexpr-*` 函数。主要在 `defmacro` 的函数体中用于组装
和拆解形式。

**S 表达式数据的类型是 `Option<Sexpr>`。** 空列表不是 `Sexpr` 的变体，而是 `Option` 的 `none`，`Sexpr` 本身表示
"非空的 S 表达式"。因此 `sexpr-*` 的参数和返回值都是 `Option<Sexpr>`。

- 在期望 `Option<Sexpr>` 的位置，`()` 是空列表（也可以写成 `(Option::none)`）
- 在期望 `Option<Sexpr>` 的位置，`Sexpr` 会隐式扩展（没有运行时转换）。反方向——把 `Option<Sexpr>` 当作 `Sexpr`
  使用——是在声称"不是空列表"，需要用 `match` 或 `unwrap` 明确表示
- 在 `match` 中，`Sexpr` 的 19 种变体和 `none` 可以**平铺在同一组分支中**
  （[语法参考](../syntax.md#43-match--模式匹配)）

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | 创建 `Sexpr` 单元 |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | 第一个元素。**空列表时返回空列表**（遵循 CL）。不是 `Cons` 的原子时 panic |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | 其余部分。**空列表时返回空列表**（遵循 CL）。不是 `Cons` 的原子时 panic |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | 是否为 `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | 是否为空列表 |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | 是否不是 `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | 是否为 `Sym`（符号） |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | `int` 变体的内容（fixnum 或多精度）。类型不符时 panic |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | 该宽度变体的内容。类型不符时 panic |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | 浮点变体的内容。类型不符时 panic |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | `Char` 的内容。类型不符时 panic |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | `Bool` 的内容。类型不符时 panic |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | `Str` 的内容。类型不符时 panic |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | `Sym` 的名字。类型不符时 panic |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 同一性比较（`Cons`/`Str` 比较对象同一性，其他比较值） |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 结构相等（`Cons` 递归比较，`Str` 比较内容） |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 在 `equal` 的基础上忽略大小写，并跨类型比较数值 |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | 连接两个 `Sexpr` 列表（非破坏性）。`,@` 展开为它 |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | 对 `Sexpr` 列表的每个元素应用 `f` 得到的新 `Sexpr` 列表（第 4 章的 `map` 用于 `Iter`，不能遍历 `Sexpr` 列表） |

数值访问器按类型分为 9 个，是因为 `Sexpr` 是"值的类型在其他任何地方都没有写出的唯一场所"。放入 `Sexpr` 的 `u8` 作为
`u8` 变体放入，只能用 `(sexpr-u8 s)` 取出。传给 `(sexpr-int s)` 会 panic——不会悄悄扩展宽度来作答。读入的数据
（`'(1 2 3)`、宏的参数）中的整数是 `int` 变体，用 `(sexpr-int s)` 读取。

`Sexpr` 列表没有 `rplaca`/`nconc` 之类的破坏性操作。`Sexpr` 单元创建后不能修改。

## 3. 符号

`symbol` 是符号本身的类型。在需要 `Sexpr` 的上下文中会隐式转换，但反方向没有自动转换。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | 取出符号名 |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | 从字符串创建符号（intern） |
| `keywordp` | `(keywordp s)` | `symbol→bool` | 是否为关键字（`:name`）。冒号是名字的一部分，所以按首字符判断（[语法参考](../syntax.md#1-词法元素)） |

`gensym` 见[宏](system.md#8-宏)。

## 4. `Iter` 上的序列函数

序列函数是 **`Iter` trait 上的泛型函数**。从集合用 `(iter coll)` 取得迭代器后传入（`Vector<T>` / `HashTable<K,V>` /
`Array<T>` 支持。`Sexpr` 列表没有实现 `Iter`，不是这些函数的对象）。**结果集合以新的 `Vector` 返回。** 表中的
`Iter<A>` 表示"`Item` 为 `A` 的任意 `Iter` 实现类型"。要再次遍历返回的 `Vector`，传入 `(iter result)`。

接受谓词的函数（对应 CL 的 `-if` 系列）：

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | 映射 |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 只保留满足条件的元素 |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 去掉满足条件的元素 |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | 第一个满足条件的元素 |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | 第一个满足条件的位置 |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | 满足条件的个数 |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 是否所有元素都满足条件 |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 是否有元素满足条件（相当于 CL 的 `some`。是不与 `Some` 构造函数冲突的名字） |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | 左折叠 |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | 右折叠 |
| `collect` | `(collect it)` | `Iter<A>→Vector<A>` | 收集剩下的全部元素。用来把下面 `lazy` 函数的结果变成 `Vector` |

下标、长度与切片：

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | 元素个数 |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | 连接迭代器。也可以写 3 个以上 |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | CL 的 `concatenate`。结果类型写成**引用符号的字面量**（CL 是运行时的类型说明符）。`'vector` 需要 1 个以上，`'string` 0 个以上（0 个时为 `""`）。`Sexpr` 列表不适用（用 `sexpr-append`） |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | 反转（非破坏性） |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | 第 `n` 个元素（越界时为 `None`） |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | 参数顺序不同的 `nth` |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | 前 `n` 个 |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)`（`end` 按长度截断） |
| `last` | `(last it)` | `Iter<A>→Option<A>` | 最后一个**元素**（不是 CL 的"最后一个单元"） |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | 去掉最后一个元素 |

要求 `Eq` / `Ord` 约束的函数（用 trait 代替谓词进行比较。[标准 trait](traits.md#2-eq--ord比较)）：

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | 是否有与 `x` 相等的元素（与 CL 不同，返回 `bool` 而不是列表的剩余部分） |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | 第一个与 `x` 相等的元素 |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | 第一个与 `x` 相等的位置 |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | 与 `x` 相等的元素个数 |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL 的 `(sort sequence predicate)`。稳定的非破坏性排序。`cmp` 在"第 1 个参数严格排在第 2 个参数之前"时为 `true` |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | 第一个 `car` 与 `k` 相等的序对。值用 `(cdr p)` 取出 |

这些函数以及第 5 章的许多函数也接受 CL 的关键字参数 `:key` / `:test` / `:test-not` / `:start` / `:end` /
`:from-end` / `:count`（第 6 章）。

### 惰性迭代器（`lazy` 模块）

`lazy` 模块的函数不构建 `Vector`，而是**返回迭代器**。元素在下一个元素被请求时才计算，因此没有尽头的迭代器（`iterate`、`repeat`）只要在下游用 `take` 或 `take-while` 停下就能使用。返回值都实现了 `Iter`，所以 `lazy` 的函数可以层层嵌套，也可以直接传给上面表中的函数。要变成 `Vector` 就用 `collect`。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `lazy::map` | `(lazy::map it f)` | `(Iter<A>,(fn (A) U))→Iter<U>` | 对每个元素应用 `f` |
| `lazy::filter` | `(lazy::filter it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | 只保留满足条件的元素 |
| `lazy::take` | `(lazy::take it n)` | `(Iter<A>,int)→Iter<A>` | 前 `n` 个 |
| `lazy::take-while` | `(lazy::take-while it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | 直到第一个不满足条件的元素之前 |
| `lazy::skip` | `(lazy::skip it n)` | `(Iter<A>,int)→Iter<A>` | 跳过前 `n` 个 |
| `lazy::enumerate` | `(lazy::enumerate it)` | `Iter<A>→Iter<#{int A}>` | 从 0 开始计数的位置与元素的组 |
| `lazy::zip` | `(lazy::zip a b)` | `(Iter<A>,Iter<B>)→Iter<#{A B}>` | 从两边各取一个组成的组。随较短的一方结束 |
| `lazy::chain` | `(lazy::chain a b)` | `(Iter<A>,Iter<A>)→Iter<A>` | 先是 `a` 的元素，然后是 `b` 的元素 |
| `lazy::flat-map` | `(lazy::flat-map it f)` | `(Iter<A>,(fn (A) Iter<B>))→Iter<B>` | 用 `f` 把每个元素变成迭代器，再依次连接起来 |
| `lazy::iterate` | `(lazy::iterate x f)` | `(A,(fn (A) A))→Iter<A>` | `x`、`(f x)`、`(f (f x))`……无尽地继续 |
| `lazy::repeat` | `(lazy::repeat x)` | `A→Iter<A>` | 无尽地重复 `x` |

表中的 `Iter<U>` 等实际上是在函数名后加上 `-iter` 的结构体类型（`lazy::map` 的是 `lazy::map-iter<I,A,U>`，`I` 是源迭代器的类型）。只有在不写就无法确定的地方才需要写出类型，例如传给 `lazy::flat-map` 的 lambda 的返回值。

```lisp
(collect (lazy::take (lazy::filter (lazy::iterate 1 (lambda ((n int)) int (+ n 1)))
                                   (lambda ((n int)) bool (= 0 (mod n 3))))
                     4))                                  ; => #(3 6 9 12)

(doiter (#{i s} (lazy::enumerate (iter (the Vector<string> #("a" "b")))))
  (println "~a: ~a" i s))                                 ; 0: a 和 1: b

(-> (lazy::iterate 1 (lambda ((n int)) int (* n 2)))
    (lazy::take-while (lambda ((n int)) bool (< n 100)))
    collect)                                              ; => #(1 2 4 8 16 32 64)
```

`->` 是把值依次作为后续各个表达式的第 1 个参数传入的宏（[Option 与 Result](option-result.md)）。

## 5. CL 其余的序列函数

都是与第 4 章相同的 `Iter` 上的泛型函数。结果集合以新的 `Vector` 返回。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CL 的命名下标 |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | 去掉第一个后的剩余部分（是新的 `Vector`，不是共享的尾部） |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | 把迭代器实体化为 `Vector`（CL 的 `copy-seq`/`copy-list`） |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | 反转 `a` 后接上 `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` 个 `x`（CL 的 `make-list`/`make-sequence`）。与 `Vector::new` 一样类型参数来自期望类型，所以裸 `let` 需要 `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 与 `member` 一样返回 **`bool`**（迭代器没有可返回的尾部） |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `any`/`every` 的否定 |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | 与各肯定版同类型 | 谓词取反的版本 |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | 按值删除 |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | 去重。遵循 CL **保留最后一次出现**（要保留第一次出现用 `:from-end true`） |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | 按值／谓词替换 |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | 作用于 `Iter<cons-cell<K,V>>` | `assoc` 的谓词版、值侧版 |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | 在开头添加序对 |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | 把两个序列配对。在较短者处停止 |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | CL 多序列的 `mapcar`。在较短者处停止 |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | 用于副作用的映射 |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | 映射后连接 |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | 对连续的**尾部**映射 |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | 对尾部的副作用映射（`maplist` 版的 `mapc`） |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | 对尾部映射后连接（`maplist` 版的 `mapcan`） |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | `sub` 第一次出现的位置。接收者是 `string` 时选择 `string` 的方法（[字符串](collections.md#1-字符串-string)） |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | 第一个不一致的位置。相等时为 `none` |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | 合并。CL 要求已排序，这里对连接结果排序 |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | 没有时添加**到开头** |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | 集合运算。CL 不规定顺序，这里按**首次出现顺序**保持稳定 |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | 包含关系 |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | 是否为后缀／去掉后缀的前半部分。CL 询问的是**结构共享**，但这里没有可共享的结构，所以询问的是**按值**的后缀 |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | 逐元素相等。`Vector<T>` 本身没有实现 `Eq` |
| `caar`…`cddddr` | `(cadr p)` | 作用于嵌套的序对 | CL 的 28 个函数。遍历的是**序对而不是列表**，`cadr` 接受 `cons-cell<A,cons-cell<B,C>>` |

CL 中有而这里没有的：`list*`（没有替换末尾的非正规列表这一概念）、`copy-tree`/`copy-alist`/`sublis`/`subst`/
`subst-if`（无法写出遍历任意深度异构树的类型。作为 `Sexpr` 的树，`equal` 相当于 `tree-equal`）、属性列表一套
`getf`/`get-properties`/`symbol-plist`/`remprop`（没有键值交替排列的无类型列表这种表示。同样的作用由 `assoc`
（关联列表）或 `HashTable` 承担）、在 `Vector<T>` 与 `Sexpr` 列表之间相互转换的函数（`Sexpr` 列表的每个元素类型可能
不同，无法用单一元素类型 `T` 写出）。

## 6. 关键字参数

第 4 章和第 5 章的函数接受 CL 序列函数的关键字 `:key` / `:test` / `:test-not` / `:start` / `:end` / `:from-end` /
`:count`。都是**可省略的**。

| 关键字 | 类型 | 含义 |
|---|---|---|
| `:key` | `(fn (A) A)` | 在比较或谓词之前应用于元素的投影 |
| `:test` | `(fn (A A) bool)` | 代替 `Eq` 约束的 `equals` 使用的相等判断。第 1 个参数是**要找的项**，第 2 个参数是（应用 `:key` 后的）元素——与 CL 顺序相同 |
| `:test-not` | `(fn (A A) bool)` | `:test` 的否定 |
| `:start` `:end` | `int` | 扫描的窗口 `[start, end)`。下标相对于整个序列 |
| `:from-end` | `bool` | 查找返回**最后一个**匹配。与 `:count` 一起使用时从末尾一侧开始影响 |
| `:count` | `int` | `remove`／`substitute` 系列影响的最大个数 |

哪个函数接受哪些关键字遵循 CL：

| 函数 | 接受的关键字 |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | 上面全部（包括 `:count`） |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not`（`assoc` 的 `:key` 作用于 `car`，`rassoc` 的作用于 `cdr`） |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; 只删除末尾一侧的 1 个
(position 3 (iter v) :start 1)                          ; 下标相对于整个序列
```

**与 CL 的不同之处**：

1. **`:key` 的投影封闭在元素类型之内**（`(fn (A) A)`）。不能像 CL 那样投影到其他类型——增加类型变量的话，省略时就
   无法确定。需要投影到不同类型时，可以改为给 `-if` 系列传 lambda（`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`）。
2. **在基于项的查找中，`:key` 只作用于元素**（不作用于要找的项）。与 CL 的 `find`/`position`/`count`/`member`/
   `remove`/`substitute` 规则相同。集合运算中两边都是元素，所以两边都作用。
3. **只有 `search` 的关键字是名称而不是编号。** CL 中 `:start1`/`:end1` 是**模式**，`:start2`/`:end2` 是被搜索的
   序列，但本语言中接收者在前，同样的编号意思正好相反——而且是悄无声息的。`:start`/`:end` 是接收者，
   `:sub-start`/`:sub-end` 是模式，所以顺手写下的 `:start1` 会得到"未知关键字"错误。`mismatch` 和 `replace` 的
   参数顺序与 CL 一致，所以保留 CL 的编号。

## 7. 破坏性操作

`Vector<T>` 的方法。**修改接收者并返回接收者本身**，所以 `(nreverse v)` 写法与 `reverse` 相同，`v` 自身也会反转。

| 名称 | 形式 | 说明 |
|---|---|---|
| `nreverse` | `(nreverse v)` | 原地反转 |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | `remove`／`remove-if`／`filter`／`remove-duplicates` 的原地版本 |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | `substitute` 系列的原地版本 |
| `nbutlast` | `(nbutlast v)` | 去掉末尾一个 |
| `fill` | `(fill v x)` | 所有元素设为 `x`。不改变长度 |
| `replace` | `(replace v src)` | 从开头用 `src` 的元素覆盖。`(min (len v) (len src))` 个 |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`。个数同上 |
| `nconc` | `(nconc v w)` | 把 `w` 的元素追加到 `v`。与 CL 不同，**不是修改共享结构**（`w` 不受影响） |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | 用 `src` 替换 `v` 的内容（长度也会改变） |
| `rplaca` `rplacd` | `(rplaca p x)` | 修改 `cons-cell` 的 `car`/`cdr` 并返回该单元本身 |

接受的关键字：

| 破坏性版本 | 接受的关键字 |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2`（接收者是 CL 的 `sequence-1`） |

`vector-push-extend`/`vector-pop` 就是 `Vector<T>` 的 `push`/`pop` 本身——`Vector<T>` 总是可以增长，所以没有对应 CL
中"带填充指针的向量"与"简单向量"之分的东西。

## 8. 高阶函数

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | 原样返回 |
| `const` | `(const x y)` | `(A,B)→A` | 返回第一个参数 |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | 函数复合 `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | 交换双参数函数的参数顺序 |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | 谓词的否定 |

没有 CL 的 `constantly`（被忽略的参数的类型只出现在返回类型中，无法确定）。请写成 `(lambda ((x T)) A v)`。
