<!-- translated-from: docs/ja/reference/functions/numbers.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Zahlen

Operationen auf Ganzzahlen, Gleitkommazahlen, rationalen Zahlen, komplexen Zahlen und Wahrheitswerten sowie
weitere zahlenbezogene Funktionen. Wie man die Aufrufformen liest, steht unter
[Eingebaute Funktionen](README.md).

## 1. Ganzzahlen fester Breite

Es gibt sieben Ganzzahltypen: **`int`** (CLs `integer`: beliebige Genauigkeit und der Standardtyp von
Ganzzahlliteralen ohne Annotation; Kapitel 3) sowie die Typen fester Breite `i8` `i16` `i32` `u8` `u16`
`u32`. Für welchen eine Operation aufgelöst wird, bestimmt der Typ des ersten Arguments (sie sind voneinander
unabhängig, ohne implizite Umwandlungen). **Einen 64-Bit-Ganzzahltyp gibt es nicht.** Ein Laufzeitwert ist ein
Wort, dessen niedrige Bits eine Markierung sind, daher bleiben für eine direkte Ganzzahl nur 63 Bit, und ein
Typ, der 64 Bit beansprucht, müsste irgendwo das oberste Bit fallen lassen. `int` wird zur Bignum, sobald es
diese 63 Bit überschreitet; spielt die Breite keine Rolle, verwendet man daher `int`. Die Tabelle unten gilt
für die sechs Typen fester Breite (die Tabelle für `int` steht in Kapitel 3).

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | Die vier Grundrechenarten. `/` schneidet gegen null ab und löst bei Division durch null einen Panic aus |
| `mod` | `(mod a b)` | `(T,T)→T` | Rest (CLs `mod`, **abrundende Division**: Das Vorzeichen folgt dem Divisor. `(mod -7 3)`→`2`). Panic bei Division durch null |
| `rem` | `(rem a b)` | `(T,T)→T` | Rest (CLs `rem`, **abschneidende Division**: Das Vorzeichen folgt dem Dividenden. `(rem -7 3)`→`-1`). Panic bei Division durch null |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Entsprechen CLs zweistelligem `floor`/`ceiling`/`round`/`truncate` (`(floor 7 2)`→Quotient 3, Rest 1). Statt Mehrfachwerten geben sie Quotient und Rest in einer `cons-cell` zurück (`car`=Quotient, `cdr`=Rest). `round-div` rundet Gleichstände wie CL zur geraden Zahl |
| `abs` | `(abs x)` | `T→T` | Absolutbetrag |
| `signum` | `(signum x)` | `T→T` | Vorzeichen (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Größter gemeinsamer Teiler |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Kleinstes gemeinsames Vielfaches (0, wenn eines 0 ist) |
| `max` `min` | `(op a b)` | `(T,T)→T` | Das größere / kleinere (drei oder mehr Argumente werden durch den variadischen Zucker aus Kapitel 8 expandiert) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Vergleich |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Alle dasselbe wie `=` (bei Zahlen desselben Typs gibt es keinen Unterschied) |
| `int->float` | `(int->float x)` | `T→f64` | Erweiternde Umwandlung in `f64` |
| `int->int` | `(int->int x)` | `T→int` | Erweiternde Umwandlung in `int` (immer exakt). Das, was `(as int x)` tut |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Erweiternde Umwandlung in `ratio` (immer exakt) |
| `int->char` | `(int->char x)` | `T→char` | Deutet den Wert als Unicode-Skalarwert. Panic bei einem ungültigen Wert |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Eine Fassung von `int->char`, die bei Fehlschlag `None` zurückgibt |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Breitenumwandlung. Werte, die nicht passen, werden abgeschnitten (wie Rusts `as`) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Dieselbe Umwandlung als Frage. `None`, wenn der Wert nicht in diese Breite passt |

Diese Umwandlungen sind auch das, was die Spezialformen `(as Type x)`/`(try-as Type x)`
([Syntaxreferenz](../syntax.md#7-weitere-spezialformen)) tun. Bitoperationen (`logand`/`ash`/`ldb` usw.) und
Prädikate (`zerop`/`evenp` usw.) haben über die Typen hinweg dieselbe Form, daher sind sie in den Kapiteln 11
und 9 gesammelt.

`i8` `i16` `u8` `u16` `u32` haben genau die Tabelle dieses Kapitels, und `f32` hat genau die `f64`-Tabelle aus
Kapitel 4.

**Ein Typname bedeutet seine Breite und Vorzeichenbehaftung, sonst nichts.** `i32` bedeutet „32 Bit als
vorzeichenbehaftet behandeln“ und `u32` „32 Bit als vorzeichenlos behandeln“. `(+ (the u8 200) (the u8 100))`
ist `44`, `(+ 2147483647 1)` (als `i32`) ist `-2147483648`, und `(lognot (the u32 0))` ist `4294967295`. Bei
`f32` ist es dasselbe: ein echtes binary32. `(/ (the f32 1.0) (the f32 3.0))` wird als `0.33333334`
ausgegeben, ein anderer Wert als das `f64`-Ergebnis `0.3333333333333333`.

Der abgeleitete CL-Katalog (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` und die Prädikate aus Kapitel 9)
existiert für `int`/`i32`/`f64`/`ratio`. Braucht man ihn für eine andere Breite, wechselt man mit
`(as int x)` / `(as i32 x)` hinüber (Breitenumwandlungen gibt es für jedes Paar).

## 2. Rohe Wörter an der C-Grenze (`ptr` / `c-long` / `c-ulong`)

Drei Typen, die nur dazu dienen, Werte an C-Funktionen, die mit
[`defffi`](../syntax.md#33-defffi--c-funktionen-deklarieren-ffi) deklariert wurden, zu übergeben und von ihnen
zu erhalten. `ptr` ist ein undurchsichtiger Zeiger, und `c-long` / `c-ulong` sind Cs `long` /
`unsigned long`. Um einen davon zu einem Wert zu machen, muss man sich innerhalb von `(unsafe ...)` befinden.

**Es gibt keine Arithmetik.** Nichts aus der Tabelle von Kapitel 1 gilt: Weder `(+ p 1)` noch `(< n m)` lässt
sich schreiben. Es sind Wörter, die man an C übergibt, keine Typen zum Rechnen; zum Rechnen wechselt man zu
einem Typ mit Breite. `c-long` / `c-ulong` haben nur Umwandlungen:

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | Dieselben Breitenumwandlungen wie in Kapitel 1. Werte, die nicht passen, werden abgeschnitten |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Dieselbe Umwandlung als Frage |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | Der Weg hinein, vom anderen rohen Wort und von den Ganzzahltypen aus Kapitel 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Wie oben |
| `int->int` | `(int->int x)` | `T→int` | **Immer exakt**. Der ehrliche Weg, ein `size_t` zu lesen, das nicht in ein `i32` passt |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` tun genau das, und die Umwandlungen gibt es
für jedes Paar mit den Ganzzahltypen aus Kapitel 1. `ptr` hat nicht einmal diese Tabelle: Es gibt keine
Möglichkeit, einen Zeiger als Zahl zu lesen. Es ist ein Wert, der nur übergeben, erhalten und an eine andere
C-Funktion weitergereicht wird.

**Sie lassen sich auch nicht ausgeben.** `(println "~a" x)` akzeptiert kein rohes Wort (es hat keine
`Sexpr`-Darstellung), daher wechselt man zuerst zu einem Typ mit Breite, wie in `(println "~a" (as int n))`.

„Einen 64-Bit-Ganzzahltyp gibt es nicht“ vom Anfang von Kapitel 1 gilt auch für diese drei. Es gilt, **weil
sie sich nicht speichern lassen**: Sie können kein `defstruct`-Feld, kein `defvar`, nicht innerhalb eines
Typarguments und nicht innerhalb eines `Sexpr` sein, daher sind es Wörter, die eine Funktion nur als
Argumente, Rückgabewerte und lokale Variablen durchlaufen. Details stehen in der
[Syntaxreferenz](../syntax.md#ptr--c-long--c-ulong--rohe-maschinenwörter).

## 3. Ganzzahlen beliebiger Genauigkeit `int`

CLs `integer` und die **Ganzzahl** dieser Sprache: Ganzzahlliterale ohne Annotation haben diesen Typ, und
eingebaute Funktionen, die eine Zahl zurückgeben, wie `length` und `char->int`, geben diesen Typ zurück. Ein
Wert wird als direkter 63-Bit-Wert (Fixnum) gehalten, solange er passt, wird automatisch zur Bignum befördert,
wenn das Ergebnis einer Operation nicht mehr passt, und wird wieder zum Direktwert, wenn er wieder passt. `eq`
ist im Fixnum-Bereich immer Wertgleichheit, und `eql`/`=` sind numerische Gleichheit über den ganzen Bereich.
Es ist ein anderer Typ als die Ganzzahltypen fester Breite (Kapitel 1), ohne implizite Umwandlung:
`(as int x)` ist die exakte Erweiterung von einer festen Breite, und `(as i32 n)` / `(try-as i32 n)` sind das
Abschneiden / die Prüfung von `int` aus (dieselbe Bedeutung wie `int->W` / `try-int->W` in Kapitel 1).

Die Ganzzahlvariante von `Sexpr` ist ebenfalls einfach `int` (`(int n)` akzeptiert sowohl Fixnums als auch
Bignums).

Eingebaute Funktionen, die einen Index oder eine Anzahl nehmen (`substring`, `get` von `Vector`, die
Verschiebeweite von `ash` usw.), akzeptieren `int`, aber einen Wert zu übergeben, der nicht in eine Fixnum
passt, ist ein Laufzeitfehler („an integer argument does not fit a fixnum“).

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Laufen nie über (sie befördern) |
| `/` | `(/ a b)` | `(int,int)→int` | Schneidet gegen null ab. Panic bei Division durch null |
| `mod` | `(mod a b)` | `(int,int)→int` | Rest der abrundenden Division (das Vorzeichen folgt dem Divisor) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Alle `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Dasselbe wie in Kapitel 11 (Zweierkomplement mit unendlich vielen Bits) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Dasselbe wie in Kapitel 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Abschneiden / Prüfung. `W` ist eine der sechs Breiten oder `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Identität (auf der Seite der festen Breiten und C-Wörter erweitert `int->int`; Kapitel 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | Dieselbe Form wie in Kapitel 1. `expt` akzeptiert nur nicht negative Exponenten |

## 4. Gleitkommazahlen (`f64` / `f32`)

`f32` hat dieselbe Tabelle.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE 754. Division durch null löst keinen Panic aus; sie ergibt `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Rest der abrundenden Division (wie in CL; das Vorzeichen folgt dem Divisor. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Rest der abschneidenden Division (wie in CL; das Vorzeichen folgt dem Dividenden. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Vergleich |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Alle dasselbe wie `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Potenz |
| `abs` | `(abs x)` | `f64→f64` | Absolutbetrag |
| `signum` | `(signum x)` | `f64→f64` | Vorzeichen (`1.0`/`-1.0`; `±0.0`/`NaN` werden unverändert zurückgegeben. Wie in CL, anders als Rusts `signum`) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | Das größere / kleinere (drei oder mehr Argumente werden durch den variadischen Zucker aus Kapitel 8 expandiert) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Einstellige Operationen |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Transzendente Funktionen. `log` ist der natürliche Logarithmus |
| `log` (zwei Argumente) | `(log x base)` | `(f64,f64)→f64` | Logarithmus zu einer gegebenen Basis. Expandiert zu `(/ (log x) (log base))` (Kapitel 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Entsprechen CLs zweistelligen Fassungen (`(floor 7.0 2.0)`→Quotient 3, Rest 1). Derselbe Entwurf wie die gleichnamigen Funktionen in Kapitel 1 (`car`=Quotient, `cdr`=Rest) |
| `float->int` | `(float->int x)` | `f64→int` | Wandelt durch Abschneiden gegen null in `int` um (CLs `truncate`; exakt für endliche Werte jeder Größe). Panic bei Unendlich und NaN. Für eine feste Breite verwendet man `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Wandelt als exakte binäre rationale Zahl in ein `ratio` um (CLs `rational`) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Wandelt zwischen Gleitkommabreiten um. `float->f32` rundet zum nächsten Wert, `float->f64` ist immer exakt. Das, was `(as f32 x)` tut |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | Dieselbe Umwandlung als Frage. `none`, wenn das Runden den Wert ändert (das Erweitern auf `f64` ist immer `some`). Das, was `(try-as f32 x)` tut |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | CLs gleichnamige Funktionen. Aliasse von `floor`/`ceiling`/`round`/`truncate` oben: In CL geben die Varianten ohne Präfix Ganzzahlen zurück, daher entsprechen die mit `f`-Präfix dem Verhalten dieser Sprache |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | Jeweils 2 / 53 / 53 (nur die Genauigkeit von `0.0` ist 0). `f64` ist immer IEEE-754 binary64, daher sind es Konstanten |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` oder `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | Die Mantisse (in `[1/2,1)`, ohne Vorzeichen) und der Exponent. CL gibt drei Werte zurück, aber es gibt keine Mehrfachwerte, daher bleibt das Vorzeichen `float-sign` überlassen |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | Dieselbe Zerlegung mit einer exakten ganzzahligen 53-Bit-Mantisse. `mantissa * 2^exponent` ist genau der ursprüngliche Wert |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **Die einfachste rationale Zahl, die als diese Gleitkommazahl zurückgelesen wird** (`(rationalize 0.1)` ist `1/10`). Für den exakten Binärwert verwendet man `float->ratio` |

**Unterschied zu CL: wie `round` rundet.** `round` (und damit `fround`/`round-div`) rundet **von null weg**
(`(round 2.5)` = `3.0`). CL rundet **zur geraden Zahl** und ergibt `2`.

## 5. Rationale Zahlen `ratio`

CL-kompatible rationale Zahlen beliebiger Genauigkeit. Sie werden immer gekürzt mit positivem Nenner gehalten
und auf dem Heap angelegt. Mit den Ganzzahltypen oder `f64` gibt es keine implizite Umwandlung (man verwendet
eine ausdrückliche Umwandlungsmethode oder `as`/`try-as`). Zur Syntax der Bruchliterale siehe die
[Syntaxreferenz](../syntax.md#1-lexikalische-elemente).

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | Die vier Grundrechenarten (Ergebnisse immer gekürzt). `/` löst bei Division durch null einen Panic aus |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Rest der abrundenden Division (wie in CL; das Vorzeichen folgt dem Divisor) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Rest der abschneidenden Division (wie in CL; das Vorzeichen folgt dem Dividenden) |
| `abs` | `(abs x)` | `ratio→ratio` | Absolutbetrag |
| `signum` | `(signum x)` | `ratio→ratio` | Vorzeichen (gibt `1`/`-1`/`0` als `ratio` zurück) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Potenz. Der Exponent muss ein ganzzahliges `ratio` sein (sonst Panic). Ein negativer Exponent ergibt den Kehrwert |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | Das größere / kleinere |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` hat keine Bitoperationen (in CL gibt es sie nur für Ganzzahlen) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Vergleich |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Alle dasselbe wie `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Zähler in gekürzter Form (derselbe Name wie in CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Nenner in gekürzter Form (immer positiv) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Ganzzahliger Teil (gegen null abgeschnitten) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Wandelt in `f64` um |

Die Wege hinein von Ganzzahlen fester Breite und `f64` sind `int->int`/`int->ratio` (Kapitel 1) und
`float->int`/`float->ratio` (Kapitel 4). `int`/`ratio` sind eigene Typen, unabhängig von `i32` und den
anderen, und gemischte Arithmetik braucht ausdrückliche Umwandlungen.

## 6. Komplexe Zahlen `complex`

Eine Struktur (`defstruct`) der Standardbibliothek.

**Zwei Unterschiede zu CL** (beide folgen aus der statischen Typisierung):

1. **Die Komponenten sind immer `f64`.** Eine komplexe Zahl in CL kann auch rationale Zahlen halten, und
   `(complex 1 2)` und `(complex 1.0 2.0)` sind verschiedene Typen. Ein statischer Typ muss sich für eines
   entscheiden, und die transzendenten Funktionen geben die Gleitkommaart zurück.
2. **`(sqrt -1.0)` ist das reelle `sqrt` (NaN).** In CL kann `sqrt` aus einer reellen Zahl eine komplexe
   zurückgeben, aber das `sqrt` von `f64` muss ein `f64` zurückgeben. Ein komplexes Ergebnis entsteht aus
   einem komplexen Argument: `(sqrt (complex -1.0 0.0))` ist `i`.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Erzeugung. Die Komponenten lassen sich direkt als `z::re`/`z::im` lesen |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Realteil und Imaginärteil. **Sie funktionieren auch auf reellen Zahlen** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), wie in CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Konjugierte (funktioniert auch auf reellen Zahlen) |
| `phase` | `(phase z)` | `complex→f64` | Argument in (-pi,pi] (funktioniert auch auf reellen Zahlen) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Absolutbetrag. **Das einzige `abs`, das nicht den Typ des Empfängers zurückgibt** (wie in CL ist der Betrag einer komplexen Zahl reell) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Komplexe Arithmetik |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Komponentenweise Gleichheit. Auch `Eq` ist implementiert (kein `Ord`: Komplexe Zahlen haben keine Ordnung, und auch CLs `<` lehnt sie ab) |
| `zerop` | `(zerop z)` | `complex→bool` | Ob beide Komponenten 0 sind |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` liefern Hauptwerte |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | Der Winkel des Vektors `(x,y)`. **CLs zweistelliges `(atan y x)` ist Zucker hierfür** (es verzweigt nach der Anzahl der Argumente, wie das zweistellige `log`) |

Es implementiert `print-object`, daher geben `~a`/`~s` es wie CL als `#C(re im)` aus (der Reader dieser Sprache
hat keine Syntax `#C`, um es zurückzulesen).

## 7. Wahrheitswerte

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Negation |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Alle vergleichen Werte auf Gleichheit |

`and`/`or` brauchen Kurzschlussauswertung und sind daher Spezialformen
([Syntaxreferenz](../syntax.md#4-bindung-und-verzweigung)).

## 8. Numerische Hilfsfunktionen und Aufrufzucker

`abs`/`signum` (alle numerischen Typen), `gcd`/`lcm` (nur Ganzzahltypen), `rem` (alle reellen Typen
einschließlich `f64`) und `expt` (`int`/`f64`/`ratio`) sind als Methoden jedes numerischen Typs definiert
(aufgelöst über den Typ des Empfängers: `(abs x)` ist die Methode für den Typ von `x`). Die Details für jeden
Typ stehen in den Kapiteln 1, 3, 4 und 5. Ganzzahlen fester Breite haben kein `expt` (sie haben keine
Beförderung und würden überlaufen; man wechselt mit `(as int x)` zu `int` und verwendet dessen `expt`).

### 8.1 Variadische Formen und Formen mit 0/1 Argumenten

Arithmetik und Vergleiche in CL sind variadisch, aber Methoden werden nur über den Typ des Empfängers
aufgelöst, nicht über die Anzahl der Argumente. Daher **expandiert die Prüfung die folgenden Formen in
zweistellige Aufrufe**.

| Was man schreiben kann | Expansion | Gilt für |
|---|---|---|
| `(op a b c ...)` | Die Linksfaltung `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)`, wobei jeder Term an eine Hilfsvariable gebunden wird | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Diejenigen der obigen, die ein neutrales Element haben |
| `(op x)` | Für `+ * max min logand logior logxor` `x` selbst. `(- x)` negiert, `(/ x)` ergibt den Kehrwert, `(gcd x)`/`(lcm x)` ergeben `(abs x)` (wie in CL) | Wie oben |
| `(cmp x)` | Wertet `x` aus und ergibt `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Jeder Term wird genau einmal von links nach rechts ausgewertet (deshalb laufen die variadischen Vergleiche über
Hilfsvariablen). Die variadische Form von `/=` vergleicht **benachbarte Paare**, anders als CL, das fragt, ob
sich alle Paare unterscheiden.

### 8.2 `isqrt` und ganzzahliges `expt`

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | Die größte Ganzzahl, die die Quadratwurzel nicht übersteigt. Panic bei einem negativen Wert |
| `expt` | `(expt n e)` | `(T,T)→T` | Potenz (durch Quadrieren). CL gibt bei negativem Exponenten eine rationale Zahl zurück, aber ein Ganzzahltyp kann sie nicht darstellen, daher Panic; zuerst in `ratio` umwandeln |

## 9. Prädikate

| Name | Form | Typ | Typen |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (nur Ganzzahltypen, wie in CL) |

Es gibt **keine Typprädikate** wie CLs `numberp`/`integerp`/`floatp`. Bei statischer Typisierung steht der Typ
eines Wertes bereits fest, ohne zur Laufzeit zu fragen.

## 10. Konstanten

| Name | Typ | Wert |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | An `boole` übergebene Operationscodes (anstelle von CLs Schlüsselwörtern) |

Konstanten für numerische Grenzen (CLHS 12.1.4.2 / 12.1.3):

| Name | Typ | Beschreibung |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | Die obere / untere Grenze eines direkten 63-Bit-Wertes (2^62-1 / -2^62). Ein `int` darüber hinaus wird zur Bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | Der größte / kleinste endliche Wert |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | Der kleinste von null verschiedene Betrag, einschließlich subnormaler Zahlen |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | Dasselbe, beschränkt auf normalisierte Zahlen |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | Sie folgen der Definition von CL (das kleinste positive `e` mit `(/= (+ 1 e) 1)`), daher sind sie **um eine ULP größer als** 2^-53: 2^-53 selbst rundet beim Runden zum nächsten geraden Wert auf `1.0` zurück |

## 11. Bitoperationen

Definiert auf dem Zweierkomplement mit unendlich vielen Bits (CL 12.10). Sie sind für die Ganzzahltypen fester
Breite und `int` implementiert, nicht für `ratio` (auch CL hat Bitoperationen nur für Ganzzahlen).

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | Bitweises Und, Oder, exklusives Oder (variadische und nullstellige Fassungen in 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Bitweises Komplement |
| `ash` | `(ash x count)` | `(T,int)→T` | Arithmetische Verschiebung. Nach links, wenn `count` positiv ist, nach rechts, wenn negativ |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Ob Bit `index` gesetzt ist (**die Argumentreihenfolge ist umgekehrt zu CL**; siehe unten) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | Die Anzahl der gesetzten Bits (bei einer negativen Zahl die Anzahl der 0-Bits) |
| `integer-length` | `(integer-length x)` | `T→T` | Die Anzahl der Bits, die zur Darstellung nötig sind, ohne das Vorzeichen |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | Die übrigen sieben, aus den obigen zusammengesetzt |

**Nur das zweite Argument von `ash` ist `int` statt `T`.** Es ist eine **Distanz** in Bits, kein Wert vom Typ
des Empfängers, daher sagen Breite und Vorzeichenbehaftung des Empfängers nichts über die Distanz (aus
demselben Grund, aus dem `count` in CLs `(ash integer count)` eine beliebige Ganzzahl ist). Einen
vorzeichenlosen Wert nach rechts zu verschieben, ist eine logische Verschiebung (`(ash (the u8 200) -3)` =
`25`), bei einem vorzeichenbehafteten eine arithmetische Verschiebung, die gegen minus unendlich rundet
(`(ash (the i32 -100) -4)` = `-7`). Der `index` von `logbitp` ist aus demselben Grund `int`.

**Bytespezifikatoren.** Statt des undurchsichtigen Objekts, das CLs `byte` zurückgibt, wird eine
`cons-cell<int,int>` (`car`=Größe, `cdr`=Position) verwendet. Größe und Position sind beide Anzahlen von Bits,
daher sind sie `int`, unabhängig von der Breite der Ganzzahl, die zerlegt wird.

**Die Ganzzahl ist das erste Argument, in anderer Reihenfolge als in CL.** CL schreibt
`(ldb bytespec integer)`, aber diese Sprache wählt eine Methode nach dem Typ des Empfängers (des ersten
Arguments), und mit dem Spezifikator vorn ließe sich nicht nach dem Typ der Ganzzahl wählen. Alle anderen
Bitoperationen haben die Form `(op integer ...)` (`(logand a b)`, `(ash x count)`, `(lognot x)`), und nur die
`ldb`-Familie und `logbitp` waren umgekehrt, daher wurden diese angeglichen. Die übrigen Argumente behalten
CLs relative Reihenfolge, daher wird `(dpb newbyte spec n)` zu `(dpb n newbyte spec)`.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Erzeugt einen Bytespezifikator |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Holt eine Komponente heraus |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Extrahiert das angegebene Byte aus `x`, rechtsbündig |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Ob irgendein Bit im angegebenen Byte gesetzt ist |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Löscht alles außerhalb des angegebenen Bytes (Positionen bleiben) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Setzt das rechtsbündige `newbyte` in das angegebene Byte von `x` ein |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Die positionserhaltende Fassung von `dpb` |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | Eine der 16 zweistelligen logischen Operationen, gewählt durch `op` (eine `boole-*`-Konstante aus Kapitel 10) |

`T` ist ein Typ, der den Trait `Bits` implementiert, also `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Nur `boole`
behält `op` vorn, da es keinen Grund gibt, dort CLs Reihenfolge zu ändern.

## 12. Zufallszahlen

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Eine Zufallszahl von `0` bis ausschließlich `n`. Wird der Zustand weggelassen, zieht sie aus `*random-state*` und schreitet ihn fort |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Ohne Argument ein neuer Zustand; mit einem eine Kopie davon (die Kopie wiederholt dieselbe Folge) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Immer `true` (der statische Typ schließt andere Typen bereits aus; es existiert nur als Entsprechung zu CL) |
| `*random-state*` | — | `random-state` | Der Standardzustand von `random`. Eine zuweisbare globale Variable (mit `setf` ersetzen) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | Der Zustand, den die Ganzzahl benennt. Derselbe Startwert wiederholt immer dieselbe Folge |

Der Generator ist xorshift64 und liefert dieselbe Folge, ob interpretiert oder kompiliert.

Ein neuer Zustand aus `make-random-state` wird aus der Uhrzeit initialisiert und lässt sich daher über
Programmläufe hinweg nicht reproduzieren. Zum Reproduzieren verwendet man `seed-random-state`:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; gibt bei jedem Lauf dieselben drei Zahlen aus
```

**CL hat keine portable Möglichkeit, einen Startwert anzugeben** (`make-random-state` nimmt nur
`nil`/`t`/einen Zustand), daher folgt dieser Name SBCLs `sb-ext:seed-random-state` statt CL.

Verschiedene Startwerte ergeben verschiedene Folgen. `(seed-random-state 0)` und `(seed-random-state 1)`
ergeben verschiedene Folgen, ebenso `-7` und `7`.
