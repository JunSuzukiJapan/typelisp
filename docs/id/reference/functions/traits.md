<!-- translated-from: docs/ja/reference/functions/traits.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# Trait Standar

Trait untuk iterasi, perbandingan, aritmetika, serta penambahan dan pengambilan elemen. Trait standar lainnya ada di bab masing-masing:
`Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Tipe kesalahan](option-result.md#3-tipe-kesalahan-dan-trait-error)), `print-object`
([Pencetakan](printing.md#5-print-object-representasi-cetak-per-tipe)), serta trait stream dan
`Pathish` ([Stream dan Berkas](streams-files.md)). Tipe mana yang mengimplementasikan trait mana ada
di [Tipe](../types.md). Cara mendefinisikan trait ada di
[Referensi Sintaks](../syntax.md#39-deftrait--impl--trait).

## 1. Trait `Iter` dan iterasi

```lisp
(deftrait Iter ()
  (:type Item)
  (next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` mengimplementasikan `Iter` melalui `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (dapatkan iteratornya dengan `(iter collection)`).
`Chan<T>` adalah `Iter` itu sendiri (`recv` berperan sebagai `next`; [Kanal](concurrency.md#2-chant--kanal)).
Daftar `Sexpr` tidak mengimplementasikan `Iter` (tipe elemennya tidak seragam). Jika Anda
mengimplementasikan `Iter` untuk tipe Anda sendiri, tipe itu dapat ditelusuri dengan `doiter` apa
adanya, dan diserahkan ke [fungsi sekuens](sequences.md#4-fungsi-sekuens-pada-iter).

## 2. `Eq` / `Ord` (perbandingan)

Ini sepadan dengan `PartialEq`/`PartialOrd` pada Rust (dinamai `Eq`/`Ord`). Keduanya dipakai pada
batas `:where` fungsi generik untuk mensyaratkan bahwa tipe elemen dapat dibandingkan
(`sort`/`member`/`assoc` dan sebagainya).

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; must be implemented
  (not-equals (self (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less (self (other Self)) bool)                  ; must be implemented
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
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
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; tanpa metode; gabungan lima
```

**Di dalam batas, Anda dapat menulis operator.** Ketika penerimanya adalah variabel tipe yang
dibatasi oleh `:where`, operator dibaca sebagai metode trait (`+`→`add`, `-`→`sub`, `*`→`mul`,
`/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Metode trait tidak dinamai `+` karena `+` adalah nama metode bawaan dan `impl` menolak
mendefinisikannya ulang (`cannot redefine built-in method`). Tidak ada `Neg`: `(- x)` diekspansi
menjadi `(- (- x x) x)`, sehingga `Sub` sudah cukup.

Diimplementasikan untuk: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` pada semua tipe numerik (kecuali
`complex`), dan `Bits` pada semua tipe bilangan bulat dan `int`.


**`Div` menamai tipe hasilnya dengan tipe terkait `Output`.** Seperti di CL, `/` pada dua bilangan bulat
adalah `rational`, sehingga `Output` milik `int` adalah `rational`, dan milik tipe numerik lain adalah
tipe itu sendiri. Karena `Output` berbeda antar tipe, `Number` tidak menyertakan `Div` (daftar supertrait
harus menetapkan setiap tipe terkait). Fungsi generik yang membagi menerima tipe hasil dengan
`(:where (Div T (Output R)))`, atau meminta pembagian yang tertutup dalam `T` dengan
`(:where (Div T (Output T)))` (maka `int` tidak dapat diteruskan).

## 4. Trait `Push` (menambahkan elemen)

```lisp
(deftrait Push ()
  (:type Item)
  (push (self (x Item)) ()))
```

Koleksi yang dapat ditambahi elemen satu per satu. Fungsi generik meminta "tipe yang dapat ditambahi
`T`" dengan `(:where (Push C (Item T)))`, lalu menambahkan ke koleksi mana pun dengan `(push c x)`
yang sama.

| Tipe | Yang dilakukan `push` |
|---|---|
| `Vector<T>` | Menambahkan di akhir |
| `Deque<T>` | Menambahkan di akhir (`push-back`) |
| `Array<T>` | Menambahkan di posisi fill pointer (`push-extend`). Panic tanpa fill pointer |
| `HashSet<T>` | Memasukkannya ke himpunan (`insert`). Merupakan `Push` hanya jika tipe elemennya mengimplementasikan `Hash` |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

Tipe buatan sendiri yang mengimplementasikan `Push` dapat diberikan ke fungsi generik yang sama.
Tipe itu juga dapat menjadi objek trait, seperti pada `:dyn Push<int>`.

## 5. Trait `Pop` (mengambil elemen)

```lisp
(deftrait Pop ()
  (:type Item)
  (pop (self) Option<Item>))
```

Koleksi yang elemen terakhir yang ditambahkan `push` dapat diambil kembali darinya. `none` jika
kosong. Fungsi generik memintanya dengan `(:where (Pop C (Item T)))`.

| Tipe | Yang dilakukan `pop` |
|---|---|
| `Vector<T>` | Mengambil elemen terakhir |
| `Deque<T>` | Mengambil elemen terakhir (`pop-back`) |
| `Array<T>` | Mengambil elemen tepat sebelum fill pointer. Panic jika tanpa fill pointer |

`HashSet<T>` tidak punya "elemen terakhir", jadi ia `Push` tetapi bukan `Pop`.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

Seperti `Push`, tipe Anda sendiri dapat mengimplementasikannya, dan ia dapat menjadi objek trait,
seperti `:dyn Pop<int>`.
