<!-- translated-from: docs/ja/reference/functions/option-result.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Option, Result, dan Tipe Kesalahan

## 1. `Option<T>` / `Result<T,E>`

Konstruktor: `Option<T>` memiliki `Some(T)` / `None`. `Result<T,E>` memiliki `Ok(T)` / `Err(E)`.
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

Konstruktornya adalah `Option::some`/`Option::none`/`Result::ok`/`Result::err` (atau, setelah
`(use option)`/`(use result)`, nama polos `some`/`none`/`ok`/`err`).

Percabangan ditulis secara eksplisit dengan `match`. Tidak ada sintaks yang sepadan dengan `?` pada
Rust.

## 2. Representasi `Option<T>` saat dijalankan

Seperti di Rust, **`Option<T>` biasanya tidak membuat kotak**. `some v` adalah `v` itu sendiri dan
`none` adalah nilai daftar kosong, tanpa alokasi dan tanpa tipuan. `Option<Sexpr>` (tempat daftar
kosong adalah `none`), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>`, dan
`Option<(fn ...)>` semuanya berbentuk ini.

Kotak dipakai hanya ketika nilai `T` tidak dapat dibedakan dari nilai daftar kosong:

| `T` | Representasi | Alasan |
|---|---|---|
| `Option<U>` (bersarang) | Kotak | `none` bagian dalam akan menjadi nilai yang sama dengan `none` bagian luar |
| `()` | Kotak | Nilai `()` adalah nilai daftar kosong itu sendiri |
| `ptr` / `c-long` / `c-ulong` | Kotak | Seluruh 64 bit adalah nilai, tidak menyisakan ruang untuk membedakan |
| Lainnya | Tanpa kotak | — |

Representasi ditentukan oleh tipe saja dan tidak dapat dibaca dari nilai. Saat mencetak,
`(some ...)`/`none` direkonstruksi dari tipe statis, sehingga `(format false "~a" opt)` mencetak
`(some 1)`. Ada dua batasan:

- **Ia tidak dapat dimasukkan ke dalam `:dyn Trait`** (memberikan nilai `Option<int>` yang untuknya
  Anda menulis `(impl Speak Option<int> ...)` ke `:dyn Speak` adalah kesalahan).
- Downcast `(the Option<T> ...)` dari `Sexpr` **menyebut konstruktor**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. Bentuk yang mengikat seluruh nilai,
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
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; the concrete type goes into E as it is
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; handle any kind uniformly
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Untuk mengumpulkan beberapa tipe kesalahan ke dalam satu `Result`, gunakan `Result<T, :dyn Error>`
(sepadan dengan `Box<dyn Error>` pada Rust), dan perluas kesalahan konkret dengan `as-dyn-error`.
Karena tidak ada `?`, konversi ini ditulis secara eksplisit:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Tipe dan trait berbagi satu ruang nama** (seperti di Rust). Dalam satu modul, `defstruct`/`defenum`
dan trait tidak dapat bernama sama, dan menulis nama trait pada posisi tipe dilaporkan sebagai
"`error` is a trait, not a type — write `:dyn error`".

Kegagalan yang tidak dapat dipulihkan dinyatakan dengan `panic`. Untuk `panic` dan
`catch`/`throw`, lihat [Referensi Sintaks](../syntax.md#8-keluar-non-lokal-catch--throw--unwind-protect);
untuk kebijakan penanganan kesalahan, lihat [bab 9 pada dokumen yang sama](../syntax.md#9-kebijakan-penanganan-kesalahan).
