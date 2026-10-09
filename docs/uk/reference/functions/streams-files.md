<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Потоки та файли

Трейти й методи потоків, конкретні типи потоків, файлові операції та шляхи. Мережеві сокети теж є
потоками, і їх описано в розділі [Мережа](network.md).

## 1. Ієрархія трейтів

Те, що CL виражає ієрархією класів, тут виражено **ієрархією трейтів**. І напрям (ввід / вивід), і тип
елемента визначаються **статично**, тож питати під час виконання «чи можна читати з цього потоку?» не
потрібно.

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; character input
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; character output
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; input that can push back one character
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; byte input
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; byte output
```

Функція, що читає знаки, приймає будь-який тип потоку, вбудований чи користувацький, якщо вона приймає
`(where (CharInput S))` або `:dyn CharInput`.

## 2. Методи

Кожен метод `CharInput` має типову реалізацію. Реалізація пише лише `read-item`.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | Наступний елемент. `none` наприкінці. **Єдиний метод, який треба реалізувати** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Наступний знак |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | До наступного нового рядка (новий рядок споживається й вилучається). Останній рядок, що не закінчується новим рядком, теж повертається |
| `read-all` | `(read-all s)` | `(S)→string` | Усе, що лишилося |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Лише знак, що вже під рукою. `none` замість очікування |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Додає до `v` до `n` знаків і повертає, скільки справді прочитано. Менше за `n` лише наприкінці |

`listen` міститься в `InputStream` (батьківському для `CharInput`):

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Чи можна відповісти на наступне читання без очікування. Типово `false`, **сторона, яка ніколи не бреше**: `true` було б припущенням, а хибне припущення змусило б `read-char-no-hang` блокуватися. Усі вбудовані потоки його перевизначають. **Для користувацьких потоків, що його не перевизначають, `read-char-no-hang` завжди повертає `none`** |

`PeekInput` (що успадковує `CharInput`) додає **повернення одного знака назад**. Лише сам потік має місце,
де зберігати повернутий знак, тож це не може мати типової реалізації й становить окремий трейт.
`file-stream`/`string-input-stream`/`standard-stream` його реалізують, а будь-який інший потік отримує його,
коли обгорнути його через `make-peek-stream` (розділ 4).

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Змушує наступне читання повернути `c`. **Єдиний метод, який треба реалізувати**. Як у CL, гарантовано лише один знак |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Дивиться на наступний знак, не споживаючи його |

Так само для `CharOutput` реалізація пише лише `write-item`.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Записує один елемент. **Єдиний метод, який треба реалізувати** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Записує один знак |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Записує рядок |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Рядок і новий рядок |
| `terpri` | `(terpri s)` | `(S)→()` | Один новий рядок (назва з CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Один новий рядок, якщо не на початку рядка |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Чи наступний записаний знак почне рядок. Типово `false` (тож `fresh-line` записує новий рядок: коли сумніваєшся, запис — безпечніша сторона). Усі вбудовані потоки його перевизначають |
| `finish-output` | `(finish-output s)` | `(S)→()` | Скидає буфер |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Записує всі знаки `v` по порядку |

`at-line-start` пам'ятає **лише те, що було записано через цей потік**. `print`/`println`/
`(format true ...)` пишуть у стандартний вивід, минаючи `*standard-output*`, тож якщо змішувати їх,
`(fresh-line *standard-output*)` не знає про нові рядки, записані `println`. Дотримуйтеся одного з них.

`Stream` спільний для всіх потоків:

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Чи він ще відкритий |
| `close` | `(close s)` | `(S)→()` | Закриває його. **GC потоків не закриває**, тож робіть це явно (або через `with-open-file`) |

## 3. Конкретні типи потоків

| Тип | Як створити | Реалізовані трейти |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` — одна з трьох констант `direction-input` / `direction-output` /
`direction-append`. `open-file` повертає `Err(FileError)`, якщо файл не вдається відкрити (відсутній файл —
це звичайний результат, а не panic). Ім'ям файлу може бути рядок або `pathname` (`Pathish` у розділі 9).

