<!-- translated-from: docs/ja/reference/functions/traits.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Traits standard

Les traits d'itération, de comparaison, d'arithmétique et d'ajout et de retrait d'éléments. Les autres traits standard se trouvent dans leurs propres
chapitres : `Hash` ([HashTable](collections.md#4-hashtablekv)), `Error`
([Types d'erreur](option-result.md#3-types-derreur-et-le-trait-error)), `print-object`
([Affichage](printing.md#5-print-object-représentation-imprimée-par-type)), ainsi que les traits de flux et
`Pathish` ([Flux et fichiers](streams-files.md)). Quels types implémentent lesquels se trouve dans
[Types](../types.md). La façon de définir des traits se trouve dans la
[Référence de la syntaxe](../syntax.md#39-deftrait--impl--traits).

## 1. Le trait `Iter` et l'itération

```lisp
(deftrait Iter ()
  (:type Item)
  (defmethod next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implémentent `Iter` respectivement via `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (on obtient l'itérateur avec `(iter collection)`). `Chan<T>` est lui-même un
`Iter` (`recv` joue le rôle de `next` ; [Canaux](concurrency.md#2-chant--canaux)). Les listes `Sexpr`
n'implémentent pas `Iter` (leurs types d'éléments ne sont pas uniformes). Si vous implémentez `Iter` pour votre
propre type, il peut être parcouru tel quel avec `doiter` et passé aux
[fonctions de séquence](sequences.md#4-fonctions-de-séquence-sur-iter).

## 2. `Equal` / `Ord` (comparaison)

`Equal` est le trait qu'utilise `equal` ; il correspond au `PartialEq` de Rust. `Ord` correspond au
`PartialOrd` de Rust. On les utilise dans les bornes `:where` des fonctions génériques pour exiger que le
type des éléments puisse être comparé.

```lisp
(deftrait Equal ()
  (defmethod equal (self (other Self)) bool))                ; à implémenter
(deftrait Ord (Equal)                                     ; hérite de Equal
  (defmethod less (self (other Self)) bool)                  ; à implémenter
  (defmethod less-equal (self (other Self)) bool (not (less other self)))
  (defmethod greater (self (other Self)) bool (less other self))
  (defmethod greater-equal (self (other Self)) bool (not (less self other))))
```

Pour implémenter `Equal`, on écrit `equal`, et pour `Ord` seulement `less`. Les implémentations par
défaut complètent le reste de `Ord`. `Ord` hérite de `Equal`, donc il faut `impl Equal X` avant `impl Ord X`.

`(equal a b)` appelle l'implémentation de `Equal` du type de `a`. L'écrire pour un type qui n'implémente
pas `Equal` est une erreur de type. Pour comparer par identité, on utilise `eq`/`eql`, et par structure
`equalp`. Ces trois-là s'appliquent à une valeur de n'importe quel type.

Chaque méthode de trait peut être appelée directement comme fonction (dans une borne `where (Equal A)`/
`(Ord A)`, ou sur un type concret qui l'implémente) :

| Nom | Forme | Type | Description |
|---|---|---|---|
| `equal` | `(equal a b)` | `(A,A)→bool` where `Equal A` | S'ils sont égaux (le `==` de Rust) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Equal` est implémenté pour : tous les types numériques (`i8` à `u32` / `f32` / `f64` / `int` / `ratio`),
`bool` `char` `string` `symbol` `complex`, `Sexpr`, `cons-cell<A,B>` et les tuples (élément par élément,
quand les éléments sont `Equal`), ainsi que `Option<T>` et `Result<T,E>` (quand ils sont de la même
variante et que leur contenu est `equal`). Pour les types intégrés, la méthode intégrée `equal` du type
est telle quelle l'implémentation de `Equal`. Les nombres se comparent avec `=`, les chaînes par contenu en
respectant la casse. `Ord` est implémenté pour : tous les types numériques, `char` `string` et
`cons-cell<A,B>` (lexicographiquement, quand les éléments sont `Ord`).

Le `equal` d'une liste (`Sexpr`) est le `equal` de CL : les conses se comparent par structure, les chaînes
par contenu et tout autre atome par `eql`. Cette comparaison parcourt les valeurs à l'exécution et
n'appelle aucune méthode. Une structure dans une liste se compare donc par identité, comme en CL, quel que
soit l'`impl Equal` de son type.

Les noms des méthodes de `Ord` ne chevauchent pas les opérateurs intégrés (`< <= > >=`) ni `lt`, car on ne
peut pas redéfinir l'intégré et chaque implémentation leur délègue. Les opérateurs de comparaison scalaires
eux-mêmes sont des méthodes intégrées par type de récepteur ([Nombres](numbers.md),
[Chaînes et caractères](collections.md)). Dans une borne, l'opérateur écrit se lit comme la méthode du trait
(chapitre 3). `=` devient `equal`, et `/=` devient `(not (equal a b))`.

## 3. Traits arithmétiques (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Une couche permettant au code générique d'exiger « un type qu'on peut additionner ». **L'arithmétique sur des types
concrets utilise les opérateurs intégrés** ([Nombres](numbers.md)) et ne passe pas par cette couche.

```lisp
(deftrait Add () (defmethod add (self (other Self)) Self))
(deftrait Sub () (defmethod sub (self (other Self)) Self))
(deftrait Mul () (defmethod mul (self (other Self)) Self))
(deftrait Div () (:type Output) (defmethod div (self (other Self)) Output))
(deftrait Rem () (defmethod remainder (self (other Self)) Self))
(deftrait Bits ()
  (defmethod bit-and (self (other Self)) Self)
  (defmethod bit-or  (self (other Self)) Self)
  (defmethod bit-xor (self (other Self)) Self)
  (defmethod bit-not (self) Self)
  (defmethod shift (self (count int)) Self))                 ; la distance est toujours int (comme pour ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; aucune méthode ; une combinaison de cinq
```

**À l'intérieur d'une borne, on peut écrire des opérateurs.** Quand le receveur est une variable de type liée par
`:where`, les opérateurs sont lus comme des méthodes de trait (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`,
`rem`→`remainder`, `logand`→`bit-and`, `=`→`equal`, `<`→`less` …) :

```lisp
(defun sum3<T> ((a T) (b T) (c T)) T (:where (Number T))
  (+ a b c))                                              ; = (add (add a b) c)
```

La méthode de trait ne s'appelle pas `+` parce que `+` est le nom d'une méthode intégrée et qu'`impl` refuse de la
redéfinir (`cannot redefine built-in method`). Il n'y a pas de `Neg` : `(- x)` s'expanse en `(- (- x x) x)` ;
`Sub` suffit donc.

Implémentés pour : `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Number` sur tous les types numériques (sauf `complex`), et
`Bits` sur tous les types entiers et `int`.


**`Div` nomme son type de résultat par le type associé `Output`.** Comme en CL, `/` sur deux entiers
est un `rational` : l'`Output` de `int` est donc `rational`, et celui de tout autre type numérique est
le type lui-même. Comme `Output` diffère selon le type, `Number` n'inclut pas `Div` (une liste de
supertraits doit fixer chaque type associé). Une fonction générique qui divise reçoit le type du
résultat avec `(:where (Div T (Output R)))`, ou exige une division fermée dans `T` avec
`(:where (Div T (Output T)))` (un `int` ne peut alors pas être passé).

## 4. Le trait `Push` (ajouter des éléments)

```lisp
(deftrait Push ()
  (:type Item)
  (defmethod push (self (x Item)) ()))
```

Une collection à laquelle on peut ajouter des éléments un par un. Une fonction générique demande «
un type auquel on peut ajouter `T` » avec `(:where (Push C (Item T)))`, et ajoute à n'importe
laquelle de ces collections avec le même `(push c x)`.

| Type | Ce que fait `push` |
|---|---|
| `Vector<T>` | Ajoute à la fin |
| `Deque<T>` | Ajoute à la fin (`push-back`) |
| `Array<T>` | Ajoute au pointeur de remplissage (`push-extend`). Panic sans pointeur de remplissage |
| `HashSet<T>` | Le met dans l'ensemble (`insert`) |

```lisp
(use std::collections::Deque)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))

(let ((v (the Vector<int> (Vector::new)))
      (d (the Deque<int> (Deque::new))))
  (push-all v (the Vector<int> #(1 2)))
  (push-all d (the Vector<int> #(3 4)))
  (println "~a ~a" v d))                                  ; #(1 2) #<deque 3 4>
```

Un type à vous qui implémente `Push` peut être passé aux mêmes fonctions génériques. Il peut aussi
être un objet trait, comme dans `:dyn Push<int>`.

## 5. Le trait `Pop` (retirer des éléments)

```lisp
(deftrait Pop ()
  (:type Item)
  (defmethod pop (self) Option<Item>))
```

Une collection dont on peut retirer l'élément ajouté en dernier par `push`. `none` quand elle est
vide. Une fonction générique la demande avec `(:where (Pop C (Item T)))`.

| Type | Ce que fait `pop` |
|---|---|
| `Vector<T>` | Retire le dernier élément |
| `Deque<T>` | Retire le dernier élément (`pop-back`) |
| `Array<T>` | Retire l'élément juste avant le pointeur de remplissage. Panic sans pointeur de remplissage |

Un `HashSet<T>` n'a pas de « dernier élément » : c'est un `Push`, mais pas un `Pop`.

```lisp
(use std::collections::Deque)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))

(let ((v (the Vector<int> #(1 2 3)))
      (d (the Deque<int> (Deque::new))))
  (push d 1) (push d 2)
  (println "~a ~a" (pop-all v) (pop-all d)))              ; #(3 2 1) #(2 1)
```

Comme pour `Push`, un type à vous peut l'implémenter, et il peut être un objet trait, comme dans
`:dyn Pop<int>`.
