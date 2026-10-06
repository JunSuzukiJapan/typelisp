<!-- translated-from: docs/ja/guide/io.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Файловый ввод-вывод, потоки и сеть

Это руководство показывает основы чтения и записи файлов, работы с путями и обмена через сокеты. Списки функций
приведены в [Потоках и файлах](../reference/functions/streams-files.md) и [Сети](../reference/functions/network.md).

## 1. Неудачи возвращаются как `Result`

Операции, которые могут не удаться в зависимости от окружения, например открытие файла или подключение, возвращают
`Result`. Отсутствующий файл — не ошибка в программе, поэтому `panic` не происходит. Разделяйте исходы через `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; если файла нет:
;; error: config.txt: No such file or directory (os error 2)
```

Когда известно, что операция не может не удаться, или в маленьком скрипте, где остановка при неудаче допустима,
`unwrap` извлекает значение. Если это `Err`, происходит panic.

## 2. Чтение и запись файла целиком

Самые простые функции работают с файлом целиком.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. Чтение и запись через потоки

Чтобы читать или писать понемногу, откройте поток через `with-open-file`. Как бы ни было покинуто тело, поток
закрывается. Значение — `Result<значение тела, FileError>`.

```lisp
;; запись
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; чтение по одной строке
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Есть три направления открытия файла: `direction-input` (чтение), `direction-output` (запись; существующее
  содержимое отбрасывается) и `direction-append` (дописывание в конец).
- `read-line` возвращает `none` в конце файла.
- Если вы открываете файл через `open-file` вместо `with-open-file`, всегда вызывайте `close`. Сборщик мусора потоки
  не закрывает.

Чтобы читать и писать байты, откройте файл через `open-binary-input` / `open-binary-output` и используйте
`read-byte` / `write-byte`. Символьные и байтовые потоки — разные типы, поэтому попытка читать байты из символьного
потока — ошибка типа.

### Строка как поток

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Стандартный ввод и вывод

`*standard-input*`, `*standard-output*` и `*error-output*` тоже являются потоками. `(read-line *standard-input*)`
читает одну строку.

### Обобщённые функции чтения и записи

Каждый вид потока — отдельный тип, но общие операции собраны в трейты. Функция, принимающая аргумент с
`(where (CharInput S))`, может читать из файла, строки или сокета.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Пути

Функции, принимающие имя файла, принимают как строку, так и `pathname`. Используйте `pathname`, чтобы разбить путь на
части или собрать его.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; заменить только расширение
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; поместить имя файла в каталог
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Операции с файловой системой:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; создать вместе с родительскими
(probe-file "out/deep")                           ; => true (существует)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => список содержимого
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Разделитель всегда `/`. Нет компонентов хоста, устройства и версии, как в путях Common Lisp, нет шаблонов и
логических путей.

## 5. TCP

Соединение через сокет — тоже поток, поэтому `read-line` и `write-line` работают с ним как есть.

### Сервер

Основной приём — запускать по задаче на соединение. `accept` и `read-line` останавливают **только эту задачу**, пока
не придут данные, так что обработка других соединений продолжается.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; другая сторона закрыла соединение
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` принимает соединения только с этой машины, а `"0.0.0.0"` — на всех интерфейсах. О задачах см.
[главу 12 справочника по синтаксису](../reference/syntax.md#12-конкурентность-задачи).

### Клиент

`with-connection` подключается, выполняет тело и в конце закрывает соединение. Значение —
`Result<значение тела, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Чтобы ограничить время операции, передайте `:timeout` (в секундах).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; поставить таймер на следующее чтение
```

### Когда другая сторона разрывает соединение

Если другая сторона сбрасывает соединение или обрывает его на полпути, `panic` не происходит. Последующие чтения
возвращают `none`, а записи молча отбрасываются. Чтобы понять, было ли соединение закрыто штатно или с ошибкой,
проверьте `(socket-error c)`.

## 6. Разрешение имён (DNS)

Отдельной функции для разрешения имён нет. Если передать имя хоста в `tcp-connect`, `tls-connect` или `send-to`, оно
разрешается внутри них. Разрешение происходит в другом потоке, поэтому во время ожидания другие задачи продолжают
работать. Если у имени несколько адресов, они пробуются по очереди.

Если имя не удаётся разрешить, возвращается `Err`.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` используется как `tcp-connect` и после подключения выполняет рукопожатие TLS. Сертификат сервера
проверяется по стандартным корневым сертификатам. Результат — обычный `socket-stream`, так что чтение и запись
работают как с TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

TLS-сервер создаётся передачей в `tls-listen` файлов сертификата и закрытого ключа (PEM).

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; serve для TCP работает как есть
          ((err e) (println "accept: ~a" (message e))))))
```

При пробе с самоподписанным сертификатом передайте на стороне клиента `:ca-file "cert.pem"`, чтобы он доверял этому
сертификату. Взаимный TLS и обслуживание нескольких сайтов одним слушателем описаны в
[Сети](../reference/functions/network.md#2-tcp--tls--unix-сокеты).

## 8. UDP

UDP — не поток; он отправляет и получает по одной датаграмме. Данные — последовательность байтов (`Vector<int>`);
преобразование из строк и в строки — через `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; даже для отправки нужен сокет; 0 оставляет выбор порта ОС
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` — это `ip:port` отправителя, его можно как есть использовать как адрес назначения `send-to`.

## 9. Unix-сокеты

Передайте путь файла сокета в `unix-listen` / `unix-connect`. Получаемое значение — тот же `socket-stream`, что и для
TCP. `unix-listen` возвращает `Err`, если файл уже существует. Закрытие слушателя удаляет файл.
