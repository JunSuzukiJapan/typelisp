<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
# Option, Result и типы ошибок

## 1. `Option<T>` / `Result<T,E>`

Конструкторы: у `Option<T>` — `Some(T)` / `None`. У `Result<T,E>` — `Ok(T)` / `Err(E)`. `E` может быть любым типом:
туда одинаково подходят встроенные конкретные типы ошибок и типы, которые вы пишете сами через `defstruct`/`defenum`
(глава 3).

| Имя | Форма | Option | Result | Описание |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Извлекает значение. Panic на `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | Значение или значение по умолчанию |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Является ли `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Является ли `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Является ли `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Является ли `Err` |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | Извлекает значение. При `None`/`Err` — panic с `msg` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | Значение или результат `f`. `f` вызывается только при `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | Применяет `f` к содержимому `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | Применяет `f` к содержимому `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | При `Some`/`Ok` передаёт содержимое в `f` и возвращает её результат |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | При `None`/`Err` возвращает результат `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | Превращает `Some(v)` в `Ok(v)`, а `None` — в `Err(e)` |

Конструкторы — `Option::some`/`Option::none`/`Result::ok`/`Result::err` (или, после `(use option)`/`(use result)`,
голые имена `some`/`none`/`ok`/`err`).

Ветвление записывается явно через `match` или в виде цепочки через `map`/`and-then` и остальные
методы выше. Синтаксиса, соответствующего `?` в Rust, нет.

`map` у `Option`/`Result` — это метод, отдельный от `map` для [последовательностей](sequences.md).
Вызывается он, когда тип первого аргумента — `Option`/`Result`.

Макрос `->` по очереди передаёт значение первым аргументом каждой следующей форме (как `->` в
Clojure). `(-> x (f a) (g b))` превращается в `(g (f x a) b)`. Имя без скобок, `h`, считается
`(h x)`. Первый аргумент метода — его получатель, поэтому комбинаторы соединяются в цепочку как
есть:

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. Представление `Option<T>` во время выполнения

Как и в Rust, **`Option<T>` обычно не создаёт обёртки**. `some v` — это само `v`, а `none` — значение пустого списка,
без выделения памяти и без косвенности. `Option<Sexpr>` (где пустой список — `none`), `Option<int>`,
`Option<string>`, `Option<my-struct>`, `Option<f64>` и `Option<(fn ...)>` — все имеют такую форму.

Обёртка используется только тогда, когда значение `T` нельзя отличить от значения пустого списка:

| `T` | Представление | Причина |
|---|---|---|
| `Option<U>` (вложенный) | Обёртка | Внутренний `none` был бы тем же значением, что и внешний `none` |
| `()` | Обёртка | Значение `()` — это само значение пустого списка |
| `ptr` / `c-long` / `c-ulong` | Обёртка | Все 64 бита — значение, места для различения не остаётся |
| Всё остальное | Без обёртки | — |

Представление определяется только типом и не читается из значения. При печати `(some ...)`/`none` восстанавливается
по статическому типу, поэтому `(format false "~a" opt)` выводит `(some 1)`. Есть два ограничения:

- **Его нельзя поместить в `:dyn Trait`** (передача значения `Option<int>`, для которого написано
  `(impl Speak Option<int> ...)`, в `:dyn Speak` — ошибка).
- Нисходящее приведение `(the Option<T> ...)` из `Sexpr` **называет конструктор**: `(the Option<int> (some x))` /
  `(the Option<int> (none))`. Форма, связывающая значение целиком, `(the Option<int> o)`, — ошибка.

## 3. Типы ошибок и трейт `Error`

По образцу `std::error::Error` из Rust **`Error` — не тип, а трейт**. Конкретные типы, представляющие ошибки, свои
для каждого назначения, и каждый реализует `Error`.

| Тип | Кем создаётся |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Операции с файлами и потоками ([Потоки и файлы](streams-files.md)) |
| `NetError` | Сетевые операции ([Сеть](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. `simple-error` из CL: выбор по умолчанию, когда нужно просто сказать, что произошло |
| `WrappedError` | `(wrap-error msg cause)`. Тип, несущий и ваше сообщение, и причину; из-за него у трейта `Error` есть `source` |

`ParseIntError`–`NetError` — каждый «перечисление с единственным вариантом, содержащим одну строку сообщения», и имя
типа совпадает с именем варианта (`(match e ((ParseIntError m) m))`, создаётся через
`(ParseIntError::ParseIntError "...")`). В них нет ничего особенного: они обрабатываются точно так же, как ваши
собственные типы ошибок, написанные через `(defstruct my-err (...))` / `(defenum my-err ...)`.

| Имя | Форма | Тип | Описание |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Сообщение об ошибке (метод трейта `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | Причина, которую оборачивает эта ошибка, или `None`, если её нет (`Error::source` из Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` реализует `Error`) | Расширяет конкретный тип ошибки до типажного объекта |
| `describe-error` | `(describe-error e)` | `E→string` (`E` реализует `Error`) | Сообщение и цепочка причин, найденная по `source`, по одной причине на строку. В CL аналога нет («caused by» в Rust) |

Если реализовать `Error` для собственного типа ошибки, с ним можно работать **так же**, как со встроенными ошибками:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; конкретный тип идёт в E как есть
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; обрабатывать все виды единообразно
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Чтобы собрать несколько типов ошибок в одном `Result`, используйте `Result<T, :dyn Error>` (соответствует
`Box<dyn Error>` в Rust) и расширяйте конкретные ошибки через `as-dyn-error`. Поскольку `?` нет, это преобразование
пишется явно:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Типы и трейты разделяют одно пространство имён** (как в Rust). В одном модуле `defstruct`/`defenum` и трейт не могут
иметь одинаковое имя, а имя трейта в позиции типа сообщается как «`error` is a trait, not a type — write
`:dyn error`».

Невосстановимые неудачи выражаются через `panic`. О `panic` и `catch`/`throw` см.
[Справочник по синтаксису](../syntax.md#8-нелокальные-выходы-catch--throw--unwind-protect); о принципах обработки
ошибок — [его главу 9](../syntax.md#9-принципы-обработки-ошибок).
