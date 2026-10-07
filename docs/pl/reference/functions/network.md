<!-- translated-from: docs/ja/reference/functions/network.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Sieć (TCP / TLS / gniazda domeny Unix / UDP)

Gniazda są członkami [strumieni](streams-files.md). Połączenie widziane jest w **dwóch widokach tego samego
połączenia**, `socket-stream` (znaki) i `socket-byte-stream` (bajty), a
`read-line`/`write-line`/`read-byte`/`format` działają na nich bez zmian. Nasłuchiwacz to
`socket-listener`. **TCP, TLS i gniazda domeny Unix dzielą jeden typ** (jak `net.Conn` w Go): po
nawiązaniu połączenia odczyt i zapis są takie same, różni się tylko sposób ich tworzenia. UDP nie jest
strumieniem, lecz datagramami (`udp-socket`).

**Czeka zadanie, a nie wątek.** `accept`, `read-line`, `write-string`, `tcp-connect`
(w tym rozwiązywanie nazw) i `recv-from` zatrzymują *to zadanie*, jeśli nie są gotowe (jak
`sleep`/`recv`), a inne zadania działają dalej. Dlatego serwer można napisać w takim samym kształcie jak
w Go, `(task (serve c))` na połączenie ([Referencja składni 12.5](../syntax.md#125-gdzie-zadania-się-przełączają)).

## 1. Typy

| Typ | Zaimplementowane traity | Jak go uzyskać |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | `Err` z poniższych funkcji |

Typ strumienia ma jeden typ elementu (z tego samego powodu co `file-stream`/`binary-file-stream`), więc
znaki i bajty to różne typy. `byte-stream-of`/`char-stream-of` zwracają wartości wskazujące
**to samo połączenie** i dzielące bufor odbiorczy: w ten sposób pisze się rzeczy takie jak HTTP, gdzie
nagłówki są czytane jako znaki, a treść jako bajty. Odczyt bajtu zaraz po `unread-char` dla
znaku jest błędem (ta sama reguła co dla plików).

## 2. TCP / TLS / gniazda domeny Unix

| Nazwa | Użycie | Typ | Znaczenie |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Nawiązuje połączenie. `host` może być nazwą lub adresem. Jeśli nazwa ma kilka adresów, są próbowane po kolei (`localhost` to `::1` i `127.0.0.1`). Nieudane wyszukanie nazwy, odrzucone połączenie lub przekroczenie `:timeout` sekund daje `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS po `tcp-connect`. Certyfikat jest weryfikowany względem nazwy `host` (`:server-name`, jeśli nazwa do weryfikacji różni się od miejsca, z którym się łączysz) za pomocą certyfikatów głównych Mozilli. Z podanym `:ca-file` (PEM) ufa **tylko certyfikatom w nim zawartym** (prywatne CA lub sam certyfikat, który przedstawia twoje `tls-listen`). `:cert-file`/`:key-file` (oba albo żaden) to nasz certyfikat, przedstawiany, gdy serwer o niego poprosi (mutual TLS). Uzgadnianie kończy się tutaj, więc jeśli certyfikat serwera nie przejdzie weryfikacji, to wywołanie zwraca `Err`. Wynikiem jest zwykły `socket-stream` |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Łączy z gniazdem domeny Unix `path`. Jest lokalne, więc nie ma oczekiwania na uzgadnianie ani limitu czasu |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Nasłuchuje. `"127.0.0.1"` to tylko ta maszyna, `"0.0.0.0"` to każdy interfejs. Przekazanie `0` jako `port` pozwala systemowi wybrać |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | Wersja TLS dla `tcp-listen`. `cert-file` to łańcuch certyfikatów (PEM, własny certyfikat pierwszy), a `key-file` to klucz prywatny. Oba są tutaj wczytywane i sprawdzane, więc niepasujący klucz daje `Err` z tego wywołania, a nie przy pierwszym kliencie. `accept` wraca **przed uzgadnianiem**, a pierwszy odczyt lub zapis przez zadanie obsługujące połączenie kończy uzgadnianie (jak `tls.Conn` w Go), więc klient wolno uzgadniający nie blokuje innych `accept`. Z podanym `:client-ca` (PEM) **wymaga od każdego klienta** przedstawienia certyfikatu wydanego przez CA z niego (mutual TLS). Bez niego żaden nie jest wymagany |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Nasłuchuje pod `path`. **`Err`, jeśli plik już istnieje** (może należeć do innego działającego procesu, więc nie jest po cichu zastępowany). `close` usuwa plik |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | Następne połączenie. Zatrzymuje zadanie, aż jakieś nadejdzie. Poddaje się z `Err` po `:timeout` sekundach |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Do chwili, gdy można czytać bez czekania, lub przez `secs` sekund. `true` oznacza to pierwsze (w tym dane już zbuforowane). Sposób na nałożenie zegara na odczyt: `(if (wait-readable c 5.0) (read-line c) ...)`. Obiecuje, że **następny odczyt nie zatrzyma się**; `read-line` może czekać na resztę linii |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Do chwili, gdy można pisać, lub przez `secs` sekund |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Dodaje jeszcze jeden certyfikat do nasłuchiwacza `tls-listen`, przedstawiany klientom, którzy proszą o `name` (SNI): kilka witryn na jednym nasłuchiwaczu. To, czy łańcuch jest dla `name`, jest sprawdzane tutaj, a jeśli nie, to wywołanie zwraca `Err`. Klienci proszący o nazwę, której nikt nie dodał, lub o żadną nazwę, dostają certyfikat z `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | Podmiot (subject) certyfikatu partnera (`CN=client,O=Example,C=JP`, postać RFC 4514, najbardziej szczegółowy pierwszy). Tak serwer mutual TLS dowiaduje się, „kto się połączył" (po pierwszym odczycie, który kończy uzgadnianie). Po stronie klienta nazwa certyfikatu serwera. `none` dla zwykłych połączeń, przed uzgadnianiem lub gdy partner nie przedstawił certyfikatu |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | W połączeniu TLS po stronie serwera nazwa, o którą poprosił klient (SNI). Tak serwer z kilkoma witrynami dodanymi przez `tls-add-certificate` dowiaduje się, „która witryna". `none` dla zwykłych połączeń, po stronie klienta lub bez nazwy (połączono po adresie) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Wyłącza Nagle (`TCP_NODELAY`). `write-string` za każdym razem idzie aż do gniazda, więc przy włączonym Nagle odpowiedź zapisana w dwóch częściach, nagłówku i treści, czeka na opóźnione ACK partnera: użyj `true` dla protokołów żądanie/odpowiedź. Gniazda domeny Unix nie mają Nagle i po prostu się powiedzie |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Sonduje partnera w stanie bezczynności (`SO_KEEPALIVE`). Wykrywa partnera, który zniknął bez zamknięcia (wyciągnięty kabel, zatrzymany host) i resetuje połączenie. Domyślny okres systemu jest długi (często 2 godziny), więc połącz z poniższym `set-keepalive-period`. Gniazda domeny Unix go nie mają, więc powoduje panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Sekundy bezczynności przed pierwszą sondą i odstęp między sondami (całe sekundy, co najmniej 1). Jak `SetKeepAlivePeriod` w Go, oba są ustawiane na tę samą wartość |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Jeśli **partner** zerwał to połączenie, jego pierwsze niepowodzenie (reset, alert TLS, rozłączenie w trakcie zapisu). `none`, jeśli zdrowe: EOF od partnera zamykającego poprawnie nie jest niepowodzeniem. Zobacz „Niepowodzenia partnera" poniżej |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | `host:port` naszej strony. Sposób na poznanie wybranego portu po `(tcp-listen h 0)`. Dla gniazd Unix ścieżka (łączący się koniec to `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | `host:port` partnera lub ścieżka dla gniazd Unix |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Zamyka tylko stronę wysyłającą (half-close). Partner czyta EOF, a ta strona nadal może czytać. Sygnał „wysłałem całe żądanie". Dla TLS wysyła także `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | Bajtowa wersja tego samego połączenia |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | Znakowa wersja tego samego połączenia |
| `close` | `(close s)` | `Stream` | Wysyła bufor, a potem zamyka. GC go nie zamyka |
| `with-connection` | `(with-connection (var host port) body...)` | Makro | Połącz, wykonaj ciało, zamknij. `Result<wartość ciała, NetError>` (ten sam kształt co `with-open-file`) |

`write-string`/`write-line` **wracają po zapisaniu wszystkiego** (jak `net.Conn.Write` w Go). Aby
połączyć małe zapisy, zbieraj je w `string-output-stream` i zapisz raz. `listen` jest `true` tylko
wtedy, gdy w buforze odbiorczym coś jest, więc `read-char-no-hang` działa tak, jak mówi jego nazwa.

**Niepowodzenia partnera nie powodują panic.** Jeśli druga strona zresetuje połączenie, odrzuci uzgadnianie TLS
lub zerwie je w środku zapisu, nie jest to błąd w tym programie, więc serwer nie zatrzymuje się
razem ze swoimi pozostałymi klientami. Pierwsze niepowodzenie jest zapisywane w połączeniu; późniejsze odczyty zwracają
`none` (wyglądając jak EOF), a zapisy nie trafiają nigdzie i wracają po cichu. Aby je rozróżnić, użyj
`socket-error`, w tym samym kształcie co `bufio.Scanner.Err` w Go (`read-item` jest `Option` i nie ma
innego kanału do zgłaszania). Panic powodują tylko błędy samego programu (zamknięty uchwyt, odczyt
bajtu zaraz po `unread-char`).

**Limity czasu to jawne argumenty lub `wait-readable`.** Nie ma formy, jak `SetReadDeadline` w Go,
w której strumień niesie termin i `read-line` kończy się niepowodzeniem: `read-item` zwraca `Option<Item>` i nie ma
sposobu na zwrócenie błędu. Zegar jest potrzebny w trzech miejscach: przy łączeniu, przyjmowaniu i „następnym odczycie",
a każde z nich ma na to argument.

```lisp
;; serwer: jedno zadanie na połączenie
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; klient
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

;; serwer TLS (certyfikat można wytworzyć na przykład za pomocą
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; powyższe serve bez zmian; pierwszy read-line to uzgadnianie
          ((err e) (println "accept: ~a" (message e))))))
