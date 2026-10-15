<!-- translated-from: docs/ja/reference/functions/format.md @ 7aae962c61e6f36448edc8bab6b37ff177aefdb3 -->
# Direttive di formato

Le direttive scritte nelle stringhe di controllo di `print`/`println`/`format`. Coprono quasi tutte le
direttive di `format` di CL. Le funzioni stesse sono descritte in
[Stampa](printing.md#1-print--println--format).

## 1. Come si scrivono le direttive

Ogni direttiva è `~`, poi **parametri di prefisso** facoltativi (separati da virgole: un intero / `'c` (un
carattere) / `v` (preso dall'argomento successivo) / `#` (il numero di argomenti rimasti)), poi i
**modificatori** facoltativi `:` e `@`, poi il carattere della direttiva, in quest'ordine. I caratteri
delle direttive non distinguono maiuscole e minuscole.

La stringa di controllo deve essere un letterale ([Stampa](printing.md#1-print--println--format)). Inoltre,
in fase di controllo vengono verificate le seguenti cose.

- **Il numero e i tipi degli argomenti.** Per ogni direttiva che consuma un argomento: se ne è rimasto
  uno e se il suo tipo è accettato (le note "argomento" nelle tabelle seguenti). Dove il percorso dipende
  da valori a runtime, come lo spostamento con `~*`, quale clausola di `~[` viene presa, se `~^` scatta o
  quante volte `~@{` si ripete, viene controllato **ogni percorso**. Gli argomenti in eccesso vanno bene
  (come in CL).
- **Parametri e modificatori.** Un modificatore non accettato, troppi parametri e valori fuori intervallo
  (una larghezza negativa, una base diversa da 2-36, un intero dove è atteso un carattere e così via)
  sono errori. Non vengono mai ignorati o arrotondati in silenzio.

Gli **elementi** di un argomento lista (`~{`, `~:{`, `~<...~:>`) sono `Sexpr`; né il loro numero né il
tipo di ciascun elemento si ricavano dai tipi. I requisiti sugli elementi (un carattere per `~c`, ecc.)
e gli elementi mancanti si controllano all'arrivo dei valori e sono errori a tempo di esecuzione (non si
passa mai a un'altra rappresentazione al loro posto).

`~d` `~b` `~o` `~x` e `~r` con base stampano un non intero come farebbe `~a`, in decimale, come in CL
(`(format false "~5d" 1/2)` è `"  1/2"`). Le altre regole permissive di CL non sono adottate: che
`~:[` tratti qualsiasi valore come booleano non viene reinterpretato così; è un errore di tipo.

## 2. Output (consumano un argomento)

| Direttiva | Parametri / modificatori | Significato |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=giustifica a destra | Estetica (il `princ` di CL; stringhe senza virgolette). L'argomento può essere di qualsiasi tipo |
| `~s` | Come sopra | Standard (il `prin1` di CL; una forma che può essere riletta). L'argomento può essere di qualsiasi tipo |
| `~w` | — | Il `write` di CL. Fa il pretty-print se `*print-pretty*` è vero, altrimenti come `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=gruppi di cifre, `@`=sempre un segno | Interi decimali/binari/ottali/esadecimali. L'argomento può essere qualsiasi (un non intero si stampa come con `~a`, in decimale) |
| `~r` | `~radix,mincol,padchar,commachar,interval` (con una base) oppure nessuno | Con una base, quella base (da 2 a 36). Senza: `~r`=cardinale inglese, `~:r`=ordinale inglese, `~@r`=numeri romani, `~:@r`=numeri romani antichi. L'argomento è un intero |
| `~p` | `:`=torna indietro di uno, `@`=y/ies | Plurali (`~p`→"s", `~@p`→"y"/"ies"). L'argomento è un intero |
| `~c` | `:`=nome, `@`=sintassi `#\` | Un carattere. L'argomento è un `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=segno | Virgola fissa. L'argomento è un numero |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=segno | Notazione esponenziale. L'argomento è un numero. I parametri exponent-digits, scale e overflowchar di CL non sono supportati (indicarli è un errore) |
| `~g` | `@`=segno | Virgola mobile generale. L'argomento è un numero. Non accetta parametri |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Notazione monetaria. L'argomento è un numero |

## 3. Output (non consumano argomenti)

| Direttiva | Significato |
|---|---|
| `~%` | A capo (`~n%` per n di essi) |
| `~&` | fresh-line (un a capo a meno che non si sia all'inizio di una riga; `~n&`) |
| `~\|` | Interruzione di pagina (form feed) |
| `~~` | Un `~` letterale (`~n~` per n di essi) |
| `~t` | Tabulazione (`~colnum,colincT`. Se si è già alla colonna colnum o oltre, avanza di un multiplo di colinc; non si muove se colinc è 0. `@`=relativa. `:`=una tabulazione relativa all'inizio del blocco logico, che funziona solo durante il pretty-printing) |
| `~_` | A capo condizionale (pretty; semplice=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Indentazione (pretty; `~ni`=inizio del blocco + n / `~n:i`=colonna corrente + n) |
| `~<newline>` | Ignora l'a capo (`:`=mantiene gli spazi, `@`=mantiene l'a capo) |

Come in CL, le direttive del pretty-printer (`~_` `~i` `~:t` `~<...~:>`, e il percorso di pretty-printing
di `~a`/`~s`/`~w`) non fanno nulla quando `*print-pretty*` è falso. È falso per impostazione predefinita.

## 4. Strutture di controllo

| Direttiva | Significato |
|---|---|
| `~(...~)` | Conversione di maiuscole/minuscole (`~(` minuscolo, `~:(` iniziale maiuscola per ogni parola, `~@(` iniziale maiuscola solo per la prima parola, `~:@(` tutto maiuscolo) |
| `~[...~;...~]` | Selezione condizionale (ramifica su un argomento intero. Con `~n[`, `~v[` o `~#[`, ramifica su quel valore e non prende argomenti. `~:;`=la clausola predefinita, solo come ultima clausola). `~:[false~;true~]` ramifica su un argomento `bool` e ha esattamente due clausole |
| `~{...~}` | Iterazione (percorre un argomento lista. `~:{`=per sottolista, `~@{`=sugli argomenti rimanenti, `~:@{`=su ciascuna lista tra gli argomenti rimanenti, `~^`=uscita, `~:}`=esegue una volta anche se vuota). Un corpo che non consuma alcun argomento in un'iterazione è un errore (non terminerebbe mai) |
| `~<...~;...~>` | Giustificazione (distribuisce i segmenti su `~mincol` colonne. `:`/`@`=riempimento alle estremità) |
| `~<...~;...~:>` | **Blocco logico** (chiuso con `~:>`; una cosa diversa dalla giustificazione qui sopra). Il primo segmento è il prefisso e l'ultimo è il suffisso (entrambi solo stringhe letterali). Con il separatore `~@;`, il prefisso è un **prefisso per riga**. `~:<` imposta come predefiniti prefisso/suffisso `(`/`)`. L'argomento è una lista (`~@<` usa al suo posto gli argomenti rimanenti) |
| `~*` | Salto di argomenti (`~n*`=avanti di n, `~:*`=indietro, `~@*`=a una posizione assoluta) |
| `~/name/` | Chiamata di una funzione o di un metodo (capitolo 5; i flag `:`/`@` passano a ciò che viene chiamato; non prende parametri) |

Le seguenti direttive di CL non sono supportate (sono errori in fase di controllo).

- `~?` e `~@?`: prendono una stringa di controllo come argomento a runtime, quindi gli argomenti che le
  sue direttive consumano non si possono controllare. Scrivi quelle direttive direttamente nella stringa
  di controllo.
- `~@[...~]`: verifica se un argomento non è nil, ma questo linguaggio non ha nil. Usa
  `~:[false~;true~]`, che ramifica su un `bool`.
- `~{~}` con corpo vuoto: prende il corpo da un argomento a runtime. Scrivi le direttive dentro le
  graffe.

## 5. `~/name/`

`~/name/` risolve `name` con le stesse regole di una chiamata `(name arg colon at)` scritta in quel punto,
e stampa la stringa restituita dalla chiamata. Consuma un argomento. `colon` e `at` sono `bool` che dicono
se la direttiva porta `:` e `@`.

```lisp
(defstruct point (x i32) (y i32))
(impl point
  (brief (self (colon bool) (at bool)) string
    (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y))))
(defun yen ((n int) (colon bool) (at bool)) string
  (format false "~a yen" n))

(println "~a" (format false "~/brief/"  (make-instance 'point :x 3 :y 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (make-instance 'point :x 3 :y 4)))   ; => <3,4>
(println "~a" (format false "~/yen/" 500))                                    ; => 500 yen
```

- La risoluzione del nome è la stessa di una chiamata ([Riferimento della sintassi 3.10](../syntax.md#310-module--use--namespace)). Se il
  tipo dell'argomento ha un metodo `name`, viene chiamato quello; altrimenti la funzione visibile `name`.
  Una funzione di un modulo si può indicare con il suo percorso (`~/m::shout/`).
- Si può chiamare una funzione o un metodo scritto in typelisp che riceve l'argomento, `colon` e `at` così
  come sono e restituisce una `string`. Le funzioni integrate non si possono chiamare.
- **Che cosa chiamare si decide al controllo.** Il controllore trova ogni argomento che può raggiungere quel
  `~/name/`, tenendo conto degli spostamenti di `~*` e `~[`, e risolve il nome per il tipo di ciascuno. Se
  uno di essi non si risolve è un errore al controllo, e lo è anche il caso in cui gli argomenti possibili
  chiamerebbero due definizioni diverse. A tempo di esecuzione non si cerca alcun nome, quindi funziona allo
  stesso modo negli eseguibili AOT.
- **Non si può usare dentro `~{`, `~:{`, `~:@{` né `~<...~:>`** (errore al controllo). Il tipo degli
  elementi di un argomento lista è `Sexpr`, che non può stabilire che cosa chiamare. `~@{` percorre gli
  argomenti restanti della chiamata, i cui tipi sono noti, quindi lì si può usare.

La differenza rispetto a CL: CL cerca il nome come funzione globale. Qui la risoluzione è la stessa di una
chiamata, quindi un metodo del tipo dell'argomento viene scelto prima di una funzione con lo stesso nome.
