<!-- translated-from: editor/emacs/README_JP.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# typelisp-mode (Emacs)

Una modalità maggiore di Emacs per modificare sorgenti typelisp (`.typl`).
La versione per VS Code si trova in [../vscode/](../vscode/README_it.md). Le due condividono le stesse
tabelle di parole chiave e le stesse regole di indentazione, e `cargo test --test editor_keyword_sync_test`
lo verifica in modo meccanico (vedi la fine di questo documento).

## Funzionalità

- Evidenziazione della sintassi
  - Forme speciali e costrutti di controllo (`defun` `let` `if` `match` `loop` `lambda` `setf` `as`
    `apply`, `print`/`println`/`format`, la famiglia `pprint` e così via)
  - Nomi definiti (il `NAME` di `(defun NAME ...)` come nome di funzione, di `(defstruct NAME ...)`
    come nome di tipo e di `(defvar (NAME ...))` come nome di variabile; lo stesso con `pub`, come
    in `(pub defun NAME ...)`)
  - Parole chiave di namespace e di dichiarazione (`pub` `module` `use` `load` `impl` `where`) e
    marcatori della lista dei parametri lambda (`&rest` `&optional` `&key`)
  - Funzioni predefinite (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car` e così via)
  - Tipi primitivi (inclusi `bignum` / `ratio`), tipi predefiniti, tipi di errore predefiniti
    (`ParseIntError` e così via), tipi utente `Capitalized` e il tipo oggetto-trait `:dyn Trait`
  - **Usi di tipi definiti dall'utente** (i nomi di `defstruct`/`defenum`/`deftrait` sono di solito
    in minuscolo (`rect` `todo-item` `board`), quindi la regola `Capitalized` non li cattura).
    Quando è collegata a `typl-lsp`, la modalità li colora a partire dai semantic token del server
    (funziona anche con `eglot`; vedi sotto). Quando non è collegata, ripiega sulla raccolta dei
    nomi di tipo definiti nel buffer
  - Letterali (`true` `false`, letterali numerici (decimale / `0xff` / `1.5` / `1/3`), letterali
    carattere come `#\Space`, stringhe, parole chiave come `:name`)
  - Direttive di controllo di `format` dentro le stringhe (`~a` `~5,'0d` `~{...~}` e così via)
  - Globali in stile CL con asterischi (`*print-pretty*` e così via)
- Commenti
  - Commenti di riga `;`
  - Commenti di blocco **annidabili** `#| ... |#`
- Navigazione delle S-expression e indentazione in stile Lisp
- Un indice delle definizioni tramite `imenu` (funzioni / metodi / macro / tipi / trait / `impl` /
  variabili / moduli)
- Comandi che eseguono la CLI `typl` (vedi sotto)

## Scorciatoie da tastiera

| Tasto | Comando | Cosa fa |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Salva ed esegue `typl FILE` (tramite `compile`, così puoi saltare alle righe di errore) |
| `C-c C-z` | `typelisp-repl` | Avvia il REPL di `typl` in un buffer comint |

Imposta la posizione di `typl` con `typelisp-program` (predefinito `"typl"`).
La diagnostica ha la forma `error: FILE:LINE:COL: ...`, che `compilation-mode` sa interpretare, quindi
`next-error` / `C-x \`` saltano direttamente al punto.

## Installazione

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

I file `.typl` si aprono automaticamente in `typelisp-mode` (la modalità è registrata in
`auto-mode-alist`).

Con `use-package`:

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Language Server (`typl-lsp`)

Una volta compilato `typl-lsp`, può essere usato da `eglot` (integrato in Emacs 29+) o da `lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

Con `eglot`:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Con `lsp-mode`:

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

Supportati: diagnostica (errori di sintassi e di tipo e avvisi di ridefinizione, inviati tramite
`textDocument/publishDiagnostics`), hover, vai alla definizione, completamento (`:` è registrato come
carattere di attivazione) e semantic token. I riferimenti tra file tramite `use` vengono risolti (il
server cerca verso l'alto il `typelisp.toml` della radice del progetto; per i dettagli si veda il
[Riferimento della sintassi 3.11](../../docs/it/reference/syntax.md#311-file-e-moduli-progetti-con-più-file)).
Le modifiche non salvate nei buffer aperti dell'editor si riflettono subito nella diagnostica sia dei
file da cui dipendono sia dei file che dipendono da loro.

### Evidenziazione dei nomi di tipo (semantic token)

Tramite `textDocument/semanticTokens`, il server riporta **le posizioni che il checker ha davvero
risolto come nomi di tipo**. Poiché non si tratta di una corrispondenza testuale:

- Vengono colorati anche i tipi che arrivano da altri file tramite `use` (un intervallo che la
  risoluzione all'interno del buffer non può raggiungere per principio)
- Le chiamate di una **funzione** con lo stesso nome di un tipo non vengono colorate (il checker le ha
  risolte come funzioni, perciò lì non viene registrato alcun token)

Sul lato client:

- **`eglot` (Emacs 31 e successivi)**: eglot disegna da sé i token (`eglot-semantic-tokens-mode`).
  `typelisp-mode` si fa da parte
- **`eglot` (Emacs 30 e precedenti)**: questa versione di eglot non gestisce semanticTokens. Quindi
  **`typelisp-mode` invia da sé la richiesta e disegna il risultato con gli overlay**
  (`typelisp-semantic-tokens-mode`, attivata automaticamente quando eglot si collega)
- **`lsp-mode`**: supporto nativo (imposta `lsp-semantic-tokens-enable` a `t`). In tal caso
  `typelisp-mode` si fa da parte

`scripts/emacs-semantic-smoke.el` si collega davvero tramite eglot e controlla il lato che disegna
nell'Emacs in uso. Con qualsiasi client, il ripiego interno al buffer si ritira mentre il server sta
rispondendo (in modo che due insiemi di regole non dipingano lo stesso buffer).

| Impostazione | Valore predefinito | Cosa fa |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Con l'eglot di Emacs 30 e precedenti, se colorare a partire dai semantic token del server |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Secondi di inattività dopo una modifica prima di richiedere di nuovo (tienilo maggiore di `eglot-send-changes-idle-time`) |

## Note

- typelisp converte i simboli in minuscolo durante la lettura, ma l'evidenziazione distingue
  maiuscole e minuscole, per poter riconoscere i nomi di tipo che iniziano con una lettera maiuscola.
- L'indentazione è decisa dalla dedicata `typelisp-indent-function`, che consulta
  `typelisp-indent-specs` (una alist). La modalità mantiene voci proprie anche per le forme il cui
  nome coincide con quello di Emacs Lisp (`defun` `let` `if` ...) perché le proprietà dei simboli
  sono **globali**, e impostarvi valori di typelisp cambierebbe l'indentazione degli altri buffer
  Lisp nella stessa sessione. Inoltre le forme di typelisp hanno una struttura diversa anche quando
  condividono il nome con Emacs Lisp: `(defun NAME (PARAMS) RETTYPE ...)` ha tre elementi
  nell'intestazione, e `if` è fissato a tre elementi con un `else` obbligatorio. Quindi nemmeno i
  valori possono essere condivisi.
  Sono stati controllati tutti i file `.typl` sotto `examples/`: `indent-region` non cambia nemmeno un
  byte, e appiattire tutta l'indentazione e re-indentare ripristina l'originale (la versione VS Code
  rispetta lo stesso criterio sugli stessi file).

## Rilevare lo scostamento nelle definizioni degli editor

Le tabelle di parole chiave sono mantenute due volte, una qui e una nella versione VS Code. Per
evitare che le definizioni degli editor restino indietro mentre l'implementazione avanza, esiste un
test sul lato Rust:

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
