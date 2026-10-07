<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Standardtraits

Traits för iteration, jämförelse och aritmetik. De andra standardtraits finns i sina egna kapitel: `Hash`
([HashTable](collections.md#4-hashtablekv)), `Error`
([Feltyper](option-result.md#3-feltyper-och-traitet-error)), `print-object`
([Utskrift](printing.md#5-print-object-utskriftsform-per-typ)), samt strömtraits och `Pathish`
([Strömmar och filer](streams-files.md)). Vilka typer som implementerar vilka finns i
[Typer](../types.md). Hur man definierar traits finns i
[Syntaxreferensen](../syntax.md#39-deftrait--impl--traits).

## 1. Traitet `Iter` och iteration

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementerar `Iter` genom `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (iteratorn fås med `(iter collection)`).
`Chan<T>` är själv en `Iter` (`recv` spelar rollen som `next`; [Kanaler](concurrency.md#2-chant--kanaler)).
Listor av `Sexpr` implementerar inte `Iter` (deras elementtyper är inte enhetliga). Om du implementerar
`Iter` för din egen typ kan den gås igenom med `doiter` som den är, och skickas till
[sekvensfunktionerna](sequences.md#4-sekvensfunktioner-på-iter).

## 2. `Eq` / `Ord` (jämförelse)

Dessa motsvarar Rusts `PartialEq`/`PartialOrd` (här kallade `Eq`/`Ord`). De används i
`where`-gränser för generiska funktioner för att kräva att elementtyper kan jämföras
(`sort`/`member`/`assoc` och så vidare).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; måste implementeras
  (not-equals ((self Self) (other Self)) bool             ; standardimplementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; ärver från Eq
  (less ((self Self) (other Self)) bool)                  ; måste implementeras
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

För att implementera `Eq` skriver man bara `equals`, och för `Ord` bara `less`. Standardimplementationerna
fyller i resten. `Ord` ärver från `Eq`, så `impl Eq X` behövs före `impl Ord X`.

Varje trait-metod kan anropas som en funktion som den är (inuti en gräns `where (Eq A)`/`(Ord A)`, eller på
en konkret typ som implementerar den):

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Om de är lika (Rusts `==`) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Om de inte är lika (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` är implementerat för: alla numeriska typer (`i8` till `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr` (`eq`, det vill säga identitet; används av värdemönstren i
`match`) och `cons-cell<A,B>` (rekursivt, när elementen är `Eq`). `Ord` är implementerat för: alla
numeriska typer, `char` `string` och `cons-cell<A,B>` (lexikografiskt, när elementen är `Ord`).

Metodnamnen överlappar inte de inbyggda operatorerna (`= /= < <= > >=`) eller `eq`/`lt` eftersom de
inbyggda inte kan omdefinieras, och varje implementation delegerar till dem. De skalära
jämförelseoperatorerna är själva inbyggda metoder för varje mottagartyp ([Tal](numbers.md),
[Strängar och tecken](collections.md)). Inuti en gräns läses operatorerna som trait-metoderna när man
skriver dem (kapitel 3).

## 3. Aritmetiska traits (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Ett lager för generisk kod att kräva "en typ som kan adderas". **Aritmetik på konkreta typer använder de
inbyggda operatorerna** ([Tal](numbers.md)) och går inte genom det här lagret.

```lisp
(deftrait Add () (add ((self Self) (other Self)) Self))
(deftrait Sub () (sub ((self Self) (other Self)) Self))
(deftrait Mul () (mul ((self Self) (other Self)) Self))
(deftrait Div () (div ((self Self) (other Self)) Self))
(deftrait Rem () (remainder ((self Self) (other Self)) Self))
(deftrait Bits ()
  (bit-and ((self Self) (other Self)) Self)
  (bit-or  ((self Self) (other Self)) Self)
  (bit-xor ((self Self) (other Self)) Self)
  (bit-not ((self Self)) Self)
  (shift ((self Self) (count int)) Self))                 ; avståndet är alltid int (som med ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; inga metoder; en kombination av sex
```

**Inuti en gräns kan man skriva operatorer.** När mottagaren är en typvariabel bunden av `where` läses
operatorer som trait-metoder (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`, `rem`→`remainder`,
`logand`→`bit-and`, `=`→`equals`, `<`→`less` …):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Trait-metoden heter inte `+` eftersom `+` är namnet på en inbyggd metod och `impl` vägrar omdefiniera
den (`cannot redefine built-in method`). Det finns ingen `Neg`: `(- x)` expanderar till
`(- (- x x) x)`, så `Sub` räcker.

Implementerat för: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` på alla numeriska typer (utom `complex`), och
`Bits` på alla heltalstyper och `int`.
