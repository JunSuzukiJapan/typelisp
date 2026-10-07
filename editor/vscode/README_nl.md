<!-- translated-from: editor/vscode/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp (VS Code)

Een VS Code-extensie voor het bewerken van typelisp-broncode (`.typl`).
De Emacs-versie staat in [../emacs/](../emacs/README_nl.md). De twee delen dezelfde sleutelwoordtabellen
en dezelfde inspringregels, en `cargo test --test editor_keyword_sync_test` controleert dat
mechanisch (zie hieronder).

## Functies

- **Syntaxiskleuring** (een TextMate-grammatica; geen language server nodig)
  - Speciale vormen en besturingsconstructies (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, de `pprint`-familie)
  - Gedefinieerde namen (`(defun NAME ...)` als functie, `(defstruct NAME ...)` als type,
    `(defvar (NAME ...))` als variabele; hetzelfde met `pub`, zoals in `(pub defun NAME ...)`) en
    beide namen van `(impl Trait Type)`
  - Namespace- en declaratiesleutelwoorden (`pub` `module` `use` `load` `impl` `where`) en
    markeringen van lambdalijsten (`&rest` `&optional` `&key`)
  - Ingebouwde functies, primitieve types (inclusief `bignum` / `ratio`), ingebouwde foutentypes,
    door de gebruiker gedefinieerde types met hoofdletter (`Capitalized`), en het trait-objecttype
    `:dyn Trait` (ook binnen generieke argumenten)
  - Getalliterals (decimaal / `0xff` / `1.5` / `3.0e10` / `1/3`), tekenliterals zoals
    `#\Space`, keywords zoals `:name`, en globale variabelen met sterretjes zoals `*print-pretty*`
  - **`format`-besturingsdirectieven binnen strings** (`~a` `~5,'0d` `~{...~}` `~^` enzovoort)
  - Regelcommentaar `;` en **nestbaar** blokcommentaar `#| ... |#`
- **Gebruik van door de gebruiker gedefinieerde types** (semantic tokens)
  - De namen van `defstruct` / `defenum` / `deftrait` zijn meestal kleine letters (`rect` `todo-item`
    `board`), dus de regel `Capitalized` vangt ze niet, en een TextMate-grammatica werkt regel voor
    regel en kan niet het hele bestand zien. Semantic tokens kunnen dat wel, wat de toestand verhelpt
    waarin een statisch getypeerde taal alleen zijn typeannotaties ongekleurd liet
  - Wanneer verbonden met `typl-lsp`, ontvangt de extensie **de posities die de checker werkelijk als
    typenamen heeft opgelost**. Dus types die via `use` uit andere bestanden komen worden gekleurd, en
    aanroepen van een **functie** met dezelfde naam als een type niet (de checker loste ze als functies
    op, dus daar wordt in de eerste plaats geen token vastgelegd)
  - Wanneer de server niet is verbonden of niet is gebouwd, valt de extensie terug op een tekstscan die
    binnen het bestand oplost. Dat is een benadering: ze kan geen types uit andere bestanden vinden, en
    ze kan een functie met dezelfde naam als een type niet onderscheiden
- **Lisp-inspringing** (VS Code heeft geen ingebouwde Lisp-inspringing, dus de extensie implementeert
  haar)
  - Document opmaken, selectie opmaken, en opmaken tijdens het typen (Enter en `)`, wanneer
    `editor.formatOnType` is ingeschakeld)
- **Overzicht / broodkruimels / `Ctrl+Shift+O`** (functies, methoden, macro's, types, traits, `impl`,
  variabelen, modules)
- **`typl-lsp`-integratie** (diagnostiek, hover, naar definitie gaan, aanvulling, semantic tokens)
- **`typl`-CLI-commando's** (uitvoeren, REPL)

Alles behalve de language server werkt met de extensie alleen, dus zelfs in een checkout waar `typl-lsp`
niet is gebouwd zijn kleuring, inspringing, het overzicht en (bestandslokale) typekleuring beschikbaar.

## Installatie

De extensie staat niet op de Marketplace, dus bouw haar lokaal en installeer haar.

```sh
cd editor/vscode
npm install
npm run compile
```

Doe dan een van deze:

- **Probeer haar in een ontwikkelhost**: open `editor/vscode` in VS Code en druk op `F5`
- **Installeer haar permanent**: maak een `.vsix` met `npx @vscode/vsce package`, en gebruik dan
  "..." → "Install from VSIX..." in de weergave Extensions

`.typl`-bestanden openen automatisch in typelisp-modus.

## Sneltoetsen

| Toets | Commando | Wat het doet |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Opslaan en `typl FILE` uitvoeren |
| `Ctrl+Alt+Z` | `typelisp.repl` | De `typl`-REPL starten |

De opdrachtpalet heeft ook `typelisp: Restart Language Server`.

## Instellingen

| Instelling | Standaard | Wat het doet |
|---|---|---|
| `typelisp.program` | `typl` | Pad van de `typl`-CLI |
| `typelisp.languageServer.enable` | `true` | Of er verbinding met `typl-lsp` wordt gemaakt |
| `typelisp.languageServer.path` | (leeg) | Pad van `typl-lsp`. Indien leeg zoekt de extensie `target/release/typl-lsp` van de werkruimte, daarna `target/debug/typl-lsp`, daarna `PATH` |
| `typelisp.trace.server` | `off` | Log het LSP JSON-RPC-verkeer |

Bouw de language server met:

```sh
cargo build --release --bin typl-lsp
```

