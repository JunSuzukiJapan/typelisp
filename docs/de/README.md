<!-- translated-from: docs/ja/README.md @ 25173243b2eff6e1cad20c77d2fc8be7e35e0124 -->
# typelisp-Dokumentation (Deutsch)

typelisp ist ein statisch typisiertes Lisp. Installation und Bau sind in der
[README.md](../../README.md) (Englisch) im Wurzelverzeichnis des Repositorys beschrieben.

## Merkmale

### Tasks und Kanäle

Man startet Tasks (leichtgewichtige Threads), die sich Werte über Kanäle weiterreichen. Einzelheiten
stehen unter [Nebenläufigkeit](tutorial/concurrency.md).

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

Typparameter stehen in spitzen Klammern hinter dem Namen eines Typs oder einer Funktion.
Trait-Schranken stehen in einer `:where`-Klausel. Einzelheiten stehen unter
[Grundlagen der Typen](tutorial/types.md) und [Traits](tutorial/traits.md).

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

### Option und Result statt nil

Es gibt kein `nil`. Eine Funktion, die eventuell keinen Wert hat, gibt `Option<T>` zurück, eine, die
fehlschlagen kann, `Result<T,E>`; den Inhalt holt man mit `match` heraus. Wer ein `Option<string>`
als `string` verwendet oder in einem `match` den Fall `none` vergisst, erhält schon vor der
Ausführung einen Typfehler. Einzelheiten stehen unter [Grundlagen der Typen](tutorial/types.md) und
[Fehlerbehandlung](tutorial/errors.md).

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

### Verketten mit `->`

Das Makro `->` übergibt einen Wert der Reihe nach als erstes Argument an die folgenden Ausdrücke.
`(-> x (f a) (g b))` ist dasselbe wie `(g (f x a) b)`. So lassen sich Schritte, die `Option` oder
`Result` zurückgeben, von oben nach unten schreiben. Einzelheiten stehen unter
[Option, Result und Fehlertypen](reference/functions/option-result.md).

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

### Typgeprüfte Makros

Makros schreibt man wie in Common Lisp mit `defmacro`, Quasiquote und `gensym`. Der expandierte Code
wird wie von Hand geschriebener Code typgeprüft; passt er nicht zu den Typen, wird der Fehler an der
Stelle gemeldet, an der das Makro aufgerufen wurde. Einzelheiten stehen unter
[Makros](tutorial/macros.md).

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

Wer typelisp zum ersten Mal begegnet, liest am besten in dieser Reihenfolge.

- [Erste Schritte](tutorial/intro.md): die REPL, Funktionen, Variablen, Verzweigungen, Schleifen, Listen und `Vector`
- [Grundlagen der Typen](tutorial/types.md): statische Typen, `Option`, `Result`, Strukturen, Aufzählungen, Generics
- [Traits](tutorial/traits.md): `deftrait` / `impl`, Trait-Schranken, `:dyn`
- [Makros](tutorial/macros.md): `defmacro`, Quasiquote, `gensym`, `macrolet`
- [Fehlerbehandlung](tutorial/errors.md): `Result`, `panic`, `catch` / `throw`, `unwind-protect`
- [Nebenläufigkeit](tutorial/concurrency.md): Tasks, Kanäle, `select`, `Mutex`, `thread`

## Leitfäden

- [Module und Dateiaufteilung](guide/modules.md): `use`, `pub`, wie Dateien den Modulen entsprechen
- [Kompilierung](guide/compile.md): der JIT, Programme mit AOT-Kompilierung erstellen, Dumps
- [Datei-E/A, Streams und Netzwerk](guide/io.md): Dateien, Pfadnamen, TCP / TLS / UDP, Namensauflösung
- [C-FFI](guide/ffi.md): C-Funktionen mit `defffi` aufrufen (einschließlich Callbacks und C-Structs mit `def-c-struct`)
- [Editor-Anbindung](guide/editors.md): `typl-lsp` und die Einrichtung von VS Code / Emacs
- [Für Common-Lisp-Programmierer](guide/from-common-lisp.md): wie sich typelisp von CL unterscheidet und wie man CL-Code umschreibt

## Referenz

- [Syntaxreferenz](reference/syntax.md): Lexik, Typschreibweise, Definitionen, Steuerformen, Kompilierung, Nebenläufigkeit
- [Eingebaute Funktionen](reference/functions/README.md): eingebaute Funktionen, Methoden und die Standardbibliothek
- [Typen](reference/types.md): die Typen und die Traits, die jeder von ihnen implementiert
- [Fehlermeldungen](reference/errors.md): was die häufigen Fehler bedeuten und wie man sie behebt

## Editor-Anbindung

Die Schritte zur Einrichtung stehen im [Leitfaden zur Editor-Anbindung](guide/editors.md). Die
Tastenbelegungen und Einstellungen der einzelnen Editoren sind in diesen Dokumenten aufgeführt:

- [Emacs (typelisp-mode)](../../editor/emacs/README_de.md)
- [VS Code](../../editor/vscode/README_de.md)
