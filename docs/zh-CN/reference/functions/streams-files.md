<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 流与文件

流的 trait 与方法、具体流类型、文件操作、路径名。网络套接字也是流的一员，见[网络](network.md)。

## 1. trait 层次

CL 用类层次表示的东西，在这里用 **trait 层次**表示。方向（输入／输出）和元素类型都是**静态**确定的，所以不需要在运行时询问
"这个流能读吗"。

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; 字符输入
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; 字符输出
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; 可以退回一个字符的输入
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; 字节输入
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; 字节输出
```

读取字符的函数只要接受 `(where (CharInput S))` 或 `:dyn CharInput`，就能接受任何流类型，无论是内置的还是用户定义的。

## 2. 方法

`CharInput` 的所有方法都有默认实现。实现一方只需写 `read-item`。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | 下一个元素。到末尾时为 `none`。**唯一必须实现的方法** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | 下一个字符 |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | 到下一个换行为止（换行被消耗并去掉）。不以换行结尾的最后一行也会返回 |
| `read-all` | `(read-all s)` | `(S)→string` | 剩余的全部 |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | 只取已经在手边的一个字符。与其等待不如返回 `none` |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | 最多把 `n` 个字符 push 到 `v`，返回实际读到的数量。只有到末尾时才会少于 `n` |

`listen` 在 `InputStream`（`CharInput` 的父 trait）中：

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | 下一次读取能否不等待就得到回答。默认是 `false`——**绝不会成为谎言的一侧**。`true` 是推测，猜错的话 `read-char-no-hang` 会阻塞。所有内置流都已覆盖它。**对于没有覆盖它的用户定义流，`read-char-no-hang` 总是返回 `none`** |

`PeekInput`（继承 `CharInput`）增加了**退回一个字符**。放置退回字符的地方只有流自身才有，所以无法有默认实现，作为单独的
trait。`file-stream`/`string-input-stream`/`standard-stream` 已实现，其他的用 `make-peek-stream` 包装即可得到（第 4 章）。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | 使下一次读取返回 `c`。**唯一必须实现的方法**。与 CL 一样只保证一个字符 |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | 不消耗地查看下一个字符 |

`CharOutput` 同样，实现一方只需写 `write-item`。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | 写一个元素。**唯一必须实现的方法** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | 写一个字符 |
| `write-string` | `(write-string s str)` | `(S,string)→()` | 写字符串 |
| `write-line` | `(write-line s str)` | `(S,string)→()` | 字符串加换行 |
| `terpri` | `(terpri s)` | `(S)→()` | 一个换行（CL 的名字） |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | 不在行首时输出一个换行 |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | 下一个写入的字符是否处于行首。默认是 `false`（即 `fresh-line` 会写换行。不清楚时写比较安全）。所有内置流都已覆盖它 |
| `finish-output` | `(finish-output s)` | `(S)→()` | 送出缓冲区 |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | 依次写出 `v` 中的所有字符 |

`at-line-start` 记得的**只是经由该流写入的内容**。`print`/`println`/`(format true ...)` 不经过 `*standard-output*`
就写到标准输出，所以两者混用时，`(fresh-line *standard-output*)` 的判断不知道 `println` 写过的换行。请统一用其中一种。

`Stream` 是所有流共同的：

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | 是否仍然打开 |
| `close` | `(close s)` | `(S)→()` | 关闭。**GC 不会关闭**，请显式关闭（或使用 `with-open-file`） |

## 3. 具体流类型

| 类型 | 创建方法 | 实现的 trait |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` 是 `direction-input` / `direction-output` / `direction-append` 三个常量。`open-file` 打不开时返回
`Err(FileError)`（不存在的文件是普通的结果，不是 panic）。文件名可以是字符串也可以是 `pathname`（第 9 章的 `Pathish`）。

`(get-output-stream-string s)` 返回写入 `string-output-stream` 的内容并清空。与 CL 一样，`close` 之后也能取出。

**字节 I/O** 使用 `ByteInput`/`ByteOutput`。它们把 `InputStream`/`OutputStream` 的 `Item` 固定为 `int`，与
`CharInput`/`CharOutput` 固定为 `char` 的形式相同。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | 下一个字节。文件末尾时为 `none` |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | 写一个字节。0..255 以外是错误 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | 字符版本的字节版 |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | 同上 |

CL 在**调用**中决定元素类型，如 `(open name :element-type '(unsigned-byte 8))`，而这里元素类型是流的**类型**，所以不同的是
打开它的函数。从字符流读取字节是类型错误（`string-input-stream` 没有实现 `ByteInput`）。用 `unread-char` 退回字符后紧接着
读取字节也是错误。

## 4. 组合流

都是标准库中的 `defstruct`，也可以嵌套。

| 名称 | 形式 | 说明 |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | 写到 `Vector<:dyn CharOutput>` 的全部 |
| `make-two-way-stream` | `(make-two-way-stream in out)` | 从 `in` 读，写到 `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | 从 `in` 读，读到的字符也写到 `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | 依次接续读取 `Vector<:dyn CharInput>` |
| `make-peek-stream` | `(make-peek-stream in)` | 给任意 `:dyn CharInput` 加上一个字符的退回，使之成为 `PeekInput`（供 `read-sexpr` 使用） |

## 5. 宏

