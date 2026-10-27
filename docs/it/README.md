<!-- translated-from: docs/ja/README.md @ 804fb3914b90de67225569295b86c07e97db869d -->
# Documentazione di typelisp (Italiano)

typelisp è un Lisp a tipizzazione statica. Per l'installazione e la compilazione si veda il
[README.md](../../README.md) (in inglese) nella radice del repository.

## Caratteristiche

### Task e canali

Si avviano task (thread leggeri) che si passano valori attraverso i canali. Per i dettagli si veda
[Concorrenza](tutorial/concurrency.md).

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### Option e Result al posto di nil

Non esiste `nil`. Una funzione che può non avere un valore restituisce `Option<T>`, una che può
fallire restituisce `Result<T,E>`; il contenuto si estrae con `match`. Usare un `Option<string>`
come `string`, o dimenticare il caso `none` in un `match`, è un errore di tipo prima ancora
dell'esecuzione. Per i dettagli si vedano [Fondamenti dei tipi](tutorial/types.md) e [Gestione degli
errori](tutorial/errors.md).

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### Macro con controllo dei tipi

Le macro si scrivono con `defmacro`, quasiquote e `gensym`, come in Common Lisp. Il codice espanso
viene controllato come quello scritto a mano e, se non rispetta i tipi, l'errore viene segnalato nel
punto in cui la macro è stata chiamata. Per i dettagli si veda [Macro](tutorial/macros.md).

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## Tutorial

Se è la prima volta che usi typelisp, leggi questi documenti nell'ordine indicato.

- [Primi passi](tutorial/intro.md): il REPL, le funzioni, le variabili, i condizionali, i cicli, le liste e `Vector`
- [Fondamenti dei tipi](tutorial/types.md): tipi statici, `Option`, `Result`, strutture, enumerazioni, generici
- [Trait](tutorial/traits.md): `deftrait` / `impl`, vincoli di trait, `:dyn`
- [Macro](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Gestione degli errori](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Concorrenza](tutorial/concurrency.md): task, canali, `select`, `Mutex`, `thread`

## Guide

- [Moduli e organizzazione dei file](guide/modules.md): `use`, `pub`, come i file corrispondono ai moduli
- [Compilazione](guide/compile.md): il JIT, la creazione di eseguibili con la compilazione AOT, i dump
- [I/O su file, stream e rete](guide/io.md): file, pathname, TCP / TLS / UDP, risoluzione dei nomi
- [FFI C](guide/ffi.md): chiamare funzioni C con `defffi` (inclusi i callback e le struct C con `def-c-struct`)
- [Integrazione con gli editor](guide/editors.md): `typl-lsp` e la configurazione di VS Code / Emacs
- [Per i programmatori Common Lisp](guide/from-common-lisp.md): in cosa typelisp differisce da CL e come riscrivere il codice CL

## Riferimento

- [Riferimento della sintassi](reference/syntax.md): sintassi lessicale, scrittura dei tipi, definizioni, forme di controllo, compilazione, concorrenza
- [Funzioni predefinite](reference/functions/README.md): funzioni predefinite, metodi e libreria standard
- [Tipi](reference/types.md): i tipi e i trait implementati da ciascuno
- [Messaggi di errore](reference/errors.md): il significato degli errori più comuni e come correggerli

## Integrazione con gli editor

Le istruzioni di configurazione si trovano nella [guida all'integrazione con gli editor](guide/editors.md).
Le scorciatoie da tastiera e le impostazioni di ciascun editor sono elencate in questi documenti:

- [Emacs (typelisp-mode)](../../editor/emacs/README_it.md)
- [VS Code](../../editor/vscode/README_it.md)
