<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Потоки и файлы

Трейты и методы потоков, конкретные типы потоков, операции с файлами и пути. Сетевые сокеты тоже являются потоками и
описаны в [Сети](network.md).

## 1. Иерархия трейтов

То, что CL выражает иерархией классов, здесь выражается **иерархией трейтов**. И направление (ввод / вывод), и тип
элементов определяются **статически**, поэтому нет нужды спрашивать во время выполнения «можно ли читать из этого
потока?».

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; символьный ввод
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; символьный вывод
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; ввод, способный вернуть один символ
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; байтовый ввод
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; байтовый вывод
```

Функция, читающая символы, принимает любой тип потока, встроенный или пользовательский, если она принимает
`(where (CharInput S))` или `:dyn CharInput`.

## 2. Методы

У каждого метода `CharInput` есть реализация по умолчанию. Реализация пишет только `read-item`.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | Следующий элемент. `none` в конце. **Единственный метод, который нужно реализовать** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | Следующий символ |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | До следующего перевода строки (перевод строки потребляется и удаляется). Последняя строка без перевода строки тоже возвращается |
| `read-all` | `(read-all s)` | `(S)→string` | Всё, что осталось |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Только символ, который уже доступен. Лучше `none`, чем ждать |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Добавляет в `v` до `n` символов и возвращает, сколько реально прочитано. Меньше `n` — только в конце |

`listen` находится в `InputStream` (родителе `CharInput`):

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Можно ли ответить на следующее чтение без ожидания. По умолчанию `false`, **сторона, которая никогда не лжёт**: `true` было бы догадкой, а неверная догадка заставила бы `read-char-no-hang` заблокироваться. Все встроенные потоки его переопределяют. **Для пользовательских потоков, которые его не переопределяют, `read-char-no-hang` всегда возвращает `none`** |

`PeekInput` (наследуется от `CharInput`) добавляет **возврат одного символа**. Место, где хранить возвращённый символ,
есть только у самого потока, поэтому реализации по умолчанию быть не может, и это отдельный трейт.
`file-stream`/`string-input-stream`/`standard-stream` его реализуют, а любой другой поток получает его, будучи
обёрнутым через `make-peek-stream` (глава 4).

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Заставляет следующее чтение вернуть `c`. **Единственный метод, который нужно реализовать**. Как в CL, гарантируется только один символ |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Смотрит на следующий символ, не потребляя его |

Аналогично, для `CharOutput` реализация пишет только `write-item`.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Пишет один элемент. **Единственный метод, который нужно реализовать** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Пишет один символ |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Пишет строку |
| `write-line` | `(write-line s str)` | `(S,string)→()` | Строка и перевод строки |
| `terpri` | `(terpri s)` | `(S)→()` | Один перевод строки (имя из CL) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | Один перевод строки, если не в начале строки |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Начнёт ли следующий записанный символ строку. По умолчанию `false` (поэтому `fresh-line` пишет перевод строки: в сомнении писать безопаснее). Все встроенные потоки его переопределяют |
| `finish-output` | `(finish-output s)` | `(S)→()` | Сбрасывает буфер |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Пишет по порядку все символы `v` |

`at-line-start` помнит **только то, что было записано через этот поток**. `print`/`println`/`(format true ...)` пишут в
стандартный вывод, не проходя через `*standard-output*`, поэтому если смешивать их, `(fresh-line *standard-output*)`
не знает о переводах строк, записанных `println`. Придерживайтесь одного из них.

`Stream` общий для всех потоков:

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Открыт ли ещё |
| `close` | `(close s)` | `(S)→()` | Закрывает. **Сборщик мусора не закрывает потоки**, поэтому делайте это явно (или через `with-open-file`) |

## 3. Конкретные типы потоков

| Тип | Как создать | Реализуемые трейты |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` — одна из трёх констант `direction-input` / `direction-output` / `direction-append`. `open-file`
возвращает `Err(FileError)`, если файл не удаётся открыть (отсутствующий файл — обычный результат, а не panic). Имя
файла может быть строкой или `pathname` (`Pathish` в главе 9).

