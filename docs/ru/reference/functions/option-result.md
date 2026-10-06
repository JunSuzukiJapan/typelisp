<!-- translated-from: docs/ja/reference/functions/option-result.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
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

Конструкторы — `Option::some`/`Option::none`/`Result::ok`/`Result::err` (или, после `(use option)`/`(use result)`,
голые имена `some`/`none`/`ok`/`err`).

Ветвления пишутся явно через `match`. Синтаксиса, соответствующего `?` из Rust, нет.

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
