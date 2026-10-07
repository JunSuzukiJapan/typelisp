<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Editorintegratie (typl-lsp)

`typl-lsp` is de typelisp-language server. Verbonden met een editor die LSP (het Language Server
Protocol) ondersteunt, biedt hij deze functies voor het bestand dat je bewerkt:

- Diagnostiek: leesfouten, typefouten en waarschuwingen over herdefinities
- Hover: het type van een expressie tussen haakjes en de docstring van de definitie die wordt
  aangeroepen (niet getoond voor kale variabelenamen)
- Naar definitie gaan
- Aanvulling (kandidaten verschijnen als je `:` typt)
- Kleuring van typenamen (semantic tokens), ook van types die met `use` uit andere bestanden komen

Verwijzingen tussen bestanden via `use` worden opgelost. Niet-opgeslagen wijzigingen in een ander
geopend bestand worden meteen weerspiegeld in de diagnostiek van de bestanden die dat bestand met
`use` gebruiken.

## 1. Bouwen

```sh
cargo build --release --bin typl-lsp
```

Dit levert `target/release/typl-lsp` op. Als je met `cargo install` hebt geïnstalleerd, zoals
beschreven in [README.md](../../../README.md), staat het in `~/.cargo/bin/typl-lsp`, naast `typl`.

## 2. VS Code

De extensie staat in `editor/vscode` in de repository. Ze is niet gepubliceerd op de Marketplace,
dus bouw en installeer haar zelf.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # produces a .vsix
```

Kies "Install from VSIX..." in het "..."-menu van de weergave Extensions en selecteer de `.vsix`
die je hebt gebouwd.

De extensie zoekt `typl-lsp` in `target/release/typl-lsp` van de werkruimte, daarna in
`target/debug/typl-lsp` en daarna op `PATH`. Staat het ergens anders, schrijf dan het pad op in de
instelling `typelisp.languageServer.path`.

| Instelling | Standaard | Betekenis |
|---|---|---|
| `typelisp.program` | `typl` | Pad van `typl` |
| `typelisp.languageServer.enable` | `true` | Of er verbinding met `typl-lsp` wordt gemaakt |
| `typelisp.languageServer.path` | (leeg) | Pad van `typl-lsp` |

`Ctrl+Alt+R` slaat het bestand dat je bewerkt op en voert het uit met `typl`, en `Ctrl+Alt+Z`
start de REPL. Meer informatie staat in de [README van de VS Code-extensie](../../../editor/vscode/README_nl.md).

## 3. Emacs

`typelisp-mode` staat in `editor/emacs` in de repository.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Instellingen om met `eglot` (meegeleverd met Emacs 29 en later) verbinding te maken met `typl-lsp`:

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

De eglot van Emacs 31 en later kleurt typenamen (semantic tokens) zelf. De eglot van Emacs 30 en
eerder ondersteunt die niet, dus `typelisp-mode` kleurt de typenamen dan zelf. Zet bij `lsp-mode`
`lsp-semantic-tokens-enable` op `t`.

`C-c C-c` voert het bestand uit dat je bewerkt, en `C-c C-z` start de REPL. Meer informatie staat in de
[README van typelisp-mode](../../../editor/emacs/README_nl.md).

## 4. Andere editors

`typl-lsp` spreekt LSP via standaardinvoer en standaarduitvoer en accepteert geen
opdrachtregelargumenten. Configureer de LSP-client van je editor zo dat hij `typl-lsp` start voor
`.typl`-bestanden.

## 5. Hoe projecten worden herkend

`typl-lsp` zoekt naar `typelisp.toml`, te beginnen bij de map van het geopende bestand en dan
omhoog, en lost `use` op met die plek als bronroot. Dit zijn dezelfde regels als wanneer `typl` een
bestand uitvoert ([Modules en bestandsindeling](modules.md#2-een-project-opzetten)). Zet bij een
project dat uit meerdere bestanden bestaat `typelisp.toml` in de root ervan.

## 6. De language server voert je programma niet uit

`typl-lsp` levert diagnostiek op door alleen te lezen en te typechecken. Hij voert het programma dat
je bewerkt nooit uit. De diagnostiek draait bij elke toetsaanslag, dus code met neveneffecten of code
die nooit eindigt kan daar niet worden uitgevoerd. De enige uitzondering is het registreren van
`defmacro`s, wat nodig is om de macro-aanroepen te controleren die erna komen.

Hierdoor verschijnen fouten die pas optreden wanneer `typl` het programma uitvoert (`panic`, een
ontbrekend bestand enzovoort) niet in de diagnostiek van de language server.
