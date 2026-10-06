<!-- translated-from: docs/ja/tutorial/macros.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Makros

Ein Makro ist eine Funktion, die ein Programm entgegennimmt und ein Programm zurückgibt. Mit Makros lässt sich
neue Syntax schaffen, die Funktionen nicht ausdrücken können. Makros in typelisp funktionieren wie `defmacro`
in Common Lisp. Dieses Kapitel setzt voraus, dass „Listen (S-Ausdrücke)“ in [Erste Schritte](intro.md)
gelesen wurde.

## 1. Wie sich Makros von Funktionen unterscheiden

Eine Funktion erhält ihre Argumente, **nachdem sie ausgewertet wurden**. Ein Makro erhält sie **als
Ausdrücke, vor der Auswertung** (als S-Ausdrucksdaten), baut daraus einen anderen Ausdruck und gibt ihn
zurück. Der zurückgegebene Ausdruck ersetzt den Makroaufruf, und erst dann wird er typgeprüft und ausgeführt.
Diese Ersetzung heißt **Expansion**.

Eine Syntax wie `unless` lässt sich zum Beispiel nicht als Funktion schreiben. Als Funktion würde der Rumpf
auch dann zuerst ausgewertet, wenn die Bedingung wahr ist.

## 2. `defmacro` und Quasiquote

Bauen wir `my-unless`, das seinen Rumpf nur ausführt, wenn die Bedingung falsch ist.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Für Makroargumente werden keine Typen geschrieben. Jedes Argument sind S-Ausdrucksdaten.
- `&rest body` nimmt die übrigen Argumente zusammen als eine Liste entgegen.
- Ein Ausdruck, der mit `` ` `` (Quasiquote) beginnt, wird so, wie er geschrieben ist, als Daten gebaut.
  Darin gilt:
  - `,test` setzt den Inhalt der Variablen `test` an dieser Stelle ein.
  - `,@body` fügt die Elemente der Liste `body` an dieser Stelle ein.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Die Expansion lässt sich mit `macroexpand-1` überprüfen. Beim Schreiben eines Makros kommt man am schnellsten
voran, wenn man sich zuerst die Expansion ansieht.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Auch Expansionen werden typgeprüft

Der Ausdruck, den ein Makro zurückgibt, wird wie jeder von Hand geschriebene Ausdruck typgeprüft.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

Der Fehler wird an der Stelle gemeldet, an der das Makro aufgerufen wurde.

Die Regeln, dass der else-Zweig von `if` nicht weggelassen werden kann und dass beide Zweige eines `if`
denselben Typ haben müssen, gelten für Expansionen unverändert. Das obige `my-unless` endet mit
`(progn ,@body ())`, damit beide Zweige des `if` den Typ `()` haben, gleich welchen Typ der letzte Ausdruck
des Rumpfes hat.

## 4. Namenskonflikte und `gensym`

Ein direkt geschriebenes Makro, das die Werte zweier Variablen vertauscht, sieht so aus:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Meistens funktioniert es, aber es versagt, wenn die Variable des Aufrufers zufällig `tmp` heißt.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (nicht vertauscht)
```

Die Expansion ist `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, und das vom Makro erzeugte `tmp`
verdeckt das `tmp` des Aufrufers.

Um das zu vermeiden, erzeugt man die Namen der innerhalb eines Makros verwendeten Variablen mit `gensym`.
`gensym` gibt ein neues Symbol zurück, das nirgends in einem Programm geschrieben werden kann.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Wie in Common Lisp verhindern typelisp-Makros Namenskonflikte nicht automatisch (sie sind nicht hygienisch).
Merken Sie sich: **Für die Bindungen, die ein Makro erzeugt, `gensym` verwenden.**

Ebenso lässt sich ein Makro, das seinen Rumpf eine gegebene Anzahl von Malen wiederholt, so schreiben:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Je nach Argumenten unterschiedlich expandieren

Ein Makrorumpf ist gewöhnlicher typelisp-Code und kann seine Argumente daher mit `if` oder `match`
untersuchen und eine andere Expansion bauen. Die Argumente sind S-Ausdrucksdaten (`Option<Sexpr>`), und die
leere Liste ist `none`.

Bauen wir `my-and`, das `true` zurückgibt, wenn alle seine Bedingungen wahr sind.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; keine Argumente
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; nur eines
         `(if ,f (my-and ,@more) false)))             ; zwei oder mehr
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` ist ein Muster, das den Kopf einer Liste in `f` und den Rest in `more` nimmt.
- `sexpr-null` prüft, ob S-Ausdrucksdaten die leere Liste sind.
- Der abschließende Zweig `_` ist nötig, weil S-Ausdrucksdaten neben Listen auch andere Formen haben (Zahlen,
  Zeichenketten usw.), und `match` verlangt, dass auch diese abgedeckt werden. Ein `&rest`-Argument ist immer
  eine Liste, daher wird dieser Zweig tatsächlich nie ausgeführt.
- Ein Makro kann sich in seiner Expansion selbst aufrufen. Die Expansion wiederholt sich, bis keine
  Makroaufrufe mehr übrig sind.

## 6. Optionale Argumente

`&optional` nimmt Argumente entgegen, die weggelassen werden können. Standardwerte lassen sich angeben.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` nimmt Schlüsselwortargumente entgegen
([Syntaxreferenz 3.14](../reference/syntax.md#314-defmacro--makrodefinitionen)).

## 7. `macrolet`: Makros für nur eine Stelle

Ein Makro, das nur innerhalb eines Ausdrucks verwendet wird, lässt sich mit `macrolet` definieren. Außerhalb
ist es nicht sichtbar.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Was man beachten sollte

- **Ein Makro kann erst nach seiner Definition aufgerufen werden.** Wie Funktionen definiert man es weit oben
  in der Datei.
- Mit `(pub defmacro ...)` steht ein Makro anderen Modulen zur Verfügung.
- Ein großer Teil der Standardsyntax, darunter `when`, `unless`, `cond`, `and`, `or` und `dotimes`, ist als
  Makro definiert. Mit `(macroexpand '(when true 1))` sieht man, was darin steckt.
- Was sich als Funktion schreiben lässt, schreibt man als Funktion. Makros lassen sich nicht als Werte
  übergeben, und um zu verstehen, was sie tun, muss man ihre Expansion lesen.

## 9. Wie es weitergeht

- [Fehlerbehandlung](errors.md): `Result`, `panic`, `catch` / `throw`
- [Makrobezogene Funktionen](../reference/functions/system.md#8-makros): `gensym`, `macroexpand` und mehr
