<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# REPL

인수 없이 `typl`을 실행하면 REPL(대화 모드)이 된다. 이 문서에서는 식의 평가 외에 REPL에서 할
수 있는 것, 즉 REPL의 명령, 입력 지원, 직전 결과의 참조, 계산의 중단, 오류 후의 조사를 설명한다.

## 시작과 종료

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

`:quit`(줄여서 `:q`)이나 `Ctrl+D`로 종료한다. 입력 기록은 `~/.typl_history`에 남으므로, 다음에
시작할 때도 `Ctrl+P` / `Ctrl+N`이나 `Ctrl+R`(기록 검색)로 불러올 수 있다.

시작하면 첫 프롬프트 전에 다음을 차례로 실행한다. 어느 것도 실행한 값을 출력하지 않는다.

1. `~/.typlrc`가 있으면 그 내용. `--no-init`을 붙이면 읽지 않는다.
2. 명령줄의 `--eval FORM`과 `--load FILE`. 쓴 순서대로 실행한다. 둘 다 몇 번이든 쓸 수 있다.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load`, `--no-init`은 REPL을 시작할 때만 쓰는 것이다. 파일을 실행할 때 붙이면 오류가
된다.

시작할 때의 모듈은 `user`이다. `(in-module foo)`로 다른 모듈로 옮기면 프롬프트에 그 모듈이 표시된다.

```
typl> (in-module foo)
typl user::foo>
```

## REPL 명령

줄의 맨 앞에 쓴 `:이름`은 REPL 명령이다. `:help`로 목록을 표시한다. 목록에 없는 `:이름`은 명령이
아니라 보통의 키워드로 평가한다.

| 명령 | 의미 |
|---|---|
| `:help`(`:h`, `:?`) | 명령 목록을 표시한다 |
| `:type FORM`(`:t`) | FORM을 평가하지 않고 그 타입을 표시한다 |
| `:describe NAME`(`:d`) | 정의의 종류, 타입, 필드나 변형, 메서드, 구현한 트레이트, 정의한 위치, 문서 문자열을 표시한다 |
| `:apropos TEXT` | 이름에 TEXT를 포함하는 정의를 모든 모듈에서 찾는다(대소문자는 구별하지 않는다) |
| `:src NAME` | 정의의 소스를 표시한다 |
| `:who-calls NAME` | 함수나 메서드 NAME을 호출하는 함수와 메서드를 표시한다 |
| `:inspect FORM`(`:i`) | FORM을 평가하고 값을 타입에 따라 연다 |
| `:backtrace`(`:bt`) | 직전 줄이 실패했을 때 진행 중이던 호출을 안쪽부터 차례로 표시한다 |
| `:locals [N]` | `:backtrace`의 N번째(생략하면 0번째) 호출의 인수를 표시한다 |
| `:cd [DIR]` | 현재 폴더를 DIR로 옮긴다(생략하면 홈 폴더) |
| `:pwd` | 현재 폴더를 표시한다 |
| `:quit`(`:exit`, `:q`) | REPL을 종료한다 |

NAME에는 `inner` 같은 이름 외에 `std::seq::reverse`나 `Option::unwrap` 같은 경로도 쓸 수 있다.

```
typl> (defun inner ((x int) (s string)) int "x에 1을 더한다." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

x에 1을 더한다.
typl> :src inner
(defun inner ((x int) (s string)) int "x에 1을 더한다." (+ x 1))
```

`:src`는 파일에서 읽어 들인 정의라면 그 파일에서, REPL에서 입력한 정의라면 입력한 문자열에서
표시한다. 표준 라이브러리의 정의는 소스의 위치가 기록되어 있지 않으므로 표시할 수 없다.

`:who-calls`가 찾는 것은 지금 세션에 있는 함수의 본체이다. 제네릭 함수는 타입을 정해서 사용된
것만이 대상이다.

`:cd`는 `(load ...)`가 찾는 폴더를 바꾸지만, `use`가 파일을 찾는 폴더는 시작했을 때 그대로이다.

## 값 열기(`:inspect`)

`:inspect`는 값을 타입에 따라 연다. 구조체는 필드로, 열거형(`Option` 포함)은 그 변형의 필드로,
`Vector`는 요소로, `HashTable`은 키와 값으로 연다. 번호를 입력하면 그 부분으로 들어가고, `u`로
한 단계 위로 돌아가며, `q`로 끝낸다.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

리스트와 `Sexpr`은 열지 않고 그대로 표시한다.

## 입력 지원

단말에서 입력할 때는 다음을 쓸 수 있다.

- `Tab`으로 보완한다. 보완하는 것은 이름(정의와 `if`나 `defun` 같은 구문), `::`를 포함한 경로,
  줄 맨 앞의 REPL 명령, 문자열 안에서는 파일 이름이다.
- 함수 이름 뒤에 공백을 입력하면 그 함수의 타입을 줄 끝에 흐리게 표시한다.
- 커서 옆의 괄호와 짝이 되는 괄호를 강조한다.
- 폼이 끝나지 않은 채 줄을 바꾸면, 다음 줄은 열려 있는 괄호의 수만큼 들여 써서 시작한다.

## 직전 결과(`:*`, `:**`, `:***`)

REPL에서는 `:*`가 직전에 표시한 값, `:**`가 그 하나 앞, `:***`가 다시 하나 앞의 값이 된다. 각각
그 값의 타입을 가진다.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- 값을 표시하지 않은 입력(정의 등)과 오류가 난 입력은 결과를 진행시키지 않는다.
- 아직 그 수만큼 결과가 없을 때 `:**`나 `:***`를 쓰면 오류이다.
- 쿼트한 `':*`는 보통의 키워드이다.
- 함수 본체에 쓴 `:*`는 정의했을 때의 결과를 계속 가리킨다. Common Lisp의 `*`는 함수를 호출했을
  때의 값을 읽지만, typelisp에서는 타입을 정하기 위해 정의했을 때 정한다.
- REPL 밖(파일의 실행이나 `~/.typlrc`·`--load`·`--eval`)에서는 `:*`가 보통의 키워드이다.

## 계산 멈추기(`Ctrl+C`)

평가 도중에 `Ctrl+C`를 누르면 그 계산을 멈추고 프롬프트로 돌아간다. 그때까지 정의한 것은 남는다.
멈추는 것은 인터프리터에서 돌고 있는 루프, 컴파일한 코드의 루프, `sleep`이나 `wait`로 기다리는
도중 어느 것이든 상관없다. 진행 중인 `unwind-protect`의 뒷정리는 실행한다.

```
typl> (loop)
^Cerror: interrupted
typl>
```

입력 도중에 누른 `Ctrl+C`는 입력 중인 줄을 버리기만 한다.

## 오류 후 조사(`:backtrace`, `:locals`)

오류로 멈춘 뒤 `:backtrace`(`:bt`)로, 그때 진행 중이던 함수 호출을 안쪽부터 차례로 표시한다.
`:locals N`은 N번째 호출의 인수를 선언한 타입에 따라 표시한다.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- 함수의 마지막 식에서의 호출(꼬리 호출)에서는 호출한 쪽의 함수를 표시하지 않는다. 덕분에 꼬리
  재귀 루프는 몇 번을 돌아도 호출 하나만큼의 자리만 쓴다.
- 컴파일한 함수 안에서의 호출과 이름 없는 함수(`lambda`)는 표시하지 않는다.
- `:locals`가 표시하는 것은 인수뿐이며, `let`으로 묶은 변수는 표시하지 않는다.
- 다음 줄을 평가하면 이전 오류의 기록은 사라진다.
