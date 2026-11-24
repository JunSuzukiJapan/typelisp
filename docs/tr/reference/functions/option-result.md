<!-- translated-from: docs/ja/reference/functions/option-result.md @ dff3b51792f6d9c49bb8802bd3d3fb6e168edd92 -->
# Option, Result ve Hata Türleri

## 1. `Option<T>` / `Result<T,E>`

Yapıcılar: `Option<T>`'nin `(Some T)` / `None`'ı vardır. `Result<T,E>`'nin `(Ok T)` / `(Err E)`'si vardır.
`E` herhangi bir tür olabilir: yerleşik somut hata türleri ve `defstruct`/`defenum` ile kendi yazdığınız
türler oraya aynı şekilde uyar (3. bölüm).

| Ad | Biçim | Option | Result | Açıklama |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Değeri çıkarır. `None`/`Err` durumunda panic olur |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | Değer ya da varsayılan |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | `Some` olup olmadığı |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | `None` olup olmadığı |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | `Ok` olup olmadığı |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | `Err` olup olmadığı |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | Değeri çıkarır. `None`/`Err` ise `msg` ile panic olur |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | Değer ya da `f`'nin sonucu. `f` yalnızca `None`/`Err` iken çağrılır |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | `Some`/`Ok` içeriğine `f` uygular |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | `Err` içeriğine `f` uygular |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | `Some`/`Ok` ise içeriği `f`'ye verir ve onun sonucunu döndürür |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | `None`/`Err` ise `f`'nin sonucunu döndürür |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | `(Some v)`'yi `(Ok v)`'ye, `None`'ı `(Err e)`'ye çevirir |

`Some`/`None`/`Ok`/`Err` yapıcıları, `use` olmadan her modülde çıplak adlarıyla kullanılabilir.
`Option::Some` gibi tür adıyla da yazılabilirler.

Dallanma `match` ile açıkça yazılır ya da yukarıdaki `map`/`and-then` ve diğerleriyle zincirlenir.
Rust'taki `?`'ye karşılık gelen bir sözdizimi yoktur.

`Option`/`Result`'un `map`'i bir metottur; [diziler](sequences.md) için olan `map`'ten ayrıdır. İlk
argümanın türü `Option`/`Result` olduğunda çağrılan budur.

`->` makrosu bir değeri sırayla izleyen her formun ilk argümanı olarak geçirir (Clojure'un `->`'si
gibi). `(-> x (f a) (g b))`, `(g (f x a) b)` olur. Parantezsiz bir ad, `h`, `(h x)` olarak ele
alınır. Bir metodun ilk argümanı alıcısıdır; bu yüzden birleştiriciler olduğu gibi zincirlenir:

