<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Streams und Dateien

Stream-Traits und -Methoden, konkrete Stream-Typen, Dateioperationen und Pfadnamen. Netzwerk-Sockets sind
ebenfalls Streams und werden unter [Netzwerk](network.md) behandelt.

## 1. Die Trait-Hierarchie

Was CL mit einer Klassenhierarchie ausdrückt, wird hier mit einer **Trait-Hierarchie** ausgedrückt. Sowohl die
Richtung (Eingabe / Ausgabe) als auch der Elementtyp werden **statisch** bestimmt, daher muss man zur Laufzeit
nicht fragen „lässt sich aus diesem Stream lesen?“.

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; Zeicheneingabe
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; Zeichenausgabe
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; Eingabe, die ein Zeichen zurückstellen kann
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; Byteeingabe
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; Byteausgabe
```

Eine Funktion, die Zeichen liest, akzeptiert jeden Stream-Typ, eingebaut oder benutzerdefiniert, wenn sie
`(where (CharInput S))` oder `:dyn CharInput` nimmt.

## 2. Methoden

Jede Methode von `CharInput` hat eine Standardimplementierung. Eine Implementierung schreibt nur `read-item`.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | Das nächste Element. `none` am Ende. **Die einzige Methode, die implementiert werden muss** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Das nächste Zeichen |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Bis zum nächsten Zeilenumbruch (der Zeilenumbruch wird verbraucht und entfernt). Auch eine letzte Zeile, die nicht mit einem Zeilenumbruch endet, wird zurückgegeben |
| `read-all` | `(read-all s)` | `(S)→string` | Alles, was übrig ist |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Nur ein Zeichen, das bereits vorliegt. Lieber `none` als warten |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Legt bis zu `n` Zeichen auf `v` und gibt zurück, wie viele tatsächlich gelesen wurden. Weniger als `n` nur am Ende |

`listen` liegt in `InputStream` (dem Eltern-Trait von `CharInput`):

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Ob das nächste Lesen ohne Warten beantwortet werden kann. Der Standard ist `false`, **die Seite, die nie lügt**: `true` wäre eine Vermutung, und eine falsche Vermutung würde `read-char-no-hang` blockieren lassen. Alle eingebauten Streams überschreiben es. **Bei benutzerdefinierten Streams, die es nicht überschreiben, gibt `read-char-no-hang` immer `none` zurück** |

`PeekInput` (erbt von `CharInput`) fügt **das Zurückstellen eines Zeichens** hinzu. Nur der Stream selbst hat
einen Ort, an dem er das zurückgestellte Zeichen aufbewahren kann, daher kann dies keine
Standardimplementierung haben und ist ein eigener Trait. `file-stream`/`string-input-stream`/`standard-stream`
implementieren ihn, und jeder andere Stream bekommt ihn, wenn man ihn mit `make-peek-stream` umhüllt
(Kapitel 4).

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Lässt das nächste Lesen `c` zurückgeben. **Die einzige Methode, die implementiert werden muss**. Wie in CL ist nur ein Zeichen garantiert |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Schaut das nächste Zeichen an, ohne es zu verbrauchen |

Ebenso schreibt eine Implementierung für `CharOutput` nur `write-item`.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Schreibt ein Element. **Die einzige Methode, die implementiert werden muss** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Schreibt ein Zeichen |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Schreibt eine Zeichenkette |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Eine Zeichenkette und ein Zeilenumbruch |
| `terpri` | `(terpri s)` | `(S)→()` | Ein Zeilenumbruch (CLs Name) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Ein Zeilenumbruch, außer am Zeilenanfang |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Ob das nächste geschriebene Zeichen eine Zeile beginnt. Der Standard ist `false` (daher schreibt `fresh-line` den Zeilenumbruch: Im Zweifel ist Schreiben die sichere Seite). Alle eingebauten Streams überschreiben es |
| `finish-output` | `(finish-output s)` | `(S)→()` | Leert den Puffer |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Schreibt alle Zeichen von `v` der Reihe nach |

`at-line-start` merkt sich **nur, was durch diesen Stream geschrieben wurde**. `print`/`println`/
`(format true ...)` schreiben auf die Standardausgabe, ohne über `*standard-output*` zu gehen; mischt man die
beiden, weiß `(fresh-line *standard-output*)` daher nichts von den Zeilenumbrüchen, die `println` geschrieben
hat. Man bleibt bei einem von beiden.

`Stream` ist allen Streams gemeinsam:

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Ob er noch offen ist |
| `close` | `(close s)` | `(S)→()` | Schließt ihn. **Der GC schließt keine Streams**, daher tut man es ausdrücklich (oder mit `with-open-file`) |

## 3. Konkrete Stream-Typen

| Typ | Wie man einen erzeugt | Implementierte Traits |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` ist eine der drei Konstanten `direction-input` / `direction-output` / `direction-append`.
`open-file` gibt `Err(FileError)` zurück, wenn sich die Datei nicht öffnen lässt (eine fehlende Datei ist ein
gewöhnliches Ergebnis, kein Panic). Der Dateiname kann eine Zeichenkette oder ein `pathname` sein (`Pathish`
in Kapitel 9).

