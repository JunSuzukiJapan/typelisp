<!-- translated-from: docs/ja/guide/compile.md @ 9d79920895a73472f673dcf2ab2711b73f666967 -->
# Kompilierung

Ohne weiteres Zutun laufen typelisp-Programme im Interpreter. Darüber hinaus gibt es zwei Wege, in nativen
Code zu kompilieren, und einen Weg, eine Umgebung zu speichern. Die Einzelheiten der Spezifikation stehen in
[Kapitel 10 der Syntaxreferenz](../reference/syntax.md#10-kompilierung).

| Verfahren | Wie | Ergebnis |
|---|---|---|
| JIT-Kompilierung | `(compile name)` | Eine Funktion der laufenden Sitzung wird durch nativen Code ersetzt |
| AOT-Kompilierung | `typl -c src.typl` oder `(compile-file "src.typl" "out")` | Ein eigenständiges ausführbares Programm |
| Dump | `(dump "file.typld")` | Speichert die Definitionen; `typl --image` startet wieder mit derselben Umgebung |

## 1. Vorbereitung

Die Kompilierung verwendet LLVM 22. Wer `typl` gemäß der [README.md](../../../README.md) gebaut hat, braucht
keine weitere Vorbereitung.

Programme, die durch AOT-Kompilierung entstehen, werden gegen die statische Bibliothek `libtypelisp_front.a`
gelinkt. Ein Release-Build von `typl` (auch einer, der mit `cargo install` installiert wurde) trägt diese
Bibliothek in sich, daher ist keine Vorbereitung nötig. Beim ersten Kompilieren schreibt er die Bibliothek nach
`~/.typelisp/lib/<Build-ID>/` (oder nach `$TYPELISP_HOME/lib/<Build-ID>/`, wenn die Umgebungsvariable
`TYPELISP_HOME` gesetzt ist) und verwendet von da an diese Kopie. `typl --remove-lib` löscht sie (mit
`--others` die von anderen Versionen von `typl` geschriebenen, mit `--all` alle). Ein Debug-Build von `typl`
verwendet die Bibliothek in `target/debug/` des Repositorys, in dem er gebaut wurde. Um eine an anderer Stelle
abgelegte zu verwenden, gibt man beim Start von `typl` deren Ordner mit `--lib-dir` an (Abschnitt 3.2).
Unter macOS verwendet das Linken die Xcode Command Line Tools.

## 2. JIT-Kompilierung

Wandelt eine bereits definierte Funktion an Ort und Stelle in nativen Code um.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; ab hier führen Aufrufe den kompilierten Code aus
```

- `name` wird nicht ausgewertet. Man schreibt den Funktionsnamen so, wie er ist (nicht als Zeichenkette). Eine
  Methode schreibt man mit dem Typnamen, wie in `(compile point::norm)`.
- Funktionen, die sie aufruft, werden mitkompiliert.
- **Generische Funktionen lassen sich nicht kompilieren.** An jeder Stelle, an der sie verwendet werden, wird
  eine Kopie für jeden Typ erzeugt. Stattdessen kompiliert man die Funktion, die sie mit konkreten Typen
  aufruft.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` und `dump` sind Operationen des Interpreters, daher
  lässt sich eine Funktion, die sie aufruft, nicht kompilieren. Der Versuch ergibt einen Fehler, der den Grund
  nennt.

Um das Ergebnis der Kompilierung anzusehen, verwendet man `disassemble`.

```lisp
(disassemble fib)          ; der Maschinencode des Hosts
(disassemble fib true)     ; LLVM IR
```

## 3. Ein Programm mit AOT-Kompilierung erstellen

### 3.1 Das Programm schreiben

Als Einstiegspunkt definiert man eine **Funktion `main` ohne Argumente**.

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

Das `(main)` am Ende der Datei sorgt dafür, dass `main` aufgerufen wird, wenn man `typl hello.typl`
ausführt. `compile-file` überspringt dieses abschließende `(main)`, daher funktioniert dieselbe Datei sowohl
im Interpreter als auch mit der AOT-Kompilierung.

### 3.2 Kompilieren

Auf der Kommandozeile verwendet man `typl -c` (`typl --compile` ist dasselbe).

```sh
$ typl -c hello.typl            # erzeugt hello
$ typl -c hello.typl -o fib     # nennt das Programm fib
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Ohne `-o` erhält das Programm den Namen der Quelldatei ohne `.typl` und landet im selben Ordner wie die
Quelldatei. Endet der Name der Quelldatei nicht auf `.typl`, ist `-o` Pflicht. Zusammen mit `-c` (`--compile`)
lassen sich `--image` und `--feature` nicht angeben.

Mit `--release` erzeugt `typl` einen **Release-Build**, der `eprint`/`eprintln` (Ausgaben zur Untersuchung auf der Standardfehlerausgabe; [Ausgabe](../reference/functions/printing.md)) samt Argumenten entfernt. Was entfernt wird, wird trotzdem typgeprüft; ein Typfehler darin lässt die Kompilierung also fehlschlagen. `--release` ist eine Option von `typl` als Ganzem: Sie wirkt nicht nur bei `-c`, sondern ebenso beim Ausführen einer Datei, in der REPL und bei `typl test`. Auch ein `compile-file`, das in einem mit `--release` gestarteten `typl` aufgerufen wird, erzeugt einen Release-Build, und auch aus Code, der in einer so erzeugten ausführbaren Datei mit `eval` ausgeführt wird, werden `eprint`/`eprintln` entfernt.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # nach -c geschrieben ist es dasselbe
```

`-O0`, `-O1`, `-O2` und `-O3` legen die **Optimierungsstufe** fest. Voreingestellt ist `-O0`, das nicht optimiert. Ab `-O1` führt der Compiler zuerst einfache Optimierungen durch, etwa eine Variable, die nur an ein Literal gebunden ist, durch ihren Wert zu ersetzen oder Ganzzahlarithmetik auf Konstanten im Voraus zu berechnen, und überlässt den Rest der Optimierung von LLVM auf der gleichnamigen Stufe. Auf jeder Stufe sind die Ergebnisse der Berechnung dieselben. Wie `--release` ist es eine Option von `typl` als Ganzem: Außer für die ausführbare Datei, die `-c` erzeugt, wirkt sie auf den Code, der beim Ausführen einer Datei, in der REPL und in `typl test` kompiliert wird, sowie auf ein dort aufgerufenes `compile-file`. Code, den der Interpreter ausführt, wird nicht optimiert.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # nach -c geschrieben ist es dasselbe
```

Dasselbe lässt sich erreichen, indem man `compile-file` aus der REPL oder aus einem Programm aufruft.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Wer häufig baut, kann diese eine Zeile in eine Datei schreiben und sie mit `typl build.typl` ausführen.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Dateinamen werden relativ zum **aktuellen Verzeichnis, in dem `typl` gestartet wurde**, aufgelöst, nicht
relativ zum Ort von `build.typl`.

Um eine `libtypelisp_front.a` zu linken, die an einer anderen Stelle liegt als dort, wo `typl` sucht, gibt man
deren Ordner mit `--lib-dir` an. Das gilt sowohl für `typl -c` als auch für `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Enthält der angegebene Ordner keine `libtypelisp_front.a`, hält `typl` mit einem Fehler an. Die Datei
funktioniert nur mit dem `typl`, das zusammen mit ihr gebaut wurde. Nach einem Neubau von `typl` kopiert man
sie erneut.

### 3.3 Was eine AOT-kompilierte Datei enthalten darf

- Die oberste Ebene der Einstiegsdatei darf nur Definitionen (`defun` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) sowie `use` `module` enthalten. Ausdrücke auf oberster
  Ebene wie `(println ...)` sind nicht erlaubt, abgesehen vom abschließenden `(main)`. Die Arbeit gehört
  in `main`.
- Ohne ein `main` ohne Argumente schlägt die Kompilierung mit einem Fehler fehl.
- Die Dateien der mit `use` hereingeholten Module werden ebenfalls kompiliert und zu einem einzigen Programm
  zusammengefügt.
- Bibliotheken, die in `defffi` mit `:library` angegeben sind, werden automatisch gelinkt ([C-FFI](ffi.md)).
- Jede Funktion der Standardbibliothek lässt sich mit der AOT-Kompilierung verwenden. Auch `eval` ist
  verwendbar, dann kommen aber Typprüfung und Interpreter ins Programm, was es größer und langsamer beim Start
  macht. Programme, die `eval` nicht aufrufen, enthalten sie nicht.

### 3.4 Wie sich das Programm verhält

- `(command-line-args)` gibt einen `Vector<string>` derselben Gestalt zurück, egal ob als
  `typl hello.typl a b` oder als `./hello a b` ausgeführt. Das erste Element ist der Programmname.
- Den Exit-Code setzt man mit `(exit n)`. Kehrt `main` normal zurück, ist er 0.
- Bei einem `panic` gibt das Programm die Meldung aus und endet mit einem Code ungleich null.

### 3.5 Ressourcen des Programms

`typl -c` hat Optionen, die festlegen, welche Ressourcen das Programm während des Laufs verwenden darf.

```sh
$ typl --heap-cells 100000 -c hello.typl    # die cons-Arena auf 100000 Zellen festlegen
$ typl -c hello.typl --max-memory 64M       # höchstens 64 MiB belegen
$ typl -c hello.typl --no-cons              # keine cons-Zellen verwenden
```

- `--heap-cells N` legt die Kapazität der Arena der cons-Zellen auf `N` Zellen fest. Die Arena wächst nicht darüber hinaus: Kann die Garbage Collection keine Zelle freigeben, endet das Programm mit einem Laufzeitfehler. Ohne die Option beginnt die Arena mit 65536 Zellen und wächst nach Bedarf.
- `--max-memory SIZE` begrenzt den gesamten Speicher, den das Programm belegt. Alles zählt: die cons-Arena, Zeichenketten, Werte wie Vektoren und Hashtabellen und der Speicher der Laufzeit selbst. `SIZE` ist eine Anzahl Bytes, gefolgt optional von `K`, `M` oder `G` (das 1024-, 1024²- und 1024³-fache). Eine Belegung über die Grenze hinaus wird gemeldet, und das Programm endet mit dem Exit-Code 1. Dieses Ende lässt sich nicht mit `catch` abfangen. Ohne die Option gibt es keine Grenze.
- `--no-cons` erzeugt ein Programm, das keine cons-Zellen verwendet. Es hat keine cons-Arena; ein Programm, das etwas tut, wofür cons-Zellen nötig sind, ist daher ein **Kompilierfehler**, der diese Operation und die Kette der Funktionen von `main` bis dorthin zeigt. Cons-Zellen braucht man, um eine `Sexpr`-Liste (auch eine quotierte oder per Quasiquote gebaute) oder ein `&rest`-Argument zu bauen, sowie für `read`, `eval` und `macroexpand`. Operationen auf Zahlen, Zeichenketten, Vektoren, Hashtabellen, Strukturen, Kanälen und Ähnlichem sowie `print`/`println`/`format` brauchen keine. Lässt sich nicht zusammen mit `--heap-cells` angeben und steht nach `-c`.

`--max-memory` steht nach `-c`. `--heap-cells` darf vor oder nach `-c` stehen und gibt dann die Kapazität der cons-Arena des Programms an. Die cons-Arena zählt zur Speichergrenze; bei einer Grenze, in die die Arena nicht passt, endet das Programm, bevor es beginnt.

### 3.6 Für WebAssembly kompilieren

Mit `--target wasm32-wasip1` erzeugt `typl -c` statt eines ausführbaren Programms ein WebAssembly-Modul (ein WASI-preview-1-Kommando). Ohne `-o` heißt es wie die Quelle, mit `.wasm` statt `.typl`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # erzeugt hello.wasm
$ wasmtime hello.wasm a b                        # mit einer WASI-fähigen Laufzeitumgebung ausführen
args: #(hello.wasm a b)
fib(25) = 75025
```

- Ein mit `install.sh` installiertes `typl` lässt sich so verwenden. Eines, das mit `cargo install` installiert wird, braucht Vorbereitung (die auch Abschnitt 3.7 abdeckt). Führen Sie zuerst mit rustup `rustup target add wasm32-wasip1 wasm32-unknown-unknown` aus. Installieren Sie es dann mit `--features wasm` (`cargo install --locked --path . --features wasm`).
  Ein ohne es installiertes `typl` hat keine WebAssembly-Laufzeit, und `--target` ist ein Fehler. Für einen Debug-Build von `typl` im Repository führen Sie stattdessen im Repository `scripts/build-wasm-runtime.sh` aus.
- Gelinkt wird mit dem Linker der rustup-Toolchain (`rust-lld`); rustup wird also auch beim Kompilieren gebraucht.
- Das Modul verhält sich wie das ausführbare Programm: Standardausgabe, Standardfehler und Exit-Code, `(command-line-args)`,
  Dateien lesen und schreiben, `throw`/`catch`/`panic`, Tasks, Kanäle und `sleep` sowie `eval` funktionieren.
  Tasks laufen nacheinander in einem einzigen Thread. Dateien lassen sich nur in den Verzeichnissen lesen und
  schreiben, die die Laufzeitumgebung dem Modul zeigt (bei wasmtime mit `--dir` angegeben).
- Ein Programm, das etwas benutzt, das WebAssembly nicht hat, ist ein **Kompilierfehler**. Der Fehler zeigt, was benutzt wurde,
  und die Kette der Funktionen von `main` bis dorthin. Nicht verfügbar sind: Sockets (TCP, UDP, Unix-Domain,
  TLS, Namensauflösung), OS-Threads (`Thread`), C-Funktionen (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  und `ed`.
- WASI kann die Zeitzone nicht erfragen, daher ist die Ortszeit unbekannt. `decode-universal-time` und `encode-universal-time` ohne `zone` sowie `get-decoded-time` lösen zur Laufzeit einen panic aus. Mit `zone` funktionieren sie.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` und `--release` wirken wie bei einem ausführbaren Programm.

### 3.7 Im Browser oder in Node.js ausführen

Mit `--target wasm32-unknown-unknown` erzeugt `typl -c` ein WebAssembly-Modul, das aus JavaScript geladen und ausgeführt wird. Neben dem Modul (`hello.wasm`) entsteht im selben Ordner das JavaScript, das es lädt und ausführt (`hello.mjs`). Die Vorbereitung ist dieselbe wie in Abschnitt 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # erzeugt hello.wasm und hello.mjs
$ node hello.mjs                                       # mit Node.js ausführen
args: #()
fib(25) = 75025
```

Aus anderem JavaScript rufen Sie `run` aus `hello.mjs` auf. `write` erhält die Ausgabe als Text (`fd` ist 1 für die Standardausgabe, 2 für den Standardfehler). Der Wert von `run` ist der Exit-Code.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* zum Beispiel auf der Seite anzeigen */ } });
```

- Ohne `write` schreibt Node.js auf Standardausgabe und Standardfehler, ein Browser zeilenweise auf die Konsole.
- Im Browser liefern Sie `hello.wasm` per HTTP vom selben Ort wie `hello.mjs` aus (von `file://` lässt es sich nicht laden).
- `sleep` wartet mit `Atomics.wait`. Ein Browser erlaubt das nur in einem Worker einer cross-origin-isolierten Seite; führen Sie ein Programm, das `sleep` benutzt, daher in einem Worker aus.
- Die Ortszeit (`decode-universal-time` ohne `zone` usw.) wird mit der Zeitzone berechnet, die JavaScripts `Date` meldet.
- Zusätzlich zu dem, was in Abschnitt 3.6 nicht nutzbar ist, sind Datei- und Verzeichnisoperationen, Umgebungsvariablen, `(command-line-args)`, das Home-Verzeichnis und `dribble` ein **Kompilierfehler**. Die Standardeingabe wird als leer gelesen.

### 3.8 Für einen Mikrocontroller (Cortex-M, RISC-V) kompilieren

Mit `--target thumbv7em-none-eabihf` erzeugt `typl -c` eine ausführbare Datei (ELF) für eine Cortex-M4F- oder M7-Platine ohne Betriebssystem. Ohne `-o` wird sie nach der Quelle benannt, wobei `.typl` durch `.elf` ersetzt wird. Mit `--target thumbv8m.main-none-eabihf` ist sie für eine Cortex-M33-Platine. Mit `--target riscv32imac-unknown-none-elf` ist sie für eine Platine mit einem 32-Bit-RISC-V-Kern (Erweiterungen M, A und C). Mit `--target thumbv6m-none-eabi` ist sie für eine Cortex-M0+-Platine (M0). QEMU hat keine Cortex-M0+-Platine, daher läuft eine ausführbare Datei dafür auf `mps2-an385`, einem Cortex-M3, der dieselben Befehle unverändert ausführt (auch die Aufteilung ohne `--linker-script` ist die dieser Platine).

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # erzeugt hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # auf dem Cortex-M4 von QEMU ausführen
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # auf dem Cortex-M33 von QEMU ausführen
$ typl -c hello.typl --target thumbv6m-none-eabi
$ qemu-system-arm -M mps2-an385 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # auf dem Cortex-M3 von QEMU ausführen
$ typl -c hello.typl --target riscv32imac-unknown-none-elf
$ qemu-system-riscv32 -M virt -bios none -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # auf dem RISC-V von QEMU ausführen
```

- Ein mit `install.sh` installiertes `typl` kann direkt verwendet werden. Für `cargo install` zuerst `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf thumbv6m-none-eabi riscv32imac-unknown-none-elf` ausführen und dann mit `--features embedded` installieren (`--features wasm,embedded` für beides). Mit einem Debug-Build von `typl` im Repository `scripts/build-embedded-runtime.sh` ausführen. Gelinkt wird mit dem Linker der rustup-Toolchain.
- Den Speicher der Platine mit `--linker-script FILE` angeben. Die Datei hat die Form von `memory.x` aus `cortex-m-rt`: Adressen und Größen von FLASH und RAM. Eine Zeile wie `_typelisp_stack_size = 32K;` legt fest, wie viel des RAM der Stack belegt (ohne Angabe 64KiB); der Rest des RAM ist der Heap. Ohne die Option gilt die Aufteilung von QEMUs `mps2-an386` (für einen Cortex-M33 `mps2-an505`).

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

  Unter RISC-V ist ohne Angabe die Aufteilung von QEMUs `virt` voreingestellt. Eine angegebene Datei nennt außerdem, wie im `memory.x` von `riscv-rt`, mit `REGION_ALIAS` den Bereich, in den jede Art von Abschnitt kommt.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x20010000, LENGTH = 4M - 64K
    RAM : ORIGIN = 0x80000000, LENGTH = 16K
  }
  REGION_ALIAS("REGION_TEXT", FLASH);
  REGION_ALIAS("REGION_RODATA", FLASH);
  REGION_ALIAS("REGION_DATA", RAM);
  REGION_ALIAS("REGION_BSS", RAM);
  REGION_ALIAS("REGION_HEAP", RAM);
  REGION_ALIAS("REGION_STACK", RAM);
  _typelisp_stack_size = 4K;
  ```

- Auf einer Platine mit wenig RAM gibt `--ram SIZE` (`16K` usw.) anstelle von `--linker-script` nur an, wie viel RAM Daten, Heap und Stack haben. Der RAM der Standardaufteilung bekommt diese Größe, und der Stack belegt höchstens ein Viertel davon. Auf einer Platine, die auch den Code im RAM hält, wie `virt` bei RISC-V, kommt der Code in den Rest des RAM der Platine. Was die Laufzeitumgebung anfangs belegt, wird passend zum Heap klein gehalten. Es kann nicht zusammen mit `--linker-script` verwendet werden.

- Eine Platine hat kein Betriebssystem, also hält nichts einen überlaufenden Stack auf. Das Programm prüft daher beim Eintritt in jede seiner Funktionen, wie viel Stack übrig ist; ist es zu wenig, schreibt es `stack overflow: the program needs more than the ... bytes of the stack` und endet mit dem Exit-Code 1. Für mehr Stack vergrößert man `_typelisp_stack_size` in der Datei von `--linker-script` oder `--ram`. `--no-stack-check` lässt die Prüfung weg (das Programm wird etwas kleiner und schneller, aber ein überlaufender Stack kann den Heap unbemerkt beschädigen). In beiden Fällen steht, wenn der Stack in der Laufzeit überläuft und über sein unteres Ende hinausläuft, vor dem Bericht des folgenden Panics oder HardFaults `stack overflow: the stack has run past its bottom into the heap`.

- `--board-functions` wählt, wer Ausgabe, Eingabe, Beenden und die Uhr (die Funktionen der Platine) bereitstellt.
  - `semihosting` (Standard): über Semihosting, zum Ausführen unter QEMU (`-semihosting-config enable=on`) oder einem Debugger.
  - `c`: Die Platine stellt sie als C-Funktionen bereit. Ein Objekt oder eine statische Bibliothek, die die folgenden Funktionen definiert, mit `--link FILE` übergeben (`--link` kann mehrfach angegeben werden; eine Herstellerbibliothek, die `defffi` aufruft, wird ebenso übergeben).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 ist die Standardausgabe, 2 die Standardfehlerausgabe
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0; gelesene Bytes, 0 am Ende
  void     typelisp_host_exit(int32_t code);                              // kehrt nicht zurück
  uint64_t typelisp_host_now_us(void);                                    // Mikrosekunden einer Uhr, die nie zurückläuft
  int64_t  typelisp_host_unix_us(void);                                   // Mikrosekunden seit 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // wartet us Mikrosekunden
  ```

- Die Register einer Platine (in den Speicher eingeblendete Peripherie) werden mit `volatile-read-u8`, `volatile-read-u16` und `volatile-read-u32` gelesen und mit `volatile-write-u8`, `volatile-write-u16` und `volatile-write-u32` geschrieben. Die Adresse ist ein `int` oder eine Ganzzahl fester Breite wie `u16`, der gelesene Wert hat den Typ der Breite (`u8` usw.). Jeder Zugriff liest oder schreibt genau einmal, an der Stelle, an der er steht; der Compiler lässt ihn weder weg noch verschiebt er ihn. Alle sind nur innerhalb von `unsafe` nutzbar. Eine negative Adresse oder eine, die kein Vielfaches der Breite ist, ist ein Laufzeitfehler, bevor etwas gelesen oder geschrieben wird.

  ```lisp
  ;; UART 0 des mps2-an386 von QEMU: Daten 0x40004000, Status 0x40004004, Steuerung 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; warten, solange gesendet wird
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; Senden einschalten
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" und ein Zeilenumbruch
  ```

- Die Funktion, die bei einem Interrupt aufgerufen wird, schreibt man als `defun` mit dem Attribut `#[interrupt(N)]` (Peripherie-Interrupt `N`) oder einem wie `#[exception(SysTick)]`. Ein Interrupt wird mit `(enable-interrupt N)` eingeschaltet. Ein Handler läuft sofort; nur wenn er mitten in der Arbeit der Laufzeit kommt, läuft er, nachdem diese fertig ist. Ein Handler kann nicht warten (`sleep` ist ein Kompilierfehler). Details stehen in Abschnitt 3.17 der [Syntaxreferenz](../reference/syntax.md). Auf einem Cortex-M0+ ist `N` 0 bis 31, und `#[exception(NAME)]` kann nur `NonMaskableInt`, `SVCall`, `PendSV` und `SysTick` nennen. Unter RISC-V ist `N` die Nummer des Interrupts in `mcause` (0 bis 31: 3 ist der Software-Interrupt, 7 der Timer, 11 die externen Interrupts), und `#[exception(NAME)]` kann nicht verwendet werden.

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; Interrupt 5 selbst auslösen (das Pending-Register des NVIC)
  ```

- Die Laufzeitumgebung startet das Programm (Vektortabelle und Vorbereitung des RAM) und verwaltet den Speicher. Ein HardFault (unter RISC-V eine Exception) oder ein unbehandelter Interrupt beendet das Programm mit dem Exit-Code eines Panic (1).
- `throw`/`catch`/`panic`, Bignums, Brüche und Gleitkommazahlen sowie Tasks, Kanäle und `sleep` verhalten sich wie in der ausführbaren Datei. Tasks laufen nacheinander auf dem einen Kern. C-Funktionen (`defffi`) können verwendet werden.
- Ein Programm, das etwas verwendet, das die Platine nicht hat, ist ein **Kompilierfehler**: zusätzlich zu dem, was in Abschnitt 3.7 nicht nutzbar ist, `eval` und `macroexpand`. Die Ortszeit ist unbekannt (wie in Abschnitt 3.6).
- Die Cons-Arena beginnt mit 1024 Zellen und wächst bei Bedarf. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` und `--release` funktionieren unverändert.

### 3.9 Für einen Arduino Uno (AVR) kompilieren

Mit `--target avr-atmega328p` erzeugt `typl` eine ausführbare Datei (ELF) für den Mikrocontroller des Arduino Uno (ATmega328P). Das folgende Programm lässt die LED des Boards blinken.

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
  (unsafe (volatile-write-u8 DDRB LED))           ; als Ausgang schalten
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; umschalten
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # erzeugt blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # in QEMU ausführen
```

- Es gibt keine Laufzeitumgebung, daher ist ein Programm, das sie benutzt, ein **Kompilierfehler**. Die Meldung nennt, was benötigt wird und über welche Funktionen es erreicht wird. Verwendbar sind: Rechnen mit Ganzzahlen fester Breite (`i8` bis `u32`), `f32`, `f64`, `bool` und `char`, Funktionsaufrufe (auch rekursive), Verzweigungen, Schleifen, `defvar` und `defconstant` mit solchen Werten, `volatile-read-u8` und Ähnliches sowie Aufrufe von C-Funktionen.
- Nicht verwendbar sind: Werte im Heap wie Zeichenketten, Listen und Strukturen; `catch` und Aufrufe von Funktionswerten; Aufrufe von C-Funktionen, die eine Zeichenkette oder eine Funktion erhalten; mathematische Funktionen von Gleitkommazahlen wie `sin` und `expt` (`sqrt` und Rundungen wie `floor` sind verwendbar).
- Ein Interrupt-Handler wird als `#[interrupt(N)]` geschrieben ([Syntax](../reference/syntax.md), Abschnitt 3.17). `N` ist die Interrupt-Nummer des ATmega328P, 1 bis 25, wobei Reset als 0 zählt (der Überlauf von Timer 1 ist 13). Siehe das Beispiel unten.
- Eine Division durch 0 hält den Mikrocontroller an dieser Stelle an (es kommen auch keine Interrupts mehr). Ebenso `/` des kleinsten vorzeichenbehafteten Werts durch `-1`.
- Ein `int` ist nicht verwendbar (Kompilierfehler), da es keine feste Breite hat. Verwenden Sie einen Typ fester Breite wie `i16` oder `u16`. Auch Adressen lassen sich mit Ganzzahlen fester Breite berechnen. Was beim Kompilieren feststeht, ist verwendbar, etwa eine als Zahl geschriebene Adresse und ein `int` einer `defconstant`.
- Reicht der Stack für eine Rekursion nicht aus, hält der Mikrocontroller an.
- Eine C-Funktion wird mit `defffi` deklariert; die für AVR kompilierte Objektdatei wird mit `--link FILE` übergeben.
- Eine Schleife, die nichts tut, wird ab `-O1` entfernt. Lesen Sie in einer Warteschleife zum Beispiel ein Register.
- Wenn `main` endet, hält der Mikrocontroller an.
- Mit `--linker-script FILE` lässt sich die Speicheraufteilung ändern. Ohne Angabe sind es 32 KiB Flash abzüglich der letzten 512 Bytes des Bootloaders und 2 KiB RAM. RAM-Adressen werden mit addiertem `0x800000` geschrieben, etwa `0x800100`.
- Das Rechnen mit `f32` und `f64` übernehmen Hilfsfunktionen. Die benutzten kommen in den Flash (die für `f64` sind größer).
- Gelinkt wird wie in Abschnitt 3.8 mit dem Linker der rustup-Toolchain.

Ein Beispiel, das die LED mit einem Timer-Interrupt blinken lässt. Ein Interrupt wird im Register des jeweiligen Geräts eingeschaltet.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; Überlauf von Timer 1
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; umschalten
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; Überlauf-Interrupt einschalten
          (volatile-write-u8 TCCR1B 3))           ; mit 1/64 des Takts zählen
  (while true))
