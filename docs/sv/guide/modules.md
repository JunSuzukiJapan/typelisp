<!-- translated-from: docs/ja/guide/modules.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Moduler och filuppdelning

Den här guiden förklarar hur man sätter ihop ett program som består av flera filer. De detaljerade
reglerna finns i avsnitt 3.10 till 3.13 i [Syntaxreferensen](../reference/syntax.md#310-module--use--namnrymder).

## 1. En fil är en modul

I typelisp är **en fil en modul i sig**. Filens sökväg relativt källroten är modulens sökväg.

| Fil | Modul |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Det behövs ingen moduldeklaration överst i en fil.

## 2. Sätta upp ett projekt

Lägg en fil med namnet `typelisp.toml` i projektets rot. Den får vara tom.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

För att hålla källorna under `src/` skriver man den här enda raden i `typelisp.toml`:

```toml
src = "src"
```

`typl` letar efter `typelisp.toml` med början i katalogen för filen den kör och går uppåt, och använder
platsen där den hittar den som källrot. Hittas ingen är katalogen för filen som körs roten (i REPL den
aktuella katalogen).

## 3. Göra definitioner publika och använda dem

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; ett fält utan pub kan inte läsas utifrån

(defun square ((n i32)) i32 (* n n))   ; en funktion utan pub kan inte heller anropas utifrån

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

`geometry.typl` läses in på den plats där `(use geometry)` står. Man behöver inte läsa in den i förväg.

### Vad som görs publikt

- Funktioner, structs, enums, globala variabler, makron och metoder är synliga från andra moduler bara
  när de har `pub`. Sätt `pub` precis före definitionen, som i `(pub defun ...)`.
- För structs är **att göra typen publik och att göra fält publika skilda saker**.
  `(pub defstruct point ...)` gör typen synlig, och bara de fält som skrivs som `(pub x i32)` kan läsas
  och skrivas utifrån.
- Att använda ett namn som inte är publikt utifrån ger ett "cannot resolve"-fel som
  `unresolved path: geometry::square`. Det är samma meddelande som för ett felstavat namn, så om
  stavningen är rätt och namnet ändå inte löses upp, misstänk ett saknat `pub`.

Listan över definitioner som kan ta `pub` finns i
[Syntaxreferens 3.13](../reference/syntax.md#313-pub--synlighet).

## 4. Hur man skriver `use`

```lisp
(use geometry)              ; hämta in en modul; skriv geometry::dist2 för att använda den
(use geometry::dist2)       ; hämta in en funktion; använd den med det bara namnet dist2
(use geometry::point)       ; hämta in en typ; skriv point::new, point::origin, och point i typannoteringar
(use a::f b::g)             ; flera kan skrivas tillsammans
```

- **`use` gäller bara för formerna efter det.** Sätt det överst i filen. Att skriva `geometry::dist2`
  ovanför `use` ger `unresolved path`.
- Att skriva hela sökvägen `geometry::dist2` utan att ha gjort `use` på modulen löses inte heller upp.
  Bara `use` gör att en fil läses in.
- Att göra `use` på ett namn vars bara form redan är upptagen ger en varning. När du ändå menar att
  hämta in det, använd `shadowing-import`.
- En modul i en katalog skrivs `(use geo::shapes)`, och därefter refereras den med sin sista del
  (`shapes::...`).

### Anropa trait-metoder

Metoder som implementeras i en `impl` **tillhör typen**, inte modulens funktioner, så de anropas utan
modulnamnet.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, inte core::area
```

Metoder inuti en `impl` är alltid publika, även utan `pub`.

Ett trait i sig kan inte göras publikt för andra moduler. Håll ett traits definition, `impl`erna för det
och koden som använder det via `:dyn` i en enda modul.

## 5. Dela upp namnrymder inom en fil

För att dela upp en namnrymd ytterligare inom en fil används `module`. Den nästlas inuti filens egen
modul.

```lisp
;; inuti main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

För att lägga hela resten av filen i en namnrymd kan man skriva `(in-module util)` i stället för att
omsluta den med parenteser.

## 6. Begränsningar för beroenden

- **Cykler är inte tillåtna.** Om `a.typl` gör `(use b)` och `b.typl` gör `(use a)` blir resultatet
  felet `circular module dependency: a -> b -> a`. Flytta de definitioner båda behöver till en tredje
  modul.
- **Varken typer eller funktioner kan refereras före sin definition**, inte ens inom samma fil. För
  ömsesidigt rekursiva funktioner deklarerar man en av dem först med `defsignature`
  ([Syntaxreferens 3.2](../reference/syntax.md#32-defsignature--framåtdeklarationer)).

## 7. Körordning

Att köra `typl main.typl` går i denna ordning:

1. `main.typl` och varje fil som den gör `use` på läses och typkontrolleras. **Om det finns ett typfel
   någonstans körs ingenting.**
2. Toppnivåuttrycken i moduler som fått `use` körs före toppnivåuttrycken i de moduler som använder
   dem.
3. Toppnivåuttrycken i `main.typl` körs uppifrån och ned.

Om du samlar programmets ingångspunkt i en `main`-funktion och anropar `(main)` sist i filen kan samma
fil också användas för
[AOT-kompilering](compile.md#3-bygga-en-körbar-fil-med-aot-kompilering).

## 8. Hur detta skiljer sig från `load`

`(load "path")` läser, liksom Common Lisps `load`, innehållet i en fil **in i den aktuella namnrymden
som det är**. Det omsluter det inte i en modul, och `pub` spelar ingen roll. Använd det för sådant som
att läsa en inställningsfil eller läsa om en lokal fil i REPL. För att dela upp ett program i delar,
använd `use`.
