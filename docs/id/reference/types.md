<!-- translated-from: docs/ja/reference/types.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Tipe

Tipe yang dimiliki typelisp, dan trait standar yang diimplementasikan setiap tipe. Cara menulis tipe
ada di [Referensi Sintaks bab 2](syntax.md#2-menulis-tipe); fungsi dan metode tiap tipe ada di
[Fungsi Bawaan](functions/README.md).

## 1. Tipe primitif

| Tipe | Isi | Rincian |
|---|---|---|
| `int` | Bilangan bulat presisi sembarang. Disimpan sebagai nilai langsung selama muat dalam 63 bit, dan otomatis menjadi bignum jika melebihi itu. Tipe bawaan literal bilangan bulat tanpa anotasi | [Bilangan bab 3](functions/numbers.md#3-bilangan-bulat-presisi-sembarang-int) |
| `i8` `i16` `i32` | Bilangan bulat bertanda berlebar tetap | [Bilangan bab 1](functions/numbers.md#1-bilangan-bulat-berlebar-tetap) |
| `u8` `u16` `u32` | Bilangan bulat tak bertanda berlebar tetap | Sama seperti di atas |
| `f32` `f64` | Bilangan floating-point IEEE-754. Literal desimal bawaannya `f64` | [Bilangan bab 4](functions/numbers.md#4-bilangan-floating-point-f64--f32) |
| `ratio` | Bilangan rasional dalam bentuk paling sederhana | [Bilangan bab 5](functions/numbers.md#5-bilangan-rasional-ratio) |
| `bool` | `true` / `false` | [Bilangan bab 7](functions/numbers.md#7-boolean) |
| `char` | Nilai skalar Unicode | [Karakter](functions/collections.md#2-karakter-char) |
| `string` | String yang tidak dapat diubah | [String](functions/collections.md#1-string-string) |
| `symbol` | Simbol. Keyword (`:name`) juga bertipe ini | [Simbol](functions/sequences.md#3-simbol) |
| `()` | Tipe Unit. Nilainya juga `()` | |
| `!` | Tipe Never. Tipe ekspresi yang tidak kembali, seperti `panic`. Dapat ditaruh di tempat tipe apa pun diharapkan | |
| `ptr` `c-long` `c-ulong` | Word yang hanya dipakai untuk mengoper nilai ke dan dari C. Hanya dapat menjadi nilai di dalam `unsafe`, dan tempat kemunculannya terbatas | [Bilangan bab 2](functions/numbers.md#2-word-mentah-pada-batas-dengan-c-ptr--c-long--c-ulong) |
| `random-state` | Keadaan generator bilangan acak | [Bilangan bab 12](functions/numbers.md#12-bilangan-acak) |

Tidak ada tipe bilangan bulat 64 bit. Untuk bilangan bulat yang lebarnya tidak penting, gunakan
`int`.

## 2. Tipe generik bawaan

| Tipe | Isi | Rincian |
|---|---|---|
| `Option<T>` | Nilai yang ada atau tidak. `some` / `none` | [Option dan Result](functions/option-result.md) |
| `Result<T,E>` | Berhasil atau gagal. `ok` / `err` | Sama seperti di atas |
| `Vector<T>` | Larik yang dapat membesar | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Tabel hash. Tipe kunci harus mengimplementasikan `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Handle ke sebuah task | [Task](functions/concurrency.md#1-taskt--handle-ke-task) |
| `Thread<T>` | Handle ke task yang berjalan di thread OS khusus | [Thread](functions/concurrency.md#7-threadt--thread-os-khusus) |
| `Chan<T>` | Kanal | [Kanal](functions/concurrency.md#2-chant--kanal) |

Tipe fungsi ditulis `(fn (tipe-argumen...) tipe-kembalian)`, dan objek trait `:dyn Trait`
([Referensi Sintaks bab 2](syntax.md#2-menulis-tipe)).

## 3. Data S-expression

| Tipe | Isi | Rincian |
|---|---|---|
| `Sexpr` | S-expression tak kosong. 16 varian: `int`, `i8` sampai `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path` | [Data S-expression](functions/sequences.md#2-data-s-expression-sexpr) |
| `Option<Sexpr>` | Data S-expression secara umum. Daftar kosong `()` adalah `none` | Sama seperti di atas |

## 4. Tipe pada pustaka standar

Tipe yang didefinisikan pustaka standar (prelude) dengan `defstruct` / `defenum`. Tipe-tipe ini
diperlakukan sama seperti tipe yang Anda tulis sendiri, dan semua yang dapat dilakukan dengan
`defstruct` dapat dilakukan dengannya.

| Tipe | Isi | Rincian |
|---|---|---|
| `cons-cell<A,B>` | Sepasang nilai. `cons`/`car`/`cdr` | [Pasangan](functions/sequences.md#1-pasangan-cons-cellab) |
| `complex` | Bilangan kompleks (komponen `f64`) | [Bilangan bab 6](functions/numbers.md#6-bilangan-kompleks-complex) |
| `Array<T>` | Larik multidimensi | [Array](functions/collections.md#5-arrayt-larik-multidimensi) |
| `BitVector` | Urutan bit berpanjang tetap | [BitVector](functions/collections.md#6-bitvector-vektor-bit) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Iterator yang dikembalikan oleh `iter` pada tiap koleksi | [Iter](functions/traits.md#1-trait-iter-dan-iterasi) |
| `WaitGroup` | Menunggu N hal selesai | [WaitGroup](functions/concurrency.md#4-waitgroup--menunggu-n-penyelesaian) |
| `Mutex<T>` | Eksklusi mutual untuk data bersama | [Mutex](functions/concurrency.md#6-mutext--eksklusi-mutual-untuk-data-bersama) |
| `pathname` | Nama berkas yang dipecah menjadi bagian-bagian | [Pathname](functions/streams-files.md#9-pathname-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Stream | [Stream](functions/streams-files.md#3-tipe-stream-konkret) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Stream komposit | [Stream komposit](functions/streams-files.md#4-stream-komposit) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Jaringan | [Jaringan](functions/network.md#1-tipe) |
| `ReadOutcome` | Hasil `read-sexpr`. `datum` / `eof` | [Stream](functions/streams-files.md#6-fungsi-generik-dan-operasi-berkas) |
| `universal-time` `internal-time` `decoded-time` | Waktu | [Waktu](functions/system.md#1-waktu) |
| `heap-info` | Keadaan heap saat ini | [Alat implementasi](functions/system.md#51-field-heap-info) |

## 5. Tipe kesalahan

`Error` bukan tipe melainkan trait, dan tipe berikut mengimplementasikannya. Untuk menangani
kesalahan jenis apa pun, tulis `:dyn Error`.

| Tipe | Dihasilkan oleh |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operasi berkas dan stream |
| `NetError` | Operasi jaringan |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Rinciannya ada di [Tipe Kesalahan dan Trait Error](functions/option-result.md#3-tipe-kesalahan-dan-trait-error).

## 6. Implementasi trait standar

Tipe mana yang mengimplementasikan trait mana. Metode tiap trait ada di
[Trait Standar](functions/traits.md) dan di bab-bab yang tercantum pada kolom paling kanan.

### 6.1 Perbandingan, hashing, dan pencetakan

| Trait | Tipe yang mengimplementasikan | Rincian |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-perbandingan) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Sama seperti di atas |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` dan semua tipe kesalahan bawaan | [print-object](functions/printing.md#5-print-object-representasi-cetak-per-tipe) |

`Eq`/`Ord` pada `cons-cell<A,B>` dapat dipakai ketika tipe elemennya mengimplementasikan
`Eq`/`Ord`.

### 6.2 Aritmetika

| Trait | Tipe yang mengimplementasikan |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Rinciannya ada di [Trait aritmetika](functions/traits.md#3-trait-aritmetika-add--sub--mul--div--rem--bits--number).

### 6.3 Iterasi

| Trait | Tipe yang mengimplementasikan |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Stream

| Tipe | Trait yang diimplementasikan |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

Setiap stream mengimplementasikan `Stream`; stream masukan juga mengimplementasikan `InputStream`,
dan stream keluaran `OutputStream`. `socket-listener` dan `udp-socket` hanya mengimplementasikan
`Stream` (`close` / `open-stream-p`). Rinciannya ada di
[Stream](functions/streams-files.md#1-hierarki-trait).

### 6.5 Lainnya

| Trait | Tipe yang mengimplementasikan | Rincian |
|---|---|---|
| `Error` | Semua tipe kesalahan pada bab 5 | [Tipe kesalahan](functions/option-result.md#3-tipe-kesalahan-dan-trait-error) |
| `Pathish` | `string` `pathname` | [Pathname](functions/streams-files.md#91-trait-penunjuk-pathname-pathish) |
