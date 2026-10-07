<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Untuk Pemrogram Common Lisp

typelisp mewarisi sintaks Common Lisp (CL) dan banyak nama fungsinya, tetapi ia adalah bahasa dengan
tipe statis. Karena itu, kode CL tidak selalu berjalan apa adanya. Panduan ini mengumpulkan
hal-hal yang sering membuat orang yang terbiasa dengan CL tersandung, beserta cara menulis ulang
kodenya.

## 1. Tidak ada `nil` maupun `t`

Nilai boolean adalah `true` dan `false`. `nil` dan `t` tidak didefinisikan.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Hanya `bool` yang dapat menjadi kondisi.** Menulis `0` atau daftar kosong sebagai kondisi adalah
  kesalahan tipe. Tidak ada aturan bahwa "semua selain nil adalah benar".
- **Cabang else pada `if` tidak boleh dihilangkan.** `(if c x)` adalah kesalahan. Jika cabang else
  tidak diperlukan, gunakan `when` / `unless`.
- **"Tidak ada nilai" dinyatakan dengan `Option<T>`.** Fungsi yang di CL mengembalikan nil untuk
  berarti "tidak ditemukan" di sini mengembalikan `(some x)` atau `none`.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- Daftar kosong `()` adalah, tergantung konteks, nilai tipe Unit (nilai kembalian fungsi yang tidak
  mengembalikan apa-apa) atau daftar kosong pada data S-expression. Ia adalah nilai yang berbeda dari
  `false`.

## 2. Menulis tipe

Argumen dan nilai kembalian fungsi harus bertipe.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; a generic function
  (unwrap-or (first (iter v)) default))
