<!-- translated-from: docs/ja/reference/functions/printing.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Yazdırma

`print`/`println`/`format`, tek bağımsız değişkenli yazıcılar, pretty printer, `print-object` ve
yazdırmayı denetleyen değişkenler. Biçim yönergelerinin listesi [format.md](format.md) belgesindedir.
Akışlardan okuma ve akışlara yazma [Akışlar ve Dosyalar](streams-files.md) belgesindedir.

## 1. `print` / `println` / `format`

`print`/`println`/`format`'ın hepsi **biçim yönergelerini yorumlayan özel formlardır (CL'nin `format`
yönergeleri)**. İlk bağımsız değişken (`format` için ikincisi) **denetim string'idir** ve her yönerge
sonraki değişken sayılı bağımsız değişkenleri sırayla tüketir.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Denetim string'ini açar ve yeni satır olmadan standart çıktıya yazar |
| `println` | `(println control args...)` | `(string, ...)→Unit` | Aynısı, sonunda yeni satırla |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CL'nin `format`'ı. Açılmış string'i döndürür. `dest` `true` (CL'nin `t`'si) ise ayrıca standart çıktıya yazılır; `false` (CL'nin `nil`'i) ise yazılmaz ve yalnızca döndürülür |
| `format` (bir akışa) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | `dest` bir `bool` değilse CL'nin akış hedefidir. Açılmış string o akışa yazılır. Dönüş değeri `()`'dir (CL'nin `nil`'i) ve string döndürülmez |

`dest`'in türü anlamı ikiye böler (hangisinin geçerli olduğu statik olarak belirlenir). Akış biçimi;
somut bir akış türüyle, bir `:dyn CharOutput` ile ya da `(where (CharOutput S))` ile bağlanmış bir tür
değişkeniyle aynı şekilde yazılabilir. Ne `bool` ne de akış olan bir `dest` tür hatasıdır.

**Denetim string'i bir sabit olmalıdır** (Rust'ın `format!`'ıyla aynı kısıtlama). İçindeki yönergeler
kaç bağımsız değişkenin ve hangi türlerde alınacağını belirler; bu yüzden çalışma zamanında oluşturulan
bir string denetim zamanında okunamaz. Sabit olmak zorunda olduğundan **bağımsız değişkenlerin sayısı ve
türleri denetim zamanında kontrol edilir**: `(println "~d" "x")` ve `(println "~a ~a" 1)` denetim zamanı
hatalarıdır. Yanlış yazılmış bir yönerge, kapatılmamış bir `~(` ve hiçbir bağımsız değişkenin
yanıtlayamadığı bir `~/name/` de denetim zamanı hatalarıdır. Denetim kuralları
[format.md](format.md#1-yönergeler-nasıl-yazılır) belgesindedir. Oluşturduğunuz bir string'i yazdırmak
için onu `(format false ...)` ile yapın ve `(println "~a" s)` ile yazdırın.

Değişken sayılı bağımsız değişkenler geçirilmeden önce kendi türleriyle `Sexpr` içine sarılır:
`i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr` ile kullanıcı tanımlı
`defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>` ve benzerlerinin hepsi olduğu gibi geçirilebilir
(`(println "~a" my-struct)` doğrudan çalışır).

Bir betiği `typl file.typl` ile çalıştırmak **üst düzey ifadelerin değerlerini yazdırmaz**; bu yüzden bir
program standart çıktıya bunları çağırarak yazar. `print`/`println`/`format` çıktılarını her çağrıda
dışarı gönderir (böylece bir istem, standart girdi okunmadan önce, bir boru üzerinden bile görünür).

**`Option<Sexpr>` saydam olarak yazdırılır.** S-ifade verisinin türü `Option<Sexpr>`'dir; bu yüzden
`(some x)` sarmalayıcısı çıktıda görünmez ve içerik olduğu gibi yazdırılır. Boş liste `()` olarak
yazdırılır. Diğer `Option<T>`'ler `(some ...)` / `none` olarak yazdırılır. Aynısı struct'ların,
enum'ların ve `Vector`'lerin içindeki `Option<T>` alanları için de geçerlidir. `(eval ...)`'ten gelen bir
`Result<Option<Sexpr>,…>`, `(ok 42)` olarak ya da `none` için `(ok ())` olarak yazdırılır.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; get only the string, without printing
  (println "~a" s))                   ; => id=42
