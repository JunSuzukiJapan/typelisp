<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Сеть (TCP / TLS / Unix-сокеты / UDP)

Сокеты входят в число [потоков](streams-files.md). Соединение видно в **двух представлениях одного и того же
соединения** — `socket-stream` (символы) и `socket-byte-stream` (байты), и `read-line`/`write-line`/`read-byte`/
`format` работают с ними как есть. Слушатель — это `socket-listener`. **TCP, TLS и Unix-сокеты имеют один тип** (как
`net.Conn` в Go): после подключения чтение и запись одинаковы, различается лишь способ создания. UDP — не поток, а
датаграммы (`udp-socket`).

**Ждёт задача, а не поток ОС.** `accept`, `read-line`, `write-string`, `tcp-connect` (включая разрешение имён) и
`recv-from` останавливают *эту задачу*, если не готовы (как `sleep`/`recv`), а другие задачи продолжают работать.
Поэтому сервер можно написать в той же форме, что и в Go, — `(task (serve c))` на каждое соединение
([Справочник по синтаксису 12.5](../syntax.md#125-где-переключаются-задачи)).

## 1. Типы

| Тип | Реализуемые трейты | Как получить |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | `Err` функций ниже |

У типа потока один тип элементов (по той же причине, что у `file-stream`/`binary-file-stream`), поэтому символы и
байты — разные типы. `byte-stream-of`/`char-stream-of` возвращают значения, указывающие на **то же соединение** и
разделяющие буфер приёма: так пишут, например, HTTP, где заголовки читаются как символы, а тело — как байты. Чтение
байта сразу после `unread-char` символа — ошибка (то же правило, что и для файлов).

## 2. TCP / TLS / Unix-сокеты

| Имя | Использование | Тип | Смысл |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Подключается. `host` может быть именем или адресом. Если у имени несколько адресов, они пробуются по очереди (`localhost` — это `::1` и `127.0.0.1`). Неудачное разрешение имени, отказ в соединении или превышение `:timeout` секунд дают `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS после `tcp-connect`. Сертификат проверяется по имени `host` (`:server-name`, если проверяемое имя отличается от адреса подключения) с корневыми сертификатами Mozilla. С `:ca-file` (PEM) доверяет **только сертификатам в нём** (частный центр сертификации или тот самый сертификат, который предъявляет ваш собственный `tls-listen`). `:cert-file`/`:key-file` (оба или ни одного) — наш сертификат, предъявляемый, когда сервер его запрашивает (взаимный TLS). Рукопожатие завершается здесь, поэтому если сертификат сервера не проходит проверку, этот вызов возвращает `Err`. Результат — обычный `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Подключается к Unix-сокету `path`. Он локальный, поэтому нет ни ожидания рукопожатия, ни тайм-аута |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Слушает. `"127.0.0.1"` — только эта машина, `"0.0.0.0"` — все интерфейсы. Если передать `0` как `port`, порт выберет ОС |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | TLS-версия `tcp-listen`. `cert-file` — цепочка сертификатов (PEM, свой сертификат первым), `key-file` — закрытый ключ. Оба читаются и проверяются здесь, поэтому несовпадающий ключ даёт `Err` уже от этого вызова, а не при первом клиенте. `accept` возвращается **до рукопожатия**, и первое чтение или запись задачи, обрабатывающей соединение, завершает рукопожатие (как `tls.Conn` в Go), так что клиент, медленно выполняющий рукопожатие, не задерживает другие `accept`. С `:client-ca` (PEM) **требует от каждого клиента** сертификат, выданный центром сертификации из него (взаимный TLS). Без него сертификаты не запрашиваются |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Слушает на `path`. **`Err`, если файл уже существует** (он может принадлежать другому работающему процессу, поэтому молча не заменяется). `close` удаляет файл |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | Следующее соединение. Останавливает задачу, пока оно не придёт. По истечении `:timeout` секунд сдаётся с `Err` |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Пока можно читать без ожидания, или `secs` секунд. `true` означает первое (включая уже буферизованные данные). Способ поставить таймер на чтение: `(if (wait-readable c 5.0) (read-line c) ...)`. Обещается, что **следующее чтение не остановится**; `read-line` может ждать остаток строки |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Пока можно писать, или `secs` секунд |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Добавляет слушателю `tls-listen` ещё один сертификат, предъявляемый клиентам, запросившим `name` (SNI): несколько сайтов на одном слушателе. Здесь проверяется, подходит ли цепочка для `name`, и если нет, этот вызов возвращает `Err`. Клиенты, запросившие имя, которое никто не добавлял, или не указавшие имя, получают сертификат `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Субъект сертификата собеседника (`CN=client,O=Example,C=JP`, форма RFC 4514, от наиболее конкретного). Так сервер со взаимным TLS узнаёт, «кто подключился» (после первого чтения, которое завершает рукопожатие). На стороне клиента — имя сертификата сервера. `none` для обычных соединений, до рукопожатия или если собеседник не предъявил сертификат |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | На серверной стороне TLS-соединения — имя, запрошенное клиентом (SNI). Так сервер с несколькими сайтами, добавленными через `tls-add-certificate`, узнаёт, «какой сайт». `none` для обычных соединений, на стороне клиента или без имени (подключение по адресу) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Отключает Nagle (`TCP_NODELAY`). `write-string` каждый раз доходит до сокета, поэтому при включённом Nagle ответ, записанный в две части — заголовок и тело, — ждёт отложенного ACK собеседника: для протоколов запрос/ответ используйте `true`. У Unix-сокетов нет Nagle, и вызов просто завершается успешно |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Проверяет собеседника в периоды простоя (`SO_KEEPALIVE`). Обнаруживает собеседника, исчезнувшего без закрытия (выдернутый кабель, остановленный хост), и сбрасывает соединение. Период ОС по умолчанию велик (часто 2 часа), поэтому сочетайте с `set-keepalive-period` ниже. У Unix-сокетов этого нет, поэтому panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Секунды простоя до первой проверки и интервал между проверками (целые секунды, не меньше 1). Как `SetKeepAlivePeriod` в Go, оба задаются одним значением |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Если **собеседник** разрушил это соединение — его первая неудача (сброс, предупреждение TLS, разрыв во время записи). `none`, если всё в порядке: EOF из-за штатного закрытия собеседником неудачей не является. См. «Неудачи собеседника» ниже |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | `host:port` нашей стороны. Способ узнать выбранный порт после `(tcp-listen h 0)`. Для Unix-сокетов — путь (у подключающейся стороны — `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | `host:port` собеседника или путь для Unix-сокетов |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Закрывает только сторону отправки (полузакрытие). Собеседник читает EOF, а эта сторона ещё может читать. Сигнал «я отправил весь запрос». Для TLS также отправляет `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | Байтовая версия того же соединения |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | Символьная версия того же соединения |
| `close` | `(close s)` | `Stream` | Отправляет буфер, затем закрывает. Сборщик мусора не закрывает |
| `with-connection` | `(with-connection (var host port) body...)` | Макрос | Подключиться, выполнить тело, закрыть. `Result<значение тела, NetError>` (та же форма, что у `with-open-file`) |

`write-string`/`write-line` **возвращаются, когда всё записано** (как `net.Conn.Write` в Go). Чтобы объединить мелкие
записи, накопите их в `string-output-stream` и запишите один раз. `listen` равен `true` только когда в буфере приёма
что-то есть, поэтому `read-char-no-hang` работает так, как говорит его имя.

**Неудачи собеседника не вызывают panic.** Если другой конец сбрасывает соединение, отвергает рукопожатие TLS или
обрывает его посреди записи, это не ошибка этой программы, поэтому сервер не останавливается вместе с остальными
клиентами. Первая неудача записывается в соединение; последующие чтения возвращают `none` (выглядит как EOF), а записи
уходят в никуда и молча возвращаются. Чтобы их различить, используйте `socket-error`, по образцу
`bufio.Scanner.Err` в Go (`read-item` — это `Option`, и другого канала для сообщения у него нет). Panic вызывают
только ошибки самой программы (закрытый дескриптор, чтение байта сразу после `unread-char`).

**Тайм-ауты — это явные аргументы или `wait-readable`.** Формы, как `SetReadDeadline` в Go, где поток несёт срок и
`read-line` завершается неудачей, нет: `read-item` возвращает `Option<Item>` и не может вернуть ошибку. Таймер нужен
в трёх местах — при подключении, при приёме и при «следующем чтении», — и у каждого есть для этого аргумент.

```lisp
;; сервер: по задаче на соединение
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; клиент
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

;; TLS-сервер (сертификат можно создать, например, так:
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; serve выше как есть; первый read-line — это рукопожатие
          ((err e) (println "accept: ~a" (message e))))))
