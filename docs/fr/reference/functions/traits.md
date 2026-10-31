<!-- translated-from: docs/ja/reference/functions/traits.md @ 1ba918af8af98a319b34010b9dc04df8c4d321e0 -->
# Traits standard

Les traits d'itération, de comparaison, d'arithmétique et d'ajout d'éléments. Les autres traits standard se trouvent dans leurs propres
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
  (next (self) Option<Item>))
```

`Vector<T>`/`HashTable<K,V>`/`Array<T>` implémentent `Iter` respectivement via `vector-iter<T>`/
`hashtable-iter<K,V>`/`array-iter<T>` (on obtient l'itérateur avec `(iter collection)`). `Chan<T>` est lui-même un
`Iter` (`recv` joue le rôle de `next` ; [Canaux](concurrency.md#2-chant--canaux)). Les listes `Sexpr`
n'implémentent pas `Iter` (leurs types d'éléments ne sont pas uniformes). Si vous implémentez `Iter` pour votre
propre type, il peut être parcouru tel quel avec `doiter` et passé aux
[fonctions de séquence](sequences.md#4-fonctions-de-séquence-sur-iter).

## 2. `Eq` / `Ord` (comparaison)

Ils correspondent aux `PartialEq`/`PartialOrd` de Rust (nommés `Eq`/`Ord`). On les utilise dans les bornes `:where`
des fonctions génériques pour exiger que les types d'éléments soient comparables (`sort`/`member`/`assoc`, etc.).

```lisp
(deftrait Eq ()
  (equals (self (other Self)) bool)                ; doit être implémentée
  (not-equals (self (other Self)) bool             ; implémentation par défaut
    (not (equals self other))))
(deftrait Ord (Eq)                                        ; hérite de Eq
  (less (self (other Self)) bool)                  ; doit être implémentée
  (less-equal (self (other Self)) bool (not (less other self)))
  (greater (self (other Self)) bool (less other self))
  (greater-equal (self (other Self)) bool (not (less self other))))
```

Pour implémenter `Eq`, on écrit seulement `equals`, et pour `Ord` seulement `less`. Les implémentations par défaut
complètent le reste. `Ord` hérite de `Eq` ; `impl Eq X` est donc nécessaire avant `impl Ord X`.

Chaque méthode de trait peut être appelée telle quelle comme une fonction (à l'intérieur d'une borne
`where (Eq A)`/`(Ord A)`, ou sur un type concret qui l'implémente) :

| Nom | Forme | Type | Description |
|---|---|---|---|
| `equals` | `(equals a b)` | `(A,A)→bool` where `Eq A` | S'ils sont égaux (le `==` de Rust) |
| `not-equals` | `(not-equals a b)` | `(A,A)→bool` where `Eq A` | S'ils sont différents (`!=`) |
| `less` | `(less a b)` | `(A,A)→bool` where `Ord A` | `a < b` |
| `less-equal` | `(less-equal a b)` | `(A,A)→bool` where `Ord A` | `a <= b` |
| `greater` | `(greater a b)` | `(A,A)→bool` where `Ord A` | `a > b` |
| `greater-equal` | `(greater-equal a b)` | `(A,A)→bool` where `Ord A` | `a >= b` |

`Eq` est implémenté pour : tous les types numériques (`i8` à `u32` / `f32` / `f64` / `int` / `ratio`), `bool`
`char` `string` `symbol` `complex`, `Sexpr` (`eq`, c'est-à-dire l'identité ; utilisé par les motifs de valeur de
`match`) et `cons-cell<A,B>` (récursivement, quand les éléments sont `Eq`). `Ord` est implémenté pour : tous les
types numériques, `char` `string` et `cons-cell<A,B>` (lexicographiquement, quand les éléments sont `Ord`).

Les noms des méthodes ne recouvrent pas les opérateurs intégrés (`= /= < <= > >=`) ni `eq`/`lt`, parce que les
intégrés ne peuvent pas être redéfinis, et chaque implémentation leur délègue. Les opérateurs de comparaison
scalaires eux-mêmes sont des méthodes intégrées de chaque type receveur ([Nombres](numbers.md),
[Chaînes et caractères](collections.md)). À l'intérieur d'une borne, les opérateurs écrits sont lus comme les
méthodes du trait (chapitre 3).

## 3. Traits arithmétiques (`Add` / `Sub` / `Mul` / `Div` / `Rem` / `Bits` / `Number`)

Une couche permettant au code générique d'exiger « un type qu'on peut additionner ». **L'arithmétique sur des types
concrets utilise les opérateurs intégrés** ([Nombres](numbers.md)) et ne passe pas par cette couche.

```lisp
(deftrait Add () (add (self (other Self)) Self))
(deftrait Sub () (sub (self (other Self)) Self))
(deftrait Mul () (mul (self (other Self)) Self))
(deftrait Div () (:type Output) (div (self (other Self)) Output))
(deftrait Rem () (remainder (self (other Self)) Self))
(deftrait Bits ()
  (bit-and (self (other Self)) Self)
  (bit-or  (self (other Self)) Self)
  (bit-xor (self (other Self)) Self)
  (bit-not (self) Self)
  (shift (self (count int)) Self))                 ; la distance est toujours int (comme pour ash)
(deftrait Number (Add Sub Mul Rem Ord))                   ; aucune méthode ; une combinaison de cinq
```

**À l'intérieur d'une borne, on peut écrire des opérateurs.** Quand le receveur est une variable de type liée par
`:where`, les opérateurs sont lus comme des méthodes de trait (`+`→`add`, `-`→`sub`, `*`→`mul`, `/`→`div`,
`rem`→`remainder`, `logand`→`bit-and`, `=`→`equals`, `<`→`less` …) :

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
  (push (self (x Item)) ()))
```

Une collection à laquelle on peut ajouter des éléments un par un. Une fonction générique demande «
un type auquel on peut ajouter `T` » avec `(:where (Push C (Item T)))`, et ajoute à n'importe
laquelle de ces collections avec le même `(push c x)`.

| Type | Ce que fait `push` |
|---|---|
| `Vector<T>` | Ajoute à la fin |
| `Deque<T>` | Ajoute à la fin (`push-back`) |
| `Array<T>` | Ajoute au pointeur de remplissage (`push-extend`). Panic sans pointeur de remplissage |
| `HashSet<T>` | Le met dans l'ensemble (`insert`). N'est un `Push` que si le type des éléments implémente `Hash` |

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