```

## 2. Tek bağımsız değişkenli yazıcılar

CLHS 22.1.3'ün yazıcıları. Bir biçimi açmak yerine tek bir değeri olduğu gibi yazdırırlar. Akış
atlanabilir (varsayılan olarak `*standard-output*`).

| Ad | Biçim | Açıklama |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Geri okunabilen bir biçimde yazar (`~s` ile aynı) ve `x`'i döndürür |
| `princ` | `(princ x [stream])` | İnsanlar için bir biçimde yazar (`~a` ile aynı) ve `x`'i döndürür |
| `write` | `(write x [stream])` | `*print-escape*` true ise `prin1`, false ise `princ`. `x`'i döndürür |
| `prin1-to-string` | `(prin1-to-string x)` | Yazmak yerine bir string döndürür (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | Aynısı (`~a`). `to-string` ile aynı |
| `write-to-string` | `(write-to-string x)` | Aynısı, `*print-escape*`'i izler |

`print`/`println` bunların arasında **değildir**. Bir denetim string'i alan `format` kısayollarıdır;
CL'nin `print`'inden (yeni satır, sonra `prin1`, sonra bir boşluk) farklı bir iştir; bu yüzden her biri
kendi adını korur. Sonuç olarak **CL'nin tek bağımsız değişkenli `print`'inin bu dilde bir yazımı
yoktur**: `prin1` yazın.

Bunlar makrodur; çünkü `format`'ın değişken sayılı bağımsız değişkenleri tür değişkenlerini kabul etmez
ve türün çağrı yerinde bilinmesi gerekir.

## 3. Standart girdi ve standart akışlar

**Standart girdiyi okumak**, ayrılmış fonksiyonlarla değil, standart akış `*standard-input*` üzerindeki
`CharInput` metotlarıyla yapılır: `(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)` ([akış metotları](streams-files.md#2-metotlar)). Standart çıktı ve standart
hatanın da aynı şekilde `*standard-output*` / `*error-output*`'u vardır ve
`(write-line *standard-output* s)` gibi yazılabilir (`print`/`println`/`format`, biçim açılımına
ihtiyaç duyduğunuzda kısayollardır ve her zaman standart çıktıya yazar).

## 4. Pretty printer

Bu, CL'nin Lisp Pretty Printer'ına (CLHS 22.2) karşılık gelir. **Satır genişliğine sığmayan çıktıyı,
mantıksal blokları ve koşullu yeni satırları izleyerek böler.**

### 4.1 Denetim değişkenleri

Atanabilen global değişkenler. Bir kez `setf` yapıldığında sonraki tüm yazdırmaları etkilerler. Birini
geçici olarak değiştirmek için `dlet` kullanın (6.3).

| Değişken | Tür | Varsayılan | Anlamı |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | True ise `~a`/`~s`/`~w` ve pretty yönergeleri pretty-print yolunu izler |
| `*print-right-margin*` | `int` | `80` | Sağ kenar boşluğu (sütun olarak). 0, "boşluk yok, asla bölme" demektir. Negatif bir değer yazdırma hatasıdır |
| `*print-miser-width*` | `int` | `0` | Miser stilinin başladığı genişlik. 0, CL'nin `nil`'ine karşılık gelir (miser stili kapalı). Negatif bir değer yazdırma hatasıdır |

`pprint` ailesi ve `pprint-logical-block`, `*print-pretty*`'den bağımsız olarak her zaman pretty-print
yapar (CL'nin `pprint` tanımını izleyerek).

### 4.2 Hazır düzenler (özel formlar)

`print` gibi bunlar da özel formlardır; bu yüzden bağımsız değişken herhangi bir türde olabilir.

| Ad | Biçim | Açıklama |
|---|---|---|
| `pprint` | `(pprint x)` | Varsayılan düzenle pretty-print yapar. CL'deki gibi **önce bir yeni satır yazar** ve sonda yazmaz |
| `pprint-fill` | `(pprint-fill x)` | Her satırı sığdığı kadarıyla doldurur. Yeni satır yazmaz |
| `pprint-linear` | `(pprint-linear x)` | Tüm elemanlar tek satıra sığmıyorsa **satır başına bir eleman**. Yeni satır yazmaz |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Sütunları `colinc` geniş (varsayılan 16) bir tablo. Yeni satır yazmaz. Negatif bir `colinc` hatadır |

Varsayılan düzen (`pprint` ve `*print-pretty*` altındaki `~a`), CL'nin varsayılan
`*print-pprint-dispatch*`'ini izler: `(quote x)`'i `'x` olarak kısaltır ve `defun`/`let`/`if`/`lambda` gibi
kod formlarını "ilk satırda baş ve öngörülen sayıda bağımsız değişken, gövdenin geri kalanı iki sütun
girintili, satır başına bir form" olarak biçimlendirir. Diğer listeler doldurulur.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Mantıksal blokları kendiniz kurma

