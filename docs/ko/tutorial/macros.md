<!-- translated-from: docs/ja/tutorial/macros.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 매크로

매크로는 프로그램을 받아 프로그램을 반환하는 함수이다. 매크로를 쓰면 함수로는 표현할 수 없는 새로운 구문을 만들 수
있다. typelisp의 매크로는 Common Lisp의 `defmacro`와 같은 방식으로 동작한다. 이 장은 [시작하기](intro.md)의
"리스트(S 식)"를 읽었다고 가정한다.

## 1. 매크로와 함수의 차이

함수는 인수를 **평가한 뒤에** 받는다. 매크로는 인수를 **평가하기 전의 식 그대로**(S 식 데이터로) 받아서 다른 식을
만들어 반환한다. 반환된 식이 매크로 호출을 대신하고, 그 뒤에 비로소 타입 검사되고 실행된다. 이 치환을 **전개**라고
한다.

예를 들어 `unless` 같은 구문은 함수로 쓸 수 없다. 함수라면 조건이 참일 때에도 본체가 먼저 평가되어 버린다.

## 2. `defmacro`와 준인용

조건이 거짓일 때만 본체를 실행하는 `my-unless`를 만들어 보자.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- 매크로의 인수에는 타입을 쓰지 않는다. 인수는 모두 S 식 데이터이다.
- `&rest body`는 나머지 인수를 하나의 리스트로 모아 받는다.
- `` ` ``(준인용)으로 시작하는 식은 쓴 그대로 데이터로 만들어진다. 그 안에서는:
  - `,test`는 변수 `test`의 내용을 그 위치에 넣는다.
  - `,@body`는 리스트 `body`의 요소를 그 위치에 펼쳐 넣는다.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

전개 결과는 `macroexpand-1`로 확인할 수 있다. 매크로를 쓸 때는 먼저 전개 결과를 보는 것이 가장 빠르다.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. 전개 결과도 타입 검사된다

매크로가 반환한 식은 손으로 쓴 식과 똑같이 타입 검사된다.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

오류는 매크로를 호출한 곳에서 보고된다.

`if`의 else를 생략할 수 없다는 규칙과 `if`의 두 갈래가 같은 타입이어야 한다는 규칙은 전개 결과에도 그대로
적용된다. 위의 `my-unless`가 `(progn ,@body ())`로 끝나는 것은 본체의 마지막 식이 어떤 타입이든 `if`의 두 갈래가
`()` 타입이 되도록 하기 위해서이다.

## 4. 이름 충돌과 `gensym`

두 변수의 값을 맞바꾸는 매크로를 그대로 쓰면 다음과 같다.

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

대개는 동작하지만, 호출하는 쪽의 변수 이름이 우연히 `tmp`이면 망가진다.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (맞바뀌지 않았다)
```

전개 결과는 `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`가 되어, 매크로가 만든 `tmp`가 호출하는 쪽의
`tmp`를 가린다.

이를 피하려면 매크로 안에서 쓰는 변수의 이름을 `gensym`으로 만든다. `gensym`은 프로그램 어디에도 쓸 수 없는 새로운
심볼을 반환한다.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Common Lisp와 마찬가지로 typelisp의 매크로는 이름 충돌을 자동으로 막아 주지 않는다(비위생적이다). **매크로가 만드는
바인딩에는 `gensym`을 쓴다**고 기억해 두자.

같은 방식으로, 본체를 지정한 횟수만큼 반복하는 매크로는 다음과 같이 쓸 수 있다.

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. 인수에 따라 다르게 전개하기

매크로 본체는 보통의 typelisp 코드이므로 `if`나 `match`로 인수를 살펴보고 다른 전개 결과를 만들 수 있다. 인수는 S 식
데이터(`Option<Sexpr>`)이고, 빈 리스트는 `none`이다.

모든 조건이 참이면 `true`를 반환하는 `my-and`를 만들어 보자.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; 인수 없음
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; 하나뿐
         `(if ,f (my-and ,@more) false)))             ; 둘 이상
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)`는 리스트의 맨 앞을 `f`에, 나머지를 `more`에 넣는 패턴이다.
- `sexpr-null`은 S 식 데이터가 빈 리스트인지 검사한다.
- 마지막의 `_` 갈래가 필요한 것은 S 식 데이터에 리스트 이외의 형태(수, 문자열 등)가 있고 `match`가 그것들도 덮을
  것을 요구하기 때문이다. `&rest` 인수는 항상 리스트이므로 이 갈래가 실제로 실행되는 일은 없다.
- 매크로는 전개 결과 안에서 자기 자신을 호출할 수 있다. 전개는 매크로 호출이 남지 않을 때까지 반복된다.

## 6. 생략할 수 있는 인수

`&optional`은 생략할 수 있는 인수를 받는다. 기본값을 줄 수 있다.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key`는 키워드 인수를 받는다([문법 레퍼런스 3.14](../reference/syntax.md#314-defmacro--매크로-정의)).

## 7. `macrolet`: 한 곳에서만 쓰는 매크로

하나의 식 안에서만 쓰는 매크로는 `macrolet`으로 정의할 수 있다. 밖에서는 보이지 않는다.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. 주의할 점

- **매크로는 정의한 뒤에만 호출할 수 있다.** 함수와 마찬가지로 파일의 앞쪽에서 정의한다.
- 다른 모듈에서 쓸 수 있게 하려면 `(pub defmacro ...)`로 한다.
- `when`, `unless`, `cond`, `and`, `or`, `dotimes`를 비롯한 표준 구문의 상당수는 매크로로 정의되어 있다. 안의 내용은
  `(macroexpand '(when true 1))`로 볼 수 있다.
- 함수로 쓸 수 있는 것은 함수로 쓴다. 매크로는 값으로 넘길 수 없고, 무엇을 하는지 알려면 전개 결과를 읽어야 한다.

## 9. 다음에 읽을 것

- [오류 처리](errors.md): `Result`, `panic`, `catch` / `throw`
- [매크로 관련 함수](../reference/functions/system.md#8-매크로): `gensym`, `macroexpand` 등
