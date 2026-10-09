<!-- translated-from: docs/ja/reference/types.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Typen

Die Typen, die typelisp hat, und die Standard-Traits, die jeder Typ implementiert. Wie man Typen schreibt,
steht in [Kapitel 2 der Syntaxreferenz](syntax.md#2-typen-schreiben); die Funktionen und Methoden jedes Typs
stehen unter [Eingebaute Funktionen](functions/README.md).

## 1. Primitive Typen

| Typ | Inhalt | Details |
|---|---|---|
| `int` | Eine Ganzzahl beliebiger Genauigkeit. Wird als Direktwert gehalten, solange sie in 63 Bit passt, und wird darüber hinaus automatisch zu einer Bignum. Der Standardtyp von Ganzzahlliteralen ohne Annotation | [Zahlen, Kapitel 3](functions/numbers.md#3-ganzzahlen-beliebiger-genauigkeit-int) |
| `i8` `i16` `i32` | Vorzeichenbehaftete Ganzzahlen fester Breite | [Zahlen, Kapitel 1](functions/numbers.md#1-ganzzahlen-fester-breite) |
| `u8` `u16` `u32` | Vorzeichenlose Ganzzahlen fester Breite | Wie oben |
| `f32` `f64` | Gleitkommazahlen nach IEEE 754. Dezimalliterale sind standardmäßig `f64` | [Zahlen, Kapitel 4](functions/numbers.md#4-gleitkommazahlen-f64--f32) |
| `ratio` | Eine rationale Zahl in gekürzter Form | [Zahlen, Kapitel 5](functions/numbers.md#5-rationale-zahlen-ratio) |
| `bool` | `true` / `false` | [Zahlen, Kapitel 7](functions/numbers.md#7-wahrheitswerte) |
| `char` | Ein Unicode-Skalarwert | [Zeichen](functions/collections.md#2-zeichen-char) |
| `string` | Eine unveränderliche Zeichenkette | [Zeichenketten](functions/collections.md#1-zeichenketten-string) |
| `symbol` | Ein Symbol. Auch Schlüsselwörter (`:name`) haben diesen Typ | [Symbole](functions/sequences.md#3-symbole) |
| `()` | Der Unit-Typ. Sein Wert ist ebenfalls `()` | |
| `!` | Der Never-Typ. Der Typ von Ausdrücken, die nicht zurückkehren, wie `panic`. Kann überall stehen, wo ein beliebiger Typ erwartet wird | |
| `ptr` `c-long` `c-ulong` | Wörter, die nur dazu dienen, Werte an C zu übergeben und von C zu erhalten. Sie können nur innerhalb von `unsafe` Werte sein, und die Stellen, an denen sie vorkommen dürfen, sind begrenzt | [Zahlen, Kapitel 2](functions/numbers.md#2-rohe-wörter-an-der-c-grenze-ptr--c-long--c-ulong) |
| `random-state` | Der Zustand eines Zufallszahlengenerators | [Zahlen, Kapitel 12](functions/numbers.md#12-zufallszahlen) |

Einen 64-Bit-Ganzzahltyp gibt es nicht. Für Ganzzahlen, deren Breite keine Rolle spielt, verwendet man `int`.

## 2. Eingebaute generische Typen

| Typ | Inhalt | Details |
|---|---|---|
| `Option<T>` | Ein Wert, der da ist oder nicht. `some` / `none` | [Option und Result](functions/option-result.md) |
| `Result<T,E>` | Erfolg oder Fehlschlag. `ok` / `err` | Wie oben |
| `Vector<T>` | Ein wachsendes Array | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Eine Hashtabelle. Der Schlüsseltyp muss `Hash` implementieren | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Ein Handle auf einen Task | [Tasks](functions/concurrency.md#1-taskt--handles-auf-tasks) |
| `Thread<T>` | Ein Handle auf einen Task, der auf einem eigenen OS-Thread läuft | [Thread](functions/concurrency.md#7-threadt--eigene-os-threads) |
| `Chan<T>` | Ein Kanal | [Kanäle](functions/concurrency.md#2-chant--kanäle) |

Funktionstypen schreibt man `(fn (Argumenttypen...) Rückgabetyp)`, Trait-Objekte `:dyn Trait`
([Kapitel 2 der Syntaxreferenz](syntax.md#2-typen-schreiben)).

## 3. S-Ausdrucksdaten

| Typ | Inhalt | Details |
|---|---|---|
| `Sexpr` | Ein nicht leerer S-Ausdruck. 18 Varianten: `int`, `i8` bis `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array` | [S-Ausdrucksdaten](functions/sequences.md#2-s-ausdrucksdaten-sexpr) |
| `Option<Sexpr>` | S-Ausdrucksdaten im Allgemeinen. Die leere Liste `()` ist `none` | Wie oben |

## 4. Typen der Standardbibliothek

Typen, die die Standardbibliothek (das Prelude) mit `defstruct` / `defenum` definiert. Sie werden genauso
behandelt wie selbst geschriebene Typen, und alles, was mit einem `defstruct` geht, geht auch mit ihnen.

| Typ | Inhalt | Details |
|---|---|---|
| `cons-cell<A,B>` | Ein Paar. `cons`/`car`/`cdr` | [Paare](functions/sequences.md#1-paare-cons-cellab) |
| `complex` | Eine komplexe Zahl (Komponenten `f64`) | [Zahlen, Kapitel 6](functions/numbers.md#6-komplexe-zahlen-complex) |
| `Array<T>` | Ein mehrdimensionales Array | [Array](functions/collections.md#5-arrayt-mehrdimensionale-arrays) |
| `BitVector` | Eine Bitfolge fester Länge | [BitVector](functions/collections.md#6-bitvector-bitvektoren) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Die Iteratoren, die `iter` der jeweiligen Sammlung zurückgibt | [Iter](functions/traits.md#1-der-trait-iter-und-die-iteration) |
| `WaitGroup` | Auf den Abschluss von N Dingen warten | [WaitGroup](functions/concurrency.md#4-waitgroup--auf-n-abschlüsse-warten) |
| `Mutex<T>` | Gegenseitiger Ausschluss für geteilte Daten | [Mutex](functions/concurrency.md#6-mutext--gegenseitiger-ausschluss-für-geteilte-daten) |
| `pathname` | Ein in Teile zerlegter Dateiname | [Pfadnamen](functions/streams-files.md#9-pfadnamen-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Streams | [Streams](functions/streams-files.md#3-konkrete-stream-typen) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Zusammengesetzte Streams | [Zusammengesetzte Streams](functions/streams-files.md#4-zusammengesetzte-streams) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Netzwerk | [Netzwerk](functions/network.md#1-typen) |
| `ReadOutcome` | Das Ergebnis von `read-sexpr`. `datum` / `eof` | [Streams](functions/streams-files.md#6-generische-funktionen-und-dateioperationen) |
| `universal-time` `internal-time` `decoded-time` | Zeit | [Zeit](functions/system.md#1-zeit) |
| `heap-info` | Der aktuelle Zustand des Heaps | [Werkzeuge der Implementierung](functions/system.md#51-felder-von-heap-info) |

## 5. Fehlertypen

`Error` ist kein Typ, sondern ein Trait, und die folgenden Typen implementieren ihn. Um Fehler beliebiger Art
zu behandeln, schreibt man `:dyn Error`.

| Typ | Erzeugt von |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Datei- und Stream-Operationen |
| `NetError` | Netzwerkoperationen |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Details stehen unter [Fehlertypen und der Trait Error](functions/option-result.md#3-fehlertypen-und-der-trait-error).

## 6. Implementierungen der Standard-Traits

Welche Typen welche Traits implementieren. Die Methoden jedes Traits stehen unter
[Standard-Traits](functions/traits.md) und in den Kapiteln der rechten Spalte.

### 6.1 Vergleich, Hashing und Ausgabe

| Trait | Implementierende Typen | Details |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-vergleich) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Wie oben |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` und alle eingebauten Fehlertypen | [print-object](functions/printing.md#5-print-object-typabhängige-druckdarstellung) |

`Eq`/`Ord` von `cons-cell<A,B>` sind verwendbar, wenn die Elementtypen `Eq`/`Ord` implementieren.

### 6.2 Arithmetik

| Trait | Implementierende Typen |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Details stehen unter [Arithmetische Traits](functions/traits.md#3-arithmetische-traits-add--sub--mul--div--rem--bits--number).

### 6.3 Iteration

| Trait | Implementierende Typen |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Streams

| Typ | Implementierte Traits |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

Jeder Stream implementiert `Stream`; Eingabe-Streams implementieren zusätzlich `InputStream`, Ausgabe-Streams
`OutputStream`. `socket-listener` und `udp-socket` implementieren nur `Stream` (`close` /
`open-stream-p`). Details stehen unter [Streams](functions/streams-files.md#1-die-trait-hierarchie).

### 6.5 Sonstiges

| Trait | Implementierende Typen | Details |
|---|---|---|
| `Error` | Alle Fehlertypen aus Kapitel 5 | [Fehlertypen](functions/option-result.md#3-fehlertypen-und-der-trait-error) |
| `Pathish` | `string` `pathname` | [Pfadnamen](functions/streams-files.md#91-der-pfadbezeichner-trait-pathish) |
