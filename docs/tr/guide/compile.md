<!-- translated-from: docs/ja/guide/compile.md @ 3a6ff5cd72bee55134721d600c276c160f556bb9 -->
# Derleme

Başka bir şey yapmazsanız typelisp programları yorumlayıcıda çalışır. Buna ek olarak yerel koda
derlemenin iki yolu ve bir ortamı kaydetmenin bir yolu vardır. Belirtimin ayrıntıları
[Sözdizimi Başvurusu 10. bölüm](../reference/syntax.md#10-derleme)'dedir.

| Yöntem | Nasıl | Sonuç |
|---|---|---|
| JIT derlemesi | `(compile name)` | Çalışan oturumdaki bir fonksiyon yerel kodla değiştirilir |
| AOT derlemesi | `typl -c src.typl` veya `(compile-file "src.typl" "out")` | Bağımsız bir çalıştırılabilir dosya |
| Dump | `(dump "file.typld")` | Tanımları kaydeder; `typl --image` aynı ortamdan yeniden başlar |

## 1. Hazırlık

Derleme LLVM 22 kullanır. `typl`'yi [README.md](../../../README.md) dosyasını izleyerek derlediyseniz
başka bir hazırlık gerekmez.

AOT derlemesiyle yapılan çalıştırılabilir dosyalar, `libtypelisp_front.a` statik kütüphanesine
bağlanır. `typl`'nin bir release derlemesi (`cargo install` ile kurulan dahil) bu kütüphaneyi kendi
içinde taşır; bu yüzden hazırlık gerekmez. İlk derlediğinde kütüphaneyi
`~/.typelisp/lib/<build ID>/` altına (`TYPELISP_HOME` ortam değişkeni ayarlıysa
`$TYPELISP_HOME/lib/<build ID>/` altına) yazar ve bundan sonra o kopyayı kullanır.
`typl --remove-lib` onu siler (`--others` ile `typl`'nin başka sürümlerinin yazdıklarını; `--all`
ile hepsini). `typl`'nin bir debug derlemesi, derlendiği deponun `target/debug/` dizinindeki
kütüphaneyi kullanır. Başka bir yere konmuş birini kullanmak için `typl`'yi başlatırken klasörünü
`--lib-dir` ile verin (bölüm 3.2).
macOS'ta bağlama için Xcode Command Line Tools kullanılır.

## 2. JIT derlemesi

Bu, önceden tanımlanmış bir fonksiyonu anında yerel koda çevirir.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; from here on, calls run the compiled code
```

- `name` değerlendirilmez. Fonksiyon adını olduğu gibi yazın (string olarak değil). Bir metot için,
  `(compile point::norm)` örneğindeki gibi tür adıyla birlikte yazın.
- Çağırdığı fonksiyonlar onunla birlikte derlenir.
- **Jenerik fonksiyonlar derlenemez.** Her tür için bir kopya, kullanıldığı her yerde yapılır.
  Bunun yerine onu somut türlerle çağıran fonksiyonu derleyin.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` ve `dump` yorumlayıcı işlemleridir; bu
  yüzden bunları çağıran bir fonksiyon derlenemez. Derlemeyi denemek nedenini belirten bir hata verir.

Derlemenin sonucuna bakmak için `disassemble` kullanın.

```lisp
(disassemble fib)          ; the host machine code
(disassemble fib true)     ; LLVM IR
```

## 3. AOT derlemesiyle çalıştırılabilir dosya üretme

### 3.1 Programı yazma

Giriş noktası olarak **bağımsız değişken almayan bir `main` fonksiyonu** tanımlayın.

```lisp
(use std::env::command-line-args)
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

Dosyanın sonundaki `(main)`, `typl hello.typl` çalıştırdığınızda `main`'in çağrılması için oradadır.
`compile-file` bu son `(main)`'i atlar; böylece aynı dosya hem yorumlayıcıda hem AOT derlemesiyle
çalışır.

### 3.2 Derleme

Komut satırından `typl -c` kullanın (`typl --compile` aynıdır).

```sh
$ typl -c hello.typl            # makes hello
$ typl -c hello.typl -o fib     # names the executable fib
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

`-o` olmadan çalıştırılabilir dosya, kaynak dosyanın adından `.typl` çıkarılarak adlandırılır ve
kaynak dosyayla aynı klasöre konur. Kaynak dosya adı `.typl` ile bitmiyorsa `-o` zorunludur.
`-c` (`--compile`) ile `--image` ve `--feature` verilemez.

`--release` ile `typl` bir **release derlemesi** yapar; bu, `eprint`/`eprintln`'i (inceleme amaçlı standart hata çıktısı; [Yazdırma](../reference/functions/printing.md)) argümanlarıyla birlikte kaldırır. Kaldırılanlar yine de tür denetiminden geçer, bu yüzden oradaki bir tür hatası derlemeyi başarısız kılar. `--release`, `typl`'ın bütününe ait bir seçenektir: yalnızca `-c` için değil, bir dosyayı çalıştırırken, REPL'de ve `typl test` için de aynı şekilde etkilidir. `--release` ile başlatılan bir `typl` içinde çağrılan `compile-file` da release derlemesi yapar ve böyle yapılan çalıştırılabilir dosyanın içinde `eval` ile çalıştırılan koddan da `eprint`/`eprintln` kaldırılır.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # -c'den sonra yazılsa da aynı
```

`-O0`, `-O1`, `-O2` ve `-O3` **optimizasyon düzeyini** belirler. Varsayılan `-O0`'dır ve optimizasyon yapmaz. `-O1` ve üstünde derleyici önce basit optimizasyonlar yapar; örneğin yalnızca bir değişmeze bağlanmış bir değişkeni değeriyle değiştirir ya da sabitler üzerindeki tamsayı aritmetiğini önceden hesaplar; geri kalanı ise LLVM'nin aynı adlı düzeydeki optimizasyonuna bırakır. Her düzeyde hesaplamanın sonuçları aynıdır. `--release` gibi bu da `typl`'nin bütününe ait bir seçenektir: `-c`'nin oluşturduğu çalıştırılabilir dosyanın yanı sıra bir dosya çalıştırılırken, REPL'de ve `typl test` içinde derlenen koda ve orada çağrılan `compile-file`'a da etki eder. Yorumlayıcının çalıştırdığı kod optimize edilmez.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # -c'den sonra yazılsa da aynı
```

Aynısını REPL'den ya da bir programdan `compile-file` çağırarak da yapabilirsiniz.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Tekrar tekrar derliyorsanız bu tek satırı bir dosyaya koyup `typl build.typl` ile çalıştırabilirsiniz.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Dosya adları `build.typl`'nin bulunduğu yerden değil, **`typl`'nin başlatıldığı geçerli dizinden**
çözümlenir.

`typl`'nin baktığı yerden başka bir yere konmuş bir `libtypelisp_front.a`'yı bağlamak için klasörünü
`--lib-dir` ile verin. Hem `typl -c` hem `compile-file` için geçerlidir.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Verilen klasörde `libtypelisp_front.a` yoksa `typl` bir hatayla durur. Dosya yalnızca onunla birlikte
derlenmiş `typl` ile çalışır. `typl`'yi yeniden derledikten sonra onu yeniden kopyalayın.

### 3.3 AOT ile derlenen bir dosyada neler bulunabilir

- Giriş dosyasının üst düzeyi yalnızca tanımlar (`defun` `defvar` `defparameter`
  `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait` `impl` `defffi`,
  `(unsafe (def-c-struct ...))`) ile `use` ve `module` içerebilir. Son `(main)` dışında
  `(println ...)` gibi üst düzey ifadelere izin verilmez. İşi `main` içine koyun.
- Bağımsız değişken almayan bir `main` olmadan derleme bir hatayla başarısız olur.
- `use` edilen modüllerin dosyaları da derlenir ve tek bir çalıştırılabilir dosyada birleştirilir.
- `defffi` içinde `:library` ile adlandırılan kütüphaneler otomatik olarak bağlanır ([C FFI](ffi.md)).
- Her standart kütüphane fonksiyonu AOT derlemesiyle kullanılabilir. `eval` de kullanılabilir, ancak
  o zaman tür denetleyicisi ve yorumlayıcı çalıştırılabilir dosyaya girer; bu da dosyayı büyütür ve
  başlatmayı yavaşlatır. `eval` çağırmayan programlar bunları içermez.

### 3.4 Çalıştırılabilir dosya nasıl davranır

- `(command-line-args)`, `typl hello.typl a b` olarak da `./hello a b` olarak da çalıştırılsa aynı
  biçimde bir `Vector<string>` döndürür. İlk eleman program adıdır.
- Çıkış kodunu `(exit n)` ile ayarlayın. `main` normal şekilde dönerse kod 0'dır.
- `panic` durumunda program mesajı yazdırır ve sıfırdan farklı bir kodla çıkar.

### 3.5 Çalıştırılabilir dosyanın kullandığı kaynaklar

`typl -c`, çalıştırılabilir dosyanın çalışırken hangi kaynakları kullanabileceğini belirten seçeneklere sahiptir.

```sh
$ typl --heap-cells 100000 -c hello.typl    # cons alanını 100000 hücrede sabitle
$ typl -c hello.typl --max-memory 64M       # en çok 64 MiB ayır
$ typl -c hello.typl --no-cons              # cons hücresi kullanma
```

- `--heap-cells N`, cons hücre alanının kapasitesini `N` hücrede sabitler. Alan bunun ötesine büyümez: çöp toplama tek bir hücre bile boşaltamazsa program bir çalışma zamanı hatasıyla durur. Seçenek verilmezse alan 65536 hücreyle başlar ve gerektikçe büyür.
- `--max-memory SIZE`, çalıştırılabilir dosyanın ayırdığı toplam belleği sınırlar. Her şey sayılır: cons alanı, dizgeler, vektörler ve karma tablolar gibi değerler ve çalışma zamanının kendisinin kullandığı bellek. `SIZE` bir bayt sayısıdır ve ardından `K`, `M` veya `G` (1024, 1024² ve 1024³ katı) gelebilir. Sınırı aşacak bir ayırma bildirilir ve program 1 çıkış koduyla sonlanır. Bu sonlanma `catch` ile yakalanamaz. Seçenek verilmezse sınır yoktur.
- `--no-cons`, cons hücresi kullanmayan bir çalıştırılabilir dosya oluşturur. Cons alanı yoktur; bu yüzden cons hücresi gerektiren bir şey yapan program bir **derleme hatasıdır** ve bu işlemi ve `main`'den ona ulaşan fonksiyon zincirini gösterir. Cons hücreleri bir `Sexpr` listesi (alıntılanmış liste ve quasiquote ile kurulan liste dahil) veya bir `&rest` argümanı kurmak için, ayrıca `read`, `eval` ve `macroexpand` için gerekir. Sayılar, dizgeler, vektörler, karma tablolar, yapılar, kanallar ve benzerleri üzerindeki işlemler ile `print`/`println`/`format` hiç kullanmaz. `--heap-cells` ile birlikte verilemez ve `-c`'den sonra yazılır.

`--max-memory`, `-c`'den sonra yazılır. `--heap-cells`, `-c`'den önce veya sonra yazılabilir ve o zaman çalıştırılabilir dosyanın cons alanının kapasitesidir. Cons alanı bellek sınırına dahildir; alanın sığmadığı bir sınırla program başlamadan durur.

### 3.6 WebAssembly için derlemek

`--target wasm32-wasip1` ile `typl -c`, çalıştırılabilir dosya yerine bir WebAssembly modülü (WASI preview 1 komutu) üretir. `-o` verilmezse adı, kaynağın adında `.typl` yerine `.wasm` konmuş halidir.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # hello.wasm üretir
$ wasmtime hello.wasm a b                        # WASI destekleyen bir ortamda çalıştırmak
args: #(hello.wasm a b)
fib(25) = 75025
```

- Hazırlık gerekir (3.7 bölümü için de geçerlidir). Önce rustup ile `rustup target add wasm32-wasip1 wasm32-unknown-unknown` çalıştırın. Ardından `typl`'ı `cargo install` ile kurarken `--features wasm` ekleyin (`cargo install --locked --path . --features wasm`).
  Bu olmadan kurulan `typl`'da WebAssembly çalışma zamanı yoktur ve `--target` hata verir. Depodaki hata ayıklama derlemesi `typl` için bunun yerine depoda `scripts/build-wasm-runtime.sh` çalıştırın.
- Bağlama, rustup araç zincirinin bağlayıcısını (`rust-lld`) kullanır; bu yüzden derlerken de rustup gerekir.
- Modül çalıştırılabilir dosya gibi davranır: standart çıktı, standart hata ve çıkış kodu, `(command-line-args)`,
  dosya okuma ve yazma, `throw`/`catch`/`panic`, görevler, kanallar ve `sleep`, ve `eval` çalışır.
  Görevler tek bir iş parçacığında sırayla çalışır. Dosyalar yalnızca ortamın modüle gösterdiği dizinlerde
  okunup yazılabilir (wasmtime'da `--dir` ile belirtilir).
- WebAssembly'de olmayan bir şeyi kullanan program bir **derleme hatasıdır**. Hata, kullanılanı ve oraya ulaşan,
  `main`'den başlayan fonksiyon zincirini gösterir. Kullanılamayanlar: soketler (TCP, UDP, Unix alanı,
  TLS, ad çözümleme), işletim sistemi iş parçacıkları (`Thread`), C fonksiyonları (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`,
  yerel saat dilimi ve `ed`.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` ve `--release` çalıştırılabilir dosyada olduğu gibi çalışır.

### 3.7 Tarayıcıda veya Node.js'te çalıştırmak

`--target wasm32-unknown-unknown` ile `typl -c`, JavaScript'ten yüklenip çalıştırılan bir WebAssembly modülü üretir. Modülün (`hello.wasm`) yanına, aynı klasöre, onu yükleyip çalıştıran JavaScript'i (`hello.mjs`) yazar. Hazırlık 3.6 bölümündekiyle aynıdır.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # hello.wasm ve hello.mjs üretir
$ node hello.mjs                                       # Node.js ile çalıştırmak
args: #()
fib(25) = 75025
```

Başka bir JavaScript'ten `hello.mjs` içindeki `run` fonksiyonunu çağırın. `write` çıktıyı metin olarak alır (`fd` standart çıktı için 1, standart hata için 2'dir). `run`'ın değeri çıkış kodudur.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* örneğin sayfada göstermek */ } });
```

- `write` verilmezse Node.js standart çıktıya ve standart hataya, tarayıcı ise konsola satır satır yazar.
- Tarayıcıda `hello.wasm`'ı `hello.mjs` ile aynı yerden HTTP üzerinden sunun (`file://` üzerinden yüklenemez).
- `sleep`, `Atomics.wait` ile bekler. Tarayıcı buna yalnızca cross-origin yalıtımlı bir sayfanın Worker'ı içinde izin verir; bu yüzden `sleep` kullanan programı bir Worker'da çalıştırın.
- 3.6 bölümünde kullanılamayanlara ek olarak dosya ve dizin işlemleri bir **derleme hatasıdır**. Standart girdi boş okunur ve `(command-line-args)` boş bir vektördür.

