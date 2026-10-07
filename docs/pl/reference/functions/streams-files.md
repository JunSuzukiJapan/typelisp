<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Strumienie i pliki

Traity i metody strumieni, konkretne typy strumieni, operacje na plikach i nazwy ścieżek. Gniazda sieciowe są
także strumieniami i opisano je w [Sieci](network.md).

## 1. Hierarchia traitów

To, co CL wyraża hierarchią klas, wyrażone jest tutaj **hierarchią traitów**. Zarówno
kierunek (wejście / wyjście), jak i typ elementu są ustalane **statycznie**, więc nie trzeba pytać
w czasie działania „czy ten strumień można czytać?".

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; wejście znakowe
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; wyjście znakowe
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; wejście, które potrafi odłożyć jeden znak z powrotem
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; wejście bajtowe
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; wyjście bajtowe
```

Funkcja czytająca znaki akceptuje dowolny typ strumienia, wbudowany lub zdefiniowany przez użytkownika, jeśli przyjmuje
`(where (CharInput S))` lub `:dyn CharInput`.

## 2. Metody

Każda metoda `CharInput` ma implementację domyślną. Implementacja pisze tylko `read-item`.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | Następny element. `none` na końcu. **Jedyna metoda, którą trzeba zaimplementować** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Następny znak |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Do następnej nowej linii (nowa linia jest zużywana i usuwana). Ostatnia linia niekończąca się nową linią też jest zwracana |
| `read-all` | `(read-all s)` | `(S)→string` | Wszystko, co zostało |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Tylko znak, który jest już pod ręką. `none` zamiast czekania |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Wpycha do `v` do `n` znaków i zwraca, ile faktycznie przeczytano. Mniej niż `n` tylko na końcu |

`listen` znajduje się w `InputStream` (rodzicu `CharInput`):

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Czy następny odczyt można obsłużyć bez czekania. Domyślnie `false`, **strona, która nigdy nie kłamie**: `true` byłoby zgadywaniem, a błędne zgadnięcie sprawiłoby, że `read-char-no-hang` by się blokowało. Wszystkie strumienie wbudowane to nadpisują. **Dla strumieni zdefiniowanych przez użytkownika, które tego nie nadpisują, `read-char-no-hang` zawsze zwraca `none`** |

`PeekInput` (dziedziczące z `CharInput`) dodaje **odkładanie jednego znaku z powrotem**. Tylko sam strumień
ma miejsce na przechowanie odłożonego znaku, więc nie może to mieć implementacji domyślnej i
jest osobnym traitem. `file-stream`/`string-input-stream`/`standard-stream` go implementują, a każdy inny
strumień dostaje go po opakowaniu za pomocą `make-peek-stream` (rozdział 4).

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Sprawia, że następny odczyt zwróci `c`. **Jedyna metoda, którą trzeba zaimplementować**. Jak w CL, gwarantowany jest tylko jeden znak |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Zerka na następny znak bez jego zużywania |

Podobnie dla `CharOutput` implementacja pisze tylko `write-item`.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Zapisuje jeden element. **Jedyna metoda, którą trzeba zaimplementować** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Zapisuje jeden znak |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Zapisuje łańcuch znaków |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Łańcuch i nowa linia |
| `terpri` | `(terpri s)` | `(S)→()` | Jedna nowa linia (nazwa z CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Jedna nowa linia, chyba że jesteśmy na początku linii |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Czy następny zapisany znak rozpocznie linię. Domyślnie `false` (więc `fresh-line` zapisuje nową linię: w razie wątpliwości zapis jest bezpieczną stroną). Wszystkie strumienie wbudowane to nadpisują |
| `finish-output` | `(finish-output s)` | `(S)→()` | Opróżnia bufor |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Zapisuje po kolei wszystkie znaki z `v` |

`at-line-start` pamięta **tylko to, co zapisano przez ten strumień**. `print`/`println`/
`(format true ...)` zapisują na standardowe wyjście bez przechodzenia przez `*standard-output*`, więc jeśli mieszasz
te dwa sposoby, `(fresh-line *standard-output*)` nie wie o nowych liniach zapisanych przez `println`. Trzymaj się
jednego z nich.

`Stream` jest wspólny dla wszystkich strumieni:

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Czy nadal jest otwarty |
| `close` | `(close s)` | `(S)→()` | Zamyka go. **GC nie zamyka strumieni**, więc rób to jawnie (lub za pomocą `with-open-file`) |

## 3. Konkretne typy strumieni

| Typ | Jak utworzyć | Zaimplementowane traity |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` to jedna z trzech stałych `direction-input` / `direction-output` /
`direction-append`. `open-file` zwraca `Err(FileError)`, jeśli pliku nie da się otworzyć (brakujący plik to
zwykły wynik, a nie panic). Nazwa pliku może być łańcuchem znaków lub `pathname` (`Pathish` w
rozdziale 9).

