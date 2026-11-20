<!-- translated-from: docs/ja/guide/compile.md @ eb0e7d31e1d6e61f64d90fcfb4f0dc5e6bfe813f -->
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
(use std::env::command-line-args)
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
args: #(./hello a b)
fib(25) = 75025
```

Ohne `-o` erhält das Programm den Namen der Quelldatei ohne `.typl` und landet im selben Ordner wie die
Quelldatei. Endet der Name der Quelldatei nicht auf `.typl`, ist `-o` Pflicht. Zusammen mit `-c` (`--compile`)
lassen sich `--image` und `--feature` nicht angeben.

Mit `--release` erzeugt `typl` einen **Release-Build**, der `eprint`/`eprintln` (Ausgaben zur Untersuchung auf der Standardfehlerausgabe; [Ausgabe](../reference/functions/printing.md)) samt Argumenten entfernt. Was entfernt wird, wird trotzdem typgeprüft; ein Typfehler darin lässt die Kompilierung also fehlschlagen. `--release` ist eine Option von `typl` als Ganzem: Sie wirkt nicht nur bei `-c`, sondern ebenso beim Ausführen einer Datei, in der REPL und bei `typl test`. Auch ein `compile-file`, das in einem mit `--release` gestarteten `typl` aufgerufen wird, erzeugt einen Release-Build, und auch aus Code, der in einer so erzeugten ausführbaren Datei mit `eval` ausgeführt wird, werden `eprint`/`eprintln` entfernt.

```sh
$ typl --release -c hello.typl
$ typl -c hello.typl --release     # nach -c geschrieben ist es dasselbe
```

`-O0`, `-O1`, `-O2` und `-O3` legen die **Optimierungsstufe** fest. Voreingestellt ist `-O0`, das nicht optimiert. Ab `-O1` führt der Compiler zuerst einfache Optimierungen durch, etwa eine Variable, die nur an ein Literal gebunden ist, durch ihren Wert zu ersetzen oder Ganzzahlarithmetik auf Konstanten im Voraus zu berechnen, und überlässt den Rest der Optimierung von LLVM auf der gleichnamigen Stufe. Auf jeder Stufe sind die Ergebnisse der Berechnung dieselben. Wie `--release` ist es eine Option von `typl` als Ganzem: Außer für die ausführbare Datei, die `-c` erzeugt, wirkt sie auf den Code, der beim Ausführen einer Datei, in der REPL und in `typl test` kompiliert wird, sowie auf ein dort aufgerufenes `compile-file`. Code, den der Interpreter ausführt, wird nicht optimiert.

```sh
$ typl -O2 -c hello.typl
$ typl -c hello.typl -O2     # nach -c geschrieben ist es dasselbe
```

Dasselbe lässt sich erreichen, indem man `compile-file` aus der REPL oder aus einem Programm aufruft.

```sh
$ typl
typl> (compile-file "hello.typl" "hello")
typl> :quit
$ ./hello a b
args: #(./hello a b)
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

- Die oberste Ebene der Einstiegsdatei darf nur Definitionen (`defun` `defvar`
  `defparameter` `defconstant` `defmacro` `defsignature` `defstruct` `defenum` `deftype` `deftrait`
  `impl` `defffi`, `(unsafe (def-c-struct ...))`) sowie `use` `module` enthalten. Ausdrücke auf oberster
  Ebene wie `(println ...)` sind nicht erlaubt, abgesehen vom abschließenden `(main)`. Die Arbeit gehört
  in `main`.
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

### 3.5 Ressourcen des Programms

`typl -c` hat Optionen, die festlegen, welche Ressourcen das Programm während des Laufs verwenden darf.

```sh
$ typl --heap-cells 100000 -c hello.typl    # die cons-Arena auf 100000 Zellen festlegen
$ typl -c hello.typl --max-memory 64M       # höchstens 64 MiB belegen
$ typl -c hello.typl --no-cons              # keine cons-Zellen verwenden
```

- `--heap-cells N` legt die Kapazität der Arena der cons-Zellen auf `N` Zellen fest. Die Arena wächst nicht darüber hinaus: Kann die Garbage Collection keine Zelle freigeben, endet das Programm mit einem Laufzeitfehler. Ohne die Option beginnt die Arena mit 65536 Zellen und wächst nach Bedarf.
- `--max-memory SIZE` begrenzt den gesamten Speicher, den das Programm belegt. Alles zählt: die cons-Arena, Zeichenketten, Werte wie Vektoren und Hashtabellen und der Speicher der Laufzeit selbst. `SIZE` ist eine Anzahl Bytes, gefolgt optional von `K`, `M` oder `G` (das 1024-, 1024²- und 1024³-fache). Eine Belegung über die Grenze hinaus wird gemeldet, und das Programm endet mit dem Exit-Code 1. Dieses Ende lässt sich nicht mit `catch` abfangen. Ohne die Option gibt es keine Grenze.
- `--no-cons` erzeugt ein Programm, das keine cons-Zellen verwendet. Es hat keine cons-Arena; ein Programm, das etwas tut, wofür cons-Zellen nötig sind, ist daher ein **Kompilierfehler**, der diese Operation und die Kette der Funktionen von `main` bis dorthin zeigt. Cons-Zellen braucht man, um eine `Sexpr`-Liste (auch eine quotierte oder per Quasiquote gebaute) oder ein `&rest`-Argument zu bauen, sowie für `read`, `eval` und `macroexpand`. Operationen auf Zahlen, Zeichenketten, Vektoren, Hashtabellen, Strukturen, Kanälen und Ähnlichem sowie `print`/`println`/`format` brauchen keine. Lässt sich nicht zusammen mit `--heap-cells` angeben und steht nach `-c`.

