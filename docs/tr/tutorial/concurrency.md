<!-- translated-from: docs/ja/tutorial/concurrency.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Eşzamanlılık

typelisp'te işi eşzamanlı çalıştırmak için **task'ler** (hafif thread'ler) başlatırsınız ve task'ler
birbirlerine değerleri **kanallar** üzerinden geçirir. Model, Go'nun goroutine'lerine ve kanallarına
yakındır. Bu bölüm sırasıyla bir task başlatmayı ve sonucunu almayı, kanalları, `select`'i, paylaşılan
verinin korunmasını ve ayrılmış OS thread'lerini ele alır. [Tür Temelleri](types.md) bölümünü
okuduğunuzu varsayar.

## 1. Bir task başlatma ve sonucunu bekleme

`(task (fonksiyon bağımsız-değişkenler...))`, bir fonksiyon çağrısını yeni bir task olarak başlatır.
Başlatan taraf beklemez ve devam eder. Değer, `Task<T>` türünde bir tanıtıcıdır; `(wait tanıtıcı)`
task'in bitmesini bekler ve sonucunu döndürür.

```lisp
(defun slow-square ((n int)) int
  (sleep 0.1)                  ; wait 0.1 seconds (only this task stops)
  (* n n))

(let ((a (task (slow-square 3)))
      (b (task (slow-square 4))))
  (println "~a" (+ (wait a) (wait b))))    ; 25
```

- `task` yalnızca bir fonksiyon çağrısı biçimini alır. Bağımsız değişkenler `task`'in yazıldığı yerde
  değerlendirilir; yalnızca çağrının kendisi yeni task'te çalışır.
- Birkaç ifade çalıştırmak için bir `lambda` yapıp onu olduğu yerde çağırın:
  `(task ((lambda () () (println "start") (work))))`
- `wait`'i istediğiniz kadar çağırabilirsiniz. Sonuç hatırlanır.
- Bir task, onu hiç `wait` etmeseniz bile çalışır.
- **Ana iş bittiğinde program sona erer.** Hâlâ çalışan task'ler kesilir.

## 2. Kanallar üzerinden değer geçirme

Bir `Chan<T>` kanalı, task'lerin `T` türündeki değerleri geçirdiği bir yoldur. `Chan::new`'in
bağımsız değişkeni kapasitedir (kaç değer tutabileceği). Kapasitesi 0 olan bir kanalda gönderen ve
alan, karşı taraf orada olana kadar birlikte bekler.

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

- `(send ch v)` gönderir. Kanal doluysa yer açılana kadar bekler.
- `(recv ch)` alır. Bir değer gelene kadar bekler. Sonuç bir `Option<T>`'dir; kanal kapatılıp
  boşaldığında `none` döndürür.
- Bir kanalı `doiter` ile yinelemek, kapanana kadar değer almaya devam eder. Doğrudan `map` ya da
  `filter`'a da geçirilebilir.
- Kapalı bir kanala `send` panic olur.

### İşi birkaç task arasında bölme

Yaygın bir desen, işi taşıyan tek bir kanal kurmak ve birkaç işçinin (işi işleyen task'ler) ondan iş
almasını sağlamaktır. Hangi işçi boştaysa sıradaki işi alır; bu yüzden yavaş ve hızlı işler karışık
olsa bile iş doğal olarak yayılır.

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

- A, B ve C ilk üç işten birer tanesini alır.
- 0,1 saniye sonra B ve C boşalır ve kalan işleri alır. A yavaş 1 numaralı işle meşgulken yeni iş
  almaz.
- Sonunda A bir iş, B iki iş ve C üç iş işlemiştir. Programda hangi işçinin hangi işi alacağını
  söyleyen hiçbir şey yoktur.
- `jobs`'u kapatmak her işçinin `doiter`'ını bitirir, task'ler sona erer ve her `wait` kendi sayısını
  döndürür.

`jobs` kapasitesi 0 olan bir kanaldır; bu yüzden `send`, bir işçi işi alana kadar bekler. Daha büyük
bir kapasiteyle ana task, işçileri beklemeden işi kuyruğa alabilirdi.

## 3. `select`: birkaç kanalı aynı anda bekleme

`select`, birkaç kanal işleminden hangisi önce mümkün olursa onu gerçekleştirir. `(after saniye)`,
verilen süre geçtikten sonra tek bir değer ileten bir kanaldır. `select` ile birleştirildiğinde size
bir zaman aşımı verir.

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

- `((v (recv ch)) gövde...)` bir alma koludur. `v`, bir `Option<T>` alır.
- `((send ch x) gövde...)` bir gönderme koludur.
- Birkaç kol aynı anda ilerleyebiliyorsa bunlardan biri rastgele seçilir.
- Sonda `(else gövde...)` varsa, hiçbir kol hemen ilerleyemediğinde `else` çalışır ve `select`
  beklemez.

```lisp
(let ((ch (the Chan<int> (Chan::new 0))))
  (select
    ((v (recv ch)) (println "~a" v))
    (else (println "nothing ready"))))
;; nothing ready
```

## 4. Paylaşılan veriyi koruma

Birkaç task aynı değeri değiştirdiğinde onu `Mutex<T>` ile koruyun. `with-lock` kilidi alır,
içeriği bir değişkene bağlar, gövdeyi çalıştırır ve gövdeden nasıl çıkılırsa çıkılsın kilidi her
zaman bırakır. Gövde içinde değişkene `setf` ile atamak `Mutex`'in içeriğini değiştirir.

`WaitGroup`, belirli sayıda task bitene kadar beklemek için bir araçtır. Sayıyı `add` ile artırın,
her task bittiğinde `done` çağırsın ve sayı 0'a ulaşana kadar `wait` edin.

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

Birkaç task bir `Mutex` ya da kanal üzerinden geçmeden aynı değeri aynı anda değiştirirse sonuç
garanti edilmez. Mümkün olduğunda veriyi task'ler arasında kanallar üzerinden geçirin ve veriyi
yalnızca gerektiğinde paylaşın.

## 5. Task'lerin geçiş yaptığı yerler

Task'ler işbirliğiyle geçiş yapar. Bir task yalnızca şu noktalarda diğer task'lere yol verir:

- `(yield)`, `(sleep saniye)`, `(wait tanıtıcı)`
- Beklemesi gereken bir kanal işlemi (`send`, `recv`, `select`)
- Beklemesi gereken bir soket işlemi (bağlanma, okuma, yazma vb.)

`sleep`'in bağımsız değişkeni saniye cinsinden bir `f64` sayıdır. `(sleep 1)` değil, `(sleep 1.0)`
yazın.

Task'ler birkaç OS thread'inde aynı anda çalışır. Ancak `typl` bir programı doğrudan çalıştırdığında,
yalnızca [derlenmiş](../guide/compile.md) fonksiyonları çalıştıran task'ler diğer thread'lere çıkar.
Diğer task'ler tek bir thread'de çalışır ve yukarıdaki noktalarda geçiş yapar.

## 6. `thread`: ayrılmış bir OS thread'inde çalıştırma

Yavaş bir C fonksiyonunu çağırmak gibi ([C FFI](../guide/ffi.md)) diğer task'leri tutmaması gereken
iş `thread` ile başlatılır. `task` ile aynı şekilde yazılır ve kendi OS thread'ini alır. Bitmesini
`join` ile bekleyin.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(let ((th (thread (fib 25))))
  (println "fib(25) = ~a" (join th)))     ; fib(25) = 75025
```

- `thread`'ten gelen tanıtıcının türü `Thread<T>`'dir. `wait` gibi `join` da istenildiği kadar çağrılabilir.
- `thread` yalnızca derlenebilen fonksiyonları çalıştırabilir. `typl` içinde çalışırken, çağırdığı
  fonksiyon çalışmadan önce anında derlenir.

## 7. Task'ler ve diğer özellikler

- Bir task içindeki `panic` tüm programı durdurur.
- `throw` bir task'in dışına ulaşmaz. Task gövdesinden çıkacak bir `throw`, `panic` olur.
- Bir `println`'in çıktısı, diğer task'lerin çıktısıyla asla bir satırın ortasında karışmaz.

## 8. Sırada ne okunmalı

- [Task'ler ve Kanallar](../reference/functions/concurrency.md): fonksiyonların listesi
- [Sözdizimi Başvurusu 12. bölüm](../reference/syntax.md#12-eşzamanlılık-taskler): task'lerin
  geçiş yaptığı yerler ayrıntılı olarak ve Go'dan farklar
- [Dosya G/Ç, Akışlar ve Ağ](../guide/io.md): task'lerle sunucu yazma
