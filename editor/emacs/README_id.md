<!-- translated-from: editor/emacs/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-mode (Emacs)

Major mode Emacs untuk menyunting kode sumber typelisp (`.typl`).
Versi VS Code ada di [../vscode/](../vscode/README_id.md). Keduanya memakai tabel keyword yang sama
dan aturan indentasi yang sama, dan `cargo test --test editor_keyword_sync_test` memeriksanya secara
mekanis (lihat akhir dokumen ini).

## Fitur

- Penyorotan sintaks
  - Bentuk khusus dan konstruksi kendali (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, keluarga `pprint`, dan sebagainya)
  - Nama yang didefinisikan (`NAME` pada `(defun NAME ...)` sebagai nama fungsi, pada
    `(defstruct NAME ...)` sebagai nama tipe, dan pada `(defvar (NAME ...))` sebagai nama variabel;
    sama dengan `pub`, seperti `(pub defun NAME ...)`)
  - Keyword ruang nama dan deklarasi (`pub` `module` `use` `load` `impl` `where`) dan penanda daftar
    lambda (`&rest` `&optional` `&key`)
  - Fungsi bawaan (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` dan sebagainya)
  - Tipe primitif (termasuk `bignum` / `ratio`), tipe bawaan, tipe kesalahan bawaan
    (`ParseIntError` dan sebagainya), tipe buatan pengguna berawalan huruf kapital (`Capitalized`),
    dan tipe objek trait `:dyn Trait`
  - **Pemakaian tipe buatan pengguna** (nama `defstruct`/`defenum`/`deftrait` biasanya huruf kecil
    (`rect` `todo-item` `board`), sehingga aturan `Capitalized` tidak menangkapnya). Ketika
    terhubung ke `typl-lsp`, nama itu diberi warna dari semantic token server (ini juga berfungsi
    dengan `eglot`; lihat di bawah). Ketika tidak terhubung, mode kembali mengumpulkan nama tipe yang
    didefinisikan di buffer
  - Literal (`true` `false`, literal bilangan (desimal / `0xff` / `1.5` / `1/3`), literal karakter
    seperti `#\Space`, string, keyword seperti `:name`)
  - Direktif kendali `format` di dalam string (`~a` `~5,'0d` `~{...~}` dan sebagainya)
  - Variabel global gaya CL dengan earmuff (`*print-pretty*` dan sebagainya)
- Komentar
  - Komentar baris `;`
  - Komentar blok `#| ... |#` yang **dapat bersarang**
- Navigasi S-expression dan indentasi gaya Lisp
- Indeks definisi melalui `imenu` (fungsi / metode / makro / tipe / trait / `impl` / variabel /
  modul)
- Perintah yang menjalankan CLI `typl` (di bawah)

## Pintasan papan ketik

| Tombol | Perintah | Fungsinya |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Menyimpan dan menjalankan `typl FILE` (melalui `compile`, sehingga Anda dapat melompat ke baris kesalahan) |
| `C-c C-z` | `typelisp-repl` | Memulai REPL `typl` dalam buffer comint |

Setel lokasi `typl` dengan `typelisp-program` (bawaan `"typl"`).
Diagnostik berbentuk `error: FILE:LINE:COL: ...`, yang dapat diurai `compilation-mode`, sehingga
`next-error` / `C-x \`` langsung melompat ke tempatnya.

## Pemasangan

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Berkas `.typl` terbuka dalam `typelisp-mode` secara otomatis (mode didaftarkan di
`auto-mode-alist`).

Dengan `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Language Server (`typl-lsp`)

Setelah `typl-lsp` dibangun, ia dapat dipakai dari `eglot` (bawaan Emacs 29+) atau `lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

Dengan `eglot`:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Dengan `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Yang didukung: diagnostik (kesalahan sintaks/tipe dan peringatan definisi ulang, dikirim melalui
`textDocument/publishDiagnostics`), hover, lompat ke definisi, pelengkapan (`:` didaftarkan sebagai
karakter pemicu), dan semantic token. Rujukan antarberkas melalui `use` diselesaikan (server mencari
ke atas `typelisp.toml` di akar proyek; untuk rinciannya lihat
[Referensi Sintaks 3.11](../../docs/id/reference/syntax.md#311-berkas-dan-modul-proyek-multiberkas)).
Suntingan yang belum disimpan pada buffer editor yang terbuka langsung tercermin dalam diagnostik
baik berkas yang menjadi dependensinya maupun berkas yang bergantung padanya.

### Penyorotan nama tipe (semantic token)

Melalui `textDocument/semanticTokens`, server melaporkan **posisi yang benar-benar diselesaikan
pemeriksa sebagai nama tipe**. Karena ini bukan pencocokan teks:

- Tipe yang berasal dari berkas lain melalui `use` juga diberi warna (rentang yang pada prinsipnya
  tidak terjangkau oleh resolusi di dalam buffer)
- Pemanggilan **fungsi** yang bernama sama dengan sebuah tipe tidak diberi warna (pemeriksa
  menyelesaikannya sebagai fungsi, sehingga sejak awal tidak ada token yang dicatat di sana)

Di sisi klien:

- **`eglot` (Emacs 31 ke atas)**: eglot menggambar token itu sendiri
  (`eglot-semantic-tokens-mode`). `typelisp-mode` menyingkir
- **`eglot` (Emacs 30 ke bawah)**: eglot versi ini tidak menangani semanticTokens. Jadi
  **`typelisp-mode` mengirim permintaan sendiri dan menggambar hasilnya dengan overlay**
  (`typelisp-semantic-tokens-mode`, dinyalakan otomatis ketika eglot terhubung)
- **`lsp-mode`**: dukungan bawaan (setel `lsp-semantic-tokens-enable` ke `t`). Dalam hal itu
  `typelisp-mode` menyingkir

`scripts/emacs-semantic-smoke.el` terhubung melalui eglot secara nyata dan memeriksa sisi yang
menggambar pada Emacs yang dipakai. Dengan klien apa pun, fallback di dalam buffer mundur selama
server menjawab (agar dua perangkat aturan tidak mewarnai buffer yang sama).

| Pengaturan | Bawaan | Fungsinya |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Pada eglot Emacs 30 ke bawah, apakah mewarnai dari semantic token server |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Detik menganggur setelah suntingan sebelum meminta lagi (jaga agar lebih besar dari `eglot-send-changes-idle-time`) |

## Catatan

- typelisp mengubah simbol menjadi huruf kecil saat membaca, tetapi penyorotan peka huruf besar-kecil
  agar nama tipe yang diawali huruf kapital dapat dibedakan.
- Indentasi ditentukan oleh `typelisp-indent-function` khusus, yang mencari
  `typelisp-indent-specs` (sebuah alist). Mode ini memelihara entrinya sendiri bahkan untuk bentuk
  yang namanya sama dengan Emacs Lisp (`defun` `let` `if` ...) karena properti simbol bersifat
  **global**, dan pengaturan typelisp di sana akan mengubah indentasi buffer Lisp lain dalam sesi
  yang sama. Dan bentuk typelisp berbeda bentuknya meskipun namanya sama dengan Emacs Lisp:
  `(defun NAME (PARAMS) RETTYPE ...)` memiliki tiga elemen header, dan `if` ditetapkan tiga elemen
  dengan `else` yang wajib. Jadi nilainya juga tidak dapat dibagi. Setiap berkas `.typl` di bawah
  `examples/` telah diperiksa: `indent-region` tidak mengubah satu byte pun, dan meratakan semua
  indentasi lalu mengindentasi ulang memulihkan aslinya (versi VS Code memenuhi standar yang sama
  pada berkas yang sama).

## Mendeteksi penyimpangan pada definisi editor

Tabel keyword dipelihara dua kali, sekali di sini dan sekali pada versi VS Code. Untuk mencegah
definisi editor tertinggal ketika implementasi terus bergerak, ada pengujian di sisi Rust:

```sh
cargo test --test editor_keyword_sync_test
```

Pengujian itu benar-benar memuat prelude, menelusuri registry, dan melaporkan **nama yang tidak
dikenal oleh salah satu editor**. Bentuk khusus tidak memiliki representasi saat dijalankan, sehingga
dibaca dari antara `// SPECIAL-FORM DISPATCH BEGIN` / `END` di
`crates/typelisp-front/src/check/checker.rs` (jangan hapus komentar ini). Jika gagal, tambahkan nama
yang dilaporkan ke **kedua** definisi editor.

Pengujian yang sama juga membandingkan legenda semantic token (`SEMANTIC_TOKEN_TYPES` di
`src/bin/lsp.rs` dan tabel yang dimiliki kedua editor harus sepakat dalam nama dan urutan).
Ketidakcocokan tidak menyebabkan kesalahan saat dijalankan; ia hanya menukar warna setiap token,
sehingga dikunci secara mekanis.
