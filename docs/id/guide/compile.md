<!-- translated-from: docs/ja/guide/compile.md @ 2951f1d504da73ac227a8215a24999037882fcbc -->
# Kompilasi

Jika Anda tidak melakukan apa-apa lagi, program typelisp berjalan di interpreter. Selain itu ada dua
cara mengompilasi ke kode native, dan satu cara menyimpan lingkungan. Rincian spesifikasinya ada di
[Referensi Sintaks bab 10](../reference/syntax.md#10-kompilasi).

| Metode | Cara | Hasil |
|---|---|---|
| Kompilasi JIT | `(compile name)` | Fungsi pada sesi yang berjalan diganti dengan kode native |
| Kompilasi AOT | `typl -c src.typl` atau `(compile-file "src.typl" "out")` | Berkas eksekusi mandiri |
| Dump | `(dump "file.typld")` | Menyimpan definisi; `typl --image` memulai lagi dari lingkungan yang sama |

## 1. Persiapan

Kompilasi memakai LLVM 22. Jika Anda sudah membangun `typl` dengan mengikuti
[README.md](../../../README.md), tidak diperlukan persiapan lain.

Berkas eksekusi hasil kompilasi AOT ditautkan dengan pustaka statis `libtypelisp_front.a`. Build
rilis `typl` (termasuk yang dipasang dengan `cargo install`) membawa pustaka ini di dalam dirinya,
sehingga tidak diperlukan persiapan. Pada kompilasi pertama, ia menuliskan pustaka itu ke
`~/.typelisp/lib/<ID build>/` (atau `$TYPELISP_HOME/lib/<ID build>/` jika variabel lingkungan
`TYPELISP_HOME` disetel) dan memakai salinan itu seterusnya. `typl --remove-lib` menghapusnya
(dengan `--others`, yang ditulis oleh `typl` versi lain; dengan `--all`, semuanya). Build debug
`typl` memakai pustaka di `target/debug/` pada repositori tempat ia dibangun. Untuk memakai pustaka
yang ditaruh di tempat lain, berikan foldernya dengan `--lib-dir` saat memulai `typl` (bagian 3.2).
Di macOS, penautan memakai Xcode Command Line Tools.

## 2. Kompilasi JIT

Ini mengubah fungsi yang sudah didefinisikan menjadi kode native saat itu juga.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; from here on, calls run the compiled code
```

- `name` tidak dievaluasi. Tulis nama fungsi apa adanya (bukan sebagai string). Untuk metode, tulis
  dengan nama tipe, seperti `(compile point::norm)`.
- Fungsi yang dipanggilnya ikut dikompilasi.
- **Fungsi generik tidak dapat dikompilasi.** Salinan untuk setiap tipe dibuat di tiap tempat ia
  dipakai. Kompilasilah fungsi yang memanggilnya dengan tipe konkret.
- `trace`, `step`, `disassemble`, `compile`, `compile-file`, dan `dump` adalah operasi interpreter,
  sehingga fungsi yang memanggilnya tidak dapat dikompilasi. Mencoba mengompilasinya menghasilkan
  kesalahan yang menyebutkan alasannya.

Untuk melihat hasil kompilasi, gunakan `disassemble`.

```lisp
(disassemble fib)          ; the host machine code
(disassemble fib true)     ; LLVM IR
```

## 3. Membangun berkas eksekusi dengan kompilasi AOT

### 3.1 Menulis programnya

Sebagai titik masuk, definisikan **fungsi `main` yang tidak menerima argumen**.

```lisp
(use std::env::command-line-args)
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

`(main)` di akhir berkas ada agar `main` dipanggil saat Anda menjalankan `typl hello.typl`.
`compile-file` melewati `(main)` terakhir ini, sehingga berkas yang sama bekerja baik di
interpreter maupun dengan kompilasi AOT.

### 3.2 Mengompilasi

Dari baris perintah, gunakan `typl -c` (`typl --compile` sama saja).

```sh
$ typl -c hello.typl            # makes hello
$ typl -c hello.typl -o fib     # names the executable fib
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Tanpa `-o`, berkas eksekusi dinamai sesuai berkas sumber tanpa `.typl` dan ditaruh di folder yang
sama dengan berkas sumber. Jika nama berkas sumber tidak berakhiran `.typl`, `-o` wajib diberikan.
Bersama `-c` (`--compile`), `--image` dan `--feature` tidak dapat diberikan.

Dengan `--release`, `typl` membuat **build rilis**, yang menghapus `eprint`/`eprintln` (keluaran galat standar untuk menyelidiki masalah; [Pencetakan](../reference/functions/printing.md)) beserta argumennya. Yang dihapus tetap diperiksa tipenya, sehingga galat tipe di sana membuat kompilasi gagal. `--release` adalah opsi `typl` secara keseluruhan: berlaku sama bukan hanya untuk `-c`, tetapi juga saat menjalankan file, di REPL, dan untuk `typl test`. `compile-file` yang dipanggil di dalam `typl` yang dijalankan dengan `--release` juga membuat build rilis, dan `eprint`/`eprintln` juga dihapus dari kode yang dijalankan dengan `eval` di dalam berkas eksekusi yang dibuat dengan cara itu.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # sama saja jika ditulis setelah -c
```

`-O0`, `-O1`, `-O2`, dan `-O3` menetapkan **tingkat optimasi**. Bawaannya `-O0`, yang tidak mengoptimasi. Mulai `-O1`, kompiler lebih dulu melakukan optimasi sederhana, seperti mengganti variabel yang hanya diikat ke sebuah literal dengan nilainya atau menghitung lebih awal aritmetika bilangan bulat pada konstanta, lalu menyerahkan sisanya ke optimasi LLVM pada tingkat dengan nama yang sama. Di setiap tingkat hasil perhitungannya sama. Seperti `--release`, ini adalah opsi `typl` secara keseluruhan: selain untuk file eksekusi yang dibuat `-c`, opsi ini berlaku untuk kode yang dikompilasi saat menjalankan file, di REPL, dan di `typl test`, serta untuk `compile-file` yang dipanggil di sana. Kode yang dijalankan interpreter tidak dioptimasi.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # sama saja jika ditulis setelah -c
```

Anda dapat melakukan hal yang sama dengan memanggil `compile-file` dari REPL atau dari program.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Jika Anda membangun berulang kali, Anda dapat menaruh satu baris ini di sebuah berkas dan
menjalankannya dengan `typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Nama berkas diselesaikan dari **direktori saat ini tempat `typl` dimulai**, bukan dari lokasi
`build.typl`.

Untuk menautkan `libtypelisp_front.a` yang ditaruh di tempat selain yang dicari `typl`, berikan
foldernya dengan `--lib-dir`. Ini berlaku untuk `typl -c` maupun `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Jika folder yang diberikan tidak memiliki `libtypelisp_front.a`, `typl` berhenti dengan kesalahan.
Berkas itu hanya bekerja dengan `typl` yang dibangun bersamanya. Setelah membangun ulang `typl`,
salin lagi.

### 3.3 Isi yang diperbolehkan pada berkas hasil kompilasi AOT

- Tingkat atas berkas masuk hanya boleh berisi definisi (`defun` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) serta `use` dan `module`. Ekspresi tingkat atas
  seperti `(println ...)` tidak diperbolehkan, kecuali `(main)` terakhir. Taruh pekerjaannya di
  dalam `main`.
- Tanpa `main` yang tidak menerima argumen, kompilasi gagal dengan kesalahan.
- Berkas dari modul yang di-`use` ikut dikompilasi dan digabung menjadi satu berkas eksekusi.
- Pustaka yang disebut dengan `:library` pada `defffi` ditautkan secara otomatis ([FFI C](ffi.md)).
- Semua fungsi pustaka standar dapat dipakai dengan kompilasi AOT. `eval` juga dapat dipakai, tetapi
  dalam hal itu pemeriksa tipe dan interpreter masuk ke dalam berkas eksekusi, sehingga ukurannya
  lebih besar dan startnya lebih lambat. Program yang tidak memanggil `eval` tidak menyertakannya.

### 3.4 Perilaku berkas eksekusi

- `(command-line-args)` mengembalikan `Vector<string>` dengan bentuk yang sama baik dijalankan
  sebagai `typl hello.typl a b` maupun `./hello a b`. Elemen pertama adalah nama program.
- Setel kode keluar dengan `(exit n)`. Jika `main` kembali secara normal, kodenya 0.
- Pada `panic`, program mencetak pesan dan keluar dengan kode bukan nol.

### 3.5 Sumber daya yang dipakai berkas eksekusi

`typl -c` punya opsi yang menentukan sumber daya apa yang boleh dipakai berkas eksekusi saat berjalan.

```sh
$ typl --heap-cells 100000 -c hello.typl    # tetapkan arena cons pada 100000 sel
$ typl -c hello.typl --max-memory 64M       # alokasikan paling banyak 64 MiB
$ typl -c hello.typl --no-cons              # tidak memakai sel cons
```

- `--heap-cells N` menetapkan kapasitas arena sel cons pada `N` sel. Arena tidak tumbuh melebihinya: jika pengumpulan sampah tidak bisa membebaskan satu sel pun, program berhenti dengan galat waktu jalan. Tanpa opsi ini, arena dimulai dengan 65536 sel dan tumbuh sesuai kebutuhan.
- `--max-memory SIZE` membatasi total memori yang dialokasikan berkas eksekusi. Semuanya dihitung: arena cons, string, nilai seperti vektor dan tabel hash, serta memori yang dipakai runtime itu sendiri. `SIZE` adalah jumlah byte dan boleh diikuti `K`, `M`, atau `G` (1024, 1024², dan 1024³ kali). Alokasi yang akan melewati batas dilaporkan, dan program keluar dengan kode keluar 1. Keluar seperti ini tidak dapat ditangkap dengan `catch`. Tanpa opsi ini, tidak ada batas.
- `--no-cons` membuat berkas eksekusi yang tidak memakai sel cons. Ia tidak punya arena cons, jadi program yang melakukan sesuatu yang memerlukan sel cons menjadi **galat kompilasi**, yang menampilkan operasi itu dan rangkaian fungsi dari `main` yang mencapainya. Sel cons diperlukan untuk membuat list `Sexpr` (termasuk list yang di-quote dan yang dibuat dengan quasiquote) atau argumen `&rest`, serta oleh `read`, `eval`, dan `macroexpand`. Operasi pada angka, string, vektor, tabel hash, struct, kanal, dan sejenisnya, serta `print`/`println`/`format`, tidak memakainya. Tidak dapat diberikan bersama `--heap-cells`, dan ditulis setelah `-c`.

`--max-memory` ditulis setelah `-c`. `--heap-cells` boleh ditulis sebelum atau setelah `-c`, dan menjadi kapasitas arena cons berkas eksekusi. Arena cons ikut dihitung dalam batas memori, jadi dengan batas yang tidak memuat arena, program berhenti sebelum mulai.

### 3.6 Mengompilasi untuk WebAssembly

Dengan `--target wasm32-wasip1`, `typl -c` membuat modul WebAssembly (perintah WASI preview 1) alih-alih berkas eksekusi. Tanpa `-o`, namanya adalah nama sumber dengan `.typl` diganti `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # membuat hello.wasm
$ wasmtime hello.wasm a b                        # jalankan dengan lingkungan yang mendukung WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- Ini memerlukan persiapan dan hanya bekerja dengan build debug `typl` yang dibangun di repositori.
  Jalankan `rustup target add wasm32-wasip1 wasm32-unknown-unknown` dengan rustup, lalu jalankan `scripts/build-wasm-runtime.sh` di repositori. Ini juga menyiapkan bagian 3.7.
- Modul berperilaku seperti berkas eksekusi: keluaran standar, galat standar, dan kode keluar, `(command-line-args)`,
  membaca dan menulis berkas, `throw`/`catch`/`panic`, tugas, kanal, dan `sleep`, serta `eval` berfungsi.
  Tugas berjalan bergantian dalam satu thread. Berkas hanya bisa dibaca dan ditulis di dalam direktori
  yang diperlihatkan lingkungan kepada modul (pada wasmtime, ditentukan dengan `--dir`).
- Program yang memakai sesuatu yang tidak dimiliki WebAssembly adalah **galat kompilasi**. Galat menampilkan apa yang dipakai
  dan rantai fungsi dari `main` yang mencapainya. Yang tidak bisa dipakai: soket (TCP, UDP, domain Unix,
  TLS, resolusi nama), thread OS (`Thread`), fungsi C (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`,
  zona waktu lokal, dan `ed`.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>`, dan `--release` berlaku seperti untuk berkas eksekusi.

### 3.7 Menjalankan di peramban atau Node.js

Dengan `--target wasm32-unknown-unknown`, `typl -c` membuat modul WebAssembly yang dimuat dan dijalankan dari JavaScript. Di samping modul (`hello.wasm`), di folder yang sama, dibuat JavaScript yang memuat dan menjalankannya (`hello.mjs`). Persiapannya sama dengan bagian 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # membuat hello.wasm dan hello.mjs
$ node hello.mjs                                       # jalankan dengan Node.js
args: #()
fib(25) = 75025
```

Dari JavaScript lain, panggil `run` dari `hello.mjs`. `write` menerima keluaran sebagai teks (`fd` bernilai 1 untuk keluaran standar dan 2 untuk galat standar). Nilai `run` adalah kode keluar.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* misalnya tampilkan di halaman */ } });
```

- Tanpa `write`, Node.js menulis ke keluaran standar dan galat standar, sedangkan peramban ke konsol, baris demi baris.
- Di peramban, sajikan `hello.wasm` lewat HTTP dari tempat yang sama dengan `hello.mjs` (tidak bisa dimuat dari `file://`).
- `sleep` menunggu dengan `Atomics.wait`. Peramban hanya mengizinkannya di dalam Worker pada halaman yang terisolasi lintas asal, jadi jalankan program yang memakai `sleep` di dalam Worker.
- Selain yang tidak bisa dipakai di bagian 3.6, operasi berkas dan direktori adalah **galat kompilasi**. Masukan standar terbaca kosong, dan `(command-line-args)` adalah vektor kosong.

## 4. Dump

Anda dapat menyimpan definisi sesi saat ini ke satu berkas dan memulai darinya lain kali.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

Ini juga berlaku untuk menjalankan berkas, seperti `typl --image session.typld prog.typl`.

- Yang disimpan adalah **definisi**. Ekspresi yang dievaluasi di sesi tidak disimpan.
- Fungsi yang Anda `compile` disimpan dalam bentuk terkompilasinya.
- Variabel global dipulihkan dengan **menjalankan ulang penginisialisasinya**, bukan dengan nilai yang
  dimilikinya saat dump ditulis.
- Dump tidak dapat dimuat oleh `typl` yang versinya berbeda dari yang menulisnya (itu adalah
  kesalahan).

Jika Anda menjalankan sebuah berkas dan melakukan `(dump ...)` darinya, definisi berkas itu berada
di modul yang dinamai sesuai berkas. Fungsi yang didefinisikan di `dp.typl` bernama `dp::sq`, dan
memanggilnya dari berkas lain memerlukan `pub` ([Modul dan Tata Letak Berkas](modules.md)).

## 5. Tentang berkas modul terkompilasi

Tidak ada format seperti `.fasl` pada Common Lisp untuk menulis hasil kompilasi tiap modul ke
sebuah berkas. `compile-file` membangun berkas eksekusi langsung dari sumbernya. Tidak ada berkas
perantara yang tertinggal.
