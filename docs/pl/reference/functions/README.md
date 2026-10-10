<!-- translated-from: docs/ja/reference/functions/README.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
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
