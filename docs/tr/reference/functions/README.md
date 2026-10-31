<!-- translated-from: docs/ja/reference/functions/README.md @ 2d61c22ad717f8ad6dfd178f397927e87e1d8902 -->
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

## `use` gerektiren adlar

Standart kütüphanenin adlarından bunlar, hiçbir şey yazmadan her modülde kullanılabilir
([sözdizimi başvurusu 3.10](../syntax.md#310-module--use--ad-alanları)):

- ilkel tipler, `Option`/`Result`/`Vector`, yineleme, S-ifadeleri ve listeler, dizi fonksiyonları,
  denetim makroları, karşılaştırma ve aritmetik trait'ler
- Common Lisp'in `COMMON-LISP` paketinde de bulunan her ad (`length`, `format`, `open` vb.)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, akış trait'leri `Stream`,
  `InputStream` ve `OutputStream` ile bir `HashTable`'ın anahtarlarının uyguladığı `Hash`
- bunların fonksiyon ve değişkenlerinin andığı tipler

Başka her ad `(use std::net::tcp-connect)` gibi bir `use` ile içeri alınır ya da yoluyla,
`std::net::tcp-connect` diye yazılır. Tablo bu adları ve her birinin modülünü verir.

| Modül | Adlar |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`, `digit-weight`, `name->char` |
| `std::collections` | `Deque`, `HashSet`, `Scope`, `SortedTable`, `hashtable-iter` |
| `std::env` | `command-line-args`, `getenv`, `home-directory` |
| `std::error` | `WrappedError`, `as-dyn-error`, `assert-eq`, `describe-error`, `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`, `direction-input`, `direction-output`, `directory-p`, `open-binary`, `open-binary-input`, `open-binary-output`, `open-file`, `open-input`, `open-output`, `read-file-lines`, `read-file-string`, `write-file-string` |
| `std::hash` | `*sxhash-mask*`, `sxhash-string` |
| `std::io` | `ByteInput`, `ByteOutput`, `PeekInput`, `binary-file-stream`, `copy-stream`, `make-peek-stream`, `peek-stream`, `read-lines`, `write-lines` |
| `std::net` | `NetError`, `byte-stream-of`, `char-stream-of`, `datagram`, `socket-byte-stream`, `socket-listener`, `socket-stream`, `tcp-connect`, `tcp-listen`, `tls-connect`, `tls-listen`, `udp-bind`, `udp-socket`, `unix-connect`, `unix-listen`, `with-connection` |
| `std::num` | `ParseFloatError`, `ParseIntError`, `atan2`, `parse-float`, `parse-int` |
| `std::path` | `Pathish`, `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`, `read-from-string-preserving-whitespace`, `read-sexpr`, `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`, `right-trim`, `string->utf8`, `trim`, `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`, `Context`, `Mutex`, `WaitGroup`, `after`, `with-lock` |
| `std::thread` | `Task`, `Thread`, `yield` |
| `std::time` | `internal-time-seconds`, `timezone-daylight-p`, `timezone-offset-seconds` |

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
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, aritmetik trait'ler, `Push` |
| [printing.md](printing.md) | `print`/`println`/`format`, pretty printer, `print-object`, yazıcı denetim değişkenleri |
| [format.md](format.md) | Biçim yönergeleri |
| [streams-files.md](streams-files.md) | Akışlar, dosya işlemleri, yol adları, readtable |
| [concurrency.md](concurrency.md) | Task'ler, kanallar, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix domain soketleri, UDP |
| [system.md](system.md) | Zaman, çalışma zamanı ortamı, gerçekleştirim araçları, `read`/`eval`, docstring'ler, makroyla ilgili fonksiyonlar |
