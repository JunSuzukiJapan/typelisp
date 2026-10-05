<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 네트워크 (TCP / TLS / Unix 도메인 / UDP)

소켓은 [스트림](streams-files.md)의 일원이다. 연결은 `socket-stream`(문자)과 `socket-byte-stream`(바이트)이라는
**같은 연결의 두 가지 보기**로 보이며, `read-line`/`write-line`/`read-byte`/`format`이 그대로 동작한다. 리스너는
`socket-listener`이다. **TCP, TLS, Unix 도메인 소켓은 하나의 타입을 공유한다**(Go의 `net.Conn`처럼). 연결된 뒤의 읽기와
쓰기는 같고, 만드는 방법만 다르다. UDP는 스트림이 아니라 데이터그램(`udp-socket`)이다.

**기다리는 것은 스레드가 아니라 태스크이다.** `accept`, `read-line`, `write-string`, `tcp-connect`(이름 해석 포함),
`recv-from`은 준비되지 않았으면 모두 *그 태스크*를 멈추고(`sleep`/`recv`처럼), 다른 태스크는 계속 실행된다. 그래서
서버는 연결마다 `(task (serve c))`라는 Go와 같은 모양으로 쓸 수 있다([문법 레퍼런스 12.5](../syntax.md#125-전환이-일어나는-곳)).

## 1. 타입

| 타입 | 구현하는 트레이트 | 얻는 방법 |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream`(`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | —(`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | 아래 함수들의 `Err` |

스트림 타입은 요소 타입을 하나 가지므로(`file-stream`/`binary-file-stream`과 같은 이유) 문자와 바이트는 다른 타입이다.
`byte-stream-of`/`char-stream-of`는 **같은 연결**을 가리키며 받기 버퍼를 공유하는 값을 반환한다. 헤더는 문자로, 본문은
바이트로 읽는 HTTP 같은 것은 이렇게 쓴다. 문자를 `unread-char`한 직후에 바이트를 읽으면 오류이다(파일과 같은 규칙).

## 2. TCP / TLS / Unix 도메인

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | 연결한다. `host`는 이름이든 주소든 된다. 이름에 주소가 여러 개 있으면 차례로 시도한다(`localhost`는 `::1`과 `127.0.0.1`). 이름 조회 실패, 연결 거부, `:timeout`초 초과는 `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | `tcp-connect` 뒤에 TLS. 인증서는 `host` 이름(검증할 이름이 연결할 곳과 다르면 `:server-name`)에 대해 Mozilla의 루트 인증서로 검증된다. `:ca-file`(PEM)을 주면 **그 안의 인증서만** 신뢰한다(사설 CA, 또는 자신의 `tls-listen`이 제시하는 바로 그 인증서). `:cert-file`/`:key-file`(둘 다 주거나 둘 다 주지 않는다)은 이쪽의 인증서이며, 서버가 요구하면 제시한다(상호 TLS). 핸드셰이크는 여기서 완료되므로 서버의 인증서가 통과하지 못하면 이 호출이 `Err`을 반환한다. 결과는 보통의 `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Unix 도메인 소켓 `path`에 연결한다. 로컬이므로 핸드셰이크 대기도 타임아웃도 없다 |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | 기다린다. `"127.0.0.1"`은 이 기기만, `"0.0.0.0"`은 모든 인터페이스. `port`에 `0`을 주면 OS가 고른다 |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | `tcp-listen`의 TLS판. `cert-file`은 인증서 체인(PEM, 자신의 인증서가 먼저), `key-file`은 개인 키. 둘 다 여기서 읽고 검사하므로 맞지 않는 키는 첫 클라이언트 때가 아니라 이 호출에서 `Err`이 된다. `accept`는 **핸드셰이크 전에** 반환하며, 연결을 처리하는 태스크의 첫 읽기나 쓰기가 핸드셰이크를 완료한다(Go의 `tls.Conn`처럼). 그래서 핸드셰이크가 느린 클라이언트가 다른 `accept`를 붙잡지 않는다. `:client-ca`(PEM)를 주면 **모든 클라이언트에게** 그 안의 CA가 발행한 인증서의 제시를 요구한다(상호 TLS). 없으면 요구하지 않는다 |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | `path`에서 기다린다. **파일이 이미 있으면 `Err`**(실행 중인 다른 프로세스의 것일 수 있으므로 조용히 바꾸지 않는다). `close`하면 파일이 지워진다 |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | 다음 연결. 올 때까지 태스크를 멈춘다. `:timeout`초가 지나면 `Err`로 포기한다 |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | 기다리지 않고 읽을 수 있게 될 때까지, 또는 `secs`초. `true`는 전자(이미 버퍼에 있는 데이터 포함). 읽기에 시계를 붙이는 방법: `(if (wait-readable c 5.0) (read-line c) ...)`. 약속하는 것은 **다음 읽기가 멈추지 않는다**는 것이며, `read-line`은 줄의 나머지를 기다릴 수 있다 |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | 쓸 수 있게 될 때까지, 또는 `secs`초 |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | `tls-listen`의 리스너에 인증서를 하나 더 추가하고, `name`을 요구하는 클라이언트(SNI)에게 제시한다. 하나의 리스너로 여러 사이트. 체인이 `name`의 것인지 여기서 검사하고, 아니면 이 호출이 `Err`을 반환한다. 아무도 추가하지 않은 이름을 요구하거나 이름을 요구하지 않는 클라이언트는 `tls-listen`의 인증서를 받는다 |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | 상대 인증서의 주체(`CN=client,O=Example,C=JP`, RFC 4514 형식, 가장 구체적인 것이 먼저). 상호 TLS 서버가 "누가 연결했는지" 아는 방법(핸드셰이크를 완료하는 첫 읽기 뒤에). 클라이언트 쪽에서는 서버 인증서의 이름. 평문 연결, 핸드셰이크 전, 상대가 인증서를 제시하지 않았으면 `none` |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | 서버 쪽 TLS 연결에서 클라이언트가 요구한 이름(SNI). `tls-add-certificate`로 여러 사이트를 둔 서버가 "어느 사이트인지" 아는 방법. 평문 연결, 클라이언트 쪽, 이름이 없으면(주소로 연결) `none` |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Nagle을 끈다(`TCP_NODELAY`). `write-string`은 매번 소켓까지 가므로 Nagle이 켜져 있으면 헤더와 본문 두 번에 나누어 쓴 응답이 상대의 지연 ACK를 기다리게 된다. 요청/응답 프로토콜에서는 `true`로 한다. Unix 도메인 소켓에는 Nagle이 없으며 그냥 성공한다 |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | 유휴 중에 상대를 탐지한다(`SO_KEEPALIVE`). 닫지 않고 사라진 상대(뽑힌 케이블, 멈춘 호스트)를 감지해 연결을 리셋한다. OS 기본 주기는 길므로(흔히 2시간) 아래 `set-keepalive-period`와 함께 쓴다. Unix 도메인 소켓에는 없으므로 panic한다 |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | 첫 탐지까지의 유휴 초 수와 탐지 사이의 간격(정수 초, 1 이상). Go의 `SetKeepAlivePeriod`처럼 둘 다 같은 값으로 설정한다 |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | **상대가** 이 연결을 망가뜨렸다면 그 첫 실패(리셋, TLS 경고, 쓰기 중 끊김). 정상이면 `none`. 상대가 깨끗하게 닫은 EOF는 실패가 아니다. 아래 "상대 쪽의 실패" 참고 |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | 이쪽의 `host:port`. `(tcp-listen h 0)` 뒤에 선택된 포트를 아는 방법. Unix 소켓에서는 경로(연결하는 쪽은 `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | 상대의 `host:port`, Unix 소켓에서는 경로 |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | 보내는 쪽만 닫는다(하프 클로즈). 상대는 EOF를 읽고, 이쪽은 아직 읽을 수 있다. "요청을 다 보냈다"는 신호. TLS에서는 `close_notify`도 보낸다 |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | 같은 연결의 바이트판 |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | 같은 연결의 문자판 |
| `close` | `(close s)` | `Stream` | 버퍼를 내보낸 뒤 닫는다. GC는 닫지 않는다 |
| `with-connection` | `(with-connection (var host port) body...)` | 매크로 | 연결하고, 본체를 실행하고, 닫는다. `Result<본체의 값, NetError>`(`with-open-file`과 같은 모양) |

`write-string`/`write-line`은 **모두 쓴 뒤에 반환한다**(Go의 `net.Conn.Write`처럼). 작은 쓰기를 묶으려면
`string-output-stream`에 모아서 한 번에 쓴다. `listen`은 받기 버퍼에 무언가 있을 때만 `true`이므로
`read-char-no-hang`은 이름대로 동작한다.

**상대 쪽의 실패는 panic하지 않는다.** 상대가 연결을 리셋하거나, TLS 핸드셰이크를 거부하거나, 쓰기 도중에 끊어도
이 프로그램의 실수가 아니므로 서버가 다른 클라이언트와 함께 멈추지는 않는다. 첫 실패는 연결에 기록되고, 이후의 읽기는
`none`을 반환하며(EOF와 똑같이 보인다), 쓰기는 어디에도 가지 않고 조용히 반환한다. 구별하려면 Go의
`bufio.Scanner.Err`와 같은 모양인 `socket-error`를 쓴다(`read-item`은 `Option`이며 따로 알릴 통로가 없다). panic하는
것은 프로그램 자신의 실수(닫힌 핸들, `unread-char` 직후의 바이트 읽기)뿐이다.

**타임아웃은 명시적인 인수 또는 `wait-readable`이다.** Go의 `SetReadDeadline`처럼 스트림이 기한을 가지고
`read-line`이 실패하는 형태는 없다. `read-item`은 `Option<Item>`을 반환하며 오류를 반환할 방법이 없기 때문이다. 시계가
필요한 곳은 연결, 받아들이기, "다음 읽기"의 세 곳이며, 각각에 인수가 있다.

```lisp
;; 서버: 연결마다 태스크 하나
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; 클라이언트
(match (with-connection (c "127.0.0.1" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "~a" reply))
  ((err e) (println "~a" (message e))))

;; HTTPS
(let ((c (unwrap (tls-connect "example.com" 443 :timeout 10.0))))
  (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
  (println "~a" (unwrap (read-line c)))       ; HTTP/1.1 200 OK
  (close c))

;; TLS 서버(인증서는 예를 들어 다음으로 만들 수 있다
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; 위의 serve 그대로. 첫 read-line이 핸드셰이크
          ((err e) (println "accept: ~a" (message e))))))
;; 그 클라이언트: 자신의 인증서를 신뢰하고 연결한다
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; 상호 TLS: 서버는 ca.pem이 발행한 클라이언트 인증서를 요구하고, 클라이언트는 그것을 제시한다
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; 서버 쪽에서 첫 read-line 뒤에: 누가 연결했는가
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; 하나의 리스너에 두 사이트(SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; 연결에서 (requested-server-name c)가 어느 쪽인지 알려 준다
```

상호 TLS에서 인증서를 제시하지 않는(또는 통과하지 못하는) 클라이언트는 TLS 1.3에서는 서버가 판단하기 전에 자기 쪽
핸드셰이크를 마치므로 `tls-connect`는 `Ok`를 반환하고 **첫 읽기가 `none`을 반환한다**(경고는 `socket-error`에 있다).
서버 쪽에서도 같은 연결의 첫 읽기가 `none`을 반환한다. 어느 쪽도 panic하지 않는다.

## 3. UDP

| 이름 | 쓰는 법 | 타입 | 의미 |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | 소켓을 만든다. 보내기만 할 때도 필요하다(`port`는 `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | 데이터그램을 하나 보낸다. 이름은 해석된다. 도착했는지는 알 수 없다(UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | 다음 데이터그램. `from`은 `ip:port`이며 그대로 `send-to`의 `host`로 넘길 수 있다 |

문자열과 바이트 열의 변환은 `string->utf8` / `utf8->string`으로 한다([문자열](collections.md#1-문자열-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. 없는 것

- 상대 인증서에서 **주체 이외의 것**(SAN, 유효 기간, 발행자)을 읽는 방법.
- **`select`로 소켓과 채널을 동시에 기다리기.** Go와 마찬가지로 "읽는 태스크를 시작해 채널에 흘려보내게 한다"로 쓴다.
- **HTTP/2**. Unix 도메인 **데이터그램**(`SOCK_DGRAM`).
- **스트림이 가지는 기한**(위의 이유로 시계는 인수이다).
