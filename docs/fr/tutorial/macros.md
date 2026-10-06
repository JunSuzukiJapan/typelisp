<!-- translated-from: docs/ja/tutorial/macros.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Macros

Une macro est une fonction qui prend un programme et renvoie un programme. Les macros permettent de créer une
syntaxe nouvelle que les fonctions ne peuvent pas exprimer. Les macros de typelisp fonctionnent comme le
`defmacro` de Common Lisp. Ce chapitre suppose que vous avez lu « Listes (S-expressions) » dans
[Premiers pas](intro.md).

## 1. En quoi les macros diffèrent des fonctions

Une fonction reçoit ses arguments **après leur évaluation**. Une macro les reçoit **sous forme d'expressions,
avant évaluation** (comme données S-expression), construit une autre expression et la renvoie. L'expression
renvoyée remplace l'appel de macro, et c'est seulement alors qu'elle est vérifiée et exécutée. Ce remplacement
s'appelle l'**expansion**.

Par exemple, une syntaxe comme `unless` ne peut pas s'écrire comme une fonction. Sous forme de fonction, le corps
serait évalué d'abord, même quand la condition est vraie.

## 2. `defmacro` et quasiquote

Créons `my-unless`, qui n'exécute son corps que si la condition est fausse.

```lisp
(defmacro my-unless (test &rest body)
  `(if ,test () (progn ,@body ())))
```

- On n'écrit pas de types pour les arguments d'une macro. Chaque argument est une donnée S-expression.
- `&rest body` reçoit les arguments restants réunis en une seule liste.
- Une expression commençant par `` ` `` (quasiquote) est construite comme donnée, telle qu'écrite. À
  l'intérieur :
  - `,test` insère le contenu de la variable `test` à cet endroit.
  - `,@body` insère à cet endroit les éléments de la liste `body`.

```lisp
(my-unless (> 1 2)
  (println "one")
  (println "two"))
;; one
;; two
```

On peut vérifier l'expansion avec `macroexpand-1`. Quand on écrit une macro, regarder d'abord son expansion est
le moyen le plus rapide d'avancer.

```
typl> (macroexpand-1 '(my-unless (> 1 2) (println "one")))
(ok (if (> 1 2) () (progn (println "one") ())))
```

## 3. Les expansions sont elles aussi vérifiées

L'expression que renvoie une macro est vérifiée comme n'importe quelle expression écrite à la main.

```
typl> (defmacro add-a (x) `(+ ,x "a"))
typl> (add-a 1)
error: <stdin>:1:1: type error: type mismatch: expected `int`, found `string`
```

L'erreur est signalée à l'endroit où la macro a été appelée.

Les règles selon lesquelles la branche « sinon » d'un `if` ne peut pas être omise et les deux branches d'un `if`
doivent avoir le même type s'appliquent telles quelles aux expansions. Le `my-unless` ci-dessus se termine par
`(progn ,@body ())` afin que, quel que soit le type de la dernière expression du corps, les deux branches du `if`
aient le type `()`.

## 4. Conflits de noms et `gensym`

Une macro naïve qui échange les valeurs de deux variables ressemble à ceci :

```lisp
(defmacro swap-bad (a b)
  `(let ((tmp ,a))
     (setf ,a ,b)
     (setf ,b tmp)))
```

Elle fonctionne la plupart du temps, mais se casse quand la variable de l'appelant s'appelle justement `tmp`.

```lisp
(let ((tmp 1) (other 2))
  (swap-bad tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=1 other=2   (pas échangées)
```

L'expansion est `(let ((tmp tmp)) (setf tmp other) (setf other tmp))`, et le `tmp` créé par la macro masque le
`tmp` de l'appelant.

Pour l'éviter, créez avec `gensym` les noms des variables utilisées à l'intérieur d'une macro. `gensym` renvoie un
nouveau symbole qu'on ne peut écrire nulle part dans un programme.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))
```

```lisp
(let ((tmp 1) (other 2))
  (swap tmp other)
  (println "tmp=~a other=~a" tmp other))
;; tmp=2 other=1
```

Comme en Common Lisp, les macros de typelisp n'empêchent pas automatiquement les conflits de noms (elles ne sont
pas hygiéniques). Retenez : **utilisez `gensym` pour les liaisons que crée une macro.**

De la même façon, une macro qui répète son corps un nombre donné de fois peut s'écrire ainsi :

```lisp
(defmacro repeat (n &rest body)
  (let ((i (gensym "i")))
    `(dotimes (,i ,n) ,@body)))

(repeat 3 (println "hi"))
```

## 5. Une expansion qui dépend des arguments

Le corps d'une macro est du code typelisp ordinaire ; il peut donc examiner ses arguments avec `if` ou `match` et
construire une expansion différente. Les arguments sont des données S-expression (`Option<Sexpr>`), et la liste
vide est `none`.

Créons `my-and`, qui renvoie `true` si toutes ses conditions sont vraies.

```lisp
(defmacro my-and (&rest forms)
  (match forms
    ((none) 'true)                                    ; pas d'arguments
    ((cons f more)
     (if (sexpr-null more)
         f                                            ; un seul
         `(if ,f (my-and ,@more) false)))             ; deux ou plus
    (_ (panic "my-and: not a list"))))

