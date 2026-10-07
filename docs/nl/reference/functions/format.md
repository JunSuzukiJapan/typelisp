<!-- translated-from: docs/ja/reference/functions/format.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Formatdirectieven

De directieven die in de stuurstrings van `print`/`println`/`format` worden geschreven. Ze dekken
vrijwel alle formatdirectieven van `format` in CL. De functies zelf worden beschreven in
[Afdrukken](printing.md#1-print--println--format).

## 1. Hoe je directieven schrijft

Elke directief is `~`, dan optionele **prefixparameters** (gescheiden door komma's: een geheel getal /
`'c` (een teken) / `v` (genomen uit het volgende argument) / `#` (het aantal resterende argumenten)),
dan de optionele **modificatoren** `:` en `@`, dan het directieventeken, in die volgorde.
Directieventekens zijn niet hoofdlettergevoelig.

De stuurstring moet een literal zijn ([Afdrukken](printing.md#1-print--println--format)). Daarnaast
worden bij het controleren de volgende zaken gecontroleerd.

- **Het aantal en de types van argumenten.** Voor elke directief die een argument consumeert: of er
  nog een argument over is, en of zijn type wordt geaccepteerd (de aantekeningen "argument" in de
  onderstaande tabellen). Waar het pad van runtimewaarden afhangt, zoals bij verplaatsen met `~*`,
  welke clausule van `~[` wordt genomen, of `~^` afgaat, of hoe vaak `~@{` herhaalt, wordt **elk pad**
  gecontroleerd. Overgebleven argumenten zijn geen probleem (zoals in CL).
- **Parameters en modificatoren.** Een modificator die niet wordt geaccepteerd, te veel parameters en
  waarden buiten het bereik (een negatieve breedte, een grondtal anders dan 2 tot en met 36, een geheel
  getal waar een teken wordt verwacht enzovoort) zijn fouten. Ze worden nooit stilzwijgend genegeerd of
  afgerond.

De **elementen** van een lijstargument (`~{`, `~:{`, `~<...~:>`) zijn `Sexpr`s, en noch hun aantal noch
het type van elk element kan uit de types worden afgeleid. Eisen aan elementen (een geheel getal voor
`~d`, enzovoort) en ontbrekende elementen worden gecontroleerd wanneer de waarden binnenkomen, en zijn
runtimefouten (er wordt nooit in plaats daarvan naar een andere representatie overgeschakeld).

De soepele regels van CL zijn niet overgenomen. Een niet-geheel getal aan `~d` geven en het als `~a`
laten afdrukken, of `~:[` dat elke waarde als boolean behandelt, worden niet op die manier
geherinterpreteerd; het zijn typefouten.

## 2. Uitvoer (verbruikt één argument)

| Directief | Parameters / modificatoren | Betekenis |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=rechts uitlijnen | Esthetisch (`princ` van CL; strings zonder aanhalingstekens). Het argument kan van elk type zijn |
| `~s` | Idem | Standaard (`prin1` van CL; een vorm die weer kan worden ingelezen). Het argument kan van elk type zijn |
| `~w` | — | `write` van CL. Pretty-print als `*print-pretty*` waar is, anders hetzelfde als `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=cijfergroepen, `@`=altijd een teken | Decimale/binaire/octale/hexadecimale gehele getallen. Het argument is een geheel getal |
| `~r` | `~radix,mincol,padchar,commachar,interval` (met een grondtal) of geen | Met een grondtal dat grondtal (2 tot en met 36). Zonder: `~r`=Engels hoofdtelwoord, `~:r`=Engels rangtelwoord, `~@r`=Romeinse cijfers, `~:@r`=oude Romeinse cijfers. Het argument is een geheel getal |
| `~p` | `:`=één terug, `@`=y/ies | Meervouden (`~p`→"s", `~@p`→"y"/"ies"). Het argument is een geheel getal |
| `~c` | `:`=naam, `@`=`#\`-syntaxis | Een teken. Het argument is een `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=teken | Vaste komma. Het argument is een getal |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=teken | Exponentiële notatie. Het argument is een getal. De parameters exponent-digits, scale en overflowchar van CL worden niet ondersteund (ze opgeven is een fout) |
| `~g` | `@`=teken | Algemene drijvende komma. Het argument is een getal. Neemt geen parameters |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Monetaire notatie. Het argument is een getal |

## 3. Uitvoer (verbruikt geen argumenten)

| Directief | Betekenis |
|---|---|
| `~%` | Nieuwe regel (`~n%` voor n stuks) |
| `~&` | fresh-line (een nieuwe regel tenzij aan het begin van een regel; `~n&`) |
| `~\|` | Pagina-einde (form feed) |
| `~~` | Een letterlijke `~` (`~n~` voor n stuks) |
| `~t` | Tab (`~colnum,colincT`. Als al op kolom colnum of verder, schuift door met een veelvoud van colinc; beweegt niet als colinc 0 is. `@`=relatief. `:`=een tab relatief aan het begin van het logische blok, wat alleen werkt bij pretty-printing) |
| `~_` | Voorwaardelijke nieuwe regel (pretty; kaal=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Inspringing (pretty; `~ni`=begin van het blok + n / `~n:i`=huidige kolom + n) |
| `~<newline>` | Negeert de nieuwe regel (`:`=houd de witruimte, `@`=houd de nieuwe regel) |

Net als in CL doen de pretty-printerdirectieven (`~_` `~i` `~:t` `~<...~:>`, en het
pretty-printpad van `~a`/`~s`/`~w`) allemaal niets wanneer `*print-pretty*` onwaar is. Standaard is
hij onwaar.

## 4. Besturingsstructuren

| Directief | Betekenis |
|---|---|
| `~(...~)` | Hoofdlettergebruik omzetten (`~(` kleine letters, `~:(` elk woord met hoofdletter, `~@(` alleen het eerste woord met hoofdletter, `~:@(` alles hoofdletters) |
| `~[...~;...~]` | Voorwaardelijke selectie (vertakt op een geheel getalargument. Met `~n[`, `~v[` of `~#[` vertakt het op die waarde en neemt het geen argument. `~:;`=de standaardclausule, alleen als laatste clausule). `~:[false~;true~]` vertakt op een `bool`-argument en heeft precies twee clausules |
| `~{...~}` | Iteratie (doorloopt een lijstargument. `~:{`=per sublijst, `~@{`=over de resterende argumenten, `~:@{`=over elke lijst onder de resterende argumenten, `~^`=verlaten, `~:}`=één keer uitvoeren ook als leeg). Een body die in één iteratie geen argument verbruikt is een fout (het zou nooit eindigen) |
| `~<...~;...~>` | Rechtvaardiging (verdeelt segmenten over `~mincol` kolommen. `:`/`@`=opvulling aan de uiteinden) |
| `~<...~;...~:>` | **Logisch blok** (afgesloten met `~:>`; iets anders dan de bovenstaande rechtvaardiging). Het eerste segment is het voorvoegsel en het laatste het achtervoegsel (beide alleen letterlijke strings). Met het scheidingsteken `~@;` is het voorvoegsel een **voorvoegsel per regel**. `~:<` neemt als standaard voor voorvoegsel/achtervoegsel `(`/`)`. Het argument is één lijst (`~@<` gebruikt in plaats daarvan de resterende argumenten) |
| `~*` | Argumenten overslaan (`~n*`=n vooruit, `~:*`=terug, `~@*`=naar een absolute positie) |
| `~/name/` | Methode-aanroep (hoofdstuk 5. De vlaggen `:`/`@` worden aan de methode doorgegeven. Neemt geen parameters) |

De volgende CL-directieven worden niet ondersteund (het zijn fouten bij het controleren).

- `~?` en `~@?`: ze nemen een stuurstring als runtime-argument, dus de argumenten die hun directieven
  verbruiken kunnen niet worden gecontroleerd. Schrijf die directieven rechtstreeks in de
  stuurstring.
- `~@[...~]`: het test of een argument niet nil is, maar deze taal heeft geen nil. Gebruik
  `~:[false~;true~]`, dat op een `bool` vertakt.
- `~{~}` met een lege body: het haalt de body uit een runtime-argument. Schrijf de directieven
  binnen de accolades.

## 5. `~/name/`

**Eén verschil met CL: de naam wordt niet als globale functie opgezocht maar als methode van het eigen
type van het argument.** De methode heeft de vorm `((self Self) (colon bool) (at bool)) → string`, en
de `:`/`@` van de directief worden ongewijzigd doorgegeven.

De manier van CL om hem als globale functie op te zoeken kan in deze taal niet veilig worden
geïmplementeerd. Zelfs met een letterlijke stuurstring is het type van de elementen van een
lijstargument (binnen `~{`) bij het controleren niet bekend, en een functie alleen op naam opzoeken
zou een functie kunnen aanroepen die voor een ander type was bedoeld. Kiezen op het type van de waarde
betekent dat de methode precies voor dat type wordt getypechecked, wat veilig is (hetzelfde mechanisme
als `print-object`). Het werkt ook voor waarden als `string`/`bool`/`char`/`symbol`/lijsten. Alleen
voor gehele getallen, waarvan de breedte niet uit de waarde kan worden afgeleid, is het een fout
**wanneer meer dan één geheel type een methode met die naam definieert**.

Op welk argument het van toepassing is, is niet bekend, maar welke methoden het kan aanroepen wel. De
checker verzamelt elke `~/name/` uit de letterlijke stuurstring en legt vast welke van de types van de
argumenten op die aanroepplek een methode van de bovenstaande vorm hebben. Dus **als geen van de
argumenttypes de methode heeft, is het een fout bij het controleren** (niet tijdens runtime), en het
werkt ook in AOT-uitvoerbare bestanden.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```
