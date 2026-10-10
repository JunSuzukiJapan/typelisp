<!-- translated-from: docs/ja/reference/functions/printing.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# 출력

`print`/`println`/`format`, 인수 하나의 출력 함수, 프리티 프린터, `print-object`, 출력을 제어하는 변수. 서식 지시자 목록은
[format.md](format.md)에 있다. 스트림 읽기와 쓰기는 [스트림과 파일](streams-files.md)에 있다.

## 1. `print` / `println` / `format`

`print`/`println`/`format`은 모두 **서식 지시자(CL의 `format` 지시자)를 해석하는 특수 형식**이다. 첫 번째 인수(`format`은
두 번째)가 **제어 문자열**이며, 각 지시자가 뒤따르는 가변 인수를 차례로 소비한다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | 제어 문자열을 전개해 줄바꿈 없이 표준 출력에 쓴다 |
| `println` | `(println control args...)` | `(string, ...)→Unit` | 같은 것을 끝에 줄바꿈을 붙여 |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CL의 `format`. 전개한 문자열을 반환한다. `dest`가 `true`(CL의 `t`)이면 표준 출력에도 쓰고, `false`(CL의 `nil`)이면 쓰지 않고 반환만 한다 |
| `format`(스트림으로) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | `dest`가 `bool`이 아니면 CL의 스트림 출력 대상이다. 전개한 문자열을 그 스트림에 쓴다. 반환값은 `()`(CL의 `nil`)이며 문자열은 반환하지 않는다 |

`dest`의 타입이 의미를 둘로 나눈다(어느 쪽인지는 정적으로 정해진다). 스트림 형식은 구체 스트림 타입, `:dyn CharOutput`,
`(where (CharOutput S))`로 묶인 타입 변수 어느 것으로도 똑같이 쓸 수 있다. `bool`도 스트림도 아닌 `dest`는 타입 오류이다.

**제어 문자열은 리터럴이어야 한다**(Rust의 `format!`과 같은 제한). 그 안의 지시자가 인수를 몇 개, 어떤 타입으로 받을지
정하므로 실행 중에 만든 문자열은 검사 시에 읽을 수 없다. 리터럴이어야 하므로 **인수의 개수와 타입은 검사 시에
검사된다**. `(println "~d" "x")`와 `(println "~a ~a" 1)`은 검사 시 오류이다. 철자가 틀린 지시자, 닫히지 않은 `~(`, 어느
인수도 답할 수 없는 `~/name/`도 검사 시 오류이다. 검사 규칙은 [format.md](format.md#1-지시자-쓰는-법)에 있다. 직접 만든
문자열을 출력하려면 `(format false ...)`로 만들어 `(println "~a" s)`로 출력한다.

가변 인수는 각자의 타입으로 `Sexpr`에 감싸져 넘겨진다. `i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`, 그리고
사용자 정의 `defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>` 등을 모두 그대로 넘길 수 있다(`(println "~a" my-struct)`가
그대로 동작한다).

`typl file.typl`로 스크립트를 실행하면 **최상위 식의 값은 출력되지 않으므로** 프로그램은 이것들을 호출해서 표준 출력에
쓴다. `print`/`println`/`format`은 호출할 때마다 출력을 내보낸다(파이프를 거쳐도 표준 입력을 읽기 전에 프롬프트가 보이도록).

**`Option<Sexpr>`는 투명하게 출력된다.** S 식 데이터의 타입은 `Option<Sexpr>`이므로 `(some x)` 감싸개는 출력에 나타나지
않고 내용이 그대로 출력된다. 빈 리스트는 `()`로 출력된다. 그 밖의 `Option<T>`는 `(some ...)` / `none`으로 출력된다.
구조체, 열거형, `Vector` 안의 `Option<T>` 필드도 마찬가지이다. `(eval ...)`의 `Result<Option<Sexpr>,…>`는 `(ok 42)`,
`none`이면 `(ok ())`로 출력된다.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; 출력하지 않고 문자열만 얻는다
  (println "~a" s))                   ; => id=42
