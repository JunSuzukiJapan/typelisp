<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Per i programmatori Common Lisp

typelisp eredita la sintassi di Common Lisp (CL) e molti dei suoi nomi di funzione, ma è un linguaggio
a tipizzazione statica. Per questo il codice CL non sempre funziona così com'è. Questa guida raccoglie
i punti in cui chi è abituato a CL tende a inciampare, insieme al modo di riscrivere il codice.

## 1. Non esistono `nil` e `t`

I valori booleani sono `true` e `false`. `nil` e `t` non sono definiti.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Solo un `bool` può essere una condizione.** Scrivere `0` o una lista vuota come condizione è un
  errore di tipo. Non esiste la regola "tutto ciò che non è nil è vero".
- **Il ramo else di `if` non può essere omesso.** `(if c x)` è un errore. Quando non serve un ramo
  else, usa `when` / `unless`.
- **"Nessun valore" si esprime con `Option<T>`.** Una funzione che in CL restituiva nil per dire "non
  trovato" qui restituisce `(some x)` oppure `none`.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- La lista vuota `()` è, a seconda del contesto, o il valore del tipo Unit (il valore di ritorno di una
  funzione che non restituisce nulla) o la lista vuota dei dati S-expression. È un valore diverso da
  `false`.

## 2. Scrivere i tipi

Gli argomenti delle funzioni e i valori di ritorno devono avere tipi.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; a generic function
  (unwrap-or (first (iter v)) default))
