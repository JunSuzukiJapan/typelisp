<!-- translated-from: docs/ja/guide/ffi.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# C FFI (defffi)

Den här guiden förklarar hur man anropar C-funktioner från typelisp. Listan över typer som kan
deklareras och begränsningarna finns i
[Syntaxreferens 3.3](../reference/syntax.md#33-defffi--deklarera-c-funktioner-ffi).

## 1. Deklarera och anropa en funktion

`defffi` deklarerar namnet och typerna för en C-funktion.

```lisp
(defffi (c-getpid "getpid") () i32)            ; typelisp-namnet och C-symbolens namn
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; slå upp den i libm
```

Anrop omsluts av `(unsafe ...)`.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` behövs eftersom kompilatorn inte kan kontrollera att de deklarerade typerna stämmer med de
verkliga typerna på C-sidan. Att skriva `unsafe` betyder att du, som skriver, tar ansvar för den
kontrollen. Glömmer du det får du ett fel som förklarar detta.

## 2. Skriva ett säkert omslag

Den avsedda användningen är att begränsa `unsafe` till ett enda ställe och visa en vanlig funktion utåt.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; anroparen behöver inget unsafe
(str-len "hello")  ; => 5
```

## 3. Hur typer motsvarar varandra

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Heltal med samma bredd |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (även `size_t`, `int64_t` och så vidare) |
| `ptr` | Vilken pekare som helst (`void *`, `FILE *` och så vidare) |
| `(ptr T)` | En pekare till `T` ([avsnitt 7](#7-c-structs)) |

### Strängar

- En `string` du skickar kopieras till en NUL-terminerad C-sträng, som frigörs när anropet har
  återvänt. En NUL mitt i strängen är ett fel.
- Resultatet av en funktion som returnerar `string` kopieras också. Minnet på C-sidan frigörs inte.
  För funktioner som returnerar en sträng som anroparen måste frigöra (som `strdup`) tar man emot
  resultatet som `ptr` och gör `free` själv.
- Om en funktion som deklarerats returnera `string` returnerar NULL är det ett fel. Ta emot resultatet
  av funktioner som kan returnera NULL (som `getenv`) som `ptr`.

### `c-long` / `c-ulong` / `ptr`

De här typerna finns bara för att skicka värden över gränsen mot C och **stöder ingen aritmetik**. För
att använda en som ett typelisp-heltal konverterar man den med `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int förlorar ingenting av 64-bitsvärdet
(try-as i32 (unsafe (c-strlen s)))  ; none om det inte ryms i ett i32
(unsafe (c-malloc 16))              ; heltalsliteraler kan skickas som de är
```

Ett `ptr` är ett värde som ska lämnas tillbaka till C-funktioner. Det finns inget sätt att läsa vad det
pekar på från typelisp-sidan.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

De här typerna kan bara förekomma som funktionsargument, returvärden och lokala variabler. De kan inte
vara fält i structs, globala variabler eller typargument till `Vector` och liknande.

## 4. Namnge ett bibliotek

Utan `:library` slås symbolen upp i det som redan är länkat in i processen (libc och så vidare).
Funktioner från andra bibliotek behöver `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Ett kort namn som `"sqlite3"` slås upp som `libsqlite3.dylib` och sedan `libsqlite3.so`.
- Ett namn som innehåller `/` behandlas som en sökväg.
- Om den deklarerade symbolen inte hittas nämner felet den vid namn.

## 5. AOT-kompilering

Program som använder `defffi` kan göras till körbara filer med
[`compile-file`](compile.md#3-bygga-en-körbar-fil-med-aot-kompilering) som de är. Bibliotek som
namnges med `:library` läggs till vid länkningen automatiskt, så `compile-file` behöver inga extra
argument.

## 6. Callbacks

Du kan skicka en typelisp-funktion till en C-funktion och låta den anropas tillbaka. Skriv en funktionstyp
bland argumenttyperna i `defffi`, och lägg vid anropet ett funktionsnamn eller ett `lambda`-uttryck på
den positionen.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") returnerar p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Bara funktioner **utan fria variabler** kan skickas. Toppnivåfunktioner, `lambda`s och lokala
  `labels`-funktioner fungerar alla, men att referera till en lokal variabel i en omslutande skopa är
  ett fel vid typkontrollen. C skickar bara de deklarerade argumenten, så det finns inget sätt att
  leverera fångade variabler. För att behålla tillstånd används globala variabler.
- En variabel som håller en funktion kan inte skickas. Skriv ett funktionsnamn eller ett
  `lambda`-uttryck på stället.
- En `panic` eller `throw` inuti callbacken når anroparen efter att C-funktionen har återvänt.
- Callbacken kan bara anropas medan den C-funktion som typelisp anropade körs. Den kan inte användas
  från sådant som `atexit` eller signalhanterare.

## 7. C-structs

För att skicka något som en array av structs till en C-funktion deklarerar man en struct med samma
layout som i C med `def-c-struct` och allokerar den inuti `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; deklareras inuti ett unsafe på toppnivå

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; fyra item, alla noll
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` allokerar `n` värden av typen `T` och returnerar en `(ptr T)`. `(c-ref p i)` är en
  pekare till det `i`:te, `p::field` är ett fält och `(c-deref p)` är det som en pekare till en skalär
  som `i32` pekar på. Alla kan skrivas med `setf`.
- `(as ptr p)` gör den till en otypad `ptr` för att skickas till C-funktioner som tar en `void *`.
- Storleken på `item` (8 här) och positionen för varje fält bestäms av samma regler som i C.

### Livslängd för allokerat minne

Allokerat minne frigörs när kontrollen lämnar det yttersta `unsafe` i den funktionen. Detsamma händer
när det lämnas med `panic` eller `throw`. Därför kan ett `(ptr T)`-värde inte tas ut ur `unsafe`. Att
göra det till värdet av `unsafe`, fånga det i en closure, skicka det till en `task` och kasta det med
`throw` är alla typfel. Kopiera de värden du vill använda utanför till tal eller en `defstruct` inuti
`unsafe`.

När man allokerar inuti en `lambda` eller en `labels`-funktion skriver man ett `unsafe` inuti den
funktionen.

### Minne som C har allokerat

En pekare som tas emot från C som en `(ptr T)` (ett `defffi`-returvärde, ett callback-argument och så
vidare) är ett fel om den inte pekar inuti minne som allokerats med `c-alloc`. Deklarera funktioner som
tar emot minne som C allokerat med `malloc`, eller NULL, med den otypade `ptr`.

## 8. Vad som inte går att göra

- **Variadiska funktioner** (`printf` och liknande) kan inte deklareras. Den variadiska delen skickas
  enligt andra regler än de fasta argumenten. Deklarera ett separat namn för varje antal argument du
  använder.
- **Att skicka eller returnera structs per värde** är inte möjligt. Använd funktioner som skickar
  pekare.
- **Generiska deklarationer** är inte möjliga.
- **Samma namn som en inbyggd funktion** kan inte användas.
- **De kan inte skickas som funktionsvärden.** Du kan inte skicka en som i `(map xs c-abs)`; omslut den
  i en `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
