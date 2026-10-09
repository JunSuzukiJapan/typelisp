<!-- translated-from: docs/ja/guide/from-common-lisp.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Pour les programmeurs Common Lisp

typelisp reprend la syntaxe de Common Lisp (CL) et beaucoup de ses noms de fonctions, mais c'est un langage à
typage statique. De ce fait, du code CL ne fonctionne pas toujours tel quel. Ce guide rassemble les points où les
habitués de CL trébuchent souvent, avec la façon de réécrire le code.

## 1. Il n'y a ni `nil` ni `t`

Les valeurs booléennes sont `true` et `false`. `nil` et `t` ne sont pas définis.

```lisp
(if (> x 0) "pos" "non-pos")
(defvar (debug bool) false)
```

- **Seul un `bool` peut être une condition.** Écrire `0` ou une liste vide comme condition est une erreur de type.
  La règle « tout ce qui n'est pas nil est vrai » n'existe pas.
- **La branche « sinon » de `if` ne peut pas être omise.** `(if c x)` est une erreur. Quand aucune branche
  « sinon » n'est nécessaire, utilisez `when` / `unless`.
- **« Pas de valeur » s'exprime avec `Option<T>`.** Une fonction qui renvoyait nil en CL pour dire « non trouvé »
  renvoie ici `(some x)` ou `none`.

  ```lisp
  (match (position 3 (iter v))
    ((some i) (println "found at ~a" i))
    ((none) (println "not found")))
  ```

- La liste vide `()` est, selon le contexte, soit la valeur du type Unit (la valeur de retour d'une fonction qui ne
  renvoie rien), soit la liste vide des données S-expression. C'est une valeur différente de `false`.

## 2. Écrire les types

Les arguments et les valeurs de retour des fonctions doivent avoir des types.

```lisp
(defun area ((w i32) (h i32)) i32
  (* w h))

(defun first-or<T> ((v Vector<T>) (default T)) T      ; une fonction générique
  (unwrap-or (first (iter v)) default))
```

- Une définition sans types comme `(defun f (x) x)` ne peut pas s'écrire.
- Les variables globales comme `defvar` ont aussi besoin d'un type : `(defvar (count int) 0)`.
- `the` n'est pas une vérification à l'exécution, mais une annotation pour le vérificateur de types.
- **Il n'y a aucun moyen d'examiner les types à l'exécution.** Il n'y a ni `typep` ni `type-of`, car le type de
  chaque valeur est fixé à la compilation. Pour accepter un type parmi plusieurs, créez un type somme avec
  `defenum` ou utilisez un trait.
- `deftype` définit un alias de type. Un type décrivant une plage de valeurs, comme
  `(deftype small () '(integer 0 9))`, ne peut pas être créé.

Le type entier par défaut `int` est en précision arbitraire ; comme l'integer de CL, sa taille n'a pas de limite
supérieure. Les types de largeur fixe `i8` à `i32` et `u8` à `u32` existent aussi. Il n'y a pas de type entier de
largeur fixe sur 64 bits.

## 3. Les fonctions comme valeurs

typelisp ne sépare pas les espaces de noms des fonctions et des variables. Le nom d'une fonction peut être passé
tel quel comme valeur. Il n'y a ni `#'` ni `funcall`.

```lisp
(defun twice ((x int)) int (* 2 x))
(defun apply-to ((f (fn (int) int)) (x int)) int
  (f x))                                  ; l'appeler directement, pas avec funcall

