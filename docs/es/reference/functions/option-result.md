<!-- translated-from: docs/ja/reference/functions/option-result.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Option, Result y tipos de error

## 1. `Option<T>` / `Result<T,E>`

Constructores: `Option<T>` tiene `Some(T)` / `None`. `Result<T,E>` tiene `Ok(T)` / `Err(E)`. `E` puede ser
cualquier tipo: los tipos de error concretos incorporados y los tipos que escribas tú con
`defstruct`/`defenum` encajan ahí igual (capítulo 3).

| Nombre | Forma | Option | Result | Descripción |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Saca el valor. Panic con `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | El valor, o el valor por defecto |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Si es `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Si es `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Si es `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Si es `Err` |

Los constructores son `Option::some`/`Option::none`/`Result::ok`/`Result::err` (o, tras
`(use option)`/`(use result)`, los nombres simples `some`/`none`/`ok`/`err`).

Las ramificaciones se escriben explícitamente con `match`. No hay sintaxis correspondiente al `?` de Rust.

## 2. La representación en tiempo de ejecución de `Option<T>`

Como en Rust, **`Option<T>` normalmente no crea una caja**. `some v` es el propio `v` y `none` es el valor de
la lista vacía, sin reserva de memoria ni indirección. `Option<Sexpr>` (donde la lista vacía es `none`),
`Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` y `Option<(fn ...)>` toman todos esta
forma.

Solo se usa una caja cuando un valor de `T` no se puede distinguir del valor de la lista vacía:

| `T` | Representación | Motivo |
|---|---|---|
| `Option<U>` (anidado) | Caja | El `none` interior sería el mismo valor que el `none` exterior |
| `()` | Caja | El valor de `()` es el propio valor de la lista vacía |
| `ptr` / `c-long` / `c-ulong` | Caja | Los 64 bits son valor, sin margen para distinguirlos |
| Cualquier otro | Sin caja | — |

La representación la decide solo el tipo y no se puede leer a partir de un valor. Al imprimir,
`(some ...)`/`none` se reconstruye a partir del tipo estático, así que `(format false "~a" opt)` imprime
`(some 1)`. Hay dos restricciones:

- **No se puede meter en un `:dyn Trait`** (pasar un valor de `Option<int>` para el que escribiste
  `(impl Speak Option<int> ...)` a un `:dyn Speak` es un error).
- Una conversión descendente `(the Option<T> ...)` desde un `Sexpr` **nombra un constructor**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. La forma que liga el valor entero,
  `(the Option<int> o)`, es un error.

## 3. Tipos de error y el trait `Error`

Siguiendo el `std::error::Error` de Rust, **`Error` no es un tipo sino un trait**. Los tipos concretos que
representan errores son distintos para cada propósito, y cada uno implementa `Error`.

| Tipo | Lo producen |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operaciones con archivos y streams ([Streams y archivos](streams-files.md)) |
| `NetError` | Operaciones de red ([Red](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. El `simple-error` de CL: la elección por defecto cuando solo quieres decir qué ocurrió |
| `WrappedError` | `(wrap-error msg cause)`. Un tipo que lleva a la vez tu propio mensaje y la causa; es la razón por la que el trait `Error` tiene `source` |

Desde `ParseIntError` hasta `NetError` son cada uno "una enumeración con una sola variante que contiene una
cadena de mensaje", y el nombre del tipo y el de la variante son iguales (`(match e ((ParseIntError m) m))`,
construido con `(ParseIntError::ParseIntError "...")`). No tienen nada de especial: se tratan exactamente
como tus propios tipos de error escritos con `(defstruct my-err (...))` / `(defenum my-err ...)`.

| Nombre | Forma | Tipo | Descripción |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | El mensaje de error (un método del trait `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | La causa que envuelve este error, o `None` si no la hay (el `Error::source` de Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implementa `Error`) | Amplía un tipo de error concreto al objeto trait |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implementa `Error`) | El mensaje y la cadena de causas encontradas siguiendo `source`, una causa por línea. CL no tiene equivalente (el "caused by" de Rust) |

Si implementas `Error` para tu propio tipo de error, se puede manejar **igual** que los errores incorporados:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; el tipo concreto va tal cual en E
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; manejar cualquier clase de forma uniforme
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Para reunir varios tipos de error en un único `Result`, usa `Result<T, :dyn Error>` (corresponde al
`Box<dyn Error>` de Rust), y amplía los errores concretos con `as-dyn-error`. Como no hay `?`, esta
conversión se escribe explícitamente:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Los tipos y los traits comparten un espacio de nombres** (como en Rust). Dentro de un módulo, un
`defstruct`/`defenum` y un trait no pueden tener el mismo nombre, y escribir un nombre de trait en una
posición de tipo se informa como "`error` is a trait, not a type — write `:dyn error`".

Los fallos irrecuperables se expresan con `panic`. Para `panic` y `catch`/`throw`, consulta la
[Referencia de sintaxis](../syntax.md#8-salidas-no-locales-catch--throw--unwind-protect); para la política
de manejo de errores, [el capítulo 9 del mismo documento](../syntax.md#9-política-de-manejo-de-errores).
