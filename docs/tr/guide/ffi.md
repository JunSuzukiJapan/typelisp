<!-- translated-from: docs/ja/guide/ffi.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# C FFI (defffi)

Bu kılavuz, typelisp'ten C fonksiyonlarının nasıl çağrılacağını anlatır. Bildirilebilen türlerin
listesi ve kısıtlamalar
[Sözdizimi Başvurusu 3.3](../reference/syntax.md#33-defffi--c-fonksiyonlarını-bildirme-ffi)'tedir.

## 1. Bir fonksiyonu bildirme ve çağırma

`defffi`, bir C fonksiyonunun adını ve türlerini bildirir.

```lisp
(defffi (c-getpid "getpid") () i32)            ; the typelisp name and the C symbol name
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; look it up in libm
```

Çağrılar `(unsafe ...)` içine sarılır.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` gereklidir, çünkü derleyici bildirilen türlerin C tarafındaki gerçek türlerle eşleştiğini
denetleyemez. `unsafe` yazmak, bu denetimin sorumluluğunu yazan kişi olarak sizin üstlendiğiniz
anlamına gelir. Onu unutmak, bunu açıklayan bir hata verir.

## 2. Güvenli bir sarmalayıcı yazma

Amaçlanan kullanım, `unsafe`'i tek bir yere hapsetmek ve dışarıya sıradan bir fonksiyon sunmaktır.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; the caller needs no unsafe
(str-len "hello")  ; => 5
```

## 3. Türler nasıl karşılık gelir

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Aynı genişlikte tamsayılar |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (ayrıca `size_t`, `int64_t` vb.) |
| `ptr` | Herhangi bir işaretçi (`void *`, `FILE *` vb.) |
| `(ptr T)` | `T`'ye bir işaretçi ([bölüm 7](#7-c-structları)) |

### String'ler

- Geçirdiğiniz bir `string`, NUL ile sonlandırılmış bir C string'ine kopyalanır ve bu, çağrı
  döndükten sonra serbest bırakılır. String'in ortasındaki bir NUL hatadır.
- `string` döndüren bir fonksiyonun sonucu da kopyalanır. C tarafındaki bellek serbest bırakılmaz.
  Çağıranın serbest bırakması gereken bir string döndüren fonksiyonlarda (`strdup` gibi) sonucu
  `ptr` olarak alın ve `free`'yi kendiniz çağırın.
- `string` döndürmek üzere bildirilmiş bir fonksiyon NULL döndürürse bu bir hatadır. NULL
  döndürebilecek fonksiyonların (`getenv` gibi) sonucunu `ptr` olarak alın.

### `c-long` / `c-ulong` / `ptr`

Bu türler yalnızca C ile sınır boyunca değer geçirmek için vardır ve **hiçbir aritmetiği
desteklemez**. Birini typelisp tamsayısı olarak kullanmak için `as` ile dönüştürün.

```lisp
(as int (unsafe (c-strlen s)))      ; int does not lose any of the 64-bit value
(try-as i32 (unsafe (c-strlen s)))  ; none if it does not fit in an i32
(unsafe (c-malloc 16))              ; integer literals can be passed as they are
```

Bir `ptr`, C fonksiyonlarına geri verilecek bir değerdir. İşaret ettiği şeyi typelisp tarafından
okumanın bir yolu yoktur.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Bu türler yalnızca fonksiyon bağımsız değişkeni, dönüş değeri ve yerel değişken olarak
görünebilir. Struct alanı, global değişken ya da `Vector` gibilerin tür bağımsız değişkeni
olamazlar.

## 4. Bir kütüphaneyi adlandırma

`:library` olmadan, sembol süreçte zaten bağlı olanlar (libc vb.) içinde aranır. Diğer
kütüphanelerin fonksiyonları `:library` gerektirir.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- `"sqlite3"` gibi kısa bir ad önce `libsqlite3.dylib`, sonra `libsqlite3.so` olarak aranır.
- `/` içeren bir ad yol olarak ele alınır.
- Bildirilen sembol bulunamazsa hata onu adıyla belirtir.

## 5. AOT derlemesi

`defffi` kullanan programlar, olduğu gibi
[`compile-file`](compile.md#3-aot-derlemesiyle-çalıştırılabilir-dosya-üretme) ile çalıştırılabilir
dosyaya dönüştürülebilir. `:library` ile adlandırılan kütüphaneler bağlama sırasında otomatik olarak
eklenir; bu yüzden `compile-file` ek bağımsız değişken gerektirmez.

## 6. Geri çağrılar

Bir typelisp fonksiyonunu bir C fonksiyonuna geçirip geri çağrılmasını sağlayabilirsiniz.
`defffi`'nin bağımsız değişken türleri arasına bir fonksiyon türü yazın ve çağrıda o konuma bir
fonksiyon adı ya da `lambda` ifadesi koyun.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") returns p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Yalnızca **serbest değişkeni olmayan** fonksiyonlar geçirilebilir. Üst düzey fonksiyonlar,
  `lambda`'lar ve yerel `labels` fonksiyonlarının hepsi çalışır; ancak çevreleyen kapsamın bir yerel
  değişkenine başvurmak, tür denetimi sırasında hatadır. C yalnızca bildirilen bağımsız değişkenleri
  geçirir; bu yüzden yakalanan değişkenleri iletmenin bir yolu yoktur. Durum tutmak için global
  değişkenler kullanın.
- Bir fonksiyon tutan değişken geçirilemez. Yerine bir fonksiyon adı ya da `lambda` ifadesi yazın.
- Geri çağrı içindeki bir `panic` ya da `throw`, C fonksiyonu döndükten sonra çağırana ulaşır.
- Geri çağrı yalnızca typelisp'in çağırdığı C fonksiyonu çalışırken çağrılabilir. `atexit` ya da
  sinyal işleyicileri gibi şeylerden kullanılamaz.

## 7. C struct'ları

Bir C fonksiyonuna struct dizisi gibi bir şey geçirmek için `def-c-struct` ile C'dekiyle aynı
düzende bir struct bildirin ve onu `unsafe` içinde ayırın.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; declared inside a top-level unsafe

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; four items, all zero
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)`, `T` türünden `n` değer ayırır ve bir `(ptr T)` döndürür. `(c-ref p i)`, `i`'inci
  değere bir işaretçidir, `p::field` bir alandır ve `(c-deref p)`, `i32` gibi bir skalere işaretçinin
  işaret ettiği şeydir. Hepsi `setf` ile yazılabilir.
- `(as ptr p)`, onu `void *` alan C fonksiyonlarına geçirmek üzere türsüz bir `ptr`'ye çevirir.
- `item`'ın boyutu (burada 8) ve her alanın konumu, C'dekiyle aynı kurallarla belirlenir.

### Ayrılan belleğin ömrü

Ayrılan bellek, kontrol o fonksiyondaki en dıştaki `unsafe`'den çıktığında serbest bırakılır. Aynısı
`panic` ya da `throw` ile çıkıldığında da olur. Bu nedenle bir `(ptr T)` değeri `unsafe`'in dışına
çıkarılamaz. Onu `unsafe`'in değeri yapmak, bir closure'a yakalamak, bir `task`'e geçirmek ve
`throw` ile fırlatmak, hepsi tür hatasıdır. Dışarıda kullanmak istediğiniz değerleri `unsafe`
içinde sayılara ya da bir `defstruct`'a kopyalayın.

Bir `lambda` ya da `labels` fonksiyonu içinde bellek ayırırken, o fonksiyonun içine bir `unsafe`
yazın.

### C'nin ayırdığı bellek

C'den `(ptr T)` olarak alınan bir işaretçi (bir `defffi` dönüş değeri, bir geri çağrı bağımsız
değişkeni vb.), `c-alloc` ile ayrılmış belleğin içini göstermiyorsa hatadır. C'nin `malloc` ile
ayırdığı belleği ya da NULL'u alan fonksiyonları türsüz `ptr` ile bildirin.

## 8. Yapılamayanlar

- **Değişken sayıda bağımsız değişken alan fonksiyonlar** (`printf` ve benzerleri) bildirilemez.
  Değişken kısım, sabit bağımsız değişkenlerden farklı kurallarla geçirilir. Kullandığınız her
  bağımsız değişken sayısı için ayrı bir ad bildirin.
- **Struct'ları değerle geçirmek ya da döndürmek** mümkün değildir. İşaretçi geçiren fonksiyonlar
  kullanın.
- **Jenerik bildirimler** mümkün değildir.
- **Yerleşik bir fonksiyonla aynı ad** kullanılamaz.
- **Fonksiyon değeri olarak geçirilemezler.** `(map xs c-abs)` gibi geçiremezsiniz; bir `lambda`
  içine sarın.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
