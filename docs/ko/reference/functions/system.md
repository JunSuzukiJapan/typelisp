<!-- translated-from: docs/ja/reference/functions/system.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 시간, 환경, 구현

시간 함수, 실행 환경에 대한 질의, 구현 도구, 텍스트의 해석과 평가, 문서 문자열, 매크로.

## 1. 시간

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `universal-time` | — | `defstruct` | 필드 두 개: `day`(1900-01-01부터의 일수)와 `second`(그날 안의 초, 0..86399) |
| `internal-time` | — | `defstruct` | 필드 두 개: `second`와 `microsecond`(그 초 안, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | CL의 기원(1900-01-01 UTC)부터의 시간 |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | 프로세스 기준의 경과 시간 |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | 이 프로세스가 쓴 **CPU 시간**(사용자와 시스템의 합) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | 초 단위의 수로. 두 측정의 차이를 보고하는 형태 |
| `internal-time-units-per-second` | — | `int` | `1000000`(마이크로초). `microsecond` 필드의 단위. CL처럼 값은 구현이 정한다 |
| `time` | `(time form)` | 매크로 | `form`을 실행하고 실제 시간과 CPU 시간을 한 줄씩 출력한 뒤 `form`의 값을 그대로 반환한다 |

실제 시간과 CPU 시간은 서로 다른 것을 알려 준다. 주로 입출력을 기다리는 처리에서는 둘이 크게 다르며, 그 차이야말로 알고
싶은 것이므로 `time`은 둘 다 보인다.

태스크를 멈추는 `sleep`은 [태스크와 채널](concurrency.md#3-yield--sleep--양보)에 있다.

## 2. 날짜의 분해와 조립

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **필드 아홉 개**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. CL의 반환값 아홉 개를 하나의 구조체로(다중 값이 없다) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | 유니버설 타임을 달력 구성 요소로. `zone`은 그리니치에서 서쪽으로의 시간 수(CL과 같은 방향). **생략하면 지역 시간**(CL과 같다) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | 반대. `zone`이 없으면 인수를 **지역 시간**으로 읽는다 |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | 지금을 지역 시간으로 분해한 것 |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | 그 유니버설 타임에서 지역 시간의 그리니치 서쪽 오프셋(**초** 단위) |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | 그 유니버설 타임에 서머타임이 적용되고 있었는지 |

CL처럼 `day-of-week`는 **0이 월요일, 6이 일요일**이다.

**`zone`이 없으면 지역 시간을 쓴다**(CL과 같다). 지역 오프셋은 OS에 묻으므로 결과는 기계가 있는 곳에 따라 달라진다.
**zone을 명시하면 결정적이 되며**, `0`은 UTC이다.

`zone`의 단위는 CL처럼 "그리니치에서 서쪽으로의 시간 수"이므로 UTC+9는 `-9`가 된다. 다만 **인수는 정수이고 결과의 `zone`
필드는 `f64`이다**. 실제 오프셋은 늘 정수 시간이 아니며(인도는 +5:30, 네팔은 +5:45), 보고하는 값을 반올림하면 조용히
거짓말을 하게 된다. 손으로 쓰는 zone은 정수 시간이므로 인수는 `int`이다.

`zone`을 주면 CL이 정한 대로 `daylight-p`는 `false`이고 `zone`은 준 값 그대로이다(*If a time-zone is supplied, daylight
saving time information is ignored*).

서머타임 전환 안에 드는 지역 시간은 애초에 하나로 정해지지 않으며, CL도 어느 쪽을 택할지 말하지 않는다.
`encode-universal-time`은 그런 시간에 대해 두 답 중 하나를 반환한다.

## 3. 실행 환경

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | 명령줄. **0번째 요소는 프로그램 이름** |
| `getenv` | `(getenv name)` | `string→Option<string>` | 환경 변수. 설정되지 않았거나 UTF-8이 아니면 `none` |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. `user-homedir-pathname`([경로명](streams-files.md#92-함수))의 바탕 |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | 구현의 버전 |
| `machine-type` | `(machine-type)` | `()→string` | CPU 아키텍처(`x86_64` / `aarch64` …). **빌드 대상**의 값 |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | 호스트 이름 |
| `machine-version` | `(machine-version)` | `()→Option<string>` | **지금 실행 중인** 하드웨어의 이름(`Apple M1` / `Intel(R) Xeon(R) …`). 알 수 없는 곳에서는 `none` |
| `software-type` | `(software-type)` | `()→string` | OS(`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | OS의 릴리스(`uname -r`, 예를 들어 `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | 설치 장소의 짧은 이름. **항상 `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | 마찬가지로 긴 이름. **항상 `none`** |

`Option`을 반환하는 것은 CL이 `NIL`을 허용하는 항목이다(*or nil if no such name can be determined*). POSIX에는 사이트 이름을
기록할 곳이 없으므로 항상 `none`이며, SBCL도 같은 것을 반환한다. `machine-type`과 `machine-version`의 차이에 주의한다.
앞의 것은 이 바이너리가 **빌드된** 아키텍처이고, 뒤의 것은 지금 그것을 **실행하는** 칩이다.

`command-line-args`의 0번째 요소는 `typl script.typl a b`에서는 스크립트의 경로, `./prog a b`로 실행하는 AOT 실행 파일에서는
실행 파일 자신이다. **어느 실행 방법이든 같은 인수를 같은 인덱스로 읽는다**(`typl`은 자기 이름과 `--heap-cells` 같은
옵션을 빼고 넘긴다).

## 4. 사용자에게 묻기

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | `y` / `n` 한 글자를 받는다. 받을 때까지 다시 묻는다 |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | `yes` / `no`를 철자대로 입력하게 한다. 실수의 대가가 큰 질문용 |

둘 다 `*standard-input*`에서 읽는다. 다시 묻기를 멈추는 것은 입력의 끝뿐이며, 그때의 결과는 `false`이다.

## 5. 구현 도구(CLHS 25.2)

구현이 자기 자신에 대한 질문에 답하는 층. `heap-info` / `room` / `dribble`은 보통의 함수이고, `trace` / `untrace` / `step` /
`disassemble` / `ed`는 **특수 형식**이다(`trace` / `untrace` / `disassemble` / `ed`는 정의의 *이름*을, `step`은 *형식*을 모두
평가하지 않고 받는다).

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | 힙의 현재 상태를 구조체로. `room`이 출력하는 것과 같은 수 |
| `room` | `(room &optional verbose)` | `(bool)→()` | `heap-info`를 `*standard-output*`에 보고한다. `(room true)`는 더 자세히 |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | 세션의 출력을 `path`에 기록하기 시작한다 / 인수 없이 호출하면 기록을 멈춘다 |
| `trace` | `(trace name...)` | `Sexpr` | 이름 붙은 정의의 호출을 `*trace-output*`에 보고한다. 지금 추적 중인 이름의 리스트를 반환한다 |
| `untrace` | `(untrace name...)` | `Sexpr` | 보고를 멈춘다. **인수가 없으면 모두 해제** |
| `step` | `(step form)` | `form`의 타입 | `form`을 평가하며 호출마다 멈춰서 묻는다 |
| `disassemble` | `(disassemble name [llvm])` | `()` | 그 정의가 무엇이 되었는지 출력한다. 기본은 호스트의 기계어, `true`면 LLVM IR |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | `$VISUAL` / `$EDITOR`를 시작한다. 이름을 주면 그 정의가 쓰인 줄을 연다 |

`trace`/`untrace`/`step`/`disassemble`은 인터프리터 전용이며, 이것들을 호출하는 함수는 컴파일할 수 없다
([문법 레퍼런스 10장](../syntax.md#10-컴파일)).

### 5.1 `heap-info`의 필드

| 필드 | 타입 | 내용 |
|---|---|---|
| `capacity` / `live` / `free` | `int` | cons 영역 전체와 그 내역. 항상 `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | 힙의 다른 세 종류의 객체의 현재 개수 |
| `gc-count` | `int` | 구현이 시작된 뒤 수집한 횟수 |
| `growable` | `bool` | 영역이 아직 늘어날 수 있는지 |

필드는 `growable`을 빼고 모두 `int`이다. 증가의 상한(`typl --heap-cells`의 설명 참고)은 보고하지 않는다. 읽는 쪽이 알고
싶은 것은 아직 늘어날 수 있는지(`growable`)이기 때문이다.

### 5.2 `trace` / `step`이 볼 수 있는 것과 없는 것

- **컴파일된 본체를 가진 정의도, 인터프리트 중인 호출 지점에서는 보인다.**
- **컴파일된 코드 *안의* 호출 지점은 보이지 않는다.** 컴파일된 본체를 가진 이름을 추적하면 그렇다는 한 줄 주석이 붙는다.
  SBCL이 지역 호출에 대해 설명하는 것과 같은 제한이다.
- **클로저 값을 통한 호출(`funcall`/`apply`)은 보이지 않는다.** 클로저에는 이름이 없다.
- **제네릭 정의는 대상이 아니다.** 타입마다의 사본은 쓰이는 곳마다 만들어지므로 이름을 붙일 단일한 본체가 없다
  (`compile`이 거절할 때와 같은 이유, 같은 문구).

`step`의 명령은 `s`(이 호출 안으로 들어간다. 빈 줄도 같다), `n`(이 호출을 건너뛴다), `c`(이제부터 묻지 않는다), `q`(중단)이다.
**표준 입력이 터미널이 아니면 `step`은 그냥 `form`을 평가한다.** 아무도 답할 수 없는 프롬프트에서 스크립트와 테스트가
멈추지 않도록 CLHS가 명시적으로 허용하는 퇴화한 동작이다.

`ed`의 `$VISUAL` / `$EDITOR`는 공백에서 나뉘므로 `EDITOR="code -w"`도 동작한다. 둘 다 설정되지 않았으면 결과는 `Err`이며
`vi`를 추측하지 않는다. 줄 번호는 `+N` 형태로 먼저 넘긴다.

`dribble`은 세션의 출력이 프로세스를 떠나는 세 가지 길을 모두 기록한다. `print`/`println`/`format`이 쓰는 것, 표준 출력에
연결된 스트림에 쓰는 것, 그리고 REPL에 입력한 줄과 REPL이 돌려 출력하는 값이다.

## 6. 해석과 평가

모두 실행 시의 텍스트와 데이터(프로그램 자신이 통제하지 않는 것)를 다루므로 실패하면 panic하지 않고 `Result`의 `Err`을
반환한다. 오류 타입은 연산마다의 구체 타입이다([오류 타입](option-result.md#3-오류-타입과-error-트레이트)).

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | CL의 `parse-integer`. 앞뒤의 공백(`trim`과 같은 집합)을 건너뛰고, 부호 `+`/`-`를 최대 하나, 그다음 기수 `radix`(기본 10, 2~36. 10 이상의 숫자는 대소문자 어느 쪽이든)의 숫자를 읽는다. 자릿수에 제한은 없다(`int`). 다른 문자가 남으면 `Err`. `:junk-allowed true`면 첫 숫자가 아닌 문자에서 멈추고 나머지를 무시하지만 숫자가 하나도 없으면 `Err`(CL의 `nil`에 해당). CL의 두 번째 값(읽기가 끝난 위치)은 반환하지 않는다. 범위를 벗어난 `radix`는 panic(텍스트가 아니라 호출한 쪽의 실수) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | 부동소수점 수. `inf`/`nan`도 받는다 |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | `s`에서 `Sexpr`를 하나 읽는다(소스 코드를 읽는 것과 같은 리더로). 괄호의 짝이 맞지 않거나 끝나지 않은 문자열 등은 `Err`. 스트림에서 읽기는 `read-sexpr`([스트림](streams-files.md#6-제네릭-함수와-파일-조작)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read`에 **읽기가 끝난 위치**를 더한 것. `(car r)`가 값, `(cdr r)`가 다음에 읽을 문자의 위치. `start`의 기본값은 0 |
| `read-from-string-preserving-whitespace` | 위와 같음 | 위와 같음 | 같지만 데이터를 끝낸 공백을 소비하지 않는다. 차이는 반환하는 위치에 나타난다 |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | `form`을 실행 시에 타입 검사해 평가한다. CL의 `eval`을 따른다 |

CL은 `read-from-string`에서 **두 값**(값과 위치)을 반환하지만 이 언어에는 다중 값이 없으므로 `cons-cell` 하나를 반환한다.
위치가 있으면 문자열을 데이터 하나씩 읽는 것이 다시 훑기가 아니라 루프가 된다.

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

`preserving-whitespace`가 만드는 차이는 **공백 한 글자**이다. CL의 `read`는 데이터를 끝낸 공백을 소비하고
`read-preserving-whitespace`는 남긴다. `(read-from-string "12 34")`는 위치 3을, 남기는 판은 2를 반환한다.

리더가 받는 수의 구문은 [문법 레퍼런스 1장](../syntax.md#1-어휘-요소)에 있다. `*print-radix*`
([출력](printing.md#62-기수-대소문자-읽을-수-있는-형태))가 출력하는 것은 그대로 다시 읽을 수 있다. CL의 `*read-base*`는
없다.

### 6.1 `eval`의 의미

CLHS의 `eval`을 따른다. **현재의 전역 환경**(실행 시에 더한 정의를 포함한 전역 함수, 변수, 타입, 매크로)과 **빈 렉시컬
환경**(호출한 쪽의 `let`/`lambda`의 지역 바인딩은 보이지 않는다)에서 평가한다. 식도 정의(`defun`/`defvar`/`defstruct`/
`defenum`/`defmacro`)도 평가할 수 있으며, 정의는 즉시 그리고 영구히 전역 환경에 등록된다.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; 전역 x가 보인다
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; 정의한 이름을 반환한다
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; 방금 만든 정의가 보인다
```

- **반환값**: 식이면 결과를 `Option<Sexpr>`로, 정의면 정의한 이름의 심볼(CL과 같다). 결과를 쓰려면 `Sexpr`를 `match`로
  분해한다(`(int n)`/`(str s)`/…).
- **정적 타입에 의한 차이(중요)**: CL은 결과의 실제 값을 반환하지만 이 언어에서는 반환 타입이 한결같이
  `Result<Option<Sexpr>,EvalError>`일 수밖에 없다. 또 **정적으로 쓴 코드는 `eval`이 실행 시에 정의하는 이름을 앞질러
  참조할 수 없다**. 파일에 직접 쓴 `(sq 9)`는 `sq`를 정의하는 `eval`이 실행되기 전에 검사되어 "정의되지 않음"이 된다.
  다만 **나중의 `eval`에서는 보인다**(그 타입 검사는 정의 뒤인 실행 시에 돌기 때문이다). REPL은 한 줄씩 검사하고 실행하므로
  `eval`로 정의한 이름을 다음 줄에서 바로 호출할 수 있다.
- **오류**: 타입 오류와 구문 오류는 `Err`을 반환한다(panic하지 않는다). 평가한 코드 안의 **실행 시 panic**(0으로 나누기
  등)은 직접 쓴 코드에서처럼 전파된다. 사이에 있는 `unwind-protect`의 뒷정리는 실행된다
  ([문법 레퍼런스 8장](../syntax.md#8-비지역-탈출catch--throw--unwind-protect)).
- **이름공간**: `typl file.typl`로 실행할 때와 AOT 실행 파일 안에서 `eval`은 스크립트의 모듈의 이름공간에서 평가한다
  (스크립트 자신의 전역이 보인다). REPL은 루트 이름공간에서 평가한다.
- **컴파일**: `read`도 `eval`도 컴파일할 수 있다. AOT 실행 파일에서의 다룸과 그 결과(eval에 넘긴 형식은 인터프리트된다)는
  [문법 레퍼런스 10.2](../syntax.md#102-aot-실행-파일-안의-eval)에 있다.

## 7. 문서 문자열 / `documentation`

`defun`/`defmethod`(`impl` 안 포함)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/`deftype`/`deftrait`는 문서
문자열을 가질 수 있다. 위치는 각각 CL의 규칙을 따른다.

| 형식 | 문서 문자열의 위치 |
|---|---|
| `defun` / `defmethod` / `defmacro` | 본체의 맨 앞(반환 타입과 `where` 절 뒤). 뒤에 본체 형식이 하나 이상 있을 때만. 홀로 있는 문자열은 반환값으로 남는다 |
| `defvar` / `defconstant` | 초깃값 **뒤**: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | 이름 **바로 뒤**, 필드/변형 앞 |
| `deftype` | 이름 **바로 뒤**, 타입 앞: `(deftype meters "doc" i32)` |
| `deftrait` | 상위 트레이트 목록 바로 뒤, 항목 앞. 트레이트 전체에 하나. **기본 구현을 가진 메서드**는 본체 바로 앞에 자기 문서 문자열을 둘 수 있다 |

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `documentation` | `(documentation name)` | (특수 형식. `name`은 맨 심볼 또는 `Type::method`)→`Option<string>` | `name`의 문서 문자열을 반환한다 |

`quote`/`compile`처럼 `documentation`은 특수 형식이다(`name`을 평가하지 않은 이름으로 읽는다). CL의
`(documentation 'name 'function)`과 달리 타입 인수를 받지 않으며, 대신 맨 이름을 **변수 → 함수 → 타입 → 트레이트 →
매크로** 순서로 해석한다(맨 식별자를 식으로 평가할 때와 같은 우선순위). `Type::method` 형식은 연관 메서드나 정적 메서드의
문서 문자열을 찾는다.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**값은 검사 시에 정해진다.** 이름이 어떤 정의로도 해석되지 않으면 검사 시 오류이다(정의되지 않은 변수를 참조할 때처럼).
해석되지만 문서 문자열이 없으면 결과는 `Option::none`이다.

**대상이 아닌 것**:

- `(setf documentation)`(실행 시에 문서 문자열 바꾸기)은 없다.
- 모듈로 한정한 자유 이름(`mod::name`. `Type::method`는 지원한다)은 지원하지 않는다.
- `deftrait` 안의 **본체가 없는** 메서드 선언은 문서 문자열을 가질 수 없다. 끝의 문자열 리터럴이 그 자체로 기본 구현의 본체
  (반환값)가 되어 둘을 구별할 방법이 없다.

언어 서버(`typl-lsp`)의 호버도 문서 문자열을 보인다.

## 8. 매크로

매크로를 정의하는 방법은 [문법 레퍼런스 3.14](../syntax.md#314-defmacro--매크로-정의)에 있다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | 새 심볼. 이름은 `" <prefix><n>"`이며 `n`은 `*gensym-counter*`. 맨 앞의 공백은 소스에 쓸 수 없으므로 생성된 바인딩이 쓰인 이름과 겹치는 일은 없다 |
| `*gensym-counter*` | 변수 | `int` | `gensym`이 다음에 쓰는 수. CL처럼 읽고 설정할 수 있다 |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | 매크로 호출을 한 단계 전개한다. `none`은 "매크로 호출이 아니다"라는 뜻 |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | 매크로가 아니게 될 때까지 반복한다 |

`macroexpand-1`은 `Option`을 반환한다. CL은 "전개했는지"를 두 번째 반환값으로 알리지만 다중 값이 없으므로 `none`이 그 역할을
한다. **자기 자신의 호출로 전개되는 매크로가 매크로 아닌 것과 혼동되는 일은 결코 없다.** 한 단계의 전개는 타입 검사기가 쓰는
것과 같은 것이므로 프로그램이 보는 것과 검사기가 본 것이 어긋나지 않는다.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none은 빈 리스트로 보인다(Option<Sexpr>는 투명)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

CL에 있고 이 언어에 없는 것: `eval-when`(`:compile-toplevel`/`:load-toplevel`/`:execute`가 항상 일치하므로 고를 구별이
없다), `define-compiler-macro`, `load-time-value`, `make-symbol`/`copy-symbol`/`gentemp`(인턴되지 않은 심볼. 바인딩은
이름으로 찾으므로 얻을 것이 없다).

## 9. 지역 매크로 바인딩(`macrolet` / `symbol-macrolet`)

둘 다 **값이 아닌 이름**을 렉시컬하게 묶는 특수 형식이다. 실행 시에는 아무것도 남지 않으며, 컴파일되는 것은 본체를 전개한
형식이다.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- `macrolet`의 바인딩은 같은 이름의 전역 매크로를 **본체 동안만** 가린다. 람다 목록은 `defmacro`와 같다
  (`&optional`/`&rest`/`&key`).
- **같은 `macrolet`의 형제는 서로의 *본체*에서 보이지 않는다**(CL과 같다. `labels`와의 차이). 전개 결과는 쓰는 곳에서
  검사되므로 `earlier`가 `(later ...)`로 전개되는 것은 동작한다. 그곳에서는 둘 다 보이기 때문이다.
- `symbol-macrolet`의 이름은 보통의 바인딩으로 환경에 들어간다. 그래서 안쪽의 `let`이 같은 이름을 가리고, 바깥의 변수는
  가려진다. CL의 규칙이 그대로 나온다.
- **`setf`는 전개 결과에 쓴다.** `(setf head 42)`는 `(setf (get v 0) 42)`이다.
- 전개 결과는 **쓰는 곳의 환경**(묶은 곳이 아니라)에서 검사된다.

## 10. 기타

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | 거짓이면 panic. 메시지가 없으면 `assertion failed: <쓴 그대로의 검사식>`(매크로이므로 식 자체를 이름 댈 수 있다). CL의 재시작은 이 언어에 없다 |
| `warn` | `(warn control args...)` | `(string,...)→()` | `WARNING: `을 앞에 붙인 한 줄을 `*error-output*`에 쓰고 **계속한다**. `Result`를 반환하지도 프로그램을 끝내지도 않고 무언가를 알리는 방법 |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | `body` 동안만 전역 변수를 바꾸고 나갈 때 되돌린다. CL은 이것을 `let`으로 쓰지만 이 언어의 `let`은 항상 렉시컬하게 묶으므로 다른 이름이다(Emacs Lisp의 같은 이름의 매크로와 같은 역할). 정상 종료, `throw`, `panic`, `break`/`return` 중 어떻게 빠져나가든 되돌린다. **태스크마다의 바인딩이 아니다** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | 모든 프린터 제어 변수를 표준 값으로, `*read-eval*`을 `true`로 해서 `body`를 실행한다([출력](printing.md#6-출력량-제어)) |
| `exit` | `(exit code)` | `int→!` | 프로세스를 끝낸다 |
| `dump` | `(dump path)` | `string→bool` | 현재 환경(타입 정보와 컴파일된 본체)을 하나의 파일에 쓴다. `typl --image <path>`로 거기서 다시 시작한다. 인터프리터 전용([문법 레퍼런스 10.1](../syntax.md#101-덤프)) |
