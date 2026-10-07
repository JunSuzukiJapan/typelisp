<!-- translated-from: docs/ja/guide/modules.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Moduli e organizzazione dei file

Questa guida spiega come mettere insieme un programma composto da più file. Le regole dettagliate si
trovano nelle sezioni da 3.10 a 3.13 del [Riferimento della sintassi](../reference/syntax.md#310-module--use--namespace).

## 1. Un file è un modulo

In typelisp, **un file è un modulo a sé stante**. Il percorso del file relativo alla radice dei
sorgenti è il percorso del modulo.

| File | Modulo |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Non c'è bisogno di scrivere una dichiarazione di modulo all'inizio di un file.

## 2. Configurazione di un progetto

Metti un file chiamato `typelisp.toml` nella radice del progetto. Può essere vuoto.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Per tenere i sorgenti sotto `src/`, scrivi questa riga in `typelisp.toml`:

```toml
src = "src"
```

`typl` cerca `typelisp.toml` partendo dalla directory del file che esegue e risalendo, e usa il punto
in cui lo trova come radice dei sorgenti. Se non ne trova nessuno, la radice è la directory del file
in esecuzione (nel REPL, la directory corrente).

## 3. Rendere pubbliche le definizioni e usarle

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; a field without pub cannot be read from outside

(defun square ((n i32)) i32 (* n n))   ; a function without pub cannot be called from outside either

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl` viene caricato nel punto in cui è scritto `(use geometry)`. Non c'è bisogno di
caricarlo prima.

### Che cosa viene reso pubblico

- Funzioni, strutture, enumerazioni, variabili globali, macro e metodi sono visibili da altri moduli
  solo quando portano `pub`. Metti `pub` subito prima della definizione, come in `(pub defun ...)`.
- Per le strutture, **rendere pubblico il tipo e rendere pubblici i campi sono due cose separate**.
  `(pub defstruct point ...)` rende visibile il tipo, e solo i campi scritti come `(pub x i32)`
  possono essere letti e scritti dall'esterno.
- Usare dall'esterno un nome che non è pubblico produce un errore di "impossibile risolvere" come
  `unresolved path: geometry::square`. È lo stesso messaggio che si ottiene per un nome scritto male,
  quindi se l'ortografia è giusta e il nome continua a non risolversi, sospetta un `pub` mancante.

L'elenco delle definizioni che possono ricevere `pub` si trova nel
[Riferimento della sintassi 3.13](../reference/syntax.md#313-pub--visibilità).

## 4. Come si scrive `use`

```lisp
(use geometry)              ; bring in a module; write geometry::dist2 to use it
(use geometry::dist2)       ; bring in a function; use it by the bare name dist2
(use geometry::point)       ; bring in a type; write point::new, point::origin, and point in type annotations
(use a::f b::g)             ; several can be written together
```

- **`use` ha effetto solo sulle forme che lo seguono.** Mettilo all'inizio del file. Scrivere
  `geometry::dist2` sopra `use` produce `unresolved path`.
- Anche scrivere il percorso completo `geometry::dist2` senza aver fatto `use` del modulo non si
  risolve. Solo `use` fa caricare un file.
- Fare `use` di un nome la cui forma semplice è già occupata produce un avviso. Quando vuoi comunque
  importarlo, usa `shadowing-import`.
- Un modulo dentro una directory si scrive `(use geo::shapes)`, e dopo si fa riferimento ad esso con
  la sua ultima parte (`shapes::...`).

### Chiamare i metodi dei trait

I metodi implementati in un `impl` **appartengono al tipo**, non alle funzioni del modulo, quindi si
chiamano senza il nome del modulo.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, not core::area
```

I metodi dentro un `impl` sono sempre pubblici, anche senza `pub`.

Un trait in sé non può essere reso pubblico verso altri moduli. Tieni la definizione di un trait, gli
`impl` per esso e il codice che lo usa tramite `:dyn` in un unico modulo.

## 5. Suddividere i namespace all'interno di un file

Per suddividere ulteriormente un namespace all'interno di un file, usa `module`. Viene annidato dentro
il modulo del file stesso.

```lisp
;; inside main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Per mettere tutto il resto del file in un unico namespace, puoi scrivere `(in-module util)` invece di
racchiuderlo tra parentesi.

## 6. Restrizioni sulle dipendenze

- **I cicli non sono ammessi.** Se `a.typl` fa `(use b)` e `b.typl` fa `(use a)`, il risultato è
  l'errore `circular module dependency: a -> b -> a`. Sposta le definizioni di cui entrambi hanno
  bisogno in un terzo modulo.
- **Né i tipi né le funzioni possono essere referenziati prima di essere definiti**, nemmeno
  all'interno dello stesso file. Per funzioni mutuamente ricorsive, dichiara prima una di esse con
  `defsignature` ([Riferimento della sintassi 3.2](../reference/syntax.md#32-defsignature--dichiarazioni-anticipate)).

## 7. Ordine di esecuzione

Eseguire `typl main.typl` procede in questo ordine:

1. `main.typl` e ogni file di cui fa `use` vengono letti e controllati nei tipi. **Se c'è un errore di
   tipo in un punto qualsiasi, non viene eseguito nulla.**
2. Le espressioni di primo livello dei moduli importati con `use` vengono eseguite prima di quelle dei
   moduli che li usano.
3. Le espressioni di primo livello di `main.typl` vengono eseguite dall'alto verso il basso.

Se raccogli il punto di ingresso del programma in una funzione `main` e chiami `(main)` alla fine del
file, lo stesso file può essere usato anche per la
[compilazione AOT](compile.md#3-creazione-di-un-eseguibile-con-la-compilazione-aot).

## 8. Differenze rispetto a `load`

`(load "path")`, come il `load` di Common Lisp, legge il contenuto di un file **nel namespace corrente
così com'è**. Non lo racchiude in un modulo, e `pub` non ha alcun ruolo. Usalo per cose come leggere
un file di impostazioni o ricaricare un file locale nel REPL. Per suddividere un programma in parti,
usa `use`.
