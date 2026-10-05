<!-- translated-from: docs/ja/tutorial/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Concurrency

In typelisp you run work concurrently by starting **tasks** (lightweight threads), and tasks pass
values to each other through **channels**. The model is close to Go's goroutines and channels.
This chapter covers, in order, starting a task and getting its result, channels, `select`,
protecting shared data, and dedicated OS threads. It assumes you have read
[Type Basics](types.md).

## 1. Starting a task and waiting for its result

`(task (function arguments...))` starts a function call as a new task. The starting side does not
wait and carries on. The value is a handle of type `Task<T>`; `(wait handle)` waits for the task to
finish and returns its result.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; wait 0.1 seconds (only this task stops)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` takes only the form of a function call. The arguments are evaluated where the `task` is
  written; only the call itself runs in the new task.
- To run several expressions, make a `lambda` and call it on the spot:
  `(task ((lambda () () (println "start") (work))))`
- You may call `wait` as many times as you like. The result is remembered.
- A task runs even if you never `wait` for it.
- **When the main work finishes, the program ends.** Any tasks still running are cut off.

## 2. Passing values through channels

A channel `Chan<T>` is a path through which tasks pass values of type `T`. The argument to
`Chan::new` is the capacity (how many values it can hold). On a channel of capacity 0, the sender
and the receiver both wait until the other side is there.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; send
  (close ch))                  ; no more sends

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; receive until it is closed
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` sends. If the channel is full, it waits until there is room.
- `(recv ch)` receives. It waits until a value arrives. The result is an `Option<T>`; once the
  channel is closed and empty, it returns `none`.
- Iterating a channel with `doiter` keeps receiving values until it is closed. It can also be
  passed straight to `map` or `filter`.
- `send` on a closed channel panics.

### Splitting work among several tasks

A common pattern is to set up one channel carrying the work and have several workers (tasks that
process the work) take jobs from it. Whichever worker is free takes the next job, so even when slow
and quick jobs are mixed, the work spreads out naturally.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; how long this job takes

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; take one job at a time until jobs is closed
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; working; meanwhile other workers take the next jobs
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; how many jobs this worker did

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
  (close jobs)                    ; that is all the work
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

- A, B and C each take one of the first three jobs.
- After 0.1 seconds B and C are free and take the remaining jobs. While A is busy with the slow job
  1, it takes no new work.
- In the end A handled one job, B two and C three. Nothing in the program says which worker takes
  which job.
- Closing `jobs` ends each worker's `doiter`, the tasks finish, and each `wait` returns its count.

`jobs` is a channel of capacity 0, so `send` waits until some worker takes the job. With a larger
capacity, the main task could queue up work without waiting for the workers.

## 3. `select`: waiting on several channels at once

`select` performs whichever of several channel operations becomes possible first. `(after seconds)`
is a channel that delivers one value once the given time has passed. Combined with `select`, it
gives you a timeout.

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

- `((v (recv ch)) body...)` is a receive arm. `v` gets an `Option<T>`.
- `((send ch x) body...)` is a send arm.
- When several arms can proceed at the same time, one of them is chosen at random.
- With `(else body...)` at the end, `else` runs when no arm can proceed right away, and `select`
  does not wait.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Protecting shared data

When several tasks modify the same value, protect it with `Mutex<T>`. `with-lock` takes the lock,
binds the contents to a variable, runs the body, and always releases the lock however the body is
left. Assigning to the variable with `setf` inside the body changes the contents of the `Mutex`.

`WaitGroup` is a tool for waiting until a given number of tasks have finished. Raise the count with
`add`, have each task call `done` when it finishes, and `wait` until the count reaches 0.

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

If several tasks modify the same value at the same time without going through a `Mutex` or a
channel, the result is not guaranteed. Pass data between tasks through channels where you can, and
share data only when you need to.

## 5. Where tasks switch

Tasks switch cooperatively. A task gives way to other tasks only at these points:

- `(yield)`, `(sleep seconds)`, `(wait handle)`
- A channel operation that has to wait (`send`, `recv`, `select`)
- A socket operation that has to wait (connecting, reading, writing and so on)

The argument of `sleep` is an `f64` number of seconds. Write `(sleep 1.0)`, not `(sleep 1)`.

Tasks run at the same time on several OS threads. However, when `typl` runs a program directly,
only tasks running [compiled](../guide/compile.md) functions go out to other threads. The other
tasks run on a single thread, switching at the points above.

## 6. `thread`: running on a dedicated OS thread

Work that should not hold up other tasks, such as calling a slow C function ([C FFI](../guide/ffi.md)),
is started with `thread`. It is written the same way as `task`, and gets an OS thread of its own.
Wait for it to finish with `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- The handle from `thread` has type `Thread<T>`. Like `wait`, `join` can be called any number of
  times.
- `thread` can only run functions that can be compiled. When running in `typl`, the function it
  calls is compiled on the spot before it runs.

## 7. Tasks and other features

- A `panic` inside a task stops the whole program.
- `throw` does not reach outside a task. A `throw` that would leave the task's body becomes a
  `panic`.
- The output of one `println` is never mixed into the middle of a line with other tasks' output.

## 8. What to read next

- [Tasks and Channels](../reference/functions/concurrency.md): the list of functions
- [Syntax Reference chapter 12](../reference/syntax.md#12-concurrency-tasks): where tasks switch in
  detail, and differences from Go
- [File I/O, Streams and Networking](../guide/io.md): writing a server with tasks
