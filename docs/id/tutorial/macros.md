<!-- translated-from: docs/ja/tutorial/macros.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Makro

Makro adalah fungsi yang menerima program dan mengembalikan program. Dengan makro Anda dapat membuat
sintaks baru yang tidak dapat dinyatakan oleh fungsi. Makro typelisp bekerja sama seperti `defmacro`
pada Common Lisp. Bab ini mengasumsikan Anda sudah membaca "Daftar (S-expression)" di
[Memulai](intro.md).

## 1. Perbedaan makro dengan fungsi

Fungsi menerima argumennya **setelah dievaluasi**. Makro menerimanya **sebagai ekspresi, sebelum
dievaluasi** (sebagai data S-expression), menyusun ekspresi lain, dan mengembalikannya. Ekspresi yang
dikembalikan menggantikan pemanggilan makro, dan barulah setelah itu diperiksa tipenya dan
dijalankan. Penggantian ini disebut **ekspansi**.

Misalnya, sintaks seperti `unless` tidak dapat ditulis sebagai fungsi. Sebagai fungsi, badannya akan
dievaluasi lebih dulu bahkan ketika kondisinya benar.

## 2. `defmacro` dan quasiquote

Mari buat `my-unless`, yang menjalankan badannya hanya ketika kondisinya salah.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Argumen makro tidak ditulis tipenya. Setiap argumen adalah data S-expression.
- `&rest body` menerima argumen sisanya bersama-sama sebagai satu daftar.
- Ekspresi yang diawali `` ` `` (quasiquote) disusun sebagai data, sebagaimana ditulis. Di dalamnya:
  - `,test` menyisipkan isi variabel `test` pada posisi itu.
  - `,@body` menyambungkan elemen daftar `body` pada posisi itu.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Anda dapat memeriksa ekspansinya dengan `macroexpand-1`. Saat menulis makro, melihat ekspansinya
lebih dulu adalah cara tercepat untuk maju.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Ekspansi juga diperiksa tipenya

Ekspresi yang dikembalikan makro diperiksa tipenya seperti ekspresi apa pun yang Anda tulis
sendiri.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

Kesalahan dilaporkan di tempat makro dipanggil.

Aturan bahwa cabang else pada `if` tidak boleh dihilangkan, dan bahwa kedua cabang `if` harus
bertipe sama, berlaku pada ekspansi apa adanya. `my-unless` di atas diakhiri dengan
`(progn ,@body ())` agar, apa pun tipe ekspresi terakhir badan, kedua cabang `if` bertipe `()`.

## 4. Benturan nama dan `gensym`

Makro sederhana yang menukar nilai dua variabel terlihat seperti ini:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Makro ini bekerja hampir selalu, tetapi rusak ketika variabel pemanggil kebetulan bernama `tmp`.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (not swapped)
```

Ekspansinya adalah `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, dan `tmp` yang dibuat
makro menyembunyikan `tmp` milik pemanggil.

Untuk menghindarinya, buat nama variabel yang dipakai di dalam makro dengan `gensym`. `gensym`
mengembalikan simbol baru yang tidak mungkin ditulis di bagian program mana pun.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Seperti pada Common Lisp, makro typelisp tidak mencegah benturan nama secara otomatis (tidak
higienis). Ingatlah: **gunakan `gensym` untuk pengikatan yang dibuat oleh makro.**

Dengan cara yang sama, makro yang mengulang badannya sebanyak jumlah tertentu dapat ditulis seperti
ini:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Ekspansi yang berbeda sesuai argumen

Badan makro adalah kode typelisp biasa, sehingga dapat memeriksa argumennya dengan `if` atau `match`
dan menyusun ekspansi yang berbeda. Argumennya adalah data S-expression (`Option<Sexpr>`), dan daftar
kosong adalah `none`.

Mari buat `my-and`, yang mengembalikan `true` jika semua kondisinya benar.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; no arguments
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; just one
         `(if ,f (my-and ,@more) false)))             ; two or more
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` adalah pola yang mengambil kepala daftar ke `f` dan sisanya ke `more`.
- `sexpr-null` menguji apakah data S-expression adalah daftar kosong.
- Cabang `_` terakhir diperlukan karena data S-expression memiliki bentuk selain daftar (bilangan,
  string, dan sebagainya), dan `match` mensyaratkan bentuk-bentuk itu tercakup juga. Argumen `&rest`
  selalu berupa daftar, sehingga cabang ini sebenarnya tidak pernah dijalankan.
- Makro dapat memanggil dirinya sendiri dalam ekspansinya. Ekspansi berulang sampai tidak ada
  pemanggilan makro yang tersisa.

## 6. Argumen opsional

`&optional` menerima argumen yang boleh dihilangkan. Nilai bawaan dapat diberikan.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` menerima argumen keyword
([Referensi Sintaks 3.14](../reference/syntax.md#314-defmacro--definisi-makro)).

## 7. `macrolet`: makro untuk satu tempat saja

Makro yang hanya dipakai di dalam satu ekspresi dapat didefinisikan dengan `macrolet`. Makro itu
tidak terlihat di luar.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Hal yang perlu diingat

- **Makro hanya dapat dipanggil setelah definisinya.** Seperti fungsi, definisikan di dekat awal
  berkas.
- Buat makro tersedia untuk modul lain dengan `(pub defmacro ...)`.
- Banyak sintaks standar, termasuk `when`, `unless`, `cond`, `and`, `or`, dan `dotimes`, didefinisikan
  sebagai makro. Anda dapat melihat isinya dengan `(macroexpand '(when true 1))`.
- Jika sesuatu dapat ditulis sebagai fungsi, tulislah sebagai fungsi. Makro tidak dapat diserahkan
  sebagai nilai, dan Anda harus membaca ekspansinya untuk memahami apa yang dilakukannya.

## 9. Yang dibaca selanjutnya

- [Penanganan Kesalahan](errors.md): `Result`, `panic`, `catch` / `throw`
- [Fungsi makro](../reference/functions/system.md#8-makro): `gensym`, `macroexpand`, dan lainnya
