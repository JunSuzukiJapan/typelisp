<!-- translated-from: docs/ja/guide/compile.md @ 00b2198ebe05054debbc1a0d1f9ebf1a9835d7a0 -->
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
  TLS, rozwiązywanie nazw), wątków systemowych (`Thread`), funkcji C (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  ani `ed`.
- WASI nie ma sposobu, by poznać strefę czasową, więc czas lokalny jest nieznany. `decode-universal-time` i `encode-universal-time` bez `zone` oraz `get-decoded-time` kończą się panic w czasie wykonania. Działają, gdy poda się `zone`.
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
- Czas lokalny (`decode-universal-time` bez `zone` itd.) jest wyznaczany ze strefy czasowej, którą podaje `Date` z JavaScriptu.
- Oprócz tego, czego nie można użyć w sekcji 3.6, operacje na plikach i katalogach, zmienne środowiskowe, `(command-line-args)`, katalog domowy i `dribble` są **błędem kompilacji**. Standardowe wejście jest czytane jako puste.

### 3.8 Kompilacja dla mikrokontrolera (Cortex-M)

Z `--target thumbv7em-none-eabihf` polecenie `typl -c` tworzy plik wykonywalny (ELF) dla płytki Cortex-M4F lub M7 bez systemu operacyjnego. Bez `-o` otrzymuje nazwę źródła z `.typl` zamienionym na `.elf`. Z `--target thumbv8m.main-none-eabihf` jest dla płytki z Cortex-M33.

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # tworzy hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # uruchom na Cortex-M4 w QEMU
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # uruchom na Cortex-M33 w QEMU
```

- `typl` zainstalowany przez `install.sh` działa od razu. Aby zainstalować przez `cargo install`, uruchom `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf`, a potem zainstaluj z `--features embedded` (`--features wasm,embedded` dla obu). Przy kompilacji debugowej `typl` w repozytorium uruchom `scripts/build-embedded-runtime.sh`. Linkowanie używa linkera z toolchaina rustup.
- Pamięć płytki podaj przez `--linker-script FILE`. Zapisuje się ją jak `memory.x` z `cortex-m-rt`: adresy i rozmiary FLASH i RAM. Wiersz w rodzaju `_typelisp_stack_size = 32K;` określa, ile RAM zajmuje stos (domyślnie 64KiB); reszta RAM to sterta. Bez tej opcji używany jest układ `mps2-an386` (`mps2-an505` dla Cortex-M33) z QEMU.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

- `--board-functions` wybiera, kto dostarcza wyjście, wejście, zakończenie i zegar (funkcje płytki).
  - `semihosting` (domyślnie): przez semihosting, do uruchamiania w QEMU (`-semihosting-config enable=on`) lub z debugerem.
  - `c`: płytka dostarcza je jako funkcje C. Przekaż przez `--link FILE` obiekt lub bibliotekę statyczną definiującą poniższe funkcje (`--link` można powtarzać; bibliotekę producenta wywoływaną przez `defffi` przekazuje się tak samo).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 to standardowe wyjście, 2 standardowe wyjście błędów
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0; przeczytane bajty, 0 na końcu
  void     typelisp_host_exit(int32_t code);                              // nie wraca
  uint64_t typelisp_host_now_us(void);                                    // mikrosekundy zegara, który nigdy się nie cofa
  int64_t  typelisp_host_unix_us(void);                                   // mikrosekundy od 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // czeka us mikrosekund
  ```

- Rejestry płytki (peryferia odwzorowane w pamięci) odczytuje się przez `volatile-read-u8`, `volatile-read-u16` i `volatile-read-u32`, a zapisuje przez `volatile-write-u8`, `volatile-write-u16` i `volatile-write-u32`. Adres jest typu `int`, a odczytana wartość ma typ danej szerokości (`u8` itd.). Każda z nich czyta lub pisze dokładnie raz, w miejscu, w którym jest zapisana; kompilator jej nie pomija ani nie przenosi. Wszystkich można używać tylko wewnątrz `unsafe`. Adres ujemny albo niebędący wielokrotnością szerokości to błąd czasu wykonania, zanim cokolwiek zostanie odczytane lub zapisane.

  ```lisp
  ;; UART 0 płytki mps2-an386 w QEMU: dane 0x40004000, stan 0x40004004, sterowanie 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; czekaj, dopóki wysyła
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; włącz wysyłanie
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" i znak nowej linii
  ```

- Funkcję wywoływaną, gdy nadchodzi przerwanie, pisze się jako `defun` z atrybutem `#[interrupt(N)]` (przerwanie peryferyjne `N`) lub takim jak `#[exception(SysTick)]`. Przerwanie włącza się przez `(enable-interrupt N)`. Procedura obsługi uruchamia się od razu; tylko gdy nadejdzie w trakcie pracy środowiska uruchomieniowego, uruchamia się po jej zakończeniu. Procedura obsługi nie może czekać (`sleep` to błąd kompilacji). Szczegóły są w sekcji 3.17 [dokumentacji składni](../reference/syntax.md).

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; samemu wywołać przerwanie 5 (rejestr oczekujących przerwań NVIC)
  ```

- Środowisko uruchomieniowe uruchamia program (tablica wektorów i przygotowanie RAM) i przydziela pamięć. HardFault lub nieobsłużone przerwanie kończy program kodem wyjścia panic (1).
- `throw`/`catch`/`panic`, bignumy, ułamki i liczby zmiennoprzecinkowe oraz zadania, kanały i `sleep` działają jak w pliku wykonywalnym. Zadania wykonują się po kolei na jedynym rdzeniu. Można używać funkcji C (`defffi`).
- Program, który używa czegoś, czego płytka nie ma, jest **błędem kompilacji**: oprócz tego, czego nie można użyć w sekcji 3.7, `eval` i `macroexpand`. Czas lokalny jest nieznany (jak w sekcji 3.6).
- Arena cons zaczyna od 1024 komórek i rośnie w razie potrzeby. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` i `--release` działają bez zmian.

### 3.9 Kompilacja dla Arduino Uno (AVR)

Z opcją `--target avr-atmega328p` powstaje plik wykonywalny (ELF) dla mikrokontrolera Arduino Uno (ATmega328P). Poniższy program miga diodą LED na płytce.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)                         ; PB5

