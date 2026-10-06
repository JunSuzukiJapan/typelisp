<!-- translated-from: editor/emacs/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp-mode (Emacs)

Un mode majeur Emacs pour éditer du code source typelisp (`.typl`).
La version VS Code se trouve dans [../vscode/](../vscode/README_fr.md). Les deux partagent les
mêmes tables de mots-clés et les mêmes règles d'indentation, et
`cargo test --test editor_keyword_sync_test` le vérifie mécaniquement (voir la fin de ce document).

## Fonctionnalités

- Coloration syntaxique
  - Formes spéciales et constructions de contrôle (`defun` `let` `if` `match` `loop` `lambda`
    `setf` `as` `apply`, `print`/`println`/`format`, la famille `pprint`, etc.)
  - Noms définis (le `NAME` de `(defun NAME ...)` comme nom de fonction, celui de
    `(defstruct NAME ...)` comme nom de type et celui de `(defvar (NAME ...))` comme nom de
    variable ; de même avec `pub`, comme dans `(pub defun NAME ...)`)
  - Mots-clés d'espaces de noms et de déclarations (`pub` `module` `use` `load` `impl` `where`) et
    marqueurs de liste lambda (`&rest` `&optional` `&key`)
  - Fonctions intégrées (`car` `map` `foldl` `unwrap` `parse-int` `message` `sexpr-car`, etc.)
  - Types primitifs (y compris `bignum` / `ratio`), types intégrés, types d'erreur intégrés
    (`ParseIntError`, etc.), types utilisateur `Capitalized` et le type d'objet trait `:dyn Trait`
  - **Utilisations de types définis par l'utilisateur** (les noms de `defstruct`/`defenum`/`deftrait`
    sont généralement en minuscules (`rect` `todo-item` `board`), donc la règle `Capitalized` ne
    les attrape pas). Lorsqu'il est connecté à `typl-lsp`, le mode les colore à partir des semantic
    tokens du serveur (cela fonctionne aussi avec `eglot` ; voir plus bas). Sans connexion, il se
    rabat sur la collecte des noms de types définis dans le tampon
  - Littéraux (`true` `false`, littéraux numériques (décimal / `0xff` / `1.5` / `1/3`), littéraux
    de caractère comme `#\Space`, chaînes, mots-clés comme `:name`)
  - Directives de contrôle de `format` dans les chaînes (`~a` `~5,'0d` `~{...~}`, etc.)
  - Variables globales à « cache-oreilles » à la manière de CL (`*print-pretty*`, etc.)
- Commentaires
  - Commentaires de ligne `;`
  - Commentaires de bloc **imbriquables** `#| ... |#`
- Navigation par S-expressions et indentation à la manière de Lisp
- Un index des définitions via `imenu` (fonctions / méthodes / macros / types / traits / `impl` /
  variables / modules)
- Des commandes qui lancent la CLI `typl` (ci-dessous)

## Raccourcis clavier

| Touche | Commande | Effet |
|---|---|---|
| `C-c C-c` | `typelisp-run-buffer` | Enregistre et exécute `typl FILE` (via `compile`, on peut donc sauter aux lignes en erreur) |
| `C-c C-z` | `typelisp-repl` | Lance le REPL de `typl` dans un tampon comint |

L'emplacement de `typl` se règle avec `typelisp-program` (par défaut `"typl"`).
Les diagnostics ont la forme `error: FILE:LINE:COL: ...`, que `compilation-mode` sait analyser,
donc `next-error` / `C-x \`` saute directement à l'endroit concerné.

## Installation

```elisp
(add-to-list 'load-path "/path/to/typelisp/editor/emacs")
(require 'typelisp-mode)
```

Les fichiers `.typl` s'ouvrent automatiquement en `typelisp-mode` (le mode est enregistré dans
`auto-mode-alist`).

Avec `use-package` :

```elisp
(use-package typelisp-mode
  :load-path "/path/to/typelisp/editor/emacs"
  :mode "\\.typl\\'")
```

## Serveur de langage (`typl-lsp`)

Une fois `typl-lsp` construit, il peut être utilisé depuis `eglot` (inclus dans Emacs 29+) ou
`lsp-mode`.

```sh
cargo build --release --bin typl-lsp
```

Avec `eglot` :

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

