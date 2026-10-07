<!-- translated-from: editor/vscode/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp (VS Code)

Un'estensione di VS Code per modificare sorgenti typelisp (`.typl`).
La versione per Emacs si trova in [../emacs/](../emacs/README_it.md). Le due condividono le stesse
tabelle di parole chiave e le stesse regole di indentazione, e `cargo test --test editor_keyword_sync_test`
lo verifica in modo meccanico (vedi sotto).

## Funzionalità

- **Evidenziazione della sintassi** (una grammatica TextMate; non serve alcun language server)
  - Forme speciali e costrutti di controllo (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, la famiglia `pprint`)
  - Nomi definiti (`(defun NAME ...)` come funzione, `(defstruct NAME ...)` come tipo,
    `(defvar (NAME ...))` come variabile; lo stesso con `pub`, come in `(pub defun NAME ...)`) ed
    entrambi i nomi di `(impl Trait Type)`
  - Parole chiave di namespace e di dichiarazione (`pub` `module` `use` `load` `impl` `where`) e
    marcatori della lista dei parametri lambda (`&rest` `&optional` `&key`)
  - Funzioni predefinite, tipi primitivi (inclusi `bignum` / `ratio`), tipi di errore predefiniti,
    tipi utente `Capitalized` e il tipo oggetto-trait `:dyn Trait` (anche dentro gli argomenti
    generici)
  - Letterali numerici (decimale / `0xff` / `1.5` / `3.0e10` / `1/3`), letterali carattere come
    `#\Space`, parole chiave come `:name` e globali con asterischi come `*print-pretty*`
  - **Direttive di controllo di `format` dentro le stringhe** (`~a` `~5,'0d` `~{...~}` `~^` e così via)
  - Commenti di riga `;` e commenti di blocco **annidabili** `#| ... |#`
- **Usi di tipi definiti dall'utente** (semantic token)
  - I nomi di `defstruct` / `defenum` / `deftrait` sono di solito in minuscolo (`rect` `todo-item`
    `board`), quindi la regola `Capitalized` non li cattura, e una grammatica TextMate lavora riga per
    riga e non può vedere l'intero file. I semantic token possono, e così si risolve la situazione in
    cui un linguaggio a tipizzazione statica lasciava senza colore solo le sue annotazioni di tipo
  - Quando è collegata a `typl-lsp`, l'estensione riceve **le posizioni che il checker ha davvero
    risolto come nomi di tipo**. Quindi i tipi che arrivano da altri file tramite `use` vengono
    colorati, mentre le chiamate di una **funzione** con lo stesso nome di un tipo no (il checker le
    ha risolte come funzioni, perciò lì non viene registrato alcun token)
  - Quando il server non è collegato o non è stato compilato, l'estensione ripiega su una scansione
    del testo che risolve all'interno del file. È un'approssimazione: non trova i tipi di altri file
    e non sa distinguere una funzione con lo stesso nome di un tipo
