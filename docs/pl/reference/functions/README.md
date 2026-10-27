<!-- translated-from: docs/ja/reference/functions/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# Funkcje wbudowane

Lista funkcji wbudowanych, metod i biblioteki standardowej. Składnię (formy specjalne i sposoby
definiowania) znajdziesz w [Referencji składni](../syntax.md), a listę typów w
[Typach](../types.md).

## Formy wywołania

Istnieją trzy formy wywołania.

- Funkcje wolne: `(name args...)`
- Metody instancji: `(name receiver args...)` (rozstrzygane na podstawie statycznego typu pierwszego argumentu)
- Metody statyczne (funkcje powiązane): `(Type::name args...)`

Każdy typ może mieć własną metodę o tej samej nazwie. `(+ a b)` wywołuje `+` typu `a`.

## Nazwy, które wymagają `use`

Spośród nazw biblioteki standardowej tych można używać w każdym module, nic nie pisząc
([opis składni 3.10](../syntax.md#310-module--use--przestrzenie-nazw)):

- typy proste, `Option`/`Result`/`Vector`, iteracja, S-wyrażenia i listy, funkcje sekwencji, makra
  sterujące, porównanie i traity arytmetyczne
- każda nazwa, która jest też nazwą pakietu `COMMON-LISP` z Common Lisp (`length`, `format`, `open`
  itd.)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, traity strumieni `Stream`,
  `InputStream` i `OutputStream` oraz `Hash`, który implementują klucze `HashTable`
- typy, o których mówią ich funkcje i zmienne

Każdą inną nazwę wprowadza się przez `use`, jak `(use std::net::tcp-connect)`, albo pisze się jej
ścieżkę, `std::net::tcp-connect`. Tabela podaje te nazwy i moduł każdej z nich.

| Moduł | Nazwy |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`, `digit-weight`, `name->char` |
| `std::collections` | `Deque`, `HashSet`, `Scope`, `SortedTable`, `hashtable-iter` |
| `std::env` | `command-line-args`, `getenv`, `home-directory` |
| `std::error` | `WrappedError`, `as-dyn-error`, `assert-eq`, `describe-error`, `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`, `direction-input`, `direction-output`, `directory-p`, `open-binary`, `open-binary-input`, `open-binary-output`, `open-file`, `open-input`, `open-output`, `read-file-lines`, `read-file-string`, `write-file-string` |
| `std::hash` | `*sxhash-mask*`, `sxhash-string` |
| `std::io` | `ByteInput`, `ByteOutput`, `PeekInput`, `binary-file-stream`, `copy-stream`, `make-peek-stream`, `peek-stream`, `read-lines`, `write-lines` |
| `std::net` | `NetError`, `byte-stream-of`, `char-stream-of`, `datagram`, `socket-byte-stream`, `socket-listener`, `socket-stream`, `tcp-connect`, `tcp-listen`, `tls-connect`, `tls-listen`, `udp-bind`, `udp-socket`, `unix-connect`, `unix-listen`, `with-connection` |
| `std::num` | `ParseFloatError`, `ParseIntError`, `atan2`, `parse-float`, `parse-int` |
| `std::path` | `Pathish`, `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`, `read-from-string-preserving-whitespace`, `read-sexpr`, `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`, `right-trim`, `string->utf8`, `trim`, `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`, `Context`, `Mutex`, `WaitGroup`, `after`, `with-lock` |
| `std::thread` | `Task`, `Thread`, `yield` |
| `std::time` | `internal-time-seconds`, `timezone-daylight-p`, `timezone-offset-seconds` |

## Jak czytać tabele

Tabele w każdym rozdziale mają kolumny „nazwa, forma, typ, opis". Kolumna typu jest
zapisana jako `(typ-argumentu,...)→typ-zwracany`.

- Pojedyncza wielka litera, taka jak `T`, `A` lub `B`, to zmienna typowa.
- Uwaga w rodzaju `where Eq A` to ograniczenie traitu, które musi spełniać zmienna typowa.
- `Iter<A>` oznacza „dowolną implementację `Iter`, której `Item` to `A`".
- Argumenty oznaczone `&optional` / `&key` można pominąć.

## Rozdziały

| Plik | Zawartość |
|---|---|
| [numbers.md](numbers.md) | Liczby całkowite, zmiennoprzecinkowe, wymierne, zespolone, wartości logiczne, operacje bitowe, liczby losowe |
| [sequences.md](sequences.md) | Para `cons-cell`, dane w postaci S-wyrażeń `Sexpr`, symbole, funkcje na sekwencjach, leniwe iteratory `lazy`, funkcje wyższego rzędu |
| [collections.md](collections.md) | Łańcuchy znaków, znaki, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, typy błędów i trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, traity arytmetyczne |
| [printing.md](printing.md) | `print`/`println`/`format`, pretty printer, `print-object`, zmienne sterujące drukarką |
| [format.md](format.md) | Dyrektywy formatu |
| [streams-files.md](streams-files.md) | Strumienie, operacje na plikach, nazwy ścieżek, readtable |
| [concurrency.md](concurrency.md) | Zadania, kanały, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, gniazda domeny Unix, UDP |
| [system.md](system.md) | Czas, środowisko uruchomieniowe, narzędzia implementacji, `read`/`eval`, docstringi, funkcje związane z makrami |
