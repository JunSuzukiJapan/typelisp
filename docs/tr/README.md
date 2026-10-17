<!-- translated-from: docs/ja/README.md @ 7d830ae3902dd3c581c7cf6fa5365b91b34106da -->
# typelisp Belgeleri (Türkçe)

typelisp, statik tür denetimli bir Lisp'tir. Kurulumu ve derlenmesi için deponun kök dizinindeki
[README.md](../../README.md) dosyasına bakın (İngilizce).

## Özellikler

### Task'ler ve kanallar

Task'ler (hafif thread'ler) başlatılır ve task'ler kanallar üzerinden birbirine değer geçirir.
Ayrıntılar için [Eşzamanlılık](tutorial/concurrency.md) belgesine bakın.

```lisp
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### nil yerine Option ve Result

`nil` yoktur. Değeri olmayabilecek bir fonksiyon `Option<T>`, başarısız olabilecek bir fonksiyon
`Result<T,E>` döndürür; içerik `match` ile çıkarılır. Bir `Option<string>` değerini `string` gibi
kullanmak ya da bir `match` içinde `none` durumunu yazmayı unutmak, program çalışmadan önce bir tür
hatasıdır. Ayrıntılar için [Tür Temelleri](tutorial/types.md) ve [Hata Yönetimi](tutorial/errors.md)
belgelerine bakın.

```lisp
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### Tür denetiminden geçen makrolar

Makrolar Common Lisp'teki gibi `defmacro`, quasiquote ve `gensym` ile yazılır. Genişletilmiş kod da
elle yazılmış kod gibi tür denetiminden geçer; türlere uymazsa hata, makronun çağrıldığı yerde
bildirilir. Ayrıntılar için [Makrolar](tutorial/macros.md) belgesine bakın.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## Öğretici

typelisp'e yeni başlıyorsanız bunları sırayla okuyun.

- [Başlarken](tutorial/intro.md): REPL, fonksiyonlar, değişkenler, koşullar, döngüler, listeler ve `Vector`
- [Tür Temelleri](tutorial/types.md): statik türler, `Option`, `Result`, struct'lar, enum'lar, jenerikler
- [Trait'ler](tutorial/traits.md): `deftrait` / `impl`, trait sınırları, `:dyn`
- [Makrolar](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Hata Yönetimi](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Eşzamanlılık](tutorial/concurrency.md): task'ler, kanallar, `select`, `Mutex`, `thread`

## Kılavuzlar

- [Modüller ve Dosya Düzeni](guide/modules.md): `use`, `pub`, dosyaların modüllere karşılık gelme biçimi
- [Derleme](guide/compile.md): JIT, AOT derlemesiyle çalıştırılabilir dosya üretme, dump'lar
- [Dosya G/Ç, Akışlar ve Ağ](guide/io.md): dosyalar, yol adları, TCP / TLS / UDP, ad çözümleme
- [C FFI](guide/ffi.md): `defffi` ile C fonksiyonlarını çağırma (geri çağrılar ve `def-c-struct` ile C struct'ları dahil)
- [Editör Entegrasyonu](guide/editors.md): `typl-lsp` ve VS Code / Emacs kurulumu
- [Common Lisp Programcıları İçin](guide/from-common-lisp.md): typelisp'in CL'den farkları ve CL kodunun nasıl yeniden yazılacağı

## Başvuru

- [Sözdizimi Başvurusu](reference/syntax.md): sözcüksel sözdizimi, türlerin yazımı, tanımlar, denetim formları, derleme, eşzamanlılık
- [Yerleşik Fonksiyonlar](reference/functions/README.md): yerleşik fonksiyonlar, metotlar ve standart kütüphane
- [Türler](reference/types.md): türler ve her birinin gerçekleştirdiği trait'ler
- [Hata Mesajları](reference/errors.md): sık görülen hataların anlamı ve nasıl giderileceği

## Editör Entegrasyonu

Kurulum adımları [Editör Entegrasyonu kılavuzunda](guide/editors.md) yer alır. Her editörün tuş
atamaları ve ayarları şu belgelerde listelenmiştir:

- [Emacs (typelisp-mode)](../../editor/emacs/README_tr.md)
- [VS Code](../../editor/vscode/README_tr.md)
