<!-- translated-from: docs/ja/guide/compile.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
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
Bersama `-c` (`--compile`), `--image`, `--heap-cells`, dan `--feature` tidak dapat diberikan.

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

- Tingkat atas berkas masuk hanya boleh berisi definisi (`defun` `defmethod` `defvar`
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
