<!-- translated-from: docs/ja/reference/functions/printing.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Afdrukken

`print`/`println`/`format`, de printers met één argument, de pretty printer, `print-object` en de
variabelen die het afdrukken besturen. De lijst met formatdirectieven staat in [format.md](format.md).
Lezen van en schrijven naar streams staat in [Streams en bestanden](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` zijn allemaal **speciale vormen die formatdirectieven interpreteren (de
`format`-directieven van CL)**. Het eerste argument (het tweede bij `format`) is de **stuurstring**, en
elke directief verbruikt achtereenvolgens de volgende variadische argumenten.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Expandeert de stuurstring en schrijft hem zonder nieuwe regel naar standaarduitvoer |
| `println` | `(println control args...)` | `(string, ...)→Unit` | Hetzelfde, met aan het einde een nieuwe regel |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | `format` van CL. Geeft de geëxpandeerde string terug. Als `dest` `true` is (`t` van CL), wordt hij ook naar standaarduitvoer geschreven; bij `false` (`nil` van CL) wordt hij niet geschreven en alleen teruggegeven |
| `format` (naar een stream) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | Als `dest` geen `bool` is, is het de streambestemming van CL. De geëxpandeerde string wordt naar die stream geschreven. De returnwaarde is `()` (`nil` van CL), en er wordt geen string teruggegeven |

Het type van `dest` splitst de betekenis in twee (welke geldt, wordt statisch bepaald). De streamvorm
kan op dezelfde manier worden geschreven met een concreet streamtype, een `:dyn CharOutput`, of een
typevariabele die door `(where (CharOutput S))` is gebonden. Een `dest` die geen `bool` en geen stream
is, is een typefout.

**De stuurstring moet een literal zijn** (dezelfde beperking als `format!` van Rust). De directieven
daarin bepalen hoeveel argumenten van welke types worden genomen, dus een string die tijdens runtime
wordt opgebouwd kan niet bij het controleren worden gelezen. Omdat hij een literal moet zijn, worden
**het aantal en de types van de argumenten bij het controleren gecontroleerd**: `(println "~d" "x")` en
`(println "~a ~a" 1)` zijn fouten bij het controleren. Een verkeerd gespelde directief, een niet
gesloten `~(` en een `~/name/` die geen enkel argument kan beantwoorden zijn ook fouten bij het
controleren. De controleregels staan in [format.md](format.md#1-hoe-je-directieven-schrijft). Om een
string af te drukken die je opbouwt, maak je hem met `(format false ...)` en druk je hem af met
`(println "~a" s)`.

De variadische argumenten worden met hun eigen types in `Sexpr` gewikkeld voordat ze worden
doorgegeven: `i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`, en ook door de gebruiker
gedefinieerde `defstruct`/`defenum`/`Vector<T>`/`HashTable<K,V>` en dergelijke, kunnen allemaal zoals
ze zijn worden doorgegeven (`(println "~a" my-struct)` werkt gewoon).

Een script uitvoeren met `typl file.typl` **drukt de waarden van expressies op het hoogste niveau niet
af**, dus een programma schrijft naar standaarduitvoer door deze aan te roepen.
`print`/`println`/`format` sturen hun uitvoer bij elke aanroep naar buiten (zodat een prompt zichtbaar
is voordat standaardinvoer wordt gelezen, ook via een pipe).

**`Option<Sexpr>` wordt transparant afgedrukt.** Het type van S-expressiedata is `Option<Sexpr>`, dus
de wrapper `(some x)` verschijnt niet in de uitvoer en de inhoud wordt zoals ze is afgedrukt. De lege
lijst wordt afgedrukt als `()`. Andere `Option<T>` worden afgedrukt als `(some ...)` / `none`. Hetzelfde
geldt voor `Option<T>`-velden binnen structs, enums en `Vector`s. Een `Result<Option<Sexpr>,…>` van
`(eval ...)` wordt afgedrukt als `(ok 42)`, of `(ok ())` voor `none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; get only the string, without printing
  (println "~a" s))                   ; => id=42
```

## 2. Printers met één argument

De printers van CLHS 22.1.3. In plaats van een format te expanderen drukken ze één waarde zoals ze is
af. De stream kan worden weggelaten (standaard `*standard-output*`).

| Naam | Vorm | Beschrijving |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Schrijft in een vorm die weer kan worden ingelezen (hetzelfde als `~s`) en geeft `x` terug |
| `princ` | `(princ x [stream])` | Schrijft in een vorm voor mensen (hetzelfde als `~a`) en geeft `x` terug |
| `write` | `(write x [stream])` | `prin1` als `*print-escape*` waar is, `princ` als onwaar. Geeft `x` terug |
| `prin1-to-string` | `(prin1-to-string x)` | Geeft een string terug in plaats van te schrijven (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | Hetzelfde (`~a`). Hetzelfde als `to-string` |
| `write-to-string` | `(write-to-string x)` | Hetzelfde, volgens `*print-escape*` |

`print`/`println` horen **niet** hierbij. Het zijn afkortingen van `format` die een stuurstring nemen,
een andere taak dan `print` van CL (nieuwe regel, dan `prin1`, dan een spatie), dus elk behoudt zijn
eigen naam. Daardoor **heeft het `print` van CL met één argument geen schrijfwijze in deze taal**:
schrijf `prin1`.

Dit zijn macro's, omdat de variadische argumenten van `format` geen typevariabelen accepteren en het
type op de aanroepplek bekend moet zijn.

## 3. Standaardinvoer en de standaardstreams

**Standaardinvoer lezen** gebeurt niet met speciale functies maar met de `CharInput`-methoden op de
standaardstream `*standard-input*`: `(read-line *standard-input*)` / `(read-char *standard-input*)` /
`(read-all *standard-input*)` ([streammethoden](streams-files.md#2-methoden)). Standaarduitvoer en
standaardfout hebben op dezelfde manier `*standard-output*` / `*error-output*`, en kunnen worden
geschreven zoals in `(write-line *standard-output* s)` (`print`/`println`/`format` zijn snelkoppelingen
voor wanneer je formatexpansie nodig hebt, en schrijven altijd naar standaarduitvoer).

## 4. De pretty printer

Dit komt overeen met de Lisp Pretty Printer van CL (CLHS 22.2). **Hij breekt uitvoer die niet binnen de
regelbreedte past, volgens logische blokken en voorwaardelijke nieuwe regels.**

### 4.1 Besturingsvariabelen

Globale variabelen waaraan kan worden toegewezen. Zodra ze met `setf` zijn gezet, beïnvloeden ze al het
latere afdrukken. Gebruik `dlet` (6.3) om er een tijdelijk te wijzigen.

| Variabele | Type | Standaard | Betekenis |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Indien waar nemen `~a`/`~s`/`~w` en de pretty-directieven het pretty-printpad |
| `*print-right-margin*` | `int` | `80` | De rechtermarge (in kolommen). 0 betekent "geen marge, nooit breken". Een negatieve waarde is een afdrukfout |
| `*print-miser-width*` | `int` | `0` | De breedte waarop de miser-stijl begint. 0 komt overeen met `nil` van CL (miser-stijl uit). Een negatieve waarde is een afdrukfout |

De `pprint`-familie en `pprint-logical-block` maken altijd pretty uitvoer, ongeacht
`*print-pretty*` (volgens de definitie van `pprint` van CL).

### 4.2 Kant-en-klare lay-outs (speciale vormen)

Net als `print` zijn dit speciale vormen, dus het argument kan van elk type zijn.

| Naam | Vorm | Beschrijving |
|---|---|---|
| `pprint` | `(pprint x)` | Pretty-print met de standaardlay-out. Zoals in CL **schrijft het eerst een nieuwe regel** en geen aan het einde |
| `pprint-fill` | `(pprint-fill x)` | Vult elke regel met zoveel als past. Schrijft geen nieuwe regel |
| `pprint-linear` | `(pprint-linear x)` | Als niet alle elementen op één regel passen, **één element per regel**. Schrijft geen nieuwe regel |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Een tabel met kolommen van `colinc` breed (standaard 16). Schrijft geen nieuwe regel. Een negatieve `colinc` is een fout |

De standaardlay-out (`pprint`, en `~a` onder `*print-pretty*`) volgt de standaard
`*print-pprint-dispatch*` van CL: hij verkort `(quote x)` tot `'x`, en formatteert codevormen zoals
`defun`/`let`/`if`/`lambda` als "de kop en het voorgeschreven aantal argumenten op de eerste regel, en
de rest van de body twee kolommen ingesprongen, één vorm per regel". Andere lijsten worden gevuld.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Zelf logische blokken bouwen

| Naam | Vorm | Beschrijving |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Een speciale vorm die een logisch blok opent. `obj` is de lijst die `pprint-pop` doorloopt (`()` als er geen wordt doorlopen). `:prefix` en `:per-line-prefix` sluiten elkaar uit (zoals in CL) |
| `pprint-newline` | `(pprint-newline kind)` | Een voorwaardelijke nieuwe regel. `kind` is `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Inspringing. `kind` is `:block` (vanaf het begin van het blok) / `:current` (vanaf de huidige kolom) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Een tab. `kind` is `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` en `colinc` zijn niet-negatief (een fout als negatief) |
| `pprint-pop` | `(pprint-pop)` | Neemt het volgende element uit de lijst van het blok (`()` als hij uitgeput is) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Of de lijst uitgeput is |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Als hij uitgeput is, `break` uit de omsluitende `loop` (een macro) |

Logische blokken nemen geen streamargument: **een geopend logisch blok is impliciete toestand**. Het
buitenste `pprint-logical-block` start het, en wanneer het sluit, wordt het geheel in één keer
geformatteerd en naar standaarduitvoer geschreven. Zolang het open is, gaat de uitvoer van
`print`/`println`/`(format true ...)`/`pprint` allemaal in dat blok, dus **je schrijft de inhoud met
gewone `print` en markeert alleen de plekken om te breken met `pprint-newline` en verwanten**, waardoor
de code er bijna hetzelfde uitziet als in CL.

In CL is `pprint-exit-if-list-exhausted` een niet-lokale uitgang uit `pprint-logical-block`; hier is het
**een `break` uit de omsluitende `loop`** (`pprint-logical-block` stelt geen `block` in). Het idioom van
CL zet het hoe dan ook altijd binnen een `loop`, dus het leest hetzelfde.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Regels voor voorwaardelijke nieuwe regels (CLHS `pprint-newline`):

- `:mandatory` breekt altijd.
- `:linear` breekt als het omsluitende logische blok niet op één regel past. De beslissing geldt per
  blok, dus **alle `:linear`-nieuwe-regels van één blok breken samen** (dit is het "alles op één regel
  of één element per regel" van `pprint-linear`).
- `:fill` breekt als (a) de volgende sectie niet in de rest van de regel past, (b) de vorige sectie niet
  op één regel paste, of (c) in miser-stijl het blok niet op één regel past.
- `:miser` werkt alleen in miser-stijl als `:linear` (wanneer het blok begint binnen
  `*print-miser-width*` van de rechtermarge).

## 5. `print-object` (afgedrukte weergave per type)

`impl print-object <type>` schrijven laat `print`/`println`/`format`/`pprint` waarden van dat type met
die implementatie afdrukken, **ook wanneer ze in lijsten zijn genest**. Het komt overeen met de
generieke functie `print-object` van CL (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argument | Betekenis |
|---|---|
| `self` | De waarde die moet worden afgedrukt |
| `escape` | `*print-escape*` van CL. `true` voor `~s`/`prin1`/`pprint` (een vorm die weer kan worden ingelezen), `false` voor `~a`/`princ` (voor mensen). Een implementatie die er niet om geeft mag het negeren |

De teruggegeven `string` gaat rechtstreeks naar de uitvoer. Types zonder `impl` worden in de
ingebouwde weergave afgedrukt (van de vorm `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   works when nested too
```

Het combineert ook met de pretty printer (hoofdstuk 4). Als `*print-pretty*` waar is, wordt een lijst
die de door de implementatie teruggegeven strings bevat bij de rechtermarge gebroken.

De afgedrukte weergaven van de types van de standaardbibliotheek. Types die ook in CL bestaan worden op
dezelfde manier afgedrukt als in SBCL. Wanneer de REPL een resultaat toont, gebruikt hij dezelfde
weergave als `~s`.

| Type | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#<vector<int> 1 2 3>` | Hetzelfde (elementen met `~a`) |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Hetzelfde |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (het getal is een intern volgnummer) | Hetzelfde |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Een geheel getal (de waarde van `get-universal-time` / `get-internal-real-time` van CL) | Hetzelfde |
| Foutentypes (`ParseIntError`, `SimpleError` enzovoort) | `#<simpleerror "boom">` | Alleen de melding (`boom`) |
| `complex` | `#C(1.0 2.0)` | Hetzelfde |
| `Array<T>` | `#2A((0 0) (0 0))` | Hetzelfde |
| Streams | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Hetzelfde |
| Sockets | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Hetzelfde |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` aan het einde tijdens zomertijd) | Hetzelfde |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Hetzelfde |
| `defstruct`-types | `#<point x: 1 y: 2>` (veldnamen en waarden) | Hetzelfde (velden met `~a`) |

Regels:

- **Registratie is statisch.** Een `impl` wordt als gewone methodedefinitie getypechecked, dus een
  verkeerd gespelde typenaam of een verkeerde signatuur is een compileerfout.
- **Het werkt ook voor generieke types.** `(impl print-object box<T> (where (print-object T)) ...)` gaat
  naar een aparte body voor elk typeargument: een waarde onthoudt zijn type inclusief zijn typeargumenten
  (`box<i32>`). Ingebouwde generieke types zoals `Vector<T>` werken op dezelfde manier.
- **De keuze wordt tijdens het afdrukken gemaakt.** Welke directief welk argument verbruikt hangt af van
  de runtime-inhoud van de stuurstring, dus het onderscheid tussen `~a` en `~s` (dat wil zeggen
  `escape`) is pas op het moment van afdrukken bekend. Dit is hetzelfde als bij CLOS, waar
  `print-object`-methoden "per klasse worden gedefinieerd en bij het afdrukken worden gekozen".
- **Opnieuw binnengaan valt terug op de ingebouwde weergave.** Als een implementatie zichzelf afdrukt met
  `(format false "~a" self)`, zou dat eindeloos recurseren, dus wanneer een waarde die wordt afgedrukt
  opnieuw verschijnt, wordt de ingebouwde weergave gebruikt. Dit kijkt naar waarde-identiteit, niet naar
  een dieptelimiet, dus het staat het legitiem afdrukken van geneste zelfverwijzende structuren niet in
  de weg.
- **Elk scalair type implementeert deze trait.** Dit is **zodat hij als bound kan worden gebruikt**: de
  variadische argumenten van `format` kunnen geen typevariabelen nemen, dus deze bound is de enige manier
  waarop generieke code kan zeggen "waarden van een onbekend type mogen worden weergegeven" (dezelfde vorm
  als `T: Display` van Rust). De `print-object` van `Array<T>` is een voorbeeld.
- **Bij typeargumenten die niet aan de bound voldoen, wordt stilzwijgend de ingebouwde weergave
  gebruikt.** `(impl print-object Array<T> (where (print-object T)))` is van toepassing op
  `Array<i32>`, maar niet op een `Array` waarvan de elementen een `defstruct` zonder `print-object` zijn.
  Het zou nergens op slaan als het louter maken van een array een fout was, dus het is geen fout.
- Het andere mechanisme van CL, `set-pprint-dispatch` / `*print-pprint-dispatch*` (een
  runtime-register met typespecificaties als sleutel), **is niet overgenomen**. De registraties ervan
  worden niet gecontroleerd, wat niet past bij een statisch getypeerde taal.

## 6. Bepalen hoeveel wordt afgedrukt

### 6.1 Diepte, lengte en delen

De besturingsvariabelen van CLHS 22.1.1 die bepalen "hoeveel van een waarde wordt afgedrukt". Net als de
drie in 4.1 zijn het globale variabelen waaraan kan worden toegewezen, en ze gelden voor alle
`print`/`println`/`format`/`pprint`, of `*print-pretty*` nu waar is of niet.

| Variabele | Type | Standaard | Betekenis |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Objecten die op deze diepte of dieper zijn genest worden door `#` vervangen. Het object dat wordt afgedrukt staat op diepte 0. 0 betekent onbeperkt |
| `*print-length*` | `int` | `0` | Drukt lijstelementen (en de velden van `defstruct`/`defenum`-waarden) af tot dit aantal en vervangt de rest door `...`. 0 betekent onbeperkt |
| `*print-circle*` | `bool` | `false` | Indien waar wordt de waarde vóór het afdrukken gescand en krijgen **objecten die twee of meer keer voorkomen labels**. Het eerste voorkomen is `#n=…` en latere `#n#` |

CL gebruikt `nil` voor "onbeperkt", maar deze taal heeft geen `nil`, dus net als bij
`*print-right-margin*` **betekent 0 onbeperkt**. Negatieve waarden hebben geen betekenis en zijn
afdrukfouten. De standaardwaarden zijn allemaal "geen limiet / geen labels", overeenkomend met de
beginwaarden van CL.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Circulaire structuren kunnen alleen worden afgedrukt wanneer `*print-circle*` waar is.** Als je een
waarde afdrukt die naar zichzelf wijst terwijl hij onwaar is (de standaard), blijft de printer de cyclus
volgen en crasht het proces. CL is hetzelfde (CLHS laat het afdrukken van circulaire structuren
ongedefinieerd wanneer `*print-circle*` onwaar is).

Een cyclus kan alleen worden gemaakt door "een `defstruct`-veld met `setf` naar zichzelf te laten
wijzen" (`Sexpr`-cellen kunnen na het maken niet worden gewijzigd, dus een lijst als `'(1 2 3)` kan nooit
circulair zijn):

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a points to a itself
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Labels **beginnen voor elk afgedrukt ding opnieuw bij 1** (zoals in CL). Ook zonder cyclus krijgt een
object dat twee keer voorkomt `#1=`/`#1#`, zodat in de uitvoer de informatie behouden blijft dat "deze
twee hetzelfde object zijn", zoals CL voorschrijft:

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Een waarde zonder deling **toont helemaal geen labels**, dus deze variabele op waar laten staan
verandert de uitvoer van alledaagse code niet.

