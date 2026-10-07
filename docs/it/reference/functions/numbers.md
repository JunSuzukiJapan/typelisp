<!-- translated-from: docs/ja/reference/functions/numbers.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Numeri

Operazioni su interi, numeri in virgola mobile, razionali, numeri complessi e booleani, e altre funzioni
legate ai numeri. Per come leggere le forme di chiamata, si veda [Funzioni predefinite](README.md).

## 1. Interi a larghezza fissa

Ci sono sette tipi interi: **`int`** (l'`integer` di CL: precisione arbitraria, e tipo predefinito dei
letterali interi senza annotazione; capitolo 3), e i tipi a larghezza fissa `i8` `i16` `i32` `u8` `u16`
`u32`. Per quale tipo si risolve un'operazione lo decide il tipo del primo argomento (sono indipendenti
l'uno dall'altro, senza conversioni implicite). **Non esiste un tipo intero a 64 bit.** Un valore a
runtime è una parola i cui bit bassi sono un tag, quindi restano solo 63 bit per un intero immediato, e
un tipo che dichiarasse 64 bit dovrebbe scartare da qualche parte il bit più alto. `int` diventa un
bignum una volta superati quei 63 bit, quindi se la larghezza non importa, usa `int`. La tabella
seguente riguarda i sei tipi a larghezza fissa (la tabella per `int` è nel capitolo 3).

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | Le quattro operazioni aritmetiche. `/` tronca verso zero e va in panic per divisione per zero |
| `mod` | `(mod a b)` | `(T,T)→T` | Resto (il `mod` di CL, **divisione con arrotondamento per difetto**: il segno segue il divisore. `(mod -7 3)`→`2`). Va in panic per divisione per zero |
| `rem` | `(rem a b)` | `(T,T)→T` | Resto (il `rem` di CL, **divisione con troncamento**: il segno segue il dividendo. `(rem -7 3)`→`-1`). Va in panic per divisione per zero |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Corrispondono ai `floor`/`ceiling`/`round`/`truncate` a due argomenti di CL (`(floor 7 2)`→quoziente 3, resto 1). Invece dei valori multipli, restituiscono quoziente e resto in una `cons-cell` (`car`=quoziente, `cdr`=resto). `round-div` arrotonda i casi a metà al pari, come fa CL |
| `abs` | `(abs x)` | `T→T` | Valore assoluto |
| `signum` | `(signum x)` | `T→T` | Segno (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Massimo comune divisore |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Minimo comune multiplo (0 se uno dei due è 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | Il maggiore / minore (tre o più argomenti vengono espansi dallo zucchero variadico del capitolo 8) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Confronto |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Tutti uguali a `=` (per numeri dello stesso tipo non c'è differenza) |
| `int->float` | `(int->float x)` | `T→f64` | Conversione di allargamento a `f64` |
| `int->int` | `(int->int x)` | `T→int` | Conversione di allargamento a `int` (sempre esatta). Ciò che fa `(as int x)` |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Conversione di allargamento a `ratio` (sempre esatta) |
| `int->char` | `(int->char x)` | `T→char` | Interpreta il valore come valore scalare Unicode. Va in panic su un valore non valido |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Una versione di `int->char` che restituisce `None` in caso di fallimento |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Conversione di larghezza. I valori che non stanno vengono troncati (come l'`as` di Rust) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | La stessa conversione posta come domanda. `None` se il valore non sta in quella larghezza |

Queste conversioni sono anche ciò che fanno le forme speciali `(as Type x)`/`(try-as Type x)`
([Riferimento della sintassi](../syntax.md#7-altre-forme-speciali)). Le operazioni sui bit
(`logand`/`ash`/`ldb` e così via) e i predicati (`zerop`/`evenp` e così via) hanno la stessa forma tra i
tipi, quindi sono raccolti nei capitoli 11 e 9.

`i8` `i16` `u8` `u16` `u32` hanno esattamente la tabella di questo capitolo, e `f32` ha esattamente la
tabella di `f64` del capitolo 4.

**Un nome di tipo significa la sua larghezza e il suo segno, nulla di più.** `i32` significa "tratta 32
bit come con segno" e `u32` significa "tratta 32 bit come senza segno". `(+ (the u8 200) (the u8 100))`
è `44`, `(+ 2147483647 1)` (come `i32`) è `-2147483648`, e `(lognot (the u32 0))` è `4294967295`. `f32` è
lo stesso: un vero binary32. `(/ (the f32 1.0) (the f32 3.0))` viene stampato come `0.33333334`, un valore
diverso dal risultato `f64` `0.3333333333333333`.

Il catalogo derivato di CL (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` e i predicati del capitolo 9)
esiste per `int`/`i32`/`f64`/`ratio`. Se ti serve per un'altra larghezza, passa con `(as int x)` /
`(as i32 x)` (le conversioni di larghezza esistono per ogni coppia).

## 2. Parole grezze al confine con C (`ptr` / `c-long` / `c-ulong`)

Tre tipi usati solo per passare valori da e verso funzioni C dichiarate con
[`defffi`](../syntax.md#33-defffi--dichiarare-funzioni-c-ffi). `ptr` è un puntatore opaco, e
`c-long` / `c-ulong` sono i `long` / `unsigned long` di C. Per renderne uno un valore bisogna trovarsi
dentro `(unsafe ...)`.

**Non c'è aritmetica.** Non si applica nulla della tabella del capitolo 1: non si può scrivere né
`(+ p 1)` né `(< n m)`. Sono parole da consegnare a C, non tipi con cui calcolare, quindi per calcolare
passa a un tipo con larghezza. `c-long` / `c-ulong` hanno solo conversioni:

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | Le stesse conversioni di larghezza del capitolo 1. I valori che non stanno vengono troncati |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | La stessa conversione posta come domanda |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | La via d'ingresso, dall'altra parola grezza e dai tipi interi del capitolo 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Come sopra |
| `int->int` | `(int->int x)` | `T→int` | **Sempre esatta**. Il modo onesto di leggere un `size_t` che non sta in un `i32` |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` sono ciò che queste fanno, e le
conversioni esistono per ogni coppia con i tipi interi del capitolo 1. `ptr` non ha nemmeno questa
tabella: non è previsto alcun modo di leggere un puntatore come numero. È un valore che viene solo
passato, ricevuto e consegnato a un'altra funzione C.

**Non si possono nemmeno stampare.** `(println "~a" x)` non accetta una parola grezza (non ha una
rappresentazione `Sexpr`), quindi passala prima a un tipo con larghezza, come in
`(println "~a" (as int n))`.

"Non esiste un tipo intero a 64 bit" dall'inizio del capitolo 1 vale anche per questi tre. Vale
**perché non possono essere memorizzati**: non possono essere un campo di `defstruct`, una `defvar`,
dentro un argomento di tipo o dentro una `Sexpr`, quindi sono parole che attraversano una funzione solo
come argomenti, valori di ritorno e variabili locali. Per i dettagli, si veda il
[Riferimento della sintassi](../syntax.md#ptr--c-long--c-ulong--parole-macchina-grezze).

## 3. Interi a precisione arbitraria `int`

L'`integer` di CL, e l'**intero** di questo linguaggio: i letterali interi senza annotazione hanno
questo tipo, e le funzioni predefinite che restituiscono un numero, come `length` e `char->int`,
restituiscono questo tipo. Un valore è tenuto come valore immediato a 63 bit (fixnum) finché sta, viene
promosso automaticamente a bignum quando il risultato di un'operazione non sta più, e torna a valore
immediato quando sta di nuovo. `eq` è sempre identità di valore dentro l'intervallo dei fixnum, ed
`eql`/`=` sono identità numerica su tutto l'intervallo. È un tipo diverso dai tipi interi a larghezza
fissa (capitolo 1), senza conversione implicita: `(as int x)` è l'allargamento esatto da una larghezza
fissa, e `(as i32 n)` / `(try-as i32 n)` sono il troncamento / controllo a partire da `int` (lo stesso
significato di `int->W` / `try-int->W` nel capitolo 1).

Anche la variante intera di `Sexpr` è semplicemente `int` (`(int n)` accetta sia fixnum sia bignum).

Le funzioni predefinite che prendono un indice o un conteggio (`substring`, `get` di `Vector`, il
conteggio di scorrimento di `ash` e così via) accettano `int`, ma passare un valore che non sta in un
fixnum è un errore a runtime ("an integer argument does not fit a fixnum").

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Non vanno mai in overflow (promuovono) |
| `/` | `(/ a b)` | `(int,int)→int` | Tronca verso zero. Va in panic per divisione per zero |
| `mod` | `(mod a b)` | `(int,int)→int` | Resto della divisione con arrotondamento per difetto (il segno segue il divisore) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Tutti `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Come nel capitolo 11 (complemento a due con infiniti bit) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Come nel capitolo 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Troncamento / controllo. `W` è una delle sei larghezze oppure `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Identità (dal lato delle larghezze fisse e delle parole C, `int->int` allarga; capitolo 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | Stessa forma del capitolo 1. `expt` accetta solo esponenti non negativi |

## 4. Numeri in virgola mobile (`f64` / `f32`)

`f32` ha la stessa tabella.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. La divisione per zero non va in panic; dà `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Resto della divisione con arrotondamento per difetto (come in CL; il segno segue il divisore. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Resto della divisione con troncamento (come in CL; il segno segue il dividendo. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Confronto |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Tutti uguali a `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Potenza |
| `abs` | `(abs x)` | `f64→f64` | Valore assoluto |
| `signum` | `(signum x)` | `f64→f64` | Segno (`1.0`/`-1.0`; `±0.0`/`NaN` vengono restituiti così come sono. Come in CL, a differenza del `signum` di Rust) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | Il maggiore / minore (tre o più argomenti vengono espansi dallo zucchero variadico del capitolo 8) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Operazioni unarie |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Funzioni trascendenti. `log` è il logaritmo naturale |
| `log` (due argomenti) | `(log x base)` | `(f64,f64)→f64` | Logaritmo in una data base. Espanso in `(/ (log x) (log base))` (capitolo 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Corrispondono alle versioni a due argomenti di CL (`(floor 7.0 2.0)`→quoziente 3, resto 1). Lo stesso progetto delle funzioni omonime del capitolo 1 (`car`=quoziente, `cdr`=resto) |
| `float->int` | `(float->int x)` | `f64→int` | Converte in `int` troncando verso zero (il `truncate` di CL; esatto per valori finiti di qualsiasi dimensione). Va in panic su infinito e NaN. Per una larghezza fissa, usa `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Converte in `ratio` come razionale binario esatto (il `rational` di CL) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Converte tra larghezze in virgola mobile. `float->f32` arrotonda al più vicino, `float->f64` è sempre esatta. Ciò che fa `(as f32 x)` |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | La stessa conversione posta come domanda. `none` se l'arrotondamento cambia il valore (l'allargamento a `f64` è sempre `some`). Ciò che fa `(try-as f32 x)` |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | Le funzioni omonime di CL. Alias di `floor`/`ceiling`/`round`/`truncate` sopra: in CL quelle senza prefisso restituiscono interi, quindi quelle con prefisso `f` corrispondono al comportamento di questo linguaggio |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | Rispettivamente 2 / 53 / 53 (solo la precisione di `0.0` è 0). `f64` è sempre IEEE-754 binary64, quindi sono costanti |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` oppure `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | La mantissa (in `[1/2,1)`, senza segno) e l'esponente. CL restituisce tre valori, ma non esistono valori multipli, quindi il segno è lasciato a `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | La stessa scomposizione con una mantissa intera esatta a 53 bit. `mantissa * 2^exponent` è esattamente il valore originale |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **Il razionale più semplice che viene riletto come quel float** (`(rationalize 0.1)` è `1/10`). Per il valore binario esatto, usa `float->ratio` |

**Differenza rispetto a CL: come arrotonda `round`.** `round` (e quindi `fround`/`round-div`) arrotonda
**lontano da zero** (`(round 2.5)` = `3.0`). CL arrotonda **al pari**, dando `2`.

## 5. Razionali `ratio`

Razionali a precisione arbitraria compatibili con CL. Sono sempre mantenuti ridotti ai minimi termini con
denominatore positivo, e sono allocati nello heap. Non c'è conversione implicita con i tipi interi o
`f64` (usa un metodo di conversione esplicito oppure `as`/`try-as`). Per la sintassi dei letterali ratio,
si veda il [Riferimento della sintassi](../syntax.md#1-elementi-lessicali).

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | Le quattro operazioni (risultati sempre ai minimi termini). `/` va in panic per divisione per zero |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Resto della divisione con arrotondamento per difetto (come in CL; il segno segue il divisore) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Resto della divisione con troncamento (come in CL; il segno segue il dividendo) |
| `abs` | `(abs x)` | `ratio→ratio` | Valore assoluto |
| `signum` | `(signum x)` | `ratio→ratio` | Segno (restituisce `1`/`-1`/`0` come `ratio`) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Potenza. L'esponente deve essere un `ratio` a valore intero (altrimenti va in panic). Un esponente negativo dà il reciproco |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | Il maggiore / minore |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` non ha operazioni sui bit (in CL sono solo per gli interi) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Confronto |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Tutti uguali a `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Numeratore ai minimi termini (stesso nome che in CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Denominatore ai minimi termini (sempre positivo) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Parte intera (troncata verso zero) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Converte in `f64` |

Le vie d'ingresso dagli interi a larghezza fissa e da `f64` sono `int->int`/`int->ratio` (capitolo 1) e
`float->int`/`float->ratio` (capitolo 4). `int`/`ratio` sono tipi separati, indipendenti da `i32` e dagli
altri, e l'aritmetica mista richiede conversioni esplicite.

## 6. Numeri complessi `complex`

Una struttura (`defstruct`) della libreria standard.

**Due differenze rispetto a CL** (entrambe derivano dalla tipizzazione statica):

1. **Le componenti sono sempre `f64`.** Un complesso di CL può contenere anche razionali, e
   `(complex 1 2)` e `(complex 1.0 2.0)` sono tipi diversi. Un tipo statico deve sceglierne uno, e le
   funzioni trascendenti restituiscono il tipo in virgola mobile.
2. **`(sqrt -1.0)` è il `sqrt` reale (NaN).** In CL, `sqrt` può restituire un numero complesso a partire
   da uno reale, ma il `sqrt` di `f64` deve restituire un `f64`. Un risultato complesso viene da un
   argomento complesso: `(sqrt (complex -1.0 0.0))` è `i`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Costruzione. Le componenti si possono leggere direttamente come `z::re`/`z::im` |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Parte reale e parte immaginaria. **Funzionano anche sui numeri reali** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), come in CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Coniugato (funziona anche sui numeri reali) |
| `phase` | `(phase z)` | `complex→f64` | Argomento in (-pi,pi] (funziona anche sui numeri reali) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Valore assoluto. **L'unico `abs` che non restituisce il tipo del ricevitore** (come in CL, il valore assoluto di un numero complesso è reale) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Aritmetica complessa |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Uguaglianza componente per componente. Anche `Eq` è implementato (non c'è `Ord`: i numeri complessi non hanno ordine, e anche il `<` di CL li rifiuta) |
| `zerop` | `(zerop z)` | `complex→bool` | Se entrambe le componenti sono 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` danno i valori principali |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | L'angolo del vettore `(x,y)`. **L'`(atan y x)` a due argomenti di CL è zucchero sintattico per questa** (ramifica sul numero di argomenti, come il `log` a due argomenti) |

Implementa `print-object`, quindi `~a`/`~s` lo stampano come `#C(re im)`, come fa CL (il lettore di questo
linguaggio non ha la sintassi `#C` per rileggerlo).

## 7. Booleani

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Negazione |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Tutti confrontano i valori per uguaglianza |

`and`/`or` richiedono la valutazione a corto circuito, quindi sono forme speciali
([Riferimento della sintassi](../syntax.md#4-binding-e-condizionali)).

## 8. Funzioni di supporto numeriche e zucchero di chiamata

`abs`/`signum` (tutti i tipi numerici), `gcd`/`lcm` (solo tipi interi), `rem` (tutti i tipi reali incluso
`f64`) ed `expt` (`int`/`f64`/`ratio`) sono definiti come metodi di ciascun tipo numerico (risolti in
base al tipo del ricevitore: `(abs x)` è il metodo per il tipo di `x`). I dettagli per ciascun tipo si
trovano nei capitoli 1, 3, 4 e 5. Gli interi a larghezza fissa non hanno `expt` (non hanno promozione e
andrebbero in overflow; passa a `int` con `(as int x)` e usa il suo `expt`).

### 8.1 Forme variadiche e con 0/1 argomenti

L'aritmetica e i confronti di CL sono variadici, ma i metodi vengono risolti solo in base al tipo del
ricevitore, non al numero di argomenti. Quindi **il checker espande le forme seguenti in chiamate a due
argomenti**.

| Forma che puoi scrivere | Espansione | Si applica a |
|---|---|---|
| `(op a b c ...)` | Il fold a sinistra `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)` con ogni termine associato a un temporaneo | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Quelle tra le precedenti che hanno un elemento neutro |
| `(op x)` | Per `+ * max min logand logior logxor`, `x` stesso. `(- x)` nega, `(/ x)` dà il reciproco, `(gcd x)`/`(lcm x)` danno `(abs x)` (come in CL) | Come sopra |
| `(cmp x)` | Valuta `x` e dà `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Ogni termine viene valutato esattamente una volta, da sinistra a destra (è per questo che i confronti
variadici passano per temporanei). La forma variadica di `/=` confronta **coppie adiacenti**, a
differenza di CL, che chiede se tutte le coppie differiscono.

### 8.2 `isqrt` ed `expt` intero

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | Il massimo intero che non supera la radice quadrata. Va in panic su un valore negativo |
| `expt` | `(expt n e)` | `(T,T)→T` | Potenza (per quadrati successivi). CL restituisce un razionale per un esponente negativo, ma un tipo intero non può rappresentarlo, quindi va in panic; converti prima in `ratio` |

## 9. Predicati

| Nome | Forma | Tipo | Tipi |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (solo tipi interi, come in CL) |

**Non esistono predicati di tipo** come `numberp`/`integerp`/`floatp` di CL. Con la tipizzazione statica,
il tipo di un valore è già stabilito senza chiederlo a runtime.

## 10. Costanti

| Nome | Tipo | Valore |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Codici di operazione passati a `boole` (al posto delle parole chiave di CL) |

Costanti dei limiti numerici (CLHS 12.1.4.2 / 12.1.3):

| Nome | Tipo | Descrizione |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | Il limite superiore / inferiore di un valore immediato a 63 bit (2^62-1 / -2^62). Un `int` oltre di essi diventa un bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | Il maggiore / minore valore finito |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | La più piccola grandezza non nulla, inclusi i subnormali |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | Lo stesso, limitato ai numeri normalizzati |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | Seguono la definizione di CL (il più piccolo `e` positivo con `(/= (+ 1 e) 1)`), quindi sono **un ULP più grandi di** 2^-53: lo stesso 2^-53 viene arrotondato a `1.0` con l'arrotondamento al pari più vicino |

## 11. Operazioni sui bit

Definite sul complemento a due con infiniti bit (CL 12.10). Sono implementate per i tipi interi a
larghezza fissa e per `int`, non per `ratio` (anche CL ha operazioni sui bit solo per gli interi).

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | And, or, or esclusivo bit a bit (versioni variadiche e senza argomenti in 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Complemento bit a bit |
| `ash` | `(ash x count)` | `(T,int)→T` | Scorrimento aritmetico. A sinistra se `count` è positivo, a destra se negativo |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Se il bit `index` è impostato (**l'ordine degli argomenti è l'opposto di CL**; vedi sotto) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | Il numero di bit impostati (per un numero negativo, il numero di bit a 0) |
| `integer-length` | `(integer-length x)` | `T→T` | Il numero di bit necessari per rappresentarlo, senza contare il segno |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | Le restanti sette, composte dalle precedenti |

**Solo il secondo argomento di `ash` è `int` anziché `T`.** È una **distanza** in bit, non un valore del
tipo del ricevitore, quindi la larghezza e il segno del ricevitore non dicono nulla sulla distanza (per lo
stesso motivo per cui `count` in `(ash integer count)` di CL è un intero qualsiasi). Scorrere a destra un
valore senza segno è uno scorrimento logico (`(ash (the u8 200) -3)` = `25`), e uno con segno è uno
scorrimento aritmetico con arrotondamento verso meno infinito (`(ash (the i32 -100) -4)` = `-7`).
L'`index` di `logbitp` è `int` per lo stesso motivo.

**Specificatori di byte.** Al posto dell'oggetto opaco restituito dal `byte` di CL, si usa una
`cons-cell<int,int>` (`car`=dimensione, `cdr`=posizione). Sia la dimensione sia la posizione sono numeri
di bit, quindi sono `int` qualunque sia la larghezza dell'intero che viene scomposto.

**L'intero è il primo argomento, in un ordine diverso da CL.** CL scrive `(ldb bytespec integer)`, ma
questo linguaggio sceglie un metodo in base al tipo del ricevitore (il primo argomento), e con lo
specificatore per primo non potrebbe scegliere in base al tipo dell'intero. Tutte le altre operazioni sui
bit hanno la forma `(op integer ...)` (`(logand a b)`, `(ash x count)`, `(lognot x)`), e solo la famiglia
`ldb` e `logbitp` erano al contrario, quindi sono state allineate. Gli altri argomenti mantengono l'ordine
relativo di CL, quindi `(dpb newbyte spec n)` diventa `(dpb n newbyte spec)`.

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Crea uno specificatore di byte |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Estrae una componente |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Estrae da `x` il byte specificato, allineato a destra |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Se un bit qualsiasi nel byte specificato è impostato |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Azzera tutto ciò che sta fuori dal byte specificato (mantenendo le posizioni) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Deposita `newbyte` allineato a destra nel byte specificato di `x` |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | La versione di `dpb` che preserva la posizione |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | Una delle 16 operazioni logiche a due operandi, scelta da `op` (una costante `boole-*` del capitolo 10) |

`T` è un tipo che implementa il trait `Bits`, cioè `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Solo `boole`
mantiene `op` per primo, dato che lì non c'è motivo di cambiare l'ordine di CL.

## 12. Numeri casuali

| Nome | Forma | Tipo | Descrizione |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Un numero casuale da `0` fino a `n` escluso. Se lo stato è omesso, attinge da `*random-state*` e lo fa avanzare |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Senza argomento, un nuovo stato; dato uno, una sua copia (la copia riproduce la stessa sequenza) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Sempre `true` (il tipo statico esclude già gli altri tipi; esiste solo per corrispondere a CL) |
| `*random-state*` | — | `random-state` | Lo stato predefinito di `random`. Una globale a cui si può assegnare (sostituiscila con `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | Lo stato che l'intero nomina. Lo stesso seme riproduce sempre la stessa sequenza |

Il generatore è xorshift64 e restituisce la stessa sequenza sia interpretato sia compilato.

Un nuovo stato da `make-random-state` viene inizializzato dall'orologio di sistema, quindi non è
riproducibile tra un'esecuzione e l'altra. Per riprodurre, usa `seed-random-state`:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; prints the same three numbers on every run
```

**CL non ha un modo portabile di fornire un seme** (`make-random-state` accetta solo `nil`/`t`/uno stato),
quindi questo nome segue `sb-ext:seed-random-state` di SBCL anziché CL.

Semi diversi danno sequenze diverse. `(seed-random-state 0)` e `(seed-random-state 1)` danno sequenze
diverse, e così `-7` e `7`.
