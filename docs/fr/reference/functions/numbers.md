<!-- translated-from: docs/ja/reference/functions/numbers.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Nombres

Les opérations sur les entiers, les nombres à virgule flottante, les rationnels, les nombres complexes et les
booléens, et d'autres fonctions liées aux nombres. Pour lire les formes d'appel, voir
[Fonctions intégrées](README.md).

## 1. Entiers de largeur fixe

Il y a sept types entiers : **`int`** (l'`integer` de CL : précision arbitraire, et type par défaut des littéraux
entiers sans annotation ; chapitre 3) et les types de largeur fixe `i8` `i16` `i32` `u8` `u16` `u32`. Le type pour
lequel une opération est résolue est décidé par le type du premier argument (ils sont indépendants les uns des
autres, sans conversions implicites). **Il n'existe pas de type entier sur 64 bits.** Une valeur à l'exécution est
un mot dont les bits de poids faible sont une étiquette ; il ne reste donc que 63 bits pour un entier immédiat, et
un type prétendant faire 64 bits devrait perdre le bit de poids fort quelque part. `int` devient un bignum au-delà
de ces 63 bits ; si la largeur importe peu, utilisez donc `int`. Le tableau ci-dessous concerne les six types de
largeur fixe (le tableau pour `int` se trouve au chapitre 3).