## 4. Dump'lar

Geçerli oturumun tanımlarını tek bir dosyaya kaydedip bir sonraki sefer ondan başlayabilirsiniz.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

Bir dosyayı çalıştırmak için de işe yarar; örneğin `typl --image session.typld prog.typl`.

- Kaydedilenler **tanımlardır**. Oturumda değerlendirilen ifadeler kaydedilmez.
- `compile` ettiğiniz fonksiyonlar derlenmiş biçimleriyle kaydedilir.
- Global değişkenler, dump yazıldığındaki değerleriyle değil, **başlatıcıları yeniden çalıştırılarak**
  geri yüklenir.
- Bir dump, onu yazandan farklı sürümdeki bir `typl` ile yüklenemez (bu bir hatadır).

Bir dosyayı çalıştırıp ondan `(dump ...)` yaparsanız, o dosyanın tanımları dosyanın adını taşıyan
bir modüldedir. `dp.typl` içinde tanımlanan bir fonksiyonun adı `dp::sq` olur ve onu başka bir
dosyadan çağırmak `pub` gerektirir ([Modüller ve Dosya Düzeni](modules.md)).

## 5. Derlenmiş modül dosyaları hakkında

Common Lisp'in `.fasl`'i gibi, her modülün derlenmiş sonucunu bir dosyaya yazan bir biçim yoktur.
`compile-file` çalıştırılabilir dosyayı doğrudan kaynaklardan üretir. Geride ara dosya kalmaz.