`(get-output-stream-string s)` повертає те, що записано в `string-output-stream`, і спорожнює його. Як у
CL, його можна отримати навіть після `close`.

**Байтовий ввід-вивід** використовує `ByteInput`/`ByteOutput`. Вони фіксують `Item` у
`InputStream`/`OutputStream` як `int`, так само як `CharInput`/`CharOutput` фіксують його як `char`.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | Наступний байт. `none` наприкінці файлу |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Записує один байт. Помилка поза 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | Знакова версія, у байтах |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Так само |

CL визначає тип елемента **у виклику**, як у `(open name :element-type '(unsigned-byte 8))`, але тут тип
елемента — це **тип** потоку, тож відрізняється функція, що його відкриває. Читання байтів зі знакового
потоку — це помилка типів (`string-input-stream` не реалізує `ByteInput`). Читання байта одразу після
повернення знака через `unread-char` теж є помилкою.

## 4. Складені потоки

Усі — це `defstruct` у стандартній бібліотеці, і їх можна вкладати.

| Назва | Форма | Опис |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Пише в усі елементи `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Читає з `in` і пише в `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Читає з `in` і також записує прочитані знаки в `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Читає `Vector<:dyn CharInput>` один за одним |
| `make-peek-stream` | `(make-peek-stream in)` | Додає повернення одного знака до будь-якого `:dyn CharInput`, роблячи його `PeekInput` (для `read-sexpr`) |

## 5. Макроси

| Назва | Форма | Опис |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Відкрити, виконати тіло, закрити. `Result<значення тіла, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Читає з рядка |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Повертає записане |

