<!-- translated-from: docs/ja/reference/functions/collections.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 字符串、字符与集合

`string`、`char`、`Vector<T>`、`HashTable<K,V>`、`Array<T>`、`BitVector`。

## 1. 字符串 `string`

字符串是不可变的。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | 转为大写（仅 ASCII）。与 CL 的 `string-upcase` 一样返回新字符串。字符串不可变，所以没有破坏性版本 `nstring-upcase`，由它代替 |
| `downcase` | `(downcase s)` | `string→string` | 转为小写（仅 ASCII）。代替 `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | 每个词的首字母大写、其余小写（CL 的 `string-capitalize`）。词是字母数字的极大连续序列 |
| `length` | `(length s)` | `string→int` | 字符数 |
| `ref` | `(ref s i)` | `(string,int)→char` | 第 `i` 个字符。越界时 panic |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | 子字符串 `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | 连接。也可以写 3 个以上（与 `(concatenate 'string ...)` 相同） |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | 字典序比较 |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | 字典序的严格小于（与 `<` 相同） |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | 同一性比较（比较是否为同一对象，而不是内容） |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | 内容比较（区分大小写） |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | 内容比较（忽略大小写，仅 ASCII） |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | 内容是否不同（CL 的 `string/=`。可变参数形式比较相邻的对） |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | 忽略大小写的顺序比较（CL 的 `string-lessp` 等）。有公共前缀时较短者较小 |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | 由 `n` 个 `c` 组成的字符串（CL 的 `make-string`） |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | `sub` 第一次出现的位置。**CL 的 `search` 参数顺序相反**（`(search pattern sequence)`）。空字符串在 0 处找到。关键字见[序列的关键字参数](sequences.md#6-关键字参数) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | 第一个不一致的位置。只有 `equal` 时为 `none`。一方是另一方的前缀时为较短者的末尾。关键字同上 |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | 从两端／左端／右端去掉 `bag` 中包含的字符（CL 的 `string-trim` 等）。省略 `bag` 时为空白字符 `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | 以 `sep` 分割。CL 中没有对应物。连续的分隔符会产生空元素。`sep` 为空时 panic |
| `to-string` | `(to-string x)` | `T→string` | 相当于 `~a` 的字符串化。为 `int`/`i32`/`f64`/`bool`/`char`/`string` 实现（CL 的 `princ-to-string`） |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | 编码为 UTF-8（每个元素 0..255） |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | 解码。不是合法的 UTF-8 时为 `none` |

## 2. 字符 `char`

`char` 是 Unicode 标量值。大小写转换和分类只处理 ASCII 范围。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | 转为大写（仅 ASCII） |
| `downcase` | `(downcase c)` | `char→char` | 转为小写（仅 ASCII） |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | 按码点比较 |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | 按码点的严格小于（与 `<` 相同） |
| `alphap` | `(alphap c)` | `char→bool` | 是否为 ASCII 字母 |
| `digitp` | `(digitp c)` | `char→bool` | 是否为 ASCII 数字 |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | 值的比较 |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | 忽略大小写的值比较（CL 的 `char-equal`） |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | 值是否不同（CL 的 `char/=`。**可变参数形式比较相邻的对**，与询问所有对是否互不相同的 CL 不同） |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | 忽略大小写的顺序比较（CL 的 `char-lessp` 等） |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | 是否大写／是否小写／是否有大小写之分（CL 的 `upper-case-p` 等） |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | 是否为字母或数字（与 CL 同名） |
| `graphicp` | `(graphicp c)` | `char→bool` | 是否可打印。包括空格，不包括换行和制表符（CL 的 `graphic-char-p`） |
| `standardp` | `(standardp c)` | `char→bool` | 是否为 CL 的 96 个标准字符之一，即 `graphicp` 再加上换行（CL 的 `standard-char-p`） |
| `char->int` | `(char->int c)` | `char→int` | Unicode 标量值（反方向是[数值](numbers.md#1-固定宽度整数)的 `int->char`/`try-int->char`）。相当于 CL 的 `char-code`/`char-int` |
| `char->string` | `(char->string c)` | `char→string` | 只含一个字符的字符串。CL 的 `string` 函数通过接受指定符兼顾这一点，但本语言没有指定符，所以把方向写进了名字 |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | 该基数下数字的**权值**（CL 的 `digit-char-p`）。`digitp` 是返回 `bool` 的另一个函数 |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | 表示权值 `w` 的字符。10 以上为大写（CL 的 `digit-char`。基数最大 36） |
| `char->name` | `(char->name c)` | `char→Option<string>` | 字符名。只有读取器能读的命名字符才有名字（CL 的 `char-name`） |
| `name->char` | `(name->char s)` | `string→Option<char>` | 由字符名得到字符。忽略大小写，也接受读取器的别名（`linefeed`/`null`）（CL 的 `name-char`） |

没有相当于 `char-code-limit` 的常量（`char` 的上限由 Unicode 而不是语言决定）。

## 3. `Vector<T>`

可变长数组。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | 创建空向量。类型参数由期望类型决定，所以在裸 `let` 中写成 `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` 个 `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | 追加到末尾 |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | 读取第 `i` 个。越界时 panic |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | 修改第 `i` 个。越界时 panic。也可以写成 `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | 元素个数 |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | 去掉末尾元素并返回。为空时为 `None`（与 `get`/`set` 不同，不会 panic） |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | 创建实现了 `Iter` 的迭代器 |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | 没有与 `x` 相等的元素时追加到末尾（CL 的 `pushnew`。不需要修改位置，所以是方法而不是宏） |

`map`/`filter` 等是[序列函数](sequences.md#4-iter-上的序列函数)——像 `(map (iter v) f)` 这样经 `iter` 传入。破坏性
操作（`nreverse`、`delete` 等）见[破坏性操作](sequences.md#7-破坏性操作)。

## 4. `HashTable<K,V>`

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | 创建空表 |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | 查找 |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | 插入或覆盖 |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | 删除，如有旧值则返回 |
| `count` | `(count h)` | `HashTable<K,V>→int` | 元素个数 |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | 全部删除 |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | 键的快照 |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | 值的快照 |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | `(k . v)` 序对的快照 |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | 实现了 `Iter` 的迭代器。元素是 `(k . v)` 的 `cons-cell`。相当于 CL 的 `with-hash-table-iterator`，`doiter`/`map`/`filter` 等可以直接使用 |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | CL 的 `maphash` |
| `size` | `(size h)` | `HashTable<K,V>→int` | CL 的 `hash-table-size`。在本表中是占用数（等于 `count`） |

**只要实现了 `Hash`，任何类型都可以作为键**——包括 `defstruct`/`defenum`。`get`/`set`/`remove` 带有
`(where (Hash K))`，所以以没有实现它的类型为键的表是**类型错误**（`f64` 没有 `Hash` 是因为 `NaN`）。

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; 返回非负且放得进 fixnum 的值
```

已实现：`int` 和 6 种固定宽度整数、`bool`、`char`、`string`、`symbol`（浮点数没有）。在自己的类型中，把结果与
`*sxhash-mask*`（2^30-1）做 `logand` 以保持非负。想对字符串求哈希时，可以调用 `string` 的实现所使用的
`(sxhash-string s)`（FNV-1a 32 位）。

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

键是否相同由**键的类型本身**决定（`sxhash`，以及 `Hash` 的超 trait `Eq` 的 `equals`），而不是对象的同一性。所以像上面
那样，可以用"不同的值但相等"的键查找。

`sxhash` 冲突也无妨（`Hash` 的约定是反方向的——只规定 `equals` 相等则哈希相同）。冲突的键由 `equals` 区分。

## 5. `Array<T>`（多维数组）

标准库中的 `defstruct`。不是内置类型，所以 `defstruct` 能做的事全都可以做。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | CL 的 `make-array`。`dims` 会被复制。`init` 是所有单元的初始值（CL 的 `:initial-element`。本语言没有"未绑定的单元"，所以是必需的）。`:fill-pointer` 只用于一维 |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | CL 的 `aref` / `(setf (aref …))`。下标越界时 panic |
| `aref` | `(aref a i j …)` | — | 用裸下标书写的 CL 写法。展开为上面的 `get`/`set`。`(setf (aref a i j) v)` 也一样 |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | CL 的 `row-major-aref`。平坦的下标 |
| `rank` | `(rank a)` | `Array<T>→int` | CL 的 `array-rank` |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | CL 的 `array-dimension` |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | CL 的 `array-dimensions`。与 CL 返回新列表一样，返回**副本** |
| `total-size` | `(total-size a)` | `Array<T>→int` | CL 的 `array-total-size`（与填充指针无关的已分配单元数） |
| `len` | `(len a)` | `Array<T>→int` | CL 对数组的 `length`。有填充指针时为其值，否则为 `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | CL 的 `array-in-bounds-p`。下标的**个数**不对时也为假（不是错误） |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | CL 的 `array-row-major-index` |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | CL 的 `adjust-array`。不能改变秩。仍在范围内的元素按下标保留，新增的单元为 `init`。与 CL 不同，不返回数组（本语言的数组都是可调整的，没有要返回的第二个数组） |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CL 的 `vector-push-extend`。没有填充指针时 panic |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CL 的 `vector-pop`。为空时为 `none` |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | 填充指针（没有时为 `none`）。可以用 `(setf a::fill-pointer …)` 写入 |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | 按行主序的迭代器。有填充指针时在那里停止 |

- **下标是 `Vector<int>`。** 方法无法声明"末尾跟着若干个同类型参数"的形式，`aref` 语法糖弥补了这一差距。
- **没有** `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`。接收者的
  静态类型已经回答了这些问题。
- `Array::new` 是 `defstruct` 生成的按字段顺序的构造函数，不是用来创建数组的。请使用 `Array::make`。
- **打印采用 CL 的数组语法。** 秩 1 为 `#(1 2 3)`，其他为 `#nA` 加上与维数相同层数的括号（`#2A((1 2 3) (4 5 6))`），
  秩 0 为 `#0A5`。有填充指针时在那里截断。把 `*print-array*`（[打印](printing.md#6-控制打印量)）设为假，就只打印
  形状 `#<array 2x3>`。只有元素是没有写 `print-object` 的 `defstruct` 的数组以内置形式 `#<array<...> ...>` 打印
  （不是错误）。

## 6. `BitVector`（位向量）

固定长度的位序列。标准库中的 `defstruct`。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | 长度为 `n`，所有位为 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | 越界时 panic |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | CL 的写法。也可以写 `(setf (bit v i) b)`。CL 的 `sbit` 与 `bit` 的唯一区别是要求简单位向量，但本语言的位向量只有一种 |
| `len` | `(len v)` | `BitVector→int` | 位数 |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | 返回新的位向量。长度不同时 panic。没有 CL 的第 3 个参数（结果的写入目标） |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | 补集 |

没有 `bit-vector-p`（静态类型已经回答了）。
