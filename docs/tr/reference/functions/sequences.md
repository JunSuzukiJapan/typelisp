<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Çiftler, S-İfadeler ve Diziler

Jenerik çift `cons-cell`, S-ifade verisi `Sexpr`, semboller, `Iter` üzerine yazılmış dizi fonksiyonları ve
yüksek dereceli fonksiyonlar.

## 1. Çiftler `cons-cell<A,B>`

`cons`/`car`/`cdr`, **jenerik çift türü `cons-cell<A,B>`'nin** (standart kütüphanedeki bir `defstruct`)
yapıcısı ve alan erişimcileridir. Alanlar `variable::car`/`variable::cdr` olarak
([Sözdizimi Başvurusu](../syntax.md#36-defstruct--structlar-kullanıcı-tanımlı-türler)'ndaki `defstruct`
erişimci sözdizimi) ya da `(car variable)`/`(cdr variable)` olarak okunabilir. Değiştirmek için
`(setf variable::car v)`/`(setf variable::cdr v)` kullanın.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Bir çift yapar |
| `car` | `(car p)` | `cons-cell<A,B>→A` | İlk eleman |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Geri kalan |

`cons-cell` ayrıca bir demet sözdiziminin yerine de geçer. Çoklu değerler döndüren CL fonksiyonları
(`floor`'un bölümü ve kalanı, `read-from-string`'in değeri ve konumu vb.) bu dilde bir `cons-cell`
döndürür.

## 2. S-ifade verisi `Sexpr`

`read`'in döndürdüğü `Sexpr` veri türünün 19 varyantı vardır:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` ve `array`, `#(..)` ve `#nA(..)` olarak yazılmış verilerdir ([Sözdizimi
Başvurusu](../syntax.md#1-sözcüksel-öğeler)); sırasıyla bir `Vector<Option<Sexpr>>` ve bir
`Array<Option<Sexpr>>` tutarlar: `(vector v)`'nin bağladığı `v` üzerinde `len`, `get` ve diğerleri
`tuple`, `#{..}` ile yazılmış veridir ve `(tuple v)`'nin bağladığı `v`, öğelerden oluşan yeni bir
`Vector<Option<Sexpr>>`'dir (her uzunluktaki demet tek bir türle alınsın diye).
doğrudan çalışır.
S-ifade hücreleri, 1. bölümün genel `cons`/`car`/`cdr`'siyle değil, `sexpr-*` fonksiyonlarıyla ele alınır.
Esas olarak `defmacro` gövdelerinde formları oluşturmak ve ayrıştırmak için kullanılırlar.

**S-ifade verisinin türü `Option<Sexpr>`'dir.** Boş liste `Sexpr`'in bir varyantı değil, `Option`'ın
`none`'dır ve `Sexpr`'in kendisi "boş olmayan bir S-ifade" demektir. Bu yüzden `sexpr-*` fonksiyonları
`Option<Sexpr>` alır ve döndürür.

- `()`, bir `Option<Sexpr>` beklenen yerde boş listedir (`(Option::none)` olarak da yazılabilir)
- `Sexpr`, bir `Option<Sexpr>` beklenen yerde örtük olarak genişler (çalışma zamanı dönüşümü olmadan).
  Ters yön, yani bir `Option<Sexpr>`'i `Sexpr` olarak kullanmak, "bu boş liste değildir" iddiasında
  bulunur; bu yüzden `match` ya da `unwrap` ile açıkça belirtilmelidir
- `match` içinde `Sexpr`'in 19 varyantı ve `none` **aynı kol listesinde düz olarak** yazılabilir
  ([Sözdizimi Başvurusu](../syntax.md#43-match--örüntü-eşleme))

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Bir `Sexpr` hücresi yapar |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | İlk eleman. **Boş liste için boş liste** (CL'deki gibi). `Cons` olmayan bir atomda panic olur |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Geri kalan. **Boş liste için boş liste** (CL'deki gibi). `Cons` olmayan bir atomda panic olur |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Bir `Cons` olup olmadığı |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Boş liste olup olmadığı |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | `Cons` olmayıp olmadığı |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Bir `Sym` (sembol) olup olmadığı |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | `int` varyantının içeriği (fixnum ya da bignum). Başka bir türde panic olur |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | O genişlikteki varyantın içeriği. Başka bir türde panic olur |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Kayan noktalı varyantların içeriği. Başka bir türde panic olur |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Bir `Char`'ın içeriği. Başka bir türde panic olur |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Bir `Bool`'un içeriği. Başka bir türde panic olur |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Bir `Str`'nin içeriği. Başka bir türde panic olur |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Bir `Sym`'in adı. Başka bir türde panic olur |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Kimlik karşılaştırması (`Cons`/`Str` nesne kimliğini, geri kalanlar değerleri karşılaştırır) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Yapısal eşitlik (`Cons` özyinelemeli, `Str` içeriğe göre) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | `equal` gibi, ayrıca büyük/küçük harf duyarsız karşılaştırma ve türler arası sayı karşılaştırması |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | İki `Sexpr` listesini birleştirir (yıkıcı olmayan). `,@` buna açılır |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Bir `Sexpr` listesinin her elemanına `f` uygulanmış yeni bir `Sexpr` listesi (4. bölümün `map`'i `Iter` içindir ve bir `Sexpr` listesinde gezemez) |

Tür başına bir tane olmak üzere dokuz sayısal erişimci vardır; çünkü bir `Sexpr`, "bir değerin türünün
başka hiçbir yerde yazılmadığı tek yer"dir. Bir `Sexpr` içine konan bir `u8`, `u8` varyantı olarak girer ve
yalnızca `(sexpr-u8 s)` ile çıkar. Onu `(sexpr-int s)`'e geçirmek panic olur; yanıtı asla sessizce
genişletmez. Okunan verideki tamsayılar (`'(1 2 3)`, makro bağımsız değişkenleri) `int` varyantındandır ve
`(sexpr-int s)` ile okunur.

`Sexpr` listelerinin `rplaca`/`nconc` gibi yıkıcı işlemleri yoktur. Bir `Sexpr` hücresi oluşturulduktan
sonra değiştirilemez.

## 3. Semboller

`symbol`, sembollerin kendilerinin türüdür. Bir `Sexpr` gereken yerde örtük olarak dönüşür, ancak ters
yönde otomatik olarak dönüşmez.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Sembolün adını çıkarır |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Bir string'den sembol yapar (intern eder) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Bir anahtar sözcük (`:name`) olup olmadığı. İki nokta adın bir parçasıdır; bu yüzden sınama ilk karaktere bakar ([Sözdizimi Başvurusu](../syntax.md#1-sözcüksel-öğeler)) |

`gensym` için [Makrolar](system.md#8-makrolar) bölümüne bakın.

## 4. `Iter` üzerinde dizi fonksiyonları

Dizi fonksiyonları **`Iter` trait'i üzerinde jenerik fonksiyonlardır**. Bir koleksiyondan `(iter coll)` ile
bir yineleyici alın ve geçirin (`Vector<T>` / `HashTable<K,V>` / `Array<T>` bunu destekler; bir `Sexpr`
listesi `Iter`'i gerçekleştirmez; bu yüzden bu fonksiyonlar ona uygulanmaz). **Ortaya çıkan koleksiyon
yeni bir `Vector` olarak döndürülür.** Tablolardaki `Iter<A>`, "`Item`'ı `A` olan herhangi bir `Iter`
gerçekleştirmesi" demektir. Döndürülen `Vector`'de yeniden gezmek için `(iter result)` geçirin.

Bir yüklem alan fonksiyonlar (CL'nin `-if` ailesine karşılık gelir):

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Eşleme |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Yalnızca yüklemi sağlayan elemanlar |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Yüklemi sağlayan elemanları kaldırır |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Yüklemi sağlayan ilk eleman |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | Yüklemi sağlayan ilk konum |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Kaçının yüklemi sağladığı |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Her elemanın yüklemi sağlayıp sağlamadığı |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Herhangi bir elemanın yüklemi sağlayıp sağlamadığı (CL'nin `some`'una karşılık gelir; `Some` yapıcısıyla çakışmayan bir ad) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Soldan katlama |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Sağdan katlama |

İndeksleme, uzunluk ve dilimleme:

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Eleman sayısı |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Yineleyicileri birleştirir. Üç ya da daha fazlası verilebilir |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | CL'nin `concatenate`'i. Sonuç türü **tırnaklı bir sembol sabiti** olarak yazılır (CL çalışma zamanı tür belirleyicisi kullanır). `'vector` bir ya da daha fazlasını, `'string` sıfır ya da daha fazlasını alır (sıfır için `""`). `Sexpr` listeleri kapsanmaz (`sexpr-append` kullanın) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Tersine çevirme (yıkıcı olmayan) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | `n`'inci eleman (aralık dışında `None`) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | Bağımsız değişkenleri ters sırada `nth` |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | İlk `n` eleman |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` uzunluğa kırpılır) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | Son **eleman** (CL'deki gibi "son hücre" değil) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Sonuncu hariç hepsi |

`Eq` / `Ord` sınırı gerektiren fonksiyonlar (bir yüklem yerine bir trait üzerinden karşılaştırırlar;
[Standart Trait'ler](traits.md#2-eq--ord-karşılaştırma)):

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | `x`'e eşit bir eleman olup olmadığı (CL'den farklı olarak listenin geri kalanı değil, bir `bool`) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | `x`'e eşit ilk eleman |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | `x`'e eşit ilk konum |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Kaç elemanın `x`'e eşit olduğu |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL'nin `(sort sequence predicate)`'i. Kararlı, yıkıcı olmayan bir sıralama. `cmp`, "birinci bağımsız değişken kesin olarak ikinciden önce gelir" olduğunda `true`'dur |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | `car`'ı `k`'ye eşit olan ilk çift. Değeri `(cdr p)` ile çıkarın |

Bunlar ve 5. bölümdeki fonksiyonların çoğu, CL'nin `:key` / `:test` / `:test-not` / `:start` / `:end` /
`:from-end` / `:count` anahtar sözcüklü bağımsız değişkenlerini de alır (6. bölüm).

## 5. CL'nin dizi fonksiyonlarının geri kalanı

Hepsi 4. bölümdeki gibi `Iter` üzerinde jenerik fonksiyonlardır. Ortaya çıkan koleksiyonlar yeni
`Vector`'ler olarak döndürülür.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CL'nin adlandırılmış indeksleri |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | İlki hariç hepsi (yeni bir `Vector`, paylaşılan bir kuyruk değil) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Bir yineleyiciyi bir `Vector`'e dönüştürür (CL'nin `copy-seq`/`copy-list`'i) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | Tersine çevrilmiş `a`, ardından `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x`'in `n` kopyası (CL'nin `make-list`/`make-sequence`'i). `Vector::new` gibi tür bağımsız değişkeni beklenen türden gelir; bu yüzden çıplak bir `let` `the` gerektirir |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `member` gibi bir **`bool`** (bir yineleyicinin döndürülecek bir kuyruğu yoktur) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `any`/`every`'nin olumsuzlamaları |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Olumlu sürümlerle aynı türler | Yüklemi olumsuzlanmış sürümler |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Değere göre kaldırır |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Yinelenenleri kaldırır. CL'deki gibi **son görünüm korunur** (`:from-end true` ilkini korur) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Değere / yükleme göre değiştirir |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | `Iter<cons-cell<K,V>>` üzerinde | `assoc`'un yüklemli ve değer tarafı sürümleri |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Başa bir çift ekler |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | İki diziyi eşleştirir. Kısa olanda durur |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | CL'nin birkaç dizi üzerinde `mapcar`'ı. Kısa olanda durur |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Yan etkiler için eşleme |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Eşler ve birleştirir |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Ardışık **kuyruklar** üzerinde eşler |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Yan etkiler için kuyruklar üzerinde eşler (`mapc`'nin `maplist` karşılığı) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Kuyruklar üzerinde eşler ve birleştirir (`mapcan`'ın `maplist` karşılığı) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | `sub`'ın ilk göründüğü konum. Alıcı bir `string` ise `string` metodu seçilir ([String'ler](collections.md#1-stringler-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | İlk farklılaşma konumu. Eşitlerse `none` |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Birleştirme. CL sıralı girdiler ister; bu, birleştirmeyi sıralar |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Orada değilse `x`'i **başa** ekler |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Küme işlemleri. CL sırayı belirtmez; burada kararlıdır, **ilk görünüm sırasına göre** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | İçerme |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Bir sonek olup olmadığı / sonekten önceki kısım. CL **paylaşılan yapıyı** sorar, ancak paylaşılacak yapı yoktur; bu yüzden bu, bir soneki **değer olarak** sorar |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Eleman eleman eşitlik. `Vector<T>`'nin kendisi `Eq`'i gerçekleştirmez |
| `caar`…`cddddr` | `(cadr p)` | iç içe çiftler üzerinde | CL'nin 28 fonksiyonu. **Listelerde değil, çiftlerde** gezerler: `cadr`, bir `cons-cell<A,cons-cell<B,C>>` alır |

CL'de olup bu dilde olmayanlar: `list*` (kuyruğu değiştirilmiş düzensiz liste kavramı yoktur),
`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (hiçbir tür, keyfi derinlikte heterojen bir ağaçta
gezinmeyi betimleyemez; bir `Sexpr` ağacı için `equal`, `tree-equal`'a karşılık gelir), özellik listesi
ailesi `getf`/`get-properties`/`symbol-plist`/`remprop` (anahtarlar ile değerleri değiştiren türsüz bir
liste gösterimi yoktur; `assoc` (ilişkilendirme listeleri) ya da `HashTable` aynı rolü doldurur) ve
`Vector<T>` ile `Sexpr` listeleri arasında dönüştüren fonksiyonlar (bir `Sexpr` listesinin elemanlarının
her biri farklı türde olabilir; bu yüzden tek bir `T` eleman türüyle yazılamazlar).

## 6. Anahtar sözcüklü bağımsız değişkenler

4. ve 5. bölümlerdeki fonksiyonlar CL'nin dizi anahtar sözcüklerini alır: `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count`. Hepsi **isteğe bağlıdır**.

| Anahtar sözcük | Tür | Anlamı |
|---|---|---|
| `:key` | `(fn (A) A)` | Karşılaştırmadan ya da sınamadan önce her elemana uygulanan bir izdüşüm |
| `:test` | `(fn (A A) bool)` | `Eq` sınırındaki `equals` yerine kullanılan bir eşitlik sınaması. İlk bağımsız değişken **aranan öğe**, ikincisi (`:key`'den sonraki) elemandır; CL ile aynı sırada |
| `:test-not` | `(fn (A A) bool)` | `:test`'in olumsuzlaması |
| `:start` `:end` | `int` | Taranacak `[start, end)` penceresi. İndeksler tüm diziye göredir |
| `:from-end` | `bool` | Bir arama **son** eşleşmeyle yanıt verir. `:count` ile birleştirildiğinde etkilenen elemanlar sondan alınır |
| `:count` | `int` | `remove` / `substitute` ailelerinin etkilediği en fazla eleman sayısı |

Hangi fonksiyonun hangisini aldığı CL'yi izler:

| Fonksiyon | Aldığı anahtar sözcükler |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Yukarıdakilerin hepsi (`:count` dahil) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (`assoc`'un `:key`'i `car`'a, `rassoc`'unki `cdr`'ye uygulanır) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; removes only one, from the end
(position 3 (iter v) :start 1)                          ; the index is relative to the whole sequence
```

**CL'den farklar**:

1. **`:key`'in izdüşümü eleman türü içinde kalır** (`(fn (A) A)`). CL'deki gibi başka bir türe izdüşüm
   yapamaz: bağımsız değişken atlandığında ek bir tür değişkeni belirlenemezdi. Farklı bir türe
   izdüşüm gereken yerde bunun yerine `-if` ailesine bir lambda geçirin
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **Öğe tabanlı aramalarda `:key` yalnızca elemanlara uygulanır** (aranan öğeye değil). Bu, CL'nin
   `find`/`position`/`count`/`member`/`remove`/`substitute`'uyla aynı kuraldır. Küme işlemlerinde iki
   taraf da elemandır; bu yüzden ikisine de uygulanır.
3. **Yalnızca `search`'ün anahtar sözcükleri numaralandırılmak yerine adlandırılmıştır.** CL'de
   `:start1`/`:end1` **örüntü** için, `:start2`/`:end2` aranan dizi içindir. Bu dilde alıcı önce gelir;
   bu yüzden aynı numaralar tersi anlama gelirdi ve bunu sessizce yapardı. `:start`/`:end` alıcı için,
   `:sub-start`/`:sub-end` örüntü içindir; bu yüzden dalgın bir `:start1` "bilinmeyen anahtar sözcük"
   hatası verir. `mismatch` ve `replace` CL ile aynı bağımsız değişken sırasına sahiptir; bu yüzden
   CL'nin numaralarını korurlar.

## 7. Yıkıcı işlemler

`Vector<T>`'nin metotları. **Alıcıyı değiştirir ve alıcının kendisini döndürürler**; bu yüzden
`(nreverse v)`, `reverse` ile aynı şekilde yazılır ve `v`'nin kendisi de tersine çevrilir.

| Ad | Biçim | Açıklama |
|---|---|---|
| `nreverse` | `(nreverse v)` | Yerinde tersine çevirir |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | `remove` / `remove-if` / `filter` / `remove-duplicates`'in yerinde sürümleri |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | `substitute` ailesinin yerinde sürümleri |
| `nbutlast` | `(nbutlast v)` | Son elemanı atar |
| `fill` | `(fill v x)` | Her elemanı `x` yapar. Uzunluk değişmez |
| `replace` | `(replace v src)` | `src`'nin elemanlarıyla baştan itibaren üzerine yazar. `(min (len v) (len src))` eleman |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Yukarıdakiyle aynı sayı |
| `nconc` | `(nconc v w)` | `w`'nin elemanlarını `v`'ye ekler. CL'den farklı olarak **paylaşılan yapıyı yeniden yazmaz** (`w` etkilenmez) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | `v`'nin içeriğini `src` ile değiştirir (uzunluk da değişir) |
| `rplaca` `rplacd` | `(rplaca p x)` | Bir `cons-cell`'in `car`/`cdr`'sini yeniden yazar ve hücrenin kendisini döndürür |

Aldığı anahtar sözcükler:

| Yıkıcı sürüm | Aldığı anahtar sözcükler |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (alıcı CL'nin `sequence-1`'idir) |

`vector-push-extend`/`vector-pop` yalnızca `Vector<T>`'nin `push`/`pop`'udur. Bir `Vector<T>` her zaman
büyür; bu yüzden CL'nin "doldurma işaretçili bir vektör" ile "basit bir vektör" ayrımına karşılık gelen
bir şey yoktur.

## 8. Yüksek dereceli fonksiyonlar

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Bağımsız değişkenini döndürür |
| `const` | `(const x y)` | `(A,B)→A` | İlk bağımsız değişkeni döndürür |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Fonksiyon bileşimi `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | İki bağımsız değişkenli bir fonksiyonun bağımsız değişkenlerini değiştirir |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Bir yüklemin olumsuzlaması |

CL'nin `constantly`'si yoktur (yok sayılan bağımsız değişkenin türü yalnızca dönüş türünde görünür ve
belirlenemezdi). `(lambda ((x T)) A v)` yazın.
