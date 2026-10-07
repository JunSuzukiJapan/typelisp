<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Foutafhandeling

Foutafhandeling in typelisp verdeelt fouten in twee soorten.

| Soort fout | Voorbeelden | Hoe het wordt uitgedrukt |
|---|---|---|
| Fouten die kunnen gebeuren (herstelbaar) | Een bestand ontbreekt, invoer is geen getal | Een `Result<T,E>` teruggeven |
| Fouten in het programma (niet herstelbaar) | Een index buiten het bereik, `unwrap` van `none`, deling door nul | Stoppen met `panic` |

Daarbovenop zijn er `catch` / `throw`, die in één keer veel functieaanroepen verlaten, en
`unwind-protect`, dat opruimcode uitvoert hoe zijn body ook wordt verlaten. Dit hoofdstuk gaat ervan
uit dat je de paragraaf over `Result` in [Basis van types](types.md) hebt gelezen.

## 1. Een `Result` teruggeven en met `match` ontvangen

Hier is een functie die een poortnummer uit een string leest. Ze kan op twee manieren mislukken: de
invoer is geen getal, of ze valt buiten het bereik.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

De aanroeper scheidt succes van mislukking met `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- De waarde van een functie die `Result` teruggeeft kan niet worden gebruikt tenzij `match` het geval
  `err` afhandelt. Vergeten een mislukking af te handelen is een typefout.
- De fout van `parse-int` is een waarde van het type `ParseIntError`. `(message e)` geeft de
  meldingsstring.

## 2. Een mislukking doorgeven aan de aanroeper

Er is geen afkorting zoals `?` in Rust. Wanneer je meerdere functies die `Result` teruggeven na
elkaar aanroept, schrijf je het deel "geef de mislukking ongewijzigd terug" met `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Wanneer je weet dat een bewerking niet kan mislukken, of in een klein script waarin stoppen bij een
fout prima is, haalt `unwrap` de inhoud eruit. Is de waarde een `err`, dan geeft het een panic. Als
een standaardwaarde volstaat, gebruik dan `unwrap-or`.

## 3. Een eigen foutentype maken

Fouten als type uitdrukken in plaats van als string laat de aanroeper op de soort fout vertakken. Een
foutentype is een gewone `defenum` of `defstruct` die de trait `Error` implementeert.

```lisp
(defenum config-error
  (missing string)          ; a setting is missing
  (invalid string int))     ; a value is wrong

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` geeft een beschrijving van de fout terug.
- `source` geeft een andere fout terug die deze veroorzaakte. Zonder oorzaak is het `none`.

## 4. Verschillende soorten fouten combineren

Als één functie zowel `parse-int` (`ParseIntError`) als `check-workers` (`config-error`) aanroept,
zijn er twee foutentypes, en die kunnen niet allebei de `E` van één `Result<T,E>` zijn. Maak in dat
geval van `E` het type `:dyn Error` (een fout van elk type dat `Error` implementeert). Converteer elke
fout met `as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Bij `"4"`, `"-1"` en `"abc"` zijn de resultaten:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Voor `:dyn` zie paragraaf 5 van [Traits](traits.md).

## 5. `panic`: fouten in het programma

Wanneer het programma een toestand bereikt die nooit mag voorkomen, stop je het met `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Het type van `panic` is `!` (het keert niet terug), dus het kan overal worden geschreven waar een
  willekeurig type wordt verwacht. Daarom passen de twee takken van de bovenstaande `if`.
- Deze bewerkingen geven ook een panic: `unwrap` van `none` of `err`, `get` met een index buiten het
  bereik, en gehele deling door nul.
- `panic` stopt het programma. Ook wanneer het in een taak gebeurt, stopt het hele programma.
- In de REPL beëindigt een `panic` de REPL niet; hij wacht op de volgende invoer.
- Je kunt `(todo)` schrijven voor "nog niet geschreven" en `(unreachable)` voor "dit punt zou nooit
  bereikt mogen worden". Beide geven een panic.

`panic` is geen vervanging voor `Result`. Gebruik `Result` voor fouten die kunnen gebeuren, zoals
gebruikersinvoer of of een bestand bestaat.

## 6. `catch` / `throw`: over functies heen springen

`throw` springt direct naar de omsluitende `catch` met dezelfde tag, hoeveel functieaanroepen
daartussen ook liggen.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

Heeft `v` geen negatief getal, dan geeft `validate` `"all fine"` terug; bevat het `-7`, dan springt de
besturing vanuit `check-all` naar de `catch`, die `"negative: -7"` teruggeeft.

- Schrijf de tag als een gewoon symbool, zoals `'bad-input`.
- **Elke tag draagt waarden van precies één type.** In het bovenstaande voorbeeld draagt `'bad-input`
  een `string`, dus een `int` met dezelfde tag gooien is een typefout. Het type van de `catch`-body
  moet ook overeenkomen met het type van de tag.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- Een `throw` zonder `catch` met dezelfde tag om te bereiken is een fout.

Als je alleen vroegtijdig vanuit een functie wilt terugkeren, gebruik dan `return-from` in plaats van
`catch` / `throw`. `return-from` kan geen functies overschrijden, maar in ruil daarvoor kun je aan de
broncode zien waarheen het terugkeert.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: altijd opruimen

`(unwind-protect body cleanup)` voert de opruimcode uit hoe de body ook wordt verlaten: wanneer hij
normaal eindigt, wanneer hij met `throw` wordt verlaten en wanneer er een panic optreedt.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Gebruik het voor zaken als "sluit een geopend bestand altijd" of "geef een genomen lock altijd vrij".
`with-open-file` en `with-lock` uit de standaardbibliotheek gebruiken intern `unwind-protect`.

## 8. Over het conditiesysteem van Common Lisp

typelisp neemt het conditiesysteem van Common Lisp (`handler-case`, `restart-case` enzovoort) niet
over. Het toont in de types niet welke fouten een functie kan veroorzaken, wat slecht past bij
statische typering. Fouten die kunnen gebeuren worden met `Result` in de types geschreven, en
overdrachten van besturing gebeuren met `catch` / `throw`.

## 9. Wat je hierna kunt lezen

- [Gelijktijdigheid](concurrency.md): taken en kanalen
- [Option, Result en foutentypes](../reference/functions/option-result.md): de lijst met functies
- [Foutmeldingen](../reference/errors.md): wat veelvoorkomende fouten betekenen en hoe je ze oplost
