<!-- translated-from: docs/ja/reference/functions/README.md @ 2d61c22ad717f8ad6dfd178f397927e87e1d8902 -->
# Funciones incorporadas

La lista de funciones incorporadas, métodos y la biblioteca estándar. Para la sintaxis (formas especiales y
cómo definir cosas), consulta la [Referencia de sintaxis](../syntax.md); para la lista de tipos,
[Tipos](../types.md).

## Formas de llamada

Hay tres formas de llamada.

- Funciones libres: `(name args...)`
- Métodos de instancia: `(name receiver args...)` (se resuelven a partir del tipo estático del primer
  argumento)
- Métodos estáticos (funciones asociadas): `(Type::name args...)`

Cada tipo puede tener su propio método con el mismo nombre. `(+ a b)` llama al `+` del tipo de `a`.

## Nombres que necesitan un `use`

De los nombres de la biblioteca estándar, estos se pueden usar desde cualquier módulo sin escribir
nada ([referencia de sintaxis 3.10](../syntax.md#310-module--use--espacios-de-nombres)):

- los tipos primitivos, `Option`/`Result`/`Vector`, la iteración, las expresiones S y las listas, las
  funciones de secuencias, las macros de control, la comparación y los traits aritméticos
- todo nombre que también lo es del paquete `COMMON-LISP` de Common Lisp (`length`, `format`, `open`,
  etc.)
- `Error`, `assert`, `print-object`, `HashTable`, `Array`, `BitVector`, los traits de flujo `Stream`,
  `InputStream` y `OutputStream`, y `Hash`, que implementan las claves de una `HashTable`
- los tipos que mencionan sus funciones y variables

Cualquier otro nombre se trae con un `use`, como `(use std::net::tcp-connect)`, o se escribe con su
ruta, `std::net::tcp-connect`. La tabla da esos nombres y el módulo de cada uno.

| Módulo | Nombres |
|---|---|
| `std::array` | `array-iter` |
| `std::char` | `digit->char`, `digit-weight`, `name->char` |
| `std::collections` | `Deque`, `HashSet`, `Scope`, `SortedTable`, `hashtable-iter` |
| `std::env` | `command-line-args`, `getenv`, `home-directory` |
| `std::error` | `WrappedError`, `as-dyn-error`, `assert-eq`, `describe-error`, `wrap-error` |
| `std::fmt` | `pprint-list-exhausted` |
| `std::fs` | `direction-append`, `direction-input`, `direction-output`, `directory-p`, `open-binary`, `open-binary-input`, `open-binary-output`, `open-file`, `open-input`, `open-output`, `read-file-lines`, `read-file-string`, `write-file-string` |
| `std::hash` | `*sxhash-mask*`, `sxhash-string` |
| `std::io` | `ByteInput`, `ByteOutput`, `PeekInput`, `binary-file-stream`, `copy-stream`, `make-peek-stream`, `peek-stream`, `read-lines`, `write-lines` |
| `std::net` | `NetError`, `byte-stream-of`, `char-stream-of`, `datagram`, `socket-byte-stream`, `socket-listener`, `socket-stream`, `tcp-connect`, `tcp-listen`, `tls-connect`, `tls-listen`, `udp-bind`, `udp-socket`, `unix-connect`, `unix-listen`, `with-connection` |
| `std::num` | `ParseFloatError`, `ParseIntError`, `atan2`, `parse-float`, `parse-int` |
| `std::path` | `Pathish`, `pathname-absolute-p` |
| `std::process` | `exit` |
| `std::random` | `seed-random-state` |
| `std::reader` | `ReadOutcome`, `read-from-string-preserving-whitespace`, `read-sexpr`, `read-sexpr-preserving-whitespace` |
| `std::repl` | `heap-info` |
| `std::string` | `left-trim`, `right-trim`, `string->utf8`, `trim`, `utf8->string` |
| `std::symbol` | `symbol->string` |
| `std::sync` | `Chan`, `Context`, `Mutex`, `WaitGroup`, `after`, `with-lock` |
| `std::thread` | `Task`, `Thread`, `yield` |
| `std::time` | `internal-time-seconds`, `timezone-daylight-p`, `timezone-offset-seconds` |

## Cómo leer las tablas

Las tablas de cada capítulo tienen las columnas "nombre, forma, tipo, descripción". La columna de tipo se
escribe como `(tipo-de-argumento,...)→tipo-de-retorno`.

- Una sola letra mayúscula como `T`, `A` o `B` es una variable de tipo.
- Una nota como `where Eq A` es una restricción de trait que debe cumplir la variable de tipo.
- `Iter<A>` significa "cualquier implementación de `Iter` cuyo `Item` sea `A`".
- Los argumentos marcados con `&optional` / `&key` se pueden omitir.

## Capítulos

| Archivo | Contenido |
|---|---|
| [numbers.md](numbers.md) | Enteros, números de coma flotante, racionales, números complejos, booleanos, operaciones de bits, números aleatorios |
| [sequences.md](sequences.md) | El par `cons-cell`, los datos de expresiones S `Sexpr`, símbolos, funciones de secuencia, iteradores perezosos `lazy`, funciones de orden superior |
| [collections.md](collections.md) | Cadenas, caracteres, `Vector`, `HashTable`, `Array`, `BitVector`, `HashSet`, `SortedTable`, `Deque` |
| [option-result.md](option-result.md) | `Option`, `Result`, tipos de error y el trait `Error` |
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, traits aritméticos, `Push` |
| [printing.md](printing.md) | `print`/`println`/`format`, el pretty printer, `print-object`, variables de control de la impresora |
| [format.md](format.md) | Directivas de formato |
| [streams-files.md](streams-files.md) | Streams, operaciones con archivos, nombres de ruta, readtable |
| [concurrency.md](concurrency.md) | Tareas, canales, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, sockets de dominio Unix, UDP |
| [system.md](system.md) | Tiempo, el entorno de ejecución, herramientas de la implementación, `read`/`eval`, docstrings, funciones relacionadas con macros |
