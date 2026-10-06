<!-- translated-from: docs/ja/reference/functions/README.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Fonctions intégrées

La liste des fonctions intégrées, des méthodes et de la bibliothèque standard. Pour la syntaxe (formes spéciales et
manière de définir les choses), voir la [Référence de la syntaxe](../syntax.md) ; pour la liste des types, voir
[Types](../types.md).

## Formes d'appel

Il existe trois formes d'appel.

- Fonctions libres : `(name args...)`
- Méthodes d'instance : `(name receiver args...)` (résolues selon le type statique du premier argument)
- Méthodes statiques (fonctions associées) : `(Type::name args...)`

Chaque type peut avoir sa propre méthode de même nom. `(+ a b)` appelle le `+` du type de `a`.

## Lire les tableaux

Les tableaux de chaque chapitre ont les colonnes « nom, forme, type, description ». La colonne type s'écrit
`(type-d'argument,...)→type-de-retour`.

- Une seule lettre majuscule comme `T`, `A` ou `B` est une variable de type.
- Une mention comme `where Eq A` est une borne de trait que la variable de type doit satisfaire.
- `Iter<A>` signifie « n'importe quelle implémentation de `Iter` dont l'`Item` est `A` ».
- Les arguments marqués `&optional` / `&key` peuvent être omis.

## Chapitres

| Fichier | Contenu |
|---|---|
| [numbers.md](numbers.md) | Entiers, nombres à virgule flottante, rationnels, nombres complexes, booléens, opérations sur les bits, nombres aléatoires |
| [sequences.md](sequences.md) | La paire `cons-cell`, les données S-expression `Sexpr`, les symboles, les fonctions de séquence, les fonctions d'ordre supérieur |
| [collections.md](collections.md) | Chaînes, caractères, `Vector`, `HashTable`, `Array`, `BitVector` |
| [option-result.md](option-result.md) | `Option`, `Result`, les types d'erreur et le trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, les traits arithmétiques |
| [printing.md](printing.md) | `print`/`println`/`format`, le pretty printer, `print-object`, les variables de contrôle de l'affichage |
| [format.md](format.md) | Directives de format |
| [streams-files.md](streams-files.md) | Flux, opérations sur les fichiers, noms de chemin, readtable |
| [concurrency.md](concurrency.md) | Tâches, canaux, `WaitGroup`, `Mutex`, `Thread` |
| [network.md](network.md) | TCP, TLS, sockets du domaine Unix, UDP |
| [system.md](system.md) | Temps, environnement d'exécution, outils de l'implémentation, `read`/`eval`, docstrings, fonctions liées aux macros |
