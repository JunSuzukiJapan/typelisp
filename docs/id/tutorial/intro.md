<!-- translated-from: docs/ja/tutorial/intro.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Memulai

Dimulai dari mengevaluasi ekspresi di REPL, bab ini membahas fungsi, variabel, percabangan,
perulangan, serta daftar dan `Vector`, secara berurutan. Untuk cara membangun `typl`, lihat
[README.md](../../../README.md).

## 1. Memulai REPL

Jika dijalankan tanpa argumen, `typl` masuk ke REPL (mode interaktif). Ketik ekspresi setelah
`typl>` dan ekspresi itu dievaluasi saat itu juga lalu nilainya dicetak. `:quit` keluar dari REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Mulai dari sini, masukan dan hasil REPL ditampilkan dalam bentuk ini.

## 2. Mengevaluasi ekspresi

typelisp adalah Lisp, jadi sebuah ekspresi dibungkus dengan tanda kurung dengan **operator atau nama
fungsi di depan**. Anda menulis `(+ 1 2)`, bukan `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Bilangan terdiri dari jenis berikut:

- **Bilangan bulat** bertipe `int`. Tidak ada batas atas ukurannya.
- **Bilangan desimal** bertipe `f64`. Tulis dengan titik desimal, seperti `1.5` atau `2.0`.
- Anda tidak dapat mencampur `int` dan `f64` dalam satu perhitungan. `(+ 1 2.0)` adalah kesalahan
  tipe. Untuk mengonversi, tulis `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` pada dua bilangan bulat menghasilkan bilangan bulat dengan bagian pecahan dibuang (tidak
menghasilkan pecahan seperti pada Common Lisp). Gunakan `(mod 7 2)` untuk sisa bagi.

Nilai boolean adalah `true` dan `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Mendefinisikan fungsi

Fungsi didefinisikan dengan `defun`. **Tipe argumen dan tipe nilai kembalian selalu ditulis.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` berarti "argumen `n` bertipe `int`". Dengan beberapa argumen, daftarkan semuanya:
  `((a int) (b int))`.
- `int` setelah daftar argumen adalah tipe nilai kembalian.
- Nilai ekspresi terakhir pada badan adalah nilai kembalian fungsi. Anda tidak menulis `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Pemanggilan yang tipenya tidak cocok dilaporkan sebagai kesalahan tipe **sebelum dijalankan**. Saat
menjalankan berkas, satu kesalahan tipe di mana pun berarti tidak satu baris pun program dijalankan.

Untuk membuat argumen opsional, gunakan `&optional`. Jika Anda memberikan nilai bawaan, argumen
mengambil nilai itu saat dihilangkan.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

`false` yang diberikan sebagai argumen pertama `format` berarti "kembalikan hasilnya sebagai string,
bukan mencetaknya". Setiap `~a` diganti dengan argumen berikutnya.

## 4. Variabel

Variabel lokal dibuat dengan `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- Tipe variabel `let` diambil dari nilai awalnya. Anda tidak perlu menuliskannya.
- Variabel dalam satu `let` tidak dapat saling merujuk. Untuk membangun satu variabel dari variabel
  sebelumnya, gunakan `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Untuk mengubah nilai variabel, gunakan `setf`. **Penugasan tidak dapat mengubah tipe variabel.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Variabel global didefinisikan dengan `defvar`. Di sini tipenya memang ditulis.

```lisp
(defvar (counter int) 0)
```

## 5. Percabangan

### if

Tulis `(if kondisi ekspresi-then ekspresi-else)`. **Ekspresi else tidak boleh dihilangkan.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Hanya ekspresi bertipe `bool` yang dapat menjadi kondisi. Menulis bilangan, seperti `(if 0 ...)`,
  adalah kesalahan tipe.
- Ekspresi then dan else harus bertipe sama.

Jika tidak ada yang perlu dilakukan pada kasus salah, gunakan `when` (dan `unless` untuk
kebalikannya).

```lisp
(when (> n 100)
  (println "large")
  (println "really large"))
```

### cond

