<!-- translated-from: docs/ja/guide/modules.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
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
  (tag string))                        ; un champ sans pub ne peut pas être lu de l'extérieur

(defun square ((n i32)) i32 (* n n))   ; une fonction sans pub ne peut pas non plus être appelée de l'extérieur

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
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

- Les fonctions, structures, énumérations, variables globales, macros et méthodes ne sont visibles depuis d'autres
  modules que si elles portent `pub`. Placez `pub` juste avant la définition, comme dans `(pub defun ...)`.
- Pour les structures, **rendre le type public et rendre les champs publics sont deux choses distinctes**.
  `(pub defstruct point ...)` rend le type visible, et seuls les champs écrits `(pub x i32)` peuvent être lus et
  écrits de l'extérieur.
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
(use a::f b::g)             ; on peut en écrire plusieurs ensemble
```

- **`use` ne prend effet que pour les formes qui le suivent.** Placez-le en tête du fichier. Écrire
  `geometry::dist2` au-dessus du `use` donne `unresolved path`.
- Écrire le chemin complet `geometry::dist2` sans avoir importé le module avec `use` ne se résout pas non plus.
  Seul `use` provoque le chargement d'un fichier.
- Importer avec `use` un nom dont la forme nue est déjà prise donne un avertissement. Quand vous voulez quand même
  l'importer, utilisez `shadowing-import`.
- Un module à l'intérieur d'un répertoire s'écrit `(use geo::shapes)`, et ensuite on y fait référence par sa
  dernière partie (`shapes::...`).

### Appeler des méthodes de trait

Les méthodes implémentées dans un `impl` **appartiennent au type**, pas aux fonctions du module ; on les appelle
donc sans le nom du module.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, pas core::area
```

Les méthodes à l'intérieur d'un `impl` sont toujours publiques, même sans `pub`.

Un trait lui-même ne peut pas être rendu public pour d'autres modules. Gardez la définition d'un trait, ses `impl`
et le code qui l'utilise via `:dyn` dans un seul module.

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
