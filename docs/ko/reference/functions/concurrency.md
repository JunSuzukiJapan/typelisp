<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# 태스크와 채널

태스크(경량 스레드)의 어휘. 태스크를 시작하는 `task`와 `thread`, 여러 가지를 기다리는 `select`는 특수 형식이며
[문법 레퍼런스](../syntax.md#12-동시성태스크)에 있다. 이 장은 나머지인 타입, 메서드, 함수를 다룬다.

태스크는 **협조적**이며, 태스크는 쓴 곳에서만 전환된다. 태스크는 `TYPELISP_THREADS`개의 OS 스레드에서 동시에
실행된다(`typl`에서는 컴파일된 태스크만 다른 스레드로 나간다). 어디서 전환되는지와 Go와의 차이는
[문법 레퍼런스 12.5](../syntax.md#125-전환이-일어나는-곳)와 [12.7](../syntax.md#127-go와의-차이)에 있다.

## 1. `Task<T>` — 태스크 핸들

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | 완료를 기다려 그 값을 반환한다 |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **`wait`는 몇 번이든 해도 된다**(값은 캐시된다). Rust의 `JoinHandle::join`과 달리 핸들을 소비하지 않으므로 여러
  곳에서 기다릴 수 있다.
- **`wait`하지 않아도 태스크는 실행된다.** 핸들을 버려도 멈추지 않는다.
- 보통의 값이므로 `Vector<Task<()>>`에도 넣을 수 있다.
- **주 태스크가 끝나면 프로세스가 끝난다**(Go와 같다). 실행 중인 다른 태스크는 중단되고, `unwind-protect`의 뒷정리는
  실행되지 않는다. 스택 되감기가 아니라 프로세스 종료이기 때문이다.

## 2. `Chan<T>` — 채널

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | 용량 `n`의 채널. `0`은 랑데부(버퍼 없음) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | 빈자리가 생길 때까지 기다린 뒤 건넨다 |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | 값이 올 때까지 기다린다. 닫히고 비면 `none` |
| `close` | `(close ch)` | `(Chan<T>)→()` | 닫는다 |
| `len` | `(len ch)` | `(Chan<T>)→int` | 지금 버퍼에 있는 값의 개수 |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | 용량 |

**타입 인수는 `the`로 준다**(`(the Vector<i32> (Vector::new))`와 같은 쓰는 법). **용량은 반드시 쓴다.** Go가
`make(chan int)`와 `make(chan int, 16)`으로 쓰는 두 경우를 `(Chan::new 0)`과 `(Chan::new 16)`으로 쓴다.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go의 for v := range ch
```

- **`Chan<T>`는 그 자체로 이터레이터이다**(`Iter`를 구현한다). `recv`는 `Iter::next`와 같은 타입인 `Option<T>`를
  반환하므로 `doiter`와 `map`/`filter`/`foldl`이 모두 그대로 동작한다.
- **닫힌 채널에 `send`하면 panic하고**, **두 번째 `close`도 panic한다**(둘 다 Go와 같다). 복구할 수 있는 실패가 아니라
  프로그램의 버그이므로 `Result`가 아니다.
- **태스크가 `send`하려고 기다리는 채널을 닫으면 그 태스크가 panic한다**(Go의 규칙).
- 닫힌 채널에서 `recv`하면 버퍼에 남은 것을 반환하고, 비고 나면 계속 `none`을 반환한다.
- `close`는 받는 쪽의 타입으로 해석되므로 `Stream` 트레이트의 `close`와는 다른 것이다. `Chan<T>`는 `Stream`을 구현하지
  않는다.
- **음수 용량은 panic한다**(조용히 0으로 반올림되지 않는다).

## 3. `yield` / `sleep` — 양보

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | 자기 차례의 나머지를 양보한다(Go의 `runtime.Gosched`) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | **그 태스크만** 멈춘다. 다른 태스크는 계속 실행된다 |

`sleep`은 스레드가 아니라 태스크를 멈춘다. 실행할 수 있는 태스크가 하나도 없을 때만 가장 가까운 기한까지 OS의
`sleep`에 들어간다. `(sleep 0.0)`은 CL의 "0초 동안 양보"이다.

CL과 마찬가지로 `sleep`은 **초**를 받는다. 정수는 부동소수점 수로 자동 변환되지 않으므로 CL의 `(sleep 1)`은 여기서는
`(sleep 1.0)`이라고 쓴다. 음수나 NaN은 panic한다.

## 4. `WaitGroup` — N개의 완료 기다리기

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | 남은 일이 없는 그룹 |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | 카운터를 늘린다. 일을 시작하기 전에 |
| `done` | `(done wg)` | `(WaitGroup)→()` | 하나가 끝났다. 0이 되면 기다리는 쪽을 모두 풀어 준다 |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | 0이 될 때까지 기다린다. 몇 개의 태스크에서든 |

`(wait wg)`와 `(wait task)`는 받는 쪽의 타입으로 해석되므로 같은 이름으로 공존한다. 카운터가 0 아래로 내려가면
panic한다(`done`을 너무 많이 호출했거나 음수 `add`). Go와 마찬가지로 0으로 돌아온 그룹은 `add`부터 다시 쓸 수 있다.
태스크가 다른 OS 스레드에서 실행되어도 갱신은 사라지지 않는다.

**`Task<T>`가 있으므로 Go만큼 필요하지는 않다.** `(doiter (t tasks) (wait t))`로 충분한 경우가 많다. 동적으로 늘어나는
일이나 핸들을 들고 있고 싶지 않을 때를 위한 도구이다.

```lisp
;; fan-in: 입력마다 태스크를 시작해 합류한다(이 언어에는 nil 채널이 없다)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — 시간이 지나면 전달되는 채널

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | `sec`초 뒤에 값을 하나 전달하는 채널 |

Go의 `time.After`. `select`의 타임아웃 갈래에 그대로 쓸 수 있다
([문법 레퍼런스 12.3](../syntax.md#123-select--여러-채널-연산을-동시에-기다리기)). 용량이 1이므로 아무도 받지 않아도
보내는 태스크는 끝날 수 있다.

## 6. `Mutex<T>` — 공유 데이터의 상호 배제

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | `v`를 가진, 잠기지 않은 뮤텍스 |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | 락을 잡는다(기다린다) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | 놓는다. 잠겨 있지 않으면 panic |
| `with-lock` | `(with-lock (x m) body...)` | 매크로 | 잠그고, 내용을 `x`에 묶어 `body`를 실행하고, **반드시** 놓는다 |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x`는 값의 사본이 아니라 "장소"이다**(`symbol-macrolet`). `(setf x 42)`는 뮤텍스의 내용을 바꾼다.
- `with-lock`은 `unwind-protect`로 놓으므로 정상 종료, `throw`, `panic`, `break`/`return`/`return-from` 중 어떻게
  빠져나가든 락이 놓인다.
- **다시 들어가면 교착 상태가 된다**(panic하지 않는다). 자기 락에 걸려 멈춘 태스크에 대해서는 스케줄러가 "진행할 수
  있는 것이 없다"고 보고한다.
- **`m::v`는 락 밖에서 내용을 건드린다.** 다른 태스크가 바꾸는 중일 수 있다는 의미에서 정의되지 않는다. Go의
  `sync.Mutex`와 같은 입장이다. 소유권과 빌림 검사가 없는 언어에서는 `MutexGuard` 같은 정적 보장을 만들 수 없다.

## 7. `Thread<T>` — 전용 OS 스레드

`(thread (f args...))`([문법 레퍼런스 12.2](../syntax.md#122-thread--전용-os-스레드에서-태스크-시작))가 반환하는
핸들. `Task<T>`의 짝이다.

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | 완료를 기다려 값을 반환한다(멈추는 것은 호출한 **태스크**. 몇 번이든 호출할 수 있고 값은 캐시된다) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | `(thread (f))`의 함수판(Rust의 `std::thread::spawn`) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | 실행 중인 OS 스레드의 번호. 프로세스 안에서 유일하며 "같은 스레드인지" 이상의 의미는 없다 |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | 기계가 동시에 실행할 수 있는 스레드 수(`TYPELISP_THREADS`의 기본값). OS가 답하지 않으면 panic |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; 블로킹하는 C 함수
(let ((th (thread (sleepy 500000))))
  ...                                            ; 그동안 다른 태스크는 계속 진행된다
  (join th))                                     ; => 500000
```

- 블로킹하는 C 함수(`defffi`)를 호출해도 멈추는 것은 그 스레드뿐이다.
- `thread` 안의 `task`는 보통의 태스크로 다른 스레드에서 실행된다.
- `typl`에서도 쓸 수 있다. 인터프리트 중의 `(thread (f ...))`와 `Thread::spawn`은 그 자리에서 실행할 함수를 컴파일한 뒤
  전용 스레드에서 실행한다. 바깥의 지역 변수를 참조하는 `lambda`는 그대로는 컴파일할 수 없어 panic한다
  ([문법 레퍼런스 12.2](../syntax.md#122-thread--전용-os-스레드에서-태스크-시작)). 컴파일된 함수 안에서 만든 `lambda`는 넘길 수 있다.

## 8. `Context` — 협조적인 취소

Go의 `context.Context`. 밖에서 멈추고 싶은 작업에 넘긴다. 멈추는 것은 **협조적**이어서 `cancel`은 아무것도 중단하지 않는다. 태스크나 스레드가 스스로
`is-cancelled`를 보거나 `done`을 수신해서 알아차린다.

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `Context::background` | `(Context::background)` | `()→Context` | 루트가 될 새 컨텍스트 |
| `Context::with-cancel` | `(Context::with-cancel parent)` | `(Context)→Context` | `parent`의 자식을 만든다 |
| `Context::with-timeout` | `(Context::with-timeout parent sec)` | `(Context,f64)→Context` | `parent`의 자식을 만들고, `sec`초 뒤 스스로 취소한다 |
| `cancel` | `(cancel ctx)` | `(Context)→()` | 취소한다. 몇 번 불러도 된다 |
| `done` | `(done ctx)` | `(Context)→Chan<()>` | 취소되면 닫히는 채널 |
| `is-cancelled` | `(is-cancelled ctx)` | `(Context)→bool` | 취소되었는지 |

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
  (cancel ctx)                          ; job 1, job 2 다음에 stopped
  (sleep 0.1))
```

- **취소는 자식에게 전해진다.** `with-cancel`/`with-timeout`으로 만든 컨텍스트는 부모가 취소되면 함께 취소된다. 반대 방향(자식에서 부모로)으로는
  전해지지 않는다.
- 이미 취소된 컨텍스트에서 만든 자식은 처음부터 취소되어 있다.
- `done`은 닫힐 뿐 값은 보내지 않는다. 수신하면 `none`이 돌아온다.
- `(Context::background)`는 부를 때마다 별개의 루트를 만든다. Go의 `Background()`는 하나뿐이고 취소할 수 없지만, 여기서는 루트도 취소할 수
  있으며 그 영향은 거기서 만든 것에만 미친다.
- 태스크 사이에서도 스레드 사이에서도 넘길 수 있다.

## 9. 없는 것

- **`Atomic`**. `Mutex`로 충분하다.
- **태스크 지역 변수**(Go에도 없다).
- **nil 채널**. 이유와 대신 쓰는 법은 [문법 레퍼런스 12.7](../syntax.md#127-go와의-차이)에 있다.