;; его клиент: подключиться, доверяя собственному сертификату
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; взаимный TLS: сервер требует клиентский сертификат, выданный ca.pem, а клиент его предъявляет
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; на стороне сервера после первого read-line: кто подключился
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; два сайта на одном слушателе (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; на соединении (requested-server-name c) говорит, какой
```

При взаимном TLS клиент, не предъявивший сертификат (или предъявивший непроходящий), в TLS 1.3 завершает свою часть
рукопожатия до того, как решит сервер, поэтому `tls-connect` возвращает `Ok`, а **первое чтение возвращает `none`** (с
предупреждением в `socket-error`). На стороне сервера первое чтение того же соединения тоже возвращает `none`. Ни одна
сторона не вызывает panic.

## 3. UDP

| Имя | Использование | Тип | Смысл |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Создаёт сокет. Нужен даже просто для отправки (`port` равен `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Отправляет одну датаграмму. Имена разрешаются. Дошла ли она, неизвестно (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Следующая датаграмма. `from` — это `ip:port`, его можно как есть передать как `host` в `send-to` |

Преобразование между строками и последовательностями байтов — через `string->utf8` / `utf8->string`
([Строки](collections.md#1-строки-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Чего нет

- Способа прочитать из сертификата собеседника что-либо **кроме субъекта** (SAN, срок действия, издателя).
- **Ожидания сокета и канала одновременно через `select`.** Как и в Go, пишите это как «запустить задачу, которая
  читает и наполняет канал».
- **HTTP/2**. **Датаграмм** Unix-сокетов (`SOCK_DGRAM`).
- **Сроков, переносимых потоком** (по указанным выше причинам таймеры — это аргументы).
