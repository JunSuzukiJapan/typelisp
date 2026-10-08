<!-- translated-from: docs/ja/tutorial/intro.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Primi passi

Partendo dalla valutazione di espressioni nel REPL, questo capitolo tratta nell'ordine funzioni,
variabili, condizionali, cicli, liste e `Vector`. Per sapere come compilare `typl`, si veda il
[README.md](../../../README.md).

## 1. Avviare il REPL

Avviato senza argomenti, `typl` entra nel REPL (modalità interattiva). Digita un'espressione dopo
`typl>` e viene valutata subito e il suo valore viene stampato. `:quit` esce dal REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

D'ora in avanti, l'input e i risultati del REPL sono mostrati in questa forma.

## 2. Valutare espressioni

typelisp è un Lisp, quindi un'espressione è racchiusa tra parentesi con **l'operatore o il nome della
funzione per primo**. Si scrive `(+ 1 2)`, non `1 + 2`.

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

I numeri sono di questi tipi:

- Gli **interi** hanno tipo `int`. Non c'è un limite superiore alla loro dimensione.
- I **decimali** hanno tipo `f64`. Si scrivono con il punto decimale, come `1.5` o `2.0`.
- Non si possono mescolare `int` e `f64` in un calcolo. `(+ 1 2.0)` è un errore di tipo. Per
  convertire, scrivi `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` su due interi dà un intero con la parte frazionaria scartata (non produce una frazione come fa
Common Lisp). Usa `(mod 7 2)` per il resto.

I valori booleani sono `true` e `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Definire funzioni

Le funzioni si definiscono con `defun`. **I tipi degli argomenti e il tipo di ritorno si scrivono
sempre.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` significa "un argomento `n` di tipo `int`". Con più argomenti, elencali:
  `((a int) (b int))`.
- L'`int` dopo l'elenco degli argomenti è il tipo di ritorno.
- Il valore dell'ultima espressione nel corpo è il valore di ritorno della funzione. Non si scrive
  `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Una chiamata i cui tipi non corrispondono viene segnalata come errore di tipo **prima di essere
eseguita**. Quando esegui un file, un solo errore di tipo in un punto qualsiasi significa che non viene
eseguita nemmeno una riga del programma.

Per rendere facoltativo un argomento, usa `&optional`. Se indichi un valore predefinito, l'argomento
assume quel valore quando viene omesso.

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

Il `false` dato come primo argomento di `format` significa "restituisci il risultato come stringa invece
di stamparlo". Ogni `~a` viene sostituito dall'argomento successivo.

## 4. Variabili

Le variabili locali si creano con `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- Il tipo di una variabile di `let` è ricavato dal suo valore iniziale. Non serve scriverlo.
- Le variabili di uno stesso `let` non possono riferirsi l'una all'altra. Per costruire una variabile
  a partire da quella precedente, usa `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Per cambiare il valore di una variabile, usa `setf`. **L'assegnamento non può cambiare il tipo della
variabile.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Le variabili globali si definiscono con `defvar`. Qui il tipo lo scrivi.

```lisp
(defvar (counter int) 0)
```

## 5. Condizionali

### if

Scrivi `(if condizione espressione-then espressione-else)`. **L'espressione else non può essere
omessa.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Solo un'espressione di tipo `bool` può essere una condizione. Scrivere un numero, come in
  `(if 0 ...)`, è un errore di tipo.
- Le espressioni then ed else devono avere lo stesso tipo.

Quando nel caso falso non deve succedere nulla, usa `when` (e `unless` per l'opposto).

```lisp
(when (> n 100)
  (println "large")
  (println "really large"))
```

### cond

Con tre o più condizioni, `cond` si legge meglio. L'`else` finale viene preso quando nessuna delle
condizioni è vera.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Per ramificare in base alla forma di un valore, usa `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` corrisponde a qualsiasi valore. Poiché `int` ha innumerevoli valori, omettere il ramo `_` è un
errore che dice che non tutti i casi sono coperti. Dove `match` dà il meglio di sé è nello scomporre
`Option` e i tipi che definisci tu, che compaiono nel capitolo successivo, [Fondamenti dei tipi](types.md).

## 6. Cicli

Una funzione può chiamare se stessa.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Per un numero fisso di ripetizioni, usa `dotimes`. `i` va da 0 a `n - 1`.

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

Ci sono anche `while`, `do` e il `loop` esteso di Common Lisp. Le parole di clausola del `loop` esteso
si scrivono come parole chiave (`:for`, `:collect` e così via).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#<vector<int> 1 4 9 16 25>
```

## 7. Liste e Vector

### Vector

Per contenere una sequenza di valori dello stesso tipo, usa `Vector<T>`. `T` è il tipo degli elementi.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; prints #<vector<int> 3 1 2>
```

- `(Vector::new)` da solo non determina il tipo degli elementi, quindi indica il tipo con
  `(the Vector<int> ...)`.
- `(push v x)` aggiunge in fondo, `(get v i)` legge l'elemento `i` e `(len v)` dà la lunghezza.
- Un `get` con un indice fuori intervallo ferma il programma con un errore.

### lambda e funzioni di ordine superiore

Le funzioni anonime si creano con `lambda`. Come con `defun`, si scrivono i tipi degli argomenti e del
valore di ritorno.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` e simili prendono un `Vector` trasformato in **iteratore** con
`(iter v)`. La collezione viene prima e la funzione dopo. La `v` sopra è stata legata con `let`,
quindi non si può usare fuori da quel `let`. L'esempio seguente definisce prima `v` con `defvar`.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #<vector<int> 30 10 20>
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #<vector<int> 3 2>
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #<vector<int> 1 2 3>
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Per elaborare gli elementi uno per uno, usa `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Una funzione che prende una funzione come argomento scrive il tipo di quell'argomento come
`(fn (tipi-degli-argomenti...) tipo-di-ritorno)`. Una funzione definita con `defun` può essere passata
per nome come valore.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Liste (S-expression)

Le liste create con `'(1 2 3)` o `(list 1 2 3)` sono **dati S-expression**. I loro elementi non devono
necessariamente condividere un tipo.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

I dati S-expression servono soprattutto a trattare i programmi stessi, nelle macro ([Macro](macros.md))
e con `read`. Per dati i cui elementi hanno un tipo noto, usa `Vector<T>`. Una lista S-expression si
può scorrere con `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Una coppia di due valori si crea con `cons` e si scompone con `car` e `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Scrivere un programma in un file

Un programma può essere scritto in un file (con estensione `.typl`) ed eseguito con
`typl nome-del-file`. Usa `println` per mostrare i risultati.

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

- `println` stampa usando le stesse direttive di `format` e termina con un a capo. `print` non
  aggiunge l'a capo.
- `~a` incorpora un valore in forma leggibile da una persona, e `~s` in una forma che può essere
  riletta (le stringhe ricevono i loro `"`).
- Un file viene letto dall'alto verso il basso. **Una funzione non può essere chiamata prima della sua
  definizione.**

## 9. Che cosa leggere dopo

- [Fondamenti dei tipi](types.md): `Option`, `Result`, strutture, enumerazioni, generici
- [Per i programmatori Common Lisp](../guide/from-common-lisp.md): un elenco delle differenze per chi
  conosce Common Lisp