| 名称 | 形式 | 说明 |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | 打开→主体→关闭。`Result<主体的值, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | 从字符串读取 |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | 返回写入的内容 |

## 6. 泛型函数与文件操作

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | 全部转送 |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | 剩余的所有行 |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | 读取一个 `Sexpr`（CL 的 `read`）。输入末尾为 `Ok(eof)`，读到时为 `Ok(datum d)`，不是数据时为 `Err`。**消耗结束该 datum 的一个空白字符**（与 CL 相同）。`ReadOutcome` 不是 `Option<Sexpr>`，是为了不让"读到空列表 `()`"与"输入末尾"用同一个值表示 |
| `read-sexpr-preserving-whitespace` | 同上 | 同上 | 同上，但保留空白（CL 的 `read-preserving-whitespace`） |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | 读到 `ch` 为止并组成列表。`ch` 被消耗。输入用尽时为 `Err` |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | 逐行写出 |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | 全部内容 |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 所有行 |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | 写出 |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | 是否存在 |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | 删除、重命名（参数是 `Pathish`） |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | 解析了符号链接和 `.`/`..` 的绝对路径。不存在时为 `Err` |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | 最后修改时间。是**世界时**，所以 `decode-universal-time`（[时间](system.md#2-日期的分解与合成)）可以读取 |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | 所有者的登录名。文件不存在时为 `Err`，所有者的 uid 在密码数据库中没有条目时为 `Ok(none)`——把 CL 区分的两种情况原样区分 |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | 是否为目录。**不存在时也为 `false`**——区分两者用 `probe-file` |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 以 truename（与 `truename` 一样是解析了符号链接的绝对路径）列出内容。没有目标的符号链接不包括在内。`.`/`..` 不包括在内。顺序为 OS 给出的顺序 |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | 连同父目录一起创建。已存在时成功 |

指定文件的参数**都可以是字符串或 `pathname`**——与 CL 的路径名指定符同样处理，不是通过运行时的类型检查，而是通过 `Pathish`
trait 解析（第 9 章）。

`read-delimited-list` 的结束字符**也会结束记号**。只在深度 0 生效，`(1 2]` 中的 `]` 被当作列表自身文本的一部分读取，
报告为损坏的列表。没有 CL 的第 3 个参数 `recursive-p` 的对应物。

## 7. 让自己的类型成为流

只写一个 `write-item`，其余由默认实现提供。也可以放进组合流。

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; 其余方法全部使用默认实现

(write-line (counter::new 0) "four")   ; write-line、terpri、fresh-line 都能工作
```

输入一方也一样，只写 `read-item`。即使是自身没有退回功能的类型，用 `(read-sexpr (make-peek-stream my-stream))` 包装后也能
`read`。

## 8. readtable

| 名称 | 调用方式 | 类型 | 说明 |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | 由 `f` 读取字符 `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | 返回已注册的内容 |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | 由 `f` 读取两个字符的序列 `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | 同上 |

`F` 是 `(fn (string-input-stream char) Option<Sexpr>)`。用法、何时生效以及与 CL 的区别见
[语法参考](../syntax.md#11-读取宏readtable)。

## 9. 路径名 `pathname`

拆分后的文件名。持有以 `/` 分隔的目录成分、名称、类型（扩展名），以及是否从根开始。

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   在最后一个点处切分
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 路径名指定符 trait `Pathish`

在 CL 接受路径名指定符（字符串或路径名）的地方，这里接受 `Pathish`。`string` 和 `pathname` 都实现了它，**所有文件操作都以
泛型方式接受它**，所以 `(open-input "a.txt")` 和 `(open-input p)` 都是普通的调用（没有运行时的类型检查）。字符串一侧的
`namestring` 只是返回自身，所以只要传入字符串，就不会进行解析。

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | 字符串表示。必须实现 |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | 转换为 `pathname`（CL 的 `pathname` 函数。与类型名冲突，所以改了名）。必须实现 |

### 9.2 函数

| 名称 | 形式 | 类型 | 说明 |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | 拆分字符串。末尾的 `/`（或空名）表示"没有名称"＝目录 |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | 只用给出的成分组装（全部是 `&key`）。省略的名称、类型保持"没有"，是 `merge-pathnames` 要填补的对象 |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | 从外到内的目录成分 |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | 去掉类型的名称。目录时为 `none` |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | 最后一个点之后的部分。开头的点不算（`.gitignore` 整个是名称） |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | 是否从根开始 |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | 主目录。没有 `$HOME` 时为 `none`（CL 也允许 `NIL`） |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | 到最后一个 `/` 为止的部分 |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | 只有 `name.type` 部分 |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | 用 `default` 补全 `p` 中没有的成分。相对的 `p` 放在 `default` 的目录下，绝对的 `p` 保持自己的目录 |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | 以 `default` 为基准的相对表示。不在基准之下时为 `p` 的全部 |

类型参数都带有 `(where (Pathish P))`。

## 10. 与 CL 的区别

- **是 trait 层次而不是类层次。** 没有 `input-stream-p` / `output-stream-p`——方向由类型持有，不是在运行时询问的问题。
- **`read` 的字符串版和流版名字不同。** `(read "...")`（相当于 CL 的 `read-from-string` 的第 1 个值。还需要读取结束位置时
  用 `read-from-string`）和 `(read-sexpr s)`（CL 的 `read`）。调用解析到唯一的接收者类型，所以不能同名重载。
- **退回是单独的 trait**（`PeekInput`）。为了不强迫只需要 `read-char` 的类型实现 `unread-char`。
- **关闭是显式的。** GC 不会关闭流（GC 何时运行无法预测，交给它的话关闭的时机也无法预测）。使用 `with-open-file` 是安全的。
- **路径名没有主机、设备、版本成分。** 没有通配符路径名，也没有逻辑路径名（`logical-pathname`）。分隔符固定为 `/`。
- **`pathname` 函数是 `to-pathname`。** 因为类型、trait 和函数共享同一命名空间。
- **没有通配符匹配**，所以 `directory` 只是"列出该目录的内容"的函数。CL 的 `directory` 会与路径名模式进行匹配。
