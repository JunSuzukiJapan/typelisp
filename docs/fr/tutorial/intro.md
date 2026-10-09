<!-- translated-from: docs/ja/tutorial/intro.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Premiers pas

En partant de l'évaluation d'expressions dans la REPL, ce chapitre aborde dans l'ordre les fonctions, les
variables, les conditionnelles, les boucles, ainsi que les listes et `Vector`. Pour compiler `typl`, voir le
[README.md](../../../README.md).

## 1. Lancer la REPL

Lancé sans arguments, `typl` entre dans la REPL (mode interactif). Tapez une expression après `typl>` : elle est
évaluée sur-le-champ et sa valeur est affichée. `:quit` quitte la REPL.

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

Dans la suite, les saisies dans la REPL et leurs résultats sont présentés sous cette forme.

## 2. Évaluer des expressions

typelisp est un Lisp : une expression s'écrit entre parenthèses avec **l'opérateur ou le nom de fonction en
premier**. On écrit `(+ 1 2)`, pas `1 + 2`.

```
typl> (* 2 (+ 3 4))
14
typl> (+ 1 2 3 4)
10
typl> "hello"
"hello"
typl> (upcase "hello")
"HELLO"
```

Les nombres sont de ces sortes :

- Les **entiers** ont le type `int`. Leur taille n'a pas de limite supérieure.
- Les **décimaux** ont le type `f64`. On les écrit avec un point décimal, comme `1.5` ou `2.0`.
- On ne peut pas mélanger `int` et `f64` dans un calcul. `(+ 1 2.0)` est une erreur de type. Pour convertir,
  écrivez `(as f64 1)`.

```
typl> (* 123456789012345 123456789012345)
15241578753238669120562399025
typl> (/ 7 2)
3
typl> (/ 7.0 2.0)
3.5
```

`/` sur deux entiers donne un entier dont la partie fractionnaire est supprimée (il ne produit pas de fraction
comme le fait Common Lisp). Pour le reste, utilisez `(mod 7 2)`.

Les valeurs booléennes sont `true` et `false`.

```
typl> (> 3 2)
true
typl> (and (> 3 2) (< 3 2))
false
```

## 3. Définir des fonctions

Les fonctions se définissent avec `defun`. **Les types des arguments et le type de retour s'écrivent
toujours.**

```lisp
(defun square ((n int)) int
  (* n n))
```

- `(n int)` signifie « un argument `n` de type `int` ». Avec plusieurs arguments, on les énumère :
  `((a int) (b int))`.
- Le `int` après la liste des arguments est le type de retour.
- La valeur de la dernière expression du corps est la valeur de retour de la fonction. On n'écrit pas
  `return`.

```
typl> (defun square ((n int)) int (* n n))
typl> (square 12)
144
typl> (square "a")
error: <stdin>:1:9: type error: type mismatch: expected `int`, found `string`
```

Un appel dont les types ne correspondent pas est signalé comme erreur de type **avant de s'exécuter**. Quand on
exécute un fichier, une seule erreur de type n'importe où suffit pour qu'aucune ligne du programme ne
s'exécute.

Pour rendre un argument facultatif, utilisez `&optional`. Si vous donnez une valeur par défaut, l'argument prend
cette valeur quand il est omis.

```lisp
(defun greet ((name string) &optional (greeting string "Hello")) string
  (format false "~a, ~a!" greeting name))
```

```
typl> (greet "Ann")
"Hello, Ann!"
typl> (greet "Ann" "Hi")
"Hi, Ann!"
```

Le `false` passé en premier argument de `format` signifie « renvoyer le résultat sous forme de chaîne au lieu de
l'afficher ». Chaque `~a` est remplacé par l'argument suivant.

## 4. Variables

Les variables locales se créent avec `let`.

```lisp
(defun sum-of-squares ((a int) (b int)) int
  (let ((aa (square a))
        (bb (square b)))
    (+ aa bb)))
```

- Le type d'une variable de `let` est déduit de sa valeur initiale. Il n'est pas nécessaire de l'écrire.
- Les variables d'un même `let` ne peuvent pas se référencer entre elles. Pour construire une variable à partir
  de la précédente, utilisez `let*`.

```
typl> (let* ((a 1) (b (+ a 1))) (* a b))
2
```

Pour changer la valeur d'une variable, utilisez `setf`. **Une affectation ne peut pas changer le type de la
variable.**

```
typl> (let ((x 1)) (setf x "a"))
error: <stdin>:1:22: type error: type mismatch: expected `int`, found `string`
```

Les variables globales se définissent avec `defvar`. Ici, on écrit le type.

```lisp
(defvar (counter int) 0)
```

## 5. Conditionnelles

### if

Écrivez `(if condition expression-alors expression-sinon)`. **L'expression « sinon » ne peut pas être omise.**

```lisp
(defun sign ((n int)) string
  (if (< n 0) "negative" "non-negative"))
```

- Seule une expression de type `bool` peut être une condition. Écrire un nombre, comme `(if 0 ...)`, est une
  erreur de type.
- Les expressions « alors » et « sinon » doivent avoir le même type.

