<!-- translated-from: docs/ja/guide/compile.md @ 0d6d73aceba107bf7480412efc65502ef674dda8 -->
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
(use std::env::command-line-args)
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
args: #(./hello a b)
fib(25) = 75025
```

Sans `-o`, l'exécutable porte le nom du fichier source sans `.typl` et est placé dans le même dossier que le
fichier source. Si le nom du fichier source ne se termine pas par `.typl`, `-o` est obligatoire. Avec `-c`
(`--compile`), on ne peut pas donner `--image` ni `--feature`.

Avec `--release`, `typl` produit une **compilation release**, qui retire `eprint`/`eprintln` (sortie d'erreur standard pour l'investigation ; [Affichage](../reference/functions/printing.md)) avec leurs arguments. Ce qui est retiré est tout de même vérifié par les types : une erreur de type à cet endroit fait donc échouer la compilation. `--release` est une option de `typl` dans son ensemble : elle agit de la même façon non seulement pour `-c`, mais aussi lors de l'exécution d'un fichier, dans le REPL et pour `typl test`. Un `compile-file` appelé dans un `typl` lancé avec `--release` produit lui aussi une compilation release, et `eprint`/`eprintln` sont aussi retirés du code évalué par `eval` dans l'exécutable ainsi produit.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # pareil écrit après -c
```

`-O0`, `-O1`, `-O2` et `-O3` fixent le **niveau d'optimisation**. Le niveau par défaut est `-O0`, qui n'optimise pas. À partir de `-O1`, le compilateur fait d'abord des optimisations simples, comme remplacer une variable liée seulement à un littéral par sa valeur ou calculer à l'avance l'arithmétique entière sur des constantes, puis laisse le reste à l'optimisation de LLVM au niveau du même nom. À tous les niveaux, les résultats du calcul sont les mêmes. Comme `--release`, c'est une option de `typl` dans son ensemble : outre l'exécutable que produit `-c`, elle s'applique au code compilé pendant l'exécution d'un fichier, dans le REPL et dans `typl test`, ainsi qu'à un `compile-file` appelé à cet endroit. Le code que l'interpréteur exécute n'est pas optimisé.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # pareil écrit après -c
```

On peut faire la même chose en appelant `compile-file` depuis la REPL ou depuis un programme.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
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

- Le niveau supérieur du fichier d'entrée ne peut contenir que des définitions (`defun` `defvar`
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

### 3.5 Ressources utilisées par l'exécutable

`typl -c` a des options qui indiquent les ressources que l'exécutable peut utiliser pendant son exécution.

```sh
$ typl --heap-cells 100000 -c hello.typl    # fixe l'arène cons à 100000 cellules
$ typl -c hello.typl --max-memory 64M       # alloue au plus 64 Mio
$ typl -c hello.typl --no-cons              # n'utilise aucune cellule cons
```

- `--heap-cells N` fixe la capacité de l'arène des cellules cons à `N` cellules. L'arène ne grandit pas au-delà : si le ramasse-miettes ne peut libérer aucune cellule, le programme s'arrête sur une erreur d'exécution. Sans l'option, l'arène commence à 65536 cellules et grandit selon les besoins.
- `--max-memory SIZE` borne la mémoire totale que l'exécutable alloue. Tout compte : l'arène cons, les chaînes, les valeurs comme les vecteurs et les tables de hachage, et la mémoire du runtime lui-même. `SIZE` est un nombre d'octets, éventuellement suivi de `K`, `M` ou `G` (1024, 1024² et 1024³ fois). Une allocation qui dépasserait la borne est signalée, et le programme se termine avec le code de sortie 1. Cette sortie ne peut pas être interceptée par `catch`. Sans l'option, il n'y a pas de borne.
- `--no-cons` produit un exécutable qui n'utilise aucune cellule cons. Il n'a pas d'arène cons : un programme qui fait quelque chose nécessitant des cellules cons est donc une **erreur de compilation**, qui montre cette opération et la chaîne de fonctions depuis `main` qui y mène. Les cellules cons sont nécessaires pour construire une liste `Sexpr` (y compris une liste quotée ou construite par quasi-quote) ou un argument `&rest`, ainsi que pour `read`, `eval` et `macroexpand`. Les opérations sur les nombres, chaînes, vecteurs, tables de hachage, structures, canaux, etc., et `print`/`println`/`format` n'en utilisent aucune. Ne peut pas être donné avec `--heap-cells`, et s'écrit après `-c`.

`--max-memory` s'écrit après `-c`. `--heap-cells` peut s'écrire avant ou après `-c`, et donne alors la capacité de l'arène cons de l'exécutable. L'arène cons compte dans la borne de mémoire : avec une borne où l'arène ne tient pas, le programme s'arrête avant de commencer.

### 3.6 Compiler pour WebAssembly

Avec `--target wasm32-wasip1`, `typl -c` produit un module WebAssembly (une commande WASI preview 1) au lieu d'un exécutable. Sans `-o`, il porte le nom de la source avec `.typl` remplacé par `.wasm`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # produit hello.wasm
$ wasmtime hello.wasm a b                        # l'exécuter avec un environnement compatible WASI
args: #(hello.wasm a b)
fib(25) = 75025
```