;; jego klient: połącz, ufając jego własnemu certyfikatowi
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; mutual TLS: serwer wymaga certyfikatu klienta wydanego przez ca.pem, a klient go przedstawia
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; po stronie serwera, po pierwszym read-line: kto się połączył
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; dwie witryny na jednym nasłuchiwaczu (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; w połączeniu (requested-server-name c) mówi, która
```

W przypadku mutual TLS klient, który nie przedstawia certyfikatu (lub taki, który nie przechodzi weryfikacji), kończy własną
stronę uzgadniania, zanim serwer zdecyduje, w TLS 1.3, więc `tls-connect` zwraca `Ok`, a **pierwszy
odczyt zwraca `none`** (z alertem w `socket-error`). Po stronie serwera pierwszy odczyt
tego samego połączenia także zwraca `none`. Żadna ze stron nie powoduje panic.

## 3. UDP

| Nazwa | Użycie | Typ | Znaczenie |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Tworzy gniazdo. Potrzebne nawet tylko do wysyłania (`port` to `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Wysyła jeden datagram. Nazwy są rozwiązywane. Czy dotarł, nie wiadomo (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | Następny datagram. `from` to `ip:port` i można go przekazać bez zmian jako `host` w `send-to` |

Konwersję między łańcuchami znaków a ciągami bajtów wykonuje się za pomocą `string->utf8` / `utf8->string`
([Łańcuchy znaków](collections.md#1-łańcuchy-znaków-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. Czego nie ma

- Sposobu odczytu czegokolwiek z certyfikatu partnera **poza podmiotem** (SAN, okres ważności,
  wystawca).
- **Oczekiwania na gniazdo i kanał naraz za pomocą `select`.** Tak jak w Go, napisz to jako „uruchom zadanie, które
  czyta, i niech zasila kanał".
- **HTTP/2**. **Datagramów** domeny Unix (`SOCK_DGRAM`).
- **Terminów niesionych przez strumień** (z powodów opisanych wyżej zegary są argumentami).
