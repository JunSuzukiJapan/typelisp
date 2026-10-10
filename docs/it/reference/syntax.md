<!-- translated-from: docs/ja/reference/syntax.md @ 37af68009626057caa98d1dc23e3879b42e983c7 -->
# Riferimento della sintassi di typelisp

typelisp è un Lisp a tipizzazione statica, scritto in S-expression. Per l'elenco delle funzioni e dei
metodi predefiniti, si veda [Funzioni predefinite](functions/README.md); per l'elenco dei tipi,
[types.md](types.md); e per leggere i messaggi di errore, [errors.md](errors.md).

## 1. Elementi lessicali

- **Senza distinzione tra maiuscole e minuscole.** I simboli vengono tutti normalizzati in minuscolo
  durante la lettura.
- **Commenti**: da `;` fino alla fine della riga (commenti di riga). `#| ... |#` (commenti di blocco,
  che possono annidarsi).
- **Valutazione in fase di lettura**: `#.(expr)` **esegue la forma seguente durante la lettura** e tratta
  il suo valore come ciò che è stato letto. È l'unico punto in cui il lettore è qualcosa di più di una
  funzione del testo. Fin dove può arrivare dipende dal percorso di lettura, come in CL:
  - `(load ...)` e il REPL valutano una forma alla volta, quindi può chiamare **funzioni definite prima
    nello stesso testo** (il `load` di CL).
  - Un file di modulo viene controllato come unità ed eseguito da chi lo importa con `use`, quindi `#.`
    può raggiungere solo la libreria standard e ciò che la sessione ha già eseguito. Né le definizioni
    del file stesso né quelle dei moduli che importa con `use` **sono ancora state eseguite** (proprio
    come il `compile-file` di CL ha bisogno di `eval-when`).
  - `read` / `read-from-string` dentro un programma valutano anch'essi `#.` (come in CL).
  - Impostare `*read-eval*` (predefinito `true`) a `false` rende `#.` un errore di lettura ovunque: un
    interruttore per impedire che il testo letto come dati esegua codice (come in CL). Viene consultato
    a ogni `#.`, quindi un `setf` ha effetto dalla forma letta successiva. Dentro
    `with-standard-io-syntax` è `true`.