`(get-output-stream-string s)` gibt zurück, was in einen `string-output-stream` geschrieben wurde, und leert
ihn. Wie in CL lässt es sich auch nach `close` noch herausholen.

**Byte-E/A** verwendet `ByteInput`/`ByteOutput`. Diese legen das `Item` von `InputStream`/`OutputStream` auf
`int` fest, so wie `CharInput`/`CharOutput` es auf `char` festlegen.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | Das nächste Byte. `none` am Dateiende |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Schreibt ein Byte. Ein Fehler außerhalb von 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | Die Zeichenfassung, in Bytes |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Wie oben |

CL bestimmt den Elementtyp **im Aufruf**, wie in `(open name :element-type '(unsigned-byte 8))`, aber hier ist
der Elementtyp **der Typ** des Streams, daher unterscheidet sich die Funktion, die ihn öffnet. Bytes aus einem
Zeichen-Stream zu lesen, ist ein Typfehler (`string-input-stream` implementiert `ByteInput` nicht). Ein Byte
direkt nach dem Zurückstellen eines Zeichens mit `unread-char` zu lesen, ist ebenfalls ein Fehler.

## 4. Zusammengesetzte Streams

Alle sind `defstruct`s der Standardbibliothek und lassen sich verschachteln.

| Name | Form | Beschreibung |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Schreibt in alle eines `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Liest aus `in` und schreibt in `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Liest aus `in` und schreibt die gelesenen Zeichen zusätzlich in `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Liest einen `Vector<:dyn CharInput>` nacheinander |
| `make-peek-stream` | `(make-peek-stream in)` | Fügt einem beliebigen `:dyn CharInput` ein Zurückstellen von einem Zeichen hinzu und macht ihn zu einem `PeekInput` (für `read-sexpr`) |

## 5. Makros

| Name | Form | Beschreibung |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Öffnen, Rumpf ausführen, schließen. `Result<Wert des Rumpfes, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Liest aus einer Zeichenkette |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Gibt zurück, was geschrieben wurde |

## 6. Generische Funktionen und Dateioperationen

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Überträgt alles |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Alle verbleibenden Zeilen |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Liest ein `Sexpr` (CLs `read`). `Ok(eof)` am Ende der Eingabe, `Ok(datum d)`, wenn eines gelesen wurde, `Err`, wenn es keine Daten sind. Es **verbraucht das eine Leerraumzeichen**, das das Datum beendet hat (wie in CL). `ReadOutcome` ist kein `Option<Sexpr>`, damit das Lesen der leeren Liste `()` und das Ende der Eingabe nicht derselbe Wert sind |
| `read-sexpr-preserving-whitespace` | Wie oben | Wie oben | Dasselbe, lässt aber den Leerraum stehen (CLs `read-preserving-whitespace`) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Liest bis `ch` und bildet eine Liste. `ch` wird verbraucht. `Err`, wenn die Eingabe zu Ende geht |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Schreibt Zeile für Zeile |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | Der ganze Inhalt |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Alle Zeilen |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Schreibt ihn hinaus |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Ob sie existiert |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Löschen, umbenennen (Argumente sind `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Der absolute Pfad mit aufgelösten symbolischen Links und `.`/`..`. `Err`, wenn er nicht existiert |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | Der Zeitpunkt der letzten Änderung. Es ist **Universalzeit**, daher kann `decode-universal-time` ([Zeit](system.md#2-datumsangaben-zerlegen-und-zusammensetzen)) sie lesen |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | Der Anmeldename des Besitzers. `Err`, wenn die Datei nicht existiert, `Ok(none)`, wenn die uid des Besitzers keinen Eintrag in der Passwortdatenbank hat: Die beiden Fälle, die CL unterscheidet, bleiben getrennt |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Ob es ein Verzeichnis ist. **Auch `false`, wenn es nicht existiert**; um beides zu unterscheiden, verwendet man `probe-file` |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Listet den Inhalt nach truename auf (der absolute Pfad mit aufgelösten symbolischen Links, wie bei `truename`). Symbolische Links, deren Ziel fehlt, werden ausgelassen. `.`/`..` werden ausgelassen. Die Reihenfolge ist die, die das BS liefert |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Legt es samt Eltern an. Gelingt, wenn es bereits existiert |

Jedes Argument, das eine Datei benennt, **kann eine Zeichenkette oder ein `pathname` sein**. Das ist dieselbe
Behandlung wie bei CLs Pfadnamensbezeichnern, aufgelöst über den Trait `Pathish` statt über eine Typprüfung zur
Laufzeit (Kapitel 9).

Das Endzeichen von `read-delimited-list` **beendet auch Token**. Es wirkt nur in Tiefe 0: In `(1 2]` wird das
`]` als Teil des Textes der Liste selbst gelesen und als kaputte Liste gemeldet. Ein Gegenstück zu CLs drittem
Argument `recursive-p` gibt es nicht.

## 7. Den eigenen Typ zu einem Stream machen

Man schreibt ein `write-item`, und die Standardimplementierungen bringen den Rest mit. Er kann auch in
zusammengesetzte Streams gesteckt werden.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; alle übrigen Methoden sind die Standardimplementierungen

(write-line (counter::new 0) "four")   ; write-line, terpri und fresh-line funktionieren alle
```

