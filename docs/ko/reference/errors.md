<!-- translated-from: docs/ja/reference/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 오류 메시지

`typl`이 내는 주요 오류 메시지의 의미와 고치는 방법.

## 1. 오류 읽는 법

오류는 다음 형식으로 표준 오류에 출력된다.

```text
error: 파일:줄:열: 종류: 메시지
```

`종류`는 오류가 언제 발견되었는지를 알려 준다.

| 종류 | 시점 | 의미 |
|---|---|---|
| `type error` | 실행 전(검사 시) | 타입이나 이름의 실수. 그 형식은 실행되지 않는다 |
| (종류 없음) | 읽기 또는 검사 시 | 괄호의 짝이 맞지 않는 등의 문법 실수, 또는 찾을 수 없는 이름 |
| `panic` | 실행 중 | 복구할 수 없는 실패. `unwind-protect`의 뒷정리를 실행한 뒤 프로그램이 멈춘다 |

`warning:`으로 시작하는 줄은 경고이며 처리는 계속된다.

`파일:줄:열`은 실수가 있는 식을 가리킨다. 표준 라이브러리 함수 안에서 일어난 실행 시 오류는 프로그램이 그 함수를
호출한 곳을 가리킨다. 위치가 없는 오류도 있다(`error: panic: ...` 등).

예:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

`main.typl`의 1번째 줄 24번째 열의 식이 `i32`를 기대하는 곳에서 `string`이었다는 뜻이다.

## 2. 검사 시의 오류

실행 전에 발견되는 실수. 고칠 때까지 그 형식은 실행되지 않는다.

### 2.1 타입

| 메시지 | 의미와 고치는 법 |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | `T` 타입이 필요한 곳에 `U` 타입의 식이 있다. 암묵적 변환은 없다. 수라면 `(as T x)`로 변환한다. `int`와 `i32`도 다른 타입이다 |
| ``integer literal 300 is out of range for u8 (0..=255)`` | 리터럴이 타입에 들어가지 않는다. 잘라 내려면 `(as u8 300)`이라고 쓴다 |
| ``unknown type `foo`: no type of that name is visible here. ...`` | 그 이름의 타입이 없다. 타입은 그것을 쓰는 첫 형식보다 앞에서 정의한다(타입에는 전방 선언이 없다). 타입 변수를 뜻했다면 함수 이름 뒤의 `<foo>` 같은 선언 위치에 쓴다([문법 레퍼런스 3.6](syntax.md#36-defstruct--구조체사용자-정의-타입)) |
| ``cannot infer type argument `t` for `vector::new` `` | 타입 인수를 정할 수 없다. `(the Vector<int> (Vector::new))`처럼 `the`로 타입을 쓴다 |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match`가 모든 변형을 다루지 않는다. 빠진 변형의 갈래나 `_` 갈래를 추가한다 |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | 함수가 요구하는 트레이트를 넘긴 타입이 구현하지 않는다. `(impl Eq pt ...)`를 쓴다([표준 트레이트](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | `:dyn`을 기대하는 곳에 트레이트를 구현하지 않은 타입의 값을 넘겼다. `impl`을 쓴다 |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | 타입을 쓰는 곳에 트레이트 이름을 썼다. `:dyn Error`라고 쓴다 |
| ``if: (if cond then else)`` | `if`의 모양이 잘못되었다. `if`에는 else가 필요하다. 필요 없으면 `when`을 쓴다 |

### 2.2 이름

| 메시지 | 의미와 고치는 법 |
|---|---|
| `no such function: bar` | 그 이름의 함수나 메서드가 없다. 철자를 확인한다 |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | 메서드는 첫 번째 인수의 타입으로 선택된다. 그 이름의 메서드는 있지만 첫 번째 인수의 타입(여기서는 `int`)에는 없다. 메시지 끝에 그 메서드를 가진 타입이 나열된다 |
| `unbound variable: y` | 그 이름의 변수가 없다. 철자와 바인딩의 범위(`let` 밖에서 쓰고 있지 않은지)를 확인한다 |
| ``use: unresolved `nosuch` `` | `use`로 지정한 모듈을 찾을 수 없다. 파일 이름과 모듈 경로의 대응은 [문법 레퍼런스 3.11](syntax.md#311-파일과-모듈의-대응여러-파일-프로젝트)을 참고한다 |
| `unresolved path: c::hidden` | 모듈은 있지만 그 이름이 없거나, `pub`이 없어서 보이지 않는다 |
| `circular module dependency: a -> b -> a` | 모듈끼리 서로 `use`하고 있다. 공통 부분을 다른 모듈로 옮긴다 |
| ``return-from: no enclosing block named `nope` `` | `return-from`에 준 이름의 `block`이 바깥에 없다. 함수의 블록은 그 함수 안에서만 쓸 수 있다 |

### 2.3 호출

| 메시지 | 의미와 고치는 법 |
|---|---|
| `f: expected 1 argument(s), got 2` | 인수의 개수가 맞지 않는다 |
| `f: unknown keyword argument :b` | 함수에 없는 키워드 인수를 넘겼다 |
| `new: expected 1 field(s), got 2` | 구조체 생성자에 넘긴 값의 개수가 필드 수와 맞지 않는다 |
| ``setf: cannot assign to constant `k` `` | `defconstant`로 정의한 이름에 대입했다. 바꿔야 한다면 `defvar`를 쓴다 |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | `defsignature`로 선언한 함수가 정의되지 않았다 |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | `~/name/`으로 호출하는 메서드를 가진 인수 타입이 없다([서식 지시자 5장](functions/format.md#5-name)) |

## 3. 읽기 오류

| 메시지 | 의미와 고치는 법 |
|---|---|
| `unexpected end of input while reading a list` | 닫는 괄호가 모자란다. 위치는 읽기가 끝난 곳(파일 끝 등)을 가리키므로 여는 괄호 쪽을 찾는다 |

## 4. 실행 시의 오류(panic)

| 메시지 | 의미와 고치는 법 |
|---|---|
| `panic: divide by zero` | 정수나 유리수의 0으로 나누기. 부동소수점의 0으로 나누기는 panic하지 않고 `inf`/`NaN`이 된다 |
| `panic: unwrap: called on none` | `none`에 `unwrap`을 적용했다. `match`나 `unwrap-or`로 `none`의 경우를 처리한다 |
| `panic: Vector: index 5 out of bounds` | 범위를 벗어난 인덱스. `len`으로 길이를 확인하거나, 범위를 벗어나면 `none`을 반환하는 함수(`nth`, `pop` 등)를 쓴다 |
| `panic: an integer argument does not fit a fixnum` | 인덱스나 개수를 받는 인수에 63비트에 들어가지 않는 `int`를 넘겼다 |
| `throw: no enclosing (catch 'oops) for this throw` | 같은 태그의 `catch`가 바깥에 없는 상태에서 `throw`가 실행되었다 |
| `panic: <message>` | 프로그램이 `(panic "<message>")`를 호출했다. `assert`가 실패하면 `assertion failed: ...`가 된다 |

`panic`은 태스크 안에서 일어나도 프로세스 전체를 멈춘다([문법 레퍼런스 12.4](syntax.md#124-다른-기능과의-관계)).
복구하고 싶은 실패는 `Result`로 나타낸다([문법 레퍼런스 9장](syntax.md#9-오류-처리-방침)).

## 5. 경고

| 메시지 | 의미 |
|---|---|
| ``warning: redefining function `f` `` | 같은 이름의 함수를 다시 정의했다. 나중의 정의가 유효하다. REPL에서 정의를 고칠 때는 보통 나온다 |
