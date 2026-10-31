<!-- translated-from: docs/ja/reference/functions/README.md @ 2d61c22ad717f8ad6dfd178f397927e87e1d8902 -->
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

## Noms qui demandent un `use`

Parmi les noms de la bibliothèque standard, ceux-ci s'utilisent dans tout module sans rien écrire
([référence de la syntaxe 3.10](../syntax.md#310-module--use--espaces-de-noms)) :

- les types primitifs, `Option`/`Result`/`Vector`, l'itération, les S-expressions et les listes, les
  fonctions de séquence, les macros de contrôle, la comparaison et les traits arithmétiques
- tout nom qui est aussi un nom du paquet `COMMON-LISP` de Common Lisp (`length`, `format`, `open`,
  etc.)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, les traits de flux `Stream`,
  `InputStream` et `OutputStream`, et `Hash`, qu'implémentent les clés d'une `HashTable`
- les types que mentionnent leurs fonctions et variables

Tout autre nom s'importe avec un `use`, comme `(use std::net::tcp-connect)`, ou s'écrit par son
chemin, `std::net::tcp-connect`. Le tableau donne ces noms et le module de chacun.

| Module | Noms |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`, `digit-weight`, `name->char` |
| `std::collections` | `Deque`, `HashSet`, `Scope`, `SortedTable`, `hashtable-iter` |
| `std::env` | `command-line-args`, `getenv`, `home-directory` |
| `std::error` | `WrappedError`, `as-dyn-error`, `assert-eq`, `describe-error`, `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`, `direction-input`, `direction-output`, `directory-p`, `open-binary`, `open-binary-input`, `open-binary-output`, `open-file`, `open-input`, `open-output`, `read-file-lines`, `read-file-string`, `write-file-string` |
| `std::hash` | `*sxhash-mask*`, `sxhash-string` |
| `std::io` | `ByteInput`, `ByteOutput`, `PeekInput`, `binary-file-stream`, `copy-stream`, `make-peek-stream`, `peek-stream`, `read-lines`, `write-lines` |
| `std::net` | `NetError`, `byte-stream-of`, `char-stream-of`, `datagram`, `socket-byte-stream`, `socket-listener`, `socket-stream`, `tcp-connect`, `tcp-listen`, `tls-connect`, `tls-listen`, `udp-bind`, `udp-socket`, `unix-connect`, `unix-listen`, `with-connection` |
| `std::num` | `ParseFloatError`, `ParseIntError`, `atan2`, `parse-float`, `parse-int` |
| `std::path` | `Pathish`, `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`, `read-from-string-preserving-whitespace`, `read-sexpr`, `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`, `right-trim`, `string->utf8`, `trim`, `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`, `Context`, `Mutex`, `WaitGroup`, `after`, `with-lock` |
| `std::thread` | `Task`, `Thread`, `yield` |
| `std::time` | `internal-time-seconds`, `timezone-daylight-p`, `timezone-offset-seconds` |

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
| [sequences.md](sequences.md) | La paire `cons-cell`, les données S-expression `Sexpr`, les symboles, les fonctions de séquence, itérateurs paresseux `lazy`, les fonctions d'ordre supérieur |
| [collections.md](collections.md) | Chaînes, caractères, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, les types d'erreur et le trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, les traits arithmétiques, `Push` |
| [printing.md](printing.md) | `print`/`println`/`format`, le pretty printer, `print-object`, les variables de contrôle de l'affichage |
| [format.md](format.md) | Directives de format |
| [streams-files.md](streams-files.md) | Flux, opérations sur les fichiers, noms de chemin, readtable |
| [concurrency.md](concurrency.md) | Tâches, canaux, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, sockets du domaine Unix, UDP |
| [system.md](system.md) | Temps, environnement d'exécution, outils de l'implémentation, `read`/`eval`, docstrings, fonctions liées aux macros |
