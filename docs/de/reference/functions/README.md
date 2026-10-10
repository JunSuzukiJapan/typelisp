<!-- translated-from: docs/ja/reference/functions/README.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
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
| [concurrency.md](concurrency.md) | Tasks, Kanäle, `WaitGroup`, `Mutex`, `Thread` |
| [network.md](network.md) | TCP, TLS, Unix-Domain-Sockets, UDP |
| [system.md](system.md) | Zeit, Laufzeitumgebung, Werkzeuge der Implementierung, `read`/`eval`, Docstrings, makrobezogene Funktionen |
