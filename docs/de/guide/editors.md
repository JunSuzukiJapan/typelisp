<!-- translated-from: docs/ja/guide/editors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Editor-Anbindung (typl-lsp)

`typl-lsp` ist der Sprachserver von typelisp. An einen Editor angebunden, der LSP (das Language Server
Protocol) unterstützt, stellt er für die gerade bearbeitete Datei diese Funktionen bereit:

- Diagnosen: Lesefehler, Typfehler und Warnungen bei Neudefinitionen
- Hover: der Typ eines geklammerten Ausdrucks und der Docstring der Definition, die er aufruft (für einfache
  Variablennamen nicht angezeigt)
- Gehe zur Definition
- Vervollständigung (Kandidaten erscheinen, wenn man `:` tippt)
- Einfärbung von Typnamen (semantische Tokens), auch für Typen, die mit `use` aus anderen Dateien
  hereingeholt werden

Dateiübergreifende Verweise über `use` werden aufgelöst. Nicht gespeicherte Änderungen an einer anderen
geöffneten Datei schlagen sich sofort in den Diagnosen der Dateien nieder, die sie mit `use` hereinholen.

## 1. Bauen

```sh
cargo build --release --bin typl-lsp
```

Das erzeugt `target/release/typl-lsp`. Wer wie in der [README.md](../../../README.md) beschrieben mit
`cargo install` installiert hat, findet es neben `typl` unter `~/.cargo/bin/typl-lsp`.

## 2. VS Code

Die Erweiterung liegt im Repository unter `editor/vscode`. Sie ist nicht im Marketplace veröffentlicht, daher
baut und installiert man sie selbst.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # erzeugt eine .vsix
```

Im Menü „...“ der Erweiterungsansicht wählt man „Install from VSIX...“ und dann die gebaute `.vsix`.

Die Erweiterung sucht `typl-lsp` unter `target/release/typl-lsp` im Arbeitsbereich, dann unter
`target/debug/typl-lsp`, dann im `PATH`. Liegt es woanders, trägt man den Pfad in die Einstellung
`typelisp.languageServer.path` ein.

| Einstellung | Standard | Bedeutung |
|---|---|---|
| `typelisp.program` | `typl` | Pfad von `typl` |
| `typelisp.languageServer.enable` | `true` | Ob mit `typl-lsp` verbunden wird |
| `typelisp.languageServer.path` | (leer) | Pfad von `typl-lsp` |

`Ctrl+Alt+R` speichert die bearbeitete Datei und führt sie mit `typl` aus, `Ctrl+Alt+Z` startet die REPL.
Mehr dazu in der [README der VS-Code-Erweiterung](../../../editor/vscode/README.md) (auf Japanisch).

## 3. Emacs

`typelisp-mode` liegt im Repository unter `editor/emacs`.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Einstellungen, um mit `eglot` (ab Emacs 29 enthalten) eine Verbindung zu `typl-lsp` herzustellen:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Mit `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

eglot unterstützt keine semantischen Tokens; bei eglot färbt `typelisp-mode` die Typnamen deshalb selbst ein.
Mit `lsp-mode` setzt man `lsp-semantic-tokens-enable` auf `t`.

`C-c C-c` führt die bearbeitete Datei aus, `C-c C-z` startet die REPL. Mehr dazu in der
[README von typelisp-mode](../../../editor/emacs/README.md) (auf Japanisch).

## 4. Andere Editoren

`typl-lsp` spricht LSP über Standardein- und -ausgabe und nimmt keine Kommandozeilenargumente. Man
konfiguriert den LSP-Client des Editors so, dass er `typl-lsp` für `.typl`-Dateien startet.

## 5. Wie Projekte erkannt werden

`typl-lsp` sucht `typelisp.toml` ausgehend vom Verzeichnis der geöffneten Datei nach oben und löst `use` mit
dem Fundort als Quellwurzel auf. Es gelten dieselben Regeln wie beim Ausführen einer Datei durch `typl`
([Module und Dateiaufteilung](modules.md#2-ein-projekt-einrichten)). Bei einem Projekt aus mehreren Dateien
legt man `typelisp.toml` in dessen Wurzel.

## 6. Der Sprachserver führt das Programm nicht aus

`typl-lsp` erzeugt Diagnosen allein durch Lesen und Typprüfen. Das bearbeitete Programm wird nie ausgeführt.
Diagnosen laufen bei jedem Tastendruck, daher kann dort weder Code mit Seiteneffekten noch Code laufen, der
nie endet. Die einzige Ausnahme ist das Registrieren von `defmacro`s, das nötig ist, um die nachfolgenden
Makroaufrufe zu prüfen.

Deshalb erscheinen Fehler, die nur auftreten, wenn `typl` das Programm ausführt (`panic`, eine fehlende Datei
usw.), nicht in den Diagnosen des Sprachservers.
