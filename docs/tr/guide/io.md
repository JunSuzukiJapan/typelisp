<!-- translated-from: docs/ja/guide/io.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Dosya G/Ç, Akışlar ve Ağ

Bu kılavuz, dosya okuma ve yazmanın, yol adlarının ve soket iletişiminin temellerini gösterir.
Fonksiyon listeleri [Akışlar ve Dosyalar](../reference/functions/streams-files.md) ve
[Ağ](../reference/functions/network.md) belgelerindedir.

## 1. Başarısızlıklar `Result` olarak döner

Bir dosyayı açmak ya da bağlanmak gibi ortama bağlı olarak başarısız olabilen işlemler bir `Result`
döndürür. Eksik bir dosya programda bir hata değildir; bu yüzden `panic` olmaz. Sonuçları `match` ile
ayırın.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; if the file is missing:
;; error: config.txt: No such file or directory (os error 2)
```

Bir işlemin başarısız olamayacağını bildiğinizde ya da başarısızlıkta durmanın sakıncasız olduğu
küçük bir betikte, `unwrap` değeri çıkarır. Bir `Err` ise panic olur.

## 2. Bir dosyanın tamamını okuma ve yazma

En kolay fonksiyonlar dosyanın tamamını bir kerede işler.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. Akışlarla okuma ve yazma

Her seferinde biraz okumak ya da yazmak için `with-open-file` ile bir akış açın. Gövdeden nasıl
çıkılırsa çıkılsın akış kapatılır. Değer `Result<gövdenin değeri, FileError>` olur.

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

- Bir dosyayı açmanın üç yönü vardır: `direction-input` (okuma), `direction-output` (yazma; mevcut
  içerik atılır) ve `direction-append` (sona ekleme).
- `read-line`, dosyanın sonunda `none` döndürür.
- Bir dosyayı `with-open-file` yerine `open-file` ile açarsanız her zaman `close` çağırın. GC
  akışları kapatmaz.

Bayt okumak ve yazmak için dosyayı `open-binary-input` / `open-binary-output` ile açın ve
`read-byte` / `write-byte` kullanın. Karakter akışları ile bayt akışları farklı türlerdir; bu yüzden
bir karakter akışından bayt okumaya çalışmak bir tür hatasıdır.

### Bir string'i akış olarak kullanma

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Standart girdi ve çıktı

`*standard-input*`, `*standard-output*` ve `*error-output*` da akışlardır.
`(read-line *standard-input*)` bir satır okur.

### Okuma ve yazma fonksiyonlarını jenerik yapma

Her akış türü kendi başına bir türdür, ancak ortak işlemler trait'lerde toplanmıştır. Bağımsız
değişkenini `(where (CharInput S))` ile alan bir fonksiyon, bir dosyadan, bir string'den ya da bir
soketten okuyabilir.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Yol adları

Bir dosya adı alan fonksiyonlar bir string ya da bir `pathname` kabul eder. Bir yolu parçalara
ayırmak ya da bir yol oluşturmak için `pathname` kullanın.

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

Dosya sistemi işlemleri:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; create it along with its parents
(probe-file "out/deep")                           ; => true (it exists)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => the list of contents
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Ayırıcı her zaman `/`'tir. Common Lisp yol adlarındaki gibi host, aygıt ya da sürüm bileşenleri
yoktur; joker karakterler ve mantıksal yol adları da yoktur.

## 5. TCP

Bir soket bağlantısı da bir akıştır; bu yüzden `read-line` ve `write-line` onun üzerinde olduğu gibi
çalışır.

### Sunucu

Temel desen, her bağlantı için bir task başlatmaktır. `accept` ve `read-line`, veri gelene kadar
**yalnızca o task'i** durdurur; böylece diğer bağlantıların işlenmesi sürer.

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

`"127.0.0.1"` yalnızca bu makineden gelen bağlantıları kabul eder, `"0.0.0.0"` ise her arayüzde
kabul eder. Task'ler için [Sözdizimi Başvurusu 12. bölüm](../reference/syntax.md#12-eşzamanlılık-taskler)'e
bakın.

### İstemci

`with-connection` bağlanır, gövdesini çalıştırır ve sonunda bağlantıyı kapatır. Değer
`Result<gövdenin değeri, NetError>` olur.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Bir işleme zaman sınırı koymak için `:timeout` (saniye cinsinden) geçirin.

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; put a clock on the next read
```

### Karşı taraf bağlantıyı kopardığında

Karşı taraf bağlantıyı sıfırlarsa ya da ortada keserse `panic` olmaz. Sonraki okumalar `none`
döndürür ve yazmalar sessizce atılır. Bağlantının temiz biçimde mi kapandığını yoksa başarısız mı
olduğunu anlamak için `(socket-error c)` değerine bakın.

## 6. Ad çözümleme (DNS)

Ad çözümleme için ayrılmış bir fonksiyon yoktur. `tcp-connect`, `tls-connect` ya da `send-to`'ya bir
host adı geçirmek, adı bunların içinde çözümler. Çözümleme başka bir thread'de yapılır; bu yüzden
beklerken diğer task'ler çalışmaya devam eder. Bir adın birden çok adresi varsa sırayla denenirler.

Ad çözümlenemezse bir `Err` döner.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect`, `tcp-connect` gibi kullanılır ve bağlandıktan sonra TLS el sıkışmasını yapar. Sunucu
sertifikası standart kök sertifikalara karşı doğrulanır. Sonuç sıradan bir `socket-stream`'dir; bu
yüzden okuma ve yazma TCP'deki gibi çalışır.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Bir TLS sunucusu, `tls-listen`'a sertifika ve özel anahtar dosyaları (PEM) geçirilerek oluşturulur.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; the TCP serve works as it is
          ((err e) (println "accept: ~a" (message e))))))
```

Kendinden imzalı bir sertifikayla denerken, istemci tarafında o sertifikaya güvenmesi için
`:ca-file "cert.pem"` geçirin. Karşılıklı TLS ve tek bir dinleyiciden birkaç siteye hizmet verme
[Ağ](../reference/functions/network.md#2-tcp--tls--unix-domain) belgesinde ele alınmıştır.

## 8. UDP

UDP bir akış değildir; her seferinde bir datagram gönderir ve alır. Veri bir bayt dizisidir
(`Vector<int>`); string'lerle dönüşümleri `string->utf8` / `utf8->string` ile yapın.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; even sending needs a socket; 0 leaves the port to the OS
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)`, göndericinin `ip:port` değeridir ve `send-to`'nun hedefi olarak olduğu gibi
kullanılabilir.

## 9. Unix domain soketleri

`unix-listen` / `unix-connect`'e soket dosyasının yolunu geçirin. Elde ettiğiniz değer, TCP'dekiyle
aynı `socket-stream`'dir. Dosya zaten varsa `unix-listen` bir `Err` döndürür. Dinleyiciyi kapatmak
dosyayı kaldırır.
