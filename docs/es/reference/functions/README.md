<!-- translated-from: docs/ja/reference/functions/README.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
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
| [traits.md](traits.md) | `Iter`, `Eq`/`Ord`, traits aritméticos |
| [printing.md](printing.md) | `print`/`println`/`format`, el pretty printer, `print-object`, variables de control de la impresora |
| [format.md](format.md) | Directivas de formato |
| [streams-files.md](streams-files.md) | Streams, operaciones con archivos, nombres de ruta, readtable |
| [concurrency.md](concurrency.md) | Tareas, canales, `WaitGroup`, `Mutex`, `Thread`, `Context` |
| [network.md](network.md) | TCP, TLS, sockets de dominio Unix, UDP |
| [system.md](system.md) | Tiempo, el entorno de ejecución, herramientas de la implementación, `read`/`eval`, docstrings, funciones relacionadas con macros |
