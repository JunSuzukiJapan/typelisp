<!-- translated-from: docs/ja/tutorial/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Konkurensi

Di typelisp Anda menjalankan pekerjaan secara konkuren dengan memulai **task** (thread ringan), dan
task saling mengoper nilai melalui **kanal**. Modelnya mirip dengan goroutine dan channel pada Go.
Bab ini membahas, secara berurutan, memulai task dan mengambil hasilnya, kanal, `select`,
melindungi data bersama, dan thread OS khusus. Bab ini mengasumsikan Anda sudah membaca
[Dasar-dasar Tipe](types.md).

## 1. Memulai task dan menunggu hasilnya

`(task (fungsi argumen...))` memulai pemanggilan fungsi sebagai task baru. Pihak yang memulai tidak
menunggu dan terus berjalan. Nilainya adalah handle bertipe `Task<T>`; `(wait handle)` menunggu task
selesai dan mengembalikan hasilnya.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; wait 0.1 seconds (only this task stops)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` hanya menerima bentuk pemanggilan fungsi. Argumen dievaluasi di tempat `task` ditulis;
  hanya pemanggilannya sendiri yang berjalan di task baru.
- Untuk menjalankan beberapa ekspresi, buat `lambda` dan panggil di tempat:
  `(task ((lambda () () (println "start") (work))))`
- Anda boleh memanggil `wait` sebanyak yang Anda mau. Hasilnya diingat.
- Task berjalan meskipun Anda tidak pernah `wait` padanya.
- **Ketika pekerjaan utama selesai, program berakhir.** Task yang masih berjalan dihentikan paksa.

## 2. Mengoper nilai melalui kanal

Kanal `Chan<T>` adalah jalur tempat task mengoper nilai bertipe `T`. Argumen `Chan::new` adalah
kapasitas (berapa banyak nilai yang dapat ditampung). Pada kanal berkapasitas 0, pengirim dan
penerima sama-sama menunggu sampai pihak lainnya ada.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n)
    (send ch (* i 10)))        ; send
  (close ch))                  ; no more sends

(let ((ch (the Chan<int> (Chan::new 0))))
  (task (produce ch 4))
  (doiter (v ch)               ; receive until it is closed
    (println "got ~a" v)))
```

```
got 0
got 10
got 20
got 30
```

- `(send ch v)` mengirim. Jika kanal penuh, ia menunggu sampai ada ruang.
- `(recv ch)` menerima. Ia menunggu sampai ada nilai tiba. Hasilnya adalah `Option<T>`; setelah kanal
  ditutup dan kosong, ia mengembalikan `none`.
- Mengiterasi kanal dengan `doiter` terus menerima nilai sampai kanal ditutup. Kanal juga dapat
  langsung diserahkan ke `map` atau `filter`.
- `send` pada kanal yang sudah ditutup melakukan panic.

### Membagi pekerjaan ke beberapa task

Pola yang umum adalah menyiapkan satu kanal yang membawa pekerjaan dan membiarkan beberapa worker
(task yang memproses pekerjaan) mengambil job darinya. Worker mana pun yang bebas mengambil job
berikutnya, sehingga bahkan ketika job lambat dan cepat bercampur, pekerjaan tersebar secara
alami.

```lisp
(defstruct job
  (id int)
  (seconds f64))                  ; how long this job takes

(defun worker ((name string) (jobs Chan<job>)) int
  (let ((count 0))
    (doiter (j jobs)              ; take one job at a time until jobs is closed
      (println "~a: start  job ~a (~as)" name j::id j::seconds)
      (sleep j::seconds)          ; working; meanwhile other workers take the next jobs
      (println "~a: finish job ~a" name j::id)
      (setf count (+ count 1)))
    count))                       ; how many jobs this worker did

(let* ((jobs (the Chan<job> (Chan::new 0)))
       (a (task (worker "A" jobs)))
       (b (task (worker "B" jobs)))
       (c (task (worker "C" jobs))))
  (send jobs (job::new 1 0.3))
  (send jobs (job::new 2 0.1))
  (send jobs (job::new 3 0.1))
  (send jobs (job::new 4 0.2))
  (send jobs (job::new 5 0.1))
  (send jobs (job::new 6 0.1))
  (close jobs)                    ; that is all the work
  (println "A: ~a jobs, B: ~a jobs, C: ~a jobs" (wait a) (wait b) (wait c)))
```

```
A: start  job 1 (0.3s)
B: start  job 2 (0.1s)
C: start  job 3 (0.1s)
B: finish job 2
B: start  job 4 (0.2s)
C: finish job 3
C: start  job 5 (0.1s)
C: finish job 5
C: start  job 6 (0.1s)
A: finish job 1
B: finish job 4
C: finish job 6
A: 1 jobs, B: 2 jobs, C: 3 jobs
```

- A, B, dan C masing-masing mengambil satu dari tiga job pertama.
- Setelah 0,1 detik B dan C bebas dan mengambil job yang tersisa. Selama A sibuk dengan job 1 yang
  lambat, ia tidak mengambil pekerjaan baru.
- Pada akhirnya A menangani satu job, B dua, dan C tiga. Tidak ada dalam program yang menyatakan
  worker mana mengambil job mana.
