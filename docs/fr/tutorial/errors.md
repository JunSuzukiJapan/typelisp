<!-- translated-from: docs/ja/tutorial/errors.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Gestion des erreurs

La gestion des erreurs de typelisp répartit les échecs en deux sortes.

| Sorte d'échec | Exemples | Comment l'exprimer |
|---|---|---|
| Échecs qui peuvent arriver (récupérables) | Un fichier manque, une saisie n'est pas un nombre | Renvoyer un `Result<T,E>` |
| Erreurs dans le programme (irrécupérables) | Un indice hors limites, `unwrap` sur `none`, une division par zéro | S'arrêter avec `panic` |

S'y ajoutent `catch` / `throw`, qui sortent de plusieurs appels de fonctions d'un coup, et `unwind-protect`, qui
exécute un nettoyage quelle que soit la façon dont son corps est quitté. Ce chapitre suppose que vous avez lu la
section `Result` de [Bases des types](types.md).

## 1. Renvoyer un `Result` et le recevoir avec `match`

Voici une fonction qui lit un numéro de port depuis une chaîne. Elle peut échouer de deux façons : la saisie n'est
pas un nombre, ou elle est hors limites.

```lisp
(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (and (>= n 1) (<= n 65535))
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))
```

L'appelant sépare le succès de l'échec avec `match`.

```lisp
(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))
;; error: port out of range: 99999
```

- La valeur d'une fonction qui renvoie un `Result` ne peut pas être utilisée tant que `match` ne traite pas le cas
  `err`. Oublier de traiter l'échec est une erreur de type.
- L'erreur de `parse-int` est une valeur de type `ParseIntError`. `(message e)` donne sa chaîne de message.

## 2. Transmettre un échec à l'appelant

Il n'existe pas de raccourci comme le `?` de Rust. Quand on appelle à la suite plusieurs fonctions qui renvoient un
`Result`, on écrit la partie « renvoyer l'échec tel quel » avec `match`.

```lisp
(defun add-ports ((a string) (b string)) Result<int,string>
  (match (parse-port a)
    ((err m) (Result::err m))
    ((ok x) (match (parse-port b)
              ((err m) (Result::err m))
              ((ok y) (Result::ok (+ x y)))))))

(add-ports "1" "2")       ; => (ok 3)
(add-ports "1" "x")       ; => (err "parse-int: invalid integer literal: \"x\"")
```

Quand on sait qu'une opération ne peut pas échouer, ou dans un petit script où s'arrêter en cas d'échec convient,
`unwrap` extrait le contenu. Si la valeur est un `err`, c'est un panic. Si une valeur par défaut suffit, utilisez
`unwrap-or`.

## 3. Créer son propre type d'erreur

Exprimer les erreurs par un type plutôt que par une chaîne permet à l'appelant de bifurquer selon la sorte
d'erreur. Un type d'erreur est un `defenum` ou un `defstruct` ordinaire qui implémente le trait `Error`.

```lisp
(defenum config-error
  (missing string)          ; un réglage manque
  (invalid string int))     ; une valeur est incorrecte

(impl Error config-error
  (message ((self Self)) string
    (match self
      ((missing key) (format false "missing key: ~a" key))
      ((invalid key v) (format false "invalid value for ~a: ~a" key v))))
  (source ((self Self)) Option<:dyn Error> (Option::none)))

(defun check-workers ((n int)) Result<int,config-error>
  (if (> n 0)
      (Result::ok n)
      (Result::err (config-error::invalid "workers" n))))
```

- `message` renvoie une description de l'erreur.
- `source` renvoie une autre erreur qui a causé celle-ci. Sans cause, c'est `none`.

## 4. Combiner différentes sortes d'erreurs

