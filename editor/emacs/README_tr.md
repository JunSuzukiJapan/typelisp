<!-- translated-from: editor/emacs/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-mode (Emacs)

typelisp kaynak kodunu (`.typl`) düzenlemek için bir Emacs ana kipi.
VS Code sürümü [../vscode/](../vscode/README_tr.md) dizinindedir. İkisi aynı anahtar sözcük tablolarını
ve aynı girinti kurallarını paylaşır ve `cargo test --test editor_keyword_sync_test` bunu makineyle
denetler (bu belgenin sonuna bakın).

## Özellikler

- Sözdizimi renklendirme
  - Özel formlar ve denetim yapıları (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, `pprint` ailesi vb.)
  - Tanımlanan adlar (`(defun NAME ...)`'ın `NAME`'i bir fonksiyon adı olarak, `(defstruct NAME ...)`'ınki
    bir tür adı olarak ve `(defvar (NAME ...))`'ınki bir değişken adı olarak; `(pub defun NAME ...)`
    gibi `pub` ile de aynı)
  - Ad alanı ve bildirim anahtar sözcükleri (`pub` `module` `use` `load` `impl` `where`) ve lambda
    listesi işaretçileri (`&rest` `&optional` `&key`)
  - Yerleşik fonksiyonlar (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` vb.)
  - İlkel türler (`bignum` / `ratio` dahil), yerleşik türler, yerleşik hata türleri (`ParseIntError`
    vb.), `Capitalized` kullanıcı türleri ve `:dyn Trait` trait nesnesi türü
  - **Kullanıcı tanımlı türlerin kullanımları** (`defstruct`/`defenum`/`deftrait` adları genellikle
    küçük harftir (`rect` `todo-item` `board`); bu yüzden `Capitalized` kuralı onları yakalamaz).
    `typl-lsp`'ye bağlandığında sunucunun semantik token'larından renklendirilirler (bu `eglot` ile de
    çalışır; aşağıya bakın). Bağlı olmadığında kip, arabellekte tanımlanan tür adlarını toplamaya geri
    döner
  - Sabitler (`true` `false`, sayı sabitleri (onluk / `0xff` / `1.5` / `1/3`), `#\Space` gibi karakter
    sabitleri, string'ler, `:name` gibi anahtar sözcükler)
  - String'lerin içindeki `format` denetim yönergeleri (`~a` `~5,'0d` `~{...~}` vb.)
  - Earmuff'lı CL tarzı globaller (`*print-pretty*` vb.)
- Yorumlar
  - `;` satır yorumları
  - **İç içe geçebilen** `#| ... |#` blok yorumları
- S-ifade gezinmesi ve Lisp tarzı girintileme
- `imenu` aracılığıyla bir tanım dizini (fonksiyonlar / metotlar / makrolar / türler / trait'ler /
  `impl` / değişkenler / modüller)
- `typl` CLI'sini çalıştıran komutlar (aşağıda)

## Tuş atamaları

| Tuş | Komut | Ne yapar |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Kaydet ve `typl FILE` çalıştır (`compile` üzerinden; böylece hata satırlarına atlayabilirsiniz) |
| `C-c C-z` | `typelisp-repl` | `typl` REPL'ini bir comint arabelleğinde başlat |

`typl`'nin konumunu `typelisp-program` ile ayarlayın (varsayılan `"typl"`).
Tanılamalar `compilation-mode`'un ayrıştırabildiği `error: FILE:LINE:COL: ...` biçimindedir; bu yüzden
`next-error` / `C-x \`` doğrudan yere atlar.

## Kurulum

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`.typl` dosyaları otomatik olarak `typelisp-mode`'da açılır (kip `auto-mode-alist` içine kaydedilir).

`use-package` ile:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Dil Sunucusu (`typl-lsp`)

`typl-lsp` derlendikten sonra `eglot` (Emacs 29+'ta yerleşik) ya da `lsp-mode` üzerinden kullanılabilir.

```sh
cargo build --release --bin typl-lsp
```

`eglot` ile:

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

Desteklenenler: tanılama (sözdizimi/tür hataları ve yeniden tanımlama uyarıları;
`textDocument/publishDiagnostics` ile gönderilir), üzerine gelme, tanıma git, tamamlama (`:` bir
tetikleyici karakter olarak kaydedilir) ve semantik token'lar. `use` aracılığıyla dosyalar arasındaki
başvurular çözümlenir (sunucu, proje kökünün `typelisp.toml` dosyasını yukarı doğru arar; ayrıntılar için
[Sözdizimi Başvurusu 3.11](../../docs/tr/reference/syntax.md#311-dosyalar-ve-modüller-çok-dosyalı-projeler)'e
bakın). Açık editör arabelleklerindeki kaydedilmemiş düzenlemeler, hem bağlı oldukları dosyaların hem de
kendilerine bağlı olan dosyaların tanılamalarına hemen yansır.

### Tür adı renklendirme (semantik token'lar)

Sunucu, `textDocument/semanticTokens` aracılığıyla **denetleyicinin gerçekten tür adı olarak çözümlediği
konumları** bildirir. Bu metin eşleme olmadığından:

- `use` ile başka dosyalardan gelen türler de renklendirilir (arabellek içindeki çözümlemenin ilke olarak
  ulaşamayacağı bir aralık)
- Bir türle aynı ada sahip bir **fonksiyonun** çağrıları renklendirilmez (denetleyici onları fonksiyon
  olarak çözümledi; bu yüzden orada baştan hiçbir token kaydedilmez)

İstemci tarafında:

- **`eglot` (Emacs 31 ve sonrası)**: eglot token'ları kendisi çizer (`eglot-semantic-tokens-mode`).
  `typelisp-mode` kenara çekilir
- **`eglot` (Emacs 30 ve öncesi)**: eglot'un bu sürümü semanticTokens'i ele almaz. Bu yüzden
  **`typelisp-mode` isteği kendisi gönderir ve sonucu overlay'lerle çizer**
  (`typelisp-semantic-tokens-mode`, eglot bağlandığında otomatik olarak açılır)
- **`lsp-mode`**: yerel destek (`lsp-semantic-tokens-enable`'i `t` yapın). Bu durumda `typelisp-mode`
  kenara çekilir

`scripts/emacs-semantic-smoke.el`, eglot üzerinden gerçekten bağlanır ve kullanılan Emacs'te çizimi yapan
tarafı denetler. Hangi istemciyle olursa olsun, sunucu yanıt verirken arabellek içi geri dönüş geri çekilir
(böylece iki kural kümesi aynı arabelleği boyamaz).

| Ayar | Varsayılan | Ne yapar |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Emacs 30 ve öncesinin eglot'unda, sunucunun semantik token'larından renklendirilip renklendirilmeyeceği |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Bir düzenlemeden sonra yeniden istemeden önceki boşta saniyeler (`eglot-send-changes-idle-time`'dan büyük tutun) |

## Notlar

- typelisp, sembolleri okurken küçük harfe çevirir, ancak büyük harfle başlayan tür adlarını ayırt
  edebilmek için renklendirme büyük/küçük harf duyarlıdır.
- Girintileme, `typelisp-indent-specs`'e (bir alist) bakan özel `typelisp-indent-function` ile belirlenir.
  Kip, adlarını Emacs Lisp ile paylaşan formlar (`defun` `let` `if` ...) için bile kendi girdilerini
  tutar; çünkü sembol özellikleri **globaldir** ve oradaki typelisp ayarları aynı oturumdaki diğer Lisp
  arabelleklerinin girintisini değiştirirdi. Ayrıca typelisp'in formları, Emacs Lisp ile bir ad
  paylaştıklarında bile biçimce farklıdır: `(defun NAME (PARAMS) RETTYPE ...)` üç elemanlı bir başlığa
  sahiptir ve `if` zorunlu bir `else` ile üç elemanda sabittir. Bu yüzden değerler de paylaşılamaz.
  `examples/` altındaki her `.typl` dosyası denetlenmiştir: `indent-region` tek bir baytı bile
  değiştirmez ve tüm girintiyi düzleştirip yeniden girintilemek özgün hali geri yükler (VS Code sürümü
  aynı dosyalarda aynı ölçütü karşılar).

## Editör tanımlarındaki kaymayı algılama

Anahtar sözcük tabloları iki kez, bir kez burada ve bir kez VS Code sürümünde tutulur. Gerçekleştirim
ilerlerken editör tanımlarının geride kalmasını önlemek için Rust tarafında bir test vardır:

```sh
cargo test --test editor_keyword_sync_test
```

Prelude'ü gerçekten yükler, kayıt defterinde gezinir ve **editörlerden birinin bilmediği adları**
bildirir. Özel formların çalışma zamanı gösterimi yoktur; bu yüzden
`crates/typelisp-front/src/check/checker.rs` içindeki `// SPECIAL-FORM DISPATCH BEGIN` / `END`
arasından okunurlar (bu yorumları silmeyin). Başarısız olursa bildirilen adları **her iki** editör
tanımına da ekleyin.

Aynı test ayrıca semantik token lejantını da karşılaştırır (`src/bin/lsp.rs` içindeki
`SEMANTIC_TOKEN_TYPES` ile iki editörün tuttuğu tablolar ad ve sırada uyuşmalıdır). Uyuşmazlık çalışma
zamanı hatasına yol açmaz; yalnızca her token'ın renklerini değiştirir; bu yüzden makineyle sabitlenmiştir.
