<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
# Option, Result et types d'erreur

## 1. `Option<T>` / `Result<T,E>`

Constructeurs : `Option<T>` a `Some(T)` / `None`. `Result<T,E>` a `Ok(T)` / `Err(E)`. `E` peut être n'importe quel
type : les types d'erreur concrets intégrés, comme les types que vous écrivez vous-même avec
`defstruct`/`defenum`, y trouvent leur place de la même façon (chapitre 3).

| Nom | Forme | Option | Result | Description |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Extrait la valeur. Panic sur `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | La valeur, ou la valeur par défaut |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Si c'est `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Si c'est `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Si c'est `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Si c'est `Err` |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | Extrait la valeur. Panic avec `msg` sur `None`/`Err` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | La valeur, ou le résultat de `f`. `f` n'est appelée que sur `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | Applique `f` au contenu de `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | Applique `f` au contenu de `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | Sur `Some`/`Ok`, passe le contenu à `f` et renvoie son résultat |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | Sur `None`/`Err`, renvoie le résultat de `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | Transforme `Some(v)` en `Ok(v)` et `None` en `Err(e)` |

Les constructeurs sont `Option::some`/`Option::none`/`Result::ok`/`Result::err` (ou, après
`(use option)`/`(use result)`, les noms nus `some`/`none`/`ok`/`err`).

Le branchement s'écrit explicitement avec `match`, ou s'enchaîne avec `map`/`and-then` et les autres
ci-dessus. Il n'y a pas de syntaxe correspondant au `?` de Rust.

Le `map` de `Option`/`Result` est une méthode, distincte du `map` des [séquences](sequences.md).
C'est lui qui est appelé quand le type du premier argument est `Option`/`Result`.

La macro `->` passe une valeur comme premier argument de chaque forme suivante, dans l'ordre (comme
`->` de Clojure). `(-> x (f a) (g b))` devient `(g (f x a) b)`. Un nom sans parenthèses, `h`, est
traité comme `(h x)`. Le premier argument d'une méthode est son receveur, donc les combinateurs
s'enchaînent tels quels :

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. La représentation à l'exécution de `Option<T>`

Comme en Rust, **`Option<T>` ne crée généralement pas de boîte**. `some v` est `v` lui-même et `none` est la valeur
de la liste vide, sans allocation ni indirection. `Option<Sexpr>` (où la liste vide est `none`), `Option<int>`,
`Option<string>`, `Option<my-struct>`, `Option<f64>` et `Option<(fn ...)>` prennent tous cette forme.

Une boîte n'est utilisée que lorsqu'une valeur de `T` ne peut pas être distinguée de la valeur de la liste vide :

| `T` | Représentation | Raison |
|---|---|---|
| `Option<U>` (imbriqué) | Boîte | Le `none` intérieur serait la même valeur que le `none` extérieur |
| `()` | Boîte | La valeur de `()` est la valeur de la liste vide elle-même |
| `ptr` / `c-long` / `c-ulong` | Boîte | Les 64 bits sont tous de la valeur ; il ne reste pas de place pour les distinguer |
| Tout le reste | Pas de boîte | — |

La représentation est décidée par le seul type et ne peut pas se lire dans une valeur. À l'affichage, `(some ...)`/
`none` est reconstruit à partir du type statique ; `(format false "~a" opt)` affiche donc `(some 1)`. Il y a deux
restrictions :

- **Elle ne peut pas être placée dans un `:dyn Trait`** (passer une valeur de `Option<int>` pour laquelle vous avez
  écrit `(impl Speak Option<int> ...)` à un `:dyn Speak` est une erreur).
- Un transtypage descendant `(the Option<T> ...)` depuis un `Sexpr` **nomme un constructeur** :
  `(the Option<int> (some x))` / `(the Option<int> (none))`. La forme qui lie la valeur entière,
  `(the Option<int> o)`, est une erreur.

## 3. Types d'erreur et le trait `Error`

Sur le modèle du `std::error::Error` de Rust, **`Error` n'est pas un type mais un trait**. Les types concrets qui
représentent les erreurs sont distincts pour chaque usage, et chacun implémente `Error`.

| Type | Produit par |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Opérations sur les fichiers et les flux ([Flux et fichiers](streams-files.md)) |
| `NetError` | Opérations réseau ([Réseau](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. Le `simple-error` de CL : le choix par défaut quand on veut juste dire ce qui s'est passé |
| `WrappedError` | `(wrap-error msg cause)`. Un type qui porte à la fois votre propre message et la cause ; c'est pourquoi le trait `Error` a `source` |

`ParseIntError` à `NetError` sont chacun « une énumération à une seule variante contenant une chaîne de message »,
et le nom du type et celui de la variante sont identiques (`(match e ((ParseIntError m) m))`, construit avec
`(ParseIntError::ParseIntError "...")`). Ils n'ont rien de particulier : ils sont traités exactement comme vos
propres types d'erreur écrits avec `(defstruct my-err (...))` / `(defenum my-err ...)`.

| Nom | Forme | Type | Description |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Le message d'erreur (une méthode du trait `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | La cause qu'enveloppe cette erreur, ou `None` s'il n'y en a pas (le `Error::source` de Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implémente `Error`) | Élargit un type d'erreur concret en objet trait |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implémente `Error`) | Le message et la chaîne des causes trouvée en suivant `source`, une cause par ligne. CL n'a pas d'équivalent (le « caused by » de Rust) |

Si vous implémentez `Error` pour votre propre type d'erreur, il peut être traité **de la même façon** que les
erreurs intégrées :

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; le type concret va tel quel dans E
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; traiter uniformément toutes les sortes
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Pour réunir plusieurs types d'erreur dans un même `Result`, utilisez `Result<T, :dyn Error>` (correspondant au
`Box<dyn Error>` de Rust) et élargissez les erreurs concrètes avec `as-dyn-error`. Comme il n'y a pas de `?`, cette
conversion s'écrit explicitement :

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Les types et les traits partagent un même espace de noms** (comme en Rust). Dans un module, un
`defstruct`/`defenum` et un trait ne peuvent pas avoir le même nom, et écrire un nom de trait à une position de type
est signalé par « `error` is a trait, not a type — write `:dyn error` ».

Les échecs irrécupérables s'expriment avec `panic`. Pour `panic` et `catch`/`throw`, voir la
[Référence de la syntaxe](../syntax.md#8-sorties-non-locales-catch--throw--unwind-protect) ; pour les principes de
gestion des erreurs, voir [son chapitre 9](../syntax.md#9-principes-de-gestion-des-erreurs).
