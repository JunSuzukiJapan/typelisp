<!-- translated-from: docs/ja/reference/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Fehlermeldungen

Was die wichtigsten Fehlermeldungen von `typl` bedeuten und wie man sie behebt.

## 1. Einen Fehler lesen

Fehler werden in dieser Form auf die Standardfehlerausgabe geschrieben:

```text
error: Datei:Zeile:Spalte: Art: Meldung
```

Die `Art` sagt, wann der Fehler gefunden wurde.

| Art | Wann | Bedeutung |
|---|---|---|
| `type error` | Vor der Ausführung (bei der Prüfung) | Ein Fehler bei Typen oder Namen. Diese Form wird nicht ausgeführt |
| (keine Art) | Beim Lesen oder Prüfen | Ein Syntaxfehler wie unausgeglichene Klammern, oder ein Name, der nicht gefunden wird |
| `panic` | Während der Ausführung | Ein nicht behebbarer Fehlschlag. Das Programm hält an, nachdem die Aufräumarbeit von `unwind-protect` gelaufen ist |

Zeilen, die mit `warning:` beginnen, sind Warnungen, und die Verarbeitung läuft weiter.

`Datei:Zeile:Spalte` zeigt auf den fehlerhaften Ausdruck. Bei einem Laufzeitfehler innerhalb einer Funktion der
Standardbibliothek zeigt es auf die Stelle, an der das Programm diese Funktion aufgerufen hat. Manche Fehler
haben keine Position (etwa `error: panic: ...`).

Beispiel:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Das bedeutet, dass der Ausdruck in Zeile 1, Spalte 24 von `main.typl` ein `string` war, wo ein `i32` erwartet
wurde.

## 2. Fehler bei der Prüfung

Fehler, die vor der Ausführung gefunden werden. Die Form wird erst ausgeführt, wenn sie behoben sind.

### 2.1 Typen

