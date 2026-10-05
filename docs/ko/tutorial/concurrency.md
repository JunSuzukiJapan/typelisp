<!-- translated-from: docs/ja/tutorial/concurrency.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 동시성

typelisp에서는 **태스크**(경량 스레드)를 시작해서 처리를 동시에 진행하고, 태스크끼리는 **채널**을 통해 값을
주고받는다. 모델은 Go의 고루틴과 채널에 가깝다. 이 장에서는 태스크를 시작하고 결과를 받는 방법, 채널, `select`,
공유 데이터 보호, 전용 OS 스레드를 차례로 다룬다. [타입의 기초](types.md)를 읽었다고 가정한다.

## 1. 태스크를 시작하고 결과를 기다리기

`(task (함수 인수...))`는 함수 호출을 새로운 태스크로 시작한다. 시작한 쪽은 기다리지 않고 다음으로 나아간다. 값은
`Task<T>` 타입의 핸들이고, `(wait 핸들)`은 태스크가 끝나기를 기다려 그 결과를 반환한다.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; 0.1초 기다린다(이 태스크만 멈춘다)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task`는 함수 호출의 형태만 받는다. 인수는 `task`를 쓴 곳에서 평가되고, 새 태스크에서 실행되는 것은 호출뿐이다.
- 여러 식을 실행하려면 `lambda`를 만들어 그 자리에서 호출한다:
  `(task ((lambda () () (println "start") (work))))`
- `wait`는 몇 번이든 호출해도 된다. 결과는 기억된다.
- `wait`하지 않아도 태스크는 실행된다.
- **주 처리가 끝나면 프로그램은 끝난다.** 아직 실행 중인 태스크는 중단된다.

## 2. 채널로 값 주고받기

채널 `Chan<T>`는 태스크끼리 `T` 타입의 값을 주고받는 통로이다. `Chan::new`의 인수는 용량(담을 수 있는 값의
개수)이다. 용량 0의 채널에서는 보내는 쪽과 받는 쪽이 서로 상대가 올 때까지 기다린다.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; 보내기
  (close ch))                  ; 더 이상 보내지 않는다

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; 닫힐 때까지 받는다
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)`는 보낸다. 채널이 가득 차 있으면 빈자리가 생길 때까지 기다린다.
- `(recv ch)`는 받는다. 값이 도착할 때까지 기다린다. 결과는 `Option<T>`이고, 채널이 닫히고 비면 `none`을 반환한다.
- 채널을 `doiter`로 순회하면 닫힐 때까지 값을 계속 받는다. `map`이나 `filter`에 그대로 넘길 수도 있다.
- 닫힌 채널에 `send`하면 panic한다.

### 여러 태스크에 일을 나누기

일을 나르는 채널을 하나 두고 여러 워커(일을 처리하는 태스크)가 거기서 일을 가져가는 것이 흔한 패턴이다. 비어 있는
워커가 다음 일을 가져가므로 느린 일과 빠른 일이 섞여 있어도 일이 자연스럽게 나뉜다.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; 이 일에 걸리는 시간

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; jobs가 닫힐 때까지 하나씩 가져간다
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; 작업 중. 그동안 다른 워커가 다음 일을 가져간다
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; 이 워커가 처리한 일의 수

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
  (close jobs)                    ; 일은 이것으로 전부
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

- A, B, C가 처음 세 개의 일을 하나씩 가져간다.
- 0.1초 뒤 B와 C가 비어서 남은 일을 가져간다. A는 느린 일 1을 처리하는 동안 새 일을 가져가지 않는다.
- 결국 A는 1개, B는 2개, C는 3개를 처리했다. 어느 워커가 어느 일을 가져갈지는 프로그램 어디에도 쓰여 있지 않다.
- `jobs`를 닫으면 각 워커의 `doiter`가 끝나고, 태스크가 끝나고, 각 `wait`가 개수를 반환한다.

`jobs`는 용량 0의 채널이므로 `send`는 어느 워커가 일을 가져갈 때까지 기다린다. 용량을 늘리면 주 태스크는 워커를
기다리지 않고 일을 쌓아 둘 수 있다.

