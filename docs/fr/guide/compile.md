<!-- translated-from: docs/ja/guide/compile.md @ e5e6bdf72dbe4cf76a395c536f23b887cdae8fea -->
# Compilation

Sauf indication contraire, les programmes typelisp s'exécutent dans l'interpréteur. Il existe en outre deux façons
de compiler en code natif et une façon de sauvegarder un environnement. Les détails de la spécification se
trouvent au [chapitre 10 de la référence de la syntaxe](../reference/syntax.md#10-compilation).

| Méthode | Comment | Résultat |
|---|---|---|
| Compilation JIT | `(compile name)` | Une fonction de la session en cours est remplacée par du code natif |
| Compilation AOT | `typl -c src.typl` ou `(compile-file "src.typl" "out")` | Un exécutable autonome |
| Dump | `(dump "file.typld")` | Sauvegarde les définitions ; `typl --image` repart du même environnement |

## 1. Préparation

La compilation utilise LLVM 22. Si vous avez construit `typl` en suivant le [README.md](../../../README.md), aucune
autre préparation n'est nécessaire.

Les exécutables produits par compilation AOT sont liés à la bibliothèque statique `libtypelisp_front.a`. Une
version release de `typl` (y compris celle installée avec `cargo install`) embarque cette bibliothèque ; aucune
préparation n'est donc nécessaire. À la première compilation, elle écrit la bibliothèque dans
`~/.typelisp/lib/<identifiant de build>/` (ou dans `$TYPELISP_HOME/lib/<identifiant de build>/` si la variable
d'environnement `TYPELISP_HOME` est définie) et utilise cette copie ensuite. `typl --remove-lib` la supprime (avec
`--others`, celles écrites par d'autres versions de `typl` ; avec `--all`, toutes). Une version debug de `typl`
utilise la bibliothèque dans `target/debug/` du dépôt où elle a été construite. Pour en utiliser une placée
ailleurs, donnez son dossier avec `--lib-dir` au lancement de `typl` (section 3.2).
Sous macOS, l'édition de liens utilise les Xcode Command Line Tools.

## 2. Compilation JIT

Elle transforme sur place une fonction déjà définie en code natif.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; désormais, les appels exécutent le code compilé
```

- `name` n'est pas évalué. Écrivez le nom de la fonction tel quel (pas sous forme de chaîne). Pour une méthode,
  écrivez-la avec le nom du type, comme `(compile point::norm)`.
- Les fonctions qu'elle appelle sont compilées avec elle.
- **Les fonctions génériques ne peuvent pas être compilées.** Une copie par type est créée à chaque endroit où
  elles sont utilisées. Compilez plutôt la fonction qui l'appelle avec des types concrets.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` et `dump` sont des opérations de l'interpréteur ; une
  fonction qui les appelle ne peut donc pas être compilée. Tenter de la compiler donne une erreur qui en indique la
  raison.

Pour examiner le résultat de la compilation, utilisez `disassemble`.

```lisp
(disassemble fib)          ; le code machine de l'hôte
(disassemble fib true)     ; LLVM IR
```

## 3. Créer un exécutable par compilation AOT

### 3.1 Écrire le programme

Comme point d'entrée, définissez une **fonction `main` sans argument**.

```lisp
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

Le `(main)` à la fin du fichier sert à appeler `main` quand on lance `typl hello.typl`. `compile-file` ignore ce
`(main)` final ; le même fichier fonctionne donc à la fois dans l'interpréteur et en compilation AOT.

### 3.2 Compiler

En ligne de commande, utilisez `typl -c` (`typl --compile` est équivalent).

```sh
$ typl -c hello.typl            # produit hello
$ typl -c hello.typl -o fib     # nomme l'exécutable fib
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Sans `-o`, l'exécutable porte le nom du fichier source sans `.typl` et est placé dans le même dossier que le
fichier source. Si le nom du fichier source ne se termine pas par `.typl`, `-o` est obligatoire. Avec `-c`
(`--compile`), on ne peut pas donner `--image`, `--heap-cells` ni `--feature`.

On peut faire la même chose en appelant `compile-file` depuis la REPL ou depuis un programme.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Si vous construisez souvent, vous pouvez mettre cette seule ligne dans un fichier et l'exécuter avec
`typl build.typl`.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Les noms de fichiers sont résolus à partir du **répertoire courant où `typl` a été lancé**, pas de l'emplacement de
`build.typl`.

Pour lier un `libtypelisp_front.a` placé ailleurs que là où `typl` le cherche, donnez son dossier avec `--lib-dir`.
Cela s'applique à la fois à `typl -c` et à `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Si le dossier donné ne contient pas de `libtypelisp_front.a`, `typl` s'arrête sur une erreur. Le fichier ne
fonctionne qu'avec le `typl` construit en même temps que lui. Après avoir reconstruit `typl`, copiez-le de
nouveau.

### 3.3 Ce que peut contenir un fichier compilé en AOT

- Le niveau supérieur du fichier d'entrée ne peut contenir que des définitions (`defun` `defmethod` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) ainsi que `use` et `module`. Les expressions de niveau
  supérieur comme `(println ...)` ne sont pas permises, à l'exception du `(main)` final. Mettez le travail
  dans `main`.
- Sans `main` sans argument, la compilation échoue avec une erreur.
- Les fichiers des modules importés avec `use` sont aussi compilés et réunis dans un seul exécutable.
- Les bibliothèques nommées avec `:library` dans `defffi` sont liées automatiquement ([FFI C](ffi.md)).
- Toutes les fonctions de la bibliothèque standard peuvent s'utiliser en compilation AOT. `eval` aussi, mais alors
  le vérificateur de types et l'interpréteur entrent dans l'exécutable, qui devient plus gros et plus lent à
  démarrer. Les programmes qui n'appellent pas `eval` ne les incluent pas.

### 3.4 Comportement de l'exécutable

- `(command-line-args)` renvoie un `Vector<string>` de même forme, qu'on lance `typl hello.typl a b` ou
  `./hello a b`. Le premier élément est le nom du programme.
- Fixez le code de sortie avec `(exit n)`. Si `main` revient normalement, il vaut 0.
- En cas de `panic`, le programme affiche le message et se termine avec un code non nul.

## 4. Dumps

On peut sauvegarder les définitions de la session courante dans un fichier et en repartir la fois suivante.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

Cela fonctionne aussi pour exécuter un fichier, comme `typl --image session.typld prog.typl`.

- Ce qui est sauvegardé, ce sont les **définitions**. Les expressions évaluées dans la session ne sont pas
  sauvegardées.
- Les fonctions compilées avec `compile` sont sauvegardées sous leur forme compilée.
- Les variables globales sont restaurées **en réexécutant leurs initialiseurs**, et non avec les valeurs qu'elles
  avaient au moment de l'écriture du dump.
- Un dump ne peut pas être chargé par un `typl` d'une autre version que celle qui l'a écrit (c'est une erreur).

Si vous exécutez un fichier et faites `(dump ...)` depuis celui-ci, les définitions de ce fichier se trouvent dans
un module qui porte le nom du fichier. Une fonction définie dans `dp.typl` s'appelle `dp::sq`, et l'appeler depuis
un autre fichier exige `pub` ([Modules et découpage en fichiers](modules.md)).

## 5. À propos des fichiers de modules compilés

Il n'existe pas de format, comme le `.fasl` de Common Lisp, pour écrire dans un fichier le résultat compilé de
chaque module. `compile-file` construit l'exécutable directement à partir des sources. Aucun fichier intermédiaire
n'est laissé.
