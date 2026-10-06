<!-- translated-from: docs/ja/tutorial/traits.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Traits

Un trait est une promesse : « ce type prend en charge ces opérations ». Les traits permettent à plusieurs types
de partager des opérations de même nom, si bien qu'une fonction qui les utilise n'a pas à être écrite une fois
par type. Ils fonctionnent presque exactement comme les traits de Rust. Ce chapitre suppose que vous avez lu
[Bases des types](types.md).

## 1. Définir et implémenter un trait

Définissons comme trait `Shape` les opérations qui renvoient l'aire et le nom d'une forme.

```lisp
(deftrait Shape ()
  (area ((self Self)) int)
  (name ((self Self)) string))
```

- Le `()` après le nom du trait est la liste des traits dont il hérite (section 4). Laissez-la vide s'il n'y en a
  pas.
- Chaque ligne déclare une méthode. `Self` désigne « le type qui implémente ce trait ».

Pour implémenter un trait pour un type, écrivez un `impl`.

```lisp
(defstruct circle (r int))
(defstruct rect (w int) (h int))

(impl Shape circle
  (area ((self Self)) int (* 3 self::r self::r))
  (name ((self Self)) string "circle"))

(impl Shape rect
  (area ((self Self)) int (* self::w self::h))
  (name ((self Self)) string "rect"))
```

Les méthodes implémentées s'appellent exactement comme des fonctions ordinaires.

```lisp
(area (circle::new 2))     ; => 12
(area (rect::new 3 4))     ; => 12
```

Omettre ne serait-ce qu'une des méthodes que le trait déclare est une erreur de type au niveau de l'`impl`.

## 2. Bornes de traits : « n'importe quel type qui implémente ce trait »

On peut poser une condition sur le paramètre de type d'une fonction générique avec `where`. C'est ce qu'on appelle
une **borne de trait**.

```lisp
(defun report<T> ((s T)) string
  (where (Shape T))
  (format false "~a: ~a" (name s) (area s)))

(report (circle::new 1))   ; => "circle: 3"
(report (rect::new 2 5))   ; => "rect: 10"
```

Grâce à `(where (Shape T))`, le corps peut utiliser `name` et `area` sur des valeurs de `T`. Sans la borne, on ne
sait rien de `T`, et on ne pourrait pas les appeler.

Passer un type qui n'implémente pas `Shape` est une erreur de type à l'appel.

```
(report 5)
error: ...: type error: report: type `int` does not implement trait `shape` required by `where` clause on type parameter `t`
```

Une fonction générique reçoit sa propre copie pour chaque type avec lequel elle est appelée. Aucun test de type ni
aucun branchement à l'exécution n'entre en jeu.

## 3. Implémentations par défaut

Si une méthode de trait a un corps, ce corps est utilisé quand un `impl` omet la méthode.

```lisp
(deftrait Describe ()
  (label ((self Self)) string)
  (describe ((self Self)) string
    (format false "<~a>" (label self))))

(impl Describe circle
  (label ((self Self)) string "a circle"))            ; describe est celle par défaut

(impl Describe rect
  (label ((self Self)) string "a rect")
  (describe ((self Self)) string "[rect]"))           ; celle écrite ici est prioritaire

(describe (circle::new 1))     ; => "<a circle>"
(describe (rect::new 1 1))     ; => "[rect]"
```

## 4. Implémenter les traits standard

La bibliothèque standard a aussi des traits. En implémenter un rend disponibles pour votre type les fonctions
standard qui l'utilisent.

| Trait | Méthodes à implémenter | Ce que cela permet |
|---|---|---|
| `Eq` | `equals` | `member`, `find`, `position`, le motif `(= expr)` de `match`, etc. |
| `Ord` | `less` | `less-equal`, `greater`, etc. `Ord` hérite de `Eq` |
| `print-object` | `print-object` | La façon dont `println` et consorts affichent les valeurs |
| `Iter` | `next` | `doiter`, `map`, `filter`, `sort`, etc. |
| `Error` | `message`, `source` | L'utilisation comme type d'erreur ([Gestion des erreurs](errors.md)) |

Implémentons `Eq` et `Ord` pour un type représentant une somme d'argent. Comme `Ord` hérite de `Eq`, l'`impl` de
`Eq` doit venir en premier.

