<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Akışlar ve Dosyalar

Akış trait'leri ve metotları, somut akış türleri, dosya işlemleri ve yol adları. Ağ soketleri de
akışlardır ve [Ağ](network.md) belgesinde ele alınır.

## 1. Trait hiyerarşisi

CL'nin bir sınıf hiyerarşisiyle ifade ettiği şey burada bir **trait hiyerarşisiyle** ifade edilir. Hem
yön (girdi / çıktı) hem de eleman türü **statik olarak** belirlenir; bu yüzden çalışma zamanında "bu akış
okunabilir mi?" diye sormaya gerek yoktur.

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; character input
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; character output
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; input that can push back one character
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; byte input
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; byte output
```

Karakter okuyan bir fonksiyon, `(where (CharInput S))` ya da `:dyn CharInput` alıyorsa, yerleşik ya da
kullanıcı tanımlı her akış türünü kabul eder.

## 2. Metotlar

`CharInput`'un her metodunun varsayılan bir gerçekleştirmesi vardır. Bir gerçekleştirme yalnızca
`read-item` yazar.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | Sıradaki eleman. Sonda `none`. **Gerçekleştirilmesi gereken tek metot** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Sıradaki karakter |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Sıradaki yeni satıra kadar (yeni satır tüketilir ve çıkarılır). Yeni satırla bitmeyen son satır da döndürülür |
| `read-all` | `(read-all s)` | `(S)→string` | Kalan her şey |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Yalnızca zaten elde olan bir karakter. Beklemek yerine `none` |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | `v`'ye en fazla `n` karakter iter ve gerçekte kaç tanesinin okunduğunu döndürür. `n`'den azı yalnızca sonda |

`listen`, `InputStream`'dedir (`CharInput`'un ebeveyni):

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Sonraki okumanın beklemeden yanıtlanıp yanıtlanamayacağı. Varsayılan `false`'tur, **asla yalan olmayan taraf**: `true` bir tahmin olurdu ve yanlış bir tahmin `read-char-no-hang`'ı bloke ederdi. Tüm yerleşik akışlar onu geçersiz kılar. **Onu geçersiz kılmayan kullanıcı tanımlı akışlar için `read-char-no-hang` her zaman `none` döndürür** |

`PeekInput` (`CharInput`'tan miras alır) **bir karakteri geri itme** ekler. Geri itilen karakteri tutacak
yer yalnızca akışın kendisinde vardır; bu yüzden bunun varsayılan bir gerçekleştirmesi olamaz ve ayrı bir
trait'tir. `file-stream`/`string-input-stream`/`standard-stream` onu gerçekleştirir ve diğer her akış
`make-peek-stream` ile sarıldığında onu alır (4. bölüm).

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Sonraki okumanın `c` döndürmesini sağlar. **Gerçekleştirilmesi gereken tek metot**. CL'deki gibi yalnızca bir karakter garanti edilir |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Sıradaki karaktere tüketmeden bakar |

Aynı şekilde `CharOutput` için bir gerçekleştirme yalnızca `write-item` yazar.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Bir eleman yazar. **Gerçekleştirilmesi gereken tek metot** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Bir karakter yazar |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Bir string yazar |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Bir string ve bir yeni satır |
| `terpri` | `(terpri s)` | `(S)→()` | Bir yeni satır (CL'nin adı) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Satır başında değilse bir yeni satır |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Yazılacak sonraki karakterin bir satır başlatıp başlatmayacağı. Varsayılan `false`'tur (bu yüzden `fresh-line` yeni satırı yazar: şüphede yazmak güvenli taraftır). Tüm yerleşik akışlar onu geçersiz kılar |
| `finish-output` | `(finish-output s)` | `(S)→()` | Tamponu boşaltır |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | `v`'nin tüm karakterlerini sırayla yazar |

`at-line-start` **yalnızca o akış üzerinden yazılanı** hatırlar. `print`/`println`/`(format true ...)`
standart çıktıya `*standard-output*`'tan geçmeden yazar; bu yüzden ikisini karıştırırsanız
`(fresh-line *standard-output*)`, `println`'in yazdığı yeni satırlardan haberdar olmaz. Birine bağlı
kalın.

`Stream`, tüm akışlar için ortaktır:

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Hâlâ açık olup olmadığı |
| `close` | `(close s)` | `(S)→()` | Kapatır. **GC akışları kapatmaz**; bu yüzden bunu açıkça (ya da `with-open-file` ile) yapın |

## 3. Somut akış türleri

| Tür | Nasıl yapılır | Gerçekleştirilen trait'ler |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction`, `direction-input` / `direction-output` / `direction-append` üç sabitinden biridir.
`open-file`, dosya açılamazsa `Err(FileError)` döndürür (eksik bir dosya sıradan bir sonuçtur, panic
değil). Dosya adı bir string ya da bir `pathname` olabilir (9. bölümdeki `Pathish`).

`(get-output-stream-string s)`, bir `string-output-stream`'e yazılanı döndürür ve onu boşaltır. CL'deki
gibi `close`'dan sonra bile alınabilir.

