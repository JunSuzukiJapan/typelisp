<!-- translated-from: docs/ja/guide/modules.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Modul dan Tata Letak Berkas

Panduan ini menjelaskan cara menyusun program yang terdiri dari beberapa berkas. Aturan terperinci
ada di bagian 3.10 sampai 3.13 pada [Referensi Sintaks](../reference/syntax.md#310-module--use--ruang-nama).

## 1. Satu berkas adalah satu modul

Di typelisp, **sebuah berkas adalah modul tersendiri**. Path berkas relatif terhadap akar sumber
adalah path modulnya.

| Berkas | Modul |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Tidak perlu menulis deklarasi modul di awal berkas.

## 2. Menyiapkan proyek

Taruh berkas bernama `typelisp.toml` di akar proyek. Isinya boleh kosong.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Untuk menyimpan sumber di bawah `src/`, tulis satu baris ini di `typelisp.toml`:

```toml
src = "src"
```

`typl` mencari `typelisp.toml` mulai dari direktori berkas yang dijalankannya lalu naik ke atas,
dan memakai tempat ditemukannya sebagai akar sumber. Jika tidak ditemukan, direktori berkas yang
dijalankan menjadi akar (di REPL, direktori saat ini).

## 3. Membuat definisi publik dan memakainya

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; a field without pub cannot be read from outside

(defun square ((n i32)) i32 (* n n))   ; a function without pub cannot be called from outside either

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl` dimuat pada titik `(use geometry)` ditulis. Tidak perlu memuatnya lebih dulu.

### Apa saja yang dipublikasikan

- Fungsi, struct, enum, variabel global, makro, dan metode hanya terlihat dari modul lain jika
  diberi `pub`. Taruh `pub` tepat sebelum definisi, seperti pada `(pub defun ...)`.
- Untuk struct, **memublikasikan tipe dan memublikasikan field adalah dua hal yang terpisah**.
  `(pub defstruct point ...)` membuat tipenya terlihat, dan hanya field yang ditulis sebagai
  `(pub x i32)` yang dapat dibaca dan ditulis dari luar.
- Memakai nama yang tidak publik dari luar menghasilkan kesalahan "tidak dapat diselesaikan" seperti
  `unresolved path: geometry::square`. Pesannya sama dengan pesan untuk nama yang salah eja, jadi
  jika ejaannya sudah benar tetapi nama tetap tidak dapat diselesaikan, curigai `pub` yang
  terlewat.

Daftar definisi yang dapat diberi `pub` ada di
[Referensi Sintaks 3.13](../reference/syntax.md#313-pub--visibilitas).

## 4. Cara menulis `use`

```lisp
(use geometry)              ; bring in a module; write geometry::dist2 to use it
(use geometry::dist2)       ; bring in a function; use it by the bare name dist2
(use geometry::point)       ; bring in a type; write point::new, point::origin, and point in type annotations
(use a::f b::g)             ; several can be written together
```

- **`use` hanya berlaku untuk bentuk-bentuk sesudahnya.** Taruh di awal berkas. Menulis
  `geometry::dist2` di atas `use` menghasilkan `unresolved path`.
- Menulis path lengkap `geometry::dist2` tanpa meng-`use` modulnya juga tidak dapat diselesaikan.
  Hanya `use` yang menyebabkan sebuah berkas dimuat.
- Meng-`use` nama yang bentuk polosnya sudah terpakai menghasilkan peringatan. Jika memang ingin
  membawanya masuk, gunakan `shadowing-import`.
- Modul di dalam direktori ditulis `(use geo::shapes)`, dan setelah itu dirujuk dengan bagian
  terakhirnya (`shapes::...`).

### Memanggil metode trait

Metode yang diimplementasikan dalam `impl` **milik tipe**, bukan milik fungsi modul, sehingga
dipanggil tanpa nama modul.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, not core::area
```

Metode di dalam `impl` selalu publik, meskipun tanpa `pub`.

Trait itu sendiri tidak dapat dipublikasikan ke modul lain. Simpan definisi trait, `impl` untuknya,
dan kode yang memakainya melalui `:dyn` dalam satu modul.

## 5. Memisahkan ruang nama di dalam satu berkas

Untuk memisahkan ruang nama lebih lanjut di dalam satu berkas, gunakan `module`. Ia bersarang di
dalam modul milik berkas itu sendiri.

```lisp
;; inside main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Untuk memasukkan seluruh sisa berkas ke satu ruang nama, Anda dapat menulis `(in-module util)`
sebagai ganti membungkusnya dengan tanda kurung.

## 6. Batasan dependensi

- **Siklus tidak diperbolehkan.** Jika `a.typl` melakukan `(use b)` dan `b.typl` melakukan
  `(use a)`, hasilnya adalah kesalahan `circular module dependency: a -> b -> a`. Pindahkan
  definisi yang dibutuhkan keduanya ke modul ketiga.
- **Baik tipe maupun fungsi tidak dapat dirujuk sebelum didefinisikan**, bahkan di dalam berkas yang
  sama. Untuk fungsi yang saling rekursif, deklarasikan salah satunya lebih dulu dengan
  `defsignature` ([Referensi Sintaks 3.2](../reference/syntax.md#32-defsignature--deklarasi-maju)).

## 7. Urutan eksekusi

Menjalankan `typl main.typl` berlangsung dengan urutan berikut:

1. `main.typl` dan setiap berkas yang di-`use` darinya dibaca dan diperiksa tipenya. **Jika ada
   kesalahan tipe di mana pun, tidak ada yang dijalankan.**
2. Ekspresi tingkat atas dari modul yang di-`use` dijalankan sebelum ekspresi dari modul yang
   memakainya.
3. Ekspresi tingkat atas `main.typl` dijalankan dari atas ke bawah.

Jika Anda mengumpulkan titik masuk program ke dalam fungsi `main` dan memanggil `(main)` di akhir
berkas, berkas yang sama juga dapat dipakai untuk
[kompilasi AOT](compile.md#3-membangun-berkas-eksekusi-dengan-kompilasi-aot).

## 8. Perbedaannya dengan `load`

`(load "path")`, seperti `load` pada Common Lisp, membaca isi sebuah berkas **ke dalam ruang nama
saat ini apa adanya**. Isinya tidak dibungkus dalam modul, dan `pub` tidak berperan. Gunakan untuk
hal seperti membaca berkas pengaturan atau memuat ulang berkas lokal di REPL. Untuk membagi program
menjadi beberapa bagian, gunakan `use`.
