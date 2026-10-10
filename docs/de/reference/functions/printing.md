<!-- translated-from: docs/ja/reference/functions/printing.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Ausgabe

`print`/`println`/`format`, die einstelligen Ausgabefunktionen, der Pretty Printer, `print-object` und die
Variablen, die die Ausgabe steuern. Die Liste der Formatdirektiven steht in [format.md](format.md). Lesen aus
und Schreiben in Streams steht unter [Streams und Dateien](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` sind allesamt **Spezialformen, die Formatdirektiven (die `format`-Direktiven von
CL) interpretieren**. Das erste Argument (bei `format` das zweite) ist die **Steuerzeichenkette**, und jede
Direktive verbraucht der Reihe nach die folgenden variadischen Argumente.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Expandiert die Steuerzeichenkette und schreibt sie ohne Zeilenumbruch auf die Standardausgabe |
| `println` | `(println control args...)` | `(string, ...)→Unit` | Dasselbe, mit einem Zeilenumbruch am Ende |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | CLs `format`. Gibt die expandierte Zeichenkette zurück. Ist `dest` `true` (CLs `t`), wird sie zusätzlich auf die Standardausgabe geschrieben; ist es `false` (CLs `nil`), wird sie nicht geschrieben, sondern nur zurückgegeben |
| `format` (in einen Stream) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | Ist `dest` kein `bool`, ist es CLs Stream-Ziel. Die expandierte Zeichenkette wird in diesen Stream geschrieben. Der Rückgabewert ist `()` (CLs `nil`), und es wird keine Zeichenkette zurückgegeben |

Der Typ von `dest` teilt die Bedeutung in zwei (welche gilt, wird statisch entschieden). Die Stream-Form lässt
sich gleichermaßen mit einem konkreten Stream-Typ, einem `:dyn CharOutput` oder einer durch
`(where (CharOutput S))` gebundenen Typvariable schreiben. Ein `dest`, das weder ein `bool` noch ein Stream
ist, ist ein Typfehler.

**Die Steuerzeichenkette muss ein Literal sein** (dieselbe Einschränkung wie bei Rusts `format!`). Die
Direktiven darin bestimmen, wie viele Argumente welcher Typen genommen werden, daher lässt sich eine zur
Laufzeit gebaute Zeichenkette bei der Prüfung nicht lesen. Weil sie ein Literal sein muss, **werden Anzahl und
Typen der Argumente bei der Prüfung kontrolliert**: `(println "~d" "x")` und `(println "~a ~a" 1)` sind Fehler
bei der Prüfung. Eine falsch geschriebene Direktive, ein nicht geschlossenes `~(` und ein `~/name/`, das kein
Argument beantworten kann, sind ebenfalls Fehler bei der Prüfung. Die Prüfregeln stehen in
[format.md](format.md#1-wie-man-direktiven-schreibt). Um eine selbst gebaute Zeichenkette auszugeben, erzeugt
man sie mit `(format false ...)` und gibt sie mit `(println "~a" s)` aus.

Die variadischen Argumente werden vor der Übergabe mit ihren eigenen Typen in `Sexpr` gehüllt:
`i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr` sowie benutzerdefinierte `defstruct`/`defenum`/
`Vector<T>`/`HashTable<K,V>` und Ähnliches lassen sich alle unverändert übergeben (`(println "~a" my-struct)`
funktioniert einfach).

Führt man ein Skript mit `typl file.typl` aus, **werden die Werte der Ausdrücke auf oberster Ebene nicht
ausgegeben**, daher schreibt ein Programm durch Aufrufe dieser Funktionen auf die Standardausgabe.
`print`/`println`/`format` schicken ihre Ausgabe bei jedem Aufruf hinaus (damit eine Eingabeaufforderung
sichtbar ist, bevor die Standardeingabe gelesen wird, auch über eine Pipe).

**`Option<Sexpr>` wird transparent ausgegeben.** Der Typ von S-Ausdrucksdaten ist `Option<Sexpr>`, daher
erscheint die Hülle `(some x)` nicht in der Ausgabe, und der Inhalt wird unverändert ausgegeben. Die leere Liste
wird als `()` ausgegeben. Andere `Option<T>` werden als `(some ...)` / `none` ausgegeben. Dasselbe gilt für
`Option<T>`-Felder innerhalb von Strukturen, Enums und `Vector`s. Ein `Result<Option<Sexpr>,…>` aus
`(eval ...)` wird als `(ok 42)` ausgegeben, oder bei `none` als `(ok ())`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; nur die Zeichenkette holen, ohne Ausgabe
  (println "~a" s))                   ; => id=42
```

## 2. Einstellige Ausgabefunktionen

Die Ausgabefunktionen aus CLHS 22.1.3. Statt ein Format zu expandieren, geben sie einen einzelnen Wert
unverändert aus. Der Stream kann weggelassen werden (Standard ist `*standard-output*`).

| Name | Form | Beschreibung |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Schreibt in einer Form, die sich zurücklesen lässt (dasselbe wie `~s`), und gibt `x` zurück |
| `princ` | `(princ x [stream])` | Schreibt in einer Form für Menschen (dasselbe wie `~a`) und gibt `x` zurück |
| `write` | `(write x [stream])` | `prin1`, wenn `*print-escape*` wahr ist, `princ`, wenn falsch. Gibt `x` zurück |
| `prin1-to-string` | `(prin1-to-string x)` | Gibt eine Zeichenkette zurück, statt zu schreiben (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | Dasselbe (`~a`). Dasselbe wie `to-string` |
| `write-to-string` | `(write-to-string x)` | Dasselbe, gemäß `*print-escape*` |

`print`/`println` gehören **nicht** dazu. Sie sind Kurzformen für `format`, die eine Steuerzeichenkette nehmen,
eine andere Aufgabe als CLs `print` (Zeilenumbruch, dann `prin1`, dann ein Leerzeichen), daher behält jede
ihren eigenen Namen. Folglich **hat CLs einstelliges `print` in dieser Sprache keine Schreibweise**: Man
schreibt `prin1`.

Diese sind Makros, weil die variadischen Argumente von `format` keine Typvariablen akzeptieren und der Typ an
der Aufrufstelle bekannt sein muss.

## 3. Standardeingabe und die Standard-Streams

**Das Lesen der Standardeingabe** geschieht nicht mit eigenen Funktionen, sondern mit den Methoden von
`CharInput` auf dem Standard-Stream `*standard-input*`: `(read-line *standard-input*)` /
`(read-char *standard-input*)` / `(read-all *standard-input*)` ([Stream-Methoden](streams-files.md#2-methoden)).
Standardausgabe und Standardfehlerausgabe haben entsprechend `*standard-output*` / `*error-output*` und lassen
sich wie in `(write-line *standard-output* s)` beschreiben (`print`/`println`/`format` sind Abkürzungen für den
Fall, dass man Formatexpansion braucht, und schreiben immer auf die Standardausgabe).

## 4. Der Pretty Printer

Dies entspricht CLs Lisp Pretty Printer (CLHS 22.2). **Er bricht Ausgabe, die nicht in die Zeilenbreite passt,
gemäß logischen Blöcken und bedingten Zeilenumbrüchen um.**

### 4.1 Steuervariablen

Zuweisbare globale Variablen. Einmal mit `setf` gesetzt, wirken sie auf jede spätere Ausgabe. Um eine
vorübergehend zu ändern, verwendet man `dlet` (6.3).

| Variable | Typ | Standard | Bedeutung |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Wenn wahr, nehmen `~a`/`~s`/`~w` und die Pretty-Direktiven den Weg der schönen Formatierung |
| `*print-right-margin*` | `int` | `80` | Der rechte Rand (in Spalten). 0 bedeutet „kein Rand, nie umbrechen“. Ein negativer Wert ist ein Ausgabefehler |
| `*print-miser-width*` | `int` | `0` | Die Breite, ab der der Miser-Stil beginnt. 0 entspricht CLs `nil` (Miser-Stil aus). Ein negativer Wert ist ein Ausgabefehler |

Die `pprint`-Familie und `pprint-logical-block` formatieren unabhängig von `*print-pretty*` immer schön (gemäß
der Definition von CLs `pprint`).

### 4.2 Fertige Layouts (Spezialformen)

Wie `print` sind dies Spezialformen, daher kann das Argument beliebigen Typs sein.

| Name | Form | Beschreibung |
|---|---|---|
| `pprint` | `(pprint x)` | Formatiert schön mit dem Standardlayout. Wie in CL **schreibt es zuerst einen Zeilenumbruch** und keinen am Ende |
| `pprint-fill` | `(pprint-fill x)` | Füllt jede Zeile mit so viel, wie hineinpasst. Schreibt keinen Zeilenumbruch |
| `pprint-linear` | `(pprint-linear x)` | Passen nicht alle Elemente auf eine Zeile, **ein Element pro Zeile**. Schreibt keinen Zeilenumbruch |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Eine Tabelle mit `colinc` Spalten breiten Spalten (Standard 16). Schreibt keinen Zeilenumbruch. Ein negatives `colinc` ist ein Fehler |

Das Standardlayout (`pprint` und `~a` unter `*print-pretty*`) folgt CLs Standard-`*print-pprint-dispatch*`: Es
kürzt `(quote x)` zu `'x` ab und formatiert Codeformen wie `defun`/`let`/`if`/`lambda` als „der Kopf und die
vorgeschriebene Anzahl Argumente auf der ersten Zeile, und der Rest des Rumpfes um zwei Spalten eingerückt,
eine Form pro Zeile“. Andere Listen werden gefüllt.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Logische Blöcke selbst bauen

| Name | Form | Beschreibung |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Eine Spezialform, die einen logischen Block öffnet. `obj` ist die Liste, die `pprint-pop` durchläuft (`()`, wenn keine durchlaufen wird). `:prefix` und `:per-line-prefix` schließen sich gegenseitig aus (wie in CL) |
| `pprint-newline` | `(pprint-newline kind)` | Ein bedingter Zeilenumbruch. `kind` ist `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Einrückung. `kind` ist `:block` (vom Blockanfang) / `:current` (von der aktuellen Spalte) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Ein Tabulator. `kind` ist `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` und `colinc` sind nicht negativ (negativ ist ein Fehler) |
| `pprint-pop` | `(pprint-pop)` | Nimmt das nächste Element aus der Liste des Blocks (`()`, wenn sie erschöpft ist) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Ob die Liste erschöpft ist |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Ist sie erschöpft, verlässt es mit `break` das umschließende `loop` (ein Makro) |

Logische Blöcke nehmen kein Stream-Argument: **Ein offener logischer Block ist impliziter Zustand**. Das
äußerste `pprint-logical-block` beginnt ihn, und wenn es sich schließt, wird das Ganze formatiert und auf einmal
auf die Standardausgabe geschrieben. Solange er offen ist, geht die Ausgabe von `print`/`println`/
`(format true ...)`/`pprint` vollständig in diesen Block, daher **schreibt man den Inhalt mit gewöhnlichem
`print` und markiert nur die Umbruchstellen mit `pprint-newline` und Verwandten**, wodurch der Code fast
genauso aussieht wie in CL.

In CL ist `pprint-exit-if-list-exhausted` ein nichtlokaler Ausgang aus `pprint-logical-block`; hier ist es
**ein `break` aus dem umschließenden `loop`** (`pprint-logical-block` richtet keinen `block` ein). CLs Idiom
setzt es ohnehin immer in ein `loop`, daher liest es sich gleich.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Regeln für bedingte Zeilenumbrüche (CLHS `pprint-newline`):

- `:mandatory` bricht immer um.
- `:linear` bricht um, wenn der umschließende logische Block nicht auf eine Zeile passt. Die Entscheidung fällt
  pro Block, daher **brechen alle `:linear`-Umbrüche eines Blocks gemeinsam um** (das ist das „alles auf einer
  Zeile oder ein Element pro Zeile“ von `pprint-linear`).
- `:fill` bricht um, wenn (a) der nächste Abschnitt nicht in den Rest der Zeile passt, (b) der vorige Abschnitt
  nicht auf eine Zeile passte oder (c) im Miser-Stil der Block nicht auf eine Zeile passt.
- `:miser` wirkt nur im Miser-Stil als `:linear` (wenn der Block innerhalb von `*print-miser-width*` vor dem
  rechten Rand beginnt).

## 5. `print-object` (typabhängige Druckdarstellung)

Schreibt man `impl print-object <Typ>`, geben `print`/`println`/`format`/`pprint` Werte dieses Typs mit dieser
Implementierung aus, **auch wenn sie in Listen verschachtelt sind**. Es entspricht CLs generischer Funktion
`print-object` (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argument | Bedeutung |
|---|---|
| `self` | Der auszugebende Wert |
| `escape` | CLs `*print-escape*`. `true` für `~s`/`prin1`/`pprint` (eine Form, die sich zurücklesen lässt), `false` für `~a`/`princ` (für Menschen). Eine Implementierung, der das egal ist, darf es ignorieren |

Die zurückgegebene `string` geht direkt in die Ausgabe. Typen ohne `impl` werden in der eingebauten Darstellung
ausgegeben (der Form `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   funktioniert auch verschachtelt
```

Es lässt sich auch mit dem Pretty Printer kombinieren (Kapitel 4). Ist `*print-pretty*` wahr, wird eine Liste,
die die von der Implementierung zurückgegebenen Zeichenketten enthält, am rechten Rand umgebrochen.

Die Druckdarstellungen der Typen der Standardbibliothek. Typen, die es auch in CL gibt, werden genauso
ausgegeben wie in SBCL. Wenn die REPL ein Ergebnis zeigt, verwendet sie dieselbe Darstellung wie `~s`.

| Typ | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`, `#("a" "b")` | `#(1 2 3)`, `#(a b)` |
| Tupel `#{..}` | `#{1 "a"}` | `#{1 a}` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Dasselbe |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (die Zahl ist eine interne laufende Nummer) | Dasselbe |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Eine Ganzzahl (der Wert von CLs `get-universal-time` / `get-internal-real-time`) | Dasselbe |
| Fehlertypen (`ParseIntError`, `SimpleError` usw.) | `#<simpleerror "boom">` | Nur die Meldung (`boom`) |
| `complex` | `#C(1.0 2.0)` | Dasselbe |
| `Array<T>` | `#2A((0 0) (0 0))` | Dasselbe |
| Streams | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Dasselbe |
| Sockets | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Dasselbe |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (während der Sommerzeit `dst` am Ende) | Dasselbe |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Dasselbe |
| `defstruct`-Typen | `#<point x: 1 y: 2>` (Feldnamen und Werte) | Dasselbe (Felder mit `~a`) |

Regeln:

- **Die Registrierung ist statisch.** Ein `impl` wird als gewöhnliche Methodendefinition typgeprüft, daher ist
  ein falsch geschriebener Typname oder eine falsche Signatur ein Kompilierfehler.
- **Es funktioniert auch für generische Typen.** `(impl print-object box<T> (where (print-object T)) ...)` geht
  für jedes Typargument an einen eigenen Rumpf: Ein Wert merkt sich seinen Typ einschließlich der Typargumente
  (`box<i32>`). Eingebaute generische Typen wie `Vector<T>` funktionieren genauso.
- **Die Wahl fällt beim Ausgeben.** Welche Direktive welches Argument verbraucht, hängt vom Laufzeitinhalt der
  Steuerzeichenkette ab, daher ist der Unterschied zwischen `~a` und `~s` (also `escape`) erst im Moment der
  Ausgabe bekannt. Das ist dasselbe wie bei CLOS, wo `print-object`-Methoden „pro Klasse definiert und beim
  Ausgeben gewählt“ werden.
- **Erneutes Betreten fällt auf die eingebaute Darstellung zurück.** Gibt eine Implementierung sich selbst mit
  `(format false "~a" self)` aus, würde sie endlos rekursieren, daher wird, wenn ein gerade ausgegebener Wert
  erneut erscheint, die eingebaute Darstellung verwendet. Das betrachtet die Identität des Wertes, keine
  Tiefengrenze, daher steht es der legitimen Ausgabe verschachtelter selbstbezüglicher Strukturen nicht im
  Weg.
- **Jeder skalare Typ implementiert diesen Trait.** Das dient dazu, **ihn als Schranke verwenden zu können**:
  Die variadischen Argumente von `format` können keine Typvariablen nehmen, daher ist diese Schranke die
  einzige Möglichkeit für generischen Code zu sagen „Werte eines unbekannten Typs dürfen dargestellt werden“
  (dieselbe Form wie Rusts `T: Display`). Das `print-object` von `Array<T>` ist ein Beispiel.
- **Bei Typargumenten, die die Schranke nicht erfüllen, wird stillschweigend die eingebaute Darstellung
  verwendet.** `(impl print-object Array<T> (where (print-object T)))` gilt für `Array<i32>`, aber nicht für
  ein `Array`, dessen Elemente ein `defstruct` ohne `print-object` sind. Es wäre unsinnig, wenn schon das
  Anlegen eines Arrays ein Fehler wäre, daher ist es kein Fehler.
- CLs anderer Mechanismus, `set-pprint-dispatch` / `*print-pprint-dispatch*` (ein Laufzeitregister mit
  Typspezifikatoren als Schlüssel), **wird nicht übernommen**. Seine Registrierungen sind ungeprüft, was nicht
  zu einer statisch typisierten Sprache passt.

## 6. Steuern, wie viel ausgegeben wird

### 6.1 Tiefe, Länge und Teilung

Die Steuervariablen aus CLHS 22.1.1, die bestimmen, „wie viel von einem Wert ausgegeben wird“. Wie die drei aus
4.1 sind sie zuweisbare globale Variablen und gelten für alle von `print`/`println`/`format`/`pprint`, ob
`*print-pretty*` wahr ist oder nicht.

| Variable | Typ | Standard | Bedeutung |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Objekte, die in dieser Tiefe oder tiefer verschachtelt sind, werden durch `#` ersetzt. Das ausgegebene Objekt hat die Tiefe 0. 0 bedeutet unbegrenzt |
| `*print-length*` | `int` | `0` | Gibt Listenelemente (und die Felder von `defstruct`/`defenum`-Werten) bis zu dieser Anzahl aus und ersetzt den Rest durch `...`. 0 bedeutet unbegrenzt |
| `*print-circle*` | `bool` | `false` | Wenn wahr, wird der Wert vor der Ausgabe durchsucht, und **Objekte, die zweimal oder öfter vorkommen, bekommen Marken**. Das erste Vorkommen ist `#n=…`, spätere `#n#` |

CL verwendet `nil` für „unbegrenzt“, aber diese Sprache hat kein `nil`, daher bedeutet wie bei
`*print-right-margin*` **0 unbegrenzt**. Negative Werte haben keine Bedeutung und sind Ausgabefehler. Die
Standardwerte sind alle „keine Grenze / keine Marken“, passend zu CLs Anfangswerten.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Zirkuläre Strukturen lassen sich nur ausgeben, wenn `*print-circle*` wahr ist.** Gibt man einen Wert, der auf
sich selbst zeigt, aus, während es falsch ist (Standard), folgt die Ausgabe dem Zyklus endlos, und der Prozess
stürzt ab. In CL ist es dasselbe (CLHS lässt die Ausgabe zirkulärer Strukturen undefiniert, wenn
`*print-circle*` falsch ist).

Einen Zyklus kann man nur bilden, indem man „ein `defstruct`-Feld mit `setf` auf sich selbst zeigen lässt“
(`Sexpr`-Zellen lassen sich nach der Erzeugung nicht ändern, daher kann eine Liste wie `'(1 2 3)` nie zirkulär
sein):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a zeigt auf a selbst
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Marken **beginnen bei jeder Ausgabe wieder bei 1** (wie in CL). Auch ohne Zyklus bekommt dasselbe Objekt, das
zweimal vorkommt, `#1=`/`#1#`, womit, wie CL es vorschreibt, die Information „diese beiden sind dasselbe
Objekt“ in der Ausgabe erhalten bleibt:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Ein Wert ohne Teilung **zeigt überhaupt keine Marken**, daher ändert es die Ausgabe alltäglichen Codes nicht,
wenn man diese Variable wahr lässt.

### 6.2 Basis, Schreibweise und Lesbarkeit

| Variable | Typ | Standard | Bedeutung |
|---|---|---|---|
| `*print-base*` | `int` | `10` | Die Basis für die Ausgabe von Ganzzahlen (feste Breite und `int`). Außerhalb von 2 bis 36 ist es ein **Ausgabefehler** (auch CL legt den Bereich fest) |
| `*print-radix*` | `bool` | `false` | Wenn wahr, wird eine Basismarkierung angefügt: `#b`/`#o`/`#x`, `#NNr` für andere Basen und ein nachgestellter `.` für Basis 10. Die Markierung steht **vor** dem Vorzeichen (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | Die Schreibweise von Symbolnamen: `:upcase` / `:downcase` / `:capitalize` (dieselben Schreibweisen wie in CL). Jedes andere Symbol ist ein Ausgabefehler |
| `*print-readably*` | `bool` | `false` | Wenn wahr, wird in einer Form ausgegeben, die sich zurücklesen lässt. Es erzwingt Maskierung und schaltet die Kürzungen von `*print-level*`/`*print-length*` ab |
| `*print-lines*` | `int` | `0` | Die Anzahl der Zeilen, die der Pretty Printer verwenden darf. Der Überschuss wird abgeschnitten, wie in CL mit `..` am Ende. 0 bedeutet unbegrenzt. Ein negativer Wert ist ein Ausgabefehler |
| `*print-escape*` | `bool` | `true` | Ob `write`/`write-to-string` `prin1` oder `princ` ausführen. **Nur diese beiden lesen sie** |
| `*print-array*` | `bool` | `true` | Ob `Vector<T>` und `Array<T>` ihren Inhalt zeigen. Wenn wahr, die Array-Syntax von CL (`#(1 2 3)` / `#2A((1 2) (3 4))`); wenn falsch, nur Typ und Form, `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

Die Markierungen, die `*print-radix*` anfügt, kann der Reader zurücklesen (die Basisschreibweise in der
[Syntaxreferenz](../syntax.md#1-lexikalische-elemente)).

**Warum der Standard von `*print-case*` von CL abweicht**: CLs Standard ist `:upcase`, weil CLs Reader
Symbolnamen in Großbuchstaben speichert, er bedeutet also „wie gespeichert“. Dieser Reader speichert sie in
Kleinbuchstaben, daher ist der Standard mit derselben Bedeutung `:downcase`.

**Die fehlende Hälfte von `*print-readably*`**: CL signalisiert `print-not-readable` für Werte, die sich nicht
zurücklesen lassen, aber diese Sprache hat keine Bedingung, die sie signalisieren könnte, und keine
Möglichkeit, über die Lesbarkeit von Benutzertypen zu entscheiden, die `print-object` beliebig ausgeben kann.
Vorhanden sind nur die erzwungene Maskierung und das Aufheben der Kürzungen.

**Warum nur `write` `*print-escape*` liest**: Wie CLHS vorschreibt, binden `~s`/`prin1`/`pprint` sie auf wahr
und `~a`/`princ` auf falsch, jeweils nur für die Dauer ihres eigenen Aufrufs. Die einzigen Leser, die sie
ungebunden sehen, sind daher `write`/`write-to-string`. Eine Implementierung von `print-object` sollte ihr
eigenes Argument `escape` lesen statt dieser globalen Variable: Dieses Argument trägt den Wert, den die
Direktive gewählt hat.

**Was CL hat und diese Sprache nicht**: `*print-gensym*` (es gibt keine nicht internierten Symbole).

### 6.3 Vorübergehendes Überschreiben

CL bindet diese mit `let`, aber `let` bindet in dieser Sprache lexikalisch, daher verwendet man `dlet`
([Sonstiges](system.md#10-sonstiges)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; die Grenzen gelten nur für diese eine Ausgabe
(with-standard-io-syntax (println "~a" x))   ; mit allem auf den Standardwerten ausgeben
```

`with-standard-io-syntax` führt seinen Rumpf aus, mit allen Steuervariablen der Ausgabe auf ihren
Standardwerten und `*read-eval*` auf `true`.