**Bayt G/Ç**, `ByteInput`/`ByteOutput` kullanır. Bunlar `InputStream`/`OutputStream`'in `Item`'ını,
`CharInput`/`CharOutput`'un onu `char`'a sabitlemesiyle aynı şekilde `int`'e sabitler.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | Sıradaki bayt. Dosyanın sonunda `none` |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Bir bayt yazar. 0..255 dışında hata |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | Karakter sürümü, bayt olarak |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Yukarıdakiyle aynı |

CL eleman türüne **çağrıda**, `(open name :element-type '(unsigned-byte 8))` gibi karar verir, ancak
burada eleman türü akışın **türüdür**; bu yüzden değişen şey onu açan fonksiyondur. Bir karakter
akışından bayt okumak tür hatasıdır (`string-input-stream`, `ByteInput`'u gerçekleştirmez). Bir karakteri
`unread-char` ile geri ittikten hemen sonra bayt okumak da hatadır.

## 4. Bileşik akışlar

Hepsi standart kütüphanedeki `defstruct`'lardır ve iç içe geçirilebilir.

| Ad | Biçim | Açıklama |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Bir `Vector<:dyn CharOutput>`'un hepsine yazar |
| `make-two-way-stream` | `(make-two-way-stream in out)` | `in`'den okur ve `out`'a yazar |
| `make-echo-stream` | `(make-echo-stream in out)` | `in`'den okur ve okunan karakterleri ayrıca `out`'a yazar |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Bir `Vector<:dyn CharInput>`'u art arda okur |
| `make-peek-stream` | `(make-peek-stream in)` | Herhangi bir `:dyn CharInput`'a bir karakterlik geri itme ekler ve onu bir `PeekInput` yapar (`read-sexpr` için) |

## 5. Makrolar

| Ad | Biçim | Açıklama |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Aç, gövdeyi çalıştır, kapat. `Result<gövdenin değeri, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Bir string'den okur |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Yazılanı döndürür |

## 6. Jenerik fonksiyonlar ve dosya işlemleri

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Her şeyi aktarır |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Kalan tüm satırlar |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Bir `Sexpr` okur (CL'nin `read`'i). Girdinin sonunda `Ok(eof)`, biri okunduğunda `Ok(datum d)`, veri değilse `Err`. Veriyi bitiren **tek boşluk karakterini tüketir** (CL'deki gibi). `ReadOutcome`, boş listeyi `()` okumakla girdinin sonu aynı değer olmasın diye bir `Option<Sexpr>` değildir |
| `read-sexpr-preserving-whitespace` | Yukarıdakiyle aynı | Yukarıdakiyle aynı | Aynısı, ancak boşluğu bırakır (CL'nin `read-preserving-whitespace`'i) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | `ch`'ye kadar okur ve bir liste yapar. `ch` tüketilir. Girdi biterse `Err` |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Her seferinde bir satır yazar |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | Tüm içerik |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Tüm satırlar |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Yazar |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Var olup olmadığı |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Sil, yeniden adlandır (bağımsız değişkenler `Pathish`'tir) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Sembolik bağlantıları ve `.`/`..`'ı çözülmüş mutlak yol. Yoksa `Err` |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | Son değiştirilme zamanı. **Evrensel zaman**dır; bu yüzden `decode-universal-time` ([Zaman](system.md#2-tarihleri-çözme-ve-kodlama)) onu okuyabilir |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | Sahibin oturum açma adı. Dosya yoksa `Err`, sahibin uid'sinin parola veritabanında girdisi yoksa `Ok(none)`: CL'nin ayırt ettiği iki durum ayrı tutulur |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Bir dizin olup olmadığı. **Yoksa da `false`**; ikisini ayırt etmek için `probe-file` kullanın |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | İçeriği truename ile listeler (`truename`'deki gibi sembolik bağlantıları çözülmüş mutlak yol). Hedefi eksik olan sembolik bağlantılar dışarıda bırakılır. `.`/`..` dışarıda bırakılır. Sıra işletim sisteminin verdiği şeydir |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Ebeveynleriyle birlikte oluşturur. Zaten varsa başarılı olur |

Bir dosyayı adlandıran her bağımsız değişken **bir string ya da bir `pathname` olabilir**. Bu, CL'nin
yol adı belirleyicileriyle aynı muameledir; bir çalışma zamanı tür sınaması yerine `Pathish` trait'i
üzerinden çözümlenir (9. bölüm).

`read-delimited-list`'in sonlandırıcı karakteri **belirteçleri de bitirir**. Yalnızca 0. derinlikte
etkili olur: `(1 2]` içinde `]` listenin kendi metninin parçası olarak okunur ve bozuk bir liste olarak
bildirilir. CL'nin üçüncü bağımsız değişkeni `recursive-p`'nin karşılığı yoktur.

## 7. Kendi türünüzü akış yapma

Bir `write-item` yazın, varsayılan gerçekleştirmeler geri kalanı getirir. Bileşik akışların içine de
girebilir.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; every remaining method is the default

(write-line (counter::new 0) "four")   ; write-line, terpri and fresh-line all work
```

