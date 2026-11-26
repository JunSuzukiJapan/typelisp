<!-- translated-from: docs/ja/guide/repl.md @ fddb236d8b1f4c292e9e126057ed7732aa8259a8 -->
# La REPL

Lancé sans argument, `typl` ouvre la REPL (mode interactif). Ce document présente ce que la REPL
permet en plus de l'évaluation d'expressions : ses commandes, l'aide à la saisie, l'accès aux
résultats précédents, l'arrêt d'un calcul et l'examen après une erreur.

## Démarrer et quitter

```
$ typl
typl> (+ 1 2)
3
typl> :quit
```

On quitte avec `:quit` (en abrégé `:q`) ou `Ctrl+D`. L'historique des saisies est conservé dans
`~/.typl_history` ; au lancement suivant, on le retrouve avec `Ctrl+P` / `Ctrl+N` ou `Ctrl+R`
(recherche dans l'historique).

Au démarrage, avant la première invite, les éléments suivants sont exécutés dans l'ordre. Aucun
n'affiche les valeurs qu'il produit.

1. Le contenu de `~/.typlrc`, s'il existe. `--no-init` empêche sa lecture.
2. `--eval FORM` et `--load FILE` de la ligne de commande, dans l'ordre où ils sont écrits. Chacun
   peut être donné autant de fois que voulu.

```
$ typl --load util.typl --eval '(println "ready")'
ready
typl>
```

`--eval`, `--load` et `--no-init` ne servent qu'au lancement de la REPL. Les donner pour exécuter
un fichier est une erreur.

La REPL démarre dans le module `user`. Quand on passe dans un autre module avec
`(in-module foo)`, l'invite affiche ce module.

```
typl> (in-module foo)
typl user::foo>
```

## Commandes de la REPL

Un `:nom` écrit en début de ligne est une commande de la REPL. `:help` en donne la liste. Un `:nom`
absent de la liste n'est pas une commande : il est évalué comme un mot-clé ordinaire.

| Commande | Signification |
|---|---|
| `:help` (`:h`, `:?`) | lister les commandes |
| `:type FORM` (`:t`) | afficher le type de FORM sans l'évaluer |
| `:describe NAME` (`:d`) | afficher la nature de la définition, son type, ses champs ou variantes, ses méthodes, les traits qu'elle implémente, l'endroit où elle a été définie et sa chaîne de documentation |
| `:apropos TEXT` | chercher dans tous les modules les définitions dont le nom contient TEXT (sans tenir compte de la casse) |
| `:src NAME` | afficher le source d'une définition |
| `:who-calls NAME` | afficher les fonctions et méthodes qui appellent la fonction ou la méthode NAME |
| `:inspect FORM` (`:i`) | évaluer FORM et ouvrir sa valeur selon son type |
| `:backtrace` (`:bt`) | afficher les appels en cours lors de l'échec de la ligne précédente, du plus interne vers l'extérieur |
| `:locals [N]` | afficher les arguments de l'appel N de `:backtrace` (l'appel 0 si N est omis) |
| `:cd [DIR]` | changer le dossier courant pour DIR (le dossier personnel si DIR est omis) |
| `:pwd` | afficher le dossier courant |
| `:quit` (`:exit`, `:q`) | quitter la REPL |

NAME peut être un nom comme `inner` ou un chemin comme `std::seq::reverse` ou `Option::unwrap`.

```
typl> (defun inner ((x int) (s string)) int "Ajoute 1 à x." (+ x 1))
typl> :describe inner
user::inner is a function
  type: (fn (int string) int)
  parameters: x s
  defined at <stdin>:1:1

Ajoute 1 à x.
typl> :src inner
(defun inner ((x int) (s string)) int "Ajoute 1 à x." (+ x 1))
```

`:src` affiche une définition chargée depuis un fichier à partir de ce fichier, et une définition
saisie dans la REPL à partir du texte saisi. L'emplacement dans le source des définitions de la
bibliothèque standard n'est pas enregistré ; leur source ne peut donc pas être affiché.

`:who-calls` parcourt les corps des fonctions de la session courante. Pour une fonction générique,
seules les utilisations instanciées avec des types concrets sont trouvées.

`:cd` change le dossier où cherche `(load ...)`, mais `use` continue de chercher les fichiers dans
le dossier où la REPL a été lancée.

## Ouvrir une valeur (`:inspect`)