```lisp
(defstruct money (yen int))

(impl Eq money
  (equals ((self Self) (other Self)) bool (= self::yen other::yen)))

(impl Ord money
  (less ((self Self) (other Self)) bool (< self::yen other::yen)))

(greater (money::new 5) (money::new 3))     ; => true (l'implémentation par défaut de Ord)
```

Implémenter `print-object` décide de la façon dont `println` affiche la valeur. L'argument `escape` vaut `true`
quand une forme relisible est demandée, comme avec `~s`.

```lisp
(impl print-object money
  (print-object ((self Self) (escape bool)) string
    (format false "~a yen" self::yen)))

(println "~a" (money::new 500))              ; 500 yen
```

Combiné à une borne de trait, on peut écrire une fonction qui marche pour tout type implémentant `Ord`.

```lisp
(defun largest<T> ((v Vector<T>)) Option<T>
  (where (Ord T))
  (foldl (iter v)
         (lambda ((best Option<T>) (x T)) Option<T>
           (match best
             ((some b) (if (less b x) (Option::some x) best))
             ((none) (Option::some x))))
         (Option::none)))
```

Avec un `Vector` contenant dans cet ordre des valeurs `money` de 300, 900 et 100, elle renvoie `(some 900 yen)`.

## 5. `:dyn` : traiter ensemble des valeurs de types différents

Tous les éléments d'un `Vector<T>` ont le même type ; des valeurs `circle` et `rect` ne peuvent donc pas aller dans
un même `Vector<circle>`. Pour traiter ensemble « quelque chose qui implémente `Shape` », utilisez le type
`:dyn Shape`.

```lisp
(defun total-area ((shapes Vector<:dyn Shape>)) int
  (let ((sum 0))
    (doiter (s (iter shapes))
      (setf sum (+ sum (area s))))
    sum))

(let ((shapes (the Vector<:dyn Shape> (Vector::new))))
  (push shapes (circle::new 1))
  (push shapes (rect::new 2 3))
  (doiter (s (iter shapes))
    (println "~a -> ~a" (name s) (area s)))
  (println "total ~a" (total-area shapes)))
```

```
circle -> 3
rect -> 6
total 9
```

- Une valeur `circle` ou `rect` placée là où un `:dyn Shape` est attendu est convertie automatiquement.
- Le type dont l'`area` est exécutée par l'appel `(area s)` est décidé à l'exécution par le type de ce que
  contient `s`.
- Placer une valeur dont le type n'implémente pas `Shape` là où un `:dyn Shape` est attendu est une erreur de
  type.

Choisir entre les bornes de traits de la section 2 et `:dyn` :

| | Borne de trait (`where`) | `:dyn Trait` |
|---|---|---|
| Quand la méthode appelée est décidée | Avant l'exécution | À l'exécution |
| Mélanger des types dans un même `Vector` | Impossible | Possible |
| Types utilisables | Sans restriction | Structures, énumérations, `int`, `string`, `f64` et d'autres (pas `bool`, `char`, `symbol`, `i32` et semblables) |

La liste exacte des types utilisables se trouve dans
[Référence de la syntaxe 3.9](../reference/syntax.md#39-deftrait--impl--traits).

Certains traits ne peuvent pas s'utiliser avec `:dyn` : ceux dont les méthodes utilisent `Self` pour un argument
autre que `self` ou pour la valeur de retour (comme `equals` dans `Eq`). Comme le type n'est connu qu'à
l'exécution, il n'y a aucun moyen de produire « une valeur du même type ».

## 6. Restrictions

- Gardez la définition d'un trait, ses `impl` et le code qui l'utilise via `:dyn` dans un même module (fichier).
  Un trait ne peut pas encore être rendu visible depuis d'autres modules.
- Les types et les traits partagent un même espace de noms. Dans un même module, un type et un trait ne peuvent
  pas porter le même nom.

## 7. Que lire ensuite

- [Macros](macros.md) : définir sa propre syntaxe
- [Référence de la syntaxe 3.9](../reference/syntax.md#39-deftrait--impl--traits) : implémentations
  génériques globales, types associés, etc.
- [Traits standard](../reference/functions/traits.md) : la liste des traits de la bibliothèque standard
