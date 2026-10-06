<!-- translated-from: docs/ja/reference/functions/system.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Temps, environnement et implémentation

Les fonctions de temps, les questions sur l'environnement d'exécution, les outils de l'implémentation, l'analyse et
l'évaluation de texte, les docstrings et les macros.

## 1. Temps

| Nom | Forme | Type | Description |
|---|---|---|---|
| `universal-time` | — | `defstruct` | Deux champs : `day` (jours depuis le 1900-01-01) et `second` (la seconde dans ce jour, 0..86399) |
| `internal-time` | — | `defstruct` | Deux champs : `second` et `microsecond` (dans cette seconde, 0..999999) |
| `get-universal-time` | `(get-universal-time)` | `()→universal-time` | Le temps écoulé depuis l'époque de CL (1900-01-01 UTC) |
| `get-internal-real-time` | `(get-internal-real-time)` | `()→internal-time` | Le temps écoulé, relatif au processus |
| `get-internal-run-time` | `(get-internal-run-time)` | `()→internal-time` | Le **temps CPU** utilisé par ce processus (utilisateur plus système) |
| `internal-time-seconds` | `(internal-time-seconds it)` | `internal-time→f64` | En nombre de secondes. La forme pour rapporter la différence entre deux mesures |
| `internal-time-units-per-second` | — | `int` | `1000000` (microsecondes), l'unité du champ `microsecond`. Comme en CL, la valeur est au choix de l'implémentation |
| `time` | `(time form)` | Macro | Exécute `form`, affiche le temps réel et le temps CPU sur une ligne chacun, et renvoie telle quelle la valeur de `form` |

Le temps réel et le temps CPU disent des choses différentes. Pour un travail qui attend surtout des E/S, les deux
diffèrent beaucoup, et c'est justement cette différence qu'on veut connaître ; `time` montre donc les deux.

