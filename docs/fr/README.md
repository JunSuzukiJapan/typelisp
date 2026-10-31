<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# Documentation de typelisp (français)

typelisp est un Lisp à typage statique. Pour l'installer et le compiler, voir le
[README.md](../../README.md) (en anglais) à la racine du dépôt.

## Caractéristiques

### Tâches et canaux

On lance des tâches (des threads légers) qui se passent des valeurs par des canaux. Voir
[Concurrence](tutorial/concurrency.md) pour plus de détails.

```lisp
(use std::sync::Chan)
(defun worker ((name string) (jobs Chan<int>) (results Chan<string>)) ()
  (doiter (n jobs)
    (sleep 0.1)
    (send results (format false "~a: ~d * ~d = ~d" name n n (* n n)))))

(defun main () ()
  (let ((jobs (the Chan<int> (Chan::new 0)))
        (results (the Chan<string> (Chan::new 10))))
    (task (worker "A" jobs results))
    (task (worker "B" jobs results))
    (dotimes (i 4)
      (send jobs (+ i 1)))
    (close jobs)
    (dotimes (i 4)
      (println "~a" (unwrap (recv results))))))

(main)
```

### Génériques

Les paramètres de type s'écrivent entre chevrons après le nom d'un type ou d'une fonction. Les
bornes de traits se placent dans une clause `:where`. Voir [Bases des types](tutorial/types.md) et
[Traits](tutorial/traits.md) pour plus de détails.

```lisp
(defstruct Pair<A,B> (first A) (second B))

(impl Pair<A,B>
  (swap (self) Pair<B,A>
    (make-instance 'Pair :first self::second :second self::first)))

(defun largest<T> ((xs Vector<T>)) Option<T> (:where (Ord T))
  (let ((best (the Option<T> (Option::none))))
    (doiter (x (iter xs))
      (setf best (Option::some (match best
                                 ((some b) (if (> x b) x b))
                                 ((none) x)))))
    best))

(let ((p (swap (make-instance 'Pair :first 1 :second "one"))))
  (println "~a ~a" p::first p::second))                       ; one 1

(println "~a" (largest (the Vector<int> #(3 9 4))))           ; (some 9)
(println "~a" (largest (the Vector<string> #("pear" "fig")))) ; (some pear)
```

### Option et Result au lieu de nil

Il n'y a pas de `nil`. Une fonction qui peut ne pas avoir de valeur renvoie `Option<T>`, et une
fonction qui peut échouer renvoie `Result<T,E>` ; on en extrait le contenu avec `match`. Utiliser un
`Option<string>` comme un `string`, ou oublier le cas `none` dans un `match`, est une erreur de type
avant même l'exécution du programme. Voir [Bases des types](tutorial/types.md) et [Gestion des
erreurs](tutorial/errors.md) pour plus de détails.

```lisp
(use std::num::parse-int)
(defun find-user ((id int)) Option<string>
  (case id
    (1 (Option::some "alice"))
    (2 (Option::some "bob"))
    (:else (Option::none))))

(defun parse-port ((s string)) Result<int,string>
  (match (parse-int s)
    ((ok n) (if (<= 1 n 65535)
                (Result::ok n)
                (Result::err (format false "port out of range: ~a" n))))
    ((err e) (Result::err (message e)))))

(match (find-user 3)
  ((some name) (println "user: ~a" name))
  ((none) (println "not found")))        ; not found

(match (parse-port "99999")
  ((ok p) (println "port ~a" p))
  ((err m) (println "error: ~a" m)))     ; error: port out of range: 99999
```

### Enchaîner avec `->`

La macro `->` passe une valeur, tour à tour, comme premier argument de chacune des formes
suivantes. `(-> x (f a) (g b))` équivaut à `(g (f x a) b)`. Les étapes qui renvoient un `Option` ou
un `Result` s'écrivent ainsi de haut en bas. Voir
[Option, Result et types d'erreur](reference/functions/option-result.md) pour plus de détails.

```lisp
(use std::num::parse-int)
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2))
      (Option::some (truncate n 2))
      (Option::none)))

(println "~a"
  (-> (Option::some 40)
      (and-then half)                          ; (some 20)
      (and-then half)                          ; (some 10)
      (map (lambda ((x int)) int (+ x 1)))     ; (some 11)
      (unwrap-or 0)))                          ; 11

(println "~a"
  (-> (parse-int "abc")
      (map (lambda ((n int)) int (* n 2)))
      (unwrap-or -1)))                         ; -1
```

### Macros vérifiées par les types

Les macros s'écrivent avec `defmacro`, la quasi-citation et `gensym`, comme en Common Lisp. Le code
développé est vérifié comme du code écrit à la main ; s'il ne respecte pas les types, l'erreur est
signalée à l'endroit où la macro a été appelée. Voir [Macros](tutorial/macros.md) pour plus de
détails.

```lisp
(defmacro swap (a b)
  (let ((tmp (gensym "tmp")))
    `(let ((,tmp ,a))
       (setf ,a ,b)
       (setf ,b ,tmp))))

(let ((x 1) (y 2))
  (swap x y)
  (println "x=~a y=~a" x y))             ; x=2 y=1
```

```lisp
(defmacro add-a (x) `(+ ,x "a"))
(add-a 1)
;; error: type error: type mismatch: expected `int`, found `string`
```

## Tutoriel

Si vous découvrez typelisp, lisez ces documents dans l'ordre.

- [Premiers pas](tutorial/intro.md) : la REPL, les fonctions, les variables, les conditionnelles, les boucles, les listes et `Vector`
- [Bases des types](tutorial/types.md) : types statiques, `Option`, `Result`, structures, énumérations, génériques
- [Traits](tutorial/traits.md) : `deftrait` / `impl`, bornes de traits, `:dyn`
- [Macros](tutorial/macros.md) : `defmacro`, quasiquote, `gensym`, `macrolet`
- [Gestion des erreurs](tutorial/errors.md) : `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Concurrence](tutorial/concurrency.md) : tâches, canaux, `select`, `Mutex`, `thread`

## Guides

- [Modules et découpage en fichiers](guide/modules.md) : `use`, `pub`, correspondance entre fichiers et modules
- [Compilation](guide/compile.md) : le JIT, la création d'exécutables par compilation AOT, les dumps
- [E/S de fichiers, flux et réseau](guide/io.md) : fichiers, noms de chemin, TCP / TLS / UDP, résolution de noms
- [FFI C](guide/ffi.md) : appeler des fonctions C avec `defffi` (y compris les callbacks et les structures C avec `def-c-struct`)
- [Intégration aux éditeurs](guide/editors.md) : `typl-lsp` et la configuration de VS Code / Emacs
- [Pour les programmeurs Common Lisp](guide/from-common-lisp.md) : en quoi typelisp diffère de CL et comment réécrire du code CL

## Référence

- [Référence de la syntaxe](reference/syntax.md) : lexique, écriture des types, définitions, formes de contrôle, compilation, concurrence
- [Fonctions intégrées](reference/functions/README.md) : fonctions intégrées, méthodes et bibliothèque standard
- [Types](reference/types.md) : les types et les traits que chacun implémente
- [Messages d'erreur](reference/errors.md) : ce que signifient les erreurs courantes et comment les corriger

## Intégration aux éditeurs

Les étapes de configuration se trouvent dans le [guide d'intégration aux éditeurs](guide/editors.md). Les
raccourcis clavier et les réglages de chaque éditeur sont décrits dans ces documents :

- [Emacs (typelisp-mode)](../../editor/emacs/README_fr.md)
- [VS Code](../../editor/vscode/README_fr.md)
