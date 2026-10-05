<!-- translated-from: docs/ja/README.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# typelisp 문서 (한국어)

typelisp는 정적 타입 Lisp이다. 설치와 빌드 방법은 저장소 최상위의 [README.md](../../README.md)(영어)를
참고한다.

## 튜토리얼

typelisp를 처음 접한다면 순서대로 읽는다.

- [시작하기](tutorial/intro.md): REPL, 함수, 변수, 조건 분기, 반복, 리스트와 `Vector`
- [타입의 기초](tutorial/types.md): 정적 타입, `Option`, `Result`, 구조체, 열거형, 제네릭
- [트레이트](tutorial/traits.md): `deftrait` / `impl`, 트레이트 경계, `:dyn`
- [매크로](tutorial/macros.md): `defmacro`, 준인용, `gensym`, `macrolet`
- [오류 처리](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [동시성](tutorial/concurrency.md): 태스크, 채널, `select`, `Mutex`, `thread`

## 가이드

- [모듈과 파일 구성](guide/modules.md): `use`, `pub`, 파일과 모듈의 대응
- [컴파일](guide/compile.md): JIT, AOT 컴파일로 실행 파일 만들기, 덤프
- [파일 입출력, 스트림, 네트워크](guide/io.md): 파일, 경로명, TCP / TLS / UDP, 이름 해석
- [C FFI](guide/ffi.md): `defffi`로 C 함수 호출하기(콜백과 `def-c-struct`로 다루는 C 구조체 포함)
- [에디터 연동](guide/editors.md): `typl-lsp`와 VS Code / Emacs 설정
- [Common Lisp 사용자를 위해](guide/from-common-lisp.md): CL과의 차이와 CL 코드를 고쳐 쓰는 방법

## 레퍼런스

- [문법 레퍼런스](reference/syntax.md): 어휘, 타입 표기, 정의, 제어 구문, 컴파일, 동시성
- [내장 함수](reference/functions/README.md): 내장 함수, 메서드, 표준 라이브러리
- [타입 목록](reference/types.md): 타입과 각 타입이 구현하는 트레이트
- [오류 메시지](reference/errors.md): 자주 보는 오류의 의미와 고치는 방법

## 에디터 연동

설정 순서는 [에디터 연동 가이드](guide/editors.md)에 있다. 각 에디터의 키 바인딩과 설정 항목은 다음 문서에
정리되어 있다(일본어).

- [Emacs (typelisp-mode)](../../editor/emacs/README.md)
- [VS Code](../../editor/vscode/README.md)
