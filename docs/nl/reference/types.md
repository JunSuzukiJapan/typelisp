<!-- translated-from: docs/ja/reference/types.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Types

De types die typelisp heeft en de standaardtraits die elk type implementeert. Hoe je types schrijft
staat in [Syntaxreferentie hoofdstuk 2](syntax.md#2-types-schrijven); de functies en methoden van elk
type staan in [Ingebouwde functies](functions/README.md).

## 1. Primitieve types

| Type | Inhoud | Details |
|---|---|---|
| `int` | Een geheel getal met willekeurige precisie. Wordt als directe waarde bewaard zolang het in 63 bits past, en wordt daarboven automatisch een bignum. Het standaardtype van gehele literals zonder annotatie | [Getallen hoofdstuk 3](functions/numbers.md#3-gehele-getallen-met-willekeurige-precisie-int) |
| `i8` `i16` `i32` | Gehele getallen met teken en vaste breedte | [Getallen hoofdstuk 1](functions/numbers.md#1-gehele-getallen-met-vaste-breedte) |
| `u8` `u16` `u32` | Gehele getallen zonder teken en met vaste breedte | Idem |
| `f32` `f64` | IEEE-754-drijvendekommagetallen. Decimale literals zijn standaard `f64` | [Getallen hoofdstuk 4](functions/numbers.md#4-drijvendekommagetallen-f64--f32) |
| `ratio` | Een rationaal getal in laagste termen | [Getallen hoofdstuk 5](functions/numbers.md#5-rationale-getallen-ratio) |
| `bool` | `true` / `false` | [Getallen hoofdstuk 7](functions/numbers.md#7-booleans) |
| `char` | Een Unicode-scalarwaarde | [Tekens](functions/collections.md#2-tekens-char) |
| `string` | Een onveranderlijke string | [Strings](functions/collections.md#1-strings-string) |
| `symbol` | Een symbool. Keywords (`:name`) hebben ook dit type | [Symbolen](functions/sequences.md#3-symbolen) |
| `()` | Het type Unit. Zijn waarde is ook `()` | |
| `!` | Het type Never. Het type van expressies die niet terugkeren, zoals `panic`. Kan worden geplaatst waar een willekeurig type wordt verwacht | |
| `ptr` `c-long` `c-ulong` | Woorden die alleen worden gebruikt om waarden van en naar C door te geven. Ze kunnen alleen binnen `unsafe` waarden zijn, en de plekken waar ze mogen voorkomen zijn beperkt | [Getallen hoofdstuk 2](functions/numbers.md#2-ruwe-woorden-aan-de-c-grens-ptr--c-long--c-ulong) |
| `random-state` | De toestand van een generator voor willekeurige getallen | [Getallen hoofdstuk 12](functions/numbers.md#12-willekeurige-getallen) |

Er is geen 64-bits geheel type. Gebruik voor gehele getallen waarvan de breedte er niet toe doet
`int`.

## 2. Ingebouwde generieke types

| Type | Inhoud | Details |
|---|---|---|
| `Option<T>` | Een waarde die er is of niet. `some` / `none` | [Option en Result](functions/option-result.md) |
| `Result<T,E>` | Succes of mislukking. `ok` / `err` | Idem |
| `Vector<T>` | Een groeibare array | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Een hashtabel. Het sleuteltype moet `Hash` implementeren | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Een handle naar een taak | [Taken](functions/concurrency.md#1-taskt--handles-voor-taken) |
| `Thread<T>` | Een handle naar een taak die op een eigen OS-thread draait | [Thread](functions/concurrency.md#7-threadt--eigen-os-threads) |
| `Chan<T>` | Een kanaal | [Kanalen](functions/concurrency.md#2-chant--kanalen) |

Functietypes worden geschreven als `(fn (argumenttypes...) returntype)`, en trait-objecten als
`:dyn Trait` ([Syntaxreferentie hoofdstuk 2](syntax.md#2-types-schrijven)).

## 3. S-expressiedata

| Type | Inhoud | Details |
|---|---|---|
| `Sexpr` | Een niet-lege S-expressie. 16 varianten: `int`, `i8` tot en met `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path` | [S-expressiedata](functions/sequences.md#2-s-expressiedata-sexpr) |
| `Option<Sexpr>` | S-expressiedata in het algemeen. De lege lijst `()` is `none` | Idem |

## 4. Types in de standaardbibliotheek

Types die de standaardbibliotheek (de prelude) met `defstruct` / `defenum` definieert. Ze worden
hetzelfde behandeld als types die je zelf schrijft, en alles wat met een `defstruct` kan, kan ook met
hen.

| Type | Inhoud | Details |
|---|---|---|
| `cons-cell<A,B>` | Een paar. `cons`/`car`/`cdr` | [Paren](functions/sequences.md#1-paren-cons-cellab) |
| `complex` | Een complex getal (componenten van `f64`) | [Getallen hoofdstuk 6](functions/numbers.md#6-complexe-getallen-complex) |
| `Array<T>` | Een meerdimensionale array | [Array](functions/collections.md#5-arrayt-meerdimensionale-arrays) |
| `BitVector` | Een bitreeks met vaste lengte | [BitVector](functions/collections.md#6-bitvector-bitvectoren) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | De iterators die `iter` van elke collectie teruggeeft | [Iter](functions/traits.md#1-de-trait-iter-en-iteratie) |
| `WaitGroup` | Wachten tot N dingen klaar zijn | [WaitGroup](functions/concurrency.md#4-waitgroup--wachten-op-n-voltooiingen) |
| `Mutex<T>` | Wederzijdse uitsluiting voor gedeelde gegevens | [Mutex](functions/concurrency.md#6-mutext--wederzijdse-uitsluiting-voor-gedeelde-gegevens) |
| `pathname` | Een in delen gesplitste bestandsnaam | [Padnamen](functions/streams-files.md#9-padnamen-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Streams | [Streams](functions/streams-files.md#3-concrete-streamtypes) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Samengestelde streams | [Samengestelde streams](functions/streams-files.md#4-samengestelde-streams) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Netwerk | [Netwerk](functions/network.md#1-types) |
| `ReadOutcome` | Het resultaat van `read-sexpr`. `datum` / `eof` | [Streams](functions/streams-files.md#6-generieke-functies-en-bestandsbewerkingen) |
| `universal-time` `internal-time` `decoded-time` | Tijd | [Tijd](functions/system.md#1-tijd) |
| `heap-info` | De huidige toestand van de heap | [Implementatiehulpmiddelen](functions/system.md#51-velden-van-heap-info) |

## 5. Foutentypes

`Error` is geen type maar een trait, en de volgende types implementeren hem. Schrijf `:dyn Error`
om fouten van elke soort af te handelen.

| Type | Geproduceerd door |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Bestands- en streambewerkingen |
| `NetError` | Netwerkbewerkingen |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Details staan in [Foutentypes en de trait Error](functions/option-result.md#3-foutentypes-en-de-trait-error).

## 6. Implementaties van standaardtraits

Welke types welke traits implementeren. De methoden van elke trait staan in
[Standaardtraits](functions/traits.md) en in de hoofdstukken die in de meest rechtse kolom worden
genoemd.

### 6.1 Vergelijking, hashing en afdrukken

| Trait | Implementerende types | Details |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-vergelijking) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Idem |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` en alle ingebouwde foutentypes | [print-object](functions/printing.md#5-print-object-afgedrukte-weergave-per-type) |

`Eq`/`Ord` van `cons-cell<A,B>` kunnen worden gebruikt wanneer de elementtypes `Eq`/`Ord`
implementeren.

### 6.2 Rekenkunde

| Trait | Implementerende types |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Details staan in [Rekenkundige traits](functions/traits.md#3-rekenkundige-traits-add--sub--mul--div--rem--bits--number).

### 6.3 Iteratie

| Trait | Implementerende types |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Streams

| Type | Geïmplementeerde traits |
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

Elke stream implementeert `Stream`; invoerstreams implementeren ook `InputStream` en uitvoerstreams
`OutputStream`. `socket-listener` en `udp-socket` implementeren alleen `Stream` (`close` /
`open-stream-p`). Details staan in [Streams](functions/streams-files.md#1-traitstructuur).

### 6.5 Overige

| Trait | Implementerende types | Details |
|---|---|---|
| `Error` | Alle foutentypes uit hoofdstuk 5 | [Foutentypes](functions/option-result.md#3-foutentypes-en-de-trait-error) |
| `Pathish` | `string` `pathname` | [Padnamen](functions/streams-files.md#91-de-pathname-designator-trait-pathish) |