Girdi de aynı şekilde çalışır: yalnızca `read-item` yazarsınız. Kendi geri itmesi olmayan bir tür bile,
`(read-sexpr (make-peek-stream my-stream))` gibi sarıldığında `read` edilebilir.

## 8. readtable

| Ad | Çağrı | Tür | Açıklama |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f`, `c` karakterini okur |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Kayıtlı olanı döndürür |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f`, `d s` iki karakterlik dizisini okur |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Yukarıdakiyle aynı |

`F`, `(fn (string-input-stream char) Option<Sexpr>)`'dir. Nasıl kullanılacakları, ne zaman etkili
oldukları ve CL'den farkları [Sözdizimi Başvurusu](../syntax.md#11-okuyucu-makroları-readtable)'ndadır.

## 9. Yol adları `pathname`

Parçalara ayrılmış bir dosya adı. `/` ile ayrılmış dizin bileşenlerini, adı, türü (uzantıyı) ve kökten
başlayıp başlamadığını tutar.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   split at the last dot
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Yol adı belirleyici trait'i `Pathish`

CL'nin bir yol adı belirleyicisi (bir string ya da bir yol adı) kabul ettiği yerde, bu dil bir `Pathish`
kabul eder. Hem `string` hem `pathname` onu gerçekleştirir ve **her dosya işlemi onu jenerik olarak
alır**; bu yüzden `(open-input "a.txt")` ve `(open-input p)` ikisi de sıradan çağrılardır (çalışma
zamanı tür sınaması yoktur). Bir string'in `namestring`'i yalnızca kendisini döndürür; bu yüzden bir
string geçirdiğiniz sürece hiçbir ayrıştırma olmaz.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | String biçimi. Gerçekleştirilmesi gerekir |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Bir `pathname`'e dönüştürür (CL'nin `pathname` fonksiyonu; tür adıyla çakışacağından yeniden adlandırılmıştır). Gerçekleştirilmesi gerekir |

### 9.2 Fonksiyonlar

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Bir string'i parçalara ayırır. Sondaki bir `/` (ya da boş bir ad) "ad yok", yani bir dizin demektir |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Yalnızca verilen bileşenlerden bir tane kurar (hepsi `&key`). Atlanan bir ad ya da tür "yok" kalır ve `merge-pathnames`'in doldurduğu bir şeydir |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Dizin bileşenleri, en dıştakinden başlayarak |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Tür olmadan ad. Bir dizin için `none` |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Son noktadan sonrası. Baştaki nokta sayılmaz (`.gitignore`'un tamamı addır) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Kökten başlayıp başlamadığı |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | Ev dizini. `$HOME` yoksa `none` (CL de `NIL`'e izin verir) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | Son `/`'a kadar olan kısım |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Yalnızca `name.type` kısmı |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | `p`'de eksik bileşenleri `default`'tan doldurur. Göreli bir `p`, `default`'un dizininin altına gider; mutlak bir `p` kendi dizinini korur |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | `default`'a göre biçim. Tabanın altında değilse `p`'nin tamamı |

Tür bağımsız değişkenlerinin hepsi `(where (Pathish P))` taşır.

## 10. CL'den farklar

- **Bir sınıf hiyerarşisi değil, bir trait hiyerarşisi.** `input-stream-p` / `output-stream-p` yoktur:
  yönü tür taşır; bu yüzden çalışma zamanında sorulacak bir soru değildir.
- **`read`'in string ve akış sürümleri farklı adlara sahiptir.** `(read "...")` (CL'nin
  `read-from-string`'inin ilk değerine karşılık gelir; okumanın bittiği konuma da ihtiyacınız varsa
  `read-from-string` kullanın) ve `(read-sexpr s)` (CL'nin `read`'i). Bir çağrı tek bir alıcı türüne
  çözümlenir; bu yüzden aynı ad aşırı yüklenemez.
- **Geri itme ayrı bir trait'tir** (`PeekInput`); bu yüzden yalnızca `read-char`'a ihtiyaç duyan türler
  `unread-char`'ı gerçekleştirmeye zorlanmaz.
- **Kapatma açıktır.** GC akışları kapatmaz (GC öngörülemeyen zamanlarda çalışır; bu yüzden işi GC'ye
  bırakmak kapanma anını da öngörülemez kılardı). `with-open-file` kullanmak güvenli yoldur.
- **Yol adlarında host, aygıt ya da sürüm bileşenleri yoktur.** Joker karakterli yol adları ve mantıksal
  yol adları (`logical-pathname`) yoktur. Ayırıcı her zaman `/`'tir.
- **`pathname` fonksiyonu `to-pathname`'dir**, çünkü türler, trait'ler ve fonksiyonlar tek bir ad
  alanını paylaşır.
- **Joker karakterle eşleme yoktur**; bu yüzden `directory`, "o dizinin içeriğini listeleyen" bir
  fonksiyondur ve daha fazlası değildir. CL'nin `directory`'si bir yol adı örüntüsüne göre eşleştirir.
