<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
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
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implementano `Iter` rispettivamente tramite `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (si ottiene l'iteratore con `(iter collection)`).
`Chan<T>` è esso stesso un `Iter` (`recv` svolge il ruolo di `next`; [Canali](concurrency.md#2-chant--canali)).
Le liste `Sexpr` non implementano `Iter` (i loro tipi di elemento non sono uniformi). Se implementi
`Iter` per il tuo tipo, esso può essere percorso con `doiter` così com'è e passato alle
[funzioni sulle sequenze](sequences.md#4-funzioni-sulle-sequenze-con-iter).

## 2. `Equal` / `Ord` (confronto)

`Equal` è il trait che usa `equal` e corrisponde al `PartialEq` di Rust. `Ord` corrisponde al `PartialOrd`
di Rust. Si usano nei vincoli `:where` delle funzioni generiche per richiedere che il tipo degli elementi
sia confrontabile.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; da implementare
(deftrait Ord (Equal)                                     ; eredita da Equal
  (defmethod less (self (other Self)) bool)                  ; da implementare
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

Per implementare `Equal` si scrive `equal`, e per `Ord` solo `less`. Le implementazioni predefinite
completano il resto di `Ord`. `Ord` eredita da `Equal`, quindi serve `impl Equal X` prima di `impl Ord X`.

`(equal a b)` chiama l'implementazione di `Equal` del tipo di `a`. Scriverlo per un tipo che non implementa
`Equal` è un errore di tipo. Per confrontare per identità si usano `eq`/`eql`, per struttura `equalp`.
Questi tre si possono usare su un valore di qualsiasi tipo.

Ogni metodo di trait si può chiamare direttamente come funzione (dentro un vincolo `where (Equal A)`/
`(Ord A)`, o su un tipo concreto che lo implementa):

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | Se sono uguali (il `==` di Rust) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` è implementato per: tutti i tipi numerici (da `i8` a `u32` / `f32` / `f64` / `int` / `ratio`),
`bool` `char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` e le tuple (elemento per elemento,
quando gli elementi sono `Equal`), e `Option<T>` e `Result<T,E>` (quando sono la stessa variante e il loro
contenuto è `equal`). Per i tipi predefiniti, il metodo predefinito `equal` del tipo è così com'è
l'implementazione di `Equal`. I numeri si confrontano con `=`, le stringhe per contenuto distinguendo
maiuscole e minuscole. `Ord` è implementato per: tutti i tipi numerici, `char` `string` e `cons-cell<A,B>`
(lessicograficamente, quando gli elementi sono `Ord`).

L'`equal` di una lista (`Sexpr`) è l'`equal` di CL: i cons si confrontano per struttura, le stringhe per
contenuto e ogni altro atomo con `eql`. Questo confronto percorre i valori a run time e non chiama alcun
metodo. Così una struct dentro una lista si confronta per identità, come in CL, qualunque sia l'`impl Equal`
del suo tipo.

I nomi dei metodi di `Ord` non si sovrappongono agli operatori predefiniti (`< <= > >=`) né a `lt`, perché
ciò che è predefinito non si può ridefinire e ogni implementazione delega a loro. Gli operatori di
confronto scalari sono metodi predefiniti per tipo di ricevente ([Numeri](numbers.md),
[Stringhe e caratteri](collections.md)). Dentro un vincolo l'operatore scritto si legge come metodo del
trait (capitolo 3). `=` diventa `equal`, e `/=` diventa `(not (equal a b))`.

## 3. Trait aritmetici (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Uno strato che permette al codice generico di richiedere "un tipo che si possa sommare". **L'aritmetica
sui tipi concreti usa gli operatori predefiniti** ([Numeri](numbers.md)) e non passa per questo strato.

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
  (defmethod shift (self (count int)) Self))                 ; the distance is always int (as with ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; nessun metodo; una combinazione di cinque
```

**All'interno di un vincolo, puoi scrivere gli operatori.** Quando il ricevitore è una variabile di tipo
vincolata da `:where`, gli operatori vengono letti come metodi del trait (`+`→`add`, `-`→`sub`,
`*`→`mul`, `/`→`div`, `rem`→`remainder`, `logand`→`bit-and`, `=`→`equal`, `<`→`less` ...):

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
  (defmethod push (self (x Item)) ()))
```

Una collezione a cui si possono aggiungere elementi uno alla volta. Una funzione generica chiede «un
tipo a cui si può aggiungere `T`» con `(:where (Push C (Item T)))` e aggiunge a qualunque collezione
di questo tipo con lo stesso `(push c x)`.

| Tipo | Che cosa fa `push` |
|---|---|
| `Vector<T>` | Aggiunge in fondo |
| `Deque<T>` | Aggiunge in fondo (`push-back`) |
| `Array<T>` | Aggiunge al fill pointer (`push-extend`). Va in panic senza fill pointer |
| `HashSet<T>` | Lo mette nell'insieme (`insert`) |

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
  (defmethod pop (self) Option<Item>))
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
