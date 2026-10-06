<!-- translated-from: editor/emacs/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp-mode (Emacs)

Ein Emacs-Hauptmodus zum Bearbeiten von typelisp-Quelltext (`.typl`).
Die VS-Code-Version liegt in [../vscode/](../vscode/README_de.md). Beide verwenden dieselben
Schlüsselworttabellen und dieselben Einrückungsregeln, und `cargo test --test editor_keyword_sync_test`
prüft das maschinell (siehe Ende dieses Dokuments).

## Funktionen

- Syntaxhervorhebung
  - Spezialformen und Kontrollkonstrukte (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, die `pprint`-Familie usw.)
  - Definierte Namen (das `NAME` von `(defun NAME ...)` als Funktionsname, das von
    `(defstruct NAME ...)` als Typname und das von `(defvar (NAME ...))` als Variablenname;
    ebenso mit `pub`, wie in `(pub defun NAME ...)`)
  - Schlüsselwörter für Namensräume und Deklarationen (`pub` `module` `use` `load` `impl` `where`)
    und Lambda-Listen-Marker (`&rest` `&optional` `&key`)
  - Eingebaute Funktionen (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` usw.)
  - Primitive Typen (einschließlich `bignum` / `ratio`), eingebaute Typen, eingebaute Fehlertypen
    (`ParseIntError` usw.), `Capitalized`-Benutzertypen und der Trait-Objekttyp `:dyn Trait`
  - **Verwendungen benutzerdefinierter Typen** (die Namen von `defstruct`/`defenum`/`deftrait` sind
    meist kleingeschrieben (`rect` `todo-item` `board`), daher erfasst die `Capitalized`-Regel sie
    nicht). Bei Verbindung zu `typl-lsp` werden sie anhand der Semantic Tokens des Servers gefärbt
    (das funktioniert auch mit `eglot`; siehe unten). Ohne Verbindung weicht der Modus darauf aus,
    die im Puffer definierten Typnamen zu sammeln
  - Literale (`true` `false`, Zahlliterale (dezimal / `0xff` / `1.5` / `1/3`), Zeichenliterale wie
    `#\Space`, Zeichenketten, Schlüsselwörter wie `:name`)
  - `format`-Steuerdirektiven in Zeichenketten (`~a` `~5,'0d` `~{...~}` usw.)
  - Globale Variablen mit „Ohrenschützern“ im CL-Stil (`*print-pretty*` usw.)
- Kommentare
  - Zeilenkommentare `;`
  - **Verschachtelbare** Blockkommentare `#| ... |#`
- Navigation über S-Ausdrücke und Einrückung im Lisp-Stil
- Ein Definitionsverzeichnis über `imenu` (Funktionen / Methoden / Makros / Typen / Traits /
  `impl` / Variablen / Module)
- Befehle, die die `typl`-CLI ausführen (siehe unten)

## Tastenbelegung

| Taste | Befehl | Wirkung |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Speichert und führt `typl FILE` aus (über `compile`, daher kann man zu Fehlerzeilen springen) |
| `C-c C-z` | `typelisp-repl` | Startet die REPL von `typl` in einem comint-Puffer |

Den Ort von `typl` legt man mit `typelisp-program` fest (Standard `"typl"`).
Diagnosen haben die Form `error: FILE:LINE:COL: ...`, die `compilation-mode` auswerten kann, sodass
`next-error` / `C-x \`` direkt zur Stelle springt.

## Installation

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

`.typl`-Dateien werden automatisch in `typelisp-mode` geöffnet (der Modus ist in
`auto-mode-alist` eingetragen).

Mit `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Sprachserver (`typl-lsp`)

Sobald `typl-lsp` gebaut ist, lässt er sich aus `eglot` (in Emacs 29+ enthalten) oder `lsp-mode`
verwenden.

```sh
cargo build --release --bin typl-lsp
```

Mit `eglot`:

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

Unterstützt werden Diagnosen (Syntax- und Typfehler sowie Warnungen zu Neudefinitionen, gemeldet
über `textDocument/publishDiagnostics`), Hover, Sprung zur Definition (goto-definition),
Vervollständigung (`:` ist als Auslösezeichen registriert) und Semantic Tokens. Verweise über
Dateigrenzen hinweg per `use` werden aufgelöst (der Server sucht aufwärts nach der `typelisp.toml`
der Projektwurzel; Einzelheiten in der
[Syntaxreferenz 3.11](../../docs/de/reference/syntax.md#311-dateien-und-module-projekte-mit-mehreren-dateien)).
Ungespeicherte Änderungen in geöffneten Puffern wirken sich sofort auf die Diagnosen sowohl der
Dateien aus, von denen sie abhängen, als auch der Dateien, die von ihnen abhängen.

### Hervorhebung von Typnamen (Semantic Tokens)

Über `textDocument/semanticTokens` meldet der Server **die Stellen, die der Checker tatsächlich als
Typnamen aufgelöst hat**. Da das kein Textabgleich ist:

- Typen, die per `use` aus anderen Dateien kommen, werden ebenfalls gefärbt (ein Bereich, den eine
  Auflösung innerhalb des Puffers grundsätzlich nicht erreicht)
- Aufrufe einer **Funktion** mit demselben Namen wie ein Typ werden nicht gefärbt (der Checker hat
  sie als Funktionen aufgelöst, daher wird dort gar kein Token aufgezeichnet)

Auf der Clientseite:

- **`eglot` (Emacs 31 und neuer)**: eglot zeichnet die Tokens selbst
  (`eglot-semantic-tokens-mode`). `typelisp-mode` hält sich heraus
- **`eglot` (Emacs 30 und älter)**: diese Version von eglot verarbeitet semanticTokens nicht. Daher
  **sendet `typelisp-mode` die Anfrage selbst und zeichnet das Ergebnis mit Overlays**
  (`typelisp-semantic-tokens-mode`, wird beim Verbinden von eglot automatisch eingeschaltet)
- **`lsp-mode`**: native Unterstützung (`lsp-semantic-tokens-enable` auf `t` setzen). In diesem
  Fall hält sich `typelisp-mode` heraus

`scripts/emacs-semantic-smoke.el` verbindet sich tatsächlich über eglot und prüft die Seite, die im
verwendeten Emacs das Zeichnen übernimmt. Bei jedem Client tritt die pufferinterne Ausweichlösung
zurück, solange der Server antwortet (damit nicht zwei Regelsätze denselben Puffer färben).

| Einstellung | Standard | Wirkung |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Ob mit dem eglot von Emacs 30 und älter anhand der Semantic Tokens des Servers gefärbt wird |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Leerlaufsekunden nach einer Änderung, bevor erneut angefragt wird (größer als `eglot-send-changes-idle-time` halten) |

## Hinweise

- typelisp wandelt Symbole beim Lesen in Kleinbuchstaben um, die Hervorhebung unterscheidet aber
  Groß- und Kleinschreibung, damit Typnamen mit großem Anfangsbuchstaben unterscheidbar bleiben.
- Die Einrückung bestimmt die eigene Funktion `typelisp-indent-function`, die in
  `typelisp-indent-specs` (einer Assoziationsliste) nachschlägt. Der Modus führt eigene Einträge
  auch für Formen, deren Namen er mit Emacs Lisp teilt (`defun` `let` `if` ...), weil
  Symboleigenschaften **global** sind und typelisp-Einstellungen dort die Einrückung anderer
  Lisp-Puffer derselben Sitzung ändern würden. Außerdem unterscheiden sich die Formen von typelisp
  in ihrer Gestalt, selbst wenn sie den Namen mit Emacs Lisp teilen —
  `(defun NAME (PARAMS) RETTYPE ...)` hat drei Kopfelemente, und `if` hat fest drei Elemente mit
  einem zwingenden `else` —, daher lassen sich auch die Werte nicht teilen.
  Für jede `.typl`-Datei unter `examples/` ist geprüft, dass `indent-region` kein einziges Byte
  ändert und dass das Entfernen aller Einrückung mit anschließendem Neueinrücken das Original
  wiederherstellt (die VS-Code-Version erfüllt dasselbe Kriterium mit denselben Dateien).

## Erkennen von Abweichungen in den Editordefinitionen

Die Schlüsselworttabellen werden doppelt gepflegt, hier und in der VS-Code-Version. Damit die
Editordefinitionen nicht zurückbleiben, während die Implementierung voranschreitet, gibt es auf
der Rust-Seite einen Test:

```sh
cargo test --test editor_keyword_sync_test
```

Er lädt tatsächlich das Prelude, durchläuft die Registry und meldet **Namen, die einer der beiden
Editoren nicht kennt**. Spezialformen haben keine Laufzeitdarstellung, daher werden sie zwischen
`// SPECIAL-FORM DISPATCH BEGIN` / `END` in `crates/typelisp-front/src/check/checker.rs` gelesen
(diese Kommentare nicht löschen). Schlägt er fehl, die gemeldeten Namen in **beide**
Editordefinitionen eintragen.

Derselbe Test vergleicht auch die Legende der Semantic Tokens (`SEMANTIC_TOKEN_TYPES` in
`src/bin/lsp.rs` und die Tabellen beider Editoren müssen in Namen und Reihenfolge übereinstimmen).
Eine Abweichung verursacht keinen Laufzeitfehler; sie vertauscht nur die Farben aller Tokens, daher
ist das maschinell festgehalten.