| Ad | Biçim | Açıklama |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Bir mantıksal blok açan özel form. `obj`, `pprint-pop`'un gezdiği listedir (hiçbiri gezilmiyorsa `()`). `:prefix` ve `:per-line-prefix` birbirini dışlar (CL'deki gibi) |
| `pprint-newline` | `(pprint-newline kind)` | Koşullu bir yeni satır. `kind`, `:linear` / `:fill` / `:miser` / `:mandatory`'dir |
| `pprint-indent` | `(pprint-indent kind n)` | Girinti. `kind`, `:block` (bloğun başından) / `:current` (geçerli sütundan)'dır |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Bir sekme. `kind`, `:line` / `:section` / `:line-relative` / `:section-relative`'dir. `colnum` ve `colinc` negatif olmayandır (negatifse hata) |
| `pprint-pop` | `(pprint-pop)` | Bloğun listesinden sıradaki elemanı alır (tükenmişse `()`) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Listenin tükenip tükenmediği |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Tükenmişse çevreleyen `loop`'tan `break` eder (bir makro) |

Mantıksal bloklar bir akış bağımsız değişkeni almaz: **açık bir mantıksal blok örtük durumdur**. En dıştaki
`pprint-logical-block` onu başlatır ve kapandığında bütün biçimlendirilip standart çıktıya bir kerede
yazılır. Açıkken `print`/`println`/`(format true ...)`/`pprint` çıktılarının hepsi o bloğa girer; bu
yüzden **içeriği sıradan `print` ile yazarsınız ve yalnızca bölünecek yerleri `pprint-newline` ve
benzerleriyle işaretlersiniz**; bu da kodu CL'dekiyle neredeyse aynı gösterir.

CL'de `pprint-exit-if-list-exhausted`, `pprint-logical-block`'tan yerel olmayan bir çıkıştır; burada
**çevreleyen `loop`'tan bir `break`'tir** (`pprint-logical-block` bir `block` kurmaz). CL'nin deyimi zaten
her zaman onu bir `loop` içine koyar; bu yüzden aynı okunur.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Koşullu yeni satırların kuralları (CLHS `pprint-newline`):

