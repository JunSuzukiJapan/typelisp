<!-- translated-from: docs/ja/guide/compile.md @ 8c1712373ea704e08417e1fe1eafb4ea4a7f41a2 -->
# Compileren

Tenzij je iets anders doet, draaien typelisp-programma's in de interpreter. Daarnaast zijn er twee
manieren om naar native code te compileren en één manier om een omgeving op te slaan. De details van
de specificatie staan in [Syntaxreferentie hoofdstuk 10](../reference/syntax.md#10-compilatie).

| Methode | Hoe | Resultaat |
|---|---|---|
| JIT-compilatie | `(compile name)` | Een functie in de lopende sessie wordt vervangen door native code |
| AOT-compilatie | `typl -c src.typl` of `(compile-file "src.typl" "out")` | Een zelfstandig uitvoerbaar bestand |
| Dump | `(dump "file.typld")` | Slaat de definities op; `typl --image` start opnieuw vanuit dezelfde omgeving |

## 1. Voorbereiding

Compileren gebruikt LLVM 22. Als je `typl` hebt gebouwd volgens [README.md](../../../README.md), is
verder geen voorbereiding nodig.

Uitvoerbare bestanden die met AOT-compilatie zijn gemaakt, worden gelinkt met de statische
bibliotheek `libtypelisp_front.a`. Een release-build van `typl` (ook een die met `cargo install` is
geïnstalleerd) draagt deze bibliotheek in zichzelf mee, dus er is geen voorbereiding nodig. De eerste
keer dat hij compileert, schrijft hij de bibliotheek weg naar `~/.typelisp/lib/<build ID>/` (of naar
`$TYPELISP_HOME/lib/<build ID>/` als de omgevingsvariabele `TYPELISP_HOME` is gezet) en gebruikt
daarna die kopie. `typl --remove-lib` verwijdert haar (met `--others` de kopieën die door andere
versies van `typl` zijn weggeschreven; met `--all` alle). Een debug-build van `typl` gebruikt de
bibliotheek in `target/debug/` van de repository waarin hij is gebouwd. Om een bibliotheek te
gebruiken die ergens anders staat, geef je haar map op met `--lib-dir` bij het starten van `typl`
(paragraaf 3.2).
Op macOS gebruikt het linken de Xcode Command Line Tools.

## 2. JIT-compilatie

Dit maakt van een al gedefinieerde functie ter plekke native code.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; from here on, calls run the compiled code
```

- `name` wordt niet geëvalueerd. Schrijf de functienaam zoals hij is (niet als string). Schrijf een
  methode met de typenaam, zoals in `(compile point::norm)`.
- Functies die hij aanroept worden samen met hem gecompileerd.
- **Generieke functies kunnen niet worden gecompileerd.** Op elke plek waar ze worden gebruikt wordt
  een kopie per type gemaakt. Compileer in plaats daarvan de functie die ze met concrete types
  aanroept.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` en `dump` zijn bewerkingen van de
  interpreter, dus een functie die ze aanroept kan niet worden gecompileerd. Proberen haar te
  compileren geeft een fout die de reden noemt.

Gebruik `disassemble` om het resultaat van de compilatie te bekijken.

```lisp
(disassemble fib)          ; the host machine code
(disassemble fib true)     ; LLVM IR
```

## 3. Een uitvoerbaar bestand bouwen met AOT-compilatie

### 3.1 Het programma schrijven

Definieer als beginpunt een **functie `main` zonder argumenten**.

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

De `(main)` aan het einde van het bestand staat er zodat `main` wordt aangeroepen wanneer je
`typl hello.typl` uitvoert. `compile-file` slaat deze laatste `(main)` over, zodat hetzelfde bestand
zowel in de interpreter als met AOT-compilatie werkt.

### 3.2 Compileren

Gebruik vanaf de opdrachtregel `typl -c` (`typl --compile` is hetzelfde).

```sh
$ typl -c hello.typl            # makes hello
$ typl -c hello.typl -o fib     # names the executable fib
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Zonder `-o` krijgt het uitvoerbare bestand de naam van het bronbestand zonder `.typl` en wordt het in
dezelfde map als het bronbestand geplaatst. Eindigt de naam van het bronbestand niet op `.typl`, dan
is `-o` verplicht. Bij `-c` (`--compile`) kunnen `--image` en `--feature` niet worden
opgegeven.

Met `--release` maakt `typl` een **release-build**, die `eprint`/`eprintln` (uitvoer naar standaardfout om iets te onderzoeken; [Afdrukken](../reference/functions/printing.md)) samen met hun argumenten verwijdert. Wat wordt verwijderd, wordt toch op types gecontroleerd, dus een typefout daarin laat de compilatie mislukken. `--release` is een optie van `typl` als geheel: ze werkt niet alleen bij `-c`, maar ook bij het uitvoeren van een bestand, in de REPL en bij `typl test`. Een `compile-file` dat wordt aangeroepen in een `typl` met `--release` maakt ook een release-build, en ook uit code die met `eval` wordt uitgevoerd in een zo gemaakt uitvoerbaar bestand worden `eprint`/`eprintln` verwijderd.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # hetzelfde als het na -c staat
```

`-O0`, `-O1`, `-O2` en `-O3` stellen het **optimalisatieniveau** in. De standaard is `-O0`, dat niet optimaliseert. Vanaf `-O1` voert de compiler eerst eenvoudige optimalisaties uit, zoals een variabele die alleen aan een literal gebonden is door zijn waarde vervangen of gehele-getallenrekenkunde op constanten vooraf uitrekenen, en laat de rest over aan de optimalisatie van LLVM op het niveau met dezelfde naam. Op elk niveau zijn de uitkomsten van de berekening hetzelfde. Net als `--release` is het een optie van `typl` als geheel: behalve voor het uitvoerbare bestand dat `-c` maakt, werkt het op de code die gecompileerd wordt bij het uitvoeren van een bestand, in de REPL en in `typl test`, en op een `compile-file` die daar wordt aangeroepen. Code die de interpreter uitvoert, wordt niet geoptimaliseerd.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # hetzelfde als het na -c staat
```

Hetzelfde kun je doen door `compile-file` aan te roepen vanuit de REPL of vanuit een programma.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Als je herhaaldelijk bouwt, kun je deze ene regel in een bestand zetten en met `typl build.typl`
uitvoeren.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Bestandsnamen worden opgelost vanuit **de huidige map waarin `typl` is gestart**, niet vanuit de
locatie van `build.typl`.

Om een `libtypelisp_front.a` te linken die ergens anders staat dan waar `typl` zoekt, geef je haar
map op met `--lib-dir`. Dit geldt voor zowel `typl -c` als `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Heeft de opgegeven map geen `libtypelisp_front.a`, dan stopt `typl` met een fout. Het bestand werkt
alleen met de `typl` die er samen mee is gebouwd. Kopieer het opnieuw nadat je `typl` opnieuw hebt
gebouwd.

### 3.3 Wat een AOT-gecompileerd bestand mag bevatten

- Het hoogste niveau van het invoerbestand mag alleen definities bevatten (`defun` `defvar` `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype`
  `deftrait` `impl` `defffi`, `(unsafe (def-c-struct ...))`) en `use` `module`. Expressies op het
  hoogste niveau zoals `(println ...)` zijn niet toegestaan, behalve de laatste `(main)`. Zet het
  werk binnen `main`.
- Zonder een `main` zonder argumenten mislukt het compileren met een fout.
- De bestanden van modules die met `use` zijn binnengehaald worden ook gecompileerd en samengevoegd
  tot één uitvoerbaar bestand.
- Bibliotheken die in `defffi` met `:library` zijn genoemd worden automatisch gelinkt
  ([C-FFI](ffi.md)).
- Elke functie van de standaardbibliotheek kan met AOT-compilatie worden gebruikt. Ook `eval` kan
  worden gebruikt, maar dan komen de typechecker en de interpreter in het uitvoerbare bestand, waardoor
  het groter wordt en trager start. Programma's die `eval` niet aanroepen bevatten ze niet.

### 3.4 Hoe het uitvoerbare bestand zich gedraagt

- `(command-line-args)` geeft een `Vector<string>` van dezelfde vorm terug, of je nu
  `typl hello.typl a b` of `./hello a b` uitvoert. Het eerste element is de programmanaam.
- Stel de afsluitcode in met `(exit n)`. Als `main` normaal terugkeert, is die 0.
- Bij `panic` drukt het programma de melding af en eindigt met een code ongelijk aan nul.

### 3.5 Middelen die het uitvoerbare bestand gebruikt

`typl -c` heeft opties die bepalen welke middelen het uitvoerbare bestand tijdens het draaien mag gebruiken.

```sh
$ typl --heap-cells 100000 -c hello.typl    # zet de cons-arena vast op 100000 cellen
$ typl -c hello.typl --max-memory 64M       # alloceer hoogstens 64 MiB
$ typl -c hello.typl --no-cons              # gebruik geen cons-cellen
```

- `--heap-cells N` zet de capaciteit van de arena van cons-cellen vast op `N` cellen. De arena groeit daar niet voorbij: kan de garbage collection geen cel vrijmaken, dan stopt het programma met een runtimefout. Zonder de optie begint de arena met 65536 cellen en groeit ze naar behoefte.
- `--max-memory SIZE` begrenst het totale geheugen dat het uitvoerbare bestand alloceert. Alles telt mee: de cons-arena, strings, waarden zoals vectoren en hashtabellen, en het geheugen van de runtime zelf. `SIZE` is een aantal bytes, eventueel gevolgd door `K`, `M` of `G` (1024, 1024² en 1024³ keer). Een allocatie die over de grens zou gaan wordt gemeld, en het programma eindigt met exitcode 1. Dit einde is niet met `catch` op te vangen. Zonder de optie is er geen grens.
- `--no-cons` maakt een uitvoerbaar bestand dat geen cons-cellen gebruikt. Het heeft geen cons-arena, dus een programma dat iets doet waarvoor cons-cellen nodig zijn is een **compileerfout**, die die bewerking toont en de keten van functies vanaf `main` die haar bereikt. Cons-cellen zijn nodig om een `Sexpr`-lijst (ook een gequote lijst of een die met quasiquote is gebouwd) of een `&rest`-argument te bouwen, en voor `read`, `eval` en `macroexpand`. Bewerkingen op getallen, strings, vectoren, hashtabellen, structs, kanalen en dergelijke, en `print`/`println`/`format`, gebruiken er geen. Kan niet samen met `--heap-cells` worden opgegeven en staat na `-c`.

`--max-memory` schrijf je na `-c`. `--heap-cells` mag voor of na `-c` staan en is dan de capaciteit van de cons-arena van het uitvoerbare bestand. De cons-arena telt mee voor de geheugengrens: bij een grens waar de arena niet in past, stopt het programma voordat het begint.

### 3.6 Compileren voor WebAssembly

Met `--target wasm32-wasip1` maakt `typl -c` een WebAssembly-module (een WASI preview 1-commando) in plaats van een uitvoerbaar bestand. Zonder `-o` krijgt die de naam van de bron, met `.wasm` in plaats van `.typl`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # maakt hello.wasm
$ wasmtime hello.wasm a b                        # uitvoeren met een omgeving die WASI ondersteunt
args: #(hello.wasm a b)
fib(25) = 75025
```

- Een met `install.sh` geïnstalleerde `typl` is zo te gebruiken. Een die met `cargo install` wordt geïnstalleerd, vraagt voorbereiding (die ook voor paragraaf 3.7 geldt). Voer eerst met rustup `rustup target add wasm32-wasip1 wasm32-unknown-unknown` uit. Installeer hem daarna met `--features wasm` (`cargo install --locked --path . --features wasm`).
  Een zonder deze optie geïnstalleerde `typl` heeft geen WebAssembly-runtime, en `--target` is een fout. Voor een debug-build van `typl` in de repository voert u in plaats daarvan `scripts/build-wasm-runtime.sh` uit in de repository.
- Het linken gebruikt de linker van de rustup-toolchain (`rust-lld`), dus ook bij het compileren is rustup nodig.
- De module gedraagt zich als het uitvoerbare bestand: standaarduitvoer, standaardfout en exitcode, `(command-line-args)`,
  bestanden lezen en schrijven, `throw`/`catch`/`panic`, taken, kanalen en `sleep`, en `eval` werken.
  Taken draaien na elkaar op één thread. Bestanden kunnen alleen gelezen en geschreven worden in de mappen
  die de omgeving aan de module toont (bij wasmtime opgegeven met `--dir`).
- Een programma dat iets gebruikt wat WebAssembly niet heeft, is een **compileerfout**. De fout toont wat gebruikt is
  en de keten van functies vanaf `main` die er komt. Niet beschikbaar: sockets (TCP, UDP, Unix-domein,
  TLS, naamresolutie), OS-threads (`Thread`), C-functies (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  en `ed`.
- WASI kan de tijdzone niet opvragen, dus de lokale tijd is onbekend. `decode-universal-time` en `encode-universal-time` zonder `zone`, en `get-decoded-time`, geven tijdens het uitvoeren een panic. Met `zone` werken ze.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` en `--release` werken zoals bij een uitvoerbaar bestand.

### 3.7 Uitvoeren in een browser of in Node.js

Met `--target wasm32-unknown-unknown` maakt `typl -c` een WebAssembly-module die vanuit JavaScript wordt geladen en uitgevoerd. Naast de module (`hello.wasm`) komt in dezelfde map het JavaScript dat haar laadt en uitvoert (`hello.mjs`). De voorbereiding is dezelfde als in paragraaf 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # maakt hello.wasm en hello.mjs
$ node hello.mjs                                       # uitvoeren met Node.js
args: #()
fib(25) = 75025
```

Vanuit ander JavaScript roept u `run` uit `hello.mjs` aan. `write` krijgt de uitvoer als tekst (`fd` is 1 voor standaarduitvoer, 2 voor standaardfout). De waarde van `run` is de exitcode.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* bijvoorbeeld op de pagina tonen */ } });
```

- Zonder `write` schrijft Node.js naar standaarduitvoer en standaardfout, en een browser regel voor regel naar de console.
- Serveer in een browser `hello.wasm` via HTTP vanaf dezelfde plek als `hello.mjs` (vanaf `file://` kan het niet geladen worden).
- `sleep` wacht met `Atomics.wait`. Een browser staat dat alleen toe in een Worker van een cross-origin-geïsoleerde pagina; voer een programma dat `sleep` gebruikt dus uit in een Worker.
- De lokale tijd (`decode-universal-time` zonder `zone` enzovoort) wordt berekend met de tijdzone die `Date` van JavaScript meldt.
- Naast wat in paragraaf 3.6 niet kan, zijn bestands- en mapbewerkingen, omgevingsvariabelen, `(command-line-args)`, de thuismap en `dribble` een **compileerfout**. Standaardinvoer wordt als leeg gelezen.

### 3.8 Compileren voor een microcontroller (Cortex-M, RISC-V)

Met `--target thumbv7em-none-eabihf` maakt `typl -c` een uitvoerbaar bestand (ELF) voor een Cortex-M4F- of M7-bord zonder besturingssysteem. Zonder `-o` krijgt het de naam van de bron met `.typl` vervangen door `.elf`. Met `--target thumbv8m.main-none-eabihf` is het voor een Cortex-M33-bord. Met `--target riscv32imac-unknown-none-elf` is het voor een bord met een 32-bits RISC-V-kern (de uitbreidingen M, A en C). Met `--target thumbv6m-none-eabi` is het voor een Cortex-M0+-bord (M0). QEMU heeft geen Cortex-M0+-bord, dus een uitvoerbaar bestand daarvoor draait op `mps2-an385`, een Cortex-M3, die dezelfde instructies ongewijzigd uitvoert (ook de indeling zonder `--linker-script` is die van dat bord).

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # maakt hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # draai het op de Cortex-M4 van QEMU
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # uitvoeren op de Cortex-M33 van QEMU
$ typl -c hello.typl --target thumbv6m-none-eabi
$ qemu-system-arm -M mps2-an385 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # uitvoeren op de Cortex-M3 van QEMU
$ typl -c hello.typl --target riscv32imac-unknown-none-elf
$ qemu-system-riscv32 -M virt -bios none -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # uitvoeren op de RISC-V van QEMU
```

- Een met `install.sh` geïnstalleerde `typl` kan direct worden gebruikt. Voor `cargo install` voer je eerst `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf thumbv6m-none-eabi riscv32imac-unknown-none-elf` uit en installeer je met `--features embedded` (`--features wasm,embedded` voor beide). Met een debugbuild van `typl` in de repository voer je `scripts/build-embedded-runtime.sh` uit. Het linken gebruikt de linker van de rustup-toolchain.
- Geef het geheugen van het bord op met `--linker-script FILE`. Het heeft de vorm van de `memory.x` van `cortex-m-rt`: de adressen en groottes van FLASH en RAM. Een regel als `_typelisp_stack_size = 32K;` bepaalt hoeveel RAM de stack inneemt (zonder opgave 64KiB); de rest van het RAM is de heap. Zonder de optie geldt de indeling van QEMU's `mps2-an386` (`mps2-an505` voor een Cortex-M33).

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

  Op RISC-V is zonder opgave de indeling van QEMU's `virt` de standaard. Een opgegeven bestand noemt ook met `REGION_ALIAS`, zoals de `memory.x` van `riscv-rt`, het gebied waar elk soort sectie komt.

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

- `--board-functions` kiest wie uitvoer, invoer, afsluiten en de klok (de functies van het bord) levert.
  - `semihosting` (standaard): via semihosting, om onder QEMU (`-semihosting-config enable=on`) of een debugger te draaien.
  - `c`: het bord levert ze als C-functies. Geef met `--link FILE` een object of statische bibliotheek door die de functies hieronder definieert (`--link` mag herhaald worden; een bibliotheek van de fabrikant die `defffi` aanroept geef je op dezelfde manier door).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 is standaarduitvoer, 2 standaardfout
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0; gelezen bytes, 0 aan het eind
  void     typelisp_host_exit(int32_t code);                              // keert niet terug
  uint64_t typelisp_host_now_us(void);                                    // microseconden van een klok die nooit teruggaat
  int64_t  typelisp_host_unix_us(void);                                   // microseconden sinds 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // wacht us microseconden
  ```

- De registers van een bord (in het geheugen afgebeelde randapparatuur) lees je met `volatile-read-u8`, `volatile-read-u16` en `volatile-read-u32`, en schrijf je met `volatile-write-u8`, `volatile-write-u16` en `volatile-write-u32`. Het adres is een `int` of een geheel getal van vaste breedte zoals `u16`, en de gelezen waarde heeft het type van de breedte (`u8` enzovoort). Elk leest of schrijft precies één keer, op de plek waar het staat; de compiler laat het niet weg en verplaatst het niet. Alle zijn alleen binnen `unsafe` te gebruiken. Een adres dat negatief is, of geen veelvoud van de breedte, is een runtimefout voordat er iets gelezen of geschreven wordt.

  ```lisp
  ;; UART 0 van de mps2-an386 van QEMU: data 0x40004000, status 0x40004004, besturing 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; wachten zolang hij verstuurt
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; versturen inschakelen
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" en een nieuwe regel
  ```

- Schrijf de functie die wordt aangeroepen als er een interrupt komt als een `defun` met het attribuut `#[interrupt(N)]` (periferie-interrupt `N`) of een zoals `#[exception(SysTick)]`. Schakel een interrupt in met `(enable-interrupt N)`. Een handler draait meteen; alleen als hij midden in werk van de runtime komt, draait hij nadat dat werk klaar is. Een handler kan niet wachten (`sleep` is een compileerfout). De details staan in paragraaf 3.17 van de [syntaxreferentie](../reference/syntax.md). Op een Cortex-M0+ is `N` 0 tot en met 31, en kan `#[exception(NAME)]` alleen `NonMaskableInt`, `SVCall`, `PendSV` en `SysTick` noemen. Op RISC-V is `N` het nummer van de interrupt in `mcause` (0 tot en met 31: 3 is de software-interrupt, 7 de timer, 11 de externe interrupts), en `#[exception(NAME)]` kan niet worden gebruikt.

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; interrupt 5 zelf veroorzaken (het pending-register van de NVIC)
  ```

- De runtime start het programma (de vectortabel en het klaarmaken van het RAM) en wijst geheugen toe. Een HardFault (op RISC-V een exceptie) of een onafgehandelde interrupt beëindigt het programma met de exitcode van een panic (1).
- `throw`/`catch`/`panic`, bignums, breuken en drijvende-kommagetallen, en taken, kanalen en `sleep` gedragen zich zoals in het uitvoerbare bestand. Taken draaien om de beurt op de ene kern. C-functies (`defffi`) kunnen worden gebruikt.
- Een programma dat iets gebruikt wat het bord niet heeft, is een **compileerfout**: naast wat in paragraaf 3.7 niet kan, `eval` en `macroexpand`. De lokale tijd is onbekend (zoals in paragraaf 3.6).
- De cons-arena begint met 1024 cellen en groeit wanneer nodig. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` en `--release` werken zoals ze zijn.

### 3.9 Compileren voor een Arduino Uno (AVR)

Met `--target avr-atmega328p` maakt u een uitvoerbaar bestand (ELF) voor de microcontroller van de Arduino Uno (ATmega328P). Het volgende programma laat de led van het bord knipperen.

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
  (unsafe (volatile-write-u8 DDRB LED))           ; als uitgang instellen
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; omkeren
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # maakt blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # in QEMU uitvoeren
```

- Er is geen runtime, dus een programma dat die gebruikt is een **compileerfout**. De fout vermeldt wat er nodig is en via welke functies het wordt bereikt. Bruikbaar zijn: rekenen met gehele getallen van vaste breedte (`i8` tot en met `u32`), `f32`, `f64`, `bool` en `char`, functieaanroepen (ook recursieve), voorwaarden, lussen, `defvar` en `defconstant` met zulke waarden, `volatile-read-u8` en dergelijke, en aanroepen van C-functies.
- Niet bruikbaar zijn: waarden in de heap, zoals strings, lijsten en structs; `catch` en aanroepen van functiewaarden; aanroepen van C-functies die een string of een functie krijgen; wiskundige functies van drijvendekommagetallen zoals `sin` en `expt` (`sqrt` en afronden zoals `floor` zijn wel bruikbaar).
- Een interrupthandler schrijf je als `#[interrupt(N)]` ([Syntaxis](../reference/syntax.md), paragraaf 3.17). `N` is het interruptnummer van de ATmega328P, 1 tot en met 25 met reset als 0 geteld (overloop van timer 1 is 13). Zie het voorbeeld hieronder.
- Delen door 0 stopt de microcontroller ter plekke (er komen ook geen interrupts meer). Dat geldt ook voor `/` van de kleinste waarde met teken door `-1`.
- Een `int` is niet bruikbaar (een compileerfout), omdat het geen vaste breedte heeft. Gebruik een type van vaste breedte, zoals `i16` of `u16`. Ook adressen reken je uit met gehele getallen van vaste breedte. Wat bij het compileren vastligt is wel bruikbaar, zoals een als getal geschreven adres en een `int` van een `defconstant`.
- Raakt de stack bij een recursie op, dan stopt de microcontroller.
- Een C-functie declareer je met `defffi`; het voor AVR gecompileerde objectbestand geef je mee met `--link FILE`.
- Een lus die niets doet, verdwijnt vanaf `-O1`. Lees in een wachtlus bijvoorbeeld een register.
- Als `main` eindigt, stopt de microcontroller.
- Met `--linker-script FILE` verandert u de geheugenindeling. Zonder deze optie is dat 32 KiB flash min de laatste 512 bytes van de bootloader, en 2 KiB RAM. RAM-adressen worden geschreven met `0x800000` erbij opgeteld, zoals `0x800100`.
- Het rekenen met `f32` en `f64` doen hulpfuncties. De gebruikte komen in de flash (die voor `f64` zijn groter).
- Voor het linken wordt, net als in paragraaf 3.8, de linker van de rustup-toolchain gebruikt.

Een voorbeeld dat de led laat knipperen met een timerinterrupt. Een interrupt schakel je in het register van het apparaat zelf in.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; overloop van timer 1
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; omkeren
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; schakel de overloopinterrupt in
          (volatile-write-u8 TCCR1B 3))           ; tel op 1/64 van de klok
  (while true))