Quand rien ne doit se passer dans le cas faux, utilisez `when` (et `unless` pour l'inverse).

```lisp
(defun report-size ((n int)) ()
  (when (> n 100)
    (println "large")
    (println "really large")))
```

### cond

Avec trois conditions ou plus, `cond` est plus lisible. Le `else` final est pris quand aucune des conditions
n'est vérifiée.

```lisp
(defun describe-number ((n int)) string
  (cond ((< n 0) "negative")
        ((= n 0) "zero")
        ((< n 10) "small")
        (else "large")))
```

### match

Pour bifurquer selon la forme d'une valeur, utilisez `match`.

```lisp
(defun day-name ((d int)) string
  (match d
    (0 "Sun")
    (6 "Sat")
    (_ "weekday")))
```

`_` correspond à n'importe quelle valeur. Comme `int` a d'innombrables valeurs, omettre la branche `_` est une
erreur indiquant que tous les cas ne sont pas couverts. Là où `match` brille vraiment, c'est pour décomposer
`Option` et les types que vous définissez vous-même, présentés au chapitre suivant,
[Bases des types](types.md).

## 6. Boucles

Une fonction peut s'appeler elle-même.

```lisp
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
typl> (fact 30)
265252859812191058636308480000000
```

Pour un nombre fixe de répétitions, utilisez `dotimes`. `i` va de 0 à `n - 1`.

```lisp
(defun sum-to ((n int)) int
  (let ((total 0))
    (dotimes (i (+ n 1))
      (setf total (+ total i)))
    total))
```

```
typl> (sum-to 100)
5050
```

Il existe aussi `while`, `do` et le `loop` étendu de Common Lisp. Les mots de clause du `loop` étendu s'écrivent
comme des mots-clés (`:for`, `:collect`, etc.).

```
typl> (loop :for i :from 1 :to 5 :collect (* i i))
#(1 4 9 16 25)
```

## 7. Listes et Vector

### Vector

Pour conserver une suite de valeurs d'un même type, utilisez `Vector<T>`. `T` est le type des éléments.

```lisp
(let ((v (the Vector<int> (Vector::new))))
  (push v 3)
  (push v 1)
  (push v 2)
  (println "~a" v))
;; affiche #(3 1 2)
```

- `(Vector::new)` seul ne détermine pas le type des éléments ; on le donne avec `(the Vector<int> ...)`.
- `(push v x)` ajoute à la fin, `(get v i)` lit l'élément `i` et `(len v)` donne la longueur.
- Un `get` avec un indice hors limites arrête le programme sur une erreur.

### lambda et fonctions d'ordre supérieur

Les fonctions anonymes se créent avec `lambda`. Comme pour `defun`, on écrit les types des arguments et du
retour.

```
typl> ((lambda ((x int)) int (* x 2)) 21)
42
```

`map`, `filter`, `sort`, `foldl` et consorts prennent un `Vector` transformé en **itérateur**
avec `(iter v)`. La collection vient en premier et la fonction en second. Le `v` ci-dessus a
été lié par `let` et n'est donc pas utilisable hors de ce `let`. L'exemple suivant définit
d'abord `v` avec `defvar`.

```lisp
(defvar (v Vector<int>) (Vector::new))
(push v 3)
(push v 1)
(push v 2)

(map (iter v) (lambda ((x int)) int (* x 10)))                 ; => #(30 10 20)
(filter (iter v) (lambda ((x int)) bool (> x 1)))              ; => #(3 2)
(sort (iter v) (lambda ((a int) (b int)) bool (< a b)))        ; => #(1 2 3)
(foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)  ; => 6
```

Pour traiter les éléments un par un, utilisez `doiter`.

```lisp
(doiter (x (iter v))
  (println "item ~a" x))
```

Une fonction qui prend une fonction en argument écrit le type de cet argument sous la forme
`(fn (types-des-arguments...) type-de-retour)`. Une fonction définie avec `defun` peut être passée comme valeur
par son nom.

```lisp
(defun twice ((f (fn (int) int)) (x int)) int
  (f (f x)))

(twice square 3)                                ; => 81
(twice (lambda ((x int)) int (* x 3)) 2)        ; => 18
```

### Listes (S-expressions)

Les listes créées avec `'(1 2 3)` ou `(list 1 2 3)` sont des **données S-expression**. Leurs éléments n'ont pas
besoin d'avoir le même type.

```
typl> '(1 2 3)
(1 2 3)
typl> (list 1 "two" 'three)
(1 "two" three)
```

Les données S-expression servent surtout à manipuler des programmes eux-mêmes, dans les macros
([Macros](macros.md)) et avec `read`. Pour des données dont le type des éléments est connu, utilisez
`Vector<T>`. Une liste S-expression se parcourt avec `dolist`.

```lisp
(dolist (x '(1 2 3))
  (println "x=~a" x))
```

Une paire de deux valeurs se crée avec `cons` et se décompose avec `car` et `cdr`.

```
typl> (let ((p (cons "age" 42))) (cdr p))
42
```

## 8. Écrire un programme dans un fichier

Un programme peut s'écrire dans un fichier (d'extension `.typl`) et s'exécuter avec `typl nom-du-fichier`.
Utilisez `println` pour afficher les résultats.

```lisp
;; hello.typl
(defun fact ((n int)) int
  (if (= n 0) 1 (* n (fact (- n 1)))))

(dotimes (i 5)
  (println "~a! = ~a" i (fact i)))
```

```sh
$ typl hello.typl
0! = 1
1! = 1
2! = 2
3! = 6
4! = 24
```

- `println` affiche avec les mêmes directives que `format` et termine par un saut de ligne. `print` n'ajoute pas
  le saut de ligne.
- `~a` insère une valeur sous une forme lisible par un humain, et `~s` sous une forme relisible (les chaînes
  gardent leurs `"`).
- Un fichier est lu de haut en bas. **Une fonction ne peut pas être appelée avant sa définition.**

## 9. Que lire ensuite

- [Bases des types](types.md) : `Option`, `Result`, structures, énumérations, génériques
- [Pour les programmeurs Common Lisp](../guide/from-common-lisp.md) : une liste des différences pour qui connaît
  Common Lisp
