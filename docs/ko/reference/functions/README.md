<!-- translated-from: docs/ja/reference/functions/README.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 내장 함수

내장 함수, 메서드, 표준 라이브러리의 목록. 문법(특수 형식과 정의 방법)은 [문법 레퍼런스](../syntax.md), 타입 목록은
[타입 목록](../types.md)을 참고한다.

## 호출 형식

호출 형식은 세 가지이다.

- 자유 함수: `(name args...)`
- 인스턴스 메서드: `(name receiver args...)`(첫 번째 인수의 정적 타입으로 해석된다)
- 정적 메서드(연관 함수): `(Type::name args...)`

타입마다 같은 이름의 메서드를 가질 수 있다. `(+ a b)`는 `a`의 타입의 `+`를 호출한다.

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
| [sequences.md](sequences.md) | 쌍 `cons-cell`, S 식 데이터 `Sexpr`, 심볼, 시퀀스 함수, 고차 함수 |
| [collections.md](collections.md) | 문자열, 문자, `Vector`, `HashTable`, `Array`, `BitVector` |
| [option-result.md](option-result.md) | `Option`, `Result`, 오류 타입과 `Error` 트레이트 |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, 산술 트레이트 |
| [printing.md](printing.md) | `print`/`println`/`format`, 프리티 프린터, `print-object`, 프린터 제어 변수 |
| [format.md](format.md) | 서식 지시자 |
| [streams-files.md](streams-files.md) | 스트림, 파일 조작, 경로명, readtable |
| [concurrency.md](concurrency.md) | 태스크, 채널, `WaitGroup`, `Mutex`, `Thread` |
| [network.md](network.md) | TCP, TLS, Unix 도메인 소켓, UDP |
| [system.md](system.md) | 시간, 실행 환경, 구현 도구, `read`/`eval`, 문서 문자열, 매크로 관련 함수 |
