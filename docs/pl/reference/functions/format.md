<!-- translated-from: docs/ja/reference/functions/format.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Dyrektywy formatu

Dyrektywy zapisywane w łańcuchach sterujących `print`/`println`/`format`. Obejmują niemal wszystkie
dyrektywy `format` z CL. Same funkcje są opisane w
[Wypisywaniu](printing.md#1-print--println--format).

## 1. Jak zapisywać dyrektywy

Każda dyrektywa składa się, w tej kolejności, z `~`, opcjonalnych **parametrów prefiksowych** (rozdzielonych przecinkami: liczba całkowita / `'c` (znak) /
`v` (pobierany z następnego argumentu) / `#` (liczba pozostałych argumentów)), opcjonalnych
**modyfikatorów** `:` i `@` oraz znaku dyrektywy. Znaki dyrektyw
nie rozróżniają wielkości liter.

Łańcuch sterujący musi być literałem ([Wypisywanie](printing.md#1-print--println--format)). Poza
tym w czasie sprawdzania weryfikowane jest następujące.

- **Liczba i typy argumentów.** Dla każdej dyrektywy zużywającej argument: czy pozostał
  jakiś argument i czy jego typ jest akceptowany (uwagi „argument" w poniższych tabelach). Tam gdzie
  ścieżka zależy od wartości z czasu działania, jak przy przesuwaniu za pomocą `~*`, wyborze klauzuli `~[`,
  tym, czy `~^` zadziała, albo liczbie powtórzeń `~@{`, sprawdzana jest **każda ścieżka**. Nadmiarowe argumenty
  są dopuszczalne (jak w CL).
- **Parametry i modyfikatory.** Niedozwolony modyfikator, zbyt wiele parametrów oraz wartości spoza zakresu
  (ujemna szerokość, podstawa inna niż od 2 do 36, liczba całkowita tam, gdzie oczekiwany jest znak, i tak
  dalej) są błędami. Nigdy nie są po cichu ignorowane ani zaokrąglane.

**Elementy** argumentu będącego listą (`~{`, `~:{`, `~<...~:>`) to `Sexpr`, a ani ich liczba, ani
typ każdego elementu nie mogą być znane z typów. Wymagania wobec elementów (liczba całkowita dla `~d`
i tak dalej) oraz brakujące elementy są sprawdzane w chwili nadejścia wartości i są błędami czasu działania (nigdy
nie następuje przełączenie na inną reprezentację).

Łagodne reguły CL nie zostały przyjęte. Przekazanie nieliczby całkowitej do `~d` i wypisanie jej jak `~a`
albo traktowanie przez `~:[` dowolnej wartości jako wartości logicznej nie są reinterpretowane w ten sposób; są
błędami typu.

## 2. Wyjście (zużywające jeden argument)

| Dyrektywa | Parametry / modyfikatory | Znaczenie |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=wyrównanie do prawej | Estetyczne (`princ` z CL; łańcuchy bez cudzysłowów). Argument może być dowolnego typu |
| `~s` | Jak wyżej | Standardowe (`prin1` z CL; postać możliwa do wczytania z powrotem). Argument może być dowolnego typu |
| `~w` | — | `write` z CL. Wypisuje w sposób ozdobny (pretty), jeśli `*print-pretty*` jest prawdą, w przeciwnym razie tak samo jak `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=grupy cyfr, `@`=zawsze ze znakiem | Liczby całkowite dziesiętne/dwójkowe/ósemkowe/szesnastkowe. Argument jest liczbą całkowitą |
| `~r` | `~radix,mincol,padchar,commachar,interval` (z podstawą) lub brak | Z podstawą w tej podstawie (od 2 do 36). Bez niej: `~r`=angielskie liczebniki główne, `~:r`=angielskie liczebniki porządkowe, `~@r`=cyfry rzymskie, `~:@r`=stare cyfry rzymskie. Argument jest liczbą całkowitą |
| `~p` | `:`=cofnij o jeden, `@`=y/ies | Liczba mnoga (`~p`→"s", `~@p`→"y"/"ies"). Argument jest liczbą całkowitą |
| `~c` | `:`=nazwa, `@`=składnia `#\` | Znak. Argument jest typu `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=znak | Stałoprzecinkowo. Argument jest liczbą |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=znak | Notacja wykładnicza. Argument jest liczbą. Parametry exponent-digits, scale i overflowchar z CL nie są obsługiwane (ich podanie jest błędem) |
| `~g` | `@`=znak | Ogólna zmiennoprzecinkowa. Argument jest liczbą. Nie przyjmuje parametrów |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Notacja pieniężna. Argument jest liczbą |

## 3. Wyjście (niezużywające argumentów)

| Dyrektywa | Znaczenie |
|---|---|
| `~%` | Nowa linia (`~n%` dla n sztuk) |
| `~&` | fresh-line (nowa linia, chyba że jesteśmy na początku linii; `~n&`) |
| `~\|` | Podział strony (form feed) |
| `~~` | Dosłowne `~` (`~n~` dla n sztuk) |
| `~t` | Tabulacja (`~colnum,colincT`. Jeśli już jesteśmy w kolumnie colnum lub dalej, przesuwa o wielokrotność colinc; nie przesuwa, jeśli colinc wynosi 0. `@`=względnie. `:`=tabulacja względem początku bloku logicznego, działająca tylko przy ozdobnym wypisywaniu) |
| `~_` | Warunkowa nowa linia (pretty; zwykła=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Wcięcie (pretty; `~ni`=początek bloku + n / `~n:i`=bieżąca kolumna + n) |
| `~<newline>` | Ignoruje nową linię (`:`=zachowaj białe znaki, `@`=zachowaj nową linię) |

Tak jak w CL, dyrektywy ozdobnego wypisywania (`~_` `~i` `~:t` `~<...~:>` oraz ścieżka ozdobnego wypisywania
`~a`/`~s`/`~w`) nic nie robią, gdy `*print-pretty*` jest fałszem. Domyślnie jest fałszem.

## 4. Struktury sterujące

| Dyrektywa | Znaczenie |
|---|---|
| `~(...~)` | Konwersja wielkości liter (`~(` małe litery, `~:(` każde słowo wielką literą, `~@(` tylko pierwsze słowo wielką literą, `~:@(` wszystkie wielkie litery) |
| `~[...~;...~]` | Wybór warunkowy (rozgałęzia według argumentu całkowitego. Z `~n[`, `~v[` lub `~#[` rozgałęzia według tej wartości i nie przyjmuje argumentu. `~:;`=klauzula domyślna, tylko jako ostatnia klauzula). `~:[false~;true~]` rozgałęzia według argumentu `bool` i ma dokładnie dwie klauzule |
| `~{...~}` | Iteracja (przechodzi po argumencie będącym listą. `~:{`=po podlistach, `~@{`=po pozostałych argumentach, `~:@{`=po każdej liście wśród pozostałych argumentów, `~^`=wyjście, `~:}`=wykonaj raz, nawet jeśli pusta). Ciało, które w jednej iteracji nie zużywa żadnego argumentu, jest błędem (nigdy by się nie skończyło) |
| `~<...~;...~>` | Justowanie (rozkłada segmenty na `~mincol` kolumn. `:`/`@`=dopełnienie na końcach) |
| `~<...~;...~:>` | **Blok logiczny** (zamykany za pomocą `~:>`; to coś innego niż powyższe justowanie). Pierwszy segment jest prefiksem, a ostatni sufiksem (oba tylko jako literały łańcuchowe). Z separatorem `~@;` prefiks jest **prefiksem dla każdej linii**. `~:<` ustawia domyślnie prefiks/sufiks na `(`/`)`. Argumentem jest jedna lista (`~@<` używa na miejscu pozostałych argumentów) |
| `~*` | Pomijanie argumentów (`~n*`=naprzód o n, `~:*`=wstecz, `~@*`=do pozycji bezwzględnej) |
| `~/name/` | Wywołanie metody (rozdział 5. Flagi `:`/`@` są przekazywane do metody. Nie przyjmuje parametrów) |

Następujące dyrektywy CL nie są obsługiwane (są błędami czasu sprawdzania).

- `~?` i `~@?`: przyjmują łańcuch sterujący jako argument z czasu działania, więc argumenty, które
  zużywają ich dyrektywy, nie mogą być sprawdzone. Zapisz te dyrektywy bezpośrednio w łańcuchu sterującym.
- `~@[...~]`: sprawdza, czy argument nie jest nil, ale ten język nie ma nil. Użyj `~:[false~;true~]`,
  które rozgałęzia się według `bool`.
- `~{~}` z pustym ciałem: bierze ciało z argumentu czasu działania. Zapisz dyrektywy wewnątrz
  nawiasów klamrowych.

## 5. `~/name/`

**Jedna różnica względem CL: nazwa jest wyszukiwana nie jako funkcja globalna, lecz jako metoda
własnego typu argumentu.** Metoda ma kształt `((self Self) (colon bool) (at bool)) → string`, a
`:`/`@` dyrektywy są przekazywane bez zmian.

Sposób z CL, polegający na wyszukiwaniu jako funkcji globalnej, nie może być bezpiecznie zaimplementowany w tym języku. Nawet
przy literałowym łańcuchu sterującym typ elementów argumentu będącego listą (wewnątrz `~{`) nie jest znany
w czasie sprawdzania, a wyszukiwanie funkcji po samej nazwie mogłoby wywołać funkcję przeznaczoną dla innego typu.
Wybór według typu wartości oznacza, że metoda jest sprawdzana pod względem typów dokładnie dla tego typu, co jest bezpieczne
(ten sam mechanizm co `print-object`). Działa to także dla wartości takich jak `string`/`bool`/`char`/
`symbol`/listy. Tylko dla liczb całkowitych, których szerokości nie da się poznać z wartości, jest to błąd, **gdy
więcej niż jeden typ całkowity definiuje metodę o tej nazwie**.

Nie wiadomo, do którego argumentu się odnosi, ale wiadomo, które metody może wywołać. Moduł sprawdzający zbiera
każde `~/name/` z literałowego łańcucha sterującego i zapisuje, wśród typów argumentów w tym miejscu
wywołania, te, które mają metodę o powyższym kształcie. Zatem **jeśli żaden z typów argumentów nie ma
tej metody, jest to błąd czasu sprawdzania** (nie czasu działania), i działa to także w plikach wykonywalnych AOT.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```
