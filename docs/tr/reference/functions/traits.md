<!-- translated-from: docs/ja/reference/functions/traits.md @ 1ba918af8af98a319b34010b9dc04df8c4d321e0 -->
# Standart Trait'ler

Yineleme, karşılaştırma, aritmetik ve öğe ekleme için trait'ler. Diğer standart trait'ler kendi bölümlerindedir:
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
  (next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>`, sırasıyla `vector-iter<T>`/`hashtable-iter<K,V>`/
`array-iter<T>` aracılığıyla `Iter`'i gerçekleştirir (yineleyiciyi `(iter collection)` ile alın).
`Chan<T>` kendisi bir `Iter`'dir (`recv`, `next` rolünü oynar; [Kanallar](concurrency.md#2-chant--kanallar)).
`Sexpr` listeleri `Iter`'i gerçekleştirmez (eleman türleri tekdüze değildir). Kendi türünüz için `Iter`'i
gerçekleştirirseniz `doiter` ile olduğu gibi gezilebilir ve
[dizi fonksiyonlarına](sequences.md#4-iter-üzerinde-dizi-fonksiyonları) geçirilebilir.

## 2. `Eq` / `Ord` (karşılaştırma)

Bunlar Rust'ın `PartialEq`/`PartialOrd`'una karşılık gelir (`Eq`/`Ord` olarak adlandırılmıştır).
Eleman türlerinin karşılaştırılabildiğini istemek için jenerik fonksiyonların `:where` sınırlarında
kullanılırlar (`sort`/`member`/`assoc` vb.).

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; must be implemented
  (not-equals (self (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less (self (other Self)) bool)                  ; must be implemented
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
```

`Eq`'i gerçekleştirmek için yalnızca `equals`, `Ord` için yalnızca `less` yazarsınız. Varsayılan
gerçekleştirmeler geri kalanını doldurur. `Ord`, `Eq`'ten miras alır; bu yüzden `impl Ord X`'ten önce
`impl Eq X` gerekir.

Her trait metodu olduğu gibi bir fonksiyon olarak çağrılabilir (bir `where (Eq A)`/`(Ord A)` sınırı
içinde ya da onu gerçekleştiren somut bir tür üzerinde):

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Eşit olup olmadıkları (Rust'ın `==`'i) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Eşit olmayıp olmadıkları (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` şunlar için gerçekleştirilmiştir: tüm sayısal türler (`i8`'den `u32`'ye / `f32` / `f64` / `int` /
`ratio`), `bool` `char` `string` `symbol` `complex`, `Sexpr` (`eq`, yani kimlik; `match`'in değer
örüntülerinde kullanılır) ve `cons-cell<A,B>` (elemanlar `Eq` olduğunda özyinelemeli olarak). `Ord`
şunlar için gerçekleştirilmiştir: tüm sayısal türler, `char` `string` ve `cons-cell<A,B>` (elemanlar
`Ord` olduğunda sözlük sırasıyla).

Metot adları, yerleşik işleçlerle (`= /= < <= > >=`) ya da `eq`/`lt` ile çakışmaz; çünkü yerleşikler
yeniden tanımlanamaz ve her gerçekleştirme onlara devreder. Skaler karşılaştırma işleçlerinin kendileri
her alıcı türünün yerleşik metotlarıdır ([Sayılar](numbers.md), [String'ler ve Karakterler](collections.md)).
Bir sınırın içinde işleçleri yazmak onları trait metotları olarak okur (3. bölüm).

## 3. Aritmetik trait'ler (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Jenerik kodun "toplanabilen bir tür" istemesi için bir katman. **Somut türler üzerindeki aritmetik
yerleşik işleçleri kullanır** ([Sayılar](numbers.md)) ve bu katmandan geçmez.

```lisp
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; metot yok; beşinin birleşimi
```

**Bir sınırın içinde işleç yazabilirsiniz.** Alıcı, `:where` ile bağlanmış bir tür değişkeni olduğunda
işleçler trait metotları olarak okunur (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

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
  (push (self (x Item)) ()))
```

Öğelerin teker teker eklenebildiği bir koleksiyon. Jenerik bir fonksiyon
`(:where (Push C (Item T)))` ile "`T` eklenebilen bir tür" ister ve bu tür her koleksiyona aynı
`(push c x)` ile ekler.

| Tür | `push` ne yapar |
|---|---|
| `Vector<T>` | Sona ekler |
| `Deque<T>` | Sona ekler (`push-back`) |
| `Array<T>` | Doldurma işaretçisinin yerine ekler (`push-extend`). Doldurma işaretçisi olmadan panic olur |
| `HashSet<T>` | Kümeye koyar (`insert`). Yalnızca öğe türü `Hash`'i uyguladığında `Push`'tur |

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
