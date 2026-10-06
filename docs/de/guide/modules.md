<!-- translated-from: docs/ja/guide/modules.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Module und Dateiaufteilung

Dieser Leitfaden erklärt, wie man ein Programm aus mehreren Dateien zusammenstellt. Die genauen Regeln stehen
in den Abschnitten 3.10 bis 3.13 der [Syntaxreferenz](../reference/syntax.md#310-module--use--namensräume).

## 1. Eine Datei ist ein Modul

In typelisp ist **eine Datei für sich ein Modul**. Der Pfad der Datei relativ zur Quellwurzel ist der Pfad des
Moduls.

| Datei | Modul |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Eine Moduldeklaration am Anfang der Datei ist nicht nötig.

## 2. Ein Projekt einrichten

Man legt eine Datei namens `typelisp.toml` in die Wurzel des Projekts. Sie darf leer sein.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Um die Quellen unter `src/` abzulegen, schreibt man diese eine Zeile in `typelisp.toml`:

```toml
src = "src"
```

`typl` sucht `typelisp.toml` ausgehend vom Verzeichnis der ausgeführten Datei nach oben und verwendet den
Fundort als Quellwurzel. Wird keine gefunden, ist das Verzeichnis der ausgeführten Datei die Wurzel (in der
REPL das aktuelle Verzeichnis).

## 3. Definitionen öffentlich machen und verwenden

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; ein Feld ohne pub kann von außen nicht gelesen werden

(defun square ((n i32)) i32 (* n n))   ; eine Funktion ohne pub kann von außen ebenfalls nicht aufgerufen werden

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl` wird an der Stelle geladen, an der `(use geometry)` steht. Es muss nicht vorher geladen
werden.

### Was öffentlich wird

- Funktionen, Strukturen, Aufzählungen, globale Variablen, Makros und Methoden sind nur dann aus anderen
  Modulen sichtbar, wenn sie `pub` tragen. `pub` steht direkt vor der Definition, wie in `(pub defun ...)`.
- Bei Strukturen sind **das Öffentlichmachen des Typs und das Öffentlichmachen der Felder getrennte Dinge**.
  `(pub defstruct point ...)` macht den Typ sichtbar, und nur Felder, die als `(pub x i32)` geschrieben sind,
  lassen sich von außen lesen und schreiben.
- Wer von außen einen nicht öffentlichen Namen verwendet, bekommt einen Fehler „nicht auflösbar“ wie
  `unresolved path: geometry::square`. Es ist dieselbe Meldung wie bei einem Tippfehler; stimmt die
  Schreibweise und wird der Name trotzdem nicht aufgelöst, fehlt vermutlich ein `pub`.

Die Liste der Definitionen, die `pub` tragen können, steht in
[Syntaxreferenz 3.13](../reference/syntax.md#313-pub--sichtbarkeit).

## 4. Wie man `use` schreibt

```lisp
(use geometry)              ; ein Modul hereinholen; zum Verwenden schreibt man geometry::dist2
(use geometry::dist2)       ; eine Funktion hereinholen; man verwendet sie mit dem einfachen Namen dist2
(use geometry::point)       ; einen Typ hereinholen; point::new, point::origin und point in Typangaben
(use a::f b::g)             ; mehrere lassen sich zusammen schreiben
```

- **`use` wirkt nur auf die Formen danach.** Es gehört an den Anfang der Datei. Steht `geometry::dist2` über
  dem `use`, ergibt das `unresolved path`.
- Den vollen Pfad `geometry::dist2` zu schreiben, ohne das Modul mit `use` hereinzuholen, wird ebenfalls nicht
  aufgelöst. Nur `use` sorgt dafür, dass eine Datei geladen wird.
- Ein `use` eines Namens, dessen einfache Form bereits belegt ist, erzeugt eine Warnung. Will man ihn trotzdem
  hereinholen, verwendet man `shadowing-import`.
- Ein Modul in einem Verzeichnis schreibt man `(use geo::shapes)`, und danach wird es mit seinem letzten Teil
  angesprochen (`shapes::...`).

### Trait-Methoden aufrufen

Methoden, die in einem `impl` implementiert sind, **gehören zum Typ**, nicht zu den Funktionen des Moduls,
und werden daher ohne den Modulnamen aufgerufen.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, nicht core::area
```

Methoden innerhalb eines `impl` sind immer öffentlich, auch ohne `pub`.

Ein Trait selbst lässt sich nicht für andere Module öffentlich machen. Die Definition eines Traits, seine
`impl`s und der Code, der ihn über `:dyn` verwendet, gehören in ein einziges Modul.

## 5. Namensräume innerhalb einer Datei aufteilen

Um einen Namensraum innerhalb einer Datei weiter aufzuteilen, verwendet man `module`. Es wird im eigenen
Modul der Datei verschachtelt.

```lisp
;; innerhalb von main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Um den ganzen Rest der Datei in einen Namensraum zu stellen, kann man statt einer Klammerung auch
`(in-module util)` schreiben.

## 6. Einschränkungen bei Abhängigkeiten

- **Zyklen sind nicht erlaubt.** Wenn `a.typl` `(use b)` und `b.typl` `(use a)` ausführt, ist das Ergebnis der
  Fehler `circular module dependency: a -> b -> a`. Die Definitionen, die beide brauchen, verschiebt man in ein
  drittes Modul.
- **Weder Typen noch Funktionen können vor ihrer Definition verwendet werden**, auch nicht innerhalb derselben
  Datei. Bei sich gegenseitig rekursiven Funktionen deklariert man eine davon zuerst mit `defsignature`
  ([Syntaxreferenz 3.2](../reference/syntax.md#32-defsignature--vorwärtsdeklarationen)).

## 7. Reihenfolge der Ausführung

`typl main.typl` läuft in dieser Reihenfolge ab:

1. `main.typl` und alle Dateien, die von dort mit `use` hereingeholt werden, werden gelesen und typgeprüft.
   **Gibt es irgendwo einen Typfehler, wird nichts ausgeführt.**
2. Die Ausdrücke auf oberster Ebene der hereingeholten Module laufen vor denen der Module, die sie verwenden.
3. Die Ausdrücke auf oberster Ebene von `main.typl` laufen von oben nach unten.

Fasst man den Einstiegspunkt des Programms in einer Funktion `main` zusammen und ruft am Ende der Datei
`(main)` auf, lässt sich dieselbe Datei auch für die
[AOT-Kompilierung](compile.md#3-ein-programm-mit-aot-kompilierung-erstellen) verwenden.

## 8. Wie sich das von `load` unterscheidet

`(load "Pfad")` liest wie `load` in Common Lisp den Inhalt einer Datei **unverändert in den aktuellen
Namensraum** ein. Er wird nicht in ein Modul gehüllt, und `pub` spielt keine Rolle. Man verwendet es etwa, um
eine Einstellungsdatei zu lesen oder eine lokale Datei in der REPL neu zu laden. Um ein Programm in Teile zu
zerlegen, verwendet man `use`.
