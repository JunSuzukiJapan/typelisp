<!-- translated-from: editor/vscode/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp (VS Code)

Ett VS Code-tillägg för att redigera typelisp-källkod (`.typl`).
Emacs-versionen finns i [../emacs/](../emacs/README_sv.md). De två delar samma nyckelordstabeller och
samma indragsregler, och `cargo test --test editor_keyword_sync_test` kontrollerar det mekaniskt (se
nedan).

## Funktioner

- **Syntaxfärgläggning** (en TextMate-grammatik; ingen språkserver behövs)
  - Specialformer och kontrollkonstruktioner (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, `pprint`-familjen)
  - Definierade namn (`(defun NAME ...)` som en funktion, `(defstruct NAME ...)` som en typ,
    `(defvar (NAME ...))` som en variabel; detsamma med `pub`, som i `(pub defun NAME ...)`) och
    båda namnen i `(impl Trait Type)`
  - Nyckelord för namnrymder och deklarationer (`pub` `module` `use` `load` `impl` `where`) och
    markörer i lambdalistor (`&rest` `&optional` `&key`)
  - Inbyggda funktioner, primitiva typer (inklusive `bignum` / `ratio`), inbyggda feltyper,
    användartyper med `Stor begynnelsebokstav` och trait-objekttypen `:dyn Trait` (även inuti
    generiska argument)
  - Taliteraler (decimala / `0xff` / `1.5` / `3.0e10` / `1/3`), teckenliteraler som
    `#\Space`, keywords som `:name` och globaler med öronskydd som `*print-pretty*`
  - **Kontrolldirektiv för `format` inuti strängar** (`~a` `~5,'0d` `~{...~}` `~^` och så vidare)
  - Radkommentarer `;` och **nästlingsbara** blockkommentarer `#| ... |#`
- **Användningar av användardefinierade typer** (semantiska tokens)
  - Namnen på `defstruct` / `defenum` / `deftrait` skrivs vanligtvis med gemener (`rect` `todo-item`
    `board`), så regeln för stor begynnelsebokstav fångar dem inte, och en TextMate-grammatik arbetar rad
    för rad och kan inte se hela filen. Semantiska tokens kan det, vilket åtgärdar tillståndet där ett
    statiskt typat språk lämnade bara sina typannoteringar ofärgade
  - När tillägget är anslutet till `typl-lsp` tar det emot **de positioner som kontrollen faktiskt
    löste upp som typnamn**. Så typer som kommer från andra filer genom `use` färgläggs, och anrop av en
    **funktion** med samma namn som en typ gör det inte (kontrollen löste upp dem som funktioner, så
    ingen token registreras där från första början)
  - När servern inte är ansluten eller inte är byggd faller tillägget tillbaka på en textskanning som löser
    upp inom filen. Det är en approximation: den kan inte hitta typer från andra filer, och den kan inte
    skilja en funktion med samma namn som en typ
- **Lisp-indrag** (VS Code har inget inbyggt Lisp-indrag, så tillägget implementerar det)
  - Formatera dokument, formatera markering och formatera under skrivning (Enter och `)`, när
    `editor.formatOnType` är aktiverat)
- **Disposition / brödsmulor / `Ctrl+Shift+O`** (funktioner, metoder, makron, typer, traits, `impl`,
  variabler, moduler)
- **Integration med `typl-lsp`** (diagnostik, hovring, gå till definition, komplettering, semantiska tokens)
- **Kommandon för `typl` CLI** (kör, REPL)

Allt utom språkservern fungerar med bara tillägget, så även i en utcheckning där `typl-lsp` inte har
byggts finns färgläggning, indrag, dispositionen och (fillokal) typfärgläggning tillgängliga.

## Installation

Tillägget finns inte på Marketplace, så bygg det lokalt och installera det.

```sh
cd editor/vscode
npm install
npm run compile
```

Gör sedan antingen:

- **Pröva det i en utvecklingsvärd**: öppna `editor/vscode` i VS Code och tryck `F5`
- **Installera det permanent**: gör en `.vsix` med `npx @vscode/vsce package` och använd sedan
  "..." → "Install from VSIX..." i vyn Extensions

`.typl`-filer öppnas automatiskt i typelisp-läge.

## Tangentbindningar

