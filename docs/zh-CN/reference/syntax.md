<!-- translated-from: docs/ja/reference/syntax.md @ 37af68009626057caa98d1dc23e3879b42e983c7 -->
# typelisp 语法参考

typelisp 是一种静态类型的 Lisp，语法采用 S 表达式。内置函数和方法的一览见[内置函数](functions/README.md)，类型一览见
[types.md](types.md)，错误消息的读法见 [errors.md](errors.md)。

## 1. 词法元素

- **不区分大小写。** 符号在读取时全部规范化为小写。
- **注释**：从 `;` 到行尾（行注释）。`#| ... |#`（可以嵌套的块注释）。
- **读取时求值**：`#.(式)` **在读取的同时执行**后面的形式，并把它的值当作读到的东西。这是读取器不只是文本函数的唯一地方。
  能达到的范围取决于读取路径，这一点与 CL 相同：
  - `(load ...)` 和 REPL 逐个形式求值，所以可以调用**同一文本中前面定义的函数**（CL 的 `load`）。
  - 模块文件作为一个单元检查，执行由 `use` 它的一方进行，所以 `#.` 能达到的只有标准库和该会话已经执行过的东西。文件自身的
    定义以及它 `use` 的模块的定义都**还没有运行**（与 CL 的 `compile-file` 需要 `eval-when` 相同）。
  - 程序中的 `read` / `read-from-string` 也会求值 `#.`（与 CL 相同）。
  - 把 `*read-eval*`（默认 `true`）设为 `false`，`#.` 在任何地方都是读取错误——这是不让作为数据读取的文本执行代码的开关
    （与 CL 相同）。每次遇到 `#.` 都会读取它，所以 `setf` 从下一个读取的形式开始生效。在 `with-standard-io-syntax` 中为 `true`。
