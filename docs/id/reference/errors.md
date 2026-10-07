<!-- translated-from: docs/ja/reference/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Pesan Kesalahan

Arti pesan kesalahan utama dari `typl` dan cara memperbaikinya.

## 1. Membaca kesalahan

Kesalahan ditulis ke keluaran kesalahan standar dalam bentuk ini:

```text
error: file:line:column: kind: message
```

`kind` memberi tahu kapan kesalahan ditemukan.

| Jenis | Kapan | Arti |
|---|---|---|
| `type error` | Sebelum dijalankan (saat pemeriksaan) | Kesalahan pada tipe atau nama. Bentuk itu tidak dijalankan |
| (tanpa jenis) | Saat membaca atau memeriksa | Kesalahan sintaks seperti tanda kurung tidak seimbang, atau nama yang tidak ditemukan |
| `panic` | Saat dijalankan | Kegagalan yang tidak dapat dipulihkan. Program berhenti setelah menjalankan pembersihan `unwind-protect` |

Baris yang diawali `warning:` adalah peringatan, dan pemrosesan terus berlanjut.

`file:line:column` menunjuk ekspresi yang salah. Untuk kesalahan saat dijalankan yang terjadi di
dalam fungsi pustaka standar, ia menunjuk tempat program memanggil fungsi itu. Sebagian kesalahan
tidak memiliki posisi (seperti `error: panic: ...`).

Contoh:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Ini berarti ekspresi di baris 1, kolom 24 pada `main.typl` adalah `string` padahal yang diharapkan
`i32`.

## 2. Kesalahan saat pemeriksaan

Kesalahan yang ditemukan sebelum dijalankan. Bentuk itu tidak dijalankan sampai diperbaiki.

### 2.1 Tipe

| Pesan | Arti dan perbaikan |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Ekspresi bertipe `U` berada di tempat yang membutuhkan tipe `T`. Tidak ada konversi implisit; untuk bilangan, konversikan dengan `(as T x)`. `int` dan `i32` juga merupakan tipe yang berbeda |
| ``integer literal 300 is out of range for u8 (0..=255)`` | Literal tidak muat dalam tipenya. Jika Anda ingin memotongnya, tulis `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Tidak ada tipe dengan nama itu. Definisikan tipe sebelum bentuk pertama yang memakainya (tipe tidak punya deklarasi maju). Jika yang Anda maksud variabel tipe, tulis di posisi pendeklarasian seperti `<foo>` setelah nama fungsi ([Referensi Sintaks 3.6](syntax.md#36-defstruct--struct-tipe-buatan-pengguna)) |
| ``cannot infer type argument `t` for `vector::new` `` | Argumen tipe tidak dapat ditentukan. Tulis tipenya dengan `the`, seperti `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` tidak menangani setiap varian. Tambahkan cabang untuk varian yang kurang, atau cabang `_` |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | Fungsi mensyaratkan trait yang tidak diimplementasikan oleh tipe yang Anda berikan. Tulis `(impl Eq pt ...)` ([Trait Standar](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Nilai bertipe yang tidak mengimplementasikan trait diberikan di tempat yang mengharapkan `:dyn`. Tulis `impl`-nya |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Nama trait ditulis di tempat tipe. Tulis `:dyn Error` |
| ``if: (if cond then else)`` | `if` memiliki bentuk yang salah. `if` mensyaratkan cabang else. Jika tidak memerlukannya, gunakan `when` |

### 2.2 Nama

| Pesan | Arti dan perbaikan |
|---|---|
| `no such function: bar` | Tidak ada fungsi atau metode dengan nama itu. Periksa ejaannya |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Metode dipilih berdasarkan tipe argumen pertama. Metode bernama itu ada, tetapi tidak untuk tipe argumen pertama (di sini `int`). Akhir pesan mendaftar tipe yang memiliki metode itu |
| `unbound variable: y` | Tidak ada variabel dengan nama itu. Periksa ejaan dan lingkup pengikatannya (apakah dipakai di luar `let`-nya?) |
| ``use: unresolved `nosuch` `` | Modul yang disebut pada `use` tidak ditemukan. Untuk cara nama berkas dipetakan ke path modul, lihat [Referensi Sintaks 3.11](syntax.md#311-berkas-dan-modul-proyek-multiberkas) |
| `unresolved path: c::hidden` | Modulnya ada, tetapi namanya tidak, atau tidak terlihat karena tidak memiliki `pub` |
| `circular module dependency: a -> b -> a` | Modul saling meng-`use`. Pindahkan bagian bersama ke modul terpisah |
| ``return-from: no enclosing block named `nope` `` | Tidak ada `block` bernama itu yang melingkupi `return-from`. Block milik fungsi hanya dapat dipakai di dalam fungsi itu |

### 2.3 Pemanggilan

| Pesan | Arti dan perbaikan |
|---|---|
| `f: expected 1 argument(s), got 2` | Jumlah argumen tidak sesuai |
| `f: unknown keyword argument :b` | Argumen keyword yang tidak dimiliki fungsi diberikan |
| `new: expected 1 field(s), got 2` | Jumlah nilai yang diberikan ke konstruktor struct tidak sesuai dengan jumlah field |
| ``setf: cannot assign to constant `k` `` | Nama yang didefinisikan dengan `defconstant` ditugasi. Jika perlu berubah, gunakan `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Fungsi yang dideklarasikan dengan `defsignature` tidak didefinisikan |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | Tidak ada tipe argumen yang memiliki metode yang dipanggil dengan `~/name/` ([Direktif Format bab 5](functions/format.md#5-name)) |

## 3. Kesalahan pembacaan

| Pesan | Arti dan perbaikan |
|---|---|
| `unexpected end of input while reading a list` | Tanda kurung tutup kurang. Posisinya menunjuk tempat pembacaan berakhir (seperti akhir berkas), jadi carilah tanda kurung buka |

## 4. Kesalahan saat dijalankan (panic)

| Pesan | Arti dan perbaikan |
|---|---|
| `panic: divide by zero` | Pembagian dengan nol pada bilangan bulat atau rasio. Pembagian floating-point dengan nol tidak melakukan panic; hasilnya `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` diterapkan pada `none`. Tangani kasus `none` dengan `match` atau `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Indeks di luar rentang. Periksa panjang dengan `len`, atau gunakan fungsi yang mengembalikan `none` saat di luar rentang (`nth`, `pop`, dan sebagainya) |
| `panic: an integer argument does not fit a fixnum` | `int` yang tidak muat dalam 63 bit diberikan ke argumen yang menerima indeks atau jumlah |
| `throw: no enclosing (catch 'oops) for this throw` | `throw` dijalankan tanpa `catch` bertag sama yang melingkupinya |
| `panic: <message>` | Program memanggil `(panic "<message>")`. `assert` yang gagal menghasilkan `assertion failed: ...` |

`panic` menghentikan seluruh proses bahkan ketika terjadi di dalam task
([Referensi Sintaks 12.4](syntax.md#124-interaksi-dengan-fitur-lain)). Nyatakan kegagalan yang ingin
Anda pulihkan dengan `Result` ([Referensi Sintaks bab 9](syntax.md#9-kebijakan-penanganan-kesalahan)).

## 5. Peringatan

| Pesan | Arti |
|---|---|
| ``warning: redefining function `f` `` | Fungsi bernama sama didefinisikan lagi. Definisi yang belakangan berlaku. Ini muncul secara normal ketika Anda memperbaiki definisi di REPL |
