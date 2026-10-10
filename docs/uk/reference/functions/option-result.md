<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
# Option, Result і типи помилок

## 1. `Option<T>` / `Result<T,E>`

Конструктори: `Option<T>` має `Some(T)` / `None`. `Result<T,E>` має `Ok(T)` / `Err(E)`. `E` може бути
будь-яким типом: вбудовані конкретні типи помилок і типи, які ви пишете самі через
`defstruct`/`defenum`, підходять туди однаково (розділ 3).

| Назва | Форма | Option | Result | Опис |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Виймає значення. Завершується через panic на `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | Значення або типове значення |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Чи це `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Чи це `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Чи це `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Чи це `Err` |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | Видобуває значення. При `None`/`Err` — panic з `msg` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | Значення або результат `f`. `f` викликається лише при `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | Застосовує `f` до вмісту `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | Застосовує `f` до вмісту `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | При `Some`/`Ok` передає вміст у `f` і повертає її результат |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | При `None`/`Err` повертає результат `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | Перетворює `Some(v)` на `Ok(v)`, а `None` — на `Err(e)` |

Конструктори — це `Option::some`/`Option::none`/`Result::ok`/`Result::err` (або, після
`(use option)`/`(use result)`, голі назви `some`/`none`/`ok`/`err`).

Розгалуження записується явно через `match` або ланцюжком через `map`/`and-then` та інші методи
вище. Синтаксису, що відповідає `?` у Rust, немає.

`map` у `Option`/`Result` — це метод, окремий від `map` для [послідовностей](sequences.md). Він
викликається, коли тип першого аргументу — `Option`/`Result`.

Макрос `->` по черзі передає значення першим аргументом кожній наступній формі (як `->` у Clojure).
`(-> x (f a) (g b))` стає `(g (f x a) b)`. Ім'я без дужок, `h`, вважається `(h x)`. Перший аргумент
методу — його отримувач, тож комбінатори з'єднуються в ланцюжок як є:

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

## 2. Представлення `Option<T>` під час виконання

Як і в Rust, **`Option<T>` зазвичай не створює коробки**. `some v` — це сам `v`, а `none` — це
значення порожнього списку, без виділення пам'яті та без опосередкування. `Option<Sexpr>` (де порожній
список — це `none`), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` і
`Option<(fn ...)>` усі мають цю форму.

Коробка використовується лише тоді, коли значення типу `T` не можна відрізнити від значення порожнього
списку:

| `T` | Представлення | Причина |
|---|---|---|
| `Option<U>` (вкладений) | Коробка | Внутрішній `none` був би тим самим значенням, що й зовнішній `none` |
| `()` | Коробка | Значення `()` — це саме значення порожнього списку |
| `ptr` / `c-long` / `c-ulong` | Коробка | Усі 64 біти — значення, і місця, щоб їх розрізнити, не лишається |
| Будь-що інше | Без коробки | — |

Представлення визначається лише типом і не може бути прочитане зі значення. Під час друку
`(some ...)`/`none` відновлюється зі статичного типу, тож `(format false "~a" opt)` друкує
`(some 1)`. Є два обмеження:

- **Його не можна покласти в `:dyn Trait`** (передавання в `:dyn Speak` значення `Option<int>`, для
  якого ви написали `(impl Speak Option<int> ...)`, — це помилка).
- Приведення `(the Option<T> ...)` з `Sexpr` **називає конструктор**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. Форма, що зв'язує все значення,
  `(the Option<int> o)`, — це помилка.

## 3. Типи помилок і трейт `Error`

За прикладом `std::error::Error` у Rust, **`Error` — це не тип, а трейт**. Конкретні типи, що
представляють помилки, окремі для кожного призначення, і кожен реалізує `Error`.

| Тип | Чим створюється |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Операції з файлами та потоками ([Потоки та файли](streams-files.md)) |
| `NetError` | Мережеві операції ([Мережа](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. `simple-error` з CL: типовий вибір, коли просто треба сказати, що сталося |
| `WrappedError` | `(wrap-error msg cause)`. Тип, що несе і ваше повідомлення, і причину; саме через нього в трейті `Error` є `source` |

Від `ParseIntError` до `NetError` кожен — це «перелік з одним варіантом, що містить один рядок
повідомлення», а назва типу й назва варіанта збігаються (`(match e ((ParseIntError m) m))`, створюється
через `(ParseIntError::ParseIntError "...")`). Нічого особливого в них немає: вони трактуються точно так
само, як ваші власні типи помилок, написані через `(defstruct my-err (...))` / `(defenum my-err ...)`.

| Назва | Форма | Тип | Опис |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Повідомлення про помилку (метод трейта `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | Причина, яку ця помилка обгортає, або `None`, якщо її немає (`Error::source` з Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` реалізує `Error`) | Розширює конкретний тип помилки до трейт-об'єкта |
| `describe-error` | `(describe-error e)` | `E→string` (`E` реалізує `Error`) | Повідомлення та ланцюг причин, знайдений проходом за `source`, по одній причині на рядок. У CL відповідника немає («caused by» з Rust) |

Якщо реалізувати `Error` для власного типу помилки, його можна обробляти **так само**, як вбудовані
помилки:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; the concrete type goes into E as it is
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; handle any kind uniformly
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Щоб зібрати кілька типів помилок в один `Result`, використовуйте `Result<T, :dyn Error>` (відповідає
`Box<dyn Error>` з Rust) і розширюйте конкретні помилки через `as-dyn-error`. Оскільки `?` немає, це
перетворення записується явно:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Типи й трейти ділять один простір імен** (як у Rust). В одному модулі `defstruct`/`defenum` і
трейт не можуть мати однакову назву, а запис назви трейта на місці типу повідомляється як
«`error` is a trait, not a type — write `:dyn error`».

Невідновлювані збої виражаються через `panic`. Про `panic` і `catch`/`throw` див.
[Довідник із синтаксису](../syntax.md#8-нелокальні-виходи-catch--throw--unwind-protect); про політику
обробки помилок див. [розділ 9 того ж довідника](../syntax.md#9-політика-обробки-помилок).
