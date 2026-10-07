<!-- translated-from: docs/ja/reference/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Hata Mesajları

`typl`'nin başlıca hata mesajlarının ne anlama geldiği ve nasıl düzeltileceği.

## 1. Bir hatayı okuma

Hatalar standart hataya şu biçimde yazılır:

```text
error: file:line:column: kind: message
```

`kind`, hatanın ne zaman bulunduğunu söyler.

| Tür | Ne zaman | Anlamı |
|---|---|---|
| `type error` | Çalışmadan önce (denetim sırasında) | Türlerde ya da adlarda bir hata. O form çalıştırılmaz |
| (tür yok) | Okuma ya da denetim sırasında | Dengesiz parantezler gibi bir sözdizimi hatası ya da bulunamayan bir ad |
| `panic` | Çalışırken | Kurtarılamaz bir başarısızlık. Program, `unwind-protect` temizliğini çalıştırdıktan sonra durur |

`warning:` ile başlayan satırlar uyarılardır ve işlem sürer.

`file:line:column`, hatalı ifadeyi gösterir. Standart kütüphane fonksiyonunun içinde ortaya çıkan bir
çalışma zamanı hatası için, programın o fonksiyonu çağırdığı yeri gösterir. Bazı hataların konumu yoktur
(`error: panic: ...` gibi).

Örnek:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Bu, `main.typl` dosyasının 1. satır, 24. sütunundaki ifadenin, `i32` beklenen yerde bir `string`
olduğu anlamına gelir.

## 2. Denetim sırasındaki hatalar

Çalışmadan önce bulunan hatalar. Düzeltilene kadar form çalıştırılmaz.

### 2.1 Türler

