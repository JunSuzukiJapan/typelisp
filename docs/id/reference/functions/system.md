<!-- translated-from: docs/ja/reference/functions/system.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Waktu, Lingkungan, dan Implementasi

Fungsi untuk waktu, pertanyaan tentang lingkungan runtime, alat implementasi, penguraian dan
evaluasi teks, docstring, dan makro.

## 1. Waktu

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Dua field: `day` (hari sejak 1900-01-01) dan `second` (detik dalam hari itu, 0..86399) |
| `internal-time` | — | `defstruct` | Dua field: `second` dan `microsecond` (dalam detik itu, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | Waktu sejak epoch CL (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Waktu yang berlalu relatif terhadap proses |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | **Waktu CPU** yang telah dipakai proses ini (pengguna ditambah sistem) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Sebagai jumlah detik. Bentuk untuk melaporkan selisih dua pembacaan |
| `internal-time-units-per-second` | — | `int` | `1000000` (mikrodetik), satuan field `microsecond`. Seperti di CL, nilainya adalah pilihan implementasi |
| `time` | `(time form)` | Makro | Menjalankan `form`, mencetak waktu nyata dan waktu CPU masing-masing satu baris, dan mengembalikan nilai `form` apa adanya |

Waktu nyata dan waktu CPU memberi tahu hal yang berbeda. Untuk pekerjaan yang sebagian besar menunggu
I/O, keduanya berbeda jauh, dan selisih itulah yang ingin Anda ketahui, sehingga `time` menampilkan
keduanya.

`sleep`, yang menghentikan sebuah task, ada di [Task dan Kanal](concurrency.md#3-yield--sleep--mengalah).

## 2. Mendekode dan mengodekan tanggal

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Sembilan field**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. Sembilan nilai kembalian CL sebagai satu struct (tidak ada nilai ganda) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Universal time menjadi komponen kalender. `zone` adalah jam di sebelah barat Greenwich (arah yang sama seperti CL). **Jika dihilangkan, waktu lokal** (seperti di CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | Kebalikannya. Tanpa `zone`, argumen dibaca sebagai **waktu lokal** |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Sekarang, didekode dalam waktu lokal |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | Offset waktu lokal di sebelah barat Greenwich, dalam **detik**, pada universal time itu |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Apakah waktu musim panas berlaku pada universal time itu |

Seperti di CL, untuk `day-of-week` **0 adalah Senin dan 6 adalah Minggu**.

**Tanpa `zone`, waktu lokal dipakai**, seperti di CL. Offset lokal ditanyakan ke OS, sehingga hasilnya
bergantung pada letak mesin. **Memberikan zone secara eksplisit membuatnya deterministik**, dan `0`
adalah UTC.

Satuan `zone` adalah, seperti di CL, "jam di sebelah barat Greenwich", sehingga UTC+9 dibaca sebagai
`-9`. Namun, **argumennya bilangan bulat dan field `zone` pada hasilnya adalah `f64`**. Offset nyata
tidak selalu jam bulat (India +5:30, Nepal +5:45), dan membulatkan nilai yang dilaporkan akan
berbohong secara diam-diam. Zone yang Anda tulis dengan tangan adalah jam bulat, sehingga argumennya
`int`.

Ketika `zone` diberikan, `daylight-p` adalah `false` dan `zone` tepat sama dengan nilai yang diberikan,
sebagaimana ditetapkan CL (*If a time-zone is supplied, daylight saving time information is ignored*).

Waktu lokal yang jatuh di dalam transisi waktu musim panas sejak awal tidak unik, dan CL tidak
menyatakan mana yang diambil. `encode-universal-time` mengembalikan salah satu dari dua jawaban untuk
waktu seperti itu.

## 3. Lingkungan runtime

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | Baris perintah. **Elemen 0 adalah nama program** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Variabel lingkungan. `none` jika tidak disetel atau bukan UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. Dasar `user-homedir-pathname` ([Pathname](streams-files.md#92-fungsi)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | Versi implementasi |
| `machine-type` | `(machine-type)` | `()→string` | Arsitektur CPU (`x86_64` / `aarch64` …). Nilai **target build** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | Nama host |
| `machine-version` | `(machine-version)` | `()→Option<string>` | Nama perangkat keras yang **berjalan sekarang** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` jika tidak dapat ditentukan |
| `software-type` | `(software-type)` | `()→string` | OS (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | Rilis OS (`uname -r`, misalnya `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Nama singkat lokasi pemasangan. **Selalu `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Demikian pula, nama panjang. **Selalu `none`** |

Yang mengembalikan `Option` adalah butir yang dalam CL boleh `NIL` (*or nil if no such name can be
determined*). POSIX tidak memiliki tempat untuk mencatat nama situs, sehingga selalu `none`; SBCL
mengembalikan hal yang sama. Perhatikan perbedaan `machine-type` dan `machine-version`: yang pertama
adalah arsitektur tempat biner ini **dibangun**, yang kedua adalah chip yang **menjalankannya**
sekarang.

Elemen 0 `command-line-args` adalah path skrip untuk `typl script.typl a b`, dan berkas eksekusi itu
sendiri untuk berkas eksekusi AOT yang dijalankan sebagai `./prog a b`. **Kedua cara menjalankan
membaca argumen yang sama pada indeks yang sama** (`typl` membuang namanya sendiri dan opsi seperti
`--heap-cells` sebelum meneruskannya).

## 4. Bertanya kepada pengguna

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Menerima satu `y` / `n`. Bertanya lagi sampai mendapatkannya |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Membuat pengguna mengeja `yes` / `no`. Untuk pertanyaan yang kesalahannya mahal |

Keduanya membaca dari `*standard-input*`. Hanya akhir masukan yang menghentikan pertanyaan ulang, dan
hasilnya `false`.

## 5. Alat implementasi (CLHS 25.2)

Lapisan tempat implementasi menjawab pertanyaan tentang dirinya sendiri. `heap-info` / `room` /
`dribble` adalah fungsi biasa; `trace` / `untrace` / `step` / `disassemble` / `ed` adalah **bentuk
khusus** (`trace` / `untrace` / `disassemble` / `ed` menerima *nama* sebuah definisi, dan `step`
sebuah *bentuk*, semuanya tidak dievaluasi).

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | Keadaan heap saat ini sebagai struct. Angka yang sama yang dicetak `room` |
| `room` | `(room &optional verbose)` | `(bool)→()` | Melaporkan `heap-info` ke `*standard-output*`. `(room true)` memberi rincian lebih |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Mulai merekam keluaran sesi ke `path` / berhenti merekam jika dipanggil tanpa argumen |
| `trace` | `(trace name...)` | `Sexpr` | Melaporkan pemanggilan definisi yang disebutkan ke `*trace-output*`. Mengembalikan daftar nama yang sedang dilacak sekarang |
| `untrace` | `(untrace name...)` | `Sexpr` | Berhenti melaporkan. **Tanpa argumen, menghapus semuanya** |
| `step` | `(step form)` | Tipe `form` | Mengevaluasi `form`, berhenti pada setiap pemanggilan untuk bertanya |
| `disassemble` | `(disassemble name [llvm])` | `()` | Mencetak menjadi apa definisi itu. Kode mesin host secara bawaan, LLVM IR dengan `true` |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Menjalankan `$VISUAL` / `$EDITOR`. Jika diberi nama, membuka baris tempat definisi itu ditulis |

`trace`/`untrace`/`step`/`disassemble` hanya untuk interpreter, dan fungsi yang memanggilnya tidak
dapat dikompilasi ([Referensi Sintaks bab 10](../syntax.md#10-kompilasi)).

### 5.1 Field `heap-info`

| Field | Tipe | Isi |
|---|---|---|
| `capacity` / `live` / `free` | `int` | Seluruh arena cons dan rinciannya. Selalu `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | Jumlah saat ini dari tiga jenis objek lain pada heap |
| `gc-count` | `int` | Jumlah pengumpulan sejak implementasi dimulai |
| `growable` | `bool` | Apakah arena masih dapat membesar |

Field semuanya `int` (kecuali `growable`). Batas pertumbuhan (lihat deskripsi `typl --heap-cells`)
tidak dilaporkan, karena yang ingin diketahui pembaca adalah apakah ia masih dapat membesar
(`growable`).

### 5.2 Apa yang dapat dan tidak dapat dilihat `trace` / `step`

- **Definisi dengan badan terkompilasi juga terlihat, dari tempat pemanggilan yang diinterpretasi.**
- **Tempat pemanggilan *di dalam* kode terkompilasi tidak terlihat.** Melacak nama yang memiliki badan
  terkompilasi menambahkan catatan satu baris yang menyatakan hal itu. Keterbatasan yang sama yang
  dijelaskan SBCL untuk pemanggilan lokal.
- **Pemanggilan melalui nilai closure (`funcall`/`apply`) tidak terlihat.** Closure tidak memiliki
  nama.
- **Definisi generik tidak tercakup.** Salinan untuk setiap tipe dibuat di tiap tempat pemakaian,
  sehingga tidak ada satu badan untuk dinamai (alasan yang sama, dan susunan kata yang sama, seperti
  ketika `compile` menolak).

Perintah pada `step` adalah `s` (masuk ke pemanggilan ini; baris kosong melakukan hal yang sama), `n`
(lewati pemanggilan ini), `c` (berhenti bertanya mulai dari sini), dan `q` (batalkan). **Jika masukan
standar bukan terminal, `step` hanya mengevaluasi `form`**: perilaku degenerat yang secara eksplisit
diizinkan CLHS, agar skrip dan pengujian tidak menggantung pada prompt yang tidak dapat dijawab siapa
pun.

`$VISUAL` / `$EDITOR` pada `ed` dipecah pada spasi putih, sehingga `EDITOR="code -w"` berfungsi.
Jika keduanya tidak disetel, hasilnya `Err`: ia tidak menebak `vi`. Nomor baris diserahkan lebih
dulu, dalam bentuk `+N`.

`dribble` merekam ketiga cara keluaran sesi meninggalkan proses: apa yang ditulis
`print`/`println`/`format`, apa yang ditulis ke stream yang terhubung ke keluaran standar, dan baris
yang diketik ke REPL beserta nilai yang dicetak balik REPL.

## 6. Parsing dan evaluasi

Semuanya menangani teks dan data dari saat dijalankan (yang tidak dikendalikan program itu sendiri),
sehingga saat gagal mereka mengembalikan `Err` dari `Result` dan bukan panic. Tipe kesalahannya
adalah tipe konkret per operasi ([Tipe kesalahan](option-result.md#3-tipe-kesalahan-dan-trait-error)).

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | `parse-integer` pada CL. Melewati spasi putih di awal dan akhir (himpunan yang sama seperti `trim`), membaca paling banyak satu tanda `+`/`-`, lalu digit pada basis `radix` (bawaan 10, 2 sampai 36; digit di atas 10 dalam huruf besar atau kecil). Tidak ada batas jumlah digit (`int`). Karakter lain yang tersisa menghasilkan `Err`. Dengan `:junk-allowed true`, ia berhenti pada non-digit pertama dan mengabaikan sisanya, tetapi menghasilkan `Err` jika tidak ada satu digit pun (sepadan dengan `nil` pada CL). Ia tidak mengembalikan nilai kedua CL (posisi tempat pembacaan berakhir). `radix` di luar rentang melakukan panic (kesalahan pemanggil, bukan pada teks) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Bilangan floating-point. Juga menerima `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Membaca satu `Sexpr` dari `s` (dengan reader yang sama yang membaca kode sumber). Tanda kurung tidak seimbang, string yang tidak tertutup, dan sejenisnya menghasilkan `Err`. Membaca dari stream adalah `read-sexpr` ([Stream](streams-files.md#6-fungsi-generik-dan-operasi-berkas)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` ditambah **posisi tempat pembacaan berakhir**. `(car r)` adalah nilainya dan `(cdr r)` posisi karakter berikutnya yang akan dibaca. `start` bawaannya 0 |
| `read-from-string-preserving-whitespace` | Sama seperti di atas | Sama seperti di atas | Sama, tetapi tidak mengonsumsi spasi putih yang mengakhiri datum. Perbedaannya terlihat pada posisi yang dikembalikan |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Memeriksa tipe `form` saat dijalankan dan mengevaluasinya. Mengikuti `eval` pada CL |

CL mengembalikan **dua nilai** (nilai dan posisi) dari `read-from-string`, tetapi bahasa ini tidak
memiliki nilai ganda, sehingga ia mengembalikan satu `cons-cell`. Dengan adanya posisi, membaca string
satu datum sekali waktu menjadi perulangan dan bukan pemindaian ulang:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

Perbedaan yang dibuat `preserving-whitespace` adalah **satu karakter spasi putih**: `read` pada CL
mengonsumsi spasi putih yang mengakhiri datum, dan `read-preserving-whitespace` membiarkannya.
`(read-from-string "12 34")` mengembalikan posisi 3, dan versi preserving mengembalikan 2.

Sintaks bilangan yang diterima reader ada di [Referensi Sintaks bab 1](../syntax.md#1-unsur-leksikal).
Apa yang dicetak `*print-radix*` ([Pencetakan](printing.md#62-basis-huruf-besar-kecil-dan-keterbacaan))
dapat dibaca kembali apa adanya. Tidak ada `*read-base*` CL.

### 6.1 Arti `eval`

Ia mengikuti `eval` pada CLHS: ia mengevaluasi di **lingkungan global saat ini** (fungsi, variabel,
tipe, dan makro global, termasuk definisi yang ditambahkan saat dijalankan) dan di **lingkungan
leksikal kosong** (pengikatan lokal `let`/`lambda` milik pemanggil tidak terlihat). Baik ekspresi
maupun definisi (`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`) dapat dievaluasi, dan definisi
langsung didaftarkan ke lingkungan global secara permanen.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; the global x is visible
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; returns the defined name
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; the definition just made is visible
```

- **Nilai kembalian**: untuk ekspresi, hasilnya sebagai `Option<Sexpr>`; untuk definisi, simbol nama
  yang didefinisikan (seperti di CL). Untuk memakai hasilnya, bongkar `Sexpr` dengan `match`
  (`(int n)`/`(str s)`/…).
- **Perbedaan akibat tipe statis (penting)**: CL mengembalikan nilai sebenarnya dari hasil, tetapi di
  bahasa ini tipe kembalian hanya dapat seragam `Result<Option<Sexpr>,EvalError>`. Selain itu,
  **kode yang ditulis secara statis tidak dapat merujuk ke depan pada nama yang didefinisikan `eval`
  saat dijalankan**: `(sq 9)` yang ditulis langsung di berkas diperiksa sebelum `eval` yang
  mendefinisikan `sq` berjalan, dan "tidak terdefinisi". Namun, **`eval` berikutnya dapat melihatnya**
  (pemeriksaan tipenya berjalan saat dijalankan, setelah definisi). REPL memeriksa dan menjalankan
  satu baris sekali waktu, sehingga nama yang didefinisikan dengan `eval` dapat dipanggil langsung
  dari baris berikutnya.
- **Kesalahan**: kesalahan tipe dan kesalahan sintaks mengembalikan `Err` (tidak melakukan panic).
  **Panic saat dijalankan** pada kode yang dievaluasi (pembagian dengan nol dan sebagainya)
  merambat seperti pada kode yang ditulis langsung. Pembersihan `unwind-protect` di antaranya
  dijalankan ([Referensi Sintaks bab 8](../syntax.md#8-keluar-non-lokal-catch--throw--unwind-protect)).
- **Ruang nama**: ketika dijalankan oleh `typl file.typl` dan di dalam berkas eksekusi AOT, `eval`
  mengevaluasi di ruang nama modul skrip (variabel global milik skrip terlihat). REPL mengevaluasi di
  ruang nama akar.
- **Kompilasi**: baik `read` maupun `eval` dapat dikompilasi. Cara penanganannya pada berkas eksekusi
  AOT, dan akibatnya (bentuk yang diserahkan ke eval diinterpretasi), ada di
  [Referensi Sintaks 10.2](../syntax.md#102-eval-pada-berkas-eksekusi-aot).

## 7. Docstring / `documentation`

`defun`/`defmethod` (termasuk di dalam `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/
`defenum`/`deftype`/`deftrait` dapat membawa docstring. Posisinya mengikuti aturan CL untuk masing-masing:

| Bentuk | Posisi docstring |
|---|---|
| `defun` / `defmethod` / `defmacro` | Di awal badan (setelah tipe kembalian dan klausa `where`). Hanya ketika setidaknya satu bentuk badan menyusul; string tunggal tetap menjadi nilai kembalian |
| `defvar` / `defconstant` | **Setelah** nilai awal: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Tepat setelah** nama, sebelum field/varian |
| `deftype` | **Tepat setelah** nama, sebelum tipe: `(deftype meters "doc" i32)` |
| `deftrait` | Tepat setelah daftar supertrait, sebelum butir. Satu untuk seluruh trait. **Metode dengan implementasi bawaan** dapat menaruh docstring sendiri tepat sebelum badannya |

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `documentation` | `(documentation name)` | (bentuk khusus; `name` adalah simbol polos atau `Type::method`)→`Option<string>` | Mengembalikan docstring `name` |

Seperti `quote`/`compile`, `documentation` adalah bentuk khusus (ia membaca `name` sebagai nama yang
tidak dievaluasi). Tidak seperti `(documentation 'name 'function)` pada CL, ia tidak menerima argumen
tipe; sebagai gantinya ia menyelesaikan nama polos dengan urutan **variabel → fungsi → tipe → trait →
makro** (prioritas yang sama seperti untuk pengenal polos yang dievaluasi sebagai ekspresi). Bentuk
`Type::method` mencari docstring metode terkait atau statis.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**Nilainya ditentukan saat pemeriksaan**: jika nama tidak diselesaikan ke definisi apa pun, itu
adalah kesalahan saat pemeriksaan (seperti merujuk variabel yang tidak terdefinisi). Jika
diselesaikan tetapi tidak ada docstring, hasilnya `Option::none`.

**Tidak tercakup**:

- `(setf documentation)` (mengubah docstring saat dijalankan) tidak ada.
- Nama bebas yang dikualifikasi modul (`mod::name`; `Type::method` didukung) tidak didukung.
- Deklarasi metode pada `deftrait` **tanpa badan** tidak dapat memiliki docstring. Literal string di
  akhir akan menjadi badan (nilai kembalian) implementasi bawaan itu sendiri, sehingga tidak ada cara
  membedakan keduanya.

Hover pada language server (`typl-lsp`) juga menampilkan docstring.

## 8. Makro

Cara mendefinisikan makro ada di [Referensi Sintaks 3.14](../syntax.md#314-defmacro--definisi-makro).

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Simbol baru. Namanya `" <prefix><n>"`, dengan `n` adalah `*gensym-counter*`. Spasi di awal tidak dapat ditulis pada kode sumber, sehingga pengikatan yang dihasilkan tidak pernah bentrok dengan nama yang ditulis |
| `*gensym-counter*` | Variabel | `int` | Bilangan yang dipakai `gensym` berikutnya. Seperti di CL, dapat dibaca dan disetel |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Mengekspansi pemanggilan makro satu langkah. `none` berarti "bukan pemanggilan makro" |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Mengulang sampai bukan makro lagi |

`macroexpand-1` mengembalikan `Option`. CL melaporkan "apakah diekspansi" sebagai nilai kembalian
kedua, tetapi tidak ada nilai ganda, sehingga `none` berperan itu. **Makro yang berekspansi menjadi
pemanggilan dirinya sendiri tidak pernah dapat tertukar dengan non-makro.** Satu langkah ekspansi
adalah yang sama dengan yang dipakai pemeriksa tipe, sehingga apa yang dilihat program dan apa yang
dilihat pemeriksa tidak pernah berbeda.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none shows as the empty list (Option<Sexpr> is transparent)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

Yang dimiliki CL dan tidak dimiliki bahasa ini: `eval-when` (`:compile-toplevel`/`:load-toplevel`/
`:execute` selalu bertepatan, sehingga tidak ada pembedaan untuk dipilih), `define-compiler-macro`,
`load-time-value`, `make-symbol`/`copy-symbol`/`gentemp` (simbol yang tidak di-intern; pengikatan
dicari berdasarkan nama, sehingga tidak ada yang diperoleh).

## 9. Pengikatan makro lokal (`macrolet` / `symbol-macrolet`)

Keduanya adalah bentuk khusus yang mengikat secara leksikal **nama yang bukan nilai**. Tidak ada yang
tersisa saat dijalankan: yang dikompilasi adalah bentuk ekspansi dari badan.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Pengikatan `macrolet` menyembunyikan makro global bernama sama **hanya selama badan**. Daftar
  lambda-nya sama seperti `defmacro` (`&optional`/`&rest`/`&key`).
- **Saudara dalam `macrolet` yang sama tidak dapat saling melihat dari *badan*-nya** (seperti di CL;
  ini perbedaan dengan `labels`). Ekspansi diperiksa di tempat pemakaian, sehingga `earlier` yang
  berekspansi menjadi `(later ...)` berfungsi: keduanya terlihat di tempat itu.
- Nama `symbol-macrolet` masuk ke lingkungan sebagai pengikatan biasa. Jadi `let` di dalamnya
  menyembunyikan nama yang sama, dan variabel di luar tersembunyi: aturan CL muncul apa adanya.
- **`setf` menulis ke ekspansinya.** `(setf head 42)` adalah `(setf (get v 0) 42)`.
- Ekspansi diperiksa di **lingkungan tempat pemakaian** (bukan tempat pengikatan).

## 10. Lainnya

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Panic jika salah. Tanpa pesan, `assertion failed: <tes sebagaimana ditulis>` (ia adalah makro, sehingga dapat menyebut ekspresinya sendiri). Restart CL tidak ada di bahasa ini |
| `warn` | `(warn control args...)` | `(string,...)→()` | Menulis satu baris berawalan `WARNING: ` ke `*error-output*` dan **melanjutkan**. Cara melaporkan sesuatu tanpa mengembalikan `Result` dan tanpa mengakhiri program |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Mengganti variabel global hanya selama `body` dan memulihkannya saat keluar. CL menulis ini sebagai `let`, tetapi `let` pada bahasa ini selalu mengikat secara leksikal, karena itu namanya terpisah (peran yang sama seperti makro Emacs Lisp bernama sama). Memulihkan bagaimanapun badan ditinggalkan: selesai normal, `throw`, `panic`, `break`/`return`. **Bukan pengikatan per task** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Menjalankan `body` dengan setiap variabel kendali pencetak pada nilai standarnya dan `*read-eval*` disetel ke `true` ([Pencetakan](printing.md#6-mengendalikan-seberapa-banyak-yang-dicetak)) |
| `exit` | `(exit code)` | `int→!` | Mengakhiri proses |
| `dump` | `(dump path)` | `string→bool` | Menulis lingkungan saat ini (informasi tipe ditambah badan terkompilasi) ke satu berkas. `typl --image <path>` memulai lagi darinya. Hanya untuk interpreter ([Referensi Sintaks 10.1](../syntax.md#101-dump)) |
