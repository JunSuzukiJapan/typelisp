<!-- translated-from: docs/ja/tutorial/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 并发

在 typelisp 中，通过启动**任务**（轻量级线程）让多项工作并发进行，任务之间通过**通道**传递值。这与 Go 的
goroutine 和通道很接近。本章依次介绍任务的启动与结果的获取、通道、`select`、共享数据的互斥，以及专用的 OS 线程。
阅读本章前请先读[类型基础](types.md)。

## 1. 启动任务并等待结果

`(task (函数 参数...))` 把一次函数调用作为新任务启动。启动方不等待，继续往下执行。返回值是 `Task<T>` 类型的句柄，
用 `(wait 句柄)` 等待其完成并取得结果。

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; 等待 0.1 秒（只有这个任务停下）
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` 中只能写函数调用的形式。参数在写 `task` 的地方求值，在新任务中执行的只有调用本身。
- 要执行多个表达式时，创建一个 `lambda` 并当场调用：
  `(task ((lambda () () (println "start") (work))))`
- `wait` 可以调用任意多次，结果会被记住。
- 即使不 `wait`，任务也会执行。
- **主处理结束时，程序就结束。** 仍在运行的任务会被中途终止。

## 2. 用通道传递值

通道 `Chan<T>` 是任务之间传递 `T` 类型值的通路。`Chan::new` 的参数是容量（可以积存的值的个数）。容量为 0 的
通道上，发送方和接收方都要等到对方就绪。

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; 发送
  (close ch))                  ; 不再发送

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; 一直接收到关闭为止
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` 发送。通道满时等待到有空位。
- `(recv ch)` 接收。等待到有值到达。返回值是 `Option<T>`，通道关闭且为空后返回 `none`。
- 用 `doiter` 遍历通道，会一直接收值直到通道关闭。也可以直接传给 `map` 或 `filter`。
- 向已关闭的通道 `send` 会 panic。

### 把工作分给多个任务

常见的形式是准备一条传送工作的通道，让多个工作者（处理工作的任务）从中领取。空闲的工作者领取下一项工作，所以即使
耗时的工作和很快结束的工作混在一起，工作也会自然地分摊。

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; 这项工作需要的时间

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; 在 jobs 关闭前，一次领取一项工作
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; 工作中。这期间其他工作者领取下一项工作
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; 处理的工作数

(let* ((jobs (the Chan<job> (Chan::new 0)))
       (a (task (worker "A" jobs)))
       (b (task (worker "B" jobs)))
       (c (task (worker "C" jobs))))
  (send jobs (job::new 1 0.3))
  (send jobs (job::new 2 0.1))
  (send jobs (job::new 3 0.1))
  (send jobs (job::new 4 0.2))
  (send jobs (job::new 5 0.1))
  (send jobs (job::new 6 0.1))
  (close jobs)                    ; 工作就这些
  (println "A: ~a jobs, B: ~a jobs, C: ~a jobs" (wait a) (wait b) (wait c)))
```

```
A: start  job 1 (0.3s)
B: start  job 2 (0.1s)
C: start  job 3 (0.1s)
B: finish job 2
B: start  job 4 (0.2s)
C: finish job 3
C: start  job 5 (0.1s)
C: finish job 5
C: start  job 6 (0.1s)
A: finish job 1
B: finish job 4
C: finish job 6
A: 1 jobs, B: 2 jobs, C: 3 jobs
```

- 最初的 3 项工作由 A、B、C 各领一项。
- 0.1 秒后 B 和 C 空闲下来，领取剩下的工作。A 在处理耗时的工作 1 期间不领取新工作。
- 结果 A 处理了 1 项，B 处理了 2 项，C 处理了 3 项。程序中并没有写哪个工作者领哪项工作。
- 关闭 `jobs` 后，各工作者的 `doiter` 结束、任务完成，`wait` 返回各自处理的数量。

`jobs` 是容量为 0 的通道，所以 `send` 会等到某个工作者领取工作。增大容量后，主任务可以不等工作者就把工作积存起来。

## 3. `select`：同时等待多个通道

`select` 在多个通道操作中，执行最先变得可以进行的那一个。`(after 秒)` 是经过指定时间后送达一个值的通道。与它组合
就能写出超时。

```lisp
(defun late-send ((ch Chan<string>) (sec f64)) ()
  (sleep sec)
  (send ch "done"))

