<!-- translated-from: docs/ja/reference/syntax.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Syntaxreferenz von typelisp

typelisp ist ein statisch typisiertes Lisp, geschrieben in S-Ausdrücken. Die Liste der eingebauten Funktionen
und Methoden steht unter [Eingebaute Funktionen](functions/README.md), die Liste der Typen in
[types.md](types.md) und wie man Fehlermeldungen liest in [errors.md](errors.md).

## 1. Lexikalische Elemente

- **Groß-/Kleinschreibung wird nicht unterschieden.** Symbole werden beim Lesen alle in Kleinbuchstaben
  normalisiert.
- **Kommentare**: von `;` bis zum Zeilenende (Zeilenkommentare). `#| ... |#` (Blockkommentare, die sich
  verschachteln lassen).
- **Auswertung zur Lesezeit**: `#.(expr)` **führt die folgende Form während des Lesens aus** und behandelt ihren
  Wert als das Gelesene. Das ist die einzige Stelle, an der der Reader mehr ist als eine Funktion des Textes. Wie
  weit sie reicht, hängt wie in CL vom Leseweg ab:
  - `(load ...)` und die REPL werten eine Form nach der anderen aus, daher kann sie **früher im selben Text
    definierte Funktionen** aufrufen (CLs `load`).
  - Eine Moduldatei wird als Einheit geprüft und von dem ausgeführt, der sie mit `use` hereinholt, daher erreicht
    `#.` nur die Standardbibliothek und das, was die Sitzung bereits ausgeführt hat. Weder die eigenen
    Definitionen der Datei noch die der Module, die sie mit `use` hereinholt, **sind schon gelaufen** (so wie
    CLs `compile-file` `eval-when` braucht).
  - `read` / `read-from-string` innerhalb eines Programms werten `#.` ebenfalls aus (wie in CL).
  - Setzt man `*read-eval*` (Standard `true`) auf `false`, wird `#.` überall zu einem Lesefehler: ein Schalter,
    damit als Daten gelesener Text keinen Code ausführt (wie in CL). Er wird bei jedem `#.` abgefragt, daher
    wirkt ein `setf` ab der nächsten gelesenen Form. Innerhalb von `with-standard-io-syntax` ist er `true`.
