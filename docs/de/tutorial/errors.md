<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Fehlerbehandlung

Die Fehlerbehandlung in typelisp teilt Fehlschläge in zwei Arten ein.

| Art des Fehlschlags | Beispiele | Wie er ausgedrückt wird |
|---|---|---|
| Fehlschläge, die vorkommen können (behebbar) | Eine Datei fehlt, die Eingabe ist keine Zahl | Ein `Result<T,E>` zurückgeben |
| Fehler im Programm (nicht behebbar) | Ein Index außerhalb des Bereichs, `unwrap` von `none`, Division durch null | Mit `panic` anhalten |

Dazu kommen `catch` / `throw`, die viele Funktionsaufrufe auf einmal verlassen, und `unwind-protect`, das
eine Aufräumarbeit ausführt, wie auch immer sein Rumpf verlassen wird. Dieses Kapitel setzt voraus, dass der
Abschnitt über `Result` in [Grundlagen der Typen](types.md) gelesen wurde.

## 1. Ein `Result` zurückgeben und mit `match` entgegennehmen

Hier ist eine Funktion, die eine Portnummer aus einer Zeichenkette liest. Sie kann auf zwei Arten
fehlschlagen: Die Eingabe ist keine Zahl, oder sie liegt außerhalb des Bereichs.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Der Aufrufer trennt Erfolg und Fehlschlag mit `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- Der Wert einer Funktion, die `Result` zurückgibt, ist nur verwendbar, wenn `match` den Fall `err`
  behandelt. Wer vergisst, den Fehlschlag zu behandeln, bekommt einen Typfehler.
- Der Fehler von `parse-int` ist ein Wert vom Typ `ParseIntError`. `(message e)` liefert seine
  Meldungszeichenkette.

## 2. Einen Fehlschlag an den Aufrufer weiterreichen

Eine Kurzschreibweise wie Rusts `?` gibt es nicht. Ruft man nacheinander mehrere Funktionen auf, die `Result`
zurückgeben, schreibt man den Teil „den Fehlschlag unverändert zurückgeben“ mit `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Wenn feststeht, dass eine Operation nicht fehlschlagen kann, oder in einem kleinen Skript, in dem ein Abbruch
beim Fehlschlag in Ordnung ist, holt `unwrap` den Inhalt heraus. Ist der Wert ein `err`, kommt es zu einem
Panic. Genügt ein Standardwert, verwendet man `unwrap-or`.

## 3. Einen eigenen Fehlertyp erstellen

Fehler als Typ statt als Zeichenkette auszudrücken, erlaubt es dem Aufrufer, nach der Art des Fehlers zu
verzweigen. Ein Fehlertyp ist ein gewöhnliches `defenum` oder `defstruct`, das den Trait `Error`
implementiert.

```lisp
(defenum config-error
  (missing string)          ; eine Einstellung fehlt
  (invalid string int))     ; ein Wert ist falsch

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` gibt eine Beschreibung des Fehlers zurück.
- `source` gibt einen anderen Fehler zurück, der diesen verursacht hat. Ohne Ursache ist es `none`.

## 4. Verschiedene Arten von Fehlern zusammenführen

Ruft eine Funktion sowohl `parse-int` (`ParseIntError`) als auch `check-workers` (`config-error`) auf, gibt es
zwei Fehlertypen, und sie können nicht beide das `E` eines einzigen `Result<T,E>` sein. In diesem Fall macht
man `E` zu `:dyn Error` (einem Fehler beliebigen Typs, der `Error` implementiert). Jeder Fehler wird mit
`as-dyn-error` umgewandelt.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Für `"4"`, `"-1"` und `"abc"` ergeben sich:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Zu `:dyn` siehe Abschnitt 5 in [Traits](traits.md).

## 5. `panic`: Fehler im Programm

Erreicht das Programm einen Zustand, der nie eintreten dürfte, hält man es mit `panic` an.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Der Typ von `panic` ist `!` (es kehrt nicht zurück), daher kann es überall stehen, wo irgendein Typ erwartet
  wird. Deshalb passen die beiden Zweige des obigen `if` zusammen.
- Auch diese Operationen lösen einen Panic aus: `unwrap` von `none` oder `err`, `get` mit einem Index außerhalb
  des Bereichs und die ganzzahlige Division durch null.
- `panic` hält das Programm an. Auch wenn es innerhalb eines Tasks auftritt, hält das ganze Programm an.
- In der REPL beendet ein `panic` die REPL nicht; sie wartet auf die nächste Eingabe.
- Man kann `(todo)` für „noch nicht geschrieben“ und `(unreachable)` für „diese Stelle sollte nie erreicht
  werden“ schreiben. Beide lösen einen Panic aus.

`panic` ist kein Ersatz für `Result`. Für Fehlschläge, die vorkommen können, etwa bei Benutzereingaben oder
bei der Frage, ob eine Datei existiert, verwendet man `Result`.

## 6. `catch` / `throw`: über Funktionen hinweg springen

`throw` springt direkt zum umgebenden `catch` mit derselben Marke, gleich wie viele Funktionsaufrufe
dazwischen liegen.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

Enthält `v` keine negative Zahl, gibt `validate` `"all fine"` zurück; enthält es `-7`, springt die Steuerung
aus `check-all` heraus zum `catch`, das `"negative: -7"` zurückgibt.

- Die Marke schreibt man als einfaches Symbol, wie `'bad-input`.
- **Jede Marke transportiert Werte genau eines Typs.** Im obigen Beispiel transportiert `'bad-input` einen
  `string`, daher ist es ein Typfehler, mit derselben Marke ein `int` zu werfen. Auch der Typ des Rumpfes von
  `catch` muss zum Typ der Marke passen.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- Ein `throw`, das kein `catch` mit derselben Marke erreichen kann, ist ein Fehler.

Wer nur innerhalb einer Funktion vorzeitig zurückkehren will, verwendet `return-from` statt `catch` /
`throw`. `return-from` kann keine Funktionsgrenzen überschreiten, dafür sieht man beim Lesen des Quelltexts,
wohin es zurückkehrt.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: immer aufräumen

`(unwind-protect Rumpf Aufräumen)` führt die Aufräumarbeit aus, wie auch immer der Rumpf verlassen wird: bei
normalem Ende, beim Verlassen durch `throw` und bei einem Panic.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Man verwendet es etwa für „eine geöffnete Datei immer schließen“ oder „eine genommene Sperre immer
freigeben“. `with-open-file` und `with-lock` aus der Standardbibliothek verwenden intern `unwind-protect`.

## 8. Zum Bedingungssystem von Common Lisp

typelisp übernimmt das Bedingungssystem von Common Lisp (`handler-case`, `restart-case` usw.) nicht. Es zeigt
in den Typen nicht an, welche Fehlschläge eine Funktion verursachen kann, und passt damit schlecht zu
statischer Typisierung. Fehlschläge, die vorkommen können, werden mit `Result` in die Typen geschrieben, und
Kontrollübergaben erfolgen mit `catch` / `throw`.

## 9. Wie es weitergeht

- [Nebenläufigkeit](concurrency.md): Tasks und Kanäle
- [Option, Result und Fehlertypen](../reference/functions/option-result.md): die Liste der Funktionen
- [Fehlermeldungen](../reference/errors.md): was die häufigen Fehler bedeuten und wie man sie behebt
