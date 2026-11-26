<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# Die REPL

Wird `typl` ohne Argumente gestartet, erhält man die REPL (den interaktiven Modus). Dieses Dokument
beschreibt, was die REPL über das Auswerten von Ausdrücken hinaus kann: ihre Befehle, die Hilfe
beim Tippen, den Zugriff auf vorherige Ergebnisse, das Abbrechen einer Berechnung und die
Untersuchung nach einem Fehler.

## Starten und Beenden

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Beendet wird mit `:quit` (kurz `:q`) oder `Ctrl+D`. Die Eingabehistorie wird in `~/.typl_history`
gespeichert; beim nächsten Start lässt sie sich mit `Ctrl+P` / `Ctrl+N` oder `Ctrl+R`
(Suche in der Historie) wieder abrufen.

Beim Start wird vor der ersten Eingabeaufforderung Folgendes der Reihe nach ausgeführt. Keiner
dieser Schritte gibt die erzeugten Werte aus.

1. Der Inhalt von `~/.typlrc`, falls vorhanden. Mit `--no-init` wird die Datei nicht gelesen.
2. `--eval FORM` und `--load FILE` von der Kommandozeile, in der angegebenen Reihenfolge. Beide
   dürfen beliebig oft angegeben werden.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` und `--no-init` gelten nur für den Start der REPL. Beim Ausführen einer Datei
sind sie ein Fehler.

Die REPL beginnt im Modul `user`. Wechselt man mit `(in-module foo)` in ein anderes Modul, zeigt
die Eingabeaufforderung dieses Modul an.

```
typl> (in-module foo)
typl user::foo>
```

## REPL-Befehle

Ein `:name` am Anfang einer Zeile ist ein REPL-Befehl. `:help` listet sie auf. Ein `:name`, das
nicht in der Liste steht, ist kein Befehl, sondern wird als gewöhnliches Schlüsselwort ausgewertet.

| Befehl | Bedeutung |
|---|---|
| `:help` (`:h`, `:?`) | die Befehle auflisten |
| `:type FORM` (`:t`) | den Typ von FORM ausgeben, ohne FORM auszuwerten |
| `:describe NAME` (`:d`) | Art der Definition, Typ, Felder oder Varianten, Methoden, implementierte Traits, Ort der Definition und Dokumentationsstring ausgeben |
| `:apropos TEXT` | in allen Modulen die Definitionen suchen, deren Name TEXT enthält (ohne Beachtung der Groß- und Kleinschreibung) |
| `:src NAME` | den Quelltext einer Definition ausgeben |
| `:who-calls NAME` | die Funktionen und Methoden ausgeben, die die Funktion oder Methode NAME aufrufen |
| `:inspect FORM` (`:i`) | FORM auswerten und den Wert entsprechend seinem Typ öffnen |
| `:backtrace` (`:bt`) | die Aufrufe, die beim Fehlschlagen der vorigen Zeile liefen, von innen nach außen ausgeben |
| `:locals [N]` | die Argumente des Aufrufs N von `:backtrace` ausgeben (ohne N: Aufruf 0) |
| `:cd [DIR]` | in den Ordner DIR wechseln (ohne DIR: in den Home-Ordner) |
| `:pwd` | den aktuellen Ordner ausgeben |
| `:quit` (`:exit`, `:q`) | die REPL beenden |

NAME kann ein Name wie `inner` sein oder ein Pfad wie `std::seq::reverse` oder `Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Addiert 1 zu x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Addiert 1 zu x.
typl> :src inner
(defun inner ((x int) (s string)) int "Addiert 1 zu x." (+ x 1))
```

`:src` zeigt eine aus einer Datei geladene Definition aus dieser Datei und eine in der REPL
eingegebene Definition aus dem eingegebenen Text. Für Definitionen der Standardbibliothek ist
kein Ort im Quelltext gespeichert, daher kann ihr Quelltext nicht angezeigt werden.

`:who-calls` durchsucht die Rümpfe der Funktionen der aktuellen Sitzung. Bei einer generischen
Funktion werden nur die Verwendungen gefunden, die mit konkreten Typen instanziiert wurden.

`:cd` ändert den Ordner, in dem `(load ...)` sucht; `use` sucht Dateien aber weiterhin in dem
Ordner, in dem die REPL gestartet wurde.

## Einen Wert öffnen (`:inspect`)

`:inspect` öffnet einen Wert entsprechend seinem Typ. Ein Struct öffnet sich in seine Felder, ein
Enum (auch `Option`) in die Felder seiner Variante, ein `Vector` in seine Elemente und eine
`HashTable` in ihre Schlüssel und Werte. Eine Zahl führt in diesen Teil hinein, `u` geht eine
Ebene zurück und `q` beendet.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

Listen und `Sexpr` werden nicht geöffnet, sondern so ausgegeben, wie sie sind.

## Hilfe beim Tippen

Bei der Eingabe an einem Terminal steht Folgendes zur Verfügung.

- `Tab` vervollständigt Namen (Definitionen und Syntax wie `if` und `defun`), Pfade mit `::`,
  REPL-Befehle am Zeilenanfang und innerhalb eines Strings Dateinamen.
- Nach dem Namen einer Funktion und einem Leerzeichen wird der Typ der Funktion abgeblendet am
  Zeilenende angezeigt.
- Die Klammer, die zu der Klammer neben dem Cursor passt, wird hervorgehoben.
- Drückt man Enter, bevor eine Form abgeschlossen ist, beginnt die nächste Zeile um die Zahl der
  offenen Klammern eingerückt.

## Vorherige Ergebnisse (`:*`, `:**`, `:***`)

In der REPL ist `:*` der zuletzt ausgegebene Wert, `:**` der davor und `:***` der davor. Jeder hat
den Typ seines Werts.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- Eine Eingabe, die keinen Wert ausgegeben hat (etwa eine Definition), und eine fehlgeschlagene
  Eingabe rücken die Ergebnisse nicht weiter.
- `:**` oder `:***` zu schreiben, bevor es so viele Ergebnisse gibt, ist ein Fehler.
- Ein quotiertes `':*` ist ein gewöhnliches Schlüsselwort.
- Ein im Rumpf einer Funktion geschriebenes `:*` verweist weiterhin auf das Ergebnis vom Zeitpunkt
  der Definition. Das `*` von Common Lisp liest den Wert zum Zeitpunkt des Aufrufs; typelisp legt
  ihn bei der Definition fest, damit er einen Typ hat.
