<!-- translated-from: docs/ja/guide/compile.md @ 00b2198ebe05054debbc1a0d1f9ebf1a9835d7a0 -->
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
(use std::env::command-line-args)
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
args: #(./hello a b)
fib(25) = 75025
```

`-o`가 없으면 실행 파일 이름은 소스 파일 이름에서 `.typl`을 뗀 것이 되고, 소스 파일과 같은 폴더에 놓인다. 소스 파일
이름이 `.typl`로 끝나지 않으면 `-o`가 필요하다. `-c`(`--compile`)와 함께 `--image`, `--feature`는 줄
수 없다.

`typl`에 `--release`를 붙이면 **릴리스 빌드**가 되어 `eprint`/`eprintln`(조사를 위한 표준 오류 출력. [출력](../reference/functions/printing.md))을 인수째 제거합니다. 제거하는 것도 타입 검사는 하므로, 거기에 타입 오류가 있으면 컴파일이 실패합니다. `--release`는 `typl` 전체의 옵션으로, `-c`뿐 아니라 파일 실행, REPL, `typl test`에서도 똑같이 적용됩니다. `--release`를 붙인 `typl` 안에서 호출한 `compile-file`도 릴리스 빌드가 되며, 그렇게 만든 실행 파일 안에서 `eval`한 코드에서도 `eprint`/`eprintln`은 제거됩니다.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # -c 뒤에 써도 같음
```

`-O0` `-O1` `-O2` `-O3`으로 **최적화 수준**을 지정한다. 기본값은 `-O0`이며 최적화하지 않는다. `-O1` 이상에서는 리터럴만 바인딩한 변수를 그 값으로 바꾸거나 상수끼리의 정수 연산을 미리 계산해 두는 등의 간단한 최적화를 한 뒤, 나머지는 LLVM의 같은 이름 수준의 최적화에 맡긴다. 어느 수준에서도 계산 결과는 달라지지 않는다. `--release`와 마찬가지로 `typl` 전체의 옵션이어서, `-c`로 만드는 실행 파일 외에 파일 실행·REPL·`typl test` 안에서 컴파일되는 코드와 그곳에서 호출한 `compile-file`에도 적용된다. 인터프리터가 실행하는 코드는 최적화하지 않는다.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # -c 뒤에 써도 같음
```

REPL이나 프로그램에서 `compile-file`을 호출해도 같은 일을 할 수 있다.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
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

- 시작점 파일의 최상위에 쓸 수 있는 것은 정의(`defun` `defvar` `defparameter` `defconstant`
  `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait` `impl` `defffi`,
  `(unsafe (def-c-struct ...))`)와 `use` `module`뿐이다. `(println ...)` 같은 최상위 식은 마지막의
  `(main)`을 빼고 쓸 수 없다. 처리는 `main` 안에 쓴다.
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

### 3.5 실행 파일이 쓰는 자원

`typl -c`에는 실행 파일이 실행 중에 쓸 수 있는 자원을 지정하는 옵션이 있다.

```sh
$ typl --heap-cells 100000 -c hello.typl    # cons 영역을 100000 셀로 고정한다
$ typl -c hello.typl --max-memory 64M       # 메모리를 최대 64 MiB까지 확보한다
$ typl -c hello.typl --no-cons              # cons 셀을 쓰지 않는다
```

- `--heap-cells N`은 cons 셀 영역의 용량을 `N` 셀로 고정한다. 영역은 그 이상 커지지 않는다. 가비지 컬렉션으로도 빈 셀을 만들 수 없으면 실행 시간 오류로 멈춘다. 지정하지 않으면 65536 셀로 시작해 필요에 따라 커진다.
- `--max-memory SIZE`는 실행 파일이 확보하는 메모리 합계의 상한이다. cons 영역, 문자열, 벡터나 해시 표 같은 값, 런타임 자신이 쓰는 메모리를 모두 센다. `SIZE`는 바이트 수이며 `K` `M` `G`(각각 1024배, 1024²배, 1024³배)를 붙여 쓸 수 있다. 상한을 넘겨 확보하려 하면 그 사실을 표시하고 종료 코드 1로 끝난다. 이 종료는 `catch`로 잡을 수 없다. 지정하지 않으면 상한은 없다.
- `--no-cons`는 cons 셀을 쓰지 않는 실행 파일을 만든다. cons 영역이 없으므로, cons 셀이 필요한 조작을 하는 프로그램은 **컴파일 오류**가 된다. 오류에는 그 조작과, `main`에서 거기까지 닿는 함수의 연쇄가 표시된다. cons 셀이 필요한 것은 `Sexpr` 리스트(인용한 리스트나 준인용으로 만드는 것 포함)나 `&rest` 인수를 만들 때, `read`, `eval`, `macroexpand`를 쓸 때다. 수, 문자열, 벡터, 해시 표, 구조체, 채널 등의 조작과 `print`/`println`/`format`은 cons 셀을 쓰지 않는다. `--heap-cells`와 함께 지정할 수 없으며, `-c` 뒤에 쓴다.

`--max-memory`는 `-c` 뒤에 쓴다. `--heap-cells`는 `-c` 앞에도 뒤에도 쓸 수 있으며, 이때 실행 파일의 cons 영역의 용량이 된다. cons 영역도 메모리 상한에 포함되므로, 영역이 들어가지 않는 상한을 지정하면 프로그램이 시작되기 전에 멈춘다.

### 3.6 WebAssembly용으로 컴파일하기

`--target wasm32-wasip1`을 붙이면 `typl -c`는 실행 파일 대신 WebAssembly 모듈(WASI preview 1 명령)을 만듭니다. `-o`를 생략하면 소스 이름의 `.typl`을 `.wasm`으로 바꾼 이름이 됩니다.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # hello.wasm이 만들어진다
$ wasmtime hello.wasm a b                        # WASI를 지원하는 실행 환경에서 실행한다
args: #(hello.wasm a b)
fib(25) = 75025
```

