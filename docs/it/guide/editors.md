<!-- translated-from: docs/ja/guide/editors.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Integrazione con gli editor (typl-lsp)

`typl-lsp` è il language server di typelisp. Collegato a un editor che supporta LSP (il Language
Server Protocol), fornisce queste funzioni per il file che stai modificando:

- Diagnostica: errori di lettura, errori di tipo e avvisi di ridefinizione
- Hover: il tipo di un'espressione tra parentesi e la docstring della definizione che essa chiama
  (non mostrata per i semplici nomi di variabile)
- Vai alla definizione
- Completamento (i candidati compaiono quando digiti `:`)
- Colorazione dei nomi di tipo (semantic token), compresi i tipi importati con `use` da altri file

I riferimenti tra file tramite `use` vengono risolti. Le modifiche non salvate di un altro file aperto
si riflettono subito nella diagnostica dei file che lo importano con `use`.

## 1. Compilazione

```sh
cargo build --release --bin typl-lsp
```

Questo produce `target/release/typl-lsp`. Se hai installato con `cargo install` come descritto nel
[README.md](../../../README.md), si trova in `~/.cargo/bin/typl-lsp` insieme a `typl`.

## 2. VS Code

L'estensione si trova in `editor/vscode` nel repository. Non è pubblicata sul Marketplace, quindi
devi compilarla e installarla da te.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # produce un .vsix
```

Scegli "Install from VSIX..." dal menu "..." della vista Estensioni e seleziona il `.vsix` che hai
creato.

L'estensione cerca `typl-lsp` in `target/release/typl-lsp` dell'area di lavoro, poi in
`target/debug/typl-lsp`, poi nel `PATH`. Se lo hai messo altrove, scrivi il suo percorso
nell'impostazione `typelisp.languageServer.path`.

| Impostazione | Valore predefinito | Significato |
|---|---|---|
| `typelisp.program` | `typl` | Percorso di `typl` |
| `typelisp.languageServer.enable` | `true` | Se collegarsi a `typl-lsp` |
| `typelisp.languageServer.path` | (vuoto) | Percorso di `typl-lsp` |

`Ctrl+Alt+R` salva il file che stai modificando e lo esegue con `typl`, mentre `Ctrl+Alt+Z` avvia il
REPL. Per saperne di più, vedi il [README dell'estensione per VS Code](../../../editor/vscode/README_it.md).

## 3. Emacs

`typelisp-mode` si trova in `editor/emacs` nel repository.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Impostazioni per collegarsi a `typl-lsp` con `eglot` (incluso in Emacs 29 e successivi):

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

L'eglot di Emacs 31 e successivi colora da solo i nomi di tipo (semantic token). L'eglot di Emacs 30
e precedenti non li supporta, quindi è `typelisp-mode` a colorare i nomi di tipo. Con `lsp-mode`,
imposta `lsp-semantic-tokens-enable` a `t`.

`C-c C-c` esegue il file che stai modificando e `C-c C-z` avvia il REPL. Per saperne di più, vedi il
[README di typelisp-mode](../../../editor/emacs/README_it.md).

## 4. Altri editor

`typl-lsp` parla LSP su standard input e standard output e non accetta argomenti da riga di comando.
Configura il client LSP del tuo editor in modo che avvii `typl-lsp` per i file `.typl`.

## 5. Come vengono riconosciuti i progetti

`typl-lsp` cerca `typelisp.toml` partendo dalla directory del file aperto e risalendo, e risolve
`use` usando quella posizione come radice dei sorgenti. Sono le stesse regole usate quando `typl`
esegue un file ([Moduli e organizzazione dei file](modules.md#2-configurazione-di-un-progetto)). Per un
progetto composto da più file, metti `typelisp.toml` nella sua radice.

## 6. Il language server non esegue il tuo programma

`typl-lsp` produce la diagnostica soltanto leggendo e controllando i tipi. Non esegue mai il
programma che stai modificando. La diagnostica viene calcolata a ogni battuta di tasto, quindi non può
permettersi di eseguire lì codice con effetti collaterali o codice che non termina mai. L'unica
eccezione è la registrazione dei `defmacro`, necessaria per controllare le chiamate di macro che li
seguono.

Per questo motivo, gli errori che si verificano solo quando `typl` esegue il programma (`panic`, un
file mancante e così via) non compaiono nella diagnostica del language server.
