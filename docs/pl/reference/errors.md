<!-- translated-from: docs/ja/reference/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Komunikaty o błędach

Co oznaczają główne komunikaty o błędach z `typl` i jak je naprawić.

## 1. Odczytywanie błędu

Błędy są zapisywane na standardowe wyjście błędów w tej postaci:

```text
error: file:line:column: kind: message
```

`kind` mówi, kiedy błąd został znaleziony.

| Rodzaj | Kiedy | Znaczenie |
|---|---|---|
| `type error` | Przed uruchomieniem (w czasie sprawdzania) | Błąd w typach lub nazwach. Ta forma nie jest uruchamiana |
| (brak rodzaju) | Przy wczytywaniu lub sprawdzaniu | Błąd składni, taki jak niezbalansowane nawiasy, albo nazwa, której nie można znaleźć |
| `panic` | W trakcie działania | Nieodwracalne niepowodzenie. Program zatrzymuje się po wykonaniu kodu sprzątającego `unwind-protect` |

Linie zaczynające się od `warning:` to ostrzeżenia, a przetwarzanie trwa dalej.

`file:line:column` wskazuje wyrażenie z błędem. W przypadku błędu w czasie działania, który zdarza się
wewnątrz funkcji biblioteki standardowej, wskazuje miejsce, w którym program wywołał tę funkcję.
Niektóre błędy nie mają pozycji (jak `error: panic: ...`).

Przykład:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Oznacza to, że wyrażenie w linii 1, kolumnie 24 pliku `main.typl` było typu `string` tam, gdzie oczekiwano `i32`.

## 2. Błędy w czasie sprawdzania

Błędy znalezione przed uruchomieniem. Forma nie jest uruchamiana, dopóki nie zostaną naprawione.

### 2.1 Typy

| Komunikat | Znaczenie i naprawa |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Wyrażenie typu `U` stoi tam, gdzie potrzebny jest typ `T`. Nie ma niejawnych konwersji; dla liczb przekonwertuj za pomocą `(as T x)`. `int` i `i32` to także różne typy |
| ``integer literal 300 is out of range for u8 (0..=255)`` | Literał nie mieści się w typie. Jeśli chcesz go obciąć, napisz `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Nie ma typu o tej nazwie. Zdefiniuj typ przed pierwszą formą, która go używa (typy nie mają deklaracji wyprzedzających). Jeśli miałeś na myśli zmienną typową, zapisz ją w pozycji deklarującej, takiej jak `<foo>` po nazwie funkcji ([Referencja składni 3.6](syntax.md#36-defstruct--struktury-typy-definiowane-przez-użytkownika)) |
| ``cannot infer type argument `t` for `vector::new` `` | Nie da się określić argumentu typu. Zapisz typ za pomocą `the`, jak w `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` nie obsługuje każdego wariantu. Dodaj ramiona dla brakujących wariantów lub ramię `_` |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | Funkcja wymaga traitu, którego przekazany typ nie implementuje. Napisz `(impl Eq pt ...)` ([Traity standardowe](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Wartość typu, który nie implementuje traitu, została przekazana tam, gdzie oczekiwane jest `:dyn`. Napisz `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Nazwę traitu zapisano tam, gdzie powinien stać typ. Napisz `:dyn Error` |
| ``if: (if cond then else)`` | `if` ma niewłaściwy kształt. `if` wymaga gałęzi else. Gdy jej nie potrzebujesz, użyj `when` |

### 2.2 Nazwy

| Komunikat | Znaczenie i naprawa |
|---|---|
| `no such function: bar` | Nie ma funkcji ani metody o tej nazwie. Sprawdź pisownię |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Metody są wybierane według typu pierwszego argumentu. Metoda o tej nazwie istnieje, ale nie dla typu pierwszego argumentu (tutaj `int`). Koniec komunikatu wymienia typy, które mają tę metodę |
| `unbound variable: y` | Nie ma zmiennej o tej nazwie. Sprawdź pisownię i zakres wiązania (czy jest użyta poza swoim `let`?) |
| ``use: unresolved `nosuch` `` | Nie można znaleźć modułu wskazanego w `use`. Informacje o tym, jak nazwy plików odpowiadają ścieżkom modułów, znajdziesz w [Referencji składni 3.11](syntax.md#311-pliki-i-moduły-projekty-wieloplikowe) |
| `unresolved path: c::hidden` | Moduł istnieje, ale nazwa nie, albo nie jest widoczna, bo brakuje jej `pub` |
| `circular module dependency: a -> b -> a` | Moduły wprowadzają się nawzajem przez `use`. Przenieś wspólną część do osobnego modułu |
| ``return-from: no enclosing block named `nope` `` | Żaden `block` o nazwie podanej w `return-from` go nie otacza. Bloku funkcji można używać tylko wewnątrz tej funkcji |

### 2.3 Wywołania

| Komunikat | Znaczenie i naprawa |
|---|---|
| `f: expected 1 argument(s), got 2` | Liczba argumentów się nie zgadza |
| `f: unknown keyword argument :b` | Przekazano argument kluczowy, którego funkcja nie ma |
| `new: expected 1 field(s), got 2` | Liczba wartości przekazanych do konstruktora struktury nie zgadza się z liczbą pól |
| ``setf: cannot assign to constant `k` `` | Przypisano do nazwy zdefiniowanej za pomocą `defconstant`. Jeśli ma się zmieniać, użyj `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Funkcja zadeklarowana za pomocą `defsignature` nie jest zdefiniowana |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | Żaden z typów argumentów nie ma metody wywoływanej za pomocą `~/name/` ([Dyrektywy formatu, rozdział 5](functions/format.md#5-name)) |

## 3. Błędy odczytu

| Komunikat | Znaczenie i naprawa |
|---|---|
| `unexpected end of input while reading a list` | Brakuje nawiasu zamykającego. Pozycja wskazuje miejsce, w którym zakończył się odczyt (na przykład koniec pliku), więc szukaj nawiasu otwierającego |

## 4. Błędy w czasie działania (panic)

| Komunikat | Znaczenie i naprawa |
|---|---|
| `panic: divide by zero` | Dzielenie przez zero na liczbach całkowitych lub ułamkach. Dzielenie zmiennoprzecinkowe przez zero nie powoduje panic; daje `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` zastosowano do `none`. Obsłuż przypadek `none` za pomocą `match` lub `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Indeks poza zakresem. Sprawdź długość za pomocą `len` albo użyj funkcji zwracającej `none` poza zakresem (`nth`, `pop` i tak dalej) |
| `panic: an integer argument does not fit a fixnum` | `int`, który nie mieści się w 63 bitach, przekazano do argumentu przyjmującego indeks lub liczbę |
| `throw: no enclosing (catch 'oops) for this throw` | Wykonano `throw` bez otaczającego `catch` z tym samym znacznikiem |
| `panic: <message>` | Program wywołał `(panic "<message>")`. Nieudane `assert` daje `assertion failed: ...` |

`panic` zatrzymuje cały proces nawet wtedy, gdy zdarza się wewnątrz zadania
([Referencja składni 12.4](syntax.md#124-współdziałanie-z-innymi-funkcjami)). Niepowodzenia, z których chcesz
się podnieść, wyrażaj za pomocą `Result` ([Referencja składni, rozdział 9](syntax.md#9-zasady-obsługi-błędów)).

## 5. Ostrzeżenia

| Komunikat | Znaczenie |
|---|---|
| ``warning: redefining function `f` `` | Funkcja o tej samej nazwie została zdefiniowana ponownie. Obowiązuje późniejsza definicja. Pojawia się normalnie, gdy poprawiasz definicję w REPL |
