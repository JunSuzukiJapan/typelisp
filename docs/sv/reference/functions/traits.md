<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Standardtraits

Traits för iteration, jämförelse, aritmetik och att lägga till och ta ut element. De andra standardtraits finns i sina egna kapitel: `Hash`
([HashTable](collections.md#4-hashtablekv)), `Error`
([Feltyper](option-result.md#3-feltyper-och-traitet-error)), `print-object`
([Utskrift](printing.md#5-print-object-utskriftsform-per-typ)), samt strömtraits och `Pathish`
([Strömmar och filer](streams-files.md)). Vilka typer som implementerar vilka finns i
[Typer](../types.md). Hur man definierar traits finns i
[Syntaxreferensen](../syntax.md#39-deftrait--impl--traits).

## 1. Traitet `Iter` och iteration

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementerar `Iter` genom `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (iteratorn fås med `(iter collection)`).
`Chan<T>` är själv en `Iter` (`recv` spelar rollen som `next`; [Kanaler](concurrency.md#2-chant--kanaler)).
Listor av `Sexpr` implementerar inte `Iter` (deras elementtyper är inte enhetliga). Om du implementerar
`Iter` för din egen typ kan den gås igenom med `doiter` som den är, och skickas till
[sekvensfunktionerna](sequences.md#4-sekvensfunktioner-på-iter).

## 2. `Equal` / `Ord` (jämförelse)

`Equal` är den trait som `equal` använder och motsvarar Rusts `PartialEq`. `Ord` motsvarar Rusts
`PartialOrd`. De används i generiska funktioners `:where`-gränser för att kräva att elementtypen går att
jämföra.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; måste implementeras
(deftrait Ord (Equal)                                     ; ärver från Equal
  (defmethod less (self (other Self)) bool)                  ; måste implementeras
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

För att implementera `Equal` skriver man `equal`, och för `Ord` bara `less`. Standardimplementationerna
fyller i resten av `Ord`. `Ord` ärver från `Equal`, så `impl Equal X` behövs före `impl Ord X`.

`(equal a b)` anropar `Equal`-implementationen för typen hos `a`. Att skriva det för en typ som inte
implementerar `Equal` är ett typfel. För att jämföra efter identitet använder man `eq`/`eql`, efter
struktur `equalp`. De tre kan användas på ett värde av vilken typ som helst.

Varje traitmetod kan anropas direkt som en funktion (inom en gräns `where (Equal A)`/`(Ord A)`, eller på
en konkret typ som implementerar den):

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Om de är lika (Rusts `==`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` är implementerat för: alla numeriska typer (`i8` till `u32` / `f32` / `f64` / `int` / `ratio`),
`bool` `char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` och tuplerna (element för element,
när elementen är `Equal`), samt `Option<T>` och `Result<T,E>` (när de är samma variant och innehållet är
`equal`). För de inbyggda typerna är typens egen inbyggda metod `equal` som den är `Equal`-implementationen.
Tal jämförs med `=`, strängar efter innehåll med skillnad på stora och små bokstäver. `Ord` är implementerat
för: alla numeriska typer, `char` `string` och `cons-cell<A,B>` (lexikografiskt, när elementen är `Ord`).

`equal` för en lista (`Sexpr`) är CL:s `equal`: conses jämförs efter struktur, strängar efter innehåll och
varje annan atom med `eql`. Jämförelsen går igenom värdena vid körning och anropar ingen metod. En struct i
en lista jämförs därför efter identitet, som i CL, oavsett vilket `impl Equal` dess typ har.

Namnen på `Ord`:s metoder krockar inte med de inbyggda operatorerna (`< <= > >=`) eller `lt`, eftersom det
inbyggda inte kan omdefinieras och varje implementation delegerar till dem. De skalära
jämförelseoperatorerna är själva inbyggda metoder per mottagartyp ([Tal](numbers.md),
[Strängar och tecken](collections.md)). Inom en gräns läses den skrivna operatorn som traitens metod
(kapitel 3). `=` blir `equal`, och `/=` blir `(not (equal a b))`.

## 3. Aritmetiska traits (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Ett lager för generisk kod att kräva "en typ som kan adderas". **Aritmetik på konkreta typer använder de
inbyggda operatorerna** ([Tal](numbers.md)) och går inte genom det här lagret.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; avståndet är alltid int (som med ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; inga metoder; en kombination av fem
```

**Inuti en gräns kan man skriva operatorer.** När mottagaren är en typvariabel bunden av `:where` läses
operatorer som trait-metoder (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equal`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Trait-metoden heter inte `+` eftersom `+` är namnet på en inbyggd metod och `impl` vägrar omdefiniera
den (`cannot redefine built-in method`). Det finns ingen `Neg`: `(- x)` expanderar till
`(- (- x x) x)`, så `Sub` räcker.

Implementerat för: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` på alla numeriska typer (utom `complex`), och
`Bits` på alla heltalstyper och `int`.


**`Div` namnger sin resultattyp med den associerade typen `Output`.** Som i CL är `/` på två heltal en
`rational`, så `Output` för `int` är `rational` och för varje annan numerisk typ typen själv. Eftersom
`Output` skiljer sig mellan typerna innehåller `Number` inte `Div` (en supertrait-lista måste fästa
varje associerad typ). En generisk funktion som dividerar tar emot resultattypen med
`(:where (Div T (Output R)))`, eller kräver en division sluten i `T` med `(:where (Div T (Output T)))` (då
kan en `int` inte skickas).

## 4. Traiten `Push` (lägga till element)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

En samling som element kan läggas till i ett i taget. En generisk funktion begär ”en typ som `T` kan
läggas till i” med `(:where (Push C (Item T)))` och lägger till i varje sådan samling med samma
`(push c x)`.

| Typ | Vad `push` gör |
|---|---|
| `Vector<T>` | Lägger till sist |
| `Deque<T>` | Lägger till sist (`push-back`) |
| `Array<T>` | Lägger till vid fyllpekaren (`push-extend`). Ger panic utan fyllpekare |
| `HashSet<T>` | Lägger det i mängden (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

En egen typ som implementerar `Push` kan skickas till samma generiska funktioner. Den kan också vara
ett trait-objekt, som i `:dyn Push<int>`.

## 5. Traiten `Pop` (ta ut element)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

En samling som elementet som `push` lade till sist kan tas ut ur igen. `none` när den är tom. En
generisk funktion kräver den med `(:where (Pop C (Item T)))`.

| Typ | Vad `pop` gör |
|---|---|
| `Vector<T>` | Tar ut det sista elementet |
| `Deque<T>` | Tar ut det sista elementet (`pop-back`) |
| `Array<T>` | Tar ut elementet närmast före fyllpekaren. Panik utan fyllpekare |

En `HashSet<T>` har inget "sista element", så den är en `Push` men inte en `Pop`.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

Precis som `Push` kan en egen typ implementera den, och den kan vara ett traitobjekt, som i
`:dyn Pop<int>`.
