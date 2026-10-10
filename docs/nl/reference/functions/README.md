<!-- translated-from: docs/ja/reference/functions/README.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
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
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, rekenkundige traits |
| [printing.md](printing.md) | `print`/`println`/`format`, de pretty printer, `print-object`, besturingsvariabelen van de printer |
| [format.md](format.md) | Formatdirectieven |
| [streams-files.md](streams-files.md) | Streams, bestandsbewerkingen, padnamen, readtable |
| [concurrency.md](concurrency.md) | Taken, kanalen, `WaitGroup`, `Mutex`, `Thread` |
| [network.md](network.md) | TCP, TLS, Unix-domainsockets, UDP |
| [system.md](system.md) | Tijd, de runtime-omgeving, implementatiehulpmiddelen, `read`/`eval`, docstrings, macrogerelateerde functies |
