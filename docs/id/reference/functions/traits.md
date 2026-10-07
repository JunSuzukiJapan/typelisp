<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Trait Standar

Trait untuk iterasi, perbandingan, dan aritmetika. Trait standar lainnya ada di bab masing-masing:
`Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Tipe kesalahan](option-result.md#3-tipe-kesalahan-dan-trait-error)), `print-object`
([Pencetakan](printing.md#5-print-object-representasi-cetak-per-tipe)), serta trait stream dan
`Pathish` ([Stream dan Berkas](streams-files.md)). Tipe mana yang mengimplementasikan trait mana ada
di [Tipe](../types.md). Cara mendefinisikan trait ada di
[Referensi Sintaks](../syntax.md#39-deftrait--impl--trait).

## 1. Trait `Iter` dan iterasi

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` mengimplementasikan `Iter` melalui `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (dapatkan iteratornya dengan `(iter collection)`).
`Chan<T>` adalah `Iter` itu sendiri (`recv` berperan sebagai `next`; [Kanal](concurrency.md#2-chant--kanal)).
Daftar `Sexpr` tidak mengimplementasikan `Iter` (tipe elemennya tidak seragam). Jika Anda
mengimplementasikan `Iter` untuk tipe Anda sendiri, tipe itu dapat ditelusuri dengan `doiter` apa
adanya, dan diserahkan ke [fungsi sekuens](sequences.md#4-fungsi-sekuens-pada-iter).

## 2. `Eq` / `Ord` (perbandingan)

Ini sepadan dengan `PartialEq`/`PartialOrd` pada Rust (dinamai `Eq`/`Ord`). Keduanya dipakai pada
batas `where` fungsi generik untuk mensyaratkan bahwa tipe elemen dapat dibandingkan
(`sort`/`member`/`assoc` dan sebagainya).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; must be implemented
  (not-equals ((self Self) (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less ((self Self) (other Self)) bool)                  ; must be implemented
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

Untuk mengimplementasikan `Eq` Anda hanya menulis `equals`, dan untuk `Ord` hanya `less`. Implementasi
bawaan mengisi sisanya. `Ord` mewarisi `Eq`, sehingga `impl Eq X` diperlukan sebelum `impl Ord X`.

Setiap metode trait dapat dipanggil sebagai fungsi apa adanya (di dalam batas `where (Eq A)`/
`(Ord A)`, atau pada tipe konkret yang mengimplementasikannya):

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Apakah keduanya sama (`==` pada Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Apakah keduanya tidak sama (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` diimplementasikan untuk: semua tipe numerik (`i8` sampai `u32` / `f32` / `f64` / `int` /
`ratio`), `bool` `char` `string` `symbol` `complex`, `Sexpr` (`eq`, yaitu identitas; dipakai oleh
pola nilai pada `match`), dan `cons-cell<A,B>` (secara rekursif, ketika elemennya `Eq`). `Ord`
diimplementasikan untuk: semua tipe numerik, `char` `string`, dan `cons-cell<A,B>` (secara
leksikografis, ketika elemennya `Ord`).

Nama metode tidak tumpang tindih dengan operator bawaan (`= /= < <= > >=`) atau `eq`/`lt` karena
fungsi bawaan tidak dapat didefinisikan ulang, dan setiap implementasi mendelegasikan kepadanya.
Operator perbandingan skalar sendiri adalah metode bawaan pada tiap tipe penerima
([Bilangan](numbers.md), [String dan Karakter](collections.md)). Di dalam sebuah batas, menulis
operator dibaca sebagai metode trait (bab 3).

## 3. Trait aritmetika (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Lapisan agar kode generik dapat mensyaratkan "tipe yang dapat dijumlahkan". **Aritmetika pada tipe
konkret memakai operator bawaan** ([Bilangan](numbers.md)) dan tidak melalui lapisan ini.

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; no methods; a combination of six
```

**Di dalam batas, Anda dapat menulis operator.** Ketika penerimanya adalah variabel tipe yang
dibatasi oleh `where`, operator dibaca sebagai metode trait (`+`→`add`, `-`→`sub`, `*`→`mul`,
`/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Metode trait tidak dinamai `+` karena `+` adalah nama metode bawaan dan `impl` menolak
mendefinisikannya ulang (`cannot redefine built-in method`). Tidak ada `Neg`: `(- x)` diekspansi
menjadi `(- (- x x) x)`, sehingga `Sub` sudah cukup.

Diimplementasikan untuk: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` pada semua tipe numerik (kecuali
`complex`), dan `Bits` pada semua tipe bilangan bulat dan `int`.
