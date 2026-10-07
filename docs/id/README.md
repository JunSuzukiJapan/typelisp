<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Dokumentasi typelisp (Bahasa Indonesia)

typelisp adalah Lisp dengan tipe statis. Untuk cara memasang dan membangunnya, lihat
[README.md](../../README.md) (dalam bahasa Inggris) di direktori teratas repositori.

## Tutorial

Jika Anda baru mengenal typelisp, bacalah berurutan.

- [Memulai](tutorial/intro.md): REPL, fungsi, variabel, percabangan, perulangan, daftar, dan `Vector`
- [Dasar-dasar Tipe](tutorial/types.md): tipe statis, `Option`, `Result`, struct, enum, generik
- [Trait](tutorial/traits.md): `deftrait` / `impl`, batas trait, `:dyn`
- [Makro](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Penanganan Kesalahan](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Konkurensi](tutorial/concurrency.md): task, kanal, `select`, `Mutex`, `thread`

## Panduan

- [Modul dan Tata Letak Berkas](guide/modules.md): `use`, `pub`, cara berkas dipetakan ke modul
- [Kompilasi](guide/compile.md): JIT, membangun berkas eksekusi dengan kompilasi AOT, dump
- [I/O Berkas, Stream, dan Jaringan](guide/io.md): berkas, pathname, TCP / TLS / UDP, resolusi nama
- [FFI C](guide/ffi.md): memanggil fungsi C dengan `defffi` (termasuk callback dan struct C dengan `def-c-struct`)
- [Integrasi Editor](guide/editors.md): `typl-lsp` dan cara menyiapkan VS Code / Emacs
- [Untuk Pemrogram Common Lisp](guide/from-common-lisp.md): perbedaan typelisp dengan CL dan cara menulis ulang kode CL

## Referensi

- [Referensi Sintaks](reference/syntax.md): sintaks leksikal, penulisan tipe, definisi, bentuk kendali, kompilasi, konkurensi
- [Fungsi Bawaan](reference/functions/README.md): fungsi bawaan, metode, dan pustaka standar
- [Tipe](reference/types.md): daftar tipe beserta trait yang diimplementasikan masing-masing
- [Pesan Kesalahan](reference/errors.md): arti kesalahan yang umum dan cara memperbaikinya

## Integrasi Editor

Langkah penyiapan ada di [panduan Integrasi Editor](guide/editors.md). Pintasan papan ketik dan
pengaturan tiap editor dicantumkan dalam dokumen berikut:

- [Emacs (typelisp-mode)](../../editor/emacs/README_id.md)
- [VS Code](../../editor/vscode/README_id.md)
