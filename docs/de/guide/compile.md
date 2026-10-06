<!-- translated-from: docs/ja/guide/compile.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Kompilierung

Ohne weiteres Zutun laufen typelisp-Programme im Interpreter. Darüber hinaus gibt es zwei Wege, in nativen
Code zu kompilieren, und einen Weg, eine Umgebung zu speichern. Die Einzelheiten der Spezifikation stehen in
[Kapitel 10 der Syntaxreferenz](../reference/syntax.md#10-kompilierung).

| Verfahren | Wie | Ergebnis |
|---|---|---|
| JIT-Kompilierung | `(compile name)` | Eine Funktion der laufenden Sitzung wird durch nativen Code ersetzt |
| AOT-Kompilierung | `typl -c src.typl` oder `(compile-file "src.typl" "out")` | Ein eigenständiges ausführbares Programm |
| Dump | `(dump "file.typld")` | Speichert die Definitionen; `typl --image` startet wieder mit derselben Umgebung |

## 1. Vorbereitung

Die Kompilierung verwendet LLVM 22. Wer `typl` gemäß der [README.md](../../../README.md) gebaut hat, braucht
keine weitere Vorbereitung.

Programme, die durch AOT-Kompilierung entstehen, werden gegen die statische Bibliothek `libtypelisp_front.a`
gelinkt. Ein Release-Build von `typl` (auch einer, der mit `cargo install` installiert wurde) trägt diese
Bibliothek in sich, daher ist keine Vorbereitung nötig. Beim ersten Kompilieren schreibt er die Bibliothek nach
`~/.typelisp/lib/<Build-ID>/` (oder nach `$TYPELISP_HOME/lib/<Build-ID>/`, wenn die Umgebungsvariable
`TYPELISP_HOME` gesetzt ist) und verwendet von da an diese Kopie. `typl --remove-lib` löscht sie (mit
`--others` die von anderen Versionen von `typl` geschriebenen, mit `--all` alle). Ein Debug-Build von `typl`
verwendet die Bibliothek in `target/debug/` des Repositorys, in dem er gebaut wurde. Um eine an anderer Stelle
abgelegte zu verwenden, gibt man beim Start von `typl` deren Ordner mit `--lib-dir` an (Abschnitt 3.2).
Unter macOS verwendet das Linken die Xcode Command Line Tools.

## 2. JIT-Kompilierung

Wandelt eine bereits definierte Funktion an Ort und Stelle in nativen Code um.

```lisp
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(compile fib)
(fib 30)           ; ab hier führen Aufrufe den kompilierten Code aus
```

- `name` wird nicht ausgewertet. Man schreibt den Funktionsnamen so, wie er ist (nicht als Zeichenkette). Eine
  Methode schreibt man mit dem Typnamen, wie in `(compile point::norm)`.
- Funktionen, die sie aufruft, werden mitkompiliert.
- **Generische Funktionen lassen sich nicht kompilieren.** An jeder Stelle, an der sie verwendet werden, wird
  eine Kopie für jeden Typ erzeugt. Stattdessen kompiliert man die Funktion, die sie mit konkreten Typen
  aufruft.
- `trace`, `step`, `disassemble`, `compile`, `compile-file` und `dump` sind Operationen des Interpreters, daher
  lässt sich eine Funktion, die sie aufruft, nicht kompilieren. Der Versuch ergibt einen Fehler, der den Grund
  nennt.

Um das Ergebnis der Kompilierung anzusehen, verwendet man `disassemble`.

```lisp
(disassemble fib)          ; der Maschinencode des Hosts
(disassemble fib true)     ; LLVM IR
```

## 3. Ein Programm mit AOT-Kompilierung erstellen

### 3.1 Das Programm schreiben

Als Einstiegspunkt definiert man eine **Funktion `main` ohne Argumente**.

```lisp
;; hello.typl
(defun fib ((n int)) int
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(defun main () ()
  (println "args: ~a" (command-line-args))
  (println "fib(25) = ~a" (fib 25)))

(main)
```

Das `(main)` am Ende der Datei sorgt dafür, dass `main` aufgerufen wird, wenn man `typl hello.typl`
ausführt. `compile-file` überspringt dieses abschließende `(main)`, daher funktioniert dieselbe Datei sowohl
im Interpreter als auch mit der AOT-Kompilierung.

### 3.2 Kompilieren

Auf der Kommandozeile verwendet man `typl -c` (`typl --compile` ist dasselbe).

```sh
$ typl -c hello.typl            # erzeugt hello
$ typl -c hello.typl -o fib     # nennt das Programm fib
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Ohne `-o` erhält das Programm den Namen der Quelldatei ohne `.typl` und landet im selben Ordner wie die
Quelldatei. Endet der Name der Quelldatei nicht auf `.typl`, ist `-o` Pflicht. Zusammen mit `-c` (`--compile`)
lassen sich `--image`, `--heap-cells` und `--feature` nicht angeben.

Dasselbe lässt sich erreichen, indem man `compile-file` aus der REPL oder aus einem Programm aufruft.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #<vector<string> ./hello a b>
fib(25) = 75025
```

Wer häufig baut, kann diese eine Zeile in eine Datei schreiben und sie mit `typl build.typl` ausführen.

```lisp
;; build.typl
(compile-file "hello.typl" "hello")
```

Dateinamen werden relativ zum **aktuellen Verzeichnis, in dem `typl` gestartet wurde**, aufgelöst, nicht
relativ zum Ort von `build.typl`.

Um eine `libtypelisp_front.a` zu linken, die an einer anderen Stelle liegt als dort, wo `typl` sucht, gibt man
deren Ordner mit `--lib-dir` an. Das gilt sowohl für `typl -c` als auch für `compile-file`.

```sh
$ typl --lib-dir ~/lib/typelisp -c hello.typl
$ typl --lib-dir ~/lib/typelisp build.typl
```

Enthält der angegebene Ordner keine `libtypelisp_front.a`, hält `typl` mit einem Fehler an. Die Datei
funktioniert nur mit dem `typl`, das zusammen mit ihr gebaut wurde. Nach einem Neubau von `typl` kopiert man
sie erneut.

### 3.3 Was eine AOT-kompilierte Datei enthalten darf

- Die oberste Ebene der Einstiegsdatei darf nur Definitionen (`defun` `defmethod` `defvar` `defconstant`
  `defstruct` `defenum` `deftype` `deftrait` `impl` `defffi`, `(unsafe (def-c-struct ...))`) sowie `use`
  `module` enthalten. Ausdrücke auf oberster Ebene wie `(println ...)` sind nicht erlaubt, abgesehen vom
  abschließenden `(main)`. Die Arbeit gehört in `main`.
- `defmacro` darf nicht in der Einstiegsdatei stehen. Makros definiert man in einem anderen Modul mit
  `(pub defmacro ...)` und holt sie mit `use` herein.
- Eine Datei, die `defsignature` enthält, lässt sich nicht AOT-kompilieren, gleich ob es die Einstiegsdatei
  oder ein mit `use` hereingeholtes Modul ist.
- Ohne ein `main` ohne Argumente schlägt die Kompilierung mit einem Fehler fehl.
- Die Dateien der mit `use` hereingeholten Module werden ebenfalls kompiliert und zu einem einzigen Programm
  zusammengefügt.
- Bibliotheken, die in `defffi` mit `:library` angegeben sind, werden automatisch gelinkt ([C-FFI](ffi.md)).
- Jede Funktion der Standardbibliothek lässt sich mit der AOT-Kompilierung verwenden. Auch `eval` ist
  verwendbar, dann kommen aber Typprüfung und Interpreter ins Programm, was es größer und langsamer beim Start
  macht. Programme, die `eval` nicht aufrufen, enthalten sie nicht.

### 3.4 Wie sich das Programm verhält

- `(command-line-args)` gibt einen `Vector<string>` derselben Gestalt zurück, egal ob als
  `typl hello.typl a b` oder als `./hello a b` ausgeführt. Das erste Element ist der Programmname.
- Den Exit-Code setzt man mit `(exit n)`. Kehrt `main` normal zurück, ist er 0.
- Bei einem `panic` gibt das Programm die Meldung aus und endet mit einem Code ungleich null.

## 4. Dumps

Man kann die Definitionen der aktuellen Sitzung in eine Datei speichern und beim nächsten Mal von dort aus
starten.

```sh
$ typl
typl> (defun sq ((n i32)) i32 (* n n))
typl> (compile sq)
true
typl> (dump "session.typld")
true
typl> :quit
$ typl --image session.typld
typl> (sq 9)
81
```

Das funktioniert auch beim Ausführen einer Datei, wie in `typl --image session.typld prog.typl`.

- Gespeichert werden die **Definitionen**. In der Sitzung ausgewertete Ausdrücke werden nicht gespeichert.
- Mit `compile` kompilierte Funktionen werden in kompilierter Form gespeichert.
- Globale Variablen werden wiederhergestellt, indem **ihre Initialisierer erneut ausgeführt werden**, nicht mit
  den Werten, die sie beim Schreiben des Dumps hatten.
- Ein Dump lässt sich nicht von einem `typl` einer anderen Version laden als der, die ihn geschrieben hat (das
  ist ein Fehler).

Führt man eine Datei aus und ruft darin `(dump ...)` auf, stehen die Definitionen dieser Datei in einem Modul,
das nach der Datei benannt ist. Eine in `dp.typl` definierte Funktion heißt `dp::sq`, und um sie aus einer
anderen Datei aufzurufen, braucht es `pub` ([Module und Dateiaufteilung](modules.md)).

## 5. Zu kompilierten Moduldateien

Es gibt kein Format wie `.fasl` in Common Lisp, um das Kompilat jedes Moduls in eine Datei zu schreiben.
`compile-file` baut das Programm direkt aus den Quellen. Es bleiben keine Zwischendateien zurück.
