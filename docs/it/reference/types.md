<!-- translated-from: docs/ja/reference/types.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Tipi

I tipi che typelisp possiede e i trait standard che ciascun tipo implementa. Come si scrivono i tipi è
spiegato nel [capitolo 2 del Riferimento della sintassi](syntax.md#2-scrittura-dei-tipi); le funzioni e i
metodi di ciascun tipo si trovano in [Funzioni predefinite](functions/README.md).

## 1. Tipi primitivi

| Tipo | Contenuto | Dettagli |
|---|---|---|
| `int` | Un intero a precisione arbitraria. Tenuto come valore immediato finché sta in 63 bit, e diventa automaticamente un bignum oltre. Il tipo predefinito dei letterali interi senza annotazione | [Numeri, capitolo 3](functions/numbers.md#3-interi-a-precisione-arbitraria-int) |
| `i8` `i16` `i32` | Interi con segno a larghezza fissa | [Numeri, capitolo 1](functions/numbers.md#1-interi-a-larghezza-fissa) |
| `u8` `u16` `u32` | Interi senza segno a larghezza fissa | Come sopra |
| `f32` `f64` | Numeri in virgola mobile IEEE-754. I letterali decimali sono `f64` per impostazione predefinita | [Numeri, capitolo 4](functions/numbers.md#4-numeri-in-virgola-mobile-f64--f32) |
| `ratio` | Un numero razionale ridotto ai minimi termini | [Numeri, capitolo 5](functions/numbers.md#5-razionali-ratio) |
| `bool` | `true` / `false` | [Numeri, capitolo 7](functions/numbers.md#7-booleani) |
| `char` | Un valore scalare Unicode | [Caratteri](functions/collections.md#2-caratteri-char) |
| `string` | Una stringa immutabile | [Stringhe](functions/collections.md#1-stringhe-string) |
| `symbol` | Un simbolo. Anche le parole chiave (`:name`) hanno questo tipo | [Simboli](functions/sequences.md#3-simboli) |
| `()` | Il tipo Unit. Anche il suo valore è `()` | |
| `!` | Il tipo Never. Il tipo delle espressioni che non ritornano, come `panic`. Può essere messo dove è atteso un tipo qualsiasi | |
| `ptr` `c-long` `c-ulong` | Parole usate solo per passare valori da e verso C. Possono essere valori solo dentro `unsafe`, e i punti in cui possono comparire sono limitati | [Numeri, capitolo 2](functions/numbers.md#2-parole-grezze-al-confine-con-c-ptr--c-long--c-ulong) |
| `random-state` | Lo stato di un generatore di numeri casuali | [Numeri, capitolo 12](functions/numbers.md#12-numeri-casuali) |

Non esiste un tipo intero a 64 bit. Per gli interi la cui larghezza non importa, usa `int`.

## 2. Tipi generici predefiniti

| Tipo | Contenuto | Dettagli |
|---|---|---|
| `Option<T>` | Un valore che c'è o non c'è. `some` / `none` | [Option e Result](functions/option-result.md) |
| `Result<T,E>` | Successo o fallimento. `ok` / `err` | Come sopra |
| `Vector<T>` | Un array espandibile | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Una tabella hash. Il tipo della chiave deve implementare `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Un handle a un task | [Task](functions/concurrency.md#1-taskt--handle-dei-task) |
| `Thread<T>` | Un handle a un task in esecuzione su un thread del sistema operativo dedicato | [Thread](functions/concurrency.md#7-threadt--thread-del-sistema-operativo-dedicati) |
| `Chan<T>` | Un canale | [Canali](functions/concurrency.md#2-chant--canali) |

I tipi funzione si scrivono `(fn (tipi-degli-argomenti...) tipo-di-ritorno)`, e gli oggetti-trait
`:dyn Trait` ([capitolo 2 del Riferimento della sintassi](syntax.md#2-scrittura-dei-tipi)).

## 3. Dati S-expression

| Tipo | Contenuto | Dettagli |
|---|---|---|
| `Sexpr` | Una S-expression non vuota. 16 varianti: `int`, da `i8` a `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path` | [Dati S-expression](functions/sequences.md#2-dati-s-expression-sexpr) |
| `Option<Sexpr>` | I dati S-expression in generale. La lista vuota `()` è `none` | Come sopra |

## 4. Tipi della libreria standard

Tipi che la libreria standard (il prelude) definisce con `defstruct` / `defenum`. Sono trattati allo
stesso modo dei tipi che scrivi tu, e tutto ciò che si può fare con una `defstruct` si può fare con
essi.

| Tipo | Contenuto | Dettagli |
|---|---|---|
| `cons-cell<A,B>` | Una coppia. `cons`/`car`/`cdr` | [Coppie](functions/sequences.md#1-coppie-cons-cellab) |
| `complex` | Un numero complesso (componenti `f64`) | [Numeri, capitolo 6](functions/numbers.md#6-numeri-complessi-complex) |
| `Array<T>` | Un array multidimensionale | [Array](functions/collections.md#5-arrayt-array-multidimensionali) |
| `BitVector` | Una sequenza di bit a lunghezza fissa | [BitVector](functions/collections.md#6-bitvector-vettori-di-bit) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Gli iteratori restituiti da `iter` di ciascuna collezione | [Iter](functions/traits.md#1-il-trait-iter-e-literazione) |
| `WaitGroup` | Attendere il completamento di N cose | [WaitGroup](functions/concurrency.md#4-waitgroup--attendere-n-completamenti) |
| `Mutex<T>` | Mutua esclusione per dati condivisi | [Mutex](functions/concurrency.md#6-mutext--mutua-esclusione-per-dati-condivisi) |
| `pathname` | Un nome di file diviso in parti | [Pathname](functions/streams-files.md#9-pathname-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Stream | [Stream](functions/streams-files.md#3-tipi-di-stream-concreti) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Stream compositi | [Stream compositi](functions/streams-files.md#4-stream-compositi) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Rete | [Rete](functions/network.md#1-tipi) |
| `ReadOutcome` | Il risultato di `read-sexpr`. `datum` / `eof` | [Stream](functions/streams-files.md#6-funzioni-generiche-e-operazioni-sui-file) |
| `universal-time` `internal-time` `decoded-time` | Tempo | [Tempo](functions/system.md#1-tempo) |
| `heap-info` | Lo stato corrente dello heap | [Strumenti di implementazione](functions/system.md#51-campi-di-heap-info) |

## 5. Tipi di errore

`Error` non è un tipo ma un trait, e i tipi seguenti lo implementano. Per gestire errori di qualsiasi
genere, scrivi `:dyn Error`.

| Tipo | Prodotto da |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operazioni su file e stream |
| `NetError` | Operazioni di rete |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

I dettagli sono in [Tipi di errore e il trait Error](functions/option-result.md#3-tipi-di-errore-e-il-trait-error).

## 6. Implementazioni dei trait standard

Quali tipi implementano quali trait. I metodi di ciascun trait si trovano in
[Trait standard](functions/traits.md) e nei capitoli elencati nella colonna più a destra.

### 6.1 Confronto, hashing e stampa

| Trait | Tipi che lo implementano | Dettagli |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-confronto) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Come sopra |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` e tutti i tipi di errore predefiniti | [print-object](functions/printing.md#5-print-object-rappresentazione-stampata-per-tipo) |

`Eq`/`Ord` di `cons-cell<A,B>` si possono usare quando i tipi degli elementi implementano `Eq`/`Ord`.

### 6.2 Aritmetica

| Trait | Tipi che lo implementano |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

I dettagli sono in [Trait aritmetici](functions/traits.md#3-trait-aritmetici-add--sub--mul--div--rem--bits--number).

### 6.3 Iterazione

| Trait | Tipi che lo implementano |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Stream

| Tipo | Trait implementati |
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

Ogni stream implementa `Stream`; gli stream di input implementano anche `InputStream`, e quelli di
output `OutputStream`. `socket-listener` e `udp-socket` implementano solo `Stream` (`close` /
`open-stream-p`). I dettagli sono in [Stream](functions/streams-files.md#1-la-gerarchia-dei-trait).

### 6.5 Altri

| Trait | Tipi che lo implementano | Dettagli |
|---|---|---|
| `Error` | Tutti i tipi di errore del capitolo 5 | [Tipi di errore](functions/option-result.md#3-tipi-di-errore-e-il-trait-error) |
| `Pathish` | `string` `pathname` | [Pathname](functions/streams-files.md#91-il-trait-designatore-di-pathname-pathish) |
