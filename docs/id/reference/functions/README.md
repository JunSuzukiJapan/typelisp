<!-- translated-from: docs/ja/reference/functions/README.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
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
