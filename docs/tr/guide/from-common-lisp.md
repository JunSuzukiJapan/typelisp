<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Common Lisp Programcıları İçin

typelisp, Common Lisp'in (CL) sözdizimini ve fonksiyon adlarının çoğunu devralır, ancak statik tür
denetimli bir dildir. Bu nedenle CL kodu her zaman yazıldığı gibi çalışmaz. Bu kılavuz, CL'ye
alışkın kişilerin sık takıldığı noktaları, kodun nasıl yeniden yazılacağıyla birlikte toplar.

## 1. `nil` ve `t` yoktur

Mantıksal değerler `true` ve `false`'tur. `nil` ve `t` tanımlı değildir.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Yalnızca bir `bool` koşul olabilir.** Koşul olarak `0` ya da boş bir liste yazmak tür hatasıdır.
  "nil dışındaki her şey doğrudur" diye bir kural yoktur.
- **`if`'in else dalı atlanamaz.** `(if c x)` bir hatadır. Else dalına gerek olmadığında
  `when` / `unless` kullanın.
- **"Değer yok", `Option<T>` ile ifade edilir.** CL'de "bulunamadı" anlamında nil döndüren bir
  fonksiyon burada `(some x)` ya da `none` döndürür.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- Boş liste `()`, bağlama göre ya Unit türünün değeridir (hiçbir şey döndürmeyen bir fonksiyonun
  dönüş değeri) ya da S-ifade verisinin boş listesidir. `false`'tan farklı bir değerdir.

## 2. Türleri yazma

Fonksiyon bağımsız değişkenlerinin ve dönüş değerlerinin türleri olmalıdır.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; a generic function
  (unwrap-or (first (iter v)) default))