- `install.sh`로 설치한 `typl`은 그대로 쓸 수 있습니다. `cargo install`로 설치할 때는 준비가 필요합니다(3.7절의 준비도 겸합니다). 먼저 rustup으로 `rustup target add wasm32-wasip1 wasm32-unknown-unknown`을 실행합니다. 그다음 `--features wasm`을 붙여 설치합니다(`cargo install --locked --path . --features wasm`).
  붙이지 않고 설치한 `typl`은 WebAssembly 런타임이 없으므로 `--target`은 오류가 됩니다. 저장소의 디버그 빌드 `typl`을 쓸 때는 대신 저장소에서 `scripts/build-wasm-runtime.sh`를 실행합니다.
- 링크에는 rustup toolchain의 링커(`rust-lld`)를 쓰므로, 컴파일할 때도 rustup이 필요합니다.
- 실행 파일과 똑같이 동작합니다. 표준 출력, 표준 오류, 종료 코드, `(command-line-args)`,
  파일 읽기와 쓰기, `throw`/`catch`/`panic`, 태스크, 채널, `sleep`, `eval`을 쓸 수 있습니다.
  태스크는 하나의 스레드에서 차례로 실행됩니다. 파일은 실행 환경이 모듈에 보여 주는
  디렉터리 안에서만 읽고 쓸 수 있습니다(wasmtime에서는 `--dir`로 지정합니다).
