<!-- translated-from: docs/ja/reference/functions/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# Fungsi Bawaan

Daftar fungsi bawaan, metode, dan pustaka standar. Untuk sintaks (bentuk khusus dan cara
mendefinisikan sesuatu), lihat [Referensi Sintaks](../syntax.md); untuk daftar tipe, lihat
[Tipe](../types.md).

## Bentuk pemanggilan

Ada tiga bentuk pemanggilan.

- Fungsi bebas: `(name args...)`
- Metode instans: `(name receiver args...)` (diselesaikan dari tipe statis argumen pertama)
- Metode statis (fungsi terkait): `(Type::name args...)`

Setiap tipe boleh memiliki metode bernama sama miliknya sendiri. `(+ a b)` memanggil `+` milik tipe
`a`.

## Nama yang memerlukan `use`

Dari nama-nama pustaka standar, yang berikut dapat dipakai di setiap modul tanpa menulis apa pun
([referensi sintaks 3.10](../syntax.md#310-module--use--ruang-nama)):

- tipe primitif, `Option`/`Result`/`Vector`, iterasi, ekspresi S dan list, fungsi sekuens, makro
  kendali, perbandingan dan trait aritmetika
- setiap nama yang juga nama paket `COMMON-LISP` milik Common Lisp (`length`, `format`, `open`, dan
  lain-lain)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, trait stream `Stream`,
  `InputStream`, dan `OutputStream`, serta `Hash`, yang diimplementasikan oleh kunci `HashTable`
- tipe yang disebut oleh fungsi dan variabel semua itu

Nama lain dibawa masuk dengan `use`, seperti `(use std::net::tcp-connect)`, atau ditulis dengan
path-nya, `std::net::tcp-connect`. Tabel berikut memuat nama-nama itu dan modul masing-masing.

| Modul | Nama |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`, `digit-weight`, `name->char` |
| `std::collections` | `Deque`, `HashSet`, `Scope`, `SortedTable`, `hashtable-iter` |
| `std::env` | `command-line-args`, `getenv`, `home-directory` |
| `std::error` | `WrappedError`, `as-dyn-error`, `assert-eq`, `describe-error`, `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`, `direction-input`, `direction-output`, `directory-p`, `open-binary`, `open-binary-input`, `open-binary-output`, `open-file`, `open-input`, `open-output`, `read-file-lines`, `read-file-string`, `write-file-string` |
| `std::hash` | `*sxhash-mask*`, `sxhash-string` |
| `std::io` | `ByteInput`, `ByteOutput`, `PeekInput`, `binary-file-stream`, `copy-stream`, `make-peek-stream`, `peek-stream`, `read-lines`, `write-lines` |
| `std::net` | `NetError`, `byte-stream-of`, `char-stream-of`, `datagram`, `socket-byte-stream`, `socket-listener`, `socket-stream`, `tcp-connect`, `tcp-listen`, `tls-connect`, `tls-listen`, `udp-bind`, `udp-socket`, `unix-connect`, `unix-listen`, `with-connection` |
| `std::num` | `ParseFloatError`, `ParseIntError`, `atan2`, `parse-float`, `parse-int` |
| `std::path` | `Pathish`, `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`, `read-from-string-preserving-whitespace`, `read-sexpr`, `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`, `right-trim`, `string->utf8`, `trim`, `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`, `Context`, `Mutex`, `WaitGroup`, `after`, `with-lock` |
| `std::thread` | `Task`, `Thread`, `yield` |
| `std::time` | `internal-time-seconds`, `timezone-daylight-p`, `timezone-offset-seconds` |

## Cara membaca tabel

Tabel di setiap bab memiliki kolom "nama, bentuk, tipe, deskripsi". Kolom tipe ditulis sebagai
`(tipe-argumen,...)→tipe-kembalian`.

- Satu huruf kapital seperti `T`, `A`, atau `B` adalah variabel tipe.
- Catatan seperti `where Eq A` adalah batas trait yang harus dipenuhi variabel tipe.
- `Iter<A>` berarti "implementasi `Iter` apa pun yang `Item`-nya adalah `A`".
- Argumen bertanda `&optional` / `&key` boleh dihilangkan.

## Bab

| Berkas | Isi |
|---|---|
| [numbers.md](numbers.md) | Bilangan bulat, bilangan floating-point, rasional, bilangan kompleks, boolean, operasi bit, bilangan acak |
| [sequences.md](sequences.md) | Pasangan `cons-cell`, data S-expression `Sexpr`, simbol, fungsi sekuens, iterator malas `lazy`, fungsi tingkat tinggi |
| [collections.md](collections.md) | String, karakter, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, tipe kesalahan, dan trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, trait aritmetika |
| [printing.md](printing.md) | `print`/`println`/`format`, pretty printer, `print-object`, variabel kendali pencetak |
| [format.md](format.md) | Direktif format |
| [streams-files.md](streams-files.md) | Stream, operasi berkas, pathname, readtable |
| [concurrency.md](concurrency.md) | Task, kanal, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, socket domain Unix, UDP |
| [system.md](system.md) | Waktu, lingkungan runtime, alat implementasi, `read`/`eval`, docstring, fungsi terkait makro |
