<!-- translated-from: docs/ja/reference/functions/format.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
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

Gli **elementi** di un argomento lista (`~{`, `~:{`, `~<...~:>`) sono `Sexpr`, e né il loro numero né il
tipo di ciascun elemento si possono conoscere dai tipi. I requisiti sugli elementi (un intero per `~d`, e
così via) e gli elementi mancanti vengono controllati quando i valori arrivano, e sono errori a runtime
(non si passa mai a una rappresentazione diversa).

Le regole permissive di CL non sono adottate. Passare un non intero a `~d` e farlo stampare come `~a`, o
`~:[` che tratta qualsiasi valore come booleano, non vengono reinterpretati in quel modo; sono errori di
tipo.

## 2. Output (consumano un argomento)

| Direttiva | Parametri / modificatori | Significato |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=giustifica a destra | Estetica (il `princ` di CL; stringhe senza virgolette). L'argomento può essere di qualsiasi tipo |
| `~s` | Come sopra | Standard (il `prin1` di CL; una forma che può essere riletta). L'argomento può essere di qualsiasi tipo |
| `~w` | — | Il `write` di CL. Fa il pretty-print se `*print-pretty*` è vero, altrimenti come `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=gruppi di cifre, `@`=sempre un segno | Interi in decimale/binario/ottale/esadecimale. L'argomento è un intero |
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
| `~/name/` | Chiamata di metodo (capitolo 5. I flag `:`/`@` vengono passati al metodo. Non accetta parametri) |

Le seguenti direttive di CL non sono supportate (sono errori in fase di controllo).

- `~?` e `~@?`: prendono una stringa di controllo come argomento a runtime, quindi gli argomenti che le
  sue direttive consumano non si possono controllare. Scrivi quelle direttive direttamente nella stringa
  di controllo.
- `~@[...~]`: verifica se un argomento non è nil, ma questo linguaggio non ha nil. Usa
  `~:[false~;true~]`, che ramifica su un `bool`.
- `~{~}` con corpo vuoto: prende il corpo da un argomento a runtime. Scrivi le direttive dentro le
  graffe.

## 5. `~/name/`

**Una differenza rispetto a CL: il nome viene cercato non come funzione globale ma come metodo del tipo
dell'argomento stesso.** Il metodo ha la forma `((self Self) (colon bool) (at bool)) → string`, e i `:`/`@`
della direttiva vengono passati così come sono.

Il modo di CL di cercarlo come funzione globale non si può implementare in modo sicuro in questo
linguaggio. Anche con una stringa di controllo letterale, il tipo degli elementi di un argomento lista
(dentro `~{`) non è noto in fase di controllo, e cercare una funzione solo per nome potrebbe chiamare una
funzione pensata per un altro tipo. Scegliere in base al tipo del valore significa che il metodo viene
controllato nei tipi esattamente per quel tipo, il che è sicuro (lo stesso meccanismo di `print-object`).
Funziona anche per valori come `string`/`bool`/`char`/`symbol`/liste. Solo per gli interi, la cui larghezza
non si può capire dal valore, è un errore **quando più di un tipo intero definisce un metodo con quel
nome**.

Non si sa a quale argomento si applichi, ma si sa quali metodi potrebbe chiamare. Il checker raccoglie ogni
`~/name/` dalla stringa di controllo letterale e registra, tra i tipi degli argomenti in quel punto di
chiamata, quelli che hanno un metodo della forma indicata. Quindi **se nessuno dei tipi degli argomenti ha
il metodo, è un errore in fase di controllo** (non a runtime), e funziona anche negli eseguibili AOT.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```
