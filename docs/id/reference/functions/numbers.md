<!-- translated-from: docs/ja/reference/functions/numbers.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Bilangan

Operasi pada bilangan bulat, bilangan floating-point, rasional, bilangan kompleks, dan boolean, serta
fungsi lain yang terkait bilangan. Untuk cara membaca bentuk pemanggilan, lihat
[Fungsi Bawaan](README.md).

## 1. Bilangan bulat berlebar tetap

Ada tujuh tipe bilangan bulat: **`int`** (`integer` pada CL: presisi sembarang, dan tipe bawaan
literal bilangan bulat tanpa anotasi; bab 3), serta yang berlebar tetap `i8` `i16` `i32` `u8` `u16`
`u32`. Untuk tipe mana sebuah operasi diselesaikan ditentukan oleh tipe argumen pertama (tipe-tipe
itu saling bebas, tanpa konversi implisit). **Tidak ada tipe bilangan bulat 64 bit.** Nilai saat
dijalankan adalah satu word yang bit rendahnya adalah tag, sehingga hanya 63 bit yang tersisa untuk
bilangan bulat langsung, dan tipe yang mengklaim 64 bit harus membuang bit teratas di suatu tempat.
`int` menjadi bignum setelah melewati 63 bit itu, jadi jika lebarnya tidak penting, gunakan `int`.
Tabel di bawah untuk enam tipe berlebar tetap (tabel untuk `int` ada di bab 3).

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | Empat operasi aritmetika. `/` memotong ke arah nol dan panic pada pembagian dengan nol |
| `mod` | `(mod a b)` | `(T,T)→T` | Sisa bagi (`mod` pada CL, **pembagian floor**: tandanya mengikuti pembagi. `(mod -7 3)`→`2`). Panic pada pembagian dengan nol |
| `rem` | `(rem a b)` | `(T,T)→T` | Sisa bagi (`rem` pada CL, **pembagian pemotongan**: tandanya mengikuti yang dibagi. `(rem -7 3)`→`-1`). Panic pada pembagian dengan nol |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Sepadan dengan `floor`/`ceiling`/`round`/`truncate` dua argumen pada CL (`(floor 7 2)`→hasil bagi 3, sisa 1). Sebagai ganti nilai ganda, mengembalikan hasil bagi dan sisa dalam `cons-cell` (`car`=hasil bagi, `cdr`=sisa). `round-div` membulatkan nilai tengah ke genap, seperti CL |
| `abs` | `(abs x)` | `T→T` | Nilai mutlak |
| `signum` | `(signum x)` | `T→T` | Tanda (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Faktor persekutuan terbesar |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Kelipatan persekutuan terkecil (0 jika salah satunya 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | Yang lebih besar / lebih kecil (tiga argumen atau lebih diekspansi oleh gula variadik pada bab 8) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Perbandingan |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Semuanya sama dengan `=` (tidak ada perbedaan untuk bilangan bertipe sama) |
| `int->float` | `(int->float x)` | `T→f64` | Konversi pelebaran ke `f64` |
| `int->int` | `(int->int x)` | `T→int` | Konversi pelebaran ke `int` (selalu tepat). Yang dilakukan `(as int x)` |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Konversi pelebaran ke `ratio` (selalu tepat) |
| `int->char` | `(int->char x)` | `T→char` | Menafsirkan nilai sebagai nilai skalar Unicode. Panic pada nilai yang tidak sah |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Versi `int->char` yang mengembalikan `None` saat gagal |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Konversi lebar. Nilai yang tidak muat dipotong (seperti `as` pada Rust) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Konversi yang sama sebagai pertanyaan. `None` jika nilai tidak muat pada lebar itu |

Konversi ini juga yang dilakukan bentuk khusus `(as Type x)`/`(try-as Type x)`
([Referensi Sintaks](../syntax.md#7-bentuk-khusus-lainnya)). Operasi bit (`logand`/`ash`/`ldb` dan
sebagainya) dan predikat (`zerop`/`evenp` dan sebagainya) memiliki bentuk yang sama pada semua tipe,
sehingga dikumpulkan di bab 11 dan 9.

`i8` `i16` `u8` `u16` `u32` memiliki tepat tabel bab ini, dan `f32` memiliki tepat tabel `f64` pada
bab 4.

**Nama tipe hanya berarti lebar dan tandanya, tidak lebih.** `i32` berarti "perlakukan 32 bit sebagai
bertanda" dan `u32` berarti "perlakukan 32 bit sebagai tak bertanda". `(+ (the u8 200) (the u8 100))`
adalah `44`, `(+ 2147483647 1)` (sebagai `i32`) adalah `-2147483648`, dan `(lognot (the u32 0))`
adalah `4294967295`. `f32` sama saja: binary32 yang sebenarnya. `(/ (the f32 1.0) (the f32 3.0))`
dicetak sebagai `0.33333334`, nilai yang berbeda dari hasil `f64` `0.3333333333333333`.

Katalog turunan CL (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` dan predikat bab 9) ada untuk
`int`/`i32`/`f64`/`ratio`. Jika Anda membutuhkannya untuk lebar lain, pindah dengan `(as int x)` /
`(as i32 x)` (konversi lebar ada untuk setiap pasangan).

## 2. Word mentah pada batas dengan C (`ptr` / `c-long` / `c-ulong`)

Tiga tipe yang hanya dipakai untuk mengoper nilai ke dan dari fungsi C yang dideklarasikan dengan
[`defffi`](../syntax.md#33-defffi--deklarasi-fungsi-c-ffi). `ptr` adalah pointer buram, dan
`c-long` / `c-ulong` adalah `long` / `unsigned long` pada C. Menjadikannya sebuah nilai mensyaratkan
berada di dalam `(unsafe ...)`.

**Tidak ada aritmetika.** Tidak satu pun tabel bab 1 berlaku: baik `(+ p 1)` maupun `(< n m)` tidak
dapat ditulis. Ini adalah word untuk diserahkan ke C, bukan tipe untuk berhitung, jadi untuk
berhitung, pindah ke tipe berlebar. `c-long` / `c-ulong` hanya memiliki konversi:

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | Konversi lebar yang sama seperti bab 1. Nilai yang tidak muat dipotong |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Konversi yang sama sebagai pertanyaan |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | Jalan masuk, dari word mentah yang lain dan dari tipe bilangan bulat pada bab 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Sama seperti di atas |
| `int->int` | `(int->int x)` | `T→int` | **Selalu tepat**. Cara jujur membaca `size_t` yang tidak muat dalam `i32` |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` adalah yang dilakukan konversi ini,
dan konversinya ada untuk setiap pasangan dengan tipe bilangan bulat pada bab 1. `ptr` bahkan tidak
memiliki tabel ini: tidak disediakan cara membaca pointer sebagai bilangan. Ia adalah nilai yang
hanya dioper, diterima, dan diserahkan ke fungsi C lain.

**Ia juga tidak dapat dicetak.** `(println "~a" x)` tidak menerima word mentah (ia tidak memiliki
representasi `Sexpr`), jadi pindahkan lebih dulu ke tipe berlebar, seperti
`(println "~a" (as int n))`.

"Tidak ada tipe bilangan bulat 64 bit" dari awal bab 1 berlaku juga untuk ketiganya. Ia berlaku
**karena ketiganya tidak dapat disimpan**: tidak dapat menjadi field `defstruct`, `defvar`, di dalam
argumen tipe, atau di dalam `Sexpr`, sehingga ketiganya adalah word yang hanya lewat melalui fungsi
sebagai argumen, nilai kembalian, dan variabel lokal. Untuk rinciannya, lihat
[Referensi Sintaks](../syntax.md#ptr--c-long--c-ulong--word-mesin-mentah).

## 3. Bilangan bulat presisi sembarang `int`

`integer` pada CL, dan **bilangan bulat** bahasa ini: literal bilangan bulat tanpa anotasi bertipe
ini, dan fungsi bawaan yang mengembalikan bilangan, seperti `length` dan `char->int`, mengembalikan
tipe ini. Nilai disimpan sebagai nilai langsung 63 bit (fixnum) selama muat, dipromosikan secara
otomatis menjadi bignum ketika hasil operasi tidak lagi muat, dan kembali menjadi nilai langsung
ketika muat lagi. `eq` selalu identitas nilai dalam rentang fixnum, dan `eql`/`=` adalah identitas
numerik di seluruh rentang. Ia adalah tipe yang berbeda dari tipe bilangan bulat berlebar tetap (bab
1), tanpa konversi implisit: `(as int x)` adalah pelebaran tepat dari lebar tetap, dan
`(as i32 n)` / `(try-as i32 n)` adalah pemotongan / pemeriksaan dari `int` (makna yang sama seperti
`int->W` / `try-int->W` pada bab 1).

Varian bilangan bulat pada `Sexpr` juga hanya `int` (`(int n)` menerima fixnum maupun bignum).

Fungsi bawaan yang menerima indeks atau jumlah (`substring`, `get` pada `Vector`, jumlah geser pada
`ash`, dan sebagainya) menerima `int`, tetapi memberikan nilai yang tidak muat dalam fixnum adalah
kesalahan saat dijalankan ("an integer argument does not fit a fixnum").

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Tidak pernah overflow (dipromosikan) |
| `/` | `(/ a b)` | `(int,int)→int` | Memotong ke arah nol. Panic pada pembagian dengan nol |
| `mod` | `(mod a b)` | `(int,int)→int` | Sisa pembagian floor (tandanya mengikuti pembagi) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Semuanya `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Sama seperti bab 11 (komplemen dua dengan bit tak hingga) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Sama seperti bab 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Pemotongan / pemeriksaan. `W` adalah salah satu dari enam lebar atau `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Identitas (pada sisi lebar tetap dan word C, `int->int` melebarkan; bab 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | Bentuk yang sama seperti bab 1. `expt` hanya menerima eksponen non-negatif |

## 4. Bilangan floating-point (`f64` / `f32`)

`f32` memiliki tabel yang sama.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. Pembagian dengan nol tidak melakukan panic; hasilnya `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Sisa pembagian floor (seperti di CL; tandanya mengikuti pembagi. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Sisa pembagian pemotongan (seperti di CL; tandanya mengikuti yang dibagi. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Perbandingan |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Semuanya sama dengan `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Pangkat |
| `abs` | `(abs x)` | `f64→f64` | Nilai mutlak |
| `signum` | `(signum x)` | `f64→f64` | Tanda (`1.0`/`-1.0`; `±0.0`/`NaN` dikembalikan apa adanya. Seperti di CL, tidak seperti `signum` pada Rust) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | Yang lebih besar / lebih kecil (tiga argumen atau lebih diekspansi oleh gula variadik pada bab 8) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Operasi uner |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Fungsi transendental. `log` adalah logaritma natural |
| `log` (dua argumen) | `(log x base)` | `(f64,f64)→f64` | Logaritma dengan basis tertentu. Diekspansi menjadi `(/ (log x) (log base))` (bab 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Sepadan dengan versi dua argumen pada CL (`(floor 7.0 2.0)`→hasil bagi 3, sisa 1). Rancangan yang sama seperti fungsi bernama sama pada bab 1 (`car`=hasil bagi, `cdr`=sisa) |
| `float->int` | `(float->int x)` | `f64→int` | Mengubah menjadi `int` dengan memotong ke arah nol (`truncate` pada CL; tepat untuk nilai hingga berukuran berapa pun). Panic pada tak hingga dan NaN. Untuk lebar tetap, gunakan `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Mengubah menjadi `ratio` sebagai rasional biner yang tepat (`rational` pada CL) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Mengubah antarlebar floating-point. `float->f32` membulatkan ke yang terdekat, `float->f64` selalu tepat. Yang dilakukan `(as f32 x)` |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | Konversi yang sama sebagai pertanyaan. `none` jika pembulatan mengubah nilai (pelebaran ke `f64` selalu `some`). Yang dilakukan `(try-as f32 x)` |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | Fungsi bernama sama pada CL. Alias `floor`/`ceiling`/`round`/`truncate` di atas: di CL yang tanpa awalan mengembalikan bilangan bulat, sehingga yang berawalan `f` sesuai dengan perilaku bahasa ini |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | Masing-masing 2 / 53 / 53 (hanya presisi `0.0` yang 0). `f64` selalu IEEE-754 binary64, sehingga ini konstanta |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` atau `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | Mantisa (dalam `[1/2,1)`, tanpa tanda) dan eksponen. CL mengembalikan tiga nilai, tetapi tidak ada nilai ganda, sehingga tanda diserahkan ke `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | Dekomposisi yang sama dengan mantisa bilangan bulat 53 bit yang tepat. `mantissa * 2^exponent` tepat sama dengan nilai asli |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **Rasional paling sederhana yang terbaca kembali sebagai float itu** (`(rationalize 0.1)` adalah `1/10`). Untuk nilai biner yang tepat, gunakan `float->ratio` |

**Perbedaan dari CL: cara `round` membulatkan.** `round` (dan karenanya `fround`/`round-div`)
membulatkan **menjauhi nol** (`(round 2.5)` = `3.0`). CL membulatkan **ke genap**, menghasilkan `2`.

## 5. Bilangan rasional `ratio`

Rasional presisi sembarang yang kompatibel dengan CL. Selalu disimpan dalam bentuk paling sederhana
dengan penyebut positif, dan dialokasikan di heap. Tidak ada konversi implisit dengan tipe bilangan
bulat atau `f64` (gunakan metode konversi eksplisit atau `as`/`try-as`). Untuk sintaks literal
rasio, lihat [Referensi Sintaks](../syntax.md#1-unsur-leksikal).

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | Empat operasi (hasil selalu dalam bentuk paling sederhana). `/` panic pada pembagian dengan nol |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Sisa pembagian floor (seperti di CL; tandanya mengikuti pembagi) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Sisa pembagian pemotongan (seperti di CL; tandanya mengikuti yang dibagi) |
| `abs` | `(abs x)` | `ratio→ratio` | Nilai mutlak |
| `signum` | `(signum x)` | `ratio→ratio` | Tanda (mengembalikan `1`/`-1`/`0` sebagai `ratio`) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Pangkat. Eksponen harus berupa `ratio` bernilai bilangan bulat (jika tidak, panic). Eksponen negatif menghasilkan kebalikan |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | Yang lebih besar / lebih kecil |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` tidak memiliki operasi bit (di CL hanya untuk bilangan bulat) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Perbandingan |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Semuanya sama dengan `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Pembilang dalam bentuk paling sederhana (nama sama seperti di CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Penyebut dalam bentuk paling sederhana (selalu positif) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Bagian bilangan bulat (dipotong ke arah nol) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Mengubah menjadi `f64` |

Jalan masuk dari bilangan bulat berlebar tetap dan `f64` adalah `int->int`/`int->ratio` (bab 1) dan
`float->int`/`float->ratio` (bab 4). `int`/`ratio` adalah tipe terpisah yang bebas dari `i32` dan
lainnya, dan aritmetika campuran memerlukan konversi eksplisit.

## 6. Bilangan kompleks `complex`

Sebuah struct (`defstruct`) pada pustaka standar.

**Dua perbedaan dari CL** (keduanya berasal dari tipe statis):

1. **Komponennya selalu `f64`.** Complex pada CL juga dapat menyimpan rasional, dan `(complex 1 2)`
   dan `(complex 1.0 2.0)` adalah tipe yang berbeda. Tipe statis harus memilih satu, dan fungsi
   transendental mengembalikan jenis floating-point.
2. **`(sqrt -1.0)` adalah `sqrt` riil (NaN).** Di CL, `sqrt` dapat mengembalikan bilangan kompleks
   dari bilangan riil, tetapi `sqrt` pada `f64` harus mengembalikan `f64`. Hasil kompleks berasal
   dari argumen kompleks: `(sqrt (complex -1.0 0.0))` adalah `i`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Konstruksi. Komponen dapat dibaca langsung sebagai `z::re`/`z::im` |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Bagian riil dan bagian imajiner. **Keduanya juga bekerja pada bilangan riil** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), seperti di CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Konjugat (juga bekerja pada bilangan riil) |
| `phase` | `(phase z)` | `complex→f64` | Argumen dalam (-pi,pi] (juga bekerja pada bilangan riil) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Nilai mutlak. **Satu-satunya `abs` yang tidak mengembalikan tipe penerima** (seperti di CL, nilai mutlak bilangan kompleks adalah riil) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Aritmetika kompleks |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Kesamaan per komponen. `Eq` juga diimplementasikan (tidak ada `Ord`: bilangan kompleks tidak memiliki urutan, dan `<` pada CL juga menolaknya) |
| `zerop` | `(zerop z)` | `complex→bool` | Apakah kedua komponen 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` memberikan nilai utama |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | Sudut vektor `(x,y)`. **`(atan y x)` dua argumen pada CL adalah gula untuk ini** (ia bercabang pada jumlah argumen, seperti `log` dua argumen) |

Ia mengimplementasikan `print-object`, sehingga `~a`/`~s` mencetaknya sebagai `#C(re im)`, seperti
CL (reader bahasa ini tidak memiliki sintaks `#C` untuk membacanya kembali).

## 7. Boolean

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Negasi |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Semuanya membandingkan nilai untuk kesamaan |

`and`/`or` memerlukan evaluasi hubung singkat, sehingga keduanya adalah bentuk khusus
([Referensi Sintaks](../syntax.md#4-pengikatan-dan-percabangan)).

## 8. Pembantu numerik dan gula pemanggilan

`abs`/`signum` (semua tipe numerik), `gcd`/`lcm` (hanya tipe bilangan bulat), `rem` (semua tipe riil
termasuk `f64`), dan `expt` (`int`/`f64`/`ratio`) didefinisikan sebagai metode pada setiap tipe
numerik (diselesaikan berdasarkan tipe penerima: `(abs x)` adalah metode untuk tipe `x`). Rincian
tiap tipe ada di bab 1, 3, 4, dan 5. Bilangan bulat berlebar tetap tidak memiliki `expt` (tidak ada
promosi dan akan overflow; pindah ke `int` dengan `(as int x)` dan gunakan `expt`-nya).

### 8.1 Bentuk variadik dan 0/1 argumen

Aritmetika dan perbandingan CL bersifat variadik, tetapi metode diselesaikan hanya berdasarkan tipe
penerima, bukan jumlah argumen. Jadi **pemeriksa mengekspansi bentuk-bentuk berikut menjadi
pemanggilan dua argumen**.

| Bentuk yang dapat Anda tulis | Ekspansi | Berlaku untuk |
|---|---|---|
| `(op a b c ...)` | Lipatan kiri `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)` dengan tiap suku diikat ke variabel sementara | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Yang di atas yang memiliki elemen identitas |
| `(op x)` | Untuk `+ * max min logand logior logxor`, `x` itu sendiri. `(- x)` menegasikan, `(/ x)` memberikan kebalikan, `(gcd x)`/`(lcm x)` memberikan `(abs x)` (seperti di CL) | Sama seperti di atas |
| `(cmp x)` | Mengevaluasi `x` dan memberikan `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Setiap suku dievaluasi tepat sekali, dari kiri ke kanan (itulah sebabnya perbandingan variadik
melalui variabel sementara). Bentuk variadik `/=` membandingkan **pasangan bersebelahan**, tidak
seperti CL, yang menanyakan apakah semua pasangan berbeda.

### 8.2 `isqrt` dan `expt` bilangan bulat

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | Bilangan bulat terbesar yang tidak melebihi akar kuadrat. Panic pada nilai negatif |
| `expt` | `(expt n e)` | `(T,T)→T` | Pangkat (dengan pengkuadratan). CL mengembalikan rasional untuk eksponen negatif, tetapi tipe bilangan bulat tidak dapat merepresentasikannya, sehingga panic; ubah dulu ke `ratio` |

## 9. Predikat

| Nama | Bentuk | Tipe | Tipe yang didukung |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (hanya tipe bilangan bulat, seperti di CL) |

**Tidak ada predikat tipe** seperti `numberp`/`integerp`/`floatp` pada CL. Dengan tipe statis, tipe
sebuah nilai sudah pasti tanpa perlu ditanyakan saat dijalankan.

## 10. Konstanta

| Nama | Tipe | Nilai |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Kode operasi yang diberikan ke `boole` (sebagai ganti keyword CL) |

Konstanta batas numerik (CLHS 12.1.4.2 / 12.1.3):

| Nama | Tipe | Deskripsi |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | Batas atas / bawah nilai langsung 63 bit (2^62-1 / -2^62). `int` di luar batas itu menjadi bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | Nilai hingga terbesar / terkecil |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | Besaran bukan nol terkecil, termasuk subnormal |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | Sama, dibatasi pada bilangan ternormalisasi |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | Mengikuti definisi CL (`e` positif terkecil dengan `(/= (+ 1 e) 1)`), sehingga nilainya **satu ULP lebih besar dari** 2^-53: 2^-53 sendiri dibulatkan kembali ke `1.0` pada pembulatan ke genap terdekat |

## 11. Operasi bit

Didefinisikan pada komplemen dua dengan bit tak hingga (CL 12.10). Diimplementasikan untuk tipe
bilangan bulat berlebar tetap dan `int`, tidak untuk `ratio` (CL juga hanya memiliki operasi bit
untuk bilangan bulat).

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | And, or, exclusive or bitwise (versi variadik dan nol argumen di 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Komplemen bitwise |
| `ash` | `(ash x count)` | `(T,int)→T` | Geser aritmetika. Ke kiri jika `count` positif, ke kanan jika negatif |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Apakah bit `index` terset (**urutan argumen kebalikan dari CL**; lihat di bawah) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | Jumlah bit yang terset (untuk bilangan negatif, jumlah bit 0) |
| `integer-length` | `(integer-length x)` | `T→T` | Jumlah bit yang diperlukan untuk merepresentasikannya, tanpa menghitung tanda |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | Tujuh sisanya, disusun dari yang di atas |

**Hanya argumen kedua `ash` yang `int` dan bukan `T`.** Ia adalah **jarak** dalam bit, bukan nilai
bertipe penerima, sehingga lebar dan tanda penerima tidak mengatakan apa-apa tentang jarak (untuk
alasan yang sama bahwa `count` pada `(ash integer count)` CL adalah bilangan bulat apa pun).
Menggeser nilai tak bertanda ke kanan adalah geser logika (`(ash (the u8 200) -3)` = `25`), dan nilai
bertanda adalah geser aritmetika yang membulatkan ke arah tak hingga negatif
(`(ash (the i32 -100) -4)` = `-7`). `index` pada `logbitp` adalah `int` untuk alasan yang sama.

**Byte specifier.** Sebagai ganti objek buram yang dikembalikan `byte` pada CL, dipakai
`cons-cell<int,int>` (`car`=ukuran, `cdr`=posisi). Baik ukuran maupun posisi adalah jumlah bit,
sehingga keduanya `int` berapa pun lebar bilangan bulat yang dibongkar.

**Bilangan bulat adalah argumen pertama, dengan urutan berbeda dari CL.** CL menulis
`(ldb bytespec integer)`, tetapi bahasa ini memilih metode berdasarkan tipe penerima (argumen
pertama), dan dengan specifier di depan ia tidak dapat memilih berdasarkan tipe bilangan bulat.
Semua operasi bit lainnya berbentuk `(op integer ...)` (`(logand a b)`, `(ash x count)`,
`(lognot x)`), dan hanya keluarga `ldb` dan `logbitp` yang terbalik, sehingga keduanya disamakan.
Argumen sisanya mempertahankan urutan relatif CL, sehingga `(dpb newbyte spec n)` menjadi
`(dpb n newbyte spec)`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Membuat byte specifier |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Mengambil sebuah komponen |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Mengekstrak byte yang ditentukan dari `x`, rata kanan |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Apakah ada bit yang terset pada byte yang ditentukan |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Mengosongkan semua di luar byte yang ditentukan (mempertahankan posisi) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Menaruh `newbyte` yang rata kanan ke dalam byte `x` yang ditentukan |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Versi `dpb` yang mempertahankan posisi |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | Salah satu dari 16 operasi logika dua operan, dipilih oleh `op` (konstanta `boole-*` dari bab 10) |

`T` adalah tipe yang mengimplementasikan trait `Bits`, yaitu `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`.
Hanya `boole` yang mempertahankan `op` di depan, karena tidak ada alasan mengubah urutan CL di sana.

## 12. Bilangan acak

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Bilangan acak dari `0` sampai tetapi tidak termasuk `n`. Jika state dihilangkan, mengambil dari `*random-state*` dan memajukannya |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Tanpa argumen, state baru; dengan argumen, salinannya (salinan memutar ulang urutan yang sama) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Selalu `true` (tipe statis sudah menyingkirkan tipe lain; ada hanya untuk bersesuaian dengan CL) |
| `*random-state*` | — | `random-state` | State bawaan `random`. Variabel global yang dapat ditugasi (gantilah dengan `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | State yang dinamai oleh bilangan bulat itu. Seed yang sama selalu memutar ulang urutan yang sama |

Generatornya adalah xorshift64 dan mengembalikan urutan yang sama baik diinterpretasi maupun
dikompilasi.

State baru dari `make-random-state` diberi seed dari jam dinding, sehingga tidak dapat direproduksi
antarpenjalanan. Untuk mereproduksi, gunakan `seed-random-state`:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; prints the same three numbers on every run
```

**CL tidak memiliki cara portabel untuk memberi seed** (`make-random-state` hanya menerima
`nil`/`t`/sebuah state), sehingga nama ini mengikuti `sb-ext:seed-random-state` pada SBCL, bukan CL.

Seed yang berbeda menghasilkan urutan yang berbeda. `(seed-random-state 0)` dan
`(seed-random-state 1)` menghasilkan urutan yang berbeda, begitu pula `-7` dan `7`.