| Mesaj | Anlamı ve düzeltme |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | `U` türünde bir ifade, `T` türünün gerektiği yerde duruyor. Örtük dönüşüm yoktur; sayılar için `(as T x)` ile dönüştürün. `int` ve `i32` de farklı türlerdir |
| ``integer literal 300 is out of range for u8 (0..=255)`` | Sabit türe sığmıyor. Kırpılmasını istiyorsanız `(as u8 300)` yazın |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Bu adda bir tür yok. Bir türü, onu kullanan ilk formdan önce tanımlayın (türlerin ileri bildirimi yoktur). Bir tür değişkeni kastettiyseniz, onu fonksiyon adından sonraki `<foo>` gibi bir bildirim konumuna yazın ([Sözdizimi Başvurusu 3.6](syntax.md#36-defstruct--structlar-kullanıcı-tanımlı-türler)) |
| ``cannot infer type argument `t` for `vector::new` `` | Bir tür bağımsız değişkeni belirlenemiyor. Türü `(the Vector<int> (Vector::new))` gibi `the` ile yazın |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` her varyantı ele almıyor. Eksik varyantlar için kollar ya da bir `_` kolu ekleyin |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | Fonksiyon, geçirdiğiniz türün gerçekleştirmediği bir trait istiyor. `(impl Eq pt ...)` yazın ([Standart Trait'ler](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | `:dyn` beklenen yere, trait'i gerçekleştirmeyen bir türün değeri geçirildi. `impl`'i yazın |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Bir türün geleceği yere trait adı yazıldı. `:dyn Error` yazın |
| ``if: (if cond then else)`` | `if`'in biçimi yanlış. `if` bir else dalı gerektirir. İhtiyacınız yoksa `when` kullanın |

### 2.2 Adlar

| Mesaj | Anlamı ve düzeltme |
|---|---|
| `no such function: bar` | Bu adda bir fonksiyon ya da metot yok. Yazımı kontrol edin |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Metotlar ilk bağımsız değişkenin türüne göre seçilir. Bu adda bir metot var, ancak ilk bağımsız değişkenin türü için (burada `int`) yok. Mesajın sonu, metoda sahip türleri listeler |
| `unbound variable: y` | Bu adda bir değişken yok. Yazımı ve bağlamanın kapsamını kontrol edin (`let`'inin dışında mı kullanılıyor?) |
| ``use: unresolved `nosuch` `` | `use` içinde adı verilen modül bulunamıyor. Dosya adlarının modül yollarına nasıl karşılık geldiği için [Sözdizimi Başvurusu 3.11](syntax.md#311-dosyalar-ve-modüller-çok-dosyalı-projeler)'e bakın |
| `unresolved path: c::hidden` | Modül var, ancak ad yok ya da `pub` eksik olduğu için görünür değil |
| `circular module dependency: a -> b -> a` | Modüller birbirini `use` ediyor. Ortak kısmı ayrı bir modüle taşıyın |
| ``return-from: no enclosing block named `nope` `` | `return-from`'a verilen adda bir `block` onu çevrelemiyor. Bir fonksiyonun block'u yalnızca o fonksiyonun içinde kullanılabilir |

### 2.3 Çağrılar

| Mesaj | Anlamı ve düzeltme |
|---|---|
| `f: expected 1 argument(s), got 2` | Bağımsız değişken sayısı uyuşmuyor |
| `f: unknown keyword argument :b` | Fonksiyonda bulunmayan bir anahtar sözcüklü bağımsız değişken geçirildi |
| `new: expected 1 field(s), got 2` | Bir struct yapıcısına geçirilen değer sayısı, alan sayısıyla uyuşmuyor |
| ``setf: cannot assign to constant `k` `` | `defconstant` ile tanımlanmış bir ada atama yapıldı. Değişmesi gerekiyorsa `defvar` kullanın |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | `defsignature` ile bildirilen bir fonksiyon tanımlanmamış |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | Bağımsız değişken türlerinden hiçbirinde `~/name/` ile çağrılan metot yok ([Biçim Yönergeleri 5. bölüm](functions/format.md#5-name)) |

## 3. Okuma hataları

| Mesaj | Anlamı ve düzeltme |
|---|---|
| `unexpected end of input while reading a list` | Bir kapanış parantezi eksik. Konum, okumanın bittiği yeri (dosyanın sonu gibi) gösterir; bu yüzden açılış parantezini arayın |

## 4. Çalışma zamanındaki hatalar (panic)

| Mesaj | Anlamı ve düzeltme |
|---|---|
| `panic: divide by zero` | Tamsayılarla ya da oranlarla sıfıra bölme. Kayan noktalı sıfıra bölme panic olmaz; `inf`/`NaN` verir |
| `panic: unwrap: called on none` | `none`'a `unwrap` uygulandı. `none` durumunu `match` ya da `unwrap-or` ile ele alın |
| `panic: Vector: index 5 out of bounds` | Aralık dışı bir indeks. Uzunluğu `len` ile kontrol edin ya da aralık dışında `none` döndüren bir fonksiyon kullanın (`nth`, `pop` vb.) |
| `panic: an integer argument does not fit a fixnum` | Bir indeks ya da sayı alan bir bağımsız değişkene, 63 bite sığmayan bir `int` geçirildi |
| `throw: no enclosing (catch 'oops) for this throw` | Aynı etiketli çevreleyen bir `catch` olmadan bir `throw` çalıştı |
| `panic: <message>` | Program `(panic "<message>")` çağırdı. Başarısız bir `assert`, `assertion failed: ...` verir |

Bir `panic`, bir task'in içinde olsa bile tüm süreci durdurur
([Sözdizimi Başvurusu 12.4](syntax.md#124-diğer-özelliklerle-etkileşim)). Kurtarmak istediğiniz
başarısızlıkları `Result` ile ifade edin ([Sözdizimi Başvurusu 9. bölüm](syntax.md#9-hata-yönetimi-politikası)).

## 5. Uyarılar

| Mesaj | Anlamı |
|---|---|
| ``warning: redefining function `f` `` | Aynı adlı bir fonksiyon yeniden tanımlandı. Sonraki tanım geçerli olur. REPL'de bir tanımı düzelttiğinizde normal olarak görünür |