```

- Definisi tanpa tipe, seperti `(defun f (x) x)`, tidak dapat ditulis.
- Variabel global seperti `defvar` juga memerlukan tipe: `(defvar (count int) 0)`.
- `the` bukan pemeriksaan saat dijalankan melainkan anotasi untuk pemeriksa tipe.
- **Tidak ada cara memeriksa tipe saat dijalankan.** Tidak ada `typep` atau `type-of`, karena tipe
  setiap nilai ditetapkan saat kompilasi. Untuk menerima salah satu dari beberapa tipe, buat sum type
  dengan `defenum` atau gunakan trait.
- `deftype` mendefinisikan alias tipe. Tipe yang menggambarkan rentang nilai, seperti
  `(deftype small () '(integer 0 9))`, tidak dapat dibuat.

Tipe bilangan bulat bawaan `int` memiliki presisi sembarang; seperti integer pada CL, tidak ada batas
atas ukurannya. Tipe berlebar tetap `i8` sampai `i32` dan `u8` sampai `u32` juga ada. Tidak ada tipe
bilangan bulat berlebar tetap 64 bit.

## 3. Fungsi sebagai nilai

typelisp tidak memisahkan ruang nama fungsi dan variabel. Nama sebuah fungsi dapat diserahkan sebagai
nilai apa adanya. Tidak ada `#'` dan tidak ada `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; call it directly, not with funcall

(apply-to twice 5)                        ; twice, not #'twice
```

- Fungsi bawaan seperti `+` dan `1+` juga dapat diserahkan sebagai nilai apa adanya, di tempat tipe
  argumennya sudah tetap, seperti pada `(fn (int) int)`. Saat menyerahkannya ke fungsi generik seperti
  `foldl` atau `map`, tidak diketahui `+` milik tipe mana yang dimaksud, sehingga bungkuslah dengan
  `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Fungsi sekuens menerima **koleksi lebih dulu dan fungsi kemudian**: `(map it f)`,
  `(filter it f)`, `(foldl it f init)`. Ini kebalikan dari `(mapcar f list)` pada CL.
- `lambda` tidak dapat memakai `&optional` atau `&key` (`&rest` dapat dipakai).
- **Fungsi tidak dapat dipanggil sebelum didefinisikan.** Di CL Anda dapat memanggil fungsi yang
  didefinisikan kemudian, tetapi di sini itu menghasilkan `no such function`. Untuk fungsi yang
  saling rekursif, deklarasikan salah satunya dengan `defsignature` lebih dulu.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Daftar dan Vector

Yang sepadan dengan list CL adalah **data S-expression**, yang tipenya `Option<Sexpr>` (daftar kosong
adalah `none`). `(list 1 2 3)` dan `'(a b c)` bertipe ini. Data S-expression adalah sesuatu yang
dikerjakan makro dan `read`; untuk wadah data biasa, gunakan **`Vector<T>`**.

| Yang Anda inginkan | CL | typelisp |
|---|---|---|
| Kepala dan sisa S-expression | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Menelusuri daftar S-expression | `(dolist (x xs) ...)` | Sama |
| Urutan elemen bertipe sama | List atau vector | `Vector<T>` |
| Sepasang nilai | `(cons a b)` | `(cons a b)` (tipenya `cons-cell<A,B>`) |
| Pemetaan | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` adalah pengakses pasangan `cons-cell<A,B>` yang dibuat dengan `cons`. Keduanya tidak
dapat dipakai pada daftar S-expression.

Membuat `Vector`:

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 1)
  (push v 2)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #<vector<int> 10 20>
```

Fungsi sekuens seperti `map`, `filter`, `sort`, dan `find` bekerja pada nilai yang mengimplementasikan
trait `Iter`. Serahkan `Vector` setelah mengubahnya menjadi iterator dengan `(iter v)`.

## 5. Tidak ada nilai ganda

Tidak ada `values` dan tidak ada `multiple-value-bind`. Fungsi yang di CL mengembalikan beberapa nilai
di sini mengembalikan pasangan atau struct.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → sebuah `cons-cell` dengan `car` 3 dan `cdr` 1 |
| `(decode-universal-time t)` → 9 nilai | Struct `decoded-time` |
| `(read-from-string s)` → nilai, posisi | `(read-from-string s)` mengembalikan `cons-cell` berisi nilai dan posisi di dalam `Result`. Jika hanya perlu nilainya, `(read s)` |

## 6. Tidak ada variabel khusus (pengikatan dinamis)

`let` selalu mengikat secara leksikal. Jika Anda mengikat ulang variabel yang didefinisikan dengan
`defvar` memakai `let`, fungsi yang dipanggil dari sana tetap melihat nilai aslinya.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 in CL, 1 in typelisp
```

Untuk mengubah variabel kendali seperti `*print-base*` secara sementara, gunakan `dlet`. Ia
menetapkan nilai dan memulihkan yang asli bagaimanapun badannya ditinggalkan.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` menulis ulang variabel global itu sendiri, sehingga ia bukan pengikatan per thread.

## 7. Sistem kondisi tidak diadopsi

Tidak ada `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error`, atau `signal`.
Semuanya kurang cocok dengan tipe statis. Sebagai gantinya, dua hal ini dipakai untuk tujuan yang
berbeda:

- **Kegagalan yang dapat dipulihkan mengembalikan `Result<T,E>`.** Pemanggil memisahkan `ok` / `err`
  dengan `match`. Tidak ada pintasan seperti `?` pada Rust.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Kegagalan yang tidak dapat dipulihkan (bug) adalah `panic`.** `(panic "message")`, memberikan
  `none` ke `unwrap`, pembagian dengan 0, dan indeks di luar rentang termasuk jenis ini, dan program
  berhenti. Pembersihan `unwind-protect` dijalankan sebelum berhenti.

Tipe kesalahan disatukan oleh trait `Error`, dan `(message e)` memberikan pesannya. Cara membuat tipe
kesalahan sendiri ada di
[Option, Result, dan Tipe Kesalahan](../reference/functions/option-result.md#3-tipe-kesalahan-dan-trait-error).
`assert` dan `warn` dapat dipakai seperti di CL.

`catch` / `throw` / `unwind-protect` ada. Namun, tag `catch` dibatasi pada simbol literal yang tidak
dievaluasi (`'done`), dan nilai yang dilempar dengan satu tag memiliki satu tipe.

## 8. Tidak ada CLOS

Tidak ada `defclass`, `defgeneric`, atau kombinasi metode.

- Tipe data didefinisikan dengan `defstruct` (struct) dan `defenum` (sum type).
- `defmethod` mendefinisikan metode yang targetnya ditentukan hanya oleh **tipe statis argumen
  pertama**. Tidak ada multiple dispatch.
- Untuk memberikan operasi yang sama pada banyak tipe, gunakan trait (`deftrait` / `impl`). Untuk
  nilai yang tipe konkretnya ditentukan saat dijalankan, gunakan tipe `:dyn Trait`
  ([Referensi Sintaks 3.9](../reference/syntax.md#39-deftrait--impl--trait)).

Perbedaan `defstruct`:

- Konstruktornya adalah `TypeName::new`: `(point::new 1 2)`. Jika Anda menginginkan nama seperti
  `make-point`, opsi `(:constructor make-point)` membuatnya.
- Selain `(x p)`, pengakses dapat ditulis `p::x`. Ubah dengan `(setf p::x 5)`.
- Tidak ada predikat (`point-p`) yang dibuat. Tidak ada `:conc-name`, `:type`, atau `:named`.
- `:include` hanya mewarisi slot; tipenya tidak menjadi subtipe dari induk.

## 9. Modul sebagai ganti paket

Tidak ada paket. Ruang nama adalah modul, dan sebuah berkas adalah modul tersendiri. Sebagai ganti
`pkg:symbol`, tulis `module::name`, dan bawa nama masuk dengan `use`
([Modul dan Tata Letak Berkas](modules.md)).

Keyword `:foo` ada dan merupakan simbol yang dievaluasi menjadi dirinya sendiri. Karena tidak ada
paket, titik dua adalah bagian dari nama: `(symbol->string :foo)` mengembalikan `":foo"`.

## 10. Perbedaan dalam pembacaan dan sintaks

- Huruf besar dan kecil tidak dibedakan (simbol menjadi huruf kecil saat dibaca). Ini sama dengan CL.
- Tidak ada `#'` (bagian 3). Literal bilangan kompleks `#c(...)` tidak dapat dibaca; buat bilangan
  kompleks dengan `(complex 1.0 2.0)`.
- Klausa `loop` yang diperluas ditulis dengan keyword: `(loop :for i :from 1 :to 3 :collect i)`.
  `loop` yang tidak diawali keyword adalah perulangan tak hingga sederhana, yang ditinggalkan dengan
  `(break)` atau `(return value)`. `return` meninggalkan perulangan terdalam (untuk meninggalkan
  fungsi, gunakan `return-from`).
- Tujuan `format` adalah `false` (mengembalikan string), `true` (keluaran standar), atau sebuah
  stream. Direktif format sama seperti di CL.
- Membaca dari string adalah `(read "...")`, dan membaca dari stream adalah `(read-sexpr s)`. Keduanya
  mengembalikan `Result`.
- `eval` memeriksa tipe ekspresi yang diberikan sebelum mengevaluasinya, dan mengembalikan `Result`.
  Rujukan maju tidak dimungkinkan, sama seperti pada kode sumber.
- Tidak ada `eval-when`.
- Nama fungsi tidak memakai akhiran `?` atau `!`. Predikat dinamai dengan `-p` / `p` seperti di CL
  (`zerop`, `sexpr-null`), atau dengan `is-` di depannya (`is-some`).

## 11. Fungsi utama dengan nama berbeda

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (dari stream) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Versi dua argumen dari `floor` dan sejenisnya | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (urutan argumen dibalik; bagian 4) |
| `length` (pada vector) | `len` |
| `hash-table-count` | `count` / `size` |

Daftar fungsi ada di [Fungsi Bawaan](../reference/functions/README.md).

## 12. Hal lain yang tidak ada

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` dan `copy-readtable`, `readtable-case` (reader macro itu sendiri dapat didefinisikan
  dengan `set-macro-character`)
- Pathname logis dan pathname wildcard
- `input-stream-p` / `output-stream-p` (arah sebuah stream ditentukan oleh tipenya)