- Un `typl` installé avec `install.sh` s'utilise tel quel. Un `typl` installé par `cargo install` demande une préparation (qui vaut aussi pour la section 3.7). Lancez d'abord `rustup target add wasm32-wasip1 wasm32-unknown-unknown` avec rustup. Puis installez-le avec `--features wasm` (`cargo install --locked --path . --features wasm`).
  Un `typl` installé sans cela n'a pas de runtime WebAssembly, et `--target` est une erreur. Pour une version de débogage de `typl` dans le dépôt, lancez plutôt `scripts/build-wasm-runtime.sh` dans le dépôt.
- L'édition de liens utilise l'éditeur de liens de la toolchain rustup (`rust-lld`) : rustup est donc nécessaire aussi à la compilation.
- Le module se comporte comme l'exécutable : sortie standard, erreur standard et code de sortie, `(command-line-args)`,
  lecture et écriture de fichiers, `throw`/`catch`/`panic`, tâches, canaux et `sleep`, et `eval` fonctionnent.
  Les tâches s'exécutent l'une après l'autre sur un seul thread. Les fichiers ne peuvent être lus et écrits que
  dans les répertoires que l'environnement montre au module (avec wasmtime, on les donne avec `--dir`).
- Un programme qui utilise ce que WebAssembly n'a pas est une **erreur de compilation**. L'erreur indique ce qui est utilisé
  et la chaîne de fonctions depuis `main` qui y mène. Indisponibles : les sockets (TCP, UDP, domaine Unix,
  TLS, résolution de noms), les threads du système (`Thread`), les fonctions C (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`
  et `ed`.
- WASI n'a aucun moyen de connaître le fuseau horaire, donc l'heure locale est inconnue. `decode-universal-time` et `encode-universal-time` sans `zone`, ainsi que `get-decoded-time`, font un panic à l'exécution. Ils fonctionnent si `zone` est donné.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` et `--release` s'appliquent comme pour un exécutable.

### 3.7 Exécuter dans un navigateur ou avec Node.js

Avec `--target wasm32-unknown-unknown`, `typl -c` produit un module WebAssembly à charger et exécuter depuis JavaScript. À côté du module (`hello.wasm`), dans le même dossier, il écrit le JavaScript qui le charge et l'exécute (`hello.mjs`). La préparation est la même qu'à la section 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # produit hello.wasm et hello.mjs
$ node hello.mjs                                       # l'exécuter avec Node.js
args: #()
fib(25) = 75025
```

Depuis un autre JavaScript, appelez `run` de `hello.mjs`. `write` reçoit la sortie sous forme de texte (`fd` vaut 1 pour la sortie standard, 2 pour l'erreur standard). La valeur de `run` est le code de sortie.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* l'afficher dans la page, par exemple */ } });
```

- Sans `write`, Node.js écrit sur la sortie standard et l'erreur standard, et un navigateur dans la console, ligne par ligne.
- Dans un navigateur, servez `hello.wasm` en HTTP depuis le même endroit que `hello.mjs` (il ne peut pas être chargé depuis `file://`).
- `sleep` attend avec `Atomics.wait`. Un navigateur ne le permet que dans un Worker d'une page isolée cross-origin : exécutez donc dans un Worker un programme qui utilise `sleep`.
- L'heure locale (`decode-universal-time` sans `zone`, etc.) est calculée avec le fuseau horaire que donne `Date` de JavaScript.
- En plus de ce que la section 3.6 ne permet pas, les opérations sur les fichiers et les répertoires, les variables d'environnement, `(command-line-args)`, le répertoire personnel et `dribble` sont une **erreur de compilation**. L'entrée standard est lue comme vide.

### 3.8 Compiler pour un microcontrôleur (Cortex-M)

Avec `--target thumbv7em-none-eabihf`, `typl -c` crée un exécutable (ELF) pour une carte Cortex-M4F ou M7 sans système d'exploitation. Sans `-o`, il porte le nom de la source avec `.typl` remplacé par `.elf`.

```sh
$ typl -c hello.typl --target thumbv7em-none-eabihf          # crée hello.elf
$ qemu-system-arm -M mps2-an386 -nographic \
    -semihosting-config enable=on,target=native -kernel hello.elf   # l'exécuter sur le Cortex-M4 de QEMU
```

- Un `typl` installé avec `install.sh` s'utilise tel quel. Pour installer avec `cargo install`, lancez `rustup target add thumbv7em-none-eabihf`, puis installez avec `--features embedded` (`--features wasm,embedded` pour les deux). Avec une compilation de débogage de `typl` dans le dépôt, lancez `scripts/build-embedded-runtime.sh`. L'édition de liens utilise l'éditeur de liens de la toolchain rustup.
- Indiquez la mémoire de la carte avec `--linker-script FILE`. Elle s'écrit comme le `memory.x` de `cortex-m-rt` : les adresses et tailles de FLASH et de RAM. Une ligne comme `_typelisp_stack_size = 32K;` fixe la part de RAM prise par la pile (64KiB sinon) ; le reste de la RAM est le tas. Sans l'option, la disposition est celle du `mps2-an386` de QEMU.

  ```
  MEMORY
  {
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
  }
  _typelisp_stack_size = 16K;
  ```

- `--board-functions` choisit qui fournit la sortie, l'entrée, la fin du programme et l'horloge (les fonctions de la carte).
  - `semihosting` (par défaut) : par semihosting, pour exécuter sous QEMU (`-semihosting-config enable=on`) ou un débogueur.
  - `c` : la carte les fournit sous forme de fonctions C. Passez avec `--link FILE` un objet ou une bibliothèque statique qui définit les fonctions ci-dessous (`--link` peut être répété ; une bibliothèque du fabricant appelée par `defffi` se passe de la même façon).

  ```c
  void     typelisp_host_write(int32_t fd, const uint8_t *p, size_t len); // fd 1 est la sortie standard, 2 la sortie d'erreur
  size_t   typelisp_host_read(int32_t fd, uint8_t *p, size_t len);        // fd 0 ; octets lus, 0 à la fin
  void     typelisp_host_exit(int32_t code);                              // ne revient pas
  uint64_t typelisp_host_now_us(void);                                    // microsecondes d'une horloge qui ne recule jamais
  int64_t  typelisp_host_unix_us(void);                                   // microsecondes depuis 1970-01-01
  void     typelisp_host_sleep_us(uint64_t us);                           // attend us microsecondes
  ```

- Les registres d'une carte (périphériques projetés en mémoire) se lisent avec `volatile-read-u8`, `volatile-read-u16` et `volatile-read-u32`, et s'écrivent avec `volatile-write-u8`, `volatile-write-u16` et `volatile-write-u32`. L'adresse est un `int`, et la valeur lue a le type de la largeur (`u8`, etc.). Chacun lit ou écrit exactement une fois, là où il est écrit ; le compilateur ne l'omet ni ne le déplace. Tous ne s'utilisent qu'à l'intérieur de `unsafe`. Une adresse négative, ou qui n'est pas un multiple de la largeur, est une erreur à l'exécution avant toute lecture ou écriture.

  ```lisp
  ;; UART 0 du mps2-an386 de QEMU : données 0x40004000, état 0x40004004, contrôle 0x40004008
  (defun put-byte ((c u32)) ()
    (unsafe
      (loop (when (= (logand (volatile-read-u32 #x40004004) 1) 0) (break)))   ; attendre tant qu'il envoie
      (volatile-write-u32 #x40004000 c)))

  (defun main () ()
    (unsafe (volatile-write-u32 #x40004008 1))                               ; activer l'envoi
    (put-byte 104) (put-byte 105) (put-byte 10))                             ; "hi" et un saut de ligne
  ```

- Le runtime démarre le programme (la table des vecteurs et la préparation de la RAM) et alloue la mémoire. Un HardFault ou une interruption non traitée termine le programme avec le code de sortie d'un panic (1).
- `throw`/`catch`/`panic`, les bignums, les fractions et les flottants, ainsi que les tâches, les canaux et `sleep` se comportent comme dans l'exécutable. Les tâches s'exécutent à tour de rôle sur l'unique cœur. Les fonctions C (`defffi`) sont utilisables.
- Un programme qui utilise ce que la carte n'a pas est une **erreur de compilation** : en plus de ce que la section 3.7 ne permet pas, `eval` et `macroexpand`. L'heure locale est inconnue (comme à la section 3.6).
- L'arène de cons commence à 1024 cellules et grandit au besoin. `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` et `--release` fonctionnent tels quels.

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
