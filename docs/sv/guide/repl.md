<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# REPL

Startar man `typl` utan argument hamnar man i REPL (det interaktiva läget). Det här dokumentet
beskriver vad REPL kan utöver att evaluera uttryck: kommandona, hjälp när man skriver, att
hänvisa till tidigare resultat, att stoppa en beräkning och att undersöka efter ett fel.

## Starta och avsluta

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Man avslutar med `:quit` (kort `:q`) eller `Ctrl+D`. Inmatningshistoriken sparas i
`~/.typl_history`, så nästa gång kan den hämtas fram med `Ctrl+P` / `Ctrl+N` eller `Ctrl+R`
(sökning i historiken).

Vid start körs följande i tur och ordning före den första prompten. Inget av det skriver ut de
värden det ger.

1. Innehållet i `~/.typlrc`, om filen finns. Med `--no-init` läses den inte.
2. `--eval FORM` och `--load FILE` från kommandoraden, i den ordning de står. Båda kan anges hur
   många gånger som helst.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` och `--no-init` gäller bara när REPL startas. Att ange dem när man kör en fil
är ett fel.

REPL börjar i modulen `user`. Byter man till en annan modul med `(in-module foo)` visar prompten
den modulen.

```
typl> (in-module foo)
typl user::foo>
```

## REPL-kommandon

Ett `:namn` i början av en rad är ett REPL-kommando. `:help` listar dem. Ett `:namn` som inte finns
i listan är inget kommando; det evalueras som ett vanligt nyckelord.

| Kommando | Betydelse |
|---|---|
| `:help` (`:h`, `:?`) | lista kommandona |
| `:type FORM` (`:t`) | skriva ut typen för FORM utan att evaluera den |
| `:describe NAME` (`:d`) | skriva ut vilken sorts definition det är, dess typ, fält eller varianter, metoder, vilka traits den implementerar, var den definierades och dess dokumentationssträng |
| `:apropos TEXT` | söka i alla moduler efter definitioner vars namn innehåller TEXT (skiftläget spelar ingen roll) |
| `:src NAME` | skriva ut källkoden för en definition |
| `:who-calls NAME` | skriva ut de funktioner och metoder som anropar funktionen eller metoden NAME |
| `:inspect FORM` (`:i`) | evaluera FORM och öppna värdet efter dess typ |
| `:backtrace` (`:bt`) | skriva ut de anrop som pågick när förra raden misslyckades, innerst först |
| `:locals [N]` | skriva ut argumenten till anrop N i `:backtrace` (anrop 0 om N utelämnas) |
| `:cd [DIR]` | byta aktuell mapp till DIR (till hemmappen om DIR utelämnas) |
| `:pwd` | skriva ut aktuell mapp |
| `:quit` (`:exit`, `:q`) | avsluta REPL |

NAME kan vara ett namn som `inner` eller en sökväg som `std::seq::reverse` eller `Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Lägger till 1 till x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Lägger till 1 till x.
typl> :src inner
(defun inner ((x int) (s string)) int "Lägger till 1 till x." (+ x 1))
```

`:src` visar en definition som laddats från en fil ur den filen, och en definition som skrivits i
REPL ur den inskrivna texten. För definitioner i standardbiblioteket finns ingen plats i
källkoden sparad, så deras källkod kan inte visas.

`:who-calls` söker i kropparna hos funktionerna i den aktuella sessionen. För en generisk funktion
hittas bara de användningar som har instansierats med konkreta typer.

`:cd` ändrar mappen där `(load ...)` letar, men `use` fortsätter att leta efter filer i den mapp
där REPL startades.

## Öppna ett värde (`:inspect`)

`:inspect` öppnar ett värde efter dess typ. En struct öppnas i sina fält, en enum (även `Option`) i
fälten för sin variant, en `Vector` i sina element och en `HashTable` i sina nycklar och värden.
En siffra går in i den delen, `u` går upp en nivå och `q` avslutar.

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

Listor och `Sexpr` öppnas inte; de skrivs ut som de är.

## Hjälp när man skriver

När man skriver i en terminal finns följande.

- `Tab` kompletterar namn (definitioner och syntax som `if` och `defun`), sökvägar med `::`,
  REPL-kommandon i början av en rad och, inuti en sträng, filnamn.
- Efter namnet på en funktion följt av ett mellanslag visas funktionens typ nedtonad i slutet av
  raden.
- Parentesen som hör ihop med parentesen bredvid markören markeras.
- Trycker man Enter innan en form är klar börjar nästa rad indragen efter antalet öppna parenteser.

## Tidigare resultat (`:*`, `:**`, `:***`)

I REPL är `:*` det senast utskrivna värdet, `:**` det före det och `:***` det före det. Vart och ett
har typen för sitt värde.

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

- En inmatning som inte skrev ut något värde (till exempel en definition) eller som misslyckades
  flyttar inte resultaten framåt.
- Att skriva `:**` eller `:***` innan det finns så många resultat är ett fel.
- Ett quotat `':*` är ett vanligt nyckelord.
- Ett `:*` som skrivits i kroppen på en funktion fortsätter att avse resultatet från när funktionen
  definierades. Common Lisps `*` läser värdet när funktionen anropas, men typelisp bestämmer det vid
  definitionen så att det har en typ.
- Utanför REPL (när en fil körs, och i `~/.typlrc`, `--load` och `--eval`) är `:*` ett vanligt
  nyckelord.

## Stoppa en beräkning (`Ctrl+C`)

Trycker man `Ctrl+C` under en evaluering stoppas beräkningen och man kommer tillbaka till prompten.
Det man definierat tidigare finns kvar. Det fungerar oavsett om beräkningen är en loop i
interpretatorn, en loop i kompilerad kod eller väntan i `sleep` eller `wait`. Uppstädningen i
pågående `unwind-protect` körs.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` medan man skriver kastar bara bort raden man håller på att skriva.

## Undersöka efter ett fel (`:backtrace`, `:locals`)

När en evaluering har stannat med ett fel skriver `:backtrace` (`:bt`) ut de funktionsanrop som
pågick då, innerst först. `:locals N` skriver ut argumenten till anrop N efter deras deklarerade
typer.

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

- För ett anrop från det sista uttrycket i en funktion (ett svansanrop) visas inte den anropande
  funktionen. Det är därför en svansrekursiv loop bara använder utrymmet för ett enda anrop, hur
  många varv den än går.
- Anrop inuti kompilerade funktioner och anonyma funktioner (`lambda`) visas inte.
- `:locals` visar bara argumenten, inte variabler som bundits med `let`.
- När nästa rad evalueras kastas uppgifterna om det förra felet.