| Nom | Forme | Type | Description |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | Les quatre opérations. `/` tronque vers zéro et déclenche un panic en cas de division par zéro |
| `mod` | `(mod a b)` | `(T,T)→T` | Reste (le `mod` de CL, **division par défaut** : le signe suit le diviseur. `(mod -7 3)`→`2`). Panic en cas de division par zéro |
| `rem` | `(rem a b)` | `(T,T)→T` | Reste (le `rem` de CL, **division tronquée** : le signe suit le dividende. `(rem -7 3)`→`-1`). Panic en cas de division par zéro |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Correspondent aux `floor`/`ceiling`/`round`/`truncate` à deux arguments de CL (`(floor 7 2)`→quotient 3, reste 1). Au lieu de valeurs multiples, ils renvoient le quotient et le reste dans une `cons-cell` (`car`=quotient, `cdr`=reste). `round-div` arrondit les égalités au pair, comme CL |
| `abs` | `(abs x)` | `T→T` | Valeur absolue |
| `signum` | `(signum x)` | `T→T` | Signe (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Plus grand commun diviseur |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Plus petit commun multiple (0 si l'un vaut 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | Le plus grand / le plus petit (trois arguments ou plus sont développés par le sucre variadique du chapitre 8) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Comparaison |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Tous identiques à `=` (aucune différence pour des nombres de même type) |
| `int->float` | `(int->float x)` | `T→f64` | Conversion élargissante en `f64` |
| `int->int` | `(int->int x)` | `T→int` | Conversion élargissante en `int` (toujours exacte). Ce que fait `(as int x)` |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Conversion élargissante en `ratio` (toujours exacte) |
| `int->char` | `(int->char x)` | `T→char` | Interprète la valeur comme une valeur scalaire Unicode. Panic sur une valeur invalide |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Une version de `int->char` qui renvoie `None` en cas d'échec |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Conversion de largeur. Les valeurs qui ne tiennent pas sont tronquées (comme le `as` de Rust) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | La même conversion sous forme de question. `None` si la valeur ne tient pas dans cette largeur |

Ces conversions sont aussi ce que font les formes spéciales `(as Type x)`/`(try-as Type x)`
([Référence de la syntaxe](../syntax.md#7-autres-formes-spéciales)). Les opérations sur les bits
(`logand`/`ash`/`ldb`, etc.) et les prédicats (`zerop`/`evenp`, etc.) ont la même forme pour tous les types ; elles
sont donc regroupées aux chapitres 11 et 9.

`i8` `i16` `u8` `u16` `u32` ont exactement le tableau de ce chapitre, et `f32` a exactement le tableau de `f64` du
chapitre 4.

**Un nom de type désigne sa largeur et son signe, rien de plus.** `i32` signifie « traiter 32 bits comme signés » et
`u32` « traiter 32 bits comme non signés ». `(+ (the u8 200) (the u8 100))` vaut `44`, `(+ 2147483647 1)` (en
`i32`) vaut `-2147483648`, et `(lognot (the u32 0))` vaut `4294967295`. Il en va de même pour `f32` : un vrai
binary32. `(/ (the f32 1.0) (the f32 3.0))` s'affiche `0.33333334`, une valeur différente du résultat `f64`
`0.3333333333333333`.

Le catalogue CL dérivé (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` et les prédicats du chapitre 9) existe pour
`int`/`i32`/`f64`/`ratio`. Si vous en avez besoin pour une autre largeur, passez par `(as int x)` / `(as i32 x)`
(les conversions de largeur existent pour toutes les paires).

## 2. Mots bruts à la frontière C (`ptr` / `c-long` / `c-ulong`)

Trois types servant uniquement à passer des valeurs à des fonctions C déclarées avec
[`defffi`](../syntax.md#33-defffi--déclarer-des-fonctions-c-ffi) et à en recevoir. `ptr` est un pointeur opaque,
et `c-long` / `c-ulong` sont les `long` / `unsigned long` de C. Pour en faire une valeur, il faut être à l'intérieur
de `(unsafe ...)`.

**Il n'y a pas d'arithmétique.** Rien du tableau du chapitre 1 ne s'applique : ni `(+ p 1)` ni `(< n m)` ne
peuvent s'écrire. Ce sont des mots à remettre à C, pas des types avec lesquels calculer ; pour calculer, passez à un
type doté d'une largeur. `c-long` / `c-ulong` n'ont que des conversions :

| Nom | Forme | Type | Description |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | Les mêmes conversions de largeur qu'au chapitre 1. Les valeurs qui ne tiennent pas sont tronquées |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | La même conversion sous forme de question |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | Le chemin d'entrée, depuis l'autre mot brut et depuis les types entiers du chapitre 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Idem |
| `int->int` | `(int->int x)` | `T→int` | **Toujours exacte**. La façon honnête de lire un `size_t` qui ne tient pas dans un `i32` |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` font exactement cela, et les conversions existent
pour toutes les paires avec les types entiers du chapitre 1. `ptr` n'a même pas ce tableau : aucun moyen n'est
fourni pour lire un pointeur comme un nombre. C'est une valeur qu'on ne fait que passer, recevoir et transmettre à
une autre fonction C.

**Ils ne peuvent pas non plus être affichés.** `(println "~a" x)` n'accepte pas de mot brut (il n'a pas de
représentation `Sexpr`) ; passez d'abord à un type doté d'une largeur, comme dans `(println "~a" (as int n))`.

« Il n'existe pas de type entier sur 64 bits », au début du chapitre 1, vaut aussi pour ces trois-là. Cela tient
**parce qu'ils ne peuvent pas être stockés** : ils ne peuvent être ni un champ de `defstruct`, ni un `defvar`, ni à
l'intérieur d'un argument de type ou d'un `Sexpr` ; ce sont donc des mots qui ne font que traverser une fonction
comme arguments, valeurs de retour et variables locales. Pour les détails, voir la
[Référence de la syntaxe](../syntax.md#ptr--c-long--c-ulong--mots-machine-bruts).

## 3. Entiers en précision arbitraire `int`

L'`integer` de CL, et l'**entier** de ce langage : les littéraux entiers sans annotation ont ce type, et les
fonctions intégrées qui renvoient un nombre, comme `length` et `char->int`, renvoient ce type. Une valeur est
conservée comme valeur immédiate sur 63 bits (fixnum) tant qu'elle tient, est promue automatiquement en bignum
quand le résultat d'une opération ne tient plus, et redevient une valeur immédiate quand elle tient de nouveau.
`eq` est toujours l'identité de valeur dans la plage des fixnums, et `eql`/`=` l'identité numérique sur toute la
plage. C'est un type différent des types entiers de largeur fixe (chapitre 1), sans conversion implicite :
`(as int x)` est l'élargissement exact depuis une largeur fixe, et `(as i32 n)` / `(try-as i32 n)` sont la
troncature / la vérification depuis `int` (le même sens que `int->W` / `try-int->W` au chapitre 1).

La variante entière de `Sexpr` est aussi simplement `int` (`(int n)` accepte fixnums et bignums).

Les fonctions intégrées qui prennent un indice ou un compte (`substring`, le `get` de `Vector`, la distance de
décalage de `ash`, etc.) acceptent `int`, mais passer une valeur qui ne tient pas dans un fixnum est une erreur
d'exécution (« an integer argument does not fit a fixnum »).

| Nom | Forme | Type | Description |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Ne débordent jamais (ils promeuvent) |
| `/` | `(/ a b)` | `(int,int)→int` | Tronque vers zéro. Panic en cas de division par zéro |
| `mod` | `(mod a b)` | `(int,int)→int` | Reste de la division par défaut (le signe suit le diviseur) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Tous `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Comme au chapitre 11 (complément à deux avec une infinité de bits) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Comme au chapitre 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Troncature / vérification. `W` est l'une des six largeurs ou `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Identité (du côté des largeurs fixes et des mots C, `int->int` élargit ; chapitre 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | Même forme qu'au chapitre 1. `expt` n'accepte que des exposants positifs ou nuls |

## 4. Nombres à virgule flottante (`f64` / `f32`)

`f32` a le même tableau.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE 754. La division par zéro ne déclenche pas de panic ; elle donne `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Reste de la division par défaut (comme en CL ; le signe suit le diviseur. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Reste de la division tronquée (comme en CL ; le signe suit le dividende. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Comparaison |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Tous identiques à `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Puissance |
| `abs` | `(abs x)` | `f64→f64` | Valeur absolue |
| `signum` | `(signum x)` | `f64→f64` | Signe (`1.0`/`-1.0` ; `±0.0`/`NaN` sont renvoyés tels quels. Comme en CL, contrairement au `signum` de Rust) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | Le plus grand / le plus petit (trois arguments ou plus sont développés par le sucre variadique du chapitre 8) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Opérations unaires |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Fonctions transcendantes. `log` est le logarithme naturel |
| `log` (deux arguments) | `(log x base)` | `(f64,f64)→f64` | Logarithme dans une base donnée. Développé en `(/ (log x) (log base))` (chapitre 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Correspondent aux versions à deux arguments de CL (`(floor 7.0 2.0)`→quotient 3, reste 1). Même conception que les fonctions de même nom au chapitre 1 (`car`=quotient, `cdr`=reste) |
| `float->int` | `(float->int x)` | `f64→int` | Convertit en `int` en tronquant vers zéro (le `truncate` de CL ; exact pour les valeurs finies de toute taille). Panic sur l'infini et NaN. Pour une largeur fixe, utilisez `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Convertit en `ratio` comme rationnel binaire exact (le `rational` de CL) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Convertit entre largeurs flottantes. `float->f32` arrondit au plus proche, `float->f64` est toujours exact. Ce que fait `(as f32 x)` |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | La même conversion sous forme de question. `none` si l'arrondi change la valeur (l'élargissement en `f64` est toujours `some`). Ce que fait `(try-as f32 x)` |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | Les fonctions de même nom de CL. Alias de `floor`/`ceiling`/`round`/`truncate` ci-dessus : en CL, les versions sans préfixe renvoient des entiers ; celles préfixées par `f` correspondent donc au comportement de ce langage |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | Respectivement 2 / 53 / 53 (seule la précision de `0.0` vaut 0). `f64` est toujours un binary64 IEEE 754 ; ce sont donc des constantes |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` ou `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | La mantisse (dans `[1/2,1)`, sans signe) et l'exposant. CL renvoie trois valeurs, mais il n'y a pas de valeurs multiples ; le signe est donc laissé à `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | La même décomposition avec une mantisse entière exacte de 53 bits. `mantissa * 2^exponent` vaut exactement la valeur d'origine |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **Le rationnel le plus simple qui se relit comme ce flottant** (`(rationalize 0.1)` vaut `1/10`). Pour la valeur binaire exacte, utilisez `float->ratio` |

**Différence avec CL : la façon dont `round` arrondit.** `round` (et donc `fround`/`round-div`) arrondit **en
s'éloignant de zéro** (`(round 2.5)` = `3.0`). CL arrondit **au pair**, ce qui donne `2`.

## 5. Rationnels `ratio`

Des rationnels en précision arbitraire compatibles avec CL. Ils sont toujours conservés sous forme irréductible
avec un dénominateur positif et alloués sur le tas. Il n'y a pas de conversion implicite avec les types entiers ou
`f64` (utilisez une méthode de conversion explicite ou `as`/`try-as`). Pour la syntaxe des littéraux de fractions,
voir la [Référence de la syntaxe](../syntax.md#1-éléments-lexicaux).

| Nom | Forme | Type | Description |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | Les quatre opérations (résultats toujours irréductibles). `/` déclenche un panic en cas de division par zéro |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Reste de la division par défaut (comme en CL ; le signe suit le diviseur) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Reste de la division tronquée (comme en CL ; le signe suit le dividende) |
| `abs` | `(abs x)` | `ratio→ratio` | Valeur absolue |
| `signum` | `(signum x)` | `ratio→ratio` | Signe (renvoie `1`/`-1`/`0` sous forme de `ratio`) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Puissance. L'exposant doit être un `ratio` à valeur entière (sinon panic). Un exposant négatif donne l'inverse |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | Le plus grand / le plus petit |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` n'a pas d'opérations sur les bits (en CL, elles ne concernent que les entiers) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Comparaison |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Tous identiques à `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Numérateur sous forme irréductible (même nom qu'en CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Dénominateur sous forme irréductible (toujours positif) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Partie entière (tronquée vers zéro) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Convertit en `f64` |

Les chemins d'entrée depuis les entiers de largeur fixe et `f64` sont `int->int`/`int->ratio` (chapitre 1) et
`float->int`/`float->ratio` (chapitre 4). `int`/`ratio` sont des types distincts, indépendants de `i32` et des
autres, et l'arithmétique mixte exige des conversions explicites.

## 6. Nombres complexes `complex`

Une structure (`defstruct`) de la bibliothèque standard.

**Deux différences avec CL** (toutes deux découlent du typage statique) :

1. **Les composantes sont toujours `f64`.** Un complexe CL peut aussi contenir des rationnels, et `(complex 1 2)`
   et `(complex 1.0 2.0)` sont de types différents. Un type statique doit en choisir un, et les fonctions
   transcendantes renvoient la sorte flottante.
2. **`(sqrt -1.0)` est le `sqrt` réel (NaN).** En CL, `sqrt` peut renvoyer un complexe à partir d'un réel, mais le
   `sqrt` de `f64` doit renvoyer un `f64`. Un résultat complexe vient d'un argument complexe :
   `(sqrt (complex -1.0 0.0))` vaut `i`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Construction. Les composantes se lisent directement comme `z::re`/`z::im` |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Partie réelle et partie imaginaire. **Fonctionnent aussi sur les réels** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), comme en CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Conjugué (fonctionne aussi sur les réels) |
| `phase` | `(phase z)` | `complex→f64` | Argument dans (-pi,pi] (fonctionne aussi sur les réels) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Valeur absolue. **Le seul `abs` qui ne renvoie pas le type du receveur** (comme en CL, la valeur absolue d'un complexe est réelle) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Arithmétique complexe |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Égalité composante par composante. `Eq` est aussi implémenté (pas d'`Ord` : les complexes n'ont pas d'ordre, et le `<` de CL les rejette aussi) |
| `zerop` | `(zerop z)` | `complex→bool` | Si les deux composantes valent 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` donnent les valeurs principales |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | L'angle du vecteur `(x,y)`. **Le `(atan y x)` à deux arguments de CL est du sucre pour celui-ci** (il bifurque selon le nombre d'arguments, comme le `log` à deux arguments) |

Il implémente `print-object` ; `~a`/`~s` l'affichent donc `#C(re im)`, comme CL (le lecteur de ce langage n'a pas
de syntaxe `#C` pour le relire).

## 7. Booléens

| Nom | Forme | Type | Description |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Négation |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Tous comparent les valeurs |

`and`/`or` exigent une évaluation en court-circuit ; ce sont donc des formes spéciales
([Référence de la syntaxe](../syntax.md#4-liaison-et-conditionnelles)).

## 8. Auxiliaires numériques et sucre d'appel

`abs`/`signum` (tous les types numériques), `gcd`/`lcm` (types entiers uniquement), `rem` (tous les types réels, y
compris `f64`) et `expt` (`int`/`f64`/`ratio`) sont définis comme méthodes de chaque type numérique (résolues selon
le type du receveur : `(abs x)` est la méthode du type de `x`). Les détails pour chaque type se trouvent aux
chapitres 1, 3, 4 et 5. Les entiers de largeur fixe n'ont pas d'`expt` (ils n'ont pas de promotion et
déborderaient ; passez à `int` avec `(as int x)` et utilisez son `expt`).

### 8.1 Formes variadiques et formes à 0/1 argument

L'arithmétique et la comparaison de CL sont variadiques, mais les méthodes ne sont résolues que selon le type du
receveur, pas selon le nombre d'arguments. **Le vérificateur développe donc les formes suivantes en appels à deux
arguments.**

| Forme qu'on peut écrire | Développement | S'applique à |
|---|---|---|
| `(op a b c ...)` | Le pli à gauche `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)`, chaque terme étant lié à une variable temporaire | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Ceux des précédents qui ont un élément neutre |
| `(op x)` | Pour `+ * max min logand logior logxor`, `x` lui-même. `(- x)` donne l'opposé, `(/ x)` l'inverse, `(gcd x)`/`(lcm x)` donnent `(abs x)` (comme en CL) | Idem |
| `(cmp x)` | Évalue `x` et donne `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Chaque terme est évalué exactement une fois, de gauche à droite (c'est pourquoi les comparaisons variadiques passent
par des variables temporaires). La forme variadique de `/=` compare **les paires adjacentes**, contrairement à CL,
qui demande si toutes les paires diffèrent.

### 8.2 `isqrt` et `expt` entier

| Nom | Forme | Type | Description |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | Le plus grand entier ne dépassant pas la racine carrée. Panic sur une valeur négative |
| `expt` | `(expt n e)` | `(T,T)→T` | Puissance (par élévation au carré). CL renvoie un rationnel pour un exposant négatif, mais un type entier ne peut pas le représenter ; c'est donc un panic ; convertissez d'abord en `ratio` |

## 9. Prédicats

| Nom | Forme | Type | Types |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (types entiers uniquement, comme en CL) |

Il n'y a **pas de prédicats de type** comme les `numberp`/`integerp`/`floatp` de CL. Avec le typage statique, le
type d'une valeur est déjà fixé sans avoir à le demander à l'exécution.

## 10. Constantes

| Nom | Type | Valeur |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Codes d'opération passés à `boole` (à la place des mots-clés de CL) |

Constantes de limites numériques (CLHS 12.1.4.2 / 12.1.3) :

| Nom | Type | Description |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | Les limites supérieure / inférieure d'une valeur immédiate sur 63 bits (2^62-1 / -2^62). Un `int` au-delà devient un bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | Les plus grande / plus petite valeurs finies |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | La plus petite grandeur non nulle, sous-normaux compris |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | Idem, limité aux nombres normalisés |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | Ils suivent la définition de CL (le plus petit `e` positif tel que `(/= (+ 1 e) 1)`) ; ils sont donc **plus grands d'un ULP que** 2^-53 : 2^-53 lui-même s'arrondit à `1.0` avec l'arrondi au plus proche pair |

## 11. Opérations sur les bits

Définies sur le complément à deux avec une infinité de bits (CL 12.10). Elles sont implémentées pour les types
entiers de largeur fixe et `int`, pas pour `ratio` (CL n'a lui aussi d'opérations sur les bits que pour les
entiers).

| Nom | Forme | Type | Description |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | Et, ou, ou exclusif bit à bit (versions variadiques et sans argument en 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Complément bit à bit |
| `ash` | `(ash x count)` | `(T,int)→T` | Décalage arithmétique. À gauche si `count` est positif, à droite s'il est négatif |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Si le bit `index` est à 1 (**l'ordre des arguments est l'inverse de CL** ; voir ci-dessous) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | Le nombre de bits à 1 (pour un nombre négatif, le nombre de bits à 0) |
| `integer-length` | `(integer-length x)` | `T→T` | Le nombre de bits nécessaires pour le représenter, sans compter le signe |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | Les sept restantes, composées à partir des précédentes |

**Seul le second argument de `ash` est `int` plutôt que `T`.** C'est une **distance** en bits, pas une valeur du
type du receveur ; la largeur et le signe du receveur ne disent donc rien de la distance (pour la même raison que le
`count` de `(ash integer count)` en CL est un entier quelconque). Décaler à droite une valeur non signée est un
décalage logique (`(ash (the u8 200) -3)` = `25`), et une valeur signée un décalage arithmétique arrondissant vers
moins l'infini (`(ash (the i32 -100) -4)` = `-7`). L'`index` de `logbitp` est `int` pour la même raison.

**Spécificateurs d'octet.** Au lieu de l'objet opaque que renvoie le `byte` de CL, on utilise une
`cons-cell<int,int>` (`car`=taille, `cdr`=position). Taille et position sont des nombres de bits ; elles sont donc
`int` quelle que soit la largeur de l'entier décomposé.

**L'entier est le premier argument, dans un ordre différent de CL.** CL écrit `(ldb bytespec integer)`, mais ce
langage choisit une méthode selon le type du receveur (le premier argument), et avec le spécificateur en premier,
il ne pourrait pas choisir selon le type de l'entier. Toutes les autres opérations sur les bits ont la forme
`(op integer ...)` (`(logand a b)`, `(ash x count)`, `(lognot x)`), et seules la famille `ldb` et `logbitp` étaient
dans l'autre sens ; elles ont donc été alignées. Les autres arguments gardent l'ordre relatif de CL ;
`(dpb newbyte spec n)` devient donc `(dpb n newbyte spec)`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Crée un spécificateur d'octet |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Extrait une composante |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Extrait de `x` l'octet spécifié, aligné à droite |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Si un bit de l'octet spécifié est à 1 |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Efface tout ce qui est hors de l'octet spécifié (en gardant les positions) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Dépose le `newbyte` aligné à droite dans l'octet spécifié de `x` |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | La version de `dpb` qui conserve les positions |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | L'une des 16 opérations logiques à deux opérandes, choisie par `op` (une constante `boole-*` du chapitre 10) |

`T` est un type qui implémente le trait `Bits`, à savoir `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Seul `boole`
garde `op` en premier, puisqu'il n'y a aucune raison d'y changer l'ordre de CL.

## 12. Nombres aléatoires

| Nom | Forme | Type | Description |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Un nombre aléatoire de `0` inclus à `n` exclu. Si l'état est omis, tire dans `*random-state*` et le fait avancer |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Sans argument, un nouvel état ; avec un état, une copie de celui-ci (la copie rejoue la même suite) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Toujours `true` (le type statique exclut déjà les autres types ; il n'existe que pour correspondre à CL) |
| `*random-state*` | — | `random-state` | L'état par défaut de `random`. Une variable globale affectable (remplacez-la avec `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | L'état que désigne l'entier. La même graine rejoue toujours la même suite |

Le générateur est xorshift64 et renvoie la même suite en interprétation comme en compilation.

Un nouvel état de `make-random-state` est initialisé à partir de l'horloge ; il ne peut donc pas être reproduit d'une
exécution à l'autre. Pour reproduire, utilisez `seed-random-state` :

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; affiche les trois mêmes nombres à chaque exécution
```

**CL n'a pas de moyen portable de donner une graine** (`make-random-state` ne prend que `nil`/`t`/un état) ; ce nom
suit donc le `sb-ext:seed-random-state` de SBCL plutôt que CL.

Des graines différentes donnent des suites différentes. `(seed-random-state 0)` et `(seed-random-state 1)` donnent
des suites différentes, tout comme `-7` et `7`.
