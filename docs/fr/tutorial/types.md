<!-- translated-from: docs/ja/tutorial/types.md @ fc3823e182015d6a1ef25d03ecdf8ca958af01f2 -->
# Bases des types

typelisp est un langage à typage statique. Ce chapitre explique ce que le vérificateur de types fait pour vous,
les types que vous utiliserez le plus (`Option`, `Result`, structures et énumérations) et les génériques. Il
suppose que vous avez lu [Premiers pas](intro.md).

## 1. Ce que signifie le typage statique

En typelisp, le type de chaque expression est fixé avant l'exécution du programme. Une expression dont les types
ne concordent pas est une erreur avant que quoi que ce soit ne s'exécute.

```lisp
(defun square ((n int)) int (* n n))

(defun main () ()
  (println "start")
  (println "~a" (square "3")))    ; erreur de type

(main)
```

L'exécution de ce fichier s'arrête sur une erreur de type sans même afficher `start`.

```
error: main.typl:5:25: type error: type mismatch: expected `int`, found `string`
```

Il faut écrire les types des arguments et des valeurs de retour des fonctions, des variables globales et des
champs de structure. Le type d'une variable de `let` est déduit de sa valeur initiale.

Les principaux types :

| Type | Exemples de valeurs |
|---|---|
| `int` | `42`, `-7` (entiers en précision arbitraire) |
| `i8` `i16` `i32` `u8` `u16` `u32` | Entiers de largeur fixe |
| `f64` `f32` | `1.5` |
| `bool` | `true`, `false` |
| `char` | `#\a` |
| `string` | `"hello"` |
| `symbol` | `'foo`, `:key` |
| `()` | Le type de retour d'une fonction qui ne renvoie pas de valeur |

Il n'existe aucun moyen de demander le type d'une valeur à l'exécution (pas de `typep` ni de `type-of` comme en
Common Lisp), car chaque type est déjà connu avant l'exécution du programme.

## 2. `Option<T>` : une valeur qui peut manquer

typelisp n'a pas de `nil`. « Il peut ne pas y avoir de valeur » s'exprime avec le type `Option<T>`. Une valeur de
`Option<T>` est soit `some`, qui contient une valeur de `T`, soit `none`, qui ne contient rien.

```lisp
(defun safe-div ((a int) (b int)) Option<int>
  (if (= b 0)
      (Option::none)
      (Option::some (/ a b))))
```

```
typl> (safe-div 10 2)
(some 5)
typl> (safe-div 1 0)
none
```

`Option<int>` n'est pas `int` ; on ne peut donc pas l'utiliser tel quel dans un calcul. `(+ (safe-div 10 2) 1)`
est une erreur de type. Pour utiliser le contenu, séparez `some` de `none` avec `match`.

```lisp
(defun show-div ((a int) (b int)) string
  (match (safe-div a b)
    ((some q) (format false "~a" q))
    ((none) "undefined")))
```

- Dans la branche `(some q)`, le contenu est lié à la variable `q`.
- `match` vérifie que ses branches **couvrent tous les cas**. Oublier la branche `(none)` est une erreur de type.

### Pourquoi il n'y a pas de nil

Dans beaucoup de langages, `nil` (`null`) peut tenir lieu d'une valeur de n'importe quel type. En conséquence,
oublier de traiter le cas « pas de valeur » passe inaperçu jusqu'à l'exécution. En typelisp, un endroit où une
valeur peut manquer a le type `Option<T>`, et le code ne passe pas la vérification de types tant que `match` ne
traite pas le cas `none`. Un cas oublié est détecté avant l'exécution du programme.

Les conditions suivent la même idée : seul un `bool` peut être la condition d'un `if`. Il n'y a pas de règle
comme celle de Common Lisp selon laquelle « tout ce qui n'est pas `nil` est vrai ».

### Opérations courantes

| Forme | Signification |
|---|---|
| `(unwrap-or opt default)` | Le contenu pour `some` ; la valeur par défaut pour `none` |
| `(unwrap opt)` | Extrait le contenu. Arrête le programme sur `none` |
| `(is-some opt)` / `(is-none opt)` | Teste de quel cas il s'agit |

Beaucoup de fonctions de la bibliothèque standard renvoient une `Option`. Par exemple, `position` renvoie la
position dans `some` si l'élément est trouvé, et `none` sinon.

```lisp
(defvar (fruits Vector<string>) (Vector::new))
(push fruits "apple")
(push fruits "banana")

(match (position "banana" (iter fruits))
  ((some i) (println "found at ~a" i))
  ((none) (println "not found")))
```

## 3. `Result<T,E>` : une opération qui peut échouer

Une opération qui peut échouer renvoie `Result<T,E>` : `ok` contenant une valeur de `T` en cas de succès, ou
`err` contenant une erreur `E` en cas d'échec.

