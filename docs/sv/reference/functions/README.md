<!-- translated-from: docs/ja/reference/functions/README.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
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
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, aritmetiska traits |
| [printing.md](printing.md) | `print`/`println`/`format`, den snygga skrivaren (pretty printer), `print-object`, kontrollvariabler för utskrift |
| [format.md](format.md) | Formatdirektiv |
| [streams-files.md](streams-files.md) | Strömmar, filoperationer, pathnames, readtable |
| [concurrency.md](concurrency.md) | Tasks, kanaler, `WaitGroup`, `Mutex`, `Thread` |
| [network.md](network.md) | TCP, TLS, Unix-domänsocketar, UDP |
| [system.md](system.md) | Tid, körmiljön, implementationsverktyg, `read`/`eval`, dokumentationssträngar, makrorelaterade funktioner |
