<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
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
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` mengimplementasikan `Iter` melalui `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (dapatkan iteratornya dengan `(iter collection)`).
`Chan<T>` adalah `Iter` itu sendiri (`recv` berperan sebagai `next`; [Kanal](concurrency.md#2-chant--kanal)).
Daftar `Sexpr` tidak mengimplementasikan `Iter` (tipe elemennya tidak seragam). Jika Anda
mengimplementasikan `Iter` untuk tipe Anda sendiri, tipe itu dapat ditelusuri dengan `doiter` apa
adanya, dan diserahkan ke [fungsi sekuens](sequences.md#4-fungsi-sekuens-pada-iter).

## 2. `Equal` / `Ord` (perbandingan)

`Equal` adalah trait yang dipakai `equal` dan setara dengan `PartialEq` di Rust. `Ord` setara dengan
`PartialOrd` di Rust. Keduanya dipakai di batasan `:where` fungsi generik untuk mensyaratkan bahwa tipe
elemen dapat dibandingkan.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; wajib diimplementasikan
(deftrait Ord (Equal)                                     ; mewarisi Equal
  (defmethod less (self (other Self)) bool)                  ; wajib diimplementasikan
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

Untuk mengimplementasikan `Equal` tulis `equal`, dan untuk `Ord` cukup `less`. Implementasi bawaan
melengkapi sisa `Ord`. `Ord` mewarisi `Equal`, jadi `impl Equal X` diperlukan sebelum `impl Ord X`.

`(equal a b)` memanggil implementasi `Equal` dari tipe `a`. Menuliskannya untuk tipe yang tidak
mengimplementasikan `Equal` adalah galat tipe. Untuk membandingkan berdasarkan identitas gunakan
`eq`/`eql`, dan berdasarkan struktur gunakan `equalp`. Ketiganya dapat dipakai pada nilai bertipe apa pun.

Setiap metode trait dapat dipanggil langsung sebagai fungsi (di dalam batasan `where (Equal A)`/`(Ord A)`,
atau pada tipe konkret yang mengimplementasikannya):

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Apakah keduanya sama (`==` di Rust) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` diimplementasikan untuk: semua tipe numerik (`i8` sampai `u32` / `f32` / `f64` / `int` / `ratio`),
`bool` `char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` dan tuple (per elemen, bila elemennya
`Equal`), serta `Option<T>` dan `Result<T,E>` (bila variannya sama dan isinya `equal`). Untuk tipe bawaan,
metode bawaan `equal` milik tipe itu sendiri apa adanya adalah implementasi `Equal`. Bilangan dibandingkan
dengan `=`, string berdasarkan isi dengan membedakan huruf besar dan kecil. `Ord` diimplementasikan untuk:
semua tipe numerik, `char` `string`, dan `cons-cell<A,B>` (secara leksikografis, bila elemennya `Ord`).

`equal` sebuah list (`Sexpr`) adalah `equal` CL: cons dibandingkan berdasarkan struktur, string berdasarkan
isi, dan atom lainnya dengan `eql`. Perbandingan ini menelusuri nilai saat runtime dan tidak memanggil
metode apa pun. Jadi struct di dalam list dibandingkan berdasarkan identitas, seperti di CL, apa pun
`impl Equal` tipenya.

Nama metode `Ord` tidak bertabrakan dengan operator bawaan (`< <= > >=`) atau `lt` karena yang bawaan tidak
dapat didefinisikan ulang dan setiap implementasi mendelegasikan kepadanya. Operator perbandingan skalar
sendiri adalah metode bawaan per tipe penerima ([Bilangan](numbers.md), [String dan karakter](collections.md)).
Di dalam batasan, operator yang ditulis dibaca sebagai metode trait (bab 3). `=` menjadi `equal`, dan `/=`
menjadi `(not (equal a b))`.

## 3. Trait aritmetika (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Lapisan agar kode generik dapat mensyaratkan "tipe yang dapat dijumlahkan". **Aritmetika pada tipe
konkret memakai operator bawaan** ([Bilangan](numbers.md)) dan tidak melalui lapisan ini.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; tanpa metode; gabungan lima
```

**Di dalam batas, Anda dapat menulis operator.** Ketika penerimanya adalah variabel tipe yang
dibatasi oleh `:where`, operator dibaca sebagai metode trait (`+`→`add`, `-`→`sub`, `*`→`mul`,
`/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

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
  (defmethod push (self (x Item)) ()))
```

Koleksi yang dapat ditambahi elemen satu per satu. Fungsi generik meminta "tipe yang dapat ditambahi
`T`" dengan `(:where (Push C (Item T)))`, lalu menambahkan ke koleksi mana pun dengan `(push c x)`
yang sama.

| Tipe | Yang dilakukan `push` |
|---|---|
| `Vector<T>` | Menambahkan di akhir |
| `Deque<T>` | Menambahkan di akhir (`push-back`) |
| `Array<T>` | Menambahkan di posisi fill pointer (`push-extend`). Panic tanpa fill pointer |
| `HashSet<T>` | Memasukkannya ke himpunan (`insert`) |

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
  (defmethod pop (self) Option<Item>))
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
