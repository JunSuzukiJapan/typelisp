<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Task dan Kanal

Kosakata task (thread ringan). `task` dan `thread`, yang memulainya, dan `select`, yang menunggu
beberapa hal, adalah bentuk khusus dan ada di [Referensi Sintaks](../syntax.md#12-konkurensi-task).
Bab ini membahas sisanya: tipe, metode, dan fungsi.

Task bersifat **kooperatif**: sebuah task berpindah hanya pada titik yang Anda tulis. Task berjalan
pada saat yang sama di `TYPELISP_THREADS` thread OS (di `typl`, hanya task terkompilasi yang keluar
ke thread lain). Tempat task berpindah dan perbedaannya dengan Go ada di
[Referensi Sintaks 12.5](../syntax.md#125-tempat-task-berpindah) dan
[12.7](../syntax.md#127-perbedaan-dengan-go).

## 1. `Task<T>` — handle ke task

| Nama | Penggunaan | Tipe | Arti |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Menunggu selesai dan mengembalikan nilainya |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **Anda boleh `wait` berapa kali pun** (nilainya di-cache). Tidak seperti `JoinHandle::join` pada
  Rust, ia tidak mengonsumsi handle, sehingga dapat ditunggu dari beberapa tempat.
- **Task berjalan meskipun Anda tidak pernah `wait`.** Membuang handle tidak menghentikannya.
- Ia adalah nilai biasa, sehingga dapat masuk ke `Vector<Task<()>>`.
- **Ketika task utama berakhir, proses berakhir** (seperti di Go). Task lain yang sedang berjalan
  dihentikan paksa, dan pembersihan `unwind-protect` tidak dijalankan, karena ini adalah keluarnya
  proses, bukan pelepasan stack.

## 2. `Chan<T>` — kanal

| Nama | Penggunaan | Tipe | Arti |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | Kanal berkapasitas `n`. `0` adalah rendezvous (tanpa buffer) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Menunggu sampai ada ruang, lalu menyerahkan nilai |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Menunggu sampai nilai tiba. `none` setelah ditutup dan kosong |
| `close` | `(close ch)` | `(Chan<T>)→()` | Menutupnya |
| `len` | `(len ch)` | `(Chan<T>)→int` | Berapa banyak nilai di buffer saat ini |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | Kapasitas |

**Argumen tipe diberikan dengan `the`** (ditulis sama seperti `(the Vector<i32> (Vector::new))`).
**Kapasitas harus selalu ditulis**: dua kasus yang di Go ditulis `make(chan int)` dan
`make(chan int, 16)` di sini ditulis `(Chan::new 0)` dan `(Chan::new 16)`.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go's for v := range ch
```

- **`Chan<T>` adalah iteratornya sendiri** (ia mengimplementasikan `Iter`). `recv` mengembalikan
  `Option<T>`, tipe yang sama seperti `Iter::next`, sehingga `doiter` dan `map`/`filter`/`foldl`
  semuanya bekerja padanya apa adanya.
- **`send` pada kanal yang sudah ditutup melakukan panic**, dan **`close` kedua kalinya juga
  melakukan panic** (keduanya seperti di Go). Ini adalah bug dalam program, bukan kegagalan yang
  dapat dipulihkan, sehingga bukan `Result`.
- **Menutup kanal yang sedang ditunggu sebuah task untuk `send` membuat task itu melakukan panic**
  (aturan Go).
- `recv` pada kanal yang sudah ditutup mengembalikan apa yang tersisa di buffer, dan setelah kosong,
  terus mengembalikan `none`.
- `close` diselesaikan berdasarkan tipe penerima, sehingga ia berbeda dari `close` pada trait
  `Stream`. `Chan<T>` tidak mengimplementasikan `Stream`.
- **Kapasitas negatif melakukan panic** (tidak dibulatkan diam-diam ke 0).

## 3. `yield` / `sleep` — mengalah

| Nama | Penggunaan | Tipe | Arti |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Melepaskan sisa gilirannya (`runtime.Gosched` pada Go) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | Menghentikan **hanya task itu**. Yang lain terus berjalan |

`sleep` menghentikan sebuah task, bukan sebuah thread. Hanya ketika tidak ada task sama sekali yang
dapat berjalan ia masuk ke `sleep` OS sampai tenggat terdekat. `(sleep 0.0)` adalah "yield selama 0
detik" pada CL.

Seperti di CL, `sleep` menerima **detik**. Bilangan bulat tidak dikonversi otomatis menjadi bilangan
floating-point, sehingga `(sleep 1)` pada CL di sini ditulis `(sleep 1.0)`. Nilai negatif atau NaN
melakukan panic.

## 4. `WaitGroup` — menunggu N penyelesaian

| Nama | Penggunaan | Tipe | Arti |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Grup tanpa yang tertunda |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Menambah penghitung. Lakukan sebelum pekerjaan dimulai |
| `done` | `(done wg)` | `(WaitGroup)→()` | Satu telah selesai. Pada 0, setiap penunggu dilepaskan |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | Menunggu sampai mencapai 0. Dari sejumlah task mana pun |

`(wait wg)` dan `(wait task)` diselesaikan berdasarkan tipe penerima, sehingga keduanya hidup
berdampingan dengan satu nama. Jika penghitung turun di bawah 0, ia melakukan panic (`done`
dipanggil terlalu sering, atau `add` negatif). Seperti di Go, grup yang kembali ke 0 dapat dipakai
lagi mulai dari `add`. Tidak ada pembaruan yang hilang bahkan ketika task berjalan di thread OS yang
terpisah.

**Dengan adanya `Task<T>`, kebutuhannya lebih sedikit daripada di Go**: `(doiter (t tasks) (wait t))`
sering sudah cukup. Ia adalah alat untuk pekerjaan yang bertambah secara dinamis, atau ketika Anda
tidak ingin menyimpan handle.

```lisp
;; fan-in: start one task per input and join them (this language has no nil channels)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — kanal yang mengirim setelah suatu waktu

| Nama | Penggunaan | Tipe | Arti |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | Kanal yang mengirim satu nilai setelah `sec` detik |

`time.After` pada Go. Ia dapat ditulis apa adanya pada cabang batas waktu `select`
([Referensi Sintaks 12.3](../syntax.md#123-select--menunggu-beberapa-operasi-kanal-sekaligus)).
Kapasitasnya 1, sehingga task pengirim dapat selesai meskipun tidak ada yang menerima.

## 6. `Mutex<T>` — eksklusi mutual untuk data bersama

| Nama | Penggunaan | Tipe | Arti |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | Mutex yang tidak terkunci dan menyimpan `v` |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Mengambil kunci (menunggu) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Melepasnya. Panic jika tidak terkunci |
| `with-lock` | `(with-lock (x m) body...)` | Makro | Mengunci, mengikat isinya ke `x`, menjalankan `body`, dan **selalu** melepas |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` bukan salinan nilai melainkan sebuah "place"** (`symbol-macrolet`). `(setf x 42)` mengubah
  isi mutex.
- `with-lock` melepas dengan `unwind-protect`, sehingga kunci dilepas bagaimanapun badannya
  ditinggalkan: selesai normal, `throw`, `panic`, atau `break`/`return`/`return-from`.
- **Masuk kembali menyebabkan deadlock** (tidak melakukan panic). Scheduler melaporkan bahwa "tidak
  ada yang dapat membuat kemajuan" untuk task yang tersangkut pada kuncinya sendiri.
- **`m::v` menyentuh isi dari luar kunci**, yang tidak terdefinisi dalam arti task lain mungkin
  sedang di tengah mengubahnya. Ini posisi yang sama dengan `sync.Mutex` pada Go: pada bahasa tanpa
  pemeriksaan kepemilikan atau peminjaman, jaminan statis seperti `MutexGuard` tidak dapat dibangun.

## 7. `Thread<T>` — thread OS khusus

Handle yang dikembalikan oleh `(thread (f args...))`
([Referensi Sintaks 12.2](../syntax.md#122-thread--memulai-task-pada-thread-os-khusus)). Padanan
`Task<T>`.

| Nama | Penggunaan | Tipe | Arti |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Menunggu selesai dan mengembalikan nilainya (**task** pemanggil berhenti. Dapat dipanggil berapa kali pun; nilainya di-cache) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | Versi fungsi dari `(thread (f))` (`std::thread::spawn` pada Rust) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | Nomor thread OS yang sedang berjalan. Unik dalam proses, tanpa makna selain "apakah ini thread yang sama" |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | Jumlah thread yang dapat dijalankan mesin sekaligus (bawaan `TYPELISP_THREADS`). Panic jika OS tidak menjawab |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; a blocking C function
(let ((th (thread (sleepy 500000))))
  ...                                            ; other tasks keep going meanwhile
  (join th))                                     ; => 500000
```

- Memanggil fungsi C yang memblokir (`defffi`) hanya menghentikan thread itu.
- `task` di dalam `thread` berjalan sebagai task biasa di thread lain.
- Ia dapat dipakai di `typl` juga. Saat menginterpretasi, `(thread (f ...))` dan `Thread::spawn`
  mengompilasi fungsi yang akan dijalankan saat itu juga lalu menjalankannya di thread khusus.
  `lambda` yang merujuk variabel lokal di luarnya tidak dapat dikompilasi sendiri dan melakukan panic
  ([Referensi Sintaks 12.2](../syntax.md#122-thread--memulai-task-pada-thread-os-khusus)). `lambda`
  yang dibuat di dalam fungsi terkompilasi dapat diserahkan.

## 8. Yang tidak ada

- **`Atomic`**. `Mutex` sudah cukup.
- **Variabel lokal task** (Go juga tidak memilikinya).
- **Kanal nil**. Alasan dan alternatifnya ada di
  [Referensi Sintaks 12.7](../syntax.md#127-perbedaan-dengan-go).
