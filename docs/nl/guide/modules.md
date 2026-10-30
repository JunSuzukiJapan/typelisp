<!-- translated-from: docs/ja/guide/modules.md @ b4f23852eb3f4b11e3fd96c05da870bdd383e84f -->
# Modules en bestandsindeling

Deze handleiding legt uit hoe je een programma samenstelt dat uit meerdere bestanden bestaat. De
gedetailleerde regels staan in de paragrafen 3.10 tot en met 3.13 van de
[Syntaxreferentie](../reference/syntax.md#310-module--use--namespaces).

## 1. Eén bestand is één module

In typelisp **is een bestand zelf een module**. Het pad van het bestand ten opzichte van de bronroot
is het pad van de module.

| Bestand | Module |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Bovenaan een bestand hoeft geen moduledeclaratie te staan.

## 2. Een project opzetten

Zet een bestand met de naam `typelisp.toml` in de root van het project. Het mag leeg zijn.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Wil je de bronnen onder `src/` houden, schrijf dan deze ene regel in `typelisp.toml`:

```toml
src = "src"
```

`typl` zoekt naar `typelisp.toml`, te beginnen bij de map van het bestand dat hij uitvoert en dan
omhoog, en gebruikt de plek waar hij het vindt als bronroot. Wordt er geen gevonden, dan is de map
van het uitgevoerde bestand de root (in de REPL de huidige map).

## 3. Definities openbaar maken en gebruiken

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; een veld zonder pub kan van buiten niet gelezen of geschreven worden

(defun square ((n i32)) i32 (* n n))   ; ook een functie zonder pub kan van buiten niet aangeroepen worden

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(impl point
  (pub new ((x i32) (y i32) (tag string)) point
    (make-instance 'point :x x :y y :tag tag))
  (pub origin () point (point::new 0 0 "o")))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl` wordt geladen op de plek waar `(use geometry)` staat. Je hoeft het niet eerder te
laden.

### Wat openbaar wordt gemaakt

- Functies, structs, enums, traits, globale variabelen en macro's zijn alleen vanuit andere modules zichtbaar als ze
  `pub` dragen. Zet `pub` direct voor de definitie, zoals in `(pub defun ...)`.
- Van de methoden die in een `impl` staan, zijn alleen de items die als `(pub new ...)` zijn geschreven van
  buiten bruikbaar.
- Bij structs **zijn het publiek maken van het type en het publiek maken van de velden twee aparte zaken**.
  `(pub defstruct point ...)` maakt het type zichtbaar, en alleen de velden die als `(pub x i32)` zijn
  geschreven, kunnen van buiten gelezen en geschreven worden.
- Evenzo kunnen in andere modules alleen `pub`-velden aan `make-instance` worden gegeven. De `point`
  hierboven heeft een `tag` die niet `pub` is; andere modules kunnen hem daarom niet met `make-instance`
  maken en gebruiken `point::new`.
- Een naam die niet openbaar is van buitenaf gebruiken geeft een fout "kan niet worden opgelost",
  zoals `unresolved path: geometry::square`. Het is dezelfde melding als bij een verkeerd gespelde
  naam. Klopt de spelling en wordt de naam toch niet opgelost, vermoed dan een ontbrekende `pub`.

De lijst met definities die `pub` kunnen krijgen staat in
[Syntaxreferentie 3.13](../reference/syntax.md#313-pub--zichtbaarheid).

## 4. Hoe je `use` schrijft

```lisp
(use geometry)              ; bring in a module; write geometry::dist2 to use it
(use geometry::dist2)       ; bring in a function; use it by the bare name dist2
(use geometry::point)       ; bring in a type; write point::new, point::origin, and point in type annotations
(use geometry::point::new)  ; bring in a type's static method; use it by the bare name new
(use geometry::shape::*)    ; make all of a type's constructors and public static methods usable by bare names
(use a::f b::g)             ; several can be written together
```

- **`use` werkt alleen voor de vormen die erna komen.** Zet het bovenaan het bestand. Schrijf je
  `geometry::dist2` boven de `use`, dan krijg je `unresolved path`.
- Het volledige pad `geometry::dist2` schrijven zonder de module met `use` binnen te halen wordt ook
  niet opgelost. Alleen `use` zorgt ervoor dat een bestand wordt geladen.
- Een naam met `use` binnenhalen waarvan de kale vorm al bezet is, geeft een waarschuwing. Wil je
  hem toch binnenhalen, gebruik dan `shadowing-import`. Bij constructors en statische methoden van een type is het een fout, geen waarschuwing.
- `(use geometry::point)` haalt alleen de naam van het type binnen. Om `point::new` als `new` te schrijven,
  schrijf je `(use geometry::point::new)` of `(use geometry::point::*)`.
- Een module in een map schrijf je als `(use geo::shapes)`, en daarna verwijs je ernaar met het
  laatste deel (`shapes::...`).

### Een naam van de standaardbibliotheek gebruiken

Een functie of iets anders definiëren onder een naam die de standaardbibliotheek aanbiedt, zoals
`length` of `first`, is een fout.

```
type error: defun: `length` is a name the standard library offers every module; write `(shadow length)` before defining it here
```

Als je besluit dat de module je eigen definitie moet gebruiken, schrijf je `shadow` vóór de
definitie.

```lisp
(shadow first)

(defun first ((a int) (b int)) int a)

(println "~a" (first 10 20))                                          ; first van deze module => 10
(println "~a" (std::seq::first (iter (the Vector<int> #(1 2 3)))))    ; first van de standaardbibliotheek => (some 1)
```

Na `shadow` verwijst `first` in die module naar je eigen definitie. Om de `first` van de
standaardbibliotheek te gebruiken, schrijf je het pad, `std::seq::first`. `shadow` werkt alleen in
de module waar het staat; in andere modules is `first` nog steeds die van de standaardbibliotheek.
De namen van speciale vormen zoals `if` en `let` kunnen niet ge-`shadow`d worden en ook niet voor
definities gebruikt worden.

### Trait-methoden aanroepen

Methoden die in een `impl` zijn geïmplementeerd **horen bij het type**, niet bij de functies van de
module, en worden dus zonder modulenaam aangeroepen.

```lisp
(use shapes::core)
(println "~a" (area (make-instance 'core::square :side 4)))    ; area, not core::area
```

Methoden in de `impl` van een trait zijn altijd publiek, ook zonder `pub`. Van de methoden in een inherente
`impl` (`(impl point ...)`) zijn alleen die met `pub` publiek.

Ook traits zijn alleen vanuit andere modules zichtbaar als ze als `(pub deftrait ...)` zijn geschreven. In
andere modules schrijf je een trait als pad zoals `shapes::Shape` of haal je hem binnen met `use`, en zo
zet je hem in een `impl`.

## 5. Namespaces binnen een bestand opsplitsen

Om een namespace binnen één bestand verder op te splitsen gebruik je `module`. Die wordt genest
binnen de eigen module van het bestand.

```lisp
;; inside main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Om de hele rest van het bestand in één namespace te zetten, kun je `(in-module util)` schrijven in
plaats van alles tussen haakjes te zetten.

## 6. Beperkingen aan afhankelijkheden

- **Cycli zijn niet toegestaan.** Als `a.typl` `(use b)` doet en `b.typl` `(use a)`, is het resultaat
  de fout `circular module dependency: a -> b -> a`. Verplaats de definities die beide nodig hebben
  naar een derde module.
- **Noch types noch functies kunnen worden gebruikt voordat ze zijn gedefinieerd**, ook niet binnen
  hetzelfde bestand. Declareer bij onderling recursieve functies eerst een van beide met
  `defsignature`
  ([Syntaxreferentie 3.2](../reference/syntax.md#32-defsignature--voorwaartse-declaraties)).

## 7. Volgorde van uitvoering

`typl main.typl` uitvoeren verloopt in deze volgorde:

1. `main.typl` en elk bestand dat ermee via `use` wordt gebruikt worden gelezen en getypechecked.
   **Als er ergens een typefout is, wordt er niets uitgevoerd.**
2. De expressies op het hoogste niveau van modules die met `use` zijn binnengehaald, worden
   uitgevoerd vóór die van de modules die ze gebruiken.
3. De expressies op het hoogste niveau van `main.typl` worden van boven naar beneden uitgevoerd.

Als je het beginpunt van het programma in een functie `main` onderbrengt en aan het einde van het
bestand `(main)` aanroept, kan hetzelfde bestand ook voor
[AOT-compilatie](compile.md#3-een-uitvoerbaar-bestand-bouwen-met-aot-compilatie) worden gebruikt.

## 8. Hoe dit verschilt van `load`

`(load "path")` leest, net als `load` in Common Lisp, de inhoud van een bestand **zoals ze is in de
huidige namespace**. Het verpakt ze niet in een module en `pub` speelt geen rol. Gebruik het voor
zaken als het lezen van een instellingenbestand of het opnieuw laden van een lokaal bestand in de
REPL. Gebruik `use` om een programma in delen op te splitsen.