`--max-memory` steht nach `-c`. `--heap-cells` darf vor oder nach `-c` stehen und gibt dann die Kapazität der cons-Arena des Programms an. Die cons-Arena zählt zur Speichergrenze; bei einer Grenze, in die die Arena nicht passt, endet das Programm, bevor es beginnt.

### 3.6 Für WebAssembly kompilieren

Mit `--target wasm32-wasip1` erzeugt `typl -c` statt eines ausführbaren Programms ein WebAssembly-Modul (ein WASI-preview-1-Kommando). Ohne `-o` heißt es wie die Quelle, mit `.wasm` statt `.typl`.

```sh
$ typl -c hello.typl --target wasm32-wasip1      # erzeugt hello.wasm
$ wasmtime hello.wasm a b                        # mit einer WASI-fähigen Laufzeitumgebung ausführen
args: #(hello.wasm a b)
fib(25) = 75025
```

- Ein mit `install.sh` installiertes `typl` lässt sich so verwenden. Eines, das mit `cargo install` installiert wird, braucht Vorbereitung (die auch Abschnitt 3.7 abdeckt). Führen Sie zuerst mit rustup `rustup target add wasm32-wasip1 wasm32-unknown-unknown` aus. Installieren Sie es dann mit `--features wasm` (`cargo install --locked --path . --features wasm`).
  Ein ohne es installiertes `typl` hat keine WebAssembly-Laufzeit, und `--target` ist ein Fehler. Für einen Debug-Build von `typl` im Repository führen Sie stattdessen im Repository `scripts/build-wasm-runtime.sh` aus.
- Gelinkt wird mit dem Linker der rustup-Toolchain (`rust-lld`); rustup wird also auch beim Kompilieren gebraucht.
- Das Modul verhält sich wie das ausführbare Programm: Standardausgabe, Standardfehler und Exit-Code, `(command-line-args)`,
  Dateien lesen und schreiben, `throw`/`catch`/`panic`, Tasks, Kanäle und `sleep` sowie `eval` funktionieren.
  Tasks laufen nacheinander in einem einzigen Thread. Dateien lassen sich nur in den Verzeichnissen lesen und
  schreiben, die die Laufzeitumgebung dem Modul zeigt (bei wasmtime mit `--dir` angegeben).
- Ein Programm, das etwas benutzt, das WebAssembly nicht hat, ist ein **Kompilierfehler**. Der Fehler zeigt, was benutzt wurde,
  und die Kette der Funktionen von `main` bis dorthin. Nicht verfügbar sind: Sockets (TCP, UDP, Unix-Domain,
  TLS, Namensauflösung), OS-Threads (`Thread`), C-Funktionen (`defffi`), `get-internal-run-time`, `machine-instance`, `software-version`, `file-author`,
  die lokale Zeitzone und `ed`.
- `--heap-cells`, `--max-memory`, `--no-cons`, `-O<N>` und `--release` wirken wie bei einem ausführbaren Programm.

### 3.7 Im Browser oder in Node.js ausführen

Mit `--target wasm32-unknown-unknown` erzeugt `typl -c` ein WebAssembly-Modul, das aus JavaScript geladen und ausgeführt wird. Neben dem Modul (`hello.wasm`) entsteht im selben Ordner das JavaScript, das es lädt und ausführt (`hello.mjs`). Die Vorbereitung ist dieselbe wie in Abschnitt 3.6.

```sh
$ typl -c hello.typl --target wasm32-unknown-unknown   # erzeugt hello.wasm und hello.mjs
$ node hello.mjs                                       # mit Node.js ausführen
args: #()
fib(25) = 75025
```

Aus anderem JavaScript rufen Sie `run` aus `hello.mjs` auf. `write` erhält die Ausgabe als Text (`fd` ist 1 für die Standardausgabe, 2 für den Standardfehler). Der Wert von `run` ist der Exit-Code.

```js
import { run } from './hello.mjs';
const code = await run({ write: (fd, text) => { /* zum Beispiel auf der Seite anzeigen */ } });
```

- Ohne `write` schreibt Node.js auf Standardausgabe und Standardfehler, ein Browser zeilenweise auf die Konsole.
- Im Browser liefern Sie `hello.wasm` per HTTP vom selben Ort wie `hello.mjs` aus (von `file://` lässt es sich nicht laden).
- `sleep` wartet mit `Atomics.wait`. Ein Browser erlaubt das nur in einem Worker einer cross-origin-isolierten Seite; führen Sie ein Programm, das `sleep` benutzt, daher in einem Worker aus.
- Zusätzlich zu dem, was in Abschnitt 3.6 nicht verfügbar ist, sind Datei- und Verzeichnisoperationen ein **Kompilierfehler**. Die Standardeingabe liest sich als leer, und `(command-line-args)` ist ein leerer Vektor.

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