```

- Ein Programm mit einem Handler lässt Interrupts zu, sobald die globalen Variablen initialisiert sind. Richten Sie die Geräte am Anfang von `main` ein.
- `enable-interrupt` und `#[exception(NAME)]` sind nicht verwendbar (Übersetzungsfehler).
- Während ein Handler läuft, warten andere Interrupts. Wie jede andere Funktion kann ein Handler die Laufzeit nicht verwenden.
- Ein einzelnes Lesen oder Schreiben einer globalen Variable wird nicht von einem Handler unterbrochen, auch nicht bei Werten von zwei Byte oder mehr. Dazu werden Interrupts beim Lesen einer Variable gesperrt, die ein Handler schreibt, und beim Schreiben einer Variable, die ein Handler liest oder schreibt; eine Warnung beim Kompilieren nennt die Variable. Zwischen Lesen und Schreiben einer Aktualisierung (`(setf n (+ n 1))`) kann ein Interrupt kommen. Um eine Variable zu aktualisieren, die auch ein Handler schreibt, lesen Sie das Statusregister (Adresse `#x5F`) und merken es sich, schreiben 0 hinein, um Interrupts anzuhalten, und schreiben danach den gemerkten Wert zurück.
- Ein Interrupt ohne Handler hält den Mikrocontroller an.

