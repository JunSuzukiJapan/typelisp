<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Hata Yönetimi

typelisp'te hata yönetimi başarısızlıkları iki türe ayırır.

| Başarısızlık türü | Örnekler | Nasıl ifade edilir |
|---|---|---|
| Olabilecek başarısızlıklar (kurtarılabilir) | Bir dosya eksik, girdi bir sayı değil | Bir `Result<T,E>` döndürmek |
| Programdaki hatalar (kurtarılamaz) | Aralık dışı bir indeks, `none`'a `unwrap`, sıfıra bölme | `panic` ile durmak |

Bunların üstüne, birçok fonksiyon çağrısından bir kerede çıkan `catch` / `throw` ve gövdesinden
nasıl çıkılırsa çıkılsın temizliği çalıştıran `unwind-protect` vardır. Bu bölüm,
[Tür Temelleri](types.md) içindeki `Result` kısmını okuduğunuzu varsayar.

## 1. Bir `Result` döndürün ve `match` ile alın

İşte bir string'den port numarası okuyan bir fonksiyon. İki şekilde başarısız olabilir: girdi bir sayı
değildir ya da aralığın dışındadır.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Çağıran, başarıyı ve başarısızlığı `match` ile ayırır.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- `Result` döndüren bir fonksiyonun değeri, `match` `err` durumunu ele almadıkça kullanılamaz.
  Başarısızlığı ele almayı unutmak bir tür hatasıdır.
- `parse-int`'ten gelen hata, `ParseIntError` türünde bir değerdir. `(message e)` mesaj string'ini verir.

## 2. Bir başarısızlığı çağırana iletme

Rust'ın `?`'si gibi bir kısayol yoktur. `Result` döndüren birkaç fonksiyonu sırayla çağırırken,
"başarısızlığı olduğu gibi döndür" kısmını `match` ile yazın.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Bir işlemin başarısız olamayacağını bildiğinizde ya da başarısızlıkta durmanın sakıncasız olduğu
küçük bir betikte, `unwrap` içeriği çıkarır. Değer bir `err` ise panic olur. Bir varsayılan değer
yeterliyse `unwrap-or` kullanın.

## 3. Kendi hata türünüzü yapma

Hataları string yerine bir tür olarak ifade etmek, çağıranın hatanın çeşidine göre dallanmasını
sağlar. Bir hata türü, `Error` trait'ini gerçekleştiren sıradan bir `defenum` ya da `defstruct`'tır.

```lisp
(defenum config-error
  (missing string)          ; a setting is missing
  (invalid string int))     ; a value is wrong

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message`, hatanın bir açıklamasını döndürür.
- `source`, bu hataya yol açan başka bir hatayı döndürür. Neden yoksa `none`'dır.

## 4. Farklı türden hataları birleştirme

Bir fonksiyon hem `parse-int`'i (`ParseIntError`) hem `check-workers`'ı (`config-error`) çağırıyorsa
iki hata türü vardır ve ikisi birden tek bir `Result<T,E>`'nin `E`'si olamaz. Bu durumda `E`'yi
`:dyn Error` yapın (`Error`'u gerçekleştiren herhangi bir türden hata). Her hatayı
`as-dyn-error` ile dönüştürün.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

`"4"`, `"-1"` ve `"abc"` verildiğinde sonuçlar şunlardır:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

`:dyn` için [Trait'ler](traits.md) bölümünün 5. kısmına bakın.

## 5. `panic`: programdaki hatalar

Program asla olmaması gereken bir duruma ulaştığında onu `panic` ile durdurun.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- `panic`'in türü `!`'dir (dönmez); bu yüzden herhangi bir türün beklendiği yere yazılabilir. Yukarıdaki
  `if`'in iki dalının uymasının nedeni budur.
- Şu işlemler de panic olur: `none` ya da `err`'e `unwrap`, aralık dışı indeksle `get` ve sıfıra tamsayı
  bölme.
- `panic` programı durdurur. Bir task'in içinde bile olsa tüm program durur.
- REPL'de bir `panic` REPL'i sonlandırmaz; bir sonraki girdiyi bekler.
- "Henüz yazılmadı" için `(todo)`, "bu noktaya asla ulaşılmamalı" için `(unreachable)` yazabilirsiniz.
  İkisi de panic olur.

`panic`, `Result`'ın yerine geçmez. Kullanıcı girdisi ya da bir dosyanın var olup olmaması gibi olabilecek
başarısızlıklar için `Result` kullanın.

## 6. `catch` / `throw`: fonksiyonlar arasından atlayarak çıkma

`throw`, aradaki fonksiyon çağrısı sayısı ne olursa olsun, aynı etiketli çevreleyen `catch`'e doğrudan
atlar.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

`v` içinde negatif sayı yoksa `validate`, `"all fine"` döndürür; `-7` içeriyorsa kontrol
`check-all`'ın içinden `catch`'e atlar ve o da `"negative: -7"` döndürür.

- Etiketi `'bad-input` gibi düz bir sembol olarak yazın.
- **Her etiket tam olarak tek bir türden değerler taşır.** Yukarıdaki örnekte `'bad-input` bir
  `string` taşır; bu yüzden aynı etiketle bir `int` fırlatmak tür hatasıdır. `catch` gövdesinin türü de
  etiketin türüyle uyuşmalıdır.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- Ulaşılacak aynı etiketli bir `catch`'i olmayan bir `throw` hatadır.

Yalnızca bir fonksiyonun içinden erken dönmek istiyorsanız `catch` / `throw` yerine `return-from`
kullanın. `return-from` fonksiyonları aşamaz, ancak karşılığında nereye döndüğünü kaynağı okuyarak
anlayabilirsiniz.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: her zaman temizle

`(unwind-protect body cleanup)`, gövdeden nasıl çıkılırsa çıkılsın temizliği çalıştırır: normal
şekilde bittiğinde, `throw` ile çıkıldığında ve panic olduğunda.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

"Açtığınız bir dosyayı her zaman kapat" ya da "aldığınız bir kilidi her zaman bırak" gibi şeyler için
kullanın. Standart kütüphanenin `with-open-file` ve `with-lock`'u içeride `unwind-protect` kullanır.

## 8. Common Lisp'in koşul sistemi hakkında

typelisp, Common Lisp'in koşul sistemini (`handler-case`, `restart-case` vb.) benimsemez. Bir
fonksiyonun hangi başarısızlıklara yol açabileceğini türlerde göstermez; bu da statik tür denetimiyle
kötü uyuşur. Olabilecek başarısızlıklar `Result` ile türlerde yazılır ve denetim aktarımları
`catch` / `throw` ile yapılır.

## 9. Sırada ne okunmalı

- [Eşzamanlılık](concurrency.md): task'ler ve kanallar
- [Option, Result ve Hata Türleri](../reference/functions/option-result.md): fonksiyonların listesi
- [Hata Mesajları](../reference/errors.md): sık görülen hataların anlamı ve nasıl giderileceği
