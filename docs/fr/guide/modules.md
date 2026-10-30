<!-- translated-from: docs/ja/guide/modules.md @ b4f23852eb3f4b11e3fd96c05da870bdd383e84f -->
# Modules et découpage en fichiers

Ce guide explique comment assembler un programme composé de plusieurs fichiers. Les règles détaillées se trouvent
dans les sections 3.10 à 3.13 de la
[Référence de la syntaxe](../reference/syntax.md#310-module--use--espaces-de-noms).

## 1. Un fichier est un module

En typelisp, **un fichier est un module à lui seul**. Le chemin du fichier relatif à la racine des sources est le
chemin du module.

| Fichier | Module |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Il n'est pas nécessaire d'écrire une déclaration de module en tête du fichier.

## 2. Mettre en place un projet

Placez un fichier nommé `typelisp.toml` à la racine du projet. Il peut être vide.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Pour garder les sources sous `src/`, écrivez cette seule ligne dans `typelisp.toml` :

```toml
src = "src"
```

`typl` cherche `typelisp.toml` en partant du répertoire du fichier qu'il exécute et en remontant, et utilise
l'endroit où il le trouve comme racine des sources. S'il n'en trouve pas, le répertoire du fichier exécuté est la
racine (dans la REPL, le répertoire courant).

## 3. Rendre des définitions publiques et les utiliser

`geometry.typl` :

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; un champ sans pub ne peut être ni lu ni écrit de l'extérieur

(defun square ((n i32)) i32 (* n n))   ; une fonction sans pub ne peut pas non plus être appelée de l'extérieur

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(impl point
  (pub new ((x i32) (y i32) (tag string)) point
    (make-instance 'point :x x :y y :tag tag))
  (pub origin () point (point::new 0 0 "o")))
```

`main.typl` :

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl` est chargé à l'endroit où `(use geometry)` est écrit. Il n'est pas nécessaire de le charger au
préalable.

### Ce qui est rendu public

- Les fonctions, structures, énumérations, traits, variables globales et macros ne sont visibles depuis les autres
  modules que si elles portent `pub`. Placez `pub` juste avant la définition, comme dans `(pub defun ...)`.
- Parmi les méthodes écrites dans un `impl`, seules celles écrites `(pub new ...)`, élément par élément,
  sont utilisables de l'extérieur.
- Pour les structures, **rendre le type public et rendre les champs publics sont deux choses distinctes**.
  `(pub defstruct point ...)` rend le type visible, et seuls les champs écrits `(pub x i32)` peuvent être
  lus et écrits de l'extérieur.
- De même, dans les autres modules, seuls les champs `pub` peuvent être donnés à `make-instance`. Le
  `point` ci-dessus a un `tag` qui n'est pas `pub` : les autres modules ne peuvent donc pas le créer avec
  `make-instance` et utilisent `point::new`.
- Utiliser de l'extérieur un nom qui n'est pas public donne une erreur « impossible à résoudre » comme
  `unresolved path: geometry::square`. C'est le même message que pour un nom mal orthographié ; si
  l'orthographe est correcte et que le nom ne se résout toujours pas, soupçonnez un `pub` manquant.

La liste des définitions qui peuvent porter `pub` se trouve dans
[Référence de la syntaxe 3.13](../reference/syntax.md#313-pub--visibilité).

## 4. Comment écrire `use`

```lisp
(use geometry)              ; importer un module ; écrire geometry::dist2 pour l'utiliser
(use geometry::dist2)       ; importer une fonction ; l'utiliser par le nom nu dist2
(use geometry::point)       ; importer un type ; écrire point::new, point::origin, et point dans les annotations de type
(use geometry::point::new)  ; importer une méthode statique d'un type ; l'utiliser par le nom nu new
(use geometry::shape::*)    ; rendre utilisables par leurs noms nus tous les constructeurs et méthodes statiques publiques d'un type
(use a::f b::g)             ; on peut en écrire plusieurs ensemble
```

- **`use` ne prend effet que pour les formes qui le suivent.** Placez-le en tête du fichier. Écrire
  `geometry::dist2` au-dessus du `use` donne `unresolved path`.
- Écrire le chemin complet `geometry::dist2` sans avoir importé le module avec `use` ne se résout pas non plus.
  Seul `use` provoque le chargement d'un fichier.
- Importer avec `use` un nom dont la forme nue est déjà prise donne un avertissement. Quand vous voulez quand même
  l'importer, utilisez `shadowing-import`. Pour les constructeurs et les méthodes statiques d'un type, c'est une erreur et non un avertissement.
- `(use geometry::point)` n'importe que le nom du type. Pour écrire `point::new` sous la forme `new`, écrivez
  `(use geometry::point::new)` ou `(use geometry::point::*)`.
- Un module à l'intérieur d'un répertoire s'écrit `(use geo::shapes)`, et ensuite on y fait référence par sa
  dernière partie (`shapes::...`).

### Utiliser un nom offert par la bibliothèque standard

Définir une fonction ou autre chose sous un nom offert par la bibliothèque standard, comme `length`
ou `first`, est une erreur.

```
type error: defun: `length` is a name the standard library offers every module; write `(shadow length)` before defining it here
```

Quand vous décidez que le module doit utiliser votre propre définition, écrivez `shadow` avant la
définition.

```lisp
(shadow first)

(defun first ((a int) (b int)) int a)

(println "~a" (first 10 20))                                          ; le first de ce module => 10
(println "~a" (std::seq::first (iter (the Vector<int> #(1 2 3)))))    ; le first de la bibliothèque standard => (some 1)
```

Après `shadow`, écrire `first` dans ce module désigne votre propre définition. Pour utiliser le
`first` de la bibliothèque standard, écrivez son chemin, `std::seq::first`. `shadow` n'agit que dans
le module où il est écrit ; dans les autres modules, `first` reste celui de la bibliothèque
standard. Les noms des formes spéciales comme `if` et `let` ne peuvent pas être `shadow`, ni servir
à des définitions.

### Appeler des méthodes de trait

Les méthodes implémentées dans un `impl` **appartiennent au type**, pas aux fonctions du module ; on les appelle
donc sans le nom du module.

```lisp
(use shapes::core)
(println "~a" (area (make-instance 'core::square :side 4)))    ; area, pas core::area
```

Les méthodes de l'`impl` d'un trait sont toujours publiques, même sans `pub`. Parmi les méthodes d'un
`impl` inhérent (`(impl point ...)`), seules celles qui portent `pub` sont publiques.

Les traits aussi ne sont visibles depuis les autres modules que s'ils sont écrits `(pub deftrait ...)`.
Dans les autres modules, on écrit un trait avec un chemin comme `shapes::Shape` ou on l'importe avec
`use`, et on le met ainsi dans un `impl`.

## 5. Découper les espaces de noms à l'intérieur d'un fichier

Pour découper davantage un espace de noms dans un même fichier, utilisez `module`. Il est imbriqué dans le module
propre du fichier.

```lisp
;; dans main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Pour placer tout le reste du fichier dans un espace de noms, on peut écrire `(in-module util)` au lieu de
l'envelopper de parenthèses.

## 6. Restrictions sur les dépendances

- **Les cycles ne sont pas autorisés.** Si `a.typl` fait `(use b)` et que `b.typl` fait `(use a)`, le résultat est
  l'erreur `circular module dependency: a -> b -> a`. Déplacez les définitions dont les deux ont besoin dans un
  troisième module.
- **Ni les types ni les fonctions ne peuvent être référencés avant leur définition**, même dans le même fichier.
  Pour des fonctions mutuellement récursives, déclarez d'abord l'une d'elles avec `defsignature`
  ([Référence de la syntaxe 3.2](../reference/syntax.md#32-defsignature--déclarations-anticipées)).

## 7. Ordre d'exécution

Lancer `typl main.typl` se déroule dans cet ordre :

1. `main.typl` et tous les fichiers importés depuis lui avec `use` sont lus et vérifiés. **S'il y a une erreur de
   type quelque part, rien ne s'exécute.**
2. Les expressions de niveau supérieur des modules importés s'exécutent avant celles des modules qui les
   importent.
3. Les expressions de niveau supérieur de `main.typl` s'exécutent de haut en bas.

Si vous réunissez le point d'entrée du programme dans une fonction `main` et appelez `(main)` à la fin du fichier,
le même fichier peut aussi servir à la
[compilation AOT](compile.md#3-créer-un-exécutable-par-compilation-aot).

## 8. Différence avec `load`

`(load "path")`, comme le `load` de Common Lisp, lit le contenu d'un fichier **tel quel dans l'espace de noms
courant**. Il ne l'enveloppe pas dans un module, et `pub` ne joue aucun rôle. Utilisez-le pour lire un fichier de
réglages ou recharger un fichier local dans la REPL. Pour découper un programme en parties, utilisez `use`.
