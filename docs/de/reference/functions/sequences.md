<!-- translated-from: docs/ja/reference/functions/sequences.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Paare, S-Ausdrücke und Sequenzen

Das generische Paar `cons-cell`, S-Ausdrucksdaten `Sexpr`, Symbole, die auf `Iter` aufgebauten
Sequenzfunktionen und Funktionen höherer Ordnung.

## 1. Paare `cons-cell<A,B>`

`cons`/`car`/`cdr` sind der Konstruktor und die Feldzugriffe des **generischen Paartyps `cons-cell<A,B>`**
(ein `defstruct` der Standardbibliothek). Die Felder lassen sich entweder als `Variable::car`/`Variable::cdr`
(die `defstruct`-Zugriffssyntax der
[Syntaxreferenz](../syntax.md#36-defstruct--strukturen-benutzerdefinierte-typen)) oder als
`(car Variable)`/`(cdr Variable)` lesen. Geändert werden sie mit `(setf Variable::car v)`/
`(setf Variable::cdr v)`.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Erzeugt ein Paar |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Das erste Element |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Der Rest |

`cons-cell` dient auch anstelle einer Tupelsyntax. CL-Funktionen, die mehrere Werte zurückgeben (Quotient und
Rest von `floor`, Wert und Position von `read-from-string` usw.), geben in dieser Sprache eine `cons-cell`
zurück.

## 2. S-Ausdrucksdaten `Sexpr`

Der von `read` zurückgegebene Datentyp `Sexpr` hat 16 Varianten:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path`.
S-Ausdruckszellen werden nicht mit dem allgemeinen `cons`/`car`/`cdr` aus Kapitel 1 behandelt, sondern mit den
Funktionen `sexpr-*`. Sie werden hauptsächlich in `defmacro`-Rümpfen verwendet, um Formen aufzubauen und zu
zerlegen.

**Der Typ von S-Ausdrucksdaten ist `Option<Sexpr>`.** Die leere Liste ist keine Variante von `Sexpr`, sondern
das `none` von `Option`, und `Sexpr` selbst bedeutet „ein nicht leerer S-Ausdruck“. Daher nehmen und liefern
die Funktionen `sexpr-*` `Option<Sexpr>`.

- `()` ist die leere Liste, wo ein `Option<Sexpr>` erwartet wird (auch als `(Option::none)` schreibbar)
- `Sexpr` wird implizit erweitert, wo ein `Option<Sexpr>` erwartet wird (ohne Umwandlung zur Laufzeit). Die
  umgekehrte Richtung, ein `Option<Sexpr>` als `Sexpr` zu verwenden, behauptet „das ist nicht die leere
  Liste“ und muss daher ausdrücklich mit `match` oder `unwrap` angegeben werden
- In `match` lassen sich die 16 Varianten von `Sexpr` und `none` **flach in derselben Liste von Zweigen**
  schreiben ([Syntaxreferenz](../syntax.md#43-match--mustervergleich))

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Erzeugt eine `Sexpr`-Zelle |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Das erste Element. **Für die leere Liste die leere Liste** (wie in CL). Panic bei einem Atom, das kein `Cons` ist |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Der Rest. **Für die leere Liste die leere Liste** (wie in CL). Panic bei einem Atom, das kein `Cons` ist |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Ob es ein `Cons` ist |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Ob es die leere Liste ist |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Ob es kein `Cons` ist |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Ob es ein `Sym` (Symbol) ist |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | Der Inhalt der Variante `int` (Fixnum oder Bignum). Panic bei einem anderen Typ |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | Der Inhalt der Variante dieser Breite. Panic bei einem anderen Typ |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Der Inhalt der Gleitkommavarianten. Panic bei einem anderen Typ |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Der Inhalt eines `Char`. Panic bei einem anderen Typ |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Der Inhalt eines `Bool`. Panic bei einem anderen Typ |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Der Inhalt eines `Str`. Panic bei einem anderen Typ |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Der Name eines `Sym`. Panic bei einem anderen Typ |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Vergleich der Identität (`Cons`/`Str` vergleichen Objektidentität, der Rest vergleicht Werte) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Strukturelle Gleichheit (`Cons` rekursiv, `Str` nach Inhalt) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Wie `equal`, zusätzlich Vergleich ohne Unterscheidung von Groß-/Kleinschreibung und Vergleich von Zahlen über Typgrenzen hinweg |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Verkettet zwei `Sexpr`-Listen (nicht destruktiv). `,@` expandiert hierzu |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Eine neue `Sexpr`-Liste, bei der `f` auf jedes Element einer `Sexpr`-Liste angewandt wurde (das `map` aus Kapitel 4 ist für `Iter` und kann keine `Sexpr`-Liste durchlaufen) |

Es gibt neun numerische Zugriffsfunktionen, eine pro Typ, weil ein `Sexpr` „der einzige Ort ist, an dem der
Typ eines Wertes nirgends sonst geschrieben steht“. Ein `u8`, das man in ein `Sexpr` steckt, geht als Variante
`u8` hinein und kommt nur mit `(sexpr-u8 s)` heraus. Es an `(sexpr-int s)` zu übergeben, löst einen Panic
aus; die Antwort wird nie stillschweigend erweitert. Die Ganzzahlen in gelesenen Daten (`'(1 2 3)`,
Makroargumente) sind von der Variante `int` und werden mit `(sexpr-int s)` gelesen.

`Sexpr`-Listen haben keine destruktiven Operationen wie `rplaca`/`nconc`. Eine `Sexpr`-Zelle lässt sich nach
der Erzeugung nicht ändern.

## 3. Symbole

`symbol` ist der Typ der Symbole selbst. Es wird implizit umgewandelt, wo ein `Sexpr` verlangt wird, aber in
der umgekehrten Richtung nicht automatisch.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Holt den Namen des Symbols heraus |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Erzeugt ein Symbol aus einer Zeichenkette (interniert es) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Ob es ein Schlüsselwort (`:name`) ist. Der Doppelpunkt ist Teil des Namens, daher prüft der Test das erste Zeichen ([Syntaxreferenz](../syntax.md#1-lexikalische-elemente)) |

Zu `gensym` siehe [Makros](system.md#8-makros).

## 4. Sequenzfunktionen auf `Iter`

Die Sequenzfunktionen sind **generische Funktionen über dem Trait `Iter`**. Von einer Sammlung holt man mit
`(iter coll)` einen Iterator und übergibt ihn (`Vector<T>` / `HashTable<K,V>` / `Array<T>` unterstützen das;
eine `Sexpr`-Liste implementiert `Iter` nicht, daher gelten diese Funktionen nicht für sie). **Eine
Ergebnissammlung wird als neuer `Vector` zurückgegeben.** `Iter<A>` in den Tabellen bedeutet „jede
Implementierung von `Iter`, deren `Item` `A` ist“. Um den zurückgegebenen `Vector` erneut zu durchlaufen,
übergibt man `(iter result)`.

Funktionen, die ein Prädikat nehmen (entsprechen CLs `-if`-Familie):

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Abbildung |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Nur die Elemente, die das Prädikat erfüllen |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Entfernt die Elemente, die das Prädikat erfüllen |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Das erste Element, das das Prädikat erfüllt |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | Die erste Position, die das Prädikat erfüllt |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Wie viele das Prädikat erfüllen |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Ob jedes Element das Prädikat erfüllt |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Ob irgendein Element das Prädikat erfüllt (entspricht CLs `some`; ein Name, der nicht mit dem Konstruktor `Some` kollidiert) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Linksfaltung |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Rechtsfaltung |

Indizierung, Länge und Ausschnitte:

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Anzahl der Elemente |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Verkettet Iteratoren. Auch drei oder mehr sind möglich |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | CLs `concatenate`. Der Ergebnistyp wird als **quotiertes Symbolliteral** geschrieben (CL verwendet einen Typspezifikator zur Laufzeit). `'vector` nimmt eines oder mehr, `'string` null oder mehr (`""` bei null). `Sexpr`-Listen sind nicht abgedeckt (man verwendet `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Umkehrung (nicht destruktiv) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Element `n` (`None` außerhalb des Bereichs) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` mit umgekehrten Argumenten |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | Die ersten `n` Elemente |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` wird auf die Länge begrenzt) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | Das letzte **Element** (nicht „die letzte Zelle“ wie in CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Alle außer dem letzten Element |

Funktionen, die eine Schranke `Eq` / `Ord` verlangen (sie vergleichen über einen Trait statt über ein
Prädikat; [Standard-Traits](traits.md#2-eq--ord-vergleich)):

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Ob ein Element gleich `x` existiert (anders als CL ein `bool`, nicht der Rest der Liste) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | Das erste Element gleich `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | Die erste Position gleich `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Wie viele Elemente gleich `x` sind |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CLs `(sort sequence predicate)`. Eine stabile, nicht destruktive Sortierung. `cmp` ist `true`, wenn „das erste Argument strikt vor dem zweiten kommt“ |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | Das erste Paar, dessen `car` gleich `k` ist. Den Wert holt man mit `(cdr p)` heraus |

Diese und viele Funktionen aus Kapitel 5 nehmen auch CLs Schlüsselwortargumente `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (Kapitel 6).

## 5. Die übrigen Sequenzfunktionen von CL

Alle sind wie in Kapitel 4 generische Funktionen auf `Iter`. Ergebnissammlungen werden als neue `Vector`s
zurückgegeben.

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CLs benannte Indizes |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Alle außer dem ersten (ein neuer `Vector`, kein geteiltes Ende) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Macht aus einem Iterator einen `Vector` (CLs `copy-seq`/`copy-list`) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` umgekehrt, gefolgt von `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` Kopien von `x` (CLs `make-list`/`make-sequence`). Wie bei `Vector::new` stammt das Typargument aus dem erwarteten Typ, daher braucht ein bloßes `let` `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Wie `member` ein **`bool`** (ein Iterator hat kein Ende, das er zurückgeben könnte) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Die Negationen von `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Dieselben Typen wie die positiven Fassungen | Fassungen mit negiertem Prädikat |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Entfernt nach Wert |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Entfernt Duplikate. Wie in CL **bleibt das letzte Vorkommen erhalten** (`:from-end true` behält das erste) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Ersetzt nach Wert / Prädikat |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | auf `Iter<cons-cell<K,V>>` | Die Prädikats- und wertseitigen Fassungen von `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Fügt vorn ein Paar hinzu |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Paart zwei Sequenzen. Hält bei der kürzeren an |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | CLs `mapcar` über mehrere Sequenzen. Hält bei der kürzeren an |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Abbildung um der Seiteneffekte willen |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Bildet ab und verkettet |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Bildet über aufeinanderfolgende **Enden** ab |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Bildet um der Seiteneffekte willen über Enden ab (das `maplist`-Gegenstück zu `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Bildet über Enden ab und verkettet (das `maplist`-Gegenstück zu `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Die Position, an der `sub` zuerst vorkommt. Ist der Empfänger eine `string`, wird die `string`-Methode gewählt ([Zeichenketten](collections.md#1-zeichenketten-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Die erste Position, an der sie sich unterscheiden. `none`, wenn sie gleich sind |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Zusammenführen. CL verlangt sortierte Eingaben; dies sortiert die Verkettung |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Fügt `x` **vorn** hinzu, wenn es nicht da ist |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Mengenoperationen. CL legt die Reihenfolge nicht fest; hier ist sie stabil, **in der Reihenfolge des ersten Auftretens** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Inklusion |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Ob es ein Suffix ist / der Teil vor dem Suffix. CL fragt nach **geteilter Struktur**, aber es gibt keine Struktur zu teilen, daher fragt dies nach einem Suffix **als Werte** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Elementweise Gleichheit. `Vector<T>` selbst implementiert `Eq` nicht |
| `caar`…`cddddr` | `(cadr p)` | auf verschachtelten Paaren | CLs 28 Funktionen. Sie durchlaufen **Paare, keine Listen**: `cadr` nimmt eine `cons-cell<A,cons-cell<B,C>>` |

Was CL hat und diese Sprache nicht: `list*` (es gibt keinen Begriff einer unechten Liste, deren Ende ersetzt
wird), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (kein Typ kann das Durchlaufen eines heterogenen
Baums beliebiger Tiefe beschreiben; für einen Baum aus `Sexpr` entspricht `equal` `tree-equal`), die Familie der
Eigenschaftslisten `getf`/`get-properties`/`symbol-plist`/`remprop` (es gibt keine Darstellung als untypisierte
Liste mit abwechselnden Schlüsseln und Werten; `assoc` (Assoziationslisten) oder `HashTable` erfüllen dieselbe
Rolle) sowie Funktionen, die zwischen `Vector<T>` und `Sexpr`-Listen umwandeln (die Elemente einer
`Sexpr`-Liste können jeweils einen anderen Typ haben, daher lassen sie sich nicht mit einem einzigen
Elementtyp `T` schreiben).

## 6. Schlüsselwortargumente

Die Funktionen aus den Kapiteln 4 und 5 nehmen CLs Sequenzschlüsselwörter `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count`. Alle sind **optional**.

| Schlüsselwort | Typ | Bedeutung |
|---|---|---|
| `:key` | `(fn (A) A)` | Eine Projektion, die vor dem Vergleichen oder Prüfen auf jedes Element angewandt wird |
| `:test` | `(fn (A A) bool)` | Ein Gleichheitstest, der statt `equals` aus der Schranke `Eq` verwendet wird. Das erste Argument ist **das gesuchte Objekt**, das zweite das Element (nach `:key`), in derselben Reihenfolge wie in CL |
| `:test-not` | `(fn (A A) bool)` | Die Negation von `:test` |
| `:start` `:end` | `int` | Das zu durchsuchende Fenster `[start, end)`. Indizes sind relativ zur ganzen Sequenz |
| `:from-end` | `bool` | Eine Suche antwortet mit dem **letzten** Treffer. In Kombination mit `:count` werden die betroffenen Elemente vom Ende genommen |
| `:count` | `int` | Die Höchstzahl der Elemente, die die Familien `remove` / `substitute` betreffen |

Welche Funktion welche nimmt, folgt CL:

| Funktion | Genommene Schlüsselwörter |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Alle obigen (einschließlich `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (das `:key` von `assoc` gilt für das `car`, das von `rassoc` für das `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; entfernt nur eines, vom Ende her
(position 3 (iter v) :start 1)                          ; der Index ist relativ zur ganzen Sequenz
```

**Unterschiede zu CL**:

1. **Die Projektion von `:key` bleibt innerhalb des Elementtyps** (`(fn (A) A)`). Sie kann nicht wie in CL auf
   einen anderen Typ projizieren: Eine zusätzliche Typvariable ließe sich nicht bestimmen, wenn das Argument
   weggelassen wird. Wo eine Projektion auf einen anderen Typ nötig ist, übergibt man stattdessen ein lambda an
   die `-if`-Familie (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **Bei Suchen nach einem Objekt gilt `:key` nur für die Elemente** (nicht für das gesuchte Objekt). Das ist
   dieselbe Regel wie bei CLs `find`/`position`/`count`/`member`/`remove`/`substitute`. Bei Mengenoperationen
   sind beide Seiten Elemente, daher gilt es für beide.
3. **Nur die Schlüsselwörter von `search` sind benannt statt nummeriert.** In CL gelten `:start1`/`:end1` für
   das **Muster** und `:start2`/`:end2` für die durchsuchte Sequenz. In dieser Sprache kommt der Empfänger
   zuerst, daher würden dieselben Nummern das Gegenteil bedeuten, und zwar stillschweigend. `:start`/`:end`
   gelten für den Empfänger und `:sub-start`/`:sub-end` für das Muster, daher ergibt ein gedankenloses
   `:start1` den Fehler „unbekanntes Schlüsselwort“. `mismatch` und `replace` haben dieselbe
   Argumentreihenfolge wie CL und behalten daher CLs Nummern.

## 7. Destruktive Operationen

Methoden von `Vector<T>`. **Sie verändern den Empfänger und geben den Empfänger selbst zurück**, daher schreibt
man `(nreverse v)` genauso wie `reverse`, und `v` selbst wird ebenfalls umgekehrt.

| Name | Form | Beschreibung |
|---|---|---|
| `nreverse` | `(nreverse v)` | Kehrt an Ort und Stelle um |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Fassungen an Ort und Stelle von `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Fassungen an Ort und Stelle der `substitute`-Familie |
| `nbutlast` | `(nbutlast v)` | Entfernt das letzte Element |
| `fill` | `(fill v x)` | Setzt jedes Element auf `x`. Die Länge ändert sich nicht |
| `replace` | `(replace v src)` | Überschreibt von vorn mit den Elementen von `src`. `(min (len v) (len src))` Elemente |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Dieselbe Anzahl wie oben |
| `nconc` | `(nconc v w)` | Hängt die Elemente von `w` an `v` an. Anders als CL **schreibt es keine geteilte Struktur um** (`w` ist nicht betroffen) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Ersetzt den Inhalt von `v` durch `src` (auch die Länge ändert sich) |
| `rplaca` `rplacd` | `(rplaca p x)` | Schreibt das `car`/`cdr` einer `cons-cell` um und gibt die Zelle selbst zurück |

Genommene Schlüsselwörter:

| Destruktive Fassung | Genommene Schlüsselwörter |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (der Empfänger ist CLs `sequence-1`) |

`vector-push-extend`/`vector-pop` sind einfach `push`/`pop` von `Vector<T>`. Ein `Vector<T>` wächst immer, daher
entspricht nichts CLs Unterscheidung zwischen „einem Vektor mit Füllzeiger“ und „einem einfachen Vektor“.

## 8. Funktionen höherer Ordnung

| Name | Form | Typ | Beschreibung |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Gibt sein Argument zurück |
| `const` | `(const x y)` | `(A,B)→A` | Gibt das erste Argument zurück |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Funktionskomposition `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Vertauscht die Argumente einer zweistelligen Funktion |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Negation eines Prädikats |

Ein CL-`constantly` gibt es nicht (der Typ des ignorierten Arguments käme nur im Rückgabetyp vor und ließe sich
nicht bestimmen). Man schreibt `(lambda ((x T)) A v)`.
