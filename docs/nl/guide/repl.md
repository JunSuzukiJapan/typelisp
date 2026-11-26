<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# De REPL

Wie `typl` zonder argumenten start, komt in de REPL (de interactieve modus). Dit document
beschrijft wat de REPL naast het evalueren van expressies kan: de opdrachten, hulp bij het typen,
verwijzen naar vorige resultaten, een berekening stoppen en onderzoeken na een fout.

## Starten en afsluiten

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Afsluiten gaat met `:quit` (kort `:q`) of `Ctrl+D`. De invoergeschiedenis wordt bewaard in
`~/.typl_history`; bij de volgende start haal je die terug met `Ctrl+P` / `Ctrl+N` of `Ctrl+R`
(zoeken in de geschiedenis).

Bij het starten wordt vóór de eerste prompt het volgende op volgorde uitgevoerd. Geen van deze
stappen toont de waarden die ze opleveren.

1. De inhoud van `~/.typlrc`, als dat bestand bestaat. Met `--no-init` wordt het niet gelezen.
2. `--eval FORM` en `--load FILE` op de opdrachtregel, in de volgorde waarin ze staan. Beide mogen
   zo vaak als je wilt worden opgegeven.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` en `--no-init` zijn alleen voor het starten van de REPL. Ze opgeven bij het
uitvoeren van een bestand is een fout.

De REPL begint in de module `user`. Ga je met `(in-module foo)` naar een andere module, dan toont
de prompt die module.

```
typl> (in-module foo)
typl user::foo>
```

## REPL-opdrachten

Een `:naam` aan het begin van een regel is een REPL-opdracht. `:help` geeft de lijst. Een `:naam`
die niet in de lijst staat is geen opdracht; die wordt geëvalueerd als een gewoon trefwoord.

| Opdracht | Betekenis |
|---|---|
| `:help` (`:h`, `:?`) | de opdrachten opsommen |
| `:type FORM` (`:t`) | het type van FORM tonen zonder FORM te evalueren |
| `:describe NAME` (`:d`) | soort definitie, type, velden of varianten, methoden, geïmplementeerde traits, plaats van de definitie en documentatiestring tonen |
| `:apropos TEXT` | in alle modules de definities zoeken waarvan de naam TEXT bevat (hoofdletters en kleine letters gelden als gelijk) |
| `:src NAME` | de broncode van een definitie tonen |
| `:who-calls NAME` | de functies en methoden tonen die de functie of methode NAME aanroepen |
| `:inspect FORM` (`:i`) | FORM evalueren en de waarde volgens het type openen |
| `:backtrace` (`:bt`) | de aanroepen tonen die liepen toen de vorige regel mislukte, van binnen naar buiten |
| `:locals [N]` | de argumenten van aanroep N van `:backtrace` tonen (aanroep 0 als N ontbreekt) |
| `:cd [DIR]` | de huidige map naar DIR verplaatsen (naar de thuismap als DIR ontbreekt) |
| `:pwd` | de huidige map tonen |
| `:quit` (`:exit`, `:q`) | de REPL afsluiten |

NAME kan een naam zijn zoals `inner`, of een pad zoals `std::seq::reverse` of `Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Telt 1 op bij x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Telt 1 op bij x.
typl> :src inner
(defun inner ((x int) (s string)) int "Telt 1 op bij x." (+ x 1))
```

`:src` toont een definitie die uit een bestand is geladen uit dat bestand, en een definitie die in
de REPL is ingetypt uit de ingetypte tekst. Voor definities in de standaardbibliotheek is geen
plaats in de broncode vastgelegd, dus hun broncode kan niet worden getoond.

`:who-calls` doorzoekt de bodies van de functies in de huidige sessie. Bij een generieke functie
worden alleen de toepassingen gevonden die met concrete typen zijn geïnstantieerd.

`:cd` verandert de map waarin `(load ...)` zoekt, maar `use` blijft bestanden zoeken in de map
waarin de REPL is gestart.

## Een waarde openen (`:inspect`)

`:inspect` opent een waarde volgens het type. Een struct opent in zijn velden, een enum (ook
`Option`) in de velden van zijn variant, een `Vector` in zijn elementen en een `HashTable` in zijn
sleutels en waarden. Een getal gaat dat deel in, `u` gaat een niveau terug en `q` stopt.

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

Lijsten en `Sexpr` worden niet geopend; ze worden getoond zoals ze zijn.

## Hulp bij het typen

Wie in een terminal typt, heeft het volgende ter beschikking.

- `Tab` vult namen aan (definities en syntaxis zoals `if` en `defun`), paden met `::`,
  REPL-opdrachten aan het begin van een regel en, binnen een string, bestandsnamen.
- Na de naam van een functie gevolgd door een spatie verschijnt het type van die functie gedimd aan
  het eind van de regel.
- Het haakje dat hoort bij het haakje naast de cursor wordt gemarkeerd.
- Druk je op Enter voordat een vorm af is, dan begint de volgende regel ingesprongen volgens het
  aantal open haakjes.

## Vorige resultaten (`:*`, `:**`, `:***`)

In de REPL is `:*` de laatst getoonde waarde, `:**` die daarvoor en `:***` die daar weer voor. Elk
heeft het type van zijn waarde.

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

- Invoer die geen waarde toonde (zoals een definitie) of die mislukte, schuift de resultaten niet
  op.
- `:**` of `:***` schrijven voordat er zoveel resultaten zijn, is een fout.
- Een gequote `':*` is een gewoon trefwoord.
- Een `:*` in de body van een functie blijft verwijzen naar het resultaat van het moment waarop de
  functie werd gedefinieerd. De `*` van Common Lisp leest de waarde op het moment van de aanroep,
  maar typelisp legt die bij de definitie vast, zodat hij een type heeft.
- Buiten de REPL (bij het uitvoeren van een bestand en in `~/.typlrc`, `--load` en `--eval`) is
  `:*` een gewoon trefwoord.

## Een berekening stoppen (`Ctrl+C`)

Wie tijdens een evaluatie op `Ctrl+C` drukt, stopt die berekening en keert terug naar de prompt.
Wat eerder is gedefinieerd, blijft bestaan. Dat werkt bij een lus in de interpreter, een lus in
gecompileerde code en tijdens het wachten in `sleep` of `wait`. Het opruimwerk van lopende
`unwind-protect` wordt uitgevoerd.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` tijdens het typen gooit alleen de regel weg die je aan het typen bent.

## Onderzoeken na een fout (`:backtrace`, `:locals`)

Nadat een evaluatie met een fout is gestopt, toont `:backtrace` (`:bt`) de functieaanroepen die op
dat moment liepen, van binnen naar buiten. `:locals N` toont de argumenten van aanroep N volgens
hun gedeclareerde typen.

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

- Bij een aanroep vanuit de laatste expressie van een functie (een staartaanroep) wordt de
  aanroepende functie niet getoond. Daardoor gebruikt een staartrecursieve lus, hoe vaak hij ook
  rondgaat, de ruimte van één aanroep.
- Aanroepen binnen gecompileerde functies en anonieme functies (`lambda`) worden niet getoond.
- `:locals` toont alleen de argumenten, niet de variabelen die met `let` zijn gebonden.
- Bij het evalueren van de volgende regel wordt de vastlegging van de vorige fout weggegooid.
