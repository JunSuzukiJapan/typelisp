<!-- translated-from: docs/ja/reference/functions/collections.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Zeichenketten, Zeichen und Sammlungen

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` und `BitVector`.

## 1. Zeichenketten `string`

Zeichenketten sind unveränderlich.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Wandelt in Großbuchstaben um (nur ASCII). Gibt wie CLs `string-upcase` eine neue Zeichenkette zurück. Zeichenketten sind unveränderlich, daher gibt es kein destruktives `nstring-upcase`; diese Funktion tritt an seine Stelle |
| `downcase` | `(downcase s)` | `string→string` | Wandelt in Kleinbuchstaben um (nur ASCII). Tritt an die Stelle von `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Schreibt den ersten Buchstaben jedes Wortes groß und den Rest klein (CLs `string-capitalize`). Ein Wort ist eine maximale Folge von Buchstaben und Ziffern |
| `length` | `(length s)` | `string→int` | Anzahl der Zeichen |
| `ref` | `(ref s i)` | `(string,int)→char` | Das Zeichen `i`. Panic außerhalb des Bereichs |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | Die Teilzeichenkette `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Verkettung. Auch drei oder mehr sind möglich (dasselbe wie `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Lexikografischer Vergleich |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Strikt lexikografisch kleiner (dasselbe wie `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Vergleich der Identität (ob es dasselbe Objekt ist, nicht derselbe Inhalt) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Vergleicht den Inhalt (Groß-/Kleinschreibung wird unterschieden) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Vergleicht den Inhalt (ohne Unterscheidung von Groß-/Kleinschreibung, nur ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Ob sich der Inhalt unterscheidet (CLs `string/=`. Die variadische Form vergleicht benachbarte Paare) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Ordnung ohne Unterscheidung von Groß-/Kleinschreibung (CLs `string-lessp` usw.). Bei gemeinsamem Präfix ist die kürzere kleiner |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Eine Zeichenkette aus `n` Kopien von `c` (CLs `make-string`) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | Die Position, an der `sub` zuerst vorkommt. **CLs `search` hat die Argumente umgekehrt** (`(search pattern sequence)`). Die leere Zeichenkette wird bei 0 gefunden. Zu Schlüsselwörtern siehe [Schlüsselwortargumente der Sequenzen](sequences.md#6-schlüsselwortargumente) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | Die erste Position, an der sie sich unterscheiden. `none` nur, wenn sie `equal` sind. Ist die eine ein Präfix der anderen, das Ende der kürzeren. Schlüsselwörter wie oben |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Entfernt die in `bag` enthaltenen Zeichen an beiden Enden / links / rechts (CLs `string-trim` usw.). Ohne `bag` Leerraum `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Teilt an `sep`. In CL gibt es kein Gegenstück. Aufeinanderfolgende Trennzeichen ergeben leere Elemente. Panic, wenn `sep` leer ist |
| `to-string` | `(to-string x)` | `T→string` | Wandelt wie `~a` in eine Zeichenkette um. Implementiert für `int`/`i32`/`f64`/`bool`/`char`/`string` (CLs `princ-to-string`) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Kodiert als UTF-8 (jedes Element 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Dekodiert. `none`, wenn es kein gültiges UTF-8 ist |

## 2. Zeichen `char`

Ein `char` ist ein Unicode-Skalarwert. Umwandlung der Schreibweise und Klassifikation behandeln nur den
ASCII-Bereich.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Wandelt in einen Großbuchstaben um (nur ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Wandelt in einen Kleinbuchstaben um (nur ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Vergleich nach Codepunkt |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Strikt kleiner nach Codepunkt (dasselbe wie `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Ob es ein ASCII-Buchstabe ist |
| `digitp` | `(digitp c)` | `char→bool` | Ob es eine ASCII-Ziffer ist |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Vergleicht die Werte |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Vergleicht die Werte ohne Unterscheidung von Groß-/Kleinschreibung (CLs `char-equal`) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Ob sich die Werte unterscheiden (CLs `char/=`. **Die variadische Form vergleicht benachbarte Paare**, anders als CL, das fragt, ob sich alle Paare unterscheiden) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Ordnung ohne Unterscheidung von Groß-/Kleinschreibung (CLs `char-lessp` usw.) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Großbuchstabe / Kleinbuchstabe / unterscheidet überhaupt Groß- und Kleinschreibung (CLs `upper-case-p` usw.) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Ein Buchstabe oder eine Ziffer (derselbe Name wie in CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Ob es druckbar ist. Schließt das Leerzeichen ein, nicht aber Zeilenumbruch oder Tabulator (CLs `graphic-char-p`) |
| `standardp` | `(standardp c)` | `char→bool` | Ob es eines der 96 Standardzeichen von CL ist, also `graphicp` plus Zeilenumbruch (CLs `standard-char-p`) |
| `char->int` | `(char->int c)` | `char→int` | Der Unicode-Skalarwert (die Umkehrung ist `int->char`/`try-int->char` unter [Zahlen](numbers.md#1-ganzzahlen-fester-breite)). Entspricht CLs `char-code`/`char-int` |
| `char->string` | `(char->string c)` | `char→string` | Eine Zeichenkette aus einem Zeichen. CLs Funktion `string` deckt das ab, indem sie einen Bezeichner nimmt, aber diese Sprache hat keine Bezeichner, daher steht die Richtung im Namen |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | Das **Gewicht** der Ziffer in dieser Basis (CLs `digit-char-p`). `digitp` ist eine eigene Funktion, die `bool` zurückgibt |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Das Zeichen für das Gewicht `w`. Großbuchstaben ab 10 (CLs `digit-char`; die Basis ist höchstens 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Der Name des Zeichens. Nur die benannten Zeichen, die der Reader lesen kann, haben Namen (CLs `char-name`) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Das Zeichen zu einem Namen. Ohne Unterscheidung von Groß-/Kleinschreibung, akzeptiert auch die Aliasse des Readers (`linefeed`/`null`) (CLs `name-char`) |

Eine Konstante, die `char-code-limit` entspricht, gibt es nicht (die Obergrenze von `char` legt Unicode fest,
nicht die Sprache).

## 3. `Vector<T>`

Ein wachsendes Array.
Ein Wert lässt sich als `#(1 2 3)` schreiben
([Syntaxreferenz](../syntax.md#1-lexikalische-elemente); der Elementtyp ergibt sich aus dem Kontext
oder dem ersten Element, und jede Auswertung erzeugt einen neuen Vektor). Ausgegeben wird er
ebenfalls als `#(1 2 3)`.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Erzeugt einen leeren Vektor. Das Typargument stammt aus dem erwarteten Typ, daher schreibt man in einem bloßen `let` `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` Kopien von `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Hängt ans Ende an |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Liest das Element `i`. Panic außerhalb des Bereichs |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Ändert das Element `i`. Panic außerhalb des Bereichs. Lässt sich auch als `(setf (get v i) x)` schreiben |
| `len` | `(len v)` | `Vector<T>→int` | Anzahl der Elemente |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Entfernt das letzte Element und gibt es zurück. `None`, wenn leer (anders als `get`/`set` kein Panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Erzeugt einen Iterator, der `Iter` implementiert |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Hängt `x` an, wenn kein gleiches Element existiert (CLs `pushnew`. Es muss keinen Ort umschreiben, daher ist es eine Methode statt eines Makros) |

`map`/`filter` und Verwandte sind [Sequenzfunktionen](sequences.md#4-sequenzfunktionen-auf-iter): Man übergibt
den Vektor über `iter`, wie in `(map (iter v) f)`. Destruktive Operationen (`nreverse`, `delete` usw.) stehen
unter [Destruktive Operationen](sequences.md#7-destruktive-operationen).

## 4. `HashTable<K,V>`

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Erzeugt eine leere Tabelle |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Nachschlagen |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Einfügen oder überschreiben |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Entfernt den Eintrag und gibt den alten Wert zurück, falls vorhanden |
| `count` | `(count h)` | `HashTable<K,V>→int` | Anzahl der Einträge |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Entfernt alles |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Eine Momentaufnahme der Schlüssel |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Eine Momentaufnahme der Werte |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Eine Momentaufnahme der Paare `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Ein Iterator, der `Iter` implementiert. Die Elemente sind `cons-cell`s `(k . v)`. Entspricht CLs `with-hash-table-iterator`; `doiter`/`map`/`filter` und andere funktionen unverändert darauf |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | CLs `maphash` |
| `size` | `(size h)` | `HashTable<K,V>→int` | CLs `hash-table-size`. Bei dieser Tabelle die Anzahl der belegten Einträge (gleich `count`) |

**Jeder Typ, der `Hash` implementiert, kann Schlüssel sein**, auch `defstruct`/`defenum`-Typen.
`get`/`set`/`remove` tragen `(where (Hash K))`, daher ist eine Tabelle, deren Schlüsseltyp ihn nicht
implementiert, ein **Typfehler** (`f64` hat wegen `NaN` kein `Hash`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; gibt einen nicht negativen Wert zurück, der in eine Fixnum passt
```

Implementiert für: `int` und die sechs Ganzzahltypen fester Breite, `bool`, `char`, `string` und `symbol`
(nicht für Gleitkommazahlen). Bei eigenen Typen hält man das Ergebnis nicht negativ, indem man es mit
`*sxhash-mask*` (2^30-1) per `logand` verknüpft. Um eine Zeichenkette zu hashen, kann man
`(sxhash-string s)` (32-Bit-FNV-1a) aufrufen, das die Implementierung für `string` verwendet.

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

Ob zwei Schlüssel derselbe sind, entscheidet **der Schlüsseltyp selbst** (`sxhash` und `equals` aus `Eq`, dem
Obertrait von `Hash`), nicht die Identität der Objekte. Deshalb kann man wie oben mit einem Schlüssel
nachschlagen, der „ein anderer Wert, aber gleich“ ist.

Kollisionen von `sxhash` sind in Ordnung (der Vertrag von `Hash` gilt nur in eine Richtung: gleiche Werte
müssen denselben Hash haben). Kollidierende Schlüssel werden durch `equals` unterschieden.

## 5. `Array<T>` (mehrdimensionale Arrays)

Ein `defstruct` der Standardbibliothek. Es ist kein eingebauter Typ, daher geht mit ihm alles, was mit einem
`defstruct` geht.
Ein Wert lässt sich als `#2A((1 2) (3 4))` schreiben
([Syntaxreferenz](../syntax.md#1-lexikalische-elemente)).

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | CLs `make-array`. `dims` wird kopiert. `init` ist der Anfangswert jeder Zelle (CLs `:initial-element`; diese Sprache kennt keine „ungebundene Zelle“, daher ist es Pflicht). `:fill-pointer` nur bei einer Dimension |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | CLs `aref` / `(setf (aref …))`. Panic, wenn ein Index außerhalb des Bereichs liegt |
| `aref` | `(aref a i j …)` | — | CLs Schreibweise mit bloßen Indizes. Expandiert zu `get`/`set` oben. Auch `(setf (aref a i j) v)` funktioniert |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | CLs `row-major-aref`. Ein flacher Index |
| `rank` | `(rank a)` | `Array<T>→int` | CLs `array-rank` |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | CLs `array-dimension` |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | CLs `array-dimensions`. Gibt eine **Kopie** zurück, so wie CL eine frische Liste zurückgibt |
| `total-size` | `(total-size a)` | `Array<T>→int` | CLs `array-total-size` (die Anzahl der angelegten Zellen, unabhängig vom Füllzeiger) |
| `len` | `(len a)` | `Array<T>→int` | CLs `length` auf Arrays. Der Füllzeiger, falls vorhanden, sonst `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | CLs `array-in-bounds-p`. Falsch (kein Fehler), auch wenn die **Anzahl** der Indizes falsch ist |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | CLs `array-row-major-index` |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | CLs `adjust-array`. Der Rang kann sich nicht ändern. Elemente, die im Bereich bleiben, behalten ihre Indizes, neue Zellen bekommen `init`. Anders als CL gibt es das Array nicht zurück (jedes Array dieser Sprache ist anpassbar, daher gibt es kein zweites Array, das man zurückgeben könnte) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CLs `vector-push-extend`. Panic ohne Füllzeiger |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CLs `vector-pop`. `none`, wenn leer |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Der Füllzeiger (`none`, wenn keiner da ist). Lässt sich mit `(setf a::fill-pointer …)` schreiben |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Ein Iterator in zeilenweiser Reihenfolge. Hält am Füllzeiger an, falls einer da ist |

- **Indizes sind ein `Vector<int>`.** Eine Methode kann nicht „am Ende beliebig oft dasselbe Argument
  desselben Typs“ deklarieren, und der syntaktische Zucker `aref` überbrückt diese Lücke.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p` **gibt es
  nicht**. Der statische Typ des Empfängers beantwortet diese Fragen bereits.
- `Array::new` ist der von `defstruct` erzeugte Konstruktor in Feldreihenfolge und nicht zum Anlegen von Arrays
  gedacht. Man verwendet `Array::make`.
- **Arrays werden in CLs Array-Syntax ausgegeben.** Rang 1 ist `#(1 2 3)`; andere Ränge sind `#nA`, gefolgt von
  entsprechend vielen Klammerebenen (`#2A((1 2 3) (4 5 6))`); Rang 0 ist `#0A5`. Die Ausgabe hält am
  Füllzeiger an, falls einer da ist. Setzt man `*print-array*`
  ([Ausgabe](printing.md#6-steuern-wie-viel-ausgegeben-wird)) auf false, wird nur die Form `#<array 2x3>`
  ausgegeben. Nur ein Array, dessen Elemente ein `defstruct` ohne `print-object` sind, wird in der eingebauten
  Form `#<array<...> ...>` ausgegeben (das ist kein Fehler).

## 6. `BitVector` (Bitvektoren)

Eine Bitfolge fester Länge. Ein `defstruct` der Standardbibliothek.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Länge `n`, alle Bits 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Panic außerhalb des Bereichs |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | CLs Schreibweisen. Auch `(setf (bit v i) b)` funktioniert. CLs `sbit` unterscheidet sich von `bit` nur darin, dass es einen einfachen Bitvektor verlangt, aber diese Sprache hat nur eine Art von Bitvektor |
| `len` | `(len v)` | `BitVector→int` | Anzahl der Bits |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Geben einen neuen Bitvektor zurück. Panic, wenn die Längen verschieden sind. Ein drittes Argument wie in CL (das Ziel des Ergebnisses) gibt es nicht |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Komplement |

`bit-vector-p` gibt es nicht (der statische Typ beantwortet es).
