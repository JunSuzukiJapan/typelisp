<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Editör Entegrasyonu (typl-lsp)

`typl-lsp`, typelisp'in dil sunucusudur. LSP'yi (Language Server Protocol) destekleyen bir editöre
bağlandığında, düzenlediğiniz dosya için şu özellikleri sağlar:

- Tanılama: okuma hataları, tür hataları ve yeniden tanımlama uyarıları
- Üzerine gelme (hover): parantezli bir ifadenin türü ve çağırdığı tanımın docstring'i (çıplak
  değişken adları için gösterilmez)
- Tanıma git
- Tamamlama (`:` yazdığınızda adaylar belirir)
- Tür adlarının renklendirilmesi (semantik token'lar); diğer dosyalardan `use` ile alınan türler dahil

`use` aracılığıyla dosyalar arasındaki başvurular çözümlenir. Açık olan başka bir dosyadaki
kaydedilmemiş düzenlemeler, o dosyayı `use` eden dosyaların tanılamalarına hemen yansır.

## 1. Derleme

```sh
cargo build --release --bin typl-lsp
```

Bu, `target/release/typl-lsp` dosyasını üretir. [README.md](../../../README.md) dosyasında anlatıldığı
gibi `cargo install` ile kurduysanız, `typl` ile birlikte `~/.cargo/bin/typl-lsp` konumundadır.

## 2. VS Code

Eklenti, depodaki `editor/vscode` dizinindedir. Marketplace'te yayımlanmamıştır; bu yüzden onu
kendiniz derleyip kurmanız gerekir.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # produces a .vsix
```

Extensions görünümündeki "..." menüsünden "Install from VSIX..." seçeneğini seçin ve derlediğiniz
`.vsix` dosyasını gösterin.

Eklenti `typl-lsp`'yi önce çalışma alanının `target/release/typl-lsp` yolunda, sonra
`target/debug/typl-lsp` yolunda, ardından `PATH` içinde arar. Başka bir yere koyduysanız yolunu
`typelisp.languageServer.path` ayarına yazın.

| Ayar | Varsayılan | Anlamı |
|---|---|---|
| `typelisp.program` | `typl` | `typl` yolu |
| `typelisp.languageServer.enable` | `true` | `typl-lsp`'ye bağlanılıp bağlanılmayacağı |
| `typelisp.languageServer.path` | (boş) | `typl-lsp` yolu |

`Ctrl+Alt+R` düzenlediğiniz dosyayı kaydedip `typl` ile çalıştırır, `Ctrl+Alt+Z` ise REPL'i başlatır.
Daha fazlası için [VS Code eklentisinin README'sine](../../../editor/vscode/README_tr.md) bakın.

## 3. Emacs

`typelisp-mode`, depodaki `editor/emacs` dizinindedir.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`eglot` ile (Emacs 29 ve sonrasıyla birlikte gelir) `typl-lsp`'ye bağlanma ayarları:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

`lsp-mode` ile:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Emacs 31 ve sonrasının eglot'u tür adlarını (semantik token'lar) kendisi renklendirir. Emacs 30 ve
öncesinin eglot'u bunları desteklemez; bu yüzden tür adlarını `typelisp-mode` renklendirir.
`lsp-mode` ile `lsp-semantic-tokens-enable` değerini `t` yapın.

`C-c C-c` düzenlediğiniz dosyayı çalıştırır, `C-c C-z` ise REPL'i başlatır. Daha fazlası için
[typelisp-mode README'sine](../../../editor/emacs/README_tr.md) bakın.

## 4. Diğer editörler

`typl-lsp`, standart girdi ve çıktı üzerinden LSP konuşur ve komut satırı bağımsız değişkeni almaz.
Editörünüzün LSP istemcisini, `.typl` dosyaları için `typl-lsp`'yi başlatacak şekilde yapılandırın.

## 5. Projelerin nasıl tanındığı

`typl-lsp`, açılan dosyanın dizininden başlayıp yukarı doğru çıkarak `typelisp.toml` dosyasını arar
ve `use` ifadelerini o konumu kaynak kökü sayarak çözümler. Bunlar, `typl`'nin bir dosyayı
çalıştırırkenki kurallarıyla aynıdır ([Modüller ve Dosya Düzeni](modules.md#2-bir-projeyi-kurma)).
Birkaç dosyadan oluşan bir projede `typelisp.toml` dosyasını projenin köküne koyun.

## 6. Dil sunucusu programınızı çalıştırmaz

`typl-lsp` tanılamaları yalnızca okuyarak ve türleri denetleyerek üretir. Düzenlediğiniz programı
asla çalıştırmaz. Tanılamalar her tuş vuruşunda çalışır; bu yüzden yan etkisi olan ya da hiç
bitmeyen kodu orada çalıştırmayı göze alamaz. Tek istisna, kendilerinden sonra gelen makro
çağrılarını denetlemek için gerekli olan `defmacro` kayıtlarıdır.

Bu nedenle, yalnızca `typl` programı çalıştırırken ortaya çıkan hatalar (`panic`, eksik bir dosya
vb.) dil sunucusunun tanılamalarında görünmez.
