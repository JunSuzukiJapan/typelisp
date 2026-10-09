<!-- translated-from: docs/ja/reference/types.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Typer

De typer typelisp har, och de standardtraits som varje typ implementerar. Hur typer skrivs finns i
[Syntaxreferens kapitel 2](syntax.md#2-hur-typer-skrivs); funktionerna och metoderna för varje typ finns i
[Inbyggda funktioner](functions/README.md).

## 1. Primitiva typer

| Typ | Innehåll | Detaljer |
|---|---|---|
| `int` | Ett heltal med godtycklig precision. Hålls som ett omedelbart värde så länge det ryms i 63 bitar och blir automatiskt ett bignum därutöver. Standardtypen för heltalsliteraler utan annotering | [Tal kapitel 3](functions/numbers.md#3-heltal-med-godtycklig-precision-int) |
| `i8` `i16` `i32` | Heltal med tecken och fast bredd | [Tal kapitel 1](functions/numbers.md#1-heltal-med-fast-bredd) |
| `u8` `u16` `u32` | Heltal utan tecken och med fast bredd | Som ovan |
| `f32` `f64` | IEEE-754-flyttal. Decimalliteraler får som standard `f64` | [Tal kapitel 4](functions/numbers.md#4-flyttal-f64--f32) |
| `ratio` | Ett rationellt tal i enklaste form | [Tal kapitel 5](functions/numbers.md#5-kvoter-ratio) |
| `bool` | `true` / `false` | [Tal kapitel 7](functions/numbers.md#7-booleska-värden) |
| `char` | Ett skalärt Unicode-värde | [Tecken](functions/collections.md#2-tecken-char) |
| `string` | En oföränderlig sträng | [Strängar](functions/collections.md#1-strängar-string) |
| `symbol` | En symbol. Keywords (`:name`) har också den här typen | [Symboler](functions/sequences.md#3-symboler) |
| `()` | Typen Unit. Dess värde är också `()` | |
| `!` | Typen Never. Typen på uttryck som inte returnerar, som `panic`. Kan placeras där vilken typ som helst förväntas | |
| `ptr` `c-long` `c-ulong` | Ord som bara används för att skicka värden till och från C. De kan vara värden bara inuti `unsafe`, och de platser där de kan förekomma är begränsade | [Tal kapitel 2](functions/numbers.md#2-råa-ord-vid-c-gränsen-ptr--c-long--c-ulong) |
| `random-state` | Tillståndet hos en slumptalsgenerator | [Tal kapitel 12](functions/numbers.md#12-slumptal) |

Det finns ingen 64-bitars heltalstyp. För heltal där bredden inte spelar någon roll används `int`.

## 2. Inbyggda generiska typer

| Typ | Innehåll | Detaljer |
|---|---|---|
| `Option<T>` | Ett värde som finns eller inte. `some` / `none` | [Option och Result](functions/option-result.md) |
| `Result<T,E>` | Lyckat eller misslyckat. `ok` / `err` | Som ovan |
| `Vector<T>` | En växande array | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | En hashtabell. Nyckeltypen måste implementera `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Ett handtag till en task | [Tasks](functions/concurrency.md#1-taskt--handtag-till-tasks) |
| `Thread<T>` | Ett handtag till en task som körs på en dedikerad OS-tråd | [Thread](functions/concurrency.md#7-threadt--dedikerade-os-trådar) |
| `Chan<T>` | En kanal | [Kanaler](functions/concurrency.md#2-chant--kanaler) |

Funktionstyper skrivs `(fn (argumenttyper...) returtyp)`, och trait-objekt `:dyn Trait`
([Syntaxreferens kapitel 2](syntax.md#2-hur-typer-skrivs)).

## 3. S-uttrycksdata

| Typ | Innehåll | Detaljer |
|---|---|---|
| `Sexpr` | Ett icke-tomt S-uttryck. 18 varianter: `int`, `i8` till `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array` | [S-uttrycksdata](functions/sequences.md#2-s-uttrycksdata-sexpr) |
| `Option<Sexpr>` | S-uttrycksdata i allmänhet. Den tomma listan `()` är `none` | Som ovan |

## 4. Typer i standardbiblioteket

Typer som standardbiblioteket (prelude) definierar med `defstruct` / `defenum`. De behandlas på samma
sätt som typer du skriver själv, och allt du kan göra med en `defstruct` kan göras med dem.

| Typ | Innehåll | Detaljer |
|---|---|---|
| `cons-cell<A,B>` | Ett par. `cons`/`car`/`cdr` | [Par](functions/sequences.md#1-par-cons-cellab) |
| `complex` | Ett komplext tal (komponenter av typen `f64`) | [Tal kapitel 6](functions/numbers.md#6-komplexa-tal-complex) |
| `Array<T>` | En flerdimensionell array | [Array](functions/collections.md#5-arrayt-flerdimensionella-arrayer) |
| `BitVector` | En följd av bitar med fast längd | [BitVector](functions/collections.md#6-bitvector-bitvektorer) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | De iteratorer som `iter` för varje samling returnerar | [Iter](functions/traits.md#1-traitet-iter-och-iteration) |
| `WaitGroup` | Att vänta på att N saker blir klara | [WaitGroup](functions/concurrency.md#4-waitgroup--vänta-på-n-färdigställanden) |
| `Mutex<T>` | Ömsesidig uteslutning för delad data | [Mutex](functions/concurrency.md#6-mutext--ömsesidig-uteslutning-för-delad-data) |
| `pathname` | Ett filnamn uppdelat i delar | [Pathnames](functions/streams-files.md#9-pathnames-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Strömmar | [Strömmar](functions/streams-files.md#3-konkreta-strömtyper) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Sammansatta strömmar | [Sammansatta strömmar](functions/streams-files.md#4-sammansatta-strömmar) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Nätverk | [Nätverk](functions/network.md#1-typer) |
| `ReadOutcome` | Resultatet av `read-sexpr`. `datum` / `eof` | [Strömmar](functions/streams-files.md#6-generiska-funktioner-och-filoperationer) |
| `universal-time` `internal-time` `decoded-time` | Tid | [Tid](functions/system.md#1-tid) |
| `heap-info` | Heapens aktuella tillstånd | [Implementationsverktyg](functions/system.md#51-fält-i-heap-info) |

## 5. Feltyper

`Error` är inte en typ utan ett trait, och följande typer implementerar det. För att hantera fel av
vilket slag som helst skriver man `:dyn Error`.

| Typ | Skapas av |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Fil- och strömoperationer |
| `NetError` | Nätverksoperationer |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Detaljer finns i [Feltyper och traitet Error](functions/option-result.md#3-feltyper-och-traitet-error).

## 6. Implementationer av standardtraits

Vilka typer som implementerar vilka traits. Metoderna för varje trait finns i
[Standardtraits](functions/traits.md) och i de kapitel som anges i kolumnen längst till höger.

### 6.1 Jämförelse, hashning och utskrift

| Trait | Implementerande typer | Detaljer |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-jämförelse) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Som ovan |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` och alla inbyggda feltyper | [print-object](functions/printing.md#5-print-object-utskriftsform-per-typ) |

`Eq`/`Ord` för `cons-cell<A,B>` kan användas när elementtyperna implementerar `Eq`/`Ord`.

### 6.2 Aritmetik

| Trait | Implementerande typer |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Detaljer finns i [Aritmetiska traits](functions/traits.md#3-aritmetiska-traits-add--sub--mul--div--rem--bits--number).

### 6.3 Iteration

| Trait | Implementerande typer |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Strömmar

| Typ | Implementerade traits |
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

Varje ström implementerar `Stream`; inströmmar implementerar också `InputStream` och utströmmar
`OutputStream`. `socket-listener` och `udp-socket` implementerar bara `Stream` (`close` /
`open-stream-p`). Detaljer finns i [Strömmar](functions/streams-files.md#1-trait-hierarkin).

### 6.5 Övrigt

| Trait | Implementerande typer | Detaljer |
|---|---|---|
| `Error` | Alla feltyper i kapitel 5 | [Feltyper](functions/option-result.md#3-feltyper-och-traitet-error) |
| `Pathish` | `string` `pathname` | [Pathnames](functions/streams-files.md#91-pathname-designator-traitet-pathish) |
