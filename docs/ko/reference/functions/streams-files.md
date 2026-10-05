<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 스트림과 파일

스트림의 트레이트와 메서드, 구체 스트림 타입, 파일 조작, 경로명. 네트워크 소켓도 스트림이며 [네트워크](network.md)에서
다룬다.

## 1. 트레이트 계층

CL이 클래스 계층으로 나타내는 것을 여기서는 **트레이트 계층**으로 나타낸다. 방향(입력 / 출력)도 요소 타입도 **정적으로**
정해지므로 실행 중에 "이 스트림은 읽을 수 있는가"를 물을 필요가 없다.

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; 문자 입력
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; 문자 출력
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; 한 글자 되돌릴 수 있는 입력
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; 바이트 입력
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; 바이트 출력
```

문자를 읽는 함수는 `(where (CharInput S))`나 `:dyn CharInput`으로 받으면 내장이든 사용자 정의든 어떤 스트림 타입이든 받는다.

## 2. 메서드

`CharInput`의 메서드는 모두 기본 구현을 가진다. 구현하는 쪽은 `read-item`만 쓴다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | 다음 요소. 끝이면 `none`. **구현해야 하는 유일한 메서드** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | 다음 문자 |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | 다음 줄바꿈까지(줄바꿈은 소비되고 제거된다). 줄바꿈으로 끝나지 않는 마지막 줄도 반환된다 |
| `read-all` | `(read-all s)` | `(S)→string` | 남은 전부 |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | 이미 손에 있는 문자만. 기다리지 않고 `none` |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | 최대 `n`개의 문자를 `v`에 push하고 실제로 읽은 개수를 반환한다. `n`보다 적은 것은 끝에서뿐이다 |

`listen`은 `InputStream`(`CharInput`의 부모)에 있다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | 다음 읽기가 기다리지 않고 답해질 수 있는지. 기본값은 **결코 거짓말이 되지 않는 쪽**인 `false`이다. `true`는 추측이 되며, 추측이 틀리면 `read-char-no-hang`이 블로킹된다. 내장 스트림은 모두 이것을 덮어쓴다. **덮어쓰지 않은 사용자 정의 스트림에서 `read-char-no-hang`은 항상 `none`을 반환한다** |

`PeekInput`(`CharInput`을 상속한다)은 **한 글자 되돌리기**를 더한다. 되돌린 문자를 둘 곳은 스트림 자신만 가지므로 이것은
기본 구현을 가질 수 없어 별도의 트레이트이다. `file-stream`/`string-input-stream`/`standard-stream`이 구현하고, 그 밖의
스트림은 `make-peek-stream`으로 감싸면 얻는다(4장).

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | 다음 읽기가 `c`를 반환하게 한다. **구현해야 하는 유일한 메서드**. CL처럼 보장되는 것은 한 글자뿐이다 |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | 다음 문자를 소비하지 않고 본다 |

마찬가지로 `CharOutput`도 구현하는 쪽은 `write-item`만 쓴다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | 요소를 하나 쓴다. **구현해야 하는 유일한 메서드** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | 한 글자 쓴다 |
| `write-string` | `(write-string s str)` | `(S,string)→()` | 문자열을 쓴다 |
| `write-line` | `(write-line s str)` | `(S,string)→()` | 문자열과 줄바꿈 |
| `terpri` | `(terpri s)` | `(S)→()` | 줄바꿈 하나(CL의 이름) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | 줄 맨 앞이 아니면 줄바꿈 하나 |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | 다음에 쓰는 문자가 줄을 시작하는지. 기본값은 `false`(그래서 `fresh-line`은 줄바꿈을 쓴다. 의심스러우면 쓰는 쪽이 안전하다). 내장 스트림은 모두 이것을 덮어쓴다 |
| `finish-output` | `(finish-output s)` | `(S)→()` | 버퍼를 내보낸다 |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | `v`의 문자를 모두 차례로 쓴다 |

`at-line-start`가 기억하는 것은 **그 스트림을 통해 쓴 것뿐이다**. `print`/`println`/`(format true ...)`는
`*standard-output*`를 거치지 않고 표준 출력에 쓰므로, 둘을 섞으면 `(fresh-line *standard-output*)`는 `println`이 쓴 줄바꿈을
모른다. 어느 한쪽으로 통일한다.

`Stream`은 모든 스트림에 공통이다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | 아직 열려 있는지 |
| `close` | `(close s)` | `(S)→()` | 닫는다. **GC는 스트림을 닫지 않으므로** 명시적으로(또는 `with-open-file`로) 닫는다 |

## 3. 구체적인 스트림 타입

| 타입 | 만드는 법 | 구현하는 트레이트 |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction`은 세 상수 `direction-input` / `direction-output` / `direction-append` 중 하나이다. `open-file`은 파일을 열 수
없으면 `Err(FileError)`를 반환한다(파일이 없는 것은 panic이 아니라 보통의 결과). 파일 이름은 문자열이든 `pathname`이든
된다(9장의 `Pathish`).

