<!-- translated-from: docs/ja/reference/functions/numbers.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Liczby

Operacje na liczbach całkowitych, zmiennoprzecinkowych, wymiernych, zespolonych i wartościach logicznych oraz inne
funkcje związane z liczbami. Informacje o czytaniu form wywołania znajdziesz w [Funkcjach wbudowanych](README.md).

## 1. Liczby całkowite o stałej szerokości

Istnieje siedem typów całkowitych: **`int`** (`integer` z CL: dowolna precyzja i domyślny typ
literałów całkowitych bez adnotacji; rozdział 3) oraz typy o stałej szerokości `i8` `i16` `i32` `u8` `u16` `u32`.
To, dla którego typu operacja jest rozstrzygana, zależy od typu pierwszego argumentu (typy są
niezależne od siebie, bez niejawnych konwersji). **Nie ma 64-bitowego typu całkowitego.** Wartość w
czasie działania to jedno słowo, którego niskie bity są znacznikiem, więc dla natychmiastowej liczby
całkowitej zostaje tylko 63 bity, a typ deklarujący 64 bity musiałby gdzieś zgubić najwyższy bit. `int` staje się bignum,
gdy przekroczy te 63 bity, więc jeśli szerokość nie ma znaczenia, użyj `int`. Poniższa tabela dotyczy
sześciu typów o stałej szerokości (tabela dla `int` znajduje się w rozdziale 3).

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(T,T)→T` | Cztery działania arytmetyczne. `/` obcina w stronę zera i powoduje panic przy dzieleniu przez zero |
| `mod` | `(mod a b)` | `(T,T)→T` | Reszta (`mod` z CL, **dzielenie z podłogą**: znak zgodny z dzielnikiem. `(mod -7 3)`→`2`). Powoduje panic przy dzieleniu przez zero |
| `rem` | `(rem a b)` | `(T,T)→T` | Reszta (`rem` z CL, **dzielenie z obcięciem**: znak zgodny z dzielną. `(rem -7 3)`→`-1`). Powoduje panic przy dzieleniu przez zero |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(T,T)→cons-cell<T,T>` | Odpowiadają dwuargumentowym `floor`/`ceiling`/`round`/`truncate` z CL (`(floor 7 2)`→iloraz 3, reszta 1). Zamiast wielu wartości zwracają iloraz i resztę w `cons-cell` (`car`=iloraz, `cdr`=reszta). `round-div` zaokrągla połówki do parzystej, jak CL |
| `abs` | `(abs x)` | `T→T` | Wartość bezwzględna |
| `signum` | `(signum x)` | `T→T` | Znak (`1`/`-1`/`0`) |
| `gcd` | `(gcd a b)` | `(T,T)→T` | Największy wspólny dzielnik |
| `lcm` | `(lcm a b)` | `(T,T)→T` | Najmniejsza wspólna wielokrotność (0, jeśli któraś jest 0) |
| `max` `min` | `(op a b)` | `(T,T)→T` | Większa / mniejsza (trzy lub więcej argumentów jest rozwijane za pomocą lukru dla zmiennej liczby argumentów z rozdziału 8) |
| `1+` `1-` | `(op x)` | `T→T` | `x±1` |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(T,T)→bool` | Porównanie |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(T,T)→bool` | Wszystkie takie same jak `=` (dla liczb tego samego typu nie ma różnicy) |
| `int->float` | `(int->float x)` | `T→f64` | Konwersja rozszerzająca do `f64` |
| `int->int` | `(int->int x)` | `T→int` | Konwersja rozszerzająca do `int` (zawsze dokładna). To, co robi `(as int x)` |
| `int->ratio` | `(int->ratio x)` | `T→ratio` | Konwersja rozszerzająca do `ratio` (zawsze dokładna) |
| `int->char` | `(int->char x)` | `T→char` | Interpretuje wartość jako skalarną wartość Unicode. Powoduje panic przy niepoprawnej wartości |
| `try-int->char` | `(try-int->char x)` | `T→Option<char>` | Wersja `int->char` zwracająca `None` przy niepowodzeniu |
| `int->i8` `int->i16` `int->i32` `int->u8` `int->u16` `int->u32` | `(int->W x)` | `T→W` | Konwersja szerokości. Wartości, które się nie mieszczą, są obcinane (jak `as` w Rust) |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Ta sama konwersja w postaci pytania. `None`, jeśli wartość nie mieści się w tej szerokości |

