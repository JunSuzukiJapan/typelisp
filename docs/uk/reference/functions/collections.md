<!-- translated-from: docs/ja/reference/functions/collections.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Рядки, знаки та колекції

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>` і `BitVector`.

## 1. Рядки `string`

Рядки незмінні.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Перетворює на верхній регістр (лише ASCII). Як `string-upcase` з CL, повертає новий рядок. Рядки незмінні, тож деструктивного `nstring-upcase` немає; ця функція займає його місце |
| `downcase` | `(downcase s)` | `string→string` | Перетворює на нижній регістр (лише ASCII). Займає місце `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Робить першу літеру кожного слова великою, а решту малими (`string-capitalize` з CL). Слово — це максимальна послідовність літер і цифр |
| `length` | `(length s)` | `string→int` | Кількість знаків |
| `ref` | `(ref s i)` | `(string,int)→char` | Знак `i`. Panic поза межами |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | Підрядок `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Конкатенація. Можна передати три й більше (те саме, що `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Лексикографічне порівняння |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Строге лексикографічне «менше» (те саме, що `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Порівняння тотожності (чи це той самий об'єкт, а не той самий вміст) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Порівнює вміст (з урахуванням регістру) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Порівнює вміст (без урахування регістру, лише ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Чи відрізняється вміст (`string/=` з CL. Форма зі змінною кількістю аргументів порівнює сусідні пари) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Упорядкування без урахування регістру (`string-lessp` тощо з CL). За спільного префікса менший — коротший |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Рядок із `n` копій `c` (`make-string` з CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | Позиція, де `sub` з'являється вперше. **У `search` з CL аргументи в зворотному порядку** (`(search pattern sequence)`). Порожній рядок знаходиться на 0. Про ключові слова див. [іменовані аргументи послідовностей](sequences.md#6-іменовані-аргументи) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | Перша позиція, де вони відрізняються. `none` лише коли вони `equal`. Якщо один є префіксом іншого, кінець коротшого. Ключові слова — як вище |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Вилучає знаки, що містяться в `bag`, з обох кінців / зліва / справа (`string-trim` тощо з CL). Без `bag` — пробільні знаки `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Розбиває за `sep`. У CL відповідника немає. Послідовні роздільники дають порожні елементи. Panic, якщо `sep` порожній |
| `to-string` | `(to-string x)` | `T→string` | Перетворює на рядок, як `~a`. Реалізовано для `int`/`i32`/`f64`/`bool`/`char`/`string` (`princ-to-string` з CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Кодує як UTF-8 (кожен елемент 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Декодує. `none`, якщо це не коректний UTF-8 |

## 2. Знаки `char`

`char` — це скалярне значення Unicode. Перетворення регістру та класифікація обробляють лише діапазон
ASCII.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Перетворює на верхній регістр (лише ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Перетворює на нижній регістр (лише ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Порівняння за кодовою позицією |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Строге «менше» за кодовою позицією (те саме, що `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Чи це літера ASCII |
| `digitp` | `(digitp c)` | `char→bool` | Чи це цифра ASCII |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Порівнює значення |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Порівнює значення без урахування регістру (`char-equal` з CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Чи відрізняються значення (`char/=` з CL. **Форма зі змінною кількістю аргументів порівнює сусідні пари**, на відміну від CL, що питає, чи всі пари відрізняються) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Упорядкування без урахування регістру (`char-lessp` тощо з CL) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Верхній регістр / нижній регістр / має взагалі розрізнення регістрів (`upper-case-p` тощо з CL) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Літера або цифра (та сама назва, що й у CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Чи друкований. Включає пробіл, але не новий рядок і не табуляцію (`graphic-char-p` з CL) |
| `standardp` | `(standardp c)` | `char→bool` | Чи це один із 96 стандартних знаків CL, тобто `graphicp` плюс новий рядок (`standard-char-p` з CL) |
| `char->int` | `(char->int c)` | `char→int` | Скалярне значення Unicode (зворотне — `int->char`/`try-int->char` у розділі [Числа](numbers.md#1-цілі-числа-фіксованої-ширини)). Відповідає `char-code`/`char-int` з CL |
| `char->string` | `(char->string c)` | `char→string` | Рядок з одного знака. Функція `string` з CL покриває це, приймаючи designator, але в цій мові designator-ів немає, тож напрям указано в назві |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | **Вага** цифри в цій системі числення (`digit-char-p` з CL). `digitp` — окрема функція, що повертає `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Знак для ваги `w`. Верхній регістр для 10 і більше (`digit-char` з CL; основа не більша за 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Назва знака. Назви мають лише іменовані знаки, які читач уміє прочитати (`char-name` з CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Знак за назвою. Нечутливий до регістру, також приймає псевдоніми читача (`linefeed`/`null`) (`name-char` з CL) |

Константи, що відповідала б `char-code-limit`, немає (верхню межу `char` задає Unicode, а не мова).

## 3. `Vector<T>`

Масив, що росте.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Створює порожній вектор. Аргумент типу береться з очікуваного типу, тож у голому `let` пишіть `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` копій `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Додає в кінець |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Читає елемент `i`. Panic поза межами |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Змінює елемент `i`. Panic поза межами. Можна також писати `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Кількість елементів |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Вилучає останній елемент і повертає його. `None`, якщо порожньо (на відміну від `get`/`set`, panic не виникає) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Створює ітератор, що реалізує `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Додає `x`, якщо рівного елемента немає (`pushnew` з CL. Йому не треба переписувати place, тож це метод, а не макрос) |

`map`/`filter` та подібні — це [функції над послідовностями](sequences.md#4-функції-над-послідовностями-для-iter):
передавайте вектор через `iter`, як-от `(map (iter v) f)`. Деструктивні операції (`nreverse`, `delete`
тощо) описано в розділі [Деструктивні операції](sequences.md#7-деструктивні-операції).

## 4. `HashTable<K,V>`

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Створює порожню таблицю |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Пошук |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Вставка або перезапис |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Вилучає запис і повертає старе значення, якщо воно було |
| `count` | `(count h)` | `HashTable<K,V>→int` | Кількість записів |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Вилучає все |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Знімок ключів |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Знімок значень |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Знімок пар `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Ітератор, що реалізує `Iter`. Елементи — це `cons-cell` `(k . v)`. Відповідає `with-hash-table-iterator` з CL; `doiter`/`map`/`filter` та інші працюють із ним як є |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | `maphash` з CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | `hash-table-size` з CL. У цій таблиці це кількість зайнятих записів (дорівнює `count`) |

**Ключем може бути будь-який тип, що реалізує `Hash`**, зокрема типи `defstruct`/`defenum`. `get`/`set`/
`remove` несуть `(where (Hash K))`, тож таблиця з ключем типу, що його не реалізує, — це **помилка
типів** (у `f64` немає `Hash` через `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; returns a non-negative value that fits in a fixnum
```

Реалізовано для: `int` і шести цілих типів фіксованої ширини, `bool`, `char`, `string` і `symbol` (не для
чисел з рухомою комою). Для власних типів тримайте результат невід'ємним, виконуючи `logand` із
`*sxhash-mask*` (2^30-1). Щоб захешувати рядок, можна викликати `(sxhash-string s)` (32-бітний FNV-1a),
який використовує реалізація для `string`.

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

Чи є два ключі однаковими, вирішує **сам тип ключа** (`sxhash` і `equals` з `Eq`, супертрейта `Hash`), а
не тотожність об'єктів. Саме тому, як вище, можна шукати за ключем, що є «іншим значенням, але рівним».

Колізії `sxhash` припустимі (контракт `Hash` односторонній: рівні значення мають мати той самий хеш).
Ключі, що колізують, розрізняються через `equals`.

## 5. `Array<T>` (багатовимірні масиви)

`defstruct` у стандартній бібліотеці. Це не вбудований тип, тож усе, що можна робити з `defstruct`, можна
робити й з ним.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | `make-array` з CL. `dims` копіюється. `init` — початкове значення кожної комірки (`:initial-element` з CL; у цій мові немає «незв'язаної комірки», тож воно обов'язкове). `:fill-pointer` лише для одного виміру |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | `aref` / `(setf (aref …))` з CL. Panic, якщо індекс поза межами |
| `aref` | `(aref a i j …)` | — | Запис CL з голими індексами. Розгортається в наведені вище `get`/`set`. `(setf (aref a i j) v)` теж працює |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | `row-major-aref` з CL. Плоский індекс |
| `rank` | `(rank a)` | `Array<T>→int` | `array-rank` з CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | `array-dimension` з CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | `array-dimensions` з CL. Повертає **копію**, так само як CL повертає свіжий список |
| `total-size` | `(total-size a)` | `Array<T>→int` | `array-total-size` з CL (кількість виділених комірок, не пов'язана з покажчиком заповнення) |
| `len` | `(len a)` | `Array<T>→int` | `length` з CL для масивів. Покажчик заповнення, якщо він є, інакше `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | `array-in-bounds-p` з CL. Хибність (не помилка), навіть коли неправильна **кількість** індексів |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | `array-row-major-index` з CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | `adjust-array` з CL. Ранг змінити не можна. Елементи, що лишаються в межах, зберігаються за своїми індексами, а нові комірки отримують `init`. На відміну від CL, масив не повертає (кожен масив у цій мові регульований, тож другого масиву для повернення немає) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | `vector-push-extend` з CL. Panic без покажчика заповнення |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | `vector-pop` з CL. `none`, якщо порожньо |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Покажчик заповнення (`none`, якщо його немає). Можна записувати через `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Ітератор у порядку рядків (row-major). Зупиняється на покажчику заповнення, якщо він є |

- **Індекси — це `Vector<int>`.** Метод не може оголосити «аргументи того самого типу, що повторюються
  будь-яку кількість разів наприкінці», і цю прогалину закриває цукор `aref`.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **не існують**. На ці питання вже відповідає статичний тип одержувача.
- `Array::new` — це конструктор у порядку полів, згенерований `defstruct`, і він не призначений для
  створення масивів. Використовуйте `Array::make`.
- **Масиви друкуються в синтаксисі масивів CL.** Ранг 1 — це `#(1 2 3)`; інші ранги — `#nA`, за яким
  стільки рівнів дужок (`#2A((1 2 3) (4 5 6))`); ранг 0 — `#0A5`. Друк зупиняється на покажчику
  заповнення, якщо він є. Якщо задати `*print-array*` ([Друк](printing.md#6-керування-обсягом-друку))
  хибним, друкується лише форма, `#<array 2x3>`. Лише масив, елементи якого — `defstruct` без
  `print-object`, друкується у вбудованій формі `#<array<...> ...>` (це не помилка).

## 6. `BitVector` (бітові вектори)

Послідовність бітів фіксованої довжини. `defstruct` у стандартній бібліотеці.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Довжина `n`, усі біти 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Panic поза межами |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | Записи CL. `(setf (bit v i) b)` теж працює. `sbit` з CL відрізняється від `bit` лише вимогою простого бітового вектора, але в цій мові є лише один вид бітового вектора |
| `len` | `(len v)` | `BitVector→int` | Кількість бітів |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Повертають новий бітовий вектор. Panic, якщо довжини різні. Третього аргументу, як у CL (призначення результату), немає |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Доповнення |

`bit-vector-p` немає (на це відповідає статичний тип).
