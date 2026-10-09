<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Common Lisp 사용자를 위해

typelisp는 Common Lisp(CL)의 문법과 많은 함수 이름을 이어받았지만 정적 타입 언어이다. 그래서 CL 코드가 쓴 그대로
동작하지는 않는다. 이 가이드는 CL에 익숙한 사람이 걸려 넘어지기 쉬운 점을 고쳐 쓰는 방법과 함께 모은 것이다.

## 1. `nil`과 `t`가 없다

진리값은 `true`와 `false`이다. `nil`과 `t`는 정의되어 있지 않다.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **조건이 될 수 있는 것은 `bool`뿐이다.** `0`이나 빈 리스트를 조건으로 쓰면 타입 오류이다. "nil 이외에는 모두 참"
  이라는 규칙은 없다.
- **`if`의 else는 생략할 수 없다.** `(if c x)`는 오류이다. else가 필요 없으면 `when` / `unless`를 쓴다.
- **"값이 없음"은 `Option<T>`로 나타낸다.** CL에서 "찾지 못함"을 뜻하는 nil을 반환하던 함수는 여기서는 `(some x)`나
  `none`을 반환한다.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- 빈 리스트 `()`는 문맥에 따라 Unit 타입의 값(아무것도 반환하지 않는 함수의 반환값)이거나 S 식 데이터의 빈 리스트이다.
  `false`와는 다른 값이다.

## 2. 타입 쓰기

함수의 인수와 반환값에는 타입이 필요하다.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; 제네릭 함수
  (unwrap-or (first (iter v)) default))
