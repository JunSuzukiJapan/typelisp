<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# REPL

Uruchomienie `typl` bez argumentów otwiera REPL (tryb interaktywny). Ten dokument opisuje, co
REPL potrafi oprócz obliczania wyrażeń: jego polecenia, pomoc przy pisaniu, odwołania do
poprzednich wyników, przerywanie obliczeń i badanie po błędzie.

## Uruchamianie i wychodzenie

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Wychodzi się przez `:quit` (w skrócie `:q`) lub `Ctrl+D`. Historia wpisanych wierszy jest
zapisywana w `~/.typl_history`, więc przy następnym uruchomieniu można ją przywołać przez
`Ctrl+P` / `Ctrl+N` albo `Ctrl+R` (wyszukiwanie w historii).

Przy uruchomieniu, przed pierwszym znakiem zachęty, wykonywane jest po kolei to, co poniżej. Żaden
z tych kroków nie wypisuje wartości, które daje.

1. Zawartość `~/.typlrc`, jeśli ten plik istnieje. Z `--no-init` nie jest czytany.
2. `--eval FORM` i `--load FILE` z wiersza poleceń, w kolejności zapisu. Oba można podać dowolnie
   wiele razy.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` i `--no-init` służą tylko do uruchamiania REPL. Podanie ich przy uruchamianiu
pliku jest błędem.

REPL zaczyna w module `user`. Po przejściu do innego modułu przez `(in-module foo)` znak zachęty
pokazuje ten moduł.

```
typl> (in-module foo)
typl user::foo>
```

## Polecenia REPL

`:nazwa` zapisana na początku wiersza jest poleceniem REPL. `:help` wypisuje ich listę. `:nazwa`,
której nie ma na liście, nie jest poleceniem; jest obliczana jako zwykłe słowo kluczowe.

| Polecenie | Znaczenie |
|---|---|
| `:help` (`:h`, `:?`) | wypisać polecenia |
| `:type FORM` (`:t`) | wypisać typ FORM bez obliczania go |
| `:describe NAME` (`:d`) | wypisać rodzaj definicji, jej typ, pola lub warianty, metody, implementowane traity, miejsce definicji i napis dokumentacyjny |
| `:apropos TEXT` | znaleźć we wszystkich modułach definicje, których nazwa zawiera TEXT (bez rozróżniania wielkości liter) |
| `:src NAME` | wypisać kod źródłowy definicji |
| `:who-calls NAME` | wypisać funkcje i metody, które wywołują funkcję lub metodę NAME |
| `:inspect FORM` (`:i`) | obliczyć FORM i otworzyć jego wartość według typu |
| `:backtrace` (`:bt`) | wypisać wywołania, które trwały, gdy poprzedni wiersz zakończył się błędem, od najbardziej wewnętrznego |
| `:locals [N]` | wypisać argumenty wywołania N z `:backtrace` (wywołania 0, jeśli pominięto N) |
| `:cd [DIR]` | zmienić bieżący folder na DIR (na folder domowy, jeśli pominięto DIR) |
| `:pwd` | wypisać bieżący folder |
| `:quit` (`:exit`, `:q`) | wyjść z REPL |

NAME może być nazwą, np. `inner`, albo ścieżką, np. `std::seq::reverse` lub `Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Dodaje 1 do x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Dodaje 1 do x.
typl> :src inner
(defun inner ((x int) (s string)) int "Dodaje 1 do x." (+ x 1))
```

`:src` pokazuje definicję wczytaną z pliku na podstawie tego pliku, a definicję wpisaną w REPL na
podstawie wpisanego tekstu. Dla definicji z biblioteki standardowej nie jest zapisane ich miejsce w
kodzie źródłowym, więc nie da się go pokazać.

`:who-calls` przeszukuje ciała funkcji z bieżącej sesji. Dla funkcji generycznej znajduje tylko
użycia skonkretyzowane dla konkretnych typów.

`:cd` zmienia folder, w którym szuka `(load ...)`, ale `use` dalej szuka plików w folderze, w
którym uruchomiono REPL.

## Otwieranie wartości (`:inspect`)

`:inspect` otwiera wartość według jej typu. Struktura otwiera się na swoje pola, enum (także
`Option`) na pola swojego wariantu, `Vector` na swoje elementy, a `HashTable` na swoje klucze i
wartości. Liczba wchodzi do danej części, `u` cofa o poziom, a `q` kończy.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

Listy i `Sexpr` nie są otwierane; są wypisywane bez zmian.

## Pomoc przy pisaniu

Przy pisaniu w terminalu dostępne jest to, co poniżej.

- `Tab` uzupełnia nazwy (definicje i składnię, np. `if` i `defun`), ścieżki z `::`, polecenia REPL
  na początku wiersza, a wewnątrz napisu nazwy plików.
- Po nazwie funkcji i spacji na końcu wiersza pojawia się przygaszony typ tej funkcji.
- Nawias pasujący do nawiasu obok kursora jest wyróżniany.
- Jeśli naciśnie się Enter przed końcem formy, następny wiersz zaczyna się z wcięciem zależnym od
  liczby otwartych nawiasów.

## Poprzednie wyniki (`:*`, `:**`, `:***`)

W REPL `:*` to ostatnio wypisana wartość, `:**` poprzednia, a `:***` jeszcze wcześniejsza. Każda ma
typ swojej wartości.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- Wpis, który nie wypisał żadnej wartości (np. definicja), albo zakończył się błędem, nie przesuwa
  wyników.
- Napisanie `:**` lub `:***`, zanim pojawi się tyle wyników, jest błędem.
- Zacytowane `':*` jest zwykłym słowem kluczowym.
- `:*` zapisane w ciele funkcji nadal wskazuje wynik z chwili, gdy funkcję zdefiniowano. `*` w
  Common Lisp odczytuje wartość w chwili wywołania, ale typelisp ustala ją przy definicji, żeby
  miała typ.
- Poza REPL (przy uruchamianiu pliku oraz w `~/.typlrc`, `--load` i `--eval`) `:*` jest zwykłym
  słowem kluczowym.

## Przerywanie obliczeń (`Ctrl+C`)

Naciśnięcie `Ctrl+C` w trakcie obliczania przerywa to obliczenie i wraca do znaku zachęty. To, co
zdefiniowano wcześniej, zostaje. Działa to zarówno dla pętli w interpreterze, pętli w
skompilowanym kodzie, jak i oczekiwania w `sleep` lub `wait`. Sprzątanie trwających
`unwind-protect` jest wykonywane.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` naciśnięte w trakcie pisania tylko porzuca wpisywany wiersz.

## Badanie po błędzie (`:backtrace`, `:locals`)

Gdy obliczenie zatrzyma się na błędzie, `:backtrace` (`:bt`) wypisuje wywołania funkcji, które
wtedy trwały, od najbardziej wewnętrznego. `:locals N` wypisuje argumenty wywołania N według ich
zadeklarowanych typów.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- Przy wywołaniu z ostatniego wyrażenia funkcji (wywołaniu ogonowym) funkcja wywołująca nie jest
  pokazywana. Dzięki temu pętla z rekurencją ogonową zajmuje miejsce jednego wywołania, niezależnie
  od liczby obrotów.
- Wywołania wewnątrz skompilowanych funkcji i funkcje anonimowe (`lambda`) nie są pokazywane.
- `:locals` pokazuje tylko argumenty, a nie zmienne związane przez `let`.
- Obliczenie następnego wiersza usuwa zapis poprzedniego błędu.
