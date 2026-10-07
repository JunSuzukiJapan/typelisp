<!-- translated-from: docs/ja/reference/functions/option-result.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Option, Result e tipi di errore

## 1. `Option<T>` / `Result<T,E>`

Costruttori: `Option<T>` ha `Some(T)` / `None`. `Result<T,E>` ha `Ok(T)` / `Err(E)`. `E` può essere
qualsiasi tipo: i tipi di errore concreti predefiniti e i tipi che scrivi tu con `defstruct`/`defenum`
vi si adattano allo stesso modo (capitolo 3).

| Nome | Forma | Option | Result | Descrizione |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Estrae il valore. Va in panic su `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | Il valore, oppure quello predefinito |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Se è `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Se è `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Se è `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Se è `Err` |

I costruttori sono `Option::some`/`Option::none`/`Result::ok`/`Result::err` (oppure, dopo
`(use option)`/`(use result)`, i nomi semplici `some`/`none`/`ok`/`err`).

La ramificazione si scrive esplicitamente con `match`. Non esiste una sintassi corrispondente al `?` di
Rust.

## 2. La rappresentazione a runtime di `Option<T>`

Come in Rust, **`Option<T>` di solito non crea alcun box**. `some v` è `v` stesso e `none` è il valore
della lista vuota, senza allocazione né indirezione. `Option<Sexpr>` (dove la lista vuota è `none`),
`Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` e `Option<(fn ...)>` prendono tutti
questa forma.

Un box viene usato solo quando un valore di `T` non può essere distinto dal valore della lista vuota:

| `T` | Rappresentazione | Motivo |
|---|---|---|
| `Option<U>` (annidato) | Box | Il `none` interno sarebbe lo stesso valore del `none` esterno |
| `()` | Box | Il valore di `()` è il valore della lista vuota stesso |
| `ptr` / `c-long` / `c-ulong` | Box | Tutti i 64 bit sono valore, senza lasciare spazio per distinguerli |
| Qualsiasi altro | Nessun box | — |

La rappresentazione è decisa dal solo tipo e non si può leggere da un valore. In fase di stampa,
`(some ...)`/`none` viene ricostruito dal tipo statico, quindi `(format false "~a" opt)` stampa
`(some 1)`. Ci sono due restrizioni:

- **Non può essere messo in un `:dyn Trait`** (passare a un `:dyn Speak` un valore di `Option<int>` per
  il quale hai scritto `(impl Speak Option<int> ...)` è un errore).
- Un downcast `(the Option<T> ...)` da una `Sexpr` **nomina un costruttore**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. La forma che associa l'intero valore,
  `(the Option<int> o)`, è un errore.

## 3. Tipi di errore e il trait `Error`

Seguendo `std::error::Error` di Rust, **`Error` non è un tipo ma un trait**. I tipi concreti che
rappresentano gli errori sono distinti per ciascuno scopo, e ciascuno implementa `Error`.

| Tipo | Prodotto da |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operazioni su file e stream ([Stream e file](streams-files.md)) |
| `NetError` | Operazioni di rete ([Rete](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. Il `simple-error` di CL: la scelta predefinita quando vuoi solo dire che cosa è successo |
| `WrappedError` | `(wrap-error msg cause)`. Un tipo che porta sia il tuo messaggio sia la causa; è il motivo per cui il trait `Error` ha `source` |

Da `ParseIntError` a `NetError` ciascuno è "un'enumerazione con un'unica variante che contiene una
stringa di messaggio", e il nome del tipo e il nome della variante sono lo stesso
(`(match e ((ParseIntError m) m))`, costruito con `(ParseIntError::ParseIntError "...")`). Non hanno
nulla di speciale: vengono trattati esattamente come i tuoi tipi di errore scritti con
`(defstruct my-err (...))` / `(defenum my-err ...)`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Il messaggio di errore (un metodo del trait `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | La causa che questo errore racchiude, oppure `None` se non ce n'è (l'`Error::source` di Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implementa `Error`) | Allarga un tipo di errore concreto all'oggetto-trait |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implementa `Error`) | Il messaggio e la catena di cause trovata seguendo `source`, una causa per riga. CL non ha un equivalente (il "caused by" di Rust) |

Se implementi `Error` per il tuo tipo di errore, esso può essere gestito **allo stesso modo** degli
errori predefiniti:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; the concrete type goes into E as it is
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; handle any kind uniformly
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Per raccogliere più tipi di errore in un unico `Result`, usa `Result<T, :dyn Error>` (corrispondente al
`Box<dyn Error>` di Rust) e allarga gli errori concreti con `as-dyn-error`. Poiché non esiste `?`, questa
conversione si scrive esplicitamente:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Tipi e trait condividono un unico namespace** (come in Rust). All'interno di un modulo, una
`defstruct`/`defenum` e un trait non possono avere lo stesso nome, e scrivere un nome di trait in
posizione di tipo viene segnalato come "`error` is a trait, not a type — write `:dyn error`".

I fallimenti irrecuperabili si esprimono con `panic`. Per `panic` e `catch`/`throw`, si veda il
[Riferimento della sintassi](../syntax.md#8-uscite-non-locali-catch--throw--unwind-protect); per la
politica di gestione degli errori, si veda il [capitolo 9 dello stesso](../syntax.md#9-politica-di-gestione-degli-errori).