Eingabe funktioniert genauso: Man schreibt nur `read-item`. Selbst ein Typ ohne eigenes Zurückstellen lässt
sich lesen, sobald er umhüllt ist, wie in `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Name | Aufruf | Typ | Beschreibung |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` liest das Zeichen `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Gibt zurück, was registriert ist |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` liest die Zwei-Zeichen-Folge `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Wie oben |

`F` ist `(fn (string-input-stream char) Option<Sexpr>)`. Wie man sie verwendet, wann sie wirken und wie sie
sich von CL unterscheiden, steht in der [Syntaxreferenz](../syntax.md#11-lesemakros-readtable).

## 9. Pfadnamen `pathname`

Ein in Teile zerlegter Dateiname. Er hält die durch `/` getrennten Verzeichniskomponenten, den Namen, den Typ
(die Endung) und ob er an der Wurzel beginnt.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")   am letzten Punkt geteilt
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Der Pfadbezeichner-Trait `Pathish`

Wo CL einen Pfadnamensbezeichner (eine Zeichenkette oder einen Pfadnamen) akzeptiert, akzeptiert diese Sprache
ein `Pathish`. Sowohl `string` als auch `pathname` implementieren ihn, und **jede Dateioperation nimmt ihn
generisch**, daher sind `(open-input "a.txt")` und `(open-input p)` beides gewöhnliche Aufrufe (es gibt keine
Typprüfung zur Laufzeit). Das `namestring` einer Zeichenkette gibt einfach sie selbst zurück; solange man eine
Zeichenkette übergibt, findet also kein Parsen statt.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | Die Zeichenkettenform. Muss implementiert werden |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Wandelt in einen `pathname` um (CLs Funktion `pathname`, umbenannt, weil sie mit dem Typnamen kollidieren würde). Muss implementiert werden |

### 9.2 Funktionen

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Zerlegt eine Zeichenkette in Teile. Ein abschließendes `/` (oder ein leerer Name) bedeutet „kein Name“, also ein Verzeichnis |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Baut einen aus genau den angegebenen Komponenten (alle `&key`). Ein weggelassener Name oder Typ bleibt „abwesend“ und ist etwas, das `merge-pathnames` ergänzt |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Die Verzeichniskomponenten, die äußerste zuerst |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Der Name ohne den Typ. `none` bei einem Verzeichnis |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Nach dem letzten Punkt. Ein führender Punkt zählt nicht (ganz `.gitignore` ist der Name) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Ob er an der Wurzel beginnt |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | Das Home-Verzeichnis. `none`, wenn es kein `$HOME` gibt (CL erlaubt auch `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | Der Teil bis zum letzten `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Nur der Teil `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Ergänzt die in `p` fehlenden Komponenten aus `default`. Ein relatives `p` kommt unter das Verzeichnis von `default`; ein absolutes `p` behält sein eigenes Verzeichnis |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | Die Form relativ zu `default`. Ganz `p`, wenn es nicht unter der Basis liegt |

Die Typargumente tragen alle `(where (Pathish P))`.

## 10. Unterschiede zu CL

- **Eine Trait-Hierarchie, keine Klassenhierarchie.** Es gibt kein `input-stream-p` / `output-stream-p`: Der Typ
  trägt die Richtung, daher ist es keine Frage, die man zur Laufzeit stellt.
- **`read` hat für die Zeichenketten- und die Stream-Fassung verschiedene Namen.** `(read "...")` (entspricht dem
  ersten Wert von CLs `read-from-string`; braucht man auch die Position, an der das Lesen endete, verwendet man
  `read-from-string`) und `(read-sexpr s)` (CLs `read`). Ein Aufruf wird zu einem Empfängertyp aufgelöst, daher
  lässt sich derselbe Name nicht überladen.
- **Das Zurückstellen ist ein eigener Trait** (`PeekInput`), daher müssen Typen, die nur `read-char` brauchen,
  nicht `unread-char` implementieren.
- **Schließen ist ausdrücklich.** Der GC schließt keine Streams (der GC läuft zu unvorhersehbaren Zeitpunkten,
  daher wäre auch der Moment des Schließens unvorhersehbar, wenn man es ihm überließe). `with-open-file` zu
  verwenden ist der sichere Weg.
- **Pfadnamen haben keine Host-, Geräte- oder Versionskomponenten.** Es gibt weder Pfadnamen mit Platzhaltern
  noch logische Pfadnamen (`logical-pathname`). Das Trennzeichen ist immer `/`.
- **Die Funktion `pathname` heißt `to-pathname`**, weil Typen, Traits und Funktionen sich einen Namensraum
  teilen.
- **Es gibt keinen Abgleich mit Platzhaltern**, daher ist `directory` eine Funktion, die „den Inhalt dieses
  Verzeichnisses auflistet“, und nichts weiter. CLs `directory` gleicht gegen ein Pfadnamensmuster ab.
