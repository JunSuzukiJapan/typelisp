<!-- translated-from: docs/ja/reference/functions/README.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# 내장 함수

내장 함수, 메서드, 표준 라이브러리의 목록. 문법(특수 형식과 정의 방법)은 [문법 레퍼런스](../syntax.md), 타입 목록은
[타입 목록](../types.md)을 참고한다.

## 호출 형식

호출 형식은 세 가지이다.

- 자유 함수: `(name args...)`
- 인스턴스 메서드: `(name receiver args...)`(첫 번째 인수의 정적 타입으로 해석된다)
- 정적 메서드(연관 함수): `(Type::name args...)`

타입마다 같은 이름의 메서드를 가질 수 있다. `(+ a b)`는 `a`의 타입의 `+`를 호출한다.

## `use`가 필요한 이름

표준 라이브러리의 이름 가운데 아무것도 쓰지 않고 어느 모듈에서나 쓸 수 있는 것은 다음과 같다
([구문 레퍼런스 3.10](../syntax.md#310-module--use--이름공간)).

- 기본 타입, `Option`·`Result`·`Vector`, 반복, S 식과 리스트, 열 함수, 제어 매크로, 비교, 산술 트레이트
- Common Lisp의 `COMMON-LISP` 패키지에 있는 이름과 같은 이름의 것(`length`·`format`·`open` 등)
- `Error`·`assert`·`print-object`, `HashTable`·`Array`·`BitVector`, 스트림 트레이트
  `Stream`·`InputStream`·`OutputStream`, `HashTable`의 키가 구현하는 `Hash`
- 이것들의 함수·변수의 타입에 나타나는 타입

그 밖의 이름은 `(use std::net::tcp-connect)`처럼 `use`로 가져오거나 `std::net::tcp-connect`와 같이
경로로 쓴다. 다음 표가 그 이름과 위치이다.

| 모듈 | 이름 |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`、`digit-weight`、`name->char` |
| `std::collections` | `Deque`、`HashSet`、`Scope`、`SortedTable`、`hashtable-iter` |
| `std::env` | `command-line-args`、`getenv`、`home-directory` |
| `std::error` | `WrappedError`、`as-dyn-error`、`assert-eq`、`describe-error`、`wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`、`direction-input`、`direction-output`、`directory-p`、`open-binary`、`open-binary-input`、`open-binary-output`、`open-file`、`open-input`、`open-output`、`read-file-lines`、`read-file-string`、`write-file-string` |
| `std::hash` | `*sxhash-mask*`、`sxhash-string` |
| `std::io` | `ByteInput`、`ByteOutput`、`PeekInput`、`binary-file-stream`、`copy-stream`、`make-peek-stream`、`peek-stream`、`read-lines`、`write-lines` |
| `std::net` | `NetError`、`byte-stream-of`、`char-stream-of`、`datagram`、`socket-byte-stream`、`socket-listener`、`socket-stream`、`tcp-connect`、`tcp-listen`、`tls-connect`、`tls-listen`、`udp-bind`、`udp-socket`、`unix-connect`、`unix-listen`、`with-connection` |
| `std::num` | `ParseFloatError`、`ParseIntError`、`atan2`、`parse-float`、`parse-int` |
| `std::path` | `Pathish`、`pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`、`read-from-string-preserving-whitespace`、`read-sexpr`、`read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`、`right-trim`、`string->utf8`、`trim`、`utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`、`Context`、`Mutex`、`WaitGroup`、`after`、`with-lock` |
| `std::thread` | `Task`、`Thread`、`yield` |
| `std::time` | `internal-time-seconds`、`timezone-daylight-p`、`timezone-offset-seconds` |

## 표 읽는 법

각 장의 표는 "이름, 형식, 타입, 설명"의 열을 가진다. 타입 열은 `(인수-타입,...)→반환-타입`으로 쓴다.

- `T`, `A`, `B` 같은 대문자 한 글자는 타입 변수이다.
- `where Eq A` 같은 주석은 타입 변수가 만족해야 하는 트레이트 경계이다.
- `Iter<A>`는 "`Item`이 `A`인 `Iter`의 어떤 구현"이라는 뜻이다.
- `&optional` / `&key`가 붙은 인수는 생략할 수 있다.

## 장 목록

| 파일 | 내용 |
|---|---|
| [numbers.md](numbers.md) | 정수, 부동소수점 수, 유리수, 복소수, 진리값, 비트 연산, 난수 |
| [sequences.md](sequences.md) | 쌍 `cons-cell`, S 식 데이터 `Sexpr`, 심볼, 시퀀스 함수, 지연 이터레이터 `lazy`, 고차 함수 |
| [collections.md](collections.md) | 문자열, 문자, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, 오류 타입과 `Error` 트레이트 |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, 산술 트레이트, `Push`, `Pop` |
| [printing.md](printing.md) | `print`/`println`/`format`, 프리티 프린터, `print-object`, 프린터 제어 변수 |
| [format.md](format.md) | 서식 지시자 |
| [streams-files.md](streams-files.md) | 스트림, 파일 조작, 경로명, readtable |
| [concurrency.md](concurrency.md) | 태스크, 채널, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix 도메인 소켓, UDP |
| [system.md](system.md) | 시간, 실행 환경, 구현 도구, `read`/`eval`, 문서 문자열, 매크로 관련 함수 |
