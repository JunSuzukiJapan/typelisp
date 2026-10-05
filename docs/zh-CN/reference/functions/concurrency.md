<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 任务与通道

任务（轻量级线程）的词汇。启动它们的 `task`・`thread` 和多路等待的 `select` 是特殊形式，见
[语法参考](../syntax.md#12-并发任务)。本章是其余部分——类型、方法和函数。

任务是**协作式**的，一个任务只在你写下的地方切换。任务在 `TYPELISP_THREADS` 个 OS 线程上同时运行（在 `typl` 中只有编译过的
任务才会到其他线程上运行）。在哪里切换、与 Go 的区别见[语法参考 12.5](../syntax.md#125-切换发生的位置)和
[12.7](../syntax.md#127-与-go-的区别)。

## 1. `Task<T>` — 任务句柄

| 名称 | 用法 | 类型 | 含义 |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | 等待完成并返回其值 |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **可以 `wait` 任意多次**（值会被缓存）。与 Rust 的 `JoinHandle::join` 不同，不会消耗句柄，可以从多处等待。
- **即使不 `wait`，任务也会运行。** 丢弃句柄也不会让它停止。
- 它是普通的值，所以也能放进 `Vector<Task<()>>`。
- **主任务结束时进程结束**（与 Go 相同）。其他运行中的任务被中断，`unwind-protect` 的 cleanup 不会执行——因为这是进程结束，
  不是栈展开。

## 2. `Chan<T>` — 通道

| 名称 | 用法 | 类型 | 含义 |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | 容量为 `n` 的通道。`0` 是会合（无缓冲） |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | 等到有空位再交出 |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | 等到有值到达。关闭且为空时为 `none` |
| `close` | `(close ch)` | `(Chan<T>)→()` | 关闭 |
| `len` | `(len ch)` | `(Chan<T>)→int` | 当前缓冲区中的个数 |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | 容量 |

**类型参数用 `the` 确定**（与 `(the Vector<i32> (Vector::new))` 的写法相同）。**容量必须写出**——Go 分别写成
`make(chan int)` 和 `make(chan int, 16)` 的两种情况，在这里写成 `(Chan::new 0)` 和 `(Chan::new 16)`。

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go 的 for v := range ch
```

- **`Chan<T>` 是它自己的迭代器**（实现了 `Iter`）。`recv` 返回 `Option<T>`，与 `Iter::next` 同型，所以 `doiter` 以及
  `map`/`filter`/`foldl` 都可以直接使用。
- **向已关闭的通道 `send` 会 panic**，**第 2 次 `close` 也会 panic**（都与 Go 相同。这是程序的 bug 而不是可恢复的失败，所以
  不用 `Result`）。
- **关闭有任务正在等待 `send` 的通道，该任务会 panic**（Go 的规则）。
- 从已关闭的通道 `recv`，缓冲区中有剩余就返回它，变空之后一直返回 `none`。
- `close` 按接收者的类型解析，所以与 `Stream` trait 的 `close` 是不同的东西。`Chan<T>` 没有实现 `Stream`。
- **负的容量会 panic**（不会悄悄舍入为 0）。

## 3. `yield` / `sleep` — 让出

| 名称 | 用法 | 类型 | 含义 |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | 放弃本轮剩余的执行权（Go 的 `runtime.Gosched`） |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | **只让该任务**停下。其他任务继续运行 |

`sleep` 停下的是任务而不是线程。只有当没有任何任务可运行时，才会进入 OS 的 `sleep` 直到最近的期限。`(sleep 0.0)` 是 CL 式的
"让出 0 秒"。

与 CL 一样，`sleep` 接受**秒**。整数不会自动转换为浮点数，所以 CL 的 `(sleep 1)` 在这里写成 `(sleep 1.0)`。负值或 NaN 会
panic。

## 4. `WaitGroup` — 等待 N 个完成

| 名称 | 用法 | 类型 | 含义 |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | 没有未完成项的组 |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | 增加计数器。在工作开始之前 |
| `done` | `(done wg)` | `(WaitGroup)→()` | 完成了一个。为 0 时释放所有等待者 |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | 等到变为 0。可以从任意多个任务等待 |

`(wait wg)` 和 `(wait task)` 按接收者的类型解析，所以同名共存。计数器低于 0 时 panic（`done` 调用过多、负的 `add`）。与 Go
一样，回到 0 的组可以再次从 `add` 开始使用。即使任务在不同的 OS 线程上运行，更新也不会丢失。

**有了 `Task<T>`，就不像 Go 那样需要它了**——很多场合 `(doiter (t tasks) (wait t))` 就够了。它是为动态增加的工作，或者不想
持有句柄的情况准备的工具。

```lisp
;; fan-in：每个输入启动一个任务再汇合（本语言没有 nil 通道）
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — 按时间送达的通道

| 名称 | 用法 | 类型 | 含义 |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | `sec` 秒后送达一个值的通道 |

Go 的 `time.After`。可以直接写在 `select` 的超时分支中（[语法参考 12.3](../syntax.md#123-select--同时等待多个通道操作)）。
容量为 1，所以即使没有人接收，发送方的任务也能结束。

## 6. `Mutex<T>` — 共享数据的互斥

| 名称 | 用法 | 类型 | 含义 |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | 持有 `v`、未加锁的互斥锁 |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | 取得锁（会等待） |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | 归还。未加锁时 panic |
| `with-lock` | `(with-lock (x m) body...)` | 宏 | 加锁，把内容绑定到 `x` 执行 `body`，**一定**释放 |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` 不是值的副本而是"位置"**（`symbol-macrolet`）。`(setf x 42)` 会修改互斥锁的内容。
- `with-lock` 用 `unwind-protect` 释放，所以无论以正常结束、`throw`、panic，还是 `break`/`return`/`return-from` 中哪种方式
  退出都会释放。
- **重入会死锁**（不会 panic）。因自己的锁而停住的任务，调度器会报告"没有任何任务能推进"。
- **`m::v` 可以从锁外部访问内容**，但在其他任务可能正在修改的意义上是未定义的。与 Go 的 `sync.Mutex` 立场相同，在没有所有权和
  借用检查的语言中，无法构建 `MutexGuard` 那样的静态保证。

## 7. `Thread<T>` — 专用 OS 线程

`(thread (f args...))`（[语法参考 12.2](../syntax.md#122-thread--在专用-os-线程上启动任务)）返回的句柄。是 `Task<T>`
的对应物。

| 名称 | 用法 | 类型 | 含义 |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | 等待完成并返回其值（停下的是调用它的**任务**。可以调用任意多次，值会被缓存） |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | `(thread (f))` 的函数版本（Rust 的 `std::thread::spawn`） |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | 正在运行的 OS 线程的编号。在进程内唯一，除了"是否同一线程"之外没有别的含义 |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | 机器能同时运行的线程数（`TYPELISP_THREADS` 的默认值）。OS 不回答时 panic |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; 会阻塞的 C 函数
(let ((th (thread (sleepy 500000))))
  ...                                            ; 其间其他任务继续推进
  (join th))                                     ; => 500000
```

- 调用会阻塞的 C 函数（`defffi`）时，停下的只有该线程。
- `thread` 中的 `task` 作为普通任务在其他线程上运行。
- 在 `typl` 中也可以使用。解释执行中的 `(thread (f ...))` 会当场编译 `f`，然后在专用线程上运行。
- `Thread::spawn` 接受函数值，所以不会当场编译。在 `typl` 正在解释执行的顶层调用它会 panic
  （[语法参考 12.2](../syntax.md#122-thread--在专用-os-线程上启动任务)）。在编译过的函数中可以使用。

## 8. 没有的东西

- **`Atomic`**。`Mutex` 就够了。
- **任务局部变量**（Go 也没有）。
- **nil 通道**。原因与替代写法见[语法参考 12.7](../syntax.md#127-与-go-的区别)。