Dengan tiga kondisi atau lebih, `cond` lebih mudah dibaca. `else` terakhir dipakai ketika tidak ada
kondisi yang terpenuhi.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Untuk bercabang berdasarkan bentuk suatu nilai, gunakan `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` cocok dengan nilai apa pun. Karena `int` memiliki nilai yang tak terhitung banyaknya,
menghilangkan cabang `_` adalah kesalahan yang menyatakan bahwa tidak semua kasus tercakup. Kekuatan
sebenarnya `match` ada pada membongkar `Option` dan tipe yang Anda definisikan sendiri, yang muncul
di bab berikutnya, [Dasar-dasar Tipe](types.md).

## 6. Perulangan

Sebuah fungsi dapat memanggil dirinya sendiri.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Untuk jumlah pengulangan tetap, gunakan `dotimes`. `i` berjalan dari 0 sampai `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

Ada juga `while`, `do`, dan `loop` yang diperluas dari Common Lisp. Kata klausa pada `loop` yang
diperluas ditulis sebagai keyword (`:for`, `:collect`, dan sebagainya).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. Daftar dan Vector

### Vector

Untuk menyimpan urutan nilai bertipe sama, gunakan `Vector<T>`. `T` adalah tipe elemen.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; prints #<vector<int> 3 1 2>
```

- `(Vector::new)` saja tidak menentukan tipe elemen, jadi berikan tipenya dengan
  `(the Vector<int> ...)`.
- `(push v x)` menambahkan di akhir, `(get v i)` membaca elemen `i`, dan `(len v)` memberikan
  panjangnya.
- `get` dengan indeks di luar rentang menghentikan program dengan kesalahan.

### lambda dan fungsi tingkat tinggi

Fungsi anonim dibuat dengan `lambda`. Seperti pada `defun`, Anda menulis tipe argumen dan nilai
kembalian.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl`, dan sejenisnya menerima `Vector` yang diubah menjadi
**iterator** dengan `(iter v)`. Koleksi didahulukan dan fungsi menyusul. `v` di atas diikat dengan
`let`, jadi tidak bisa dipakai di luar `let` tersebut. Contoh berikut terlebih dahulu
mendefinisikan `v` dengan `defvar`.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Untuk memproses elemen satu per satu, gunakan `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Fungsi yang menerima fungsi sebagai argumen menulis tipe argumen itu sebagai
`(fn (tipe-argumen...) tipe-kembalian)`. Fungsi yang didefinisikan dengan `defun` dapat diserahkan
dengan namanya sebagai nilai.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Daftar (S-expression)

Daftar yang dibuat dengan `'(1 2 3)` atau `(list 1 2 3)` adalah **data S-expression**. Elemennya
tidak harus bertipe sama.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

Data S-expression terutama untuk menangani program itu sendiri, pada makro ([Makro](macros.md)) dan
dengan `read`. Untuk data yang tipe elemennya diketahui, gunakan `Vector<T>`. Daftar S-expression
dapat ditelusuri dengan `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Pasangan dua nilai dibuat dengan `cons` dan dibongkar dengan `car` dan `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Menulis program dalam berkas

Program dapat ditulis dalam berkas (berekstensi `.typl`) dan dijalankan dengan
`typl nama-berkas`. Gunakan `println` untuk menampilkan hasil.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` mencetak dengan direktif yang sama seperti `format` dan diakhiri baris baru. `print`
  tidak menambahkan baris baru.
- `~a` menyisipkan nilai dalam bentuk yang mudah dibaca manusia, dan `~s` dalam bentuk yang dapat
  dibaca kembali (string mendapatkan tanda `"`).
- Berkas dibaca dari atas ke bawah. **Fungsi tidak dapat dipanggil sebelum definisinya.**

## 9. Yang dibaca selanjutnya

- [Dasar-dasar Tipe](types.md): `Option`, `Result`, struct, enum, generik
- [Untuk Pemrogram Common Lisp](../guide/from-common-lisp.md): daftar perbedaan bagi orang yang
  mengenal Common Lisp