```

- Een programma met een handler laat interrupts toe zodra de globale variabelen geïnitialiseerd zijn. Stel de apparaten in aan het begin van `main`.
- `enable-interrupt` en `#[exception(NAME)]` zijn niet bruikbaar (compileerfout).
- Terwijl een handler loopt, wachten andere interrupts. Net als elke andere functie kan een handler de runtime niet gebruiken.
- Eén keer lezen of schrijven van een globale variabele wordt niet door een handler onderbroken, ook niet bij waarden van twee bytes of meer. Daartoe worden interrupts uitgeschakeld bij het lezen van een variabele die een handler schrijft en bij het schrijven van een variabele die een handler leest of schrijft, en een waarschuwing bij het compileren meldt de variabele. Tussen het lezen en schrijven van een bijwerking (`(setf n (+ n 1))`) kan wel een interrupt komen. Om een variabele bij te werken die een handler ook schrijft, lees en bewaar je het statusregister (adres `#x5F`), schrijf je er 0 in om interrupts te stoppen, en schrijf je daarna de bewaarde waarde terug.
- Een interrupt zonder handler stopt de microcontroller.

## 4. Dumps

Je kunt de definities van de huidige sessie in één bestand opslaan en er de volgende keer mee
beginnen.

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

Het werkt ook bij het uitvoeren van een bestand, zoals in `typl --image session.typld prog.typl`.

- Wat wordt opgeslagen zijn de **definities**. Expressies die in de sessie zijn geëvalueerd worden
  niet opgeslagen.
- Functies die je met `compile` hebt gecompileerd worden in gecompileerde vorm opgeslagen.
- Globale variabelen worden hersteld door **hun initializers opnieuw uit te voeren**, niet met de
  waarden die ze hadden toen de dump werd geschreven.
- Een dump kan niet worden geladen door een `typl` van een andere versie dan die welke hem schreef
  (het is een fout).

Voer je een bestand uit en roep je daaruit `(dump ...)` aan, dan staan de definities van dat bestand
in een module die naar het bestand is genoemd. Een functie die in `dp.typl` is gedefinieerd heet
`dp::sq`, en om haar vanuit een ander bestand aan te roepen is `pub` nodig
([Modules en bestandsindeling](modules.md)).

## 5. Over gecompileerde modulebestanden

Er is geen formaat, zoals de `.fasl` van Common Lisp, om het gecompileerde resultaat van elke module
naar een bestand weg te schrijven. `compile-file` bouwt het uitvoerbare bestand rechtstreeks uit de
bronnen. Er blijven geen tussenbestanden achter.