`(get-output-stream-string s)` zwraca to, co zapisano do `string-output-stream`, i je opróżnia.
Jak w CL, można je pobrać nawet po `close`.

**Bajtowe wejście/wyjście** używa `ByteInput`/`ByteOutput`. Ustalają one `Item` w `InputStream`/`OutputStream` na
`int`, tak samo jak `CharInput`/`CharOutput` ustalają go na `char`.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | Następny bajt. `none` na końcu pliku |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Zapisuje jeden bajt. Błąd poza zakresem 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | Wersja znakowa, w bajtach |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Jak wyżej |

CL ustala typ elementu **w wywołaniu**, jak w `(open name :element-type '(unsigned-byte 8))`,
ale tutaj typ elementu jest **typem** strumienia, więc różni się funkcja, która go otwiera.
Odczyt bajtów ze strumienia znakowego jest błędem typu (`string-input-stream` nie implementuje
`ByteInput`). Odczyt bajtu zaraz po odłożeniu znaku za pomocą `unread-char` też jest błędem.

## 4. Strumienie złożone

Wszystkie to `defstruct` w bibliotece standardowej i można je zagnieżdżać.

| Nazwa | Forma | Opis |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Zapisuje do wszystkich z `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Czyta z `in` i zapisuje do `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Czyta z `in` i zapisuje przeczytane znaki także do `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Czyta `Vector<:dyn CharInput>` jeden po drugim |
| `make-peek-stream` | `(make-peek-stream in)` | Dodaje jednoznakowe odłożenie do dowolnego `:dyn CharInput`, czyniąc z niego `PeekInput` (dla `read-sexpr`) |

## 5. Makra

| Nazwa | Forma | Opis |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Otwiera, wykonuje ciało, zamyka. `Result<wartość ciała, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Czyta z łańcucha znaków |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Zwraca to, co zapisano |

