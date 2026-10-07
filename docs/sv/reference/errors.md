<!-- translated-from: docs/ja/reference/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Felmeddelanden

Vad de viktigaste felmeddelandena från `typl` betyder och hur man åtgärdar dem.

## 1. Läsa ett fel

Fel skrivs till standard fel i den här formen:

```text
error: file:line:column: kind: message
```

`kind` talar om när felet hittades.

| Slag | När | Betydelse |
|---|---|---|
| `type error` | Före körning (vid kontrollen) | Ett fel i typer eller namn. Den formen körs inte |
| (inget slag) | Vid läsning eller kontroll | Ett syntaxfel som obalanserade parenteser, eller ett namn som inte hittas |
| `panic` | Under körning | Ett icke återhämtningsbart misslyckande. Programmet stoppar efter att ha kört uppstädningen i `unwind-protect` |

Rader som börjar med `warning:` är varningar, och bearbetningen fortsätter.

`file:line:column` pekar på uttrycket med felet. För ett fel vid körning som uppstår inuti en funktion i
standardbiblioteket pekar det på stället där programmet anropade den funktionen. Vissa fel har ingen
position (som `error: panic: ...`).

Exempel:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Det betyder att uttrycket på rad 1, kolumn 24 i `main.typl` var en `string` där en `i32` förväntades.

## 2. Fel vid kontrollen

Fel som hittas före körning. Formen körs inte förrän de är åtgärdade.

### 2.1 Typer

| Meddelande | Betydelse och åtgärd |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Ett uttryck av typen `U` står där typen `T` behövs. Det finns inga implicita konverteringar; för tal konverterar man med `(as T x)`. `int` och `i32` är också olika typer |
| ``integer literal 300 is out of range for u8 (0..=255)`` | Literalen ryms inte i typen. Om du vill att den ska kapas, skriv `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Det finns ingen typ med det namnet. Definiera en typ före den första form som använder den (typer har ingen framåtdeklaration). Om du menade en typvariabel, skriv den på en deklarerande plats som `<foo>` efter funktionsnamnet ([Syntaxreferens 3.6](syntax.md#36-defstruct--structs-användardefinierade-typer)) |
| ``cannot infer type argument `t` for `vector::new` `` | Ett typargument kan inte avgöras. Skriv typen med `the`, som i `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | `match` hanterar inte alla varianter. Lägg till grenar för de saknade varianterna, eller en `_`-gren |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | Funktionen kräver ett trait som typen du skickade inte implementerar. Skriv `(impl Eq pt ...)` ([Standardtraits](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Ett värde av en typ som inte implementerar traitet skickades där en `:dyn` förväntas. Skriv `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Ett traitnamn skrevs där en typ ska stå. Skriv `:dyn Error` |
| ``if: (if cond then else)`` | `if` har fel form. `if` kräver en else-gren. När du inte behöver någon, använd `when` |

### 2.2 Namn

| Meddelande | Betydelse och åtgärd |
|---|---|
| `no such function: bar` | Det finns ingen funktion eller metod med det namnet. Kontrollera stavningen |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Metoder väljs efter typen på det första argumentet. En metod med det namnet finns, men inte för det första argumentets typ (här `int`). Slutet av meddelandet listar de typer som har metoden |
| `unbound variable: y` | Det finns ingen variabel med det namnet. Kontrollera stavningen och bindningens räckvidd (används den utanför sitt `let`?) |
| ``use: unresolved `nosuch` `` | Modulen som anges i `use` hittas inte. För hur filnamn motsvarar modulsökvägar, se [Syntaxreferens 3.11](syntax.md#311-filer-och-moduler-projekt-med-flera-filer) |
| `unresolved path: c::hidden` | Modulen finns, men inte namnet, eller så är det inte synligt eftersom det saknar `pub` |
| `circular module dependency: a -> b -> a` | Moduler gör `use` på varandra. Flytta den gemensamma delen till en separat modul |
| ``return-from: no enclosing block named `nope` `` | Inget `block` med det namn som getts till `return-from` omsluter det. En funktions block kan bara användas inuti den funktionen |

### 2.3 Anrop

| Meddelande | Betydelse och åtgärd |
|---|---|
| `f: expected 1 argument(s), got 2` | Antalet argument stämmer inte |
| `f: unknown keyword argument :b` | Ett nyckelordsargument som funktionen inte har skickades |
| `new: expected 1 field(s), got 2` | Antalet värden som skickats till en structs konstruktor stämmer inte med antalet fält |
| ``setf: cannot assign to constant `k` `` | Ett namn som definierats med `defconstant` tilldelades. Om det behöver ändras, använd `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | En funktion som deklarerats med `defsignature` är inte definierad |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | Ingen av argumenttyperna har metoden som anropas med `~/name/` ([Formatdirektiv kapitel 5](functions/format.md#5-name)) |

## 3. Läsfel

| Meddelande | Betydelse och åtgärd |
|---|---|
| `unexpected end of input while reading a list` | En avslutande parentes saknas. Positionen pekar på där läsningen tog slut (som filslutet), så leta efter den inledande parentesen |

## 4. Fel vid körning (panic)

| Meddelande | Betydelse och åtgärd |
|---|---|
| `panic: divide by zero` | Division med noll med heltal eller kvoter (ratio). Flyttalsdivision med noll ger inte panic; den ger `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` tillämpades på `none`. Hantera `none`-fallet med `match` eller `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Ett index utanför intervallet. Kontrollera längden med `len`, eller använd en funktion som returnerar `none` när det är utanför intervallet (`nth`, `pop` och så vidare) |
| `panic: an integer argument does not fit a fixnum` | Ett `int` som inte ryms i 63 bitar skickades till ett argument som tar ett index eller ett antal |
| `throw: no enclosing (catch 'oops) for this throw` | Ett `throw` kördes utan något omslutande `catch` med samma tagg |
| `panic: <message>` | Programmet anropade `(panic "<message>")`. Ett misslyckat `assert` ger `assertion failed: ...` |

En `panic` stoppar hela processen även när den sker inuti en task
([Syntaxreferens 12.4](syntax.md#124-samspel-med-andra-funktioner)). Uttryck misslyckanden du vill
återhämta dig från med `Result` ([Syntaxreferens kapitel 9](syntax.md#9-policy-för-felhantering)).

## 5. Varningar

| Meddelande | Betydelse |
|---|---|
| ``warning: redefining function `f` `` | En funktion med samma namn definierades igen. Den senare definitionen gäller. Den visas normalt när du rättar en definition i REPL |
