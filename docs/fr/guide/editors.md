<!-- translated-from: docs/ja/guide/editors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Intégration aux éditeurs (typl-lsp)

`typl-lsp` est le serveur de langage de typelisp. Connecté à un éditeur qui prend en charge LSP (le Language Server
Protocol), il fournit ces fonctionnalités pour le fichier en cours d'édition :

- Diagnostics : erreurs de lecture, erreurs de type et avertissements de redéfinition
- Survol : le type d'une expression entre parenthèses et la docstring de la définition qu'elle appelle (non
  affiché pour les simples noms de variables)
- Aller à la définition
- Complétion (les candidats apparaissent quand on tape `:`)
- Coloration des noms de types (jetons sémantiques), y compris des types importés d'autres fichiers avec `use`

Les références entre fichiers via `use` sont résolues. Les modifications non enregistrées d'un autre fichier ouvert
se reflètent immédiatement dans les diagnostics des fichiers qui l'importent avec `use`.

## 1. Construire

```sh
cargo build --release --bin typl-lsp
```

Cela produit `target/release/typl-lsp`. Si vous avez installé avec `cargo install` comme décrit dans le
[README.md](../../../README.md), il se trouve dans `~/.cargo/bin/typl-lsp` à côté de `typl`.

## 2. VS Code

L'extension se trouve dans `editor/vscode` du dépôt. Elle n'est pas publiée sur le Marketplace ; construisez-la et
installez-la vous-même.

```sh
cd editor/vscode
npm install
npm run compile
npx @vscode/vsce package      # produit un .vsix
```

Dans le menu « ... » de la vue Extensions, choisissez « Install from VSIX... » puis sélectionnez le `.vsix`
construit.

L'extension cherche `typl-lsp` dans `target/release/typl-lsp` de l'espace de travail, puis dans
`target/debug/typl-lsp`, puis dans le `PATH`. S'il se trouve ailleurs, indiquez son chemin dans le réglage
`typelisp.languageServer.path`.

| Réglage | Par défaut | Signification |
|---|---|---|
| `typelisp.program` | `typl` | Chemin de `typl` |
| `typelisp.languageServer.enable` | `true` | Se connecter ou non à `typl-lsp` |
| `typelisp.languageServer.path` | (vide) | Chemin de `typl-lsp` |

`Ctrl+Alt+R` enregistre le fichier en cours d'édition et l'exécute avec `typl`, et `Ctrl+Alt+Z` lance la REPL.
Pour en savoir plus, voir le [README de l'extension VS Code](../../../editor/vscode/README.md) (en japonais).

## 3. Emacs

`typelisp-mode` se trouve dans `editor/emacs` du dépôt.

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Réglages pour se connecter à `typl-lsp` avec `eglot` (fourni avec Emacs 29 et ultérieur) :

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(typelisp-mode . ("/path/to/typelisp/target/release/typl-lsp"))))
(add-hook 'typelisp-mode-hook #'eglot-ensure)
```

Avec `lsp-mode` :

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/typelisp/target/release/typl-lsp")
    :major-modes '(typelisp-mode)
    :server-id 'typl-lsp)))
(add-hook 'typelisp-mode-hook #'lsp)
```

eglot ne prend pas en charge les jetons sémantiques ; avec eglot, `typelisp-mode` colore donc lui-même les noms de
types. Avec `lsp-mode`, mettez `lsp-semantic-tokens-enable` à `t`.

`C-c C-c` exécute le fichier en cours d'édition, et `C-c C-z` lance la REPL. Pour en savoir plus, voir le
[README de typelisp-mode](../../../editor/emacs/README.md) (en japonais).

## 4. Autres éditeurs

`typl-lsp` parle LSP sur l'entrée et la sortie standard et ne prend pas d'arguments en ligne de commande.
Configurez le client LSP de votre éditeur pour lancer `typl-lsp` pour les fichiers `.typl`.

## 5. Comment les projets sont reconnus

`typl-lsp` cherche `typelisp.toml` en partant du répertoire du fichier ouvert et en remontant, et résout `use` en
prenant cet endroit comme racine des sources. Ce sont les mêmes règles que lorsque `typl` exécute un fichier
([Modules et découpage en fichiers](modules.md#2-mettre-en-place-un-projet)). Pour un projet composé de plusieurs
fichiers, placez `typelisp.toml` à sa racine.

## 6. Le serveur de langage n'exécute pas votre programme

`typl-lsp` produit les diagnostics uniquement en lisant et en vérifiant les types. Il n'exécute jamais le programme
en cours d'édition. Les diagnostics s'exécutent à chaque frappe ; il ne peut donc pas se permettre d'y exécuter du
code à effets de bord ni du code qui ne se termine jamais. La seule exception est l'enregistrement des `defmacro`,
nécessaire pour vérifier les appels de macro qui les suivent.

C'est pourquoi les erreurs qui ne se produisent que lorsque `typl` exécute le programme (`panic`, un fichier
manquant, etc.) n'apparaissent pas dans les diagnostics du serveur de langage.
