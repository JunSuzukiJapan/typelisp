<!-- translated-from: docs/ja/reference/functions/format.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Biçim Yönergeleri

`print`/`println`/`format`'ın denetim string'lerinde yazılan yönergeler. CL'nin `format` yönergelerinin
neredeyse tamamını kapsar. Fonksiyonların kendileri [Yazdırma](printing.md#1-print--println--format)
belgesinde anlatılmıştır.

## 1. Yönergeler nasıl yazılır

Her yönerge sırasıyla `~`, ardından isteğe bağlı **önek parametreleri** (virgülle ayrılmış: bir
tamsayı / `'c` (bir karakter) / `v` (sonraki bağımsız değişkenden alınır) / `#` (kalan bağımsız
değişken sayısı)), ardından isteğe bağlı `:` ve `@` **değiştiricileri** ve sonra yönerge karakteridir.
Yönerge karakterleri büyük/küçük harf duyarsızdır.

Denetim string'i bir sabit olmalıdır ([Yazdırma](printing.md#1-print--println--format)). Bunun
üzerine, denetim sırasında şunlar kontrol edilir.

- **Bağımsız değişkenlerin sayısı ve türleri.** Bir bağımsız değişken tüketen her yönerge için: bir
  bağımsız değişkenin kalıp kalmadığı ve türünün kabul edilip edilmediği (aşağıdaki tablolardaki
  "bağımsız değişken" notları). `~*` ile hareket etme, `~[`'nin hangi yan tümcesinin alındığı, `~^`'nin
  tetiklenip tetiklenmediği ya da `~@{`'nin kaç kez tekrarladığı gibi yolun çalışma zamanı değerlerine
  bağlı olduğu yerlerde **her yol** denetlenir. Artan bağımsız değişkenler sorun değildir (CL'deki gibi).
- **Parametreler ve değiştiriciler.** Kabul edilmeyen bir değiştirici, çok fazla parametre ve aralık
  dışı değerler (negatif bir genişlik, 2 ile 36 dışında bir taban, bir karakter beklenen yerde bir
  tamsayı vb.) hatadır. Asla sessizce yok sayılmaz ya da yuvarlanmaz.

Bir liste bağımsız değişkeninin (`~{`, `~:{`, `~<...~:>`) **elemanları** `Sexpr`'dir ve ne sayıları ne de
her elemanın türü türlerden bilinebilir. Elemanlara ilişkin gereksinimler (`~d` için bir tamsayı vb.) ve
eksik elemanlar, değerler geldiğinde denetlenir ve çalışma zamanı hatalarıdır (bunun yerine asla farklı
bir gösterime geçmez).

CL'nin esnek kuralları benimsenmemiştir. `~d`'ye tamsayı olmayan bir değer geçirip `~a` gibi
yazdırılmasını sağlamak ya da `~:[`'nin her değeri bir boolean saymasının bu şekilde yeniden
yorumlanmaz; bunlar tür hatalarıdır.

## 2. Çıktı (bir bağımsız değişken tüketen)

| Yönerge | Parametreler / değiştiriciler | Anlamı |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=sağa yasla | Estetik (CL'nin `princ`'i; string'ler tırnaksız). Bağımsız değişken herhangi bir türde olabilir |
| `~s` | Yukarıdakiyle aynı | Standart (CL'nin `prin1`'i; geri okunabilen bir biçim). Bağımsız değişken herhangi bir türde olabilir |
| `~w` | — | CL'nin `write`'ı. `*print-pretty*` true ise pretty-print yapar, aksi halde `~s` ile aynı |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=basamak grupları, `@`=her zaman işaret | Onlu/ikili/sekizli/onaltılı tamsayılar. Bağımsız değişken bir tamsayıdır |
| `~r` | `~radix,mincol,padchar,commachar,interval` (bir tabanla) ya da yok | Bir tabanla o taban (2 ile 36). Tabansız: `~r`=İngilizce asal sayı sözcükleri, `~:r`=İngilizce sıra sayıları, `~@r`=Roma rakamları, `~:@r`=eski Roma rakamları. Bağımsız değişken bir tamsayıdır |
| `~p` | `:`=bir geri git, `@`=y/ies | Çoğullar (`~p`→"s", `~@p`→"y"/"ies"). Bağımsız değişken bir tamsayıdır |
| `~c` | `:`=ad, `@`=`#\` sözdizimi | Bir karakter. Bağımsız değişken bir `char`'dır |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=işaret | Sabit noktalı. Bağımsız değişken bir sayıdır |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=işaret | Üstel gösterim. Bağımsız değişken bir sayıdır. CL'nin üs-basamakları, ölçek ve overflowchar parametreleri desteklenmez (verilmeleri hatadır) |
| `~g` | `@`=işaret | Genel kayan noktalı. Bağımsız değişken bir sayıdır. Parametre almaz |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Parasal gösterim. Bağımsız değişken bir sayıdır |

## 3. Çıktı (bağımsız değişken tüketmeyen)

| Yönerge | Anlamı |
|---|---|
| `~%` | Yeni satır (n tane için `~n%`) |
| `~&` | fresh-line (satır başında değilse yeni satır; `~n&`) |
| `~\|` | Sayfa sonu (form feed) |
| `~~` | Düz bir `~` (n tane için `~n~`) |
| `~t` | Sekme (`~colnum,colincT`. Zaten colnum sütununda ya da ötesindeyse colinc'in bir katı kadar ilerler; colinc 0 ise hareket etmez. `@`=göreli. `:`=mantıksal bloğun başlangıcına göre bir sekme; yalnızca pretty-print sırasında çalışır) |
| `~_` | Koşullu yeni satır (pretty; yalın=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Girinti (pretty; `~ni`=bloğun başı + n / `~n:i`=geçerli sütun + n) |
| `~<newline>` | Yeni satırı yok sayar (`:`=boşluğu koru, `@`=yeni satırı koru) |

CL'de olduğu gibi pretty-printer yönergelerinin (`~_` `~i` `~:t` `~<...~:>` ve `~a`/`~s`/`~w`'nin
pretty-print yolu) hepsi `*print-pretty*` false iken hiçbir şey yapmaz. Varsayılan olarak false'tur.

## 4. Denetim yapıları

| Yönerge | Anlamı |
|---|---|
| `~(...~)` | Harf büyüklüğü dönüşümü (`~(` küçük harf, `~:(` her sözcüğü büyük harfle başlat, `~@(` yalnızca ilk sözcüğü büyük harfle başlat, `~:@(` tümü büyük harf) |
| `~[...~;...~]` | Koşullu seçim (bir tamsayı bağımsız değişkenine göre dallanır. `~n[`, `~v[` ya da `~#[` ile o değere göre dallanır ve bağımsız değişken almaz. `~:;`=varsayılan yan tümce, yalnızca son yan tümce olarak). `~:[false~;true~]` bir `bool` bağımsız değişkenine göre dallanır ve tam olarak iki yan tümcesi vardır |
| `~{...~}` | Yineleme (bir liste bağımsız değişkeninde gezinir. `~:{`=alt liste başına, `~@{`=kalan bağımsız değişkenler üzerinde, `~:@{`=kalan bağımsız değişkenler arasındaki her liste üzerinde, `~^`=çıkış, `~:}`=boş olsa bile bir kez çalıştır). Bir yinelemede hiçbir bağımsız değişken tüketmeyen bir gövde hatadır (asla bitmezdi) |
| `~<...~;...~>` | Hizalama (parçaları `~mincol` sütuna yayar. `:`/`@`=uçlarda dolgu) |
| `~<...~;...~:>` | **Mantıksal blok** (`~:>` ile kapatılır; yukarıdaki hizalamadan farklı bir şeydir). İlk parça önek, son parça sonektir (ikisi de yalnızca sabit string'ler). `~@;` ayırıcısıyla önek bir **satır başına önek** olur. `~:<` önek/soneki varsayılan olarak `(`/`)` yapar. Bağımsız değişken bir listedir (`~@<` kalan bağımsız değişkenleri yerinde kullanır) |
| `~*` | Bağımsız değişkenleri atlama (`~n*`=n ileri, `~:*`=geri, `~@*`=mutlak bir konuma) |
| `~/name/` | Metot çağrısı (5. bölüm. `:`/`@` bayrakları metoda geçirilir. Parametre almaz) |

Aşağıdaki CL yönergeleri desteklenmez (bunlar denetim zamanı hatalarıdır).

- `~?` ve `~@?`: bir denetim string'ini çalışma zamanı bağımsız değişkeni olarak alırlar; bu yüzden
  yönergelerinin tükettiği bağımsız değişkenler denetlenemez. Bu yönergeleri doğrudan denetim
  string'ine yazın.
- `~@[...~]`: bir bağımsız değişkenin nil olmadığını sınar, ancak bu dilde nil yoktur. Bir `bool`'a göre
  dallanan `~:[false~;true~]` kullanın.
- Boş gövdeli `~{~}`: gövdeyi bir çalışma zamanı bağımsız değişkeninden alır. Yönergeleri süslü
  parantezlerin içine yazın.

## 5. `~/name/`

**CL'den bir fark: ad, global bir fonksiyon olarak değil, bağımsız değişkenin kendi türünün bir metodu
olarak aranır.** Metot `((self Self) (colon bool) (at bool)) → string` biçimindedir ve yönergenin
`:`/`@`'si olduğu gibi geçirilir.

CL'nin onu global fonksiyon olarak arama yöntemi bu dilde güvenli biçimde gerçekleştirilemez. Denetim
string'i sabit olsa bile, bir liste bağımsız değişkeninin (`~{` içindeki) elemanlarının türü denetim
zamanında bilinmez ve bir fonksiyonu yalnızca adıyla aramak, başka bir tür için tasarlanmış bir
fonksiyonu çağırabilir. Değerin türüne göre seçmek, metodun tam olarak o tür için tür denetiminden
geçmesi anlamına gelir; bu da güvenlidir (`print-object` ile aynı mekanizma). `string`/`bool`/`char`/
`symbol`/listeler gibi değerler için de çalışır. Yalnızca genişliği değerden anlaşılamayan tamsayılar
için, **birden fazla tamsayı türü o adda bir metot tanımladığında** hatadır.

Hangi bağımsız değişkene uygulandığı bilinmez, ancak hangi metotları çağırabileceği bilinir. Denetleyici
sabit denetim string'indeki her `~/name/`'i toplar ve o çağrı yerindeki bağımsız değişkenlerin türleri
arasından, yukarıdaki biçimde bir metoda sahip olanları kaydeder. Bu yüzden **bağımsız değişken
türlerinden hiçbiri metoda sahip değilse bu bir denetim zamanı hatasıdır** (çalışma zamanı hatası
değil) ve AOT çalıştırılabilir dosyalarında da çalışır.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```
