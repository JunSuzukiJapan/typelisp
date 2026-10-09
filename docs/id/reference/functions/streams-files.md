<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Stream dan Berkas

Trait dan metode stream, tipe stream konkret, operasi berkas, dan pathname. Socket jaringan juga
merupakan stream, dan dibahas di [Jaringan](network.md).

## 1. Hierarki trait

Apa yang dinyatakan CL dengan hierarki kelas dinyatakan di sini dengan **hierarki trait**. Baik arah
(masukan / keluaran) maupun tipe elemen ditentukan **secara statis**, sehingga tidak perlu menanyakan
saat dijalankan "apakah stream ini dapat dibaca?".

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; character input
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; character output
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; input that can push back one character
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; byte input
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; byte output
```

Fungsi yang membaca karakter menerima tipe stream apa pun, bawaan maupun buatan pengguna, jika ia
menerima `(where (CharInput S))` atau `:dyn CharInput`.

## 2. Metode

Setiap metode `CharInput` memiliki implementasi bawaan. Sebuah implementasi hanya menulis
`read-item`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | Elemen berikutnya. `none` di akhir. **Satu-satunya metode yang wajib diimplementasikan** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Karakter berikutnya |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Sampai baris baru berikutnya (baris baru dikonsumsi dan dibuang). Baris terakhir yang tidak berakhir dengan baris baru juga dikembalikan |
| `read-all` | `(read-all s)` | `(S)→string` | Semua yang tersisa |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Hanya karakter yang sudah tersedia. `none` dan bukan menunggu |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Mendorong sampai `n` karakter ke `v` dan mengembalikan berapa banyak yang benar-benar dibaca. Kurang dari `n` hanya di akhir |

`listen` ada di `InputStream` (induk `CharInput`):

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Apakah pembacaan berikutnya dapat dijawab tanpa menunggu. Bawaannya `false`, **sisi yang tidak pernah berbohong**: `true` akan berupa tebakan, dan tebakan yang salah akan membuat `read-char-no-hang` terblokir. Semua stream bawaan menimpanya. **Untuk stream buatan pengguna yang tidak menimpanya, `read-char-no-hang` selalu mengembalikan `none`** |

`PeekInput` (yang mewarisi `CharInput`) menambahkan **mengembalikan satu karakter**. Hanya stream itu
sendiri yang memiliki tempat menyimpan karakter yang dikembalikan, sehingga ini tidak dapat memiliki
implementasi bawaan dan merupakan trait terpisah. `file-stream`/`string-input-stream`/
`standard-stream` mengimplementasikannya, dan stream lain mendapatkannya ketika dibungkus dengan
`make-peek-stream` (bab 4).

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Membuat pembacaan berikutnya mengembalikan `c`. **Satu-satunya metode yang wajib diimplementasikan**. Seperti di CL, hanya satu karakter yang dijamin |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Melihat karakter berikutnya tanpa mengonsumsinya |

Demikian pula, untuk `CharOutput` sebuah implementasi hanya menulis `write-item`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Menulis satu elemen. **Satu-satunya metode yang wajib diimplementasikan** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Menulis satu karakter |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Menulis string |
| `write-line` | `(write-line s str)` | `(S,string)→()` | String dan baris baru |
| `terpri` | `(terpri s)` | `(S)→()` | Satu baris baru (nama CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Satu baris baru kecuali di awal baris |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Apakah karakter berikutnya yang ditulis akan memulai baris. Bawaannya `false` (sehingga `fresh-line` menulis baris baru: saat ragu, menulis adalah sisi yang aman). Semua stream bawaan menimpanya |
| `finish-output` | `(finish-output s)` | `(S)→()` | Mengosongkan buffer |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Menulis semua karakter `v` secara berurutan |

`at-line-start` mengingat **hanya apa yang ditulis melalui stream itu**. `print`/`println`/
`(format true ...)` menulis ke keluaran standar tanpa melalui `*standard-output*`, sehingga jika Anda
mencampur keduanya, `(fresh-line *standard-output*)` tidak mengetahui baris baru yang ditulis
`println`. Tetaplah pada salah satunya.

`Stream` umum bagi semua stream:

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Apakah masih terbuka |
| `close` | `(close s)` | `(S)→()` | Menutupnya. **GC tidak menutup stream**, jadi lakukan secara eksplisit (atau dengan `with-open-file`) |

## 3. Tipe stream konkret

| Tipe | Cara membuatnya | Trait yang diimplementasikan |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` adalah salah satu dari tiga konstanta `direction-input` / `direction-output` /
`direction-append`. `open-file` mengembalikan `Err(FileError)` jika berkas tidak dapat dibuka
(berkas yang tidak ada adalah hasil biasa, bukan panic). Nama berkas dapat berupa string atau
`pathname` (`Pathish` pada bab 9).

