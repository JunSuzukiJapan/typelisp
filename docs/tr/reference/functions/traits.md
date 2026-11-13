<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Standart Trait'ler

Yineleme, karşılaştırma, aritmetik ve öğe ekleme ile çıkarma için trait'ler. Diğer standart trait'ler kendi bölümlerindedir:
`Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Hata türleri](option-result.md#3-hata-türleri-ve-error-traiti)), `print-object`
([Yazdırma](printing.md#5-print-object-türe-göre-yazdırılan-gösterim)) ve akış trait'leri ile `Pathish`
([Akışlar ve Dosyalar](streams-files.md)). Hangi türlerin hangisini gerçekleştirdiği
[Türler](../types.md) belgesindedir. Trait'lerin nasıl tanımlanacağı
[Sözdizimi Başvurusu](../syntax.md#39-deftrait--impl--traitler)'ndadır.

## 1. `Iter` trait'i ve yineleme

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>`, sırasıyla `vector-iter<T>`/`hashtable-iter<K,V>`/
`array-iter<T>` aracılığıyla `Iter`'i gerçekleştirir (yineleyiciyi `(iter collection)` ile alın).
`Chan<T>` kendisi bir `Iter`'dir (`recv`, `next` rolünü oynar; [Kanallar](concurrency.md#2-chant--kanallar)).
`Sexpr` listeleri `Iter`'i gerçekleştirmez (eleman türleri tekdüze değildir). Kendi türünüz için `Iter`'i
gerçekleştirirseniz `doiter` ile olduğu gibi gezilebilir ve
[dizi fonksiyonlarına](sequences.md#4-iter-üzerinde-dizi-fonksiyonları) geçirilebilir.

## 2. `Equal` / `Ord` (karşılaştırma)

`Equal`, `equal`'ın kullandığı trait'tir ve Rust'taki `PartialEq`'e karşılık gelir. `Ord`, Rust'taki
`PartialOrd`'a karşılık gelir. Genel fonksiyonların `:where` sınırlarında, eleman türünün
karşılaştırılabilmesini istemek için kullanılırlar.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; uygulanmalı
(deftrait Ord (Equal)                                     ; Equal'dan kalıtır
  (defmethod less (self (other Self)) bool)                  ; uygulanmalı
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

`Equal`'ı uygulamak için `equal`, `Ord` için yalnızca `less` yazılır. `Ord`'un geri kalanını varsayılan
uygulamalar doldurur. `Ord`, `Equal`'dan kalıttığı için `impl Ord X`'ten önce `impl Equal X` gerekir.

`(equal a b)`, `a`'nın türünün `Equal` uygulamasını çağırır. `Equal`'ı uygulamayan bir tür için yazmak
tür hatasıdır. Kimliğe göre karşılaştırmak için `eq`/`eql`, yapıya göre karşılaştırmak için `equalp`
kullanılır. Bu üçü her türden değerde kullanılabilir.

Her trait metodu doğrudan bir fonksiyon olarak çağrılabilir (bir `where (Equal A)`/`(Ord A)` sınırı içinde
ya da onu uygulayan somut bir türde):

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Eşit olup olmadıkları (Rust'taki `==`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` şunlar için uygulanmıştır: tüm sayısal türler (`i8`'den `u32`'ye / `f32` / `f64` / `int` /
`ratio`), `bool` `char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` ve demetler (elemanlar
`Equal` olduğunda eleman eleman), ayrıca `Option<T>` ve `Result<T,E>` (aynı varyant olup içerikleri
`equal` olduğunda). Yerleşik türlerde, türün kendi yerleşik `equal` metodu olduğu gibi `Equal`
uygulamasıdır. Sayılar `=` ile, dizgeler büyük-küçük harf ayrımıyla içeriğe göre karşılaştırılır. `Ord`
şunlar için uygulanmıştır: tüm sayısal türler, `char` `string` ve `cons-cell<A,B>` (elemanlar `Ord`
olduğunda sözlüksel olarak).

Bir listenin (`Sexpr`) `equal`'ı CL'in `equal`'ıdır: cons'lar yapıya, dizgeler içeriğe, diğer her atom
`eql`'e göre karşılaştırılır. Bu karşılaştırma değerleri çalışma zamanında dolaşır ve hiçbir metot
çağırmaz. Bu yüzden bir liste içindeki struct, türünün `impl Equal`'ı ne olursa olsun, CL'deki gibi
kimliğe göre karşılaştırılır.

`Ord`'un metot adları yerleşik operatörlerle (`< <= > >=`) ya da `lt` ile çakışmaz; çünkü yerleşikler
yeniden tanımlanamaz ve her uygulama onlara devreder. Skaler karşılaştırma operatörlerinin kendisi,
alıcı türüne göre yerleşik metotlardır ([Sayılar](numbers.md), [Dizgeler ve karakterler](collections.md)).
Bir sınır içinde yazılan operatör trait'in metodu olarak okunur (3. bölüm). `=` `equal` olur, `/=` ise
`(not (equal a b))` olur.

## 3. Aritmetik trait'ler (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Jenerik kodun "toplanabilen bir tür" istemesi için bir katman. **Somut türler üzerindeki aritmetik
yerleşik işleçleri kullanır** ([Sayılar](numbers.md)) ve bu katmandan geçmez.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; metot yok; beşinin birleşimi
```

**Bir sınırın içinde işleç yazabilirsiniz.** Alıcı, `:where` ile bağlanmış bir tür değişkeni olduğunda
işleçler trait metotları olarak okunur (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Trait metodu `+` olarak adlandırılmamıştır; çünkü `+`, yerleşik bir metodun adıdır ve `impl` onu yeniden
tanımlamayı reddeder (`cannot redefine built-in method`). `Neg` yoktur: `(- x)`, `(- (- x x) x)`'e açılır;
bu yüzden `Sub` yeterlidir.

Şunlar için gerçekleştirilmiştir: tüm sayısal türlerde (`complex` hariç) `Add`/`Sub`/`Mul`/`Div`/`Rem`/
`Number` ve tüm tamsayı türlerinde ve `int`'te `Bits`.


**`Div`, sonuç tipini ilişkili tip `Output` ile adlandırır.** CL'deki gibi iki tam sayı üzerinde `/`
bir `rational`'dır; bu yüzden `int`'in `Output`'u `rational`, diğer her sayısal tipinki tipin
kendisidir. `Output` tipten tipe değiştiği için `Number`, `Div`'i içermez (üst trait listesi her
ilişkili tipi sabitlemelidir). Bölme yapan genel bir fonksiyon sonuç tipini
`(:where (Div T (Output R)))` ile alır ya da `(:where (Div T (Output T)))` ile `T` içinde kapalı bir
bölme ister (o zaman `int` geçirilemez).

## 4. `Push` trait'i (öğe ekleme)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

Öğelerin teker teker eklenebildiği bir koleksiyon. Jenerik bir fonksiyon
`(:where (Push C (Item T)))` ile "`T` eklenebilen bir tür" ister ve bu tür her koleksiyona aynı
`(push c x)` ile ekler.

| Tür | `push` ne yapar |
|---|---|
| `Vector<T>` | Sona ekler |
| `Deque<T>` | Sona ekler (`push-back`) |
| `Array<T>` | Doldurma işaretçisinin yerine ekler (`push-extend`). Doldurma işaretçisi olmadan panic olur |
| `HashSet<T>` | Kümeye koyar (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

`Push`'u uygulayan kendi türünüz aynı jenerik fonksiyonlara geçirilebilir. `:dyn Push<int>` gibi bir
trait nesnesi de olabilir.

## 5. `Pop` trait'i (öğe çıkarma)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

`push` ile en son eklenen öğenin geri çıkarılabildiği koleksiyon. Boşsa `none`. Genel bir fonksiyon
bunu `(:where (Pop C (Item T)))` ile ister.

| Tür | `pop` ne yapar |
|---|---|
| `Vector<T>` | Son öğeyi çıkarır |
| `Deque<T>` | Son öğeyi çıkarır (`pop-back`) |
| `Array<T>` | Doldurma işaretçisinin hemen önündeki öğeyi çıkarır. Doldurma işaretçisi yoksa panic |

`HashSet<T>`'nin "son öğesi" yoktur; bu yüzden `Push`'tur ama `Pop` değildir.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

`Push` gibi, kendi türünüz de bunu uygulayabilir ve `:dyn Pop<int>` gibi bir trait nesnesi olabilir.
