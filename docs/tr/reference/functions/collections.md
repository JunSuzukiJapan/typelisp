<!-- translated-from: docs/ja/reference/functions/collections.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# String'ler, Karakterler ve Koleksiyonlar

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` ve `BitVector`.

## 1. String'ler `string`

String'ler değiştirilemezdir.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Büyük harfe çevirir (yalnızca ASCII). CL'nin `string-upcase`'i gibi yeni bir string döndürür. String'ler değiştirilemez olduğundan yıkıcı bir `nstring-upcase` yoktur; bu onun yerini alır |
| `downcase` | `(downcase s)` | `string→string` | Küçük harfe çevirir (yalnızca ASCII). `nstring-downcase`'in yerini alır |
| `capitalize` | `(capitalize s)` | `string→string` | Her sözcüğün ilk harfini büyük, geri kalanını küçük yapar (CL'nin `string-capitalize`'ı). Sözcük, harf ve rakamlardan oluşan en uzun dizidir |
| `length` | `(length s)` | `string→int` | Karakter sayısı |
| `ref` | `(ref s i)` | `(string,int)→char` | `i`'inci karakter. Aralık dışında panic olur |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | `[start,end)` alt string'i |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Birleştirme. Üç ya da daha fazlası verilebilir (`(concatenate 'string ...)` ile aynı) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Sözlük sırasıyla karşılaştırma |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Katı sözlük sırasıyla küçüktür (`<` ile aynı) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Kimlik karşılaştırması (içeriğin değil, aynı nesne olup olmadığının) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | İçeriği karşılaştırır (büyük/küçük harf duyarlı) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | İçeriği karşılaştırır (büyük/küçük harf duyarsız, yalnızca ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | İçeriğin farklı olup olmadığı (CL'nin `string/=`'i. Çok bağımsız değişkenli biçim bitişik çiftleri karşılaştırır) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Büyük/küçük harf duyarsız sıralama (CL'nin `string-lessp`'i vb.). Ortak bir önek varsa kısa olan küçüktür |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | `c`'nin `n` kopyasından oluşan bir string (CL'nin `make-string`'i) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | `sub`'ın ilk göründüğü konum. **CL'nin `search`'ünde bağımsız değişkenler ters sıradadır** (`(search pattern sequence)`). Boş string 0'da bulunur. Anahtar sözcükler için [dizilerin anahtar sözcüklü bağımsız değişkenlerine](sequences.md#6-anahtar-sözcüklü-bağımsız-değişkenler) bakın |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | İlk farklılaşma konumu. Yalnızca `equal` olduklarında `none`. Biri diğerinin önekiyse kısa olanın sonu. Anahtar sözcükler yukarıdaki gibi |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | `bag` içindeki karakterleri iki uçtan / soldan / sağdan kaldırır (CL'nin `string-trim`'i vb.). `bag` olmadan boşluk `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | `sep` noktalarından böler. CL'de karşılığı yoktur. Ardışık ayırıcılar boş elemanlar üretir. `sep` boşsa panic olur |
| `to-string` | `(to-string x)` | `T→string` | `~a`'nın yaptığı gibi bir string'e çevirir. `int`/`i32`/`f64`/`bool`/`char`/`string` için gerçekleştirilmiştir (CL'nin `princ-to-string`'i) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | UTF-8 olarak kodlar (her eleman 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Çözer. Geçerli UTF-8 değilse `none` |

## 2. Karakterler `char`

Bir `char`, bir Unicode skaler değeridir. Büyük/küçük harf dönüşümü ve sınıflandırma yalnızca ASCII aralığını ele alır.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Büyük harfe çevirir (yalnızca ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Küçük harfe çevirir (yalnızca ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Kod noktasına göre karşılaştırma |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Kod noktasına göre katı küçüktür (`<` ile aynı) |
| `alphap` | `(alphap c)` | `char→bool` | Bir ASCII harfi olup olmadığı |
| `digitp` | `(digitp c)` | `char→bool` | Bir ASCII rakamı olup olmadığı |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Değerleri karşılaştırır |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Değerleri harf büyüklüğünü yok sayarak karşılaştırır (CL'nin `char-equal`'ı) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Değerlerin farklı olup olmadığı (CL'nin `char/=`'i. **Çok bağımsız değişkenli biçim bitişik çiftleri karşılaştırır**; CL ise tüm çiftlerin farklı olup olmadığını sorar) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Büyük/küçük harf duyarsız sıralama (CL'nin `char-lessp`'i vb.) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Büyük harf / küçük harf / hiç harf büyüklüğü ayrımı var mı (CL'nin `upper-case-p`'si vb.) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Bir harf ya da rakam (CL'deki ad aynı) |
| `graphicp` | `(graphicp c)` | `char→bool` | Yazdırılabilir olup olmadığı. Boşluğu içerir; yeni satırı ya da sekmeyi içermez (CL'nin `graphic-char-p`'si) |
| `standardp` | `(standardp c)` | `char→bool` | CL'nin 96 standart karakterinden biri olup olmadığı, yani `graphicp` artı yeni satır (CL'nin `standard-char-p`'si) |
| `char->int` | `(char->int c)` | `char→int` | Unicode skaler değeri (tersi, [Sayılar](numbers.md#1-sabit-genişlikli-tamsayılar) içindeki `int->char`/`try-int->char`'dır). CL'nin `char-code`/`char-int`'ine karşılık gelir |
| `char->string` | `(char->string c)` | `char→string` | Tek karakterlik bir string. CL'nin `string` fonksiyonu bir belirleyici alarak bunu kapsar, ancak bu dilde belirleyici yoktur; bu yüzden yön adın içindedir |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | Rakamın o tabandaki **ağırlığı** (CL'nin `digit-char-p`'si). `digitp`, `bool` döndüren ayrı bir fonksiyondur |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | `w` ağırlığının karakteri. 10 ve üzeri için büyük harf (CL'nin `digit-char`'ı; taban en çok 36'dır) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Karakterin adı. Yalnızca okuyucunun okuyabildiği adlandırılmış karakterlerin adı vardır (CL'nin `char-name`'i) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Bir adın karakteri. Büyük/küçük harf duyarsızdır ve okuyucunun takma adlarını da (`linefeed`/`null`) kabul eder (CL'nin `name-char`'ı) |

`char-code-limit`'e karşılık gelen bir sabit yoktur (`char`'ın üst sınırını dil değil, Unicode belirler).

## 3. `Vector<T>`

Büyüyebilen bir dizi.
Bir değer `#(1 2 3)` olarak yazılabilir ([Sözdizimi Başvurusu](../syntax.md#1-sözcüksel-öğeler);
eleman türü bağlamdan ya da ilk elemandan gelir ve her değerlendirme yeni bir vektör oluşturur).
Yazdırıldığında da `#(1 2 3)` olur.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Boş bir vektör yapar. Tür bağımsız değişkeni beklenen türden gelir; bu yüzden çıplak bir `let` içinde `(the Vector<i32> (Vector::new))` yazın |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x`'in `n` kopyası |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Sona ekler |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | `i`'inci elemanı okur. Aralık dışında panic olur |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | `i`'inci elemanı değiştirir. Aralık dışında panic olur. `(setf (get v i) x)` olarak da yazılabilir |
| `len` | `(len v)` | `Vector<T>→int` | Eleman sayısı |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Son elemanı kaldırır ve döndürür. Boşsa `None` (`get`/`set`'ten farklı olarak panic olmaz) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | `Iter`'i gerçekleştiren bir yineleyici yapar |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Eşit bir eleman yoksa `x`'i ekler (CL'nin `pushnew`'i. Bir yeri yeniden yazmasına gerek olmadığından makro değil metottur) |

`map`/`filter` ve benzerleri [dizi fonksiyonlarıdır](sequences.md#4-iter-üzerinde-dizi-fonksiyonları):
vektörü `(map (iter v) f)` gibi `iter`'den geçirin. Yıkıcı işlemler (`nreverse`, `delete` vb.)
[Yıkıcı işlemler](sequences.md#7-yıkıcı-işlemler) bölümündedir.

## 4. `HashTable<K,V>`

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Boş bir tablo yapar |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Arama |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Ekler ya da üzerine yazar |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Girdiyi kaldırır ve varsa eski değeri döndürür |
| `count` | `(count h)` | `HashTable<K,V>→int` | Girdi sayısı |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Her şeyi kaldırır |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Anahtarların bir anlık görüntüsü |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Değerlerin bir anlık görüntüsü |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | `(k . v)` çiftlerinin bir anlık görüntüsü |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | `Iter`'i gerçekleştiren bir yineleyici. Elemanlar `(k . v)` `cons-cell`'leridir. CL'nin `with-hash-table-iterator`'ına karşılık gelir; `doiter`/`map`/`filter` ve diğerleri onun üzerinde olduğu gibi çalışır |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | CL'nin `maphash`'i |
| `size` | `(size h)` | `HashTable<K,V>→int` | CL'nin `hash-table-size`'ı. Bu tabloda dolu girdilerin sayısıdır (`count`'a eşit) |

**`Hash`'i gerçekleştiren her tür anahtar olabilir**; `defstruct`/`defenum` türleri dahil.
`get`/`set`/`remove` `(where (Hash K))` taşır; bu yüzden onu gerçekleştirmeyen bir türle anahtarlanan
bir tablo **tür hatasıdır** (`f64`, `NaN` yüzünden `Hash`'e sahip değildir).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; returns a non-negative value that fits in a fixnum
```

Gerçekleştirilenler: `int` ve altı sabit genişlikli tamsayı, `bool`, `char`, `string` ve `symbol`
(kayan noktalı sayılar için değil). Kendi türleriniz için sonucu `*sxhash-mask*` (2^30-1) ile
`logand`'leyerek negatif olmayan tutun. Bir string'in karmasını almak için, `string` gerçekleştirmesinin
kullandığı `(sxhash-string s)`'i (32 bit FNV-1a) çağırabilirsiniz.

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

İki anahtarın aynı olup olmadığına nesne kimliği değil, **anahtar türünün kendisi** (`sxhash` ve `Hash`'in
üst trait'i `Eq`'ten `equals`) karar verir. Bu yüzden yukarıdaki gibi "farklı bir değer ama eşit" bir
anahtarla arama yapabilirsiniz.

`sxhash`'in çakışması sorun değildir (`Hash` sözleşmesi yalnızca tek yönlüdür: eşit değerlerin karması
aynı olmalıdır). Çakışan anahtarlar `equals` ile ayırt edilir.

## 5. `Array<T>` (çok boyutlu diziler)

Standart kütüphanedeki bir `defstruct`. Yerleşik bir tür değildir; bu yüzden bir `defstruct` ile
yapabileceğiniz her şey onunla da yapılabilir.
Bir değer `#2A((1 2) (3 4))` olarak yazılabilir ([Sözdizimi
Başvurusu](../syntax.md#1-sözcüksel-öğeler)).

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | CL'nin `make-array`'i. `dims` kopyalanır. `init`, her hücrenin başlangıç değeridir (CL'nin `:initial-element`'i; bu dilde "bağlanmamış hücre" yoktur, bu yüzden zorunludur). `:fill-pointer` yalnızca tek boyut için |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | CL'nin `aref` / `(setf (aref …))`'i. Bir indeks aralık dışındaysa panic olur |
| `aref` | `(aref a i j …)` | — | Çıplak indekslerle CL yazımı. Yukarıdaki `get`/`set`'e açılır. `(setf (aref a i j) v)` de çalışır |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | CL'nin `row-major-aref`'i. Düz bir indeks |
| `rank` | `(rank a)` | `Array<T>→int` | CL'nin `array-rank`'ı |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | CL'nin `array-dimension`'ı |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | CL'nin `array-dimensions`'ı. CL'nin yeni bir liste döndürmesi gibi bir **kopya** döndürür |
| `total-size` | `(total-size a)` | `Array<T>→int` | CL'nin `array-total-size`'ı (ayrılan hücre sayısı; doldurma işaretçisiyle ilgisi yok) |
| `len` | `(len a)` | `Array<T>→int` | CL'nin dizilerdeki `length`'i. Varsa doldurma işaretçisi, yoksa `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | CL'nin `array-in-bounds-p`'si. İndeks **sayısı** yanlış olsa bile false döner (hata değil) |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | CL'nin `array-row-major-index`'i |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | CL'nin `adjust-array`'i. Rank değişemez. Aralıkta kalan elemanlar indekslerinde korunur ve yeni hücreler `init` alır. CL'den farklı olarak diziyi döndürmez (bu dildeki her dizi ayarlanabilirdir; bu yüzden döndürülecek ikinci bir dizi yoktur) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CL'nin `vector-push-extend`'i. Doldurma işaretçisi olmadan panic olur |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CL'nin `vector-pop`'u. Boşsa `none` |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Doldurma işaretçisi (yoksa `none`). `(setf a::fill-pointer …)` ile yazılabilir |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Satır öncelikli sırayla bir yineleyici. Varsa doldurma işaretçisinde durur |

- **İndeksler bir `Vector<int>`'tir.** Bir metot "sonda aynı türden bağımsız değişkenin istenen sayıda
  tekrarını" bildiremez ve `aref` şekeri bu boşluğu kapatır.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **yoktur**. Alıcının statik türü bu soruları zaten yanıtlar.
- `Array::new`, `defstruct`'ın ürettiği alan sıralı yapıcıdır ve dizi oluşturmak için tasarlanmamıştır.
  `Array::make` kullanın.
- **Diziler CL'nin dizi sözdizimiyle yazdırılır.** Rank 1 `#(1 2 3)`'tür; diğer rank'lar `#nA` ve onu
  izleyen o kadar düzey parantezdir (`#2A((1 2 3) (4 5 6))`); rank 0 `#0A5`'tir. Varsa doldurma
  işaretçisinde yazdırma durur. `*print-array*`'i ([Yazdırma](printing.md#6-ne-kadarının-yazdırılacağını-denetleme))
  false yapmak yalnızca biçimi, `#<array 2x3>`'ü yazdırır. Yalnızca elemanları `print-object`'siz bir
  `defstruct` olan bir dizi yerleşik `#<array<...> ...>` biçiminde yazdırılır (hata değildir).

## 6. `BitVector` (bit vektörleri)

Sabit uzunluklu bir bit dizisi. Standart kütüphanedeki bir `defstruct`.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Uzunluk `n`, tüm bitler 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Aralık dışında panic olur |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | CL yazımları. `(setf (bit v i) b)` de çalışır. CL'nin `sbit`'i `bit`'ten yalnızca basit bir bit vektörü istemesiyle ayrılır, ancak bu dilde yalnızca tek tür bit vektörü vardır |
| `len` | `(len v)` | `BitVector→int` | Bit sayısı |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Yeni bir bit vektörü döndürür. Uzunluklar farklıysa panic olur. CL'deki gibi üçüncü bir bağımsız değişken (sonucun hedefi) yoktur |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Tümleyen |

`bit-vector-p` yoktur (statik tür bunu yanıtlar).
