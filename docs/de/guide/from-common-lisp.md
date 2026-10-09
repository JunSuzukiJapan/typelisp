<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Für Common-Lisp-Programmierer

typelisp übernimmt die Syntax von Common Lisp (CL) und viele seiner Funktionsnamen, ist aber eine statisch
typisierte Sprache. Daher funktioniert CL-Code nicht immer so, wie er geschrieben ist. Dieser Leitfaden sammelt
die Stellen, an denen CL-Gewohnte gern stolpern, zusammen damit, wie man den Code umschreibt.

## 1. Es gibt kein `nil` und kein `t`

Die Wahrheitswerte sind `true` und `false`. `nil` und `t` sind nicht definiert.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Nur ein `bool` kann eine Bedingung sein.** `0` oder eine leere Liste als Bedingung ist ein Typfehler. Die
  Regel „alles außer nil ist wahr“ gibt es nicht.
- **Der else-Zweig von `if` kann nicht weggelassen werden.** `(if c x)` ist ein Fehler. Wird kein else-Zweig
  gebraucht, verwendet man `when` / `unless`.
- **„Kein Wert“ wird mit `Option<T>` ausgedrückt.** Eine Funktion, die in CL nil für „nicht gefunden“
  zurückgab, gibt hier `(some x)` oder `none` zurück.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- Die leere Liste `()` ist je nach Kontext entweder der Wert des Unit-Typs (der Rückgabewert einer Funktion,
  die nichts zurückgibt) oder die leere Liste der S-Ausdrucksdaten. Sie ist ein anderer Wert als `false`.

## 2. Typen schreiben

Funktionsargumente und Rückgabewerte brauchen Typen.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; eine generische Funktion
  (unwrap-or (first (iter v)) default))
