<!-- translated-from: docs/ja/reference/functions/concurrency.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# Task'ler ve Kanallar

Task'lerin (hafif thread'lerin) sözcük dağarcığı. Onları başlatan `task` ve `thread` ile birkaç şeyi
bekleyen `select` özel formlardır ve [Sözdizimi Başvurusu](../syntax.md#12-eşzamanlılık-taskler)
belgesindedir. Bu bölüm geri kalanı ele alır: türleri, metotları ve fonksiyonları.

Task'ler **işbirlikçidir**: bir task yalnızca sizin yazdığınız noktalarda geçiş yapar. Task'ler
`TYPELISP_THREADS` sayıda OS thread'inde aynı anda çalışır (`typl` içinde yalnızca derlenmiş task'ler
diğer thread'lere çıkar). Nerede geçiş yaptıkları ve bunun Go'dan farkı
[Sözdizimi Başvurusu 12.5](../syntax.md#125-tasklerin-geçiş-yaptığı-yerler) ve
[12.7](../syntax.md#127-godan-farklar) bölümlerindedir.

## 1. `Task<T>` — task tanıtıcıları

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `wait` | `(wait t)` | `(Task<T>)→T` | Bitmesini bekler ve değerini döndürür |

```lisp
(let ((t1 (task (work 7))))
  (wait t1))
```

- **İstediğiniz kadar `wait` edebilirsiniz** (değer önbelleğe alınır). Rust'ın `JoinHandle::join`'inden
  farklı olarak tanıtıcıyı tüketmez; bu yüzden birkaç yerden beklenebilir.
- **Bir task, hiç `wait` etmeseniz bile çalışır.** Tanıtıcıyı bırakmak onu durdurmaz.
- Sıradan bir değerdir; bu yüzden bir `Vector<Task<()>>` içine girebilir.
- **Ana task bittiğinde süreç biter** (Go'daki gibi). Çalışan diğer task'ler kesilir ve
  `unwind-protect` temizliği çalışmaz; çünkü bu bir yığın çözme değil, süreçten çıkıştır.

## 2. `Chan<T>` — kanallar

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `Chan::new` | `(the Chan<int> (Chan::new 0))` | `(int)→Chan<T>` | `n` kapasiteli bir kanal. `0` bir buluşmadır (tampon yok) |
| `send` | `(send ch v)` | `(Chan<T>,T)→()` | Yer olana kadar bekler, sonra değeri teslim eder |
| `recv` | `(recv ch)` | `(Chan<T>)→Option<T>` | Bir değer gelene kadar bekler. Kapatılıp boşaldığında `none` |
| `close` | `(close ch)` | `(Chan<T>)→()` | Kapatır |
| `len` | `(len ch)` | `(Chan<T>)→int` | Şu anda tamponda kaç değer olduğu |
| `cap` | `(cap ch)` | `(Chan<T>)→int` | Kapasite |

**Tür bağımsız değişkeni `the` ile verilir** (`(the Vector<i32> (Vector::new))` ile aynı şekilde yazılır).
**Kapasite her zaman yazılmalıdır**: Go'nun `make(chan int)` ve `make(chan int, 16)` olarak yazdığı iki
durum, burada `(Chan::new 0)` ve `(Chan::new 16)` olarak yazılır.

```lisp
(defun produce ((ch Chan<int>) (n int)) ()
  (dotimes (i n) (send ch (* i 2)))
  (close ch))

(let ((ch (the Chan<int> (Chan::new 2))))
  (task (produce ch 5))
  (doiter (v ch) (println "~a" v)))       ; Go's for v := range ch
```

- **Bir `Chan<T>` kendi yineleyicisidir** (`Iter`'i gerçekleştirir). `recv`, `Iter::next` ile aynı tür
  olan bir `Option<T>` döndürür; bu yüzden `doiter` ve `map`/`filter`/`foldl` hepsi onun üzerinde olduğu
  gibi çalışır.
- **Kapalı bir kanala `send` panic olur** ve **ikinci bir `close` da panic olur** (ikisi de Go'daki
  gibi). Bunlar programdaki hatalardır, kurtarılabilir başarısızlıklar değildir; bu yüzden `Result`
  değildirler.
- **Bir task'in `send` için beklediği kanalı kapatmak, o task'in panic olmasına yol açar** (Go'nun kuralı).
- Kapalı bir kanalda `recv`, tamponda kalanı döndürür ve boşaldığında `none` döndürmeye devam eder.
- `close`, alıcının türüne göre çözümlenir; bu yüzden `Stream` trait'inin `close`'undan farklı bir
  şeydir. `Chan<T>`, `Stream`'i gerçekleştirmez.
- **Negatif bir kapasite panic olur** (sessizce 0'a yuvarlanmaz).

## 3. `yield` / `sleep` — yol verme

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `yield` | `(yield)` | `()→()` | Sırasının kalanından vazgeçer (Go'nun `runtime.Gosched`'i) |
| `sleep` | `(sleep 0.5)` | `(f64)→()` | **Yalnızca o task'i** durdurur. Diğerleri çalışmaya devam eder |

`sleep` bir thread'i değil, bir task'i durdurur. Yalnızca hiçbir task çalışamadığında en yakın son
tarihe kadar bir OS `sleep`'ine girer. `(sleep 0.0)`, CL'nin "0 saniyeliğine yol ver"idir.

CL'deki gibi `sleep` **saniye** alır. Tamsayılar otomatik olarak kayan noktalı sayıya dönüştürülmez;
bu yüzden CL'nin `(sleep 1)`'i burada `(sleep 1.0)` olarak yazılır. Negatif bir değer ya da NaN
panic olur.

## 4. `WaitGroup` — N işin bitmesini bekleme

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `WaitGroup::make` | `(the WaitGroup (WaitGroup::make))` | `()→WaitGroup` | Bekleyen hiçbir şeyi olmayan bir grup |
| `add` | `(add wg 1)` | `(WaitGroup,int)→()` | Sayaca ekler. İş başlamadan önce yapın |
| `done` | `(done wg)` | `(WaitGroup)→()` | Biri bitti. 0'da tüm bekleyenler serbest bırakılır |
| `wait` | `(wait wg)` | `(WaitGroup)→()` | 0'a ulaşana kadar bekler. İstenen sayıda task'ten |

`(wait wg)` ve `(wait task)`, alıcının türüne göre çözümlenir; bu yüzden tek bir ad altında bir arada
bulunurlar. Sayaç 0'ın altına inerse panic olur (`done` çok fazla çağrıldı ya da negatif bir `add`).
Go'daki gibi, 0'a dönmüş bir grup `add` ile başlayarak yeniden kullanılabilir. Task'ler ayrı OS
thread'lerinde çalışsa bile hiçbir güncelleme kaybolmaz.

**`Task<T>` mevcut olduğundan Go'ya göre daha az gerekir**: çoğu zaman `(doiter (t tasks) (wait t))`
yeterlidir. Dinamik olarak büyüyen işler için ya da tanıtıcıları tutmak istemediğinizde bir araçtır.

```lisp
;; fan-in: start one task per input and join them (this language has no nil channels)
(defvar (left i32) 0)
(defun drain ((in Chan<i32>) (out Chan<i32>)) ()
  (doiter (v in) (send out v))
  (setf left (- left 1))
  (if (eq left 0) (close out) ()))
```

## 5. `after` — bir süre sonra ileten kanal

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `after` | `(recv (after 0.5))` | `(f64)→Chan<()>` | `sec` saniye sonra tek bir değer ileten bir kanal |

Go'nun `time.After`'ı. `select`'in zaman aşımı kolunda olduğu gibi yazılabilir
([Sözdizimi Başvurusu 12.3](../syntax.md#123-select--birkaç-kanal-işlemini-aynı-anda-bekleme)).
Kapasitesi 1'dir; bu yüzden kimse almasa bile gönderen task bitebilir.

## 6. `Mutex<T>` — paylaşılan veri için karşılıklı dışlama

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `Mutex::make` | `(the Mutex<i32> (Mutex::make 0))` | `(T)→Mutex<T>` | `v`'yi tutan kilitsiz bir mutex |
| `lock` | `(lock m)` | `(Mutex<T>)→()` | Kilidi alır (bekler) |
| `unlock` | `(unlock m)` | `(Mutex<T>)→()` | Bırakır. Kilitli değilse panic olur |
| `with-lock` | `(with-lock (x m) body...)` | Makro | Kilitler, içeriği `x`'e bağlar, `body`'yi çalıştırır ve **her zaman** bırakır |

```lisp
(let ((counter (the Mutex<i32> (Mutex::make 0))))
  (with-lock (n counter) (setf n (+ n 1)))
  (with-lock (n counter) n))                     ; => 1
```

- **`x` değerin bir kopyası değil, bir "yer"dir** (`symbol-macrolet`). `(setf x 42)` mutex'in
  içeriğini değiştirir.
- `with-lock`, `unwind-protect` ile bırakır; bu yüzden gövdeden nasıl çıkılırsa çıkılsın kilit bırakılır:
  normal tamamlanma, `throw`, `panic` ya da `break`/`return`/`return-from`.
- **Yeniden girmek kilitlenir** (panic olmaz). Zamanlayıcı, kendi kilidine takılmış bir task için
  "hiçbir şey ilerleyemiyor" diye bildirir.
- **`m::v`, içeriğe kilidin dışından dokunur**; bu, başka bir task'in onu değiştirmenin ortasında
  olabilmesi anlamında tanımsızdır. Bu, Go'nun `sync.Mutex`'iyle aynı konumdur: sahiplik ya da ödünç
  alma denetimi olmayan bir dilde `MutexGuard` gibi statik bir güvence kurulamaz.

## 7. `Thread<T>` — ayrılmış OS thread'leri

`(thread (f args...))`'in döndürdüğü tanıtıcı
([Sözdizimi Başvurusu 12.2](../syntax.md#122-thread--ayrılmış-bir-os-threadinde-task-başlatma)).
`Task<T>`'nin karşılığı.

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `join` | `(join th)` | `(Thread<T>)→T` | Bitmesini bekler ve değeri döndürür (çağıran **task** durur. İstenen sayıda çağrılabilir; değer önbelleğe alınır) |
| `Thread::spawn` | `(Thread::spawn (lambda () int 42))` | `((fn () T))→Thread<T>` | `(thread (f))`'in fonksiyon sürümü (Rust'ın `std::thread::spawn`'ı) |
| `Thread::current-id` | `(Thread::current-id)` | `()→int` | Çalışmakta olan OS thread'inin numarası. Süreç içinde benzersizdir; "aynı thread mi" ötesinde anlamı yoktur |
| `Thread::available-parallelism` | `(Thread::available-parallelism)` | `()→int` | Makinenin aynı anda çalıştırabildiği thread sayısı (`TYPELISP_THREADS`'in varsayılanı). İşletim sistemi yanıt vermezse panic olur |

```lisp
(defffi (c-usleep "usleep") (u32) i32)
(defun sleepy ((us int)) int
  (progn (unsafe (c-usleep (as u32 us))) us))    ; a blocking C function
(let ((th (thread (sleepy 500000))))
  ...                                            ; other tasks keep going meanwhile
  (join th))                                     ; => 500000
```

- Engelleyen bir C fonksiyonunu (`defffi`) çağırmak yalnızca o thread'i durdurur.
- Bir `thread` içindeki `task`, diğer thread'lerde sıradan bir task olarak çalışır.
- `typl` içinde de kullanılabilir. Yorumlanırken `(thread (f ...))` ve `Thread::spawn`, çalıştırılacak
  fonksiyonu anında derler ve sonra ayrılmış thread'de çalıştırır. Dışındaki yerel değişkenlere
  başvuran bir `lambda` tek başına derlenemez ve panic olur
  ([Sözdizimi Başvurusu 12.2](../syntax.md#122-thread--ayrılmış-bir-os-threadinde-task-başlatma)).
  Derlenmiş bir fonksiyonun içinde oluşturulan bir `lambda` geçirilebilir.

## 8. `Context` — iş birliğine dayalı iptal

Go'nun `context.Context`'i. Dışarıdan durdurabilmek istediğiniz işe verilir. Durdurma **iş birliğine
dayalıdır**: `cancel` hiçbir şeyi kesmez; bir görev ya da iş parçacığı bunu kendisi `is-cancelled`'ı
denetleyerek ya da `done`'dan alarak fark eder.

| Ad | Kullanım | Tür | Anlamı |
|---|---|---|---|
| `Context::background` | `(Context::background)` | `()→Context` | Kök olacak yeni bir bağlam |
| `Context::with-cancel` | `(Context::with-cancel parent)` | `(Context)→Context` | `parent`'ın bir çocuğunu oluşturur |
| `Context::with-timeout` | `(Context::with-timeout parent sec)` | `(Context,f64)→Context` | `sec` saniye sonra kendini iptal eden bir `parent` çocuğu oluşturur |
| `cancel` | `(cancel ctx)` | `(Context)→()` | İptal eder. İstenildiği kadar çağrılabilir |
| `done` | `(done ctx)` | `(Context)→Chan<()>` | Bağlam iptal edildiğinde kapanan bir kanal |
| `is-cancelled` | `(is-cancelled ctx)` | `(Context)→bool` | İptal edilip edilmediği |

```lisp
(defun worker ((ctx Context) (jobs Chan<int>)) ()
  (loop
    (select
      ((v (recv (done ctx))) (println "stopped") (break))
      ((j (recv jobs)) (match j
                         ((some n) (println "job ~a" n))
                         ((none) (break)))))))

(let* ((ctx (Context::with-timeout (Context::background) 1.0))
       (jobs (the Chan<int> (Chan::new 0))))
  (task (worker ctx jobs))
  (send jobs 1)
  (send jobs 2)
  (cancel ctx)                          ; job 1, job 2, ardından stopped
  (sleep 0.1))
```

- **İptal çocuklara ulaşır.** `with-cancel`/`with-timeout` ile oluşturulan bir bağlam, ebeveyniyle
  birlikte iptal edilir. Ters yönde (çocuktan ebeveyne) gitmez.
- Zaten iptal edilmiş bir bağlamdan oluşturulan çocuk baştan iptal edilmiş olur.
- `done` yalnızca kapatılır; değer gönderilmez. Alma işlemi `none` döndürür.
- Her `(Context::background)` çağrısı ayrı bir kök oluşturur. Go'nun `Background()`'ı tektir ve
  iptal edilemez; burada bir kök de iptal edilebilir ve bu yalnızca ondan oluşturulanları etkiler.
- Bir bağlam görevler arasında ve iş parçacıkları arasında aktarılabilir.

## 9. Bulunmayanlar

- **`Atomic`**. `Mutex` yeterlidir.
- **Task'e yerel değişkenler** (Go'da da yoktur).
- **nil kanallar**. Gerekçe ve alternatif
  [Sözdizimi Başvurusu 12.7](../syntax.md#127-godan-farklar) bölümündedir.
