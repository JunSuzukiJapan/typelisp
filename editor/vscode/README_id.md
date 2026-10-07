<!-- translated-from: editor/vscode/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp (VS Code)

Ekstensi VS Code untuk menyunting kode sumber typelisp (`.typl`).
Versi Emacs ada di [../emacs/](../emacs/README_id.md). Keduanya memakai tabel keyword yang sama dan
aturan indentasi yang sama, dan `cargo test --test editor_keyword_sync_test` memeriksanya secara
mekanis (lihat di bawah).

## Fitur

- **Penyorotan sintaks** (grammar TextMate; tidak memerlukan language server)
  - Bentuk khusus dan konstruksi kendali (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, keluarga `pprint`)
  - Nama yang didefinisikan (`(defun NAME ...)` sebagai fungsi, `(defstruct NAME ...)` sebagai tipe,
    `(defvar (NAME ...))` sebagai variabel; sama dengan `pub`, seperti `(pub defun NAME ...)`) dan
    kedua nama pada `(impl Trait Type)`
  - Keyword ruang nama dan deklarasi (`pub` `module` `use` `load` `impl` `where`) dan penanda daftar
    lambda (`&rest` `&optional` `&key`)
  - Fungsi bawaan, tipe primitif (termasuk `bignum` / `ratio`), tipe kesalahan bawaan, tipe
    buatan pengguna berawalan huruf kapital (`Capitalized`), dan tipe objek trait `:dyn Trait`
    (juga di dalam argumen generik)
  - Literal bilangan (desimal / `0xff` / `1.5` / `3.0e10` / `1/3`), literal karakter seperti
    `#\Space`, keyword seperti `:name`, dan variabel global dengan earmuff seperti
    `*print-pretty*`
  - **Direktif kendali `format` di dalam string** (`~a` `~5,'0d` `~{...~}` `~^` dan sebagainya)
  - Komentar baris `;` dan komentar blok `#| ... |#` yang **dapat bersarang**
- **Pemakaian tipe buatan pengguna** (semantic token)
  - Nama `defstruct` / `defenum` / `deftrait` biasanya huruf kecil (`rect` `todo-item` `board`),
    sehingga aturan `Capitalized` tidak menangkapnya, dan grammar TextMate bekerja baris demi baris
    serta tidak dapat melihat seluruh berkas. Semantic token dapat, yang memperbaiki keadaan ketika
    bahasa bertipe statis hanya menyisakan anotasi tipenya tanpa warna
  - Ketika terhubung ke `typl-lsp`, ekstensi menerima **posisi yang benar-benar diselesaikan
    pemeriksa sebagai nama tipe**. Jadi tipe yang berasal dari berkas lain melalui `use` diberi
    warna, dan pemanggilan **fungsi** yang bernama sama dengan sebuah tipe tidak (pemeriksa
    menyelesaikannya sebagai fungsi, sehingga sejak awal tidak ada token yang dicatat di sana)
  - Ketika server tidak terhubung atau belum dibangun, ekstensi kembali ke pemindaian teks yang
    menyelesaikan di dalam berkas. Itu hanya aproksimasi: ia tidak dapat menemukan tipe dari berkas
    lain, dan tidak dapat membedakan fungsi yang bernama sama dengan sebuah tipe
- **Indentasi Lisp** (VS Code tidak memiliki indentasi Lisp bawaan, sehingga ekstensi
  mengimplementasikannya)
  - Format Document, Format Selection, dan format on type (Enter dan `)`, ketika
    `editor.formatOnType` diaktifkan)
- **Outline / breadcrumb / `Ctrl+Shift+O`** (fungsi, metode, makro, tipe, trait, `impl`,
  variabel, modul)
- **Integrasi `typl-lsp`** (diagnostik, hover, lompat ke definisi, pelengkapan, semantic token)
- **Perintah CLI `typl`** (menjalankan, REPL)

Semuanya kecuali language server bekerja dengan ekstensi saja, sehingga bahkan pada checkout tempat
`typl-lsp` belum dibangun, penyorotan, indentasi, Outline, dan penyorotan tipe (tingkat berkas) tetap
tersedia.

## Pemasangan

Ekstensi tidak ada di Marketplace, jadi bangun secara lokal lalu pasang.

```sh
cd editor/vscode
npm install
npm run compile
```

Lalu pilih salah satu:

- **Mencobanya di development host**: buka `editor/vscode` di VS Code dan tekan `F5`
- **Memasangnya secara permanen**: buat `.vsix` dengan `npx @vscode/vsce package`, lalu gunakan
  "..." → "Install from VSIX..." pada tampilan Extensions

Berkas `.typl` terbuka dalam mode typelisp secara otomatis.

## Pintasan papan ketik

