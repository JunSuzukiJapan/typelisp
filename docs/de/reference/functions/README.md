<!-- translated-from: docs/ja/reference/functions/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# Eingebaute Funktionen

Die Liste der eingebauten Funktionen, der Methoden und der Standardbibliothek. Zur Syntax (Spezialformen und
wie man Dinge definiert) siehe die [Syntaxreferenz](../syntax.md); zur Liste der Typen siehe
[Typen](../types.md).

## Aufrufformen

Es gibt drei Aufrufformen.

- Freie Funktionen: `(name args...)`
- Instanzmethoden: `(name receiver args...)` (aufgelöst über den statischen Typ des ersten Arguments)
- Statische Methoden (assoziierte Funktionen): `(Type::name args...)`

Jeder Typ kann eine eigene Methode gleichen Namens haben. `(+ a b)` ruft das `+` des Typs von `a` auf.

## Namen, die ein `use` brauchen

Von den Namen der Standardbibliothek sind diese in jedem Modul verwendbar, ohne etwas zu schreiben
([Syntaxreferenz 3.10](../syntax.md#310-module--use--namensräume)):

- die primitiven Typen, `Option`/`Result`/`Vector`, Iteration, S-Ausdrücke und Listen, die
  Sequenzfunktionen, die Steuermakros, Vergleich und die arithmetischen Traits
- jeder Name, der auch ein Name des `COMMON-LISP`-Pakets von Common Lisp ist (`length`, `format`,
  `open` usw.)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, die Stream-Traits `Stream`,
  `InputStream` und `OutputStream` sowie `Hash`, das die Schlüssel einer `HashTable` implementieren
- die Typen, die die Funktionen und Variablen davon erwähnen

Jeden anderen Namen holt man mit `use` herein, etwa `(use std::net::tcp-connect)`, oder schreibt
seinen Pfad, `std::net::tcp-connect`. Die Tabelle nennt diese Namen und das Modul, in dem jeder steht.

| Modul | Namen |
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

## Die Tabellen lesen

Die Tabellen in jedem Kapitel haben die Spalten „Name, Form, Typ, Beschreibung“. Die Typspalte wird als
`(Argumenttyp,...)→Rückgabetyp` geschrieben.

- Ein einzelner Großbuchstabe wie `T`, `A` oder `B` ist eine Typvariable.
- Ein Vermerk wie `where Eq A` ist eine Trait-Schranke, die die Typvariable erfüllen muss.
- `Iter<A>` bedeutet „jede Implementierung von `Iter`, deren `Item` `A` ist“.
- Mit `&optional` / `&key` markierte Argumente können weggelassen werden.

## Kapitel

| Datei | Inhalt |
|---|---|
| [numbers.md](numbers.md) | Ganzzahlen, Gleitkommazahlen, rationale Zahlen, komplexe Zahlen, Wahrheitswerte, Bitoperationen, Zufallszahlen |
| [sequences.md](sequences.md) | Das Paar `cons-cell`, S-Ausdrucksdaten `Sexpr`, Symbole, Sequenzfunktionen, lazy Iteratoren `lazy`, Funktionen höherer Ordnung |
| [collections.md](collections.md) | Zeichenketten, Zeichen, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, Fehlertypen und der Trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, arithmetische Traits |
| [printing.md](printing.md) | `print`/`println`/`format`, der Pretty Printer, `print-object`, Steuervariablen der Ausgabe |
| [format.md](format.md) | Formatdirektiven |
| [streams-files.md](streams-files.md) | Streams, Dateioperationen, Pfadnamen, readtable |
| [concurrency.md](concurrency.md) | Tasks, Kanäle, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, Unix-Domain-Sockets, UDP |
| [system.md](system.md) | Zeit, Laufzeitumgebung, Werkzeuge der Implementierung, `read`/`eval`, Docstrings, makrobezogene Funktionen |
