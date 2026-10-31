<!-- translated-from: docs/ja/reference/functions/README.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# Ingebouwde functies

De lijst met ingebouwde functies, methoden en de standaardbibliotheek. Voor syntaxis (speciale vormen
en hoe je dingen definieert) zie de [Syntaxreferentie](../syntax.md); voor de lijst met types zie
[Types](../types.md).

## Aanroepvormen

Er zijn drie aanroepvormen.

- Vrije functies: `(name args...)`
- Instantiemethoden: `(name receiver args...)` (opgelost vanuit het statische type van het eerste
  argument)
- Statische methoden (geassocieerde functies): `(Type::name args...)`

Elk type kan een eigen methode met dezelfde naam hebben. `(+ a b)` roept de `+` van het type van `a`
aan.

## Namen die een `use` nodig hebben

Van de namen van de standaardbibliotheek zijn deze in elk module te gebruiken zonder iets te schrijven
([syntaxisreferentie 3.10](../syntax.md#310-module--use--namespaces)):

- de primitieve typen, `Option`/`Result`/`Vector`, iteratie, S-expressies en lijsten, de reeksfuncties,
  de besturingsmacro's, vergelijking en de rekenkundige traits
- elke naam die ook een naam is van het `COMMON-LISP`-pakket van Common Lisp (`length`, `format`,
  `open` enzovoort)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, de stream-traits `Stream`,
  `InputStream` en `OutputStream`, en `Hash`, dat de sleutels van een `HashTable` implementeren
- de typen die de functies en variabelen hiervan noemen

Elke andere naam haal je binnen met een `use`, zoals `(use std::net::tcp-connect)`, of schrijf je als
pad, `std::net::tcp-connect`. De tabel geeft die namen en het module van elk.

| Module | Namen |
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

## De tabellen lezen

De tabellen in elk hoofdstuk hebben de kolommen "naam, vorm, type, beschrijving". De typekolom wordt
geschreven als `(argumenttype,...)→returntype`.

- Een enkele hoofdletter zoals `T`, `A` of `B` is een typevariabele.
- Een aantekening zoals `where Eq A` is een trait bound waaraan de typevariabele moet voldoen.
- `Iter<A>` betekent "elke implementatie van `Iter` waarvan `Item` gelijk is aan `A`".
- Argumenten die met `&optional` / `&key` zijn gemarkeerd mogen worden weggelaten.

## Hoofdstukken

| Bestand | Inhoud |
|---|---|
| [numbers.md](numbers.md) | Gehele getallen, drijvendekommagetallen, rationale getallen, complexe getallen, booleans, bitbewerkingen, willekeurige getallen |
| [sequences.md](sequences.md) | Het paar `cons-cell`, S-expressiedata `Sexpr`, symbolen, sequentiefuncties, luie iterators `lazy`, functies van hogere orde |
| [collections.md](collections.md) | Strings, tekens, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, foutentypes en de trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, rekenkundige traits, `Push`, `Pop` |
| [printing.md](printing.md) | `print`/`println`/`format`, de pretty printer, `print-object`, besturingsvariabelen van de printer |
| [format.md](format.md) | Formatdirectieven |
| [streams-files.md](streams-files.md) | Streams, bestandsbewerkingen, padnamen, readtable |
| [concurrency.md](concurrency.md) | Taken, kanalen, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix-domainsockets, UDP |
| [system.md](system.md) | Tijd, de runtime-omgeving, implementatiehulpmiddelen, `read`/`eval`, docstrings, macrogerelateerde functies |
