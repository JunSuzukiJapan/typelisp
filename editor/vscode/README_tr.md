<!-- translated-from: editor/vscode/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp (VS Code)

typelisp kaynak kodunu (`.typl`) düzenlemek için bir VS Code eklentisi.
Emacs sürümü [../emacs/](../emacs/README_tr.md) dizinindedir. İkisi aynı anahtar sözcük tablolarını ve
aynı girinti kurallarını paylaşır ve `cargo test --test editor_keyword_sync_test` bunu makineyle
denetler (aşağıya bakın).

## Özellikler

- **Sözdizimi renklendirme** (bir TextMate dilbilgisi; dil sunucusu gerekmez)
  - Özel formlar ve denetim yapıları (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, `pprint` ailesi)
  - Tanımlanan adlar (`(defun NAME ...)` bir fonksiyon olarak, `(defstruct NAME ...)` bir tür olarak,
    `(defvar (NAME ...))` bir değişken olarak; `(pub defun NAME ...)` gibi `pub` ile de aynı) ve
    `(impl Trait Type)`'ın her iki adı
  - Ad alanı ve bildirim anahtar sözcükleri (`pub` `module` `use` `load` `impl` `where`) ve lambda
    listesi işaretçileri (`&rest` `&optional` `&key`)
  - Yerleşik fonksiyonlar, ilkel türler (`bignum` / `ratio` dahil), yerleşik hata türleri,
    `Capitalized` kullanıcı türleri ve `:dyn Trait` trait nesnesi türü (jenerik bağımsız değişkenlerin
    içinde de)
  - Sayı sabitleri (onluk / `0xff` / `1.5` / `3.0e10` / `1/3`), `#\Space` gibi karakter sabitleri,
    `:name` gibi anahtar sözcükler ve `*print-pretty*` gibi earmuff'lı globaller
  - **String'lerin içindeki `format` denetim yönergeleri** (`~a` `~5,'0d` `~{...~}` `~^` vb.)
  - `;` satır yorumları ve **iç içe geçebilen** `#| ... |#` blok yorumları
