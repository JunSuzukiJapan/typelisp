<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 오류 처리

typelisp의 오류 처리는 실패를 두 종류로 나눈다.

| 실패의 종류 | 예 | 나타내는 방법 |
|---|---|---|
| 일어날 수 있는 실패(복구 가능) | 파일이 없다, 입력이 수가 아니다 | `Result<T,E>`를 반환한다 |
| 프로그램의 실수(복구 불가능) | 범위를 벗어난 인덱스, `none`의 `unwrap`, 0으로 나누기 | `panic`으로 멈춘다 |

여기에 더해 여러 함수 호출을 한꺼번에 빠져나가는 `catch` / `throw`와, 본체를 어떻게 빠져나가든 뒷정리를 실행하는
`unwind-protect`가 있다. 이 장은 [타입의 기초](types.md)의 `Result` 절을 읽었다고 가정한다.

## 1. `Result`를 반환하고 `match`로 받기

문자열에서 포트 번호를 읽는 함수이다. 입력이 수가 아닌 경우와 범위를 벗어난 경우, 두 가지로 실패할 수 있다.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

호출하는 쪽은 `match`로 성공과 실패를 나눈다.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- `Result`를 반환하는 함수의 값은 `match`로 `err`의 경우를 처리하지 않으면 쓸 수 없다. 실패 처리를 잊으면 타입
  오류이다.
- `parse-int`의 오류는 `ParseIntError` 타입의 값이다. `(message e)`로 메시지 문자열을 얻는다.

## 2. 실패를 호출한 쪽으로 넘기기

Rust의 `?` 같은 줄임 표기는 없다. `Result`를 반환하는 함수를 차례로 호출할 때는 "실패를 그대로 반환하는" 부분을
`match`로 쓴다.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

실패하지 않는다는 것을 알고 있을 때나, 실패하면 멈춰도 괜찮은 작은 스크립트에서는 `unwrap`으로 안의 값을 꺼낸다.
값이 `err`이면 panic한다. 기본값으로 충분하다면 `unwrap-or`를 쓴다.

## 3. 자신만의 오류 타입 만들기

오류를 문자열이 아니라 타입으로 나타내면 호출하는 쪽이 오류의 종류에 따라 분기할 수 있다. 오류 타입은 `Error`
트레이트를 구현한 보통의 `defenum`이나 `defstruct`이다.

```lisp
(defenum config-error
  (missing string)          ; 설정 항목이 없다
  (invalid string int))     ; 값이 잘못되었다

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message`는 오류의 설명을 반환한다.
- `source`는 이 오류의 원인이 된 다른 오류를 반환한다. 원인이 없으면 `none`이다.

## 4. 다른 종류의 오류를 합치기

하나의 함수가 `parse-int`(`ParseIntError`)와 `check-workers`(`config-error`)를 모두 호출하면 오류 타입이 두 개가
되어 하나의 `Result<T,E>`의 `E`가 될 수 없다. 그럴 때는 `E`를 `:dyn Error`(`Error`를 구현한 어떤 타입의 오류)로
한다. 각 오류는 `as-dyn-error`로 변환한다.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

`"4"`, `"-1"`, `"abc"`를 주면 결과는 다음과 같다.

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

`:dyn`에 대해서는 [트레이트](traits.md)의 5절을 참고한다.

## 5. `panic`: 프로그램의 실수

일어나서는 안 되는 상태에 이르면 `panic`으로 프로그램을 멈춘다.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- `panic`의 타입은 `!`(반환하지 않는다)이므로 어떤 타입이 기대되는 곳에도 쓸 수 있다. 위의 `if`의 두 갈래가 맞는
  것은 그 때문이다.
- 다음 연산도 panic한다. `none`이나 `err`의 `unwrap`, 범위를 벗어난 인덱스의 `get`, 정수의 0으로 나누기.
- `panic`은 프로그램을 멈춘다. 태스크 안에서 일어나도 프로그램 전체가 멈춘다.
- REPL에서는 `panic`이 일어나도 REPL은 끝나지 않고 다음 입력을 기다린다.
- "아직 쓰지 않았다"에는 `(todo)`, "여기에 도달할 리 없다"에는 `(unreachable)`을 쓸 수 있다. 둘 다 panic한다.

`panic`은 `Result`를 대신하는 것이 아니다. 사용자 입력이나 파일이 있는지 같은, 일어날 수 있는 실패에는 `Result`를
쓴다.

## 6. `catch` / `throw`: 함수를 넘어 빠져나가기

`throw`는 사이에 함수 호출이 몇 겹 있든 같은 태그를 가진 바깥의 `catch`까지 단번에 빠져나간다.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

`v`에 음수가 없으면 `validate`는 `"all fine"`을 반환하고, `-7`이 들어 있으면 `check-all` 안에서 `catch`까지 빠져나가
`"negative: -7"`을 반환한다.

- 태그는 `'bad-input`처럼 그냥 심볼로 쓴다.
- **하나의 태그가 나르는 값의 타입은 하나뿐이다.** 위의 예에서 `'bad-input`은 `string`을 나르므로 같은 태그로
  `int`를 throw하면 타입 오류이다. `catch`의 본체의 타입도 태그의 타입과 맞아야 한다.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- 도달할 같은 태그의 `catch`가 없는 `throw`는 오류이다.

함수 안에서 일찍 반환하고 싶을 뿐이라면 `catch` / `throw` 대신 `return-from`을 쓴다. `return-from`은 함수를 넘을 수
없지만, 그 대신 어디로 돌아가는지 소스를 읽으면 알 수 있다.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: 반드시 뒷정리하기

`(unwind-protect 본체 뒷정리)`는 본체를 어떻게 빠져나가든 뒷정리를 실행한다. 정상적으로 끝났을 때, `throw`로
빠져나갔을 때, panic했을 때 모두 그렇다.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

"연 파일은 반드시 닫는다", "잡은 락은 반드시 놓는다" 같은 데 쓴다. 표준 라이브러리의 `with-open-file`과
`with-lock`은 내부에서 `unwind-protect`를 쓴다.

## 8. Common Lisp의 컨디션 시스템에 대해

typelisp는 Common Lisp의 컨디션 시스템(`handler-case`, `restart-case` 등)을 채택하지 않는다. 함수가 어떤 실패를
일으킬 수 있는지가 타입에 나타나지 않아 정적 타입과 잘 맞지 않기 때문이다. 일어날 수 있는 실패는 `Result`로 타입에
쓰고, 제어의 이동은 `catch` / `throw`로 한다.

## 9. 다음에 읽을 것

- [동시성](concurrency.md): 태스크와 채널
- [Option, Result, 오류 타입](../reference/functions/option-result.md): 함수 목록
- [오류 메시지](../reference/errors.md): 자주 보는 오류의 의미와 고치는 방법