(my-and (> 2 1) (> 3 2))      ; => true
(my-and (> 2 1) (> 1 3))      ; => false
```

- `(cons f more)` est un motif qui place la tête d'une liste dans `f` et le reste dans `more`.
- `sexpr-null` teste si une donnée S-expression est la liste vide.
- La branche finale `_` est nécessaire parce que les données S-expression ont d'autres formes que les listes
  (nombres, chaînes, etc.), et `match` exige qu'elles soient couvertes aussi. Un argument `&rest` est toujours une
  liste ; cette branche ne s'exécute donc jamais réellement.
- Une macro peut s'appeler elle-même dans son expansion. L'expansion se répète jusqu'à ce qu'il ne reste plus
  d'appels de macro.

## 6. Arguments facultatifs

`&optional` reçoit des arguments qui peuvent être omis. On peut donner des valeurs par défaut.

```lisp
(defmacro inc-by (place &optional (n 1))
  `(setf ,place (+ ,place ,n)))

(let ((c 10))
  (inc-by c)
  (inc-by c 5)
  c)                            ; => 16
```

`&key` reçoit des arguments mots-clés
([Référence de la syntaxe 3.14](../reference/syntax.md#314-defmacro--définitions-de-macros)).

## 7. `macrolet` : des macros pour un seul endroit

Une macro utilisée uniquement dans une expression peut se définir avec `macrolet`. Elle n'est pas visible à
l'extérieur.

```lisp
(macrolet ((sq (x) `(* ,x ,x)))
  (+ (sq 3) (sq 4)))            ; => 25
```

## 8. Points à garder en tête

- **Une macro ne peut être appelée qu'après sa définition.** Comme pour les fonctions, définissez-la vers le haut
  du fichier.
- Rendez une macro disponible pour d'autres modules avec `(pub defmacro ...)`.
- Une grande partie de la syntaxe standard, dont `when`, `unless`, `cond`, `and`, `or` et `dotimes`, est définie
  par des macros. On peut voir ce qu'il y a dedans avec `(macroexpand '(when true 1))`.
- Si quelque chose peut s'écrire comme une fonction, écrivez-le comme une fonction. Les macros ne peuvent pas être
  passées comme valeurs, et il faut lire leur expansion pour comprendre ce qu'elles font.

## 9. Que lire ensuite

- [Gestion des erreurs](errors.md) : `Result`, `panic`, `catch` / `throw`
- [Fonctions liées aux macros](../reference/functions/system.md#8-macros) : `gensym`, `macroexpand`, etc.
