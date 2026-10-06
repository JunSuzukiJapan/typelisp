<!-- translated-from: docs/ja/reference/functions/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 표준 트레이트

반복, 비교, 산술을 위한 트레이트. 그 밖의 표준 트레이트는 각자의 장에 있다: `Hash`([HashTable](collections.md#4-hashtablekv)),
`Error`([오류 타입](option-result.md#3-오류-타입과-error-트레이트)), `print-object`
([출력](printing.md#5-print-object타입별-출력-표현)), 스트림 트레이트와 `Pathish`([스트림과 파일](streams-files.md)).
어느 타입이 어느 것을 구현하는지는 [타입 목록](../types.md)에 있다. 트레이트를 정의하는 방법은
[문법 레퍼런스](../syntax.md#39-deftrait--impl--트레이트-기구)에 있다.

## 1. `Iter` 트레이트와 반복

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>`는 각각 `vector-iter<T>`/`hashtable-iter<K,V>`/`array-iter<T>`를 통해 `Iter`를
구현한다(이터레이터는 `(iter collection)`으로 얻는다). `Chan<T>`는 그 자체로 `Iter`이다(`recv`가 `next`의 역할을 한다.
[채널](concurrency.md#2-chant--채널)). `Sexpr` 리스트는 `Iter`를 구현하지 않는다(요소 타입이 한결같지 않다). 자신의 타입에
`Iter`를 구현하면 그대로 `doiter`로 순회할 수 있고 [시퀀스 함수](sequences.md#4-iter에-대한-시퀀스-함수)에 넘길 수 있다.

## 2. `Eq` / `Ord`(비교)

Rust의 `PartialEq`/`PartialOrd`에 해당한다(이름은 `Eq`/`Ord`). 제네릭 함수의 `where` 경계에서 요소 타입을 비교할 수 있음을
요구하는 데 쓴다(`sort`/`member`/`assoc` 등).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; 구현해야 한다
  (not-equals ((self Self) (other Self)) bool             ; 기본 구현
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; Eq를 상속한다
  (less ((self Self) (other Self)) bool)                  ; 구현해야 한다
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

`Eq`를 구현하려면 `equals`만, `Ord`는 `less`만 쓴다. 나머지는 기본 구현이 채운다. `Ord`는 `Eq`를 상속하므로
`impl Ord X` 전에 `impl Eq X`가 필요하다.

각 트레이트 메서드는 그대로 함수로 호출할 수 있다(`where (Eq A)`/`(Ord A)` 경계 안에서, 또는 그것을 구현한 구체 타입에
대해).

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | 같은지(Rust의 `==`) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | 같지 않은지(`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq`를 구현하는 타입: 모든 수 타입(`i8`~`u32` / `f32` / `f64` / `int` / `ratio`), `bool` `char` `string` `symbol`
`complex`, `Sexpr`(`eq`, 즉 동일성. `match`의 값 패턴이 쓴다), `cons-cell<A,B>`(요소가 `Eq`일 때 재귀적으로). `Ord`를
구현하는 타입: 모든 수 타입, `char` `string`, `cons-cell<A,B>`(요소가 `Ord`일 때 사전순으로).

메서드 이름이 내장 연산자(`= /= < <= > >=`)나 `eq`/`lt`와 겹치지 않는 것은 내장된 것을 다시 정의할 수 없기 때문이며, 각
구현은 그것들에 위임한다. 스칼라 비교 연산자 자체는 각 받는 쪽 타입의 내장 메서드이다([수](numbers.md),
[문자열과 문자](collections.md)). 경계 안에서 연산자를 쓰면 트레이트 메서드로 읽힌다(3장).

## 3. 산술 트레이트(`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

제네릭 코드가 "더할 수 있는 타입"을 요구하기 위한 층. **구체 타입의 산술은 내장 연산자를 쓰며**([수](numbers.md)) 이 층을
거치지 않는다.

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; 거리는 항상 int(ash와 같다)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; 메서드 없음. 여섯 가지의 조합
```

**경계 안에서는 연산자를 쓸 수 있다.** 받는 쪽이 `where`로 묶인 타입 변수이면 연산자는 트레이트 메서드로 읽힌다
(`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …).

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

트레이트 메서드의 이름이 `+`가 아닌 것은 `+`가 내장 메서드의 이름이고 `impl`이 그것의 재정의를 거부하기 때문이다
(`cannot redefine built-in method`). `Neg`는 없다. `(- x)`는 `(- (- x x) x)`로 전개되므로 `Sub`로 충분하다.

구현하는 타입: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number`는 모든 수 타입(`complex` 제외), `Bits`는 모든 정수 타입과 `int`.
