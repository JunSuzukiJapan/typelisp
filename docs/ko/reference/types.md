<!-- translated-from: docs/ja/reference/types.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# 타입 목록

typelisp에 있는 타입과 각 타입이 구현하는 표준 트레이트. 타입 쓰는 법은
[문법 레퍼런스 2장](syntax.md#2-타입-표기), 각 타입의 함수와 메서드는 [내장 함수](functions/README.md)에 있다.

## 1. 기본 타입

| 타입 | 내용 | 자세히 |
|---|---|---|
| `int` | 임의 정밀도 정수. 63비트에 들어가는 동안은 즉시값으로 갖고, 넘으면 자동으로 다배정도가 된다. 주석 없는 정수 리터럴의 기본 타입 | [수 3장](functions/numbers.md#3-임의-정밀도-정수-int) |
| `i8` `i16` `i32` | 부호 있는 고정 폭 정수 | [수 1장](functions/numbers.md#1-고정-폭-정수) |
| `u8` `u16` `u32` | 부호 없는 고정 폭 정수 | 위와 같음 |
| `f32` `f64` | IEEE-754 부동소수점 수. 소수 리터럴의 기본은 `f64` | [수 4장](functions/numbers.md#4-부동소수점-수f64--f32) |
| `ratio` | 기약 분수인 유리수 | [수 5장](functions/numbers.md#5-유리수-ratio) |
| `bool` | `true` / `false` | [수 7장](functions/numbers.md#7-진리값) |
| `char` | Unicode 스칼라 값 | [문자](functions/collections.md#2-문자-char) |
| `string` | 변경할 수 없는 문자열 | [문자열](functions/collections.md#1-문자열-string) |
| `symbol` | 심볼. 키워드(`:name`)도 이 타입 | [심볼](functions/sequences.md#3-심볼) |
| `()` | Unit 타입. 값도 `()` | |
| `!` | Never 타입. `panic` 같은 반환하지 않는 식의 타입. 어떤 타입이 기대되는 곳에도 둘 수 있다 | |
| `ptr` `c-long` `c-ulong` | C와 값을 주고받기 위해서만 쓰는 워드. `unsafe` 안에서만 값이 될 수 있고, 나올 수 있는 곳도 제한된다 | [수 2장](functions/numbers.md#2-c-경계의-원시-워드ptr--c-long--c-ulong) |
| `random-state` | 난수 생성기의 상태 | [수 12장](functions/numbers.md#12-난수) |

64비트 정수 타입은 없다. 폭이 상관없는 정수에는 `int`를 쓴다.

## 2. 내장 제네릭 타입

| 타입 | 내용 | 자세히 |
|---|---|---|
| `Option<T>` | 있거나 없는 값. `some` / `none` | [Option과 Result](functions/option-result.md) |
| `Result<T,E>` | 성공 또는 실패. `ok` / `err` | 위와 같음 |
| `Vector<T>` | 늘어나는 배열 | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | 해시 테이블. 키 타입은 `Hash`를 구현해야 한다 | [HashTable](functions/collections.md#4-hashtablekv) |
| `#{T0 T1 ...}` | 튜플(요소 1~12개). 요소는 `t::0`로 읽는다 | [구문 2장](syntax.md#2-타입-표기) |
| `Task<T>` | 태스크 핸들 | [태스크](functions/concurrency.md#1-taskt--태스크-핸들) |
| `Thread<T>` | 전용 OS 스레드에서 실행되는 태스크의 핸들 | [Thread](functions/concurrency.md#7-threadt--전용-os-스레드) |
| `Chan<T>` | 채널 | [채널](functions/concurrency.md#2-chant--채널) |

함수 타입은 `(fn (인수-타입...) 반환-타입)`, 트레이트 객체는 `:dyn Trait`으로 쓴다
([문법 레퍼런스 2장](syntax.md#2-타입-표기)).

## 3. S 식 데이터

| 타입 | 내용 | 자세히 |
|---|---|---|
| `Sexpr` | 비어 있지 않은 S 식. 19개 변형: `int`, `i8`~`u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array`, `tuple` | [S 식 데이터](functions/sequences.md#2-s-식-데이터-sexpr) |
| `Option<Sexpr>` | S 식 데이터 전반. 빈 리스트 `()`는 `none` | 위와 같음 |

## 4. 표준 라이브러리의 타입

표준 라이브러리(prelude)가 `defstruct` / `defenum`으로 정의하는 타입. 직접 쓴 타입과 똑같이 다루어지며, `defstruct`로
할 수 있는 일은 모두 할 수 있다.

| 타입 | 내용 | 자세히 |
|---|---|---|
| `cons-cell<A,B>` | 쌍. `cons`/`car`/`cdr` | [쌍](functions/sequences.md#1-쌍-cons-cellab) |
| `complex` | 복소수(`f64` 성분) | [수 6장](functions/numbers.md#6-복소수-complex) |
| `Array<T>` | 다차원 배열 | [Array](functions/collections.md#5-arrayt다차원-배열) |
| `BitVector` | 고정 길이의 비트 열 | [BitVector](functions/collections.md#6-bitvector비트-벡터) |
| `HashSet<T>` | 중복 없는 요소의 모임 | [HashSet](functions/collections.md#7-hashsett) |
| `SortedTable<K,V>` | 키 순서로 정렬된 표 | [SortedTable](functions/collections.md#8-sortedtablekv) |
| `Deque<T>` | 양쪽 끝에서 넣고 뺄 수 있는 열 | [Deque](functions/collections.md#9-dequet) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | 각 컬렉션의 `iter`가 반환하는 이터레이터 | [Iter](functions/traits.md#1-iter-트레이트와-반복) |
| `lazy::map-iter<I,A,U>` 등 | `lazy` 모듈의 함수가 돌려주는 이터레이터 | [지연 이터레이터](functions/sequences.md#지연-이터레이터lazy-모듈) |
| `WaitGroup` | N개가 끝나기를 기다린다 | [WaitGroup](functions/concurrency.md#4-waitgroup--n개의-완료-기다리기) |
| `Mutex<T>` | 공유 데이터의 상호 배제 | [Mutex](functions/concurrency.md#6-mutext--공유-데이터의-상호-배제) |
| `Context` | 협조적인 취소 | [Context](functions/concurrency.md#8-context--협조적인-취소) |
| `pathname` | 부분으로 나눈 파일 이름 | [경로명](functions/streams-files.md#9-경로명-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | 스트림 | [스트림](functions/streams-files.md#3-구체적인-스트림-타입) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | 합성 스트림 | [합성 스트림](functions/streams-files.md#4-합성-스트림) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | 네트워크 | [네트워크](functions/network.md#1-타입) |
| `ReadOutcome` | `read-sexpr`의 결과. `datum` / `eof` | [스트림](functions/streams-files.md#6-제네릭-함수와-파일-조작) |
| `universal-time` `internal-time` `decoded-time` | 시간 | [시간](functions/system.md#1-시간) |
| `heap-info` | 힙의 현재 상태 | [구현 도구](functions/system.md#51-heap-info의-필드) |

## 5. 오류 타입

`Error`는 타입이 아니라 트레이트이며, 다음 타입이 그것을 구현한다. 모든 종류의 오류를 다루려면 `:dyn Error`라고 쓴다.

| 타입 | 만드는 곳 |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | 파일과 스트림 조작 |
| `NetError` | 네트워크 조작 |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

자세한 내용은 [오류 타입과 Error 트레이트](functions/option-result.md#3-오류-타입과-error-트레이트)에 있다.

## 6. 표준 트레이트의 구현

어느 타입이 어느 트레이트를 구현하는지. 각 트레이트의 메서드는 [표준 트레이트](functions/traits.md)와 맨 오른쪽 열의
장에 있다.

### 6.1 비교, 해시, 출력

| 트레이트 | 구현하는 타입 | 자세히 |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` `#{..}` | [Eq / Ord](functions/traits.md#2-eq--ord비교) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` `#{..}` | 위와 같음 |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` `#{..}` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `#{..}` `Array<T>` `HashSet<T>` `SortedTable<K,V>` `Deque<T>` `Context` `pathname` `universal-time` `internal-time`와 모든 내장 오류 타입 | [print-object](functions/printing.md#5-print-object타입별-출력-표현) |

`cons-cell<A,B>`와 튜플 `#{..}`의 트레이트, 컬렉션의 `print-object`는 요소 타입이 그 트레이트를 구현할 때 쓸 수 있다.

### 6.2 산술

| 트레이트 | 구현하는 타입 |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

자세한 내용은 [산술 트레이트](functions/traits.md#3-산술-트레이트add--sub--mul--div--rem--bits--number)에 있다.

### 6.3 반복

| 트레이트 | 구현하는 타입 |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` `lazy` 모듈의 타입(`lazy::map-iter<I,A,U>` 등) |

### 6.4 스트림

| 타입 | 구현하는 트레이트 |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

모든 스트림은 `Stream`을 구현하고, 입력 스트림은 `InputStream`, 출력 스트림은 `OutputStream`도 구현한다.
`socket-listener`와 `udp-socket`은 `Stream`(`close` / `open-stream-p`)만 구현한다. 자세한 내용은
[스트림](functions/streams-files.md#1-트레이트-계층)에 있다.

### 6.5 기타

| 트레이트 | 구현하는 타입 | 자세히 |
|---|---|---|
| `Error` | 5장의 모든 오류 타입 | [오류 타입](functions/option-result.md#3-오류-타입과-error-트레이트) |
| `Pathish` | `string` `pathname` | [경로명](functions/streams-files.md#91-경로명-지정자-트레이트-pathish) |
