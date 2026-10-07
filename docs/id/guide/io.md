<!-- translated-from: docs/ja/guide/io.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# I/O Berkas, Stream, dan Jaringan

Panduan ini menunjukkan dasar-dasar membaca dan menulis berkas, pathname, dan komunikasi socket.
Daftar fungsinya ada di [Stream dan Berkas](../reference/functions/streams-files.md) dan
[Jaringan](../reference/functions/network.md).

## 1. Kegagalan dikembalikan sebagai `Result`

Operasi yang dapat gagal bergantung pada lingkungan, seperti membuka berkas atau menyambung,
mengembalikan `Result`. Berkas yang tidak ada bukan kesalahan program, sehingga tidak `panic`.
Pisahkan hasilnya dengan `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; if the file is missing:
;; error: config.txt: No such file or directory (os error 2)
```

Ketika Anda tahu operasinya tidak mungkin gagal, atau pada skrip kecil yang berhenti saat gagal
tidak masalah, `unwrap` mengambil nilainya. Jika berupa `Err`, ia melakukan panic.

## 2. Membaca dan menulis seluruh berkas

Fungsi yang paling mudah menangani seluruh berkas sekaligus.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #<vector<string> "abc">)
```

## 3. Membaca dan menulis dengan stream

Untuk membaca atau menulis sedikit demi sedikit, buka stream dengan `with-open-file`. Bagaimanapun
badannya ditinggalkan, stream ditutup. Nilainya adalah `Result<nilai dari badan, FileError>`.

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

- Ada tiga arah untuk membuka berkas: `direction-input` (baca), `direction-output` (tulis; isi yang
  ada dibuang), dan `direction-append` (tambahkan di akhir).
- `read-line` mengembalikan `none` di akhir berkas.
- Jika Anda membuka berkas dengan `open-file` dan bukan `with-open-file`, selalu panggil `close`. GC
  tidak menutup stream.

Untuk membaca dan menulis byte, buka berkas dengan `open-binary-input` / `open-binary-output` dan
gunakan `read-byte` / `write-byte`. Stream karakter dan stream byte adalah tipe yang berbeda, jadi
mencoba membaca byte dari stream karakter adalah kesalahan tipe.

### Memakai string sebagai stream

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Masukan dan keluaran standar

`*standard-input*`, `*standard-output*`, dan `*error-output*` juga merupakan stream.
`(read-line *standard-input*)` membaca satu baris.

### Membuat fungsi baca dan tulis menjadi generik

Setiap jenis stream adalah tipenya sendiri, tetapi operasi bersamanya dikumpulkan ke dalam trait.
Fungsi yang menerima argumennya dengan `(where (CharInput S))` dapat membaca dari berkas, string,
atau socket.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Pathname

Fungsi yang menerima nama berkas menerima string atau `pathname`. Gunakan `pathname` untuk memecah
path menjadi bagian-bagian atau untuk menyusunnya.

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

Operasi sistem berkas:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; create it along with its parents
(probe-file "out/deep")                           ; => true (it exists)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => the list of contents
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

Pemisahnya selalu `/`. Tidak ada komponen host, device, atau versi seperti pada pathname Common Lisp,
dan tidak ada wildcard atau pathname logis.

## 5. TCP

Koneksi socket juga merupakan stream, sehingga `read-line` dan `write-line` bekerja padanya apa
adanya.

### Server

Pola dasarnya adalah memulai satu task per koneksi. `accept` dan `read-line` menghentikan **hanya
task itu** sampai data tiba, sehingga penanganan koneksi lain terus berlanjut.

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

`"127.0.0.1"` hanya menerima koneksi dari mesin ini, dan `"0.0.0.0"` menerimanya di setiap
antarmuka. Untuk task, lihat [Referensi Sintaks bab 12](../reference/syntax.md#12-konkurensi-task).

### Klien

`with-connection` menyambung, menjalankan badannya, dan menutup koneksi di akhir. Nilainya adalah
`Result<nilai dari badan, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Untuk memberi batas waktu pada suatu operasi, berikan `:timeout` (dalam detik).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; put a clock on the next read
```

### Ketika pihak seberang terputus

Jika pihak seberang mereset koneksi atau memutusnya di tengah jalan, tidak terjadi `panic`.
Pembacaan berikutnya mengembalikan `none`, dan penulisan dibuang tanpa pemberitahuan. Untuk
membedakan apakah koneksi ditutup dengan baik atau gagal, periksa `(socket-error c)`.

## 6. Resolusi nama (DNS)

Tidak ada fungsi khusus untuk resolusi nama. Memberikan nama host ke `tcp-connect`, `tls-connect`,
atau `send-to` menyelesaikannya di dalamnya. Resolusi berlangsung di thread lain, sehingga task lain
terus berjalan selama menunggu. Jika sebuah nama memiliki beberapa alamat, alamat-alamat itu dicoba
bergiliran.

Jika nama tidak dapat diselesaikan, `Err` dikembalikan.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` dipakai seperti `tcp-connect` dan melakukan jabat tangan TLS setelah tersambung.
Sertifikat server diverifikasi terhadap sertifikat akar standar. Hasilnya adalah `socket-stream`
biasa, sehingga membaca dan menulis bekerja seperti pada TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Server TLS dibuat dengan memberikan berkas sertifikat dan kunci privat (PEM) ke `tls-listen`.

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; the TCP serve works as it is
          ((err e) (println "accept: ~a" (message e))))))
```

Saat mencobanya dengan sertifikat yang ditandatangani sendiri, berikan `:ca-file "cert.pem"` di sisi
klien agar ia memercayai sertifikat itu. TLS timbal balik, dan melayani beberapa situs dari satu
listener, dibahas di [Jaringan](../reference/functions/network.md#2-tcp--tls--unix-domain).

## 8. UDP

UDP bukan stream; ia mengirim dan menerima satu datagram sekali waktu. Datanya adalah urutan byte
(`Vector<int>`); konversikan dari dan ke string dengan `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; even sending needs a socket; 0 leaves the port to the OS
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` adalah `ip:port` pengirim, yang dapat dipakai apa adanya sebagai tujuan `send-to`.

## 9. Socket domain Unix

Berikan path berkas socket ke `unix-listen` / `unix-connect`. Nilai yang Anda peroleh adalah
`socket-stream` yang sama seperti pada TCP. `unix-listen` mengembalikan `Err` jika berkasnya sudah
ada. Menutup listener menghapus berkasnya.
