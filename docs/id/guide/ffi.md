<!-- translated-from: docs/ja/guide/ffi.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# FFI C (defffi)

Panduan ini menjelaskan cara memanggil fungsi C dari typelisp. Daftar tipe yang dapat dideklarasikan
dan batasannya ada di [Referensi Sintaks 3.3](../reference/syntax.md#33-defffi--deklarasi-fungsi-c-ffi).

## 1. Mendeklarasikan dan memanggil fungsi

`defffi` mendeklarasikan nama dan tipe sebuah fungsi C.

```lisp
(defffi (c-getpid "getpid") () i32)            ; the typelisp name and the C symbol name
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; look it up in libm
```

Pemanggilan dibungkus dengan `(unsafe ...)`.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` diperlukan karena kompiler tidak dapat memeriksa bahwa tipe yang dideklarasikan sesuai
dengan tipe sebenarnya di sisi C. Menulis `unsafe` berarti Anda, sebagai penulis, bertanggung jawab
atas pemeriksaan itu. Jika terlupa, akan muncul kesalahan yang menjelaskan hal ini.

## 2. Menulis pembungkus yang aman

Penggunaan yang dimaksudkan adalah mengurung `unsafe` di satu tempat dan menyajikan fungsi biasa ke
luar.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; the caller needs no unsafe
(str-len "hello")  ; => 5
```

## 3. Padanan tipe

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Bilangan bulat dengan lebar yang sama |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (juga `size_t`, `int64_t`, dan sebagainya) |
| `ptr` | Pointer apa pun (`void *`, `FILE *`, dan sebagainya) |
| `(ptr T)` | Pointer ke `T` ([bagian 7](#7-struct-c)) |

### String

- `string` yang Anda berikan disalin menjadi string C berakhiran NUL, yang dibebaskan setelah
  pemanggilan kembali. NUL di tengah string adalah kesalahan.
- Hasil fungsi yang mengembalikan `string` juga disalin. Memori di sisi C tidak dibebaskan. Untuk
  fungsi yang mengembalikan string yang harus dibebaskan oleh pemanggil (seperti `strdup`), ambil
  hasilnya sebagai `ptr` dan `free` sendiri.
- Jika fungsi yang dideklarasikan mengembalikan `string` ternyata mengembalikan NULL, itu adalah
  kesalahan. Ambil hasil fungsi yang mungkin mengembalikan NULL (seperti `getenv`) sebagai `ptr`.

### `c-long` / `c-ulong` / `ptr`

Tipe-tipe ini ada hanya untuk menyeberangkan nilai melewati batas dengan C, dan **tidak mendukung
aritmetika**. Untuk memakainya sebagai bilangan bulat typelisp, konversikan dengan `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int does not lose any of the 64-bit value
(try-as i32 (unsafe (c-strlen s)))  ; none if it does not fit in an i32
(unsafe (c-malloc 16))              ; integer literals can be passed as they are
```

`ptr` adalah nilai yang diserahkan kembali ke fungsi C. Tidak ada cara membaca apa yang
ditunjuknya dari sisi typelisp.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Tipe-tipe ini hanya dapat muncul sebagai argumen fungsi, nilai kembalian, dan variabel lokal.
Tipe-tipe ini tidak dapat menjadi field struct, variabel global, atau argumen tipe untuk `Vector`
dan sejenisnya.

## 4. Menyebut pustaka

Tanpa `:library`, simbol dicari pada apa yang sudah tertaut ke dalam proses (libc dan sebagainya).
Fungsi dari pustaka lain memerlukan `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Nama pendek seperti `"sqlite3"` dicari sebagai `libsqlite3.dylib`, lalu `libsqlite3.so`.
- Nama yang mengandung `/` diperlakukan sebagai path.
- Jika simbol yang dideklarasikan tidak ditemukan, kesalahan menyebut namanya.

## 5. Kompilasi AOT

Program yang memakai `defffi` dapat dijadikan berkas eksekusi dengan
[`compile-file`](compile.md#3-membangun-berkas-eksekusi-dengan-kompilasi-aot) apa adanya. Pustaka
yang disebut dengan `:library` ditambahkan secara otomatis saat penautan, sehingga `compile-file`
tidak memerlukan argumen tambahan.

## 6. Callback

Anda dapat menyerahkan fungsi typelisp ke fungsi C dan membiarkannya dipanggil balik. Tulis tipe
fungsi di antara tipe argumen pada `defffi`, dan pada pemanggilan taruh nama fungsi atau ekspresi
`lambda` di posisi itu.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") returns p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Hanya fungsi **tanpa variabel bebas** yang dapat diserahkan. Fungsi tingkat atas, `lambda`, dan
  fungsi `labels` lokal semuanya dapat, tetapi merujuk variabel lokal dari lingkup yang melingkupi
  adalah kesalahan pada saat pemeriksaan tipe. C hanya meneruskan argumen yang dideklarasikan,
  sehingga tidak ada cara menyampaikan variabel yang ditangkap. Untuk menyimpan keadaan, gunakan
  variabel global.
- Variabel yang menyimpan fungsi tidak dapat diserahkan. Tulis nama fungsi atau ekspresi `lambda`
  langsung di tempatnya.
- `panic` atau `throw` di dalam callback sampai ke pemanggil setelah fungsi C kembali.
- Callback hanya dapat dipanggil selama fungsi C yang dipanggil typelisp sedang berjalan. Callback
  tidak dapat dipakai dari hal seperti `atexit` atau penangan sinyal.

## 7. Struct C

Untuk menyerahkan sesuatu seperti larik struct ke fungsi C, deklarasikan struct dengan tata letak
yang sama seperti di C memakai `def-c-struct`, dan alokasikan di dalam `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; declared inside a top-level unsafe

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; four items, all zero
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` mengalokasikan `n` nilai `T` dan mengembalikan `(ptr T)`. `(c-ref p i)` adalah
  pointer ke elemen ke-`i`, `p::field` adalah sebuah field, dan `(c-deref p)` adalah apa yang
  ditunjuk pointer ke skalar seperti `i32`. Semuanya dapat ditulis dengan `setf`.
- `(as ptr p)` mengubahnya menjadi `ptr` tak bertipe untuk diserahkan ke fungsi C yang menerima
  `void *`.
- Ukuran `item` (di sini 8) dan posisi tiap field ditentukan oleh aturan yang sama seperti di C.

### Masa hidup memori yang dialokasikan

Memori yang dialokasikan dibebaskan ketika kendali meninggalkan `unsafe` terluar di fungsi itu.
Hal yang sama terjadi ketika ditinggalkan lewat `panic` atau `throw`. Karena itu, nilai `(ptr T)`
tidak dapat dibawa keluar dari `unsafe`. Menjadikannya nilai dari `unsafe`, menangkapnya dalam
closure, menyerahkannya ke `task`, dan melemparnya dengan `throw` semuanya adalah kesalahan tipe.
Salin nilai yang ingin Anda pakai di luar ke bilangan atau `defstruct` di dalam `unsafe`.

Saat mengalokasikan di dalam `lambda` atau fungsi `labels`, tulis `unsafe` di dalam fungsi itu.

### Memori yang dialokasikan oleh C

Pointer yang diterima dari C sebagai `(ptr T)` (nilai kembalian `defffi`, argumen callback, dan
sebagainya) adalah kesalahan kecuali menunjuk ke dalam memori yang dialokasikan dengan `c-alloc`.
Deklarasikan fungsi yang menerima memori yang dialokasikan C dengan `malloc`, atau NULL, memakai
`ptr` tak bertipe.

## 8. Yang tidak dapat dilakukan

- **Fungsi variadik** (`printf` dan sejenisnya) tidak dapat dideklarasikan. Bagian variadik
  diteruskan dengan aturan yang berbeda dari argumen tetap. Deklarasikan nama terpisah untuk setiap
  jumlah argumen yang Anda pakai.
- **Menyerahkan atau mengembalikan struct secara nilai** tidak dimungkinkan. Gunakan fungsi yang
  meneruskan pointer.
- **Deklarasi generik** tidak dimungkinkan.
- **Nama yang sama dengan fungsi bawaan** tidak dapat dipakai.
- **Tidak dapat diserahkan sebagai nilai fungsi.** Anda tidak dapat menyerahkannya seperti pada
  `(map xs c-abs)`; bungkuslah dengan `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
