<!-- translated-from: docs/ja/guide/io.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Файловий ввід-вивід, потоки та мережа

Цей посібник показує основи читання й запису файлів, шляхів і комунікації через сокети. Переліки
функцій наведено в розділах [Потоки та файли](../reference/functions/streams-files.md) і
[Мережа](../reference/functions/network.md).

## 1. Невдачі повертаються як `Result`

Операції, що можуть зазнати невдачі залежно від середовища, як-от відкриття файлу чи з'єднання,
повертають `Result`. Відсутній файл — це не помилка в програмі, тож `panic` не виникає. Розділяйте
результати через `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; if the file is missing:
;; error: config.txt: No such file or directory (os error 2)
```

Коли ви знаєте, що операція не може зазнати невдачі, або в невеликому скрипті, де зупинка при невдачі
припустима, `unwrap` виймає значення. Якщо це `Err`, він завершується через panic.

## 2. Читання й запис цілого файлу

Найпростіші функції обробляють файл цілком.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. Читання й запис через потоки

Щоб читати чи писати потроху, відкрийте потік через `with-open-file`. Як би не залишили тіло, потік
закривається. Значення — це `Result<значення тіла, FileError>`.

```lisp
;; write
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; read one line at a time
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Файл можна відкрити в трьох напрямках: `direction-input` (читання), `direction-output` (запис;
  наявний вміст відкидається) і `direction-append` (дописування в кінець).
- `read-line` повертає `none` у кінці файлу.
- Якщо ви відкриваєте файл через `open-file` замість `with-open-file`, завжди викликайте `close`. GC
  потоків не закриває.

Щоб читати й писати байти, відкрийте файл через `open-binary-input` / `open-binary-output` і
використовуйте `read-byte` / `write-byte`. Символьні потоки та потоки байтів — різні типи, тож спроба
прочитати байти із символьного потоку — це помилка типів.

### Використання рядка як потоку

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Стандартний ввід і вивід

`*standard-input*`, `*standard-output*` і `*error-output*` — теж потоки.
`(read-line *standard-input*)` читає один рядок.

### Узагальнення функцій читання й запису

Кожен вид потоку — окремий тип, але спільні операції зібрано в трейти. Функція, що приймає аргумент
з `(where (CharInput S))`, може читати з файлу, рядка або сокета.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Шляхи (pathname)

Функції, що приймають ім'я файлу, приймають або рядок, або `pathname`. Використовуйте `pathname`, щоб
розбити шлях на частини або побудувати його.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; replace just the extension
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; put a file name inside a directory
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Операції з файловою системою:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; create it along with its parents
(probe-file "out/deep")                           ; => true (it exists)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => the list of contents
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Роздільник — завжди `/`. Складників хоста, пристрою чи версії, як у шляхів Common Lisp, немає, як немає
й шаблонів чи логічних шляхів.

## 5. TCP

З'єднання через сокет — теж потік, тож `read-line` і `write-line` працюють із ним як є.

### Сервер

Базовий шаблон — запускати одну задачу на з'єднання. `accept` і `read-line` зупиняють **лише цю задачу**,
доки не надійдуть дані, тож обробка інших з'єднань триває.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; the other side closed
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` приймає з'єднання лише з цієї машини, а `"0.0.0.0"` — на кожному інтерфейсі. Про задачі
див. [Довідник із синтаксису, розділ 12](../reference/syntax.md#12-конкурентність-задачі).

### Клієнт

`with-connection` з'єднується, виконує своє тіло й наприкінці закриває з'єднання. Значення — це
`Result<значення тіла, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Щоб обмежити операцію в часі, передайте `:timeout` (у секундах).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; put a clock on the next read
```

### Коли інша сторона розриває з'єднання

Якщо інша сторона скидає з'єднання або обриває його посередині, `panic` не виникає. Наступні читання
повертають `none`, а записи мовчки відкидаються. Щоб відрізнити, чи з'єднання закрито чисто, чи воно
зазнало збою, перевірте `(socket-error c)`.

## 6. Розв'язання імен (DNS)

Окремої функції для розв'язання імен немає. Передача імені хоста в `tcp-connect`, `tls-connect` або
`send-to` розв'язує його всередині них. Розв'язання відбувається в іншому потоці, тож інші задачі
продовжують виконуватися, поки воно чекає. Якщо в імені кілька адрес, їх пробують по черзі.

Якщо ім'я розв'язати не вдається, повертається `Err`.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` використовується як `tcp-connect` і виконує рукостискання TLS після з'єднання. Сертифікат
сервера перевіряється за стандартними кореневими сертифікатами. Результат — звичайний
`socket-stream`, тож читання й запис працюють як із TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

TLS-сервер створюється передачею файлів сертифіката й закритого ключа (PEM) у `tls-listen`.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; the TCP serve works as it is
          ((err e) (println "accept: ~a" (message e))))))
```

Під час випробування із самопідписаним сертифікатом передайте на боці клієнта `:ca-file "cert.pem"`, щоб
він довіряв цьому сертифікату. Взаємний TLS і обслуговування кількох сайтів одним слухачем описано в
розділі [Мережа](../reference/functions/network.md#2-tcp--tls--доменні-сокети-unix).

## 8. UDP

UDP — це не потік; він надсилає й отримує по одній дейтаграмі. Дані — це послідовність байтів
(`Vector<int>`); перетворюйте в рядки й назад через `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; even sending needs a socket; 0 leaves the port to the OS
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` — це `ip:port` відправника, який можна використати як є як адресу призначення для `send-to`.

## 9. Доменні сокети Unix

Передайте шлях до файлу сокета в `unix-listen` / `unix-connect`. Ви отримаєте той самий
`socket-stream`, що й з TCP. `unix-listen` повертає `Err`, якщо файл уже існує. Закриття
слухача видаляє файл.
