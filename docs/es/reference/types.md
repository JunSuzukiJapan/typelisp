<!-- translated-from: docs/ja/reference/types.md @ 1a01065673fd9d568c138bb444c88c5345552937 -->
# Tipos

Los tipos que tiene typelisp y los traits estándar que implementa cada uno. Cómo escribir tipos está en el
[capítulo 2 de la Referencia de sintaxis](syntax.md#2-escritura-de-tipos); las funciones y métodos de cada
tipo, en [Funciones incorporadas](functions/README.md).

## 1. Tipos primitivos

| Tipo | Contenido | Detalles |
|---|---|---|
| `int` | Un entero de precisión arbitraria. Se guarda como valor inmediato mientras cabe en 63 bits y se convierte automáticamente en bignum más allá. El tipo por defecto de los literales enteros sin anotar | [Números, capítulo 3](functions/numbers.md#3-enteros-de-precisión-arbitraria-int) |
| `i8` `i16` `i32` | Enteros de ancho fijo con signo | [Números, capítulo 1](functions/numbers.md#1-enteros-de-ancho-fijo) |
| `u8` `u16` `u32` | Enteros de ancho fijo sin signo | Igual que arriba |
| `f32` `f64` | Números de coma flotante IEEE-754. Los literales decimales son `f64` por defecto | [Números, capítulo 4](functions/numbers.md#4-números-de-coma-flotante-f64--f32) |
| `ratio` | Un número racional en su forma irreducible | [Números, capítulo 5](functions/numbers.md#5-racionales-ratio) |
| `bool` | `true` / `false` | [Números, capítulo 7](functions/numbers.md#7-booleanos) |
| `char` | Un valor escalar Unicode | [Caracteres](functions/collections.md#2-caracteres-char) |
| `string` | Una cadena inmutable | [Cadenas](functions/collections.md#1-cadenas-string) |
| `symbol` | Un símbolo. Las palabras clave (`:name`) también tienen este tipo | [Símbolos](functions/sequences.md#3-símbolos) |
| `()` | El tipo Unit. Su valor también es `()` | |
| `!` | El tipo Never. El tipo de las expresiones que no retornan, como `panic`. Se puede colocar donde se espere cualquier tipo | |
| `ptr` `c-long` `c-ulong` | Palabras usadas solo para pasar valores a C y desde C. Solo pueden ser valores dentro de `unsafe`, y los lugares donde pueden aparecer están limitados | [Números, capítulo 2](functions/numbers.md#2-palabras-en-bruto-en-la-frontera-con-c-ptr--c-long--c-ulong) |
| `random-state` | El estado de un generador de números aleatorios | [Números, capítulo 12](functions/numbers.md#12-números-aleatorios) |

No hay tipo entero de 64 bits. Para enteros cuyo ancho no importa, usa `int`.

## 2. Tipos genéricos incorporados

| Tipo | Contenido | Detalles |
|---|---|---|
| `Option<T>` | Un valor que está o no está. `some` / `none` | [Option y Result](functions/option-result.md) |
| `Result<T,E>` | Éxito o fallo. `ok` / `err` | Igual que arriba |
| `Vector<T>` | Un arreglo que puede crecer | [Vector](functions/collections.md#3-vectort) |
| `HashTable<K,V>` | Una tabla hash. El tipo de la clave debe implementar `Hash` | [HashTable](functions/collections.md#4-hashtablekv) |
| `#{T0 T1 ...}` | Tupla (de 1 a 12 elementos). Los elementos se leen con `t::0` | [Sintaxis, capítulo 2](syntax.md#2-escritura-de-tipos) |
| `Task<T>` | Un manejador de una tarea | [Tareas](functions/concurrency.md#1-taskt--manejadores-de-tareas) |
| `Thread<T>` | Un manejador de una tarea que se ejecuta en un hilo de SO dedicado | [Thread](functions/concurrency.md#7-threadt--hilos-de-so-dedicados) |
| `Chan<T>` | Un canal | [Canales](functions/concurrency.md#2-chant--canales) |

Los tipos de función se escriben `(fn (tipos-de-argumentos...) tipo-de-retorno)`, y los objetos trait
`:dyn Trait` ([capítulo 2 de la Referencia de sintaxis](syntax.md#2-escritura-de-tipos)).

## 3. Datos de expresiones S

| Tipo | Contenido | Detalles |
|---|---|---|
| `Sexpr` | Una expresión S no vacía. 19 variantes: `int`, `i8` a `u32`, `f32`, `f64`, `char`, `bool`, `sym`, `str`, `cons`, `ratio`, `path`, `vector`, `array`, `tuple` | [Datos de expresiones S](functions/sequences.md#2-datos-de-expresiones-s-sexpr) |
| `Option<Sexpr>` | Los datos de expresiones S en general. La lista vacía `()` es `none` | Igual que arriba |

## 4. Tipos de la biblioteca estándar

Tipos que la biblioteca estándar (el prelude) define con `defstruct` / `defenum`. Se tratan igual que los
tipos que escribes tú, y con ellos se puede hacer todo lo que se puede hacer con un `defstruct`.

| Tipo | Contenido | Detalles |
|---|---|---|
| `cons-cell<A,B>` | Un par. `cons`/`car`/`cdr` | [Pares](functions/sequences.md#1-pares-cons-cellab) |
| `complex` | Un número complejo (componentes `f64`) | [Números, capítulo 6](functions/numbers.md#6-números-complejos-complex) |
| `Array<T>` | Un arreglo multidimensional | [Array](functions/collections.md#5-arrayt-arreglos-multidimensionales) |
| `BitVector` | Una secuencia de bits de longitud fija | [BitVector](functions/collections.md#6-bitvector-vectores-de-bits) |
| `HashSet<T>` | Una colección de elementos sin duplicados | [HashSet](functions/collections.md#7-hashsett) |
| `SortedTable<K,V>` | Una tabla ordenada por clave | [SortedTable](functions/collections.md#8-sortedtablekv) |
| `Deque<T>` | Una secuencia en la que se mete y se saca por ambos extremos | [Deque](functions/collections.md#9-dequet) |
| `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` | Los iteradores que devuelve `iter` de cada colección | [Iter](functions/traits.md#1-el-trait-iter-y-la-iteración) |
| `lazy::map-iter<I,A,U>` etc. | Los iteradores que devuelven las funciones del módulo `lazy` | [Iteradores perezosos](functions/sequences.md#iteradores-perezosos-el-módulo-lazy) |
| `WaitGroup` | Esperar a que terminen N cosas | [WaitGroup](functions/concurrency.md#4-waitgroup--esperar-n-finalizaciones) |
| `Mutex<T>` | Exclusión mutua para datos compartidos | [Mutex](functions/concurrency.md#6-mutext--exclusión-mutua-para-datos-compartidos) |
| `Context` | Cancelación cooperativa | [Context](functions/concurrency.md#8-context--cancelación-cooperativa) |
| `pathname` | Un nombre de archivo dividido en partes | [Nombres de ruta](functions/streams-files.md#9-nombres-de-ruta-pathname) |
| `file-stream` `binary-file-stream` `string-input-stream` `string-output-stream` `standard-stream` | Streams | [Streams](functions/streams-files.md#3-tipos-de-stream-concretos) |
| `broadcast-stream` `two-way-stream` `echo-stream` `concatenated-stream` `peek-stream` | Streams compuestos | [Streams compuestos](functions/streams-files.md#4-streams-compuestos) |
| `socket-stream` `socket-byte-stream` `socket-listener` `udp-socket` `datagram` | Red | [Red](functions/network.md#1-tipos) |
| `ReadOutcome` | El resultado de `read-sexpr`. `datum` / `eof` | [Streams](functions/streams-files.md#6-funciones-genéricas-y-operaciones-con-archivos) |
| `universal-time` `internal-time` `decoded-time` | Tiempo | [Tiempo](functions/system.md#1-tiempo) |
| `heap-info` | El estado actual del montículo | [Herramientas de la implementación](functions/system.md#51-campos-de-heap-info) |

## 5. Tipos de error

`Error` no es un tipo sino un trait, y lo implementan los siguientes tipos. Para manejar errores de
cualquier clase, escribe `:dyn Error`.

| Tipo | Lo producen |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Operaciones con archivos y streams |
| `NetError` | Operaciones de red |
| `SimpleError` | `simple-error` |
| `WrappedError` | `wrap-error` |

Los detalles están en [Tipos de error y el trait Error](functions/option-result.md#3-tipos-de-error-y-el-trait-error).

## 6. Implementaciones de traits estándar

Qué tipos implementan qué traits. Los métodos de cada trait están en [Traits estándar](functions/traits.md)
y en los capítulos indicados en la columna de la derecha.

### 6.1 Comparación, hash e impresión

| Trait | Tipos que lo implementan | Detalles |
|---|---|---|
| `Eq` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `Sexpr` `cons-cell<A,B>` `#{..}` | [Eq / Ord](functions/traits.md#2-eq--ord-comparación) |
| `Ord` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `char` `string` `cons-cell<A,B>` `#{..}` | Igual que arriba |
| `Hash` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `bool` `char` `string` `symbol` `#{..}` | [HashTable](functions/collections.md#4-hashtablekv) |
| `print-object` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` `complex` `bool` `char` `string` `symbol` `#{..}` `Array<T>` `HashSet<T>` `SortedTable<K,V>` `Deque<T>` `Context` `pathname` `universal-time` `internal-time` y todos los tipos de error incorporados | [print-object](functions/printing.md#5-print-object-representación-impresa-por-tipo) |

Los traits de `cons-cell<A,B>` y de las tuplas `#{..}`, y el `print-object` de las colecciones, se
pueden usar cuando los tipos de los elementos implementan ese trait.

### 6.2 Aritmética

| Trait | Tipos que lo implementan |
|---|---|
| `Add` `Sub` `Mul` `Div` `Rem` `Number` | `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `ratio` |
| `Bits` | `int` `i8` `i16` `i32` `u8` `u16` `u32` |

Los detalles están en [Traits aritméticos](functions/traits.md#3-traits-aritméticos-add--sub--mul--div--rem--bits--number).

### 6.3 Iteración

| Trait | Tipos que lo implementan |
|---|---|
| `Iter` | `vector-iter<T>` `hashtable-iter<K,V>` `array-iter<T>` `Chan<T>` los tipos del módulo `lazy` (`lazy::map-iter<I,A,U>`, etc.) |

### 6.4 Streams

| Tipo | Traits implementados |
|---|---|
| `file-stream` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `ByteInput` `ByteOutput` |
| `string-input-stream` | `CharInput` `PeekInput` |
| `string-output-stream` | `CharOutput` |
| `standard-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` |
| `broadcast-stream` | `CharOutput` |
| `two-way-stream` | `CharInput` `CharOutput` |
| `echo-stream` | `CharInput` |
| `concatenated-stream` | `CharInput` |
| `peek-stream` | `CharInput` `PeekInput` |

Todos los streams implementan `Stream`; los de entrada también implementan `InputStream`, y los de salida
`OutputStream`. `socket-listener` y `udp-socket` solo implementan `Stream` (`close` / `open-stream-p`). Los
detalles están en [Streams](functions/streams-files.md#1-la-jerarquía-de-traits).

### 6.5 Otros

| Trait | Tipos que lo implementan | Detalles |
|---|---|---|
| `Error` | Todos los tipos de error del capítulo 5 | [Tipos de error](functions/option-result.md#3-tipos-de-error-y-el-trait-error) |
| `Pathish` | `string` `pathname` | [Nombres de ruta](functions/streams-files.md#91-el-trait-designador-de-rutas-pathish) |
