<!-- translated-from: docs/ja/tutorial/intro.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Komma igång

Kapitlet utgår från att utvärdera uttryck i REPL och går sedan igenom funktioner, variabler, villkor,
slingor samt listor och `Vector`, i den ordningen. Hur man bygger `typl` beskrivs i
[README.md](../../../README.md).

## 1. Starta REPL

Startas `typl` utan argument hamnar den i REPL (interaktivt läge). Skriv ett uttryck efter
`typl>` så utvärderas det direkt och värdet skrivs ut. `:quit` lämnar REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Från och med nu visas indata och resultat i REPL i den här formen.

## 2. Utvärdera uttryck

typelisp är ett Lisp, så ett uttryck omges av parenteser med **operatorn eller funktionsnamnet
först**. Man skriver `(+ 1 2)`, inte `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Tal finns i dessa slag:

- **Heltal** har typen `int`. Det finns ingen övre gräns för deras storlek.
- **Decimaltal** har typen `f64`. Skriv dem med decimalpunkt, som `1.5` eller `2.0`.
- Man kan inte blanda `int` och `f64` i en beräkning. `(+ 1 2.0)` är ett typfel. För att konvertera
  skriver man `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` på två heltal ger ett heltal där bråkdelen har tagits bort (det ger inget bråk, som i Common
Lisp). Använd `(mod 7 2)` för resten.

De booleska värdena är `true` och `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Definiera funktioner

Funktioner definieras med `defun`. **Argumentens typer och returtypen skrivs alltid ut.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` betyder "ett argument `n` av typen `int`". Med flera argument listar man dem:
  `((a int) (b int))`.
- `int` efter argumentlistan är returtypen.
- Värdet av det sista uttrycket i kroppen är funktionens returvärde. Man skriver inte `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Ett anrop vars typer inte stämmer rapporteras som ett typfel **innan det körs**. När du kör en fil
innebär ett enda typfel någonstans att inte en rad av programmet körs.

För att göra ett argument valfritt används `&optional`. Anger man ett standardvärde får argumentet
det värdet när det utelämnas.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

`false` som första argument till `format` betyder "returnera resultatet som en sträng i stället för att
skriva ut det". Varje `~a` ersätts av nästa argument.

## 4. Variabler

Lokala variabler skapas med `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- Typen på en `let`-variabel hämtas från dess startvärde. Du behöver inte skriva den.
- Variablerna i ett och samma `let` kan inte referera till varandra. För att bygga en variabel av den
  föregående använder man `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

För att ändra värdet på en variabel används `setf`. **En tilldelning kan inte ändra variabelns typ.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Globala variabler definieras med `defvar`. Här skriver man typen.

```lisp
(defvar (counter int) 0)
```

## 5. Villkor

### if

Skriv `(if villkor då-uttryck annars-uttryck)`. **Annars-uttrycket kan inte utelämnas.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Bara ett uttryck av typen `bool` kan vara villkor. Att skriva ett tal, som i `(if 0 ...)`, är ett
  typfel.
- Då-uttrycket och annars-uttrycket måste ha samma typ.

När ingenting ska hända i falskt fall används `when` (och `unless` för det motsatta).

```lisp
(when (> n 100)
  (println "large")
  (println "really large"))
```

### cond

Med tre eller fler villkor är `cond` lättare att läsa. Det avslutande `else` väljs när inget av
villkoren gäller.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

För att förgrena efter ett värdes form används `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` matchar vilket värde som helst. Eftersom `int` har oräkneligt många värden är det ett fel att
utelämna `_`-grenen, med meddelandet att inte alla fall täcks. Där `match` verkligen kommer till sin
rätt är för att plocka isär `Option` och typer du definierar själv, vilket kommer i nästa kapitel,
[Grunderna i typer](types.md).

## 6. Slingor

En funktion kan anropa sig själv.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

För ett fast antal upprepningar används `dotimes`. `i` går från 0 till `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

Det finns också `while`, `do` och den utökade `loop` från Common Lisp. Nyckelorden i den utökade
`loop` skrivs som keywords (`:for`, `:collect` och så vidare).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. Listor och Vector

### Vector

För att hålla en följd av värden av samma typ används `Vector<T>`. `T` är elementtypen.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; skriver ut #<vector<int> 3 1 2>
```

- Enbart `(Vector::new)` avgör inte elementtypen, så ange typen med
  `(the Vector<int> ...)`.
- `(push v x)` lägger till sist, `(get v i)` läser element `i` och `(len v)` ger längden.
- Ett `get` med ett index utanför intervallet stoppar programmet med ett fel.

### lambda och högre ordningens funktioner

Anonyma funktioner skapas med `lambda`. Som med `defun` skriver man argumentens och returtypen.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` och liknande tar en `Vector` som gjorts om till en **iterator** med
`(iter v)`. Samlingen kommer först och funktionen därefter. `v` i nästa exempel är den `Vector` med
`3 1 2` som byggdes ovan.

```lisp
(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

För att behandla elementen ett efter ett används `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

En funktion som tar en funktion som argument skriver det argumentets typ som
`(fn (argumenttyper...) returtyp)`. En funktion som definierats med `defun` kan skickas med sitt namn
som ett värde.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Listor (S-uttryck)

Listor som skapas med `'(1 2 3)` eller `(list 1 2 3)` är **S-uttrycksdata**. Deras element behöver inte
ha samma typ.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

S-uttrycksdata används främst för att hantera själva program, i makron ([Makron](macros.md)) och med
`read`. För data vars element har en känd typ används `Vector<T>`. En lista av S-uttryck kan gås
igenom med `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Ett par av två värden skapas med `cons` och plockas isär med `car` och `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Skriva ett program i en fil

Ett program kan skrivas i en fil (med filändelsen `.typl`) och köras med `typl filnamn`.
Använd `println` för att visa resultat.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` skriver ut med samma direktiv som `format` och avslutar med ett radslut. `print` lägger
  inte till radslutet.
- `~a` bäddar in ett värde i läsbar form och `~s` i en form som kan läsas tillbaka (strängar får sina
  `"`).
- En fil läses uppifrån och ned. **En funktion kan inte anropas före sin definition.**

## 9. Vad man läser härnäst

- [Grunderna i typer](types.md): `Option`, `Result`, structs, enums, generiska typer
- [För Common Lisp-programmerare](../guide/from-common-lisp.md): en lista över skillnader för den som
  kan Common Lisp
