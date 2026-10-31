<!-- translated-from: docs/ja/reference/syntax.md @ 1ba918af8af98a319b34010b9dc04df8c4d321e0 -->
# Syntaxreferenz von typelisp

typelisp ist ein statisch typisiertes Lisp, geschrieben in S-Ausdrücken. Die Liste der eingebauten Funktionen
und Methoden steht unter [Eingebaute Funktionen](functions/README.md), die Liste der Typen in
[types.md](types.md) und wie man Fehlermeldungen liest in [errors.md](errors.md).

## 1. Lexikalische Elemente

- **Groß-/Kleinschreibung wird unterschieden.** `foo` und `Foo` sind verschiedene Symbole. Typnamen beginnen mit einem Großbuchstaben (`Option`, `Vector` usw.; Grundtypen wie `int` und `string` werden klein geschrieben).
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
- **Brüche**: `Zähler/Nenner` (zum Beispiel `1/3`; auch mit Basispräfix wie `#x1/F`). Beim Lesen gekürzt, wie CL es vorschreibt
  (`2/4` ist `1/2`). Solche mit ganzzahligem Wert (`4/2` usw.) werden als `int` gelesen, nicht als `ratio`. Ein
  Nenner null (`1/0`) ist ein Lesefehler.
- **Zeichen**: `#\` gefolgt von einem Zeichen oder einem Zeichennamen. Zum Beispiel `#\a` `#\Space`
  `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul` (auch `#\Null`) `#\Backspace`. Bei Namen wird
  Groß-/Kleinschreibung nicht unterschieden.
- **Zeichenketten**: `"..."`. Die Maskierungen sind `\n` `\t` `\r` `\0` `\\` `\"` (jedes andere `\x` ist einfach
  `x`).