`(get-output-stream-string s)`는 `string-output-stream`에 쓴 것을 반환하고 비운다. CL처럼 `close` 뒤에도 꺼낼 수 있다.

**바이트 입출력**은 `ByteInput`/`ByteOutput`을 쓴다. `CharInput`/`CharOutput`이 `InputStream`/`OutputStream`의 `Item`을
`char`로 고정하는 것과 같은 방식으로 `int`로 고정한다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | 다음 바이트. 파일 끝이면 `none` |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | 한 바이트 쓴다. 0..255 밖이면 오류 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | 문자판을 바이트로 |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | 위와 같음 |

CL은 `(open name :element-type '(unsigned-byte 8))`처럼 **호출**에서 요소 타입을 정하지만, 여기서는 요소 타입이 스트림의
**타입**이므로 여는 함수가 다르다. 문자 스트림에서 바이트를 읽으면 타입 오류이다(`string-input-stream`은 `ByteInput`을
구현하지 않는다). `unread-char`로 문자를 되돌린 직후에 바이트를 읽는 것도 오류이다.

## 4. 합성 스트림

모두 표준 라이브러리의 `defstruct`이며 중첩할 수 있다.

| 이름 | 형식 | 설명 |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | `Vector<:dyn CharOutput>`의 모두에 쓴다 |
| `make-two-way-stream` | `(make-two-way-stream in out)` | `in`에서 읽고 `out`에 쓴다 |
| `make-echo-stream` | `(make-echo-stream in out)` | `in`에서 읽고, 읽은 문자를 `out`에도 쓴다 |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | `Vector<:dyn CharInput>`를 차례로 읽는다 |
| `make-peek-stream` | `(make-peek-stream in)` | 아무 `:dyn CharInput`에 한 글자 되돌리기를 더해 `PeekInput`으로 만든다(`read-sexpr`용) |

## 5. 매크로

