<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# Пари, S-вирази та послідовності

Узагальнена пара `cons-cell`, дані у вигляді S-виразів `Sexpr`, символи, функції над послідовностями,
написані поверх `Iter`, і функції вищого порядку.

## 1. Пари `cons-cell<A,B>`

`cons`/`car`/`cdr` — це конструктор і засоби доступу до полів **узагальненого типу пари
`cons-cell<A,B>`** (`defstruct` у стандартній бібліотеці). Поля можна читати як
`variable::car`/`variable::cdr` (синтаксис доступу `defstruct` з
[Довідника із синтаксису](../syntax.md#36-defstruct--структури-користувацькі-типи)) або як
`(car variable)`/`(cdr variable)`. Щоб їх змінити, використовуйте `(setf variable::car v)`/`(setf variable::cdr v)`.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Створює пару |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Перший елемент |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Решта |

`cons-cell` також слугує замість синтаксису кортежів. Функції CL, що повертають множинні значення (частка й
остача `floor`, значення й позиція `read-from-string` тощо), у цій мові повертають `cons-cell`.

## 2. Дані у вигляді S-виразів `Sexpr`

Тип даних `Sexpr`, що повертає `read`, має 19 варіантів:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` і `array` — це дані, записані як `#(..)` і `#nA(..)` (див. [довідник із
синтаксису](../syntax.md#1-лексичні-елементи)); вони містять відповідно `Vector<Option<Sexpr>>` і
`Array<Option<Sexpr>>`, тож `len`, `get` та інше працюють просто з `v`, зв'язаним у `(vector v)`.
`tuple` — дані, записані через `#{..}`, а `v`, яку зв'язує `(tuple v)`, — новий
`Vector<Option<Sexpr>>` з елементів (щоб кортеж будь-якої довжини приймався одним типом).
Комірки S-виразів обробляються не загальними `cons`/`car`/`cdr` з розділу 1, а функціями `sexpr-*`. Їх
використовують насамперед у тілах `defmacro` для побудови та розбору форм.

**Тип даних у вигляді S-виразів — це `Option<Sexpr>`.** Порожній список — це не варіант `Sexpr`, а
`none` з `Option`, а сам `Sexpr` означає «непорожній S-вираз». Тож функції `sexpr-*` приймають і
повертають `Option<Sexpr>`.

- `()` — це порожній список там, де очікується `Option<Sexpr>` (його можна також писати
  `(Option::none)`)
- `Sexpr` неявно розширюється там, де очікується `Option<Sexpr>` (без перетворення під час виконання).
  Зворотний напрямок, використання `Option<Sexpr>` як `Sexpr`, стверджує «це не порожній список», тож це
  слід указати явно через `match` або `unwrap`
- У `match` 19 варіантів `Sexpr` і `none` можна писати **плоско, в одному списку гілок**
  ([Довідник із синтаксису](../syntax.md#43-match--зіставлення-з-візерунком))

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Створює комірку `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Перший елемент. **Порожній список для порожнього списку** (як у CL). Panic на атомі, що не є `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Решта. **Порожній список для порожнього списку** (як у CL). Panic на атомі, що не є `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Чи це `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Чи це порожній список |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Чи це не `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Чи це `Sym` (символ) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | Вміст варіанта `int` (fixnum або bignum). Panic на іншому типі |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | Вміст варіанта цієї ширини. Panic на іншому типі |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Вміст варіантів з рухомою комою. Panic на іншому типі |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Вміст `Char`. Panic на іншому типі |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Вміст `Bool`. Panic на іншому типі |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Вміст `Str`. Panic на іншому типі |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Назва `Sym`. Panic на іншому типі |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Порівняння тотожності (`Cons`/`Str` порівнюють тотожність об'єктів, решта — значення) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Структурна рівність (`Cons` рекурсивно, `Str` за вмістом) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Як `equal`, плюс порівняння без урахування регістру та порівняння чисел різних типів |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | З'єднує два списки `Sexpr` (недеструктивно). `,@` розгортається в це |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Новий список `Sexpr`, у якому `f` застосовано до кожного елемента списку `Sexpr` (`map` з розділу 4 призначений для `Iter` і не може обійти список `Sexpr`) |

Є дев'ять числових засобів доступу, по одному на тип, бо `Sexpr` — це «єдине місце, де тип значення не
записано більше ніде». `u8`, поміщений у `Sexpr`, потрапляє туди як варіант `u8` і виходить лише через
`(sexpr-u8 s)`. Передача його в `(sexpr-int s)` спричиняє panic; відповідь ніколи не розширюється мовчки.
Цілі числа в прочитаних даних (`'(1 2 3)`, аргументи макросів) належать до варіанта `int` і читаються
через `(sexpr-int s)`.

Списки `Sexpr` не мають деструктивних операцій на кшталт `rplaca`/`nconc`. Комірку `Sexpr` не можна
змінити після створення.

## 3. Символи

`symbol` — це тип самих символів. Він неявно перетворюється там, де потрібен `Sexpr`, але не
автоматично в зворотному напрямку.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Виймає назву символу |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Створює символ із рядка (інтернує його) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Чи це ключове слово (`:name`). Двокрапка — частина назви, тож перевірка дивиться на перший знак ([Довідник із синтаксису](../syntax.md#1-лексичні-елементи)) |

Про `gensym` див. [Макроси](system.md#8-макроси).

## 4. Функції над послідовностями для `Iter`

Функції над послідовностями — це **узагальнені функції над трейтом `Iter`**. З колекції отримайте
ітератор через `(iter coll)` і передайте його (це підтримують `Vector<T>` / `HashTable<K,V>` /
`Array<T>`; список `Sexpr` не реалізує `Iter`, тож ці функції до нього не застосовуються). **Колекція
результату повертається як новий `Vector`.** `Iter<A>` у таблицях означає «будь-яка реалізація `Iter`, у
якої `Item` дорівнює `A`». Щоб знову обійти повернутий `Vector`, передайте `(iter result)`.

Функції, що приймають предикат (відповідають родині `-if` з CL):

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Відображення |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Лише елементи, що задовольняють предикат |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Вилучає елементи, що задовольняють предикат |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Перший елемент, що задовольняє предикат |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | Перша позиція, що задовольняє предикат |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Скільки задовольняють предикат |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Чи кожен елемент задовольняє предикат |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Чи будь-який елемент задовольняє предикат (відповідає `some` з CL; назва, що не конфліктує з конструктором `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Лівий згортальний вираз |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Правий згортальний вираз |

Індексація, довжина та зрізи:

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Кількість елементів |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | З'єднує ітератори. Можна передати три й більше |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | `concatenate` з CL. Тип результату записується як **цитований літерал-символ** (CL використовує специфікатор типу часу виконання). `'vector` приймає один і більше, `'string` — нуль і більше (`""` для нуля). Списки `Sexpr` не охоплені (використовуйте `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Розворот (недеструктивний) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Елемент `n` (`None` поза межами) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` з аргументами в зворотному порядку |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | Перші `n` елементів |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` обмежується довжиною) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | Останній **елемент** (а не «остання комірка», як у CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Усе, крім останнього елемента |

Функції, що вимагають обмеження `Eq` / `Ord` (порівнюють через трейт, а не через предикат;
[Стандартні трейти](traits.md#2-eq--ord-порівняння)):

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Чи існує елемент, рівний `x` (на відміну від CL, `bool`, а не решта списку) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | Перший елемент, рівний `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | Перша позиція, рівна `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Скільки елементів рівні `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | `(sort sequence predicate)` з CL. Стабільне недеструктивне сортування. `cmp` дорівнює `true`, коли «перший аргумент суворо передує другому» |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | Перша пара, чий `car` дорівнює `k`. Значення виймається через `(cdr p)` |

Ці та багато функцій з розділу 5 також приймають іменовані аргументи CL `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (розділ 6).

## 5. Решта функцій над послідовностями з CL

Усі — узагальнені функції над `Iter`, як і в розділі 4. Колекції результатів повертаються як нові
`Vector`.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | Іменовані індекси CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Усе, крім першого (новий `Vector`, а не спільний хвіст) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Матеріалізує ітератор у `Vector` (`copy-seq`/`copy-list` з CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` у зворотному порядку, за ним `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` копій `x` (`make-list`/`make-sequence` з CL). Як і для `Vector::new`, аргумент типу береться з очікуваного типу, тож голому `let` потрібен `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Як `member`, **`bool`** (в ітератора немає хвоста, який можна було б повернути) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Заперечення `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Ті самі типи, що й у позитивних версій | Версії із запереченим предикатом |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Вилучає за значенням |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Вилучає дублікати. Як у CL, **зберігається останнє входження** (`:from-end true` зберігає перше) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Замінює за значенням / предикатом |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | на `Iter<cons-cell<K,V>>` | Версії `assoc` за предикатом і за стороною значення |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Додає пару на початок |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Складає пари з двох послідовностей. Зупиняється на коротшій |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | `mapcar` з CL над кількома послідовностями. Зупиняється на коротшій |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Відображення заради побічних ефектів |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Відображає й з'єднує |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Відображає над послідовними **хвостами** |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Відображає над хвостами заради побічних ефектів (відповідник `mapc` для `maplist`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Відображає над хвостами й з'єднує (відповідник `mapcan` для `maplist`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Позиція, де `sub` з'являється вперше. Якщо одержувач — `string`, вибирається метод `string` ([Рядки](collections.md#1-рядки-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Перша позиція, де вони відрізняються. `none`, якщо вони рівні |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Злиття. CL вимагає відсортованих вхідних даних; ця функція сортує конкатенацію |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Додає `x` **на початок**, якщо його немає |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Операції над множинами. CL не визначає порядок; тут він стабільний, **у порядку першої появи** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Включення |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Чи це суфікс / частина перед суфіксом. CL питає про **спільну структуру**, але тут немає структури, яку можна було б ділити, тож це питає про суфікс **як про значення** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Поелементна рівність. Сам `Vector<T>` не реалізує `Eq` |
| `caar`…`cddddr` | `(cadr p)` | на вкладених парах | 28 функцій CL. Вони обходять **пари, а не списки**: `cadr` приймає `cons-cell<A,cons-cell<B,C>>` |

Що є в CL, а в цій мові немає: `list*` (немає поняття неправильного списку із замінним хвостом),
`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (жоден тип не може описати обхід гетерогенного дерева
довільної глибини; для дерева з `Sexpr` `equal` відповідає `tree-equal`), родина списків властивостей
`getf`/`get-properties`/`symbol-plist`/`remprop` (немає представлення у вигляді нетипізованого списку з
чергуванням ключів і значень; ту саму роль виконують `assoc` (асоціативні списки) або `HashTable`) і
функції, що перетворюють між `Vector<T>` та списками `Sexpr` (елементи списку `Sexpr` можуть мати різні
типи, тож їх не можна записати одним типом елемента `T`).

## 6. Іменовані аргументи

Функції з розділів 4 і 5 приймають ключові слова послідовностей CL `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count`. Усі вони **необов'язкові**.

| Ключове слово | Тип | Значення |
|---|---|---|
| `:key` | `(fn (A) A)` | Проєкція, що застосовується до кожного елемента перед порівнянням чи перевіркою |
| `:test` | `(fn (A A) bool)` | Перевірка рівності, що використовується замість `equals` з обмеження `Eq`. Перший аргумент — **шуканий елемент**, другий — елемент (після `:key`), у тому самому порядку, що й у CL |
| `:test-not` | `(fn (A A) bool)` | Заперечення `:test` |
| `:start` `:end` | `int` | Вікно `[start, end)` для сканування. Індекси відраховуються від усієї послідовності |
| `:from-end` | `bool` | Пошук відповідає **останнім** збігом. У поєднанні з `:count` елементи, на які діє операція, беруться з кінця |
| `:count` | `int` | Максимальна кількість елементів, на які діють родини `remove` / `substitute` |

Яка функція яке приймає, слідує CL:

| Функція | Прийняті ключові слова |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Усі наведені вище (включно з `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (`:key` в `assoc` застосовується до `car`, а в `rassoc` — до `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; removes only one, from the end
(position 3 (iter v) :start 1)                          ; the index is relative to the whole sequence
```

**Відмінності від CL**:

1. **Проєкція `:key` лишається в межах типу елемента** (`(fn (A) A)`). Вона не може проєктувати на інший
   тип, як у CL: додаткову змінну типу не вдалося б визначити, коли аргумент пропущено. Де потрібна
   проєкція на інший тип, передайте натомість лямбду в родину `-if`
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **У пошуках за елементом `:key` застосовується лише до елементів** (а не до шуканого елемента). Це те
   саме правило, що й у `find`/`position`/`count`/`member`/`remove`/`substitute` з CL. В операціях над
   множинами обидві сторони — елементи, тож воно застосовується до обох.
3. **Лише ключові слова `search` названо, а не пронумеровано.** У CL `:start1`/`:end1` стосуються
   **шаблону**, а `:start2`/`:end2` — послідовності, у якій шукають. У цій мові одержувач іде першим,
   тож ті самі номери означали б протилежне, до того ж мовчки. `:start`/`:end` стосуються одержувача, а
   `:sub-start`/`:sub-end` — шаблону, тож неуважно написаний `:start1` дає помилку «unknown keyword».
   `mismatch` і `replace` мають той самий порядок аргументів, що й CL, тож зберігають номери CL.

## 7. Деструктивні операції

Методи `Vector<T>`. **Вони змінюють одержувача й повертають самого одержувача**, тож `(nreverse v)`
записується так само, як `reverse`, а сам `v` теж розвертається.

| Назва | Форма | Опис |
|---|---|---|
| `nreverse` | `(nreverse v)` | Розвертає на місці |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Версії `remove` / `remove-if` / `filter` / `remove-duplicates` на місці |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Версії родини `substitute` на місці |
| `nbutlast` | `(nbutlast v)` | Відкидає останній елемент |
| `fill` | `(fill v x)` | Задає кожному елементу значення `x`. Довжина не змінюється |
| `replace` | `(replace v src)` | Перезаписує з початку елементами `src`. `(min (len v) (len src))` елементів |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Та сама кількість, що й вище |
| `nconc` | `(nconc v w)` | Дописує елементи `w` до `v`. На відміну від CL, **не переписує спільну структуру** (`w` не зачіпається) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Замінює вміст `v` на `src` (довжина теж змінюється) |
| `rplaca` `rplacd` | `(rplaca p x)` | Переписує `car`/`cdr` у `cons-cell` і повертає саму комірку |

Прийняті ключові слова:

| Деструктивна версія | Прийняті ключові слова |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (одержувач — це `sequence-1` з CL) |

`vector-push-extend`/`vector-pop` — це просто `push`/`pop` з `Vector<T>`. `Vector<T>` завжди росте, тож
нічого не відповідає розрізненню CL між «вектором із покажчиком заповнення» і «простим вектором».

## 8. Функції вищого порядку

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Повертає свій аргумент |
| `const` | `(const x y)` | `(A,B)→A` | Повертає перший аргумент |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Композиція функцій `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Міняє місцями аргументи функції з двома аргументами |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Заперечення предиката |

`constantly` з CL немає (тип ігнорованого аргументу з'являвся б лише в типі результату й не міг би бути
визначений). Пишіть `(lambda ((x T)) A v)`.
