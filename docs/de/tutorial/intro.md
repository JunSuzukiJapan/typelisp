<!-- translated-from: docs/ja/tutorial/intro.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Erste Schritte

Ausgehend vom Auswerten von Ausdrücken in der REPL behandelt dieses Kapitel der Reihe nach Funktionen,
Variablen, Verzweigungen, Schleifen sowie Listen und `Vector`. Wie man `typl` baut, steht in der
[README.md](../../../README.md).

## 1. Die REPL starten

Ohne Argumente gestartet, geht `typl` in die REPL (den interaktiven Modus). Ein Ausdruck, der nach `typl>`
eingegeben wird, wird sofort ausgewertet und sein Wert ausgegeben. `:quit` verlässt die REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Von hier an werden Eingaben und Ergebnisse der REPL in dieser Form gezeigt.

## 2. Ausdrücke auswerten

typelisp ist ein Lisp, also steht ein Ausdruck in Klammern, mit **dem Operator oder Funktionsnamen
zuerst**. Man schreibt `(+ 1 2)`, nicht `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Zahlen gibt es in diesen Arten:

- **Ganzzahlen** haben den Typ `int`. Ihre Größe hat keine Obergrenze.
- **Dezimalzahlen** haben den Typ `f64`. Man schreibt sie mit Dezimalpunkt, wie `1.5` oder `2.0`.
- `int` und `f64` lassen sich in einer Rechnung nicht mischen. `(+ 1 2.0)` ist ein Typfehler. Zum Umwandeln
  schreibt man `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` auf zwei Ganzzahlen ergibt eine Ganzzahl, bei der der Nachkommateil wegfällt (es entsteht kein Bruch wie
in Common Lisp). Für den Rest verwendet man `(mod 7 2)`.

Die Wahrheitswerte sind `true` und `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Funktionen definieren

Funktionen werden mit `defun` definiert. **Die Typen der Argumente und der Rückgabetyp werden immer
geschrieben.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` bedeutet „ein Argument `n` vom Typ `int`“. Mehrere Argumente werden aufgereiht:
  `((a int) (b int))`.
- Das `int` nach der Argumentliste ist der Rückgabetyp.
- Der Wert des letzten Ausdrucks im Rumpf ist der Rückgabewert der Funktion. `return` wird nicht geschrieben.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Ein Aufruf, dessen Typen nicht passen, wird **vor der Ausführung** als Typfehler gemeldet. Beim Ausführen
einer Datei genügt ein einziger Typfehler an irgendeiner Stelle, und keine einzige Zeile des Programms wird
ausgeführt.

Um ein Argument optional zu machen, verwendet man `&optional`. Mit einem Standardwert nimmt das Argument
diesen Wert an, wenn es weggelassen wird.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

Das `false` als erstes Argument von `format` bedeutet „das Ergebnis als Zeichenkette zurückgeben, statt es
auszugeben“. Jedes `~a` wird durch das nächste Argument ersetzt.

## 4. Variablen

Lokale Variablen werden mit `let` angelegt.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- Der Typ einer `let`-Variablen ergibt sich aus ihrem Anfangswert. Man muss ihn nicht schreiben.
- Die Variablen eines `let` können sich nicht gegenseitig verwenden. Um eine Variable aus der vorigen
  aufzubauen, verwendet man `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Um den Wert einer Variablen zu ändern, verwendet man `setf`. **Eine Zuweisung kann den Typ der Variablen
nicht ändern.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Globale Variablen werden mit `defvar` definiert. Hier schreibt man den Typ.

```lisp
(defvar (counter int) 0)
```

## 5. Verzweigungen

### if

Man schreibt `(if Bedingung Ausdruck-wenn-wahr Ausdruck-wenn-falsch)`. **Der Ausdruck für den falschen Fall
kann nicht weggelassen werden.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Nur ein Ausdruck vom Typ `bool` kann eine Bedingung sein. Eine Zahl, wie in `(if 0 ...)`, ist ein
  Typfehler.
- Die Ausdrücke für den wahren und den falschen Fall müssen denselben Typ haben.

Wenn im falschen Fall nichts geschehen soll, verwendet man `when` (und `unless` für das Gegenteil).

```lisp
(when (> n 100)
  (println "large")
  (println "really large"))
```

