<!-- translated-from: docs/ja/guide/ffi.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# FFI C (defffi)

Questa guida spiega come chiamare funzioni C da typelisp. L'elenco dei tipi che si possono dichiarare
e le restrizioni si trovano nel [Riferimento della sintassi 3.3](../reference/syntax.md#33-defffi--dichiarare-funzioni-c-ffi).

## 1. Dichiarare e chiamare una funzione

`defffi` dichiara il nome e i tipi di una funzione C.

```lisp
(defffi (c-getpid "getpid") () i32)            ; the typelisp name and the C symbol name
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; look it up in libm
```

Le chiamate vanno racchiuse in `(unsafe ...)`.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` è necessario perché il compilatore non può verificare che i tipi dichiarati corrispondano ai
tipi reali sul lato C. Scrivere `unsafe` significa che tu, chi scrive, ti assumi la responsabilità di
quel controllo. Dimenticarlo produce un errore che lo spiega.

## 2. Scrivere un wrapper sicuro

L'uso previsto è confinare `unsafe` in un unico punto e presentare all'esterno una funzione ordinaria.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; the caller needs no unsafe
(str-len "hello")  ; => 5
```

## 3. Corrispondenza dei tipi

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Interi della stessa larghezza |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (anche `size_t`, `int64_t` e così via) |
| `ptr` | Qualsiasi puntatore (`void *`, `FILE *` e così via) |
| `(ptr T)` | Un puntatore a `T` ([sezione 7](#7-strutture-c)) |

### Stringhe

- Una `string` che passi viene copiata in una stringa C terminata da NUL, che viene liberata dopo il
  ritorno della chiamata. Un NUL in mezzo alla stringa è un errore.
- Anche il risultato di una funzione che restituisce `string` viene copiato. La memoria sul lato C non
  viene liberata. Per le funzioni che restituiscono una stringa che il chiamante deve liberare (come
  `strdup`), prendi il risultato come `ptr` e liberalo tu stesso con `free`.
- Se una funzione dichiarata per restituire `string` restituisce NULL, è un errore. Prendi come `ptr`
  il risultato delle funzioni che possono restituire NULL (come `getenv`).

### `c-long` / `c-ulong` / `ptr`

Questi tipi esistono solo per passare valori attraverso il confine con C e **non supportano alcuna
aritmetica**. Per usarne uno come intero typelisp, convertilo con `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int does not lose any of the 64-bit value
(try-as i32 (unsafe (c-strlen s)))  ; none if it does not fit in an i32
(unsafe (c-malloc 16))              ; integer literals can be passed as they are
```

Un `ptr` è un valore da restituire alle funzioni C. Non c'è modo di leggere dal lato typelisp ciò a cui
punta.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Questi tipi possono comparire solo come argomenti di funzione, valori di ritorno e variabili locali.
Non possono essere campi di strutture, variabili globali o argomenti di tipo di `Vector` e simili.

## 4. Indicare una libreria

Senza `:library`, il simbolo viene cercato in ciò che è già collegato al processo (libc e così via). Le
funzioni di altre librerie richiedono `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Un nome breve come `"sqlite3"` viene cercato come `libsqlite3.dylib`, poi `libsqlite3.so`.
- Un nome che contiene `/` viene trattato come un percorso.
- Se il simbolo dichiarato non viene trovato, l'errore lo nomina.

## 5. Compilazione AOT

I programmi che usano `defffi` possono essere trasformati in eseguibili così come sono con
[`compile-file`](compile.md#3-creazione-di-un-eseguibile-con-la-compilazione-aot). Le librerie indicate con
`:library` vengono aggiunte automaticamente in fase di collegamento, quindi `compile-file` non
richiede argomenti aggiuntivi.

## 6. Callback

Puoi passare una funzione typelisp a una funzione C e farla richiamare. Scrivi un tipo funzione tra i
tipi degli argomenti di `defffi`, e nella chiamata metti in quella posizione un nome di funzione o
un'espressione `lambda`.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") returns p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Si possono passare solo funzioni **senza variabili libere**. Funzioni di primo livello, `lambda` e
  funzioni locali `labels` vanno tutte bene, ma fare riferimento a una variabile locale di uno scope
  circostante è un errore in fase di controllo dei tipi. C passa solo gli argomenti dichiarati, quindi
  non c'è modo di consegnare le variabili catturate. Per mantenere uno stato, usa variabili globali.
- Non si può passare una variabile che contiene una funzione. Scrivi al suo posto un nome di funzione
  o un'espressione `lambda`.
- Un `panic` o un `throw` dentro il callback raggiunge il chiamante dopo il ritorno della funzione C.
- Il callback può essere chiamato solo mentre la funzione C chiamata da typelisp è in esecuzione. Non
  può essere usato da cose come `atexit` o gli handler di segnale.

## 7. Strutture C

Per passare a una funzione C qualcosa come un array di strutture, dichiara con `def-c-struct` una
struttura con lo stesso layout di quella in C e allocala dentro `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; declared inside a top-level unsafe

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; four items, all zero
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` alloca `n` valori di `T` e restituisce un `(ptr T)`. `(c-ref p i)` è un puntatore
  all'`i`-esimo, `p::field` è un campo e `(c-deref p)` è ciò a cui punta un puntatore a uno scalare
  come `i32`. Tutti possono essere scritti con `setf`.
- `(as ptr p)` lo trasforma in un `ptr` non tipizzato, da passare alle funzioni C che accettano un
  `void *`.
- La dimensione di `item` (qui 8) e la posizione di ciascun campo sono determinate dalle stesse regole
  di C.

### Durata della memoria allocata

La memoria allocata viene liberata quando il controllo esce dall'`unsafe` più esterno di quella
funzione. Lo stesso accade quando si esce con `panic` o `throw`. Per questo motivo un valore `(ptr T)`
non può essere portato fuori dall'`unsafe`. Farne il valore dell'`unsafe`, catturarlo in una chiusura,
passarlo a un `task` e lanciarlo con `throw` sono tutti errori di tipo. Copia i valori che vuoi usare
all'esterno in numeri o in una `defstruct` dentro l'`unsafe`.

Quando alloci dentro una `lambda` o una funzione `labels`, scrivi un `unsafe` dentro quella funzione.

### Memoria allocata da C

Un puntatore ricevuto da C come `(ptr T)` (un valore di ritorno di `defffi`, un argomento di callback e
così via) è un errore a meno che non punti dentro memoria allocata con `c-alloc`. Dichiara con il
`ptr` non tipizzato le funzioni che ricevono memoria allocata da C con `malloc`, o NULL.

## 8. Che cosa non si può fare

- Le **funzioni variadiche** (`printf` e simili) non si possono dichiarare. La parte variadica viene
  passata con regole diverse da quelle degli argomenti fissi. Dichiara un nome distinto per ciascun
  numero di argomenti che usi.
- **Passare o restituire strutture per valore** non è possibile. Usa funzioni che passano puntatori.
- Le **dichiarazioni generiche** non sono possibili.
- **Lo stesso nome di una funzione predefinita** non può essere usato.
- **Non si possono passare come valori di funzione.** Non puoi passarne una come in `(map xs c-abs)`;
  racchiudila in una `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
