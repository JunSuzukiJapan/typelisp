<!-- translated-from: docs/ja/reference/functions/sequences.md @ eae672c1a271f0b6947f024e81dee8338b2f5ff4 -->
# Paires, S-expressions et séquences

La paire générique `cons-cell`, les données S-expression `Sexpr`, les symboles, les fonctions de séquence écrites
au-dessus de `Iter` et les fonctions d'ordre supérieur.

## 1. Paires `cons-cell<A,B>`

`cons`/`car`/`cdr` sont le constructeur et les accesseurs de champs du **type de paire générique `cons-cell<A,B>`**
(un `defstruct` de la bibliothèque standard). Les champs se lisent soit comme `variable::car`/`variable::cdr` (la
syntaxe d'accès de `defstruct` de la
[Référence de la syntaxe](../syntax.md#36-defstruct--structures-types-définis-par-lutilisateur)), soit comme
`(car variable)`/`(cdr variable)`. Pour les modifier, utilisez `(setf variable::car v)`/`(setf variable::cdr v)`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Crée une paire |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Le premier élément |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Le reste |

`cons-cell` tient aussi lieu de syntaxe de tuple. Les fonctions de CL qui renvoient plusieurs valeurs (le quotient et
le reste de `floor`, la valeur et la position de `read-from-string`, etc.) renvoient une `cons-cell` dans ce langage.

## 2. Données S-expression `Sexpr`

Le type de données `Sexpr` renvoyé par `read` a 19 variantes :
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` et `array` sont des données écrites `#(..)` et `#nA(..)` ([référence de la
syntaxe](../syntax.md#1-éléments-lexicaux)), qui contiennent respectivement un
`Vector<Option<Sexpr>>` et un `Array<Option<Sexpr>>` : `len`, `get`, etc. s'appliquent directement
au `v` que lie `(vector v)`.
`tuple` est une donnée écrite avec `#{..}`, et le `v` que lie `(tuple v)` est un nouveau
`Vector<Option<Sexpr>>` des éléments (pour recevoir avec un seul type un tuple de n'importe quelle
longueur).
Les cellules S-expression ne se manipulent pas avec les `cons`/`car`/`cdr` généraux du chapitre 1 mais avec les
fonctions `sexpr-*`. Elles servent surtout dans les corps de `defmacro` pour construire et décomposer des formes.

**Le type des données S-expression est `Option<Sexpr>`.** La liste vide n'est pas une variante de `Sexpr` mais le
`none` d'`Option`, et `Sexpr` lui-même signifie « une S-expression non vide ». Les fonctions `sexpr-*` prennent et
renvoient donc des `Option<Sexpr>`.

- `()` est la liste vide là où une `Option<Sexpr>` est attendue (on peut aussi écrire `(Option::none)`)
- `Sexpr` s'élargit implicitement là où une `Option<Sexpr>` est attendue (sans conversion à l'exécution). Le sens
  inverse, utiliser une `Option<Sexpr>` comme `Sexpr`, affirme « ce n'est pas la liste vide » et doit donc être
  indiqué explicitement avec `match` ou `unwrap`
- Dans `match`, les 19 variantes de `Sexpr` et `none` peuvent s'écrire **à plat dans la même liste de branches**
  ([Référence de la syntaxe](../syntax.md#43-match--filtrage-par-motifs))

| Nom | Forme | Type | Description |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Crée une cellule `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Le premier élément. **La liste vide pour la liste vide** (comme en CL). Panic sur un atome qui n'est pas un `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Le reste. **La liste vide pour la liste vide** (comme en CL). Panic sur un atome qui n'est pas un `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Si c'est un `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Si c'est la liste vide |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Si ce n'est pas un `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Si c'est un `Sym` (symbole) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | Le contenu de la variante `int` (fixnum ou bignum). Panic sur un autre type |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | Le contenu de la variante de cette largeur. Panic sur un autre type |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Le contenu des variantes flottantes. Panic sur un autre type |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Le contenu d'un `Char`. Panic sur un autre type |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Le contenu d'un `Bool`. Panic sur un autre type |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Le contenu d'un `Str`. Panic sur un autre type |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Le nom d'un `Sym`. Panic sur un autre type |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Comparaison d'identité (`Cons`/`Str` comparent l'identité des objets, le reste compare les valeurs) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Égalité structurelle (`Cons` récursivement, `Str` par contenu) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Comme `equal`, plus la comparaison insensible à la casse et la comparaison de nombres de types différents |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Concatène deux listes `Sexpr` (de façon non destructive). `,@` s'expanse en cela |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Une nouvelle liste `Sexpr` avec `f` appliquée à chaque élément d'une liste `Sexpr` (le `map` du chapitre 4 est pour `Iter` et ne peut pas parcourir une liste `Sexpr`) |

Il y a neuf accesseurs numériques, un par type, parce qu'un `Sexpr` est « le seul endroit où le type d'une valeur
n'est écrit nulle part ailleurs ». Un `u8` placé dans un `Sexpr` y entre comme variante `u8` et n'en sort qu'avec
`(sexpr-u8 s)`. Le passer à `(sexpr-int s)` déclenche un panic ; la réponse n'est jamais élargie silencieusement. Les
entiers des données lues (`'(1 2 3)`, arguments de macro) sont de la variante `int` et se lisent avec
`(sexpr-int s)`.

Les listes `Sexpr` n'ont pas d'opérations destructives comme `rplaca`/`nconc`. Une cellule `Sexpr` ne peut pas être
modifiée après sa création.

## 3. Symboles

`symbol` est le type des symboles eux-mêmes. Il se convertit implicitement là où un `Sexpr` est exigé, mais pas
automatiquement dans l'autre sens.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Extrait le nom du symbole |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Crée un symbole à partir d'une chaîne (l'interne) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Si c'est un mot-clé (`:name`). Le deux-points fait partie du nom ; le test regarde donc le premier caractère ([Référence de la syntaxe](../syntax.md#1-éléments-lexicaux)) |

Pour `gensym`, voir [Macros](system.md#8-macros).

## 4. Fonctions de séquence sur `Iter`

Les fonctions de séquence sont des **fonctions génériques sur le trait `Iter`**. À partir d'une collection, obtenez
un itérateur avec `(iter coll)` et passez-le (`Vector<T>` / `HashTable<K,V>` / `Array<T>` le permettent ; une liste
`Sexpr` n'implémente pas `Iter`, ces fonctions ne s'y appliquent donc pas). **Une collection résultat est renvoyée
sous forme de nouveau `Vector`.** `Iter<A>` dans les tableaux signifie « n'importe quelle implémentation de `Iter`
dont l'`Item` est `A` ». Pour parcourir de nouveau le `Vector` renvoyé, passez `(iter result)`.

Fonctions qui prennent un prédicat (correspondant à la famille `-if` de CL) :

| Nom | Forme | Type | Description |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Application |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Seulement les éléments qui satisfont le prédicat |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Retire les éléments qui satisfont le prédicat |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Le premier élément qui satisfait le prédicat |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | La première position qui satisfait le prédicat |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Combien satisfont le prédicat |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Si chaque élément satisfait le prédicat |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Si un élément satisfait le prédicat (correspond au `some` de CL ; un nom qui n'entre pas en conflit avec le constructeur `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Pli à gauche |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Pli à droite |
| `collect` | `(collect it)` | `Iter<A>→Vector<A>` | Rassemble tous les éléments restants. Sert à transformer en `Vector` le résultat d'une fonction `lazy` ci-dessous |

Indexation, longueur et découpage :

| Nom | Forme | Type | Description |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Nombre d'éléments |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Concatène des itérateurs. On peut en donner trois ou plus |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | Le `concatenate` de CL. Le type du résultat s'écrit comme **un symbole littéral quoté** (CL utilise un spécificateur de type à l'exécution). `'vector` en prend un ou plus, `'string` zéro ou plus (`""` pour zéro). Les listes `Sexpr` ne sont pas couvertes (utilisez `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Inversion (non destructive) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | L'élément `n` (`None` hors limites) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` avec les arguments dans l'autre sens |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | Les `n` premiers éléments |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` est ramené à la longueur) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | Le dernier **élément** (pas « la dernière cellule » comme en CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Tous sauf le dernier élément |

Fonctions qui exigent une borne `Eq` / `Ord` (elles comparent via un trait au lieu d'un prédicat ;
[Traits standard](traits.md#2-eq--ord-comparaison)) :

| Nom | Forme | Type | Description |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | S'il existe un élément égal à `x` (contrairement à CL, un `bool`, pas le reste de la liste) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | Le premier élément égal à `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | La première position égale à `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Combien d'éléments sont égaux à `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | Le `(sort sequence predicate)` de CL. Un tri stable et non destructif. `cmp` vaut `true` quand « le premier argument vient strictement avant le second » |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | La première paire dont le `car` est égal à `k`. On extrait la valeur avec `(cdr p)` |

Celles-ci et beaucoup de fonctions du chapitre 5 prennent aussi les arguments mots-clés de CL `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (chapitre 6).

### Itérateurs paresseux (le module `lazy`)

Les fonctions du module `lazy` ne construisent pas de `Vector` : **elles renvoient un itérateur**.
Un élément n'est calculé que lorsque le suivant est demandé ; un itérateur sans fin (`iterate`,
`repeat`) est donc utilisable tant qu'un `take` ou un `take-while` en aval l'arrête. Chaque résultat
implémente `Iter` : les fonctions `lazy` s'imbriquent, et les fonctions des tableaux ci-dessus les
acceptent telles quelles. `collect` en fait un `Vector`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `lazy::map` | `(lazy::map it f)` | `(Iter<A>,(fn (A) U))→Iter<U>` | Applique `f` à chaque élément |
| `lazy::filter` | `(lazy::filter it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | Seulement les éléments qui satisfont la condition |
| `lazy::take` | `(lazy::take it n)` | `(Iter<A>,int)→Iter<A>` | Les `n` premiers |
| `lazy::take-while` | `(lazy::take-while it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | Jusqu'au premier élément qui ne satisfait pas la condition, exclu |
| `lazy::skip` | `(lazy::skip it n)` | `(Iter<A>,int)→Iter<A>` | Saute les `n` premiers |
| `lazy::enumerate` | `(lazy::enumerate it)` | `Iter<A>→Iter<#{int A}>` | Paires de la position, comptée à partir de 0, et de l'élément |
| `lazy::zip` | `(lazy::zip a b)` | `(Iter<A>,Iter<B>)→Iter<#{A B}>` | Paires prenant un élément de chaque côté. S'arrête avec le plus court |
| `lazy::chain` | `(lazy::chain a b)` | `(Iter<A>,Iter<A>)→Iter<A>` | Les éléments de `a`, puis ceux de `b` |
| `lazy::flat-map` | `(lazy::flat-map it f)` | `(Iter<A>,(fn (A) Iter<B>))→Iter<B>` | Fait de chaque élément un itérateur avec `f` et les enchaîne dans l'ordre |
| `lazy::iterate` | `(lazy::iterate x f)` | `(A,(fn (A) A))→Iter<A>` | `x`, `(f x)`, `(f (f x))`, ... sans fin |
| `lazy::repeat` | `(lazy::repeat x)` | `A→Iter<A>` | Répète `x` sans fin |

`Iter<U>` et consorts dans le tableau sont en réalité des types struct nommés d'après la fonction
suivie de `-iter` (pour `lazy::map`, `lazy::map-iter<I,A,U>`, où `I` est le type de l'itérateur
source). On n'écrit le type que là où rien d'autre ne le fixe, comme le type de retour de la lambda
passée à `lazy::flat-map`.

```lisp
(collect (lazy::take (lazy::filter (lazy::iterate 1 (lambda ((n int)) int (+ n 1)))
                                   (lambda ((n int)) bool (= 0 (mod n 3))))
                     4))                                  ; => #(3 6 9 12)

(doiter (#{i s} (lazy::enumerate (iter (the Vector<string> #("a" "b")))))
  (println "~a: ~a" i s))                                 ; 0: a et 1: b

(-> (lazy::iterate 1 (lambda ((n int)) int (* n 2)))
    (lazy::take-while (lambda ((n int)) bool (< n 100)))
    collect)                                              ; => #(1 2 4 8 16 32 64)
```

`->` est la macro qui passe une valeur comme premier argument de chaque forme suivante, dans l'ordre
([Option et Result](option-result.md)).

## 5. Le reste des fonctions de séquence de CL

Toutes sont des fonctions génériques sur `Iter`, comme au chapitre 4. Les collections résultats sont renvoyées sous
forme de nouveaux `Vector`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | Les indices nommés de CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Tous sauf le premier (un nouveau `Vector`, pas une queue partagée) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Matérialise un itérateur en `Vector` (le `copy-seq`/`copy-list` de CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` inversé, suivi de `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` copies de `x` (le `make-list`/`make-sequence` de CL). Comme pour `Vector::new`, l'argument de type vient du type attendu ; un simple `let` a donc besoin de `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Comme `member`, un **`bool`** (un itérateur n'a pas de queue à renvoyer) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Les négations de `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Mêmes types que les versions positives | Versions avec le prédicat nié |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Retire par valeur |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Retire les doublons. Comme en CL, **la dernière occurrence est conservée** (`:from-end true` garde la première) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Remplace par valeur / prédicat |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | sur `Iter<cons-cell<K,V>>` | Les versions à prédicat et côté valeur de `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Ajoute une paire en tête |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Apparie deux séquences. S'arrête à la plus courte |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | Le `mapcar` de CL sur plusieurs séquences. S'arrête à la plus courte |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Application pour les effets de bord |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Applique et concatène |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Applique sur les **queues** successives |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Applique sur les queues pour les effets de bord (le pendant `maplist` de `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Applique sur les queues et concatène (le pendant `maplist` de `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | La position de la première occurrence de `sub`. Si le receveur est une `string`, la méthode de `string` est choisie ([Chaînes](collections.md#1-chaînes-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | La première position où elles diffèrent. `none` si elles sont égales |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Fusion. CL exige des entrées triées ; ceci trie la concaténation |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Ajoute `x` **en tête** s'il n'y est pas |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Opérations ensemblistes. CL ne spécifie pas l'ordre ; ici il est stable, **dans l'ordre de première apparition** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Inclusion |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | S'il s'agit d'un suffixe / la partie avant le suffixe. CL pose la question de la **structure partagée**, mais il n'y a pas de structure à partager ; on demande donc un suffixe **en tant que valeurs** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Égalité élément par élément. `Vector<T>` lui-même n'implémente pas `Eq` |
| `caar`…`cddddr` | `(cadr p)` | sur des paires imbriquées | Les 28 fonctions de CL. Elles parcourent **des paires, pas des listes** : `cadr` prend une `cons-cell<A,cons-cell<B,C>>` |

Ce que CL a et que ce langage n'a pas : `list*` (il n'y a pas de notion de liste impropre dont on remplace la
queue), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (aucun type ne peut décrire le parcours d'un arbre
hétérogène de profondeur quelconque ; pour un arbre de `Sexpr`, `equal` correspond à `tree-equal`), la famille des
listes de propriétés `getf`/`get-properties`/`symbol-plist`/`remprop` (il n'y a pas de représentation en liste non
typée alternant clés et valeurs ; `assoc` (listes d'association) ou `HashTable` remplissent le même rôle), et les
fonctions de conversion entre `Vector<T>` et listes `Sexpr` (les éléments d'une liste `Sexpr` peuvent avoir chacun
un type différent ; ils ne peuvent donc pas s'écrire avec un seul type d'élément `T`).

## 6. Arguments mots-clés

Les fonctions des chapitres 4 et 5 prennent les mots-clés de séquence de CL `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count`. Tous sont **facultatifs**.

| Mot-clé | Type | Signification |
|---|---|---|
| `:key` | `(fn (A) A)` | Une projection appliquée à chaque élément avant comparaison ou test |
| `:test` | `(fn (A A) bool)` | Un test d'égalité utilisé à la place de l'`equals` de la borne `Eq`. Le premier argument est **l'objet recherché**, le second l'élément (après `:key`), dans le même ordre que CL |
| `:test-not` | `(fn (A A) bool)` | La négation de `:test` |
| `:start` `:end` | `int` | La fenêtre `[start, end)` à parcourir. Les indices sont relatifs à la séquence entière |
| `:from-end` | `bool` | Une recherche répond avec la **dernière** correspondance. Combiné à `:count`, les éléments concernés sont pris à partir de la fin |
| `:count` | `int` | Le nombre maximal d'éléments que concernent les familles `remove` / `substitute` |

Quelle fonction prend lesquels suit CL :

| Fonction | Mots-clés acceptés |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Tous ceux ci-dessus (y compris `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (le `:key` de `assoc` s'applique au `car`, celui de `rassoc` au `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; n'en retire qu'un, depuis la fin
(position 3 (iter v) :start 1)                          ; l'indice est relatif à la séquence entière
```

**Différences avec CL** :

1. **La projection de `:key` reste dans le type des éléments** (`(fn (A) A)`). Elle ne peut pas projeter vers un
   autre type comme en CL : une variable de type supplémentaire ne pourrait pas être déterminée quand l'argument est
   omis. Là où une projection vers un autre type est nécessaire, passez plutôt un lambda à la famille `-if`
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **Dans les recherches par objet, `:key` ne s'applique qu'aux éléments** (pas à l'objet recherché). C'est la même
   règle que pour les `find`/`position`/`count`/`member`/`remove`/`substitute` de CL. Dans les opérations
   ensemblistes, les deux côtés sont des éléments ; il s'applique donc aux deux.
3. **Seuls les mots-clés de `search` sont nommés plutôt que numérotés.** En CL, `:start1`/`:end1` concernent le
   **motif** et `:start2`/`:end2` la séquence parcourue. Dans ce langage, le receveur vient en premier ; les mêmes
   numéros signifieraient donc l'inverse, et silencieusement. `:start`/`:end` concernent le receveur et
   `:sub-start`/`:sub-end` le motif ; un `:start1` distrait donne donc une erreur « mot-clé inconnu ». `mismatch` et
   `replace` ont le même ordre d'arguments que CL et gardent donc les numéros de CL.

## 7. Opérations destructives

Méthodes de `Vector<T>`. **Elles modifient le receveur et renvoient le receveur lui-même** ; `(nreverse v)` s'écrit
donc comme `reverse`, et `v` lui-même est aussi inversé.

| Nom | Forme | Description |
|---|---|---|
| `nreverse` | `(nreverse v)` | Inverse sur place |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Versions sur place de `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Versions sur place de la famille `substitute` |
| `nbutlast` | `(nbutlast v)` | Supprime le dernier élément |
| `fill` | `(fill v x)` | Met chaque élément à `x`. La longueur ne change pas |
| `replace` | `(replace v src)` | Écrase depuis le début avec les éléments de `src`. `(min (len v) (len src))` éléments |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Même nombre que ci-dessus |
| `nconc` | `(nconc v w)` | Ajoute les éléments de `w` à `v`. Contrairement à CL, **il ne réécrit pas de structure partagée** (`w` n'est pas affecté) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Remplace le contenu de `v` par `src` (la longueur change aussi) |
| `rplaca` `rplacd` | `(rplaca p x)` | Réécrit le `car`/`cdr` d'une `cons-cell` et renvoie la cellule elle-même |

Mots-clés acceptés :

| Version destructive | Mots-clés acceptés |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (le receveur est le `sequence-1` de CL) |

`vector-push-extend`/`vector-pop` sont simplement les `push`/`pop` de `Vector<T>`. Un `Vector<T>` grandit toujours ;
rien ne correspond donc à la distinction de CL entre « un vecteur avec pointeur de remplissage » et « un vecteur
simple ».

## 8. Fonctions d'ordre supérieur

| Nom | Forme | Type | Description |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Renvoie son argument |
| `const` | `(const x y)` | `(A,B)→A` | Renvoie le premier argument |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Composition de fonctions `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Échange les arguments d'une fonction à deux arguments |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Négation d'un prédicat |

Il n'y a pas de `constantly` de CL (le type de l'argument ignoré n'apparaîtrait que dans le type de retour et ne
pourrait pas être déterminé). Écrivez `(lambda ((x T)) A v)`.
