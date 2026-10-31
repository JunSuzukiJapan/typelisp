<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# typelisp-dokumentation (Svenska)

typelisp är ett statiskt typat Lisp. Installation och bygge beskrivs i [README.md](../../README.md)
(på engelska) högst upp i repositoriet.

## Egenskaper

### Tasks och kanaler

Du startar tasks (lättviktstrådar) som skickar värden till varandra genom kanaler. Se
[Samtidighet](tutorial/concurrency.md) för detaljer.

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

### Generiska typer

Typparametrar skrivs inom vinkelparenteser efter namnet på en typ eller funktion. Trait-gränser
skrivs i en `:where`-klausul. Se [Grunderna i typer](tutorial/types.md) och [Traits](tutorial/traits.md)
för detaljer.

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

### Option och Result i stället för nil

Det finns inget `nil`. En funktion som kanske saknar värde returnerar `Option<T>`, och en som kan
misslyckas returnerar `Result<T,E>`; innehållet tas ut med `match`. Att använda en `Option<string>`
som en `string`, eller att glömma fallet `none` i en `match`, är ett typfel redan innan programmet
körs. Se [Grunderna i typer](tutorial/types.md) och [Felhantering](tutorial/errors.md) för detaljer.

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

### Kedja med `->`

Makrot `->` skickar ett värde i tur och ordning som första argument till varje följande form.
`(-> x (f a) (g b))` är detsamma som `(g (f x a) b)`. Steg som returnerar `Option` eller `Result`
kan då skrivas uppifrån och ned. Se [Option, Result och feltyper](reference/functions/option-result.md)
för detaljer.

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

### Typkontrollerade makron

Makron skrivs med `defmacro`, quasiquote och `gensym`, som i Common Lisp. Den expanderade koden
typkontrolleras precis som handskriven kod, och om den inte stämmer med typerna rapporteras felet
där makrot anropades. Se [Makron](tutorial/macros.md) för detaljer.

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

## Handledning

Är du ny med typelisp, läs dessa i ordning.

- [Komma igång](tutorial/intro.md): REPL, funktioner, variabler, villkor, slingor, listor och `Vector`
- [Grunderna i typer](tutorial/types.md): statiska typer, `Option`, `Result`, structs, enums, generiska typer
- [Traits](tutorial/traits.md): `deftrait` / `impl`, trait-gränser, `:dyn`
- [Makron](tutorial/macros.md): `defmacro`, quasiquote, `gensym`, `macrolet`
- [Felhantering](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Samtidighet](tutorial/concurrency.md): tasks, kanaler, `select`, `Mutex`, `thread`

## Guider

- [Moduler och filuppdelning](guide/modules.md): `use`, `pub`, hur filer motsvarar moduler
- [Kompilering](guide/compile.md): JIT, att bygga körbara filer med AOT-kompilering, dumpar
- [Fil-I/O, strömmar och nätverk](guide/io.md): filer, pathnames, TCP / TLS / UDP, namnuppslagning
- [C FFI](guide/ffi.md): att anropa C-funktioner med `defffi` (inklusive callbacks och C-structs med `def-c-struct`)
- [Editorintegration](guide/editors.md): `typl-lsp` och hur man ställer in VS Code / Emacs
- [För Common Lisp-programmerare](guide/from-common-lisp.md): hur typelisp skiljer sig från CL och hur man skriver om CL-kod

## Referens

- [Syntaxreferens](reference/syntax.md): lexikalisk syntax, hur typer skrivs, definitioner, kontrollformer, kompilering, samtidighet
- [Inbyggda funktioner](reference/functions/README.md): inbyggda funktioner, metoder och standardbiblioteket
- [Typer](reference/types.md): typerna och de traits som var och en implementerar
- [Felmeddelanden](reference/errors.md): vad de vanliga felen betyder och hur man åtgärdar dem

## Editorintegration

Installationsanvisningar finns i [guiden för editorintegration](guide/editors.md). Tangentbindningar och
inställningar för varje editor står i dessa dokument:

- [Emacs (typelisp-mode)](../../editor/emacs/README_sv.md)
- [VS Code](../../editor/vscode/README_sv.md)
