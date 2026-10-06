<!-- translated-from: docs/ja/tutorial/intro.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 시작하기

REPL에서 식을 평가하는 것부터 시작해 함수, 변수, 조건 분기, 반복, 리스트와 `Vector`를 차례로 다룬다. `typl`을
빌드하는 방법은 [README.md](../../../README.md)를 참고한다.

## 1. REPL 시작하기

인수 없이 시작하면 `typl`은 REPL(대화형 모드)에 들어간다. `typl>` 뒤에 식을 입력하면 그 자리에서 평가되어 값이
출력된다. `:quit`로 REPL을 빠져나온다.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

이후로는 REPL의 입력과 결과를 이 형태로 보인다.

## 2. 식 평가하기

typelisp는 Lisp이므로 식은 괄호로 감싸고 **연산자나 함수 이름을 맨 앞에** 쓴다. `1 + 2`가 아니라 `(+ 1 2)`라고
쓴다.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

수에는 다음과 같은 종류가 있다.

- **정수**의 타입은 `int`이다. 크기에 상한이 없다.
- **소수**의 타입은 `f64`이다. `1.5`나 `2.0`처럼 소수점을 붙여 쓴다.
- `int`와 `f64`를 섞어서 계산할 수는 없다. `(+ 1 2.0)`은 타입 오류이다. 변환하려면 `(as f64 1)`이라고 쓴다.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

정수끼리의 `/`는 소수점 이하를 버린 정수가 된다(Common Lisp처럼 분수가 되지는 않는다). 나머지는 `(mod 7 2)`를
쓴다.

진리값은 `true`와 `false`이다.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. 함수 정의하기

함수는 `defun`으로 정의한다. **인수의 타입과 반환값의 타입은 반드시 쓴다.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)`는 "`int` 타입의 인수 `n`"이라는 뜻이다. 인수가 여러 개이면 `((a int) (b int))`처럼 나열한다.
- 인수 목록 뒤의 `int`가 반환값의 타입이다.
- 본체의 마지막 식의 값이 함수의 반환값이 된다. `return`은 쓰지 않는다.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

타입이 맞지 않는 호출은 **실행되기 전에** 타입 오류로 보고된다. 파일을 실행할 때는 어딘가에 타입 오류가 하나라도
있으면 프로그램은 한 줄도 실행되지 않는다.

인수를 생략할 수 있게 하려면 `&optional`을 쓴다. 기본값을 주면 생략했을 때 그 값이 된다.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

`format`의 첫 번째 인수로 준 `false`는 "출력하지 말고 결과를 문자열로 반환하라"는 뜻이다. `~a`는 차례로 다음
인수로 바뀐다.

## 4. 변수

지역 변수는 `let`으로 만든다.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- `let` 변수의 타입은 초깃값에서 정해진다. 쓸 필요가 없다.
- 하나의 `let` 안의 변수끼리는 서로 참조할 수 없다. 앞의 변수를 써서 다음 변수를 만들려면 `let*`을 쓴다.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

변수의 값을 바꾸려면 `setf`를 쓴다. **대입으로 변수의 타입을 바꿀 수는 없다.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

전역 변수는 `defvar`로 정의한다. 여기서는 타입을 쓴다.

```lisp
(defvar (counter int) 0)
```

## 5. 조건 분기

### if

`(if 조건 참일-때의-식 거짓일-때의-식)`이라고 쓴다. **거짓일 때의 식은 생략할 수 없다.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- 조건에 쓸 수 있는 것은 `bool` 타입의 식뿐이다. `(if 0 ...)`처럼 수를 쓰면 타입 오류이다.
- 참일 때의 식과 거짓일 때의 식은 같은 타입이어야 한다.

거짓일 때 아무것도 하지 않을 때는 `when`을 쓴다(반대는 `unless`).

```lisp
(when (> n 100)
  (println "large")
  (println "really large"))