- **Booleani**: `true` / `false`.
- **Interi**: decimali (`42`, `-7`). Un segno `+`/`-` può venire per primo. Le altre basi si scrivono con
  la sintassi di base di CL `#b`/`#o`/`#x`/`#NNr` (il segno va dopo il marcatore: `#x-ff`). Il prefisso
  `0x` non è in CL e non è adottato: `0xff` viene letto come simbolo.
  Un letterale intero senza annotazione di tipo è `int` per impostazione predefinita (precisione
  arbitraria, [Numeri](functions/numbers.md#3-interi-a-precisione-arbitraria-int)), senza limite
  superiore alla sua dimensione.
  **Se il tipo atteso è un tipo intero a larghezza fissa, il letterale assume quel tipo, e si verifica
  che il tipo possa contenere il valore**: `(the u8 300)` è un errore di tipo (se vuoi che venga
  troncato, scrivi `(as u8 300)`). Grazie a questa regola si possono scrivere `(the u32 4294967295)` e
  `(the u32 #xFFFFFFFF)`.
  Se un valore `int` stia in un immediato a 63 bit o diventi un bignum è deciso dalla sua dimensione,
  senza sintassi speciale (come in CL).
- **Numeri in virgola mobile**: quelli che contengono un punto decimale o un esponente (`e`/`E`) (`1.5`,
  `3.0e10`). `f64` per impostazione predefinita (`f32` se è il tipo atteso).
- **Rapporti**: `numeratore/denominatore` (solo decimale, per esempio `1/3`). Ridotti in lettura, come
  specifica CL (`2/4` è `1/2`). Quelli con valore intero (`4/2` e così via) vengono letti come `int`,
  non `ratio`. Un denominatore zero (`1/0`) è un errore di lettura.
- **Caratteri**: `#\` seguito da un carattere o da un nome di carattere. Per esempio `#\a` `#\Space`
  `#\Newline` `#\Tab` `#\Return` `#\Page` `#\Nul` (anche `#\Null`) `#\Backspace`. I nomi non distinguono
  maiuscole e minuscole.
- **Stringhe**: `"..."`. Gli escape sono `\n` `\t` `\r` `\0` `\\` `\"` (qualsiasi altro `\x` è
  semplicemente `x`).
- **Simboli**: qualsiasi token che contenga lettere, cifre e simboli (`+` `<=` `my-func` e così via).
  `]` e `}` terminano un token, quindi non possono comparire dentro un simbolo, e trovarne uno
  all'inizio di un dato è un errore di lettura. `[` e `{` possono invece comparire in un simbolo:
  come in CL, restano liberi perché il programmatore li usi nelle [macro di
  lettura](#11-reader-macro-readtable).
- **Parole chiave**: simboli che iniziano con i due punti, come `:name` (come in CL). Valutano se stesse:
  non cercano alcun binding e il loro valore è se stesse, con tipo statico `symbol`. Le parole chiave
  con lo stesso nome sono sempre lo stesso oggetto (`(eq :foo :FOO)` è vero; come gli altri simboli
  vengono convertite in minuscolo). I due punti fanno parte del nome, quindi `(symbol->string :foo)` è
  `":foo"` (typelisp non ha un sistema di package, quindi differisce dal `symbol-name` di CL). Un `:`
  isolato o con altri due punti come `:a:b` è un errore di lettura. Si verifica con `keywordp`. Quelli
  che iniziano con `::` non sono parole chiave ma percorsi assoluti (sotto).
  Si noti che `:dyn` è una parola chiave riservata solo per le posizioni di tipo; scriverla altrove è un
  errore (si veda il [capitolo 2](#2-scrittura-dei-tipi)).
- **Liste**: `(a b c)`. Si possono leggere anche le coppie puntate `(a . b)`.
- **Vettori**: `#(1 2 3)` (come in CL). Il contenuto è fatto solo di letterali e non viene valutato:
  la `a` di `#(a b)` è un simbolo, non una variabile. Il tipo degli elementi viene dal contesto
  (`(the Vector<i32> #(1 2))`), oppure dal primo elemento se non c'è contesto (`#(1 2 3)` è un
  `Vector<int>`). Tutti gli elementi devono avere lo stesso tipo: `#(1 "a")` è un errore di tipo, e
  così `#()` senza elementi né contesto. Ogni valutazione crea un nuovo vettore. Dove sono attesi
  dati S-expression (`(the Option<Sexpr> #(1 x))`, `'#(..)`, ciò che restituisce `read`), è un
  `Vector<Option<Sexpr>>` i cui elementi sono tutti dati: la variante `vector` di `Sexpr`.
- **Array**: `#2A((1 2) (3 4))` (come in CL). Il numero tra `#` e `A` è il rango, e altrettanti
  primi livelli di annidamento delle liste nel contenuto sono le dimensioni. `#0A x` è un array a
  zero dimensioni che contiene un elemento. Liste dello stesso livello con lunghezze diverse sono un
  errore di lettura. Il tipo si decide come per i vettori ed è un `Array<T>` (senza elementi deve
  darlo il contesto, come in `(the Array<f64> #2A(()))`). Come dato S-expression è un
  `Array<Option<Sexpr>>`: la variante `array` di `Sexpr`.
- **La lista vuota `()`**: a seconda del contesto, il valore del tipo `Unit` o il `none` di
  `Option<Sexpr>`. **`Sexpr` non ha una variante lista vuota**: `Sexpr` significa "una S-expression non
  vuota", e il tipo dei dati S-expression è `Option<Sexpr>` (si veda "Pattern per `Option<Sexpr>`" in
  [4.3 match](#43-match--pattern-matching)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (significativo solo dentro un quasiquote)
  - `,@x` → `(unquote-splicing x)` (inserito come elementi di lista all'espansione)
- **Percorsi `::`**: `foo::bar` viene letto come un percorso attraverso moduli, tipi e membri (non come un
  singolo nome di simbolo). Uno che inizia con `::`, come `::foo`, è un percorso assoluto dalla radice.
  Un `::` dentro argomenti generici (`Vec<a::b>` e simili) non viene trattato come separatore di
  percorso.

## 2. Scrittura dei tipi

Nel sorgente, i tipi si scrivono come normali simboli o liste.

- **Tipi primitivi**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string`
  `symbol`. `int` è il tipo intero (l'integer di CL, che si muove automaticamente tra immediati a 63 bit
  e bignum; [Numeri](functions/numbers.md#3-interi-a-precisione-arbitraria-int)), e i sei tipi a
  larghezza fissa prendono il nome dalla loro larghezza e dal loro segno (non esiste un tipo intero a
  64 bit; si veda [Numeri](functions/numbers.md#1-interi-a-larghezza-fissa)).
- **Il tipo razionale**: `ratio` (razionali ai minimi termini). Allocato nello heap come in CL, senza
  conversione implicita con `int`/`f64` e simili (converti esplicitamente con `as`/`try-as` o un metodo
  di conversione; si veda [Numeri](functions/numbers.md#5-razionali-ratio)).
- **Parole grezze al confine con C**: `ptr` (un puntatore opaco), `c-long` / `c-ulong`. Solo per la FFI:
  per renderne uno un valore serve `(unsafe ...)`, e i punti in cui possono comparire sono limitati
  ([3.3 defffi](#ptr--c-long--c-ulong--parole-macchina-grezze)). Non usarli dove vuoi un intero a 64 bit:
  non hanno aritmetica.
- **Tipi opachi mutabili**: `random-state` (lo stato di un generatore di numeri casuali). Non può stare
  in `Vector<T>`/`HashTable<K,V>`/`Sexpr` (può stare in `Option<T>`/`Result<T,E>`).
- **Il tipo Unit**: `()`
- **Il tipo Never**: `!` (il tipo delle espressioni divergenti come `panic`/`unreachable`/`todo`/un ciclo
  che non ritorna mai. Si adatta a qualsiasi tipo atteso)
- **Tipi funzione**: `(fn (tipi-degli-argomenti...) tipo-di-ritorno)`. Il tipo di una funzione con
  argomenti variadici è `(fn (tipi-degli-argomenti... &rest tipo-dell-elemento) tipo-di-ritorno)`.
- **Tipi generici**: `Name<T1,T2,...>` (letto come un unico token senza spazi).
  Per esempio `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  Il tipo unit `()` si può scrivere anche come argomento di tipo (`Result<(), FileError>`). `(`/`)` sono
  normalmente delimitatori che terminano un token, ma finché una parentesi angolare è aperta, questa
  coppia di caratteri viene lasciata passare. `()` si può usare anche come tipo di campo o di argomento.
- **La forma applicativa dei tipi generici**: `(Name T1 T2 ...)`, una grafia a lista che nomina lo stesso
  tipo di `Name<T1,T2,...>`. Per esempio `(vector char)` equivale a `Vector<char>`.
  La forma con nome è il modo usuale di scriverlo; questa forma **esiste per quando un argomento di tipo
  non si può scrivere dentro un nome**: un argomento di tipo è esso stesso un'espressione di tipo, ma
  dentro un nome a token singolo si possono scrivere solo nomi, `()` e `:dyn`, non tipi funzione (non
  esiste una grafia come `Vector<(fn (i32) i32)>`). Può anche comparire in questa forma quando
  l'implementazione mostra un tipo, come il risultato della sostituzione di un tipo associato di un
  trait in una firma.
- **Nomi di tipo qualificati**: possono essere qualificati con `::`, come in `module::Type`.
- **Tipi oggetto-trait**: `:dyn Trait` (due parole separate da spazio che formano un unico tipo).
  Rappresenta un valore il cui tipo concreto è deciso a runtime; le chiamate ai metodi del trait passano
  per una vtable (dispatch dinamico). Per un trait con tipi associati, essi vengono fissati per
  posizione nell'ordine di dichiarazione (`:dyn Iter<i32>` fissa `Item` a `i32`). Si può scrivere anche
  dentro argomenti generici: `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`. I valori
  concreti vengono racchiusi automaticamente in box nelle posizioni attese; la forma esplicita è
  `(as :dyn Trait expr)`.
  Un valore di `:dyn Sub` può essere passato così com'è dove è richiesto un `:dyn Super` di uno
  qualsiasi dei suoi supertrait (tutto ciò che eredita, transitivamente) (upcasting). Non può essere
  passato a un trait non correlato.
  Per le condizioni che un trait deve soddisfare per essere usato con `:dyn`, si veda
  [3.9 deftrait / impl](#39-deftrait--impl--trait). Scrivere `:dyn` fuori da una posizione di tipo è un
  errore.
- Tipi generici predefiniti: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>`, e i tipi di concorrenza `Task<T>` / `Thread<T>` / `Chan<T>`
  ([capitolo 12](#12-concorrenza-task)). C'è anche `Sexpr`, il tipo dei dati S-expression. I tipi di
  errore concreti predefiniti sono `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` /
  `FileError` / `NetError`, e la libreria standard ha le strutture `SimpleError` / `WrappedError`
  (`Error` non è un tipo ma un trait: usalo come `:dyn Error`). L'elenco si trova in [types.md](types.md).
- **Tipi e trait condividono un unico namespace** (come in Rust): all'interno di un modulo, un tipo
  (`defstruct`/`defenum`) e un trait (`deftrait`) non possono avere lo stesso nome.

## 3. Definizioni di primo livello

### 3.1 defun — definizioni di funzione

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- I tipi degli argomenti e il tipo di ritorno sono obbligatori.
- Una funzione generica scrive i propri parametri di tipo tra parentesi angolari dopo il nome:
  `(defun name<T1,T2...> (params) Ret body...)` (la stessa sintassi con parentesi angolari di `Vector<T>`
  nelle posizioni di tipo).
- `defun`/`lambda`/`defmethod` accettano argomenti variadici quando `&rest (name Type)` è scritto alla
  fine: `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (nel corpo, `xs` è sempre associato come
  `Option<Sexpr>`, una lista S-expression. Ogni argomento effettivo nella chiamata viene controllato nei
  tipi singolarmente come `Type2` e poi incapsulato in una `Sexpr`).
  Anche `defmacro` ha un proprio `&rest`, ma differisce perché è sempre una `Sexpr` non tipizzata
  (`defun`/`lambda` dichiarano il tipo dell'elemento). Un tipo funzione può descrivere anche una funzione
  variadica, come `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (per `defun` e `defmethod`; non per `lambda`/`labels`, per il motivo
  seguente, e `defmacro` ha un'implementazione separata, anch'essa sotto). L'ordine è quello di CL:
  `required &optional &rest &key`. Ogni parametro si scrive `(name Type)` oppure
  `(name Type default-expr)`:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; no default
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> in the body

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; with a default
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; the caller writes `:name value`, in any order; omitted ones take their defaults
  ```

  - **Un parametro senza espressione predefinita ha tipo `Option<Type>`.** Se omesso, è `none`; se
    passato, il valore semplice scritto dal chiamante viene incapsulato automaticamente in `some`. Ciò
    che CL fa con una variabile supplied-p ("è stato fornito?") qui compare sul lato del tipo statico.
  - Con un'espressione predefinita, il tipo resta `Type` come dichiarato. Quando omesso, quella
    **espressione controllata** viene incorporata nella chiamata così com'è (valutata a ogni chiamata).
  - **`&key` non può essere mescolato con `&optional`/`&rest` in un'unica lista di argomenti.** Questo
    evita un'ambiguità che ha lo stesso CL (se un argomento effettivo finale venga preso da un
    `&optional` posizionale o abbinato per etichetta a un `&key` dipende dai *valori*) vietando la
    combinazione. `&optional` e `&rest` si possono usare insieme.
  - Si possono usare nelle funzioni generiche, ma **un parametro di tipo che compare solo negli
    argomenti omessi non può essere inferito ed è un errore** (non c'è alcun valore con cui
    confrontarlo).
  - **`defmethod` può avere le stesse tre sezioni** (sia per i metodi di istanza sia per le funzioni
    statiche). Elenca `&optional`/`&rest`/`&key` dopo il ricevitore:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; static function
    (point::origin :y 7)
    ```

    Si possono usare anche nei metodi di tipi generici, ma **il tipo di un parametro con espressione
    predefinita non può menzionare i parametri di tipo del proprietario** (la stessa restrizione che ha
    `defun` per i propri parametri di tipo: ciò che viene incorporato quando l'argomento è omesso è
    un'espressione *controllata*, quindi il suo tipo non può essere lasciato come variabile astratta).
  - **Non si possono usare nei metodi dei trait.** `deftrait` non ha sintassi per essi, e se solo il
    lato `impl` potesse dichiarare sezioni, le chiamate con un ricevitore `:dyn` (che riempiono gli
    argomenti dalla dichiarazione del trait) e le chiamate con un ricevitore concreto (che li riempiono
    dalla dichiarazione dell'`impl`) diventerebbero cose diverse. L'arità di uno slot della vtable è
    fissa.
  - **Non si possono usare in `lambda` / `labels`** (`&rest` sì). Per riempire un argomento omesso, il
    chiamante deve leggere **l'espressione predefinita controllata del chiamato**, disponibile solo da
    una firma risolta per nome. Una `lambda` viene passata in giro come valore, e l'unica cosa che
    descrive quel valore è il suo tipo funzione `(fn ...)`: non c'è posto in esso per un'espressione, e
    se ci fosse, "due lambda con la stessa firma ma predefiniti diversi" diventerebbero tipi diversi.
    `&rest` resta nell'ambito dei tipi, quindi si può scrivere in un tipo funzione.
- **I riferimenti in avanti si dichiarano con `defsignature`** (sotto). Un nome che non è stato
  dichiarato non può essere chiamato prima della sua definizione, perché il primo livello viene
  controllato ed eseguito una forma alla volta, in ordine di sorgente.
- Per richiedere vincoli di trait, scrivi una clausola `where` subito prima del corpo:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (fissare un tipo associato con `(AssocName ConcreteType)` è facoltativo).
- **Docstring**: un letterale stringa all'inizio del corpo, subito dopo la clausola `where` (se c'è),
  diventa la docstring (come in CL). Solo quando segue almeno una forma del corpo, però: una stringa da
  sola resta il valore di ritorno e non viene presa come docstring: `(defun f () string "doc" "value")`
  ha una docstring e restituisce `"value"`, mentre `(defun f () string "value")` non ha docstring e
  restituisce `"value"`. Si può recuperare con `(documentation name)`
  ([docstring](functions/system.md#7-docstring--documentation)).

### 3.2 defsignature — dichiarazioni anticipate

```lisp
(defsignature name (argument-types...) return-type)
(pub defsignature name (argument-types...) return-type)
```

Per chiamare una `defun` definita **dopo** di te, dichiarala prima così. La ricorsione mutua si può
scrivere solo in questo modo:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Gli argomenti sono elencati **solo come tipi**; non c'è corpo, quindi non c'è nulla a cui dare nomi.
`&rest` si può scrivere per ultimo, come `&rest tipo-dell-elemento`.

Le dichiarazioni **vengono controllate**:

- La definizione che segue deve corrispondere alla dichiarazione (il numero e i tipi degli argomenti, il
  tipo di ritorno, `&rest`, e se è `pub`). Una discordanza è un errore alla definizione.
- Dichiarare senza definire è un errore (segnalato quando il file / modulo termina il caricamento). Il
  REPL non lo segnala dopo ogni input, perché una dichiarazione e la sua definizione devono poter
  essere digitate su righe separate.
- Una dichiarazione collocata **dopo** la definizione è un errore, dato che una tale dichiarazione non
  potrebbe fare nulla.

Tre cose non si possono dichiarare:

- **Le funzioni generiche.** Creare una copia per ciascun tipo richiede il corpo, e una dichiarazione
  non ne ha. Una chiamata in avanti potrebbe essere risolta ma l'istanziazione fallirebbe, quindi la
  dichiarazione viene rifiutata in anticipo.
- **`&optional`/`&key`.** La loro firma include l'espressione **controllata** di ciascun valore
  predefinito (incorporata nella chiamata quando l'argomento è omesso), e una dichiarazione non ha
  posto per essa.
- **Qualsiasi cosa diversa da `defun`.** Una `defmacro` ha bisogno che il corpo della macro sia **già
  stato eseguito** per espandersi, cosa che la registrazione di una firma non può sostituire. Per i tipi
  (`defstruct`/`defenum`/`deftrait`), registrarli è "ciò di cui ha bisogno il codice che registra il
  tipo stesso", che non è autosufficiente come lo è una firma. Un `defmethod` viene registrato sul tipo
  che lo possiede, quindi segue il tipo.

L'equivalente in CL è `(declaim (ftype (function (i32) bool) even2))`, ma viene con un intero sistema di
dichiarazioni ed è solo **consultivo**. Qui, con la tipizzazione statica, le dichiarazioni vengono
controllate.

### 3.3 defffi — dichiarare funzioni C (FFI)

```lisp
(defffi (name "c_symbol") (argument-types...) return-type)
(defffi (name "c_symbol") (argument-types...) return-type :library "name")
(defffi name (argument-types...) return-type)              ; name = the C symbol name
(pub defffi ...)
```

Dichiara una funzione C in modo che possa essere chiamata. La forma è la stessa di `defsignature` (un
nome, tipi degli argomenti, un tipo di ritorno e nessun corpo), ma l'assenza di corpo significa una cosa
diversa. `defsignature` è una promessa che "la definirò più tardi", mentre `defffi` dichiara che
"qualcun altro ha già scritto e compilato il corpo".

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

Il nome typelisp e il nome del simbolo C si possono scrivere separatamente perché gli identificatori
typelisp di solito contengono `-` e quelli C no. Se il nome C è omesso, il nome viene usato così com'è
come nome del simbolo C.

**Le chiamate richiedono `(unsafe ...)`** (anche per funzioni solo su scalari). Il compilatore non ha
modo di confermare che la firma C dichiarata corrisponda a quella reale e può solo fidarsi della
dichiarazione; `unsafe` è il segno che ti assumi quella responsabilità. Il modo previsto è racchiuderla
una volta e creare un wrapper sicuro:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; no unsafe needed from here on
```

I tipi che si possono scrivere sono `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void)
`string` `ptr` `c-long` `c-ulong`, e i puntatori tipizzati `(ptr T)`
([sotto](#def-c-struct-e-puntatori-tipizzati--allocare-struct-c)).

`string` è `const char *`. Le stringhe typelisp non sono terminate da NUL e possono esse stesse contenere
NUL, quindi **vengono copiate in una stringa C quando passate**, e liberate dopo la chiamata. Un NUL nella
stringa è un errore: C guarderebbe solo fino ad esso, quindi verrebbe passata in silenzio una stringa
diversa.

**Anche le stringhe restituite vengono copiate**, e non liberate: ciò che C restituisce appartiene a C, e
può puntare dentro una tabella statica, come con `getenv`. Le funzioni che restituiscono memoria che il
chiamante deve liberare (`strdup` e così via) vanno prese come `ptr` e liberate da te.

Funzionano correttamente anche le funzioni il cui risultato punta dentro un argomento (`strchr`,
`strstr`): il risultato viene copiato prima che l'argomento sia liberato.

Se una funzione dichiarata per restituire `string` restituisce NULL, è un errore, perché `string` non ha
un valore che significhi "non c'era". Se NULL è possibile, prendi il risultato come `ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Con `:library`, quella libreria condivisa viene aperta e il simbolo viene cercato in essa. Senza, il
simbolo viene cercato **nel processo stesso** (tutto ciò che è già collegato, compresa libc). Un nome
breve come `sqlite3` viene cercato come `libsqlite3.dylib` / `libsqlite3.so` in quest'ordine, e un nome
che contiene `/` viene trattato come un percorso. Le librerie aperte non vengono mai chiuse: il codice
che punta alle loro funzioni continua a funzionare, quindi l'unica durata corretta è quella del
processo.

#### ptr / c-long / c-ulong — parole macchina grezze

`ptr` è un puntatore opaco (`void *`, `FILE *`, qualunque cosa intendesse la dichiarazione).
`c-long` / `c-ulong` sono i `long` / `unsigned long` di C (anche `size_t`, `int64_t` e `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Non chiamarli `i64` / `u64` è una scelta deliberata.** Questo linguaggio non ha un tipo intero a 64 bit,
perché un immediato con tag ha solo 63 bit ([capitolo 2](#2-scrittura-dei-tipi)). Il nome `c-long` dice
"questa è una parola che attraversa il confine con C, non un intero di questo linguaggio".

**Non hanno aritmetica.** `(+ x 1)` non si può scrivere. Si potrebbe fornire ma non lo si fa, in modo che
nessun calcolo venga eseguito su un valore che non può essere memorizzato da nessuna parte e che ha una
larghezza diversa da ogni altro numero, per lo stesso motivo per cui il tipo intero a 64 bit è stato
escluso. Ci sono **solo conversioni**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; read what came back
(as int (unsafe (c-strlen s)))               ; this one to read it exactly (int does not lose 64 bits)
(try-as i32 (unsafe (c-strlen s)))           ; ask whether it fits
(as c-ulong n)                               ; make one from another integer
```

I **letterali** interi assumono il tipo atteso, quindi non serve `as` solo per passarne uno:

```lisp
(unsafe (c-malloc 16))                       ; 16 is read as a c-ulong
```

I letterali fuori intervallo vengono rifiutati come con le altre larghezze (`(c-malloc -1)` non sta in un
`c-ulong`).

**I punti in cui possono comparire sono limitati**: solo tipi degli argomenti, tipi di ritorno e variabili
locali. Ciascuno dei seguenti è un errore:

```lisp
(defstruct handle (p ptr))          ; a struct field
(defenum maybe (none) (some ptr))   ; an enum field
(defvar (block ptr) ...)            ; a global
(defffi f ((vector ptr)) i32)       ; inside a type argument
```

C'è un solo motivo per tutti: **lo slot etichetta ciò che contiene**. L'etichettatura scarterebbe i bit
alti del puntatore, lo stesso motivo per cui il tipo intero a 64 bit è stato escluso, quindi non è
consentita nemmeno in `unsafe`. Non è una questione di permesso: quella rappresentazione non esiste.

Per lo stesso motivo, non possono essere variabili locali **catturate** da funzioni annidate (un binding
catturato va in una cella, e una cella etichetta ciò che contiene). Questo è noto in fase di
compilazione e viene segnalato da `(compile f)`.

Il GC non traccia `ptr`. Punta fuori dallo heap, quindi è corretto.

Quattro cose non si possono dichiarare:

- **Argomenti variadici** (`printf`). La parte variadica viene passata con regole diverse da quelle degli
  argomenti fissi (sullo stack su AArch64 Darwin), quindi non può essere chiamata correttamente da una
  firma fissa. `&rest` viene rifiutato.
- **Passare o restituire strutture per valore.** Per lo stesso motivo (dipende dalla convenzione di
  chiamata di ciascuna piattaforma). I tipi scrivibili sono limitati all'elenco sopra, quindi non si può
  scrivere.
- **Generici.** C non ha un equivalente.
- **Lo stesso nome di una funzione predefinita.** Una chiamata compilata risolverebbe quel nome nella
  funzione predefinita, quindi viene rifiutato anziché andare male in silenzio.

#### Callback — far richiamare C

Scrivere un tipo funzione `(fn (types...) return-type)` come tipo di un argomento rende quell'argomento
una funzione che C richiama (un callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; a top-level function
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; a lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; a local function
```

Un puntatore a funzione C non è altro che un indirizzo di codice, e C lo chiama passando solo gli
argomenti dichiarati. Non c'è posto dove passare le variabili catturate, quindi **si possono passare solo
funzioni senza variabili libere**, e questo viene controllato in fase di controllo dei tipi.

- Scrivi un nome di funzione o un'espressione `lambda` **direttamente** come argomento effettivo. Una
  variabile che contiene una funzione non si può passare: quale funzione contenga, e quindi se abbia
  variabili libere, non è noto fino al runtime.
- Una `lambda` è un errore se fa riferimento a variabili locali esterne. Le variabili globali e le
  funzioni di primo livello possono essere referenziate.
- Una funzione locale (`labels`) non deve avere variabili libere, comprese quelle delle funzioni sorelle
  che chiama. Le funzioni sorelle condividono il posto in cui sono conservate le variabili catturate,
  quindi ciò che una sorella chiamata cattura viene catturato anche da questa funzione.
- Una funzione generica ottiene i propri tipi dal tipo funzione dichiarato.
- I tipi che si possono scrivere nel tipo funzione sono gli stessi dell'elenco sopra. Tuttavia, `string`
  non può essere il tipo di ritorno di un callback (consegnerebbe a C memoria che nessuno libera).
  Un argomento `string` copia in una stringa typelisp la stringa passata da C.

Le chiamate a funzioni C si possono scrivere solo dentro `unsafe`, quindi i callback si possono passare
solo dentro `unsafe`.

**Il callback può essere chiamato solo mentre la funzione C chiamata da typelisp è in esecuzione.** Se
viene chiamato da qualsiasi altro posto (un thread che non esegue typelisp, un handler di segnale, una
funzione registrata con `atexit`), stampa il motivo e ferma il processo.

**I fallimenti non si propagano attraverso C.** Un `panic` o un `throw` dentro il callback non può
svolgere lo stack attraverso i frame C (sarebbe comportamento indefinito), quindi a C viene restituito 0,
e il fallimento viene rilanciato al chiamante quando la funzione C ritorna. Se il callback viene chiamato
di nuovo tra il fallimento e il ritorno della funzione C, non viene eseguito e viene restituito 0.

Un'operazione che dovrebbe attendere dentro un callback (un `recv` su un canale vuoto e così via) è un
errore ([12.6](#126-codice-compilato-e-task)).

Quando una funzione viene ridefinita, la nuova definizione viene chiamata dalla volta successiva in cui
viene passata a C.

Funziona allo stesso modo con AOT (`compile-file`). I punti di ingresso che C chiama sono incorporati
nell'eseguibile.

**Non si possono passare come valori.** Una dichiarazione FFI non si può scrivere così com'è per la `f` di
`(map f xs)`: un valore funzione è una chiusura che avvolge il corpo di una definizione, e questa
dichiarazione non ha un corpo da avvolgere. Avvolgila in una `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

Anche `(disassemble c-abs)` viene rifiutato: ciò che si potrebbe mostrare è il codice macchina di C, che
questo compilatore non ha prodotto. `(compile c-abs)` riesce (e non fa nulla, dato che è già compilato).

**Funziona anche con AOT (`compile-file`).** Il linker risolve le funzioni C stesse. Se una dichiarazione
ha `:library`, quella libreria viene aggiunta alla riga di collegamento come `-l` (i duplicati vengono
fusi in uno), quindi `compile-file` non richiede argomenti aggiuntivi. `compile-file` stesso legge il
sorgente, quindi può raccoglierle dalle dichiarazioni.

I simboli vengono cercati anche in fase di compilazione. Se una funzione dichiarata non esiste, l'errore
la nomina prima di qualsiasi errore del linker.

La libreria standard (il prelude) non usa `defffi`. La libreria standard entra per intero in ogni
eseguibile, quindi una dichiarazione con `:library` al suo interno collegherebbe quella libreria anche ai
programmi che non usano la FFI.

#### def-c-struct e puntatori tipizzati — allocare struct C

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Dichiara una struttura con lo stesso layout di quella in C. Si può scrivere solo dentro un `unsafe` di
primo livello (che non può contenere altro che `def-c-struct`). Una docstring si può mettere subito dopo
il nome.

I tipi che si possono scrivere per i campi sono `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32`
`f64` `bool` `ptr`, i puntatori tipizzati `(ptr T)` e altre `def-c-struct` (incorporate per valore). Il
layout (l'offset di ciascun campo, e la dimensione e l'allineamento della struttura) viene calcolato con
le regole di C (assumendo LP64). Si può scrivere un campo che punta alla struttura stessa, ma la
struttura non può incorporare se stessa.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x at 0, y at 8, size 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

Il nome di una `def-c-struct` entra nel namespace dei tipi (nessuna `defstruct` o simile con lo stesso
nome può stare nello stesso modulo), ma **non è il tipo di un valore**. Non si può scrivere
`(defun f ((p point)) ...)`; compare solo come ciò a cui punta un puntatore tipizzato.

**Un puntatore tipizzato `(ptr T)`** è un indirizzo che punta a un `T`. `T` è uno dei tipi che si possono
scrivere per i campi sopra. È una parola macchina grezza come `ptr`, con le stesse regole su dove può
comparire (solo argomenti, tipi di ritorno e variabili locali; può essere un valore solo dentro
`unsafe`).

L'allocazione, la lettura e la scrittura si scrivono nelle forme seguenti. Tutte si possono usare solo
dentro `unsafe`.

| Forma | Significato |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Alloca `n` valori di `T` (1 se omesso). Il contenuto è riempito con 0. Restituisce un `(ptr T)` |
| `(c-ref p i)` | Un puntatore all'elemento `i` a partire da `p`. Un errore se fuori dall'intervallo allocato |
| `(c-deref p)` / `(setf (c-deref p) v)` | Legge / scrive lo scalare a cui punta `p` |
| `p::field` / `(setf p::field v)` | Legge / scrive un campo di una struttura. Leggere un campo che è una struttura incorporata dà il suo indirizzo (`(ptr inner-type)`) |
| `(as ptr p)` | Dimentica il tipo, ottenendo un `ptr` (da passare a qualcosa come il `void *` di `qsort`). Non c'è conversione inversa |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**La memoria allocata viene liberata quando il controllo esce dall'`unsafe` che l'ha allocata.** Il
proprietario è l'`unsafe` lessicalmente più esterno all'interno della stessa funzione. Viene liberata sia
che il codice termini normalmente sia che esca con `panic`, `throw` o `return-from`. Le funzioni `lambda`
e `labels` sono funzioni separate, quindi un `c-alloc` al loro interno richiede un `unsafe` proprio al
loro interno.

Per questo motivo, un puntatore tipizzato non può uscire dall'`unsafe` che l'ha allocato. Ciascuno dei
seguenti è un errore di tipo:

- Farne il valore dell'espressione `unsafe` (quindi non si può nemmeno restituire da una funzione)
- Catturarlo in una chiusura (`lambda`, `labels`)
- Passarlo a `task` / `thread`
- Lanciarlo con `throw`

Per usare i valori fuori dall'`unsafe`, copiali in una `defstruct` o in numeri dentro l'`unsafe` e
restituisci quelli.

**La memoria allocata sul lato C non viene gestita.** I valori che arrivano da C come puntatori tipizzati
(valori di ritorno di `defffi`, argomenti di callback, valori letti da campi di tipo puntatore) vengono
controllati a runtime per vedere se puntano a un valore di quel tipo all'interno di un'allocazione
`c-alloc` viva, e sono un errore in caso contrario. Anche NULL è un errore. Per ricevere memoria che C ha
allocato, o NULL, usa il `ptr` non tipizzato (il cui contenuto non può essere letto).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Quando l'argomento di un callback viene rifiutato dal controllo, viene segnalato al chiamante quando la
funzione C ritorna, proprio come un fallimento dentro un callback.

### 3.4 defvar / defparameter / defconstant — variabili globali

```lisp
(defvar (name Type) init-expr)        ; initializes only if not yet bound
(defparameter (name Type) init-expr)  ; assigns every time
(defconstant (name Type) init-expr)

; with a docstring (in the same order as CL's defvar/defparameter/defconstant: after the value)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**La differenza tra `defvar` e `defparameter` si vede alla ricarica** (come in CL). Se la globale è **già
associata, `defvar` non valuta nemmeno l'inizializzatore**, quindi quando modifichi un file di
impostazioni e lo rileggi, i valori che la sessione ha cambiato restano così come sono. `defparameter`
assegna ogni volta, quindi rileggerlo riporta i valori a quelli scritti.

L'annotazione di tipo è obbligatoria (non viene inferita dall'inizializzatore). `defvar` può essere
modificata; `defconstant` no (`setf` è un errore).

### 3.5 defmethod — definizioni di metodo

```lisp
; instance method: can be called as (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / associated function: can be called as (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

Il chiamante risolve il metodo a partire dal tipo statico di `obj` (dispatch singolo, statico). Una
docstring si può collocare nella stessa posizione e con le stesse regole di `defun` (subito dopo la
clausola `where`, all'inizio del corpo, solo quando seguono forme del corpo). Lo stesso vale per i metodi
dentro `impl`; si recuperano con `(documentation Type::method)`.

I parametri di tipo propri di un metodo si scrivono nel suo nome con `<...>`, come per `defun`. I
parametri di tipo del tipo del ricevitore (`T` sotto) sono fissati dal ricevitore; quelli propri del
metodo (`U`) sono dedotti dagli argomenti di ogni chiamata.

```lisp
(defstruct Box<T> (v T))

(defmethod fmap<U> ((self Box<T>) (f (fn (T) U))) Box<U>
  (Box::new (f self::v)))

(fmap (Box::new 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- I parametri di tipo propri del metodo devono avere nomi diversi sia dai parametri di tipo
  dichiarati dal tipo del ricevitore (la `T` di `(defstruct Box<T> ...)`) sia dai nomi scritti nel
  ricevitore.
- Se il tipo del ricevitore è generico, nel ricevitore si scrivono tutti i suoi parametri di tipo
  come variabili (`Box<T>`) oppure tutti come tipi concreti (`Box<int>`).
- Un metodo dentro `impl` non può aggiungere parametri di tipo: la sua firma segue quella dichiarata
  dal trait.

### 3.6 defstruct — strutture (tipi definiti dall'utente)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; generic (type parameters in angle brackets)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Ogni campo è `(name type)` oppure `(pub name type)` (visibilità per campo, indipendente dal `pub` della
  struttura stessa). Un'espressione in più alla fine diventa il **valore predefinito** dello slot
  (`(x i32 0)`); si veda l'elenco delle opzioni sotto.
- Vengono generati automaticamente:
  - Il costruttore `Name::new` (argomenti in ordine dei campi)
  - I getter `(field-name instance)`, con lo zucchero `instance::field-name`
  - I setter `(set-field-name instance value)`, con lo zucchero `(setf instance::field-name value)`
- Per rendere `pub` la struttura stessa, metti `pub` davanti, come in `(pub defstruct ...)`.
- **Definisci un tipo prima di nominarlo.** Il tipo di un campo può essere la struttura stessa
  (`(next Option<node>)`), ma non un tipo definito dopo: i tipi non hanno una dichiarazione anticipata
  corrispondente a `defsignature`. Un nome non ancora definito dà lo stesso errore `unknown type` nel
  tipo di un argomento di `defun` o in `the`. Quindi due tipi che si riferiscono l'uno all'altro non si
  possono scrivere.
- **Le variabili di tipo sono solo quelle scritte in posizioni di dichiarazione.** Per
  `defun`/`defstruct`/`defenum`/`deftype`, il `<T>` del nome; per `defmethod`, il tipo del
  ricevitore (`(self box<T>)`, oppure `box<T>` per un metodo statico) e il `<U>` del nome del
  metodo; per `impl`, il tipo bersaglio e `impl<T>`; per `deftrait`, `Self` e i tipi associati di
  `(type Item)`. Un nome che compare per la prima volta in qualsiasi altro punto (argomenti, valore
  di ritorno, `the`/`lambda` nel corpo) non diventa una variabile di tipo; è `unknown type`.
- **Docstring**: un letterale stringa subito dopo il nome, prima dei campi, diventa la docstring
  (`(defstruct Name "doc" (field Type)...)`, la stessa posizione del `defstruct` di CL). Un campo ha
  sempre la forma `(name Type ...)` e non può mai essere una stringa semplice, quindi non c'è ambiguità.
  Si recupera con `(documentation Name)`.

#### Elenco delle opzioni

Scrivere una lista `(Name option...)` in posizione di nome specifica delle opzioni (la stessa posizione di
CL).

```lisp
(defstruct (point (:constructor make-point)          ; keyword constructor
                  (:constructor at (x &optional y))  ; BOA constructor
                  (:copier copy-point))
  (x i32 0)          ; a third element is that slot's default value
  (y i32 0))

(point::make-point :y 7)   ; x is 0
(point::at 1)              ; y is 0
(point::at 1 2)
(copy-point p)             ; a shallow copy (the same as CL's copier)
```

- **`:constructor`**: ciò che viene generato è una **funzione statica** del tipo (`point::make-point`), il
  cui corpo è sempre `(point::new ...)`. `new` resta l'unico costruttore strutturale; ciò che si crea qui
  è un *modo di chiamarlo*. Se ne possono dichiarare più di uno.
  - `(:constructor name)` prende ogni slot come `&key`. **Ogni slot ha bisogno di un valore predefinito**
    (questo linguaggio non ha nulla di corrispondente allo "slot non associato" di CL).
  - `(:constructor name (slot...))` prende gli slot indicati come argomenti posizionali (in qualsiasi
    ordine). Gli slot non indicati vengono riempiti con i loro valori predefiniti, quindi **hanno bisogno
    di valori predefiniti**. Dopo `&optional`, il resto può essere omesso (e ha del pari bisogno di valori
    predefiniti).
- **`:copier`**: genera un **metodo di istanza** che restituisce un nuovo valore con gli stessi valori di
  slot. Superficiale, come il copier di CL.
- **`:include Parent`**: antepone gli slot del genitore (anche i valori predefiniti vengono ereditati; il
  genitore può trovarsi in un altro file). **Non crea alcuna relazione di tipo**: il figlio non è un
  sottotipo del genitore, i metodi del genitore non si applicano al figlio, e non c'è alcun test a
  runtime che colleghi i due. Questo linguaggio non ha sottotipi; le interfacce comuni sono compito di
  `deftrait`. Viene unita solo la *lista* degli slot.
- **I valori predefiniti degli slot sono letti solo dai costruttori generati.** Scrivere un valore
  predefinito senza dichiarare alcun `:constructor` è un errore, dato che non potrebbe mai essere usato.
- Opzioni omesse, e perché:
  - **`:conc-name`**: in CL antepone un prefisso agli accessori per evitare conflitti in un unico
    namespace piatto di funzioni. Qui gli accessori sono metodi con dispatch sul tipo del ricevitore,
    quindi i conflitti non avvengono, e un prefisso romperebbe `instance::field` (che conosce solo il
    nome dello slot).
  - **`:predicate`**: risponde a runtime a "questo valore è un `point`?". Qui i tipi sono una
    classificazione a tempo di compilazione senza testimone a runtime, e non esiste una posizione in cui
    esista "un valore di tipo sconosciuto che potrebbe essere un point" (`match` su `Sexpr` è sigillato, e
    `:dyn` non si può sottoporre a downcast), quindi un predicato generato potrebbe solo restituire
    `true`.
  - **`:type` / `:initial-offset` / `:named`**: sostituiscono la rappresentazione del valore con una
    lista o un vettore. La rappresentazione appartiene al compilatore e non può essere osservata dal
    linguaggio.

### 3.7 defenum — enumerazioni (tipi somma)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; a variant with a payload (positional fields)
  (Variant2)                  ; a variant without a payload
  ...)

; generic
(defenum Option<T>
  (Some T)
  (None))
```

- Ogni variante ha la forma `(VariantName FieldType...)`. I campi sono solo posizionali (non hanno
  nomi). Serve almeno una variante, e i nomi non possono ripetersi.
- I valori si costruiscono, come con i `Option`/`Result` predefiniti, qualificati o tramite `use`:
  `(Name::Variant1 a b)`, oppure `(Variant1 a b)` dopo `(use Name)`.
- Possono essere scomposti con `match` / `if-let`. `match` controlla l'esaustività (deve coprire ogni
  variante o avere un `_`):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Metodi e funzioni associate si aggiungono in seguito con `defmethod`/`impl`, come con `defstruct`.
- Per rendere `pub` l'enumerazione stessa, scrivi `(pub defenum ...)`.
- **Docstring**: la stessa posizione e le stesse regole di `defstruct`, subito dopo il nome, prima delle
  varianti (`(defenum Name "doc" (Variant ...)...)`). Si recupera con `(documentation Name)`.

### 3.8 deftype — alias di tipo

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

Il `deftype` di CL, ristretto a ciò che ha senso in un linguaggio a tipizzazione statica: **una grafia di
un tipo, non un tipo**.

- La posizione del nome è la stessa di `defun`, e gli argomenti generici si scrivono `Name<T,U>`. Nel
  punto d'uso serve esattamente il numero dichiarato di argomenti di tipo (troppi o troppo pochi è un
  errore immediato).
- L'espansione avviene **dentro il parser dei tipi**. Quindi nulla a valle sa che l'alias esiste: le
  chiavi di monomorfizzazione, i dump, il percorso di compilazione e i **messaggi di errore** mostrano
  tutti la forma espansa. Se `(f "x")` fallisce contro una funzione che richiede `meters`, il messaggio
  dice `i32`.
- **Non è un nuovo tipo.** `(deftype meters i32)` rende `meters` e `i32` lo stesso tipo, quindi
  confonderli non viene rilevato. Se vuoi tenerli distinti, usa `defstruct`.
- **Non è un predicato.** Il `(deftype small () '(integer 0 9))` di CL descrive un *insieme di valori*
  che `typep` verifica a runtime, ma qui i tipi sono una classificazione a tempo di compilazione senza
  testimone a runtime, quindi un alias che restringe i valori non avrebbe nulla da restringere.
- **Non può contenere se stesso.** Un alias viene espanso dove è scritto, quindi non c'è alcun posto in
  cui possa ricorrere. I tipi di dati ricorsivi si scrivono con `defstruct`/`defenum`.
- Condivide il namespace con tipi e trait (all'interno di un modulo non può avere lo stesso nome di una
  `defstruct`/`defenum`/`deftrait`). Rendilo pubblico con `(pub deftype ...)` e importalo con
  `(use m::meters)`.
- **Docstring**: subito dopo il nome, prima del tipo (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — trait

```lisp
(deftrait TraitName (SuperTrait...)      ; the supertrait list is required; () if none
  (type AssocName)                       ; associated types (any number, optional)
  (method-name ((self Self) params...) RetType)          ; no body = must be implemented
  (method-name ((self Self) params...) RetType body...)) ; with a body = default implementation

(impl TraitName TargetType
  (where (Trait A)...)                   ; bounds applying to the whole impl (optional)
  (type AssocName ConcreteType)          ; makes an associated type concrete
  (method-name (recv params...) RetType body...))
```

Attraverso `impl`, ogni metodo viene registrato come un normale `defmethod` di `TargetType`. I trait
vengono richiamati come vincoli di trait nelle clausole `where` delle funzioni generiche (si veda
[3.1 defun](#31-defun--definizioni-di-funzione)). Un nome di trait può anche essere un percorso `::` come
`m::Trait`.

**L'elenco dei supertrait (obbligatorio)**: scritto sempre subito dopo il nome del trait. Ogni elemento è
un semplice nome di trait, oppure, se quel trait ha tipi associati, `(Trait (Assoc Type))` con **tutti i
suoi tipi associati fissati**.

```lisp
(deftrait Eq () ...)                       ; no supertraits
(deftrait Ord (Eq) ...)                    ; Rust's trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; pinning an associated type
  (rewind ((self Self)) ()))
```

L'ereditarietà ha tre effetti. (1) `impl Ord X` richiede che `impl Eq X` sia scritto **per primo** (una
regola sull'ordine di scrittura: l'unica forma che si può decidere in modo deterministico nel REPL e con
`load` passo per passo, e più severa di Rust). (2) `(where (Ord T))` da solo permette di chiamare anche i
metodi di `Eq`. (3) I metodi di `Eq` si possono chiamare attraverso un `:dyn Ord`, e un valore `:dyn Ord`
può essere passato così com'è dove è richiesto un `:dyn Eq` (upcasting). Un sottotrait che ridichiara un
metodo con lo stesso nome del genitore, e l'ereditare metodi con lo stesso nome da due genitori, sono
entrambi errori (una vtable ha uno slot per nome). L'ereditarietà a diamante si fonde in un unico slot.

**Implementazioni predefinite**: un corpo dopo la firma viene usato quando un `impl` omette il metodo. Il
corpo viene risolto nel **namespace del modulo** in cui il trait è scritto, quindi può chiamare funzioni
non pubbliche di quel modulo. Anche i metodi con corpo possono avere clausole `where` e docstring. Il
corpo viene controllato nei tipi **una volta, nel punto di dichiarazione**, con `Self` lasciato come
variabile di tipo (vincolata da `Self: il trait stesso`), come in Rust: gli errori che fallirebbero per
ogni `impl` e per ogni tipo che implementa, anche nei predefiniti che nessun `impl` omette mai, vengono
trovati lì. Le chiamate su `self` a metodi del trait stesso o dei suoi supertrait passano attraverso
questo vincolo, e i tipi associati sono fissati a se stessi, quindi una firma che restituisce `Item` viene
confrontata con il corpo senza conoscere il tipo concreto.

**Implementazioni blanket**: fare del bersaglio una variabile di tipo implementa il trait in un colpo solo
per ogni tipo che soddisfa i vincoli.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; no body at all; everything is the default
```

**Non viene generato alcun codice finché un tipo concreto non lo usa davvero** (una volta per tipo, con lo
stesso meccanismo della normale monomorfizzazione). Un trait può avere al massimo un'implementazione
blanket. Se un tipo ha un `impl` esplicito, quello ha la priorità. Il controllo dei tipi del corpo è
separato dalla generazione: viene fatto una volta nel punto di dichiarazione, **con il bersaglio lasciato
come variabile di tipo** (come in Rust), quindi anche un'implementazione mai usata ha i propri errori
trovati lì se fallirebbero per ogni bersaglio sotto i vincoli dichiarati. Le chiamate giustificate dai
vincoli (`(less self other)` sotto `(where (Ord T))` e così via) passano, come nel corpo di una `defun`
generica.

**Docstring**: un `deftrait` può avere una docstring per l'intero trait, come letterale stringa subito
dopo l'elenco dei supertrait, prima delle voci (`(deftrait Name () "doc" (type ...) (method ...)...)`).
Una firma senza corpo non può avere una docstring: una stringa finale sarebbe essa stessa il valore di
ritorno di un'implementazione predefinita, quindi i due casi non si potrebbero distinguere.

I trait forniti dalla libreria standard: **`Iter`** (`next` / tipo associato `Item`; la base di `doiter` e
delle funzioni sulle sequenze), **`Eq`** (`equals`; `not-equals` è un'implementazione predefinita),
**`Ord`** (eredita `Eq`; deve essere implementato solo `less`, e `less-equal` / `greater` /
`greater-equal` sono implementazioni predefinite), **`Error`** (`message` / `source`; `:dyn Error` per
trattare uniformemente i tipi di errore), **`print-object`** (una rappresentazione stampata per tipo),
**`Pathish`** (designatori di pathname: una stringa o un `pathname`), e la gerarchia degli stream
**`Stream`** → **`InputStream`** / **`OutputStream`** → **`CharInput`** / **`CharOutput`** →
**`PeekInput`**. Quali tipi implementano quali trait si trova in [types.md](types.md); i metodi di ogni
trait si trovano in [Trait standard](functions/traits.md),
[Tipi di errore](functions/option-result.md#3-tipi-di-errore-e-il-trait-error),
[print-object](functions/printing.md#5-print-object-rappresentazione-stampata-per-tipo) e
[Stream](functions/streams-files.md). Se fai `impl` di `Iter` per il tuo tipo di collezione, `doiter`
(capitolo 5) e `map` / `filter` / `sort` e simili funzionano su di esso così come sono.

Le chiamate ai trait sono **statiche** per impostazione predefinita (risolte dal tipo statico del
ricevitore). Per trattare valori il cui tipo concreto è deciso a runtime, il tipo oggetto-trait
`:dyn Trait` (capitolo 2) dà il dispatch dinamico attraverso una vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; one call site, an answer per implementation
```

Solo i trait in cui "ogni metodo ha un ricevitore `self`, non usa `Self` in nessun punto diverso dal
ricevitore, e non è esso stesso né generico né variadico" possono diventare `:dyn` (i metodi ereditati
devono soddisfare le stesse condizioni).

Solo i tipi i cui valori hanno una rappresentazione sullo heap possono entrare in un box `:dyn`:

| Può entrare | Non può entrare |
|---|---|
| I tipi `defstruct` / `defenum` (compresi `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` e le strutture della libreria standard), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Interi a larghezza fissa (da `i8` a `u32`), `f32`, `bool`, `char`, `symbol`, `()`, tipi funzione e `Option<T>` senza box ([la rappresentazione a runtime di Option](functions/option-result.md#2-la-rappresentazione-a-runtime-di-optiont)) |

Mettere un valore di un tipo che non può entrare dove è atteso un `:dyn` è un errore di tipo. Per trattare
tali valori attraverso `:dyn`, avvolgili in una struttura, come in `(defstruct flag (v bool))`.

### 3.10 module / use — namespace

```lisp
(module path body...)      ; path is a sequence of segments such as foo or foo::bar
(in-module path)           ; from here to the end of this unit, inside path (the flat form of module)
(use path...)              ; alias functions, types and modules into the current namespace
(import path...)           ; the same as use (a CL-compatible spelling)
(shadowing-import path...) ; a use that knowingly takes a bare name already in use
```

- `module` crea un namespace. **I tipi non sono namespace** (come in Rust, un tipo ha solo funzioni
  associate e metodi).
- Fare `use` di un tipo rende disponibili per nome semplice anche i suoi costruttori e i metodi statici
  pubblici (per esempio, dopo `(use option)`, `some`/`none` si possono chiamare senza
  `option::some`/`option::none`).
- L'ordine di risoluzione dei nomi semplici (identificatori non qualificati): forme speciali →
  costruttori → funzioni libere (namespace corrente → radice) → metodi di istanza (risolti dal tipo
  statico del primo argomento). Non risale attraverso i moduli genitori intermedi.
- Un percorso qualificato `a::b` risolve `a` nell'ordine sopra; se è un modulo, entra al suo interno, e se
  è un tipo, l'ultimo segmento viene risolto come elemento associato.
- **`use` influenza le forme che lo seguono.** Un file viene letto una forma alla volta, e le dipendenze
  vengono risolte subito prima che la forma venga controllata, quindi scrivere `m::f` **sopra**
  `(use m)` dà `unresolved path`. Metti `use` all'inizio del file.
- **`use` può prendere più percorsi** (`(use a::f b::g)`). `import` è una grafia compatibile con CL con
  lo stesso comportamento.
- **Un `use` il cui nome semplice è già occupato viene segnalato.** La risoluzione di un nome semplice
  guarda le definizioni del modulo stesso prima degli alias, quindi `(use m::twice)` dopo
  `(defun twice ...)` **non fa nulla**. Se lo intendi davvero, scrivi `shadowing-import` (non può
  comunque battere una definizione, dato che non c'è modo di rimuoverne una; batte solo gli alias
  precedenti).
- **`in-module` è la forma piatta di `(module path body...)`.** Scrivere `(in-module geometry)` mette
  tutto da lì fino alla fine dell'unità (il file, o il corpo del `module` che lo racchiude) dentro
  `geometry`. Va **dentro** il modulo del file stesso (`main::geometry` per `main.typl`). Due di fila si
  annidano in ordine. È diverso dall'`in-package` di CL, e ha un nome diverso: in questo sistema il file
  è già un modulo, quindi non c'è nulla da "selezionare", e tutto ciò che una forma può fare è
  annidare.

### 3.11 File e moduli (progetti con più file)

Il percorso del file relativo alla radice dei sorgenti è il percorso del modulo:
il contenuto di `<root>/geo/point.typl` è implicitamente racchiuso nel modulo `geo::point`
(anche una directory è un segmento, nello stile di Rust / Python). Un `(module bar ...)` esplicito nel
file si annida **dentro** di esso (`geo::point::bar`), quindi il percorso derivato e una dichiarazione
esplicita non collidono mai.

- **Radice dei sorgenti**: metti un file manifesto `typelisp.toml` nella radice del progetto (può essere
  vuoto; facoltativamente una riga `src = "src"` indica la directory dei sorgenti). Viene trovato
  risalendo dalla directory del file di destinazione. Senza manifesto, la directory del file di ingresso
  (la directory corrente per il REPL) è la radice.
- **Caricamento su richiesta**: quando `(use geo::point)` fa riferimento a un modulo non ancora caricato,
  il file corrispondente (`geo/point.typl`) viene caricato, controllato nei tipi e registrato
  automaticamente. `use a::b::c` cerca prima il prefisso più lungo: `a/b/c.typl` → `a/b.typl` →
  `a.typl` (dato che `c` può essere un elemento dentro un modulo). Le definizioni visibili da altri
  moduli richiedono `pub` ([3.13 pub](#313-pub--visibilità)).
- **I riferimenti circolari sono errori**: la catena viene segnalata nella forma
  `circular module dependency: a -> b -> a`.
- **Esecuzione**: `typl <file.typl>` esegue un file (senza argomenti, il REPL). `use` nel REPL risolve i
  file con le stesse regole.
- **Capacità dell'arena dei cons**: `typl --heap-cells N` imposta la **capacità iniziale** dell'arena
  delle celle cons (predefinita 65536; funziona anche la forma `--heap-cells=N`, sia per l'esecuzione di
  file sia per il REPL). L'arena **cresce aggiungendone altre** quando scarseggia. Il limite di crescita
  è 256 volte la capacità iniziale, e un'allocazione oltre di esso dà `heap exhausted`: la capacità
  iniziale significa "alloca questa quantità all'inizio", e il limite significa "oltre questo, trattalo
  come una perdita".

### 3.12 load — caricamento piatto

```lisp
(load "path")   ; top level only; path is a string literal
```

- **Caricamento piatto** in stile CL: legge le forme del file di destinazione **nel namespace corrente**
  così come sono (senza racchiuderle in un modulo, a differenza di `use`). Solo al primo livello
  (dentro il corpo di una funzione è un errore di tipo).
- `path` è relativo alla directory del file che carica (dal REPL, alla cwd del processo). Se non ha
  estensione, viene aggiunto `.typl`.
- Anche `(load ...)`/`(use ...)` nel file caricato vengono elaborati ricorsivamente.
- **Legge una forma alla volta e la esegue subito** (come fa il `load` di CL). La forma *k* ha finito di
  essere eseguita prima che *k+1* venga letta: anche se c'è un errore di sintassi o di tipo a metà, le
  forme precedenti sono già state eseguite. I file di modulo caricati con `use` sono diversi: vengono
  controllati come un'unica unità e la loro esecuzione è lasciata a chi li importa con `use`
  (corrispondente al `compile-file` di CL).

### 3.13 pub — visibilità

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` si può mettere solo sugli undici tipi sopra (non su `module`/`use`/`deftrait`/`impl`). Si scrive con
la parola chiave della definizione subito dopo `pub`, non nella forma `(pub (defun ...))` che racchiude
la definizione tra parentesi. Un `pub` rende pubblica esattamente una definizione (non si possono
contrassegnare più definizioni in una volta).

### 3.14 defmacro — definizioni di macro

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- Tutti i parametri e il valore di ritorno sono sempre `Sexpr`, quindi non si scrivono annotazioni di
  tipo.
- Macro non igieniche in stile CL (evitare i conflitti con `gensym` è responsabilità dell'autore della
  macro).
- La lista dei parametri segue l'ordine di CL `required &optional &rest &key` (ogni marcatore al più una
  volta, e solo in quest'ordine).
  - `&optional` … argomenti facoltativi. `name` oppure `(name default-expr)`. L'espressione predefinita
    viene valutata al momento dell'espansione (può fare riferimento a parametri associati prima) e
    associata quando l'argomento è omesso (senza predefinito, la lista vuota `()`).
  - `&rest name` … riceve insieme, come un'unica lista `Sexpr`, gli argomenti posizionali rimanenti.
  - `&key` … argomenti con parola chiave. `name` oppure `(name default-expr)`. Il chiamante li passa come
    `:name value` (in qualsiasi ordine). Quando omessi, l'espressione predefinita (la lista vuota `()`
    se non c'è). Parole chiave sconosciute o una sequenza `:key` di lunghezza dispari sono errori.
- Esempi: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — binding locali di macro

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; lexically scoped macros
(symbol-macrolet ((name expansion) ...) body...)         ; a name stands for a form
```

Entrambe sono forme speciali di **espressione**, e a runtime non resta nulla (ciò che viene compilato è la
forma espansa del corpo). La lista dei parametri è la stessa di `defmacro`. Le regole dettagliate e gli
esempi si trovano in
[Binding locali di macro](functions/system.md#9-binding-locali-di-macro-macrolet--symbol-macrolet).

## 4. Binding e condizionali

```lisp
(let ((name val) ...) body...)      ; parallel binding
(let* ((name val) ...) body...)     ; sequential binding (earlier bindings usable in later initializers)

(if cond then else)                 ; else is required (always three elements)
(when cond body...)                 ; an if without else (Unit type). defmacro
(unless cond body...)               ; the negation of when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; a list of keys: matches if any of them does
  (else body...))                   ; expr is evaluated once. keys are compared with equal.
                                     ; keys are "literals" and are not evaluated (as in CL).
                                     ; a bare symbol a means the symbol 'a.
                                     ; writing 'a is an error (use the bare a). defmacro
(ecase expr (key body...) ...)      ; a case requiring a match. panics if nothing matches. defmacro
(ccase expr (key body...) ...)      ; CL's ccase. there are no restarts to offer, so it is the same as ecase. defmacro
(and expr...)                       ; short-circuit evaluation. true with zero arguments. defmacro
(or expr...)                        ; short-circuit evaluation. false with zero arguments. defmacro
(progn body...)                     ; runs in order and returns the last value
(unsafe body...)                    ; the same as progn, plus permission to write FFI calls
                                     ; and raw words. see 3.3 defffi
(prog1 form more...)                ; evaluates everything; the value is that of form. defmacro
(prog2 a b more...)                 ; evaluates everything; the value is that of b. defmacro
(the Type expr)                     ; a type annotation (no run-time effect)
```

### 4.1 unsafe — assumersi ipotesi che non si possono verificare

```lisp
(unsafe body...)
```

Uguale a `progn`: valuta il corpo in ordine e restituisce l'ultimo valore. Non crea alcuno scope e non è
un confine di funzione (`break` / `return-from` passano direttamente all'esterno). La differenza è che
alcune cose si possono scrivere solo al suo interno.

Attualmente tre cose richiedono `unsafe`: chiamare funzioni C dichiarate con
[defffi](#33-defffi--dichiarare-funzioni-c-ffi), rendere valori le parole macchina grezze (`ptr` /
`c-long` / `c-ulong` / `(ptr T)`), e [`def-c-struct` e `c-alloc`](#def-c-struct-e-puntatori-tipizzati--allocare-struct-c).

La memoria allocata con `c-alloc` viene liberata all'uscita dall'`unsafe` più esterno all'interno della
stessa funzione. Solo quell'`unsafe`, a differenza di `progn`, ha del lavoro da fare in uscita: la
liberazione.

Ciò di cui `unsafe` si fa carico sono le seguenti ipotesi che il compilatore non può verificare:

- **Che i tipi corrispondano.** Che la firma C dichiarata corrisponda a quella reale. In caso contrario,
  gli argomenti vanno nei registri sbagliati e i valori di ritorno vengono letti con la larghezza
  sbagliata.
- **La sicurezza della memoria.** Ciò che il lato C fa con ciò che riceve.
- **Lo stato dell'intero processo.** Variabili d'ambiente, handler di segnale, `errno`. Per esempio,
  chiamare `setenv` tramite la FFI rompe le ipotesi che il `decode-universal-time` di questa
  implementazione fa quando calcola l'ora locale.
- **La sicurezza dei thread.**

Non è una via di fuga dal controllo dei tipi. `(unsafe (+ 1 "two"))` non passa. Ciò che è permesso è
scrivere certe **operazioni**, non scrivere assurdità.

Funziona in modo lessicale. Il corpo di una `lambda` scritta dentro `unsafe` eredita il permesso (come le
chiusure dentro i blocchi `unsafe` di Rust). Il valore può essere chiamato più tardi da fuori
dall'`unsafe`, ma scriverlo lì è esso stesso considerato come accettare la responsabilità.

### 4.2 destructuring-bind — scomporre le liste per forma

```lisp
(destructuring-bind lambda-list form body...)
```

Scompone **per forma** la lista che `form` produce e la associa. La lista dei parametri è quella di
`defmacro` (required → `&optional` → `&rest`/`&body` → `&key`, ciascuno con espressioni predefinite), per
lo stesso motivo per cui CL ne condivide una tra i due: sono due forme che scompongono la stessa cosa.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Ogni variabile associata è un `Option<Sexpr>`.** Questa non è una limitazione dell'implementazione
  ma la natura di ciò che viene associato: le liste S-expression sono le uniche liste di questo
  linguaggio, quindi non c'è un altro tipo da dare agli elementi. Ripiegare su `match` dove serve uno
  scalare è come nel corpo di una `defmacro`.
- **Una forma che non corrisponde va in panic** (corrispondente all'errore di CL): troppo pochi o troppi
  elementi, una sequenza `&key` di lunghezza dispari, o una parola chiave sconosciuta. `sexpr-car` è una
  funzione permissiva che restituisce `()` per `()`, quindi senza il controllo, una lista corta verrebbe
  associata in silenzio a una sequenza vuota.
- **Le liste di parametri annidate non sono supportate.** Nemmeno `defmacro` le accetta, quindi c'è una
  sola regola. `(a (b c))` non associa in silenzio una sottolista a `b`; è un errore che lo dice.
- Le espressioni predefinite di `&optional` / `&key` vengono **valutate solo quando usate** (come in CL).
- Non c'è nulla di corrispondente a `&allow-other-keys` di CL (nemmeno `defmacro` ce l'ha).

### 4.3 match — pattern matching

```lisp
(match expr
  (pattern body...)
  ...)
```

Tipi di pattern:
- `_` — carattere jolly
- Un nome di variabile — un pattern di binding (corrisponde sempre). Tuttavia, se il tipo dello
  scrutinee ha una variante con quel nome, viene risolto come **il pattern con nome di variante semplice
  qui sotto**
- Un nome di variante semplice — corrisponde a una variante che non prende argomenti
  (`(match c (red 1) (blue 2))`). Scrivere una variante con campi con il suo nome semplice è un errore di
  arità, quindi scrivila tra parentesi, come in `(circle r)`
- **Letterali immediati**: interi / `true`/`false` / caratteri — confrontati come parole
- **Letterali di valore**: stringhe / numeri in virgola mobile / simboli (`'foo`) / interi bignum /
  rapporti — confrontati per valore con l'`Eq::equals` di quel tipo
  ([Trait standard](functions/traits.md#2-eq--ord-confronto)). Le stringhe si confrontano per contenuto,
  non per identità
- `(= expr)` — valuta un'espressione qualsiasi e confronta con `Eq::equals`. L'unico modo di confrontare
  tipi che non hanno sintassi di letterale (istanze di `defstruct`, globali, risultati calcolati), e
  un'implementazione `Eq` definita dall'utente diventa la regola di confronto così com'è. `expr` può
  fare riferimento a qualsiasi cosa visibile dalla posizione del ramo (argomenti, binding esterni,
  globali)
- `(Ctor sub-pattern...)` — pattern di costruttore (`Some x` `None` `Cons a d` `Ok v` e così via)

Confrontare un tipo che non implementa `Eq` con un letterale di valore / `(= expr)` è un errore di tipo
(questo linguaggio sceglie di dire "questi non si possono confrontare" anziché lasciare un ramo che in
silenzio non corrisponde mai).

**Letterali di valore contro uno scrutinee `Sexpr`**: l'`Eq` di `sexpr` è `eq` (l'identità di CL), quindi
gli immediati (`'foo` (internato) / interi / caratteri / `true`/`false`) si possono scrivere così come
sono e corrispondono per contenuto:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

I letterali non immediati (stringhe / numeri in virgola mobile / interi bignum / rapporti) **non si
possono scrivere** contro una `Sexpr`. Il loro `eq` confronta l'identità dell'oggetto, il che
produrrebbe un "ramo che supera il controllo dei tipi ma non corrisponde mai", quindi è un errore che
nomina il pattern di variante: scrivi `(str "hi")` e viene scomposto in una `string` e confrontato per
contenuto. `(= expr)` richiede esplicitamente `equals`, quindi questa restrizione non si applica ad
esso.

**Lo scrutinee non deve essere necessariamente un ADT.** `string`/`symbol`/`i32`/`f64` e simili si possono
confrontare direttamente (è lì che vanno i pattern con letterali stringa). Tuttavia, un tipo senza
varianti non può essere coperto per enumerazione, quindi serve `_` (o un pattern di binding che agisce da
carattere jolly):

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; a type without variants needs `_`
```

Contro uno scrutinee `Sexpr`, oltre ai 18 pattern di variante predefiniti sopra, si possono scrivere
**pattern di downcast** (estrazione di istanze di ADT definiti dall'utente): sintassi per riottenere,
con `match`, un'istanza di una `defstruct`/`defenum` (capitolo 3) che è stata convertita implicitamente
in `Sexpr`, come in `(list p 42)`:

- `(TypeName sub-pattern...)` — scomposizione dei campi con il **nome del tipo** per primo (solo
  strutture: una `defstruct` ha sempre una variante, quindi si scrive con il nome del tipo anziché con
  un nome di variante). Per esempio, per `(defstruct point (x f64) (y f64))`, `(point x y)`.
- Un nome di variante semplice `(VariantName sub-pattern...)` — estrae una variante di una `defenum`.
  Risolto come nome semplice visibile dopo `(use EnumType)` (le stesse regole di visibilità della
  chiamata del costruttore). Per esempio, per `(defenum color (red) (blue))`, `(red)` `(blue)` dopo
  `(use color)`. Se i nomi di variante di più enumerazioni visibili confliggono, è un errore di
  ambiguità, quindi si può scrivere anche la forma qualificata `(EnumType::VariantName ...)` (non serve
  `use`).
- `(the Type pattern)` — un downcast dell'intero tipo (associandolo nel suo insieme). Non scompone i
  campi; passa il valore a `pattern` così com'è. L'unico modo di estrarre una struttura mutabile
  mantenendone l'identità, e anche l'unico modo di estrarre un `Vector<T>`/`HashTable<K,V>` da una
  `Sexpr` (non hanno una forma di scomposizione dei campi). Per esempio, dopo `(the point p)`,
  `(setf p::x 9)` si riflette anche nell'istanza originale nella lista.

**Pattern per `Option<Sexpr>`**: il tipo dei dati S-expression non è `Sexpr` ma `Option<Sexpr>`, e la
lista vuota non è una variante di `Sexpr` ma il `none` di `Option`. Quindi quando si confronta un
`Option<Sexpr>`, le 18 varianti di `Sexpr` e `none` si possono scrivere **piatte nella stessa lista di
rami** (non serve un `match` esterno per togliere l'`Option`):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; the empty list
    (_          9)))
```

L'esaustività viene controllata nello stesso universo piatto: le 18 varianti di `Sexpr` più `none`, 19 in
tutto. Dimenticare `(none)` è un errore a meno che non ci sia un `_`. Si può scrivere anche `(some x)`,
che associa "qualcosa di non vuoto".

Questo zucchero si applica **esattamente** solo a `Option<Sexpr>`. Per `Option<Option<Sexpr>>`, non
sarebbe chiaro quale strato `(int n)` abbia tolto, quindi scrivi due livelli di `match` come al solito.

Gli stessi pattern di downcast si possono usare così come sono su **uno scrutinee oggetto-trait (`:dyn
Trait`, capitolo 2)**: `match` lo estrae dal box e poi lo consegna al meccanismo dei pattern `Sexpr`
sopra, quindi non c'è sintassi aggiuntiva. L'insieme dei tipi che implementano è aperto, quindi non può
mai essere esaustivo, e `_` è obbligatorio:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; field decomposition with the type name first
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Inferenza dei tipi tra i rami**: tutti i rami devono avere lo stesso tipo (tranne i rami che divergono,
come con `panic`). In un `match` scritto dove non è atteso alcun tipo, i rami si completano a vicenda gli
argomenti di tipo mancanti: `(result::ok v)` fissa solo `T`, e `(result::err e)` solo `E`, ma insieme
fissano `Result<T,E>`. Un argomento di tipo che nessun ramo riesce a fissare alla fine è un errore di quel
ramo (`cannot infer type argument ...`). Fuori da `match`, un argomento di tipo che non si può fissare è
un errore immediato.

Il controllo di esaustività di un `match` che usa pattern di downcast non li conta ai fini della
copertura delle varianti proprie di `Sexpr` (un `match` che elenca solo pattern di downcast deve essere
chiuso con `_`). Per gli ADT generici (`defstruct point<T> ...` e così via), gli argomenti di tipo di un
pattern di downcast non si possono inferire, quindi la forma di scomposizione dei campi (`(point ...)`) e
la forma con nome di variante semplice non si possono usare; indicali con `the`, come in
`(the point<i32> p)`.

**I downcast guardano anche l'istanziazione.** Gli argomenti di tipo espliciti vengono usati per la
corrispondenza: `(the point<i32> p)` lascia passare solo valori di `point<i32>`, e un `point<string>`
passa oltre al ramo successivo. Questo perché un valore ricorda il proprio tipo compresi gli argomenti di
tipo (lo stesso meccanismo che sceglie `print-object`).

```lisp
(if-let (pattern val) then els)     ; then (with bindings) if val matches pattern, else els. defmacro
(while-let (pattern val) body...)   ; loops while val (re-evaluated each time) matches pattern. defmacro
```

## 5. Iterazione

```lisp
(loop body...)                      ; an infinite loop. leave with break/return
(while test body...)                ; loops while test is true. defmacro
(until test body...)                ; loops while test is false (the negation of while). defmacro
(dotimes (var count-expr) body...)  ; evaluates count-expr once and runs var over 0..count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; CL-style iteration with parallel stepping. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; the sequential version of do (let* binding, assigned in order). defmacro
(doiter (var coll-expr) body...)    ; iterates over a value implementing the Iter trait. defmacro

(break)                             ; leaves only the innermost loop. the value is always Unit
(return)                            ; leaves only the innermost loop
(return value)                      ; leaves the innermost loop with a value
```

Sia `break` sia `return` escono **solo dal ciclo più interno che li racchiude** (non sono un ritorno
anticipato dalla funzione, e non possono attraversare il confine di una `lambda`). Il tipo di un `loop` è
l'unione dei tipi dei valori dei `break`/`return` trovati al suo interno (`!` se non se ne esce mai). Per
uscire da una funzione, usa `return-from`, sotto.

### 5.1 `block` / `return-from` — uscite con nome

```lisp
(block name body...)                ; a named exit target. the value is the last form,
                                    ; or the value passed by return-from
(return-from name)                  ; leaves that block with Unit
(return-from name value)            ; leaves with a value
```

**Ogni funzione di `defun` / `defmethod` / `labels` stabilisce implicitamente un block con il proprio
nome** (come in CL). Quindi `(return-from f v)` è un ritorno anticipato dalla funzione:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` è un'uscita **lessicale**, e il nome viene **risolto dove è scritto**: il checker associa un
`return-from` al `block` che lo racchiude e unisce il tipo del suo valore al tipo di uscita del block.
Quindi:

- Un `return-from` senza un `block` corrispondente è un **errore di tipo** (non un errore a runtime).
- Un valore il cui tipo non si adatta alle altre uscite o al tipo del corpo è un **errore di tipo** (la
  stessa regola dei rami di `match`).
- Se block con lo stesso nome sono annidati, **vince quello interno** (la regola di shadowing di CL).
- **Non può attraversare i confini di funzione.** Da dentro una `lambda`, non si può uscire verso un
  `block` esterno (`lambda` non stabilisce alcun block: i block impliciti di CL richiedono un *nome*, e
  le funzioni anonime non ne hanno). Ciò che deve attraversare è `catch`/`throw` (capitolo 8, che è
  **dinamico**).

Come `break`/`return` (capitolo 5), è un'uscita **statica**, quindi nel codice compilato è un salto a un
basic block fissato in fase di compilazione. Se c'è un `unwind-protect` nel mezzo, il suo `cleanup` viene
eseguito (capitolo 8).

Se non scrivi mai `return-from`, il block implicito non costa nulla.

### 5.2 `loop` esteso (il LOOP di CL)

**Se il primo elemento di `loop` è una parola chiave**, viene letto come una sequenza di clausole. Altrimenti
resta il semplice ciclo di cui sopra, e il significato dei `loop` esistenti non cambia (la stessa regola
del simple loop di CL).

CL scrive le parole di clausola come simboli semplici (`(loop for i from 1 to 3 collect i)`), ma qui
**tutte sono parole chiave**: un `for` semplice sarebbe solo un riferimento a variabile, ed essere una
parola chiave è anche ciò che lo distingue da un ciclo semplice. L'eccezione è `=`, che separa una
variabile da un valore: la sua posizione è non ambigua, quindi viene letto sia semplice sia come parola
chiave (`:=`).

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Clausole di variabile** (scritte prima delle clausole del corpo. È la regola di CL: scritte dopo,
potrebbero essere lette come "itera solo da lì in poi", quindi è un errore):

| Clausola | Significato |
|---|---|
| `:with v = e` | Associa una volta. Può leggere le variabili delle clausole precedenti |
| `:for v :in s` / `:for v :across s` | Gli elementi di un `Iter` in ordine. La distinzione di CL tra lista e vettore qui non esiste, quindi sono due grafie della stessa clausola |
| `:for v :on s` | I **suffissi** successivi. CL passa la coda cons condivisa, ma un `Iter` non ha una coda da condividere, quindi ciascuno è un nuovo `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Conteggio. Funzionano anche `:downfrom`/`:upfrom` |
| `:for v = e [:then f]` | Parte con `e`, e dalla seconda volta usa `f` (senza `:then`, `e` ogni volta) |
| `:repeat n` | Itera quel numero di volte |

Con più `:for`, avanzano **in parallelo**, e il ciclo termina non appena uno qualsiasi è esaurito.

**Clausole del corpo** (eseguite ogni volta, nell'ordine in cui sono scritte):

| Clausola | Significato |
|---|---|
| `:do form...` | Per gli effetti collaterali |
| `:collect e [:into v]` | Raccoglie in un `Vector<T>` |
| `:append e [:into v]` | Aggiunge il contenuto di un `Iter` |
| `:sum e` / `:count e` | La somma / il numero di volte in cui è stato vero |
| `:maximize e` / `:minimize e` | Il massimo / minimo. **`Option<T>`** (proprio come CL restituisce nil per una sequenza vuota; un tipo `Ord` arbitrario non ha un elemento minimo) |
| `:always e` / `:never e` | `true` se tutti valgono; `false` subito quando uno fallisce |
| `:thereis e` | `e` è un **`Option<T>`**. Restituisce il primo `some`, oppure `none` se non ce n'è (è ciò che corrisponde al "primo valore non nil" di CL; per verificare un `bool`, usa `:always`/`:never`) |
| `:while e` / `:until e` | **Termina normalmente** qui (`:finally` viene eseguito, e ciò che è stato raccolto è la risposta) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Rende condizionale una clausola |
| `:return e` | Esce subito con quel valore (`:finally` non viene eseguito, come in CL) |
| `:initially form...` / `:finally form...` | Prima del ciclo / al completamento normale |

**`:named name`** (prima di qualsiasi altra clausola, una sola volta) avvolge l'intero ciclo in
`(block name …)`. `(return-from name e)` può uscire subito anche da dentro cicli annidati, e come
`:return`, `:finally` non viene eseguito. Senza un nome, non viene stabilito alcun block: il `loop` senza
nome di CL stabilisce `block nil`, ma qui non c'è `nil`, e `break`/`return` (capitolo 5) forniscono già
"esci dal ciclo più interno".

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

Omettere `:finally (return 0)` è un **errore di tipo**. Sono semplicemente le regole di `block` al lavoro
(5.1): il tipo dell'uscita `int` non si adatta al `()` con cui il ciclo termina quando è esaurito.

**Il valore del ciclo** è l'accumulo della clausola di accumulo se ce n'è una (la prima, se ce ne sono
più), `true` per `:always`/`:never`, `none` per `:thereis`, e `()` se non c'è. Se l'ultima cosa in
`:finally` è `(return e)`, quello è il valore: l'idioma `finally (return …)` di CL, l'unico modo in cui un
ciclo che non accumula può indicare la propria risposta.

**Differenze rispetto a CL / che cosa non è incluso**:

- **Le parole di clausola sono parole chiave** (sopra).
- `:maximize`/`:minimize`/`:thereis` restituiscono `Option<T>` (non c'è nil).
- **Scrivere solo `:return`, senza accumulo né `:finally`, è un errore.** CL restituisce nil quando è
  esaurito, ma qui non esiste, quindi il ciclo deve dire quale sia il suo valore quando è esaurito.
- Unire clausole parallele con `:and`, `:being`/l'iterazione dedicata sulle tabelle hash, `:it` e `:nconc`
  non sono inclusi.
- Il tipo dell'elemento di `:collect` viene dal tipo dell'espressione accumulata. Tentare di raccogliere
  un tipo che **non si può scrivere come nome di tipo**, come un tipo funzione, è un errore che lo dice.

## 6. Valori funzione e chiamate

```lisp
(lambda (params) RetType body...)   ; makes a first-class function value (a closure)
(labels ((name (params) RetType body...) ...) body...)   ; local function definitions that can be mutually recursive
(apply f arg1 ... argN rest-list)   ; calls f (a variadic function with &rest), spreading rest-list
```

Anche le funzioni con nome si possono passare così come sono come valori (come argomenti di funzioni di
ordine superiore e così via).

## 7. Altre forme speciali

```lisp
(setq var value ...)                ; CL's variable assignment. just a sequence of (setf var value). defmacro
(psetq var value ...)               ; parallel assignment. evaluates all values first, then assigns. defmacro
(psetf place value ...)             ; psetq generalized to places (the same expansion). defmacro
(setf place value)                  ; assignment to a place. a place is a variable name / var::field /
                                     ; a call of the form (accessor recv key...). valid if the
                                     ; static type of recv has an instance method named
                                     ; set-{accessor} (for the get of Vector<T> and HashTable<K,V>,
                                     ; set corresponds as an exception; otherwise set-accessor-name).
                                     ; the value is the value assigned (as in CL). so
                                     ; in (if c (setf x 1) ()), then and else do not have matching types
(incf place)  (incf place delta)    ; place += delta (delta=1 if omitted). the result is as with setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 if omitted)
(rotatef place1 place2 ... placeN)  ; rotates N places (new place1=old place2, ...,
                                     ; new placeN=old place1). each place's subforms evaluated once
(shiftf place1 ... placeN newvalue) ; shifts the values of place2..N left and puts newvalue in placeN.
                                     ; the return value is the old value of place1
(list e1 e2 ... en)                 ; expands to (cons e1 (cons e2 (... ()))). () with zero arguments.
                                     ; each element is converted to Sexpr implicitly (like CL's cons, it
                                     ; can hold any value). scalars (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) are wrapped in the matching Sexpr variant, and defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> and the like go in as they are
                                     ; (at no conversion cost). the same for &rest/format arguments.
(source-file)                       ; the name of the file this form was read from (string). fixed as a
                                     ; constant at check time. corresponds to CL's *load-pathname*, but is
                                     ; not a variable: module bodies run after checking, so "currently
                                     ; loading" cannot be relied on, while at check time it is always known.
                                     ; for sources that are not files, the reader's name for them (<stdin>/<input>)
(quote datum)                       ; the same as 'datum. returns it as Sexpr data without evaluating
(quasiquote template)               ; the same as `template. embeds expressions in the template with ,/,@
(documentation name)                ; returns the docstring of name (a bare name or Type::method) as Option<string>
(panic message)                     ; message: string. ends abnormally with an unrecoverable error. type !
(unreachable)                       ; expands to (panic "unreachable"). defmacro
(todo)                              ; expands to (panic "todo"). defmacro
(as Type expr)                      ; numeric/character type conversion. conversions that can fail panic on failure
(try-as Type expr)                  ; like as, but returns the result as Option<Type> (None on failure)
(print control args...)             ; expands the format and writes to standard output (no newline)
(println control args...)           ; the same (with a newline at the end)
(format dest control args...)       ; CL's format. returns the expanded string
(pprint x)                          ; pretty-prints. writes a newline first, as in CL
(pprint-fill x)                     ; fill layout
(pprint-linear x)                   ; all on one line or one element per line
(pprint-tabular x [colinc])         ; tabular layout (16 columns by default)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; build a logical block yourself
```

La famiglia `print`/`println`/`format`/`pprint` è composta da forme speciali, quindi i loro argomenti
variadici (un singolo oggetto per la famiglia `pprint`) vengono incapsulati in `Sexpr` con i loro tipi
prima di essere passati: è per questo che `(println "~a" my-struct)` funziona e basta. I dettagli delle
direttive di formato e del pretty printer si trovano in [Direttive di formato](functions/format.md) e
[Stampa](functions/printing.md#4-il-pretty-printer).

`as`/`try-as` trattano solo il catalogo numerico e dei caratteri (tra `int`, i tipi interi a larghezza
fissa, `f32`/`f64`/`ratio`/`char`). Lo stesso tipo non è una conversione. **Le conversioni tra larghezze
intere (incluso `int`) e tra `f32`↔`f64` sono conversioni vere**: `as` tronca / arrotonda, e `try-as`
risponde se sta in quella larghezza (precisione). `(as int x)` è l'allargamento esatto da una larghezza
fissa, e `(as i32 n)` il troncamento da `int`. Intero → `char` può fallire fuori intervallo, quindi `as`
va in panic e `try-as` dà `None`. Tutto il resto (allargamento, e il troncamento di
`float->int`/`ratio->int`) riesce sempre. `float->int`/`ratio->int`/`char->int` arrivano a `int`, e se si
chiede una larghezza più stretta, dopo di esse viene chiamato `int->W`. Questo è zucchero che si espande
nei corrispondenti metodi di conversione (`int->char`/`int->int`/`int->W` e così via in
[Numeri](functions/numbers.md)).

`documentation`, come `quote`/`compile`, è una forma speciale che legge `name` senza valutarlo, come
simbolo semplice non valutato / percorso `::`. A differenza del `(documentation 'name 'function)` di CL,
non prende un argomento di tipo: risolve `name` nell'ordine variabile → funzione → tipo → trait → macro
(la stessa priorità di quando un identificatore semplice viene valutato come espressione) e restituisce
la docstring della definizione trovata (`(documentation Type::method)` è per i metodi). Non riuscire a
risolvere (nessuna definizione con quel nome) è un errore in fase di controllo; una definizione che
esiste ma non ha docstring dà `Option::none`. Tutto viene deciso come costante in fase di controllo:
non avviene alcuna ricerca a runtime. I nomi liberi qualificati dal modulo (`mod::name`, tranne
`Type::method`) non sono supportati.

## 8. Uscite non locali (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; runs body. if (throw 'tag v) happens anywhere
                                    ; body reaches, that v becomes the value
(throw 'tag value)                  ; exits to the nearest dynamically enclosing (catch 'tag ...)
(unwind-protect protected cleanup)  ; runs cleanup however protected is left
```

A differenza di `break`/`return` (capitolo 5), questa è un'uscita **dinamica**: `throw` non cerca
lessicalmente il `catch` che lo circonda, e raggiunge un `catch` con lo stesso tag attraverso un numero
qualsiasi di chiamate di funzione.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; if not found, the value at the end as usual
```

- **I tag sono solo simboli letterali** (`'done`). A differenza di CL, non vengono valutati.
- **Un tag porta un tipo.** Il tipo viene deciso la prima volta che si usa `'tag`, e ogni successivo
  `throw`/`catch` dello stesso simbolo viene controllato rispetto ad esso. Usarlo con un altro tipo è un
  errore di tipo.
- Il tipo di `throw` è `!` (diverge). Il tipo di `(catch 'tag expr)` è l'unione del tipo di `expr` e del
  tipo del tag.
- Il valore di `unwind-protect` è il valore di `protected`. Il valore di `cleanup` viene scartato.
  `cleanup` viene eseguito in qualunque modo si esca da `protected`: oltre al completamento normale,
  `throw` e `panic`, viene eseguito anche quando si esce con `break`/`return`/`return-from`. Un'uscita
  non locale dello stesso `cleanup` prevale sull'uscita in corso.
- Gli `unwind-protect` annidati vengono eseguiti dall'interno verso l'esterno. Un `break` che esce da un
  ciclo **dentro** `protected` non è uscito da `protected`, quindi il suo `cleanup` non viene eseguito.

Le condizioni di CL (`define-condition`/`handler-bind`/`invoke-restart`) non sono adottate. Non si
adattano alla tipizzazione statica, quindi i fallimenti recuperabili si esprimono con `Result`
(capitolo 9).

## 9. Politica di gestione degli errori

- Fallimenti recuperabili: `Result<T,E>` + `match`. Fallimenti irrecuperabili (bug, invarianti rotti):
  `panic`.
- Non c'è sintassi corrispondente a `?`/try. I rami si scrivono esplicitamente con `match`.
- I nomi di funzioni e forme speciali non usano `!` (operazioni distruttive) o `?` (predicati) come
  suffissi. I predicati si chiamano con un suffisso `-p`/`p` (`zerop`, `consp` e così via) o un prefisso
  `is-` (`is-some`, `is-ok` e così via).

## 10. Compilazione

```lisp
(compile name)                      ; JIT-compiles an already defined defun/method into native code
(compile-file src-path out-path)    ; AOT-compiles a source file into a native executable (skips the final `(main)`)
(dump path)                         ; writes the current environment (type information + compiled bodies) to one file
(disassemble name)                  ; prints what that definition becomes (host machine code by default, LLVM IR with true as the second argument)
```

`compile` è una forma speciale; `name` non viene valutato e viene letto come simbolo semplice non valutato
/ percorso `::` (una stringa è un errore di tipo). Le funzioni generiche non possono essere il bersaglio:
una copia per ciascun tipo viene creata in ogni punto d'uso, quindi non esiste un unico corpo compilato.
**Un nome che non si può risolvere è un errore in fase di controllo** e non viene mai portato al runtime
(ci sono messaggi separati per: il tipo esiste ma non quel metodo / né il tipo né la funzione esistono /
un nome semplice non definito). La visibilità qui è trattata come per qualsiasi altro riferimento:
"esiste ma non è visibile da qui" fallisce in fase di controllo, esattamente come "non si risolve".

Anche i chiamati vengono compilati in modo transitivo, quindi **una funzione che (anche indirettamente)
chiama qualcosa che non può essere compilato non può essere compilata**. Il processo non va in crash;
viene rifiutata con un errore che lo dice. Ogni funzione predefinita può essere compilata, quindi le uniche
funzioni rifiutate in questo modo sono quelle che chiamano le seguenti operazioni solo dell'interprete:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

Quelle solo dell'interprete sono `compile`/`compile-file`/`dump` e
`trace`/`untrace`/`step`/`disassemble`
([Strumenti di implementazione](functions/system.md#5-strumenti-di-implementazione-clhs-252)). Più che
cose che non si possono compilare, sono operazioni del lato che compila (ciò che `dump` scrive è
l'ambiente stesso dell'interprete, che un eseguibile AOT non ha; ciò che `trace` osserva e dove `step` si
ferma sono i percorsi di chiamata dell'interprete in esecuzione; e `disassemble` usa il compilatore
stesso). `room`/`dribble`/`ed` non ne fanno parte e possono essere compilate normalmente.

Ciò che **si può** compilare: l'I/O su stream e file, `random`, `gensym`,
`symbol->string`/`string->symbol`, `parse-int`/`parse-float`,
`get-universal-time`/`get-internal-real-time`, `exit`, le funzioni trascendenti, le operazioni sui bit,
`catch`/`throw`/`unwind-protect`, tutti e quattro `eq`/`eql`/`equal`/`equalp` (il che permette di
compilare `case` per ogni tipo), l'intera famiglia di stampa compresi
`print`/`println`/`format`/`pprint` e `pprint-logical-block`, `read` ed `eval`. La libreria standard viene
distribuita già compilata.

Un eseguibile AOT contiene solo le funzionalità che il programma usa. Un programma che non stampa non
riceve il motore di formattazione, uno che non chiama `read` non riceve il lettore, e uno che non chiama
`eval` non riceve né il checker né l'interprete.

Dalla riga di comando, `typl -c src-path [-o out-path]` (`-c` si può scrivere anche `--compile`) fa lo
stesso di `compile-file`. Senza `-o`, l'output è `src-path` senza l'estensione `.typl`. Per impostazione
predefinita, la libreria statica `libtypelisp_front.a` collegata agli eseguibili è, per una build di
release di `typl`, quella che `typl` porta al proprio interno, scritta al primo collegamento in
`$TYPELISP_HOME/lib/<ID della build>/` (oppure `~/.typelisp/lib/<ID della build>/` senza
`TYPELISP_HOME`) e usata da lì; per una build di debug, quella in cui `typl` è stato compilato.
`typl --remove-lib` elimina ciò che quel `typl` ha scritto. Con `--others`, elimina quelle di altri ID di
build; con `--all`, quelle di ogni ID di build. Con `typl --lib-dir DIR`, viene usata quella in `DIR`
(sia per `-c` sia per `compile-file`), e se non c'è, è un errore all'avvio.

### 10.1 Dump

```lisp
(dump "session.typld")     ; write one out
```
```sh
typl --image session.typld prog.typl   # start from it
typl --image session.typld             # the REPL too
```

Un dump contiene informazioni di tipo e corpi compilati in un unico file. Ciò che `(dump path)` scrive è
ciò che la sessione corrente ha caricato (la libreria standard, o un dump passato con `--image`) più
**ciò che la sessione stessa ha definito**. Quindi l'output è autosufficiente, e `typl --image` ripristina
lo stesso ambiente. Ciò che la sessione ha `(compile f)`ato viene scritto nella sua forma compilata.

Ciò che viene salvato sono **definizioni, non cronologia**:

- Le espressioni di primo livello della sessione (`(println ...)` e così via) non sono incluse. Sarebbe
  un problema se il caricamento le rieseguisse.
- Le variabili globali tornano con **il valore del loro inizializzatore eseguito di nuovo**, non il
  valore al momento del dump. È una differenza deliberata rispetto al `save-lisp-and-die` di SBCL (che
  scrive lo heap così com'è), e questa scelta fa sparire un'intera famiglia di problemi: i "valori che non
  si possono salvare", come stream aperti, puntatori a funzione di chiusure e memoria esterna.
- A differenza di `save-lisp-and-die`, **il processo non muore**, dato che scrivere non danneggia
  l'immagine.

Un dump registra le versioni della libreria standard e del compilatore dell'implementazione che l'ha
scritto. Caricarlo con un `typl` di versione diversa è un errore; non viene mai accettato in silenzio.

### 10.2 `eval` negli eseguibili AOT

`eval` controlla i tipi rispetto all'"ambiente globale corrente" e poi valuta
([Analisi e valutazione](functions/system.md#6-analisi-e-valutazione)). Quell'ambiente (le tabelle di
firme, tipi e macro che il checker consulta, e i corpi che l'interprete può eseguire) **non è nel codice
macchina**. Una funzione compilata non è altro che un simbolo collocato a un indirizzo; non ha né i tipi
dei suoi argomenti né una tabella per cercare i corpi per nome.

Quindi, solo per i programmi che chiamano `eval`, `compile-file` **costruisce quell'ambiente in fase di
compilazione e lo scrive nell'eseguibile**. Il formato è lo stesso di un dump, e contiene la parte della
libreria standard e la parte propria del programma. Tutto ciò che avviene all'avvio è il suo
ripristino: il sorgente non viene riletto, e nulla viene controllato di nuovo nei tipi. Ai programmi che
non chiamano `eval` non viene aggiunto nulla.

Conseguenze:

- **L'avvio richiede più tempo e l'eseguibile è più grande**, dato che entrano il codice del checker e
  dell'interprete e un'istantanea dell'ambiente. Anche lo heap viene reso un po' più grande.
- **Le forme passate a eval vengono interpretate.** Anche quando la forma passata a eval chiama le
  funzioni del programma stesso, ciò che viene eseguito è il corpo interpretabile che l'istantanea
  contiene. Il risultato è lo stesso; differisce solo la velocità.

La memorizzazione delle variabili globali è **condivisa** con il codice compilato (gli stessi slot).
L'inizializzatore di una `defvar` viene eseguito una volta dall'inizializzazione compilata, e il
ripristino lo salta, quindi un inizializzatore con effetti collaterali non viene eseguito due volte.

`compile-file` legge anche la libreria standard (e ne incorpora i corpi nell'eseguibile), quindi le
funzioni della libreria standard come `abs`/`gcd`, e `(impl print-object ...)` così come
`(defmethod print-object ...)`, si possono usare con AOT.

`compile-file` accetta anche `use` (e `import`/`shadowing-import`). Il `(use m)` del file di ingresso
trova i file con le stesse regole di `typl file.typl`, e anche i file di dipendenza trovati vengono
compilati e collegati nell'eseguibile: una struttura in cui `main.typl` legge `http.typl` tramite
`(use http)` può essere compilata con AOT così com'è. Anche le definizioni del file di ingresso vanno nel
modulo che porta il nome del file, come con `typl file.typl` (`point` in `p.typl` è `p::point`). Quindi la
rappresentazione stampata dei valori (`#<p::point x: 1 y: 2>`) è la stessa in qualunque modo venga
eseguito.

## 11. Reader macro (readtable)

Ciò che il lettore **fa quando incontra un certo carattere** può essere sostituito dal programma
(CLHS 23.1).

```lisp
(set-macro-character c f)             ; f reads the character c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f reads the two-character sequence d s
(get-dispatch-macro-character d s)    ; Option<f>
```

Il tipo di `f` è `(fn (string-input-stream char) Option<Sexpr>)`. Il primo argomento è **uno stream sul
testo non ancora letto**, e il secondo è **il carattere che lo ha attivato** (il secondo carattere per un
dispatch). Il valore di ritorno diventa il dato letto in quel punto. Lo stream è un tipo concreto anziché
`:dyn PeekInput` perché il lettore passa sempre questo unico tipo: `read-sexpr` / `read-char` /
`peek-char` / `unread-char` / `read-delimited-list` prendono tutti `(where (PeekInput S))`, quindi
funzionano tutti sul tipo concreto così com'è.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => read as (not (equal 1 2)), that is, true
```

Il lettore **guarda i caratteri macro prima della sintassi predefinita**, quindi può prendere il controllo
anche di `(` e `'`. I sotto-caratteri di `#` registrati in questo modo hanno la priorità sui predefiniti
`#b`/`#x`/`#.`. Un carattere diverso da `#` diventa subito un carattere di dispatch quando viene passato a
`set-dispatch-macro-character`: **non** c'è un equivalente del `make-dispatch-macro-character` di CL. La
registrazione svolge già il proprio compito, quindi un passo separato non avrebbe nulla da fare.

**Quando hanno effetto** dipende dal percorso di lettura, come per `#.` (capitolo 1):

- Il REPL e `(load ...)` eseguono una forma alla volta, quindi le **funzioni definite nelle forme
  precedenti** si possono registrare così come sono.
- I file di modulo vengono controllati come unità ed eseguiti dopo, quindi **vengono eseguite subito solo
  le chiamate a `set-macro-character` / `set-dispatch-macro-character`** (il ruolo di
  `(eval-when (:compile-toplevel) ...)` di CL). Poiché vengono eseguite subito, **la funzione passata
  deve già esistere in quel punto**. Una `defun` nello stesso file non è ancora stata eseguita, quindi
  scrivi una `lambda`, oppure usa la libreria standard o qualcosa che è già stato eseguito. Sono coperte
  solo le chiamate di primo livello; non guarda dentro `progn` o `let`.

Anche i `read` / `read-from-string` predefiniti consultano la readtable (come in CL).

**Che cosa non c'è**: `*readtable*` e `copy-readtable`, e `readtable-case`. I primi due perché una
readtable **non è un valore**: un valore dovrebbe essere "qualcosa che si può consegnare a un lettore",
ma il lettore che legge il sorgente è fuori dal programma, senza un posto a cui consegnarlo.
`readtable-case` perché il capitolo 1 stabilisce che il lettore di questo linguaggio converte sempre in
minuscolo (il `:downcase` di CL).

## 12. Concorrenza (task)

**Un task è un thread leggero** (in termini di Go, ciò che avvia un'istruzione `go`) e viene eseguito in
modo cooperativo (non c'è prelazione). L'alternanza non passa per il kernel, e lo stato di esecuzione
vive sullo heap anziché su uno stack macchina, quindi creare molti task costa poco.

**I task vengono eseguiti contemporaneamente su più thread del sistema operativo** (parallelismo
multicore). Il numero di thread è la variabile d'ambiente `TYPELISP_THREADS` (il totale, incluso il
thread che esegue `main`; il valore predefinito è il parallelismo della macchina). In `typl`, **solo i
task compilati** vengono eseguiti su altri thread, e i task interpretati vengono eseguiti sul thread
dell'interprete (12.7). I dati condivisi passano attraverso `Mutex<T>` o `Chan<T>`; letture e scritture
simultanee che non lo fanno sono indefinite, come in Go (12.7).

Del vocabolario, **solo `task` / `thread` / `select` sono forme speciali**; il resto sono funzioni, metodi
e macro ordinari ([Task e canali](functions/concurrency.md)).

### 12.1 `task` — avvio di un task

```lisp
(task (f arg...))                   ; returns Task<T>, where T is the return type of f
```

**Accetta solo la forma di una chiamata.** `f` e ogni `arg` vengono valutati dove è scritto il `task`,
nell'ordine in cui sono scritti, e solo **la chiamata** avviene nel nuovo task. È la stessa regola di
`go f(x)` di Go, ed è anche il motivo per cui prende una forma di chiamata anziché un thunk: un thunk
catturerebbe i suoi argomenti senza valutarli.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i is evaluated on the spot each time; no capture trap

(task ((lambda () ()                ; to run an arbitrary body, call a lambda
         (println "start")
         (send ch 1))))
```

Le forme speciali (`if` / `let` / `progn` …) non si possono scrivere direttamente sotto `task`.

**Perché non può essere una funzione**: scrivere `(spawn (lambda () T body...))` richiederebbe di
scrivere `T` per esteso, dato che `lambda` richiede un'annotazione del tipo di ritorno, e una macro non
conosce il tipo di ritorno di `(f a b)`. Solo il checker lo conosce.

### 12.2 `thread` — avvio di un task su un thread OS dedicato

```lisp
(thread (f arg...))                 ; returns Thread<T>, where T is the return type of f
(join th)                           ; waits for completion and returns its value (any number of times)
```

La forma e le regole di valutazione sono le stesse di `task` (accetta solo una forma di chiamata, e `f` e
`arg` vengono valutati dove è scritto). La differenza è dove viene eseguito: **avvia un thread del
sistema operativo dedicato a quel task e viene eseguito solo su di esso**. Non è multiplexato con altri
task, quindi chiamare al suo interno una funzione C bloccante (`defffi`) ferma solo quel thread, e gli
altri task fanno progressi. Al suo interno si possono usare così come sono `task`, `send`, `recv` e il
resto.

- `Thread<T>` è la controparte di `Task<T>`. Come `wait`, `join` ferma **il task chiamante**, e il valore
  viene messo in cache. Quando il task termina, termina anche il thread.
- Le regole di panic sono le stesse di `task` (cade l'intero processo). Quando `main` ritorna, il
  processo termina.
- Per scriverlo come funzione, usa `(Thread::spawn (lambda () T body...))` (il `std::thread::spawn` di
  Rust). Si può passare anche una funzione con nome.
- **Solo il codice compilato viene eseguito su un thread dedicato.** Quando `typl` valuta
  `(thread (f ...))` o `Thread::spawn` mentre interpreta, compila al volo la funzione da eseguire (e ciò
  che chiama) prima di eseguirla. Ciò che non si può compilare (una `lambda` che fa riferimento a
  variabili locali esterne, la costruzione di una struttura e così via) è, prima che il thread venga
  avviato, un panic trattato come un `(panic ...)`. Una `lambda` che fa riferimento a variabili locali
  si può passare se è creata dentro una funzione compilata.

### 12.3 `select` — attesa di più operazioni su canali contemporaneamente

```lisp
(select
  ((v (recv ch1)) body...)          ; a receive arm. v is bound to an Option<T>
  ((send ch2 x) body...)            ; a send arm
  (else body...))                   ; optional. **if written, it goes last**
```

- **Con `else`, non blocca** (il `default` di Go). Senza, attende finché una diventa possibile.
- **Se più rami sono possibili contemporaneamente, ne viene scelto uno a caso** (in ordine di
  scrittura, i rami successivi andrebbero in starvation).
- La `v` di un ramo di ricezione è un **`Option<T>`**. Un canale chiuso è "una risposta", non un motivo
  per saltare il ramo, quindi fai `match` su di essa dentro il ramo.
- Il tipo è **l'unione dei tipi dei corpi di tutti i rami** (la stessa regola dei rami di `match`).
- `(select)` con zero rami è un errore di tipo (il `select{}` di Go, che blocca per sempre, non è
  adottato). Lo è anche un `select` con solo `else`, dato che equivale a scrivere direttamente il corpo.

**Le espressioni dei canali e i valori da inviare vengono valutati una volta ciascuno, da sinistra a
destra, qualunque ramo venga scelto** (la stessa disciplina che `case` ha per le sue chiavi).

```lisp
(select                             ; receiving with a timeout
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([un canale che consegna dopo un certo tempo](functions/concurrency.md#5-after--un-canale-che-consegna-dopo-un-certo-tempo))
è "un canale che consegna un valore dopo `sec` secondi", corrispondente al `time.After` di Go.

### 12.4 Interazione con le altre funzionalità

| Funzionalità | Come si relaziona ai task |
|---|---|
| `catch` / `throw` | **Non attraversano i confini dei task.** Un `throw` che tenta di uscire dal corpo di un task è un panic |
| `unwind-protect` | La pulizia viene eseguita quando un task termina naturalmente. **Non viene eseguita quando il processo termina perché il task principale è terminato** |
| `block` / `return-from` | Lessicali, quindi non attraversano i confini delle `lambda` |
| `panic` | Come in Go, cade l'intero processo. `wait` non osserva un panic come valore |
| `dlet` | **Non è un binding per task.** Continua a "prendere in prestito e restituire una globale", quindi i task interferiscono tra loro |
| Standard output | Condiviso da tutti i task. L'output di un singolo `println` non viene mai mescolato con altri in mezzo a una riga |
| `compile` / `eval` | Nessuna restrizione. `(compile f)` dentro un task funziona |

### 12.5 Dove i task si alternano

La pianificazione è cooperativa, quindi **i task si alternano solo dove lo scrivi**: `(yield)`,
`(sleep ...)`, `(wait ...)`, le **operazioni su canali che devono attendere** (`send`/`recv`/`select`), e
le **operazioni su socket che devono attendere** (`accept` / `tcp-connect` (inclusa la risoluzione dei
nomi) / lettura e scrittura di socket / `recv-from`; [Rete](functions/network.md)). Tutti i socket sono
non bloccanti: se uno non è pronto, si ferma solo quel task, e riprende quando il sistema operativo dice
che è pronto, nella stessa forma del netpoller di Go. Solo quando nessun task può essere eseguito
l'implementazione attende il sistema operativo fino alla scadenza di `sleep` più vicina.

Le operazioni su canali che possono rispondere subito (un `send` con spazio nel buffer, un `recv` con un
valore in attesa, `(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **non consumano il turno**. Questo
significa che non vieni interrotto inaspettatamente da una lettura, ed è trattato in modo diverso da
`(sleep 0.0)`, che è il "cedi il passo per 0 secondi" di CL.

**Non c'è prelazione.** Un ciclo stretto che non chiama nulla manda in starvation gli altri task.
Tuttavia, i cicli compilati cedono periodicamente il controllo allo scheduler, quindi un ciclo stretto
compilato non li manda in starvation.

### 12.6 Codice compilato e task

Anche il codice compilato può sospendere i task. Lo stesso vale per gli eseguibili creati con
`compile-file`: `main` viene eseguito come task principale dello scheduler, e `task`, `sleep`, `wait`, i
canali e le attese su socket funzionano tutti con lo stesso significato che hanno in `typl`. Quando
`main` ritorna, il processo termina e i task rimanenti vengono interrotti (come in Go). L'interprete non
viene mai inserito nell'eseguibile per amore dello scheduler.

L'unica eccezione è "dentro un callback della FFI C", dove le operazioni che **dovrebbero attendere**
sono errori (più amichevole di un deadlock silenzioso): mentre una funzione passata con `defffi` viene
chiamata da C, lo stack di C è in cima, e non c'è modo di sospendere il task e riprenderlo più tardi.

Anche i punti seguenti sono funzioni chiamate nel mezzo di un task, ma non possono sospendere: i metodi
`print-object`, `~/name/` in `format`, le reader macro, l'interno di `eval` e gli inizializzatori
`defvar` negli eseguibili AOT. Qui, **le operazioni che rispondono senza attendere passano** (`(recv ch)`
con un valore nel buffer, `read-line` su un socket con dati già ricevuti, `(task ...)`, `(yield)` e così
via), e **le operazioni che dovrebbero davvero attendere sono errori** (non fermano il processo
all'istante, ma sono un panic come `` `recv` cannot block: ... ``, trattato come un `(panic ...)`).

### 12.7 Differenze rispetto a Go

- **In `typl`, solo i task compilati passano ad altri thread.** Lo stato dell'interprete non può essere
  condiviso tra thread, quindi i task di un `task` interpretato vengono eseguiti sul thread
  dell'interprete. Anche un task compilato **passa al thread dell'interprete e vi resta** (non torna
  indietro) nel punto in cui chiama un valore funzione interpretato, chiama un metodo `:dyn` che nessuno
  ha compilato, o chiama `eval`/`macroexpand`/`read`. Se un calcolo lungo tocca anche una sola volta
  codice interpretato lungo il percorso, il resto viene eseguito sul thread dell'interprete.
- **In `typl`, i worker vivono solo per una valutazione di primo livello.** Mentre il REPL attende input,
  e tra le forme di primo livello, gli altri thread non fanno avanzare i task (i task rimanenti
  proseguono da dove si erano fermati nella valutazione successiva). Alla fine di una valutazione,
  attende che ogni thread termini il proprio passo corrente, quindi se una funzione C (`defffi`) continua
  a bloccarsi dentro un `thread`, la valutazione non termina finché non ritorna.
- **Stampa sui worker**: i metodi `print-object` / `~/name/` interpretati non possono essere eseguiti su
  altri thread, quindi stampare tali valori su un altro thread è un panic trattato come un `(panic ...)`
  (`(compile T::print-object)`, oppure stampa dal task principale).
- **Le corse sui dati sono indefinite** (la stessa posizione di Go). Il risultato di più task che
  modificano lo stesso valore senza passare per `Mutex<T>` / `Chan<T>` non è garantito.
- **`task` restituisce un valore.** A differenza dell'istruzione `go` di Go, restituisce un `Task<T>`, e
  `(wait t)` ottiene il risultato.
- **Non ci sono canali nil.** L'idioma fan-in di Go (impostare a `nil` un canale chiuso per toglierlo dai
  rami di `select`) non si può scrivere, quindi avvia un task per ogni ingresso e uniscili con un
  `WaitGroup` ([WaitGroup](functions/concurrency.md#4-waitgroup--attendere-n-completamenti)). È anche il
  modo raccomandato in Go, ma è **la prima differenza in cui si imbattono le persone che vengono da
  Go**.
