<!-- translated-from: docs/ja/tutorial/types.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Fondamenti dei tipi

typelisp è un linguaggio a tipizzazione statica. Questo capitolo spiega che cosa fa il type checker per
te, i tipi che userai più spesso (`Option`, `Result`, strutture ed enumerazioni) e i generici. Presume
che tu abbia letto [Primi passi](intro.md).

## 1. Che cosa significa tipizzazione statica

In typelisp, il tipo di ogni espressione è stabilito prima che il programma venga eseguito. Un'espressione
i cui tipi non si adattano è un errore prima che venga eseguito qualsiasi cosa.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; type error

(main)
```

Eseguire questo file si ferma con un errore di tipo senza nemmeno stampare `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Devi scrivere i tipi per gli argomenti delle funzioni e i valori di ritorno, per le variabili globali e
per i campi delle strutture. Il tipo di una variabile di `let` è ricavato dal suo valore iniziale.

I tipi principali:

| Tipo | Valori di esempio |
|---|---|
| `int` | `42`, `-7` (interi a precisione arbitraria) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Interi a larghezza fissa |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Il tipo di ritorno di una funzione che non restituisce alcun valore |

Non c'è modo di chiedere il tipo di un valore a runtime (niente `typep` o `type-of` di Common Lisp),
perché ogni tipo è già noto prima che il programma venga eseguito.

## 2. `Option<T>`: un valore che può mancare

typelisp non ha `nil`. "Può non esserci un valore" si esprime con il tipo `Option<T>`. Un valore di
`Option<T>` è o `some`, che contiene un valore di `T`, o `none`, che non contiene nulla.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` non è `int`, quindi non può essere usato così com'è nell'aritmetica. `(+ (safe-div 10 2) 1)`
è un errore di tipo. Per usare ciò che c'è dentro, separa `some` da `none` con `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- Nel ramo `(some q)`, il contenuto viene associato alla variabile `q`.
- `match` verifica che i suoi rami **coprano ogni caso**. Dimenticare il ramo `(none)` è un errore di
  tipo.

### Perché non esiste nil

In molti linguaggi, `nil` (`null`) può prendere il posto di un valore di qualsiasi tipo. Di conseguenza,
dimenticare di gestire il caso "nessun valore" passa inosservato finché il programma non viene
eseguito. In typelisp, un punto in cui un valore può mancare ha tipo `Option<T>`, e il codice non supera
il type checker se `match` non gestisce il caso `none`. Un caso dimenticato viene trovato prima che il
programma venga eseguito.

Le condizioni seguono la stessa idea: solo un `bool` può essere la condizione di `if`. Non esiste una
regola come quella di Common Lisp "qualsiasi cosa diversa da `nil` è vera".

### Operazioni comuni

| Forma | Significato |
|---|---|
| `(unwrap-or opt default)` | Il contenuto per `some`; il valore predefinito per `none` |
| `(unwrap opt)` | Estrae il contenuto. Ferma il programma su `none` |
| `(is-some opt)` / `(is-none opt)` | Verifica quale dei due è |

Molte funzioni della libreria standard restituiscono `Option`. Per esempio, `position` restituisce la
posizione dentro `some` se l'elemento viene trovato e `none` se non lo è.

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>`: un'operazione che può fallire

Un'operazione che può fallire restituisce `Result<T,E>`: `ok` che contiene un valore di `T` in caso di
successo, oppure `err` che contiene un errore `E` in caso di fallimento.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Anche le tue funzioni possono restituire `Result`.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Usa `Option` quando un valore mancante non richiede spiegazioni, e `Result` quando vuoi dire perché
qualcosa è fallito. [Gestione degli errori](errors.md) approfondisce la gestione degli errori.

## 4. `defstruct`: le strutture

Un tipo con campi con nome si definisce con `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

La definizione ti fornisce quanto segue:

```lisp
(let ((p (point::new 3 4)))     ; create one (arguments in field order)
  (println "~a" p::x)           ; read a field; (x p) also works
  (setf p::x 10)                ; change it
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Per dare a una struttura funzioni proprie, usa `defmethod`. Il tipo del primo argomento (`self`) decide
a quale tipo appartiene il metodo.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Scrivere solo il nome del tipo al posto di un argomento `self` crea una funzione che si chiama come
`point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum`: una tra più forme

Un valore che è una tra più forme, come "un cerchio, un rettangolo o un punto", si definisce con
`defenum`. Ogni forma si chiama **variante**. Ogni variante può contenere un numero e un tipo diversi di
valori.

```lisp
(defenum shape
  (circle int)        ; radius
  (rect int int)      ; width and height
  (dot))              ; holds no value
```

I valori si creano con il nome del tipo davanti, come in `shape::circle`. In `match`, vengono scomposti
per nome di variante.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Anche qui `match` verifica che ogni caso sia coperto. Se in seguito aggiungi una variante a `shape`, ogni
`match` che non la gestisce diventa un errore di tipo, quindi nessun punto da correggere viene
trascurato.

Dopo `(use shape)`, puoi scrivere `(rect 5 6)` senza il nome del tipo.

`Option` e `Result` sono enumerazioni costruite con questo stesso meccanismo.

## 6. Generici

Una funzione che funziona per qualsiasi tipo si definisce con un **parametro di tipo** `<T>` dopo il suo
nome.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

Non indichi il tipo quando la chiami. `T` viene ricavato dagli argomenti.

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T is int
(first-or names "none")    ; T is string
(first-or ints "none")     ; type error: ints is a Vector<int>, so T is int
```

Anche le strutture e le enumerazioni possono essere generiche. `Vector<T>`, `Option<T>` e `Result<T,E>`
sono tipi di questo genere.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

All'interno di una funzione generica non si sa nulla di `T`, quindi non puoi confrontare né sommare
valori di `T`. Per richiedere qualcosa come "qualsiasi tipo che possa essere confrontato", usa i trait
([Trait](traits.md)).

## 7. Dare a un tipo un altro nome

`deftype` dà a un tipo un altro nome.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` è solo un'altra grafia di `int`, non un nuovo tipo. Passare un semplice `int` dove è atteso
`meters` non è un errore. Se vuoi tenerli distinti, crea una struttura, come in
`(defstruct meters (value int))`.

## 8. Che cosa leggere dopo

- [Trait](traits.md): dare ai tipi operazioni in comune
- [Tipi](../reference/types.md): i tipi predefiniti e i trait implementati da ciascuno