(apply-to twice 5)                        ; twice, pas #'twice
```

- Les fonctions intégrées comme `+` et `1+` peuvent aussi être passées telles quelles comme valeurs, quand le type
  de l'argument est fixé, comme dans `(fn (int) int)`. Quand on en passe une à une fonction générique comme
  `foldl` ou `map`, on ne sait pas de quel type est le `+` voulu ; enveloppez-la alors dans un `lambda`.

  ```lisp
  (apply-to 1+ 5)                                          ; => 6
  (foldl (iter v) (lambda ((acc int) (x int)) int (+ acc x)) 0)
  ```

- Les fonctions de séquence prennent **la collection en premier et la fonction en second** : `(map it f)`,
  `(filter it f)`, `(foldl it f init)`. C'est l'inverse du `(mapcar f list)` de CL.
- `lambda` ne peut pas utiliser `&optional` ni `&key` (`&rest`, si).
- **Une fonction ne peut pas être appelée avant d'être définie.** En CL, on peut appeler une fonction définie plus
  loin ; ici, cela donne `no such function`. Pour des fonctions mutuellement récursives, déclarez d'abord l'une
  d'elles avec `defsignature`.

  ```lisp
  (defsignature odd2 (i32) bool)
  (defun even2 ((n i32)) bool (if (= n 0) true (odd2 (- n 1))))
  (defun odd2 ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
  ```

## 4. Listes et Vector

Ce qui correspond à une liste CL, ce sont les **données S-expression**, de type `Option<Sexpr>` (la liste vide est
`none`). `(list 1 2 3)` et `'(a b c)` ont ce type. Les données S-expression sont ce que manipulent les macros et
`read` ; comme conteneur de données ordinaire, utilisez **`Vector<T>`**.

| Ce qu'on veut | CL | typelisp |
|---|---|---|
| Tête et reste d'une S-expression | `(car xs)` `(cdr xs)` | `(sexpr-car xs)` `(sexpr-cdr xs)` |
| Parcourir une liste S-expression | `(dolist (x xs) ...)` | Pareil |
| Une suite d'éléments d'un même type | Une liste ou un vecteur | `Vector<T>` |
| Une paire | `(cons a b)` | `(cons a b)` (son type est `cons-cell<A,B>`) |
| Application | `(mapcar f xs)` | `(map (iter v) f)` |

`car` / `cdr` sont les accesseurs de la paire `cons-cell<A,B>` créée avec `cons`. Ils ne s'utilisent pas sur des
listes S-expression.

Un `Vector` s'écrit avec `#(..)`, comme un vecteur en CL, et s'imprime aussi sous la forme `#(..)` :

```lisp
(let ((v #(1 2)))
  (push v 3)
  (map (iter v) (lambda ((x int)) int (* x 10))))   ; => #(10 20 30)
```

Il y a trois différences avec CL. Tous les éléments doivent avoir le même type (`#(1 "a")` est une
erreur de type). Chaque évaluation crée un nouveau vecteur : le modifier n'a donc pas d'effet sur
l'évaluation suivante (en CL, le résultat de la modification d'un littéral n'est pas défini). Un
`#()` vide a besoin de son type, comme dans `(the Vector<int> #())`. Un tableau multidimensionnel
s'écrit `#2A((1 2) (3 4))`, comme en CL.

Les fonctions de séquence comme `map`, `filter`, `sort` et `find` agissent sur des valeurs qui implémentent le
trait `Iter`. Passez un `Vector` après l'avoir transformé en itérateur avec `(iter v)`.

## 5. Il n'y a pas de valeurs multiples

Il n'y a ni `values` ni `multiple-value-bind`. Les fonctions qui renvoient plusieurs valeurs en CL renvoient ici
une paire ou une structure.

| CL | typelisp |
|---|---|
| `(floor 7 2)` → 3, 1 | `(floor-div 7 2)` → une `cons-cell` dont le `car` vaut 3 et le `cdr` 1 |
| `(decode-universal-time t)` → 9 valeurs | Une structure `decoded-time` |
| `(read-from-string s)` → valeur, position | `(read-from-string s)` renvoie une `cons-cell` de la valeur et de la position dans un `Result`. Pour la valeur seule, `(read s)` |

## 6. Il n'y a pas de variables spéciales (liaison dynamique)

`let` lie toujours lexicalement. Si vous liez de nouveau avec `let` une variable définie par `defvar`, les
fonctions appelées depuis là voient toujours la valeur d'origine.

```lisp
(defvar (*depth* int) 1)
(defun show () () (println "~a" *depth*))
(let ((*depth* 2)) (show))       ; 2 en CL, 1 en typelisp
```

Pour changer temporairement une variable de contrôle comme `*print-base*`, utilisez `dlet`. Il affecte la valeur
et restaure l'originale quelle que soit la façon dont le corps est quitté.

```lisp
(dlet ((*print-base* 16))
  (format false "~a" 255))       ; => "ff"
```

`dlet` réécrit la variable globale elle-même ; ce n'est donc pas une liaison par thread.

## 7. Le système de conditions n'est pas adopté

Il n'y a ni `define-condition`, ni `handler-case`, ni `handler-bind`, ni `restart-case`, ni `error`, ni `signal`.
Ils s'accordent mal avec le typage statique. À la place, ces deux mécanismes servent à des fins différentes :

- **Les échecs récupérables renvoient `Result<T,E>`.** L'appelant sépare `ok` / `err` avec `match`. Il n'existe pas
  de raccourci comme le `?` de Rust.

  ```lisp
  (match (parse-int "42x")
    ((ok n) n)
    ((err e) (progn (println "bad input: ~a" (message e)) 0)))
  ```

- **Les échecs irrécupérables (bogues) sont des `panic`.** `(panic "message")`, passer `none` à `unwrap`, diviser
  par 0 et un indice hors limites sont de cette sorte, et le programme s'arrête. Le nettoyage de `unwind-protect`
  s'exécute avant l'arrêt.

Les types d'erreur sont unifiés par le trait `Error`, et `(message e)` donne le message. La façon de créer son
propre type d'erreur se trouve dans
[Option, Result et types d'erreur](../reference/functions/option-result.md#3-types-derreur-et-le-trait-error).
`assert` et `warn` s'utilisent comme en CL.

`catch` / `throw` / `unwind-protect` existent. Cependant, l'étiquette de `catch` est limitée à un symbole littéral
non évalué (`'done`), et les valeurs lancées avec une étiquette ont un seul type.

## 8. Il n'y a pas de CLOS

Il n'y a ni `defclass`, ni `defgeneric`, ni combinaison de méthodes.

- Les types de données se définissent avec `defstruct` (structures) et `defenum` (types somme).
- `defmethod` définit des méthodes dont la cible est décidée uniquement par **le type statique du premier
  argument**. Il n'y a pas de dispatch multiple.
- Pour donner des opérations communes à plusieurs types, utilisez des traits (`deftrait` / `impl`). Pour des
  valeurs dont le type concret n'est connu qu'à l'exécution, utilisez le type `:dyn Trait`
  ([Référence de la syntaxe 3.9](../reference/syntax.md#39-deftrait--impl--traits)).

Ce qui diffère pour `defstruct` :

- Le constructeur est `NomDuType::new` : `(point::new 1 2)`. Si vous voulez un nom comme `make-point`, l'option
  `(:constructor make-point)` en crée un.
- En plus de `(x p)`, un accesseur peut s'écrire `p::x`. On le modifie avec `(setf p::x 5)`.
- Aucun prédicat (`point-p`) n'est créé. Il n'y a ni `:conc-name`, ni `:type`, ni `:named`.
- `:include` hérite seulement des slots ; le type ne devient pas un sous-type du parent.

## 9. Des modules au lieu des paquetages

Il n'y a pas de paquetages. Les espaces de noms sont des modules, et un fichier est un module à lui seul. Au lieu
de `pkg:symbol`, écrivez `module::name`, et importez les noms avec `use`
([Modules et découpage en fichiers](modules.md)).

Les mots-clés `:foo` existent ; ce sont des symboles qui s'évaluent en eux-mêmes. Comme il n'y a pas de
paquetages, le deux-points fait partie du nom : `(symbol->string :foo)` renvoie `":foo"`.

## 10. Différences de lecture et de syntaxe

- Majuscules et minuscules ne sont pas distinguées (les symboles passent en minuscules à la lecture). Comme en CL.
- Il n'y a pas de `#'` (section 3). Les littéraux de nombres complexes `#c(...)` ne peuvent pas être lus ; créez
  les nombres complexes avec `(complex 1.0 2.0)`.
- Les clauses du `loop` étendu s'écrivent avec des mots-clés : `(loop :for i :from 1 :to 3 :collect i)`. Un `loop`
  qui ne commence pas par un mot-clé est une simple boucle infinie, dont on sort avec `(break)` ou
  `(return valeur)`. `return` quitte la boucle la plus interne (pour quitter une fonction, utilisez `return-from`).
- La destination de `format` est `false` (renvoyer une chaîne), `true` (sortie standard) ou un flux. Les
  directives de format sont les mêmes qu'en CL.
- On lit depuis une chaîne avec `(read "...")` et depuis un flux avec `(read-sexpr s)`. Les deux renvoient un
  `Result`.
- `eval` vérifie les types de l'expression donnée avant de l'évaluer et renvoie un `Result`. Les références
  anticipées sont impossibles, comme dans le code source.
- Il n'y a pas d'`eval-when`.
- Les noms de fonctions n'utilisent pas les suffixes `?` ou `!`. Les prédicats se nomment avec `-p` / `p` comme en
  CL (`zerop`, `sexpr-null`), ou avec `is-` devant (`is-some`).

## 11. Principales fonctions dont le nom diffère

| CL | typelisp |
|---|---|
| `string-upcase` / `string-downcase` | `upcase` / `downcase` |
| `read` (depuis un flux) | `read-sexpr` |
| `pathname` | `to-pathname` |
| Versions à deux arguments de `floor` et consorts | `floor-div` `ceiling-div` `round-div` `truncate-div` |
| `mapcar` | `map` (ordre des arguments inversé ; section 4) |
| `length` (d'un vecteur) | `len` |
| `hash-table-count` | `count` / `size` |

La liste des fonctions se trouve dans [Fonctions intégrées](../reference/functions/README.md).

## 12. Autres choses qui n'existent pas

- `progv`, `symbol-function`, `symbol-value`
- `*readtable*` et `copy-readtable`, `readtable-case` (les macros de lecture elles-mêmes peuvent se définir avec
  `set-macro-character`)
- Les noms de chemin logiques et les noms de chemin avec jokers
- `input-stream-p` / `output-stream-p` (la direction d'un flux est décidée par son type)