- WebAssembly에 없는 것을 쓰는 프로그램은 **컴파일 오류**가 됩니다. 오류에는 쓴 것과
  `main`에서 그곳에 이르는 함수의 연쇄가 표시됩니다. 쓸 수 없는 것은 소켓(TCP, UDP, Unix 도메인,
  TLS, 이름 해석), OS 스레드(`Thread`), C 함수(`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`,
  `ed`입니다.
- WASI에는 시간대를 알아낼 방법이 없으므로 지역 시간을 알 수 없습니다. `zone`을 생략한 `decode-universal-time`·`encode-universal-time`과 `get-decoded-time`은 실행 시 panic합니다. `zone`을 주면 쓸 수 있습니다.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>`, `--release`는 그대로 쓸 수 있습니다.

### 3.7 브라우저나 Node.js에서 실행하기

`--target wasm32-unknown-unknown`을 붙이면 `typl -c`는 JavaScript에서 불러와 실행하는 WebAssembly 모듈을 만듭니다. 모듈(`hello.wasm`) 옆, 같은 폴더에 그것을 불러와 실행하는 JavaScript(`hello.mjs`)가 만들어집니다. 준비는 3.6절과 같습니다.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # hello.wasm과 hello.mjs가 만들어진다
$ node hello.mjs                                       # Node.js로 실행한다
args: #()
fib(25) = 75025
```

다른 JavaScript에서는 `hello.mjs`의 `run`을 호출합니다. `write`에는 출력이 문자열로 전달됩니다(`fd`는 표준 출력이 1, 표준 오류가 2). `run`의 값은 종료 코드입니다.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* 예를 들어 화면에 표시 */ } });
```

- `write`를 넘기지 않으면 Node.js에서는 표준 출력과 표준 오류로, 브라우저에서는 콘솔로 한 줄씩 출력합니다.
- 브라우저에서는 `hello.wasm`을 `hello.mjs`와 같은 위치에서 HTTP로 제공하세요(`file://`에서는 불러올 수 없습니다).
- `sleep`은 `Atomics.wait`로 기다립니다. 브라우저에서는 이것을 교차 출처 격리된 페이지의 Worker 안에서만 쓸 수 있으므로, `sleep`을 쓰는 프로그램은 Worker에서 실행하세요.
- 지역 시간(`zone`을 생략한 `decode-universal-time` 등)은 JavaScript의 `Date`가 알려 주는 시간대로 구합니다.
- 3.6절에서 쓸 수 없는 것에 더해, 파일과 디렉터리 조작, 환경 변수, `(command-line-args)`, 홈 디렉터리, `dribble`이 **컴파일 오류**가 됩니다. 표준 입력은 빈 것으로 읽힙니다.

### 3.8 마이크로컨트롤러(Cortex-M)용으로 컴파일하기

`--target thumbv7em-none-eabihf`를 붙이면 OS가 없는 Cortex-M4F・M7 보드용 실행 파일(ELF)을 만듭니다. `-o`를 생략하면 소스의 `.typl`을 `.elf`로 바꾼 이름이 됩니다. `--target thumbv8m.main-none-eabihf`이면 Cortex-M33 보드용입니다.

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # hello.elf가 생긴다
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # QEMU의 Cortex-M4에서 실행한다
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # QEMU의 Cortex-M33에서 실행한다
```

- `install.sh`로 설치한 `typl`은 그대로 쓸 수 있습니다. `cargo install`로 설치할 때는 `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf`를 실행한 뒤 `--features embedded`를 붙여 설치합니다(wasm과 둘 다라면 `--features wasm,embedded`). 저장소의 디버그 빌드 `typl`에서는 `scripts/build-embedded-runtime.sh`를 실행합니다. 링크에는 rustup 툴체인의 링커를 씁니다.
- 보드의 메모리 배치는 `--linker-script FILE`로 넘깁니다. 쓰는 법은 `cortex-m-rt`의 `memory.x`와 같고, FLASH와 RAM의 주소와 크기를 씁니다. `_typelisp_stack_size = 32K;`처럼 쓰면 RAM 중 스택이 쓰는 크기가 됩니다(쓰지 않으면 64KiB). RAM의 나머지가 힙입니다. 생략하면 QEMU의 `mps2-an386`(Cortex-M33에서는 `mps2-an505`) 배치가 됩니다.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

- 출력・입력・종료・시각(보드의 함수)을 누가 제공할지는 `--board-functions`로 고릅니다.
  - `semihosting`(기본값): semihosting으로 제공합니다. QEMU(`-semihosting-config enable=on`)나 디버거로 실행할 때 씁니다.
  - `c`: 보드가 C 함수로 제공합니다. 아래 함수를 정의한 오브젝트나 정적 라이브러리를 `--link FILE`로 넘깁니다(`--link`는 여러 번 쓸 수 있습니다. `defffi`로 부르는 벤더의 라이브러리도 같은 방식으로 넘깁니다).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1은 표준 출력, 2는 표준 오류
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0, 읽은 바이트 수, 끝이면 0
  void     typelisp_host_exit(int32_t code);                              // 돌아오지 않는다
  uint64_t typelisp_host_now_us(void);                                    // 거꾸로 가지 않는 시계의 마이크로초
  int64_t  typelisp_host_unix_us(void);                                   // 1970-01-01부터의 마이크로초
  void     typelisp_host_sleep_us(uint64_t us);                           // us 마이크로초 기다린다
  ```