```

## 2. 인수 하나의 출력 함수

CLHS 22.1.3의 출력 함수. 서식을 전개하는 대신 값 하나를 그대로 출력한다. 스트림은 생략할 수 있다(기본값은
`*standard-output*`).

| 이름 | 형식 | 설명 |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | 다시 읽을 수 있는 형태로 쓰고(`~s`와 같다) `x`를 반환한다 |
| `princ` | `(princ x [stream])` | 사람을 위한 형태로 쓰고(`~a`와 같다) `x`를 반환한다 |
| `write` | `(write x [stream])` | `*print-escape*`가 참이면 `prin1`, 거짓이면 `princ`. `x`를 반환한다 |
| `prin1-to-string` | `(prin1-to-string x)` | 쓰는 대신 문자열을 반환한다(`~s`) |
| `princ-to-string` | `(princ-to-string x)` | 같다(`~a`). `to-string`과 같다 |
| `write-to-string` | `(write-to-string x)` | 같다. `*print-escape*`를 따른다 |

`print`/`println`은 여기에 **들어가지 않는다**. 제어 문자열을 받는 `format`의 줄임이며 CL의 `print`(줄바꿈, `prin1`, 공백)와는
다른 일이므로 각자 자기 이름을 유지한다. 그 결과 **CL의 1인수 `print`는 이 언어에 표기가 없다**. `prin1`을 쓴다.

이것들은 매크로이다. `format`의 가변 인수는 타입 변수를 받지 않으며 호출 지점에서 타입을 알아야 하기 때문이다.

## 3. 표준 입력과 표준 스트림

**표준 입력 읽기**는 전용 함수가 아니라 표준 스트림 `*standard-input*`에 대한 `CharInput` 메서드로 한다:
`(read-line *standard-input*)` / `(read-char *standard-input*)` / `(read-all *standard-input*)`
([스트림의 메서드](streams-files.md#2-메서드)). 표준 출력과 표준 오류도 마찬가지로 `*standard-output*` /
`*error-output*`이 있으며 `(write-line *standard-output* s)`처럼 쓸 수 있다(`print`/`println`/`format`은 서식 전개가 필요할
때의 지름길이며 항상 표준 출력에 쓴다).

## 4. 프리티 프린터

CL의 Lisp Pretty Printer(CLHS 22.2)에 해당한다. **줄 폭에 들어가지 않는 출력을 논리 블록과 조건부 줄바꿈에 따라
나눈다.**

### 4.1 제어 변수

대입할 수 있는 전역 변수. 한 번 `setf`하면 이후의 모든 출력에 영향을 준다. 일시적으로 바꾸려면 `dlet`을 쓴다(6.3).

| 변수 | 타입 | 기본값 | 의미 |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | 참이면 `~a`/`~s`/`~w`와 pretty 지시자가 프리티 프린트 경로를 탄다 |
| `*print-right-margin*` | `int` | `80` | 오른쪽 여백(열 단위). 0은 "여백 없음, 절대 나누지 않음". 음수는 출력 오류 |
| `*print-miser-width*` | `int` | `0` | miser 스타일이 시작되는 폭. 0은 CL의 `nil`(miser 스타일 끔)에 해당한다. 음수는 출력 오류 |

`pprint` 계열과 `pprint-logical-block`은 `*print-pretty*`와 관계없이 항상 프리티 프린트한다(CL의 `pprint` 정의를 따른다).

### 4.2 이미 만들어진 레이아웃(특수 형식)

`print`처럼 특수 형식이므로 인수는 어떤 타입이든 된다.

| 이름 | 형식 | 설명 |
|---|---|---|
| `pprint` | `(pprint x)` | 기본 레이아웃으로 프리티 프린트한다. CL처럼 **먼저 줄바꿈을 쓰고** 끝에는 쓰지 않는다 |
| `pprint-fill` | `(pprint-fill x)` | 각 줄을 들어가는 만큼 채운다. 줄바꿈을 쓰지 않는다 |
| `pprint-linear` | `(pprint-linear x)` | 모든 요소가 한 줄에 들어가지 않으면 **한 줄에 한 요소**. 줄바꿈을 쓰지 않는다 |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | `colinc` 폭(기본 16)의 열을 가진 표. 줄바꿈을 쓰지 않는다. 음수 `colinc`는 오류 |

기본 레이아웃(`pprint`, 그리고 `*print-pretty*` 아래의 `~a`)은 CL의 기본 `*print-pprint-dispatch*`를 따른다. `(quote x)`를
`'x`로 줄이고, `defun`/`let`/`if`/`lambda` 같은 코드 형식은 "첫 줄에 머리와 정해진 개수의 인수, 본체의 나머지는 2열
들여 한 줄에 한 형식"으로 서식화한다. 그 밖의 리스트는 채운다.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 논리 블록 직접 만들기

