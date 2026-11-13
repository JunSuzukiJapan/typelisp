<!-- translated-from: docs/ja/reference/functions/collections.md @ cedb434055399f2f83e17dcdaa55007fe4fc9dad -->
# Строки, символы и коллекции

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>`, `BitVector`, `HashSet<T>`, `SortedTable<K,V>` и `Deque<T>`.

## 1. Строки `string`

Строки неизменяемы.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Переводит в верхний регистр (только ASCII). Как `string-upcase` из CL, возвращает новую строку. Строки неизменяемы, поэтому разрушающего `nstring-upcase` нет; эта функция его заменяет |
| `downcase` | `(downcase s)` | `string→string` | Переводит в нижний регистр (только ASCII). Заменяет `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Делает заглавной первую букву каждого слова, а остальные — строчными (`string-capitalize` из CL). Слово — максимальная последовательность букв и цифр |
| `length` | `(length s)` | `string→int` | Число символов |
| `ref` | `(ref s i)` | `(string,int)→char` | Символ `i`. Вне диапазона — panic |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | Подстрока `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Конкатенация. Можно передать три и больше (то же, что `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Лексикографическое сравнение |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Строго меньше в лексикографическом порядке (то же, что `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Сравнение идентичности (тот же ли это объект, а не то же ли содержимое) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Сравнивает содержимое (с учётом регистра) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Сравнивает содержимое (без учёта регистра, только ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Различается ли содержимое (`string/=` из CL. Форма с переменным числом аргументов сравнивает соседние пары) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Упорядочение без учёта регистра (`string-lessp` из CL и т. д.). При общем префиксе меньше более короткая |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | Строка из `n` копий `c` (`make-string` из CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | Позиция первого вхождения `sub`. **У `search` из CL аргументы в обратном порядке** (`(search pattern sequence)`). Пустая строка находится в позиции 0. О ключевых словах см. [ключевые аргументы последовательностей](sequences.md#6-ключевые-аргументы) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | Первая позиция, где они различаются. `none`, только если они `equal`. Если одна — префикс другой, конец более короткой. Ключевые слова — как выше |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Удаляет символы из `bag` с обоих концов / слева / справа (`string-trim` из CL и т. д.). Без `bag` — пробельные `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Разбивает по `sep`. В CL аналога нет. Подряд идущие разделители дают пустые элементы. Panic, если `sep` пуст |
| `to-string` | `(to-string x)` | `T→string` | Преобразует в строку так же, как `~a`. Реализовано для `int`/`i32`/`f64`/`bool`/`char`/`string` (`princ-to-string` из CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Кодирует в UTF-8 (каждый элемент 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Декодирует. `none`, если это не корректный UTF-8 |

## 2. Символы `char`

`char` — скалярное значение Unicode. Преобразование регистра и классификация работают только с диапазоном ASCII.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Переводит в верхний регистр (только ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Переводит в нижний регистр (только ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Сравнение по кодовой точке |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Строго меньше по кодовой точке (то же, что `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Является ли буквой ASCII |
| `digitp` | `(digitp c)` | `char→bool` | Является ли цифрой ASCII |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Сравнивает значения |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Сравнивает значения без учёта регистра (`char-equal` из CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Различаются ли значения (`char/=` из CL. **Форма с переменным числом аргументов сравнивает соседние пары**, в отличие от CL, где проверяется, различаются ли все пары) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Упорядочение без учёта регистра (`char-lessp` из CL и т. д.) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Верхний регистр / нижний регистр / вообще имеет различие регистров (`upper-case-p` из CL и т. д.) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Буква или цифра (то же имя, что в CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Печатаемый ли. Включает пробел, но не перевод строки и не табуляцию (`graphic-char-p` из CL) |
| `standardp` | `(standardp c)` | `char→bool` | Входит ли в 96 стандартных символов CL, то есть `graphicp` плюс перевод строки (`standard-char-p` из CL) |
| `char->int` | `(char->int c)` | `char→int` | Скалярное значение Unicode (обратное — `int->char`/`try-int->char` в [Числах](numbers.md#1-целые-фиксированной-ширины)). Соответствует `char-code`/`char-int` из CL |
| `char->string` | `(char->string c)` | `char→string` | Строка из одного символа. Функция `string` из CL покрывает это, принимая обозначение, но в этом языке обозначений нет, поэтому направление указано в имени |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | **Вес** цифры в этом основании (`digit-char-p` из CL). `digitp` — отдельная функция, возвращающая `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Символ для веса `w`. Для 10 и выше — заглавные (`digit-char` из CL; основание не больше 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Имя символа. Имена есть только у именованных символов, которые умеет читать читатель (`char-name` из CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Символ по имени. Без учёта регистра, принимает и псевдонимы читателя (`linefeed`/`null`) (`name-char` из CL) |

Константы, соответствующей `char-code-limit`, нет (верхний предел `char` задаёт Unicode, а не язык).

## 3. `Vector<T>`

Растущий массив.
Значение можно записать как `#(1 2 3)` (см. [справочник по
синтаксису](../syntax.md#1-лексические-элементы); тип элементов берётся из контекста или первого
элемента, а каждое вычисление создаёт новый вектор). Печатается оно тоже как `#(1 2 3)`.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Создаёт пустой вектор. Аргумент типа берётся из ожидаемого типа, поэтому в голом `let` пишите `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` копий `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Добавляет в конец (метод трейта [`Push`](traits.md#4-трейт-push-добавление-элементов)) |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Читает элемент `i`. Вне диапазона — panic |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Изменяет элемент `i`. Вне диапазона — panic. Можно также записать `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Число элементов |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Удаляет последний элемент и возвращает его. `none`, если пуст (в отличие от `get`/`set`, без panic) (метод трейта [`Pop`](traits.md#5-трейт-pop-извлечение-элементов)) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Создаёт итератор, реализующий `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` | Добавляет `x` в конец, если нет элемента, `eql` ему (`pushnew` из CL. Переписывать место не нужно, поэтому это метод, а не макрос) |

`map`/`filter` и подобные — это [функции последовательностей](sequences.md#4-функции-последовательностей-над-iter):
вектор передаётся через `iter`, как в `(map (iter v) f)`. Разрушающие операции (`nreverse`, `delete` и т. д.) — в
[Разрушающих операциях](sequences.md#7-разрушающие-операции).

## 4. `HashTable<K,V>`

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `new` | `(HashTable::new)` / `(HashTable::new :test 'equal)` | `()→HashTable<K,V>` | Создаёт пустую таблицу. `:test` — способ сравнения ключей (таблица ниже); без него `eql` |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Поиск |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Вставка или перезапись |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Удаляет запись и возвращает старое значение, если оно было |
| `count` | `(count h)` | `HashTable<K,V>→int` | Число записей |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Удаляет всё |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Снимок ключей |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Снимок значений |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Снимок пар `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Итератор, реализующий `Iter`. Элементы — `cons-cell` вида `(k . v)`. Соответствует `with-hash-table-iterator` из CL; `doiter`/`map`/`filter` и другие работают с ним как есть |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | `maphash` из CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | `hash-table-size` из CL. Для этой таблицы — число занятых записей (равно `count`) |

Способ сравнения ключей выбирают при создании с помощью `:test`, как в `make-hash-table` из CL. `:test`
принимает одно из `eq`, `eql`, `equal` и `equalp` (можно писать `'equal` или `equal`).

| `:test` | Как сравниваются ключи | Типы ключей |
|---|---|---|
| `eql` (по умолчанию) | Числа и символы по значению, остальное по идентичности | Любой тип |
| `eq` | По идентичности | Любой тип |
| `equal` | Через `equal` (`Equal`) и хеш `sxhash` (`Hash`) типа ключа | Типы, реализующие `Hash` |
| `equalp` | Через `equalp` (строки без учёта регистра, структуры по полям) | Любой тип |

По умолчанию `eql`, поэтому две отдельно созданные строки с одинаковым содержимым — это два ключа. Для
строковых ключей пишут `(HashTable::new :test 'equal)`. Если тип ключей таблицы `equal` не реализует
`Hash`, это **ошибка типа** в месте создания таблицы (у `f64` нет `Hash` из-за `NaN`).

```lisp
(deftrait Hash (Equal)
  (defmethod sxhash (self) int))              ; возвращает неотрицательное значение, помещающееся в fixnum
```

`Hash` реализован для: `int` и шести целых фиксированной ширины, `bool`, `char`, `string`, `symbol` (не для
чисел с плавающей точкой). Для своих типов результат держат неотрицательным, объединяя его через `logand`
с `*sxhash-mask*` (2^30-1). Чтобы получить хеш строки, можно вызвать `(sxhash-string s)` (FNV-1a 32 бита),
который использует реализация для `string`.

```lisp
(use std::hash::*sxhash-mask*)
(defstruct point (x int) (y int))
(impl Equal point
  (defmethod equal (self (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (defmethod sxhash (self) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new :test 'equal))))
  (progn (set h (make-instance 'point :x 1 :y 2) "a")
         (get h (make-instance 'point :x 1 :y 2))))          ; => (some "a")
```

В таблице `equal` то, одинаковы ли два ключа, решает **сам тип ключа** (`sxhash` и `equal` из `Equal`,
супертрейта `Hash`), а не идентичность объектов. Поэтому, как выше, ключ, который «другое значение, но
равное», находит запись.

Коллизии `sxhash` допустимы (контракт `Hash` действует в обратную сторону: он говорит лишь, что значения,
равные по `equal`, имеют один хеш). Сталкивающиеся ключи различаются через `equal`.

## 5. `Array<T>` (многомерные массивы)

`defstruct` из стандартной библиотеки. Это не встроенный тип, поэтому с ним можно делать всё, что можно делать с
`defstruct`.
Значение можно записать как `#2A((1 2) (3 4))` (см. [справочник по
синтаксису](../syntax.md#1-лексические-элементы)).

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `new` | `(Array::new dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | `make-array` из CL. `dims` копируется. `init` — начальное значение каждой ячейки (`:initial-element` из CL; в этом языке нет «несвязанной ячейки», поэтому он обязателен). `:fill-pointer` только для одной размерности |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | `aref` / `(setf (aref …))` из CL. Panic, если индекс вне диапазона |
| `aref` | `(aref a i j …)` | — | Запись CL с голыми индексами. Раскрывается в `get`/`set` выше. `(setf (aref a i j) v)` тоже работает |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | `row-major-aref` из CL. Плоский индекс |
| `rank` | `(rank a)` | `Array<T>→int` | `array-rank` из CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | `array-dimension` из CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | `array-dimensions` из CL. Возвращает **копию**, как CL возвращает новый список |
| `total-size` | `(total-size a)` | `Array<T>→int` | `array-total-size` из CL (число выделенных ячеек, независимо от указателя заполнения) |
| `len` | `(len a)` | `Array<T>→int` | `length` из CL для массивов. Указатель заполнения, если он есть, иначе `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | `array-in-bounds-p` из CL. Ложь (а не ошибка), даже если неверно **число** индексов |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | `array-row-major-index` из CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | `adjust-array` из CL. Ранг изменить нельзя. Элементы, остающиеся в диапазоне, сохраняют свои индексы, а новые ячейки получают `init`. В отличие от CL, массив не возвращается (любой массив в этом языке изменяем по размеру, так что второго массива для возврата нет) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | `vector-push-extend` из CL. Без указателя заполнения — panic |
| `push` | `(push a x)` | `(Array<T>,T)→Unit` | То же, что `push-extend` (метод трейта [`Push`](traits.md#4-трейт-push-добавление-элементов)) |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | `vector-pop` из CL. `none`, если пуст (метод трейта [`Pop`](traits.md#5-трейт-pop-извлечение-элементов)) |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Указатель заполнения (`none`, если его нет). Можно записать через `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Итератор в порядке по строкам. Останавливается на указателе заполнения, если он есть |

- **Индексы — это `Vector<int>`.** Метод не может объявить «один и тот же тип аргумента, повторённый в конце любое
  число раз», и этот разрыв закрывает сахар `aref`.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p` **не существуют**.
  Статический тип получателя уже отвечает на эти вопросы.
- **Массивы печатаются в синтаксисе массивов CL.** Ранг 1 — `#(1 2 3)`; другие ранги — `#nA`, за которым следует
  столько же уровней скобок (`#2A((1 2 3) (4 5 6))`); ранг 0 — `#0A5`. Печать останавливается на указателе
  заполнения, если он есть. Если установить `*print-array*` ([Печать](printing.md#6-управление-объёмом-вывода)) в
  ложь, печатается только форма `#<array 2x3>`. Лишь массив, элементы которого — `defstruct` без `print-object`,
  печатается во встроенной форме `#<array<...> ...>` (это не ошибка).

## 6. `BitVector` (битовые векторы)

Последовательность битов фиксированной длины. `defstruct` из стандартной библиотеки.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `new` | `(BitVector::new n)` | `int→BitVector` | Длина `n`, все биты 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Вне диапазона — panic |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | Записи из CL. `(setf (bit v i) b)` тоже работает. `sbit` в CL отличается от `bit` лишь требованием простого битового вектора, но в этом языке битовый вектор только одного вида |
| `len` | `(len v)` | `BitVector→int` | Число битов |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Возвращают новый битовый вектор. Panic, если длины различаются. Третьего аргумента, как в CL (место назначения результата), нет |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Дополнение |

`bit-vector-p` нет (на это отвечает статический тип).

## 7. `HashSet<T>`

Набор элементов без повторов (`HashSet` из Rust). `defstruct` стандартной библиотеки, содержимое которого —
`HashTable<T,()>`. Как сравниваются элементы, определяет `:test`, как для ключей `HashTable`, а во
множестве `equal` тип элементов должен реализовывать `Hash`.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `new` | `(HashSet::new)` / `(HashSet::new :test 'equal)` | `()→HashSet<T>` | Создаёт пустое множество. Аргумент типа берётся из ожидаемого типа. `:test` как у `HashTable`; без него `eql` |
| `insert` | `(insert s x)` | `(HashSet<T>,T)→bool` | Добавляет `x`. `true`, если его не было, `false`, если уже был |
| `contains` | `(contains s x)` | `(HashSet<T>,T)→bool` | Есть ли `x` |
| `remove` | `(remove s x)` | `(HashSet<T>,T)→bool` | Удаляет `x`. `true`, если он был |
| `push` | `(push s x)` | `(HashSet<T>,T)→Unit` | То же, что `insert`, но не сообщает, был ли `x` новым (метод трейта [`Push`](traits.md#4-трейт-push-добавление-элементов)) |
| `count` | `(count s)` | `HashSet<T>→int` | Число элементов |
| `clear` | `(clear s)` | `HashSet<T>→Unit` | Удаляет всё |
| `iter` | `(iter s)` | `HashSet<T>→vector-iter<T>` | Итератор по элементам. Порядок не определён |

```lisp
(use std::collections::HashSet)
(let ((seen (the HashSet<string> (HashSet::new :test 'equal))))
  (doiter (w (iter (the Vector<string> #("a" "b" "a"))))
    (if (insert seen w) () (println "dup: ~a" w))))    ; dup: a
```

Общее для трёх типов глав 7–9:

- `iter` обходит копию, снятую в момент вызова. Изменение той же коллекции внутри `doiter` этот цикл
  не видит.
- Если типы элементов реализуют `print-object`, элементы печатаются в виде `#<hashset "a" "b">`
  `#<sortedtable 1 "a">` `#<deque 1 2>`.

## 8. `SortedTable<K,V>`

Таблица в порядке возрастания ключей (`BTreeMap` из Rust). Тип ключа должен реализовывать `Ord`.
Ключи и значения хранятся в двух `Vector` в порядке ключей, поиск двоичный. `set` нового ключа и
`remove` сдвигают элементы после его позиции.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `new` | `(SortedTable::new)` | `()→SortedTable<K,V>` | Создаёт пустую таблицу |
| `get` | `(get t k)` | `(SortedTable<K,V>,K)→Option<V>` | Поиск |
| `set` | `(set t k v)` | `(SortedTable<K,V>,K,V)→Unit` | Вставка или перезапись |
| `remove` | `(remove t k)` | `(SortedTable<K,V>,K)→Option<V>` | Удаляет и возвращает старое значение, если оно было |
| `count` | `(count t)` | `SortedTable<K,V>→int` | Число элементов |
| `clear` | `(clear t)` | `SortedTable<K,V>→Unit` | Удаляет всё |
| `keys` | `(keys t)` | `SortedTable<K,V>→Vector<K>` | Ключи по возрастанию |
| `values` | `(values t)` | `SortedTable<K,V>→Vector<V>` | Значения в порядке ключей |
| `iter` | `(iter t)` | `SortedTable<K,V>→vector-iter<#{K V}>` | Кортежи `#{ключ значение}` в порядке ключей |

```lisp
(use std::collections::SortedTable)
(let ((t (the SortedTable<string,int> (SortedTable::new))))
  (set t "pear" 3) (set t "apple" 5)
  (doiter (#{k v} (iter t)) (println "~a ~a" k v)))    ; apple 5 и pear 3
```

## 9. `Deque<T>`

Последовательность, в которую можно добавлять и из которой можно брать с обоих концов (`VecDeque` из
Rust).

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `new` | `(Deque::new)` | `()→Deque<T>` | Создаёт пустую последовательность |
| `push-front` / `push-back` | `(push-front d x)` | `(Deque<T>,T)→Unit` | Добавляет в начало / конец |
| `push` | `(push d x)` | `(Deque<T>,T)→Unit` | То же, что `push-back` (метод трейта [`Push`](traits.md#4-трейт-push-добавление-элементов)) |
| `pop-front` / `pop-back` | `(pop-front d)` | `Deque<T>→Option<T>` | Извлекает элемент из начала / конца и возвращает его. `none`, если пусто |
| `pop` | `(pop d)` | `Deque<T>→Option<T>` | То же, что `pop-back` (метод трейта [`Pop`](traits.md#5-трейт-pop-извлечение-элементов)) |
| `front` / `back` | `(front d)` | `Deque<T>→Option<T>` | Смотрит на элемент в начале / конце (не извлекая) |
| `get` | `(get d i)` | `(Deque<T>,int)→Option<T>` | `i`-й от начала. `none` вне диапазона |
| `set` | `(set d i x)` | `(Deque<T>,int,T)→Unit` | Перезаписывает `i`-й. Panic вне диапазона |
| `count` | `(count d)` | `Deque<T>→int` | Число элементов |
| `clear` | `(clear d)` | `Deque<T>→Unit` | Удаляет всё |
| `iter` | `(iter d)` | `Deque<T>→vector-iter<T>` | С начала, по порядку |

```lisp
(use std::collections::Deque)
(let ((q (the Deque<int> (Deque::new))))
  (push-back q 1) (push-back q 2) (push-front q 0)
  (println "~s ~s ~s" (pop-front q) (pop-back q) q))    ; (some 0) (some 2) #<deque 1>
```
