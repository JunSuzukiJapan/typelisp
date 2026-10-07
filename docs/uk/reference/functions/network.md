<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Мережа (TCP / TLS / доменні сокети Unix / UDP)

Сокети — члени [потоків](streams-files.md). З'єднання видно у **двох поданнях того самого з'єднання**:
`socket-stream` (знаки) і `socket-byte-stream` (байти), і `read-line`/`write-line`/`read-byte`/`format`
працюють із ними як є. Слухач — це `socket-listener`. **TCP, TLS і доменні сокети Unix ділять один тип**
(як `net.Conn` у Go): після з'єднання читання й запис однакові, а відрізняється лише спосіб створення.
UDP — це не потік, а дейтаграми (`udp-socket`).

**Чекає задача, а не потік.** `accept`, `read-line`, `write-string`, `tcp-connect` (включно з
розв'язанням імен) і `recv-from` усі зупиняють *цю задачу*, якщо не готові (як `sleep`/`recv`), а інші
задачі продовжують виконуватися. Тому сервер можна писати в тій самій формі, що й у Go,
`(task (serve c))` на з'єднання ([Довідник із синтаксису 12.5](../syntax.md#125-де-задачі-перемикаються)).

## 1. Типи

| Тип | Реалізовані трейти | Як отримати |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | `Err` функцій нижче |

Тип потоку має один тип елемента (з тієї самої причини, що й `file-stream`/`binary-file-stream`), тож
знаки та байти — різні типи. `byte-stream-of`/`char-stream-of` повертають значення, що вказують на
**те саме з'єднання** і ділять буфер приймання: так пишуть речі на кшталт HTTP, де заголовки читаються як
знаки, а тіло як байти. Читання байта одразу після `unread-char` знака — це помилка (те саме правило, що
й для файлів).

## 2. TCP / TLS / доменні сокети Unix

| Назва | Використання | Тип | Значення |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | З'єднується. `host` може бути іменем або адресою. Якщо в імені кілька адрес, їх пробують по черзі (`localhost` — це `::1` і `127.0.0.1`). Невдале розв'язання імені, відмова у з'єднанні або перевищення `:timeout` секунд дає `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS після `tcp-connect`. Сертифікат перевіряється за іменем `host` (`:server-name`, якщо ім'я для перевірки відрізняється від місця з'єднання) за кореневими сертифікатами Mozilla. З `:ca-file` (PEM) довіряє **лише сертифікатам у ньому** (приватний ЦС або саме той сертифікат, який подає ваш `tls-listen`). `:cert-file`/`:key-file` (обидва або жоден) — це наш сертифікат, який подається, коли сервер його просить (взаємний TLS). Рукостискання завершується тут, тож якщо сертифікат сервера не проходить, цей виклик повертає `Err`. Результат — звичайний `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | З'єднується з доменним сокетом Unix `path`. Він локальний, тож очікування рукостискання та тайм-ауту немає |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Слухає. `"127.0.0.1"` — лише ця машина, `"0.0.0.0"` — кожен інтерфейс. Передача `0` як `port` дозволяє ОС вибрати |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | Версія `tcp-listen` для TLS. `cert-file` — ланцюг сертифікатів (PEM, власний сертифікат першим), а `key-file` — закритий ключ. Обидва читаються й перевіряються тут, тож ключ, що не збігається, дає `Err` від цього виклику, а не від першого клієнта. `accept` повертається **до рукостискання**, і перше читання чи запис задачею, що обробляє з'єднання, завершує рукостискання (як `tls.Conn` у Go), тож клієнт, повільний у рукостисканні, не затримує інші `accept`. З `:client-ca` (PEM) **вимагає від кожного клієнта** подати сертифікат, виданий ЦС із нього (взаємний TLS). Без нього жодного не просить |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Слухає за `path`. **`Err`, якщо файл уже існує** (він може належати іншому запущеному процесові, тож його мовчки не замінюють). `close` видаляє файл |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | Наступне з'єднання. Зупиняє задачу, доки не надійде. Здається з `Err` після `:timeout` секунд |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Доки можна буде читати без очікування, або `secs` секунд. `true` означає перше (включно з уже буферизованими даними). Спосіб поставити годинник на читання: `(if (wait-readable c 5.0) (read-line c) ...)`. Обіцяється, що **наступне читання не зупиниться**; `read-line` може чекати решту рядка |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Доки можна буде писати, або `secs` секунд |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Додає ще один сертифікат слухачеві `tls-listen`, який подається клієнтам, що просять `name` (SNI): кілька сайтів на одному слухачеві. Чи є ланцюг для `name`, перевіряється тут, і якщо ні, цей виклик повертає `Err`. Клієнти, що просять ім'я, якого ніхто не додав, або жодного імені, отримують сертифікат `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Суб'єкт сертифіката співрозмовника (`CN=client,O=Example,C=JP`, форма RFC 4514, найконкретніше першим). Як сервер із взаємним TLS дізнається, «хто під'єднався» (після першого читання, що завершує рукостискання). На стороні клієнта — ім'я сертифіката сервера. `none` для звичайних з'єднань, до рукостискання або якщо співрозмовник не подав сертифіката |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | На серверному TLS-з'єднанні — ім'я, яке попросив клієнт (SNI). Як сервер із кількома сайтами, доданими через `tls-add-certificate`, дізнається, «який сайт». `none` для звичайних з'єднань, на стороні клієнта або без імені (з'єднання за адресою) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Вимикає Nagle (`TCP_NODELAY`). `write-string` щоразу доходить до сокета, тож з увімкненим Nagle відповідь, записана двома частинами, заголовком і тілом, чекає відкладеного ACK співрозмовника: для протоколів запит/відповідь використовуйте `true`. Доменні сокети Unix не мають Nagle, і виклик просто успішний |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Опитує співрозмовника під час простою (`SO_KEEPALIVE`). Виявляє співрозмовника, що зник, не закривши з'єднання (висмикнутий кабель, зупинений хост), і скидає з'єднання. Типовий період ОС довгий (часто 2 години), тож поєднуйте з `set-keepalive-period` нижче. Доменні сокети Unix його не мають, тож завершується через panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Секунди простою до першої проби та інтервал між пробами (цілі секунди, не менше 1). Як `SetKeepAlivePeriod` у Go, обидва задаються тим самим значенням |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Якщо **співрозмовник** розірвав це з'єднання, його перша невдача (скидання, сповіщення TLS, розрив під час запису). `none`, якщо все гаразд: EOF від чистого закриття співрозмовником — не невдача. Див. «Збої співрозмовника» нижче |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | `host:port` нашої сторони. Спосіб дізнатися обраний порт після `(tcp-listen h 0)`. Для сокетів Unix — шлях (кінець, що з'єднується, — `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | `host:port` співрозмовника або шлях для сокетів Unix |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Закриває лише сторону надсилання (напівзакриття). Співрозмовник читає EOF, а ця сторона ще може читати. Сигнал «я надіслав увесь запит». Для TLS також надсилає `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | Байтова версія того самого з'єднання |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | Знакова версія того самого з'єднання |
| `close` | `(close s)` | `Stream` | Скидає буфер, потім закриває. GC його не закриває |
| `with-connection` | `(with-connection (var host port) body...)` | Макрос | З'єднується, виконує тіло, закриває. `Result<значення тіла, NetError>` (та сама форма, що й `with-open-file`) |

`write-string`/`write-line` **повертаються після запису всього** (як `net.Conn.Write` у Go). Щоб зібрати
дрібні записи разом, накопичуйте їх у `string-output-stream` і записуйте один раз. `listen` дорівнює
`true` лише коли в буфері приймання щось є, тож `read-char-no-hang` працює так, як каже його назва.

**Збої співрозмовника не спричиняють panic.** Якщо інший кінець скидає з'єднання, відхиляє рукостискання
TLS або обриває його посеред запису, це не помилка цієї програми, тож сервер не зупиняється разом зі
своїми іншими клієнтами. Перша невдача записується на з'єднанні; наступні читання повертають `none`
(виглядаючи як EOF), а записи нікуди не йдуть і мовчки повертаються. Щоб їх розрізнити, використовуйте
`socket-error`, у тій самій формі, що й `bufio.Scanner.Err` у Go (`read-item` — це `Option`, і в нього
немає іншого каналу для повідомлення). Panic спричиняють лише помилки самої програми (закритий дескриптор,
читання байта одразу після `unread-char`).

**Тайм-аути — це явні аргументи або `wait-readable`.** Форми на кшталт `SetReadDeadline` з Go, коли
потік несе дедлайн і `read-line` завершується невдачею, немає: `read-item` повертає `Option<Item>` і не
має способу повернути помилку. Годинник потрібен у трьох місцях: з'єднання, приймання та «наступне
читання», і кожне має для нього аргумент.

```lisp
;; server: one task per connection
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; client
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

;; TLS server (the certificate can be made, for example, with
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; the serve above as it is; the first read-line is the handshake
          ((err e) (println "accept: ~a" (message e))))))
;; its client: connect trusting its own certificate
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; mutual TLS: the server requires a client certificate issued by ca.pem, and the client presents one
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; on the server side, after the first read-line: who connected
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; two sites on one listener (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; on the connection, (requested-server-name c) says which
```

У разі взаємного TLS клієнт, що не подає сертифіката (або подає такий, що не проходить), завершує свою
сторону рукостискання до того, як сервер вирішить, за TLS 1.3, тож `tls-connect` повертає `Ok`, а **перше
читання повертає `none`** (зі сповіщенням у `socket-error`). На стороні сервера перше читання того самого
з'єднання теж повертає `none`. Жодна зі сторін не завершується через panic.

## 3. UDP

| Назва | Використання | Тип | Значення |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Створює сокет. Потрібен навіть для самого надсилання (`port` дорівнює `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Надсилає одну дейтаграму. Імена розв'язуються. Чи дійшла вона, невідомо (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Наступна дейтаграма. `from` — це `ip:port`, який можна передати як є як `host` для `send-to` |

Перетворюйте між рядками та послідовностями байтів через `string->utf8` / `utf8->string`
([Рядки](collections.md#1-рядки-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Чого немає

- Способу прочитати щось із сертифіката співрозмовника, **крім суб'єкта** (SAN, термін дії, видавець).
- **Очікування на сокеті та каналі одночасно через `select`.** Як і в Go, пишіть це як «запустити
  задачу, що читає, і нехай вона наповнює канал».
- **HTTP/2**. **Дейтаграми** доменних сокетів Unix (`SOCK_DGRAM`).
- **Дедлайнів, що їх несе потік** (з наведених вище причин годинники — це аргументи).
