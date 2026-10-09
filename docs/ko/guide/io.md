<!-- translated-from: docs/ja/guide/io.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# 파일 입출력, 스트림, 네트워크

이 가이드에서는 파일 읽기와 쓰기, 경로명, 소켓 통신의 기본을 보인다. 함수 목록은
[스트림과 파일](../reference/functions/streams-files.md)과 [네트워크](../reference/functions/network.md)에 있다.

## 1. 실패는 `Result`로 돌아온다

파일 열기나 연결처럼 환경에 따라 실패할 수 있는 연산은 `Result`를 반환한다. 파일이 없는 것은 프로그램의 실수가
아니므로 `panic`하지 않는다. `match`로 나눈다.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; 파일이 없으면:
;; error: config.txt: No such file or directory (os error 2)
```

실패하지 않는다는 것을 알고 있을 때나, 실패하면 멈춰도 괜찮은 작은 스크립트에서는 `unwrap`으로 값을 꺼낸다. `Err`이면
panic한다.

## 2. 파일 전체를 읽고 쓰기

가장 간단한 함수는 파일 전체를 한 번에 다룬다.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #("abc"))
```

## 3. 스트림으로 읽고 쓰기

조금씩 읽거나 쓰려면 `with-open-file`로 스트림을 연다. 본체를 어떻게 빠져나가든 스트림은 닫힌다. 값은
`Result<본체의 값, FileError>`이다.

```lisp
;; 쓰기
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; 한 줄씩 읽기
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- 파일을 여는 방향은 `direction-input`(읽기), `direction-output`(쓰기. 기존 내용은 버려진다),
  `direction-append`(끝에 추가)의 세 가지이다.
- `read-line`은 파일 끝에서 `none`을 반환한다.
- `with-open-file` 대신 `open-file`로 열었다면 반드시 `close`를 호출한다. GC는 스트림을 닫지 않는다.

바이트를 읽고 쓰려면 `open-binary-input` / `open-binary-output`으로 열고 `read-byte` / `write-byte`를 쓴다. 문자
스트림과 바이트 스트림은 다른 타입이므로 문자 스트림에서 바이트를 읽으려 하면 타입 오류이다.

### 문자열을 스트림으로 쓰기

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### 표준 입출력

`*standard-input*`, `*standard-output*`, `*error-output*`도 스트림이다. `(read-line *standard-input*)`은 한 줄을
읽는다.

### 읽고 쓰는 함수를 일반화하기

스트림은 종류마다 다른 타입이지만 공통 연산은 트레이트로 모여 있다. 인수를 `(where (CharInput S))`로 받는 함수는
파일, 문자열, 소켓 어느 것에서든 읽을 수 있다.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. 경로명

파일 이름을 받는 함수는 문자열과 `pathname` 어느 쪽이든 받는다. 경로를 부분으로 나누거나 조립하려면 `pathname`을
쓴다.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; 확장자만 바꾸기
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; 디렉터리 안에 파일 이름 넣기
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

파일 시스템 조작:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; 부모까지 함께 만든다
(probe-file "out/deep")                           ; => true (있다)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => 내용의 목록
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

구분자는 언제나 `/`이다. Common Lisp 경로명의 호스트, 디바이스, 버전 구성 요소는 없고, 와일드카드와 논리 경로명도 없다.

## 5. TCP

소켓 연결도 스트림이므로 `read-line`과 `write-line`이 그대로 동작한다.

### 서버

연결마다 태스크를 하나 시작하는 것이 기본형이다. `accept`와 `read-line`은 데이터가 올 때까지 **그 태스크만** 멈추므로
다른 연결의 처리는 계속된다.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; 상대가 닫았다
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"`은 이 기기에서 오는 연결만, `"0.0.0.0"`은 모든 인터페이스에서 오는 연결을 받는다. 태스크에 대해서는
[문법 레퍼런스 12장](../reference/syntax.md#12-동시성태스크)을 참고한다.

### 클라이언트

`with-connection`은 연결하고 본체를 실행한 뒤 마지막에 연결을 닫는다. 값은 `Result<본체의 값, NetError>`이다.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

연산에 시간 제한을 두려면 `:timeout`(초)을 넘긴다.

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; 다음 읽기에 시계를 붙인다
```

### 상대가 끊었을 때

상대가 연결을 리셋하거나 도중에 끊어도 `panic`하지 않는다. 이후의 읽기는 `none`을 반환하고, 쓰기는 조용히 버려진다.
깨끗하게 닫혔는지 실패했는지 구별하려면 `(socket-error c)`를 확인한다.

## 6. 이름 해석(DNS)

이름 해석 전용 함수는 없다. `tcp-connect`, `tls-connect`, `send-to`에 호스트 이름을 넘기면 그 안에서 해석된다. 해석은
다른 스레드에서 일어나므로 기다리는 동안에도 다른 태스크는 계속 실행된다. 이름에 주소가 여러 개 있으면 차례로 시도한다.

이름을 해석하지 못하면 `Err`이 반환된다.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect`는 `tcp-connect`처럼 쓰며, 연결한 뒤 TLS 핸드셰이크를 한다. 서버 인증서는 표준 루트 인증서로 검증된다.
결과는 보통의 `socket-stream`이므로 읽기와 쓰기는 TCP와 같다.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

TLS 서버는 `tls-listen`에 인증서와 개인 키 파일(PEM)을 넘겨 만든다.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; TCP의 serve가 그대로 동작한다
          ((err e) (println "accept: ~a" (message e))))))
```

자체 서명 인증서로 시험할 때는 클라이언트 쪽에 `:ca-file "cert.pem"`을 넘겨 그 인증서를 신뢰하게 한다. 상호 TLS와
하나의 리스너로 여러 사이트를 제공하는 방법은 [네트워크](../reference/functions/network.md#2-tcp--tls--unix-도메인)에서
다룬다.

## 8. UDP

UDP는 스트림이 아니며 데이터그램을 하나씩 보내고 받는다. 데이터는 바이트 열(`Vector<int>`)이고, 문자열과의 변환은
`string->utf8` / `utf8->string`으로 한다.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; 보내기에도 소켓이 필요하다. 0은 포트를 OS에 맡긴다
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)`는 보낸 쪽의 `ip:port`이며 그대로 `send-to`의 대상으로 쓸 수 있다.

## 9. Unix 도메인 소켓

`unix-listen` / `unix-connect`에 소켓 파일의 경로를 넘긴다. 얻는 값은 TCP와 같은 `socket-stream`이다. `unix-listen`은
파일이 이미 있으면 `Err`을 반환한다. 리스너를 닫으면 파일이 지워진다.
