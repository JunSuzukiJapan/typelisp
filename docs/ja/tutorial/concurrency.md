# 並行処理

typelisp では、**タスク**（軽量スレッド）を起動して複数の処理を並行に進め、タスクどうしは
**チャネル**で値を受け渡します。Go の goroutine とチャネルに近い仕組みです。この章では、
タスクの起動と結果の受け取り、チャネル、`select`、共有データの排他、専用の OS スレッドを
順に説明します。[型の基本](types.md) を読んでいることを前提にします。

## 1. タスクを起動して結果を待つ

`(task (関数 引数...))` で、関数呼び出しを新しいタスクとして起動します。起動した側は待たずに
先へ進みます。戻り値は `Task<T>` 型のハンドルで、`(wait ハンドル)` で完了を待って結果を
受け取ります。

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; 0.1 秒待つ（このタスクだけが止まる）
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` に書けるのは関数呼び出しの形だけです。引数は `task` を書いた場所で評価され、新しい
  タスクで実行されるのは呼び出しだけです。
- 複数の式を実行したいときは、`lambda` を作ってその場で呼びます：
  `(task ((lambda () () (println "start") (work))))`
- `wait` は何度呼んでもかまいません。結果は覚えておかれます。
- `wait` しなくてもタスクは実行されます。
- **メインの処理が終わると、プログラムは終了します。** 残っているタスクは途中で打ち切られます。

## 2. チャネルで値を受け渡す

チャネル `Chan<T>` は、タスクどうしで `T` 型の値を受け渡す通り道です。`Chan::new` の引数は
容量（ためておける値の数）です。容量 0 のチャネルでは、送る側と受け取る側がそろうまで
両方が待ちます。

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; 送る
  (close ch))                  ; もう送らない

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; 閉じられるまで受け取る
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` で送ります。チャネルがいっぱいなら、空きが出るまで待ちます。
- `(recv ch)` で受け取ります。値が来るまで待ちます。戻り値は `Option<T>` で、チャネルが閉じられて
  空になると `none` を返します。
- `doiter` でチャネルを回すと、閉じられるまで値を受け取り続けます。`map` や `filter` にも
  そのまま渡せます。
- 閉じたチャネルに `send` すると `panic` します。

### 仕事を複数のタスクに分ける

よくある形は、仕事を流すチャネルを 1 本用意して、複数のワーカー（仕事を処理するタスク）に
そこから取らせるものです。手の空いたワーカーから次の仕事を取るので、時間のかかる仕事と
すぐ終わる仕事が混ざっていても、仕事が自然に分担されます。

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; この仕事にかかる時間

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; jobs が閉じられるまで、仕事を 1 つずつ取る
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; 仕事中。この間に他のワーカーが次の仕事を取る
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; 処理した仕事の数

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
  (close jobs)                    ; 仕事はこれで全部
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

- 最初の 3 つの仕事は、A、B、C が 1 つずつ取ります。
- 0.1 秒後に B と C が手を空け、残りの仕事を取ります。A は時間のかかる仕事 1 をしている間、
  新しい仕事を取りません。
- 結果として、A が 1 件、B が 2 件、C が 3 件を処理しました。どのワーカーがどの仕事を取るかは
  プログラムには書いていません。
- `jobs` を閉じると、各ワーカーの `doiter` が終わってタスクが終了し、`wait` がそれぞれの件数を返します。

`jobs` は容量 0 のチャネルなので、`send` は、どれかのワーカーが仕事を受け取るまで待ちます。
容量を大きくすると、メインのタスクはワーカーを待たずに仕事を積んでおけます。

## 3. `select`：複数のチャネルを同時に待つ

`select` は、複数のチャネル操作のうち、先にできるようになったものを 1 つ実行します。
`(after 秒)` は、指定した時間が経つと値が 1 つ届くチャネルです。これと組み合わせると
タイムアウトが書けます。

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

- `((v (recv ch)) 本体...)` は受信の腕です。`v` には `Option<T>` が入ります。
- `((send ch x) 本体...)` と書くと送信の腕になります。
- 同時に複数の腕が実行できるときは、そのうちの 1 つが無作為に選ばれます。
- 最後に `(else 本体...)` を書くと、どの腕もすぐには実行できないときに `else` を実行し、待ちません。

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. 共有データを守る

複数のタスクが同じ値を書き換えるときは、`Mutex<T>` で守ります。`with-lock` はロックを取り、
中身を変数に結び付けて本体を実行し、どう抜けても必ずロックを返します。本体の中で変数に
`setf` すると、`Mutex` の中身が書き換わります。

`WaitGroup` は、決まった数のタスクの完了を待つための道具です。`add` で数を増やし、各タスクが
終わったら `done` を呼び、`wait` で数が 0 になるまで待ちます。

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

`Mutex` やチャネルを通さずに、複数のタスクから同じ値を同時に書き換えた場合の結果は保証されません。
タスクどうしでデータを渡すときは、できるだけチャネルを使い、共有は必要な場合だけにしてください。

## 5. タスクが切り替わる場所

タスクは協調的に切り替わります。1 つのタスクが他のタスクに順番を譲るのは、次の場所だけです。

- `(yield)`、`(sleep 秒)`、`(wait ハンドル)`
- 待つことになったチャネル操作（`send`、`recv`、`select`）
- 待つことになったソケット操作（接続、読み書きなど）

`sleep` の引数は秒を表す `f64` です。`(sleep 1)` ではなく `(sleep 1.0)` と書きます。

タスクは複数の OS スレッドで同時に実行されます。ただし `typl` でプログラムをそのまま実行して
いるときは、他のスレッドへ出るのは[コンパイル](../guide/compile.md)済みの関数を実行するタスクだけです。
それ以外のタスクは 1 つのスレッドの上で、上の場所で切り替わりながら進みます。

## 6. `thread`：専用の OS スレッドで実行する

時間のかかる C の関数（[C FFI](../guide/ffi.md)）を呼ぶなど、他のタスクを止めたくない処理は
`thread` で起動します。書き方は `task` と同じで、そのタスクのためだけに OS スレッドを 1 本
用意します。完了は `join` で待ちます。

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- `thread` のハンドルは `Thread<T>` 型です。`join` は `wait` と同じく何度でも呼べます。
- `thread` で実行できるのはコンパイルできる関数だけです。`typl` で実行しているときは、呼ぶ関数を
  その場でコンパイルしてから実行します。

## 7. タスクと他の機能

- タスクの中で `panic` すると、プログラム全体が止まります。
- `throw` はタスクの外へは届きません。タスクの本体を抜ける `throw` は `panic` になります。
- `println` の 1 回の出力が、他のタスクの出力と行の途中で混ざることはありません。

## 8. 次に読むもの

- [タスクとチャネル](../reference/functions/concurrency.md)：関数の一覧
- [構文リファレンス 12 章](../reference/syntax.md#12-並行機構タスク)：切り替わる場所の詳細、Go との違い
- [ファイル I/O、ストリーム、ネットワーク](../guide/io.md)：タスクを使ったサーバの書き方
