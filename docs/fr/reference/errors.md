<!-- translated-from: docs/ja/reference/errors.md @ 919605e85abc17d3889701183c762718842ab3b4 -->
# Messages d'erreur

Ce que signifient les principaux messages d'erreur de `typl` et comment les corriger.

## 1. Lire une erreur

Les erreurs sont écrites sur la sortie d'erreur standard sous cette forme :

```text
error: fichier:ligne:colonne: sorte: message
```

La `sorte` indique quand l'erreur a été trouvée.

| Sorte | Quand | Signification |
|---|---|---|
| `type error` | Avant l'exécution (à la vérification) | Une erreur de types ou de noms. Cette forme n'est pas exécutée |
| (pas de sorte) | À la lecture ou à la vérification | Une erreur de syntaxe comme des parenthèses déséquilibrées, ou un nom introuvable |
| `panic` | Pendant l'exécution | Un échec irrécupérable. Le programme s'arrête après avoir exécuté le nettoyage de `unwind-protect` |

Les lignes qui commencent par `warning:` sont des avertissements, et le traitement continue.

`fichier:ligne:colonne` désigne l'expression fautive. Pour une erreur d'exécution qui survient dans une fonction de
la bibliothèque standard, il désigne l'endroit où le programme a appelé cette fonction. Certaines erreurs n'ont
pas de position (comme `error: panic: ...`).

Exemple :

```text
error: main.typl:1:24: type error: type mismatch: expected `i32`, found `string`
```

Cela signifie que l'expression à la ligne 1, colonne 24 de `main.typl` était une `string` là où un `i32` était
attendu.

## 2. Erreurs à la vérification

Les erreurs trouvées avant l'exécution. La forme n'est pas exécutée tant qu'elles ne sont pas corrigées.

### 2.1 Types

