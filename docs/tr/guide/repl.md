<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# REPL

`typl` argümansız başlatılınca REPL'e (etkileşimli kip) girilir. Bu belge, REPL'in ifadeleri
değerlendirmenin yanında neler yapabildiğini anlatır: komutları, yazarken yardım, önceki sonuçlara
başvurma, bir hesaplamayı durdurma ve bir hatadan sonra inceleme.

## Başlatma ve çıkma

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

`:quit` (kısaca `:q`) ya da `Ctrl+D` ile çıkılır. Girdi geçmişi `~/.typl_history` dosyasında
tutulur; bir sonraki başlatmada `Ctrl+P` / `Ctrl+N` ya da `Ctrl+R` (geçmişte arama) ile geri
çağrılabilir.

Başlatmada, ilk istemden önce şunlar sırayla çalıştırılır. Hiçbiri ürettiği değerleri yazdırmaz.

1. Varsa `~/.typlrc` dosyasının içeriği. `--no-init` verilirse okunmaz.
2. Komut satırındaki `--eval FORM` ve `--load FILE`, yazıldıkları sırayla. İkisi de istenildiği
   kadar verilebilir.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` ve `--no-init` yalnızca REPL'i başlatmak içindir. Bir dosya çalıştırılırken
verilmeleri hatadır.

REPL `user` modülünde başlar. `(in-module foo)` ile başka bir modüle geçilince istem o modülü
gösterir.

```
typl> (in-module foo)
typl user::foo>
```

## REPL komutları

Satırın başına yazılan bir `:ad` bir REPL komutudur. `:help` bunları listeler. Listede olmayan bir
`:ad` komut değildir; sıradan bir anahtar sözcük olarak değerlendirilir.

| Komut | Anlamı |
|---|---|
| `:help` (`:h`, `:?`) | komutları listelemek |
| `:type FORM` (`:t`) | FORM'u değerlendirmeden türünü yazdırmak |
| `:describe NAME` (`:d`) | tanımın türünü, tipini, alanlarını ya da varyantlarını, metotlarını, uyguladığı trait'leri, tanımlandığı yeri ve belgelendirme dizgisini yazdırmak |
| `:apropos TEXT` | tüm modüllerde adı TEXT içeren tanımları bulmak (büyük/küçük harf ayrımı yapılmaz) |
| `:src NAME` | bir tanımın kaynak kodunu yazdırmak |
| `:who-calls NAME` | NAME fonksiyonunu ya da metodunu çağıran fonksiyon ve metotları yazdırmak |
| `:inspect FORM` (`:i`) | FORM'u değerlendirip değerini türüne göre açmak |
| `:backtrace` (`:bt`) | önceki satır başarısız olduğunda sürmekte olan çağrıları en içtekinden başlayarak yazdırmak |
| `:locals [N]` | `:backtrace` içindeki N numaralı çağrının argümanlarını yazdırmak (N verilmezse 0) |
| `:cd [DIR]` | geçerli klasörü DIR yapmak (DIR verilmezse ev klasörü) |
| `:pwd` | geçerli klasörü yazdırmak |
| `:quit` (`:exit`, `:q`) | REPL'den çıkmak |

NAME, `inner` gibi bir ad ya da `std::seq::reverse` veya `Option::unwrap` gibi bir yol olabilir.

```
typl> (defun inner ((x int) (s string)) int "x'e 1 ekler." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

x'e 1 ekler.
typl> :src inner
(defun inner ((x int) (s string)) int "x'e 1 ekler." (+ x 1))
```

`:src`, bir dosyadan yüklenen tanımı o dosyadan, REPL'e yazılan tanımı ise yazılan metinden
gösterir. Standart kütüphanedeki tanımların kaynaktaki yeri kaydedilmediği için kaynakları
gösterilemez.

`:who-calls`, geçerli oturumdaki fonksiyonların gövdelerinde arar. Genel (generic) bir fonksiyon
için yalnızca somut türlerle örneklenmiş kullanımlar bulunur.

`:cd`, `(load ...)`'un aradığı klasörü değiştirir; ancak `use` dosyaları REPL'in başlatıldığı
klasörde aramaya devam eder.

## Bir değeri açmak (`:inspect`)

`:inspect` bir değeri türüne göre açar. Bir struct alanlarına, bir enum (`Option` dahil)
varyantının alanlarına, bir `Vector` öğelerine, bir `HashTable` anahtarlarına ve değerlerine
açılır. Bir sayı o parçaya girer, `u` bir üst düzeye döner, `q` bitirir.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

Listeler ve `Sexpr` açılmaz; oldukları gibi yazdırılır.

## Yazarken yardım

Bir terminalden yazarken şunlar kullanılabilir.

- `Tab`; adları (tanımlar ve `if`, `defun` gibi sözdizimi), `::` içeren yolları, satır başındaki
  REPL komutlarını ve bir dizginin içindeyken dosya adlarını tamamlar.
- Bir fonksiyon adından sonra boşluk yazılınca, o fonksiyonun türü satırın sonunda soluk olarak
  gösterilir.
- İmlecin yanındaki parantezin eşi vurgulanır.
- Bir form bitmeden Enter'a basılırsa, sonraki satır açık parantez sayısı kadar girintili başlar.

## Önceki sonuçlar (`:*`, `:**`, `:***`)

REPL'de `:*` en son yazdırılan değer, `:**` ondan önceki, `:***` de ondan bir önceki değerdir. Her
biri kendi değerinin türünü taşır.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- Değer yazdırmayan (örneğin bir tanım) ya da başarısız olan girdi sonuçları ilerletmez.
- O kadar sonuç oluşmadan `:**` veya `:***` yazmak hatadır.
- Tırnaklanmış `':*` sıradan bir anahtar sözcüktür.
- Bir fonksiyonun gövdesine yazılan `:*`, fonksiyonun tanımlandığı andaki sonucu göstermeye devam
  eder. Common Lisp'in `*`'ı değeri çağrı anında okur; typelisp ise bir türü olsun diye bunu tanım
  anında sabitler.
- REPL dışında (bir dosya çalıştırılırken ve `~/.typlrc`, `--load`, `--eval` içinde) `:*` sıradan
  bir anahtar sözcüktür.

## Bir hesaplamayı durdurmak (`Ctrl+C`)

Değerlendirme sırasında `Ctrl+C`'ye basmak o hesaplamayı durdurur ve isteme döner. Daha önce
tanımlananlar kalır. Hesaplama yorumlayıcıdaki bir döngü, derlenmiş koddaki bir döngü ya da
`sleep` veya `wait` içindeki bir bekleme olsun, bu çalışır. Süren `unwind-protect`'lerin temizlik
işleri yapılır.

```
typl> (loop)
^Cerror: interrupted
typl>
```

Yazarken basılan `Ctrl+C` yalnızca yazılmakta olan satırı atar.

## Bir hatadan sonra inceleme (`:backtrace`, `:locals`)

Bir değerlendirme hatayla durduktan sonra `:backtrace` (`:bt`), o anda sürmekte olan fonksiyon
çağrılarını en içtekinden başlayarak yazdırır. `:locals N`, N numaralı çağrının argümanlarını
bildirilmiş türlerine göre yazdırır.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- Bir fonksiyonun son ifadesinden yapılan çağrıda (kuyruk çağrısında) çağıran fonksiyon
  gösterilmez. Bu sayede kuyruk özyinelemeli bir döngü kaç tur dönerse dönsün tek bir çağrının yerini
  kullanır.
- Derlenmiş fonksiyonların içindeki çağrılar ve adsız fonksiyonlar (`lambda`) gösterilmez.
- `:locals` yalnızca argümanları gösterir; `let` ile bağlanan değişkenleri göstermez.
- Sonraki satır değerlendirilince önceki hatanın kaydı silinir.