Verwijzingen tussen bestanden via `use` worden opgelost door omhoog te zoeken naar de
`typelisp.toml` van de projectroot (voor details zie
[Syntaxreferentie 3.11](../../docs/nl/reference/syntax.md#311-bestanden-en-modules-projecten-met-meerdere-bestanden)).

## De taak-probleemmatcher

De extensie biedt een probleemmatcher met de naam `typelisp`. `typl` drukt diagnostiek af in de vorm
`error: FILE:LINE:COL: message`, dus ze kan rechtstreeks naar het paneel Problemen:

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

## Ontwikkeling

```sh
npm run compile   # tsc
npm run watch     # build on change
npm test          # node --test (grammar, indentation, symbols, type references, manifest)
```

De tests dekken alleen de delen die de module `vscode` niet nodig hebben. Daarvoor zijn `src/indent.ts`
en `src/symbols.ts` als pure functies geschreven, en alleen `src/extension.ts` raakt de editor-API aan.

- `src/test/grammar.test.ts` — tokeniseert echt met de grammatica, met dezelfde engine als VS Code
  (`vscode-textmate` + `vscode-oniguruma`), en controleert het resultaat.
  Oniguruma verschilt in details van reguliere expressies van Emacs (het behandelt bijvoorbeeld een
  `]` aan het begin van een tekenklasse niet als letterlijk), en zulke verschillen worden alleen
  gevonden door de echte engine uit te voeren.
- `src/test/indent.test.ts` — vereist voor elk `.typl`-bestand onder `examples/` dat
  **het platmaken van alle inspringing en het herstellen ervan byte voor byte overeenkomt met de
  vastgelegde inhoud**. De Emacs-modus voldoet aan dezelfde norm op dezelfde bestanden, en dat maakt
  "de twee editors zijn het eens" tot een geverifieerde bewering.
  Daarnaast is `src/test/fixtures/emacs-indent-reference.txt` referentie-uitvoer die is verzameld door
  `indent-region` echt uit te voeren in een Emacs-buffer in `typelisp-mode`. De verwachte kant is geen
  herformulering van de TS-implementatie maar **wat de andere editor werkelijk produceert**, dus de
  getrouwheid van de port wordt rechtstreeks gecontroleerd (het omvat `let*` `do` `doiter` `labels`
  `impl` `pprint-logical-block`, quote-voorvoegsels en meer).
- `src/test/symbols.test.ts` — de inhoud van het overzicht en de detectie van typeverwijzingen door de
  terugvaloptie. Het aantal definities moet exact overeenkomen met een onafhankelijke telling van
  definitievormen aan het begin van regels. De grensregels voor typeverwijzingen zijn bewust afgestemd op
  de terugvaloptie van de Emacs-versie (VS Code gebruikt een lookbehind; Emacs drukt dezelfde verzameling
  uit door één voorafgaand teken te verbruiken).
- De door oplossing aangestuurde tokens van de server (`crates/typelisp-front/src/check/semantic.rs`)
  worden gecontroleerd door `cargo test --test lsp_semantic_test` en
  `scripts/lsp-semantic-smoke.py` (dat een echt proces via stdio aanstuurt). De Emacs-client wordt
  gecontroleerd door `scripts/emacs-semantic-smoke.el` via een echte eglot-verbinding.
- `src/test/manifest.test.ts` — `package.json` is het ene onderdeel dat de compiler niet controleert, dus
  dit controleert dat de gedeclareerde commando's en de `registerCommand`-aanroepen dezelfde verzameling
  zijn, waarnaar de sneltoetsen verwijzen, dat de instellingen die de code leest zijn gedeclareerd, en dat
  de probleemmatcher kan parsen wat `typl` werkelijk afdrukt.

### Afwijkingen in de editordefinities opsporen

De sleutelwoordtabellen worden tweemaal bijgehouden, in de Emacs-versie en de VS Code-versie. Om te
voorkomen dat de editordefinities achterop raken terwijl de implementatie verder gaat, is er een test aan
de Rust-kant:

```sh
cargo test --test editor_keyword_sync_test
```

Ze laadt echt de prelude, doorloopt het register en meldt **namen die een van beide editors niet kent**.
Speciale vormen hebben geen runtime-representatie, dus ze worden gelezen uit het gedeelte tussen
`// SPECIAL-FORM DISPATCH BEGIN` / `END` in `crates/typelisp-front/src/check/checker.rs` (verwijder deze
commentaren niet). Als ze faalt, voeg dan de gemelde namen aan **beide** editordefinities toe.

Dezelfde test vergelijkt ook de legenda van de semantic tokens (`SEMANTIC_TOKEN_TYPES` in
`src/bin/lsp.rs` en de tabellen die beide editors bevatten moeten in namen en volgorde overeenkomen).
Een afwijking veroorzaakt geen runtimefout; ze verwisselt alleen de kleuren van elk token, dus ze wordt
mechanisch vastgelegd.

## Opmerkingen

- typelisp zet symbolen bij het lezen om naar kleine letters, maar de kleuring is hoofdlettergevoelig
  zodat typenamen die met een hoofdletter beginnen kunnen worden onderscheiden.
- De inspringing wordt bepaald door `INDENT_SPECS` in `src/indent.ts`. Het is een port van
  `typelisp-indent-specs` van de Emacs-versie, met dezelfde waarden en regels. Plekken waar een vorm in
  vorm verschilt van de Emacs Lisp-vorm met dezelfde naam worden zoals ze zijn overgenomen: de header van
  `(defun NAME (PARAMS) RETTYPE ...)` heeft drie elementen, `if` ligt vast op drie elementen met een
  verplichte `else`, enzovoort.
- De inhoud van `#| ... |#` wordt bij het opmaken opnieuw ingesprongen. Dit komt overeen met het gedrag
  van `indent-region` van Emacs.
