<!-- translated-from: docs/ja/tutorial/types.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 타입의 기초

typelisp는 정적 타입 언어이다. 이 장에서는 타입 검사기가 해 주는 일, 자주 쓰는 타입(`Option`, `Result`, 구조체,
열거형), 제네릭을 설명한다. [시작하기](intro.md)를 읽었다고 가정한다.

## 1. 정적 타입이란

typelisp에서는 모든 식의 타입이 프로그램을 실행하기 전에 정해진다. 타입이 맞지 않는 식은 아무것도 실행되기 전에
오류가 된다.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; 타입 오류

(main)
```

이 파일을 실행하면 `start`도 출력되지 않고 타입 오류로 멈춘다.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

타입을 써야 하는 곳은 함수의 인수와 반환값, 전역 변수, 구조체의 필드이다. `let` 변수의 타입은 초깃값에서 정해진다.

주요 타입:

| 타입 | 값의 예 |
|---|---|
| `int` | `42`, `-7`(임의 정밀도 정수) |
| `i8` `i16` `i32` `u8` `u16` `u32` | 고정 폭 정수 |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | 값을 반환하지 않는 함수의 반환 타입 |

실행 중에 값의 타입을 물어보는 수단(Common Lisp의 `typep`이나 `type-of`)은 없다. 모든 타입이 실행 전에 이미
알려져 있기 때문이다.

## 2. `Option<T>`: 없을 수도 있는 값

typelisp에는 `nil`이 없다. "값이 없을 수도 있다"는 것은 `Option<T>` 타입으로 나타낸다. `Option<T>`의 값은 `T`의
값을 하나 가진 `some`이거나, 아무것도 가지지 않은 `none` 중 하나이다.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>`는 `int`가 아니므로 그대로는 계산에 쓸 수 없다. `(+ (safe-div 10 2) 1)`은 타입 오류이다. 안의 값을
쓰려면 `match`로 `some`과 `none`을 나눈다.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- `(some q)` 갈래에서는 안의 값이 변수 `q`에 묶인다.
- `match`는 갈래가 **모든 경우를 덮는지** 검사한다. `(none)` 갈래를 잊으면 타입 오류이다.

### nil이 없는 이유

많은 언어에서 `nil`(`null`)은 어떤 타입의 값 대신으로도 쓸 수 있다. 그 결과 "값이 없는" 경우의 처리를 잊어도
실행할 때까지 알아차리지 못한다. typelisp에서는 값이 없을 수 있는 곳은 `Option<T>` 타입이 되고, `match`로 `none`의
경우를 처리하지 않으면 타입 검사를 통과하지 못한다. 잊은 경우는 실행 전에 발견된다.

조건도 같은 생각을 따른다. `if`의 조건이 될 수 있는 것은 `bool`뿐이다. Common Lisp의 "`nil` 이외에는 참" 같은
규칙은 없다.

### 자주 쓰는 연산

| 형식 | 의미 |
|---|---|
| `(unwrap-or opt default)` | `some`이면 안의 값, `none`이면 기본값 |
| `(unwrap opt)` | 안의 값을 꺼낸다. `none`이면 프로그램이 멈춘다 |
| `(is-some opt)` / `(is-none opt)` | 어느 쪽인지 검사한다 |

표준 라이브러리 함수 중에는 `Option`을 반환하는 것이 많다. 예를 들어 `position`은 요소가 발견되면 그 위치를
`some`으로, 발견되지 않으면 `none`을 반환한다.

```lisp
(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: 실패할 수 있는 처리

실패할 수 있는 처리는 `Result<T,E>`를 반환한다. 성공하면 `T`의 값을 가진 `ok`, 실패하면 오류 `E`를 가진 `err`이다.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

직접 만든 함수도 `Result`를 반환할 수 있다.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

값이 없는 이유를 설명할 필요가 없으면 `Option`, 실패한 이유를 전하고 싶으면 `Result`를 쓴다. 오류 처리는
[오류 처리](errors.md)에서 자세히 다룬다.

## 4. `defstruct`: 구조체

이름 붙은 필드를 가진 타입은 `defstruct`로 정의한다.

```lisp
(defstruct point
  (x int)
  (y int))
```

정의하면 다음을 쓸 수 있게 된다.

```lisp
(let ((p (point::new 3 4)))     ; 만들기(인수는 필드 순서)
  (println "~a" p::x)           ; 필드 읽기. (x p)도 된다
  (setf p::x 10)                ; 바꾸기
  (println "~a" p))             ; #<point x: 10 y: 4>
```

구조체에 고유한 함수를 주려면 `defmethod`를 쓴다. 첫 번째 인수(`self`)의 타입이 메서드가 속하는 타입을 정한다.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

`self` 인수 대신 타입 이름만 쓰면 `point::origin`으로 호출하는 함수가 된다.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: 여러 모양 중 하나

"원, 직사각형, 점 중 하나"처럼 여러 모양 중 하나인 값은 `defenum`으로 정의한다. 각 모양을 **변형(variant)**이라고
한다. 변형마다 다른 개수와 타입의 값을 가질 수 있다.

```lisp
(defenum shape
  (circle int)        ; 반지름
  (rect int int)      ; 너비와 높이
  (dot))              ; 값을 가지지 않는다
```

값은 `shape::circle`처럼 타입 이름을 앞에 붙여 만든다. `match`에서는 변형 이름으로 분해한다.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

여기서도 `match`는 모든 경우를 덮는지 검사한다. 나중에 `shape`에 변형을 추가하면 그것을 처리하지 않는 `match`가
모두 타입 오류가 되므로 고쳐야 할 곳을 놓치지 않는다.

`(use shape)` 뒤에는 타입 이름 없이 `(rect 5 6)`이라고 쓸 수 있다.

`Option`과 `Result`도 이와 같은 구조로 만든 열거형이다.

## 6. 제네릭

어떤 타입에도 쓸 수 있는 함수는 이름 뒤에 **타입 매개변수** `<T>`를 붙여 정의한다.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

호출할 때 타입은 주지 않는다. `T`는 인수에서 정해진다.

```lisp
(first-or ints 7)          ; T는 int
(first-or names "none")    ; T는 string
(first-or ints "none")     ; 타입 오류: ints는 Vector<int>이므로 T는 int
```

구조체와 열거형도 제네릭으로 만들 수 있다. `Vector<T>`, `Option<T>`, `Result<T,E>`가 이런 타입이다.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

제네릭 함수 안에서는 `T`에 대해 아무것도 알 수 없으므로 `T`의 값을 비교하거나 더할 수 없다. "비교할 수 있는
타입이라면 무엇이든" 같은 조건을 붙이려면 트레이트를 쓴다([트레이트](traits.md)).

## 7. 타입에 다른 이름 붙이기

`deftype`은 타입에 다른 이름을 붙인다.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters`는 `int`의 다른 표기일 뿐 새로운 타입이 아니다. `meters`를 기대하는 곳에 그냥 `int`를 넘겨도 오류가 되지
않는다. 구별하고 싶다면 `(defstruct meters (value int))`처럼 구조체를 만든다.

## 8. 다음에 읽을 것

- [트레이트](traits.md): 타입에 공통 연산을 주기
- [타입 목록](../reference/types.md): 내장 타입과 각 타입이 구현하는 트레이트
