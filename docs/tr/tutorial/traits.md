<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Trait'ler

Bir trait, "bu tür şu işlemleri destekler" sözüdür. Trait'ler birkaç türün aynı adlı işlemleri
paylaşmasını sağlar; böylece onları kullanan bir fonksiyonun her tür için ayrı ayrı yazılması
gerekmez. Rust'ın trait'leriyle neredeyse aynı çalışırlar. Bu bölüm [Tür Temelleri](types.md)
bölümünü okuduğunuzu varsayar.

## 1. Bir trait tanımlama ve gerçekleştirme

Bir şeklin alanını ve adını döndüren işlemleri `Shape` trait'i olarak tanımlayın.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- Trait adından sonraki `()`, miras aldığı trait'lerin listesidir (bölüm 4). Hiç yoksa boş bırakın.
- Her satır bir metot bildirir. `Self`, "bu trait'i gerçekleştiren tür" anlamına gelir.

Bir trait'i bir tür için gerçekleştirmek üzere bir `impl` yazın.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

Gerçekleştirilen metotlar sıradan fonksiyonlar gibi çağrılır.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Trait'in bildirdiği metotlardan birini bile atlamak, `impl`'de bir tür hatasıdır.

## 2. Trait sınırları: "bu trait'i gerçekleştiren herhangi bir tür"

Jenerik bir fonksiyonun tür parametresine `where` ile bir koşul koyabilirsiniz. Buna **trait
sınırı** denir.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

`(where (Shape T))` sayesinde gövde, `T` türündeki değerlerde `name` ve `area`'yı kullanabilir. Sınır
olmadan `T` hakkında hiçbir şey bilinmez; bu yüzden onlar çağrılamazdı.

`Shape`'i gerçekleştirmeyen bir türü geçirmek, çağrıda bir tür hatasıdır.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Jenerik bir fonksiyon, çağrıldığı her tür için kendi kopyasını alır. Çalışma zamanında tür sınaması
ya da dallanma söz konusu değildir.

## 3. Varsayılan gerçekleştirmeler

Bir trait metodunun gövdesi varsa, bir `impl` o metodu atladığında bu gövde kullanılır.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe is the default one

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; the one written here takes priority

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Standart trait'leri gerçekleştirme

Standart kütüphanenin de trait'leri vardır. Birini gerçekleştirmek, onu kullanan standart
fonksiyonları türünüz için kullanılabilir kılar.

| Trait | Gerçekleştirilecek metotlar | Neyi mümkün kılar |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, `match`'in `(= expr)` örüntüsü vb. |
| `Ord` | `less` | `less-equal`, `greater` vb. `Ord`, `Eq`'ten miras alır |
| `print-object` | `print-object` | Değerlerin `println` ve benzerleriyle nasıl gösterileceği |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` vb. |
| `Error` | `message`, `source` | Hata türü olarak kullanım ([Hata Yönetimi](errors.md)) |

Bir para miktarını temsil eden bir tür için `Eq` ve `Ord`'u gerçekleştirelim. `Ord`, `Eq`'ten miras
aldığından önce `Eq`'in `impl`'i gelmelidir.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (the default implementation in Ord)
```

`print-object`'i gerçekleştirmek, `println`'in değeri nasıl göstereceğini belirler. `escape`
bağımsız değişkeni, `~s` gibi geri okunabilecek bir biçim istendiğinde `true`'dur.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Bir trait sınırıyla birleştirildiğinde, `Ord`'u gerçekleştiren her tür için çalışan bir fonksiyon
yazabilirsiniz.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Sırasıyla 300, 900 ve 100 değerli `money` içeren bir `Vector` verildiğinde `(some 900 yen)` döndürür.

## 5. `:dyn`: farklı türlerin değerlerini birlikte ele alma

Bir `Vector<T>`'nin tüm elemanları aynı türdedir; bu yüzden `circle` ve `rect` değerleri tek bir
`Vector<circle>` içine giremez. "`Shape`'i gerçekleştiren herhangi bir şeyi" birlikte ele almak için
`:dyn Shape` türünü kullanın.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Bir `:dyn Shape` beklenen yere konan bir `circle` ya da `rect` değeri otomatik olarak dönüştürülür.
- `(area s)` çağrısının hangi türün `area`'sını çalıştıracağı, `s`'nin tuttuğu şeyin türüne göre
  çalışma zamanında belirlenir.
- Türü `Shape`'i gerçekleştirmeyen bir değeri `:dyn Shape` beklenen yere koymak bir tür hatasıdır.

Bölüm 2'deki trait sınırları ile `:dyn` arasında seçim:

| | Trait sınırı (`where`) | `:dyn Trait` |
|---|---|---|
| Çağrılan metodun belirlendiği an | Çalışmadan önce | Çalışma zamanında |
| Tek bir `Vector`'de türleri karıştırma | Mümkün değil | Mümkün |
| Kullanılabilen türler | Kısıtlama yok | Struct'lar, enum'lar, `int`, `string`, `f64` ve diğerleri (`bool`, `char`, `symbol`, `i32` ve benzerleri değil) |

Kullanılabilen türlerin kesin listesi
[Sözdizimi Başvurusu 3.9](../reference/syntax.md#39-deftrait--impl--traitler)'dadır.

Bazı trait'ler `:dyn` ile kullanılamaz: metotları `self` dışındaki bir bağımsız değişken için ya da
dönüş değeri için `Self` kullananlar (`Eq`'teki `equals` gibi). Tür çalışma zamanına kadar
bilinmediğinden, "aynı türden bir değer" üretmenin bir yolu yoktur.

## 6. Kısıtlamalar

- Bir trait'in tanımını, onun `impl`'lerini ve onu `:dyn` ile kullanan kodu tek bir modülde (dosyada)
  tutun. Bir trait henüz diğer modüllere görünür yapılamaz.
- Türler ve trait'ler tek bir ad alanını paylaşır. Tek bir modül içinde bir tür ile bir trait aynı
  ada sahip olamaz.

## 7. Sırada ne okunmalı

- [Makrolar](macros.md): kendi sözdiziminizi tanımlama
- [Sözdizimi Başvurusu 3.9](../reference/syntax.md#39-deftrait--impl--traitler): blanket
  gerçekleştirmeler, ilişkili türler ve daha fazlası
- [Standart Trait'ler](../reference/functions/traits.md): standart kütüphanedeki trait'lerin listesi