- **Indentazione Lisp** (VS Code non ha un'indentazione Lisp integrata, quindi la implementa
  l'estensione)
  - Formatta documento, Formatta selezione e formattazione durante la digitazione (Invio e `)`,
    quando `editor.formatOnType` è attivo)
- **Struttura / breadcrumb / `Ctrl+Shift+O`** (funzioni, metodi, macro, tipi, trait, `impl`,
  variabili, moduli)
- **Integrazione con `typl-lsp`** (diagnostica, hover, vai alla definizione, completamento, semantic token)
- **Comandi della CLI `typl`** (esecuzione, REPL)

Tutto tranne il language server funziona con la sola estensione, quindi anche in un checkout in cui
`typl-lsp` non è stato compilato sono disponibili l'evidenziazione, l'indentazione, la struttura e
l'evidenziazione dei tipi (limitata al file).

## Installazione

L'estensione non è sul Marketplace, quindi compilala in locale e installala.

```sh
cd editor/vscode
npm install
npm run compile
```

Poi scegli una di queste strade:

- **Provarla in un host di sviluppo**: apri `editor/vscode` in VS Code e premi `F5`
- **Installarla in modo permanente**: crea un `.vsix` con `npx @vscode/vsce package`, poi usa
  "..." → "Install from VSIX..." nella vista Estensioni

I file `.typl` si aprono automaticamente in modalità typelisp.

## Scorciatoie da tastiera

| Tasto | Comando | Cosa fa |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Salva ed esegue `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | Avvia il REPL di `typl` |

La palette dei comandi contiene anche `typelisp: Restart Language Server`.

## Impostazioni

| Impostazione | Valore predefinito | Cosa fa |
|---|---|---|
| `typelisp.program` | `typl` | Percorso della CLI `typl` |
| `typelisp.languageServer.enable` | `true` | Se collegarsi a `typl-lsp` |
| `typelisp.languageServer.path` | (vuoto) | Percorso di `typl-lsp`. Se è vuoto, l'estensione cerca `target/release/typl-lsp` dell'area di lavoro, poi `target/debug/typl-lsp`, poi `PATH` |
| `typelisp.trace.server` | `off` | Registra il traffico JSON-RPC di LSP |

Compila il language server con:

```sh
cargo build --release --bin typl-lsp
```

I riferimenti tra file tramite `use` vengono risolti cercando verso l'alto il `typelisp.toml` della
radice del progetto (per i dettagli si veda il
[Riferimento della sintassi 3.11](../../docs/it/reference/syntax.md#311-file-e-moduli-progetti-con-più-file)).

## Il problem matcher dei task

L'estensione fornisce un problem matcher chiamato `typelisp`. `typl` stampa la diagnostica nella forma
`error: FILE:LINE:COL: message`, quindi può andare direttamente nel pannello Problemi:

```jsonc
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "typl: run",
      "type": "shell",
      "command": "typl ${file}",
      "problemMatcher": "$typelisp"
    }
  ]
}
```

## Sviluppo

```sh
npm run compile   # tsc
npm run watch     # build on change
npm test          # node --test (grammar, indentation, symbols, type references, manifest)
```

I test coprono solo le parti che non richiedono il modulo `vscode`. Per questo `src/indent.ts` e
`src/symbols.ts` sono scritti come funzioni pure, e solo `src/extension.ts` tocca l'API dell'editor.

- `src/test/grammar.test.ts` — tokenizza davvero con la grammatica, usando lo stesso motore di VS
  Code (`vscode-textmate` + `vscode-oniguruma`), e controlla il risultato.
  Oniguruma differisce nei dettagli dalle espressioni regolari di Emacs (per esempio non tratta come
  letterale un `]` all'inizio di una classe di caratteri), e tali differenze si trovano solo
  eseguendo il motore reale.
- `src/test/indent.test.ts` — per ogni file `.typl` sotto `examples/`, richiede che
  **appiattire tutta l'indentazione e ripristinarla coincida byte per byte con il contenuto
  committato**. La modalità Emacs rispetta lo stesso criterio sugli stessi file, ed è questo che fa
  di "i due editor concordano" un'affermazione verificata.
  Inoltre, `src/test/fixtures/emacs-indent-reference.txt` è un output di riferimento raccolto
  eseguendo davvero `indent-region` in un buffer `typelisp-mode` di Emacs. Il lato atteso non è una
  riformulazione dell'implementazione TS ma **ciò che l'altro editor produce davvero**, quindi la
  fedeltà del port viene controllata direttamente (include `let*` `do` `doiter` `labels` `impl`
  `pprint-logical-block`, prefissi di quote e altro).
- `src/test/symbols.test.ts` — il contenuto della struttura e il rilevamento dei riferimenti ai tipi
  nel ripiego. Il numero di definizioni deve coincidere esattamente con un conteggio indipendente
  delle forme di definizione a inizio riga. Le regole di confine per i riferimenti ai tipi sono
  volutamente allineate con il ripiego della versione Emacs (VS Code usa un lookbehind; Emacs
  esprime lo stesso insieme consumando un carattere precedente).
- I token guidati dalla risoluzione del server (`crates/typelisp-front/src/check/semantic.rs`) sono
  controllati da `cargo test --test lsp_semantic_test` e da `scripts/lsp-semantic-smoke.py` (che
  pilota un vero processo su stdio). Il client Emacs è controllato da
  `scripts/emacs-semantic-smoke.el` su una vera connessione eglot.
- `src/test/manifest.test.ts` — `package.json` è l'unica parte che il compilatore non controlla,
  quindi questo test verifica che i comandi dichiarati e le chiamate a `registerCommand` siano lo
  stesso insieme, a cosa si riferiscono le scorciatoie, che le impostazioni lette dal codice siano
  dichiarate e che il problem matcher sappia interpretare ciò che `typl` stampa davvero.

### Rilevare lo scostamento nelle definizioni degli editor

Le tabelle di parole chiave sono mantenute due volte, nella versione Emacs e nella versione VS Code.
Per evitare che le definizioni degli editor restino indietro mentre l'implementazione avanza, esiste
un test sul lato Rust:

```sh
cargo test --test editor_keyword_sync_test
```

Carica davvero il prelude, percorre il registro e segnala **i nomi che uno dei due editor non
conosce**. Le forme speciali non hanno una rappresentazione a runtime, quindi vengono lette tra
`// SPECIAL-FORM DISPATCH BEGIN` / `END` in `crates/typelisp-front/src/check/checker.rs` (non
cancellare questi commenti). Se il test fallisce, aggiungi i nomi segnalati a **entrambe** le
definizioni degli editor.

Lo stesso test confronta anche la legenda dei semantic token (`SEMANTIC_TOKEN_TYPES` in
`src/bin/lsp.rs` e le tabelle che entrambi gli editor contengono devono concordare in nomi e ordine).
Una discordanza non causa alcun errore a runtime; si limita a scambiare i colori di ogni token,
quindi viene bloccata in modo meccanico.

## Note

- typelisp converte i simboli in minuscolo durante la lettura, ma l'evidenziazione distingue
  maiuscole e minuscole, per poter riconoscere i nomi di tipo che iniziano con una lettera maiuscola.
- L'indentazione è decisa da `INDENT_SPECS` in `src/indent.ts`. È un port di
  `typelisp-indent-specs` della versione Emacs, con gli stessi valori e le stesse regole. I punti in
  cui una forma ha una struttura diversa dalla forma Emacs Lisp omonima sono stati riportati così
  come sono: l'intestazione di `(defun NAME (PARAMS) RETTYPE ...)` ha tre elementi, `if` è fissato a
  tre elementi con un `else` obbligatorio, e così via.
- Il contenuto di `#| ... |#` viene re-indentato durante la formattazione. Questo coincide con il
  comportamento di `indent-region` di Emacs.
