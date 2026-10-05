<!-- translated-from: docs/ja/guide/compile.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 컴파일

아무것도 하지 않으면 typelisp 프로그램은 인터프리터에서 실행된다. 그 밖에 네이티브 코드로 컴파일하는 방법이 두 가지,
환경을 저장하는 방법이 하나 있다. 사양의 자세한 내용은 [문법 레퍼런스 10장](../reference/syntax.md#10-컴파일)에 있다.

| 방법 | 쓰는 법 | 결과 |
|---|---|---|
| JIT 컴파일 | `(compile name)` | 실행 중인 세션의 함수가 네이티브 코드로 바뀐다 |
| AOT 컴파일 | `typl -c src.typl` 또는 `(compile-file "src.typl" "out")` | 단독으로 동작하는 실행 파일 |
| 덤프 | `(dump "file.typld")` | 정의를 저장하고, `typl --image`로 같은 환경에서 다시 시작한다 |

## 1. 준비

컴파일에는 LLVM 22를 쓴다. [README.md](../../../README.md)를 따라 `typl`을 빌드했다면 따로 준비할 것은 없다.

AOT 컴파일로 만드는 실행 파일에는 정적 라이브러리 `libtypelisp_front.a`가 링크된다. 릴리스 빌드의 `typl`(`cargo
install`로 설치한 것 포함)은 이 라이브러리를 내부에 가지고 있으므로 준비할 필요가 없다. 처음 컴파일할 때
`~/.typelisp/lib/<빌드 ID>/`(환경 변수 `TYPELISP_HOME`이 설정되어 있으면 `$TYPELISP_HOME/lib/<빌드 ID>/`)에
라이브러리를 써 내고, 이후로는 그것을 쓴다. `typl --remove-lib`로 지운다(`--others`를 붙이면 다른 버전의 `typl`이 써
낸 것, `--all`을 붙이면 전부). 디버그 빌드의 `typl`은 빌드한 저장소의 `target/debug/`에 있는 라이브러리를 쓴다. 다른
곳에 둔 것을 쓰려면 `typl`을 시작할 때 `--lib-dir`로 그 폴더를 준다(3.2절).
macOS에서는 링크에 Xcode Command Line Tools를 쓴다.

## 2. JIT 컴파일

이미 정의한 함수를 그 자리에서 네이티브 코드로 바꾼다.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; 이후의 호출은 컴파일된 코드를 실행한다
```

- `name`은 평가되지 않는다. 함수 이름을 그대로 쓴다(문자열이 아니다). 메서드는 `(compile point::norm)`처럼 타입
  이름을 붙여 쓴다.
- 호출하는 함수도 함께 컴파일된다.
- **제네릭 함수는 컴파일할 수 없다.** 타입마다의 사본은 쓰이는 곳마다 만들어진다. 대신 그것을 구체적인 타입으로
  호출하는 함수를 컴파일한다.
- `trace`, `step`, `disassemble`, `compile`, `compile-file`, `dump`는 인터프리터의 조작이므로 이것들을 호출하는
  함수는 컴파일할 수 없다. 컴파일하려고 하면 이유를 밝힌 오류가 된다.

컴파일 결과를 보려면 `disassemble`을 쓴다.

```lisp
(disassemble fib)          ; 호스트의 기계어
(disassemble fib true)     ; LLVM IR
```

## 3. AOT 컴파일로 실행 파일 만들기

### 3.1 프로그램 쓰기

시작점으로 **인수를 받지 않는 `main` 함수**를 정의한다.

```lisp
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

파일 끝의 `(main)`은 `typl hello.typl`로 실행했을 때 `main`이 호출되도록 하기 위한 것이다. `compile-file`은 이 마지막
`(main)`을 건너뛰므로 같은 파일을 인터프리터에서도 AOT 컴파일에서도 쓸 수 있다.

### 3.2 컴파일하기

명령줄에서는 `typl -c`를 쓴다(`typl --compile`도 같다).

```sh
$ typl -c hello.typl            # hello가 만들어진다
$ typl -c hello.typl -o fib     # 실행 파일 이름을 fib로 한다
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

`-o`가 없으면 실행 파일 이름은 소스 파일 이름에서 `.typl`을 뗀 것이 되고, 소스 파일과 같은 폴더에 놓인다. 소스 파일
이름이 `.typl`로 끝나지 않으면 `-o`가 필요하다. `-c`(`--compile`)와 함께 `--image`, `--heap-cells`, `--feature`는 줄
수 없다.

REPL이나 프로그램에서 `compile-file`을 호출해도 같은 일을 할 수 있다.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

여러 번 빌드한다면 이 한 줄을 파일에 써 두고 `typl build.typl`로 실행할 수 있다.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

파일 이름은 `build.typl`이 있는 곳이 아니라 **`typl`을 시작한 현재 디렉터리**를 기준으로 해석된다.

`typl`이 찾는 곳이 아닌 다른 곳에 둔 `libtypelisp_front.a`를 링크하려면 `--lib-dir`로 그 폴더를 준다. `typl -c`와
`compile-file` 모두에 적용된다.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

지정한 폴더에 `libtypelisp_front.a`가 없으면 `typl`은 오류로 멈춘다. 이 파일은 함께 빌드한 `typl`에서만 동작한다.
`typl`을 다시 빌드했다면 다시 복사한다.

### 3.3 AOT 컴파일하는 파일에 쓸 수 있는 것

- 시작점 파일의 최상위에 쓸 수 있는 것은 정의(`defun` `defmethod` `defvar` `defconstant` `defstruct` `defenum`
  `deftype` `deftrait` `impl` `defffi`, `(unsafe (def-c-struct ...))`)와 `use` `module`뿐이다. `(println ...)` 같은
  최상위 식은 마지막의 `(main)`을 빼고 쓸 수 없다. 처리는 `main` 안에 쓴다.
- 시작점 파일에는 `defmacro`를 쓸 수 없다. 매크로는 다른 모듈에서 `(pub defmacro ...)`로 정의하고 `use`한다.
- `defsignature`를 포함한 파일은 시작점 파일이든 `use`되는 모듈이든 AOT 컴파일할 수 없다.
- 인수를 받지 않는 `main`이 없으면 컴파일은 오류가 된다.
- `use`한 모듈의 파일도 컴파일되어 하나의 실행 파일로 합쳐진다.
- `defffi`의 `:library`로 지정한 라이브러리는 자동으로 링크된다([C FFI](ffi.md)).
- 표준 라이브러리의 함수는 모두 AOT 컴파일에서 쓸 수 있다. `eval`도 쓸 수 있지만, 그 경우 실행 파일에 타입 검사기와
  인터프리터가 들어가므로 크기가 커지고 시작이 느려진다. `eval`을 호출하지 않는 프로그램에는 들어가지 않는다.

### 3.4 실행 파일의 동작

- `(command-line-args)`는 `typl hello.typl a b`로 실행해도 `./hello a b`로 실행해도 같은 모양의 `Vector<string>`을
  반환한다. 첫 번째 요소는 프로그램 이름이다.
- 종료 코드는 `(exit n)`으로 지정한다. `main`이 정상적으로 반환하면 0이다.
- `panic`하면 메시지를 출력하고 0이 아닌 코드로 끝난다.

## 4. 덤프

현재 세션의 정의를 하나의 파일에 저장하고 다음에는 거기서 시작할 수 있다.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

`typl --image session.typld prog.typl`처럼 파일을 실행할 때도 쓸 수 있다.

- 저장되는 것은 **정의**이다. 세션에서 평가한 식은 저장되지 않는다.
- `compile`한 함수는 컴파일된 형태로 저장된다.
- 전역 변수는 덤프를 쓸 때의 값이 아니라 **초기화 식을 다시 실행해서** 복원된다.
- 덤프는 그것을 쓴 것과 다른 버전의 `typl`에서는 읽을 수 없다(오류가 된다).

파일을 실행하고 거기서 `(dump ...)`하면 그 파일의 정의는 파일 이름의 모듈에 들어간다. `dp.typl`에서 정의한 함수의
이름은 `dp::sq`이고, 다른 파일에서 호출하려면 `pub`이 필요하다([모듈과 파일 구성](modules.md)).

## 5. 컴파일된 모듈 파일에 대해

Common Lisp의 `.fasl`처럼 모듈마다의 컴파일 결과를 파일로 써 내는 형식은 없다. `compile-file`은 소스에서 직접 실행
파일을 만든다. 중간 파일은 남지 않는다.
