<!-- translated-from: docs/ja/README.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Documentation de typelisp (français)

typelisp est un Lisp à typage statique. Pour l'installer et le compiler, voir le
[README.md](../../README.md) (en anglais) à la racine du dépôt.

## Tutoriel

Si vous découvrez typelisp, lisez ces documents dans l'ordre.

- [Premiers pas](tutorial/intro.md) : la REPL, les fonctions, les variables, les conditionnelles, les boucles, les listes et `Vector`
- [Bases des types](tutorial/types.md) : types statiques, `Option`, `Result`, structures, énumérations, génériques
- [Traits](tutorial/traits.md) : `deftrait` / `impl`, bornes de traits, `:dyn`
- [Macros](tutorial/macros.md) : `defmacro`, quasiquote, `gensym`, `macrolet`
- [Gestion des erreurs](tutorial/errors.md) : `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Concurrence](tutorial/concurrency.md) : tâches, canaux, `select`, `Mutex`, `thread`

## Guides

- [Modules et découpage en fichiers](guide/modules.md) : `use`, `pub`, correspondance entre fichiers et modules
- [Compilation](guide/compile.md) : le JIT, la création d'exécutables par compilation AOT, les dumps
- [E/S de fichiers, flux et réseau](guide/io.md) : fichiers, noms de chemin, TCP / TLS / UDP, résolution de noms
- [FFI C](guide/ffi.md) : appeler des fonctions C avec `defffi` (y compris les callbacks et les structures C avec `def-c-struct`)
- [Intégration aux éditeurs](guide/editors.md) : `typl-lsp` et la configuration de VS Code / Emacs
- [Pour les programmeurs Common Lisp](guide/from-common-lisp.md) : en quoi typelisp diffère de CL et comment réécrire du code CL

## Référence

- [Référence de la syntaxe](reference/syntax.md) : lexique, écriture des types, définitions, formes de contrôle, compilation, concurrence
- [Fonctions intégrées](reference/functions/README.md) : fonctions intégrées, méthodes et bibliothèque standard
- [Types](reference/types.md) : les types et les traits que chacun implémente
- [Messages d'erreur](reference/errors.md) : ce que signifient les erreurs courantes et comment les corriger

## Intégration aux éditeurs

Les étapes de configuration se trouvent dans le [guide d'intégration aux éditeurs](guide/editors.md). Les
raccourcis clavier et les réglages de chaque éditeur sont décrits dans ces documents :

- [Emacs (typelisp-mode)](../../editor/emacs/README_fr.md)
- [VS Code](../../editor/vscode/README_fr.md)