- 보드의 레지스터(메모리에 매핑된 주변 장치)는 `volatile-read-u8`, `volatile-read-u16`, `volatile-read-u32`로 읽고 `volatile-write-u8`, `volatile-write-u16`, `volatile-write-u32`로 씁니다. 주소는 `int`로 넘기며, 읽은 값은 그 폭의 타입(`u8` 등)이 됩니다. 식을 쓴 자리에서 반드시 한 번 읽거나 쓰며, 컴파일러는 이를 생략하거나 옮기지 않습니다. 모두 `unsafe` 안에서만 쓸 수 있습니다. 주소가 음수이거나 폭의 배수가 아니면, 읽거나 쓰기 전에 실행 시간 오류가 됩니다.

  ```lisp
  ;; QEMU mps2-an386의 UART 0: 데이터 0x40004000, 상태 0x40004004, 제어 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; 송신 중이면 기다린다
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; 송신을 켠다
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi"와 줄바꿈
  ```

- 인터럽트가 올 때 호출되는 함수는 `defun`에 `#[interrupt(N)]`(주변 장치 인터럽트 `N`)이나 `#[exception(SysTick)]` 같은 속성을 붙여 씁니다. 인터럽트는 `(enable-interrupt N)`으로 켭니다. 핸들러는 그 자리에서 실행되며, 런타임의 처리 도중에 왔을 때만 그 처리가 끝난 뒤 실행됩니다. 핸들러 안에서는 기다릴 수 없습니다(`sleep`은 컴파일 오류입니다). 자세한 내용은 [구문 레퍼런스](../reference/syntax.md)의 3.17절에 있습니다.

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; 인터럽트 5를 스스로 일으킨다(NVIC의 보류 레지스터)
  ```

- 기동(벡터 테이블과 RAM 준비)과 메모리 확보는 런타임이 합니다. HardFault나 처리되지 않는 인터럽트가 일어나면 panic과 같은 종료 코드(1)로 끝납니다.
- `throw`/`catch`/`panic`, bignum・ratio・부동소수, 태스크・채널・`sleep`은 실행 파일과 똑같이 동작합니다. 태스크는 하나의 코어에서 차례로 실행됩니다. C 함수(`defffi`)도 쓸 수 있습니다.
- 보드에 없는 것을 쓰는 프로그램은 **컴파일 오류**가 됩니다. 3.7절에서 쓸 수 없는 것에 더해 `eval`・`macroexpand`를 쓸 수 없습니다. 지방시는 알 수 없습니다(3.6절과 같음).
- cons 아레나는 1024셀에서 시작해 모자라면 늘어납니다. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>`, `--release`는 그대로 쓸 수 있습니다.

### 3.9 Arduino Uno(AVR)용으로 컴파일하기

`--target avr-atmega328p`를 붙이면 Arduino Uno의 마이크로컨트롤러(ATmega328P)용 실행 파일(ELF)을 만듭니다. 다음 프로그램은 보드의 LED를 깜박입니다.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)                         ; PB5

