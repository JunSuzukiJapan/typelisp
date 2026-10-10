<!-- translated-from: docs/ja/reference/functions/README.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
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
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, i trait aritmetici |
| [printing.md](printing.md) | `print`/`println`/`format`, il pretty printer, `print-object`, le variabili di controllo della stampa |
| [format.md](format.md) | Direttive di formato |
| [streams-files.md](streams-files.md) | Stream, operazioni sui file, pathname, readtable |
| [concurrency.md](concurrency.md) | Task, canali, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, socket di dominio Unix, UDP |
| [system.md](system.md) | Tempo, ambiente di esecuzione, strumenti di implementazione, `read`/`eval`, docstring, funzioni legate alle macro |
