<!-- translated-from: editor/vscode/README_JP.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# typelisp (VS Code)

Une extension VS Code pour éditer du code source typelisp (`.typl`).
La version Emacs se trouve dans [../emacs/](../emacs/README_fr.md). Les deux partagent les mêmes
tables de mots-clés et les mêmes règles d'indentation, et
`cargo test --test editor_keyword_sync_test` le vérifie mécaniquement (voir plus bas).

## Fonctionnalités

- **Coloration syntaxique** (une grammaire TextMate ; pas besoin de serveur de langage)
  - Formes spéciales et constructions de contrôle (`defun` `let` `if` `match` `loop` `lambda`
    `setf` `as` `apply`, `print`/`println`/`format`, la famille `pprint`)
  - Noms définis (`(defun NAME ...)` comme fonction, `(defstruct NAME ...)` comme type,
    `(defvar (NAME ...))` comme variable ; de même avec `pub`, comme dans `(pub defun NAME ...)`)
    et les deux noms de `(impl Trait Type)`
  - Mots-clés d'espaces de noms et de déclarations (`pub` `module` `use` `load` `impl` `where`) et
    marqueurs de liste lambda (`&rest` `&optional` `&key`)
  - Fonctions intégrées, types primitifs (y compris `bignum` / `ratio`), types d'erreur intégrés,
    types utilisateur `Capitalized` et le type d'objet trait `:dyn Trait` (y compris à l'intérieur
    d'arguments génériques)
  - Littéraux numériques (décimal / `0xff` / `1.5` / `3.0e10` / `1/3`), littéraux de caractère
    comme `#\Space`, mots-clés comme `:name` et variables globales à « cache-oreilles » comme
    `*print-pretty*`
  - **Directives de contrôle de `format` dans les chaînes** (`~a` `~5,'0d` `~{...~}` `~^`, etc.)
  - Commentaires de ligne `;` et commentaires de bloc **imbriquables** `#| ... |#`