### cond

Ab drei Bedingungen liest sich `cond` besser. Das abschließende `else` wird genommen, wenn keine der
Bedingungen zutrifft.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Um nach der Form eines Wertes zu verzweigen, verwendet man `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` passt auf jeden Wert. Da `int` unzählige Werte hat, ist das Weglassen des `_`-Zweigs ein Fehler, der
besagt, dass nicht alle Fälle abgedeckt sind. Seine eigentliche Stärke zeigt `match` beim Zerlegen von
`Option` und selbst definierten Typen, die im nächsten Kapitel, [Grundlagen der Typen](types.md),
vorkommen.

## 6. Schleifen

Eine Funktion kann sich selbst aufrufen.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Für eine feste Anzahl von Wiederholungen verwendet man `dotimes`. `i` läuft von 0 bis `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

Es gibt außerdem `while`, `do` und das erweiterte `loop` aus Common Lisp. Die Klauselwörter des erweiterten
`loop` werden als Schlüsselwörter geschrieben (`:for`, `:collect` usw.).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. Listen und Vector

### Vector

Um eine Folge von Werten desselben Typs zu halten, verwendet man `Vector<T>`. `T` ist der Elementtyp.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; gibt #<vector<int> 3 1 2> aus
```

- `(Vector::new)` allein legt den Elementtyp nicht fest, deshalb gibt man den Typ mit
  `(the Vector<int> ...)` an.
- `(push v x)` hängt ans Ende an, `(get v i)` liest Element `i`, und `(len v)` liefert die Länge.
- Ein `get` mit einem Index außerhalb des Bereichs hält das Programm mit einem Fehler an.

### lambda und Funktionen höherer Ordnung

Anonyme Funktionen werden mit `lambda` erzeugt. Wie bei `defun` schreibt man die Typen der Argumente und der
Rückgabe.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` und Verwandte nehmen einen `Vector`, der mit `(iter v)` in einen
**Iterator** verwandelt wurde. Zuerst kommt die Sammlung, dann die Funktion. Das `v` oben wurde mit
`let` gebunden und ist außerhalb dieses `let` nicht verfügbar. Das nächste Beispiel definiert `v`
deshalb zuerst mit `defvar`.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Um die Elemente einzeln zu verarbeiten, verwendet man `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Eine Funktion, die eine Funktion als Argument nimmt, schreibt den Typ dieses Arguments als
`(fn (Argumenttypen...) Rückgabetyp)`. Eine mit `defun` definierte Funktion lässt sich über ihren Namen als
Wert übergeben.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Listen (S-Ausdrücke)

Listen, die mit `'(1 2 3)` oder `(list 1 2 3)` erzeugt werden, sind **S-Ausdrucksdaten**. Ihre Elemente
müssen nicht denselben Typ haben.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

S-Ausdrucksdaten dienen hauptsächlich dazu, mit Programmen selbst umzugehen, in Makros ([Makros](macros.md))
und mit `read`. Für Daten, deren Elemente einen bekannten Typ haben, verwendet man `Vector<T>`. Eine
S-Ausdrucksliste lässt sich mit `dolist` durchlaufen.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Ein Paar aus zwei Werten wird mit `cons` erzeugt und mit `car` und `cdr` zerlegt.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Ein Programm in eine Datei schreiben

Ein Programm kann in eine Datei (mit der Endung `.typl`) geschrieben und mit `typl Dateiname` ausgeführt
werden. Um Ergebnisse anzuzeigen, verwendet man `println`.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` gibt mit denselben Direktiven wie `format` aus und endet mit einem Zeilenumbruch. `print` fügt
  keinen Zeilenumbruch an.
- `~a` setzt einen Wert in menschenlesbarer Form ein, `~s` in einer Form, die sich wieder einlesen lässt
  (Zeichenketten bekommen ihre `"`).
- Eine Datei wird von oben nach unten gelesen. **Eine Funktion kann nicht vor ihrer Definition aufgerufen
  werden.**

## 9. Wie es weitergeht

- [Grundlagen der Typen](types.md): `Option`, `Result`, Strukturen, Aufzählungen, Generics
- [Für Common-Lisp-Programmierer](../guide/from-common-lisp.md): eine Liste der Unterschiede für alle, die
  Common Lisp kennen