## 6. Funkcje generyczne i operacje na plikach

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Przenosi wszystko |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Wszystkie pozostałe linie |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Czyta jedno `Sexpr` (`read` z CL). `Ok(eof)` na końcu wejścia, `Ok(datum d)`, gdy coś przeczytano, `Err`, jeśli to nie są dane. **Zużywa ten jeden biały znak**, który zakończył daną (jak w CL). `ReadOutcome` nie jest `Option<Sexpr>`, aby wczytanie pustej listy `()` i koniec wejścia nie były tą samą wartością |
| `read-sexpr-preserving-whitespace` | Jak wyżej | Jak wyżej | To samo, ale zostawia biały znak (`read-preserving-whitespace` z CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Czyta do `ch` i tworzy listę. `ch` jest zużywane. `Err`, jeśli wejście się skończy |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Zapisuje po jednej linii |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | Cała zawartość |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Wszystkie linie |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Zapisuje |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Czy istnieje |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Usunięcie, zmiana nazwy (argumenty są `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Ścieżka bezwzględna z rozwiązanymi dowiązaniami symbolicznymi oraz `.`/`..`. `Err`, jeśli nie istnieje |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | Czas ostatniej modyfikacji. To **czas uniwersalny**, więc może go odczytać `decode-universal-time` ([Czas](system.md#2-dekodowanie-i-kodowanie-dat)) |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | Nazwa logowania właściciela. `Err`, jeśli plik nie istnieje, `Ok(none)`, jeśli uid właściciela nie ma wpisu w bazie haseł: dwa przypadki, które CL rozróżnia, są zachowane osobno |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Czy jest katalogiem. **Także `false`, jeśli nie istnieje**; użyj `probe-file`, aby je rozróżnić |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Wymienia zawartość według truename (ścieżka bezwzględna z rozwiązanymi dowiązaniami symbolicznymi, jak w `truename`). Dowiązania symboliczne, których cel nie istnieje, są pomijane. `.`/`..` są pomijane. Kolejność jest taka, jaką poda system |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Tworzy go razem z katalogami nadrzędnymi. Powodzenie, jeśli już istnieje |

Każdy argument nazywający plik **może być łańcuchem znaków lub `pathname`**. Jest to to samo traktowanie co desygnatory
nazw ścieżek z CL, rozstrzygane przez trait `Pathish`, a nie przez test typu w czasie działania
(rozdział 9).

Znak kończący `read-delimited-list` **kończy także tokeny**. Działa tylko na
głębokości 0: w `(1 2]` znak `]` jest czytany jako część własnego tekstu listy i zgłaszany jako uszkodzona lista.
Nie ma odpowiednika trzeciego argumentu `recursive-p` z CL.

## 7. Uczynienie własnego typu strumieniem

Napisz jedno `write-item`, a implementacje domyślne dostarczą resztę. Taki typ może też trafić do
strumieni złożonych.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; każda pozostała metoda jest domyślna

(write-line (counter::new 0) "four")   ; write-line, terpri i fresh-line wszystkie działają
```

Wejście działa tak samo: piszesz tylko `read-item`. Nawet typ bez własnego odkładania może być
czytany za pomocą `read` po opakowaniu, jak w `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Nazwa | Wywołanie | Typ | Opis |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` wczytuje znak `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Zwraca to, co jest zarejestrowane |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` wczytuje dwuznakową sekwencję `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Jak wyżej |

`F` to `(fn (string-input-stream char) Option<Sexpr>)`. Sposób użycia, moment wejścia w życie i różnice
względem CL opisano w [Referencji składni](../syntax.md#11-makra-czytnika-readtable).

## 9. Nazwy ścieżek `pathname`

Nazwa pliku podzielona na części. Przechowuje składniki katalogu rozdzielone `/`, nazwę, typ
(rozszerzenie) oraz informację, czy zaczyna się od korzenia.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   podział na ostatniej kropce
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Trait desygnatora nazwy ścieżki `Pathish`

Tam, gdzie CL akceptuje desygnator nazwy ścieżki (łańcuch znaków lub pathname), ten język akceptuje `Pathish`.
Implementują go zarówno `string`, jak i `pathname`, a **każda operacja na plikach przyjmuje go generycznie**, więc
`(open-input "a.txt")` i `(open-input p)` są zwykłymi wywołaniami (nie ma testu typu w czasie działania).
`namestring` łańcucha znaków po prostu zwraca go samego, więc dopóki przekazujesz łańcuch, żadne parsowanie nie zachodzi.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | Postać łańcuchowa. Trzeba zaimplementować |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Konwertuje na `pathname` (funkcja `pathname` z CL, przemianowana, ponieważ kolidowałaby z nazwą typu). Trzeba zaimplementować |

### 9.2 Funkcje

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Dzieli łańcuch znaków na części. Końcowe `/` (lub pusta nazwa) oznacza „brak nazwy", czyli katalog |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Buduje z samych podanych składników (wszystkie `&key`). Pominięta nazwa lub typ pozostają „nieobecne" i są czymś, co uzupełnia `merge-pathnames` |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Składniki katalogu, najbardziej zewnętrzny pierwszy |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Nazwa bez typu. `none` dla katalogu |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | To, co po ostatniej kropce. Kropka na początku się nie liczy (całe `.gitignore` to nazwa) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Czy zaczyna się od korzenia |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | Katalog domowy. `none`, jeśli nie ma `$HOME` (CL też dopuszcza `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | Część do ostatniego `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Tylko część `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Uzupełnia składniki brakujące w `p` z `default`. Względne `p` trafia pod katalog `default`; bezwzględne `p` zachowuje własny katalog |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | Postać względna wobec `default`. Całe `p`, jeśli nie jest pod bazą |

Argumenty typu mają wszędzie `(where (Pathish P))`.

## 10. Różnice względem CL

- **Hierarchia traitów, a nie hierarchia klas.** Nie ma `input-stream-p` / `output-stream-p`: typ
  niesie kierunek, więc nie jest to pytanie do zadawania w czasie działania.
- **`read` ma różne nazwy dla wersji łańcuchowej i strumieniowej.** `(read "...")` (odpowiada
  pierwszej wartości `read-from-string` z CL; jeśli potrzebujesz też pozycji, na której zakończył się odczyt, użyj
  `read-from-string`) i `(read-sexpr s)` (`read` z CL). Wywołanie jest rozstrzygane dla jednego typu odbiorcy, więc ta
  sama nazwa nie może być przeciążona.
- **Odkładanie z powrotem to osobny trait** (`PeekInput`), więc typy, które potrzebują tylko `read-char`, nie są zmuszane do
  implementowania `unread-char`.
- **Zamykanie jest jawne.** GC nie zamyka strumieni (GC działa w nieprzewidywalnych momentach, więc
  pozostawienie tego GC uczyniłoby także moment zamknięcia nieprzewidywalnym). Użycie `with-open-file` to
  bezpieczny sposób.
- **Nazwy ścieżek nie mają składników host, urządzenie ani wersja.** Nie ma nazw ścieżek z wieloznacznikami ani
  logicznych nazw ścieżek (`logical-pathname`). Separatorem jest zawsze `/`.
- **Funkcja `pathname` to `to-pathname`**, ponieważ typy, traity i funkcje dzielą jedną
  przestrzeń nazw.
- **Nie ma dopasowywania z wieloznacznikami**, więc `directory` to funkcja, która „wymienia zawartość tego
  katalogu" i nic więcej. `directory` z CL dopasowuje względem wzorca nazwy ścieżki.