## 3. `select`: 여러 채널을 한꺼번에 기다리기

`select`는 여러 채널 연산 중 먼저 가능해진 것을 실행한다. `(after 초)`는 지정한 시간이 지나면 값을 하나 전달하는
채널이다. `select`와 함께 쓰면 타임아웃이 된다.

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

- `((v (recv ch)) 본체...)`는 받기 갈래이다. `v`에는 `Option<T>`가 들어간다.
- `((send ch x) 본체...)`는 보내기 갈래이다.
- 여러 갈래가 동시에 진행할 수 있으면 그중 하나가 무작위로 선택된다.
- 마지막에 `(else 본체...)`를 쓰면 바로 진행할 수 있는 갈래가 없을 때 `else`가 실행되고 `select`는 기다리지 않는다.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. 공유 데이터 보호하기

여러 태스크가 같은 값을 바꿀 때는 `Mutex<T>`로 보호한다. `with-lock`은 락을 잡고 내용을 변수에 묶어 본체를 실행하며,
본체를 어떻게 빠져나가든 반드시 락을 놓는다. 본체 안에서 그 변수에 `setf`로 대입하면 `Mutex`의 내용이 바뀐다.

`WaitGroup`은 정해진 개수의 태스크가 끝나기를 기다리는 도구이다. `add`로 개수를 늘리고, 각 태스크가 끝날 때
`done`을 호출하게 하고, 개수가 0이 될 때까지 `wait`한다.

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

여러 태스크가 `Mutex`나 채널을 거치지 않고 같은 값을 동시에 바꾸면 결과는 보장되지 않는다. 태스크 사이의 데이터는
가능하면 채널로 넘기고, 공유는 필요할 때만 한다.

## 5. 태스크가 전환되는 곳

태스크는 협조적으로 전환된다. 태스크가 다른 태스크에 차례를 넘기는 것은 다음 지점뿐이다.

- `(yield)`, `(sleep 초)`, `(wait 핸들)`
- 기다려야 하는 채널 연산(`send`, `recv`, `select`)
- 기다려야 하는 소켓 연산(연결, 읽기, 쓰기 등)

`sleep`의 인수는 `f64`의 초 단위 수이다. `(sleep 1)`이 아니라 `(sleep 1.0)`이라고 쓴다.

태스크는 여러 OS 스레드에서 동시에 실행된다. 다만 `typl`로 프로그램을 직접 실행할 때 다른 스레드로 나가는 것은
[컴파일](../guide/compile.md)된 함수를 실행하는 태스크뿐이다. 그 밖의 태스크는 하나의 스레드에서 위의 지점마다
전환되며 실행된다.

## 6. `thread`: 전용 OS 스레드에서 실행하기

느린 C 함수 호출([C FFI](../guide/ffi.md))처럼 다른 태스크를 붙잡아 두면 안 되는 처리는 `thread`로 시작한다.
쓰는 법은 `task`와 같고, 자기만의 OS 스레드를 얻는다. 끝나기를 `join`으로 기다린다.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- `thread`의 핸들은 `Thread<T>` 타입이다. `wait`와 마찬가지로 `join`은 몇 번이든 호출할 수 있다.
- `thread`로 실행할 수 있는 것은 컴파일할 수 있는 함수뿐이다. `typl`에서 실행할 때는 호출하는 함수가 실행 전에 그
  자리에서 컴파일된다.

## 7. 태스크와 다른 기능

- 태스크 안의 `panic`은 프로그램 전체를 멈춘다.
- `throw`는 태스크 밖에 닿지 않는다. 태스크의 본체를 빠져나가려는 `throw`는 `panic`이 된다.
- 하나의 `println`의 출력이 다른 태스크의 출력과 줄 중간에서 섞이는 일은 없다.

## 8. 다음에 읽을 것

- [태스크와 채널](../reference/functions/concurrency.md): 함수 목록
- [문법 레퍼런스 12장](../reference/syntax.md#12-동시성태스크): 태스크가 전환되는 곳의 자세한 내용과 Go와의 차이
- [파일 입출력, 스트림, 네트워크](../guide/io.md): 태스크로 서버 쓰기