```

- Una definizione senza tipi, come `(defun f (x) x)`, non si può scrivere.
- Anche le variabili globali come `defvar` richiedono un tipo: `(defvar (count int) 0)`.
- `the` non è un controllo a runtime ma un'annotazione per il type checker.
- **Non c'è modo di ispezionare i tipi a runtime.** Non esistono `typep` o `type-of`, perché il tipo di
  ogni valore è fissato in fase di compilazione. Per accettare uno tra più tipi, crea un tipo somma con
  `defenum` oppure usa un trait.
- `deftype` definisce un alias di tipo. Un tipo che descrive un intervallo di valori, come
  `(deftype small () '(integer 0 9))`, non si può creare.

Il tipo intero predefinito `int` ha precisione arbitraria; come l'integer di CL, non c'è un limite
superiore alla sua dimensione. Esistono anche i tipi a larghezza fissa da `i8` a `i32` e da `u8` a
`u32`. Non esiste un tipo intero a larghezza fissa di 64 bit.

## 3. Le funzioni come valori

typelisp non separa gli spazi dei nomi di funzioni e variabili. Il nome di una funzione può essere
passato come valore così com'è. Non esistono `#'` né `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; call it directly, not with funcall

(apply-to twice 5)                        ; twice, not #'twice
```

- Anche le funzioni predefinite come `+` e `1+` possono essere passate come valori così come sono, dove
  il tipo dell'argomento è fissato, come in `(fn (int) int)`. Quando se ne passa una a una funzione
  generica come `foldl` o `map`, non si sa di quale tipo sia il `+` inteso, quindi racchiudila in una
  `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Le funzioni sulle sequenze prendono **prima la collezione e poi la funzione**: `(map it f)`,
  `(filter it f)`, `(foldl it f init)`. È l'opposto di `(mapcar f list)` di CL.
- `lambda` non può usare `&optional` né `&key` (si può usare `&rest`).
- **Una funzione non può essere chiamata prima di essere definita.** In CL puoi chiamare una funzione
  che definirai più tardi, ma qui questo produce `no such function`. Per funzioni mutuamente
  ricorsive, dichiara prima una di esse con `defsignature`.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Liste e Vector

Ciò che corrisponde a una lista CL sono i **dati S-expression**, il cui tipo è `Option<Sexpr>` (la
lista vuota è `none`). `(list 1 2 3)` e `'(a b c)` hanno questo tipo. I dati S-expression sono ciò con
cui lavorano le macro e `read`; per un normale contenitore di dati usa **`Vector<T>`**.

| Ciò che vuoi | CL | typelisp |
|---|---|---|
| Testa e resto di una S-expression | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Scorrere una lista S-expression | `(dolist (x xs) ...)` | Lo stesso |
| Una sequenza di elementi di un solo tipo | Una lista o un vettore | `Vector<T>` |
| Una coppia | `(cons a b)` | `(cons a b)` (il suo tipo è `cons-cell<A,B>`) |
| Mappatura | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` sono gli accessori della coppia `cons-cell<A,B>` creata con `cons`. Non si possono usare
sulle liste S-expression.

Creare un `Vector`:

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 1)
  (push v 2)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #<vector<int> 10 20>
```

Le funzioni sulle sequenze come `map`, `filter`, `sort` e `find` lavorano su valori che implementano il
trait `Iter`. Passa un `Vector` dopo averlo trasformato in iteratore con `(iter v)`.

## 5. Non esistono valori multipli

Non esistono `values` né `multiple-value-bind`. Le funzioni che in CL restituiscono più valori
restituiscono una coppia o una struttura.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → una `cons-cell` il cui `car` è 3 e il cui `cdr` è 1 |
| `(decode-universal-time t)` → 9 valori | Una struttura `decoded-time` |
| `(read-from-string s)` → valore, posizione | `(read-from-string s)` restituisce una `cons-cell` di valore e posizione dentro un `Result`. Per il solo valore, `(read s)` |

## 6. Non esistono variabili speciali (binding dinamico)

`let` associa sempre in modo lessicale. Se riassoci con `let` una variabile definita con `defvar`, le
funzioni chiamate da lì vedono comunque il valore originale.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 in CL, 1 in typelisp
```

Per modificare temporaneamente una variabile di controllo come `*print-base*`, usa `dlet`. Assegna il
valore e ripristina l'originale in qualunque modo si esca dal corpo.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` riscrive la variabile globale stessa, quindi non è un binding per thread.

## 7. Il condition system non è adottato

Non esistono `define-condition`, `handler-case`, `handler-bind`, `restart-case`, `error` né `signal`.
Si adattano male alla tipizzazione statica. Al loro posto si usano questi due strumenti, per scopi
diversi:

- **I fallimenti recuperabili restituiscono `Result<T,E>`.** Il chiamante separa `ok` / `err` con
  `match`. Non esiste una scorciatoia come il `?` di Rust.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **I fallimenti irrecuperabili (bug) sono `panic`.** `(panic "message")`, passare `none` a `unwrap`,
  dividere per 0 e un indice fuori intervallo sono di questo tipo, e il programma si ferma. La pulizia
  di `unwind-protect` viene eseguita prima che si fermi.

I tipi di errore sono unificati dal trait `Error`, e `(message e)` fornisce il messaggio. Come creare il
proprio tipo di errore è spiegato in
[Option, Result e tipi di errore](../reference/functions/option-result.md#3-tipi-di-errore-e-il-trait-error).
`assert` e `warn` si possono usare come in CL.

`catch` / `throw` / `unwind-protect` esistono. Tuttavia, il tag di `catch` è limitato a un simbolo
letterale non valutato (`'done`), e i valori lanciati con un tag hanno un unico tipo.

## 8. Non esiste CLOS

Non esistono `defclass`, `defgeneric` né la combinazione dei metodi.

- I tipi di dati si definiscono con `defstruct` (strutture) e `defenum` (tipi somma).
- `defmethod` definisce metodi il cui bersaglio è deciso solo dal **tipo statico del primo
  argomento**. Non esiste il multiple dispatch.
- Per fornire operazioni comuni a più tipi, usa i trait (`deftrait` / `impl`). Per valori il cui tipo
  concreto è deciso a runtime, usa il tipo `:dyn Trait`
  ([Riferimento della sintassi 3.9](../reference/syntax.md#39-deftrait--impl--trait)).

In che cosa differisce `defstruct`:

- Il costruttore è `TypeName::new`: `(point::new 1 2)`. Se vuoi un nome come `make-point`, l'opzione
  `(:constructor make-point)` ne crea uno.
- Oltre a `(x p)`, un accessore si può scrivere `p::x`. Si modifica con `(setf p::x 5)`.
- Non viene creato alcun predicato (`point-p`). Non esistono `:conc-name`, `:type` né `:named`.
- `:include` eredita soltanto gli slot; il tipo non diventa un sottotipo del genitore.

## 9. Moduli al posto dei package

Non esistono i package. I namespace sono moduli, e un file è un modulo a sé stante. Invece di
`pkg:symbol`, scrivi `module::name`, e importa i nomi con `use`
([Moduli e organizzazione dei file](modules.md)).

Le parole chiave `:foo` esistono e sono simboli che valutano se stessi. Poiché non ci sono package, i
due punti fanno parte del nome: `(symbol->string :foo)` restituisce `":foo"`.

## 10. Differenze nella lettura e nella sintassi

- Maiuscole e minuscole non sono distinte (i simboli diventano minuscoli quando vengono letti). È
  come in CL.
- Non esiste `#'` (sezione 3). I letterali di numeri complessi `#c(...)` non si possono leggere; crea
  numeri complessi con `(complex 1.0 2.0)`.
- Le clausole del `loop` esteso si scrivono con parole chiave: `(loop :for i :from 1 :to 3 :collect i)`.
  Un `loop` che non inizia con una parola chiave è un semplice ciclo infinito, che si lascia con
  `(break)` o `(return value)`. `return` esce dal ciclo più interno (per uscire da una funzione, usa
  `return-from`).
- La destinazione di `format` è `false` (restituisce una stringa), `true` (standard output) o uno
  stream. Le direttive di formato sono le stesse di CL.
- Leggere da una stringa è `(read "...")`, e leggere da uno stream è `(read-sexpr s)`. Entrambi
  restituiscono un `Result`.
- `eval` controlla i tipi dell'espressione data prima di valutarla, e restituisce un `Result`. I
  riferimenti in avanti non sono possibili, proprio come nel codice sorgente.
- Non esiste `eval-when`.
- I nomi di funzione non usano i suffissi `?` o `!`. I predicati sono chiamati con `-p` / `p` come in
  CL (`zerop`, `sexpr-null`), oppure con `is-` davanti (`is-some`).

## 11. Funzioni principali con nomi diversi

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (da uno stream) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Versioni a due argomenti di `floor` e simili | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (ordine degli argomenti invertito; sezione 4) |
| `length` (di un vettore) | `len` |
| `hash-table-count` | `count` / `size` |

L'elenco delle funzioni si trova in [Funzioni predefinite](../reference/functions/README.md).

## 12. Altre cose che non esistono

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` e `copy-readtable`, `readtable-case` (le reader macro in sé si possono definire con
  `set-macro-character`)
- Pathname logici e pathname con caratteri jolly
- `input-stream-p` / `output-stream-p` (la direzione di uno stream è decisa dal suo tipo)
