<!-- translated-from: docs/ja/reference/types.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Types

Les types de typelisp et les traits standard que chaque type implémente. La façon d'écrire les types se trouve au
[chapitre 2 de la référence de la syntaxe](syntax.md#2-écrire-les-types) ; les fonctions et méthodes de chaque
type se trouvent dans [Fonctions intégrées](functions/README.md).

## 1. Types primitifs

| Type | Contenu | Détails |
|---|---|---|
| `int` | Un entier en précision arbitraire. Conservé comme valeur immédiate tant qu'il tient sur 63 bits, il devient automatiquement un bignum au-delà. Le type par défaut des littéraux entiers sans annotation | [Nombres, chapitre 3](functions/numbers.md#3-entiers-en-précision-arbitraire-int) |
| `i8` `i16` `i32` | Entiers signés de largeur fixe | [Nombres, chapitre 1](functions/numbers.md#1-entiers-de-largeur-fixe) |
| `u8` `u16` `u32` | Entiers non signés de largeur fixe | Idem |
| `f32` `f64` | Nombres à virgule flottante IEEE 754. Les littéraux décimaux sont `f64` par défaut | [Nombres, chapitre 4](functions/numbers.md#4-nombres-à-virgule-flottante-f64--f32) |
| `ratio` | Un nombre rationnel irréductible | [Nombres, chapitre 5](functions/numbers.md#5-rationnels-ratio) |
| `bool` | `true` / `false` | [Nombres, chapitre 7](functions/numbers.md#7-booléens) |
| `char` | Une valeur scalaire Unicode | [Caractères](functions/collections.md#2-caractères-char) |
| `string` | Une chaîne immuable | [Chaînes](functions/collections.md#1-chaînes-string) |
| `symbol` | Un symbole. Les mots-clés (`:name`) ont aussi ce type | [Symboles](functions/sequences.md#3-symboles) |
| `()` | Le type Unit. Sa valeur est aussi `()` | |
| `!` | Le type Never. Le type des expressions qui ne reviennent pas, comme `panic`. Peut se placer partout où un type quelconque est attendu | |
| `ptr` `c-long` `c-ulong` | Des mots servant uniquement à passer des valeurs à C et à en recevoir. Ils ne peuvent être des valeurs qu'à l'intérieur de `unsafe`, et les endroits où ils peuvent apparaître sont limités | [Nombres, chapitre 2](functions/numbers.md#2-mots-bruts-à-la-frontière-c-ptr--c-long--c-ulong) |
| `random-state` | L'état d'un générateur de nombres aléatoires | [Nombres, chapitre 12](functions/numbers.md#12-nombres-aléatoires) |

Il n'existe pas de type entier sur 64 bits. Pour les entiers dont la largeur importe peu, utilisez `int`.

## 2. Types génériques intégrés

| Type | Contenu | Détails |
|---|---|---|
| `Option<T>` | Une valeur présente ou non. `some` / `none` | [Option et Result](functions/option-result.md) |
| `Result<T,E>` | Succès ou échec. `ok` / `err` | Idem |
| `Vector<T>` | Un tableau extensible | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Une table de hachage. Le type des clés doit implémenter `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `Task<T>` | Une poignée de tâche | [Tâches](functions/concurrency.md#1-taskt--poignées-de-tâches) |
| `Thread<T>` | Une poignée de tâche qui s'exécute sur un thread système dédié | [Thread](functions/concurrency.md#7-threadt--threads-système-dédiés) |
| `Chan<T>` | Un canal | [Canaux](functions/concurrency.md#2-chant--canaux) |

Les types de fonctions s'écrivent `(fn (types-des-arguments...) type-de-retour)`, et les objets trait `:dyn Trait`
([chapitre 2 de la référence de la syntaxe](syntax.md#2-écrire-les-types)).

## 3. Données S-expression

| Type | Contenu | Détails |
|---|---|---|
| `Sexpr` | Une S-expression non vide. 18 variantes : `int`, `i8` à `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array` | [Données S-expression](functions/sequences.md#2-données-s-expression-sexpr) |
| `Option<Sexpr>` | Les données S-expression en général. La liste vide `()` est `none` | Idem |

## 4. Types de la bibliothèque standard

Les types que la bibliothèque standard (le prélude) définit avec `defstruct` / `defenum`. Ils sont traités comme les
types que vous écrivez vous-même, et tout ce qu'on peut faire avec un `defstruct` peut se faire avec eux.

| Type | Contenu | Détails |
|---|---|---|
| `cons-cell<A,B>` | Une paire. `cons`/`car`/`cdr` | [Paires](functions/sequences.md#1-paires-cons-cellab) |
| `complex` | Un nombre complexe (composantes `f64`) | [Nombres, chapitre 6](functions/numbers.md#6-nombres-complexes-complex) |
| `Array<T>` | Un tableau multidimensionnel | [Array](functions/collections.md#5-arrayt-tableaux-multidimensionnels) |
| `BitVector` | Une suite de bits de longueur fixe | [BitVector](functions/collections.md#6-bitvector-vecteurs-de-bits) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Les itérateurs renvoyés par `iter` de chaque collection | [Iter](functions/traits.md#1-le-trait-iter-et-litération) |
| `WaitGroup` | Attendre la fin de N choses | [WaitGroup](functions/concurrency.md#4-waitgroup--attendre-n-achèvements) |
| `Mutex<T>` | Exclusion mutuelle pour les données partagées | [Mutex](functions/concurrency.md#6-mutext--exclusion-mutuelle-pour-les-données-partagées) |
| `pathname` | Un nom de fichier découpé en parties | [Noms de chemin](functions/streams-files.md#9-noms-de-chemin-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Flux | [Flux](functions/streams-files.md#3-types-de-flux-concrets) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Flux composites | [Flux composites](functions/streams-files.md#4-flux-composites) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Réseau | [Réseau](functions/network.md#1-types) |
| `ReadOutcome` | Le résultat de `read-sexpr`. `datum` / `eof` | [Flux](functions/streams-files.md#6-fonctions-génériques-et-opérations-sur-les-fichiers) |
| `universal-time` `internal-time` `decoded-time` | Temps | [Temps](functions/system.md#1-temps) |
| `heap-info` | L'état actuel du tas | [Outils de l'implémentation](functions/system.md#51-champs-de-heap-info) |

## 5. Types d'erreur

`Error` n'est pas un type mais un trait, et les types suivants l'implémentent. Pour traiter des erreurs de
n'importe quelle sorte, écrivez `:dyn Error`.

| Type | Produit par |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Opérations sur les fichiers et les flux |
| `NetError` | Opérations réseau |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Les détails se trouvent dans [Types d'erreur et le trait Error](functions/option-result.md#3-types-derreur-et-le-trait-error).

## 6. Implémentations des traits standard

Quels types implémentent quels traits. Les méthodes de chaque trait se trouvent dans
[Traits standard](functions/traits.md) et dans les chapitres de la colonne de droite.

### 6.1 Comparaison, hachage et affichage

| Trait | Types qui l'implémentent | Détails |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` | [Eq / Ord](functions/traits.md#2-eq--ord-comparaison) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` | Idem |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Array<T>` `pathname` `universal-time` `internal-time` et tous les types d'erreur intégrés | [print-object](functions/printing.md#5-print-object-représentation-imprimée-par-type) |

`Eq`/`Ord` de `cons-cell<A,B>` sont utilisables quand les types des éléments implémentent `Eq`/`Ord`.

### 6.2 Arithmétique

| Trait | Types qui l'implémentent |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Les détails se trouvent dans [Traits arithmétiques](functions/traits.md#3-traits-arithmétiques-add--sub--mul--div--rem--bits--number).

### 6.3 Itération

| Trait | Types qui l'implémentent |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` |

### 6.4 Flux

| Type | Traits implémentés |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

Chaque flux implémente `Stream` ; les flux d'entrée implémentent en plus `InputStream`, et les flux de sortie
`OutputStream`. `socket-listener` et `udp-socket` n'implémentent que `Stream` (`close` / `open-stream-p`). Les
détails se trouvent dans [Flux](functions/streams-files.md#1-la-hiérarchie-des-traits).

### 6.5 Autres

| Trait | Types qui l'implémentent | Détails |
|---|---|---|
| `Error` | Tous les types d'erreur du chapitre 5 | [Types d'erreur](functions/option-result.md#3-types-derreur-et-le-trait-error) |
| `Pathish` | `string` `pathname` | [Noms de chemin](functions/streams-files.md#91-le-trait-de-désignateur-de-chemin-pathish) |
