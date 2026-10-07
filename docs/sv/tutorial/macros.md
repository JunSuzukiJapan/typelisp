<!-- translated-from: docs/ja/tutorial/macros.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Makron

Ett makro är en funktion som tar ett program och returnerar ett program. Med makron kan du skapa ny
syntax som funktioner inte kan uttrycka. Makron i typelisp fungerar på samma sätt som `defmacro` i
Common Lisp. Det här kapitlet förutsätter att du har läst "Listor (S-uttryck)" i
[Komma igång](intro.md).

## 1. Hur makron skiljer sig från funktioner

En funktion tar emot sina argument **efter att de har utvärderats**. Ett makro tar emot dem **som
uttryck, före utvärdering** (som S-uttrycksdata), bygger ett annat uttryck och returnerar det. Det
returnerade uttrycket ersätter makroanropet, och först därefter typkontrolleras och körs det. Den här
ersättningen kallas **expansion**.

Syntax som `unless` kan till exempel inte skrivas som en funktion. Som funktion skulle kroppen
utvärderas först även när villkoret är sant.

## 2. `defmacro` och quasiquote

Vi gör `my-unless`, som kör sin kropp bara när villkoret är falskt.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Makroargument har inga typer utskrivna. Varje argument är S-uttrycksdata.
- `&rest body` tar emot de återstående argumenten tillsammans som en lista.
- Ett uttryck som börjar med `` ` `` (quasiquote) byggs som data, precis som det står. Inuti det gäller:
  - `,test` sätter in innehållet i variabeln `test` på den platsen.
  - `,@body` fogar in elementen i listan `body` på den platsen.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Du kan kontrollera expansionen med `macroexpand-1`. När man skriver ett makro är det snabbaste sättet
att komma framåt att först titta på dess expansion.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Expansioner typkontrolleras också

Uttrycket ett makro returnerar typkontrolleras som vilket uttryck som helst du skriver för hand.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

Felet rapporteras på den plats där makrot anropades.

Reglerna att else-grenen i `if` inte kan utelämnas, och att båda grenarna i ett `if` måste ha samma typ,
gäller expansioner precis som de är. `my-unless` ovan slutar med `(progn ,@body ())` för att båda
grenarna i `if` ska ha typen `()` oavsett typen på kroppens sista uttryck.

## 4. Namnkrockar och `gensym`

Ett enkelt makro som byter plats på värdena i två variabler ser ut så här:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Det fungerar för det mesta, men går sönder när anroparens variabel råkar heta `tmp`.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (inte bytt)
```

Expansionen är `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, och det `tmp` som makrot skapade
skymmer anroparens `tmp`.

För att undvika detta skapar man namnen på variabler som används inuti ett makro med `gensym`. `gensym`
returnerar en ny symbol som inte kan skrivas någonstans i ett program.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Liksom i Common Lisp förhindrar makron i typelisp inte namnkrockar automatiskt (de är ohygieniska).
Kom ihåg: **använd `gensym` för de bindningar ett makro skapar.**

På samma sätt kan ett makro som upprepar sin kropp ett givet antal gånger skrivas så här:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Expandera olika beroende på argumenten

En makrokropp är vanlig typelisp-kod, så den kan granska sina argument med `if` eller `match` och bygga
en annan expansion. Argumenten är S-uttrycksdata (`Option<Sexpr>`), och den tomma listan är `none`.

Vi gör `my-and`, som returnerar `true` om alla dess villkor är sanna.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; inga argument
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; bara ett
         `(if ,f (my-and ,@more) false)))             ; två eller fler
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` är ett mönster som tar listans huvud till `f` och resten till `more`.
- `sexpr-null` testar om S-uttrycksdata är den tomma listan.
- Den sista `_`-grenen behövs eftersom S-uttrycksdata har andra former än listor (tal, strängar och så
  vidare), och `match` kräver att också de täcks. Ett `&rest`-argument är alltid en lista, så den här
  grenen körs aldrig i praktiken.
- Ett makro kan anropa sig självt i sin expansion. Expansionen upprepas tills inga makroanrop återstår.

## 6. Valfria argument

`&optional` tar emot argument som kan utelämnas. Standardvärden kan anges.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` tar emot nyckelordsargument
([Syntaxreferens 3.14](../reference/syntax.md#314-defmacro--makrodefinitioner)).

## 7. `macrolet`: makron för bara ett ställe

Ett makro som bara används inuti ett uttryck kan definieras med `macrolet`. Det syns inte utanför.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Sådant att ha i åtanke

- **Ett makro kan bara anropas efter sin definition.** Precis som med funktioner definierar du det
  nära början av filen.
- Gör ett makro tillgängligt för andra moduler med `(pub defmacro ...)`.
- Mycket av standardsyntaxen, inklusive `when`, `unless`, `cond`, `and`, `or` och `dotimes`, är
  definierad som makron. Du kan se vad som finns inuti med `(macroexpand '(when true 1))`.
- Om något kan skrivas som en funktion, skriv det som en funktion. Makron kan inte skickas som värden,
  och man måste läsa deras expansion för att förstå vad de gör.

## 9. Vad man läser härnäst

- [Felhantering](errors.md): `Result`, `panic`, `catch` / `throw`
- [Makrofunktioner](../reference/functions/system.md#8-makron): `gensym`, `macroexpand` med flera
