<!-- translated-from: docs/ja/reference/functions/format.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Directives de format

Les directives qu'on écrit dans les chaînes de contrôle de `print`/`println`/`format`. Elles couvrent presque toutes
les directives `format` de CL. Les fonctions elles-mêmes sont décrites dans
[Affichage](printing.md#1-print--println--format).

## 1. Comment écrire les directives

Chaque directive se compose, dans l'ordre, de `~`, de **paramètres préfixes** facultatifs (séparés par des
virgules : un entier / `'c` (un caractère) / `v` (pris dans l'argument suivant) / `#` (le nombre d'arguments
restants)), des **modificateurs** facultatifs `:` et `@`, puis du caractère de directive. Les caractères de
directive sont insensibles à la casse.

La chaîne de contrôle doit être un littéral ([Affichage](printing.md#1-print--println--format)). En plus, les points
suivants sont contrôlés à la vérification.

- **Le nombre et les types des arguments.** Pour chaque directive qui consomme un argument : s'il reste un argument
  et si son type est accepté (les mentions « argument » dans les tableaux ci-dessous). Là où le chemin dépend de
  valeurs à l'exécution, comme un déplacement avec `~*`, la clause de `~[` choisie, le déclenchement de `~^` ou le
  nombre de répétitions de `~@{`, **chaque chemin** est contrôlé. Les arguments en trop sont acceptés (comme en
  CL).
- **Paramètres et modificateurs.** Un modificateur non accepté, trop de paramètres et des valeurs hors limites (une
  largeur négative, une base hors de 2 à 36, un entier là où un caractère est attendu, etc.) sont des erreurs. Ils
  ne sont jamais ignorés ni arrondis silencieusement.

Les **éléments** d'un argument liste (`~{`, `~:{`, `~<...~:>`) sont des `Sexpr`, et ni leur nombre ni le type de
chaque élément ne peuvent se déduire des types. Les exigences sur les éléments (un entier pour `~d`, etc.) et les
éléments manquants sont contrôlés à l'arrivée des valeurs et sont des erreurs d'exécution (on ne bascule jamais
vers une autre représentation à la place).

Les règles indulgentes de CL ne sont pas adoptées. Passer un non-entier à `~d` et le faire afficher comme `~a`, ou
que `~:[` traite n'importe quelle valeur comme un booléen, n'est pas réinterprété ainsi ; ce sont des erreurs de
type.

## 2. Sortie (consomme un argument)

| Directive | Paramètres / modificateurs | Signification |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=justification à droite | Esthétique (le `princ` de CL ; chaînes sans guillemets). L'argument peut être de n'importe quel type |
| `~s` | Idem | Standard (le `prin1` de CL ; une forme relisible). L'argument peut être de n'importe quel type |
| `~w` | — | Le `write` de CL. Affiche joliment si `*print-pretty*` est vrai, sinon identique à `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=groupes de chiffres, `@`=toujours un signe | Entiers décimaux/binaires/octaux/hexadécimaux. L'argument est un entier |
| `~r` | `~radix,mincol,padchar,commachar,interval` (avec une base) ou aucun | Avec une base, dans cette base (2 à 36). Sans : `~r`=cardinal anglais, `~:r`=ordinal anglais, `~@r`=chiffres romains, `~:@r`=anciens chiffres romains. L'argument est un entier |
| `~p` | `:`=reculer d'un, `@`=y/ies | Pluriels (`~p`→"s", `~@p`→"y"/"ies"). L'argument est un entier |
| `~c` | `:`=nom, `@`=syntaxe `#\` | Un caractère. L'argument est un `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=signe | Virgule fixe. L'argument est un nombre |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=signe | Notation exponentielle. L'argument est un nombre. Les paramètres de CL pour les chiffres de l'exposant, l'échelle et overflowchar ne sont pas pris en charge (les donner est une erreur) |
| `~g` | `@`=signe | Virgule flottante générale. L'argument est un nombre. Ne prend pas de paramètres |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Notation monétaire. L'argument est un nombre |

## 3. Sortie (ne consomme pas d'argument)

| Directive | Signification |
|---|---|
| `~%` | Saut de ligne (`~n%` pour n sauts) |
| `~&` | fresh-line (un saut de ligne sauf en début de ligne ; `~n&`) |
| `~\|` | Saut de page (form feed) |
| `~~` | Un `~` littéral (`~n~` pour n) |
| `~t` | Tabulation (`~colnum,colincT`. Si l'on est déjà à la colonne colnum ou au-delà, avance d'un multiple de colinc ; pas de déplacement si colinc vaut 0. `@`=relatif. `:`=une tabulation relative au début du bloc logique, qui ne fonctionne qu'en affichage joli) |
| `~_` | Saut de ligne conditionnel (pretty ; simple=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Indentation (pretty ; `~ni`=début du bloc + n / `~n:i`=colonne courante + n) |
| `~<newline>` | Ignore le saut de ligne (`:`=garder les blancs, `@`=garder le saut de ligne) |

Comme en CL, les directives du pretty printer (`~_` `~i` `~:t` `~<...~:>`, et le chemin d'affichage joli de
`~a`/`~s`/`~w`) ne font toutes rien quand `*print-pretty*` est faux. Il est faux par défaut.

## 4. Structures de contrôle

| Directive | Signification |
|---|---|
| `~(...~)` | Conversion de casse (`~(` minuscules, `~:(` majuscule à chaque mot, `~@(` majuscule au premier mot seulement, `~:@(` tout en majuscules) |
| `~[...~;...~]` | Sélection conditionnelle (bifurque selon un argument entier. Avec `~n[`, `~v[` ou `~#[`, elle bifurque selon cette valeur et ne prend pas d'argument. `~:;`=la clause par défaut, seulement en dernière clause). `~:[false~;true~]` bifurque selon un argument `bool` et a exactement deux clauses |
| `~{...~}` | Itération (parcourt un argument liste. `~:{`=par sous-liste, `~@{`=sur les arguments restants, `~:@{`=sur chaque liste parmi les arguments restants, `~^`=sortie, `~:}`=exécuter une fois même si vide). Un corps qui ne consomme aucun argument en une itération est une erreur (il ne se terminerait jamais) |
| `~<...~;...~>` | Justification (répartit les segments sur `~mincol` colonnes. `:`/`@`=remplissage aux extrémités) |
| `~<...~;...~:>` | **Bloc logique** (fermé par `~:>` ; autre chose que la justification ci-dessus). Le premier segment est le préfixe et le dernier le suffixe (tous deux uniquement des chaînes littérales). Avec le séparateur `~@;`, le préfixe est un **préfixe par ligne**. `~:<` donne `(`/`)` comme préfixe/suffixe par défaut. L'argument est une liste (`~@<` utilise à la place les arguments restants) |
| `~*` | Sauter des arguments (`~n*`=n en avant, `~:*`=en arrière, `~@*`=vers une position absolue) |
| `~/name/` | Appel de méthode (chapitre 5. Les drapeaux `:`/`@` sont passés à la méthode. Ne prend pas de paramètres) |

Les directives de CL suivantes ne sont pas prises en charge (ce sont des erreurs à la vérification).

- `~?` et `~@?` : elles prennent une chaîne de contrôle comme argument d'exécution ; les arguments que consomment
  ses directives ne peuvent donc pas être contrôlés. Écrivez ces directives directement dans la chaîne de contrôle.
- `~@[...~]` : elle teste si un argument n'est pas nil, mais ce langage n'a pas de nil. Utilisez
  `~:[false~;true~]`, qui bifurque selon un `bool`.
- `~{~}` avec un corps vide : il prend le corps dans un argument d'exécution. Écrivez les directives entre les
  accolades.

## 5. `~/name/`

**Une différence avec CL : le nom n'est pas cherché comme fonction globale mais comme méthode du propre type de
l'argument.** La méthode a la forme `((self Self) (colon bool) (at bool)) → string`, et les `:`/`@` de la directive
sont transmis tels quels.

La façon de CL de le chercher comme fonction globale ne peut pas être mise en œuvre sûrement dans ce langage. Même
avec une chaîne de contrôle littérale, le type des éléments d'un argument liste (à l'intérieur de `~{`) n'est pas
connu à la vérification, et chercher une fonction par son seul nom pourrait appeler une fonction destinée à un
autre type. Choisir selon le type de la valeur signifie que la méthode est vérifiée exactement pour ce type, ce
qui est sûr (le même mécanisme que `print-object`). Cela fonctionne aussi pour des valeurs comme
`string`/`bool`/`char`/`symbol`/listes. Seuls les entiers, dont la largeur ne se déduit pas de la valeur, donnent
une erreur **quand plus d'un type entier définit une méthode de ce nom**.

On ne sait pas à quel argument elle s'applique, mais on sait quelles méthodes elle peut appeler. Le vérificateur
rassemble chaque `~/name/` de la chaîne de contrôle littérale et note, parmi les types des arguments à cet endroit
d'appel, ceux qui ont une méthode de la forme ci-dessus. Ainsi, **si aucun des types d'arguments n'a la méthode,
c'est une erreur à la vérification** (et non à l'exécution), et cela fonctionne aussi dans les exécutables AOT.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```