`(get-output-stream-string s)` mengembalikan apa yang telah ditulis ke `string-output-stream` dan
mengosongkannya. Seperti di CL, ia dapat diambil bahkan setelah `close`.

**I/O byte** memakai `ByteInput`/`ByteOutput`. Keduanya menetapkan `Item` pada `InputStream`/
`OutputStream` ke `int`, dengan cara yang sama seperti `CharInput`/`CharOutput` menetapkannya ke
`char`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | Byte berikutnya. `none` di akhir berkas |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Menulis satu byte. Kesalahan di luar 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | Versi karakter, dalam byte |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Sama seperti di atas |

CL menentukan tipe elemen pada **pemanggilan**, seperti `(open name :element-type '(unsigned-byte 8))`,
tetapi di sini tipe elemen adalah **tipe** stream, sehingga yang berbeda adalah fungsi yang
membukanya. Membaca byte dari stream karakter adalah kesalahan tipe (`string-input-stream` tidak
mengimplementasikan `ByteInput`). Membaca byte tepat setelah mengembalikan karakter dengan
`unread-char` juga merupakan kesalahan.

## 4. Stream komposit

Semuanya adalah `defstruct` pada pustaka standar dan dapat disarangkan.

| Nama | Bentuk | Deskripsi |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Menulis ke semua anggota `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Membaca dari `in` dan menulis ke `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Membaca dari `in` dan juga menulis karakter yang dibaca ke `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Membaca `Vector<:dyn CharInput>` satu demi satu |
| `make-peek-stream` | `(make-peek-stream in)` | Menambahkan pengembalian satu karakter pada `:dyn CharInput` apa pun, menjadikannya `PeekInput` (untuk `read-sexpr`) |

## 5. Makro

| Nama | Bentuk | Deskripsi |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Membuka, menjalankan badan, menutup. `Result<nilai dari badan, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Membaca dari string |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Mengembalikan apa yang ditulis |

