<!-- translated-from: docs/ja/reference/errors.md @ d179fbc5112a7c56e11d7cee68e15bbebfafb631 -->
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
| ``cannot infer type argument `t` for `Vector::new` `` | Een typeargument kan niet worden bepaald. Schrijf het type met `the`, zoals in `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | De `match` handelt niet elke variant af. Voeg takken toe voor de ontbrekende varianten, of een `_`-tak |
| ``type `pt` does not implement trait `eq` required by `:where` clause on type parameter `a` `` | De functie eist een trait die het type dat je doorgaf niet implementeert. Schrijf `(impl Eq pt ...)` ([Standaardtraits](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Een waarde van een type dat de trait niet implementeert werd doorgegeven waar een `:dyn` wordt verwacht. Schrijf de `impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Er werd een traitnaam geschreven waar een type hoort. Schrijf `:dyn Error` |
| ``if: (if cond then else)`` | De `if` heeft de verkeerde vorm. `if` vereist een else-tak. Heb je er geen nodig, gebruik dan `when` |

### 2.2 Namen

| Melding | Betekenis en oplossing |
|---|---|
| `no such function: bar` | Er is geen functie of methode met die naam. Controleer de spelling |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Methoden worden gekozen op het type van het eerste argument. Er bestaat een methode met die naam, maar niet voor het type van het eerste argument (hier `int`). Aan het eind van de melding staan de types die de methode hebben (bij een methode die voor één instantiatie is geschreven: die instantiatie, zoals `box<int>`) |
| ``no method `twice` for type `box<string>` (the first parameter of the function this value is expected to be, which selects the method); `twice` is a method of `box<int>` `` | Een methode die als waarde wordt doorgegeven, wordt gekozen op het type van de eerste parameter van het functietype waarin ze wordt doorgegeven. Dat type (hier `box<string>`) heeft geen methode met die naam. Het einde van de melding somt de types op die haar hebben |
| `unbound variable: y` | Er is geen variabele met die naam. Controleer de spelling en het bereik van de binding (wordt hij buiten zijn `let` gebruikt?) |
| ``cannot define variable `f`: a function of that name already exists here — a name has one meaning, whether a function, a variable or a macro`` | Twee van een functie, een globale variabele en een macro kregen in één module dezelfde naam. Hernoem er één ([Syntaxreferentie 3.10](syntax.md#310-module--use--namespaces)) |
| ``` `point` has no static method `new`; a struct's methods are written in its `impl`, ...``` | Voor een struct wordt geen `new` gegenereerd. Maak de waarde met `make-instance` of schrijf een `new` in `impl` ([Syntaxreferentie 3.5](syntax.md#35-impl--methoden-van-een-type)) |
| ``defmethod is gone — write methods in the type's `impl`: ...`` | `defmethod` bestaat niet. Methoden worden in de `impl` van het type geschreven ([Syntaxreferentie 3.5](syntax.md#35-impl--methoden-van-een-type)) |
| ``defstruct: (:constructor ...) is gone — ...`` | De optie `:constructor` bestaat niet. De constructor wordt als statische methode in `impl` geschreven |
| ``method box::m: `box` has a `m` for every instantiation already, so an `impl` of `box<int>` cannot define its own ...`` | Een methode met één naam is zowel in een `impl` met typeparameters als variabelen (`Box<T>`) als in de `impl` van een instantiatie (`Box<int>`) geschreven, in welke volgorde ook. Houd er maar één over ([Syntaxreferentie 3.5](syntax.md#35-impl--methoden-van-een-type)) |
| ``impl show box<int>: `box` implements `show` for every instantiation already, ...`` | Dezelfde trait is voor `Box<T>` en voor `Box<int>` geïmplementeerd. Houd er maar één over |
| ``use: unresolved `nosuch` `` | De module die in `use` is genoemd kan niet worden gevonden. Hoe bestandsnamen op modulepaden worden afgebeeld staat in [Syntaxreferentie 3.11](syntax.md#311-bestanden-en-modules-projecten-met-meerdere-bestanden) |
| `unresolved path: c::hidden` | De module bestaat, maar de naam niet, of hij is niet zichtbaar omdat `pub` ontbreekt |
| `circular module dependency: a -> b -> a` | Modules gebruiken elkaar met `use`. Verplaats het gedeelde deel naar een aparte module |
| ``return-from: no enclosing block named `nope` `` | Geen `block` met de aan `return-from` gegeven naam omsluit hem. Het block van een functie kan alleen binnen die functie worden gebruikt |

### 2.3 Aanroepen

| Melding | Betekenis en oplossing |
|---|---|
| `f: expected 1 argument(s), got 2` | Het aantal argumenten komt niet overeen |
| `f: unknown keyword argument :b` | Er werd een keyword-argument doorgegeven dat de functie niet heeft |
| `circle: expected 1 field(s), got 2` | Het aantal waarden dat aan een enumvariant is gegeven, klopt niet met het aantal velden van die variant |
| ``make-instance point: slot `y` is not given and has no default`` | Een veld zonder standaardwaarde is niet aan `make-instance` gegeven |
| ``make-instance point: no slot `z` (its slots are `x`, `y`)`` | Er is een veld gegeven dat de struct niet heeft |
| ``make-instance geo::pt: slot `secret` is not `pub`, so it can be given only inside module `geo` `` | Een veld dat niet `pub` is, is gegeven buiten de module die het type definieert. Laat die module een `pub`-constructor aanbieden |
| ``` `box::make` is written for `box<int>`, `box<string>`; the type the call is expected to return picks one, ...``` | De `impl`s van meerdere instantiaties hebben een statische methode met die naam en niets bepaalt welke wordt aangeroepen. Geef het verwachte type op, zoals in `(the Box<int> (Box::make))` |
| ``compile: `box::twice` is written for more than one instantiation of `box`; name one: ...`` | Een per instantiatie geschreven methode is alleen met de naam van het type aan `compile`, `trace`, `documentation` en dergelijke gegeven. Schrijf de instantiatie, zoals in `box<int>::twice` |
| ``setf: cannot assign to constant `k` `` | Er werd toegewezen aan een naam die met `defconstant` is gedefinieerd. Moet hij kunnen veranderen, gebruik dan `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Een functie die met `defsignature` is gedeclareerd is niet gedefinieerd |
| ``format: ~/nosuch/ calls what (nosuch arg colon at) with arg a `string` would: no such function: nosuch`` | De aanroep `(nosuch arg colon at)` die op die plek staat, is niet op te lossen. Definieer een methode van het type van het argument of een functie ([Formatdirectieven hoofdstuk 5](functions/format.md#5-name)) |
| ``format: ~/brief/ can be handed an element of a list argument, whose type is only `Sexpr` — ...`` | Een `~/name/` staat waar hij de elementen van een lijst krijgt, zoals binnen `~{`. Het type van de elementen kan niet bepalen wat er aangeroepen wordt |

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
