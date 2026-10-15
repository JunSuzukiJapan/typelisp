<!-- translated-from: docs/ja/reference/functions/format.md @ b228b9f6a6a6786e814804c166bce6b79db55522 -->
# Direktif Format

Direktif yang ditulis pada string kendali `print`/`println`/`format`. Direktif ini mencakup hampir
semua direktif `format` pada CL. Fungsi-fungsinya sendiri dijelaskan di
[Pencetakan](printing.md#1-print--println--format).

## 1. Cara menulis direktif

Setiap direktif berbentuk `~`, lalu **parameter awalan** opsional (dipisahkan koma: bilangan bulat /
`'c` (sebuah karakter) / `v` (diambil dari argumen berikutnya) / `#` (jumlah argumen yang tersisa)),
lalu **pengubah** opsional `:` dan `@`, lalu karakter direktif, dalam urutan itu. Karakter direktif
tidak peka huruf besar-kecil.

String kendali harus berupa literal ([Pencetakan](printing.md#1-print--println--format)). Selain itu,
hal-hal berikut diperiksa saat pemeriksaan.

- **Jumlah dan tipe argumen.** Untuk setiap direktif yang mengonsumsi argumen: apakah masih ada
  argumen yang tersisa, dan apakah tipenya diterima (catatan "argumen" pada tabel di bawah). Pada
  tempat jalurnya bergantung pada nilai saat dijalankan, seperti berpindah dengan `~*`, klausa `~[`
  mana yang diambil, apakah `~^` aktif, atau berapa kali `~@{` berulang, **setiap jalur** diperiksa.
  Argumen yang berlebih tidak masalah (seperti di CL).
- **Parameter dan pengubah.** Pengubah yang tidak diterima, parameter yang terlalu banyak, dan nilai
  di luar rentang (lebar negatif, basis selain 2 sampai 36, bilangan bulat di tempat karakter
  diharapkan, dan sebagainya) adalah kesalahan. Semuanya tidak pernah diabaikan atau dibulatkan
  secara diam-diam.

**Elemen** argumen daftar (`~{`, `~:{`, `~<...~:>`) adalah `Sexpr`; baik jumlahnya maupun tipe tiap elemen
tidak dapat diketahui dari tipe. Persyaratan pada elemen (karakter untuk `~c`, dan seterusnya) dan
elemen yang kurang diperiksa saat nilainya tiba, dan merupakan galat waktu jalan (tidak pernah beralih ke
representasi lain sebagai gantinya).

`~d` `~b` `~o` `~x` dan `~r` dengan basis mencetak nilai bukan bilangan bulat seperti yang akan dilakukan
`~a`, dalam desimal, seperti di CL (`(format false "~5d" 1/2)` adalah `"  1/2"`). Aturan longgar CL
lainnya tidak diadopsi: `~:[` yang memperlakukan nilai apa pun sebagai boolean tidak ditafsirkan ulang
begitu; itu galat tipe.

## 2. Keluaran (mengonsumsi satu argumen)

| Direktif | Parameter / pengubah | Arti |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=rata kanan | Estetis (`princ` pada CL; string tanpa tanda kutip). Argumen dapat bertipe apa pun |
| `~s` | Sama seperti di atas | Standar (`prin1` pada CL; bentuk yang dapat dibaca kembali). Argumen dapat bertipe apa pun |
| `~w` | — | `write` pada CL. Mencetak secara pretty jika `*print-pretty*` benar, jika tidak sama dengan `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=kelompok digit, `@`=selalu tanda | Bilangan bulat desimal/biner/oktal/heksadesimal. Argumen boleh apa saja (bukan bilangan bulat dicetak seperti dengan `~a`, dalam desimal) |
| `~r` | `~radix,mincol,padchar,commachar,interval` (dengan radix) atau tanpa | Dengan radix, basis itu (2 sampai 36). Tanpa radix: `~r`=bilangan kardinal Inggris, `~:r`=bilangan ordinal Inggris, `~@r`=angka Romawi, `~:@r`=angka Romawi kuno. Argumen adalah bilangan bulat |
| `~p` | `:`=mundur satu, `@`=y/ies | Bentuk jamak (`~p`→"s", `~@p`→"y"/"ies"). Argumen adalah bilangan bulat |
| `~c` | `:`=nama, `@`=sintaks `#\` | Sebuah karakter. Argumen adalah `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=tanda | Titik tetap. Argumen adalah bilangan |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=tanda | Notasi eksponensial. Argumen adalah bilangan. Parameter exponent-digits, scale, dan overflowchar pada CL tidak didukung (memberikannya adalah kesalahan) |
| `~g` | `@`=tanda | Floating-point umum. Argumen adalah bilangan. Tidak menerima parameter |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Notasi moneter. Argumen adalah bilangan |

## 3. Keluaran (tidak mengonsumsi argumen)

| Direktif | Arti |
|---|---|
| `~%` | Baris baru (`~n%` untuk n buah) |
| `~&` | fresh-line (baris baru kecuali di awal baris; `~n&`) |
| `~\|` | Pemisah halaman (form feed) |
| `~~` | `~` literal (`~n~` untuk n buah) |
| `~t` | Tab (`~colnum,colincT`. Jika sudah berada di kolom colnum atau lebih, bergerak maju sebesar kelipatan colinc; tidak bergerak jika colinc 0. `@`=relatif. `:`=tab relatif terhadap awal blok logis, yang hanya berfungsi saat pretty-printing) |
| `~_` | Baris baru bersyarat (pretty; polos=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Indentasi (pretty; `~ni`=awal blok + n / `~n:i`=kolom saat ini + n) |
| `~<newline>` | Mengabaikan baris baru (`:`=pertahankan spasi putih, `@`=pertahankan baris baru) |

Seperti di CL, direktif pretty-printer (`~_` `~i` `~:t` `~<...~:>`, dan jalur pretty-printing pada
`~a`/`~s`/`~w`) semuanya tidak melakukan apa-apa ketika `*print-pretty*` salah. Nilai bawaannya
salah.

## 4. Struktur kendali

| Direktif | Arti |
|---|---|
| `~(...~)` | Konversi huruf besar-kecil (`~(` huruf kecil, `~:(` kapitalisasi tiap kata, `~@(` kapitalisasi kata pertama saja, `~:@(` semua huruf besar) |
| `~[...~;...~]` | Pemilihan bersyarat (bercabang pada argumen bilangan bulat. Dengan `~n[`, `~v[`, atau `~#[`, ia bercabang pada nilai itu dan tidak mengambil argumen. `~:;`=klausa bawaan, hanya sebagai klausa terakhir). `~:[false~;true~]` bercabang pada argumen `bool` dan memiliki tepat dua klausa |
| `~{...~}` | Iterasi (menelusuri argumen daftar. `~:{`=per sublist, `~@{`=atas argumen yang tersisa, `~:@{`=atas tiap daftar di antara argumen yang tersisa, `~^`=keluar, `~:}`=jalankan sekali meskipun kosong). Badan yang tidak mengonsumsi argumen dalam satu iterasi adalah kesalahan (tidak akan pernah selesai) |
| `~<...~;...~>` | Justifikasi (menyebarkan segmen pada `~mincol` kolom. `:`/`@`=padding di ujung) |
| `~<...~;...~:>` | **Blok logis** (ditutup dengan `~:>`; berbeda dari justifikasi di atas). Segmen pertama adalah awalan dan yang terakhir adalah akhiran (hanya string literal). Dengan pemisah `~@;`, awalan adalah **awalan per baris**. `~:<` menjadikan awalan/akhiran bawaan `(`/`)`. Argumennya satu daftar (`~@<` memakai argumen yang tersisa di tempat) |
| `~*` | Melompati argumen (`~n*`=maju n, `~:*`=mundur, `~@*`=ke posisi absolut) |
| `~/name/` | Pemanggilan fungsi atau metode (bab 5; flag `:`/`@` diteruskan ke yang dipanggil; tidak menerima parameter) |

Direktif CL berikut tidak didukung (semuanya kesalahan saat pemeriksaan).

- `~?` dan `~@?`: keduanya mengambil string kendali sebagai argumen saat dijalankan, sehingga
  argumen yang dikonsumsi direktifnya tidak dapat diperiksa. Tulis direktif-direktif itu langsung
  pada string kendali.
- `~@[...~]`: ia menguji apakah argumen bukan nil, tetapi bahasa ini tidak memiliki nil. Gunakan
  `~:[false~;true~]`, yang bercabang pada `bool`.
- `~{~}` dengan badan kosong: ia mengambil badan dari argumen saat dijalankan. Tulis direktif di
  dalam kurung kurawal.

## 5. `~/name/`

`~/name/` menyelesaikan `name` dengan aturan yang sama seperti pemanggilan `(name arg colon at)` yang ditulis
di tempat itu, lalu mengeluarkan string yang dikembalikan pemanggilan itu. Ia memakai satu argumen. `colon`
dan `at` adalah `bool` yang menyatakan apakah direktif membawa `:` dan `@`.

```lisp
(defstruct point (x i32) (y i32))
(impl point
  (brief (self (colon bool) (at bool)) string
    (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y))))
(defun yen ((n int) (colon bool) (at bool)) string
  (format false "~a yen" n))

(println "~a" (format false "~/brief/"  (make-instance 'point :x 3 :y 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (make-instance 'point :x 3 :y 4)))   ; => <3,4>
(println "~a" (format false "~/yen/" 500))                                    ; => 500 yen
```

- Resolusi nama sama dengan pemanggilan ([Referensi Sintaks 3.10](../syntax.md#310-module--use--ruang-nama)). Jika tipe argumen memiliki metode
  `name`, metode itu yang dipanggil; jika tidak, fungsi `name` yang terlihat. Fungsi di dalam modul dapat
  ditulis dengan jalurnya (`~/m::shout/`).
- Yang dapat dipanggil adalah fungsi atau metode yang ditulis dalam typelisp, yang menerima argumen,
  `colon`, dan `at` apa adanya dan mengembalikan `string`. Fungsi bawaan tidak dapat dipanggil.
- **Apa yang dipanggil ditetapkan saat pemeriksaan.** Pemeriksa mencari setiap argumen yang dapat mencapai
  `~/name/` itu, dengan memperhitungkan lompatan `~*` dan `~[`, lalu menyelesaikan nama untuk tipe masing-
  masing. Jika salah satunya tidak dapat diselesaikan, itu galat saat pemeriksaan, begitu pula jika
  argumen-argumen yang dapat mencapainya akan memanggil dua definisi yang berbeda. Tidak ada nama yang dicari
  saat berjalan, sehingga cara kerjanya sama di executable AOT.
- **Tidak dapat dipakai di dalam `~{`, `~:{`, `~:@{`, atau `~<...~:>`** (galat saat pemeriksaan). Tipe elemen
  argumen list adalah `Sexpr`, yang tidak dapat menentukan apa yang dipanggil. `~@{` menelusuri sisa argumen
  pemanggilan, yang tipenya diketahui, sehingga di sana dapat dipakai.

Perbedaan dengan CL: CL mencari nama sebagai fungsi global. Di sini resolusinya sama dengan pemanggilan,
sehingga metode tipe argumen dipilih lebih dulu daripada fungsi bernama sama.
