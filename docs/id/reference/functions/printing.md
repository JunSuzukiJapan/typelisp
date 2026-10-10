<!-- translated-from: docs/ja/reference/functions/printing.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Pencetakan

`print`/`println`/`format`, pencetak satu argumen, pretty printer, `print-object`, dan variabel yang
mengendalikan pencetakan. Daftar direktif format ada di [format.md](format.md). Membaca dari dan
menulis ke stream ada di [Stream dan Berkas](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` semuanya adalah **bentuk khusus yang menginterpretasikan direktif format
(direktif `format` pada CL)**. Argumen pertama (kedua untuk `format`) adalah **string kendali**, dan
setiap direktif mengonsumsi argumen variadik berikutnya secara berurutan.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Mengekspansi string kendali dan menulisnya ke keluaran standar tanpa baris baru |
| `println` | `(println control args...)` | `(string, ...)→Unit` | Sama, dengan baris baru di akhir |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | `format` pada CL. Mengembalikan string hasil ekspansi. Jika `dest` adalah `true` (`t` pada CL), string juga ditulis ke keluaran standar; jika `false` (`nil` pada CL), tidak ditulis dan hanya dikembalikan |
| `format` (ke stream) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | Jika `dest` bukan `bool`, ia adalah tujuan stream pada CL. String hasil ekspansi ditulis ke stream itu. Nilai kembaliannya `()` (`nil` pada CL), dan tidak ada string yang dikembalikan |

Tipe `dest` membagi maknanya menjadi dua (yang mana yang berlaku ditentukan secara statis). Bentuk
stream dapat ditulis dengan cara yang sama dengan tipe stream konkret, `:dyn CharOutput`, atau
variabel tipe yang dibatasi oleh `(where (CharOutput S))`. `dest` yang bukan `bool` maupun stream
adalah kesalahan tipe.

**String kendali harus berupa literal** (batasan yang sama seperti `format!` pada Rust). Direktif di
dalamnya menentukan berapa banyak argumen yang diambil dan bertipe apa, sehingga string yang dibangun
saat dijalankan tidak dapat dibaca saat pemeriksaan. Karena harus berupa literal, **jumlah dan tipe
argumen diperiksa saat pemeriksaan**: `(println "~d" "x")` dan `(println "~a ~a" 1)` adalah kesalahan
saat pemeriksaan. Direktif yang salah eja, `~(` yang tidak tertutup, dan `~/name/` yang tidak dapat
dijawab oleh argumen mana pun juga merupakan kesalahan saat pemeriksaan. Aturan pemeriksaan ada di
[format.md](format.md#1-cara-menulis-direktif). Untuk mencetak string yang Anda bangun, buatlah
dengan `(format false ...)` dan cetak dengan `(println "~a" s)`.

Argumen variadik dibungkus menjadi `Sexpr` dengan tipenya sendiri sebelum diserahkan:
`i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`, serta `defstruct`/`defenum`/`Vector<T>`/
`HashTable<K,V>` buatan pengguna dan sejenisnya, semuanya dapat diserahkan apa adanya
(`(println "~a" my-struct)` langsung bekerja).

Menjalankan skrip dengan `typl file.typl` **tidak mencetak nilai ekspresi tingkat atas**, sehingga
program menulis ke keluaran standar dengan memanggil fungsi-fungsi ini. `print`/`println`/`format`
mengirim keluarannya pada setiap pemanggilan (agar prompt terlihat sebelum masukan standar dibaca,
bahkan melalui pipe).

**`Option<Sexpr>` dicetak secara transparan.** Tipe data S-expression adalah `Option<Sexpr>`, sehingga
pembungkus `(some x)` tidak muncul pada keluaran dan isinya dicetak apa adanya. Daftar kosong dicetak
sebagai `()`. `Option<T>` lainnya dicetak sebagai `(some ...)` / `none`. Hal yang sama berlaku untuk
field `Option<T>` di dalam struct, enum, dan `Vector`. `Result<Option<Sexpr>,…>` dari `(eval ...)`
dicetak sebagai `(ok 42)`, atau `(ok ())` untuk `none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; get only the string, without printing
  (println "~a" s))                   ; => id=42
```

## 2. Pencetak satu argumen

Pencetak pada CLHS 22.1.3. Sebagai ganti mengekspansi format, pencetak ini mencetak satu nilai apa
adanya. Stream boleh dihilangkan (bawaannya `*standard-output*`).

| Nama | Bentuk | Deskripsi |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Menulis dalam bentuk yang dapat dibaca kembali (sama dengan `~s`) dan mengembalikan `x` |
| `princ` | `(princ x [stream])` | Menulis dalam bentuk untuk manusia (sama dengan `~a`) dan mengembalikan `x` |
| `write` | `(write x [stream])` | `prin1` jika `*print-escape*` benar, `princ` jika salah. Mengembalikan `x` |
| `prin1-to-string` | `(prin1-to-string x)` | Mengembalikan string alih-alih menulis (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | Sama (`~a`). Sama dengan `to-string` |
| `write-to-string` | `(write-to-string x)` | Sama, mengikuti `*print-escape*` |

`print`/`println` **bukan** bagian dari ini. Keduanya adalah singkatan `format` yang menerima string
kendali, pekerjaan yang berbeda dari `print` pada CL (baris baru, lalu `prin1`, lalu spasi), sehingga
masing-masing mempertahankan namanya sendiri. Akibatnya, **`print` satu argumen pada CL tidak
memiliki ejaan di bahasa ini**: tulis `prin1`.

Semuanya adalah makro, karena argumen variadik `format` tidak menerima variabel tipe dan tipenya
harus diketahui pada tempat pemanggilan.

## 3. Masukan standar dan stream standar

**Membaca masukan standar** dilakukan bukan dengan fungsi khusus melainkan dengan metode `CharInput`
pada stream standar `*standard-input*`: `(read-line *standard-input*)` /
`(read-char *standard-input*)` / `(read-all *standard-input*)`
([metode stream](streams-files.md#2-metode)). Keluaran standar dan kesalahan standar juga memiliki
`*standard-output*` / `*error-output*`, dan dapat ditulis seperti `(write-line *standard-output* s)`
(`print`/`println`/`format` adalah jalan pintas ketika Anda memerlukan ekspansi format, dan selalu
menulis ke keluaran standar).

## 4. Pretty printer

Ini sepadan dengan Lisp Pretty Printer pada CL (CLHS 22.2). **Ia memecah keluaran yang tidak muat
dalam lebar baris, mengikuti blok logis dan baris baru bersyarat.**

### 4.1 Variabel kendali

Variabel global yang dapat ditugasi. Setelah di-`setf`, variabel ini memengaruhi semua pencetakan
berikutnya. Untuk mengubahnya sementara, gunakan `dlet` (6.3).

| Variabel | Tipe | Bawaan | Arti |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Jika benar, `~a`/`~s`/`~w` dan direktif pretty mengambil jalur pretty-printing |
| `*print-right-margin*` | `int` | `80` | Margin kanan (dalam kolom). 0 berarti "tanpa margin, tidak pernah memecah". Nilai negatif adalah kesalahan cetak |
| `*print-miser-width*` | `int` | `0` | Lebar tempat gaya miser dimulai. 0 sepadan dengan `nil` pada CL (gaya miser mati). Nilai negatif adalah kesalahan cetak |

Keluarga `pprint` dan `pprint-logical-block` selalu melakukan pretty-printing terlepas dari
`*print-pretty*` (mengikuti definisi `pprint` pada CL).

### 4.2 Tata letak siap pakai (bentuk khusus)

Seperti `print`, ini adalah bentuk khusus, sehingga argumen dapat bertipe apa pun.

| Nama | Bentuk | Deskripsi |
|---|---|---|
| `pprint` | `(pprint x)` | Mencetak secara pretty dengan tata letak bawaan. Seperti di CL, ia **menulis baris baru lebih dulu** dan tidak ada di akhir |
| `pprint-fill` | `(pprint-fill x)` | Mengisi tiap baris sebanyak yang muat. Tidak menulis baris baru |
| `pprint-linear` | `(pprint-linear x)` | Jika tidak semua elemen muat pada satu baris, **satu elemen per baris**. Tidak menulis baris baru |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Tabel dengan kolom selebar `colinc` (bawaan 16). Tidak menulis baris baru. `colinc` negatif adalah kesalahan |

Tata letak bawaan (`pprint`, dan `~a` di bawah `*print-pretty*`) mengikuti `*print-pprint-dispatch*`
bawaan CL: ia menyingkat `(quote x)` menjadi `'x`, dan memformat bentuk kode seperti
`defun`/`let`/`if`/`lambda` sebagai "kepala dan jumlah argumen yang ditentukan pada baris pertama,
dan sisa badan diindentasi dua kolom, satu bentuk per baris". Daftar lain diisi.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Membangun blok logis sendiri

| Nama | Bentuk | Deskripsi |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Bentuk khusus yang membuka blok logis. `obj` adalah daftar yang ditelusuri `pprint-pop` (`()` jika tidak ada yang ditelusuri). `:prefix` dan `:per-line-prefix` saling eksklusif (seperti di CL) |
| `pprint-newline` | `(pprint-newline kind)` | Baris baru bersyarat. `kind` adalah `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Indentasi. `kind` adalah `:block` (dari awal blok) / `:current` (dari kolom saat ini) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Tab. `kind` adalah `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` dan `colinc` non-negatif (kesalahan jika negatif) |
| `pprint-pop` | `(pprint-pop)` | Mengambil elemen berikutnya dari daftar blok (`()` jika habis) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Apakah daftar sudah habis |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Jika sudah habis, melakukan `break` keluar dari `loop` yang melingkupinya (makro) |

Blok logis tidak menerima argumen stream: **blok logis yang terbuka adalah keadaan implisit**.
`pprint-logical-block` terluar memulainya, dan ketika ditutup, seluruhnya diformat dan ditulis ke
keluaran standar sekaligus. Selama terbuka, keluaran `print`/`println`/`(format true ...)`/`pprint`
semuanya masuk ke blok itu, sehingga **Anda menulis isinya dengan `print` biasa dan menandai hanya
tempat pemecahan dengan `pprint-newline` dan sejenisnya**, yang membuat kodenya tampak hampir sama
seperti di CL.

Di CL, `pprint-exit-if-list-exhausted` adalah keluar non-lokal dari `pprint-logical-block`; di sini
ia adalah **`break` dari `loop` yang melingkupinya** (`pprint-logical-block` tidak membuat `block`).
Idiom CL selalu menaruhnya di dalam `loop` bagaimanapun juga, sehingga terbaca sama.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Aturan baris baru bersyarat (CLHS `pprint-newline`):

- `:mandatory` selalu memecah.
- `:linear` memecah jika blok logis yang melingkupinya tidak muat pada satu baris. Keputusannya per
  blok, sehingga **semua baris baru `:linear` dalam satu blok memecah bersama-sama** (inilah "semua
  dalam satu baris atau satu elemen per baris" pada `pprint-linear`).
- `:fill` memecah jika (a) bagian berikutnya tidak muat di sisa baris, (b) bagian sebelumnya tidak
  muat pada satu baris, atau (c) pada gaya miser, blok tidak muat pada satu baris.
- `:miser` bekerja sebagai `:linear` hanya pada gaya miser (ketika blok dimulai dalam jarak
  `*print-miser-width*` dari margin kanan).

## 5. `print-object` (representasi cetak per tipe)

Menulis `impl print-object <type>` membuat `print`/`println`/`format`/`pprint` mencetak nilai bertipe
itu dengan implementasi itu, **bahkan ketika nilai itu bersarang di dalam daftar**. Ini sepadan
dengan fungsi generik `print-object` pada CL (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argumen | Arti |
|---|---|
| `self` | Nilai yang dicetak |
| `escape` | `*print-escape*` pada CL. `true` untuk `~s`/`prin1`/`pprint` (bentuk yang dapat dibaca kembali), `false` untuk `~a`/`princ` (untuk manusia). Implementasi yang tidak peduli boleh mengabaikannya |

`string` yang dikembalikan langsung masuk ke keluaran. Tipe tanpa `impl` dicetak dalam representasi
bawaan (berbentuk `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   works when nested too
```

Ia juga berpadu dengan pretty printer (bab 4). Jika `*print-pretty*` benar, daftar yang berisi string
yang dikembalikan implementasi dipecah pada margin kanan.

Representasi cetak tipe pada pustaka standar. Tipe yang juga ada di CL dicetak sama seperti di SBCL.
Ketika REPL menampilkan hasil, ia memakai representasi yang sama seperti `~s`.

| Tipe | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`, `#("a" "b")` | `#(1 2 3)`, `#(a b)` |
| Tuple `#{..}` | `#{1 "a"}` | `#{1 a}` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Sama |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (angkanya nomor seri internal) | Sama |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Bilangan bulat (nilai `get-universal-time` / `get-internal-real-time` pada CL) | Sama |
| Tipe kesalahan (`ParseIntError`, `SimpleError`, dan sebagainya) | `#<simpleerror "boom">` | Hanya pesannya (`boom`) |
| `complex` | `#C(1.0 2.0)` | Sama |
| `Array<T>` | `#2A((0 0) (0 0))` | Sama |
| Stream | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Sama |
| Socket | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Sama |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` di akhir saat waktu musim panas) | Sama |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Sama |
| Tipe `defstruct` | `#<point x: 1 y: 2>` (nama field dan nilai) | Sama (field dengan `~a`) |

Aturan:

- **Pendaftaran bersifat statis.** `impl` diperiksa tipenya sebagai definisi metode biasa, sehingga
  nama tipe yang salah eja atau signature yang salah adalah kesalahan kompilasi.
- **Ia bekerja untuk tipe generik juga.** `(impl print-object box<T> (where (print-object T)) ...)`
  menuju badan terpisah untuk setiap argumen tipe: sebuah nilai mengingat tipenya termasuk argumen
  tipenya (`box<i32>`). Tipe generik bawaan seperti `Vector<T>` bekerja dengan cara yang sama.
- **Pilihan dibuat saat pencetakan.** Direktif mana yang mengonsumsi argumen mana bergantung pada isi
  string kendali saat dijalankan, sehingga pembedaan antara `~a` dan `~s` (yaitu `escape`) baru
  diketahui pada saat pencetakan. Ini sama seperti CLOS, tempat metode `print-object` "didefinisikan
  per kelas dan dipilih saat pencetakan".
- **Masuk kembali kembali ke representasi bawaan.** Jika sebuah implementasi mencetak dirinya sendiri
  dengan `(format false "~a" self)`, ia akan berulang selamanya, sehingga ketika nilai yang sedang
  dicetak muncul lagi, representasi bawaan dipakai. Ini melihat identitas nilai, bukan batas
  kedalaman, sehingga tidak menghalangi pencetakan struktur bersarang yang merujuk dirinya sendiri
  secara sah.
- **Setiap tipe skalar mengimplementasikan trait ini.** Ini **agar dapat dipakai sebagai batas**:
  argumen variadik `format` tidak dapat menerima variabel tipe, sehingga batas ini satu-satunya cara
  kode generik menyatakan "nilai bertipe tak dikenal boleh dirender" (bentuk yang sama seperti
  `T: Display` pada Rust). `print-object` pada `Array<T>` adalah contohnya.
- **Dengan argumen tipe yang tidak memenuhi batas, representasi bawaan dipakai secara diam-diam.**
  `(impl print-object Array<T> (where (print-object T)))` berlaku untuk `Array<i32>`, tetapi tidak
  untuk `Array` yang elemennya `defstruct` tanpa `print-object`. Tidak masuk akal jika sekadar
  membuat larik menjadi kesalahan, sehingga ini bukan kesalahan.
- Mekanisme CL lainnya, `set-pprint-dispatch` / `*print-pprint-dispatch*` (registri saat dijalankan
  yang berkunci type specifier), **tidak diadopsi**. Pendaftarannya tidak diperiksa, yang tidak cocok
  dengan bahasa bertipe statis.

## 6. Mengendalikan seberapa banyak yang dicetak

### 6.1 Kedalaman, panjang, dan pembagian

Variabel kendali pada CLHS 22.1.1 yang menentukan "seberapa banyak nilai dicetak". Seperti tiga
variabel pada 4.1, semuanya adalah variabel global yang dapat ditugasi, dan berlaku untuk semua
`print`/`println`/`format`/`pprint`, baik `*print-pretty*` benar maupun tidak.

| Variabel | Tipe | Bawaan | Arti |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Objek yang bersarang pada kedalaman ini atau lebih dalam diganti dengan `#`. Objek yang sedang dicetak berada pada kedalaman 0. 0 berarti tak terbatas |
| `*print-length*` | `int` | `0` | Mencetak elemen daftar (dan field nilai `defstruct`/`defenum`) sampai jumlah ini dan mengganti sisanya dengan `...`. 0 berarti tak terbatas |
| `*print-circle*` | `bool` | `false` | Jika benar, nilai dipindai sebelum dicetak dan **objek yang muncul dua kali atau lebih diberi label**. Kemunculan pertama adalah `#n=…` dan berikutnya `#n#` |

CL memakai `nil` untuk "tak terbatas", tetapi bahasa ini tidak memiliki `nil`, sehingga seperti pada
`*print-right-margin*`, **0 berarti tak terbatas**. Nilai negatif tidak bermakna dan merupakan
kesalahan cetak. Nilai bawaannya semuanya "tanpa batas / tanpa label", sesuai dengan nilai awal CL.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Struktur melingkar hanya dapat dicetak ketika `*print-circle*` benar.** Jika Anda mencetak nilai
yang menunjuk dirinya sendiri selagi ia salah (bawaan), pencetak terus mengikuti siklus dan proses
crash. CL sama saja (CLHS membiarkan pencetakan struktur melingkar tidak terdefinisi ketika
`*print-circle*` salah).

Siklus hanya dapat dibuat dengan "mengarahkan field `defstruct` ke dirinya sendiri dengan `setf`"
(sel `Sexpr` tidak dapat diubah setelah dibuat, sehingga daftar seperti `'(1 2 3)` tidak pernah dapat
melingkar):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a points to a itself
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Label **dimulai lagi dari 1 untuk setiap hal yang dicetak** (seperti di CL). Bahkan tanpa siklus,
jika objek yang sama muncul dua kali ia mendapat `#1=`/`#1#`, mempertahankan di keluaran informasi
bahwa "kedua ini adalah objek yang sama", sebagaimana ditetapkan CL:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Nilai tanpa pembagian **tidak menampilkan label sama sekali**, sehingga membiarkan variabel ini benar
tidak mengubah keluaran kode sehari-hari.

### 6.2 Basis, huruf besar-kecil, dan keterbacaan

| Variabel | Tipe | Bawaan | Arti |
|---|---|---|---|
| `*print-base*` | `int` | `10` | Basis untuk mencetak bilangan bulat (berlebar tetap dan `int`). Di luar 2 sampai 36 ia adalah **kesalahan cetak** (CL juga menetapkan rentangnya) |
| `*print-radix*` | `bool` | `false` | Jika benar, menambahkan penanda radix: `#b`/`#o`/`#x`, `#NNr` untuk basis lain, dan `.` di akhir untuk basis 10. Penanda ditaruh **sebelum** tanda (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | Huruf besar-kecil nama simbol: `:upcase` / `:downcase` / `:capitalize` (ejaan sama seperti CL). Simbol lain adalah kesalahan cetak |
| `*print-readably*` | `bool` | `false` | Jika benar, mencetak dalam bentuk yang dapat dibaca kembali. Ia memaksa escaping dan menonaktifkan pemotongan `*print-level*`/`*print-length*` |
| `*print-lines*` | `int` | `0` | Jumlah baris yang boleh dipakai pretty printer. Kelebihannya dipotong, dengan `..` di akhir seperti di CL. 0 berarti tak terbatas. Nilai negatif adalah kesalahan cetak |
| `*print-escape*` | `bool` | `true` | Apakah `write`/`write-to-string` melakukan `prin1` atau `princ`. **Hanya keduanya yang membacanya** |
| `*print-array*` | `bool` | `true` | Apakah `Vector<T>` dan `Array<T>` menampilkan isinya. Jika benar, sintaks array CL (`#(1 2 3)` / `#2A((1 2) (3 4))`); jika salah, hanya tipe dan bentuknya, `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

Penanda yang ditambahkan `*print-radix*` dapat dibaca kembali oleh reader (notasi radix pada
[Referensi Sintaks](../syntax.md#1-unsur-leksikal)).

**Mengapa bawaan `*print-case*` berbeda dari CL**: bawaan CL adalah `:upcase` karena reader CL
menyimpan nama simbol dalam huruf besar, yaitu berarti "sebagaimana disimpan". Reader ini
menyimpannya dalam huruf kecil, sehingga bawaan dengan makna yang sama adalah `:downcase`.

**Separuh `*print-readably*` yang hilang**: CL memberi sinyal `print-not-readable` untuk nilai yang
tidak dapat dibaca kembali, tetapi bahasa ini tidak memiliki kondisi untuk disinyalkan, dan tidak ada
cara memutuskan keterbacaan untuk tipe buatan pengguna, yang dapat dicetak `print-object` dengan cara
apa pun. Yang ada hanya escaping paksa dan penggantian pemotongan.

**Mengapa hanya `write` yang membaca `*print-escape*`**: sebagaimana ditetapkan CLHS, `~s`/`prin1`/
`pprint` mengikatnya ke benar, dan `~a`/`princ` ke salah, masing-masing hanya selama pemanggilannya
sendiri. Jadi satu-satunya pembaca yang melihatnya tak terikat adalah `write`/`write-to-string`.
Implementasi `print-object` sebaiknya membaca argumen `escape`-nya sendiri dan bukan variabel global
ini: argumen itu membawa nilai yang dipilih direktif.

**Yang dimiliki CL dan tidak dimiliki bahasa ini**: `*print-gensym*` (tidak ada simbol yang tidak
di-intern).

### 6.3 Penggantian sementara

CL mengikat ini dengan `let`, tetapi `let` pada bahasa ini mengikat secara leksikal, sehingga gunakan
`dlet` ([Lainnya](system.md#10-lainnya)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; the limits apply to this one print only
(with-standard-io-syntax (println "~a" x))   ; print with everything back at the standard values
```

`with-standard-io-syntax` menjalankan badannya dengan semua variabel kendali pencetak pada nilai
standarnya dan `*read-eval*` disetel ke `true`.
