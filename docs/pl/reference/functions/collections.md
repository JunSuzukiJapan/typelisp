<!-- translated-from: docs/ja/reference/functions/collections.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Łańcuchy znaków, znaki i kolekcje

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` i `BitVector`.

## 1. Łańcuchy znaków `string`

Łańcuchy znaków są niezmienne.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Zamienia na wielkie litery (tylko ASCII). Jak `string-upcase` z CL, zwraca nowy łańcuch. Łańcuchy są niezmienne, więc nie ma destrukcyjnego `nstring-upcase`; to je zastępuje |
| `downcase` | `(downcase s)` | `string→string` | Zamienia na małe litery (tylko ASCII). Zastępuje `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Zamienia pierwszą literę każdego słowa na wielką, a resztę na małe (`string-capitalize` z CL). Słowo to maksymalny ciąg liter i cyfr |
| `length` | `(length s)` | `string→int` | Liczba znaków |
| `ref` | `(ref s i)` | `(string,int)→char` | Znak `i`. Powoduje panic poza zakresem |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | Podłańcuch `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Konkatenacja. Można podać trzy lub więcej (tak samo jak `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Porównanie leksykograficzne |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Ścisłe porównanie leksykograficzne „mniejsze niż" (tak samo jak `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Porównanie tożsamości (czy to ten sam obiekt, a nie ta sama zawartość) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Porównuje zawartość (z rozróżnianiem wielkości liter) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Porównuje zawartość (bez rozróżniania wielkości liter, tylko ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Czy zawartość się różni (`string/=` z CL. Forma o zmiennej liczbie argumentów porównuje sąsiednie pary) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Porządek bez rozróżniania wielkości liter (`string-lessp` i tak dalej z CL). Przy wspólnym prefiksie krótszy jest mniejszy |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Łańcuch złożony z `n` kopii `c` (`make-string` z CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | Pozycja, w której `sub` pojawia się po raz pierwszy. **`search` z CL ma argumenty w odwrotnej kolejności** (`(search pattern sequence)`). Pusty łańcuch jest znajdowany na pozycji 0. Słowa kluczowe opisano w [argumentach kluczowych funkcji na sekwencjach](sequences.md#6-argumenty-kluczowe) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | Pierwsza pozycja, w której się różnią. `none` tylko gdy są `equal`. Jeśli jeden jest prefiksem drugiego, koniec krótszego. Słowa kluczowe jak wyżej |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Usuwa znaki zawarte w `bag` z obu końców / z lewej / z prawej (`string-trim` i tak dalej z CL). Bez `bag` białe znaki `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Dzieli w `sep`. CL nie ma odpowiednika. Kolejne separatory dają puste elementy. Powoduje panic, jeśli `sep` jest pusty |
| `to-string` | `(to-string x)` | `T→string` | Konwertuje na łańcuch tak jak `~a`. Zaimplementowane dla `int`/`i32`/`f64`/`bool`/`char`/`string` (`princ-to-string` z CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Koduje jako UTF-8 (każdy element 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Dekoduje. `none`, jeśli to nie jest poprawny UTF-8 |

## 2. Znaki `char`

`char` to skalarna wartość Unicode. Zmiana wielkości liter i klasyfikacja obsługują tylko zakres ASCII.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Zamienia na wielką literę (tylko ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Zamienia na małą literę (tylko ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Porównanie według punktu kodowego |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Ścisłe „mniejsze niż" według punktu kodowego (tak samo jak `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Czy jest literą ASCII |
| `digitp` | `(digitp c)` | `char→bool` | Czy jest cyfrą ASCII |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Porównuje wartości |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Porównuje wartości, ignorując wielkość liter (`char-equal` z CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Czy wartości się różnią (`char/=` z CL. **Forma o zmiennej liczbie argumentów porównuje sąsiednie pary**, w przeciwieństwie do CL, które pyta, czy wszystkie pary się różnią) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Porządek bez rozróżniania wielkości liter (`char-lessp` i tak dalej z CL) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Wielka litera / mała litera / w ogóle rozróżnia wielkość liter (`upper-case-p` i tak dalej z CL) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Litera lub cyfra (ta sama nazwa co w CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Czy jest drukowalny. Obejmuje spację, nie nową linię ani tabulację (`graphic-char-p` z CL) |
| `standardp` | `(standardp c)` | `char→bool` | Czy jest jednym z 96 znaków standardowych CL, czyli `graphicp` plus nowa linia (`standard-char-p` z CL) |
| `char->int` | `(char->int c)` | `char→int` | Skalarna wartość Unicode (odwrotnością jest `int->char`/`try-int->char` w [Liczbach](numbers.md#1-liczby-całkowite-o-stałej-szerokości)). Odpowiada `char-code`/`char-int` z CL |
| `char->string` | `(char->string c)` | `char→string` | Łańcuch jednoznakowy. Funkcja `string` z CL to obejmuje, przyjmując desygnator, ale ten język nie ma desygnatorów, więc kierunek jest w nazwie |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | **Waga** cyfry w danej podstawie (`digit-char-p` z CL). `digitp` to osobna funkcja zwracająca `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Znak dla wagi `w`. Wielkie litery dla 10 i więcej (`digit-char` z CL; podstawa najwyżej 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Nazwa znaku. Nazwy mają tylko nazwane znaki, które czytnik potrafi wczytać (`char-name` z CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Znak dla nazwy. Bez rozróżniania wielkości liter, przyjmuje też aliasy czytnika (`linefeed`/`null`) (`name-char` z CL) |

Nie ma stałej odpowiadającej `char-code-limit` (górna granica `char` jest ustalana przez Unicode,
a nie przez język).

## 3. `Vector<T>`

Rosnąca tablica.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Tworzy pusty wektor. Argument typu pochodzi z oczekiwanego typu, więc w samym `let` napisz `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` kopii `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Dopisuje na końcu |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Odczytuje element `i`. Powoduje panic poza zakresem |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Zmienia element `i`. Powoduje panic poza zakresem. Można też napisać `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Liczba elementów |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Usuwa ostatni element i go zwraca. `None`, jeśli pusty (w przeciwieństwie do `get`/`set` nie powoduje panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Tworzy iterator implementujący `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Dopisuje `x`, jeśli nie istnieje równy element (`pushnew` z CL. Nie musi przepisywać miejsca, więc jest metodą, a nie makrem) |

`map`/`filter` i pokrewne to [funkcje na sekwencjach](sequences.md#4-funkcje-na-sekwencjach-oparte-na-iter): przepuść
wektor przez `iter`, jak w `(map (iter v) f)`. Operacje destrukcyjne (`nreverse`, `delete` i tak
dalej) znajdują się w [Operacjach destrukcyjnych](sequences.md#7-operacje-destrukcyjne).

## 4. `HashTable<K,V>`

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Tworzy pustą tablicę |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Wyszukiwanie |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Wstawienie lub nadpisanie |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Usuwa wpis i zwraca starą wartość, jeśli była |
| `count` | `(count h)` | `HashTable<K,V>→int` | Liczba wpisów |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Usuwa wszystko |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Migawka kluczy |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Migawka wartości |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Migawka par `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Iterator implementujący `Iter`. Elementami są `cons-cell` `(k . v)`. Odpowiada `with-hash-table-iterator` z CL; `doiter`/`map`/`filter` i inne działają na nim bez zmian |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | `maphash` z CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | `hash-table-size` z CL. W tej tablicy jest to liczba zajętych wpisów (równa `count`) |

**Kluczem może być dowolny typ implementujący `Hash`**, w tym typy `defstruct`/`defenum`. `get`/`set`/
`remove` mają `(where (Hash K))`, więc tablica z kluczem typu, który go nie implementuje, jest **błędem
typu** (`f64` nie ma `Hash` z powodu `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; zwraca nieujemną wartość mieszczącą się w fixnum
```

Zaimplementowane dla: `int` i sześciu liczb całkowitych o stałej szerokości, `bool`, `char`, `string` i `symbol` (nie dla
liczb zmiennoprzecinkowych). Dla własnych typów utrzymuj wynik nieujemny, wykonując na nim `logand` z
`*sxhash-mask*` (2^30-1). Aby zahaszować łańcuch znaków, możesz wywołać `(sxhash-string s)` (32-bitowy FNV-1a), którego
używa implementacja dla `string`.

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

To, czy dwa klucze są takie same, rozstrzyga **sam typ klucza** (`sxhash` oraz `equals` z `Eq`,
supertraitu `Hash`), a nie tożsamość obiektu. Dlatego, jak wyżej, można wyszukiwać za pomocą klucza,
który jest „inną wartością, ale równą".

Kolizje `sxhash` są dopuszczalne (kontrakt `Hash` działa tylko w jedną stronę: równe wartości muszą mieć
ten sam hasz). Kolidujące klucze są rozróżniane za pomocą `equals`.

## 5. `Array<T>` (tablice wielowymiarowe)

`defstruct` w bibliotece standardowej. Nie jest to typ wbudowany, więc wszystko, co można zrobić z
`defstruct`, można zrobić także z nim.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | `make-array` z CL. `dims` jest kopiowane. `init` to wartość początkowa każdej komórki (`:initial-element` z CL; ten język nie ma „niezwiązanej komórki", więc jest wymagane). `:fill-pointer` tylko dla jednego wymiaru |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | `aref` / `(setf (aref …))` z CL. Powoduje panic, jeśli indeks jest poza zakresem |
| `aref` | `(aref a i j …)` | — | Zapis z CL z samymi indeksami. Rozwija się do powyższych `get`/`set`. `(setf (aref a i j) v)` też działa |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | `row-major-aref` z CL. Płaski indeks |
| `rank` | `(rank a)` | `Array<T>→int` | `array-rank` z CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | `array-dimension` z CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | `array-dimensions` z CL. Zwraca **kopię**, tak jak CL zwraca świeżą listę |
| `total-size` | `(total-size a)` | `Array<T>→int` | `array-total-size` z CL (liczba zaalokowanych komórek, niezależna od wskaźnika wypełnienia) |
| `len` | `(len a)` | `Array<T>→int` | `length` z CL na tablicach. Wskaźnik wypełnienia, jeśli jest, w przeciwnym razie `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | `array-in-bounds-p` z CL. Fałsz (nie błąd), nawet gdy **liczba** indeksów jest zła |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | `array-row-major-index` z CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | `adjust-array` z CL. Rangi nie można zmienić. Elementy, które pozostają w zakresie, są zachowane pod swoimi indeksami, a nowe komórki dostają `init`. W przeciwieństwie do CL nie zwraca tablicy (każda tablica w tym języku jest dostosowywalna, więc nie ma drugiej tablicy do zwrócenia) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | `vector-push-extend` z CL. Powoduje panic bez wskaźnika wypełnienia |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | `vector-pop` z CL. `none`, jeśli pusta |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Wskaźnik wypełnienia (`none`, jeśli go nie ma). Można go zapisać za pomocą `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Iterator w porządku wierszowym (row-major). Zatrzymuje się na wskaźniku wypełnienia, jeśli jest |

- **Indeksy to `Vector<int>`.** Metoda nie może zadeklarować „tego samego typu argumentu powtórzonego dowolną
  liczbę razy na końcu", a lukier składniowy `aref` wypełnia tę lukę.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **nie istnieją**. Statyczny typ odbiorcy już odpowiada na te pytania.
- `Array::new` to konstruktor w kolejności pól generowany przez `defstruct` i nie służy do tworzenia
  tablic. Użyj `Array::make`.
- **Tablice są wypisywane w składni tablic CL.** Ranga 1 to `#(1 2 3)`; inne rangi to `#nA`, po którym następuje
  tyle poziomów nawiasów (`#2A((1 2 3) (4 5 6))`); ranga 0 to `#0A5`. Wypisywanie zatrzymuje się na wskaźniku
  wypełnienia, jeśli jest. Ustawienie `*print-array*` ([Wypisywanie](printing.md#6-kontrolowanie-ile-jest-wypisywane))
  na fałsz wypisuje tylko kształt, `#<array 2x3>`. Tylko tablica, której elementami są `defstruct`
  bez `print-object`, wypisuje się we wbudowanej postaci `#<array<...> ...>` (nie jest to błąd).

## 6. `BitVector` (wektory bitowe)

Ciąg bitów o stałej długości. `defstruct` w bibliotece standardowej.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Długość `n`, wszystkie bity 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Powodują panic poza zakresem |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | Zapisy z CL. `(setf (bit v i) b)` też działa. `sbit` z CL różni się od `bit` tylko wymaganiem prostego wektora bitowego, ale ten język ma tylko jeden rodzaj wektora bitowego |
| `len` | `(len v)` | `BitVector→int` | Liczba bitów |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Zwracają nowy wektor bitowy. Powodują panic, jeśli długości się różnią. Nie ma trzeciego argumentu jak w CL (miejsca docelowego wyniku) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Dopełnienie |

Nie ma `bit-vector-p` (odpowiada na to typ statyczny).