## 4. Dumps

Man kann die Definitionen der aktuellen Sitzung in eine Datei speichern und beim nächsten Mal von dort aus
starten.

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

Das funktioniert auch beim Ausführen einer Datei, wie in `typl --image session.typld prog.typl`.

- Gespeichert werden die **Definitionen**. In der Sitzung ausgewertete Ausdrücke werden nicht gespeichert.
- Mit `compile` kompilierte Funktionen werden in kompilierter Form gespeichert.
- Globale Variablen werden wiederhergestellt, indem **ihre Initialisierer erneut ausgeführt werden**, nicht mit
  den Werten, die sie beim Schreiben des Dumps hatten.
- Ein Dump lässt sich nicht von einem `typl` einer anderen Version laden als der, die ihn geschrieben hat (das
  ist ein Fehler).

Führt man eine Datei aus und ruft darin `(dump ...)` auf, stehen die Definitionen dieser Datei in einem Modul,
das nach der Datei benannt ist. Eine in `dp.typl` definierte Funktion heißt `dp::sq`, und um sie aus einer
anderen Datei aufzurufen, braucht es `pub` ([Module und Dateiaufteilung](modules.md)).

## 5. Zu kompilierten Moduldateien

Es gibt kein Format wie `.fasl` in Common Lisp, um das Kompilat jedes Moduls in eine Datei zu schreiben.
`compile-file` baut das Programm direkt aus den Quellen. Es bleiben keine Zwischendateien zurück.