- Menutup `jobs` mengakhiri `doiter` tiap worker, task selesai, dan tiap `wait` mengembalikan
  jumlahnya.

`jobs` adalah kanal berkapasitas 0, sehingga `send` menunggu sampai ada worker yang mengambil job.
Dengan kapasitas lebih besar, task utama dapat mengantrekan pekerjaan tanpa menunggu worker.

## 3. `select`: menunggu beberapa kanal sekaligus

`select` melakukan operasi kanal mana pun yang lebih dulu menjadi mungkin di antara beberapa
operasi. `(after seconds)` adalah kanal yang mengirimkan satu nilai setelah waktu yang diberikan
berlalu. Dikombinasikan dengan `select`, ia memberi Anda batas waktu.

```lisp
(defun late-send ((ch Chan<string>) (sec f64)) ()
  (sleep sec)
  (send ch "done"))

(let ((ch (the Chan<string> (Chan::new 1))))
  (task (late-send ch 1.0))
  (select
    ((v (recv ch)) (println "~a" (unwrap v)))
    ((z (recv (after 0.2))) (println "timeout"))))
;; timeout
```

- `((v (recv ch)) body...)` adalah cabang penerimaan. `v` mendapat `Option<T>`.
- `((send ch x) body...)` adalah cabang pengiriman.
- Ketika beberapa cabang dapat berjalan pada saat yang sama, salah satunya dipilih secara acak.
- Dengan `(else body...)` di akhir, `else` dijalankan ketika tidak ada cabang yang dapat langsung
  berjalan, dan `select` tidak menunggu.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Melindungi data bersama

Ketika beberapa task mengubah nilai yang sama, lindungi dengan `Mutex<T>`. `with-lock` mengambil
kunci, mengikat isinya ke sebuah variabel, menjalankan badan, dan selalu melepas kunci bagaimanapun
badannya ditinggalkan. Menugaskan variabel itu dengan `setf` di dalam badan mengubah isi `Mutex`.

`WaitGroup` adalah alat untuk menunggu sampai sejumlah task tertentu selesai. Naikkan hitungan dengan
`add`, minta tiap task memanggil `done` saat selesai, dan `wait` sampai hitungan mencapai 0.

```lisp
(defun add-many ((counter Mutex<int>) (n int)) ()
  (dotimes (i n)
    (with-lock (c counter)
      (setf c (+ c 1)))))

(let ((counter (the Mutex<int> (Mutex::make 0)))
      (wg (the WaitGroup (WaitGroup::make))))
  (dotimes (i 4)
    (add wg 1)
    (task ((lambda () ()
             (add-many counter 1000)
             (done wg)))))
  (wait wg)
  (println "count = ~a" (with-lock (c counter) c)))    ; count = 4000
```

Jika beberapa task mengubah nilai yang sama pada saat yang sama tanpa melalui `Mutex` atau kanal,
hasilnya tidak dijamin. Oper data antartask melalui kanal bila memungkinkan, dan bagikan data hanya
bila perlu.

## 5. Tempat task berpindah

Task berpindah secara kooperatif. Sebuah task mengalah kepada task lain hanya pada titik-titik
berikut:

- `(yield)`, `(sleep seconds)`, `(wait handle)`
- Operasi kanal yang harus menunggu (`send`, `recv`, `select`)
- Operasi socket yang harus menunggu (menyambung, membaca, menulis, dan sebagainya)

Argumen `sleep` adalah bilangan `f64` dalam detik. Tulis `(sleep 1.0)`, bukan `(sleep 1)`.

Task berjalan pada saat yang sama di beberapa thread OS. Namun, ketika `typl` menjalankan program
secara langsung, hanya task yang menjalankan fungsi [terkompilasi](../guide/compile.md) yang keluar
ke thread lain. Task lainnya berjalan di satu thread, berpindah pada titik-titik di atas.

## 6. `thread`: berjalan di thread OS khusus

Pekerjaan yang tidak boleh menahan task lain, seperti memanggil fungsi C yang lambat
([FFI C](../guide/ffi.md)), dimulai dengan `thread`. Penulisannya sama seperti `task`, dan ia
mendapat thread OS tersendiri. Tunggu sampai selesai dengan `join`.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- Handle dari `thread` bertipe `Thread<T>`. Seperti `wait`, `join` dapat dipanggil berapa kali pun.
- `thread` hanya dapat menjalankan fungsi yang dapat dikompilasi. Saat berjalan di `typl`, fungsi
  yang dipanggilnya dikompilasi saat itu juga sebelum dijalankan.

## 7. Task dan fitur lain

- `panic` di dalam task menghentikan seluruh program.
- `throw` tidak menjangkau keluar dari task. `throw` yang akan meninggalkan badan task menjadi
  `panic`.
- Keluaran satu `println` tidak pernah bercampur di tengah baris dengan keluaran task lain.

## 8. Yang dibaca selanjutnya

- [Task dan Kanal](../reference/functions/concurrency.md): daftar fungsi
- [Referensi Sintaks bab 12](../reference/syntax.md#12-konkurensi-task): tempat task berpindah secara
  rinci, dan perbedaan dengan Go
- [I/O Berkas, Stream, dan Jaringan](../guide/io.md): menulis server dengan task
