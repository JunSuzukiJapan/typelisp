<!-- translated-from: docs/ja/tutorial/types.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# Tür Temelleri

typelisp statik tür denetimli bir dildir. Bu bölüm, tür denetleyicisinin sizin için ne yaptığını, en
çok kullanacağınız türleri (`Option`, `Result`, struct'lar ve enum'lar) ve jenerikleri açıklar.
[Başlarken](intro.md) bölümünü okuduğunuzu varsayar.

## 1. Statik tür denetimi ne demektir

typelisp'te her ifadenin türü, program çalışmadan önce belirlenir. Türleri uymayan bir ifade,
herhangi bir şey çalışmadan önce hatadır.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; type error

(main)
```

Bu dosyayı çalıştırmak, `start`'ı bile yazdırmadan bir tür hatasıyla durur.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Fonksiyon bağımsız değişkenleri ve dönüş değerleri, global değişkenler ve struct alanları için tür
yazmanız gerekir. Bir `let` değişkeninin türü ilk değerinden alınır.

Başlıca türler:

| Tür | Örnek değerler |
|---|---|
| `int` | `42`, `-7` (keyfi duyarlıklı tamsayılar) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Sabit genişlikli tamsayılar |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Değer döndürmeyen bir fonksiyonun dönüş türü |

Çalışma zamanında bir değerin türünü sormanın bir yolu yoktur (Common Lisp'in `typep` ya da
`type-of`'u yoktur), çünkü her tür program çalışmadan önce zaten bilinir.

## 2. `Option<T>`: eksik olabilen bir değer

typelisp'te `nil` yoktur. "Değer olmayabilir" durumu `Option<T>` türüyle ifade edilir. Bir
`Option<T>` değeri ya `T`'den tek bir değer tutan `some`'dır ya da hiçbir şey tutmayan `none`'dır.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>`, `int` değildir; bu yüzden olduğu gibi aritmetikte kullanılamaz.
`(+ (safe-div 10 2) 1)` bir tür hatasıdır. İçindekini kullanmak için `some` ile `none`'u `match` ile
ayırın.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- `(some q)` kolunda içerik `q` değişkenine bağlanır.
- `match`, kollarının **her durumu kapsadığını** denetler. `(none)` kolunu unutmak bir tür hatasıdır.

### Neden nil yok

Birçok dilde `nil` (`null`) herhangi bir türün değerinin yerine geçebilir. Sonuç olarak "değer yok"
durumunu ele almayı unutmak, program çalışana kadar fark edilmez. typelisp'te bir değerin eksik
olabileceği yer `Option<T>` türündedir ve kod, `match` `none` durumunu ele almadıkça tür
denetleyicisinden geçmez. Unutulan bir durum program çalışmadan önce bulunur.

Koşullar da aynı fikri izler: `if`'in koşulu yalnızca bir `bool` olabilir. Common Lisp'in "`nil`
dışındaki her şey doğrudur" gibi bir kuralı yoktur.

### Yaygın işlemler

| Form | Anlamı |
|---|---|
| `(unwrap-or opt default)` | `some` için içerik; `none` için varsayılan |
| `(unwrap opt)` | İçeriği çıkarır. `none` durumunda programı durdurur |
| `(is-some opt)` / `(is-none opt)` | Hangisi olduğunu sınar |

Birçok standart kütüphane fonksiyonu `Option` döndürür. Örneğin `position`, eleman bulunursa konumu
`some` içinde, bulunmazsa `none` döndürür.

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: başarısız olabilen bir işlem

Başarısız olabilen bir işlem `Result<T,E>` döndürür: başarıda `T` türünde bir değer tutan `ok`, ya da
başarısızlıkta bir `E` hatası tutan `err`.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Kendi fonksiyonlarınız da `Result` döndürebilir.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Eksik bir değerin açıklamaya ihtiyaç duymadığı durumlarda `Option`, bir şeyin neden başarısız
olduğunu söylemek istediğinizde `Result` kullanın. [Hata Yönetimi](errors.md) hataların ele alınmasını
ayrıntılı olarak işler.

## 4. `defstruct`: struct'lar

Adlandırılmış alanları olan bir tür `defstruct` ile tanımlanır.

```lisp
(defstruct point
  (x int)
  (y int))
```

Tanım size şunları verir:

```lisp
(let ((p (point::new 3 4)))     ; create one (arguments in field order)
  (println "~a" p::x)           ; read a field; (x p) also works
  (setf p::x 10)                ; change it
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Bir struct'a kendine ait fonksiyonlar vermek için `defmethod` kullanın. İlk bağımsız değişkenin
(`self`) türü, metodun hangi türe ait olduğunu belirler.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Bir `self` bağımsız değişkeni yerine yalnızca tür adını yazmak, `point::origin` olarak çağrılan bir
fonksiyon yapar.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: birkaç biçimden biri

"Bir daire, bir dikdörtgen ya da bir nokta" gibi birkaç biçimden biri olan bir değer `defenum` ile
tanımlanır. Her biçime **varyant** denir. Her varyant farklı sayıda ve türde değer tutabilir.

```lisp
(defenum shape
  (circle int)        ; radius
  (rect int int)      ; width and height
  (dot))              ; holds no value
```

Değerler, `shape::circle` gibi önüne tür adı konarak yapılır. `match` içinde varyant adına göre
ayrıştırılırlar.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Burada da `match` her durumun kapsandığını denetler. Daha sonra `shape`'e bir varyant eklerseniz, onu
ele almayan her `match` bir tür hatası olur; böylece düzeltilmesi gereken hiçbir yer gözden kaçmaz.

`(use shape)`'ten sonra tür adı olmadan `(rect 5 6)` yazabilirsiniz.

`Option` ve `Result`, aynı mekanizmayla yapılmış enum'lardır.

## 6. Jenerikler

Her tür için çalışan bir fonksiyon, adından sonra bir **tür parametresi** `<T>` ile tanımlanır.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Çağırırken türü vermezsiniz. `T` bağımsız değişkenlerden çıkarılır.

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T is int
(first-or names "none")    ; T is string
(first-or ints "none")     ; type error: ints is a Vector<int>, so T is int
```

Struct'lar ve enum'lar da jenerik olabilir. `Vector<T>`, `Option<T>` ve `Result<T,E>` bu türden
türlerdir.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Jenerik bir fonksiyonun içinde `T` hakkında hiçbir şey bilinmez; bu yüzden `T` türündeki değerleri
karşılaştıramaz ya da toplayamazsınız. "Karşılaştırılabilen her tür" gibi bir şey istemek için
trait'leri kullanın ([Trait'ler](traits.md)).

## 7. Bir türe başka bir ad verme

`deftype` bir türe başka bir ad verir.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters`, `int`'in yalnızca başka bir yazımıdır; yeni bir tür değildir. `meters` beklenen yere düz bir
`int` geçirmek hata değildir. Bunları ayrı tutmak istiyorsanız `(defstruct meters (value int))` gibi
bir struct yapın.

## 8. Sırada ne okunmalı

- [Trait'ler](traits.md): türlere ortak işlemler verme
- [Türler](../reference/types.md): yerleşik türler ve her birinin gerçekleştirdiği trait'ler