Si une fonction appelle à la fois `parse-int` (`ParseIntError`) et `check-workers` (`config-error`), il y a deux
types d'erreur, et ils ne peuvent pas être tous deux le `E` d'un même `Result<T,E>`. Dans ce cas, faites de `E` un
`:dyn Error` (une erreur de n'importe quel type qui implémente `Error`). Convertissez chaque erreur avec
`as-dyn-error`.

```lisp
(defun load-count ((s string)) Result<int, :dyn Error>
  (match (as-dyn-error (parse-int s))
    ((ok n) (as-dyn-error (check-workers n)))
    ((err e) (Result::err e))))
```

Avec `"4"`, `"-1"` et `"abc"`, les résultats sont :

```
ok 4
err invalid value for workers: -1
err parse-int: invalid integer literal: "abc"
```

Pour `:dyn`, voir la section 5 de [Traits](traits.md).

## 5. `panic` : les erreurs dans le programme

Quand le programme atteint un état qui ne doit jamais se produire, arrêtez-le avec `panic`.

```lisp
(defun safe-get ((v Vector<int>) (i int)) int
  (if (< i (len v))
      (get v i)
      (panic (format false "index ~a out of range" i))))
```

```
error: main.typl:4:7: panic: index 3 out of range
```

- Le type de `panic` est `!` (il ne revient pas) ; on peut donc l'écrire partout où un type quelconque est
  attendu. C'est pourquoi les deux branches du `if` ci-dessus concordent.
- Ces opérations déclenchent aussi un panic : `unwrap` sur `none` ou `err`, `get` avec un indice hors limites et
  la division entière par zéro.
- `panic` arrête le programme. Même s'il survient dans une tâche, tout le programme s'arrête.
- Dans la REPL, un `panic` ne met pas fin à la REPL ; elle attend la saisie suivante.
- On peut écrire `(todo)` pour « pas encore écrit » et `(unreachable)` pour « ce point ne devrait jamais être
  atteint ». Les deux déclenchent un panic.

`panic` ne remplace pas `Result`. Pour les échecs qui peuvent arriver, comme une saisie de l'utilisateur ou
l'existence d'un fichier, utilisez `Result`.

## 6. `catch` / `throw` : sortir à travers les fonctions

`throw` saute directement jusqu'au `catch` englobant de même étiquette, quel que soit le nombre d'appels de
fonctions entre les deux.

```lisp
(defun check-all ((v Vector<int>)) ()
  (doiter (x (iter v))
    (if (< x 0)
        (throw 'bad-input (format false "negative: ~a" x))
        ())))

(defun validate ((v Vector<int>)) string
  (catch 'bad-input
    (progn
      (check-all v)
      "all fine")))
```

Si `v` ne contient aucun nombre négatif, `validate` renvoie `"all fine"` ; s'il contient `-7`, le contrôle saute
de l'intérieur de `check-all` jusqu'au `catch`, qui renvoie `"negative: -7"`.

- Écrivez l'étiquette comme un simple symbole, par exemple `'bad-input`.
- **Chaque étiquette transporte des valeurs d'un seul type.** Dans l'exemple ci-dessus, `'bad-input` transporte
  une `string` ; lancer un `int` avec la même étiquette est donc une erreur de type. Le type du corps du `catch`
  doit aussi correspondre au type de l'étiquette.

  ```
  error: ...: type error: type mismatch: expected `string`, found `int`
  ```

- Un `throw` sans `catch` de même étiquette à atteindre est une erreur.

Si vous voulez seulement revenir plus tôt depuis une fonction, utilisez `return-from` au lieu de `catch` /
`throw`. `return-from` ne peut pas traverser les fonctions, mais en échange on voit où il revient en lisant le
source.

```lisp
(defun first-negative ((v Vector<int>)) Option<int>
  (doiter (x (iter v))
    (if (< x 0) (return-from first-negative (Option::some x)) ()))
  (Option::none))
```

## 7. `unwind-protect` : toujours nettoyer

`(unwind-protect body cleanup)` exécute le nettoyage quelle que soit la façon dont le corps est quitté : quand il
se termine normalement, quand il est quitté par `throw` et quand il déclenche un panic.

```lisp
(defun with-cleanup ((n int)) int
  (unwind-protect
    (progn
      (println "working on ~a" n)
      (if (< n 0) (throw 'bad-input "negative") ())
      (* n 2))
    (println "cleanup for ~a" n)))
```

```lisp
(println "~a" (with-cleanup 5))
;; working on 5
;; cleanup for 5
;; 10

(println "~a" (catch 'bad-input (progn (with-cleanup -1) "unreached")))
;; working on -1
;; cleanup for -1
;; negative
```

Utilisez-le pour des choses comme « toujours fermer un fichier ouvert » ou « toujours libérer un verrou pris ».
Les `with-open-file` et `with-lock` de la bibliothèque standard utilisent `unwind-protect` en interne.

## 8. À propos du système de conditions de Common Lisp

typelisp n'adopte pas le système de conditions de Common Lisp (`handler-case`, `restart-case`, etc.). Il ne fait
pas apparaître dans les types les échecs qu'une fonction peut provoquer, ce qui s'accorde mal avec le typage
statique. Les échecs qui peuvent arriver s'écrivent dans les types avec `Result`, et les transferts de contrôle se
font avec `catch` / `throw`.

## 9. Que lire ensuite

- [Concurrence](concurrency.md) : tâches et canaux
- [Option, Result et types d'erreur](../reference/functions/option-result.md) : la liste des fonctions
- [Messages d'erreur](../reference/errors.md) : ce que signifient les erreurs courantes et comment les corriger
