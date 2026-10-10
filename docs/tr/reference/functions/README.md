<!-- translated-from: docs/ja/reference/functions/README.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# Yerleşik Fonksiyonlar

Yerleşik fonksiyonların, metotların ve standart kütüphanenin listesi. Sözdizimi (özel formlar ve
şeylerin nasıl tanımlanacağı) için [Sözdizimi Başvurusu](../syntax.md) belgesine, türlerin listesi için
[Türler](../types.md) belgesine bakın.

## Çağrı biçimleri

Üç çağrı biçimi vardır.

- Serbest fonksiyonlar: `(name args...)`
- Örnek metotları: `(name receiver args...)` (ilk bağımsız değişkenin statik türünden çözümlenir)
- Statik metotlar (ilişkili fonksiyonlar): `(Type::name args...)`

Her türün aynı adlı kendi metodu olabilir. `(+ a b)`, `a`'nın türünün `+`'sını çağırır.

## Tabloları okuma

Her bölümdeki tabloların "ad, biçim, tür, açıklama" sütunları vardır. Tür sütunu
`(bağımsız-değişken-türü,...)→dönüş-türü` olarak yazılır.

- `T`, `A` ya da `B` gibi tek bir büyük harf bir tür değişkenidir.
- `where Eq A` gibi bir not, tür değişkeninin sağlaması gereken bir trait sınırıdır.
- `Iter<A>`, "`Item`'ı `A` olan herhangi bir `Iter` gerçekleştirmesi" demektir.
- `&optional` / `&key` ile işaretlenmiş bağımsız değişkenler atlanabilir.

## Bölümler

| Dosya | İçerik |
|---|---|
| [numbers.md](numbers.md) | Tamsayılar, kayan noktalı sayılar, rasyonel sayılar, karmaşık sayılar, mantıksal değerler, bit işlemleri, rastgele sayılar |
| [sequences.md](sequences.md) | `cons-cell` çifti, S-ifade verisi `Sexpr`, semboller, dizi fonksiyonları, tembel yineleyiciler `lazy`, yüksek dereceli fonksiyonlar |
| [collections.md](collections.md) | String'ler, karakterler, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, hata türleri ve `Error` trait'i |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, aritmetik trait'ler |
| [printing.md](printing.md) | `print`/`println`/`format`, pretty printer, `print-object`, yazıcı denetim değişkenleri |
| [format.md](format.md) | Biçim yönergeleri |
| [streams-files.md](streams-files.md) | Akışlar, dosya işlemleri, yol adları, readtable |
| [concurrency.md](concurrency.md) | Task'ler, kanallar, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix domain soketleri, UDP |
| [system.md](system.md) | Zaman, çalışma zamanı ortamı, gerçekleştirim araçları, `read`/`eval`, docstring'ler, makroyla ilgili fonksiyonlar |