(let ((ch (the Chan<string> (Chan::new 1))))
  (task (late-send ch 1.0))
  (select
    ((v (recv ch)) (println "~a" (unwrap v)))
    ((z (recv (after 0.2))) (println "timeout"))))
;; timeout
```

- `((v (recv ch)) 主体...)` 是接收分支。`v` 中是 `Option<T>`。
- 写成 `((send ch x) 主体...)` 就是发送分支。
- 同时有多个分支可以执行时，随机选择其中一个。
- 在最后写 `(else 主体...)`，当没有分支能立即执行时执行 `else`，不会等待。

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. 保护共享数据

多个任务修改同一个值时，用 `Mutex<T>` 保护。`with-lock` 取得锁，把内容绑定到变量上执行主体，无论以何种方式退出
都一定归还锁。在主体中对该变量 `setf`，就会修改 `Mutex` 的内容。

`WaitGroup` 是等待一定数量任务完成的工具。用 `add` 增加计数，各任务结束时调用 `done`，用 `wait` 等到计数变为 0。

```lisp
(defun add-many ((counter Mutex<int>) (n int)) ()
  (dotimes (i n)
    (with-lock (c counter)
      (setf c (+ c 1)))))

(let ((counter (the Mutex<int> (Mutex::make 0)))
      (wg (the WaitGroup (WaitGroup::make))))
  (dotimes (i 4)
    (add wg 1)
    (task ((lambda () ()
             (add-many counter 1000)
             (done wg)))))
  (wait wg)
  (println "count = ~a" (with-lock (c counter) c)))    ; count = 4000
```

不经过 `Mutex` 或通道，从多个任务同时修改同一个值，结果是没有保证的。任务之间传递数据时尽量使用通道，只在必要时共享。

## 5. 任务切换的位置

任务以协作方式切换。一个任务只在以下位置把执行权让给其他任务：

- `(yield)`、`(sleep 秒)`、`(wait 句柄)`
- 需要等待的通道操作（`send`、`recv`、`select`）
- 需要等待的套接字操作（连接、读写等）

`sleep` 的参数是表示秒数的 `f64`。要写 `(sleep 1.0)`，而不是 `(sleep 1)`。

任务会在多个 OS 线程上同时执行。不过，用 `typl` 直接运行程序时，只有执行[编译](../guide/compile.md)过的函数的
任务才会到其他线程上运行。其他任务在一个线程上，在上述位置切换着推进。

## 6. `thread`：在专用 OS 线程上执行

调用耗时的 C 函数（[C FFI](../guide/ffi.md)）等不想阻塞其他任务的处理，用 `thread` 启动。写法与 `task` 相同，
会为该任务单独准备一个 OS 线程。用 `join` 等待其完成。

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- `thread` 的句柄是 `Thread<T>` 类型。`join` 和 `wait` 一样可以调用任意多次。
- `thread` 只能执行可以编译的函数。用 `typl` 运行时，会先当场编译所调用的函数再执行。

## 7. 任务与其他功能

- 在任务中发生 panic，整个程序停止。
- `throw` 不会到达任务之外。要跳出任务主体的 `throw` 会变成 panic。
- 一次 `println` 的输出不会在行中间与其他任务的输出混在一起。

## 8. 接下来阅读

- [任务与通道](../reference/functions/concurrency.md)：函数一览
- [语法参考第 12 章](../reference/syntax.md#12-并发任务)：切换位置的详情、与 Go 的区别
- [文件 I/O、流与网络](../guide/io.md)：使用任务编写服务器的方法
