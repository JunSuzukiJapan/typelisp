# タスクとチャネル

タスク（軽量スレッド）の語彙。起動の `task`・`thread` と多重待ちの `select` は特殊形で、
[構文リファレンス](../syntax.md#12-並行機構タスク)にある。ここはそれ以外——型とメソッドと関数。

タスクは**協調的**で、1 つのタスクが切り替わるのは書いた場所だけ。タスクは
`TYPELISP_THREADS` 本の OS スレッドで同時に走る（`typl` で他のスレッドへ出るのはコンパイル済みの
タスクだけ）。どこで切り替わるかと Go との違いは
[構文リファレンス 12.5](../syntax.md#125-切り替わる場所) と [12.7](../syntax.md#127-go-との違い)。

## 1. `Task<T>` — タスクのハンドル

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | 完了を待ち、その値を返す |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **何度 `wait` してもよい**（値をキャッシュする）。Rust の `JoinHandle::join` と違い
  ハンドルを消費しないので、複数箇所から待てる。
- **`wait` しなくてもタスクは走る**。ハンドルを捨てても止まらない。
- ふつうの値なので `Vector<Task<()>>` にも入る。
- **メインタスクが終わればプロセスが終わる**（Go と同じ）。走っている他のタスクは中断され、
  `unwind-protect` の cleanup は走らない——スタックの巻き戻しではなくプロセス終了だから。

## 2. `Chan<T>` — チャネル

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | 容量 `n` のチャネル。`0` はランデブー（バッファ無し） |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | 空きが出るまで待って渡す |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | 値が来るまで待つ。閉じて空なら `none` |
| `close` | `(close ch)` | `(Chan<T>)→()` | 閉じる |
| `len` | `(len ch)` | `(Chan<T>)→int` | いまバッファにある個数 |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | 容量 |

**型引数は `the` で決める**（`(the Vector<i32> (Vector::new))` と同じ書き方）。容量は
**必ず書く**——Go が `make(chan int)` と `make(chan int, 16)` を書き分けるのと同じ 2 つを、
`(Chan::new 0)` と `(Chan::new 16)` で書く。

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go の for v := range ch
```

- **`Chan<T>` は自分自身のイテレータ**（`Iter` を実装する）。`recv` が `Option<T>` を返す
  ので `Iter::next` と同型で、`doiter` も `map`/`filter`/`foldl` もそのまま効く。
- **閉じたチャネルへの `send` は panic**、**2 度目の `close` も panic**（どちらも Go と同じ。
  プログラムのバグであって回復可能な失敗ではないので `Result` にしない）。
- **`send` で待っているタスクがいるチャネルを `close` すると、そのタスクが panic する**
  （Go の規則）。
- 閉じたチャネルからの `recv` は、バッファに残っていればそれを返し、空になったら `none` を
  返し続ける。
- `close` は受け手の型で解決されるので `Stream` トレイトの `close` とは別物。`Chan<T>` は
  `Stream` を実装しない。
- **負の容量は panic**（黙って 0 に丸めない）。

## 3. `yield` / `sleep` — 譲る

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | 残りの番を手放す（Go の `runtime.Gosched`） |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | **そのタスクだけ**止める。他は走り続ける |

`sleep` はスレッドではなくタスクを止める。走れるタスクが 1 つも無くなったときだけ、
最も近い期限まで OS の `sleep` に入る。`(sleep 0.0)` は CL 流の「譲る 0 秒」。

`sleep` は CL と同じく**秒**を取る。整数は浮動小数点数に自動で変換されないので、CL の `(sleep 1)` は
ここでは `(sleep 1.0)` と書く。負や NaN は panic。

## 4. `WaitGroup` — N 個の完了待ち

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | 何も未完了でない群 |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | カウンタに足す。仕事を始める前に |
| `done` | `(done wg)` | `(WaitGroup)→()` | 1 つ終わった。0 で全待機者が解放される |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | 0 になるまで待つ。何タスクからでも |

`(wait wg)` と `(wait task)` は受け手の型で解決されるので同名で共存する。
カウンタが 0 を下回ると panic する（`done` の呼びすぎ・負の `add`）。
0 に戻った群は Go と同じくもう一度 `add` から使える。タスクが別々の OS スレッドで走っても
更新は失われない。

**`Task<T>` がある以上 Go ほどは要らない**——`(doiter (t tasks) (wait t))` で足りる場面が
多い。動的に増える仕事や、ハンドルを持ちたくない場合のための道具。

```lisp
;; fan-in: 入力ごとに 1 タスク立てて合流する（この言語に nil チャネルは無い）
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — 時間で届くチャネル

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | `sec` 秒後に 1 つ届くチャネル |

Go の `time.After`。`select` のタイムアウト腕にそのまま書ける
（[構文リファレンス 12.3](../syntax.md#123-select--複数のチャネル操作を同時に待つ)）。容量 1 なので、
誰も受け取らなくても送信側のタスクは終われる。

## 6. `Mutex<T>` — 共有データの排他

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | `v` を持つ、ロックされていないミューテックス |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | ロックを取る（待つ） |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | 返す。ロックされていなければ panic |
| `with-lock` | `(with-lock (x m) body...)` | マクロ | ロックし、`x` に中身を束ねて `body` を走らせ、**必ず**解放 |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` は値のコピーではなく「場所」**（`symbol-macrolet`）。`(setf x 42)` がミューテックスの
  中身を書き換える。
- `with-lock` は `unwind-protect` で解放するので、正常終了・`throw`・`panic`・
  `break`/`return`/`return-from` のどれで抜けても解放される。
- **再入するとデッドロックする**（panic ではない）。自分のロックで止まったタスクは、
  スケジューラが「どれも進めない」と報告する。
- **`m::v` でロックの外から中身に触れる**が、別のタスクが書き換えの途中かもしれない
  という意味で未定義。Go の `sync.Mutex` と同じ立場で、所有権も借用検査も無い言語に
  `MutexGuard` のような静的保証は作れない。

## 7. `Thread<T>` — 専用の OS スレッド

`(thread (f args...))`（[構文リファレンス 12.2](../syntax.md#122-thread--専用の-os-スレッドでタスクを起動する)）が
返すハンドル。`Task<T>` の対。

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | 完了を待ち、その値を返す（呼んだ**タスク**が止まる。何度でも可、値はキャッシュ） |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | `(thread (f))` の関数版（Rust の `std::thread::spawn`） |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | 走っている OS スレッドの番号。プロセス内で一意で、「同じスレッドか」以上の意味は無い |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | マシンが同時に走らせられるスレッド数（`TYPELISP_THREADS` の既定値）。OS が答えないときは panic |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; ブロックする C 関数
(let ((th (thread (sleepy 500000))))
  ...                                            ; 他のタスクはその間も進む
  (join th))                                     ; => 500000
```

- ブロックする C 関数（`defffi`）を呼んでも、止まるのはそのスレッドだけ。
- `thread` の中の `task` は通常のタスクとして他のスレッドで走る。
- `typl` でも使える。解釈実行中の `(thread (f ...))` と `Thread::spawn` は、走らせる関数を
  その場でコンパイルしてから専用スレッドで走らせる。外側のローカル変数を参照する `lambda` は
  そのままではコンパイルできず panic する（[構文リファレンス 12.2](../syntax.md#122-thread--専用の-os-スレッドでタスクを起動する)）。
  コンパイル済みの関数の中で作った `lambda` なら渡せる。

## 8. `Context` — 協調的なキャンセル

Go の `context.Context`。仕事を外から止めたいときに渡す。止めるのは**協調的**で、`cancel` は
何も中断しない——タスクやスレッドが自分で `is-cancelled` を見るか、`done` を受信して気づく。

| 名前 | 使い方 | 型 | 意味 |
|---|---|---|---|
| `Context::background` | `(Context::background)` | `()→Context` | 根になる新しいコンテキスト |
| `Context::with-cancel` | `(Context::with-cancel parent)` | `(Context)→Context` | `parent` の子を作る |
| `Context::with-timeout` | `(Context::with-timeout parent sec)` | `(Context,f64)→Context` | `parent` の子を作り、`sec` 秒後に自分でキャンセルする |
| `cancel` | `(cancel ctx)` | `(Context)→()` | キャンセルする。何度呼んでもよい |
| `done` | `(done ctx)` | `(Context)→Chan<()>` | キャンセルされると閉じるチャネル |
| `is-cancelled` | `(is-cancelled ctx)` | `(Context)→bool` | キャンセルされたか |

```lisp
(defun worker ((ctx Context) (jobs Chan<int>)) ()
  (loop
    (select
      ((v (recv (done ctx))) (println "stopped") (break))
      ((j (recv jobs)) (match j
                         ((some n) (println "job ~a" n))
                         ((none) (break)))))))

(let* ((ctx (Context::with-timeout (Context::background) 1.0))
       (jobs (the Chan<int> (Chan::new 0))))
  (task (worker ctx jobs))
  (send jobs 1)
  (send jobs 2)
  (cancel ctx)                          ; job 1、job 2 のあと stopped
  (sleep 0.1))
```

- **キャンセルは子へ伝わる**。`with-cancel`/`with-timeout` で作ったコンテキストは、親が
  キャンセルされると一緒にキャンセルされる。逆向き（子から親）には伝わらない。
- キャンセル済みのコンテキストから作った子は、最初からキャンセルされている。
- `done` は閉じるだけで値は送られない。受信すると `none` が返る。
- `(Context::background)` は呼ぶたびに別の根を作る。Go の `Background()` は 1 つしかなく
  キャンセルできないが、ここでは根もキャンセルでき、その影響はそこから作ったものにだけ及ぶ。
- タスク間でもスレッド間でも渡せる。

## 9. 無いもの

- **`Atomic`**。`Mutex` で足りる。
- **タスクローカル変数**（Go にも無い）。
- **nil チャネル**。理由と代わりの書き方は [構文リファレンス 12.7](../syntax.md#127-go-との違い)。
