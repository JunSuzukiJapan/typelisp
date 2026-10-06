<!-- translated-from: docs/ja/reference/functions/option-result.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Option, Result und Fehlertypen

## 1. `Option<T>` / `Result<T,E>`

Konstruktoren: `Option<T>` hat `Some(T)` / `None`. `Result<T,E>` hat `Ok(T)` / `Err(E)`. `E` kann ein
beliebiger Typ sein: Die eingebauten konkreten Fehlertypen und selbst geschriebene Typen mit
`defstruct`/`defenum` passen gleichermaßen dorthin (Kapitel 3).

| Name | Form | Option | Result | Beschreibung |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Holt den Wert heraus. Panic bei `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | Der Wert oder der Standardwert |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Ob es `Some` ist |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Ob es `None` ist |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Ob es `Ok` ist |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Ob es `Err` ist |

Die Konstruktoren sind `Option::some`/`Option::none`/`Result::ok`/`Result::err` (oder nach
`(use option)`/`(use result)` die bloßen Namen `some`/`none`/`ok`/`err`).

Verzweigungen schreibt man ausdrücklich mit `match`. Eine Syntax, die Rusts `?` entspricht, gibt es nicht.

## 2. Die Laufzeitdarstellung von `Option<T>`

Wie in Rust **erzeugt `Option<T>` normalerweise keine Box**. `some v` ist `v` selbst, und `none` ist der Wert
der leeren Liste, ohne Speicheranforderung und ohne Indirektion. `Option<Sexpr>` (wo die leere Liste `none`
ist), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` und `Option<(fn ...)>` haben alle
diese Form.

Eine Box wird nur verwendet, wenn sich ein Wert von `T` nicht vom Wert der leeren Liste unterscheiden lässt:

| `T` | Darstellung | Grund |
|---|---|---|
| `Option<U>` (verschachtelt) | Box | Das innere `none` wäre derselbe Wert wie das äußere `none` |
| `()` | Box | Der Wert von `()` ist der Wert der leeren Liste selbst |
| `ptr` / `c-long` / `c-ulong` | Box | Alle 64 Bit sind Wert, es bleibt kein Platz, sie zu unterscheiden |
| Alles andere | Keine Box | — |

Die Darstellung wird allein durch den Typ bestimmt und lässt sich nicht aus einem Wert ablesen. Bei der
Ausgabe wird `(some ...)`/`none` aus dem statischen Typ wiederhergestellt, daher gibt
`(format false "~a" opt)` `(some 1)` aus. Es gibt zwei Einschränkungen:

- **Es kann nicht in ein `:dyn Trait` gesteckt werden** (einen Wert von `Option<int>`, für den man
  `(impl Speak Option<int> ...)` geschrieben hat, an ein `:dyn Speak` zu übergeben, ist ein Fehler).
- Ein Downcast `(the Option<T> ...)` aus einem `Sexpr` **nennt einen Konstruktor**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. Die Form, die den ganzen Wert bindet,
  `(the Option<int> o)`, ist ein Fehler.

## 3. Fehlertypen und der Trait `Error`

Nach dem Vorbild von Rusts `std::error::Error` ist **`Error` kein Typ, sondern ein Trait**. Die konkreten Typen,
die Fehler darstellen, sind für jeden Zweck eigene, und jeder implementiert `Error`.

| Typ | Erzeugt von |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Datei- und Stream-Operationen ([Streams und Dateien](streams-files.md)) |
| `NetError` | Netzwerkoperationen ([Netzwerk](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. CLs `simple-error`: die Standardwahl, wenn man nur sagen will, was passiert ist |
| `WrappedError` | `(wrap-error msg cause)`. Ein Typ, der sowohl die eigene Meldung als auch die Ursache trägt; deshalb hat der Trait `Error` `source` |

`ParseIntError` bis `NetError` sind jeweils „ein Enum mit einer einzigen Variante, die eine Meldungszeichenkette
hält“, und Typname und Variantenname sind gleich (`(match e ((ParseIntError m) m))`, erzeugt mit
`(ParseIntError::ParseIntError "...")`). An ihnen ist nichts Besonderes: Sie werden genauso behandelt wie
eigene Fehlertypen, die man mit `(defstruct my-err (...))` / `(defenum my-err ...)` schreibt.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Die Fehlermeldung (eine Methode des Traits `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | Die Ursache, die dieser Fehler umhüllt, oder `None`, wenn es keine gibt (Rusts `Error::source`) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implementiert `Error`) | Weitet einen konkreten Fehlertyp zum Trait-Objekt |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implementiert `Error`) | Die Meldung und die Kette der Ursachen, die man durch Verfolgen von `source` findet, eine Ursache pro Zeile. In CL gibt es kein Gegenstück (Rusts „caused by“) |

Implementiert man `Error` für den eigenen Fehlertyp, lässt er sich **genauso** behandeln wie die eingebauten
Fehler:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; der konkrete Typ kommt unverändert in E
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; jede Art einheitlich behandeln
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Um mehrere Fehlertypen in einem `Result` zu sammeln, verwendet man `Result<T, :dyn Error>` (entspricht Rusts
`Box<dyn Error>`) und weitet konkrete Fehler mit `as-dyn-error`. Da es kein `?` gibt, schreibt man diese
Umwandlung ausdrücklich:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Typen und Traits teilen sich einen Namensraum** (wie in Rust). Innerhalb eines Moduls können ein
`defstruct`/`defenum` und ein Trait nicht denselben Namen haben, und einen Trait-Namen an eine Typstelle zu
schreiben, wird als „`error` is a trait, not a type — write `:dyn error`“ gemeldet.

Nicht behebbare Fehlschläge werden mit `panic` ausgedrückt. Zu `panic` und `catch`/`throw` siehe die
[Syntaxreferenz](../syntax.md#8-nichtlokale-ausgänge-catch--throw--unwind-protect); zu den Grundsätzen der
Fehlerbehandlung siehe [Kapitel 9 derselben](../syntax.md#9-grundsätze-der-fehlerbehandlung).