Sont pris en charge : les diagnostics (erreurs de syntaxe et de type et avertissements de
redéfinition, transmis par `textDocument/publishDiagnostics`), le hover, l'aller à la définition
(goto-definition), la complétion (`:` est enregistré comme caractère déclencheur) et les semantic
tokens. Les références entre fichiers via `use` sont résolues (le serveur cherche en remontant le
`typelisp.toml` de la racine du projet ; pour les détails, voir la
[Référence de la syntaxe 3.11](../../docs/fr/reference/syntax.md#311-fichiers-et-modules-projets-à-plusieurs-fichiers)).
Les modifications non enregistrées des tampons ouverts se reflètent immédiatement dans les
diagnostics des fichiers dont ils dépendent comme de ceux qui dépendent d'eux.

### Mise en évidence des noms de types (semantic tokens)

Par `textDocument/semanticTokens`, le serveur signale **les positions que le vérificateur a
réellement résolues comme noms de types**. Comme ce n'est pas une comparaison de texte :

- Les types venus d'autres fichiers via `use` sont aussi colorés (une portée qu'une résolution
  interne au tampon ne peut pas atteindre par principe)
- Les appels d'une **fonction** portant le même nom qu'un type ne sont pas colorés (le vérificateur
  les a résolus comme des fonctions, donc aucun token n'y est enregistré)

Côté client :

- **`eglot` (Emacs 31 et suivants)** : eglot dessine les tokens lui-même
  (`eglot-semantic-tokens-mode`). `typelisp-mode` ne s'en mêle pas
- **`eglot` (Emacs 30 et antérieurs)** : cette version d'eglot ne traite pas semanticTokens. C'est
  pourquoi **`typelisp-mode` envoie lui-même la requête et dessine le résultat avec des overlays**
  (`typelisp-semantic-tokens-mode`, activé automatiquement quand eglot se connecte)
- **`lsp-mode`** : prise en charge native (mettre `lsp-semantic-tokens-enable` à `t`). Dans ce cas,
  `typelisp-mode` ne s'en mêle pas

`scripts/emacs-semantic-smoke.el` se connecte réellement par eglot et vérifie le côté qui dessine
dans l'Emacs utilisé. Quel que soit le client, la solution de repli interne au tampon s'efface
tant que le serveur répond (pour que deux jeux de règles ne colorent pas le même tampon).

| Réglage | Défaut | Effet |
|---|---|---|
| `typelisp-semantic-tokens` | `t` | Avec l'eglot d'Emacs 30 et antérieurs, s'il faut colorer à partir des semantic tokens du serveur |
| `typelisp-semantic-tokens-idle-delay` | `0.6` | Secondes d'inactivité après une modification avant de redemander (à garder supérieur à `eglot-send-changes-idle-time`) |

## Remarques

- typelisp met les symboles en minuscules à la lecture, mais la coloration distingue la casse afin
  de reconnaître les noms de types commençant par une majuscule.
- L'indentation est décidée par la fonction dédiée `typelisp-indent-function`, qui consulte
  `typelisp-indent-specs` (une liste d'association). Le mode garde ses propres entrées même pour
  les formes dont il partage le nom avec Emacs Lisp (`defun` `let` `if` ...) parce que les
  propriétés de symbole sont **globales**, et un réglage pour typelisp y modifierait l'indentation
  des autres tampons Lisp de la même session. En outre, les formes de typelisp diffèrent par leur
  forme même quand elles partagent le nom avec Emacs Lisp — `(defun NAME (PARAMS) RETTYPE ...)` a
  trois éléments d'en-tête, et `if` compte toujours trois éléments avec un `else` obligatoire —,
  donc les valeurs ne peuvent pas non plus être partagées.
  Il a été vérifié que `indent-region` ne modifie pas un seul octet d'aucun fichier `.typl` de
  `examples/`, et que supprimer toute l'indentation puis réindenter restaure l'original (la version
  VS Code satisfait le même critère sur les mêmes fichiers).

## Détecter les écarts dans les définitions des éditeurs

Les tables de mots-clés sont maintenues en double, ici et dans la version VS Code. Pour éviter que
les définitions des éditeurs prennent du retard pendant que l'implémentation avance, un test existe
côté Rust :

```sh
cargo test --test editor_keyword_sync_test
```

Il charge réellement le prélude, parcourt le registre et signale **les noms qu'un des éditeurs ne
connaît pas**. Les formes spéciales n'ont pas de représentation à l'exécution, elles sont donc lues
entre `// SPECIAL-FORM DISPATCH BEGIN` / `END` dans `crates/typelisp-front/src/check/checker.rs`
(ne supprimez pas ces commentaires). En cas d'échec, ajoutez les noms signalés aux **deux**
définitions d'éditeur.

Le même test compare aussi la légende des semantic tokens (`SEMANTIC_TOKEN_TYPES` dans
`src/bin/lsp.rs` et les tables des deux éditeurs doivent concorder en noms et en ordre). Un écart ne
provoque aucune erreur à l'exécution ; il intervertit seulement les couleurs de tous les tokens,
c'est pourquoi il est verrouillé mécaniquement.
