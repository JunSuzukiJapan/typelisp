<!-- translated-from: docs/ja/reference/functions/option-result.md @ dff3b51792f6d9c49bb8802bd3d3fb6e168edd92 -->
# Option, Result, dan Tipe Kesalahan

## 1. `Option<T>` / `Result<T,E>`

Konstruktor: `Option<T>` memiliki `(Some T)` / `None`. `Result<T,E>` memiliki `(Ok T)` / `(Err E)`.
`E` dapat berupa tipe apa pun: tipe kesalahan konkret bawaan, dan tipe yang Anda tulis sendiri
dengan `defstruct`/`defenum`, cocok di sana sama saja (bab 3).

| Nama | Bentuk | Option | Result | Deskripsi |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Mengambil nilainya. Panic pada `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | Nilainya, atau nilai bawaan |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Apakah `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Apakah `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Apakah `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Apakah `Err` |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | Mengambil nilainya. Pada `None`/`Err` panic dengan `msg` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | Nilainya, atau hasil `f`. `f` hanya dipanggil pada `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | Menerapkan `f` pada isi `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | Menerapkan `f` pada isi `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | Pada `Some`/`Ok`, meneruskan isinya ke `f` dan mengembalikan hasilnya |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | Pada `None`/`Err`, mengembalikan hasil `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | Mengubah `(Some v)` menjadi `(Ok v)` dan `None` menjadi `(Err e)` |

Konstruktor `Some`/`None`/`Ok`/`Err` dapat dipakai sebagai nama polos di modul mana pun tanpa `use`.
Konstruktor ini juga dapat ditulis dengan nama tipenya, seperti `Option::Some`.

Percabangan ditulis secara eksplisit dengan `match`, atau dirangkai dengan `map`/`and-then` dan yang
lain di atas. Tidak ada sintaks yang setara dengan `?` milik Rust.

`map` milik `Option`/`Result` adalah metode, terpisah dari `map` untuk [sekuens](sequences.md).
Metode inilah yang dipanggil ketika tipe argumen pertama adalah `Option`/`Result`.

Makro `->` meneruskan sebuah nilai sebagai argumen pertama setiap bentuk berikutnya secara berurutan
(sama seperti `->` di Clojure). `(-> x (f a) (g b))` menjadi `(g (f x a) b)`. Nama tanpa tanda
kurung, `h`, diperlakukan sebagai `(h x)`. Argumen pertama metode adalah penerimanya, jadi
kombinator dapat dirangkai apa adanya:

```lisp
(use std::num::ParseIntError std::num::parse-int)
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (Option::Some (truncate n 2)) (Option::None)))

(-> (Option::Some 8)
    (and-then half)                          ; (Some 4)
    (and-then half)                          ; (Some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (Some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. Representasi `Option<T>` saat dijalankan

Seperti di Rust, **`Option<T>` biasanya tidak membuat kotak**. `Some v` adalah `v` itu sendiri dan
`None` adalah nilai daftar kosong, tanpa alokasi dan tanpa tipuan. `Option<Sexpr>` (tempat daftar
kosong adalah `None`), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>`, dan
`Option<(fn ...)>` semuanya berbentuk ini.

Kotak dipakai hanya ketika nilai `T` tidak dapat dibedakan dari nilai daftar kosong:

| `T` | Representasi | Alasan |
|---|---|---|
| `Option<U>` (bersarang) | Kotak | `None` bagian dalam akan menjadi nilai yang sama dengan `None` bagian luar |
| `()` | Kotak | Nilai `()` adalah nilai daftar kosong itu sendiri |
| `ptr` / `c-long` / `c-ulong` | Kotak | Seluruh bit adalah nilai, tidak menyisakan ruang untuk membedakan |
| Lainnya | Tanpa kotak | — |

Representasi ditentukan oleh tipe saja dan tidak dapat dibaca dari nilai. Saat mencetak,
`(Some ...)`/`None` direkonstruksi dari tipe statis, sehingga `(format false "~a" opt)` mencetak
`(Some 1)`. Ada dua batasan:

