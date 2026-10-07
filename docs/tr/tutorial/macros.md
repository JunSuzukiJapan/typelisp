<!-- translated-from: docs/ja/tutorial/macros.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Makrolar

Bir makro, bir program alıp bir program döndüren fonksiyondur. Makrolar, fonksiyonların ifade
edemeyeceği yeni sözdizimi oluşturmanıza olanak tanır. typelisp makroları, Common Lisp'in `defmacro`'su
ile aynı şekilde çalışır. Bu bölüm, [Başlarken](intro.md) içindeki "Listeler (S-ifadeler)" kısmını
okuduğunuzu varsayar.

## 1. Makroların fonksiyonlardan farkı

Bir fonksiyon bağımsız değişkenlerini **değerlendirildikten sonra** alır. Bir makro onları
**değerlendirmeden önce, ifade olarak** (S-ifade verisi olarak) alır, başka bir ifade oluşturur ve
onu döndürür. Döndürülen ifade makro çağrısının yerini alır ve ancak o zaman türleri denetlenip
çalıştırılır. Bu değiştirmeye **açılım** denir.

Örneğin `unless` gibi bir sözdizimi fonksiyon olarak yazılamaz. Fonksiyon olsaydı, koşul doğru olsa
bile gövde önce değerlendirilirdi.

## 2. `defmacro` ve quasiquote

Gövdesini yalnızca koşul yanlış olduğunda çalıştıran `my-unless`'i yapalım.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Makro bağımsız değişkenlerinin türü yazılmaz. Her bağımsız değişken S-ifade verisidir.
- `&rest body`, kalan bağımsız değişkenleri tek bir liste olarak birlikte alır.
- `` ` `` (quasiquote) ile başlayan bir ifade, yazıldığı gibi veri olarak oluşturulur. İçinde:
  - `,test`, `test` değişkeninin içeriğini o konuma ekler.
  - `,@body`, `body` listesinin elemanlarını o konuma açarak yerleştirir.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Açılımı `macroexpand-1` ile kontrol edebilirsiniz. Bir makro yazarken önce açılımına bakmak
ilerlemenin en hızlı yoludur.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Açılımların da türleri denetlenir

Bir makronun döndürdüğü ifadenin türleri, elle yazdığınız herhangi bir ifade gibi denetlenir.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

Hata, makronun çağrıldığı yerde bildirilir.

`if`'in else dalının atlanamayacağı ve bir `if`'in iki dalının da aynı türde olması gerektiği
kuralları açılımlara olduğu gibi uygulanır. Yukarıdaki `my-unless`, gövdenin son ifadesinin türü ne
olursa olsun `if`'in iki dalının da `()` türünde olması için `(progn ,@body ())` ile biter.

## 4. Ad çakışmaları ve `gensym`

İki değişkenin değerlerini takas eden basit bir makro şöyle görünür:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Çoğu zaman çalışır, ancak çağıranın değişkeni tesadüfen `tmp` adını taşıyorsa bozulur.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (not swapped)
```

Açılım `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`'dir ve makronun yaptığı `tmp`,
çağıranın `tmp`'sini gizler.

Bunu önlemek için bir makro içinde kullanılan değişkenlerin adlarını `gensym` ile oluşturun.
`gensym`, bir programın hiçbir yerinde yazılamayacak yepyeni bir sembol döndürür.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Common Lisp'te olduğu gibi typelisp makroları ad çakışmalarını otomatik olarak önlemez (hijyenik
değildir). Şunu unutmayın: **bir makronun oluşturduğu bağlamalar için `gensym` kullanın.**

Aynı şekilde, gövdesini belirli sayıda tekrarlayan bir makro şöyle yazılabilir:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Bağımsız değişkenlere göre farklı açılma

Bir makro gövdesi sıradan typelisp kodudur; bu yüzden bağımsız değişkenlerini `if` ya da `match` ile
inceleyip farklı bir açılım oluşturabilir. Bağımsız değişkenler S-ifade verisidir
(`Option<Sexpr>`) ve boş liste `none`'dur.

Tüm koşulları doğruysa `true` döndüren `my-and`'i yapalım.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; no arguments
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; just one
         `(if ,f (my-and ,@more) false)))             ; two or more
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)`, bir listenin başını `f`'ye, geri kalanını `more`'a alan bir örüntüdür.
- `sexpr-null`, S-ifade verisinin boş liste olup olmadığını sınar.
- Son `_` kolu gereklidir, çünkü S-ifade verisinin listelerden başka biçimleri de vardır (sayılar,
  string'ler vb.) ve `match` bunların da kapsanmasını ister. Bir `&rest` bağımsız değişkeni her
  zaman bir listedir; bu yüzden bu kol aslında hiç çalışmaz.
- Bir makro açılımında kendini çağırabilir. Açılım, hiç makro çağrısı kalmayana kadar tekrarlanır.

## 6. İsteğe bağlı bağımsız değişkenler

`&optional`, atlanabilen bağımsız değişkenleri alır. Varsayılan değerler verilebilir.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` anahtar sözcüklü bağımsız değişkenleri alır
([Sözdizimi Başvurusu 3.14](../reference/syntax.md#314-defmacro--makro-tanımları)).

## 7. `macrolet`: yalnızca tek bir yer için makrolar

Yalnızca tek bir ifadenin içinde kullanılan bir makro `macrolet` ile tanımlanabilir. Dışarıdan
görünmez.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Akılda tutulacak şeyler

- **Bir makro yalnızca tanımından sonra çağrılabilir.** Fonksiyonlarda olduğu gibi, onu dosyanın
  başına yakın tanımlayın.
- Bir makroyu diğer modüllere `(pub defmacro ...)` ile açın.
- `when`, `unless`, `cond`, `and`, `or` ve `dotimes` dahil standart sözdiziminin büyük bölümü makro
  olarak tanımlanmıştır. İçinde ne olduğunu `(macroexpand '(when true 1))` ile görebilirsiniz.
- Bir şey fonksiyon olarak yazılabiliyorsa fonksiyon olarak yazın. Makrolar değer olarak
  geçirilemez ve ne yaptıklarını anlamak için açılımlarını okumanız gerekir.

## 9. Sırada ne okunmalı

- [Hata Yönetimi](errors.md): `Result`, `panic`, `catch` / `throw`
- [Makro fonksiyonları](../reference/functions/system.md#8-makrolar): `gensym`, `macroexpand` ve daha
  fazlası