- `:mandatory` her zaman böler.
- `:linear`, çevreleyen mantıksal blok tek satıra sığmıyorsa böler. Karar blok başınadır; bu yüzden **bir
  bloğun tüm `:linear` yeni satırları birlikte bölünür** (`pprint-linear`'ın "hepsi tek satırda ya da
  satır başına bir eleman"ı budur).
- `:fill`, (a) sonraki bölüm satırın kalanına sığmıyorsa, (b) önceki bölüm tek satıra sığmadıysa ya da
  (c) miser stilinde blok tek satıra sığmıyorsa böler.
- `:miser`, yalnızca miser stilinde (blok sağ kenar boşluğunun `*print-miser-width*` yakınında
  başladığında) `:linear` gibi çalışır.

## 5. `print-object` (türe göre yazdırılan gösterim)

`impl print-object <tür>` yazmak, `print`/`println`/`format`/`pprint`'in o türdeki değerleri o
gerçekleştirmeyle yazdırmasını sağlar; **listelerin içinde iç içe olduklarında bile**. CL'nin
`print-object` genel fonksiyonuna (CLHS 22.1.4) karşılık gelir.

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Bağımsız değişken | Anlamı |
|---|---|
| `self` | Yazdırılacak değer |
| `escape` | CL'nin `*print-escape*`'i. `~s`/`prin1`/`pprint` için `true` (geri okunabilen bir biçim), `~a`/`princ` için `false` (insanlar için). Umursamayan bir gerçekleştirme onu yok sayabilir |

Döndürülen `string` doğrudan çıktıya gider. `impl`'i olmayan türler yerleşik gösterimle yazdırılır
(`#<point x: 1 y: 2>` biçiminde).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   works when nested too
```

Pretty printer ile de birleşir (4. bölüm). `*print-pretty*` true ise, gerçekleştirmenin döndürdüğü
string'leri içeren bir liste sağ kenar boşluğunda bölünür.

Standart kütüphanenin türlerinin yazdırılan gösterimleri. CL'de de bulunan türler SBCL'deki gibi
yazdırılır. REPL bir sonuç gösterdiğinde `~s` ile aynı gösterimi kullanır.

| Tür | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`, `#("a" "b")` | `#(1 2 3)`, `#(a b)` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Aynısı |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (sayı bir iç seri numarasıdır) | Aynısı |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Bir tamsayı (CL'nin `get-universal-time` / `get-internal-real-time` değeri) | Aynısı |
| Hata türleri (`ParseIntError`, `SimpleError` vb.) | `#<simpleerror "boom">` | Yalnızca mesaj (`boom`) |
| `complex` | `#C(1.0 2.0)` | Aynısı |
| `Array<T>` | `#2A((0 0) (0 0))` | Aynısı |
| Akışlar | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Aynısı |
| Soketler | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Aynısı |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (yaz saatinde sonda `dst`) | Aynısı |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Aynısı |
| `defstruct` türleri | `#<point x: 1 y: 2>` (alan adları ve değerleri) | Aynısı (alanlar `~a` ile) |

Kurallar:

- **Kayıt statiktir.** Bir `impl`, sıradan bir metot tanımı gibi tür denetiminden geçer; bu yüzden yanlış
  yazılmış bir tür adı ya da yanlış bir imza derleme hatasıdır.
- **Jenerik türler için de çalışır.** `(impl print-object box<T> (where (print-object T)) ...)`, her tür
  bağımsız değişkeni için ayrı bir gövdeye gider: bir değer türünü tür bağımsız değişkenleri dahil
  hatırlar (`box<i32>`). `Vector<T>` gibi yerleşik jenerik türler de aynı şekilde çalışır.
- **Seçim yazdırma zamanında yapılır.** Hangi yönergenin hangi bağımsız değişkeni tükettiği, denetim
  string'inin çalışma zamanı içeriğine bağlıdır; bu yüzden `~a` ile `~s` arasındaki ayrım (yani `escape`)
  yalnızca yazdırma anında bilinir. Bu, `print-object` metotlarının "sınıf başına tanımlanıp yazdırma
  zamanında seçildiği" CLOS ile aynıdır.
- **Yeniden giriş yerleşik gösterime düşer.** Bir gerçekleştirme kendini `(format false "~a" self)` ile
  yazdırırsa sonsuza dek özyinelerdi; bu yüzden yazdırılmakta olan bir değer yeniden göründüğünde
  yerleşik gösterim kullanılır. Bu, bir derinlik sınırına değil, değer kimliğine bakar; bu yüzden iç
  içe kendine başvuran yapıların meşru biçimde yazdırılmasının önüne geçmez.
- **Her skaler tür bu trait'i gerçekleştirir.** Bu, **sınır olarak kullanılabilmesi içindir**:
  `format`'ın değişken sayılı bağımsız değişkenleri tür değişkenlerini alamaz; bu yüzden jenerik kodun
  "bilinmeyen türdeki değerler görüntülenebilir" demesinin tek yolu bu sınırdır (Rust'ın `T: Display`'i
  ile aynı biçim). `Array<T>`'nin `print-object`'i bir örnektir.
- **Sınırı karşılamayan tür bağımsız değişkenleriyle yerleşik gösterim sessizce kullanılır.**
  `(impl print-object Array<T> (where (print-object T)))`, `Array<i32>` için geçerlidir, ancak elemanları
  `print-object`'siz bir `defstruct` olan bir `Array` için geçerli değildir. Yalnızca bir dizi oluşturmanın
  hata olması anlamsız olurdu; bu yüzden hata değildir.
- CL'nin diğer mekanizması olan `set-pprint-dispatch` / `*print-pprint-dispatch*` (tür belirleyicileriyle
  anahtarlanan bir çalışma zamanı kayıt defteri) **benimsenmemiştir**. Kayıtları denetlenmez; bu da
  statik tür denetimli bir dile uymaz.

## 6. Ne kadarının yazdırılacağını denetleme

### 6.1 Derinlik, uzunluk ve paylaşım

"Bir değerin ne kadarının yazdırılacağına" karar veren CLHS 22.1.1'in denetim değişkenleri. 4.1'deki üçü
gibi bunlar da atanabilen globallerdir ve `*print-pretty*` true olsun olmasın `print`/`println`/`format`/
`pprint`'in hepsine uygulanır.

| Değişken | Tür | Varsayılan | Anlamı |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Bu derinlikte ya da daha derinde iç içe olan nesneler `#` ile değiştirilir. Yazdırılan nesne 0. derinliktedir. 0 sınırsız demektir |
| `*print-length*` | `int` | `0` | Liste elemanlarını (ve `defstruct`/`defenum` değerlerinin alanlarını) bu sayıya kadar yazdırır ve geri kalanını `...` ile değiştirir. 0 sınırsız demektir |
| `*print-circle*` | `bool` | `false` | True ise değer yazdırmadan önce taranır ve **iki ya da daha fazla kez görünen nesneler etiket alır**. İlk görünüm `#n=…`, sonrakiler `#n#` olur |

CL "sınırsız" için `nil` kullanır, ancak bu dilde `nil` yoktur; bu yüzden `*print-right-margin*`'de
olduğu gibi **0 sınırsız demektir**. Negatif değerlerin anlamı yoktur ve yazdırma hatalarıdır.
Varsayılanların hepsi "sınır yok / etiket yok"tur ve CL'nin başlangıç değerleriyle eşleşir.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Döngüsel yapılar yalnızca `*print-circle*` true iken yazdırılabilir.** Kendine işaret eden bir değeri
false iken (varsayılan) yazdırırsanız, yazıcı döngüyü izlemeye devam eder ve süreç çöker. CL de aynıdır
(CLHS, `*print-circle*` false iken döngüsel yapıların yazdırılmasını tanımsız bırakır).

Bir döngü yalnızca "bir `defstruct` alanını `setf` ile kendine işaret ettirmekle" yapılabilir (`Sexpr`
hücreleri oluşturulduktan sonra değiştirilemez; bu yüzden `'(1 2 3)` gibi bir liste asla döngüsel olamaz):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a points to a itself
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Etiketler **yazdırılan her şey için 1'den yeniden başlar** (CL'deki gibi). Döngü olmasa bile aynı nesne iki
kez görünürse `#1=`/`#1#` alır; bu da CL'nin belirttiği gibi "bu ikisi aynı nesne" bilgisini çıktıda
korur:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Paylaşımı olmayan bir değer **hiç etiket göstermez**; bu yüzden bu değişkeni true bırakmak gündelik
kodun çıktısını değiştirmez.