| 이름 | 형식 | 설명 |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | 열고, 본체를 실행하고, 닫는다. `Result<본체의 값, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | 문자열에서 읽는다 |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | 쓴 것을 반환한다 |

## 6. 제네릭 함수와 파일 조작

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | 전부 옮긴다 |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | 남은 줄 전부 |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | `Sexpr`를 하나 읽는다(CL의 `read`). 입력 끝이면 `Ok(eof)`, 읽으면 `Ok(datum d)`, 데이터가 아니면 `Err`. 데이터를 끝낸 **공백 한 글자를 소비한다**(CL과 같다). `ReadOutcome`가 `Option<Sexpr>`가 아닌 것은 빈 리스트 `()`를 읽은 것과 입력 끝이 같은 값이 되지 않게 하기 위해서이다 |
| `read-sexpr-preserving-whitespace` | 위와 같음 | 위와 같음 | 같지만 공백을 남긴다(CL의 `read-preserving-whitespace`) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | `ch`까지 읽어 리스트로 만든다. `ch`는 소비된다. 입력이 다하면 `Err` |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | 한 줄씩 쓴다 |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | 내용 전체 |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 모든 줄 |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | 써 낸다 |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | 있는지 |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | 삭제, 이름 바꾸기(인수는 `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | 심볼릭 링크와 `.`/`..`를 해석한 절대 경로. 없으면 `Err` |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | 마지막으로 수정한 시각. **유니버설 타임**이므로 `decode-universal-time`([시간](system.md#2-날짜의-분해와-조립))으로 읽을 수 있다 |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | 소유자의 로그인 이름. 파일이 없으면 `Err`, 소유자의 uid가 암호 데이터베이스에 없으면 `Ok(none)`. CL이 구별하는 두 경우를 나누어 둔다 |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | 디렉터리인지. **없을 때도 `false`**이다. 둘을 구별하려면 `probe-file`을 쓴다 |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | 내용을 truename(`truename`처럼 심볼릭 링크를 해석한 절대 경로)으로 나열한다. 대상이 없는 심볼릭 링크는 빠진다. `.`/`..`는 빠진다. 순서는 OS가 주는 대로 |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | 부모까지 함께 만든다. 이미 있으면 성공 |

파일을 가리키는 인수는 모두 **문자열이든 `pathname`이든 된다**. CL의 경로명 지정자와 같은 다룸이며, 실행 시의 타입
검사가 아니라 `Pathish` 트레이트로 해석된다(9장).

`read-delimited-list`의 끝 문자는 **토큰도 끝낸다**. 효과가 있는 것은 깊이 0에서뿐이다. `(1 2]`에서 `]`는 리스트 자신의
텍스트의 일부로 읽혀 깨진 리스트로 보고된다. CL의 세 번째 인수 `recursive-p`에 해당하는 것은 없다.

## 7. 자신의 타입을 스트림으로 만들기

`write-item` 하나를 쓰면 기본 구현이 나머지를 데려온다. 합성 스트림에도 넣을 수 있다.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; 남은 메서드는 모두 기본 구현

(write-line (counter::new 0) "four")   ; write-line, terpri, fresh-line이 모두 동작한다
```

입력도 마찬가지로 `read-item`만 쓴다. 자체 되돌리기가 없는 타입도 `(read-sexpr (make-peek-stream my-stream))`처럼 감싸면
`read`할 수 있다.

## 8. readtable

| 이름 | 호출 | 타입 | 설명 |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f`가 문자 `c`를 읽는다 |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | 등록된 것을 반환한다 |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f`가 두 글자 열 `d s`를 읽는다 |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | 위와 같음 |

`F`는 `(fn (string-input-stream char) Option<Sexpr>)`이다. 쓰는 법, 효과가 생기는 시점, CL과의 차이는
[문법 레퍼런스](../syntax.md#11-리더-매크로readtable)에 있다.

## 9. 경로명 `pathname`

부분으로 나눈 파일 이름. `/`로 구분된 디렉터리 구성 요소, 이름, 타입(확장자), 루트에서 시작하는지를 가진다.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   마지막 점에서 나눈다
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 경로명 지정자 트레이트 `Pathish`

CL이 경로명 지정자(문자열 또는 경로명)를 받는 곳에서 이 언어는 `Pathish`를 받는다. `string`과 `pathname`이 둘 다
구현하며, **모든 파일 조작이 그것을 제네릭하게 받으므로** `(open-input "a.txt")`도 `(open-input p)`도 보통의 호출이다(실행
시의 타입 검사는 없다). 문자열의 `namestring`은 자기 자신을 반환할 뿐이므로 문자열을 넘기는 한 해석은 일어나지 않는다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | 문자열 형태. 구현해야 한다 |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | `pathname`으로 바꾼다(CL의 `pathname` 함수. 타입 이름과 겹치므로 이름을 바꿨다). 구현해야 한다 |

### 9.2 함수

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | 문자열을 부분으로 나눈다. 끝의 `/`(또는 빈 이름)는 "이름 없음", 즉 디렉터리를 뜻한다 |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | 준 구성 요소만으로 만든다(모두 `&key`). 생략한 이름이나 타입은 "없음"으로 남으며 `merge-pathnames`가 채우는 대상이다 |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | 디렉터리 구성 요소. 바깥쪽이 먼저 |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | 타입을 뺀 이름. 디렉터리면 `none` |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | 마지막 점 뒤. 맨 앞의 점은 세지 않는다(`.gitignore`는 전부 이름) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | 루트에서 시작하는지 |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | 홈 디렉터리. `$HOME`이 없으면 `none`(CL도 `NIL`을 허용한다) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | 마지막 `/`까지의 부분 |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | `name.type` 부분만 |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | `p`에 빠진 구성 요소를 `default`에서 채운다. 상대 경로의 `p`는 `default`의 디렉터리 아래로 가고, 절대 경로의 `p`는 자신의 디렉터리를 유지한다 |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | `default`에 대한 상대 형태. 기준 아래에 없으면 `p` 전부 |

타입 인수는 모두 `(where (Pathish P))`를 가진다.

## 10. CL과의 차이

- **클래스 계층이 아니라 트레이트 계층.** `input-stream-p` / `output-stream-p`는 없다. 방향은 타입이 나르므로 실행 중에 물을
  질문이 아니다.
- **`read`는 문자열판과 스트림판의 이름이 다르다.** `(read "...")`(CL의 `read-from-string`의 첫 번째 값에 해당. 읽기가 끝난
  위치도 필요하면 `read-from-string`)와 `(read-sexpr s)`(CL의 `read`). 호출은 하나의 받는 쪽 타입으로 해석되므로 같은 이름을
  오버로드할 수 없다.
- **되돌리기는 별도의 트레이트**(`PeekInput`)이므로 `read-char`만 필요한 타입이 `unread-char`의 구현을 강요받지 않는다.
- **닫기는 명시적이다.** GC는 스트림을 닫지 않는다(GC는 예측할 수 없는 때에 돌므로 GC에 맡기면 닫히는 순간도 예측할 수
  없게 된다). `with-open-file`을 쓰는 것이 안전한 방법이다.
- **경로명에는 호스트, 디바이스, 버전 구성 요소가 없다.** 와일드카드 경로명도 논리 경로명(`logical-pathname`)도 없다.
  구분자는 언제나 `/`이다.
- **`pathname` 함수는 `to-pathname`이다.** 타입, 트레이트, 함수가 하나의 이름공간을 공유하기 때문이다.
- **와일드카드에 의한 매칭이 없으므로** `directory`는 "그 디렉터리의 내용을 나열하는" 함수일 뿐이다. CL의 `directory`는
  경로명 패턴과 매칭한다.
