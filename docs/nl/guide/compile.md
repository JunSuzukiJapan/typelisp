<!-- translated-from: docs/ja/guide/compile.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
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
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Zonder `-o` krijgt het uitvoerbare bestand de naam van het bronbestand zonder `.typl` en wordt het in
dezelfde map als het bronbestand geplaatst. Eindigt de naam van het bronbestand niet op `.typl`, dan
is `-o` verplicht. Bij `-c` (`--compile`) kunnen `--image`, `--heap-cells` en `--feature` niet worden
opgegeven.

Hetzelfde kun je doen door `compile-file` aan te roepen vanuit de REPL of vanuit een programma.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
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

- Het hoogste niveau van het invoerbestand mag alleen definities bevatten (`defun` `defmethod`
  `defvar` `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype`
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