```lisp
(use std::num::ParseIntError std::num::parse-int)
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (Option::Some (truncate n 2)) (Option::None)))

(-> (Option::Some 8)
    (and-then half)                          ; (Some 4)
    (and-then half)                          ; (Some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (Some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. `Option<T>`'nin çalışma zamanı gösterimi

Rust'taki gibi **`Option<T>` genellikle kutu oluşturmaz**. `Some v`, `v`'nin kendisidir ve `None` boş
liste değeridir; ayırma ve dolaylılık yoktur. `Option<Sexpr>` (boş listenin `None` olduğu),
`Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` ve `Option<(fn ...)>` hepsi bu
biçimi alır.

Kutu yalnızca `T` türündeki bir değer boş liste değerinden ayırt edilemediğinde kullanılır:

| `T` | Gösterim | Neden |
|---|---|---|
| `Option<U>` (iç içe) | Kutu | İçteki `None`, dıştaki `None` ile aynı değer olurdu |
| `()` | Kutu | `()`'nin değeri boş liste değerinin kendisidir |
| `ptr` / `c-long` / `c-ulong` | Kutu | Bitlerin hepsi değerdir; ayırt etmek için yer kalmaz |
| Diğer her şey | Kutu yok | — |

Gösterim yalnızca tür tarafından belirlenir ve bir değerden okunamaz. Yazdırırken `(Some ...)`/`None`
statik türden yeniden oluşturulur; bu yüzden `(format false "~a" opt)` `(Some 1)` yazdırır. İki kısıtlama
vardır:

- **Bir `:dyn Trait` içine konamaz** (`(impl Speak Option<int> ...)` yazdığınız bir `Option<int>` değerini
  bir `:dyn Speak`'e geçirmek hatadır).
- Bir `Sexpr`'ten `(the Option<T> ...)` aşağı dönüşümü **bir yapıcı adlandırır**:
  `(the Option<int> (Some x))` / `(the Option<int> (None))`. Tüm değeri bağlayan biçim,
  `(the Option<int> o)`, hatadır.

## 3. Hata türleri ve `Error` trait'i

Rust'ın `std::error::Error`'unu izleyerek **`Error` bir tür değil, bir trait'tir**. Hataları temsil
eden somut türler her amaç için ayrıdır ve her biri `Error`'ı gerçekleştirir.

| Tür | Şunun ürettiği |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Dosya ve akış işlemleri ([Akışlar ve Dosyalar](streams-files.md)) |
| `NetError` | Ağ işlemleri ([Ağ](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. CL'nin `simple-error`'ı: yalnızca ne olduğunu söylemek istediğinizde varsayılan seçim |
| `WrappedError` | `(wrap-error msg cause)`. Hem kendi mesajınızı hem nedeni taşıyan bir tür; `Error` trait'inde `source` olmasının nedeni budur |

`ParseIntError`'dan `NetError`'a kadar her biri "tek bir mesaj string'i tutan tek varyantlı bir enum"dur
ve tür adı ile varyant adı aynıdır (`(match e ((ParseIntError m) m))`, `(ParseIntError::ParseIntError "...")`
ile oluşturulur). Bunlarda özel bir şey yoktur: `(defstruct my-err (...))` / `(defenum my-err ...)` ile
yazdığınız kendi hata türleriniz gibi aynen ele alınırlar.

| Ad | Biçim | Tür | Açıklama |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Hata mesajı (`Error` trait'inin bir metodu) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | Bu hatanın sardığı neden ya da yoksa `None` (Rust'ın `Error::source`'u) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E`, `Error`'ı gerçekleştirir) | Somut bir hata türünü trait nesnesine genişletir |
| `describe-error` | `(describe-error e)` | `E→string` (`E`, `Error`'ı gerçekleştirir) | Mesaj ve `source` izlenerek bulunan nedenler zinciri; her satırda bir neden. CL'de karşılığı yoktur (Rust'ın "caused by"'ı) |

Kendi hata türünüz için `Error`'ı gerçekleştirirseniz, yerleşik hatalarla **aynı şekilde** ele
alınabilir:

```lisp
(use std::num::ParseIntError)
(defstruct io-err (path string))
(impl Error io-err
  (defmethod message (self) string (append "io failed: " self::path))
  (defmethod source (self) Option<:dyn Error> (Option::None)))

(defun open-it ((p string)) Result<i32, io-err>          ; the concrete type goes into E as it is
  (if (equal p "") (Result::Err (make-instance 'io-err :path "<empty>")) (Result::Ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; handle any kind uniformly
(describe (make-instance 'io-err :path "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Birkaç hata türünü tek bir `Result` içinde toplamak için `Result<T, :dyn Error>` kullanın (Rust'ın
`Box<dyn Error>`'una karşılık gelir) ve somut hataları `as-dyn-error` ile genişletin. `?` olmadığından bu
dönüşüm açıkça yazılır:

```lisp
(use std::error::as-dyn-error std::num::parse-int)
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((Ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((Err e) (Result::Err e))))
```

**Türler ve trait'ler tek bir ad alanını paylaşır** (Rust'taki gibi). Tek bir modül içinde bir
`defstruct`/`defenum` ile bir trait aynı ada sahip olamaz ve bir tür konumuna trait adı yazmak
"`error` is a trait, not a type — write `:dyn error`" olarak bildirilir.

Kurtarılamayan başarısızlıklar `panic` ile ifade edilir. `panic` ve `catch`/`throw` için
[Sözdizimi Başvurusu](../syntax.md#8-yerel-olmayan-çıkışlar-catch--throw--unwind-protect)'na; hata yönetimi
politikası için [aynı belgenin 9. bölümüne](../syntax.md#9-hata-yönetimi-politikası) bakın.