(defun wait-a-bit () ()
  (let ((i (the u32 0)))
    (while (< i (the u32 400000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))           ; 출력으로 설정
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; 반전
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # blink.elf가 만들어짐
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # QEMU에서 실행
```

- 런타임이 없으므로 런타임을 사용하는 프로그램은 **컴파일 오류**가 됩니다. 오류에는 무엇이 필요하고 어떤 함수를 거쳐 도달하는지가 나옵니다. 사용할 수 있는 것은 고정 폭 정수(`i8`부터 `u32`까지), `int`, `f32`, `f64`, `bool`, `char`의 계산, 함수 호출(재귀 포함), 조건 분기, 반복문, 그 값들을 담는 `defvar`와 `defconstant`, `volatile-read-u8` 등, C 함수 호출입니다.
- 사용할 수 없는 것: 문자열·리스트·구조체처럼 힙에 놓이는 값. `catch`, 함수 값 호출. 문자열이나 함수를 넘기는 C 함수 호출. `sin`, `expt` 같은 부동소수점 수학 함수(`sqrt`와 `floor` 같은 반올림은 사용할 수 있습니다).
- 인터럽트 핸들러는 `#[interrupt(N)]`로 씁니다([구문](../reference/syntax.md) 3.17절). `N`은 ATmega328P의 인터럽트 번호로, 리셋을 0으로 센 1~25입니다(타이머 1 오버플로는 13). 아래 예를 보십시오.
- 0으로 나누면 그 자리에서 마이크로컨트롤러가 멈춥니다(인터럽트도 더 오지 않습니다). 부호 있는 최솟값을 `-1`로 나누는 `/`도 마찬가지입니다.
- `int`는 -16384부터 16383까지입니다. 계산 결과나 `int`로 변환한 값이 이 범위를 벗어나면 마이크로컨트롤러가 멈춥니다. 부동소수점 수를 정수로 변환할 때는 `int`를 거치므로 변환할 수 있는 것도 이 범위입니다.
- 재귀로 스택이 모자라면 마이크로컨트롤러가 멈춥니다.
- C 함수는 `defffi`로 선언하고, AVR용으로 컴파일한 오브젝트 파일을 `--link FILE`로 넘깁니다.
- 아무것도 하지 않는 반복문은 `-O1` 이상에서 제거됩니다. 기다리는 반복문 안에서는 레지스터를 읽는 등의 작업을 하십시오.
- `main`이 끝나면 마이크로컨트롤러가 멈춥니다.
- 메모리 배치는 `--linker-script FILE`로 바꿀 수 있습니다. 생략하면 플래시 32KiB에서 끝부분의 부트로더 512바이트를 뺀 영역과 RAM 2KiB입니다. RAM 주소는 `0x800100`처럼 `0x800000`을 더해서 씁니다.
- `f32`와 `f64`의 계산은 보조 함수가 합니다. 사용한 것만 플래시에 들어갑니다(`f64` 쪽이 더 큽니다).
- 링크에는 3.8절과 같이 rustup 툴체인의 링커를 사용합니다.

타이머 인터럽트로 LED를 깜빡이는 예입니다. 인터럽트는 주변 장치의 레지스터에서 활성화합니다.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; 타이머 1 오버플로
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; 반전
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; 오버플로 인터럽트를 활성화
          (volatile-write-u8 TCCR1B 3))           ; 클록의 1/64로 세기 시작
  (while true))
```

- 핸들러가 있는 프로그램은 전역 변수의 초기화가 끝나면 인터럽트를 받습니다. `main`의 처음에 주변 장치를 설정하십시오.
- `enable-interrupt`와 `#[exception(NAME)]`은 쓸 수 없습니다(컴파일 에러).
- 핸들러가 실행되는 동안 다른 인터럽트는 기다립니다. 핸들러도 다른 함수처럼 런타임을 쓸 수 없습니다.
- 전역 변수를 한 번 읽거나 쓰는 동작은 2바이트 이상의 값이라도 도중에 핸들러가 끼어들지 않습니다. 이를 위한 인터럽트 금지는 핸들러가 쓰는 변수를 읽을 때와 핸들러가 읽거나 쓰는 변수를 쓸 때 하며, 그 변수를 컴파일할 때 경고로 알립니다. 읽고 나서 쓰는 갱신(`(setf n (+ n 1))`) 사이에는 인터럽트가 올 수 있습니다. 핸들러도 쓰는 변수를 갱신할 때는 상태 레지스터(`#x5F` 번지)를 읽어 보관하고, 0을 써서 인터럽트를 멈춘 다음, 끝나면 보관한 값을 다시 쓰십시오.
- 핸들러가 없는 인터럽트가 오면 마이크로컨트롤러는 멈춥니다.

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
