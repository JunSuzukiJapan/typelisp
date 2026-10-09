<!-- translated-from: docs/ja/reference/functions/printing.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Affichage

`print`/`println`/`format`, les fonctions d'affichage à un argument, le pretty printer, `print-object` et les
variables qui contrôlent l'affichage. La liste des directives de format se trouve dans [format.md](format.md). La
lecture et l'écriture sur des flux se trouvent dans [Flux et fichiers](streams-files.md).

## 1. `print` / `println` / `format`

`print`/`println`/`format` sont tous des **formes spéciales qui interprètent des directives de format (les
directives `format` de CL)**. Le premier argument (le second pour `format`) est la **chaîne de contrôle**, et chaque
directive consomme tour à tour les arguments variadiques qui suivent.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `print` | `(print control args...)` | `(string, ...)→Unit` | Développe la chaîne de contrôle et l'écrit sur la sortie standard sans saut de ligne |
| `println` | `(println control args...)` | `(string, ...)→Unit` | Idem, avec un saut de ligne à la fin |
| `format` | `(format dest control args...)` | `(bool, string, ...)→string` | Le `format` de CL. Renvoie la chaîne développée. Si `dest` est `true` (le `t` de CL), elle est aussi écrite sur la sortie standard ; si `false` (le `nil` de CL), elle n'est pas écrite et seulement renvoyée |
| `format` (vers un flux) | `(format stream control args...)` | `(S, string, ...)→()` where `CharOutput S` | Si `dest` n'est pas un `bool`, c'est la destination flux de CL. La chaîne développée est écrite dans ce flux. La valeur de retour est `()` (le `nil` de CL), et aucune chaîne n'est renvoyée |

