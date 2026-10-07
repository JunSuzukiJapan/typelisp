<!-- translated-from: docs/ja/tutorial/errors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Gestione degli errori

La gestione degli errori in typelisp divide i fallimenti in due tipi.

| Tipo di fallimento | Esempi | Come viene espresso |
|---|---|---|
| Fallimenti che possono accadere (recuperabili) | Un file manca, l'input non è un numero | Restituire un `Result<T,E>` |
| Errori nel programma (non recuperabili) | Un indice fuori intervallo, `unwrap` di `none`, divisione per zero | Fermarsi con `panic` |

Oltre a questi ci sono `catch` / `throw`, che escono da molte chiamate di funzione in una volta, e
`unwind-protect`, che esegue la pulizia in qualunque modo si esca dal suo corpo. Questo capitolo
presume che tu abbia letto la sezione su `Result` di [Fondamenti dei tipi](types.md).

## 1. Restituire un `Result` e riceverlo con `match`

Ecco una funzione che legge un numero di porta da una stringa. Può fallire in due modi: l'input non è un
numero, oppure è fuori intervallo.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

Il chiamante separa il successo dal fallimento con `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- Il valore di una funzione che restituisce `Result` non può essere usato a meno che `match` non gestisca
  il caso `err`. Dimenticare di gestire il fallimento è un errore di tipo.
- L'errore di `parse-int` è un valore di tipo `ParseIntError`. `(message e)` ne fornisce la stringa del
  messaggio.

## 2. Passare un fallimento al chiamante

Non esiste una scorciatoia come il `?` di Rust. Quando chiami in successione più funzioni che
restituiscono `Result`, scrivi con `match` la parte "restituisci il fallimento così com'è".

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Quando sai che un'operazione non può fallire, o in un piccolo script in cui va bene fermarsi in caso di
fallimento, `unwrap` estrae il contenuto. Se il valore è un `err`, va in panic. Se basta un valore
predefinito, usa `unwrap-or`.

## 3. Creare un proprio tipo di errore

Esprimere gli errori come un tipo anziché come stringa permette al chiamante di ramificare in base al
tipo di errore. Un tipo di errore è un normale `defenum` o `defstruct` che implementa il trait `Error`.

```lisp
(defenum config-error
  (missing string)          ; a setting is missing
  (invalid string int))     ; a value is wrong

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` restituisce una descrizione dell'errore.
- `source` restituisce un altro errore che ha causato questo. Senza causa, è `none`.

## 4. Combinare tipi di errore diversi

Se una funzione chiama sia `parse-int` (`ParseIntError`) sia `check-workers` (`config-error`), ci sono
due tipi di errore, e non possono essere entrambi la `E` di un unico `Result<T,E>`. In tal caso, fai di
`E` un `:dyn Error` (un errore di qualsiasi tipo che implementi `Error`). Converti ogni errore con
`as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Dati `"4"`, `"-1"` e `"abc"`, i risultati sono:

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Per `:dyn`, si veda la sezione 5 di [Trait](traits.md).

## 5. `panic`: errori nel programma

Quando il programma raggiunge uno stato che non deve mai verificarsi, fermalo con `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Il tipo di `panic` è `!` (non ritorna), quindi può essere scritto ovunque sia atteso un tipo
  qualsiasi. Per questo i due rami dell'`if` qui sopra si adattano.
- Anche queste operazioni vanno in panic: `unwrap` di `none` o `err`, `get` con un indice fuori
  intervallo e la divisione intera per zero.
- `panic` ferma il programma. Anche quando accade dentro un task, si ferma l'intero programma.
- Nel REPL, un `panic` non termina il REPL; questo attende il prossimo input.
- Puoi scrivere `(todo)` per "non ancora scritto" e `(unreachable)` per "questo punto non dovrebbe mai
  essere raggiunto". Entrambi vanno in panic.

`panic` non sostituisce `Result`. Per i fallimenti che possono accadere, come l'input dell'utente o
l'esistenza di un file, usa `Result`.

## 6. `catch` / `throw`: saltare fuori attraverso più funzioni

`throw` salta direttamente fuori fino al `catch` che lo racchiude con lo stesso tag, per quante chiamate
di funzione ci siano in mezzo.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

Se `v` non ha numeri negativi, `validate` restituisce `"all fine"`; se contiene `-7`, il controllo salta
dall'interno di `check-all` fino al `catch`, che restituisce `"negative: -7"`.

- Scrivi il tag come un semplice simbolo, come `'bad-input`.
- **Ogni tag trasporta valori di un solo tipo.** Nell'esempio qui sopra `'bad-input` trasporta una
  `string`, quindi lanciare un `int` con lo stesso tag è un errore di tipo. Anche il tipo del corpo del
  `catch` deve corrispondere al tipo del tag.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- Un `throw` senza alcun `catch` con lo stesso tag da raggiungere è un errore.

Se vuoi solo ritornare in anticipo dall'interno di una funzione, usa `return-from` invece di `catch` /
`throw`. `return-from` non può attraversare le funzioni, ma in cambio puoi capire dove ritorna leggendo
il sorgente.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect`: ripulire sempre

`(unwind-protect body cleanup)` esegue la pulizia in qualunque modo si esca dal corpo: quando termina
normalmente, quando si esce con `throw` e quando va in panic.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Usalo per cose come "chiudi sempre un file che hai aperto" o "rilascia sempre un lock che hai preso".
`with-open-file` e `with-lock` della libreria standard usano internamente `unwind-protect`.

## 8. Il condition system di Common Lisp

typelisp non adotta il condition system di Common Lisp (`handler-case`, `restart-case` e così via). Non
mostra nei tipi quali fallimenti una funzione può causare, il che si adatta male alla tipizzazione
statica. I fallimenti che possono accadere si scrivono nei tipi con `Result`, e i trasferimenti di
controllo si fanno con `catch` / `throw`.

## 9. Che cosa leggere dopo

- [Concorrenza](concurrency.md): task e canali
- [Option, Result e tipi di errore](../reference/functions/option-result.md): l'elenco delle funzioni
- [Messaggi di errore](../reference/errors.md): il significato degli errori più comuni e come correggerli
