<!-- translated-from: docs/ja/reference/types.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Typy

Typy, które ma typelisp, oraz standardowe traity implementowane przez każdy typ. Sposób zapisu typów opisano w
[Referencji składni, rozdział 2](syntax.md#2-zapis-typów); funkcje i metody każdego typu znajdują się w
[Funkcjach wbudowanych](functions/README.md).

## 1. Typy prymitywne

| Typ | Zawartość | Szczegóły |
|---|---|---|
| `int` | Liczba całkowita o dowolnej precyzji. Przechowywana jako wartość natychmiastowa, dopóki mieści się w 63 bitach, a poza tym zakresem automatycznie staje się bignum. Domyślny typ literałów całkowitych bez adnotacji | [Liczby, rozdział 3](functions/numbers.md#3-liczby-całkowite-o-dowolnej-precyzji-int) |
| `i8` `i16` `i32` | Liczby całkowite ze znakiem o stałej szerokości | [Liczby, rozdział 1](functions/numbers.md#1-liczby-całkowite-o-stałej-szerokości) |
| `u8` `u16` `u32` | Liczby całkowite bez znaku o stałej szerokości | Jak wyżej |
| `f32` `f64` | Liczby zmiennoprzecinkowe IEEE-754. Literały dziesiętne domyślnie mają typ `f64` | [Liczby, rozdział 4](functions/numbers.md#4-liczby-zmiennoprzecinkowe-f64--f32) |
| `ratio` | Liczba wymierna w postaci nieskracalnej | [Liczby, rozdział 5](functions/numbers.md#5-liczby-wymierne-ratio) |
| `bool` | `true` / `false` | [Liczby, rozdział 7](functions/numbers.md#7-wartości-logiczne) |
| `char` | Skalarna wartość Unicode | [Znaki](functions/collections.md#2-znaki-char) |
| `string` | Niezmienny łańcuch znaków | [Łańcuchy znaków](functions/collections.md#1-łańcuchy-znaków-string) |
| `symbol` | Symbol. Słowa kluczowe (`:name`) też mają ten typ | [Symbole](functions/sequences.md#3-symbole) |
| `()` | Typ Unit. Jego wartością jest również `()` | |
| `!` | Typ Never. Typ wyrażeń, które nie wracają, takich jak `panic`. Można go umieścić tam, gdzie oczekiwany jest dowolny typ | |
| `ptr` `c-long` `c-ulong` | Słowa używane tylko do przekazywania wartości do C i z C. Mogą być wartościami tylko wewnątrz `unsafe`, a miejsca ich występowania są ograniczone | [Liczby, rozdział 2](functions/numbers.md#2-surowe-słowa-na-granicy-z-c-ptr--c-long--c-ulong) |
| `random-state` | Stan generatora liczb losowych | [Liczby, rozdział 12](functions/numbers.md#12-liczby-losowe) |

Nie ma 64-bitowego typu całkowitego. Dla liczb całkowitych, których szerokość nie ma znaczenia, używaj `int`.

## 2. Wbudowane typy generyczne

| Typ | Zawartość | Szczegóły |
|---|---|---|
| `Option<T>` | Wartość, która jest albo jej nie ma. `some` / `none` | [Option i Result](functions/option-result.md) |
| `Result<T,E>` | Sukces lub porażka. `ok` / `err` | Jak wyżej |
| `Vector<T>` | Rosnąca tablica | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Tablica mieszająca. Typ klucza musi implementować `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Uchwyt zadania | [Zadania](functions/concurrency.md#1-taskt--uchwyty-zadań) |
| `Thread<T>` | Uchwyt zadania uruchomionego na dedykowanym wątku systemu operacyjnego | [Thread](functions/concurrency.md#7-threadt--dedykowane-wątki-systemu-operacyjnego) |
| `Chan<T>` | Kanał | [Kanały](functions/concurrency.md#2-chant--kanały) |

Typy funkcyjne zapisuje się `(fn (typy-argumentów...) typ-zwracany)`, a obiekty traitów `:dyn Trait`
([Referencja składni, rozdział 2](syntax.md#2-zapis-typów)).

## 3. Dane w postaci S-wyrażeń

| Typ | Zawartość | Szczegóły |
|---|---|---|
| `Sexpr` | Niepuste S-wyrażenie. 18 wariantów: `int`, od `i8` do `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array` | [Dane w postaci S-wyrażeń](functions/sequences.md#2-dane-w-postaci-s-wyrażeń-sexpr) |
| `Option<Sexpr>` | Dane w postaci S-wyrażeń w ogólności. Pusta lista `()` to `none` | Jak wyżej |

## 4. Typy w bibliotece standardowej

Typy, które biblioteka standardowa (prelude) definiuje za pomocą `defstruct` / `defenum`. Są traktowane tak
samo jak typy, które piszesz sam, i wszystko, co można zrobić z `defstruct`, można zrobić także z nimi.

| Typ | Zawartość | Szczegóły |
|---|---|---|
| `cons-cell<A,B>` | Para. `cons`/`car`/`cdr` | [Pary](functions/sequences.md#1-pary-cons-cellab) |
| `complex` | Liczba zespolona (składowe `f64`) | [Liczby, rozdział 6](functions/numbers.md#6-liczby-zespolone-complex) |
| `Array<T>` | Tablica wielowymiarowa | [Array](functions/collections.md#5-arrayt-tablice-wielowymiarowe) |
| `BitVector` | Ciąg bitów o stałej długości | [BitVector](functions/collections.md#6-bitvector-wektory-bitowe) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Iteratory zwracane przez `iter` każdej kolekcji | [Iter](functions/traits.md#1-trait-iter-i-iteracja) |
| `WaitGroup` | Oczekiwanie na zakończenie N rzeczy | [WaitGroup](functions/concurrency.md#4-waitgroup--oczekiwanie-na-n-zakończeń) |
| `Mutex<T>` | Wzajemne wykluczanie dla współdzielonych danych | [Mutex](functions/concurrency.md#6-mutext--wzajemne-wykluczanie-dla-współdzielonych-danych) |
| `pathname` | Nazwa pliku podzielona na części | [Nazwy ścieżek](functions/streams-files.md#9-nazwy-ścieżek-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Strumienie | [Strumienie](functions/streams-files.md#3-konkretne-typy-strumieni) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Strumienie złożone | [Strumienie złożone](functions/streams-files.md#4-strumienie-złożone) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Sieć | [Sieć](functions/network.md#1-typy) |
| `ReadOutcome` | Wynik `read-sexpr`. `datum` / `eof` | [Strumienie](functions/streams-files.md#6-funkcje-generyczne-i-operacje-na-plikach) |
| `universal-time` `internal-time` `decoded-time` | Czas | [Czas](functions/system.md#1-czas) |
| `heap-info` | Bieżący stan sterty | [Narzędzia implementacji](functions/system.md#51-pola-heap-info) |

## 5. Typy błędów

`Error` nie jest typem, lecz traitem, który implementują poniższe typy. Aby obsługiwać błędy dowolnego
rodzaju, zapisz `:dyn Error`.

| Typ | Tworzony przez |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operacje na plikach i strumieniach |
| `NetError` | Operacje sieciowe |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Szczegóły znajdują się w [Typach błędów i traicie Error](functions/option-result.md#3-typy-błędów-i-trait-error).

## 6. Implementacje traitów standardowych

Które typy implementują które traity. Metody każdego traitu znajdują się w
[Traitach standardowych](functions/traits.md) oraz w rozdziałach wymienionych w skrajnej prawej kolumnie.

### 6.1 Porównywanie, haszowanie i wypisywanie

| Trait | Typy implementujące | Szczegóły |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-porównywanie) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Jak wyżej |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` oraz wszystkie wbudowane typy błędów | [print-object](functions/printing.md#5-print-object-reprezentacja-wypisywana-według-typu) |

`Eq`/`Ord` dla `cons-cell<A,B>` można używać, gdy typy elementów implementują `Eq`/`Ord`.

### 6.2 Arytmetyka

| Trait | Typy implementujące |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Szczegóły znajdują się w [Traitach arytmetycznych](functions/traits.md#3-traity-arytmetyczne-add--sub--mul--div--rem--bits--number).

### 6.3 Iteracja

| Trait | Typy implementujące |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Strumienie

| Typ | Zaimplementowane traity |
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

Każdy strumień implementuje `Stream`; strumienie wejściowe implementują także `InputStream`, a strumienie wyjściowe
`OutputStream`. `socket-listener` i `udp-socket` implementują tylko `Stream` (`close` /
`open-stream-p`). Szczegóły znajdują się w [Strumieniach](functions/streams-files.md#1-hierarchia-traitów).

### 6.5 Pozostałe

| Trait | Typy implementujące | Szczegóły |
|---|---|---|
| `Error` | Wszystkie typy błędów z rozdziału 5 | [Typy błędów](functions/option-result.md#3-typy-błędów-i-trait-error) |
| `Pathish` | `string` `pathname` | [Nazwy ścieżek](functions/streams-files.md#91-trait-desygnatora-nazwy-ścieżki-pathish) |