| Tangent | Kommando | Vad det gör |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Spara och kör `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | Starta REPL för `typl` |

Kommandopaletten har också `typelisp: Restart Language Server`.

## Inställningar

| Inställning | Standard | Vad det gör |
|---|---|---|
| `typelisp.program` | `typl` | Sökväg till CLI:t `typl` |
| `typelisp.languageServer.enable` | `true` | Om man ska ansluta till `typl-lsp` |
| `typelisp.languageServer.path` | (tom) | Sökväg till `typl-lsp`. När den är tom letar tillägget efter arbetsytans `target/release/typl-lsp`, sedan `target/debug/typl-lsp`, sedan `PATH` |
| `typelisp.trace.server` | `off` | Logga LSP JSON-RPC-trafiken |

Bygg språkservern med:

```sh
cargo build --release --bin typl-lsp
```

Referenser mellan filer genom `use` löses upp genom att söka uppåt efter projektrotens
`typelisp.toml` (för detaljer se
[Syntaxreferens 3.11](../../docs/sv/reference/syntax.md#311-filer-och-moduler-projekt-med-flera-filer)).

## Problemmatcharen för uppgifter

Tillägget tillhandahåller en problemmatchare med namnet `typelisp`. `typl` skriver ut diagnostik i formen
`error: FILE:LINE:COL: message`, så den kan gå rakt till panelen Problems:

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

## Utveckling

```sh
npm run compile   # tsc
npm run watch     # bygg vid ändring
npm test          # node --test (grammatik, indrag, symboler, typreferenser, manifest)
```

Testerna täcker bara de delar som inte behöver modulen `vscode`. För det är `src/indent.ts` och
`src/symbols.ts` skrivna som rena funktioner, och bara `src/extension.ts` rör editorns API.

- `src/test/grammar.test.ts` — tokeniserar på riktigt med grammatiken, med samma motor som VS Code
  (`vscode-textmate` + `vscode-oniguruma`), och kontrollerar resultatet.
  Oniguruma skiljer sig från Emacs reguljära uttryck i detaljer (till exempel behandlar det inte ett
  `]` i början av en teckenklass som en literal), och sådana skillnader hittas bara genom att köra den
  riktiga motorn.
- `src/test/indent.test.ts` — för varje `.typl`-fil under `examples/` krävs att
  **plattgöra allt indrag och återställa det stämmer med det incheckade innehållet byte för byte**.
  Emacs-läget uppfyller samma standard på samma filer, och det är det som gör "de två editorerna är
  överens" till ett verifierat påstående.
  Dessutom är `src/test/fixtures/emacs-indent-reference.txt` referensutdata som samlats genom att
  faktiskt köra `indent-region` i en Emacs `typelisp-mode`-buffert. Den förväntade sidan är inte en
  omformulering av TS-implementationen utan **det den andra editorn faktiskt producerar**, så
  portens trohet kontrolleras direkt (den omfattar `let*` `do` `doiter` `labels` `impl`
  `pprint-logical-block`, quote-prefix och mer).
- `src/test/symbols.test.ts` — innehållet i dispositionen och reservlösningens upptäckt av
  typreferenser. Antalet definitioner måste stämma exakt med en oberoende räkning av definitionsformer i
  början av rader. Gränsreglerna för typreferenser är medvetet anpassade till Emacs-versionens
  reservlösning (VS Code använder en lookbehind; Emacs uttrycker samma mängd genom att förbruka ett
  föregående tecken).
- Serverns upplösningsdrivna tokens (`crates/typelisp-front/src/check/semantic.rs`) kontrolleras av
  `cargo test --test lsp_semantic_test` och `scripts/lsp-semantic-smoke.py` (som driver en riktig process
  över stdio). Emacs-klienten kontrolleras av
  `scripts/emacs-semantic-smoke.el` över en riktig eglot-anslutning.
- `src/test/manifest.test.ts` — `package.json` är den enda delen kompilatorn inte kontrollerar, så detta
  kontrollerar att de deklarerade kommandona och anropen av `registerCommand` är samma mängd, vad
  tangentbindningarna refererar till, att de inställningar koden läser är deklarerade, och att
  problemmatcharen kan tolka det `typl` faktiskt skriver ut.

### Upptäcka avdrift i editordefinitionerna

Nyckelordstabellerna underhålls två gånger, i Emacs-versionen och i VS Code-versionen. För att
förhindra att editordefinitionerna hamnar efter medan implementationen går vidare finns ett test på
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

## Anmärkningar

- typelisp gör symboler till gemener vid läsning, men färgläggningen skiljer på stora och små bokstäver så
  att typnamn som börjar med en versal kan skiljas åt.
- Indraget avgörs av `INDENT_SPECS` i `src/indent.ts`. Det är en port av Emacs-versionens
  `typelisp-indent-specs`, med samma värden och regler. Platser där en form skiljer sig i form från Emacs
  Lisp-formen med samma namn förs över som de är: huvudet i
  `(defun NAME (PARAMS) RETTYPE ...)` har tre element, `if` är fast vid tre element med ett obligatoriskt
  `else`, och så vidare.
- Innehållet i `#| ... |#` dras om vid formatering. Detta stämmer med beteendet hos Emacs `indent-region`.