- **Kullanıcı tanımlı türlerin kullanımları** (semantik token'lar)
  - `defstruct` / `defenum` / `deftrait` adları genellikle küçük harftir (`rect` `todo-item` `board`);
    bu yüzden `Capitalized` kuralı onları yakalamaz ve bir TextMate dilbilgisi satır satır çalışır ve
    dosyanın tamamını göremez. Semantik token'lar görebilir; bu da statik tür denetimli bir dilin
    yalnızca tür ek açıklamalarını renksiz bıraktığı durumu düzeltir
  - `typl-lsp`'ye bağlandığında eklenti **denetleyicinin gerçekten tür adı olarak çözümlediği
    konumları** alır. Bu yüzden `use` ile başka dosyalardan gelen türler renklendirilir ve bir türle
    aynı ada sahip bir **fonksiyonun** çağrıları renklendirilmez (denetleyici onları fonksiyon olarak
    çözümledi; bu yüzden orada baştan hiçbir token kaydedilmez)
  - Sunucu bağlı olmadığında ya da derlenmemişse eklenti, dosya içinde çözümleyen bir metin taramasına
    geri döner. Bu bir yaklaşıklıktır: başka dosyalardan gelen türleri bulamaz ve bir türle aynı ada
    sahip bir fonksiyonu ayırt edemez
- **Lisp girintileme** (VS Code'un yerleşik bir Lisp girintilemesi yoktur; bu yüzden eklenti onu
  gerçekleştirir)
  - Belgeyi Biçimlendir, Seçimi Biçimlendir ve yazarken biçimlendir (`editor.formatOnType` etkinse
    Enter ve `)`)
- **Anahat / ekmek kırıntıları / `Ctrl+Shift+O`** (fonksiyonlar, metotlar, makrolar, türler,
  trait'ler, `impl`, değişkenler, modüller)
- **`typl-lsp` entegrasyonu** (tanılama, üzerine gelme, tanıma git, tamamlama, semantik token'lar)
- **`typl` CLI komutları** (çalıştır, REPL)

Dil sunucusu dışındaki her şey yalnızca eklentiyle çalışır; bu yüzden `typl-lsp`'nin derlenmediği bir
checkout'ta bile renklendirme, girintileme, Anahat ve (dosya yerel) tür renklendirmesi kullanılabilir.

## Kurulum

Eklenti Marketplace'te değildir; bu yüzden yerelde derleyip kurun.

```sh
cd editor/vscode
npm install
npm run compile
```

Ardından şunlardan birini yapın:

- **Bir geliştirme ana makinesinde deneyin**: VS Code'da `editor/vscode`'u açın ve `F5`'e basın
- **Kalıcı olarak kurun**: `npx @vscode/vsce package` ile bir `.vsix` yapın, sonra Extensions
  görünümünde "..." → "Install from VSIX..." kullanın

`.typl` dosyaları otomatik olarak typelisp kipinde açılır.

## Tuş atamaları

| Tuş | Komut | Ne yapar |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Kaydet ve `typl FILE` çalıştır |
| `Ctrl+Alt+Z` | `typelisp.repl` | `typl` REPL'ini başlat |

Komut paletinde ayrıca `typelisp: Restart Language Server` vardır.

## Ayarlar

| Ayar | Varsayılan | Ne yapar |
|---|---|---|
| `typelisp.program` | `typl` | `typl` CLI'sinin yolu |
| `typelisp.languageServer.enable` | `true` | `typl-lsp`'ye bağlanılıp bağlanılmayacağı |
| `typelisp.languageServer.path` | (boş) | `typl-lsp` yolu. Boşsa eklenti çalışma alanının `target/release/typl-lsp` yolunu, sonra `target/debug/typl-lsp` yolunu, sonra `PATH`'i arar |
| `typelisp.trace.server` | `off` | LSP JSON-RPC trafiğini günlüğe yaz |

Dil sunucusunu şöyle derleyin:

```sh
cargo build --release --bin typl-lsp
```

`use` aracılığıyla dosyalar arasındaki başvurular, proje kökünün `typelisp.toml` dosyası yukarı doğru
aranarak çözümlenir (ayrıntılar için
[Sözdizimi Başvurusu 3.11](../../docs/tr/reference/syntax.md#311-dosyalar-ve-modüller-çok-dosyalı-projeler)'e
bakın).

## Görev problem eşleştiricisi

Eklenti, `typelisp` adında bir problem eşleştiricisi sağlar. `typl` tanılamaları
`error: FILE:LINE:COL: message` biçiminde yazdırır; bu yüzden doğrudan Problems paneline gidebilirler:

```jsonc
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "typl: run",
      "type": "shell",
      "command": "typl ${file}",
      "problemMatcher": "$typelisp"
    }
  ]
}
```

## Geliştirme

```sh
npm run compile   # tsc
npm run watch     # build on change
npm test          # node --test (grammar, indentation, symbols, type references, manifest)
```

Testler yalnızca `vscode` modülüne ihtiyaç duymayan kısımları kapsar. Bunun için `src/indent.ts` ve
`src/symbols.ts` saf fonksiyonlar olarak yazılmıştır ve yalnızca `src/extension.ts` editör API'sine
dokunur.

- `src/test/grammar.test.ts` — dilbilgisiyle gerçekten token'lara ayırır (VS Code ile aynı motoru
  kullanarak: `vscode-textmate` + `vscode-oniguruma`) ve sonucu denetler.
  Oniguruma, ayrıntılarda Emacs düzenli ifadelerinden farklıdır (örneğin bir karakter sınıfının
  başındaki `]`'yi düz karakter saymaz) ve bu tür farklar yalnızca gerçek motor çalıştırılarak bulunur.
- `src/test/indent.test.ts` — `examples/` altındaki her `.typl` dosyası için **tüm girintiyi düzleştirip
  geri yüklemenin, işlenmiş içerikle bayt bayt eşleşmesini** ister.
  Emacs kipi aynı dosyalarda aynı ölçütü karşılar ve "iki editör uyuşuyor" ifadesini doğrulanmış bir
  iddia yapan şey budur.
  Ayrıca `src/test/fixtures/emacs-indent-reference.txt`, bir Emacs `typelisp-mode` arabelleğinde
  `indent-region` gerçekten çalıştırılarak toplanan başvuru çıktısıdır. Beklenen taraf TS
  gerçekleştiriminin bir yeniden ifadesi değil, **diğer editörün gerçekten ürettiği şeydir**; böylece
  portun sadakati doğrudan denetlenir (`let*` `do` `doiter` `labels` `impl`
  `pprint-logical-block`, quote önekleri ve daha fazlasını içerir).
- `src/test/symbols.test.ts` — Anahat içeriği ve geri dönüşün tür başvurularını algılaması. Tanım sayısı,
  satır başlarındaki tanım formlarının bağımsız bir sayımıyla tam olarak eşleşmelidir. Tür başvuruları
  için sınır kuralları, bilinçli olarak Emacs sürümünün geri dönüşüyle hizalanmıştır (VS Code bir
  lookbehind kullanır; Emacs aynı kümeyi bir önceki karakteri tüketerek ifade eder).
- Sunucunun çözümleme güdümlü token'ları (`crates/typelisp-front/src/check/semantic.rs`),
  `cargo test --test lsp_semantic_test` ve `scripts/lsp-semantic-smoke.py` ile (stdio üzerinden gerçek bir
  süreci yönlendirir) denetlenir. Emacs istemcisi, gerçek bir eglot bağlantısı üzerinden
  `scripts/emacs-semantic-smoke.el` ile denetlenir.
- `src/test/manifest.test.ts` — `package.json`, derleyicinin denetlemediği tek kısımdır; bu yüzden bu
  test, bildirilen komutlar ile `registerCommand` çağrılarının aynı küme olduğunu, tuş atamalarının neye
  başvurduğunu, kodun okuduğu ayarların bildirildiğini ve problem eşleştiricisinin `typl`'nin gerçekten
  yazdırdığını ayrıştırabildiğini denetler.

### Editör tanımlarındaki kaymayı algılama

Anahtar sözcük tabloları iki kez, Emacs sürümünde ve VS Code sürümünde tutulur. Gerçekleştirim ilerlerken
editör tanımlarının geride kalmasını önlemek için Rust tarafında bir test vardır:

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

## Notlar

- typelisp, sembolleri okurken küçük harfe çevirir, ancak büyük harfle başlayan tür adlarını ayırt
  edebilmek için renklendirme büyük/küçük harf duyarlıdır.
- Girintileme, `src/indent.ts` içindeki `INDENT_SPECS` ile belirlenir. Emacs sürümünün
  `typelisp-indent-specs`'inin bir portudur; aynı değerlere ve kurallara sahiptir. Bir formun aynı adlı
  Emacs Lisp formundan biçimce farklı olduğu yerler olduğu gibi taşınmıştır:
  `(defun NAME (PARAMS) RETTYPE ...)` başlığı üç elemanlıdır, `if` zorunlu bir `else` ile üç elemanda
  sabittir vb.
- `#| ... |#` içeriği biçimlendirme sırasında yeniden girintilenir. Bu, Emacs'in `indent-region`
  davranışıyla eşleşir.