`sleep`, qui arrête une tâche, se trouve dans [Tâches et canaux](concurrency.md#3-yield--sleep--céder-la-place).

## 2. Décoder et encoder des dates

| Nom | Forme | Type | Description |
|---|---|---|---|
| `decoded-time` | — | `defstruct` | **Neuf champs** : `second` / `minute` / `hour` / `date` / `month` / `year` / `day-of-week` / `daylight-p` / `zone`. Les neuf valeurs de retour de CL en une seule structure (il n'y a pas de valeurs multiples) |
| `decode-universal-time` | `(decode-universal-time ut &optional zone)` | `(universal-time,int)→decoded-time` | Le temps universel en composants calendaires. `zone` est en heures à l'ouest de Greenwich (la même direction que CL). **Omis, c'est l'heure locale** (comme en CL) |
| `encode-universal-time` | `(encode-universal-time sec min hour date month year &optional zone)` | `(int×6,int)→universal-time` | L'inverse. Sans `zone`, les arguments sont lus comme **heure locale** |
| `get-decoded-time` | `(get-decoded-time)` | `()→decoded-time` | Maintenant, décodé en heure locale |
| `timezone-offset-seconds` | `(timezone-offset-seconds day second)` | `(int,int)→Option<int>` | Le décalage de l'heure locale à l'ouest de Greenwich, en **secondes**, à ce temps universel |
| `timezone-daylight-p` | `(timezone-daylight-p day second)` | `(int,int)→Option<bool>` | Si l'heure d'été était en vigueur à ce temps universel |

Comme en CL, pour `day-of-week`, **0 est lundi et 6 dimanche**.

**Sans `zone`, l'heure locale est utilisée**, comme en CL. Le décalage local est demandé au système ; le résultat
dépend donc de l'endroit où se trouve la machine. **Donner une zone explicite le rend déterministe**, et `0` est UTC.

L'unité de `zone` est, comme en CL, « heures à l'ouest de Greenwich » ; UTC+9 se lit donc `-9`. Cependant,
**l'argument est un entier et le champ `zone` du résultat est un `f64`**. Les décalages réels ne sont pas toujours
des heures entières (l'Inde est à +5:30, le Népal à +5:45), et arrondir la valeur rapportée mentirait
silencieusement. Une zone écrite à la main est un nombre entier d'heures ; l'argument est donc `int`.

Quand `zone` est donnée, `daylight-p` vaut `false` et `zone` vaut exactement la valeur donnée, comme le spécifie CL
(*If a time-zone is supplied, daylight saving time information is ignored*).

Une heure locale qui tombe dans un changement d'heure n'est pas unique à la base, et CL ne dit pas laquelle prendre.
`encode-universal-time` renvoie l'une des deux réponses pour une telle heure.

## 3. L'environnement d'exécution

| Nom | Forme | Type | Description |
|---|---|---|---|
| `command-line-args` | `(command-line-args)` | `()→Vector<string>` | La ligne de commande. **L'élément 0 est le nom du programme** |
| `getenv` | `(getenv name)` | `string→Option<string>` | Une variable d'environnement. `none` si elle n'est pas définie ou pas en UTF-8 |
| `home-directory` | `(home-directory)` | `()→Option<string>` | `$HOME`. La base de `user-homedir-pathname` ([Noms de chemin](streams-files.md#92-fonctions)) |
| `lisp-implementation-type` | `(lisp-implementation-type)` | `()→string` | `"typelisp"` |
| `lisp-implementation-version` | `(lisp-implementation-version)` | `()→string` | La version de l'implémentation |
| `machine-type` | `(machine-type)` | `()→string` | L'architecture du CPU (`x86_64` / `aarch64` …). La valeur de la **cible de construction** |
| `machine-instance` | `(machine-instance)` | `()→Option<string>` | Le nom d'hôte |
| `machine-version` | `(machine-version)` | `()→Option<string>` | Le nom du matériel **en cours d'exécution** (`Apple M1` / `Intel(R) Xeon(R) …`). `none` là où il ne peut pas être déterminé |
| `software-type` | `(software-type)` | `()→string` | Le système (`macos` / `linux` …) |
| `software-version` | `(software-version)` | `()→Option<string>` | La version du système (`uname -r`, par exemple `24.6.0`) |
| `short-site-name` | `(short-site-name)` | `()→Option<string>` | Un nom court du site d'installation. **Toujours `none`** |
| `long-site-name` | `(long-site-name)` | `()→Option<string>` | De même, un nom long. **Toujours `none`** |

Celles qui renvoient une `Option` sont des éléments pour lesquels CL autorise `NIL` (*or nil if no such name can be
determined*). POSIX n'a pas d'endroit où enregistrer les noms de site ; ils sont donc toujours `none` ; SBCL renvoie
la même chose. Notez la différence entre `machine-type` et `machine-version` : le premier est l'architecture pour
laquelle ce binaire a été **construit**, le second la puce qui l'**exécute** maintenant.

L'élément 0 de `command-line-args` est le chemin du script pour `typl script.typl a b`, et l'exécutable lui-même pour
un exécutable AOT lancé avec `./prog a b`. **Les deux façons de lancer lisent les mêmes arguments aux mêmes
indices** (`typl` retire son propre nom et les options comme `--heap-cells` avant de les transmettre).

## 4. Interroger l'utilisateur

| Nom | Forme | Type | Description |
|---|---|---|---|
| `y-or-n-p` | `(y-or-n-p question)` | `string→bool` | Accepte un seul `y` / `n`. Redemande jusqu'à en obtenir un |
| `yes-or-no-p` | `(yes-or-no-p question)` | `string→bool` | Fait écrire en toutes lettres `yes` / `no`. Pour les questions où une erreur coûte cher |

Les deux lisent depuis `*standard-input*`. Seule la fin de l'entrée arrête les nouvelles questions, et le résultat
est alors `false`.

## 5. Outils de l'implémentation (CLHS 25.2)

La couche où l'implémentation répond à des questions sur elle-même. `heap-info` / `room` / `dribble` sont des
fonctions ordinaires ; `trace` / `untrace` / `step` / `disassemble` / `ed` sont des **formes spéciales**
(`trace` / `untrace` / `disassemble` / `ed` prennent le *nom* d'une définition, et `step` une *forme*, tous non
évalués).

| Nom | Forme | Type | Description |
|---|---|---|---|
| `heap-info` | `(heap-info)` | `()→heap-info` | L'état actuel du tas sous forme de structure. Les mêmes nombres que ceux qu'affiche `room` |
| `room` | `(room &optional verbose)` | `(bool)→()` | Rapporte `heap-info` sur `*standard-output*`. `(room true)` donne plus de détails |
| `dribble` | `(dribble &optional path)` | `(string)→Result<(),FileError>` | Commence à enregistrer la sortie de la session dans `path` / arrête l'enregistrement quand appelé sans argument |
| `trace` | `(trace name...)` | `Sexpr` | Rapporte sur `*trace-output*` les appels des définitions nommées. Renvoie la liste des noms actuellement tracés |
| `untrace` | `(untrace name...)` | `Sexpr` | Arrête le rapport. **Sans arguments, les retire tous** |
| `step` | `(step form)` | Le type de `form` | Évalue `form` en s'arrêtant à chaque appel pour demander |
| `disassemble` | `(disassemble name [llvm])` | `()` | Affiche ce que devient cette définition. Le code machine de l'hôte par défaut, LLVM IR avec `true` |
| `ed` | `(ed)` / `(ed name)` / `(ed "path")` | `Result<(),FileError>` | Lance `$VISUAL` / `$EDITOR`. Avec un nom, ouvre la ligne où cette définition est écrite |

`trace`/`untrace`/`step`/`disassemble` sont propres à l'interpréteur, et les fonctions qui les appellent ne peuvent
pas être compilées ([chapitre 10 de la référence de la syntaxe](../syntax.md#10-compilation)).

### 5.1 Champs de `heap-info`

| Champ | Type | Contenu |
|---|---|---|
| `capacity` / `live` / `free` | `int` | L'arène cons entière et sa répartition. Toujours `live + free = capacity` |
| `symbols` / `strings` / `boxes` | `int` | Les nombres actuels des trois autres sortes d'objets du tas |
| `gc-count` | `int` | Le nombre de collectes depuis le démarrage de l'implémentation |
| `growable` | `bool` | Si l'arène peut encore grandir |

Les champs sont tous `int` (sauf `growable`). La limite de croissance (voir la description de `typl --heap-cells`)
n'est pas rapportée, parce que ce que veut savoir un lecteur, c'est si elle peut encore grandir (`growable`).

### 5.2 Ce que `trace` / `step` voient ou non

- **Les définitions à corps compilé sont aussi visibles, depuis des endroits d'appel interprétés.**
- **Les endroits d'appel *à l'intérieur* du code compilé ne sont pas visibles.** Tracer un nom dont le corps est
  compilé ajoute une note d'une ligne le signalant. C'est la même limitation que SBCL décrit pour les appels
  locaux.
- **Les appels via des valeurs de fermeture (`funcall`/`apply`) ne sont pas visibles.** Les fermetures n'ont pas de
  nom.
- **Les définitions génériques ne sont pas couvertes.** Une copie par type est créée à chaque endroit d'utilisation ;
  il n'y a donc pas de corps unique à nommer (la même raison, et la même formulation, que lorsque `compile` refuse).

Les commandes de `step` sont `s` (entrer dans cet appel ; une ligne vide fait de même), `n` (sauter cet appel), `c`
(ne plus demander à partir d'ici) et `q` (abandonner). **Si l'entrée standard n'est pas un terminal, `step` évalue
simplement `form`** : un comportement dégénéré que CLHS autorise explicitement, afin que scripts et tests ne restent
pas bloqués sur une invite à laquelle personne ne peut répondre.

Le `$VISUAL` / `$EDITOR` de `ed` est découpé aux blancs ; `EDITOR="code -w"` fonctionne donc. Si aucun n'est défini,
le résultat est `Err` : il ne devine pas `vi`. Le numéro de ligne est passé en premier, sous la forme `+N`.

`dribble` enregistre les trois façons dont la sortie de la session quitte le processus : ce qu'écrivent
`print`/`println`/`format`, ce qui est écrit dans les flux reliés à la sortie standard, et les lignes tapées dans la
REPL avec les valeurs que la REPL renvoie.

## 6. Analyse et évaluation

Toutes traitent du texte et des données venant de l'exécution (que le programme lui-même ne contrôle pas) ; en cas
d'échec, elles renvoient donc l'`Err` d'un `Result` plutôt que de déclencher un panic. Les types d'erreur sont des
types concrets par opération ([Types d'erreur](option-result.md#3-types-derreur-et-le-trait-error)).

| Nom | Forme | Type | Description |
|---|---|---|---|
| `parse-int` | `(parse-int s &key radix junk-allowed)` | `string→Result<int,ParseIntError>` | Le `parse-integer` de CL. Saute les blancs de début et de fin (le même ensemble que `trim`), lit au plus un signe `+`/`-`, puis des chiffres en base `radix` (10 par défaut, 2 à 36 ; les chiffres au-delà de 10 dans l'une ou l'autre casse). Le nombre de chiffres n'est pas limité (`int`). Tout autre caractère restant donne `Err`. Avec `:junk-allowed true`, il s'arrête au premier non-chiffre et ignore le reste, mais donne `Err` s'il n'y a pas un seul chiffre (correspondant au `nil` de CL). Il ne renvoie pas la seconde valeur de CL (la position où la lecture s'est arrêtée). Un `radix` hors limites déclenche un panic (une erreur de l'appelant, pas du texte) |
| `parse-float` | `(parse-float s)` | `string→Result<f64,ParseFloatError>` | Un nombre à virgule flottante. Accepte aussi `inf`/`nan` |
| `read` | `(read s)` | `string→Result<Option<Sexpr>,ReadError>` | Lit un `Sexpr` depuis `s` (avec le même lecteur que celui qui lit le code source). Parenthèses déséquilibrées, chaînes non terminées et semblables donnent `Err`. La lecture depuis un flux se fait avec `read-sexpr` ([Flux](streams-files.md#6-fonctions-génériques-et-opérations-sur-les-fichiers)) |
| `read-from-string` | `(read-from-string s [start])` | `(string,int)→Result<cons-cell<Option<Sexpr>,int>,ReadError>` | `read` plus **la position où la lecture s'est arrêtée**. `(car r)` est la valeur et `(cdr r)` la position du prochain caractère à lire. `start` vaut 0 par défaut |
| `read-from-string-preserving-whitespace` | Idem | Idem | Idem, mais ne consomme pas le blanc qui a terminé la donnée. La différence se voit dans la position renvoyée |
| `eval` | `(eval form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Vérifie les types de `form` à l'exécution et l'évalue. Suit l'`eval` de CL |

CL renvoie **deux valeurs** (la valeur et la position) depuis `read-from-string`, mais ce langage n'a pas de valeurs
multiples ; il renvoie donc une `cons-cell`. Avoir la position fait de la lecture d'une chaîne donnée par donnée une
boucle plutôt qu'un nouveau parcours :

```lisp
(let ((s "1 2 3") (i 0) (going true))
  (while going
    (match (read-from-string s i)
      ((ok p) (progn (println "~s" (car p)) (setf i (cdr p))
                     (if (>= i (length s)) (progn (setf going false) ()) ()) ()))
      ((err e) (progn (setf going false) ())))))
```

La différence que fait `preserving-whitespace` est **un caractère blanc** : le `read` de CL consomme le blanc qui a
terminé la donnée, et `read-preserving-whitespace` le laisse. `(read-from-string "12 34")` renvoie la position 3, et
la version qui préserve renvoie 2.

La syntaxe des nombres qu'accepte le lecteur se trouve au
[chapitre 1 de la référence de la syntaxe](../syntax.md#1-éléments-lexicaux). Ce qu'affiche `*print-radix*`
([Affichage](printing.md#62-base-casse-et-lisibilité)) peut être relu tel quel. Il n'y a pas de `*read-base*` de CL.

### 6.1 Ce que signifie `eval`

Il suit l'`eval` de CLHS : il évalue dans **l'environnement global courant** (fonctions, variables, types et macros
globaux, y compris les définitions ajoutées à l'exécution) et dans **l'environnement lexical nul** (les liaisons
locales du `let`/`lambda` de l'appelant ne sont pas visibles). On peut évaluer aussi bien des expressions que des
définitions (`defun`/`defvar`/`defstruct`/`defenum`/`defmacro`), et les définitions sont enregistrées dans
l'environnement global immédiatement et durablement.

```lisp
(eval (unwrap (read "(+ 40 2)")))                 ; => (ok 42)
(defvar (x i32) 10)
(eval (unwrap (read "(+ x 5)")))                  ; => (ok 15)  ; le x global est visible
(eval (unwrap (read "(defun sq ((n i32)) i32 (* n n))")))  ; => (ok sq)  ; renvoie le nom défini
(eval (unwrap (read "(sq 9)")))                   ; => (ok 81)  ; la définition qui vient d'être faite est visible
```

- **Valeur de retour** : pour une expression, le résultat sous forme d'`Option<Sexpr>` ; pour une définition, le
  symbole du nom défini (comme en CL). Pour utiliser le résultat, décomposez le `Sexpr` avec `match`
  (`(int n)`/`(str s)`/…).
- **Différences dues aux types statiques (important)** : CL renvoie la valeur réelle du résultat, mais dans ce
  langage, le type de retour ne peut être qu'uniformément `Result<Option<Sexpr>,EvalError>`. De plus, **le code écrit
  statiquement ne peut pas faire référence par anticipation à des noms qu'`eval` définit à l'exécution** : un
  `(sq 9)` écrit directement dans un fichier est vérifié avant que l'`eval` qui définit `sq` ne s'exécute, et il est
  « indéfini ». En revanche, **les `eval` suivants le voient** (leur vérification a lieu à l'exécution, après la
  définition). La REPL vérifie et exécute une ligne à la fois ; un nom défini avec `eval` peut donc être appelé
  directement dès la ligne suivante.
- **Erreurs** : les erreurs de type et de syntaxe renvoient `Err` (pas de panic). Les **panics à l'exécution** dans le
  code évalué (division par zéro, etc.) se propagent comme pour du code écrit directement. Le nettoyage de tout
  `unwind-protect` intermédiaire s'exécute
  ([chapitre 8 de la référence de la syntaxe](../syntax.md#8-sorties-non-locales-catch--throw--unwind-protect)).
- **Espace de noms** : lancé par `typl file.typl` et à l'intérieur d'un exécutable AOT, `eval` évalue dans l'espace
  de noms du module du script (les globaux propres au script sont visibles). La REPL évalue dans l'espace de noms
  racine.
- **Compilation** : `read` et `eval` peuvent tous deux être compilés. La façon dont ils sont traités dans les
  exécutables AOT, et les conséquences (les formes passées à eval sont interprétées), se trouvent dans
  [Référence de la syntaxe 10.2](../syntax.md#102-eval-dans-les-exécutables-aot).

## 7. Docstrings / `documentation`

`defun`/`defmethod` (y compris dans `impl`)/`defmacro`/`defvar`/`defconstant`/`defstruct`/`defenum`/`deftype`/
`deftrait` peuvent porter des docstrings. La position suit la règle de CL pour chacun :

| Forme | Position de la docstring |
|---|---|
| `defun` / `defmethod` / `defmacro` | Au début du corps (après le type de retour et la clause `where`). Seulement si au moins une forme de corps suit ; une chaîne seule reste la valeur de retour |
| `defvar` / `defconstant` | **Après** la valeur initiale : `(defvar (name Type) value "doc")` |
| `defstruct` / `defenum` | **Juste après** le nom, avant les champs/variantes |
| `deftype` | **Juste après** le nom, avant le type : `(deftype meters "doc" i32)` |
| `deftrait` | Juste après la liste des super-traits, avant les éléments. Une pour tout le trait. **Les méthodes avec implémentation par défaut** peuvent placer leur propre docstring juste avant leur corps |

| Nom | Forme | Type | Description |
|---|---|---|---|
| `documentation` | `(documentation name)` | (forme spéciale ; `name` est un symbole nu ou `Type::method`)→`Option<string>` | Renvoie la docstring de `name` |

Comme `quote`/`compile`, `documentation` est une forme spéciale (elle lit `name` comme un nom non évalué).
Contrairement au `(documentation 'name 'function)` de CL, elle ne prend pas d'argument de type ; elle résout plutôt un
nom nu dans l'ordre **variable → fonction → type → trait → macro** (la même priorité que pour un identificateur nu
évalué comme expression). La forme `Type::method` cherche la docstring d'une méthode associée ou statique.

```lisp
(defun square ((n i32)) i32
  "Returns n squared."
  (* n n))

(unwrap-or (documentation square) "no docs")   ; => "Returns n squared."

(defstruct point
  "A 2D point."
  (x i32)
  (y i32))

(unwrap-or (documentation point) "no docs")    ; => "A 2D point."
```

**La valeur est décidée à la vérification** : si le nom ne se résout vers aucune définition, c'est une erreur à la
vérification (comme une référence à une variable non définie). S'il se résout mais qu'il n'y a pas de docstring, le
résultat est `Option::none`.

**Non couvert** :

- `(setf documentation)` (modifier une docstring à l'exécution) n'existe pas.
- Les noms libres qualifiés par un module (`mod::name` ; `Type::method` est pris en charge) ne sont pas pris en charge.
- Une déclaration de méthode dans un `deftrait` **sans corps** ne peut pas avoir de docstring. Une chaîne littérale
  finale serait elle-même le corps (la valeur de retour) d'une implémentation par défaut ; il n'y a donc aucun moyen
  de distinguer les deux.

Le survol du serveur de langage (`typl-lsp`) affiche aussi les docstrings.

## 8. Macros

La façon de définir des macros se trouve dans
[Référence de la syntaxe 3.14](../syntax.md#314-defmacro--définitions-de-macros).

| Nom | Forme | Type | Description |
|---|---|---|---|
| `gensym` | `(gensym)` / `(gensym prefix)` | `(&optional string)→symbol` | Un nouveau symbole. Son nom est `" <prefix><n>"`, où `n` est `*gensym-counter*`. Un espace initial ne peut pas s'écrire dans le source ; les liaisons générées n'entrent donc jamais en conflit avec des noms écrits |
| `*gensym-counter*` | Variable | `int` | Le nombre que `gensym` utilisera ensuite. Comme en CL, il peut être lu et modifié |
| `macroexpand-1` | `(macroexpand-1 form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Développe un appel de macro d'un pas. `none` signifie « pas un appel de macro » |
| `macroexpand` | `(macroexpand form)` | `Option<Sexpr>→Result<Option<Sexpr>,EvalError>` | Répète jusqu'à ce que ce ne soit plus une macro |

`macroexpand-1` renvoie une `Option`. CL signale « s'il y a eu développement » par une seconde valeur de retour, mais
il n'y a pas de valeurs multiples ; `none` joue donc ce rôle. **Une macro qui se développe en un appel d'elle-même
ne peut jamais être confondue avec une non-macro.** Un pas de développement est le même que celui qu'utilise le
vérificateur de types ; ce que voit le programme et ce qu'a vu le vérificateur ne divergent donc jamais.

```lisp
(defmacro twice (x) `(+ ,x ,x))
(macroexpand-1 '(twice 5))   ; => (ok (+ 5 5))
(macroexpand-1 '(+ 1 2))     ; => (ok ())      ; none s'affiche comme la liste vide (Option<Sexpr> est transparent)
(macroexpand '(when true 1)) ; => (ok (if true (progn 1 ()) ()))
```

Ce que CL a et que ce langage n'a pas : `eval-when` (`:compile-toplevel`/`:load-toplevel`/`:execute` coïncident
toujours ; il n'y a donc pas de distinction à choisir), `define-compiler-macro`, `load-time-value`,
`make-symbol`/`copy-symbol`/`gentemp` (symboles non internés ; les liaisons se cherchent par nom, il n'y aurait donc
rien à y gagner).

## 9. Liaisons de macros locales (`macrolet` / `symbol-macrolet`)

Les deux sont des formes spéciales qui lient lexicalement **des noms qui ne sont pas des valeurs**. Rien ne subsiste à
l'exécution : ce qui est compilé, c'est la forme développée du corps.

```lisp
(macrolet ((twice (x) `(+ ,x ,x)))
  (twice 21))                       ; => 42

(let ((v (the Vector<i32> (Vector::new))))
  (progn (push v 7)
    (symbol-macrolet ((head (get v 0)))
      (progn (setf head 42) head))))  ; => 42
```

- Une liaison `macrolet` masque une macro globale de même nom **seulement pendant le corps**. La liste lambda est la
  même que pour `defmacro` (`&optional`/`&rest`/`&key`).
- **Les membres d'un même `macrolet` ne se voient pas mutuellement depuis leurs *corps*** (comme en CL ; c'est la
  différence avec `labels`). Les développements sont vérifiés à l'endroit d'utilisation ; `earlier` qui se développe
  en `(later ...)` fonctionne donc : les deux sont visibles à cet endroit.
- Un nom de `symbol-macrolet` entre dans l'environnement comme une liaison ordinaire. Un `let` intérieur masque donc
  le même nom, et une variable extérieure est masquée : les règles de CL en découlent telles quelles.
- **`setf` écrit dans le développement.** `(setf head 42)` est `(setf (get v 0) 42)`.
- Les développements sont vérifiés dans **l'environnement de l'endroit d'utilisation** (pas de l'endroit de liaison).

## 10. Divers

| Nom | Forme | Type | Description |
|---|---|---|---|
| `assert` | `(assert test)` / `(assert test msg)` | `(bool[,string])→()` | Panic si faux. Sans message, `assertion failed: <le test tel qu'écrit>` (c'est une macro, elle peut donc nommer l'expression elle-même). Les restarts de CL n'existent pas dans ce langage |
| `warn` | `(warn control args...)` | `(string,...)→()` | Écrit une ligne préfixée par `WARNING: ` sur `*error-output*` et **continue**. Un moyen de signaler quelque chose sans renvoyer de `Result` et sans terminer le programme |
| `dlet` | `(dlet ((*var* val)...) body...)` | — | Remplace des globales seulement pendant `body` et les restaure à la sortie. CL écrit cela avec `let`, mais `let` lie toujours lexicalement dans ce langage, d'où le nom distinct (le même rôle que la macro de même nom d'Emacs Lisp). Les restaure quelle que soit la façon dont le corps est quitté : fin normale, `throw`, `panic`, `break`/`return`. **Pas une liaison par tâche** |
| `with-standard-io-syntax` | `(with-standard-io-syntax body...)` | — | Exécute `body` avec chaque variable de contrôle de l'affichage à sa valeur standard et `*read-eval*` à `true` ([Affichage](printing.md#6-contrôler-la-quantité-imprimée)) |
| `exit` | `(exit code)` | `int→!` | Termine le processus |
| `dump` | `(dump path)` | `string→bool` | Écrit l'environnement courant (informations de type plus corps compilés) dans un fichier. `typl --image <path>` repart de celui-ci. Propre à l'interpréteur ([Référence de la syntaxe 10.1](../syntax.md#101-dumps)) |
