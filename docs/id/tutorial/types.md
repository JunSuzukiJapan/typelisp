<!-- translated-from: docs/ja/tutorial/types.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Dasar-dasar Tipe

typelisp adalah bahasa dengan tipe statis. Bab ini menjelaskan apa yang dilakukan pemeriksa tipe
untuk Anda, tipe yang paling sering dipakai (`Option`, `Result`, struct, dan enum), serta generik.
Bab ini mengasumsikan Anda sudah membaca [Memulai](intro.md).

## 1. Arti tipe statis

Di typelisp, tipe setiap ekspresi ditetapkan sebelum program dijalankan. Ekspresi yang tipenya tidak
cocok adalah kesalahan sebelum apa pun dijalankan.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; type error

(main)
```

Menjalankan berkas ini berhenti dengan kesalahan tipe bahkan tanpa mencetak `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Anda perlu menulis tipe untuk argumen dan nilai kembalian fungsi, variabel global, dan field struct.
Tipe variabel `let` diambil dari nilai awalnya.

Tipe utama:

| Tipe | Contoh nilai |
|---|---|
| `int` | `42`, `-7` (bilangan bulat presisi sembarang) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Bilangan bulat berlebar tetap |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Tipe kembalian fungsi yang tidak mengembalikan nilai |

Tidak ada cara menanyakan tipe suatu nilai saat dijalankan (tidak ada `typep` atau `type-of` seperti
pada Common Lisp), karena setiap tipe sudah diketahui sebelum program dijalankan.

## 2. `Option<T>`: nilai yang mungkin tidak ada

typelisp tidak punya `nil`. "Mungkin tidak ada nilai" dinyatakan dengan tipe `Option<T>`. Nilai
`Option<T>` adalah `some`, yang menyimpan satu nilai `T`, atau `none`, yang tidak menyimpan apa-apa.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` bukan `int`, sehingga tidak dapat dipakai dalam aritmetika apa adanya.
`(+ (safe-div 10 2) 1)` adalah kesalahan tipe. Untuk memakai isinya, pisahkan `some` dari `none`
dengan `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- Pada cabang `(some q)`, isinya diikat ke variabel `q`.
- `match` memeriksa bahwa cabang-cabangnya **mencakup setiap kasus**. Melupakan cabang `(none)`
  adalah kesalahan tipe.

### Mengapa tidak ada nil

Di banyak bahasa, `nil` (`null`) dapat menggantikan nilai bertipe apa pun. Akibatnya, lupa menangani
kasus "tidak ada nilai" tidak diketahui sampai program dijalankan. Di typelisp, tempat yang nilainya
mungkin tidak ada bertipe `Option<T>`, dan kode tidak lolos pemeriksa tipe kecuali `match`
menangani kasus `none`. Kasus yang terlupa ditemukan sebelum program dijalankan.

Kondisi mengikuti gagasan yang sama: hanya `bool` yang dapat menjadi kondisi `if`. Tidak ada aturan
seperti "semua selain `nil` adalah benar" pada Common Lisp.

### Operasi umum

| Bentuk | Arti |
|---|---|
| `(unwrap-or opt default)` | Isinya untuk `some`; nilai bawaan untuk `none` |
| `(unwrap opt)` | Mengambil isinya. Menghentikan program pada `none` |
| `(is-some opt)` / `(is-none opt)` | Menguji yang mana |

Banyak fungsi pustaka standar mengembalikan `Option`. Misalnya, `position` mengembalikan posisi di
dalam `some` jika elemen ditemukan dan `none` jika tidak.

```lisp
(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: operasi yang mungkin gagal

Operasi yang dapat gagal mengembalikan `Result<T,E>`: `ok` yang menyimpan nilai `T` jika berhasil,
atau `err` yang menyimpan kesalahan `E` jika gagal.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Fungsi Anda sendiri juga dapat mengembalikan `Result`.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Gunakan `Option` ketika ketiadaan nilai tidak perlu dijelaskan, dan `Result` ketika Anda ingin
menyatakan mengapa sesuatu gagal. [Penanganan Kesalahan](errors.md) membahas penanganan kesalahan
secara rinci.

## 4. `defstruct`: struct

Tipe dengan field bernama didefinisikan dengan `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

Definisi itu memberi Anda hal berikut:

```lisp
(let ((p (point::new 3 4)))     ; create one (arguments in field order)
  (println "~a" p::x)           ; read a field; (x p) also works
  (setf p::x 10)                ; change it
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Untuk memberi struct fungsi miliknya sendiri, gunakan `defmethod`. Tipe argumen pertama (`self`)
menentukan tipe mana yang memiliki metode itu.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Menulis hanya nama tipe sebagai ganti argumen `self` membuat fungsi yang dipanggil sebagai
`point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: salah satu dari beberapa bentuk

Nilai yang merupakan salah satu dari beberapa bentuk, seperti "lingkaran, persegi panjang, atau
titik", didefinisikan dengan `defenum`. Setiap bentuk disebut **varian**. Setiap varian dapat
menyimpan jumlah dan tipe nilai yang berbeda.

```lisp
(defenum shape
  (circle int)        ; radius
  (rect int int)      ; width and height
  (dot))              ; holds no value
```

Nilai dibuat dengan nama tipe di depan, seperti `shape::circle`. Pada `match`, nilai dibongkar
berdasarkan nama varian.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Di sini pun, `match` memeriksa bahwa setiap kasus tercakup. Jika kemudian Anda menambahkan varian ke
`shape`, setiap `match` yang tidak menanganinya menjadi kesalahan tipe, sehingga tidak ada tempat
yang perlu diperbaiki yang terlewat.

Setelah `(use shape)`, Anda dapat menulis `(rect 5 6)` tanpa nama tipe.

`Option` dan `Result` adalah enum yang dibangun dengan mekanisme yang sama.

## 6. Generik

Fungsi yang bekerja untuk tipe apa pun didefinisikan dengan **parameter tipe** `<T>` setelah
namanya.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Anda tidak memberikan tipe saat memanggilnya. `T` ditentukan dari argumen.

```lisp
(first-or ints 7)          ; T is int
(first-or names "none")    ; T is string
(first-or ints "none")     ; type error: ints is a Vector<int>, so T is int
```

Struct dan enum juga dapat generik. `Vector<T>`, `Option<T>`, dan `Result<T,E>` adalah tipe jenis
ini.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Di dalam fungsi generik tidak ada yang diketahui tentang `T`, sehingga Anda tidak dapat
membandingkan atau menjumlahkan nilai `T`. Untuk mensyaratkan sesuatu seperti "tipe apa pun yang
dapat dibandingkan", gunakan trait ([Trait](traits.md)).

## 7. Memberi nama lain pada tipe

`deftype` memberi nama lain pada sebuah tipe.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` hanyalah ejaan lain dari `int`, bukan tipe baru. Memberikan `int` biasa di tempat `meters`
diharapkan bukan kesalahan. Jika Anda ingin keduanya dipisahkan, buat struct, seperti
`(defstruct meters (value int))`.

## 8. Yang dibaca selanjutnya

- [Trait](traits.md): memberi tipe operasi yang sama
- [Tipe](../reference/types.md): tipe bawaan dan trait yang diimplementasikan masing-masing