## 6. Узагальнені функції та файлові операції

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Переносить усе |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Усі рядки, що залишилися |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Читає один `Sexpr` (`read` з CL). `Ok(eof)` наприкінці вводу, `Ok(datum d)`, коли щось прочитано, `Err`, якщо це не дані. Він **споживає один пробільний знак**, що завершив дані (як у CL). `ReadOutcome` — це не `Option<Sexpr>`, щоб читання порожнього списку `()` і кінець вводу не були тим самим значенням |
| `read-sexpr-preserving-whitespace` | Так само | Так само | Те саме, але лишає пробільний знак (`read-preserving-whitespace` з CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Читає до `ch` і створює список. `ch` споживається. `Err`, якщо ввід закінчився |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Записує по одному рядку |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | Увесь вміст |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Усі рядки |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Записує |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Чи існує |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Видалити, перейменувати (аргументи — `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Абсолютний шлях із розв'язаними символічними посиланнями та `.`/`..`. `Err`, якщо не існує |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | Час останньої зміни. Це **універсальний час**, тож його вміє читати `decode-universal-time` ([Час](system.md#2-розкодування-та-кодування-дат)) |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | Ім'я власника для входу. `Err`, якщо файлу не існує, `Ok(none)`, якщо uid власника не має запису в базі паролів: два випадки, які розрізняє CL, лишаються розділеними |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Чи це каталог. **Також `false`, якщо не існує**; щоб їх розрізнити, використовуйте `probe-file` |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Перелічує вміст за truename (абсолютний шлях із розв'язаними символічними посиланнями, як у `truename`). Символічні посилання з відсутньою ціллю пропускаються. `.`/`..` пропускаються. Порядок — той, що дає ОС |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Створює його разом із батьківськими. Успішно, якщо вже існує |

Кожен аргумент, що називає файл, **може бути рядком або `pathname`**. Це те саме трактування, що й у
designator-ів шляхів CL, що розв'язується через трейт `Pathish`, а не через перевірку типу під час
виконання (розділ 9).

Завершальний знак `read-delimited-list` **також завершує токени**. Він діє лише на глибині 0: у `(1 2]`
знак `]` читається як частина власного тексту списку й повідомляється як зламаний список. Відповідника
третього аргументу `recursive-p` з CL немає.

## 7. Як зробити власний тип потоком

Напишіть один `write-item`, а типові реалізації принесуть решту. Його також можна класти у складені
потоки.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; every remaining method is the default

(write-line (counter::new 0) "four")   ; write-line, terpri and fresh-line all work
```

Ввід працює так само: пишете лише `read-item`. Навіть тип без власного повернення знака можна `read`,
щойно обгорнувши його, як-от `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Назва | Виклик | Тип | Опис |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` читає знак `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Повертає зареєстроване |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` читає послідовність із двох знаків `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Так само |

`F` — це `(fn (string-input-stream char) Option<Sexpr>)`. Як їх використовувати, коли вони набувають чинності
та чим відрізняються від CL, описано в [Довіднику із синтаксису](../syntax.md#11-макроси-читача-readtable).

## 9. Шляхи `pathname`

Ім'я файлу, розбите на частини. Воно містить складники каталогу, розділені `/`, ім'я, тип (розширення) і
те, чи починається з кореня.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")   split at the last dot
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Трейт designator-а шляху `Pathish`

Там, де CL приймає designator шляху (рядок або шлях), ця мова приймає `Pathish`. І `string`, і `pathname`
його реалізують, і **кожна файлова операція приймає його узагальнено**, тож `(open-input "a.txt")` і
`(open-input p)` — звичайні виклики (перевірки типу під час виконання немає). `namestring` рядка просто
повертає себе, тож поки ви передаєте рядок, розбір не відбувається.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | Рядкова форма. Треба реалізувати |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Перетворює на `pathname` (функція `pathname` з CL, перейменована, бо конфліктувала б із назвою типу). Треба реалізувати |

### 9.2 Функції

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Розбиває рядок на частини. Завершальний `/` (або порожнє ім'я) означає «імені немає», тобто каталог |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Будує з лише наведених складників (усі `&key`). Пропущені ім'я чи тип лишаються «відсутніми» і є тим, що заповнює `merge-pathnames` |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Складники каталогу, найзовнішній першим |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Ім'я без типу. `none` для каталогу |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | Після останньої крапки. Початкова крапка не рахується (весь `.gitignore` — це ім'я) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Чи починається з кореня |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | Домашній каталог. `none`, якщо немає `$HOME` (CL теж дозволяє `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | Частина до останнього `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Лише частина `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Заповнює складники, яких бракує в `p`, зі `default`. Відносний `p` потрапляє під каталог `default`; абсолютний `p` зберігає власний каталог |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | Форма відносно `default`. Увесь `p`, якщо він не під базою |

Усі аргументи типів несуть `(where (Pathish P))`.

## 10. Відмінності від CL

- **Ієрархія трейтів, а не ієрархія класів.** Немає `input-stream-p` / `output-stream-p`: тип несе
  напрям, тож це не питання, яке слід ставити під час виконання.
- **У `read` різні назви для рядкової та потокової версій.** `(read "...")` (відповідає першому значенню
  `read-from-string` з CL; якщо потрібна також позиція, де читання закінчилося, використовуйте
  `read-from-string`) і `(read-sexpr s)` (`read` з CL). Виклик розв'язується до одного типу одержувача,
  тож ту саму назву не можна перевантажити.
- **Повернення знака — окремий трейт** (`PeekInput`), тож типи, яким потрібен лише `read-char`, не
  змушені реалізовувати `unread-char`.
- **Закриття явне.** GC потоків не закриває (GC запускається в непередбачувані моменти, тож
  залишення цього GC зробило б момент закриття теж непередбачуваним). Безпечний спосіб — використовувати
  `with-open-file`.
- **Шляхи не мають складників хоста, пристрою чи версії.** Немає шляхів із шаблонами й логічних
  шляхів (`logical-pathname`). Роздільник — завжди `/`.
- **Функція `pathname` називається `to-pathname`**, бо типи, трейти та функції ділять один простір імен.
- **Зіставлення за шаблонами немає**, тож `directory` — це функція, що «перелічує вміст цього каталогу», і
  не більше. `directory` у CL зіставляє із зразком шляху.