- **Wahrheitswerte**: `true` / `false`.
- **Ganzzahlen**: dezimal (`42`, `-7`). Ein Vorzeichen `+`/`-` darf vorangehen. Andere Basen schreibt man mit
  CLs Basissyntax `#b`/`#o`/`#x`/`#NNr` (das Vorzeichen steht nach der Markierung: `#x-ff`). Das Präfix `0x`
  gibt es in CL nicht, und es wird nicht übernommen: `0xff` wird als Symbol gelesen.
  Ein Ganzzahlliteral ohne Typannotation ist standardmäßig `int` (beliebige Genauigkeit,
  [Zahlen](functions/numbers.md#3-ganzzahlen-beliebiger-genauigkeit-int)), ohne Obergrenze seiner Größe.
  **Ist der erwartete Typ ein Ganzzahltyp fester Breite, nimmt das Literal diesen Typ an, und es wird geprüft, ob
  der Typ den Wert fassen kann**: `(the u8 300)` ist ein Typfehler (wer es abschneiden will, schreibt
  `(as u8 300)`). `(the u32 4294967295)` und `(the u32 #xFFFFFFFF)` lassen sich dank dieser Regel schreiben.
  Ob ein `int`-Wert in einen 63-Bit-Direktwert passt oder zur Bignum wird, entscheidet seine Größe, ohne
  besondere Syntax (wie in CL).
- **Gleitkommazahlen**: solche mit Dezimalpunkt oder Exponent (`e`/`E`) (`1.5`, `3.0e10`). Standardmäßig `f64`
  (`f32`, wenn das der erwartete Typ ist).
- **Brüche**: `Zähler/Nenner` (nur dezimal, zum Beispiel `1/3`). Beim Lesen gekürzt, wie CL es vorschreibt
  (`2/4` ist `1/2`). Solche mit ganzzahligem Wert (`4/2` usw.) werden als `int` gelesen, nicht als `ratio`. Ein
  Nenner null (`1/0`) ist ein Lesefehler.
- **Zeichen**: `#\` gefolgt von einem Zeichen oder einem Zeichennamen. Zum Beispiel `#\a` `#\Space`
  `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul` (auch `#\Null`) `#\Backspace`. Bei Namen wird
  Groß-/Kleinschreibung nicht unterschieden.
- **Zeichenketten**: `"..."`. Die Maskierungen sind `\n` `\t` `\r` `\0` `\\` `\"` (jedes andere `\x` ist einfach
  `x`).
- **Symbole**: jedes Token aus Buchstaben, Ziffern und Sonderzeichen (`+` `<=` `my-func` usw.).
- **Schlüsselwörter**: Symbole, die mit einem Doppelpunkt beginnen, wie `:name` (wie in CL). Sie werten zu sich
  selbst aus: Sie suchen keine Bindung, ihr Wert sind sie selbst, mit dem statischen Typ `symbol`.
  Schlüsselwörter gleichen Namens sind immer dasselbe Objekt (`(eq :foo :FOO)` ist wahr; wie andere Symbole
  werden sie kleingeschrieben). Der Doppelpunkt selbst ist Teil des Namens, daher ist `(symbol->string :foo)`
  `":foo"` (typelisp hat kein Paketsystem, daher unterscheidet sich das von CLs `symbol-name`). Ein einzelnes
  `:` oder eines mit weiteren Doppelpunkten wie `:a:b` ist ein Lesefehler. Geprüft wird mit `keywordp`. Was mit
  `::` beginnt, ist kein Schlüsselwort, sondern ein absoluter Pfad (unten).
  Man beachte, dass `:dyn` ein reserviertes Schlüsselwort nur für Typstellen ist; es anderswo zu schreiben, ist
  ein Fehler (siehe [Kapitel 2](#2-typen-schreiben)).
- **Listen**: `(a b c)`. Auch gepunktete Paare `(a . b)` lassen sich lesen.
- **Die leere Liste `()`**: je nach Kontext der Wert des Typs `Unit` oder das `none` von `Option<Sexpr>`.
  **`Sexpr` hat keine Variante für die leere Liste**: `Sexpr` bedeutet „ein nicht leerer S-Ausdruck“, und der
  Typ von S-Ausdrucksdaten ist `Option<Sexpr>` (siehe „Muster für `Option<Sexpr>`“ in
  [4.3 match](#43-match--mustervergleich)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (nur innerhalb eines quasiquote sinnvoll)
  - `,@x` → `(unquote-splicing x)` (bei der Expansion als Listenelemente eingefügt)
- **Pfade `::`**: `foo::bar` wird als Pfad durch Module, Typen und Mitglieder gelesen (nicht als ein einzelner
  Symbolname). Was mit `::` beginnt, wie `::foo`, ist ein absoluter Pfad ab der Wurzel. Ein `::` innerhalb
  generischer Argumente (`Vec<a::b>` und Ähnliches) wird nicht als Pfadtrenner behandelt.

## 2. Typen schreiben

Im Quelltext werden Typen als gewöhnliche Symbole oder Listen geschrieben.

- **Primitive Typen**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string` `symbol`.
  `int` ist der Ganzzahltyp (CLs integer, der automatisch zwischen 63-Bit-Direktwerten und Bignums wechselt;
  [Zahlen](functions/numbers.md#3-ganzzahlen-beliebiger-genauigkeit-int)), und die sechs Typen fester Breite
  sind nach Breite und Vorzeichenbehaftung benannt (einen 64-Bit-Ganzzahltyp gibt es nicht; siehe
  [Zahlen](functions/numbers.md#1-ganzzahlen-fester-breite)).
- **Der rationale Typ**: `ratio` (gekürzte rationale Zahlen). Wie in CL auf dem Heap angelegt, ohne implizite
  Umwandlung mit `int`/`f64` und Ähnlichem (ausdrücklich mit `as`/`try-as` oder einer Umwandlungsmethode
  umwandeln; siehe [Zahlen](functions/numbers.md#5-rationale-zahlen-ratio)).
- **Rohe Wörter an der C-Grenze**: `ptr` (ein undurchsichtiger Zeiger), `c-long` / `c-ulong`. Nur für das FFI:
  Um einen davon zu einem Wert zu machen, braucht es `(unsafe ...)`, und die Stellen, an denen sie vorkommen
  dürfen, sind begrenzt ([3.3 defffi](#ptr--c-long--c-ulong--rohe-maschinenwörter)). Man verwendet sie nicht,
  wo man eine 64-Bit-Ganzzahl will: Sie haben keine Arithmetik.
- **Undurchsichtige veränderliche Typen**: `random-state` (der Zustand eines Zufallszahlengenerators). Er kann
  nicht in `Vector<T>`/`HashTable<K,V>`/`Sexpr` (wohl aber in `Option<T>`/`Result<T,E>`).
- **Der Unit-Typ**: `()`
- **Der Never-Typ**: `!` (der Typ divergierender Ausdrücke wie `panic`/`unreachable`/`todo`/eine Schleife, die
  nie zurückkehrt. Er passt zu jedem erwarteten Typ)
- **Funktionstypen**: `(fn (Argumenttypen...) Rückgabetyp)`. Der Typ einer Funktion mit variadischen Argumenten
  ist `(fn (Argumenttypen... &rest Elementtyp) Rückgabetyp)`.
- **Generische Typen**: `Name<T1,T2,...>` (als einzelnes Token ohne Leerzeichen gelesen).
  Zum Beispiel `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  Auch der Unit-Typ `()` lässt sich als Typargument schreiben (`Result<(), FileError>`). `(`/`)` sind
  normalerweise Trennzeichen, die ein Token beenden, aber solange eine spitze Klammer offen ist, wird dieses
  eine Zeichenpaar durchgelassen. `()` lässt sich auch als Feld- oder Argumenttyp verwenden.
- **Die Anwendungsform generischer Typen**: `(Name T1 T2 ...)`, eine Listenschreibweise, die denselben Typ
  benennt wie `Name<T1,T2,...>`. Zum Beispiel ist `(vector char)` dasselbe wie `Vector<char>`.
  Die Namensform ist die übliche Schreibweise; diese Form **existiert für den Fall, dass sich ein Typargument
  nicht innerhalb eines Namens schreiben lässt**: Ein Typargument ist selbst ein Typausdruck, aber innerhalb
  eines Namens aus einem einzelnen Token lassen sich nur Namen, `()` und `:dyn` schreiben, keine Funktionstypen
  (eine Schreibweise wie `Vector<(fn (i32) i32)>` gibt es nicht). In dieser Form kann ein Typ auch erscheinen,
  wenn die Implementierung ihn anzeigt, etwa als Ergebnis des Einsetzens des assoziierten Typs eines Traits in
  eine Signatur.
- **Qualifizierte Typnamen**: lassen sich mit `::` qualifizieren, wie in `module::Type`.
- **Trait-Objekttypen**: `:dyn Trait` (zwei durch Leerzeichen getrennte Wörter, die einen Typ bilden). Stellt
  einen Wert dar, dessen konkreter Typ zur Laufzeit feststeht; Aufrufe von Trait-Methoden gehen über eine
  vtable (dynamischer Dispatch). Bei einem Trait mit assoziierten Typen werden diese positionell in
  Deklarationsreihenfolge festgelegt (`:dyn Iter<i32>` legt `Item` auf `i32` fest). Es lässt sich auch innerhalb
  generischer Argumente schreiben: `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`. Konkrete Werte
  werden an erwarteten Stellen automatisch in eine Box gesteckt; die ausdrückliche Form ist
  `(as :dyn Trait expr)`.
  Ein Wert von `:dyn Sub` lässt sich unverändert übergeben, wo ein `:dyn Super` eines seiner Obertraits (alles,
  was er transitiv erbt) verlangt wird (Upcast). An einen nicht verwandten Trait lässt er sich nicht übergeben.
  Die Bedingungen, die ein Trait für die Verwendung mit `:dyn` erfüllen muss, stehen in
  [3.9 deftrait / impl](#39-deftrait--impl--traits). `:dyn` außerhalb einer Typstelle zu schreiben, ist ein
  Fehler.
- Eingebaute generische Typen: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>` und die Nebenläufigkeitstypen `Task<T>` / `Thread<T>` / `Chan<T>`
  ([Kapitel 12](#12-nebenläufigkeit-tasks)). Außerdem gibt es `Sexpr`, den Typ von S-Ausdrucksdaten. Die
  eingebauten konkreten Fehlertypen sind `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` /
  `FileError` / `NetError`, und die Standardbibliothek hat die Strukturen `SimpleError` / `WrappedError`
  (`Error` ist kein Typ, sondern ein Trait: Man verwendet ihn als `:dyn Error`). Die Liste steht in
  [types.md](types.md).
- **Typen und Traits teilen sich einen Namensraum** (wie in Rust): Innerhalb eines Moduls können ein Typ
  (`defstruct`/`defenum`) und ein Trait (`deftrait`) nicht denselben Namen haben.

## 3. Definitionen auf oberster Ebene

### 3.1 defun — Funktionsdefinitionen

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Argumenttypen und Rückgabetyp sind Pflicht.
- Eine generische Funktion schreibt ihre Typparameter in spitzen Klammern nach ihrem Namen:
  `(defun name<T1,T2...> (params) Ret body...)` (dieselbe Spitzklammersyntax wie `Vector<T>` an Typstellen).
- `defun`/`lambda`/`defmethod` akzeptieren variadische Argumente, wenn am Ende `&rest (name Type)` steht:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (im Rumpf ist `xs` immer als `Option<Sexpr>`
  gebunden, eine S-Ausdrucksliste. Jedes tatsächliche Argument wird beim Aufruf einzeln als `Type2` typgeprüft
  und dann in ein `Sexpr` gehüllt).
  `defmacro` hat ebenfalls ein eigenes `&rest`, unterscheidet sich aber darin, dass es immer ein untypisiertes
  `Sexpr` ist (`defun`/`lambda` geben den Elementtyp an). Ein Funktionstyp kann auch eine variadische Funktion
  beschreiben, als `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (für `defun` und `defmethod`; nicht für `lambda`/`labels`, aus dem unten genannten
  Grund, und `defmacro` hat eine eigene Implementierung, ebenfalls unten). Die Reihenfolge ist die von CL:
  `required &optional &rest &key`. Jeder Parameter wird `(name Type)` oder `(name Type default-expr)`
  geschrieben:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; ohne Standardwert
    (match suffix ((some s) (append name s)) ((none) name)))         ; im Rumpf Option<string>

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; mit Standardwert
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; der Aufrufer schreibt `:name Wert` in beliebiger Reihenfolge; weggelassene nehmen ihre Standardwerte
  ```

  - **Ein Parameter ohne Standardausdruck hat den Typ `Option<Type>`.** Weggelassen ist er `none`; übergeben,
    wird der bloße Wert, den der Aufrufer geschrieben hat, automatisch in `some` gehüllt. Was CL mit einer
    supplied-p-Variable macht („wurde er übergeben?“), zeigt sich stattdessen auf der Seite des statischen
    Typs.
  - Mit Standardausdruck bleibt der Typ `Type` wie deklariert. Beim Weglassen wird dieser **geprüfte Ausdruck**
    unverändert in den Aufruf eingebettet (bei jedem Aufruf ausgewertet).
  - **`&key` lässt sich in einer Argumentliste nicht mit `&optional`/`&rest` mischen.** Damit wird eine
    Mehrdeutigkeit vermieden, die CL selbst hat (ob ein abschließendes tatsächliches Argument von einem
    positionellen `&optional` genommen oder als `&key` über die Marke zugeordnet wird, hängt von den *Werten*
    ab), indem die Kombination verboten wird. `&optional` und `&rest` lassen sich zusammen verwenden.
  - Sie lassen sich in generischen Funktionen verwenden, aber **ein Typparameter, der nur in weggelassenen
    Argumenten vorkommt, lässt sich nicht ableiten und ist ein Fehler** (es gibt keinen Wert, gegen den man
    abgleichen könnte).
  - **`defmethod` kann dieselben drei Abschnitte haben** (sowohl für Instanzmethoden als auch für statische
    Funktionen). Man führt `&optional`/`&rest`/`&key` nach dem Empfänger auf:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; statische Funktion
    (point::origin :y 7)
    ```

    Sie lassen sich auch in Methoden generischer Typen verwenden, aber **der Typ eines Parameters mit
    Standardausdruck kann die Typparameter des Besitzers nicht erwähnen** (dieselbe Einschränkung, die `defun`
    für seine eigenen Typparameter hat: Was beim Weglassen des Arguments eingebettet wird, ist ein *geprüfter*
    Ausdruck, daher kann sein Typ nicht als abstrakte Variable stehen bleiben).
  - **In Trait-Methoden lassen sie sich nicht verwenden.** `deftrait` hat keine Syntax dafür, und könnte nur
    die `impl`-Seite Abschnitte deklarieren, würden Aufrufe mit einem `:dyn`-Empfänger (die Argumente aus der
    Deklaration des Traits ergänzen) und Aufrufe mit einem konkreten Empfänger (die sie aus der Deklaration des
    `impl` ergänzen) zu verschiedenen Dingen. Die Stelligkeit eines vtable-Eintrags ist fest.
  - **In `lambda` / `labels` lassen sie sich nicht verwenden** (`&rest` schon). Um ein weggelassenes Argument zu
    ergänzen, muss der Aufrufer **den geprüften Standardausdruck des Aufgerufenen** lesen, der nur aus einer
    über den Namen aufgelösten Signatur verfügbar ist. Ein `lambda` wird als Wert herumgereicht, und das
    Einzige, was diesen Wert beschreibt, ist sein Funktionstyp `(fn ...)`: Darin gibt es keinen Platz für einen
    Ausdruck, und gäbe es ihn, würden „zwei lambdas mit derselben Signatur, aber verschiedenen Standardwerten“
    zu verschiedenen Typen. `&rest` bleibt innerhalb der Typfrage und lässt sich daher in einem Funktionstyp
    schreiben.
- **Vorwärtsverweise werden mit `defsignature` deklariert** (unten). Ein nicht deklarierter Name lässt sich vor
  seiner Definition nicht aufrufen, weil die oberste Ebene eine Form nach der anderen in Quelltextreihenfolge
  geprüft und ausgeführt wird.
- Um Trait-Schranken zu verlangen, schreibt man direkt vor den Rumpf eine `where`-Klausel:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (einen assoziierten Typ mit `(AssocName ConcreteType)` festzulegen, ist optional).
- **Docstrings**: Ein Zeichenkettenliteral am Anfang des Rumpfes, direkt nach der `where`-Klausel (falls
  vorhanden), wird zum Docstring (wie in CL). Allerdings nur, wenn mindestens eine Rumpfform folgt: Eine
  einzelne Zeichenkette bleibt der Rückgabewert und wird nicht als Docstring genommen:
  `(defun f () string "doc" "value")` hat einen Docstring und gibt `"value"` zurück, während
  `(defun f () string "value")` keinen Docstring hat und `"value"` zurückgibt. Abrufen lässt er sich mit
  `(documentation name)` ([Docstrings](functions/system.md#7-docstrings--documentation)).

### 3.2 defsignature — Vorwärtsdeklarationen

```lisp
(defsignature name (argument-types...) return-type)
(pub defsignature name (argument-types...) return-type)
```

Um ein `defun` aufzurufen, das **später** als man selbst definiert ist, deklariert man es zuerst so.
Wechselseitige Rekursion lässt sich nur so schreiben:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Die Argumente werden **nur als Typen** aufgeführt; es gibt keinen Rumpf, daher gibt es nichts, dem man Namen
geben müsste. `&rest` lässt sich als Letztes schreiben, als `&rest Elementtyp`.

Deklarationen **werden geprüft**:

- Die folgende Definition muss zur Deklaration passen (Anzahl und Typen der Argumente, Rückgabetyp, `&rest` und
  ob sie `pub` ist). Eine Abweichung ist ein Fehler an der Definition.
- Deklarieren ohne Definieren ist ein Fehler (gemeldet, wenn die Datei / das Modul fertig geladen ist). Die REPL
  meldet es nicht nach jeder Eingabe, denn eine Deklaration und ihre Definition sollen sich auf getrennten
  Zeilen eingeben lassen.
- Eine Deklaration, die **nach** der Definition steht, ist ein Fehler, da eine solche Deklaration nichts
  bewirken könnte.

Drei Dinge lassen sich nicht deklarieren:

- **Generische Funktionen.** Für jeden Typ eine Kopie zu erzeugen, braucht den Rumpf, und eine Deklaration hat
  keinen. Ein Vorwärtsaufruf ließe sich auflösen, aber die Instanziierung würde fehlschlagen, daher wird die
  Deklaration von vornherein abgelehnt.
- **`&optional`/`&key`.** Ihre Signatur enthält den **geprüften** Ausdruck jedes Standardwerts (beim Weglassen
  des Arguments in den Aufruf eingebettet), und eine Deklaration hat keinen Platz dafür.
- **Alles außer `defun`.** Ein `defmacro` braucht zum Expandieren einen Makrorumpf, der **bereits gelaufen**
  ist, was die Registrierung einer Signatur nicht ersetzen kann. Bei Typen (`defstruct`/`defenum`/`deftrait`)
  ist die Registrierung „das, was der Code braucht, der den Typ selbst registriert“, was nicht so in sich
  abgeschlossen ist wie eine Signatur. Ein `defmethod` wird an dem Typ registriert, dem es gehört, und folgt
  daher dem Typ.

CLs Gegenstück ist `(declaim (ftype (function (i32) bool) even2))`, aber das bringt ein ganzes
Deklarationssystem mit sich und ist nur **beratend**. Hier werden Deklarationen dank statischer Typisierung
geprüft.

### 3.3 defffi — C-Funktionen deklarieren (FFI)

```lisp
(defffi (name "c_symbol") (argument-types...) return-type)
(defffi (name "c_symbol") (argument-types...) return-type :library "name")
(defffi name (argument-types...) return-type)              ; name = der Symbolname in C
(pub defffi ...)
```

Deklariert eine C-Funktion, damit sie sich aufrufen lässt. Die Form ist dieselbe wie bei `defsignature` (ein
Name, Argumenttypen, ein Rückgabetyp und kein Rumpf), aber das Fehlen eines Rumpfes bedeutet etwas anderes.
`defsignature` ist ein Versprechen „ich definiere es später“, während `defffi` erklärt „jemand anders hat den
Rumpf bereits geschrieben und kompiliert“.

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

Der Name in typelisp und der Symbolname in C lassen sich getrennt schreiben, weil typelisp-Bezeichner meist `-`
enthalten und C-Bezeichner das nicht können. Wird der C-Name weggelassen, wird der Name unverändert als
Symbolname in C verwendet.

**Aufrufe verlangen `(unsafe ...)`** (auch bei Funktionen, die nur Skalare nehmen). Der Compiler kann nicht
bestätigen, dass die deklarierte C-Signatur zur wirklichen passt, und kann der Deklaration nur vertrauen;
`unsafe` ist das Zeichen, dass man diese Verantwortung übernimmt. Vorgesehen ist, es einmal zu umhüllen und
eine sichere Hülle zu bauen:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; ab hier kein unsafe nötig
```

Schreiben lassen sich die Typen `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void) `string`
`ptr` `c-long` `c-ulong` sowie typisierte Zeiger `(ptr T)`
([unten](#def-c-struct-und-typisierte-zeiger--c-structs-anlegen)).

`string` ist `const char *`. typelisp-Zeichenketten sind nicht NUL-terminiert und können selbst NUL enthalten,
daher **werden sie bei der Übergabe in eine C-Zeichenkette kopiert** und nach dem Aufruf freigegeben. Ein NUL in
der Zeichenkette ist ein Fehler: C würde nur bis dorthin schauen, daher würde stillschweigend eine andere
Zeichenkette übergeben.

**Auch zurückgegebene Zeichenketten werden kopiert** und nicht freigegeben: Was C zurückgibt, gehört C, und es
kann, wie bei `getenv`, in eine statische Tabelle zeigen. Funktionen, die Speicher zurückgeben, den der Aufrufer
freigeben muss (`strdup` usw.), sollte man als `ptr` entgegennehmen und selbst freigeben.

Auch Funktionen, deren Ergebnis in ein Argument zeigt (`strchr`, `strstr`), funktionieren korrekt: Das Ergebnis
wird kopiert, bevor das Argument freigegeben wird.

Gibt eine als `string`-zurückgebend deklarierte Funktion NULL zurück, ist das ein Fehler, denn `string` hat
keinen Wert, der „es gab keinen“ bedeutet. Ist NULL möglich, nimmt man das Ergebnis als `ptr` entgegen.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Mit `:library` wird diese gemeinsame Bibliothek geöffnet und das Symbol darin gesucht. Ohne wird das Symbol in
**dem Prozess selbst** gesucht (alles bereits Gelinkte, einschließlich libc). Ein kurzer Name wie `sqlite3`
wird der Reihe nach als `libsqlite3.dylib` / `libsqlite3.so` gesucht, und ein Name, der `/` enthält, wird als
Pfad behandelt. Geöffnete Bibliotheken werden nie geschlossen: Code, der auf ihre Funktionen zeigt, läuft
weiter, daher ist die einzig korrekte Lebensdauer die des Prozesses.

#### ptr / c-long / c-ulong — rohe Maschinenwörter

`ptr` ist ein undurchsichtiger Zeiger (`void *`, `FILE *`, was auch immer die Deklaration meinte). `c-long` /
`c-ulong` sind Cs `long` / `unsigned long` (auch `size_t`, `int64_t` und `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Dass sie nicht `i64` / `u64` heißen, ist Absicht.** Diese Sprache hat keinen 64-Bit-Ganzzahltyp, weil ein
markierter Direktwert nur 63 Bit hat ([Kapitel 2](#2-typen-schreiben)). Der Name `c-long` sagt „das ist ein
Wort, das die Grenze zu C überquert, keine Ganzzahl dieser Sprache“.

**Sie haben keine Arithmetik.** `(+ x 1)` lässt sich nicht schreiben. Man könnte sie bereitstellen, tut es aber
nicht, damit keine Berechnung auf einem Wert läuft, der sich nirgends speichern lässt und eine andere Breite
als jede andere Zahl hat, aus demselben Grund, aus dem der 64-Bit-Ganzzahltyp weggelassen wurde. Es gibt **nur
Umwandlungen**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; lesen, was zurückkam
(as int (unsafe (c-strlen s)))               ; damit liest man es exakt (int verliert keine 64 Bit)
(try-as i32 (unsafe (c-strlen s)))           ; fragen, ob es passt
(as c-ulong n)                               ; aus einer anderen Ganzzahl erzeugen
```

Ganzzahl**literale** nehmen den erwarteten Typ an, daher braucht man kein `as`, nur um eines zu übergeben:

```lisp
(unsafe (c-malloc 16))                       ; 16 wird als c-ulong gelesen
```

Literale außerhalb des Bereichs werden wie bei anderen Breiten abgelehnt (`(c-malloc -1)` passt nicht in ein
`c-ulong`).

**Die Stellen, an denen sie vorkommen dürfen, sind begrenzt**: nur Argumenttypen, Rückgabetypen und lokale
Variablen. Jedes der folgenden ist ein Fehler:

```lisp
(defstruct handle (p ptr))          ; ein Strukturfeld
(defenum maybe (none) (some ptr))   ; ein Enum-Feld
(defvar (block ptr) ...)            ; eine globale Variable
(defffi f ((vector ptr)) i32)       ; innerhalb eines Typarguments
```

Es gibt für alle einen Grund: **Der Platz markiert, was er hält**. Eine Markierung würde die obersten Bits des
Zeigers fallen lassen, derselbe Grund, aus dem der 64-Bit-Ganzzahltyp weggelassen wurde, daher ist es nicht
einmal in `unsafe` erlaubt. Das ist keine Frage der Erlaubnis: Diese Darstellung gibt es nicht.

Aus demselben Grund können sie keine lokalen Variablen sein, die von verschachtelten Funktionen **eingefangen**
werden (eine eingefangene Bindung kommt in eine Zelle, und eine Zelle markiert, was sie hält). Das ist zur
Kompilierzeit bekannt und wird von `(compile f)` gemeldet.

Der GC verfolgt `ptr` nicht. Er zeigt außerhalb des Heaps, daher ist das korrekt.

Vier Dinge lassen sich nicht deklarieren:

- **Variadische Argumente** (`printf`). Der variadische Teil wird nach anderen Regeln übergeben als die festen
  Argumente (auf AArch64 Darwin auf dem Stack), daher lässt er sich aus einer festen Signatur nicht korrekt
  aufrufen. `&rest` wird abgelehnt.
- **Structs per Wert übergeben oder zurückgeben.** Aus demselben Grund (es hängt von der Aufrufkonvention jeder
  Plattform ab). Die schreibbaren Typen sind auf die obige Liste beschränkt, daher lässt es sich nicht
  schreiben.
- **Generics.** C hat kein Gegenstück.
- **Derselbe Name wie eine eingebaute Funktion.** Ein kompilierter Aufruf würde diesen Namen zur eingebauten
  auflösen, daher wird es abgelehnt, statt stillschweigend falsch zu laufen.

#### Callbacks — C zurückrufen lassen

Schreibt man als Argumenttyp einen Funktionstyp `(fn (Typen...) Rückgabetyp)`, wird dieses Argument zu einer
Funktion, die C zurückruft (ein Callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; eine Funktion auf oberster Ebene
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; ein lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; eine lokale Funktion
```

Ein C-Funktionszeiger ist nichts als eine Codeadresse, und C ruft ihn nur mit den deklarierten Argumenten auf.
Es gibt keinen Ort, an dem eingefangene Variablen übergeben werden könnten, daher **lassen sich nur Funktionen
ohne freie Variablen übergeben**, und das wird bei der Typprüfung kontrolliert.

- Als tatsächliches Argument schreibt man **direkt** einen Funktionsnamen oder einen `lambda`-Ausdruck. Eine
  Variable, die eine Funktion enthält, lässt sich nicht übergeben: Welche Funktion sie enthält und damit, ob
  diese freie Variablen hat, ist erst zur Laufzeit bekannt.
- Ein `lambda` ist ein Fehler, wenn es sich auf lokale Variablen außerhalb seiner selbst bezieht. Auf globale
  Variablen und Funktionen auf oberster Ebene darf es sich beziehen.
- Eine lokale Funktion (`labels`) darf keine freien Variablen haben, einschließlich derer der
  Geschwisterfunktionen, die sie aufruft. Geschwisterfunktionen teilen sich den Ort, an dem eingefangene
  Variablen aufbewahrt werden, daher fängt diese Funktion auch ein, was eine aufgerufene Geschwisterfunktion
  einfängt.
- Eine generische Funktion bekommt ihre Typen aus dem deklarierten Funktionstyp.
- Die Typen, die sich im Funktionstyp schreiben lassen, sind dieselben wie in der obigen Liste. `string` kann
  jedoch nicht der Rückgabetyp eines Callbacks sein (es würde C Speicher übergeben, den niemand freigibt). Ein
  `string`-Argument kopiert die von C übergebene Zeichenkette in eine typelisp-Zeichenkette.

C-Funktionsaufrufe lassen sich nur innerhalb von `unsafe` schreiben, daher lassen sich Callbacks nur innerhalb
von `unsafe` übergeben.

**Der Callback kann nur aufgerufen werden, solange die C-Funktion läuft, die typelisp aufgerufen hat.** Wird er
von anderswo aufgerufen (einem Thread, der kein typelisp ausführt, einem Signalhandler, einer mit `atexit`
registrierten Funktion), gibt er den Grund aus und hält den Prozess an.

**Fehlschläge breiten sich nicht durch C aus.** Ein `panic` oder `throw` innerhalb des Callbacks kann nicht durch
C-Frames abwickeln (das wäre undefiniertes Verhalten), daher wird an C 0 zurückgegeben und der Fehlschlag an den
Aufrufer erneut geworfen, wenn die C-Funktion zurückkehrt. Wird der Callback zwischen dem Fehlschlag und der
Rückkehr der C-Funktion erneut aufgerufen, wird er nicht ausgeführt, und es wird 0 zurückgegeben.

Eine Operation, die innerhalb eines Callbacks warten müsste (ein `recv` auf einem leeren Kanal usw.), ist ein
Fehler ([12.6](#126-kompilierter-code-und-tasks)).

Wird eine Funktion neu definiert, wird ab der nächsten Übergabe an C die neue Definition aufgerufen.

Mit AOT (`compile-file`) funktioniert es genauso. Die Einsprungpunkte, die C aufruft, werden in das ausführbare
Programm eingebaut.

**Sie lassen sich nicht als Werte übergeben.** Eine FFI-Deklaration lässt sich nicht unverändert als `f` von
`(map f xs)` schreiben: Ein Funktionswert ist eine Closure, die den Rumpf einer Definition umhüllt, und diese
Deklaration hat keinen Rumpf zum Umhüllen. Man hüllt sie in ein `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

Auch `(disassemble c-abs)` wird abgelehnt: Was sich zeigen ließe, wäre Cs Maschinencode, den dieser Compiler
nicht erzeugt hat. `(compile c-abs)` gelingt (und tut nichts, da es bereits kompiliert ist).

**Es funktioniert auch mit AOT (`compile-file`).** Der Linker löst die C-Funktionen selbst auf. Hat eine
Deklaration `:library`, wird diese Bibliothek der Linkzeile als `-l` hinzugefügt (Duplikate werden zu einem
zusammengeführt), daher braucht `compile-file` keine zusätzlichen Argumente. `compile-file` liest den Quelltext
selbst und kann sie daher aus den Deklarationen einsammeln.

Symbole werden auch beim Bauen gesucht. Existiert eine deklarierte Funktion nicht, nennt der Fehler sie vor
jedem Linkfehler.

Die Standardbibliothek (das Prelude) verwendet kein `defffi`. Die Standardbibliothek geht vollständig in jedes
ausführbare Programm, daher würde eine Deklaration mit `:library` dort diese Bibliothek sogar in Programme
linken, die das FFI nicht verwenden.

#### def-c-struct und typisierte Zeiger — C-Structs anlegen

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Deklariert ein Struct mit demselben Layout wie in C. Es lässt sich nur innerhalb eines `unsafe` auf oberster
Ebene schreiben (das nichts als `def-c-struct`s enthalten darf). Direkt nach dem Namen kann ein Docstring
stehen.

Für Felder lassen sich die Typen `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32` `f64` `bool`
`ptr`, typisierte Zeiger `(ptr T)` und andere `def-c-struct`s (per Wert eingebettet) schreiben. Das Layout (der
Versatz jedes Feldes sowie Größe und Ausrichtung des Structs) wird nach den Regeln von C berechnet (unter
Annahme von LP64). Ein Feld, das auf das Struct selbst zeigt, lässt sich schreiben, aber das Struct kann sich
nicht selbst einbetten.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x bei 0, y bei 8, Größe 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

Der Name eines `def-c-struct` kommt in den Typnamensraum (im selben Modul kann kein gleichnamiges `defstruct`
oder Ähnliches stehen), aber **er ist nicht der Typ eines Wertes**. Man kann nicht `(defun f ((p point)) ...)`
schreiben; er erscheint nur als das, worauf ein typisierter Zeiger zeigt.

**Ein typisierter Zeiger `(ptr T)`** ist eine Adresse, die auf ein `T` zeigt. `T` ist einer der oben für Felder
schreibbaren Typen. Er ist ein rohes Maschinenwort wie `ptr`, mit denselben Regeln, wo er vorkommen darf (nur
Argumente, Rückgabetypen und lokale Variablen; nur innerhalb von `unsafe` kann er ein Wert sein).

Anlegen, Lesen und Schreiben schreibt man in den folgenden Formen. Alle lassen sich nur innerhalb von `unsafe`
verwenden.

| Form | Bedeutung |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Legt `n` Werte von `T` an (1, wenn weggelassen). Der Inhalt wird mit 0 gefüllt. Gibt einen `(ptr T)` zurück |
| `(c-ref p i)` | Ein Zeiger auf Element `i` ab `p`. Ein Fehler, wenn außerhalb des angelegten Bereichs |
| `(c-deref p)` / `(setf (c-deref p) v)` | Liest / schreibt den Skalar, auf den `p` zeigt |
| `p::field` / `(setf p::field v)` | Liest / schreibt ein Feld eines Structs. Das Lesen eines Feldes, das ein eingebettetes Struct ist, liefert dessen Adresse (`(ptr inner-type)`) |
| `(as ptr p)` | Vergisst den Typ und macht einen `ptr` daraus (um ihn an etwas wie das `void *` von `qsort` zu übergeben). Eine Umwandlung zurück gibt es nicht |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**Angelegter Speicher wird freigegeben, wenn die Ausführung das `unsafe` verlässt, das ihn angelegt hat.** Der
Besitzer ist das lexikalisch äußerste `unsafe` innerhalb derselben Funktion. Er wird freigegeben, ob der Code
normal endet oder durch `panic`, `throw` oder `return-from` verlassen wird. `lambda`- und `labels`-Funktionen
sind eigene Funktionen, daher braucht ein `c-alloc` in ihnen ein eigenes `unsafe` in ihnen.

Deshalb kann ein typisierter Zeiger das `unsafe`, das ihn angelegt hat, nicht verlassen. Jedes der folgenden ist
ein Typfehler:

- Ihn zum Wert des `unsafe`-Ausdrucks zu machen (daher lässt er sich auch nicht aus einer Funktion zurückgeben)
- Ihn in einer Closure einzufangen (`lambda`, `labels`)
- Ihn an `task` / `thread` zu übergeben
- Ihn mit `throw` zu werfen

Um Werte außerhalb des `unsafe` zu verwenden, kopiert man sie innerhalb des `unsafe` in ein `defstruct` oder in
Zahlen und gibt diese zurück.

**Auf der C-Seite angelegter Speicher wird nicht behandelt.** Werte, die von C als typisierte Zeiger
hereinkommen (Rückgabewerte von `defffi`, Callback-Argumente, aus zeigertypisierten Feldern gelesene Werte),
werden zur Laufzeit daraufhin geprüft, ob sie innerhalb einer lebenden `c-alloc`-Anlage auf einen Wert dieses
Typs zeigen, und sind sonst ein Fehler. Auch NULL ist ein Fehler. Um Speicher entgegenzunehmen, den C angelegt
hat, oder NULL, verwendet man den untypisierten `ptr` (dessen Inhalt sich nicht lesen lässt).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Wird das Argument eines Callbacks von der Prüfung abgelehnt, wird es dem Aufrufer gemeldet, wenn die
C-Funktion zurückkehrt, genau wie ein Fehlschlag innerhalb eines Callbacks.

### 3.4 defvar / defparameter / defconstant — globale Variablen

```lisp
(defvar (name Type) init-expr)        ; initialisiert nur, wenn noch nicht gebunden
(defparameter (name Type) init-expr)  ; weist jedes Mal zu
(defconstant (name Type) init-expr)

; mit Docstring (in derselben Reihenfolge wie CLs defvar/defparameter/defconstant: nach dem Wert)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**Der Unterschied zwischen `defvar` und `defparameter` zeigt sich beim erneuten Laden** (wie in CL). Ist die
globale Variable **bereits gebunden, wertet `defvar` nicht einmal den Initialisierer aus**; bearbeitet man eine
Einstellungsdatei und liest sie erneut, bleiben die Werte, die die Sitzung geändert hat, daher erhalten.
`defparameter` weist jedes Mal zu, daher bringt erneutes Lesen die Werte auf das zurück, was geschrieben steht.

Die Typannotation ist Pflicht (sie wird nicht aus dem Initialisierer abgeleitet). `defvar` lässt sich ändern;
`defconstant` nicht (`setf` ist ein Fehler).

### 3.5 defmethod — Methodendefinitionen

```lisp
; Instanzmethode: aufrufbar als (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; statische / assoziierte Funktion: aufrufbar als (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

Der Aufrufer löst die Methode über den statischen Typ von `obj` auf (einfacher, statischer Dispatch). Ein
Docstring kann an derselben Position und nach denselben Regeln wie bei `defun` stehen (direkt nach der
`where`-Klausel, am Anfang des Rumpfes, nur wenn Rumpfformen folgen). Dasselbe gilt für Methoden innerhalb von
`impl`; abgerufen werden sie mit `(documentation Type::method)`.

### 3.6 defstruct — Strukturen (benutzerdefinierte Typen)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; generisch (Typparameter in spitzen Klammern)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Jedes Feld ist `(name type)` oder `(pub name type)` (Sichtbarkeit pro Feld, unabhängig vom `pub` der Struktur
  selbst). Ein weiterer Ausdruck am Ende wird zum **Standardwert** des Slots (`(x i32 0)`); siehe die
  Optionsliste unten.
- Folgendes wird automatisch erzeugt:
  - Der Konstruktor `Name::new` (Argumente in Feldreihenfolge)
  - Getter `(field-name instance)`, mit dem Zucker `instance::field-name`
  - Setter `(set-field-name instance value)`, mit dem Zucker `(setf instance::field-name value)`
- Um die Struktur selbst `pub` zu machen, stellt man `pub` voran, wie in `(pub defstruct ...)`.
- **Einen Typ definiert man, bevor man ihn nennt.** Der Typ eines Feldes kann die Struktur selbst sein
  (`(next Option<node>)`), aber kein später definierter Typ: Typen haben keine Vorwärtsdeklaration, die
  `defsignature` entspräche. Ein noch nicht definierter Name ergibt in einem `defun`-Argumenttyp oder in `the`
  denselben Fehler `unknown type`. Zwei Typen, die sich gegenseitig verweisen, lassen sich daher nicht
  schreiben.
- **Typvariablen sind nur die an deklarierenden Stellen geschriebenen.** Bei `defun`/`defstruct`/`defenum`/
  `deftype` das `<T>` des Namens; bei `defmethod` der Typ des Empfängers (`(self box<T>)` oder `box<T>` bei
  einer statischen Methode); bei `impl` der Zieltyp und `impl<T>`; bei `deftrait` `Self` und die assoziierten
  Typen aus `(type Item)`. Ein Name, der anderswo zum ersten Mal vorkommt (Argumente, Rückgabewert,
  `the`/`lambda` im Rumpf), wird keine Typvariable; er ist `unknown type`.
- **Docstrings**: Ein Zeichenkettenliteral direkt nach dem Namen, vor den Feldern, wird zum Docstring
  (`(defstruct Name "doc" (field Type)...)`, dieselbe Position wie bei CLs `defstruct`). Ein Feld hat immer die
  Form `(name Type ...)` und kann nie eine bloße Zeichenkette sein, daher gibt es keine Mehrdeutigkeit.
  Abgerufen wird er mit `(documentation Name)`.

#### Optionsliste

Schreibt man an die Namensposition eine Liste `(Name option...)`, gibt man Optionen an (dieselbe Position wie in
CL).

```lisp
(defstruct (point (:constructor make-point)          ; Schlüsselwort-Konstruktor
                  (:constructor at (x &optional y))  ; BOA-Konstruktor
                  (:copier copy-point))
  (x i32 0)          ; ein drittes Element ist der Standardwert dieses Slots
  (y i32 0))

(point::make-point :y 7)   ; x ist 0
(point::at 1)              ; y ist 0
(point::at 1 2)
(copy-point p)             ; eine flache Kopie (dasselbe wie CLs copier)
```

- **`:constructor`**: Erzeugt wird eine **statische Funktion** des Typs (`point::make-point`), deren Rumpf
  immer `(point::new ...)` ist. `new` bleibt der eine strukturelle Konstruktor; was hier entsteht, ist eine
  *Art, ihn aufzurufen*. Mehrere lassen sich deklarieren.
  - `(:constructor name)` nimmt jeden Slot als `&key`. **Jeder Slot braucht einen Standardwert** (diese Sprache
    hat nichts, was CLs „ungebundenem Slot“ entspräche).
  - `(:constructor name (slot...))` nimmt die genannten Slots als positionelle Argumente (in beliebiger
    Reihenfolge). Nicht genannte Slots werden mit ihren Standardwerten gefüllt, daher **brauchen sie
    Standardwerte**. Nach `&optional` darf der Rest weggelassen werden (und braucht ebenso Standardwerte).
- **`:copier`**: Erzeugt eine **Instanzmethode**, die einen neuen Wert mit denselben Slotwerten zurückgibt.
  Flach, wie CLs copier.
- **`:include Parent`**: Stellt die Slots des Elterntyps voran (auch Standardwerte werden geerbt; der Elterntyp
  darf in einer anderen Datei stehen). **Es entsteht keine Typbeziehung**: Das Kind ist kein Untertyp des
  Elterntyps, die Methoden des Elterntyps gelten nicht für das Kind, und es gibt keine Laufzeitprüfung, die
  beide verbindet. Diese Sprache hat keine Untertypen; gemeinsame Schnittstellen sind die Aufgabe von
  `deftrait`. Nur die *Liste* der Slots wird zusammengefügt.
- **Slot-Standardwerte werden nur von erzeugten Konstruktoren gelesen.** Einen Standardwert zu schreiben, ohne
  ein `:constructor` zu deklarieren, ist ein Fehler, da er nie verwendet werden könnte.
- Weggelassene Optionen und warum:
  - **`:conc-name`**: In CL stellt es Zugriffsfunktionen ein Präfix voran, um Kollisionen in einem einzigen
    flachen Funktionsnamensraum zu vermeiden. Hier sind Zugriffsfunktionen Methoden, die nach dem Typ des
    Empfängers gewählt werden, daher kommt es nicht zu Kollisionen, und ein Präfix würde `instance::field`
    zerstören (das nur den Slotnamen kennt).
  - **`:predicate`**: Beantwortet zur Laufzeit „ist dieser Wert ein `point`?“. Hier sind Typen eine
    Klassifikation zur Kompilierzeit ohne Zeugen zur Laufzeit, und es gibt keine Stelle, an der „ein Wert
    unbekannten Typs, der ein point sein könnte“ existiert (`match` auf `Sexpr` ist abgeschlossen, und `:dyn`
    lässt sich nicht herabcasten), daher könnte ein erzeugtes Prädikat immer nur `true` zurückgeben.
  - **`:type` / `:initial-offset` / `:named`**: Diese ersetzen die Darstellung des Wertes durch eine Liste oder
    einen Vektor. Die Darstellung gehört dem Compiler und lässt sich aus der Sprache nicht beobachten.

### 3.7 defenum — Aufzählungen (Summentypen)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; eine Variante mit Nutzlast (positionelle Felder)
  (Variant2)                  ; eine Variante ohne Nutzlast
  ...)

; generisch
(defenum Option<T>
  (Some T)
  (None))
```

- Jede Variante hat die Form `(VariantName FieldType...)`. Felder sind nur positionell (sie haben keine Namen).
  Mindestens eine Variante ist nötig, und Namen dürfen sich nicht wiederholen.
- Werte werden wie bei den eingebauten `Option`/`Result` qualifiziert oder über `use` gebildet:
  `(Name::Variant1 a b)` oder nach `(use Name)` `(Variant1 a b)`.
- Sie lassen sich mit `match` / `if-let` zerlegen. `match` prüft die Vollständigkeit (es muss jede Variante
  abdecken oder ein `_` haben):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Methoden und assoziierte Funktionen werden wie bei `defstruct` nachträglich mit `defmethod`/`impl`
  hinzugefügt.
- Um das Enum selbst `pub` zu machen, schreibt man `(pub defenum ...)`.
- **Docstrings**: dieselbe Position und dieselben Regeln wie bei `defstruct`, direkt nach dem Namen, vor den
  Varianten (`(defenum Name "doc" (Variant ...)...)`). Abgerufen wird er mit `(documentation Name)`.

### 3.8 deftype — Typaliase

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

CLs `deftype`, eingeengt auf das, was in einer statisch typisierten Sprache sinnvoll ist: **eine Schreibweise
eines Typs, kein Typ**.

- Die Namensposition ist dieselbe wie bei `defun`, und generische Argumente schreibt man `Name<T,U>`. An der
  Verwendungsstelle ist genau die deklarierte Anzahl von Typargumenten nötig (zu viele oder zu wenige ist dort
  ein Fehler).
- Die Expansion geschieht **innerhalb des Typparsers**. Daher weiß nichts danach, dass der Alias existiert: Die
  Schlüssel der Monomorphisierung, Dumps, der Kompilierweg und **Fehlermeldungen** zeigen alle die expandierte
  Form. Scheitert `(f "x")` an einer Funktion, die `meters` verlangt, sagt die Meldung `i32`.
- **Es ist kein neuer Typ.** `(deftype meters i32)` macht `meters` und `i32` zum selben Typ, daher wird ein
  Verwechseln nicht erkannt. Will man sie getrennt halten, verwendet man `defstruct`.
- **Es ist kein Prädikat.** CLs `(deftype small () '(integer 0 9))` beschreibt eine *Menge von Werten*, die
  `typep` zur Laufzeit prüft, aber hier sind Typen eine Klassifikation zur Kompilierzeit ohne Zeugen zur
  Laufzeit, daher hätte ein Alias, der Werte einschränkt, nichts einzuschränken.
- **Es kann sich nicht selbst enthalten.** Ein Alias wird dort expandiert, wo er geschrieben steht, daher gibt es
  keinen Ort, an den er rekursieren könnte. Rekursive Datentypen schreibt man mit `defstruct`/`defenum`.
- Er teilt sich den Namensraum mit Typen und Traits (innerhalb eines Moduls kann er nicht denselben Namen haben
  wie ein `defstruct`/`defenum`/`deftrait`). Öffentlich macht man ihn mit `(pub deftype ...)`, hereingeholt wird
  er mit `(use m::meters)`.
- **Docstrings**: direkt nach dem Namen, vor dem Typ (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — Traits

```lisp
(deftrait TraitName (SuperTrait...)      ; die Liste der Obertraits ist Pflicht; () wenn keine
  (type AssocName)                       ; assoziierte Typen (beliebig viele, optional)
  (method-name ((self Self) params...) RetType)          ; ohne Rumpf = muss implementiert werden
  (method-name ((self Self) params...) RetType body...)) ; mit Rumpf = Standardimplementierung

(impl TraitName TargetType
  (where (Trait A)...)                   ; Schranken für das ganze impl (optional)
  (type AssocName ConcreteType)          ; macht einen assoziierten Typ konkret
  (method-name (recv params...) RetType body...))
```

Über `impl` wird jede Methode als gewöhnliches `defmethod` von `TargetType` registriert. Auf Traits wird als
Trait-Schranken in den `where`-Klauseln generischer Funktionen verwiesen (siehe
[3.1 defun](#31-defun--funktionsdefinitionen)). Ein Trait-Name kann auch ein `::`-Pfad wie `m::Trait` sein.

**Die Liste der Obertraits (Pflicht)**: steht immer direkt nach dem Trait-Namen. Jedes Element ist ein bloßer
Trait-Name oder, wenn dieser Trait assoziierte Typen hat, `(Trait (Assoc Type))` mit **allen seinen
assoziierten Typen festgelegt**.

```lisp
(deftrait Eq () ...)                       ; keine Obertraits
(deftrait Ord (Eq) ...)                    ; Rusts trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; einen assoziierten Typ festlegen
  (rewind ((self Self)) ()))
```

Vererbung hat drei Wirkungen. (1) `impl Ord X` verlangt, dass `impl Eq X` **vorher** geschrieben ist (eine Regel
über die Schreibreihenfolge: die einzige Form, die sich in der REPL und bei schrittweisem `load`
deterministisch entscheiden lässt, und strenger als Rust). (2) `(where (Ord T))` allein erlaubt auch den Aufruf
der Methoden von `Eq`. (3) Die Methoden von `Eq` lassen sich über ein `:dyn Ord` aufrufen, und ein
`:dyn Ord`-Wert lässt sich unverändert übergeben, wo ein `:dyn Eq` verlangt wird (Upcast). Dass ein Untertrait
eine gleichnamige Methode seines Elterntraits neu deklariert und dass gleichnamige Methoden von zwei
Elterntraits geerbt werden, sind beides Fehler (eine vtable hat einen Eintrag pro Name). Rautenvererbung wird zu
einem Eintrag zusammengeführt.

**Standardimplementierungen**: Ein Rumpf nach der Signatur wird verwendet, wenn ein `impl` die Methode
weglässt. Der Rumpf wird im **Namensraum des Moduls** aufgelöst, in dem der Trait steht, daher kann er nicht
öffentliche Funktionen dieses Moduls aufrufen. Methoden mit Rumpf können auch `where`-Klauseln und Docstrings
haben. Der Rumpf wird wie in Rust **einmal, an der Deklarationsstelle**, typgeprüft, mit `Self` als Typvariable
(beschränkt durch `Self: der Trait selbst`): Fehler, die für jedes `impl` und jeden implementierenden Typ
scheitern würden, werden dort gefunden, auch in Standardimplementierungen, die kein `impl` je weglässt.
Aufrufe auf `self` von Methoden des Traits selbst oder seiner Obertraits gehen durch diese Schranke, und
assoziierte Typen sind auf sich selbst festgelegt, daher wird eine Signatur, die `Item` zurückgibt, gegen den
Rumpf abgeglichen, ohne den konkreten Typ zu kennen.

**Pauschalimplementierungen**: Macht man das Ziel zu einer Typvariable, wird der Trait auf einen Schlag für jeden
Typ implementiert, der die Schranken erfüllt.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; überhaupt kein Rumpf; alles ist die Standardimplementierung
```

**Es wird kein Code erzeugt, bis ein konkreter Typ ihn tatsächlich verwendet** (einmal pro Typ, nach demselben
Mechanismus wie die gewöhnliche Monomorphisierung). Ein Trait kann höchstens eine Pauschalimplementierung haben.
Hat ein Typ ein ausdrückliches `impl`, hat dieses Vorrang. Die Typprüfung des Rumpfes ist von der Erzeugung
getrennt: Sie geschieht wie in Rust einmal an der Deklarationsstelle, **mit dem Ziel als Typvariable**, daher
werden selbst bei einer Implementierung, die nie verwendet wird, Fehler dort gefunden, wenn sie unter den
deklarierten Schranken für jedes Ziel scheitern würden. Durch die Schranken gerechtfertigte Aufrufe
(`(less self other)` unter `(where (Ord T))` usw.) gehen durch, wie im Rumpf eines generischen `defun`.

**Docstrings**: Ein `deftrait` kann einen Docstring für den ganzen Trait haben, als Zeichenkettenliteral direkt
nach der Liste der Obertraits, vor den Einträgen (`(deftrait Name () "doc" (type ...) (method ...)...)`). Eine
Signatur ohne Rumpf kann keinen Docstring haben: Eine abschließende Zeichenkette wäre selbst der Rückgabewert
einer Standardimplementierung, daher ließen sich beide nicht unterscheiden.

Die Traits, die die Standardbibliothek bereitstellt: **`Iter`** (`next` / assoziierter Typ `Item`; die Grundlage
von `doiter` und den Sequenzfunktionen), **`Eq`** (`equals`; `not-equals` ist eine Standardimplementierung),
**`Ord`** (erbt `Eq`; nur `less` muss implementiert werden, und `less-equal` / `greater` / `greater-equal` sind
Standardimplementierungen), **`Error`** (`message` / `source`; `:dyn Error`, um Fehlertypen einheitlich zu
behandeln), **`print-object`** (eine typabhängige Druckdarstellung), **`Pathish`** (Pfadnamensbezeichner: eine
Zeichenkette oder ein `pathname`) sowie die Stream-Hierarchie **`Stream`** → **`InputStream`** /
**`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**. Welche Typen welche Traits
implementieren, steht in [types.md](types.md); die Methoden jedes Traits stehen unter
[Standard-Traits](functions/traits.md), [Fehlertypen](functions/option-result.md#3-fehlertypen-und-der-trait-error),
[print-object](functions/printing.md#5-print-object-typabhängige-druckdarstellung) und
[Streams](functions/streams-files.md). Implementiert man `Iter` für einen eigenen Sammlungstyp, funktionieren
`doiter` (Kapitel 5) und `map` / `filter` / `sort` und Ähnliches unverändert darauf.

Trait-Aufrufe sind standardmäßig **statisch** (über den statischen Typ des Empfängers aufgelöst). Um Werte zu
behandeln, deren konkreter Typ zur Laufzeit feststeht, liefert der Trait-Objekttyp `:dyn Trait` (Kapitel 2)
dynamischen Dispatch über eine vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; eine Aufrufstelle, eine Antwort pro Implementierung
```

Nur Traits, bei denen „jede Methode einen `self`-Empfänger hat, `Self` nirgends außer im Empfänger verwendet und
selbst weder generisch noch variadisch ist“, lassen sich zu `:dyn` machen (geerbte Methoden müssen dieselben
Bedingungen erfüllen).

Nur Typen, deren Werte eine Darstellung auf dem Heap haben, können in eine `:dyn`-Box:

| Kann hinein | Kann nicht hinein |
|---|---|
| `defstruct`/`defenum`-Typen (einschließlich `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` und der Strukturen der Standardbibliothek), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Ganzzahlen fester Breite (`i8` bis `u32`), `f32`, `bool`, `char`, `symbol`, `()`, Funktionstypen und `Option<T>` ohne Box ([die Laufzeitdarstellung von Option](functions/option-result.md#2-die-laufzeitdarstellung-von-optiont)) |

Einen Wert eines Typs, der nicht hinein kann, dort zu platzieren, wo ein `:dyn` erwartet wird, ist ein
Typfehler. Um solche Werte über `:dyn` zu behandeln, hüllt man sie in eine Struktur, wie in
`(defstruct flag (v bool))`.

### 3.10 module / use — Namensräume

```lisp
(module path body...)      ; path ist eine Folge von Segmenten wie foo oder foo::bar
(in-module path)           ; ab hier bis zum Ende dieser Einheit innerhalb von path (die flache Form von module)
(use path...)              ; Funktionen, Typen und Module als Alias in den aktuellen Namensraum holen
(import path...)           ; dasselbe wie use (eine CL-kompatible Schreibweise)
(shadowing-import path...) ; ein use, das bewusst einen bereits verwendeten bloßen Namen nimmt
```

- `module` erzeugt einen Namensraum. **Typen sind keine Namensräume** (wie in Rust hat ein Typ nur assoziierte
  Funktionen und Methoden).
- Holt man einen Typ mit `use` herein, sind seine Konstruktoren und öffentlichen statischen Methoden auch über
  den bloßen Namen verfügbar (zum Beispiel lassen sich nach `(use option)` `some`/`none` ohne
  `option::some`/`option::none` aufrufen).
- Die Auflösungsreihenfolge bloßer Namen (nicht qualifizierter Bezeichner): Spezialformen → Konstruktoren →
  freie Funktionen (aktueller Namensraum → Wurzel) → Instanzmethoden (aufgelöst über den statischen Typ des
  ersten Arguments). Sie geht nicht durch dazwischenliegende Elternmodule hinauf.
- Ein qualifizierter Pfad `a::b` löst `a` in der obigen Reihenfolge auf; ist es ein Modul, geht er hinein, ist
  es ein Typ, wird das letzte Segment als assoziiertes Element aufgelöst.
- **`use` wirkt auf die Formen danach.** Eine Datei wird eine Form nach der anderen gelesen, und Abhängigkeiten
  werden direkt vor der Prüfung der Form aufgelöst; schreibt man `m::f` **oberhalb** von `(use m)`, ergibt das
  daher `unresolved path`. `use` setzt man an den Anfang der Datei.
- **`use` kann mehrere Pfade nehmen** (`(use a::f b::g)`). `import` ist eine CL-kompatible Schreibweise mit
  demselben Verhalten.
- **Ein `use`, dessen bloßer Name bereits belegt ist, wird gemeldet.** Die Auflösung eines bloßen Namens schaut
  vor den Aliassen auf die eigenen Definitionen des Moduls, daher **tut** `(use m::twice)` nach
  `(defun twice ...)` **nichts**. Ist es so gemeint, schreibt man `shadowing-import` (gegen eine Definition kommt
  es trotzdem nicht an, da es keine Möglichkeit gibt, eine zu entfernen; es schlägt nur frühere Aliasse).
- **`in-module` ist die flache Form von `(module path body...)`.** Schreibt man `(in-module geometry)`, kommt
  alles von dort bis zum Ende der Einheit (der Datei oder des Rumpfes des umschließenden `module`) in
  `geometry`. Es geht **innerhalb** des eigenen Moduls der Datei (`main::geometry` bei `main.typl`). Zwei
  hintereinander verschachteln sich der Reihe nach. Es unterscheidet sich von CLs `in-package` und heißt
  daher anders: In diesem System ist die Datei bereits ein Modul, daher gibt es nichts „auszuwählen“, und alles,
  was eine Form tun kann, ist verschachteln.

### 3.11 Dateien und Module (Projekte mit mehreren Dateien)

Der Dateipfad relativ zur Quellwurzel ist der Modulpfad:
Der Inhalt von `<root>/geo/point.typl` wird implizit in das Modul `geo::point` gehüllt (auch ein Verzeichnis ist
ein Segment, im Stil von Rust / Python). Ein ausdrückliches `(module bar ...)` in der Datei verschachtelt sich
**darin** (`geo::point::bar`), daher kollidieren der abgeleitete Pfad und eine ausdrückliche Deklaration nie.

- **Quellwurzel**: Man legt eine Manifestdatei `typelisp.toml` in die Wurzel des Projekts (sie kann leer sein;
  optional benennt eine Zeile `src = "src"` das Quellverzeichnis). Sie wird gefunden, indem man vom Verzeichnis
  der Zieldatei aus nach oben geht. Ohne Manifest ist das Verzeichnis der Einstiegsdatei (bei der REPL das
  aktuelle Verzeichnis) die Wurzel.
- **Laden bei Bedarf**: Verweist `(use geo::point)` auf ein noch nicht geladenes Modul, wird die passende Datei
  (`geo/point.typl`) automatisch geladen, typgeprüft und registriert. `use a::b::c` sucht zuerst das längste
  Präfix: `a/b/c.typl` → `a/b.typl` → `a.typl` (da `c` ein Element innerhalb eines Moduls sein kann). Aus
  anderen Modulen sichtbare Definitionen brauchen `pub` ([3.13 pub](#313-pub--sichtbarkeit)).
- **Zirkuläre Verweise sind Fehler**: Die Kette wird in der Form `circular module dependency: a -> b -> a`
  gemeldet.
- **Ausführen**: `typl <file.typl>` führt eine Datei aus (ohne Argumente die REPL). `use` in der REPL löst
  Dateien nach denselben Regeln auf.
- **Kapazität der cons-Arena**: `typl --heap-cells N` setzt die **Anfangskapazität** der Arena für cons-Zellen
  (Standard 65536; auch die Form `--heap-cells=N` funktioniert, sowohl beim Ausführen von Dateien als auch in
  der REPL). Die Arena **wächst durch Hinzufügen**, wenn sie knapp wird. Die Grenze des Wachstums ist das
  256-Fache der Anfangskapazität, und eine Anforderung darüber hinaus ergibt `heap exhausted`: Die
  Anfangskapazität bedeutet „zu Beginn so viel anlegen“, und die Grenze bedeutet „darüber hinaus als Leck
  behandeln“.

### 3.12 load — flaches Laden

```lisp
(load "path")   ; nur auf oberster Ebene; path ist ein Zeichenkettenliteral
```

- **Flaches Laden** im Stil von CL: liest die Formen der Zieldatei unverändert **in den aktuellen Namensraum**
  (ohne sie anders als `use` in ein Modul zu hüllen). Nur auf oberster Ebene (innerhalb eines Funktionsrumpfes
  ist es ein Typfehler).
- `path` ist relativ zum Verzeichnis der ladenden Datei (aus der REPL zum Arbeitsverzeichnis des Prozesses). Hat
  es keine Endung, wird `.typl` angehängt.
- `(load ...)`/`(use ...)` in der geladenen Datei werden ebenfalls rekursiv verarbeitet.
- **Es liest eine Form nach der anderen und führt sie an Ort und Stelle aus** (wie CLs `load`). Form *k* ist
  fertig ausgeführt, bevor *k+1* gelesen wird: Selbst wenn mittendrin ein Syntax- oder Typfehler auftritt, sind
  die Formen davor bereits gelaufen. Von `use` geladene Moduldateien sind anders: Sie werden als eine Einheit
  geprüft, und ihre Ausführung bleibt dem überlassen, der sie mit `use` hereingeholt hat (entspricht CLs
  `compile-file`).

### 3.13 pub — Sichtbarkeit

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` lässt sich nur an die elf obigen Arten setzen (nicht an `module`/`use`/`deftrait`/`impl`). Man schreibt
das Definitionsschlüsselwort direkt nach `pub`, nicht in der Form `(pub (defun ...))`, die die Definition in
Klammern hüllt. Ein `pub` macht genau eine Definition öffentlich (mehrere Definitionen lassen sich nicht auf
einmal markieren).

### 3.14 defmacro — Makrodefinitionen

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- Alle Parameter und der Rückgabewert sind immer `Sexpr`, daher schreibt man keine Typannotationen.
- Nicht hygienische Makros im Stil von CL (Kollisionen mit `gensym` zu vermeiden, ist Sache des Makroautors).
- Die Lambda-Liste folgt CLs Reihenfolge `required &optional &rest &key` (jede Markierung höchstens einmal und
  nur in dieser Reihenfolge).
  - `&optional` … optionale Argumente. `name` oder `(name default-expr)`. Der Standardausdruck wird zur
    Expansionszeit ausgewertet (er kann sich auf früher gebundene Parameter beziehen) und gebunden, wenn das
    Argument weggelassen wird (ohne Standardwert die leere Liste `()`).
  - `&rest name` … nimmt die übrigen positionellen Argumente zusammen als eine `Sexpr`-Liste entgegen.
  - `&key` … Schlüsselwortargumente. `name` oder `(name default-expr)`. Der Aufrufer übergibt sie als
    `:name value` (in beliebiger Reihenfolge). Wird eines weggelassen, der Standardausdruck (ohne einen die
    leere Liste `()`). Unbekannte Schlüsselwörter oder eine `:key`-Folge ungerader Länge sind Fehler.
- Beispiele: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — lokale Makrobindungen

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; lexikalisch gültige Makros
(symbol-macrolet ((name expansion) ...) body...)         ; ein Name steht für eine Form
```

Beide sind Spezialformen für **Ausdrücke**, und zur Laufzeit bleibt nichts übrig (kompiliert wird die
expandierte Form des Rumpfes). Die Lambda-Liste ist dieselbe wie bei `defmacro`. Die genauen Regeln und
Beispiele stehen unter
[Lokale Makrobindungen](functions/system.md#9-lokale-makrobindungen-macrolet--symbol-macrolet).

## 4. Bindung und Verzweigung

```lisp
(let ((name val) ...) body...)      ; parallele Bindung
(let* ((name val) ...) body...)     ; sequenzielle Bindung (frühere Bindungen in späteren Initialisierern verwendbar)

(if cond then else)                 ; else ist Pflicht (immer drei Elemente)
(when cond body...)                 ; ein if ohne else (Unit-Typ). defmacro
(unless cond body...)               ; die Negation von when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; eine Liste von Schlüsseln: passt, wenn einer davon passt
  (else body...))                   ; expr wird einmal ausgewertet. Schlüssel werden mit equal verglichen.
                                     ; Schlüssel sind „Literale“ und werden nicht ausgewertet (wie in CL).
                                     ; ein bloßes Symbol a bedeutet das Symbol 'a.
                                     ; 'a zu schreiben ist ein Fehler (das bloße a verwenden). defmacro
(ecase expr (key body...) ...)      ; ein case, das einen Treffer verlangt. Panic, wenn nichts passt. defmacro
(ccase expr (key body...) ...)      ; CLs ccase. es gibt keine Restarts anzubieten, daher dasselbe wie ecase. defmacro
(and expr...)                       ; Kurzschlussauswertung. true bei null Argumenten. defmacro
(or expr...)                        ; Kurzschlussauswertung. false bei null Argumenten. defmacro
(progn body...)                     ; führt der Reihe nach aus und gibt den letzten Wert zurück
(unsafe body...)                    ; dasselbe wie progn, plus die Erlaubnis, FFI-Aufrufe
                                     ; und rohe Wörter zu schreiben. siehe 3.3 defffi
(prog1 form more...)                ; wertet alles aus; der Wert ist der von form. defmacro
(prog2 a b more...)                 ; wertet alles aus; der Wert ist der von b. defmacro
(the Type expr)                     ; eine Typannotation (keine Wirkung zur Laufzeit)
```

### 4.1 unsafe — nicht prüfbare Annahmen übernehmen

```lisp
(unsafe body...)
```

Dasselbe wie `progn`: wertet den Rumpf der Reihe nach aus und gibt den letzten Wert zurück. Es erzeugt keinen
Gültigkeitsbereich und ist keine Funktionsgrenze (`break` / `return-from` gehen direkt nach außen durch). Der
Unterschied ist, dass sich manche Dinge nur darin schreiben lassen.

Drei Dinge verlangen derzeit `unsafe`: der Aufruf von C-Funktionen, die mit
[defffi](#33-defffi--c-funktionen-deklarieren-ffi) deklariert sind, rohe Maschinenwörter (`ptr` / `c-long` /
`c-ulong` / `(ptr T)`) zu Werten zu machen, sowie
[`def-c-struct` und `c-alloc`](#def-c-struct-und-typisierte-zeiger--c-structs-anlegen).

Mit `c-alloc` angelegter Speicher wird beim Verlassen des äußersten `unsafe` innerhalb derselben Funktion
freigegeben. Nur dieses `unsafe` hat, anders als `progn`, beim Verlassen etwas zu tun: das Freigeben.

Was `unsafe` übernimmt, sind die folgenden Annahmen, die der Compiler nicht prüfen kann:

- **Dass die Typen passen.** Dass die deklarierte C-Signatur zur wirklichen passt. Wenn nicht, gehen Argumente in
  die falschen Register, und Rückgabewerte werden in der falschen Breite gelesen.
- **Speichersicherheit.** Was die C-Seite mit dem macht, was sie bekommt.
- **Prozessweiter Zustand.** Umgebungsvariablen, Signalhandler, `errno`. Ruft man zum Beispiel `setenv` über das
  FFI auf, zerstört das die Annahmen, die `decode-universal-time` dieser Implementierung beim Berechnen der
  Ortszeit macht.
- **Threadsicherheit.**

Es ist kein Ausweg aus der Typprüfung. `(unsafe (+ 1 "two"))` geht nicht durch. Erlaubt wird, bestimmte
**Operationen** zu schreiben, nicht, Unsinn zu schreiben.

Es wirkt lexikalisch. Der Rumpf eines innerhalb von `unsafe` geschriebenen `lambda` erbt die Erlaubnis (wie
Closures innerhalb von Rusts `unsafe`-Blöcken). Der Wert kann später von außerhalb des `unsafe` aufgerufen
werden, aber ihn dort zu schreiben, gilt bereits als Übernahme der Verantwortung.

### 4.2 destructuring-bind — Listen nach ihrer Form zerlegen

```lisp
(destructuring-bind lambda-list form body...)
```

Zerlegt die Liste, die `form` liefert, **nach ihrer Form** und bindet sie. Die Lambda-Liste ist die von
`defmacro` (Pflicht → `&optional` → `&rest`/`&body` → `&key`, jeweils mit Standardausdrücken), aus demselben
Grund, aus dem CL eine für beide teilt: Es sind zwei Formen, die dasselbe zerlegen.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Jede gebundene Variable ist ein `Option<Sexpr>`.** Das ist keine Einschränkung der Implementierung, sondern
  das Wesen dessen, was gebunden wird: S-Ausdruckslisten sind die einzigen Listen dieser Sprache, daher gibt es
  keinen anderen Typ, den man den Elementen geben könnte. Wo ein Skalar gebraucht wird, auf `match`
  auszuweichen, ist dasselbe wie in einem `defmacro`-Rumpf.
- **Eine Form, die nicht passt, löst einen Panic aus** (entspricht CLs Fehler): zu wenige oder zu viele Elemente,
  eine `&key`-Folge ungerader Länge oder ein unbekanntes Schlüsselwort. `sexpr-car` ist eine nachsichtige
  Funktion, die für `()` `()` zurückgibt; ohne die Prüfung würde eine zu kurze Liste daher stillschweigend an
  eine leere Folge gebunden.
- **Verschachtelte Lambda-Listen werden nicht unterstützt.** `defmacro` nimmt sie auch nicht, daher gibt es eine
  Regel. `(a (b c))` bindet nicht stillschweigend eine Unterliste an `b`; es ist ein Fehler, der das sagt.
- Die Standardausdrücke von `&optional` / `&key` werden **nur ausgewertet, wenn sie verwendet werden** (wie in
  CL).
- Es gibt nichts, was CLs `&allow-other-keys` entspräche (auch `defmacro` hat keines).

### 4.3 match — Mustervergleich

```lisp
(match expr
  (pattern body...)
  ...)
```

Arten von Mustern:
- `_` — Platzhalter
- Ein Variablenname — ein Bindungsmuster (passt immer). Hat der Typ des untersuchten Wertes jedoch eine
  Variante dieses Namens, wird er als **das unten beschriebene Muster mit bloßem Variantennamen** aufgelöst
- Ein bloßer Variantenname — passt auf eine Variante ohne Argumente (`(match c (red 1) (blue 2))`). Eine Variante
  mit Feldern mit ihrem bloßen Namen zu schreiben, ist ein Stelligkeitsfehler; man schreibt sie in Klammern, wie
  in `(circle r)`
- **Direkte Literale**: Ganzzahlen / `true`/`false` / Zeichen — als Wörter verglichen
- **Wertliterale**: Zeichenketten / Gleitkommazahlen / Symbole (`'foo`) / Bignum-Ganzzahlen / Brüche — nach
  Wert mit dem `Eq::equals` dieses Typs verglichen ([Standard-Traits](functions/traits.md#2-eq--ord-vergleich)).
  Zeichenketten werden nach Inhalt verglichen, nicht nach Identität
- `(= expr)` — wertet einen beliebigen Ausdruck aus und vergleicht mit `Eq::equals`. Die einzige Möglichkeit,
  Typen ohne Literalsyntax zu vergleichen (`defstruct`-Instanzen, globale Variablen, berechnete Ergebnisse), und
  eine benutzerdefinierte `Eq`-Implementierung wird unverändert zur Vergleichsregel. `expr` kann sich auf alles
  beziehen, was von der Position des Zweigs aus sichtbar ist (Argumente, äußere Bindungen, globale Variablen)
- `(Ctor sub-pattern...)` — Konstruktormuster (`Some x` `None` `Cons a d` `Ok v` usw.)

Einen Typ, der `Eq` nicht implementiert, mit einem Wertliteral / `(= expr)` zu vergleichen, ist ein Typfehler
(diese Sprache sagt lieber „diese lassen sich nicht vergleichen“, als einen Zweig stehen zu lassen, der
stillschweigend nie passt).

**Wertliterale gegen einen `Sexpr` als untersuchten Wert**: Das `Eq` von `sexpr` ist `eq` (CLs Identität), daher
lassen sich direkte Werte (`'foo` (interniert) / Ganzzahlen / Zeichen / `true`/`false`) unverändert schreiben und
passen nach Inhalt:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Nicht direkte Literale (Zeichenketten / Gleitkommazahlen / Bignum-Ganzzahlen / Brüche) **lassen sich gegen ein
`Sexpr` nicht schreiben**. Ihr `eq` vergleicht die Identität der Objekte, was einen „Zweig ergäbe, der die
Typprüfung besteht, aber nie passt“, daher ist es ein Fehler, der das Variantenmuster nennt: Schreibt man
`(str "hi")`, wird es in eine `string` zerlegt und nach Inhalt verglichen. `(= expr)` verlangt ausdrücklich
`equals`, daher gilt diese Einschränkung dafür nicht.

**Der untersuchte Wert muss kein ADT sein.** `string`/`symbol`/`i32`/`f64` und Ähnliches lassen sich direkt
vergleichen (dorthin gehören Muster mit Zeichenkettenliteralen). Ein Typ ohne Varianten lässt sich jedoch nicht
durch Aufzählung abdecken, daher ist `_` (oder ein als Platzhalter wirkendes Bindungsmuster) Pflicht:

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; ein Typ ohne Varianten braucht `_`
```

Gegen einen `Sexpr` als untersuchten Wert lassen sich neben den 16 eingebauten Variantenmustern oben
**Downcast-Muster** schreiben (Herausholen von Instanzen benutzerdefinierter ADTs): Syntax, um mit `match` eine
Instanz eines `defstruct`/`defenum` (Kapitel 3) zurückzubekommen, die implizit in `Sexpr` umgewandelt wurde, wie
in `(list p 42)`:

- `(TypeName sub-pattern...)` — Zerlegung in Felder mit dem **Typnamen** vorn (nur Strukturen: Ein `defstruct`
  hat immer eine Variante, daher schreibt man es mit dem Typnamen statt mit einem Variantennamen). Zum Beispiel
  für `(defstruct point (x f64) (y f64))` `(point x y)`.
- Ein bloßer Variantenname `(VariantName sub-pattern...)` — holt eine Variante eines `defenum` heraus. Aufgelöst
  als bloßer Name, der nach `(use EnumType)` sichtbar ist (dieselben Sichtbarkeitsregeln wie beim Aufruf des
  Konstruktors). Zum Beispiel für `(defenum color (red) (blue))` nach `(use color)` `(red)` `(blue)`. Kollidieren
  Variantennamen mehrerer sichtbarer Enums, ist es ein Mehrdeutigkeitsfehler, daher lässt sich auch die
  qualifizierte Form `(EnumType::VariantName ...)` schreiben (kein `use` nötig).
- `(the Type pattern)` — ein Downcast des ganzen Typs (bindet ihn als Ganzes). Es zerlegt keine Felder; es
  übergibt den Wert unverändert an `pattern`. Die einzige Möglichkeit, eine veränderliche Struktur unter Erhalt
  ihrer Identität herauszuholen, und auch die einzige Möglichkeit, einen `Vector<T>`/`HashTable<K,V>` aus einem
  `Sexpr` zu holen (sie haben keine Form zur Zerlegung in Felder). Zum Beispiel schlägt sich nach
  `(the point p)` ein `(setf p::x 9)` auch in der ursprünglichen Instanz in der Liste nieder.

**Muster für `Option<Sexpr>`**: Der Typ von S-Ausdrucksdaten ist nicht `Sexpr`, sondern `Option<Sexpr>`, und die
leere Liste ist keine Variante von `Sexpr`, sondern das `none` von `Option`. Vergleicht man daher ein
`Option<Sexpr>`, lassen sich die 16 Varianten von `Sexpr` und `none` **flach in derselben Liste von Zweigen**
schreiben (es braucht kein äußeres `match`, um das `Option` abzuschälen):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; die leere Liste
    (_          9)))
```

Die Vollständigkeit wird im selben flachen Universum geprüft: die 16 Varianten von `Sexpr` plus `none`,
insgesamt 17. `(none)` zu vergessen, ist ein Fehler, sofern es kein `_` gibt. Auch `(some x)` lässt sich
schreiben und bindet „etwas nicht Leeres“.

Dieser Zucker gilt **genau** nur für `Option<Sexpr>`. Bei `Option<Option<Sexpr>>` wäre unklar, welche Schicht
`(int n)` abgeschält hat, daher schreibt man wie üblich zwei Ebenen `match`.

Dieselben Downcast-Muster lassen sich unverändert auf einen **Trait-Objekt-Wert (`:dyn Trait`, Kapitel 2)** als
untersuchten Wert anwenden: `match` packt ihn aus und übergibt ihn dann der obigen Maschinerie für
`Sexpr`-Muster, daher gibt es keine zusätzliche Syntax. Die Menge der implementierenden Typen ist offen, daher
kann es nie vollständig sein, und `_` ist Pflicht:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; Zerlegung in Felder mit dem Typnamen vorn
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Typableitung über Zweige hinweg**: Alle Zweige müssen denselben Typ haben (außer Zweigen, die divergieren, wie
mit `panic`). In einem `match`, das dort steht, wo kein Typ erwartet wird, ergänzen die Zweige gegenseitig ihre
fehlenden Typargumente: `(result::ok v)` legt nur `T` fest und `(result::err e)` nur `E`, aber zusammen legen sie
`Result<T,E>` fest. Ein Typargument, das bis zum Ende kein Zweig festlegen kann, ist ein Fehler dieses Zweigs
(`cannot infer type argument ...`). Außerhalb von `match` ist ein nicht festlegbares Typargument dort ein Fehler.

Die Vollständigkeitsprüfung eines `match` mit Downcast-Mustern zählt diese nicht zur Abdeckung der eigenen
Varianten von `Sexpr` (ein `match`, das nur Downcast-Muster aufführt, muss mit `_` abgeschlossen werden). Bei
generischen ADTs (`defstruct point<T> ...` usw.) lassen sich die Typargumente eines Downcast-Musters nicht
ableiten, daher lassen sich die Form zur Zerlegung in Felder (`(point ...)`) und die Form mit bloßem
Variantennamen nicht verwenden; man gibt sie mit `the` an, wie in `(the point<i32> p)`.

**Downcasts betrachten auch die Instanziierung.** Ausdrückliche Typargumente werden zum Abgleich verwendet:
`(the point<i32> p)` lässt nur Werte von `point<i32>` durch, und ein `point<string>` geht zum nächsten Zweig
weiter. Das liegt daran, dass sich ein Wert seinen Typ einschließlich der Typargumente merkt (derselbe
Mechanismus, der `print-object` wählt).

```lisp
(if-let (pattern val) then els)     ; then (mit Bindungen), wenn val auf pattern passt, sonst els. defmacro
(while-let (pattern val) body...)   ; wiederholt, solange val (jedes Mal neu ausgewertet) auf pattern passt. defmacro
```

## 5. Iteration

```lisp
(loop body...)                      ; eine Endlosschleife. mit break/return verlassen
(while test body...)                ; wiederholt, solange test wahr ist. defmacro
(until test body...)                ; wiederholt, solange test falsch ist (die Negation von while). defmacro
(dotimes (var count-expr) body...)  ; wertet count-expr einmal aus und lässt var über 0..count-1 laufen. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; Iteration im Stil von CL mit parallelem Fortschalten. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; die sequenzielle Fassung von do (Bindung wie let*, der Reihe nach zugewiesen). defmacro
(doiter (var coll-expr) body...)    ; iteriert über einen Wert, der den Trait Iter implementiert. defmacro

(break)                             ; verlässt nur die innerste Schleife. der Wert ist immer Unit
(return)                            ; verlässt nur die innerste Schleife
(return value)                      ; verlässt die innerste Schleife mit einem Wert
```

Sowohl `break` als auch `return` verlassen **nur die innerste umschließende Schleife** (sie sind keine vorzeitige
Rückkehr aus der Funktion und können keine `lambda`-Grenze überschreiten). Der Typ eines `loop` ist die
Vereinigung der Werttypen der darin gefundenen `break`/`return` (`!`, wenn sie nie verlassen wird). Um eine
Funktion zu verlassen, verwendet man `return-from`, unten.

### 5.1 `block` / `return-from` — benannte Ausgänge

```lisp
(block name body...)                ; ein benanntes Ausgangsziel. der Wert ist die letzte Form
                                    ; oder der von return-from übergebene Wert
(return-from name)                  ; verlässt diesen block mit Unit
(return-from name value)            ; verlässt ihn mit einem Wert
```

**Jede Funktion von `defun` / `defmethod` / `labels` richtet implizit einen Block mit ihrem eigenen Namen ein**
(wie in CL). Daher ist `(return-from f v)` eine vorzeitige Rückkehr aus der Funktion:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` ist ein **lexikalischer** Ausgang, und der Name wird **dort aufgelöst, wo er geschrieben steht**: Die
Prüfung ordnet ein `return-from` dem umschließenden `block` zu und vereinigt den Typ seines Wertes mit dem
Ausgangstyp des Blocks. Daher:

- Ein `return-from` ohne passenden `block` ist ein **Typfehler** (kein Laufzeitfehler).
- Ein Wert, dessen Typ nicht zu den anderen Ausgängen oder dem Typ des Rumpfes passt, ist ein **Typfehler**
  (dieselbe Regel wie bei `match`-Zweigen).
- Sind Blöcke gleichen Namens verschachtelt, **gewinnt der innere** (CLs Verdeckungsregel).
- **Funktionsgrenzen lassen sich nicht überschreiten.** Aus einem `lambda` heraus kann man nicht zu einem äußeren
  `block` hinaus (`lambda` richtet keinen Block ein: CLs implizite Blöcke brauchen einen *Namen*, und anonyme
  Funktionen haben keinen). Was überschreiten muss, ist `catch`/`throw` (Kapitel 8, das **dynamisch** ist).

Wie `break`/`return` (Kapitel 5) ist es ein **statischer** Ausgang, daher ist es in kompiliertem Code ein Sprung
zu einem zur Kompilierzeit festgelegten Basisblock. Liegt ein `unwind-protect` dazwischen, läuft dessen
`cleanup` (Kapitel 8).

Schreibt man nie `return-from`, kostet der implizite Block nichts.

### 5.2 Erweitertes `loop` (CLs LOOP)

**Ist das erste Element von `loop` ein Schlüsselwort**, wird es als Folge von Klauseln gelesen. Andernfalls
bleibt es die einfache Schleife oben, und die Bedeutung bestehender `loop`s ändert sich nicht (dieselbe Regel wie
CLs eigene Regel für die einfache Schleife).

CL schreibt die Klauselwörter als bloße Symbole (`(loop for i from 1 to 3 collect i)`), aber hier **sind sie alle
Schlüsselwörter**: Ein bloßes `for` wäre nur ein Variablenverweis, und ein Schlüsselwort zu sein, unterscheidet
es zugleich von einer einfachen Schleife. Die Ausnahme ist `=`, das eine Variable von einem Wert trennt: Seine
Position ist eindeutig, daher wird es bloß oder als Schlüsselwort (`:=`) gelesen.

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #<vector<int> 1 2 3>
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #<vector<int> 1 2 4 8>
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Variablenklauseln** (vor den Rumpfklauseln geschrieben. Das ist CLs Regel: Danach geschrieben, könnten sie als
„erst ab hier iterieren“ gelesen werden, daher ist es ein Fehler):

| Klausel | Bedeutung |
|---|---|
| `:with v = e` | Bindet einmal. Darf die Variablen früherer Klauseln lesen |
| `:for v :in s` / `:for v :across s` | Die Elemente eines `Iter` der Reihe nach. CLs Unterscheidung zwischen Liste und Vektor gibt es hier nicht, daher sind es zwei Schreibweisen derselben Klausel |
| `:for v :on s` | Die aufeinanderfolgenden **Suffixe**. CL übergibt die geteilte Endzelle, aber ein `Iter` hat kein Ende zu teilen, daher ist jedes ein neuer `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Zählen. Auch `:downfrom`/`:upfrom` funktionieren |
| `:for v = e [:then f]` | Beginnt mit `e` und verwendet ab dem zweiten Mal `f` (ohne `:then` jedes Mal `e`) |
| `:repeat n` | Iteriert so oft |

Bei mehreren `:for` schreiten sie **parallel** fort, und die Schleife endet, sobald eines erschöpft ist.

**Rumpfklauseln** (laufen jedes Mal, in der geschriebenen Reihenfolge):

| Klausel | Bedeutung |
|---|---|
| `:do form...` | Für Seiteneffekte |
| `:collect e [:into v]` | Sammelt in einen `Vector<T>` |
| `:append e [:into v]` | Hängt den Inhalt eines `Iter` an |
| `:sum e` / `:count e` | Die Summe / wie oft es wahr war |
| `:maximize e` / `:minimize e` | Das Maximum / Minimum. **`Option<T>`** (so wie CL für eine leere Folge nil zurückgibt; ein beliebiger `Ord`-Typ hat kein kleinstes Element) |
| `:always e` / `:never e` | `true`, wenn alle gelten; sofort `false`, sobald eines scheitert |
| `:thereis e` | `e` ist ein **`Option<T>`**. Gibt das erste `some` zurück oder `none`, wenn es keines gibt (das entspricht CLs „erster nicht-nil-Wert“; um ein `bool` zu prüfen, verwendet man `:always`/`:never`) |
| `:while e` / `:until e` | **Endet hier normal** (`:finally` läuft, und das Gesammelte ist die Antwort) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Macht eine Klausel bedingt |
| `:return e` | Verlässt sofort mit diesem Wert (`:finally` läuft nicht, wie in CL) |
| `:initially form...` / `:finally form...` | Vor der Schleife / bei normalem Abschluss |

**`:named name`** (vor jeder anderen Klausel, nur einmal) hüllt die ganze Schleife in `(block name …)`.
`(return-from name e)` kann auch aus verschachtelten Schleifen heraus sofort verlassen, und wie bei `:return`
läuft `:finally` nicht. Ohne Namen wird kein Block eingerichtet: CLs unbenanntes `loop` richtet `block nil` ein,
aber hier gibt es kein `nil`, und `break`/`return` (Kapitel 5) bieten bereits „die innerste Schleife verlassen“.

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

`:finally (return 0)` wegzulassen, ist ein **Typfehler**. Es sind einfach die Regeln von `block` am Werk (5.1):
Der Typ `int` des Ausgangs passt nicht zum `()`, das die Schleife hinterlässt, wenn sie erschöpft ist.

**Der Wert der Schleife** ist das Gesammelte der sammelnden Klausel, falls es eine gibt (die erste, wenn es
mehrere gibt), `true` bei `:always`/`:never`, `none` bei `:thereis` und `()`, wenn es keine gibt. Ist das Letzte
in `:finally` `(return e)`, ist das der Wert: CLs Idiom `finally (return …)`, die einzige Möglichkeit, wie eine
nicht sammelnde Schleife ihre eigene Antwort nennen kann.

**Unterschiede zu CL / was nicht enthalten ist**:

- **Klauselwörter sind Schlüsselwörter** (oben).
- `:maximize`/`:minimize`/`:thereis` geben `Option<T>` zurück (es gibt kein nil).
- **Nur `:return` zu schreiben, ohne Sammeln und ohne `:finally`, ist ein Fehler.** CL gibt bei Erschöpfung nil
  zurück, aber so etwas gibt es hier nicht, daher muss die Schleife sagen, was ihr Wert bei Erschöpfung ist.
- Das Verbinden paralleler Klauseln mit `:and`, `:being`/eigene Iteration über Hashtabellen, `:it` und `:nconc`
  sind nicht enthalten.
- Der Elementtyp von `:collect` stammt aus dem Typ des gesammelten Ausdrucks. Versucht man, einen Typ zu
  sammeln, der **sich nicht als Typname schreiben lässt**, etwa einen Funktionstyp, ist das ein Fehler, der das
  sagt.

## 6. Funktionswerte und Aufrufe

```lisp
(lambda (params) RetType body...)   ; erzeugt einen Funktionswert erster Klasse (eine Closure)
(labels ((name (params) RetType body...) ...) body...)   ; lokale Funktionsdefinitionen, die sich wechselseitig rekursiv aufrufen können
(apply f arg1 ... argN rest-list)   ; ruft f (eine variadische Funktion mit &rest) auf und breitet rest-list aus
```

Auch benannte Funktionen lassen sich unverändert als Werte übergeben (als Argumente an Funktionen höherer Ordnung
usw.).

## 7. Weitere Spezialformen

```lisp
(setq var value ...)                ; CLs Variablenzuweisung. nur eine Folge von (setf var value). defmacro
(psetq var value ...)               ; parallele Zuweisung. wertet zuerst alle Werte aus, dann wird zugewiesen. defmacro
(psetf place value ...)             ; psetq auf Orte verallgemeinert (dieselbe Expansion). defmacro
(setf place value)                  ; Zuweisung an einen Ort. ein Ort ist ein Variablenname / var::field /
                                     ; ein Aufruf der Form (accessor recv key...). gültig, wenn der
                                     ; statische Typ von recv eine Instanzmethode namens
                                     ; set-{accessor} hat (zum get von Vector<T> und HashTable<K,V>
                                     ; gehört ausnahmsweise set; sonst set-accessor-name).
                                     ; der Wert ist der zugewiesene Wert (wie in CL). daher
                                     ; passen in (if c (setf x 1) ()) die Typen von then und else nicht zusammen
(incf place)  (incf place delta)    ; place += delta (delta=1, wenn weggelassen). das Ergebnis wie bei setf
(decf place)  (decf place delta)    ; place -= delta (delta=1, wenn weggelassen)
(rotatef place1 place2 ... placeN)  ; rotiert N Orte (neues place1=altes place2, ...,
                                     ; neues placeN=altes place1). Teilformen jedes Ortes einmal ausgewertet
(shiftf place1 ... placeN newvalue) ; verschiebt die Werte von place2..N nach links und setzt newvalue in placeN.
                                     ; der Rückgabewert ist der alte Wert von place1
(list e1 e2 ... en)                 ; expandiert zu (cons e1 (cons e2 (... ()))). () bei null Argumenten.
                                     ; jedes Element wird implizit in Sexpr umgewandelt (wie CLs cons kann es
                                     ; jeden Wert halten). Skalare (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) werden in die passende Sexpr-Variante gehüllt, und defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> und Ähnliches gehen unverändert hinein
                                     ; (ohne Umwandlungskosten). dasselbe gilt für &rest-/format-Argumente.
(source-file)                       ; der Name der Datei, aus der diese Form gelesen wurde (string). bei der
                                     ; Prüfung als Konstante festgelegt. entspricht CLs *load-pathname*, ist aber
                                     ; keine Variable: Modulrümpfe laufen nach der Prüfung, daher kann man sich auf
                                     ; „wird gerade geladen“ nicht verlassen, bei der Prüfung ist es dagegen immer bekannt.
                                     ; bei Quellen, die keine Dateien sind, der Name, den der Reader ihnen gibt (<stdin>/<input>)
(quote datum)                       ; dasselbe wie 'datum. gibt es unausgewertet als Sexpr-Daten zurück
(quasiquote template)               ; dasselbe wie `template. bettet mit ,/,@ Ausdrücke in die Vorlage ein
(documentation name)                ; gibt den Docstring von name (bloßer Name oder Type::method) als Option<string> zurück
(panic message)                     ; message: string. endet abnormal mit einem nicht behebbaren Fehler. Typ !
(unreachable)                       ; expandiert zu (panic "unreachable"). defmacro
(todo)                              ; expandiert zu (panic "todo"). defmacro
(as Type expr)                      ; Umwandlung numerischer/Zeichentypen. Umwandlungen, die fehlschlagen können, lösen dann einen Panic aus
(try-as Type expr)                  ; wie as, gibt das Ergebnis aber als Option<Type> zurück (None bei Fehlschlag)
(print control args...)             ; expandiert das Format und schreibt auf die Standardausgabe (ohne Zeilenumbruch)
(println control args...)           ; dasselbe (mit einem Zeilenumbruch am Ende)
(format dest control args...)       ; CLs format. gibt die expandierte Zeichenkette zurück
(pprint x)                          ; formatiert schön. schreibt wie in CL zuerst einen Zeilenumbruch
(pprint-fill x)                     ; Füll-Layout
(pprint-linear x)                   ; alles auf einer Zeile oder ein Element pro Zeile
(pprint-tabular x [colinc])         ; tabellarisches Layout (standardmäßig 16 Spalten)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; einen logischen Block selbst bauen
```

Die Familie `print`/`println`/`format`/`pprint` sind Spezialformen, daher werden ihre variadischen Argumente (bei
der `pprint`-Familie ein einzelnes Objekt) vor der Übergabe mit ihren eigenen Typen in `Sexpr` gehüllt: Deshalb
funktioniert `(println "~a" my-struct)` einfach. Die Details der Formatdirektiven und des Pretty Printers stehen
unter [Formatdirektiven](functions/format.md) und [Ausgabe](functions/printing.md#4-der-pretty-printer).

`as`/`try-as` behandeln nur den Katalog numerischer und Zeichentypen (zwischen `int`, den Ganzzahltypen fester
Breite, `f32`/`f64`/`ratio`/`char`). Derselbe Typ ist keine Umwandlung. **Umwandlungen zwischen
Ganzzahlbreiten (einschließlich `int`) und zwischen `f32`↔`f64` sind echte Umwandlungen**: `as` schneidet ab /
rundet, und `try-as` beantwortet, ob es in diese Breite (Genauigkeit) passt. `(as int x)` ist die exakte
Erweiterung von einer festen Breite, und `(as i32 n)` das Abschneiden von `int`. Ganzzahl → `char` kann außerhalb
des Bereichs fehlschlagen, daher löst `as` einen Panic aus und `try-as` liefert `None`. Alles andere (Erweiterung
und das Abschneiden von `float->int`/`ratio->int`) gelingt immer. `float->int`/`ratio->int`/`char->int` landen
bei `int`, und wird eine schmalere Breite verlangt, wird danach `int->W` aufgerufen. Das ist Zucker, der zu den
entsprechenden Umwandlungsmethoden expandiert (`int->char`/`int->int`/`int->W` usw. unter
[Zahlen](functions/numbers.md)).

`documentation` ist wie `quote`/`compile` eine Spezialform, die `name` unausgewertet liest, als bloßes Symbol /
`::`-Pfad. Anders als CLs `(documentation 'name 'function)` nimmt es kein Typargument: Es löst `name` in der
Reihenfolge Variable → Funktion → Typ → Trait → Makro auf (dieselbe Priorität wie bei einem bloßen Bezeichner,
der als Ausdruck ausgewertet wird) und gibt den Docstring der gefundenen Definition zurück
(`(documentation Type::method)` ist für Methoden). Scheitert die Auflösung (keine Definition dieses Namens), ist
das ein Fehler bei der Prüfung; eine Definition, die existiert, aber keinen Docstring hat, ergibt `Option::none`.
Alles wird bei der Prüfung als Konstante bestimmt: Zur Laufzeit wird nichts nachgeschlagen. Modulqualifizierte
freie Namen (`mod::name`, außer `Type::method`) werden nicht unterstützt.

## 8. Nichtlokale Ausgänge (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; führt body aus. geschieht irgendwo, wohin body gelangt,
                                    ; (throw 'tag v), wird dieses v zum Wert
(throw 'tag value)                  ; springt zum nächsten dynamisch umschließenden (catch 'tag ...)
(unwind-protect protected cleanup)  ; führt cleanup aus, wie auch immer protected verlassen wird
```

Anders als `break`/`return` (Kapitel 5) ist das ein **dynamischer** Ausgang: `throw` sucht nicht lexikalisch nach
dem `catch` um sich herum, sondern erreicht ein `catch` mit derselben Marke über beliebig viele
Funktionsaufrufe hinweg.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; wenn nicht gefunden, wie üblich der Wert am Ende
```

- **Marken sind nur literale Symbole** (`'done`). Anders als in CL werden sie nicht ausgewertet.
- **Eine Marke trägt einen Typ.** Der Typ wird bei der ersten Verwendung von `'tag` bestimmt, und jedes spätere
  `throw`/`catch` desselben Symbols wird dagegen geprüft. Sie mit einem anderen Typ zu verwenden, ist ein
  Typfehler.
- Der Typ von `throw` ist `!` (es divergiert). Der Typ von `(catch 'tag expr)` ist die Vereinigung des Typs von
  `expr` und des Typs der Marke.
- Der Wert von `unwind-protect` ist der Wert von `protected`. Der Wert von `cleanup` wird verworfen. `cleanup`
  läuft, wie auch immer `protected` verlassen wird: zusätzlich zu normalem Abschluss, `throw` und `panic` läuft es
  auch, wenn es durch `break`/`return`/`return-from` verlassen wird. Ein nichtlokaler Ausgang durch `cleanup`
  selbst gewinnt gegen den gerade laufenden Ausgang.
- Verschachtelte `unwind-protect` laufen von innen nach außen. Ein `break`, das eine Schleife **innerhalb** von
  `protected` verlässt, hat `protected` nicht verlassen, daher läuft dessen `cleanup` nicht.

CLs Bedingungen (`define-condition`/`handler-bind`/`invoke-restart`) werden nicht übernommen. Sie passen nicht zur
statischen Typisierung, daher werden behebbare Fehlschläge mit `Result` ausgedrückt (Kapitel 9).

## 9. Grundsätze der Fehlerbehandlung

- Behebbare Fehlschläge: `Result<T,E>` + `match`. Nicht behebbare Fehlschläge (Programmfehler, verletzte
  Invarianten): `panic`.
- Eine Syntax, die `?`/try entspräche, gibt es nicht. Verzweigungen schreibt man ausdrücklich mit `match`.
- Namen von Funktionen und Spezialformen verwenden `!` (destruktive Operationen) oder `?` (Prädikate) nicht als
  Suffixe. Prädikate werden mit dem Suffix `-p`/`p` (`zerop`, `consp` usw.) oder dem Präfix `is-` (`is-some`,
  `is-ok` usw.) benannt.

## 10. Kompilierung

```lisp
(compile name)                      ; JIT-kompiliert ein bereits definiertes defun/eine Methode in nativen Code
(compile-file src-path out-path)    ; AOT-kompiliert eine Quelldatei in ein natives ausführbares Programm (überspringt das abschließende `(main)`)
(dump path)                         ; schreibt die aktuelle Umgebung (Typinformation + kompilierte Rümpfe) in eine Datei
(disassemble name)                  ; gibt aus, was aus dieser Definition wird (standardmäßig Maschinencode des Rechners, mit true als zweitem Argument LLVM IR)
```

`compile` ist eine Spezialform; `name` wird nicht ausgewertet, sondern als unausgewertetes bloßes Symbol /
`::`-Pfad gelesen (eine Zeichenkette ist ein Typfehler). Generische Funktionen lassen sich nicht als Ziel
angeben: An jeder Verwendungsstelle wird für jeden Typ eine Kopie erzeugt, daher gibt es keinen einzelnen
kompilierten Rumpf. **Ein Name, der sich nicht auflösen lässt, ist ein Fehler bei der Prüfung** und wird nie in die
Laufzeit getragen (es gibt eigene Meldungen für: der Typ existiert, aber nicht diese Methode / weder der Typ noch
die Funktion existiert / ein bloßer undefinierter Name). Die Sichtbarkeit wird hier wie bei jedem anderen Verweis
behandelt: „existiert, ist aber von hier aus nicht sichtbar“ scheitert bei der Prüfung, genau wie „lässt sich
nicht auflösen“.

Aufgerufene Funktionen werden ebenfalls transitiv kompiliert, daher **lässt sich eine Funktion, die (auch
indirekt) etwas aufruft, das sich nicht kompilieren lässt, nicht kompilieren**. Der Prozess stürzt nicht ab; es
wird mit einem Fehler abgelehnt, der das sagt. Jede eingebaute Funktion lässt sich kompilieren, daher werden auf
diese Weise nur Funktionen abgelehnt, die die folgenden Operationen aufrufen, die es nur im Interpreter gibt:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

Die Operationen, die es nur im Interpreter gibt, sind `compile`/`compile-file`/`dump` und
`trace`/`untrace`/`step`/`disassemble`
([Werkzeuge der Implementierung](functions/system.md#5-werkzeuge-der-implementierung-clhs-252)). Statt Dinge zu
sein, die sich nicht kompilieren lassen, sind dies Operationen der kompilierenden Seite (was `dump` hinausschreibt,
ist die Umgebung des Interpreters selbst, die ein AOT-Programm nicht hat; was `trace` beobachtet und wo `step`
anhält, sind die Aufrufwege des laufenden Interpreters; und `disassemble` verwendet den Compiler selbst).
`room`/`dribble`/`ed` gehören nicht dazu und lassen sich normal kompilieren.

Was sich kompilieren **lässt**: Stream- und Datei-E/A, `random`, `gensym`, `symbol->string`/`string->symbol`,
`parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, die transzendenten Funktionen,
Bitoperationen, `catch`/`throw`/`unwind-protect`, alle vier von `eq`/`eql`/`equal`/`equalp` (wodurch sich `case`
für jeden Typ kompilieren lässt), die ganze Ausgabefamilie einschließlich `print`/`println`/`format`/`pprint` und
`pprint-logical-block`, `read` und `eval`. Die Standardbibliothek wird bereits kompiliert ausgeliefert.

Ein AOT-Programm enthält nur die Funktionalität, die das Programm verwendet. Ein Programm, das nichts ausgibt,
bekommt keine Formatierungsmaschine, eines, das `read` nicht aufruft, keinen Reader, und eines, das `eval` nicht
aufruft, weder Prüfung noch Interpreter.

Von der Kommandozeile tut `typl -c src-path [-o out-path]` (`-c` lässt sich auch `--compile` schreiben) dasselbe
wie `compile-file`. Ohne `-o` ist die Ausgabe `src-path` ohne die Endung `.typl`. Standardmäßig ist die statische
Bibliothek `libtypelisp_front.a`, die in ausführbare Programme gelinkt wird, bei einem Release-Build von `typl`
die, die `typl` in sich trägt, beim ersten Linken nach `$TYPELISP_HOME/lib/<Build-ID>/` (oder ohne
`TYPELISP_HOME` nach `~/.typelisp/lib/<Build-ID>/`) geschrieben und von dort verwendet; bei einem Debug-Build die
aus dem Ort, an dem `typl` gebaut wurde. `typl --remove-lib` löscht, was dieses `typl` hinausgeschrieben hat. Mit
`--others` löscht es die anderer Build-IDs, mit `--all` die aller Build-IDs. Mit `typl --lib-dir DIR` wird die in
`DIR` verwendet (sowohl für `-c` als auch für `compile-file`), und ist sie dort nicht, ist es beim Start ein
Fehler.

### 10.1 Dumps

```lisp
(dump "session.typld")     ; einen hinausschreiben
```
```sh
typl --image session.typld prog.typl   # daraus starten
typl --image session.typld             # auch die REPL
```

Ein Dump hält Typinformation und kompilierte Rümpfe in einer Datei. Was `(dump path)` schreibt, ist das, was die
aktuelle Sitzung geladen hat (die Standardbibliothek oder einen mit `--image` übergebenen Dump), plus **das, was die
Sitzung selbst definiert hat**. Daher ist die Ausgabe in sich abgeschlossen, und `typl --image` stellt dieselbe
Umgebung her. Was die Sitzung mit `(compile f)` kompiliert hat, wird in seiner kompilierten Form geschrieben.

Gespeichert werden **Definitionen, nicht der Verlauf**:

- Die Ausdrücke der Sitzung auf oberster Ebene (`(println ...)` usw.) sind nicht enthalten. Es wäre ein Problem,
  wenn das Laden sie erneut ausführte.
- Globale Variablen kommen mit **dem Wert ihres erneut ausgeführten Initialisierers** zurück, nicht mit dem Wert
  zum Zeitpunkt des Dumps. Das ist ein bewusster Unterschied zu SBCLs `save-lisp-and-die` (das den Heap
  unverändert hinausschreibt), und diese Wahl lässt eine ganze Familie von Problemen verschwinden: „Werte, die
  sich nicht speichern lassen“, wie offene Streams, Funktionszeiger von Closures und externer Speicher.
- Anders als bei `save-lisp-and-die` **stirbt der Prozess nicht**, da das Schreiben das Abbild nicht beschädigt.

Ein Dump vermerkt die Versionen der Standardbibliothek und des Compilers der Implementierung, die ihn geschrieben
hat. Ihn mit einem `typl` einer anderen Version zu laden, ist ein Fehler; er wird nie stillschweigend akzeptiert.

### 10.2 `eval` in AOT-Programmen

`eval` typprüft gegen „die aktuelle globale Umgebung“ und wertet dann aus
([Parsen und Auswerten](functions/system.md#6-parsen-und-auswerten)). Diese Umgebung (die Tabellen der
Signaturen, Typen und Makros, die die Prüfung zu Rate zieht, und die Rümpfe, die der Interpreter ausführen kann)
ist **nicht im Maschinencode**. Eine kompilierte Funktion ist nichts als ein an einer Adresse platziertes Symbol;
sie hat weder ihre Argumenttypen noch eine Tabelle, um Rümpfe über den Namen nachzuschlagen.

Daher **baut `compile-file` nur für Programme, die `eval` aufrufen, diese Umgebung zur Kompilierzeit und schreibt
sie in das ausführbare Programm**. Das Format ist dasselbe wie bei einem Dump und enthält den Teil der
Standardbibliothek und den eigenen Teil des Programms. Beim Start wird nur wiederhergestellt: Der Quelltext wird
nicht erneut gelesen, und nichts wird erneut typgeprüft. Programmen, die `eval` nicht aufrufen, wird nichts
hinzugefügt.

Folgen:

- **Der Start dauert länger, und das ausführbare Programm ist größer**, da der Code der Prüfung und des
  Interpreters und eine Momentaufnahme der Umgebung hineingehen. Auch der Heap wird etwas größer gemacht.
- **An eval übergebene Formen werden interpretiert.** Selbst wenn die an eval übergebene Form die eigenen
  Funktionen des Programms aufruft, läuft der interpretierbare Rumpf, den die Momentaufnahme hält. Das Ergebnis
  ist dasselbe; nur die Geschwindigkeit unterscheidet sich.

Der Speicher globaler Variablen wird mit kompiliertem Code **geteilt** (dieselben Plätze). Ein
`defvar`-Initialisierer wird einmal von der kompilierten Initialisierung ausgeführt, und die Wiederherstellung
überspringt ihn, daher läuft ein Initialisierer mit Seiteneffekten nicht zweimal.

`compile-file` liest auch die Standardbibliothek (und bettet ihre Rümpfe in das ausführbare Programm ein), daher
lassen sich Funktionen der Standardbibliothek wie `abs`/`gcd` sowie `(impl print-object ...)` und
`(defmethod print-object ...)` mit AOT verwenden.

`compile-file` akzeptiert auch `use` (und `import`/`shadowing-import`). Das `(use m)` der Einstiegsdatei findet
Dateien nach denselben Regeln wie `typl file.typl`, und die gefundenen abhängigen Dateien werden ebenfalls
kompiliert und in das ausführbare Programm gelinkt: Eine Anordnung, in der `main.typl` über `(use http)`
`http.typl` liest, lässt sich unverändert AOT-kompilieren. Auch die eigenen Definitionen der Einstiegsdatei kommen
wie bei `typl file.typl` in das nach der Datei benannte Modul (`point` in `p.typl` ist `p::point`). Daher ist die
Druckdarstellung von Werten (`#<p::point x: 1 y: 2>`) gleich, wie auch immer es ausgeführt wird.

## 11. Lesemakros (readtable)

Was der Reader **tut, wenn er auf ein bestimmtes Zeichen trifft**, lässt sich aus dem Programm heraus ersetzen
(CLHS 23.1).

```lisp
(set-macro-character c f)             ; f liest das Zeichen c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f liest die Zwei-Zeichen-Folge d s
(get-dispatch-macro-character d s)    ; Option<f>
```

Der Typ von `f` ist `(fn (string-input-stream char) Option<Sexpr>)`. Das erste Argument ist **ein Stream über den
noch nicht gelesenen Text**, und das zweite ist **das auslösende Zeichen** (bei einem Dispatch das zweite
Zeichen). Der Rückgabewert wird zu den an dieser Stelle gelesenen Daten. Der Stream ist ein konkreter Typ statt
`:dyn PeekInput`, weil der Reader immer diese eine Art übergibt: `read-sexpr` / `read-char` / `peek-char` /
`unread-char` / `read-delimited-list` nehmen alle `(where (PeekInput S))`, daher funktionieren sie alle
unverändert auf dem konkreten Typ.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => gelesen als (not (equal 1 2)), also true
```

Der Reader **schaut vor der eingebauten Syntax auf Makrozeichen**, daher kann er auch `(` und `'` übernehmen. So
registrierte Unterzeichen von `#` haben Vorrang vor den eingebauten `#b`/`#x`/`#.`. Ein anderes Zeichen als `#`
wird sofort zu einem Dispatch-Zeichen, wenn es an `set-dispatch-macro-character` übergeben wird: Es gibt **kein**
Gegenstück zu CLs `make-dispatch-macro-character`. Die Registrierung erledigt die Aufgabe bereits, daher hätte ein
eigener Schritt nichts zu tun.

**Wann sie wirken**, hängt wie bei `#.` (Kapitel 1) vom Leseweg ab:

- Die REPL und `(load ...)` führen eine Form nach der anderen aus, daher lassen sich **in früheren Formen
  definierte Funktionen** unverändert registrieren.
- Moduldateien werden als Einheit geprüft und später ausgeführt, daher **laufen nur die Aufrufe von
  `set-macro-character` / `set-dispatch-macro-character` sofort** (die Rolle von CLs
  `(eval-when (:compile-toplevel) ...)`). Da sie sofort laufen, **muss die übergebene Funktion zu diesem Zeitpunkt
  bereits existieren**. Ein `defun` in derselben Datei ist noch nicht gelaufen, daher schreibt man ein `lambda`
  oder verwendet die Standardbibliothek oder etwas, das bereits gelaufen ist. Nur Aufrufe auf oberster Ebene sind
  abgedeckt; es schaut nicht in `progn` oder `let` hinein.

Auch die eingebauten `read` / `read-from-string` ziehen die readtable zu Rate (wie in CL).

**Was es nicht gibt**: `*readtable*` und `copy-readtable` sowie `readtable-case`. Die ersten beiden, weil eine
readtable **kein Wert** ist: Ein Wert müsste „etwas sein, das man einem Reader übergeben kann“, aber der Reader,
der den Quelltext liest, liegt außerhalb des Programms, ohne einen Ort, an den man sie übergeben könnte.
`readtable-case`, weil Kapitel 1 festlegt, dass der Reader dieser Sprache immer kleinschreibt (CLs `:downcase`).


## 12. Nebenläufigkeit (Tasks)

**Ein Task ist ein leichtgewichtiger Thread** (in der Sprache von Go das, was eine `go`-Anweisung startet) und
läuft kooperativ (es gibt keine Verdrängung). Der Wechsel geht nicht über den Kernel, und der Ausführungszustand
liegt auf dem Heap statt auf einem Maschinenstack, daher lassen sich Tasks günstig in großer Zahl erzeugen.

**Tasks laufen gleichzeitig auf mehreren OS-Threads** (Parallelität auf mehreren Kernen). Die Anzahl der Threads
ist die Umgebungsvariable `TYPELISP_THREADS` (die Gesamtzahl, einschließlich des Threads, der `main` ausführt; der
Standard ist die Parallelität der Maschine). In `typl` laufen **nur kompilierte Tasks** auf anderen Threads, und
interpretierte Tasks laufen auf dem Thread des Interpreters (12.7). Geteilte Daten gehen über `Mutex<T>` oder
`Chan<T>`; gleichzeitiges Lesen und Schreiben, das das nicht tut, ist wie in Go undefiniert (12.7).

Vom Vokabular **sind nur `task` / `thread` / `select` Spezialformen**; der Rest sind gewöhnliche Funktionen,
Methoden und Makros ([Tasks und Kanäle](functions/concurrency.md)).

### 12.1 `task` — einen Task starten

```lisp
(task (f arg...))                   ; gibt Task<T> zurück, wobei T der Rückgabetyp von f ist
```

**Es nimmt nur die Form eines Aufrufs.** `f` und jedes `arg` werden dort ausgewertet, wo das `task` steht, in der
geschriebenen Reihenfolge, und nur **der Aufruf** geschieht im neuen Task. Das ist dieselbe Regel wie bei Gos
`go f(x)`, und deshalb nimmt es auch eine Aufrufform statt eines Thunks: Ein Thunk würde seine Argumente einfangen,
ohne sie auszuwerten.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i wird jedes Mal an Ort und Stelle ausgewertet; keine Einfangfalle

(task ((lambda () ()                ; um einen beliebigen Rumpf auszuführen, ein lambda aufrufen
         (println "start")
         (send ch 1))))
```

Spezialformen (`if` / `let` / `progn` …) lassen sich nicht direkt unter `task` schreiben.

**Warum es keine Funktion sein kann**: `(spawn (lambda () T body...))` zu schreiben, würde verlangen, `T`
auszuschreiben, da `lambda` eine Annotation des Rückgabetyps verlangt, und ein Makro kennt den Rückgabetyp von
`(f a b)` nicht. Nur die Prüfung kennt ihn.

### 12.2 `thread` — einen Task auf einem eigenen OS-Thread starten

```lisp
(thread (f arg...))                 ; gibt Thread<T> zurück, wobei T der Rückgabetyp von f ist
(join th)                           ; wartet auf den Abschluss und gibt seinen Wert zurück (beliebig oft)
```

Form und Auswertungsregeln sind dieselben wie bei `task` (es nimmt nur eine Aufrufform, und `f` und `arg` werden
dort ausgewertet, wo es steht). Der Unterschied ist, wo es läuft: **Es startet einen OS-Thread, der diesem Task
gewidmet ist, und läuft nur darauf**. Es wird nicht mit anderen Tasks gemultiplext, daher hält der Aufruf einer
blockierenden C-Funktion (`defffi`) darin nur diesen Thread an, und andere Tasks kommen voran. Darin lassen sich
`task`, `send`, `recv` und der Rest unverändert verwenden.

- `Thread<T>` ist das Gegenstück zu `Task<T>`. Wie `wait` hält `join` **den aufrufenden Task** an, und der Wert wird
  zwischengespeichert. Wenn der Task endet, endet auch der Thread.
- Die Panic-Regeln sind dieselben wie bei `task` (der ganze Prozess geht unter). Kehrt `main` zurück, endet der
  Prozess.
- Um es als Funktion zu schreiben, verwendet man `(Thread::spawn (lambda () T body...))` (Rusts
  `std::thread::spawn`). Der übergebene Funktionswert muss kompiliert sein. Von einer obersten Ebene aus
  aufgerufen, die `typl` interpretiert, löst es vor dem Start des Threads einen Panic aus, genauso wie ein
  `(panic ...)`, ob ein `lambda` oder eine benannte Funktion übergeben wird. Aus kompilierten Funktionen heraus
  ist es verwendbar.
- **Auf einem eigenen Thread läuft nur kompilierter Code.** Wertet `typl` beim Interpretieren `(thread (f ...))`
  aus, kompiliert es `f` (und was es aufruft) an Ort und Stelle, bevor es ausgeführt wird. Ein Aufruf, der sich
  nicht kompilieren lässt (der Wert eines interpretierten `lambda`, die Konstruktion einer Struktur usw.), ist vor
  dem Start des Threads ein Panic, der wie ein `(panic ...)` behandelt wird.

### 12.3 `select` — auf mehrere Kanaloperationen gleichzeitig warten

```lisp
(select
  ((v (recv ch1)) body...)          ; ein Empfangszweig. v wird an ein Option<T> gebunden
  ((send ch2 x) body...)            ; ein Sendezweig
  (else body...))                   ; optional. **wenn geschrieben, steht es am Ende**
```

- **Mit `else` blockiert es nicht** (Gos `default`). Ohne wartet es, bis einer möglich wird.
- **Sind mehrere gleichzeitig möglich, wird einer zufällig gewählt** (in geschriebener Reihenfolge würden spätere
  Zweige verhungern).
- Das `v` eines Empfangszweigs ist ein **`Option<T>`**. Ein geschlossener Kanal ist „eine Antwort“, kein Grund,
  den Zweig zu überspringen, daher wendet man innerhalb des Zweigs `match` darauf an.
- Der Typ ist **die Vereinigung der Typen aller Zweigrümpfe** (dieselbe Regel wie bei `match`-Zweigen).
- `(select)` mit null Zweigen ist ein Typfehler (Gos `select{}`, das ewig blockiert, wird nicht übernommen). Ein
  `select` nur mit `else` ebenfalls, da es dasselbe ist, wie den Rumpf direkt zu schreiben.

**Die Kanalausdrücke und die zu sendenden Werte werden jeweils einmal von links nach rechts ausgewertet, welcher
Zweig auch gewählt wird** (dieselbe Disziplin, die `case` für seine Schlüssel hat).

```lisp
(select                             ; Empfangen mit Timeout
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([ein Kanal, der nach einer Zeit liefert](functions/concurrency.md#5-after--ein-kanal-der-nach-einer-zeit-liefert))
ist „ein Kanal, der nach `sec` Sekunden einen Wert liefert“, entsprechend Gos `time.After`.

### 12.4 Zusammenspiel mit anderen Funktionen

| Funktion | Wie sie mit Tasks zusammenhängt |
|---|---|
| `catch` / `throw` | **Überschreiten keine Task-Grenzen.** Ein `throw`, das den Rumpf eines Tasks verlassen will, ist ein Panic |
| `unwind-protect` | Die Aufräumarbeit läuft, wenn ein Task natürlich endet. **Sie läuft nicht, wenn der Prozess endet, weil der Haupttask geendet hat** |
| `block` / `return-from` | Lexikalisch, daher überschreiten sie keine `lambda`-Grenzen |
| `panic` | Wie in Go geht der ganze Prozess unter. `wait` beobachtet einen Panic nicht als Wert |
| `dlet` | **Keine Bindung pro Task.** Es „leiht und gibt eine globale Variable zurück“, daher beeinflussen sich Tasks gegenseitig |
| Standardausgabe | Von allen Tasks geteilt. Die Ausgabe eines `println` wird nie mitten in der Zeile mit anderen vermischt |
| `compile` / `eval` | Keine Einschränkungen. `(compile f)` innerhalb eines Tasks funktioniert |

### 12.5 Wo Tasks wechseln

Die Planung ist kooperativ, daher **wechseln Tasks nur dort, wo man es schreibt**: `(yield)`, `(sleep ...)`,
`(wait ...)`, **Kanaloperationen, die warten müssen** (`send`/`recv`/`select`), und **Socket-Operationen, die
warten müssen** (`accept` / `tcp-connect` (einschließlich Namensauflösung) / Lesen und Schreiben von Sockets /
`recv-from`; [Netzwerk](functions/network.md)). Alle Sockets sind nicht blockierend: Ist einer nicht bereit,
hält nur dieser Task an, und er läuft weiter, wenn das BS meldet, dass er bereit ist, dieselbe Form wie Gos
netpoller. Nur wenn kein Task laufen kann, wartet die Implementierung beim BS bis zur nächsten `sleep`-Frist.

Kanaloperationen, die sofort antworten können (ein `send` mit Platz im Puffer, ein `recv` mit wartendem Wert,
`(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`), **verbrauchen den Zug nicht**. Das bedeutet, dass man nicht
unerwartet durch einen Lesevorgang unterbrochen wird, und es wird anders behandelt als `(sleep 0.0)`, das CLs „für
0 Sekunden den Vortritt lassen“ ist.

**Es gibt keine Verdrängung.** Eine enge Schleife, die nichts aufruft, lässt andere Tasks verhungern. Kompilierte
Schleifen geben die Kontrolle jedoch regelmäßig an den Scheduler ab, daher lässt eine kompilierte enge Schleife
sie nicht verhungern.

### 12.6 Kompilierter Code und Tasks

Auch kompilierter Code kann Tasks anhalten. Dasselbe gilt für mit `compile-file` erzeugte ausführbare Programme:
`main` läuft als Haupttask des Schedulers, und `task`, `sleep`, `wait`, Kanäle und das Warten auf Sockets
funktionieren alle mit derselben Bedeutung wie in `typl`. Kehrt `main` zurück, endet der Prozess, und die
verbleibenden Tasks werden abgeschnitten (wie in Go). Der Interpreter wird nie um des Schedulers willen in das
ausführbare Programm aufgenommen.

Die eine Ausnahme ist „innerhalb eines C-FFI-Callbacks“, wo Operationen, die **warten müssten**, Fehler sind
(freundlicher als stillschweigend zu verklemmen): Während eine mit `defffi` übergebene Funktion von C aus
aufgerufen wird, liegt Cs Stack obenauf, und es gibt keine Möglichkeit, den Task anzuhalten und später
fortzusetzen.

Die folgenden Stellen sind ebenfalls Funktionen, die mitten in einem Task aufgerufen werden, und können dennoch
nicht anhalten: `print-object`-Methoden, `~/name/` in `format`, Lesemakros, das Innere von `eval` und
`defvar`-Initialisierer in AOT-Programmen. Hier **gehen Operationen durch, die ohne Warten antworten**
(`(recv ch)` mit einem Wert im Puffer, `read-line` auf einem Socket mit bereits empfangenen Daten, `(task ...)`,
`(yield)` usw.), und **Operationen, die wirklich warten müssten, sind Fehler** (nicht ein sofortiges Anhalten des
Prozesses, sondern ein Panic wie `` `recv` cannot block: ... ``, der wie ein `(panic ...)` behandelt wird).

### 12.7 Unterschiede zu Go

- **In `typl` gehen nur kompilierte Tasks auf andere Threads.** Der Zustand des Interpreters lässt sich nicht
  zwischen Threads teilen, daher laufen Tasks aus einem interpretierten `task` auf dem Thread des Interpreters.
  Auch ein kompilierter Task **wechselt auf den Thread des Interpreters und bleibt dort** (er kehrt nicht zurück),
  sobald er einen interpretierten Funktionswert aufruft, eine `:dyn`-Methode aufruft, die niemand kompiliert hat,
  oder `eval`/`macroexpand`/`read` aufruft. Berührt eine lange Berechnung unterwegs auch nur einmal
  interpretierten Code, läuft der Rest auf dem Thread des Interpreters.
- **In `typl` leben die Arbeiter nur für eine Auswertung auf oberster Ebene.** Während die REPL auf Eingabe wartet
  und zwischen Formen auf oberster Ebene bringen andere Threads keine Tasks voran (verbleibende Tasks machen bei
  der nächsten Auswertung dort weiter, wo sie aufgehört haben). Am Ende einer Auswertung wartet es darauf, dass
  jeder Thread seinen aktuellen Schritt beendet; blockiert eine C-Funktion (`defffi`) innerhalb eines `thread`
  fortwährend, endet die Auswertung daher erst, wenn sie zurückkehrt.
- **Ausgabe auf Arbeitern**: Interpretierte `print-object`- / `~/name/`-Methoden können nicht auf anderen Threads
  laufen, daher ist die Ausgabe solcher Werte auf einem anderen Thread ein Panic, der wie ein `(panic ...)`
  behandelt wird (`(compile T::print-object)` oder aus dem Haupttask ausgeben).
- **Datenwettläufe sind undefiniert** (dieselbe Position wie Go). Das Ergebnis, wenn mehrere Tasks denselben Wert
  ändern, ohne über `Mutex<T>` / `Chan<T>` zu gehen, ist nicht garantiert.
- **`task` gibt einen Wert zurück.** Anders als Gos `go`-Anweisung gibt es ein `Task<T>` zurück, und `(wait t)`
  holt das Ergebnis.
- **Es gibt keine nil-Kanäle.** Gos Fan-in-Idiom (einen geschlossenen Kanal auf `nil` setzen, um ihn aus den
  Zweigen von `select` zu entfernen) lässt sich nicht schreiben; man startet daher pro Eingabe einen Task und führt
  sie mit einer `WaitGroup` zusammen ([WaitGroup](functions/concurrency.md#4-waitgroup--auf-n-abschlüsse-warten)).
  Das ist auch in Go der empfohlene Weg, aber es ist **der erste Unterschied, auf den Umsteiger von Go stoßen**.
