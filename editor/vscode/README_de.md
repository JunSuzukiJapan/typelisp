<!-- translated-from: editor/vscode/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp (VS Code)

Eine VS-Code-Erweiterung zum Bearbeiten von typelisp-Quelltext (`.typl`).
Die Emacs-Version liegt in [../emacs/](../emacs/README_de.md). Beide verwenden dieselben
Schlüsselworttabellen und dieselben Einrückungsregeln, und `cargo test --test editor_keyword_sync_test`
prüft das maschinell (siehe unten).

## Funktionen

- **Syntaxhervorhebung** (eine TextMate-Grammatik; kein Sprachserver nötig)
  - Spezialformen und Kontrollkonstrukte (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, die `pprint`-Familie)
  - Definierte Namen (`(defun NAME ...)` als Funktion, `(defstruct NAME ...)` als Typ,
    `(defvar (NAME ...))` als Variable; ebenso mit `pub`, wie in `(pub defun NAME ...)`) und beide
    Namen von `(impl Trait Type)`
  - Schlüsselwörter für Namensräume und Deklarationen (`pub` `module` `use` `load` `impl` `where`)
    und Lambda-Listen-Marker (`&rest` `&optional` `&key`)
  - Eingebaute Funktionen, primitive Typen (einschließlich `bignum` / `ratio`), eingebaute
    Fehlertypen, `Capitalized`-Benutzertypen und der Trait-Objekttyp `:dyn Trait` (auch innerhalb
    generischer Argumente)
  - Zahlliterale (dezimal / `0xff` / `1.5` / `3.0e10` / `1/3`), Zeichenliterale wie `#\Space`,
    Schlüsselwörter wie `:name` und globale Variablen mit „Ohrenschützern“ wie `*print-pretty*`
  - **`format`-Steuerdirektiven in Zeichenketten** (`~a` `~5,'0d` `~{...~}` `~^` usw.)
  - Zeilenkommentare `;` und **verschachtelbare** Blockkommentare `#| ... |#`
- **Verwendungen benutzerdefinierter Typen** (Semantic Tokens)
  - Die Namen von `defstruct` / `defenum` / `deftrait` sind meist kleingeschrieben (`rect`
    `todo-item` `board`), daher erfasst die `Capitalized`-Regel sie nicht, und eine
    TextMate-Grammatik arbeitet zeilenweise und sieht nicht die ganze Datei. Semantic Tokens sehen
    sie, was den Zustand behebt, dass in einer statisch typisierten Sprache ausgerechnet die
    Typannotationen ungefärbt blieben
  - Bei Verbindung zu `typl-lsp` erhält die Erweiterung **die Stellen, die der Checker tatsächlich
    als Typnamen aufgelöst hat**. Daher werden Typen, die per `use` aus anderen Dateien kommen,
    gefärbt, und Aufrufe einer **Funktion** mit demselben Namen wie ein Typ nicht (der Checker hat
    sie als Funktionen aufgelöst, daher wird dort gar kein Token aufgezeichnet)
  - Ist der Server nicht verbunden oder nicht gebaut, weicht die Erweiterung auf eine Textsuche aus,
    die innerhalb der Datei auflöst. Das ist eine Näherung: sie findet keine Typen aus anderen
    Dateien und kann eine Funktion mit demselben Namen wie ein Typ nicht unterscheiden
- **Lisp-Einrückung** (VS Code hat keine eingebaute Lisp-Einrückung, daher implementiert die
  Erweiterung sie)
  - Dokument formatieren, Auswahl formatieren und Formatieren bei Eingabe (Enter und `)`, wenn
    `editor.formatOnType` aktiviert ist)
- **Outline / Breadcrumbs / `Ctrl+Shift+O`** (Funktionen, Methoden, Makros, Typen, Traits, `impl`,
  Variablen, Module)
- **Anbindung an `typl-lsp`** (Diagnosen, Hover, Sprung zur Definition, Vervollständigung,
  Semantic Tokens)
- **`typl`-CLI-Befehle** (Ausführen, REPL)

Alles außer dem Sprachserver funktioniert allein mit der Erweiterung, daher stehen selbst in einem
Checkout, in dem `typl-lsp` nicht gebaut ist, Hervorhebung, Einrückung, Outline und (dateilokale)
Typhervorhebung zur Verfügung.

## Installation

Die Erweiterung ist nicht im Marketplace, daher lokal bauen und installieren.

```sh
cd editor/vscode
npm install
npm run compile
```

Danach eines von beiden:

- **In einem Entwicklungshost ausprobieren**: `editor/vscode` in VS Code öffnen und `F5` drücken
- **Dauerhaft installieren**: mit `npx @vscode/vsce package` eine `.vsix` erzeugen und dann in der
  Erweiterungsansicht „...“ → „Install from VSIX...“ wählen

`.typl`-Dateien werden automatisch im typelisp-Modus geöffnet.

## Tastenbelegung

| Taste | Befehl | Wirkung |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Speichert und führt `typl FILE` aus |
| `Ctrl+Alt+Z` | `typelisp.repl` | Startet die REPL von `typl` |

Die Befehlspalette enthält außerdem `typelisp: Restart Language Server`.

## Einstellungen

| Einstellung | Standard | Wirkung |
|---|---|---|
| `typelisp.program` | `typl` | Pfad der `typl`-CLI |
| `typelisp.languageServer.enable` | `true` | Ob eine Verbindung zu `typl-lsp` hergestellt wird |
| `typelisp.languageServer.path` | (leer) | Pfad von `typl-lsp`. Ist er leer, wird in dieser Reihenfolge gesucht: `target/release/typl-lsp` des Arbeitsbereichs, `target/debug/typl-lsp`, `PATH` |
| `typelisp.trace.server` | `off` | Protokolliert den JSON-RPC-Verkehr von LSP |

