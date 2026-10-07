<!-- translated-from: docs/ja/reference/functions/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Trait standard

I trait per l'iterazione, il confronto e l'aritmetica. Gli altri trait standard si trovano nei rispettivi
capitoli: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Tipi di errore](option-result.md#3-tipi-di-errore-e-il-trait-error)), `print-object`
([Stampa](printing.md#5-print-object-rappresentazione-stampata-per-tipo)), e i trait degli stream e
`Pathish` ([Stream e file](streams-files.md)). Quali tipi implementano quali si trova in
[Tipi](../types.md). Come definire i trait è spiegato nel
[Riferimento della sintassi](../syntax.md#39-deftrait--impl--trait).

## 1. Il trait `Iter` e l'iterazione

```lisp
(deftrait Iter ()
  (type Item)
  (next ((self Self)) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementano `Iter` rispettivamente tramite `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (si ottiene l'iteratore con `(iter collection)`).
`Chan<T>` è esso stesso un `Iter` (`recv` svolge il ruolo di `next`; [Canali](concurrency.md#2-chant--canali)).
Le liste `Sexpr` non implementano `Iter` (i loro tipi di elemento non sono uniformi). Se implementi
`Iter` per il tuo tipo, esso può essere percorso con `doiter` così com'è e passato alle
[funzioni sulle sequenze](sequences.md#4-funzioni-sulle-sequenze-con-iter).

## 2. `Eq` / `Ord` (confronto)

Corrispondono a `PartialEq`/`PartialOrd` di Rust (chiamati `Eq`/`Ord`). Si usano nei vincoli `where` delle
funzioni generiche per richiedere che i tipi degli elementi siano confrontabili
(`sort`/`member`/`assoc` e così via).

```lisp
(deftrait Eq ()
  (equals ((self Self) (other Self)) bool)                ; must be implemented
  (not-equals ((self Self) (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less ((self Self) (other Self)) bool)                  ; must be implemented
  (less-equal ((self Self) (other Self)) bool (not (less other self)))
  (greater ((self Self) (other Self)) bool (less other self))
  (greater-equal ((self Self) (other Self)) bool (not (less self other))))
```

Per implementare `Eq` si scrive solo `equals`, e per `Ord` solo `less`. Le implementazioni predefinite
completano il resto. `Ord` eredita da `Eq`, quindi `impl Eq X` serve prima di `impl Ord X`.

Ogni metodo dei trait si può chiamare come funzione così com'è (dentro un vincolo `where (Eq A)`/`(Ord A)`,
oppure su un tipo concreto che lo implementa):

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | Se sono uguali (il `==` di Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | Se non sono uguali (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` è implementato per: tutti i tipi numerici (da `i8` a `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr` (`eq`, cioè l'identità; usato dai pattern di valore di
`match`) e `cons-cell<A,B>` (ricorsivamente, quando gli elementi sono `Eq`). `Ord` è implementato per:
tutti i tipi numerici, `char` `string` e `cons-cell<A,B>` (in ordine lessicografico, quando gli elementi
sono `Ord`).

I nomi dei metodi non si sovrappongono agli operatori predefiniti (`= /= < <= > >=`) né a `eq`/`lt`
perché i predefiniti non possono essere ridefiniti, e ogni implementazione delega ad essi. Gli operatori
di confronto scalari sono essi stessi metodi predefiniti di ciascun tipo di ricevitore ([Numeri](numbers.md),
[Stringhe e caratteri](collections.md)). All'interno di un vincolo, scrivere gli operatori li fa leggere
come metodi del trait (capitolo 3).

## 3. Trait aritmetici (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Uno strato che permette al codice generico di richiedere "un tipo che si possa sommare". **L'aritmetica
sui tipi concreti usa gli operatori predefiniti** ([Numeri](numbers.md)) e non passa per questo strato.

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
  (shift ((self Self) (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Div Rem Ord))               ; no methods; a combination of six
```

**All'interno di un vincolo, puoi scrivere gli operatori.** Quando il ricevitore è una variabile di tipo
vincolata da `where`, gli operatori vengono letti come metodi del trait (`+`→`add`, `-`→`sub`,
`*`→`mul`, `/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` ...):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Il metodo del trait non si chiama `+` perché `+` è il nome di un metodo predefinito e `impl` rifiuta di
ridefinirlo (`cannot redefine built-in method`). Non esiste `Neg`: `(- x)` si espande in
`(- (- x x) x)`, quindi basta `Sub`.

Implementato per: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` su tutti i tipi numerici (tranne `complex`), e
`Bits` su tutti i tipi interi e su `int`.
