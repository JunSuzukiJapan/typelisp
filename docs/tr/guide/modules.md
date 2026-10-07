<!-- translated-from: docs/ja/guide/modules.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Modüller ve Dosya Düzeni

Bu kılavuz, birkaç dosyadan oluşan bir programın nasıl bir araya getirileceğini anlatır. Ayrıntılı
kurallar [Sözdizimi Başvurusu](../reference/syntax.md#310-module--use--ad-alanları)'nun 3.10 ile 3.13
arasındaki bölümlerindedir.

## 1. Bir dosya bir modüldür

typelisp'te **bir dosya kendi başına bir modüldür**. Dosyanın kaynak köküne göre yolu, modülün
yoludur.

| Dosya | Modül |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Dosyanın başına bir modül bildirimi yazmaya gerek yoktur.

## 2. Bir projeyi kurma

Projenin köküne `typelisp.toml` adında bir dosya koyun. Boş olabilir.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Kaynakları `src/` altında tutmak için `typelisp.toml` dosyasına şu tek satırı yazın:

```toml
src = "src"
```

`typl`, `typelisp.toml` dosyasını çalıştırdığı dosyanın dizininden başlayıp yukarı doğru çıkarak
arar ve bulduğu yeri kaynak kökü olarak kullanır. Hiçbiri bulunamazsa, çalıştırılan dosyanın dizini
kök olur (REPL'de geçerli dizin).

## 3. Tanımları genel yapma ve kullanma

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; a field without pub cannot be read from outside

(defun square ((n i32)) i32 (* n n))   ; a function without pub cannot be called from outside either

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl`, `(use geometry)` yazıldığı noktada yüklenir. Önceden yüklemeye gerek yoktur.

### Neler genel yapılır

- Fonksiyonlar, struct'lar, enum'lar, global değişkenler, makrolar ve metotlar, yalnızca `pub`
  taşıdıklarında diğer modüllerden görünür. `pub`'ı tanımın hemen önüne, `(pub defun ...)` gibi
  koyun.
- Struct'larda **türü genel yapmak ile alanları genel yapmak ayrı şeylerdir**.
  `(pub defstruct point ...)` türü görünür kılar; dışarıdan yalnızca `(pub x i32)` biçiminde
  yazılan alanlar okunabilir ve yazılabilir.
- Genel olmayan bir ad dışarıdan kullanılırsa `unresolved path: geometry::square` gibi bir "çözümlenemedi"
  hatası alırsınız. Bu, yanlış yazılmış bir adla alınan mesajın aynısıdır; bu yüzden yazım doğruysa
  ve ad yine de çözümlenmiyorsa eksik bir `pub`'dan şüphelenin.

`pub` alabilen tanımların listesi
[Sözdizimi Başvurusu 3.13](../reference/syntax.md#313-pub--görünürlük)'tedir.

## 4. `use` nasıl yazılır

```lisp
(use geometry)              ; bring in a module; write geometry::dist2 to use it
(use geometry::dist2)       ; bring in a function; use it by the bare name dist2
(use geometry::point)       ; bring in a type; write point::new, point::origin, and point in type annotations
(use a::f b::g)             ; several can be written together
```

- **`use` yalnızca kendisinden sonraki formlar için geçerlidir.** Dosyanın en başına koyun.
  `geometry::dist2`'yi `use`'un üstüne yazarsanız `unresolved path` hatası alırsınız.
- Modülü `use` etmeden tam yolu `geometry::dist2` yazmak da çözümlenmez. Bir dosyanın yüklenmesine
  yalnızca `use` yol açar.
- Çıplak biçimi zaten alınmış bir adı `use` etmek uyarı verir. Yine de almak istediğinizde
  `shadowing-import` kullanın.
- Bir dizinin içindeki modül `(use geo::shapes)` diye yazılır ve bundan sonra son bölümüyle
  (`shapes::...`) anılır.

### Trait metotlarını çağırma

Bir `impl` içinde gerçekleştirilen metotlar modülün fonksiyonlarına değil, **türe aittir**; bu
yüzden modül adı olmadan çağrılırlar.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, not core::area
```

Bir `impl` içindeki metotlar `pub` olmasa bile her zaman geneldir.

Bir trait'in kendisi diğer modüllere açık hale getirilemez. Bir trait'in tanımını, onun `impl`'lerini
ve onu `:dyn` ile kullanan kodu tek bir modülde tutun.

## 5. Bir dosya içinde ad alanlarını bölme

Tek bir dosya içinde ad alanını daha da bölmek için `module` kullanın. Dosyanın kendi modülünün
içine yerleşir.

```lisp
;; inside main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Dosyanın geri kalanının tamamını tek bir ad alanına koymak için, parantez içine almak yerine
`(in-module util)` yazabilirsiniz.

## 6. Bağımlılık kısıtlamaları

- **Döngülere izin verilmez.** `a.typl` `(use b)` yapar ve `b.typl` de `(use a)` yaparsa sonuç
  `circular module dependency: a -> b -> a` hatasıdır. İkisinin de ihtiyaç duyduğu tanımları
  üçüncü bir modüle taşıyın.
- **Ne türlere ne de fonksiyonlara tanımlanmadan önce başvurulabilir**; aynı dosya içinde bile.
  Karşılıklı özyinelemeli fonksiyonlar için birini önceden `defsignature` ile bildirin
  ([Sözdizimi Başvurusu 3.2](../reference/syntax.md#32-defsignature--ileri-bildirimler)).

## 7. Çalıştırma sırası

`typl main.typl` çalıştırmak şu sırayla ilerler:

1. `main.typl` ve ondan `use` edilen her dosya okunur ve türleri denetlenir. **Herhangi bir yerde
   tür hatası varsa hiçbir şey çalışmaz.**
2. `use` edilen modüllerin üst düzey ifadeleri, onları kullanan modüllerinkinden önce çalışır.
3. `main.typl`'nin üst düzey ifadeleri yukarıdan aşağıya çalışır.

Programın giriş noktasını bir `main` fonksiyonunda toplayıp dosyanın sonunda `(main)` çağırırsanız,
aynı dosya [AOT derlemesi](compile.md#3-aot-derlemesiyle-çalıştırılabilir-dosya-üretme) için de
kullanılabilir.

## 8. `load`'dan farkı

`(load "path")`, Common Lisp'in `load`'u gibi bir dosyanın içeriğini **olduğu gibi geçerli ad
alanına** okur. Onları bir modüle sarmaz ve `pub` hiçbir rol oynamaz. Bir ayar dosyasını okumak ya
da REPL'de yerel bir dosyayı yeniden yüklemek gibi işler için kullanın. Bir programı parçalara
bölmek için `use` kullanın.
