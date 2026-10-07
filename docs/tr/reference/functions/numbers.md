<!-- translated-from: docs/ja/reference/functions/numbers.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Sayılar

Tamsayılar, kayan noktalı sayılar, rasyonel sayılar, karmaşık sayılar ve mantıksal değerler üzerindeki
işlemler ile sayıyla ilgili diğer fonksiyonlar. Çağrı biçimlerinin nasıl okunacağı için
[Yerleşik Fonksiyonlar](README.md) belgesine bakın.

## 1. Sabit genişlikli tamsayılar

Yedi tamsayı türü vardır: **`int`** (CL'nin `integer`'ı: keyfi duyarlık ve türsüz tamsayı sabitlerinin
varsayılan türü; 3. bölüm) ve sabit genişlikli `i8` `i16` `i32` `u8` `u16` `u32`. Bir işlemin hangisi
için çözümleneceği ilk bağımsız değişkenin türüyle belirlenir (birbirlerinden bağımsızdırlar, örtük
dönüşüm yoktur). **64 bitlik bir tamsayı türü yoktur.** Bir çalışma zamanı değeri, düşük bitleri etiket
olan tek bir sözcüktür; bu yüzden anlık bir tamsayı için yalnızca 63 bit kalır ve 64 bit iddia eden bir
tür üst biti bir yerde atmak zorunda kalırdı. `int`, bu 63 biti geçtiğinde bignum olur; bu yüzden genişlik
önemli değilse `int` kullanın. Aşağıdaki tablo altı sabit genişlikli tür içindir (`int` tablosu 3.
bölümdedir).

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | Dört aritmetik işlem. `/` sıfıra doğru keser ve sıfıra bölmede panic olur |
| `mod` | `(mod a b)` | `(T,T)→T` | Kalan (CL'nin `mod`'u, **taban bölme**: işaret böleni izler. `(mod -7 3)`→`2`). Sıfıra bölmede panic olur |
| `rem` | `(rem a b)` | `(T,T)→T` | Kalan (CL'nin `rem`'i, **kesen bölme**: işaret bölüneni izler. `(rem -7 3)`→`-1`). Sıfıra bölmede panic olur |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | CL'nin iki bağımsız değişkenli `floor`/`ceiling`/`round`/`truncate`'ine karşılık gelir (`(floor 7 2)`→bölüm 3, kalan 1). Çoklu değerler yerine bölümü ve kalanı bir `cons-cell` içinde döndürürler (`car`=bölüm, `cdr`=kalan). `round-div`, CL'nin yaptığı gibi yarımları çifte yuvarlar |
| `abs` | `(abs x)` | `T→T` | Mutlak değer |
| `signum` | `(signum x)` | `T→T` | İşaret (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | En büyük ortak bölen |
| `lcm` | `(lcm a b)` | `(T,T)→T` | En küçük ortak kat (biri 0 ise 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | Büyük olan / küçük olan (üç ya da daha fazla bağımsız değişken 8. bölümün değişken sayılı şekeriyle açılır) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Karşılaştırma |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Hepsi `=` ile aynı (aynı türden sayılar için fark yoktur) |
| `int->float` | `(int->float x)` | `T→f64` | `f64`'e genişleten dönüşüm |
| `int->int` | `(int->int x)` | `T→int` | `int`'e genişleten dönüşüm (her zaman tam). `(as int x)`'in yaptığı şey |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | `ratio`'ya genişleten dönüşüm (her zaman tam) |
| `int->char` | `(int->char x)` | `T→char` | Değeri bir Unicode skaler değeri olarak yorumlar. Geçersiz bir değerde panic olur |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Başarısızlıkta `None` döndüren bir `int->char` sürümü |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Genişlik dönüşümü. Sığmayan değerler kırpılır (Rust'ın `as`'ı gibi) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Aynı dönüşüm bir soru olarak. Değer o genişliğe sığmıyorsa `None` |

Bu dönüşümler, `(as Type x)`/`(try-as Type x)` özel formlarının
([Sözdizimi Başvurusu](../syntax.md#7-diğer-özel-formlar)) yaptığı şeylerdir. Bit işlemleri
(`logand`/`ash`/`ldb` vb.) ve yüklemler (`zerop`/`evenp` vb.) türler arasında aynı biçime sahiptir; bu
yüzden 11. ve 9. bölümlerde toplanmışlardır.

`i8` `i16` `u8` `u16` `u32` tam olarak bu bölümün tablosuna, `f32` ise tam olarak 4. bölümün `f64`
tablosuna sahiptir.

**Bir tür adı yalnızca genişliği ve işaretliliği demektir, başka bir şey değil.** `i32`, "32 biti işaretli
say" ve `u32`, "32 biti işaretsiz say" demektir. `(+ (the u8 200) (the u8 100))` `44`'tür,
`(+ 2147483647 1)` (`i32` olarak) `-2147483648`'dir ve `(lognot (the u32 0))` `4294967295`'tir. `f32` de
aynıdır: gerçek bir binary32. `(/ (the f32 1.0) (the f32 3.0))`, `f64` sonucu `0.3333333333333333`'ten
farklı bir değer olarak `0.33333334` yazdırır.

Türetilmiş CL kataloğu (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` ve 9. bölümün yüklemleri)
`int`/`i32`/`f64`/`ratio` için vardır. Başka bir genişlik için gerekiyorsa `(as int x)` /
`(as i32 x)` ile geçin (genişlik dönüşümleri her çift için vardır).

## 2. C sınırındaki ham sözcükler (`ptr` / `c-long` / `c-ulong`)

[`defffi`](../syntax.md#33-defffi--c-fonksiyonlarını-bildirme-ffi) ile bildirilen C fonksiyonlarına ve
onlardan değer geçirmek için kullanılan üç tür. `ptr` opak bir işaretçidir ve `c-long` / `c-ulong`
C'nin `long` / `unsigned long`'udur. Birini değer yapmak `(unsafe ...)` içinde olmayı gerektirir.

**Aritmetik yoktur.** 1. bölümün tablosunun hiçbiri uygulanmaz: ne `(+ p 1)` ne de `(< n m)` yazılabilir.
Bunlar C'ye verilecek sözcüklerdir, hesaplanacak türler değil; bu yüzden hesaplamak için genişlikli bir
türe geçin. `c-long` / `c-ulong`'un yalnızca dönüşümleri vardır:

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | 1. bölümdekiyle aynı genişlik dönüşümleri. Sığmayan değerler kırpılır |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Aynı dönüşüm bir soru olarak |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | Diğer ham sözcükten ve 1. bölümün tamsayı türlerinden giriş yolu |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Yukarıdakiyle aynı |
| `int->int` | `(int->int x)` | `T→int` | **Her zaman tam**. Bir `i32`'ye sığmayan bir `size_t`'yi okumanın dürüst yolu |

Bunların yaptığı şeyler `(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)`'dir ve
dönüşümler 1. bölümün tamsayı türleriyle her çift için vardır. `ptr`'nin bu tablosu bile yoktur: bir
işaretçiyi sayı olarak okumanın yolu sağlanmamıştır. Yalnızca geçirilen, alınan ve başka bir C
fonksiyonuna iletilen bir değerdir.

**Yazdırılamazlar da.** `(println "~a" x)` bir ham sözcüğü kabul etmez (`Sexpr` gösterimi yoktur); bu
yüzden önce `(println "~a" (as int n))` gibi genişlikli bir türe geçin.

1. bölümün başındaki "64 bitlik bir tamsayı türü yoktur" bu üçü için de geçerlidir. **Saklanamadıkları
için** geçerlidir: bir `defstruct` alanı, bir `defvar`, bir tür bağımsız değişkeninin içi ya da bir
`Sexpr`'in içi olamazlar; bu yüzden bir fonksiyondan yalnızca bağımsız değişken, dönüş değeri ve yerel
değişken olarak geçen sözcüklerdir. Ayrıntılar için
[Sözdizimi Başvurusu](../syntax.md#ptr--c-long--c-ulong--ham-makine-sözcükleri)'na bakın.

## 3. Keyfi duyarlıklı tamsayılar `int`

CL'nin `integer`'ı ve bu dilin **tamsayısı**: türsüz tamsayı sabitleri bu türdedir ve `length` ile
`char->int` gibi bir sayı döndüren yerleşik fonksiyonlar bu türü döndürür. Bir değer, sığdığı sürece 63
bitlik anlık değer (fixnum) olarak tutulur, bir işlemin sonucu artık sığmadığında otomatik olarak
bignum'a yükseltilir ve yeniden sığdığında anlık değere geri döner. `eq`, fixnum aralığında her zaman
değer kimliğidir ve `eql`/`=` tüm aralık boyunca sayısal kimliktir. Sabit genişlikli tamsayı
türlerinden (1. bölüm) farklı bir türdür ve örtük dönüşüm yoktur: `(as int x)` sabit bir genişlikten
tam genişletmedir ve `(as i32 n)` / `(try-as i32 n)`, `int`'ten kırpma / denetimdir (1. bölümdeki
`int->W` / `try-int->W` ile aynı anlam).

`Sexpr`'in tamsayı varyantı da yalnızca `int`'tir (`(int n)` hem fixnum'ları hem bignum'ları kabul
eder).

Bir indeks ya da sayı alan yerleşik fonksiyonlar (`substring`, `Vector`'ün `get`'i, `ash`'in kaydırma
sayısı vb.) `int` kabul eder, ancak bir fixnum'a sığmayan bir değer geçirmek çalışma zamanı hatasıdır
("an integer argument does not fit a fixnum").

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Asla taşmaz (yükseltirler) |
| `/` | `(/ a b)` | `(int,int)→int` | Sıfıra doğru keser. Sıfıra bölmede panic olur |
| `mod` | `(mod a b)` | `(int,int)→int` | Taban bölmenin kalanı (işaret böleni izler) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Hepsi `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | 11. bölümdekiyle aynı (sonsuz sayıda bitli ikiye tümleyen) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | 1. bölümdekiyle aynı |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Kırpma / denetim. `W`, altı genişlikten biri ya da `c-long`/`c-ulong`'dur |
| `int->int` | | `int→int` | Özdeşlik (sabit genişlikli ve C sözcüğü tarafında `int->int` genişletir; 1. bölüm) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | 1. bölümle aynı biçim. `expt` yalnızca negatif olmayan üsleri kabul eder |

## 4. Kayan noktalı sayılar (`f64` / `f32`)

`f32` aynı tabloya sahiptir.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. Sıfıra bölme panic olmaz; `inf`/`NaN` verir |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Taban bölmenin kalanı (CL'deki gibi; işaret böleni izler. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Kesen bölmenin kalanı (CL'deki gibi; işaret bölüneni izler. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Karşılaştırma |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Hepsi `=` ile aynı |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Üs alma |
| `abs` | `(abs x)` | `f64→f64` | Mutlak değer |
| `signum` | `(signum x)` | `f64→f64` | İşaret (`1.0`/`-1.0`; `±0.0`/`NaN` olduğu gibi döndürülür. CL'deki gibi, Rust'ın `signum`'ından farklı) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | Büyük olan / küçük olan (üç ya da daha fazla bağımsız değişken 8. bölümün değişken sayılı şekeriyle açılır) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Tekli işlemler |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Aşkın fonksiyonlar. `log` doğal logaritmadır |
| `log` (iki bağımsız değişken) | `(log x base)` | `(f64,f64)→f64` | Verilen tabanda logaritma. `(/ (log x) (log base))`'e açılır (8. bölüm) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | CL'nin iki bağımsız değişkenli sürümlerine karşılık gelir (`(floor 7.0 2.0)`→bölüm 3, kalan 1). 1. bölümdeki aynı adlı fonksiyonlarla aynı tasarım (`car`=bölüm, `cdr`=kalan) |
| `float->int` | `(float->int x)` | `f64→int` | Sıfıra doğru keserek `int`'e çevirir (CL'nin `truncate`'i; her boyuttaki sonlu değerler için tam). Sonsuzluk ve NaN'da panic olur. Sabit bir genişlik için `(as i32 x)` kullanın |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Tam ikili rasyonel olarak bir `ratio`'ya çevirir (CL'nin `rational`'ı) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Kayan noktalı genişlikler arasında dönüştürür. `float->f32` en yakına yuvarlar, `float->f64` her zaman tamdır. `(as f32 x)`'in yaptığı şey |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | Aynı dönüşüm bir soru olarak. Yuvarlama değeri değiştiriyorsa `none` (`f64`'e genişletme her zaman `some`'dır). `(try-as f32 x)`'in yaptığı şey |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | CL'nin aynı adlı fonksiyonları. Yukarıdaki `floor`/`ceiling`/`round`/`truncate`'in takma adları: CL'de öneksiz olanlar tamsayı döndürür; bu yüzden `f` önekli olanlar bu dilin davranışıyla eşleşir |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | Sırasıyla 2 / 53 / 53 (yalnızca `0.0`'ın duyarlığı 0'dır). `f64` her zaman IEEE-754 binary64'tür; bu yüzden bunlar sabittir |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` ya da `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | Mantis (`[1/2,1)` aralığında, işaretsiz) ve üs. CL üç değer döndürür, ancak çoklu değerler yoktur; bu yüzden işaret `float-sign`'a bırakılmıştır |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | Tam 53 bitlik tamsayı mantisli aynı ayrıştırma. `mantissa * 2^exponent` tam olarak özgün değerdir |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **O kayan noktalı sayı olarak geri okunan en yalın rasyonel** (`(rationalize 0.1)`, `1/10`'dur). Tam ikili değer için `float->ratio` kullanın |

**CL'den fark: `round` nasıl yuvarlar.** `round` (ve dolayısıyla `fround`/`round-div`) **sıfırdan
uzağa** yuvarlar (`(round 2.5)` = `3.0`). CL **çifte** yuvarlar ve `2` verir.

## 5. Rasyonel sayılar `ratio`

CL uyumlu keyfi duyarlıklı rasyonel sayılar. Her zaman en sade biçimde ve pozitif paydayla tutulurlar ve
heap'te ayrılırlar. Tamsayı türleri ya da `f64` ile örtük dönüşüm yoktur (açık bir dönüşüm metodu ya da
`as`/`try-as` kullanın). Oran sabiti sözdizimi için
[Sözdizimi Başvurusu](../syntax.md#1-sözcüksel-öğeler)'na bakın.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | Dört işlem (sonuçlar her zaman en sade biçimde). `/` sıfıra bölmede panic olur |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Taban bölmenin kalanı (CL'deki gibi; işaret böleni izler) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Kesen bölmenin kalanı (CL'deki gibi; işaret bölüneni izler) |
| `abs` | `(abs x)` | `ratio→ratio` | Mutlak değer |
| `signum` | `(signum x)` | `ratio→ratio` | İşaret (`1`/`-1`/`0`'ı bir `ratio` olarak döndürür) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Üs alma. Üs, tamsayı değerli bir `ratio` olmalıdır (aksi halde panic olur). Negatif bir üs çarpmaya göre tersi verir |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | Büyük olan / küçük olan |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio`'nun bit işlemleri yoktur (CL'de bunlar yalnızca tamsayılar içindir) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Karşılaştırma |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Hepsi `=` ile aynı |
| `numerator` | `(numerator x)` | `ratio→int` | En sade biçimdeki pay (CL'deki ad aynı) |
| `denominator` | `(denominator x)` | `ratio→int` | En sade biçimdeki payda (her zaman pozitif) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Tamsayı kısmı (sıfıra doğru kesilmiş) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | `f64`'e çevirir |

Sabit genişlikli tamsayılardan ve `f64`'ten giriş yolları `int->int`/`int->ratio` (1. bölüm) ve
`float->int`/`float->ratio`'dur (4. bölüm). `int`/`ratio`, `i32` ve diğerlerinden bağımsız ayrı
türlerdir ve karışık aritmetik açık dönüşümler gerektirir.

## 6. Karmaşık sayılar `complex`

Standart kütüphanedeki bir struct (`defstruct`).

**CL'den iki fark** (ikisi de statik tür denetiminden kaynaklanır):

1. **Bileşenler her zaman `f64`'tür.** Bir CL karmaşık sayısı rasyonelleri de tutabilir ve
   `(complex 1 2)` ile `(complex 1.0 2.0)` farklı türlerdir. Statik bir tür birini seçmek zorundadır ve
   aşkın fonksiyonlar kayan noktalı türü döndürür.
2. **`(sqrt -1.0)` gerçek `sqrt`'tır (NaN).** CL'de `sqrt`, gerçek bir sayıdan karmaşık bir sayı
   döndürebilir, ancak `f64`'ün `sqrt`'ı bir `f64` döndürmek zorundadır. Karmaşık bir sonuç, karmaşık
   bir bağımsız değişkenden gelir: `(sqrt (complex -1.0 0.0))` `i`'dir.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Oluşturma. Bileşenler doğrudan `z::re`/`z::im` olarak okunabilir |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Gerçek kısım ve sanal kısım. **Gerçek sayılarda da çalışırlar** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), CL'deki gibi |
| `conjugate` | `(conjugate z)` | `complex→complex` | Eşlenik (gerçek sayılarda da çalışır) |
| `phase` | `(phase z)` | `complex→f64` | (-pi,pi] aralığında argüman (gerçek sayılarda da çalışır) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Mutlak değer. **Alıcının türünü döndürmeyen tek `abs`** (CL'deki gibi, bir karmaşık sayının mutlak değeri gerçektir) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Karmaşık aritmetik |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Bileşen bileşen eşitlik. `Eq` de gerçekleştirilmiştir (`Ord` yoktur: karmaşık sayıların sırası yoktur ve CL'nin `<`'ü de onları reddeder) |
| `zerop` | `(zerop z)` | `complex→bool` | Her iki bileşenin de 0 olup olmadığı |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` esas değerleri verir |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | `(x,y)` vektörünün açısı. **CL'nin iki bağımsız değişkenli `(atan y x)`'i bunun şekeridir** (iki bağımsız değişkenli `log` gibi bağımsız değişken sayısına göre dallanır) |

`print-object`'i gerçekleştirir; bu yüzden `~a`/`~s`, CL'nin yaptığı gibi onu `#C(re im)` olarak
yazdırır (bu dilin okuyucusunda geri okumak için `#C` sözdizimi yoktur).

## 7. Mantıksal değerler

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Olumsuzlama |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Hepsi değerleri eşitlik için karşılaştırır |

`and`/`or` kısa devre değerlendirmesi gerektirir; bu yüzden özel formlardır
([Sözdizimi Başvurusu](../syntax.md#4-bağlama-ve-koşullar)).

## 8. Sayısal yardımcılar ve çağrı şekeri

`abs`/`signum` (tüm sayısal türler), `gcd`/`lcm` (yalnızca tamsayı türleri), `rem` (`f64` dahil tüm
gerçek türler) ve `expt` (`int`/`f64`/`ratio`) her sayısal türün metotları olarak tanımlanır (alıcının
türüne göre çözümlenir: `(abs x)`, `x`'in türünün metodudur). Her tür için ayrıntılar 1, 3, 4 ve 5.
bölümlerdedir. Sabit genişlikli tamsayıların `expt`'i yoktur (yükseltmeleri yoktur ve taşarlardı;
`(as int x)` ile `int`'e geçin ve onun `expt`'ini kullanın).

### 8.1 Değişken sayılı ve 0/1 bağımsız değişkenli biçimler

CL'nin aritmetiği ve karşılaştırması değişken sayılıdır, ancak metotlar bağımsız değişken sayısına göre
değil, yalnızca alıcının türüne göre çözümlenir. Bu yüzden **denetleyici aşağıdaki biçimleri iki
bağımsız değişkenli çağrılara açar**.

| Yazabileceğiniz biçim | Açılım | Uygulandığı yerler |
|---|---|---|
| `(op a b c ...)` | Soldan katlama `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | Her terim geçici bir değişkene bağlanarak `(and (cmp a b) (cmp b c) ...)` | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Yukarıdakilerden etkisiz elemanı olanlar |
| `(op x)` | `+ * max min logand logior logxor` için `x`'in kendisi. `(- x)` işareti değiştirir, `(/ x)` tersini verir, `(gcd x)`/`(lcm x)` `(abs x)` verir (CL'deki gibi) | Yukarıdakiyle aynı |
| `(cmp x)` | `x`'i değerlendirir ve `true` verir | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Her terim soldan sağa tam olarak bir kez değerlendirilir (değişken sayılı karşılaştırmaların geçici
değişkenlerden geçmesinin nedeni budur). `/=`'in değişken sayılı biçimi, CL'nin tüm çiftlerin farklı
olup olmadığını sormasından farklı olarak **bitişik çiftleri** karşılaştırır.

### 8.2 `isqrt` ve tamsayı `expt`

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | Karekökü aşmayan en büyük tamsayı. Negatif bir değerde panic olur |
| `expt` | `(expt n e)` | `(T,T)→T` | Üs alma (kare alma yoluyla). CL negatif üs için bir rasyonel döndürür, ancak bir tamsayı türü bunu temsil edemez; bu yüzden panic olur; önce `ratio`'ya çevirin |

## 9. Yüklemler

| Ad | Biçim | Tür | Türler |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (CL'deki gibi yalnızca tamsayı türleri) |

CL'nin `numberp`/`integerp`/`floatp`'si gibi **tür yüklemleri yoktur**. Statik tür denetimiyle bir
değerin türü, çalışma zamanında sorulmadan zaten belirlidir.

## 10. Sabitler

| Ad | Tür | Değer |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | `boole`'a geçirilen işlem kodları (CL'nin anahtar sözcüklerinin yerine) |

Sayısal sınır sabitleri (CLHS 12.1.4.2 / 12.1.3):

| Ad | Tür | Açıklama |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | 63 bitlik anlık değerin üst / alt sınırı (2^62-1 / -2^62). Bunları aşan bir `int` bignum olur |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | En büyük / en küçük sonlu değerler |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | Alt normaller dahil en küçük sıfırdan farklı büyüklük |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | Aynısı, normalleştirilmiş sayılarla sınırlı |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | CL'nin tanımını izlerler (`(/= (+ 1 e) 1)` olan en küçük pozitif `e`); bu yüzden 2^-53'ten **bir ULP büyüktürler**: 2^-53'ün kendisi, çifte yuvarlayan en yakına yuvarlamada `1.0`'a geri yuvarlanır |

## 11. Bit işlemleri

Sonsuz sayıda bitli ikiye tümleyen üzerinde tanımlıdır (CL 12.10). Sabit genişlikli tamsayı türleri ve
`int` için gerçekleştirilmiştir, `ratio` için değil (CL'de de bit işlemleri yalnızca tamsayılar içindir).

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | Bit düzeyinde ve, veya, özel veya (değişken sayılı ve sıfır bağımsız değişkenli sürümler 8.1'de) |
| `lognot` | `(lognot x)` | `T→T` | Bit düzeyinde tümleyen |
| `ash` | `(ash x count)` | `(T,int)→T` | Aritmetik kaydırma. `count` pozitifse sola, negatifse sağa |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | `index` bitinin ayarlı olup olmadığı (**bağımsız değişken sırası CL'nin tersidir**; aşağıya bakın) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | Ayarlı bitlerin sayısı (negatif bir sayı için 0 bitlerinin sayısı) |
| `integer-length` | `(integer-length x)` | `T→T` | İşareti saymadan temsil etmek için gereken bit sayısı |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | Yukarıdakilerden oluşturulan kalan yedisi |

**`ash`'in yalnızca ikinci bağımsız değişkeni `T` değil, `int`'tir.** Bu, alıcının türünden bir değer
değil, bit cinsinden bir **uzaklıktır**; bu yüzden alıcının genişliği ve işaretliliği uzaklık hakkında
hiçbir şey söylemez (CL'nin `(ash integer count)`'undaki `count`'un herhangi bir tamsayı olmasıyla aynı
nedenle). İşaretsiz bir değeri sağa kaydırmak mantıksal bir kaydırmadır (`(ash (the u8 200) -3)` = `25`)
ve işaretli bir değerin kaydırılması negatif sonsuza yuvarlayan aritmetik bir kaydırmadır
(`(ash (the i32 -100) -4)` = `-7`). `logbitp`'nin `index`'i de aynı nedenle `int`'tir.

**Bayt belirleyicileri.** CL'nin `byte`'ının döndürdüğü opak nesne yerine bir `cons-cell<int,int>`
(`car`=boyut, `cdr`=konum) kullanılır. Hem boyut hem konum bit sayılarıdır; bu yüzden ayrıştırılan
tamsayının genişliği ne olursa olsun `int`'tirler.

**Tamsayı ilk bağımsız değişkendir; CL'den farklı bir sırayla.** CL `(ldb bytespec integer)` yazar,
ancak bu dil bir metodu alıcının (ilk bağımsız değişkenin) türüne göre seçer ve belirleyici önce
gelirse tamsayının türüne göre seçemezdi. Diğer tüm bit işlemleri `(op integer ...)` biçimindedir
(`(logand a b)`, `(ash x count)`, `(lognot x)`); yalnızca `ldb` ailesi ve `logbitp` ters idi, bu yüzden
bunlar hizalandı. Kalan bağımsız değişkenler CL'nin göreli sırasını korur; bu yüzden
`(dpb newbyte spec n)`, `(dpb n newbyte spec)` olur.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Bir bayt belirleyicisi yapar |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Bir bileşeni çıkarır |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | `x`'ten belirtilen baytı sağa yaslı çıkarır |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Belirtilen baytta herhangi bir bitin ayarlı olup olmadığı |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Belirtilen baytın dışındaki her şeyi temizler (konumları korur) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Sağa yaslı `newbyte`'ı `x`'in belirtilen baytına yerleştirir |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | `dpb`'nin konumu koruyan sürümü |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | `op` ile seçilen 16 iki işlenenli mantıksal işlemden biri (10. bölümden bir `boole-*` sabiti) |

`T`, `Bits` trait'ini gerçekleştiren bir türdür; yani `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Yalnızca
`boole` `op`'u ilk sırada tutar; çünkü orada CL'nin sırasını değiştirmek için bir neden yoktur.

## 12. Rastgele sayılar

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | `0`'dan `n`'e kadar (`n` hariç) bir rastgele sayı. Durum atlanırsa `*random-state*`'ten çeker ve onu ilerletir |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Bağımsız değişken olmadan yeni bir durum; biri verilirse onun bir kopyası (kopya aynı diziyi yeniden oynatır) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Her zaman `true` (statik tür diğer türleri zaten dışlar; yalnızca CL'ye karşılık gelmek için vardır) |
| `*random-state*` | — | `random-state` | `random`'ın varsayılan durumu. Atanabilen bir global (`setf` ile değiştirin) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | Tamsayının adlandırdığı durum. Aynı tohum her zaman aynı diziyi yeniden oynatır |

Üreteç xorshift64'tür ve yorumlansa da derlense de aynı diziyi döndürür.

`make-random-state`'ten gelen yeni bir durum duvar saatinden tohumlanır; bu yüzden çalıştırmalar arasında
yeniden üretilemez. Yeniden üretmek için `seed-random-state` kullanın:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; prints the same three numbers on every run
```

**CL'de tohum vermenin taşınabilir bir yolu yoktur** (`make-random-state` yalnızca `nil`/`t`/bir durum
alır); bu yüzden bu ad CL'yi değil, SBCL'in `sb-ext:seed-random-state`'ini izler.

Farklı tohumlar farklı diziler verir. `(seed-random-state 0)` ve `(seed-random-state 1)` farklı diziler
verir; `-7` ve `7` de öyle.
