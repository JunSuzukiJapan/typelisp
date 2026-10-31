<!-- translated-from: docs/ja/reference/functions/README.md @ 2d61c22ad717f8ad6dfd178f397927e87e1d8902 -->
# Inbyggda funktioner

Listan över inbyggda funktioner, metoder och standardbiblioteket. För syntax (specialformer och hur man
definierar saker), se [Syntaxreferensen](../syntax.md); för listan över typer, se
[Typer](../types.md).

## Anropsformer

Det finns tre anropsformer.

- Fria funktioner: `(name args...)`
- Instansmetoder: `(name receiver args...)` (löses upp från det första argumentets statiska typ)
- Statiska metoder (associerade funktioner): `(Type::name args...)`

Varje typ kan ha en egen metod med samma namn. `(+ a b)` anropar `+` för `a`:s typ.

## Namn som behöver en `use`

Av standardbibliotekets namn kan dessa användas i varje modul utan att skriva något
([syntaxreferens 3.10](../syntax.md#310-module--use--namnrymder)):

- de primitiva typerna, `Option`/`Result`/`Vector`, iteration, S-uttryck och listor,
  sekvensfunktionerna, styrmakrona, jämförelse och de aritmetiska traitsen
- varje namn som också är ett namn i Common Lisps paket `COMMON-LISP` (`length`, `format`, `open`
  med flera)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, ströminterfacen `Stream`,
  `InputStream` och `OutputStream`, samt `Hash`, som nycklarna i en `HashTable` implementerar
- typerna som dessas funktioner och variabler nämner

Alla andra namn tas in med en `use`, som `(use std::net::tcp-connect)`, eller skrivs med sin väg,
`std::net::tcp-connect`. Tabellen ger dessa namn och modulen för vart och ett.

| Modul | Namn |
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

## Läsa tabellerna

Tabellerna i varje kapitel har kolumnerna "namn, form, typ, beskrivning". Typkolumnen skrivs som
`(argumenttyp,...)→returtyp`.

- En enda stor bokstav som `T`, `A` eller `B` är en typvariabel.
- En not som `where Eq A` är en trait-gräns som typvariabeln måste uppfylla.
- `Iter<A>` betyder "vilken implementation av `Iter` som helst vars `Item` är `A`".
- Argument markerade `&optional` / `&key` kan utelämnas.

## Kapitel

| Fil | Innehåll |
|---|---|
| [numbers.md](numbers.md) | Heltal, flyttal, kvoter, komplexa tal, booleska värden, bitoperationer, slumptal |
| [sequences.md](sequences.md) | Paret `cons-cell`, S-uttrycksdata `Sexpr`, symboler, sekvensfunktioner, lata iteratorer `lazy`, högre ordningens funktioner |
| [collections.md](collections.md) | Strängar, tecken, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, feltyper och traitet `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, aritmetiska traits, `Push` |
| [printing.md](printing.md) | `print`/`println`/`format`, den snygga skrivaren (pretty printer), `print-object`, kontrollvariabler för utskrift |
| [format.md](format.md) | Formatdirektiv |
| [streams-files.md](streams-files.md) | Strömmar, filoperationer, pathnames, readtable |
| [concurrency.md](concurrency.md) | Tasks, kanaler, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix-domänsocketar, UDP |
| [system.md](system.md) | Tid, körmiljön, implementationsverktyg, `read`/`eval`, dokumentationssträngar, makrorelaterade funktioner |
