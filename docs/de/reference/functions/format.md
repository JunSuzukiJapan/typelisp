<!-- translated-from: docs/ja/reference/functions/format.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Formatdirektiven

Die Direktiven, die man in die Steuerzeichenketten von `print`/`println`/`format` schreibt. Sie decken nahezu
alle `format`-Direktiven von CL ab. Die Funktionen selbst sind unter
[Ausgabe](printing.md#1-print--println--format) beschrieben.

## 1. Wie man Direktiven schreibt

Jede Direktive besteht der Reihe nach aus `~`, optionalen **Präfixparametern** (durch Kommas getrennt: eine
Ganzzahl / `'c` (ein Zeichen) / `v` (aus dem nächsten Argument genommen) / `#` (die Anzahl der verbleibenden
Argumente)), den optionalen **Modifikatoren** `:` und `@` und dem Direktivenzeichen. Bei Direktivenzeichen
wird Groß-/Kleinschreibung nicht unterschieden.

Die Steuerzeichenkette muss ein Literal sein ([Ausgabe](printing.md#1-print--println--format)). Darüber hinaus
wird bei der Prüfung Folgendes kontrolliert.

- **Anzahl und Typen der Argumente.** Für jede Direktive, die ein Argument verbraucht: ob noch ein Argument
  übrig ist und ob sein Typ akzeptiert wird (die Vermerke „Argument“ in den Tabellen unten). Wo der Weg von
  Laufzeitwerten abhängt, etwa beim Springen mit `~*`, bei der Wahl der Klausel von `~[`, beim Auslösen von
  `~^` oder bei der Anzahl der Wiederholungen von `~@{`, wird **jeder Weg** geprüft. Übrige Argumente sind in
  Ordnung (wie in CL).
- **Parameter und Modifikatoren.** Ein nicht akzeptierter Modifikator, zu viele Parameter und Werte außerhalb
  des Bereichs (eine negative Breite, eine Basis außer 2 bis 36, eine Ganzzahl, wo ein Zeichen erwartet wird,
  usw.) sind Fehler. Sie werden nie stillschweigend ignoriert oder gerundet.

Die **Elemente** eines Listenarguments (`~{`, `~:{`, `~<...~:>`) sind `Sexpr`s, und weder ihre Anzahl noch der
Typ jedes Elements lässt sich aus den Typen ablesen. Anforderungen an Elemente (eine Ganzzahl für `~d` usw.)
und fehlende Elemente werden geprüft, wenn die Werte ankommen, und sind Laufzeitfehler (es wird nie
stattdessen auf eine andere Darstellung ausgewichen).

CLs nachsichtige Regeln werden nicht übernommen. Eine Nicht-Ganzzahl an `~d` zu übergeben und sie wie `~a`
ausgeben zu lassen oder dass `~:[` jeden Wert als Wahrheitswert behandelt, wird nicht so umgedeutet; es sind
Typfehler.

## 2. Ausgabe (verbraucht ein Argument)

| Direktive | Parameter / Modifikatoren | Bedeutung |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=rechtsbündig | Ästhetisch (CLs `princ`; Zeichenketten ohne Anführungszeichen). Das Argument kann beliebigen Typs sein |
| `~s` | Wie oben | Standard (CLs `prin1`; eine Form, die sich zurücklesen lässt). Das Argument kann beliebigen Typs sein |
| `~w` | — | CLs `write`. Formatiert schön, wenn `*print-pretty*` wahr ist, sonst dasselbe wie `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=Zifferngruppen, `@`=immer ein Vorzeichen | Dezimale/binäre/oktale/hexadezimale Ganzzahlen. Das Argument ist eine Ganzzahl |
| `~r` | `~radix,mincol,padchar,commachar,interval` (mit Basis) oder keine | Mit Basis in dieser Basis (2 bis 36). Ohne: `~r`=englische Grundzahl, `~:r`=englische Ordnungszahl, `~@r`=römische Zahlen, `~:@r`=alte römische Zahlen. Das Argument ist eine Ganzzahl |
| `~p` | `:`=eins zurück, `@`=y/ies | Plurale (`~p`→"s", `~@p`→"y"/"ies"). Das Argument ist eine Ganzzahl |
| `~c` | `:`=Name, `@`=Syntax `#\` | Ein Zeichen. Das Argument ist ein `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=Vorzeichen | Festkomma. Das Argument ist eine Zahl |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=Vorzeichen | Exponentialschreibweise. Das Argument ist eine Zahl. CLs Parameter für Exponentenziffern, Skalierung und overflowchar werden nicht unterstützt (sie anzugeben ist ein Fehler) |
| `~g` | `@`=Vorzeichen | Allgemeine Gleitkommadarstellung. Das Argument ist eine Zahl. Nimmt keine Parameter |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Währungsschreibweise. Das Argument ist eine Zahl |

## 3. Ausgabe (verbraucht keine Argumente)

| Direktive | Bedeutung |
|---|---|
| `~%` | Zeilenumbruch (`~n%` für n davon) |
| `~&` | fresh-line (ein Zeilenumbruch, außer am Zeilenanfang; `~n&`) |
| `~\|` | Seitenumbruch (Seitenvorschub) |
| `~~` | Ein literales `~` (`~n~` für n davon) |
| `~t` | Tabulator (`~colnum,colincT`. Ist man schon in Spalte colnum oder dahinter, geht es um ein Vielfaches von colinc weiter; bei colinc 0 keine Bewegung. `@`=relativ. `:`=ein Tabulator relativ zum Anfang des logischen Blocks, der nur bei schöner Formatierung wirkt) |
| `~_` | Bedingter Zeilenumbruch (pretty; einfach=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Einrückung (pretty; `~ni`=Blockanfang + n / `~n:i`=aktuelle Spalte + n) |
| `~<newline>` | Ignoriert den Zeilenumbruch (`:`=Leerraum behalten, `@`=Zeilenumbruch behalten) |

Wie in CL tun die Direktiven des Pretty Printers (`~_` `~i` `~:t` `~<...~:>` und der Weg der schönen
Formatierung von `~a`/`~s`/`~w`) allesamt nichts, wenn `*print-pretty*` falsch ist. Standardmäßig ist es falsch.

## 4. Kontrollstrukturen

| Direktive | Bedeutung |
|---|---|
| `~(...~)` | Umwandlung der Schreibweise (`~(` Kleinbuchstaben, `~:(` jedes Wort großschreiben, `~@(` nur das erste Wort großschreiben, `~:@(` alles in Großbuchstaben) |
| `~[...~;...~]` | Bedingte Auswahl (verzweigt nach einem Ganzzahlargument. Mit `~n[`, `~v[` oder `~#[` verzweigt sie nach diesem Wert und nimmt kein Argument. `~:;`=die Standardklausel, nur als letzte Klausel). `~:[false~;true~]` verzweigt nach einem `bool`-Argument und hat genau zwei Klauseln |
| `~{...~}` | Iteration (durchläuft ein Listenargument. `~:{`=pro Unterliste, `~@{`=über die verbleibenden Argumente, `~:@{`=über jede Liste unter den verbleibenden Argumenten, `~^`=verlassen, `~:}`=einmal ausführen, auch wenn leer). Ein Rumpf, der in einer Iteration kein Argument verbraucht, ist ein Fehler (er würde nie enden) |
| `~<...~;...~>` | Ausrichtung (verteilt Segmente über `~mincol` Spalten. `:`/`@`=Auffüllen an den Enden) |
| `~<...~;...~:>` | **Logischer Block** (mit `~:>` geschlossen; etwas anderes als die Ausrichtung oben). Das erste Segment ist das Präfix und das letzte das Suffix (beides nur literale Zeichenketten). Mit dem Trenner `~@;` ist das Präfix ein **Präfix pro Zeile**. `~:<` setzt Präfix/Suffix standardmäßig auf `(`/`)`. Das Argument ist eine Liste (`~@<` verwendet stattdessen die verbleibenden Argumente) |
| `~*` | Argumente überspringen (`~n*`=n vorwärts, `~:*`=zurück, `~@*`=an eine absolute Position) |
| `~/name/` | Methodenaufruf (Kapitel 5. Die Flags `:`/`@` werden an die Methode übergeben. Nimmt keine Parameter) |

Die folgenden Direktiven von CL werden nicht unterstützt (sie sind Fehler bei der Prüfung).

- `~?` und `~@?`: Sie nehmen eine Steuerzeichenkette als Laufzeitargument, daher lassen sich die Argumente,
  die deren Direktiven verbrauchen, nicht prüfen. Man schreibt diese Direktiven direkt in die
  Steuerzeichenkette.
- `~@[...~]`: Es prüft, ob ein Argument nicht nil ist, aber diese Sprache hat kein nil. Man verwendet
  `~:[false~;true~]`, das nach einem `bool` verzweigt.
- `~{~}` mit leerem Rumpf: Es nimmt den Rumpf aus einem Laufzeitargument. Man schreibt die Direktiven in die
  geschweiften Klammern.

## 5. `~/name/`

**Ein Unterschied zu CL: Der Name wird nicht als globale Funktion, sondern als Methode des eigenen Typs des
Arguments gesucht.** Die Methode hat die Form `((self Self) (colon bool) (at bool)) → string`, und `:`/`@` der
Direktive werden unverändert durchgereicht.

CLs Weg, sie als globale Funktion zu suchen, lässt sich in dieser Sprache nicht sicher umsetzen. Selbst bei
einer literalen Steuerzeichenkette ist der Typ der Elemente eines Listenarguments (innerhalb von `~{`) bei
der Prüfung nicht bekannt, und eine Funktion allein über ihren Namen zu suchen, könnte eine Funktion
aufrufen, die für einen anderen Typ gedacht ist. Die Wahl nach dem Typ des Wertes bedeutet, dass die Methode
genau für diesen Typ typgeprüft ist, was sicher ist (derselbe Mechanismus wie bei `print-object`). Es
funktioniert auch für Werte wie `string`/`bool`/`char`/`symbol`/Listen. Nur bei Ganzzahlen, deren Breite sich
nicht aus dem Wert ablesen lässt, ist es ein Fehler, **wenn mehr als ein Ganzzahltyp eine Methode dieses
Namens definiert**.

Auf welches Argument sie angewandt wird, ist nicht bekannt, wohl aber, welche Methoden sie aufrufen könnte.
Die Prüfung sammelt jedes `~/name/` aus der literalen Steuerzeichenkette und vermerkt unter den Typen der
Argumente an dieser Aufrufstelle diejenigen, die eine Methode der obigen Form haben. Daher gilt: **Hat keiner
der Argumenttypen die Methode, ist das ein Fehler bei der Prüfung** (nicht zur Laufzeit), und es funktioniert
auch in AOT-Programmen.

```lisp
(defstruct point (x i32) (y i32))
(defmethod brief ((self point) (colon bool) (at bool)) string
  (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y)))
(println "~a" (format false "~/brief/"  (point::new 3 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (point::new 3 4)))   ; => <3,4>
```