| 이름 | 형식 | 설명 |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | 논리 블록을 여는 특수 형식. `obj`는 `pprint-pop`이 순회하는 리스트(순회하지 않으면 `()`). `:prefix`와 `:per-line-prefix`는 함께 쓸 수 없다(CL과 같다) |
| `pprint-newline` | `(pprint-newline kind)` | 조건부 줄바꿈. `kind`는 `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | 들여쓰기. `kind`는 `:block`(블록 시작부터) / `:current`(현재 열부터) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | 탭. `kind`는 `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum`과 `colinc`는 음이 아니다(음수면 오류) |
| `pprint-pop` | `(pprint-pop)` | 블록의 리스트에서 다음 요소를 꺼낸다(다 썼으면 `()`) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | 리스트를 다 썼는지 |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | 다 썼으면 바깥의 `loop`에서 `break`한다(매크로) |

논리 블록은 스트림 인수를 받지 않는다. **열려 있는 논리 블록은 암묵적인 상태이다.** 가장 바깥의 `pprint-logical-block`이
그것을 시작하고, 닫힐 때 전체가 서식화되어 한꺼번에 표준 출력에 쓰인다. 열려 있는 동안 `print`/`println`/
`(format true ...)`/`pprint`의 출력은 모두 그 블록에 들어가므로 **내용은 보통의 `print`로 쓰고 나눌 곳만 `pprint-newline`
등으로 표시하면 되어**, 코드는 CL과 거의 같은 모양이 된다.

CL에서 `pprint-exit-if-list-exhausted`는 `pprint-logical-block`에서의 비지역 탈출이지만, 여기서는 **바깥 `loop`에서의
`break`**이다(`pprint-logical-block`은 `block`을 세우지 않는다). CL의 관용구는 어차피 항상 `loop` 안에 두므로 똑같이
읽힌다.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

조건부 줄바꿈의 규칙(CLHS `pprint-newline`):

- `:mandatory`는 항상 나눈다.
- `:linear`는 바깥 논리 블록이 한 줄에 들어가지 않으면 나눈다. 판단은 블록 단위이므로 **한 블록의 `:linear` 줄바꿈은
  모두 함께 나뉜다**(이것이 `pprint-linear`의 "모두 한 줄 또는 한 줄에 한 요소"이다).
- `:fill`은 (a) 다음 구간이 줄의 나머지에 들어가지 않거나, (b) 앞 구간이 한 줄에 들어가지 않았거나, (c) miser 스타일에서
  블록이 한 줄에 들어가지 않으면 나눈다.
- `:miser`는 miser 스타일일 때(블록이 오른쪽 여백에서 `*print-miser-width*` 이내에서 시작될 때)만 `:linear`로 동작한다.

## 5. `print-object`(타입별 출력 표현)

`impl print-object <type>`을 쓰면 `print`/`println`/`format`/`pprint`가 그 타입의 값을 **리스트 안에 중첩되어 있어도** 그
구현으로 출력한다. CL의 총칭 함수 `print-object`(CLHS 22.1.4)에 해당한다.

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| 인수 | 의미 |
|---|---|
| `self` | 출력할 값 |
| `escape` | CL의 `*print-escape*`. `~s`/`prin1`/`pprint`(다시 읽을 수 있는 형태)에서는 `true`, `~a`/`princ`(사람을 위한 것)에서는 `false`. 신경 쓰지 않는 구현은 무시해도 된다 |

반환한 `string`은 그대로 출력에 들어간다. `impl`이 없는 타입은 내장 표현(`#<point x: 1 y: 2>` 형식)으로 출력된다.

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   중첩되어도 동작한다
```

프리티 프린터(4장)와도 함께 쓸 수 있다. `*print-pretty*`가 참이면 구현이 반환한 문자열을 포함한 리스트가 오른쪽 여백에서
나뉜다.

표준 라이브러리 타입의 출력 표현. CL에도 있는 타입은 SBCL과 같은 방식으로 출력된다. REPL이 결과를 보일 때는 `~s`와 같은
표현을 쓴다.

| 타입 | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`, `#("a" "b")` | `#(1 2 3)`, `#(a b)` |
| 튜플 `#{..}` | `#{1 "a"}` | `#{1 a}` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | 같다 |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>`(수는 내부 일련번호) | 같다 |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | 정수(CL의 `get-universal-time` / `get-internal-real-time`의 값) | 같다 |
| 오류 타입(`ParseIntError`, `SimpleError` 등) | `#<simpleerror "boom">` | 메시지만(`boom`) |
| `complex` | `#C(1.0 2.0)` | 같다 |
| `Array<T>` | `#2A((0 0) (0 0))` | 같다 |
| 스트림 | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | 같다 |
| 소켓 | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | 같다 |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>`(서머타임 중에는 끝에 `dst`) | 같다 |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | 같다 |
| `defstruct` 타입 | `#<point x: 1 y: 2>`(필드 이름과 값) | 같다(필드는 `~a`) |

