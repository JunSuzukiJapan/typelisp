<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Traits

Ein Trait ist das Versprechen „dieser Typ unterstützt diese Operationen“. Mit Traits können mehrere Typen
gleichnamige Operationen teilen, sodass eine Funktion, die sie verwendet, nicht für jeden Typ eigens
geschrieben werden muss. Sie funktionieren fast genau wie die Traits in Rust. Dieses Kapitel setzt voraus,
dass [Grundlagen der Typen](types.md) gelesen wurde.

## 1. Einen Trait definieren und implementieren

Wir definieren die Operationen, die Fläche und Namen einer Figur zurückgeben, als Trait `Shape`.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- Das `()` nach dem Trait-Namen ist die Liste der Traits, von denen er erbt (Abschnitt 4). Ohne Vorfahren
  bleibt sie leer.
- Jede Zeile deklariert eine Methode. `Self` steht für „den Typ, der diesen Trait implementiert“.

Um einen Trait für einen Typ zu implementieren, schreibt man ein `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

Die implementierten Methoden werden wie gewöhnliche Funktionen aufgerufen.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Fehlt auch nur eine der Methoden, die der Trait deklariert, ist das ein Typfehler beim `impl`.

## 2. Trait-Schranken: „jeder Typ, der diesen Trait implementiert“

Mit `where` lässt sich eine Bedingung an den Typparameter einer generischen Funktion knüpfen. Das nennt man
eine **Trait-Schranke**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Dank `(where (Shape T))` kann der Rumpf `name` und `area` auf Werte von `T` anwenden. Ohne die Schranke wäre
über `T` nichts bekannt, und sie ließen sich nicht aufrufen.

Wer einen Typ übergibt, der `Shape` nicht implementiert, bekommt beim Aufruf einen Typfehler.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Eine generische Funktion erhält für jeden Typ, mit dem sie aufgerufen wird, eine eigene Kopie.
Typprüfungen oder Verzweigungen zur Laufzeit gibt es dabei nicht.

## 3. Standardimplementierungen

Hat eine Trait-Methode einen Rumpf, wird dieser verwendet, wenn ein `impl` die Methode weglässt.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe ist die Standardimplementierung

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; die hier geschriebene hat Vorrang

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Standard-Traits implementieren

Auch die Standardbibliothek hat Traits. Implementiert man einen davon, stehen die Standardfunktionen, die ihn
verwenden, für den eigenen Typ zur Verfügung.

| Trait | Zu implementierende Methoden | Was dadurch möglich wird |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, das Muster `(= expr)` von `match` usw. |
| `Ord` | `less` | `less-equal`, `greater` usw. `Ord` erbt von `Eq` |
| `print-object` | `print-object` | Wie `println` und Verwandte Werte anzeigen |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` usw. |
| `Error` | `message`, `source` | Verwendung als Fehlertyp ([Fehlerbehandlung](errors.md)) |

Implementieren wir `Eq` und `Ord` für einen Typ, der einen Geldbetrag darstellt. Da `Ord` von `Eq` erbt, muss
das `impl` von `Eq` zuerst kommen.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (die Standardimplementierung in Ord)
```

Mit `print-object` legt man fest, wie `println` den Wert anzeigt. Das Argument `escape` ist `true`, wenn
eine wieder einlesbare Form verlangt wird, wie bei `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Zusammen mit einer Trait-Schranke lässt sich eine Funktion schreiben, die für jeden Typ funktioniert, der
`Ord` implementiert.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Für einen `Vector` mit den `money`-Werten 300, 900 und 100 in dieser Reihenfolge liefert sie
`(some 900 yen)`.

## 5. `:dyn`: Werte verschiedener Typen gemeinsam behandeln

Alle Elemente eines `Vector<T>` haben denselben Typ, daher passen `circle`- und `rect`-Werte nicht in einen
gemeinsamen `Vector<circle>`. Um „etwas, das `Shape` implementiert“ gemeinsam zu behandeln, verwendet man den
Typ `:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Ein `circle`- oder `rect`-Wert, der dort steht, wo ein `:dyn Shape` erwartet wird, wird automatisch
  umgewandelt.
- Welches `area` der Aufruf `(area s)` ausführt, entscheidet sich zur Laufzeit am Typ dessen, was `s`
  enthält.
- Einen Wert, dessen Typ `Shape` nicht implementiert, dort zu platzieren, wo ein `:dyn Shape` erwartet wird,
  ist ein Typfehler.

Wie man zwischen den Trait-Schranken aus Abschnitt 2 und `:dyn` wählt:

| | Trait-Schranke (`where`) | `:dyn Trait` |
|---|---|---|
| Wann die aufgerufene Methode feststeht | Vor der Ausführung | Zur Laufzeit |
| Typen in einem `Vector` mischen | Nicht möglich | Möglich |
| Verwendbare Typen | Keine Einschränkung | Strukturen, Aufzählungen, `int`, `string`, `f64` und andere (nicht `bool`, `char`, `symbol`, `i32` und ähnliche) |

Die genaue Liste der verwendbaren Typen steht in
[Syntaxreferenz 3.9](../reference/syntax.md#39-deftrait--impl--traits).

Manche Traits lassen sich nicht mit `:dyn` verwenden: solche, deren Methoden `Self` für ein anderes Argument
als `self` oder für den Rückgabewert verwenden (wie `equals` in `Eq`). Da der Typ erst zur Laufzeit feststeht,
gibt es keine Möglichkeit, „einen Wert desselben Typs“ zu erzeugen.

## 6. Einschränkungen

- Die Definition eines Traits, seine `impl`s und der Code, der ihn über `:dyn` verwendet, gehören in ein
  Modul (eine Datei). Einen Trait für andere Module sichtbar zu machen, ist noch nicht möglich.
- Typen und Traits teilen sich einen Namensraum. Innerhalb eines Moduls können ein Typ und ein Trait nicht
  denselben Namen haben.

## 7. Wie es weitergeht

- [Makros](macros.md): eigene Syntax definieren
- [Syntaxreferenz 3.9](../reference/syntax.md#39-deftrait--impl--traits): Pauschalimplementierungen
  (blanket), assoziierte Typen und mehr
- [Standard-Traits](../reference/functions/traits.md): die Liste der Traits in der Standardbibliothek
