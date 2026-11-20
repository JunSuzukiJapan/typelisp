<!-- translated-from: docs/ja/guide/compile.md @ eb0e7d31e1d6e61f64d90fcfb4f0dc5e6bfe813f -->
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
(use std::env::command-line-args)
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
args: #(./hello a b)
fib(25) = 75025
```

Bez `-o` plik wykonywalny dostaje nazwę pliku źródłowego bez `.typl` i trafia do
tego samego katalogu co plik źródłowy. Jeśli nazwa pliku źródłowego nie kończy się na `.typl`, `-o` jest
wymagane. Przy `-c` (`--compile`) nie można podawać `--image` ani `--feature`.

Z `--release` `typl` tworzy **kompilację release**, która usuwa `eprint`/`eprintln` (wyjście na standardowe wyjście błędów służące badaniu problemów; [Wypisywanie](../reference/functions/printing.md)) razem z ich argumentami. To, co usuwa, i tak jest sprawdzane pod względem typów, więc błąd typu w tym miejscu powoduje niepowodzenie kompilacji. `--release` to opcja całego `typl`: działa tak samo nie tylko przy `-c`, ale też przy uruchamianiu pliku, w REPL i w `typl test`. `compile-file` wywołane w `typl` uruchomionym z `--release` również tworzy kompilację release, a `eprint`/`eprintln` są usuwane także z kodu wykonywanego przez `eval` w tak utworzonym pliku wykonywalnym.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # to samo, gdy zapisane po -c
```

`-O0`, `-O1`, `-O2` i `-O3` ustawiają **poziom optymalizacji**. Domyślny jest `-O0`, który nie optymalizuje. Od `-O1` kompilator najpierw wykonuje proste optymalizacje, takie jak zastąpienie zmiennej związanej tylko z literałem jej wartością czy obliczenie z góry arytmetyki całkowitej na stałych, a resztę pozostawia optymalizacji LLVM na poziomie o tej samej nazwie. Na każdym poziomie wyniki obliczeń są takie same. Podobnie jak `--release` jest to opcja całego `typl`: oprócz pliku wykonywalnego tworzonego przez `-c` działa na kod kompilowany podczas uruchamiania pliku, w REPL i w `typl test`, a także na wywołane tam `compile-file`. Kod wykonywany przez interpreter nie jest optymalizowany.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # to samo, gdy zapisane po -c
```

To samo można zrobić, wywołując `compile-file` z REPL lub z programu.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
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

- Najwyższy poziom pliku wejściowego może zawierać tylko definicje (`defun` `defvar`
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

### 3.5 Zasoby używane przez plik wykonywalny

`typl -c` ma opcje określające, jakich zasobów plik wykonywalny może używać podczas działania.

```sh
$ typl --heap-cells 100000 -c hello.typl    # ustala arenę cons na 100000 komórek
$ typl -c hello.typl --max-memory 64M       # przydziela najwyżej 64 MiB
$ typl -c hello.typl --no-cons              # nie używa komórek cons
```

- `--heap-cells N` ustala pojemność areny komórek cons na `N` komórek. Arena ponad to nie rośnie: jeśli odśmiecanie nie zwolni żadnej komórki, program zatrzymuje się z błędem wykonania. Bez tej opcji arena zaczyna od 65536 komórek i rośnie w miarę potrzeby.
- `--max-memory SIZE` ogranicza całkowitą pamięć przydzielaną przez plik wykonywalny. Liczy się wszystko: arena cons, napisy, wartości takie jak wektory i tablice haszujące oraz pamięć samego środowiska uruchomieniowego. `SIZE` to liczba bajtów, po której może stać `K`, `M` lub `G` (1024, 1024² i 1024³ razy). Przydział, który przekroczyłby granicę, jest zgłaszany, a program kończy się z kodem wyjścia 1. Tego zakończenia nie da się przechwycić przez `catch`. Bez tej opcji nie ma granicy.
- `--no-cons` tworzy plik wykonywalny, który nie używa komórek cons. Nie ma areny cons, więc program robiący coś, co wymaga komórek cons, jest **błędem kompilacji**, który pokazuje tę operację i łańcuch funkcji od `main`, który do niej prowadzi. Komórki cons są potrzebne do zbudowania listy `Sexpr` (także listy cytowanej i zbudowanej przez quasi-cytowanie) lub argumentu `&rest`, a także przez `read`, `eval` i `macroexpand`. Operacje na liczbach, napisach, wektorach, tablicach haszujących, strukturach, kanałach itp. oraz `print`/`println`/`format` ich nie używają. Nie można podać razem z `--heap-cells`; pisze się po `-c`.

`--max-memory` pisze się po `-c`. `--heap-cells` można napisać przed `-c` lub po nim i jest wtedy pojemnością areny cons pliku wykonywalnego. Arena cons wlicza się do granicy pamięci, więc przy granicy, w której arena się nie mieści, program zatrzymuje się, zanim się zacznie.

### 3.6 Kompilowanie do WebAssembly

Z `--target wasm32-wasip1` polecenie `typl -c` tworzy moduł WebAssembly (polecenie WASI preview 1) zamiast pliku wykonywalnego. Bez `-o` ma on nazwę źródła z `.typl` zamienionym na `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # tworzy hello.wasm
$ wasmtime hello.wasm a b                        # uruchomienie w środowisku obsługującym WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- `typl` zainstalowany przez `install.sh` działa od razu. Instalacja przez `cargo install` wymaga przygotowania (obejmującego też podrozdział 3.7). Najpierw uruchom `rustup target add wasm32-wasip1 wasm32-unknown-unknown` przez rustup. Następnie zainstaluj go z `--features wasm` (`cargo install --locked --path . --features wasm`).
  `typl` zainstalowany bez tego nie ma środowiska uruchomieniowego WebAssembly i `--target` kończy się błędem. Dla wersji debug `typl` w repozytorium uruchom zamiast tego `scripts/build-wasm-runtime.sh` w repozytorium.