### 6.2 Taban, harf büyüklüğü ve okunabilirlik

| Değişken | Tür | Varsayılan | Anlamı |
|---|---|---|---|
| `*print-base*` | `int` | `10` | Tamsayıları (sabit genişlikli ve `int`) yazdırma tabanı. 2 ile 36 dışında bir **yazdırma hatasıdır** (CL de aralığı belirtir) |
| `*print-radix*` | `bool` | `false` | True ise bir taban işareti ekler: `#b`/`#o`/`#x`, diğer tabanlar için `#NNr` ve 10 tabanı için sondaki `.`. İşaret, işaretin **önüne** gelir (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | Sembol adlarının harf büyüklüğü: `:upcase` / `:downcase` / `:capitalize` (CL ile aynı yazımlar). Başka herhangi bir sembol yazdırma hatasıdır |
| `*print-readably*` | `bool` | `false` | True ise geri okunabilen bir biçimde yazdırır. Kaçışı zorlar ve `*print-level*`/`*print-length*` kesmelerini devre dışı bırakır |
| `*print-lines*` | `int` | `0` | Pretty printer'ın kullanabileceği satır sayısı. Fazlası kesilir ve CL'deki gibi sonda `..` olur. 0 sınırsız demektir. Negatif bir değer yazdırma hatasıdır |
| `*print-escape*` | `bool` | `true` | `write`/`write-to-string`'in `prin1` mi `princ` mi yapacağı. **Yalnızca bu ikisi onu okur** |
| `*print-array*` | `bool` | `true` | `Vector<T>` ve `Array<T>`'nin içeriğini gösterip göstermediği. Doğruysa CL'nin dizi sözdizimi (`#(1 2 3)` / `#2A((1 2) (3 4))`); yanlışsa yalnızca tür ve şekil, `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

`*print-radix*`'in eklediği işaretler okuyucu tarafından geri okunabilir
([Sözdizimi Başvurusu](../syntax.md#1-sözcüksel-öğeler)'ndaki taban gösterimi).

**`*print-case*`'in varsayılanının CL'den neden farklı olduğu**: CL'nin varsayılanı `:upcase`'tir; çünkü
CL'nin okuyucusu sembol adlarını büyük harfle saklar, yani "saklandığı gibi" anlamına gelir. Bu okuyucu
onları küçük harfle saklar; bu yüzden aynı anlama gelen varsayılan `:downcase`'tir.

**`*print-readably*`'in eksik yarısı**: CL, geri okunamayan değerler için `print-not-readable` bildirir,
ancak bu dilde bildirilecek bir koşul yoktur ve `print-object`'in istediği gibi yazdırabildiği kullanıcı
türleri için okunabilirliğe karar vermenin bir yolu yoktur. Yalnızca zorunlu kaçış ve kesmelerin geçersiz
kılınması vardır.

**Neden yalnızca `write` `*print-escape*`'i okur**: CLHS'nin belirttiği gibi `~s`/`prin1`/`pprint` onu
true'ya, `~a`/`princ` ise false'a bağlar; her biri yalnızca kendi çağrısının süresi boyunca. Bu yüzden onu
bağlanmamış gören tek okuyucular `write`/`write-to-string`'dir. Bir `print-object` gerçekleştirmesi bu
global yerine kendi `escape` bağımsız değişkenini okumalıdır: o bağımsız değişken yönergenin seçtiği değeri
taşır.

**CL'de olup bu dilde olmayan**: `*print-gensym*` (internlenmemiş semboller yoktur).

### 6.3 Geçici geçersiz kılmalar

CL bunları `let` ile bağlar, ancak bu dilde `let` sözcüksel olarak bağlar; bu yüzden `dlet` kullanın
([Diğer](system.md#10-diğer)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; the limits apply to this one print only
(with-standard-io-syntax (println "~a" x))   ; print with everything back at the standard values
```

`with-standard-io-syntax`, gövdesini tüm yazıcı denetim değişkenleri standart değerlerinde ve
`*read-eval*` `true` olarak ayarlı çalıştırır.
