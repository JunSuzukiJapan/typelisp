<!-- translated-from: docs/ja/reference/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Messaggi di errore

Che cosa significano i principali messaggi di errore di `typl` e come correggerli.

## 1. Leggere un errore

Gli errori vengono scritti sullo standard error in questa forma:

```text
error: file:line:column: kind: message
```

Il `kind` indica quando è stato trovato l'errore.

| Tipo | Quando | Significato |
|---|---|---|
| `type error` | Prima dell'esecuzione (in fase di controllo) | Un errore nei tipi o nei nomi. Quella forma non viene eseguita |
| (nessun tipo) | In fase di lettura o di controllo | Un errore di sintassi come parentesi non bilanciate, o un nome che non si trova |
| `panic` | Durante l'esecuzione | Un fallimento irrecuperabile. Il programma si ferma dopo aver eseguito la pulizia di `unwind-protect` |

Le righe che iniziano con `warning:` sono avvisi, e l'elaborazione continua.

`file:line:column` indica l'espressione con l'errore. Per un errore a runtime che avviene dentro una
funzione della libreria standard, indica il punto in cui il programma ha chiamato quella funzione.
Alcuni errori non hanno una posizione (come `error: panic: ...`).

Esempio:

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Significa che l'espressione alla riga 1, colonna 24 di `main.typl` era una `string` dove era atteso un
`i32`.

## 2. Errori in fase di controllo

Errori trovati prima dell'esecuzione. La forma non viene eseguita finché non vengono corretti.

### 2.1 Tipi

| Messaggio | Significato e correzione |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Un'espressione di tipo `U` si trova dove serve il tipo `T`. Non ci sono conversioni implicite; per i numeri, converti con `(as T x)`. Anche `int` e `i32` sono tipi diversi |
| ``integer literal 300 is out of range for u8 (0..=255)`` | Il letterale non sta nel tipo. Se vuoi che venga troncato, scrivi `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Non esiste un tipo con quel nome. Definisci un tipo prima della prima forma che lo usa (i tipi non hanno dichiarazione anticipata). Se intendevi una variabile di tipo, scrivila in una posizione di dichiarazione come `<foo>` dopo il nome della funzione ([Riferimento della sintassi 3.6](syntax.md#36-defstruct--strutture-tipi-definiti-dallutente)) |
| ``cannot infer type argument `t` for `vector::new` `` | Non è possibile determinare un argomento di tipo. Scrivi il tipo con `the`, come in `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | Il `match` non gestisce ogni variante. Aggiungi rami per le varianti mancanti, oppure un ramo `_` |
| ``type `pt` does not implement trait `eq` required by `where` clause on type parameter `a` `` | La funzione richiede un trait che il tipo passato non implementa. Scrivi `(impl Eq pt ...)` ([Trait standard](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Un valore di un tipo che non implementa il trait è stato passato dove è atteso un `:dyn`. Scrivi l'`impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Un nome di trait è stato scritto dove va un tipo. Scrivi `:dyn Error` |
| ``if: (if cond then else)`` | L'`if` ha la forma sbagliata. `if` richiede un ramo else. Quando non te ne serve uno, usa `when` |

### 2.2 Nomi

| Messaggio | Significato e correzione |
|---|---|
| `no such function: bar` | Non esiste una funzione o un metodo con quel nome. Controlla l'ortografia |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | I metodi vengono selezionati in base al tipo del primo argomento. Un metodo con quel nome esiste, ma non per il tipo del primo argomento (qui `int`). La fine del messaggio elenca i tipi che hanno il metodo |
| `unbound variable: y` | Non esiste una variabile con quel nome. Controlla l'ortografia e lo scope del binding (viene usata fuori dal suo `let`?) |
| ``use: unresolved `nosuch` `` | Il modulo indicato in `use` non si trova. Per sapere come i nomi dei file corrispondono ai percorsi dei moduli, si veda il [Riferimento della sintassi 3.11](syntax.md#311-file-e-moduli-progetti-con-più-file) |
| `unresolved path: c::hidden` | Il modulo esiste, ma il nome no, oppure non è visibile perché manca `pub` |
| `circular module dependency: a -> b -> a` | I moduli si importano a vicenda con `use`. Sposta la parte condivisa in un modulo separato |
| ``return-from: no enclosing block named `nope` `` | Nessun `block` con il nome dato a `return-from` lo racchiude. Il block di una funzione può essere usato solo dentro quella funzione |

### 2.3 Chiamate

| Messaggio | Significato e correzione |
|---|---|
| `f: expected 1 argument(s), got 2` | Il numero di argomenti non corrisponde |
| `f: unknown keyword argument :b` | È stato passato un argomento con parola chiave che la funzione non ha |
| `new: expected 1 field(s), got 2` | Il numero di valori passati a un costruttore di struttura non corrisponde al numero di campi |
| ``setf: cannot assign to constant `k` `` | È stato assegnato un nome definito con `defconstant`. Se deve cambiare, usa `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Una funzione dichiarata con `defsignature` non è definita |
| ``format: ~/nosuch/ — no argument here has a method `nosuch` of the shape ...`` | Nessuno dei tipi degli argomenti ha il metodo chiamato con `~/name/` ([Direttive di formato, capitolo 5](functions/format.md#5-name)) |

## 3. Errori di lettura

| Messaggio | Significato e correzione |
|---|---|
| `unexpected end of input while reading a list` | Manca una parentesi di chiusura. La posizione indica dove è terminata la lettura (come la fine del file), quindi cerca la parentesi di apertura |

## 4. Errori a runtime (panic)

| Messaggio | Significato e correzione |
|---|---|
| `panic: divide by zero` | Divisione per zero con interi o razionali. La divisione per zero in virgola mobile non va in panic; dà `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` è stato applicato a `none`. Gestisci il caso `none` con `match` o `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Un indice fuori intervallo. Controlla la lunghezza con `len`, oppure usa una funzione che restituisce `none` quando è fuori intervallo (`nth`, `pop` e così via) |
| `panic: an integer argument does not fit a fixnum` | Un `int` che non sta in 63 bit è stato passato a un argomento che prende un indice o un conteggio |
| `throw: no enclosing (catch 'oops) for this throw` | Un `throw` è stato eseguito senza alcun `catch` con lo stesso tag che lo racchiuda |
| `panic: <message>` | Il programma ha chiamato `(panic "<message>")`. Un `assert` fallito dà `assertion failed: ...` |

Un `panic` ferma l'intero processo anche quando avviene dentro un task
([Riferimento della sintassi 12.4](syntax.md#124-interazione-con-le-altre-funzionalità)). Esprimi con `Result` i
fallimenti da cui vuoi riprenderti ([capitolo 9 del Riferimento della sintassi](syntax.md#9-politica-di-gestione-degli-errori)).

## 5. Avvisi

| Messaggio | Significato |
|---|---|
| ``warning: redefining function `f` `` | Una funzione con lo stesso nome è stata definita di nuovo. Ha effetto la definizione successiva. Compare normalmente quando correggi una definizione nel REPL |