`(get-output-stream-string s)` возвращает то, что было записано в `string-output-stream`, и очищает его. Как в CL,
забрать содержимое можно и после `close`.

**Байтовый ввод-вывод** использует `ByteInput`/`ByteOutput`. Они фиксируют `Item` у `InputStream`/`OutputStream` как
`int`, так же как `CharInput`/`CharOutput` фиксируют его как `char`.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | Следующий байт. `none` в конце файла |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Пишет один байт. Вне 0..255 — ошибка |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | Символьная версия, в байтах |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | То же |

CL определяет тип элементов **в вызове**, как `(open name :element-type '(unsigned-byte 8))`, а здесь тип элементов —
это **тип** потока, поэтому различается функция, которая его открывает. Чтение байтов из символьного потока — ошибка
типа (`string-input-stream` не реализует `ByteInput`). Чтение байта сразу после возврата символа через `unread-char` —
тоже ошибка.

## 4. Составные потоки

Все — `defstruct` из стандартной библиотеки, их можно вкладывать.

| Имя | Форма | Описание |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Пишет во все потоки `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Читает из `in` и пишет в `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Читает из `in` и также пишет прочитанные символы в `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Читает по очереди потоки `Vector<:dyn CharInput>` |
| `make-peek-stream` | `(make-peek-stream in)` | Добавляет к любому `:dyn CharInput` возврат одного символа, делая его `PeekInput` (для `read-sexpr`) |

## 5. Макросы

| Имя | Форма | Описание |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Открыть, выполнить тело, закрыть. `Result<значение тела, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Читает из строки |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Возвращает то, что было записано |

## 6. Обобщённые функции и операции с файлами

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Переносит всё |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | Все оставшиеся строки |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Читает один `Sexpr` (`read` из CL). `Ok(eof)` в конце ввода, `Ok(datum d)`, когда данное прочитано, `Err`, если это не данные. Он **потребляет один пробельный символ**, завершивший данное (как в CL). `ReadOutcome` — не `Option<Sexpr>`, чтобы чтение пустого списка `()` и конец ввода не были одним и тем же значением |
| `read-sexpr-preserving-whitespace` | То же | То же | То же, но оставляет пробельный символ (`read-preserving-whitespace` из CL) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Читает до `ch` и составляет список. `ch` потребляется. `Err`, если ввод заканчивается |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Пишет по одной строке |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | Всё содержимое |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Все строки |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Записывает |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Существует ли |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Удалить, переименовать (аргументы — `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | Абсолютный путь с разрешёнными символическими ссылками и `.`/`..`. `Err`, если не существует |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | Время последнего изменения. Это **универсальное время**, поэтому его может прочитать `decode-universal-time` ([Время](system.md#2-разбор-и-сборка-дат)) |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | Имя входа владельца. `Err`, если файла нет, `Ok(none)`, если для uid владельца нет записи в базе паролей: два случая, которые различает CL, остаются раздельными |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Является ли каталогом. **Тоже `false`, если не существует**; чтобы различить эти случаи, используйте `probe-file` |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Перечисляет содержимое по truename (абсолютный путь с разрешёнными символическими ссылками, как у `truename`). Символические ссылки с отсутствующей целью пропускаются. `.`/`..` пропускаются. Порядок — тот, что даёт ОС |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Создаёт вместе с родительскими. Успешно, если уже существует |

Любой аргумент, обозначающий файл, **может быть строкой или `pathname`**. Это то же обращение, что с обозначениями
путей в CL, разрешаемое через трейт `Pathish`, а не через проверку типа во время выполнения (глава 9).

Завершающий символ `read-delimited-list` **завершает и лексемы**. Он действует только на глубине 0: в `(1 2]` `]`
читается как часть собственного текста списка и сообщается как испорченный список. Аналога третьего аргумента CL
`recursive-p` нет.

## 7. Как сделать свой тип потоком

Напишите один `write-item`, и реализации по умолчанию принесут остальное. Его можно помещать и в составные потоки.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; все остальные методы — по умолчанию

(write-line (counter::new 0) "four")   ; write-line, terpri и fresh-line работают
```

