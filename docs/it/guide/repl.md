<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# Il REPL

Avviando `typl` senza argomenti si entra nel REPL (modalità interattiva). Questo documento descrive
cosa offre il REPL oltre alla valutazione delle espressioni: i suoi comandi, l'aiuto durante la
digitazione, il riferimento ai risultati precedenti, l'interruzione di un calcolo e l'indagine
dopo un errore.

## Avvio e uscita

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Si esce con `:quit` (abbreviato `:q`) o `Ctrl+D`. La cronologia dell'input viene salvata in
`~/.typl_history`, così al successivo avvio la si può richiamare con `Ctrl+P` / `Ctrl+N` o
`Ctrl+R` (ricerca nella cronologia).

All'avvio, prima del primo prompt, viene eseguito quanto segue, in ordine. Nessuno di questi passi
stampa i valori che produce.

1. Il contenuto di `~/.typlrc`, se esiste. Con `--no-init` non viene letto.
2. `--eval FORM` e `--load FILE` dalla riga di comando, nell'ordine in cui sono scritti. Entrambi
   possono essere ripetuti quante volte si vuole.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` e `--no-init` servono solo ad avviare il REPL. Darli quando si esegue un file è
un errore.

Il REPL parte nel modulo `user`. Quando ci si sposta in un altro modulo con `(in-module foo)`, il
prompt mostra quel modulo.

```
typl> (in-module foo)
typl user::foo>
```

## Comandi del REPL

Un `:nome` scritto all'inizio di una riga è un comando del REPL. `:help` ne mostra l'elenco. Un
`:nome` che non è nell'elenco non è un comando: viene valutato come una normale parola chiave.

| Comando | Significato |
|---|---|
| `:help` (`:h`, `:?`) | elencare i comandi |
| `:type FORM` (`:t`) | stampare il tipo di FORM senza valutarlo |
| `:describe NAME` (`:d`) | stampare il tipo di definizione, il suo tipo, i campi o le varianti, i metodi, i trait che implementa, dove è stata definita e la sua stringa di documentazione |
| `:apropos TEXT` | cercare in tutti i moduli le definizioni il cui nome contiene TEXT (senza distinguere maiuscole e minuscole) |
| `:src NAME` | stampare il sorgente di una definizione |
| `:who-calls NAME` | stampare le funzioni e i metodi che chiamano la funzione o il metodo NAME |
| `:inspect FORM` (`:i`) | valutare FORM e aprirne il valore secondo il suo tipo |
| `:backtrace` (`:bt`) | stampare le chiamate in corso quando la riga precedente è fallita, dalla più interna verso l'esterno |
| `:locals [N]` | stampare gli argomenti della chiamata N di `:backtrace` (la 0 se omesso) |
| `:cd [DIR]` | cambiare la cartella corrente in DIR (la cartella home se omesso) |
| `:pwd` | stampare la cartella corrente |
| `:quit` (`:exit`, `:q`) | uscire dal REPL |

NAME può essere un nome come `inner` o un percorso come `std::seq::reverse` o `Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Somma 1 a x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Somma 1 a x.
typl> :src inner
(defun inner ((x int) (s string)) int "Somma 1 a x." (+ x 1))
```

`:src` mostra una definizione caricata da un file prendendola da quel file, e una definizione
digitata nel REPL prendendola dal testo digitato. Per le definizioni della libreria standard non è
registrata la posizione nel sorgente, quindi il loro sorgente non si può mostrare.

`:who-calls` cerca nei corpi delle funzioni della sessione corrente. Per una funzione generica
trova solo gli usi istanziati con tipi concreti.

`:cd` cambia la cartella in cui cerca `(load ...)`, ma `use` continua a cercare i file nella
cartella in cui è stato avviato il REPL.

## Aprire un valore (`:inspect`)

`:inspect` apre un valore secondo il suo tipo. Una struct si apre nei suoi campi, un enum (compreso
`Option`) nei campi della sua variante, un `Vector` nei suoi elementi e una `HashTable` nelle sue
chiavi e valori. Un numero entra in quella parte, `u` risale di un livello e `q` termina.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

Le liste e i `Sexpr` non vengono aperti: sono stampati così come sono.

## Aiuto durante la digitazione

Quando si digita in un terminale è disponibile quanto segue.

- `Tab` completa i nomi (le definizioni e la sintassi come `if` e `defun`), i percorsi con `::`, i
  comandi del REPL a inizio riga e, dentro una stringa, i nomi di file.
- Dopo il nome di una funzione seguito da uno spazio, il tipo della funzione viene mostrato
  attenuato in fondo alla riga.
- Viene evidenziata la parentesi corrispondente a quella accanto al cursore.
- Se si preme Invio prima che una forma sia finita, la riga successiva inizia rientrata secondo il
  numero di parentesi aperte.

## Risultati precedenti (`:*`, `:**`, `:***`)

Nel REPL, `:*` è l'ultimo valore stampato, `:**` quello prima e `:***` quello ancora prima.
Ciascuno ha il tipo del suo valore.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- Un input che non ha stampato alcun valore (come una definizione) o che è fallito non fa avanzare
  i risultati.
- Scrivere `:**` o `:***` prima che ci siano altrettanti risultati è un errore.
- Un `':*` quotato è una normale parola chiave.
- Un `:*` scritto nel corpo di una funzione continua a riferirsi al risultato del momento in cui la
  funzione è stata definita. Il `*` di Common Lisp legge il valore al momento della chiamata, ma
  typelisp lo fissa alla definizione perché abbia un tipo.
- Fuori dal REPL (eseguendo un file, o in `~/.typlrc`, `--load` e `--eval`), `:*` è una normale
  parola chiave.

## Interrompere un calcolo (`Ctrl+C`)

Premendo `Ctrl+C` durante una valutazione si interrompe quel calcolo e si torna al prompt. Ciò che
è stato definito prima resta definito. Funziona sia per un ciclo nell'interprete, sia per un ciclo
in codice compilato, sia durante un'attesa in `sleep` o `wait`. Le operazioni di pulizia degli
`unwind-protect` in corso vengono eseguite.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` premuto durante la digitazione scarta solo la riga che si sta scrivendo.

## Indagare dopo un errore (`:backtrace`, `:locals`)

Dopo che una valutazione si è fermata con un errore, `:backtrace` (`:bt`) stampa le chiamate di
funzione in corso in quel momento, dalla più interna verso l'esterno. `:locals N` stampa gli
argomenti della chiamata N secondo i loro tipi dichiarati.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- Per una chiamata fatta dall'ultima espressione di una funzione (una chiamata di coda), la
  funzione chiamante non viene mostrata. È ciò che permette a un ciclo ricorsivo di coda di usare
  lo spazio di una sola chiamata, per quanti giri faccia.
- Le chiamate all'interno di funzioni compilate e le funzioni anonime (`lambda`) non vengono
  mostrate.
- `:locals` mostra solo gli argomenti, non le variabili legate con `let`.
- Valutando la riga successiva, la registrazione dell'errore precedente viene scartata.
