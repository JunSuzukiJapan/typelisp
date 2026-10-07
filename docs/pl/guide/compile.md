<!-- translated-from: docs/ja/guide/compile.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Kompilacja

O ile nie zrobisz nic więcej, programy typelisp działają w interpreterze. Ponadto istnieją dwa
sposoby kompilacji do kodu natywnego i jeden sposób zapisania środowiska. Szczegóły
specyfikacji znajdują się w [Referencji składni, rozdział 10](../reference/syntax.md#10-kompilacja).

| Metoda | Jak | Rezultat |
|---|---|---|
| Kompilacja JIT | `(compile name)` | Funkcja w działającej sesji zostaje zastąpiona kodem natywnym |
| Kompilacja AOT | `typl -c src.typl` lub `(compile-file "src.typl" "out")` | Samodzielny plik wykonywalny |
| Zrzut (dump) | `(dump "file.typld")` | Zapisuje definicje; `typl --image` startuje ponownie z tego samego środowiska |

## 1. Przygotowanie

Kompilacja używa LLVM 22. Jeśli zbudowałeś `typl` zgodnie z [README.md](../../../README.md), nie
jest potrzebne żadne dalsze przygotowanie.

Pliki wykonywalne tworzone przez kompilację AOT są linkowane z biblioteką statyczną `libtypelisp_front.a`. Wydaniowa
(release) wersja `typl` (także zainstalowana za pomocą `cargo install`) nosi tę bibliotekę w sobie, więc żadne przygotowanie nie jest potrzebne. Przy pierwszej kompilacji zapisuje bibliotekę do
`~/.typelisp/lib/<identyfikator kompilacji>/` (lub do `$TYPELISP_HOME/lib/<identyfikator kompilacji>/`, jeśli ustawiono zmienną środowiskową
`TYPELISP_HOME`) i od tej pory używa tej kopii. `typl --remove-lib` ją usuwa (z
`--others` te zapisane przez inne wersje `typl`; z `--all` wszystkie). Wersja debugowa
`typl` używa biblioteki z `target/debug/` repozytorium, w którym została zbudowana. Aby użyć biblioteki umieszczonej
gdzie indziej, podaj jej katalog za pomocą `--lib-dir` przy uruchamianiu `typl` (sekcja 3.2).
W macOS linkowanie korzysta z Xcode Command Line Tools.

## 2. Kompilacja JIT

Zamienia już zdefiniowaną funkcję na kod natywny na miejscu.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; od tej chwili wywołania uruchamiają skompilowany kod
```

- `name` nie jest obliczane. Zapisz nazwę funkcji tak, jak jest (nie jako łańcuch znaków). Dla metody zapisz ją
  z nazwą typu, jak w `(compile point::norm)`.
- Funkcje, które wywołuje, są kompilowane razem z nią.
- **Funkcji generycznych nie można kompilować.** Kopia dla każdego typu jest tworzona w każdym miejscu użycia.
  Zamiast tego skompiluj funkcję, która je wywołuje, z konkretnymi typami.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` i `dump` to operacje interpretera, więc
  funkcji, która je wywołuje, nie można skompilować. Próba jej skompilowania daje błąd z podaniem przyczyny.

Aby obejrzeć wynik kompilacji, użyj `disassemble`.

```lisp
(disassemble fib)          ; kod maszynowy hosta
(disassemble fib true)     ; LLVM IR
```

## 3. Budowanie pliku wykonywalnego za pomocą kompilacji AOT

### 3.1 Pisanie programu

Jako punkt wejścia zdefiniuj **funkcję `main` bez argumentów**.

```lisp
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

Końcowe `(main)` w pliku jest po to, aby `main` było wywoływane, gdy uruchamiasz
`typl hello.typl`. `compile-file` pomija to końcowe `(main)`, więc ten sam plik działa zarówno w
interpreterze, jak i z kompilacją AOT.

### 3.2 Kompilowanie

Z wiersza poleceń użyj `typl -c` (`typl --compile` jest tym samym).

```sh
$ typl -c hello.typl            # tworzy hello
$ typl -c hello.typl -o fib     # nazywa plik wykonywalny fib
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Bez `-o` plik wykonywalny dostaje nazwę pliku źródłowego bez `.typl` i trafia do
tego samego katalogu co plik źródłowy. Jeśli nazwa pliku źródłowego nie kończy się na `.typl`, `-o` jest
wymagane. Przy `-c` (`--compile`) nie można podawać `--image`, `--heap-cells` ani `--feature`.

To samo można zrobić, wywołując `compile-file` z REPL lub z programu.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Jeśli budujesz wielokrotnie, możesz umieścić tę jedną linię w pliku i uruchamiać ją poleceniem `typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Nazwy plików są rozwiązywane względem **bieżącego katalogu, w którym uruchomiono `typl`**, a nie względem
położenia `build.typl`.

Aby zlinkować `libtypelisp_front.a` umieszczone poza miejscem, w którym szuka `typl`, podaj jego katalog za pomocą
`--lib-dir`. Dotyczy to zarówno `typl -c`, jak i `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Jeśli podany katalog nie zawiera `libtypelisp_front.a`, `typl` zatrzymuje się z błędem. Plik działa tylko
z `typl` zbudowanym razem z nim. Po przebudowaniu `typl` skopiuj go ponownie.

### 3.3 Co może zawierać plik kompilowany AOT

- Najwyższy poziom pliku wejściowego może zawierać tylko definicje (`defun` `defmethod` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) oraz `use` `module`. Wyrażenia najwyższego poziomu, takie jak
  `(println ...)`, są niedozwolone, z wyjątkiem końcowego `(main)`. Umieść pracę wewnątrz `main`.
- Bez `main` bez argumentów kompilacja kończy się błędem.
- Pliki modułów wprowadzonych przez `use` także są kompilowane i łączone w jeden plik wykonywalny.
- Biblioteki wskazane za pomocą `:library` w `defffi` są linkowane automatycznie ([FFI do C](ffi.md)).
- Każdej funkcji biblioteki standardowej można używać z kompilacją AOT. Można też używać `eval`, ale wtedy
  sprawdzanie typów i interpreter trafiają do pliku wykonywalnego, co czyni go większym i wolniej startującym.
  Programy, które nie wywołują `eval`, ich nie zawierają.

### 3.4 Jak zachowuje się plik wykonywalny

- `(command-line-args)` zwraca `Vector<string>` o tym samym kształcie, niezależnie od tego, czy uruchomiono
  `typl hello.typl a b`, czy `./hello a b`. Pierwszym elementem jest nazwa programu.
- Kod wyjścia ustawia się za pomocą `(exit n)`. Jeśli `main` zakończy się normalnie, wynosi 0.
- Przy `panic` program wypisuje komunikat i kończy działanie z niezerowym kodem.

## 4. Zrzuty (dumps)

Definicje bieżącej sesji możesz zapisać do jednego pliku i następnym razem od niego zacząć.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

Działa to także przy uruchamianiu pliku, jak w `typl --image session.typld prog.typl`.

- Zapisywane są **definicje**. Wyrażenia obliczone w sesji nie są zapisywane.
- Funkcje, które `compile`-owałeś, są zapisywane w postaci skompilowanej.
- Zmienne globalne są przywracane przez **ponowne uruchomienie ich inicjalizatorów**, a nie z wartościami, które miały,
  gdy zapisywano zrzut.
- Zrzutu nie można wczytać za pomocą `typl` w innej wersji niż ta, która go zapisała (jest to
  błąd).

Jeśli uruchomisz plik i zrobisz z niego `(dump ...)`, definicje tego pliku znajdą się w module nazwanym od
pliku. Funkcja zdefiniowana w `dp.typl` nazywa się `dp::sq`, a wywołanie jej z innego pliku wymaga
`pub` ([Moduły i układ plików](modules.md)).

## 5. O skompilowanych plikach modułów

Nie istnieje format, taki jak `.fasl` w Common Lisp, do zapisywania skompilowanego wyniku każdego modułu
do pliku. `compile-file` buduje plik wykonywalny bezpośrednio ze źródeł. Nie zostają żadne pliki pośrednie.
