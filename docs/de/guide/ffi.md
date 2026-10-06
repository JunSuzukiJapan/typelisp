<!-- translated-from: docs/ja/guide/ffi.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# C-FFI (defffi)

Dieser Leitfaden erklärt, wie man aus typelisp C-Funktionen aufruft. Die Liste der deklarierbaren Typen und
die Einschränkungen stehen in [Syntaxreferenz 3.3](../reference/syntax.md#33-defffi--c-funktionen-deklarieren-ffi).

## 1. Eine Funktion deklarieren und aufrufen

`defffi` deklariert Namen und Typen einer C-Funktion.

```lisp
(defffi (c-getpid "getpid") () i32)            ; der Name in typelisp und der Symbolname in C
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; in libm suchen
```

Aufrufe werden in `(unsafe ...)` gehüllt.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` ist nötig, weil der Compiler nicht prüfen kann, ob die deklarierten Typen zu den tatsächlichen Typen
auf der C-Seite passen. Wer `unsafe` schreibt, übernimmt als Autor die Verantwortung für diese Prüfung. Wer es
vergisst, bekommt einen Fehler, der das erklärt.

## 2. Eine sichere Hülle schreiben

Vorgesehen ist, `unsafe` auf eine Stelle zu beschränken und nach außen eine gewöhnliche Funktion anzubieten.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; der Aufrufer braucht kein unsafe
(str-len "hello")  ; => 5
```

## 3. Wie sich die Typen entsprechen

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Ganzzahlen derselben Breite |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (auch `size_t`, `int64_t` usw.) |
| `ptr` | Jeder Zeiger (`void *`, `FILE *` usw.) |
| `(ptr T)` | Ein Zeiger auf `T` ([Abschnitt 7](#7-c-structs)) |

### Zeichenketten

- Ein übergebener `string` wird in eine NUL-terminierte C-Zeichenkette kopiert, die nach der Rückkehr des
  Aufrufs freigegeben wird. Ein NUL mitten in der Zeichenkette ist ein Fehler.
- Auch das Ergebnis einer Funktion, die `string` zurückgibt, wird kopiert. Der Speicher auf der C-Seite wird
  nicht freigegeben. Bei Funktionen, die eine Zeichenkette zurückgeben, die der Aufrufer freigeben muss (wie
  `strdup`), nimmt man das Ergebnis als `ptr` entgegen und ruft `free` selbst auf.
- Gibt eine als `string`-zurückgebend deklarierte Funktion NULL zurück, ist das ein Fehler. Das Ergebnis von
  Funktionen, die NULL zurückgeben können (wie `getenv`), nimmt man als `ptr` entgegen.

### `c-long` / `c-ulong` / `ptr`

Diese Typen gibt es nur, um Werte über die Grenze zu C zu reichen, und sie **unterstützen keine Arithmetik**.
Um einen davon als typelisp-Ganzzahl zu verwenden, wandelt man ihn mit `as` um.

```lisp
(as int (unsafe (c-strlen s)))      ; int verliert nichts vom 64-Bit-Wert
(try-as i32 (unsafe (c-strlen s)))  ; none, wenn er nicht in ein i32 passt
(unsafe (c-malloc 16))              ; Ganzzahlliterale lassen sich direkt übergeben
```

Ein `ptr` ist ein Wert, der an C-Funktionen zurückgereicht wird. Von der typelisp-Seite aus gibt es keine
Möglichkeit zu lesen, worauf er zeigt.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Diese Typen dürfen nur als Funktionsargumente, Rückgabewerte und lokale Variablen vorkommen. Sie können keine
Strukturfelder, globalen Variablen oder Typargumente von `Vector` und ähnlichem sein.

## 4. Eine Bibliothek angeben

Ohne `:library` wird das Symbol in dem gesucht, was bereits in den Prozess gelinkt ist (libc usw.).
Funktionen aus anderen Bibliotheken brauchen `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Ein kurzer Name wie `"sqlite3"` wird als `libsqlite3.dylib` und dann als `libsqlite3.so` gesucht.
- Ein Name, der `/` enthält, wird als Pfad behandelt.
- Wird das deklarierte Symbol nicht gefunden, nennt der Fehler es.

## 5. AOT-Kompilierung

Programme, die `defffi` verwenden, lassen sich unverändert mit
[`compile-file`](compile.md#3-ein-programm-mit-aot-kompilierung-erstellen) zu ausführbaren Programmen machen.
Mit `:library` angegebene Bibliotheken werden beim Linken automatisch hinzugefügt, daher braucht
`compile-file` keine zusätzlichen Argumente.

## 6. Callbacks

Man kann einer C-Funktion eine typelisp-Funktion übergeben und sie zurückrufen lassen. Dazu schreibt man unter
den Argumenttypen von `defffi` einen Funktionstyp und setzt beim Aufruf an diese Stelle einen Funktionsnamen
oder einen `lambda`-Ausdruck.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") gibt p zurück
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Übergeben lassen sich nur Funktionen **ohne freie Variablen**. Funktionen auf oberster Ebene, `lambda`s und
  lokale `labels`-Funktionen funktionieren alle, aber der Bezug auf eine lokale Variable eines umgebenden
  Gültigkeitsbereichs ist ein Fehler bei der Typprüfung. C übergibt nur die deklarierten Argumente, daher gibt
  es keine Möglichkeit, eingefangene Variablen mitzugeben. Um Zustand zu halten, verwendet man globale
  Variablen.
- Eine Variable, die eine Funktion enthält, lässt sich nicht übergeben. An ihre Stelle schreibt man einen
  Funktionsnamen oder einen `lambda`-Ausdruck.
- Ein `panic` oder `throw` innerhalb des Callbacks erreicht den Aufrufer, nachdem die C-Funktion zurückgekehrt
  ist.
- Der Callback kann nur aufgerufen werden, solange die C-Funktion läuft, die typelisp aufgerufen hat. Aus
  Dingen wie `atexit` oder Signalhandlern lässt er sich nicht verwenden.

## 7. C-Structs

Um so etwas wie ein Array von Structs an eine C-Funktion zu übergeben, deklariert man mit `def-c-struct` ein
Struct mit demselben Layout wie in C und legt es innerhalb von `unsafe` an.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; innerhalb eines unsafe auf oberster Ebene deklariert

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; vier Elemente, alle mit null
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` legt `n` Werte von `T` an und gibt einen `(ptr T)` zurück. `(c-ref p i)` ist ein Zeiger auf
  das `i`-te, `p::field` ist ein Feld, und `(c-deref p)` ist das, worauf ein Zeiger auf einen Skalar wie `i32`
  zeigt. Alle lassen sich mit `setf` beschreiben.
- `(as ptr p)` macht daraus einen untypisierten `ptr`, um ihn an C-Funktionen zu übergeben, die ein `void *`
  nehmen.
- Die Größe von `item` (hier 8) und die Position jedes Feldes werden nach denselben Regeln wie in C bestimmt.

### Lebensdauer des angelegten Speichers

Angelegter Speicher wird freigegeben, wenn die Ausführung das äußerste `unsafe` in dieser Funktion verlässt.
Dasselbe geschieht beim Verlassen durch `panic` oder `throw`. Deshalb lässt sich ein `(ptr T)`-Wert nicht aus
dem `unsafe` herausnehmen. Ihn zum Wert des `unsafe` zu machen, ihn in einer Closure einzufangen, ihn an einen
`task` zu übergeben und ihn mit `throw` zu werfen, sind allesamt Typfehler. Werte, die man außerhalb verwenden
will, kopiert man innerhalb des `unsafe` in Zahlen oder ein `defstruct`.

Wer innerhalb eines `lambda` oder einer `labels`-Funktion Speicher anlegt, schreibt innerhalb dieser Funktion
ein `unsafe`.

### Von C angelegter Speicher

Ein von C als `(ptr T)` erhaltener Zeiger (ein Rückgabewert von `defffi`, ein Callback-Argument usw.) ist ein
Fehler, sofern er nicht in Speicher zeigt, der mit `c-alloc` angelegt wurde. Funktionen, die Speicher
entgegennehmen, den C mit `malloc` angelegt hat, oder NULL, deklariert man mit dem untypisierten `ptr`.

## 8. Was nicht geht

- **Variadische Funktionen** (`printf` und ähnliche) lassen sich nicht deklarieren. Der variadische Teil wird
  nach anderen Regeln übergeben als die festen Argumente. Für jede verwendete Argumentanzahl deklariert man
  einen eigenen Namen.
- **Structs per Wert übergeben oder zurückgeben** ist nicht möglich. Man verwendet Funktionen, die Zeiger
  übergeben.
- **Generische Deklarationen** sind nicht möglich.
- **Derselbe Name wie eine eingebaute Funktion** ist nicht verwendbar.
- **Sie lassen sich nicht als Funktionswerte übergeben.** Man kann eine nicht wie in `(map xs c-abs)`
  übergeben; man hüllt sie in ein `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
