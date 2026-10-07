<!-- translated-from: editor/emacs/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-mode (Emacs)

Een major mode van Emacs voor het bewerken van typelisp-broncode (`.typl`).
De VS Code-versie staat in [../vscode/](../vscode/README_nl.md). De twee delen dezelfde
sleutelwoordtabellen en dezelfde inspringregels, en `cargo test --test editor_keyword_sync_test`
controleert dat mechanisch (zie het einde van dit document).

## Functies

- Syntaxiskleuring
  - Speciale vormen en besturingsconstructies (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, de `pprint`-familie, enzovoort)
  - Gedefinieerde namen (de `NAME` van `(defun NAME ...)` als functienaam, van `(defstruct NAME ...)`
    als typenaam, en van `(defvar (NAME ...))` als variabelenaam; hetzelfde met `pub`, zoals in
    `(pub defun NAME ...)`)
  - Namespace- en declaratiesleutelwoorden (`pub` `module` `use` `load` `impl` `where`) en
    markeringen van lambdalijsten (`&rest` `&optional` `&key`)
  - Ingebouwde functies (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` enzovoort)
  - Primitieve types (inclusief `bignum` / `ratio`), ingebouwde types, ingebouwde foutentypes
    (`ParseIntError` enzovoort), door de gebruiker gedefinieerde types met hoofdletter (`Capitalized`),
    en het trait-objecttype `:dyn Trait`
  - **Gebruik van door de gebruiker gedefinieerde types** (de namen van
    `defstruct`/`defenum`/`deftrait` zijn meestal kleine letters (`rect` `todo-item` `board`), dus de
    regel `Capitalized` vangt ze niet). Wanneer verbonden met `typl-lsp`, worden ze gekleurd uit de
    semantic tokens van de server (dit werkt ook met `eglot`; zie hieronder). Wanneer niet verbonden,
    valt de modus terug op het verzamelen van de typenamen die in de buffer zijn gedefinieerd
  - Literals (`true` `false`, getalliterals (decimaal / `0xff` / `1.5` / `1/3`), tekenliterals zoals
    `#\Space`, strings, keywords zoals `:name`)
  - `format`-besturingsdirectieven binnen strings (`~a` `~5,'0d` `~{...~}` enzovoort)
  - Globale variabelen met sterretjes in de stijl van CL (`*print-pretty*` enzovoort)
- Commentaar
  - Regelcommentaar `;`
  - **Nestbaar** blokcommentaar `#| ... |#`
- Navigatie door S-expressies en inspringing in Lisp-stijl
- Een definitie-index via `imenu` (functies / methoden / macro's / types / traits / `impl` /
  variabelen / modules)
- Commando's die de `typl`-CLI uitvoeren (hieronder)

## Sneltoetsen

| Toets | Commando | Wat het doet |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Opslaan en `typl FILE` uitvoeren (via `compile`, zodat je naar foutregels kunt springen) |
| `C-c C-z` | `typelisp-repl` | De `typl`-REPL in een comint-buffer starten |

Stel de locatie van `typl` in met `typelisp-program` (standaard `"typl"`).
Diagnostiek heeft de vorm `error: FILE:LINE:COL: ...`, die `compilation-mode` kan parsen, dus
`next-error` / `C-x \`` springt rechtstreeks naar de plek.

## Installatie

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`.typl`-bestanden openen automatisch in `typelisp-mode` (de modus is in `auto-mode-alist`
geregistreerd).

Met `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Language server (`typl-lsp`)

Zodra `typl-lsp` is gebouwd, kan hij vanuit `eglot` (ingebouwd in Emacs 29+) of `lsp-mode` worden
gebruikt.

```sh
cargo build --release --bin typl-lsp
```

Met `eglot`:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Met `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Ondersteund: diagnostiek (syntaxis-/typefouten en waarschuwingen over herdefinities, verzonden via
`textDocument/publishDiagnostics`), hover, naar definitie gaan, aanvulling (`:` is als triggerteken
geregistreerd) en semantic tokens. Verwijzingen tussen bestanden via `use` worden opgelost (de server
zoekt omhoog naar de `typelisp.toml` van de projectroot; voor details zie
[Syntaxreferentie 3.11](../../docs/nl/reference/syntax.md#311-bestanden-en-modules-projecten-met-meerdere-bestanden)).
Niet-opgeslagen wijzigingen in geopende editorbuffers worden meteen weerspiegeld in de diagnostiek van
zowel de bestanden waarvan ze afhangen als de bestanden die ervan afhangen.

### Typenamen kleuren (semantic tokens)

Via `textDocument/semanticTokens` meldt de server **de posities die de checker werkelijk als typenamen
heeft opgelost**. Omdat dit geen tekstmatching is:

- Types die via `use` uit andere bestanden komen worden ook gekleurd (een bereik dat oplossing binnen de
  buffer in principe niet kan bereiken)
- Aanroepen van een **functie** met dezelfde naam als een type worden niet gekleurd (de checker loste ze
  als functies op, dus daar wordt in de eerste plaats geen token vastgelegd)

Aan de clientkant:

- **`eglot` (Emacs 31 en later)**: eglot tekent de tokens zelf (`eglot-semantic-tokens-mode`).
  `typelisp-mode` houdt zich afzijdig
- **`eglot` (Emacs 30 en eerder)**: deze versie van eglot verwerkt semanticTokens niet. Dus
  **`typelisp-mode` stuurt het verzoek zelf en tekent het resultaat met overlays**
  (`typelisp-semantic-tokens-mode`, automatisch ingeschakeld wanneer eglot verbindt)
- **`lsp-mode`**: native ondersteuning (zet `lsp-semantic-tokens-enable` op `t`). In dat geval houdt
  `typelisp-mode` zich afzijdig

`scripts/emacs-semantic-smoke.el` maakt echt verbinding via eglot en controleert de kant die tekent in de
gebruikte Emacs. Bij elke client treedt de terugvaloptie in de buffer terug zolang de server antwoordt
(zodat twee regelsets niet dezelfde buffer verven).

| Instelling | Standaard | Wat het doet |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Bij de eglot van Emacs 30 en eerder: of er wordt gekleurd uit de semantic tokens van de server |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Inactieve seconden na een wijziging voordat opnieuw wordt gevraagd (houd het groter dan `eglot-send-changes-idle-time`) |

## Opmerkingen

- typelisp zet symbolen bij het lezen om naar kleine letters, maar de kleuring is hoofdlettergevoelig
  zodat typenamen die met een hoofdletter beginnen kunnen worden onderscheiden.
- De inspringing wordt bepaald door de speciale `typelisp-indent-function`, die
  `typelisp-indent-specs` (een alist) raadpleegt. De modus houdt eigen invoeren bij, zelfs voor vormen
  waarvan de naam met Emacs Lisp wordt gedeeld (`defun` `let` `if` ...), omdat symbooleigenschappen
  **globaal** zijn, en typelisp-instellingen daar de inspringing van andere Lisp-buffers in dezelfde
  sessie zouden veranderen. En de vormen van typelisp verschillen in vorm, ook wanneer ze een naam met
  Emacs Lisp delen: `(defun NAME (PARAMS) RETTYPE ...)` heeft drie headerelementen, en `if` ligt vast op
  drie elementen met een verplichte `else`. Dus de waarden kunnen ook niet worden gedeeld.
  Elk `.typl`-bestand onder `examples/` is gecontroleerd: `indent-region` wijzigt geen enkele byte, en het
  platmaken van alle inspringing en opnieuw inspringen herstelt het origineel (de VS Code-versie voldoet
  aan dezelfde norm op dezelfde bestanden).

## Afwijkingen in de editordefinities opsporen

De sleutelwoordtabellen worden tweemaal bijgehouden, eenmaal hier en eenmaal in de VS Code-versie. Om te
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