```

- Eine Definition ohne Typen wie `(defun f (x) x)` lässt sich nicht schreiben.
- Auch globale Variablen wie `defvar` brauchen einen Typ: `(defvar (count int) 0)`.
- `the` ist keine Prüfung zur Laufzeit, sondern eine Angabe für die Typprüfung.
- **Es gibt keine Möglichkeit, Typen zur Laufzeit zu untersuchen.** Es gibt kein `typep` und kein `type-of`,
  denn der Typ jedes Wertes steht zur Kompilierzeit fest. Um einen von mehreren Typen zu akzeptieren, legt man
  mit `defenum` einen Summentyp an oder verwendet einen Trait.
- `deftype` definiert einen Typalias. Ein Typ, der einen Wertebereich beschreibt, wie
  `(deftype small () '(integer 0 9))`, lässt sich nicht anlegen.

Der Standard-Ganzzahltyp `int` hat beliebige Genauigkeit; wie CLs integer hat seine Größe keine Obergrenze.
Auch die Typen fester Breite `i8` bis `i32` und `u8` bis `u32` gibt es. Einen Ganzzahltyp fester Breite mit
64 Bit gibt es nicht.

## 3. Funktionen als Werte

typelisp trennt die Namensräume von Funktionen und Variablen nicht. Der Name einer Funktion lässt sich
unverändert als Wert übergeben. Es gibt kein `#'` und kein `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; direkt aufrufen, nicht mit funcall

(apply-to twice 5)                        ; twice, nicht #'twice
```

- Auch eingebaute Funktionen wie `+` und `1+` lassen sich unverändert als Werte übergeben, wenn der Typ des
  Arguments feststeht, wie in `(fn (int) int)`. Übergibt man eine an eine generische Funktion wie `foldl` oder
  `map`, ist nicht bekannt, welches `+` welchen Typs gemeint ist, daher hüllt man sie in ein `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Sequenzfunktionen nehmen **zuerst die Sammlung und dann die Funktion**: `(map it f)`, `(filter it f)`,
  `(foldl it f init)`. Das ist umgekehrt zu CLs `(mapcar f list)`.
- `lambda` kann weder `&optional` noch `&key` verwenden (`&rest` schon).
- **Eine Funktion kann nicht vor ihrer Definition aufgerufen werden.** In CL kann man eine Funktion aufrufen,
  die erst später definiert wird, hier ergibt das `no such function`. Bei sich gegenseitig rekursiven
  Funktionen deklariert man eine davon zuerst mit `defsignature`.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Listen und Vector

Was einer CL-Liste entspricht, sind **S-Ausdrucksdaten**, deren Typ `Option<Sexpr>` ist (die leere Liste ist
`none`). `(list 1 2 3)` und `'(a b c)` haben diesen Typ. S-Ausdrucksdaten sind etwas, womit Makros und `read`
arbeiten; als gewöhnlichen Datenbehälter verwendet man **`Vector<T>`**.

| Was man will | CL | typelisp |
|---|---|---|
| Kopf und Rest eines S-Ausdrucks | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Eine S-Ausdrucksliste durchlaufen | `(dolist (x xs) ...)` | Dasselbe |
| Eine Folge von Elementen eines Typs | Eine Liste oder ein Vektor | `Vector<T>` |
| Ein Paar | `(cons a b)` | `(cons a b)` (sein Typ ist `cons-cell<A,B>`) |
| Abbildung | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` sind die Zugriffsfunktionen des mit `cons` erzeugten Paares `cons-cell<A,B>`. Auf
S-Ausdruckslisten lassen sie sich nicht anwenden.

Ein `Vector` lässt sich wie ein Vektor in CL mit `#(..)` schreiben und wird auch als `#(..)`
ausgegeben:

```lisp
(let ((v #(1 2)))
  (push v 3)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #(10 20 30)
```

Es gibt drei Unterschiede zu CL. Alle Elemente müssen denselben Typ haben (`#(1 "a")` ist ein
Typfehler). Jede Auswertung erzeugt einen neuen Vektor, sodass eine Änderung die nächste Auswertung
nicht beeinflusst (in CL ist das Ergebnis, ein Literal zu ändern, undefiniert). Ein leeres `#()`
braucht seinen Typ, etwa `(the Vector<int> #())`. Ein mehrdimensionales Array schreibt man wie in CL
als `#2A((1 2) (3 4))`.

Sequenzfunktionen wie `map`, `filter`, `sort` und `find` arbeiten auf Werten, die den Trait `Iter`
implementieren. Einen `Vector` übergibt man, nachdem man ihn mit `(iter v)` in einen Iterator verwandelt hat.

## 5. Es gibt keine Mehrfachwerte

Es gibt kein `values` und kein `multiple-value-bind`. Funktionen, die in CL mehrere Werte zurückgeben, geben
hier ein Paar oder eine Struktur zurück.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → eine `cons-cell`, deren `car` 3 und deren `cdr` 1 ist |
| `(decode-universal-time t)` → 9 Werte | Eine Struktur `decoded-time` |
| `(read-from-string s)` → Wert, Position | `(read-from-string s)` gibt eine `cons-cell` aus Wert und Position in einem `Result` zurück. Nur für den Wert `(read s)` |

## 6. Es gibt keine speziellen Variablen (dynamische Bindung)

`let` bindet immer lexikalisch. Bindet man eine mit `defvar` definierte Variable mit `let` neu, sehen die von
dort aufgerufenen Funktionen weiterhin den ursprünglichen Wert.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; in CL 2, in typelisp 1
```

Um eine Steuervariable wie `*print-base*` vorübergehend zu ändern, verwendet man `dlet`. Es weist den Wert zu
und stellt den ursprünglichen wieder her, wie auch immer der Rumpf verlassen wird.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` schreibt die globale Variable selbst um, ist also keine Bindung pro Thread.

## 7. Das Bedingungssystem wird nicht übernommen

Es gibt kein `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` oder `signal`. Sie
passen schlecht zu statischer Typisierung. Stattdessen werden diese beiden für unterschiedliche Zwecke
verwendet:

- **Behebbare Fehlschläge geben `Result<T,E>` zurück.** Der Aufrufer trennt `ok` / `err` mit `match`. Eine
  Kurzschreibweise wie Rusts `?` gibt es nicht.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Nicht behebbare Fehlschläge (Programmfehler) sind `panic`.** `(panic "message")`, `none` an `unwrap`
  übergeben, Division durch 0 und ein Index außerhalb des Bereichs gehören dazu, und das Programm hält an. Die
  Aufräumarbeit von `unwind-protect` läuft, bevor es anhält.

Fehlertypen werden durch den Trait `Error` vereinheitlicht, und `(message e)` liefert die Meldung. Wie man
einen eigenen Fehlertyp erstellt, steht in
[Option, Result und Fehlertypen](../reference/functions/option-result.md#3-fehlertypen-und-der-trait-error).
`assert` und `warn` lassen sich wie in CL verwenden.

`catch` / `throw` / `unwind-protect` gibt es. Die Marke von `catch` ist jedoch auf ein nicht ausgewertetes
literales Symbol (`'done`) beschränkt, und die mit einer Marke geworfenen Werte haben einen einzigen Typ.

## 8. Es gibt kein CLOS

Es gibt kein `defclass`, kein `defgeneric` und keine Methodenkombination.

- Datentypen werden mit `defstruct` (Strukturen) und `defenum` (Summentypen) definiert.
- `defmethod` definiert Methoden, deren Ziel allein durch **den statischen Typ des ersten Arguments** bestimmt
  wird. Mehrfachdispatch gibt es nicht.
- Um Typen gemeinsame Operationen zu geben, verwendet man Traits (`deftrait` / `impl`). Für Werte, deren
  konkreter Typ erst zur Laufzeit feststeht, verwendet man den Typ `:dyn Trait`
  ([Syntaxreferenz 3.9](../reference/syntax.md#39-deftrait--impl--traits)).

Wie sich `defstruct` unterscheidet:

- Der Konstruktor ist `Typname::new`: `(point::new 1 2)`. Wer einen Namen wie `make-point` will, bekommt ihn
  mit der Option `(:constructor make-point)`.
- Neben `(x p)` lässt sich eine Zugriffsfunktion als `p::x` schreiben. Geändert wird mit `(setf p::x 5)`.
- Es wird kein Prädikat (`point-p`) erzeugt. Es gibt kein `:conc-name`, `:type` oder `:named`.
- `:include` übernimmt nur die Slots; der Typ wird kein Untertyp des Elterntyps.

## 9. Module statt Pakete

Es gibt keine Pakete. Namensräume sind Module, und eine Datei ist für sich ein Modul. Statt `pkg:symbol`
schreibt man `module::name`, und Namen holt man mit `use` herein ([Module und Dateiaufteilung](modules.md)).

Schlüsselwörter `:foo` gibt es; es sind Symbole, die zu sich selbst auswerten. Da es keine Pakete gibt, ist der
Doppelpunkt Teil des Namens: `(symbol->string :foo)` gibt `":foo"` zurück.

## 10. Unterschiede beim Lesen und in der Syntax

- Groß- und Kleinschreibung werden nicht unterschieden (Symbole werden beim Lesen kleingeschrieben). Wie in CL.
- Es gibt kein `#'` (Abschnitt 3). Literale komplexer Zahlen `#c(...)` lassen sich nicht lesen; komplexe Zahlen
  erzeugt man mit `(complex 1.0 2.0)`.
- Die Klauseln des erweiterten `loop` werden mit Schlüsselwörtern geschrieben:
  `(loop :for i :from 1 :to 3 :collect i)`. Ein `loop`, das nicht mit einem Schlüsselwort beginnt, ist eine
  einfache Endlosschleife, die man mit `(break)` oder `(return Wert)` verlässt. `return` verlässt die innerste
  Schleife (um eine Funktion zu verlassen, verwendet man `return-from`).
- Das Ziel von `format` ist `false` (eine Zeichenkette zurückgeben), `true` (Standardausgabe) oder ein Stream.
  Die Formatdirektiven sind dieselben wie in CL.
- Aus einer Zeichenkette liest man mit `(read "...")`, aus einem Stream mit `(read-sexpr s)`. Beide geben ein
  `Result` zurück.
- `eval` typprüft den übergebenen Ausdruck vor dem Auswerten und gibt ein `Result` zurück.
  Vorwärtsverweise sind wie im Quelltext nicht möglich.
- Es gibt kein `eval-when`.
- Funktionsnamen verwenden keine Suffixe `?` oder `!`. Prädikate werden wie in CL mit `-p` / `p` benannt
  (`zerop`, `sexpr-null`) oder mit vorangestelltem `is-` (`is-some`).

## 11. Wichtige Funktionen mit anderen Namen

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (aus einem Stream) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Zwei-Argument-Fassungen von `floor` und Verwandten | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (Argumentreihenfolge umgekehrt; Abschnitt 4) |
| `length` (eines Vektors) | `len` |
| `hash-table-count` | `count` / `size` |

Die Liste der Funktionen steht unter [Eingebaute Funktionen](../reference/functions/README.md).

## 12. Was es sonst nicht gibt

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` und `copy-readtable`, `readtable-case` (Lesemakros selbst lassen sich mit
  `set-macro-character` definieren)
- Logische Pfadnamen und Pfadnamen mit Platzhaltern
- `input-stream-p` / `output-stream-p` (die Richtung eines Streams bestimmt sein Typ)
