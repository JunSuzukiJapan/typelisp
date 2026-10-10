<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# Tasks and Channels

The vocabulary of tasks (lightweight threads). `task` and `thread`, which start them, and `select`,
which waits on several things, are special forms and are in the
[Syntax Reference](../syntax.md#12-concurrency-tasks). This chapter covers the rest: types, methods
and functions.

Tasks are **cooperative**: a task switches only at the points you write. Tasks run at the same time on
`TYPELISP_THREADS` OS threads (in `typl`, only compiled tasks go out to other threads). Where they
switch and how this differs from Go are in
[Syntax Reference 12.5](../syntax.md#125-where-tasks-switch) and
[12.7](../syntax.md#127-differences-from-go).

## 1. `Task<T>` — handles to tasks

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Waits for completion and returns its value |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **You may `wait` any number of times** (the value is cached). Unlike Rust's `JoinHandle::join`, it
  does not consume the handle, so it can be waited on from several places.
- **A task runs even if you never `wait`.** Dropping the handle does not stop it.
- It is an ordinary value, so it can go into a `Vector<Task<()>>`.
- **When the main task ends, the process ends** (as in Go). Other running tasks are cut off, and the
  cleanup of `unwind-protect` does not run, because this is process exit, not stack unwinding.

## 2. `Chan<T>` — channels

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | A channel of capacity `n`. `0` is a rendezvous (no buffer) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Waits until there is room, then hands the value over |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Waits until a value arrives. `none` once it is closed and empty |
| `close` | `(close ch)` | `(Chan<T>)→()` | Closes it |
| `len` | `(len ch)` | `(Chan<T>)→int` | How many values are in the buffer now |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | The capacity |

**The type argument is given with `the`** (written the same way as `(the Vector<i32> (Vector::new))`).
**The capacity must always be written**: the two cases Go writes as `make(chan int)` and
`make(chan int, 16)` are written `(Chan::new 0)` and `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go's for v := range ch
```

- **A `Chan<T>` is its own iterator** (it implements `Iter`). `recv` returns an `Option<T>`, the same
  type as `Iter::next`, so `doiter` and `map`/`filter`/`foldl` all work on it as they are.
- **`send` on a closed channel panics**, and **a second `close` panics too** (both as in Go). These are
  bugs in the program, not recoverable failures, so they are not `Result`s.
- **Closing a channel that a task is waiting to `send` on makes that task panic** (Go's rule).
- `recv` on a closed channel returns what is left in the buffer, and once it is empty, keeps returning
  `none`.
- `close` is resolved by the receiver's type, so it is a different thing from the `close` of the
  `Stream` trait. `Chan<T>` does not implement `Stream`.
- **A negative capacity panics** (it is not silently rounded to 0).

## 3. `yield` / `sleep` — giving way

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Gives up the rest of its turn (Go's `runtime.Gosched`) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Stops **only that task**. The others keep running |

`sleep` stops a task, not a thread. Only when no task at all can run does it go into an OS `sleep`
until the nearest deadline. `(sleep 0.0)` is CL's "yield for 0 seconds".

As in CL, `sleep` takes **seconds**. Integers are not converted to floating-point numbers
automatically, so CL's `(sleep 1)` is written `(sleep 1.0)` here. A negative value or NaN panics.

## 4. `WaitGroup` — waiting for N completions

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | A group with nothing outstanding |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Adds to the counter. Do it before the work starts |
| `done` | `(done wg)` | `(WaitGroup)→()` | One has finished. At 0, every waiter is released |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Waits until it reaches 0. From any number of tasks |

`(wait wg)` and `(wait task)` are resolved by the receiver's type, so they coexist under one name. If
the counter goes below 0, it panics (`done` called too often, or a negative `add`). As in Go, a group
that is back at 0 can be used again starting with `add`. No update is lost even when the tasks run on
separate OS threads.

**With `Task<T>` available, it is needed less than in Go**: `(doiter (t tasks) (wait t))` is often
enough. It is a tool for work that grows dynamically, or for when you do not want to keep the handles.

```lisp
;; fan-in: start one task per input and join them (this language has no nil channels)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — a channel that delivers after a time

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | A channel that delivers one value after `sec` seconds |

Go's `time.After`. It can be written as it is in the timeout arm of `select`
([Syntax Reference 12.3](../syntax.md#123-select--waiting-on-several-channel-operations-at-once)). Its
capacity is 1, so the sending task can finish even if no one receives.

## 6. `Mutex<T>` — mutual exclusion for shared data

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | An unlocked mutex holding `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Takes the lock (waits) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Releases it. Panics if it is not locked |
| `with-lock` | `(with-lock (x m) body...)` | Macro | Locks, binds the contents to `x`, runs `body`, and **always** releases |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` is not a copy of the value but a "place"** (`symbol-macrolet`). `(setf x 42)` changes the
  contents of the mutex.
- `with-lock` releases with `unwind-protect`, so the lock is released however the body is left: normal
  completion, `throw`, `panic`, or `break`/`return`/`return-from`.
- **Re-entering deadlocks** (it does not panic). The scheduler reports that "nothing can make progress"
  for a task stuck on its own lock.
- **`m::v` touches the contents from outside the lock**, which is undefined in the sense that another
  task may be in the middle of changing it. This is the same position as Go's `sync.Mutex`: in a
  language with no ownership or borrow checking, a static guarantee like `MutexGuard` cannot be built.

## 7. `Thread<T>` — dedicated OS threads

The handle returned by `(thread (f args...))`
([Syntax Reference 12.2](../syntax.md#122-thread--starting-a-task-on-a-dedicated-os-thread)). The
counterpart of `Task<T>`.

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Waits for completion and returns the value (the calling **task** stops. Can be called any number of times; the value is cached) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | The function version of `(thread (f))` (Rust's `std::thread::spawn`) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | The number of the OS thread that is running. Unique within the process, with no meaning beyond "is this the same thread" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | The number of threads the machine can run at once (the default of `TYPELISP_THREADS`). Panics if the OS does not answer |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; a blocking C function
(let ((th (thread (sleepy 500000))))
  ...                                            ; other tasks keep going meanwhile
  (join th))                                     ; => 500000
```

- Calling a blocking C function (`defffi`) stops only that thread.
- A `task` inside a `thread` runs as an ordinary task on other threads.
- It can be used in `typl` too. When interpreting, `(thread (f ...))` and `Thread::spawn` compile the
  function to run on the spot and then run it on the dedicated thread. A `lambda` that refers to local
  variables outside it cannot be compiled on its own and panics
  ([Syntax Reference 12.2](../syntax.md#122-thread--starting-a-task-on-a-dedicated-os-thread)). A `lambda` created inside a compiled
  function can be passed.

## 8. `Context` — cooperative cancellation

Go's `context.Context`. You pass one to work you may want to stop from outside. Stopping is
**cooperative**: `cancel` interrupts nothing; a task or thread notices by checking `is-cancelled`
itself or by receiving on `done`.

| Name | Usage | Type | Meaning |
|---|---|---|---|
| `Context::background` | `(Context::background)` | `()→Context` | A new context to be a root |
| `Context::with-cancel` | `(Context::with-cancel parent)` | `(Context)→Context` | Makes a child of `parent` |
| `Context::with-timeout` | `(Context::with-timeout parent sec)` | `(Context,f64)→Context` | Makes a child of `parent` that cancels itself after `sec` seconds |
| `cancel` | `(cancel ctx)` | `(Context)→()` | Cancels. May be called any number of times |
| `done` | `(done ctx)` | `(Context)→Chan<()>` | A channel that is closed when the context is cancelled |
| `is-cancelled` | `(is-cancelled ctx)` | `(Context)→bool` | Whether it has been cancelled |

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
  (cancel ctx)                          ; job 1, job 2, then stopped
  (sleep 0.1))
```

- **Cancellation reaches the children.** A context made with `with-cancel`/`with-timeout` is
  cancelled along with its parent. It does not go the other way (from child to parent).
- A child made from a context already cancelled starts out cancelled.
- `done` is only closed; no value is sent. A receive returns `none`.
- Each call of `(Context::background)` makes a separate root. Go's `Background()` is a single one
  that cannot be cancelled; here a root can be cancelled too, and that affects only what was made
  from it.
- A context can be passed between tasks and between threads.

## 9. What is not there

- **`Atomic`**. `Mutex` is enough.
- **Task-local variables** (Go does not have them either).
- **nil channels**. The reason and the alternative are in
  [Syntax Reference 12.7](../syntax.md#127-differences-from-go).
