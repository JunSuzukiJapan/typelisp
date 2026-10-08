<!-- translated-from: docs/ja/tutorial/intro.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# Başlarken

Bu bölüm, REPL'de ifade değerlendirmekten başlayarak sırasıyla fonksiyonları, değişkenleri,
koşulları, döngüleri, listeleri ve `Vector`'ü ele alır. `typl`'nin nasıl derleneceği için
[README.md](../../../README.md) dosyasına bakın.

## 1. REPL'i başlatma

Bağımsız değişken olmadan başlatılan `typl`, REPL'e (etkileşimli kip) girer. `typl>` isteminden sonra
bir ifade yazın; ifade anında değerlendirilir ve değeri yazdırılır. `:quit` REPL'den çıkar.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Bundan sonra REPL girdisi ve sonuçları bu biçimde gösterilir.

## 2. İfadeleri değerlendirme

typelisp bir Lisp'tir; bu yüzden bir ifade, **işleç ya da fonksiyon adı başta olacak şekilde**
parantezlere sarılır. `1 + 2` değil, `(+ 1 2)` yazarsınız.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Sayılar şu türlerde gelir:

- **Tamsayılar** `int` türündedir. Boyutlarının üst sınırı yoktur.
- **Ondalıklı sayılar** `f64` türündedir. `1.5` ya da `2.0` gibi ondalık noktayla yazılırlar.
- Bir hesaplamada `int` ile `f64`'ü karıştıramazsınız. `(+ 1 2.0)` bir tür hatasıdır. Dönüştürmek
  için `(as f64 1)` yazın.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

İki tamsayı üzerinde `/`, kesirli kısmı atılmış bir tamsayı verir (Common Lisp'teki gibi bir kesir
üretmez). Kalan için `(mod 7 2)` kullanın.

Mantıksal değerler `true` ve `false`'tur.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Fonksiyon tanımlama

Fonksiyonlar `defun` ile tanımlanır. **Bağımsız değişken türleri ve dönüş türü her zaman yazılır.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)`, "`int` türünde bir `n` bağımsız değişkeni" demektir. Birden çok bağımsız değişken varsa
  hepsini sıralayın: `((a int) (b int))`.
- Bağımsız değişken listesinden sonraki `int` dönüş türüdür.
- Gövdedeki son ifadenin değeri fonksiyonun dönüş değeridir. `return` yazmazsınız.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Türleri uyuşmayan bir çağrı, **çalışmadan önce** tür hatası olarak bildirilir. Bir dosyayı
çalıştırırken herhangi bir yerdeki tek bir tür hatası, programın tek bir satırının bile
çalışmaması demektir.

Bir bağımsız değişkeni isteğe bağlı yapmak için `&optional` kullanın. Bir varsayılan değer verirseniz,
bağımsız değişken atlandığında o değeri alır.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

`format`'ın ilk bağımsız değişkeni olarak verilen `false`, "sonucu yazdırmak yerine bir string olarak
döndür" demektir. Her `~a`, sıradaki bağımsız değişkenle değiştirilir.

## 4. Değişkenler

Yerel değişkenler `let` ile oluşturulur.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- Bir `let` değişkeninin türü ilk değerinden alınır. Yazmanız gerekmez.
- Bir `let`'in değişkenleri birbirine başvuramaz. Bir değişkeni kendisinden öncekinden oluşturmak
  için `let*` kullanın.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Bir değişkenin değerini değiştirmek için `setf` kullanın. **Atama, değişkenin türünü değiştiremez.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Global değişkenler `defvar` ile tanımlanır. Burada türü yazarsınız.

```lisp
(defvar (counter int) 0)
```

## 5. Koşullar

### if

`(if koşul then-ifadesi else-ifadesi)` yazın. **Else ifadesi atlanamaz.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Yalnızca `bool` türündeki bir ifade koşul olabilir. `(if 0 ...)` gibi bir sayı yazmak tür hatasıdır.
- Then ve else ifadelerinin türü aynı olmalıdır.

Yanlış durumda hiçbir şey olmaması gerektiğinde `when` kullanın (tersi için `unless`).

```lisp
(defun report-size ((n int)) ()
  (when (> n 100)
    (println "large")
    (println "really large")))