규칙:

- **등록은 정적이다.** `impl`은 보통의 메서드 정의로 타입 검사되므로 타입 이름의 철자 실수나 잘못된 시그니처는 컴파일
  오류이다.
- **제네릭 타입에도 동작한다.** `(impl print-object box<T> (where (print-object T)) ...)`는 타입 인수마다 다른 본체로 간다.
  값은 타입 인수를 포함한 자기 타입(`box<i32>`)을 기억한다. `Vector<T>` 같은 내장 제네릭 타입도 같은 방식으로 동작한다.
- **선택은 출력할 때 이루어진다.** 어느 지시자가 어느 인수를 소비할지는 제어 문자열의 실행 시 내용에 따라 달라지므로
  `~a`와 `~s`의 구별(즉 `escape`)은 출력하는 순간에만 알 수 있다. `print-object` 메서드가 "클래스마다 정의되고 출력할 때
  선택되는" CLOS와 같다.
- **재진입은 내장 표현으로 물러난다.** 구현이 `(format false "~a" self)`로 자기 자신을 출력하면 끝없이 재귀하므로, 출력
  중인 값이 다시 나타나면 내장 표현을 쓴다. 깊이 제한이 아니라 값의 동일성을 보므로 중첩된 자기 참조 구조를 정당하게
  출력하는 것을 방해하지 않는다.
- **모든 스칼라 타입이 이 트레이트를 구현한다.** 이것은 **경계로 쓸 수 있게 하기 위해서이다**. `format`의 가변 인수는 타입
  변수를 받을 수 없으므로 이 경계는 제네릭 코드가 "알 수 없는 타입의 값을 그려도 된다"고 말하는 유일한 방법이다(Rust의
  `T: Display`와 같은 모양). `Array<T>`의 `print-object`가 그 예이다.
- **경계를 만족하지 않는 타입 인수에서는 조용히 내장 표현을 쓴다.** `(impl print-object Array<T> (where (print-object T)))`는
  `Array<i32>`에는 적용되지만 요소가 `print-object` 없는 `defstruct`인 `Array`에는 적용되지 않는다. 배열을 만들기만 해도
  오류가 되는 것은 말이 되지 않으므로 오류로 하지 않는다.
- CL의 또 다른 구조인 `set-pprint-dispatch` / `*print-pprint-dispatch*`(타입 지정자를 키로 하는 실행 시 등록부)는
  **채택하지 않는다**. 그 등록은 검사되지 않으며 정적 타입 언어와 맞지 않는다.

## 6. 출력량 제어

### 6.1 깊이, 길이, 공유

"값을 어디까지 출력할지"를 정하는 CLHS 22.1.1의 제어 변수. 4.1의 셋과 마찬가지로 대입할 수 있는 전역 변수이며,
`*print-pretty*`가 참이든 아니든 `print`/`println`/`format`/`pprint` 모두에 적용된다.

| 변수 | 타입 | 기본값 | 의미 |
|---|---|---|---|
| `*print-level*` | `int` | `0` | 이 깊이 이상으로 중첩된 객체는 `#`로 바뀐다. 출력하는 객체가 깊이 0. 0은 무제한 |
| `*print-length*` | `int` | `0` | 리스트의 요소(그리고 `defstruct`/`defenum` 값의 필드)를 이 개수까지 출력하고 나머지를 `...`로 바꾼다. 0은 무제한 |
| `*print-circle*` | `bool` | `false` | 참이면 출력 전에 값을 훑어 **두 번 이상 나타나는 객체에 레이블을 붙인다**. 처음은 `#n=…`, 이후는 `#n#` |

CL은 "무제한"에 `nil`을 쓰지만 이 언어에는 `nil`이 없으므로 `*print-right-margin*`처럼 **0이 무제한**이다. 음수는 의미가
없으며 출력 오류이다. 기본값은 모두 "제한 없음 / 레이블 없음"으로 CL의 초깃값과 같다.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**순환 구조는 `*print-circle*`이 참일 때만 출력할 수 있다.** 거짓(기본값)일 때 자기 자신을 가리키는 값을 출력하면 프린터가
순환을 계속 따라가 프로세스가 충돌한다. CL도 같다(CLHS는 `*print-circle*`이 거짓일 때 순환 구조의 출력을 정의하지
않는다).