```

- `(defun f (x) x)`처럼 타입이 없는 정의는 쓸 수 없다.
- `defvar` 같은 전역 변수에도 타입이 필요하다: `(defvar (count int) 0)`.
- `the`는 실행 중의 검사가 아니라 타입 검사기를 위한 주석이다.
- **실행 중에 타입을 조사하는 수단은 없다.** `typep`도 `type-of`도 없다. 모든 값의 타입이 컴파일할 때 정해져 있기
  때문이다. 여러 타입 중 하나를 받으려면 `defenum`으로 합 타입을 만들거나 트레이트를 쓴다.
- `deftype`은 타입 별칭을 정의한다. `(deftype small () '(integer 0 9))`처럼 값의 범위를 나타내는 타입은 만들 수 없다.

기본 정수 타입 `int`는 임의 정밀도이며, CL의 integer처럼 크기에 상한이 없다. 고정 폭 타입 `i8`~`i32`,
`u8`~`u32`도 있다. 64비트 고정 폭 정수 타입은 없다.

## 3. 값으로서의 함수

typelisp는 함수와 변수의 이름공간을 나누지 않는다. 함수 이름을 그대로 값으로 넘길 수 있다. `#'`도 `funcall`도 없다.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; funcall이 아니라 직접 호출한다

(apply-to twice 5)                        ; #'twice가 아니라 twice
```

- `+`나 `1+` 같은 내장 함수도 `(fn (int) int)`처럼 인수의 타입이 정해져 있는 곳에는 그대로 값으로 넘길 수 있다.
  `foldl`이나 `map` 같은 제네릭 함수에 넘길 때는 어느 타입의 `+`인지 알 수 없으므로 `lambda`로 감싼다.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- 시퀀스 함수는 **컬렉션이 먼저, 함수가 나중**이다: `(map it f)`, `(filter it f)`, `(foldl it f init)`. CL의
  `(mapcar f list)`와 반대이다.
- `lambda`에서는 `&optional`과 `&key`를 쓸 수 없다(`&rest`는 쓸 수 있다).
- **함수는 정의하기 전에 호출할 수 없다.** CL에서는 나중에 정의할 함수를 호출할 수 있지만 여기서는
  `no such function`이 된다. 상호 재귀하는 함수는 한쪽을 `defsignature`로 먼저 선언한다.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. 리스트와 Vector

CL의 리스트에 해당하는 것은 **S 식 데이터**이며 그 타입은 `Option<Sexpr>`이다(빈 리스트는 `none`). `(list 1 2 3)`과
`'(a b c)`가 이 타입이다. S 식 데이터는 매크로와 `read`가 다루는 것이며, 보통의 데이터 그릇에는 **`Vector<T>`**를
쓴다.

| 하고 싶은 것 | CL | typelisp |
|---|---|---|
| S 식의 머리와 나머지 | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| S 식 리스트 순회 | `(dolist (x xs) ...)` | 같다 |
| 같은 타입의 요소의 나열 | 리스트나 벡터 | `Vector<T>` |
| 쌍 | `(cons a b)` | `(cons a b)`(타입은 `cons-cell<A,B>`) |
| 사상 | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr`는 `cons`로 만든 쌍 `cons-cell<A,B>`의 접근자이다. S 식 리스트에는 쓸 수 없다.

`Vector`는 CL의 벡터처럼 `#(..)`로 쓸 수 있고, 출력도 `#(..)`이다:

```lisp
(let ((v #(1 2)))
  (push v 3)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #(10 20 30)
```

CL과 다른 점은 세 가지다. 요소의 타입이 모두 같아야 한다(`#(1 "a")`는 타입 오류). 평가할 때마다 새 벡터가 만들어지므로, 바꾸어도 다음 평가에는 영향이
없다(CL에서는 리터럴을 바꾼 결과가 정의되지 않는다). 요소가 없는 `#()`에는 `(the Vector<int> #())`처럼 타입을 붙인다. 다차원 배열
`#2A((1 2) (3 4))`도 CL과 같은 표기다.

`map`, `filter`, `sort`, `find` 같은 시퀀스 함수는 `Iter` 트레이트를 구현한 값에 동작한다. `Vector`는 `(iter v)`로
이터레이터로 만든 뒤에 넘긴다.

## 5. 다중 값이 없다

`values`도 `multiple-value-bind`도 없다. CL에서 여러 값을 반환하는 함수는 쌍이나 구조체를 반환한다.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → `car`가 3, `cdr`가 1인 `cons-cell` |
| `(decode-universal-time t)` → 값 9개 | `decoded-time` 구조체 |
| `(read-from-string s)` → 값, 위치 | `(read-from-string s)`는 값과 위치의 `cons-cell`을 `Result`에 담아 반환한다. 값만 필요하면 `(read s)` |

## 6. 특수 변수(동적 바인딩)가 없다

`let`은 언제나 렉시컬하게 묶는다. `defvar`로 정의한 변수를 `let`으로 다시 묶어도 거기서 호출한 함수에서는 원래 값이
보인다.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; CL에서는 2, typelisp에서는 1
```

`*print-base*` 같은 제어 변수를 일시적으로 바꾸려면 `dlet`을 쓴다. 값을 대입하고, 본체를 어떻게 빠져나가든 원래대로
되돌린다.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet`은 전역 변수 자체를 고쳐 쓰므로 스레드마다의 바인딩이 아니다.

## 7. 컨디션 시스템은 채택하지 않았다

`define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error`, `signal`은 없다. 정적 타입과 잘 맞지
않기 때문이다. 대신 다음 두 가지를 용도에 따라 나누어 쓴다.

- **복구할 수 있는 실패는 `Result<T,E>`를 반환한다.** 호출한 쪽은 `match`로 `ok` / `err`를 나눈다. Rust의 `?` 같은
  줄임 표기는 없다.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **복구할 수 없는 실패(버그)는 `panic`이다.** `(panic "message")`, `unwrap`에 `none`을 넘기기, 0으로 나누기, 범위를
  벗어난 인덱스가 이 부류이며 프로그램이 멈춘다. 멈추기 전에 `unwind-protect`의 뒷정리는 실행된다.

오류 타입은 `Error` 트레이트로 통일되며 `(message e)`로 메시지를 얻는다. 자신만의 오류 타입을 만드는 방법은
[Option, Result, 오류 타입](../reference/functions/option-result.md#3-오류-타입과-error-트레이트)에 있다. `assert`와
`warn`은 CL처럼 쓸 수 있다.

`catch` / `throw` / `unwind-protect`는 있다. 다만 `catch`의 태그는 평가되지 않는 리터럴 심볼(`'done`)로 한정되고,
하나의 태그로 던지는 값의 타입은 하나이다.

## 8. CLOS가 없다

`defclass`, `defgeneric`, 메서드 결합은 없다.

- 데이터 타입은 `defstruct`(구조체)와 `defenum`(합 타입)으로 정의한다.
- `defmethod`는 **첫 번째 인수의 정적 타입**만으로 대상이 정해지는 메서드를 정의한다. 다중 디스패치는 없다.
- 타입에 공통 연산을 주려면 트레이트(`deftrait` / `impl`)를 쓴다. 구체적인 타입이 실행 중에 정해지는 값에는
  `:dyn Trait` 타입을 쓴다([문법 레퍼런스 3.9](../reference/syntax.md#39-deftrait--impl--트레이트-기구)).

`defstruct`의 차이:

- 생성자는 `타입이름::new`이다: `(point::new 1 2)`. `make-point` 같은 이름을 원하면 `(:constructor make-point)`
  옵션으로 만들 수 있다.
- 접근자는 `(x p)` 외에 `p::x`로도 쓸 수 있다. `(setf p::x 5)`로 바꾼다.
- 술어(`point-p`)는 만들어지지 않는다. `:conc-name`, `:type`, `:named`는 없다.
- `:include`는 슬롯을 이어받을 뿐이며, 타입이 부모의 하위 타입이 되지는 않는다.

## 9. 패키지 대신 모듈

패키지는 없다. 이름공간은 모듈이며, 파일이 그 자체로 모듈이다. `pkg:symbol` 대신 `module::name`이라고 쓰고, `use`로
이름을 들여온다([모듈과 파일 구성](modules.md)).

키워드 `:foo`는 있으며 자기 자신으로 평가되는 심볼이다. 패키지가 없으므로 콜론은 이름의 일부이다:
`(symbol->string :foo)`는 `":foo"`를 반환한다.

## 10. 읽기와 문법의 차이

- 대문자와 소문자를 구별하지 않는다(심볼은 읽을 때 소문자가 된다). CL과 같다.
- `#'`는 없다(3절). 복소수 리터럴 `#c(...)`는 읽을 수 없다. 복소수는 `(complex 1.0 2.0)`으로 만든다.
- 확장 `loop`의 절은 키워드로 쓴다: `(loop :for i :from 1 :to 3 :collect i)`. 키워드로 시작하지 않는 `loop`는 단순한
  무한 루프이며 `(break)`나 `(return 값)`으로 빠져나간다. `return`은 가장 안쪽의 루프를 빠져나간다(함수를 빠져나가려면
  `return-from`).
- `format`의 출력 대상은 `false`(문자열을 반환), `true`(표준 출력), 또는 스트림이다. 서식 지시자는 CL과 같다.
- 문자열에서 읽기는 `(read "...")`, 스트림에서 읽기는 `(read-sexpr s)`이다. 둘 다 `Result`를 반환한다.
- `eval`은 받은 식을 타입 검사한 뒤에 평가하고 `Result`를 반환한다. 소스 코드에서와 마찬가지로 전방 참조는 할 수 없다.
- `eval-when`은 없다.
- 함수 이름에 `?`나 `!` 접미사를 쓰지 않는다. 술어는 CL처럼 `-p` / `p`(`zerop`, `sexpr-null`)나 앞에 `is-`를
  붙여(`is-some`) 이름 짓는다.

## 11. 이름이 다른 주요 함수

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read`(스트림에서) | `read-sexpr` |
| `pathname` | `to-pathname` |
| `floor` 등의 2인수 판 | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map`(인수 순서가 반대. 4절) |
| `length`(벡터의) | `len` |
| `hash-table-count` | `count` / `size` |

함수 목록은 [내장 함수](../reference/functions/README.md)에 있다.

## 12. 그 밖에 없는 것

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*`과 `copy-readtable`, `readtable-case`(리더 매크로 자체는 `set-macro-character`로 정의할 수 있다)
- 논리 경로명과 와일드카드 경로명
- `input-stream-p` / `output-stream-p`(스트림의 방향은 타입으로 정해진다)