Le type de `dest` scinde le sens en deux (lequel s'applique est décidé statiquement). La forme flux peut s'écrire de
la même façon avec un type de flux concret, un `:dyn CharOutput` ou une variable de type liée par
`(where (CharOutput S))`. Un `dest` qui n'est ni un `bool` ni un flux est une erreur de type.

**La chaîne de contrôle doit être un littéral** (la même restriction que le `format!` de Rust). Les directives
qu'elle contient décident combien d'arguments sont pris et de quels types ; une chaîne construite à l'exécution ne
peut donc pas être lue à la vérification. Parce qu'elle doit être un littéral, **le nombre et les types des
arguments sont contrôlés à la vérification** : `(println "~d" "x")` et `(println "~a ~a" 1)` sont des erreurs à la
vérification. Une directive mal orthographiée, un `~(` non fermé et un `~/name/` auquel aucun argument ne peut
répondre sont aussi des erreurs à la vérification. Les règles de contrôle se trouvent dans
[format.md](format.md#1-comment-écrire-les-directives). Pour afficher une chaîne que vous construisez, créez-la avec
`(format false ...)` et affichez-la avec `(println "~a" s)`.

Les arguments variadiques sont enveloppés en `Sexpr` avec leurs propres types avant d'être passés :
`i32`/`f64`/`int`/`ratio`/`char`/`bool`/`string`/`Sexpr`, ainsi que les `defstruct`/`defenum`/`Vector<T>`/
`HashTable<K,V>` définis par l'utilisateur et semblables, peuvent tous être passés tels quels
(`(println "~a" my-struct)` fonctionne simplement).

Exécuter un script avec `typl file.typl` **n'affiche pas les valeurs des expressions de niveau supérieur** ; un
programme écrit donc sur la sortie standard en appelant ces fonctions. `print`/`println`/`format` envoient leur
sortie à chaque appel (afin qu'une invite soit visible avant la lecture de l'entrée standard, même à travers un
tube).

**`Option<Sexpr>` s'affiche de façon transparente.** Le type des données S-expression est `Option<Sexpr>` ;
l'enveloppe `(some x)` n'apparaît donc pas dans la sortie et le contenu est affiché tel quel. La liste vide
s'affiche `()`. Les autres `Option<T>` s'affichent `(some ...)` / `none`. Il en va de même pour les champs
`Option<T>` à l'intérieur des structures, des énumérations et des `Vector`. Un `Result<Option<Sexpr>,…>` issu de
`(eval ...)` s'affiche `(ok 42)`, ou `(ok ())` pour `none`.

```lisp
(println "~a" (the Option<Sexpr> (Option::some 42)))   ; => 42
(println "~a" (the Option<Sexpr> ()))                  ; => ()
(println "~a" (the Option<i32>   (Option::some 42)))   ; => (some 42)
(defstruct p (x Option<int>))
(println "~a" (p::new (Option::some 1)))               ; => #<p x: (some 1)>
(println "~a" (p::new (Option::none)))                 ; => #<p x: none>
```

```lisp
(println "~a + ~a = ~d" 1 2 3)        ; => 1 + 2 = 3
(println "[~5,'0d]" 42)               ; => [00042]
(println "~:d" 1234567)               ; => 1,234,567
(println "~@r / ~r" 2024 42)          ; => MMXXIV / forty-two
(println "~{~a~^, ~}" '(a b c))       ; => a, b, c
(println "~[zero~;one~;two~]" 1)      ; => one
(let ((s (format false "id=~d" 42)))  ; obtenir seulement la chaîne, sans afficher
  (println "~a" s))                   ; => id=42
```

## 2. Fonctions d'affichage à un argument

Les fonctions d'affichage de CLHS 22.1.3. Au lieu de développer un format, elles affichent une seule valeur telle
quelle. Le flux peut être omis (par défaut `*standard-output*`).

| Nom | Forme | Description |
|---|---|---|
| `prin1` | `(prin1 x [stream])` | Écrit sous une forme relisible (identique à `~s`) et renvoie `x` |
| `princ` | `(princ x [stream])` | Écrit sous une forme pour les humains (identique à `~a`) et renvoie `x` |
| `write` | `(write x [stream])` | `prin1` si `*print-escape*` est vrai, `princ` s'il est faux. Renvoie `x` |
| `prin1-to-string` | `(prin1-to-string x)` | Renvoie une chaîne au lieu d'écrire (`~s`) |
| `princ-to-string` | `(princ-to-string x)` | Idem (`~a`). Identique à `to-string` |
| `write-to-string` | `(write-to-string x)` | Idem, selon `*print-escape*` |

`print`/`println` n'en font **pas** partie. Ce sont des raccourcis de `format` qui prennent une chaîne de contrôle,
un rôle différent du `print` de CL (saut de ligne, puis `prin1`, puis un espace) ; chacun garde donc son propre nom.
En conséquence, **le `print` à un argument de CL n'a pas d'écriture dans ce langage** : écrivez `prin1`.

Ce sont des macros, parce que les arguments variadiques de `format` n'acceptent pas de variables de type et que le
type doit être connu à l'endroit de l'appel.

## 3. Entrée standard et flux standard

**La lecture de l'entrée standard** ne se fait pas avec des fonctions dédiées mais avec les méthodes de
`CharInput` sur le flux standard `*standard-input*` : `(read-line *standard-input*)` /
`(read-char *standard-input*)` / `(read-all *standard-input*)` ([méthodes de flux](streams-files.md#2-méthodes)).
La sortie standard et la sortie d'erreur ont de même `*standard-output*` / `*error-output*`, et s'écrivent comme dans
`(write-line *standard-output* s)` (`print`/`println`/`format` sont des raccourcis pour quand on a besoin du
développement de format, et écrivent toujours sur la sortie standard).

## 4. Le pretty printer

Il correspond au Lisp Pretty Printer de CL (CLHS 22.2). **Il coupe la sortie qui ne tient pas dans la largeur de
ligne, en suivant des blocs logiques et des sauts de ligne conditionnels.**

### 4.1 Variables de contrôle

Des variables globales affectables. Une fois modifiées avec `setf`, elles affectent tout l'affichage ultérieur. Pour
en changer une temporairement, utilisez `dlet` (6.3).

| Variable | Type | Défaut | Signification |
|---|---|---|---|
| `*print-pretty*` | `bool` | `false` | Si vrai, `~a`/`~s`/`~w` et les directives pretty prennent le chemin de l'affichage joli |
| `*print-right-margin*` | `int` | `80` | La marge droite (en colonnes). 0 signifie « pas de marge, ne jamais couper ». Une valeur négative est une erreur d'affichage |
| `*print-miser-width*` | `int` | `0` | La largeur à partir de laquelle commence le style miser. 0 correspond au `nil` de CL (style miser désactivé). Une valeur négative est une erreur d'affichage |

La famille `pprint` et `pprint-logical-block` affichent toujours joliment, quel que soit `*print-pretty*` (selon la
définition du `pprint` de CL).

### 4.2 Mises en page toutes faites (formes spéciales)

Comme `print`, ce sont des formes spéciales ; l'argument peut donc être de n'importe quel type.

| Nom | Forme | Description |
|---|---|---|
| `pprint` | `(pprint x)` | Affiche joliment avec la mise en page par défaut. Comme en CL, **il écrit d'abord un saut de ligne** et aucun à la fin |
| `pprint-fill` | `(pprint-fill x)` | Remplit chaque ligne d'autant que possible. N'écrit pas de saut de ligne |
| `pprint-linear` | `(pprint-linear x)` | Si tous les éléments ne tiennent pas sur une ligne, **un élément par ligne**. N'écrit pas de saut de ligne |
| `pprint-tabular` | `(pprint-tabular x [colinc])` | Un tableau aux colonnes de `colinc` de large (16 par défaut). N'écrit pas de saut de ligne. Un `colinc` négatif est une erreur |

La mise en page par défaut (`pprint`, et `~a` sous `*print-pretty*`) suit le `*print-pprint-dispatch*` par défaut de
CL : elle abrège `(quote x)` en `'x` et met en forme les formes de code comme `defun`/`let`/`if`/`lambda` comme « la
tête et le nombre prescrit d'arguments sur la première ligne, et le reste du corps indenté de deux colonnes, une
forme par ligne ». Les autres listes sont remplies.

```lisp
(setf *print-right-margin* 20)
(pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
;; =>
;; (1 2 3 4 5 6 7 8 9
;;  10 11 12 13 14 15)
(pprint '(defun f (x) i32 (+ x 1) (* x 2)))
;; =>
;; (defun f (x) i32
;;   (+ x 1)
;;   (* x 2))
```

### 4.3 Construire soi-même des blocs logiques

| Nom | Forme | Description |
|---|---|---|
| `pprint-logical-block` | `(pprint-logical-block (obj :prefix p :per-line-prefix p :suffix s) body...)` | Une forme spéciale qui ouvre un bloc logique. `obj` est la liste que parcourt `pprint-pop` (`()` si aucune n'est parcourue). `:prefix` et `:per-line-prefix` s'excluent mutuellement (comme en CL) |
| `pprint-newline` | `(pprint-newline kind)` | Un saut de ligne conditionnel. `kind` vaut `:linear` / `:fill` / `:miser` / `:mandatory` |
| `pprint-indent` | `(pprint-indent kind n)` | Indentation. `kind` vaut `:block` (depuis le début du bloc) / `:current` (depuis la colonne courante) |
| `pprint-tab` | `(pprint-tab kind colnum colinc)` | Une tabulation. `kind` vaut `:line` / `:section` / `:line-relative` / `:section-relative`. `colnum` et `colinc` sont positifs ou nuls (une erreur s'ils sont négatifs) |
| `pprint-pop` | `(pprint-pop)` | Prend l'élément suivant de la liste du bloc (`()` si elle est épuisée) |
| `pprint-list-exhausted` | `(pprint-list-exhausted)` | Si la liste est épuisée |
| `pprint-exit-if-list-exhausted` | `(pprint-exit-if-list-exhausted)` | Si elle est épuisée, sort par `break` de la `loop` englobante (une macro) |

Les blocs logiques ne prennent pas d'argument flux : **un bloc logique ouvert est un état implicite**. Le
`pprint-logical-block` le plus extérieur le démarre, et quand il se ferme, l'ensemble est mis en forme et écrit d'un
coup sur la sortie standard. Tant qu'il est ouvert, la sortie de `print`/`println`/`(format true ...)`/`pprint` va
entièrement dans ce bloc ; **on écrit donc le contenu avec un `print` ordinaire et on ne marque que les endroits de
coupure avec `pprint-newline` et consorts**, ce qui donne un code presque identique à celui de CL.

En CL, `pprint-exit-if-list-exhausted` est une sortie non locale de `pprint-logical-block` ; ici, c'est **un `break`
de la `loop` englobante** (`pprint-logical-block` n'établit pas de `block`). L'idiome de CL le place de toute façon
toujours dans une `loop` ; cela se lit donc de la même manière.

```lisp
(setf *print-right-margin* 24)
(pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
  (loop (pprint-exit-if-list-exhausted)
        (print "~w" (pprint-pop))
        (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
;; =>
;; (alpha beta gamma delta
;;  epsilon zeta)
```

Règles des sauts de ligne conditionnels (CLHS `pprint-newline`) :

- `:mandatory` coupe toujours.
- `:linear` coupe si le bloc logique englobant ne tient pas sur une ligne. La décision se prend par bloc ; **tous les
  sauts `:linear` d'un bloc coupent ensemble** (c'est le « tout sur une ligne ou un élément par ligne » de
  `pprint-linear`).
- `:fill` coupe si (a) la section suivante ne tient pas dans le reste de la ligne, (b) la section précédente n'a pas
  tenu sur une ligne, ou (c) en style miser, le bloc ne tient pas sur une ligne.
- `:miser` agit comme `:linear` uniquement en style miser (quand le bloc commence à moins de
  `*print-miser-width*` de la marge droite).

## 5. `print-object` (représentation imprimée par type)

Écrire `impl print-object <type>` fait afficher par `print`/`println`/`format`/`pprint` les valeurs de ce type avec
cette implémentation, **même quand elles sont imbriquées dans des listes**. Cela correspond à la fonction générique
`print-object` de CL (CLHS 22.1.4).

```lisp
(deftrait print-object ()
  (print-object ((self Self) (escape bool)) string))
```

| Argument | Signification |
|---|---|
| `self` | La valeur à afficher |
| `escape` | Le `*print-escape*` de CL. `true` pour `~s`/`prin1`/`pprint` (une forme relisible), `false` pour `~a`/`princ` (pour les humains). Une implémentation qui ne s'en soucie pas peut l'ignorer |

La `string` renvoyée va directement dans la sortie. Les types sans `impl` s'affichent dans la représentation
intégrée (de la forme `#<point x: 1 y: 2>`).

```lisp
(defstruct point (x i32) (y i32))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))

(println "~a" p)              ; => (1,2)
(println "~s" p)              ; => #S(point :x 1 :y 2)
(println "~a" (list p q))     ; => ((1,2) (3,4))   fonctionne aussi imbriqué
```

Cela se combine aussi avec le pretty printer (chapitre 4). Si `*print-pretty*` est vrai, une liste contenant les
chaînes renvoyées par l'implémentation est coupée à la marge droite.

Les représentations imprimées des types de la bibliothèque standard. Les types qui existent aussi en CL
s'affichent comme dans SBCL. Quand la REPL montre un résultat, elle utilise la même représentation que `~s`.

| Type | `~s` | `~a` |
|---|---|---|
| `Vector<T>` | `#(1 2 3)`, `#("a" "b")` | `#(1 2 3)`, `#(a b)` |
| `HashTable<K,V>` | `#<hashtable<string,int> count=1>` | Idem |
| `Chan<T>` / `Task<T>` / `Thread<T>` | `#<chan<int> 0>` (le nombre est un numéro de série interne) | Idem |
| `pathname` | `#P"/tmp/a.txt"` | `/tmp/a.txt` |
| `universal-time` / `internal-time` | Un entier (la valeur du `get-universal-time` / `get-internal-real-time` de CL) | Idem |
| Types d'erreur (`ParseIntError`, `SimpleError`, etc.) | `#<simpleerror "boom">` | Le message seulement (`boom`) |
| `complex` | `#C(1.0 2.0)` | Idem |
| `Array<T>` | `#2A((0 0) (0 0))` | Idem |
| Flux | `#<file-stream for "file /tmp/a.txt" {7}>`, `#<string-output-stream {5}>`, `#<two-way-stream :input-stream … :output-stream …>` | Idem |
| Sockets | `#<socket-stream for "socket 127.0.0.1:5000, peer: 127.0.0.1:6000" {10}>`, `#<socket-listener 0.0.0.0:8080, fd: 6 {13}>` | Idem |
| `decoded-time` | `#<decoded-time 2026-09-28 13:40:24 +09:00 Mon>` (`dst` à la fin pendant l'heure d'été) | Idem |
| `heap-info` | `#<heap-info 176246 of 262144 cells live (67%), 85898 free, 2058 symbols, 5928 strings, 199 boxes, 2 collections, growable>` | Idem |
| Types `defstruct` | `#<point x: 1 y: 2>` (noms et valeurs des champs) | Idem (champs avec `~a`) |

Règles :

- **L'enregistrement est statique.** Un `impl` est vérifié comme une définition de méthode ordinaire ; un nom de type
  mal orthographié ou une mauvaise signature est donc une erreur de compilation.
- **Cela marche aussi pour les types génériques.** `(impl print-object box<T> (where (print-object T)) ...)` mène à
  un corps distinct pour chaque argument de type : une valeur se souvient de son type, arguments de type compris
  (`box<i32>`). Les types génériques intégrés comme `Vector<T>` fonctionnent de la même façon.
- **Le choix se fait à l'affichage.** Quelle directive consomme quel argument dépend du contenu de la chaîne de
  contrôle à l'exécution ; la distinction entre `~a` et `~s` (c'est-à-dire `escape`) n'est donc connue qu'au
  moment de l'affichage. C'est comme CLOS, où les méthodes `print-object` sont « définies par classe et choisies à
  l'affichage ».
- **Une réentrée retombe sur la représentation intégrée.** Si une implémentation s'affiche elle-même avec
  `(format false "~a" self)`, elle récurserait indéfiniment ; quand une valeur en cours d'affichage réapparaît, la
  représentation intégrée est donc utilisée. Cela regarde l'identité de la valeur, pas une limite de profondeur ;
  cela ne gêne donc pas l'affichage légitime de structures autoréférentes imbriquées.
- **Chaque type scalaire implémente ce trait.** C'est **pour pouvoir l'utiliser comme borne** : les arguments
  variadiques de `format` ne peuvent pas prendre de variables de type ; cette borne est donc le seul moyen pour du
  code générique de dire « les valeurs d'un type inconnu peuvent être rendues » (la même forme que le `T: Display`
  de Rust). Le `print-object` de `Array<T>` en est un exemple.
- **Avec des arguments de type qui ne satisfont pas la borne, la représentation intégrée est utilisée
  silencieusement.** `(impl print-object Array<T> (where (print-object T)))` s'applique à `Array<i32>`, mais pas à
  un `Array` dont les éléments sont un `defstruct` sans `print-object`. Il serait absurde que la simple création
  d'un tableau soit une erreur ; ce n'en est donc pas une.
- L'autre mécanisme de CL, `set-pprint-dispatch` / `*print-pprint-dispatch*` (un registre à l'exécution indexé par
  des spécificateurs de type), **n'est pas adopté**. Ses enregistrements ne sont pas vérifiés, ce qui ne convient pas
  à un langage à typage statique.

## 6. Contrôler la quantité imprimée

### 6.1 Profondeur, longueur et partage

Les variables de contrôle de CLHS 22.1.1 qui décident « combien d'une valeur est affiché ». Comme les trois de 4.1,
ce sont des variables globales affectables, et elles s'appliquent à tous les `print`/`println`/`format`/`pprint`,
que `*print-pretty*` soit vrai ou non.

| Variable | Type | Défaut | Signification |
|---|---|---|---|
| `*print-level*` | `int` | `0` | Les objets imbriqués à cette profondeur ou plus sont remplacés par `#`. L'objet affiché est à la profondeur 0. 0 signifie illimité |
| `*print-length*` | `int` | `0` | Affiche les éléments de liste (et les champs des valeurs `defstruct`/`defenum`) jusqu'à ce nombre et remplace le reste par `...`. 0 signifie illimité |
| `*print-circle*` | `bool` | `false` | Si vrai, la valeur est parcourue avant l'affichage et **les objets qui apparaissent deux fois ou plus reçoivent des étiquettes**. La première occurrence est `#n=…` et les suivantes `#n#` |

CL utilise `nil` pour « illimité », mais ce langage n'a pas de `nil` ; comme pour `*print-right-margin*`, **0
signifie illimité**. Les valeurs négatives n'ont pas de sens et sont des erreurs d'affichage. Les valeurs par défaut
sont toutes « pas de limite / pas d'étiquettes », comme les valeurs initiales de CL.

```lisp
(setf *print-level* 2)
(println "~a" '(1 (2 (3 (4)))))   ; => (1 (2 #))
(setf *print-level* 0)

(setf *print-length* 4)
(println "~a" '(1 2 3 4 5 6))     ; => (1 2 3 4 ...)
(setf *print-length* 0)
```

**Les structures circulaires ne peuvent être affichées que si `*print-circle*` est vrai.** Si vous affichez une
valeur qui pointe vers elle-même alors qu'il est faux (par défaut), l'affichage suit le cycle indéfiniment et le
processus plante. C'est pareil en CL (CLHS laisse indéfini l'affichage des structures circulaires quand
`*print-circle*` est faux).

Un cycle ne peut se créer qu'en « faisant pointer un champ de `defstruct` vers lui-même avec `setf` » (les cellules
`Sexpr` ne peuvent pas être modifiées après leur création ; une liste comme `'(1 2 3)` ne peut donc jamais être
circulaire) :

```lisp
(defstruct node (val int) (next Option<node>))

(let ((a (node::new 1 (Option::none))))
  (setf a::next (Option::some a))   ; a pointe vers a lui-même
  (setf *print-circle* true)
  (println "~a" a))                 ; => #1=#<node val: 1 next: (some #1#)>
```

Les étiquettes **recommencent à 1 pour chaque affichage** (comme en CL). Même sans cycle, si le même objet apparaît
deux fois, il reçoit `#1=`/`#1#`, conservant dans la sortie l'information « ces deux-là sont le même objet », comme
le spécifie CL :

```lisp
(setf *print-circle* true)
(let ((x '(1 2)))
  (println "~a" (list x x)))        ; => (#1=(1 2) #1#)
```

Une valeur sans partage **n'affiche aucune étiquette** ; laisser cette variable à vrai ne change donc pas la sortie
du code courant.

### 6.2 Base, casse et lisibilité

| Variable | Type | Défaut | Signification |
|---|---|---|---|
| `*print-base*` | `int` | `10` | La base d'affichage des entiers (largeur fixe et `int`). En dehors de 2 à 36, c'est une **erreur d'affichage** (CL spécifie aussi la plage) |
| `*print-radix*` | `bool` | `false` | Si vrai, ajoute un marqueur de base : `#b`/`#o`/`#x`, `#NNr` pour les autres bases, et un `.` final pour la base 10. Le marqueur va **avant** le signe (`#x-ff`) |
| `*print-case*` | `symbol` | `:downcase` | La casse des noms de symboles : `:upcase` / `:downcase` / `:capitalize` (les mêmes écritures que CL). Tout autre symbole est une erreur d'affichage |
| `*print-readably*` | `bool` | `false` | Si vrai, affiche sous une forme relisible. Force l'échappement et désactive les coupures de `*print-level*`/`*print-length*` |
| `*print-lines*` | `int` | `0` | Le nombre de lignes que peut utiliser le pretty printer. L'excédent est coupé, avec `..` à la fin comme en CL. 0 signifie illimité. Une valeur négative est une erreur d'affichage |
| `*print-escape*` | `bool` | `true` | Si `write`/`write-to-string` font `prin1` ou `princ`. **Seules ces deux-là la lisent** |
| `*print-array*` | `bool` | `true` | Si `Vector<T>` et `Array<T>` montrent leur contenu. Si vrai, la syntaxe des tableaux de CL (`#(1 2 3)` / `#2A((1 2) (3 4))`) ; si faux, seulement le type et la forme, `#<vector<int> 3>` / `#<array 2x3>` |

```lisp
(dlet ((*print-base* 16)) (format false "~a" 255))                    ; => "ff"
(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255)) ; => "#xff"
(dlet ((*print-case* :upcase)) (format false "~a" 'hello))            ; => "HELLO"
```

Les marqueurs qu'ajoute `*print-radix*` peuvent être relus par le lecteur (la notation de base dans la
[Référence de la syntaxe](../syntax.md#1-éléments-lexicaux)).

**Pourquoi la valeur par défaut de `*print-case*` diffère de CL** : celle de CL est `:upcase` parce que le lecteur de
CL stocke les noms de symboles en majuscules ; elle signifie donc « tel que stocké ». Ce lecteur les stocke en
minuscules ; la valeur par défaut de même sens est donc `:downcase`.

**La moitié manquante de `*print-readably*`** : CL signale `print-not-readable` pour les valeurs qui ne peuvent pas
être relues, mais ce langage n'a pas de condition à signaler, ni de moyen de décider de la lisibilité des types
utilisateur, que `print-object` peut afficher de n'importe quelle façon. Seuls l'échappement forcé et la levée des
coupures sont présents.

**Pourquoi seul `write` lit `*print-escape*`** : comme le spécifie CLHS, `~s`/`prin1`/`pprint` la lient à vrai et
`~a`/`princ` à faux, chacun pour la durée de son propre appel. Les seuls lecteurs qui la voient non liée sont donc
`write`/`write-to-string`. Une implémentation de `print-object` devrait lire son propre argument `escape` plutôt que
cette variable globale : cet argument porte la valeur que la directive a choisie.

**Ce que CL a et que ce langage n'a pas** : `*print-gensym*` (il n'y a pas de symboles non internés).

### 6.3 Remplacements temporaires

CL les lie avec `let`, mais dans ce langage `let` lie lexicalement ; utilisez donc `dlet`
([Divers](system.md#10-divers)) :

```lisp
(dlet ((*print-level* 2) (*print-length* 4))
  (println "~a" x))                 ; les limites ne s'appliquent qu'à cet affichage
(with-standard-io-syntax (println "~a" x))   ; afficher avec tout revenu aux valeurs standard
```

`with-standard-io-syntax` exécute son corps avec toutes les variables de contrôle de l'affichage à leurs valeurs
standard et `*read-eval*` à `true`.