- **Symbole**: jedes Token aus Buchstaben, Ziffern und Sonderzeichen (`+` `<=` `my-func` usw.).
  `]` und `}` beenden ein Token und können daher nicht in einem Symbol stehen; steht eines am Anfang
  eines Datums, ist das ein Lesefehler. `[` und `{` können in einem Symbol stehen: Wie in CL bleiben
  sie frei, damit Programmierer sie für [Lesemakros](#11-lesemakros-readtable) verwenden können.
  Was zwischen `|` und `|` steht, wird so gelesen, wie es geschrieben ist, einschließlich Trennzeichen und
  Doppelpunkten (CLs Mehrfach-Escape): `|:foo|` ist ein gewöhnliches Symbol mit dem Namen `:foo`, kein
  Schlüsselwort.
- **Schlüsselwörter**: Symbole, die mit einem Doppelpunkt beginnen, wie `:name` (wie in CL). In welchem Modul
  sie auch geschrieben werden, sie sind das eine Symbol dieses Namens im Schlüsselwort-Namensraum `keyword` (CLs
  Paket `KEYWORD`). Sie werten zu sich selbst aus: Sie suchen keine Bindung, ihr Wert sind sie selbst, mit dem
  statischen Typ `symbol`. Schlüsselwörter gleichen Namens sind immer dasselbe Objekt (wie andere Symbole unterscheiden sie Groß- und Kleinschreibung, also ist `(eq :foo :FOO)` falsch). Der Doppelpunkt ist nicht Teil des Namens:
  `(std::symbol::symbol->string :foo)` ist `"foo"`. `~s` druckt `:foo`, `~a` druckt `foo`. Ein Schlüsselwort ist eine
  Konstante, deren Wert es selbst ist; es kann daher weder als Variable gebunden oder zugewiesen werden noch der
  Name einer Definition sein (ein Typfehler). Aus einer Zeichenkette erzeugt man es mit
  `(intern "foo" "keyword")`. Geprüft wird mit `keywordp` (ob es zum Schlüsselwort-Namensraum gehört). Ein
  einzelnes `:` oder eines mit weiteren Doppelpunkten wie `:a:b` ist ein Lesefehler. Was mit `::` beginnt, ist
  kein Schlüsselwort, sondern ein absoluter Pfad (unten).
  Man beachte, dass `:dyn` ein reserviertes Schlüsselwort nur für Typstellen ist; es anderswo zu schreiben, ist
  ein Fehler (siehe [Kapitel 2](#2-typen-schreiben)).
- **Listen**: `(a b c)`. Auch gepunktete Paare `(a . b)` lassen sich lesen.
- **Vektoren**: `#(1 2 3)` (wie in CL). Der Inhalt besteht nur aus Literalen und wird nicht
  ausgewertet: Das `a` in `#(a b)` ist ein Symbol, keine Variable. Der Elementtyp ergibt sich aus
  dem Kontext (`(the Vector<i32> #(1 2))`) oder, ohne Kontext, aus dem ersten Element (`#(1 2 3)`
  ist ein `Vector<int>`). Alle Elemente müssen denselben Typ haben: `#(1 "a")` ist ein Typfehler,
  ebenso `#()` ohne Elemente und ohne Kontext. Jede Auswertung erzeugt einen neuen Vektor. Wo
  S-Ausdrucksdaten erwartet werden (`(the Option<Sexpr> #(1 x))`, `'#(..)`, was `read` liefert), ist
  er ein `Vector<Option<Sexpr>>`, dessen Elemente alle Daten sind: die Variante `vector` von
  `Sexpr`.
- **Arrays**: `#2A((1 2) (3 4))` (wie in CL). Die Zahl zwischen `#` und `A` ist der Rang, und die
  ersten so vielen Ebenen der Listenverschachtelung im Inhalt sind die Dimensionen. `#0A x` ist ein
  nulldimensionales Array mit einem Element. Listen derselben Ebene mit unterschiedlicher Länge sind
  ein Lesefehler. Der Typ wird wie bei Vektoren bestimmt und ist ein `Array<T>` (ohne Elemente muss
  der Kontext ihn liefern, etwa `(the Array<f64> #2A(()))`). Als S-Ausdrucksdaten ist es ein
  `Array<Option<Sexpr>>`: die Variante `array` von `Sexpr`.
- **Tupel**: `#{"foo" 123 45.6}`. Der Inhalt besteht ausschließlich aus Literalen und wird nicht
  ausgewertet (das `a` von `#{a b}` ist ein Symbol). Der Typ jedes Elements wird Position für
  Position vom Kontext bestimmt (`(the #{i32 f64} #{1 2.0})`), ohne Kontext ist es der eigene Typ
  des Elements (`#{"foo" 123}` ist `#{string int}`). Ein Tupel hat 1 bis 12 Elemente. Jede
  Auswertung erzeugt ein neues Tupel. Um eines aus berechneten Werten zu bauen, verwendet man
  `(tuple a b)`. Wo S-Ausdrucksdaten erwartet werden, ist es ein Tupel, dessen Elemente alle Daten
  sind: die Variante `tuple` von `Sexpr`.
- **Attribute**: `#[test]` ist eine Markierung, die an der nächsten Form hängt (wie in Rust geschrieben). Das bisher einzige Attribut ist `test`, und es gehört an ein `defun` ([3.16 `#[test]`](#316-test--testfunktionen)). Mit `read` gelesen wird daraus die Liste `(%internal::attribute (test) form)`.
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
- **Die rationalen Typen**: `ratio` (gekürzte rationale Zahlen) und `rational` (ein `int` oder ein
  `ratio`, CLs rational). `/` auf zwei ganzen Zahlen und Arithmetik, die ein `int` mit einem `ratio`
  mischt, ergeben ein `rational`, und `int`/`ratio`-Werte können unverändert übergeben werden, wo ein
  `rational` erwartet wird. Es gibt keine implizite Umwandlung mit den Ganzzahlen fester Breite oder
  `f64` (explizit mit `as`/`try-as` oder einer Umwandlungsmethode; siehe [Zahlen](functions/numbers.md#5-rationale-zahlen-ratio)).
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
- **Generische Typen**: `Name<T1,T2,...>`. Zum Beispiel `Option<i32>` `Result<i32,ParseIntError>`
  `HashTable<string,i32>` `Vector<T>`. Folgt auf ein `<` direkt nach einem Namen ein Typ, liest der
  Leser bis zum passenden `>` Typargumente. Darin dürfen Leerzeichen und Zeilenumbrüche trennen
  (`HashTable<string, int>`), und der Unit-Typ `()` (`Result<(), FileError>`), `:dyn Trait` und
  Tupeltypen lassen sich schreiben. Folgt auf das `<` kein Typ, gehört dieses `<` zum Namen (`<=`,
  `string<`). Typargumente, die nicht mit `>` geschlossen sind oder nicht wie Typen aussehen, sind
  ein Lesefehler (auch `(a<b c)` ist ein Fehler, in den Typargumenten `b c`). `()` lässt sich auch
  als Feld- oder Argumenttyp verwenden.
- **Die Anwendungsform generischer Typen**: `(Name T1 T2 ...)`, eine Listenschreibweise, die
  denselben Typ benennt wie `Name<T1,T2,...>`. Zum Beispiel ist `(vector char)` dasselbe wie
  `Vector<char>`. Die Namensform ist die übliche Schreibweise; diese Form **existiert für den Fall,
  dass sich ein Typargument nicht innerhalb eines Namens schreiben lässt**: Ein Typargument ist
  selbst ein Typausdruck, aber innerhalb von `<..>` lassen sich nur Namen, `()`, `:dyn` und
  Tupeltypen schreiben, keine Funktionstypen (`Vector<(fn (i32) i32)>` ist ein Lesefehler; gibt man
  dem Funktionstyp mit `deftype` einen Namen, lässt sich `Vector<F>` schreiben). In dieser Form kann
  ein Typ auch erscheinen, wenn die Implementierung ihn anzeigt, etwa als Ergebnis des Einsetzens
  des assoziierten Typs eines Traits in eine Signatur.
- **Tupeltypen**: `#{int string}` (dieselbe Form wie die Werte). Ein Tupel hat 1 bis 12 Elemente,
  und ein Tupeltyp kann auch Typargument sein (`Vector<#{int string}>`). Elemente liest man mit
  `t::0` `t::1` und ändert sie mit `(setf t::0 v)`. `Eq`, `Ord` (Vergleich ab dem ersten Element),
  `Hash` und `print-object` stehen zur Verfügung, wenn alle Elementtypen sie implementieren.
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
- Eingebaute generische Typen: `Option<T>` (`(some T)` / `none`), `Result<T,E>` (`(ok T)` / `(err E)`),
  `HashTable<K,V>`, `Vector<T>` und die Nebenläufigkeitstypen `Task<T>` / `Thread<T>` / `Chan<T>`
  ([Kapitel 12](#12-nebenläufigkeit-tasks)). Außerdem gibt es `Sexpr`, den Typ von S-Ausdrucksdaten. Die
  eingebauten konkreten Fehlertypen sind `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` /
  `FileError` / `NetError`, und die Standardbibliothek hat die Strukturen `SimpleError` / `WrappedError`
  (`Error` ist kein Typ, sondern ein Trait: Man verwendet ihn als `:dyn Error`). Die Liste steht in
  [types.md](types.md).
- **Typen und Traits teilen sich einen Namensraum** (wie in Rust): Innerhalb eines Moduls können ein Typ
  (`defstruct`/`defenum`) und ein Trait (`deftrait`) nicht denselben Namen haben.

## 3. Definitionen auf oberster Ebene

Die Namen der Spezialformen (reservierte Wörter wie `if`, `let` und `step`, die in jedem Modul dasselbe bezeichnen) können nicht als Namen von Funktionen, Makros, globalen Variablen, Typen oder Traits verwendet werden. Eine solche Definition ist ein Fehler.

### 3.1 defun — Funktionsdefinitionen

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Argumenttypen und Rückgabetyp sind Pflicht.
- Eine generische Funktion schreibt ihre Typparameter in spitzen Klammern nach ihrem Namen:
  `(defun name<T1,T2...> (params) Ret body...)` (dieselbe Spitzklammersyntax wie `Vector<T>` an Typstellen).
- `defun`/`lambda`/Methoden in `impl` akzeptieren variadische Argumente, wenn am Ende `&rest (name Type)` steht:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (im Rumpf ist `xs` immer als `Option<Sexpr>`
  gebunden, eine S-Ausdrucksliste. Jedes tatsächliche Argument wird beim Aufruf einzeln als `Type2` typgeprüft
  und dann in ein `Sexpr` gehüllt).
  `defmacro` hat ebenfalls ein eigenes `&rest`, unterscheidet sich aber darin, dass es immer ein untypisiertes
  `Sexpr` ist (`defun`/`lambda` geben den Elementtyp an). Ein Funktionstyp kann auch eine variadische Funktion
  beschreiben, als `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (für `defun` und Methoden in `impl`; nicht für `lambda`/`labels`, aus dem unten genannten
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
  - **Methoden in `impl` können dieselben drei Abschnitte haben** (sowohl Instanzmethoden als auch
    statische Methoden). Eine Instanzmethode schreibt `&optional`/`&rest`/`&key` nach `self`:

    ```lisp
    (defstruct box (w i32) (h i32))
    (impl box
      (grow (self &key (dw i32 0) (dh i32 0)) i32 ...))
    (grow (make-instance 'box :w 1 :h 2) :dh 10)

    (defstruct point (x i32) (y i32))
    (impl point
      (origin (&key (x i32 0) (y i32 0)) point (make-instance 'point :x x :y y)))   ; statische Methode
    (point::origin :y 7)
    ```

    Sie lassen sich auch in Methoden generischer Typen verwenden, aber **der Typ eines Parameters mit
    Standardausdruck kann die Typparameter des Besitzers nicht erwähnen** (dieselbe Einschränkung, die `defun`
    für seine eigenen Typparameter hat: Was beim Weglassen des Arguments eingebettet wird, ist ein *geprüfter*
    Ausdruck, daher kann sein Typ nicht als abstrakte Variable stehen bleiben).
  - **In Trait-Methoden lassen sie sich nicht verwenden.** Das gilt auch für die Methoden, die im `impl` eines Traits stehen. `deftrait` hat keine Syntax dafür, und könnte nur
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
- Um Trait-Schranken zu verlangen, schreibt man direkt vor den Rumpf eine `:where`-Klausel:
  `(defun name<T> (params) Ret (:where (Trait T (AssocName ConcreteType)...)) body...)`
  (einen assoziierten Typ mit `(AssocName ConcreteType)` festzulegen, ist optional).
- **Docstrings**: Ein Zeichenkettenliteral am Anfang des Rumpfes, direkt nach der `:where`-Klausel (falls
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
  abgeschlossen ist wie eine Signatur. Die Methoden eines `impl` werden an dem Typ registriert, dem sie gehören, und folgen daher dem Typ.

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

### 3.5 impl — Methoden eines Typs

```lisp
(impl Type
  (name ((arg Type2) ...) RetType body...)            ; statische Methode: (Type::name args...)
  (name (self (arg Type2) ...) RetType body...)       ; Instanzmethode: (name obj args...)
  (pub name ...))                                      ; auch außerhalb des Moduls des Typs öffentlich

; ein generischer Typ
(impl Type<T> (:where (Trait T)...) item...)
```

Gibt einem mit `defstruct` (3.6) oder `defenum` (3.7) definierten Typ Methoden. Entspricht dem
inhärenten impl von Rust (`impl Point { ... }`).

- Ein Eintrag hat die Form `(Name (Parameter...) Rückgabetyp Rumpf...)`, dieselbe wie ein Eintrag im
  `impl` eines Traits (3.9).
- Ein Eintrag, dessen erster Parameter `self` ist, ist eine **Instanzmethode** und wird als
  `(name obj args...)` aufgerufen. `self` wird ohne Typ geschrieben
  (sein Typ ist `Self`, der Zieltyp; `(self Type)` ist nicht erlaubt).
- Ein Eintrag ohne `self` ist eine **statische Methode** und wird als `(Type::name args...)` aufgerufen.
  Eine statische Methode, die einen Wert erzeugt, ist der Konstruktor dieses Typs. Der Name ist frei;
  empfohlen wird `new`.
- **Die Sichtbarkeit gilt pro Eintrag.** Nur Einträge, die als `(pub name ...)` geschrieben sind, können
  außerhalb des Moduls verwendet werden, das den Typ definiert. Ohne `pub` ist ein Eintrag nur innerhalb
  dieses Moduls verwendbar.
- Ein Typ kann beliebig viele `impl` haben.
- Der Aufrufer löst die Methode über den statischen Typ von `obj` auf (einfacher, statischer Dispatch).
  Eine Methode wird **nur über den Typ des ersten Arguments** gesucht (3.10).
- **Docstrings** können an derselben Stelle und nach denselben Regeln wie bei `defun` stehen (am Anfang
  des Rumpfs, nur wenn noch Rumpfformen folgen). Abgerufen werden sie mit `(documentation Type::name)`.
  Dasselbe gilt für die Methoden im `impl` eines Traits.
- Nur die eigenen Typen des Programms können eines haben. Ein Typ der Standardbibliothek wie `Vector`,
  `Option` oder `int` kann es nicht (der Fehler lautet `... is a standard library type`). Um einem
  Standardtyp eine Operation hinzuzufügen, definiere einen Trait und schreibe dessen `impl` für diesen Typ.
- Folgt in `(impl X ...)` auf `X` ein Eintrag (eine Liste), ein `:where` oder nichts, ist es dieses
  inhärente impl; folgt ein Name, ist es das `impl` eines Traits (`(impl Trait Type ...)`, 3.9).

```lisp
(defstruct Point (pub x f64) (pub y f64))

(impl Point
  (pub new ((x f64) (y f64)) Point
    (make-instance 'Point :x x :y y))
  (pub origin () Point
    (make-instance 'Point :x 0.0 :y 0.0))
  (pub area (self) f64
    (* self::x self::y)))

(Point::new 1.0 2.0)          ; statische Methode
(area (Point::new 2.0 3.0))   ; Instanzmethode. => 6.0
```

Die eigenen Typparameter einer Methode werden wie bei `defun` mit `<...>` in ihren Namen geschrieben.
Die Typparameter des Zieltyps (`T` unten) werden durch den Empfänger festgelegt; die eigenen der Methode
(`U`) werden aus den Argumenten jedes Aufrufs abgeleitet.

```lisp
(defstruct Box<T> (v T))

(impl Box<T>
  (fmap<U> (self (f (fn (T) U))) Box<U>
    (make-instance 'Box :v (f self::v))))

(fmap (make-instance 'Box :v 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- Gib den eigenen Typparametern der Methode Namen, die sich von den im `impl` geschriebenen
  Typparametern des Zieltyps unterscheiden (dem `T` von `(impl Box<T> ...)`).
- Eine Methode im `impl` eines Traits kann keine Typparameter hinzufügen: Ihre Signatur folgt der, die
  der Trait deklariert.

Das `impl` eines generischen Typs schreibt alle seine Typparameter als Variablen (`Box<T>`, allen
Instanziierungen gemeinsam) oder alle als konkrete Typen (`Box<int>`, nur für diese Instanziierung). Die
`impl`s verschiedener Instanziierungen sind getrennte Implementierungen, daher können `Box<int>` und
`Box<string>` je eine Methode desselben Namens haben.

```lisp
(defstruct Box<T> (v T))

(impl Box<int>
  (twice (self) int (* 2 self::v))
  (make () Box<int> (make-instance 'Box :v 7)))
(impl Box<string>
  (twice (self) string (format false "~a~a" self::v self::v))
  (make () Box<string> (make-instance 'Box :v "s")))

(twice (make-instance 'Box :v 4))      ; => 8
(twice (make-instance 'Box :v "ab"))   ; => "abab"
(twice (the Box<int> (Box::make)))     ; => 14
```

- Eine Instanzmethode wird nach dem Typ des Empfängers gewählt. Eine Instanziierung ohne `impl` (etwa
  `Box<f64>`) hat die Methode nicht.
- Eine statische Methode wird nach dem Typ gewählt, den der Aufruf zurückgeben soll (das `(the Box<int> ...)`
  oben). Hat nur eine Instanziierung eine statische Methode dieses Namens, ist kein erwarteter Typ nötig.
- Eine Methode desselben Namens kann nicht zugleich in einem `impl` mit Variablen (`Box<T>`) und im `impl`
  einer Instanziierung (`Box<int>`) stehen; in beiden Reihenfolgen ist das ein Fehler. Der Accessor eines
  Felds zählt als Methode des `impl` mit Variablen.
- Um eine solche Methode in `documentation`, `compile`, `trace` und Ähnlichem zu benennen, schreibt man die
  Instanziierung: `Box<int>::twice`. Hat nur eine Instanziierung die Methode, geht auch `Box::twice`.

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
  selbst). Ein weiterer Ausdruck am Ende wird zum **Standardwert** des Slots (`(x i32 0)`).
- Werte werden mit `make-instance` erzeugt (unten). Ein Konstruktor, der die Felder in ihrer Reihenfolge
  nimmt, wird nicht erzeugt. Wer einen Konstruktor möchte, schreibt ihn als statische Methode in `impl`
  (3.5).
- Folgendes wird automatisch erzeugt:
  - Leser `(field-name instance)`, mit der Kurzform `instance::field-name`
  - Schreiber `(set-field-name instance value)`, mit der Kurzform `(setf instance::field-name value)`
- Um die Struktur selbst `pub` zu machen, stellt man `pub` voran, wie in `(pub defstruct ...)`.
- **Einen Typ definiert man, bevor man ihn nennt.** Der Typ eines Feldes kann die Struktur selbst sein
  (`(next Option<node>)`), aber kein später definierter Typ: Typen haben keine Vorwärtsdeklaration, die
  `defsignature` entspräche. Ein noch nicht definierter Name ergibt in einem `defun`-Argumenttyp oder in `the`
  denselben Fehler `unknown type`. Zwei Typen, die sich gegenseitig verweisen, lassen sich daher nicht
  schreiben.
- **Typvariablen sind nur die an deklarierenden Stellen geschriebenen.** Bei
  `defun`/`defstruct`/`defenum`/`deftype` das `<T>` des Namens; bei einem inhärenten `impl` der Zieltyp
  (`(impl box<T> ...)`) und das `<U>` im Namen jeder Methode; beim `impl` eines Traits der Zieltyp und
  `impl<T>`; bei `deftrait` `Self` und die assoziierten Typen von `(:type Item)`. Ein Name, der zum ersten
  Mal an einer anderen Stelle auftaucht (Argumente, Rückgabewert, `the`/`lambda` im Rumpf), wird keine
  Typvariable; er ist `unknown type`.
- **Docstrings**: Ein Zeichenkettenliteral direkt nach dem Namen, vor den Feldern, wird zum Docstring
  (`(defstruct Name "doc" (field Type)...)`, dieselbe Position wie bei CLs `defstruct`). Ein Feld hat immer die
  Form `(name Type ...)` und kann nie eine bloße Zeichenkette sein, daher gibt es keine Mehrdeutigkeit.
  Abgerufen wird er mit `(documentation Name)`.

#### make-instance — einen Wert erzeugen

```lisp
(make-instance 'Name :field1 value1 :field2 value2 ...)
```

Ein Strukturwert wird wie in CLOS mit `make-instance` erzeugt.

```lisp
(defstruct point (pub x i32) (pub y i32 0))

(make-instance 'point :x 1 :y 2)
(make-instance 'point :y 2 :x 1)      ; beliebige Reihenfolge
(make-instance 'point :x 1)           ; y ist sein Standardwert 0
```

- Der Typname wird als mit `'` quotiertes Symbol geschrieben. Er wählt keinen Typ zur Laufzeit; er steht
  zur Prüfzeit fest (wie das `'vector` von `concatenate`).
- Felder werden als Paare `:name wert` angegeben. Die Werte werden je einmal in der geschriebenen
  Reihenfolge ausgewertet.
- Ein nicht angegebenes Feld erhält seinen Standardwert. Ein Feld ohne Standardwert nicht anzugeben ist
  ein Typfehler, ebenso ein unbekanntes Feld und dasselbe Feld zweimal.
- Die Typargumente einer generischen Struktur werden aus den Werten und dem erwarteten Typ abgeleitet.
  Man kann sie auch hinschreiben, wie in `'Box<int>`.

  ```lisp
  (defstruct Box<T> (v T))
  (make-instance 'Box :v 1)                    ; Box<int>
  (the Box<f64> (make-instance 'Box :v 1.5))   ; Box<f64>
  (make-instance 'Box<int> :v 7)
  ```

- Außerhalb des Moduls, das den Typ definiert, können Felder, die nicht `pub` sind, nicht angegeben
  werden. Hat ein solches Feld keinen Standardwert, können andere Module den Wert gar nicht erzeugen. Um
  es ihnen zu erlauben, schreibe einen `pub`-Konstruktor in `impl` (3.5).

#### Optionsliste

Eine Liste `(Name option...)` an der Stelle des Namens gibt Optionen an (dieselbe Stelle wie in CL).

```lisp
(defstruct (point (:copier copy-point))
  (x i32 0)          ; ein drittes Element ist der Standardwert dieses Slots
  (y i32 0))

(copy-point p)       ; eine flache Kopie (wie der copier von CL)
```

- **`:copier`**: erzeugt eine **Instanzmethode**, die einen neuen Wert mit denselben Slotwerten
  zurückgibt. Flach, wie der copier von CL.
- **`:include Parent`**: Stellt die Slots des Elterntyps voran (auch Standardwerte werden geerbt; der Elterntyp
  darf in einer anderen Datei stehen). **Es entsteht keine Typbeziehung**: Das Kind ist kein Untertyp des
  Elterntyps, die Methoden des Elterntyps gelten nicht für das Kind, und es gibt keine Laufzeitprüfung, die
  beide verbindet. Diese Sprache hat keine Untertypen; gemeinsame Schnittstellen sind die Aufgabe von
  `deftrait`. Nur die *Liste* der Slots wird zusammengefügt.
- Weggelassene Optionen und warum:
  - **`:constructor`**: Ein Konstruktor wird als statische Methode in `impl` geschrieben (3.5). Diese
    Option zu schreiben ist ein Fehler, der zeigt, wie man sie umschreibt.
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
  (some T)
  (none))
```

- Jede Variante hat die Form `(VariantName FieldType...)`. Felder sind nur positionell (sie haben keine Namen).
  Mindestens eine Variante ist nötig, und Namen dürfen sich nicht wiederholen.
- Werte werden wie bei den eingebauten `Option`/`Result` qualifiziert oder über `use` gebildet:
  `(Name::Variant1 a b)` oder nach `(use Name)` `(Variant1 a b)`.
- Sie lassen sich mit `match` / `if-let` zerlegen. `match` prüft die Vollständigkeit (es muss jede Variante
  abdecken oder ein `_` haben):
  ```lisp
  (match opt
    ((some v) v)
    ((none) 0))
  ```
- Methoden und statische Methoden werden wie bei `defstruct` in `impl` (3.5) geschrieben.
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
  (:type AssocName)                       ; assoziierte Typen (beliebig viele, optional)
  (method-name (self params...) RetType)          ; ohne Rumpf = muss implementiert werden
  (method-name (self params...) RetType body...)) ; mit Rumpf = Standardimplementierung

(impl TraitName TargetType
  (:where (Trait A)...)                   ; Schranken für das ganze impl (optional)
  (:type AssocName ConcreteType)          ; macht einen assoziierten Typ konkret
  (method-name (self params...) RetType body...))
```

Über `impl` wird jede Methode als Methode von `TargetType` registriert und wie eine Methode eines
inhärenten `impl` (3.5) aufgerufen. Die Methoden im `impl` eines Traits sind immer öffentlich, ohne `pub`.
Traits werden in den `:where`-Klauseln generischer Funktionen als Trait-Schranken referenziert (siehe
[3.1 defun](#31-defun--funktionsdefinitionen)). Ein Traitname kann auch ein `::`-Pfad wie `m::Trait` sein.

**Die Liste der Obertraits (Pflicht)**: steht immer direkt nach dem Trait-Namen. Jedes Element ist ein bloßer
Trait-Name oder, wenn dieser Trait assoziierte Typen hat, `(Trait (Assoc Type))` mit **allen seinen
assoziierten Typen festgelegt**.

```lisp
(deftrait Eq () ...)                       ; keine Obertraits
(deftrait Ord (Eq) ...)                    ; Rusts trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; einen assoziierten Typ festlegen
  (rewind (self) ()))
```

Vererbung hat drei Wirkungen. (1) `impl Ord X` verlangt, dass `impl Eq X` **vorher** geschrieben ist (eine Regel
über die Schreibreihenfolge: die einzige Form, die sich in der REPL und bei schrittweisem `load`
deterministisch entscheiden lässt, und strenger als Rust). (2) `(:where (Ord T))` allein erlaubt auch den Aufruf
der Methoden von `Eq`. (3) Die Methoden von `Eq` lassen sich über ein `:dyn Ord` aufrufen, und ein
`:dyn Ord`-Wert lässt sich unverändert übergeben, wo ein `:dyn Eq` verlangt wird (Upcast). Dass ein Untertrait
eine gleichnamige Methode seines Elterntraits neu deklariert und dass gleichnamige Methoden von zwei
Elterntraits geerbt werden, sind beides Fehler (eine vtable hat einen Eintrag pro Name). Rautenvererbung wird zu
einem Eintrag zusammengeführt.

**Standardimplementierungen**: Ein Rumpf nach der Signatur wird verwendet, wenn ein `impl` die Methode
weglässt. Der Rumpf wird im **Namensraum des Moduls** aufgelöst, in dem der Trait steht, daher kann er nicht
öffentliche Funktionen dieses Moduls aufrufen. Methoden mit Rumpf können auch `:where`-Klauseln und Docstrings
haben. Der Rumpf wird wie in Rust **einmal, an der Deklarationsstelle**, typgeprüft, mit `Self` als Typvariable
(beschränkt durch `Self: der Trait selbst`): Fehler, die für jedes `impl` und jeden implementierenden Typ
scheitern würden, werden dort gefunden, auch in Standardimplementierungen, die kein `impl` je weglässt.
Aufrufe auf `self` von Methoden des Traits selbst oder seiner Obertraits gehen durch diese Schranke, und
assoziierte Typen sind auf sich selbst festgelegt, daher wird eine Signatur, die `Item` zurückgibt, gegen den
Rumpf abgeglichen, ohne den konkreten Typ zu kennen.

**Implementierungen je Instanziierung**: Ein generischer Typ kann für jede Instanziierung ein eigenes
`impl` haben (`(impl Show Box<int> ...)` und `(impl Show Box<string> ...)`). `:dyn`, `:where`-Schranken,
assoziierte Typen und `print-object` wirken je Instanziierung, und eine Instanziierung ohne `impl`
(`Box<f64>`) implementiert den Trait nicht. Derselbe Trait kann nicht zugleich für `Box<T>` und `Box<int>`
implementiert werden (dieselbe Regel wie für Methoden in 3.5). Ein Supertrait muss nur an dieser
Instanziierung implementiert sein (durch ein `impl` von `Box<int>` oder von `Box<T>`).

**Pauschalimplementierungen**: Macht man das Ziel zu einer Typvariable, wird der Trait auf einen Schlag für jeden
Typ implementiert, der die Schranken erfüllt.

```lisp
(deftrait Clamp (Ord)
  (clamp (self (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (:where (Ord T)))          ; überhaupt kein Rumpf; alles ist die Standardimplementierung
```

**Es wird kein Code erzeugt, bis ein konkreter Typ ihn tatsächlich verwendet** (einmal pro Typ, nach demselben
Mechanismus wie die gewöhnliche Monomorphisierung). Ein Trait kann höchstens eine Pauschalimplementierung haben.
Hat ein Typ ein ausdrückliches `impl`, hat dieses Vorrang. Die Typprüfung des Rumpfes ist von der Erzeugung
getrennt: Sie geschieht wie in Rust einmal an der Deklarationsstelle, **mit dem Ziel als Typvariable**, daher
werden selbst bei einer Implementierung, die nie verwendet wird, Fehler dort gefunden, wenn sie unter den
deklarierten Schranken für jedes Ziel scheitern würden. Durch die Schranken gerechtfertigte Aufrufe
(`(less self other)` unter `(:where (Ord T))` usw.) gehen durch, wie im Rumpf eines generischen `defun`.

**Docstrings**: Ein `deftrait` kann einen Docstring für den ganzen Trait haben, als Zeichenkettenliteral direkt
nach der Liste der Obertraits, vor den Einträgen (`(deftrait Name () "doc" (:type ...) (method ...)...)`). Eine
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
(deftrait Drawable () (draw (self) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw (self) string "circle"))
(impl Drawable square (draw (self) string "square"))

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
(use Type::member)         ; einen Konstruktor oder eine statische Methode eines Typs unter dem bloßen Namen verwendbar machen
(use Type::*)              ; alle Konstruktoren und öffentlichen statischen Methoden eines Typs unter bloßen Namen verwendbar machen
(pub use path...)          ; Hereingeholtes auch über den Pfad dieses Moduls erreichbar machen
(import path...)           ; dasselbe wie use (eine CL-kompatible Schreibweise)
(shadowing-import path...) ; ein use, das bewusst einen bereits verwendeten bloßen Namen nimmt
(shadow name...)           ; macht einen Namen, den die Standardbibliothek anbietet, zum eigenen Namen dieses Moduls
```

- `module` erzeugt einen Namensraum. **Typen sind keine Namensräume** (wie in Rust hat ein Typ nur statische
  Methoden und Methoden).
- `(use Type)` holt nur den Namen des Typs herein. Die Varianten eines Enums und die statischen Methoden
  schreibt man über den Typnamen, etwa `Option::some` oder `Chan::new`. Auch nach
  `(use std::sync::Chan)` ist ein bloßes `(new 1)` ein Fehler, solange nirgends eine Funktion namens
  `new` definiert ist.
- Um Konstruktoren und statische Methoden eines Typs unter bloßen Namen zu verwenden, holt man sie
  einzeln herein, etwa `(use std::sync::Chan::new)`, oder mit `(use Type::*)` alle Konstruktoren und
  öffentlichen statischen Methoden (der Konstruktor eines Structs gehört nicht dazu, denn ein Struct
  wird mit `make-instance` erzeugt). Mit `pub use` lassen sie sich nicht re-exportieren.
- Derselbe bloße Name kann nicht aus zwei verschiedenen Typen hereingeholt werden
  (`(use std::sync::Chan::* std::sync::WaitGroup::*)` ist ein Fehler, da beide `new` haben). Ebenso ist
  es ein Fehler, einen Namen hereinzuholen, der einer Funktion, globalen Variable oder Makro des Moduls
  gleicht. Um bewusst zu ersetzen, verwendet man `shadowing-import`.
- `some`/`none` von `Option` und `ok`/`err` von `Result` bietet die Standardbibliothek jedem Modul an,
  daher sind sie ohne `use` unter dem bloßen Namen verwendbar (wie Rusts Prelude
  `Some`/`None`/`Ok`/`Err` hereinholt).
- **Ein Name hat eine Bedeutung.** Funktionen, globale Variablen und Makros teilen sich einen
  Namensraum, und innerhalb eines Moduls können keine zwei von ihnen denselben Namen haben (ein Fehler,
  egal welcher zuerst definiert wird). Funktionen und Variablen haben keine getrennten Namensräume wie in
  CL. Bei demselben Namen in verschiedenen Modulen ist der nähere sichtbar.
- Auflösung eines bloßen Namens `n` (an der Aufrufstelle `(n a ...)`; Namen von Sonderformen stehen
  vorher fest):
  1. Lokale (Parameter sowie die Bindungen von `let`, `labels`, `macrolet` und `symbol-macrolet`)
  2. Wenn der Modulname `n` ein Makro ist, dieses Makro
  3. Konstruktoren und statische Methoden: die per `use` hereingeholten, sowie die Konstruktoren von
     `Sexpr` (`sym`, `int` usw.) und von `Option`/`Result`, die die Standardbibliothek anbietet
  4. Die Methode `n` des Typs des ersten Arguments (Methoden werden **nur über den Typ des ersten
     Arguments** gesucht)
  5. Wenn der Modulname `n` eine Funktion oder eine globale Variable ist, diese (bei einer globalen
     Variable wird die Funktion aufgerufen, die ihr Wert ist)

  Ein Modulname wird in den eigenen Definitionen des aktuellen Moduls, dann in den `use`-Aliasen, dann in
  den Namen, die die Standardbibliothek anbietet, gesucht und steht mit dem ersten Fund fest. Elternmodule
  werden nicht durchsucht. Um eine Definition eines Elternmoduls zu verwenden, schreibt man eine
  Definition, die `pub` trägt, als Pfad von der Wurzel wie `::geo::f` oder holt sie mit `use` herein. Dieselbe Regel gilt, wo eine Funktion als Wert
  übergeben wird; „der Typ des ersten Arguments“ in Schritt 4 ist dann der erste Parametertyp des dort
  erwarteten Funktionstyps (`norm` an einer Stelle, die `(fn (pt) int)` erwartet, ergibt die Methode `norm`
  von `pt`). Steht der erwartete Typ noch nicht fest, wird Schritt 4 übersprungen.
- **Die Namen, die die Standardbibliothek anbietet**, sind in jedem Modul ohne `use` verwendbar: die
  primitiven Typen, `Option`/`Result`/`Vector`, Iteration, S-Ausdrücke und Listen, die
  Sequenzfunktionen, die Steuermakros, Vergleich und die arithmetischen Traits; jeder Name, der auch
  ein Name des `COMMON-LISP`-Pakets von Common Lisp ist; und die Typen, die diese erwähnen. Jeden
  anderen Namen — darunter die Hilfen für Nebenläufigkeit, Netz und Dateien — holt man mit `use`
  herein, etwa `(use std::sync::Chan)`, oder schreibt seinen Pfad, `std::sync::Chan` (die Liste steht
  unter [Namen, die ein `use` brauchen](functions/README.md#namen-die-ein-use-brauchen) bei den eingebauten Funktionen). `use` holt Funktionen,
  globale Variablen, Makros, Typen, Traits, `deftype`-Aliase und Module herein. Ein `use` wirkt nur in
  dem Modul, in dem es steht: Um einen Namen in einem `(module ...)` zu verwenden, schreibt man das
  `use` dort hinein.
- Ein qualifizierter Pfad `a::...::n` wird zu einem Modulnamen (Makro, Funktion oder globale Variable)
  des von `a::...` benannten Moduls aufgelöst oder zu einer Variante oder statischen Methode eines Typs.
  `x::f` an einer Wertstelle (wo `x` einen Wert benennt) ist ein Feldzugriff.
- **`use` wirkt auf die Formen danach.** Eine Datei wird eine Form nach der anderen gelesen, und Abhängigkeiten
  werden direkt vor der Prüfung der Form aufgelöst; schreibt man `m::f` **oberhalb** von `(use m)`, ergibt das
  daher `unresolved path`. `use` setzt man an den Anfang der Datei.
- **`use` kann mehrere Pfade nehmen** (`(use a::f b::g)`). `import` ist eine CL-kompatible Schreibweise mit
  demselben Verhalten.
- **Um einen Namen zu definieren, den die Standardbibliothek anbietet, schreibt man zuerst `(shadow
  name...)`.** Ohne das ist `(defun length ...)` ein Fehler. Ab der Stelle, an der `shadow` gelesen
  wird, ist `name` innerhalb dieses Moduls (der Datei oder des Rumpfs des umschließenden `module`)
  der eigene Name des Moduls, und die Definition der Standardbibliothek ist unter dem bloßen Namen
  nicht mehr sichtbar. Über ihren Pfad, etwa `std::seq::length`, bleibt sie benutzbar. Andere Module
  sind nicht betroffen. Reservierte Wörter (die Namen von Spezialformen) lassen sich nicht mit
  `shadow` überdecken. Es wirkt wie `shadow` in CL.
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
  Definitionen, die in der REPL geschrieben werden, landen im Modul `user` (`(module m ...)` in der
  REPL ist `user::m`).
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
(pub defstruct ...)
(pub defenum ...)
(pub deftrait ...)
(pub deftype ...)
```

`pub` kann nur an die elf obigen Arten und an `use` gesetzt werden (nicht an `module`/`impl`).
`(pub use path...)` **re-exportiert**, was es hereinholt: andere Module erreichen es über einen Pfad
durch dieses Modul (`m::name`) und holen es mit `(use m::name)` herein. Re-exportieren lässt sich nur
eine Definition, kein Modul.
Es wird
geschrieben, indem das Definitionsschlüsselwort direkt auf `pub` folgt, nicht in der Form `(pub (defun ...))`,
die die Definition in Klammern einschließt. Ein `pub` macht genau eine Definition öffentlich (mehrere
Definitionen können nicht auf einmal markiert werden).

Die Methoden eines inhärenten `impl` werden Eintrag für Eintrag als `(pub name ...)` öffentlich gemacht
(3.5). Strukturfelder werden `(pub x i32)` geschrieben (3.6).

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

### 3.16 `#[test]` — Testfunktionen

```lisp
(use std::error::assert-eq)
#[test]
(defun adds () ()
  (assert-eq (+ 1 2) 3))
```

Ein mit `#[test]` markiertes `defun` ist eine Testfunktion, die `typl test` eine nach der anderen aufruft (wie Rusts `#[test]`). Sie darf keine Argumente nehmen und muss `()` oder `Result<(), E>` zurückgeben (Typparameter kann sie auch nicht nehmen). `pub` braucht sie nicht. Ein Test, der ein `Result` zurückgibt, schlägt fehl, wenn er ein `err` zurückgibt, und der Fehler wird angezeigt; daher muss `E` `Error` oder `print-object` implementieren (so wie Rust für den Fehlertyp eines Tests `Debug` verlangt). Ein `Error` wird mit `describe-error` (die Meldung und die Kette ihrer Ursachen) angezeigt, alles andere mit `print-object`. Auch `:dyn Error` ist ein `Error`, sodass ein Test verschiedene Fehler in einem Typ sammeln und zurückgeben kann.

```lisp
(use std::num::ParseIntError std::num::parse-int)
#[test]
(defun parses () Result<(),ParseIntError>
  (match (parse-int "12")
    ((ok _) (Result::ok ()))
    ((err e) (Result::err e))))
```

- **Ein gewöhnlicher Lauf und `compile-file` überspringen sie**, ohne auch nur ihren Inhalt zu prüfen (so wie Rust Tests nur unter `--test` kompiliert). Das Programm selbst kann keine Testfunktion aufrufen.
- Ein Test kann in jeder Datei stehen, die Funktionen enthält, und in jedem `module`. Tests lassen sich auch getrennt im Ordner `tests/` des Projekts ablegen.
- Ein Test schlägt durch ein `panic`, einen Laufzeitfehler oder die Rückgabe eines `err` fehl. Geprüft wird mit `assert` oder mit `assert-eq`, das beide Seiten ausgibt ([System](functions/system.md)). Schlägt ein Test fehl, laufen die übrigen trotzdem.

```sh
typl test                 # alle Tests des Projekts in diesem Ordner ausführen
typl test parse           # nur die, deren Name (der volle Name, etwa `tests::more::parse-int`) parse enthält
typl test foo.typl        # die Tests von foo.typl (und der Dateien, die es per use lädt)
typl test foo.typl parse  # davon die, deren Name parse enthält
```

In einem Projekt (der `typelisp.toml`, die man findet, wenn man die Ordner nach oben geht) lädt `typl test` jede `.typl`-Datei unter dem Quellordner und jede `.typl`-Datei unter dem Ordner `tests/` neben der `typelisp.toml`. Das Modul einer Datei unter `tests/` ergibt sich aus ihrem Pfad vom Ordner der `typelisp.toml` aus (`tests/parse.typl` ist `tests::parse`).

`typl test` **führt die Ausdrücke auf oberster Ebene** der geladenen Dateien **nicht aus**. Das ist die eigene Arbeit des Programms, die ein Testlauf nicht tut (so wie `go test` `main` nicht aufruft). Definitionen (`defun`, der Anfangswert eines `defvar` usw.) und Registrierungen von Reader-Makros geschehen wie gewohnt. Gibt es irgendwo einen Typfehler, läuft kein einziger Test.

Das Ergebnis ist eine Zeile pro Test; die Gründe der Fehlschläge stehen gesammelt am Ende. Schlägt auch nur ein Test fehl, ist der Exit-Code 1.

```text
running 2 tests
test math::adds ... ok
test math::adds-wrong ... FAILED

failures:

---- math::adds-wrong ----
src/math.typl:9:3: panic: assertion failed: (assert-eq (add 1 2) 4)
  left: 3
 right: 4

test result: FAILED. 1 passed; 1 failed; 0 filtered out
```

Was ein Test mit `print` ausgibt, geht direkt auf die Standardausgabe (es wird nicht abgefangen). Für Ausgaben zur Untersuchung schreiben `eprint`/`eprintln` ([Ausgabe](functions/printing.md)) auf die Standardfehlerausgabe und vermischen sich nicht mit der Ausgabe des Programms.

## 4. Bindung und Verzweigung

```lisp
(let ((name val) ...) body...)      ; parallele Bindung
(let* ((name val) ...) body...)     ; sequenzielle Bindung (frühere Bindungen in späteren Initialisierern verwendbar)

(if cond then else)                 ; else ist Pflicht (immer drei Elemente)
(when cond body...)                 ; ein if ohne else (Unit-Typ). defmacro
(unless cond body...)               ; die Negation von when. defmacro
(cond (test1 body...) (test2 body...) ... (:else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; eine Liste von Schlüsseln: passt, wenn einer davon passt
  (:else body...))                   ; expr wird einmal ausgewertet. Schlüssel werden mit equal verglichen.
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
- `(:= expr)` — wertet einen beliebigen Ausdruck aus und vergleicht mit `Eq::equals`. Die einzige Möglichkeit,
  Typen ohne Literalsyntax zu vergleichen (`defstruct`-Instanzen, globale Variablen, berechnete Ergebnisse), und
  eine benutzerdefinierte `Eq`-Implementierung wird unverändert zur Vergleichsregel. `expr` kann sich auf alles
  beziehen, was von der Position des Zweigs aus sichtbar ist (Argumente, äußere Bindungen, globale Variablen)
- `(Ctor sub-pattern...)` — Konstruktormuster (`(some x)` `(none)` `(cons a d)` `(ok v)` usw.)
- `(Type :slot pattern... :slot...)` — Strukturmuster. Schreibe die Slots nach dem Typnamen als
  Schlüsselwörter. Folgt auf ein Schlüsselwort ein Muster, wird dieser Slot gegen das Muster geprüft;
  folgt ein Schlüsselwort oder nichts, wird der Slot an eine Variable seines eigenen Namens gebunden.
  Nicht geschriebene Slots werden nicht geprüft (das Muster hängt also nicht von Anzahl oder Reihenfolge
  der Felder ab):

  ```lisp
  (defstruct point (x i32) (y Option<i32>))

  (defun f ((p point)) i32
    (match p
      ((point :x :y (some b)) (+ x b))   ; bindet x und prüft y gegen (some b)
      ((point :x a) a)))                 ; bindet a
  ```
- `#{p0 p1 ...}` — ein Tupelmuster. Jedes Element wird mit seinem eigenen Teilmuster abgeglichen.
  Gegen ein `Sexpr` passt es nur auf `#{..}`-Daten mit derselben Anzahl an Elementen
- `(:or p1 p2 ...)` — ein Oder-Muster: Es passt, wenn eine der Alternativen passt. Es gibt nur einen
  Rumpf, daher muss jede Alternative dieselben Variablen mit denselben Typen binden. Es kann auch
  innerhalb eines Konstruktormusters stehen (`(some (:or (circle r) (rect r _)))`)

Einen Typ, der `Eq` nicht implementiert, mit einem Wertliteral / `(:= expr)` zu vergleichen, ist ein Typfehler
(diese Sprache sagt lieber „diese lassen sich nicht vergleichen“, als einen Zweig stehen zu lassen, der
stillschweigend nie passt).

**Guards**: Steht `:when Bedingung` hinter dem Muster, wird der Zweig nur gewählt, wenn das Muster
passt und zusätzlich die Bedingung wahr ist. Ist die Bedingung falsch, wird der nächste Zweig
versucht. Die Bedingung kann die Variablen lesen, die das Muster gebunden hat.

```lisp
(defun classify ((n int)) string
  (match n
    (0 "zero")
    (k :when (< k 0) "negative")
    (k :when (evenp k) "even")
    (_ "odd")))
```

- Ein Zweig mit Guard zählt nicht für die Vollständigkeit (wie in Rust). Seine Bedingung kann falsch
  sein, daher brauchen die Varianten, die er abdecken würde, zusätzlich einen Zweig ohne Guard oder
  ein `_`.
- Oder-Muster und Guards lassen sich kombinieren. Der Guard wird ausgewertet, egal welche
  Alternative gepasst hat (`((:or 1 2 3) :when on "small")`).

**Wertliterale gegen einen `Sexpr` als untersuchten Wert**: Das `Eq` von `sexpr` ist `eq` (CLs Identität), daher
lassen sich direkte Werte (`'foo` (interniert) / Ganzzahlen / Zeichen / `true`/`false`) unverändert schreiben und
passen nach Inhalt:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Nicht direkte Literale (Zeichenketten / Gleitkommazahlen / Bignum-Ganzzahlen / Brüche) **lassen sich gegen ein
`Sexpr` nicht schreiben**. Ihr `eq` vergleicht die Identität der Objekte, was einen „Zweig ergäbe, der die
Typprüfung besteht, aber nie passt“, daher ist es ein Fehler, der das Variantenmuster nennt: Schreibt man
`(str "hi")`, wird es in eine `string` zerlegt und nach Inhalt verglichen. `(:= expr)` verlangt ausdrücklich
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

Gegen einen `Sexpr` als untersuchten Wert lassen sich neben den 19 eingebauten Variantenmustern oben
**Downcast-Muster** schreiben (Herausholen von Instanzen benutzerdefinierter ADTs): Syntax, um mit `match` eine
Instanz eines `defstruct`/`defenum` (Kapitel 3) zurückzubekommen, die implizit in `Sexpr` umgewandelt wurde, wie
in `(list p 42)`:

- `(TypeName :slot pattern...)` — Feldzerlegung mit dem **Typnamen** vorn (nur Strukturen), in derselben
  Form wie die Strukturmuster oben geschrieben. Zum Beispiel für `(defstruct point (x f64) (y f64))`:
  `(point :x :y)`.
- Ein bloßer Variantenname `(VariantName sub-pattern...)` — entnimmt eine Variante eines `defenum`. Wird
  als bloßer Name aufgelöst, der durch Hereinholen sichtbar ist, etwa mit `(use EnumType::*)` (dieselben
  Sichtbarkeitsregeln wie beim Aufruf des Konstruktors). Zum Beispiel für `(defenum color (red) (blue))`
  nach `(use color::*)` `(red)` `(blue)`. Mit der qualifizierten Form `(EnumType::VariantName ...)` ist
  kein `use` nötig.
- `(the Type pattern)` — ein Downcast des ganzen Typs (bindet ihn als Ganzes). Es zerlegt keine Felder; es
  übergibt den Wert unverändert an `pattern`. Die einzige Möglichkeit, eine veränderliche Struktur unter Erhalt
  ihrer Identität herauszuholen, und auch die einzige Möglichkeit, einen `Vector<T>`/`HashTable<K,V>` aus einem
  `Sexpr` zu holen (sie haben keine Form zur Zerlegung in Felder). Zum Beispiel schlägt sich nach
  `(the point p)` ein `(setf p::x 9)` auch in der ursprünglichen Instanz in der Liste nieder.

**Muster für `Option<Sexpr>`**: Der Typ von S-Ausdrucksdaten ist nicht `Sexpr`, sondern `Option<Sexpr>`, und die
leere Liste ist keine Variante von `Sexpr`, sondern das `none` von `Option`. Vergleicht man daher ein
`Option<Sexpr>`, lassen sich die 19 Varianten von `Sexpr` und `none` **flach in derselben Liste von Zweigen**
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

Die Vollständigkeit wird im selben flachen Universum geprüft: die 19 Varianten von `Sexpr` plus `none`,
insgesamt 20. `(none)` zu vergessen, ist ein Fehler, sofern es kein `_` gibt. Auch `(some x)` lässt sich
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
    ((circle :r) (* (* r r) 3))    ; Zerlegung in Felder mit dem Typnamen vorn
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Typableitung über Zweige hinweg**: Alle Zweige müssen denselben Typ haben (außer Zweigen, die divergieren, wie
mit `panic`). In einem `match`, das dort steht, wo kein Typ erwartet wird, ergänzen die Zweige gegenseitig ihre
fehlenden Typargumente: `(Result::ok v)` legt nur `T` fest und `(Result::err e)` nur `E`, aber zusammen legen sie
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

**Jede Funktion von `defun` / Methoden in `impl` / `labels` richtet implizit einen Block mit ihrem eigenen Namen ein**
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
es zugleich von einer einfachen Schleife. Auch das Wort, das eine Variable von ihrem Wert trennt, ist keine Ausnahme: Es wird `:=` geschrieben.

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x := 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Variablenklauseln** (vor den Rumpfklauseln geschrieben. Das ist CLs Regel: Danach geschrieben, könnten sie als
„erst ab hier iterieren“ gelesen werden, daher ist es ein Fehler):

| Klausel | Bedeutung |
|---|---|
| `:with v := e` | Bindet einmal. Darf die Variablen früherer Klauseln lesen |
| `:for v :in s` / `:for v :across s` | Die Elemente eines `Iter` der Reihe nach. CLs Unterscheidung zwischen Liste und Vektor gibt es hier nicht, daher sind es zwei Schreibweisen derselben Klausel |
| `:for v :on s` | Die aufeinanderfolgenden **Suffixe**. CL übergibt die geteilte Endzelle, aber ein `Iter` hat kein Ende zu teilen, daher ist jedes ein neuer `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Zählen. Auch `:downfrom`/`:upfrom` funktionieren |
| `:for v := e [:then f]` | Beginnt mit `e` und verwendet ab dem zweiten Mal `f` (ohne `:then` jedes Mal `e`) |
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
(try-as Type expr)                  ; wie as, gibt das Ergebnis aber als Option<Type> zurück (none bei Fehlschlag)
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
des Bereichs fehlschlagen, daher löst `as` einen Panic aus und `try-as` liefert `none`. Alles andere (Erweiterung
und das Abschneiden von `float->int`/`ratio->int`) gelingt immer. `float->int`/`ratio->int`/`char->int` landen
bei `int`, und wird eine schmalere Breite verlangt, wird danach `int->W` aufgerufen. Das ist Zucker, der zu den
entsprechenden Umwandlungsmethoden expandiert (`int->char`/`int->int`/`int->W` usw. unter
[Zahlen](functions/numbers.md)).

Von `rational` wandeln `as`/`try-as` nach `int` um (`as` schneidet ab, `try-as` gelingt nur bei einer
ganzen Zahl), nach `ratio` und `f64`; von `int`/`ratio` nach `rational`.

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

Was sich kompilieren **lässt**: Stream- und Datei-E/A, `random`, `gensym`, `symbol->string`/`intern`,
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

`compile-file` liest auch die Standardbibliothek (und bettet deren Rümpfe in die ausführbare Datei ein), daher
können Funktionen der Standardbibliothek wie `abs`/`gcd` und `(impl print-object ...)` mit AOT verwendet
werden.

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
`unread-char` / `read-delimited-list` nehmen alle `(:where (PeekInput S))`, daher funktionieren sie alle
unverändert auf dem konkreten Typ.

```lisp
(use std::reader::read-sexpr)
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
`readtable-case`, weil Kapitel 1 festlegt, dass der Reader dieser Sprache Namen so liest, wie sie geschrieben sind (CLs `:preserve`).


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
  `std::thread::spawn`). Auch eine benannte Funktion kann übergeben werden.
- **Auf einem eigenen Thread läuft nur kompilierter Code.** Wertet `typl` beim Interpretieren `(thread (f ...))`
  oder `Thread::spawn` aus, kompiliert es die auszuführende Funktion (und was sie aufruft) an Ort und Stelle,
  bevor es sie ausführt. Was sich nicht kompilieren lässt (ein `lambda`, das auf lokale Variablen außerhalb
  verweist, die Konstruktion einer Struktur usw.), ist vor dem Start des Threads ein Panic, der wie ein
  `(panic ...)` behandelt wird. Ein `lambda`, das auf lokale Variablen verweist, lässt sich übergeben, wenn es
  innerhalb einer kompilierten Funktion erzeugt wird.

### 12.3 `select` — auf mehrere Kanaloperationen gleichzeitig warten

```lisp
(select
  ((v (recv ch1)) body...)          ; ein Empfangszweig. v wird an ein Option<T> gebunden
  ((send ch2 x) body...)            ; ein Sendezweig
  (:else body...))                   ; optional. **wenn geschrieben, steht es am Ende**
```

- **Mit `:else` blockiert es nicht** (Gos `default`). Ohne wartet es, bis einer möglich wird.
- **Sind mehrere gleichzeitig möglich, wird einer zufällig gewählt** (in geschriebener Reihenfolge würden spätere
  Zweige verhungern).
- Das `v` eines Empfangszweigs ist ein **`Option<T>`**. Ein geschlossener Kanal ist „eine Antwort“, kein Grund,
  den Zweig zu überspringen, daher wendet man innerhalb des Zweigs `match` darauf an.
- Der Typ ist **die Vereinigung der Typen aller Zweigrümpfe** (dieselbe Regel wie bei `match`-Zweigen).
- `(select)` mit null Zweigen ist ein Typfehler (Gos `select{}`, das ewig blockiert, wird nicht übernommen). Ein
  `select` nur mit `:else` ebenfalls, da es dasselbe ist, wie den Rumpf direkt zu schreiben.

**Die Kanalausdrücke und die zu sendenden Werte werden jeweils einmal von links nach rechts ausgewertet, welcher
Zweig auch gewählt wird** (dieselbe Disziplin, die `case` für seine Schlüssel hat).

```lisp
(use std::sync::after)
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
- **Ausgabe auf Workern**: Interpretierte `print-object`-Methoden sowie interpretierte Funktionen und
  Methoden, die `~/name/` aufruft, können nicht auf anderen Threads laufen; solche Werte auf einem anderen
  Thread auszugeben ist daher ein Panic, der wie ein `(panic ...)` behandelt wird
  (`(compile T::print-object)` oder aus der Haupttask ausgeben).
- **Datenwettläufe sind undefiniert** (dieselbe Position wie Go). Das Ergebnis, wenn mehrere Tasks denselben Wert
  ändern, ohne über `Mutex<T>` / `Chan<T>` zu gehen, ist nicht garantiert.
- **`task` gibt einen Wert zurück.** Anders als Gos `go`-Anweisung gibt es ein `Task<T>` zurück, und `(wait t)`
  holt das Ergebnis.
- **Es gibt keine nil-Kanäle.** Gos Fan-in-Idiom (einen geschlossenen Kanal auf `nil` setzen, um ihn aus den
  Zweigen von `select` zu entfernen) lässt sich nicht schreiben; man startet daher pro Eingabe einen Task und führt
  sie mit einer `WaitGroup` zusammen ([WaitGroup](functions/concurrency.md#4-waitgroup--auf-n-abschlüsse-warten)).
  Das ist auch in Go der empfohlene Weg, aber es ist **der erste Unterschied, auf den Umsteiger von Go stoßen**.