- **布尔值**：`true` / `false`。
- **整数**：十进制（`42`、`-7`）。可以前置符号 `+`/`-`。十进制以外用 CL 的基数宏 `#b`/`#o`/`#x`/`#NNr` 书写（符号在标记之后，
  `#x-ff`）。CL 中没有 `0x` 前缀，所以不采用——`0xff` 会被读作符号。
  没有类型注解的整数字面量默认是 `int`（任意精度，[数值](functions/numbers.md#3-任意精度整数-int)）——大小没有上限。
  **期望类型是固定宽度整数类型时，字面量就是该类型，并会检查该类型能否容纳这个值**——`(the u8 300)` 是类型错误（想要截断时写
  `(as u8 300)`）。`(the u32 4294967295)` 和 `(the u32 #xFFFFFFFF)` 正是借助这条规则才能写出。`int` 的值是放进 63 位立即值
  还是成为多精度，由值的大小决定，没有专门的语法（与 CL 相同）。
- **浮点数**：包含小数点或指数记号（`e`/`E`）的数（`1.5`、`3.0e10`）。默认为 `f64`（期望类型是 `f32` 时为该类型）。
- **比例 (ratio)**：`分子/分母`（只有十进制，例如 `1/3`）。读取时按 CL 规范约分（`2/4` 为 `1/2`）。值为整数的（`4/2` 等）读作
  `int` 而不是 `ratio`。分母为 `0`（`1/0`）是读取错误。
- **字符**：`#\` 后跟一个字符或命名字符。例如 `#\a` `#\Space` `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul`（也可写
  `#\Null`）`#\Backspace`。名称不区分大小写。
- **字符串**：`"..."`。转义有 `\n` `\t` `\r` `\0` `\\` `\"`（其他的 `\x` 就是 `x`）。
- **符号**：包含字母、数字和符号的任意记号（`+` `<=` `my-func` 等）。
  `]` 和 `}` 会结束一个记号，所以不能写在符号中；在数据的开头遇到它们是读取错误。`[` 和 `{` 可以写在符号中：与 CL 一样，它们留给程序员在[读取宏](#11-读取宏readtable)中使用。
- **关键字**：像 `:name` 这样以冒号开头的符号（遵循 CL）。它是自求值的——不查找绑定，值就是它自身，静态类型为 `symbol`。同名的
  关键字总是同一个对象（`(eq :foo :FOO)` 为真。与其他符号一样会被转为小写）。冒号本身是名字的一部分，`(symbol->string :foo)`
  是 `":foo"`（typelisp 没有包机制，所以与 CL 的 `symbol-name` 不同）。单独的 `:` 或像 `:a:b` 这样包含额外冒号的是读取错误。
  判断用 `keywordp`。以 `::` 开头的不是关键字，而是绝对路径（见下文）。
  另外，`:dyn` 是只用于类型位置的保留关键字，写在其他位置是错误（见[第 2 章](#2-类型的写法)）。
- **列表**：`(a b c)`。点对 `(a . b)` 也可以读取。
- **向量**：`#(1 2 3)`（与 CL 相同）。内容全部是字面量，不会求值——`#(a b)` 中的 `a` 是符号而不是变量。元素类型由上下文决定（`(the Vector<i32> #(1 2))`），没有上下文时取第一个元素的类型（`#(1 2 3)` 是 `Vector<int>`）。元素类型必须一致，`#(1 "a")` 是类型错误。既没有元素也没有上下文的 `#()` 也是类型错误。每次求值都会创建新的向量。在期望 S 表达式数据的位置（`(the Option<Sexpr> #(1 x))`、`'#(..)`、`read` 读到的数据），它是元素全部为数据的 `Vector<Option<Sexpr>>`——即 `Sexpr` 的 `vector` 变体。
- **数组**：`#2A((1 2) (3 4))`（与 CL 相同）。`#` 与 `A` 之间的数是维数，内容中列表嵌套的前这么多层就是各个维度。`#0A x` 是只有一个元素的零维数组。同一层的列表长度不一致是读取错误。类型的确定方式与向量相同，结果是 `Array<T>`（没有元素时需要上下文，例如 `(the Array<f64> #2A(()))`）。作为 S 表达式数据，它是 `Array<Option<Sexpr>>`——即 `Sexpr` 的 `array` 变体。
- **空列表 `()`**：根据上下文，是 `Unit` 类型的值，或者是 `Option<Sexpr>` 的 `none`。**`Sexpr` 没有空列表的变体**——`Sexpr`
  表示"非空的 S 表达式"，S 表达式数据的类型是 `Option<Sexpr>`（见 [4.3 match](#43-match--模式匹配) 的"`Option<Sexpr>` 的模式"）。
- **quote/quasiquote/unquote**：
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)`（只在 quasiquote 中有意义）
  - `,@x` → `(unquote-splicing x)`（展开时作为列表元素拼接）
- **路径 `::`**：`foo::bar` 读作经由模块、类型、成员的路径（不会成为一个符号名）。像 `::foo` 这样以 `::` 开头时是从根开始的
  绝对路径。泛型参数内部的 `::`（`Vec<a::b>` 等）不当作路径分隔符。

## 2. 类型的写法

在源代码中，类型写成普通的符号或列表。

- **基本类型**：`int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string` `symbol`。
  `int` 是整数（CL 的 integer——在 63 位立即值与多精度之间自动转换，[数值](functions/numbers.md#3-任意精度整数-int)），6 种
  固定宽度类型以宽度和符号命名（没有 64 位整数类型——见[数值](functions/numbers.md#1-固定宽度整数)）。
- **有理数类型**：`ratio`（既约有理数）。遵循 CL 在堆上分配，与 `int`/`f64` 等之间没有隐式转换（用 `as`/`try-as` 或转换方法
  显式转换。见[数值](functions/numbers.md#5-有理数-ratio)）。
- **C 边界上的原始字**：`ptr`（不透明指针）、`c-long` / `c-ulong`。只用于 FFI，成为值需要 `(unsafe ...)`，可以出现的位置也
  有限（[3.3 defffi](#ptr--c-long--c-ulong--原始机器字)）。想要 64 位整数时不要用它们——它们没有算术。
- **不透明的可变类型**：`random-state`（随机数生成器的状态）。不能放进 `Vector<T>`/`HashTable<K,V>`/`Sexpr`（可以放进
  `Option<T>`/`Result<T,E>`）。
- **Unit 类型**：`()`
- **Never 类型**：`!`（`panic`/`unreachable`/`todo`/不返回的循环等发散表达式的类型。适合任何期望类型）
- **函数类型**：`(fn (参数类型...) 返回值类型)`。带可变参数的函数类型为 `(fn (参数类型... &rest 元素类型) 返回值类型)`。
- **泛型类型**：`Name<T1,T2,...>`（作为不含空白的一个记号读取）。
  例如：`Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`。
  类型参数中也可以写 unit 类型 `()`（`Result<(), FileError>`）。`(`/`)` 本来是切分记号的分隔符，但只在尖括号打开期间，这一对
  字符可以通过。`()` 也可以作为字段类型、参数类型使用。
- **泛型类型的应用形式**：`(Name T1 T2 ...)` —— 指代与 `Name<T1,T2,...>` 相同类型的列表写法。例如 `(vector char)` 与
  `Vector<char>` 相同。
  名字形式是通常的写法，这种形式**是为类型参数无法写进名字的情况准备的**——类型参数本身是类型表达式，但在一个记号的名字中只能
  写名字、`()` 和 `:dyn`，不能写函数类型（不存在 `Vector<(fn (i32) i32)>` 这种写法）。实现显示类型时也可能以这种形式出现，
  例如把 trait 的关联类型代入签名的结果。
- **限定类型名**：可以像 `module::Type` 这样用 `::` 限定。
- **trait 对象类型**：`:dyn Trait`（以空白分隔的两个词构成一个类型）。表示具体类型在运行时确定的值，trait 方法的调用经由
  vtable 进行动态分派。有关联类型的 trait 按声明顺序以位置固定（`:dyn Iter<i32>` 把 `Item` 固定为 `i32`）。也可以写在泛型参数
  内部：`Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`。具体值在期望位置自动装箱，显式形式为 `(as :dyn Trait 式)`。
  `:dyn Sub` 的值可以直接传给要求其超 trait（传递继承的所有 trait）的 `:dyn Super` 的位置（向上转换）。不能传给没有继承关系的
  trait。可以成为 `:dyn` 的 trait 的条件见 [3.9 deftrait / impl](#39-deftrait--impl--trait-机制)。在类型位置以外写 `:dyn` 是错误。
- 内置泛型类型：`Option<T>`（`Some(T)` / `None`）、`Result<T,E>`（`Ok(T)` / `Err(E)`）、`HashTable<K,V>`、`Vector<T>`，以及
  并发机制的 `Task<T>` / `Thread<T>` / `Chan<T>`（[第 12 章](#12-并发任务)）。还有 S 表达式数据的类型 `Sexpr`。内置的具体错误
  类型有 `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` / `FileError` / `NetError`，标准库的结构体有
  `SimpleError` / `WrappedError`（`Error` 不是类型而是 trait——作为 `:dyn Error` 使用）。一览见 [types.md](types.md)。
- **类型与 trait 位于同一命名空间**（与 Rust 相同）：在同一模块中，类型（`defstruct`/`defenum`）与 trait（`deftrait`）不能同名。

## 3. 顶层定义

### 3.1 defun — 函数定义

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- 参数类型和返回值类型是必需的。
- 泛型函数在名字后用尖括号写类型参数：`(defun name<T1,T2...> (params) Ret body...)`（与类型位置的 `Vector<T>` 相同的尖括号语法）。
- `defun`/`lambda`/`defmethod` 在末尾写 `&rest (name Type)` 可以接受可变参数：
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)`（在函数体中 `xs` 总是绑定为 `Option<Sexpr>`——S 表达式的列表。调用方
  的每个实际参数各自作为 `Type2` 进行类型检查后再包装进 `Sexpr`）。
  `defmacro` 也有自己的 `&rest`，但不同之处在于它总是无类型的 `Sexpr`（`defun`/`lambda` 明确写出元素类型）。`fn` 类型也可以写成
  `(fn (T1... &rest Te) Ret)` 的形式来表示可变参数函数的类型。
- **`&optional` / `&key`**（用于 `defun` 和 `defmethod`。`lambda`/`labels` 由于下述原因不在对象之内，`defmacro` 是下述的另一种
  实现）。顺序遵循 CL 为 `必需 &optional &rest &key`。每个参数写成 `(name Type)` 或 `(name Type 默认式)`：

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; 没有默认值
    (match suffix ((some s) (append name s)) ((none) name)))         ; 函数体中是 Option<string>

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; 有默认值
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; 调用方写 `:name 值`，顺序任意。省略的取默认值
  ```

  - **没有写默认式的参数的类型是 `Option<Type>`。** 省略时为 `none`，传入时调用方写的裸值自动包装为 `some`。CL 中"通过
    supplied-p 变量得知是否省略"的东西，在这里体现在静态类型一侧。
  - 写了默认式时，类型保持声明的 `Type`。省略时，那个**经过检查的式子**原样嵌入调用方（每次调用都求值）。
  - **`&key` 不能与 `&optional`/`&rest` 混在同一个参数列表中。** CL 本身有一个歧义（末尾的实际参数是由按位置填充的 `&optional`
    接收，还是由按标签匹配的 `&key` 接收，取决于*值*），通过禁止这种组合来回避。`&optional` 与 `&rest` 可以一起使用。
  - 泛型函数中也可以使用，但**只出现在被省略参数中的类型参数无法推断，是错误**（那里没有可以对照的值）。
  - **`defmethod` 也可以写同样的 3 个区段**（实例方法和静态函数都可以）。在接收者之后排列 `&optional`/`&rest`/`&key`：

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; 静态函数
    (point::origin :y 7)
    ```

    泛型类型的方法中也可以使用，但**写了默认式的参数的类型中不能写所有者的类型参数**（与 `defun` 对自身类型参数的限制相同。
    省略时嵌入的是*经过检查的*式子，其类型不能停留在抽象的变量上）。
  - **trait 的方法中不能使用。** `deftrait` 一侧没有这种语法，如果只有 `impl` 一侧能声明区段，那么以 `:dyn` 为接收者的调用
    （从 trait 的声明填充参数）与以具体类型为接收者的调用（从 `impl` 的声明填充）就会成为不同的东西。vtable 槽的参数个数是固定的。
  - **`lambda` / `labels` 中不能使用**（可以使用 `&rest`）。要填补被省略的参数，调用方需要读取**被调用方经过检查的默认式**，
    而它只能从按名字解析的签名中得到。`lambda` 作为值传递，描述这个值的只有函数类型 `(fn ...)`——那里没有放式子的地方，如果放了，
    "签名相同而只有默认值不同的两个 lambda"就会成为不同的类型。`&rest` 只涉及类型的问题，所以可以写进函数类型。
- **前向引用用 `defsignature` 声明**（见下文）。没有声明的名字不能在定义之前调用——因为顶层是按源代码顺序逐个形式检查和执行的。
- 要求 trait 约束时，在函数体之前写 `where` 子句：`(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  （用 `(AssocName ConcreteType)` 固定关联类型可以省略）。
- **文档字符串**：在 `where` 子句（如果有）之后、函数体开头放置字符串字面量，它就成为文档字符串（遵循 CL）。但仅当其后至少还有
  一个函数体形式时——单独的字符串仍是返回值，不会被视为文档字符串：`(defun f () string "doc" "value")` 带有文档字符串并返回
  `"value"`，而 `(defun f () string "value")` 没有文档字符串并返回 `"value"`。可以用 `(documentation name)` 取出
  （[文档字符串](functions/system.md#7-文档字符串--documentation)）。

### 3.2 defsignature — 前向声明

```lisp
(defsignature name (参数类型...) 返回类型)
(pub defsignature name (参数类型...) 返回类型)
```

要调用在自己**之后**定义的 `defun`，先这样声明。相互递归只能这样写：

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

参数**只列出类型**。没有函数体，所以没有需要命名的对象。`&rest` 可以在最后写成 `&rest 元素类型`。

声明**会被检查**：

- 后面的定义必须与声明一致（参数的个数和类型、返回类型、`&rest`、是否 `pub`）。不一致时在定义处报错。
- 声明了而不定义是错误（在文件／模块读取完毕时报告）。REPL 不会在每次输入后报告——因为声明和定义应该可以分在不同的行输入。
- 放在定义**之后**的声明是错误，因为它什么也做不了。

有 3 种东西不能声明：

- **泛型函数。** 生成每种类型的实体需要函数体，而声明没有函数体。前向调用即使能解析，实例化也会失败，所以在声明时就拒绝。
- **`&optional`/`&key`。** 它们的签名包含每个默认值**经过检查的**式子（省略参数时原样嵌入调用方），而声明没有地方放它。
- **`defun` 以外的东西。** `defmacro` 展开时需要宏函数体**已经执行过**，注册签名无法替代。类型（`defstruct`/`defenum`/
  `deftrait`）的注册是"注册类型的代码本身所需要的东西"，不像签名那样自成一体。`defmethod` 注册到所属类型上，所以随类型而定。

CL 中对应的是 `(declaim (ftype (function (i32) bool) even2))`，但那伴随着整套声明系统，而且只是**建议**。这里是静态类型，
所以声明会被检查。

### 3.3 defffi — 声明 C 函数（FFI）

```lisp
(defffi (名字 "c_symbol") (参数类型...) 返回类型)
(defffi (名字 "c_symbol") (参数类型...) 返回类型 :library "名字")
(defffi 名字 (参数类型...) 返回类型)              ; 名字 = C 的符号名
(pub defffi ...)
```

声明 C 函数，使其可以调用。形式与 `defsignature` 相同——名字、参数类型、返回类型、没有函数体——但没有函数体的含义不同。
`defsignature` 是"以后由自己定义"的约定，而 `defffi` 是"函数体已经由别人写好并编译了"的声明。

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

之所以可以把 typelisp 一侧的名字与 C 的符号名分开写，是因为 typelisp 的标识符通常含有 `-`，而 C 的标识符不能含有。省略 C 名时，
名字直接作为 C 的符号名。

**调用需要 `(unsafe ...)`**（即使是只处理标量的函数）。编译器没有办法确认声明的 C 签名与真正的是否一致，只能相信声明——
`unsafe` 是承担这份责任的标记。预期的写法是只包装一次，做成安全的包装函数：

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; 之后不再需要 unsafe
```

可以书写的类型是 `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()`（void）`string` `ptr` `c-long` `c-ulong`，以及
类型化指针 `(ptr T)`（[见后文](#def-c-struct-与类型化指针--分配-c-结构体)）。

`string` 是 `const char *`。typelisp 的字符串不以 NUL 结尾，自身也可能含有 NUL，所以**传递时复制成 C 字符串**，调用结束后释放。
字符串中有 NUL 时是错误——C 只看到它之前的部分，等于悄悄传了另一个字符串。

**返回时也会复制**，不会释放——C 返回的东西属于 C，可能像 `getenv` 那样指向静态表。返回需要调用方释放的内存的函数（`strdup` 等）
请用 `ptr` 接收并自行释放。

结果指向参数内部的函数（`strchr`、`strstr`）也能正确工作。顺序是先复制再释放参数。

声明为返回 `string` 的函数返回 NULL 时是错误。因为 `string` 没有表示"没有"的值。可能为 NULL 时请用 `ptr` 接收。

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

写了 `:library` 时打开该共享库并在其中查找符号。省略时从**进程自身**（已经链接的所有东西——包括 libc）中查找。`sqlite3` 这样的
短名称按 `libsqlite3.dylib` / `libsqlite3.so` 的顺序查找，含有 `/` 时视为路径。打开的库不会关闭——指向其中函数的代码会继续运行，
所以正确的生命周期只有进程的生命周期。

#### ptr / c-long / c-ulong —— 原始机器字

`ptr` 是不透明指针（`void *`、`FILE *`，或者声明所指的任何东西）。`c-long` / `c-ulong` 是 C 的 `long` / `unsigned long`
（`size_t`、`int64_t`、`intptr_t` 也一样）。

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**不叫 `i64` / `u64` 是有意的。** 本语言没有 64 位整数类型——带标签的立即值只有 63 位（[第 2 章](#2-类型的写法)）。`c-long`
这个名字在说"这是跨越与 C 边界的字，不是本语言的整数"。

**没有算术。** 不能写 `(+ x 1)`。能提供却没有提供，是为了不让无处保存、宽度与其他所有数都不同的值参与计算——与去掉 64 位整数
类型的理由相同。有的**只是转换**：

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; 读取返回的东西
(as int (unsafe (c-strlen s)))               ; 要精确读取用这个（int 不会丢失 64 位）
(try-as i32 (unsafe (c-strlen s)))           ; 询问能否放下
(as c-ulong n)                               ; 从其他整数创建
```

整数**字面量**取期望的类型，所以只是传递的话不需要 `as`：

```lisp
(unsafe (c-malloc 16))                       ; 16 读作 c-ulong
```

超出范围的字面量与其他宽度一样会被拒绝（`(c-malloc -1)` 放不进 `c-ulong`）。

**可以出现的位置有限。** 只有参数类型、返回类型和局部变量。以下都是错误：

```lisp
(defstruct handle (p ptr))          ; 结构体的字段
(defenum maybe (none) (some ptr))   ; 枚举的字段
(defvar (block ptr) ...)            ; 全局变量
(defffi f ((vector ptr)) i32)       ; 类型参数内部
```

理由只有一个：这些都是**槽会给内容加标签**的地方。加上标签，指针的最高位就会丢失——与去掉 64 位整数类型的理由相同，所以即使在
`unsafe` 中也不允许。这不是许可的问题，而是那种表示不存在。

出于同样的理由，它也不能成为被嵌套函数**捕获**的局部变量（被捕获的绑定放进单元，单元会给内容加标签）。这在编译时就能知道，
由 `(compile f)` 报告。

GC 不追踪 `ptr`。它指向堆的外部，这样做是正确的。

有 4 种东西不能声明：

- **可变参数**（`printf`）。可变部分按与固定参数不同的规则传递（在 AArch64 Darwin 上是栈），无法从固定签名正确调用。`&rest`
  会被拒绝。
- **结构体的按值传递和按值返回。** 理由相同（取决于各平台的传递规则）。把可写的类型限定在上面的列表中，使之无法写出。
- **泛型。** C 中没有对应物。
- **与内置同名。** 编译后的调用会按那个名字解析到内置函数，与其悄悄出错不如拒绝。

#### 回调 —— 让 C 回调

在参数类型中写函数类型 `(fn (类型...) 返回类型)`，该参数就成为 C 回调的函数（回调）。

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; 顶层函数
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; 局部函数
```

C 的函数指针只是代码地址，C 只传递声明的参数来调用。没有传递捕获变量的地方，所以**只能传递没有自由变量的函数**，这在类型检查时
检查。

- 实际参数中**直接**写函数名或 `lambda` 表达式。不能传递存放函数的变量——其中是哪个函数，因而是否有自由变量，要到运行时才知道。
- `lambda` 引用其外部的局部变量时是错误。可以引用全局变量和顶层函数。
- 局部函数（`labels`）要求连同它调用的兄弟函数在内都没有自由变量。兄弟函数共享存放捕获变量的地方，所以被调用的兄弟函数的捕获
  也是该函数的捕获。
- 泛型函数的类型由声明的函数类型决定。
- 函数类型中可以写的类型与上面的列表相同。但回调的返回类型不能写 `string`（会把没有人释放的内存交给 C）。`string` 参数会把 C
  传来的字符串复制成 typelisp 的字符串。

C 函数的调用只能写在 `unsafe` 中，所以只有在 `unsafe` 中才能传递回调。

**只有在 typelisp 调用的 C 函数运行期间才能回调。** 从其他地方——没有运行 typelisp 的线程、信号处理函数、用 `atexit` 注册的函数——
调用时，会显示原因并停止进程。

**失败不会越过 C 传播。** 回调中的 panic 或 `throw` 无法越过 C 的栈帧展开（会成为未定义行为），所以向 C 返回 0，在 C 函数返回时
再向调用方重新抛出。从失败到 C 函数返回之间再次被调用时，不执行而返回 0。

回调中需要等待的操作（从空通道 `recv` 等）是错误（[12.6](#126-编译后的代码与任务)）。

重新定义函数后，从下次传给 C 开始调用新的定义。

在 AOT（`compile-file`）中也同样工作。C 调用的入口嵌入可执行文件中。

**不能作为值传递。** 不能把 FFI 声明直接写在 `(map f xs)` 的 `f` 中——函数值是包装定义函数体的闭包，而这个声明没有可包装的
函数体。请用 `lambda` 包装：

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` 也会被拒绝。能显示的是 C 一侧的机器码，那不是这个编译器生成的。`(compile c-abs)` 会成功（什么也不做——
因为已经编译过了）。

**在 AOT（`compile-file`）中也能工作。** C 函数本身由链接器解析。如果有写了 `:library` 的声明，该库会作为 `-l` 加到链接命令行
（重复的合为一个）——不需要给 `compile-file` 加参数。读取源代码的是 compile-file 自己，所以可以从声明中收集。

构建时也会查找符号。声明了不存在的函数时，会在链接错误之前得到点出该名字的错误。

标准库（prelude）不使用 `defffi`。标准库会整个进入每个可执行文件，如果其中有带 `:library` 的声明，连不使用 FFI 的程序也会链接
那个库。

#### def-c-struct 与类型化指针 —— 分配 C 结构体

```lisp
(unsafe
  (def-c-struct 名字 (字段 类型)...)
  ...)
(unsafe (pub def-c-struct ...))
```

声明与 C 布局相同的结构体。只能写在顶层的 `unsafe` 中（那个 `unsafe` 中只能写 `def-c-struct`）。可以在名字之后放置文档字符串。

字段可以写的类型是 `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32` `f64` `bool` `ptr`、类型化指针 `(ptr T)`，以及
其他 `def-c-struct`（按值嵌入）。布局（各字段的偏移、结构体的大小和对齐）按 C 的规则计算（以 LP64 为前提）。可以写指向自身的
字段，但不能嵌入自身。

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x 在 0，y 在 8，大小 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

`def-c-struct` 的名字进入类型的命名空间（同一模块中不能有同名的 `defstruct` 等），但**不是值的类型**。不能写
`(defun f ((p point)) ...)`，它只作为类型化指针所指的对象出现。

**类型化指针 `(ptr T)`** 是指向 `T` 的地址。`T` 是上面字段可写的类型之一。与 `ptr` 一样是原始机器字，可以出现的位置规则也相同
（只有参数、返回类型和局部变量，只在 `unsafe` 中能成为值）。

分配和读写用以下形式书写。都只能在 `unsafe` 中使用。

| 形式 | 含义 |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | 分配 `n` 个（省略时 1 个）`T`。内容填充为 0。返回 `(ptr T)` |
| `(c-ref p i)` | 从 `p` 起第 `i` 个元素的指针。超出分配范围时是错误 |
| `(c-deref p)` / `(setf (c-deref p) v)` | 读取／写入 `p` 所指的标量 |
| `p::field` / `(setf p::field v)` | 读取／写入结构体的字段。读取嵌入的结构体字段得到其地址（`(ptr 内部类型)`） |
| `(as ptr p)` | 忘掉类型变为 `ptr`（为了传给 `qsort` 的 `void *` 之类）。没有反方向的转换 |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**分配的内存在离开分配它的 `unsafe` 时释放。** 拥有者是同一函数中词法上最外层的 `unsafe`。无论正常结束，还是因 panic、`throw`、
`return-from` 离开，都会释放。`lambda` 和 `labels` 的函数是另外的函数，所以 `c-alloc` 需要在其中有自己的 `unsafe`。

因此，类型化指针不能带出分配它的 `unsafe`。以下都是类型检查时的错误。

- 作为 `unsafe` 表达式的值（因而也不能从函数返回）
- 在闭包（`lambda`、`labels`）中捕获
- 传给 `task` / `thread`
- 用 `throw` 抛出

想在 `unsafe` 之外使用值时，在 `unsafe` 中复制到 `defstruct` 或数值后返回。

**不处理 C 一侧分配的内存。** 从 C 作为类型化指针进来的值——`defffi` 的返回值、回调的参数、读取指针类型字段得到的值——会在运行时
检查它是否指向某个存活的 `c-alloc` 分配中该类型的值的位置，不是时为错误。NULL 也是错误。想接收 C 分配的内存或 NULL 时，用无类型的
`ptr` 接收（读不到内容）。

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

回调的参数被检查拒绝时，与回调中的失败一样，在 C 函数返回时传给调用方。

### 3.4 defvar / defparameter / defconstant — 全局变量

```lisp
(defvar (name Type) init-expr)        ; 只在尚未绑定时初始化
(defparameter (name Type) init-expr)  ; 每次都赋值
(defconstant (name Type) init-expr)

; 带文档字符串（与 CL 的 defvar/defparameter/defconstant 相同的顺序：在值之后）
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**`defvar` 与 `defparameter` 的区别在重新加载时体现**（与 CL 相同）。如果该全局变量**已经绑定，`defvar` 连初始化式都不求值**，
所以编辑设置文件后重新读取，会话中修改过的值保持不变。`defparameter` 每次都赋值，所以重新读取后会回到文件中写的值。

类型注解是必需的（不从初始化式推断）。`defvar` 可以修改，`defconstant` 不可以（`setf` 是错误）。

### 3.5 defmethod — 方法定义

```lisp
; 实例方法：可以以 (m obj args...) 的形式调用
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / 关联函数：可以以 (Type::name args...) 的形式调用
(defmethod name (Type (arg Type2) ...) RetType body...)
```

调用方按 `obj` 的静态类型解析方法（单一、静态分派）。可以在与 `defun` 相同的位置、按相同的规则放置文档字符串（`where` 子句之后、
函数体开头，仅当其后还有函数体形式时）。`impl` 中的方法也一样——用 `(documentation Type::method)` 取出。

方法自身的类型参数与 `defun` 一样，用 `<...>` 写在名字里。接收者类型的类型参数（下例中的 `T`）由接收者决定，方法自身的类型参数（`U`）从每次调用的参数推断。

```lisp
(defstruct Box<T> (v T))

(defmethod fmap<U> ((self Box<T>) (f (fn (T) U))) Box<U>
  (Box::new (f self::v)))

(fmap (Box::new 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- 方法自身的类型参数的名字，既要与接收者类型声明的类型参数（`(defstruct Box<T> ...)` 的 `T`）不同，也要与接收者中写的名字不同。
- 接收者类型是泛型时，接收者中要么把它的类型参数全部写成变量（`Box<T>`），要么全部写成具体类型（`Box<int>`）。
- `impl` 中的方法不能添加类型参数，其签名遵循 trait 声明的签名。

### 3.6 defstruct — 结构体（用户定义类型）

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; 泛型（类型参数写在尖括号中）
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- 每个字段是 `(name type)` 或 `(pub name type)`（按字段设置公开，与结构体本身的 `pub` 无关）。末尾再写一个式子，就成为该槽的
  **默认值**（`(x i32 0)`）——见后面的选项列表。
- 自动生成以下内容：
  - 构造函数 `Name::new`（按字段顺序传参）
  - 读取器 `(field-name instance)`，语法糖 `instance::field-name`
  - 设置器 `(set-field-name instance value)`，语法糖 `(setf instance::field-name value)`
- 要让结构体本身 `pub`，像 `(pub defstruct ...)` 这样在前面加 `pub`。
- **类型要在被指名之前定义。** 字段的类型可以写自身（`(next Option<node>)`），但不能写之后定义的类型——类型没有相当于
  `defsignature` 的前向声明。尚未定义的名字，在 `defun` 的参数类型和 `the` 中同样会得到 `unknown type` 错误。因此互相引用的两个
  类型无法写出。
- **类型变量只有写在声明部分的那些。** `defun`/`defstruct`/`defenum`/`deftype` 是名字的 `<T>`，`defmethod` 是接收者的类型
  （`(self box<T>)`，静态方法是 `box<T>`）以及名字中的 `<U>`，`impl` 是对象类型和 `impl<T>`，`deftrait` 是 `Self` 以及 `(type Item)` 的关联类型。
  在其他地方——参数、返回值、函数体中的 `the`/`lambda`——首次出现的名字不会成为类型变量，而是 `unknown type`。
- **文档字符串**：在名字之后、字段列表之前放置字符串字面量，它就成为文档字符串（`(defstruct Name "doc" (field Type)...)`——与
  CL 的 `defstruct` 位置相同）。字段总是 `(name Type ...)` 的形式，不可能是裸字符串，所以没有歧义。用 `(documentation Name)` 取出。

#### 选项列表

在名字的位置写列表 `(Name option...)` 可以指定选项（与 CL 位置相同）。

```lisp
(defstruct (point (:constructor make-point)          ; 关键字构造函数
                  (:constructor at (x &optional y))  ; BOA 构造函数
                  (:copier copy-point))
  (x i32 0)          ; 第 3 个元素是该槽的默认值
  (y i32 0))

(point::make-point :y 7)   ; x 为 0
(point::at 1)              ; y 为 0
(point::at 1 2)
(copy-point p)             ; 浅复制（与 CL 的 copier 相同）
```

- **`:constructor`** —— 生成的是类型的**静态函数**（`point::make-point`），函数体一定是 `(point::new ...)`。`new` 仍是结构上
  唯一的构造函数，这里创建的是它的*调用方式*。可以声明多个。
  - `(:constructor name)` —— 以 `&key` 接受所有槽。**所有槽都需要默认值**（本语言没有相当于 CL"未绑定槽"的东西）。
  - `(:constructor name (slot...))` —— 以位置参数接受所列的槽（顺序任意）。没有列出的槽用默认值填充，所以**需要默认值**。插入
    `&optional` 后，其后的可以省略（同样需要默认值）。
- **`:copier`** —— 生成返回槽值相同的新值的**实例方法**。与 CL 的 copier 一样是浅复制。
- **`:include Parent`** —— 把父的槽列表连接到开头（默认值也继承。父可以在别的文件中）。**不建立类型关系**——子不是父的子类型，
  父的方法不适用于子，也没有连接两者的运行时检查。本语言没有子类型，共同的接口由 `deftrait` 负责。连接的只是槽的*列表*。
- **槽的默认值只由生成的构造函数读取。** 一个 `:constructor` 都没有声明却写了默认值，因为不可能被使用，所以是错误。
- 不加入的选项及其原因：
  - **`:conc-name`** —— 在 CL 中是给访问器加前缀，以避免在一个平坦的函数命名空间中冲突。在这里访问器是按接收者类型分派的方法，
    不会冲突，而且加前缀会破坏 `instance::field`（只知道槽名）。
  - **`:predicate`** —— 在运行时回答"这个值是 `point` 吗"。在这里类型是没有运行时见证的编译时分类，也不存在"可能是 point 的
    未知类型的值"所在的位置（对 `Sexpr` 的 `match` 是封闭的，`:dyn` 不能向下转换），所以生成的谓词只能总是返回 `true`。
  - **`:type` / `:initial-offset` / `:named`** —— 把值的表示换成列表或向量的指定。表示属于编译器，从语言中无法观察。

### 3.7 defenum — 枚举（和类型）

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; 带载荷的变体（位置字段）
  (Variant2)                  ; 不带载荷的变体
  ...)

; 泛型
(defenum Option<T>
  (Some T)
  (None))
```

- 每个变体的形式是 `(VariantName FieldType...)`。字段只能按位置指定（没有名字）。至少需要一个变体，名字不能重复。
- 值的构造与内置的 `Option`/`Result` 一样，用限定名或经由 `use`：`(Name::Variant1 a b)`，或在 `(use Name)` 之后写
  `(Variant1 a b)`。
- 可以用 `match` / `if-let` 拆解。`match` 检查穷尽性（需要覆盖所有变体或有 `_`）：
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- 方法／关联函数与 `defstruct` 一样用 `defmethod`/`impl` 后加。
- 要让枚举本身 `pub`，写 `(pub defenum ...)`。
- **文档字符串**：与 `defstruct` 相同的位置和规则——名字之后、变体列表之前（`(defenum Name "doc" (Variant ...)...)`）。用
  `(documentation Name)` 取出。

### 3.8 deftype — 类型别名

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

把 CL 的 `deftype` 缩小到在静态类型语言中有意义的范围——**是类型的写法，而不是类型**。

- 名字的位置与 `defun` 相同，泛型参数写成 `Name<T,U>`。在使用处需要与声明个数相同的类型参数（多或少都当场报错）。
- 展开发生在**类型解析器内部**。因此下游谁也不知道别名的存在——单态化的键、转储、编译路径，以及**错误消息**，全都显示展开后的
  形式。`(f "x")` 对要求 `meters` 的函数失败时，消息中显示的是 `i32`。
- **不是新类型。** `(deftype meters i32)` 使 `meters` 与 `i32` 成为同一类型，所以混用什么也不会被捕获。要区分请用 `defstruct`。
- **不会成为谓词。** CL 的 `(deftype small () '(integer 0 9))` 表示*值的集合*，由 `typep` 在运行时判断，但在这里类型是没有
  运行时见证的编译时分类，所以限制值的别名没有可限制的对象。
- **不能包含自身。** 别名在书写处展开，所以没有递归的去处。递归的数据类型用 `defstruct`/`defenum` 书写。
- 与类型、trait 共享命名空间（同一模块中不能与 `defstruct`/`defenum`/`deftrait` 同名）。用 `(pub deftype ...)` 公开，用
  `(use m::meters)` 引入。
- **文档字符串**：名字之后、类型之前（`(deftype Name "doc" Type)`）。

### 3.9 deftrait / impl — trait 机制

```lisp
(deftrait TraitName (SuperTrait...)      ; 继承列表是必需的。没有时为 ()
  (type AssocName)                       ; 关联类型（可以多个，可以省略）
  (method-name ((self Self) params...) RetType)          ; 没有函数体＝必须实现
  (method-name ((self Self) params...) RetType body...)) ; 有函数体＝默认实现

(impl TraitName TargetType
  (where (Trait A)...)                   ; 作用于整个 impl 的约束（可以省略）
  (type AssocName ConcreteType)          ; 具体化关联类型
  (method-name (recv params...) RetType body...))
```

通过 `impl`，每个方法都作为 `TargetType` 的普通 `defmethod` 注册。在泛型函数的 `where` 子句中作为 trait 约束引用（见
[3.1 defun](#31-defun--函数定义)）。trait 名也可以写成 `m::Trait` 这样的 `::` 路径。

**继承列表（必需）**：一定写在 trait 名之后。元素是裸 trait 名，或者在该 trait 有关联类型时，写成**固定了所有关联类型**的
`(Trait (Assoc Type))`。

```lisp
(deftrait Eq () ...)                       ; 没有继承
(deftrait Ord (Eq) ...)                    ; Rust 的 trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; 固定关联类型
  (rewind ((self Self)) ()))
```

继承有 3 个效果。(1) `impl Ord X` 要求**先**写 `impl Eq X`（关于书写顺序的规则。是在 REPL 和逐个 `load` 中都能确定地判断的唯一
形式，比 Rust 更严格）。(2) 只要 `(where (Ord T))` 就能调用 `Eq` 的方法。(3) 可以从 `:dyn Ord` 调用 `Eq` 的方法，`:dyn Ord` 的值
可以直接传给要求 `:dyn Eq` 的位置（向上转换）。子 trait 重新声明与父同名的方法，以及从两个父继承同名的方法，都是错误（vtable 的槽
每个名字一个）。菱形继承会合并为一个槽。

**默认实现**：在签名后写函数体，`impl` 省略该方法时就使用它。函数体在写下 trait 的**模块的命名空间**中解析，所以也可以调用该模块
中非公开的函数。有函数体的方法也可以写 `where` 子句和文档字符串。函数体的类型检查**在声明时进行一次**，`Self` 保持为类型变量
（以 `Self: 该 trait` 为约束）（与 Rust 相同）——即使是没有任何 `impl` 省略的默认实现，对任何实现类型都通不过的错误也会在那里被
排除。对 `self` 调用该 trait 自身及其继承来源的方法会借这个约束通过，关联类型固定为自身，所以返回 `Item` 的签名与函数体在不知道
具体类型的情况下进行对照。

**全面实现（blanket implementation）**：把对象设为类型变量，就能一次性为满足约束的所有类型实现。

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; 没有函数体——全部使用默认实现
```

**在具体类型实际使用之前不会生成代码**（每种类型一次，与普通单态化相同的机制）。一个 trait 最多一个全面实现。同一类型有显式
`impl` 时，那个优先。函数体的类型检查与生成是分开的，在声明时**保持对象为类型变量**进行一次（与 Rust 相同）——即使是一次也没被
使用的实现，在声明的约束下对任何对象都通不过的错误也会在那里被排除。约束所允许的调用（`(where (Ord T))` 下的
`(less self other)` 等）与泛型 `defun` 的函数体同样对待，可以通过。

**文档字符串**：`deftrait` 在继承列表之后、项目列表之前放置字符串字面量，整个 trait 就能有一个文档字符串
（`(deftrait Name () "doc" (type ...) (method ...)...)`）。没有函数体的签名不能写文档字符串——末尾的字符串本身会成为默认实现的
返回值，两者无法区分。

标准库提供的 trait：**`Iter`**（`next`／关联类型 `Item`。`doiter`／序列函数的基础）、**`Eq`**（`equals`。`not-equals` 是默认
实现）、**`Ord`**（继承 `Eq`。只有 `less` 必须实现，`less-equal`／`greater`／`greater-equal` 是默认实现）、**`Error`**
（`message`／`source`。统一处理错误类型的 `:dyn Error`）、**`print-object`**（按类型的打印表示）、**`Pathish`**（路径名指定符＝
字符串或 `pathname`）、流的层次 **`Stream`** → **`InputStream`**／**`OutputStream`** → **`CharInput`**／**`CharOutput`** →
**`PeekInput`**。哪些类型实现了哪些 trait 见 [types.md](types.md)，各 trait 的方法见[标准 trait](functions/traits.md)、
[错误类型](functions/option-result.md#3-错误类型与-error-trait)、[print-object](functions/printing.md#5-print-object按类型的打印表示)、
[流](functions/streams-files.md)。为自己的集合类型 `impl` `Iter`，`doiter`（第 5 章）以及 `map`／`filter`／`sort` 等就能直接使用。

trait 的调用默认是**静态的**（按接收者的静态类型解析）。想处理具体类型在运行时确定的值时，使用 trait 对象类型 `:dyn Trait`
（第 2 章），就会经由 vtable 进行动态分派：

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; 一个调用处，每个实现各自的回答
```

能成为 `:dyn Trait` 的只有"所有方法都有 `self` 接收者，除接收者外不使用 `Self`，方法本身既不是泛型也不是可变参数"的 trait
（继承的方法也必须满足同样的条件）。

能放进 `:dyn` 盒子的只有值在堆上有表示的类型：

| 可以放入 | 不能放入 |
|---|---|
| `defstruct` / `defenum` 的类型（包括 `Vector<T>`、`cons-cell<A,B>`、`Result<T,E>` 以及标准库的结构体）、`HashTable<K,V>`、`Sexpr`、`int`、`ratio`、`f64`、`string`、`random-state` | 固定宽度整数（`i8`〜`u32`）、`f32`、`bool`、`char`、`symbol`、`()`、函数类型、没有盒子的 `Option<T>`（[Option 的运行时表示](functions/option-result.md#2-optiont-的运行时表示)） |

把不能放入的类型的值放在 `:dyn` 的位置是类型错误。想用 `:dyn` 处理这样的值时，像 `(defstruct flag (v bool))` 这样用结构体包装。

### 3.10 module / use — 命名空间

```lisp
(module path body...)      ; path 是 foo 或 foo::bar 这样的段序列
(in-module path)           ; 从此到这个单元末尾都在 path 中（module 的平铺形式）
(use path...)              ; 把函数、类型、模块作为别名引入当前命名空间
(import path...)           ; 与 use 相同（CL 兼容的写法）
(shadowing-import path...) ; 明知裸名已被占用仍要取用的 use
```

- `module` 创建命名空间。**类型不是命名空间**（与 Rust 相同，类型只拥有关联函数／方法）。
- 用 `use` 引入类型后，该类型的构造函数和公开的 static 方法也可以用裸名使用（例如：`(use option)` 之后可以不写
  `option::some`/`option::none` 而调用 `some`/`none`）。
- 裸名（没有限定的标识符）的解析顺序：特殊形式 → 构造函数 → 自由函数（当前命名空间 → 根）→ 实例方法（按第一个参数的静态类型
  解析）。不会追溯到中间的父模块。
- 限定路径 `a::b` 按上述顺序解析 `a`，是模块就进入内部，是类型就把最后一段作为关联项解析。
- **`use` 对其后的形式生效。** 文件逐个形式读取，依赖也在检查该形式之前解析，所以在 `(use m)` **上面**写 `m::f` 会得到
  `unresolved path`。请把 `use` 放在文件开头。
- **`use` 可以接受多个路径**（`(use a::f b::g)`）。`import` 是行为相同的 CL 兼容写法。
- **裸名已被占用的 `use` 会被报告。** 裸名的解析先看该模块自身的定义，再看别名，所以 `(defun twice ...)` 之后的
  `(use m::twice)` **什么也不做**。明知如此仍要这样做时写 `shadowing-import`（但它胜不过定义——没有撤销定义的手段。它能胜过的
  只有先前的别名）。
- **`in-module` 是 `(module path body...)` 的平铺形式。** 写 `(in-module geometry)`，从那里到该单元（文件，或外层 `module` 的
  函数体）末尾都在 `geometry` 中。它进入文件自身模块的**内部**（对 `main.typl` 是 `main::geometry`）。连续写两个会依次嵌套。
  它与 CL 的 `in-package` 不同，名字也有意区分——在这个系统中文件已经是模块，没有可"选择"的对象，形式能做的只有嵌套。

### 3.11 文件与模块的对应（多文件项目）

从源代码根目录起的相对文件路径就是模块路径：
`<root>/geo/point.typl` 的内容隐式地包在模块 `geo::point` 中（目录也是一段，Rust/Python 的方式）。文件中显式的
`(module bar ...)` 嵌套在其**内部**（`geo::point::bar`）——推导出的路径与显式声明不会冲突。

- **源代码根目录**：在项目根目录放置清单文件 `typelisp.toml`（可以为空。可选地用 `src = "src"` 一行指定源代码目录）。从对象
  文件所在目录向上查找。没有清单时，入口文件所在目录（REPL 是当前目录）为根目录。
- **按需加载**：`(use geo::point)` 引用尚未读入的模块时，自动读入对应的文件（`geo/point.typl`），进行类型检查并注册。
  `use a::b::c` 按最长前缀的顺序查找 `a/b/c.typl` → `a/b.typl` → `a.typl`（因为 `c` 可能是模块内的项）。从其他模块可见的定义
  需要 `pub`（[3.13 pub](#313-pub--公开)）。
- **循环引用是错误**：以 `circular module dependency: a -> b -> a` 的形式报告链条。
- **运行**：用 `typl <file.typl>` 运行文件（不带参数时为 REPL）。REPL 中的 `use` 也按同样的约定解析文件。
- **cons 区域容量**：用 `typl --heap-cells N` 指定 cons 单元区域的**初始容量**（默认 65536。也可以写 `--heap-cells=N`，文件运行
  和 REPL 都适用）。区域不足时会**追加增长**。增长的上限是初始容量的 256 倍，超出它的分配会成为 `heap exhausted`——也就是说，初始
  容量意味着"最初分配这么多"，上限意味着"超过这里就视为泄漏"。

### 3.12 load — 平铺加载

```lisp
(load "path")   ; 只能在顶层。path 是字符串字面量
```

- CL 式的**平铺加载**：把对象文件的形式**原样读入当前命名空间**（不像 `use` 那样用模块包装）。只能在顶层（在函数体中是类型错误）。
- `path` 相对于读入方文件所在的目录（从 REPL 读入时相对于进程的 cwd）。没有扩展名时补上 `.typl`。
- 读入的文件自身的 `(load ...)`/`(use ...)` 也会递归处理。
- **逐个形式读取，并当场执行**（与 CL 的 `load` 相同）。形式 *k* 在 *k+1* 被读取之前就已经执行完毕——即使中途有语法错误或类型错误，
  之前的形式也已执行。通过 `use` 读入的模块文件与此不同，作为一个单元检查，执行交给 `use` 它的一方（相当于 CL 的 `compile-file`）。

### 3.13 pub — 公开

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

能加 `pub` 的只有上面 11 种（`module`/`use`/`deftrait`/`impl` 不能加）。不是用括号包裹定义形式的 `(pub (defun ...))` 形式，而是在
`pub` 之后紧接定义关键字。一个 `pub` 只能公开一个定义（不能一次指定多个定义）。

### 3.14 defmacro — 宏定义

```lisp
(defmacro name (必需... &optional opt... &rest rest-name &key key...) body...)
```

- 所有参数和返回值总是 `Sexpr`，所以不写类型注解。
- CL 式的非卫生宏（用 `gensym` 避免冲突是宏作者的责任）。
- lambda 列表遵循 CL 的 `必需 &optional &rest &key` 顺序（每个标记至多一次，只能按这个顺序）。
  - `&optional` … 可省略的参数。`name` 或 `(name 默认式)`。默认式在展开时求值（可以引用之前已绑定的参数），省略时绑定
    （不写默认值时为空列表 `()`）。
  - `&rest name` … 把剩余的位置参数合为一个 `Sexpr` 列表接收。
  - `&key` … 关键字参数。`name` 或 `(name 默认式)`。调用方用 `:name 值` 传入（顺序任意）。省略时为默认式（没有时为空列表 `()`）。
    未知的关键字或奇数个的 `:key` 序列是错误。
- 例：`(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`。

### 3.15 macrolet / symbol-macrolet — 局部宏绑定

```lisp
(macrolet ((name (lambda 列表) body...) ...) body...)   ; 词法作用域的宏
(symbol-macrolet ((name 展开形式) ...) body...)          ; 名字代表一个形式
```

两者都是**表达式**的特殊形式，运行时什么也不留下（被编译的是函数体展开后的形式）。lambda 列表与 `defmacro` 相同。详细规则和例子见
[局部宏绑定](functions/system.md#9-局部宏绑定macrolet--symbol-macrolet)。

## 4. 绑定与条件分支

```lisp
(let ((name val) ...) body...)      ; 并行绑定
(let* ((name val) ...) body...)     ; 顺序绑定（前面的绑定可以用在后面的初始化式中）

(if cond then else)                 ; else 是必需的（固定 3 个元素）
(when cond body...)                 ; 没有 else 的 if（Unit 类型）。defmacro
(unless cond body...)               ; when 的否定版。defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; 键列表：匹配其中任一个
  (else body...))                   ; expr 只求值一次。key 用 equal 比较。
                                     ; key 是"字面量"，不求值（与 CL 相同）。
                                     ; 裸符号 a 表示符号 'a。
                                     ; 写 'a 是错误（使用裸的 a）。defmacro
(ecase expr (key body...) ...)      ; 要求穷尽的 case。都不匹配时 panic。defmacro
(ccase expr (key body...) ...)      ; CL 的 ccase。没有可提供的 restart，所以与 ecase 相同。defmacro
(and expr...)                       ; 短路求值。0 个参数时为 true。defmacro
(or expr...)                        ; 短路求值。0 个参数时为 false。defmacro
(progn body...)                     ; 依次执行，返回最后的值
(unsafe body...)                    ; 与 progn 相同。另外给予书写 FFI 调用
                                     ; 和原始字的许可。见 3.3 defffi
(prog1 form more...)                ; 全部求值，值为 form 的。defmacro
(prog2 a b more...)                 ; 全部求值，值为 b 的。defmacro
(the Type expr)                     ; 类型注解（没有运行时效果）
```

### 4.1 unsafe — 承担无法检查的前提

```lisp
(unsafe body...)
```

与 `progn` 相同——依次求值函数体，返回最后的值。不创建作用域，也不是函数边界（`break` / `return-from` 直接穿过到外部）。不同之处
在于有些东西只能写在它里面。

目前要求 `unsafe` 的有 3 种：调用用 [defffi](#33-defffi--声明-c-函数ffi) 声明的 C 函数，把原始机器字（`ptr` / `c-long` /
`c-ulong` / `(ptr T)`）作为值，以及 [`def-c-struct` 与 `c-alloc`](#def-c-struct-与类型化指针--分配-c-结构体)。

用 `c-alloc` 分配的内存在离开同一函数中最外层的 `unsafe` 时释放。只有那个 `unsafe` 与 `progn` 不同，离开时有释放的处理。

`unsafe` 承担的是编译器无法确认的以下前提：

- **类型一致。** 声明的 C 签名与真正的一致。不一致时，参数会放进错误的寄存器，返回值会以错误的宽度读取。
- **内存安全。** C 一侧如何处理传给它的东西。
- **进程全局的状态。** 环境变量、信号处理函数、`errno`。例如通过 FFI 调用 `setenv`，会破坏本实现的 `decode-universal-time` 求本地
  时间时的前提。
- **线程安全。**

它不是逃避类型检查的出口。`(unsafe (+ 1 "two"))` 不会通过。允许的是书写特定的**操作**，而不是书写胡乱的东西。

它在词法上起作用。写在 `unsafe` 中的 `lambda` 的函数体继承这个许可（与 Rust 的 `unsafe` 块中的闭包相同）——这个值以后可能会从
`unsafe` 之外调用，但写在那里本身就被视为承担了责任。

### 4.2 destructuring-bind — 按形状拆解列表

```lisp
(destructuring-bind lambda 列表 form body...)
```

把 `form` 产生的列表**按形状**拆解并绑定。lambda 列表是 `defmacro` 的（必需 → `&optional` → `&rest`/`&body` → `&key`，各自带默认
式），理由与 CL 让两者共用一个相同——它们是拆解同一种东西的两种形式。

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **绑定的变量全部是 `Option<Sexpr>`。** 这不是实现的限制，而是被绑定对象的性质：S 表达式列表是本语言中唯一的列表，没有其他可以给
  元素的类型。在需要标量的地方转到 `match`，与 `defmacro` 的函数体相同。
- **形状不符时 panic**（相当于 CL 的错误）。元素不足、过多，`&key` 的序列为奇数个，未知的关键字，都是如此。`sexpr-car` 对 `()`
  返回 `()`，是宽松的函数，所以不写检查的话，短的列表会悄悄绑定成空序列。
- **不支持嵌套的 lambda 列表。** `defmacro` 也不接受，以保持规则唯一。`(a (b c))` 不会悄悄把子列表绑定到 `b`，而是得到说明这一点的
  错误。
- `&optional` / `&key` 的默认式**只在使用时求值**（与 CL 相同）。
- 没有相当于 CL 的 `&allow-other-keys` 的东西（`defmacro` 也没有）。

### 4.3 match — 模式匹配

```lisp
(match expr
  (pattern body...)
  ...)
```

模式的种类：
- `_` —— 通配符
- 变量名 —— 绑定模式（总是匹配）。但如果被匹配值的类型有该名字的变体，就解析为**下面的裸变体名模式**
- 裸变体名 —— 匹配不带参数的变体（`(match c (red 1) (blue 2))`）。用裸名写带字段的变体会得到参数个数错误，所以像 `(circle r)`
  这样用括号写
- **立即值字面量**：整数 / `true`/`false` / 字符 —— 按字比较
- **值字面量**：字符串 / 浮点数 / 符号（`'foo`）/ 多精度整数 / ratio —— 用该类型的 `Eq::equals`
  （[标准 trait](functions/traits.md#2-eq--ord比较)）按值比较。字符串比较的是内容，而不是同一性
- `(= expr)` —— 求值任意式子，用 `Eq::equals` 比较。是比较没有字面量语法的类型（`defstruct` 实例、全局变量、计算结果）的唯一写法，
  用户定义的 `Eq` 实现直接成为比较规则。`expr` 可以引用从该分支位置可见的任何东西（参数、外层绑定、全局变量）
- `(Ctor sub-pattern...)` —— 构造函数模式（`Some x` `None` `Cons a d` `Ok v` 等）

用值字面量／`(= expr)` 比较没有实现 `Eq` 的类型是类型错误（比起留下悄悄不匹配的分支，选择说明无法比较）。

**对 `Sexpr` 被匹配值的值字面量**：`sexpr` 的 `Eq` 是 `eq`（CL 的同一性），所以立即值——`'foo`（已 intern）/ 整数 / 字符 /
`true`/`false`——可以直接写，按内容匹配：

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

不是立即值的字面量（字符串 / 浮点数 / 多精度整数 / ratio）**不能**对 `Sexpr` 书写。它们的 `eq` 比较对象的同一性，会成为"类型能
通过但永远不匹配的分支"，所以作为点出变体模式的错误——写 `(str "hi")` 就会拆解为 `string` 进行内容比较。`(= expr)` 明确要求
`equals`，所以不受这个限制。

**被匹配值不必是 ADT。** `string`/`symbol`/`i32`/`f64` 等可以直接 `match`（那正是字符串字面量模式的用武之地）。但没有变体的类型
无法通过枚举穷尽，所以 `_`（或者作为通配符的绑定模式）是必需的：

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; 没有变体的类型，所以需要 `_`
```

对 `Sexpr` 被匹配值，除了上面的内置 18 种变体模式，还可以写**向下转换模式**（取出用户定义 ADT 的实例）——用 `match` 取回像
`(list p 42)` 这样隐式转换为 `Sexpr` 的 `defstruct`/`defenum`（第 3 章）实例的语法：

- `(TypeName sub-pattern...)` —— 把**类型名**放在开头的字段拆解（只用于 struct，`defstruct` 总是只有一个变体，所以用类型名而不是
  变体名书写）。例：对 `(defstruct point (x f64) (y f64))` 写 `(point x y)`。
- 裸变体名 `(VariantName sub-pattern...)` —— 取出 `defenum` 的变体。作为 `(use EnumType)` 之后可见的裸名解析（与调用构造函数时的
  可见性规则相同）。例：对 `(defenum color (red) (blue))`，在 `(use color)` 之后写 `(red)` `(blue)`。多个可见的 enum 的变体名冲突
  时会得到歧义错误，所以也可以写限定形式 `(EnumType::VariantName ...)`（不需要 `use`）。
- `(the Type pattern)` —— 以整个类型进行向下转换（整体绑定）。不拆解字段，把值原样交给 `pattern`。是在保持可变 struct 同一性的
  同时取出它的唯一写法，也是从 `Sexpr` 取出 `Vector<T>`/`HashTable<K,V>` 的唯一手段（两者没有字段拆解形式）。例：在
  `(the point p)` 之后 `(setf p::x 9)`，也会反映到列表中的原实例。

**`Option<Sexpr>` 的模式**：S 表达式数据的类型不是 `Sexpr` 而是 `Option<Sexpr>`，空列表不是 `Sexpr` 的变体，而是 `Option` 的
`none`。因此 `match` `Option<Sexpr>` 时，`Sexpr` 的 18 种变体和 `none` 可以**平铺在同一组分支中**（不需要剥去 `Option` 的外层
`match`）：

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; 空列表
    (_          9)))
```

穷尽性也在同一个平铺的全集——`Sexpr` 的 18 种变体加上 `none` 共 19 个——中检查。忘写 `(none)` 时，只要没有 `_` 就是错误。也可以写
`(some x)`，绑定"非空的某物"。

这个语法糖**恰好**只适用于 `Option<Sexpr>`。对 `Option<Option<Sexpr>>`，无法确定 `(int n)` 剥去的是哪一层，所以照常写两层 `match`。

**trait 对象（`:dyn Trait`，第 2 章）的被匹配值**也可以直接使用同样的向下转换模式——`match` 先拆开盒子再交给上面的 `Sexpr` 模式
机制，所以没有额外的语法。实现类型的集合是开放的，所以不会穷尽，`_` 是必需的：

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; 类型名在前的字段拆解
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**分支之间的类型推断**：所有分支必须是同一类型（`panic` 等发散的分支除外）。在没有期望类型的位置写的 `match` 中，分支会**互相**
补足缺少的类型参数——`(result::ok v)` 只决定 `T`，`(result::err e)` 只决定 `E`，两者并列就决定了 `Result<T,E>`。到最后仍有任何
分支都无法决定的类型参数时，会成为该分支自身的错误（`cannot infer type argument ...`）。在 `match` 之外，无法决定的类型参数当场就是错误。

使用向下转换模式的 `match` 的穷尽性检查，不计入 `Sexpr` 本身变体的覆盖（只列出向下转换模式的 `match` 需要用 `_` 收尾）。泛型 ADT
（`defstruct point<T> ...` 等）无法推断向下转换模式的类型参数，所以不能使用字段拆解形式（`(point ...)`）／裸变体形式，要像
`(the point<i32> p)` 这样用 `the` 明示。

**向下转换会看到实例化。** 明示的类型参数用于匹配——`(the point<i32> p)` 只放行 `point<i32>` 的值，`point<string>` 会略过到下一个
分支。因为值记得包括自身类型参数在内的类型（与选择 `print-object` 的机制相同）。

```lisp
(if-let (pattern val) then els)     ; val 匹配 pattern 则为 then（带绑定），否则为 els。defmacro
(while-let (pattern val) body...)   ; 在 val（每次重新求值）匹配 pattern 期间循环。defmacro
```

## 5. 迭代

```lisp
(loop body...)                      ; 无限循环。用 break/return 退出
(while test body...)                ; test 为真期间循环。defmacro
(until test body...)                ; test 为假期间循环（while 的否定版）。defmacro
(dotimes (var count-expr) body...)  ; 求值 count-expr 一次，让 var 遍历 0..count-1。defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; CL 式的并行步进迭代。defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; do 的顺序版（let* 绑定、依次赋值）。defmacro
(doiter (var coll-expr) body...)    ; 遍历实现了 Iter trait 的值。defmacro

(break)                             ; 只退出最内层的循环。值总是 Unit
(return)                            ; 只退出最内层的循环
(return value)                      ; 带值退出最内层的循环
```

`break`/`return` 都**只退出最内层的外围循环**（不是函数的提前返回。不能越过 `lambda` 的边界）。`loop` 的类型是内部找到的
`break`/`return` 的值类型的合并类型（一次也不退出时为 `!`）。要退出函数，使用下面的 `return-from`。

### 5.1 `block` / `return-from` — 具名退出

```lisp
(block name body...)                ; 具名的退出目标。值为最后的形式，
                                    ; 或者 return-from 传来的值
(return-from name)                  ; 以 Unit 退出该 block
(return-from name value)            ; 带值退出
```

**`defun` / `defmethod` / `labels` 的每个函数都隐式地建立以自己名字命名的 block**（与 CL 相同）。所以 `(return-from f v)` 就是
函数的提前返回：

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` 是**词法的**退出，名字**在书写处解析**——检查器把 `return-from` 对应到外围的 `block`，把其值的类型合并进该块的退出类型。因此：

- 没有对应 `block` 的 `return-from` 是**类型错误**（不是运行时错误）。
- 值的类型与其他退出或函数体的类型不符时是**类型错误**（与 `match` 分支的规则相同）。
- 同名的 `block` 嵌套时**内层胜出**（CL 的遮蔽规则）。
- **不能越过函数的边界。** 不能从 `lambda` 内部退出到外部的 `block`（`lambda` 不建立 block——CL 的隐式 block 要求*名字*，而匿名
  函数没有）。需要越过的用 `catch`/`throw`（第 8 章，那是**动态的**）。

与 `break`/`return`（第 5 章）一样是**静态的**退出，所以在编译后的代码中是跳转到编译时就确定的基本块。中间有 `unwind-protect` 时，
其 `cleanup` 会执行（第 8 章）。

一次也不写 `return-from` 的话，隐式的 block 没有任何开销。

### 5.2 扩展 `loop`（CL 的 LOOP）

`loop` 的**第 1 个元素是关键字时**，作为子句序列读取。否则仍是上面的简单循环，已经写好的 `loop` 的含义不变（与 CL 自身的 simple
loop 规则相同）。

CL 用裸符号书写子句词（`(loop for i from 1 to 3 collect i)`），但这里**全部是关键字**——裸的 `for` 只会成为变量引用，而是不是
关键字也正是与简单循环的分界。例外是分隔变量和值的 `=`，它的位置是唯一的，所以裸写和关键字（`:=`）都能读取。

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**变量子句**（写在主体子句之前。这是 CL 的规则：写在后面会被读成"只从那里开始循环"，所以是错误）：

| 子句 | 含义 |
|---|---|
| `:with v = e` | 只绑定一次。可以读取前面子句的变量 |
| `:for v :in s` / `:for v :across s` | 依次取 `Iter` 的元素。这里没有 CL 的列表／向量之分，所以是同一子句的不同写法 |
| `:for v :on s` | 依次取之后的**后缀**。CL 传递共享的尾部 cons，但 `Iter` 没有可共享的尾部，所以是新的 `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | 计数。也可以用 `:downfrom`/`:upfrom` |
| `:for v = e [:then f]` | 从 `e` 开始，第 2 次以后为 `f`（没有 `:then` 时每次都是 `e`） |
| `:repeat n` | 循环这么多次 |

写多个 `:for` 时**并行**推进，任何一个用尽时结束。

**主体子句**（每次按书写顺序执行）：

| 子句 | 含义 |
|---|---|
| `:do form...` | 用于副作用 |
| `:collect e [:into v]` | 收集到 `Vector<T>` |
| `:append e [:into v]` | 追加 `Iter` 的内容 |
| `:sum e` / `:count e` | 合计 / 为真的次数 |
| `:maximize e` / `:minimize e` | 最大 / 最小。**`Option<T>`**（与 CL 对空序列返回 nil 相同。任意的 `Ord` 类型没有最小元） |
| `:always e` / `:never e` | 全部满足则为 `true`，一旦不满足立即为 `false` |
| `:thereis e` | `e` 是 **`Option<T>`**。返回第一个 `some`，没有时为 `none`（相当于 CL 的"第一个非 nil 值"的就是它。要测试 `bool` 用 `:always`/`:never`） |
| `:while e` / `:until e` | 在这里**正常结束**（`:finally` 会执行，收集的东西就是答案） |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | 让一个子句成为有条件的 |
| `:return e` | 立即以该值退出（`:finally` 不执行。与 CL 相同） |
| `:initially form...` / `:finally form...` | 循环之前 / 正常结束时 |

**`:named name`**（在所有其他子句之前，只能一个）用 `(block name …)` 包围整个循环。`(return-from name e)` 即使从嵌套循环中也能
一口气退出，与 `:return` 一样，`:finally` 不执行。不命名时不建立 block——CL 的无名 `loop` 建立 `block nil`，但这里没有 `nil`，而且
`break`/`return`（第 5 章）已经提供了"退出最内层的循环"。

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

省略 `:finally (return 0)` 是**类型错误**。这只是 `block` 的规则在起作用（5.1）：退出的类型 `int` 与循环用尽时留下的 `()` 不符。

**循环的值**：有累积子句时为其累积（有多个时为第一个），`:always`/`:never` 时为 `true`，`:thereis` 时为 `none`，都没有时为 `()`。
`:finally` 的最后是 `(return e)` 时，它就是值——这是 CL 的 `finally (return …)` 惯用法，是不做累积的循环表明自己答案的唯一方法。

**与 CL 的区别 / 没有加入的东西**：

- **子句词是关键字**（如上）。
- `:maximize`/`:minimize`/`:thereis` 返回 `Option<T>`（因为没有 nil）。
- **只写 `:return` 而既没有累积也没有 `:finally` 是错误。** CL 用尽时返回 nil，但这里没有它，所以循环必须说明用尽时的值。
- 没有加入用 `:and` 连接并行子句、`:being`／哈希表专用的迭代、`:it`、`:nconc`。
- `:collect` 的元素类型由被累积的式子的类型决定。要收集函数类型这类**无法写成类型名的类型**时，会得到说明这一点的错误。

## 6. 函数值与调用

```lisp
(lambda (params) RetType body...)   ; 创建一等函数值（闭包）
(labels ((name (params) RetType body...) ...) body...)   ; 可以相互递归的局部函数定义
(apply f arg1 ... argN rest-list)   ; 展开 rest-list 来调用 f（带 &rest 的可变参数函数）
```

具名函数也可以直接作为值传递（作为高阶函数的参数等）。

## 7. 其他特殊形式

```lisp
(setq var value ...)                ; CL 的变量赋值。只是依次排列 (setf var value)。defmacro
(psetq var value ...)               ; 并行赋值。先求值所有值再赋值。defmacro
(psetf place value ...)             ; 把 psetq 推广到 place（同样的展开）。defmacro
(setf place value)                  ; 对 place 赋值。place 是变量名 / var::field /
                                     ; (accessor recv key...) 形式的调用。recv 的静态
                                     ; 类型有名为 set-{accessor} 的实例方法时
                                     ; 成立（Vector<T>・HashTable<K,V> 的 get 例外地
                                     ; 对应 set，其他为 set-访问器名）。
                                     ; 值是赋予的值（与 CL 相同）。因此
                                     ; (if c (setf x 1) ()) 中 then 与 else 的类型不符
(incf place)  (incf place delta)    ; place += delta（省略时 delta=1）。结果与 setf 相同
(decf place)  (decf place delta)    ; place -= delta（省略时 delta=1）
(rotatef place1 place2 ... placeN)  ; 循环移位 N 个 place（新 place1=旧 place2, ...,
                                     ; 新 placeN=旧 place1）。每个 place 的子式只求值一次
(shiftf place1 ... placeN newvalue) ; 把 place2..N 的值左移，把 newvalue 放进 placeN。
                                     ; 返回值是旧 place1 的值
(list e1 e2 ... en)                 ; 展开为 (cons e1 (cons e2 (... ())))。0 个参数时为 ()
                                     ; 各元素隐式转换为 Sexpr（与 CL 的 cons 一样，可以持有任意
                                     ; 值）。标量（int/i32/f64/ratio/char/bool/string/
                                     ; symbol）包装为对应的 Sexpr 变体，defstruct/defenum/
                                     ; Vector<T>/HashTable<K,V> 等原样放入（没有转换的
                                     ; 开销）。&rest/format 的参数也一样。
(source-file)                       ; 读取这个形式的文件名（string）。在检查时
                                     ; 作为常量确定。相当于 CL 的 *load-pathname*，但不是变量
                                     ; ——模块的函数体在检查之后执行，所以"现在
                                     ; 正在加载"靠不住，而在检查时总是已知的。
                                     ; 不是文件的源代码为读取器的称呼（<stdin>/<input>）
(quote datum)                       ; 与 'datum 相同。不求值，作为 Sexpr 数据返回
(quasiquote template)               ; 与 `template 相同。用 ,/,@ 把式子嵌入模板
(documentation name)                ; 以 Option<string> 返回 name（裸名或 Type::method）的文档字符串
(panic message)                     ; message: string。以不可恢复的错误异常终止。类型为 !
(unreachable)                       ; 展开为 (panic "unreachable")。defmacro
(todo)                              ; 展开为 (panic "todo")。defmacro
(as Type expr)                      ; 数值/字符的类型转换。可能失败的转换失败时 panic
(try-as Type expr)                  ; 与 as 相同，但以 Option<Type> 返回结果（失败时为 None）
(print control args...)             ; 展开格式写到标准输出（不换行）
(println control args...)           ; 同上（末尾换行）
(format dest control args...)       ; CL 的 format。返回展开结果的 string
(pprint x)                          ; 用 pretty printer 美化输出。遵循 CL 先输出换行
(pprint-fill x)                     ; 填充布局
(pprint-linear x)                   ; 全部一行或每行一个元素
(pprint-tabular x [colinc])         ; 表格布局（默认 16 列）
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; 自己构建逻辑块
```

`print`/`println`/`format`/`pprint` 系列是特殊形式，所以可变参数（`pprint` 系列是一个对象）以各自的类型包装进 `Sexpr` 传递——
`(println "~a" my-struct)` 能直接工作就是这个原因。格式指令和 pretty printer 的详情见[格式指令](functions/format.md)和
[打印](functions/printing.md#4-pretty-printer)。

`as`/`try-as` 能处理的只有数值和字符的目录（`int`、固定宽度整数类型、`f32`/`f64`/`ratio`/`char` 之间）。同一类型不转换。**整数
宽度之间（包括 `int`）以及 `f32`↔`f64` 是真正的转换**——`as` 截断／舍入，`try-as` 回答能否放进该宽度（精度）。`(as int x)` 是
从固定宽度的精确扩展，`(as i32 n)` 是从 `int` 的截断。整数→`char` 可能因超出范围而失败，所以 `as` 会 panic，`try-as` 为 `None`。
其他（扩展转换以及 `float->int`/`ratio->int` 的截断）总是成功。`float->int`/`ratio->int`/`char->int` 落到 `int`，要求更窄的宽度
时接着调用 `int->W`。是展开为相应转换方法（[数值](functions/numbers.md)的 `int->char`/`int->int`/`int->W` 等）的语法糖。

`documentation` 与 `quote`/`compile` 一样是不求值 `name`、作为未求值的裸符号/`::` 路径读取的特殊形式。与 CL 的
`(documentation 'name 'function)` 不同，不接受类型参数——按变量→函数→类型→trait→宏的顺序（与把裸标识符作为表达式求值时的优先级
相同）解析 `name`，返回找到的定义的文档字符串（`(documentation Type::method)` 专用于方法）。解析本身失败（没有该名字的定义）是检查时
的错误，有定义但没有文档字符串时为 `Option::none`。全部在检查时作为常量确定——不会发生运行时的查找。不支持带模块限定的自由名
（`mod::name`，`Type::method` 除外）。

## 8. 非局部退出（catch / throw / unwind-protect）

```lisp
(catch 'tag body)                   ; 执行 body。在 body 所能达到范围的任何地方
                                    ; 发生 (throw 'tag v) 时，以该 v 为值
(throw 'tag value)                  ; 退出到最近的动态外围 (catch 'tag ...)
(unwind-protect protected cleanup)  ; 无论以何种方式离开 protected 都执行 cleanup
```

与 `break`/`return`（第 5 章）不同，这是**动态的**退出——`throw` 并不在词法上寻找包围自己的 `catch`，跨越多少层函数都能到达同一
标签的 `catch`。

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; 找不到时照常为末尾的值
```

- **标签只能是字面量符号**（`'done`）。与 CL 不同，不会被求值。
- **标签携带类型。** `'tag` 第一次使用时确定类型，之后同一符号的 `throw`/`catch` 全部与它对照。用别的类型使用是类型错误。
- `throw` 的类型是 `!`（发散）。`(catch 'tag expr)` 的类型是 `expr` 的类型与标签类型的合并类型。
- `unwind-protect` 的值是 `protected` 的值。`cleanup` 的值被丢弃。无论以何种方式离开 `protected`，`cleanup` 都会执行——除了
  正常结束、`throw`、panic，以 `break`/`return`/`return-from` 离开时也会执行。`cleanup` 自身的非局部退出胜过正在进行的退出。
- 嵌套的 `unwind-protect` 从内向外依次执行。退出 `protected` **内部**循环的 `break` 并没有离开 `protected`，所以其 `cleanup` 不执行。

没有采用 CL 的条件（`define-condition`/`handler-bind`/`invoke-restart`）。它们与静态类型不相容，所以可恢复的失败用 `Result` 表示
（第 9 章）。

## 9. 错误处理方针

- 可恢复的失败用 `Result<T,E>` + `match`。不可恢复的失败（bug、不变式被破坏）用 `panic`。
- 没有相当于 `?`/try 的语法。分支用 `match` 明确写出。
- 函数和特殊形式的名字不使用 `!`（破坏性操作）或 `?`（谓词）作为后缀。谓词用 `-p`/`p` 后缀（`zerop` `consp` 等）或前置 `is-`
  （`is-some` `is-ok` 等）命名。

## 10. 编译

```lisp
(compile name)                      ; 把已定义的 defun/方法 JIT 编译为本机代码
(compile-file src-path out-path)    ; 把源文件 AOT 编译为本机可执行文件（跳过末尾的 `(main)`）
(dump path)                         ; 把当前环境（类型信息 + 编译后的函数体）写到一个文件
(disassemble name)                  ; 打印该定义变成了什么（默认是主机的机器码，第 2 个参数为 true 时是 LLVM IR）
```

`compile` 是特殊形式，`name` 不被求值，作为未求值的裸符号/`::` 路径读取（字符串是类型错误）。泛型函数不能作为对象——每种类型的
实体在每个使用处生成，不存在单一的编译后函数体。**无法解析的名字是检查时的错误**，不会推迟到运行时（类型存在但没有该方法／类型
和函数都没有／裸的未定义名，分别有不同的消息）。这里的可见性与其他引用同样处理，"存在但从这里不可见"与"无法解析"一样在检查时失败。

被调用者也会被传递编译，所以**（即使间接地）调用了不能编译的东西的函数不能编译**。进程不会崩溃，而是以说明这一点的错误拒绝。所有
内置函数都能编译，所以以这种形式被拒绝的只有调用以下解释器专用操作的函数：

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

解释器专用的是 `compile`/`compile-file`/`dump` 以及 `trace`/`untrace`/`step`/`disassemble`
（[实现工具](functions/system.md#5-实现工具clhs-252)）。与其说它们不能编译，不如说它们是进行编译一方的操作（`dump` 写出的是解释器
的环境本身，而 AOT 可执行文件中没有那个环境。`trace` 看的、`step` 停住的都是正在运行的解释器的调用路径，`disassemble` 使用编译器
本身）。`room`/`dribble`/`ed` 不是这一类，可以正常编译。

**可以**编译的东西：流和文件 I/O、`random`、`gensym`、`symbol->string`/`string->symbol`、`parse-int`/`parse-float`、
`get-universal-time`/`get-internal-real-time`、`exit`、超越函数、位运算、`catch`/`throw`/`unwind-protect`、
`eq`/`eql`/`equal`/`equalp` 全部 4 个（`case` 因此也能对所有类型编译）、包括 `print`/`println`/`format`/`pprint` 和
`pprint-logical-block` 在内的全部打印功能、`read`，以及 `eval`。标准库以已编译的状态附带。

AOT 可执行文件中只包含程序使用的功能。不打印的程序不包含格式引擎，不调用 `read` 的程序不包含读取器，不调用 `eval` 的程序不包含检查器
和解释器。

在命令行中，`typl -c src-path [-o out-path]`（`-c` 也可以写成 `--compile`）做与 `compile-file` 相同的事。省略 `-o` 时，输出为
从 `src-path` 去掉扩展名 `.typl` 的名字。链接到可执行文件的静态库 `libtypelisp_front.a`，默认情况下：发布版构建的 `typl` 在第一次
链接时把内部带有的库写出到 `$TYPELISP_HOME/lib/<构建ID>/`（没有 `TYPELISP_HOME` 时为 `~/.typelisp/lib/<构建ID>/`）并使用它；
调试版构建使用构建 `typl` 处的库。`typl --remove-lib` 删除该 `typl` 写出的库。加 `--others` 删除其他构建 ID 的，加 `--all` 删除所有
构建 ID 的。指定 `typl --lib-dir DIR` 时使用 `DIR` 中的库（对 `-c` 和 `compile-file` 都有效），那里没有时在启动时报错。

### 10.1 转储

```lisp
(dump "session.typld")     ; 写出
```
```sh
typl --image session.typld prog.typl   # 从它启动
typl --image session.typld             # REPL 也一样
```

转储是把类型信息和编译后的函数体放进一个文件的东西。`(dump path)` 写出的是当前会话读入的东西（标准库，或者用 `--image` 传入的
转储）加上**会话自身定义的东西**。所以输出是自成一体的，用 `typl --image` 可以启动同样的环境。会话中 `(compile f)` 过的东西以编译后
的形式写出。

保存的是**定义而不是历史**：

- 会话的顶层表达式（`(println ...)` 等）不包括在内。加载时重新执行会造成麻烦。
- 全局变量恢复为**重新执行初始化式得到的值**，而不是转储时的值。这是与 SBCL 的 `save-lisp-and-die`（原样写出堆）有意的不同，这个
  选择使"无法保存的值"——打开的流、闭包的函数指针、外部内存——这一整类问题都消失了。
- 与 `save-lisp-and-die` 不同，**进程不会结束**。因为写出不会破坏映像。

转储记录了写出它的实现的标准库和编译器的版本。用版本不同的 `typl` 读入时会报错，绝不会悄悄接受。

### 10.2 AOT 可执行文件中的 `eval`

`eval` 对"当前的全局环境"进行类型检查后再求值（[解析与求值](functions/system.md#6-解析与求值)）。那个环境——检查器查找的签名、
类型、宏的表，以及解释器能执行的函数体——**不在机器码中**。编译后的函数只是放在某个地址上的符号，既没有参数的类型，也没有按名字查找
函数体的表。

因此，`compile-file` 只对调用 `eval` 的程序，**在编译时组装出那个环境并写进可执行文件**。格式与转储相同，包含标准库的部分和程序
自身的部分。启动时只做恢复，不会重新读取源代码，也不会重新进行类型检查。不调用 `eval` 的程序什么也不添加。

后果：

- **启动更费时间，可执行文件更大。** 因为包含了检查器和解释器的代码以及环境的快照。堆也取得稍大一些。
- **eval 的形式被解释执行。** 即使 eval 调用程序自身函数的形式，运行的也是快照所持有的解释执行用的函数体。结果相同，只有速度不同。

全局变量的存储与编译后的代码**共享**（同一个槽）。`defvar` 的初始化式由编译后的初始化执行一次，恢复时跳过——以免有副作用的初始化式
执行两次。

`compile-file` 也会读取标准库（把其函数体嵌入可执行文件），所以 `abs`/`gcd` 这样的标准库函数，以及 `(impl print-object ...)`、
`(defmethod print-object ...)` 都可以在 AOT 中使用。

`compile-file` 也接受 `use`（以及 `import`/`shadowing-import`）。入口文件的 `(use m)` 按与 `typl file.typl` 相同的规则查找文件，
找到的依赖文件也会被编译并链接进可执行文件——`main.typl` 用 `(use http)` 读取 `http.typl` 的结构也能直接进行 AOT 编译。入口文件
自身的定义也与 `typl file.typl` 一样放进以文件名命名的模块（`p.typl` 中的 `point` 是 `p::point`）。所以值的显示
（`#<p::point x: 1 y: 2>`）无论用哪种方式运行都相同。

## 11. 读取宏（readtable）

可以从程序中替换读取器**遇到某个字符时做什么**（CLHS 23.1）。

```lisp
(set-macro-character c f)             ; 由 f 读取字符 c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; 由 f 读取两个字符的序列 d s
(get-dispatch-macro-character d s)    ; Option<f>
```

`f` 的类型是 `(fn (string-input-stream char) Option<Sexpr>)`。第 1 个参数是**以尚未读取的文本为内容的流**，第 2 个参数是**触发的
字符**（分派时为第 2 个字符）。返回值成为在那里读到的数据。流是具体类型而不是 `:dyn PeekInput`，是因为读取器传递的总是这一种——
`read-sexpr` / `read-char` / `peek-char` / `unread-char` / `read-delimited-list` 都是 `(where (PeekInput S))`，所以具体类型
可以直接全部使用。

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => 读作 (not (equal 1 2))，即 true
```

读取器**先于内置语法查看宏字符**，所以也可以夺取 `(` 和 `'`。`#` 的子字符的注册优先于内置的 `#b`/`#x`/`#.`。`#` 以外的字符传给
`set-dispatch-macro-character` 也会当场成为分派字符——**没有**相当于 CL 的 `make-dispatch-macro-character` 的东西。注册本身就起到
了它的作用，作为单独的一步保留也没有事可做。

**何时生效**与 `#.`（第 1 章）相同，取决于读取路径：

- REPL 和 `(load ...)` 逐个形式执行，所以可以直接注册**前面形式中定义的函数**。
- 模块文件作为一个单元检查，之后才执行——所以 `set-macro-character` / `set-dispatch-macro-character` 的**只有调用本身会立即执行**
  （相当于 CL 的 `(eval-when (:compile-toplevel) ...)` 的作用）。既然立即执行，**传入的函数在那个时刻必须已经存在**。同一文件中的
  `defun` 还没有运行，所以请用 `lambda` 书写，或使用标准库或已经运行过的东西。对象只是顶层的调用，不会查看 `progn` 或 `let` 内部。

内置的 `read` / `read-from-string` 也会参照 readtable（与 CL 相同）。

**没有的东西**：`*readtable*` 和 `copy-readtable`，以及 `readtable-case`。前两者是因为 readtable **不是值**——作为值它必须是"可以
交给读取器的东西"，但读取源代码的读取器在程序之外，没有可交给的去处。`readtable-case` 是因为第 1 章规定了本语言的读取器总是转为小写
（CL 的 `:downcase`）。


## 12. 并发（任务）

**任务是轻量级线程**（用 Go 来说就是 `go` 语句启动的东西），以协作方式运行（没有抢占）。切换不经过内核，执行状态在堆上而不是机器栈上，
所以任务可以廉价地大量创建。

**任务在多个 OS 线程上同时运行**（多核并行）。线程数由环境变量 `TYPELISP_THREADS` 决定（包括运行 `main` 的线程在内的总数，默认是
机器的并行度）。在 `typl` 中**只有编译过的任务**会在其他线程上运行，解释执行的任务在解释器的线程上运行（12.7）。共享数据要经过
`Mutex<T>` 或 `Chan<T>`——不经过它们的同时读写与 Go 一样是未定义的（12.7）。

词汇中**特殊形式只有 `task` / `thread` / `select` 这 3 个**，其余是普通的函数、方法和宏（[任务与通道](functions/concurrency.md)）。

### 12.1 `task` — 启动任务

```lisp
(task (f arg...))                   ; 返回 Task<T>。T 是 f 的返回类型
```

**只接受调用形式。** `f` 和各 `arg` 都在写 `task` 的地方按书写顺序求值，在新任务中发生的只有**调用**。这与 Go 的 `go f(x)` 规则相同，
也是它接受调用形式而不是 thunk 的原因——thunk 会不求值就捕获参数。

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i 每次当场求值。没有捕获的陷阱

(task ((lambda () ()                ; 想运行任意函数体时调用 lambda
         (println "start")
         (send ch 1))))
```

特殊形式（`if` / `let` / `progn` …）不能直接写在 `task` 之下。

**不能做成函数的原因**：如果写成 `(spawn (lambda () T body...))`，就需要写出 `T`，而 `lambda` 必须有返回类型注解，宏又不知道
`(f a b)` 的返回类型。知道它的只有检查器。

### 12.2 `thread` — 在专用 OS 线程上启动任务

```lisp
(thread (f arg...))                 ; 返回 Thread<T>。T 是 f 的返回类型
(join th)                           ; 等待完成并返回其值（可以多次）
```

形式和求值规则与 `task` 相同（只接受调用形式，`f` 和 `arg` 都在书写处求值）。不同之处在于运行的地方：**为该任务专门启动一个 OS 线程，
只在它上面运行**。它不与其他任务多路复用，所以即使在其中调用会阻塞的 C 函数（`defffi`），停下的也只有那个线程，其他任务照常推进。
其中可以直接使用 `task`・`send`・`recv` 等。

- `Thread<T>` 是 `Task<T>` 的对应物。`join` 与 `wait` 一样停下的是**调用它的任务**，值会被缓存。任务结束时线程也结束。
- panic 的规则与 `task` 相同（整个进程崩溃）。`main` 返回时进程结束。
- 想以函数形式书写时，用 `(Thread::spawn (lambda () T body...))`（Rust 的 `std::thread::spawn`）。也可以传入具名函数。
- **在专用线程上运行的只有编译过的代码。** `typl` 在解释执行中求值 `(thread (f ...))` 或 `Thread::spawn` 时，会当场编译要运行的
  函数（以及从它调用的东西）再运行。不能编译的东西——引用外部局部变量的 `lambda`、结构体的构造等——会在启动线程之前成为与
  `(panic ...)` 同样处理的 panic。引用局部变量的 `lambda`，只要在编译过的函数中创建，就可以传入。

### 12.3 `select` — 同时等待多个通道操作

```lisp
(select
  ((v (recv ch1)) body...)          ; 接收分支。v 绑定为 Option<T>
  ((send ch2 x) body...)            ; 发送分支
  (else body...))                   ; 可以省略。**要写的话放在最后**
```

- **有 `else` 时不会阻塞**（Go 的 `default`）。没有时等到某个分支变得可能。
- **同时有多个可能时随机选择一个**（按书写顺序的话，后面的分支会饿死）。
- 接收分支的 `v` 是 **`Option<T>`**。关闭的通道是"回答"而不是跳过分支的理由，所以在分支中 `match`。
- 类型是**所有分支函数体类型的合并类型**（与 `match` 分支的规则相同）。
- 0 个分支的 `(select)` 是类型错误（不采用 Go 的 `select{}`＝永久阻塞）。只有 `else` 的 `select` 也一样——与直接写函数体相同。

**无论选择哪个分支，通道表达式和要发送的值都从左到右各求值一次**（与 `case` 对键所持的规则相同）。

```lisp
(select                             ; 带超时的接收
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after`（[按时间送达的通道](functions/concurrency.md#5-after--按时间送达的通道)）是"`sec` 秒后送达一个值的通道"，相当于 Go 的
`time.After`。

### 12.4 与其他功能的关系

| 功能 | 与任务的关系 |
|---|---|
| `catch` / `throw` | **不越过任务边界。** 要跳出任务函数体的 `throw` 是 panic |
| `unwind-protect` | 任务自然结束时 cleanup 会执行。**因主任务结束导致的进程结束时不执行** |
| `block` / `return-from` | 是词法的，所以不越过 `lambda` 边界 |
| `panic` | 与 Go 一样整个进程崩溃。`wait` 不会把 panic 作为值观察到 |
| `dlet` | **不是按任务的绑定。** 仍然是"借用全局变量再归还"，所以任务之间会互相干扰 |
| 标准输出 | 所有任务共享。一次 `println` 的输出不会在行中间与其他输出混在一起 |
| `compile` / `eval` | 没有限制。任务中的 `(compile f)` 可以通过 |

### 12.5 切换发生的位置

由于是协作式调度，**只在你写下的地方切换**：`(yield)`、`(sleep ...)`、`(wait ...)`、**需要等待的通道操作**（`send`/`recv`/`select`），
以及**需要等待的套接字操作**（`accept`／`tcp-connect`（包括名称解析）／套接字的读写／`recv-from`，[网络](functions/network.md)）。
套接字全部是非阻塞的，未就绪时只有该任务停下，在 OS 回答已就绪时恢复——与 Go 的 netpoller 形式相同。只有在没有可运行的任务时，
实现才会等待 OS 直到最近的 `sleep` 期限。

能当场得到回答的通道操作——缓冲区有空位的 `send`、有值的 `recv`、`(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`——**不消耗
执行权**。这意味着不会因为读取而被意外打断，与作为 CL 式"让出 0 秒"的 `(sleep 0.0)` 区别对待。

**没有抢占。** 什么都不调用的紧密循环会让其他任务饿死——不过编译过的循环会定期把控制权交给调度器，所以编译过的紧密循环不会让其他任务饿死。

### 12.6 编译后的代码与任务

编译后的代码也能挂起任务。用 `compile-file` 生成的可执行文件也一样，`main` 作为调度器的主任务运行——`task`・`sleep`・`wait`・通道・
套接字等待，全部以与 `typl` 相同的含义工作，`main` 返回时进程结束，其余任务被中断（与 Go 相同）。不会为了调度器而把解释器放进可执行文件。

例外只有"C 的 FFI 回调中"，在那里**需要等待的**操作是错误（比悄悄死锁更友好）——用 `defffi` 传入的函数被 C 调用期间，C 的栈叠在上面，
没有挂起任务、之后再恢复的手段。

以下位置也是在任务中途被调用的函数，却不能挂起：`print-object` 方法、`format` 的 `~/name/`、读取宏、`eval` 内部、AOT 可执行文件的
`defvar` 初始化式。在这些地方，**不等待就能得到回答的操作可以通过**（缓冲区有值的 `(recv ch)`、已收到数据的套接字的 `read-line`、
`(task ...)`、`(yield)` 等），**真正需要等待的操作是错误**（不是当场停止进程，而是作为 `` `recv` cannot block: ... `` 这样与
`(panic ...)` 同样处理的 panic）。

### 12.7 与 Go 的区别

- **在 `typl` 中到其他线程上运行的只有编译过的任务。** 解释器的状态不能在线程之间共享，所以解释执行的 `task` 的任务在解释器的线程上
  运行。编译过的任务也会在调用解释执行的函数值、调用没人编译过的 `:dyn` 方法、调用 `eval`/`macroexpand`/`read` 的时刻**转移到解释器的
  线程，之后一直留在那里**（不会回去）。长时间的处理中途只要接触过一次解释执行的代码，剩余部分就在解释器的线程上运行。
- **`typl` 的工作线程只存活一次顶层求值的时间。** 在 REPL 等待输入期间以及顶层形式之间，其他线程不推进任务（剩下的任务在下次求值时
  接着运行）。求值结束时会等待各线程完成当前的一步，所以如果 `thread` 中有持续阻塞的 C 函数（`defffi`），在它返回之前求值不会结束。
- **工作线程上的打印**：解释执行的 `print-object`／`~/name/` 方法不能在其他线程上运行，所以在其他线程上打印这样的值会成为与
  `(panic ...)` 同样处理的 panic（`(compile T::print-object)`，或者从主任务打印）。
- **数据竞争是未定义的**（与 Go 立场相同）。不经过 `Mutex<T>`・`Chan<T>` 从多个任务修改同一个值的结果没有保证。
- **`task` 返回值。** 与 Go 的 `go` 语句不同，它返回 `Task<T>`，用 `(wait t)` 可以取得结果。
- **没有 nil 通道。** Go 的 fan-in 惯用法（把关闭的通道设为 `nil` 以从 `select` 的分支中去掉）无法写出，所以为每个输入启动一个任务，
  再用 `WaitGroup` 汇合（[WaitGroup](functions/concurrency.md#4-waitgroup--等待-n-个完成)）。这在 Go 中也是推荐的写法，但却是
  **从 Go 转过来的人最先遇到的差异**。