- Außerhalb der REPL (beim Ausführen einer Datei sowie in `~/.typlrc`, `--load` und `--eval`) ist
  `:*` ein gewöhnliches Schlüsselwort.

## Eine Berechnung abbrechen (`Ctrl+C`)

Drückt man während einer Auswertung `Ctrl+C`, wird die Berechnung abgebrochen und man kehrt zur
Eingabeaufforderung zurück. Was vorher definiert wurde, bleibt erhalten. Das funktioniert bei einer
Schleife im Interpreter, einer Schleife in kompiliertem Code und beim Warten in `sleep` oder
`wait`. Die Aufräumarbeiten laufender `unwind-protect` werden ausgeführt.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` während der Eingabe verwirft nur die gerade eingegebene Zeile.

## Untersuchung nach einem Fehler (`:backtrace`, `:locals`)

Nachdem eine Auswertung mit einem Fehler stehengeblieben ist, gibt `:backtrace` (`:bt`) die
Funktionsaufrufe, die zu diesem Zeitpunkt liefen, von innen nach außen aus. `:locals N` gibt die
Argumente des Aufrufs N entsprechend ihren deklarierten Typen aus.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- Bei einem Aufruf aus dem letzten Ausdruck einer Funktion (einem Endaufruf) wird die aufrufende
  Funktion nicht angezeigt. Deshalb braucht eine endrekursive Schleife, egal wie oft sie läuft, nur
  den Platz eines einzigen Aufrufs.
- Aufrufe innerhalb kompilierter Funktionen und anonyme Funktionen (`lambda`) werden nicht
  angezeigt.
- `:locals` zeigt nur die Argumente, nicht die mit `let` gebundenen Variablen.
- Wird die nächste Zeile ausgewertet, wird der Eintrag zum vorigen Fehler verworfen.