| Meldung | Bedeutung und Behebung |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Ein Ausdruck vom Typ `U` steht dort, wo der Typ `T` gebraucht wird. Es gibt keine impliziten Umwandlungen; Zahlen wandelt man mit `(as T x)` um. Auch `int` und `i32` sind verschiedene Typen |
| ``integer literal 300 is out of range for u8 (0..=255)`` | Das Literal passt nicht in den Typ. Wer es abschneiden will, schreibt `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Es gibt keinen Typ dieses Namens. Einen Typ definiert man vor der ersten Form, die ihn verwendet (Typen haben keine Vorwärtsdeklaration). War eine Typvariable gemeint, schreibt man sie an eine Deklarationsstelle wie `<foo>` nach dem Funktionsnamen ([Syntaxreferenz 3.6](syntax.md#36-defstruct--strukturen-benutzerdefinierte-typen)) |
| ``cannot infer type argument `t` for `vector::new` `` | Ein Typargument lässt sich nicht bestimmen. Man schreibt den Typ mit `the`, wie in `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | Das `match` behandelt nicht alle Varianten. Man ergänzt Zweige für die fehlenden Varianten oder einen `_`-Zweig |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | Die Funktion verlangt einen Trait, den der übergebene Typ nicht implementiert. Man schreibt `(impl Eq pt ...)` ([Standard-Traits](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Ein Wert eines Typs, der den Trait nicht implementiert, wurde dort übergeben, wo ein `:dyn` erwartet wird. Man schreibt das `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | An einer Typstelle steht ein Trait-Name. Man schreibt `:dyn Error` |
| ``if: (if cond then else)`` | Das `if` hat die falsche Form. `if` verlangt einen else-Zweig. Wird keiner gebraucht, verwendet man `when` |

### 2.2 Namen

| Meldung | Bedeutung und Behebung |
|---|---|
| `no such function: bar` | Es gibt keine Funktion und keine Methode dieses Namens. Schreibweise prüfen |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Methoden werden nach dem Typ des ersten Arguments gewählt. Eine Methode dieses Namens gibt es, aber nicht für den Typ des ersten Arguments (hier `int`). Am Ende der Meldung stehen die Typen, die die Methode haben |
| `unbound variable: y` | Es gibt keine Variable dieses Namens. Schreibweise und Gültigkeitsbereich der Bindung prüfen (wird sie außerhalb ihres `let` verwendet?) |
| ``use: unresolved `nosuch` `` | Das in `use` genannte Modul wird nicht gefunden. Wie Dateinamen den Modulpfaden entsprechen, steht in [Syntaxreferenz 3.11](syntax.md#311-dateien-und-module-projekte-mit-mehreren-dateien) |
| `unresolved path: c::hidden` | Das Modul existiert, der Name aber nicht, oder er ist nicht sichtbar, weil `pub` fehlt |
| `circular module dependency: a -> b -> a` | Module holen sich gegenseitig mit `use` herein. Den gemeinsamen Teil verschiebt man in ein eigenes Modul |
| ``return-from: no enclosing block named `nope` `` | Kein `block` mit dem an `return-from` übergebenen Namen umschließt ihn. Der Block einer Funktion ist nur innerhalb dieser Funktion verwendbar |

### 2.3 Aufrufe

| Meldung | Bedeutung und Behebung |
|---|---|
| `f: expected 1 argument(s), got 2` | Die Anzahl der Argumente stimmt nicht |
| `f: unknown keyword argument :b` | Es wurde ein Schlüsselwortargument übergeben, das die Funktion nicht hat |
| `new: expected 1 field(s), got 2` | Die Anzahl der an einen Strukturkonstruktor übergebenen Werte stimmt nicht mit der Anzahl der Felder überein |
| ``setf: cannot assign to constant `k` `` | Es wurde an einen mit `defconstant` definierten Namen zugewiesen. Muss er sich ändern, verwendet man `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Eine mit `defsignature` deklarierte Funktion ist nicht definiert |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | Keiner der Argumenttypen hat die mit `~/name/` aufgerufene Methode ([Formatdirektiven, Kapitel 5](functions/format.md#5-name)) |

## 3. Lesefehler

| Meldung | Bedeutung und Behebung |
|---|---|
| `unexpected end of input while reading a list` | Eine schließende Klammer fehlt. Die Position zeigt, wo das Lesen endete (etwa das Dateiende), daher sucht man die öffnende Klammer |

## 4. Fehler zur Laufzeit (panic)

| Meldung | Bedeutung und Behebung |
|---|---|
| `panic: divide by zero` | Division durch null bei Ganzzahlen oder rationalen Zahlen. Die Gleitkommadivision durch null löst keinen Panic aus; sie ergibt `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` wurde auf `none` angewandt. Den Fall `none` behandelt man mit `match` oder `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Ein Index außerhalb des Bereichs. Die Länge prüft man mit `len`, oder man verwendet eine Funktion, die außerhalb des Bereichs `none` zurückgibt (`nth`, `pop` usw.) |
| `panic: an integer argument does not fit a fixnum` | Ein `int`, das nicht in 63 Bit passt, wurde an ein Argument übergeben, das einen Index oder eine Anzahl nimmt |
| `throw: no enclosing (catch 'oops) for this throw` | Ein `throw` lief ohne umschließendes `catch` mit derselben Marke |
| `panic: <message>` | Das Programm hat `(panic "<message>")` aufgerufen. Ein fehlgeschlagenes `assert` ergibt `assertion failed: ...` |

Ein `panic` hält den ganzen Prozess an, auch wenn er innerhalb eines Tasks auftritt
([Syntaxreferenz 12.4](syntax.md#124-zusammenspiel-mit-anderen-funktionen)). Fehlschläge, von denen man sich
erholen will, drückt man mit `Result` aus ([Kapitel 9 der Syntaxreferenz](syntax.md#9-grundsätze-der-fehlerbehandlung)).

## 5. Warnungen

| Meldung | Bedeutung |
|---|---|
| ``warning: redefining function `f` `` | Eine gleichnamige Funktion wurde erneut definiert. Die spätere Definition gilt. Das erscheint normalerweise, wenn man in der REPL eine Definition korrigiert |
