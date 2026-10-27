<!-- translated-from: docs/ja/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# typelisp 문서 (한국어)

typelisp는 정적 타입 Lisp이다. 설치와 빌드 방법은 저장소 최상위의 [README.md](../../README.md)(영어)를
참고한다.

## 특징

### 태스크와 채널

태스크(경량 스레드)를 시작하고, 채널을 통해 태스크끼리 값을 주고받는다. 자세한 내용은 [동시성](tutorial/concurrency.md) 문서를 참조한다.

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### nil 대신 Option과 Result

`nil`은 없다. 값이 없을 수 있으면 `Option<T>`를, 실패할 수 있으면 `Result<T,E>`를 반환하고, `match`로 내용을 꺼낸다. `Option<string>`을 `string`처럼 쓰거나 `match`에서 `none`인 경우를 빠뜨리면 실행하기 전에 타입 오류가 된다. 자세한 내용은 [타입의 기초](tutorial/types.md), [오류 처리](tutorial/errors.md) 문서를 참조한다.

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### 타입 검사되는 매크로

매크로는 Common Lisp와 마찬가지로 `defmacro`, quasiquote, `gensym`으로 작성한다. 전개된 코드도 직접 작성한 코드와 똑같이 타입 검사를 받으며, 타입이 맞지 않으면 매크로를 호출한 위치에서 오류가 난다. 자세한 내용은 [매크로](tutorial/macros.md) 문서를 참조한다.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

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
정리되어 있다.

- [Emacs (typelisp-mode)](../../editor/emacs/README_ko.md)
- [VS Code](../../editor/vscode/README_ko.md)
