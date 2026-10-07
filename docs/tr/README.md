<!-- translated-from: docs/ja/README.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp Belgeleri (Türkçe)

typelisp, statik tür denetimli bir Lisp'tir. Kurulumu ve derlenmesi için deponun kök dizinindeki
[README.md](../../README.md) dosyasına bakın (İngilizce).

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
