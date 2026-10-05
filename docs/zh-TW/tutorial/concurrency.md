<!-- translated-from: docs/ja/tutorial/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 並行

在 typelisp 中，透過啟動**任務**（輕量級執行緒）讓多項工作並行進行，任務之間透過**通道**傳遞值。這與 Go 的 goroutine 和通道很接近。
本章依序說明任務的啟動與結果的取得、通道、`select`、共享資料的互斥，以及專用的 OS 執行緒。閱讀本章前請先讀[型別基礎](types.md)。

## 1. 啟動任務並等待結果

`(task (函式 引數...))` 把一次函式呼叫作為新任務啟動。啟動的一方不會等待，而是繼續往下執行。回傳值是 `Task<T>` 型別的控制代碼，用
`(wait 控制代碼)` 等待其完成並取得結果。

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; 等待 0.1 秒（只有這個任務停下）
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` 中只能寫函式呼叫的形式。引數在寫 `task` 的地方求值，在新任務中執行的只有呼叫本身。
- 要執行多個運算式時，建立一個 `lambda` 並當場呼叫：
  `(task ((lambda () () (println "start") (work))))`
- `wait` 可以呼叫任意多次，結果會被記住。
- 即使不 `wait`，任務也會執行。
- **主要處理結束時，程式就結束。** 仍在執行的任務會被中途終止。

## 2. 用通道傳遞值

通道 `Chan<T>` 是任務之間傳遞 `T` 型別值的通路。`Chan::new` 的引數是容量（可以累積的值的個數）。容量為 0 的通道，傳送方與接收方都要
等到對方就緒。

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; 傳送
  (close ch))                  ; 不再傳送

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; 一直接收到關閉為止
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` 傳送。通道滿了時會等到有空位。
- `(recv ch)` 接收。會等到有值抵達。回傳值是 `Option<T>`，通道關閉且變空後回傳 `none`。
- 用 `doiter` 走訪通道，會一直接收值直到通道關閉。也可以直接傳給 `map` 或 `filter`。
- 對已關閉的通道 `send` 會 panic。

### 把工作分給多個任務

常見的形式是準備一條傳送工作的通道，讓多個工作者（處理工作的任務）從中領取。有空的工作者會領取下一項工作，所以即使耗時的工作與很快結束的
工作混在一起，工作也會自然地分攤。

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; 這項工作需要的時間

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; 在 jobs 關閉前，一次領取一項工作
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; 工作中。這段期間其他工作者領取下一項工作
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; 處理的工作數

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
  (close jobs)                    ; 工作就這些
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

- 最初的 3 項工作由 A、B、C 各領一項。
- 0.1 秒後 B 與 C 有空了，領取剩下的工作。A 在處理耗時的工作 1 期間不領取新工作。
- 結果 A 處理了 1 項，B 處理了 2 項，C 處理了 3 項。程式中並沒有寫哪個工作者領哪項工作。
- 關閉 `jobs` 後，各工作者的 `doiter` 結束、任務完成，`wait` 回傳各自處理的數量。

`jobs` 是容量為 0 的通道，所以 `send` 會等到某個工作者領取工作。加大容量後，主任務可以不等工作者就把工作累積起來。

## 3. `select`：同時等待多個通道

`select` 在多個通道操作中，執行最先變得可以進行的那一個。`(after 秒)` 是經過指定時間後送達一個值的通道。與它組合就能寫出逾時。

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

- `((v (recv ch)) 本體...)` 是接收分支。`v` 中是 `Option<T>`。
- 寫成 `((send ch x) 本體...)` 就是傳送分支。
- 同時有多個分支可以執行時，會隨機選擇其中一個。
- 在最後寫 `(else 本體...)`，沒有分支能立即執行時就執行 `else`，不會等待。

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. 保護共享資料

多個任務修改同一個值時，用 `Mutex<T>` 保護。`with-lock` 取得鎖，把內容繫結到變數上執行本體，無論以何種方式離開都一定會歸還鎖。在本體中
對該變數 `setf`，就會修改 `Mutex` 的內容。

`WaitGroup` 是等待一定數量的任務完成的工具。用 `add` 增加計數，各任務結束時呼叫 `done`，用 `wait` 等到計數變成 0。

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

不經過 `Mutex` 或通道，從多個任務同時修改同一個值，結果是沒有保證的。任務之間傳遞資料時盡量使用通道，只在必要時才共享。

## 5. 任務切換的位置

任務以協作方式切換。一個任務只在以下位置把執行權讓給其他任務：

- `(yield)`、`(sleep 秒)`、`(wait 控制代碼)`
- 需要等待的通道操作（`send`、`recv`、`select`）
- 需要等待的 socket 操作（連線、讀寫等）

`sleep` 的引數是表示秒數的 `f64`。要寫 `(sleep 1.0)`，而不是 `(sleep 1)`。

任務會在多個 OS 執行緒上同時執行。不過，用 `typl` 直接執行程式時，只有執行[編譯](../guide/compile.md)過的函式的任務才會到其他執行緒上
執行。其他任務在一個執行緒上，在上述位置切換著推進。

## 6. `thread`：在專用 OS 執行緒上執行

呼叫耗時的 C 函式（[C FFI](../guide/ffi.md)）等不想阻礙其他任務的處理，用 `thread` 啟動。寫法與 `task` 相同，會為該任務單獨準備一個
OS 執行緒。用 `join` 等待其完成。

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- `thread` 的控制代碼是 `Thread<T>` 型別。`join` 與 `wait` 一樣可以呼叫任意多次。
- `thread` 只能執行可以編譯的函式。以 `typl` 執行時，會先當場編譯所呼叫的函式再執行。

## 7. 任務與其他功能

- 在任務中發生 panic，整個程式會停止。
- `throw` 不會到達任務之外。要跳出任務本體的 `throw` 會變成 panic。
- 一次 `println` 的輸出不會在行中間與其他任務的輸出混在一起。

## 8. 接下來閱讀

- [任務與通道](../reference/functions/concurrency.md)：函式一覽
- [語法參考第 12 章](../reference/syntax.md#12-並行任務)：切換位置的詳情、與 Go 的差異
- [檔案 I/O、串流與網路](../guide/io.md)：使用任務撰寫伺服器的方法
