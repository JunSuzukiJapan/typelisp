<!-- translated-from: docs/ja/reference/functions/traits.md @ 9a4e7d23b09eb3f3244bfb876df1ec57576d6874 -->
# Trait standard

I trait per l'iterazione, il confronto, l'aritmetica e l'aggiunta e la rimozione di elementi. Gli altri trait standard si trovano nei rispettivi
capitoli: `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Tipi di errore](option-result.md#3-tipi-di-errore-e-il-trait-error)), `print-object`
([Stampa](printing.md#5-print-object-rappresentazione-stampata-per-tipo)), e i trait degli stream e
`Pathish` ([Stream e file](streams-files.md)). Quali tipi implementano quali si trova in
[Tipi](../types.md). Come definire i trait è spiegato nel
[Riferimento della sintassi](../syntax.md#39-deftrait--impl--trait).

## 1. Il trait `Iter` e l'iterazione

```lisp
(deftrait Iter ()
  (:type Item)
  (next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementano `Iter` rispettivamente tramite `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (si ottiene l'iteratore con `(iter collection)`).
`Chan<T>` è esso stesso un `Iter` (`recv` svolge il ruolo di `next`; [Canali](concurrency.md#2-chant--canali)).
Le liste `Sexpr` non implementano `Iter` (i loro tipi di elemento non sono uniformi). Se implementi
`Iter` per il tuo tipo, esso può essere percorso con `doiter` così com'è e passato alle
[funzioni sulle sequenze](sequences.md#4-funzioni-sulle-sequenze-con-iter).

## 2. `Eq` / `Ord` (confronto)

Corrispondono a `PartialEq`/`PartialOrd` di Rust (chiamati `Eq`/`Ord`). Si usano nei vincoli `:where` delle
funzioni generiche per richiedere che i tipi degli elementi siano confrontabili
(`sort`/`member`/`assoc` e così via).

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; must be implemented
  (not-equals (self (other Self)) bool             ; default implementation
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; inherits from Eq
  (less (self (other Self)) bool)                  ; must be implemented
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
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
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; nessun metodo; una combinazione di cinque
```

**All'interno di un vincolo, puoi scrivere gli operatori.** Quando il ricevitore è una variabile di tipo
vincolata da `:where`, gli operatori vengono letti come metodi del trait (`+`→`add`, `-`→`sub`,
`*`→`mul`, `/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` ...):

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

Il metodo del trait non si chiama `+` perché `+` è il nome di un metodo predefinito e `impl` rifiuta di
ridefinirlo (`cannot redefine built-in method`). Non esiste `Neg`: `(- x)` si espande in
`(- (- x x) x)`, quindi basta `Sub`.

Implementato per: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` su tutti i tipi numerici (tranne `complex`), e
`Bits` su tutti i tipi interi e su `int`.


**`Div` nomina il proprio tipo di risultato con il tipo associato `Output`.** Come in CL, `/` su due
interi è un `rational`, quindi l'`Output` di `int` è `rational` e quello di ogni altro tipo numerico è il
tipo stesso. Poiché `Output` differisce tra i tipi, `Number` non include `Div` (una lista di supertrait
deve fissare ogni tipo associato). Una funzione generica che divide riceve il tipo del risultato con
`(:where (Div T (Output R)))`, oppure chiede una divisione chiusa in `T` con `(:where (Div T (Output T)))`
(in tal caso non si può passare un `int`).

## 4. Il trait `Push` (aggiungere elementi)

```lisp
(deftrait Push ()
  (:type Item)
  (push (self (x Item)) ()))
```

Una collezione a cui si possono aggiungere elementi uno alla volta. Una funzione generica chiede «un
tipo a cui si può aggiungere `T`» con `(:where (Push C (Item T)))` e aggiunge a qualunque collezione
di questo tipo con lo stesso `(push c x)`.

| Tipo | Che cosa fa `push` |
|---|---|
| `Vector<T>` | Aggiunge in fondo |
| `Deque<T>` | Aggiunge in fondo (`push-back`) |
| `Array<T>` | Aggiunge al fill pointer (`push-extend`). Va in panic senza fill pointer |
| `HashSet<T>` | Lo mette nell'insieme (`insert`). È un `Push` solo quando il tipo degli elementi implementa `Hash` |

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

Un tipo proprio che implementa `Push` si può passare alle stesse funzioni generiche. Può anche
essere un oggetto trait, come in `:dyn Push<int>`.

## 5. Il trait `Pop` (togliere elementi)

```lisp
(deftrait Pop ()
  (:type Item)
  (pop (self) Option<Item>))
```

Una collezione da cui si può togliere l'elemento aggiunto per ultimo con `push`. `none` se è vuota.
Una funzione generica la chiede con `(:where (Pop C (Item T)))`.

| Tipo | Che cosa fa `pop` |
|---|---|
| `Vector<T>` | Toglie l'ultimo elemento |
| `Deque<T>` | Toglie l'ultimo elemento (`pop-back`) |
| `Array<T>` | Toglie l'elemento subito prima del fill pointer. Va in panic senza fill pointer |

Un `HashSet<T>` non ha un «ultimo elemento», quindi è un `Push` ma non un `Pop`.

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

Come per `Push`, un tipo proprio può implementarlo, e può essere un oggetto trait, come in
`:dyn Pop<int>`.
