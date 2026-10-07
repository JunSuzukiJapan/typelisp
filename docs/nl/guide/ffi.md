<!-- translated-from: docs/ja/guide/ffi.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# C-FFI (defffi)

Deze handleiding legt uit hoe je C-functies vanuit typelisp aanroept. De lijst met types die kunnen
worden gedeclareerd en de beperkingen staan in
[Syntaxreferentie 3.3](../reference/syntax.md#33-defffi--c-functies-declareren-ffi).

## 1. Een functie declareren en aanroepen

`defffi` declareert de naam en de types van een C-functie.

```lisp
(defffi (c-getpid "getpid") () i32)            ; the typelisp name and the C symbol name
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; look it up in libm
```

Aanroepen worden in `(unsafe ...)` gewikkeld.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` is nodig omdat de compiler niet kan controleren of de gedeclareerde types overeenkomen met
de echte types aan de C-kant. Door `unsafe` te schrijven neem jij, de schrijver, de verantwoordelijkheid
voor die controle op je. Vergeet je het, dan volgt een fout die dit uitlegt.

## 2. Een veilige wrapper schrijven

Het bedoelde gebruik is om `unsafe` tot één plek te beperken en naar buiten een gewone functie te
tonen.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; the caller needs no unsafe
(str-len "hello")  ; => 5
```

## 3. Hoe types overeenkomen

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Gehele getallen van dezelfde breedte |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (ook `size_t`, `int64_t` enzovoort) |
| `ptr` | Elke pointer (`void *`, `FILE *` enzovoort) |
| `(ptr T)` | Een pointer naar `T` ([paragraaf 7](#7-c-structs)) |

### Strings

- Een `string` die je doorgeeft wordt gekopieerd naar een met NUL afgesloten C-string, die wordt
  vrijgegeven nadat de aanroep is teruggekeerd. Een NUL midden in de string is een fout.
- Het resultaat van een functie die `string` teruggeeft wordt ook gekopieerd. Het geheugen aan de
  C-kant wordt niet vrijgegeven. Neem bij functies die een string teruggeven die de aanroeper moet
  vrijgeven (zoals `strdup`) het resultaat als `ptr` en geef het zelf vrij met `free`.
- Als een functie die als `string`-teruggevend is gedeclareerd NULL teruggeeft, is dat een fout. Neem
  het resultaat van functies die NULL kunnen teruggeven (zoals `getenv`) als `ptr`.

### `c-long` / `c-ulong` / `ptr`

Deze types bestaan alleen om waarden over de grens met C heen te geven en **ondersteunen geen
rekenkunde**. Om er een als typelisp-geheel getal te gebruiken, converteer je het met `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int does not lose any of the 64-bit value
(try-as i32 (unsafe (c-strlen s)))  ; none if it does not fit in an i32
(unsafe (c-malloc 16))              ; integer literals can be passed as they are
```

Een `ptr` is een waarde die aan C-functies moet worden teruggegeven. Vanuit typelisp is er geen
manier om te lezen waarnaar hij wijst.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Deze types kunnen alleen voorkomen als functieargumenten, returnwaarden en lokale variabelen. Ze
kunnen geen structvelden, globale variabelen of typeargumenten van `Vector` en dergelijke zijn.

## 4. Een bibliotheek noemen

Zonder `:library` wordt het symbool gezocht in wat al in het proces is gelinkt (libc enzovoort).
Functies uit andere bibliotheken hebben `:library` nodig.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Een korte naam zoals `"sqlite3"` wordt gezocht als `libsqlite3.dylib`, daarna als `libsqlite3.so`.
- Een naam die `/` bevat wordt als pad behandeld.
- Wordt het gedeclareerde symbool niet gevonden, dan noemt de fout het.

## 5. AOT-compilatie

Programma's die `defffi` gebruiken kunnen zoals ze zijn met
[`compile-file`](compile.md#3-een-uitvoerbaar-bestand-bouwen-met-aot-compilatie) in uitvoerbare
bestanden worden omgezet. Bibliotheken die met `:library` zijn genoemd worden bij het linken
automatisch toegevoegd, dus `compile-file` heeft geen extra argumenten nodig.

## 6. Callbacks

Je kunt een typelisp-functie aan een C-functie doorgeven en laten terugroepen. Schrijf een
functietype tussen de argumenttypes van `defffi`, en zet bij de aanroep op die plek een functienaam
of een `lambda`-expressie.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") returns p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Alleen functies **zonder vrije variabelen** kunnen worden doorgegeven. Functies op het hoogste
  niveau, `lambda`s en lokale `labels`-functies werken allemaal, maar verwijzen naar een lokale
  variabele van een omsluitende scope is een fout tijdens het typechecken. C geeft alleen de
  gedeclareerde argumenten door, dus er is geen manier om vastgelegde variabelen af te leveren. Gebruik
  globale variabelen om toestand te bewaren.
- Een variabele die een functie bevat kan niet worden doorgegeven. Schrijf ter plekke een functienaam
  of een `lambda`-expressie.
- Een `panic` of `throw` binnen de callback bereikt de aanroeper nadat de C-functie is teruggekeerd.
- De callback kan alleen worden aangeroepen terwijl de C-functie die typelisp heeft aangeroepen
  draait. Hij kan niet worden gebruikt vanuit zaken als `atexit` of signal handlers.

## 7. C-structs

Om iets als een array van structs aan een C-functie door te geven, declareer je met `def-c-struct`
een struct met dezelfde indeling als in C, en alloceer je die binnen `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; declared inside a top-level unsafe

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; four items, all zero
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` alloceert `n` waarden van `T` en geeft een `(ptr T)` terug. `(c-ref p i)` is een
  pointer naar het `i`-de element, `p::field` is een veld, en `(c-deref p)` is waar een pointer naar
  een scalar zoals `i32` naar wijst. Ze kunnen allemaal met `setf` worden geschreven.
- `(as ptr p)` maakt er een ongetypeerde `ptr` van om door te geven aan C-functies die een `void *`
  nemen.
- De grootte van `item` (hier 8) en de positie van elk veld worden bepaald door dezelfde regels als
  in C.

### Levensduur van gealloceerd geheugen

Gealloceerd geheugen wordt vrijgegeven wanneer de besturing de buitenste `unsafe` in die functie
verlaat. Hetzelfde gebeurt als hij door `panic` of `throw` wordt verlaten. Daarom kan een
`(ptr T)`-waarde niet buiten de `unsafe` worden meegenomen. Hem de waarde van de `unsafe` maken, hem
in een closure vastleggen, hem aan een `task` doorgeven en hem met `throw` gooien zijn allemaal
typefouten. Kopieer de waarden die je buiten de `unsafe` wilt gebruiken binnen de `unsafe` naar
getallen of een `defstruct`.

Wanneer je binnen een `lambda` of een `labels`-functie alloceert, schrijf dan binnen die functie een
`unsafe`.

### Door C gealloceerd geheugen

Een pointer die je als `(ptr T)` van C ontvangt (een returnwaarde van `defffi`, een
callback-argument enzovoort) is een fout, tenzij hij binnen geheugen wijst dat met `c-alloc` is
gealloceerd. Declareer functies die geheugen ontvangen dat C met `malloc` heeft gealloceerd, of NULL,
met de ongetypeerde `ptr`.

## 8. Wat niet kan

- **Variadische functies** (`printf` en dergelijke) kunnen niet worden gedeclareerd. Het variadische
  deel wordt volgens andere regels doorgegeven dan de vaste argumenten. Declareer voor elk aantal
  argumenten dat je gebruikt een aparte naam.
- **Structs op waarde doorgeven of teruggeven** is niet mogelijk. Gebruik functies die pointers
  doorgeven.
- **Generieke declaraties** zijn niet mogelijk.
- **Dezelfde naam als een ingebouwde functie** kan niet worden gebruikt.
- **Ze kunnen niet als functiewaarden worden doorgegeven.** Je kunt er niet een doorgeven zoals in
  `(map xs c-abs)`; wikkel hem in een `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
