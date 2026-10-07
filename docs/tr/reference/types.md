<!-- translated-from: docs/ja/reference/types.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Türler

typelisp'in sahip olduğu türler ve her türün gerçekleştirdiği standart trait'ler. Türlerin nasıl yazılacağı
[Sözdizimi Başvurusu 2. bölüm](syntax.md#2-türlerin-yazımı)'dedir; her türün fonksiyonları ve metotları
[Yerleşik Fonksiyonlar](functions/README.md) belgesindedir.

## 1. İlkel türler

| Tür | İçerik | Ayrıntılar |
|---|---|---|
| `int` | Keyfi duyarlıklı bir tamsayı. 63 bite sığdığı sürece anlık değer olarak tutulur ve bunun ötesinde otomatik olarak bignum olur. Türsüz tamsayı sabitlerinin varsayılan türü | [Sayılar 3. bölüm](functions/numbers.md#3-keyfi-duyarlıklı-tamsayılar-int) |
| `i8` `i16` `i32` | İşaretli sabit genişlikli tamsayılar | [Sayılar 1. bölüm](functions/numbers.md#1-sabit-genişlikli-tamsayılar) |
| `u8` `u16` `u32` | İşaretsiz sabit genişlikli tamsayılar | Yukarıdakiyle aynı |
| `f32` `f64` | IEEE-754 kayan noktalı sayılar. Ondalıklı sabitler varsayılan olarak `f64`'tür | [Sayılar 4. bölüm](functions/numbers.md#4-kayan-noktalı-sayılar-f64--f32) |
| `ratio` | En sade biçimde bir rasyonel sayı | [Sayılar 5. bölüm](functions/numbers.md#5-rasyonel-sayılar-ratio) |
| `bool` | `true` / `false` | [Sayılar 7. bölüm](functions/numbers.md#7-mantıksal-değerler) |
| `char` | Bir Unicode skaler değeri | [Karakterler](functions/collections.md#2-karakterler-char) |
| `string` | Değiştirilemez bir string | [String'ler](functions/collections.md#1-stringler-string) |
| `symbol` | Bir sembol. Anahtar sözcükler (`:name`) de bu türdendir | [Semboller](functions/sequences.md#3-semboller) |
| `()` | Unit türü. Değeri de `()`'dir | |
| `!` | Never türü. `panic` gibi dönmeyen ifadelerin türü. Herhangi bir türün beklendiği yere konabilir | |
| `ptr` `c-long` `c-ulong` | Yalnızca C'ye ve C'den değer geçirmek için kullanılan sözcükler. Yalnızca `unsafe` içinde değer olabilirler ve görünebilecekleri yerler sınırlıdır | [Sayılar 2. bölüm](functions/numbers.md#2-c-sınırındaki-ham-sözcükler-ptr--c-long--c-ulong) |
| `random-state` | Bir rastgele sayı üretecinin durumu | [Sayılar 12. bölüm](functions/numbers.md#12-rastgele-sayılar) |

64 bitlik bir tamsayı türü yoktur. Genişliğin önemli olmadığı tamsayılar için `int` kullanın.

## 2. Yerleşik jenerik türler

| Tür | İçerik | Ayrıntılar |
|---|---|---|
| `Option<T>` | Var olan ya da olmayan bir değer. `some` / `none` | [Option ve Result](functions/option-result.md) |
| `Result<T,E>` | Başarı ya da başarısızlık. `ok` / `err` | Yukarıdakiyle aynı |
| `Vector<T>` | Büyüyebilen bir dizi | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Bir karma tablo. Anahtar türü `Hash`'i gerçekleştirmelidir | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Bir task'in tanıtıcısı | [Task'ler](functions/concurrency.md#1-taskt--task-tanıtıcıları) |
| `Thread<T>` | Ayrılmış bir OS thread'inde çalışan bir task'in tanıtıcısı | [Thread](functions/concurrency.md#7-threadt--ayrılmış-os-threadleri) |
| `Chan<T>` | Bir kanal | [Kanallar](functions/concurrency.md#2-chant--kanallar) |

Fonksiyon türleri `(fn (bağımsız-değişken-türleri...) dönüş-türü)` olarak, trait nesneleri ise
`:dyn Trait` olarak yazılır ([Sözdizimi Başvurusu 2. bölüm](syntax.md#2-türlerin-yazımı)).

## 3. S-ifade verisi

| Tür | İçerik | Ayrıntılar |
|---|---|---|
| `Sexpr` | Boş olmayan bir S-ifade. 16 varyant: `int`, `i8`'den `u32`'ye, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path` | [S-ifade verisi](functions/sequences.md#2-s-ifade-verisi-sexpr) |
| `Option<Sexpr>` | Genel olarak S-ifade verisi. Boş liste `()`, `none`'dır | Yukarıdakiyle aynı |

## 4. Standart kütüphanedeki türler

Standart kütüphanenin (prelude) `defstruct` / `defenum` ile tanımladığı türler. Kendi yazdığınız türlerle
aynı şekilde ele alınırlar ve bir `defstruct` ile yapabileceğiniz her şey onlarla da yapılabilir.

| Tür | İçerik | Ayrıntılar |
|---|---|---|
| `cons-cell<A,B>` | Bir çift. `cons`/`car`/`cdr` | [Çiftler](functions/sequences.md#1-çiftler-cons-cellab) |
| `complex` | Bir karmaşık sayı (`f64` bileşenli) | [Sayılar 6. bölüm](functions/numbers.md#6-karmaşık-sayılar-complex) |
| `Array<T>` | Çok boyutlu bir dizi | [Array](functions/collections.md#5-arrayt-çok-boyutlu-diziler) |
| `BitVector` | Sabit uzunluklu bir bit dizisi | [BitVector](functions/collections.md#6-bitvector-bit-vektörleri) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Her koleksiyonun `iter`'inin döndürdüğü yineleyiciler | [Iter](functions/traits.md#1-iter-traiti-ve-yineleme) |
| `WaitGroup` | N şeyin bitmesini bekleme | [WaitGroup](functions/concurrency.md#4-waitgroup--n-işin-bitmesini-bekleme) |
| `Mutex<T>` | Paylaşılan veri için karşılıklı dışlama | [Mutex](functions/concurrency.md#6-mutext--paylaşılan-veri-için-karşılıklı-dışlama) |
| `pathname` | Parçalara ayrılmış bir dosya adı | [Yol adları](functions/streams-files.md#9-yol-adları-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Akışlar | [Akışlar](functions/streams-files.md#3-somut-akış-türleri) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Bileşik akışlar | [Bileşik akışlar](functions/streams-files.md#4-bileşik-akışlar) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Ağ | [Ağ](functions/network.md#1-türler) |
| `ReadOutcome` | `read-sexpr`'in sonucu. `datum` / `eof` | [Akışlar](functions/streams-files.md#6-jenerik-fonksiyonlar-ve-dosya-işlemleri) |
| `universal-time` `internal-time` `decoded-time` | Zaman | [Zaman](functions/system.md#1-zaman) |
| `heap-info` | Heap'in geçerli durumu | [Gerçekleştirim araçları](functions/system.md#51-heap-info-alanları) |

## 5. Hata türleri

`Error` bir tür değil, bir trait'tir ve aşağıdaki türler onu gerçekleştirir. Her çeşit hatayı ele almak
için `:dyn Error` yazın.

| Tür | Şunun ürettiği |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Dosya ve akış işlemleri |
| `NetError` | Ağ işlemleri |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Ayrıntılar [Hata Türleri ve Error Trait'i](functions/option-result.md#3-hata-türleri-ve-error-traiti)
bölümündedir.

## 6. Standart trait gerçekleştirmeleri

Hangi türlerin hangi trait'leri gerçekleştirdiği. Her trait'in metotları
[Standart Trait'ler](functions/traits.md) belgesinde ve en sağdaki sütunda listelenen bölümlerdedir.

### 6.1 Karşılaştırma, karma ve yazdırma

| Trait | Gerçekleştiren türler | Ayrıntılar |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-karşılaştırma) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Yukarıdakiyle aynı |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` ve tüm yerleşik hata türleri | [print-object](functions/printing.md#5-print-object-türe-göre-yazdırılan-gösterim) |

`cons-cell<A,B>`'nin `Eq`/`Ord`'u, eleman türleri `Eq`/`Ord`'u gerçekleştirdiğinde kullanılabilir.

### 6.2 Aritmetik

| Trait | Gerçekleştiren türler |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Ayrıntılar [Aritmetik trait'ler](functions/traits.md#3-aritmetik-traitler-add--sub--mul--div--rem--bits--number)
bölümündedir.

### 6.3 Yineleme

| Trait | Gerçekleştiren türler |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Akışlar

| Tür | Gerçekleştirilen trait'ler |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

Her akış `Stream`'i gerçekleştirir; girdi akışları ayrıca `InputStream`'i, çıktı akışları ise
`OutputStream`'i gerçekleştirir. `socket-listener` ve `udp-socket` yalnızca `Stream`'i gerçekleştirir
(`close` / `open-stream-p`). Ayrıntılar [Akışlar](functions/streams-files.md#1-trait-hiyerarşisi)
bölümündedir.

### 6.5 Diğerleri

| Trait | Gerçekleştiren türler | Ayrıntılar |
|---|---|---|
| `Error` | 5. bölümdeki tüm hata türleri | [Hata türleri](functions/option-result.md#3-hata-türleri-ve-error-traiti) |
| `Pathish` | `string` `pathname` | [Yol adları](functions/streams-files.md#91-yol-adı-belirleyici-traiti-pathish) |