- Do linkowania używany jest linker z toolchaina rustup (`rust-lld`), więc rustup jest potrzebny także podczas kompilacji.
- Moduł zachowuje się jak plik wykonywalny: standardowe wyjście, standardowe wyjście błędów i kod wyjścia, `(command-line-args)`,
  odczyt i zapis plików, `throw`/`catch`/`panic`, zadania, kanały i `sleep` oraz `eval` działają.
  Zadania wykonują się po kolei w jednym wątku. Pliki można czytać i zapisywać tylko w katalogach,
  które środowisko udostępnia modułowi (w wasmtime podaje się je przez `--dir`).
- Program używający czegoś, czego WebAssembly nie ma, to **błąd kompilacji**. Błąd pokazuje, czego użyto,
  i łańcuch funkcji od `main`, który do tego prowadzi. Nie można używać: gniazd (TCP, UDP, domena Unix,
  TLS, rozwiązywanie nazw), wątków systemowych (`Thread`), funkcji C (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`,
  lokalnej strefy czasowej ani `ed`.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` i `--release` działają tak samo jak dla pliku wykonywalnego.

### 3.7 Uruchamianie w przeglądarce lub w Node.js

Z `--target wasm32-unknown-unknown` polecenie `typl -c` tworzy moduł WebAssembly ładowany i uruchamiany z JavaScriptu. Obok modułu (`hello.wasm`), w tym samym folderze, powstaje JavaScript, który go ładuje i uruchamia (`hello.mjs`). Przygotowanie jest takie samo jak w podrozdziale 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # tworzy hello.wasm i hello.mjs
$ node hello.mjs                                       # uruchomienie w Node.js
args: #()
fib(25) = 75025
```

Z innego kodu JavaScript wywołaj `run` z `hello.mjs`. `write` otrzymuje wyjście jako tekst (`fd` to 1 dla standardowego wyjścia i 2 dla standardowego wyjścia błędów). Wartością `run` jest kod wyjścia.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* na przykład pokaż na stronie */ } });
```

- Bez `write` Node.js pisze na standardowe wyjście i standardowe wyjście błędów, a przeglądarka do konsoli, wiersz po wierszu.
- W przeglądarce udostępnij `hello.wasm` przez HTTP z tego samego miejsca co `hello.mjs` (nie da się go załadować z `file://`).
- `sleep` czeka przez `Atomics.wait`. Przeglądarka pozwala na to tylko w Workerze strony z izolacją cross-origin, więc program używający `sleep` uruchamiaj w Workerze.
- Oprócz tego, czego nie można używać w podrozdziale 3.6, operacje na plikach i katalogach są **błędem kompilacji**. Standardowe wejście czyta się jako puste, a `(command-line-args)` jest pustym wektorem.

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
