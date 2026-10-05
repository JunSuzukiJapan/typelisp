<!-- translated-from: docs/ja/guide/ffi.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# FFI de C (defffi)

Esta guía explica cómo llamar a funciones de C desde typelisp. La lista de tipos que se pueden declarar y
las restricciones están en [Referencia de sintaxis 3.3](../reference/syntax.md#33-defffi--declarar-funciones-de-c-ffi).

## 1. Declarar y llamar a una función

`defffi` declara el nombre y los tipos de una función de C.

```lisp
(defffi (c-getpid "getpid") () i32)            ; el nombre en typelisp y el nombre del símbolo en C
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; se busca en libm
```

Las llamadas se envuelven en `(unsafe ...)`.

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` es necesario porque el compilador no puede comprobar que los tipos declarados coinciden con los
tipos reales del lado de C. Escribir `unsafe` significa que tú, quien escribe, asumes la responsabilidad de
esa comprobación. Olvidarlo da un error que lo explica.

## 2. Escribir un envoltorio seguro

El uso previsto es confinar `unsafe` en un solo lugar y presentar una función normal hacia fuera.

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; quien llama no necesita unsafe
(str-len "hello")  ; => 5
```

## 3. Cómo se corresponden los tipos

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | Enteros del mismo ancho |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool` (`_Bool`) |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long` (también `size_t`, `int64_t`, etc.) |
| `ptr` | Cualquier puntero (`void *`, `FILE *`, etc.) |
| `(ptr T)` | Un puntero a `T` ([sección 7](#7-structs-de-c)) |

### Cadenas

- Un `string` que pasas se copia en una cadena de C terminada en NUL, que se libera cuando la llamada
  retorna. Un NUL en medio de la cadena es un error.
- El resultado de una función que devuelve `string` también se copia. La memoria del lado de C no se libera.
  Para funciones que devuelven una cadena que quien llama debe liberar (como `strdup`), recibe el resultado
  como `ptr` y haz tú el `free`.
- Si una función declarada para devolver `string` devuelve NULL, es un error. Recibe como `ptr` el
  resultado de las funciones que pueden devolver NULL (como `getenv`).

### `c-long` / `c-ulong` / `ptr`

Estos tipos existen solo para pasar valores a través de la frontera con C, y **no admiten aritmética**.
Para usar uno como entero de typelisp, conviértelo con `as`.

```lisp
(as int (unsafe (c-strlen s)))      ; int no pierde nada del valor de 64 bits
(try-as i32 (unsafe (c-strlen s)))  ; none si no cabe en un i32
(unsafe (c-malloc 16))              ; los literales enteros se pueden pasar tal cual
```

Un `ptr` es un valor para devolverlo a funciones de C. No hay forma de leer desde typelisp aquello a lo que
apunta.

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

Estos tipos solo pueden aparecer como argumentos de función, valores de retorno y variables locales. No
pueden ser campos de estructuras, variables globales ni argumentos de tipo de `Vector` y similares.

## 4. Indicar una biblioteca

Sin `:library`, el símbolo se busca en lo que ya está enlazado en el proceso (libc, etc.). Las funciones de
otras bibliotecas necesitan `:library`.

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- Un nombre corto como `"sqlite3"` se busca como `libsqlite3.dylib` y después como `libsqlite3.so`.
- Un nombre que contiene `/` se trata como una ruta.
- Si no se encuentra el símbolo declarado, el error lo nombra.

## 5. Compilación AOT

Los programas que usan `defffi` se pueden convertir en ejecutables con
[`compile-file`](compile.md#3-construir-un-ejecutable-con-compilación-aot) tal cual. Las bibliotecas
indicadas con `:library` se añaden automáticamente al enlazar, así que `compile-file` no necesita
argumentos extra.

## 6. Callbacks

Puedes pasar una función de typelisp a una función de C y hacer que esta la llame de vuelta. Escribe un tipo
de función entre los tipos de los argumentos de `defffi` y, en la llamada, pon en esa posición un nombre de
función o una expresión `lambda`.

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") devuelve p
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- Solo se pueden pasar funciones **sin variables libres**. Sirven las funciones de nivel superior, las
  `lambda` y las funciones locales de `labels`, pero referirse a una variable local de un ámbito envolvente
  es un error al comprobar tipos. C solo pasa los argumentos declarados, así que no hay forma de entregar
  variables capturadas. Para guardar estado, usa variables globales.
- No se puede pasar una variable que contiene una función. Escribe en su lugar un nombre de función o una
  expresión `lambda`.
- Un `panic` o `throw` dentro del callback llega a quien llama después de que retorna la función de C.
- El callback solo se puede llamar mientras se ejecuta la función de C a la que llamó typelisp. No se puede
  usar desde cosas como `atexit` o manejadores de señales.

## 7. Structs de C

Para pasar algo como un arreglo de structs a una función de C, declara un struct con la misma disposición
que en C usando `def-c-struct`, y resérvalo dentro de `unsafe`.

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; declarado dentro de un unsafe de nivel superior

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; cuatro elementos, todos a cero
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` reserva `n` valores de `T` y devuelve un `(ptr T)`. `(c-ref p i)` es un puntero al
  `i`-ésimo, `p::field` es un campo y `(c-deref p)` es aquello a lo que apunta un puntero a un escalar como
  `i32`. Todos se pueden escribir con `setf`.
- `(as ptr p)` lo convierte en un `ptr` sin tipo para pasarlo a funciones de C que reciben un `void *`.
- El tamaño de `item` (8 aquí) y la posición de cada campo se determinan con las mismas reglas que en C.

### Vida de la memoria reservada

La memoria reservada se libera cuando el control sale del `unsafe` más externo de esa función. Lo mismo
ocurre cuando se sale por `panic` o `throw`. Por eso, un valor `(ptr T)` no se puede sacar fuera del
`unsafe`. Hacerlo el valor del `unsafe`, capturarlo en un cierre, pasarlo a un `task` y lanzarlo con
`throw` son todos errores de tipos. Copia los valores que quieras usar fuera en números o en un
`defstruct` dentro del `unsafe`.

Al reservar dentro de una `lambda` o de una función de `labels`, escribe un `unsafe` dentro de esa función.

### Memoria reservada por C

Un puntero recibido de C como `(ptr T)` (un valor de retorno de `defffi`, un argumento de callback, etc.)
es un error a menos que apunte dentro de memoria reservada con `c-alloc`. Declara con el `ptr` sin tipo las
funciones que reciben memoria que C reservó con `malloc`, o NULL.

## 8. Lo que no se puede hacer

- No se pueden declarar **funciones variádicas** (`printf` y similares). La parte variádica se pasa con
  reglas distintas de las de los argumentos fijos. Declara un nombre distinto para cada número de argumentos
  que uses.
- No es posible **pasar ni devolver structs por valor**. Usa funciones que pasen punteros.
- No es posible hacer **declaraciones genéricas**.
- No se puede usar **el mismo nombre que una función incorporada**.
- **No se pueden pasar como valores de función.** No puedes pasar una como en `(map xs c-abs)`; envuélvela en
  una `lambda`.

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```
