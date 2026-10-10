<!-- translated-from: docs/ja/reference/syntax.md @ 37af68009626057caa98d1dc23e3879b42e983c7 -->
# Référence de la syntaxe de typelisp

typelisp est un Lisp à typage statique, écrit en S-expressions. Pour la liste des fonctions et méthodes intégrées,
voir [Fonctions intégrées](functions/README.md) ; pour la liste des types, [types.md](types.md) ; et pour lire les
messages d'erreur, [errors.md](errors.md).

## 1. Éléments lexicaux

- **Insensible à la casse.** Les symboles sont tous normalisés en minuscules à la lecture.
- **Commentaires** : de `;` à la fin de la ligne (commentaires de ligne). `#| ... |#` (commentaires de bloc, qui
  peuvent s'imbriquer).
- **Évaluation à la lecture** : `#.(expr)` **exécute la forme qui suit pendant la lecture** et traite sa valeur
  comme ce qui a été lu. C'est le seul endroit où le lecteur est plus qu'une fonction du texte. Jusqu'où cela porte
  dépend du chemin de lecture, comme en CL :
  - `(load ...)` et la REPL évaluent une forme à la fois ; on peut donc appeler **des fonctions définies plus tôt
    dans le même texte** (le `load` de CL).
  - Un fichier de module est vérifié comme une unité et exécuté par celui qui l'importe avec `use` ; `#.` n'atteint
    donc que la bibliothèque standard et ce que la session a déjà exécuté. Ni les définitions propres du fichier ni
    celles des modules qu'il importe avec `use` **ne se sont encore exécutées** (de même que le `compile-file` de CL
    a besoin d'`eval-when`).
  - `read` / `read-from-string` à l'intérieur d'un programme évaluent aussi `#.` (comme en CL).
  - Mettre `*read-eval*` (par défaut `true`) à `false` fait de `#.` une erreur de lecture partout : un interrupteur
    pour empêcher du texte lu comme donnée d'exécuter du code (comme en CL). Il est consulté à chaque `#.` ; un
    `setf` prend donc effet à partir de la forme lue suivante. À l'intérieur de `with-standard-io-syntax`, il vaut
    `true`.
- **Booléens** : `true` / `false`.
- **Entiers** : décimaux (`42`, `-7`). Un signe `+`/`-` peut précéder. Les autres bases s'écrivent avec la syntaxe
  de base de CL `#b`/`#o`/`#x`/`#NNr` (le signe vient après le marqueur : `#x-ff`). Le préfixe `0x` n'existe pas en
  CL et n'est pas adopté : `0xff` se lit comme un symbole.
  Un littéral entier sans annotation de type est `int` par défaut (précision arbitraire,
  [Nombres](functions/numbers.md#3-entiers-en-précision-arbitraire-int)), sans limite supérieure de taille. **Si le
  type attendu est un type entier de largeur fixe, le littéral prend ce type, et on vérifie que le type peut
  contenir la valeur** : `(the u8 300)` est une erreur de type (si vous voulez la tronquer, écrivez `(as u8 300)`).
  `(the u32 4294967295)` et `(the u32 #xFFFFFFFF)` peuvent s'écrire grâce à cette règle. Qu'une valeur `int` tienne
  dans une valeur immédiate de 63 bits ou devienne un bignum est décidé par sa taille, sans syntaxe particulière
  (comme en CL).
- **Nombres à virgule flottante** : ceux qui contiennent un point décimal ou un exposant (`e`/`E`) (`1.5`,
  `3.0e10`). `f64` par défaut (`f32` si c'est le type attendu).
- **Fractions** : `numérateur/dénominateur` (décimal uniquement, par exemple `1/3`). Réduites à la lecture, comme le
  spécifie CL (`2/4` est `1/2`). Celles qui ont une valeur entière (`4/2`, etc.) se lisent comme `int`, pas comme
  `ratio`. Un dénominateur nul (`1/0`) est une erreur de lecture.
- **Caractères** : `#\` suivi d'un caractère ou d'un nom de caractère. Par exemple `#\a` `#\Space` `#\Newline`
  `#\Tab` `#\Return` `#\Page` `#\Nul` (aussi `#\Null`) `#\Backspace`. Les noms sont insensibles à la casse.
- **Chaînes** : `"..."`. Les échappements sont `\n` `\t` `\r` `\0` `\\` `\"` (tout autre `\x` est simplement `x`).
- **Symboles** : tout lexème contenant lettres, chiffres et symboles (`+` `<=` `my-func`, etc.).
  `]` et `}` terminent un jeton : ils ne peuvent donc pas figurer dans un symbole, et en rencontrer
  un au début d'une donnée est une erreur de lecture. `[` et `{` peuvent figurer dans un symbole :
  comme en CL, ils sont laissés libres pour que le programmeur s'en serve dans des [macros de
  lecture](#11-macros-de-lecture-readtable).
- **Mots-clés** : les symboles qui commencent par un deux-points, comme `:name` (comme en CL). Ils s'évaluent en
  eux-mêmes : ils ne cherchent aucune liaison et leur valeur est eux-mêmes, de type statique `symbol`. Les mots-clés
  de même nom sont toujours le même objet (`(eq :foo :FOO)` est vrai ; comme les autres symboles, ils passent en
  minuscules). Le deux-points fait lui-même partie du nom ; `(symbol->string :foo)` vaut donc `":foo"` (typelisp n'a
  pas de système de paquetages, ce qui diffère donc du `symbol-name` de CL). Un `:` seul ou un nom avec d'autres
  deux-points comme `:a:b` est une erreur de lecture. On teste avec `keywordp`. Ceux qui commencent par `::` ne sont
  pas des mots-clés mais des chemins absolus (ci-dessous).
  Notez que `:dyn` est un mot-clé réservé aux positions de type ; l'écrire ailleurs est une erreur (voir le
  [chapitre 2](#2-écrire-les-types)).
- **Listes** : `(a b c)`. Les paires pointées `(a . b)` peuvent aussi se lire.
- **Vecteurs** : `#(1 2 3)` (comme en CL). Le contenu n'est fait que de littéraux et n'est pas
  évalué : le `a` de `#(a b)` est un symbole, pas une variable. Le type des éléments vient du
  contexte (`(the Vector<i32> #(1 2))`), ou du premier élément en l'absence de contexte (`#(1 2 3)`
  est un `Vector<int>`). Tous les éléments doivent avoir le même type : `#(1 "a")` est une erreur de
  type, de même que `#()` sans éléments ni contexte. Chaque évaluation crée un nouveau vecteur. Là
  où des données S-expression sont attendues (`(the Option<Sexpr> #(1 x))`, `'#(..)`, ce que renvoie
  `read`), c'est un `Vector<Option<Sexpr>>` dont les éléments sont tous des données : la variante
  `vector` de `Sexpr`.
- **Tableaux** : `#2A((1 2) (3 4))` (comme en CL). Le nombre entre `#` et `A` est le rang, et autant
  de premiers niveaux d'imbrication des listes du contenu sont les dimensions. `#0A x` est un
  tableau de dimension zéro contenant un élément. Des listes d'un même niveau de longueurs
  différentes sont une erreur de lecture. Le type se décide comme pour les vecteurs et est un
  `Array<T>` (sans éléments, le contexte doit le fournir, comme dans `(the Array<f64> #2A(()))`). En
  tant que donnée S-expression, c'est un `Array<Option<Sexpr>>` : la variante `array` de `Sexpr`.
- **La liste vide `()`** : selon le contexte, la valeur du type `Unit` ou le `none` de `Option<Sexpr>`. **`Sexpr` n'a
  pas de variante liste vide** : `Sexpr` signifie « une S-expression non vide », et le type des données S-expression
  est `Option<Sexpr>` (voir « Motifs pour `Option<Sexpr>` » dans [4.3 match](#43-match--filtrage-par-motifs)).
- **quote/quasiquote/unquote** :
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (n'a de sens qu'à l'intérieur d'un quasiquote)
  - `,@x` → `(unquote-splicing x)` (inséré comme éléments de liste au développement)
- **Chemins `::`** : `foo::bar` se lit comme un chemin à travers des modules, des types et des membres (pas comme un
  seul nom de symbole). Celui qui commence par `::`, comme `::foo`, est un chemin absolu depuis la racine. Un `::` à
  l'intérieur d'arguments génériques (`Vec<a::b>` et semblables) n'est pas traité comme séparateur de chemin.

## 2. Écrire les types

Dans le source, les types s'écrivent comme des symboles ou des listes ordinaires.

- **Types primitifs** : `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string` `symbol`. `int`
  est le type entier (l'integer de CL, qui passe automatiquement entre valeurs immédiates de 63 bits et bignums ;
  [Nombres](functions/numbers.md#3-entiers-en-précision-arbitraire-int)), et les six types de largeur fixe sont
  nommés d'après leur largeur et leur signe (il n'y a pas de type entier sur 64 bits ; voir
  [Nombres](functions/numbers.md#1-entiers-de-largeur-fixe)).
- **Le type rationnel** : `ratio` (rationnels irréductibles). Alloué sur le tas comme en CL, sans conversion
  implicite avec `int`/`f64` et semblables (convertissez explicitement avec `as`/`try-as` ou une méthode de
  conversion ; voir [Nombres](functions/numbers.md#5-rationnels-ratio)).
- **Mots bruts à la frontière C** : `ptr` (un pointeur opaque), `c-long` / `c-ulong`. Uniquement pour le FFI : en
  faire une valeur exige `(unsafe ...)`, et les endroits où ils peuvent apparaître sont limités
  ([3.3 defffi](#ptr--c-long--c-ulong--mots-machine-bruts)). Ne les utilisez pas là où vous voulez un entier de
  64 bits : ils n'ont pas d'arithmétique.
- **Types mutables opaques** : `random-state` (l'état d'un générateur de nombres aléatoires). Il ne peut pas aller
  dans `Vector<T>`/`HashTable<K,V>`/`Sexpr` (il peut aller dans `Option<T>`/`Result<T,E>`).
- **Le type Unit** : `()`
- **Le type Never** : `!` (le type des expressions divergentes comme `panic`/`unreachable`/`todo`/une boucle qui ne
  revient jamais. Il convient à tout type attendu)
- **Types de fonctions** : `(fn (types-des-arguments...) type-de-retour)`. Le type d'une fonction à arguments
  variadiques est `(fn (types-des-arguments... &rest type-d'élément) type-de-retour)`.
- **Types génériques** : `Name<T1,T2,...>` (lu comme un seul lexème, sans espaces).
  Par exemple `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`.
  Le type unit `()` peut aussi s'écrire comme argument de type (`Result<(), FileError>`). `(`/`)` sont normalement
  des délimiteurs qui terminent un lexème, mais tant qu'un chevron est ouvert, cette seule paire de caractères est
  admise. `()` peut aussi servir de type de champ ou d'argument.
- **La forme d'application des types génériques** : `(Name T1 T2 ...)`, une écriture en liste qui désigne le même
  type que `Name<T1,T2,...>`. Par exemple `(vector char)` est identique à `Vector<char>`.
  La forme nom est l'écriture habituelle ; cette forme **existe pour quand un argument de type ne peut pas s'écrire à
  l'intérieur d'un nom** : un argument de type est lui-même une expression de type, mais à l'intérieur d'un nom d'un
  seul lexème, on ne peut écrire que des noms, `()` et `:dyn`, pas des types de fonctions (il n'existe pas
  d'écriture comme `Vector<(fn (i32) i32)>`). Elle peut aussi apparaître sous cette forme quand l'implémentation
  affiche un type, comme le résultat de la substitution du type associé d'un trait dans une signature.
- **Noms de types qualifiés** : peuvent être qualifiés avec `::`, comme `module::Type`.
- **Types d'objet trait** : `:dyn Trait` (deux mots séparés par un espace formant un seul type). Représente une
  valeur dont le type concret est décidé à l'exécution ; les appels de méthodes de trait passent par une vtable
  (dispatch dynamique). Pour un trait à types associés, ceux-ci sont fixés par position dans l'ordre de
  déclaration (`:dyn Iter<i32>` fixe `Item` à `i32`). Il peut aussi s'écrire dans des arguments génériques :
  `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`. Les valeurs concrètes sont mises en boîte
  automatiquement aux positions attendues ; la forme explicite est `(as :dyn Trait expr)`.
  Une valeur de `:dyn Sub` peut être passée telle quelle là où un `:dyn Super` de l'un de ses super-traits (tout ce
  dont il hérite, transitivement) est exigé (transtypage ascendant). Elle ne peut pas être passée à un trait sans
  rapport.
  Pour les conditions qu'un trait doit remplir pour s'utiliser avec `:dyn`, voir
  [3.9 deftrait / impl](#39-deftrait--impl--traits). Écrire `:dyn` en dehors d'une position de type est une erreur.
- Types génériques intégrés : `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>`, et les types de concurrence `Task<T>` / `Thread<T>` / `Chan<T>`
  ([chapitre 12](#12-concurrence-tâches)). Il y a aussi `Sexpr`, le type des données S-expression. Les types
  d'erreur concrets intégrés sont `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` / `FileError` /
  `NetError`, et la bibliothèque standard a les structures `SimpleError` / `WrappedError` (`Error` n'est pas un type
  mais un trait : utilisez-le sous la forme `:dyn Error`). La liste se trouve dans [types.md](types.md).
- **Les types et les traits partagent un même espace de noms** (comme en Rust) : dans un module, un type
  (`defstruct`/`defenum`) et un trait (`deftrait`) ne peuvent pas avoir le même nom.

## 3. Définitions de niveau supérieur

### 3.1 defun — définitions de fonctions

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Les types des arguments et le type de retour sont obligatoires.
- Une fonction générique écrit ses paramètres de type entre chevrons après son nom :
  `(defun name<T1,T2...> (params) Ret body...)` (la même syntaxe à chevrons que `Vector<T>` aux positions de type).
- `defun`/`lambda`/`defmethod` acceptent des arguments variadiques quand `&rest (name Type)` est écrit à la fin :
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (dans le corps, `xs` est toujours lié comme
  `Option<Sexpr>`, une liste S-expression. Chaque argument effectif de l'appel est vérifié individuellement comme
  `Type2` puis enveloppé en `Sexpr`).
  `defmacro` a aussi son propre `&rest`, mais il diffère en ce qu'il est toujours un `Sexpr` non typé
  (`defun`/`lambda` indiquent le type des éléments). Un type de fonction peut aussi décrire une fonction
  variadique, sous la forme `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (pour `defun` et `defmethod` ; pas pour `lambda`/`labels`, pour la raison ci-dessous, et
  `defmacro` a une implémentation séparée, également ci-dessous). L'ordre est celui de CL :
  `required &optional &rest &key`. Chaque paramètre s'écrit `(name Type)` ou `(name Type default-expr)` :

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; sans valeur par défaut
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> dans le corps

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; avec valeur par défaut
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; l'appelant écrit `:nom valeur`, dans n'importe quel ordre ; ceux omis prennent leur valeur par défaut
  ```

  - **Un paramètre sans expression par défaut a le type `Option<Type>`.** Omis, il vaut `none` ; passé, la valeur nue
    écrite par l'appelant est automatiquement enveloppée dans `some`. Ce que CL fait avec une variable supplied-p
    (« a-t-il été fourni ? ») apparaît ici du côté du type statique.
  - Avec une expression par défaut, le type reste `Type` tel que déclaré. En cas d'omission, cette **expression
    vérifiée** est insérée telle quelle dans l'appel (évaluée à chaque appel).
  - **`&key` ne peut pas être mélangé avec `&optional`/`&rest` dans une même liste d'arguments.** Cela évite une
    ambiguïté qu'a CL lui-même (qu'un argument effectif final soit pris par un `&optional` positionnel ou associé par
    étiquette comme `&key` dépend des *valeurs*) en interdisant la combinaison. `&optional` et `&rest` peuvent
    s'utiliser ensemble.
  - On peut les utiliser dans des fonctions génériques, mais **un paramètre de type qui n'apparaît que dans des
    arguments omis ne peut pas être inféré et est une erreur** (il n'y a pas de valeur à confronter).
  - **`defmethod` peut avoir les mêmes trois sections** (pour les méthodes d'instance comme pour les fonctions
    statiques). Énumérez `&optional`/`&rest`/`&key` après le receveur :

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; fonction statique
    (point::origin :y 7)
    ```

    On peut aussi les utiliser dans les méthodes de types génériques, mais **le type d'un paramètre avec expression
    par défaut ne peut pas mentionner les paramètres de type du propriétaire** (la même restriction que `defun` a
    pour ses propres paramètres de type : ce qui est inséré quand l'argument est omis est une expression *vérifiée* ;
    son type ne peut donc pas rester une variable abstraite).
  - **Ils ne peuvent pas s'utiliser dans les méthodes de traits.** `deftrait` n'a pas de syntaxe pour eux, et si seul
    le côté `impl` pouvait déclarer des sections, les appels avec un receveur `:dyn` (qui complètent les arguments
    d'après la déclaration du trait) et les appels avec un receveur concret (qui les complètent d'après celle de
    l'`impl`) deviendraient des choses différentes. L'arité d'une entrée de vtable est fixe.
  - **Ils ne peuvent pas s'utiliser dans `lambda` / `labels`** (`&rest`, si). Pour compléter un argument omis,
    l'appelant doit lire **l'expression par défaut vérifiée de l'appelé**, qui n'est disponible que dans une
    signature résolue par nom. Un `lambda` circule comme valeur, et la seule chose qui décrit cette valeur est son
    type de fonction `(fn ...)` : il n'y a pas de place pour une expression, et s'il y en avait, « deux lambdas de
    même signature mais avec des valeurs par défaut différentes » deviendraient des types différents. `&rest` reste
    dans le domaine des types ; il peut donc s'écrire dans un type de fonction.
- **Les références anticipées se déclarent avec `defsignature`** (ci-dessous). Un nom non déclaré ne peut pas être
  appelé avant sa définition, parce que le niveau supérieur est vérifié et exécuté une forme à la fois, dans l'ordre
  du source.
- Pour exiger des bornes de traits, écrivez une clause `where` juste avant le corps :
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (fixer un type associé avec `(AssocName ConcreteType)` est facultatif).
- **Docstrings** : un littéral chaîne au début du corps, juste après la clause `where` (s'il y en a une), devient la
  docstring (comme en CL). Seulement si au moins une forme de corps suit, cependant : une chaîne seule reste la
  valeur de retour et n'est pas prise comme docstring : `(defun f () string "doc" "value")` a une docstring et
  renvoie `"value"`, tandis que `(defun f () string "value")` n'a pas de docstring et renvoie `"value"`. On la
  récupère avec `(documentation name)` ([docstrings](functions/system.md#7-docstrings--documentation)).

### 3.2 defsignature — déclarations anticipées

```lisp
(defsignature name (argument-types...) return-type)
(pub defsignature name (argument-types...) return-type)
```

Pour appeler un `defun` défini **plus loin** que soi, déclarez-le d'abord ainsi. La récursion mutuelle ne peut
s'écrire que de cette façon :

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Les arguments sont énumérés **par leurs types seulement** ; il n'y a pas de corps, donc rien à nommer. `&rest` peut
s'écrire en dernier, sous la forme `&rest type-d'élément`.

Les déclarations **sont vérifiées** :

- La définition qui suit doit correspondre à la déclaration (nombre et types des arguments, type de retour, `&rest`
  et présence de `pub`). Une discordance est une erreur à la définition.
- Déclarer sans définir est une erreur (signalée quand le fichier / module a fini de se charger). La REPL ne la
  signale pas après chaque saisie, car une déclaration et sa définition doivent pouvoir se taper sur des lignes
  séparées.
- Une déclaration placée **après** la définition est une erreur, puisqu'une telle déclaration ne pourrait rien faire.

Trois choses ne peuvent pas être déclarées :

- **Les fonctions génériques.** Créer une copie par type exige le corps, et une déclaration n'en a pas. Un appel
  anticipé pourrait être résolu mais l'instanciation échouerait ; la déclaration est donc refusée d'emblée.
- **`&optional`/`&key`.** Leur signature inclut l'expression **vérifiée** de chaque valeur par défaut (insérée dans
  l'appel quand l'argument est omis), et une déclaration n'a pas de place pour elle.
- **Tout ce qui n'est pas `defun`.** Un `defmacro` a besoin que le corps de la macro se soit **déjà exécuté** pour
  pouvoir développer, ce que l'enregistrement d'une signature ne peut pas remplacer. Pour les types
  (`defstruct`/`defenum`/`deftrait`), les enregistrer est « ce dont a besoin le code qui enregistre le type
  lui-même », ce qui n'est pas autonome comme une signature. Un `defmethod` est enregistré sur le type qui le
  possède ; il suit donc le type.

L'équivalent en CL est `(declaim (ftype (function (i32) bool) even2))`, mais cela s'accompagne de tout un système de
déclarations et n'est que **consultatif**. Ici, grâce au typage statique, les déclarations sont vérifiées.

### 3.3 defffi — déclarer des fonctions C (FFI)

```lisp
(defffi (name "c_symbol") (argument-types...) return-type)
(defffi (name "c_symbol") (argument-types...) return-type :library "name")
(defffi name (argument-types...) return-type)              ; name = le nom de symbole C
(pub defffi ...)
```

Déclare une fonction C pour pouvoir l'appeler. La forme est la même que `defsignature` (un nom, des types
d'arguments, un type de retour et pas de corps), mais l'absence de corps signifie autre chose. `defsignature` est une
promesse « je la définirai plus tard », tandis que `defffi` déclare « quelqu'un d'autre a déjà écrit et compilé le
corps ».

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

Le nom typelisp et le nom de symbole C peuvent s'écrire séparément parce que les identificateurs typelisp
contiennent généralement `-`, ce que ne peuvent pas les identificateurs C. Si le nom C est omis, le nom est utilisé
tel quel comme nom de symbole C.

**Les appels exigent `(unsafe ...)`** (même pour les fonctions qui ne prennent que des scalaires). Le compilateur n'a
aucun moyen de confirmer que la signature C déclarée correspond à la réelle et ne peut que faire confiance à la
déclaration ; `unsafe` est la marque que vous en prenez la responsabilité. L'usage prévu est de l'envelopper une
fois pour en faire une enveloppe sûre :

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; plus besoin de unsafe à partir d'ici
```

Les types qu'on peut écrire sont `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void) `string` `ptr`
`c-long` `c-ulong`, ainsi que les pointeurs typés `(ptr T)`
([ci-dessous](#def-c-struct-et-pointeurs-typés--allouer-des-structures-c)).

`string` est `const char *`. Les chaînes typelisp ne sont pas terminées par NUL et peuvent elles-mêmes contenir NUL ;
**elles sont donc copiées dans une chaîne C au moment du passage**, puis libérées après l'appel. Un NUL dans la
chaîne est une erreur : C ne regarderait que jusqu'à lui, et une autre chaîne serait passée silencieusement.

**Les chaînes renvoyées sont aussi copiées**, et non libérées : ce que renvoie C appartient à C, et peut pointer dans
une table statique, comme avec `getenv`. Les fonctions qui renvoient une mémoire que l'appelant doit libérer
(`strdup`, etc.) doivent être reçues comme `ptr` et libérées par vous-même.

Les fonctions dont le résultat pointe à l'intérieur d'un argument (`strchr`, `strstr`) fonctionnent aussi
correctement : le résultat est copié avant que l'argument soit libéré.

Si une fonction déclarée comme renvoyant `string` renvoie NULL, c'est une erreur, parce que `string` n'a pas de valeur
signifiant « il n'y en avait pas ». Si NULL est possible, recevez le résultat comme `ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Avec `:library`, cette bibliothèque partagée est ouverte et le symbole y est cherché. Sans, le symbole est cherché
dans **le processus lui-même** (tout ce qui est déjà lié, libc comprise). Un nom court comme `sqlite3` est cherché
comme `libsqlite3.dylib` / `libsqlite3.so` dans cet ordre, et un nom contenant `/` est traité comme un chemin. Les
bibliothèques ouvertes ne sont jamais fermées : du code pointant vers leurs fonctions continue de s'exécuter ; la
seule durée de vie correcte est donc celle du processus.

#### ptr / c-long / c-ulong — mots machine bruts

`ptr` est un pointeur opaque (`void *`, `FILE *`, quoi que la déclaration ait voulu dire). `c-long` / `c-ulong` sont
les `long` / `unsigned long` de C (aussi `size_t`, `int64_t` et `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Ne pas les appeler `i64` / `u64` est délibéré.** Ce langage n'a pas de type entier sur 64 bits, parce qu'une valeur
immédiate étiquetée n'a que 63 bits ([chapitre 2](#2-écrire-les-types)). Le nom `c-long` dit « c'est un mot qui
traverse la frontière avec C, pas un entier de ce langage ».

**Ils n'ont pas d'arithmétique.** `(+ x 1)` ne peut pas s'écrire. On pourrait la fournir mais on ne le fait pas, afin
qu'aucun calcul ne s'exécute sur une valeur qui ne peut être stockée nulle part et dont la largeur diffère de celle
de tous les autres nombres, pour la même raison que le type entier sur 64 bits a été laissé de côté. Il n'y a **que
des conversions** :

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; lire ce qui est revenu
(as int (unsafe (c-strlen s)))               ; celle-ci pour le lire exactement (int ne perd pas de bits sur 64)
(try-as i32 (unsafe (c-strlen s)))           ; demander s'il tient
(as c-ulong n)                               ; en créer un à partir d'un autre entier
```

Les **littéraux** entiers prennent le type attendu ; aucun `as` n'est donc nécessaire juste pour en passer un :

```lisp
(unsafe (c-malloc 16))                       ; 16 est lu comme un c-ulong
```

Les littéraux hors limites sont rejetés comme pour les autres largeurs (`(c-malloc -1)` ne tient pas dans un
`c-ulong`).

**Les endroits où ils peuvent apparaître sont limités** : types d'arguments, types de retour et variables locales
uniquement. Chacun des cas suivants est une erreur :

```lisp
(defstruct handle (p ptr))          ; un champ de structure
(defenum maybe (none) (some ptr))   ; un champ d'énumération
(defvar (block ptr) ...)            ; une globale
(defffi f ((vector ptr)) i32)       ; dans un argument de type
```

Il y a une seule raison à tous : **l'emplacement étiquette ce qu'il contient**. L'étiquetage ferait perdre les bits de
poids fort du pointeur, la même raison pour laquelle le type entier sur 64 bits a été laissé de côté ; ce n'est donc
pas permis même dans `unsafe`. Ce n'est pas une question de permission : cette représentation n'existe pas.

Pour la même raison, ils ne peuvent pas être des variables locales **capturées** par des fonctions imbriquées (une
liaison capturée va dans une cellule, et une cellule étiquette ce qu'elle contient). Cela se sait à la compilation et
est signalé par `(compile f)`.

Le GC ne suit pas `ptr`. Il pointe hors du tas ; c'est donc correct.

Quatre choses ne peuvent pas être déclarées :

- **Les arguments variadiques** (`printf`). La partie variadique est passée selon d'autres règles que les arguments
  fixes (sur la pile sous AArch64 Darwin) ; elle ne peut donc pas être appelée correctement à partir d'une signature
  fixe. `&rest` est rejeté.
- **Passer ou renvoyer des structures par valeur.** Pour la même raison (cela dépend de la convention d'appel de
  chaque plateforme). Les types qu'on peut écrire sont limités à la liste ci-dessus ; cela ne peut donc pas
  s'écrire.
- **Les génériques.** C n'a pas d'équivalent.
- **Le même nom qu'une fonction intégrée.** Un appel compilé résoudrait ce nom vers l'intégrée ; c'est donc refusé
  plutôt que de mal tourner silencieusement.

#### Callbacks — se faire rappeler par C

Écrire un type de fonction `(fn (types...) type-de-retour)` comme type d'argument fait de cet argument une fonction
que C rappelle (un callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; une fonction de niveau supérieur
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; un lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; une fonction locale
```

Un pointeur de fonction C n'est rien d'autre qu'une adresse de code, et C l'appelle en ne passant que les arguments
déclarés. Il n'y a pas d'endroit où passer des variables capturées ; **seules les fonctions sans variables libres
peuvent donc être passées**, et cela est vérifié lors de la vérification des types.

- Écrivez **directement** un nom de fonction ou une expression `lambda` comme argument effectif. Une variable
  contenant une fonction ne peut pas être passée : quelle fonction elle contient, et donc si celle-ci a des variables
  libres, n'est connu qu'à l'exécution.
- Un `lambda` est une erreur s'il fait référence à des variables locales extérieures. Les variables globales et les
  fonctions de niveau supérieur peuvent être référencées.
- Une fonction locale (`labels`) ne doit pas avoir de variables libres, y compris celles des fonctions sœurs qu'elle
  appelle. Les fonctions sœurs partagent l'endroit où sont gardées les variables capturées ; ce que capture une sœur
  appelée est donc aussi capturé par cette fonction.
- Une fonction générique tire ses types du type de fonction déclaré.
- Les types qu'on peut écrire dans le type de fonction sont les mêmes que dans la liste ci-dessus. Cependant,
  `string` ne peut pas être le type de retour d'un callback (il remettrait à C une mémoire que personne ne libère).
  Un argument `string` copie la chaîne passée par C dans une chaîne typelisp.

Les appels de fonctions C ne peuvent s'écrire qu'à l'intérieur de `unsafe` ; les callbacks ne peuvent donc être passés
qu'à l'intérieur de `unsafe`.

**Le callback ne peut être appelé que pendant l'exécution de la fonction C que typelisp a appelée.** S'il est appelé
d'ailleurs (un thread qui n'exécute pas typelisp, un gestionnaire de signal, une fonction enregistrée avec
`atexit`), il affiche la raison et arrête le processus.

**Les échecs ne se propagent pas à travers C.** Un `panic` ou un `throw` dans le callback ne peut pas dérouler à
travers des cadres C (ce serait un comportement indéfini) ; 0 est donc renvoyé à C, et l'échec est relancé vers
l'appelant au retour de la fonction C. Si le callback est rappelé entre l'échec et le retour de la fonction C, il
n'est pas exécuté et 0 est renvoyé.

Une opération qui devrait attendre à l'intérieur d'un callback (un `recv` sur un canal vide, etc.) est une erreur
([12.6](#126-code-compilé-et-tâches)).

Quand une fonction est redéfinie, la nouvelle définition est appelée à partir du prochain passage à C.

Cela fonctionne de la même façon avec AOT (`compile-file`). Les points d'entrée qu'appelle C sont intégrés à
l'exécutable.

**Elles ne peuvent pas être passées comme valeurs.** Une déclaration FFI ne peut pas s'écrire telle quelle comme le
`f` de `(map f xs)` : une valeur fonctionnelle est une fermeture qui enveloppe le corps d'une définition, et cette
déclaration n'a pas de corps à envelopper. Enveloppez-la dans un `lambda` :

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` est aussi refusé : ce qui pourrait être montré, c'est le code machine de C, que ce compilateur
n'a pas produit. `(compile c-abs)` réussit (et ne fait rien, puisque c'est déjà compilé).

**Cela fonctionne aussi avec AOT (`compile-file`).** L'éditeur de liens résout lui-même les fonctions C. Si une
déclaration a `:library`, cette bibliothèque est ajoutée à la ligne d'édition de liens sous forme de `-l` (les
doublons sont fusionnés) ; `compile-file` n'a donc besoin d'aucun argument supplémentaire. `compile-file` lit
lui-même le source et peut donc les rassembler à partir des déclarations.

Les symboles sont aussi cherchés à la construction. Si une fonction déclarée n'existe pas, l'erreur la nomme avant
toute erreur d'édition de liens.

La bibliothèque standard (le prélude) n'utilise pas `defffi`. La bibliothèque standard entre en entier dans chaque
exécutable ; une déclaration avec `:library` y ferait donc lier cette bibliothèque même dans des programmes qui
n'utilisent pas le FFI.

#### def-c-struct et pointeurs typés — allouer des structures C

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Déclare une structure de même disposition qu'en C. Elle ne peut s'écrire qu'à l'intérieur d'un `unsafe` de niveau
supérieur (qui ne peut contenir que des `def-c-struct`). Une docstring peut être placée juste après le nom.

Les types qu'on peut écrire pour les champs sont `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32` `f64`
`bool` `ptr`, les pointeurs typés `(ptr T)` et d'autres `def-c-struct` (incorporées par valeur). La disposition (le
décalage de chaque champ, ainsi que la taille et l'alignement de la structure) est calculée selon les règles de C
(en supposant LP64). Un champ pointant vers la structure elle-même peut s'écrire, mais la structure ne peut pas
s'incorporer elle-même.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x à 0, y à 8, taille 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

Le nom d'une `def-c-struct` entre dans l'espace de noms des types (aucun `defstruct` ou semblable de même nom ne peut
se trouver dans le même module), mais **ce n'est pas le type d'une valeur**. On ne peut pas écrire
`(defun f ((p point)) ...)` ; il n'apparaît que comme ce que désigne un pointeur typé.

**Un pointeur typé `(ptr T)`** est une adresse qui désigne un `T`. `T` est l'un des types qu'on peut écrire pour les
champs ci-dessus. C'est un mot machine brut comme `ptr`, avec les mêmes règles sur les endroits où il peut apparaître
(arguments, types de retour et variables locales uniquement ; il ne peut être une valeur qu'à l'intérieur de
`unsafe`).

L'allocation, la lecture et l'écriture s'écrivent sous les formes suivantes. Toutes ne s'utilisent qu'à l'intérieur
de `unsafe`.

| Forme | Signification |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Alloue `n` valeurs de `T` (1 si omis). Le contenu est rempli de 0. Renvoie un `(ptr T)` |
| `(c-ref p i)` | Un pointeur vers l'élément `i` à partir de `p`. Une erreur si hors de la plage allouée |
| `(c-deref p)` / `(setf (c-deref p) v)` | Lit / écrit le scalaire que désigne `p` |
| `p::field` / `(setf p::field v)` | Lit / écrit un champ d'une structure. Lire un champ qui est une structure incorporée donne son adresse (`(ptr inner-type)`) |
| `(as ptr p)` | Oublie le type pour en faire un `ptr` (à passer à quelque chose comme le `void *` de `qsort`). Il n'y a pas de conversion inverse |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**La mémoire allouée est libérée quand l'exécution quitte le `unsafe` qui l'a allouée.** Le propriétaire est le
`unsafe` lexicalement le plus extérieur dans la même fonction. Elle est libérée que le code se termine normalement ou
qu'il soit quitté par `panic`, `throw` ou `return-from`. Les fonctions `lambda` et `labels` sont des fonctions
distinctes ; un `c-alloc` en leur sein a donc besoin de son propre `unsafe` à l'intérieur d'elles.

De ce fait, un pointeur typé ne peut pas quitter le `unsafe` qui l'a alloué. Chacun des cas suivants est une erreur
de type :

- En faire la valeur de l'expression `unsafe` (il ne peut donc pas non plus être renvoyé par une fonction)
- Le capturer dans une fermeture (`lambda`, `labels`)
- Le passer à `task` / `thread`
- Le lancer avec `throw`

Pour utiliser des valeurs en dehors du `unsafe`, copiez-les dans un `defstruct` ou des nombres à l'intérieur du
`unsafe` et renvoyez ceux-ci.

**La mémoire allouée côté C n'est pas prise en charge.** Les valeurs qui arrivent de C comme pointeurs typés (valeurs
de retour de `defffi`, arguments de callbacks, valeurs lues dans des champs de type pointeur) sont vérifiées à
l'exécution pour voir si elles désignent une valeur de ce type dans une allocation `c-alloc` vivante, et sont une
erreur sinon. NULL est aussi une erreur. Pour recevoir une mémoire allouée par C, ou NULL, utilisez le `ptr` non typé
(dont le contenu ne peut pas être lu).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Quand l'argument d'un callback est refusé par la vérification, c'est signalé à l'appelant au retour de la fonction C,
exactement comme un échec à l'intérieur d'un callback.

### 3.4 defvar / defparameter / defconstant — variables globales

```lisp
(defvar (name Type) init-expr)        ; initialise seulement si pas encore liée
(defparameter (name Type) init-expr)  ; affecte à chaque fois
(defconstant (name Type) init-expr)

; avec une docstring (dans le même ordre que les defvar/defparameter/defconstant de CL : après la valeur)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**La différence entre `defvar` et `defparameter` se voit au rechargement** (comme en CL). Si la globale est **déjà
liée, `defvar` n'évalue même pas l'initialiseur** ; quand vous modifiez un fichier de réglages et le relisez, les
valeurs modifiées par la session restent donc telles quelles. `defparameter` affecte à chaque fois ; le relire
ramène donc les valeurs à ce qui est écrit.

L'annotation de type est obligatoire (elle n'est pas inférée à partir de l'initialiseur). `defvar` peut être modifiée ;
`defconstant` ne le peut pas (`setf` est une erreur).

### 3.5 defmethod — définitions de méthodes

```lisp
; méthode d'instance : s'appelle (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; fonction statique / associée : s'appelle (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

L'appelant résout la méthode selon le type statique de `obj` (dispatch simple et statique). Une docstring peut se
placer à la même position et selon les mêmes règles que pour `defun` (juste après la clause `where`, au début du
corps, seulement si des formes de corps suivent). Il en va de même pour les méthodes à l'intérieur d'`impl` ; on les
récupère avec `(documentation Type::method)`.

Les paramètres de type propres à une méthode s'écrivent dans son nom avec `<...>`, comme pour
`defun`. Les paramètres de type du type du receveur (`T` ci-dessous) sont fixés par le receveur ;
ceux de la méthode (`U`) sont inférés à partir des arguments de chaque appel.

```lisp
(defstruct Box<T> (v T))

(defmethod fmap<U> ((self Box<T>) (f (fn (T) U))) Box<U>
  (Box::new (f self::v)))

(fmap (Box::new 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- Les paramètres de type propres à la méthode doivent porter des noms différents des paramètres de
  type que déclare le type du receveur (le `T` de `(defstruct Box<T> ...)`) et des noms écrits dans
  le receveur.
- Si le type du receveur est générique, on écrit dans le receveur soit tous ses paramètres de type
  comme variables (`Box<T>`), soit tous comme types concrets (`Box<int>`).
- Une méthode dans un `impl` ne peut pas ajouter de paramètres de type : sa signature suit celle que
  déclare le trait.

### 3.6 defstruct — structures (types définis par l'utilisateur)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; générique (paramètres de type entre chevrons)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Chaque champ est `(name type)` ou `(pub name type)` (visibilité par champ, indépendante du `pub` de la structure
  elle-même). Une expression supplémentaire à la fin devient la **valeur par défaut** du slot (`(x i32 0)`) ; voir la
  liste d'options ci-dessous.
- Les éléments suivants sont générés automatiquement :
  - Le constructeur `Name::new` (arguments dans l'ordre des champs)
  - Les accesseurs `(field-name instance)`, avec le sucre `instance::field-name`
  - Les modificateurs `(set-field-name instance value)`, avec le sucre `(setf instance::field-name value)`
- Pour rendre la structure elle-même `pub`, placez `pub` devant, comme `(pub defstruct ...)`.
- **Définissez un type avant de le nommer.** Le type d'un champ peut être la structure elle-même
  (`(next Option<node>)`), mais pas un type défini plus loin : les types n'ont pas de déclaration anticipée
  correspondant à `defsignature`. Un nom pas encore défini donne la même erreur `unknown type` dans un type
  d'argument de `defun` ou dans `the`. Deux types qui se référencent mutuellement ne peuvent donc pas s'écrire.
- **Les variables de type sont seulement celles écrites aux positions de déclaration.** Pour
  `defun`/`defstruct`/`defenum`/`deftype`, le `<T>` du nom ; pour `defmethod`, le type du receveur
  (`(self box<T>)`, ou `box<T>` pour une méthode statique) et le `<U>` du nom de la méthode ; pour
  `impl`, le type cible et `impl<T>` ; pour `deftrait`, `Self` et les types associés de
  `(type Item)`. Un nom apparaissant pour la première fois ailleurs (arguments, valeur de retour,
  `the`/`lambda` dans le corps) ne devient pas une variable de type ; c'est `unknown type`.
- **Docstrings** : un littéral chaîne juste après le nom, avant les champs, devient la docstring
  (`(defstruct Name "doc" (field Type)...)`, la même position que le `defstruct` de CL). Un champ a toujours la forme
  `(name Type ...)` et ne peut jamais être une simple chaîne ; il n'y a donc pas d'ambiguïté. On la récupère avec
  `(documentation Name)`.

#### Liste d'options

Écrire une liste `(Name option...)` à la position du nom spécifie des options (la même position qu'en CL).

```lisp
(defstruct (point (:constructor make-point)          ; constructeur à mots-clés
                  (:constructor at (x &optional y))  ; constructeur BOA
                  (:copier copy-point))
  (x i32 0)          ; un troisième élément est la valeur par défaut de ce slot
  (y i32 0))

(point::make-point :y 7)   ; x vaut 0
(point::at 1)              ; y vaut 0
(point::at 1 2)
(copy-point p)             ; une copie superficielle (comme le copier de CL)
```

- **`:constructor`** : ce qui est généré est une **fonction statique** du type (`point::make-point`), dont le corps est
  toujours `(point::new ...)`. `new` reste l'unique constructeur structurel ; ce qui est créé ici est une *façon de
  l'appeler*. On peut en déclarer plusieurs.
  - `(:constructor name)` prend chaque slot en `&key`. **Chaque slot a besoin d'une valeur par défaut** (ce langage
    n'a rien qui corresponde au « slot non lié » de CL).
  - `(:constructor name (slot...))` prend les slots nommés comme arguments positionnels (dans n'importe quel ordre).
    Les slots non nommés sont remplis avec leurs valeurs par défaut ; **ils ont donc besoin de valeurs par défaut**.
    Après `&optional`, le reste peut être omis (et a de même besoin de valeurs par défaut).
- **`:copier`** : génère une **méthode d'instance** qui renvoie une nouvelle valeur avec les mêmes valeurs de slots.
  Superficielle, comme le copier de CL.
- **`:include Parent`** : place en tête les slots du parent (les valeurs par défaut sont héritées aussi ; le parent
  peut être dans un autre fichier). **Il ne crée aucune relation de types** : l'enfant n'est pas un sous-type du
  parent, les méthodes du parent ne s'appliquent pas à l'enfant, et aucun test à l'exécution ne relie les deux. Ce
  langage n'a pas de sous-typage ; les interfaces communes sont le rôle de `deftrait`. Seule la *liste* des slots est
  jointe.
- **Les valeurs par défaut des slots ne sont lues que par les constructeurs générés.** Écrire une valeur par défaut
  sans déclarer de `:constructor` est une erreur, puisqu'elle ne pourrait jamais servir.
- Options laissées de côté, et pourquoi :
  - **`:conc-name`** : en CL, il préfixe les accesseurs pour éviter les conflits dans un espace de noms de fonctions
    unique et plat. Ici, les accesseurs sont des méthodes choisies selon le type du receveur ; les conflits ne se
    produisent donc pas, et un préfixe casserait `instance::field` (qui ne connaît que le nom du slot).
  - **`:predicate`** : répond à l'exécution à « cette valeur est-elle un `point` ? ». Ici, les types sont une
    classification à la compilation sans témoin à l'exécution, et il n'existe aucune position où « une valeur de type
    inconnu qui pourrait être un point » existe (`match` sur `Sexpr` est fermé, et `:dyn` ne peut pas être transtypé
    vers le bas) ; un prédicat généré ne pourrait donc jamais renvoyer que `true`.
  - **`:type` / `:initial-offset` / `:named`** : ils remplacent la représentation de la valeur par une liste ou un
    vecteur. La représentation appartient au compilateur et ne peut pas être observée depuis le langage.

### 3.7 defenum — énumérations (types somme)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; une variante avec charge utile (champs positionnels)
  (Variant2)                  ; une variante sans charge utile
  ...)

; générique
(defenum Option<T>
  (Some T)
  (None))
```

- Chaque variante a la forme `(VariantName FieldType...)`. Les champs sont uniquement positionnels (ils n'ont pas de
  nom). Au moins une variante est nécessaire, et les noms ne peuvent pas se répéter.
- Les valeurs se construisent, comme pour les `Option`/`Result` intégrés, de façon qualifiée ou via `use` :
  `(Name::Variant1 a b)`, ou `(Variant1 a b)` après `(use Name)`.
- Elles peuvent se décomposer avec `match` / `if-let`. `match` vérifie l'exhaustivité (il doit couvrir chaque variante
  ou avoir un `_`) :
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Les méthodes et fonctions associées s'ajoutent ensuite avec `defmethod`/`impl`, comme pour `defstruct`.
- Pour rendre l'énumération elle-même `pub`, écrivez `(pub defenum ...)`.
- **Docstrings** : la même position et les mêmes règles que `defstruct`, juste après le nom, avant les variantes
  (`(defenum Name "doc" (Variant ...)...)`). On la récupère avec `(documentation Name)`.

### 3.8 deftype — alias de types

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

Le `deftype` de CL, réduit à ce qui a du sens dans un langage à typage statique : **une écriture d'un type, pas un
type**.

- La position du nom est la même que pour `defun`, et les arguments génériques s'écrivent `Name<T,U>`. À l'endroit
  d'utilisation, il faut exactement le nombre déclaré d'arguments de type (trop ou trop peu est une erreur sur
  place).
- Le développement a lieu **à l'intérieur de l'analyseur de types**. Rien en aval ne sait donc que l'alias existe :
  les clés de monomorphisation, les dumps, le chemin de compilation et **les messages d'erreur** montrent tous la
  forme développée. Si `(f "x")` échoue contre une fonction qui exige `meters`, le message dit `i32`.
- **Ce n'est pas un nouveau type.** `(deftype meters i32)` fait de `meters` et `i32` le même type ; les confondre
  n'est donc pas détecté. Si vous voulez les distinguer, utilisez `defstruct`.
- **Ce n'est pas un prédicat.** Le `(deftype small () '(integer 0 9))` de CL décrit un *ensemble de valeurs* que
  `typep` teste à l'exécution, mais ici les types sont une classification à la compilation sans témoin à
  l'exécution ; un alias restreignant des valeurs n'aurait rien à restreindre.
- **Il ne peut pas se contenir lui-même.** Un alias est développé là où il est écrit ; il n'a donc nulle part où
  récurser. Les types de données récursifs s'écrivent avec `defstruct`/`defenum`.
- Il partage l'espace de noms avec les types et les traits (dans un module, il ne peut pas avoir le même nom qu'un
  `defstruct`/`defenum`/`deftrait`). On le rend public avec `(pub deftype ...)` et on l'importe avec
  `(use m::meters)`.
- **Docstrings** : juste après le nom, avant le type (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — traits

```lisp
(deftrait TraitName (SuperTrait...)      ; la liste des super-traits est obligatoire ; () s'il n'y en a pas
  (type AssocName)                       ; types associés (nombre quelconque, facultatifs)
  (method-name ((self Self) params...) RetType)          ; sans corps = doit être implémentée
  (method-name ((self Self) params...) RetType body...)) ; avec corps = implémentation par défaut

(impl TraitName TargetType
  (where (Trait A)...)                   ; bornes qui s'appliquent à tout l'impl (facultatif)
  (type AssocName ConcreteType)          ; rend concret un type associé
  (method-name (recv params...) RetType body...))
```

Via `impl`, chaque méthode est enregistrée comme un `defmethod` ordinaire de `TargetType`. Les traits sont référencés
comme bornes de traits dans les clauses `where` des fonctions génériques (voir
[3.1 defun](#31-defun--définitions-de-fonctions)). Un nom de trait peut aussi être un chemin `::` comme `m::Trait`.

**La liste des super-traits (obligatoire)** : toujours écrite juste après le nom du trait. Chaque élément est un nom
de trait nu ou, si ce trait a des types associés, `(Trait (Assoc Type))` avec **tous ses types associés fixés**.

```lisp
(deftrait Eq () ...)                       ; pas de super-traits
(deftrait Ord (Eq) ...)                    ; le trait Ord: Eq de Rust
(deftrait CharSource ((Iter (Item char)))  ; fixer un type associé
  (rewind ((self Self)) ()))
```

L'héritage a trois effets. (1) `impl Ord X` exige que `impl Eq X` soit écrit **avant** (une règle d'ordre d'écriture :
la seule forme décidable de façon déterministe dans la REPL et avec un `load` pas à pas, et plus stricte que Rust).
(2) `(where (Ord T))` seul permet aussi d'appeler les méthodes de `Eq`. (3) Les méthodes de `Eq` peuvent être appelées
via un `:dyn Ord`, et une valeur `:dyn Ord` peut être passée telle quelle là où un `:dyn Eq` est exigé (transtypage
ascendant). Un sous-trait qui redéclare une méthode de même nom que son parent, et l'héritage de méthodes de même nom
depuis deux parents, sont tous deux des erreurs (une vtable a une entrée par nom). L'héritage en losange est fusionné
en une entrée.

**Implémentations par défaut** : un corps après la signature est utilisé quand un `impl` omet la méthode. Le corps est
résolu dans **l'espace de noms du module** où le trait est écrit ; il peut donc appeler des fonctions non publiques de
ce module. Les méthodes avec corps peuvent aussi avoir des clauses `where` et des docstrings. Le corps est vérifié
**une seule fois, à l'endroit de la déclaration**, avec `Self` laissé comme variable de type (bornée par
`Self: le trait lui-même`), comme en Rust : les erreurs qui échoueraient pour chaque `impl` et chaque type
implémentant, même dans des implémentations par défaut qu'aucun `impl` n'omet jamais, y sont détectées. Les appels sur
`self` aux méthodes du trait lui-même ou de ses super-traits passent par cette borne, et les types associés sont
fixés à eux-mêmes ; une signature qui renvoie `Item` est donc confrontée au corps sans connaître le type concret.

**Implémentations génériques globales** : faire de la cible une variable de type implémente le trait d'un coup pour
chaque type qui satisfait les bornes.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; aucun corps ; tout est par défaut
```

**Aucun code n'est généré tant qu'un type concret ne l'utilise pas réellement** (une fois par type, par le même
mécanisme que la monomorphisation ordinaire). Un trait peut avoir au plus une implémentation générique globale. Si un
type a un `impl` explicite, celui-ci est prioritaire. La vérification du corps est distincte de la génération : elle
se fait une fois à l'endroit de la déclaration, **avec la cible laissée comme variable de type** (comme en Rust) ;
même une implémentation jamais utilisée voit donc ses erreurs détectées là si elles échoueraient pour toute cible
sous les bornes déclarées. Les appels justifiés par les bornes (`(less self other)` sous `(where (Ord T))`, etc.)
passent, comme dans le corps d'un `defun` générique.

**Docstrings** : un `deftrait` peut avoir une docstring pour l'ensemble du trait, sous forme de littéral chaîne juste
après la liste des super-traits, avant les éléments (`(deftrait Name () "doc" (type ...) (method ...)...)`). Une
signature sans corps ne peut pas avoir de docstring : une chaîne finale serait elle-même la valeur de retour d'une
implémentation par défaut ; les deux ne pourraient donc pas être distingués.

Les traits que fournit la bibliothèque standard : **`Iter`** (`next` / type associé `Item` ; la base de `doiter` et
des fonctions de séquence), **`Eq`** (`equals` ; `not-equals` est une implémentation par défaut), **`Ord`** (hérite de
`Eq` ; seule `less` doit être implémentée, et `less-equal` / `greater` / `greater-equal` sont des implémentations par
défaut), **`Error`** (`message` / `source` ; `:dyn Error` pour traiter uniformément les types d'erreur),
**`print-object`** (une représentation imprimée par type), **`Pathish`** (désignateurs de noms de chemin : une chaîne
ou un `pathname`), et la hiérarchie des flux **`Stream`** → **`InputStream`** / **`OutputStream`** →
**`CharInput`** / **`CharOutput`** → **`PeekInput`**. Quels types implémentent quels traits se trouve dans
[types.md](types.md) ; les méthodes de chaque trait se trouvent dans [Traits standard](functions/traits.md),
[Types d'erreur](functions/option-result.md#3-types-derreur-et-le-trait-error),
[print-object](functions/printing.md#5-print-object-représentation-imprimée-par-type) et
[Flux](functions/streams-files.md). Si vous faites un `impl` de `Iter` pour votre propre type de collection, `doiter`
(chapitre 5) et `map` / `filter` / `sort` et semblables y fonctionnent tels quels.

Les appels de traits sont **statiques** par défaut (résolus selon le type statique du receveur). Pour traiter des
valeurs dont le type concret est décidé à l'exécution, le type d'objet trait `:dyn Trait` (chapitre 2) donne un
dispatch dynamique via une vtable :

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; un endroit d'appel, une réponse par implémentation
```

Seuls les traits où « chaque méthode a un receveur `self`, n'utilise `Self` nulle part ailleurs que dans le receveur,
et n'est elle-même ni générique ni variadique » peuvent devenir `:dyn` (les méthodes héritées doivent remplir les
mêmes conditions).

Seuls les types dont les valeurs ont une représentation sur le tas peuvent entrer dans une boîte `:dyn` :

| Peuvent entrer | Ne peuvent pas entrer |
|---|---|
| Les types `defstruct` / `defenum` (y compris `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` et les structures de la bibliothèque standard), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Les entiers de largeur fixe (`i8` à `u32`), `f32`, `bool`, `char`, `symbol`, `()`, les types de fonctions, et `Option<T>` sans boîte ([la représentation à l'exécution d'Option](functions/option-result.md#2-la-représentation-à-lexécution-de-optiont)) |

Placer une valeur d'un type qui ne peut pas entrer là où un `:dyn` est attendu est une erreur de type. Pour traiter de
telles valeurs via `:dyn`, enveloppez-les dans une structure, comme `(defstruct flag (v bool))`.

### 3.10 module / use — espaces de noms

```lisp
(module path body...)      ; path est une suite de segments comme foo ou foo::bar
(in-module path)           ; d'ici à la fin de cette unité, à l'intérieur de path (la forme plate de module)
(use path...)              ; crée des alias de fonctions, types et modules dans l'espace de noms courant
(import path...)           ; identique à use (une écriture compatible CL)
(shadowing-import path...) ; un use qui prend sciemment un nom nu déjà utilisé
```

- `module` crée un espace de noms. **Les types ne sont pas des espaces de noms** (comme en Rust, un type n'a que des
  fonctions associées et des méthodes).
- Importer un type avec `use` rend aussi ses constructeurs et ses méthodes statiques publiques disponibles par nom nu
  (par exemple, après `(use option)`, `some`/`none` peuvent s'appeler sans `option::some`/`option::none`).
- L'ordre de résolution des noms nus (identificateurs non qualifiés) : formes spéciales → constructeurs → fonctions
  libres (espace de noms courant → racine) → méthodes d'instance (résolues selon le type statique du premier
  argument). Il ne remonte pas à travers les modules parents intermédiaires.
- Un chemin qualifié `a::b` résout `a` dans l'ordre ci-dessus ; si c'est un module, il y entre, et si c'est un type, le
  dernier segment est résolu comme élément associé.
- **`use` affecte les formes qui le suivent.** Un fichier est lu une forme à la fois, et les dépendances sont résolues
  juste avant la vérification de la forme ; écrire `m::f` **au-dessus** de `(use m)` donne donc `unresolved path`.
  Placez `use` en tête du fichier.
- **`use` peut prendre plusieurs chemins** (`(use a::f b::g)`). `import` est une écriture compatible CL au même
  comportement.
- **Un `use` dont le nom nu est déjà pris est signalé.** La résolution d'un nom nu regarde les définitions propres du
  module avant les alias ; `(use m::twice)` après `(defun twice ...)` **ne fait donc rien**. Si c'est voulu, écrivez
  `shadowing-import` (il ne peut toujours pas l'emporter sur une définition, puisqu'il n'y a aucun moyen d'en retirer
  une ; il ne l'emporte que sur les alias antérieurs).
- **`in-module` est la forme plate de `(module path body...)`.** Écrire `(in-module geometry)` place tout ce qui suit
  jusqu'à la fin de l'unité (le fichier, ou le corps du `module` englobant) à l'intérieur de `geometry`. Il va **à
  l'intérieur** du module propre du fichier (`main::geometry` pour `main.typl`). Deux à la suite s'imbriquent dans
  l'ordre. C'est différent de l'`in-package` de CL, d'où un nom différent : dans ce système, le fichier est déjà un
  module ; il n'y a donc rien à « sélectionner », et tout ce qu'une forme peut faire est imbriquer.

### 3.11 Fichiers et modules (projets à plusieurs fichiers)

Le chemin du fichier relatif à la racine des sources est le chemin du module :
le contenu de `<root>/geo/point.typl` est implicitement enveloppé dans le module `geo::point` (un répertoire est aussi
un segment, à la manière de Rust / Python). Un `(module bar ...)` explicite dans le fichier s'imbrique **à
l'intérieur** (`geo::point::bar`) ; le chemin dérivé et une déclaration explicite n'entrent donc jamais en conflit.

- **Racine des sources** : placez un fichier manifeste `typelisp.toml` à la racine du projet (il peut être vide ;
  facultativement, une ligne `src = "src"` nomme le répertoire des sources). Il est trouvé en remontant depuis le
  répertoire du fichier cible. Sans manifeste, le répertoire du fichier d'entrée (le répertoire courant pour la REPL)
  est la racine.
- **Chargement à la demande** : quand `(use geo::point)` fait référence à un module pas encore chargé, le fichier
  correspondant (`geo/point.typl`) est chargé, vérifié et enregistré automatiquement. `use a::b::c` cherche d'abord le
  préfixe le plus long : `a/b/c.typl` → `a/b.typl` → `a.typl` (puisque `c` peut être un élément à l'intérieur d'un
  module). Les définitions visibles depuis d'autres modules ont besoin de `pub` ([3.13 pub](#313-pub--visibilité)).
- **Les références circulaires sont des erreurs** : la chaîne est signalée sous la forme
  `circular module dependency: a -> b -> a`.
- **Exécution** : `typl <file.typl>` exécute un fichier (sans arguments, la REPL). `use` dans la REPL résout les
  fichiers selon les mêmes règles.
- **Capacité de l'arène cons** : `typl --heap-cells N` fixe la **capacité initiale** de l'arène des cellules cons
  (65536 par défaut ; la forme `--heap-cells=N` fonctionne aussi, pour l'exécution de fichiers comme pour la REPL).
  L'arène **grandit par ajouts** quand elle manque de place. La limite de croissance est 256 fois la capacité
  initiale, et une allocation au-delà donne `heap exhausted` : la capacité initiale signifie « allouer autant au
  début », et la limite signifie « au-delà, traiter comme une fuite ».

### 3.12 load — chargement à plat

```lisp
(load "path")   ; niveau supérieur uniquement ; path est un littéral chaîne
```

- **Chargement à plat** à la manière de CL : lit les formes du fichier cible telles quelles **dans l'espace de noms
  courant** (sans les envelopper dans un module, contrairement à `use`). Niveau supérieur uniquement (à l'intérieur
  d'un corps de fonction, c'est une erreur de type).
- `path` est relatif au répertoire du fichier qui charge (depuis la REPL, au répertoire courant du processus). S'il
  n'a pas d'extension, `.typl` est ajouté.
- Les `(load ...)`/`(use ...)` du fichier chargé sont aussi traités récursivement.
- **Il lit une forme à la fois et l'exécute sur place** (comme le `load` de CL). La forme *k* a fini de s'exécuter
  avant que *k+1* soit lue : même s'il y a une erreur de syntaxe ou de type en cours de route, les formes précédentes
  se sont déjà exécutées. Les fichiers de modules chargés par `use` sont différents : ils sont vérifiés comme une
  unité et leur exécution est laissée à celui qui les a importés avec `use` (correspondant au `compile-file` de CL).

### 3.13 pub — visibilité

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` ne peut se placer que sur les onze sortes ci-dessus (pas sur `module`/`use`/`deftrait`/`impl`). Il s'écrit avec
le mot-clé de définition juste après `pub`, pas sous la forme `(pub (defun ...))` qui envelopperait la définition entre
parenthèses. Un `pub` rend publique exactement une définition (plusieurs définitions ne peuvent pas être marquées d'un
coup).

### 3.14 defmacro — définitions de macros

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- Tous les paramètres et la valeur de retour sont toujours des `Sexpr` ; on n'écrit donc pas d'annotations de type.
- Macros non hygiéniques à la manière de CL (éviter les conflits avec `gensym` est la responsabilité de l'auteur de la
  macro).
- La liste lambda suit l'ordre de CL `required &optional &rest &key` (chaque marqueur au plus une fois, et seulement
  dans cet ordre).
  - `&optional` … arguments facultatifs. `name` ou `(name default-expr)`. L'expression par défaut est évaluée au
    moment du développement (elle peut faire référence aux paramètres liés plus tôt) et liée quand l'argument est
    omis (sans valeur par défaut, la liste vide `()`).
  - `&rest name` … reçoit les arguments positionnels restants réunis en une seule liste `Sexpr`.
  - `&key` … arguments mots-clés. `name` ou `(name default-expr)`. L'appelant les passe sous la forme `:name value`
    (dans n'importe quel ordre). Omis, l'expression par défaut (la liste vide `()` s'il n'y en a pas). Les mots-clés
    inconnus ou une suite `:key` de longueur impaire sont des erreurs.
- Exemples : `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — liaisons de macros locales

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; macros à portée lexicale
(symbol-macrolet ((name expansion) ...) body...)         ; un nom tient lieu d'une forme
```

Ce sont toutes deux des formes spéciales d'**expression**, et rien ne subsiste à l'exécution (ce qui est compilé, c'est
la forme développée du corps). La liste lambda est la même que pour `defmacro`. Les règles détaillées et les exemples
se trouvent dans
[Liaisons de macros locales](functions/system.md#9-liaisons-de-macros-locales-macrolet--symbol-macrolet).

## 4. Liaison et conditionnelles

```lisp
(let ((name val) ...) body...)      ; liaison parallèle
(let* ((name val) ...) body...)     ; liaison séquentielle (les liaisons antérieures sont utilisables dans les initialiseurs suivants)

(if cond then else)                 ; else est obligatoire (toujours trois éléments)
(when cond body...)                 ; un if sans else (type Unit). defmacro
(unless cond body...)               ; la négation de when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; une liste de clés : correspond si l'une d'elles correspond
  (else body...))                   ; expr est évaluée une fois. les clés sont comparées avec equal.
                                     ; les clés sont des « littéraux » et ne sont pas évaluées (comme en CL).
                                     ; un symbole nu a signifie le symbole 'a.
                                     ; écrire 'a est une erreur (utiliser le a nu). defmacro
(ecase expr (key body...) ...)      ; un case exigeant une correspondance. panic si rien ne correspond. defmacro
(ccase expr (key body...) ...)      ; le ccase de CL. il n'y a pas de restarts à offrir, c'est donc identique à ecase. defmacro
(and expr...)                       ; évaluation en court-circuit. true sans argument. defmacro
(or expr...)                        ; évaluation en court-circuit. false sans argument. defmacro
(progn body...)                     ; exécute dans l'ordre et renvoie la dernière valeur
(unsafe body...)                    ; identique à progn, plus la permission d'écrire des appels FFI
                                     ; et des mots bruts. voir 3.3 defffi
(prog1 form more...)                ; évalue tout ; la valeur est celle de form. defmacro
(prog2 a b more...)                 ; évalue tout ; la valeur est celle de b. defmacro
(the Type expr)                     ; une annotation de type (sans effet à l'exécution)
```

### 4.1 unsafe — assumer des hypothèses invérifiables

```lisp
(unsafe body...)
```

Identique à `progn` : évalue le corps dans l'ordre et renvoie la dernière valeur. Il ne crée pas de portée et n'est pas
une frontière de fonction (`break` / `return-from` traversent directement vers l'extérieur). La différence est que
certaines choses ne peuvent s'écrire qu'à l'intérieur.

Trois choses exigent actuellement `unsafe` : appeler des fonctions C déclarées avec
[defffi](#33-defffi--déclarer-des-fonctions-c-ffi), faire des mots machine bruts (`ptr` / `c-long` / `c-ulong` /
`(ptr T)`) des valeurs, et [`def-c-struct` et `c-alloc`](#def-c-struct-et-pointeurs-typés--allouer-des-structures-c).

La mémoire allouée avec `c-alloc` est libérée en quittant le `unsafe` le plus extérieur de la même fonction. Seul ce
`unsafe`, contrairement à `progn`, a un travail à faire à la sortie : la libération.

Ce que `unsafe` assume, ce sont les hypothèses suivantes que le compilateur ne peut pas vérifier :

- **Que les types correspondent.** Que la signature C déclarée corresponde à la réelle. Sinon, les arguments vont dans
  les mauvais registres et les valeurs de retour sont lues avec la mauvaise largeur.
- **La sûreté mémoire.** Ce que le côté C fait de ce qu'on lui donne.
- **L'état global du processus.** Variables d'environnement, gestionnaires de signaux, `errno`. Par exemple, appeler
  `setenv` via le FFI casse les hypothèses que fait le `decode-universal-time` de cette implémentation quand il
  calcule l'heure locale.
- **La sûreté vis-à-vis des threads.**

Ce n'est pas une échappatoire à la vérification des types. `(unsafe (+ 1 "two"))` ne passe pas. Ce qui est permis,
c'est d'écrire certaines **opérations**, pas d'écrire des absurdités.

Il agit lexicalement. Le corps d'un `lambda` écrit à l'intérieur de `unsafe` hérite de la permission (comme les
fermetures à l'intérieur des blocs `unsafe` de Rust). La valeur peut ensuite être appelée en dehors du `unsafe`, mais
l'avoir écrite là vaut déjà acceptation de la responsabilité.

### 4.2 destructuring-bind — décomposer des listes selon leur forme

```lisp
(destructuring-bind lambda-list form body...)
```

Décompose **selon sa forme** la liste que produit `form` et la lie. La liste lambda est celle de `defmacro`
(obligatoires → `&optional` → `&rest`/`&body` → `&key`, chacun avec des expressions par défaut), pour la même raison
que CL en partage une entre les deux : ce sont deux formes qui décomposent la même chose.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Chaque variable liée est une `Option<Sexpr>`.** Ce n'est pas une limitation de l'implémentation mais la nature de
  ce qui est lié : les listes S-expression sont les seules listes de ce langage ; il n'y a donc pas d'autre type à
  donner aux éléments. Se rabattre sur `match` là où un scalaire est nécessaire est la même chose que dans un corps de
  `defmacro`.
- **Une forme qui ne correspond pas déclenche un panic** (correspondant à l'erreur de CL) : trop peu ou trop
  d'éléments, une suite `&key` de longueur impaire, ou un mot-clé inconnu. `sexpr-car` est une fonction indulgente qui
  renvoie `()` pour `()` ; sans la vérification, une liste trop courte serait silencieusement liée à une suite vide.
- **Les listes lambda imbriquées ne sont pas prises en charge.** `defmacro` ne les prend pas non plus ; il y a donc une
  seule règle. `(a (b c))` ne lie pas silencieusement une sous-liste à `b` ; c'est une erreur qui le dit.
- Les expressions par défaut de `&optional` / `&key` ne sont **évaluées que lorsqu'elles servent** (comme en CL).
- Il n'y a rien qui corresponde au `&allow-other-keys` de CL (`defmacro` n'en a pas non plus).

### 4.3 match — filtrage par motifs

```lisp
(match expr
  (pattern body...)
  ...)
```

Sortes de motifs :
- `_` — joker
- Un nom de variable — un motif de liaison (correspond toujours). Cependant, si le type de la valeur examinée a une
  variante de ce nom, il est résolu comme **le motif de nom de variante nu ci-dessous**
- Un nom de variante nu — correspond à une variante sans arguments (`(match c (red 1) (blue 2))`). Écrire une variante
  à champs par son nom nu est une erreur d'arité ; écrivez-la entre parenthèses, comme `(circle r)`
- **Littéraux immédiats** : entiers / `true`/`false` / caractères — comparés comme des mots
- **Littéraux de valeur** : chaînes / nombres à virgule flottante / symboles (`'foo`) / entiers bignum / fractions —
  comparés par valeur avec l'`Eq::equals` de ce type ([Traits standard](functions/traits.md#2-eq--ord-comparaison)).
  Les chaînes se comparent par contenu, pas par identité
- `(= expr)` — évalue une expression quelconque et compare avec `Eq::equals`. Le seul moyen de comparer des types sans
  syntaxe littérale (instances de `defstruct`, globales, résultats calculés), et une implémentation d'`Eq` définie
  par l'utilisateur devient telle quelle la règle de comparaison. `expr` peut faire référence à tout ce qui est
  visible depuis la position de la branche (arguments, liaisons extérieures, globales)
- `(Ctor sub-pattern...)` — motifs de constructeur (`Some x` `None` `Cons a d` `Ok v`, etc.)

Comparer un type qui n'implémente pas `Eq` avec un littéral de valeur / `(= expr)` est une erreur de type (ce langage
choisit de dire « on ne peut pas les comparer » plutôt que de laisser une branche qui ne correspond silencieusement
jamais).

**Littéraux de valeur face à une valeur examinée `Sexpr`** : l'`Eq` de `sexpr` est `eq` (l'identité de CL) ; les
immédiats (`'foo` (interné) / entiers / caractères / `true`/`false`) peuvent donc s'écrire tels quels et correspondent
par contenu :

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Les littéraux non immédiats (chaînes / nombres à virgule flottante / entiers bignum / fractions) **ne peuvent pas
s'écrire** face à un `Sexpr`. Leur `eq` compare l'identité des objets, ce qui donnerait « une branche qui passe la
vérification mais ne correspond jamais » ; c'est donc une erreur qui nomme le motif de variante : écrivez
`(str "hi")` et il est décomposé en `string` et comparé par contenu. `(= expr)` demande explicitement `equals` ; cette
restriction ne s'y applique donc pas.

**La valeur examinée n'a pas besoin d'être un ADT.** `string`/`symbol`/`i32`/`f64` et semblables peuvent être filtrés
directement (c'est là que vont les motifs de littéraux chaînes). Cependant, un type sans variantes ne peut pas être
couvert par énumération ; `_` (ou un motif de liaison qui fait office de joker) est donc obligatoire :

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; un type sans variantes a besoin de `_`
```

Face à une valeur examinée `Sexpr`, en plus des 18 motifs de variantes intégrés ci-dessus, on peut écrire des
**motifs de transtypage descendant** (extraction d'instances d'ADT définis par l'utilisateur) : une syntaxe pour
récupérer avec `match` une instance d'un `defstruct`/`defenum` (chapitre 3) convertie implicitement en `Sexpr`, comme
dans `(list p 42)` :

- `(TypeName sub-pattern...)` — décomposition en champs avec le **nom du type** en tête (structures uniquement : un
  `defstruct` a toujours une seule variante ; il s'écrit donc avec le nom du type plutôt qu'un nom de variante). Par
  exemple, pour `(defstruct point (x f64) (y f64))`, `(point x y)`.
- Un nom de variante nu `(VariantName sub-pattern...)` — extrait une variante d'un `defenum`. Résolu comme un nom nu
  visible après `(use EnumType)` (les mêmes règles de visibilité que pour l'appel du constructeur). Par exemple, pour
  `(defenum color (red) (blue))`, `(red)` `(blue)` après `(use color)`. Si des noms de variantes de plusieurs
  énumérations visibles entrent en conflit, c'est une erreur d'ambiguïté ; la forme qualifiée
  `(EnumType::VariantName ...)` peut donc aussi s'écrire (pas besoin de `use`).
- `(the Type pattern)` — un transtypage descendant du type entier (le lie comme un tout). Il ne décompose pas les
  champs ; il passe la valeur telle quelle à `pattern`. Le seul moyen d'extraire une structure mutable en gardant son
  identité, et aussi le seul moyen de tirer un `Vector<T>`/`HashTable<K,V>` d'un `Sexpr` (ils n'ont pas de forme de
  décomposition en champs). Par exemple, après `(the point p)`, `(setf p::x 9)` se répercute aussi sur l'instance
  d'origine dans la liste.

**Motifs pour `Option<Sexpr>`** : le type des données S-expression n'est pas `Sexpr` mais `Option<Sexpr>`, et la liste
vide n'est pas une variante de `Sexpr` mais le `none` d'`Option`. Quand on filtre une `Option<Sexpr>`, les 18
variantes de `Sexpr` et `none` peuvent donc s'écrire **à plat dans la même liste de branches** (pas besoin d'un
`match` extérieur pour retirer l'`Option`) :

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; la liste vide
    (_          9)))
```

L'exhaustivité est vérifiée dans le même univers plat : les 18 variantes de `Sexpr` plus `none`, 19 en tout. Oublier
`(none)` est une erreur à moins qu'il y ait un `_`. `(some x)` peut aussi s'écrire et lie « quelque chose de non
vide ».

Ce sucre s'applique **exactement** à `Option<Sexpr>` seulement. Pour `Option<Option<Sexpr>>`, on ne saurait pas quelle
couche `(int n)` a retirée ; écrivez donc deux niveaux de `match` comme d'habitude.

Les mêmes motifs de transtypage descendant s'utilisent tels quels sur une valeur examinée **objet trait (`:dyn Trait`,
chapitre 2)** : `match` la sort de sa boîte puis la confie à la mécanique des motifs `Sexpr` ci-dessus ; il n'y a donc
pas de syntaxe supplémentaire. L'ensemble des types implémentants est ouvert ; il ne peut donc jamais être exhaustif,
et `_` est obligatoire :

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; décomposition en champs avec le nom du type en tête
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Inférence de types entre branches** : toutes les branches doivent avoir le même type (sauf les branches qui
divergent, comme avec `panic`). Dans un `match` écrit là où aucun type n'est attendu, les branches complètent
mutuellement leurs arguments de type manquants : `(result::ok v)` ne fixe que `T`, et `(result::err e)` que `E`, mais
ensemble ils fixent `Result<T,E>`. Un argument de type qu'aucune branche ne peut fixer à la fin est une erreur de cette
branche (`cannot infer type argument ...`). En dehors de `match`, un argument de type qui ne peut pas être fixé est
une erreur sur place.

La vérification d'exhaustivité d'un `match` utilisant des motifs de transtypage descendant ne les compte pas dans la
couverture des variantes propres de `Sexpr` (un `match` qui n'énumère que des motifs de transtypage descendant doit se
terminer par `_`). Pour les ADT génériques (`defstruct point<T> ...`, etc.), les arguments de type d'un motif de
transtypage descendant ne peuvent pas être inférés ; la forme de décomposition en champs (`(point ...)`) et la forme
de variante nue ne peuvent donc pas s'utiliser ; indiquez-les avec `the`, comme `(the point<i32> p)`.

**Les transtypages descendants regardent aussi l'instanciation.** Les arguments de type explicites servent à la
correspondance : `(the point<i32> p)` ne laisse passer que les valeurs de `point<i32>`, et un `point<string>` passe à
la branche suivante. C'est parce qu'une valeur se souvient de son type, arguments de type compris (le même mécanisme
qui choisit `print-object`).

```lisp
(if-let (pattern val) then els)     ; then (avec liaisons) si val correspond à pattern, sinon els. defmacro
(while-let (pattern val) body...)   ; boucle tant que val (réévaluée à chaque fois) correspond à pattern. defmacro
```

## 5. Itération

```lisp
(loop body...)                      ; une boucle infinie. en sortir avec break/return
(while test body...)                ; boucle tant que test est vrai. defmacro
(until test body...)                ; boucle tant que test est faux (la négation de while). defmacro
(dotimes (var count-expr) body...)  ; évalue count-expr une fois et fait parcourir 0..count-1 à var. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; itération à la manière de CL avec avancement parallèle. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; la version séquentielle de do (liaison let*, affectation dans l'ordre). defmacro
(doiter (var coll-expr) body...)    ; itère sur une valeur qui implémente le trait Iter. defmacro

(break)                             ; ne quitte que la boucle la plus interne. la valeur est toujours Unit
(return)                            ; ne quitte que la boucle la plus interne
(return value)                      ; quitte la boucle la plus interne avec une valeur
```

`break`/`return` ne quittent tous deux **que la boucle englobante la plus interne** (ce n'est pas un retour anticipé
de la fonction, et ils ne peuvent pas franchir une frontière de `lambda`). Le type d'un `loop` est la réunion des types
des valeurs des `break`/`return` qu'il contient (`!` s'il n'est jamais quitté). Pour quitter une fonction, utilisez
`return-from`, ci-dessous.

### 5.1 `block` / `return-from` — sorties nommées

```lisp
(block name body...)                ; une cible de sortie nommée. la valeur est la dernière forme,
                                    ; ou la valeur passée par return-from
(return-from name)                  ; quitte ce bloc avec Unit
(return-from name value)            ; quitte avec une valeur
```

**Chaque fonction de `defun` / `defmethod` / `labels` établit implicitement un bloc portant son propre nom** (comme en
CL). `(return-from f v)` est donc un retour anticipé de la fonction :

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` est une sortie **lexicale**, et le nom est **résolu là où il est écrit** : le vérificateur associe un
`return-from` au `block` englobant et réunit le type de sa valeur au type de sortie du bloc. Ainsi :

- Un `return-from` sans `block` correspondant est une **erreur de type** (pas une erreur d'exécution).
- Une valeur dont le type ne s'accorde pas avec les autres sorties ni avec le type du corps est une **erreur de type**
  (la même règle que pour les branches de `match`).
- Si des blocs de même nom sont imbriqués, **le plus interne l'emporte** (la règle de masquage de CL).
- **Il ne peut pas franchir les frontières de fonctions.** Depuis l'intérieur d'un `lambda`, on ne peut pas sortir vers
  un `block` extérieur (`lambda` n'établit pas de bloc : les blocs implicites de CL ont besoin d'un *nom*, et les
  fonctions anonymes n'en ont pas). Ce qui doit franchir, c'est `catch`/`throw` (chapitre 8, qui est **dynamique**).

Comme `break`/`return` (chapitre 5), c'est une sortie **statique** ; dans le code compilé, c'est donc un branchement
vers un bloc de base fixé à la compilation. S'il y a un `unwind-protect` entre les deux, son `cleanup` s'exécute
(chapitre 8).

Si vous n'écrivez jamais `return-from`, le bloc implicite ne coûte rien.

### 5.2 `loop` étendu (le LOOP de CL)

**Si le premier élément de `loop` est un mot-clé**, il est lu comme une suite de clauses. Sinon, il reste la boucle
simple ci-dessus, et le sens des `loop` existants ne change pas (la même règle que la règle de boucle simple de CL
elle-même).

CL écrit les mots de clause comme des symboles nus (`(loop for i from 1 to 3 collect i)`), mais ici **ce sont tous des
mots-clés** : un `for` nu serait juste une référence de variable, et être un mot-clé est aussi ce qui le distingue
d'une boucle simple. L'exception est `=`, qui sépare une variable d'une valeur : sa position est sans ambiguïté ; il se
lit donc nu ou comme mot-clé (`:=`).

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Clauses de variables** (écrites avant les clauses de corps. C'est la règle de CL : écrites après, elles pourraient
se lire comme « n'itérer qu'à partir d'ici » ; c'est donc une erreur) :

| Clause | Signification |
|---|---|
| `:with v = e` | Lie une fois. Peut lire les variables des clauses précédentes |
| `:for v :in s` / `:for v :across s` | Les éléments d'un `Iter` dans l'ordre. La distinction liste/vecteur de CL n'existe pas ici ; ce sont donc deux écritures de la même clause |
| `:for v :on s` | Les **suffixes** successifs. CL passe la cellule de queue partagée, mais un `Iter` n'a pas de queue à partager ; chacun est donc un nouveau `Vector` |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Comptage. `:downfrom`/`:upfrom` fonctionnent aussi |
| `:for v = e [:then f]` | Commence par `e`, puis à partir de la deuxième fois utilise `f` (sans `:then`, `e` à chaque fois) |
| `:repeat n` | Itère autant de fois |

Avec plusieurs `:for`, ils avancent **en parallèle**, et la boucle se termine dès que l'un d'eux est épuisé.

**Clauses de corps** (exécutées à chaque fois, dans l'ordre écrit) :

| Clause | Signification |
|---|---|
| `:do form...` | Pour les effets de bord |
| `:collect e [:into v]` | Collecte dans un `Vector<T>` |
| `:append e [:into v]` | Ajoute le contenu d'un `Iter` |
| `:sum e` / `:count e` | La somme / le nombre de fois où c'était vrai |
| `:maximize e` / `:minimize e` | Le maximum / minimum. **`Option<T>`** (comme CL renvoie nil pour une séquence vide ; un type `Ord` quelconque n'a pas de plus petit élément) |
| `:always e` / `:never e` | `true` si tous sont vérifiés ; `false` immédiatement dès que l'un échoue |
| `:thereis e` | `e` est une **`Option<T>`**. Renvoie le premier `some`, ou `none` s'il n'y en a pas (c'est ce qui correspond à la « première valeur non nil » de CL ; pour tester un `bool`, utilisez `:always`/`:never`) |
| `:while e` / `:until e` | **Se termine normalement** ici (`:finally` s'exécute, et ce qui a été collecté est la réponse) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Rend une clause conditionnelle |
| `:return e` | Sort immédiatement avec cette valeur (`:finally` ne s'exécute pas, comme en CL) |
| `:initially form...` / `:finally form...` | Avant la boucle / à la fin normale |

**`:named name`** (avant toute autre clause, une seule fois) enveloppe toute la boucle dans `(block name …)`.
`(return-from name e)` peut sortir d'un coup même depuis des boucles imbriquées, et comme pour `:return`, `:finally` ne
s'exécute pas. Sans nom, aucun bloc n'est établi : la `loop` sans nom de CL établit `block nil`, mais il n'y a pas de
`nil` ici, et `break`/`return` (chapitre 5) fournissent déjà « quitter la boucle la plus interne ».

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

Omettre `:finally (return 0)` est une **erreur de type**. Ce ne sont que les règles de `block` à l'œuvre (5.1) : le
type `int` de la sortie ne s'accorde pas avec le `()` que laisse la boucle quand elle est épuisée.

**La valeur de la boucle** est l'accumulation de la clause d'accumulation s'il y en a une (la première s'il y en a
plusieurs), `true` pour `:always`/`:never`, `none` pour `:thereis`, et `()` s'il n'y en a pas. Si la dernière chose de
`:finally` est `(return e)`, c'est la valeur : l'idiome `finally (return …)` de CL, le seul moyen pour une boucle qui
n'accumule pas de nommer sa propre réponse.

**Différences avec CL / ce qui n'est pas inclus** :

- **Les mots de clause sont des mots-clés** (ci-dessus).
- `:maximize`/`:minimize`/`:thereis` renvoient `Option<T>` (il n'y a pas de nil).
- **N'écrire que `:return`, sans accumulation ni `:finally`, est une erreur.** CL renvoie nil quand la boucle est
  épuisée, mais cela n'existe pas ici ; la boucle doit donc dire quelle est sa valeur quand elle est épuisée.
- Joindre des clauses parallèles avec `:and`, `:being`/l'itération dédiée sur les tables de hachage, `:it` et `:nconc`
  ne sont pas inclus.
- Le type d'élément de `:collect` vient du type de l'expression accumulée. Tenter de collecter un type qui **ne peut
  pas s'écrire comme nom de type**, comme un type de fonction, est une erreur qui le dit.

## 6. Valeurs fonctionnelles et appels

```lisp
(lambda (params) RetType body...)   ; crée une valeur fonctionnelle de première classe (une fermeture)
(labels ((name (params) RetType body...) ...) body...)   ; définitions de fonctions locales pouvant être mutuellement récursives
(apply f arg1 ... argN rest-list)   ; appelle f (une fonction variadique avec &rest) en étalant rest-list
```

Les fonctions nommées peuvent aussi être passées telles quelles comme valeurs (comme arguments de fonctions d'ordre
supérieur, etc.).

## 7. Autres formes spéciales

```lisp
(setq var value ...)                ; l'affectation de variable de CL. juste une suite de (setf var value). defmacro
(psetq var value ...)               ; affectation parallèle. évalue d'abord toutes les valeurs, puis affecte. defmacro
(psetf place value ...)             ; psetq généralisé aux emplacements (le même développement). defmacro
(setf place value)                  ; affectation à un emplacement. un emplacement est un nom de variable / var::field /
                                     ; un appel de la forme (accessor recv key...). valide si le
                                     ; type statique de recv a une méthode d'instance nommée
                                     ; set-{accessor} (pour le get de Vector<T> et HashTable<K,V>,
                                     ; set correspond par exception ; sinon set-accessor-name).
                                     ; la valeur est la valeur affectée (comme en CL). donc
                                     ; dans (if c (setf x 1) ()), then et else n'ont pas des types concordants
(incf place)  (incf place delta)    ; place += delta (delta=1 si omis). le résultat est comme avec setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 si omis)
(rotatef place1 place2 ... placeN)  ; fait tourner N emplacements (nouveau place1=ancien place2, ...,
                                     ; nouveau placeN=ancien place1). sous-formes de chaque emplacement évaluées une fois
(shiftf place1 ... placeN newvalue) ; décale à gauche les valeurs de place2..N et met newvalue dans placeN.
                                     ; la valeur de retour est l'ancienne valeur de place1
(list e1 e2 ... en)                 ; s'expanse en (cons e1 (cons e2 (... ()))). () sans argument.
                                     ; chaque élément est converti implicitement en Sexpr (comme le cons de CL, il
                                     ; peut contenir n'importe quelle valeur). les scalaires (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) sont enveloppés dans la variante Sexpr correspondante, et defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> et semblables y entrent tels quels
                                     ; (sans coût de conversion). de même pour les arguments &rest/format.
(source-file)                       ; le nom du fichier d'où cette forme a été lue (string). fixé comme
                                     ; constante à la vérification. correspond au *load-pathname* de CL, mais n'est
                                     ; pas une variable : les corps de modules s'exécutent après la vérification ; on ne peut donc
                                     ; pas compter sur « en cours de chargement », alors qu'à la vérification c'est toujours connu.
                                     ; pour les sources qui ne sont pas des fichiers, le nom que leur donne le lecteur (<stdin>/<input>)
(quote datum)                       ; identique à 'datum. le renvoie comme donnée Sexpr sans l'évaluer
(quasiquote template)               ; identique à `template. insère des expressions dans le modèle avec ,/,@
(documentation name)                ; renvoie la docstring de name (un nom nu ou Type::method) sous forme d'Option<string>
(panic message)                     ; message : string. termine anormalement sur une erreur irrécupérable. type !
(unreachable)                       ; s'expanse en (panic "unreachable"). defmacro
(todo)                              ; s'expanse en (panic "todo"). defmacro
(as Type expr)                      ; conversion de type numérique/caractère. les conversions qui peuvent échouer déclenchent alors un panic
(try-as Type expr)                  ; comme as, mais renvoie le résultat sous forme d'Option<Type> (None en cas d'échec)
(print control args...)             ; développe le format et écrit sur la sortie standard (sans saut de ligne)
(println control args...)           ; idem (avec un saut de ligne à la fin)
(format dest control args...)       ; le format de CL. renvoie la chaîne développée
(pprint x)                          ; affiche joliment. écrit d'abord un saut de ligne, comme en CL
(pprint-fill x)                     ; mise en page remplie
(pprint-linear x)                   ; tout sur une ligne ou un élément par ligne
(pprint-tabular x [colinc])         ; mise en page tabulaire (16 colonnes par défaut)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; construire soi-même un bloc logique
```

La famille `print`/`println`/`format`/`pprint` sont des formes spéciales ; leurs arguments variadiques (un seul objet
pour la famille `pprint`) sont donc enveloppés en `Sexpr` avec leurs propres types avant d'être passés : c'est
pourquoi `(println "~a" my-struct)` fonctionne simplement. Les détails des directives de format et du pretty printer
se trouvent dans [Directives de format](functions/format.md) et [Affichage](functions/printing.md#4-le-pretty-printer).

`as`/`try-as` ne traitent que le catalogue numérique et caractère (entre `int`, les types entiers de largeur fixe,
`f32`/`f64`/`ratio`/`char`). Le même type n'est pas une conversion. **Les conversions entre largeurs d'entiers (y
compris `int`) et entre `f32`↔`f64` sont de vraies conversions** : `as` tronque / arrondit, et `try-as` répond si cela
tient dans cette largeur (précision). `(as int x)` est l'élargissement exact depuis une largeur fixe, et `(as i32 n)`
la troncature depuis `int`. Entier → `char` peut échouer hors limites ; `as` déclenche donc un panic et `try-as` donne
`None`. Tout le reste (l'élargissement, et la troncature de `float->int`/`ratio->int`) réussit toujours.
`float->int`/`ratio->int`/`char->int` aboutissent à `int`, et si une largeur plus étroite est demandée, `int->W` est
appelé ensuite. C'est du sucre qui s'expanse vers les méthodes de conversion correspondantes
(`int->char`/`int->int`/`int->W`, etc. dans [Nombres](functions/numbers.md)).

`documentation`, comme `quote`/`compile`, est une forme spéciale qui lit `name` sans l'évaluer, comme un symbole nu /
chemin `::` non évalué. Contrairement au `(documentation 'name 'function)` de CL, elle ne prend pas d'argument de
type : elle résout `name` dans l'ordre variable → fonction → type → trait → macro (la même priorité que lorsqu'un
identificateur nu est évalué comme expression) et renvoie la docstring de la définition trouvée
(`(documentation Type::method)` est pour les méthodes). Un échec de résolution (aucune définition de ce nom) est une
erreur à la vérification ; une définition qui existe mais sans docstring donne `Option::none`. Tout est décidé comme
constante à la vérification : aucune recherche n'a lieu à l'exécution. Les noms libres qualifiés par un module
(`mod::name`, sauf `Type::method`) ne sont pas pris en charge.

## 8. Sorties non locales (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; exécute body. si (throw 'tag v) se produit n'importe où
                                    ; où body parvient, ce v devient la valeur
(throw 'tag value)                  ; sort vers le (catch 'tag ...) englobant dynamiquement le plus proche
(unwind-protect protected cleanup)  ; exécute cleanup quelle que soit la façon dont protected est quitté
```

Contrairement à `break`/`return` (chapitre 5), c'est une sortie **dynamique** : `throw` ne cherche pas lexicalement le
`catch` qui l'entoure, et atteint un `catch` de même étiquette à travers un nombre quelconque d'appels de fonctions.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; si rien n'est trouvé, la valeur à la fin comme d'habitude
```

- **Les étiquettes ne sont que des symboles littéraux** (`'done`). Contrairement à CL, elles ne sont pas évaluées.
- **Une étiquette porte un type.** Le type est décidé à la première utilisation de `'tag`, et chaque `throw`/`catch`
  ultérieur du même symbole est vérifié par rapport à lui. L'utiliser avec un autre type est une erreur de type.
- Le type de `throw` est `!` (il diverge). Le type de `(catch 'tag expr)` est la réunion du type de `expr` et du type
  de l'étiquette.
- La valeur de `unwind-protect` est la valeur de `protected`. La valeur de `cleanup` est ignorée. `cleanup` s'exécute
  quelle que soit la façon dont `protected` est quitté : en plus de la fin normale, de `throw` et de `panic`, il
  s'exécute aussi quand on le quitte par `break`/`return`/`return-from`. Une sortie non locale par `cleanup`
  lui-même l'emporte sur la sortie en cours.
- Les `unwind-protect` imbriqués s'exécutent de l'intérieur vers l'extérieur. Un `break` qui quitte une boucle **à
  l'intérieur** de `protected` n'a pas quitté `protected` ; son `cleanup` ne s'exécute donc pas.

Les conditions de CL (`define-condition`/`handler-bind`/`invoke-restart`) ne sont pas adoptées. Elles ne
s'accordent pas avec le typage statique ; les échecs récupérables s'expriment donc avec `Result` (chapitre 9).

## 9. Principes de gestion des erreurs

- Échecs récupérables : `Result<T,E>` + `match`. Échecs irrécupérables (bogues, invariants violés) : `panic`.
- Il n'y a pas de syntaxe correspondant à `?`/try. Les bifurcations s'écrivent explicitement avec `match`.
- Les noms de fonctions et de formes spéciales n'utilisent pas `!` (opérations destructives) ni `?` (prédicats) comme
  suffixes. Les prédicats se nomment avec un suffixe `-p`/`p` (`zerop`, `consp`, etc.) ou un préfixe `is-`
  (`is-some`, `is-ok`, etc.).

## 10. Compilation

```lisp
(compile name)                      ; compile en JIT un defun/une méthode déjà définie en code natif
(compile-file src-path out-path)    ; compile en AOT un fichier source en exécutable natif (ignore le `(main)` final)
(dump path)                         ; écrit l'environnement courant (informations de type + corps compilés) dans un fichier
(disassemble name)                  ; affiche ce que devient cette définition (code machine de l'hôte par défaut, LLVM IR avec true en second argument)
```

`compile` est une forme spéciale ; `name` n'est pas évalué et se lit comme un symbole nu / chemin `::` non évalué (une
chaîne est une erreur de type). Les fonctions génériques ne peuvent pas être ciblées : une copie par type est créée à
chaque endroit d'utilisation ; il n'existe donc pas de corps compilé unique. **Un nom qui ne peut pas être résolu est
une erreur à la vérification** et n'est jamais reporté à l'exécution (il y a des messages distincts pour : le type
existe mais pas cette méthode / ni le type ni la fonction n'existent / un nom nu non défini). La visibilité est
traitée ici comme pour toute autre référence : « existe mais n'est pas visible d'ici » échoue à la vérification,
exactement comme « ne se résout pas ».

Les fonctions appelées sont aussi compilées transitivement ; **une fonction qui appelle (même indirectement) quelque
chose qui ne peut pas être compilé ne peut pas être compilée**. Le processus ne plante pas ; c'est refusé avec une
erreur qui le dit. Toutes les fonctions intégrées peuvent être compilées ; les seules fonctions refusées de cette
façon sont celles qui appellent les opérations suivantes, propres à l'interpréteur :

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

Les opérations propres à l'interpréteur sont `compile`/`compile-file`/`dump` et
`trace`/`untrace`/`step`/`disassemble`
([Outils de l'implémentation](functions/system.md#5-outils-de-limplémentation-clhs-252)). Plutôt que des choses
impossibles à compiler, ce sont des opérations du côté qui compile (ce que `dump` écrit, c'est l'environnement de
l'interpréteur lui-même, qu'un exécutable AOT n'a pas ; ce que `trace` observe et où `step` s'arrête, ce sont les
chemins d'appel de l'interpréteur en cours ; et `disassemble` utilise le compilateur lui-même).
`room`/`dribble`/`ed` n'en font pas partie et se compilent normalement.

Ce qui **peut** être compilé : les E/S de flux et de fichiers, `random`, `gensym`, `symbol->string`/`string->symbol`,
`parse-int`/`parse-float`, `get-universal-time`/`get-internal-real-time`, `exit`, les fonctions transcendantes, les
opérations sur les bits, `catch`/`throw`/`unwind-protect`, les quatre `eq`/`eql`/`equal`/`equalp` (ce qui permet de
compiler `case` pour tout type), toute la famille d'affichage y compris `print`/`println`/`format`/`pprint` et
`pprint-logical-block`, `read` et `eval`. La bibliothèque standard est livrée déjà compilée.

Un exécutable AOT ne contient que les fonctionnalités qu'utilise le programme. Un programme qui n'affiche rien n'a pas
de moteur de formatage, un qui n'appelle pas `read` n'a pas de lecteur, et un qui n'appelle pas `eval` n'a ni
vérificateur ni interpréteur.

En ligne de commande, `typl -c src-path [-o out-path]` (`-c` peut aussi s'écrire `--compile`) fait la même chose que
`compile-file`. Sans `-o`, la sortie est `src-path` sans l'extension `.typl`. Par défaut, la bibliothèque statique
`libtypelisp_front.a` liée aux exécutables est, pour une version release de `typl`, celle que `typl` porte en
lui-même, écrite au premier lien dans `$TYPELISP_HOME/lib/<identifiant de build>/` (ou dans
`~/.typelisp/lib/<identifiant de build>/` sans `TYPELISP_HOME`) et utilisée depuis là ; pour une version debug,
celle de l'endroit où `typl` a été construit. `typl --remove-lib` supprime ce que ce `typl` a écrit. Avec `--others`,
il supprime celles des autres identifiants de build ; avec `--all`, celles de tous les identifiants de build. Avec
`typl --lib-dir DIR`, celle de `DIR` est utilisée (pour `-c` comme pour `compile-file`), et si elle n'y est pas, c'est
une erreur au démarrage.

### 10.1 Dumps

```lisp
(dump "session.typld")     ; en écrire un
```
```sh
typl --image session.typld prog.typl   # repartir de celui-ci
typl --image session.typld             # la REPL aussi
```

Un dump contient les informations de type et les corps compilés dans un seul fichier. Ce qu'écrit `(dump path)`,
c'est ce que la session courante a chargé (la bibliothèque standard, ou un dump passé avec `--image`) plus **ce que
la session elle-même a défini**. La sortie est donc autonome, et `typl --image` remet en place le même environnement.
Ce que la session a compilé avec `(compile f)` est écrit sous sa forme compilée.

Ce qui est sauvegardé, ce sont **les définitions, pas l'historique** :

- Les expressions de niveau supérieur de la session (`(println ...)`, etc.) ne sont pas incluses. Ce serait un
  problème si le chargement les réexécutait.
- Les variables globales reviennent avec **la valeur de leur initialiseur réexécuté**, pas la valeur au moment du
  dump. C'est une différence délibérée avec le `save-lisp-and-die` de SBCL (qui écrit le tas tel quel), et ce choix
  fait disparaître toute une famille de problèmes : les « valeurs qui ne peuvent pas être sauvegardées », comme les
  flux ouverts, les pointeurs de fonctions de fermetures et la mémoire externe.
- Contrairement à `save-lisp-and-die`, **le processus ne meurt pas**, puisque l'écriture n'abîme pas l'image.

Un dump enregistre les versions de la bibliothèque standard et du compilateur de l'implémentation qui l'a écrit. Le
charger avec un `typl` d'une autre version est une erreur ; il n'est jamais accepté silencieusement.

### 10.2 `eval` dans les exécutables AOT

`eval` vérifie les types par rapport à « l'environnement global courant » puis évalue
([Analyse et évaluation](functions/system.md#6-analyse-et-évaluation)). Cet environnement (les tables de signatures,
de types et de macros que consulte le vérificateur, et les corps que l'interpréteur peut exécuter) **n'est pas dans
le code machine**. Une fonction compilée n'est rien de plus qu'un symbole placé à une adresse ; elle n'a ni ses types
d'arguments ni de table pour chercher des corps par nom.

Ainsi, seulement pour les programmes qui appellent `eval`, `compile-file` **construit cet environnement à la
compilation et l'écrit dans l'exécutable**. Le format est le même qu'un dump, contenant la partie de la bibliothèque
standard et la partie propre au programme. Au démarrage, on ne fait que le restaurer : le source n'est pas relu, et
rien n'est revérifié. Rien n'est ajouté aux programmes qui n'appellent pas `eval`.

Conséquences :

- **Le démarrage est plus long et l'exécutable plus gros**, puisque le code du vérificateur et de l'interpréteur et
  un instantané de l'environnement y entrent. Le tas est aussi un peu agrandi.
- **Les formes passées à eval sont interprétées.** Même quand la forme passée à eval appelle les propres fonctions du
  programme, ce qui s'exécute, c'est le corps interprétable que contient l'instantané. Le résultat est le même ; seule
  la vitesse diffère.

Le stockage des variables globales est **partagé** avec le code compilé (les mêmes emplacements). Un initialiseur de
`defvar` est exécuté une fois par l'initialisation compilée, et la restauration l'ignore ; un initialiseur à effets
de bord ne s'exécute donc pas deux fois.

`compile-file` lit aussi la bibliothèque standard (et incorpore ses corps dans l'exécutable) ; les fonctions de la
bibliothèque standard comme `abs`/`gcd`, ainsi que `(impl print-object ...)` et `(defmethod print-object ...)`, peuvent
donc s'utiliser avec AOT.

`compile-file` accepte aussi `use` (et `import`/`shadowing-import`). Le `(use m)` du fichier d'entrée trouve les
fichiers selon les mêmes règles que `typl file.typl`, et les fichiers de dépendance trouvés sont aussi compilés et liés
dans l'exécutable : une organisation où `main.typl` lit `http.typl` via `(use http)` peut être compilée en AOT telle
quelle. Les définitions propres du fichier d'entrée vont aussi dans le module qui porte le nom du fichier, comme avec
`typl file.typl` (`point` dans `p.typl` est `p::point`). La représentation imprimée des valeurs
(`#<p::point x: 1 y: 2>`) est donc la même quelle que soit la façon de l'exécuter.

## 11. Macros de lecture (readtable)

Ce que fait le lecteur **quand il rencontre un certain caractère** peut être remplacé depuis le programme (CLHS 23.1).

```lisp
(set-macro-character c f)             ; f lit le caractère c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f lit la séquence de deux caractères d s
(get-dispatch-macro-character d s)    ; Option<f>
```

Le type de `f` est `(fn (string-input-stream char) Option<Sexpr>)`. Le premier argument est **un flux sur le texte pas
encore lu**, et le second est **le caractère déclencheur** (le second caractère pour un dispatch). La valeur de retour
devient la donnée lue à cet endroit. Le flux est un type concret plutôt que `:dyn PeekInput` parce que le lecteur passe
toujours cette seule sorte : `read-sexpr` / `read-char` / `peek-char` / `unread-char` / `read-delimited-list` prennent
tous `(where (PeekInput S))` ; ils fonctionnent donc tous tels quels sur le type concret.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => lu comme (not (equal 1 2)), c'est-à-dire true
```

Le lecteur **regarde les caractères macro avant la syntaxe intégrée** ; il peut donc aussi prendre en charge `(` et
`'`. Les sous-caractères de `#` enregistrés de cette façon ont priorité sur les `#b`/`#x`/`#.` intégrés. Un caractère
autre que `#` devient sur-le-champ un caractère de dispatch quand il est passé à `set-dispatch-macro-character` : il
n'y a **pas** d'équivalent au `make-dispatch-macro-character` de CL. L'enregistrement fait déjà le travail ; une étape
séparée n'aurait rien à faire.

**Quand ils prennent effet** dépend du chemin de lecture, comme pour `#.` (chapitre 1) :

- La REPL et `(load ...)` exécutent une forme à la fois ; **des fonctions définies dans des formes antérieures**
  peuvent donc être enregistrées telles quelles.
- Les fichiers de modules sont vérifiés comme une unité et exécutés plus tard ; **seuls les appels à
  `set-macro-character` / `set-dispatch-macro-character` s'exécutent immédiatement** (le rôle du
  `(eval-when (:compile-toplevel) ...)` de CL). Puisqu'ils s'exécutent immédiatement, **la fonction passée doit déjà
  exister à ce moment-là**. Un `defun` du même fichier ne s'est pas encore exécuté ; écrivez donc un `lambda`, ou
  utilisez la bibliothèque standard ou quelque chose qui s'est déjà exécuté. Seuls les appels de niveau supérieur
  sont couverts ; il ne regarde pas à l'intérieur de `progn` ou de `let`.

Les `read` / `read-from-string` intégrés consultent aussi la readtable (comme en CL).

**Ce qui n'existe pas** : `*readtable*` et `copy-readtable`, ainsi que `readtable-case`. Les deux premiers parce
qu'une readtable **n'est pas une valeur** : une valeur devrait être « quelque chose qu'on peut remettre à un lecteur »,
mais le lecteur qui lit le source est en dehors du programme, sans endroit où la remettre. `readtable-case` parce que
le chapitre 1 décide que le lecteur de ce langage met toujours en minuscules (le `:downcase` de CL).


## 12. Concurrence (tâches)

**Une tâche est un thread léger** (dans les termes de Go, ce que lance une instruction `go`) et s'exécute de façon
coopérative (il n'y a pas de préemption). Le changement ne passe pas par le noyau, et l'état d'exécution vit sur le tas
plutôt que sur une pile machine ; les tâches sont donc peu coûteuses à créer en grand nombre.

**Les tâches s'exécutent en même temps sur plusieurs threads système** (parallélisme multicœur). Le nombre de threads
est la variable d'environnement `TYPELISP_THREADS` (le total, y compris le thread qui exécute `main` ; par défaut, le
parallélisme de la machine). Dans `typl`, **seules les tâches compilées** s'exécutent sur d'autres threads, et les
tâches interprétées s'exécutent sur le thread de l'interpréteur (12.7). Les données partagées passent par `Mutex<T>`
ou `Chan<T>` ; des lectures et écritures simultanées qui ne le font pas sont indéfinies, comme en Go (12.7).

Du vocabulaire, **seuls `task` / `thread` / `select` sont des formes spéciales** ; le reste sont des fonctions, méthodes
et macros ordinaires ([Tâches et canaux](functions/concurrency.md)).

### 12.1 `task` — lancer une tâche

```lisp
(task (f arg...))                   ; renvoie Task<T>, où T est le type de retour de f
```

**Il ne prend que la forme d'un appel.** `f` et chaque `arg` sont évalués là où le `task` est écrit, dans l'ordre
écrit, et seul **l'appel** a lieu dans la nouvelle tâche. C'est la même règle que le `go f(x)` de Go, et c'est aussi
pourquoi il prend une forme d'appel plutôt qu'un thunk : un thunk capturerait ses arguments sans les évaluer.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i est évalué sur place à chaque fois ; pas de piège de capture

(task ((lambda () ()                ; pour exécuter un corps quelconque, appeler un lambda
         (println "start")
         (send ch 1))))
```

Les formes spéciales (`if` / `let` / `progn` …) ne peuvent pas s'écrire directement sous `task`.

**Pourquoi ce ne peut pas être une fonction** : écrire `(spawn (lambda () T body...))` exigerait d'écrire `T`, puisque
`lambda` exige une annotation du type de retour, et une macro ne connaît pas le type de retour de `(f a b)`. Seul le
vérificateur le connaît.

### 12.2 `thread` — lancer une tâche sur un thread système dédié

```lisp
(thread (f arg...))                 ; renvoie Thread<T>, où T est le type de retour de f
(join th)                           ; attend la fin et renvoie sa valeur (autant de fois qu'on veut)
```

La forme et les règles d'évaluation sont les mêmes que pour `task` (il ne prend qu'une forme d'appel, et `f` et `arg`
sont évalués là où il est écrit). La différence est l'endroit où il s'exécute : **il lance un thread système dédié à
cette tâche et ne s'exécute que dessus**. Il n'est pas multiplexé avec d'autres tâches ; appeler une fonction C
bloquante (`defffi`) à l'intérieur n'arrête donc que ce thread, et les autres tâches progressent. À l'intérieur,
`task`, `send`, `recv` et le reste s'utilisent tels quels.

- `Thread<T>` est le pendant de `Task<T>`. Comme `wait`, `join` arrête **la tâche appelante**, et la valeur est mise
  en cache. Quand la tâche se termine, le thread se termine aussi.
- Les règles de panic sont les mêmes que pour `task` (tout le processus s'arrête). Quand `main` revient, le processus
  se termine.
- Pour l'écrire comme une fonction, utilisez `(Thread::spawn (lambda () T body...))` (le `std::thread::spawn` de
  Rust). On peut aussi passer une fonction nommée.
- **Seul du code compilé s'exécute sur un thread dédié.** Quand `typl` évalue `(thread (f ...))` ou
  `Thread::spawn` en interprétation, il compile sur place la fonction à exécuter (et ce qu'elle appelle) avant de
  l'exécuter. Ce qui ne peut pas être compilé (un `lambda` qui fait référence à des variables locales extérieures,
  la construction d'une structure, etc.) est, avant le lancement du thread, un panic traité comme un
  `(panic ...)`. Un `lambda` qui fait référence à des variables locales peut être passé s'il est créé dans une
  fonction compilée.

### 12.3 `select` — attendre plusieurs opérations de canal à la fois

```lisp
(select
  ((v (recv ch1)) body...)          ; une branche de réception. v est lié à une Option<T>
  ((send ch2 x) body...)            ; une branche d'envoi
  (else body...))                   ; facultative. **si elle est écrite, elle vient en dernier**
```

- **Avec `else`, il ne bloque pas** (le `default` de Go). Sans, il attend que l'une devienne possible.
- **Si plusieurs sont possibles en même temps, l'une est choisie au hasard** (dans l'ordre écrit, les branches
  suivantes seraient affamées).
- Le `v` d'une branche de réception est une **`Option<T>`**. Un canal fermé est « une réponse », pas une raison de
  sauter la branche ; faites donc un `match` dessus à l'intérieur de la branche.
- Le type est **la réunion des types de tous les corps de branches** (la même règle que pour les branches de
  `match`).
- `(select)` sans branche est une erreur de type (le `select{}` de Go, bloquant pour toujours, n'est pas adopté). Un
  `select` avec seulement `else` aussi, puisque c'est la même chose qu'écrire le corps directement.

**Les expressions de canal et les valeurs à envoyer sont évaluées une fois chacune, de gauche à droite, quelle que
soit la branche choisie** (la même discipline que `case` a pour ses clés).

```lisp
(select                             ; recevoir avec un délai
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([un canal qui livre après un délai](functions/concurrency.md#5-after--un-canal-qui-livre-après-un-délai)) est
« un canal qui livre une valeur après `sec` secondes », correspondant au `time.After` de Go.

### 12.4 Interaction avec les autres fonctionnalités

| Fonctionnalité | Rapport avec les tâches |
|---|---|
| `catch` / `throw` | **Ne franchissent pas les frontières de tâches.** Un `throw` qui tente de quitter le corps d'une tâche est un panic |
| `unwind-protect` | Le nettoyage s'exécute quand une tâche se termine naturellement. **Il ne s'exécute pas quand le processus se termine parce que la tâche principale s'est terminée** |
| `block` / `return-from` | Lexicaux ; ils ne franchissent donc pas les frontières de `lambda` |
| `panic` | Comme en Go, tout le processus s'arrête. `wait` n'observe pas un panic comme une valeur |
| `dlet` | **Pas une liaison par tâche.** Il « emprunte et rend une globale » ; les tâches interfèrent donc entre elles |
| Sortie standard | Partagée par toutes les tâches. La sortie d'un `println` n'est jamais mêlée à d'autres au milieu d'une ligne |
| `compile` / `eval` | Pas de restrictions. `(compile f)` dans une tâche fonctionne |

### 12.5 Où les tâches changent

L'ordonnancement est coopératif ; **les tâches ne changent donc que là où vous l'écrivez** : `(yield)`,
`(sleep ...)`, `(wait ...)`, **les opérations de canal qui doivent attendre** (`send`/`recv`/`select`), et **les
opérations sur sockets qui doivent attendre** (`accept` / `tcp-connect` (y compris la résolution de noms) / lecture et
écriture sur des sockets / `recv-from` ; [Réseau](functions/network.md)). Toutes les sockets sont non bloquantes : si
l'une n'est pas prête, seule cette tâche s'arrête, et elle reprend quand le système signale qu'elle est prête, sur le
même modèle que le netpoller de Go. C'est seulement quand aucune tâche ne peut s'exécuter que l'implémentation attend
le système jusqu'à l'échéance de `sleep` la plus proche.

Les opérations de canal qui peuvent répondre sur-le-champ (un `send` avec de la place dans le tampon, un `recv` avec
une valeur en attente, `(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **ne consomment pas le tour**. Cela signifie
qu'on n'est pas interrompu à l'improviste par une lecture, et c'est traité différemment de `(sleep 0.0)`, qui est le
« céder pendant 0 seconde » de CL.

**Il n'y a pas de préemption.** Une boucle serrée qui n'appelle rien affame les autres tâches. Cependant, les boucles
compilées rendent périodiquement la main à l'ordonnanceur ; une boucle serrée compilée ne les affame donc pas.

### 12.6 Code compilé et tâches

Le code compilé peut aussi suspendre des tâches. Il en va de même pour les exécutables produits avec `compile-file` :
`main` s'exécute comme tâche principale de l'ordonnanceur, et `task`, `sleep`, `wait`, les canaux et les attentes sur
sockets fonctionnent tous avec le même sens que dans `typl`. Quand `main` revient, le processus se termine et les
tâches restantes sont interrompues (comme en Go). L'interpréteur n'est jamais mis dans l'exécutable pour les besoins
de l'ordonnanceur.

La seule exception est « à l'intérieur d'un callback FFI C », où les opérations qui **devraient attendre** sont des
erreurs (plus aimable qu'un interblocage silencieux) : pendant qu'une fonction passée avec `defffi` est appelée depuis
C, la pile de C est au-dessus, et il n'y a aucun moyen de suspendre la tâche pour la reprendre plus tard.

Les endroits suivants sont aussi des fonctions appelées au milieu d'une tâche, mais ne peuvent pas suspendre : les
méthodes `print-object`, `~/name/` dans `format`, les macros de lecture, l'intérieur d'`eval` et les initialiseurs
de `defvar` dans les exécutables AOT. Ici, **les opérations qui répondent sans attendre passent** (`(recv ch)` avec
une valeur dans le tampon, `read-line` sur une socket avec des données déjà reçues, `(task ...)`, `(yield)`, etc.), et
**les opérations qui devraient vraiment attendre sont des erreurs** (pas un arrêt immédiat du processus, mais un
panic comme `` `recv` cannot block: ... ``, traité comme un `(panic ...)`).

### 12.7 Différences avec Go

- **Dans `typl`, seules les tâches compilées partent sur d'autres threads.** L'état de l'interpréteur ne peut pas être
  partagé entre threads ; les tâches d'un `task` interprété s'exécutent donc sur le thread de l'interpréteur. Une tâche
  compilée aussi **passe sur le thread de l'interpréteur et y reste** (elle ne revient pas) au moment où elle appelle
  une valeur fonctionnelle interprétée, appelle une méthode `:dyn` que personne n'a compilée, ou appelle
  `eval`/`macroexpand`/`read`. Si un long calcul touche du code interprété ne serait-ce qu'une fois en route, le reste
  s'exécute sur le thread de l'interpréteur.
- **Dans `typl`, les workers ne vivent que le temps d'une évaluation de niveau supérieur.** Pendant que la REPL attend
  une saisie, et entre les formes de niveau supérieur, les autres threads ne font pas avancer les tâches (les tâches
  restantes reprennent là où elles en étaient à l'évaluation suivante). À la fin d'une évaluation, il attend que chaque
  thread termine son pas en cours ; si une fonction C (`defffi`) continue de bloquer à l'intérieur d'un `thread`,
  l'évaluation ne se termine donc pas avant son retour.
- **Affichage sur les workers** : les méthodes `print-object` / `~/name/` interprétées ne peuvent pas s'exécuter sur
  d'autres threads ; afficher de telles valeurs sur un autre thread est donc un panic traité comme un `(panic ...)`
  (`(compile T::print-object)`, ou afficher depuis la tâche principale).
- **Les courses de données sont indéfinies** (la même position que Go). Le résultat de plusieurs tâches modifiant la
  même valeur sans passer par `Mutex<T>` / `Chan<T>` n'est pas garanti.
- **`task` renvoie une valeur.** Contrairement à l'instruction `go` de Go, il renvoie un `Task<T>`, et `(wait t)`
  obtient le résultat.
- **Il n'y a pas de canaux nil.** L'idiome fan-in de Go (mettre à `nil` un canal fermé pour le retirer des branches de
  `select`) ne peut pas s'écrire ; lancez donc une tâche par entrée et réunissez-les avec un `WaitGroup`
  ([WaitGroup](functions/concurrency.md#4-waitgroup--attendre-n-achèvements)). C'est aussi la manière recommandée en
  Go, mais c'est **la première différence à laquelle se heurtent les habitués de Go**.
