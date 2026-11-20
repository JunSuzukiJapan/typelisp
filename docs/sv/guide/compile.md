<!-- translated-from: docs/ja/guide/compile.md @ eb0e7d31e1d6e61f64d90fcfb4f0dc5e6bfe813f -->
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
  TLS, namnuppslagning), OS-trådar (`Thread`), C-funktioner (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`,
  den lokala tidszonen och `ed`.
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
- Utöver det som inte kan användas i avsnitt 3.6 är fil- och katalogoperationer ett **kompileringsfel**. Standard in läses som tom, och `(command-line-args)` är en tom vektor.

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
