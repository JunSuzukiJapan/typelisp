<!-- translated-from: docs/ja/tutorial/types.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Grundlagen der Typen

typelisp ist eine statisch typisierte Sprache. Dieses Kapitel erklärt, was die Typprüfung leistet, welche
Typen man am häufigsten verwendet (`Option`, `Result`, Strukturen und Aufzählungen) und wie Generics
funktionieren. Es setzt voraus, dass [Erste Schritte](intro.md) gelesen wurde.

## 1. Was statische Typisierung bedeutet

In typelisp steht der Typ jedes Ausdrucks fest, bevor das Programm läuft. Ein Ausdruck, dessen Typen nicht
passen, ist ein Fehler, bevor überhaupt etwas ausgeführt wird.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; Typfehler

(main)
```

Beim Ausführen dieser Datei bricht das Programm mit einem Typfehler ab, ohne auch nur `start` auszugeben.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Typen müssen für die Argumente und Rückgabewerte von Funktionen, für globale Variablen und für die Felder von
Strukturen geschrieben werden. Der Typ einer `let`-Variablen ergibt sich aus ihrem Anfangswert.

Die wichtigsten Typen:

| Typ | Beispielwerte |
|---|---|
| `int` | `42`, `-7` (Ganzzahlen beliebiger Genauigkeit) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Ganzzahlen fester Breite |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Der Rückgabetyp einer Funktion, die keinen Wert zurückgibt |

Es gibt keine Möglichkeit, zur Laufzeit nach dem Typ eines Wertes zu fragen (kein `typep` oder `type-of` wie
in Common Lisp), denn jeder Typ ist bereits vor der Ausführung bekannt.

## 2. `Option<T>`: ein Wert, der fehlen kann

typelisp hat kein `nil`. „Es gibt vielleicht keinen Wert“ wird mit dem Typ `Option<T>` ausgedrückt. Ein Wert
von `Option<T>` ist entweder `some`, das einen Wert von `T` enthält, oder `none`, das nichts enthält.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` ist nicht `int` und lässt sich daher nicht direkt in Rechnungen verwenden.
`(+ (safe-div 10 2) 1)` ist ein Typfehler. Um den Inhalt zu verwenden, trennt man `some` und `none` mit
`match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- Im Zweig `(some q)` wird der Inhalt an die Variable `q` gebunden.
- `match` prüft, ob seine Zweige **alle Fälle abdecken**. Wer den Zweig `(none)` vergisst, bekommt einen
  Typfehler.

### Warum es kein nil gibt

In vielen Sprachen kann `nil` (`null`) für einen Wert beliebigen Typs stehen. Die Folge: Wer vergisst, den
Fall „kein Wert“ zu behandeln, merkt das erst zur Laufzeit. In typelisp hat eine Stelle, an der ein Wert
fehlen kann, den Typ `Option<T>`, und der Code besteht die Typprüfung nur, wenn `match` den Fall `none`
behandelt. Ein vergessener Fall wird vor der Ausführung entdeckt.

Bedingungen folgen derselben Idee: Nur ein `bool` kann die Bedingung von `if` sein. Eine Regel wie in Common
Lisp, nach der „alles außer `nil` wahr ist“, gibt es nicht.

### Häufige Operationen

| Form | Bedeutung |
|---|---|
| `(unwrap-or opt default)` | Der Inhalt bei `some`, der Standardwert bei `none` |
| `(unwrap opt)` | Nimmt den Inhalt heraus. Hält das Programm bei `none` an |
| `(is-some opt)` / `(is-none opt)` | Prüft, welcher Fall vorliegt |

Viele Funktionen der Standardbibliothek geben `Option` zurück. `position` etwa gibt die Position in `some`
zurück, wenn das Element gefunden wird, und `none`, wenn nicht.

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: eine Operation, die fehlschlagen kann

Eine Operation, die fehlschlagen kann, gibt `Result<T,E>` zurück: `ok` mit einem Wert von `T` bei Erfolg,
oder `err` mit einem Fehler `E` bei Misserfolg.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Auch eigene Funktionen können `Result` zurückgeben.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

`Option` verwendet man, wenn das Fehlen eines Wertes keiner Erklärung bedarf, `Result`, wenn man sagen will,
warum etwas fehlgeschlagen ist. [Fehlerbehandlung](errors.md) geht ausführlich auf den Umgang mit Fehlern
ein.

## 4. `defstruct`: Strukturen

Ein Typ mit benannten Feldern wird mit `defstruct` definiert.

```lisp
(defstruct point
  (x int)
  (y int))
```

Die Definition stellt Folgendes bereit:

```lisp
(let ((p (point::new 3 4)))     ; eine erzeugen (Argumente in Feldreihenfolge)
  (println "~a" p::x)           ; ein Feld lesen; (x p) geht auch
  (setf p::x 10)                ; es ändern
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Um einer Struktur eigene Funktionen zu geben, verwendet man `defmethod`. Der Typ des ersten Arguments
(`self`) bestimmt, zu welchem Typ die Methode gehört.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Steht statt eines `self`-Arguments nur der Typname, entsteht eine Funktion, die als `point::origin`
aufgerufen wird.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: eine von mehreren Formen

Ein Wert, der eine von mehreren Formen ist, etwa „ein Kreis, ein Rechteck oder ein Punkt“, wird mit
`defenum` definiert. Jede Form heißt **Variante**. Jede Variante kann eine andere Anzahl und andere Typen von
Werten enthalten.

```lisp
(defenum shape
  (circle int)        ; Radius
  (rect int int)      ; Breite und Höhe
  (dot))              ; enthält keinen Wert
```

Werte werden mit dem Typnamen davor erzeugt, wie in `shape::circle`. In `match` werden sie über den
Variantennamen zerlegt.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Auch hier prüft `match`, ob alle Fälle abgedeckt sind. Kommt später eine Variante zu `shape` hinzu, wird
jedes `match`, das sie nicht behandelt, zum Typfehler, sodass keine Stelle, die angepasst werden muss,
übersehen wird.

Nach `(use shape)` kann man `(rect 5 6)` ohne den Typnamen schreiben.

`Option` und `Result` sind Aufzählungen, die mit genau diesem Mechanismus gebaut sind.

## 6. Generics

Eine Funktion, die für jeden Typ funktioniert, wird mit einem **Typparameter** `<T>` nach ihrem Namen
definiert.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Beim Aufruf gibt man den Typ nicht an. `T` ergibt sich aus den Argumenten.

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T ist int
(first-or names "none")    ; T ist string
(first-or ints "none")     ; Typfehler: ints ist ein Vector<int>, also ist T int
```

Auch Strukturen und Aufzählungen können generisch sein. `Vector<T>`, `Option<T>` und `Result<T,E>` sind Typen
dieser Art.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

Innerhalb einer generischen Funktion weiß man nichts über `T`, daher kann man Werte von `T` weder vergleichen
noch addieren. Um etwas wie „jeder Typ, der sich vergleichen lässt“ zu verlangen, verwendet man Traits
([Traits](traits.md)).

## 7. Einem Typ einen anderen Namen geben

`deftype` gibt einem Typ einen anderen Namen.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` ist nur eine andere Schreibweise für `int`, kein neuer Typ. Ein gewöhnliches `int` dort zu übergeben,
wo `meters` erwartet wird, ist kein Fehler. Wer die beiden getrennt halten will, legt eine Struktur an, wie
`(defstruct meters (value int))`.

## 8. Wie es weitergeht

- [Traits](traits.md): Typen gemeinsame Operationen geben
- [Typen](../reference/types.md): die eingebauten Typen und die Traits, die jeder von ihnen implementiert