## 6. Fungsi generik dan operasi berkas

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Memindahkan semuanya |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Semua baris yang tersisa |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Membaca satu `Sexpr` (`read` pada CL). `Ok(eof)` di akhir masukan, `Ok(datum d)` ketika satu dibaca, `Err` jika bukan data. Ia **mengonsumsi satu karakter spasi putih** yang mengakhiri datum (seperti di CL). `ReadOutcome` bukan `Option<Sexpr>` agar membaca daftar kosong `()` dan akhir masukan bukan nilai yang sama |
| `read-sexpr-preserving-whitespace` | Sama seperti di atas | Sama seperti di atas | Sama, tetapi membiarkan spasi putih (`read-preserving-whitespace` pada CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Membaca sampai `ch` dan membuat daftar. `ch` dikonsumsi. `Err` jika masukan habis |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Menulis satu baris sekali waktu |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | Seluruh isi |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Semua baris |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Menuliskannya |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Apakah ada |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Menghapus, mengganti nama (argumennya `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Path absolut dengan tautan simbolik dan `.`/`..` diselesaikan. `Err` jika tidak ada |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | Waktu modifikasi terakhir. Berupa **universal time**, sehingga `decode-universal-time` ([Waktu](system.md#2-mendekode-dan-mengodekan-tanggal)) dapat membacanya |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | Nama login pemilik. `Err` jika berkas tidak ada, `Ok(none)` jika uid pemilik tidak memiliki entri di basis data password: dua kasus yang dibedakan CL tetap dipisahkan |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Apakah direktori. **Juga `false` jika tidak ada**; gunakan `probe-file` untuk membedakan keduanya |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Mendaftar isinya berdasarkan truename (path absolut dengan tautan simbolik diselesaikan, seperti `truename`). Tautan simbolik yang targetnya hilang dilewati. `.`/`..` dilewati. Urutannya sesuai yang diberikan OS |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Membuatnya beserta induknya. Berhasil jika sudah ada |

Setiap argumen yang menamai berkas **dapat berupa string atau `pathname`**. Ini perlakuan yang sama
seperti pathname designator pada CL, diselesaikan melalui trait `Pathish` dan bukan pengujian tipe
saat dijalankan (bab 9).

Karakter pengakhir pada `read-delimited-list` **juga mengakhiri token**. Ia berlaku hanya pada
kedalaman 0: pada `(1 2]` karakter `]` dibaca sebagai bagian dari teks daftar itu sendiri dan
dilaporkan sebagai daftar yang rusak. Tidak ada padanan untuk argumen ketiga `recursive-p` pada CL.

## 7. Menjadikan tipe Anda sendiri sebuah stream

Tulis satu `write-item` dan implementasi bawaan membawa sisanya. Ia juga dapat masuk ke stream
komposit.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; every remaining method is the default

(write-line (counter::new 0) "four")   ; write-line, terpri and fresh-line all work
```

Masukan bekerja dengan cara yang sama: Anda hanya menulis `read-item`. Bahkan tipe tanpa pengembalian
karakter sendiri dapat di-`read` setelah dibungkus, seperti `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Nama | Pemanggilan | Tipe | Deskripsi |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` membaca karakter `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Mengembalikan apa yang terdaftar |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` membaca urutan dua karakter `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Sama seperti di atas |

`F` adalah `(fn (string-input-stream char) Option<Sexpr>)`. Cara memakainya, kapan berlaku, dan
perbedaannya dengan CL ada di [Referensi Sintaks](../syntax.md#11-reader-macro-readtable).

## 9. Pathname `pathname`

Nama berkas yang dipecah menjadi bagian-bagian. Ia menyimpan komponen direktori yang dipisahkan `/`,
nama, tipe (ekstensi), dan apakah dimulai dari akar.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")   split at the last dot
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Trait penunjuk pathname `Pathish`

Di tempat CL menerima pathname designator (string atau pathname), bahasa ini menerima `Pathish`.
Baik `string` maupun `pathname` mengimplementasikannya, dan **setiap operasi berkas menerimanya
secara generik**, sehingga `(open-input "a.txt")` dan `(open-input p)` keduanya adalah pemanggilan
biasa (tidak ada pengujian tipe saat dijalankan). `namestring` pada string hanya mengembalikan
dirinya, sehingga selama Anda menyerahkan string, tidak ada penguraian yang terjadi.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | Bentuk string. Wajib diimplementasikan |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Mengubah menjadi `pathname` (fungsi `pathname` pada CL, diganti namanya karena akan bentrok dengan nama tipe). Wajib diimplementasikan |

### 9.2 Fungsi

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Memecah string menjadi bagian-bagian. `/` di akhir (atau nama kosong) berarti "tanpa nama", yaitu direktori |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Membangun satu dari komponen yang diberikan saja (semuanya `&key`). Nama atau tipe yang dihilangkan tetap "tidak ada" dan merupakan sesuatu yang diisi oleh `merge-pathnames` |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Komponen direktori, yang terluar lebih dulu |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Nama tanpa tipe. `none` untuk direktori |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Setelah titik terakhir. Titik di awal tidak dihitung (seluruh `.gitignore` adalah nama) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Apakah dimulai dari akar |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | Direktori home. `none` jika tidak ada `$HOME` (CL juga mengizinkan `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | Bagian sampai `/` terakhir |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Hanya bagian `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Mengisi komponen yang hilang dari `p` dengan `default`. `p` relatif ditaruh di bawah direktori `default`; `p` absolut mempertahankan direktorinya sendiri |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | Bentuk relatif terhadap `default`. Seluruh `p` jika tidak berada di bawah basis |

Argumen tipe semuanya membawa `(where (Pathish P))`.

## 10. Perbedaan dari CL

- **Hierarki trait, bukan hierarki kelas.** Tidak ada `input-stream-p` / `output-stream-p`: tipe
  membawa arah, sehingga bukan pertanyaan yang diajukan saat dijalankan.
- **`read` memiliki nama yang berbeda untuk versi string dan stream.** `(read "...")` (sepadan dengan
  nilai pertama `read-from-string` pada CL; jika Anda juga memerlukan posisi tempat pembacaan
  berakhir, gunakan `read-from-string`) dan `(read-sexpr s)` (`read` pada CL). Sebuah pemanggilan
  diselesaikan ke satu tipe penerima, sehingga nama yang sama tidak dapat di-overload.
- **Pengembalian karakter adalah trait terpisah** (`PeekInput`), sehingga tipe yang hanya memerlukan
  `read-char` tidak dipaksa mengimplementasikan `unread-char`.
- **Penutupan bersifat eksplisit.** GC tidak menutup stream (GC berjalan pada waktu yang tidak dapat
  diprediksi, sehingga menyerahkannya ke GC akan membuat saat penutupan juga tidak dapat
  diprediksi). Memakai `with-open-file` adalah cara yang aman.
- **Pathname tidak memiliki komponen host, device, atau versi.** Tidak ada pathname wildcard dan
  tidak ada pathname logis (`logical-pathname`). Pemisahnya selalu `/`.
- **Fungsi `pathname` adalah `to-pathname`**, karena tipe, trait, dan fungsi berbagi satu ruang nama.
- **Tidak ada pencocokan dengan wildcard**, sehingga `directory` adalah fungsi yang "mendaftar isi
  direktori itu" dan tidak lebih. `directory` pada CL mencocokkan terhadap pola pathname.
