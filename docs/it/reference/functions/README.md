<!-- translated-from: docs/ja/reference/functions/README.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# Funzioni predefinite

L'elenco delle funzioni predefinite, dei metodi e della libreria standard. Per la sintassi (forme
speciali e modo di definire le cose), si veda il [Riferimento della sintassi](../syntax.md); per
l'elenco dei tipi, si veda [Tipi](../types.md).

## Forme di chiamata

Ci sono tre forme di chiamata.

- Funzioni libere: `(name args...)`
- Metodi di istanza: `(name receiver args...)` (risolti a partire dal tipo statico del primo argomento)
- Metodi statici (funzioni associate): `(Type::name args...)`

Ogni tipo può avere un proprio metodo con lo stesso nome. `(+ a b)` chiama il `+` del tipo di `a`.

## Nomi che richiedono un `use`

Dei nomi della libreria standard, questi si usano in ogni modulo senza scrivere nulla
([riferimento della sintassi 3.10](../syntax.md#310-module--use--namespace)):

- i tipi primitivi, `Option`/`Result`/`Vector`, l'iterazione, le S-espressioni e le liste, le funzioni
  sulle sequenze, le macro di controllo, il confronto e i trait aritmetici
- ogni nome che è anche un nome del pacchetto `COMMON-LISP` di Common Lisp (`length`, `format`, `open`
  ecc.)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, i trait di flusso `Stream`,
  `InputStream` e `OutputStream`, e `Hash`, che implementano le chiavi di una `HashTable`
- i tipi che nominano le loro funzioni e variabili

Ogni altro nome si porta dentro con un `use`, come `(use std::net::tcp-connect)`, o si scrive con il
suo percorso, `std::net::tcp-connect`. La tabella dà questi nomi e il modulo di ciascuno.

| Modulo | Nomi |
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

## Leggere le tabelle

Le tabelle di ciascun capitolo hanno le colonne "nome, forma, tipo, descrizione". La colonna del tipo si
scrive come `(tipo-dell-argomento,...)→tipo-di-ritorno`.

- Una singola lettera maiuscola come `T`, `A` o `B` è una variabile di tipo.
- Una nota come `where Eq A` è un vincolo di trait che la variabile di tipo deve soddisfare.
- `Iter<A>` significa "qualsiasi implementazione di `Iter` il cui `Item` sia `A`".
- Gli argomenti contrassegnati con `&optional` / `&key` possono essere omessi.

## Capitoli

| File | Contenuto |
|---|---|
| [numbers.md](numbers.md) | Interi, numeri in virgola mobile, razionali, numeri complessi, booleani, operazioni sui bit, numeri casuali |
| [sequences.md](sequences.md) | La coppia `cons-cell`, i dati S-expression `Sexpr`, i simboli, le funzioni sulle sequenze, iteratori pigri `lazy`, le funzioni di ordine superiore |
| [collections.md](collections.md) | Stringhe, caratteri, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, i tipi di errore e il trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, i trait aritmetici, `Push`, `Pop` |
| [printing.md](printing.md) | `print`/`println`/`format`, il pretty printer, `print-object`, le variabili di controllo della stampa |
| [format.md](format.md) | Direttive di formato |
| [streams-files.md](streams-files.md) | Stream, operazioni sui file, pathname, readtable |
| [concurrency.md](concurrency.md) | Task, canali, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, socket di dominio Unix, UDP |
| [system.md](system.md) | Tempo, ambiente di esecuzione, strumenti di implementazione, `read`/`eval`, docstring, funzioni legate alle macro |