Den Sprachserver baut man mit:

```sh
cargo build --release --bin typl-lsp
```

Verweise über Dateigrenzen hinweg per `use` werden aufgelöst, indem aufwärts nach der
`typelisp.toml` der Projektwurzel gesucht wird (Einzelheiten in der
[Syntaxreferenz 3.11](../../docs/de/reference/syntax.md#311-dateien-und-module-projekte-mit-mehreren-dateien)).

## Der Problem Matcher für Tasks

Die Erweiterung stellt einen Problem Matcher namens `typelisp` bereit. `typl` gibt Diagnosen in der
Form `error: FILE:LINE:COL: message` aus, daher können sie direkt im Problems-Panel landen:

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

## Entwicklung

```sh
npm run compile   # tsc
npm run watch     # bei Änderungen neu bauen
npm test          # node --test (Grammatik, Einrückung, Symbole, Typverweise, Manifest)
```

Die Tests decken nur die Teile ab, die das Modul `vscode` nicht brauchen. Dafür sind
`src/indent.ts` und `src/symbols.ts` als reine Funktionen geschrieben, und nur `src/extension.ts`
berührt die Editor-API.

- `src/test/grammar.test.ts` — tokenisiert tatsächlich mit der Grammatik, mit derselben Engine wie
  VS Code (`vscode-textmate` + `vscode-oniguruma`), und prüft das Ergebnis.
  Oniguruma unterscheidet sich von regulären Ausdrücken in Emacs in Details (zum Beispiel behandelt
  es ein `]` am Anfang einer Zeichenklasse nicht als Literal), und solche Unterschiede findet man
  nur, wenn man die echte Engine laufen lässt.
- `src/test/indent.test.ts` — verlangt für jede `.typl`-Datei unter `examples/`, dass **das
  Entfernen aller Einrückung und ihr Wiederherstellen Byte für Byte mit dem eingecheckten Inhalt
  übereinstimmt**.
  Der Emacs-Modus erfüllt dasselbe Kriterium mit denselben Dateien, und das macht „die beiden
  Editoren stimmen überein“ zu einer geprüften Aussage.
  Zusätzlich ist `src/test/fixtures/emacs-indent-reference.txt` eine Referenzausgabe, die durch
  tatsächliches Ausführen von `indent-region` in einem `typelisp-mode`-Puffer von Emacs gewonnen
  wurde. Der erwartete Wert ist keine Wiederholung der TS-Implementierung, sondern **das, was der
  andere Editor tatsächlich erzeugt**, daher wird die Treue der Portierung direkt geprüft (darunter
  `let*` `do` `doiter` `labels` `impl` `pprint-logical-block`, Quote-Präfixe und mehr).
- `src/test/symbols.test.ts` — der Inhalt der Outline und die Erkennung von Typverweisen der
  Ausweichlösung. Die Zahl der Definitionen muss genau mit einer unabhängigen Zählung der
  Definitionsformen am Zeilenanfang übereinstimmen. Die Grenzregeln für Typverweise sind bewusst an
  die Ausweichlösung der Emacs-Version angeglichen (VS Code nutzt ein Lookbehind; Emacs drückt
  dieselbe Menge aus, indem es ein vorangehendes Zeichen verbraucht).
- Die auflösungsgetriebenen Tokens des Servers (`crates/typelisp-front/src/check/semantic.rs`)
  prüfen `cargo test --test lsp_semantic_test` und `scripts/lsp-semantic-smoke.py` (das einen
  echten Prozess über stdio steuert). Den Emacs-Client prüft `scripts/emacs-semantic-smoke.el` über
  eine echte eglot-Verbindung.
- `src/test/manifest.test.ts` — `package.json` ist der einzige Teil, den der Compiler nicht prüft,
  daher prüft dieser Test, dass die deklarierten Befehle und die `registerCommand`-Aufrufe dieselbe
  Menge bilden, worauf sich die Tastenbelegungen beziehen, dass die vom Code gelesenen
  Einstellungen deklariert sind und dass der Problem Matcher auswerten kann, was `typl` tatsächlich
  ausgibt.

### Erkennen von Abweichungen in den Editordefinitionen

Die Schlüsselworttabellen werden doppelt gepflegt, in der Emacs-Version und in der
VS-Code-Version. Damit die Editordefinitionen nicht zurückbleiben, während die Implementierung
voranschreitet, gibt es auf der Rust-Seite einen Test:

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

## Hinweise

- typelisp wandelt Symbole beim Lesen in Kleinbuchstaben um, die Hervorhebung unterscheidet aber
  Groß- und Kleinschreibung, damit Typnamen mit großem Anfangsbuchstaben unterscheidbar bleiben.
- Die Einrückung bestimmt `INDENT_SPECS` in `src/indent.ts`. Es ist eine Portierung von
  `typelisp-indent-specs` der Emacs-Version, mit denselben Werten und Regeln. Stellen, an denen
  eine Form in ihrer Gestalt von der gleichnamigen Form in Emacs Lisp abweicht, sind unverändert
  übernommen: der Kopf von `(defun NAME (PARAMS) RETTYPE ...)` hat drei Elemente, `if` hat fest
  drei Elemente mit einem zwingenden `else` usw.
- Der Inhalt von `#| ... |#` wird beim Formatieren neu eingerückt. Das entspricht dem Verhalten von
  `indent-region` in Emacs.
