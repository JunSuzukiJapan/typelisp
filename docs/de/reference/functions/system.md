<!-- translated-from: docs/ja/reference/functions/system.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Zeit, Umgebung und Implementierung

Funktionen für Zeit, Abfragen der Laufzeitumgebung, Werkzeuge der Implementierung, Parsen und Auswerten von
Text, Docstrings und Makros.

## 1. Zeit

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Zwei Felder: `day` (Tage seit 1900-01-01) und `second` (die Sekunde innerhalb dieses Tages, 0..86399) |
| `internal-time` | — | `defstruct` | Zwei Felder: `second` und `microsecond` (innerhalb dieser Sekunde, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | Die Zeit seit CLs Epoche (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Verstrichene Zeit relativ zum Prozess |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | Die **CPU-Zeit**, die dieser Prozess verbraucht hat (Benutzer plus System) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | Als Anzahl Sekunden. Die Form, um die Differenz zweier Messungen anzugeben |
| `internal-time-units-per-second` | — | `int` | `1000000` (Mikrosekunden), die Einheit des Feldes `microsecond`. Wie in CL ist der Wert Sache der Implementierung |
| `time` | `(time form)` | Makro | Führt `form` aus, gibt Echtzeit und CPU-Zeit auf je einer Zeile aus und gibt den Wert von `form` unverändert zurück |

Echtzeit und CPU-Zeit sagen Verschiedenes. Bei Arbeit, die hauptsächlich auf E/A wartet, unterscheiden sich
beide stark, und genau diesen Unterschied will man wissen, daher zeigt `time` beide.

`sleep`, das einen Task anhält, steht unter [Tasks und Kanäle](concurrency.md#3-yield--sleep--den-vortritt-lassen).

## 2. Datumsangaben zerlegen und zusammensetzen

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Neun Felder**: `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. CLs neun Rückgabewerte als eine Struktur (es gibt keine Mehrfachwerte) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Universalzeit in Kalenderkomponenten. `zone` sind Stunden westlich von Greenwich (dieselbe Richtung wie in CL). **Weggelassen ist es Ortszeit** (wie in CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | Die Umkehrung. Ohne `zone` werden die Argumente als **Ortszeit** gelesen |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Jetzt, in Ortszeit zerlegt |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | Der Versatz der Ortszeit westlich von Greenwich in **Sekunden** zu dieser Universalzeit |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Ob zu dieser Universalzeit Sommerzeit galt |

Wie in CL ist bei `day-of-week` **0 Montag und 6 Sonntag**.

**Ohne `zone` wird Ortszeit verwendet**, wie in CL. Der lokale Versatz wird beim BS erfragt, daher hängt das
Ergebnis davon ab, wo die Maschine steht. **Eine ausdrückliche Zone macht es deterministisch**, und `0` ist UTC.

Die Einheit von `zone` ist wie in CL „Stunden westlich von Greenwich“, daher liest sich UTC+9 als `-9`. Allerdings
**ist das Argument eine Ganzzahl und das Feld `zone` des Ergebnisses ein `f64`**. Echte Versätze sind nicht immer
ganze Stunden (Indien ist +5:30, Nepal +5:45), und den gemeldeten Wert zu runden, würde stillschweigend eine
Lüge erzählen. Eine Zone, die man von Hand schreibt, ist eine ganze Zahl von Stunden, daher ist das Argument
`int`.

Wird `zone` angegeben, ist `daylight-p` `false` und `zone` genau der angegebene Wert, wie CL es vorschreibt
(*If a time-zone is supplied, daylight saving time information is ignored*).

Eine Ortszeit, die in einen Sommerzeitwechsel fällt, ist von vornherein nicht eindeutig, und CL sagt nicht,
welche zu nehmen ist. `encode-universal-time` gibt für eine solche Zeit eine der beiden Antworten zurück.

## 3. Die Laufzeitumgebung

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | Die Kommandozeile. **Element 0 ist der Programmname** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Eine Umgebungsvariable. `none`, wenn nicht gesetzt oder kein UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. Die Grundlage von `user-homedir-pathname` ([Pfadnamen](streams-files.md#92-funktionen)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | Die Version der Implementierung |
| `machine-type` | `(machine-type)` | `()→string` | Die CPU-Architektur (`x86_64` / `aarch64` …). Der Wert des **Build-Ziels** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | Der Rechnername |
| `machine-version` | `(machine-version)` | `()→Option<string>` | Der Name der **gerade laufenden** Hardware (`Apple M1` / `Intel(R) Xeon(R) …`). `none`, wo er sich nicht ermitteln lässt |
| `software-type` | `(software-type)` | `()→string` | Das BS (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | Die BS-Version (`uname -r`, zum Beispiel `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Ein kurzer Name des Installationsorts. **Immer `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | Ebenso ein langer Name. **Immer `none`** |

Die Funktionen, die `Option` zurückgeben, sind Punkte, für die CL `NIL` erlaubt (*or nil if no such name can be
determined*). POSIX hat keinen Ort, an dem Ortsnamen festgehalten würden, daher sind sie immer `none`; SBCL gibt
dasselbe zurück. Man beachte den Unterschied zwischen `machine-type` und `machine-version`: Ersteres ist die
Architektur, für die diese Binärdatei **gebaut** wurde, Letzteres der Chip, der sie gerade **ausführt**.

Element 0 von `command-line-args` ist bei `typl script.typl a b` der Pfad des Skripts und bei einem als
`./prog a b` ausgeführten AOT-Programm die ausführbare Datei selbst. **Beide Ausführungsarten lesen dieselben
Argumente an denselben Indizes** (`typl` entfernt seinen eigenen Namen und Optionen wie `--heap-cells`, bevor
es sie weiterreicht).

## 4. Den Benutzer fragen

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Nimmt ein einzelnes `y` / `n`. Fragt erneut, bis es eines bekommt |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Lässt den Benutzer `yes` / `no` ausschreiben. Für Fragen, bei denen ein Fehler teuer ist |

Beide lesen aus `*standard-input*`. Nur das Ende der Eingabe beendet das erneute Fragen, und dann ist das
Ergebnis `false`.

## 5. Werkzeuge der Implementierung (CLHS 25.2)

Die Schicht, in der die Implementierung Fragen über sich selbst beantwortet. `heap-info` / `room` / `dribble`
sind gewöhnliche Funktionen; `trace` / `untrace` / `step` / `disassemble` / `ed` sind **Spezialformen**
(`trace` / `untrace` / `disassemble` / `ed` nehmen den *Namen* einer Definition und `step` eine *Form*, alle
unausgewertet).

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | Der aktuelle Zustand des Heaps als Struktur. Dieselben Zahlen, die `room` ausgibt |
| `room` | `(room &optional verbose)` | `(bool)→()` | Meldet `heap-info` an `*standard-output*`. `(room true)` liefert mehr Details |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Beginnt, die Ausgabe der Sitzung in `path` aufzuzeichnen / beendet die Aufzeichnung, wenn ohne Argument aufgerufen |
| `trace` | `(trace name...)` | `Sexpr` | Meldet Aufrufe der genannten Definitionen an `*trace-output*`. Gibt die Liste der gerade verfolgten Namen zurück |
| `untrace` | `(untrace name...)` | `Sexpr` | Beendet die Meldungen. **Ohne Argumente entfernt es alle** |
| `step` | `(step form)` | Der Typ von `form` | Wertet `form` aus und hält bei jedem Aufruf an, um zu fragen |
| `disassemble` | `(disassemble name [llvm])` | `()` | Gibt aus, was aus dieser Definition wird. Standardmäßig der Maschinencode des Rechners, mit `true` LLVM IR |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Startet `$VISUAL` / `$EDITOR`. Mit einem Namen öffnet es die Zeile, in der diese Definition steht |

`trace`/`untrace`/`step`/`disassemble` gibt es nur im Interpreter, und Funktionen, die sie aufrufen, lassen sich
nicht kompilieren ([Kapitel 10 der Syntaxreferenz](../syntax.md#10-kompilierung)).

### 5.1 Felder von `heap-info`

| Feld | Typ | Inhalt |
|---|---|---|
| `capacity` / `live` / `free` | `int` | Die ganze cons-Arena und ihre Aufteilung. Immer `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | Die aktuelle Anzahl der drei anderen Objektarten des Heaps |
| `gc-count` | `int` | Die Anzahl der Sammlungen seit dem Start der Implementierung |
| `growable` | `bool` | Ob die Arena noch wachsen kann |

Die Felder sind alle `int` (außer `growable`). Die Wachstumsgrenze (siehe die Beschreibung von
`typl --heap-cells`) wird nicht gemeldet, denn ein Leser will wissen, ob sie noch wachsen kann (`growable`).

### 5.2 Was `trace` / `step` sehen können und was nicht

- **Definitionen mit kompilierten Rümpfen sind ebenfalls sichtbar, von interpretierten Aufrufstellen aus.**
- **Aufrufstellen *innerhalb* kompilierten Codes sind nicht sichtbar.** Verfolgt man einen Namen mit
  kompiliertem Rumpf, wird ein einzeiliger Hinweis darauf angefügt. Dieselbe Einschränkung, die SBCL für lokale
  Aufrufe beschreibt.
- **Aufrufe über Closure-Werte (`funcall`/`apply`) sind nicht sichtbar.** Closures haben keine Namen.
- **Generische Definitionen sind nicht abgedeckt.** An jeder Verwendungsstelle wird für jeden Typ eine Kopie
  erzeugt, daher gibt es keinen einzelnen Rumpf, den man benennen könnte (derselbe Grund und derselbe Wortlaut
  wie bei der Ablehnung durch `compile`).

Die Befehle von `step` sind `s` (in diesen Aufruf hineingehen; eine leere Zeile tut dasselbe), `n` (diesen Aufruf
überspringen), `c` (ab hier nicht mehr fragen) und `q` (abbrechen). **Ist die Standardeingabe kein Terminal,
wertet `step` einfach `form` aus**: ein entartetes Verhalten, das CLHS ausdrücklich erlaubt, damit Skripte und
Tests nicht an einer Eingabeaufforderung hängen, die niemand beantworten kann.

`$VISUAL` / `$EDITOR` von `ed` wird an Leerraum geteilt, daher funktioniert `EDITOR="code -w"`. Ist keines
gesetzt, ist das Ergebnis `Err`: Es rät nicht `vi`. Die Zeilennummer wird zuerst übergeben, in der Form `+N`.

`dribble` zeichnet alle drei Wege auf, auf denen die Ausgabe der Sitzung den Prozess verlässt: was
`print`/`println`/`format` schreiben, was in mit der Standardausgabe verbundene Streams geschrieben wird, und
die in die REPL getippten Zeilen samt den Werten, die die REPL zurückgibt.

## 6. Parsen und Auswerten

All diese behandeln Text und Daten aus der Laufzeit (die das Programm selbst nicht kontrolliert), daher geben
sie bei Fehlschlag das `Err` eines `Result` zurück, statt einen Panic auszulösen. Die Fehlertypen sind konkrete
Typen pro Operation ([Fehlertypen](option-result.md#3-fehlertypen-und-der-trait-error)).

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | CLs `parse-integer`. Überspringt führenden und abschließenden Leerraum (dieselbe Menge wie `trim`), liest höchstens ein Vorzeichen `+`/`-` und dann Ziffern zur Basis `radix` (Standard 10, 2 bis 36; Ziffern über 10 in beliebiger Schreibweise). Die Anzahl der Ziffern ist unbegrenzt (`int`). Andere übrig bleibende Zeichen ergeben `Err`. Mit `:junk-allowed true` hält es bei der ersten Nicht-Ziffer an und ignoriert den Rest, ergibt aber `Err`, wenn es keine einzige Ziffer gibt (entspricht CLs `nil`). Den zweiten Wert von CL (die Position, an der das Lesen endete) gibt es nicht zurück. Ein `radix` außerhalb des Bereichs löst einen Panic aus (ein Fehler des Aufrufers, nicht im Text) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Eine Gleitkommazahl. Akzeptiert auch `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Liest ein `Sexpr` aus `s` (mit demselben Reader, der Quelltext liest). Unausgeglichene Klammern, nicht abgeschlossene Zeichenketten und Ähnliches ergeben `Err`. Aus einem Stream liest man mit `read-sexpr` ([Streams](streams-files.md#6-generische-funktionen-und-dateioperationen)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` plus **die Position, an der das Lesen endete**. `(car r)` ist der Wert und `(cdr r)` die Position des nächsten zu lesenden Zeichens. `start` ist standardmäßig 0 |
| `read-from-string-preserving-whitespace` | Wie oben | Wie oben | Dasselbe, verbraucht aber nicht den Leerraum, der das Datum beendet hat. Der Unterschied zeigt sich in der zurückgegebenen Position |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Typprüft `form` zur Laufzeit und wertet es aus. Folgt CLs `eval` |

CL gibt aus `read-from-string` **zwei Werte** zurück (den Wert und die Position), aber diese Sprache hat keine
Mehrfachwerte, daher gibt sie eine `cons-cell` zurück. Mit der Position wird das datumweise Lesen einer
Zeichenkette zu einer Schleife statt einem erneuten Durchsuchen:

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

Der Unterschied durch `preserving-whitespace` ist **ein Leerraumzeichen**: CLs `read` verbraucht den Leerraum,
der das Datum beendet hat, und `read-preserving-whitespace` lässt ihn stehen. `(read-from-string "12 34")` gibt
die Position 3 zurück, die bewahrende Fassung 2.

Die Zahlensyntax, die der Reader akzeptiert, steht in
[Kapitel 1 der Syntaxreferenz](../syntax.md#1-lexikalische-elemente). Was `*print-radix*`
([Ausgabe](printing.md#62-basis-schreibweise-und-lesbarkeit)) ausgibt, lässt sich unverändert zurücklesen. Ein
CL-`*read-base*` gibt es nicht.

### 6.1 Was `eval` bedeutet

Es folgt CLHS `eval`: Es wertet in **der aktuellen globalen Umgebung** aus (globale Funktionen, Variablen, Typen
und Makros, einschließlich zur Laufzeit hinzugefügter Definitionen) und in **der leeren lexikalischen Umgebung**
(die lokalen Bindungen von `let`/`lambda` des Aufrufers sind nicht sichtbar). Sowohl Ausdrücke als auch
Definitionen (`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`) lassen sich auswerten, und Definitionen werden
sofort und dauerhaft in der globalen Umgebung registriert.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; das globale x ist sichtbar
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; gibt den definierten Namen zurück
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; die gerade erstellte Definition ist sichtbar
```

- **Rückgabewert**: bei einem Ausdruck das Ergebnis als `Option<Sexpr>`; bei einer Definition das Symbol des
  definierten Namens (wie in CL). Um das Ergebnis zu verwenden, zerlegt man das `Sexpr` mit `match`
  (`(int n)`/`(str s)`/…).
- **Unterschiede durch statische Typen (wichtig)**: CL gibt den tatsächlichen Wert des Ergebnisses zurück, aber
  in dieser Sprache kann der Rückgabetyp nur einheitlich `Result<Option<Sexpr>,EvalError>` sein. Außerdem **kann
  statisch geschriebener Code nicht auf Namen vorausverweisen, die `eval` zur Laufzeit definiert**: Ein direkt in
  eine Datei geschriebenes `(sq 9)` wird geprüft, bevor das `eval`, das `sq` definiert, läuft, und ist
  „undefiniert“. **Spätere `eval`s sehen es jedoch** (ihre Typprüfung läuft zur Laufzeit, nach der Definition).
  Die REPL prüft und führt Zeile für Zeile aus, daher lässt sich ein mit `eval` definierter Name ab der nächsten
  Zeile direkt aufrufen.
- **Fehler**: Typfehler und Syntaxfehler geben `Err` zurück (kein Panic). **Panics zur Laufzeit** im
  ausgewerteten Code (Division durch null usw.) breiten sich aus, wie sie es bei direkt geschriebenem Code
  täten. Die Aufräumarbeit jedes `unwind-protect` dazwischen läuft
  ([Kapitel 8 der Syntaxreferenz](../syntax.md#8-nichtlokale-ausgänge-catch--throw--unwind-protect)).
- **Namensraum**: Bei Ausführung durch `typl file.typl` und innerhalb eines AOT-Programms wertet `eval` im
  Namensraum des Moduls des Skripts aus (die eigenen globalen Namen des Skripts sind sichtbar). Die REPL wertet
  im Wurzelnamensraum aus.
- **Kompilierung**: Sowohl `read` als auch `eval` lassen sich kompilieren. Wie sie in AOT-Programmen behandelt
  werden und welche Folgen das hat (an eval übergebene Formen werden interpretiert), steht in
  [Syntaxreferenz 10.2](../syntax.md#102-eval-in-aot-programmen).

## 7. Docstrings / `documentation`

`defun`/`defmethod` (auch innerhalb von `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/
`deftype`/`deftrait` können Docstrings tragen. Die Position folgt jeweils der Regel von CL:

| Form | Position des Docstrings |
|---|---|
| `defun` / `defmethod` / `defmacro` | Am Anfang des Rumpfes (nach dem Rückgabetyp und der `where`-Klausel). Nur wenn mindestens eine Rumpfform folgt; eine einzelne Zeichenkette bleibt der Rückgabewert |
| `defvar` / `defconstant` | **Nach** dem Anfangswert: `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Direkt nach** dem Namen, vor den Feldern/Varianten |
| `deftype` | **Direkt nach** dem Namen, vor dem Typ: `(deftype meters "doc" i32)` |
| `deftrait` | Direkt nach der Liste der Obertraits, vor den Einträgen. Einer für den ganzen Trait. **Methoden mit Standardimplementierung** können ihren eigenen Docstring direkt vor ihren Rumpf setzen |

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `documentation` | `(documentation name)` | (Spezialform; `name` ist ein bloßes Symbol oder `Type::method`)→`Option<string>` | Gibt den Docstring von `name` zurück |

Wie `quote`/`compile` ist `documentation` eine Spezialform (es liest `name` als unausgewerteten Namen). Anders
als CLs `(documentation 'name 'function)` nimmt es kein Typargument; stattdessen löst es einen bloßen Namen in
der Reihenfolge **Variable → Funktion → Typ → Trait → Makro** auf (dieselbe Priorität wie bei einem bloßen
Bezeichner, der als Ausdruck ausgewertet wird). Die Form `Type::method` schlägt den Docstring einer assoziierten
oder statischen Methode nach.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**Der Wert wird bei der Prüfung bestimmt**: Löst sich der Name zu keiner Definition auf, ist es ein Fehler bei
der Prüfung (wie der Verweis auf eine undefinierte Variable). Löst er sich auf, gibt es aber keinen Docstring,
ist das Ergebnis `Option::none`.

**Nicht abgedeckt**:

- `(setf documentation)` (einen Docstring zur Laufzeit ändern) gibt es nicht.
- Modulqualifizierte freie Namen (`mod::name`; `Type::method` wird unterstützt) werden nicht unterstützt.
- Eine Methodendeklaration in einem `deftrait` **ohne Rumpf** kann keinen Docstring haben. Ein abschließendes
  Zeichenkettenliteral wäre selbst der Rumpf (der Rückgabewert) einer Standardimplementierung, daher gibt es
  keine Möglichkeit, beides zu unterscheiden.

Auch der Hover des Sprachservers (`typl-lsp`) zeigt Docstrings an.

## 8. Makros

Wie man Makros definiert, steht in [Syntaxreferenz 3.14](../syntax.md#314-defmacro--makrodefinitionen).

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Ein neues Symbol. Sein Name ist `" <prefix><n>"`, wobei `n` `*gensym-counter*` ist. Ein führendes Leerzeichen lässt sich im Quelltext nicht schreiben, daher kollidieren die erzeugten Bindungen nie mit geschriebenen Namen |
| `*gensym-counter*` | Variable | `int` | Die Zahl, die `gensym` als Nächstes verwendet. Wie in CL lesbar und setzbar |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Expandiert einen Makroaufruf um einen Schritt. `none` bedeutet „kein Makroaufruf“ |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Wiederholt, bis es kein Makro mehr ist |

`macroexpand-1` gibt ein `Option` zurück. CL meldet „ob expandiert wurde“ als zweiten Rückgabewert, aber es gibt
keine Mehrfachwerte, daher übernimmt `none` diese Rolle. **Ein Makro, das zu einem Aufruf seiner selbst
expandiert, kann nie mit einem Nicht-Makro verwechselt werden.** Ein Expansionsschritt ist derselbe, den die
Typprüfung verwendet, daher laufen das, was das Programm sieht, und das, was die Prüfung sah, nie auseinander.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none erscheint als leere Liste (Option<Sexpr> ist transparent)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

Was CL hat und diese Sprache nicht: `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute` fallen immer
zusammen, daher gibt es keine Unterscheidung zu wählen), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (nicht internierte Symbole; Bindungen werden über den Namen gesucht, daher
wäre nichts zu gewinnen).

## 9. Lokale Makrobindungen (`macrolet` / `symbol-macrolet`)

Beide sind Spezialformen, die **Namen, die keine Werte sind**, lexikalisch binden. Zur Laufzeit bleibt nichts
übrig: Kompiliert wird die expandierte Form des Rumpfes.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Eine `macrolet`-Bindung verdeckt ein gleichnamiges globales Makro **nur während des Rumpfes**. Die
  Lambda-Liste ist dieselbe wie bei `defmacro` (`&optional`/`&rest`/`&key`).
- **Geschwister desselben `macrolet` sehen einander nicht aus ihren *Rümpfen*** (wie in CL; das ist der
  Unterschied zu `labels`). Expansionen werden an der Verwendungsstelle geprüft, daher funktioniert es, wenn
  `earlier` zu `(later ...)` expandiert: An dieser Stelle sind beide sichtbar.
- Ein `symbol-macrolet`-Name kommt als gewöhnliche Bindung in die Umgebung. Daher verdeckt ein inneres `let`
  denselben Namen, und eine äußere Variable wird verdeckt: CLs Regeln ergeben sich unverändert.
- **`setf` schreibt in die Expansion.** `(setf head 42)` ist `(setf (get v 0) 42)`.
- Expansionen werden in **der Umgebung der Verwendungsstelle** geprüft (nicht der Bindungsstelle).

## 10. Sonstiges

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Panic, wenn falsch. Ohne Meldung `assertion failed: <der Test, wie geschrieben>` (es ist ein Makro und kann daher den Ausdruck selbst nennen). CLs Restarts gibt es in dieser Sprache nicht |
| `warn` | `(warn control args...)` | `(string,...)→()` | Schreibt eine Zeile mit dem Präfix `WARNING: ` auf `*error-output*` und **läuft weiter**. Eine Möglichkeit, etwas zu melden, ohne ein `Result` zurückzugeben und ohne das Programm zu beenden |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Ersetzt globale Variablen nur während `body` und stellt sie beim Verlassen wieder her. CL schreibt das als `let`, aber `let` bindet in dieser Sprache immer lexikalisch, daher der eigene Name (dieselbe Rolle wie das gleichnamige Makro von Emacs Lisp). Stellt sie wieder her, wie auch immer der Rumpf verlassen wird: normaler Abschluss, `throw`, `panic`, `break`/`return`. **Keine Bindung pro Task** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Führt `body` aus, mit jeder Steuervariable der Ausgabe auf ihrem Standardwert und `*read-eval*` auf `true` ([Ausgabe](printing.md#6-steuern-wie-viel-ausgegeben-wird)) |
| `exit` | `(exit code)` | `int→!` | Beendet den Prozess |
| `dump` | `(dump path)` | `string→bool` | Schreibt die aktuelle Umgebung (Typinformation plus kompilierte Rümpfe) in eine Datei. `typl --image <path>` startet erneut daraus. Nur im Interpreter ([Syntaxreferenz 10.1](../syntax.md#101-dumps)) |
