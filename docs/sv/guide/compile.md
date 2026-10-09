<!-- translated-from: docs/ja/guide/compile.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
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
Om källfilens namn inte slutar på `.typl` krävs `-o`. Med `-c` (`--compile`) kan `--image`,
`--heap-cells` och `--feature` inte anges.

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

- Toppnivån i ingångsfilen får bara innehålla definitioner (`defun` `defmethod` `defvar`
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
