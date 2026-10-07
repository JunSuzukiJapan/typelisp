<!-- translated-from: docs/ja/reference/syntax.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp Sözdizimi Başvurusu

typelisp, S-ifadelerle yazılan, statik tür denetimli bir Lisp'tir. Yerleşik fonksiyonların ve metotların
listesi için [Yerleşik Fonksiyonlar](functions/README.md), türlerin listesi için [types.md](types.md) ve
hata mesajlarının okunması için [errors.md](errors.md) belgelerine bakın.

## 1. Sözcüksel öğeler

- **Büyük/küçük harf duyarsız.** Semboller okunurken hep küçük harfe normalleştirilir.
- **Yorumlar**: `;`'den satır sonuna kadar (satır yorumları). `#| ... |#` (iç içe geçebilen blok
  yorumları).
- **Okuma zamanı değerlendirmesi**: `#.(expr)`, **okurken sonraki formu çalıştırır** ve değerini okunan
  şey olarak ele alır. Okuyucunun metnin bir fonksiyonundan fazlası olduğu tek yer burasıdır. Ne kadar
  uzağa erişebileceği, CL'deki gibi okuma yoluna bağlıdır:
  - `(load ...)` ve REPL her seferinde bir formu değerlendirir; bu yüzden **aynı metinde daha önce
    tanımlanan fonksiyonları** çağırabilir (CL'nin `load`'u).
  - Bir modül dosyası bir bütün olarak denetlenir ve onu `use` edenin tarafından çalıştırılır; bu yüzden
    `#.` yalnızca standart kütüphaneye ve oturumun zaten çalıştırdığı şeylere erişebilir. Ne dosyanın
    kendi tanımları ne de `use` ettiği modüllerin tanımları **henüz çalışmamıştır** (CL'nin
    `compile-file`'ının `eval-when`'e ihtiyaç duyması gibi).
  - Bir program içindeki `read` / `read-from-string` de `#.`'yı değerlendirir (CL'deki gibi).
  - `*read-eval*`'i (varsayılan `true`) `false` yapmak, `#.`'yı her yerde bir okuma hatası yapar:
    veri olarak okunan metnin kod çalıştırmasını engelleyen bir anahtar (CL'deki gibi). Her `#.`'da
    danışılır; bu yüzden bir `setf`, okunan sonraki formdan itibaren etkili olur.
    `with-standard-io-syntax` içinde `true`'dur.
- **Mantıksal değerler**: `true` / `false`.
- **Tamsayılar**: onluk (`42`, `-7`). Önce bir `+`/`-` işareti gelebilir. Diğer tabanlar CL'nin taban
  sözdizimi `#b`/`#o`/`#x`/`#NNr` ile yazılır (işaret belirteçten sonra gelir: `#x-ff`). `0x` öneki
  CL'de yoktur ve benimsenmemiştir: `0xff` bir sembol olarak okunur.
  Tür ek açıklaması olmayan bir tamsayı sabiti varsayılan olarak `int`'tir (keyfi duyarlık,
  [Sayılar](functions/numbers.md#3-keyfi-duyarlıklı-tamsayılar-int)) ve boyutunun üst sınırı yoktur.
  **Beklenen tür sabit genişlikli bir tamsayı türüyse sabit o türü alır ve türün değeri tutabildiği
  denetlenir**: `(the u8 300)` bir tür hatasıdır (kırpılmasını istiyorsanız `(as u8 300)` yazın).
  `(the u32 4294967295)` ve `(the u32 #xFFFFFFFF)` bu kural sayesinde yazılabilir.
  Bir `int` değerinin 63 bitlik anlık değere sığıp sığmadığı ya da bignum olup olmadığı, boyutuna göre
  belirlenir ve özel bir sözdizimi yoktur (CL'deki gibi).
- **Kayan noktalı sayılar**: ondalık nokta ya da üs (`e`/`E`) içerenler (`1.5`, `3.0e10`). Varsayılan
  olarak `f64` (beklenen tür buysa `f32`).
- **Oranlar**: `pay/payda` (yalnızca onluk, örneğin `1/3`). CL'nin belirttiği gibi okunurken sadeleştirilir
  (`2/4`, `1/2`'dir). Tamsayı değerli olanlar (`4/2` vb.) `ratio` değil, `int` olarak okunur. Sıfır
  payda (`1/0`) bir okuma hatasıdır.
- **Karakterler**: `#\` ve ardından bir karakter ya da bir karakter adı. Örneğin `#\a` `#\Space`
  `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul` (ayrıca `#\Null`) `#\Backspace`. Adlar büyük/küçük
  harf duyarsızdır.
- **String'ler**: `"..."`. Kaçışlar `\n` `\t` `\r` `\0` `\\` `\"`'dır (diğer her `\x` yalnızca `x`'tir).
- **Semboller**: harf, rakam ve simge içeren herhangi bir belirteç (`+` `<=` `my-func` vb.).
- **Anahtar sözcükler**: `:name` gibi iki nokta üst üsteyle başlayan semboller (CL'deki gibi). Kendi
  kendilerini değerlendirirler: hiçbir bağlamaya bakmazlar ve değerleri kendileridir; statik türleri
  `symbol`'dür. Aynı adlı anahtar sözcükler her zaman aynı nesnedir (`(eq :foo :FOO)` doğrudur; diğer
  semboller gibi küçük harfe çevrilirler). İki noktanın kendisi adın bir parçasıdır; bu yüzden
  `(symbol->string :foo)`, `":foo"`'dur (typelisp'in paket sistemi yoktur; bu, CL'nin `symbol-name`'inden
  farklıdır). Yalnız bir `:` ya da `:a:b` gibi ek iki noktalı olanlar bir okuma hatasıdır. `keywordp` ile
  sınayın. `::` ile başlayanlar anahtar sözcük değil, mutlak yollardır (aşağıda).
  `:dyn`'in yalnızca tür konumları için ayrılmış bir anahtar sözcük olduğunu, başka bir yere yazmanın
  hata olduğunu unutmayın ([2. bölüm](#2-türlerin-yazımı)'e bakın).
- **Listeler**: `(a b c)`. Noktalı çiftler `(a . b)` de okunabilir.
- **Boş liste `()`**: bağlama göre `Unit` türünün değeri ya da `Option<Sexpr>`'in `none`'ı.
  **`Sexpr`'in boş liste varyantı yoktur**: `Sexpr`, "boş olmayan bir S-ifade" demektir ve S-ifade
  verisinin türü `Option<Sexpr>`'dir ([4.3 match](#43-match--örüntü-eşleme) içindeki "`Option<Sexpr>`
  için örüntüler"e bakın).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (yalnızca bir quasiquote içinde anlamlıdır)
  - `,@x` → `(unquote-splicing x)` (açılımda liste elemanları olarak yerleştirilir)
- **`::` yolları**: `foo::bar`, modüller, türler ve üyeler boyunca bir yol olarak okunur (tek bir sembol
  adı olarak değil). `::foo` gibi `::` ile başlayan bir yol, kökten mutlak bir yoldur. Jenerik
  bağımsız değişkenlerin içindeki bir `::` (`Vec<a::b>` ve benzerleri) yol ayırıcısı sayılmaz.

## 2. Türlerin yazımı

Kaynakta türler sıradan semboller ya da listeler olarak yazılır.

- **İlkel türler**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string`
  `symbol`. `int`, tamsayı türüdür (CL'nin integer'ı; 63 bitlik anlık değerler ile bignum'lar arasında
  otomatik geçiş yapar; [Sayılar](functions/numbers.md#3-keyfi-duyarlıklı-tamsayılar-int)) ve altı sabit
  genişlikli tür genişliklerinden ve işaretliliklerinden adlandırılır (64 bitlik bir tamsayı türü yoktur;
  [Sayılar](functions/numbers.md#1-sabit-genişlikli-tamsayılar)'a bakın).
- **Rasyonel tür**: `ratio` (en sade biçimdeki rasyonel sayılar). CL'deki gibi heap'te ayrılır;
  `int`/`f64` ve benzerleriyle örtük dönüşüm yoktur (`as`/`try-as` ya da bir dönüşüm metoduyla açıkça
  dönüştürün; [Sayılar](functions/numbers.md#5-rasyonel-sayılar-ratio)'a bakın).
- **C sınırındaki ham sözcükler**: `ptr` (opak bir işaretçi), `c-long` / `c-ulong`. Yalnızca FFI için:
  birini değer yapmak `(unsafe ...)` gerektirir ve görünebilecekleri yerler sınırlıdır
  ([3.3 defffi](#ptr--c-long--c-ulong--ham-makine-sözcükleri)). Bunları 64 bitlik bir tamsayı
  istediğiniz yerde kullanmayın: aritmetikleri yoktur.
- **Opak değiştirilebilir türler**: `random-state` (bir rastgele sayı üretecinin durumu).
  `Vector<T>`/`HashTable<K,V>`/`Sexpr` içine giremez (`Option<T>`/`Result<T,E>` içine girebilir).
- **Unit türü**: `()`
- **Never türü**: `!` (`panic`/`unreachable`/`todo` ya da hiç dönmeyen bir döngü gibi sapan ifadelerin
  türü. Her beklenen türe uyar)
- **Fonksiyon türleri**: `(fn (bağımsız-değişken-türleri...) dönüş-türü)`. Değişken sayılı bağımsız
  değişkenli bir fonksiyonun türü `(fn (bağımsız-değişken-türleri... &rest eleman-türü) dönüş-türü)`'dür.
- **Jenerik türler**: `Name<T1,T2,...>` (boşluksuz tek bir belirteç olarak okunur).
  Örneğin `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  Unit türü `()` de bir tür bağımsız değişkeni olarak yazılabilir (`Result<(), FileError>`). `(`/`)`
  normalde bir belirteci bitiren ayırıcılardır, ancak bir açılı parantez açıkken bu tek çift karaktere izin
  verilir. `()` bir alan türü ya da bir bağımsız değişken türü olarak da kullanılabilir.
- **Jenerik türlerin uygulama biçimi**: `(Name T1 T2 ...)`, `Name<T1,T2,...>` ile aynı türü adlandıran
  bir liste yazımı. Örneğin `(vector char)`, `Vector<char>` ile aynıdır.
  Ad biçimi olağan yazım yoludur; bu biçim **bir tür bağımsız değişkeninin bir ad içinde
  yazılamadığı durumlar için vardır**: bir tür bağımsız değişkeni kendi başına bir tür ifadesidir, ancak
  tek belirteçli bir ad içinde yalnızca adlar, `()` ve `:dyn` yazılabilir; fonksiyon türleri yazılamaz
  (`Vector<(fn (i32) i32)>` diye bir yazım yoktur). Gerçekleştirim bir türü gösterdiğinde de bu biçimde
  görünebilir; örneğin bir trait'in ilişkili türünü bir imzaya yerleştirmenin sonucu.
- **Nitelikli tür adları**: `module::Type` gibi `::` ile nitelenebilir.
- **Trait nesnesi türleri**: `:dyn Trait` (tek bir tür oluşturan boşlukla ayrılmış iki sözcük). Somut
  türü çalışma zamanında belirlenen bir değeri temsil eder; trait metot çağrıları bir vtable üzerinden
  gider (dinamik dağıtım). İlişkili türleri olan bir trait için bunlar bildirim sırasına göre konumsal
  olarak sabitlenir (`:dyn Iter<i32>`, `Item`'ı `i32`'ye sabitler). Jenerik bağımsız değişkenlerin içine
  de yazılabilir: `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`. Somut değerler beklenen
  konumlarda otomatik olarak kutulanır; açık biçim `(as :dyn Trait expr)`'dir.
  Bir `:dyn Sub` değeri, üst trait'lerinden herhangi birinin (geçişli olarak miras aldığı her şey)
  `:dyn Super`'ünün gerektiği yere olduğu gibi geçirilebilir (yukarı dönüşüm). İlgisiz bir trait'e
  geçirilemez.
  Bir trait'in `:dyn` ile kullanılması için karşılaması gereken koşullar için
  [3.9 deftrait / impl](#39-deftrait--impl--traitler)'a bakın. `:dyn`'i bir tür konumunun dışına yazmak
  hatadır.
- Yerleşik jenerik türler: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>` ve eşzamanlılık türleri `Task<T>` / `Thread<T>` / `Chan<T>`
  ([12. bölüm](#12-eşzamanlılık-taskler)). S-ifade verisinin türü olan `Sexpr` de vardır. Yerleşik somut
  hata türleri `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` / `FileError` /
  `NetError`'dır ve standart kütüphanede `SimpleError` / `WrappedError` struct'ları vardır (`Error` bir
  tür değil, bir trait'tir: `:dyn Error` olarak kullanın). Liste [types.md](types.md) belgesindedir.
- **Türler ve trait'ler tek bir ad alanını paylaşır** (Rust'taki gibi): tek bir modül içinde bir tür
  (`defstruct`/`defenum`) ile bir trait (`deftrait`) aynı ada sahip olamaz.

## 3. Üst düzey tanımlar

### 3.1 defun — fonksiyon tanımları

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Bağımsız değişken türleri ve dönüş türü zorunludur.
- Jenerik bir fonksiyon tür parametrelerini adından sonra açılı parantezler içinde yazar:
  `(defun name<T1,T2...> (params) Ret body...)` (tür konumlarındaki `Vector<T>` ile aynı açılı parantez
  sözdizimi).
- `defun`/`lambda`/`defmethod`, sonda `&rest (name Type)` yazıldığında değişken sayılı bağımsız
  değişkenleri kabul eder: `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (gövdede `xs` her
  zaman bir `Option<Sexpr>`, yani bir S-ifade listesi olarak bağlanır. Çağrıdaki her gerçek bağımsız
  değişken ayrı ayrı `Type2` olarak tür denetiminden geçer ve sonra bir `Sexpr` içine sarılır).
  `defmacro`'nun da kendi `&rest`'i vardır, ancak her zaman türsüz bir `Sexpr` olmasıyla farklıdır
  (`defun`/`lambda` eleman türünü belirtir). Bir fonksiyon türü de değişken sayılı bir fonksiyonu
  `(fn (T1... &rest Te) Ret)` olarak betimleyebilir.
- **`&optional` / `&key`** (`defun` ve `defmethod` için; aşağıdaki nedenle `lambda`/`labels` için değil
  ve `defmacro`'nun ayrı bir gerçekleştirimi vardır, o da aşağıda). Sıra CL'dekidir:
  `required &optional &rest &key`. Her parametre `(name Type)` ya da `(name Type default-expr)` olarak
  yazılır:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; no default
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> in the body

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; with a default
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; the caller writes `:name value`, in any order; omitted ones take their defaults
  ```

  - **Varsayılan ifadesi olmayan bir parametrenin türü `Option<Type>`'dır.** Atlanırsa `none`'dır;
    geçirilirse çağıranın yazdığı çıplak değer otomatik olarak `some` içine sarılır. CL'nin bir supplied-p
    değişkeniyle yaptığı şey ("sağlandı mı?") bunun yerine statik türün tarafında görünür.
  - Bir varsayılan ifadesi varsa tür, bildirildiği gibi `Type` kalır. Atlandığında o **denetlenmiş ifade**
    çağrıya olduğu gibi gömülür (her çağrıda değerlendirilir).
  - **`&key`, bir bağımsız değişken listesinde `&optional`/`&rest` ile karıştırılamaz.** Bu,
    CL'nin kendisinin sahip olduğu bir belirsizliği (sondaki bir gerçek bağımsız değişkenin konumsal bir
    `&optional` tarafından mı alındığı yoksa bir `&key` olarak etiketle mi eşleştiği *değerlere* bağlıdır)
    birleşimi yasaklayarak önler. `&optional` ve `&rest` birlikte kullanılabilir.
  - Jenerik fonksiyonlarda kullanılabilirler, ancak **yalnızca atlanan bağımsız değişkenlerde görünen bir
    tür parametresi çıkarılamaz ve hatadır** (eşleşecek bir değer yoktur).
  - **`defmethod` aynı üç bölüme sahip olabilir** (hem örnek metotları hem statik fonksiyonlar için).
    `&optional`/`&rest`/`&key`'i alıcıdan sonra listeleyin:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; static function
    (point::origin :y 7)
    ```

    Jenerik türlerin metotlarında da kullanılabilirler, ancak **varsayılan ifadesi olan bir parametrenin
    türü sahibin tür parametrelerinden söz edemez** (`defun`'un kendi tür parametreleri için sahip olduğu
    aynı kısıtlama: bağımsız değişken atlandığında gömülen şey *denetlenmiş* bir ifadedir; bu yüzden
    türü soyut bir değişken olarak bırakılamaz).
  - **Trait metotlarında kullanılamazlar.** `deftrait`'in bunlar için sözdizimi yoktur ve yalnızca
    `impl` tarafı bölümleri bildirebilseydi, bir `:dyn` alıcılı çağrılar (bağımsız değişkenleri trait'in
    bildiriminden dolduran) ile somut alıcılı çağrılar (bunları `impl`'in bildiriminden dolduran) farklı
    şeyler olurdu. Bir vtable yuvasının arity'si sabittir.
  - **`lambda` / `labels` içinde kullanılamazlar** (`&rest` kullanılabilir). Atlanan bir bağımsız
    değişkeni doldurmak için çağıranın **çağrılanın denetlenmiş varsayılan ifadesini** okuması gerekir;
    bu yalnızca ada göre çözümlenen bir imzadan elde edilebilir. Bir `lambda` bir değer olarak dolaştırılır
    ve o değeri betimleyen tek şey fonksiyon türü `(fn ...)`'dir: içinde bir ifade için yer yoktur ve
    olsaydı "aynı imzalı ama farklı varsayılanlı iki lambda" farklı türler olurdu. `&rest` türler
    meselesinin içinde kalır; bu yüzden bir fonksiyon türünde yazılabilir.
- **İleri başvurular `defsignature` ile bildirilir** (aşağıda). Bildirilmemiş bir ad, tanımından önce
  çağrılamaz; çünkü üst düzey, kaynak sırasıyla her seferinde bir form denetlenir ve çalıştırılır.
- Trait sınırlarını zorunlu kılmak için gövdeden hemen önce bir `where` yan tümcesi yazın:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (bir ilişkili türü `(AssocName ConcreteType)` ile sabitlemek isteğe bağlıdır).
- **Docstring'ler**: gövdenin başındaki, varsa `where` yan tümcesinden hemen sonraki bir string sabiti
  docstring olur (CL'deki gibi). Ancak yalnızca onu en az bir gövde formu izlediğinde: tek başına bir
  string dönüş değeri olarak kalır ve docstring sayılmaz: `(defun f () string "doc" "value")` bir
  docstring'e sahiptir ve `"value"` döndürür; `(defun f () string "value")` ise docstring'e sahip değildir
  ve `"value"` döndürür. `(documentation name)` ile alınabilir
  ([docstring'ler](functions/system.md#7-docstringler--documentation)).

### 3.2 defsignature — ileri bildirimler

```lisp
(defsignature name (argument-types...) return-type)
(pub defsignature name (argument-types...) return-type)
```

Kendinizden **sonra** tanımlanan bir `defun`'u çağırmak için onu önce böyle bildirin. Karşılıklı
özyineleme yalnızca bu şekilde yazılabilir:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Bağımsız değişkenler **yalnızca türler** olarak listelenir; gövde yoktur; bu yüzden ad verilecek bir şey
yoktur. `&rest` sona `&rest eleman-türü` olarak yazılabilir.

Bildirimler **denetlenir**:

- Onu izleyen tanım bildirimle eşleşmelidir (bağımsız değişkenlerin sayısı ve türleri, dönüş türü,
  `&rest` ve `pub` olup olmadığı). Uyuşmazlık tanımda bir hatadır.
- Tanımlamadan bildirmek bir hatadır (dosya / modül yüklemeyi bitirdiğinde bildirilir). REPL bunu her
  girdiden sonra bildirmez; çünkü bir bildirim ile tanımı ayrı satırlarda yazılabilmelidir.
- Tanımdan **sonra** konan bir bildirim hatadır; çünkü böyle bir bildirim hiçbir şey yapamazdı.

Üç şey bildirilemez:

- **Jenerik fonksiyonlar.** Her tür için bir kopya yapmak gövdeyi gerektirir ve bir bildirimin gövdesi
  yoktur. İleri bir çağrı çözümlenebilirdi, ancak örnekleme başarısız olurdu; bu yüzden bildirim baştan
  reddedilir.
- **`&optional`/`&key`.** İmzaları, her varsayılan değerin **denetlenmiş** ifadesini içerir (bağımsız
  değişken atlandığında çağrıya gömülür) ve bir bildirimin bunun için yeri yoktur.
- **`defun` dışındaki her şey.** Bir `defmacro`, açılabilmek için makro gövdesinin **zaten çalışmış**
  olmasını gerektirir; bir imza kaydetmek bunun yerini tutamaz. Türler (`defstruct`/`defenum`/
  `deftrait`) için onları kaydetmek, "türün kendisini kaydeden kodun ihtiyaç duyduğu şeydir"; bu, bir
  imzanın olduğu gibi kendi kendine yeterli değildir. Bir `defmethod`, sahibi olan türe kaydedilir; bu
  yüzden türü izler.

CL'deki karşılığı `(declaim (ftype (function (i32) bool) even2))`'dir, ancak bu bütün bir bildirim
sistemiyle gelir ve yalnızca **danışmandır**. Burada, statik tür denetimiyle bildirimler denetlenir.

### 3.3 defffi — C fonksiyonlarını bildirme (FFI)

```lisp
(defffi (name "c_symbol") (argument-types...) return-type)
(defffi (name "c_symbol") (argument-types...) return-type :library "name")
(defffi name (argument-types...) return-type)              ; name = the C symbol name
(pub defffi ...)
```

Bir C fonksiyonunu çağrılabilmesi için bildirir. Biçim `defsignature` ile aynıdır (bir ad, bağımsız
değişken türleri, bir dönüş türü ve gövde yok), ancak gövdenin olmaması farklı bir anlama gelir.
`defsignature`, "bunu sonra tanımlayacağım" sözüdür; `defffi` ise "başka biri gövdeyi zaten yazıp
derledi" diye bildirir.

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

typelisp adı ve C sembol adı ayrı yazılabilir; çünkü typelisp tanımlayıcıları genellikle `-` içerir ve C
tanımlayıcıları içeremez. C adı atlanırsa ad olduğu gibi C sembol adı olarak kullanılır.

**Çağrılar `(unsafe ...)` gerektirir** (yalnızca skalerler üzerindeki fonksiyonlar için bile).
Derleyicinin bildirilen C imzasının gerçek olanla eşleştiğini doğrulamanın bir yolu yoktur ve yalnızca
bildirime güvenebilir; `unsafe`, bu sorumluluğu üstlendiğinizin işaretidir. Amaçlanan yol, onu bir kez
sarıp güvenli bir sarmalayıcı yapmaktır:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; no unsafe needed from here on
```

Yazılabilen türler `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void) `string` `ptr`
`c-long` `c-ulong` ve türlü işaretçiler `(ptr T)`'dir
([aşağıda](#def-c-struct-ve-türlü-işaretçiler--c-structları-ayırma)).

`string`, `const char *`'dır. typelisp string'leri NUL ile sonlandırılmaz ve NUL'un kendisini içerebilir;
bu yüzden **geçirilirken bir C string'ine kopyalanırlar** ve çağrıdan sonra serbest bırakılır.
String'deki bir NUL hatadır: C yalnızca ona kadar bakardı; bu yüzden sessizce farklı bir string
geçirilmiş olurdu.

**Döndürülen string'ler de kopyalanır** ve serbest bırakılmaz: C'nin döndürdüğü C'ye aittir ve
`getenv`'de olduğu gibi statik bir tabloya işaret ediyor olabilir. Çağıranın serbest bırakması gereken
belleği döndüren fonksiyonlar (`strdup` vb.) `ptr` olarak alınmalı ve kendiniz serbest bırakmalısınız.

Sonucu bir bağımsız değişkenin içine işaret eden fonksiyonlar (`strchr`, `strstr`) de doğru çalışır:
sonuç, bağımsız değişken serbest bırakılmadan önce kopyalanır.

`string` döndürmek üzere bildirilmiş bir fonksiyon NULL döndürürse bu bir hatadır; çünkü `string`'in
"hiçbiri yoktu" anlamına gelen bir değeri yoktur. NULL mümkünse sonucu `ptr` olarak alın.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

`:library` ile o paylaşılan kütüphane açılır ve sembol onda aranır. Olmadan sembol **sürecin kendisinde**
(libc dahil, zaten bağlı olan her şey) aranır. `sqlite3` gibi kısa bir ad sırasıyla
`libsqlite3.dylib` / `libsqlite3.so` olarak aranır ve `/` içeren bir ad yol olarak ele alınır. Açılan
kütüphaneler asla kapatılmaz: fonksiyonlarına işaret eden kod çalışmaya devam eder; bu yüzden tek doğru
ömür sürecinkidir.

#### ptr / c-long / c-ulong — ham makine sözcükleri

`ptr` opak bir işaretçidir (`void *`, `FILE *`, bildirimin ne kastettiyse). `c-long` / `c-ulong`, C'nin
`long` / `unsigned long`'udur (ayrıca `size_t`, `int64_t` ve `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Onlara `i64` / `u64` dememek bilinçlidir.** Bu dilde 64 bitlik bir tamsayı türü yoktur; çünkü etiketli
bir anlık değerin yalnızca 63 biti vardır ([2. bölüm](#2-türlerin-yazımı)). `c-long` adı "bu, bu dilin
bir tamsayısı değil, C ile sınırı geçen bir sözcüktür" der.

**Aritmetikleri yoktur.** `(+ x 1)` yazılamaz. Sağlanabilirdi, ancak sağlanmamıştır; böylece hiçbir yerde
saklanamayan ve diğer her sayıdan farklı bir genişliğe sahip bir değer üzerinde hesaplama çalışmaz; 64
bitlik tamsayı türünün dışarıda bırakılmasıyla aynı nedenle. **Yalnızca dönüşümler** vardır:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; read what came back
(as int (unsafe (c-strlen s)))               ; this one to read it exactly (int does not lose 64 bits)
(try-as i32 (unsafe (c-strlen s)))           ; ask whether it fits
(as c-ulong n)                               ; make one from another integer
```

Tamsayı **sabitleri** beklenen türü alır; bu yüzden birini geçirmek için yalnızca `as` yazmak gerekmez:

```lisp
(unsafe (c-malloc 16))                       ; 16 is read as a c-ulong
```

Aralık dışı sabitler diğer genişliklerde olduğu gibi reddedilir (`(c-malloc -1)` bir `c-ulong`'a sığmaz).

**Görünebilecekleri yerler sınırlıdır**: yalnızca bağımsız değişken türleri, dönüş türleri ve yerel
değişkenler. Aşağıdakilerin her biri hatadır:

```lisp
(defstruct handle (p ptr))          ; a struct field
(defenum maybe (none) (some ptr))   ; an enum field
(defvar (block ptr) ...)            ; a global
(defffi f ((vector ptr)) i32)       ; inside a type argument
```

Hepsinin tek bir nedeni vardır: **yuva, tuttuğu şeyi etiketler**. Etiketleme işaretçinin üst bitlerini
atardı; 64 bitlik tamsayı türünün dışarıda bırakılmasıyla aynı neden; bu yüzden `unsafe` içinde bile
izin verilmez. Bu bir izin meselesi değildir: o gösterim yoktur.

Aynı nedenle, iç içe fonksiyonlar tarafından **yakalanan** yerel değişkenler de olamazlar (yakalanan bir
bağlama bir hücreye girer ve bir hücre tuttuğu şeyi etiketler). Bu derleme zamanında bilinir ve
`(compile f)` tarafından bildirilir.

GC `ptr`'yi izlemez. Heap'in dışına işaret eder; bu yüzden bu doğrudur.

Dört şey bildirilemez:

- **Değişken sayılı bağımsız değişkenler** (`printf`). Değişken kısım sabit bağımsız değişkenlerden farklı
  kurallarla geçirilir (AArch64 Darwin'de yığında); bu yüzden sabit bir imzadan doğru çağrılamaz.
  `&rest` reddedilir.
- **Struct'ları değerle geçirmek ya da döndürmek.** Aynı nedenle (her platformun çağrı kuralına bağlıdır).
  Yazılabilen türler yukarıdaki listeyle sınırlıdır; bu yüzden hecelenemez.
- **Jenerikler.** C'de karşılığı yoktur.
- **Bir yerleşikle aynı ad.** Derlenmiş bir çağrı o adı yerleşiğe çözümlerdi; bu yüzden sessizce
  yanlış gitmek yerine reddedilir.

#### Geri çağrılar — C'nin geri çağırması

Bir bağımsız değişken türü olarak bir fonksiyon türü `(fn (types...) return-type)` yazmak, o bağımsız
değişkeni C'nin geri çağırdığı bir fonksiyon (bir geri çağrı) yapar.

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; a top-level function
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; a lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; a local function
```

Bir C fonksiyon işaretçisi bir kod adresinden başka bir şey değildir ve C onu yalnızca bildirilen
bağımsız değişkenleri geçirerek çağırır. Yakalanan değişkenleri geçirecek bir yer yoktur; bu yüzden
**yalnızca serbest değişkeni olmayan fonksiyonlar geçirilebilir** ve bu, tür denetimi sırasında
denetlenir.

- Gerçek bağımsız değişken olarak bir fonksiyon adını ya da bir `lambda` ifadesini **doğrudan** yazın.
  Bir fonksiyon tutan değişken geçirilemez: hangi fonksiyonu tuttuğu ve dolayısıyla serbest değişkenleri
  olup olmadığı çalışma zamanına kadar bilinmez.
- Bir `lambda`, dışındaki yerel değişkenlere başvuruyorsa hatadır. Global değişkenlere ve üst düzey
  fonksiyonlara başvurulabilir.
- Yerel bir fonksiyonun (`labels`), çağırdığı kardeş fonksiyonlarınkiler dahil serbest değişkeni
  olmamalıdır. Kardeş fonksiyonlar, yakalanan değişkenlerin tutulduğu yeri paylaşır; bu yüzden çağrılan
  bir kardeşin yakaladığı şeyi bu fonksiyon da yakalamış olur.
- Jenerik bir fonksiyon türlerini bildirilen fonksiyon türünden alır.
- Fonksiyon türünde yazılabilen türler yukarıdaki listeyle aynıdır. Ancak `string`, bir geri çağrının
  dönüş türü olamaz (C'ye kimsenin serbest bırakmayacağı bellek verirdi). Bir `string` bağımsız değişkeni,
  C'nin geçirdiği string'i bir typelisp string'ine kopyalar.

C fonksiyon çağrıları yalnızca `unsafe` içinde yazılabilir; bu yüzden geri çağrılar yalnızca `unsafe`
içinde geçirilebilir.

**Geri çağrı yalnızca typelisp'in çağırdığı C fonksiyonu çalışırken çağrılabilir.** Başka bir yerden
(typelisp çalıştırmayan bir thread, bir sinyal işleyicisi, `atexit` ile kaydedilmiş bir fonksiyon)
çağrılırsa nedeni yazdırır ve süreci durdurur.

**Başarısızlıklar C üzerinden yayılmaz.** Geri çağrı içindeki bir `panic` ya da `throw`, C çerçeveleri
boyunca çözülemez (bu tanımsız davranış olurdu); bu yüzden C'ye 0 döndürülür ve başarısızlık, C fonksiyonu
döndüğünde çağırana yeniden fırlatılır. Geri çağrı, başarısızlık ile C fonksiyonunun dönüşü arasında
yeniden çağrılırsa çalıştırılmaz ve 0 döndürülür.

Bir geri çağrının içinde beklemesi gereken bir işlem (boş bir kanalda `recv` vb.) hatadır
([12.6](#126-derlenmiş-kod-ve-taskler)).

Bir fonksiyon yeniden tanımlandığında yeni tanım, C'ye bir sonraki geçirilişinden itibaren çağrılır.

AOT (`compile-file`) ile de aynı şekilde çalışır. C'nin çağırdığı giriş noktaları çalıştırılabilir
dosyanın içine yerleştirilir.

**Değer olarak geçirilemezler.** Bir FFI bildirimi, `(map f xs)`'in `f`'si için olduğu gibi yazılamaz:
bir fonksiyon değeri, bir tanımın gövdesini saran bir closure'dır ve bu bildirimin sarılacak gövdesi
yoktur. Onu bir `lambda` içine sarın:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` de reddedilir: gösterilebilecek şey, bu derleyicinin üretmediği C'nin makine
kodudur. `(compile c-abs)` başarılı olur (ve zaten derlenmiş olduğundan hiçbir şey yapmaz).

**AOT (`compile-file`) ile de çalışır.** Bağlayıcı C fonksiyonlarının kendisini çözümler. Bir bildirimde
`:library` varsa o kütüphane bağlama satırına `-l` olarak eklenir (kopyalar tek bir tanede birleştirilir);
bu yüzden `compile-file` ek bağımsız değişken gerektirmez. `compile-file` kaynağı kendisi okur; bu yüzden
onları bildirimlerden toplayabilir.

Semboller derleme zamanında da aranır. Bildirilen bir fonksiyon yoksa hata, herhangi bir bağlama
hatasından önce onu adlandırır.

Standart kütüphane (prelude) `defffi` kullanmaz. Standart kütüphane her çalıştırılabilir dosyaya
bütünüyle girer; bu yüzden orada `:library` içeren bir bildirim, FFI'yi kullanmayan programlara bile o
kütüphaneyi bağlardı.

#### def-c-struct ve türlü işaretçiler — C struct'ları ayırma

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

C'dekiyle aynı düzende bir struct bildirir. Yalnızca üst düzey bir `unsafe` içinde yazılabilir (bunun
içinde `def-c-struct`'lardan başka bir şey olamaz). Adın hemen ardından bir docstring konabilir.

Alanlar için yazılabilen türler `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32` `f64` `bool`
`ptr`, türlü işaretçiler `(ptr T)` ve diğer `def-c-struct`'lardır (değerle gömülü). Düzen (her alanın
ofseti ile struct'ın boyutu ve hizalaması) C'nin kurallarıyla hesaplanır (LP64 varsayılarak). Struct'ın
kendisine işaret eden bir alan yazılabilir, ancak struct kendini gömemez.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x at 0, y at 8, size 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

Bir `def-c-struct`'ın adı tür ad alanına girer (aynı modülde aynı adlı bir `defstruct` ya da benzeri
olamaz), ancak **bir değerin türü değildir**. `(defun f ((p point)) ...)` yazamazsınız; yalnızca türlü
bir işaretçinin işaret ettiği şey olarak görünür.

**Türlü bir işaretçi `(ptr T)`**, bir `T`'ye işaret eden bir adrestir. `T`, yukarıda alanlar için
yazılabilen türlerden biridir. `ptr` gibi ham bir makine sözcüğüdür ve görünebileceği yerlerle ilgili
kuralları aynıdır (yalnızca bağımsız değişkenler, dönüş türleri ve yerel değişkenler; yalnızca `unsafe`
içinde değer olabilir).

Ayırma, okuma ve yazma aşağıdaki biçimlerle yazılır. Hepsi yalnızca `unsafe` içinde kullanılabilir.

| Biçim | Anlamı |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | `T`'den `n` değer ayırır (atlanırsa 1). İçerik 0 ile doldurulur. Bir `(ptr T)` döndürür |
| `(c-ref p i)` | `p`'den `i`'inci elemana bir işaretçi. Ayrılan aralığın dışındaysa hata |
| `(c-deref p)` / `(setf (c-deref p) v)` | `p`'nin işaret ettiği skaleri okur / yazar |
| `p::field` / `(setf p::field v)` | Bir struct'ın bir alanını okur / yazar. Gömülü bir struct olan alanı okumak adresini verir (`(ptr inner-type)`) |
| `(as ptr p)` | Türü unutur ve bir `ptr` yapar (`qsort`'un `void *`'ı gibi bir şeye geçirmek için). Geri dönüşüm yoktur |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**Ayrılan bellek, kontrol onu ayıran `unsafe`'ten çıktığında serbest bırakılır.** Sahip, aynı fonksiyon
içinde sözcüksel olarak en dıştaki `unsafe`'tir. Kod normal şekilde bitse de `panic`, `throw` ya da
`return-from` ile çıksa da serbest bırakılır. `lambda` ve `labels` fonksiyonları ayrı fonksiyonlardır;
bu yüzden içlerindeki bir `c-alloc`, kendi içlerinde kendi `unsafe`'ini gerektirir.

Bu nedenle türlü bir işaretçi, onu ayıran `unsafe`'in dışına çıkamaz. Aşağıdakilerin her biri bir tür
hatasıdır:

- Onu `unsafe` ifadesinin değeri yapmak (dolayısıyla bir fonksiyondan da döndürülemez)
- Bir closure'a yakalamak (`lambda`, `labels`)
- Onu `task` / `thread`'e geçirmek
- Onu `throw` ile fırlatmak

Değerleri `unsafe`'in dışında kullanmak için, onları `unsafe` içinde bir `defstruct`'a ya da sayılara
kopyalayın ve onları döndürün.

**C tarafında ayrılan bellek ele alınmaz.** C'den türlü işaretçiler olarak gelen değerler (`defffi` dönüş
değerleri, geri çağrı bağımsız değişkenleri, işaretçi türlü alanlardan okunan değerler), çalışma
zamanında, canlı bir `c-alloc` ayırması içinde o türden bir değere işaret edip etmedikleri bakımından
denetlenir ve etmiyorlarsa hatadır. NULL da hatadır. C'nin ayırdığı belleği ya da NULL'u almak için
türsüz `ptr` kullanın (içeriği okunamaz).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Bir geri çağrının bağımsız değişkeni denetim tarafından reddedildiğinde, tıpkı bir geri çağrı içindeki
bir başarısızlık gibi, C fonksiyonu döndüğünde çağırana bildirilir.

### 3.4 defvar / defparameter / defconstant — global değişkenler

```lisp
(defvar (name Type) init-expr)        ; initializes only if not yet bound
(defparameter (name Type) init-expr)  ; assigns every time
(defconstant (name Type) init-expr)

; with a docstring (in the same order as CL's defvar/defparameter/defconstant: after the value)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**`defvar` ile `defparameter` arasındaki fark yeniden yüklemede görünür** (CL'deki gibi). Global **zaten
bağlıysa `defvar` başlatıcıyı değerlendirmez bile**; bu yüzden bir ayar dosyasını düzenleyip yeniden
okuduğunuzda, oturumun değiştirdiği değerler olduğu gibi kalır. `defparameter` her seferinde atar; bu
yüzden yeniden okumak değerleri yazılana geri getirir.

Tür ek açıklaması zorunludur (başlatıcıdan çıkarılmaz). `defvar` değiştirilebilir; `defconstant`
değiştirilemez (`setf` bir hatadır).

### 3.5 defmethod — metot tanımları

```lisp
; instance method: can be called as (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / associated function: can be called as (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

Çağıran, metodu `obj`'nin statik türünden çözümler (tekli, statik dağıtım). Bir docstring, `defun` ile
aynı konumda ve aynı kurallarla konabilir (`where` yan tümcesinden hemen sonra, gövdenin başında,
yalnızca gövde formları izlediğinde). Aynısı `impl` içindeki metotlar için de geçerlidir;
`(documentation Type::method)` ile alınırlar.

### 3.6 defstruct — struct'lar (kullanıcı tanımlı türler)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; generic (type parameters in angle brackets)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Her alan `(name type)` ya da `(pub name type)`'dır (alan başına görünürlük, struct'ın kendi `pub`'ından
  bağımsızdır). Sondaki bir ifade daha, slot'un **varsayılan değeri** olur (`(x i32 0)`); aşağıdaki seçenek
  listesine bakın.
- Aşağıdakiler otomatik olarak üretilir:
  - Yapıcı `Name::new` (bağımsız değişkenler alan sırasıyla)
  - `instance::field-name` şekeriyle birlikte `(field-name instance)` okuyucuları
  - `(setf instance::field-name value)` şekeriyle birlikte `(set-field-name instance value)` yazıcıları
- Struct'ın kendisini `pub` yapmak için `(pub defstruct ...)` gibi önüne `pub` koyun.
- **Bir türü adlandırmadan önce tanımlayın.** Bir alanın türü struct'ın kendisi olabilir
  (`(next Option<node>)`), ancak sonradan tanımlanan bir tür olamaz: türlerin `defsignature`'a karşılık
  gelen bir ileri bildirimi yoktur. Henüz tanımlanmamış bir ad, bir `defun` bağımsız değişken türünde ya da
  `the` içinde aynı `unknown type` hatasını verir. Bu yüzden birbirine başvuran iki tür yazılamaz.
- **Tür değişkenleri yalnızca bildirim konumlarında yazılanlardır.** `defun`/`defstruct`/`defenum`/
  `deftype` için adın `<T>`'si; `defmethod` için alıcının türü (`(self box<T>)` ya da statik bir metot için
  `box<T>`); `impl` için hedef tür ve `impl<T>`; `deftrait` için `Self` ve `(type Item)`'ın ilişkili
  türleri. Başka herhangi bir yerde (bağımsız değişkenler, dönüş değeri, gövdedeki `the`/`lambda`) ilk
  kez görünen bir ad tür değişkeni olmaz; `unknown type`'tır.
- **Docstring'ler**: adın hemen ardından, alanlardan önce gelen bir string sabiti docstring olur
  (`(defstruct Name "doc" (field Type)...)`, CL'nin `defstruct`'ıyla aynı konum). Bir alan her zaman
  `(name Type ...)` biçimindedir ve asla çıplak bir string olamaz; bu yüzden belirsizlik yoktur.
  `(documentation Name)` ile alın.

#### Seçenek listesi

Ad konumuna bir liste `(Name option...)` yazmak seçenekleri belirtir (CL ile aynı konum).

```lisp
(defstruct (point (:constructor make-point)          ; keyword constructor
                  (:constructor at (x &optional y))  ; BOA constructor
                  (:copier copy-point))
  (x i32 0)          ; a third element is that slot's default value
  (y i32 0))

(point::make-point :y 7)   ; x is 0
(point::at 1)              ; y is 0
(point::at 1 2)
(copy-point p)             ; a shallow copy (the same as CL's copier)
```

- **`:constructor`**: üretilen şey, türün bir **statik fonksiyonudur** (`point::make-point`) ve gövdesi
  her zaman `(point::new ...)`'dir. `new`, tek yapısal yapıcı olarak kalır; burada yapılan şey onu
  çağırmanın bir *yoludur*. Birkaç tane bildirilebilir.
  - `(:constructor name)` her slot'u `&key` olarak alır. **Her slot'un bir varsayılanı gerekir** (bu dilde
    CL'nin "bağlanmamış slot"una karşılık gelen bir şey yoktur).
  - `(:constructor name (slot...))` adı verilen slot'ları konumsal bağımsız değişkenler olarak alır (her
    sırada). Adı verilmeyen slot'lar varsayılanlarıyla doldurulur; bu yüzden **varsayılanları gerekir**.
    `&optional`'dan sonra geri kalanlar atlanabilir (ve aynı şekilde varsayılan gerektirir).
- **`:copier`**: aynı slot değerlerine sahip yeni bir değer döndüren bir **örnek metodu** üretir. CL'nin
  kopyalayıcısı gibi sığdır.
- **`:include Parent`**: ebeveynin slot'larını başa ekler (varsayılanlar da miras alınır; ebeveyn başka bir
  dosyada olabilir). **Hiçbir tür ilişkisi yaratmaz**: çocuk ebeveynin bir alt türü değildir, ebeveynin
  metotları çocuğa uygulanmaz ve ikisini bağlayan bir çalışma zamanı sınaması yoktur. Bu dilde alt
  tipleme yoktur; ortak arayüzler `deftrait`'in işidir. Yalnızca slot'ların *listesi* birleştirilir.
- **Slot varsayılanları yalnızca üretilen yapıcılar tarafından okunur.** Hiçbir `:constructor`
  bildirmeden bir varsayılan yazmak hatadır; çünkü asla kullanılamazdı.
- Dışarıda bırakılan seçenekler ve nedenleri:
  - **`:conc-name`**: CL'de tek bir düz fonksiyon ad alanındaki çakışmaları önlemek için erişimcilere
    önek koyar. Burada erişimciler alıcının türüne göre dağıtılan metotlardır; bu yüzden çakışma olmaz ve
    bir önek `instance::field`'ı (yalnızca slot adını bilen) bozardı.
  - **`:predicate`**: çalışma zamanında "bu değer bir `point` mi?" sorusunu yanıtlar. Burada türler,
    çalışma zamanı tanığı olmayan bir derleme zamanı sınıflandırmasıdır ve "bir point olabilecek bilinmeyen
    türde bir değer"in var olduğu bir konum yoktur (`Sexpr` üzerindeki `match` mühürlüdür ve `:dyn` aşağı
    dönüştürülemez); bu yüzden üretilen bir yüklem yalnızca `true` döndürebilirdi.
  - **`:type` / `:initial-offset` / `:named`**: bunlar değerin gösterimini bir liste ya da vektörle
    değiştirir. Gösterim derleyiciye aittir ve dilden gözlemlenemez.

### 3.7 defenum — enum'lar (toplam türler)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; a variant with a payload (positional fields)
  (Variant2)                  ; a variant without a payload
  ...)

; generic
(defenum Option<T>
  (Some T)
  (None))
```

- Her varyant `(VariantName FieldType...)` biçimindedir. Alanlar yalnızca konumsaldır (adları yoktur). En az
  bir varyant gerekir ve adlar tekrarlanamaz.
- Değerler, yerleşik `Option`/`Result` gibi, nitelikli ya da `use` aracılığıyla oluşturulur:
  `(Name::Variant1 a b)` ya da `(use Name)`'den sonra `(Variant1 a b)`.
- `match` / `if-let` ile ayrıştırılabilirler. `match` kapsamlılığı denetler (her varyantı kapsamalı ya da
  bir `_` içermelidir):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Metotlar ve ilişkili fonksiyonlar, `defstruct`'ta olduğu gibi, sonradan `defmethod`/`impl` ile eklenir.
- Enum'un kendisini `pub` yapmak için `(pub defenum ...)` yazın.
- **Docstring'ler**: `defstruct` ile aynı konum ve kurallar, adın hemen ardından, varyantlardan önce
  (`(defenum Name "doc" (Variant ...)...)`). `(documentation Name)` ile alın.

### 3.8 deftype — tür takma adları

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

CL'nin `deftype`'ı, statik tür denetimli bir dilde anlamlı olana daraltılmış: **bir türün yazımı, bir tür
değil**.

- Ad konumu `defun` ile aynıdır ve jenerik bağımsız değişkenler `Name<T,U>` olarak yazılır. Kullanım
  yerinde tam olarak bildirilen sayıda tür bağımsız değişkeni gerekir (çok ya da az olması orada hatadır).
- Açılım **tür ayrıştırıcısının içinde** olur. Bu yüzden aşağı akıştaki hiçbir şey takma adın var olduğunu
  bilmez: tekilleştirme (monomorphization) anahtarları, dump'lar, derleme yolu ve **hata mesajlarının**
  hepsi açılmış biçimi gösterir. `(f "x")`, `meters` gerektiren bir fonksiyona karşı başarısız olursa
  mesaj `i32` der.
- **Yeni bir tür değildir.** `(deftype meters i32)`, `meters` ile `i32`'yi aynı tür yapar; bu yüzden
  karıştırılmaları yakalanmaz. Ayrı tutmak istiyorsanız `defstruct` kullanın.
- **Bir yüklem değildir.** CL'nin `(deftype small () '(integer 0 9))`'u, `typep`'in çalışma zamanında
  sınadığı bir *değer kümesini* betimler, ancak burada türler çalışma zamanı tanığı olmayan bir derleme
  zamanı sınıflandırmasıdır; bu yüzden değerleri kısıtlayan bir takma adın kısıtlayacağı bir şey olmazdı.
- **Kendini içeremez.** Bir takma ad yazıldığı yerde açılır; bu yüzden özyineleyeceği bir yer yoktur.
  Özyinelemeli veri türleri `defstruct`/`defenum` ile yazılır.
- Türler ve trait'lerle ad alanını paylaşır (tek bir modül içinde bir `defstruct`/`defenum`/`deftrait` ile
  aynı ada sahip olamaz). `(pub deftype ...)` ile genel yapın ve `(use m::meters)` ile getirin.
- **Docstring'ler**: adın hemen ardından, türden önce (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — trait'ler

```lisp
(deftrait TraitName (SuperTrait...)      ; the supertrait list is required; () if none
  (type AssocName)                       ; associated types (any number, optional)
  (method-name ((self Self) params...) RetType)          ; no body = must be implemented
  (method-name ((self Self) params...) RetType body...)) ; with a body = default implementation

(impl TraitName TargetType
  (where (Trait A)...)                   ; bounds applying to the whole impl (optional)
  (type AssocName ConcreteType)          ; makes an associated type concrete
  (method-name (recv params...) RetType body...))
```

`impl` aracılığıyla her metot, `TargetType`'ın sıradan bir `defmethod`'u olarak kaydedilir. Trait'lere,
jenerik fonksiyonların `where` yan tümcelerinde trait sınırları olarak başvurulur
([3.1 defun](#31-defun--fonksiyon-tanımları)'a bakın). Bir trait adı `m::Trait` gibi bir `::` yolu da
olabilir.

**Üst trait listesi (zorunlu)**: her zaman trait adının hemen ardından yazılır. Her eleman çıplak bir
trait adıdır ya da o trait'in ilişkili türleri varsa, **tüm ilişkili türleri sabitlenmiş** olarak
`(Trait (Assoc Type))`'tir.

```lisp
(deftrait Eq () ...)                       ; no supertraits
(deftrait Ord (Eq) ...)                    ; Rust's trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; pinning an associated type
  (rewind ((self Self)) ()))
```

Mirasın üç etkisi vardır. (1) `impl Ord X`, `impl Eq X`'in **önce** yazılmasını gerektirir (yazma sırasıyla
ilgili bir kural: REPL'de ve adım adım `load` ile belirlenimci olarak karara bağlanabilen tek biçim ve
Rust'tan daha katı). (2) Yalnızca `(where (Ord T))` bile `Eq`'in metotlarını çağırmanıza izin verir. (3)
`Eq`'in metotları bir `:dyn Ord` üzerinden çağrılabilir ve bir `:dyn Ord` değeri, bir `:dyn Eq`'in
gerektiği yere olduğu gibi geçirilebilir (yukarı dönüşüm). Bir alt trait'in ebeveyniyle aynı adlı bir
metodu yeniden bildirmesi ve iki ebeveynden aynı adlı metotlar miras alması, ikisi de hatadır (bir
vtable'ın ad başına bir yuvası vardır). Elmas miras tek bir yuvada birleşir.

**Varsayılan gerçekleştirmeler**: imzadan sonraki bir gövde, bir `impl` metodu atladığında kullanılır.
Gövde, trait'in yazıldığı **modülün ad alanında** çözümlenir; bu yüzden o modülün genel olmayan
fonksiyonlarını çağırabilir. Gövdeli metotlar `where` yan tümceleri ve docstring'ler de taşıyabilir.
Gövde, Rust'taki gibi `Self` bir tür değişkeni olarak (`Self: trait'in kendisi` ile sınırlı) bırakılarak,
**bildirim noktasında bir kez** tür denetiminden geçer: her `impl` ve her gerçekleştiren tür için
başarısız olacak hatalar, hiçbir `impl`'in asla atlamadığı varsayılanlarda bile orada yakalanır.
`self` üzerinde trait'in kendisinin ya da üst trait'lerinin metotlarına yapılan çağrılar bu sınırdan
geçer ve ilişkili türler kendilerine sabitlenir; bu yüzden `Item` döndüren bir imza, somut türü bilmeden
gövdeyle eşleştirilir.

**Blanket gerçekleştirmeler**: hedefi bir tür değişkeni yapmak, trait'i sınırları karşılayan her tür için
bir anda gerçekleştirir.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; no body at all; everything is the default
```

**Somut bir tür onu gerçekten kullanana kadar hiçbir kod üretilmez** (tür başına bir kez, sıradan
tekilleştirmeyle aynı mekanizmayla). Bir trait'in en fazla bir blanket gerçekleştirmesi olabilir. Bir
türün açık bir `impl`'i varsa o öncelik alır. Gövdenin tür denetimi üretimden ayrıdır: bildirim noktasında
bir kez, **hedef bir tür değişkeni olarak bırakılarak** (Rust'taki gibi) yapılır; bu yüzden hiç
kullanılmayan bir gerçekleştirmenin bile, bildirilen sınırlar altında her hedef için başarısız olacak
hataları orada yakalanır. Sınırların haklı kıldığı çağrılar (`(where (Ord T))` altında `(less self other)`
vb.), jenerik bir `defun`'un gövdesinde olduğu gibi geçer.

**Docstring'ler**: bir `deftrait`, üst trait listesinin hemen ardından, öğelerden önce bir string sabiti
olarak tüm trait için bir docstring'e sahip olabilir (`(deftrait Name () "doc" (type ...) (method ...)...)`).
Gövdesiz bir imzanın docstring'i olamaz: sondaki bir string, bir varsayılan gerçekleştirmenin dönüş değeri
olurdu; bu yüzden ikisi ayırt edilemezdi.

Standart kütüphanenin sağladığı trait'ler: **`Iter`** (`next` / ilişkili tür `Item`; `doiter` ve dizi
fonksiyonlarının temeli), **`Eq`** (`equals`; `not-equals` bir varsayılan gerçekleştirmedir), **`Ord`**
(`Eq`'ten miras alır; yalnızca `less` gerçekleştirilmelidir ve `less-equal` / `greater` /
`greater-equal` varsayılan gerçekleştirmelerdir), **`Error`** (`message` / `source`; hata türlerini
tekdüze ele almak için `:dyn Error`), **`print-object`** (türe göre yazdırılan gösterim), **`Pathish`**
(yol adı belirleyicileri: bir string ya da bir `pathname`) ve akış hiyerarşisi **`Stream`** →
**`InputStream`** / **`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**. Hangi
türlerin hangi trait'leri gerçekleştirdiği [types.md](types.md) belgesindedir; her trait'in metotları
[Standart Trait'ler](functions/traits.md), [Hata türleri](functions/option-result.md#3-hata-türleri-ve-error-traiti),
[print-object](functions/printing.md#5-print-object-türe-göre-yazdırılan-gösterim) ve
[Akışlar](functions/streams-files.md) belgelerindedir. Kendi koleksiyon türünüz için `Iter`'i `impl`
ederseniz `doiter` (5. bölüm) ve `map` / `filter` / `sort` ve benzerleri onun üzerinde olduğu gibi çalışır.

Trait çağrıları varsayılan olarak **statiktir** (alıcının statik türüyle çözümlenir). Somut türü
çalışma zamanında belirlenen değerleri ele almak için trait nesnesi türü `:dyn Trait` (2. bölüm), bir
vtable üzerinden dinamik dağıtım verir:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; one call site, an answer per implementation
```

Yalnızca "her metodun bir `self` alıcısı vardır, alıcı dışında hiçbir yerde `Self` kullanmaz ve kendisi ne
jenerik ne de değişken sayılıdır" olan trait'ler `:dyn` yapılabilir (miras alınan metotlar aynı koşulları
karşılamalıdır).

Yalnızca değerlerinin heap'te bir gösterimi olan türler bir `:dyn` kutusuna girebilir:

| Girebilir | Giremez |
|---|---|
| `defstruct` / `defenum` türleri (`Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` ve standart kütüphanenin struct'ları dahil), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Sabit genişlikli tamsayılar (`i8`'den `u32`'ye), `f32`, `bool`, `char`, `symbol`, `()`, fonksiyon türleri ve kutusuz `Option<T>` ([Option'ın çalışma zamanı gösterimi](functions/option-result.md#2-optiontnin-çalışma-zamanı-gösterimi)) |

Giremeyen bir türün değerini bir `:dyn` beklenen yere koymak tür hatasıdır. Bu tür değerleri `:dyn`
üzerinden ele almak için onları `(defstruct flag (v bool))` gibi bir struct içine sarın.

### 3.10 module / use — ad alanları

```lisp
(module path body...)      ; path is a sequence of segments such as foo or foo::bar
(in-module path)           ; from here to the end of this unit, inside path (the flat form of module)
(use path...)              ; alias functions, types and modules into the current namespace
(import path...)           ; the same as use (a CL-compatible spelling)
(shadowing-import path...) ; a use that knowingly takes a bare name already in use
```

- `module` bir ad alanı oluşturur. **Türler ad alanı değildir** (Rust'taki gibi bir türün yalnızca ilişkili
  fonksiyonları ve metotları vardır).
- Bir türü `use` etmek, onun yapıcılarını ve genel statik metotlarını da çıplak adla kullanılabilir kılar
  (örneğin `(use option)`'dan sonra `some`/`none`, `option::some`/`option::none` olmadan çağrılabilir).
- Çıplak adların (niteliksiz tanımlayıcılar) çözümleme sırası: özel formlar → yapıcılar → serbest
  fonksiyonlar (geçerli ad alanı → kök) → örnek metotları (ilk bağımsız değişkenin statik türüyle
  çözümlenir). Aradaki ebeveyn modüller üzerinden yukarı çıkmaz.
- Nitelikli bir yol `a::b`, `a`'yı yukarıdaki sırayla çözümler; bir modülse içine girer, bir türse son
  bölüm ilişkili bir öğe olarak çözümlenir.
- **`use`, kendisinden sonraki formları etkiler.** Bir dosya her seferinde bir form okunur ve bağımlılıklar
  formun denetlenmesinden hemen önce çözümlenir; bu yüzden `m::f`'yi `(use m)`'in **üstüne** yazmak
  `unresolved path` verir. `use`'u dosyanın başına koyun.
- **`use` birkaç yol alabilir** (`(use a::f b::g)`). `import`, aynı davranışa sahip CL uyumlu bir
  yazımdır.
- **Çıplak adı zaten alınmış bir `use` bildirilir.** Bir çıplak adı çözümlemek, takma adlardan önce
  modülün kendi tanımlarına bakar; bu yüzden `(defun twice ...)`'dan sonraki `(use m::twice)` **hiçbir
  şey yapmaz**. Kastettiyseniz `shadowing-import` yazın (yine de bir tanımı yenemez, çünkü birini
  kaldırmanın yolu yoktur; yalnızca daha önceki takma adları yener).
- **`in-module`, `(module path body...)`'ın düz biçimidir.** `(in-module geometry)` yazmak, oradan
  birimin sonuna kadar (dosya ya da çevreleyen `module`'ün gövdesi) her şeyi `geometry` içine koyar.
  Dosyanın kendi modülünün **içine** gider (`main.typl` için `main::geometry`). Art arda iki tane sırayla
  iç içe geçer. CL'nin `in-package`'ından farklıdır ve farklı adlandırılmıştır: bu sistemde dosya zaten
  bir modüldür; bu yüzden "seçilecek" bir şey yoktur ve bir formun yapabileceği tek şey iç içe geçmektir.

### 3.11 Dosyalar ve modüller (çok dosyalı projeler)

Kaynak köküne göre dosya yolu, modül yoludur:
`<root>/geo/point.typl` içeriği örtük olarak `geo::point` modülüne sarılır (bir dizin de Rust / Python
tarzında bir bölümdür). Dosyadaki açık bir `(module bar ...)` onun **içine** (`geo::point::bar`) yerleşir;
bu yüzden türetilen yol ile açık bir bildirim asla çakışmaz.

- **Kaynak kökü**: projenin köküne bir manifest dosyası `typelisp.toml` koyun (boş olabilir; isteğe bağlı
  olarak tek bir `src = "src"` satırı kaynak dizinini adlandırır). Hedef dosyanın dizininden başlayarak
  yukarı çıkılarak bulunur. Manifest yoksa giriş dosyasının dizini (REPL için geçerli dizin) köktür.
- **İsteğe bağlı yükleme**: `(use geo::point)` henüz yüklenmemiş bir modüle başvurduğunda eşleşen dosya
  (`geo/point.typl`) otomatik olarak yüklenir, türleri denetlenir ve kaydedilir. `use a::b::c` en uzun
  öneki önce arar: `a/b/c.typl` → `a/b.typl` → `a.typl` (çünkü `c`, bir modülün içindeki bir öğe
  olabilir). Diğer modüllerden görünen tanımlar `pub` gerektirir ([3.13 pub](#313-pub--görünürlük)).
- **Döngüsel başvurular hatadır**: zincir `circular module dependency: a -> b -> a` biçiminde bildirilir.
- **Çalıştırma**: `typl <file.typl>` bir dosyayı çalıştırır (bağımsız değişkensiz REPL). REPL'deki `use`
  dosyaları aynı kurallarla çözümler.
- **Cons arena kapasitesi**: `typl --heap-cells N`, cons hücresi arenasının **başlangıç kapasitesini**
  ayarlar (varsayılan 65536; `--heap-cells=N` biçimi de hem dosya çalıştırma hem REPL için çalışır). Arena
  yetmediğinde **daha fazla ekleyerek büyür**. Büyümenin sınırı başlangıç kapasitesinin 256 katıdır ve onu
  aşan bir ayırma `heap exhausted` verir: başlangıç kapasitesi "başta bu kadar ayır" demektir ve sınır
  "bunun ötesini sızıntı say" demektir.

### 3.12 load — düz yükleme

```lisp
(load "path")   ; top level only; path is a string literal
```

- CL tarzı **düz yükleme**: hedef dosyanın formlarını (`use`'tan farklı olarak onları bir modüle
  sarmadan) olduğu gibi **geçerli ad alanına** okur. Yalnızca üst düzeyde (bir fonksiyon gövdesi içinde
  tür hatasıdır).
- `path`, yükleyen dosyanın dizinine göredir (REPL'den sürecin cwd'sine göre). Uzantısı yoksa `.typl`
  eklenir.
- Yüklenen dosyadaki `(load ...)`/`(use ...)` de özyinelemeli olarak işlenir.
- **Her seferinde bir form okur ve onu anında çalıştırır** (CL'nin `load`'unun yaptığı gibi). *k* formu,
  *k+1* okunmadan önce çalışmayı bitirmiştir: ortada bir sözdizimi ya da tür hatası olsa bile, ondan önceki
  formlar zaten çalışmıştır. `use` ile yüklenen modül dosyaları farklıdır: bir birim olarak denetlenirler
  ve çalıştırılmaları, onları `use` edene bırakılır (CL'nin `compile-file`'ına karşılık gelir).

### 3.13 pub — görünürlük

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` yalnızca yukarıdaki on bir türe konabilir (`module`/`use`/`deftrait`/`impl`'e değil). Tanım
anahtar sözcüğüyle `pub`'ın hemen ardından yazılır; tanımı parantezlere saran `(pub (defun ...))`
biçiminde değil. Bir `pub`, tam olarak bir tanımı genel yapar (birkaç tanım bir seferde
işaretlenemez).

### 3.14 defmacro — makro tanımları

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- Tüm parametreler ve dönüş değeri her zaman `Sexpr`'dir; bu yüzden tür ek açıklaması yazılmaz.
- CL tarzı hijyenik olmayan makrolar (`gensym` ile çakışmaları önlemek makro yazarının sorumluluğundadır).
- Lambda listesi CL'nin `required &optional &rest &key` sırasındadır (her işaretçi en fazla bir kez ve
  yalnızca bu sırayla).
  - `&optional` … isteğe bağlı bağımsız değişkenler. `name` ya da `(name default-expr)`. Varsayılan ifade
    açılım zamanında değerlendirilir (daha önce bağlanan parametrelere başvurabilir) ve bağımsız değişken
    atlandığında bağlanır (varsayılan yoksa boş liste `()`).
  - `&rest name` … kalan konumsal bağımsız değişkenleri tek bir `Sexpr` listesi olarak birlikte alır.
  - `&key` … anahtar sözcüklü bağımsız değişkenler. `name` ya da `(name default-expr)`. Çağıran bunları
    `:name value` olarak geçirir (her sırada). Atlandığında varsayılan ifade (yoksa boş liste `()`).
    Bilinmeyen anahtar sözcükler ya da tek uzunluklu bir `:key` dizisi hatadır.
- Örnekler: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — yerel makro bağlamaları

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; lexically scoped macros
(symbol-macrolet ((name expansion) ...) body...)         ; a name stands for a form
```

İkisi de **ifade** özel formlarıdır ve çalışma zamanında hiçbir şey kalmaz (derlenen şey gövdenin açılmış
biçimidir). Lambda listesi `defmacro` ile aynıdır. Ayrıntılı kurallar ve örnekler
[Yerel makro bağlamaları](functions/system.md#9-yerel-makro-bağlamaları-macrolet--symbol-macrolet)
bölümündedir.

## 4. Bağlama ve koşullar

```lisp
(let ((name val) ...) body...)      ; parallel binding
(let* ((name val) ...) body...)     ; sequential binding (earlier bindings usable in later initializers)

(if cond then else)                 ; else is required (always three elements)
(when cond body...)                 ; an if without else (Unit type). defmacro
(unless cond body...)               ; the negation of when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; a list of keys: matches if any of them does
  (else body...))                   ; expr is evaluated once. keys are compared with equal.
                                     ; keys are "literals" and are not evaluated (as in CL).
                                     ; a bare symbol a means the symbol 'a.
                                     ; writing 'a is an error (use the bare a). defmacro
(ecase expr (key body...) ...)      ; a case requiring a match. panics if nothing matches. defmacro
(ccase expr (key body...) ...)      ; CL's ccase. there are no restarts to offer, so it is the same as ecase. defmacro
(and expr...)                       ; short-circuit evaluation. true with zero arguments. defmacro
(or expr...)                        ; short-circuit evaluation. false with zero arguments. defmacro
(progn body...)                     ; runs in order and returns the last value
(unsafe body...)                    ; the same as progn, plus permission to write FFI calls
                                     ; and raw words. see 3.3 defffi
(prog1 form more...)                ; evaluates everything; the value is that of form. defmacro
(prog2 a b more...)                 ; evaluates everything; the value is that of b. defmacro
(the Type expr)                     ; a type annotation (no run-time effect)
```

### 4.1 unsafe — denetlenemeyen varsayımları üstlenme

```lisp
(unsafe body...)
```

`progn` ile aynıdır: gövdeyi sırayla değerlendirir ve son değeri döndürür. Kapsam oluşturmaz ve bir
fonksiyon sınırı değildir (`break` / `return-from` doğrudan dışarıya geçer). Fark, bazı şeylerin yalnızca
içinde yazılabilmesidir.

Şu anda üç şey `unsafe` gerektirir: [defffi](#33-defffi--c-fonksiyonlarını-bildirme-ffi) ile bildirilen C
fonksiyonlarını çağırmak, ham makine sözcüklerini (`ptr` / `c-long` / `c-ulong` / `(ptr T)`) değer yapmak
ve [`def-c-struct` ile `c-alloc`](#def-c-struct-ve-türlü-işaretçiler--c-structları-ayırma).

`c-alloc` ile ayrılan bellek, aynı fonksiyon içindeki en dıştaki `unsafe`'ten çıkıldığında serbest
bırakılır. `progn`'dan farklı olarak yalnızca o `unsafe`'in çıkışta yapacak işi vardır: serbest bırakma.

`unsafe`'in üstlendikleri, derleyicinin doğrulayamadığı şu varsayımlardır:

- **Türlerin eşleştiği.** Bildirilen C imzasının gerçek olanla eşleştiği. Eşleşmezse bağımsız değişkenler
  yanlış yazmaçlara girer ve dönüş değerleri yanlış genişlikte okunur.
- **Bellek güvenliği.** C tarafının kendisine verilenle yaptığı şey.
- **Süreç genelindeki durum.** Ortam değişkenleri, sinyal işleyicileri, `errno`. Örneğin FFI üzerinden
  `setenv` çağırmak, bu gerçekleştirimin `decode-universal-time`'ının yerel zamanı hesaplarken yaptığı
  varsayımları bozar.
- **Thread güvenliği.**

Tür denetiminden bir çıkış yolu değildir. `(unsafe (+ 1 "two"))` geçmez. İzin verilen şey, belirli
**işlemleri** yazmaktır; saçmalık yazmak değil.

Sözcüksel olarak çalışır. `unsafe` içine yazılan bir `lambda`'nın gövdesi izni miras alır (Rust'ın
`unsafe` bloklarındaki closure'larda olduğu gibi). Değer daha sonra `unsafe`'in dışından çağrılabilir,
ancak onu orada yazmak, sorumluluğu kabul etmek sayılır.

### 4.2 destructuring-bind — listeleri biçimlerine göre ayrıştırma

```lisp
(destructuring-bind lambda-list form body...)
```

`form`'un ürettiği listeyi **biçimine göre** ayrıştırır ve bağlar. Lambda listesi `defmacro`'nunkidir
(zorunlu → `&optional` → `&rest`/`&body` → `&key`, her biri varsayılan ifadeli); CL'nin ikisi arasında
tek bir tane paylaşmasıyla aynı nedenle: bunlar aynı şeyi ayrıştıran iki formdur.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Bağlanan her değişken bir `Option<Sexpr>`'dir.** Bu, gerçekleştirimin bir kısıtlaması değil, bağlanan
  şeyin doğasıdır: bu dildeki tek listeler S-ifade listeleridir; bu yüzden elemanlara verilecek başka
  bir tür yoktur. Bir skalerin gerektiği yerde `match`'e dönmek, bir `defmacro` gövdesindeki ile aynıdır.
- **Eşleşmeyen bir biçim panic olur** (CL'nin hatasına karşılık gelir): çok az ya da çok fazla eleman,
  tek uzunluklu bir `&key` dizisi ya da bilinmeyen bir anahtar sözcük. `sexpr-car`, `()` için `()`
  döndüren esnek bir fonksiyondur; bu yüzden denetim olmadan kısa bir liste sessizce boş bir diziye
  bağlanırdı.
- **İç içe lambda listeleri desteklenmez.** `defmacro` da bunları almaz; bu yüzden tek bir kural vardır.
  `(a (b c))` bir alt listeyi sessizce `b`'ye bağlamaz; bunu söyleyen bir hatadır.
- `&optional` / `&key`'in varsayılan ifadeleri **yalnızca kullanıldıklarında değerlendirilir** (CL'deki
  gibi).
- CL'nin `&allow-other-keys`'ine karşılık gelen bir şey yoktur (`defmacro`'da da yoktur).

### 4.3 match — örüntü eşleme

```lisp
(match expr
  (pattern body...)
  ...)
```

Örüntü türleri:
- `_` — joker
- Bir değişken adı — bir bağlama örüntüsü (her zaman eşleşir). Ancak denetlenen değerin türünde o adda bir
  varyant varsa **aşağıdaki çıplak varyant adı örüntüsü** olarak çözümlenir
- Bir çıplak varyant adı — bağımsız değişken almayan bir varyantla eşleşir (`(match c (red 1) (blue 2))`).
  Alanlı bir varyantı çıplak adıyla yazmak bir arity hatasıdır; bu yüzden onu `(circle r)` gibi
  parantez içinde yazın
- **Anlık sabitler**: tamsayılar / `true`/`false` / karakterler — sözcük olarak karşılaştırılır
- **Değer sabitleri**: string'ler / kayan noktalı sayılar / semboller (`'foo`) / bignum tamsayılar /
  oranlar — o türün `Eq::equals`'ıyla değere göre karşılaştırılır
  ([Standart Trait'ler](functions/traits.md#2-eq--ord-karşılaştırma)). String'ler kimliğe değil, içeriğe
  göre karşılaştırılır
- `(= expr)` — herhangi bir ifadeyi değerlendirir ve `Eq::equals` ile karşılaştırır. Sabit sözdizimi
  olmayan türleri (`defstruct` örnekleri, globaller, hesaplanan sonuçlar) karşılaştırmanın tek yolu ve
  kullanıcı tanımlı bir `Eq` gerçekleştirmesi olduğu gibi karşılaştırma kuralı olur. `expr`, kolun
  konumundan görünen her şeye (bağımsız değişkenler, dış bağlamalar, globaller) başvurabilir
- `(Ctor sub-pattern...)` — yapıcı örüntüleri (`Some x` `None` `Cons a d` `Ok v` vb.)

`Eq`'i gerçekleştirmeyen bir türü bir değer sabitiyle / `(= expr)` ile karşılaştırmak tür hatasıdır (bu
dil, sessizce asla eşleşmeyen bir kolu bırakmak yerine "bunlar karşılaştırılamaz" demeyi seçer).

**Bir `Sexpr` denetlenen değerine karşı değer sabitleri**: `sexpr`'in `Eq`'i `eq`'tir (CL'nin kimliği);
bu yüzden anlık değerler (`'foo` (intern edilmiş) / tamsayılar / karakterler / `true`/`false`) olduğu
gibi yazılabilir ve içeriğe göre eşleşir:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Anlık olmayan sabitler (string'ler / kayan noktalı sayılar / bignum tamsayılar / oranlar) bir `Sexpr`'e
karşı **yazılamaz**. Bunların `eq`'i nesne kimliğini karşılaştırır; bu da "tür denetiminden geçen ama asla
eşleşmeyen bir kol" yapardı; bu yüzden varyant örüntüsünü adlandıran bir hatadır: `(str "hi")` yazın;
bir `string`'e ayrıştırılır ve içeriğe göre karşılaştırılır. `(= expr)` açıkça `equals` ister; bu yüzden
bu kısıtlama ona uygulanmaz.

**Denetlenen değerin bir ADT olması gerekmez.** `string`/`symbol`/`i32`/`f64` ve benzerleri doğrudan
eşleştirilebilir (string sabiti örüntülerinin gittiği yer burasıdır). Ancak varyantı olmayan bir tür
sayımla kapsanamaz; bu yüzden `_` (ya da joker olarak davranan bir bağlama örüntüsü) gereklidir:

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; a type without variants needs `_`
```

Bir `Sexpr` denetlenen değerine karşı, yukarıdaki 16 yerleşik varyant örüntüsünün yanı sıra **aşağı
dönüşüm örüntüleri** (kullanıcı tanımlı ADT'lerin örneklerini çıkarma) yazılabilir:
`(list p 42)` gibi bir `Sexpr`'e örtük olarak dönüştürülmüş bir `defstruct`/`defenum` (3. bölüm)
örneğini `match` ile geri almak için sözdizimi:

- `(TypeName sub-pattern...)` — **tür adı** önde olmak üzere alan ayrıştırma (yalnızca struct'lar: bir
  `defstruct`'ın her zaman tek bir varyantı vardır; bu yüzden bir varyant adıyla değil, tür adıyla
  yazılır). Örneğin `(defstruct point (x f64) (y f64))` için `(point x y)`.
- Bir çıplak varyant adı `(VariantName sub-pattern...)` — bir `defenum`'un bir varyantını çıkarır.
  `(use EnumType)`'tan sonra görünen çıplak bir ad olarak çözümlenir (yapıcıyı çağırırken geçerli olan
  aynı görünürlük kuralları). Örneğin `(defenum color (red) (blue))` için `(use color)`'dan sonra `(red)`
  `(blue)`. Görünen birkaç enum'un varyant adları çakışırsa bir belirsizlik hatasıdır; bu yüzden nitelikli
  biçim `(EnumType::VariantName ...)` de yazılabilir (`use` gerekmez).
- `(the Type pattern)` — tüm türün aşağı dönüşümü (bütün olarak bağlama). Alanları ayrıştırmaz; değeri
  olduğu gibi `pattern`'e geçirir. Kimliğini koruyarak değiştirilebilir bir struct'ı çıkarmanın tek yolu
  ve ayrıca bir `Vector<T>`/`HashTable<K,V>`'yi bir `Sexpr`'ten çıkarmanın da tek yolu (alan ayrıştırma
  biçimleri yoktur). Örneğin `(the point p)`'den sonra `(setf p::x 9)`, listedeki özgün örneğe de
  yansır.

**`Option<Sexpr>` için örüntüler**: S-ifade verisinin türü `Sexpr` değil, `Option<Sexpr>`'dir ve boş
liste `Sexpr`'in bir varyantı değil, `Option`'ın `none`'dır. Bu yüzden bir `Option<Sexpr>`'i eşleştirirken
`Sexpr`'in 16 varyantı ve `none` **aynı kol listesinde düz olarak** yazılabilir (`Option`'ı soyacak dış
bir `match` gerekmez):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; the empty list
    (_          9)))
```

Kapsamlılık aynı düz evrende denetlenir: `Sexpr`'in 16 varyantı artı `none`, toplam 17. `_` yoksa
`(none)`'ı unutmak hatadır. `(some x)` de yazılabilir ve "boş olmayan bir şeyi" bağlar.

Bu şeker **tam olarak** yalnızca `Option<Sexpr>`'e uygulanır. `Option<Option<Sexpr>>` için `(int n)`'in
hangi katmanı soyduğu belirsiz olurdu; bu yüzden olağan şekilde iki düzey `match` yazın.

Aynı aşağı dönüşüm örüntüleri, **bir trait nesnesi (`:dyn Trait`, 2. bölüm) denetlenen değerinde** olduğu
gibi kullanılabilir: `match` onu kutudan çıkarır ve sonra yukarıdaki `Sexpr` örüntü mekanizmasına verir;
bu yüzden ek sözdizimi yoktur. Gerçekleştiren türler kümesi açıktır; bu yüzden asla kapsamlı olamaz ve
`_` gereklidir:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; field decomposition with the type name first
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Kollar arası tür çıkarımı**: tüm kolların türü aynı olmalıdır (`panic` gibi sapan kollar hariç). Hiçbir
türün beklenmediği yere yazılan bir `match` içinde kollar birbirlerinin eksik tür bağımsız değişkenlerini
doldurur: `(result::ok v)` yalnızca `T`'yi, `(result::err e)` yalnızca `E`'yi sabitler, ancak birlikte
`Result<T,E>`'yi sabitlerler. Sonuna kadar hiçbir kolun sabitleyemediği bir tür bağımsız değişkeni, o
kolun hatasıdır (`cannot infer type argument ...`). `match` dışında, sabitlenemeyen bir tür bağımsız
değişkeni orada hatadır.

Aşağı dönüşüm örüntüleri kullanan bir `match`'in kapsamlılık denetimi, bunları `Sexpr`'in kendi
varyantlarının kapsamına saymaz (yalnızca aşağı dönüşüm örüntüleri listeleyen bir `match` `_` ile
kapatılmalıdır). Jenerik ADT'ler (`defstruct point<T> ...` vb.) için bir aşağı dönüşüm örüntüsünün tür
bağımsız değişkenleri çıkarılamaz; bu yüzden alan ayrıştırma biçimi (`(point ...)`) ve çıplak varyant
biçimi kullanılamaz; bunları `(the point<i32> p)` gibi `the` ile belirtin.

**Aşağı dönüşümler örneklemeye de bakar.** Açık tür bağımsız değişkenleri eşleştirme için kullanılır:
`(the point<i32> p)` yalnızca `point<i32>` değerlerini geçirir ve bir `point<string>` sonraki kola geçer.
Bunun nedeni, bir değerin türünü tür bağımsız değişkenleri dahil hatırlamasıdır (`print-object`'i seçen
aynı mekanizma).

```lisp
(if-let (pattern val) then els)     ; then (with bindings) if val matches pattern, else els. defmacro
(while-let (pattern val) body...)   ; loops while val (re-evaluated each time) matches pattern. defmacro
```

## 5. Yineleme

```lisp
(loop body...)                      ; an infinite loop. leave with break/return
(while test body...)                ; loops while test is true. defmacro
(until test body...)                ; loops while test is false (the negation of while). defmacro
(dotimes (var count-expr) body...)  ; evaluates count-expr once and runs var over 0..count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; CL-style iteration with parallel stepping. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; the sequential version of do (let* binding, assigned in order). defmacro
(doiter (var coll-expr) body...)    ; iterates over a value implementing the Iter trait. defmacro

(break)                             ; leaves only the innermost loop. the value is always Unit
(return)                            ; leaves only the innermost loop
(return value)                      ; leaves the innermost loop with a value
```

`break`/`return`'ün ikisi de **yalnızca en içteki çevreleyen döngüden** çıkar (fonksiyondan erken dönüş
değildirler ve bir `lambda` sınırını aşamazlar). Bir `loop`'un türü, içinde bulunan
`break`/`return`'lerin değer türlerinin birleşimidir (hiç çıkılmıyorsa `!`). Bir fonksiyondan çıkmak için
aşağıdaki `return-from`'u kullanın.

### 5.1 `block` / `return-from` — adlandırılmış çıkışlar

```lisp
(block name body...)                ; a named exit target. the value is the last form,
                                    ; or the value passed by return-from
(return-from name)                  ; leaves that block with Unit
(return-from name value)            ; leaves with a value
```

**`defun` / `defmethod` / `labels`'in her fonksiyonu, kendi adıyla örtük olarak bir block kurar** (CL'deki
gibi). Bu yüzden `(return-from f v)`, fonksiyondan erken bir dönüştür:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` **sözcüksel** bir çıkıştır ve ad **yazıldığı yerde çözümlenir**: denetleyici bir
`return-from`'u çevreleyen `block` ile ilişkilendirir ve değerinin türünü block'un çıkış türüne
birleştirir. Bu yüzden:

- Eşleşen bir `block`'u olmayan bir `return-from` bir **tür hatasıdır** (çalışma zamanı hatası değil).
- Türü diğer çıkışlara ya da gövdenin türüne uymayan bir değer **tür hatasıdır** (`match` kollarıyla aynı
  kural).
- Aynı adlı block'lar iç içeyse **içteki kazanır** (CL'nin gölgeleme kuralı).
- **Fonksiyon sınırlarını aşamaz.** Bir `lambda`'nın içinden dıştaki bir `block`'tan çıkamazsınız
  (`lambda` hiçbir block kurmaz: CL'nin örtük block'ları bir *ad* gerektirir ve anonim fonksiyonların adı
  yoktur). Aşması gereken şey `catch`/`throw`'dur (8. bölüm, **dinamiktir**).

`break`/`return` (5. bölüm) gibi bir **statik** çıkıştır; bu yüzden derlenmiş kodda, derleme zamanında
sabitlenen bir temel bloğa dallanmadır. Aradaki bir `unwind-protect` varsa `cleanup`'ı çalışır (8.
bölüm).

`return-from`'u hiç yazmazsanız örtük block'un hiçbir maliyeti yoktur.

### 5.2 Genişletilmiş `loop` (CL'nin LOOP'u)

**`loop`'un ilk elemanı bir anahtar sözcükse**, yan tümceler dizisi olarak okunur. Aksi halde yukarıdaki
basit döngü olarak kalır ve mevcut `loop`'ların anlamı değişmez (CL'nin kendi basit döngü kuralıyla aynı).

CL yan tümce sözcüklerini çıplak semboller olarak yazar (`(loop for i from 1 to 3 collect i)`), ancak
burada **hepsi anahtar sözcüktür**: çıplak bir `for` yalnızca bir değişken başvurusu olurdu ve anahtar
sözcük olmak, onu basit bir döngüden ayıran şeydir. İstisna, bir değişkeni bir değerden ayıran `=`'tir:
konumu belirsiz değildir; bu yüzden çıplak ya da bir anahtar sözcük (`:=`) olarak okunur.

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #<vector<int> 1 2 3>
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #<vector<int> 1 2 4 8>
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Değişken yan tümceleri** (gövde yan tümcelerinden önce yazılır. Bu CL'nin kuralıdır: sonra yazılırlarsa
"yalnızca oradan sonra yinele" diye okunabilirlerdi; bu yüzden hatadır):

| Yan tümce | Anlamı |
|---|---|
| `:with v = e` | Bir kez bağlar. Önceki yan tümcelerin değişkenlerini okuyabilir |
| `:for v :in s` / `:for v :across s` | Bir `Iter`'in elemanları sırayla. CL'nin liste/vektör ayrımı burada yoktur; bu yüzden bunlar aynı yan tümcenin iki yazımıdır |
| `:for v :on s` | Ardışık **sonekler**. CL paylaşılan kuyruk cons'unu geçirir, ancak bir `Iter`'in paylaşılacak kuyruğu yoktur; bu yüzden her biri yeni bir `Vector`'dür |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Sayma. `:downfrom`/`:upfrom` da çalışır |
| `:for v = e [:then f]` | `e` ile başlar ve ikinci seferden itibaren `f` kullanır (`:then` olmadan her seferinde `e`) |
| `:repeat n` | O kadar kez yineler |

Birkaç `:for` ile bunlar **paralel olarak** ilerler ve herhangi biri tükenir tükenmez döngü biter.

**Gövde yan tümceleri** (her seferinde, yazılan sırayla çalışır):

| Yan tümce | Anlamı |
|---|---|
| `:do form...` | Yan etkiler için |
| `:collect e [:into v]` | Bir `Vector<T>` içine toplar |
| `:append e [:into v]` | Bir `Iter`'in içeriğini ekler |
| `:sum e` / `:count e` | Toplam / doğru olduğu sayı |
| `:maximize e` / `:minimize e` | Maksimum / minimum. **`Option<T>`** (CL'nin boş bir dizi için nil döndürmesi gibi; keyfi bir `Ord` türünün en küçük elemanı yoktur) |
| `:always e` / `:never e` | Hepsi sağlanıyorsa `true`; biri başarısız olur olmaz hemen `false` |
| `:thereis e` | `e` bir **`Option<T>`**'dir. İlk `some`'u döndürür; yoksa `none` (bu, CL'nin "ilk nil olmayan değer"ine karşılık gelendir; bir `bool`'u sınamak için `:always`/`:never` kullanın) |
| `:while e` / `:until e` | Burada **normal şekilde biter** (`:finally` çalışır ve toplanan şey yanıttır) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Bir yan tümceyi koşullu yapar |
| `:return e` | O değerle hemen çıkar (`:finally` çalışmaz, CL'deki gibi) |
| `:initially form...` / `:finally form...` | Döngüden önce / normal tamamlanmada |

**`:named name`** (diğer herhangi bir yan tümceden önce, yalnızca bir kez) tüm döngüyü
`(block name …)` içine sarar. `(return-from name e)`, iç içe döngülerin içinden bile bir anda çıkabilir
ve `:return` gibi `:finally` çalışmaz. Ad olmadan hiçbir block kurulmaz: CL'nin adsız `loop`'u
`block nil` kurar, ancak burada `nil` yoktur ve `break`/`return` (5. bölüm) zaten "en içteki döngüden
çık"ı sağlar.

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

`:finally (return 0)`'ı atlamak bir **tür hatasıdır**. Bu, yalnızca `block` kurallarının iş başında
olmasıdır (5.1): çıkışın `int` türü, döngü tükendiğinde bıraktığı `()`'ya uymaz.

**Döngünün değeri**, varsa biriktiren yan tümcenin birikimidir (birkaç tane varsa ilki), `:always`/
`:never` için `true`, `:thereis` için `none` ve hiçbiri yoksa `()`'dir. `:finally` içindeki son şey
`(return e)` ise değer odur: CL'nin `finally (return …)` deyimi; biriktirmeyen bir döngünün kendi
yanıtını adlandırabilmesinin tek yolu.

**CL'den farklar / dahil olmayanlar**:

- **Yan tümce sözcükleri anahtar sözcüklerdir** (yukarıda).
- `:maximize`/`:minimize`/`:thereis`, `Option<T>` döndürür (nil yoktur).
- **Ne bir birikim ne de `:finally` olmadan yalnızca `:return` yazmak hatadır.** CL tükendiğinde nil
  döndürür, ancak burada böyle bir şey yoktur; bu yüzden döngü, tükendiğinde değerinin ne olduğunu
  söylemelidir.
- Paralel yan tümceleri `:and` ile birleştirme, `:being`/karma tablolar üzerinde ayrılmış yineleme, `:it`
  ve `:nconc` dahil değildir.
- `:collect`'in eleman türü, biriktirilen ifadenin türünden gelir. Bir fonksiyon türü gibi **bir tür adı
  olarak yazılamayan** bir türü toplamaya çalışmak, bunu söyleyen bir hatadır.

## 6. Fonksiyon değerleri ve çağrılar

```lisp
(lambda (params) RetType body...)   ; makes a first-class function value (a closure)
(labels ((name (params) RetType body...) ...) body...)   ; local function definitions that can be mutually recursive
(apply f arg1 ... argN rest-list)   ; calls f (a variadic function with &rest), spreading rest-list
```
Adlandırılmış fonksiyonlar da olduğu gibi değer olarak geçirilebilir (yüksek dereceli fonksiyonlara
bağımsız değişken olarak vb.).

## 7. Diğer özel formlar

```lisp
(setq var value ...)                ; CL's variable assignment. just a sequence of (setf var value). defmacro
(psetq var value ...)               ; parallel assignment. evaluates all values first, then assigns. defmacro
(psetf place value ...)             ; psetq generalized to places (the same expansion). defmacro
(setf place value)                  ; assignment to a place. a place is a variable name / var::field /
                                     ; a call of the form (accessor recv key...). valid if the
                                     ; static type of recv has an instance method named
                                     ; set-{accessor} (for the get of Vector<T> and HashTable<K,V>,
                                     ; set corresponds as an exception; otherwise set-accessor-name).
                                     ; the value is the value assigned (as in CL). so
                                     ; in (if c (setf x 1) ()), then and else do not have matching types
(incf place)  (incf place delta)    ; place += delta (delta=1 if omitted). the result is as with setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 if omitted)
(rotatef place1 place2 ... placeN)  ; rotates N places (new place1=old place2, ...,
                                     ; new placeN=old place1). each place's subforms evaluated once
(shiftf place1 ... placeN newvalue) ; shifts the values of place2..N left and puts newvalue in placeN.
                                     ; the return value is the old value of place1
(list e1 e2 ... en)                 ; expands to (cons e1 (cons e2 (... ()))). () with zero arguments.
                                     ; each element is converted to Sexpr implicitly (like CL's cons, it
                                     ; can hold any value). scalars (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) are wrapped in the matching Sexpr variant, and defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> and the like go in as they are
                                     ; (at no conversion cost). the same for &rest/format arguments.
(source-file)                       ; the name of the file this form was read from (string). fixed as a
                                     ; constant at check time. corresponds to CL's *load-pathname*, but is
                                     ; not a variable: module bodies run after checking, so "currently
                                     ; loading" cannot be relied on, while at check time it is always known.
                                     ; for sources that are not files, the reader's name for them (<stdin>/<input>)
(quote datum)                       ; the same as 'datum. returns it as Sexpr data without evaluating
(quasiquote template)               ; the same as `template. embeds expressions in the template with ,/,@
(documentation name)                ; returns the docstring of name (a bare name or Type::method) as Option<string>
(panic message)                     ; message: string. ends abnormally with an unrecoverable error. type !
(unreachable)                       ; expands to (panic "unreachable"). defmacro
(todo)                              ; expands to (panic "todo"). defmacro
(as Type expr)                      ; numeric/character type conversion. conversions that can fail panic on failure
(try-as Type expr)                  ; like as, but returns the result as Option<Type> (None on failure)
(print control args...)             ; expands the format and writes to standard output (no newline)
(println control args...)           ; the same (with a newline at the end)
(format dest control args...)       ; CL's format. returns the expanded string
(pprint x)                          ; pretty-prints. writes a newline first, as in CL
(pprint-fill x)                     ; fill layout
(pprint-linear x)                   ; all on one line or one element per line
(pprint-tabular x [colinc])         ; tabular layout (16 columns by default)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; build a logical block yourself
```

`print`/`println`/`format`/`pprint` ailesi özel formlardır; bu yüzden değişken sayılı bağımsız
değişkenleri (`pprint` ailesi için tek bir nesne) geçirilmeden önce kendi türleriyle `Sexpr` içine
sarılır: `(println "~a" my-struct)`'ın doğrudan çalışmasının nedeni budur. Biçim yönergelerinin ve pretty
printer'ın ayrıntıları [Biçim Yönergeleri](functions/format.md) ve
[Yazdırma](functions/printing.md#4-pretty-printer) belgelerindedir.

`as`/`try-as` yalnızca sayısal ve karakter kataloğunu ele alır (`int`, sabit genişlikli tamsayı
türleri, `f32`/`f64`/`ratio`/`char` arasında). Aynı tür dönüşüm değildir. **Tamsayı genişlikleri
(`int` dahil) arasındaki ve `f32`↔`f64` arasındaki dönüşümler gerçek dönüşümlerdir**: `as` kırpar /
yuvarlar ve `try-as` o genişliğe (duyarlığa) sığıp sığmadığını yanıtlar. `(as int x)`, sabit bir
genişlikten tam genişletmedir ve `(as i32 n)`, `int`'ten kırpmadır. Tamsayı → `char` aralık dışında
başarısız olabilir; bu yüzden `as` panic olur ve `try-as` `None` verir. Geri kalan her şey (genişletme ve
`float->int`/`ratio->int`'in kırpması) her zaman başarılı olur. `float->int`/`ratio->int`/`char->int`
`int`'e iner ve daha dar bir genişlik istenirse onlardan sonra `int->W` çağrılır. Bu, karşılık gelen
dönüşüm metotlarına açılan bir şekerdir ([Sayılar](functions/numbers.md)'daki
`int->char`/`int->int`/`int->W` vb.).

`documentation`, `quote`/`compile` gibi, `name`'i değerlendirmeden, değerlendirilmemiş çıplak bir sembol /
`::` yolu olarak okuyan bir özel formdur. CL'nin `(documentation 'name 'function)`'ından farklı olarak bir
tür bağımsız değişkeni almaz: `name`'i değişken → fonksiyon → tür → trait → makro sırasıyla çözümler
(çıplak bir tanımlayıcı bir ifade olarak değerlendirildiğinde olduğu gibi aynı öncelik) ve bulunan tanımın
docstring'ini döndürür (`(documentation Type::method)` metotlar içindir). Çözümlenememesi (o adda tanım
yok) bir denetim zamanı hatasıdır; var olan ama docstring'i olmayan bir tanım `Option::none` verir. Her şey
denetim zamanında bir sabit olarak belirlenir: çalışma zamanı araması olmaz. Modül nitelikli serbest adlar
(`Type::method` dışında `mod::name`) desteklenmez.

## 8. Yerel olmayan çıkışlar (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; runs body. if (throw 'tag v) happens anywhere
                                    ; body reaches, that v becomes the value
(throw 'tag value)                  ; exits to the nearest dynamically enclosing (catch 'tag ...)
(unwind-protect protected cleanup)  ; runs cleanup however protected is left
```

`break`/`return`'ün (5. bölüm) aksine bu, **dinamik** bir çıkıştır: `throw` çevresindeki `catch`'i
sözcüksel olarak aramaz ve istenen sayıda fonksiyon çağrısı boyunca aynı etiketli bir `catch`'e ulaşır.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; if not found, the value at the end as usual
```

- **Etiketler yalnızca sabit sembollerdir** (`'done`). CL'den farklı olarak değerlendirilmezler.
- **Bir etiket bir tür taşır.** Tür, `'tag`'in ilk kullanıldığı anda belirlenir ve aynı sembolün sonraki
  her `throw`/`catch`'i ona karşı denetlenir. Onu başka bir türle kullanmak tür hatasıdır.
- `throw`'un türü `!`'dir (sapar). `(catch 'tag expr)`'in türü, `expr`'in türü ile etiketin türünün
  birleşimidir.
- `unwind-protect`'in değeri `protected`'ın değeridir. `cleanup`'ın değeri atılır. `cleanup`,
  `protected`'dan nasıl çıkılırsa çıkılsın çalışır: normal tamamlanma, `throw` ve `panic`'e ek olarak
  `break`/`return`/`return-from` ile çıkıldığında da çalışır. `cleanup`'ın kendisinin yaptığı yerel olmayan
  bir çıkış, süren çıkışa üstün gelir.
- İç içe `unwind-protect`'ler içten dışa çalışır. `protected`'ın **içindeki** bir döngüden çıkan bir
  `break`, `protected`'dan çıkmış olmaz; bu yüzden `cleanup`'ı çalışmaz.

CL'nin koşulları (`define-condition`/`handler-bind`/`invoke-restart`) benimsenmemiştir. Statik tür
denetimine uymazlar; bu yüzden kurtarılabilir başarısızlıklar `Result` ile ifade edilir (9. bölüm).

## 9. Hata yönetimi politikası

- Kurtarılabilir başarısızlıklar: `Result<T,E>` + `match`. Kurtarılamayan başarısızlıklar (hatalar, bozuk
  değişmezler): `panic`.
- `?`/try'a karşılık gelen bir sözdizimi yoktur. Dallanmalar `match` ile açıkça yazılır.
- Fonksiyon ve özel form adları sonek olarak `!` (yıkıcı işlemler) ya da `?` (yüklemler) kullanmaz.
  Yüklemler `-p`/`p` soneki (`zerop`, `consp` vb.) ya da `is-` öneki (`is-some`, `is-ok` vb.) ile
  adlandırılır.

## 10. Derleme

```lisp
(compile name)                      ; JIT-compiles an already defined defun/method into native code
(compile-file src-path out-path)    ; AOT-compiles a source file into a native executable (skips the final `(main)`)
(dump path)                         ; writes the current environment (type information + compiled bodies) to one file
(disassemble name)                  ; prints what that definition becomes (host machine code by default, LLVM IR with true as the second argument)
```

`compile` bir özel formdur; `name` değerlendirilmez ve değerlendirilmemiş çıplak bir sembol / `::` yolu
olarak okunur (bir string tür hatasıdır). Jenerik fonksiyonlar hedeflenemez: her tür için bir kopya,
kullanıldığı her yerde yapılır; bu yüzden tek bir derlenmiş gövde yoktur. **Çözümlenemeyen bir ad bir
denetim zamanı hatasıdır** ve asla çalışma zamanına taşınmaz (şunlar için ayrı mesajlar vardır: tür var
ama o metot yok / ne tür ne fonksiyon var / çıplak tanımsız bir ad). Buradaki görünürlük başka herhangi
bir başvuru gibi ele alınır: "var ama buradan görünür değil", tıpkı "çözümlenmiyor" gibi denetim zamanında
başarısız olur.

Çağrılanlar da geçişli olarak derlenir; bu yüzden **(dolaylı olarak bile) derlenemeyen bir şeyi çağıran
bir fonksiyon derlenemez**. Süreç çökmez; bunu söyleyen bir hatayla reddedilir. Her yerleşik fonksiyon
derlenebilir; bu yüzden bu şekilde reddedilen tek fonksiyonlar, aşağıdaki yalnızca yorumlayıcıya özgü
işlemleri çağıranlardır:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

Yalnızca yorumlayıcıya özgü olanlar `compile`/`compile-file`/`dump` ve
`trace`/`untrace`/`step`/`disassemble`'dır
([Gerçekleştirim araçları](functions/system.md#5-gerçekleştirim-araçları-clhs-252)). Bunlar derlenemeyen
şeyler olmaktan çok, derleyen tarafın işlemleridir (`dump`'ın yazdığı şey yorumlayıcının ortamının
kendisidir ve bir AOT çalıştırılabilir dosyasında yoktur; `trace`'in izlediği ve `step`'in durduğu yerler
çalışan yorumlayıcının çağrı yollarıdır; ve `disassemble` derleyicinin kendisini kullanır).
`room`/`dribble`/`ed` bunların arasında değildir ve normal şekilde derlenebilir.

**Derlenebilenler**: akış ve dosya G/Ç'si, `random`, `gensym`, `symbol->string`/`string->symbol`,
`parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, aşkın fonksiyonlar, bit
işlemleri, `catch`/`throw`/`unwind-protect`, `eq`/`eql`/`equal`/`equalp`'in dördü de (bu, `case`'in her
tür için derlenmesini sağlar), `print`/`println`/`format`/`pprint` ve `pprint-logical-block` dahil tüm
yazdırma ailesi, `read` ve `eval`. Standart kütüphane önceden derlenmiş olarak gelir.

Bir AOT çalıştırılabilir dosyası yalnızca programın kullandığı özellikleri içerir. Yazdırmayan bir program
biçimlendirme motoru almaz, `read` çağırmayan bir program okuyucu almaz ve `eval` çağırmayan bir program
denetleyici ve yorumlayıcı almaz.

Komut satırından `typl -c src-path [-o out-path]` (`-c` ayrıca `--compile` olarak da yazılabilir),
`compile-file` ile aynısını yapar. `-o` olmadan çıktı, `.typl` uzantısı kaldırılmış `src-path`'tir.
Varsayılan olarak çalıştırılabilir dosyalara bağlanan `libtypelisp_front.a` statik kütüphanesi, `typl`'nin
bir release derlemesi için, `typl`'nin kendi içinde taşıdığı ve ilk bağlamada
`$TYPELISP_HOME/lib/<build ID>/` altına (`TYPELISP_HOME` olmadan `~/.typelisp/lib/<build ID>/` altına)
yazılıp oradan kullanılandır; bir debug derlemesi için `typl`'nin derlendiği yerdeki kütüphanedir.
`typl --remove-lib`, o `typl`'nin yazdıklarını siler. `--others` ile diğer derleme kimliklerinin
yazdıklarını, `--all` ile her derleme kimliğinin yazdıklarını siler. `typl --lib-dir DIR` ile `DIR`
içindeki kullanılır (hem `-c` hem `compile-file` için) ve orada yoksa başlangıçta bir hatadır.

### 10.1 Dump'lar

```lisp
(dump "session.typld")     ; write one out
```
```sh
typl --image session.typld prog.typl   # start from it
typl --image session.typld             # the REPL too
```

Bir dump, tür bilgisini ve derlenmiş gövdeleri tek bir dosyada tutar. `(dump path)`'in yazdığı şey,
geçerli oturumun yüklediği şey (standart kütüphane ya da `--image` ile geçirilen bir dump) artı **oturumun
kendisinin tanımladıklarıdır**. Bu yüzden çıktı kendi kendine yeterlidir ve `typl --image` aynı ortamı
getirir. Oturumun `(compile f)` ettiği, derlenmiş biçimiyle yazılır.

Kaydedilen şey **tanımlardır, geçmiş değil**:

- Oturumun üst düzey ifadeleri (`(println ...)` vb.) dahil değildir. Yükleme onları yeniden çalıştırsaydı
  sorun olurdu.
- Global değişkenler, dump anındaki değerle değil, **başlatıcılarının yeniden çalıştırılan değeriyle**
  geri gelir. Bu, SBCL'in `save-lisp-and-die`'ından (heap'i olduğu gibi yazar) bilinçli bir farktır ve bu
  seçim bir sorunlar ailesini ortadan kaldırır: açık akışlar, closure'ların fonksiyon işaretçileri ve dış
  bellek gibi "kaydedilemeyen değerler".
- `save-lisp-and-die`'dan farklı olarak **süreç ölmez**; çünkü yazmak imgeye zarar vermez.

Bir dump, onu yazan gerçekleştirimin standart kütüphanesinin ve derleyicisinin sürümlerini kaydeder.
Onu farklı sürümdeki bir `typl` ile yüklemek bir hatadır; asla sessizce kabul edilmez.

### 10.2 AOT çalıştırılabilir dosyalarda `eval`

`eval`, "geçerli global ortama" karşı tür denetimi yapar ve sonra değerlendirir
([Ayrıştırma ve değerlendirme](functions/system.md#6-ayrıştırma-ve-değerlendirme)). Bu ortam (denetleyicinin
danıştığı imza, tür ve makro tabloları ve yorumlayıcının çalıştırabildiği gövdeler) **makine kodunda
değildir**. Derlenmiş bir fonksiyon, bir adrese konmuş bir sembolden başka bir şey değildir; ne
bağımsız değişken türleri ne de gövdeleri adla aramak için bir tablosu vardır.

Bu yüzden yalnızca `eval` çağıran programlar için `compile-file` **o ortamı derleme zamanında kurar ve
çalıştırılabilir dosyaya yazar**. Biçim bir dump ile aynıdır ve standart kütüphanenin kısmını ve
programın kendi kısmını içerir. Başlangıçta yapılan tek şey onu geri yüklemektir: kaynak yeniden
okunmaz ve hiçbir şey yeniden tür denetiminden geçmez. `eval` çağırmayan programlara hiçbir şey eklenmez.

Sonuçlar:

- **Başlangıç daha uzun sürer ve çalıştırılabilir dosya daha büyüktür**; çünkü denetleyicinin ve
  yorumlayıcının kodu ve ortamın bir anlık görüntüsü girer. Heap de biraz daha büyük yapılır.
- **Eval'e geçirilen formlar yorumlanır.** Eval'e geçirilen form programın kendi fonksiyonlarını
  çağırdığında bile, çalışan şey anlık görüntünün tuttuğu yorumlanabilir gövdedir. Sonuç aynıdır; yalnızca
  hız farklıdır.

Global değişkenlerin depolaması derlenmiş kodla **paylaşılır** (aynı yuvalar). Bir `defvar` başlatıcısı
derlenmiş başlatma tarafından bir kez çalıştırılır ve geri yükleme onu atlar; bu yüzden yan etkili bir
başlatıcı iki kez çalışmaz.

`compile-file` standart kütüphaneyi de okur (ve gövdelerini çalıştırılabilir dosyaya gömer); bu yüzden
`abs`/`gcd` gibi standart kütüphane fonksiyonları ile `(defmethod print-object ...)`'nin yanı sıra
`(impl print-object ...)` de AOT ile kullanılabilir.

`compile-file` ayrıca `use`'u (ve `import`/`shadowing-import`'u) kabul eder. Giriş dosyasının `(use m)`'i
dosyaları `typl file.typl` ile aynı kurallarla bulur ve bulunan bağımlılık dosyaları da derlenip
çalıştırılabilir dosyaya bağlanır: `main.typl`'nin `http.typl`'yi `(use http)` ile okuduğu bir düzen
olduğu gibi AOT ile derlenebilir. Giriş dosyasının kendi tanımları da, `typl file.typl`'de olduğu gibi,
dosyanın adını taşıyan modüle girer (`p.typl` içindeki `point`, `p::point`'tir). Bu yüzden değerlerin
yazdırılan gösterimi (`#<p::point x: 1 y: 2>`) hangi yolla çalıştırılırsa çalıştırılsın aynıdır.

## 11. Okuyucu makroları (readtable)

Okuyucunun **belirli bir karakterle karşılaştığında ne yaptığı** programdan değiştirilebilir (CLHS 23.1).

```lisp
(set-macro-character c f)             ; f reads the character c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f reads the two-character sequence d s
(get-dispatch-macro-character d s)    ; Option<f>
```

`f`'in türü `(fn (string-input-stream char) Option<Sexpr>)`'dir. İlk bağımsız değişken **henüz
okunmamış metin üzerinde bir akıştır** ve ikincisi **onu tetikleyen karakterdir** (dağıtım için ikinci
karakter). Dönüş değeri o noktada okunan veri olur. Akış, `:dyn PeekInput` değil, somut bir türdür; çünkü
okuyucu her zaman bu tek türü geçirir: `read-sexpr` / `read-char` / `peek-char` / `unread-char` /
`read-delimited-list`'in hepsi `(where (PeekInput S))` alır; bu yüzden hepsi somut türde olduğu gibi
çalışır.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => read as (not (equal 1 2)), that is, true
```

Okuyucu **makro karakterlerine yerleşik sözdiziminden önce bakar**; bu yüzden `(` ve `'`'ı da devralabilir.
Bu şekilde kaydedilen `#`'in alt karakterleri, yerleşik `#b`/`#x`/`#.`'ya göre öncelik alır. `#` dışındaki
bir karakter, `set-dispatch-macro-character`'e geçirildiğinde anında bir dağıtım karakteri olur: CL'nin
`make-dispatch-macro-character`'ının karşılığı **yoktur**. Kayıt zaten işini yapar; bu yüzden ayrı bir
adımın yapacak bir şeyi olmazdı.

**Ne zaman etkili oldukları** okuma yoluna bağlıdır; `#.` ile aynı (1. bölüm):

- REPL ve `(load ...)` her seferinde bir formu çalıştırır; bu yüzden **önceki formlarda tanımlanan
  fonksiyonlar** olduğu gibi kaydedilebilir.
- Modül dosyaları bir birim olarak denetlenir ve sonra çalıştırılır; bu yüzden **yalnızca
  `set-macro-character` / `set-dispatch-macro-character` çağrıları hemen çalışır** (CL'nin
  `(eval-when (:compile-toplevel) ...)`'ının rolü). Hemen çalıştıkları için **geçirilen fonksiyon o
  noktada zaten var olmalıdır**. Aynı dosyadaki bir `defun` henüz çalışmamıştır; bu yüzden bir `lambda`
  yazın ya da standart kütüphaneyi veya zaten çalışmış bir şeyi kullanın. Yalnızca üst düzey çağrılar
  kapsanır; `progn` ya da `let` içine bakmaz.

Yerleşik `read` / `read-from-string` de readtable'a danışır (CL'deki gibi).

**Olmayanlar**: `*readtable*` ve `copy-readtable` ile `readtable-case`. İlk ikisi, bir readtable **bir değer
olmadığı** için: bir değer "bir okuyucuya verilebilecek bir şey" olmak zorundadır, ancak kaynağı okuyan
okuyucu programın dışındadır ve verilecek bir yeri yoktur. `readtable-case`, 1. bölümün bu dilin
okuyucusunun her zaman küçük harfe çevirdiğine (CL'nin `:downcase`'i) karar vermesi nedeniyle.

## 12. Eşzamanlılık (task'ler)

**Bir task hafif bir thread'dir** (Go'nun diliyle bir `go` deyiminin başlattığı şey) ve işbirliğiyle
çalışır (önceden kesme yoktur). Geçiş çekirdekten geçmez ve yürütme durumu bir makine yığınında değil,
heap'te bulunur; bu yüzden task'leri büyük sayılarda oluşturmak ucuzdur.

**Task'ler birkaç OS thread'inde aynı anda çalışır** (çok çekirdekli paralellik). Thread sayısı
`TYPELISP_THREADS` ortam değişkenidir (`main`'i çalıştıran thread dahil toplam; varsayılan makinenin
paralelliğidir). `typl` içinde **yalnızca derlenmiş task'ler** diğer thread'lerde çalışır ve yorumlanan
task'ler yorumlayıcının thread'inde çalışır (12.7). Paylaşılan veri `Mutex<T>` ya da `Chan<T>`
üzerinden geçer; bunlardan geçmeyen eşzamanlı okuma ve yazmalar Go'daki gibi tanımsızdır (12.7).

Sözcük dağarcığından **yalnızca `task` / `thread` / `select` özel formlardır**; geri kalanı sıradan
fonksiyonlar, metotlar ve makrolardır ([Task'ler ve Kanallar](functions/concurrency.md)).

### 12.1 `task` — bir task başlatma

```lisp
(task (f arg...))                   ; returns Task<T>, where T is the return type of f
```

**Yalnızca bir çağrı biçimini alır.** `f` ve her `arg`, `task`'in yazıldığı yerde, yazıldıkları sırayla
değerlendirilir ve yeni task'te yalnızca **çağrı** gerçekleşir. Bu, Go'nun `go f(x)`'iyle aynı kuraldır
ve onun bir thunk değil bir çağrı biçimi alma nedeni de budur: bir thunk bağımsız değişkenlerini
değerlendirmeden yakalardı.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i is evaluated on the spot each time; no capture trap

(task ((lambda () ()                ; to run an arbitrary body, call a lambda
         (println "start")
         (send ch 1))))
```

Özel formlar (`if` / `let` / `progn` …) doğrudan `task`'in altına yazılamaz.

**Neden bir fonksiyon olamaz**: `(spawn (lambda () T body...))` yazmak `T`'yi açıkça yazmayı gerektirirdi;
çünkü `lambda` bir dönüş türü ek açıklaması ister ve bir makro `(f a b)`'nin dönüş türünü bilmez. Onu
yalnızca denetleyici bilir.

### 12.2 `thread` — ayrılmış bir OS thread'inde task başlatma

```lisp
(thread (f arg...))                 ; returns Thread<T>, where T is the return type of f
(join th)                           ; waits for completion and returns its value (any number of times)
```

Biçim ve değerlendirme kuralları `task` ile aynıdır (yalnızca bir çağrı biçimi alır ve `f` ile `arg`
yazıldıkları yerde değerlendirilir). Fark, nerede çalıştığıdır: **o task'e ayrılmış bir OS thread'i
başlatır ve yalnızca onun üzerinde çalışır**. Diğer task'lerle çoklanmaz; bu yüzden içinde engelleyen bir
C fonksiyonu (`defffi`) çağırmak yalnızca o thread'i durdurur ve diğer task'ler ilerler. İçinde `task`,
`send`, `recv` ve diğerleri olduğu gibi kullanılabilir.

- `Thread<T>`, `Task<T>`'nin karşılığıdır. `wait` gibi `join` da **çağıran task'i** durdurur ve değer
  önbelleğe alınır. Task bittiğinde thread de biter.
- Panic kuralları `task` ile aynıdır (tüm süreç çöker). `main` döndüğünde süreç biter.
- Bir fonksiyon olarak yazmak için `(Thread::spawn (lambda () T body...))` kullanın (Rust'ın
  `std::thread::spawn`'ı). Adlandırılmış bir fonksiyon da geçirilebilir.
- **Ayrılmış bir thread'de yalnızca derlenmiş kod çalışır.** `typl` yorumlarken `(thread (f ...))` ya da
  `Thread::spawn`'ı değerlendirdiğinde, çalıştırılacak fonksiyonu (ve çağırdıklarını) çalıştırmadan önce
  anında derler. Derlenemeyen şey (dışındaki yerel değişkenlere başvuran bir `lambda`, bir struct
  oluşturma vb.) thread başlatılmadan önce, bir `(panic ...)` ile aynı şekilde ele alınan bir panic'tir.
  Yerel değişkenlere başvuran bir `lambda`, derlenmiş bir fonksiyonun içinde oluşturulmuşsa geçirilebilir.

### 12.3 `select` — birkaç kanal işlemini aynı anda bekleme

```lisp
(select
  ((v (recv ch1)) body...)          ; a receive arm. v is bound to an Option<T>
  ((send ch2 x) body...)            ; a send arm
  (else body...))                   ; optional. **if written, it goes last**
```

- **`else` ile bloke olmaz** (Go'nun `default`'u). Olmadan biri mümkün olana kadar bekler.
- **Birkaçı aynı anda mümkünse biri rastgele seçilir** (yazılan sırayla olsaydı sonraki kollar açlıktan
  ölürdü).
- Bir alma kolunun `v`'si bir **`Option<T>`**'dir. Kapalı bir kanal "bir yanıttır", kolu atlama nedeni
  değildir; bu yüzden kolun içinde ona `match` uygulayın.
- Tür, **tüm kolların gövdelerinin türlerinin birleşimidir** (`match` kollarıyla aynı kural).
- Sıfır kollu `(select)` bir tür hatasıdır (Go'nun sonsuza dek bloke olan `select{}`'ı benimsenmemiştir).
  Yalnızca `else` içeren bir `select` de öyledir; çünkü gövdeyi doğrudan yazmakla aynıdır.

**Kanal ifadeleri ve gönderilecek değerler, hangi kol seçilirse seçilsin, soldan sağa bir kez
değerlendirilir** (`case`'in anahtarları için sahip olduğu aynı disiplin).

```lisp
(select                             ; receiving with a timeout
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([bir süre sonra ileten kanal](functions/concurrency.md#5-after--bir-süre-sonra-ileten-kanal)),
Go'nun `time.After`'ına karşılık gelen "`sec` saniye sonra tek bir değer ileten bir kanal"dır.

### 12.4 Diğer özelliklerle etkileşim

| Özellik | Task'lerle ilişkisi |
|---|---|
| `catch` / `throw` | **Task sınırlarını aşmaz.** Bir task'in gövdesinden çıkmaya çalışan bir `throw` bir panic'tir |
| `unwind-protect` | Temizlik, bir task doğal olarak bittiğinde çalışır. **Ana task bittiği için süreç bittiğinde çalışmaz** |
| `block` / `return-from` | Sözcüksel olduklarından `lambda` sınırlarını aşmazlar |
| `panic` | Go'daki gibi tüm süreç çöker. `wait` bir panic'i değer olarak gözlemlemez |
| `dlet` | **Task başına bir bağlama değildir.** Hâlâ "bir globali ödünç alıp geri verir"; bu yüzden task'ler birbirine karışır |
| Standart çıktı | Tüm task'ler tarafından paylaşılır. Bir `println`'in çıktısı asla bir satırın ortasında diğerleriyle karışmaz |
| `compile` / `eval` | Kısıtlama yok. Bir task içinde `(compile f)` çalışır |

### 12.5 Task'lerin geçiş yaptığı yerler

Zamanlama işbirlikçidir; bu yüzden **task'ler yalnızca sizin yazdığınız yerlerde geçiş yapar**:
`(yield)`, `(sleep ...)`, `(wait ...)`, **beklemesi gereken kanal işlemleri** (`send`/`recv`/`select`) ve
**beklemesi gereken soket işlemleri** (`accept` / `tcp-connect` (ad çözümleme dahil) / soketleri okuma ve
yazma / `recv-from`; [Ağ](functions/network.md)). Tüm soketler engellemesizdir: biri hazır değilse yalnızca
o task durur ve işletim sistemi hazır olduğunu söylediğinde sürer; Go'nun netpoller'ıyla aynı biçim.
Hiçbir task çalışamadığında gerçekleştirim, en yakın `sleep` son tarihine kadar işletim sisteminde bekler.

Yerinde yanıt verebilen kanal işlemleri (tamponda yeri olan bir `send`, bekleyen bir değeri olan bir
`recv`, `(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **sırayı tüketmez**. Bu, bir okuma tarafından
beklenmedik şekilde kesilmediğiniz anlamına gelir ve CL'nin "0 saniyeliğine yol ver"i olan
`(sleep 0.0)`'dan farklı ele alınır.

**Önceden kesme yoktur.** Hiçbir şey çağırmayan sıkı bir döngü diğer task'leri aç bırakır. Ancak derlenmiş
döngüler denetimi düzenli aralıklarla zamanlayıcıya verir; bu yüzden derlenmiş sıkı bir döngü onları aç
bırakmaz.

### 12.6 Derlenmiş kod ve task'ler

Derlenmiş kod da task'leri askıya alabilir. `compile-file` ile yapılan çalıştırılabilir dosyalar için de
aynısı geçerlidir: `main`, zamanlayıcının ana task'i olarak çalışır ve `task`, `sleep`, `wait`, kanallar ve
soket beklemelerinin hepsi `typl`'dekiyle aynı anlamla çalışır. `main` döndüğünde süreç biter ve kalan
task'ler kesilir (Go'daki gibi). Yorumlayıcı, zamanlayıcı uğruna asla çalıştırılabilir dosyaya konmaz.

Tek istisna, **beklemesi gereken** işlemlerin hata olduğu (sessizce kilitlenmekten daha dostça) "bir C
FFI geri çağrısının içi"dir: `defffi` ile geçirilen bir fonksiyon C'den çağrılırken C'nin yığını
üsttedir ve task'i askıya alıp sonra sürdürmenin bir yolu yoktur.

Aşağıdaki yerler de bir task'in ortasında çağrılan fonksiyonlardır, ancak askıya alamazlar:
`print-object` metotları, `format` içindeki `~/name/`, okuyucu makroları, `eval`'in içi ve AOT çalıştırılabilir
dosyalarındaki `defvar` başlatıcıları. Burada **beklemeden yanıt veren işlemler geçer** (tamponda değer
olan bir `(recv ch)`, zaten veri alınmış bir soketteki `read-line`, `(task ...)`, `(yield)` vb.) ve
**gerçekten beklemesi gereken işlemler hatadır** (süreci anında durdurmak değil, `` `recv` cannot block: ... ``
gibi bir panic, bir `(panic ...)` ile aynı şekilde ele alınır).

### 12.7 Go'dan farklar

- **`typl` içinde yalnızca derlenmiş task'ler diğer thread'lere çıkar.** Yorumlayıcının durumu thread'ler
  arasında paylaşılamaz; bu yüzden yorumlanan bir `task`'ten gelen task'ler yorumlayıcının thread'inde
  çalışır. Derlenmiş bir task da, yorumlanan bir fonksiyon değerini çağırdığı, kimsenin derlemediği bir
  `:dyn` metodunu çağırdığı ya da `eval`/`macroexpand`/`read` çağırdığı noktada **yorumlayıcının
  thread'ine geçer ve orada kalır** (geri dönmez). Uzun bir hesaplama yolda bir kez bile yorumlanan koda
  dokunursa, geri kalanı yorumlayıcının thread'inde çalışır.
- **`typl` içinde işçiler yalnızca bir üst düzey değerlendirme süresince yaşar.** REPL girdi beklerken ve
  üst düzey formlar arasında diğer thread'ler task'leri ilerletmez (kalan task'ler sonraki değerlendirmede
  kaldıkları yerden sürer). Bir değerlendirmenin sonunda her thread'in geçerli adımını bitirmesini bekler;
  bu yüzden bir `thread` içinde bir C fonksiyonu (`defffi`) bloke olmaya devam ederse değerlendirme o
  dönene kadar bitmez.
- **İşçilerde yazdırma**: yorumlanan `print-object` / `~/name/` metotları diğer thread'lerde çalışamaz;
  bu yüzden bu tür değerleri başka bir thread'de yazdırmak, bir `(panic ...)` ile aynı şekilde ele alınan
  bir panic'tir (`(compile T::print-object)` yapın ya da ana task'ten yazdırın).
- **Veri yarışları tanımsızdır** (Go ile aynı konum). Birkaç task'in aynı değeri `Mutex<T>` / `Chan<T>`
  üzerinden geçmeden değiştirmesinin sonucu garanti edilmez.
- **`task` bir değer döndürür.** Go'nun `go` deyiminden farklı olarak bir `Task<T>` döndürür ve
  `(wait t)` sonucu alır.
- **nil kanallar yoktur.** Go'nun fan-in deyimi (kapalı bir kanalı `select`'in kollarından düşürmek için
  `nil` yapmak) yazılamaz; bu yüzden girdi başına bir task başlatıp onları bir `WaitGroup` ile birleştirin
  ([WaitGroup](functions/concurrency.md#4-waitgroup--n-işin-bitmesini-bekleme)). Go'da da önerilen yol
  budur, ancak bu, **Go'dan gelenlerin karşılaştığı ilk farktır**.
