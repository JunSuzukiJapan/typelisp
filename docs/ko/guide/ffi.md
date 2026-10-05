<!-- translated-from: docs/ja/guide/ffi.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# C FFI (defffi)

이 가이드에서는 typelisp에서 C 함수를 호출하는 방법을 설명한다. 선언할 수 있는 타입의 목록과 제한은
[문법 레퍼런스 3.3](../reference/syntax.md#33-defffi--c-함수-선언ffi)에 있다.

## 1. 함수를 선언하고 호출하기

`defffi`로 C 함수의 이름과 타입을 선언한다.

```lisp
(defffi (c-getpid "getpid") () i32)            ; typelisp 쪽 이름과 C의 심볼 이름
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; libm에서 찾는다
```

호출은 `(unsafe ...)`로 감싼다.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe`가 필요한 것은 선언한 타입이 C 쪽의 실제 타입과 맞는지 컴파일러가 확인할 수 없기 때문이다. `unsafe`를 쓰는
것은 그 확인을 쓰는 사람이 책임진다는 뜻이다. 잊으면 그 사실을 설명하는 오류가 된다.

## 2. 안전한 래퍼 쓰기

`unsafe`를 한곳에 가두고 밖에는 보통의 함수를 보이는 것이 의도된 쓰임새이다.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; 호출하는 쪽에는 unsafe가 필요 없다
(str-len "hello")  ; => 5
```

## 3. 타입의 대응

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | 같은 폭의 정수 |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool`(`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long`(`size_t`, `int64_t` 등도) |
| `ptr` | 아무 포인터(`void *`, `FILE *` 등) |
| `(ptr T)` | `T`를 가리키는 포인터([7절](#7-c-구조체)) |

### 문자열

- 넘긴 `string`은 NUL로 끝나는 C 문자열로 복사되고, 호출이 반환된 뒤 해제된다. 문자열 중간에 NUL이 있으면 오류이다.
- `string`을 반환하는 함수의 결과도 복사된다. C 쪽의 메모리는 해제하지 않는다. 호출한 쪽이 해제해야 하는 문자열을
  반환하는 함수(`strdup` 등)는 결과를 `ptr`로 받아 직접 `free`한다.
- `string`을 반환한다고 선언한 함수가 NULL을 반환하면 오류이다. NULL을 반환할 수 있는 함수(`getenv` 등)의 결과는
  `ptr`로 받는다.

### `c-long` / `c-ulong` / `ptr`

이 타입들은 C와의 경계에서 값을 넘기기 위해서만 있으며 **산술은 없다**. typelisp의 정수로 쓰려면 `as`로 변환한다.

```lisp
(as int (unsafe (c-strlen s)))      ; int는 64비트 값을 하나도 잃지 않는다
(try-as i32 (unsafe (c-strlen s)))  ; i32에 들어가지 않으면 none
(unsafe (c-malloc 16))              ; 정수 리터럴은 그대로 넘길 수 있다
```

`ptr`은 C 함수에 다시 넘기기 위한 값이다. 가리키는 곳을 typelisp 쪽에서 읽을 방법은 없다.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

이 타입들이 나올 수 있는 곳은 함수의 인수, 반환값, 지역 변수뿐이다. 구조체의 필드, 전역 변수, `Vector` 등의 타입
인수가 될 수는 없다.

## 4. 라이브러리 지정하기

`:library`가 없으면 프로세스에 이미 링크되어 있는 것(libc 등)에서 심볼을 찾는다. 다른 라이브러리의 함수에는
`:library`가 필요하다.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- `"sqlite3"` 같은 짧은 이름은 `libsqlite3.dylib`, `libsqlite3.so` 순서로 찾는다.
- `/`가 들어 있는 이름은 경로로 다룬다.
- 선언한 심볼을 찾지 못하면 그 이름을 밝힌 오류가 된다.

## 5. AOT 컴파일

`defffi`를 쓰는 프로그램은 그대로 [`compile-file`](compile.md#3-aot-컴파일로-실행-파일-만들기)로 실행 파일로 만들 수
있다. `:library`로 지정한 라이브러리는 링크할 때 자동으로 추가되므로 `compile-file`에 따로 인수를 줄 필요는 없다.

## 6. 콜백

typelisp 함수를 C 함수에 넘겨서 다시 호출하게 할 수 있다. `defffi`의 인수 타입에 함수 타입을 쓰고, 호출할 때 그 위치에
함수 이름이나 `lambda` 식을 쓴다.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "")는 p를 반환한다
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- 넘길 수 있는 것은 **자유 변수가 없는** 함수뿐이다. 최상위 함수, `lambda`, 지역 `labels` 함수 모두 되지만, 바깥
  범위의 지역 변수를 참조하면 타입 검사 때 오류가 된다. C는 선언한 인수만 넘기므로 포착한 변수를 전달할 방법이 없다.
  상태를 가지려면 전역 변수를 쓴다.
- 함수를 담은 변수는 넘길 수 없다. 그 자리에 함수 이름이나 `lambda` 식을 쓴다.
- 콜백 안의 `panic`이나 `throw`는 C 함수가 반환된 뒤에 호출한 쪽에 전해진다.
- 콜백을 호출할 수 있는 것은 typelisp가 호출한 C 함수가 실행되는 동안뿐이다. `atexit`나 시그널 핸들러 등에서는 쓸 수
  없다.

## 7. C 구조체

구조체의 배열 같은 것을 C 함수에 넘기려면 `def-c-struct`로 C와 같은 배치의 구조체를 선언하고 `unsafe` 안에서
할당한다.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; 최상위의 unsafe 안에서 선언한다

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; 4개, 모두 0
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)`은 `T`를 `n`개 할당하고 `(ptr T)`를 반환한다. `(c-ref p i)`는 `i`번째의 포인터, `p::field`는 필드,
  `(c-deref p)`는 `i32` 같은 스칼라를 가리키는 포인터가 가리키는 값이다. 모두 `setf`로 쓸 수 있다.
- `(as ptr p)`는 `void *`를 받는 C 함수에 넘기기 위해 타입 없는 `ptr`로 바꾼다.
- `item`의 크기(여기서는 8)와 각 필드의 위치는 C와 같은 규칙으로 정해진다.

### 할당한 메모리의 수명

할당한 메모리는 그 함수에서 가장 바깥의 `unsafe`를 빠져나갈 때 해제된다. `panic`이나 `throw`로 빠져나갈 때도
마찬가지이다. 그래서 `(ptr T)` 값을 `unsafe` 밖으로 가지고 나갈 수 없다. `unsafe`의 값으로 하기, 클로저에서
포착하기, `task`에 넘기기, `throw`로 던지기는 모두 타입 오류이다. 밖에서 쓰고 싶은 값은 `unsafe` 안에서 수나
`defstruct`에 복사한다.

`lambda`나 `labels` 함수 안에서 할당할 때는 그 함수 안에 `unsafe`를 쓴다.

### C가 할당한 메모리

C에서 `(ptr T)`로 받은 포인터(`defffi`의 반환값, 콜백의 인수 등)는 `c-alloc`으로 할당한 메모리 안을 가리키지 않으면
오류이다. C가 `malloc`으로 할당한 메모리나 NULL을 받는 함수는 타입 없는 `ptr`로 선언한다.

## 8. 할 수 없는 것

- **가변 인수 함수**(`printf` 등)는 선언할 수 없다. 가변 부분은 고정 인수와 다른 규칙으로 넘겨진다. 쓰는 인수 개수마다
  다른 이름으로 선언한다.
- **구조체를 값으로 넘기거나 반환하는 것**은 할 수 없다. 포인터를 넘기는 함수를 쓴다.
- **제네릭 선언**은 할 수 없다.
- **내장 함수와 같은 이름**은 쓸 수 없다.
- **함수 값으로 넘길 수 없다.** `(map xs c-abs)`처럼 넘길 수는 없으므로 `lambda`로 감싼다.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
