<!-- translated-from: docs/ja/reference/functions/option-result.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Option, Result, 오류 타입

## 1. `Option<T>` / `Result<T,E>`

생성자: `Option<T>`는 `Some(T)` / `None`. `Result<T,E>`는 `Ok(T)` / `Err(E)`. `E`는 어떤 타입이든 된다. 내장 구체 오류
타입도, `defstruct`/`defenum`으로 직접 쓴 타입도 똑같이 들어간다(3장).

| 이름 | 형식 | Option | Result | 설명 |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | 값을 꺼낸다. `None`/`Err`이면 panic |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | 값, 또는 기본값 |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | `Some`인지 |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | `None`인지 |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | `Ok`인지 |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | `Err`인지 |

생성자는 `Option::some`/`Option::none`/`Result::ok`/`Result::err`이다(`(use option)`/`(use result)` 뒤에는 맨 이름
`some`/`none`/`ok`/`err`).

분기는 `match`로 명시적으로 쓴다. Rust의 `?`에 해당하는 구문은 없다.

## 2. `Option<T>`의 실행 시 표현

Rust와 마찬가지로 **`Option<T>`는 보통 상자를 만들지 않는다**. `some v`는 `v` 그 자체이고 `none`은 빈 리스트의 값이며,
할당도 간접 참조도 없다. `Option<Sexpr>`(빈 리스트가 `none`), `Option<int>`, `Option<string>`, `Option<my-struct>`,
`Option<f64>`, `Option<(fn ...)>`이 모두 이 형태이다.

상자를 쓰는 것은 `T`의 값을 빈 리스트의 값과 구별할 수 없을 때뿐이다.

| `T` | 표현 | 이유 |
|---|---|---|
| `Option<U>`(중첩) | 상자 | 안쪽의 `none`이 바깥쪽의 `none`과 같은 값이 되어 버린다 |
| `()` | 상자 | `()`의 값은 빈 리스트의 값 그 자체이다 |
| `ptr` / `c-long` / `c-ulong` | 상자 | 64비트 전부가 값이라 구별할 여지가 없다 |
| 그 밖 | 상자 없음 | — |

표현은 타입만으로 정해지며 값에서 읽을 수는 없다. 출력할 때는 정적 타입에서 `(some ...)`/`none`을 다시 만들므로
`(format false "~a" opt)`는 `(some 1)`을 출력한다. 제한은 두 가지이다.

- **`:dyn Trait`에 넣을 수 없다**(`(impl Speak Option<int> ...)`를 쓴 `Option<int>`의 값을 `:dyn Speak`에 넘기면 오류).
- `Sexpr`에서의 `(the Option<T> ...)` 다운캐스트는 **생성자를 지정한다**: `(the Option<int> (some x))` /
  `(the Option<int> (none))`. 값 전체를 묶는 형식 `(the Option<int> o)`는 오류이다.

## 3. 오류 타입과 `Error` 트레이트

Rust의 `std::error::Error`를 따라 **`Error`는 타입이 아니라 트레이트이다**. 오류를 나타내는 구체 타입은 용도마다 따로
있고, 각각 `Error`를 구현한다.

| 타입 | 만드는 곳 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | 파일과 스트림 조작([스트림과 파일](streams-files.md)) |
| `NetError` | 네트워크 조작([네트워크](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. CL의 `simple-error`. 무엇이 일어났는지만 말하고 싶을 때의 기본 선택 |
| `WrappedError` | `(wrap-error msg cause)`. 자신의 메시지와 원인을 함께 가지는 타입. `Error` 트레이트에 `source`가 있는 이유 |

`ParseIntError`부터 `NetError`까지는 각각 "메시지 문자열 하나를 가진 변형 하나뿐인 열거형"이며, 타입 이름과 변형 이름이
같다(`(match e ((ParseIntError m) m))`, 만들 때는 `(ParseIntError::ParseIntError "...")`). 특별한 것은 없으며,
`(defstruct my-err (...))` / `(defenum my-err ...)`로 직접 쓴 오류 타입과 똑같이 다루어진다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | 오류 메시지(`Error` 트레이트의 메서드) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | 이 오류가 감싸는 원인. 없으면 `None`(Rust의 `Error::source`) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>`(`E`는 `Error`를 구현) | 구체 오류 타입을 트레이트 객체로 넓힌다 |
| `describe-error` | `(describe-error e)` | `E→string`(`E`는 `Error`를 구현) | 메시지와 `source`를 따라가며 찾은 원인의 사슬. 원인마다 한 줄. CL에는 대응하는 것이 없다(Rust의 "caused by") |

직접 만든 오류 타입에 `Error`를 구현하면 내장 오류와 **똑같이** 다룰 수 있다.

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; 구체 타입을 그대로 E에 넣는다
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; 어떤 종류든 똑같이 다룬다
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

여러 오류 타입을 하나의 `Result`에 모으려면 `Result<T, :dyn Error>`(Rust의 `Box<dyn Error>`에 해당)를 쓰고, 구체 오류는
`as-dyn-error`로 넓힌다. `?`가 없으므로 이 변환은 명시적으로 쓴다.

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**타입과 트레이트는 하나의 이름공간을 공유한다**(Rust와 같다). 하나의 모듈 안에서 `defstruct`/`defenum`과 트레이트는 같은
이름을 가질 수 없고, 타입 위치에 트레이트 이름을 쓰면 "`error` is a trait, not a type — write `:dyn error`"로 보고된다.

복구할 수 없는 실패는 `panic`으로 나타낸다. `panic`과 `catch`/`throw`는
[문법 레퍼런스](../syntax.md#8-비지역-탈출catch--throw--unwind-protect), 오류 처리 방침은
[같은 문서의 9장](../syntax.md#9-오류-처리-방침)을 참고한다.
