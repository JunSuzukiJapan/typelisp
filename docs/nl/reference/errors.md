<!-- translated-from: docs/ja/reference/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Foutmeldingen

Wat de belangrijkste foutmeldingen van `typl` betekenen en hoe je ze oplost.

## 1. Een fout lezen

Fouten worden in deze vorm naar standaardfout geschreven:

```text
error: file:line:column: kind: message
```

De `kind` vertelt wanneer de fout werd gevonden.

| Soort | Wanneer | Betekenis |
|---|---|---|
| `type error` | Voor het uitvoeren (bij het controleren) | Een fout in types of namen. Die vorm wordt niet uitgevoerd |
| (geen soort) | Bij het lezen of controleren | Een syntaxisfout zoals ongebalanceerde haakjes, of een naam die niet kan worden gevonden |
| `panic` | Tijdens het uitvoeren | Een niet-herstelbare fout. Het programma stopt na het uitvoeren van de opruiming van `unwind-protect` |

Regels die met `warning:` beginnen zijn waarschuwingen, en de verwerking gaat door.

`file:line:column` wijst naar de expressie met de fout. Bij een runtimefout die binnen een
standaardbibliotheekfunctie optreedt, wijst het naar de plek waar het programma die functie aanriep.
Sommige fouten hebben geen positie (zoals `error: panic: ...`).

Voorbeeld:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Dit betekent dat de expressie op regel 1, kolom 24 van `main.typl` een `string` was waar een `i32`
werd verwacht.

## 2. Fouten bij het controleren

Fouten die vóór het uitvoeren worden gevonden. De vorm wordt niet uitgevoerd totdat ze zijn opgelost.

### 2.1 Types

| Melding | Betekenis en oplossing |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Een expressie van het type `U` staat waar type `T` nodig is. Er zijn geen impliciete conversies; converteer getallen met `(as T x)`. `int` en `i32` zijn ook verschillende types |
| ``integer literal 300 is out of range for u8 (0..=255)`` | De literal past niet in het type. Wil je hem afkappen, schrijf dan `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Er is geen type met die naam. Definieer een type vóór de eerste vorm die het gebruikt (types hebben geen voorwaartse declaratie). Bedoelde je een typevariabele, schrijf hem dan op een declarerende plek zoals `<foo>` na de functienaam ([Syntaxreferentie 3.6](syntax.md#36-defstruct--structs-door-de-gebruiker-gedefinieerde-types)) |
| ``cannot infer type argument `t` for `vector::new` `` | Een typeargument kan niet worden bepaald. Schrijf het type met `the`, zoals in `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | De `match` handelt niet elke variant af. Voeg takken toe voor de ontbrekende varianten, of een `_`-tak |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | De functie eist een trait die het type dat je doorgaf niet implementeert. Schrijf `(impl Eq pt ...)` ([Standaardtraits](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Een waarde van een type dat de trait niet implementeert werd doorgegeven waar een `:dyn` wordt verwacht. Schrijf de `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Er werd een traitnaam geschreven waar een type hoort. Schrijf `:dyn Error` |
| ``if: (if cond then else)`` | De `if` heeft de verkeerde vorm. `if` vereist een else-tak. Heb je er geen nodig, gebruik dan `when` |

### 2.2 Namen

| Melding | Betekenis en oplossing |
|---|---|
| `no such function: bar` | Er is geen functie of methode met die naam. Controleer de spelling |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Methoden worden gekozen op het type van het eerste argument. Er bestaat een methode met die naam, maar niet voor het type van het eerste argument (hier `int`). Aan het eind van de melding staan de types die de methode hebben |
| `unbound variable: y` | Er is geen variabele met die naam. Controleer de spelling en het bereik van de binding (wordt hij buiten zijn `let` gebruikt?) |
| ``use: unresolved `nosuch` `` | De module die in `use` is genoemd kan niet worden gevonden. Hoe bestandsnamen op modulepaden worden afgebeeld staat in [Syntaxreferentie 3.11](syntax.md#311-bestanden-en-modules-projecten-met-meerdere-bestanden) |
| `unresolved path: c::hidden` | De module bestaat, maar de naam niet, of hij is niet zichtbaar omdat `pub` ontbreekt |
| `circular module dependency: a -> b -> a` | Modules gebruiken elkaar met `use`. Verplaats het gedeelde deel naar een aparte module |
| ``return-from: no enclosing block named `nope` `` | Geen `block` met de aan `return-from` gegeven naam omsluit hem. Het block van een functie kan alleen binnen die functie worden gebruikt |

### 2.3 Aanroepen

| Melding | Betekenis en oplossing |
|---|---|
| `f: expected 1 argument(s), got 2` | Het aantal argumenten komt niet overeen |
| `f: unknown keyword argument :b` | Er werd een keyword-argument doorgegeven dat de functie niet heeft |
| `new: expected 1 field(s), got 2` | Het aantal waarden dat aan een struct-constructor werd doorgegeven komt niet overeen met het aantal velden |
| ``setf: cannot assign to constant `k` `` | Er werd toegewezen aan een naam die met `defconstant` is gedefinieerd. Moet hij kunnen veranderen, gebruik dan `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Een functie die met `defsignature` is gedeclareerd is niet gedefinieerd |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | Geen van de argumenttypes heeft de methode die met `~/name/` wordt aangeroepen ([Formatdirectieven hoofdstuk 5](functions/format.md#5-name)) |

## 3. Leesfouten

| Melding | Betekenis en oplossing |
|---|---|
| `unexpected end of input while reading a list` | Er ontbreekt een sluithaakje. De positie wijst naar waar het lezen eindigde (zoals het einde van het bestand), dus zoek naar het openingshaakje |

## 4. Fouten tijdens het uitvoeren (panic)

| Melding | Betekenis en oplossing |
|---|---|
| `panic: divide by zero` | Deling door nul met gehele getallen of ratio's. Deling door nul met drijvende komma geeft geen panic; het geeft `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` werd op `none` toegepast. Handel het geval `none` af met `match` of `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Een index buiten het bereik. Controleer de lengte met `len`, of gebruik een functie die bij overschrijding `none` teruggeeft (`nth`, `pop` enzovoort) |
| `panic: an integer argument does not fit a fixnum` | Een `int` die niet in 63 bits past werd doorgegeven aan een argument dat een index of een aantal neemt |
| `throw: no enclosing (catch 'oops) for this throw` | Een `throw` werd uitgevoerd zonder omsluitende `catch` met dezelfde tag |
| `panic: <message>` | Het programma riep `(panic "<message>")` aan. Een mislukte `assert` geeft `assertion failed: ...` |

Een `panic` stopt het hele proces, ook wanneer hij binnen een taak optreedt
([Syntaxreferentie 12.4](syntax.md#124-samenspel-met-andere-functies)). Druk fouten waarvan je wilt
herstellen uit met `Result` ([Syntaxreferentie hoofdstuk 9](syntax.md#9-foutafhandelingsbeleid)).

## 5. Waarschuwingen

| Melding | Betekenis |
|---|---|
| ``warning: redefining function `f` `` | Een functie met dezelfde naam werd opnieuw gedefinieerd. De latere definitie wordt van kracht. Dit verschijnt normaal wanneer je in de REPL een definitie corrigeert |
