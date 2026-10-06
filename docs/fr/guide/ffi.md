<!-- translated-from: docs/ja/guide/ffi.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# FFI C (defffi)

Ce guide explique comment appeler des fonctions C depuis typelisp. La liste des types déclarables et les
restrictions se trouvent dans
[Référence de la syntaxe 3.3](../reference/syntax.md#33-defffi--déclarer-des-fonctions-c-ffi).

## 1. Déclarer et appeler une fonction

`defffi` déclare le nom et les types d'une fonction C.

```lisp
(defffi (c-getpid "getpid") () i32)            ; le nom typelisp et le nom de symbole C
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; la chercher dans libm
```

Les appels s'enveloppent dans `(unsafe ...)`.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` est nécessaire parce que le compilateur ne peut pas vérifier que les types déclarés correspondent aux
types réels côté C. Écrire `unsafe` signifie que vous, l'auteur, prenez la responsabilité de cette vérification.
L'oublier donne une erreur qui l'explique.

## 2. Écrire une enveloppe sûre

L'usage prévu est de confiner `unsafe` à un seul endroit et de présenter une fonction ordinaire à l'extérieur.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; l'appelant n'a pas besoin de unsafe
(str-len "hello")  ; => 5
```

## 3. Correspondance des types

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Entiers de même largeur |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (aussi `size_t`, `int64_t`, etc.) |
| `ptr` | N'importe quel pointeur (`void *`, `FILE *`, etc.) |
| `(ptr T)` | Un pointeur vers `T` ([section 7](#7-structures-c)) |

### Chaînes

- Une `string` passée est copiée dans une chaîne C terminée par NUL, libérée au retour de l'appel. Un NUL au
  milieu de la chaîne est une erreur.
- Le résultat d'une fonction qui renvoie `string` est copié lui aussi. La mémoire côté C n'est pas libérée. Pour
  les fonctions qui renvoient une chaîne que l'appelant doit libérer (comme `strdup`), recevez le résultat comme
  `ptr` et appelez `free` vous-même.
- Si une fonction déclarée comme renvoyant `string` renvoie NULL, c'est une erreur. Recevez comme `ptr` le
  résultat des fonctions qui peuvent renvoyer NULL (comme `getenv`).

### `c-long` / `c-ulong` / `ptr`

Ces types n'existent que pour faire passer des valeurs à la frontière avec C et **ne prennent en charge aucune
arithmétique**. Pour en utiliser un comme entier typelisp, convertissez-le avec `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int ne perd rien de la valeur 64 bits
(try-as i32 (unsafe (c-strlen s)))  ; none si elle ne tient pas dans un i32
(unsafe (c-malloc 16))              ; les littéraux entiers peuvent être passés tels quels
```

Un `ptr` est une valeur à rendre à des fonctions C. Il n'y a aucun moyen de lire ce qu'il désigne depuis typelisp.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Ces types ne peuvent apparaître que comme arguments de fonction, valeurs de retour et variables locales. Ils ne
peuvent pas être des champs de structure, des variables globales ni des arguments de type de `Vector` et
semblables.

## 4. Nommer une bibliothèque

Sans `:library`, le symbole est cherché dans ce qui est déjà lié au processus (libc, etc.). Les fonctions d'autres
bibliothèques exigent `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Un nom court comme `"sqlite3"` est cherché comme `libsqlite3.dylib`, puis `libsqlite3.so`.
- Un nom contenant `/` est traité comme un chemin.
- Si le symbole déclaré est introuvable, l'erreur le nomme.

## 5. Compilation AOT

Les programmes qui utilisent `defffi` peuvent être transformés tels quels en exécutables avec
[`compile-file`](compile.md#3-créer-un-exécutable-par-compilation-aot). Les bibliothèques nommées avec `:library`
sont ajoutées automatiquement à l'édition de liens ; `compile-file` n'a donc besoin d'aucun argument
supplémentaire.

## 6. Callbacks

On peut passer une fonction typelisp à une fonction C et la faire rappeler. Écrivez un type de fonction parmi les
types d'arguments de `defffi`, et à l'appel placez à cette position un nom de fonction ou une expression
`lambda`.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") renvoie p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Seules les fonctions **sans variables libres** peuvent être passées. Les fonctions de niveau supérieur, les
  `lambda` et les fonctions locales `labels` fonctionnent toutes, mais faire référence à une variable locale d'une
  portée englobante est une erreur à la vérification des types. C ne passe que les arguments déclarés ; il n'y a
  donc aucun moyen de transmettre des variables capturées. Pour conserver un état, utilisez des variables
  globales.
- Une variable contenant une fonction ne peut pas être passée. Écrivez à la place un nom de fonction ou une
  expression `lambda`.
- Un `panic` ou un `throw` dans le callback atteint l'appelant après le retour de la fonction C.
- Le callback ne peut être appelé que pendant l'exécution de la fonction C que typelisp a appelée. Il ne peut pas
  être utilisé depuis des choses comme `atexit` ou des gestionnaires de signaux.

## 7. Structures C

Pour passer quelque chose comme un tableau de structures à une fonction C, déclarez avec `def-c-struct` une
structure de même disposition qu'en C, et allouez-la à l'intérieur de `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; déclarée dans un unsafe de niveau supérieur

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; quatre éléments, tous à zéro
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` alloue `n` valeurs de `T` et renvoie un `(ptr T)`. `(c-ref p i)` est un pointeur vers le
  `i`-ième, `p::field` est un champ, et `(c-deref p)` est ce que désigne un pointeur vers un scalaire comme `i32`.
  Tous peuvent être écrits avec `setf`.
- `(as ptr p)` le transforme en `ptr` non typé pour le passer à des fonctions C qui prennent un `void *`.
- La taille de `item` (ici 8) et la position de chaque champ sont déterminées selon les mêmes règles qu'en C.

### Durée de vie de la mémoire allouée

La mémoire allouée est libérée quand l'exécution quitte le `unsafe` le plus extérieur de cette fonction. Il en va
de même quand on le quitte par `panic` ou `throw`. De ce fait, une valeur `(ptr T)` ne peut pas sortir du
`unsafe`. En faire la valeur du `unsafe`, la capturer dans une fermeture, la passer à un `task` ou la lancer avec
`throw` sont tous des erreurs de type. Copiez les valeurs que vous voulez utiliser à l'extérieur dans des nombres
ou un `defstruct` à l'intérieur du `unsafe`.

Quand on alloue à l'intérieur d'un `lambda` ou d'une fonction `labels`, on écrit un `unsafe` à l'intérieur de
cette fonction.

### Mémoire allouée par C

Un pointeur reçu de C comme `(ptr T)` (une valeur de retour de `defffi`, un argument de callback, etc.) est une
erreur s'il ne pointe pas dans de la mémoire allouée avec `c-alloc`. Déclarez avec le `ptr` non typé les fonctions
qui reçoivent de la mémoire allouée par C avec `malloc`, ou NULL.

## 8. Ce qui est impossible

- **Les fonctions variadiques** (`printf` et semblables) ne peuvent pas être déclarées. La partie variadique est
  passée selon d'autres règles que les arguments fixes. Déclarez un nom distinct pour chaque nombre d'arguments
  que vous utilisez.
- **Passer ou renvoyer des structures par valeur** est impossible. Utilisez des fonctions qui passent des
  pointeurs.
- **Les déclarations génériques** sont impossibles.
- **Le même nom qu'une fonction intégrée** ne peut pas être utilisé.
- **Elles ne peuvent pas être passées comme valeurs fonctionnelles.** On ne peut pas en passer une comme dans
  `(map xs c-abs)` ; enveloppez-la dans un `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