```lisp
(match (parse-int "42")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; n = 43

(match (parse-int "4x2")
  ((ok n) (println "n = ~a" (+ n 1)))
  ((err e) (println "error: ~a" (message e))))
;; error: parse-int: invalid integer literal: "4x2"
```

Vos propres fonctions peuvent aussi renvoyer un `Result`.

```lisp
(defun checked-age ((n int)) Result<int,string>
  (if (< n 0)
      (Result::err "age must not be negative")
      (Result::ok n)))
```

Utilisez `Option` quand une valeur manquante ne demande pas d'explication, et `Result` quand vous voulez dire
pourquoi quelque chose a échoué. [Gestion des erreurs](errors.md) détaille le traitement des erreurs.

## 4. `defstruct` : les structures

Un type à champs nommés se définit avec `defstruct`.

```lisp
(defstruct point
  (x int)
  (y int))
```

La définition vous fournit ceci :

```lisp
(let ((p (point::new 3 4)))     ; en créer une (arguments dans l'ordre des champs)
  (println "~a" p::x)           ; lire un champ ; (x p) fonctionne aussi
  (setf p::x 10)                ; le modifier
  (println "~a" p))             ; #<point x: 10 y: 4>
```

Pour donner à une structure ses propres fonctions, utilisez `defmethod`. Le type du premier argument (`self`)
décide à quel type appartient la méthode.

```lisp
(defmethod norm2 ((self point)) int
  (+ (* self::x self::x) (* self::y self::y)))

(norm2 (point::new 3 4))        ; => 25
```

Écrire seulement le nom du type au lieu d'un argument `self` crée une fonction qui s'appelle sous la forme
`point::origin`.

```lisp
(defmethod origin (point) point
  (point::new 0 0))

(point::origin)                 ; => #<point x: 0 y: 0>
```

## 5. `defenum` : une forme parmi plusieurs

Une valeur qui prend une forme parmi plusieurs, comme « un cercle, un rectangle ou un point », se définit avec
`defenum`. Chaque forme s'appelle une **variante**. Chaque variante peut contenir un nombre et des types de
valeurs différents.

```lisp
(defenum shape
  (circle int)        ; rayon
  (rect int int)      ; largeur et hauteur
  (dot))              ; ne contient aucune valeur
```

Les valeurs se créent avec le nom du type devant, comme `shape::circle`. Dans `match`, on les décompose par nom de
variante.

```lisp
(defun area ((s shape)) int
  (match s
    ((circle r) (* 3 r r))
    ((rect w h) (* w h))
    ((dot) 0)))

(area (shape::circle 2))        ; => 12
(area (shape::rect 3 4))        ; => 12
```

Ici aussi, `match` vérifie que tous les cas sont couverts. Si vous ajoutez plus tard une variante à `shape`,
chaque `match` qui ne la traite pas devient une erreur de type : aucun endroit à corriger n'est oublié.

Après `(use shape)`, on peut écrire `(rect 5 6)` sans le nom du type.

`Option` et `Result` sont des énumérations construites avec ce même mécanisme.

## 6. Génériques

Une fonction qui marche pour n'importe quel type se définit avec un **paramètre de type** `<T>` après son nom.

```lisp
(defun first-or<T> ((v Vector<T>) (default T)) T
  (if (= (len v) 0)
      default
      (get v 0)))
```

On ne donne pas le type à l'appel. `T` est déduit des arguments.

```lisp
(defvar (ints Vector<int>) (Vector::new))
(defvar (names Vector<string>) (Vector::new))
(push names "Ann")

(first-or ints 7)          ; T est int
(first-or names "none")    ; T est string
(first-or ints "none")     ; erreur de type : ints est un Vector<int>, donc T est int
```

Les structures et les énumérations peuvent aussi être génériques. `Vector<T>`, `Option<T>` et `Result<T,E>` sont
des types de ce genre.

```lisp
(defstruct pair<A,B>
  (left A)
  (right B))

(pair::new 1 "one")        ; pair<int,string>
```

À l'intérieur d'une fonction générique, on ne sait rien de `T` ; on ne peut donc ni comparer ni additionner des
valeurs de `T`. Pour exiger quelque chose comme « n'importe quel type comparable », utilisez les traits
([Traits](traits.md)).

## 7. Donner un autre nom à un type

`deftype` donne un autre nom à un type.

```lisp
(deftype meters int)
(defun double ((m meters)) meters (* m 2))
```

`meters` n'est qu'une autre écriture de `int`, pas un nouveau type. Passer un simple `int` là où `meters` est
attendu n'est pas une erreur. Si vous voulez les distinguer, créez une structure, comme
`(defstruct meters (value int))`.

## 8. Que lire ensuite

- [Traits](traits.md) : donner des opérations communes à des types
- [Types](../reference/types.md) : les types intégrés et les traits que chacun implémente
