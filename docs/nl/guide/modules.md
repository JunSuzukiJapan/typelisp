<!-- translated-from: docs/ja/guide/modules.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
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
  (tag string))                        ; a field without pub cannot be read from outside

(defun square ((n i32)) i32 (* n n))   ; a function without pub cannot be called from outside either

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
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

- Functies, structs, enums, globale variabelen, macro's en methoden zijn alleen zichtbaar vanuit
  andere modules als ze `pub` dragen. Zet `pub` direct vóór de definitie, zoals in `(pub defun ...)`.
- Bij structs zijn **het openbaar maken van het type en het openbaar maken van velden twee aparte
  zaken**. `(pub defstruct point ...)` maakt het type zichtbaar, en alleen de velden die als
  `(pub x i32)` zijn geschreven kunnen van buiten worden gelezen en geschreven.
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
(use a::f b::g)             ; several can be written together
```

- **`use` werkt alleen voor de vormen die erna komen.** Zet het bovenaan het bestand. Schrijf je
  `geometry::dist2` boven de `use`, dan krijg je `unresolved path`.
- Het volledige pad `geometry::dist2` schrijven zonder de module met `use` binnen te halen wordt ook
  niet opgelost. Alleen `use` zorgt ervoor dat een bestand wordt geladen.
- Een naam met `use` binnenhalen waarvan de kale vorm al bezet is, geeft een waarschuwing. Wil je
  hem toch binnenhalen, gebruik dan `shadowing-import`.
- Een module in een map schrijf je als `(use geo::shapes)`, en daarna verwijs je ernaar met het
  laatste deel (`shapes::...`).

### Trait-methoden aanroepen

Methoden die in een `impl` zijn geïmplementeerd **horen bij het type**, niet bij de functies van de
module, en worden dus zonder modulenaam aangeroepen.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, not core::area
```

Methoden binnen een `impl` zijn altijd openbaar, ook zonder `pub`.

Een trait zelf kan niet openbaar worden gemaakt voor andere modules. Houd de definitie van een trait,
de `impl`s ervoor en de code die hem via `:dyn` gebruikt in één enkele module.

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