(defun wait-a-bit () ()
  (let ((i (the u32 0)))
    (while (< i (the u32 400000))
      (unsafe (volatile-read-u8 PINB))
      (setf i (+ i 1)))
    ()))

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED))           ; ustaw jako wyjście
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; odwróć
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # tworzy blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # uruchom w QEMU
```

- Nie ma środowiska uruchomieniowego, więc program, który go używa, to **błąd kompilacji**. Błąd podaje, co jest potrzebne i przez które funkcje się do tego dochodzi. Można używać: obliczeń na liczbach całkowitych o stałej szerokości (od `i8` do `u32`), `int`, `f32`, `f64`, `bool` i `char`, wywołań funkcji (także rekurencyjnych), warunków, pętli, `defvar` i `defconstant` z takimi wartościami, `volatile-read-u8` i podobnych oraz wywołań funkcji C.
- Nie można używać: wartości na stercie, takich jak napisy, listy i struktury; `catch` i wywołań wartości funkcyjnych; wywołań funkcji C, które przyjmują napis lub funkcję; funkcji matematycznych na liczbach zmiennoprzecinkowych, takich jak `sin` i `expt` (`sqrt` i zaokrąglenia, takie jak `floor`, są dostępne).
- Procedurę obsługi przerwania zapisuje się jako `#[interrupt(N)]` ([Składnia](../reference/syntax.md), rozdział 3.17). `N` to numer przerwania ATmega328P, od 1 do 25, licząc reset jako 0 (przepełnienie licznika 1 to 13). Zobacz przykład poniżej.
- Dzielenie przez 0 zatrzymuje mikrokontroler w tym miejscu (nie nadchodzą też już przerwania). Tak samo `/` najmniejszej wartości ze znakiem przez `-1`.
- `int` mieści się od -16384 do 16383. Gdy wynik obliczenia albo wartość zamieniona na `int` wykracza poza ten zakres, mikrokontroler się zatrzymuje. Liczba zmiennoprzecinkowa jest zamieniana na całkowitą przez `int`, więc ten zakres dotyczy też takiej zamiany.
- Gdy rekurencji zabraknie stosu, mikrokontroler się zatrzymuje.
- Funkcję C deklaruje się przez `defffi`, a plik obiektowy skompilowany dla AVR podaje się przez `--link FILE`.
- Pętla, która nic nie robi, znika od `-O1` wzwyż. W pętli oczekującej należy na przykład odczytywać rejestr.
- Gdy `main` się kończy, mikrokontroler się zatrzymuje.
- Układ pamięci zmienia `--linker-script FILE`. Bez tej opcji jest to 32 KiB pamięci flash bez ostatnich 512 bajtów programu rozruchowego oraz 2 KiB RAM. Adresy RAM zapisuje się z dodanym `0x800000`, na przykład `0x800100`.
- Obliczenia na `f32` i `f64` wykonują funkcje pomocnicze. Te, których się używa, trafiają do pamięci flash (te dla `f64` są większe).
- Do konsolidacji używany jest linker z zestawu narzędzi rustup, jak w punkcie 3.8.

Przykład, który miga diodą LED za pomocą przerwania licznika. Przerwanie włącza się w rejestrze danego urządzenia.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; przepełnienie licznika 1
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; odwróć
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; włącz przerwanie przepełnienia
          (volatile-write-u8 TCCR1B 3))           ; licz z 1/64 zegara
  (while true))
```

- Program z procedurą obsługi dopuszcza przerwania, gdy tylko zmienne globalne zostaną zainicjowane. Urządzenia należy skonfigurować na początku `main`.
- Nie można używać `enable-interrupt` ani `#[exception(NAME)]` (błąd kompilacji).
- Gdy działa procedura obsługi, inne przerwania czekają. Jak każda inna funkcja, procedura obsługi nie może używać środowiska uruchomieniowego.
- Pojedynczego odczytu ani zapisu zmiennej globalnej procedura obsługi nie przerywa, nawet dla wartości dwubajtowych i większych. W tym celu przerwania są wyłączane przy odczycie zmiennej, którą zapisuje procedura obsługi, i przy zapisie zmiennej, którą procedura obsługi odczytuje lub zapisuje, a zmienną wskazuje ostrzeżenie podczas kompilacji. Między odczytem a zapisem przy aktualizacji (`(setf n (+ n 1))`) przerwanie może nadejść. Aby zaktualizować zmienną, którą zapisuje też procedura obsługi, należy odczytać i zachować rejestr stanu (adres `#x5F`), zapisać do niego 0, by zatrzymać przerwania, a potem zapisać z powrotem zachowaną wartość.
- Przerwanie bez procedury obsługi zatrzymuje mikrokontroler.

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
