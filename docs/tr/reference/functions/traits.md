<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Standart Trait'ler

Yineleme, karşılaştırma ve aritmetik için trait'ler. Diğer standart trait'ler kendi bölümlerindedir:
`Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Hata türleri](option-result.md#3-hata-türleri-ve-error-traiti)), `print-object`
([Yazdırma](printing.md#5-print-object-türe-göre-yazdırılan-gösterim)) ve akış trait'leri ile `Pathish`
([Akışlar ve Dosyalar](streams-files.md)). Hangi türlerin hangisini gerçekleştirdiği
[Türler](../types.md) belgesindedir. Trait'lerin nasıl tanımlanacağı
[Sözdizimi Başvurusu](../syntax.md#39-deftrait--impl--traitler)'ndadır.

## 1. `Iter` trait'i ve yineleme

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>`, sırasıyla `vector-iter<T>`/`hashtable-iter<K,V>`/
`array-iter<T>` aracılığıyla `Iter`'i gerçekleştirir (yineleyiciyi `(iter collection)` ile alın).
`Chan<T>` kendisi bir `Iter`'dir (`recv`, `next` rolünü oynar; [Kanallar](concurrency.md#2-chant--kanallar)).
`Sexpr` listeleri `Iter`'i gerçekleştirmez (eleman türleri tekdüze değildir). Kendi türünüz için `Iter`'i
gerçekleştirirseniz `doiter` ile olduğu gibi gezilebilir ve
[dizi fonksiyonlarına](sequences.md#4-iter-üzerinde-dizi-fonksiyonları) geçirilebilir.

## 2. `Eq` / `Ord` (karşılaştırma)

Bunlar Rust'ın `PartialEq`/`PartialOrd`'una karşılık gelir (`Eq`/`Ord` olarak adlandırılmıştır).
Eleman türlerinin karşılaştırılabildiğini istemek için jenerik fonksiyonların `where` sınırlarında
kullanılırlar (`sort`/`member`/`assoc` vb.).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; must be implemented
  (not-equals ((self Self) (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less ((self Self) (other Self)) bool)                  ; must be implemented
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
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
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; no methods; a combination of six
```

**Bir sınırın içinde işleç yazabilirsiniz.** Alıcı, `where` ile bağlanmış bir tür değişkeni olduğunda
işleçler trait metotları olarak okunur (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Trait metodu `+` olarak adlandırılmamıştır; çünkü `+`, yerleşik bir metodun adıdır ve `impl` onu yeniden
tanımlamayı reddeder (`cannot redefine built-in method`). `Neg` yoktur: `(- x)`, `(- (- x x) x)`'e açılır;
bu yüzden `Sub` yeterlidir.

Şunlar için gerçekleştirilmiştir: tüm sayısal türlerde (`complex` hariç) `Add`/`Sub`/`Mul`/`Div`/`Rem`/
`Number` ve tüm tamsayı türlerinde ve `int`'te `Bits`.