```

### cond

조건이 세 개 이상이면 `cond`가 읽기 쉽다. 마지막의 `else`는 어느 조건에도 해당하지 않을 때 선택된다.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

값의 모양에 따라 분기하려면 `match`를 쓴다.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_`는 어떤 값에도 들어맞는다. `int`의 값은 셀 수 없이 많으므로 `_` 갈래를 빠뜨리면 모든 경우를 다루지 않았다는
오류가 된다. `match`가 진가를 발휘하는 것은 다음 장 [타입의 기초](types.md)에서 다루는 `Option`이나 직접 정의한
타입을 분해할 때이다.

## 6. 반복

함수는 자기 자신을 호출할 수 있다.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

정해진 횟수만큼 반복하려면 `dotimes`를 쓴다. `i`는 0부터 `n - 1`까지 움직인다.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

`while`, `do`, Common Lisp의 확장 `loop`도 있다. 확장 `loop`의 절 단어는 키워드(`:for`, `:collect` 등)로 쓴다.

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. 리스트와 Vector

### Vector

같은 타입의 값을 나란히 담으려면 `Vector<T>`를 쓴다. `T`는 요소의 타입이다.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; #<vector<int> 3 1 2> 가 출력된다
```

- `(Vector::new)`만으로는 요소의 타입이 정해지지 않으므로 `(the Vector<int> ...)`로 타입을 준다.
- `(push v x)`는 끝에 추가하고, `(get v i)`는 `i`번째 요소를 읽고, `(len v)`는 길이를 준다.
- 범위를 벗어난 인덱스로 `get`하면 프로그램은 오류로 멈춘다.

### lambda와 고차 함수

이름 없는 함수는 `lambda`로 만든다. `defun`과 마찬가지로 인수와 반환값의 타입을 쓴다.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` 등은 `Vector`를 `(iter v)`로 **이터레이터**로 만든 것을 받는다. 컬렉션이 먼저,
함수가 나중이다. 다음 예의 `v`는 위에서 만든 `3 1 2`의 `Vector`이다.

```lisp
(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

요소를 하나씩 처리하려면 `doiter`를 쓴다.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

함수를 인수로 받는 함수는 그 인수의 타입을 `(fn (인수-타입...) 반환-타입)`으로 쓴다. `defun`으로 정의한 함수는
이름 그대로 값으로 넘길 수 있다.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### 리스트(S 식)

`'(1 2 3)`이나 `(list 1 2 3)`으로 만드는 리스트는 **S 식 데이터**이다. 요소의 타입이 같지 않아도 된다.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

S 식 데이터는 주로 매크로([매크로](macros.md))나 `read`에서 프로그램 자체를 다루는 데 쓴다. 요소의 타입이 정해진
데이터에는 `Vector<T>`를 쓴다. S 식 리스트는 `dolist`로 순회할 수 있다.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

두 값의 쌍은 `cons`로 만들고 `car`와 `cdr`로 꺼낸다.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. 파일에 프로그램 쓰기

프로그램은 파일(확장자 `.typl`)에 써서 `typl 파일-이름`으로 실행할 수 있다. 결과를 보이려면 `println`을 쓴다.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println`은 `format`과 같은 지시자로 출력하고 끝에 줄바꿈을 붙인다. `print`는 줄바꿈을 붙이지 않는다.
- `~a`는 값을 사람이 읽기 쉬운 형태로, `~s`는 다시 읽어 들일 수 있는 형태로(문자열이면 `"`가 붙는다) 넣는다.
- 파일은 위에서부터 차례로 읽힌다. **함수는 정의보다 앞에서 호출할 수 없다.**

## 9. 다음에 읽을 것

- [타입의 기초](types.md): `Option`, `Result`, 구조체, 열거형, 제네릭
- [Common Lisp 사용자를 위해](../guide/from-common-lisp.md): Common Lisp를 아는 사람을 위한 차이점 목록
