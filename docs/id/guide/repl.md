<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# REPL

Menjalankan `typl` tanpa argumen akan membuka REPL (mode interaktif). Dokumen ini menjelaskan apa
yang bisa dilakukan REPL selain mengevaluasi ekspresi: perintah-perintahnya, bantuan saat
mengetik, merujuk hasil sebelumnya, menghentikan komputasi, dan menyelidiki setelah galat.

## Memulai dan keluar

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Keluar dengan `:quit` (singkatnya `:q`) atau `Ctrl+D`. Riwayat masukan disimpan di
`~/.typl_history`, sehingga saat dijalankan lagi riwayat itu bisa dipanggil dengan `Ctrl+P` /
`Ctrl+N` atau `Ctrl+R` (pencarian riwayat).

Saat dimulai, sebelum prompt pertama, hal-hal berikut dijalankan berurutan. Tidak satu pun
menampilkan nilai yang dihasilkannya.

1. Isi `~/.typlrc`, jika ada. Dengan `--no-init` berkas itu tidak dibaca.
2. `--eval FORM` dan `--load FILE` dari baris perintah, sesuai urutan penulisan. Keduanya boleh
   diberikan berapa kali pun.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load`, dan `--no-init` hanya untuk memulai REPL. Memberikannya saat menjalankan
berkas adalah galat.

REPL dimulai di modul `user`. Saat berpindah ke modul lain dengan `(in-module foo)`, prompt
menampilkan modul itu.

```
typl> (in-module foo)
typl user::foo>
```

## Perintah REPL

`:nama` yang ditulis di awal baris adalah perintah REPL. `:help` menampilkan daftarnya. `:nama`
yang tidak ada di daftar bukan perintah; ia dievaluasi sebagai kata kunci biasa.

| Perintah | Arti |
|---|---|
| `:help` (`:h`, `:?`) | menampilkan daftar perintah |
| `:type FORM` (`:t`) | menampilkan tipe FORM tanpa mengevaluasinya |
| `:describe NAME` (`:d`) | menampilkan jenis definisi, tipenya, field atau variannya, metodenya, trait yang diimplementasikannya, tempat definisinya, dan string dokumentasinya |
| `:apropos TEXT` | mencari di semua modul definisi yang namanya mengandung TEXT (huruf besar dan kecil dianggap sama) |
| `:src NAME` | menampilkan kode sumber sebuah definisi |
| `:who-calls NAME` | menampilkan fungsi dan metode yang memanggil fungsi atau metode NAME |
| `:inspect FORM` (`:i`) | mengevaluasi FORM dan membuka nilainya sesuai tipenya |
| `:backtrace` (`:bt`) | menampilkan pemanggilan yang sedang berlangsung ketika baris sebelumnya gagal, mulai dari yang terdalam |
| `:locals [N]` | menampilkan argumen pemanggilan ke-N dari `:backtrace` (ke-0 jika N dihilangkan) |
| `:cd [DIR]` | memindahkan folder saat ini ke DIR (ke folder rumah jika DIR dihilangkan) |
| `:pwd` | menampilkan folder saat ini |
| `:quit` (`:exit`, `:q`) | keluar dari REPL |

NAME bisa berupa nama seperti `inner`, atau jalur seperti `std::seq::reverse` atau
`Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Menambahkan 1 ke x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Menambahkan 1 ke x.
typl> :src inner
(defun inner ((x int) (s string)) int "Menambahkan 1 ke x." (+ x 1))
```

`:src` menampilkan definisi yang dimuat dari berkas dari berkas itu, dan definisi yang diketik di
REPL dari teks yang diketik. Definisi di pustaka standar tidak memiliki catatan lokasi sumber,
sehingga sumbernya tidak bisa ditampilkan.

`:who-calls` mencari di badan fungsi-fungsi dalam sesi saat ini. Untuk fungsi generik, hanya
pemakaian yang sudah diinstansiasi dengan tipe konkret yang ditemukan.

`:cd` mengubah folder tempat `(load ...)` mencari, tetapi `use` tetap mencari berkas di folder
tempat REPL dijalankan.

## Membuka nilai (`:inspect`)

`:inspect` membuka nilai sesuai tipenya. Struct terbuka menjadi field-field-nya, enum (termasuk
`Option`) menjadi field variannya, `Vector` menjadi elemen-elemennya, dan `HashTable` menjadi kunci
dan nilainya. Angka masuk ke bagian itu, `u` naik satu tingkat, dan `q` selesai.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

List dan `Sexpr` tidak dibuka; keduanya ditampilkan apa adanya.

## Bantuan saat mengetik

Saat mengetik di terminal, hal-hal berikut tersedia.

- `Tab` melengkapi nama (definisi dan sintaks seperti `if` dan `defun`), jalur yang mengandung
  `::`, perintah REPL di awal baris, dan nama berkas di dalam string.
- Setelah nama fungsi diikuti spasi, tipe fungsi itu ditampilkan samar di akhir baris.
- Kurung pasangan dari kurung di sebelah kursor disorot.
- Jika Enter ditekan sebelum sebuah form selesai, baris berikutnya dimulai dengan indentasi sesuai
  jumlah kurung yang masih terbuka.

## Hasil sebelumnya (`:*`, `:**`, `:***`)

Di REPL, `:*` adalah nilai yang terakhir ditampilkan, `:**` yang sebelumnya, dan `:***` yang
sebelumnya lagi. Masing-masing bertipe sesuai nilainya.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- Masukan yang tidak menampilkan nilai (seperti definisi) atau yang gagal tidak menggeser hasil.
- Menulis `:**` atau `:***` sebelum ada sebanyak itu hasil adalah galat.
- `':*` yang di-quote adalah kata kunci biasa.
- `:*` yang ditulis di badan fungsi tetap merujuk ke hasil saat fungsi itu didefinisikan. `*` di
  Common Lisp membaca nilai saat fungsi dipanggil, tetapi typelisp menetapkannya saat definisi agar
  punya tipe.
- Di luar REPL (saat menjalankan berkas, serta di `~/.typlrc`, `--load`, dan `--eval`), `:*` adalah
  kata kunci biasa.

## Menghentikan komputasi (`Ctrl+C`)

Menekan `Ctrl+C` saat evaluasi menghentikan komputasi itu dan kembali ke prompt. Apa yang sudah
didefinisikan sebelumnya tetap ada. Ini berlaku baik untuk perulangan di interpreter, perulangan di
kode terkompilasi, maupun penantian di `sleep` atau `wait`. Pembersihan `unwind-protect` yang
sedang berlangsung dijalankan.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` yang ditekan saat mengetik hanya membuang baris yang sedang diketik.

## Menyelidiki setelah galat (`:backtrace`, `:locals`)

Setelah evaluasi berhenti karena galat, `:backtrace` (`:bt`) menampilkan pemanggilan fungsi yang
sedang berlangsung saat itu, mulai dari yang terdalam. `:locals N` menampilkan argumen pemanggilan
ke-N sesuai tipe yang dideklarasikan.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- Untuk pemanggilan dari ekspresi terakhir sebuah fungsi (pemanggilan ekor), fungsi pemanggilnya
  tidak ditampilkan. Karena itulah perulangan rekursif ekor hanya memakai ruang satu pemanggilan,
  berapa kali pun ia berputar.
- Pemanggilan di dalam fungsi terkompilasi dan fungsi anonim (`lambda`) tidak ditampilkan.
- `:locals` hanya menampilkan argumen, bukan variabel yang diikat dengan `let`.
- Mengevaluasi baris berikutnya membuang catatan galat sebelumnya.