`:inspect` ouvre une valeur selon son type. Une struct s'ouvre sur ses champs, un enum (y compris
`Option`) sur les champs de sa variante, un `Vector` sur ses éléments et une `HashTable` sur ses
clés et valeurs. Un numéro entre dans cette partie, `u` remonte d'un niveau et `q` termine.

```
typl> (defstruct point (x int) (y Option<int>))
typl> :inspect (make-instance 'point :x 1 :y (Some 2))
(make-instance 'point :x 1 :y (Some 2)) = #<point x: 1 y: (Some 2)>
  type: point
  0. x = 1
  1. y = (Some 2)
inspect> 1
y = (Some 2)
  type: Option<int>
  0. Some 0 = 2
inspect> q
```

Les listes et les `Sexpr` ne sont pas ouverts ; ils sont affichés tels quels.

## Aide à la saisie

Lors d'une saisie dans un terminal, on dispose de ce qui suit.

- `Tab` complète les noms (les définitions et la syntaxe comme `if` et `defun`), les chemins
  contenant `::`, les commandes de la REPL en début de ligne et, dans une chaîne, les noms de
  fichiers.
- Après le nom d'une fonction suivi d'une espace, le type de la fonction s'affiche en atténué en
  fin de ligne.
- La parenthèse correspondant à celle qui est à côté du curseur est mise en évidence.
- Si l'on appuie sur Entrée avant la fin d'une forme, la ligne suivante commence indentée selon le
  nombre de parenthèses ouvertes.

## Résultats précédents (`:*`, `:**`, `:***`)

Dans la REPL, `:*` est la dernière valeur affichée, `:**` celle d'avant et `:***` celle d'avant
encore. Chacune a le type de sa valeur.

```
typl> 1
1
typl> "a"
"a"
typl> 2
2
typl> (+ :*** :*)
3
```

- Une saisie qui n'a affiché aucune valeur (une définition, par exemple) ou qui a échoué ne fait
  pas avancer les résultats.
- Écrire `:**` ou `:***` alors qu'il n'y a pas encore autant de résultats est une erreur.
- Un `':*` quoté est un mot-clé ordinaire.
- Un `:*` écrit dans le corps d'une fonction continue de désigner le résultat du moment où la
  fonction a été définie. Le `*` de Common Lisp lit la valeur au moment de l'appel, mais typelisp
  la fixe à la définition pour qu'elle ait un type.
- Hors de la REPL (exécution d'un fichier, ou dans `~/.typlrc`, `--load` et `--eval`), `:*` est un
  mot-clé ordinaire.

## Arrêter un calcul (`Ctrl+C`)

Appuyer sur `Ctrl+C` pendant une évaluation arrête ce calcul et ramène à l'invite. Ce qui a été
défini auparavant est conservé. Cela fonctionne qu'il s'agisse d'une boucle dans l'interpréteur,
d'une boucle dans du code compilé ou d'une attente dans `sleep` ou `wait`. Les nettoyages des
`unwind-protect` en cours sont exécutés.

```
typl> (loop)
^Cerror: interrupted
typl>
```

`Ctrl+C` pendant la saisie ne fait qu'abandonner la ligne en cours de saisie.

## Examiner après une erreur (`:backtrace`, `:locals`)

Après l'arrêt d'une évaluation sur une erreur, `:backtrace` (`:bt`) affiche les appels de fonctions
qui étaient en cours à ce moment, du plus interne vers l'extérieur. `:locals N` affiche les
arguments de l'appel N selon leurs types déclarés.

```
typl> (defun inner ((x int)) int (if (= x 0) (panic "boom") x))
typl> (defun outer ((n int)) int (* 2 (inner n)))
typl> (outer 0)
error: <stdin>:1:40: panic: boom
typl> :bt
  0: (user::inner 0)  called at <stdin>:1:33
  1: (user::outer 0)  called at <stdin>:1:1
typl> :locals 1
  n = 0
```

- Pour un appel fait depuis la dernière expression d'une fonction (un appel terminal), la fonction
  appelante n'est pas affichée. C'est ce qui permet à une boucle récursive terminale de n'occuper
  la place que d'un seul appel, quel que soit le nombre de tours.
- Les appels à l'intérieur des fonctions compilées et les fonctions anonymes (`lambda`) ne sont pas
  affichés.
- `:locals` n'affiche que les arguments, pas les variables liées par `let`.
- L'évaluation de la ligne suivante efface l'enregistrement de l'erreur précédente.
