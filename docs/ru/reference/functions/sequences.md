<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Пары, S-выражения и последовательности

Обобщённая пара `cons-cell`, данные S-выражений `Sexpr`, символы, функции последовательностей, написанные поверх
`Iter`, и функции высшего порядка.

## 1. Пары `cons-cell<A,B>`

`cons`/`car`/`cdr` — конструктор и аксессоры полей **обобщённого типа пары `cons-cell<A,B>`** (`defstruct` из
стандартной библиотеки). Поля можно читать либо как `переменная::car`/`переменная::cdr` (синтаксис доступа к
`defstruct` из [Справочника по синтаксису](../syntax.md#36-defstruct--структуры-пользовательские-типы)), либо как
`(car переменная)`/`(cdr переменная)`. Изменять — через `(setf переменная::car v)`/`(setf переменная::cdr v)`.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Создаёт пару |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Первый элемент |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Остаток |

`cons-cell` служит и заменой синтаксиса кортежей. Функции CL, возвращающие несколько значений (частное и остаток
`floor`, значение и позиция `read-from-string` и т. д.), в этом языке возвращают `cons-cell`.

## 2. S-выражения `Sexpr`

Тип данных `Sexpr`, возвращаемый `read`, имеет 19 вариантов:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` и `array` — это данные, записанные как `#(..)` и `#nA(..)` (см. [справочник по
синтаксису](../syntax.md#1-лексические-элементы)); они содержат соответственно
`Vector<Option<Sexpr>>` и `Array<Option<Sexpr>>`, так что `len`, `get` и прочее работают прямо с
`v`, связанным в `(vector v)`.
`tuple` — данные, записанные через `#{..}`, а `v`, которую связывает `(tuple v)`, — новый
`Vector<Option<Sexpr>>` из элементов (чтобы кортеж любой длины принимался одним типом).
С ячейками S-выражений работают не общие `cons`/`car`/`cdr` из главы 1, а функции `sexpr-*`. Используются они
главным образом в телах `defmacro`, чтобы строить и разбирать формы.

**Тип данных S-выражений — `Option<Sexpr>`.** Пустой список — не вариант `Sexpr`, а `none` из `Option`, а сам `Sexpr`
означает «непустое S-выражение». Поэтому функции `sexpr-*` принимают и возвращают `Option<Sexpr>`.

- `()` — пустой список там, где ожидается `Option<Sexpr>` (можно также написать `(Option::none)`)
- `Sexpr` неявно расширяется там, где ожидается `Option<Sexpr>` (без преобразования во время выполнения). Обратное
  направление — использовать `Option<Sexpr>` как `Sexpr` — утверждает «это не пустой список», поэтому должно быть
  указано явно через `match` или `unwrap`
- В `match` 19 вариантов `Sexpr` и `none` можно писать **плоско, в одном списке ветвей**
  ([Справочник по синтаксису](../syntax.md#43-match--сопоставление-с-образцом))

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Создаёт ячейку `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Первый элемент. **Для пустого списка — пустой список** (как в CL). Panic на атоме, не являющемся `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Остаток. **Для пустого списка — пустой список** (как в CL). Panic на атоме, не являющемся `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Является ли `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Является ли пустым списком |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Не является ли `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Является ли `Sym` (символом) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | Содержимое варианта `int` (fixnum или bignum). Panic на другом типе |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | Содержимое варианта этой ширины. Panic на другом типе |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Содержимое вариантов с плавающей точкой. Panic на другом типе |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Содержимое `Char`. Panic на другом типе |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Содержимое `Bool`. Panic на другом типе |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Содержимое `Str`. Panic на другом типе |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Имя `Sym`. Panic на другом типе |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Сравнение идентичности (`Cons`/`Str` сравнивают идентичность объектов, остальное — значения) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Структурное равенство (`Cons` рекурсивно, `Str` по содержимому) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Как `equal`, плюс сравнение без учёта регистра и сравнение чисел разных типов |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Соединяет два списка `Sexpr` (без разрушения). `,@` раскрывается в это |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Новый список `Sexpr` с `f`, применённой к каждому элементу списка `Sexpr` (`map` из главы 4 — для `Iter` и не может обходить список `Sexpr`) |

Числовых аксессоров девять, по одному на тип, потому что `Sexpr` — «единственное место, где тип значения больше
нигде не записан». `u8`, помещённое в `Sexpr`, входит туда как вариант `u8` и выходит только через `(sexpr-u8 s)`.
Передача его в `(sexpr-int s)` вызывает panic; ответ никогда молча не расширяется. Целые в прочитанных данных
(`'(1 2 3)`, аргументы макросов) — варианта `int` и читаются через `(sexpr-int s)`.

У списков `Sexpr` нет разрушающих операций вроде `rplaca`/`nconc`. Ячейку `Sexpr` нельзя изменить после создания.

## 3. Символы `symbol`

`symbol` — тип самих символов. Он неявно преобразуется там, где требуется `Sexpr`, но в обратном направлении
автоматически — нет.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Извлекает имя символа |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Создаёт символ из строки (интернирует его) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Является ли ключевым словом (`:name`). Двоеточие — часть имени, поэтому проверка смотрит на первый символ ([Справочник по синтаксису](../syntax.md#1-лексические-элементы)) |

О `gensym` см. [Макросы](system.md#8-макросы).

## 4. Функции последовательностей над `Iter`

Функции последовательностей — это **обобщённые функции над трейтом `Iter`**. Из коллекции получите итератор через
`(iter coll)` и передайте его (это поддерживают `Vector<T>` / `HashTable<K,V>` / `Array<T>`; список `Sexpr` не
реализует `Iter`, поэтому эти функции к нему не применяются). **Коллекция-результат возвращается как новый
`Vector`.** `Iter<A>` в таблицах означает «любая реализация `Iter`, у которой `Item` — это `A`». Чтобы снова обойти
возвращённый `Vector`, передайте `(iter result)`.

Функции, принимающие предикат (соответствуют семейству `-if` из CL):

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Отображение |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Только элементы, удовлетворяющие предикату |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Удаляет элементы, удовлетворяющие предикату |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Первый элемент, удовлетворяющий предикату |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | Первая позиция, удовлетворяющая предикату |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Сколько удовлетворяют предикату |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Удовлетворяет ли предикату каждый элемент |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Удовлетворяет ли предикату какой-либо элемент (соответствует `some` из CL; имя, не конфликтующее с конструктором `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Левая свёртка |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Правая свёртка |

Индексация, длина и срезы:

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Число элементов |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Соединяет итераторы. Можно передать три и больше |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | `concatenate` из CL. Тип результата записывается **цитированным литералом-символом** (CL использует спецификатор типа во время выполнения). `'vector` принимает один и больше, `'string` — ноль и больше (`""` для нуля). Списки `Sexpr` не охватываются (используйте `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Обращение (без разрушения) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Элемент `n` (`None` вне диапазона) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` с аргументами в обратном порядке |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | Первые `n` элементов |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` ограничивается длиной) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | Последний **элемент** (а не «последняя ячейка», как в CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Все, кроме последнего элемента |

Функции, требующие ограничения `Eq` / `Ord` (сравнивают через трейт, а не через предикат;
[Стандартные трейты](traits.md#2-eq--ord-сравнение)):

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Есть ли элемент, равный `x` (в отличие от CL — `bool`, а не остаток списка) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | Первый элемент, равный `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | Первая позиция, равная `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Сколько элементов равны `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | `(sort sequence predicate)` из CL. Устойчивая сортировка без разрушения. `cmp` равен `true`, когда «первый аргумент строго предшествует второму» |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | Первая пара, `car` которой равен `k`. Значение извлекается через `(cdr p)` |

Эти функции и многие функции главы 5 также принимают ключевые аргументы CL `:key` / `:test` / `:test-not` / `:start`
/ `:end` / `:from-end` / `:count` (глава 6).

## 5. Остальные функции последовательностей CL

Все — обобщённые функции над `Iter`, как в главе 4. Коллекции-результаты возвращаются как новые `Vector`.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | Именованные индексы CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Все, кроме первого (новый `Vector`, а не разделяемый хвост) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Материализует итератор в `Vector` (`copy-seq`/`copy-list` из CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` в обратном порядке, затем `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` копий `x` (`make-list`/`make-sequence` из CL). Как и у `Vector::new`, аргумент типа берётся из ожидаемого типа, поэтому голому `let` нужен `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Как и `member`, — **`bool`** (у итератора нет хвоста для возврата) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Отрицания `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Те же типы, что у положительных версий | Версии с отрицанием предиката |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Удаляет по значению |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Удаляет дубликаты. Как в CL, **сохраняется последнее вхождение** (`:from-end true` сохраняет первое) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Заменяет по значению / предикату |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | над `Iter<cons-cell<K,V>>` | Версии `assoc` с предикатом и по стороне значения |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Добавляет пару в начало |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Объединяет две последовательности в пары. Останавливается на более короткой |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | `mapcar` из CL над несколькими последовательностями. Останавливается на более короткой |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Отображение ради побочных эффектов |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Отображает и соединяет |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Отображает последовательные **хвосты** |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Отображает хвосты ради побочных эффектов (аналог `mapc` для `maplist`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Отображает хвосты и соединяет (аналог `mapcan` для `maplist`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Позиция первого вхождения `sub`. Если получатель — `string`, выбирается метод `string` ([Строки](collections.md#1-строки-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Первая позиция, где они различаются. `none`, если равны |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Слияние. CL требует отсортированных входов; здесь сортируется соединение |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Добавляет `x` **в начало**, если его нет |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Операции над множествами. CL не задаёт порядок; здесь он устойчив, **в порядке первого появления** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Включение |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Является ли суффиксом / часть до суффикса. CL спрашивает о **разделяемой структуре**, но разделять нечего, поэтому здесь спрашивается о суффиксе **как значениях** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Поэлементное равенство. Сам `Vector<T>` не реализует `Eq` |
| `caar`…`cddddr` | `(cadr p)` | над вложенными парами | 28 функций CL. Обходят **пары, а не списки**: `cadr` принимает `cons-cell<A,cons-cell<B,C>>` |

Что есть в CL и чего нет в этом языке: `list*` (нет понятия несобственного списка с заменяемым хвостом),
`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (никакой тип не может описать обход неоднородного дерева
произвольной глубины; для дерева из `Sexpr` `tree-equal` соответствует `equal`), семейство списков свойств
`getf`/`get-properties`/`symbol-plist`/`remprop` (нет представления в виде нетипизированного списка с чередующимися
ключами и значениями; ту же роль выполняют `assoc` (ассоциативные списки) или `HashTable`), а также функции
преобразования между `Vector<T>` и списками `Sexpr` (элементы списка `Sexpr` могут иметь каждый свой тип, поэтому их
нельзя записать с одним типом элемента `T`).

## 6. Ключевые аргументы

Функции глав 4 и 5 принимают ключевые слова последовательностей CL `:key` / `:test` / `:test-not` / `:start` / `:end`
/ `:from-end` / `:count`. Все они **необязательны**.

| Ключевое слово | Тип | Смысл |
|---|---|---|
| `:key` | `(fn (A) A)` | Проекция, применяемая к каждому элементу перед сравнением или проверкой |
| `:test` | `(fn (A A) bool)` | Проверка равенства, используемая вместо `equals` из ограничения `Eq`. Первый аргумент — **искомый объект**, второй — элемент (после `:key`), в том же порядке, что в CL |
| `:test-not` | `(fn (A A) bool)` | Отрицание `:test` |
| `:start` `:end` | `int` | Окно `[start, end)` для просмотра. Индексы относятся ко всей последовательности |
| `:from-end` | `bool` | Поиск отвечает **последним** совпадением. В сочетании с `:count` затрагиваемые элементы берутся с конца |
| `:count` | `int` | Наибольшее число элементов, которые затрагивают семейства `remove` / `substitute` |

Какая функция что принимает, следует CL:

| Функция | Принимаемые ключевые слова |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Все перечисленные выше (включая `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (`:key` у `assoc` применяется к `car`, у `rassoc` — к `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; удаляет только один, с конца
(position 3 (iter v) :start 1)                          ; индекс относится ко всей последовательности
```

**Отличия от CL**:

1. **Проекция `:key` остаётся в пределах типа элемента** (`(fn (A) A)`). Проецировать в другой тип, как в CL, нельзя:
   дополнительную переменную типа невозможно определить, когда аргумент опущен. Где нужна проекция в другой тип,
   передайте вместо этого лямбду семейству `-if` (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **В поисках по объекту `:key` применяется только к элементам** (а не к искомому объекту). Это то же правило, что у
   `find`/`position`/`count`/`member`/`remove`/`substitute` в CL. В операциях над множествами обе стороны —
   элементы, поэтому он применяется к обеим.
3. **Только ключевые слова `search` названы, а не пронумерованы.** В CL `:start1`/`:end1` относятся к **образцу**, а
   `:start2`/`:end2` — к просматриваемой последовательности. В этом языке получатель идёт первым, поэтому те же номера
   означали бы обратное, причём молча. `:start`/`:end` относятся к получателю, а `:sub-start`/`:sub-end` — к образцу,
   так что рассеянный `:start1` даёт ошибку «неизвестное ключевое слово». У `mismatch` и `replace` тот же порядок
   аргументов, что в CL, поэтому они сохраняют номера CL.

## 7. Разрушающие операции

Методы `Vector<T>`. **Они изменяют получателя и возвращают самого получателя**, поэтому `(nreverse v)` пишется так
же, как `reverse`, и сам `v` тоже обращается.

| Имя | Форма | Описание |
|---|---|---|
| `nreverse` | `(nreverse v)` | Обращает на месте |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Версии `remove` / `remove-if` / `filter` / `remove-duplicates`, работающие на месте |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Версии семейства `substitute`, работающие на месте |
| `nbutlast` | `(nbutlast v)` | Отбрасывает последний элемент |
| `fill` | `(fill v x)` | Устанавливает каждый элемент в `x`. Длина не меняется |
| `replace` | `(replace v src)` | Перезаписывает с начала элементами `src`. `(min (len v) (len src))` элементов |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. То же количество, что выше |
| `nconc` | `(nconc v w)` | Добавляет элементы `w` в `v`. В отличие от CL, **не переписывает разделяемую структуру** (`w` не затрагивается) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Заменяет содержимое `v` на `src` (меняется и длина) |
| `rplaca` `rplacd` | `(rplaca p x)` | Переписывает `car`/`cdr` у `cons-cell` и возвращает саму ячейку |

Принимаемые ключевые слова:

| Разрушающая версия | Принимаемые ключевые слова |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (получатель — `sequence-1` из CL) |

`vector-push-extend`/`vector-pop` — это просто `push`/`pop` у `Vector<T>`. `Vector<T>` всегда растёт, поэтому различию
CL между «вектором с указателем заполнения» и «простым вектором» ничего не соответствует.

## 8. Функции высшего порядка

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Возвращает свой аргумент |
| `const` | `(const x y)` | `(A,B)→A` | Возвращает первый аргумент |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Композиция функций `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Меняет местами аргументы двухаргументной функции |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Отрицание предиката |

`constantly` из CL нет (тип игнорируемого аргумента появлялся бы только в типе результата и не мог бы быть
определён). Пишите `(lambda ((x T)) A v)`.
