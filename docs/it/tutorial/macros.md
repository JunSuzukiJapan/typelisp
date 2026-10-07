<!-- translated-from: docs/ja/tutorial/macros.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Macro

Una macro è una funzione che prende un programma e restituisce un programma. Le macro permettono di
creare una nuova sintassi che le funzioni non possono esprimere. Le macro di typelisp funzionano come
il `defmacro` di Common Lisp. Questo capitolo presume che tu abbia letto "Liste (S-expression)" in
[Primi passi](intro.md).

## 1. In che cosa le macro differiscono dalle funzioni

Una funzione riceve i propri argomenti **dopo che sono stati valutati**. Una macro li riceve **come
espressioni, prima della valutazione** (come dati S-expression), costruisce un'altra espressione e la
restituisce. L'espressione restituita sostituisce la chiamata della macro, e solo allora viene
controllata nei tipi ed eseguita. Questa sostituzione si chiama **espansione**.

Per esempio, una sintassi come `unless` non si può scrivere come funzione. Come funzione, il corpo
verrebbe valutato per primo anche quando la condizione è vera.

## 2. `defmacro` e quasiquote

Creiamo `my-unless`, che esegue il proprio corpo solo quando la condizione è falsa.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- Gli argomenti delle macro non hanno tipi scritti. Ogni argomento è un dato S-expression.
- `&rest body` riceve insieme, come un'unica lista, gli argomenti rimanenti.
- Un'espressione che inizia con `` ` `` (quasiquote) viene costruita come dato, così com'è scritta. Al
  suo interno:
  - `,test` inserisce in quella posizione il contenuto della variabile `test`.
  - `,@body` inserisce in quella posizione gli elementi della lista `body`.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

Puoi controllare l'espansione con `macroexpand-1`. Quando scrivi una macro, guardare prima la sua
espansione è il modo più rapido di procedere.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Anche le espansioni vengono controllate nei tipi

L'espressione che una macro restituisce viene controllata nei tipi come qualsiasi espressione scritta a
mano.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

L'errore viene segnalato nel punto in cui è stata chiamata la macro.

Le regole secondo cui il ramo else di `if` non può essere omesso e secondo cui entrambi i rami di un
`if` devono avere lo stesso tipo valgono per le espansioni così come sono. Il `my-unless` qui sopra
termina con `(progn ,@body ())` in modo che, qualunque sia il tipo dell'ultima espressione del corpo,
entrambi i rami dell'`if` abbiano tipo `()`.

## 4. Conflitti di nomi e `gensym`

Una macro semplice che scambia i valori di due variabili si presenta così:

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Nella maggior parte dei casi funziona, ma si rompe quando la variabile del chiamante si chiama per caso
`tmp`.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (not swapped)
```

L'espansione è `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, e il `tmp` creato dalla macro
nasconde il `tmp` del chiamante.

Per evitarlo, crea i nomi delle variabili usate dentro una macro con `gensym`. `gensym` restituisce un
simbolo nuovo che non può essere scritto in alcun punto di un programma.

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

Come in Common Lisp, le macro di typelisp non impediscono automaticamente i conflitti di nomi (non sono
igieniche). Ricorda: **usa `gensym` per i binding che una macro crea.**

Allo stesso modo, una macro che ripete il proprio corpo un dato numero di volte si può scrivere così:

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Espandere in modo diverso a seconda degli argomenti

Il corpo di una macro è normale codice typelisp, quindi può ispezionare i propri argomenti con `if` o
`match` e costruire un'espansione diversa. Gli argomenti sono dati S-expression (`Option<Sexpr>`), e
la lista vuota è `none`.

Creiamo `my-and`, che restituisce `true` se tutte le sue condizioni sono vere.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; no arguments
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; just one
         `(if ,f (my-and ,@more) false)))             ; two or more
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` è un pattern che prende la testa di una lista in `f` e il resto in `more`.
- `sexpr-null` verifica se un dato S-expression è la lista vuota.
- Il ramo finale `_` è necessario perché i dati S-expression hanno forme diverse dalle liste (numeri,
  stringhe e così via), e `match` richiede che anche quelle siano coperte. Un argomento `&rest` è
  sempre una lista, quindi questo ramo in realtà non viene mai eseguito.
- Una macro può chiamare se stessa nella propria espansione. L'espansione si ripete finché non restano
  chiamate di macro.

## 6. Argomenti facoltativi

`&optional` riceve argomenti che possono essere omessi. Si possono indicare valori predefiniti.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` riceve argomenti con parola chiave
([Riferimento della sintassi 3.14](../reference/syntax.md#314-defmacro--definizioni-di-macro)).

## 7. `macrolet`: macro per un solo punto

Una macro usata solo dentro una singola espressione si può definire con `macrolet`. Non è visibile
all'esterno.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Cose da tenere presenti

- **Una macro può essere chiamata solo dopo la sua definizione.** Come per le funzioni, definiscila
  vicino all'inizio del file.
- Rendi una macro disponibile ad altri moduli con `(pub defmacro ...)`.
- Gran parte della sintassi standard, tra cui `when`, `unless`, `cond`, `and`, `or` e `dotimes`, è
  definita come macro. Puoi vedere che cosa c'è dentro con `(macroexpand '(when true 1))`.
- Se qualcosa può essere scritto come funzione, scrivilo come funzione. Le macro non possono essere
  passate come valori, e bisogna leggerne l'espansione per capire che cosa fanno.

## 9. Che cosa leggere dopo

- [Gestione degli errori](errors.md): `Result`, `panic`, `catch` / `throw`
- [Funzioni per le macro](../reference/functions/system.md#8-macro): `gensym`, `macroexpand` e altro
