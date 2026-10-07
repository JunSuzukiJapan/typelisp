<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Editorintegration (typl-lsp)

`typl-lsp` är språkservern för typelisp. Kopplad till en editor som stöder LSP (Language Server
Protocol) ger den dessa funktioner för filen du redigerar:

- Diagnostik: läsfel, typfel och varningar om omdefinition
- Hovring: typen på ett parentesuttryck och dokumentationssträngen för den definition det anropar
  (visas inte för rena variabelnamn)
- Gå till definition
- Komplettering (kandidater visas när du skriver `:`)
- Färgläggning av typnamn (semantiska tokens), inklusive typer som hämtats med `use` från andra filer

Referenser mellan filer via `use` löses upp. Osparade ändringar i en annan öppen fil syns direkt i
diagnostiken för de filer som använder den med `use`.

## 1. Bygga

```sh
cargo build --release --bin typl-lsp
```

Detta ger `target/release/typl-lsp`. Om du installerade med `cargo install` enligt beskrivningen i
[README.md](../../../README.md) ligger den i `~/.cargo/bin/typl-lsp` tillsammans med `typl`.

## 2. VS Code

Tillägget finns i `editor/vscode` i repositoriet. Det är inte publicerat på Marketplace, så bygg och
installera det själv.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # skapar en .vsix
```

Välj "Install from VSIX..." i "..."-menyn i vyn Extensions och välj den `.vsix` du byggde.

Tillägget letar efter `typl-lsp` i arbetsytans `target/release/typl-lsp`, sedan i
`target/debug/typl-lsp` och därefter i `PATH`. Om du har lagt den någon annanstans skriver du dess
sökväg i inställningen `typelisp.languageServer.path`.

| Inställning | Standard | Betydelse |
|---|---|---|
| `typelisp.program` | `typl` | Sökväg till `typl` |
| `typelisp.languageServer.enable` | `true` | Om man ska ansluta till `typl-lsp` |
| `typelisp.languageServer.path` | (tom) | Sökväg till `typl-lsp` |

`Ctrl+Alt+R` sparar filen du redigerar och kör den med `typl`, och `Ctrl+Alt+Z` startar REPL. Mer
finns i [README för VS Code-tillägget](../../../editor/vscode/README_sv.md).

## 3. Emacs

`typelisp-mode` finns i `editor/emacs` i repositoriet.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Inställningar för att ansluta till `typl-lsp` med `eglot` (medföljer Emacs 29 och senare):

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

Eglot i Emacs 31 och senare färglägger typnamn (semantiska tokens) själv. Eglot i Emacs 30 och
tidigare stöder dem inte, så där färglägger `typelisp-mode` typnamnen i stället. Med `lsp-mode` sätter
du `lsp-semantic-tokens-enable` till `t`.

`C-c C-c` kör filen du redigerar och `C-c C-z` startar REPL. Mer finns i
[README för typelisp-mode](../../../editor/emacs/README_sv.md).

## 4. Andra editorer

`typl-lsp` talar LSP över standard in och ut och tar inga kommandoradsargument. Konfigurera
editorns LSP-klient så att den startar `typl-lsp` för `.typl`-filer.

## 5. Hur projekt identifieras

`typl-lsp` letar efter `typelisp.toml` med början i katalogen för den öppnade filen och går uppåt,
och löser upp `use` med den platsen som källrot. Det är samma regler som när `typl` kör en fil
([Moduler och filuppdelning](modules.md#2-sätta-upp-ett-projekt)). För ett projekt som består av flera
filer lägger du `typelisp.toml` i dess rot.

## 6. Språkservern kör inte ditt program

`typl-lsp` skapar diagnostik enbart genom att läsa och typkontrollera. Den kör aldrig programmet du
redigerar. Diagnostiken körs vid varje tangenttryckning, så den har inte råd att där köra kod med
sidoeffekter eller kod som aldrig tar slut. Det enda undantaget är registreringen av `defmacro`, som
behövs för att kontrollera de makroanrop som kommer efter dem.

Därför syns fel som bara uppstår när `typl` kör programmet (`panic`, en saknad fil och så vidare)
inte i språkserverns diagnostik.