- **Utilisations de types définis par l'utilisateur** (semantic tokens)
  - Les noms de `defstruct` / `defenum` / `deftrait` sont généralement en minuscules (`rect`
    `todo-item` `board`), donc la règle `Capitalized` ne les attrape pas, et une grammaire TextMate
    travaille ligne par ligne sans voir le fichier entier. Les semantic tokens le voient, ce qui
    corrige la situation où, dans un langage à typage statique, seules les annotations de type
    restaient sans couleur
  - Lorsqu'elle est connectée à `typl-lsp`, l'extension reçoit **les positions que le vérificateur
    a réellement résolues comme noms de types**. Les types venus d'autres fichiers via `use` sont
    donc colorés, et les appels d'une **fonction** portant le même nom qu'un type ne le sont pas
    (le vérificateur les a résolus comme des fonctions, donc aucun token n'y est enregistré)
  - Si le serveur n'est pas connecté ou pas construit, l'extension se rabat sur un balayage de texte
    qui résout à l'intérieur du fichier. C'est une approximation : il ne trouve pas les types venus
    d'autres fichiers et ne distingue pas une fonction portant le même nom qu'un type
- **Indentation Lisp** (VS Code n'a pas d'indentation Lisp intégrée, l'extension l'implémente donc)
  - Mettre en forme le document, mettre en forme la sélection et mise en forme à la frappe (Entrée
    et `)`, lorsque `editor.formatOnType` est activé)
- **Outline / breadcrumbs / `Ctrl+Shift+O`** (fonctions, méthodes, macros, types, traits, `impl`,
  variables, modules)
- **Intégration de `typl-lsp`** (diagnostics, hover, aller à la définition, complétion, semantic
  tokens)
- **Commandes de la CLI `typl`** (exécution, REPL)

Tout sauf le serveur de langage fonctionne avec la seule extension, donc même dans une copie où
`typl-lsp` n'a pas été construit, la coloration, l'indentation, l'Outline et la mise en évidence des
types (limitée au fichier) sont disponibles.

## Installation

L'extension n'est pas sur le Marketplace ; il faut donc la construire et l'installer localement.

```sh
cd editor/vscode
npm install
npm run compile
```

Puis l'une de ces deux options :

- **L'essayer dans un hôte de développement** : ouvrir `editor/vscode` dans VS Code et appuyer
  sur `F5`
- **L'installer durablement** : produire un `.vsix` avec `npx @vscode/vsce package`, puis utiliser
  « ... » → « Install from VSIX... » dans la vue Extensions

Les fichiers `.typl` s'ouvrent automatiquement en mode typelisp.

## Raccourcis clavier

| Touche | Commande | Effet |
|---|---|---|
| `Ctrl+Alt+R` | `typelisp.runFile` | Enregistre et exécute `typl FILE` |
| `Ctrl+Alt+Z` | `typelisp.repl` | Lance le REPL de `typl` |

La palette de commandes contient aussi `typelisp: Restart Language Server`.

## Réglages

| Réglage | Défaut | Effet |
|---|---|---|
| `typelisp.program` | `typl` | Chemin de la CLI `typl` |
| `typelisp.languageServer.enable` | `true` | S'il faut se connecter à `typl-lsp` |
| `typelisp.languageServer.path` | (vide) | Chemin de `typl-lsp`. S'il est vide, la recherche se fait dans cet ordre : `target/release/typl-lsp` de l'espace de travail, `target/debug/typl-lsp`, puis `PATH` |
| `typelisp.trace.server` | `off` | Journalise le trafic JSON-RPC de LSP |

Le serveur de langage se construit avec :

```sh
cargo build --release --bin typl-lsp
```

Les références entre fichiers via `use` sont résolues en cherchant en remontant le `typelisp.toml`
de la racine du projet (pour les détails, voir la
[Référence de la syntaxe 3.11](../../docs/fr/reference/syntax.md#311-fichiers-et-modules-projets-à-plusieurs-fichiers)).

## Le problem matcher des tâches

L'extension fournit un problem matcher nommé `typelisp`. `typl` affiche ses diagnostics sous la
forme `error: FILE:LINE:COL: message`, qui peuvent donc aller directement dans le panneau Problèmes :

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

## Développement

```sh
npm run compile   # tsc
npm run watch     # reconstruction à chaque modification
npm test          # node --test (grammaire, indentation, symboles, références de type, manifeste)
```

Les tests ne couvrent que les parties qui n'ont pas besoin du module `vscode`. Pour cela,
`src/indent.ts` et `src/symbols.ts` sont écrits comme des fonctions pures, et seul
`src/extension.ts` touche l'API de l'éditeur.

- `src/test/grammar.test.ts` — segmente réellement le texte avec la grammaire, avec le même moteur
  que VS Code (`vscode-textmate` + `vscode-oniguruma`), et vérifie le résultat.
  Oniguruma diffère des expressions régulières d'Emacs dans le détail (par exemple, il ne traite pas
  un `]` en tête de classe de caractères comme un littéral), et ces différences n'apparaissent
  qu'en faisant tourner le vrai moteur.
- `src/test/indent.test.ts` — exige, pour chaque fichier `.typl` de `examples/`, que **supprimer
  toute l'indentation puis la restaurer corresponde octet pour octet au contenu commité**.
  Le mode Emacs satisfait le même critère sur les mêmes fichiers, et c'est ce qui fait de « les deux
  éditeurs concordent » une affirmation vérifiée.
  En outre, `src/test/fixtures/emacs-indent-reference.txt` est une sortie de référence recueillie en
  exécutant réellement `indent-region` dans un tampon `typelisp-mode` d'Emacs. La valeur attendue
  n'est pas une reprise de l'implémentation TS mais **ce que l'autre éditeur produit réellement**,
  donc la fidélité du portage est vérifiée directement (y compris `let*` `do` `doiter` `labels`
  `impl` `pprint-logical-block`, les préfixes de quote, etc.).
- `src/test/symbols.test.ts` — le contenu de l'Outline et la détection des références de type par
  la solution de repli. Le nombre de définitions doit correspondre exactement à un comptage
  indépendant des formes de définition en début de ligne. Les règles de frontière des références
  de type sont volontairement alignées sur la solution de repli de la version Emacs (VS Code
  utilise un lookbehind ; Emacs exprime le même ensemble en consommant un caractère précédent).
- Les tokens du serveur guidés par la résolution (`crates/typelisp-front/src/check/semantic.rs`)
  sont vérifiés par `cargo test --test lsp_semantic_test` et `scripts/lsp-semantic-smoke.py` (qui
  pilote un vrai processus par stdio). Le client Emacs est vérifié par
  `scripts/emacs-semantic-smoke.el` sur une vraie connexion eglot.
- `src/test/manifest.test.ts` — `package.json` est la seule partie que le compilateur ne vérifie
  pas ; ce test contrôle donc que les commandes déclarées et les appels à `registerCommand` forment
  le même ensemble, ce à quoi renvoient les raccourcis clavier, que les réglages lus par le code
  sont déclarés et que le problem matcher sait analyser ce que `typl` affiche réellement.

### Détecter les écarts dans les définitions des éditeurs

Les tables de mots-clés sont maintenues en double, dans la version Emacs et dans la version VS Code.
Pour éviter que les définitions des éditeurs prennent du retard pendant que l'implémentation
avance, un test existe côté Rust :

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

## Remarques

- typelisp met les symboles en minuscules à la lecture, mais la coloration distingue la casse afin
  de reconnaître les noms de types commençant par une majuscule.
- L'indentation est décidée par `INDENT_SPECS` dans `src/indent.ts`. C'est un portage de
  `typelisp-indent-specs` de la version Emacs, avec les mêmes valeurs et les mêmes règles. Les
  endroits où une forme diffère par sa forme de la forme homonyme d'Emacs Lisp sont repris tels
  quels : l'en-tête de `(defun NAME (PARAMS) RETTYPE ...)` a trois éléments, `if` compte toujours
  trois éléments avec un `else` obligatoire, etc.
- Le contenu de `#| ... |#` est réindenté lors de la mise en forme. Cela correspond au comportement
  de `indent-region` d'Emacs.
