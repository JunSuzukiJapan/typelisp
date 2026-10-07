<!-- translated-from: docs/ja/guide/io.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Operacje na plikach, strumienie i sieć

Ten przewodnik pokazuje podstawy odczytu i zapisu plików, nazw ścieżek oraz komunikacji przez gniazda. Listy
funkcji znajdują się w [Strumieniach i plikach](../reference/functions/streams-files.md) oraz
[Sieci](../reference/functions/network.md).

## 1. Niepowodzenia wracają jako `Result`

Operacje, które mogą się nie udać w zależności od środowiska, takie jak otwarcie pliku czy nawiązanie połączenia, zwracają
`Result`. Brakujący plik nie jest błędem w programie, więc nie powoduje `panic`. Rozdziel wyniki
za pomocą `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; jeśli pliku brakuje:
;; error: config.txt: No such file or directory (os error 2)
```

Gdy wiesz, że operacja nie może się nie udać, albo w małym skrypcie, w którym zatrzymanie przy porażce jest w porządku,
`unwrap` wyjmuje wartość. Jeśli jest to `Err`, następuje panic.

## 2. Odczyt i zapis całego pliku

Najprostsze funkcje obsługują cały plik naraz.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. Odczyt i zapis za pomocą strumieni

Aby czytać lub zapisywać po trochu, otwórz strumień za pomocą `with-open-file`. Bez względu na to, jak ciało zostanie opuszczone,
strumień jest zamykany. Wartością jest `Result<wartość ciała, FileError>`.

```lisp
;; zapis
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; odczyt po jednej linii
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Istnieją trzy kierunki otwarcia pliku: `direction-input` (odczyt), `direction-output` (zapis;
  istniejąca zawartość jest odrzucana) i `direction-append` (dopisywanie na końcu).
- `read-line` zwraca `none` na końcu pliku.
- Jeśli otwierasz plik za pomocą `open-file` zamiast `with-open-file`, zawsze wywołaj `close`. GC
  nie zamyka strumieni.

Aby czytać i zapisywać bajty, otwórz plik za pomocą `open-binary-input` / `open-binary-output` i użyj
`read-byte` / `write-byte`. Strumienie znakowe i strumienie bajtowe to różne typy, więc próba
odczytu bajtów ze strumienia znakowego jest błędem typu.

### Używanie łańcucha znaków jako strumienia

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Standardowe wejście i wyjście

`*standard-input*`, `*standard-output*` i `*error-output*` także są strumieniami.
`(read-line *standard-input*)` czyta jedną linię.

### Uogólnianie funkcji odczytu i zapisu

Każdy rodzaj strumienia jest osobnym typem, ale wspólne operacje są zebrane w traity. Funkcja,
która przyjmuje argument z `(where (CharInput S))`, może czytać z pliku, z łańcucha znaków lub z gniazda.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Nazwy ścieżek (pathnames)

Funkcje przyjmujące nazwę pliku akceptują albo łańcuch znaków, albo `pathname`. Użyj `pathname`, aby podzielić
ścieżkę na części lub ją zbudować.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; zamiana samego rozszerzenia
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; umieszczenie nazwy pliku w katalogu
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Operacje na systemie plików:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; utwórz razem z katalogami nadrzędnymi
(probe-file "out/deep")                           ; => true (istnieje)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => lista zawartości
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Separatorem jest zawsze `/`. Nie ma składników host, urządzenie ani wersja, jak w nazwach ścieżek Common Lisp,
ani symboli wieloznacznych (wildcard) czy logicznych nazw ścieżek.

## 5. TCP

Połączenie przez gniazdo jest także strumieniem, więc `read-line` i `write-line` działają na nim bez zmian.

### Serwer

Podstawowy wzorzec polega na uruchomieniu jednego zadania na połączenie. `accept` i `read-line` zatrzymują **tylko to
zadanie** aż do nadejścia danych, więc obsługa innych połączeń trwa.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; druga strona zamknęła połączenie
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` przyjmuje połączenia tylko z tej maszyny, a `"0.0.0.0"` przyjmuje je na każdym
interfejsie. Informacje o zadaniach znajdziesz w [Referencji składni, rozdział 12](../reference/syntax.md#12-współbieżność-zadania).

### Klient

`with-connection` nawiązuje połączenie, wykonuje swoje ciało i na końcu zamyka połączenie. Wartością jest
`Result<wartość ciała, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Aby nałożyć limit czasu na operację, podaj `:timeout` (w sekundach).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; zegar na następny odczyt
```

### Gdy druga strona się rozłącza

Jeśli druga strona zresetuje połączenie lub zerwie je w połowie, nie powoduje to `panic`. Kolejne odczyty zwracają
`none`, a zapisy są po cichu odrzucane. Aby sprawdzić, czy połączenie zostało zamknięte poprawnie, czy
zakończyło się niepowodzeniem, sprawdź `(socket-error c)`.

## 6. Rozwiązywanie nazw (DNS)

Nie ma dedykowanej funkcji do rozwiązywania nazw. Przekazanie nazwy hosta do `tcp-connect`,
`tls-connect` lub `send-to` rozwiązuje ją wewnątrz nich. Rozwiązywanie odbywa się na innym wątku, więc inne
zadania działają dalej w czasie oczekiwania. Jeśli nazwa ma kilka adresów, są próbowane po kolei.

Jeśli nazwy nie da się rozwiązać, zwracany jest `Err`.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` używa się jak `tcp-connect`, a po nawiązaniu połączenia wykonuje on uzgadnianie TLS. Certyfikat serwera
jest weryfikowany względem standardowych certyfikatów głównych. Wynikiem jest zwykły
`socket-stream`, więc odczyt i zapis działają tak jak w TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Serwer TLS tworzy się, przekazując do `tls-listen` pliki certyfikatu i klucza prywatnego (PEM).

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; serve z TCP działa bez zmian
          ((err e) (println "accept: ~a" (message e))))))
```

Przy wypróbowywaniu z certyfikatem samopodpisanym przekaż po stronie klienta `:ca-file "cert.pem"`, aby
ufał temu certyfikatowi. Wzajemne TLS (mutual TLS) oraz obsługa kilku witryn z jednego nasłuchiwacza są
opisane w [Sieci](../reference/functions/network.md#2-tcp--tls--gniazda-domeny-unix).

## 8. UDP

UDP nie jest strumieniem; wysyła i odbiera po jednym datagramie. Dane to ciąg bajtów
(`Vector<int>`); konwersję z łańcuchów znaków i na nie wykonuje się za pomocą `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; nawet wysyłanie wymaga gniazda; 0 zostawia port systemowi
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` to `ip:port` nadawcy, którego można użyć bez zmian jako adresu docelowego w `send-to`.

## 9. Gniazda domeny Unix

Przekaż ścieżkę pliku gniazda do `unix-listen` / `unix-connect`. Otrzymywana wartość to ten sam
`socket-stream` co w TCP. `unix-listen` zwraca `Err`, jeśli plik już istnieje. Zamknięcie
nasłuchiwacza usuwa plik.