Ввод работает так же: пишется только `read-item`. Даже тип без собственного возврата символа можно читать, обернув
его, как в `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Имя | Вызов | Тип | Описание |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` читает символ `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Возвращает зарегистрированное |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` читает двухсимвольную последовательность `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | То же |

`F` — это `(fn (string-input-stream char) Option<Sexpr>)`. Как ими пользоваться, когда они вступают в силу и чем
отличаются от CL, описано в [Справочнике по синтаксису](../syntax.md#11-макросы-чтения-readtable).

## 9. Пути `pathname`

Имя файла, разбитое на части. Содержит компоненты каталогов, разделённые `/`, имя, тип (расширение) и признак того,
начинается ли путь от корня.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #<vector<string> "var" "log">
  (pathname-name p)        ; => (some "app.tar")   разделено по последней точке
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 Трейт обозначения пути `Pathish`

Там, где CL принимает обозначение пути (строку или pathname), этот язык принимает `Pathish`. Его реализуют и `string`,
и `pathname`, и **каждая операция с файлами принимает его обобщённо**, поэтому и `(open-input "a.txt")`, и
`(open-input p)` — обычные вызовы (проверки типа во время выполнения нет). `namestring` строки просто возвращает её
саму, так что пока вы передаёте строку, никакого разбора не происходит.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | Строковая форма. Обязательна к реализации |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Преобразует в `pathname` (функция `pathname` из CL, переименованная, потому что конфликтовала бы с именем типа). Обязательна к реализации |

### 9.2 Функции

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Разбивает строку на части. Завершающий `/` (или пустое имя) означает «нет имени», то есть каталог |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Строит путь только из указанных компонентов (все `&key`). Опущенные имя или тип остаются «отсутствующими», и их дополнит `merge-pathnames` |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | Компоненты каталогов, начиная с самого внешнего |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | Имя без типа. `none` для каталога |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | После последней точки. Начальная точка не считается (весь `.gitignore` — имя) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Начинается ли от корня |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | Домашний каталог. `none`, если нет `$HOME` (CL допускает и `NIL`) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | Часть до последнего `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Только часть `name.type` |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Дополняет недостающие у `p` компоненты из `default`. Относительный `p` помещается в каталог `default`; абсолютный `p` сохраняет свой каталог |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | Форма относительно `default`. Весь `p`, если он не под базой |

Все аргументы типа несут `(where (Pathish P))`.

## 10. Отличия от CL

- **Иерархия трейтов, а не классов.** Нет `input-stream-p` / `output-stream-p`: направление несёт тип, так что это не
  вопрос для времени выполнения.
- **У `read` разные имена для версии со строкой и версии с потоком.** `(read "...")` (соответствует первому значению
  `read-from-string` из CL; если нужна ещё и позиция, где закончилось чтение, используйте `read-from-string`) и
  `(read-sexpr s)` (`read` из CL). Вызов разрешается к одному типу получателя, поэтому одно имя перегрузить нельзя.
- **Возврат символа — отдельный трейт** (`PeekInput`), так что типам, которым нужен только `read-char`, не приходится
  реализовывать `unread-char`.
- **Закрытие явное.** Сборщик мусора не закрывает потоки (он работает в непредсказуемые моменты, и если поручить ему
  закрытие, момент закрытия тоже стал бы непредсказуемым). Безопасный путь — `with-open-file`.
- **У путей нет компонентов хоста, устройства и версии.** Нет путей с шаблонами и логических путей
  (`logical-pathname`). Разделитель всегда `/`.
- **Функция `pathname` называется `to-pathname`**, потому что типы, трейты и функции разделяют одно пространство
  имён.
- **Сопоставления с шаблонами нет**, поэтому `directory` — функция, которая «перечисляет содержимое этого каталога», и
  ничего более. `directory` из CL сопоставляет с образцом пути.
