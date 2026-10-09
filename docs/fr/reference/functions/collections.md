<!-- translated-from: docs/ja/reference/functions/collections.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Chaînes, caractères et collections

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` et `BitVector`.

## 1. Chaînes `string`

Les chaînes sont immuables.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Convertit en majuscules (ASCII uniquement). Comme le `string-upcase` de CL, renvoie une nouvelle chaîne. Les chaînes étant immuables, il n'y a pas de `nstring-upcase` destructif ; celle-ci en tient lieu |
| `downcase` | `(downcase s)` | `string→string` | Convertit en minuscules (ASCII uniquement). Tient lieu de `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Met en majuscule la première lettre de chaque mot et le reste en minuscules (le `string-capitalize` de CL). Un mot est une suite maximale de lettres et de chiffres |
| `length` | `(length s)` | `string→int` | Nombre de caractères |
| `ref` | `(ref s i)` | `(string,int)→char` | Le caractère `i`. Panic hors limites |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | La sous-chaîne `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Concaténation. On peut en donner trois ou plus (identique à `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Comparaison lexicographique |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Strictement inférieur dans l'ordre lexicographique (identique à `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Comparaison d'identité (s'il s'agit du même objet, pas du même contenu) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Compare le contenu (sensible à la casse) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Compare le contenu (insensible à la casse, ASCII uniquement) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Si le contenu diffère (le `string/=` de CL. La forme variadique compare les paires adjacentes) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Ordre insensible à la casse (le `string-lessp` de CL, etc.). Avec un préfixe commun, la plus courte est la plus petite |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Une chaîne de `n` copies de `c` (le `make-string` de CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | La position de la première occurrence de `sub`. **Le `search` de CL a les arguments dans l'autre sens** (`(search pattern sequence)`). La chaîne vide est trouvée en 0. Pour les mots-clés, voir les [arguments mots-clés des séquences](sequences.md#6-arguments-mots-clés) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | La première position où elles diffèrent. `none` seulement si elles sont `equal`. Si l'une est un préfixe de l'autre, la fin de la plus courte. Mots-clés comme ci-dessus |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Supprime les caractères contenus dans `bag` aux deux extrémités / à gauche / à droite (le `string-trim` de CL, etc.). Sans `bag`, les blancs `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Découpe selon `sep`. CL n'a pas d'équivalent. Des séparateurs consécutifs produisent des éléments vides. Panic si `sep` est vide |
| `to-string` | `(to-string x)` | `T→string` | Convertit en chaîne comme le fait `~a`. Implémenté pour `int`/`i32`/`f64`/`bool`/`char`/`string` (le `princ-to-string` de CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Encode en UTF-8 (chaque élément 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Décode. `none` si ce n'est pas de l'UTF-8 valide |

## 2. Caractères `char`

Un `char` est une valeur scalaire Unicode. La conversion de casse et la classification ne traitent que la plage
ASCII.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Convertit en majuscule (ASCII uniquement) |
| `downcase` | `(downcase c)` | `char→char` | Convertit en minuscule (ASCII uniquement) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Comparaison par point de code |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Strictement inférieur par point de code (identique à `<`) |
| `alphap` | `(alphap c)` | `char→bool` | S'il s'agit d'une lettre ASCII |
| `digitp` | `(digitp c)` | `char→bool` | S'il s'agit d'un chiffre ASCII |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Compare les valeurs |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Compare les valeurs sans tenir compte de la casse (le `char-equal` de CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Si les valeurs diffèrent (le `char/=` de CL. **La forme variadique compare les paires adjacentes**, contrairement à CL, qui demande si toutes les paires diffèrent) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Ordre insensible à la casse (le `char-lessp` de CL, etc.) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Majuscule / minuscule / a des distinctions de casse (le `upper-case-p` de CL, etc.) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Une lettre ou un chiffre (même nom qu'en CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | S'il est imprimable. Inclut l'espace, pas le saut de ligne ni la tabulation (le `graphic-char-p` de CL) |
| `standardp` | `(standardp c)` | `char→bool` | S'il fait partie des 96 caractères standard de CL, c'est-à-dire `graphicp` plus le saut de ligne (le `standard-char-p` de CL) |
| `char->int` | `(char->int c)` | `char→int` | La valeur scalaire Unicode (l'inverse est `int->char`/`try-int->char` dans [Nombres](numbers.md#1-entiers-de-largeur-fixe)). Correspond au `char-code`/`char-int` de CL |
| `char->string` | `(char->string c)` | `char→string` | Une chaîne d'un caractère. La fonction `string` de CL couvre ce cas en prenant un désignateur, mais ce langage n'a pas de désignateurs ; la direction figure donc dans le nom |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | Le **poids** du chiffre dans cette base (le `digit-char-p` de CL). `digitp` est une fonction distincte qui renvoie un `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Le caractère du poids `w`. Majuscules à partir de 10 (le `digit-char` de CL ; la base est au plus 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Le nom du caractère. Seuls les caractères nommés que le lecteur sait lire ont un nom (le `char-name` de CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Le caractère correspondant à un nom. Insensible à la casse, accepte aussi les alias du lecteur (`linefeed`/`null`) (le `name-char` de CL) |

Il n'existe pas de constante correspondant à `char-code-limit` (la limite supérieure de `char` est fixée par
Unicode, pas par le langage).

## 3. `Vector<T>`

Un tableau extensible.
Une valeur s'écrit `#(1 2 3)` ([référence de la syntaxe](../syntax.md#1-éléments-lexicaux) ; le type
des éléments vient du contexte ou du premier élément, et chaque évaluation crée un nouveau vecteur).
Elle s'imprime aussi `#(1 2 3)`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Crée un vecteur vide. L'argument de type vient du type attendu ; dans un simple `let`, écrivez donc `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` copies de `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Ajoute à la fin |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Lit l'élément `i`. Panic hors limites |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Modifie l'élément `i`. Panic hors limites. Peut aussi s'écrire `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Nombre d'éléments |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Retire le dernier élément et le renvoie. `None` si vide (contrairement à `get`/`set`, pas de panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Crée un itérateur qui implémente `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Ajoute `x` si aucun élément égal n'existe (le `pushnew` de CL. Il n'a pas besoin de réécrire un emplacement ; c'est donc une méthode plutôt qu'une macro) |

`map`/`filter` et consorts sont des [fonctions de séquence](sequences.md#4-fonctions-de-séquence-sur-iter) : on
passe le vecteur via `iter`, comme dans `(map (iter v) f)`. Les opérations destructives (`nreverse`, `delete`,
etc.) se trouvent dans [Opérations destructives](sequences.md#7-opérations-destructives).

## 4. `HashTable<K,V>`

| Nom | Forme | Type | Description |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Crée une table vide |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Recherche |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Insère ou écrase |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Supprime l'entrée et renvoie l'ancienne valeur, s'il y en a une |
| `count` | `(count h)` | `HashTable<K,V>→int` | Nombre d'entrées |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Supprime tout |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Un instantané des clés |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Un instantané des valeurs |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Un instantané des paires `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Un itérateur qui implémente `Iter`. Les éléments sont des `cons-cell` `(k . v)`. Correspond au `with-hash-table-iterator` de CL ; `doiter`/`map`/`filter` et les autres fonctionnent tels quels dessus |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | Le `maphash` de CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | Le `hash-table-size` de CL. Pour cette table, le nombre d'entrées occupées (égal à `count`) |

**Tout type qui implémente `Hash` peut être une clé**, y compris les types `defstruct`/`defenum`. `get`/`set`/
`remove` portent `(where (Hash K))` ; une table dont le type de clé ne l'implémente pas est donc une **erreur de
type** (`f64` n'a pas de `Hash` à cause de `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; renvoie une valeur positive ou nulle qui tient dans un fixnum
```

Implémenté pour : `int` et les six entiers de largeur fixe, `bool`, `char`, `string` et `symbol` (pas pour les
nombres à virgule flottante). Pour vos propres types, gardez le résultat positif ou nul en le combinant avec
`*sxhash-mask*` (2^30-1) par `logand`. Pour hacher une chaîne, vous pouvez appeler `(sxhash-string s)` (FNV-1a sur
32 bits), qu'utilise l'implémentation pour `string`.

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

Que deux clés soient la même est décidé par **le type de clé lui-même** (`sxhash`, et `equals` de `Eq`, le
super-trait de `Hash`), pas par l'identité des objets. C'est pourquoi, comme ci-dessus, on peut chercher avec une
clé qui est « une autre valeur mais égale ».

Les collisions de `sxhash` sont acceptables (le contrat de `Hash` ne va que dans un sens : des valeurs égales
doivent avoir le même hachage). Les clés en collision sont distinguées par `equals`.

## 5. `Array<T>` (tableaux multidimensionnels)

Un `defstruct` de la bibliothèque standard. Ce n'est pas un type intégré ; tout ce qu'on peut faire avec un
`defstruct` peut donc se faire avec lui.
Une valeur s'écrit `#2A((1 2) (3 4))` ([référence de la syntaxe](../syntax.md#1-éléments-lexicaux)).

| Nom | Forme | Type | Description |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | Le `make-array` de CL. `dims` est copié. `init` est la valeur initiale de chaque cellule (le `:initial-element` de CL ; ce langage n'a pas de « cellule non liée », il est donc obligatoire). `:fill-pointer` seulement pour une dimension |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | Le `aref` / `(setf (aref …))` de CL. Panic si un indice est hors limites |
| `aref` | `(aref a i j …)` | — | L'écriture de CL avec des indices nus. S'expanse en `get`/`set` ci-dessus. `(setf (aref a i j) v)` fonctionne aussi |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | Le `row-major-aref` de CL. Un indice plat |
| `rank` | `(rank a)` | `Array<T>→int` | Le `array-rank` de CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | Le `array-dimension` de CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | Le `array-dimensions` de CL. Renvoie une **copie**, comme CL renvoie une liste neuve |
| `total-size` | `(total-size a)` | `Array<T>→int` | Le `array-total-size` de CL (le nombre de cellules allouées, indépendamment du pointeur de remplissage) |
| `len` | `(len a)` | `Array<T>→int` | Le `length` de CL sur les tableaux. Le pointeur de remplissage s'il y en a un, sinon `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | Le `array-in-bounds-p` de CL. Faux (pas une erreur) même quand le **nombre** d'indices est incorrect |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | Le `array-row-major-index` de CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | Le `adjust-array` de CL. Le rang ne peut pas changer. Les éléments qui restent dans les limites gardent leurs indices, et les nouvelles cellules reçoivent `init`. Contrairement à CL, il ne renvoie pas le tableau (tout tableau de ce langage est ajustable ; il n'y a donc pas de second tableau à renvoyer) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | Le `vector-push-extend` de CL. Panic sans pointeur de remplissage |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | Le `vector-pop` de CL. `none` si vide |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Le pointeur de remplissage (`none` s'il n'y en a pas). Peut s'écrire avec `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Un itérateur dans l'ordre ligne par ligne. S'arrête au pointeur de remplissage s'il y en a un |

- **Les indices sont un `Vector<int>`.** Une méthode ne peut pas déclarer « le même type d'argument répété un
  nombre quelconque de fois à la fin », et le sucre `aref` comble cet écart.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p` **n'existent pas**.
  Le type statique du receveur répond déjà à ces questions.
- `Array::new` est le constructeur dans l'ordre des champs généré par `defstruct` et n'est pas destiné à créer des
  tableaux. Utilisez `Array::make`.
- **Les tableaux s'affichent dans la syntaxe de tableau de CL.** Le rang 1 donne `#(1 2 3)` ; les autres rangs
  donnent `#nA` suivi d'autant de niveaux de parenthèses (`#2A((1 2 3) (4 5 6))`) ; le rang 0 donne `#0A5`.
  L'affichage s'arrête au pointeur de remplissage s'il y en a un. Mettre `*print-array*`
  ([Affichage](printing.md#6-contrôler-la-quantité-imprimée)) à faux affiche seulement la forme, `#<array 2x3>`.
  Seul un tableau dont les éléments sont un `defstruct` sans `print-object` s'affiche sous la forme intégrée
  `#<array<...> ...>` (ce n'est pas une erreur).

## 6. `BitVector` (vecteurs de bits)

Une suite de bits de longueur fixe. Un `defstruct` de la bibliothèque standard.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Longueur `n`, tous les bits à 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Panic hors limites |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | Les écritures de CL. `(setf (bit v i) b)` fonctionne aussi. Le `sbit` de CL ne diffère de `bit` qu'en exigeant un vecteur de bits simple, mais ce langage n'a qu'une sorte de vecteur de bits |
| `len` | `(len v)` | `BitVector→int` | Nombre de bits |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Renvoient un nouveau vecteur de bits. Panic si les longueurs diffèrent. Il n'y a pas de troisième argument comme en CL (la destination du résultat) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Complément |

Il n'y a pas de `bit-vector-p` (le type statique y répond).
