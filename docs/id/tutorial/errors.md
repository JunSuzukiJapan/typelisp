<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Penanganan Kesalahan

Penanganan kesalahan di typelisp membagi kegagalan menjadi dua jenis.

| Jenis kegagalan | Contoh | Cara menyatakannya |
|---|---|---|
| Kegagalan yang dapat terjadi (dapat dipulihkan) | Berkas tidak ada, masukan bukan bilangan | Mengembalikan `Result<T,E>` |
| Kesalahan dalam program (tidak dapat dipulihkan) | Indeks di luar rentang, `unwrap` pada `none`, pembagian dengan nol | Berhenti dengan `panic` |

Selain itu ada `catch` / `throw`, yang keluar dari banyak pemanggilan fungsi sekaligus, dan
`unwind-protect`, yang menjalankan pembersihan bagaimanapun badannya ditinggalkan. Bab ini
mengasumsikan Anda sudah membaca bagian `Result` di [Dasar-dasar Tipe](types.md).

## 1. Mengembalikan `Result` dan menerimanya dengan `match`

Berikut fungsi yang membaca nomor port dari sebuah string. Fungsi ini dapat gagal dengan dua cara:
masukan bukan bilangan, atau di luar rentang.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Pemanggil memisahkan keberhasilan dan kegagalan dengan `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- Nilai fungsi yang mengembalikan `Result` tidak dapat dipakai kecuali `match` menangani kasus `err`.
  Lupa menangani kegagalan adalah kesalahan tipe.
- Kesalahan dari `parse-int` adalah nilai bertipe `ParseIntError`. `(message e)` memberikan string
  pesannya.

## 2. Meneruskan kegagalan ke pemanggil

Tidak ada pintasan seperti `?` pada Rust. Ketika memanggil beberapa fungsi yang mengembalikan
`Result` secara berurutan, tulis bagian "kembalikan kegagalan apa adanya" dengan `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Ketika Anda tahu operasinya tidak mungkin gagal, atau pada skrip kecil yang berhenti saat gagal
tidak masalah, `unwrap` mengambil isinya. Jika nilainya `err`, ia melakukan panic. Jika nilai bawaan
sudah cukup, gunakan `unwrap-or`.

## 3. Membuat tipe kesalahan sendiri

Menyatakan kesalahan sebagai tipe dan bukan string memungkinkan pemanggil bercabang berdasarkan
jenis kesalahan. Tipe kesalahan adalah `defenum` atau `defstruct` biasa yang mengimplementasikan
trait `Error`.

```lisp
(defenum config-error
  (missing string)          ; a setting is missing
  (invalid string int))     ; a value is wrong

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` mengembalikan deskripsi kesalahan.
- `source` mengembalikan kesalahan lain yang menyebabkan kesalahan ini. Jika tanpa penyebab, nilainya
  `none`.

## 4. Menggabungkan berbagai jenis kesalahan

Jika satu fungsi memanggil `parse-int` (`ParseIntError`) dan `check-workers` (`config-error`), ada
dua tipe kesalahan, dan keduanya tidak dapat menjadi `E` pada satu `Result<T,E>`. Dalam hal itu,
jadikan `E` sebagai `:dyn Error` (kesalahan bertipe apa pun yang mengimplementasikan `Error`).
Konversikan setiap kesalahan dengan `as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Dengan masukan `"4"`, `"-1"`, dan `"abc"`, hasilnya:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Untuk `:dyn`, lihat bagian 5 pada [Trait](traits.md).

## 5. `panic`: kesalahan dalam program

Ketika program mencapai keadaan yang tidak boleh terjadi, hentikan dengan `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Tipe `panic` adalah `!` (tidak kembali), sehingga dapat ditulis di mana pun tipe apa pun
  diharapkan. Itulah sebabnya kedua cabang `if` di atas cocok.
- Operasi berikut juga melakukan panic: `unwrap` pada `none` atau `err`, `get` dengan indeks di luar
  rentang, dan pembagian bilangan bulat dengan nol.
- `panic` menghentikan program. Bahkan ketika terjadi di dalam task, seluruh program berhenti.
- Di REPL, `panic` tidak mengakhiri REPL; REPL menunggu masukan berikutnya.
- Anda dapat menulis `(todo)` untuk "belum ditulis" dan `(unreachable)` untuk "titik ini tidak
  mungkin tercapai". Keduanya melakukan panic.

`panic` bukan pengganti `Result`. Untuk kegagalan yang dapat terjadi, seperti masukan pengguna atau
ada tidaknya sebuah berkas, gunakan `Result`.

## 6. `catch` / `throw`: melompat keluar melintasi fungsi

`throw` melompat langsung keluar ke `catch` terdekat yang bertag sama, berapa pun banyaknya
pemanggilan fungsi di antaranya.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

Jika `v` tidak memiliki bilangan negatif, `validate` mengembalikan `"all fine"`; jika memuat `-7`,
kendali melompat dari dalam `check-all` ke `catch`, yang mengembalikan `"negative: -7"`.

- Tulis tag sebagai simbol polos, seperti `'bad-input`.
- **Setiap tag membawa nilai dengan tepat satu tipe.** Pada contoh di atas `'bad-input` membawa
  `string`, sehingga melempar `int` dengan tag yang sama adalah kesalahan tipe. Tipe badan `catch`
  juga harus sama dengan tipe tag.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- `throw` tanpa `catch` bertag sama untuk dituju adalah kesalahan.

Jika Anda hanya ingin kembali lebih awal dari dalam sebuah fungsi, gunakan `return-from` sebagai
ganti `catch` / `throw`. `return-from` tidak dapat melintasi fungsi, tetapi sebagai gantinya Anda
dapat mengetahui ke mana ia kembali dengan membaca kode sumber.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: selalu bersihkan

`(unwind-protect body cleanup)` menjalankan pembersihan bagaimanapun badannya ditinggalkan: ketika
selesai secara normal, ketika ditinggalkan oleh `throw`, dan ketika terjadi panic.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Gunakan untuk hal seperti "selalu tutup berkas yang dibuka" atau "selalu lepaskan kunci yang
diambil". `with-open-file` dan `with-lock` pada pustaka standar memakai `unwind-protect` secara
internal.

## 8. Tentang sistem kondisi Common Lisp

typelisp tidak mengadopsi sistem kondisi Common Lisp (`handler-case`, `restart-case`, dan
sebagainya). Sistem itu tidak menunjukkan dalam tipe kegagalan apa saja yang dapat ditimbulkan
sebuah fungsi, sehingga kurang cocok dengan tipe statis. Kegagalan yang dapat terjadi ditulis dalam
tipe dengan `Result`, dan pemindahan kendali dilakukan dengan `catch` / `throw`.

## 9. Yang dibaca selanjutnya

- [Konkurensi](concurrency.md): task dan kanal
- [Option, Result, dan Tipe Kesalahan](../reference/functions/option-result.md): daftar fungsi
- [Pesan Kesalahan](../reference/errors.md): arti kesalahan yang umum dan cara memperbaikinya