Te konwersje to także to, co robią formy specjalne `(as Type x)`/`(try-as Type x)`
([Referencja składni](../syntax.md#7-pozostałe-formy-specjalne)). Operacje bitowe (`logand`/`ash`/`ldb` i tak
dalej) oraz predykaty (`zerop`/`evenp` i tak dalej) mają ten sam kształt dla różnych typów, więc zebrano je
w rozdziałach 11 i 9.

`i8` `i16` `u8` `u16` `u32` mają dokładnie tabelę z tego rozdziału, a `f32` ma dokładnie tabelę `f64`
z rozdziału 4.

**Nazwa typu oznacza jego szerokość i znakowość, nic więcej.** `i32` oznacza „traktuj 32 bity jako ze znakiem",
a `u32` oznacza „traktuj 32 bity jako bez znaku". `(+ (the u8 200) (the u8 100))` to `44`,
`(+ 2147483647 1)` (jako `i32`) to `-2147483648`, a `(lognot (the u32 0))` to `4294967295`. `f32` jest
takie samo: prawdziwe binary32. `(/ (the f32 1.0) (the f32 3.0))` wypisuje się jako `0.33333334`, co jest inną
wartością niż wynik `f64`, `0.3333333333333333`.

Pochodny katalog CL (`abs`/`signum`/`gcd`/`lcm`/`isqrt`/`expt` i predykaty z rozdziału 9)
istnieje dla `int`/`i32`/`f64`/`ratio`. Jeśli potrzebujesz go dla innej szerokości, przejdź za pomocą `(as int x)` /
`(as i32 x)` (konwersje szerokości istnieją dla każdej pary).

## 2. Surowe słowa na granicy z C (`ptr` / `c-long` / `c-ulong`)

Trzy typy używane wyłącznie do przekazywania wartości do funkcji C zadeklarowanych za pomocą
[`defffi`](../syntax.md#33-defffi--deklarowanie-funkcji-c-ffi) i z nich. `ptr` to nieprzezroczysty wskaźnik, a
`c-long` / `c-ulong` to `long` / `unsigned long` z C. Uczynienie któregoś wartością wymaga bycia wewnątrz
`(unsafe ...)`.

**Nie ma arytmetyki.** Żadna z tabeli rozdziału 1 nie ma zastosowania: nie da się napisać ani `(+ p 1)`, ani `(< n m)`.
To słowa do przekazania do C, a nie typy do obliczeń, więc aby obliczać, przejdź do
typu o określonej szerokości. `c-long` / `c-ulong` mają tylko konwersje:

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `int->i8` … `int->u32` | `(int->W x)` | `T→W` | Te same konwersje szerokości co w rozdziale 1. Wartości, które się nie mieszczą, są obcinane |
| `try-int->i8` … `try-int->u32` | `(try-int->W x)` | `T→Option<W>` | Ta sama konwersja w postaci pytania |
| `int->c-long` `int->c-ulong` | `(int->W x)` | `T→W` | Droga wejścia, z drugiego surowego słowa i z typów całkowitych z rozdziału 1 |
| `try-int->c-long` `try-int->c-ulong` | `(try-int->W x)` | `T→Option<W>` | Jak wyżej |
| `int->int` | `(int->int x)` | `T→int` | **Zawsze dokładna**. Uczciwy sposób odczytania `size_t`, które nie mieści się w `i32` |

`(as i32 x)` / `(try-as i32 x)` / `(as int x)` / `(as c-ulong n)` robią to samo, a
konwersje istnieją dla każdej pary z typami całkowitymi z rozdziału 1. `ptr` nie ma nawet tej
tabeli: nie ma sposobu odczytania wskaźnika jako liczby. Jest to wartość, która jest tylko przekazywana, odbierana
i przekazywana dalej do innej funkcji C.

**Nie można ich także wypisać.** `(println "~a" x)` nie przyjmuje surowego słowa (nie ma ono reprezentacji
`Sexpr`), więc najpierw przejdź do typu o określonej szerokości, jak w `(println "~a" (as int n))`.

„Nie ma 64-bitowego typu całkowitego" z początku rozdziału 1 dotyczy także tych trzech. Obowiązuje
**dlatego, że nie można ich przechowywać**: nie mogą być polem `defstruct`, `defvar`, wewnątrz argumentu typu
ani wewnątrz `Sexpr`, więc są słowami, które tylko przechodzą przez funkcję jako argumenty,
wartości zwracane i zmienne lokalne. Szczegóły znajdziesz w
[Referencji składni](../syntax.md#ptr--c-long--c-ulong--surowe-słowa-maszynowe).

## 3. Liczby całkowite o dowolnej precyzji `int`

`integer` z CL i **liczba całkowita** tego języka: literały całkowite bez adnotacji mają ten typ, a
wbudowane funkcje zwracające liczbę, takie jak `length` i `char->int`, zwracają ten typ. Wartość jest przechowywana
jako 63-bitowa wartość natychmiastowa (fixnum), dopóki się mieści, jest automatycznie awansowana do bignum, gdy
wynik operacji już się nie mieści, i wraca do wartości natychmiastowej, gdy znów się mieści. `eq`
to zawsze tożsamość wartości w zakresie fixnum, a `eql`/`=` to tożsamość numeryczna w całym
zakresie. Jest to inny typ niż typy całkowite o stałej szerokości (rozdział 1), bez niejawnej
konwersji: `(as int x)` to dokładne rozszerzenie ze stałej szerokości, a `(as i32 n)` /
`(try-as i32 n)` to obcięcie / sprawdzenie z `int` (to samo znaczenie co `int->W` / `try-int->W`
w rozdziale 1).

Wariant całkowity `Sexpr` to także po prostu `int` (`(int n)` przyjmuje zarówno fixnum, jak i bignum).

Wbudowane funkcje przyjmujące indeks lub liczbę (`substring`, `get` z `Vector`, liczba przesunięć `ash` i
tak dalej) akceptują `int`, ale przekazanie wartości, która nie mieści się w fixnum, jest błędem czasu działania („an integer
argument does not fit a fixnum").

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `+` `-` `*` | `(op a b)` | `(int,int)→int` | Nigdy się nie przepełniają (awansują) |
| `/` | `(/ a b)` | `(int,int)→int` | Obcina w stronę zera. Powoduje panic przy dzieleniu przez zero |
| `mod` | `(mod a b)` | `(int,int)→int` | Reszta z dzielenia z podłogą (znak zgodny z dzielnikiem) |
| `max` `min` | `(op a b)` | `(int,int)→int` | |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(int,int)→bool` | |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(int,int)→bool` | Wszystkie to `=` |
| `logand` `logior` `logxor` `lognot` `logtest` `logcount` `integer-length` `logbitp` `ash` | | | Tak samo jak w rozdziale 11 (uzupełnienie do dwóch z nieskończenie wieloma bitami) |
| `int->float` `int->ratio` `int->char` `try-int->char` | | | Tak samo jak w rozdziale 1 |
| `int->W` `try-int->W` | `(int->W x)` | `int→W` | Obcięcie / sprawdzenie. `W` to jedna z sześciu szerokości lub `c-long`/`c-ulong` |
| `int->int` | | `int→int` | Tożsamość (po stronie stałych szerokości i słów C `int->int` rozszerza; rozdział 1) |
| `abs` `signum` `rem` `gcd` `lcm` `expt` `1+` `1-` | | | Ten sam kształt co w rozdziale 1. `expt` przyjmuje tylko nieujemne wykładniki |

## 4. Liczby zmiennoprzecinkowe (`f64` / `f32`)

`f32` ma tę samą tabelę.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(f64,f64)→f64` | IEEE-754. Dzielenie przez zero nie powoduje panic; daje `inf`/`NaN` |
| `mod` | `(mod a b)` | `(f64,f64)→f64` | Reszta z dzielenia z podłogą (jak w CL; znak zgodny z dzielnikiem. `a - b*floor(a/b)`) |
| `rem` | `(rem a b)` | `(f64,f64)→f64` | Reszta z dzielenia z obcięciem (jak w CL; znak zgodny z dzielną. `a - b*truncate(a/b)`) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(f64,f64)→bool` | Porównanie |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(f64,f64)→bool` | Wszystkie takie same jak `=` |
| `expt` | `(expt a b)` | `(f64,f64)→f64` | Potęga |
| `abs` | `(abs x)` | `f64→f64` | Wartość bezwzględna |
| `signum` | `(signum x)` | `f64→f64` | Znak (`1.0`/`-1.0`; `±0.0`/`NaN` są zwracane bez zmian. Jak w CL, w przeciwieństwie do `signum` z Rust) |
| `max` `min` | `(op a b)` | `(f64,f64)→f64` | Większa / mniejsza (trzy lub więcej argumentów jest rozwijane za pomocą lukru dla zmiennej liczby argumentów z rozdziału 8) |
| `1+` `1-` | `(op x)` | `f64→f64` | `x±1.0` |
| `sqrt` `floor` `ceiling` `round` `truncate` | `(op x)` | `f64→f64` | Operacje jednoargumentowe |
| `exp` `log` `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` | `(op x)` | `f64→f64` | Funkcje przestępne. `log` to logarytm naturalny |
| `log` (dwa argumenty) | `(log x base)` | `(f64,f64)→f64` | Logarytm o zadanej podstawie. Rozwijany do `(/ (log x) (log base))` (rozdział 8) |
| `floor-div` `ceiling-div` `round-div` `truncate-div` | `(op a b)` | `(f64,f64)→cons-cell<f64,f64>` | Odpowiadają wersjom dwuargumentowym z CL (`(floor 7.0 2.0)`→iloraz 3, reszta 1). Ten sam projekt co funkcje o tych samych nazwach w rozdziale 1 (`car`=iloraz, `cdr`=reszta) |
| `float->int` | `(float->int x)` | `f64→int` | Konwertuje na `int` przez obcięcie w stronę zera (`truncate` z CL; dokładne dla skończonych wartości dowolnej wielkości). Powoduje panic dla nieskończoności i NaN. Dla stałej szerokości użyj `(as i32 x)` |
| `float->ratio` | `(float->ratio x)` | `f64→ratio` | Konwertuje na `ratio` jako dokładną binarną liczbę wymierną (`rational` z CL) |
| `float->f32` `float->f64` | `(op x)` | `f64→f32` / `f64→f64` | Konwertuje między szerokościami zmiennoprzecinkowymi. `float->f32` zaokrągla do najbliższej, `float->f64` jest zawsze dokładne. To, co robi `(as f32 x)` |
| `try-float->f32` `try-float->f64` | `(op x)` | `f64→Option<f32>` / `→Option<f64>` | Ta sama konwersja w postaci pytania. `none`, jeśli zaokrąglenie zmienia wartość (rozszerzenie do `f64` zawsze daje `some`). To, co robi `(try-as f32 x)` |
| `ffloor` `fceiling` `fround` `ftruncate` | `(op x)` | `f64→f64` | Funkcje o tych samych nazwach z CL. Aliasy powyższych `floor`/`ceiling`/`round`/`truncate`: w CL te bez prefiksu zwracają liczby całkowite, więc te z prefiksem `f` odpowiadają zachowaniu tego języka |
| `float-radix` `float-digits` `float-precision` | `(op x)` | `f64→int` | Odpowiednio 2 / 53 / 53 (tylko precyzja `0.0` to 0). `f64` to zawsze IEEE-754 binary64, więc są to stałe |
| `float-sign` | `(float-sign x)` | `f64→f64` | `1.0` lub `-1.0` |
| `scale-float` | `(scale-float x n)` | `(f64,int)→f64` | `x * 2^n` |
| `decode-float` | `(decode-float x)` | `f64→cons-cell<f64,int>` | Mantysa (w `[1/2,1)`, bez znaku) i wykładnik. CL zwraca trzy wartości, ale nie ma wielu wartości, więc znak pozostawiono `float-sign` |
| `integer-decode-float` | `(integer-decode-float x)` | `f64→cons-cell<int,int>` | Ten sam rozkład z dokładną 53-bitową całkowitą mantysą. `mantissa * 2^exponent` to dokładnie oryginalna wartość |
| `rationalize` | `(rationalize x)` | `f64→ratio` | **Najprostsza liczba wymierna, która po wczytaniu daje tę liczbę zmiennoprzecinkową** (`(rationalize 0.1)` to `1/10`). Dla dokładnej wartości binarnej użyj `float->ratio` |

**Różnica względem CL: sposób zaokrąglania `round`.** `round` (a więc `fround`/`round-div`) zaokrągla **od
zera** (`(round 2.5)` = `3.0`). CL zaokrągla **do parzystej**, dając `2`.

## 5. Liczby wymierne `ratio`

Zgodne z CL liczby wymierne o dowolnej precyzji. Są zawsze przechowywane w postaci nieskracalnej z dodatnim
mianownikiem i alokowane na stercie. Nie ma niejawnej konwersji z typami całkowitymi ani
`f64` (użyj jawnej metody konwersji lub `as`/`try-as`). Składnię literałów ułamkowych opisano w
[Referencji składni](../syntax.md#1-elementy-leksykalne).

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `+` `-` `*` `/` | `(op a b)` | `(ratio,ratio)→ratio` | Cztery działania (wyniki zawsze w postaci nieskracalnej). `/` powoduje panic przy dzieleniu przez zero |
| `mod` | `(mod a b)` | `(ratio,ratio)→ratio` | Reszta z dzielenia z podłogą (jak w CL; znak zgodny z dzielnikiem) |
| `rem` | `(rem a b)` | `(ratio,ratio)→ratio` | Reszta z dzielenia z obcięciem (jak w CL; znak zgodny z dzielną) |
| `abs` | `(abs x)` | `ratio→ratio` | Wartość bezwzględna |
| `signum` | `(signum x)` | `ratio→ratio` | Znak (zwraca `1`/`-1`/`0` jako `ratio`) |
| `expt` | `(expt a b)` | `(ratio,ratio)→ratio` | Potęga. Wykładnik musi być `ratio` o wartości całkowitej (w przeciwnym razie panic). Ujemny wykładnik daje odwrotność |
| `max` `min` | `(op a b)` | `(ratio,ratio)→ratio` | Większa / mniejsza |
| `1+` `1-` | `(op x)` | `ratio→ratio` | `x±1`. `ratio` nie ma operacji bitowych (w CL są tylko dla liczb całkowitych) |
| `<` `<=` `>` `>=` `=` `/=` | `(op a b)` | `(ratio,ratio)→bool` | Porównanie |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(ratio,ratio)→bool` | Wszystkie takie same jak `=` |
| `numerator` | `(numerator x)` | `ratio→int` | Licznik w postaci nieskracalnej (ta sama nazwa co w CL) |
| `denominator` | `(denominator x)` | `ratio→int` | Mianownik w postaci nieskracalnej (zawsze dodatni) |
| `ratio->int` | `(ratio->int x)` | `ratio→int` | Część całkowita (obcięta w stronę zera) |
| `ratio->float` | `(ratio->float x)` | `ratio→f64` | Konwertuje na `f64` |

Drogami wejścia z liczb całkowitych o stałej szerokości i z `f64` są `int->int`/`int->ratio` (rozdział 1) oraz
`float->int`/`float->ratio` (rozdział 4). `int`/`ratio` to osobne typy niezależne od `i32` i
pozostałych, a mieszana arytmetyka wymaga jawnych konwersji.

## 6. Liczby zespolone `complex`

Struktura (`defstruct`) w bibliotece standardowej.

**Dwie różnice względem CL** (obie wynikają z typowania statycznego):

1. **Składowe są zawsze `f64`.** Zespolona z CL może też przechowywać liczby wymierne, a `(complex 1 2)` i
   `(complex 1.0 2.0)` to różne typy. Typ statyczny musi wybrać jeden, a funkcje
   przestępne zwracają rodzaj zmiennoprzecinkowy.
2. **`(sqrt -1.0)` to rzeczywiste `sqrt` (NaN).** W CL `sqrt` może zwrócić liczbę zespoloną z rzeczywistej,
   ale `sqrt` z `f64` musi zwracać `f64`. Wynik zespolony pochodzi z argumentu zespolonego:
   `(sqrt (complex -1.0 0.0))` to `i`.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `complex` / `complex::new` | `(complex re im)` | `(f64,f64)→complex` | Konstrukcja. Składowe można odczytać bezpośrednio jako `z::re`/`z::im` |
| `realpart` `imagpart` | `(op z)` | `complex→f64` | Część rzeczywista i urojona. **Działają też na liczbach rzeczywistych** (`(realpart 3.0)`→`3.0`, `(imagpart 3.0)`→`0.0`), jak w CL |
| `conjugate` | `(conjugate z)` | `complex→complex` | Sprzężenie (działa też na liczbach rzeczywistych) |
| `phase` | `(phase z)` | `complex→f64` | Argument w (-pi,pi] (działa też na liczbach rzeczywistych) |
| `cis` | `(cis theta)` | `f64→complex` | `e^(i*theta)` |
| `abs` | `(abs z)` | `complex→f64` | Wartość bezwzględna. **Jedyne `abs`, które nie zwraca typu odbiorcy** (jak w CL, moduł liczby zespolonej jest rzeczywisty) |
| `+` `-` `*` `/` | `(op z w)` | `(complex,complex)→complex` | Arytmetyka zespolona |
| `=` `/=` | `(op z w)` | `(complex,complex)→bool` | Równość składowa po składowej. `Eq` jest także zaimplementowane (nie ma `Ord`: liczby zespolone nie mają porządku, a `<` z CL także ich nie przyjmuje) |
| `zerop` | `(zerop z)` | `complex→bool` | Czy obie składowe są 0 |
| `exp` `log` `sqrt` | `(op z)` | `complex→complex` | `log`/`sqrt` dają wartości główne |
| `expt` | `(expt z w)` | `(complex,complex)→complex` | `exp(w log z)`. `(expt 0 0)`=1 |
| `atan2` | `(atan2 y x)` | `(f64,f64)→f64` | Kąt wektora `(x,y)`. **Dwuargumentowe `(atan y x)` z CL jest lukrem dla tej funkcji** (rozgałęzia się według liczby argumentów, jak dwuargumentowe `log`) |

Implementuje `print-object`, więc `~a`/`~s` wypisują je jako `#C(re im)`, tak jak CL (czytnik tego języka
nie ma składni `#C`, by wczytać je z powrotem).

## 7. Wartości logiczne

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `not` | `(not b)` | `bool→bool` | Negacja |
| `eq` `eql` `equal` `equalp` | `(op a b)` | `(bool,bool)→bool` | Wszystkie porównują wartości pod kątem równości |

`and`/`or` wymagają obliczania skróconego, więc są formami specjalnymi
([Referencja składni](../syntax.md#4-wiązanie-i-instrukcje-warunkowe)).

## 8. Pomocnicze funkcje liczbowe i lukier wywołań

`abs`/`signum` (wszystkie typy liczbowe), `gcd`/`lcm` (tylko typy całkowite), `rem` (wszystkie typy rzeczywiste, w tym
`f64`) i `expt` (`int`/`f64`/`ratio`) są zdefiniowane jako metody każdego typu liczbowego (rozstrzygane według
typu odbiorcy: `(abs x)` to metoda dla typu `x`). Szczegóły dla każdego typu znajdują się w
rozdziałach 1, 3, 4 i 5. Liczby całkowite o stałej szerokości nie mają `expt` (nie mają awansu i
by się przepełniały; przejdź do `int` za pomocą `(as int x)` i użyj jego `expt`).

### 8.1 Formy o zmiennej liczbie argumentów oraz z 0/1 argumentem

Arytmetyka i porównania w CL mają zmienną liczbę argumentów, ale metody są rozstrzygane tylko według typu odbiorcy,
a nie liczby argumentów. Dlatego **moduł sprawdzający rozwija poniższe formy do wywołań
dwuargumentowych**.

| Forma, którą możesz napisać | Rozwinięcie | Dotyczy |
|---|---|---|
| `(op a b c ...)` | Lewostronne złożenie `(op (op a b) c)` | `+` `-` `*` `/` `max` `min` `logand` `logior` `logxor` `gcd` `lcm` |
| `(cmp a b c ...)` | `(and (cmp a b) (cmp b c) ...)` z każdym wyrazem związanym z wartością tymczasową | `<` `<=` `>` `>=` `=` `/=` |
| `(op)` | `(+)`=0 / `(*)`=1 / `(logior)`=`(logxor)`=0 / `(logand)`=-1 / `(gcd)`=0 / `(lcm)`=1 | Te z powyższych, które mają element neutralny |
| `(op x)` | Dla `+ * max min logand logior logxor` samo `x`. `(- x)` neguje, `(/ x)` daje odwrotność, `(gcd x)`/`(lcm x)` dają `(abs x)` (jak w CL) | Jak wyżej |
| `(cmp x)` | Oblicza `x` i daje `true` | `<` `<=` `>` `>=` `=` `/=` |
| `(log x base)` | `(/ (log x) (log base))` | `f64` |

Każdy wyraz jest obliczany dokładnie raz, od lewej do prawej (dlatego porównania o zmiennej liczbie argumentów
przechodzą przez wartości tymczasowe). Forma `/=` o zmiennej liczbie argumentów porównuje **sąsiednie pary**, w przeciwieństwie do CL, które pyta,
czy wszystkie pary się różnią.

### 8.2 `isqrt` i całkowite `expt`

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `isqrt` | `(isqrt n)` | `T→T` | Największa liczba całkowita nieprzekraczająca pierwiastka kwadratowego. Powoduje panic przy wartości ujemnej |
| `expt` | `(expt n e)` | `(T,T)→T` | Potęga (przez podnoszenie do kwadratu). CL zwraca liczbę wymierną dla ujemnego wykładnika, ale typ całkowity nie może jej reprezentować, więc następuje panic; najpierw przekonwertuj na `ratio` |

## 9. Predykaty

| Nazwa | Forma | Typ | Typy |
|---|---|---|---|
| `zerop` `plusp` `minusp` | `(op x)` | `T→bool` | `int` `i32` `f64` `ratio` |
| `evenp` `oddp` | `(op x)` | `T→bool` | `int` `i32` (tylko typy całkowite, jak w CL) |

**Nie ma predykatów typów** w rodzaju `numberp`/`integerp`/`floatp` z CL. Przy typowaniu statycznym typ
wartości jest już ustalony bez pytania w czasie działania.

## 10. Stałe

| Nazwa | Typ | Wartość |
|---|---|---|
| `pi` | `f64` | `3.141592653589793` |
| `boole-clr` `boole-set` `boole-1` `boole-2` `boole-c1` `boole-c2` `boole-and` `boole-ior` `boole-xor` `boole-eqv` `boole-nand` `boole-nor` `boole-andc1` `boole-andc2` `boole-orc1` `boole-orc2` | `int` | Kody operacji przekazywane do `boole` (zamiast słów kluczowych z CL) |

Stałe graniczne liczb (CLHS 12.1.4.2 / 12.1.3):

| Nazwa | Typ | Opis |
|---|---|---|
| `most-positive-fixnum` / `most-negative-fixnum` | `int` | Górna / dolna granica 63-bitowej wartości natychmiastowej (2^62-1 / -2^62). `int` poza nimi staje się bignum |
| `most-positive-double-float` / `most-negative-double-float` | `f64` | Największa / najmniejsza wartość skończona |
| `least-positive-double-float` / `least-negative-double-float` | `f64` | Najmniejszy niezerowy moduł, w tym liczby subnormalne |
| `least-positive-normalized-double-float` / `least-negative-normalized-double-float` | `f64` | To samo, ograniczone do liczb znormalizowanych |
| `double-float-epsilon` / `double-float-negative-epsilon` | `f64` | Zgodne z definicją z CL (najmniejsze dodatnie `e` z `(/= (+ 1 e) 1)`), więc są **o jeden ULP większe niż** 2^-53: samo 2^-53 zaokrągla się z powrotem do `1.0` przy zaokrąglaniu do najbliższej parzystej |

## 11. Operacje bitowe

Zdefiniowane na uzupełnieniu do dwóch z nieskończenie wieloma bitami (CL 12.10). Są zaimplementowane dla
typów całkowitych o stałej szerokości i `int`, nie dla `ratio` (CL ma operacje bitowe także tylko dla liczb całkowitych).

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `logand` `logior` `logxor` | `(op a b)` | `(T,T)→T` | Bitowe and, or, exclusive or (wersje o zmiennej liczbie argumentów i bezargumentowe w 8.1) |
| `lognot` | `(lognot x)` | `T→T` | Dopełnienie bitowe |
| `ash` | `(ash x count)` | `(T,int)→T` | Przesunięcie arytmetyczne. W lewo, jeśli `count` jest dodatnie, w prawo, jeśli ujemne |
| `logbitp` | `(logbitp x index)` | `(T,int)→bool` | Czy bit `index` jest ustawiony (**kolejność argumentów jest odwrotna niż w CL**; zobacz niżej) |
| `logtest` | `(logtest a b)` | `(T,T)→bool` | `(/= (logand a b) 0)` |
| `logcount` | `(logcount x)` | `T→T` | Liczba ustawionych bitów (dla liczby ujemnej liczba bitów 0) |
| `integer-length` | `(integer-length x)` | `T→T` | Liczba bitów potrzebnych do reprezentacji, bez liczenia znaku |
| `logeqv` `lognand` `lognor` `logandc1` `logandc2` `logorc1` `logorc2` | `(op a b)` | `(T,T)→T` | Pozostałe siedem, złożone z powyższych |

**Tylko drugi argument `ash` jest typu `int`, a nie `T`.** Jest to **odległość** w bitach, a nie
wartość typu odbiorcy, więc szerokość i znakowość odbiorcy nic nie mówią o odległości
(z tego samego powodu, dla którego `count` w `(ash integer count)` z CL jest dowolną liczbą całkowitą). Przesunięcie wartości bez znaku
w prawo jest przesunięciem logicznym (`(ash (the u8 200) -3)` = `25`), a wartości ze znakiem przesunięciem arytmetycznym
zaokrąglającym w stronę minus nieskończoności (`(ash (the i32 -100) -4)` = `-7`). `index` w `logbitp`
jest typu `int` z tego samego powodu.

**Specyfikatory bajtów.** Zamiast nieprzezroczystego obiektu zwracanego przez `byte` z CL używa się `cons-cell<int,int>`
(`car`=rozmiar, `cdr`=pozycja). Zarówno rozmiar, jak i pozycja to liczby bitów, więc są typu `int`
bez względu na szerokość rozbieranej liczby całkowitej.

**Liczba całkowita jest pierwszym argumentem, w innej kolejności niż w CL.** CL pisze
`(ldb bytespec integer)`, ale ten język wybiera metodę według typu odbiorcy (pierwszego
argumentu), a ze specyfikatorem na pierwszym miejscu nie mógłby wybierać według typu liczby całkowitej. Wszystkie pozostałe
operacje bitowe mają postać `(op integer ...)` (`(logand a b)`, `(ash x count)`, `(lognot x)`), a
tylko rodzina `ldb` i `logbitp` miały odwrotną kolejność, więc je dostosowano. Pozostałe
argumenty zachowują względną kolejność z CL, więc `(dpb newbyte spec n)` staje się `(dpb n newbyte spec)`.

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `byte` | `(byte size position)` | `(int,int)→cons-cell<int,int>` | Tworzy specyfikator bajtu |
| `byte-size` / `byte-position` | `(byte-size b)` | `cons-cell<int,int>→int` | Wyjmuje składową |
| `ldb` | `(ldb x b)` | `(T,cons-cell<int,int>)→T` | Wyodrębnia wskazany bajt z `x`, wyrównany do prawej |
| `ldb-test` | `(ldb-test x b)` | `(T,cons-cell<int,int>)→bool` | Czy jakikolwiek bit we wskazanym bajcie jest ustawiony |
| `mask-field` | `(mask-field x b)` | `(T,cons-cell<int,int>)→T` | Zeruje wszystko poza wskazanym bajtem (zachowując pozycje) |
| `dpb` | `(dpb x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Wstawia wyrównany do prawej `newbyte` do wskazanego bajtu `x` |
| `deposit-field` | `(deposit-field x newbyte b)` | `(T,T,cons-cell<int,int>)→T` | Wersja `dpb` zachowująca pozycję |
| `boole` | `(boole op a b)` | `(int,T,T)→T` | Jedna z 16 dwuargumentowych operacji logicznych, wybierana przez `op` (stała `boole-*` z rozdziału 10) |

`T` to typ implementujący trait `Bits`, czyli `i8`/`i16`/`i32`/`u8`/`u16`/`u32`/`int`. Tylko
`boole` zachowuje `op` na pierwszym miejscu, ponieważ nie ma powodu, by zmieniać tam kolejność z CL.

## 12. Liczby losowe

| Nazwa | Forma | Typ | Opis |
|---|---|---|---|
| `random` | `(random n [state])` | `int &optional random-state → int` | Liczba losowa od `0` do `n` bez `n`. Jeśli stan pominięto, losuje z `*random-state*` i go przesuwa |
| `make-random-state` | `(make-random-state [state])` | `&optional random-state → random-state` | Bez argumentu nowy stan; z argumentem jego kopia (kopia odtwarza ten sam ciąg) |
| `random-state-p` | `(random-state-p x)` | `random-state→bool` | Zawsze `true` (typ statyczny już wyklucza inne typy; istnieje tylko dla zgodności z CL) |
| `*random-state*` | — | `random-state` | Domyślny stan `random`. Zmienna globalna, do której można przypisywać (zastąp ją za pomocą `setf`) |
| `seed-random-state` | `(seed-random-state n)` | `int→random-state` | Stan nazwany przez liczbę całkowitą. To samo ziarno zawsze odtwarza ten sam ciąg |

Generatorem jest xorshift64 i zwraca ten sam ciąg zarówno w trybie interpretowanym, jak i skompilowanym.

Nowy stan z `make-random-state` jest inicjowany zegarem ściennym, więc nie można go odtworzyć między
uruchomieniami. Aby odtworzyć, użyj `seed-random-state`:

```lisp
(let ((s (seed-random-state 12345)))
  (println "~a ~a ~a" (random 100 s) (random 100 s) (random 100 s)))
;; przy każdym uruchomieniu wypisuje te same trzy liczby
```

**CL nie ma przenośnego sposobu podania ziarna** (`make-random-state` przyjmuje tylko `nil`/`t`/stan), więc ta
nazwa idzie za `sb-ext:seed-random-state` z SBCL, a nie za CL.

Różne ziarna dają różne ciągi. `(seed-random-state 0)` i `(seed-random-state 1)` dają
różne ciągi, podobnie jak `-7` i `7`.
