<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# typelisp-documentatie (Nederlands)

typelisp is een statisch getypeerde Lisp. Voor het installeren en bouwen zie de
[README.md](../../README.md) (in het Engels) bovenaan de repository.

## Kenmerken

### Taken en kanalen

Je start taken (lichtgewicht threads) die via kanalen waarden aan elkaar doorgeven. Zie
[Gelijktijdigheid](tutorial/concurrency.md) voor details.

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

### Generics

Typeparameters schrijf je tussen punthaken na de naam van een type of functie. Trait bounds komen
in een `:where`-clausule. Zie [Basis van types](tutorial/types.md) en [Traits](tutorial/traits.md)
voor details.

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

### Option en Result in plaats van nil

Er is geen `nil`. Een functie die mogelijk geen waarde heeft, geeft `Option<T>` terug, en een
functie die kan mislukken geeft `Result<T,E>` terug; de inhoud haal je eruit met `match`. Een
`Option<string>` gebruiken als `string`, of het geval `none` vergeten in een `match`, is een
typefout nog voordat het programma draait. Zie [Basis van types](tutorial/types.md) en
[Foutafhandeling](tutorial/errors.md) voor details.

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

### Ketenen met `->`

De macro `->` geeft een waarde om de beurt door als eerste argument van elke volgende vorm.
`(-> x (f a) (g b))` is hetzelfde als `(g (f x a) b)`. Stappen die een `Option` of `Result`
teruggeven schrijf je zo van boven naar beneden. Zie
[Option, Result en foutentypes](reference/functions/option-result.md) voor details.

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

### Macro's met typecontrole

Macro's schrijf je zoals in Common Lisp, met `defmacro`, quasiquote en `gensym`. De geëxpandeerde
code wordt net zo op types gecontroleerd als met de hand geschreven code; past die niet bij de
types, dan wordt de fout gemeld op de plek waar de macro werd aangeroepen. Zie
[Macro's](tutorial/macros.md) voor details.

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

## Tutorial

Ben je nieuw met typelisp, lees deze documenten dan in volgorde.

- [Aan de slag](tutorial/intro.md): de REPL, functies, variabelen, voorwaarden, lussen, lijsten en `Vector`
- [Basis van types](tutorial/types.md): statische types, `Option`, `Result`, structs, enums, generics
- [Traits](tutorial/traits.md): `deftrait` / `impl`, trait bounds, `:dyn`
- [Macro's](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Foutafhandeling](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Gelijktijdigheid](tutorial/concurrency.md): taken, kanalen, `select`, `Mutex`, `thread`

## Handleidingen

- [Modules en bestandsindeling](guide/modules.md): `use`, `pub`, hoe bestanden op modules worden afgebeeld
- [Compileren](guide/compile.md): de JIT, uitvoerbare bestanden bouwen met AOT-compilatie, dumps
- [Bestands-I/O, streams en netwerk](guide/io.md): bestanden, padnamen, TCP / TLS / UDP, naamresolutie
- [C-FFI](guide/ffi.md): C-functies aanroepen met `defffi` (inclusief callbacks en C-structs met `def-c-struct`)
- [Editorintegratie](guide/editors.md): `typl-lsp` en het inrichten van VS Code / Emacs
- [Voor Common Lisp-programmeurs](guide/from-common-lisp.md): hoe typelisp van CL verschilt en hoe je CL-code herschrijft

## Referentie

- [Syntaxreferentie](reference/syntax.md): lexicale syntaxis, het schrijven van types, definities, besturingsvormen, compilatie, gelijktijdigheid
- [Ingebouwde functies](reference/functions/README.md): ingebouwde functies, methoden en de standaardbibliotheek
- [Types](reference/types.md): de types en de traits die elk type implementeert
- [Foutmeldingen](reference/errors.md): wat veelvoorkomende fouten betekenen en hoe je ze oplost

## Editorintegratie

Instructies voor het inrichten staan in de [handleiding Editorintegratie](guide/editors.md). De
sneltoetsen en instellingen van elke editor staan in deze documenten:

- [Emacs (typelisp-mode)](../../editor/emacs/README_nl.md)
- [VS Code](../../editor/vscode/README_nl.md)