### 6.2 Grondtal, hoofdlettergebruik en leesbaarheid

| Variabele | Type | Standaard | Betekenis |
|---|---|---|---|
| `*print-base*` | `int` | `10` | Het grondtal voor het afdrukken van gehele getallen (met vaste breedte en `int`). Buiten 2 tot en met 36 is het een **afdrukfout** (CL specificeert het bereik ook) |
| `*print-radix*` | `bool` | `false` | Indien waar wordt een grondtalmarkering toegevoegd: `#b`/`#o`/`#x`, `#NNr` voor andere grondtallen, en een afsluitende `.` voor grondtal 10. De markering komt **vóór** het teken (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | Het hoofdlettergebruik van symboolnamen: `:upcase` / `:downcase` / `:capitalize` (dezelfde schrijfwijzen als CL). Elk ander symbool is een afdrukfout |
| `*print-readably*` | `bool` | `false` | Indien waar wordt afgedrukt in een vorm die weer kan worden ingelezen. Het dwingt escaping af en schakelt de afkappingen van `*print-level*`/`*print-length*` uit |
| `*print-lines*` | `int` | `0` | Het aantal regels dat de pretty printer mag gebruiken. Het teveel wordt afgekapt, met `..` aan het einde zoals in CL. 0 betekent onbeperkt. Een negatieve waarde is een afdrukfout |
| `*print-escape*` | `bool` | `true` | Of `write`/`write-to-string` `prin1` of `princ` doen. **Alleen die twee lezen hem** |
| `*print-array*` | `bool` | `true` | Of `Array<T>` zijn inhoud toont. Indien waar de arraysyntaxis van CL (`#(1 2 3)` / `#2A((1 2) (3 4))`); indien onwaar alleen de vorm, `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

De markeringen die `*print-radix*` toevoegt kunnen door de reader weer worden ingelezen (de
grondtalnotatie in de [Syntaxreferentie](../syntax.md#1-lexicale-elementen)).

**Waarom de standaardwaarde van `*print-case*` van CL verschilt**: de standaardwaarde van CL is
`:upcase` omdat de reader van CL symboolnamen in hoofdletters opslaat, dat wil zeggen het betekent "zoals
opgeslagen". Deze reader slaat ze in kleine letters op, dus de standaardwaarde met dezelfde betekenis is
`:downcase`.

**De ontbrekende helft van `*print-readably*`**: CL signaleert `print-not-readable` voor waarden die
niet weer kunnen worden ingelezen, maar deze taal heeft geen conditie om te signaleren, en geen manier
om de leesbaarheid van door de gebruiker gedefinieerde types te bepalen, die `print-object` op elke
manier mag afdrukken. Alleen het afdwingen van escaping en het overschrijven van de afkappingen is er.

**Waarom alleen `write` `*print-escape*` leest**: zoals CLHS voorschrijft binden `~s`/`prin1`/`pprint` hem
aan waar, en `~a`/`princ` aan onwaar, elk alleen voor de duur van hun eigen aanroep. De enige lezers die
hem ongebonden zien zijn dus `write`/`write-to-string`. Een implementatie van `print-object` hoort haar
eigen argument `escape` te lezen in plaats van deze globale variabele: dat argument draagt de waarde die
de directief koos.

**Wat CL heeft en deze taal niet**: `*print-gensym*` (er zijn geen niet-geïnterneerde symbolen).

### 6.3 Tijdelijke overschrijvingen

CL bindt deze met `let`, maar `let` in deze taal bindt lexicaal, dus gebruik `dlet`
([Overig](system.md#10-overig)):

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; the limits apply to this one print only
(with-standard-io-syntax (println "~a" x))   ; print with everything back at the standard values
```

`with-standard-io-syntax` voert zijn body uit met alle besturingsvariabelen van de printer op hun
standaardwaarden en `*read-eval*` op `true`.
