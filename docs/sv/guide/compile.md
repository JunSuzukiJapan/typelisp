<!-- translated-from: docs/ja/guide/compile.md @ 1b5dc6ac828d8e4a95800a91218b5e9dffe83f98 -->
# Kompilering

Om du inte gör något annat körs typelisp-program i tolken. Dessutom finns två sätt att kompilera till
nativ kod och ett sätt att spara en miljö. Detaljerna i specifikationen finns i
[Syntaxreferens kapitel 10](../reference/syntax.md#10-kompilering).

| Metod | Hur | Resultat |
|---|---|---|
| JIT-kompilering | `(compile name)` | En funktion i den pågående sessionen ersätts av nativ kod |
| AOT-kompilering | `typl -c src.typl` eller `(compile-file "src.typl" "out")` | En fristående körbar fil |
| Dump | `(dump "file.typld")` | Sparar definitionerna; `typl --image` startar om från samma miljö |

## 1. Förberedelser

Kompileringen använder LLVM 22. Om du har byggt `typl` enligt [README.md](../../../README.md) behövs
inga ytterligare förberedelser.

Körbara filer som görs med AOT-kompilering länkas mot det statiska biblioteket `libtypelisp_front.a`. Ett
release-bygge av `typl` (inklusive ett som installerats med `cargo install`) bär med sig det här
biblioteket i sig självt, så inga förberedelser behövs. Första gången det kompilerar skriver det ut
biblioteket till `~/.typelisp/lib/<bygg-ID>/` (eller `$TYPELISP_HOME/lib/<bygg-ID>/` om miljövariabeln
`TYPELISP_HOME` är satt) och använder den kopian därefter. `typl --remove-lib` tar bort det (med
`--others` de som skrivits av andra versioner av `typl`; med `--all` alla). Ett debug-bygge av `typl`
använder biblioteket i `target/debug/` i det repositorium det byggdes i. För att använda ett som ligger
någon annanstans anger man dess mapp med `--lib-dir` när `typl` startas (avsnitt 3.2).
På macOS används Xcode Command Line Tools vid länkningen.

## 2. JIT-kompilering

Detta gör en redan definierad funktion till nativ kod på stället.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; härifrån körs anropen med den kompilerade koden
```

- `name` utvärderas inte. Skriv funktionsnamnet som det är (inte som en sträng). För en metod skriver
  man det med typnamnet, som i `(compile point::norm)`.
- Funktioner den anropar kompileras tillsammans med den.
- **Generiska funktioner kan inte kompileras.** En kopia för varje typ görs på varje ställe där den
  används. Kompilera i stället den funktion som anropar den med konkreta typer.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` och `dump` är tolkoperationer, så en
  funktion som anropar dem kan inte kompileras. Ett försök att kompilera den ger ett fel som anger
  varför.

För att titta på resultatet av kompileringen används `disassemble`.

```lisp
(disassemble fib)          ; värdmaskinens maskinkod
(disassemble fib true)     ; LLVM IR
```

## 3. Bygga en körbar fil med AOT-kompilering

### 3.1 Skriva programmet

Som ingångspunkt definierar man en **`main`-funktion som inte tar några argument**.

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

`(main)` sist i filen finns där för att `main` ska anropas när du kör `typl hello.typl`. `compile-file`
hoppar över det avslutande `(main)`, så samma fil fungerar både i tolken och med AOT-kompilering.

### 3.2 Kompilera

Från kommandoraden används `typl -c` (`typl --compile` är detsamma).

```sh
$ typl -c hello.typl            # gör hello
$ typl -c hello.typl -o fib     # döper den körbara filen till fib
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Utan `-o` får den körbara filen namnet på källfilen utan `.typl` och läggs i samma mapp som källfilen.
Om källfilens namn inte slutar på `.typl` krävs `-o`. Med `-c` (`--compile`) kan `--image`
och `--feature` inte anges.

Med `--release` gör `typl` ett **release-bygge**, som tar bort `eprint`/`eprintln` (utdata till standard fel för felsökning; [Utskrift](../reference/functions/printing.md)) tillsammans med deras argument. Det som tas bort typkontrolleras ändå, så ett typfel där får kompileringen att misslyckas. `--release` är ett alternativ för `typl` som helhet: det verkar likadant inte bara för `-c` utan också när man kör en fil, i REPL:en och för `typl test`. En `compile-file` som anropas i en `typl` startad med `--release` gör också ett release-bygge, och `eprint`/`eprintln` tas bort även ur kod som körs med `eval` i den körbara fil som byggts så.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # samma sak skrivet efter -c
```

`-O0`, `-O1`, `-O2` och `-O3` anger **optimeringsnivån**. Standard är `-O0`, som inte optimerar. Från `-O1` gör kompilatorn först enkla optimeringar, som att ersätta en variabel som bara är bunden till en literal med dess värde eller att räkna ut heltalsaritmetik på konstanter i förväg, och lämnar resten åt LLVM:s optimering på nivån med samma namn. På varje nivå blir resultaten av beräkningen desamma. Precis som `--release` är det ett alternativ för `typl` som helhet: utöver den körbara fil som `-c` skapar gäller det koden som kompileras när en fil körs, i REPL:en och i `typl test`, samt en `compile-file` som anropas där. Kod som tolken kör optimeras inte.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # samma sak skrivet efter -c
```

Du kan göra samma sak genom att anropa `compile-file` från REPL eller från ett program.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
fib(25) = 75025
```

Om du bygger upprepade gånger kan du lägga den här enda raden i en fil och köra den med
`typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Filnamn löses upp från **den aktuella katalogen där `typl` startades**, inte från platsen för
`build.typl`.

För att länka ett `libtypelisp_front.a` som ligger någon annanstans än där `typl` letar anger man dess
mapp med `--lib-dir`. Det gäller både `typl -c` och `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Om den angivna mappen inte har något `libtypelisp_front.a` stoppar `typl` med ett fel. Filen fungerar
bara med den `typl` som byggdes tillsammans med den. Efter att ha byggt om `typl`, kopiera den igen.

### 3.3 Vad en AOT-kompilerad fil får innehålla

- Toppnivån i ingångsfilen får bara innehålla definitioner (`defun` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) samt `use` och `module`. Toppnivåuttryck som
  `(println ...)` är inte tillåtna, med undantag för det avslutande `(main)`. Lägg arbetet inuti
  `main`.
- Utan en `main` som inte tar några argument misslyckas kompileringen med ett fel.
- Filerna för moduler som fått `use` kompileras också och förenas i en enda körbar fil.
- Bibliotek som namnges med `:library` i `defffi` länkas automatiskt ([C FFI](ffi.md)).
- Varje funktion i standardbiblioteket kan användas med AOT-kompilering. `eval` kan också användas,
  men då hamnar typkontrollen och tolken i den körbara filen, vilket gör den större och långsammare
  att starta. Program som inte anropar `eval` innehåller dem inte.

### 3.4 Hur den körbara filen beter sig

- `(command-line-args)` returnerar en `Vector<string>` med samma form oavsett om den körs som
  `typl hello.typl a b` eller som `./hello a b`. Det första elementet är programnamnet.
- Ange slutkoden med `(exit n)`. Om `main` returnerar normalt är den 0.
- Vid `panic` skriver programmet ut meddelandet och avslutar med en slutkod som inte är noll.

### 3.5 Resurser som den körbara filen använder

`typl -c` har flaggor som anger vilka resurser den körbara filen får använda medan den kör.

```sh
$ typl --heap-cells 100000 -c hello.typl    # lås cons-arenan vid 100000 celler
$ typl -c hello.typl --max-memory 64M       # allokera högst 64 MiB
$ typl -c hello.typl --no-cons              # använd inga cons-celler
```

- `--heap-cells N` låser kapaciteten för arenan av cons-celler vid `N` celler. Arenan växer inte förbi den: kan skräpsamlingen inte frigöra någon cell stannar programmet med ett körtidsfel. Utan flaggan börjar arenan med 65536 celler och växer efter behov.
- `--max-memory SIZE` begränsar det totala minne den körbara filen allokerar. Allt räknas: cons-arenan, strängar, värden som vektorer och hashtabeller, och minnet som körtidsmiljön själv använder. `SIZE` är ett antal byte och kan följas av `K`, `M` eller `G` (1024, 1024² och 1024³ gånger). En allokering som skulle gå över gränsen rapporteras, och programmet avslutas med slutkod 1. Detta avslut kan inte fångas med `catch`. Utan flaggan finns ingen gräns.
- `--no-cons` bygger en körbar fil som inte använder några cons-celler. Den har ingen cons-arena, så ett program som gör något som kräver cons-celler är ett **kompileringsfel**, som visar operationen och kedjan av funktioner från `main` som når den. Cons-celler behövs för att bygga en `Sexpr`-lista (även en citerad lista och en som byggs med kvasicitering) eller ett `&rest`-argument, och av `read`, `eval` och `macroexpand`. Operationer på tal, strängar, vektorer, hashtabeller, structar, kanaler och liknande, och `print`/`println`/`format`, använder inga. Kan inte anges tillsammans med `--heap-cells` och skrivs efter `-c`.

`--max-memory` skrivs efter `-c`. `--heap-cells` kan skrivas före eller efter `-c` och är då kapaciteten för den körbara filens cons-arena. Cons-arenan räknas in i minnesgränsen, så med en gräns som arenan inte ryms i stannar programmet innan det börjar.

### 3.6 Kompilera för WebAssembly

Med `--target wasm32-wasip1` gör `typl -c` en WebAssembly-modul (ett WASI preview 1-kommando) i stället för en körbar fil. Utan `-o` får den källans namn med `.typl` utbytt mot `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # gör hello.wasm
$ wasmtime hello.wasm a b                        # kör den med en miljö som stöder WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- En `typl` installerad med `install.sh` kan användas som den är. En som installeras med `cargo install` kräver förberedelse (som också täcker avsnitt 3.7). Kör först `rustup target add wasm32-wasip1 wasm32-unknown-unknown` med rustup. Installera den sedan med `--features wasm` (`cargo install --locked --path . --features wasm`).
  En `typl` installerad utan det saknar WebAssembly-körtid, och `--target` ger ett fel. För en debug-version av `typl` i repot kör du i stället `scripts/build-wasm-runtime.sh` i repot.
- Länkningen använder rustup-verktygskedjans länkare (`rust-lld`), så rustup behövs också vid kompilering.
- Modulen beter sig som den körbara filen: standard ut, standard fel och slutkod, `(command-line-args)`,
  läsa och skriva filer, `throw`/`catch`/`panic`, uppgifter, kanaler och `sleep`, och `eval` fungerar.
  Uppgifterna körs efter varandra i en enda tråd. Filer kan bara läsas och skrivas i de kataloger
  som miljön visar för modulen (i wasmtime anges de med `--dir`).
- Ett program som använder något WebAssembly saknar är ett **kompileringsfel**. Felet visar vad som användes och
  kedjan av funktioner från `main` som når dit. Det som inte kan användas: socketar (TCP, UDP, Unix-domän,
  TLS, namnuppslagning), OS-trådar (`Thread`), C-funktioner (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  och `ed`.
- WASI har inget sätt att ta reda på tidszonen, så den lokala tiden är okänd. `decode-universal-time` och `encode-universal-time` utan `zone`, samt `get-decoded-time`, ger panic vid körning. De fungerar när `zone` anges.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` och `--release` fungerar som för en körbar fil.

### 3.7 Köra i en webbläsare eller i Node.js

Med `--target wasm32-unknown-unknown` gör `typl -c` en WebAssembly-modul som laddas och körs från JavaScript. Bredvid modulen (`hello.wasm`), i samma mapp, skrivs det JavaScript som laddar och kör den (`hello.mjs`). Förberedelsen är densamma som i avsnitt 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # gör hello.wasm och hello.mjs
$ node hello.mjs                                       # kör den med Node.js
args: #()
fib(25) = 75025
```

Från annat JavaScript anropar du `run` i `hello.mjs`. `write` får utdata som text (`fd` är 1 för standard ut och 2 för standard fel). Värdet av `run` är slutkoden.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* visa den på sidan, till exempel */ } });
```

- Utan `write` skriver Node.js till standard ut och standard fel, och en webbläsare till konsolen, en rad i taget.
- I en webbläsare ska `hello.wasm` serveras över HTTP från samma ställe som `hello.mjs` (den kan inte laddas från `file://`).
- `sleep` väntar med `Atomics.wait`. En webbläsare tillåter det bara i en Worker på en cross-origin-isolerad sida, så kör ett program som använder `sleep` i en Worker.
- Lokal tid (`decode-universal-time` utan `zone` och så vidare) beräknas med den tidszon som JavaScripts `Date` rapporterar.
- Utöver det som inte kan användas i avsnitt 3.6 är fil- och katalogoperationer, miljövariabler, `(command-line-args)`, hemkatalogen och `dribble` ett **kompileringsfel**. Standardindata läses som tom.

### 3.8 Kompilera för en mikrokontroller (Cortex-M)

Med `--target thumbv7em-none-eabihf` skapar `typl -c` en körbar fil (ELF) för ett Cortex-M4F- eller M7-kort utan operativsystem. Utan `-o` får den källans namn med `.typl` utbytt mot `.elf`. Med `--target thumbv8m.main-none-eabihf` är den för ett Cortex-M33-kort.

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # skapar hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # kör den på QEMU:s Cortex-M4
$ typl -c hello.typl --target thumbv8m.main-none-eabihf
$ qemu-system-arm -M mps2-an505 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # kör den på QEMU:s Cortex-M33
```

- En `typl` installerad med `install.sh` kan användas som den är. För att installera med `cargo install`, kör `rustup target add thumbv7em-none-eabihf thumbv8m.main-none-eabihf` och installera sedan med `--features embedded` (`--features wasm,embedded` för båda). Med en debugbygge av `typl` i förrådet, kör `scripts/build-embedded-runtime.sh`. Länkningen använder länkaren i rustup-verktygskedjan.
- Ange kortets minne med `--linker-script FILE`. Den skrivs som `cortex-m-rt`:s `memory.x`: adresser och storlekar för FLASH och RAM. En rad som `_typelisp_stack_size = 32K;` anger hur mycket av RAM som stacken tar (64KiB om inget anges); resten av RAM är heapen. Utan flaggan används layouten för QEMU:s `mps2-an386` (`mps2-an505` för en Cortex-M33).

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

- `--board-functions` väljer vem som ger utdata, indata, avslut och klockan (kortets funktioner).
  - `semihosting` (standard): via semihosting, för körning under QEMU (`-semihosting-config enable=on`) eller en debugger.
  - `c`: kortet ger dem som C-funktioner. Skicka med `--link FILE` ett objekt eller statiskt bibliotek som definierar funktionerna nedan (`--link` kan upprepas; ett leverantörsbibliotek som `defffi` anropar skickas på samma sätt).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 är standardutdata, 2 standardfel
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0; lästa byte, 0 vid slutet
  void     typelisp_host_exit(int32_t code);                              // återvänder inte
  uint64_t typelisp_host_now_us(void);                                    // mikrosekunder på en klocka som aldrig går bakåt
  int64_t  typelisp_host_unix_us(void);                                   // mikrosekunder sedan 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // väntar us mikrosekunder
  ```

- Ett korts register (kringutrustning mappad i minnet) läses med `volatile-read-u8`, `volatile-read-u16` och `volatile-read-u32` och skrivs med `volatile-write-u8`, `volatile-write-u16` och `volatile-write-u32`. Adressen är en `int` eller ett heltal av fast bredd som `u16`, och det lästa värdet har breddens typ (`u8` och så vidare). Var och en läser eller skriver exakt en gång, där den står; kompilatorn varken utelämnar eller flyttar den. Alla kan bara användas inuti `unsafe`. En adress som är negativ, eller inte en multipel av bredden, är ett körtidsfel innan något läses eller skrivs.

  ```lisp
  ;; UART 0 på QEMU:s mps2-an386: data 0x40004000, status 0x40004004, styrning 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; vänta medan den sänder
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; slå på sändning
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" och en radbrytning
  ```

- Skriv funktionen som anropas när ett avbrott kommer som en `defun` med attributet `#[interrupt(N)]` (kringutrustningsavbrottet `N`) eller ett som `#[exception(SysTick)]`. Slå på ett avbrott med `(enable-interrupt N)`. En hanterare körs genast; bara när den kommer mitt i körtidsmiljöns arbete körs den efter att arbetet är klart. En hanterare kan inte vänta (`sleep` är ett kompileringsfel). Detaljerna finns i avsnitt 3.17 i [syntaxreferensen](../reference/syntax.md).

  ```lisp
  (use std::board::enable-interrupt)

  #[interrupt(5)]
  (defun on-five () ()
    (println "interrupt 5"))

  (defun main () ()
    (enable-interrupt 5)
    (unsafe (volatile-write-u32 #xE000E200 (as u32 32))))   ; utlös avbrott 5 själv (NVIC:s register för väntande avbrott)
  ```

- Körmiljön startar programmet (vektortabellen och förberedelsen av RAM) och allokerar minne. Ett HardFault eller ett obehandlat avbrott avslutar programmet med en panics slutkod (1).
- `throw`/`catch`/`panic`, bignums, bråk och flyttal samt uppgifter, kanaler och `sleep` beter sig som i den körbara filen. Uppgifter körs i tur och ordning på den enda kärnan. C-funktioner (`defffi`) kan användas.
- Ett program som använder det kortet inte har är ett **kompileringsfel**: utöver det som inte kan användas i avsnitt 3.7, `eval` och `macroexpand`. Lokal tid är okänd (som i avsnitt 3.6).
- Cons-arenan börjar med 1024 celler och växer vid behov. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` och `--release` fungerar som de är.

### 3.9 Kompilera för Arduino Uno (AVR)

Med `--target avr-atmega328p` skapas en körbar fil (ELF) för mikrokontrollern i Arduino Uno (ATmega328P). Följande program får kortets lysdiod att blinka.

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
  (unsafe (volatile-write-u8 DDRB LED))           ; gör till utgång
  (while true
    (unsafe (volatile-write-u8 PINB LED))         ; växla
    (wait-a-bit)))
```

```sh
$ typl -c blink.typl --target avr-atmega328p -O2   # skapar blink.elf
$ qemu-system-avr -M uno -display none -serial stdio -bios blink.elf   # kör i QEMU
```

- Det finns ingen runtime, så ett program som använder den ger ett **kompileringsfel**. Felet anger vad som behövs och genom vilka funktioner det nås. Det som kan användas är beräkningar med heltal av fast bredd (`i8` till `u32`), `f32`, `f64`, `bool` och `char`, funktionsanrop (även rekursiva), villkor, loopar, `defvar` och `defconstant` med sådana värden, `volatile-read-u8` med flera samt anrop av C-funktioner.
- Det som inte kan användas är värden i heapen, till exempel strängar, listor och strukturer; `catch` och anrop av funktionsvärden; anrop av C-funktioner som tar en sträng eller en funktion; matematiska funktioner av flyttal som `sin` och `expt` (`sqrt` och avrundning som `floor` kan användas).
- En avbrottshanterare skrivs `#[interrupt(N)]` ([Syntax](../reference/syntax.md), avsnitt 3.17). `N` är avbrottets nummer på ATmega328P, 1 till 25 med återställning räknad som 0 (spill i timer 1 är 13). Se exemplet nedan.
- Division med 0 stoppar mikrokontrollern på stället (inga fler avbrott kommer heller). Detsamma gäller `/` av det minsta värdet med tecken med `-1`.
- En `int` kan inte användas (det ger ett kompileringsfel), eftersom den saknar fast bredd. Använd en typ av fast bredd, som `i16` eller `u16`. Även adresser kan beräknas med heltal av fast bredd. Det som bestäms vid kompileringen kan användas, som en adress skriven som ett tal och en `int` i en `defconstant`.
- Om stacken tar slut vid rekursion stannar mikrokontrollern.
- Deklarera en C-funktion med `defffi` och skicka med en objektfil kompilerad för AVR med `--link FILE`.
- En loop som inte gör något tas bort från och med `-O1`. Läs till exempel ett register i en vänteloop.
- När `main` är klar stannar mikrokontrollern.
- Minnets placering ändras med `--linker-script FILE`. Utan den är det 32 KiB flash minus de sista 512 byten, som är startladdarens, och 2 KiB RAM. RAM-adresser skrivs med `0x800000` adderat, till exempel `0x800100`.
- Beräkningar med `f32` och `f64` görs av hjälpfunktioner. De som används hamnar i flashminnet (de för `f64` är större).
- Länkningen använder länkaren i rustups verktygskedja, som i avsnitt 3.8.

Ett exempel som blinkar lysdioden med ett timeravbrott. Ett avbrott slås på i enhetens eget register.

```lisp
(defconstant (PINB int) #x23)
(defconstant (DDRB int) #x24)
(defconstant (LED u8) 32)
(defconstant (TIMSK1 int) #x6F)
(defconstant (TCCR1B int) #x81)
(defvar (ticks u16) 0)

#[interrupt(13)]                                  ; spill i timer 1
(defun on-overflow () ()
  (unsafe (volatile-write-u8 PINB LED))           ; växla
  (setf ticks (+ ticks 1))
  ())

(defun main () ()
  (unsafe (volatile-write-u8 DDRB LED)
          (volatile-write-u8 TIMSK1 1)            ; slå på spillavbrottet
          (volatile-write-u8 TCCR1B 3))           ; räkna med 1/64 av klockan
  (while true))
```

- Ett program med en hanterare släpper fram avbrott så snart de globala variablerna är initierade. Ställ in enheterna i början av `main`.
- `enable-interrupt` och `#[exception(NAME)]` kan inte användas (kompileringsfel).
- Medan en hanterare körs väntar andra avbrott. Som alla andra funktioner kan en hanterare inte använda körtidsmiljön.
- En enskild läsning eller skrivning av en global variabel avbryts inte av en hanterare, inte ens för värden på två byte eller mer. För detta stängs avbrott av när en variabel som en hanterare skriver läses och när en variabel som en hanterare läser eller skriver skrivs, och en varning vid kompileringen anger variabeln. Mellan läsningen och skrivningen i en uppdatering (`(setf n (+ n 1))`) kan ett avbrott komma. För att uppdatera en variabel som en hanterare också skriver, läs och spara statusregistret (adress `#x5F`), skriv 0 i det för att stoppa avbrott och skriv sedan tillbaka det sparade värdet.
- Ett avbrott utan hanterare stoppar mikrokontrollern.

## 4. Dumpar

Du kan spara definitionerna i den aktuella sessionen i en fil och starta från den nästa gång.

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

Det fungerar också för att köra en fil, som i `typl --image session.typld prog.typl`.

- Det som sparas är **definitionerna**. Uttryck som utvärderats i sessionen sparas inte.
- Funktioner du har `compile`at sparas i kompilerad form.
- Globala variabler återställs genom att **köra deras initierare igen**, inte med de värden de hade när
  dumpen skrevs.
- En dump kan inte läsas in av en `typl` av en annan version än den som skrev den (det är ett fel).

Om du kör en fil och gör `(dump ...)` från den ligger filens definitioner i en modul uppkallad efter
filen. En funktion som definierats i `dp.typl` heter `dp::sq`, och för att anropa den från en annan fil
krävs `pub` ([Moduler och filuppdelning](modules.md)).

## 5. Om kompilerade modulfiler

Det finns inget format, som Common Lisps `.fasl`, för att skriva ut det kompilerade resultatet av varje
modul till en fil. `compile-file` bygger den körbara filen direkt från källorna. Inga mellanfiler lämnas
kvar.