| Message | Signification et correction |
|---|---|
| ``type mismatch: expected `T`, found `U` `` | Une expression de type `U` se trouve là où le type `T` est nécessaire. Il n'y a pas de conversions implicites ; pour les nombres, convertissez avec `(as T x)`. `int` et `i32` sont aussi des types différents |
| ``integer literal 300 is out of range for u8 (0..=255)`` | Le littéral ne tient pas dans le type. Si vous voulez le tronquer, écrivez `(as u8 300)` |
| ``unknown type `foo`: no type of that name is visible here. ...`` | Il n'existe pas de type de ce nom. Définissez un type avant la première forme qui l'utilise (les types n'ont pas de déclaration anticipée). Si vous vouliez une variable de type, écrivez-la à une position de déclaration comme `<foo>` après le nom de la fonction ([Référence de la syntaxe 3.6](syntax.md#36-defstruct--structures-types-définis-par-lutilisateur)) |
| ``cannot infer type argument `t` for `vector::new` `` | Un argument de type ne peut pas être déterminé. Écrivez le type avec `the`, comme dans `(the Vector<int> (Vector::new))` |
| ``non-exhaustive match on `color`: 1/2 variants covered`` | Le `match` ne traite pas toutes les variantes. Ajoutez des branches pour les variantes manquantes, ou une branche `_` |
| ``type `pt` does not implement trait `eq` required by `:where` clause on type parameter `a` `` | La fonction exige un trait que le type passé n'implémente pas. Écrivez `(impl Eq pt ...)` ([Traits standard](functions/traits.md)) |
| ``` `sq` does not implement `shape`, so it cannot be used as `:dyn shape` ``` | Une valeur d'un type qui n'implémente pas le trait a été passée là où un `:dyn` est attendu. Écrivez l'`impl` |
| ``` `error` is a trait, not a type — write `:dyn error` for a trait object ``` | Un nom de trait a été écrit à la place d'un type. Écrivez `:dyn Error` |
| ``if: (if cond then else)`` | Le `if` a une mauvaise forme. `if` exige une branche « sinon ». Quand vous n'en avez pas besoin, utilisez `when` |

### 2.2 Noms

| Message | Signification et correction |
|---|---|
| `no such function: bar` | Il n'existe ni fonction ni méthode de ce nom. Vérifiez l'orthographe |
| ``no method `upcase` for type `int` (the type of the first argument, which selects the method); `upcase` is a method of `char`, `string` `` | Les méthodes sont choisies selon le type du premier argument. Une méthode de ce nom existe, mais pas pour le type du premier argument (ici `int`). La fin du message énumère les types qui ont la méthode (pour une méthode écrite pour une instanciation, l'instanciation, comme `box<int>`) |
| ``no method `twice` for type `box<string>` (the first parameter of the function this value is expected to be, which selects the method); `twice` is a method of `box<int>` `` | Une méthode passée comme valeur est choisie par le type du premier paramètre du type de fonction sous lequel elle est passée. Ce type (ici `box<string>`) n'a pas de méthode de ce nom. La fin du message énumère les types qui l'ont |
| `unbound variable: y` | Il n'existe pas de variable de ce nom. Vérifiez l'orthographe et la portée de la liaison (est-elle utilisée hors de son `let` ?) |
| ``cannot define variable `f`: a function of that name already exists here — a name has one meaning, whether a function, a variable or a macro`` | Deux éléments parmi une fonction, une variable globale et une macro ont reçu le même nom dans un module. Renommez l'un d'eux ([Référence de la syntaxe 3.10](syntax.md#310-module--use--espaces-de-noms)) |
| ``` `point` has no static method `new`; a struct's methods are written in its `impl`, ...``` | Aucun `new` n'est généré pour une structure. Créez la valeur avec `make-instance`, ou écrivez un `new` dans `impl` ([Référence de la syntaxe 3.5](syntax.md#35-impl--méthodes-dun-type)) |
| ``defmethod is gone — write methods in the type's `impl`: ...`` | `defmethod` n'existe pas. Les méthodes s'écrivent dans l'`impl` du type ([Référence de la syntaxe 3.5](syntax.md#35-impl--méthodes-dun-type)) |
| ``defstruct: (:constructor ...) is gone — ...`` | L'option `:constructor` n'existe pas. Le constructeur s'écrit dans `impl` comme méthode statique |
| ``method box::m: `box` has a `m` for every instantiation already, so an `impl` of `box<int>` cannot define its own ...`` | Une méthode d'un même nom a été écrite à la fois dans un `impl` dont les paramètres de type sont des variables (`Box<T>`) et dans l'`impl` d'une instanciation (`Box<int>`), dans un ordre ou dans l'autre. N'en garder qu'un ([Référence de la syntaxe 3.5](syntax.md#35-impl--méthodes-dun-type)) |
| ``impl show box<int>: `box` implements `show` for every instantiation already, ...`` | Le même trait a été implémenté pour `Box<T>` et pour `Box<int>`. N'en garder qu'un |
| ``use: unresolved `nosuch` `` | Le module nommé dans `use` est introuvable. Pour la correspondance entre noms de fichiers et chemins de modules, voir [Référence de la syntaxe 3.11](syntax.md#311-fichiers-et-modules-projets-à-plusieurs-fichiers) |
| `unresolved path: c::hidden` | Le module existe, mais pas le nom, ou il n'est pas visible faute de `pub` |
| `circular module dependency: a -> b -> a` | Des modules s'importent mutuellement avec `use`. Déplacez la partie commune dans un module séparé |
| ``return-from: no enclosing block named `nope` `` | Aucun `block` portant le nom donné à `return-from` ne l'englobe. Le bloc d'une fonction n'est utilisable qu'à l'intérieur de cette fonction |

### 2.3 Appels

| Message | Signification et correction |
|---|---|
| `f: expected 1 argument(s), got 2` | Le nombre d'arguments ne correspond pas |
| `f: unknown keyword argument :b` | Un argument mot-clé que la fonction ne possède pas a été passé |
| `circle: expected 1 field(s), got 2` | Le nombre de valeurs passées à une variante d'énumération ne correspond pas au nombre de champs de cette variante |
| ``make-instance point: slot `y` is not given and has no default`` | Un champ sans valeur par défaut n'a pas été donné à `make-instance` |
| ``make-instance point: no slot `z` (its slots are `x`, `y`)`` | Un champ que la structure n'a pas a été donné |
| ``make-instance geo::pt: slot `secret` is not `pub`, so it can be given only inside module `geo` `` | Un champ qui n'est pas `pub` a été donné hors du module qui définit le type. Demandez à ce module un constructeur `pub` |
| ``` `box::make` is written for `box<int>`, `box<string>`; the type the call is expected to return picks one, ...``` | Les `impl` de plusieurs instanciations ont une méthode statique de ce nom, et rien ne décide laquelle appeler. Indiquer le type attendu, comme dans `(the Box<int> (Box::make))` |
| ``compile: `box::twice` is written for more than one instantiation of `box`; name one: ...`` | Une méthode écrite par instanciation a été passée à `compile`, `trace`, `documentation`, etc. avec le seul nom du type. Écrire l'instanciation, comme dans `box<int>::twice` |
| ``setf: cannot assign to constant `k` `` | Un nom défini avec `defconstant` a été affecté. S'il doit changer, utilisez `defvar` |
| ``defsignature: `later` has no definition in this file — a declaration promises one`` | Une fonction déclarée avec `defsignature` n'est pas définie |
| ``format: ~/nosuch/ calls what (nosuch arg colon at) with arg a `string` would: no such function: nosuch`` | L'appel `(nosuch arg colon at)` écrit à cet endroit ne peut pas être résolu. Définissez une méthode du type de l'argument ou une fonction ([Directives de format, chapitre 5](functions/format.md#5-name)) |
| ``format: ~/brief/ can be handed an element of a list argument, whose type is only `Sexpr` — ...`` | Un `~/name/` est écrit là où il reçoit les éléments d'une liste, par exemple dans `~{`. Le type des éléments ne peut pas déterminer ce qu'il faut appeler |

## 3. Erreurs de lecture

| Message | Signification et correction |
|---|---|
| `unexpected end of input while reading a list` | Il manque une parenthèse fermante. La position indique où la lecture s'est arrêtée (comme la fin du fichier) ; cherchez donc la parenthèse ouvrante |

## 4. Erreurs à l'exécution (panic)

| Message | Signification et correction |
|---|---|
| `panic: divide by zero` | Division par zéro avec des entiers ou des rationnels. La division flottante par zéro ne déclenche pas de panic ; elle donne `inf`/`NaN` |
| `panic: unwrap: called on none` | `unwrap` a été appliqué à `none`. Traitez le cas `none` avec `match` ou `unwrap-or` |
| `panic: Vector: index 5 out of bounds` | Un indice hors limites. Vérifiez la longueur avec `len`, ou utilisez une fonction qui renvoie `none` hors limites (`nth`, `pop`, etc.) |
| `panic: an integer argument does not fit a fixnum` | Un `int` qui ne tient pas sur 63 bits a été passé à un argument qui prend un indice ou un compte |
| `throw: no enclosing (catch 'oops) for this throw` | Un `throw` s'est exécuté sans `catch` englobant de même étiquette |
| `panic: <message>` | Le programme a appelé `(panic "<message>")`. Un `assert` qui échoue donne `assertion failed: ...` |

Un `panic` arrête tout le processus, même s'il survient dans une tâche
([Référence de la syntaxe 12.4](syntax.md#124-interaction-avec-les-autres-fonctionnalités)). Exprimez les échecs
dont vous voulez vous remettre avec `Result`
([chapitre 9 de la référence de la syntaxe](syntax.md#9-principes-de-gestion-des-erreurs)).

## 5. Avertissements

| Message | Signification |
|---|---|
| ``warning: redefining function `f` `` | Une fonction de même nom a été définie de nouveau. La définition la plus récente prend effet. Cela apparaît normalement quand on corrige une définition dans la REPL |
