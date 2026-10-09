<!-- translated-from: docs/ja/guide/compile.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
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
`-c` (`--compile`) ile `--image`, `--heap-cells` ve `--feature` verilemez.

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

- Giriş dosyasının üst düzeyi yalnızca tanımlar (`defun` `defmethod` `defvar` `defparameter`
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