| Tombol | Perintah | Fungsinya |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Menyimpan dan menjalankan `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | Memulai REPL `typl` |

Palet perintah juga memiliki `typelisp: Restart Language Server`.

## Pengaturan

| Pengaturan | Bawaan | Fungsinya |
|---|---|---|
| `typelisp.program` | `typl` | Path CLI `typl` |
| `typelisp.languageServer.enable` | `true` | Apakah terhubung ke `typl-lsp` |
| `typelisp.languageServer.path` | (kosong) | Path `typl-lsp`. Jika kosong, ekstensi mencari `target/release/typl-lsp` pada workspace, lalu `target/debug/typl-lsp`, lalu `PATH` |
| `typelisp.trace.server` | `off` | Mencatat lalu lintas JSON-RPC LSP |

Bangun language server dengan:

```sh
cargo build --release --bin typl-lsp
```

Rujukan antarberkas melalui `use` diselesaikan dengan mencari ke atas `typelisp.toml` di akar proyek
(untuk rinciannya lihat
[Referensi Sintaks 3.11](../../docs/id/reference/syntax.md#311-berkas-dan-modul-proyek-multiberkas)).

## Problem matcher untuk task

Ekstensi menyediakan problem matcher bernama `typelisp`. `typl` mencetak diagnostik dalam bentuk
`error: FILE:LINE:COL: message`, sehingga dapat langsung masuk ke panel Problems:

```jsonc
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "typl: run",
      "type": "shell",
      "command": "typl ${file}",
      "problemMatcher": "$typelisp"
    }
  ]
}
```

## Pengembangan

```sh
npm run compile   # tsc
npm run watch     # build on change
npm test          # node --test (grammar, indentation, symbols, type references, manifest)
```

Pengujian hanya mencakup bagian yang tidak memerlukan modul `vscode`. Untuk itu, `src/indent.ts` dan
`src/symbols.ts` ditulis sebagai fungsi murni, dan hanya `src/extension.ts` yang menyentuh API
editor.

- `src/test/grammar.test.ts` — melakukan tokenisasi dengan grammar secara nyata, memakai mesin yang
  sama seperti VS Code (`vscode-textmate` + `vscode-oniguruma`), dan memeriksa hasilnya. Oniguruma
  berbeda dari ekspresi reguler Emacs dalam rincian (misalnya, ia tidak memperlakukan `]` di awal
  kelas karakter sebagai literal), dan perbedaan seperti itu hanya ditemukan dengan menjalankan
  mesin yang sebenarnya.
- `src/test/indent.test.ts` — untuk setiap berkas `.typl` di bawah `examples/`, mensyaratkan bahwa
  **meratakan semua indentasi lalu memulihkannya sama dengan isi yang di-commit byte demi byte**.
  Mode Emacs memenuhi standar yang sama pada berkas yang sama, dan itulah yang menjadikan "kedua
  editor sepakat" sebuah klaim yang terverifikasi. Selain itu,
  `src/test/fixtures/emacs-indent-reference.txt` adalah keluaran acuan yang dikumpulkan dengan
  benar-benar menjalankan `indent-region` pada buffer `typelisp-mode` di Emacs. Sisi yang diharapkan
  bukan pengulangan implementasi TS melainkan **apa yang benar-benar dihasilkan editor lain**,
  sehingga kesetiaan port diperiksa secara langsung (mencakup `let*` `do` `doiter` `labels` `impl`
  `pprint-logical-block`, awalan quote, dan lainnya).
- `src/test/symbols.test.ts` — isi Outline dan deteksi rujukan tipe pada fallback. Jumlah definisi
  harus persis sama dengan hitungan independen bentuk definisi di awal baris. Aturan batas untuk
  rujukan tipe sengaja disejajarkan dengan fallback versi Emacs (VS Code memakai lookbehind; Emacs
  menyatakan himpunan yang sama dengan mengonsumsi satu karakter sebelumnya).
- Token berbasis resolusi dari server (`crates/typelisp-front/src/check/semantic.rs`) diperiksa oleh
  `cargo test --test lsp_semantic_test` dan `scripts/lsp-semantic-smoke.py` (yang menggerakkan proses
  nyata melalui stdio). Klien Emacs diperiksa oleh `scripts/emacs-semantic-smoke.el` melalui koneksi
  eglot yang nyata.
- `src/test/manifest.test.ts` — `package.json` adalah satu-satunya bagian yang tidak diperiksa
  kompiler, sehingga pengujian ini memeriksa bahwa perintah yang dideklarasikan dan pemanggilan
  `registerCommand` adalah himpunan yang sama, apa yang dirujuk pintasan papan ketik, bahwa
  pengaturan yang dibaca kode dideklarasikan, dan bahwa problem matcher dapat mengurai apa yang
  benar-benar dicetak `typl`.

### Mendeteksi penyimpangan pada definisi editor

Tabel keyword dipelihara dua kali, pada versi Emacs dan versi VS Code. Untuk mencegah definisi editor
tertinggal ketika implementasi terus bergerak, ada pengujian di sisi Rust:

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

## Catatan

- typelisp mengubah simbol menjadi huruf kecil saat membaca, tetapi penyorotan peka huruf besar-kecil
  agar nama tipe yang diawali huruf kapital dapat dibedakan.
- Indentasi ditentukan oleh `INDENT_SPECS` di `src/indent.ts`. Ia adalah port dari
  `typelisp-indent-specs` versi Emacs, dengan nilai dan aturan yang sama. Tempat sebuah bentuk
  berbeda bentuknya dari bentuk Emacs Lisp bernama sama dibawa apa adanya: header
  `(defun NAME (PARAMS) RETTYPE ...)` memiliki tiga elemen, `if` ditetapkan tiga elemen dengan
  `else` yang wajib, dan seterusnya.
- Isi `#| ... |#` diindentasi ulang saat memformat. Ini sesuai dengan perilaku `indent-region`
  pada Emacs.