```

- `(defun f (x) x)` gibi türsüz bir tanım yazılamaz.
- `defvar` gibi global değişkenlerin de türü gerekir: `(defvar (count int) 0)`.
- `the`, çalışma zamanı denetimi değil, tür denetleyicisi için bir ek açıklamadır.
- **Türleri çalışma zamanında incelemenin bir yolu yoktur.** `typep` ya da `type-of` yoktur, çünkü
  her değerin türü derleme zamanında sabitlenir. Birkaç türden birini kabul etmek için `defenum`
  ile bir toplam tür yapın ya da bir trait kullanın.
- `deftype` bir tür takma adı tanımlar. `(deftype small () '(integer 0 9))` gibi bir değer aralığını
  betimleyen bir tür yapılamaz.

Varsayılan tamsayı türü `int` keyfi duyarlıklıdır; CL'nin integer'ı gibi boyutunun üst sınırı yoktur.
`i8`'den `i32`'ye ve `u8`'den `u32`'ye sabit genişlikli türler de vardır. 64 bit sabit genişlikli bir
tamsayı türü yoktur.

## 3. Değer olarak fonksiyonlar

typelisp, fonksiyonların ve değişkenlerin ad alanlarını ayırmaz. Bir fonksiyonun adı olduğu gibi
değer olarak geçirilebilir. `#'` ve `funcall` yoktur.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; call it directly, not with funcall

(apply-to twice 5)                        ; twice, not #'twice
```

- `+` ve `1+` gibi yerleşik fonksiyonlar da, bağımsız değişkenin türünün `(fn (int) int)` gibi
  sabit olduğu yerlerde olduğu gibi değer olarak geçirilebilir. Onlardan birini `foldl` ya da `map`
  gibi jenerik bir fonksiyona geçirirken hangi türün `+`'sının kastedildiği bilinmez; bu yüzden onu
  bir `lambda` içine sarın.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Dizi fonksiyonları **önce koleksiyonu, sonra fonksiyonu** alır: `(map it f)`, `(filter it f)`,
  `(foldl it f init)`. Bu, CL'nin `(mapcar f list)`'inin tersidir.
- `lambda`, `&optional` ya da `&key` kullanamaz (`&rest` kullanılabilir).
- **Bir fonksiyon tanımlanmadan önce çağrılamaz.** CL'de sonradan tanımlayacağınız bir fonksiyonu
  çağırabilirsiniz, ancak burada bu `no such function` hatasını verir. Karşılıklı özyinelemeli
  fonksiyonlar için birini önce `defsignature` ile bildirin.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Listeler ve Vector

CL listesine karşılık gelen şey, türü `Option<Sexpr>` olan **S-ifade verisidir** (boş liste
`none`'dur). `(list 1 2 3)` ve `'(a b c)` bu türdedir. S-ifade verisi, makroların ve `read`'in
çalıştığı şeydir; sıradan bir veri kabı için **`Vector<T>`** kullanın.

| İstediğiniz | CL | typelisp |
|---|---|---|
| Bir S-ifadenin başı ve geri kalanı | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Bir S-ifade listesinde gezinme | `(dolist (x xs) ...)` | Aynısı |
| Tek türden elemanlar dizisi | Bir liste ya da vektör | `Vector<T>` |
| Bir çift | `(cons a b)` | `(cons a b)` (türü `cons-cell<A,B>`'dir) |
| Eşleme | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr`, `cons` ile yapılan `cons-cell<A,B>` çiftinin erişimcileridir. S-ifade listelerinde
kullanılamazlar.

Bir `Vector`, CL'deki bir vektör gibi `#(..)` ile yazılır ve `#(..)` olarak yazdırılır:

```lisp
(let ((v #(1 2)))
  (push v 3)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #(10 20 30)
```

CL'den üç farkı vardır. Tüm elemanlar aynı türde olmalıdır (`#(1 "a")` bir tür hatasıdır). Her
değerlendirme yeni bir vektör oluşturur, bu yüzden onu değiştirmek bir sonraki değerlendirmeyi
etkilemez (CL'de bir literali değiştirmenin sonucu tanımsızdır). Boş bir `#()`,
`(the Vector<int> #())` gibi türüyle yazılmalıdır. Çok boyutlu bir dizi, CL'deki gibi
`#2A((1 2) (3 4))` olarak yazılır.

`map`, `filter`, `sort` ve `find` gibi dizi fonksiyonları, `Iter` trait'ini gerçekleştiren değerler
üzerinde çalışır. Bir `Vector`'ü `(iter v)` ile yineleyiciye çevirdikten sonra geçirin.

## 5. Çoklu değerler yoktur

`values` ve `multiple-value-bind` yoktur. CL'de birkaç değer döndüren fonksiyonlar burada bir çift
ya da bir struct döndürür.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → `car`'ı 3 ve `cdr`'si 1 olan bir `cons-cell` |
| `(decode-universal-time t)` → 9 değer | Bir `decoded-time` struct'ı |
| `(read-from-string s)` → değer, konum | `(read-from-string s)`, bir `Result` içinde değer ile konumun `cons-cell`'ini döndürür. Yalnızca değer için `(read s)` |

## 6. Özel değişkenler (dinamik bağlama) yoktur

`let` her zaman sözcüksel olarak bağlar. `defvar` ile tanımlanmış bir değişkeni `let` ile yeniden
bağlarsanız, oradan çağrılan fonksiyonlar yine özgün değeri görür.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 in CL, 1 in typelisp
```

`*print-base*` gibi bir denetim değişkenini geçici olarak değiştirmek için `dlet` kullanın. Değeri
atar ve gövdeden nasıl çıkılırsa çıkılsın özgün değeri geri yükler.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet`, global değişkenin kendisini yeniden yazar; bu yüzden thread başına bir bağlama değildir.

## 7. Koşul sistemi benimsenmemiştir

`define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` ve `signal` yoktur.
Bunlar statik tür denetimiyle kötü uyuşur. Bunun yerine şu ikisi farklı amaçlarla kullanılır:

- **Kurtarılabilir başarısızlıklar `Result<T,E>` döndürür.** Çağıran `ok` / `err`'i `match` ile
  ayırır. Rust'ın `?`'si gibi bir kısayol yoktur.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Kurtarılamayan başarısızlıklar (hatalar) `panic`'tir.** `(panic "message")`, `unwrap`'a `none`
  geçirmek, 0'a bölmek ve aralık dışı bir indeks bu türdendir ve program durur. `unwind-protect`'in
  temizliği durmadan önce çalışır.

Hata türleri `Error` trait'iyle birleştirilir ve `(message e)` mesajı verir. Kendi hata türünüzü
yapma yolu
[Option, Result ve Hata Türleri](../reference/functions/option-result.md#3-hata-türleri-ve-error-traiti)
belgesindedir. `assert` ve `warn` CL'deki gibi kullanılabilir.

`catch` / `throw` / `unwind-protect` vardır. Ancak `catch`'in etiketi, değerlendirilmeyen bir düz
sembolle (`'done`) sınırlıdır ve bir etiketle fırlatılan değerlerin tek bir türü vardır.

## 8. CLOS yoktur

`defclass`, `defgeneric` ya da metot kombinasyonu yoktur.

- Veri türleri `defstruct` (struct'lar) ve `defenum` (toplam türler) ile tanımlanır.
- `defmethod`, hedefi yalnızca **ilk bağımsız değişkenin statik türüyle** belirlenen metotlar
  tanımlar. Çoklu dağıtım (multiple dispatch) yoktur.
- Türler arasında ortak işlemler vermek için trait'leri (`deftrait` / `impl`) kullanın. Somut türü
  çalışma zamanında belirlenen değerler için `:dyn Trait` türünü kullanın
  ([Sözdizimi Başvurusu 3.9](../reference/syntax.md#39-deftrait--impl--traitler)).

`defstruct`'ın farkları:

- Yapıcı `TypeName::new`'dir: `(point::new 1 2)`. `make-point` gibi bir ad istiyorsanız
  `(:constructor make-point)` seçeneği bir tane oluşturur.
- `(x p)`'nin yanı sıra bir erişimci `p::x` olarak yazılabilir. `(setf p::x 5)` ile değiştirin.
- Hiçbir yüklem (`point-p`) oluşturulmaz. `:conc-name`, `:type` ya da `:named` yoktur.
- `:include` yalnızca slotları miras alır; tür, ebeveynin bir alt türü olmaz.

## 9. Paketler yerine modüller

Paketler yoktur. Ad alanları modüllerdir ve bir dosya kendi başına bir modüldür. `pkg:symbol` yerine
`module::name` yazın ve adları `use` ile getirin ([Modüller ve Dosya Düzeni](modules.md)).

`:foo` anahtar sözcükleri vardır ve kendi kendine değerlendirilen sembollerdir. Paketler olmadığından
iki nokta adın bir parçasıdır: `(symbol->string :foo)`, `":foo"` döndürür.

## 10. Okuma ve sözdizimindeki farklar

- Büyük ve küçük harf ayırt edilmez (semboller okunurken küçük harfe dönüşür). Bu, CL ile aynıdır.
- `#'` yoktur (bölüm 3). `#c(...)` karmaşık sayı sabitleri okunamaz; karmaşık sayıları
  `(complex 1.0 2.0)` ile yapın.
- Genişletilmiş `loop`'un yan tümceleri anahtar sözcüklerle yazılır:
  `(loop :for i :from 1 :to 3 :collect i)`. Anahtar sözcükle başlamayan bir `loop`, `(break)` ya da
  `(return value)` ile çıkılan basit bir sonsuz döngüdür. `return` en içteki döngüden çıkar (bir
  fonksiyondan çıkmak için `return-from` kullanın).
- `format`'ın hedefi `false` (bir string döndür), `true` (standart çıktı) ya da bir akıştır. Biçim
  yönergeleri CL'dekiyle aynıdır.
- Bir string'den okumak `(read "...")`, bir akıştan okumak `(read-sexpr s)`'tir. İkisi de bir
  `Result` döndürür.
- `eval`, verilen ifadeyi değerlendirmeden önce türlerini denetler ve bir `Result` döndürür. İleri
  başvurular, kaynak kodda olduğu gibi mümkün değildir.
- `eval-when` yoktur.
- Fonksiyon adları `?` ya da `!` sonekleri kullanmaz. Yüklemler CL'deki gibi `-p` / `p` ile
  (`zerop`, `sexpr-null`) ya da öne `is-` konarak (`is-some`) adlandırılır.

## 11. Farklı adlara sahip başlıca fonksiyonlar

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (bir akıştan) | `read-sexpr` |
| `pathname` | `to-pathname` |
| `floor` ve benzerlerinin iki bağımsız değişkenli sürümleri | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (bağımsız değişken sırası ters; bölüm 4) |
| `length` (bir vektörün) | `len` |
| `hash-table-count` | `count` / `size` |

Fonksiyonların listesi [Yerleşik Fonksiyonlar](../reference/functions/README.md) belgesindedir.

## 12. Var olmayan diğer şeyler

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` ile `copy-readtable`, `readtable-case` (okuyucu makroların kendileri
  `set-macro-character` ile tanımlanabilir)
- Mantıksal yol adları ve joker karakterli yol adları
- `input-stream-p` / `output-stream-p` (bir akışın yönü türüyle belirlenir)
