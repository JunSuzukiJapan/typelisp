<!-- translated-from: editor/emacs/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-mode (Emacs)

Ett huvudläge (major mode) i Emacs för att redigera typelisp-källkod (`.typl`).
VS Code-versionen finns i [../vscode/](../vscode/README_sv.md). De två delar samma nyckelordstabeller
och samma indragsregler, och `cargo test --test editor_keyword_sync_test` kontrollerar det mekaniskt (se
slutet av det här dokumentet).

## Funktioner

- Syntaxfärgläggning
  - Specialformer och kontrollkonstruktioner (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, `pprint`-familjen och så vidare)
  - Definierade namn (`NAME` i `(defun NAME ...)` som funktionsnamn, i `(defstruct NAME ...)`
    som typnamn och i `(defvar (NAME ...))` som variabelnamn; detsamma med `pub`, som i
    `(pub defun NAME ...)`)
  - Nyckelord för namnrymder och deklarationer (`pub` `module` `use` `load` `impl` `where`) och
    markörer i lambdalistor (`&rest` `&optional` `&key`)
  - Inbyggda funktioner (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` och så vidare)
  - Primitiva typer (inklusive `bignum` / `ratio`), inbyggda typer, inbyggda feltyper
    (`ParseIntError` och så vidare), användartyper med stor begynnelsebokstav och trait-objekttypen
    `:dyn Trait`
  - **Användningar av användardefinierade typer** (namnen på `defstruct`/`defenum`/`deftrait` skrivs
    vanligtvis med gemener (`rect` `todo-item` `board`), så regeln för stor begynnelsebokstav fångar dem
    inte). När läget är anslutet till `typl-lsp` färgläggs de från serverns semantiska tokens (det
    fungerar också med `eglot`; se nedan). När det inte är anslutet faller läget tillbaka på att samla de
    typnamn som definieras i bufferten
  - Literaler (`true` `false`, taliteraler (decimala / `0xff` / `1.5` / `1/3`), teckenliteraler som
    `#\Space`, strängar, keywords som `:name`)
  - Kontrolldirektiv för `format` inuti strängar (`~a` `~5,'0d` `~{...~}` och så vidare)
  - Globaler i CL-stil med öronskydd (`*print-pretty*` och så vidare)
- Kommentarer
  - Radkommentarer `;`
  - **Nästlingsbara** blockkommentarer `#| ... |#`
- Navigering i S-uttryck och indrag i Lisp-stil
- Ett definitionsindex genom `imenu` (funktioner / metoder / makron / typer / traits / `impl` /
  variabler / moduler)
- Kommandon som kör CLI:t `typl` (nedan)

## Tangentbindningar

| Tangent | Kommando | Vad det gör |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Spara och kör `typl FILE` (genom `compile`, så du kan hoppa till felrader) |
| `C-c C-z` | `typelisp-repl` | Starta REPL för `typl` i en comint-buffert |

Ange platsen för `typl` med `typelisp-program` (standard `"typl"`).
Diagnostik har formen `error: FILE:LINE:COL: ...`, som `compilation-mode` kan tolka, så
`next-error` / ``C-x ` `` hoppar rakt till stället.

## Installation

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`.typl`-filer öppnas automatiskt i `typelisp-mode` (läget registreras i `auto-mode-alist`).

Med `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Språkserver (`typl-lsp`)

När `typl-lsp` är byggd kan den användas från `eglot` (inbyggt i Emacs 29+) eller `lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

Med `eglot`:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Med `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Stöds: diagnostik (syntax-/typfel och varningar om omdefinition, skickade genom
`textDocument/publishDiagnostics`), hovring, gå till definition, komplettering (`:` är registrerat som
utlösartecken) och semantiska tokens. Referenser mellan filer genom `use` löses upp (servern söker uppåt
efter projektrotens `typelisp.toml`; för detaljer se
[Syntaxreferens 3.11](../../docs/sv/reference/syntax.md#311-filer-och-moduler-projekt-med-flera-filer)).
Osparade ändringar i öppna editorbuffertar syns genast i diagnostiken för både de filer de beror på och de
filer som beror på dem.

### Färgläggning av typnamn (semantiska tokens)

Genom `textDocument/semanticTokens` rapporterar servern **de positioner som kontrollen faktiskt löste upp
som typnamn**. Eftersom detta inte är textmatchning:

- Typer som kommer från andra filer genom `use` färgläggs också (ett intervall som upplösning inuti
  bufferten i princip inte kan nå)
- Anrop av en **funktion** med samma namn som en typ färgläggs inte (kontrollen löste upp dem som
  funktioner, så ingen token registreras där från första början)

På klientsidan:

- **`eglot` (Emacs 31 och senare)**: eglot ritar tokens själv (`eglot-semantic-tokens-mode`).
  `typelisp-mode` håller sig undan
- **`eglot` (Emacs 30 och tidigare)**: den här versionen av eglot hanterar inte semanticTokens. Så
  **`typelisp-mode` skickar begäran själv och ritar resultatet med overlays**
  (`typelisp-semantic-tokens-mode`, aktiveras automatiskt när eglot ansluter)
- **`lsp-mode`**: inbyggt stöd (sätt `lsp-semantic-tokens-enable` till `t`). I så fall håller
  `typelisp-mode` sig undan

`scripts/emacs-semantic-smoke.el` ansluter på riktigt genom eglot och kontrollerar sidan som ritar i den
Emacs som används. Med vilken klient som helst viker reservlösningen i bufferten undan medan servern
svarar (så att två regeluppsättningar inte målar samma buffert).

| Inställning | Standard | Vad det gör |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Med eglot i Emacs 30 och tidigare, om man ska färglägga från serverns semantiska tokens |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Inaktiva sekunder efter en ändring innan en ny begäran görs (håll den större än `eglot-send-changes-idle-time`) |

## Anmärkningar

- typelisp gör symboler till gemener vid läsning, men färgläggningen skiljer på stora och små bokstäver så
  att typnamn som börjar med en versal kan skiljas åt.
- Indraget avgörs av den dedikerade `typelisp-indent-function`, som slår upp
  `typelisp-indent-specs` (en alist). Läget behåller egna poster även för former vars namn det delar med
  Emacs Lisp (`defun` `let` `if` ...) eftersom symbolegenskaper är **globala**, och typelisp-inställningar
  där skulle ändra indraget i andra Lisp-buffertar i samma session.
  Och typelisps former skiljer sig i form även när de delar namn med Emacs Lisp:
  `(defun NAME (PARAMS) RETTYPE ...)` har tre huvudelement, och `if` är fast vid tre
  element med ett obligatoriskt `else`. Så värdena kan inte heller delas.
  Varje `.typl`-fil under `examples/` har kontrollerats: `indent-region` ändrar inte en enda byte,
  och att platta ut allt indrag och dra om det återställer originalet (VS Code-versionen uppfyller
  samma standard på samma filer).

## Upptäcka avdrift i editordefinitionerna

Nyckelordstabellerna underhålls två gånger, en gång här och en gång i VS Code-versionen. För att förhindra
att editordefinitionerna hamnar efter medan implementationen går vidare finns ett test på
Rust-sidan:

```sh
cargo test --test editor_keyword_sync_test
```

Det läser faktiskt in prelude, går igenom registret och rapporterar **namn som någondera editorn inte
känner till**. Specialformer har ingen representation vid körning, så de läses från mellan
`// SPECIAL-FORM DISPATCH BEGIN` / `END` i `crates/typelisp-front/src/check/checker.rs` (ta inte bort
dessa kommentarer). Om det misslyckas, lägg till de rapporterade namnen i **båda** editordefinitionerna.

Samma test jämför också legenden för semantiska tokens (`SEMANTIC_TOKEN_TYPES` i
`src/bin/lsp.rs` och tabellerna som båda editorerna håller måste stämma i namn och ordning). En avvikelse
ger inget fel vid körning; den byter bara plats på färgerna för varje token, så den fastställs
mekaniskt.
