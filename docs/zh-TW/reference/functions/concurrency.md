<!-- translated-from: docs/ja/reference/functions/concurrency.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# 任務與通道

任務（輕量級執行緒）的詞彙。啟動它們的 `task`・`thread` 與多路等待的 `select` 是特殊形式，見[語法參考](../syntax.md#12-並行任務)。本章是
其餘部分——型別、方法與函式。

任務是**協作式**的，一個任務只在你寫下的地方切換。任務在 `TYPELISP_THREADS` 個 OS 執行緒上同時執行（在 `typl` 中只有編譯過的任務才會到其他
執行緒上執行）。在哪裡切換、與 Go 的差異見[語法參考 12.5](../syntax.md#125-切換發生的位置)與[12.7](../syntax.md#127-與-go-的差異)。

## 1. `Task<T>` — 任務控制代碼

| 名稱 | 用法 | 型別 | 意義 |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | 等待完成並回傳其值 |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **可以 `wait` 任意多次**（值會被快取）。與 Rust 的 `JoinHandle::join` 不同，不會消耗控制代碼，可以從多處等待。
- **即使不 `wait`，任務也會執行。** 丟掉控制代碼也不會讓它停止。
- 它是一般的值，所以也能放進 `Vector<Task<()>>`。
- **主任務結束時行程就結束**（與 Go 相同）。其他執行中的任務會被中斷，`unwind-protect` 的 cleanup 不會執行——因為這是行程結束，不是堆疊展開。

## 2. `Chan<T>` — 通道

| 名稱 | 用法 | 型別 | 意義 |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | 容量為 `n` 的通道。`0` 是會合（無緩衝） |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | 等到有空位再交出 |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | 等到有值抵達。關閉且為空時為 `none` |
| `close` | `(close ch)` | `(Chan<T>)→()` | 關閉 |
| `len` | `(len ch)` | `(Chan<T>)→int` | 目前緩衝區中的個數 |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | 容量 |

**型別引數以 `the` 決定**（與 `(the Vector<i32> (Vector::new))` 的寫法相同）。**容量一定要寫**——Go 分別寫成 `make(chan int)` 與
`make(chan int, 16)` 的兩種情況，在這裡寫成 `(Chan::new 0)` 與 `(Chan::new 16)`。

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go 的 for v := range ch
```

- **`Chan<T>` 是它自己的迭代器**（實作了 `Iter`）。`recv` 回傳 `Option<T>`，與 `Iter::next` 同型，所以 `doiter` 以及 `map`/`filter`/`foldl`
  都可以直接使用。
- **對已關閉的通道 `send` 會 panic**，**第 2 次 `close` 也會 panic**（都與 Go 相同。這是程式的 bug 而不是可恢復的失敗，所以不用 `Result`）。
- **關閉有任務正在等待 `send` 的通道，該任務會 panic**（Go 的規則）。
- 從已關閉的通道 `recv`，緩衝區中有剩餘就回傳它，變空之後一直回傳 `none`。
- `close` 依接收者的型別解析，所以與 `Stream` trait 的 `close` 是不同的東西。`Chan<T>` 沒有實作 `Stream`。
- **負的容量會 panic**（不會默默捨入為 0）。

## 3. `yield` / `sleep` — 讓出

| 名稱 | 用法 | 型別 | 意義 |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | 放棄本輪剩餘的執行權（Go 的 `runtime.Gosched`） |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | **只讓該任務**停下。其他任務繼續執行 |

`sleep` 停下的是任務而不是執行緒。只有當沒有任何任務可以執行時，才會進入 OS 的 `sleep` 直到最近的期限。`(sleep 0.0)` 是 CL 式的「讓出 0 秒」。

與 CL 一樣，`sleep` 接受**秒**。整數不會自動轉換為浮點數，所以 CL 的 `(sleep 1)` 在這裡寫成 `(sleep 1.0)`。負值或 NaN 會 panic。

## 4. `WaitGroup` — 等待 N 個完成

| 名稱 | 用法 | 型別 | 意義 |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | 沒有未完成項目的群組 |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | 增加計數器。在工作開始之前 |
| `done` | `(done wg)` | `(WaitGroup)→()` | 完成了一個。變為 0 時釋放所有等待者 |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | 等到變為 0。可以從任意多個任務等待 |

`(wait wg)` 與 `(wait task)` 依接收者的型別解析，所以同名共存。計數器低於 0 時 panic（`done` 呼叫過多、負的 `add`）。與 Go 一樣，回到 0 的
群組可以再次從 `add` 開始使用。即使任務在不同的 OS 執行緒上執行，更新也不會遺失。

**有了 `Task<T>`，就不像 Go 那樣需要它了**——許多場合 `(doiter (t tasks) (wait t))` 就夠了。它是為動態增加的工作，或者不想持有控制代碼的
情況準備的工具。

```lisp
;; fan-in：每個輸入啟動一個任務再匯合（這個語言沒有 nil 通道）
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — 依時間送達的通道

| 名稱 | 用法 | 型別 | 意義 |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | `sec` 秒後送達一個值的通道 |

Go 的 `time.After`。可以直接寫在 `select` 的逾時分支中（[語法參考 12.3](../syntax.md#123-select--同時等待多個通道操作)）。容量為 1，所以即使
沒有人接收，傳送端的任務也能結束。

## 6. `Mutex<T>` — 共享資料的互斥

| 名稱 | 用法 | 型別 | 意義 |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | 持有 `v`、未上鎖的互斥鎖 |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | 取得鎖（會等待） |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | 歸還。未上鎖時 panic |
| `with-lock` | `(with-lock (x m) body...)` | 巨集 | 上鎖，把內容繫結到 `x` 執行 `body`，**一定**釋放 |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` 不是值的副本而是「位置」**（`symbol-macrolet`）。`(setf x 42)` 會修改互斥鎖的內容。
- `with-lock` 以 `unwind-protect` 釋放，所以無論以正常結束、`throw`、panic，還是 `break`/`return`/`return-from` 中哪種方式離開都會釋放。
- **重新進入會死結**（不會 panic）。因自己的鎖而停住的任務，排程器會回報「沒有任何任務能前進」。
- **`m::v` 可以從鎖的外面存取內容**，但在其他任務可能正在修改的意義上是未定義的。與 Go 的 `sync.Mutex` 立場相同，在沒有所有權與借用檢查的
  語言中，無法建立 `MutexGuard` 那樣的靜態保證。

## 7. `Thread<T>` — 專用 OS 執行緒

`(thread (f args...))`（[語法參考 12.2](../syntax.md#122-thread--在專用-os-執行緒上啟動任務)）回傳的控制代碼。是 `Task<T>` 的對應物。

| 名稱 | 用法 | 型別 | 意義 |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | 等待完成並回傳其值（停下的是呼叫它的**任務**。可以呼叫任意多次，值會被快取） |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | `(thread (f))` 的函式版本（Rust 的 `std::thread::spawn`） |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | 正在執行的 OS 執行緒的編號。在行程內唯一，除了「是否同一執行緒」之外沒有別的意義 |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | 機器能同時執行的執行緒數（`TYPELISP_THREADS` 的預設值）。OS 不回答時 panic |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; 會阻塞的 C 函式
(let ((th (thread (sleepy 500000))))
  ...                                            ; 這段期間其他任務繼續前進
  (join th))                                     ; => 500000
```

- 呼叫會阻塞的 C 函式（`defffi`）時，停下的只有該執行緒。
- `thread` 中的 `task` 作為一般任務在其他執行緒上執行。
- 在 `typl` 中也可以使用。直譯執行中的 `(thread (f ...))` 與 `Thread::spawn` 會當場編譯要執行的函式，然後在專用執行緒上執行。
  參照外部區域變數的 `lambda` 無法直接編譯，會 panic（[語法參考 12.2](../syntax.md#122-thread--在專用-os-執行緒上啟動任務)）。
  在編譯過的函式中建立的 `lambda` 可以傳入。

## 8. 沒有的東西

- **`Atomic`**。`Mutex` 就夠了。
- **任務區域變數**（Go 也沒有）。
- **nil 通道**。原因與替代寫法見[語法參考 12.7](../syntax.md#127-與-go-的差異)。