순환은 "`defstruct`의 필드를 `setf`로 자기 자신에게 향하게 하는" 방법으로만 만들 수 있다(`Sexpr` 셀은 만든 뒤 바꿀 수
없으므로 `'(1 2 3)` 같은 리스트는 절대 순환하지 않는다).

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a가 a 자신을 가리킨다
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

레이블은 **출력하는 것마다 1부터 다시 시작한다**(CL과 같다). 순환이 없어도 같은 객체가 두 번 나타나면 `#1=`/`#1#`이
붙어, CL이 정한 대로 "이 둘은 같은 객체"라는 정보를 출력에 남긴다.

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

공유가 없는 값에는 **레이블이 전혀 나오지 않으므로** 이 변수를 참으로 두어도 일상적인 코드의 출력은 바뀌지 않는다.

### 6.2 기수, 대소문자, 읽을 수 있는 형태

| 변수 | 타입 | 기본값 | 의미 |
|---|---|---|---|
| `*print-base*` | `int` | `10` | 정수(고정 폭과 `int`)를 출력하는 기수. 2~36 밖이면 **출력 오류**(CL도 범위를 정한다) |
| `*print-radix*` | `bool` | `false` | 참이면 기수 표시를 붙인다. `#b`/`#o`/`#x`, 그 밖의 기수는 `#NNr`, 10진수는 끝에 `.`. 표시는 부호 **앞에** 온다(`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | 심볼 이름의 대소문자: `:upcase` / `:downcase` / `:capitalize`(CL과 같은 철자). 다른 심볼은 출력 오류 |
| `*print-readably*` | `bool` | `false` | 참이면 다시 읽을 수 있는 형태로 출력한다. 이스케이프를 강제하고 `*print-level*`/`*print-length*`의 생략을 끈다 |
| `*print-lines*` | `int` | `0` | 프리티 프린터가 쓸 수 있는 줄 수. 넘는 부분은 잘리고 CL처럼 끝에 `..`가 붙는다. 0은 무제한. 음수는 출력 오류 |
| `*print-escape*` | `bool` | `true` | `write`/`write-to-string`이 `prin1`을 할지 `princ`를 할지. **이 둘만 읽는다** |
| `*print-array*` | `bool` | `true` | `Vector<T>`와 `Array<T>`가 내용을 보여 줄지. 참이면 CL의 배열 구문(`#(1 2 3)` / `#2A((1 2) (3 4))`), 거짓이면 타입과 모양만 `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

`*print-radix*`가 붙이는 표시는 리더가 다시 읽을 수 있다([문법 레퍼런스](../syntax.md#1-어휘-요소)의 기수 표기).

**`*print-case*`의 기본값이 CL과 다른 이유**: CL의 기본값이 `:upcase`인 것은 CL의 리더가 심볼 이름을 대문자로 저장하기
때문이며, 즉 "저장된 대로"라는 뜻이다. 이 리더는 소문자로 저장하므로 같은 의미의 기본값은 `:downcase`이다.

**`*print-readably*`의 빠진 절반**: CL은 다시 읽을 수 없는 값에 `print-not-readable`을 알리지만, 이 언어에는 알릴
컨디션이 없고, `print-object`가 어떻게든 출력할 수 있는 사용자 타입에 대해 읽을 수 있는지를 판단할 방법도 없다. 이스케이프
강제와 생략 무효화만 있다.

**`write`만 `*print-escape*`를 읽는 이유**: CLHS가 정한 대로 `~s`/`prin1`/`pprint`는 그것을 참으로, `~a`/`princ`는 거짓으로
각자의 호출 동안만 묶는다. 그래서 묶이지 않은 상태로 보는 것은 `write`/`write-to-string`뿐이다. `print-object`의 구현은 이
전역 변수가 아니라 자신의 `escape` 인수를 읽어야 한다. 그 인수가 지시자가 고른 값을 나른다.

**CL에 있고 이 언어에 없는 것**: `*print-gensym*`(인턴되지 않은 심볼이 없다).

### 6.3 일시적인 덮어쓰기

CL은 이것들을 `let`으로 묶지만 이 언어의 `let`은 렉시컬하게 묶으므로 `dlet`을 쓴다([기타](system.md#10-기타)).

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; 제한은 이 출력 하나에만 적용된다
(with-standard-io-syntax (println "~a" x))   ; 모든 것을 표준 값으로 되돌려 출력한다
```

`with-standard-io-syntax`는 모든 프린터 제어 변수를 표준 값으로, `*read-eval*`을 `true`로 해서 본체를 실행한다.
