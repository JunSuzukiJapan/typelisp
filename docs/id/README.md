<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# Dokumentasi typelisp (Bahasa Indonesia)

typelisp adalah Lisp dengan tipe statis. Untuk cara memasang dan membangunnya, lihat
[README.md](../../README.md) (dalam bahasa Inggris) di direktori teratas repositori.

## Fitur

### Task dan kanal

Mulai task (thread ringan) dan oper nilai di antara task melalui kanal. Lihat
[Konkurensi](tutorial/concurrency.md) untuk detailnya.

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### Generik

Parameter tipe ditulis di dalam kurung sudut setelah nama tipe atau fungsi. Batas trait ditulis
di klausa `:where`. Lihat [Dasar-dasar Tipe](tutorial/types.md) dan [Trait](tutorial/traits.md)
untuk detailnya.

```lisp
(defstruct Pair<A,B> (first A) (second B))

(impl Pair<A,B>
  (swap (self) Pair<B,A>
    (make-instance 'Pair :first self::second :second self::first)))

(defun largest<T> ((xs Vector<T>)) Option<T> (:where (Ord T))
  (let ((best (the Option<T> (Option::none))))
    (doiter (x (iter xs))
      (setf best (Option::some (match best
                                 ((some b) (if (> x b) x b))
                                 ((none) x)))))
    best))

(let ((p (swap (make-instance 'Pair :first 1 :second "one"))))
  (println "~a ~a" p::first p::second))                       ; one 1

(println "~a" (largest (the Vector<int> #(3 9 4))))           ; (some 9)
(println "~a" (largest (the Vector<string> #("pear" "fig")))) ; (some pear)
```

### Option dan Result sebagai pengganti nil

Tidak ada `nil`. Fungsi yang mungkin tidak punya nilai mengembalikan `Option<T>`, dan fungsi yang
mungkin gagal mengembalikan `Result<T,E>`; isinya diambil dengan `match`. Memakai `Option<string>`
sebagai `string`, atau lupa menulis kasus `none` dalam `match`, menjadi galat tipe sebelum program
dijalankan. Lihat [Dasar-dasar Tipe](tutorial/types.md) dan [Penanganan
Kesalahan](tutorial/errors.md) untuk detailnya.

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### Merangkai dengan `->`

Makro `->` meneruskan sebuah nilai secara berurutan sebagai argumen pertama setiap bentuk
berikutnya. `(-> x (f a) (g b))` sama dengan `(g (f x a) b)`. Langkah-langkah yang mengembalikan
`Option` atau `Result` dapat ditulis dari atas ke bawah. Lihat
[Option, Result, dan Tipe Kesalahan](reference/functions/option-result.md) untuk detailnya.

```lisp
(use std::num::parse-int)
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2))
      (Option::some (truncate n 2))
      (Option::none)))

(println "~a"
  (-> (Option::some 40)
      (and-then half)                          ; (some 20)
      (and-then half)                          ; (some 10)
      (map (lambda ((x int)) int (+ x 1)))     ; (some 11)
      (unwrap-or 0)))                          ; 11

(println "~a"
  (-> (parse-int "abc")
      (map (lambda ((n int)) int (* n 2)))
      (unwrap-or -1)))                         ; -1
```

### Makro yang diperiksa tipenya

Makro ditulis dengan `defmacro`, quasiquote, dan `gensym` seperti di Common Lisp. Kode hasil
ekspansi juga diperiksa tipenya seperti kode yang ditulis tangan; jika tipenya tidak cocok, galat
dilaporkan di tempat makro dipanggil. Lihat [Makro](tutorial/macros.md) untuk detailnya.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## Tutorial

Jika Anda baru mengenal typelisp, bacalah berurutan.

- [Memulai](tutorial/intro.md): REPL, fungsi, variabel, percabangan, perulangan, daftar, dan `Vector`
- [Dasar-dasar Tipe](tutorial/types.md): tipe statis, `Option`, `Result`, struct, enum, generik
- [Trait](tutorial/traits.md): `deftrait` / `impl`, batas trait, `:dyn`
- [Makro](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Penanganan Kesalahan](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Konkurensi](tutorial/concurrency.md): task, kanal, `select`, `Mutex`, `thread`

## Panduan

- [Modul dan Tata Letak Berkas](guide/modules.md): `use`, `pub`, cara berkas dipetakan ke modul
- [Kompilasi](guide/compile.md): JIT, membangun berkas eksekusi dengan kompilasi AOT, dump
- [I/O Berkas, Stream, dan Jaringan](guide/io.md): berkas, pathname, TCP / TLS / UDP, resolusi nama
- [FFI C](guide/ffi.md): memanggil fungsi C dengan `defffi` (termasuk callback dan struct C dengan `def-c-struct`)
- [Integrasi Editor](guide/editors.md): `typl-lsp` dan cara menyiapkan VS Code / Emacs
- [Untuk Pemrogram Common Lisp](guide/from-common-lisp.md): perbedaan typelisp dengan CL dan cara menulis ulang kode CL

## Referensi

- [Referensi Sintaks](reference/syntax.md): sintaks leksikal, penulisan tipe, definisi, bentuk kendali, kompilasi, konkurensi
- [Fungsi Bawaan](reference/functions/README.md): fungsi bawaan, metode, dan pustaka standar
- [Tipe](reference/types.md): daftar tipe beserta trait yang diimplementasikan masing-masing
- [Pesan Kesalahan](reference/errors.md): arti kesalahan yang umum dan cara memperbaikinya

## Integrasi Editor

Langkah penyiapan ada di [panduan Integrasi Editor](guide/editors.md). Pintasan papan ketik dan
pengaturan tiap editor dicantumkan dalam dokumen berikut:

- [Emacs (typelisp-mode)](../../editor/emacs/README_id.md)
- [VS Code](../../editor/vscode/README_id.md)
