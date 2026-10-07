<!-- translated-from: docs/ja/reference/functions/system.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Zaman, Ortam ve Gerçekleştirim

Zaman, çalışma zamanı ortamı hakkındaki sorgular, gerçekleştirim araçları, metin ayrıştırma ve
değerlendirme, docstring'ler ve makrolar için fonksiyonlar.

## 1. Zaman

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `universal-time` | — | `defstruct` | İki alan: `day` (1900-01-01'den bu yana gün) ve `second` (o gün içindeki saniye, 0..86399) |
| `internal-time` | — | `defstruct` | İki alan: `second` ve `microsecond` (o saniye içinde, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | CL'nin başlangıç anından (1900-01-01 UTC) bu yana zaman |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Sürece göre geçen zaman |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | Bu sürecin kullandığı **CPU zamanı** (kullanıcı artı sistem) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Saniye sayısı olarak. İki okuma arasındaki farkı bildirmek için biçim |
| `internal-time-units-per-second` | — | `int` | `1000000` (mikrosaniye), `microsecond` alanının birimi. CL'deki gibi değer gerçekleştirimin seçimidir |
| `time` | `(time form)` | Makro | `form`'u çalıştırır, gerçek zamanı ve CPU zamanını birer satırda yazdırır ve `form`'un değerini olduğu gibi döndürür |

Gerçek zaman ve CPU zamanı farklı şeyler söyler. Çoğunlukla G/Ç bekleyen işler için ikisi arasında büyük
fark olur ve tam da bilmek istediğiniz şey bu farktır; bu yüzden `time` ikisini de gösterir.

Bir task'i durduran `sleep`, [Task'ler ve Kanallar](concurrency.md#3-yield--sleep--yol-verme)
belgesindedir.

## 2. Tarihleri çözme ve kodlama

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Dokuz alan**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. CL'nin dokuz dönüş değeri tek bir struct olarak (çoklu değerler yoktur) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Evrensel zamanı takvim bileşenlerine çevirir. `zone`, Greenwich'in batısındaki saat sayısıdır (CL ile aynı yön). **Atlanırsa yerel zamandır** (CL'deki gibi) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | Tersi. `zone` olmadan bağımsız değişkenler **yerel zaman** olarak okunur |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Şimdi, yerel zamanda çözülmüş |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | O evrensel zamanda yerel zamanın Greenwich'in batısındaki farkı, **saniye** cinsinden |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | O evrensel zamanda yaz saati uygulamasının geçerli olup olmadığı |

CL'deki gibi `day-of-week` için **0 Pazartesi ve 6 Pazar'dır**.

**`zone` olmadan yerel zaman kullanılır**, CL'deki gibi. Yerel fark işletim sistemine sorulur; bu yüzden
sonuç makinenin bulunduğu yere bağlıdır. **Açık bir zone vermek sonucu belirlenimci yapar** ve `0`
UTC'dir.

`zone`'un birimi, CL'deki gibi "Greenwich'in batısındaki saat"tir; bu yüzden UTC+9, `-9` olarak okunur.
Ancak **bağımsız değişken bir tamsayıdır ve sonucun `zone` alanı bir `f64`'tür**. Gerçek farklar her
zaman tam saat değildir (Hindistan +5:30, Nepal +5:45) ve bildirilen değeri yuvarlamak sessizce bir
yalan söylemek olurdu. Elle yazdığınız bir zone tam sayıda saattir; bu yüzden bağımsız değişken `int`'tir.

`zone` verildiğinde `daylight-p` `false`'tur ve `zone`, CL'nin belirttiği gibi tam olarak verilen
değerdir (*Bir time-zone verilirse yaz saati bilgisi yok sayılır*).

Bir yaz saati geçişinin içine düşen yerel zaman, baştan beri benzersiz değildir ve CL hangisinin
alınacağını söylemez. `encode-universal-time`, böyle bir zaman için iki yanıttan birini döndürür.

## 3. Çalışma zamanı ortamı

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | Komut satırı. **0. eleman program adıdır** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Bir ortam değişkeni. Ayarlı değilse ya da UTF-8 değilse `none` |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. `user-homedir-pathname`'in temeli ([Yol adları](streams-files.md#92-fonksiyonlar)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | Gerçekleştirimin sürümü |
| `machine-type` | `(machine-type)` | `()→string` | CPU mimarisi (`x86_64` / `aarch64` …). **Derleme hedefinin** değeri |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | Host adı |
| `machine-version` | `(machine-version)` | `()→Option<string>` | **Şu anda çalışan** donanımın adı (`Apple M1` / `Intel(R) Xeon(R) …`). Belirlenemediği yerde `none` |
| `software-type` | `(software-type)` | `()→string` | İşletim sistemi (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | İşletim sistemi sürümü (`uname -r`, örneğin `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Kurulum yerinin kısa bir adı. **Her zaman `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Aynı şekilde, uzun bir ad. **Her zaman `none`** |

`Option` döndürenler, CL'nin `NIL`'e izin verdiği öğelerdir (*ya da böyle bir ad belirlenemiyorsa nil*).
POSIX'te site adlarını kaydedecek bir yer yoktur; bu yüzden her zaman `none`'dır; SBCL de aynısını
döndürür. `machine-type` ile `machine-version` arasındaki farka dikkat edin: birincisi bu ikili dosyanın
**derlendiği** mimaridir, ikincisi onu şu anda **çalıştıran** yongadır.

`command-line-args`'ın 0. elemanı, `typl script.typl a b` için betiğin yoludur ve `./prog a b` olarak
çalıştırılan bir AOT çalıştırılabilir dosyası için çalıştırılabilir dosyanın kendisidir. **Her iki
çalıştırma yolu da aynı bağımsız değişkenleri aynı indekslerde okur** (`typl`, bunları iletmeden önce
kendi adını ve `--heap-cells` gibi seçenekleri kaldırır).

## 4. Kullanıcıya sorma

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Tek bir `y` / `n` alır. Biri gelene kadar yeniden sorar |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Kullanıcıya `yes` / `no`'yu harf harf yazdırır. Hatanın pahalı olduğu sorular için |

İkisi de `*standard-input*`'tan okur. Yeniden sormayı yalnızca girdinin sonu durdurur ve o zaman sonuç
`false`'tur.

## 5. Gerçekleştirim araçları (CLHS 25.2)

Gerçekleştirimin kendisi hakkındaki soruları yanıtladığı katman. `heap-info` / `room` / `dribble`
sıradan fonksiyonlardır; `trace` / `untrace` / `step` / `disassemble` / `ed` **özel formlardır**
(`trace` / `untrace` / `disassemble` / `ed` bir tanımın *adını*, `step` ise bir *formu* alır; hepsi
değerlendirilmez).

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | Heap'in geçerli durumu bir struct olarak. `room`'un yazdırdığı sayıların aynısı |
| `room` | `(room &optional verbose)` | `(bool)→()` | `heap-info`'yu `*standard-output*`'a bildirir. `(room true)` daha fazla ayrıntı verir |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Oturumun çıktısını `path`'e kaydetmeye başlar / bağımsız değişkensiz çağrıldığında kaydı durdurur |
| `trace` | `(trace name...)` | `Sexpr` | Adı verilen tanımların çağrılarını `*trace-output*`'a bildirir. Şu anda izlenen adların listesini döndürür |
| `untrace` | `(untrace name...)` | `Sexpr` | Bildirmeyi durdurur. **Bağımsız değişken olmadan hepsini kaldırır** |
| `step` | `(step form)` | `form`'un türü | `form`'u değerlendirir ve her çağrıda durup sorar |
| `disassemble` | `(disassemble name [llvm])` | `()` | O tanımın neye dönüştüğünü yazdırır. Varsayılan olarak host makine kodu, `true` ile LLVM IR |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | `$VISUAL` / `$EDITOR`'ı başlatır. Bir ad verilirse o tanımın yazıldığı satırı açar |

`trace`/`untrace`/`step`/`disassemble` yalnızca yorumlayıcıya özgüdür ve onları çağıran fonksiyonlar
derlenemez ([Sözdizimi Başvurusu 10. bölüm](../syntax.md#10-derleme)).

### 5.1 `heap-info` alanları

| Alan | Tür | İçerik |
|---|---|---|
| `capacity` / `live` / `free` | `int` | Tüm cons arenası ve dökümü. Her zaman `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | Heap'in diğer üç nesne türünün geçerli sayıları |
| `gc-count` | `int` | Gerçekleştirim başladığından beri toplama sayısı |
| `growable` | `bool` | Arenanın hâlâ büyüyüp büyüyemeyeceği |

Alanların hepsi `int`'tir (`growable` hariç). Büyüme sınırı (`typl --heap-cells` açıklamasına bakın)
bildirilmez; çünkü okuyucunun bilmek istediği, hâlâ büyüyüp büyüyemeyeceğidir (`growable`).

### 5.2 `trace` / `step` neyi görebilir, neyi göremez

- **Gövdesi derlenmiş tanımlar da, yorumlanan çağrı yerlerinden görülebilir.**
- **Derlenmiş kodun *içindeki* çağrı yerleri görülemez.** Derlenmiş gövdesi olan bir adı izlemek, bunu
  söyleyen tek satırlık bir not ekler. SBCL'in yerel çağrılar için anlattığı aynı kısıtlama.
- **Closure değerleri üzerinden yapılan çağrılar (`funcall`/`apply`) görülemez.** Closure'ların adı yoktur.
- **Jenerik tanımlar kapsanmaz.** Her tür için bir kopya, kullanıldığı her yerde yapılır; bu yüzden
  adlandırılacak tek bir gövde yoktur (`compile` reddettiğinde olduğu gibi aynı neden ve aynı ifade).

`step`'in komutları `s` (bu çağrının içine gir; boş bir satır da aynısını yapar), `n` (bu çağrıyı
atla), `c` (buradan sonra sormayı bırak) ve `q`'dur (iptal). **Standart girdi bir terminal değilse
`step` yalnızca `form`'u değerlendirir**: CLHS'nin açıkça izin verdiği dejenere bir davranış; böylece
betikler ve testler kimsenin yanıtlayamayacağı bir istemde takılmaz.

`ed`'in `$VISUAL` / `$EDITOR`'ı boşluklardan bölünür; bu yüzden `EDITOR="code -w"` çalışır. İkisi de
ayarlı değilse sonuç `Err`'dir: `vi`'yi tahmin etmez. Satır numarası, `+N` biçiminde ilk olarak geçirilir.

`dribble`, oturumun çıktısının süreçten çıktığı üç yolun hepsini kaydeder: `print`/`println`/`format`'ın
yazdıkları, standart çıktıya bağlı akışlara yazılanlar ve REPL'e yazılan satırlar ile REPL'in geri
yazdırdığı değerler.

## 6. Ayrıştırma ve değerlendirme

Bunların hepsi çalışma zamanından gelen metni ve veriyi ele alır (programın kendisinin denetlemediği);
bu yüzden başarısızlıkta panic olmak yerine bir `Result`'ın `Err`'ini döndürürler. Hata türleri işlem
başına somut türlerdir ([Hata türleri](option-result.md#3-hata-türleri-ve-error-traiti)).

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | CL'nin `parse-integer`'ı. Baştaki ve sondaki boşlukları atlar (`trim` ile aynı küme), en çok bir `+`/`-` işareti, sonra `radix` tabanında (varsayılan 10, 2 ile 36; 10'un üzerindeki basamaklar iki harf büyüklüğünde de) basamakları okur. Basamak sayısına sınır yoktur (`int`). Artakalan diğer karakterler `Err` verir. `:junk-allowed true` ile ilk basamak olmayan karakterde durur ve geri kalanı yok sayar, ancak tek bir basamak bile yoksa `Err` verir (CL'nin `nil`'ine karşılık gelir). CL'nin ikinci değerini (okumanın bittiği konum) döndürmez. Aralık dışı bir `radix` panic olur (metindeki değil, çağıranın hatası) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Bir kayan noktalı sayı. `inf`/`nan` de kabul eder |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | `s`'den bir `Sexpr` okur (kaynak kodu okuyanla aynı okuyucuyla). Dengesiz parantezler, sonlandırılmamış string'ler vb. `Err` verir. Bir akıştan okumak `read-sexpr`'dir ([Akışlar](streams-files.md#6-jenerik-fonksiyonlar-ve-dosya-işlemleri)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` artı **okumanın bittiği konum**. `(car r)` değer, `(cdr r)` okunacak sonraki karakterin konumudur. `start` varsayılan olarak 0'dır |
| `read-from-string-preserving-whitespace` | Yukarıdakiyle aynı | Yukarıdakiyle aynı | Aynısı, ancak veriyi bitiren boşluğu tüketmez. Fark döndürülen konumda görünür |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | `form`'un türlerini çalışma zamanında denetler ve değerlendirir. CL'nin `eval`'ini izler |

CL, `read-from-string`'ten **iki değer** (değer ve konum) döndürür, ancak bu dilde çoklu değerler
yoktur; bu yüzden tek bir `cons-cell` döndürür. Konuma sahip olmak, bir string'i her seferinde bir veri
olarak okumayı yeniden tarama değil, bir döngü yapar:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

`preserving-whitespace`'in yarattığı fark **bir boşluk karakteridir**: CL'nin `read`'i veriyi bitiren
boşluğu tüketir ve `read-preserving-whitespace` onu bırakır. `(read-from-string "12 34")` 3 konumunu
döndürür ve koruyan sürüm 2 döndürür.

Okuyucunun kabul ettiği sayı sözdizimi [Sözdizimi Başvurusu 1. bölüm](../syntax.md#1-sözcüksel-öğeler)'dedir.
`*print-radix*`'in ([Yazdırma](printing.md#62-taban-harf-büyüklüğü-ve-okunabilirlik)) yazdırdığı şey olduğu
gibi geri okunabilir. CL'nin `*read-base*`'i yoktur.

### 6.1 `eval` ne demektir

CLHS `eval`'ini izler: **geçerli global ortamda** (global fonksiyonlar, değişkenler, türler ve makrolar;
çalışma zamanında eklenen tanımlar dahil) ve **boş sözcüksel ortamda** (çağıranın `let`/`lambda`
yerel bağlamaları görünmez) değerlendirir. Hem ifadeler hem tanımlar
(`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`) değerlendirilebilir ve tanımlar global ortama hemen ve
kalıcı olarak kaydedilir.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; the global x is visible
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; returns the defined name
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; the definition just made is visible
```

- **Dönüş değeri**: bir ifade için sonuç bir `Option<Sexpr>` olarak; bir tanım için tanımlanan adın sembolü
  (CL'deki gibi). Sonucu kullanmak için `Sexpr`'i `match` ile ayrıştırın
  (`(int n)`/`(str s)`/…).
- **Statik türlerden kaynaklanan farklar (önemli)**: CL sonucun gerçek değerini döndürür, ancak bu dilde
  dönüş türü yalnızca tekdüze olarak `Result<Option<Sexpr>,EvalError>` olabilir. Ayrıca **statik olarak
  yazılmış kod, `eval`'in çalışma zamanında tanımladığı adlara ileri başvuru yapamaz**: doğrudan bir
  dosyaya yazılan bir `(sq 9)`, `sq`'yu tanımlayan `eval` çalışmadan önce denetlenir ve "tanımsızdır".
  Ancak **sonraki `eval`'ler onu görebilir** (tür denetimleri çalışma zamanında, tanımdan sonra çalışır).
  REPL her seferinde bir satırı denetler ve çalıştırır; bu yüzden `eval` ile tanımlanan bir ad sonraki
  satırdan doğrudan çağrılabilir.
- **Hatalar**: tür hataları ve sözdizimi hataları `Err` döndürür (panic olmaz). Değerlendirilen koddaki
  **çalışma zamanı panic'leri** (sıfıra bölme vb.) doğrudan yazılmış koddaki gibi yayılır. Aradaki
  herhangi bir `unwind-protect` temizliği çalışır
  ([Sözdizimi Başvurusu 8. bölüm](../syntax.md#8-yerel-olmayan-çıkışlar-catch--throw--unwind-protect)).
- **Ad alanı**: `typl file.typl` ile ve bir AOT çalıştırılabilir dosyasının içinde çalıştırıldığında
  `eval`, betiğin modülünün ad alanında değerlendirir (betiğin kendi globalleri görünür). REPL kök ad
  alanında değerlendirir.
- **Derleme**: hem `read` hem `eval` derlenebilir. AOT çalıştırılabilir dosyalarında nasıl ele
  alındıkları ve sonuçları (eval'e geçirilen formlar yorumlanır)
  [Sözdizimi Başvurusu 10.2](../syntax.md#102-aot-çalıştırılabilir-dosyalarda-eval)'dedir.

## 7. Docstring'ler / `documentation`

`defun`/`defmethod` (`impl` içindekiler dahil)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` docstring taşıyabilir. Konum, her biri için CL'nin kuralını izler:

| Form | Docstring'in konumu |
|---|---|
| `defun` / `defmethod` / `defmacro` | Gövdenin başında (dönüş türünden ve `where` yan tümcesinden sonra). Yalnızca en az bir gövde formu izlediğinde; tek başına bir string dönüş değeri olarak kalır |
| `defvar` / `defconstant` | Başlangıç değerinden **sonra**: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | Adın **hemen ardından**, alanlardan/varyantlardan önce |
| `deftype` | Adın **hemen ardından**, türden önce: `(deftype meters "doc" i32)` |
| `deftrait` | Üst trait listesinin hemen ardından, öğelerden önce. Tüm trait için bir tane. **Varsayılan gerçekleştirmesi olan metotlar** kendi docstring'lerini gövdelerinden hemen önce koyabilir |

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `documentation` | `(documentation name)` | (özel form; `name` çıplak bir semboldür ya da `Type::method`)→`Option<string>` | `name`'in docstring'ini döndürür |

`quote`/`compile` gibi `documentation` da bir özel formdur (`name`'i değerlendirilmemiş bir ad olarak
okur). CL'nin `(documentation 'name 'function)`'ından farklı olarak bir tür bağımsız değişkeni almaz;
bunun yerine çıplak bir adı **değişken → fonksiyon → tür → trait → makro** sırasıyla çözümler (bir ifade
olarak değerlendirilen çıplak bir tanımlayıcıyla aynı öncelik). `Type::method` biçimi, ilişkili ya da
statik bir metodun docstring'ini arar.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**Değer denetim zamanında belirlenir**: ad hiçbir tanıma çözümlenmiyorsa bu bir denetim zamanı hatasıdır
(tanımsız bir değişkene başvurmak gibi). Çözümleniyor ama docstring yoksa sonuç `Option::none`'dır.

**Kapsanmayanlar**:

- `(setf documentation)` (bir docstring'i çalışma zamanında değiştirmek) yoktur.
- Modül nitelikli serbest adlar (`mod::name`; `Type::method` desteklenir) desteklenmez.
- Bir `deftrait` içindeki **gövdesiz** bir metot bildiriminin docstring'i olamaz. Sondaki bir string
  sabiti, bir varsayılan gerçekleştirmenin gövdesi (dönüş değeri) olurdu; bu yüzden ikisini ayırt etmenin
  bir yolu yoktur.

Dil sunucusunun (`typl-lsp`) üzerine gelme özelliği de docstring'leri gösterir.

## 8. Makrolar

Makroların nasıl tanımlanacağı
[Sözdizimi Başvurusu 3.14](../syntax.md#314-defmacro--makro-tanımları)'tedir.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Yeni bir sembol. Adı `" <prefix><n>"`'dir; burada `n`, `*gensym-counter*`'dır. Baştaki bir boşluk kaynakta yazılamaz; bu yüzden üretilen bağlamalar yazılmış adlarla asla çakışmaz |
| `*gensym-counter*` | Değişken | `int` | `gensym`'in sonraki kullandığı sayı. CL'deki gibi okunabilir ve ayarlanabilir |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Bir makro çağrısını bir adım açar. `none`, "bir makro çağrısı değil" demektir |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Artık bir makro olmayana kadar tekrarlar |

`macroexpand-1` bir `Option` döndürür. CL "açılıp açılmadığını" ikinci bir dönüş değeri olarak bildirir,
ancak çoklu değerler yoktur; bu yüzden bu rolü `none` oynar. **Kendi çağrısına açılan bir makro asla bir
makro olmayanla karıştırılamaz.** Bir adımlık açılım, tür denetleyicisinin kullandığıyla aynıdır; bu
yüzden programın gördüğü ile denetleyicinin gördüğü asla ayrışmaz.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none shows as the empty list (Option<Sexpr> is transparent)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

CL'de olup bu dilde olmayanlar: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute` her zaman
çakışır; bu yüzden seçilecek bir ayrım yoktur), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (internlenmemiş semboller; bağlamalar adla aranır; bu yüzden
kazanılacak bir şey olmazdı).

## 9. Yerel makro bağlamaları (`macrolet` / `symbol-macrolet`)

İkisi de **değer olmayan adları** sözcüksel olarak bağlayan özel formlardır. Çalışma zamanında hiçbir şey
kalmaz: derlenen şey gövdenin açılmış biçimidir.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Bir `macrolet` bağlaması, aynı adlı global bir makroyu **yalnızca gövde süresince** gizler. Lambda
  listesi `defmacro`'dakiyle aynıdır (`&optional`/`&rest`/`&key`).
- **Aynı `macrolet`'in kardeşleri, *gövdelerinden* birbirlerini göremez** (CL'deki gibi; `labels`'tan
  fark budur). Açılımlar kullanım yerinde denetlenir; bu yüzden `earlier`'ın `(later ...)`'e açılması
  çalışır: ikisi de o yerde görünür.
- Bir `symbol-macrolet` adı ortama sıradan bir bağlama olarak girer. Bu yüzden iç bir `let` aynı adı gizler
  ve dıştaki bir değişken gizlenir: CL'nin kuralları olduğu gibi çıkar.
- **`setf` açılıma yazar.** `(setf head 42)`, `(setf (get v 0) 42)`'dir.
- Açılımlar **kullanım yerinin ortamında** denetlenir (bağlama yerinin değil).

## 10. Diğer

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | False ise panic olur. Mesaj olmadan `assertion failed: <yazıldığı gibi sınama>` (bir makro olduğundan ifadenin kendisini adlandırabilir). CL'nin yeniden başlatmaları (restart) bu dilde yoktur |
| `warn` | `(warn control args...)` | `(string,...)→()` | `*error-output*`'a `WARNING: ` önekli bir satır yazar ve **devam eder**. Bir `Result` döndürmeden ve programı bitirmeden bir şeyi bildirmenin yolu |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Globalleri yalnızca `body` süresince değiştirir ve çıkışta geri yükler. CL bunu `let` olarak yazar, ancak bu dilde `let` her zaman sözcüksel olarak bağlar; bu yüzden ayrı ad (aynı adlı Emacs Lisp makrosuyla aynı rol). Gövdeden nasıl çıkılırsa çıkılsın geri yükler: normal tamamlanma, `throw`, `panic`, `break`/`return`. **Task başına bir bağlama değildir** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | `body`'yi her yazıcı denetim değişkeni standart değerinde ve `*read-eval*` `true` olarak ayarlı çalıştırır ([Yazdırma](printing.md#6-ne-kadarının-yazdırılacağını-denetleme)) |
| `exit` | `(exit code)` | `int→!` | Süreci bitirir |
| `dump` | `(dump path)` | `string→bool` | Geçerli ortamı (tür bilgisi artı derlenmiş gövdeler) tek bir dosyaya yazar. `typl --image <path>` ondan yeniden başlar. Yalnızca yorumlayıcıya özgü ([Sözdizimi Başvurusu 10.1](../syntax.md#101-dumplar)) |
