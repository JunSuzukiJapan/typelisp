<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 트레이트

트레이트는 "이 타입은 이런 연산을 지원한다"는 약속이다. 트레이트를 쓰면 여러 타입이 같은 이름의 연산을 공유할 수
있어서, 그것을 쓰는 함수를 타입마다 따로 쓰지 않아도 된다. Rust의 트레이트와 거의 같은 방식으로 동작한다. 이 장은
[타입의 기초](types.md)를 읽었다고 가정한다.

## 1. 트레이트 정의와 구현

도형의 넓이와 이름을 반환하는 연산을 트레이트 `Shape`로 정의한다.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- 트레이트 이름 뒤의 `()`는 상속하는 트레이트의 목록이다(4절). 없으면 비워 둔다.
- 각 줄은 메서드를 선언한다. `Self`는 "이 트레이트를 구현하는 타입"을 뜻한다.

타입에 트레이트를 구현하려면 `impl`을 쓴다.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

구현한 메서드는 보통 함수처럼 호출한다.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

트레이트가 선언한 메서드를 하나라도 빠뜨리면 `impl`에서 타입 오류가 된다.

## 2. 트레이트 경계: "이 트레이트를 구현한 타입이라면 무엇이든"

제네릭 함수의 타입 매개변수에 `where`로 조건을 붙일 수 있다. 이것을 **트레이트 경계**라고 한다.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

`(where (Shape T))`가 있으므로 본체에서 `T`의 값에 `name`과 `area`를 쓸 수 있다. 경계가 없으면 `T`에 대해 아무것도
알 수 없으므로 호출할 수 없다.

`Shape`를 구현하지 않은 타입을 넘기면 호출하는 곳에서 타입 오류가 된다.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

제네릭 함수는 호출된 타입마다 따로 사본이 만들어진다. 실행 중에 타입을 검사하거나 분기하는 일은 없다.

## 3. 기본 구현

트레이트의 메서드에 본체를 쓰면 `impl`이 그 메서드를 생략했을 때 그 본체가 쓰인다.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe는 기본 구현

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; 여기에 쓴 것이 우선

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. 표준 트레이트 구현하기

표준 라이브러리에도 트레이트가 있다. 구현하면 그것을 쓰는 표준 함수를 자신의 타입에 쓸 수 있게 된다.

| 트레이트 | 구현할 메서드 | 쓸 수 있게 되는 것 |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, `match`의 `(= expr)` 패턴 등 |
| `Ord` | `less` | `less-equal`, `greater` 등. `Ord`는 `Eq`를 상속한다 |
| `print-object` | `print-object` | `println` 등에서 값이 표시되는 방식 |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` 등 |
| `Error` | `message`, `source` | 오류 타입으로 쓰기([오류 처리](errors.md)) |

금액을 나타내는 타입에 `Eq`와 `Ord`를 구현해 보자. `Ord`는 `Eq`를 상속하므로 `Eq`의 `impl`을 먼저 써야 한다.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (Ord의 기본 구현)
```

`print-object`를 구현하면 `println`이 값을 표시하는 방식이 정해진다. 인수 `escape`는 `~s`처럼 다시 읽어 들일 수
있는 형태가 요구될 때 `true`가 된다.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

트레이트 경계와 함께 쓰면 `Ord`를 구현한 어떤 타입에도 쓸 수 있는 함수를 쓸 수 있다.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

300, 900, 100 순서로 `money` 값이 든 `Vector`를 주면 `(some 900 yen)`을 반환한다.

## 5. `:dyn`: 타입이 다른 값을 함께 다루기

`Vector<T>`의 요소는 모두 같은 타입이므로 `circle`과 `rect`의 값을 하나의 `Vector<circle>`에 넣을 수 없다.
"`Shape`를 구현한 무언가"를 함께 다루려면 `:dyn Shape` 타입을 쓴다.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- `:dyn Shape`를 기대하는 곳에 둔 `circle`이나 `rect`의 값은 자동으로 변환된다.
- `(area s)` 호출이 어느 타입의 `area`를 실행할지는 실행 중에 `s`가 담은 것의 타입으로 정해진다.
- `Shape`를 구현하지 않은 타입의 값을 `:dyn Shape`를 기대하는 곳에 두면 타입 오류이다.

2절의 트레이트 경계와 `:dyn` 중에서 고르는 기준:

| | 트레이트 경계(`where`) | `:dyn Trait` |
|---|---|---|
| 호출할 메서드가 정해지는 때 | 실행 전 | 실행 중 |
| 하나의 `Vector`에 타입 섞기 | 불가능 | 가능 |
| 쓸 수 있는 타입 | 제한 없음 | 구조체, 열거형, `int`, `string`, `f64` 등(`bool`, `char`, `symbol`, `i32` 등은 불가) |

쓸 수 있는 타입의 정확한 목록은 [문법 레퍼런스 3.9](../reference/syntax.md#39-deftrait--impl--트레이트-기구)에 있다.

`:dyn`으로 쓸 수 없는 트레이트도 있다. 메서드가 `self` 이외의 인수나 반환값에 `Self`를 쓰는 트레이트(`Eq`의
`equals` 등)이다. 실행할 때까지 타입을 알 수 없으므로 "같은 타입의 값"을 만들어 낼 방법이 없다.

## 6. 제한

- 트레이트의 정의, 그 `impl`, `:dyn`을 통해 그것을 쓰는 코드는 하나의 모듈(파일)에 둔다. 트레이트를 다른 모듈에
  보이게 하는 것은 아직 할 수 없다.
- 타입과 트레이트는 하나의 이름공간을 공유한다. 하나의 모듈 안에서 타입과 트레이트가 같은 이름을 가질 수 없다.

## 7. 다음에 읽을 것

- [매크로](macros.md): 자신만의 구문 정의하기
- [문법 레퍼런스 3.9](../reference/syntax.md#39-deftrait--impl--트레이트-기구): 블랭킷 구현, 연관 타입 등
- [표준 트레이트](../reference/functions/traits.md): 표준 라이브러리의 트레이트 목록
