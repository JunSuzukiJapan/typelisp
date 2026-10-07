<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Felhantering

Felhantering i typelisp delar in misslyckanden i två slag.

| Slag av misslyckande | Exempel | Hur det uttrycks |
|---|---|---|
| Misslyckanden som kan inträffa (återhämtningsbara) | En fil saknas, indata är inte ett tal | Returnera ett `Result<T,E>` |
| Fel i programmet (inte återhämtningsbara) | Ett index utanför intervallet, `unwrap` av `none`, division med noll | Stoppa med `panic` |

Utöver dessa finns `catch` / `throw`, som lämnar många funktionsanrop på en gång, och
`unwind-protect`, som kör en uppstädning hur kroppen än lämnas. Det här kapitlet förutsätter att du har
läst avsnittet om `Result` i [Grunderna i typer](types.md).

## 1. Returnera ett `Result` och ta emot det med `match`

Här är en funktion som läser ett portnummer från en sträng. Den kan misslyckas på två sätt: indata är
inte ett tal, eller så ligger det utanför intervallet.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Anroparen skiljer lyckat från misslyckat med `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- Värdet från en funktion som returnerar `Result` kan inte användas om inte `match` hanterar
  `err`-fallet. Att glömma att hantera misslyckande är ett typfel.
- Felet från `parse-int` är ett värde av typen `ParseIntError`. `(message e)` ger dess
  meddelandesträng.

## 2. Skicka ett misslyckande uppåt till anroparen

Det finns ingen genväg som Rusts `?`. När man anropar flera funktioner som returnerar `Result` i tur
och ordning skriver man delen "returnera misslyckandet som det är" med `match`.

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

När du vet att en operation inte kan misslyckas, eller i ett litet skript där det är i sin ordning att
stoppa vid misslyckande, tar `unwrap` ut innehållet. Om värdet är ett `err` ger det panic. Räcker ett
standardvärde används `unwrap-or`.

## 3. Göra en egen feltyp

Att uttrycka fel som en typ i stället för en sträng låter anroparen förgrena efter slaget av fel. En
feltyp är en vanlig `defenum` eller `defstruct` som implementerar traitet `Error`.

```lisp
(defenum config-error
  (missing string)          ; en inställning saknas
  (invalid string int))     ; ett värde är fel

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

- `message` returnerar en beskrivning av felet.
- `source` returnerar ett annat fel som orsakade det här. Utan orsak är det `none`.

## 4. Kombinera olika slag av fel

Om en funktion anropar både `parse-int` (`ParseIntError`) och `check-workers` (`config-error`) finns det
två feltyper, och de kan inte båda vara `E` i ett och samma `Result<T,E>`. Gör i så fall `E` till
`:dyn Error` (ett fel av vilken typ som helst som implementerar `Error`). Konvertera varje fel med
`as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Med `"4"`, `"-1"` och `"abc"` blir resultaten:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

För `:dyn`, se avsnitt 5 i [Traits](traits.md).

## 5. `panic`: fel i programmet

När programmet når ett tillstånd som aldrig får inträffa stoppar man det med `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Typen på `panic` är `!` (den returnerar inte), så den kan skrivas varhelst vilken typ som helst
  förväntas. Därför passar de två grenarna i `if` ovan ihop.
- Dessa operationer ger också panic: `unwrap` av `none` eller `err`, `get` med ett index utanför
  intervallet och heltalsdivision med noll.
- `panic` stoppar programmet. Även när det sker inuti en task stoppar hela programmet.
- I REPL avslutar en `panic` inte REPL; den väntar på nästa indata.
- Du kan skriva `(todo)` för "inte skrivet än" och `(unreachable)` för "hit ska man aldrig komma". Båda
  ger panic.

`panic` är inte en ersättning för `Result`. För misslyckanden som kan inträffa, som användarindata eller
om en fil finns, använd `Result`.

## 6. `catch` / `throw`: hoppa ut över funktionsgränser

`throw` hoppar direkt ut till det omslutande `catch` med samma tagg, hur många funktionsanrop som än
ligger emellan.

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

Om `v` inte har något negativt tal returnerar `validate` `"all fine"`; om den innehåller `-7` hoppar
kontrollen från insidan av `check-all` ut till `catch`, som returnerar `"negative: -7"`.

- Skriv taggen som en vanlig symbol, som `'bad-input`.
- **Varje tagg bär värden av exakt en typ.** I exemplet ovan bär `'bad-input` en `string`, så att
  kasta ett `int` med samma tagg är ett typfel. Typen på `catch`-kroppen måste också stämma med
  taggens typ.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- Ett `throw` utan något `catch` med samma tagg att nå är ett fel.

Om du bara vill återvända tidigt inifrån en funktion använder du `return-from` i stället för `catch` /
`throw`. `return-from` kan inte korsa funktioner, men i gengäld kan man se vart det återvänder genom
att läsa källkoden.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: städa alltid upp

`(unwind-protect body cleanup)` kör uppstädningen hur kroppen än lämnas: när den slutar normalt, när
den lämnas med `throw` och när den ger panic.

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

Använd det för sådant som "stäng alltid en fil du öppnat" eller "släpp alltid ett lås du tagit".
Standardbibliotekets `with-open-file` och `with-lock` använder `unwind-protect` internt.

## 8. Om Common Lisps conditionsystem

typelisp har inte antagit Common Lisps conditionsystem (`handler-case`, `restart-case` och så vidare).
Det visar inte i typerna vilka misslyckanden en funktion kan orsaka, vilket passar dåligt med statisk
typning. Misslyckanden som kan inträffa skrivs i typerna med `Result`, och överföringar av kontroll
görs med `catch` / `throw`.

## 9. Vad man läser härnäst

- [Samtidighet](concurrency.md): tasks och kanaler
- [Option, Result och feltyper](../reference/functions/option-result.md): listan över funktioner
- [Felmeddelanden](../reference/errors.md): vad de vanliga felen betyder och hur man åtgärdar dem
