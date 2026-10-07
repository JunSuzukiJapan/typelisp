<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Integrasi Editor (typl-lsp)

`typl-lsp` adalah language server untuk typelisp. Jika dihubungkan ke editor yang mendukung LSP
(Language Server Protocol), ia menyediakan fitur berikut untuk berkas yang sedang Anda sunting:

- Diagnostik: kesalahan pembacaan, kesalahan tipe, dan peringatan definisi ulang
- Hover: tipe dari ekspresi berkurung dan docstring dari definisi yang dipanggilnya (tidak
  ditampilkan untuk nama variabel tunggal)
- Lompat ke definisi
- Pelengkapan (kandidat muncul saat Anda mengetik `:`)
- Pewarnaan nama tipe (semantic token), termasuk tipe yang di-`use` dari berkas lain

Rujukan antarberkas melalui `use` diselesaikan. Suntingan yang belum disimpan pada berkas lain yang
sedang terbuka langsung tercermin dalam diagnostik berkas yang meng-`use`-nya.

## 1. Membangun

```sh
cargo build --release --bin typl-lsp
```

Perintah ini menghasilkan `target/release/typl-lsp`. Jika Anda memasang dengan `cargo install`
seperti dijelaskan di [README.md](../../../README.md), berkas itu ada di `~/.cargo/bin/typl-lsp`
bersama `typl`.

## 2. VS Code

Ekstensinya ada di `editor/vscode` dalam repositori. Ekstensi ini tidak dipublikasikan di
Marketplace, jadi bangun dan pasanglah sendiri.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # menghasilkan berkas .vsix
```

Pilih "Install from VSIX..." dari menu "..." pada tampilan Extensions, lalu pilih berkas `.vsix`
yang telah Anda bangun.

Ekstensi mencari `typl-lsp` di `target/release/typl-lsp` pada workspace, lalu
`target/debug/typl-lsp`, lalu di `PATH`. Jika Anda menaruhnya di tempat lain, tuliskan path-nya
pada pengaturan `typelisp.languageServer.path`.

| Pengaturan | Bawaan | Arti |
|---|---|---|
| `typelisp.program` | `typl` | Path `typl` |
| `typelisp.languageServer.enable` | `true` | Apakah terhubung ke `typl-lsp` |
| `typelisp.languageServer.path` | (kosong) | Path `typl-lsp` |

`Ctrl+Alt+R` menyimpan berkas yang sedang disunting lalu menjalankannya dengan `typl`, dan
`Ctrl+Alt+Z` memulai REPL. Selengkapnya, lihat
[README ekstensi VS Code](../../../editor/vscode/README_id.md).

## 3. Emacs

`typelisp-mode` ada di `editor/emacs` dalam repositori.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Pengaturan untuk menghubungkan ke `typl-lsp` dengan `eglot` (sudah termasuk dalam Emacs 29 ke atas):

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

Eglot pada Emacs 31 ke atas mewarnai nama tipe (semantic token) sendiri. Eglot pada Emacs 30 ke
bawah tidak mendukungnya, sehingga `typelisp-mode` yang mewarnai nama tipe sebagai gantinya. Dengan
`lsp-mode`, setel `lsp-semantic-tokens-enable` ke `t`.

`C-c C-c` menjalankan berkas yang sedang disunting, dan `C-c C-z` memulai REPL. Selengkapnya,
lihat [README typelisp-mode](../../../editor/emacs/README_id.md).

## 4. Editor lain

`typl-lsp` berbicara LSP melalui masukan dan keluaran standar dan tidak menerima argumen baris
perintah. Atur klien LSP editor Anda agar menjalankan `typl-lsp` untuk berkas `.typl`.

## 5. Cara proyek dikenali

`typl-lsp` mencari `typelisp.toml` mulai dari direktori berkas yang dibuka lalu naik ke atas,
dan menyelesaikan `use` dengan tempat itu sebagai akar sumber. Aturannya sama dengan saat `typl`
menjalankan sebuah berkas ([Modul dan Tata Letak Berkas](modules.md#2-menyiapkan-proyek)). Untuk
proyek yang terdiri dari beberapa berkas, taruh `typelisp.toml` di akarnya.

## 6. Language server tidak menjalankan program Anda

`typl-lsp` menghasilkan diagnostik hanya dengan membaca dan memeriksa tipe. Ia tidak pernah
menjalankan program yang sedang Anda sunting. Diagnostik berjalan pada setiap ketukan tombol,
sehingga tidak mungkin menjalankan kode yang punya efek samping atau kode yang tidak pernah selesai
di sana. Satu-satunya pengecualian adalah pendaftaran `defmacro`, yang diperlukan untuk memeriksa
pemanggilan makro yang muncul sesudahnya.

Karena itu, kesalahan yang hanya terjadi saat `typl` menjalankan program (`panic`, berkas yang
tidak ada, dan sebagainya) tidak muncul dalam diagnostik language server.