- **Ia tidak dapat dimasukkan ke dalam `:dyn Trait`** (memberikan nilai `Option<int>` yang untuknya
  Anda menulis `(impl Speak Option<int> ...)` ke `:dyn Speak` adalah kesalahan).
- Downcast `(the Option<T> ...)` dari `Sexpr` **menyebut konstruktor**:
  `(the Option<int> (Some x))` / `(the Option<int> (None))`. Bentuk yang mengikat seluruh nilai,
  `(the Option<int> o)`, adalah kesalahan.

## 3. Tipe kesalahan dan trait `Error`

Mengikuti `std::error::Error` pada Rust, **`Error` bukan tipe melainkan trait**. Tipe konkret yang
mewakili kesalahan terpisah untuk setiap tujuan, dan masing-masing mengimplementasikan `Error`.

| Tipe | Dihasilkan oleh |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operasi berkas dan stream ([Stream dan Berkas](streams-files.md)) |
| `NetError` | Operasi jaringan ([Jaringan](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. `simple-error` pada CL: pilihan bawaan ketika Anda hanya ingin menyatakan apa yang terjadi |
| `WrappedError` | `(wrap-error msg cause)`. Tipe yang membawa pesan Anda sendiri dan penyebabnya; inilah alasan trait `Error` memiliki `source` |

`ParseIntError` sampai `NetError` masing-masing adalah "enum dengan satu varian yang menyimpan satu
string pesan", dan nama tipe dan nama varian sama (`(match e ((ParseIntError m) m))`, dibangun dengan
`(ParseIntError::ParseIntError "...")`). Tidak ada yang istimewa pada tipe-tipe itu: semuanya
diperlakukan persis seperti tipe kesalahan Anda sendiri yang ditulis dengan
`(defstruct my-err (...))` / `(defenum my-err ...)`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Pesan kesalahan (metode trait `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | Penyebab yang dibungkus kesalahan ini, atau `None` jika tidak ada (`Error::source` pada Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` mengimplementasikan `Error`) | Memperluas tipe kesalahan konkret menjadi objek trait |
| `describe-error` | `(describe-error e)` | `E→string` (`E` mengimplementasikan `Error`) | Pesan dan rantai penyebab yang ditemukan dengan mengikuti `source`, satu penyebab per baris. CL tidak memiliki padanannya ("caused by" pada Rust) |

Jika Anda mengimplementasikan `Error` untuk tipe kesalahan Anda sendiri, tipe itu dapat ditangani
**dengan cara yang sama** seperti kesalahan bawaan:

```lisp
(use std::num::ParseIntError)
(defstruct io-err (path string))
(impl Error io-err
  (defmethod message (self) string (append "io failed: " self::path))
  (defmethod source (self) Option<:dyn Error> (Option::None)))

(defun open-it ((p string)) Result<i32, io-err>          ; the concrete type goes into E as it is
  (if (equal p "") (Result::Err (make-instance 'io-err :path "<empty>")) (Result::Ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; handle any kind uniformly
(describe (make-instance 'io-err :path "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Untuk mengumpulkan beberapa tipe kesalahan ke dalam satu `Result`, gunakan `Result<T, :dyn Error>`
(sepadan dengan `Box<dyn Error>` pada Rust), dan perluas kesalahan konkret dengan `as-dyn-error`.
Karena tidak ada `?`, konversi ini ditulis secara eksplisit:

```lisp
(use std::error::as-dyn-error std::num::parse-int)
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((Ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((Err e) (Result::Err e))))
```

**Tipe dan trait berbagi satu ruang nama** (seperti di Rust). Dalam satu modul, `defstruct`/`defenum`
dan trait tidak dapat bernama sama, dan menulis nama trait pada posisi tipe dilaporkan sebagai
"`error` is a trait, not a type — write `:dyn error`".

Kegagalan yang tidak dapat dipulihkan dinyatakan dengan `panic`. Untuk `panic` dan
`catch`/`throw`, lihat [Referensi Sintaks](../syntax.md#8-keluar-non-lokal-catch--throw--unwind-protect);
untuk kebijakan penanganan kesalahan, lihat [bab 9 pada dokumen yang sama](../syntax.md#9-kebijakan-penanganan-kesalahan).