```

### cond

Üç ya da daha fazla koşulda `cond` daha okunaklıdır. Hiçbir koşul sağlanmadığında son `else` alınır.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Bir değerin biçimine göre dallanmak için `match` kullanın.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` her değerle eşleşir. `int`'in sayısız değeri olduğundan, `_` kolunu atlamak, tüm durumların
kapsanmadığını söyleyen bir hatadır. `match`'in asıl parladığı yer, bir sonraki bölümde,
[Tür Temelleri](types.md)'nde karşımıza çıkan `Option`'ı ve kendi tanımladığınız türleri
ayrıştırmaktır.

## 6. Döngüler

Bir fonksiyon kendini çağırabilir.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Sabit sayıda tekrar için `dotimes` kullanın. `i`, 0'dan `n - 1`'e kadar gider.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

Ayrıca `while`, `do` ve Common Lisp'ten gelen genişletilmiş `loop` da vardır. Genişletilmiş
`loop`'un yan tümce sözcükleri anahtar sözcükler olarak yazılır (`:for`, `:collect` vb.).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. Listeler ve Vector

### Vector

Aynı türden bir değer dizisini tutmak için `Vector<T>` kullanın. `T` eleman türüdür.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; prints #<vector<int> 3 1 2>
```

- Yalnızca `(Vector::new)` eleman türünü belirlemez; bu yüzden türü `(the Vector<int> ...)` ile verin.
- `(push v x)` sona ekler, `(get v i)` `i`'inci elemanı okur ve `(len v)` uzunluğu verir.
- Aralık dışı bir indeksle `get`, programı bir hatayla durdurur.

### lambda ve yüksek dereceli fonksiyonlar

Anonim fonksiyonlar `lambda` ile yapılır. `defun`'daki gibi bağımsız değişken ve dönüş türlerini
yazarsınız.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` ve benzerleri, `(iter v)` ile bir **yineleyiciye** çevrilmiş bir
`Vector` alır. Koleksiyon önce, fonksiyon sonra gelir. Yukarıdaki `v`, `let` ile bağlandığı için o
`let`'in dışında kullanılamaz. Sonraki örnek önce `v`'yi `defvar` ile tanımlar.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Elemanları tek tek işlemek için `doiter` kullanın.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Bağımsız değişken olarak bir fonksiyon alan bir fonksiyon, o bağımsız değişkenin türünü
`(fn (bağımsız-değişken-türleri...) dönüş-türü)` olarak yazar. `defun` ile tanımlanmış bir fonksiyon,
adıyla değer olarak geçirilebilir.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Listeler (S-ifadeler)

`'(1 2 3)` ya da `(list 1 2 3)` ile yapılan listeler **S-ifade verisidir**. Elemanlarının aynı türde
olması gerekmez.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

S-ifade verisi esas olarak programların kendisini işlemek içindir: makrolarda ([Makrolar](macros.md))
ve `read` ile. Elemanlarının türü bilinen veriler için `Vector<T>` kullanın. Bir S-ifade listesinde
`dolist` ile gezilebilir.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

İki değerlik bir çift `cons` ile yapılır ve `car` ile `cdr` ile ayrıştırılır.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Bir dosyada program yazma

Bir program bir dosyaya (`.typl` uzantılı) yazılıp `typl dosya-adı` ile çalıştırılabilir. Sonuçları
göstermek için `println` kullanın.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println`, `format` ile aynı yönergeleri kullanarak yazdırır ve yeni satırla biter. `print` yeni
  satır eklemez.
- `~a` bir değeri insanın okuyabileceği biçimde, `~s` ise geri okunabilecek bir biçimde gömer
  (string'ler `"` alır).
- Bir dosya yukarıdan aşağıya okunur. **Bir fonksiyon, tanımından önce çağrılamaz.**

## 9. Sırada ne okunmalı

- [Tür Temelleri](types.md): `Option`, `Result`, struct'lar, enum'lar, jenerikler
- [Common Lisp Programcıları İçin](../guide/from-common-lisp.md): Common Lisp bilenler için
  farkların listesi
