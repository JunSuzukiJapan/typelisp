<!-- translated-from: docs/ja/tutorial/traits.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Trait

Un trait è una promessa che "questo tipo supporta queste operazioni". I trait permettono a più tipi di
condividere operazioni con lo stesso nome, in modo che una funzione che le usa non debba essere scritta
una volta per ciascun tipo. Funzionano quasi esattamente come i trait di Rust. Questo capitolo presume
che tu abbia letto [Fondamenti dei tipi](types.md).

## 1. Definire e implementare un trait

Definisci come trait `Shape` le operazioni che restituiscono l'area e il nome di una figura.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- Il `()` dopo il nome del trait è l'elenco dei trait da cui eredita (sezione 4). Lascialo vuoto quando
  non ce ne sono.
- Ogni riga dichiara un metodo. `Self` rappresenta "il tipo che implementa questo trait".

Per implementare un trait per un tipo, scrivi un `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

I metodi implementati si chiamano esattamente come le funzioni ordinarie.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Omettere anche uno solo dei metodi dichiarati dal trait è un errore di tipo nell'`impl`.

## 2. Vincoli di trait: "qualsiasi tipo che implementi questo trait"

Puoi porre una condizione sul parametro di tipo di una funzione generica con `where`. Questo si chiama
**vincolo di trait**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Grazie a `(where (Shape T))`, il corpo può usare `name` e `area` sui valori di `T`. Senza il vincolo non
si sa nulla di `T`, quindi non si potrebbero chiamare.

Passare un tipo che non implementa `Shape` è un errore di tipo nella chiamata.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Una funzione generica ottiene una propria copia per ciascun tipo con cui viene chiamata. Non sono
coinvolti test di tipo né diramazioni a runtime.

## 3. Implementazioni predefinite

Se un metodo di un trait ha un corpo, quel corpo viene usato quando un `impl` omette il metodo.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe is the default one

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; the one written here takes priority

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Implementare i trait standard

Anche la libreria standard ha dei trait. Implementarne uno rende disponibili per il tuo tipo le funzioni
standard che lo usano.

| Trait | Metodi da implementare | Che cosa abilita |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, il pattern `(= expr)` di `match` e così via |
| `Ord` | `less` | `less-equal`, `greater` e così via. `Ord` eredita da `Eq` |
| `print-object` | `print-object` | Come i valori vengono mostrati da `println` e simili |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort` e così via |
| `Error` | `message`, `source` | Uso come tipo di errore ([Gestione degli errori](errors.md)) |

Implementiamo `Eq` e `Ord` per un tipo che rappresenta una somma di denaro. Poiché `Ord` eredita da
`Eq`, l'`impl` di `Eq` deve venire per primo.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (the default implementation in Ord)
```

Implementare `print-object` decide come `println` mostra il valore. L'argomento `escape` è `true`
quando viene richiesta una forma che può essere riletta, come con `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Combinato con un vincolo di trait, puoi scrivere una funzione che funziona per qualsiasi tipo che
implementi `Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Dato un `Vector` con valori `money` di 300, 900 e 100 in quest'ordine, restituisce `(some 900 yen)`.

## 5. `:dyn`: gestire insieme valori di tipi diversi

Tutti gli elementi di un `Vector<T>` hanno lo stesso tipo, quindi i valori `circle` e `rect` non possono
stare in un unico `Vector<circle>`. Per gestire insieme "qualcosa che implementa `Shape`", usa il tipo
`:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Un valore `circle` o `rect` messo dove è atteso un `:dyn Shape` viene convertito automaticamente.
- Quale `area` di quale tipo esegue la chiamata `(area s)` viene deciso a runtime dal tipo di ciò che
  `s` contiene.
- Mettere un valore il cui tipo non implementa `Shape` dove è atteso un `:dyn Shape` è un errore di
  tipo.

Scegliere tra i vincoli di trait della sezione 2 e `:dyn`:

| | Vincolo di trait (`where`) | `:dyn Trait` |
|---|---|---|
| Quando viene deciso il metodo chiamato | Prima dell'esecuzione | A runtime |
| Mescolare tipi in un unico `Vector` | Non possibile | Possibile |
| Tipi utilizzabili | Nessuna restrizione | Strutture, enumerazioni, `int`, `string`, `f64` e altri (non `bool`, `char`, `symbol`, `i32` e simili) |

L'elenco esatto dei tipi utilizzabili si trova nel
[Riferimento della sintassi 3.9](../reference/syntax.md#39-deftrait--impl--trait).

Alcuni trait non si possono usare con `:dyn`: quelli i cui metodi usano `Self` per un argomento diverso
da `self` o per il valore di ritorno (come `equals` in `Eq`). Poiché il tipo non è noto fino al
runtime, non c'è modo di produrre "un valore dello stesso tipo".

## 6. Restrizioni

- Tieni la definizione di un trait, gli `impl` per esso e il codice che lo usa tramite `:dyn` in un
  unico modulo (file). Un trait non può ancora essere reso visibile ad altri moduli.
- Tipi e trait condividono un unico namespace. All'interno di un modulo, un tipo e un trait non possono
  avere lo stesso nome.

## 7. Che cosa leggere dopo

- [Macro](macros.md): definire una sintassi propria
- [Riferimento della sintassi 3.9](../reference/syntax.md#39-deftrait--impl--trait): implementazioni
  blanket, tipi associati e altro
- [Trait standard](../reference/functions/traits.md): l'elenco dei trait della libreria standard
