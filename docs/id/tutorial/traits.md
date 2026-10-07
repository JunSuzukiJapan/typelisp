<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Trait

Trait adalah janji bahwa "tipe ini mendukung operasi-operasi ini". Dengan trait, beberapa tipe dapat
berbagi operasi bernama sama, sehingga fungsi yang memakainya tidak perlu ditulis sekali untuk
setiap tipe. Cara kerjanya hampir persis seperti trait pada Rust. Bab ini mengasumsikan Anda sudah
membaca [Dasar-dasar Tipe](types.md).

## 1. Mendefinisikan dan mengimplementasikan trait

Definisikan operasi yang mengembalikan luas dan nama sebuah bentuk sebagai trait `Shape`.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- `()` setelah nama trait adalah daftar trait yang diwarisinya (bagian 4). Biarkan kosong jika tidak
  ada.
- Setiap baris mendeklarasikan sebuah metode. `Self` mewakili "tipe yang mengimplementasikan trait
  ini".

Untuk mengimplementasikan trait pada sebuah tipe, tulis `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

Metode yang diimplementasikan dipanggil sama seperti fungsi biasa.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Menghilangkan satu saja metode yang dideklarasikan trait adalah kesalahan tipe pada `impl`.

## 2. Batas trait: "tipe apa pun yang mengimplementasikan trait ini"

Anda dapat memberi syarat pada parameter tipe fungsi generik dengan `where`. Ini disebut **batas
trait**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Berkat `(where (Shape T))`, badan fungsi dapat memakai `name` dan `area` pada nilai bertipe `T`. Tanpa
batas itu tidak ada yang diketahui tentang `T`, sehingga keduanya tidak dapat dipanggil.

Memberikan tipe yang tidak mengimplementasikan `Shape` adalah kesalahan tipe pada pemanggilan.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Fungsi generik mendapatkan salinannya sendiri untuk setiap tipe yang dipakai memanggilnya. Tidak ada
pengujian tipe atau percabangan saat dijalankan yang terlibat.

## 3. Implementasi bawaan

Jika sebuah metode trait memiliki badan, badan itu dipakai ketika `impl` menghilangkan metodenya.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe is the default one

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; the one written here takes priority

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Mengimplementasikan trait standar

Pustaka standar juga memiliki trait. Mengimplementasikannya membuat fungsi standar yang memakainya
tersedia untuk tipe Anda.

| Trait | Metode yang diimplementasikan | Yang diaktifkan |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, pola `(= expr)` pada `match`, dan sebagainya |
| `Ord` | `less` | `less-equal`, `greater`, dan sebagainya. `Ord` mewarisi `Eq` |
| `print-object` | `print-object` | Cara nilai ditampilkan oleh `println` dan sejenisnya |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort`, dan sebagainya |
| `Error` | `message`, `source` | Pemakaian sebagai tipe kesalahan ([Penanganan Kesalahan](errors.md)) |

Mari implementasikan `Eq` dan `Ord` untuk tipe yang mewakili sejumlah uang. Karena `Ord` mewarisi
`Eq`, `impl` untuk `Eq` harus ditulis lebih dulu.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (the default implementation in Ord)
```

Mengimplementasikan `print-object` menentukan cara `println` menampilkan nilai. Argumen `escape`
bernilai `true` ketika bentuk yang dapat dibaca kembali diminta, seperti pada `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Dikombinasikan dengan batas trait, Anda dapat menulis fungsi yang bekerja untuk tipe apa pun yang
mengimplementasikan `Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Diberi `Vector` berisi nilai `money` 300, 900, dan 100 berurutan, fungsi ini mengembalikan
`(some 900 yen)`.

## 5. `:dyn`: menangani nilai bertipe berbeda bersama-sama

Semua elemen `Vector<T>` bertipe sama, sehingga nilai `circle` dan `rect` tidak dapat dimasukkan ke
satu `Vector<circle>`. Untuk menangani "sesuatu yang mengimplementasikan `Shape`" bersama-sama,
gunakan tipe `:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Nilai `circle` atau `rect` yang ditaruh di tempat yang mengharapkan `:dyn Shape` dikonversi secara
  otomatis.
- `area` milik tipe mana yang dijalankan oleh pemanggilan `(area s)` ditentukan saat dijalankan
  berdasarkan tipe yang disimpan `s`.
- Menaruh nilai yang tipenya tidak mengimplementasikan `Shape` di tempat yang mengharapkan
  `:dyn Shape` adalah kesalahan tipe.

Memilih antara batas trait pada bagian 2 dan `:dyn`:

| | Batas trait (`where`) | `:dyn Trait` |
|---|---|---|
| Kapan metode yang dipanggil ditentukan | Sebelum dijalankan | Saat dijalankan |
| Mencampur tipe dalam satu `Vector` | Tidak dapat | Dapat |
| Tipe yang dapat dipakai | Tanpa batasan | Struct, enum, `int`, `string`, `f64`, dan lainnya (bukan `bool`, `char`, `symbol`, `i32`, dan sejenisnya) |

Daftar persis tipe yang dapat dipakai ada di
[Referensi Sintaks 3.9](../reference/syntax.md#39-deftrait--impl--trait).

Beberapa trait tidak dapat dipakai dengan `:dyn`: yang metodenya memakai `Self` untuk argumen selain
`self` atau untuk nilai kembalian (seperti `equals` pada `Eq`). Karena tipenya tidak diketahui sampai
saat dijalankan, tidak ada cara menghasilkan "nilai bertipe sama".

## 6. Batasan

- Simpan definisi trait, `impl` untuknya, dan kode yang memakainya melalui `:dyn` dalam satu modul
  (berkas). Trait belum dapat dibuat terlihat oleh modul lain.
- Tipe dan trait berbagi satu ruang nama. Dalam satu modul, sebuah tipe dan sebuah trait tidak dapat
  bernama sama.

## 7. Yang dibaca selanjutnya

- [Makro](macros.md): mendefinisikan sintaks Anda sendiri
- [Referensi Sintaks 3.9](../reference/syntax.md#39-deftrait--impl--trait): implementasi blanket,
  tipe terkait, dan lainnya
- [Trait Standar](../reference/functions/traits.md): daftar trait di pustaka standar
